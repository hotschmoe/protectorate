package hub

import (
	"context"
	"errors"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"

	"connectrpc.com/connect"
	"google.golang.org/protobuf/types/known/structpb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
	"github.com/hotschmoe/protectorate/internal/node"
)

var testSelf = LocalNodeInfo("hubhost", "linux", "arm64")

func TestInventory(t *testing.T) {
	dir := t.TempDir()
	inv, err := OpenInventory(dir, testSelf)
	if err != nil {
		t.Fatal(err)
	}
	tests := []struct {
		name    string
		in      *pb.NodeInfo
		wantErr error
	}{
		{"ok", &pb.NodeInfo{Name: "GPU-Box", Address: "gpu-box", SshAlias: "gpu", Tier: "server", Tags: []string{"GPU", "gpu", "cuda"}, Notes: "4090"}, nil},
		{"default tier", &pb.NodeInfo{Name: "media", Address: "10.0.0.5:7475"}, nil},
		{"duplicate", &pb.NodeInfo{Name: "gpu-box", Address: "x"}, ErrNodeExists},
		{"hub name reserved", &pb.NodeInfo{Name: "hubhost", Address: "x"}, ErrNodeExists},
		{"bad name", &pb.NodeInfo{Name: "has space", Address: "x"}, ErrInvalidNode},
		{"empty addr", &pb.NodeInfo{Name: "a1"}, ErrInvalidNode},
		{"addr injection", &pb.NodeInfo{Name: "a2", Address: "x; rm -rf /"}, ErrInvalidNode},
		{"bad ssh alias", &pb.NodeInfo{Name: "a3", Address: "x", SshAlias: "-oProxyCommand=evil"}, ErrInvalidNode},
		{"bad tier", &pb.NodeInfo{Name: "a4", Address: "x", Tier: "root"}, ErrInvalidNode},
		{"bad tag", &pb.NodeInfo{Name: "a5", Address: "x", Tags: []string{"a b"}}, ErrInvalidNode},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			_, err := inv.Add(tt.in)
			if !errors.Is(err, tt.wantErr) {
				t.Fatalf("err = %v, want %v", err, tt.wantErr)
			}
		})
	}

	got := inv.List()
	if len(got) != 2 || got[0].Name != "gpu-box" || got[1].Name != "media" || got[1].Tier != TierServer {
		t.Fatalf("list = %v", got)
	}
	if strings.Join(got[0].Tags, ",") != "cuda,gpu" {
		t.Fatalf("tags = %v", got[0].Tags)
	}

	if err := inv.SetPlatform("gpu-box", "linux", "amd64"); err != nil {
		t.Fatal(err)
	}

	reopened, err := OpenInventory(dir, testSelf)
	if err != nil {
		t.Fatal(err)
	}
	n, err := reopened.Get("GPU-BOX")
	if err != nil || n.Os != "linux" || n.Arch != "amd64" || n.SshAlias != "gpu" || n.Notes != "4090" {
		t.Fatalf("reloaded = %v, %v", n, err)
	}

	hosts, err := os.ReadFile(filepath.Join(dir, "hosts.md"))
	if err != nil {
		t.Fatal(err)
	}
	for _, want := range []string{"hubhost (this machine, hub)", "| gpu-box | linux/amd64 | gpu-box | gpu | server | cuda, gpu | 4090 |", "| media | ? |"} {
		if !strings.Contains(string(hosts), want) {
			t.Errorf("hosts.md missing %q:\n%s", want, hosts)
		}
	}

	if err := inv.Remove("media"); err != nil {
		t.Fatal(err)
	}
	if err := inv.Remove("media"); !errors.Is(err, ErrNodeNotFound) {
		t.Fatalf("second remove err = %v", err)
	}
	hosts, _ = os.ReadFile(filepath.Join(dir, "hosts.md"))
	if strings.Contains(string(hosts), "media") {
		t.Fatalf("hosts.md still lists media:\n%s", hosts)
	}
	entries, _ := os.ReadDir(dir)
	for _, e := range entries {
		if strings.HasPrefix(e.Name(), ".") {
			t.Errorf("leftover temp file %s", e.Name())
		}
	}
}

