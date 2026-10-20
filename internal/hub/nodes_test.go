package hub

import (
	"context"
	"errors"
	"net"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"sync"
	"sync/atomic"
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
		{"ok", &pb.NodeInfo{Name: "GPU-Box", Address: "gpu-box", SshAlias: "gpu", Tier: "shared", Tags: []string{"GPU", "gpu", "cuda"}, Notes: "4090"}, nil},
		{"default tier", &pb.NodeInfo{Name: "media", Address: "10.0.0.5:7475"}, nil},
		{"duplicate", &pb.NodeInfo{Name: "gpu-box", Address: "x"}, ErrNodeExists},
		{"hub name reserved", &pb.NodeInfo{Name: "hubhost", Address: "x"}, ErrNodeExists},
		{"bad name", &pb.NodeInfo{Name: "has space", Address: "x"}, ErrInvalidNode},
		{"empty addr", &pb.NodeInfo{Name: "a1"}, ErrInvalidNode},
		{"addr injection", &pb.NodeInfo{Name: "a2", Address: "x; rm -rf /"}, ErrInvalidNode},
		{"addr with path", &pb.NodeInfo{Name: "a6", Address: "evil.example/x"}, ErrInvalidNode},
		{"addr with credentials", &pb.NodeInfo{Name: "a7", Address: "http://u@evil.example"}, ErrInvalidNode},
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
	if len(got) != 2 || got[0].Name != "gpu-box" || got[1].Name != "media" || got[1].Tier != TierShared {
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
	for _, want := range []string{"hubhost (this machine, hub)", "| gpu-box | linux/amd64 | gpu-box | gpu | shared | cuda, gpu | 4090 |", "| media | ? |"} {
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
		{"corrupt", `{"nodes": [unterminated`, true},
		{"unknown field", `{"nodes":[{"name":"a","addr":"x","bogus":1}]}`, true},
		{"invalid tier", `{"nodes":[{"name":"a","addr":"x","tier":"root"}]}`, true},
		{"duplicate", `{"nodes":[{"name":"a","addr":"x"},{"name":"a","addr":"y"}]}`, true},
		{"hub name", `{"nodes":[{"name":"hubhost","addr":"x"}]}`, true},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			dir := t.TempDir()
			if err := os.WriteFile(filepath.Join(dir, "nodes.json"), []byte(tt.content), 0o600); err != nil {
				t.Fatal(err)
			}
			_, err := OpenInventory(dir, testSelf)
			if (err != nil) != tt.wantErr {
				t.Fatalf("err = %v", err)
			}
			if tt.wantErr {
				raw, _ := os.ReadFile(filepath.Join(dir, "nodes.json"))
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

	if _, err := cl.AddNode(ctx, req(&pb.NodeInfo{Name: "peer", Address: strings.TrimPrefix(agentSrv.URL, "http://"), Tier: "care"})); err != nil {
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

	if _, err := removeAll(t, cl, &pb.RemoveNodeRequest{Name: "hubhost"}); connect.CodeOf(err) != connect.CodeFailedPrecondition {
		t.Fatalf("remove self code = %v", connect.CodeOf(err))
	}
	if _, err := removeAll(t, cl, &pb.RemoveNodeRequest{Name: "peer"}); err != nil {
		t.Fatal(err)
	}
	if _, err := cl.GetNode(ctx, connect.NewRequest(&pb.GetNodeRequest{Name: "peer"})); connect.CodeOf(err) != connect.CodeNotFound {
		t.Fatalf("get removed code = %v", connect.CodeOf(err))
	}
}

func TestInvitedNodeLifecycle(t *testing.T) {
	dir := t.TempDir()
	inv, err := OpenInventory(dir, testSelf)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := inv.Upsert(&pb.NodeInfo{Name: "box", Address: "box", Tier: "care", Tags: []string{"pool-gpu"}, Invited: true, InviteKeyId: 7, Notes: "n"}); err != nil {
		t.Fatal(err)
	}
	if _, err := inv.Upsert(&pb.NodeInfo{Name: "box", Tier: "shared", Invited: true, InviteKeyId: 8}); err != nil {
		t.Fatal(err)
	}
	got, _ := inv.Get("box")
	if got.Address != "box" || got.Tier != "shared" || got.Notes != "n" || !got.Invited || got.InviteKeyId != 8 || len(got.Tags) != 0 {
		t.Fatalf("upsert = %v", got)
	}
	reopened, err := OpenInventory(dir, testSelf)
	if err != nil {
		t.Fatal(err)
	}
	if g, _ := reopened.Get("box"); !g.Invited || g.InviteKeyId != 8 {
		t.Fatalf("invited state not persisted: %v", g)
	}
	if _, err := inv.Add(&pb.NodeInfo{Name: "box", Address: "box"}); !errors.Is(err, ErrNodeExists) {
		t.Fatalf("Add over existing = %v", err)
	}

	fleet := &fakeFleet{now: time.Unix(1_000_000, 0), down: map[string]bool{"box": true}, calls: map[string]int{}}
	var peers []Peer
	m := NewNodeMonitor(inv, testSelf, node.NewCollector(node.Options{DockerHost: "unix:///nonexistent.sock"}), NodeOptions{
		Interval: time.Minute, Now: func() time.Time { return fleet.now },
		Peers: func(context.Context) ([]Peer, error) { return peers, nil },
	})
	m.fetchRemote = fleet.fetch
	var mu sync.Mutex
	var events []string
	m.publish = func(kind, subject string, _ *structpb.Struct) {
		mu.Lock()
		defer mu.Unlock()
		if subject == "box" {
			events = append(events, kind+" "+subject)
		}
	}
	status := func() *pb.NodeStatus {
		s, err := m.Status("box")
		if err != nil {
			t.Fatal(err)
		}
		return s
	}

	for range 4 {
		fleet.now = fleet.now.Add(time.Minute)
		m.pollAll(context.Background())
	}
	if s := status(); s.Freshness != pb.NodeFreshness_NODE_FRESHNESS_INVITED || len(s.Alerts) != 0 {
		t.Fatalf("invited node = %v %v", s.Freshness, s.Alerts)
	}
	if len(events) != 0 {
		t.Fatalf("invited node raised events: %v", events)
	}

	peers = []Peer{{Name: "other", KeyID: 8}, {Name: "unrelated", KeyID: 3}}
	m.pollAll(context.Background())
	m.pollAll(context.Background())
	s := status()
	if s.Freshness != pb.NodeFreshness_NODE_FRESHNESS_INVITED || len(s.Alerts) != 1 || s.Alerts[0].Kind != pb.AlertKind_ALERT_KIND_NAME_MISMATCH || s.Alerts[0].Subject != "other" || !strings.Contains(s.Error, "other") {
		t.Fatalf("mismatch status = %v", s)
	}
	if n := slices.Index(events, "node.name_mismatch box"); n < 0 || slices.Index(events[n+1:], "node.name_mismatch box") >= 0 {
		t.Fatalf("mismatch events = %v, want exactly one", events)
	}

	fleet.down["box"] = false
	m.pollAll(context.Background())
	if s := status(); s.Freshness != pb.NodeFreshness_NODE_FRESHNESS_FRESH || s.Node.Invited || len(s.Alerts) != 1 {
		t.Fatalf("joined node = %v invited=%v alerts=%v", s.Freshness, s.Node.Invited, s.Alerts)
	}
	if g, _ := inv.Get("box"); g.Invited || g.InviteKeyId != 0 {
		t.Fatalf("invited not cleared: %v", g)
	}
	if !slices.Contains(events, "node.joined box") || !slices.Contains(events, "node.metrics box") {
		t.Fatalf("events = %v", events)
	}
}

// tunnel forwards to target and can silently kill connections the way a
// vanished tailnet peer does: no FIN, no RST, bytes just stop arriving.
type tunnel struct {
	ln     net.Listener
	target string
	mu     sync.Mutex
	down   bool
	conns  []*tunnelConn
}

type tunnelConn struct{ dead atomic.Bool }

func newTunnel(t *testing.T, target string) *tunnel {
	t.Helper()
	ln, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	tn := &tunnel{ln: ln, target: target}
	go func() {
		for {
			c, err := ln.Accept()
			if err != nil {
				return
			}
			go tn.serve(c)
		}
	}()
	t.Cleanup(func() { ln.Close() })
	return tn
}

func (tn *tunnel) serve(c net.Conn) {
	defer c.Close()
	tc := &tunnelConn{}
	tn.mu.Lock()
	tc.dead.Store(tn.down)
	tn.conns = append(tn.conns, tc)
	tn.mu.Unlock()
	up, err := net.Dial("tcp", tn.target)
	if err != nil {
		return
	}
	defer up.Close()
	pipe := func(dst, src net.Conn) {
		buf := make([]byte, 4096)
		for {
			n, err := src.Read(buf)
			if n > 0 && !tc.dead.Load() {
				dst.Write(buf[:n])
			}
			if err != nil {
				return
			}
		}
	}
	go pipe(up, c)
	pipe(c, up)
}

// Break blackholes every open connection; with down set, new ones too.
func (tn *tunnel) Break(down bool) {
	tn.mu.Lock()
	defer tn.mu.Unlock()
	tn.down = down
	for _, c := range tn.conns {
		c.dead.Store(true)
	}
}

func TestPollRecoversFromDeadPooledConnection(t *testing.T) {
	agent := &node.Agent{
		Collector: node.NewCollector(node.Options{DockerHost: "unix:///nonexistent.sock"}),
		Authorize: node.TokenAuthorizer("tok"),
	}
	agentSrv := httptest.NewServer(agent.Handler())
	defer agentSrv.Close()
	tn := newTunnel(t, agentSrv.Listener.Addr().String())

	inv, err := OpenInventory(t.TempDir(), testSelf)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := inv.Add(&pb.NodeInfo{Name: "box", Address: "http://" + tn.ln.Addr().String()}); err != nil {
		t.Fatal(err)
	}
	const fetchTimeout = 2 * time.Second
	m := NewNodeMonitor(inv, testSelf, node.NewCollector(node.Options{DockerHost: "unix:///nonexistent.sock"}),
		NodeOptions{Interval: time.Hour, FetchTimeout: fetchTimeout, Token: "tok", Client: &http.Client{Transport: &http.Transport{}}})
	status := func() pb.NodeFreshness {
		s, err := m.Status("box")
		if err != nil {
			t.Fatal(err)
		}
		return s.Freshness
	}
	ctx := context.Background()
	box, _ := inv.Get("box")

	m.pollOne(ctx, box)
	if got := status(); got != pb.NodeFreshness_NODE_FRESHNESS_FRESH {
		t.Fatalf("first poll = %v", got)
	}

	tn.Break(false)
	start := time.Now()
	m.pollOne(ctx, box)
	if got := status(); got != pb.NodeFreshness_NODE_FRESHNESS_FRESH {
		t.Fatalf("poll over a dead pooled connection = %v", got)
	}
	if d := time.Since(start); d > fetchTimeout {
		t.Fatalf("recovery took %s, over the %s fetch budget", d, fetchTimeout)
	}
}

func TestPollAllKeepsNodeAddedMidRound(t *testing.T) {
	inv, err := OpenInventory(t.TempDir(), testSelf)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := inv.Add(&pb.NodeInfo{Name: "first", Address: "first"}); err != nil {
		t.Fatal(err)
	}
	m := NewNodeMonitor(inv, testSelf, node.NewCollector(node.Options{DockerHost: "unix:///nonexistent.sock"}),
		NodeOptions{Interval: time.Hour})
	m.fetchRemote = func(ctx context.Context, n *pb.NodeInfo) (*pb.NodeMetrics, error) {
		if n.Name == "first" {
			late, err := inv.Add(&pb.NodeInfo{Name: "late", Address: "late"})
			if err != nil {
				t.Error(err)
				return &pb.NodeMetrics{}, nil
			}
			m.pollOne(ctx, late)
		}
		return &pb.NodeMetrics{}, nil
	}
	m.pollAll(context.Background())
	s, err := m.Status("late")
	if err != nil {
		t.Fatal(err)
	}
	if s.Freshness != pb.NodeFreshness_NODE_FRESHNESS_FRESH {
		t.Fatalf("node added mid-round lost its first poll: %v, %q", s.Freshness, s.Error)
	}
}

func TestValidNodeAddr(t *testing.T) {
	tests := []struct {
		addr string
		ok   bool
	}{
		{"gpu-box", true},
		{"gpu-box.tail1234.ts.net", true},
		{"10.0.0.5:7475", true},
		{"[fd7a:115c::1]:7475", true},
		{"fd7a:115c::1", false},
		{"http://127.0.0.1:8080", true},
		{"https://box.example.com", true},
		{"", false},
		{"ftp://box", false},
		{"http://", false},
		{"evil.com/steal", false},
		{"http://evil.com/", false},
		{"http://good@evil.com", false},
		{"evil.com@good", false},
		{"box?x=1", false},
		{"box#x", false},
		{"box:0", false},
		{"box:99999", false},
		{"box:abc", false},
		{"bo x", false},
		{"x; rm -rf /", false},
		{"-box", false},
		{"a..b", false},
		{"//evil.com", false},
	}
	for _, tt := range tests {
		t.Run(tt.addr, func(t *testing.T) {
			if got := validNodeAddr(tt.addr); got != tt.ok {
				t.Fatalf("validNodeAddr(%q) = %v, want %v", tt.addr, got, tt.ok)
			}
		})
	}
}

func TestNodeTokenNotSentOnRedirect(t *testing.T) {
	var leaked atomic.Int32
	other := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		leaked.Add(1)
	}))
	defer other.Close()
	redirector := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		http.Redirect(w, r, other.URL+r.URL.Path, http.StatusTemporaryRedirect)
	}))
	defer redirector.Close()

	m := NewNodeMonitor(nil, LocalNodeInfo("hub", "linux", "amd64"), nil, NodeOptions{Token: "secret", FetchTimeout: time.Second})
	_, err := m.fetchHTTP(context.Background(), &pb.NodeInfo{Name: "n", Address: redirector.URL})
	if err == nil {
		t.Fatal("redirected poll succeeded")
	}
	if leaked.Load() != 0 {
		t.Fatalf("redirect was followed %d times", leaked.Load())
	}
}

