# WebUI Functions Implementation Plan

This document outlines all WebUI features that currently use static placeholder data and the recommended approach for implementing real data sources.

## Overview

The WebUI has several components displaying static/placeholder values that need to be wired to real backend APIs:

| Component | Location | Status |
|-----------|----------|--------|
| Sleeve Resource Metrics | Sleeve cards | Static placeholders |
| Host Resources Panel | Sleeves tab | Static placeholders |
| Needlecast Messaging | Needlecast tab | Placeholder UI only |
| System Logs | Logs tab | Placeholder UI only |
| Configuration Viewer | Config tab | Placeholder UI only |

---

## 1. Sleeve Resource Metrics

**Current State:** `app.js:357-363` has hardcoded values:
```javascript
const uptime = '00:15:42';
const integrity = 98.7;
const memUsed = 2.1;
const memTotal = 4.0;
const cpuPct = 23;
```

### 1.1 Sleeve Uptime

**Data needed:** Time since container started

**Recommendation:** Calculate from existing data

The `SleeveInfo` struct already has `SpawnTime`. Calculate uptime client-side:
```javascript
const uptimeMs = Date.now() - new Date(s.spawn_time).getTime();
const uptime = formatDuration(uptimeMs);
```

**Implementation:**
- Backend: Already available via `/api/sleeves` (add `spawn_time` to JSON if not exposed)
- Frontend: Add `formatDuration()` helper in `app.js`

**Effort:** Low - data exists, just needs formatting

---

### 1.2 Container Memory Usage

**Data needed:** Memory used, memory limit (per container)

**Options:**

| Approach | Pros | Cons |
|----------|------|------|
| **A) Docker Stats API (Envoy polls Docker)** | Direct, no sidecar dependency, already have Docker client | Polling overhead, envoy becomes bottleneck |
| **B) Sidecar exposes /stats** | Distributed load, sidecar can cache | Requires sidecar implementation, network calls |
| **C) Docker exec + read /proc** | Works without stats API | Hacky, slow exec overhead |

**Recommendation:** **Option A - Docker Stats API**

Envoy already has a Docker client. Add a `ContainerStats()` method:

```go
// internal/envoy/docker.go
func (c *DockerClient) ContainerStats(ctx context.Context, containerID string) (*ContainerStats, error) {
    stats, err := c.client.ContainerStats(ctx, containerID, false) // false = one-shot, not stream
    // Parse stats.Body JSON for memory/cpu
}

type ContainerStats struct {
    MemoryUsed    uint64  `json:"memory_used"`
    MemoryLimit   uint64  `json:"memory_limit"`
    CPUPercent    float64 `json:"cpu_percent"`
}
```

**API endpoint:** Extend `/api/sleeves` response or add `/api/sleeves/{name}/stats`

**Effort:** Medium - Docker stats API parsing is well-documented

---

### 1.3 Container CPU Usage

**Data needed:** CPU percentage (requires two samples to calculate delta)

**Recommendation:** Same as memory - Docker Stats API

CPU calculation requires:
1. Read `cpu_stats.cpu_usage.total_usage` and `system_cpu_usage`
2. Compare with previous sample
3. Calculate: `(container_delta / system_delta) * num_cpus * 100`

**Implementation notes:**
- First call returns 0% (no previous sample)
- Subsequent calls calculate delta from previous
- Consider caching previous stats in SleeveManager

**Effort:** Medium - CPU calculation is slightly complex

---

### 1.4 Stack Integrity

**Data needed:** Metric representing "health" of the cortical stack

**Discussion:** This is a conceptual metric. Options:

| Definition | Implementation |
|------------|----------------|
| % of tasks completed | `(closed / total) * 100` from cstack stats |
| Sidecar health check | Sidecar responds to /health |
| Container health | Docker health check status |
| Composite score | Weighted combination |

**Recommendation:** Use cstack task completion ratio initially

```go
// If cstack exists and has tasks:
integrity = (stats.Closed / stats.Total) * 100
// If no tasks or no cstack:
integrity = 100 // Healthy by default
```

**Alternative:** Define as sidecar responsiveness once sidecar is implemented.

**Effort:** Low - cstack stats already available

---

