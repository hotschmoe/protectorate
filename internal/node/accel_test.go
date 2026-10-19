package node

import (
	"bufio"
	"context"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"google.golang.org/protobuf/proto"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
)

// sysfsFixture builds a sysfs tree from testdata/sysfs/<name>.txt, whose
// lines are "path = contents", "path -> symlink target" or "dir/".
func sysfsFixture(t *testing.T, name string) string {
	t.Helper()
	root := t.TempDir()
	f, err := os.Open(filepath.Join("testdata", "sysfs", name+".txt"))
	if err != nil {
		t.Fatal(err)
	}
	defer f.Close()
	sc := bufio.NewScanner(f)
	for sc.Scan() {
		line := sc.Text()
		if line == "" || strings.HasPrefix(line, "#") {
			continue
		}
		if path, target, ok := strings.Cut(line, " -> "); ok {
			p := filepath.Join(root, path)
			must(t, os.MkdirAll(filepath.Dir(p), 0o755))
			must(t, os.Symlink(target, p))
			continue
		}
		if path, contents, ok := strings.Cut(line, " = "); ok {
			p := filepath.Join(root, path)
			must(t, os.MkdirAll(filepath.Dir(p), 0o755))
			must(t, os.WriteFile(p, []byte(contents+"\n"), 0o644))
			continue
		}
		must(t, os.MkdirAll(filepath.Join(root, line), 0o755))
	}
	must(t, sc.Err())
	return root
}

func must(t *testing.T, err error) {
	t.Helper()
	if err != nil {
		t.Fatal(err)
	}
}

const gib = 1 << 30

