# Protectorate V1 - Envoy Specification

## Overview

Envoy is the central manager process for Protectorate. It runs as a Docker container with Docker socket access and is responsible for spawning sleeves, routing messages, health monitoring, and providing the CLI/API interface for users.

## Architecture

```
+------------------------------------------------------------------+
|                      ENVOY CONTAINER                              |
|                                                                   |
|  +------------------+    +------------------+    +--------------+ |
|  |   ENVOY CLI      |    |   MANAGER CORE   |    |  WEB UI      | |
|  |                  |--->|                  |<---|  (optional)  | |
|  | envoy spawn ...  |    | - Sleeve mgmt    |    |  :7470/ui    | |
|  | envoy status     |    | - Message router |    |              | |
|  | envoy kill ...   |    | - Health checker |    +--------------+ |
|  +------------------+    | - Bootstrap mode |                     |
|                          +--------+---------+                     |
|                                   |                               |
|                          +--------v---------+                     |
|                          |   HTTP API       |                     |
|                          |   :7470          |                     |
|                          +------------------+                     |
+------------------------------------------------------------------+
                                    |
                           Docker Socket Access
                                    |
         +---------------------+----+----+---------------------+
         |                     |         |                     |
         v                     v         v                     v
    [sleeve-alice]       [sleeve-bob]  [gitea]           [sleeve-carol]
```

## Modes of Operation

### Bootstrap Mode (First Run)

Detected when: No `~/.envoy/config.yaml` exists or `--bootstrap` flag is passed.

```
BOOTSTRAP SEQUENCE:
1. Detect first run (no config file)
2. Launch TUI setup wizard
3. Configure:
   - Docker socket path verification
   - Network creation (cortical-net)
   - Workspace root directory
   - GitHub mirror settings (optional)
   - Gitea admin credentials
4. Spawn Gitea container
5. Wait for Gitea healthy
6. Create cortical user in Gitea
7. Write ~/.envoy/config.yaml
8. Transition to Manager Mode
```

### Manager Mode (Normal Operation)

The steady-state operational mode. Runs continuously.

```
MANAGER LOOP (every poll_interval):
1. Health check all sleeves via sidecar /health
2. Read all sleeve .needlecast/outbox.md files
3. Route messages to target .needlecast/inbox.md files
4. Clear processed outbox messages
5. Update internal state
6. Handle any pending API requests
```

## CLI Commands

All envoy functionality is exposed via CLI. Web UI and automation tools use the same underlying API.

### Sleeve Management

```bash
# Spawn a new sleeve
envoy spawn --repo <repo-name> --goal "description of task"
envoy spawn --repo myproject --goal "implement feature X" --name alice

# List all sleeves
envoy status
envoy status --json

# Get detailed sleeve info
envoy info <sleeve-name>
envoy info alice

# Kill a sleeve
envoy kill <sleeve-name>
envoy kill alice

# Resleeve operations
envoy resleeve <sleeve-name> --soft          # Restart CLI process only
envoy resleeve <sleeve-name> --hard          # Destroy and recreate container

# Attach to sleeve terminal (via ttyd proxy or docker exec)
envoy attach <sleeve-name>
```

### Message Operations

```bash
# Send message to a sleeve
envoy send <sleeve-name> "your message here"
envoy send alice "Please review the PR when done"

# Read sleeve's outbox (for debugging)
envoy outbox <sleeve-name>

# Read sleeve's inbox
envoy inbox <sleeve-name>

# V2: Arena commands (shelved)
# envoy broadcast "message for all sleeves"
# envoy arena
```

### System Operations

```bash
# Bootstrap/setup
envoy init                    # Run bootstrap wizard
envoy init --non-interactive  # Use defaults/env vars

# Configuration
envoy config show
envoy config set poll_interval 30m

# Gitea operations
envoy gitea status
envoy gitea repos             # List repos in Gitea

# GitHub mirror
envoy mirror status
envoy mirror run              # Trigger immediate mirror sync
```

## HTTP API

Base URL: `http://envoy:7470` (internal) or exposed port on host.

### Health & Status

```
GET /health
  Response: {"status": "healthy", "uptime": 3600, "version": "1.0.0"}

GET /status
  Response: {
    "mode": "manager",
    "sleeves_active": 3,
    "sleeves_max": 10,
    "gitea_healthy": true,
    "last_poll": "2026-01-21T10:00:00Z"
  }
```

### Sleeve Management

