# Spike 3: sharing provider credentials live across sleeves

Question (ARCHITECTURE.md "Credentials", PROBLEMS.md P2): log in once on the hub,
let many isolated sleeves use each provider's credential LIVE (no stale copies
when a refresh rotates the token), without sharing the whole config dir (session
history would collide because every sleeve's workspace is `/workspace`).

Ground rule kept: each CLI runs untouched and reads/writes its own credential
file in its own normal way. Only the filesystem around it is arranged.

All dynamic experiments used FAKE homes and FAKE credentials (never the real
`~/.claude`, `~/.codex`, `~/.grok`, `~/.pi`). Real files were only `stat`-ed and
their JSON key names read.

## Summary table

| CLI | Credential file | Relocate with | Write pattern | Lock | Rotating refresh token | Recommended share |
|-----|-----------------|---------------|---------------|------|------------------------|-------------------|
| Claude Code 2.1.303 | `<secdir>/.credentials.json` (`claudeAiOauth{accessToken,refreshToken,expiresAt,refreshTokenExpiresAt,scopes,subscriptionType,rateLimitTier}`), plus onboarding/account state in `<CLAUDE_CONFIG_DIR>/.claude.json` | `CLAUDE_SECURESTORAGE_CONFIG_DIR` (credentials dir only) and `CLAUDE_CONFIG_DIR` (everything else) | temp `.credentials.json.tmp.<hex>` (O_EXCL, 0600) in same dir, then `rename()` | yes: `mkdir <secdir>/.oauth_refresh.lock` (+ `.lock.owner` file, stale 60s) | yes (refresh token expiry tracked separately) | bind-mount ONE shared dir that holds only `.credentials.json` at `CLAUDE_SECURESTORAGE_CONFIG_DIR`; per-sleeve `CLAUDE_CONFIG_DIR` |
| Codex CLI 0.157.1 | `$CODEX_HOME/auth.json` (`auth_mode, OPENAI_API_KEY, tokens{id_token,access_token,refresh_token,account_id}, last_refresh`) | `CODEX_HOME` (whole dir) | in place: `open(O_WRONLY\|O_CREAT\|O_TRUNC, 0600)`, no temp, no rename | none on disk (in-process semaphore only) | yes (`refresh_token_reused` error exists) | per-sleeve `CODEX_HOME`; `auth.json` is a symlink to the shared dir's `auth.json` |
| Grok CLI 1.0.52 | `$GROK_HOME/auth.json` (map keyed by `issuer::client`, each `{key, auth_mode, refresh_token, expires_at, oidc_*, user/team ids}`) | `GROK_HOME` (whole dir) | resolves symlinks first, temp `.<pid>.<n>.tmp` (0600) next to the TARGET, `rename()` over target | yes: `flock(LOCK_EX\|LOCK_NB)` on `$GROK_HOME/auth.json.lock` (content `pid:epoch`); code re-reads disk and "adopts sibling token" | yes (OIDC refresh_token) | per-sleeve `GROK_HOME`; symlink BOTH `auth.json` and `auth.json.lock` into the shared dir |
| pi (this checkout) | `$PI_CODING_AGENT_DIR/auth.json` (`{ "<provider>": {type, key\|oauth fields} }`) | `PI_CODING_AGENT_DIR`; sessions separately via `PI_CODING_AGENT_SESSION_DIR` | in place: `writeFileSync(..., {mode:0o600})` (truncate), under lock | yes: `proper-lockfile` `mkdir <auth.json>.lock` (realpath:false, stale 30s); re-reads when file revision changes | per provider; OAuth entries refresh under the lock | bind-mount a shared dir AS `PI_CODING_AGENT_DIR`; override `PI_CODING_AGENT_SESSION_DIR` per sleeve |
| Antigravity (`agy`) | NOT VERIFIED (not installed). CodexBar docs: macOS Keychain; with `SSH_TTY` set `agy` uses a file token store (path not documented) | `GEMINI_CLI_HOME` moves `~/.gemini` (conversation DBs); `HOME` for the rest | unknown | unknown | Google OAuth; `agy` silently refreshes expired grants | undecided, see "Open items" |

Birth-time evidence on the real files (stat only): Claude and Grok `auth.json`
have birth time == mtime (file is recreated on every save = temp+rename); Codex
and pi have birth time much older than mtime (rewritten in place).

## Per CLI details

### Claude Code
- Found by extracting the strings of the native binary (`~/.local/bin/claude`,
  Bun-compiled JS) and confirmed with `strace` on a fake home
  (`observe-claude.sh`).
- Credential dir resolution: `CLAUDE_SECURESTORAGE_CONFIG_DIR` if set (empty
  string means `~/.claude`), else the normal config dir
  (`CLAUDE_CONFIG_DIR` or `~/.claude`). This is a dedicated knob that splits
  credentials from everything else. With it set, strace showed all credential
  reads/writes and the refresh lock in the secure-storage dir, and projects/
  sessions/`.claude.json`/backups in `CLAUDE_CONFIG_DIR`.
- Write: `openat(".credentials.json.tmp.<hex>", O_WRONLY|O_CREAT|O_EXCL, 0600)`,
  `renameat` over `.credentials.json`, `fchmodat 0600`. Therefore a single-file
  bind mount fails with EBUSY (proved in `run.sh`), and a symlink at
  `.credentials.json` would be replaced by a regular file on the first refresh.
  Newer code paths also open the credential file with `O_NOFOLLOW` and report
  `refused-symlink`, so symlinks are doubly wrong here. Use the directory.
- Refresh lock: `mkdir .oauth_refresh.lock` (+ `.oauth_refresh.lock.owner`
  written via temp+rename, 60s stale, 5s heartbeat via `utimes`), always inside
  the secure-storage dir, so it is automatically shared by everyone mounting that
  dir. `mkdir` is atomic on a shared bind mount (same host kernel).
  Other processes that wait then re-read `.credentials.json` and use the new token.
- Other relevant bits: `CLAUDE_CODE_OAUTH_TOKEN` (long-lived token from
  `claude setup-token`, no refresh, no file) exists as a fallback, and
  `CLAUDE_CODE_HOST_CREDS_FILE`/`CLAUDE_CODE_HOST_AUTH_ENV_VAR` exist for
  hosted use; not explored.
- Same dir as credentials if you do NOT split: `history.jsonl`, `projects/`
  (transcripts keyed by cwd, so `/workspace` collides), `sessions/`,
  `session-env/`, `file-history/`, `shell-snapshots/`, `settings.json`,
  `plugins/`, `.claude.json` (sibling file in HOME, or inside
  `CLAUDE_CONFIG_DIR`; contains `hasCompletedOnboarding`, `oauthAccount`, project
  trust state). Collides across sleeves: yes. Hence split.
- Per-sleeve `CLAUDE_CONFIG_DIR/.claude.json` must be seeded with
  `hasCompletedOnboarding: true` or the sleeve prompts for setup (this bit the
  archive, see below). Account identity (`oauthAccount`) in it is display
  metadata; whether it is required was not testable offline.
- Observation under a FAKE expired credential: the process created the lock,
  rewrote the credential file by temp+rename, removed the lock, and reported
  "OAuth session expired and could not be refreshed". Network refresh itself was
  not exercised.

### Codex
- Source: `codex-rs/login/src/auth/storage.rs`, `manager.rs`;
  confirmed with strace (`observe-codex.sh`, `codex login --with-api-key` with a
  fake key, symlinked `auth.json`).
- `CODEX_HOME` relocates the whole dir; `cli_auth_credentials_store` default is
  `file` (other modes: keyring, auto, ephemeral; make sure sleeves stay on
  `file`, i.e. do not set keyring/auto in the shared `config.toml`).
- Write is `OpenOptions truncate+write+create, mode 0600`, so a symlinked
  `auth.json` survives, the write lands in the target, and the symlink needs no
  directory mount (a bind-mounted dir still needed so containers share the
  target). strace: single `openat(O_WRONLY|O_CREAT|O_TRUNC, 0600)` through the
  link, link intact afterwards.
- Refresh: access token expiry window / `last_refresh` older than 8 days, or on
  401. Every refresh first does a "guarded reload": re-reads `auth.json`, and if
  the on-disk tokens changed (same account id) it skips refreshing and uses the
  new ones. The only mutex is in-process (`Semaphore`), no cross-process file lock.
  If two sleeves refresh at the same instant with the same refresh token one gets
  `refresh_token_reused` and recovers by reload on the next 401. Window is small
  (the file is truncated then written; a reader can in theory see an empty file).
- Same dir: `config.toml`, `history.jsonl`, `sessions/`, `state_5.sqlite`,
  `logs_*.sqlite`, `thread_history_*.sqlite`, `shell_snapshots/`, `memories/`,
  `installation_id`. Sharing the whole dir would share sqlite DBs and history:
  collides. Do not share the dir.

### Grok
- Strings of the binary + embedded docs, and strace on a fake `GROK_HOME`
  (`observe-grok-write.sh`, `observe-grok-refresh.sh`, using the documented
  `GROK_AUTH_PROVIDER_COMMAND` hook with a fake provider so `grok login` and the
  refresh path run offline).
- Write: `readlinkat` on `auth.json` (resolves the symlink), temp
  `.<pid>.<n>.tmp` created `O_EXCL, 0600` in the directory of the TARGET,
  `fchmod 0600`, `rename` over the target. Symlink in `GROK_HOME` stays intact; the
  shared dir must be writable by the sleeve (temp file created there).
- Lock: `$GROK_HOME/auth.json.lock`, opened `O_RDWR|O_CREAT` (no `O_NOFOLLOW`),
  `flock(LOCK_EX|LOCK_NB)`, 17 bytes `pid:epoch`. Created next to the SYMLINK, not
  the target, so with only `auth.json` symlinked each sleeve has its own lock and
  refreshes are not serialised across sleeves. Pre-creating `auth.json.lock` as a
  symlink into the shared dir makes the flock land on the shared inode (verified:
  the lock content appeared in the shared dir). Strings show cross-process care
  ("refresh used disk token", "adopted sibling token after lock-loss
  revalidation", "proceeding without file lock").
- Other: `XAI_API_KEY` env fallback; hot-reloads `auth.json` when it changes
  ("Grok picks up changes ... without a restart"); `GROK_AUTH_PROVIDER_COMMAND`
  (docs: command prints a token JSON, grok re-runs it to refresh) is an official
  extension point and could let a sleeve fetch tokens from the hub with no shared
  files at all, at the cost of not using grok's own OIDC refresh.
- Same dir: `sessions/`, `config.toml`, `worktrees.db`, `memory-v2`, `secrets/`
  (0700), `trusted_folders.toml`, `agent_id`, `mcp_credentials.json`. Collides
  (sessions/db); do not share the dir. `GROK_HOME` as a symlink is refused under
  sandbox write-deny, so use a real per-sleeve dir.
- Note: unlike the others, `auth.json` here holds profile PII (email, names, ids)
  next to the tokens.

### pi
- Source `packages/coding-agent/src/core/auth-storage.ts`, `config.ts`.
- `PI_CODING_AGENT_DIR` (name derives from app name) relocates the agent dir;
  sessions separately via `PI_CODING_AGENT_SESSION_DIR`.
- All access goes through `withLock`/`withLockAsync`: `proper-lockfile` creating
  directory `auth.json.lock` next to `authPath` (`realpath:false`, stale 30s,
  `update` heartbeat), read-modify-`writeFileSync` (truncating, in place, 0600).
  File revision (`getFileRevision`) is checked so other writers' changes are
  picked up.
- A symlink is NOT enough for pi: the lock directory is created beside the link
  (sleeve-local), so refreshes would not be serialised, and a symlink to a shared
  `auth.json.lock` dir does not work with mkdir semantics. So share the dir.
  What else lives there: `settings.json`, `models.json`, `extensions/`,
  `sessions/` (moved by the env var), `*.bak-*`. Settings/models/extensions are
  config the user likely wants identical on all sleeves; sessions are moved out
  by `PI_CODING_AGENT_SESSION_DIR`. Needs only that the sleeves tolerate
  concurrent edits of `settings.json` (pi's own behaviour, not auth).
- Keys in `auth.json` are keyed by provider and may be plain API keys (static,
  no refresh at all) or OAuth entries.

### Antigravity (not installed; research only)
From `references/CodexBar/docs/antigravity.md`: login is Google OAuth; `agy`
1.2.x; on macOS credentials live in the Keychain; when `SSH_TTY` is set `agy` uses
a file-token storage (path unspecified); `agy` silently refreshes expired grants;
history/conversations under `~/.gemini/antigravity*/` with `GEMINI_CLI_HOME`
replacing `~/.gemini`; old Gemini CLI stored `~/.gemini/oauth_creds.json`. Google
refresh tokens normally do not rotate on use, so the stale-copy risk is lower
(access token only), but the file location and write pattern need to be measured
once `agy` is installed: run `strace -f -e trace=file` with a fake `HOME`, `SSH_TTY=x`,
look for the token file, apply the same decision rule (below).

## Decision rule used

1. If the CLI has a credentials-only relocation knob (Claude): mount a shared
   directory there. Nothing else to maintain.
2. Else if it writes in place (Codex): a symlink `auth.json` -> shared dir is
   enough; the symlink survives.
3. Else if it writes by temp+rename but resolves symlinks and locks with a
   separate sibling file (Grok): symlink `auth.json` AND the lock file into the
   shared dir.
4. Else if the lock is a directory beside the file (pi) and the config dir is
   relocatable in pieces: share the dir, relocate the high-collision parts.
5. Never bind-mount a single file (EBUSY on rename), never copy.

## Proven scheme (docker)

```
host: ~/.protectorate/creds/            (hub-owned; mode 0700)
        claude/.credentials.json        mounted rw at  /creds (whole tree) in each sleeve
        codex/auth.json
        grok/auth.json  grok/auth.json.lock
        pi/auth.json

sleeve env:
  CLAUDE_SECURESTORAGE_CONFIG_DIR=/creds/claude   CLAUDE_CONFIG_DIR=/home/agent/.claude (per-sleeve)
  CODEX_HOME=/home/agent/.codex   with auth.json -> /creds/codex/auth.json
  GROK_HOME=/home/agent/.grok     with auth.json -> /creds/grok/auth.json
                                       auth.json.lock -> /creds/grok/auth.json.lock
  PI_CODING_AGENT_DIR=/creds/pi   PI_CODING_AGENT_SESSION_DIR=/home/agent/.pi/sessions
```
Mount the per-provider subdir (or the whole `creds` tree) read-write as a
DIRECTORY. Each sleeve should preferably only see the providers it uses
(mount `creds/claude` only into Claude sleeves) to limit blast radius.
Symlinks are created by the sleeve entrypoint (idempotent `ln -sfn`, `-n` so an
existing directory is not descended into; archive's `ln -sf` bug).

`run.sh` starts two containers `spike-cred-a` and `spike-cred-b` (image pinned by
digest, `python:3.11-slim`), mounts one shared host dir into both, and for each
CLI has A replay that CLI's observed write pattern (`replay.py`) while B reads
through ITS OWN path. Result (all PASS on this machine, aarch64, docker 20.10):

```
claude: B sees initial/refreshed; mkdir-lock held by A visible to B
codex : B sees refreshed through its own symlink; A's symlink survived
grok  : B sees refreshed; symlink survived rename; flock held by A blocks B
pi    : B sees refreshed; lock dir held by A visible to B
neg a : single-file bind mount + rename-over -> EBUSY
neg b : a copy made at start keeps the old token (stale) while live has the new one
```
Honest limits: `replay.py` reproduces the observed syscall patterns; it is not the
real CLI (no network). The real CLIs were exercised on the host against fake
homes for: Claude (env var split + lock + temp/rename), Codex (O_TRUNC through
symlink), Grok (symlink resolve + temp/rename + flock location + shared lock via
symlinked lock file). pi was verified from source only. Cross-HOST sharing (nodes
that are separate machines) is not covered: a bind mount only shares between
containers on one kernel; flock and mkdir semantics over NFS/SSHFS are weaker.
For remote nodes use the hub as the only refresher or an ssh-mounted dir with the
same layout (see open items).

## What the archive did, and why it was fragile

Files: `archive/containers/sleeve/entrypoint.sh`, `archive/internal/envoy/auth_manager.go`,
commits `d4af9f8` (proper Claude credential sync), `d2ce210` (settings mount
regression), `ebe7f42` (auth path fix).

What it did: user runs `claude auth login` in the envoy container, then
`envoy auth sync` copies `~/.claude/.credentials.json` and `~/.claude.json` into a
shared named volume `agent-creds` (`/home/agent/.creds/claude/...`). Each sleeve's
entrypoint `cp`'s them into `~/.claude/.credentials.json` (0600, chown) and
`~/.claude.json`. Gemini/Codex/git were `ln -sf` of whole dirs
(`.creds/gemini -> ~/.config/gemini`, `.creds/codex -> ~/.codex`,
`.creds/git -> ~/.ssh`).

What worked: a single named volume as the hub-owned store; getting the exact file
names/shape right (dot-prefixed `.credentials.json` wrapper object `claudeAiOauth`,
not a plain token); seeding `.claude.json` (onboarding flag) so the sleeve does
not nag; auth status derived by stat-ing the files.

What was fragile:
- Copy at container start = stale copy. Claude rotates the refresh token on
  refresh; once any holder (hub or another sleeve) refreshes, every other copy has
  a dead refresh token and the sleeve falls out of login. Copies never flow back
  to the volume, so the hub's "synced" state is also stale after the first refresh.
- Sync was manual (`envoy auth sync`); forgetting it silently left old creds.
- Several different files had to be kept in lock-step (`.credentials.json` vs
  `.claude.json`); `d2ce210` shows the regression when the wrong settings file
  was copied (prompted for setup on boot).
- `ln -sf dir` onto an existing directory creates a nested link inside it
  (commit message of `d4af9f8`: "Symlinks failed because .claude dir already
  existed"), which is why they switched to `cp`.
- Whole-dir symlinks for Codex/Gemini share history/sessions/sqlite across all
  sleeves (collisions), and the dir contents are written concurrently.
- `chown -R` on the creds volume at every start, and credentials copied into
  per-sleeve writable layers (more places a secret lives).
- No locking story; per-sleeve independent refresh with the same refresh token is
  a race by construction.

## Open items / risks

1. Antigravity: measure on a machine with `agy` (path, write pattern, lock).
2. Claude: confirm with a real (throwaway account) refresh that two sleeves
   racing is serialised by `.oauth_refresh.lock` across containers. Locking is
   directory-based so it should hold on one host. Also decide the per-sleeve
   `.claude.json` seed (is `oauthAccount` needed for `/status`/usage display?).
3. Codex has no cross-process lock; a rare double refresh can produce
   `refresh_token_reused`. If observed in practice, add a hub-side `flock` wrapper
   around the sleeve start (not around the CLI) or a tiny hub supervisor that
   watches `codex/auth.json` and keeps a hub-owned backup.
4. File ownership/UID: sleeves write the shared files as their UID; use the same
   UID in all sleeves or a shared group with `umask 077` replaced by group-aware
   modes (CLIs force 0600 so the same UID is the simplest).
5. Remote nodes (P2): shared bind mounts do not span machines. Options: sleeves
   only on the hub; or sshfs/NFS of the creds tree (locks weaker); or Grok's
   `GROK_AUTH_PROVIDER_COMMAND` hook, and Claude's `CLAUDE_CODE_OAUTH_TOKEN`
   long-lived token, which need no shared file.
6. Blast radius: every mounted sleeve can read the full credential (and Grok
   `auth.json` contains PII). Mount only what each sleeve needs.

## Reproduce

```
cd spikes/credentials
./run.sh                                   # docker proof (needs docker + the pinned python image)
CLAUDE_BIN=$(command -v claude) ./observe-claude.sh   # fake HOME + CLAUDE_SECURESTORAGE_CONFIG_DIR, strace
CODEX_BIN=$(command -v codex)   ./observe-codex.sh    # fake CODEX_HOME, symlinked auth.json
GROK_BIN=$(command -v grok) GROK_ARGS=login ./observe-grok-write.sh     # prints "tmp: <dir>"
GROK_BIN=$(command -v grok) ./observe-grok-refresh.sh <dir>             # refresh path, lock location
LOCKLINK=1 GROK_BIN=$(command -v grok) GROK_ARGS=login ./observe-grok-write.sh   # lock symlinked into shared dir
```
`keys.py` prints JSON key names only (never values). Codex source was read from a
shallow clone of github.com/openai/codex; Claude/Grok from strings of the shipped
binaries (version-specific; re-verify on upgrades with the observe scripts).
`observe-grok.sh` is the first attempt (fake OIDC auth.json is rejected as "Not
signed in"; kept to show that grok validates the stored entry).