func TestSysfsInventory(t *testing.T) {
	b70 := "devices/pci0000:00/0000:00:03.1/0000:09:00.0/0000:0a:01.0/0000:0b:00.0"
	b70b := "devices/pci0000:40/0000:40:03.1/0000:42:00.0/0000:43:01.0/0000:44:00.0"
	tests := []struct {
		fixture string
		// next moves the cumulative counters on by two seconds' worth.
		next      map[string]string
		wantGPUs  []*pb.GpuMetrics
		wantNPUs  []*pb.AcceleratorMetrics
		wantVRAMQ []string
	}{
		{
			fixture: "ms-r1",
			wantGPUs: []*pb.GpuMetrics{{
				Vendor: "arm", Name: "Mali-G720-Immortalis 10 cores", Driver: "mali r53p0-00eac0", BusId: "CIXH5000:00",
				Integrated: true, UtilizationPercent: -1, TemperatureCelsius: -1,
			}},
			wantNPUs: []*pb.AcceleratorMetrics{{
				Kind: "npu", Vendor: "armchina", Name: "Zhouyi NPU", Driver: "armchina", BusId: "CIXH4000:00",
				UtilizationPercent: -1, FrequencyHz: 1200000000,
			}},
		},
		{
			fixture: "kidsbox",
			wantGPUs: []*pb.GpuMetrics{{
				Vendor: "arm", Name: "Mali Valhall", Driver: "panthor", BusId: "15000000.gpu",
				Integrated: true, UtilizationPercent: -1, TemperatureCelsius: -1,
			}},
			wantNPUs: []*pb.AcceleratorMetrics{{
				Kind: "npu", Vendor: "armchina", Name: "Zhouyi NPU", Driver: "armchina", BusId: "14260000.aipu",
				UtilizationPercent: -1, FrequencyHz: 1200000000,
			}},
		},
		{
			fixture: "b70-nas-server",
			next: map[string]string{
				b70 + "/tile0/gt0/gtidle/idle_residency_ms":  "1212841",
				b70b + "/tile0/gt0/gtidle/idle_residency_ms": "1182002",
			},
			wantGPUs: []*pb.GpuMetrics{
				{
					Vendor: "intel", Name: "Battlemage G31 [Intel Graphics]", Driver: "xe", BusId: "0000:0b:00.0",
					UtilizationPercent: 25, MemoryTotalBytes: 32 * gib, TemperatureCelsius: 55,
				},
				{
					Vendor: "intel", Name: "Battlemage G31 [Intel Graphics]", Driver: "xe", BusId: "0000:44:00.0",
					UtilizationPercent: 0, MemoryTotalBytes: 32 * gib, TemperatureCelsius: 51,
				},
			},
			wantVRAMQ: []string{"xe renderD128", "xe renderD129"},
		},
		{
			fixture: "intel-mixed",
			next: map[string]string{
				"devices/pci0000:00/0000:00:02.0/drm/card0/gt/gt0/rc6_residency_ms":               "2000",
				"devices/pci0000:00/0000:00:01.0/0000:03:00.0/tile0/gt0/gtidle/idle_residency_ms": "2500",
				"devices/pci0000:00/0000:00:01.1/0000:04:00.0/tile0/gt0/gtidle/idle_residency_ms": "1200",
				"devices/pci0000:00/0000:00:0b.0/npu_busy_time_us":                                "5500000",
			},
			wantGPUs: []*pb.GpuMetrics{
				{
					Vendor: "intel", Name: "Raptor Lake-S GT1 [UHD Graphics 770]", Driver: "i915", BusId: "0000:00:02.0",
					Integrated: true, UtilizationPercent: 50, TemperatureCelsius: -1,
				},
				{
					Vendor: "intel", Name: "DG2 [Arc A770]", Driver: "xe", BusId: "0000:03:00.0",
					UtilizationPercent: 0, MemoryTotalBytes: 32 * gib, TemperatureCelsius: 47,
				},
				{
					Vendor: "intel", Name: "DG2 [Arc A770]", Driver: "xe", BusId: "0000:04:00.0",
					UtilizationPercent: 75, MemoryTotalBytes: 32 * gib, TemperatureCelsius: -1,
				},
				{
					Vendor: "amd", Name: "Navi 31 [Radeon RX 7900 XT/7900 XTX]", Driver: "amdgpu", BusId: "0000:06:00.0",
					UtilizationPercent: 37, MemoryUsedBytes: proto.Uint64(1048576), MemoryTotalBytes: 25753026560, TemperatureCelsius: 44,
				},
			},
			wantNPUs: []*pb.AcceleratorMetrics{{
				Kind: "npu", Vendor: "intel", Name: "Meteor Lake NPU", Driver: "intel_vpu", BusId: "0000:00:0b.0",
				UtilizationPercent: 25, FrequencyHz: 1400000000,
			}},
			wantVRAMQ: []string{"xe renderD129", "xe renderD130"},
		},
	}
	for _, tt := range tests {
		t.Run(tt.fixture, func(t *testing.T) {
			root := sysfsFixture(t, tt.fixture)
			var queried []string
			p := newSysfsProbe(root, func(driver, render string) uint64 {
				queried = append(queried, driver+" "+render)
				return 32 * gib
			})
			p.pciIDs = []string{filepath.Join("testdata", "missing.ids"), filepath.Join("testdata", "pci.ids")}
			at := time.Unix(1000, 0)
			p.now = func() time.Time { return at }
			ctx := context.Background()
			devs := p.inventory(ctx)
			first, _ := p.sample(ctx, devs)
			for _, g := range first {
				if g.UtilizationPercent >= 0 && len(tt.next) > 0 && g.Driver != "amdgpu" {
					t.Errorf("%s: a cumulative counter needs two readings, got %v on the first", g.BusId, g.UtilizationPercent)
				}
			}
			for path, v := range tt.next {
				must(t, os.WriteFile(filepath.Join(root, path), []byte(v+"\n"), 0o644))
			}
			at = at.Add(2 * time.Second)
			gpus, npus := p.sample(ctx, devs)
			if len(gpus) != len(tt.wantGPUs) {
				t.Fatalf("gpus = %v, want %v", gpus, tt.wantGPUs)
			}
			for i := range gpus {
				if !proto.Equal(gpus[i], tt.wantGPUs[i]) {
					t.Errorf("gpu %d = %v\nwant %v", i, gpus[i], tt.wantGPUs[i])
				}
			}
			if len(npus) != len(tt.wantNPUs) {
				t.Fatalf("npus = %v, want %v", npus, tt.wantNPUs)
			}
			for i := range npus {
				if !proto.Equal(npus[i], tt.wantNPUs[i]) {
					t.Errorf("npu %d = %v\nwant %v", i, npus[i], tt.wantNPUs[i])
				}
			}
			if strings.Join(queried, ",") != strings.Join(tt.wantVRAMQ, ",") {
				t.Errorf("vram queries = %v, want %v", queried, tt.wantVRAMQ)
			}
			if again, _ := p.sample(ctx, devs); len(again) > 0 && again[0] == gpus[0] {
				t.Error("samples must be fresh messages")
			}
		})
	}
}