```
GET /sleeves
  Response: [
    {
      "id": "abc123",
      "name": "alice",
      "status": "working",
      "cli": "claude-code",
      "repo": "myproject",
      "spawn_time": "2026-01-21T09:00:00Z",
      "last_checkin": "2026-01-21T10:00:00Z",
      "ttyd_port": 7681
    },
    ...
  ]

POST /sleeves
  Body: {
    "repo": "myproject",
    "goal": "implement feature X",
    "name": "alice"  // optional, auto-assigned if omitted
  }
  Response: {"id": "abc123", "name": "alice", "status": "spawning"}

GET /sleeves/{name}
  Response: {
    "id": "abc123",
    "name": "alice",
    "container_id": "sha256:...",
    "status": "working",
    "current_task": "Implementing authentication module",
    "progress": {"total": 5, "completed": 2},
    "cli": "claude-code",
    "workspace": "/workspaces/myproject",
    "ttyd_url": "http://envoy:7470/sleeves/alice/terminal"
  }

DELETE /sleeves/{name}
  Response: {"status": "killed"}

POST /sleeves/{name}/resleeve
  Body: {"type": "soft"} or {"type": "hard"}
  Response: {"status": "resleeving"}
```

### Messaging

```
POST /sleeves/{name}/message
  Body: {"content": "your message here"}
  Response: {"status": "delivered"}

GET /sleeves/{name}/outbox
  Response: {"messages": [...]}

# V2: Arena endpoints (shelved)
# POST /arena
# GET /arena
```

### Terminal Proxy (ttyd)

```
GET /sleeves/{name}/terminal
  WebSocket proxy to sleeve's ttyd instance
  Allows web UI to embed live terminal view
```

### Docker Proxy (for sleeves)

Sleeves cannot access Docker socket directly. They request container operations via envoy.

```
POST /docker/spawn
  Body: {
    "image": "postgres:15",
    "network": "cortical-net",
    "env": {"POSTGRES_PASSWORD": "xxx"},
    "ports": ["5432:5432"]
  }
  Response: {"container_id": "sha256:...", "address": "postgres:5432"}
```

## Configuration

### File Location

`~/.envoy/config.yaml` (inside envoy container, mounted from host)

### Default Configuration

```yaml
# Envoy Manager Configuration
version: 1

# Polling and timeouts
poll_interval: 1h        # How often to check sleeves
idle_threshold: 0        # 0 = never timeout idle sleeves

# Capacity
max_sleeves: 10

# API server
api:
  port: 7470
  host: 0.0.0.0

# Docker settings
docker:
  socket: /var/run/docker.sock
  network: cortical-net
  workspace_root: /workspaces
  sleeve_image: ghcr.io/hotschmoe/protectorate-sleeve:latest

# Gitea settings
gitea:
  enabled: true
  container_name: gitea
  url: http://gitea:3000
  user: cortical
  # password/token from env: GITEA_PASSWORD, GITEA_TOKEN

# GitHub mirror (optional)
mirror:
  enabled: false
  frequency: daily        # daily, hourly, manual
  github_org: ${GITHUB_ORG}
  # token from env: GITHUB_TOKEN

# Needlecast (messaging)
# Note: .needlecast/ is inside each workspace, not a separate volume
needlecast:
  poll_interval: 30s  # How often to check outboxes
  # V2: arena_enabled: true
```

### Environment Variables

```bash
# Required
DOCKER_HOST=unix:///var/run/docker.sock

# Gitea
GITEA_PASSWORD=xxx
GITEA_TOKEN=xxx

# GitHub mirror (optional)
GITHUB_TOKEN=xxx
GITHUB_ORG=hotschmoe

# Claude Code auth (passed to sleeves)
# Mounted from host: ~/.claude/.credentials.json
```

## Internal State

Envoy maintains state in memory and persists to `~/.envoy/state.json`:

```json
{
  "sleeves": {
    "alice": {
      "id": "abc123",
      "container_id": "sha256:...",
      "name": "alice",
      "repo": "myproject",
      "cli": "claude-code",
      "status": "working",
      "spawn_time": "2026-01-21T09:00:00Z",
      "last_checkin": "2026-01-21T10:00:00Z",
      "workspace": "/workspaces/myproject",
      "ports": {
        "sidecar": 8080,
        "ttyd": 7681
      }
    }
  },
  "gitea": {
    "container_id": "sha256:...",
    "healthy": true
  },
  "last_mirror": "2026-01-21T00:00:00Z"
}
```

## Sleeve Naming

