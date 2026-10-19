# Cortical

**Container-native AI agent orchestration system.**

Named after the cortical stacks from Altered Carbon — containers are sleeves, agents are the consciousness. When a container dies and respawns, it resleeves with memory intact.

## Philosophy

- **Containers as isolation boundaries** — Each agent operates in its own environment with its own toolchain
- **Filesystem as database** — Agent state lives in plain markdown files (git-friendly, human-readable, debuggable)
- **Protocol over framework** — Simple HTTP APIs and file conventions that any AI CLI can follow
- **Manager as coordinator, not controller** — Agents are autonomous; manager provides oversight, routing, and lifecycle management

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│  User (Slack / SMS / Web UI / CLI)                              │
└─────────────────────────┬───────────────────────────────────────┘
                          │
┌─────────────────────────▼───────────────────────────────────────┐
│  cortical-manager                                               │
│  • Spawns/kills agent containers via Docker API                 │
│  • Hourly check-ins with agents                                 │
│  • Routes inter-agent messages                                  │
│  • Notifies user of progress, blockers, completions             │
│  • Proposes winddown of idle agents                             │
└─────────────────────────┬───────────────────────────────────────┘
                          │
        ┌─────────────────┼─────────────────┬─────────────────┐
        ▼                 ▼                 ▼                 ▼
┌──────────────┐  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐
│ agent-xyz    │  │ agent-abc    │  │ cortical-    │  │ cortical-    │
│ (claude-code)│  │ (opencode)   │  │ build        │  │ dashboard    │
│              │  │              │  │              │  │ (future)     │
│ ┌──────────┐ │  │ ┌──────────┐ │  │ Dedicated    │  │              │
│ │ sidecar  │ │  │ │ sidecar  │ │  │ build        │  │              │
│ │ :8080    │ │  │ │ :8080    │ │  │ resources    │  │              │
│ └──────────┘ │  │ └──────────┘ │  │              │  │              │
│ ┌──────────┐ │  │ ┌──────────┐ │  │              │  │              │
│ │ AI CLI   │ │  │ │ AI CLI   │ │  │              │  │              │
│ └──────────┘ │  │ └──────────┘ │  │              │  │              │
└──────────────┘  └──────────────┘  └──────────────┘  └──────────────┘
```

## Core Components

### Manager (`cortical-manager`)

The central coordinator. Runs as a single container with access to the Docker socket.

**Responsibilities:**
- Receive user requests (spawn agent, check status, send message)
- Create and destroy agent containers dynamically
- Perform hourly check-ins with all agents
- Route messages between agents (for cross-repo dependencies)
- Maintain activity log for user queries
- Send notifications via Slack/SMS
- Propose winddown of idle/completed agents

**Does NOT:**
- Micromanage agent work
- Make decisions for agents
- Require agents to ask permission

### Sidecar (`cortical-sidecar`)

A lightweight HTTP server baked into every agent container. Provides a standardized interface regardless of which AI CLI is running.

**Responsibilities:**
- Expose `/health`, `/status`, `/ask`, `/outbox` endpoints
- Parse `.stack/` files to report agent state
- Write incoming directives to `INBOX.md`

### Agent Containers

Docker containers running an AI CLI (Claude Code, OpenCode, Gemini CLI, Codex CLI) pointed at a workspace.

**Contains:**
- Base OS (Ubuntu/Alpine)
- AI CLI tool (configurable)
- Sidecar binary
- Git, language runtimes as needed
- Mounted workspace with `.stack/` directory

### Build Service (`cortical-build`)

Optional dedicated container with extra resources for compilation-heavy tasks.

**Use case:** Rust/Zig projects where `cargo build` would bog down an agent's container. Agents can request builds without blocking their environment.

## State Management

Agent state is managed through stack files, a lightweight markdown-based state system.

```
workspace/
├── .stack/
│   ├── CURRENT.md      # Active task, current focus, next steps
│   ├── PLAN.md         # Task list, backlog, notes
│   ├── INBOX.md        # Messages FROM manager/other agents
│   ├── OUTBOX.md       # Messages TO manager (read and cleared)
│   └── QUICKREF.md     # Command reference
├── CLAUDE.md           # AI CLI instructions (reads stack)
└── (repo files)
```

**Why markdown files?**
- Human-readable: `cat CURRENT.md` shows exactly what the agent thinks it's doing
- Git-friendly: State changes are trackable, diffable, revertable
- Crash-resilient: Container dies, respawns, reads files, continues
- Tool-agnostic: Any AI CLI that can read files can participate

## Agent Lifecycle

### 1. Spawn

User sends request to manager:
```json
{
  "repo": "https://github.com/user/project",
  "goal": "Add OAuth support with Google and GitHub providers",
  "cli": "claude-code",
  "priority": "P1"
}
```

Manager actions:
1. Clone/fork repo to `/cortical/workspaces/agent-{id}`
2. Install stack files into workspace
3. Write goal to `INBOX.md`
4. Create and start agent container
5. Return agent ID to user

### 2. Boot

1. Sidecar starts, begins serving HTTP on `:8080`
2. AI CLI launches, reads `CLAUDE.md`
3. AI reads `INBOX.md`, sees directive from manager
4. AI enters planning mode, creates tasks in `PLAN.md`
5. AI writes acknowledgment to `OUTBOX.md`

### 3. Work

1. AI works through tasks, updates `CURRENT.md` continuously
2. Sidecar serves status by parsing markdown files
3. Manager polls hourly, reads `OUTBOX.md` for updates
4. If blocked, AI writes to `OUTBOX.md`, manager routes to other agents
5. Checkpoints triggered on session stop or manually

### 4. Complete

1. AI writes completion message to `OUTBOX.md`
2. Manager notifies user: "agent-xyz completed OAuth implementation"
3. User decides: deploy, review PR, wind down
4. Manager executes: triggers deploy pipeline, then stops container

## API Reference

### Manager API (`:7470`)

#### Agents

```
POST   /agents              Spawn new agent
GET    /agents              List all agents
GET    /agents/{id}         Get agent details
POST   /agents/{id}/message Send message to agent
DELETE /agents/{id}         Wind down agent
```

#### System

```
GET    /health              Manager health
GET    /log                 Activity log
POST   /broadcast           Message all agents
```

### Sidecar API (`:8080`)

```
GET    /health              Container/CLI health
GET    /status              Current task, progress, blockers
POST   /ask                 Send question, get response
POST   /directive           Write to INBOX.md
GET    /outbox              Read and clear OUTBOX.md
```

## Configuration

### Manager (`configs/manager.yaml`)

```yaml
poll_interval: 1h              # How often to check in with agents
idle_threshold: 2h             # Propose winddown after this idle time
max_agents: 10                 # Maximum concurrent agents

