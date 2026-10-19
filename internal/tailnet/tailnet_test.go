package tailnet

import (
	"bytes"
	"context"
	"crypto/sha256"
	"crypto/x509"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net"
	"net/http"
	"net/http/httptest"
	"net/netip"
	"os"
	"path/filepath"
	"runtime"
	"slices"
	"strings"
	"syscall"
	"testing"
	"time"

	"tailscale.com/client/tailscale/apitype"
	"tailscale.com/ipn/ipnstate"
	"tailscale.com/tailcfg"
	"tailscale.com/types/key"
	"tailscale.com/types/views"
)

func TestConfigValidate(t *testing.T) {
	ok := DefaultConfig("http://10.0.0.1:7480")
	tests := []struct {
		name   string
		mutate func(*Config)
		want   string
	}{
		{"default", func(*Config) {}, ""},
		{"bad scheme", func(c *Config) { c.ServerURL = "ftp://x" }, "server_url"},
		{"no host", func(c *Config) { c.ServerURL = "http://" }, "server_url"},
		{"bad listen", func(c *Config) { c.Listen = "nope" }, "listen"},
		{"no derp", func(c *Config) { c.EmbeddedDERP, c.PublicDERP = false, false }, "DERP"},
		{"public only", func(c *Config) { c.EmbeddedDERP, c.PublicDERP = false, true }, ""},
		{"cert without key", func(c *Config) { c.TLSCert = "c.pem" }, "together"},
		{"zero expiry", func(c *Config) { c.KeyExpiryDays = 0 }, "key_expiry_days"},
		{"bad pool", func(c *Config) { c.Pools = []string{"Bad Pool"} }, "pool"},
		{"bad hub name", func(c *Config) { c.HubName = "-x" }, "hub_name"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			c := ok
			tt.mutate(&c)
			err := c.Validate()
			if tt.want == "" && err != nil || tt.want != "" && (err == nil || !strings.Contains(err.Error(), tt.want)) {
				t.Fatalf("got %v, want error containing %q", err, tt.want)
			}
		})
	}
}

func TestHeadscaleConfig(t *testing.T) {
	tests := []struct {
		name   string
		mutate func(*Config)
		check  func(t *testing.T, m map[string]any)
	}{
		{"defaults: hub DERP map file, headscale's own DERP off, public map off", func(*Config) {}, func(t *testing.T, m map[string]any) {
			derp := m["derp"].(map[string]any)
			if derp["server"].(map[string]any)["enabled"] != false {
				t.Error("headscale's plain-HTTP embedded DERP enabled")
			}
			if paths := derp["paths"].([]any); len(paths) != 1 || !strings.HasSuffix(paths[0].(string), "derpmap.json") {
				t.Errorf("paths = %v", paths)
			}
			if len(derp["urls"].([]any)) != 0 || derp["auto_update_enabled"] != false {
				t.Errorf("public derp map enabled: %v", derp)
			}
			if m["database"].(map[string]any)["type"] != "sqlite" {
				t.Error("not sqlite")
			}
			if m["dns"].(map[string]any)["override_local_dns"] != false {
				t.Error("override_local_dns")
			}
			if m["policy"].(map[string]any)["mode"] != "file" {
				t.Error("policy mode")
			}
		}},
		{"public derp", func(c *Config) { c.PublicDERP = true }, func(t *testing.T, m map[string]any) {
			if urls := m["derp"].(map[string]any)["urls"].([]any); len(urls) != 1 {
				t.Errorf("urls = %v", urls)
			}
		}},
		{"embedded off", func(c *Config) { c.EmbeddedDERP, c.PublicDERP = false, true }, func(t *testing.T, m map[string]any) {
			if paths := m["derp"].(map[string]any)["paths"].([]any); len(paths) != 0 {
				t.Errorf("paths = %v", paths)
			}
		}},
		{"tls", func(c *Config) { c.TLSCert, c.TLSKey = "/c", "/k" }, func(t *testing.T, m map[string]any) {
			if m["tls_cert_path"] != "/c" {
				t.Errorf("config = %v", m)
			}
		}},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			c := DefaultConfig("http://10.0.0.1:7480")
			tt.mutate(&c)
			raw, err := c.HeadscaleConfig(t.TempDir())
			if err != nil {
				t.Fatal(err)
			}
			var m map[string]any
			if err := json.Unmarshal(raw, &m); err != nil {
				t.Fatal(err)
			}
			tt.check(t, m)
		})
	}
}

