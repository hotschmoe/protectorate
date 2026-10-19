package tailnet_test

import (
	"context"
	"fmt"
	"io"
	"net"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/hotschmoe/protectorate/internal/hub"
	"github.com/hotschmoe/protectorate/internal/tailnet"
)

func freeAddr(t *testing.T, network string) string {
	t.Helper()
	switch network {
	case "udp":
		c, err := net.ListenPacket("udp", "127.0.0.1:0")
		if err != nil {
			t.Fatal(err)
		}
		defer c.Close()
		return c.LocalAddr().String()
	default:
		l, err := net.Listen("tcp", "127.0.0.1:0")
		if err != nil {
			t.Fatal(err)
		}
		defer l.Close()
		return l.Addr().String()
	}
}

type member struct {
	name string
	net  *tailnet.Net
}

// enroll invites, joins, reopens from stored state and (for nodes) serves the
// node port behind the hub-only authorizer.
func enroll(ctx context.Context, t *testing.T, a *tailnet.Admin, control, name string, tags []string, serve bool) member {
	t.Helper()
	key, err := a.Invite(ctx, name, tags, 5*time.Minute)
	if err != nil {
		t.Fatalf("invite %s: %v", name, err)
	}
	dir := filepath.Join(t.TempDir(), "tsnet")
	if err := tailnet.Join(ctx, dir, control, key, name); err != nil {
		t.Fatalf("join %s: %v", name, err)
	}
	n, err := tailnet.OpenNode(ctx, dir)
	if err != nil {
		t.Fatalf("open %s: %v", name, err)
	}
	t.Cleanup(func() { n.Close() })
	if serve {
		ln, err := n.Listen(tailnet.NodePort)
		if err != nil {
			t.Fatal(err)
		}
		authz := n.AuthorizeHub()
		srv := &http.Server{Handler: http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			if err := authz(r); err != nil {
				http.Error(w, err.Error(), http.StatusForbidden)
				return
			}
			fmt.Fprintf(w, "hello from %s", name)
		})}
		go srv.Serve(ln)
		t.Cleanup(func() { srv.Close() })
	}
	return member{name, n}
}

func get(ctx context.Context, c *http.Client, url string, timeout time.Duration) (int, string, error) {
	ctx, cancel := context.WithTimeout(ctx, timeout)
	defer cancel()
	req, _ := http.NewRequestWithContext(ctx, http.MethodGet, url, nil)
	res, err := c.Do(req)
	if err != nil {
		return 0, "", err
	}
	defer res.Body.Close()
	b, _ := io.ReadAll(res.Body)
	return res.StatusCode, strings.TrimSpace(string(b)), nil
}

