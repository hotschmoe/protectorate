# Protectorate V1 - Sleeve Specification

## Overview

A sleeve is a Docker container that hosts an AI CLI tool (the "consciousness" or DHF). Each sleeve runs a sidecar process for health reporting and a tmux session for process management, with ttyd providing web terminal access.

## Architecture

```
+------------------------------------------------------------------+
|                       SLEEVE CONTAINER                            |
|                                                                   |
|  +-----------------------+                                        |
|  |      SIDECAR          |  HTTP API for envoy communication      |
|  |      :8080            |  /health, /status, /outbox, /resleeve  |
|  +-----------------------+                                        |
|            |                                                      |
|            | monitors                                             |
|            v                                                      |
|  +-----------------------+                                        |
|  |       tmux            |  Session manager                       |
|  |   session: main       |  - Detached session for AI CLI         |
|  |                       |  - Survives disconnects                |
|  |   +---------------+   |  - Soft resleeve = kill/start in tmux  |
|  |   | claude-code   |   |                                        |
|  |   | (AI CLI)      |   |                                        |
|  |   +---------------+   |                                        |
|  +-----------------------+                                        |
|            |                                                      |
|            | exposes                                              |
|            v                                                      |
|  +-----------------------+                                        |
|  |       ttyd            |  Web terminal server                   |
|  |       :7681           |  - Exposes tmux session via HTTP       |
|  |                       |  - WebSocket for real-time terminal    |
|  +-----------------------+                                        |
|                                                                   |
|  MOUNTED VOLUMES:                                                 |
|  /workspace              <- Project code (from host/gitea)        |
|  /workspace/.cstack/     <- Cortical stack (memory)               |
|  /needlecast/{name}/     <- INBOX.md, OUTBOX.md                   |
|  /needlecast/arena/      <- Global arena (shared)                 |
|  /root/.claude/          <- Claude credentials (RO from host)     |
+------------------------------------------------------------------+
```

## Process Hierarchy

```
PID 1: sidecar
  |
  +-- tmux server (session: main)
  |     |
  |     +-- claude-code (or other AI CLI)
  |
  +-- ttyd (attached to tmux session)
```

The sidecar is PID 1 and supervises all other processes. This allows:
- Clean shutdown handling
- Health monitoring from within the container
- Soft resleeve by signaling tmux

## Sidecar Process

### Responsibilities

1. **Health Reporting** - Respond to envoy health checks
2. **Status Parsing** - Read `.cstack/CURRENT.md` to report sleeve state
3. **Outbox Serving** - Expose outbox contents via API
4. **Resleeve Handling** - Execute soft resleeve (kill CLI, start new one)
5. **Process Supervision** - Start tmux, ttyd, and AI CLI on boot

### Sidecar API

Port: `8080` (internal to Docker network)

```
GET /health
  Response: {"status": "healthy", "uptime": 3600}

  Health check. Returns 200 if sidecar is running.
  Does NOT check if AI CLI is responsive (that's /status).

GET /status
  Response: {
    "sleeve_id": "alice",
    "status": "working",
    "current_task": "Implementing auth module",
    "progress": {"total": 5, "completed": 2},
    "blockers": [],
    "cli": "claude-code",
    "cli_pid": 1234,
    "cli_running": true,
    "last_activity": "2026-01-21T10:00:00Z"
  }

  Parses .cstack/CURRENT.md for status info.
  Checks if CLI process is running.

GET /outbox
  Response: {
    "messages": [
      {
        "to": "bob",
        "content": "Need help with database schema",
        "timestamp": "2026-01-21T10:00:00Z"
      }
    ]
  }

  Returns parsed OUTBOX.md contents.
  Envoy reads this and routes messages.

POST /resleeve
  Body: {"cli": "gemini-cli"}  // optional, defaults to current
  Response: {"status": "resleeving"}

  Soft resleeve:
  1. Kill current CLI process in tmux
  2. Start new CLI process
  3. Return success

  The workspace and .cstack/ remain intact.

GET /terminal
  WebSocket upgrade for terminal access.
  Proxies to ttyd or returns ttyd URL.
```

### Sidecar Implementation Notes

```go
// Sidecar startup sequence
func main() {
    // 1. Read config from env/files
    config := loadConfig()

    // 2. Start tmux session
    startTmux()

    // 3. Start ttyd attached to tmux
    startTtyd()

    // 4. Start AI CLI in tmux
    startCLI(config.CLI)

    // 5. Start HTTP server
    startAPIServer()

    // 6. Wait for signals
    waitForShutdown()
}
```

## tmux Configuration

### Session Setup

