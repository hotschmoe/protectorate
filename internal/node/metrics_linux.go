package node

import (
	"context"
	"fmt"
	"os"
	"os/exec"
	"runtime"
	"strconv"
	"strings"

	"golang.org/x/sys/unix"
)

func procRead(ctx context.Context, path string) ([]byte, error) {
	if err := ctx.Err(); err != nil {
		return nil, err
	}
	b, err := os.ReadFile(path)
	if err != nil {
		return nil, fmt.Errorf("read %s: %w", path, err)
	}
	return b, nil
}

func readCPUTimes(ctx context.Context) ([]cpuTimes, error) {
	b, err := procRead(ctx, "/proc/stat")
	if err != nil {
		return nil, err
	}
	line, _, _ := strings.Cut(string(b), "\n")
	f := strings.Fields(line)
	if len(f) < 5 || f[0] != "cpu" {
		return nil, fmt.Errorf("invalid /proc/stat CPU counters")
	}
	var values [8]float64
	for i := 1; i < len(f) && i <= len(values); i++ {
		v, err := strconv.ParseFloat(f[i], 64)
		if err != nil {
			return nil, fmt.Errorf("parse CPU counter: %w", err)
		}
		values[i-1] = v
	}
	return []cpuTimes{{User: values[0], Nice: values[1], System: values[2], Idle: values[3], Iowait: values[4], Irq: values[5], Softirq: values[6], Steal: values[7]}}, nil
}

func loadAverage(ctx context.Context) (loadValues, error) {
	b, err := procRead(ctx, "/proc/loadavg")
	if err != nil {
		return loadValues{}, err
	}
	var v loadValues
	if _, err := fmt.Sscan(string(b), &v.Load1, &v.Load5, &v.Load15); err != nil {
		return v, fmt.Errorf("parse load averages: %w", err)
	}
	return v, nil
}

func memoryCounters(ctx context.Context) (map[string]uint64, error) {
	b, err := procRead(ctx, "/proc/meminfo")
	if err != nil {
		return nil, err
	}
	out := map[string]uint64{}
	for line := range strings.SplitSeq(string(b), "\n") {
		f := strings.Fields(line)
		if len(f) < 2 {
			continue
		}
		v, err := strconv.ParseUint(f[1], 10, 64)
		if err != nil {
			return nil, fmt.Errorf("parse memory counter %s: %w", f[0], err)
		}
		if len(f) > 2 && f[2] == "kB" {
			v *= 1024
		}
		out[strings.TrimSuffix(f[0], ":")] = v
	}
	return out, nil
}

func readMemory(ctx context.Context) (memoryInfo, error) {
	v, err := memoryCounters(ctx)
	if err != nil {
		return memoryInfo{}, err
	}
	if v["MemTotal"] == 0 {
		return memoryInfo{}, fmt.Errorf("missing MemTotal in /proc/meminfo")
	}
	available, ok := v["MemAvailable"]
	if !ok {
		available = v["MemFree"] + v["Buffers"] + v["Cached"] + v["SReclaimable"]
		available -= min(available, v["Shmem"])
	}
	return memoryInfo{Total: v["MemTotal"], Available: min(available, v["MemTotal"])}, nil
}

func readSwap(ctx context.Context) (memoryInfo, error) {
	v, err := memoryCounters(ctx)
	if err != nil {
		return memoryInfo{}, err
	}
	return memoryInfo{Total: v["SwapTotal"], Used: v["SwapTotal"] - min(v["SwapTotal"], v["SwapFree"])}, nil
}

func hostInfo(ctx context.Context) (hostDetails, error) {
	h := hostDetails{Platform: runtime.GOOS}
	h.Hostname, _ = os.Hostname()
	if runtime.GOOS == "android" {
		if b, err := exec.CommandContext(ctx, "/system/bin/getprop", "ro.build.version.release").Output(); err == nil {
			h.PlatformVersion = strings.TrimSpace(string(b))
		}
	}
	if b, err := procRead(ctx, "/etc/os-release"); err == nil {
		for line := range strings.SplitSeq(string(b), "\n") {
			k, v, ok := strings.Cut(line, "=")
			if !ok {
				continue
			}
			v = strings.Trim(v, `"'`)
			if k == "ID" {
				h.Platform = v
			}
			if k == "VERSION_ID" {
				h.PlatformVersion = v
			}
		}
	}
	if h.Platform == "debian" {
		if b, err := procRead(ctx, "/etc/debian_version"); err == nil {
			h.PlatformVersion = strings.TrimSpace(string(b))
		}
	}
	b, err := procRead(ctx, "/proc/uptime")
	if err != nil {
		return h, err
	}
	f := strings.Fields(string(b))
	if len(f) == 0 {
		return h, fmt.Errorf("empty /proc/uptime")
	}
	u, err := strconv.ParseFloat(f[0], 64)
	if err != nil {
		return h, fmt.Errorf("parse uptime: %w", err)
	}
	h.Uptime = uint64(u)
	return h, nil
}

func partitions(ctx context.Context) ([]partition, error) {
	b, err := procRead(ctx, "/proc/self/mounts")
	if err != nil {
		return nil, err
	}
	unescape := strings.NewReplacer(`\040`, " ", `\011`, "\t", `\012`, "\n", `\134`, `\`)
	var out []partition
	for line := range strings.SplitSeq(string(b), "\n") {
		f := strings.Fields(line)
		if len(f) < 3 {
			continue
		}
		out = append(out, partition{Device: unescape.Replace(f[0]), Mountpoint: unescape.Replace(f[1]), Fstype: f[2]})
	}
	return out, nil
}

func diskUsage(ctx context.Context, path string) (diskInfo, error) {
	if err := ctx.Err(); err != nil {
		return diskInfo{}, err
	}
	var s unix.Statfs_t
	if err := unix.Statfs(path, &s); err != nil {
		return diskInfo{}, fmt.Errorf("stat filesystem %s: %w", path, err)
	}
	v := diskInfo{Total: s.Blocks * uint64(s.Bsize), Free: s.Bavail * uint64(s.Bsize), Used: (s.Blocks - s.Bfree) * uint64(s.Bsize)}
	if denominator := v.Used + v.Free; denominator > 0 {
		v.UsedPercent = float64(v.Used) / float64(denominator) * 100
	}
	return v, nil
}
