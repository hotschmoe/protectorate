# Development vs Production Environments

This document details the differences between dev and prod configurations.

## Quick Reference

| Aspect | Development | Production |
|--------|-------------|------------|
| Compose file | `docker-compose.dev.yaml` | `docker-compose.yaml` |
| Container name | `envoy-dev` | `envoy-poe` |
| Base image | `protectorate/base:latest` (local) | `ghcr.io/hotschmoe/protectorate-envoy:latest` |
| Go binary | Volume-mounted from `./bin/envoy` | Baked into image |
| Webui | Volume-mounted (hot-reload) | Baked into image |
| Entrypoint | Volume-mounted | Baked into image |
| ttyd port | Exposed (7681) | Not exposed |
| User | root (explicit) | root (inherited) |
| DEV_MODE env | true | not set |

## Image Architecture

```
Development:
+---------------------------+
| protectorate/base:latest  |  <-- debian + tmux + ttyd + claude CLI
+---------------------------+
         |
         | docker-compose.dev.yaml mounts:
         |   - ./bin/envoy:/usr/local/bin/envoy
         |   - ./containers/envoy/entrypoint.sh:/entrypoint.sh
         |   - ./internal/envoy/web:/app/web
         v
+---------------------------+
| envoy-dev container       |  <-- runs base image with mounts
+---------------------------+

Production:
+---------------------------+
| protectorate/base:latest  |
+---------------------------+
         |
         | Dockerfile (multi-stage)
         |   - Builds Go binary in golang:1.24-alpine
         |   - Copies binary + entrypoint into image
         v
+---------------------------+
| ghcr.io/.../envoy:latest  |  <-- self-contained image
+---------------------------+
```

## Volume Mounts

### Development (docker-compose.dev.yaml)

```yaml
volumes:
  # Binary (rebuilt with make bin/envoy)
  - ./bin/envoy:/usr/local/bin/envoy:ro

  # Entrypoint script (changes picked up on restart)
  - ./containers/envoy/entrypoint.sh:/entrypoint.sh:ro

  # Webui templates/static (hot-reload, just refresh browser)
  - ./internal/envoy/web:/app/web:ro

  # Docker socket (for sleeve management)
  - /var/run/docker.sock:/var/run/docker.sock

  # Claude credentials/settings (read-only)
  - ${HOME}/.claude/.credentials.json:/home/claude/.claude/.credentials.json:ro
  - ${HOME}/.claude.json:/etc/claude/settings.json:ro
  - ${HOME}/.claude/plugins:/home/claude/.claude/plugins:ro

  # Workspaces (all sleeve directories visible here)
  - ./workspaces:/home/claude/workspaces
```

### Production (docker-compose.yaml)

```yaml
volumes:
  # Docker socket (for sleeve management)
  - /var/run/docker.sock:/var/run/docker.sock

  # Claude credentials/settings (read-only)
  - ~/.claude/.credentials.json:/home/claude/.claude/.credentials.json:ro
  - ~/.claude.json:/etc/claude/settings.json:ro
  - ~/.claude/plugins:/home/claude/.claude/plugins:ro

  # Workspaces (all sleeve directories visible here)
  - ./workspaces:/home/claude/workspaces
```

## Ports

| Port | Dev | Prod | Purpose |
|------|-----|------|---------|
| 7470 | Yes | Yes | Envoy HTTP API and WebUI |
| 7681 | Yes | No | ttyd terminal (only needed for dev debugging) |

## Environment Variables

All configuration is via environment variables (no config file needed).
See `.env.example` for full list with defaults.

### Common to both

| Variable | Default | Purpose |
|----------|---------|---------|
| ENVOY_PORT | 7470 | HTTP server port |
| DOCKER_NETWORK | raven | Docker network name |
| WORKSPACE_ROOT | /home/claude/workspaces | Container path for workspaces |
| WORKSPACE_HOST_ROOT | (required) | Host path to workspaces |
| CREDENTIALS_HOST_PATH | | Host path to Claude credentials |
| SETTINGS_HOST_PATH | | Host path to Claude settings |
| PLUGINS_HOST_PATH | | Host path to Claude plugins |
| SLEEVE_IMAGE | ghcr.io/.../sleeve:latest | Docker image for sleeves |

### Dev only

| Variable | Value | Purpose |
|----------|-------|---------|
| DEV_MODE | true | Enables dev-specific behavior |
| SLEEVE_IMAGE | protectorate/sleeve:latest | Use local sleeve image |

## Build Commands

### Development Workflow

```bash
# First time setup (build base image)
make build-base          # ~2 min, run once

# Start dev environment
make dev                 # Builds bin/envoy + starts container

# Iterate on changes
# - Webui: just refresh browser
# - Go code: make dev-restart
# - entrypoint.sh: make dev-restart (script is mounted)

# View logs
make dev-logs

# Stop
make dev-down
```

### Production Workflow

```bash
# Build all images for release
make build-all           # Builds base, envoy (multi-stage), sleeve

# Or step by step
make build-base          # Only if base changed
make build-envoy-release # Full multi-stage build
make build-sleeve        # Sleeve image

# Start production
make up

# Stop production
make down
```

## Dockerfiles

### Base Image (containers/base/Dockerfile)

Shared by both envoy and sleeve. Contains:
- debian:bookworm-slim
- curl, ca-certificates, git, tmux
- ttyd (web terminal)
- claude user (uid 1000)
- Claude CLI

### Envoy Production (containers/envoy/Dockerfile)

Multi-stage build:
1. Stage 1: golang:1.24-alpine builds the envoy binary
2. Stage 2: Copies binary + entrypoint onto base image

Configuration is entirely via environment variables (no config file).

### Envoy Dev (containers/envoy/Dockerfile.dev)

Single-stage build:
- Copies pre-built binary from `bin/envoy`
- Copies entrypoint

Note: In practice, dev uses volume mounts instead of this Dockerfile.

### Sleeve (containers/sleeve/Dockerfile)

Simple layer on base:
- Copies entrypoint script

## Filesystem Paths

### Workspaces Location

All workspaces mount directly in the home directory for discoverability:

```
Envoy container (~):
  ~/workspaces/           <-- all sleeve directories
    alice/
      .cstack/
      project-files/
    bob/
      .cstack/
      project-files/

Sleeve container (~):
  ~/workspace/            <-- this sleeve's workspace
    .cstack/
    project-files/
```

To view sleeve workspaces from within the envoy container:
```bash
ls ~/workspaces
```

### Path Mapping (Host -> Container)

| Host Path | Envoy Container | Sleeve Container |
|-----------|-----------------|------------------|
| `./workspaces/` | `~/workspaces/` | N/A |
| `./workspaces/{name}/` | `~/workspaces/{name}/` | `~/workspace/` |

## Key Differences Summary

1. **Binary source**: Dev mounts from host (fast rebuilds), prod bakes into image

2. **Webui editing**: Dev supports hot-reload via mount, prod requires image rebuild

3. **ttyd port**: Dev exposes 7681 for terminal debugging, prod does not

4. **Container name**: envoy-dev vs envoy-poe (allows running both simultaneously if needed)

5. **Image registry**: Dev uses local images, prod pulls from ghcr.io