func TestDERPMapAndCert(t *testing.T) {
	tests := []struct {
		name   string
		server string
		ipv4   string
		host   string
		stun   string
		ipSAN  bool
	}{
		{"lan ip", "http://10.0.0.1:7480", "", "10.0.0.1", "0.0.0.0:3478", true},
		{"dns name and ipv4 override", "https://hub.example.com", "203.0.113.5", "hub.example.com", "0.0.0.0:3478", false},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			home := t.TempDir()
			c := DefaultConfig(tt.server)
			c.DERPIPv4 = tt.ipv4
			if err := c.writeDERPMap(home); err != nil {
				t.Fatal(err)
			}
			raw, err := os.ReadFile(derpMapPath(home))
			if err != nil {
				t.Fatal(err)
			}
			var m struct {
				Regions map[string]struct {
					Nodes []map[string]any
				}
			}
			// The file is YAML flow syntax; the integer region key is the only non-JSON part.
			if err := json.Unmarshal(bytes.Replace(raw, []byte("{999:"), []byte(`{"999":`), 1), &m); err != nil {
				t.Fatalf("%v\n%s", err, raw)
			}
			n := m.Regions["999"].Nodes[0]
			cert, err := loadDERPCert(home, tt.host)
			if err != nil {
				t.Fatal(err)
			}
			if n["hostname"] != tt.host || n["certname"] != certPin(cert) || n["derpport"] != float64(7483) || n["stunport"] != float64(3478) {
				t.Errorf("node = %v", n)
			}
			if (n["ipv4"] != nil) != (tt.ipv4 != "") {
				t.Errorf("ipv4 = %v", n["ipv4"])
			}
			leaf, err := x509.ParseCertificate(cert.Certificate[0])
			if err != nil {
				t.Fatal(err)
			}
			if err := leaf.VerifyHostname(tt.host); err != nil || (len(leaf.IPAddresses) == 1) != tt.ipSAN {
				t.Errorf("SAN: %v ips=%v", err, leaf.IPAddresses)
			}
			again, err := loadDERPCert(home, tt.host)
			if err != nil || certPin(again) != certPin(cert) {
				t.Errorf("cert not stable across loads: %v", err)
			}
			other, err := loadDERPCert(home, "other.example.com")
			if err != nil || certPin(other) == certPin(cert) {
				t.Errorf("cert not regenerated for a new host: %v", err)
			}
		})
	}
}

type testPolicy struct {
	TagOwners map[string][]string `json:"tagOwners"`
	ACLs      []aclRule           `json:"acls"`
}