func TestSysfsEmpty(t *testing.T) {
	p := newSysfsProbe(t.TempDir(), nil)
	if devs := p.inventory(context.Background()); len(devs) != 0 {
		t.Fatalf("a VM without display devices (agentworld) reports %v", devs)
	}
}

// Records from TestListDevicesWindows on snapdragon-laptop (Windows 11 Home
// 28000, Snapdragon X2 Elite Extreme X2E94100) and officepc (Windows 11 Pro
// 26300, Core Ultra 7 270K Plus with an Arc Pro B50).
var (
	winAdreno  = winDevice{instanceID: `ACPI\VEN_QCOM&DEV_0F36&SUBSYS_16D41043&REV_0056\0`, desc: "Qualcomm(R) Adreno(TM) X2-90 GPU", mfg: "Qualcomm Incorporated", service: "QCDX", enumerator: "ACPI", bus: 0, address: -1, driverVersion: "32.0.172.1", luid: "0x00000000_0x00011bed", adapterType: 0x232b}
	winHexagon = winDevice{npu: true, instanceID: `ACPI\VEN_QCOM&DEV_0FF0&SUBSYS_CRD08480&REV_0056\2&DABA3FF&0`, desc: "Snapdragon(R) X2 Elite Extreme - X2E94100 - Qualcomm(R) Hexagon(TM) NPU", mfg: "Qualcomm Technologies, Inc.", service: "qcnspmcdm", enumerator: "ACPI", bus: 0, address: -1, driverVersion: "30.0.228.10000", luid: "0x00000000_0x00011f2f", adapterType: 0x901}
	winParsec  = winDevice{instanceID: `ROOT\DISPLAY\0000`, desc: "Parsec Virtual Display Adapter", mfg: "Parsec Cloud, Inc.", service: "WUDFRd", enumerator: "ROOT", bus: -1, address: -1, driverVersion: "0.45.0.0", luid: "0x00000000_0x000176bc", adapterType: 0x342}
	winIntelGr = winDevice{instanceID: `PCI\VEN_8086&DEV_7D67&SUBSYS_7D671849&REV_06\3&11583659&0&10`, desc: "Intel(R) Graphics", mfg: "Intel Corporation", service: "igfxn", enumerator: "PCI", bus: 0, address: 131072, driverVersion: "32.0.101.8805", memBytes: 0x7ffff000, luid: "0x00000000_0x0000d2c2", adapterType: 0x2323}
	winB50     = winDevice{instanceID: `PCI\VEN_8086&DEV_E212&SUBSYS_11148086&REV_00\6&379BCAE1&0&00080030`, desc: "Intel(R) Arc(TM) Pro B50 Graphics", mfg: "Intel Corporation", service: "igfxnd", enumerator: "PCI", bus: 4, address: 0, driverVersion: "32.0.101.8805", memBytes: 0x3f4e00000, luid: "0x00000000_0x0000c668", adapterType: 0x231b}
	winAIBoost = winDevice{npu: true, instanceID: `PCI\VEN_8086&DEV_AD1D&SUBSYS_AD1D1849&REV_01\3&11583659&0&58`, desc: "Intel(R) AI Boost", mfg: "Intel Corporation", service: "npu", enumerator: "PCI", bus: 0, address: 720896, driverVersion: "32.0.100.4778", luid: "0x00000000_0x0000d885", adapterType: 0x2901}
)