```bash
# Create detached session named "main"
tmux new-session -d -s main -x 200 -y 50

# Set options for better CLI compatibility
tmux set-option -t main mouse off
tmux set-option -t main history-limit 50000
tmux set-option -t main remain-on-exit on  # Keep pane if CLI exits
```

### Soft Resleeve via tmux

```bash
# Kill current CLI
tmux send-keys -t main C-c
sleep 1
tmux send-keys -t main "exit" Enter

# Start new CLI
tmux send-keys -t main "claude --resume" Enter
```

### Why tmux?

1. **Detached execution** - AI CLI runs without attached terminal
2. **Survive disconnects** - Session persists if network drops
3. **Easy attach** - Debugging by attaching to running session
4. **Soft resleeve** - Kill/start processes without container restart
5. **Web integration** - ttyd can connect to tmux session

## ttyd Configuration

### Purpose

ttyd exposes the tmux session as a web-accessible terminal. This allows:
- Web UI to show live terminal view
- Users to watch AI work in real-time
- Emergency intervention without docker exec

### Startup

```bash
ttyd --port 7681 --writable tmux attach-session -t main
```

Options:
- `--writable` - Allow input (can be disabled for view-only)
- `--once` - Exit after one client disconnects (disabled, we want persistent)
- `--reconnect` - Auto-reconnect on disconnect

### Security Considerations

- ttyd is only exposed on Docker network (not host)
- Envoy proxies ttyd connections with auth
- Write access can be disabled for view-only mode

## Container Image

### Dockerfile

```dockerfile
FROM debian:bookworm-slim

ENV DEBIAN_FRONTEND=noninteractive

# Base system dependencies
RUN apt-get update && apt-get install -y \
    curl \
    ca-certificates \
    git \
    tmux \
    && rm -rf /var/lib/apt/lists/*

# Install ttyd
RUN curl -fsSL https://github.com/tsl0922/ttyd/releases/download/1.7.4/ttyd.x86_64 \
    -o /usr/local/bin/ttyd && chmod +x /usr/local/bin/ttyd

# Install Node.js 20 LTS (required for Claude Code)
RUN curl -fsSL https://deb.nodesource.com/setup_20.x | bash - \
    && apt-get install -y nodejs \
    && rm -rf /var/lib/apt/lists/*

# Install Claude Code CLI
RUN npm install -g @anthropic-ai/claude-code

# Copy sidecar binary (built separately)
COPY sidecar /usr/local/bin/sidecar

# Create necessary directories
RUN mkdir -p /root/.claude /workspace

# Working directory
WORKDIR /workspace

# Expose ports
EXPOSE 8080 7681

# Sidecar is PID 1
ENTRYPOINT ["/usr/local/bin/sidecar"]
```

### Image Size

Target: ~500MB
- Base Debian: ~80MB
- Node.js: ~100MB
- Claude Code: ~200MB
- tmux + ttyd: ~20MB
- Sidecar: ~10MB

### V2 Extended Image (future)

V2 will include additional runtimes. For V1, we only support Claude Code:

```dockerfile
# V2 additions (not for V1):
# - Python 3 + pip
# - Bun
# - Rust
# - Zig
# - Gemini CLI, OpenCode, etc.
```

## Volume Mounts

### Required Mounts

| Mount Point | Source | Mode | Purpose |
|-------------|--------|------|---------|
| `/workspace` | Host/Gitea repo | rw | Project code (includes .cstack/, .needlecast/) |
| `/root/.claude/.credentials.json` | Host | ro | Claude auth |

### Optional Mounts

| Mount Point | Source | Mode | Purpose |
|-------------|--------|------|---------|
| `/root/.claude/plugins` | Host | ro | Host plugins |

**Note**: `.cstack/` and `.needlecast/` are part of the project repo (git tracked), not separate mounts. They persist through hard resleeve because the workspace volume persists.

### Docker Run Example

```bash
docker run -d \
  --name sleeve-alice \
  --network cortical-net \
  -v /workspaces/myproject:/workspace \
  -v ~/.claude/.credentials.json:/root/.claude/.credentials.json:ro \
  -e SLEEVE_NAME=alice \
  -e SLEEVE_CLI=claude-code \
  -e SLEEVE_GOAL="Implement authentication module" \
  ghcr.io/hotschmoe/protectorate-sleeve:latest
```

### What's in the Workspace

```
/workspace/                      (mounted from host)
  .cstack/                       # Memory - git tracked
    CURRENT.md
    PLAN.md
    MEMORY.md
  .needlecast/                   # Communication - git tracked
    inbox.md
    outbox.md
  src/                           # Project code
  CLAUDE.md                      # Project instructions
  ...
```