func TestTailnetEndToEnd(t *testing.T) {
	if os.Getenv("PROT_TAILNET_IT") != "1" {
		t.Skip("set PROT_TAILNET_IT=1 to run (downloads headscale, starts real processes)")
	}
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Minute)
	defer cancel()

	home := t.TempDir()
	if _, err := tailnet.Install(ctx, home, tailnet.HeadscaleVersion, nil); err != nil {
		t.Fatal(err)
	}
	listen := freeAddr(t, "tcp")
	cfg := tailnet.DefaultConfig("http://" + listen)
	cfg.Listen = listen
	cfg.GRPCListen = freeAddr(t, "tcp")
	cfg.MetricsListen = freeAddr(t, "tcp")
	cfg.STUNListen = freeAddr(t, "udp")
	cfg.Pools = []string{"build"}
	if err := cfg.Save(home); err != nil {
		t.Fatal(err)
	}

	svc, err := tailnet.Start(ctx, home, cfg, t.Logf)
	if err != nil {
		t.Fatalf("start: %v", err)
	}
	defer svc.Close()

	hubLn, err := svc.Net.Listen(tailnet.HubAPIPort)
	if err != nil {
		t.Fatal(err)
	}
	local, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	hctx, hstop := context.WithCancel(ctx)
	hubDone := make(chan error, 1)
	go func() {
		hubDone <- hub.New("test", "tok").Serve(hctx, local, hub.Extra{Listener: hubLn, Authorize: svc.Net.AuthorizeAPI()})
	}()
	defer func() {
		hstop()
		<-hubDone
	}()

	control := cfg.ServerURL
	alpha := enroll(ctx, t, svc.Admin, control, "alpha", []string{tailnet.TagNode, tailnet.TierTag("server")}, true)
	beta := enroll(ctx, t, svc.Admin, control, "beta", []string{tailnet.TagNode, tailnet.PoolTag("build")}, true)
	gamma := enroll(ctx, t, svc.Admin, control, "gamma", []string{tailnet.TagNode, tailnet.PoolTag("build")}, true)
	phone := enroll(ctx, t, svc.Admin, control, "phone", []string{tailnet.TagClient}, false)
	_ = gamma

	hc := svc.Net.HTTPClient()
	nodeURL := func(n string) string { return fmt.Sprintf("http://%s:%d/", n, tailnet.NodePort) }

	t.Run("hub reaches nodes by short name", func(t *testing.T) {
		for _, n := range []string{"alpha", "beta"} {
			code, body, err := get(ctx, hc, nodeURL(n), 30*time.Second)
			if err != nil || code != 200 || body != "hello from "+n {
				t.Fatalf("%s: code=%d body=%q err=%v", n, code, body, err)
			}
		}
	})

	t.Run("node rejects tag:node callers", func(t *testing.T) {
		code, body, err := get(ctx, beta.net.HTTPClient(), nodeURL("gamma"), 30*time.Second)
		if err != nil || code != http.StatusForbidden {
			t.Fatalf("beta->gamma (same pool, WhoIs layer): code=%d body=%q err=%v", code, body, err)
		}
	})

	t.Run("acl isolates nodes outside a shared pool", func(t *testing.T) {
		if code, _, err := get(ctx, alpha.net.HTTPClient(), nodeURL("beta"), 5*time.Second); err == nil {
			t.Fatalf("alpha->beta reachable, code=%d", code)
		}
	})

	t.Run("hub api: client allowed, node denied", func(t *testing.T) {
		apiURL := fmt.Sprintf("http://%s:%d/protectorate.v1.HubService/GetStatus", "protectorate-hub", tailnet.HubAPIPort)
		post := func(c *http.Client) (int, error) {
			ctx, cancel := context.WithTimeout(ctx, 30*time.Second)
			defer cancel()
			req, _ := http.NewRequestWithContext(ctx, http.MethodPost, apiURL, strings.NewReader("{}"))
			req.Header.Set("Content-Type", "application/json")
			res, err := c.Do(req)
			if err != nil {
				return 0, err
			}
			res.Body.Close()
			return res.StatusCode, nil
		}
		if code, err := post(phone.net.HTTPClient()); err != nil || code != 200 {
			t.Fatalf("client: code=%d err=%v", code, err)
		}
		short := alpha.net.HTTPClient()
		short.Timeout = 5 * time.Second
		if code, err := post(short); err == nil {
			t.Fatalf("node reached hub api, code=%d", code)
		}
	})

	t.Run("key expiry is enforced for nodes, not the hub", func(t *testing.T) {
		if err := svc.Admin.EnforceExpiry(ctx); err != nil {
			t.Fatal(err)
		}
		nodes, err := svc.Admin.Nodes(ctx)
		if err != nil {
			t.Fatal(err)
		}
		for _, n := range nodes {
			hasExpiry := n.Expiry != ""
			if hasExpiry != (n.GivenName != "protectorate-hub") {
				t.Errorf("%s: expiry %q", n.GivenName, n.Expiry)
			}
		}
	})

	t.Run("revoked node drops off", func(t *testing.T) {
		if err := svc.Admin.Revoke(ctx, "alpha"); err != nil {
			t.Fatal(err)
		}
		deadline := time.Now().Add(30 * time.Second)
		for {
			_, _, err := get(ctx, hc, nodeURL("alpha"), 3*time.Second)
			if err != nil {
				break
			}
			if time.Now().After(deadline) {
				t.Fatal("alpha still reachable 30s after revoke")
			}
			time.Sleep(500 * time.Millisecond)
		}
		if code, _, err := get(ctx, hc, nodeURL("beta"), 30*time.Second); err != nil || code != 200 {
			t.Fatalf("beta after revoking alpha: code=%d err=%v", code, err)
		}
	})
}
