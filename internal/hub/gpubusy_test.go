package hub

import (
	"context"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"connectrpc.com/connect"
	"google.golang.org/protobuf/proto"
	"google.golang.org/protobuf/types/known/durationpb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/internal/auth"
	"github.com/hotschmoe/protectorate/internal/node"
)

func b70(idx int32, util, temp float64, used *uint64) *pb.GpuMetrics {
	return &pb.GpuMetrics{Index: idx, Vendor: "intel", Name: "Battlemage G31 [Intel Graphics]", UtilizationPercent: util, TemperatureCelsius: temp,
		MemoryUsedBytes: used, MemoryTotalBytes: 32 * gib}
}

func TestBusyGPUs(t *testing.T) {
	t0 := time.Date(2026, 10, 20, 9, 0, 0, 0, time.UTC)
	every := func(n int, gap time.Duration, g func(i int) *pb.GpuMetrics) []gpuSample {
		var h []gpuSample
		for i := range n {
			h = addGPUSample(h, t0.Add(time.Duration(i)*gap), []*pb.GpuMetrics{g(i)})
		}
		return h
	}
	tests := []struct {
		name string
		hist []gpuSample
		want string
	}{
		{"most vram in use", every(1, 0, func(int) *pb.GpuMetrics { return b70(0, 1, 40, proto.Uint64(30*gib)) }), "94% of its VRAM is in use"},
		{"some vram in use", every(1, 0, func(int) *pb.GpuMetrics { return b70(0, 1, 40, proto.Uint64(8*gib)) }), ""},
		{"sustained load, vram unknown", every(6, 30*time.Second, func(i int) *pb.GpuMetrics { return b70(0, 90-float64(i), 60, nil) }), "utilization at least 85% for 2m30s"},
		{"load too short to judge", every(3, 30*time.Second, func(int) *pb.GpuMetrics { return b70(0, 99, 60, nil) }), ""},
		{"load with a dip", every(6, 30*time.Second, func(i int) *pb.GpuMetrics { return b70(0, map[bool]float64{true: 10, false: 95}[i == 3], 60, nil) }), ""},
		{"sustained heat", every(6, 30*time.Second, func(int) *pb.GpuMetrics { return b70(0, 20, 81, nil) }), "at 81C or more for 2m30s"},
		{"unknown figures", every(6, 30*time.Second, func(int) *pb.GpuMetrics { return b70(0, -1, -1, nil) }), ""},
		{"old samples age out", every(12, 30*time.Second, func(i int) *pb.GpuMetrics { return b70(0, map[bool]float64{true: 95, false: 5}[i < 6], 40, nil) }), ""},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			now := tt.hist[len(tt.hist)-1].at
			got := busyGPUs(tt.hist, now)
			if tt.want == "" {
				if len(got) != 0 {
					t.Fatalf("busy %v", got)
				}
				return
			}
			if len(got) != 1 || got[0].Reason != tt.want {
				t.Fatalf("busy %v, want %q", got, tt.want)
			}
		})
	}
	if h := every(40, 10*time.Second, func(int) *pb.GpuMetrics { return b70(0, 1, 1, nil) }); len(h) > gpuHistory || h[len(h)-1].at.Sub(h[0].at) > busyWindow+10*time.Second {
		t.Fatalf("history not bounded: %d samples", len(h))
	}
}

