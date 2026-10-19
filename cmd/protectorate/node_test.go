package main

import (
	"bytes"
	"context"
	"fmt"
	"net"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/spf13/cobra"
	"google.golang.org/protobuf/proto"
	"google.golang.org/protobuf/types/known/durationpb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/internal/hub"
	"github.com/hotschmoe/protectorate/internal/node"
)

// fakeTailnet hands out numbered keys and records what was expired.
type fakeTailnet struct {
	mu      sync.Mutex
	next    uint64
	expired []uint64
}

func (f *fakeTailnet) ControlURL() string { return "http://hs.lan:7480" }

func (f *fakeTailnet) InviteNode(_ context.Context, _, _ string, _ []string, _ time.Duration) (string, uint64, error) {
	f.mu.Lock()
	defer f.mu.Unlock()
	f.next++
	return fmt.Sprintf("hskey-auth-%d-test", f.next), f.next, nil
}

func (f *fakeTailnet) ExpireKey(_ context.Context, id uint64) error {
	f.mu.Lock()
	defer f.mu.Unlock()
	f.expired = append(f.expired, id)
	return nil
}

func (f *fakeTailnet) Revoke(context.Context, string) (bool, error) { return false, nil }

func (f *fakeTailnet) SetTier(context.Context, string, string) (bool, error) { return false, nil }

func (f *fakeTailnet) expiredKeys() []uint64 {
	f.mu.Lock()
	defer f.mu.Unlock()
	return slices.Clone(f.expired)
}