func TestWinAccels(t *testing.T) {
	tests := []struct {
		name      string
		devs      []winDevice
		engines   map[string]float64
		dedicated map[string]float64
		wantGPUs  []*pb.GpuMetrics
		wantNPUs  []*pb.AcceleratorMetrics
	}{
		{
			name: "snapdragon-laptop",
			devs: []winDevice{winAdreno, winHexagon},
			engines: map[string]float64{
				"pid_4_luid_0x00000000_0x00011bed_phys_0_eng_0_engtype_3d":         12,
				"pid_3588_luid_0x00000000_0x00011bed_phys_0_eng_0_engtype_3d":      20,
				"pid_3588_luid_0x00000000_0x00011bed_phys_0_eng_4_engtype_compute": 25,
				"pid_4_luid_0x00000000_0x00011f08_phys_0_eng_0_engtype_3d":         90,
				"pid_812_luid_0x00000000_0x00011f2f_phys_0_eng_0_engtype_compute":  7.5,
			},
			dedicated: map[string]float64{"luid_0x00000000_0x00011bed_phys_0": 0},
			wantGPUs: []*pb.GpuMetrics{{
				Vendor: "qualcomm", Name: "Qualcomm Adreno X2-90 GPU", Driver: "QCDX 32.0.172.1", BusId: winAdreno.instanceID,
				Integrated: true, UtilizationPercent: 32, TemperatureCelsius: -1,
			}},
			wantNPUs: []*pb.AcceleratorMetrics{{
				Kind: "npu", Vendor: "qualcomm", Name: "Snapdragon X2 Elite Extreme - X2E94100 - Qualcomm Hexagon NPU",
				Driver: "qcnspmcdm 30.0.228.10000", BusId: winHexagon.instanceID, UtilizationPercent: 7.5,
			}},
		},
		{
			name: "officepc",
			devs: []winDevice{winParsec, winIntelGr, winB50, winAIBoost},
			engines: map[string]float64{
				"pid_14260_luid_0x00000000_0x0000c668_phys_0_eng_0_engtype_3d": 0.47,
				"pid_26524_luid_0x00000000_0x0000c668_phys_0_eng_12_engtype_":  0,
				"pid_26524_luid_0x00000000_0x0000d2c2_phys_0_eng_0_engtype_3d": 3,
			},
			dedicated: map[string]float64{
				"luid_0x00000000_0x0000c668_phys_0": 3404103680,
				"luid_0x00000000_0x0000d2c2_phys_0": 0,
				"luid_0x00000000_0x0000d885_phys_0": 0,
			},
			wantGPUs: []*pb.GpuMetrics{
				{
					Vendor: "intel", Name: "Intel Graphics", Driver: "igfxn 32.0.101.8805", BusId: "0000:00:02.0",
					Integrated: true, UtilizationPercent: 3, TemperatureCelsius: -1,
				},
				{
					Vendor: "intel", Name: "Intel Arc Pro B50 Graphics", Driver: "igfxnd 32.0.101.8805", BusId: "0000:04:00.0",
					UtilizationPercent: 0.47, MemoryUsedBytes: proto.Uint64(3404103680), MemoryTotalBytes: 16993222656, TemperatureCelsius: -1,
				},
			},
			wantNPUs: []*pb.AcceleratorMetrics{{
				Kind: "npu", Vendor: "intel", Name: "Intel AI Boost", Driver: "npu 32.0.100.4778", BusId: "0000:00:0b.0", UtilizationPercent: 0,
			}},
		},
		{
			name: "first reading has no utilization",
			devs: []winDevice{winB50, {npu: true, instanceID: `PCI\VEN_8086&DEV_AD1D\x`, desc: "Intel(R) AI Boost", enumerator: "PCI", bus: 0, address: 720896, service: "npu"}},
			wantGPUs: []*pb.GpuMetrics{{
				Vendor: "intel", Name: "Intel Arc Pro B50 Graphics", Driver: "igfxnd 32.0.101.8805", BusId: "0000:04:00.0",
				UtilizationPercent: -1, MemoryTotalBytes: 16993222656, TemperatureCelsius: -1,
			}},
			wantNPUs: []*pb.AcceleratorMetrics{{Kind: "npu", Vendor: "intel", Name: "Intel AI Boost", Driver: "npu", BusId: "0000:00:0b.0", UtilizationPercent: -1}},
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			var util map[string]float64
			if tt.engines != nil {
				util = adapterUtilization(tt.engines)
			}
			gpus, npus := winSample(winAccels(tt.devs), util, adapterDedicated(tt.dedicated))
			if len(gpus) != len(tt.wantGPUs) || len(npus) != len(tt.wantNPUs) {
				t.Fatalf("got %v %v\nwant %v %v", gpus, npus, tt.wantGPUs, tt.wantNPUs)
			}
			for i := range gpus {
				if !proto.Equal(gpus[i], tt.wantGPUs[i]) {
					t.Errorf("gpu %d = %v\nwant %v", i, gpus[i], tt.wantGPUs[i])
				}
			}
			for i := range npus {
				if !proto.Equal(npus[i], tt.wantNPUs[i]) {
					t.Errorf("npu %d = %v\nwant %v", i, npus[i], tt.wantNPUs[i])
				}
			}
		})
	}
}