func TestPolicy(t *testing.T) {
	tests := []struct {
		name  string
		pools []string
		rules []string
	}{
		{"no pools", nil, []string{
			"tag:hub>tag:node:7475,tag:node:22",
			"tag:client>tag:hub:7470",
		}},
		{"pools sorted and deduplicated", []string{"gpu", "build", "gpu"}, []string{
			"tag:hub>tag:node:7475,tag:node:22",
			"tag:client>tag:hub:7470",
			"tag:pool-build>tag:pool-build:*",
			"tag:pool-gpu>tag:pool-gpu:*",
		}},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			c := DefaultConfig("http://x:1")
			c.Pools = tt.pools
			raw, err := c.Policy()
			if err != nil {
				t.Fatal(err)
			}
			var p testPolicy
			if err := json.Unmarshal(raw, &p); err != nil {
				t.Fatal(err)
			}
			var got []string
			for _, r := range p.ACLs {
				if r.Action != "accept" {
					t.Errorf("action %q", r.Action)
				}
				got = append(got, strings.Join(r.Src, ",")+">"+strings.Join(r.Dst, ","))
			}
			if !slices.Equal(got, tt.rules) {
				t.Errorf("rules\n got %v\nwant %v", got, tt.rules)
			}
			for _, tag := range []string{TagHub, TagNode, TagClient, TierTag("server"), TierTag("family")} {
				if len(p.TagOwners[tag]) == 0 {
					t.Errorf("tag %s has no owner", tag)
				}
			}
			for _, r := range p.ACLs {
				for _, d := range r.Dst {
					if strings.HasPrefix(d, TagHub+":") && !slices.Contains(r.Src, TagClient) {
						t.Errorf("hub reachable from %v", r.Src)
					}
				}
			}
		})
	}
}

func TestSanitizeLabel(t *testing.T) {
	tests := map[string]string{
		"Office-PC":             "office-pc",
		"my laptop_2":           "my-laptop-2",
		"--x--":                 "x",
		"":                      "",
		strings.Repeat("a", 80): strings.Repeat("a", 63),
	}
	for in, want := range tests {
		if got := SanitizeLabel(in); got != want {
			t.Errorf("SanitizeLabel(%q) = %q, want %q", in, got, want)
		}
	}
}

func whoFake(node *tailcfg.Node, err error) whoIsFunc {
	return func(context.Context, string) (*apitype.WhoIsResponse, error) {
		if err != nil {
			return nil, err
		}
		return &apitype.WhoIsResponse{Node: node}, nil
	}
}

func TestIdentify(t *testing.T) {
	tests := []struct {
		name    string
		who     whoIsFunc
		allowed []string
		ok      bool
	}{
		{"hub allowed on node", whoFake(&tailcfg.Node{Name: "hub", Tags: []string{TagHub}}, nil), []string{TagHub}, true},
		{"node rejected on node", whoFake(&tailcfg.Node{Name: "n", Tags: []string{TagNode}}, nil), []string{TagHub}, false},
		{"client rejected on node", whoFake(&tailcfg.Node{Tags: []string{TagClient}}, nil), []string{TagHub}, false},
		{"untagged rejected", whoFake(&tailcfg.Node{Name: "laptop"}, nil), []string{TagHub}, false},
		{"multi-tag with hub", whoFake(&tailcfg.Node{Name: "h.ts.net.", Tags: []string{TagNode, TagHub}}, nil), []string{TagHub}, true},
		{"client allowed on api", whoFake(&tailcfg.Node{Name: "phone.ts.net.", Tags: []string{TagClient}}, nil), []string{TagHub, TagClient}, true},
		{"hub allowed on api", whoFake(&tailcfg.Node{Name: "hub.ts.net.", Tags: []string{TagHub}}, nil), []string{TagHub, TagClient}, true},
		{"node rejected on api", whoFake(&tailcfg.Node{Tags: []string{TagNode, TierTag("server")}}, nil), []string{TagHub, TagClient}, false},
		{"pool tag alone rejected", whoFake(&tailcfg.Node{Tags: []string{PoolTag("build")}}, nil), []string{TagHub, TagClient}, false},
		{"whois error fails closed", whoFake(nil, errors.New("boom")), []string{TagHub}, false},
		{"no node fails closed", whoFake(nil, nil), []string{TagHub}, false},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			r := httptest.NewRequest(http.MethodGet, "/", nil)
			who, err := identify(tt.who, tt.allowed...)(r)
			if (err == nil) != tt.ok {
				t.Fatalf("err = %v, want ok=%v", err, tt.ok)
			}
			if err == nil && who == "" {
				t.Errorf("accepted caller has no identity")
			}
			if err != nil && !errors.Is(err, ErrForbidden) {
				t.Errorf("error does not wrap ErrForbidden: %v", err)
			}
		})
	}
}