## 2. Host Resources Panel

**Current State:** `index.html:57-93` has hardcoded values for CPU, memory, disk, Docker count.

### 2.1 Host CPU Usage

**Data needed:** Overall CPU utilization percentage, core count

**Recommendation:** Read from `/proc/stat` (Linux)

```go
// internal/envoy/host_stats.go
func GetHostCPUStats() (*HostCPUStats, error) {
    // Read /proc/stat, parse cpu line
    // Calculate: 100 - (idle_delta / total_delta) * 100
}

type HostCPUStats struct {
    UsagePercent float64 `json:"usage_percent"`
    Cores        int     `json:"cores"`
    Threads      int     `json:"threads"`
}
```

**API endpoint:** `GET /api/host/stats` or `GET /api/system/resources`

**Note:** Envoy runs in a container with host access. Need to read host's `/proc`, not container's. Options:
- Mount host `/proc` to `/host/proc` in envoy container
- Use `docker info` for some metrics

**Effort:** Medium - need host proc access

---

### 2.2 Host Memory Usage

**Data needed:** Used memory, total memory

**Recommendation:** Read from `/proc/meminfo` (Linux)

```go
func GetHostMemoryStats() (*HostMemoryStats, error) {
    // Read /proc/meminfo
    // Parse MemTotal, MemAvailable
    // Used = Total - Available
}

type HostMemoryStats struct {
    UsedBytes  uint64 `json:"used_bytes"`
    TotalBytes uint64 `json:"total_bytes"`
}
```

**Effort:** Low - straightforward parsing

---

### 2.3 Host Disk Usage

**Data needed:** Used disk, total disk (for workspaces volume)

**Recommendation:** Use `syscall.Statfs` or parse `df` output

```go
func GetHostDiskStats(path string) (*HostDiskStats, error) {
    var stat syscall.Statfs_t
    syscall.Statfs(path, &stat)
    total := stat.Blocks * uint64(stat.Bsize)
    free := stat.Bfree * uint64(stat.Bsize)
    used := total - free
}
```

**Path to check:** `/workspaces` mount point in envoy container

**Effort:** Low - standard syscall

---

### 2.4 Docker Container Count

**Data needed:** Running containers, container limit (if any)

**Recommendation:** Use existing Docker client

```go
func (c *DockerClient) GetContainerCount() (running int, total int, err error) {
    containers, _ := c.ListContainers()
    for _, c := range containers {
        if c.State == "running" { running++ }
        total++
    }
    return
}
```

**Limit:** Could be configurable in `envoy.yaml` or derived from Docker daemon info.

**Effort:** Low - already have container list

---

## 2.5 Workspace Size Monitoring

**Use Case:** Detect runaway processes creating massive files (e.g., a qemu.log that grew to 97GB from an unkilled background task).

**Data needed:** Size of each workspace directory in bytes

**Recommendation:** Envoy calculates directly (has access to `/workspaces`)

```go
// internal/envoy/workspace_manager.go
func (m *WorkspaceManager) GetWorkspaceSize(path string) (uint64, error) {
    var size uint64
    err := filepath.WalkDir(path, func(_ string, d fs.DirEntry, err error) error {
        if err != nil {
            return nil // Skip permission errors
        }
        if !d.IsDir() {
            info, err := d.Info()
            if err == nil {
                size += uint64(info.Size())
            }
        }
        return nil
    })
    return size, err
}
```

**Alternative:** Use `du -sb` command for speed on large directories:
```go
func (m *WorkspaceManager) GetWorkspaceSizeFast(path string) (uint64, error) {
    cmd := exec.Command("du", "-sb", path)
    output, err := cmd.Output()
    // Parse: "123456789\t/path/to/workspace\n"
    fields := strings.Fields(string(output))
    return strconv.ParseUint(fields[0], 10, 64)
}
```

### Warning Thresholds

| Level | Threshold | UI Indicator |
|-------|-----------|--------------|
| Normal | < 5 GB | Green/cyan |
| Caution | 5-10 GB | Amber |
| Warning | 10-20 GB | Amber + icon |
| Critical | > 20 GB | Magenta/red + pulsing |

