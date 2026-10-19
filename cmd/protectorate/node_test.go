package main

import (
	"bytes"
	"context"
	"net"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/spf13/cobra"
	"google.golang.org/protobuf/types/known/durationpb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/internal/hub"
	"github.com/hotschmoe/protectorate/internal/node"
)

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
		{"add", []string{"node", "add", "peer", "--addr", al.Addr().String(), "--ssh", "peer-ssh", "--tier", "family", "--tag", "lab"}, []string{"added peer"}, false},
		{"add duplicate", []string{"node", "add", "peer", "--addr", "x"}, nil, true},
		{"add without addr", []string{"node", "add", "other"}, nil, true},
		{"add bad tier", []string{"node", "add", "other", "--addr", "x", "--tier", "root"}, nil, true},
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
		for _, w := range []string{"tier", "family", "peer-ssh", "memory", "cpu"} {
			if !strings.Contains(out, w) {
				t.Fatalf("show output missing %q:\n%s", w, out)
			}
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
		o := &opts{hubURL: url}
		for _, tier := range []string{"family", "server"} {
			if err := recordInvite(context.Background(), o, "newbox", tier, []string{"gpu"}, 5); err != nil {
				t.Fatal(err)
			}
		}
		out, err := run(t, "node", "ls")
		if err != nil || !strings.Contains(out, "newbox") || !strings.Contains(out, "invited, not yet joined") || strings.Contains(out, "UNREACHABLE") {
			t.Fatalf("err=%v\n%s", err, out)
		}
		show, err := run(t, "node", "show", "newbox")
		if err != nil || !strings.Contains(show, "server") || !strings.Contains(show, "pool-gpu") {
			t.Fatalf("err=%v\n%s", err, show)
		}
		if _, err := run(t, "node", "rm", "newbox"); err != nil {
			t.Fatal(err)
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
				Filesystems: []*pb.FilesystemMetrics{{Mount: "/", UsedPercent: 36.7}, {Mount: "/data", UsedPercent: 91}},
				Gpus:        []*pb.GpuMetrics{{UtilizationPercent: 10, MemoryUsedBytes: 1 << 30, MemoryTotalBytes: 8 << 30}, {UtilizationPercent: 30, MemoryUsedBytes: 1 << 30, MemoryTotalBytes: 8 << 30}},
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
		"hub*", "linux/arm64", "34%", "2x 20% 2.0GiB/16.0GiB", "22%", "91% /data", "disk:/data", "UNREACHABLE (never seen)",
		"STALE (2m old)", "windows/amd64", "pending",
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
