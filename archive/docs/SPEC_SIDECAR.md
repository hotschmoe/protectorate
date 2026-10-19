# Sidecar Specification v1

> Lightweight HTTP server running inside each sleeve container for self-reporting.

## Overview

The sidecar eliminates the need for envoy to exec into containers for status information. Each sleeve reports its own state via a simple HTTP API.

```
+------------------+          +------------------------+
|      ENVOY       |   HTTP   |   SLEEVE CONTAINER     |
|  (orchestrator)  | <------> |  +------------------+  |
|                  |  :8080   |  |     SIDECAR      |  |
|  - spawn/kill    |          |  |  - /health       |  |
|  - workspace ops |          |  |  - /status       |  |
|  - terminal proxy|          |  +------------------+  |
+------------------+          |  |   CLAUDE CODE    |  |
                              |  |   (dtach session)|  |
                              +------------------------+
```

## Design Principles

1. **Simple** - Minimal dependencies, single Go binary
2. **Fast startup** - Ready within 1 second of container start
3. **Low overhead** - Minimal CPU/memory footprint
4. **Stateless** - No persistent state, restarts cleanly
5. **Reporter only** - Does NOT supervise processes (entrypoint.sh does that)

## V1 Scope

### Included (V1)

| Feature | Endpoint | Description |
|---------|----------|-------------|
| Health check | GET /health | Basic liveness |
| DHF detection | GET /status | CLI name + version |
| Cstack stats | GET /status | Task counts from `cs stats` |
| Process stats | GET /status | Memory, uptime from /proc |
| Auth status | GET /status | Credential file presence |

### Excluded (V2+)

| Feature | Reason |
|---------|--------|
| Process supervision | entrypoint.sh handles dtach session |
| Soft resleeve | Requires process control, defer to V2 |
| Terminal WebSocket | Envoy already proxies via docker exec |
| Outbox endpoint | Needlecast routing not ready |
| File browser | Nice-to-have, not critical |

## API Specification

### GET /health

Basic liveness check. Returns immediately if sidecar is running.

**Response:**
```json
{"status": "ok"}
```

**Status codes:**
- 200: Sidecar running
- (no response): Container/sidecar down

### GET /status

Comprehensive sleeve status. Replaces envoy's docker exec calls.

**Response:**
```json
{
  "sleeve_name": "alice",
  "dhf": {
    "name": "claude",
    "version": "2.1.20"
  },
  "workspace": {
    "path": "/home/claude/workspace",
    "cstack": {
      "exists": true,
      "open": 5,
      "ready": 2,
      "in_progress": 1,
      "blocked": 0,
      "closed": 42,
      "total": 50
    }
  },
  "process": {
    "pid": 1,
    "uptime_seconds": 3600,
    "memory_mb": 512
  },
  "auth": {
    "claude": true,
    "gemini": false
  }
}
```

**Field details:**

| Field | Source | Cache TTL |
|-------|--------|-----------|
| sleeve_name | SLEEVE_NAME env var | Forever |
| dhf.name | Detect at startup (claude, gemini, codex) | Forever |
| dhf.version | `<cli> --version` at startup | Forever |
| cstack.* | `cs stats --json` in workspace | 5 seconds |
| process.* | `/proc/self/stat`, `/proc/self/status` | None (always fresh) |
| auth.claude | File exists: `~/.claude/.credentials.json` | 30 seconds |
| auth.gemini | File exists: `~/.gemini/` | 30 seconds |

## Implementation

### Directory Structure

```
cmd/sidecar/
  main.go           # Entry point, HTTP server

internal/sidecar/
  server.go         # HTTP handlers, routing
  dhf.go            # CLI detection (claude, gemini, codex)
  cstack.go         # Run cs stats, parse output
  process.go        # Read /proc for memory/uptime
  auth.go           # Check credential files
```

### Startup Sequence

```
1. Parse environment (SLEEVE_NAME, WORKSPACE_PATH)
2. Detect DHF (try each CLI, cache result)
3. Start HTTP server on :8080
4. Log "sidecar ready"
```

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| SIDECAR_PORT | HTTP listen port | 8080 |
| WORKSPACE_PATH | Mounted workspace | /home/claude/workspace |
| SLEEVE_NAME | Container name | (required) |

### Caching Strategy

| Data | TTL | Reason |
|------|-----|--------|
| DHF name/version | Forever | Doesn't change during sleeve lifetime |
| Cstack stats | 5 seconds | Balance freshness vs cs invocation cost |
| Process stats | None | Always fresh from /proc |
| Auth status | 30 seconds | Files rarely change |

## Integration with Envoy

### Current (Exec-based)

```go
// envoy/docker.go - GetDHFInfo
func (d *DockerClient) GetDHFInfo(ctx context.Context, containerID string) (*DHFInfo, error) {
    // docker exec <container> claude --version
    output, err := d.Exec(ctx, containerID, "claude", "--version")
    // parse output...
}
```

### After (HTTP-based)

```go
// envoy/sidecar.go - new file
func (s *Server) getSidecarStatus(ctx context.Context, containerName string) (*SidecarStatus, error) {
    resp, err := http.Get(fmt.Sprintf("http://%s:8080/status", containerName))
    // parse JSON...
}
```

### Fallback Behavior

If sidecar is unreachable (starting up, crashed):
1. Return partial data (omit DHF/cstack)
2. Mark sleeve status as "degraded"
3. Retry on next poll cycle

## Sleeve Container Changes

### Current entrypoint.sh

```bash
# Already handles:
# - dtach session setup
# - Claude Code execution
# - Session restart on exit
exec sleep infinity  # <-- Replace with sidecar
```

### Updated entrypoint.sh

```bash
# ... existing dtach setup ...

# Start sidecar instead of sleep infinity
exec /usr/local/bin/sidecar
```

### Updated Dockerfile

```dockerfile
# Add sidecar binary
COPY --from=builder /sidecar /usr/local/bin/sidecar
```

## What Stays in Envoy

| Operation | Reason |
|-----------|--------|
| Spawn/Kill containers | Docker API access |
| Terminal gateway | Already works via docker exec + dtach |
| Workspace git ops | Host SSH keys required |
| Resource limits | Set at container creation |
| Needlecast routing | Cross-sleeve coordination |

## What Moves to Sidecar

| Operation | Before (Envoy) | After (Sidecar) |
|-----------|----------------|-----------------|
| DHF detection | Exec `claude --version` | GET /status |
| Cstack stats | Exec `cs stats --json` | GET /status |
| Memory usage | Docker stats API | GET /status |
| Auth status | Check host files | GET /status |

**Benefits:**
- Faster /api/sleeves response (no exec per sleeve)
- More accurate resource stats (from inside cgroup)
- Reduced Docker API load
- Sleeve self-awareness for future features

## Testing

```bash
# Build and test locally
go build -o bin/sidecar ./cmd/sidecar
SLEEVE_NAME=test WORKSPACE_PATH=/tmp ./bin/sidecar &
curl http://localhost:8080/health
curl http://localhost:8080/status | jq

# Integration test (requires running sleeve)
curl http://sleeve-alice:8080/health
curl http://sleeve-alice:8080/status | jq
```

## Future Extensions (V2+)

### Outbox Endpoint
```
GET /outbox
POST /outbox/clear
```
For needlecast message retrieval.

### Soft Resleeve
```
POST /resleeve
Body: {"cli": "gemini"}
```
Kill current CLI, start new one in dtach session.

### Task Status
```
GET /tasks
```
Real-time task list from cstack.

### Metrics Endpoint
```
GET /metrics
```
Prometheus-format metrics.
