package tailnet

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"log"
	"net"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"slices"
	"strconv"
	"strings"
	"time"

	"tailscale.com/client/local"
	"tailscale.com/client/tailscale/apitype"
	"tailscale.com/tsnet"
)

// ErrForbidden is wrapped by every authorizer rejection. Callers that cannot
// identify the peer are rejected too (fail closed).
var ErrForbidden = errors.New("forbidden")

// Options configures one tsnet member.
type Options struct {
	Dir        string
	ControlURL string
	Hostname   string
	AuthKey    string
	Logf       func(format string, args ...any)
}

// Net is a running tsnet member (hub or node).
type Net struct {
	srv *tsnet.Server
	lc  *local.Client

	// LookupRetry bounds how long a dial retries "no such host" while the
	// netmap is still filling in after a join.
	LookupRetry time.Duration
}

const defaultLookupRetry = 15 * time.Second

// Up starts the member and waits until it is connected.
func Up(ctx context.Context, o Options) (*Net, error) {
	if err := os.MkdirAll(o.Dir, 0o700); err != nil {
		return nil, fmt.Errorf("create tsnet dir: %w", err)
	}
	logf := o.Logf
	if logf == nil {
		logf = func(string, ...any) {}
	}
	srv := &tsnet.Server{
		Dir: o.Dir, Hostname: o.Hostname, ControlURL: o.ControlURL, AuthKey: o.AuthKey,
		Logf: logf, UserLogf: log.Printf,
	}
	if _, err := srv.Up(ctx); err != nil {
		srv.Close()
		return nil, fmt.Errorf("join tailnet at %s: %w", o.ControlURL, err)
	}
	lc, err := srv.LocalClient()
	if err != nil {
		srv.Close()
		return nil, fmt.Errorf("tsnet local client: %w", err)
	}
	return &Net{srv: srv, lc: lc, LookupRetry: defaultLookupRetry}, nil
}

func (n *Net) Close() error { return n.srv.Close() }

// Listen returns a listener on the member's tailnet addresses.
func (n *Net) Listen(port int) (net.Listener, error) {
	l, err := n.srv.Listen("tcp", ":"+strconv.Itoa(port))
	if err != nil {
		return nil, fmt.Errorf("listen on tailnet port %d: %w", port, err)
	}
	return l, nil
}

// HTTPClient returns a client that dials over the tailnet by MagicDNS name,
// retrying lookups that fail right after a join.
func (n *Net) HTTPClient() *http.Client {
	return &http.Client{Transport: &http.Transport{
		DialContext: func(ctx context.Context, network, addr string) (net.Conn, error) {
			return retryDial(ctx, n.srv.Dial, n.LookupRetry, network, addr)
		},
		MaxIdleConnsPerHost: 4,
		IdleConnTimeout:     90 * time.Second,
	}}
}

type dialFunc func(ctx context.Context, network, addr string) (net.Conn, error)

func isNoSuchHost(err error) bool {
	var dns *net.DNSError
	if errors.As(err, &dns) && dns.IsNotFound {
		return true
	}
	return strings.Contains(err.Error(), "no such host")
}

// retryDial retries only name-lookup failures, with capped backoff, for at
// most window. Any other error (offline peer, ACL drop) returns at once.
func retryDial(ctx context.Context, dial dialFunc, window time.Duration, network, addr string) (net.Conn, error) {
	deadline := time.Now().Add(window)
	wait := 200 * time.Millisecond
	for {
		c, err := dial(ctx, network, addr)
		if err == nil || !isNoSuchHost(err) || !time.Now().Add(wait).Before(deadline) {
			return c, err
		}
		select {
		case <-ctx.Done():
			return nil, ctx.Err()
		case <-time.After(wait):
		}
		wait = min(wait*2, 2*time.Second)
	}
}

type whoIsFunc func(ctx context.Context, remoteAddr string) (*apitype.WhoIsResponse, error)

