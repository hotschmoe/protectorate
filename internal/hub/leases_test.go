package hub

import (
	"context"
	"net/http"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"sync"
	"testing"
	"time"

	"connectrpc.com/connect"
	"google.golang.org/protobuf/proto"
	"google.golang.org/protobuf/types/known/durationpb"
	"google.golang.org/protobuf/types/known/structpb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
	"github.com/hotschmoe/protectorate/internal/jobtest"
)

type recorded struct{ kind, subject string }

func newTable(t *testing.T, path string) (*LeaseTable, *clock, *[]recorded) {
	t.Helper()
	var evs []recorded
	var mu sync.Mutex
	tb, err := OpenLeases(path, func(kind, subject string, _ *structpb.Struct) {
		mu.Lock()
		defer mu.Unlock()
		evs = append(evs, recorded{kind, subject})
	})
	if err != nil {
		t.Fatal(err)
	}
	c := &clock{t: t0}
	tb.now = c.now
	return tb, c, &evs
}

func fixed(node string, gpu *int) func(func(string, *int) *pb.Lease) (string, *int, error) {
	return func(blocked func(string, *int) *pb.Lease) (string, *int, error) {
		if l := blocked(node, gpu); l != nil {
			return "", nil, connect.NewError(connect.CodeFailedPrecondition, os.ErrExist)
		}
		return node, gpu, nil
	}
}

func intp(i int) *int { return &i }

func TestLeaseExclusivityAndExpiry(t *testing.T) {
	tb, clk, evs := newTable(t, filepath.Join(t.TempDir(), "leases.yaml"))

	a, err := tb.Acquire("alice", time.Minute, fixed("win", nil))
	if err != nil {
		t.Fatal(err)
	}
	if _, err := tb.Acquire("bob", time.Minute, fixed("win", nil)); connect.CodeOf(err) != connect.CodeFailedPrecondition {
		t.Fatalf("second holder of a node: %v", err)
	}
	if _, err := tb.Acquire("bob", time.Minute, fixed("win", intp(0))); err == nil {
		t.Fatal("a gpu of a leased node must be blocked")
	}
	g, err := tb.Acquire("carol", time.Minute, fixed("gpubox", intp(1)))
	if err != nil {
		t.Fatal(err)
	}
	if _, err := tb.Acquire("dave", time.Minute, fixed("gpubox", intp(1))); err == nil {
		t.Fatal("same gpu leased twice")
	}
	other, err := tb.Acquire("dave", time.Minute, fixed("gpubox", intp(0)))
	if err != nil || other.Resource != "gpubox:gpu0" || g.Resource != "gpubox:gpu1" {
		t.Fatalf("other gpu = %v, %v", other, err)
	}
	if _, err := tb.Acquire("erin", time.Minute, fixed("gpubox", nil)); err == nil {
		t.Fatal("whole node over leased gpus")
	}

	clk.advance(40 * time.Second)
	if _, err := tb.Renew(a.Id, 0); err != nil {
		t.Fatal(err)
	}
	clk.advance(40 * time.Second)
	if _, err := tb.Get(a.Id); err != nil {
		t.Fatalf("renewed lease expired early: %v", err)
	}
	if _, err := tb.Get(g.Id); connect.CodeOf(err) != connect.CodeNotFound {
		t.Fatalf("unrenewed lease must be gone: %v", err)
	}
	if got := tb.List(); len(got) != 1 || got[0].Id != a.Id {
		t.Fatalf("list = %v", got)
	}
	if _, err := tb.Acquire("erin", time.Minute, fixed("gpubox", nil)); err != nil {
		t.Fatalf("expired gpu leases must free the node: %v", err)
	}

	clk.advance(2 * time.Minute)
	if _, err := tb.Renew(a.Id, 0); connect.CodeOf(err) != connect.CodeNotFound {
		t.Fatalf("renew of an expired lease: %v", err)
	}
	if err := tb.Release(a.Id); connect.CodeOf(err) != connect.CodeNotFound {
		t.Fatalf("release of an expired lease: %v", err)
	}

	counts := map[string]int{}
	for _, e := range *evs {
		counts[e.kind]++
	}
	if counts["lease.acquired"] != 4 || counts["lease.renewed"] != 1 || counts["lease.expired"] != 4 {
		t.Fatalf("events = %v", counts)
	}
}