func TestNodeCLI(t *testing.T) {
	t.Setenv("PROTECTORATE_HOME", t.TempDir())
	t.Setenv("DOCKER_HOST", "unix:///nonexistent.sock")
	tok, err := hub.EnsureToken()
	if err != nil {
		t.Fatal(err)
	}
	agent := &node.Agent{Collector: node.NewCollector(node.Options{}), Authorize: node.TokenAuthorizer("nt")}
	al, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	h := hub.New("test", tok)
	if err := attachNodes(h, time.Hour, "nt", nil, nil, nil); err != nil {
		t.Fatal(err)
	}
	ts := &fakeTailnet{}
	h.SetTailnetAdmin(ts)
	hl, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithCancel(context.Background())
	done := make(chan error, 2)
	go func() { done <- agent.Serve(ctx, al) }()
	go func() { done <- h.Serve(ctx, hl) }()
	t.Cleanup(func() { cancel(); <-done; <-done })
	url := "http://" + hl.Addr().String()

	run := func(t *testing.T, args ...string) (string, error) {
		t.Helper()
		var buf bytes.Buffer
		cmd := newRoot()
		cmd.SetOut(&buf)
		cmd.SetArgs(append(args, "--hub", url))
		err := cmd.Execute()
		return buf.String(), err
	}
	waitFor := func(t *testing.T, args []string, want string) string {
		t.Helper()
		var out string
		for end := time.Now().Add(10 * time.Second); time.Now().Before(end); time.Sleep(50 * time.Millisecond) {
			out, _ = run(t, args...)
			if strings.Contains(out, want) {
				return out
			}
		}
		t.Fatalf("%v never contained %q:\n%s", args, want, out)
		return ""
	}

	tests := []struct {
		name    string
		args    []string
		want    []string
		wantErr bool
	}{
		{"add", []string{"node", "add", "peer", "--addr", al.Addr().String(), "--ssh", "peer-ssh", "--tier", "care", "--tag", "lab"}, []string{"added peer"}, false},
		{"add duplicate", []string{"node", "add", "peer", "--addr", "x"}, nil, true},
		{"add without addr", []string{"node", "add", "other"}, nil, true},
		{"add bad tier", []string{"node", "add", "other", "--addr", "x", "--tier", "root"}, nil, true},
		{"tier unknown node", []string{"node", "tier", "ghost", "care"}, nil, true},
		{"tier bad value", []string{"node", "tier", "peer", "family"}, nil, true},
		{"tier self", []string{"node", "tier", localNodeName(), "care"}, nil, true},
		{"rm unknown", []string{"node", "rm", "ghost"}, nil, true},
		{"rm self", []string{"node", "rm", localNodeName()}, nil, true},
		{"show unknown", []string{"node", "show", "ghost"}, nil, true},
		{"metrics needs --local", []string{"node", "metrics"}, nil, true},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			out, err := run(t, tt.args...)
			if (err != nil) != tt.wantErr {
				t.Fatalf("err = %v\n%s", err, out)
			}
			for _, w := range tt.want {
				if !strings.Contains(out, w) {
					t.Fatalf("output %q missing %q", out, w)
				}
			}
		})
	}

	t.Run("ls", func(t *testing.T) {
		out := waitFor(t, []string{"node", "ls"}, "fresh")
		for _, w := range []string{"NAME", "FRESHNESS", localNodeName() + "*", "peer"} {
			if !strings.Contains(out, w) {
				t.Fatalf("ls output missing %q:\n%s", w, out)
			}
		}
		js := waitFor(t, []string{"node", "ls", "--json"}, `"freshness":"NODE_FRESHNESS_FRESH"`)
		if strings.Count(js, "\n") != 1 {
			t.Fatalf("--json must be one line:\n%s", js)
		}
	})
	t.Run("show", func(t *testing.T) {
		out := waitFor(t, []string{"node", "show", "peer"}, "FILESYSTEM")
		for _, w := range []string{"tier", "care", "peer-ssh", "memory", "cpu"} {
			if !strings.Contains(out, w) {
				t.Fatalf("show output missing %q:\n%s", w, out)
			}
		}
	})
	t.Run("tier", func(t *testing.T) {
		out, err := run(t, "node", "tier", "peer", "shared")
		if err != nil || !strings.Contains(out, "peer is now tier shared") {
			t.Fatalf("err=%v\n%s", err, out)
		}
		if out, _ := run(t, "node", "show", "peer"); !strings.Contains(out, "shared") || strings.Contains(out, "care") {
			t.Fatalf("show after tier change:\n%s", out)
		}
	})
	t.Run("metrics local", func(t *testing.T) {
		out, err := run(t, "node", "metrics", "--local")
		if err != nil || !strings.Contains(out, "FILESYSTEM") {
			t.Fatalf("err=%v\n%s", err, out)
		}
		js, err := run(t, "node", "metrics", "--local", "--json")
		if err != nil || !strings.Contains(js, `"hostname":`) {
			t.Fatalf("err=%v\n%s", err, js)
		}
	})
	t.Run("invite recorded", func(t *testing.T) {
		out, err := run(t, "node", "invite", "newbox", "--tier", "care", "--pool", "gpu", "--url", "http://hub.lan:7470")
		for _, w := range []string{
			"WINDOWS (PowerShell):\n  $env:PROTECTORATE_ENROLL_KEY='hskey-auth-1-test'; $env:PROTECTORATE_NODE_NAME='newbox'; irm 'http://hub.lan:7470/install.ps1' | iex",
			"LINUX (sh):\n  curl -fsSL 'http://hub.lan:7470/install.sh' | PROTECTORATE_ENROLL_KEY='hskey-auth-1-test' sh -s -- --name 'newbox'",
			"PROTECTORATE_ENROLL_KEY=hskey-auth-1-test protectorate node install --control http://hs.lan:7480 --name newbox",
		} {
			if err != nil || !strings.Contains(out, w) {
				t.Fatalf("err=%v, missing %q in\n%s", err, w, out)
			}
		}
		if _, err := run(t, "node", "invite", "newbox"); err != nil {
			t.Fatalf("re-invite: %v", err)
		}
		ls, err := run(t, "node", "ls")
		if err != nil || !strings.Contains(ls, "newbox") || !strings.Contains(ls, "invited, not yet joined") || strings.Contains(ls, "UNREACHABLE") {
			t.Fatalf("err=%v\n%s", err, ls)
		}
		show, err := run(t, "node", "show", "newbox")
		if err != nil || !strings.Contains(show, "shared") {
			t.Fatalf("err=%v\n%s", err, show)
		}
		if out, err := run(t, "node", "rm", "newbox"); err != nil || !strings.Contains(out, "expired the invite key") {
			t.Fatalf("cancel invite: err=%v\n%s", err, out)
		}
		if got := ts.expiredKeys(); !slices.Equal(got, []uint64{1, 2}) {
			t.Fatalf("expired keys %v, want the first on re-invite and the second on cancel", got)
		}
		if _, err := run(t, "node", "invite", "Bad_Name"); err == nil {
			t.Fatal("invalid name accepted")
		}
	})
	t.Run("rm", func(t *testing.T) {
		if out, err := run(t, "node", "rm", "peer"); err != nil || !strings.Contains(out, "removed peer") {
			t.Fatalf("err=%v\n%s", err, out)
		}
		if _, err := run(t, "node", "show", "peer"); err == nil {
			t.Fatal("peer still present")
		}
	})
}

