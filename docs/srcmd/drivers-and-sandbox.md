# Harness Drivers & Sandboxing (`src/drivers/`, `src/sandbox/`)

## Hands-Off Harness Protocol (`src/drivers/`)

Protectorate drives upstream CLIs exclusively via their official machine-readable stdio modes without injecting synthetic hooks or patching external binaries:
- **`claude`** (`claude.zig`): `-p --verbose --output-format stream-json --input-format stream-json --include-partial-messages`
- **`codex`** (`codex.zig`): `app-server` JSON-RPC 2.0
- **`grok`** (`grok.zig`): `--acp` JSON-RPC 2.0
- **`pi`** (`pi.zig`): `--mode rpc` JSONL (`get_state`, `prompt`, `steer`, `abort`, `compact`)

Each adapter translates raw protocol lines into normalized `DriverEntry` events (`session_info`, `assistant_text`, `reasoning`, `tool_call`, `tool_result`, `approval_request`, `turn_started`, `turn_settled`) through `protocol.Out`, coalescing streaming deltas and capping block/output sizes (`max_block = 1 MiB`).

`drivers/session.zig` wraps a spawned child process with dedicated stdout NDJSON and bounded 16 KiB stderr ring threads, releasing `Session.mu` prior to delivering callbacks to eliminate lock-order inversions with `hub/runtime.zig`.

## Bubblewrap Sandboxing (`src/sandbox/`)

On Linux when `bwrap` is detected (`sandbox/exec.zig`), `sandbox/spec.zig` constructs depth-sorted bind mounts (`ro-bind / /`, explicit workspace `rw` or `ro`, unmasked git common-dir admin paths resolved via `git rev-parse --git-common-dir`), clears ambient environment variables (`--clearenv` + explicit safe `--setenv`), and masks unrelated harness credentials (`default_masks`) with `--tmpfs` / `/dev/null` while leaving only the active harness config path accessible.
