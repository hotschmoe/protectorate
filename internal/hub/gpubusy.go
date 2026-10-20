package hub

import (
	"fmt"
	"slices"
	"strings"
	"time"

	"connectrpc.com/connect"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
)

// A GPU is busy when something outside protectorate's leases holds it,
// judged only from what the node reports:
//
//   - memory: more than half its VRAM is in use. An idle loaded model still
//     holds VRAM, so this is the main signal where the driver reports use.
//   - load: utilization of at least busyUtil in every poll of the last
//     busyWindow (at least busyMinSamples polls spanning busySpan).
//   - heat: at least busyTemp in every poll of the same window. It covers
//     drivers that report neither VRAM use nor a trustworthy busy figure.
//
// Intel xe on Linux (b70-nas-server's Arc Pro B70s) does not tell an
// unprivileged reader how much VRAM is in use, so there only load and heat
// count: a model that is loaded but idle looks free. Installing xpu-smi on
// such a node makes the memory signal work; until then the owner keeps the
// GPU with a long lease for the model server.
const (
	busyVRAMFraction = 0.5
	busyUtil         = 60.0
	busyTemp         = 75.0
	busyWindow       = 3 * time.Minute
	busySpan         = 2 * time.Minute
	busyMinSamples   = 3
	// gpuHistory bounds the samples kept per node.
	gpuHistory = 32
)

type gpuSample struct {
	at   time.Time
	gpus []*pb.GpuMetrics
}

// addGPUSample records a poll's GPU figures, dropping samples older than the
// window.
func addGPUSample(hist []gpuSample, at time.Time, gpus []*pb.GpuMetrics) []gpuSample {
	hist = append(hist, gpuSample{at: at, gpus: gpus})
	cut := 0
	for cut < len(hist)-1 && at.Sub(hist[cut].at) > busyWindow {
		cut++
	}
	hist = hist[cut:]
	if len(hist) > gpuHistory {
		hist = hist[len(hist)-gpuHistory:]
	}
	return slices.Clip(hist)
}

// busyGPUs judges every GPU of the newest sample.
func busyGPUs(hist []gpuSample, now time.Time) []*pb.GpuBusy {
	if len(hist) == 0 {
		return nil
	}
	var out []*pb.GpuBusy
	for _, g := range hist[len(hist)-1].gpus {
		if why := gpuBusyReason(g, hist, now); why != "" {
			out = append(out, &pb.GpuBusy{Index: g.Index, Reason: why})
		}
	}
	return out
}

func gpuBusyReason(g *pb.GpuMetrics, hist []gpuSample, now time.Time) string {
	if g.MemoryUsedBytes != nil && g.MemoryTotalBytes > 0 {
		if f := float64(*g.MemoryUsedBytes) / float64(g.MemoryTotalBytes); f > busyVRAMFraction {
			return fmt.Sprintf("%.0f%% of its VRAM is in use", f*100)
		}
	}
	var window []*pb.GpuMetrics
	var oldest time.Time
	for _, s := range hist {
		if now.Sub(s.at) > busyWindow {
			continue
		}
		for _, x := range s.gpus {
			if x.Index == g.Index {
				window = append(window, x)
				if oldest.IsZero() {
					oldest = s.at
				}
			}
		}
	}
	if len(window) < busyMinSamples || now.Sub(oldest) < busySpan {
		return ""
	}
	span := now.Sub(oldest).Round(time.Second)
	if lo, ok := sustained(window, func(x *pb.GpuMetrics) float64 { return x.UtilizationPercent }, busyUtil); ok {
		return fmt.Sprintf("utilization at least %.0f%% for %s", lo, span)
	}
	if lo, ok := sustained(window, func(x *pb.GpuMetrics) float64 { return x.TemperatureCelsius }, busyTemp); ok {
		return fmt.Sprintf("at %.0fC or more for %s", lo, span)
	}
	return ""
}

// sustained reports whether every sample's figure is known and at least
// floor, with the lowest.
func sustained(xs []*pb.GpuMetrics, f func(*pb.GpuMetrics) float64, floor float64) (float64, bool) {
	lo := f(xs[0])
	for _, x := range xs {
		v := f(x)
		if v < 0 || v < floor {
			return 0, false
		}
		lo = min(lo, v)
	}
	return lo, true
}

func busyReason(st *pb.NodeStatus, idx int) string {
	for _, b := range st.BusyGpus {
		if int(b.Index) == idx {
			return b.Reason
		}
	}
	return ""
}

// busyDiscrete returns the first busy discrete GPU of a node, if any. An
// integrated GPU drives a desktop and does not keep work off the node.
func busyDiscrete(st *pb.NodeStatus) (int, string) {
	for _, g := range st.Metrics.GetGpus() {
		if g.Integrated {
			continue
		}
		if why := busyReason(st, int(g.Index)); why != "" {
			return int(g.Index), why
		}
	}
	return -1, ""
}

// gpuFree refuses an explicit request for a node (gpu nil: any of its
// discrete GPUs) or one GPU that something else is using. Freeing it is a
// person's decision, so the error says whom to ask.
func (m *NodeMonitor) gpuFree(name string, gpu *int) error {
	st, err := m.Status(name)
	if err != nil {
		return nodeErr(err)
	}
	idx, why := -1, ""
	if gpu != nil {
		idx, why = *gpu, busyReason(st, *gpu)
	} else {
		idx, why = busyDiscrete(st)
	}
	if why == "" {
		return nil
	}
	return connect.NewError(connect.CodeFailedPrecondition, fmt.Errorf("node %s gpu %d is busy: %s%s; freeing it is a person's call, ask: protectorate msg send orchestrator \"please free %s gpu %d for ...\" (or msg send owner)",
		name, idx, why, holders(st), name, idx))
}

// holders names the node's running containers, the usual holders of a GPU.
func holders(st *pb.NodeStatus) string {
	var names []string
	for _, c := range st.Metrics.GetEngine().GetContainers() {
		if c.State == "running" && len(names) < 3 {
			names = append(names, c.Name)
		}
	}
	if len(names) == 0 {
		return ""
	}
	return " (running containers: " + strings.Join(names, ", ") + ")"
}
