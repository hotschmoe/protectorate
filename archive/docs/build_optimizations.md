# Build Optimizations for Fast Development

This document outlines optimizations to dramatically speed up the development workflow by separating DEV and PROD build configurations.

## Problem Statement

Current development cycle for any change:

| Change Type | Current Time | Steps Required |
|-------------|--------------|----------------|
| Go code | ~15 sec | Rebuild binary → Rebuild container → Restart |
| Webui HTML/CSS/JS | ~15 sec | Rebuild binary (embedded) → Rebuild container → Restart |
| Config changes | ~10 sec | Restart container |

The webui is embedded via `//go:embed`, so every frontend change requires a full Go recompile.

## Solution: Volume-Mounted Dev Environment

Mount host-built binaries and webui files directly into containers, eliminating container rebuilds entirely.

| Change Type | Optimized Time | Steps Required |
|-------------|----------------|----------------|
| Go code | ~5 sec | Rebuild binary → Restart process |
| Webui HTML/CSS/JS | **0 sec** | Refresh browser |
| Config changes | ~2 sec | Restart process |

---

## Phase 1: Implement Now

### 1.1 Create docker-compose.dev.yaml

Create a dev-specific compose file that mounts binaries and webui:

```yaml
# docker-compose.dev.yaml
# Development configuration with volume-mounted binaries and hot-reload webui
#
# Usage:
#   make dev        # Start dev environment
#   make dev-logs   # View logs
#   make dev-down   # Stop dev environment

version: '3.8'

services:
  envoy:
    image: protectorate/base:latest
    container_name: envoy-dev
    ports:
      - "7470:7470"
      - "7681:7681"
    volumes:
      # Mount pre-built binary (rebuild with: make bin/envoy)
      - ./bin/envoy:/usr/local/bin/envoy:ro

      # Mount webui for hot-reload (just refresh browser!)
      - ./internal/envoy/web:/app/web:ro

      # Mount configs
      - ./configs/envoy.yaml:/etc/envoy/envoy.yaml:ro

      # Docker socket for sleeve management
      - /var/run/docker.sock:/var/run/docker.sock

      # Claude credentials and settings
      - ${HOME}/.claude/.credentials.json:/home/claude/.claude/.credentials.json:ro
      - ${HOME}/.claude.json:/etc/claude/settings.json:ro
      - ${HOME}/.claude/plugins:/home/claude/.claude/plugins:ro

      # Workspaces
      - ./workspaces:/workspaces

    environment:
      - DEV_MODE=true
      - WORKSPACE_HOST_ROOT=${PWD}/workspaces
      - CREDENTIALS_HOST_PATH=${HOME}/.claude/.credentials.json
      - SETTINGS_HOST_PATH=${HOME}/.claude.json
      - PLUGINS_HOST_PATH=${HOME}/.claude/plugins
    networks:
      - raven
    user: root
    entrypoint: ["/bin/sh", "-c"]
    command:
      - |
        # Fix permissions
        chown -R claude:claude /home/claude/.claude 2>/dev/null || true
        chown -R claude:claude /workspaces 2>/dev/null || true

        # Run envoy as claude user
        exec su claude -c '/usr/local/bin/envoy --config /etc/envoy/envoy.yaml'
    restart: unless-stopped

networks:
  raven:
    name: raven
```

### 1.2 Modify handlers.go for DEV_MODE

Add filesystem serving when `DEV_MODE=true`:

```go
// In internal/envoy/handlers.go

import (
    "os"
    "path/filepath"
)

func (s *Server) serveIndex(w http.ResponseWriter, r *http.Request) {
    // DEV_MODE: Serve from filesystem for hot-reload
    if os.Getenv("DEV_MODE") == "true" {
        // Try mounted path first, then local path
        paths := []string{
            "/app/web/templates/index.html",           // Mounted in container
            "./internal/envoy/web/templates/index.html", // Local development
        }
        for _, path := range paths {
            if _, err := os.Stat(path); err == nil {
                http.ServeFile(w, r, path)
                return
            }
        }
    }

    // PROD: Serve from embedded filesystem
    content, err := webFS.ReadFile("web/templates/index.html")
    if err != nil {
        http.Error(w, "Failed to load index", http.StatusInternalServerError)
        return
    }
    w.Header().Set("Content-Type", "text/html; charset=utf-8")
    w.Write(content)
}

// Also update static file serving if needed
func (s *Server) serveStatic(w http.ResponseWriter, r *http.Request) {
    filename := filepath.Base(r.URL.Path)

    if os.Getenv("DEV_MODE") == "true" {
        paths := []string{
            filepath.Join("/app/web/static", filename),
            filepath.Join("./internal/envoy/web/static", filename),
        }
        for _, path := range paths {
            if _, err := os.Stat(path); err == nil {
                http.ServeFile(w, r, path)
                return
            }
        }
    }

    // PROD: Serve from embedded filesystem
    content, err := webFS.ReadFile(filepath.Join("web/static", filename))
    if err != nil {
        http.NotFound(w, r)
        return
    }
    http.ServeContent(w, r, filename, time.Time{}, bytes.NewReader(content))
}
```