func TestRetryDial(t *testing.T) {
	noHost := &net.DNSError{Err: "no such host", Name: "n", IsNotFound: true}
	tests := []struct {
		name      string
		failures  int
		err       error
		window    time.Duration
		wantCalls int
		wantErr   bool
	}{
		{"succeeds first", 0, noHost, time.Second, 1, false},
		{"recovers after lookups fail", 3, noHost, 10 * time.Second, 4, false},
		{"string match", 2, errors.New("dial: lookup n: no such host"), 10 * time.Second, 3, false},
		{"other errors are not retried", 5, errors.New("connection refused"), 10 * time.Second, 1, true},
		{"window bounds retries", 1000, noHost, 500 * time.Millisecond, -1, true},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			calls := 0
			dial := func(context.Context, string, string) (net.Conn, error) {
				calls++
				if calls <= tt.failures {
					return nil, tt.err
				}
				c, _ := net.Pipe()
				return c, nil
			}
			c, err := retryDial(context.Background(), dial, tt.window, "tcp", "n:1")
			if (err != nil) != tt.wantErr {
				t.Fatalf("err = %v", err)
			}
			if c != nil {
				c.Close()
			}
			if tt.wantCalls >= 0 && calls != tt.wantCalls {
				t.Errorf("calls = %d, want %d", calls, tt.wantCalls)
			}
			if tt.wantCalls < 0 && (calls < 2 || calls > 10) {
				t.Errorf("calls = %d, want a few", calls)
			}
		})
	}
}

func TestRetryDialContextCancel(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	dial := func(context.Context, string, string) (net.Conn, error) {
		return nil, &net.DNSError{Err: "no such host", IsNotFound: true}
	}
	if _, err := retryDial(ctx, dial, time.Minute, "tcp", "n:1"); !errors.Is(err, context.Canceled) {
		t.Fatalf("err = %v", err)
	}
}

func TestParseChecksum(t *testing.T) {
	good := strings.Repeat("ab", 32)
	sums := good + "  headscale_1_linux_arm64\n" + strings.Repeat("cd", 32) + "  other\n"
	tests := []struct {
		name, sums, asset string
		want              string
		wantErr           bool
	}{
		{"found", sums, "headscale_1_linux_arm64", good, false},
		{"missing", sums, "headscale_1_linux_amd64", "", true},
		{"prefix is not a match", sums, "headscale_1_linux_arm", "", true},
		{"malformed", "zz  a\n", "a", "", true},
		{"short hash", "abcd  a\n", "a", "", true},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := parseChecksum(tt.sums, tt.asset)
			if (err != nil) != tt.wantErr || got != tt.want {
				t.Fatalf("got %q, %v", got, err)
			}
		})
	}
}

func TestInstall(t *testing.T) {
	payload := []byte("#!/bin/sh\necho headscale\n")
	sum := sha256.Sum256(payload)
	asset := "headscale_9.9.9_linux_arm64"
	serve := func(published string, body []byte) *httptest.Server {
		return httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			switch {
			case strings.HasSuffix(r.URL.Path, "/checksums.txt"):
				fmt.Fprintf(w, "%s  %s\n", published, asset)
			case strings.HasSuffix(r.URL.Path, "/"+asset):
				w.Write(body)
			default:
				http.NotFound(w, r)
			}
		}))
	}
	tests := []struct {
		name      string
		published string
		body      []byte
		wantErr   string
	}{
		{"ok", hex.EncodeToString(sum[:]), payload, ""},
		{"tampered binary", hex.EncodeToString(sum[:]), []byte("evil"), "mismatch"},
		{"wrong published sum", strings.Repeat("00", 32), payload, "mismatch"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			srv := serve(tt.published, tt.body)
			defer srv.Close()
			home := t.TempDir()
			path, err := install(context.Background(), home, "9.9.9", srv.URL, "linux", "arm64", srv.Client())
			if tt.wantErr != "" {
				if err == nil || !strings.Contains(err.Error(), tt.wantErr) {
					t.Fatalf("err = %v", err)
				}
				if _, statErr := os.Stat(BinaryPath(home, "9.9.9")); statErr == nil {
					t.Fatal("unverified binary was installed")
				}
				return
			}
			if err != nil {
				t.Fatal(err)
			}
			if fi, err := os.Stat(path); err != nil || (runtime.GOOS != "windows" && fi.Mode().Perm() != 0o755) {
				t.Fatalf("stat %s: %v %v", path, fi, err)
			}
		})
	}
	t.Run("non-linux hub", func(t *testing.T) {
		if _, err := install(context.Background(), t.TempDir(), "9.9.9", "http://x", "windows", "amd64", nil); err == nil {
			t.Fatal("expected error")
		}
	})
}

