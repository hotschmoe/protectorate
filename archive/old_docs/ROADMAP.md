# Cortical Roadmap

## Overview

This roadmap is organized into phases, each building on the previous. The goal is to have a usable system as early as possible, then iterate.

**Principles:**
- Ship something minimal that works before adding features
- Each phase should be independently useful
- Defer complexity until it's clearly needed
- Prefer boring solutions that work over clever ones that might

---

## Phase 0: Foundation

**Goal:** Project scaffolding and core libraries. Nothing runs yet, but the bones are in place.

**Duration:** 1-2 days

### Milestones

#### 0.1 Project Scaffolding
- [ ] Initialize Go module (`go mod init github.com/yourorg/cortical`)
- [ ] Create directory structure per README
- [ ] Set up Makefile with standard targets (`build`, `test`, `lint`, `run`)
- [ ] Create `.env.example` with required variables
- [ ] Add `.gitignore` (workspaces/, *.env, binaries)
- [ ] Add LICENSE (MIT)

#### 0.2 Shared Libraries
- [ ] `internal/shared/stack/` — Read/write stack files
  - [ ] `ParseCurrent(path) -> CurrentState`
  - [ ] `ParsePlan(path) -> PlanState`
  - [ ] `WriteInbox(path, message)`
  - [ ] `ReadOutbox(path) -> []Message`
  - [ ] `ClearOutbox(path)`
- [ ] `internal/shared/protocol/` — Shared types
  - [ ] `AgentStatus` enum (idle, working, blocked, done)
  - [ ] `Message` struct (from, to, type, content, timestamp)
  - [ ] `SpawnRequest`, `AgentInfo`, etc.
- [ ] `internal/shared/config/` — Configuration loading
  - [ ] Load YAML configs
  - [ ] Environment variable substitution

#### 0.3 Context-by-md Integration
- [ ] Fork/vendor stack or add as submodule
- [ ] Add `INBOX.md` and `OUTBOX.md` templates
- [ ] Update `CLAUDE.md` template with Cortical-specific instructions
- [ ] Create `scripts/install-context-md.sh`

### Deliverables
- Compiles with `go build ./...`
- Tests pass with `go test ./...`
- Can parse sample stack files

---

## Phase 1: Sidecar

**Goal:** A working sidecar that can report agent status. No manager yet — test manually.

**Duration:** 2-3 days

### Milestones

#### 1.1 Basic HTTP Server
- [ ] `cmd/sidecar/main.go` — Entry point
- [ ] `internal/sidecar/server.go` — HTTP server setup
- [ ] `GET /health` — Returns `{"status": "healthy", "uptime": ...}`
- [ ] Configuration via environment variables
  - [ ] `SIDECAR_PORT` (default: 8080)
  - [ ] `WORKSPACE_PATH` (default: /workspace)
  - [ ] `AGENT_ID` (required)

#### 1.2 Context Parsing Endpoints
- [ ] `GET /status` — Parse CURRENT.md and PLAN.md, return structured status
  ```json
  {
    "agent_id": "agent-xyz",
    "status": "working",
    "current_task": "Implementing OAuth",
    "progress": {"total": 5, "completed": 2},
    "blockers": [],
    "last_modified": "2025-01-15T10:30:00Z"
  }
  ```
- [ ] `GET /outbox` — Return messages from OUTBOX.md, clear after read
- [ ] `POST /directive` — Write message to INBOX.md

#### 1.3 Ask Endpoint (Basic)
- [ ] `POST /ask` — For now, just append question to INBOX.md
- [ ] Response: `{"acknowledged": true, "note": "Question written to INBOX.md"}`
- [ ] Future: Interactive response via CLI (Phase 4)

#### 1.4 Agent Container (Claude Code)
- [ ] `containers/agent/Dockerfile.claude-code`
  ```dockerfile
  FROM ubuntu:22.04
  # Install dependencies
  # Install Claude Code CLI
  # Copy sidecar binary
  # Copy entrypoint
  ```
- [ ] `containers/agent/entrypoint.sh`
  - [ ] Start sidecar in background
  - [ ] Launch Claude Code with workspace
  - [ ] Handle shutdown gracefully