func TestMergeGPUs(t *testing.T) {
	nv := &pb.GpuMetrics{Index: 0, Vendor: "nvidia", Name: "NVIDIA GeForce RTX 4090"}
	inv := []*pb.GpuMetrics{
		{Vendor: "intel", Name: "Intel Graphics", Integrated: true, BusId: "0000:00:02.0"},
		{Vendor: "nvidia", Name: "nvidia GPU [10de:2684]", BusId: "0000:01:00.0"},
		{Vendor: "intel", Name: "Intel Arc Pro B50 Graphics", BusId: "0000:04:00.0"},
		{Vendor: "intel", Name: "Intel Arc Pro B50 Graphics", BusId: "0000:03:00.0"},
	}
	got := mergeGPUs([]*pb.GpuMetrics{nv}, inv)
	want := []string{"0 NVIDIA GeForce RTX 4090", "1 0000:03:00.0", "2 0000:04:00.0", "3 0000:00:02.0"}
	if len(got) != len(want) {
		t.Fatalf("got %v", got)
	}
	for i, g := range got {
		label := g.BusId
		if label == "" {
			label = g.Name
		}
		if s := fmt.Sprintf("%d %s", g.Index, label); s != want[i] {
			t.Errorf("gpu %d = %q, want %q", i, s, want[i])
		}
	}
}

func TestParseXPUSMI(t *testing.T) {
	ids, bdf := parseXPUDiscovery([]byte(`{"device_list": [
		{"device_id": 0, "device_name": "Intel(R) Arc(TM) Pro B70", "pci_bdf_address": "0000:0B:00.0"},
		{"device_id": 1, "device_name": "Intel(R) Arc(TM) Pro B70", "pci_bdf_address": "0000:44:00.0"}]}`))
	if strings.Join(ids, ",") != "0,1" {
		t.Fatalf("ids = %v", ids)
	}
	got := parseXPUDump([]byte("Timestamp, DeviceId, GPU Utilization (%), GPU Memory Used (MiB)\n06:14:46.000,    0, 41.50, 20480\n06:14:46.000,    1, N/A, N/A\n"), bdf)
	if r := got["0000:0b:00.0"]; r.util != 41.5 || r.memUsed == nil || *r.memUsed != 20480*mib {
		t.Errorf("card 0 = %+v", r)
	}
	if r := got["0000:44:00.0"]; r.util != -1 || r.memUsed != nil {
		t.Errorf("card 1 = %+v", r)
	}
	if parseXPUDump([]byte("garbage"), bdf) != nil {
		t.Error("garbage must yield nothing")
	}
}