func TestLeaseRunSweepsExpired(t *testing.T) {
	tb, clk, evs := newTable(t, filepath.Join(t.TempDir(), "leases.yaml"))
	l, err := tb.Acquire("alice", time.Second, fixed("win", nil))
	if err != nil {
		t.Fatal(err)
	}
	clk.advance(2 * time.Second)
	ctx, cancel := context.WithCancel(context.Background())
	done := make(chan struct{})
	go func() { defer close(done); tb.Run(ctx) }()
	defer func() { cancel(); <-done }()
	waitUntil(t, 5*time.Second, "sweep", func() bool {
		tb.mu.Lock()
		defer tb.mu.Unlock()
		for _, e := range *evs {
			if e.kind == "lease.expired" && e.subject == l.Id {
				return true
			}
		}
		return false
	})
}

func TestLeaseValidationAndRelease(t *testing.T) {
	tb, _, evs := newTable(t, filepath.Join(t.TempDir(), "leases.yaml"))
	for _, tt := range []struct {
		name, holder string
		ttl          time.Duration
	}{{"no holder", " ", time.Minute}, {"zero ttl", "a", 0}, {"negative ttl", "a", -time.Second}, {"too long", "a", 25 * time.Hour}} {
		if _, err := tb.Acquire(tt.holder, tt.ttl, fixed("win", nil)); connect.CodeOf(err) != connect.CodeInvalidArgument {
			t.Errorf("%s: %v", tt.name, err)
		}
	}
	l, err := tb.Acquire("a", time.Minute, fixed("win", nil))
	if err != nil {
		t.Fatal(err)
	}
	if _, err := tb.Renew(l.Id, 48*time.Hour); connect.CodeOf(err) != connect.CodeInvalidArgument {
		t.Fatalf("renew with huge ttl: %v", err)
	}
	if err := tb.Release(l.Id); err != nil {
		t.Fatal(err)
	}
	if _, err := tb.Acquire("b", time.Minute, fixed("win", nil)); err != nil {
		t.Fatalf("released lease must free the node: %v", err)
	}
	if (*evs)[len(*evs)-2].kind != "lease.released" {
		t.Fatalf("events = %v", *evs)
	}
}

func TestLeasePersistence(t *testing.T) {
	path := filepath.Join(t.TempDir(), "sub", "leases.yaml")
	tb, clk, _ := newTable(t, path)
	a, err := tb.Acquire("alice", 10*time.Minute, fixed("win", nil))
	if err != nil {
		t.Fatal(err)
	}
	b, err := tb.Acquire("bob", time.Minute, fixed("gpubox", intp(2)))
	if err != nil {
		t.Fatal(err)
	}
	if fi, err := os.Stat(path); err != nil || (runtime.GOOS != "windows" && fi.Mode().Perm() != 0o600) {
		t.Fatalf("leases.yaml = %v, %v", fi, err)
	}
	entries, _ := os.ReadDir(filepath.Dir(path))
	if len(entries) != 1 {
		t.Fatalf("leftover temp files: %v", entries)
	}

	re, err := OpenLeases(path, nil)
	if err != nil {
		t.Fatal(err)
	}
	re.now = clk.now
	got := re.List()
	if len(got) != 2 || got[0].Id != b.Id || got[1].Id != a.Id {
		t.Fatalf("reloaded = %v", got)
	}
	if got[0].GpuIndex == nil || *got[0].GpuIndex != 2 || got[1].Holder != "alice" || got[1].Ttl.AsDuration() != 10*time.Minute {
		t.Fatalf("fields lost: %v", got)
	}
	if _, err := re.Acquire("eve", time.Minute, fixed("win", nil)); err == nil {
		t.Fatal("a reloaded lease must still block its node")
	}
	clk.advance(5 * time.Minute)
	if got := re.List(); len(got) != 1 || got[0].Id != a.Id {
		t.Fatalf("expiry after reload = %v", got)
	}
	raw, _ := os.ReadFile(path)
	if strings.Contains(string(raw), b.Id) {
		t.Fatal("expired lease must be removed from the file")
	}
}

