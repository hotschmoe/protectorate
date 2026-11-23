# Harness Drivers & Sandboxing (`src/drivers/`, `src/sandbox/`)

## Hands-Off Harness Protocol (`src/drivers/`)

Protectorate drives upstream CLIs exclusively via their official machine-readable stdio modes without injecting synthetic hooks or patching external binaries:
- **`claude`** (`claude.zig`): `-p --verbose --output-format stream-json --input-format stream-json --include-partial-messages`
- **`codex`** (`codex.zig`): `app-server` JSON-RPC 2.0
- **`grok`** (`grok.zig`): `--acp` JSON-RPC 2.0
- **`antigravity`** (`antigravity.zig`): `agy --input-format stream-json --output-format stream-json` (`event: init / step_update / result`, subtracting cumulative session usage counters per turn and signalling interrupts with `SIGINT`)
- **`pi`** (`pi.zig`): `--mode rpc` JSONL (`get_state`, `prompt`, `steer`, `abort`, `compact`)

Each adapter translates raw protocol lines into normalized `DriverEntry` events (`session_info`, `assistant_text`, `reasoning`, `tool_call`, `tool_result`, `approval_request`, `turn_started`, `turn_settled`) through `protocol.Out`, coalescing streaming deltas and capping block/output sizes (`max_block = 1 MiB`).

`drivers/session.zig` wraps a spawned child process with dedicated stdout NDJSON and bounded 16 KiB stderr ring threads, releasing `Session.mu` prior to delivering callbacks to eliminate lock-order inversions with `hub/runtime.zig`.

## Cross-Platform Guardrails & Sandboxing (`src/sandbox/`)

- **Per-sleeve hygiene (all OSes):** When sandboxing is enabled, `hub/runtime.zig` provisions an isolated per-sleeve temp directory (`<hub_home>/run/<sleeve_id>/tmp` exposed via `TMPDIR`/`TEMP`/`TMP`), exports `PROTECTORATE_SLEEVE`/`PROTECTORATE_WORKSPACE`, and strips nested session markers (`CLAUDECODE`, `CODEX_THREAD_ID`, etc.).
- **Process trees (`ProcessGuard` in `sandbox/exec.zig`):** Tracks POSIX process groups (`pgid = 0` + `-pid` kill) on Linux/macOS and Win32 Job Objects (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` + `TerminateJobObject` + optional `JOB_OBJECT_LIMIT_JOB_MEMORY`) on Windows for both local sleeves and remote `node/runner.zig` jobs.
- **Linux `bwrap` + Landlock `__guard` fallback:** On Linux, `sandbox/spec.zig` prefers `bwrap` (`ro-bind / /`, explicit workspace/git mounts, `--clearenv`, credential masks leaving only the active harness state dir accessible). When `bwrap` is unavailable, `hub/runtime.zig` falls back to self-re-exec via `<exe> __guard` (`sandbox/exec.zig`), applying `PR_SET_PDEATHSIG`, optional `RLIMIT_AS`, and Landlock LSM write restrictions (`handled_access_fs`) allowing writes only inside the workspace, git admin dirs, active harness state dir, sleeve temp dir, `/dev/null`, and `/dev/shm`.
