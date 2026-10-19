// Package node collects machine metrics and serves them as NodeAgentService.
package node

import (
	"context"
	"fmt"
	"os"
	"os/exec"
	"runtime"
	"sort"
	"strings"
	"sync"
	"time"

	"github.com/shirou/gopsutil/v4/cpu"
	"github.com/shirou/gopsutil/v4/disk"
	"github.com/shirou/gopsutil/v4/host"
	"github.com/shirou/gopsutil/v4/load"
	"github.com/shirou/gopsutil/v4/mem"
	"google.golang.org/protobuf/types/known/timestamppb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
)

type Options struct {
	Version string
	// WatchPaths are directories whose sizes are measured on SlowInterval.
	WatchPaths []string
	// SlowInterval is the period of directory-size and docker df
	// measurements, default 15m.
	SlowInterval time.Duration
	// DirBudget bounds the walk of one watch path, default 60s.
	DirBudget time.Duration
	// DockerHost overrides DOCKER_HOST; empty means the environment, then the
	// default socket.
	DockerHost string
	// MinAge is how long a Collect result is reused by concurrent callers,
	// default 2s.
	MinAge time.Duration
}

func (o Options) withDefaults() Options {
	if o.SlowInterval <= 0 {
		o.SlowInterval = 15 * time.Minute
	}
	if o.DirBudget <= 0 {
		o.DirBudget = 60 * time.Second
	}
	if o.MinAge <= 0 {
		o.MinAge = 2 * time.Second
	}
	return o
}

// Collector gathers metrics for the machine it runs on. Collect is cheap
// enough to call every few seconds; anything that walks the disk runs on the
// slow interval in Run (or on demand via Refresh) and is only read here.
type Collector struct {
	opts   Options
	docker *dockerClient

	collectMu sync.Mutex
	last      *pb.NodeMetrics
	lastAt    time.Time

	cpuMu   sync.Mutex
	cpuPrev *cpu.TimesStat

	slowMu sync.RWMutex
	dirs   []*pb.DirSize
	df     *dockerDF
	dfAt   time.Time
}

func NewCollector(opts Options) *Collector {
	opts = opts.withDefaults()
	return &Collector{opts: opts, docker: newDockerClient(opts.DockerHost)}
}

// Run measures the slow-changing figures now and then every SlowInterval
// until ctx is done.
func (c *Collector) Run(ctx context.Context) {
	c.Refresh(ctx)
	tick := time.NewTicker(c.opts.SlowInterval)
	defer tick.Stop()
	for {
		select {
		case <-ctx.Done():
			return
		case <-tick.C:
			c.Refresh(ctx)
		}
	}
}

// Refresh measures watch-path sizes and the container engine's disk usage,
// then stores them for Collect.
func (c *Collector) Refresh(ctx context.Context) {
	var wg sync.WaitGroup
	wg.Add(2)
	var dirs []*pb.DirSize
	var df *dockerDF
	go func() {
		defer wg.Done()
		dirs = measureDirs(ctx, c.opts.WatchPaths, c.opts.DirBudget)
	}()
	go func() {
		defer wg.Done()
		df = c.docker.diskUsage(ctx)
	}()
	wg.Wait()
	if ctx.Err() != nil {
		return
	}
	c.slowMu.Lock()
	c.dirs = dirs
	if df != nil {
		c.df, c.dfAt = df, time.Now()
	}
	c.slowMu.Unlock()
}

// Collect returns current metrics. Concurrent and rapid callers share one
// result, so the returned message must be treated as read-only.
func (c *Collector) Collect(ctx context.Context) (*pb.NodeMetrics, error) {
	c.collectMu.Lock()
	defer c.collectMu.Unlock()
	if c.last != nil && time.Since(c.lastAt) < c.opts.MinAge {
		return c.last, nil
	}
	m := &pb.NodeMetrics{Os: runtime.GOOS, Arch: runtime.GOARCH, AgentVersion: c.opts.Version, CollectedAt: timestamppb.Now()}
	m.Hostname = Hostname()

	var (
		mu sync.Mutex
		wg sync.WaitGroup
	)
	warn := func(section string, err error) {
		mu.Lock()
		m.Warnings = append(m.Warnings, fmt.Sprintf("%s: %v", section, err))
		mu.Unlock()
	}
	run := func(f func()) {
		wg.Add(1)
		go func() { defer wg.Done(); f() }()
	}
	run(func() {
		info, err := host.InfoWithContext(ctx)
		if err != nil {
			warn("host", err)
			return
		}
		mu.Lock()
		m.Platform = strings.TrimSpace(info.Platform + " " + info.PlatformVersion)
		m.UptimeSeconds = int64(info.Uptime)
		if m.Hostname == "" {
			m.Hostname = info.Hostname
		}
		mu.Unlock()
	})
	run(func() {
		cm, err := c.cpuMetrics(ctx)
		if err != nil {
			warn("cpu", err)
		}
		mu.Lock()
		m.Cpu = cm
		mu.Unlock()
	})
	run(func() {
		mm, err := memMetrics(ctx)
		if err != nil {
			warn("memory", err)
			return
		}
		mu.Lock()
		m.Memory = mm
		mu.Unlock()
	})
	run(func() {
		fs, errs := filesystemMetrics(ctx)
		mu.Lock()
		m.Filesystems = fs
		mu.Unlock()
		for _, err := range errs {
			warn("disk", err)
		}
	})
	run(func() {
		g := gpuMetrics(ctx)
		mu.Lock()
		m.Gpus = g
		mu.Unlock()
	})
	run(func() {
		eng, err := c.engineMetrics(ctx)
		if err != nil {
			warn("container engine", err)
		}
		mu.Lock()
		m.Engine = eng
		mu.Unlock()
	})
	wg.Wait()
	if err := ctx.Err(); err != nil {
		return nil, err
	}

	c.slowMu.RLock()
	m.WatchDirs = c.dirs
	c.slowMu.RUnlock()
	sort.Strings(m.Warnings)
	c.last, c.lastAt = m, time.Now()
	return m, nil
}

