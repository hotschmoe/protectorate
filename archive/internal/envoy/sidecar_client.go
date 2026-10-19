package envoy

import (
	"encoding/json"
	"fmt"
	"net/http"
	"sync"
	"time"

	"github.com/hotschmoe/protectorate/internal/protocol"
)

// SidecarClient calls sleeve sidecar HTTP APIs
type SidecarClient struct {
	http    *http.Client
	network string
}

// SidecarStatus is the response from GET /status on a sleeve sidecar
type SidecarStatus struct {
	SleeveName string              `json:"sleeve_name"`
	DHF        *SidecarDHFInfo     `json:"dhf"`
	Workspace  *SidecarWorkspace   `json:"workspace"`
	Process    *SidecarProcessInfo `json:"process"`
	Auth       *SidecarAuthInfo    `json:"auth"`
}

// SidecarDHFInfo contains CLI detection info
type SidecarDHFInfo struct {
	Name    string `json:"name"`
	Version string `json:"version,omitempty"`
}

// SidecarWorkspace contains workspace and cstack info
type SidecarWorkspace struct {
	Path   string               `json:"path"`
	Cstack *protocol.CstackStats `json:"cstack,omitempty"`
}

// SidecarProcessInfo contains process stats
type SidecarProcessInfo struct {
	PID         int     `json:"pid"`
	UptimeSecs  float64 `json:"uptime_secs"`
	MemoryRSSKB int64   `json:"memory_rss_kb"`
}

// SidecarAuthInfo contains credential status
type SidecarAuthInfo struct {
	ClaudeAuth bool `json:"claude_auth"`
	GeminiAuth bool `json:"gemini_auth"`
}

// NewSidecarClient creates a client for calling sleeve sidecars
func NewSidecarClient(network string) *SidecarClient {
	return &SidecarClient{
		http: &http.Client{
			Timeout: 5 * time.Second,
		},
		network: network,
	}
}

// GetStatus fetches status from a sleeve's sidecar
func (c *SidecarClient) GetStatus(containerName string) (*SidecarStatus, error) {
	url := fmt.Sprintf("http://%s:8080/status", containerName)
	resp, err := c.http.Get(url)
	if err != nil {
		return nil, fmt.Errorf("sidecar request failed: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("sidecar returned status %d", resp.StatusCode)
	}

	var status SidecarStatus
	if err := json.NewDecoder(resp.Body).Decode(&status); err != nil {
		return nil, fmt.Errorf("failed to decode sidecar response: %w", err)
	}

	return &status, nil
}

// Health checks if a sleeve's sidecar is healthy
func (c *SidecarClient) Health(containerName string) bool {
	url := fmt.Sprintf("http://%s:8080/health", containerName)
	resp, err := c.http.Get(url)
	if err != nil {
		return false
	}
	defer resp.Body.Close()
	return resp.StatusCode == http.StatusOK
}

// BatchGetStatus fetches status from multiple sleeves concurrently
func (c *SidecarClient) BatchGetStatus(containerNames []string) map[string]*SidecarStatus {
	results := make(map[string]*SidecarStatus)
	var mu sync.Mutex
	var wg sync.WaitGroup

	for _, name := range containerNames {
		wg.Add(1)
		go func(containerName string) {
			defer wg.Done()
			status, err := c.GetStatus(containerName)
			if err == nil {
				mu.Lock()
				results[containerName] = status
				mu.Unlock()
			}
		}(name)
	}

	wg.Wait()
	return results
}
