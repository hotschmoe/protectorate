// Command member is Protectorate's tsnet glue: one userspace tailnet member that
// the Rust hub or node agent runs as a child. See go/README.md.
package main

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"io"
	"log/slog"
	"net/http"
	"net/url"
	"os"
	"os/signal"
	"path/filepath"
	"strings"
	"syscall"
	"time"

	"tailscale.com/client/local"
	"tailscale.com/envknob"
	"tailscale.com/feature/buildfeatures"
	"tailscale.com/ipn"
	"tailscale.com/net/netns"
	"tailscale.com/tailcfg"
	"tailscale.com/tsnet"
)

const (
	tagHub   = "tag:hub"
	tagNode  = "tag:node"
	nodePort = 7475
	pinFile  = "control-key.pin"
)

type config struct {
	mode, stateDir, controlURL, hostname, socket, forward string
	udpPort                                               uint
	printRegistration, verbose                            bool
}

var log = slog.New(slog.NewTextHandler(os.Stderr, &slog.HandlerOptions{
	ReplaceAttr: func(_ []string, a slog.Attr) slog.Attr {
		if a.Key == slog.TimeKey {
			return slog.Attr{}
		}
		return a
	},
}))

func main() {
	var c config
	flag.StringVar(&c.mode, "mode", "", "hub or node")
	flag.StringVar(&c.stateDir, "state-dir", "", "tsnet state directory (keys, control-key pin)")
	flag.StringVar(&c.controlURL, "control-url", "", "headscale URL, http(s)://host[:port]")
	flag.StringVar(&c.hostname, "hostname", "", "hostname presented to control")
	flag.StringVar(&c.socket, "socket", "", "unix socket for the control API (hub: DIAL and STATUS; node: STATUS)")
	flag.StringVar(&c.forward, "forward", "", "node: local agent for accepted :7475 connections, a unix socket path or 127.0.0.1:port")
	flag.UintVar(&c.udpPort, "udp-port", 0, "fixed WireGuard UDP port (0 picks one)")
	flag.BoolVar(&c.printRegistration, "print-registration", false, "print the registration as JSON on stdout, wait until it is approved, then exit")
	flag.BoolVar(&c.verbose, "verbose", false, "also log tsnet backend messages")
	flag.Parse()
	if err := run(c); err != nil {
		log.Error("exit", "err", err)
		os.Exit(1)
	}
}

func (c config) validate() error {
	if c.mode != "hub" && c.mode != "node" {
		return fmt.Errorf("--mode must be hub or node, got %q", c.mode)
	}
	if c.stateDir == "" || c.hostname == "" {
		return errors.New("--state-dir and --hostname are required")
	}
	u, err := url.Parse(c.controlURL)
	if err != nil || (u.Scheme != "http" && u.Scheme != "https") || u.Host == "" || (u.Path != "" && u.Path != "/") {
		return fmt.Errorf("--control-url %q must be http(s)://host[:port]", c.controlURL)
	}
	if c.mode == "hub" && buildfeatures.HasPeerAPIServer {
		return errors.New("hub mode needs a member built without the peerAPI server (scripts/net-build.sh)")
	}
	if c.udpPort > 65535 {
		return fmt.Errorf("--udp-port %d out of range", c.udpPort)
	}
	if c.printRegistration {
		return nil
	}
	if c.mode == "hub" && c.socket == "" {
		return errors.New("hub mode needs --socket")
	}
	if c.mode == "node" && c.forward == "" {
		return errors.New("node mode needs --forward")
	}
	return nil
}

