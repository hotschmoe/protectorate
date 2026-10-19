package node

import (
	"fmt"
	"regexp"
	"strconv"
	"strings"

	"google.golang.org/protobuf/proto"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
)

// winDevice is a present Display or ComputeAccelerator device as SetupAPI,
// its driver key and D3DKMT describe it. Kept free of syscalls so recorded
// devices can be tested on any OS.
type winDevice struct {
	npu           bool
	instanceID    string
	desc          string
	mfg           string
	service       string
	enumerator    string
	bus           int64 // -1 when absent
	address       int64 // (device << 16) | function, -1 when absent
	driverVersion string
	memBytes      uint64 // HardwareInformation.qwMemorySize
	luid          string // "0x00000000_0x0000c668", empty when no adapter opened
	adapterType   uint32 // D3DKMT_ADAPTERTYPE bits
}

const d3dkmtHybridIntegrated = 1 << 5

var winVendors = map[string]string{"QCOM": "qualcomm", "INTC": "intel", "AMDI": "amd", "NVDA": "nvidia", "MSFT": "microsoft"}

var winVenRe = regexp.MustCompile(`(?i)\\VEN_([0-9A-Z]{4})`)

func winVendor(d winDevice) string {
	if m := winVenRe.FindStringSubmatch(d.instanceID); m != nil {
		ven := strings.ToUpper(m[1])
		if v, ok := winVendors[ven]; ok {
			return v
		}
		if id, err := strconv.ParseUint(ven, 16, 64); err == nil {
			return pciVendor(id)
		}
	}
	mfg := strings.Fields(cleanName(d.mfg))
	if len(mfg) == 0 {
		return ""
	}
	return strings.ToLower(mfg[0])
}

// winAccels turns present devices into inventory entries. Software adapters
// (Parsec, remote display, indirect display drivers) are enumerated under
// ROOT or SWD and left out.
func winAccels(devs []winDevice) []accelDevice {
	var out []accelDevice
	for _, d := range devs {
		if e := strings.ToUpper(d.enumerator); e == "ROOT" || e == "SWD" {
			continue
		}
		pci := strings.EqualFold(d.enumerator, "PCI")
		busID := d.instanceID
		if pci && d.bus >= 0 && d.address >= 0 {
			busID = fmt.Sprintf("0000:%02x:%02x.%x", d.bus, d.address>>16, d.address&0xffff)
		}
		driver := joinDriver(d.service, d.driverVersion)
		if d.npu {
			out = append(out, accelDevice{key: d.luid, npu: &pb.AcceleratorMetrics{
				Kind: "npu", Vendor: winVendor(d), Name: cleanName(d.desc), Driver: driver, BusId: busID, UtilizationPercent: -1,
			}})
			continue
		}
		g := &pb.GpuMetrics{
			Vendor: winVendor(d), Name: cleanName(d.desc), Driver: driver, BusId: busID,
			UtilizationPercent: -1, TemperatureCelsius: -1,
			Integrated: !pci || d.bus == 0 || d.adapterType&d3dkmtHybridIntegrated != 0,
		}
		if !g.Integrated {
			g.MemoryTotalBytes = d.memBytes
		}
		out = append(out, accelDevice{key: d.luid, gpu: g})
	}
	return out
}

// engineInstanceRe parses a "GPU Engine" counter instance, e.g.
// pid_4_luid_0x00000000_0x00011bed_phys_0_eng_4_engtype_compute.
var engineInstanceRe = regexp.MustCompile(`^pid_\d+_luid_(0x[0-9a-fA-F]+_0x[0-9a-fA-F]+)_phys_\d+_eng_\d+_engtype_(.*)$`)

// adapterUtilization is each adapter's busiest engine type, summed over
// processes, as Task Manager shows it.
func adapterUtilization(engines map[string]float64) map[string]float64 {
	perType := map[[2]string]float64{}
	for inst, v := range engines {
		m := engineInstanceRe.FindStringSubmatch(inst)
		if m == nil {
			continue
		}
		perType[[2]string{strings.ToLower(m[1]), m[2]}] += v
	}
	out := map[string]float64{}
	for k, v := range perType {
		if cur, ok := out[k[0]]; !ok || v > cur {
			out[k[0]] = clampPercent(v)
		}
	}
	return out
}

var adapterMemoryRe = regexp.MustCompile(`^luid_(0x[0-9a-fA-F]+_0x[0-9a-fA-F]+)_phys_\d+$`)

// adapterDedicated sums "GPU Adapter Memory" Dedicated Usage per adapter.
func adapterDedicated(mem map[string]float64) map[string]uint64 {
	out := map[string]uint64{}
	for inst, v := range mem {
		if m := adapterMemoryRe.FindStringSubmatch(inst); m != nil && v >= 0 {
			out[strings.ToLower(m[1])] += uint64(v)
		}
	}
	return out
}

// winSample applies one PDH reading to the inventory; util is nil when the
// engine counters had no valid reading yet. With a valid reading, an adapter
// no process has a context on is idle.
func winSample(devs []accelDevice, util map[string]float64, dedicated map[string]uint64) ([]*pb.GpuMetrics, []*pb.AcceleratorMetrics) {
	var gpus []*pb.GpuMetrics
	var npus []*pb.AcceleratorMetrics
	for _, d := range devs {
		u := util[d.key]
		if util == nil || d.key == "" {
			u = -1
		}
		if d.gpu != nil {
			g := cloneGPU(d.gpu)
			g.UtilizationPercent = u
			if used, ok := dedicated[d.key]; ok && !g.Integrated && d.key != "" {
				g.MemoryUsedBytes = proto.Uint64(used)
			}
			gpus = append(gpus, g)
			continue
		}
		n := cloneNPU(d.npu)
		n.UtilizationPercent = u
		npus = append(npus, n)
	}
	return gpus, npus
}

func luidString(low uint32, high int32) string {
	return fmt.Sprintf("0x%08x_0x%08x", uint32(high), low)
}