notifications:
  slack:
    webhook_url: ${SLACK_WEBHOOK_URL}
    channel: "#cortical"
  sms:
    provider: twilio
    from: "+1234567890"
    to: "+0987654321"

docker:
  network: cortical-net
  workspace_root: /cortical/workspaces
```

### Agent Templates (`configs/agents/`)

```yaml
# configs/agents/claude-code.yaml
name: claude-code
image: cortical-agent:claude-code
cli_command: claude --dangerously-skip-permissions
env:
  - ANTHROPIC_API_KEY=${ANTHROPIC_API_KEY}
resources:
  memory: 4G
  cpus: 2
```

## Quick Start

### Prerequisites

- Docker and Docker Compose
- API keys for your chosen AI CLI(s)

### Installation

```bash
git clone https://github.com/yourorg/cortical.git
cd cortical

# Copy and configure environment
cp .env.example .env
# Edit .env with your API keys

# Build containers
make build

# Start manager and build service
make up
```

### Spawn Your First Agent

```bash
# Via CLI
curl -X POST http://localhost:7470/agents \
  -H "Content-Type: application/json" \
  -d '{
    "repo": "https://github.com/you/your-project",
    "goal": "Add user authentication with JWT tokens",
    "cli": "claude-code"
  }'

# Response
{
  "agent_id": "agent-a1b2c3",
  "status": "spawning",
  "workspace": "/cortical/workspaces/agent-a1b2c3"
}
```

### Check Status

```bash
# All agents
curl http://localhost:7470/agents