func run(c config) error {
	if err := c.validate(); err != nil {
		return err
	}
	c.controlURL = strings.TrimSuffix(c.controlURL, "/")
	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()

	// No uploads to log.tailscale.com, even where logtail is compiled in.
	envknob.SetNoLogsNoSupport()
	if strings.HasPrefix(c.controlURL, "https://") {
		// Otherwise ts2021 is tried first as plain HTTP on port 80 of the
		// control host, which a TLS front such as Traefik redirects or drops.
		envknob.Setenv("TS_FORCE_NOISE_443", "true")
	}
	// Userspace only, so sockets need no bypass mark. Left on, a non-root
	// process binds every control and DERP dial to the default-route
	// interface and a VPN-only route is never used.
	netns.SetEnabled(false)

	if err := os.MkdirAll(c.stateDir, 0o700); err != nil {
		return fmt.Errorf("create state dir: %w", err)
	}
	fp, err := checkControlKey(ctx, c)
	if err != nil {
		return err
	}

	srv := &tsnet.Server{
		Dir:        c.stateDir,
		Hostname:   c.hostname,
		ControlURL: c.controlURL,
		Port:       uint16(c.udpPort),
		UserLogf: func(format string, args ...any) {
			log.Info("tsnet", "msg", fmt.Sprintf(format, args...))
		},
	}
	if c.verbose {
		srv.Logf = func(format string, args ...any) {
			log.Debug("tsnet", "msg", fmt.Sprintf(format, args...))
		}
	}
	defer srv.Close()
	if err := srv.Start(); err != nil {
		return fmt.Errorf("start tsnet: %w", err)
	}
	lc, err := srv.LocalClient()
	if err != nil {
		return fmt.Errorf("tsnet local client: %w", err)
	}
	m := &member{c: c, srv: srv, lc: lc, controlKey: fp}

	if c.printRegistration {
		return m.register(ctx)
	}

	errc := make(chan error, 3)
	go func() { errc <- m.watchLogin(ctx) }()
	if c.socket != "" {
		ln, err := listenUnix(c.socket)
		if err != nil {
			return err
		}
		defer ln.Close()
		go func() { errc <- m.serveControl(ctx, ln) }()
	}
	if c.mode == "node" {
		ln, err := srv.Listen("tcp", fmt.Sprintf(":%d", nodePort))
		if err != nil {
			return fmt.Errorf("listen on tailnet :%d: %w", nodePort, err)
		}
		defer ln.Close()
		go func() { errc <- m.serveNode(ctx, ln) }()
		go m.keepHubReachable(ctx)
	}
	log.Info("started", "mode", c.mode, "hostname", c.hostname, "control", c.controlURL, "control_key", fp)
	select {
	case <-ctx.Done():
		log.Info("stopping", "reason", context.Cause(ctx))
		// wireguard-go can block closing its UDP bind (seen with
		// TS_DEBUG_ALWAYS_USE_DERP); a stop must not hang the parent.
		time.AfterFunc(5*time.Second, func() {
			log.Error("exit", "err", "tsnet close timed out")
			os.Exit(1)
		})
		return nil
	case err := <-errc:
		return err
	}
}

type member struct {
	c          config
	srv        *tsnet.Server
	lc         *local.Client
	controlKey string
}

// checkControlKey enforces SPEC 12a item 8. The first start with an empty
// state dir is enrollment and pins the key; every later start must match it.
// tsnet fetches /key once more itself, so an active man in the middle can
// still answer the two fetches differently (REVISIT.md).
func checkControlKey(ctx context.Context, c config) (string, error) {
	fp, err := fetchControlKey(ctx, c.controlURL)
	if err != nil {
		return "", err
	}
	pinPath := filepath.Join(c.stateDir, pinFile)
	pinned, err := os.ReadFile(pinPath)
	switch {
	case err == nil:
		if got := strings.TrimSpace(string(pinned)); got != fp {
			return "", fmt.Errorf("control key changed: pinned %s, %s serves %s", got, c.controlURL, fp)
		}
		return fp, nil
	case !errors.Is(err, os.ErrNotExist):
		return "", fmt.Errorf("read control-key pin: %w", err)
	}
	if _, err := os.Stat(filepath.Join(c.stateDir, "tailscaled.state")); err == nil {
		return "", fmt.Errorf("control key pin %s is missing but tailnet state exists; refusing to re-pin", pinPath)
	}
	if err := os.WriteFile(pinPath, []byte(fp+"\n"), 0o600); err != nil {
		return "", fmt.Errorf("write control-key pin: %w", err)
	}
	log.Info("pinned control key", "control_key", fp)
	return fp, nil
}