func TestInventoryLoadErrors(t *testing.T) {
	tests := []struct {
		name, content string
		wantErr       bool
	}{
		{"empty file", "", false},
		{"corrupt", "nodes: [unterminated", true},
		{"unknown field", "nodes:\n  - name: a\n    addr: x\n    bogus: 1\n", true},
		{"invalid tier", "nodes:\n  - name: a\n    addr: x\n    tier: root\n", true},
		{"duplicate", "nodes:\n  - {name: a, addr: x}\n  - {name: a, addr: y}\n", true},
		{"hub name", "nodes:\n  - {name: hubhost, addr: x}\n", true},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			dir := t.TempDir()
			if err := os.WriteFile(filepath.Join(dir, "nodes.yaml"), []byte(tt.content), 0o600); err != nil {
				t.Fatal(err)
			}
			_, err := OpenInventory(dir, testSelf)
			if (err != nil) != tt.wantErr {
				t.Fatalf("err = %v", err)
			}
			if tt.wantErr {
				raw, _ := os.ReadFile(filepath.Join(dir, "nodes.yaml"))
				if string(raw) != tt.content {
					t.Fatal("a rejected file must not be rewritten")
				}
			}
		})
	}
}

func TestAgentURL(t *testing.T) {
	tests := []struct{ in, want string }{
		{"gpu-box", "http://gpu-box:7475"},
		{"gpu-box:9000", "http://gpu-box:9000"},
		{"10.0.0.5", "http://10.0.0.5:7475"},
		{"[fd7a::1]", "http://[fd7a::1]:7475"},
		{"[fd7a::1]:80", "http://[fd7a::1]:80"},
		{"https://n.example/", "https://n.example"},
	}
	for _, tt := range tests {
		if got := AgentURL(tt.in); got != tt.want {
			t.Errorf("AgentURL(%q) = %q, want %q", tt.in, got, tt.want)
		}
	}
}

func TestAlerts(t *testing.T) {
	mk := func(fresh pb.NodeFreshness, mem float64, disks ...float64) *pb.NodeStatus {
		m := &pb.NodeMetrics{Memory: &pb.MemoryMetrics{UsedPercent: mem}}
		for i, d := range disks {
			m.Filesystems = append(m.Filesystems, &pb.FilesystemMetrics{Mount: string(rune('a' + i)), UsedPercent: d})
		}
		return &pb.NodeStatus{Freshness: fresh, Metrics: m, Error: "refused"}
	}
	tests := []struct {
		name string
		in   *pb.NodeStatus
		want []pb.AlertKind
	}{
		{"healthy", mk(pb.NodeFreshness_NODE_FRESHNESS_FRESH, 50, 40, 60), nil},
		{"disk at threshold", mk(pb.NodeFreshness_NODE_FRESHNESS_FRESH, 50, 90), []pb.AlertKind{pb.AlertKind_ALERT_KIND_DISK_PERCENT}},
		{"two disks and mem", mk(pb.NodeFreshness_NODE_FRESHNESS_FRESH, 95, 91, 99), []pb.AlertKind{pb.AlertKind_ALERT_KIND_DISK_PERCENT, pb.AlertKind_ALERT_KIND_DISK_PERCENT, pb.AlertKind_ALERT_KIND_MEM_PRESSURE}},
		{"unreachable", mk(pb.NodeFreshness_NODE_FRESHNESS_UNREACHABLE, 10, 10), []pb.AlertKind{pb.AlertKind_ALERT_KIND_UNREACHABLE}},
		{"unreachable never seen", &pb.NodeStatus{Freshness: pb.NodeFreshness_NODE_FRESHNESS_UNREACHABLE}, []pb.AlertKind{pb.AlertKind_ALERT_KIND_UNREACHABLE}},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := alertsFor(tt.in, 90, 90)
			if len(got) != len(tt.want) {
				t.Fatalf("alerts = %v, want kinds %v", got, tt.want)
			}
			for i, w := range tt.want {
				if got[i].Kind != w {
					t.Errorf("alert %d = %v, want %v", i, got[i].Kind, w)
				}
			}
		})
	}
}

type fakeFleet struct {
	mu    sync.Mutex
	now   time.Time
	down  map[string]bool
	calls map[string]int
}

func (f *fakeFleet) fetch(_ context.Context, n *pb.NodeInfo) (*pb.NodeMetrics, error) {
	f.mu.Lock()
	f.calls[n.Name]++
	down := f.down[n.Name]
	f.mu.Unlock()
	if down {
		return nil, errors.New("connection refused")
	}
	return &pb.NodeMetrics{Os: "windows", Arch: "amd64", Cpu: &pb.CpuMetrics{UsagePercent: 12}, Memory: &pb.MemoryMetrics{UsedPercent: 40},
		Filesystems: []*pb.FilesystemMetrics{{Mount: "C:", UsedPercent: 95}}}, nil
}