func TestPinnedChecksumMismatch(t *testing.T) {
	asset := "headscale_" + HeadscaleVersion + "_linux_arm64"
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		fmt.Fprintf(w, "%s  %s\n", strings.Repeat("11", 32), asset)
	}))
	defer srv.Close()
	_, err := install(context.Background(), t.TempDir(), HeadscaleVersion, srv.URL, "linux", "arm64", srv.Client())
	if err == nil || !strings.Contains(err.Error(), "pinned") {
		t.Fatalf("err = %v", err)
	}
}

func TestConfigSaveLoad(t *testing.T) {
	home := t.TempDir()
	c := DefaultConfig("http://10.0.0.1:7480")
	c.Pools = []string{"build"}
	if err := c.Save(home); err != nil {
		t.Fatal(err)
	}
	if err := c.WriteFiles(home); err != nil {
		t.Fatal(err)
	}
	got, err := LoadConfig(home)
	if err != nil || got.ServerURL != c.ServerURL || !slices.Equal(got.Pools, c.Pools) {
		t.Fatalf("got %+v, %v", got, err)
	}
	fi, err := os.Stat(configPath(home))
	if err != nil || (runtime.GOOS != "windows" && fi.Mode().Perm() != 0o600) {
		t.Fatalf("config mode: %v %v", fi, err)
	}
	if _, err := LoadConfig(t.TempDir()); !errors.Is(err, os.ErrNotExist) {
		t.Fatalf("err = %v", err)
	}
}

func TestSocketPathShort(t *testing.T) {
	long := "/" + strings.Repeat("d/", 60)
	if p := socketPath(long); len(p) >= 108 {
		t.Fatalf("socket path too long: %d", len(p))
	}
	if p := socketPath("/h"); p != filepath.Join("/h", "tailnet", "headscale.sock") {
		t.Fatalf("p = %s", p)
	}
}

func TestRotatingLog(t *testing.T) {
	path := filepath.Join(t.TempDir(), "headscale.log")
	l, err := openRotatingLog(path, 10, 2)
	if err != nil {
		t.Fatal(err)
	}
	for _, s := range []string{"aaaaaa", "bbbbbb", "cccccc", "dddddd"} {
		if _, err := l.Write([]byte(s)); err != nil {
			t.Fatal(err)
		}
	}
	l.Close()
	want := map[string]string{path: "dddddd", path + ".1": "cccccc", path + ".2": "bbbbbb"}
	for p, w := range want {
		if got, err := os.ReadFile(p); err != nil || string(got) != w {
			t.Errorf("%s = %q, %v; want %q", filepath.Base(p), got, err, w)
		}
	}
	if _, err := os.Stat(path + ".3"); err == nil {
		t.Error("kept more than 2 rotated files")
	}

	l2, err := openRotatingLog(path, 10, 2)
	if err != nil {
		t.Fatal(err)
	}
	l2.Write([]byte("ee"))
	l2.Close()
	if got, _ := os.ReadFile(path); string(got) != "ddddddee" {
		t.Errorf("reopen did not append: %q", got)
	}
}

