package envoy

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"sync"
	"time"

	"github.com/hotschmoe/protectorate/internal/protocol"
)

// SSEBroadcaster polls for changes and broadcasts updates to SSE clients
type SSEBroadcaster struct {
	hub        *SSEHub
	sleeves    SleeveListProvider
	sidecar    SidecarStatusClient
	docker     DockerContainerStatsClient
	hostStats  HostStatsProvider
	workspaces *WorkspaceManager

	mu             sync.RWMutex
	prevSnapshots  map[string]string // sleeve name -> content hash
	prevHostHash   string
	initRequested  chan struct{}
}

// NewSSEBroadcaster creates a new SSE broadcaster
func NewSSEBroadcaster(hub *SSEHub, sleeves SleeveListProvider, sidecar SidecarStatusClient, docker DockerContainerStatsClient, hostStats HostStatsProvider, workspaces *WorkspaceManager) *SSEBroadcaster {
	return &SSEBroadcaster{
		hub:           hub,
		sleeves:       sleeves,
		sidecar:       sidecar,
		docker:        docker,
		hostStats:     hostStats,
		workspaces:    workspaces,
		prevSnapshots: make(map[string]string),
		initRequested: make(chan struct{}, 1),
	}
}

// Start begins the background polling loop
func (b *SSEBroadcaster) Start(ctx context.Context) {
	sleeveTicker := time.NewTicker(3 * time.Second)
	hostTicker := time.NewTicker(5 * time.Second)
	defer sleeveTicker.Stop()
	defer hostTicker.Stop()

	for {
		select {
		case <-ctx.Done():
			return

		case <-b.initRequested:
			// Client connected and needs initial state
			if b.hub.ClientCount() > 0 {
				b.broadcastFullGrid()
				b.broadcastHostStatsForce(ctx)
			}

		case <-sleeveTicker.C:
			if b.hub.ClientCount() > 0 {
				b.checkSleeveChanges(ctx)
			}

		case <-hostTicker.C:
			if b.hub.ClientCount() > 0 {
				b.broadcastHostStats(ctx)
			}
		}
	}
}

// RequestInit requests an initial state broadcast for a new client
func (b *SSEBroadcaster) RequestInit() {
	select {
	case b.initRequested <- struct{}{}:
	default:
		// Already pending
	}
}

// broadcastFullGrid sends the complete sleeve grid to all clients
func (b *SSEBroadcaster) broadcastFullGrid() {
	sleeves := b.sleeves.List()
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	b.enrichSleeves(ctx, sleeves)

	html := RenderSleeveGrid(sleeves)
	b.hub.Broadcast("init", html)

	// Update snapshots
	b.mu.Lock()
	b.prevSnapshots = make(map[string]string)
	for _, s := range sleeves {
		b.prevSnapshots[s.Name] = b.hashSleeve(s)
	}
	b.mu.Unlock()
}

// checkSleeveChanges detects sleeve changes and broadcasts updates
func (b *SSEBroadcaster) checkSleeveChanges(ctx context.Context) {
	sleeves := b.sleeves.List()
	b.enrichSleeves(ctx, sleeves)

	// Build current state map
	current := make(map[string]*protocol.SleeveInfo)
	currentHashes := make(map[string]string)
	for _, s := range sleeves {
		current[s.Name] = s
		currentHashes[s.Name] = b.hashSleeve(s)
	}

	b.mu.Lock()
	defer b.mu.Unlock()

	// Check for removed sleeves
	for name := range b.prevSnapshots {
		if _, exists := current[name]; !exists {
			// Sleeve was removed
			data, _ := json.Marshal(map[string]string{"name": name})
			b.hub.Broadcast("sleeve:remove", string(data))
		}
	}

	// Check for new or updated sleeves
	for name, sleeve := range current {
		prevHash, existed := b.prevSnapshots[name]
		currentHash := currentHashes[name]

		if !existed {
			// New sleeve
			html := RenderSleeveCard(sleeve)
			b.hub.Broadcast("sleeve:add", html)
		} else if prevHash != currentHash {
			// Sleeve updated
			html := RenderSleeveCardOOB(sleeve)
			b.hub.Broadcast("sleeve:update", html)
		}
	}

	// Update snapshots
	b.prevSnapshots = currentHashes
}

