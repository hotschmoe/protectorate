# Protectorate - V1 Specification

## Overview

V1 is single-machine deployment with core sleeve management, message routing, and local git infrastructure.

**V1 Constraint**: Only Claude Code is supported as the AI CLI. Multi-CLI support deferred to V2.

## Sub-Specifications

- **[SPEC_V1_ENVOY.md](SPEC_V1_ENVOY.md)** - Detailed envoy manager specification
- **[SPEC_V1_SLEEVE.md](SPEC_V1_SLEEVE.md)** - Detailed sleeve container specification

## Key Concepts

### Memory vs Communication

```
CORTICAL STACK (.cstack/)     NEEDLECAST (.needlecast/)
----------------------------  ----------------------------
Memory - sleeve's own state   Communication - inter-sleeve

CURRENT.md  - active task     inbox.md   - messages TO sleeve
PLAN.md     - backlog         outbox.md  - messages FROM sleeve
MEMORY.md   - learnings       (V2: arena.md - global broadcast)

"What I know"                 "What I say/hear"
```

Both `.cstack/` and `.needlecast/` live in the project workspace (git tracked).

**Important**: These are separate concerns in separate repos:
- [cortical-stack](https://github.com/hotschmoe/cortical-stack) - Memory format
- needlecast (future repo) - Communication protocol

## Architecture

```
                    +------------------------+
                    |        ENVOY           |
                    |    (Manager Process)   |
                    |  - CLI interface       |
                    |  - HTTP API            |
                    |  - Spawns sleeves      |
                    |  - Routes messages     |
                    |  - Health monitoring   |
                    +----------+-------------+
                               |
         +---------------------+---------------------+
         |                     |                     |
         v                     v                     v
  +---------------+     +---------------+     +---------------+
  | Sleeve: alice |     | Sleeve: bob   |     | Sleeve: carol |
  |               |     |               |     |               |
  | +-----------+ |     | +-----------+ |     | +-----------+ |
  | | sidecar   | |     | | sidecar   | |     | | sidecar   | |
  | | :8080     | |     | | :8080     | |     | | :8080     | |
  | +-----------+ |     | +-----------+ |     | +-----------+ |
  | | tmux      | |     | | tmux      | |     | | tmux      | |
  | | + claude  | |     | | + claude  | |     | | + claude  | |
  | +-----------+ |     | +-----------+ |     | +-----------+ |
  | | ttyd      | |     | | ttyd      | |     | | ttyd      | |
  | | :7681     | |     | | :7681     | |     | | :7681     | |
  | +-----------+ |     | +-----------+ |     | +-----------+ |
  | .cstack/      |     | .cstack/      |     | .cstack/      |
  +---------------+     +---------------+     +---------------+
         |                     |                     |
         +---------------------+---------------------+
                               |
                        /needlecast/
                        (shared volume)
```

## Repository Structure

```
protectorate/
  cmd/
    envoy/           # Manager binary entry point
    sidecar/         # Sidecar binary entry point
  internal/
    envoy/           # Manager implementation
    sidecar/         # Sidecar implementation
    protocol/        # Shared types
    config/          # Configuration loading
  containers/
    envoy/           # Manager Dockerfile
    sleeve/          # Sleeve Dockerfile
  configs/
    envoy.yaml       # Default manager config
```

## Containers

### Envoy Manager Container

Single container with two modes:

```
+--------------------------------------------------+
|              ENVOY CONTAINER                      |
|                                                   |
|  +-------------------+   +-------------------+   |
|  | BOOTSTRAP MODE    |   | MANAGER MODE      |   |
|  | (First run)       |   | (Normal operation)|   |
|  |                   |   |                   |   |
|  | - Check env       |   | - Spawn sleeves   |   |
|  | - Setup wizard    |   | - Route messages  |   |
|  | - Create Gitea    |   | - Health checks   |   |
|  | - Init config     |   | - API server      |   |
|  | - Create networks |   | - Web UI          |   |
|  +-------------------+   +-------------------+   |
+--------------------------------------------------+
```

### Sleeve Container

V1 sleeve is Claude Code only (minimal image):

```dockerfile
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    curl git ca-certificates tmux \
    && rm -rf /var/lib/apt/lists/*

# ttyd for web terminal
RUN curl -fsSL https://github.com/tsl0922/ttyd/releases/download/1.7.4/ttyd.x86_64 \
    -o /usr/local/bin/ttyd && chmod +x /usr/local/bin/ttyd

# Node.js (for Claude Code)
RUN curl -fsSL https://deb.nodesource.com/setup_20.x | bash - && \
    apt-get install -y nodejs && rm -rf /var/lib/apt/lists/*

# Claude Code CLI
RUN npm install -g @anthropic-ai/claude-code

# Sidecar binary
COPY sidecar /usr/local/bin/sidecar

ENTRYPOINT ["/usr/local/bin/sidecar"]
```

Image size: ~400MB (V1 minimal)

## API Summary

### Envoy Manager API (port 7470)

```
GET  /health                  # Health check
GET  /sleeves                 # List all sleeves
POST /sleeves                 # Spawn new sleeve
GET  /sleeves/{name}          # Get sleeve info
DELETE /sleeves/{name}        # Kill sleeve
POST /sleeves/{name}/resleeve # Resleeve (soft or hard)
POST /sleeves/{name}/message  # Send message to sleeve
GET  /sleeves/{name}/terminal # WebSocket proxy to ttyd
GET  /arena                   # Read global arena
POST /arena                   # Post to global arena
POST /docker/spawn            # Proxy Docker spawn for sleeves
```

### Sidecar API (port 8080)

```
GET  /health    # Health check
GET  /status    # Sleeve status from .cstack/
GET  /outbox    # Read OUTBOX.md messages
POST /resleeve  # Soft resleeve (CLI swap)
```

## Message Routing (Needlecast)

V1 uses single-file format. V2 may use file-per-message for atomicity.

```
Sleeve A writes to /workspace/.needlecast/outbox.md
       |
       v
Envoy reads outbox.md from all workspaces on polling cycle
       |
       v
Envoy routes message to Sleeve B's .needlecast/inbox.md
       |
       v
Envoy clears processed messages from outbox.md
       |
       v
Sleeve B reads inbox.md on next cycle
```

**V2**: Global arena for broadcast messaging (shelved for V1).

## CLI Commands

```bash
# Sleeve management
envoy spawn --repo foo --goal "build feature"  # Spawn sleeve
envoy status                                    # List sleeves
envoy info alice                                # Sleeve details
envoy kill alice                                # Kill sleeve
envoy resleeve alice --soft                     # Restart CLI only
envoy resleeve alice --hard                     # Recreate container
envoy attach alice                              # Connect to terminal

# Messaging
envoy send alice "message"                      # Direct message
envoy inbox alice                               # Read inbox
envoy outbox alice                              # Read outbox
# V2: envoy broadcast "announcement"            # Global arena
# V2: envoy arena                               # Read global arena

# System
envoy init                                      # Bootstrap wizard
envoy config show                               # Show config
envoy gitea status                              # Gitea health
envoy mirror run                                # Trigger GitHub sync
```

## Configuration

### envoy.yaml

```yaml
poll_interval: 1h
idle_threshold: 0  # 0 = never timeout
max_sleeves: 10
port: 7470

docker:
  network: cortical-net
  workspace_root: /workspaces
  sleeve_image: ghcr.io/hotschmoe/protectorate-sleeve:latest

gitea:
  url: http://gitea:3000
  user: cortical
  # token from env: ${GITEA_TOKEN}

mirror:
  enabled: true
  frequency: daily
  github_org: hotschmoe
  # token from env: ${GITHUB_TOKEN}

needlecast:
  poll_interval: 30s  # How often to check outboxes
  # Note: .needlecast/ is in each workspace, not a separate volume
```

## V1 Scope

### Included

- [x] Envoy manager container (with bootstrap mode)
- [x] Gitea container (spawned by envoy)
- [x] Sleeve container (Claude Code only)
- [x] tmux session management in sleeves
- [x] ttyd web terminal per sleeve
- [x] Setup wizard (TUI in manager)
- [x] Needlecast messaging (inbox/outbox, single file format)
- [x] Sleeve spawning and lifecycle
- [x] Sidecar health/status API
- [x] Daily GitHub mirror (cron)
- [x] Minimal dashboard UI (terminal viewer, sleeve list)
- [x] Soft resleeve (CLI swap via tmux)
- [x] Hard resleeve (container respawn)
- [x] Docker proxy for sleeve testing (via manager)
- [x] CLI-first design (web UI uses CLI/API)

### Excluded (V2+)

- [ ] Global arena (broadcast messaging)
- [ ] File-per-message needlecast format
- [ ] Multi-CLI support (Gemini, OpenCode, etc.)
- [ ] Extended runtime image (Rust, Zig, Bun, etc.)
- [ ] Multi-machine (MASTER/SLAVE topology)
- [ ] Traefik reverse proxy
- [ ] Agent loops / autonomous operation
- [ ] Messaging integration (Telegram/Slack)
- [ ] Warm container pool
- [ ] Sleeve deployment to production

## Policies

### One Sleeve Per Stack

Strictly enforced - no concurrent access to same .cstack/ directory.

If parallelism needed:
1. Use subagents within Claude Code (native support)
2. Use separate repos with separate sleeves

### Host Auth Inheritance

Claude Code credentials inherited via volume mount:

```bash
docker run -it \
  -v ~/.claude/.credentials.json:/root/.claude/.credentials.json:ro \
  -v $(pwd):/workspace \
  protectorate-sleeve
```

User authenticates on host once, all containers inherit.

See [docs/claude-code-docker-inheritance.md](docs/claude-code-docker-inheritance.md) for details.

### CLI-First Design

Everything is automatable via CLI. Web UI is a frontend to the CLI/API.

```
USER (human or LLM)
        |
        v
   ENVOY CLI  <----->  ENVOY API
        |                  ^
        v                  |
   Web UI  ----------------+
   (optional)
```

## Sleeve Naming

Curated list of memorable names:

```
quell, virginia, rei, mickey, trepp, tanaka,
athena, apollo, hermes, iris, prometheus,
hal, samantha, jarvis, cortana, shodan
```

Manager assigns next available name on spawn.

## Implementation Phases

### Phase 1: Foundation
- Create repos and project structure
- Basic manager skeleton
- Sleeve and envoy Dockerfiles
- Push to ghcr.io

### Phase 2: Bootstrap and Gitea
- Setup wizard TUI
- First-run detection
- Gitea spawning and configuration
- Config repo initialization

### Phase 3: Sleeve Lifecycle
- Spawn API endpoint
- Sidecar implementation (with tmux/ttyd)
- Polling loop and state tracking
- Timeout handling

### Phase 4: Communication
- Needlecast implementation
- OUTBOX polling
- INBOX writing
- Global arena

### Phase 5: UI and Polish
- Dashboard UI (sleeve list, terminal viewer)
- ttyd proxy through envoy
- GitHub mirror cron