func (c *Collector) cpuMetrics(ctx context.Context) (*pb.CpuMetrics, error) {
	cm := &pb.CpuMetrics{LogicalCores: int32(runtime.NumCPU())}
	if runtime.GOOS != "windows" {
		if avg, err := load.AvgWithContext(ctx); err == nil {
			cm.Load1, cm.Load5, cm.Load15 = &avg.Load1, &avg.Load5, &avg.Load15
		}
	}
	usage, err := c.cpuUsage(ctx)
	if err != nil {
		return cm, err
	}
	cm.UsagePercent = usage
	return cm, nil
}

// Hostname is the machine's name. Android reports "localhost", so the device
// model stands in for it there.
func Hostname() string {
	h, _ := os.Hostname()
	if runtime.GOOS == "android" && (h == "" || h == "localhost") {
		if out, err := exec.Command("/system/bin/getprop", "ro.product.model").Output(); err == nil {
			if model := strings.TrimSpace(string(out)); model != "" {
				return model
			}
		}
	}
	return h
}

// cpuUsage is the busy share since the previous call; the first call samples
// over a short window instead.
func (c *Collector) cpuUsage(ctx context.Context) (float64, error) {
	c.cpuMu.Lock()
	defer c.cpuMu.Unlock()
	if c.cpuPrev == nil {
		t, err := cpu.TimesWithContext(ctx, false)
		if err != nil || len(t) == 0 {
			return 0, fmt.Errorf("read cpu times: %w", firstErr(err))
		}
		c.cpuPrev = &t[0]
		select {
		case <-time.After(250 * time.Millisecond):
		case <-ctx.Done():
			return 0, ctx.Err()
		}
	}
	t, err := cpu.TimesWithContext(ctx, false)
	if err != nil || len(t) == 0 {
		return 0, fmt.Errorf("read cpu times: %w", firstErr(err))
	}
	p := cpuBusyPercent(*c.cpuPrev, t[0])
	c.cpuPrev = &t[0]
	return p, nil
}

func firstErr(err error) error {
	if err != nil {
		return err
	}
	return fmt.Errorf("no data")
}

func cpuBusyPercent(prev, cur cpu.TimesStat) float64 {
	idle := func(t cpu.TimesStat) float64 { return t.Idle + t.Iowait }
	total := cur.Total() - prev.Total()
	if total <= 0 {
		return 0
	}
	return clampPercent((total - (idle(cur) - idle(prev))) / total * 100)
}

func clampPercent(p float64) float64 {
	switch {
	case p < 0:
		return 0
	case p > 100:
		return 100
	}
	return p
}

func memMetrics(ctx context.Context) (*pb.MemoryMetrics, error) {
	vm, err := mem.VirtualMemoryWithContext(ctx)
	if err != nil {
		return nil, fmt.Errorf("read memory: %w", err)
	}
	mm := &pb.MemoryMetrics{TotalBytes: vm.Total, AvailableBytes: vm.Available}
	if vm.Total > 0 && vm.Available <= vm.Total {
		mm.UsedBytes = vm.Total - vm.Available
		mm.UsedPercent = float64(mm.UsedBytes) / float64(vm.Total) * 100
	}
	if sw, err := mem.SwapMemoryWithContext(ctx); err == nil && sw.Total > 0 {
		mm.SwapTotalBytes, mm.SwapUsedBytes = sw.Total, sw.Used
		mm.SwapUsedPercent = clampPercent(float64(sw.Used) / float64(sw.Total) * 100)
	}
	return mm, nil
}

var pseudoFS = map[string]bool{
	"tmpfs": true, "devtmpfs": true, "squashfs": true, "overlay": true, "ramfs": true,
	"proc": true, "sysfs": true, "cgroup": true, "cgroup2": true, "devpts": true,
	"efivarfs": true, "iso9660": true, "udf": true, "fuse.portal": true, "fuse.gvfsd-fuse": true,
	"autofs": true, "binfmt_misc": true, "bpf": true, "tracefs": true, "debugfs": true,
	"securityfs": true, "pstore": true, "configfs": true, "mqueue": true, "hugetlbfs": true,
	"nsfs": true, "fusectl": true, "rpc_pipefs": true,
}

