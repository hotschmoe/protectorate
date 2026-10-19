package node

import (
	"context"
	"encoding/json"
	"sort"
	"strconv"
	"strings"
	"time"

	"google.golang.org/protobuf/proto"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
)

// accelDevice is one GPU or NPU found by the OS inventory. Exactly one of gpu
// and npu is set; they hold the static fields and are never handed out, only
// clones of them.
type accelDevice struct {
	gpu *pb.GpuMetrics
	npu *pb.AcceleratorMetrics
	// key is how the sampler finds the device again: the adapter LUID on
	// Windows ("0x00000000_0x0000c668", as PDH prints it).
	key string
	// sysfs sampling sources on Linux; empty when the driver has none.
	busyPercentFile string   // amdgpu gpu_busy_percent
	idleFiles       []string // cumulative idle residency, milliseconds
	busyUsFile      string   // cumulative busy time, microseconds
	loadFile        string   // devfreq load, "NN@FREQHz"
	freqFile        string   // devfreq cur_freq, Hz
	freqMHzFile     string
	tempFile        string // hwmon millidegrees
	vramUsedFile    string
}

// accelProbe finds the GPUs and NPUs the OS knows about (inventory, run on
// the slow interval) and reads their live figures (sample, every Collect).
type accelProbe interface {
	inventory(ctx context.Context) []accelDevice
	sample(ctx context.Context, devs []accelDevice) ([]*pb.GpuMetrics, []*pb.AcceleratorMetrics)
}

// accelMetrics returns the node's GPUs and NPUs. It runs under collectMu.
func (c *Collector) accelMetrics(ctx context.Context) ([]*pb.GpuMetrics, []*pb.AcceleratorMetrics) {
	tools := gpuMetrics(ctx)
	if c.accel == nil {
		return tools, nil
	}
	if c.accelAt.IsZero() || time.Since(c.accelAt) >= c.opts.SlowInterval {
		c.accelDevs, c.accelAt = c.accel.inventory(ctx), time.Now()
	}
	gpus, npus := c.accel.sample(ctx, c.accelDevs)
	applyXPUSMI(ctx, gpus)
	return mergeGPUs(tools, gpus), npus
}

// mergeGPUs puts the vendor tools' GPUs first, keeping their indices, then
// the OS inventory's, discrete before integrated. An inventory GPU of a
// vendor whose tool reported is the same card seen with fewer figures, so it
// is dropped.
func mergeGPUs(tools, inv []*pb.GpuMetrics) []*pb.GpuMetrics {
	byTool := map[string]bool{}
	for _, g := range tools {
		byTool[g.Vendor] = true
	}
	out := append([]*pb.GpuMetrics(nil), tools...)
	var rest []*pb.GpuMetrics
	for _, g := range inv {
		if !byTool[g.Vendor] {
			rest = append(rest, g)
		}
	}
	sort.SliceStable(rest, func(i, j int) bool {
		if rest[i].Integrated != rest[j].Integrated {
			return !rest[i].Integrated
		}
		return rest[i].BusId < rest[j].BusId
	})
	for _, g := range rest {
		g.Index = int32(len(out))
		out = append(out, g)
	}
	return out
}

func cloneGPU(g *pb.GpuMetrics) *pb.GpuMetrics { return proto.Clone(g).(*pb.GpuMetrics) }

func cloneNPU(n *pb.AcceleratorMetrics) *pb.AcceleratorMetrics {
	return proto.Clone(n).(*pb.AcceleratorMetrics)
}

var pciVendors = map[uint64]string{
	0x8086: "intel", 0x1002: "amd", 0x1022: "amd", 0x10de: "nvidia",
	0x5143: "qualcomm", 0x17cb: "qualcomm", 0x13b5: "arm", 0x106b: "apple",
}

func pciVendor(id uint64) string {
	if v, ok := pciVendors[id]; ok {
		return v
	}
	return "0x" + strconv.FormatUint(id, 16)
}

// cleanName drops trademark marks: "Intel(R) Arc(TM) Pro B50" -> "Intel Arc Pro B50".
func cleanName(s string) string {
	for _, m := range []string{"(R)", "(r)", "(TM)", "(tm)", "®", "™"} {
		s = strings.ReplaceAll(s, m, "")
	}
	return strings.Join(strings.Fields(s), " ")
}

func joinDriver(name, version string) string {
	return strings.TrimSpace(name + " " + version)
}

type xpuReading struct {
	util    float64
	memUsed *uint64
}

// applyXPUSMI fills Intel discrete GPUs' utilization and VRAM use from
// Intel's xpu-smi when it is installed, matching cards by PCI address.
func applyXPUSMI(ctx context.Context, gpus []*pb.GpuMetrics) {
	want := false
	for _, g := range gpus {
		want = want || (g.Vendor == "intel" && !g.Integrated)
	}
	if !want {
		return
	}
	raw, ok := runTool(ctx, "xpu-smi", "discovery", "-j")
	if !ok {
		return
	}
	ids, bdf := parseXPUDiscovery(raw)
	if len(ids) == 0 {
		return
	}
	dump, ok := runTool(ctx, "xpu-smi", "dump", "-d", strings.Join(ids, ","), "-m", "0,18", "-n", "1")
	if !ok {
		return
	}
	readings := parseXPUDump(dump, bdf)
	for _, g := range gpus {
		if r, ok := readings[strings.ToLower(g.BusId)]; ok && g.Vendor == "intel" {
			if r.util >= 0 {
				g.UtilizationPercent = r.util
			}
			if r.memUsed != nil {
				g.MemoryUsedBytes = r.memUsed
			}
		}
	}
}

// parseXPUDiscovery reads `xpu-smi discovery -j`: the device ids and each
// id's PCI address.
func parseXPUDiscovery(raw []byte) ([]string, map[string]string) {
	var d struct {
		DeviceList []struct {
			DeviceID int    `json:"device_id"`
			BDF      string `json:"pci_bdf_address"`
		} `json:"device_list"`
	}
	if json.Unmarshal(raw, &d) != nil {
		return nil, nil
	}
	var ids []string
	bdf := map[string]string{}
	for _, dev := range d.DeviceList {
		id := strconv.Itoa(dev.DeviceID)
		ids = append(ids, id)
		bdf[id] = strings.ToLower(dev.BDF)
	}
	return ids, bdf
}

// parseXPUDump reads `xpu-smi dump` CSV, matching columns by header name
// because their order follows the -m list.
func parseXPUDump(raw []byte, bdf map[string]string) map[string]xpuReading {
	lines := strings.Split(strings.TrimSpace(string(raw)), "\n")
	if len(lines) < 2 {
		return nil
	}
	col := map[string]int{}
	for i, h := range strings.Split(lines[0], ",") {
		col[strings.TrimSpace(h)] = i
	}
	idCol, ok := col["DeviceId"]
	if !ok {
		return nil
	}
	field := func(f []string, name string) string {
		if i, ok := col[name]; ok && i < len(f) {
			return strings.TrimSpace(f[i])
		}
		return ""
	}
	out := map[string]xpuReading{}
	for _, line := range lines[1:] {
		f := strings.Split(line, ",")
		if idCol >= len(f) {
			continue
		}
		addr, ok := bdf[strings.TrimSpace(f[idCol])]
		if !ok {
			continue
		}
		out[addr] = xpuReading{
			util:    numOr(field(f, "GPU Utilization (%)"), -1),
			memUsed: optBytes(field(f, "GPU Memory Used (MiB)"), mib),
		}
	}
	return out
}
