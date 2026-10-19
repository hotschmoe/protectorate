# Protectorate Configuration

This document describes how Protectorate handles configuration using YAML files.

---

## Overview

Protectorate uses a layered configuration system with the following precedence (highest to lowest):

```
Environment Variables  >  YAML Config File  >  Default Values
     (runtime)              (persisted)          (built-in)
```

Configuration is stored in a Docker volume and persists across container restarts.

---

## Configuration File

**Location:** `/home/agent/.config/envoy.yaml`

**Volume:** `agent-config` (named Docker volume)

The YAML file is automatically created with defaults on first run. You can modify it via:
- **WebUI:** Config tab in the Protectorate dashboard
- **CLI:** `envoy config set <key> <value>`
- **HTTP API:** `PUT /api/config/<key>`

---

## Default Configuration

```yaml
# Protectorate Envoy Configuration
# Modify via WebUI or CLI: envoy config set <key> <value>
# Changes require envoy restart to take effect.

server:
  port: 7470

sleeves:
  max: 10                    # Maximum concurrent sleeve containers (1-100)
  poll_interval: 1h          # How often to check sleeve health
  idle_threshold: "0s"       # Auto-kill after idle (0s = never)
  image: ghcr.io/hotschmoe/protectorate-sleeve:latest

docker:
  network: raven             # Docker network for envoy and sleeves

git:
  clone_protocol: ssh        # ssh | https
  committer:
    name: ""                 # Git user.name for commits
    email: ""                # Git user.email for commits

gitea:
  enabled: false
  url: http://gitea:3000

mirror:
  enabled: false
  frequency: daily
```

---

## Configuration Keys

### Server

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `server.port` | int | 7470 | HTTP server port (read-only at runtime) |

### Sleeves

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `sleeves.max` | int | 10 | Maximum concurrent sleeve containers (1-100) |
| `sleeves.poll_interval` | duration | 1h | Interval for polling sleeve health |
| `sleeves.idle_threshold` | duration | 0 | Auto-kill after idle time (0 = never) |
| `sleeves.image` | string | ghcr.io/hotschmoe/protectorate-sleeve:latest | Docker image for sleeves |

### Docker

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `docker.network` | string | raven | Docker network name |
| `docker.workspace_root` | string | /home/agent/workspaces | Container path for workspaces (read-only) |

### Git

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `git.clone_protocol` | string | ssh | Default protocol: `ssh` or `https` |
| `git.committer.name` | string | "" | Git user.name for commits made by sleeves |
| `git.committer.email` | string | "" | Git user.email for commits made by sleeves |

---

## CLI Commands

### View Configuration

```bash
# View all configuration
docker exec envoy envoy config get

# View specific key
docker exec envoy envoy config get sleeves.max
docker exec envoy envoy config get git.clone_protocol

# JSON output
docker exec envoy envoy config get --json
```

### Modify Configuration

```bash
# Set a value
docker exec envoy envoy config set sleeves.max 15
docker exec envoy envoy config set git.clone_protocol https
docker exec envoy envoy config set git.committer.name "Your Name"
docker exec envoy envoy config set git.committer.email "you@example.com"
```

**Note:** Changes require envoy restart to take effect:
```bash
docker compose restart envoy
```

---

## HTTP API

### Get All Configuration

```http
GET /api/config
```

Response:
```json
{
  "server": { "port": 7470 },
  "sleeves": {
    "max": 10,
    "poll_interval": "1h",
    "idle_threshold": "0",
    "image": "ghcr.io/hotschmoe/protectorate-sleeve:latest"
  },
  "docker": {
    "network": "raven",
    "workspace_root": "/home/agent/workspaces"
  },
  "git": {
    "clone_protocol": "ssh",
    "committer": { "name": "", "email": "" }
  }
}
```

### Get Single Key

```http
GET /api/config/sleeves.max
```

Response:
```json
{ "key": "sleeves.max", "value": 10 }
```

### Set Value

```http
PUT /api/config/sleeves.max
Content-Type: application/json

{ "value": "15" }
```

Response:
```json
{ "key": "sleeves.max", "value": 15, "message": "saved - restart envoy to apply changes" }
```

### Reset to Default

```http
DELETE /api/config/sleeves.max
```

Response:
```json
{ "key": "sleeves.max", "value": 10, "message": "reset to default - restart envoy to apply changes" }
```

### Restart Envoy

Trigger a graceful restart of the Envoy service (relies on Docker restart policy):

```http
POST /api/restart
```

Response:
```json
{ "status": "restarting", "message": "Envoy will restart shortly. Docker will bring it back up." }
```

---

## Environment Variable Overrides

For advanced users and automation, environment variables override config file values at runtime. This is useful for Docker Compose overrides or CI/CD.

| Env Var | Config Key |
|---------|------------|
| `ENVOY_PORT` | server.port |
| `ENVOY_MAX_SLEEVES` | sleeves.max |
| `ENVOY_POLL_INTERVAL` | sleeves.poll_interval |
| `ENVOY_IDLE_THRESHOLD` | sleeves.idle_threshold |
| `SLEEVE_IMAGE` | sleeves.image |
| `DOCKER_NETWORK` | docker.network |
| `WORKSPACE_ROOT` | docker.workspace_root |
| `GIT_CLONE_PROTOCOL` | git.clone_protocol |

Example:
```yaml
# docker-compose.override.yaml
services:
  envoy:
    environment:
      ENVOY_MAX_SLEEVES: 20
      SLEEVE_IMAGE: my-custom-sleeve:latest
```

---

## WebUI Config Tab

The Protectorate WebUI includes a Config tab that provides:

1. **Authentication Section**
   - Input API keys for Claude, Gemini, Codex
   - View authentication status for each provider
   - Revoke credentials

2. **Sleeves Section**
   - Configure max sleeves, poll interval, idle threshold
   - Set sleeve Docker image

3. **Git Section**
   - Set clone protocol (SSH vs HTTPS)
   - Configure committer identity (name and email)

4. **Docker Section** (read-only)
   - View network and workspace root settings

5. **Integrations Section**
   - Gitea and GitHub Mirror (coming soon)

---

## Backup and Restore

### Backup Configuration

```bash
# Export config volume
docker run --rm -v agent-config:/config -v $(pwd):/backup alpine \
  tar czf /backup/config-backup.tar.gz -C /config .
```

### Restore Configuration

```bash
# Import config volume
docker run --rm -v agent-config:/config -v $(pwd):/backup alpine \
  tar xzf /backup/config-backup.tar.gz -C /config
```

---

## Troubleshooting

### Config file not found

If the config file doesn't exist, Protectorate uses defaults. To create it explicitly:
```bash
docker exec envoy envoy config set sleeves.max 10
```

### Changes not taking effect

Configuration changes require an envoy restart:
```bash
docker compose restart envoy
```

### Invalid configuration values

The API validates values before saving:
- `sleeves.max` must be 1-100
- `git.clone_protocol` must be `ssh` or `https`
- Duration values must be valid Go durations (e.g., `1h`, `30m`, `5s`)

### View current config file

```bash
docker exec envoy cat /home/agent/.config/envoy.yaml
```

---

## Migration from Environment Variables

If upgrading from a version that used environment variables in docker-compose.yaml:

1. Remove environment variables from docker-compose.yaml
2. Start envoy (it will use defaults)
3. Configure via WebUI or CLI:
   ```bash
   docker exec envoy envoy config set sleeves.max 15
   docker exec envoy envoy config set git.committer.name "Your Name"
   docker exec envoy envoy config set git.committer.email "you@example.com"
   ```

The new system is simpler: configuration lives in the container volume, not on the host.
