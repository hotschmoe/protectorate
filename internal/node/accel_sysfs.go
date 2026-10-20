package node

import (
	"bufio"
	"context"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strconv"
	"strings"
	"time"

	"google.golang.org/protobuf/proto"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
)

// sysfsProbe finds GPUs and NPUs under a Linux sysfs root: PCI display
// controllers, platform (SoC) devices bound to a known GPU or NPU driver, and
// the accel class.
type sysfsProbe struct {
	root   string
	pciIDs []string
	// vram returns a GPU's dedicated memory from its render node (e.g.
	// "renderD128"), 0 when unknown; nil skips the query.
	vram func(driver, renderNode string) uint64
	now  func() time.Time

	prev map[string]counterReading
}

type counterReading struct {
	value float64
	at    time.Time
}

func newSysfsProbe(root string, vram func(driver, renderNode string) uint64) *sysfsProbe {
	return &sysfsProbe{
		root:   root,
		pciIDs: []string{"/usr/share/hwdata/pci.ids", "/usr/share/misc/pci.ids", "/usr/share/pci.ids"},
		vram:   vram,
		now:    time.Now,
		prev:   map[string]counterReading{},
	}
}

// virtualGPUVendors make display adapters for VMs and BMCs, not GPUs.
var virtualGPUVendors = map[uint64]bool{
	0x1af4: true, 0x1234: true, 0x15ad: true, 0x1b36: true, 0x1414: true,
	0x1013: true, 0x1a03: true, 0x80ee: true, 0x102b: true,
}

type socDriver struct {
	vendor, name string
}

var socGPUDrivers = map[string]socDriver{
	"mali": {"arm", "Mali"}, "panthor": {"arm", "Mali"}, "panfrost": {"arm", "Mali"}, "lima": {"arm", "Mali"},
	"adreno": {"qualcomm", "Adreno"}, "etnaviv-gpu": {"vivante", "Vivante GPU"}, "v3d": {"broadcom", "VideoCore"},
	"powervr": {"imagination", "PowerVR"}, "asahi": {"apple", "Apple GPU"},
}

var socNPUDrivers = map[string]socDriver{
	"armchina": {"armchina", "Zhouyi NPU"}, "rknpu": {"rockchip", "RKNPU"},
}

var accelDrivers = map[string]socDriver{
	"intel_vpu": {"intel", "Intel NPU"}, "amdxdna": {"amd", "AMD NPU"}, "qaic": {"qualcomm", "Qualcomm Cloud AI"},
	"rocket": {"rockchip", "Rockchip NPU"}, "ethosu": {"arm", "Ethos-U NPU"},
}

func (p *sysfsProbe) path(parts ...string) string {
	return filepath.Join(append([]string{p.root}, parts...)...)
}

func readTrim(path string) string {
	b, err := os.ReadFile(path)
	if err != nil {
		return ""
	}
	return strings.TrimSpace(string(b))
}

func readHex(path string) (uint64, bool) {
	v, err := strconv.ParseUint(strings.TrimPrefix(readTrim(path), "0x"), 16, 64)
	return v, err == nil
}

func readNum(path string) (float64, bool) {
	if path == "" {
		return 0, false
	}
	v, err := strconv.ParseFloat(readTrim(path), 64)
	return v, err == nil
}

func linkBase(path string) string {
	t, err := os.Readlink(path)
	if err != nil {
		return ""
	}
	return filepath.Base(t)
}

// driverOf returns a device's driver and, when its module has one, the
// module version: "mali r53p0-00eac0".
func (p *sysfsProbe) driverOf(dev string) (string, string) {
	drv := linkBase(filepath.Join(dev, "driver"))
	if drv == "" {
		return "", ""
	}
	mod := linkBase(filepath.Join(dev, "driver", "module"))
	if mod == "" {
		return drv, drv
	}
	ver, _, _ := strings.Cut(readTrim(p.path("module", mod, "version")), " ")
	return drv, joinDriver(drv, ver)
}

func listDir(dir string) []string {
	ents, err := os.ReadDir(dir)
	if err != nil {
		return nil
	}
	out := make([]string, 0, len(ents))
	for _, e := range ents {
		out = append(out, e.Name())
	}
	return out
}

