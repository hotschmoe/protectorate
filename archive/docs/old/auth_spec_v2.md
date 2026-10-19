# Authentication Specification v2: Envoy-Centric Auth

## Overview

This document specifies a new authentication architecture for Protectorate that:
1. Centralizes authentication in the Envoy container
2. Eliminates dependency on host machine credentials
3. Supports multiple AI CLIs (Claude Code, Gemini CLI, Codex)
4. Enables platform-agnostic deployment (VM, local, cloud)

---

## Source of Truth: HTTP API

Following the CLI-HTTP ethos (`docs/cli_http_ethos.md`), the HTTP daemon is the single source of truth:

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           ENVOY CONTAINER                               │
│                                                                         │
│  ┌──────────┐      HTTP       ┌──────────────┐      ┌───────────────┐  │
│  │ envoy    │ ─────────────>  │ envoy serve  │ <──> │ Named Volumes │  │
│  │ auth     │                 │ (daemon)     │      │ - agent-creds │  │
│  │ CLI      │                 │              │      │ - workspaces  │  │
│  └──────────┘                 │ Source of    │      └───────────────┘  │
│       ^                       │ Truth        │             ^           │
│       │                       └──────────────┘             │           │
│       │                             ^                      │           │
│  Poe uses CLI                       │ HTTP            Volume I/O       │
│                                     │                      │           │
│                              ┌──────────────┐              │           │
│                              │   Web UI     │──────────────┘           │
│                              │   (human)    │                          │
│                              └──────────────┘                          │
└─────────────────────────────────────────────────────────────────────────┘
```

**Key principle**: CLI commands are thin wrappers around HTTP endpoints. All state mutations go through the daemon.

```bash
# These are equivalent:
envoy auth status --json
curl http://localhost:7470/api/auth/status