// authorizer accepts only callers whose node carries one of the allowed tags.
func authorizer(who whoIsFunc, allowed ...string) func(*http.Request) error {
	return func(r *http.Request) error {
		res, err := who(r.Context(), r.RemoteAddr)
		if err != nil {
			return fmt.Errorf("%w: identify %s: %v", ErrForbidden, r.RemoteAddr, err)
		}
		if res == nil || res.Node == nil {
			return fmt.Errorf("%w: %s is not a tagged node", ErrForbidden, r.RemoteAddr)
		}
		for _, t := range res.Node.Tags {
			if slices.Contains(allowed, t) {
				return nil
			}
		}
		return fmt.Errorf("%w: %s has tags %v", ErrForbidden, res.Node.Name, res.Node.Tags)
	}
}

// AuthorizeHub is the node-side authorizer: only the hub may call.
func (n *Net) AuthorizeHub() func(*http.Request) error {
	return authorizer(n.lc.WhoIs, TagHub)
}

// AuthorizeAPI is the hub-side authorizer for the tailnet listener: the hub
// itself and tag:client devices. tag:node is never accepted.
func (n *Net) AuthorizeAPI() func(*http.Request) error {
	return authorizer(n.lc.WhoIs, TagHub, TagClient)
}

type nodeState struct {
	ControlURL string `json:"control_url"`
	Hostname   string `json:"hostname"`
}

func nodeStatePath(dir string) string { return filepath.Join(dir, "node.json") }

// Join enrolls this machine with a preauth key, stores the tsnet state in dir
// and returns; OpenNode reuses that state without the key.
func Join(ctx context.Context, dir, control, key, name string) error {
	if err := validateControl(control); err != nil {
		return err
	}
	if !validLabel.MatchString(name) {
		return fmt.Errorf("node name %q must be a lowercase DNS label (a-z, 0-9, -)", name)
	}
	if key == "" {
		return errors.New("empty preauth key")
	}
	n, err := Up(ctx, Options{Dir: dir, ControlURL: control, Hostname: name, AuthKey: key})
	if err != nil {
		return err
	}
	closeErr := n.Close()
	b, err := json.Marshal(nodeState{ControlURL: control, Hostname: name})
	if err != nil {
		return err
	}
	return errors.Join(closeErr, writeFile(nodeStatePath(dir), b, 0o600))
}

// OpenNode reconnects a previously joined machine.
func OpenNode(ctx context.Context, dir string) (*Net, error) {
	b, err := os.ReadFile(nodeStatePath(dir))
	if errors.Is(err, os.ErrNotExist) {
		return nil, fmt.Errorf("not enrolled: run `protectorate node join` first (no %s)", nodeStatePath(dir))
	}
	if err != nil {
		return nil, err
	}
	var st nodeState
	if err := json.Unmarshal(b, &st); err != nil {
		return nil, fmt.Errorf("parse %s: %w", nodeStatePath(dir), err)
	}
	return Up(ctx, Options{Dir: dir, ControlURL: st.ControlURL, Hostname: st.Hostname})
}

func validateControl(raw string) error {
	u, err := url.Parse(raw)
	if err != nil || (u.Scheme != "http" && u.Scheme != "https") || u.Host == "" {
		return fmt.Errorf("control URL %q must be an http(s) URL", raw)
	}
	return nil
}

// UpHub starts the hub's own tsnet member as tag:hub. A fresh single-use key
// is requested only when no state exists yet.
func UpHub(ctx context.Context, home string, cfg Config, admin *Admin) (*Net, error) {
	dir := filepath.Join(Dir(home), "tsnet-hub")
	var key string
	if _, err := os.Stat(filepath.Join(dir, "tailscaled.state")); errors.Is(err, os.ErrNotExist) {
		key, err = admin.CreateKey(ctx, []string{TagHub}, 10*time.Minute)
		if err != nil {
			return nil, fmt.Errorf("hub preauth key: %w", err)
		}
	}
	return Up(ctx, Options{Dir: dir, ControlURL: cfg.ServerURL, Hostname: cfg.HubName, AuthKey: key})
}

// Joined reports whether dir holds an enrolled node's state.
func Joined(dir string) bool {
	_, err := os.Stat(nodeStatePath(dir))
	return err == nil
}
