# TODO-NEXT.md

V1 completion tasks for next session.

## Status Summary

**DONE:**
- Container orchestration (spawn/kill/recover sleeves)
- WebUI with terminal access
- Workspace management
- Docker integration (raven network, labels, recovery)
- install.sh / uninstall.sh
- GitHub Actions CI/CD (tested v0.1.0)
- Dev workflow (hot-reload, make targets)

**IN PROGRESS:**
- cortical-stack repo (separate project, will integrate into sleeve)

---

## Remaining V1 Tasks

### 1. Git Integration for Workspaces [HIGH PRIORITY]

Workspaces should be actual git repos, not just directories.

**Requirements:**
- [ ] Clone from URL when spawning a sleeve
- [ ] SpawnRequest gets a `repo_url` field (optional)
- [ ] If `repo_url` provided: clone into workspace
- [ ] If `repo_url` omitted: create empty workspace (current behavior)
- [ ] Workspace naming from repo name (e.g., `github.com/foo/bar` -> `bar`)
- [ ] Handle auth for private repos (SSH keys? GitHub token?)

**Files to modify:**
- `internal/envoy/sleeve_manager.go` - add clone logic to Spawn()
- `internal/envoy/handlers.go` - update SpawnSleeveRequest struct
- `internal/envoy/web/templates/index.html` - add repo URL input to spawn modal

**Open questions:**
- How to handle private repo auth?
- Should we support Gitea repos too? (config already has GiteaConfig stubbed)

---

### 2. Sidecar Implementation [BLOCKED: waiting on cortical-stack repo]

Once .cstack repo is integrated into sleeve image:

**Sidecar binary (`cmd/sidecar/`):**
- [ ] HTTP server on port 8080
- [ ] `GET /health` - basic health check
- [ ] `GET /status` - parse .cstack/CURRENT.md, return JSON
- [ ] `GET /outbox` - parse .needlecast/outbox.md, return messages
- [ ] `POST /resleeve` - soft resleeve (kill/restart CLI in tmux)
- [ ] Process supervision (PID 1, manage tmux/ttyd/CLI)

**Cortical stack parser (`internal/cstack/`):**
- [ ] Parse CURRENT.md for status, task, progress
- [ ] Type definitions for SleeveStatus, CurrentState

**Integration:**
- [ ] Build sidecar binary into sleeve image
- [ ] Update entrypoint.sh to run sidecar as PID 1
- [ ] Sidecar starts tmux/ttyd/CLI as children

---

### 3. Needlecast Messaging

Inter-sleeve communication via filesystem.

**Envoy side:**
- [ ] Poll sidecar `/outbox` endpoints (on poll interval)
- [ ] Route messages: outbox -> target sleeve's inbox
- [ ] Support broadcast to arena/GLOBAL.md
- [ ] Clear processed outbox entries

**Sidecar side:**
- [ ] Parse `.needlecast/outbox.md` (YAML frontmatter + markdown)
- [ ] Return as JSON array via `/outbox` endpoint

**Message format (from SPEC):**
```markdown
---
id: msg-abc123
from: alice
to: bob
type: question
time: 2026-01-21T10:00:00Z
---
Message content here
```

---

### 4. Health Monitoring Loop

Envoy should actively poll sleeves.

**Implementation:**
- [ ] Background goroutine in envoy
- [ ] Poll each sleeve's sidecar `/health` on ENVOY_POLL_INTERVAL
- [ ] Update sleeve status in SleeveManager
- [ ] Detect stuck/unresponsive sleeves
- [ ] Optional: idle timeout enforcement (ENVOY_IDLE_THRESHOLD)

**Files to modify:**
- `internal/envoy/server.go` - start monitor goroutine
- `internal/envoy/sleeve_manager.go` - add status update methods

---

### 5. Soft Resleeve

Swap AI CLI without killing container.

**Sidecar endpoint (`POST /resleeve`):**
- [ ] Kill current CLI in tmux
- [ ] Start new CLI (same or different)
- [ ] Workspace and .cstack/ persist

**Envoy integration:**
- [ ] Add resleeve API endpoint
- [ ] WebUI button for resleeve (optional for V1)

---

### 6. Protocol Package Cleanup [LOW PRIORITY]

Move shared types to `internal/protocol/` for clean separation.

- [ ] Move SleeveInfo, SpawnSleeveRequest, etc.
- [ ] Both envoy and sidecar import from protocol
- [ ] Cleaner dependency graph

---

## Dependency Graph

```
cortical-stack repo
        |
        v
  Sidecar (/status, /outbox)
        |
        +--------+--------+
        |        |        |
        v        v        v
   Needlecast  Health   Resleeve
               Monitor
```

Git integration is independent and can be done in parallel.

---

## Next Session Priority

1. **Git integration** - unblocked, high value
2. **Sidecar** - once cortical-stack repo is ready
3. **Needlecast** - depends on sidecar /outbox
4. **Health monitor** - depends on sidecar /health
