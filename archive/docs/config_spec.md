# Configuration Specification: Internalized Config

## Overview

This document specifies a new configuration architecture that:
1. Eliminates the `.env` file on host
2. Moves all configuration inside the Envoy container
3. Uses named volumes for persistence
4. Dramatically simplifies installation

**Related documents to update after implementation:**
- `install.sh` - Complete rewrite (simpler!)
- `docs/protectorate_install_convo.md` - Update flow diagrams
- `docker-compose.yaml` - Remove environment variables
- `.env.example` - Delete or convert to reference doc

---

## Current State: Host-Dependent Configuration

```
HOST MACHINE
┌─────────────────────────────────────────────────────────────────────────┐
│                                                                         │
│  ~/protectorate/                                                        │
│  ├── .env                    <-- 15+ variables, host paths, secrets     │
│  ├── docker-compose.yaml     <-- References .env                        │
│  └── workspaces/             <-- Host filesystem                        │
│                                                                         │
│  ~/.claude/                                                             │
│  ├── .credentials.json       <-- Must exist before install              │
│  └── plugins/                                                           │
│                                                                         │
│  ~/.claude.json              <-- Onboarding flag                        │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
           │
           │ bind mounts, env vars
           v
     ┌───────────┐
     │   Envoy   │
     └───────────┘
```

**Problems:**
- Complex install script (handle auth, generate .env, set paths)
- Platform-specific paths (`$HOME/.claude/`)
- Secrets in plain text `.env` file
- Must authenticate BEFORE starting containers
- Can't run in VM/cloud without host setup

---

## Proposed State: Self-Contained Configuration

```
HOST MACHINE
┌─────────────────────────────────────────────────────────────────────────┐
│                                                                         │
│  (nothing needed except Docker)                                         │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
           │
           │ docker socket only
           v
     ENVOY CONTAINER
     ┌─────────────────────────────────────────────────────────────────┐
     │                                                                 │
     │  Named Volume: agent-config                                     │
     │  └── /home/agent/.config/envoy.yaml   (all settings)            │
     │                                                                 │
     │  Named Volume: agent-creds                                      │
     │  └── /home/agent/.creds/              (all credentials)         │
     │                                                                 │
     │  Named Volume: agent-workspaces                                 │
     │  └── /home/agent/workspaces/          (all code)                │
     │                                                                 │
     │  CLI commands to configure:                                     │
     │    envoy config set max_sleeves 12                              │
     │    envoy auth claude                                            │
     │    envoy clone https://github.com/user/repo                     │
     │                                                                 │
     └─────────────────────────────────────────────────────────────────┘
```

**Benefits:**
- Trivial install: `docker compose up -d`
- Configure AFTER starting (not before)
- Platform agnostic (same on Linux, Mac, VM, cloud)
- No secrets on host filesystem
- Self-documenting via CLI

---

## Configuration Storage

### Config File: `/home/agent/.config/envoy.yaml`

Stored in `agent-config` named volume. Persists across container restarts.

```yaml
# Envoy Configuration
# Modify via: envoy config set <key> <value>
# View via:   envoy config get

server:
  port: 7470

sleeves:
  max: 10
  poll_interval: 1h
  idle_threshold: 0       # 0 = never auto-kill
  image: protectorate/sleeve:latest

docker:
  network: raven

git:
  clone_protocol: ssh     # ssh | https
  committer:
    name: ""              # Set via: envoy config git.name "John Doe"
    email: ""             # Set via: envoy config git.email "john@example.com"

# Optional integrations
gitea:
  enabled: false
  url: ""
  token: ""

mirror:
  enabled: false
  github_org: ""
  frequency: daily
```

### Credentials: `/home/agent/.creds/`

Stored in `agent-creds` named volume. Structure per auth_spec_v2.md:

```
/home/agent/.creds/
├── claude/
│   └── credentials.json
├── gemini/
│   └── credentials.json
├── codex/
│   └── credentials.json
├── git/
│   ├── id_ed25519
│   └── known_hosts
└── .auth-state.json
```

