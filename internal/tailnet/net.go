package tailnet

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"log"
	"net"
	"net/http"
	"net/netip"
	"net/url"
	"os"
	"path/filepath"
	"slices"
	"strconv"
	"strings"
	"time"

	"tailscale.com/client/local"
	"tailscale.com/client/tailscale/apitype"
	"tailscale.com/ipn/ipnstate"
	"tailscale.com/tailcfg"
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

// Transport bounds. A peer that vanishes (killed process, dead phone) never
// resets a tunneled connection, so every phase needs its own deadline.
const (
	responseHeaderTimeout = 30 * time.Second
	idleConnTimeout       = 20 * time.Second
)

var dialAttemptTimeout = 30 * time.Second

// HTTPClient returns a client that dials over the tailnet by MagicDNS name,
// retrying lookups that fail right after a join.
func (n *Net) HTTPClient() *http.Client {
	return &http.Client{Transport: newTransport(n.srv.Dial, func() time.Duration { return n.LookupRetry })}
}

func newTransport(dial dialFunc, lookupRetry func() time.Duration) *http.Transport {
	attempt := func(ctx context.Context, network, addr string) (net.Conn, error) {
		ctx, cancel := context.WithTimeout(ctx, dialAttemptTimeout)
		defer cancel()
		return dial(ctx, network, addr)
	}
	return &http.Transport{
		DialContext: func(ctx context.Context, network, addr string) (net.Conn, error) {
			return retryDial(ctx, attempt, lookupRetry(), network, addr)
		},
		MaxIdleConnsPerHost:   4,
		IdleConnTimeout:       idleConnTimeout,
		ResponseHeaderTimeout: responseHeaderTimeout,
	}
}

const (
	hubPingTimeout  = 10 * time.Second
	hubPingRetry    = 2 * time.Second
	hubPingInterval = 30 * time.Second
)

// KeepHubReachable pings the hub over the tailnet until ctx ends: quickly
// while it fails, every hubPingInterval once it works.
//
// A restarted member gets a new disco key. A peer that learned the old key
// over TSMP keeps sending discovery pings with it, and the restarted member
// cannot decrypt them, so the hub never finds a path back. A member that
// speaks first with its new key makes the peer switch at once. A node agent
// only ever answers, so it must do this itself.
func (n *Net) KeepHubReachable(ctx context.Context, logf func(string, ...any)) {
	wait, failing := time.Duration(0), false
	for {
		select {
		case <-ctx.Done():
			return
		case <-time.After(wait):
		}
		err := n.pingHub(ctx)
		switch {
		case err == nil:
			wait, failing = hubPingInterval, false
		default:
			if !failing && ctx.Err() == nil {
				logf("hub not reachable yet: %v", err)
			}
			wait, failing = hubPingRetry, true
		}
	}
}

func (n *Net) pingHub(ctx context.Context) error {
	st, err := n.lc.Status(ctx)
	if err != nil {
		return err
	}
	ip, ok := hubAddr(st)
	if !ok {
		return errors.New("hub not in the netmap")
	}
	ctx, cancel := context.WithTimeout(ctx, hubPingTimeout)
	defer cancel()
	res, err := n.lc.Ping(ctx, ip, tailcfg.PingDisco)
	if err != nil {
		return err
	}
	if res.Err != "" {
		return errors.New(res.Err)
	}
	return nil
}

func hubAddr(st *ipnstate.Status) (netip.Addr, bool) {
	for _, p := range st.Peer {
		if p.Tags != nil && p.Tags.ContainsFunc(func(t string) bool { return t == TagHub }) && len(p.TailscaleIPs) > 0 {
			return p.TailscaleIPs[0], true
		}
	}
	return netip.Addr{}, false
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

// identify accepts only callers whose node carries one of the allowed tags
// and returns the caller's node name.
func identify(who whoIsFunc, allowed ...string) func(*http.Request) (string, error) {
	return func(r *http.Request) (string, error) {
		res, err := who(r.Context(), r.RemoteAddr)
		if err != nil {
			return "", fmt.Errorf("%w: identify %s: %v", ErrForbidden, r.RemoteAddr, err)
		}
		if res == nil || res.Node == nil {
			return "", fmt.Errorf("%w: %s is not a tagged node", ErrForbidden, r.RemoteAddr)
		}
		for _, t := range res.Node.Tags {
			if slices.Contains(allowed, t) {
				return strings.TrimSuffix(res.Node.Name, "."), nil
			}
		}
		return "", fmt.Errorf("%w: %s has tags %v", ErrForbidden, res.Node.Name, res.Node.Tags)
	}
}

// AuthorizeHub is the node-side authorizer: only the hub may call.
func (n *Net) AuthorizeHub() func(*http.Request) error {
	id := identify(n.lc.WhoIs, TagHub)
	return func(r *http.Request) error {
		_, err := id(r)
		return err
	}
}

// IdentifyAPI is the hub-side identity gate for the tailnet listener: the hub
// itself and tag:client devices, never tag:node. It returns the caller's node
// name. It only identifies the device; callers must still present a token.
func (n *Net) IdentifyAPI() func(*http.Request) (string, error) {
	return identify(n.lc.WhoIs, TagHub, TagClient)
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
		key, _, err = admin.CreateKey(ctx, []string{TagHub}, 10*time.Minute)
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