func (p *sysfsProbe) pciGPUs() ([]accelDevice, []pciNamed) {
	var out []accelDevice
	var pending []pciNamed

	pciDir := p.path("bus", "pci", "devices")
	for _, addr := range listDir(pciDir) {
		dev := filepath.Join(pciDir, addr)
		class, ok := readHex(filepath.Join(dev, "class"))
		if !ok || class>>16 != 0x03 {
			continue
		}
		vendor, _ := readHex(filepath.Join(dev, "vendor"))
		device, _ := readHex(filepath.Join(dev, "device"))
		if virtualGPUVendors[vendor] {
			continue
		}
		drv, driver := p.driverOf(dev)
		_, bus, _ := strings.Cut(addr, ":")
		g := &pb.GpuMetrics{
			Vendor: pciVendor(vendor), Driver: driver, BusId: addr,
			UtilizationPercent: -1, TemperatureCelsius: -1,
			Integrated: strings.HasPrefix(bus, "00:"),
		}
		d := accelDevice{gpu: g}
		p.gpuSources(&d, dev, drv)
		id := [2]uint64{vendor, device}
		pending = append(pending, pciNamed{len(out), id, g.Vendor + " GPU [" + pciKey(id) + "]"})
		out = append(out, d)
	}
	return out, pending
}

func (p *sysfsProbe) inventory(context.Context) []accelDevice {
	out, pending := p.pciGPUs()
	platform, seen := p.platformAccels()
	out = append(out, platform...)

	classDir := p.path("class", "accel")
	for _, a := range listDir(classDir) {
		dev := filepath.Join(classDir, a, "device")
		real := realPath(dev)
		if seen[real] {
			continue
		}
		seen[real] = true
		d, id := p.classAccel(dev, real)
		if id != nil {
			pending = append(pending, pciNamed{len(out), *id, d.npu.Name})
		}
		out = append(out, d)
	}

	names := map[[2]uint64]string{}
	for _, pn := range pending {
		names[pn.id] = ""
	}
	p.lookupPCINames(names)
	for _, pn := range pending {
		name := names[pn.id]
		if name == "" {
			name = pn.fallback
		}
		if g := out[pn.i].gpu; g != nil {
			g.Name = name
		} else {
			out[pn.i].npu.Name = name
		}
	}
	return out
}

func (p *sysfsProbe) classAccel(dev, real string) (accelDevice, *[2]uint64) {
	drv, driver := p.driverOf(dev)
	s, known := accelDrivers[drv]
	n := &pb.AcceleratorMetrics{Kind: "npu", Vendor: s.vendor, Name: s.name, Driver: driver, BusId: filepath.Base(real), UtilizationPercent: -1}
	vendor, isPCI := readHex(filepath.Join(dev, "vendor"))
	device, _ := readHex(filepath.Join(dev, "device"))
	if isPCI && !known {
		n.Vendor = pciVendor(vendor)
	}
	if n.Vendor == "" {
		n.Vendor = drv
	}
	if n.Name == "" {
		n.Name = n.Vendor + " NPU"
	}
	d := accelDevice{npu: n}
	if f := filepath.Join(dev, "npu_busy_time_us"); exists(f) {
		d.busyUsFile = f
	}
	if f := filepath.Join(dev, "npu_current_frequency_mhz"); exists(f) {
		d.freqMHzFile = f
	}
	p.devfreqSources(&d, dev)
	if isPCI {
		return d, &[2]uint64{vendor, device}
	}
	return d, nil
}

func (p *sysfsProbe) platformAccels() ([]accelDevice, map[string]bool) {
	var out []accelDevice
	seen := map[string]bool{}
	platDir := p.path("bus", "platform", "devices")
	for _, name := range listDir(platDir) {
		dev := filepath.Join(platDir, name)
		drv, driver := p.driverOf(dev)
		if s, ok := socGPUDrivers[drv]; ok {
			g := &pb.GpuMetrics{
				Vendor: s.vendor, Name: socGPUName(dev, s.name), Driver: driver, BusId: name,
				Integrated: true, UtilizationPercent: -1, TemperatureCelsius: -1,
			}
			d := accelDevice{gpu: g}
			p.devfreqSources(&d, dev)
			out = append(out, d)
		}
		if s, ok := socNPUDrivers[drv]; ok {
			seen[realPath(dev)] = true
			d := accelDevice{npu: &pb.AcceleratorMetrics{
				Kind: "npu", Vendor: s.vendor, Name: s.name, Driver: driver, BusId: name, UtilizationPercent: -1,
			}}
			p.devfreqSources(&d, dev)
			out = append(out, d)
		}
	}
	return out, seen
}

