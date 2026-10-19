package sidecar

import (
	"os"
	"strconv"
	"strings"
	"time"
)

// ProcessInfo contains process information from /proc
type ProcessInfo struct {
	PID         int     `json:"pid"`
	UptimeSecs  float64 `json:"uptime_secs"`
	MemoryRSSKB int64   `json:"memory_rss_kb"`
}

// ProcessStats reads process information from /proc
type ProcessStats struct {
	pid       int
	startTime time.Time
}

// NewProcessStats creates a new process stats reader
func NewProcessStats() *ProcessStats {
	return &ProcessStats{
		pid:       os.Getpid(),
		startTime: time.Now(),
	}
}

// Info returns current process information
func (p *ProcessStats) Info() *ProcessInfo {
	info := &ProcessInfo{
		PID:        p.pid,
		UptimeSecs: time.Since(p.startTime).Seconds(),
	}

	// Read memory from /proc/self/status
	if data, err := os.ReadFile("/proc/self/status"); err == nil {
		info.MemoryRSSKB = parseVmRSS(string(data))
	}

	return info
}

func parseVmRSS(status string) int64 {
	for _, line := range strings.Split(status, "\n") {
		if strings.HasPrefix(line, "VmRSS:") {
			fields := strings.Fields(line)
			if len(fields) >= 2 {
				if val, err := strconv.ParseInt(fields[1], 10, 64); err == nil {
					return val
				}
			}
		}
	}
	return 0
}