#### 1.5 Manual Testing
- [ ] Docker build agent image
- [ ] Run container with sample workspace
- [ ] Verify all sidecar endpoints work
- [ ] Verify Claude Code can read/write stack

### Deliverables
- `cortical-sidecar` binary
- `cortical-agent:claude-code` Docker image
- Can run standalone agent container, query status via HTTP

---

## Phase 2: Manager Core

**Goal:** Manager can spawn agents, track them, and provide status. No scheduling yet.

**Duration:** 3-4 days

### Milestones

#### 2.1 Basic HTTP Server
- [ ] `cmd/manager/main.go` — Entry point
- [ ] `internal/manager/server.go` — HTTP server with router
- [ ] `GET /health` — Manager health
- [ ] Configuration loading from `configs/manager.yaml`

#### 2.2 Docker Integration
- [ ] `internal/manager/docker.go` — Docker client wrapper
- [ ] `SpawnAgent(SpawnRequest) -> Agent`
  - [ ] Clone repo to workspace
  - [ ] Install stack
  - [ ] Write initial INBOX.md with goal
  - [ ] Create and start container
- [ ] `StopAgent(agentID)`
- [ ] `ListContainers()` — Find running cortical agents
- [ ] `InspectAgent(agentID)` — Container stats

#### 2.3 Agent Tracking
- [ ] `internal/manager/agents.go` — In-memory agent registry
- [ ] Track: agent ID, container ID, status, spawn time, last check-in
- [ ] Persist to `manager-state.json` for restart recovery

#### 2.4 Agent CRUD Endpoints
- [ ] `POST /agents` — Spawn new agent
  ```json
  {
    "repo": "https://github.com/user/project",
    "goal": "Add feature X",
    "cli": "claude-code",
    "priority": "P1"
  }
  ```
- [ ] `GET /agents` — List all agents with summary status
- [ ] `GET /agents/{id}` — Detailed status (calls sidecar /status)
- [ ] `DELETE /agents/{id}` — Stop and remove agent

#### 2.5 Manager Container
- [ ] `containers/manager/Dockerfile`
- [ ] Mount Docker socket
- [ ] Mount workspaces directory
- [ ] Mount configs directory

#### 2.6 Docker Compose
- [ ] `docker-compose.yml` with manager service
- [ ] `docker-compose.dev.yml` with dev overrides
- [ ] Network configuration (cortical-net)

### Deliverables
- `cortical-manager` binary and Docker image
- Can spawn agents via API
- Can list and inspect agents
- Can wind down agents
- `docker-compose up` starts the system

---

## Phase 3: Scheduling & Communication

**Goal:** Manager performs check-ins, routes messages, proposes winddowns.

**Duration:** 3-4 days

### Milestones

#### 3.1 Scheduler
- [ ] `internal/manager/scheduler.go` — Periodic task runner
- [ ] Configurable poll interval (default: 1 hour)
- [ ] Check-in task: query all agent sidecars
- [ ] Idle detection: track last activity time

#### 3.2 Check-in Logic
- [ ] For each agent:
  - [ ] Call `GET /status`
  - [ ] Call `GET /outbox`, process messages
  - [ ] Update agent registry with latest status
  - [ ] Log activity
- [ ] Handle unreachable agents (mark degraded, retry)

#### 3.3 Inter-Agent Messaging
- [ ] `internal/manager/router.go` — Message routing
- [ ] Message types:
  - [ ] `milestone` — Agent completed something
  - [ ] `blocked` — Agent needs something from another agent
  - [ ] `question` — Agent asking for input
  - [ ] `done` — Agent completed its goal
- [ ] Routing logic:
  - [ ] If message is `blocked` on another agent, write to that agent's INBOX.md
  - [ ] If message is `done`, mark agent as complete
  - [ ] If message needs user input, queue for notification

#### 3.4 Winddown Proposals
- [ ] Track idle time per agent
- [ ] When idle > threshold, create winddown proposal
- [ ] `GET /proposals` — List pending winddown proposals
- [ ] `POST /proposals/{id}/approve` — Execute winddown
- [ ] `POST /proposals/{id}/reject` — Keep agent running