### 1.3 Add Makefile Targets

Add these targets to the Makefile:

```makefile
# =============================================================================
# Development Targets
# =============================================================================

.PHONY: dev dev-down dev-logs dev-restart dev-rebuild

# Start development environment with volume-mounted binary and hot-reload webui
dev: bin/envoy
	@echo "Starting dev environment..."
	@echo "  - Binary mounted from ./bin/envoy"
	@echo "  - Webui hot-reload enabled (just refresh browser)"
	@echo "  - API available at http://localhost:7470"
	docker-compose -f docker-compose.dev.yaml up -d
	@echo ""
	@echo "Dev environment ready!"
	@echo "  Webui: http://localhost:7470"
	@echo "  Logs:  make dev-logs"

# Stop development environment
dev-down:
	docker-compose -f docker-compose.dev.yaml down

# View development logs
dev-logs:
	docker-compose -f docker-compose.dev.yaml logs -f

# Restart envoy process (after Go code changes)
dev-restart: bin/envoy
	docker-compose -f docker-compose.dev.yaml restart envoy
	@echo "Envoy restarted with new binary"

# Full rebuild and restart (rarely needed)
dev-rebuild: bin/envoy
	docker-compose -f docker-compose.dev.yaml up -d --force-recreate
	@echo "Dev environment rebuilt"

# =============================================================================
# Production/Release Targets (existing)
# =============================================================================

.PHONY: release release-build

# Build production containers (full multi-stage builds)
release: build-all
	@echo "Production build complete"

# Alias for clarity
release-build: build-envoy-release build-sleeve
	@echo "Release containers built"
```

### 1.4 Update .gitignore

Ensure binaries are not committed:

```gitignore
# Local binaries (built on host, mounted into containers)
bin/
```

---

## Development Workflow

### Initial Setup (One Time)

```bash
# 1. Build the base image (only needed once, or when upgrading Claude CLI)
make build-base

# 2. Build the sleeve image (only needed once, or when changing sleeve config)
make build-sleeve

# 3. Start dev environment
make dev
```

### Daily Development

#### Webui Changes (HTML/CSS/JS)

```bash
# 1. Edit files in internal/envoy/web/templates/
vim internal/envoy/web/templates/index.html

# 2. Refresh browser - changes appear instantly!
# No rebuild, no restart needed
```

#### Go Code Changes

```bash
# 1. Edit Go files
vim internal/envoy/handlers.go

# 2. Rebuild binary and restart
make dev-restart

# Total time: ~5 seconds
```

#### Config Changes

```bash
# 1. Edit config
vim configs/envoy.yaml

# 2. Restart to pick up changes
make dev-restart
```

### Deploying to Production

```bash
# Full multi-stage build (creates optimized, embedded containers)
make release

# Or step by step:
make build-envoy-release  # Multi-stage Go build in Docker
make build-sleeve         # Build sleeve container

# Deploy with production compose
docker-compose up -d
```

---

## Phase 2: Future Enhancement - File Watcher

### 2.1 Install inotify-tools

```bash
# Ubuntu/Debian
sudo apt-get install inotify-tools

# Already available on most Linux systems
```

### 2.2 Create Watch Script

Create `scripts/watch.sh`:

```bash
#!/bin/bash
# scripts/watch.sh
# Watches for Go file changes and auto-rebuilds

set -e

echo "=========================================="
echo "  Protectorate Dev Watcher"
echo "=========================================="
echo ""
echo "Watching for changes in:"
echo "  - cmd/**/*.go"
echo "  - internal/**/*.go"
echo ""
echo "Press Ctrl+C to stop"
echo ""

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

rebuild() {
    echo -e "${YELLOW}[$(date +%H:%M:%S)] Change detected, rebuilding...${NC}"

    if make bin/envoy 2>&1; then
        echo -e "${GREEN}[$(date +%H:%M:%S)] Build successful, restarting envoy...${NC}"
        docker-compose -f docker-compose.dev.yaml restart envoy 2>&1
        echo -e "${GREEN}[$(date +%H:%M:%S)] Ready!${NC}"
    else
        echo -e "\033[0;31m[$(date +%H:%M:%S)] Build failed!${NC}"
    fi
    echo ""
}

# Initial build
rebuild

# Watch for changes
while true; do
    # Wait for any .go file to change
    inotifywait -r -e modify,create,delete \
        --include '.*\.go$' \
        ./cmd ./internal 2>/dev/null

    # Small delay to batch rapid changes
    sleep 0.5

    rebuild
done
```