func TestLeaseLoadErrors(t *testing.T) {
	for name, content := range map[string]string{
		"corrupt":       "leases: [",
		"unknown field": "leases:\n  - {id: a, node: n, ttl: 1m, bogus: 1}\n",
		"no ttl":        "leases:\n  - {id: a, node: n}\n",
		"duplicate":     "leases:\n  - {id: a, node: n, ttl: 1m}\n  - {id: a, node: m, ttl: 1m}\n",
	} {
		t.Run(name, func(t *testing.T) {
			path := filepath.Join(t.TempDir(), "leases.yaml")
			if err := os.WriteFile(path, []byte(content), 0o600); err != nil {
				t.Fatal(err)
			}
			if _, err := OpenLeases(path, nil); err == nil {
				t.Fatal("expected an error")
			}
			if raw, _ := os.ReadFile(path); string(raw) != content {
				t.Fatal("a rejected file must not be rewritten")
			}
		})
	}
	if tb, err := OpenLeases(filepath.Join(t.TempDir(), "none.yaml"), nil); err != nil || len(tb.List()) != 0 {
		t.Fatalf("missing file: %v", err)
	}
}

func status(name, tier string, fresh pb.NodeFreshness, os, arch string, cpu, mem float64, cores int32, availMem uint64, gpus ...*pb.GpuMetrics) *pb.NodeStatus {
	return &pb.NodeStatus{
		Node: &pb.NodeInfo{Name: name, Tier: tier}, Freshness: fresh,
		Metrics: &pb.NodeMetrics{
			Os: os, Arch: arch, Gpus: gpus,
			Cpu:    &pb.CpuMetrics{UsagePercent: cpu, LogicalCores: cores},
			Memory: &pb.MemoryMetrics{UsedPercent: mem, AvailableBytes: availMem},
		},
	}
}

func gpu(idx int32, used, total uint64) *pb.GpuMetrics {
	return &pb.GpuMetrics{Index: idx, MemoryUsedBytes: proto.Uint64(used), MemoryTotalBytes: total}
}

const gib = 1 << 30

func TestPickNode(t *testing.T) {
	fresh, stale := pb.NodeFreshness_NODE_FRESHNESS_FRESH, pb.NodeFreshness_NODE_FRESHNESS_STALE
	none := func(string, *int) *pb.Lease { return nil }
	noJobs := func(string) int { return 0 }
	fleet := []*pb.NodeStatus{
		status("busy-win", TierShared, fresh, "windows", "amd64", 90, 50, 8, 8*gib),
		status("idle-win", TierShared, fresh, "windows", "amd64", 10, 40, 8, 16*gib),
		status("stale-win", TierShared, stale, "windows", "amd64", 0, 0, 8, 32*gib),
		status("care-win", TierCare, fresh, "windows", "amd64", 1, 1, 8, 32*gib),
		status("arm-linux", TierShared, fresh, "linux", "arm64", 5, 20, 8, 4*gib),
		status("gpu-box", TierShared, fresh, "linux", "amd64", 50, 50, 16, 64*gib, gpu(0, 20*gib, 24*gib), gpu(1, 2*gib, 24*gib)),
	}
	tests := []struct {
		name    string
		sel     *pb.NodeSelector
		busy    func(string) int
		blocked func(string, *int) *pb.Lease
		warm    map[string]bool
		want    string
		wantGPU int
		wantErr string
	}{
		{name: "least loaded matching os", sel: &pb.NodeSelector{Os: "windows"}, want: "idle-win", wantGPU: -1},
		{name: "case insensitive", sel: &pb.NodeSelector{Os: "Windows", Arch: "AMD64"}, want: "idle-win", wantGPU: -1},
		{name: "care never picked", sel: &pb.NodeSelector{Os: "windows", MinFreeMemoryBytes: 24 * gib}, wantErr: "care-win: tier care"},
		{name: "arch filter", sel: &pb.NodeSelector{Arch: "arm64"}, want: "arm-linux", wantGPU: -1},
		{name: "min free memory", sel: &pb.NodeSelector{Os: "windows", MinFreeMemoryBytes: 12 * gib}, want: "idle-win", wantGPU: -1},
		{name: "memory too high", sel: &pb.NodeSelector{MinFreeMemoryBytes: 128 * gib}, wantErr: "no node matches"},
		{name: "no such os", sel: &pb.NodeSelector{Os: "darwin"}, wantErr: "os windows"},
		{name: "gpu picks most free vram", sel: &pb.NodeSelector{GpuVramBytes: 8 * gib}, want: "gpu-box", wantGPU: 1},
		{name: "gpu too big", sel: &pb.NodeSelector{GpuVramBytes: 40 * gib}, wantErr: "no free gpu"},
		{
			name: "leased gpu skipped", sel: &pb.NodeSelector{GpuVramBytes: 8 * gib}, want: "gpu-box", wantGPU: 0,
			blocked: func(n string, g *int) *pb.Lease {
				if n == "gpu-box" && g != nil && *g == 1 {
					return &pb.Lease{Holder: "x"}
				}
				return nil
			},
			wantErr: "no free gpu",
		},
		{
			name: "leased node skipped", sel: &pb.NodeSelector{Os: "windows"}, want: "busy-win", wantGPU: -1,
			blocked: func(n string, _ *int) *pb.Lease {
				if n == "idle-win" {
					return &pb.Lease{Holder: "alice"}
				}
				return nil
			},
		},
		{name: "warm cache outweighs a little load", sel: &pb.NodeSelector{Os: "windows"}, warm: map[string]bool{"busy-win": true}, want: "idle-win", wantGPU: -1},
		{name: "warm cache wins a near tie", sel: &pb.NodeSelector{Os: "windows"}, warm: map[string]bool{"busy-win": true}, busy: func(n string) int {
			if n == "idle-win" {
				return 2
			}
			return 0
		}, want: "busy-win", wantGPU: -1},
		{
			name: "queued jobs shift the choice", sel: &pb.NodeSelector{Os: "windows"}, want: "busy-win", wantGPU: -1,
			busy: func(n string) int {
				if n == "idle-win" {
					return 4
				}
				return 0
			},
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			busy, blocked := tt.busy, tt.blocked
			if busy == nil {
				busy = noJobs
			}
			if blocked == nil {
				blocked = none
			}
			node, g, err := pickNode(fleet, tt.sel, busy, blocked, tt.warm)
			if tt.wantErr != "" {
				if err == nil || !strings.Contains(err.Error(), tt.wantErr) || connect.CodeOf(err) != connect.CodeFailedPrecondition {
					t.Fatalf("err = %v, want %q", err, tt.wantErr)
				}
				return
			}
			if err != nil {
				t.Fatal(err)
			}
			got := -1
			if g != nil {
				got = *g
			}
			if node != tt.want || got != tt.wantGPU {
				t.Fatalf("picked %s gpu %d, want %s gpu %d", node, got, tt.want, tt.wantGPU)
			}
		})
	}
}