#### 3.5 Activity Log
- [ ] `internal/manager/log.go` — Append-only activity log
- [ ] Log: spawns, check-ins, messages, completions, errors
- [ ] `GET /log` — Return recent activity
- [ ] `GET /log?agent={id}` — Filter by agent

### Deliverables
- Manager automatically checks in with agents
- Messages routed between dependent agents
- Idle agents flagged for winddown
- Activity log queryable via API

---

## Phase 4: Notifications

**Goal:** User gets notified of important events via Slack/SMS.

**Duration:** 2-3 days

### Milestones

#### 4.1 Notification Interface
- [ ] `internal/manager/notifier.go` — Notification dispatcher
- [ ] Interface: `Notifier.Send(event Event)`
- [ ] Event types: `agent_spawned`, `agent_blocked`, `agent_done`, `winddown_proposed`, `error`

#### 4.2 Slack Integration
- [ ] Slack webhook client
- [ ] Message formatting (blocks, attachments)
- [ ] Configuration in `manager.yaml`
- [ ] Test with sample events

#### 4.3 SMS Integration (Twilio)
- [ ] Twilio client
- [ ] Terse message formatting (SMS length limits)
- [ ] Configuration in `manager.yaml`
- [ ] Test with sample events

#### 4.4 Notification Rules
- [ ] Configurable: which events trigger which channels
- [ ] Example:
  ```yaml
  notifications:
    rules:
      - event: agent_done
        channels: [slack, sms]
      - event: agent_blocked
        channels: [slack]
      - event: error
        channels: [slack, sms]
  ```

#### 4.5 User Commands via Slack (Optional)
- [ ] Slack slash commands or bot mentions
- [ ] `/cortical status` — List agents
- [ ] `/cortical spawn <repo> "<goal>"` — Spawn agent
- [ ] `/cortical approve <proposal-id>` — Approve winddown

### Deliverables
- Slack notifications for agent events
- SMS notifications for critical events
- Optional: Control Cortical from Slack

---

## Phase 5: Build Service

**Goal:** Dedicated build container for resource-heavy compilation.

**Duration:** 2-3 days

### Milestones

#### 5.1 Build Service Container
- [ ] `containers/build-service/Dockerfile`
- [ ] Install toolchains: Rust, Zig, Go, Node
- [ ] Cache directories as volumes
- [ ] Resource limits in compose (4+ CPU, 8+ GB RAM)

#### 5.2 Build API
- [ ] Simple HTTP API on build service
- [ ] `POST /build`
  ```json
  {
    "workspace": "/cortical/workspaces/agent-xyz",
    "command": "cargo build --release",
    "timeout": "10m"
  }
  ```
- [ ] Returns build output, exit code
- [ ] Streaming logs (optional, nice-to-have)

#### 5.3 Agent Integration
- [ ] Sidecar helper: `RequestBuild(command) -> BuildResult`
- [ ] Document how agents can request builds
- [ ] Update CLAUDE.md template with build service instructions

### Deliverables
- Build service container running
- Agents can offload heavy builds
- Build caches persist across builds

---

## Phase 6: Additional AI CLIs

**Goal:** Support for AI CLIs beyond Claude Code.

**Duration:** 1-2 days per CLI

### Milestones

#### 6.1 OpenCode Support
- [ ] `containers/agent/Dockerfile.opencode`
- [ ] `configs/agents/opencode.yaml`
- [ ] Test spawn, status, completion flow
- [ ] Document any CLI-specific quirks

#### 6.2 Gemini CLI Support
- [ ] `containers/agent/Dockerfile.gemini-cli`
- [ ] `configs/agents/gemini-cli.yaml`
- [ ] Test full flow

#### 6.3 Codex CLI Support
- [ ] `containers/agent/Dockerfile.codex-cli`
- [ ] `configs/agents/codex-cli.yaml`
- [ ] Test full flow

