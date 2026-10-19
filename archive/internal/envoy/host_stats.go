package envoy

import (
	"bufio"
	"context"
	"os"
	"strconv"
	"strings"
	"sync"
	"syscall"
	"time"

	"github.com/hotschmoe/protectorate/internal/protocol"
)


// HostStatsCollector gathers host system statistics
type HostStatsCollector struct {
	procPath      string
	docker        DockerContainerCountsClient
	maxContainers int

	mu            sync.Mutex
	lastCPUStats  *cpuRawStats
	lastCPUTime   time.Time
}

type cpuRawStats struct {
	user   uint64
	nice   uint64
	system uint64
	idle   uint64
	iowait uint64
	irq    uint64
	soft   uint64
	steal  uint64
}

func (c *cpuRawStats) total() uint64 {
	return c.user + c.nice + c.system + c.idle + c.iowait + c.irq + c.soft + c.steal
}

func (c *cpuRawStats) active() uint64 {
	return c.user + c.nice + c.system + c.irq + c.soft + c.steal
}

// NewHostStatsCollector creates a new host stats collector
func NewHostStatsCollector(docker DockerContainerCountsClient, maxContainers int) *HostStatsCollector {
	procPath := "/host/proc"
	if _, err := os.Stat(procPath); os.IsNotExist(err) {
		procPath = "/proc"
	}

	return &HostStatsCollector{
		procPath:      procPath,
		docker:        docker,
		maxContainers: maxContainers,
	}
}

// GetStats returns all host statistics
func (h *HostStatsCollector) GetStats(ctx context.Context) *protocol.HostStats {
	stats := &protocol.HostStats{}

	stats.CPU = h.GetCPUStats()
	stats.Memory = h.GetMemoryStats()
	stats.Disk = h.GetDiskStats("/home/agent/workspaces")
	stats.Docker = h.GetDockerStats(ctx)

	return stats
}

// GetMemoryStats parses /proc/meminfo for memory usage
func (h *HostStatsCollector) GetMemoryStats() *protocol.MemoryStats {
	file, err := os.Open(h.procPath + "/meminfo")
	if err != nil {
		return nil
	}
	defer file.Close()

	var memTotal, memAvailable uint64
	scanner := bufio.NewScanner(file)
	for scanner.Scan() {
		line := scanner.Text()
		fields := strings.Fields(line)
		if len(fields) < 2 {
			continue
		}

		value, _ := strconv.ParseUint(fields[1], 10, 64)
		value *= 1024 // convert from kB to bytes

		switch fields[0] {
		case "MemTotal:":
			memTotal = value
		case "MemAvailable:":
			memAvailable = value
		}
	}

	if memTotal == 0 {
		return nil
	}

	used := memTotal - memAvailable
	return &protocol.MemoryStats{
		UsedBytes:  used,
		TotalBytes: memTotal,
		Percent:    float64(used) / float64(memTotal) * 100,
	}
}

// GetCPUStats parses /proc/stat for CPU usage (requires delta calculation)
func (h *HostStatsCollector) GetCPUStats() *protocol.CPUStats {
	file, err := os.Open(h.procPath + "/stat")
	if err != nil {
		return nil
	}
	defer file.Close()

	var current cpuRawStats
	var cores, threads int

	scanner := bufio.NewScanner(file)
	for scanner.Scan() {
		line := scanner.Text()
		fields := strings.Fields(line)
		if len(fields) < 2 {
			continue
		}

		if fields[0] == "cpu" && len(fields) >= 5 {
			current.user, _ = strconv.ParseUint(fields[1], 10, 64)
			current.nice, _ = strconv.ParseUint(fields[2], 10, 64)
			current.system, _ = strconv.ParseUint(fields[3], 10, 64)
			current.idle, _ = strconv.ParseUint(fields[4], 10, 64)
			if len(fields) > 5 {
				current.iowait, _ = strconv.ParseUint(fields[5], 10, 64)
			}
			if len(fields) > 6 {
				current.irq, _ = strconv.ParseUint(fields[6], 10, 64)
			}
			if len(fields) > 7 {
				current.soft, _ = strconv.ParseUint(fields[7], 10, 64)
			}
			if len(fields) > 8 {
				current.steal, _ = strconv.ParseUint(fields[8], 10, 64)
			}
		} else if strings.HasPrefix(fields[0], "cpu") {
			threads++
		}
	}

	cores = h.getCoreCount()
	if cores == 0 {
		cores = threads
	}

	h.mu.Lock()
	defer h.mu.Unlock()

	var usagePercent float64
	if h.lastCPUStats != nil {
		totalDelta := current.total() - h.lastCPUStats.total()
		activeDelta := current.active() - h.lastCPUStats.active()
		if totalDelta > 0 {
			usagePercent = float64(activeDelta) / float64(totalDelta) * 100
		}
	}

	h.lastCPUStats = &current
	h.lastCPUTime = time.Now()

	return &protocol.CPUStats{
		UsagePercent: usagePercent,
		Cores:        cores,
		Threads:      threads,
	}
}

func (h *HostStatsCollector) getCoreCount() int {
	file, err := os.Open(h.procPath + "/cpuinfo")
	if err != nil {
		return 0
	}
	defer file.Close()

	coreIDs := make(map[string]bool)
	scanner := bufio.NewScanner(file)
	for scanner.Scan() {
		line := scanner.Text()
		if strings.HasPrefix(line, "core id") {
			parts := strings.Split(line, ":")
			if len(parts) == 2 {
				coreIDs[strings.TrimSpace(parts[1])] = true
			}
		}
	}

	if len(coreIDs) == 0 {
		return 0
	}
	return len(coreIDs)
}

// GetDiskStats returns disk usage for the given path
func (h *HostStatsCollector) GetDiskStats(path string) *protocol.DiskStats {
	var stat syscall.Statfs_t
	if err := syscall.Statfs(path, &stat); err != nil {
		return nil
	}

	total := stat.Blocks * uint64(stat.Bsize)
	free := stat.Bfree * uint64(stat.Bsize)
	used := total - free

	return &protocol.DiskStats{
		UsedBytes:  used,
		TotalBytes: total,
		Percent:    float64(used) / float64(total) * 100,
	}
}

// GetDockerStats returns Docker container counts
func (h *HostStatsCollector) GetDockerStats(ctx context.Context) *protocol.DockerStats {
	counts, err := h.docker.GetContainerCounts(ctx)
	if err != nil {
		return nil
	}

	return &protocol.DockerStats{
		RunningContainers: counts.Running,
		TotalContainers:   counts.Total,
		MaxContainers:     h.maxContainers,
	}
}