Names are assigned from a curated pool for memorability:

```
POOL: [
  "quell", "virginia", "rei", "mickey", "trepp", "tanaka",
  "athena", "apollo", "hermes", "iris", "prometheus",
  "hal", "samantha", "jarvis", "cortana", "shodan"
]
```

Assignment: Next available name from pool. If all used, generate `sleeve-{random-suffix}`.

## Envoy Container Dockerfile

```dockerfile
FROM golang:1.22-bookworm AS builder

WORKDIR /build
COPY go.mod go.sum ./
RUN go mod download
COPY . .
RUN CGO_ENABLED=0 go build -o envoy ./cmd/envoy

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    docker.io \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/envoy /usr/local/bin/envoy

# Create directories
RUN mkdir -p /root/.envoy /workspaces

EXPOSE 7470

ENTRYPOINT ["/usr/local/bin/envoy"]
CMD ["serve"]
```

## Docker Compose Example

```yaml
version: "3.8"

services:
  envoy:
    image: ghcr.io/hotschmoe/protectorate-envoy:latest
    container_name: envoy
    restart: unless-stopped
    ports:
      - "7470:7470"
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock
      - ~/.envoy:/root/.envoy
      - ~/.claude/.credentials.json:/host-claude-creds/.credentials.json:ro
      - ./workspaces:/workspaces  # Contains .cstack/ and .needlecast/ per project
    environment:
      - GITEA_PASSWORD=${GITEA_PASSWORD}
      - GITHUB_TOKEN=${GITHUB_TOKEN}
    networks:
      - cortical-net

  gitea:
    image: gitea/gitea:1.21
    container_name: gitea
    restart: unless-stopped
    volumes:
      - gitea-data:/data
    environment:
      - USER_UID=1000
      - USER_GID=1000
    networks:
      - cortical-net

volumes:
  gitea-data:

networks:
  cortical-net:
    name: cortical-net
```

## Interaction Patterns

### User -> Envoy -> Sleeve

```
User runs: envoy spawn --repo foo --goal "build feature"
    |
    v
Envoy CLI -> POST /sleeves {...}
    |
    v
Manager: docker run ... protectorate-sleeve
    |
    v
Sleeve container starts:
  - Sidecar starts on :8080
  - tmux session created
  - ttyd starts on :7681
  - Claude Code launched in tmux
    |
    v
Manager: registers sleeve, returns success
    |
    v
Envoy CLI: "Spawned sleeve 'alice' - working on foo"
```

### Sleeve -> Envoy (via Needlecast)

```
Alice's sleeve writes to /workspace/.needlecast/outbox.md:
  "---\nto: bob\n...\n---\nNeed help with database schema"
    |
    v
Envoy poll loop reads outbox from /workspaces/alice-project/.needlecast/outbox.md
    |
    v
Envoy writes to /workspaces/bob-project/.needlecast/inbox.md:
  "---\nfrom: alice\n...\n---\nNeed help with database schema"
    |
    v
Envoy clears alice's outbox.md
    |
    v
Bob's CLI reads inbox.md on next cycle
```

**Note**: Each sleeve's `.needlecast/` is inside its workspace. Envoy has access to all workspaces. Sleeves only see their own workspace.

## Error Handling

### Sleeve Health Failures

```
if sleeve /health returns error 3 times:
  mark sleeve as "unhealthy"
  if --auto-resleeve enabled:
    trigger hard resleeve
  else:
    alert user via CLI/API
```

### Gitea Unavailable

```
if gitea /health fails:
  log warning
  continue operating (sleeves still work)
  retry gitea connection on next poll
  block new sleeve spawns that require git clone
```

## V1 Scope

### Included
- [x] Bootstrap mode with TUI wizard
- [x] Manager mode with polling loop
- [x] Full CLI interface
- [x] REST API
- [x] Sleeve spawning/killing
- [x] Soft and hard resleeve
- [x] Needlecast message routing (inbox/outbox, single file format)
- [x] ttyd terminal proxy
- [x] Docker proxy for sleeve container spawning
- [x] Gitea spawning and management
- [x] Basic web UI (terminal viewer, sleeve list)

### Excluded (V2+)
- [ ] Global arena (broadcast messaging)
- [ ] File-per-message needlecast format
- [ ] Multi-node deployment
- [ ] Traefik reverse proxy
- [ ] Slack/Telegram notifications
- [ ] Advanced scheduling (priority queues)
- [ ] Sleeve migration between hosts