#### 6.4 Aider Support (Optional)
- [ ] `containers/agent/Dockerfile.aider`
- [ ] `configs/agents/aider.yaml`
- [ ] Test full flow

### Deliverables
- Multiple CLI options available
- User can specify CLI in spawn request
- All CLIs work with stack protocol

---

## Phase 7: Web Dashboard

**Goal:** Visual interface for monitoring and managing agents.

**Duration:** 1-2 weeks

### Milestones

#### 7.1 Dashboard Container
- [ ] Choose framework (recommendation: SvelteKit or Next.js)
- [ ] `containers/dashboard/Dockerfile`
- [ ] Add to docker-compose

#### 7.2 Core Views
- [ ] Agent list — Status overview of all agents
- [ ] Agent detail — Full status, CURRENT.md preview, activity
- [ ] Activity feed — Real-time log
- [ ] Spawn form — Create new agent

#### 7.3 Agent Interaction
- [ ] Send message to agent
- [ ] View INBOX/OUTBOX contents
- [ ] Approve/reject winddown proposals
- [ ] Force checkpoint

#### 7.4 Metrics (Optional)
- [ ] Agent uptime
- [ ] Tasks completed
- [ ] Time per task
- [ ] Resource usage

### Deliverables
- Web UI accessible at `http://localhost:7480`
- Visual management of all agents
- Real-time status updates

---

## Future Features

### Multi-Machine Deployment (WireGuard)

**Status:** Stretch goal

**Goal:** Run agent containers across multiple physical machines.

**Approach:**
- WireGuard mesh network connecting all machines
- Manager runs on one machine, agents can run anywhere
- Docker contexts or Docker Swarm for remote container management
- Shared workspace via NFS or Syncthing

**Milestones:**
- [ ] `scripts/setup-wireguard.sh` — Configure WireGuard peer
- [ ] Document multi-machine setup
- [ ] Test cross-machine agent spawning
- [ ] Handle network partitions gracefully

---

### Agent Deployment Pipeline

**Status:** Future

**Goal:** Agents can deploy their work to staging/production.

**Approach:**
- Integrate with existing CI/CD (GitHub Actions, GitLab CI)
- Or: Cortical-native deployment via Traefik + Docker
- Agent completes work → Manager triggers deploy → New container with app

**Considerations:**
- Security: Agent containers shouldn't have deploy credentials directly
- Manager acts as deploy proxy
- Approval workflow for production deploys

---

### Persistent Agent Sessions

**Status:** Future

**Goal:** True session persistence — AI CLI conversation history survives restarts.

**Challenge:** Most AI CLIs don't support session serialization natively.

**Approaches:**
1. **CLI-specific hacks** — If Claude Code adds session export, use it
2. **Wrapper approach** — Log all CLI I/O, replay on restart
3. **Accept ephemeral** — Rely on stack for state (current approach)

**Recommendation:** Defer until a CLI supports this natively. Context-by-md provides 80% of the value.

---

### Agent Reviewer / Code Review Flow

**Status:** Future

**Goal:** Automated code review before merge.

**Approach:**
- Dedicated "reviewer" agent (could be same or different CLI)
- Coding agent completes PR → Reviewer agent spawned → Reviews code
- Feedback routed back to coding agent or to user

**Considerations:**
- Adversarial setup reduces blind spots
- Cost: 2x LLM usage per change
- Could use cheaper model for review

---

### Research/RAG Agent

**Status:** Future

**Goal:** Specialized agent for documentation lookup, codebase search, web research.

**Approach:**
- Agent with access to vector DB, web search, documentation
- Other agents can request research via inter-agent messaging
- Centralizes context gathering, reduces redundant searches

**Considerations:**
- May not be needed if using Claude Code (has built-in web search, MCP)
- More valuable for open-source CLIs without these features

---

### Resource-Aware Scheduling

**Status:** Future

**Goal:** Manager considers host resources when spawning agents.

**Approach:**
- Monitor host CPU/memory usage
- Queue spawn requests if resources exhausted
- Prioritize by agent priority field
- Consider time-of-day scheduling (run heavy work overnight)

---

### Git Workflow Integration

**Status:** Future