// pciNamed is an inventory entry whose name comes from pci.ids.
type pciNamed struct {
	i        int
	id       [2]uint64
	fallback string
}

func exists(path string) bool {
	_, err := os.Stat(path)
	return err == nil
}

func realPath(p string) string {
	r, err := filepath.EvalSymlinks(p)
	if err != nil {
		return p
	}
	return r
}

func pciKey(id [2]uint64) string {
	return fmt.Sprintf("%04x:%04x", id[0], id[1])
}

// lookupPCINames fills the wanted vendor:device names from the first pci.ids
// file found.
func (p *sysfsProbe) lookupPCINames(want map[[2]uint64]string) {
	if len(want) == 0 {
		return
	}
	vendors := map[uint64]bool{}
	for k := range want {
		vendors[k[0]] = true
	}
	for _, path := range p.pciIDs {
		f, err := os.Open(path)
		if err != nil {
			continue
		}
		var cur uint64
		in := false
		sc := bufio.NewScanner(f)
		for sc.Scan() {
			line := sc.Text()
			if line == "" || line[0] == '#' {
				continue
			}
			if line[0] != '\t' {
				id, _, _ := strings.Cut(line, " ")
				v, err := strconv.ParseUint(id, 16, 64)
				cur, in = v, err == nil && vendors[v]
				continue
			}
			if !in || strings.HasPrefix(line, "\t\t") {
				continue
			}
			id, name, ok := strings.Cut(strings.TrimPrefix(line, "\t"), "  ")
			d, err := strconv.ParseUint(id, 16, 64)
			k := [2]uint64{cur, d}
			if _, wanted := want[k]; ok && err == nil && wanted {
				want[k] = strings.TrimSpace(name)
			}
		}
		f.Close()
		return
	}
}

var maliRevision = regexp.MustCompile(`\s+r\d+p\d+.*$`)

// socGPUName names a SoC GPU: Mali's kbase driver reports the model
// ("Mali-G720-Immortalis 10 cores"); panthor and panfrost only say the
// architecture in the device tree ("arm,mali-valhall").
func socGPUName(dev, base string) string {
	if info := readTrim(filepath.Join(dev, "gpuinfo")); info != "" {
		return maliRevision.ReplaceAllString(info, "")
	}
	compat, _, _ := strings.Cut(readTrim(filepath.Join(dev, "of_node", "compatible")), "\x00")
	if _, arch, ok := strings.Cut(compat, ",mali-"); ok && arch != "" {
		return base + " " + strings.ToUpper(arch[:1]) + arch[1:]
	}
	return base
}

func (p *sysfsProbe) gpuSources(d *accelDevice, dev, drv string) {
	g := d.gpu
	switch drv {
	case "amdgpu":
		d.busyPercentFile = filepath.Join(dev, "gpu_busy_percent")
		if !g.Integrated {
			total, _ := readNum(filepath.Join(dev, "mem_info_vram_total"))
			g.MemoryTotalBytes = uint64(total)
			d.vramUsedFile = filepath.Join(dev, "mem_info_vram_used")
		}
	case "xe":
		for _, gt := range globSorted(filepath.Join(dev, "tile*", "gt*", "gtidle")) {
			if strings.HasSuffix(readTrim(filepath.Join(gt, "name")), "-rc") {
				d.idleFiles = append(d.idleFiles, filepath.Join(gt, "idle_residency_ms"))
			}
		}
	case "i915":
		d.idleFiles = globSorted(filepath.Join(dev, "drm", "card*", "gt", "gt0", "rc6_residency_ms"))
		if len(d.idleFiles) == 0 {
			d.idleFiles = globSorted(filepath.Join(dev, "drm", "card*", "power", "rc6_residency_ms"))
		}
	}
	if !g.Integrated && g.MemoryTotalBytes == 0 && p.vram != nil {
		if r := globSorted(filepath.Join(dev, "drm", "renderD*")); len(r) > 0 {
			g.MemoryTotalBytes = p.vram(drv, filepath.Base(r[0]))
		}
	}
	d.tempFile = hwmonTemp(dev)
}

