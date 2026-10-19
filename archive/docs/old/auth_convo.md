# Claude Code Authentication Approaches

This document explores two approaches for authenticating Claude Code in Protectorate sleeves.

---

## Approach 1: Mounted Credentials (Current)

**How it works:**
- User authenticates Claude Code on host machine once
- `~/.claude/.credentials.json` is bind-mounted into sleeves
- Currently mounted as read-only

**Credentials file structure:**
```json
{
  "claudeAiOauth": {
    "accessToken": "sk-ant-oat01-...",
    "refreshToken": "sk-ant-ort01-...",
    "expiresAt": 1769136052795,
    "scopes": ["user:inference", "user:profile", "user:sessions:claude_code"],
    "subscriptionType": "max",
    "rateLimitTier": "default_claude_max_20x"
  }
}
```

**Token lifecycle:**
- Access token: ~24 hours
- Refresh token: Used to get new access token when expired
- Refresh requires WRITE access to credentials file

**Pros:**
- Simple setup (just run `claude` on host once)
- No additional tokens to manage
- Credentials stay in one place (host)
- Easy to revoke (just log out on host)

**Cons:**
- Read-only mount breaks token refresh
- Read-write mount means containers can modify host credentials
- Multiple sleeves writing same file = potential race conditions
- Token expiry mid-task causes failures

---

## Approach 2: Long-Lived Token (`setup-token`)

**How it works:**
- Run `claude setup-token` to generate 1-year OAuth token
- Pass token via `CLAUDE_CODE_OAUTH_TOKEN` environment variable
- No credentials file needed in container

**Setup:**
```bash
# On host, one time
claude setup-token
# Follow OAuth flow in browser
# Receive: sk-ant-oat01-xxxx...

# Add to .env
CLAUDE_CODE_OAUTH_TOKEN=sk-ant-oat01-xxxx...
```

**Container requirement:**
Sleeves need `~/.claude.json` with onboarding flag to skip interactive setup:
```json
{
  "hasCompletedOnboarding": true
}
```

**Token lifecycle:**
- Token valid for 1 year
- No refresh mechanism needed
- Regenerate annually via `claude setup-token`

**Pros:**
- No file mounts needed for auth
- No token refresh complications
- Each sleeve gets same token via env var (no race conditions)
- Works well in CI/CD, containers, headless environments

**Cons:**
- Additional setup step (`claude setup-token`)
- Token stored in .env file (secrets management)
- Must regenerate every year
- Known bug: still prompts for setup without onboarding flag workaround

---

## Approach 1b: Mounted Credentials (Read-Write)

**Variation:** Mount credentials as read-write instead of read-only.

**Change required:**
```go
// sleeve_manager.go - change ReadOnly from true to false
mounts = append(mounts, mount.Mount{
    Type:     mount.TypeBind,
    Source:   m.cfg.Docker.CredentialsHostPath,
    Target:   "/home/claude/.claude/.credentials.json",
    ReadOnly: false,  // was: true
})
```

**What this enables:**
- Claude Code can refresh tokens when they expire
- Seamless authentication without manual intervention
- Long-running sleeves don't fail due to token expiry

**Concerns:**

1. **Race conditions**: Multiple sleeves refreshing simultaneously
   - Sleeve A reads token, starts refresh
   - Sleeve B reads token, starts refresh
   - Sleeve A writes new token
   - Sleeve B writes different new token (overwrites A's)
   - Sleeve A's token is now invalid
   - Mitigation: File locking? Single-sleeve-at-a-time refresh?

2. **Security**: Containers can modify host credentials
   - Malicious code in container could corrupt/steal credentials
   - Compromised sleeve affects host authentication
   - Mitigation: Trust boundary already crossed (sleeves run user code)

3. **Debugging**: Harder to know which sleeve modified credentials
   - No audit trail of token refreshes
   - Mitigation: Could be acceptable for personal use

---

## Comparison Matrix

| Factor | Mounted RO | Mounted RW | Long-Lived Token |
|--------|-----------|-----------|------------------|
| Initial setup | Low (just login) | Low (just login) | Medium (setup-token) |
| Token lifetime | ~24h (refresh breaks) | ~24h (auto-refresh) | 1 year |
| Multi-sleeve safe | Yes | Race condition risk | Yes |
| Long-running tasks | Fails on expiry | Works | Works |
| Security | Best | Medium | Good |
| Maintenance | High (frequent failures) | Low | Low (annual renewal) |
| Known issues | Token expiry | Race conditions | Onboarding bug |

---

## Target Use Case: 12 Concurrent Sleeves

Protectorate targets ~12 concurrent sleeves per machine. This changes the calculus:

**Race condition probability with 12 sleeves:**
- Token refresh window: ~5 minutes before expiry
- 12 sleeves polling/working concurrently
- If token expires during active work, multiple sleeves hit refresh simultaneously
- Near-certain collision over a day of operation

**Conclusion:** Read-write mount is NOT viable for 12+ concurrent sleeves.

**Recommended approach:** Long-lived token (`setup-token`) as primary auth method.
- Single token shared via env var
- No file writes, no race conditions
- 1-year lifetime covers most use cases
- Setup once during install, forget for a year

---

## Decision: Support Both, Prefer Long-Lived

Given 12+ concurrent sleeves target:

1. **Primary (recommended):** Long-lived token via `claude setup-token`
   - Set during Protectorate install
   - Passed to sleeves via `CLAUDE_CODE_OAUTH_TOKEN` env var
   - No race conditions, works at scale

2. **Fallback:** Mounted credentials (read-only)
   - For users who can't/won't run setup-token
   - Works for short tasks (<24h) with few sleeves
   - Degrades gracefully (auth errors, user re-logs in)

**Install flow should:**
- Guide user through `claude setup-token`
- Store token in `.env`
- Validate token before proceeding
- Warn if falling back to mounted credentials

---

## Open Questions

- Should we detect token expiry and warn user proactively?
- Can Envoy health-check token validity on startup?
- Should we support ANTHROPIC_API_KEY as third option (pay-per-use)?