func TestMigrateTiers(t *testing.T) {
	const old = `{"nodes":[{"name":"kidsbox","addr":"kidsbox","tier":"family"},{"name":"agentworld","addr":"agentworld","tier":"server"},{"name":"newer","addr":"newer","tier":"shared"}]}`
	tests := []struct {
		name        string
		content     string
		backup      bool
		wantChanges []TierChange
		wantErr     string
	}{
		{"renames and keeps a copy", old, false, []TierChange{{"kidsbox", "family", "care"}, {"agentworld", "server", "shared"}}, ""},
		{"already migrated", `{"nodes":[{"name":"a","addr":"a","tier":"care"}]}`, false, nil, ""},
		{"no inventory", "", false, nil, ""},
		{"never overwrites an earlier copy", old, true, nil, "keep a copy"},
		{"corrupt file", `{"nodes": [`, false, nil, "parse"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			dir := t.TempDir()
			path := filepath.Join(dir, "nodes.json")
			if tt.content != "" {
				writeFile(t, path, []byte(tt.content), 0o600)
				if _, err := OpenInventory(dir, testSelf); tt.wantChanges != nil && (!errors.Is(err, ErrRetiredTiers) || !strings.Contains(err.Error(), "protectorate node migrate-tiers")) {
					t.Fatalf("open before migrating: %v", err)
				}
			}
			if tt.backup {
				writeFile(t, path+".pre-tiers", []byte("earlier"), 0o600)
			}
			got, err := MigrateTiers(dir, testSelf)
			if tt.wantErr != "" {
				if err == nil || !strings.Contains(err.Error(), tt.wantErr) {
					t.Fatalf("err = %v, want %q", err, tt.wantErr)
				}
				if b, _ := os.ReadFile(path); string(b) != tt.content {
					t.Fatalf("a failed migration changed nodes.json:\n%s", b)
				}
				return
			}
			if err != nil || !slices.Equal(got, tt.wantChanges) {
				t.Fatalf("changes %v %v, want %v", got, err, tt.wantChanges)
			}
			if tt.wantChanges == nil {
				return
			}
			if b, _ := os.ReadFile(path + ".pre-tiers"); string(b) != tt.content {
				t.Fatalf("backup %q", b)
			}
			inv, err := OpenInventory(dir, testSelf)
			if err != nil {
				t.Fatal(err)
			}
			tiers := map[string]string{}
			for _, n := range inv.List() {
				tiers[n.Name] = n.Tier
			}
			if tiers["kidsbox"] != TierCare || tiers["agentworld"] != TierShared || tiers["newer"] != TierShared {
				t.Fatalf("tiers %v", tiers)
			}
			if again, err := MigrateTiers(dir, testSelf); err != nil || again != nil {
				t.Fatalf("second run %v %v", again, err)
			}
		})
	}
}