func TestWriteNodeList(t *testing.T) {
	load := 1.5
	resp := &pb.ListNodesResponse{Nodes: []*pb.NodeStatus{
		{
			Node: &pb.NodeInfo{Name: "hub", Os: "linux", Arch: "arm64", Local: true}, Freshness: pb.NodeFreshness_NODE_FRESHNESS_FRESH,
			Metrics: &pb.NodeMetrics{
				Cpu:         &pb.CpuMetrics{UsagePercent: 34.4, Load1: &load},
				Memory:      &pb.MemoryMetrics{UsedPercent: 21.7},
				Filesystems: []*pb.FilesystemMetrics{{Mount: "/", UsedPercent: 36.7}, {Mount: "/data", UsedPercent: 91, Primary: true}},
				Gpus:        []*pb.GpuMetrics{{UtilizationPercent: 10, MemoryUsedBytes: proto.Uint64(1 << 30), MemoryTotalBytes: 8 << 30}, {UtilizationPercent: 30, MemoryUsedBytes: proto.Uint64(1 << 30), MemoryTotalBytes: 8 << 30}},
				Engine:      &pb.ContainerEngine{Containers: []*pb.ContainerMetrics{{}, {}}},
			},
			Alerts: []*pb.NodeAlert{{Kind: pb.AlertKind_ALERT_KIND_DISK_PERCENT, Subject: "/data"}},
		},
		{Node: &pb.NodeInfo{Name: "win"}, Freshness: pb.NodeFreshness_NODE_FRESHNESS_UNREACHABLE, Alerts: []*pb.NodeAlert{{Kind: pb.AlertKind_ALERT_KIND_UNREACHABLE}}},
		{Node: &pb.NodeInfo{Name: "old", Os: "windows", Arch: "amd64"}, Freshness: pb.NodeFreshness_NODE_FRESHNESS_STALE, Age: durationpb.New(2 * time.Minute), Metrics: &pb.NodeMetrics{}},
		{Node: &pb.NodeInfo{Name: "new"}},
	}}
	var buf bytes.Buffer
	writeNodeList(&buf, resp)
	out := buf.String()
	for _, want := range []string{
		"hub*", "linux/arm64", "34%", "2x 10% 2.0GiB/16.0GiB", "22%", "91% /data", "disk:/data", "UNREACHABLE (never seen)",
		"STALE (2m old)", "windows/amd64", "pending",
	} {
		if !strings.Contains(out, want) {
			t.Errorf("output missing %q:\n%s", want, out)
		}
	}
}

func TestGpuSummary(t *testing.T) {
	adreno := &pb.GpuMetrics{Vendor: "qualcomm", Name: "Qualcomm Adreno X2-90 GPU", Integrated: true, UtilizationPercent: -1}
	b50 := &pb.GpuMetrics{Vendor: "intel", Name: "Intel Arc Pro B50 Graphics", UtilizationPercent: 2.4, MemoryUsedBytes: proto.Uint64(3 << 30), MemoryTotalBytes: 16 << 30}
	igpu := &pb.GpuMetrics{Vendor: "intel", Name: "Intel Graphics", Integrated: true}
	b70 := &pb.GpuMetrics{Vendor: "intel", Name: "Battlemage G31 [Intel Graphics]", MemoryTotalBytes: 32 << 30}
	tests := []struct {
		name string
		gpus []*pb.GpuMetrics
		want string
	}{
		{"none", nil, "-"},
		{"integrated without utilization", []*pb.GpuMetrics{adreno}, "1x -"},
		{"discrete first, integrated adds no vram", []*pb.GpuMetrics{b50, igpu}, "2x 2% 3.0GiB/16.0GiB"},
		{"unknown vram use", []*pb.GpuMetrics{b70, b70}, "2x 0% ?/64.0GiB"},
	}
	for _, tt := range tests {
		if got := gpuSummary(tt.gpus); got != tt.want {
			t.Errorf("%s: gpuSummary = %q, want %q", tt.name, got, tt.want)
		}
	}
}