---

## CLI Commands for Configuration

### `envoy config` - View/Modify Settings

```bash
# View all configuration
envoy config get
envoy config get --json

# View specific setting
envoy config get sleeves.max
envoy config get git.committer.name

# Set values (writes to envoy.yaml, persists in volume)
envoy config set sleeves.max 12
envoy config set git.committer.name "John Doe"
envoy config set git.committer.email "john@example.com"
envoy config set git.clone_protocol https

# Reset to default
envoy config reset sleeves.max
envoy config reset --all
```

### HTTP Endpoints

Following CLI-HTTP ethos, CLI wraps HTTP:

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/api/config` | GET | Get all config |
| `/api/config/{key}` | GET | Get specific key |
| `/api/config/{key}` | PUT | Set value |
| `/api/config/{key}` | DELETE | Reset to default |

---

## Environment Variable Overrides

For advanced users and automation, environment variables can override config file values. **This is optional** - most users won't need it.

```bash
# Override without modifying config file
docker run -e ENVOY_MAX_SLEEVES=20 protectorate/envoy
```

### Precedence

```
Environment Variable  >  Config File  >  Default Value
     (runtime)           (persisted)      (code)
```

### Supported Overrides

| Env Var | Config Key | Default |
|---------|------------|---------|
| `ENVOY_PORT` | `server.port` | 7470 |
| `ENVOY_MAX_SLEEVES` | `sleeves.max` | 10 |
| `ENVOY_POLL_INTERVAL` | `sleeves.poll_interval` | 1h |
| `ENVOY_IDLE_THRESHOLD` | `sleeves.idle_threshold` | 0 |
| `ENVOY_SLEEVE_IMAGE` | `sleeves.image` | protectorate/sleeve:latest |
| `DOCKER_NETWORK` | `docker.network` | raven |

**Note:** Credentials are NEVER passed via environment variables. Use `envoy auth` commands.

---

## New docker-compose.yaml

```yaml
# Protectorate - Zero Config Install
# Just run: docker compose up -d
# Then configure inside: docker exec -it envoy envoy auth claude

services:
  envoy:
    image: ghcr.io/hotschmoe/protectorate-envoy:latest
    container_name: envoy
    ports:
      - "7470:7470"
    volumes:
      # Required: Docker control
      - /var/run/docker.sock:/var/run/docker.sock

      # Persistent data (all config/creds/workspaces)
      - agent-config:/home/agent/.config
      - agent-creds:/home/agent/.creds
      - agent-workspaces:/home/agent/workspaces

      # Optional: Hardware stats for dashboard
      - /proc:/host/proc:ro
    networks:
      - raven
    restart: unless-stopped

networks:
  raven:
    name: raven

volumes:
  agent-config:
  agent-creds:
  agent-workspaces:
```

**What's removed:**
- All `environment:` entries
- All host path bind mounts (`~/.claude/`, `./workspaces/`)
- Need for `.env` file

---

## New Install Flow

### Before (Current)

```
┌──────────────────────────────────────────────────────────────────────┐
│ install.sh                                                           │
│                                                                      │
│  1. Check/install Docker                                             │
│  2. Check/install Claude CLI        <-- On HOST                      │
│  3. Run claude auth login           <-- On HOST, opens browser       │
│  4. Generate long-lived token       <-- Complex OAuth flow           │
│  5. Clone repo to ~/protectorate                                     │
│  6. Create .env with token + paths  <-- Error-prone, secrets on disk │
│  7. Pull images                                                      │
│  8. docker compose up                                                │
│                                                                      │
│  User must: authenticate BEFORE containers start                     │
│  Friction:  High (OAuth flow, .env generation, path resolution)      │
└──────────────────────────────────────────────────────────────────────┘
```

### After (Proposed)

```
┌──────────────────────────────────────────────────────────────────────┐
│ install.sh                                                           │
│                                                                      │
│  1. Check/install Docker                                             │
│  2. Download docker-compose.yaml (one file!)                         │
│  3. docker compose up -d                                             │
│  4. Print "run these commands to configure"                          │
│                                                                      │
│  User then (inside container or via docker exec):                    │
│    envoy auth claude      # Opens browser, handles OAuth             │
│    envoy config git.name "John Doe"                                  │
│    envoy clone https://github.com/user/repo                          │
│                                                                      │
│  User must: Only have Docker                                         │
│  Friction:  Minimal (one curl, one compose up)                       │
└──────────────────────────────────────────────────────────────────────┘
```

### New install.sh (Sketch)

```bash
#!/usr/bin/env bash
set -e