**Configuration** (envoy.yaml):
```yaml
workspaces:
  size_warning_gb: 10
  size_critical_gb: 20
```

### API Response

Extend `WorkspaceInfo` in `/api/workspaces`:
```go
type WorkspaceInfo struct {
    // ... existing fields ...
    SizeBytes    uint64 `json:"size_bytes"`
    SizeWarning  bool   `json:"size_warning"`   // > warning threshold
    SizeCritical bool   `json:"size_critical"`  // > critical threshold
}
```

### WebUI Display

**Workspaces table:** Add "Size" column with color-coded values:
```html
<td class="ws-size warning">12.4 GB</td>
<td class="ws-size critical">47.2 GB</td>
```

**Sleeve cards:** Show workspace size with warning indicator:
```html
<div class="sleeve-row">
    <span class="sleeve-label">Workspace</span>
    <span class="sleeve-value">
        protectorate
        <span class="size-badge warning">12.4 GB</span>
    </span>
</div>
```

### Background Monitoring

Consider a background goroutine that:
1. Checks workspace sizes every 5 minutes
2. Logs warnings to envoy stdout when thresholds exceeded
3. Could eventually trigger alerts or notifications

```go
func (m *WorkspaceManager) StartSizeMonitor(ctx context.Context) {
    ticker := time.NewTicker(5 * time.Minute)
    for {
        select {
        case <-ticker.C:
            workspaces, _ := m.ListWorkspaces()
            for _, ws := range workspaces {
                if ws.SizeBytes > m.config.SizeCriticalBytes {
                    log.Printf("CRITICAL: Workspace %s is %.2f GB", ws.Name, float64(ws.SizeBytes)/1e9)
                }
            }
        case <-ctx.Done():
            return
        }
    }
}
```

### Performance Considerations

