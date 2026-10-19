# Protectorate Authentication Setup

This document describes how Protectorate handles authentication for AI CLI tools, inspired by [Molt Bot's](https://docs.molt.bot) credential management patterns.

---

## Design Goals

1. **Subscription-first**: Prioritize OAuth/subscription tokens over API keys
2. **Centralized storage**: Single credential store shared by envoy and all sleeves
3. **Token monitoring**: Detect expiration before it causes failures
4. **Future extensibility**: Support for refresh tokens, multiple profiles, new providers

---

## Supported Providers

| Provider | Auth Method | Refresh Support | Token Lifetime |
|----------|-------------|-----------------|----------------|
| Claude (Anthropic) | OAuth setup-token | No | **1 year** |
| Gemini (Google) | API key or OAuth | Yes (OAuth) | API key: unlimited |
| Codex (OpenAI) | OAuth PKCE | Yes | Hours (auto-refresh) |
| Git | SSH keys | N/A | Unlimited |

---

## Credential Storage Architecture

All credentials live in the `agent-creds` Docker volume, mounted at `/home/agent/.creds/`:

```
/home/agent/.creds/
+-- claude/
|   +-- credentials.json      # OAuth access token
|   +-- settings.json         # Claude Code settings (optional)
|
+-- gemini/
|   +-- credentials.json      # API key or OAuth tokens
|
+-- codex/
|   +-- auth.json             # OAuth tokens with refresh
|
+-- git/
|   +-- id_ed25519            # SSH private key
|   +-- id_ed25519.pub        # SSH public key
|   +-- known_hosts           # Trusted host keys
|
+-- .auth-state.json          # Expiration tracking (future)
```

**Symlinks in containers** ensure CLI tools find credentials in expected locations:
```
~/.claude      -> ~/.creds/claude
~/.config/gemini -> ~/.creds/gemini
~/.codex       -> ~/.creds/codex
~/.ssh         -> ~/.creds/git
```

---

## Authentication Flows

### Claude (Subscription OAuth)

Claude credentials are synced from your host machine's Claude CLI installation.

**Step 1: Authenticate Claude CLI on host**
```bash
# On your local machine (not in container)
claude auth login
# Complete browser OAuth flow
```

**Step 2: Sync credentials to Protectorate**
```bash
# Copies ~/.claude/.credentials.json and ~/.claude.json to shared volume
docker exec envoy envoy auth sync
```

**Step 3: Verify**
```bash
docker exec envoy envoy auth status
# Should show: claude: authenticated (oauth)
```

**What gets synced:**
- `~/.claude/.credentials.json` - OAuth access/refresh tokens
- `~/.claude.json` - Settings including `hasCompletedOnboarding` flag

**Sleeve inheritance:** When sleeves spawn, the entrypoint copies credentials from the shared volume. Sleeves inherit both authentication AND settings, so they skip onboarding prompts.

**Important:** After syncing credentials, you must rebuild sleeves or respawn them to pick up the new credentials:
```bash
make build-sleeve              # Rebuild sleeve image
docker exec envoy envoy kill <sleeve-name>  # Kill old sleeve
docker exec envoy envoy spawn <workspace>   # Spawn with new credentials
```

**Token expiration**: Claude OAuth tokens are valid for **1 year**. When expired:
1. `envoy auth status` shows expired
2. Re-run `claude auth login` on host
3. Re-run `envoy auth sync`
4. Respawn sleeves to pick up new credentials

---

### Gemini (API Key - Recommended)

For Gemini, API keys are simpler and don't expire.

**Step 1: Get API key**
1. Go to [Google AI Studio](https://aistudio.google.com/app/apikey)
2. Create new API key
3. Copy the key

**Step 2: Store in Protectorate**
```bash
docker exec envoy envoy auth login gemini "<api-key>"
```

**Alternative: OAuth flow** (for Google Workspace accounts)
```bash
# Run gemini CLI to complete OAuth
docker exec -it envoy gemini
# Follow prompts, credentials auto-stored
```

---

### Codex / OpenAI (OAuth with Refresh)

Codex uses PKCE OAuth with refresh tokens for automatic renewal.

**Step 1: Authenticate**
```bash
# Interactive OAuth in container
docker exec -it envoy codex auth login
# Opens browser or provides URL for manual auth
```

**Step 2: Verify**
```bash
docker exec envoy envoy auth status
# Should show: codex: authenticated (oauth)
```

Codex tokens auto-refresh; no manual intervention needed.

---

### Git (SSH Keys)

SSH keys provide persistent authentication for git operations.

**Option A: Generate new key in Protectorate**
```bash
docker exec -it envoy ssh-keygen -t ed25519 -C "protectorate@local" -f /home/agent/.creds/git/id_ed25519
# Add public key to GitHub/GitLab
docker exec envoy cat /home/agent/.creds/git/id_ed25519.pub
```

**Option B: Copy existing key**
```bash
# From host
docker cp ~/.ssh/id_ed25519 envoy:/home/agent/.creds/git/
docker cp ~/.ssh/id_ed25519.pub envoy:/home/agent/.creds/git/
docker exec envoy chown -R agent:agent /home/agent/.creds/git
docker exec envoy chmod 600 /home/agent/.creds/git/id_ed25519
```

**Verify**
```bash
docker exec envoy envoy auth status
# Should show: git: authenticated (ssh)
```

---

## Token Monitoring

### Manual Status Check

```bash
# Check all providers
docker exec envoy envoy auth status

# JSON output for scripting
docker exec envoy envoy auth status --json
```

**Exit codes** (future implementation):
- `0`: All credentials valid
- `1`: One or more expired/missing
- `2`: One or more expiring soon (within 24h)

### Automated Monitoring (Future)

The envoy container will periodically check token validity:

```yaml
# Future: envoy.yaml configuration
auth:
  monitor:
    enabled: true
    interval: 1h
    warn_before: 24h
```

When tokens are expiring:
1. Web UI shows warning banner
2. `envoy auth status` returns exit code 2
3. Optional: webhook notification

---

## Re-Authentication Workflow

When Claude token expires (most common case):

```
+------------------+
| envoy auth status|
| shows "expired"  |
+--------+---------+
         |
         v
+------------------+
| On HOST machine: |
| claude auth login|
+--------+---------+
         |
         v
+------------------+
| envoy auth sync  |
+--------+---------+
         |
         v
+------------------+
| Respawn sleeves  |
+--------+---------+
         |
         v
    [Authenticated]
```

**Quick re-auth commands:**
```bash
# 1. Re-authenticate Claude CLI on host
claude auth login

# 2. Sync credentials to Protectorate
docker exec envoy envoy auth sync

# 3. Respawn sleeves to pick up new credentials
docker exec envoy envoy kill <sleeve-name>
docker exec envoy envoy spawn <workspace>
```

---

## Multi-Account Support (Future)

For users with multiple accounts (personal/work):

```bash
# Future: profile support
envoy auth login claude --token "<token>" --profile work
envoy auth login claude --token "<token>" --profile personal

# Set default
envoy auth profile set claude work

# Per-sleeve override
envoy spawn --workspace myrepo --auth-profile personal
```

---

## Security Considerations

1. **Volume isolation**: `agent-creds` volume is not accessible from host filesystem
2. **Read-only for sleeves**: Sleeves mount credentials read-only
3. **No tokens in env vars**: Tokens stored in files, not environment variables
4. **No tokens in compose**: docker-compose.yaml contains no secrets

**Backup credentials** (if needed):
```bash
# Export to encrypted archive
docker run --rm -v agent-creds:/creds alpine tar czf - /creds | \
  gpg --encrypt -r your@email.com > creds-backup.tar.gz.gpg
```

---

## Web UI Authentication (Future)

The envoy web UI will provide guided authentication:

1. **Doctor tab**: Shows auth status for each provider
2. **Auth page**: Step-by-step setup wizard
3. **Token paste**: Secure input field for tokens
4. **QR code**: For mobile-assisted re-auth (scan to open claude setup-token)

---

## CLI Reference

```bash
# Show all auth status
envoy auth
envoy auth status

# Login to provider
envoy auth login <provider> <token>
envoy auth login claude "sk-ant-..."
envoy auth login gemini "AIza..."

# Revoke credentials
envoy auth revoke <provider>
envoy auth revoke claude

# Future: Check with exit codes
envoy auth check
# Exit 0 = all valid, 1 = expired, 2 = expiring soon
```

---

## HTTP API Reference

```
GET  /api/auth/status              # All provider status
GET  /api/auth/<provider>          # Single provider status
POST /api/auth/<provider>/login    # Store credentials
     Body: {"token": "..."}
DELETE /api/auth/<provider>        # Revoke credentials
```

---

## Troubleshooting

### "Claude: not authenticated"

1. Check if credentials exist: `docker exec envoy cat /home/agent/.creds/claude/.credentials.json`
2. Re-authenticate on host: `claude auth login`
3. Re-sync: `docker exec envoy envoy auth sync`

### "Sleeves can't access credentials" or "Sleeve asks for login/setup"

This usually means the sleeve was spawned before credentials were synced, or using an old image.

1. Verify credentials in volume: `docker exec <sleeve> ls -la /home/agent/.creds/claude/`
2. Check if credentials were copied: `docker exec <sleeve> ls -la /home/agent/.claude/.credentials.json`
3. Check settings: `docker exec <sleeve> cat /home/agent/.claude.json | grep hasCompletedOnboarding`

**Fix:** Rebuild and respawn:
```bash
make build-sleeve
docker exec envoy envoy kill <sleeve-name>
docker exec envoy envoy spawn <workspace>
```

### "Token expired during long task"

Claude tokens can expire mid-session. Current workaround:
1. Re-authenticate on host: `claude auth login`
2. Re-sync: `docker exec envoy envoy auth sync`
3. Kill and respawn the sleeve

Future: Token refresh hook that updates credentials without sleeve restart.

---

## Comparison with Molt Bot

| Feature | Molt Bot | Protectorate |
|---------|----------|--------------|
| Storage | `~/.clawdbot/agents/<id>/` | Docker volume `agent-creds` |
| Multi-profile | Yes, per-agent | Future |
| Token refresh | Yes (Codex) | Future |
| Expiration monitoring | `moltbot models status` | `envoy auth status` |
| Mobile re-auth | Termux scripts | Future (QR code) |
| API key support | Yes | Yes |
| OAuth support | Yes (PKCE) | Partial (token paste) |

---

## Roadmap

### Phase 1 (Current)
- [x] Token paste authentication
- [x] Credential storage in named volume
- [x] CLI and HTTP API for auth management
- [x] Read-only credential mount for sleeves

### Phase 2 (Next)
- [ ] Token expiration tracking in `.auth-state.json`
- [ ] `envoy auth check` with exit codes
- [ ] Web UI auth status and warnings
- [ ] Automated token validation on startup

### Phase 3 (Future)
- [ ] Full OAuth PKCE flow (no token paste)
- [ ] Automatic token refresh (Codex)
- [ ] Multi-profile support
- [ ] Webhook notifications for expiring tokens
- [ ] Mobile-assisted re-auth (QR code)