Make executable:
```bash
chmod +x scripts/watch.sh
```

### 2.3 Add Watch Makefile Target

```makefile
.PHONY: watch

# Auto-rebuild on Go file changes
watch:
	@./scripts/watch.sh
```

### 2.4 Alternative: Use air (Go Live Reload)

For more sophisticated watching, consider [air](https://github.com/cosmtrek/air):

```bash
# Install air
go install github.com/cosmtrek/air@latest
```

Create `.air.toml`:

```toml
root = "."
tmp_dir = "tmp"

[build]
  cmd = "make bin/envoy && docker-compose -f docker-compose.dev.yaml restart envoy"
  bin = "bin/envoy"
  include_ext = ["go"]
  exclude_dir = ["tmp", "bin", "workspaces", "containers"]
  delay = 500

[log]
  time = true

[color]
  main = "yellow"
  watcher = "cyan"
  build = "green"
```

Then run:
```bash
air
```

---

## Architecture Overview

```
                    DEVELOPMENT                         PRODUCTION
                    ===========                         ==========

    +-------------+                           +-------------------+
    |  Host OS    |                           |  Docker Build     |
    |-------------|                           |-------------------|
    | go build    |                           | Multi-stage       |
    | (3 sec)     |                           | golang:1.24       |
    +------+------+                           | (2+ min)          |
           |                                  +--------+----------+
           v                                           |
    +-------------+                                    v
    | bin/envoy   |                           +-------------------+
    | (mounted)   |                           | Embedded binary   |
    +------+------+                           | in container      |
           |                                  +--------+----------+
           v                                           |
    +---------------------------+             +-------------------+
    | docker-compose.dev.yaml   |             | docker-compose.yaml|
    |---------------------------|             |-------------------|
    | - Mount bin/envoy         |             | - Full build      |
    | - Mount web/ (hot-reload) |             | - Embedded assets |
    | - DEV_MODE=true           |             | - Optimized       |
    +---------------------------+             +-------------------+
           |                                           |
           v                                           v
    +---------------------------+             +-------------------+
    | envoy-dev container       |             | envoy container   |
    | (base image only)         |             | (full image)      |
    +---------------------------+             +-------------------+

    Webui changes: 0 sec (refresh)            Webui changes: 2+ min
    Go changes: 5 sec (rebuild+restart)       Go changes: 2+ min
```

---

## Checklist for Implementation

### Phase 1 (Implement Now)

- [ ] Create `docker-compose.dev.yaml`
- [ ] Modify `internal/envoy/handlers.go` for DEV_MODE
- [ ] Add dev targets to `Makefile`
- [ ] Update `.gitignore` for `bin/`
- [ ] Test workflow:
  - [ ] `make dev` starts environment
  - [ ] Webui changes appear on browser refresh
  - [ ] `make dev-restart` applies Go changes
  - [ ] `make release` still builds production containers

### Phase 2 (Future)

- [ ] Create `scripts/watch.sh`
- [ ] Add `watch` target to Makefile
- [ ] Consider `air` for more advanced watching
- [ ] Optional: Add browser auto-refresh (LiveReload)

---

## Troubleshooting

### Binary not found in container

```bash
# Check binary exists
ls -la bin/envoy

# Rebuild if needed
make bin/envoy
```

### Permission denied on binary

```bash
# Ensure executable
chmod +x bin/envoy
```

### Webui not hot-reloading

1. Check `DEV_MODE=true` is set:
   ```bash
   docker-compose -f docker-compose.dev.yaml exec envoy env | grep DEV
   ```

2. Check mount is working:
   ```bash
   docker-compose -f docker-compose.dev.yaml exec envoy ls -la /app/web/templates/
   ```

### Container won't start

```bash
# Check logs
make dev-logs

# Rebuild base image if needed
make build-base
```

---

## Summary

| Command | Purpose | When to Use |
|---------|---------|-------------|
| `make dev` | Start dev environment | Beginning of dev session |
| `make dev-restart` | Rebuild Go + restart | After Go code changes |
| `make dev-logs` | View container logs | Debugging |
| `make dev-down` | Stop dev environment | End of session |
| `make release` | Full production build | Before deployment |
| `make watch` | Auto-rebuild on changes | Phase 2 enhancement |