# Specific agent
curl http://localhost:7470/agents/agent-a1b2c3

# Direct to sidecar (if you need raw status)
curl http://localhost:7470/agents/agent-a1b2c3/proxy/status
```

### Send a Message

```bash
curl -X POST http://localhost:7470/agents/agent-a1b2c3/message \
  -H "Content-Type: application/json" \
  -d '{"message": "Prioritize the login endpoint, we need it for a demo"}'
```

### Wind Down

```bash
curl -X DELETE http://localhost:7470/agents/agent-a1b2c3
```

## Project Structure

```
cortical/
├── cmd/
│   ├── manager/
│   │   └── main.go
│   └── sidecar/
│       └── main.go
│
├── internal/
│   ├── manager/
│   │   ├── server.go        # HTTP API
│   │   ├── docker.go        # Container lifecycle
│   │   ├── scheduler.go     # Hourly check-ins
│   │   ├── notifier.go      # Slack/SMS
│   │   ├── router.go        # Inter-agent message routing
│   │   └── agents.go        # Agent state tracking
│   │
│   ├── sidecar/
│   │   ├── server.go        # HTTP API
│   │   └── context.go       # stack parser
│   │
│   └── shared/
│       ├── stack/           # stack read/write
│       ├── protocol/        # Shared types
│       └── config/          # Configuration loading
│
├── containers/
│   ├── manager/
│   │   └── Dockerfile
│   ├── agent/
│   │   ├── Dockerfile.claude-code
│   │   ├── Dockerfile.opencode
│   │   ├── Dockerfile.gemini-cli
│   │   └── entrypoint.sh
│   └── build-service/
│       └── Dockerfile
│
├── configs/
│   ├── manager.yaml
│   └── agents/
│       ├── claude-code.yaml
│       ├── opencode.yaml
│       └── gemini-cli.yaml
│
├── scripts/
│   ├── install-stack.sh
│   ├── install-stack.bat
│   └── setup-dev.sh
│
├── workspaces/              # Git-ignored, runtime
│
├── docker-compose.yml
├── docker-compose.dev.yml
├── Makefile
├── README.md
├── ROADMAP.md
└── LICENSE
```

## Supported AI CLIs

| CLI | Status | Notes |
|-----|--------|-------|
| Claude Code | Primary | Full support, recommended |
| OpenCode | Planned | Open-source alternative |
| Gemini CLI | Planned | Google's offering |
| Codex CLI | Planned | OpenAI's offering |
| Aider | Considered | Popular open-source option |

## Design Decisions

### Why containers per agent (not threads/processes)?

- **Isolation**: Agents can have conflicting dependencies
- **Resource limits**: Docker makes CPU/memory limits trivial
- **Failure domains**: One agent crashing doesn't affect others
- **Scaling**: Move containers across machines without code changes
- **Tooling**: Docker ecosystem for logging, monitoring, networking

### Why Go?

- Single static binary for both manager and sidecar
- Excellent Docker client library
- Built-in HTTP server suitable for production
- Good concurrency model for polling multiple agents
- LLMs write decent Go (important for AI-assisted development)

### Why markdown files for state (not a database)?

- Debuggable: `cat CURRENT.md` beats `SELECT * FROM agent_state`
- Git-native: Changes are commits, history is free
- AI-native: LLMs read/write markdown naturally
- No dependencies: No Postgres/Redis to configure
- Portable: Copy a workspace, get full agent state

### Why manager polls (not agents push)?

- Simpler agent implementation
- Manager controls its own schedule
- No callback configuration needed
- Graceful degradation if agent is stuck
- Easy to add more sophisticated scheduling later

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup and guidelines.

## License

MIT License. See [LICENSE](LICENSE).

## Acknowledgments

- [Beads](https://github.com/steveyegge/beads) — Inspiration for context protocols
- [Gastown](https://github.com/steveyegge/gastown) — Inspiration for agent orchestration patterns
- Altered Carbon — The naming convention that makes this project 10x cooler
