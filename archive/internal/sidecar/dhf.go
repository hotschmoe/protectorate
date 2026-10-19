package sidecar

import (
	"os/exec"
	"strings"
	"sync"
)

// DHFInfo contains information about the detected CLI
type DHFInfo struct {
	Name    string `json:"name"`
	Version string `json:"version,omitempty"`
}

// DHFDetector detects and caches CLI information
type DHFDetector struct {
	once sync.Once
	info *DHFInfo
}

// NewDHFDetector creates a new DHF detector
func NewDHFDetector() *DHFDetector {
	return &DHFDetector{}
}

// Info returns cached CLI information, detecting on first call
func (d *DHFDetector) Info() *DHFInfo {
	d.once.Do(func() {
		d.info = d.detect()
	})
	return d.info
}

var knownCLIs = []string{"claude", "gemini", "codex", "aider", "opencode"}

func (d *DHFDetector) detect() *DHFInfo {
	for _, cli := range knownCLIs {
		if version := d.tryCommand(cli, "--version"); version != "" {
			return &DHFInfo{Name: cli, Version: version}
		}
	}
	return &DHFInfo{Name: "unknown", Version: ""}
}

func (d *DHFDetector) tryCommand(name string, args ...string) string {
	output, err := exec.Command(name, args...).Output()
	if err != nil {
		return ""
	}
	line, _, _ := strings.Cut(strings.TrimSpace(string(output)), "\n")
	return line
}