// hwmonTemp picks the package or edge sensor of a PCI GPU, else its first.
func hwmonTemp(dev string) string {
	for _, h := range globSorted(filepath.Join(dev, "hwmon", "hwmon*")) {
		for _, label := range globSorted(filepath.Join(h, "temp*_label")) {
			switch readTrim(label) {
			case "pkg", "edge":
				return strings.TrimSuffix(label, "_label") + "_input"
			}
		}
		if f := filepath.Join(h, "temp1_input"); exists(f) {
			return f
		}
	}
	return ""
}

func (p *sysfsProbe) devfreqSources(d *accelDevice, dev string) {
	df := globSorted(filepath.Join(dev, "devfreq", "*"))
	if len(df) == 0 {
		return
	}
	if f := filepath.Join(df[0], "load"); exists(f) {
		d.loadFile = f
	}
	if d.freqMHzFile == "" {
		d.freqFile = filepath.Join(df[0], "cur_freq")
	}
}

func globSorted(pattern string) []string {
	m, _ := filepath.Glob(pattern)
	sort.Strings(m)
	return m
}

func (p *sysfsProbe) sample(_ context.Context, devs []accelDevice) ([]*pb.GpuMetrics, []*pb.AcceleratorMetrics) {
	now := p.now()
	var gpus []*pb.GpuMetrics
	var npus []*pb.AcceleratorMetrics
	for _, d := range devs {
		util := p.utilization(d, now)
		if d.gpu != nil {
			g := cloneGPU(d.gpu)
			g.UtilizationPercent = util
			if t, ok := readNum(d.tempFile); ok {
				g.TemperatureCelsius = t / 1000
			}
			if u, ok := readNum(d.vramUsedFile); ok {
				g.MemoryUsedBytes = proto.Uint64(uint64(u))
			}
			gpus = append(gpus, g)
			continue
		}
		n := cloneNPU(d.npu)
		n.UtilizationPercent = util
		if f, ok := readNum(d.freqMHzFile); ok {
			n.FrequencyHz = uint64(f * 1e6)
		} else if f, ok := readNum(d.freqFile); ok {
			n.FrequencyHz = uint64(f)
		}
		npus = append(npus, n)
	}
	return gpus, npus
}

// utilization reads the device's busy share, -1 when it has no source or a
// cumulative counter has no previous reading yet.
func (p *sysfsProbe) utilization(d accelDevice, now time.Time) float64 {
	if v, ok := readNum(d.busyPercentFile); ok {
		return clampPercent(v)
	}
	if d.loadFile != "" {
		load, _, _ := strings.Cut(readTrim(d.loadFile), "@")
		if v, err := strconv.ParseFloat(load, 64); err == nil {
			return clampPercent(v)
		}
	}
	if len(d.idleFiles) > 0 {
		var sum float64
		for _, f := range d.idleFiles {
			v, ok := readNum(f)
			if !ok {
				return -1
			}
			sum += v
		}
		idle := p.rate(d.idleFiles[0], sum/float64(len(d.idleFiles)), now, time.Millisecond)
		if idle < 0 {
			return -1
		}
		return clampPercent(100 - idle)
	}
	if v, ok := readNum(d.busyUsFile); ok {
		busy := p.rate(d.busyUsFile, v, now, time.Microsecond)
		if busy < 0 {
			return -1
		}
		return clampPercent(busy)
	}
	return -1
}

// rate turns a cumulative time counter into the percentage of wall time it
// advanced since the previous reading, -1 on the first reading or a reset.
func (p *sysfsProbe) rate(key string, v float64, now time.Time, unit time.Duration) float64 {
	prev, ok := p.prev[key]
	p.prev[key] = counterReading{v, now}
	wall := now.Sub(prev.at)
	if !ok || wall <= 0 || v < prev.value {
		return -1
	}
	return (v - prev.value) * float64(unit) / float64(wall) * 100
}