# Auth operations go through HTTP
envoy auth claude    # POST /api/auth/claude/login
envoy auth revoke    # DELETE /api/auth/claude
```

### Consistency Across Domains

| Domain | CLI | HTTP (Source of Truth) | Storage |
|--------|-----|------------------------|---------|
| Sleeves | `envoy spawn/kill/status` | `/api/sleeves/*` | Docker daemon |
| Auth | `envoy auth` | `/api/auth/*` | `agent-creds` volume |
| Workspaces | `envoy clone/pull/push` | `/api/workspaces/*` | `agent-workspaces` volume |
| System | `envoy doctor/stats` | `/api/system/*` | Live queries |

**Same pattern everywhere**: CLI wraps HTTP, daemon owns state, volumes persist data.

---

## Current State Analysis

### Problems with Host-Mounted Credentials

```
Current Flow:
                         HOST MACHINE
                    ┌─────────────────────┐
                    │  ~/.claude/         │
                    │  └─ credentials.json│
                    └─────────┬───────────┘
                              │ bind mount (RO)
              ┌───────────────┼───────────────┐
              │               │               │
              v               v               v
         ┌─────────┐    ┌─────────┐    ┌─────────┐
         │  Envoy  │    │ Sleeve1 │    │ Sleeve2 │
         └─────────┘    └─────────┘    └─────────┘
```

**Issues:**
1. **Platform coupling**: Requires host to have credentials pre-configured
2. **Installation friction**: User must authenticate on host before starting
3. **Cloud deployment blocked**: VMs/cloud containers don't have host credentials
4. **Mixed credential sources**: Some env vars, some files, some mounts
5. **Single CLI focus**: Current paths are Claude-specific (`/home/claude/`)

### Current Container User

All containers use `claude` user (UID 1000) with home at `/home/claude`. This is:
- Tool-specific naming (confusing for multi-CLI support)
- Hardcoded across Dockerfiles, entrypoints, and Go code

---

## Proposed Architecture

### Envoy as Auth Source

```
New Flow:
                         ENVOY CONTAINER
                    ┌─────────────────────────────┐
                    │  User authenticates via:    │
                    │  - CLI: `envoy auth claude` │
                    │  - WebUI: /auth page        │
                    │                             │
                    │  Credentials stored in:     │
                    │  /home/agent/.creds/        │
                    │  (named volume)             │
                    └─────────────┬───────────────┘
                                  │
                    Named Volume: agent-creds
                                  │
              ┌───────────────────┼───────────────────┐
              │                   │                   │
              v                   v                   v
         ┌─────────┐        ┌─────────┐        ┌─────────┐
         │  Envoy  │        │ Sleeve1 │        │ Sleeve2 │
         │  (RW)   │        │  (RO)   │        │  (RO)   │
         └─────────┘        └─────────┘        └─────────┘
```

### Key Changes

| Aspect | Current | Proposed |
|--------|---------|----------|
| User name | `claude` | `agent` |
| Home directory | `/home/claude` | `/home/agent` |
| Credential source | Host bind mount | Named volume |
| Auth location | Host machine | Envoy container |
| Write access | Host only | Envoy only |
| Sleeve access | Read-only bind | Read-only volume |

---

## Credential Storage Layout

### Named Volume: `agent-creds`

```
/home/agent/.creds/                    # Volume mount point
├── claude/                            # Claude Code credentials
│   ├── credentials.json               # OAuth tokens
│   ├── settings.json                  # User settings
│   └── plugins/                       # Claude plugins
├── gemini/                            # Gemini CLI credentials
│   └── credentials.json               # Google OAuth tokens
├── codex/                             # Codex CLI credentials
│   └── credentials.json               # OpenAI tokens
└── .auth-state.json                   # Combined auth status (envoy-managed)
```

### Auth State File

```json
{
  "version": 1,
  "updated_at": "2025-01-28T12:00:00Z",
  "providers": {
    "claude": {
      "authenticated": true,
      "expires_at": "2026-01-28T12:00:00Z",
      "type": "oauth",
      "scopes": ["user:inference", "user:profile"]
    },
    "gemini": {
      "authenticated": true,
      "expires_at": null,
      "type": "api_key"
    },
    "codex": {
      "authenticated": false,
      "expires_at": null,
      "type": "api_key"
    }
  }
}
```

---

## Symlink Strategy for AI CLI Compatibility

AI CLIs expect credentials in specific paths. We use symlinks to bridge:

### In Container (via entrypoint)

```bash
# Claude Code expects: ~/.claude/
ln -sf /home/agent/.creds/claude /home/agent/.claude

# Gemini CLI expects: ~/.config/gemini/
mkdir -p /home/agent/.config
ln -sf /home/agent/.creds/gemini /home/agent/.config/gemini

# Codex expects: ~/.codex/ or env var
ln -sf /home/agent/.creds/codex /home/agent/.codex
```

### Benefits of Symlink Approach

1. **Single volume mount**: One volume serves all CLIs
2. **Native CLI compatibility**: Each CLI sees expected paths
3. **Centralized management**: Envoy manages one directory structure
4. **Easy extension**: Add new CLI by adding subdirectory + symlink

---

## UX Flow Options

### Option A: CLI-First (Recommended)

User authenticates each provider via Envoy CLI commands:

```
┌─────────────────────────────────────────────────────────────────┐
│ $ docker run -it protectorate/envoy                             │
│                                                                 │
│ Protectorate Envoy v0.1.0                                       │
│ No AI providers authenticated. Run 'envoy auth' to configure.   │
│                                                                 │
│ $ envoy auth                                                    │
│                                                                 │
│ Authentication Status:                                          │
│   Claude Code  [ ] Not authenticated                            │
│   Gemini CLI   [ ] Not authenticated                            │
│   Codex        [ ] Not authenticated                            │
│                                                                 │
│ Which provider would you like to authenticate?                  │
│   1) Claude Code (recommended)                                  │
│   2) Gemini CLI                                                 │
│   3) Codex                                                      │
│   4) All providers                                              │
│                                                                 │
│ > 1                                                             │
│                                                                 │
│ Authenticating Claude Code...                                   │
│ Opening browser: https://claude.ai/oauth/...                    │
│ (If browser doesn't open, visit this URL manually)              │
│                                                                 │
│ Waiting for authentication... Done!                             │
│                                                                 │
│ [x] Claude Code authenticated (expires: 2026-01-28)             │
│     Subscription: Max                                           │
│     Rate limit: default_claude_max_20x                          │
│                                                                 │
│ $ envoy status                                                  │
│ Envoy: Running                                                  │
│ Auth:  Claude Code [authenticated], Gemini [not configured]     │
│ Sleeves: 0 running, 12 available                                │
└─────────────────────────────────────────────────────────────────┘
```

**Commands:**
- `envoy auth` - Interactive auth wizard
- `envoy auth claude` - Authenticate specific provider
- `envoy auth status` - Show auth status
- `envoy auth revoke <provider>` - Remove credentials

**Pros:**
- Familiar CLI workflow for developers
- Non-interactive mode for automation: `envoy auth claude --token <token>`
- Works in headless environments with token passthrough
- Clear feedback and status

**Cons:**
- Requires terminal access
- OAuth flows need browser (or token paste fallback)

---

### Option B: WebUI-First

User authenticates via browser at `http://localhost:7470/auth`:

```
┌─────────────────────────────────────────────────────────────────┐
│ PROTECTORATE                              [Status] [Auth] [Logs]│
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  Authentication                                                 │
│  ─────────────────────────────────────────────────────────────  │
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │  CLAUDE CODE                              [Authenticate] │   │
│  │  Status: Not authenticated                               │   │
│  │  Required for: Sleeve spawning with Claude               │   │
│  └─────────────────────────────────────────────────────────┘   │
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │  GEMINI CLI                               [Authenticate] │   │
│  │  Status: Not authenticated                               │   │
│  │  Required for: Sleeves with Gemini                       │   │
│  └─────────────────────────────────────────────────────────┘   │
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │  CODEX                                    [Authenticate] │   │
│  │  Status: Not authenticated                               │   │
│  │  Required for: Sleeves with Codex                        │   │
│  └─────────────────────────────────────────────────────────┘   │
│                                                                 │
│  ─────────────────────────────────────────────────────────────  │
│  Tip: You can also authenticate via CLI: `envoy auth`          │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**Flow:**
1. User clicks "Authenticate" for provider
2. Popup/redirect to OAuth flow
3. Callback to `/auth/callback/<provider>`
4. Credentials stored, UI updates

**Pros:**
- Visual feedback
- Works when running in cloud/VM with port forwarding
- Easier for non-CLI users

**Cons:**
- Requires port exposure
- OAuth callback handling is complex
- More code to maintain

---

### Option C: Hybrid (Recommended Implementation)

Support both CLI and WebUI:

```
┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│  CLI Mode (headless, scripts, advanced users):                  │
│  $ envoy auth claude                                            │
│  $ envoy auth claude --token sk-ant-oat01-xxx                   │
│                                                                 │
│  WebUI Mode (visual, cloud, casual users):                      │
│  Open http://localhost:7470/auth                                │
│  Click provider -> OAuth flow -> Done                           │
│                                                                 │
│  Both write to same named volume: agent-creds                   │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**Recommendation**: Implement CLI-first (Option A) as primary, add WebUI later.

---

## Implementation Details

### 1. User Rename: `claude` -> `agent`

**Files to modify:**

```
containers/base/Dockerfile
  - Change: useradd -m -s /bin/bash -u 1000 claude
  - To:     useradd -m -s /bin/bash -u 1000 agent

containers/envoy/entrypoint.sh
  - Change all /home/claude references to /home/agent
  - Change: su - claude
  - To:     su - agent

containers/sleeve/entrypoint.sh
  - Same changes as envoy

internal/envoy/sleeve_manager.go
  - Update mount targets from /home/claude to /home/agent

internal/envoy/handlers.go
  - Update credential path checks

internal/sidecar/auth.go
  - Update home directory expectations
```

**Migration note**: This is a breaking change. Users must rebuild images.

---

### 2. Named Volume Setup

**Create volume on first run (entrypoint.sh):**

```bash
# In envoy entrypoint
docker volume inspect agent-creds >/dev/null 2>&1 || \
    docker volume create agent-creds
```

**Mount in docker-compose.yaml:**

```yaml
services:
  envoy:
    volumes:
      - agent-creds:/home/agent/.creds:rw  # Envoy can write
      - /var/run/docker.sock:/var/run/docker.sock

volumes:
  agent-creds:
    name: agent-creds
```

**Mount for sleeves (SleeveManager.go):**

```go
mounts = append(mounts, mount.Mount{
    Type:     mount.TypeVolume,
    Source:   "agent-creds",
    Target:   "/home/agent/.creds",
    ReadOnly: true,  // Sleeves only read
})
```

---

### 3. Auth Command Implementation

Following CLI-HTTP ethos: CLI is a thin wrapper, daemon does the work.

**CLI side: `cmd/envoy/auth.go`**

```go
package main

// CLI is thin - just calls HTTP endpoints
func runAuthStatus(cmd *cobra.Command, args []string) {
    // GET /api/auth/status
    resp, err := http.Get(envoyURL + "/api/auth/status")
    // ... format and print response
}

func runAuthLogin(cmd *cobra.Command, args []string) {
    provider := args[0]  // "claude", "gemini", "codex"

    // POST /api/auth/{provider}/login
    // Returns OAuth URL or prompts for token
    resp, err := http.Post(envoyURL + "/api/auth/" + provider + "/login", ...)

    // Handle interactive flow (open browser, wait for callback, etc.)
}

func runAuthRevoke(cmd *cobra.Command, args []string) {
    provider := args[0]

    // DELETE /api/auth/{provider}
    req, _ := http.NewRequest("DELETE", envoyURL + "/api/auth/" + provider, nil)
    // ...
}
```

**Daemon side: `internal/envoy/auth_handlers.go`**

```go
package envoy

// POST /api/auth/{provider}/login
func (s *Server) handleAuthLogin(w http.ResponseWriter, r *http.Request) {
    provider := chi.URLParam(r, "provider")

    switch provider {
    case "claude":
        s.authClaude(w, r)
    case "gemini":
        s.authGemini(w, r)
    case "codex":
        s.authCodex(w, r)
    }
}

func (s *Server) authClaude(w http.ResponseWriter, r *http.Request) {
    credsDir := "/home/agent/.creds/claude"
    os.MkdirAll(credsDir, 0700)

    // Option 1: Accept token directly (non-interactive)
    if token := r.FormValue("token"); token != "" {
        // Write token to credentials file
        s.writeClaudeToken(credsDir, token)
        s.updateAuthState("claude", true)
        json.NewEncoder(w).Encode(map[string]string{"status": "authenticated"})
        return
    }

    // Option 2: Return OAuth URL for interactive flow
    oauthURL := s.getClaudeOAuthURL()
    json.NewEncoder(w).Encode(map[string]string{
        "status": "pending",
        "oauth_url": oauthURL,
        "callback": "/api/auth/claude/callback",
    })
}

// GET /api/auth/status
func (s *Server) handleAuthStatus(w http.ResponseWriter, r *http.Request) {
    state := s.loadAuthState()
    json.NewEncoder(w).Encode(state)
}
```

**HTTP Endpoints:**

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/api/auth/status` | GET | Get auth status for all providers |
| `/api/auth/{provider}/login` | POST | Initiate login flow |
| `/api/auth/{provider}/callback` | GET | OAuth callback handler |
| `/api/auth/{provider}` | DELETE | Revoke credentials |

---

### 4. Symlink Setup in Entrypoint

**containers/base/entrypoint-common.sh:**

```bash
#!/bin/bash
# Common entrypoint logic for envoy and sleeves

setup_cred_symlinks() {
    local home="/home/agent"
    local creds="/home/agent/.creds"

    # Claude Code: ~/.claude -> ~/.creds/claude
    if [ -d "$creds/claude" ]; then
        ln -sfn "$creds/claude" "$home/.claude"
    fi

    # Gemini CLI: ~/.config/gemini -> ~/.creds/gemini
    if [ -d "$creds/gemini" ]; then
        mkdir -p "$home/.config"
        ln -sfn "$creds/gemini" "$home/.config/gemini"
    fi

    # Codex: ~/.codex -> ~/.creds/codex
    if [ -d "$creds/codex" ]; then
        ln -sfn "$creds/codex" "$home/.codex"
    fi
}

setup_cred_symlinks
```

---

### 5. Config Changes

**configs/envoy.yaml:**

```yaml
# Remove host credential paths (deprecated)
# docker:
#   credentials_host_path: ...   # REMOVED
#   settings_host_path: ...      # REMOVED

# Add volume configuration
docker:
  network: raven
  credentials_volume: agent-creds
  workspace_root: /home/agent/workspaces
  sleeve_image: protectorate/sleeve:latest

# Auth configuration
auth:
  # Supported providers
  providers:
    - claude
    - gemini
    - codex
  # Default provider for new sleeves
  default_provider: claude
```

---

## Migration Path

### Phase 1: User Rename (Breaking Change)

1. Update all Dockerfiles: `claude` -> `agent`
2. Update all Go code paths
3. Update entrypoints
4. Document breaking change in CHANGELOG
5. Bump major version

### Phase 2: Named Volume (Non-Breaking)

1. Add volume support alongside existing bind mounts
2. Deprecation warning for host credential paths
3. Auto-migrate: if host credentials exist, copy to volume

### Phase 3: Auth Commands (Additive)

1. Add `envoy auth` subcommand
2. Add auth status to webui
3. Add auth health checks

### Phase 4: Remove Host Dependencies (Breaking)

1. Remove bind mount credential support
2. Remove deprecated config options
3. Update documentation

---

## Pros and Cons

### Pros

1. **Platform agnostic**: No host dependencies, runs anywhere Docker runs
2. **Self-contained**: Single `docker run` to start everything
3. **Multi-CLI support**: One volume for all providers
4. **Security**: Clear permission model (envoy writes, sleeves read)
5. **Debuggable**: Named volumes are inspectable (`docker volume inspect`)
6. **Cloud-ready**: Works in VM, cloud container services
7. **Consistent naming**: `agent` user is provider-agnostic

### Cons

1. **Breaking change**: User rename requires full rebuild
2. **Volume lifecycle**: Must manage volume separately from containers
3. **OAuth complexity**: Envoy must handle OAuth callbacks
4. **Symlink maintenance**: Must keep symlinks in sync with CLI expectations
5. **Initial setup**: User must authenticate inside container (can't just mount host creds)

---

## Platform Agnosticism Assessment

### Is Keeping Everything in Envoy a Good Idea?

**YES** - with caveats:

**Strengths:**
- Single deployment unit (one docker-compose or one `docker run`)
- No host configuration required
- Portable across environments
- Clear data boundaries (workspace volume, credential volume)

**Caveats:**
- **Volume persistence**: Named volumes persist independently. Users must manage:
  - Backup: `docker run --rm -v agent-creds:/data alpine tar czf - /data > backup.tar.gz`
  - Restore: `docker run --rm -v agent-creds:/data alpine tar xzf - < backup.tar.gz`
- **Multi-host**: Named volumes are node-local. For multi-host, need:
  - Volume driver plugins (NFS, EFS, etc.)
  - Or: re-authenticate on each host
- **Credential renewal**: If OAuth tokens expire, user must re-auth in envoy

**Recommendation**: The envoy-centric approach is correct for the target use case (single machine with 12+ sleeves). For multi-host clusters, document the volume driver approach but don't implement initially.

---

## Workspace Isolation

### Design: Envoy Owns Workspaces

Workspaces live entirely within the Envoy container via named volume. **No host filesystem access for workspaces.**

```
                           ENVOY CONTAINER
┌─────────────────────────────────────────────────────────────────────────┐
│                                                                         │
│  Named Volume: agent-workspaces                                         │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │  /home/agent/workspaces/                                        │   │
│  │  ├── project-alpha/        (git repo, cloned by envoy)          │   │
│  │  ├── project-beta/         (git repo, cloned by envoy)          │   │
│  │  └── project-gamma/        (git repo, cloned by envoy)          │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│       │                                                                 │
│       │ envoy clone https://github.com/user/repo                       │
│       │ envoy pull project-alpha                                       │
│       │ envoy push project-alpha                                       │
│       │                                                                 │
└───────┼─────────────────────────────────────────────────────────────────┘
        │
        │ Mount subdirectory (read-write)
        │
        v
   ┌─────────────────┐
   │    Sleeve       │
   │                 │
   │ /home/agent/    │
   │   workspace/    │  <-- Single workspace mounted
   │   (project-     │
   │    alpha/)      │
   └─────────────────┘
```

### Git Operations: Inside Envoy Only

All git operations happen inside the envoy container via HTTP API. CLI wraps HTTP (consistent with ethos):

```bash
# CLI commands (thin wrappers around HTTP)
envoy clone <url> [--name <workspace>]   # POST /api/workspaces
envoy pull <workspace>                    # POST /api/workspaces/{name}/pull
envoy push <workspace>                    # POST /api/workspaces/{name}/push
envoy fetch [workspace]                   # POST /api/workspaces/{name}/fetch
envoy branches <workspace>                # GET /api/workspaces/{name}/branches
envoy checkout <workspace> <branch>       # POST /api/workspaces/{name}/checkout
envoy workspaces [--json]                 # GET /api/workspaces
```

**Why envoy handles git, not sleeves or host:**
1. **Host isolation**: No host filesystem access required
2. **Credential isolation**: Git SSH keys / tokens stay in envoy, not exposed to sleeves
3. **Conflict prevention**: Envoy can coordinate pushes across sleeves
4. **Audit trail**: All git operations logged in one place
5. **Sleeves are ephemeral**: Sleeves may die mid-push; envoy is persistent
6. **Platform agnostic**: Works in VM, cloud, anywhere Docker runs

### Sleeve Workspace Mount

When spawning a sleeve, envoy mounts a single workspace subdirectory:

```go
// SleeveManager.Spawn()
mounts = append(mounts, mount.Mount{
    Type:     mount.TypeVolume,
    Source:   "agent-workspaces",
    Target:   "/home/agent/workspace",
    ReadOnly: false,  // Sleeves can modify code
    VolumeOptions: &mount.VolumeOptions{
        Subpath: workspaceName,  // e.g., "project-alpha"
    },
})
```

**Note**: Docker volume subpath requires Docker 20.10+. Alternative approach: envoy daemon can bind-mount from its own filesystem view of the volume.

### SSH Keys for Git

Git SSH keys are stored in the credentials volume:

```
/home/agent/.creds/
├── git/
│   ├── id_ed25519           # SSH private key
│   ├── id_ed25519.pub       # SSH public key
│   └── known_hosts          # GitHub, GitLab, etc.
├── claude/
├── gemini/
└── codex/
```

Configured via:
```bash
envoy auth git                           # Generate or import SSH key
envoy auth git --import ~/.ssh/id_rsa   # Import existing key (one-time from host)
```

---

## Host Isolation Model

### What Envoy DOES Access on Host

| Resource | Access | Purpose |
|----------|--------|---------|
| `/var/run/docker.sock` | Read-Write | Spawn/manage sleeves |
| `/proc` (optional) | Read-Only | Hardware stats (CPU, memory) for status |
| Network ports (7470) | Expose | WebUI and API access |

### What Envoy Does NOT Access on Host

| Resource | Status | Alternative |
|----------|--------|-------------|
| `~/.claude/` | REMOVED | Use `agent-creds` volume |
| `~/.config/` | REMOVED | Use `agent-creds` volume |
| `~/workspaces/` | REMOVED | Use `agent-workspaces` volume |
| `~/.ssh/` | REMOVED | Import keys via `envoy auth git --import` |
| `~/.gitconfig` | REMOVED | Configure git inside envoy |

### Volume Summary

```yaml
# docker-compose.yaml
services:
  envoy:
    volumes:
      # Required: Docker control
      - /var/run/docker.sock:/var/run/docker.sock

      # Required: Persistent data (named volumes)
      - agent-creds:/home/agent/.creds:rw
      - agent-workspaces:/home/agent/workspaces:rw

      # Optional: Hardware stats
      - /proc:/host/proc:ro

volumes:
  agent-creds:
    name: agent-creds
  agent-workspaces:
    name: agent-workspaces
```

### Platform Portability

This model enables:

```bash
# Local machine
docker run -d -v /var/run/docker.sock:/var/run/docker.sock \
  -v agent-creds:/home/agent/.creds \
  -v agent-workspaces:/home/agent/workspaces \
  -p 7470:7470 protectorate/envoy

# VM (same command)
# Cloud container service (same volumes, different socket handling)
# Docker-in-Docker (nested, same pattern)
```

---

## Open Questions

1. **Token refresh**: Should envoy have a background job to refresh OAuth tokens proactively?
2. **Multi-provider sleeves**: Can a sleeve use multiple CLIs? (Probably yes, with all symlinks)
3. **API key support**: For Gemini/Codex, support `GEMINI_API_KEY` env var as alternative to OAuth?
4. **Credential encryption**: Should credentials be encrypted at rest in the volume?
5. **Audit logging**: Log credential access/refresh events?

---

## Appendix: AI CLI Credential Paths

| CLI | Default Path | Env Override | Notes |
|-----|--------------|--------------|-------|
| Claude Code | `~/.claude/` | `CLAUDE_CONFIG_DIR` | OAuth, long-lived tokens |
| Gemini CLI | `~/.config/gemini/` | `GEMINI_CONFIG_DIR`? | Google OAuth |
| Codex | `~/.codex/` | `OPENAI_API_KEY` | API key or OAuth |
| Aider | `~/.aider/` | Various env vars | Multiple provider support |
| OpenCode | `~/.opencode/` | Unknown | TBD |

---

## Appendix: Docker Command Examples

### Start Envoy (First Time)

```bash
docker run -d \
  --name protectorate-envoy \
  -v /var/run/docker.sock:/var/run/docker.sock \
  -v agent-creds:/home/agent/.creds \
  -v agent-workspaces:/home/agent/workspaces \
  -p 7470:7470 \
  protectorate/envoy:latest
```

### Authenticate

```bash
docker exec -it protectorate-envoy envoy auth claude
```

### Check Status

```bash
docker exec protectorate-envoy envoy auth status
```

### Backup Credentials

```bash
docker run --rm \
  -v agent-creds:/creds:ro \
  -v $(pwd):/backup \
  alpine tar czf /backup/creds.tar.gz -C /creds .
```
