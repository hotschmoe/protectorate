# Protectorate

Container-native AI agent orchestration system written in Go.

Named after the interstellar governing body in Altered Carbon: containers are "sleeves" (bodies), AI tools are the consciousness (DHF), and Protectorate manages them all.

## TLDR

Protectorate spawns a manager container called **Envoy** that orchestrates AI agent containers called **Sleeves**. Each sleeve runs an AI CLI (Claude Code, Gemini CLI, etc.) with a lightweight **Sidecar** that reports status back to Envoy.

Sleeves operate in workspaces where:

- **Cortical Stack** (`.cstack/`) - Long-term memory that persists across sessions
- **Needlecast** (`/needlecast/`) - Inter-agent communication system (planned)

## Philosophy

```
WE DO NOT: Modify AI CLI tools (Claude Code, Gemini CLI, etc.)
WE DO:     Orchestrate dozens of them with shared memory and coordination
```

## Architecture

```
                    +------------------------+
                    |        ENVOY           |
                    |    (Manager Process)   |
                    |  - Web UI + CLI        |
                    |  - Spawns sleeves      |
                    |  - Polls sidecars      |
                    +----------+-------------+
                               |
         +---------------------+---------------------+
         |                     |                     |
         v                     v                     v
  +---------------+     +---------------+     +---------------+
  | Sleeve Alice  |     | Sleeve Bob    |     | Sleeve Carol  |
  | Claude Code   |     | Gemini CLI    |     | OpenCode      |
  | Sidecar:8080  |     | Sidecar:8080  |     | Sidecar:8080  |
  | .cstack/      |     | .cstack/      |     | .cstack/      |
  +---------------+     +---------------+     +---------------+
```

## Components

| Component | Description |
|-----------|-------------|
| Envoy | Manager container with Docker socket access, web UI, CLI, spawns/kills sleeves |
| Sleeve | Agent container with AI CLI, sidecar, and mounted workspace |
| Sidecar | Lightweight HTTP server inside each sleeve exposing /health and /status |

## Installation

One-line install for Linux:

```bash
curl -fsSL https://raw.githubusercontent.com/hotschmoe/protectorate/master/install.sh | bash
```

This will:
- Install Docker (if needed)
- Install Claude CLI (if needed)
- Authenticate with Claude
- Generate long-lived OAuth token
- Clone repo to `~/protectorate`
- Pull pre-built container images
- Start Envoy

**Web UI:** http://localhost:7470

### Uninstall

```bash
curl -fsSL https://raw.githubusercontent.com/hotschmoe/protectorate/master/uninstall.sh | bash
```

Removes containers, network, and optionally images/directory. Does not remove Docker or Claude CLI.

---

## Development

Prerequisites for building from source:

- Docker with BuildKit
- Go 1.24+
- `inotify-tools` (optional, for file watcher)

```bash
# Ubuntu/Debian - install file watcher support
sudo apt-get install inotify-tools
```

### Quick Start (Dev)

```bash
# 1. Build base image (slow, only needed once)
make build-base

# 2. Start dev environment (fast iteration)
make dev
```

**Web UI:** http://localhost:7470

### Fast Iteration

The dev environment uses volume-mounted binaries and hot-reload for the webui:

```bash
make dev           # Start dev environment
make dev-restart   # Rebuild Go + recreate container (picks up all changes)
make dev-logs      # View container logs
make dev-down      # Stop dev environment
make watch         # Auto-rebuild on file changes (requires inotify-tools)
```

| Change Type | Action |
|-------------|--------|
| HTML/CSS/JS | Just refresh browser |
| Go code | `make dev-restart` |
| Compose/entrypoint | `make dev-restart` |

See [docs/build_optimizations.md](docs/build_optimizations.md) for details.

### Production Build

```bash
make build-base    # Build base image (~2 min, run once)
make build-sleeve  # Build sleeve image
make build-envoy   # Build envoy image (dev: local Go)
make build         # Build envoy + sleeve
make release       # Full production build (multi-stage)

make up            # Start production services
make down          # Stop services
```

## API

**Envoy (port 7470)**
```
GET    /api/sleeves              List all sleeves (with sidecar status)
POST   /api/sleeves              Spawn new sleeve
GET    /api/sleeves/{name}       Get sleeve details
DELETE /api/sleeves/{name}       Kill sleeve
GET    /api/workspaces           List workspaces
POST   /api/workspaces/clone     Clone git repository
GET    /api/doctor               System health checks
GET    /api/host/stats           CPU, memory, disk stats
GET    /sleeves/{name}/terminal  WebSocket terminal access
```

**Sidecar (port 8080, internal to raven network)**
```
GET  /health    Returns {"status": "ok"}
GET  /status    DHF info, cstack stats, process info, auth status
```

Envoy polls sidecars internally - the web UI only hits Envoy endpoints.

## CLI

```bash
envoy serve                          # Start the daemon (default)
envoy status [--json]                # List sleeves
envoy spawn /workspaces/myproject    # Spawn sleeve on workspace
envoy spawn /workspaces/foo --name alice --memory 4096
envoy kill alice                     # Kill sleeve
envoy info alice                     # Detailed sleeve info
envoy doctor [--json]                # System diagnostics
envoy workspaces [--json]            # List workspaces
envoy clone https://github.com/user/repo
envoy stats [--json]                 # Host resource stats
```

## Related

- [cortical-stack](../repo_cortical-stack) - Memory format used by sleeves