// broadcastHostStats sends updated host stats to all clients (with change detection)
func (b *SSEBroadcaster) broadcastHostStats(ctx context.Context) {
	stats := b.hostStats.GetStats(ctx)
	html := RenderHostStats(stats)

	// Check if changed
	hash := hashString(html)
	b.mu.Lock()
	changed := hash != b.prevHostHash
	if changed {
		b.prevHostHash = hash
	}
	b.mu.Unlock()

	if changed {
		b.hub.Broadcast("host:stats", html)
	}
}

// broadcastHostStatsForce sends host stats without change detection (for initial load)
func (b *SSEBroadcaster) broadcastHostStatsForce(ctx context.Context) {
	stats := b.hostStats.GetStats(ctx)
	html := RenderHostStats(stats)

	b.mu.Lock()
	b.prevHostHash = hashString(html)
	b.mu.Unlock()

	b.hub.Broadcast("host:stats", html)
}

// BroadcastCloneProgress sends clone job progress updates
func (b *SSEBroadcaster) BroadcastCloneProgress(jobID, status string, progress int, errMsg string) {
	data, _ := json.Marshal(map[string]interface{}{
		"id":       jobID,
		"status":   status,
		"progress": progress,
		"error":    errMsg,
	})
	b.hub.Broadcast("clone:progress", string(data))
}

// enrichSleeves fetches sidecar status and container stats for sleeves
func (b *SSEBroadcaster) enrichSleeves(ctx context.Context, sleeves []*protocol.SleeveInfo) {
	containerNames := make([]string, 0, len(sleeves))
	for _, sleeve := range sleeves {
		containerNames = append(containerNames, sleeve.ContainerName)
	}

	sidecarStatuses := b.sidecar.BatchGetStatus(containerNames)

	// Fetch container stats concurrently
	var statsWg sync.WaitGroup
	var statsMu sync.Mutex
	statsMap := make(map[string]*protocol.ContainerResourceStats)

	for _, sleeve := range sleeves {
		statsWg.Add(1)
		go func(containerID string) {
			defer statsWg.Done()
			if stats, err := b.docker.GetContainerStats(ctx, containerID); err == nil {
				statsMu.Lock()
				statsMap[containerID] = stats
				statsMu.Unlock()
			}
		}(sleeve.ContainerID)
	}
	statsWg.Wait()

	// Apply sidecar status and stats to sleeves
	for _, sleeve := range sleeves {
		sleeve.Integrity = 100.0

		if status, ok := sidecarStatuses[sleeve.ContainerName]; ok {
			sleeve.SidecarHealthy = true
			if status.DHF != nil {
				sleeve.DHF = status.DHF.Name
				sleeve.DHFVersion = status.DHF.Version
			}
			if status.Workspace != nil && status.Workspace.Cstack != nil {
				sleeve.Integrity = calculateIntegrityFromCstack(status.Workspace.Cstack)
			}
		}

		if stats, ok := statsMap[sleeve.ContainerID]; ok {
			sleeve.Resources = stats
		}
	}
}

// hashSleeve creates a hash of sleeve state for change detection
func (b *SSEBroadcaster) hashSleeve(s *protocol.SleeveInfo) string {
	var memUsed uint64
	var cpuPct float64
	if s.Resources != nil {
		memUsed = s.Resources.MemoryUsedBytes
		cpuPct = s.Resources.CPUPercent
	}

	data, _ := json.Marshal(struct {
		Name           string
		Status         string
		Integrity      float64
		SidecarHealthy bool
		DHF            string
		DHFVersion     string
		MemUsed        uint64
		CPUPct         float64
	}{
		Name:           s.Name,
		Status:         s.Status,
		Integrity:      s.Integrity,
		SidecarHealthy: s.SidecarHealthy,
		DHF:            s.DHF,
		DHFVersion:     s.DHFVersion,
		MemUsed:        memUsed,
		CPUPct:         cpuPct,
	})
	return hashString(string(data))
}

func hashString(s string) string {
	h := sha256.Sum256([]byte(s))
	return hex.EncodeToString(h[:8])
}
