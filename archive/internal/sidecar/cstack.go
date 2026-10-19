package sidecar

import (
	"encoding/json"
	"os"
	"os/exec"
	"path/filepath"
	"time"
)

// CstackStats represents task statistics from cs stats --json
type CstackStats struct {
	Open       int  `json:"open"`
	Ready      int  `json:"ready"`
	InProgress int  `json:"in_progress"`
	Blocked    int  `json:"blocked"`
	Closed     int  `json:"closed"`
	Total      int  `json:"total"`
	Exists     bool `json:"exists"`
}

// CstackChecker checks cstack status with caching
type CstackChecker struct {
	workspacePath string
	cache         *CachedValue[*CstackStats]
}

// NewCstackChecker creates a new cstack checker
func NewCstackChecker(workspacePath string) *CstackChecker {
	c := &CstackChecker{workspacePath: workspacePath}
	c.cache = NewCachedValue(5*time.Second, c.fetch)
	return c
}

// Stats returns cstack statistics, using cache if fresh
func (c *CstackChecker) Stats() *CstackStats {
	return c.cache.Get()
}

func (c *CstackChecker) fetch() *CstackStats {
	cstackPath := filepath.Join(c.workspacePath, ".cstack")
	if _, err := os.Stat(cstackPath); os.IsNotExist(err) {
		return nil
	}

	cmd := exec.Command("cs", "stats", "--json")
	cmd.Dir = c.workspacePath
	output, err := cmd.Output()
	if err != nil {
		return &CstackStats{Exists: true}
	}

	var stats CstackStats
	if err := json.Unmarshal(output, &stats); err != nil {
		return &CstackStats{Exists: true}
	}

	stats.Exists = true
	return &stats
}