**Goal:** Manager understands Git workflows (branches, PRs, merges).

**Features:**
- Auto-create branch when spawning agent
- PR creation on completion
- Conflict detection and resolution workflow
- Integration with GitHub/GitLab APIs

---

### Secrets Management

**Status:** Future

**Goal:** Secure handling of API keys, credentials.

**Options:**
1. **Environment variables** — Current approach, simple but limited
2. **Docker secrets** — Better for Swarm deployments
3. **HashiCorp Vault** — Enterprise-grade, probably overkill
4. **SOPS/age** — Encrypted secrets in repo

**Recommendation:** Start with env vars, add Docker secrets when moving multi-machine.

---

### Audit Trail

**Status:** Future

**Goal:** Complete record of all agent actions for compliance/debugging.

**Features:**
- Log all file changes made by agents
- Log all commands executed
- Log all API calls (manager, sidecar)
- Retention policy configuration

---

## Technology Recommendations

### For Phase 7 (Dashboard)

| Option | Pros | Cons | Recommendation |
|--------|------|------|----------------|
| **SvelteKit** | Fast, lightweight, good DX | Smaller ecosystem | **Recommended** |
| **Next.js** | Huge ecosystem, React familiar | Heavier, more complex | Good alternative |
| **HTMX + Go templates** | No JS build, simple | Less interactive | If you hate JS |
| **Grafana** | Ready-made dashboards | Less customizable | For metrics only |

### For Multi-Machine (Future)

| Option | Pros | Cons | Recommendation |
|--------|------|------|----------------|
| **WireGuard** | Simple, fast, secure | Manual setup | **Recommended** |
| **Tailscale** | Managed WireGuard, easy | Dependency on service | Alternative |
| **Docker Swarm** | Native Docker clustering | Limited features | If staying Docker-native |
| **Kubernetes** | Industry standard | Massive complexity | Overkill for this |

### For Secrets (Future)

| Option | Pros | Cons | Recommendation |
|--------|------|------|----------------|
| **Env vars** | Simple, works now | No rotation, visible in inspect | **Start here** |
| **Docker secrets** | Better isolation | Swarm-only | When multi-machine |
| **1Password CLI** | Good UX, team-friendly | Dependency | If already using |
| **Vault** | Enterprise features | Complex setup | If compliance requires |

### For CI/CD Integration (Future)

| Option | Pros | Cons | Recommendation |
|--------|------|------|----------------|
| **GitHub Actions** | Native to GitHub | GitHub lock-in | If using GitHub |
| **GitLab CI** | Native to GitLab | GitLab lock-in | If using GitLab |
| **Dagger** | Container-native CI | Newer, less docs | Interesting option |
| **Cortical-native** | Full control | Build it yourself | Long-term goal |

---

## Version Milestones

### v0.1.0 — "First Sleeve"
- [ ] Phase 0 complete
- [ ] Phase 1 complete
- [ ] Can run standalone agent with sidecar

### v0.2.0 — "The Stack"
- [ ] Phase 2 complete
- [ ] Manager can spawn and track agents
- [ ] Basic docker-compose deployment

### v0.3.0 — "Needlecast"
- [ ] Phase 3 complete
- [ ] Automated check-ins
- [ ] Inter-agent messaging works

### v0.4.0 — "Envoy"
- [ ] Phase 4 complete
- [ ] Slack/SMS notifications
- [ ] User can monitor remotely

### v0.5.0 — "Forge"
- [ ] Phase 5 complete
- [ ] Build service operational

### v0.6.0 — "Protectorate"
- [ ] Phase 6 complete
- [ ] Multiple AI CLI support

### v1.0.0 — "Commonwealth"
- [ ] Phase 7 complete
- [ ] Web dashboard
- [ ] Production-ready

---

## Contributing

Contributions welcome! See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

Priority areas:
1. Testing — Unit tests, integration tests, e2e tests
2. Documentation — Improve setup guides, add examples
3. CLI support — Add new AI CLI integrations
4. Dashboard — Frontend development

---

## Changelog

### Unreleased
- Initial project structure
- README and ROADMAP created
