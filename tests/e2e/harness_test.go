package e2e

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"syscall"
	"testing"
	"time"
)

type process struct {
	cmd  *exec.Cmd
	done chan error
	log  *os.File
}
type cluster struct {
	t *testing.T
	// owner is the top-level test; processes live until it ends, even when a
	// subtest restarts them.
	owner                                   *testing.T
	ctx                                     context.Context
	binary, root, home, url, token, nodeURL string
	env                                     []string
	hub                                     *process
	node                                    *process
	client                                  *http.Client
}

func newCluster(t *testing.T) *cluster {
	t.Helper()
	if os.Getenv("PROT_E2E") != "1" {
		t.Skip("real binary scenarios: set PROT_E2E=1 (optionally PROT_E2E_BINARY=/absolute/protectorate)")
	}
	ctx, cancel := context.WithTimeout(t.Context(), 5*time.Minute)
	t.Cleanup(cancel)
	c := &cluster{t: t, owner: t, ctx: ctx, root: t.TempDir(), client: &http.Client{Timeout: 30 * time.Second}}
	c.home = filepath.Join(c.root, "hub")
	binDir := filepath.Join(c.root, ".local", "bin")
	if err := os.MkdirAll(binDir, 0700); err != nil {
		t.Fatal(err)
	}
	if selected, err := exec.LookPath("bwrap"); err == nil {
		b, err := os.ReadFile(selected)
		if err != nil {
			t.Fatalf("read selected bwrap %s: %v", selected, err)
		}
		if err := os.WriteFile(filepath.Join(binDir, "bwrap"), b, 0700); err != nil {
			t.Fatalf("copy selected bwrap into private home: %v", err)
		}
	}
	c.binary = os.Getenv("PROT_E2E_BINARY")
	if c.binary == "" {
		c.binary = filepath.Join(binDir, "protectorate")
		cmd := exec.CommandContext(ctx, "go", "build", "-p", "2", "-o", c.binary, "../../cmd/protectorate")
		if out, err := cmd.CombinedOutput(); err != nil {
			t.Fatalf("build real binary: %v\n%s", err, out)
		}
	}
	if c.binary != filepath.Join(binDir, "protectorate") {
		if !filepath.IsAbs(c.binary) {
			t.Fatal("PROT_E2E_BINARY must be an absolute path")
		}
		source, err := os.Open(c.binary)
		if err != nil {
			t.Fatalf("open supplied binary: %v", err)
		}
		destination, err := os.OpenFile(filepath.Join(binDir, "protectorate"), os.O_CREATE|os.O_EXCL|os.O_WRONLY, 0700)
		if err != nil {
			source.Close()
			t.Fatal(err)
		}
		_, copyErr := io.Copy(destination, source)
		closeErr := destination.Close()
		source.Close()
		if copyErr != nil || closeErr != nil {
			t.Fatalf("copy isolated binary: copy=%v close=%v", copyErr, closeErr)
		}
		c.binary = filepath.Join(binDir, "protectorate")
	}
	runCtx, runCancel := context.WithTimeout(t.Context(), 75*time.Second)
	t.Cleanup(runCancel)
	c.ctx = runCtx
	c.env = []string{"PATH=" + binDir + string(os.PathListSeparator) + os.Getenv("PATH"), "HOME=" + c.root, "PROTECTORATE_HOME=" + c.home, "PROTECTORATE_NODE_TOKEN=e2e-node-secret", "GOMAXPROCS=2", "HTTP_PROXY=", "HTTPS_PROXY=", "ALL_PROXY=", "NO_PROXY=*", "DOCKER_HOST=unix:///nonexistent-e2e.sock"}
	tmuxDir := filepath.Join(c.root, "tmux")
	if err := os.Mkdir(tmuxDir, 0700); err != nil {
		t.Fatal(err)
	}
	c.env = append(c.env, "TMUX_TMPDIR="+tmuxDir)
	t.Cleanup(func() {
		ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
		defer cancel()
		cmd := exec.CommandContext(ctx, "tmux", "-L", "e2e-"+filepath.Base(c.root), "kill-server")
		cmd.Env = c.env
		_ = cmd.Run()
	})
	// systemd-run --user needs the user manager's bus.
	for _, k := range []string{"XDG_RUNTIME_DIR", "DBUS_SESSION_BUS_ADDRESS"} {
		if v, ok := os.LookupEnv(k); ok {
			c.env = append(c.env, k+"="+v)
		}
	}
	if err := os.WriteFile(filepath.Join(binDir, "claude"), []byte(protocolPeer), 0700); err != nil {
		t.Fatal(err)
	}
	c.url = "http://" + loopback(t)
	c.nodeURL = "http://" + loopback(t)
	nodeBinary := filepath.Join(binDir, "node-protectorate")
	b, err := os.ReadFile(c.binary)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(nodeBinary, b, 0700); err != nil {
		t.Fatal(err)
	}
	nodeCluster := *c
	nodeCluster.binary = nodeBinary
	c.node = nodeCluster.start("node", append(c.env, "PROTECTORATE_HOME="+filepath.Join(c.root, "node"), "INVOCATION_ID=e2e"), "node", "serve", "--listen", strings.TrimPrefix(c.nodeURL, "http://"))
	c.boot()
	c.eventually(func() bool {
		return c.rpc("NodeService", "AddNode", map[string]any{"node": map[string]any{"name": "peer", "address": strings.TrimPrefix(c.nodeURL, "http://")}}, nil) == 200
	})
	c.eventually(func() bool {
		var v map[string]any
		return c.rpc("NodeService", "GetNode", map[string]any{"name": "peer"}, &v) == 200 && strings.Contains(fmt.Sprint(v), "NODE_FRESHNESS_FRESH")
	})
	return c
}
func loopback(t *testing.T) string {
	t.Helper()
	l, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	addr := l.Addr().String()
	l.Close()
	return addr
}
func (c *cluster) start(name string, env []string, args ...string) *process {
	c.t.Helper()
	f, err := os.CreateTemp(c.root, name+"-*.log")
	if err != nil {
		c.t.Fatal(err)
	}
	p := &process{cmd: exec.CommandContext(c.ctx, c.binary, args...), done: make(chan error, 1), log: f}
	p.cmd.Env = env
	p.cmd.Stdout = f
	p.cmd.Stderr = f
	if err := p.cmd.Start(); err != nil {
		c.t.Fatalf("start %s: %v", name, err)
	}
	go func() { p.done <- p.cmd.Wait(); close(p.done) }()
	owner := c.owner
	owner.Cleanup(func() {
		c.stop(p)
		f.Close()
		if owner.Failed() {
			b, _ := os.ReadFile(f.Name())
			owner.Logf("%s logs:\n%s", name, b)
		}
	})
	return p
}
func (c *cluster) stop(p *process) {
	if p == nil {
		return
	}
	select {
	case <-p.done:
		return
	default:
	}
	_ = p.cmd.Process.Signal(syscall.SIGTERM)
	select {
	case <-p.done:
	case <-time.After(5 * time.Second):
		_ = p.cmd.Process.Kill()
		<-p.done
	}
}
func (c *cluster) boot() {
	c.hub = c.start("hub", c.env, "hub", "serve", "--listen", strings.TrimPrefix(c.url, "http://"), "--node-interval", "100ms", "--tmux-socket", "e2e-"+filepath.Base(c.root))
	c.eventually(func() bool {
		b, e := os.ReadFile(filepath.Join(c.home, "token"))
		c.token = strings.TrimSpace(string(b))
		return e == nil && c.rpc("HubService", "GetStatus", map[string]any{}, nil) == 200
	})
}
func (c *cluster) eventually(f func() bool) {
	c.t.Helper()
	deadline := time.Now().Add(15 * time.Second)
	for time.Now().Before(deadline) {
		if f() {
			return
		}
		select {
		case <-c.ctx.Done():
			c.t.Fatal(c.ctx.Err())
		case <-time.After(100 * time.Millisecond):
		}
	}
	c.t.Fatal("condition did not become true within 15s")
}
func (c *cluster) rpc(service, method string, in, out any) int {
	c.t.Helper()
	b, err := json.Marshal(in)
	if err != nil {
		c.t.Fatal(err)
	}
	req, err := http.NewRequestWithContext(c.ctx, "POST", c.url+"/protectorate.v1."+service+"/"+method, bytes.NewReader(b))
	if err != nil {
		c.t.Fatal(err)
	}
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Authorization", "Bearer "+c.token)
	res, err := c.client.Do(req)
	if err != nil {
		c.t.Logf("%s.%s: %v", service, method, err)
		return 0
	}
	defer res.Body.Close()
	body, err := io.ReadAll(res.Body)
	if err != nil {
		c.t.Fatal(err)
	}
	if res.StatusCode != 200 {
		c.t.Logf("%s.%s: HTTP %d: %s", service, method, res.StatusCode, body)
	}
	if out != nil {
		if err := json.Unmarshal(body, out); err != nil {
			c.t.Fatalf("decode %s: %v", method, err)
		}
	}
	return res.StatusCode
}
func (c *cluster) call(service, method string, in any) map[string]any {
	c.t.Helper()
	var out map[string]any
	if status := c.rpc(service, method, in, &out); status != 200 {
		c.t.Fatalf("%s.%s status=%d input=%v", service, method, status, in)
	}
	return out
}
func (c *cluster) cli(args ...string) string {
	c.t.Helper()
	cmd := exec.CommandContext(c.ctx, c.binary, append([]string{"--hub", c.url}, args...)...)
	cmd.Env = c.env
	out, err := cmd.CombinedOutput()
	if err != nil {
		c.t.Fatalf("CLI %v: %v\n%s", args, err, out)
	}
	return string(out)
}