func TestNodeFitsAccelerators(t *testing.T) {
	none := func(string, *int) *pb.Lease { return nil }
	fresh := pb.NodeFreshness_NODE_FRESHNESS_FRESH
	snap := status("snap", TierShared, fresh, "windows", "arm64", 5, 30, 12, 16*gib,
		&pb.GpuMetrics{Index: 0, Vendor: "qualcomm", Name: "Qualcomm Adreno X2-90 GPU", Integrated: true})
	snap.Metrics.Npus = []*pb.AcceleratorMetrics{{Kind: "npu", Vendor: "qualcomm", Name: "Snapdragon X2 Elite Extreme - X2E94100 - Qualcomm Hexagon NPU"}}
	office := status("office", TierShared, fresh, "windows", "amd64", 5, 30, 20, 16*gib,
		&pb.GpuMetrics{Index: 0, Vendor: "intel", Name: "Intel Arc Pro B50 Graphics", MemoryUsedBytes: proto.Uint64(3 * gib), MemoryTotalBytes: 16 * gib},
		&pb.GpuMetrics{Index: 1, Vendor: "intel", Name: "Intel Graphics", Integrated: true})
	llm := status("llm", TierShared, fresh, "linux", "amd64", 5, 30, 32, 64*gib,
		&pb.GpuMetrics{Index: 0, Vendor: "intel", Name: "Battlemage G31 [Intel Graphics]", MemoryTotalBytes: 32 * gib},
		&pb.GpuMetrics{Index: 1, Vendor: "intel", Name: "Battlemage G31 [Intel Graphics]", MemoryTotalBytes: 32 * gib})
	tests := []struct {
		name    string
		st      *pb.NodeStatus
		sel     *pb.NodeSelector
		wantGPU int
		why     string
	}{
		{"gpu by name", snap, &pb.NodeSelector{Gpu: "adreno"}, -1, ""},
		{"gpu by vendor, any case", snap, &pb.NodeSelector{Gpu: "QUALCOMM"}, -1, ""},
		{"npu by name", snap, &pb.NodeSelector{Npu: "hexagon"}, -1, ""},
		{"no such npu", office, &pb.NodeSelector{Npu: "hexagon"}, -1, "no npu matching hexagon"},
		{"no such gpu", snap, &pb.NodeSelector{Gpu: "arc"}, -1, "no gpu matching arc"},
		{"integrated gpu has no vram to reserve", snap, &pb.NodeSelector{Gpu: "adreno", GpuVramBytes: gib}, -1, "no free gpu"},
		{"vram reserves the matching discrete gpu", office, &pb.NodeSelector{Gpu: "arc", GpuVramBytes: 8 * gib}, 0, ""},
		{"vram respects the gpu filter", office, &pb.NodeSelector{Gpu: "intel graphics", GpuVramBytes: 8 * gib}, -1, "no free gpu"},
		{"unknown use counts as free", llm, &pb.NodeSelector{Gpu: "battlemage", GpuVramBytes: 24 * gib}, 0, ""},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			g, why := nodeFits(tt.st, tt.sel, none)
			got := -1
			if g != nil {
				got = *g
			}
			if !strings.Contains(why, tt.why) || (tt.why == "" && why != "") || got != tt.wantGPU {
				t.Fatalf("nodeFits = gpu %d %q, want gpu %d %q", got, why, tt.wantGPU, tt.why)
			}
		})
	}
}