func TestWriteMetricsAccelerators(t *testing.T) {
	m := &pb.NodeMetrics{
		Gpus: []*pb.GpuMetrics{
			{Index: 0, Vendor: "intel", Name: "Intel Arc Pro B50 Graphics", Driver: "igfxnd 32.0.101.8805", UtilizationPercent: 2.4, MemoryUsedBytes: proto.Uint64(3 << 30), MemoryTotalBytes: 16 << 30, TemperatureCelsius: -1},
			{Index: 1, Vendor: "arm", Name: "Mali-G720-Immortalis 10 cores", Driver: "mali r53p0-00eac0", Integrated: true, UtilizationPercent: -1, TemperatureCelsius: -1},
		},
		Npus: []*pb.AcceleratorMetrics{{Kind: "npu", Vendor: "armchina", Name: "Zhouyi NPU", Driver: "armchina", UtilizationPercent: -1, FrequencyHz: 1200000000}},
	}
	var buf bytes.Buffer
	writeMetrics(&buf, m)
	out := buf.String()
	for _, want := range []string{
		"0\tintel Intel Arc Pro B50 Graphics\tdiscrete\tigfxnd 32.0.101.8805\t2%\t3.0GiB/16.0GiB\t-\n",
		"1\tarm Mali-G720-Immortalis 10 cores\tintegrated\tmali r53p0-00eac0\t-\tshared\t-\n",
		"0\tarmchina Zhouyi NPU\tarmchina\t-\t1200 MHz\n",
	} {
		if !strings.Contains(out, want) {
			t.Errorf("output missing %q:\n%s", want, out)
		}
	}
}

func TestFmtBytes(t *testing.T) {
	tests := []struct {
		in   uint64
		want string
	}{{0, "0B"}, {1023, "1023B"}, {1024, "1.0KiB"}, {5 << 30, "5.0GiB"}, {3 << 40, "3.0TiB"}}
	for _, tt := range tests {
		if got := fmtBytes(tt.in); got != tt.want {
			t.Errorf("fmtBytes(%d) = %q, want %q", tt.in, got, tt.want)
		}
	}
}

func TestNodeToken(t *testing.T) {
	dir := t.TempDir()
	write := func(name, body string) string {
		p := filepath.Join(dir, name)
		if err := os.WriteFile(p, []byte(body), 0o600); err != nil {
			t.Fatal(err)
		}
		return p
	}
	tests := []struct {
		name    string
		file    string
		env     string
		want    string
		wantErr bool
	}{
		{"none", "", "", "", false},
		{"env", "", " from-env\n", "from-env", false},
		{"file", write("tok", "from-file\n"), "", "from-file", false},
		{"file beats env", write("tok2", "from-file"), "from-env", "from-file", false},
		{"empty file", write("empty", " \n"), "from-env", "", true},
		{"missing file", filepath.Join(dir, "missing"), "from-env", "", true},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Setenv(nodeTokenEnv, tt.env)
			got, err := nodeToken(tt.file)
			if (err != nil) != tt.wantErr || got != tt.want {
				t.Fatalf("nodeToken = %q, %v", got, err)
			}
		})
	}
}

func TestNoSecretFlags(t *testing.T) {
	serve := func(c *cobra.Command) *cobra.Command {
		for _, sub := range c.Commands() {
			if sub.Name() == "serve" {
				return sub
			}
		}
		t.Fatal("no serve command")
		return nil
	}
	tests := []struct {
		name string
		cmd  *cobra.Command
		gone []string
		has  string
	}{
		{"node serve", nodeServeCmd(), []string{"token"}, "node-token-file"},
		{"hub serve", serve(hubCmd()), []string{"node-token", "token"}, "node-token-file"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			for _, g := range tt.gone {
				if tt.cmd.Flags().Lookup(g) != nil {
					t.Errorf("secret-bearing flag --%s still exists", g)
				}
			}
			if tt.cmd.Flags().Lookup(tt.has) == nil {
				t.Errorf("missing --%s", tt.has)
			}
		})
	}
}