## Cortical Stack (.cstack/)

Memory format for the sleeve. Located at `/workspace/.cstack/`.

**Important**: Cortical stack is ONLY for memory. Communication uses needlecast.

### Files

```
.cstack/
  CURRENT.md    # Active task, current focus, blockers
  PLAN.md       # Task list, backlog, priorities
  MEMORY.md     # Long-term context, decisions, learnings
```

### CURRENT.md Format

```markdown
# Current State

## Status
working

## Task
Implementing user authentication module

## Progress
- [x] Design auth flow
- [x] Create user model
- [ ] Implement JWT tokens
- [ ] Add password hashing
- [ ] Write tests

## Blockers
(none)

## Next Steps
1. Implement JWT token generation
2. Add middleware for auth checks
```

The sidecar parses this file to report status to envoy.

## Needlecast (Communication)

Needlecast is the inter-sleeve communication layer. Separate from cortical stack.

**Location**: `/workspace/.needlecast/` (in project repo, git tracked)

**V1 Format**: Single file with appended messages (V2 may use file-per-message)

### Directory Structure

```
/workspace/
  .cstack/           # Memory (sleeve's own state)
  .needlecast/       # Communication (inter-sleeve)
    inbox.md         # Messages TO this sleeve
    outbox.md        # Messages FROM this sleeve
  src/               # Project code
```

**Note**: Arena is shelved for V1. Will be added in V2 as `.needlecast/arena.md` or shared mount.

### Access Patterns

```
ENVOY can see:
  - ALL sleeves' .cstack/
  - ALL sleeves' .needlecast/

SLEEVE can see:
  - Own .cstack/
  - Own .needlecast/
  - (V2) Shared arena

SLEEVE CANNOT see:
  - Other sleeves' .cstack/
  - Other sleeves' .needlecast/
```

### Message Format (YAML Frontmatter)

Messages use YAML frontmatter for metadata, markdown for content:

```markdown
---
id: msg-abc123
from: bob
to: alice
thread: auth-help
type: question
time: 2026-01-21T10:00:00Z
---
Need help understanding the JWT implementation.
Can you review my approach?

---
id: msg-abc124
from: envoy
to: alice
type: directive
time: 2026-01-21T09:30:00Z
---
Please prioritize the authentication module.

```

### inbox.md

Messages TO this sleeve. Written by envoy, read by sleeve.

### outbox.md

Messages FROM this sleeve. Written by sleeve (or CLI), read and cleared by envoy.

```markdown
---
id: msg-def456
from: alice
to: bob
thread: auth-help
type: response
time: 2026-01-21T10:15:00Z
---
Here's my review of your JWT approach:
1. Token expiry looks good
2. Consider adding refresh tokens
3. The secret should come from env vars

---
id: msg-def457
from: alice
to: envoy
type: milestone
time: 2026-01-21T10:10:00Z
---
Completed user model implementation.
Moving on to JWT tokens.

```

### Message Types

| Type | Purpose |
|------|---------|
| `task` | Assign work to a sleeve |
| `question` | Ask another sleeve for help |
| `response` | Reply to a question |
| `milestone` | Report progress to envoy |
| `directive` | Envoy command to sleeve |
| `blocked` | Report blocker |

### V2 Consideration: File-per-message

V2 may switch to file-per-message for better atomicity:

```
.needlecast/
  inbox/
    2026-01-21T10-00-00_msg-abc123.md
    2026-01-21T10-05-00_msg-abc124.md
  outbox/
    2026-01-21T10-02-00_msg-def456.md
  arena/
    2026-01-21T09-00-00_msg-ghi789.md
```

Benefits: atomic writes, easier cleanup, cleaner git history.
Deferred to V2 to reduce initial complexity.

## Environment Variables

### Required

```bash
SLEEVE_NAME=alice           # Sleeve identifier
SLEEVE_CLI=claude-code      # Which AI CLI to run
```

### Optional

```bash
SLEEVE_GOAL="description"   # Initial goal (written to CURRENT.md)
SLEEVE_REPO=myproject       # Repo name (for reference)
SIDECAR_PORT=8080          # Sidecar API port
TTYD_PORT=7681             # ttyd web terminal port
```

## Lifecycle

### Spawn Sequence

```
1. Envoy creates container with mounts
2. Sidecar starts (PID 1)
3. Sidecar creates tmux session
4. Sidecar starts ttyd
5. Sidecar writes initial CURRENT.md if SLEEVE_GOAL set
6. Sidecar starts AI CLI in tmux
7. Sidecar starts HTTP server
8. Sidecar reports healthy to envoy
```