// keepPartition reports whether a mount is a real, user-visible filesystem.
func keepPartition(p disk.PartitionStat) bool {
	if p.Fstype == "" || (pseudoFS[strings.ToLower(p.Fstype)] && !(p.Fstype == "overlay" && p.Mountpoint == "/")) {
		return false
	}
	if runtime.GOOS == "android" && androidSystemMount(p.Mountpoint) {
		return false
	}
	for _, skip := range []string{"/etc/", "/snap/", "/var/lib/docker/", "/var/lib/containers/", "/run/", "/proc", "/sys"} {
		if strings.HasPrefix(p.Mountpoint, skip) {
			return false
		}
	}
	return true
}

var androidSystemDirs = []string{
	"/apex", "/system", "/system_ext", "/vendor", "/vendor_dlkm", "/product", "/odm", "/metadata",
	"/mnt", "/data_mirror", "/linkerconfig", "/debug_ramdisk", "/cache",
}

// androidSystemMount reports mounts of the read-only system image and the
// per-app views Android layers over /data and /storage.
func androidSystemMount(mount string) bool {
	for _, d := range androidSystemDirs {
		if mount == d || strings.HasPrefix(mount, d+"/") {
			return true
		}
	}
	return false
}

type fsKey struct {
	device string
	total  uint64
}

func filesystemMetrics(ctx context.Context) ([]*pb.FilesystemMetrics, []error) {
	parts, err := disk.PartitionsWithContext(ctx, true)
	if err != nil {
		parts = nil
	}
	var cands []disk.PartitionStat
	for _, p := range parts {
		if keepPartition(p) {
			cands = append(cands, p)
		}
	}
	if len(cands) == 0 {
		root := "/"
		if runtime.GOOS == "windows" {
			root = os.Getenv("SystemDrive") + `\`
		}
		cands = []disk.PartitionStat{{Mountpoint: root, Device: root, Fstype: "unknown"}}
	}

	type result struct {
		fs  *pb.FilesystemMetrics
		err error
	}
	results := make([]result, len(cands))
	var wg sync.WaitGroup
	for i, p := range cands {
		wg.Add(1)
		go func() {
			defer wg.Done()
			// Dead network mounts can block statfs forever, so each probe is
			// abandoned after a deadline.
			uctx, cancel := context.WithTimeout(ctx, 5*time.Second)
			defer cancel()
			ch := make(chan result, 1)
			go func() {
				if fi, err := os.Stat(p.Mountpoint); err == nil && !fi.IsDir() {
					ch <- result{}
					return
				}
				u, err := disk.UsageWithContext(uctx, p.Mountpoint)
				if err != nil {
					ch <- result{err: fmt.Errorf("%s: %w", p.Mountpoint, err)}
					return
				}
				ch <- result{fs: &pb.FilesystemMetrics{
					Mount: p.Mountpoint, Device: p.Device, Fstype: p.Fstype,
					TotalBytes: u.Total, UsedBytes: u.Used, FreeBytes: u.Free, UsedPercent: u.UsedPercent,
				}}
			}()
			select {
			case results[i] = <-ch:
			case <-uctx.Done():
				results[i] = result{err: fmt.Errorf("%s: timed out", p.Mountpoint)}
			}
		}()
	}
	wg.Wait()

	var all []*pb.FilesystemMetrics
	var errs []error
	for _, r := range results {
		if r.err != nil {
			errs = append(errs, r.err)
			continue
		}
		if r.fs == nil {
			continue
		}
		all = append(all, r.fs)
	}
	return dedupeFilesystems(all), errs
}

// dedupeFilesystems drops empty filesystems and repeated views of one device
// (bind mounts, btrfs subvolumes), keeping the shortest mount path.
func dedupeFilesystems(in []*pb.FilesystemMetrics) []*pb.FilesystemMetrics {
	sort.Slice(in, func(i, j int) bool {
		if len(in[i].Mount) != len(in[j].Mount) {
			return len(in[i].Mount) < len(in[j].Mount)
		}
		return in[i].Mount < in[j].Mount
	})
	var out []*pb.FilesystemMetrics
	seen := map[fsKey]bool{}
	for _, f := range in {
		k := fsKey{f.Device, f.TotalBytes}
		if f.TotalBytes == 0 || seen[k] {
			continue
		}
		seen[k] = true
		out = append(out, f)
	}
	sort.Slice(out, func(i, j int) bool { return out[i].Mount < out[j].Mount })
	return out
}

// WorstFilesystem returns the fullest filesystem, or nil if none was read.
func WorstFilesystem(m *pb.NodeMetrics) *pb.FilesystemMetrics {
	var worst *pb.FilesystemMetrics
	for _, f := range m.GetFilesystems() {
		if worst == nil || f.UsedPercent > worst.UsedPercent {
			worst = f
		}
	}
	return worst
}
