# Codex in a sleeve: per-sleeve app-server (spike, 2026-10-20)

Codex CLI 0.163.0 (aarch64), bwrap built like `internal/sleeve/plan.go`
(tmpfs root, `/home/<user>` = per-sleeve home, `.nvm` read-only, shared auth dir
bound at its host path, `CODEX_HOME=$HOME/.codex`, `--clearenv`). Scratch under
the session scratchpad `codexsleeve/`. Two model turns were spent ("ok" and a
"what was my previous message" resume); the TUI was opened twice without a turn.
The user's daemon (pids 289740/289841), `~/.codex/auth.json` (mtime unchanged)
and `~/.codex/sessions` were untouched.

## Findings

1. **A stdio `codex app-server` works inside the sleeve** with the shared auth
   symlinked into the per-sleeve `CODEX_HOME`. `initialize` reports
   `codexHome=/home/<user>/.codex` (the sleeve's). No daemon is involved; no
   socket is needed. `codex app-server --help`: `--listen stdio:// (default) |
   unix:// | unix://PATH | ws://IP:PORT | off`, `--stdio`, `--ws-auth`,
   subcommands `daemon {start,stop,restart,...}`, `proxy`, `generate-json-schema`.
   `-c key=value`, `--enable/--disable <feature>` work on every subcommand.
2. **Thread ownership is per CODEX_HOME.** The thread's rollout landed in the
   sleeve home (`sessions/2026/10/20/rollout-...jsonl`, `state_5.sqlite`); it is
   absent from `~/.codex` and `session_index.jsonl`. A second home could not
   resume it ("No saved session found"). `codex resume <id>` in the same sleeve
   found it and showed the history; headless `thread/resume` after the TUI
   exited kept the context (turn 2 quoted turn 1).
   - Default TUI inside a sleeve **starts its own managed daemon** in the
     sleeve home: it installs a copy of the codex package into
     `$CODEX_HOME/packages/app-server-daemon/` (368 MB per sleeve), runs
     `app-server --listen unix:// --managed-daemon` plus `daemon pid-update-loop`,
     and puts the socket at `$CODEX_HOME/app-server-control/app-server-control.sock`
     -> symlink into `/tmp/codex-daemon-<uid>/<hash>` (sleeve-private because
     `/tmp` is a tmpfs in the sandbox). While that TUI is open, a stdio
     app-server `thread/resume` fails: `thread ... already has an active writer`.
     The daemon children die with the sleeve (bwrap `--die-with-parent`, own
     pid ns); after the TUI exit the thread resumed headlessly at once.
   - `codex --disable daemon_auto_start resume <id>` (feature `daemon_auto_start`,
     stable, default true) starts **no daemon**: the TUI embeds its app-server,
     nothing is installed (home stays ~3 MB), and the same writer lock is held
     only while the TUI runs. Same one-owner rule, nothing lingering. Preferred.
   - So no conflict as long as the hub closes the chat-mode app-server (stdin
     EOF / kill) before opening the TUI, and ends the TUI before the next
     `thread/resume`. The lock is per live owner, not sticky as with the user's
     daemon.
3. **Token refresh writes through the symlink.** Not triggered on real
   credentials. Evidence: `codex login --with-api-key` (fake key, scratch home)
   goes through the same auth save path; with `$CODEX_HOME/auth.json` a symlink
   to a shared file, the symlink survived and the target file was rewritten in
   place (mode kept). That matches ARCHITECTURE.md ("in-place O_TRUNC write").
   Residual risk is unchanged: no cross-process lock, so two sleeves refreshing
   at once may hit `refresh_token_reused` and recover on the next 401. Codex
   refreshes only on 401 or when `last_refresh` is old; the real file was 5 days
   old during the test. Note a single-file bind of the real auth.json (as used in
   the test) also works for in-place writes; a rename-based save would fail
   EBUSY instead of diverging.
4. **Codex's own sandbox works nested in bwrap** (`codex sandbox -- ...` runs;
   `workspace-write` blocked writes outside the workspace; `read-only` blocked
   workspace writes). It is therefore not broken, but redundant: bwrap is the
   boundary and the nested layer only adds friction (network/paths). Config
   `approval_policy = "never"` + `sandbox_mode = "danger-full-access"` in the
   sleeve `config.toml` was honored by `thread/start` without params
   (`approvalPolicy: never`, `sandbox: dangerFullAccess`; TUI shows "YOLO mode").

## Recipe

Per-sleeve files (host side, under the sleeve home, `internal/sleeve/layout.go`
already creates the dir, the `auth.json` symlink and `CODEX_HOME`):

- `$CODEX_HOME/auth.json` -> shared auth dir (as today).
- `$CODEX_HOME/config.toml` (seed, only if absent):

      approval_policy = "never"
      sandbox_mode = "danger-full-access"

  Do not copy the user's config.toml (it carries project trust and model). The
  default model is chosen by Codex; set `model = "..."` here if needed.

Chat mode (driver): run inside the sleeve bwrap with the layout env, no extra env:

    codex app-server              # stdio, default; one process per session
    -> initialize {clientInfo, capabilities:{experimentalApi:true}}, notification "initialized"
    -> thread/start {cwd}  |  thread/resume {threadId, cwd}   # config.toml policy applies
    -> turn/start ...  ; settled on turn/completed

The driver then speaks the existing JSON-RPC over the process pipes instead of
the WebSocket-on-unix-socket used for the user's daemon (same messages; no WS
framing). Close = close stdin / kill the process.

TUI take-over (same sleeve, after the chat process is gone):

    codex --disable daemon_auto_start resume <thread-id>

(add `--disable daemon_auto_start` to every TUI launch in a sleeve, including
fresh ones, to avoid the 368 MB per-sleeve package install and background
daemons). Keep `cwd` identical; threads are keyed by cwd.

Caveats: one owner at a time per thread (hub enforces; Codex enforces too); the
`CODEX_HOME` of a sleeve must persist for resume to work; `/tmp` socket paths
are sleeve-private. Not tested: concurrent refresh, approvals in the sleeve
(policy `never` means none arrive), a TUI started without `--disable` plus a
second TUI.
