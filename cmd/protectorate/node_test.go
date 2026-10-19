package main

import (
	"bytes"
	"context"
	"net"
	"strings"
	"testing"
	"time"

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
	if err := attachNodes(h, time.Hour, "nt", nil); err != nil {
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