// fetchControlKey returns "sha256:" plus the hex SHA-256 of the 32 raw bytes
// of control's Noise public key, as served at /key.
func fetchControlKey(ctx context.Context, controlURL string) (string, error) {
	ctx, cancel := context.WithTimeout(ctx, 15*time.Second)
	defer cancel()
	keyURL := fmt.Sprintf("%s/key?v=%d", controlURL, tailcfg.CurrentCapabilityVersion)
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, keyURL, nil)
	if err != nil {
		return "", fmt.Errorf("control key request: %w", err)
	}
	res, err := http.DefaultClient.Do(req)
	if err != nil {
		return "", fmt.Errorf("fetch control key: %w", err)
	}
	defer res.Body.Close()
	body, err := io.ReadAll(io.LimitReader(res.Body, 64<<10))
	if err != nil {
		return "", fmt.Errorf("read control key from %s: %w", keyURL, err)
	}
	if res.StatusCode != http.StatusOK {
		return "", fmt.Errorf("fetch control key from %s: %s", keyURL, res.Status)
	}
	var keys tailcfg.OverTLSPublicKeyResponse
	if err := json.Unmarshal(body, &keys); err != nil {
		return "", fmt.Errorf("parse control key from %s: %w", keyURL, err)
	}
	if keys.PublicKey.IsZero() {
		return "", fmt.Errorf("control at %s serves no Noise key", controlURL)
	}
	sum := sha256.Sum256(keys.PublicKey.UntypedBytes())
	return "sha256:" + hex.EncodeToString(sum[:]), nil
}

// register prints {"event":"registration",...} once control hands out an
// auth URL, then {"event":"registered",...} once an operator approved it.
func (m *member) register(ctx context.Context) error {
	out := json.NewEncoder(os.Stdout)
	printed := false
	for {
		st, err := m.lc.StatusWithoutPeers(ctx)
		if err != nil {
			return fmt.Errorf("tsnet status: %w", err)
		}
		switch {
		case st.BackendState == ipn.Running.String():
			var ips []string
			for _, ip := range st.TailscaleIPs {
				ips = append(ips, ip.String())
			}
			return out.Encode(map[string]any{"event": "registered", "hostname": m.c.hostname, "ips": ips, "control_key": m.controlKey})
		case st.AuthURL != "" && !printed:
			u, err := url.Parse(st.AuthURL)
			if err != nil {
				return fmt.Errorf("parse auth URL %q: %w", st.AuthURL, err)
			}
			if err := out.Encode(map[string]any{
				"event":       "registration",
				"hostname":    m.c.hostname,
				"mode":        m.c.mode,
				"auth_url":    st.AuthURL,
				"auth_id":     filepath.Base(u.Path),
				"control_key": m.controlKey,
			}); err != nil {
				return err
			}
			printed = true
		}
		select {
		case <-ctx.Done():
			return ctx.Err()
		case <-time.After(500 * time.Millisecond):
		}
	}
}

// watchLogin fails the process once control asks for a login: a member that
// was never registered, or was removed, exits instead of waiting on an auth URL.
func (m *member) watchLogin(ctx context.Context) error {
	for {
		st, err := m.lc.StatusWithoutPeers(ctx)
		if err == nil && st.BackendState == ipn.NeedsLogin.String() {
			return errors.New("not registered with control: run with --print-registration")
		}
		select {
		case <-ctx.Done():
			return nil
		case <-time.After(2 * time.Second):
		}
	}
}