COMPOSE_URL="https://raw.githubusercontent.com/hotschmoe/protectorate/master/docker-compose.yaml"
INSTALL_DIR="$HOME/protectorate"

echo "Installing Protectorate..."

# 1. Check Docker
if ! command -v docker &> /dev/null; then
    echo "Docker not found. Installing..."
    curl -fsSL https://get.docker.com | sh
    sudo usermod -aG docker "$USER"
    echo "Please log out and back in, then re-run this script."
    exit 0
fi

# 2. Download compose file
mkdir -p "$INSTALL_DIR"
cd "$INSTALL_DIR"
curl -fsSL "$COMPOSE_URL" -o docker-compose.yaml

# 3. Start containers
docker compose up -d

# 4. Wait for ready
echo "Waiting for Envoy..."
for i in {1..30}; do
    if curl -s http://localhost:7470/health > /dev/null 2>&1; then
        break
    fi
    sleep 1
done

# 5. Done!
echo ""
echo "========================================"
echo "  Protectorate is running!"
echo "========================================"
echo ""
echo "Next steps - run these commands:"
echo ""
echo "  # Authenticate Claude Code"
echo "  docker exec -it envoy envoy auth claude"
echo ""
echo "  # Set your git identity"
echo "  docker exec -it envoy envoy config git.name \"Your Name\""
echo "  docker exec -it envoy envoy config git.email \"you@example.com\""
echo ""
echo "  # Clone a repository"
echo "  docker exec -it envoy envoy clone https://github.com/you/repo"
echo ""
echo "  # Open the web UI"
echo "  open http://localhost:7470"
echo ""
```

**Lines of code:** ~50 (down from ~450)
**Complexity:** Trivial (down from OAuth flows, .env generation, path handling)

---

## WebUI First-Run Experience

For users who prefer GUI, the WebUI can guide setup:

```
┌─────────────────────────────────────────────────────────────────────────┐
│ PROTECTORATE                                              [Setup Mode] │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  Welcome to Protectorate!                                               │
│                                                                         │
│  Complete these steps to get started:                                   │
│                                                                         │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │  1. AUTHENTICATE AI PROVIDER                        [Required]   │   │
│  │                                                                  │   │
│  │  [ Authenticate Claude Code ]  [ Authenticate Gemini ]          │   │
│  │                                                                  │   │
│  │  Status: Not authenticated                                       │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│                                                                         │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │  2. CONFIGURE GIT IDENTITY                          [Recommended]│   │
│  │                                                                  │   │
│  │  Name:  [_________________________]                              │   │
│  │  Email: [_________________________]                              │   │
│  │                                                [ Save ]          │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│                                                                         │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │  3. CLONE A REPOSITORY                              [Optional]   │   │
│  │                                                                  │   │
│  │  URL: [_________________________] [ Clone ]                      │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│                                                                         │
│  [ Skip Setup - I'll configure later ]                                  │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## Migration Path: Full Port (No Legacy Code)

Per project guidelines: **No backwards compatibility. Clean break. Full migration.**

### Single Release: Complete Transition

When implemented, this is a breaking change that:

1. **Removes** all `.env` file support
2. **Removes** all host credential mount support
3. **Removes** `WORKSPACE_HOST_ROOT`, `CREDENTIALS_HOST_PATH`, etc. from config
4. **Deletes** `.env.example`
5. **Rewrites** `docker-compose.yaml` (volumes only, no env vars)
6. **Rewrites** `install.sh` (50 lines, not 450)
7. **Updates** all documentation in same commit

### Implementation Order

```
1. User rename: claude -> agent (all Dockerfiles, entrypoints, Go code)
2. Config system: envoy.yaml + CLI commands + HTTP endpoints
3. Auth system: envoy auth + agent-creds volume + symlinks
4. Workspace system: agent-workspaces volume + git commands
5. Remove old code: delete host path config, .env support, credential mounts
6. Update compose files: volumes only
7. Rewrite install.sh
8. Update all docs
9. Single commit, single release
```

### For Existing Users

Existing users must:
1. Re-authenticate inside envoy (`envoy auth claude`)
2. Re-clone repositories (`envoy clone <url>`)
3. Re-configure git identity (`envoy config git.name/email`)

This is acceptable because:
- Protectorate is pre-1.0, breaking changes expected
- New flow is simpler (users benefit)
- Clean codebase > migration complexity
- Workspaces can be re-cloned (git is the source of truth)

---

## Files to Update (All in Single Release)

| File | Change |
|------|--------|
| `containers/base/Dockerfile` | Rename user `claude` -> `agent` |
| `containers/envoy/Dockerfile` | Update paths to `/home/agent/` |
| `containers/envoy/entrypoint.sh` | New user, symlink setup, default config |
| `containers/sleeve/Dockerfile` | Update paths to `/home/agent/` |
| `containers/sleeve/entrypoint.sh` | New user, symlink setup |
| `internal/config/config.go` | Load from YAML file + env overrides, remove host paths |
| `cmd/envoy/config.go` | New `envoy config` subcommand |
| `cmd/envoy/auth.go` | New `envoy auth` subcommand |
| `internal/envoy/config_handlers.go` | New `/api/config` endpoints |
| `internal/envoy/auth_handlers.go` | New `/api/auth` endpoints |
| `internal/envoy/sleeve_manager.go` | Volume mounts instead of bind mounts |
| `internal/sidecar/auth.go` | Update paths to `/home/agent/` |
| `docker-compose.yaml` | Named volumes only, no env vars |
| `docker-compose.dev.yaml` | Same changes |
| `install.sh` | Complete rewrite (~50 lines) |
| `docs/protectorate_install_convo.md` | Update flow diagrams |
| `.env.example` | **DELETE** |

---

## Comparison: Before and After

| Aspect | Before | After |
|--------|--------|-------|
| Install steps | 8 | 3 |
| Files on host | 4+ (.env, compose, workspaces, credentials) | 1 (compose only) |
| Secrets on host | Yes (.env, ~/.claude/) | No (all in volumes) |
| Auth timing | Before install | After install |
| Platform support | Linux (paths hardcoded) | Any (Docker only) |
| Config location | Host .env | Container volume |
| Config method | Edit file | CLI commands |
| Upgrade path | Re-run install, regenerate .env | `docker compose pull && up` |

---

## Open Questions

1. **Config file format**: YAML (proposed) vs JSON vs TOML?
   - YAML: Human-readable, supports comments
   - JSON: Native to Go, no comments
   - Recommendation: YAML

2. **First-run detection**: How does envoy know it's first run?
   - Check if config file exists
   - Check if any auth providers configured
   - Show setup wizard in WebUI if not configured

3. **Config validation**: What if user sets invalid values?
   - Validate on write
   - Reject invalid values with clear error
   - Never write invalid config

4. **Backup/restore**: How do users backup their config?
   ```bash
   # Backup all volumes
   docker run --rm -v agent-config:/c -v agent-creds:/r -v agent-workspaces:/w \
     -v $(pwd):/backup alpine tar czf /backup/protectorate-backup.tar.gz /c /r /w
   ```

5. **Multi-instance**: What if user wants two Protectorate instances?
   - Use different volume names: `agent-config-dev`, `agent-config-prod`
   - Or: different compose project names
