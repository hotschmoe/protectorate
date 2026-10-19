# Protectorate Installation Design

This document describes the one-line installer for Protectorate.

---

## Goal

```bash
curl -fsSL https://raw.githubusercontent.com/hotschmoe/protectorate/master/install.sh | bash
```

User runs one command and ends up with:
- Docker verified (must be pre-installed)
- Protectorate container images pulled from ghcr.io
- Named volumes created for persistent data
- Envoy container running
- Ready to authenticate and spawn sleeves

---

## Installation Flow

```
+------------------+
| Check Docker     |
| installed?       |
+--------+---------+
         |
    No   |   Yes
    v    |    |
+--------+    |
| Error:      |
| Install     |
| Docker first|
+-------------+
         |
         v
+------------------+
| Create           |
| ~/protectorate   |
+--------+---------+
         |
         v
+------------------+
| Download         |
| docker-compose   |
+--------+---------+
         |
         v
+------------------+
| Pull pre-built   |
| images from      |
| ghcr.io          |
+--------+---------+
         |
         v
+------------------+
| docker compose   |
| up -d            |
+--------+---------+
         |
         v
+------------------+
| Wait for health  |
| check            |
+--------+---------+
         |
         v
    [Ready!]

    Open http://localhost:7470
    Run: envoy auth login claude --token <TOKEN>
```

---

## Post-Install Authentication

After installation, users authenticate via the envoy CLI or web UI:

```
+------------------+
| User opens       |
| localhost:7470   |
+--------+---------+
         |
         v
+------------------+
| Check Doctor tab |
| for auth status  |
+--------+---------+
         |
         v
+------------------+
| Get token from   |
| claude.ai or     |
| claude auth      |
+--------+---------+
         |
         v
+------------------+
| envoy auth login |
| claude --token   |
+--------+---------+
         |
         v
+------------------+
| Credentials      |
| stored in        |
| agent-creds vol  |
+--------+---------+
         |
         v
    [Authenticated!]

    Clone repos and spawn sleeves
```

---

## Data Architecture

All persistent data lives in Docker named volumes:

```
Named Volumes (managed by Docker)
+------------------------------------------+
|                                          |
|  agent-config     /home/agent/.config    |
|  (envoy settings, future YAML config)    |
|                                          |
|  agent-creds      /home/agent/.creds     |
|  +-- claude/credentials.json             |
|  +-- gemini/credentials.json             |
|  +-- codex/auth.json                     |
|  +-- git/id_ed25519, known_hosts         |
|                                          |
|  agent-workspaces /home/agent/workspaces |
|  +-- repo-1/                             |
|  +-- repo-2/                             |
|  +-- ...                                 |
|                                          |
+------------------------------------------+
```

Sleeves mount these volumes:
- Workspace: volume subpath for isolation
- Credentials: read-only for security

---

## Release Flow

When a version is tagged, GitHub Actions builds and pushes images:

```
git tag v1.0.0 && git push --tags
         |
         v
GitHub Actions triggers
         |
         +---> Build ghcr.io/hotschmoe/protectorate-base:v1.0.0
         +---> Build ghcr.io/hotschmoe/protectorate-envoy:v1.0.0
         +---> Build ghcr.io/hotschmoe/protectorate-sleeve:v1.0.0
         +---> Tag all as :latest
```

Users pull pre-built images - no local building required.

---

## Uninstall

```bash
cd ~/protectorate
docker compose down -v  # -v removes volumes (data loss!)
```

To preserve data but stop services:
```bash
docker compose down  # keeps volumes
```

Removes:
- All sleeve containers
- Envoy container
- Raven network
- With `-v`: all named volumes (credentials, workspaces)

Does NOT remove:
- Docker
- ~/protectorate directory
- Container images (use `docker image prune`)

---

## Current Implementation

### install.sh (~60 lines)

Simple installer that:
1. Verifies Docker is installed and running
2. Creates ~/protectorate directory
3. Downloads docker-compose.yaml
4. Pulls container images
5. Starts envoy via docker compose
6. Waits for health check

No configuration files, tokens, or complex setup required.

### docker-compose.yaml

Production compose file with named volumes:

```yaml
services:
  envoy:
    image: ghcr.io/hotschmoe/protectorate-envoy:latest
    container_name: envoy
    ports:
      - "7470:7470"
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock
      - agent-config:/home/agent/.config
      - agent-creds:/home/agent/.creds
      - agent-workspaces:/home/agent/workspaces
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

### Auth System

Credentials managed via CLI or HTTP API:

| Command | Purpose |
|---------|---------|
| `envoy auth` | Show auth status for all providers |
| `envoy auth login claude --token TOKEN` | Store Claude credentials |
| `envoy auth login gemini --token KEY` | Store Gemini API key |
| `envoy auth revoke claude` | Remove Claude credentials |

### Config System

Configuration via CLI or HTTP API:

| Command | Purpose |
|---------|---------|
| `envoy config` | Show all configuration |
| `envoy config get sleeves.max` | Get specific value |

---

## Decisions Made

| Question | Decision |
|----------|----------|
| Install location | Always `~/protectorate` |
| Data persistence | Docker named volumes |
| Authentication | Post-install via `envoy auth` |
| Configuration | Environment vars + `envoy config` |
| Image registry | ghcr.io/hotschmoe (public) |
| Container user | `agent` (UID 1000) |

---

## Files

| File | Purpose | Status |
|------|---------|--------|
| `install.sh` | Simple installer (~60 lines) | DONE |
| `docker-compose.yaml` | Production compose with named volumes | DONE |
| `docker-compose.dev.yaml` | Development compose | DONE |
| `.github/workflows/release.yaml` | GitHub Actions for image builds | DONE |

---

## Credential Symlinks

Inside containers, CLI tools expect credentials in specific locations. The entrypoints create symlinks:

```bash
~/.claude      -> ~/.creds/claude      # Claude Code
~/.config/gemini -> ~/.creds/gemini    # Gemini CLI
~/.codex       -> ~/.creds/codex       # Codex CLI
~/.ssh         -> ~/.creds/git         # Git SSH keys
```

This allows each CLI tool to find credentials in its expected location while we store everything centrally in `.creds/`.

---

## Security Considerations

1. **Piping to bash:** Standard practice, users can review with `curl ... | less` first

2. **Credential storage:** Stored in Docker named volume
   - Not accessible from host filesystem directly
   - Encrypted at rest if Docker uses encrypted storage driver
   - Sleeves get read-only access

3. **Docker socket:** Envoy has Docker socket access
   - Required for container management
   - Standard for orchestration tools

4. **Named volumes:** Data persists across container restarts
   - Use `docker compose down -v` to fully remove
   - Volumes survive image updates

---

## Comparison: Old vs New

| Aspect | Old (v0.x) | New (v1.0) |
|--------|------------|------------|
| Install script | ~450 lines | ~60 lines |
| Config files | .env, .env.example | None required |
| Auth setup | During install | Post-install CLI |
| Data storage | Host bind mounts | Named volumes |
| Container user | claude | agent |
| Credential paths | Host paths in .env | Named volume |