- `du -sb` is faster than `filepath.WalkDir` for large directories
- Cache size results for 1-2 minutes (don't recalculate on every API call)
- Consider async calculation - return cached value immediately, update in background
- Skip `.git/objects` or provide option to exclude patterns

**Effort:** Low-Medium - straightforward implementation, caching adds complexity

---

## 3. Needlecast Messaging

**Current State:** Placeholder UI with disabled inputs

**Data needed:**
- List of sleeves (already have)
- Messages from `/needlecast/{sleeve}/INBOX.md`
- Messages from `/needlecast/{sleeve}/OUTBOX.md`
- Global messages from `/needlecast/arena/GLOBAL.md`

### Implementation Plan

**Backend:**
```
GET  /api/needlecast/messages?sleeve={name}     - Get messages for sleeve
GET  /api/needlecast/messages?channel=global    - Get global messages
POST /api/needlecast/messages                   - Send message (envoy writes to target INBOX)
```

**Message format** (parse from markdown):
```go
type NeedlecastMessage struct {
    From      string    `json:"from"`
    To        string    `json:"to"`
    Timestamp time.Time `json:"timestamp"`
    Content   string    `json:"content"`
}
```

**Filesystem structure:**
```
/needlecast/
  alice/
    INBOX.md    <- Messages TO alice (written by envoy or other sleeves)
    OUTBOX.md   <- Messages FROM alice (written by alice's sidecar)
  bob/
    INBOX.md
    OUTBOX.md
  arena/
    GLOBAL.md   <- Broadcast messages
```

**Effort:** High - new feature, needs message parsing, real-time updates

---

## 4. System Logs

**Current State:** Placeholder UI

**Data needed:**
- Envoy logs (stdout/stderr)
- Sleeve container logs (via Docker)

### Implementation Plan

**Backend:**
```
GET /api/logs?source=envoy&lines=100           - Envoy logs
GET /api/logs?source=sleeve&name={name}&lines=100  - Sleeve logs
WebSocket /api/logs/stream?source=...          - Real-time log streaming
```

**Docker logs:**
```go
func (c *DockerClient) ContainerLogs(ctx context.Context, containerID string, tail int) ([]string, error) {
    opts := container.LogsOptions{
        ShowStdout: true,
        ShowStderr: true,
        Tail:       strconv.Itoa(tail),
    }
    reader, _ := c.client.ContainerLogs(ctx, containerID, opts)
    // Parse multiplexed stream
}
```

**Effort:** Medium - Docker log API has multiplexed stream format

---

## 5. Configuration Viewer

**Current State:** Placeholder UI

**Data needed:**
- Current envoy.yaml configuration
- Runtime settings
- Environment variables (sanitized)

### Implementation Plan

**Backend:**
```
GET /api/config                    - Get current configuration
GET /api/config/env                - Get environment info (sanitized)
```

**Security:** Never expose:
- API keys
- Credentials
- Sensitive paths

**Effort:** Low - read and sanitize config file

---

## Implementation Priority

| Priority | Feature | Effort | Impact |
|----------|---------|--------|--------|
| 1 | Sleeve uptime | Low | High - easy win |
| 2 | Docker container count | Low | Medium |
| 3 | **Workspace size monitoring** | Low | **High - prevents disk disasters** |
| 4 | Host disk stats | Low | Medium |
| 5 | Stack integrity (cstack-based) | Low | Medium |
| 6 | Host memory stats | Medium | Medium |
| 7 | Host CPU stats | Medium | Medium |
| 8 | Container memory stats | Medium | High |
| 9 | Container CPU stats | Medium | High |
| 10 | Configuration viewer | Low | Low |
| 11 | System logs | Medium | Medium |
| 12 | Needlecast messaging | High | High |

---

## API Design Summary

### New Endpoints Needed

```
GET  /api/host/stats              - Host CPU, memory, disk
GET  /api/sleeves/{name}/stats    - Container CPU, memory (or extend /api/sleeves)
GET  /api/needlecast/messages     - Needlecast messages
POST /api/needlecast/messages     - Send message
GET  /api/logs                    - Log retrieval
WS   /api/logs/stream             - Log streaming
GET  /api/config                  - Configuration viewer
```

### Extended Existing Endpoints

```
GET /api/sleeves     - Add: spawn_time, cpu_percent, memory_used, memory_limit, integrity
GET /api/workspaces  - Add: size_bytes, size_warning, size_critical
```

---

## Technical Considerations

### Host Stats Access

Envoy runs in a container. To get host stats:

**Option A: Mount host /proc**
```yaml
# docker-compose.yaml
volumes:
  - /proc:/host/proc:ro
```
Then read from `/host/proc/stat`, `/host/proc/meminfo`

**Option B: Docker API**
- `docker info` provides some system info
- Limited compared to direct proc access

**Recommendation:** Mount host /proc for accurate stats

### Polling vs WebSocket

| Data Type | Recommendation |
|-----------|----------------|
| Sleeve list | Poll every 5s (current) |
| Container stats | Poll every 10s or on-demand |
| Host stats | Poll every 30s |
| Logs | WebSocket stream |
| Needlecast | Poll every 5s or WebSocket |

### Caching

Consider caching expensive operations:
- Docker stats (1-2s cache)
- Host CPU (requires delta, keep previous sample)
- Needlecast messages (read files on change)

---

## File Changes Required

### Backend (Go)

| File | Changes |
|------|---------|
| `internal/envoy/docker.go` | Add `ContainerStats()` |
| `internal/envoy/host_stats.go` | New file for host metrics |
| `internal/envoy/workspace_manager.go` | Add `GetWorkspaceSize()`, size thresholds, background monitor |
| `internal/envoy/handlers.go` | Add new API handlers |
| `internal/envoy/server.go` | Register new routes |
| `internal/protocol/types.go` | Add stats structs, extend `WorkspaceInfo` with size fields |
| `configs/envoy.yaml` | Add `size_warning_gb`, `size_critical_gb` settings |
| `docker-compose.yaml` | Mount /proc for host stats |

### Frontend (JS/HTML)

| File | Changes |
|------|---------|
| `app.js` | Replace static values with API calls |
| `app.js` | Add `formatDuration()`, `formatBytes()` helpers |
| `app.js` | Add host stats refresh function |
| `index.html` | Add IDs to host resource elements for JS updates |

---

## Next Steps

1. Start with low-effort, high-impact items (uptime, container count)
2. Implement host stats with /proc mount
3. Add container stats via Docker API
4. Wire up frontend to new APIs
5. Implement logs and needlecast as separate features