func TestMonitorFreshness(t *testing.T) {
	inv, err := OpenInventory(t.TempDir(), testSelf)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := inv.Add(&pb.NodeInfo{Name: "win", Address: "win"}); err != nil {
		t.Fatal(err)
	}
	fleet := &fakeFleet{now: time.Unix(1_000_000, 0), down: map[string]bool{}, calls: map[string]int{}}
	m := NewNodeMonitor(inv, testSelf, node.NewCollector(node.Options{DockerHost: "unix:///nonexistent.sock"}),
		NodeOptions{Interval: time.Minute, Now: func() time.Time { return fleet.now }})
	m.fetchRemote = fleet.fetch
	type ev struct{ kind, subject string }
	var events []ev
	var evMu sync.Mutex
	m.publish = func(kind, subject string, _ *structpb.Struct) {
		evMu.Lock()
		defer evMu.Unlock()
		events = append(events, ev{kind, subject})
	}

	status := func() *pb.NodeStatus {
		s, err := m.Status("win")
		if err != nil {
			t.Fatal(err)
		}
		return s
	}
	step := func(d time.Duration) { fleet.now = fleet.now.Add(d); m.pollAll(context.Background()) }

	if s := status(); s.Freshness != pb.NodeFreshness_NODE_FRESHNESS_UNSPECIFIED {
		t.Fatalf("before first poll = %v", s.Freshness)
	}
	m.pollAll(context.Background())
	if s := status(); s.Freshness != pb.NodeFreshness_NODE_FRESHNESS_FRESH || s.Metrics == nil || len(s.Alerts) != 1 {
		t.Fatalf("after good poll = %v", s)
	}
	if n, _ := inv.Get("win"); n.Os != "windows" || n.Arch != "amd64" {
		t.Fatalf("platform not learned: %v", n)
	}

	fleet.down["win"] = true
	step(time.Minute)
	if s := status(); s.Freshness != pb.NodeFreshness_NODE_FRESHNESS_STALE || s.Metrics == nil || s.Error == "" {
		t.Fatalf("one failure = %v", s.Freshness)
	}
	step(time.Minute)
	if s := status(); s.Freshness != pb.NodeFreshness_NODE_FRESHNESS_STALE {
		t.Fatalf("at stale limit = %v", s.Freshness)
	}
	step(time.Minute)
	s := status()
	if s.Freshness != pb.NodeFreshness_NODE_FRESHNESS_UNREACHABLE || s.Metrics == nil {
		t.Fatalf("past stale limit = %v (cached metrics must be kept)", s.Freshness)
	}
	step(time.Minute)

	count := func(kind string) int {
		n := 0
		for _, e := range events {
			if e.kind == kind && e.subject == "win" {
				n++
			}
		}
		return n
	}
	if count("node.unreachable") != 1 {
		t.Fatalf("unreachable events = %d, want exactly 1 per outage", count("node.unreachable"))
	}

	fleet.down["win"] = false
	step(time.Minute)
	if s := status(); s.Freshness != pb.NodeFreshness_NODE_FRESHNESS_FRESH {
		t.Fatalf("after recovery = %v", s.Freshness)
	}
	fleet.down["win"] = true
	for range 4 {
		step(time.Minute)
	}
	if count("node.unreachable") != 2 {
		t.Fatalf("second outage events = %d, want 2 total", count("node.unreachable"))
	}
	if count("node.metrics") != 2 {
		t.Fatalf("metrics events = %d, want 2", count("node.metrics"))
	}

	local, err := m.Status("HUBHOST")
	if err != nil || local.Freshness != pb.NodeFreshness_NODE_FRESHNESS_FRESH || !local.Node.Local || local.Metrics.Hostname == "" {
		t.Fatalf("local node = %v, %v", local, err)
	}
	if fleet.calls["hubhost"] != 0 {
		t.Fatal("local node must not go through the network fetch")
	}

	if err := inv.Remove("win"); err != nil {
		t.Fatal(err)
	}
	m.pollAll(context.Background())
	if _, err := m.Status("win"); !errors.Is(err, ErrNodeNotFound) {
		t.Fatalf("removed node err = %v", err)
	}
	m.mu.Lock()
	_, kept := m.state["win"]
	m.mu.Unlock()
	if kept {
		t.Fatal("state of a removed node must be pruned")
	}
}