### Soft Resleeve Sequence

```
1. Envoy sends POST /resleeve to sidecar
2. Sidecar kills CLI process in tmux
3. Sidecar clears CURRENT.md status (optional)
4. Sidecar starts new CLI in tmux
5. Sidecar reports healthy
```

### Hard Resleeve Sequence

```
1. Envoy kills container (docker kill)
2. Envoy creates new container with same mounts
3. New sidecar starts
4. Workspace and .cstack/ preserved via volumes
5. Fresh CLI starts with existing memory
```

### Kill Sequence

```
1. Envoy sends SIGTERM to container
2. Sidecar receives signal
3. Sidecar sends SIGTERM to tmux/ttyd
4. Sidecar writes "killed" to CURRENT.md (optional)
5. Container exits
```

## AI CLI Integration

### Claude Code Specifics

```bash
# Start command
claude --resume

# The --resume flag tells Claude Code to:
# - Read CLAUDE.md for project context
# - Check for existing conversation history
# - Continue previous work if available
```

### Credentials

Claude Code reads credentials from `~/.claude/.credentials.json`.

Mounted read-only from host:
```bash
-v ~/.claude/.credentials.json:/root/.claude/.credentials.json:ro
```

See `docs/claude-code-docker-inheritance.md` for details on what can be inherited.

### Required Write Directories

Claude Code needs write access to (inside container, not mounted):
- `/root/.claude/projects/` - Session history
- `/root/.claude/todos/` - Todo tracking
- `/root/.claude/statsig/` - Feature flags
- `/root/.claude/debug/` - Debug logs

These are NOT mounted from host - each sleeve has its own.

## Health Checks

### Container Health Check

```dockerfile
HEALTHCHECK --interval=30s --timeout=10s --start-period=60s --retries=3 \
  CMD curl -f http://localhost:8080/health || exit 1
```

### Sidecar Health Levels

| Level | Meaning | Action |
|-------|---------|--------|
| healthy | Sidecar running, CLI running | Normal operation |
| degraded | Sidecar running, CLI crashed | Envoy may soft resleeve |
| unhealthy | Sidecar not responding | Envoy may hard resleeve |

## Resource Limits

Default limits (configurable per-sleeve):

```yaml
resources:
  memory: 4g
  cpus: 2
```

These are set by envoy when spawning via Docker API.

## Go Package Structure

The sidecar uses internal packages for parsing cstack and needlecast files:

```
protectorate/
  internal/
    cstack/
      types.go      # CurrentState, PlanState, Task, SleeveStatus
      parser.go     # ParseCurrent, ParsePlan, ExtractStatus
    needlecast/
      types.go      # Message
      messages.go   # ReadInbox, ReadOutbox, WriteInbox, ClearOutbox
```

### internal/cstack

```go
package cstack

const StackDir = ".cstack"

type SleeveStatus string

const (
    StatusIdle    SleeveStatus = "idle"
    StatusWorking SleeveStatus = "working"
    StatusBlocked SleeveStatus = "blocked"
    StatusDone    SleeveStatus = "done"
)

type CurrentState struct {
    Status       SleeveStatus
    Task         string
    Progress     Progress
    Blockers     []string
    LastModified time.Time
}

type Progress struct {
    Total     int
    Completed int
}

func ParseCurrent(workspacePath string) (*CurrentState, error)
func ExtractStatus(workspacePath string) (SleeveStatus, error)
```

### internal/needlecast

```go
package needlecast

const NeedlecastDir = ".needlecast"

type Message struct {
    ID        string
    From      string
    To        string
    Thread    string
    Type      string
    Content   string
    Timestamp time.Time
}

func ReadInbox(workspacePath string) ([]Message, error)
func ReadOutbox(workspacePath string) ([]Message, error)
func WriteInbox(workspacePath string, msg Message) error
func ClearOutbox(workspacePath string) error
```

## V1 Scope

### Included
- [x] Sidecar with health/status/outbox/resleeve API
- [x] tmux session management
- [x] ttyd web terminal
- [x] Claude Code integration
- [x] Credentials inheritance
- [x] Needlecast inbox/outbox (single file format)
- [x] Cortical stack for memory

### Excluded (V2+)
- [ ] Multiple AI CLI support (Gemini, OpenCode)
- [ ] Extended runtime image (Rust, Zig, etc.)
- [ ] Warm container pool
- [ ] GPU access
- [ ] Global arena (broadcast messaging)
- [ ] File-per-message needlecast format
- [ ] Sleeve-to-sleeve direct communication (bypassing needlecast)