func TestNodeFitsSkipsBusyGPUs(t *testing.T) {
	none := func(string, *int) *pb.Lease { return nil }
	fresh := pb.NodeFreshness_NODE_FRESHNESS_FRESH
	llm := status("llm", TierShared, fresh, "linux", "amd64", 5, 30, 32, 64*gib, b70(0, 0, 40, nil), b70(1, 0, 40, nil))
	llm.BusyGpus = []*pb.GpuBusy{{Index: 0, Reason: "utilization at least 90% for 3m0s"}}
	both := proto.Clone(llm).(*pb.NodeStatus)
	both.BusyGpus = append(both.BusyGpus, &pb.GpuBusy{Index: 1, Reason: "94% of its VRAM is in use"})
	desk := status("desk", TierShared, fresh, "windows", "amd64", 5, 30, 8, 16*gib, &pb.GpuMetrics{Index: 0, Vendor: "intel", Name: "Intel Graphics", Integrated: true})
	desk.BusyGpus = []*pb.GpuBusy{{Index: 0, Reason: "utilization at least 70% for 3m0s"}}
	tests := []struct {
		name    string
		st      *pb.NodeStatus
		sel     *pb.NodeSelector
		wantGPU int
		why     string
	}{
		{"cpu work skips a node with a busy gpu", llm, &pb.NodeSelector{}, -1, "gpu 0 busy: utilization"},
		{"a busy integrated gpu does not", desk, &pb.NodeSelector{Os: "windows"}, -1, ""},
		{"vram lease takes the free gpu", llm, &pb.NodeSelector{Gpu: "battlemage", GpuVramBytes: 24 * gib}, 1, ""},
		{"vram lease, every gpu busy", both, &pb.NodeSelector{GpuVramBytes: 24 * gib}, -1, "no free gpu with enough vram (gpu 1 busy: 94%"},
		{"gpu by name, one free", llm, &pb.NodeSelector{Gpu: "battlemage"}, -1, ""},
		{"gpu by name, all busy", both, &pb.NodeSelector{Gpu: "battlemage"}, -1, "busy"},
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

// TestBusyGPURefusals drives the hub with a node whose first GPU is held by
// a model server: named requests are refused with whom to ask, selectors
// pass the node by, an admin job still runs.
func TestBusyGPURefusals(t *testing.T) {
	home := t.TempDir()
	t.Setenv("PROTECTORATE_HOME", home)
	inv, err := OpenInventory(t.TempDir(), testSelf)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := inv.Add(&pb.NodeInfo{Name: "llm", Address: "llm"}); err != nil {
		t.Fatal(err)
	}
	m := NewNodeMonitor(inv, testSelf, node.NewCollector(node.Options{DockerHost: "unix:///nonexistent.sock"}), NodeOptions{Interval: time.Hour})
	m.fetchRemote = func(context.Context, *pb.NodeInfo) (*pb.NodeMetrics, error) {
		return &pb.NodeMetrics{Os: "linux", Arch: "amd64", Cpu: &pb.CpuMetrics{LogicalCores: 32}, Memory: &pb.MemoryMetrics{AvailableBytes: 32 * gib},
			Gpus:   []*pb.GpuMetrics{b70(0, 2, 45, proto.Uint64(30*gib)), b70(1, 0, 40, proto.Uint64(gib))},
			Engine: &pb.ContainerEngine{Containers: []*pb.ContainerMetrics{{Name: "llama-server", State: "running"}}},
			Jobs:   &pb.JobIsolation{Sandbox: "bwrap", Admin: "sudo"}}, nil
	}
	h := New("test", testToken)
	h.SetNodes(m)
	local, err := node.NewJobs(filepath.Join(t.TempDir(), "local"), 1)
	if err != nil {
		t.Fatal(err)
	}
	local.Probe = func() *pb.JobIsolation { return &pb.JobIsolation{Sandbox: "none"} }
	if err := h.EnableJobs(home, local); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(h.jobs.Shutdown)
	info, _ := inv.Get("llm")
	m.pollOne(context.Background(), info)
	m.pollOne(context.Background(), testSelf)

	ctx := context.Background()
	lease := func(target *pb.AcquireLeaseRequest) error {
		target.Holder, target.Ttl = "agent", durationpb.New(time.Minute)
		_, err := h.AcquireLease(ctx, connect.NewRequest(target))
		return err
	}
	tests := []struct {
		name string
		call func() error
		want string
	}{
		{"named gpu lease", func() error {
			return lease(&pb.AcquireLeaseRequest{Target: &pb.AcquireLeaseRequest_Resource{Resource: "llm:gpu0"}})
		}, "llm gpu 0 is busy: 94% of its VRAM is in use (running containers: llama-server); freeing it is a person's call, ask: protectorate msg send orchestrator"},
		{"named node lease", func() error {
			return lease(&pb.AcquireLeaseRequest{Target: &pb.AcquireLeaseRequest_Resource{Resource: "llm"}})
		}, "llm gpu 0 is busy"},
		{"named job", func() error {
			_, err := h.jobs.Submit(ctx, &pb.SubmitJobRequest{Target: onNode("llm"), Command: []string{"true"}})
			return err
		}, "llm gpu 0 is busy"},
		{"admin job is maintenance, not gpu work", func() error {
			owner := auth.WithPrincipal(ctx, &auth.Principal{Owner: true})
			_, err := h.jobs.Submit(owner, &pb.SubmitJobRequest{Target: onNode("llm"), Command: []string{"true"}, Admin: true})
			return err
		}, ""},
		{"free gpu lease", func() error {
			return lease(&pb.AcquireLeaseRequest{Target: &pb.AcquireLeaseRequest_Resource{Resource: "llm:gpu1"}})
		}, ""},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := tt.call()
			if tt.want == "" {
				if err != nil {
					t.Fatal(err)
				}
				return
			}
			if connect.CodeOf(err) != connect.CodeFailedPrecondition || !strings.Contains(err.Error(), tt.want) {
				t.Fatalf("err = %v, want %q", err, tt.want)
			}
		})
	}
	sel, err := pickNodeFor(h, &pb.NodeSelector{})
	if err != nil || sel != testSelf.Name {
		t.Fatalf("selector placed on %q (%v), want the hub's node", sel, err)
	}
}

func pickNodeFor(h *Hub, sel *pb.NodeSelector) (string, error) {
	n, _, err := pickNode(h.nodes.Statuses(), sel, h.jobs.busy, func(string, *int) *pb.Lease { return nil }, nil)
	return n, err
}