func TestNodeEnrolledWith(t *testing.T) {
	tests := []struct {
		name string
		raw  string
		want uint64
	}{
		{"preauth", `{"id":"4","givenName":"a","preAuthKey":{"id":"12","key":"x"}}`, 12},
		{"interactive login", `{"id":"5","givenName":"b"}`, 0},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			var n Node
			if err := json.Unmarshal([]byte(tt.raw), &n); err != nil {
				t.Fatal(err)
			}
			if got := n.EnrolledWith(); got != tt.want {
				t.Fatalf("EnrolledWith = %d, want %d", got, tt.want)
			}
		})
	}
}

func TestTransportBoundsEveryPhase(t *testing.T) {
	old := dialAttemptTimeout
	dialAttemptTimeout = 100 * time.Millisecond
	defer func() { dialAttemptTimeout = old }()

	hang := func(ctx context.Context, _, _ string) (net.Conn, error) {
		<-ctx.Done()
		return nil, ctx.Err()
	}
	tr := newTransport(hang, func() time.Duration { return time.Minute })
	if tr.ResponseHeaderTimeout <= 0 || tr.IdleConnTimeout <= 0 || tr.IdleConnTimeout > time.Minute {
		t.Fatalf("unbounded transport: header=%s idle=%s", tr.ResponseHeaderTimeout, tr.IdleConnTimeout)
	}
	c := &http.Client{Transport: tr}
	start := time.Now()
	_, err := c.Get("http://peer:7475/")
	if !errors.Is(err, context.DeadlineExceeded) {
		t.Fatalf("hung dial = %v, want deadline exceeded", err)
	}
	if d := time.Since(start); d > 2*time.Second {
		t.Fatalf("hung dial took %s", d)
	}
}

func TestHubAddr(t *testing.T) {
	tags := func(v ...string) *views.Slice[string] { s := views.SliceOf(v); return &s }
	st := &ipnstate.Status{Peer: map[key.NodePublic]*ipnstate.PeerStatus{
		key.NewNode().Public(): {Tags: tags(TagNode), TailscaleIPs: []netip.Addr{netip.MustParseAddr("100.64.0.9")}},
		key.NewNode().Public(): {Tags: tags(TagHub)},
		key.NewNode().Public(): {},
	}}
	if _, ok := hubAddr(st); ok {
		t.Fatal("hub without addresses accepted")
	}
	want := netip.MustParseAddr("100.64.0.1")
	st.Peer[key.NewNode().Public()] = &ipnstate.PeerStatus{Tags: tags(TagHub, TagClient), TailscaleIPs: []netip.Addr{want, netip.MustParseAddr("fd7a::1")}}
	if got, ok := hubAddr(st); !ok || got != want {
		t.Fatalf("hubAddr = %v, %v; want %v", got, ok, want)
	}
}

func TestQuietDERPLog(t *testing.T) {
	reset := &net.OpError{Op: "read", Err: os.NewSyscallError("read", syscall.ECONNRESET)}
	tests := []struct {
		name   string
		format string
		args   []any
		logged bool
	}{
		{"eof", "derp: %s: %v", []any{"1.2.3.4:5", fmt.Errorf("client abcd: readFrameHeader: %w", io.EOF)}, false},
		{"reset", "derp: %s: %v", []any{"1.2.3.4:5", fmt.Errorf("client abcd: readFrameHeader: %w", reset)}, false},
		{"closed", "derp: %s: %v", []any{"1.2.3.4:5", fmt.Errorf("write: %w", net.ErrClosed)}, false},
		{"real error", "derp: %s: %v", []any{"1.2.3.4:5", errors.New("bad frame length")}, true},
		{"other message", "DERP relay on %s", []any{"x"}, true},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			var n int
			quietDERPLog(func(string, ...any) { n++ })(tt.format, tt.args...)
			if (n == 1) != tt.logged {
				t.Fatalf("logged %d times, want logged=%v", n, tt.logged)
			}
		})
	}
}