func TestNodeServiceEndToEnd(t *testing.T) {
	agent := &node.Agent{
		Collector: node.NewCollector(node.Options{DockerHost: "unix:///nonexistent.sock"}),
		Authorize: node.TokenAuthorizer("agent-secret"),
	}
	agentSrv := httptest.NewServer(agent.Handler())
	defer agentSrv.Close()

	inv, err := OpenInventory(t.TempDir(), testSelf)
	if err != nil {
		t.Fatal(err)
	}
	h := New("test", testToken)
	h.SetNodes(NewNodeMonitor(inv, testSelf, node.NewCollector(node.Options{DockerHost: "unix:///nonexistent.sock"}),
		NodeOptions{Interval: time.Hour, Token: "agent-secret"}))
	srv := httptest.NewServer(h.Handler())
	defer srv.Close()
	ctx, cancel := context.WithCancel(context.Background())
	done := make(chan struct{})
	go func() { defer close(done); h.nodes.Run(ctx) }()
	defer func() { cancel(); <-done }()

	cl := protectoratev1connect.NewNodeServiceClient(&http.Client{Transport: bearer{testToken}}, srv.URL)
	sctx, scancel := context.WithTimeout(ctx, 10*time.Second)
	defer scancel()
	sub, err := protectoratev1connect.NewEventServiceClient(&http.Client{Transport: bearer{testToken}}, srv.URL).
		Subscribe(sctx, connect.NewRequest(&pb.SubscribeRequest{}))
	if err != nil {
		t.Fatal(err)
	}
	req := func(n *pb.NodeInfo) *connect.Request[pb.AddNodeRequest] {
		return connect.NewRequest(&pb.AddNodeRequest{Node: n})
	}

	if _, err := cl.AddNode(ctx, req(&pb.NodeInfo{Name: "peer", Address: strings.TrimPrefix(agentSrv.URL, "http://"), Tier: "family"})); err != nil {
		t.Fatal(err)
	}
	if _, err := cl.AddNode(ctx, req(&pb.NodeInfo{Name: "peer", Address: "x"})); connect.CodeOf(err) != connect.CodeAlreadyExists {
		t.Fatalf("duplicate add code = %v", connect.CodeOf(err))
	}
	if _, err := cl.AddNode(ctx, req(&pb.NodeInfo{Name: "BAD NAME", Address: "x"})); connect.CodeOf(err) != connect.CodeInvalidArgument {
		t.Fatalf("invalid add code = %v", connect.CodeOf(err))
	}

	var peer *pb.NodeStatus
	deadline := time.Now().Add(10 * time.Second)
	for time.Now().Before(deadline) {
		res, err := cl.GetNode(ctx, connect.NewRequest(&pb.GetNodeRequest{Name: "peer"}))
		if err != nil {
			t.Fatal(err)
		}
		if res.Msg.Freshness == pb.NodeFreshness_NODE_FRESHNESS_FRESH {
			peer = res.Msg
			break
		}
		time.Sleep(50 * time.Millisecond)
	}
	if peer == nil || peer.Metrics.GetMemory() == nil || peer.Node.Os == "" {
		t.Fatalf("peer never became fresh: %v", peer)
	}

	var sawEvent bool
	for sub.Receive() {
		if e := sub.Msg(); e.Kind == "node.metrics" && e.Subject == "peer" {
			sawEvent = e.Payload.Fields["mem_percent"] != nil && e.Payload.Fields["freshness"].GetStringValue() == "fresh"
			break
		}
	}
	if !sawEvent {
		t.Fatalf("no node.metrics event for peer: %v", sub.Err())
	}

	list, err := cl.ListNodes(ctx, connect.NewRequest(&pb.ListNodesRequest{}))
	if err != nil || len(list.Msg.Nodes) != 2 || !list.Msg.Nodes[0].Node.Local || list.Msg.Nodes[1].Node.Name != "peer" {
		t.Fatalf("list = %v, %v", list, err)
	}

	if _, err := cl.RemoveNode(ctx, connect.NewRequest(&pb.RemoveNodeRequest{Name: "hubhost"})); connect.CodeOf(err) != connect.CodeFailedPrecondition {
		t.Fatalf("remove self code = %v", connect.CodeOf(err))
	}
	if _, err := cl.RemoveNode(ctx, connect.NewRequest(&pb.RemoveNodeRequest{Name: "peer"})); err != nil {
		t.Fatal(err)
	}
	if _, err := cl.GetNode(ctx, connect.NewRequest(&pb.GetNodeRequest{Name: "peer"})); connect.CodeOf(err) != connect.CodeNotFound {
		t.Fatalf("get removed code = %v", connect.CodeOf(err))
	}
}