func TestParseResource(t *testing.T) {
	tests := []struct {
		in      string
		node    string
		gpu     int
		wantErr bool
	}{
		{"win1", "win1", -1, false}, {"GPU-Box:gpu1", "gpu-box", 1, false}, {"n:gpu", "", 0, true},
		{"n:cpu0", "", 0, true}, {"n:gpu-1", "", 0, true}, {"bad name", "", 0, true}, {"", "", 0, true}, {"n:gpu0:x", "", 0, true},
	}
	for _, tt := range tests {
		node, gpu, err := parseResource(tt.in)
		if (err != nil) != tt.wantErr {
			t.Errorf("%q: err = %v", tt.in, err)
			continue
		}
		got := -1
		if gpu != nil {
			got = *gpu
		}
		if !tt.wantErr && (node != tt.node || got != tt.gpu) {
			t.Errorf("%q = %s, %d", tt.in, node, got)
		}
	}
}

func TestLeaseServiceEndToEnd(t *testing.T) {
	e := newJobEnv(t, 1)
	ctx := context.Background()
	sub, err := protectoratev1connect.NewEventServiceClient(&http.Client{Transport: bearer{testToken}}, e.srv.URL).
		Subscribe(ctx, connect.NewRequest(&pb.SubscribeRequest{}))
	if err != nil {
		t.Fatal(err)
	}
	defer sub.Close()

	acquire := func(req *pb.AcquireLeaseRequest) (*pb.Lease, error) {
		res, err := e.lc.AcquireLease(ctx, connect.NewRequest(req))
		if err != nil {
			return nil, err
		}
		return res.Msg, nil
	}
	ttl := durationpb.New(time.Minute)
	l, err := acquire(&pb.AcquireLeaseRequest{Target: &pb.AcquireLeaseRequest_Selector{Selector: &pb.NodeSelector{Os: runtime.GOOS}}, Holder: "alice", Ttl: ttl})
	if err != nil {
		t.Fatal(err)
	}
	if l.Node != "peer" && l.Node != "hubhost" {
		t.Fatalf("lease = %v", l)
	}
	other := "peer"
	if l.Node == "peer" {
		other = "hubhost"
	}

	if _, err := acquire(&pb.AcquireLeaseRequest{Target: &pb.AcquireLeaseRequest_Resource{Resource: l.Node}, Holder: "bob", Ttl: ttl}); connect.CodeOf(err) != connect.CodeFailedPrecondition || !strings.Contains(err.Error(), "alice") {
		t.Fatalf("leased node: %v", err)
	}
	if _, err := e.jc.SubmitJob(ctx, connect.NewRequest(&pb.SubmitJobRequest{Target: onNode(l.Node), Command: []string{"true"}})); connect.CodeOf(err) != connect.CodeFailedPrecondition {
		t.Fatalf("job on a leased node without the lease: %v", err)
	}
	picked := e.wait(t, e.submit(t, &pb.SubmitJobRequest{Target: &pb.JobTarget{Target: &pb.JobTarget_Selector{Selector: &pb.NodeSelector{Os: runtime.GOOS}}}, Command: jobtest.Script()}).Id)
	if picked.Node != other {
		t.Fatalf("selector job ran on %s, want the unleased %s", picked.Node, other)
	}
	held := e.wait(t, e.submit(t, &pb.SubmitJobRequest{Target: &pb.JobTarget{Target: &pb.JobTarget_LeaseId{LeaseId: l.Id}}, Command: jobtest.Script("say:on lease")}).Id)
	if held.State != pb.JobState_JOB_STATE_SUCCEEDED || held.Node != l.Node {
		t.Fatalf("lease job = %v", held)
	}

	if _, err := acquire(&pb.AcquireLeaseRequest{Target: &pb.AcquireLeaseRequest_Resource{Resource: other + ":gpu99"}, Holder: "bob", Ttl: ttl}); connect.CodeOf(err) != connect.CodeNotFound {
		t.Fatalf("gpu that does not exist: %v", err)
	}
	if _, err := acquire(&pb.AcquireLeaseRequest{Target: &pb.AcquireLeaseRequest_Resource{Resource: "ghost"}, Holder: "bob", Ttl: ttl}); connect.CodeOf(err) != connect.CodeNotFound {
		t.Fatalf("unknown node: %v", err)
	}
	if _, err := acquire(&pb.AcquireLeaseRequest{Holder: "bob", Ttl: ttl}); connect.CodeOf(err) != connect.CodeInvalidArgument {
		t.Fatalf("no target: %v", err)
	}

	list, err := e.lc.ListLeases(ctx, connect.NewRequest(&pb.ListLeasesRequest{}))
	if err != nil || len(list.Msg.Leases) != 1 || list.Msg.Leases[0].Id != l.Id {
		t.Fatalf("list = %v, %v", list, err)
	}
	if _, err := e.lc.RenewLease(ctx, connect.NewRequest(&pb.RenewLeaseRequest{Id: l.Id, Ttl: durationpb.New(time.Hour)})); err != nil {
		t.Fatal(err)
	}
	if _, err := e.lc.ReleaseLease(ctx, connect.NewRequest(&pb.ReleaseLeaseRequest{Id: l.Id})); err != nil {
		t.Fatal(err)
	}
	if _, err := e.lc.ReleaseLease(ctx, connect.NewRequest(&pb.ReleaseLeaseRequest{Id: l.Id})); connect.CodeOf(err) != connect.CodeNotFound {
		t.Fatalf("double release: %v", err)
	}
	if _, err := e.jc.GetJob(ctx, connect.NewRequest(&pb.GetJobRequest{Id: held.Id})); err != nil {
		t.Fatal(err)
	}
	if _, err := e.jc.SubmitJob(ctx, connect.NewRequest(&pb.SubmitJobRequest{Target: &pb.JobTarget{Target: &pb.JobTarget_LeaseId{LeaseId: l.Id}}, Command: []string{"true"}})); connect.CodeOf(err) != connect.CodeNotFound {
		t.Fatalf("job on a released lease: %v", err)
	}

	seen := map[string]bool{}
	for sub.Receive() {
		if ev := sub.Msg(); strings.HasPrefix(ev.Kind, "lease.") && ev.Subject == l.Id {
			seen[ev.Kind] = true
			if seen["lease.released"] {
				break
			}
		}
	}
	for _, k := range []string{"lease.acquired", "lease.renewed", "lease.released"} {
		if !seen[k] {
			t.Errorf("missing %s event, saw %v", k, seen)
		}
	}
}

func TestLeasesSurviveHubRestart(t *testing.T) {
	e := newJobEnv(t, 1)
	ctx := context.Background()
	res, err := e.lc.AcquireLease(ctx, connect.NewRequest(&pb.AcquireLeaseRequest{
		Target: &pb.AcquireLeaseRequest_Resource{Resource: "peer"}, Holder: "alice", Ttl: durationpb.New(time.Hour)}))
	if err != nil {
		t.Fatal(err)
	}
	re, err := OpenLeases(filepath.Join(e.home, "leases.yaml"), nil)
	if err != nil {
		t.Fatal(err)
	}
	if got := re.List(); len(got) != 1 || got[0].Id != res.Msg.Id || got[0].Holder != "alice" {
		t.Fatalf("after restart = %v", got)
	}
}
