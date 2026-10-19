# Component Responsibilities

This document defines the clear separation of concerns between Protectorate components.

## Architecture Overview

```
+------------------+     HTTP API      +------------------+
|                  |<------------------|                  |
|      ENVOY       |    (sidecar)      |      SLEEVE      |
|   (Coordinator)  |------------------>|   (AI Container) |
|                  |                   |                  |
+--------+---------+                   +--------+---------+
         |                                      |
         | Docker API                           | /proc, filesystem
         v                                      v
+------------------+                   +------------------+
|     DOCKER       |                   |  IN-CONTAINER    |
|     DAEMON       |                   |     STATE        |
+------------------+                   +------------------+
```

## Envoy Responsibilities

Envoy is the central coordinator that manages sleeves and provides the web UI.

### Container Lifecycle (via Docker API)

| Operation | Source | Why Docker API |
|-----------|--------|----------------|
| Spawn sleeve | `POST /api/sleeves` | Create container, configure mounts, set limits |
| Kill sleeve | `DELETE /api/sleeves/{name}` | Stop and remove container |
| List containers | Docker socket | Discover running sleeves on startup |
| Container status | Docker API | Running/stopped/exited state |
| Resource stats | `docker stats` | CPU%, memory% (kernel-level metrics) |

### Workspace Management (direct filesystem)

| Operation | Source | Why Filesystem |
|-----------|--------|----------------|
| List workspaces | `os.ReadDir` | Envoy has direct access to workspace root |
| Git operations | `git` CLI | Envoy runs git for fetch/pull/push |
| Cstack init | `cs init` | Run before sleeve spawns |
| Clone repos | `git clone` | Async job with progress tracking |

### Aggregated State (via Sidecar API)

| Data | Source | Endpoint |
|------|--------|----------|
| DHF name/version | Sidecar | `GET /status` |
| Cstack stats | Sidecar | `GET /status` |
| Sidecar health | Sidecar | `GET /health` |
| Process uptime | Sidecar | `GET /status` |

## Sidecar Responsibilities

Sidecar runs inside each sleeve container, exposing in-container state via HTTP.

### In-Container State (what only the sleeve can see)

| Data | How Gathered | Cache TTL |
|------|--------------|-----------|
| DHF detection | `claude --version`, `gemini --version`, etc. | Forever (detect once) |
| Cstack stats | `cs stats --json` | 5 seconds |
| Process info | `/proc/self/status` | None (instant) |
| Auth status | Check credential files exist | 30 seconds |

### Why Sidecar (not Docker exec)

| Concern | Docker Exec | Sidecar HTTP |
|---------|-------------|--------------|
| Latency | ~200-500ms per call | ~5-20ms |
| Parallelism | Sequential (docker socket contention) | Fully parallel |
| Security | Shell injection risk | Typed JSON API |
| Caching | None (stateless exec) | In-process TTL cache |
| Reliability | Fails if container busy | Always responsive |

### Endpoints

| Endpoint | Response | Use Case |
|----------|----------|----------|
| `GET /health` | `{"status": "ok"}` | Liveness probe, quick health check |
| `GET /status` | Full status object | DHF, cstack, process, auth |

### Future Endpoints (planned)

| Endpoint | Purpose |
|----------|---------|
| `GET /outbox` | Needlecast messages from sleeve |
| `POST /resleeve` | Switch CLI tool (soft resleeve) |

## Docker API Responsibilities

Docker provides container-level metrics that require kernel access.

### What Docker API Provides

| Data | Why Docker |
|------|------------|
| CPU usage % | cgroups metrics (kernel-level) |
| Memory usage/limit | cgroups memory controller |
| Container state | Running/stopped/paused |
| Network info | Container networking stack |
| Logs | Container stdout/stderr |

### What Docker API Does NOT Provide

| Data | Why Not | Solution |
|------|---------|----------|
| CLI version | Not a container property | Sidecar |
| Cstack state | Application-level | Sidecar |
| Auth status | File checks inside container | Sidecar |
| Process uptime | Process inside container | Sidecar |

## Data Flow Examples

### Listing Sleeves with Full Status

```
1. Envoy: docker.ListSleeveContainers()
   -> Get container IDs, names, status

2. Envoy: sidecar.BatchGetStatus(containerNames)
   -> Parallel HTTP calls to each sleeve's sidecar
   -> Get DHF, cstack, process, auth for each

3. Envoy: docker.GetContainerStats(containerID)
   -> Get CPU%, memory% for each container

4. Envoy: Merge data into SleeveInfo response
```

### Spawning a Sleeve

```
1. Envoy: Validate workspace exists
2. Envoy: Reserve name + workspace (atomic)
3. Envoy: docker.CreateContainer() with mounts
4. Envoy: docker.StartContainer()
5. Sleeve: entrypoint.sh runs
6. Sleeve: dtach creates shell session
7. Sleeve: sidecar starts on :8080
8. Sidecar: Detects DHF on first /status call
```

### Getting Cstack Stats

```
Option A: From workspace (envoy has direct access)
  Envoy -> getCstackInfo(workspacePath) -> cs stats --json

Option B: From sidecar (sleeve is running)
  Envoy -> GET sleeve:8080/status -> cstack stats in response
```

## Summary Table

| Component | Sees | Responsible For |
|-----------|------|-----------------|
| Envoy | Docker socket, workspace filesystem | Container lifecycle, workspace ops, aggregation |
| Sidecar | In-container /proc, filesystem | DHF detection, cstack, process stats, auth |
| Docker | Kernel cgroups, networking | CPU/memory metrics, container state |
