# REVISIT

Shortcuts taken to make something work, and what "right" (then "fast") would be. Newest last.
Each entry: where, what was done, what right looks like.

- **Release signing** (SPEC 6): no signing key; updates trust SHA256SUMS over HTTPS. Right: an
  offline-held key and a signature checked against a key embedded in the binary.
- **Outbound HTTPS through curl** (hub, nodes): no Rust TLS stack. Right: decide between
  curl forever, or a forked and trimmed rustls when curl's absence or output parsing bites.
- **Sandbox through bwrap**: right is our own small namespaces helper (SPEC 3.4).
- **Node JSON by hand** (`pt-node`): each collector has a hand-written `to_json` over a tiny
  writer (`crates/pt-node/src/json.rs`). Right: swap for pt-json's `ToJson` once it lands, and
  delete the writer.
- **Windows presence has no idle time from a plain caller** (`pt-node`, `sys/windows_presence.rs`):
  `WTSINFOEXW.LastInputTime` is 0 for the console session when queried by an ordinary admin
  (`quser` shows "none"), so `presence` reports UNKNOWN unless the screen is locked (lock flag
  works; AWAY once locked 60 s). Not yet verified from LocalSystem, which is the design's
  assumption (SPEC 3.5). Right: run as the service on officewin and compare; if still 0, have the
  service launch a console-subsystem `idle-probe` into the active session (WTSQueryUserToken +
  CreateProcessAsUser) that calls `GetLastInputInfo`. `alt_idle_secs` already reports
  `GetLastInputInfo` when the binary itself runs inside the console session. The lock flag
  (`SessionFlags` 0 = locked) was only seen unlocked; verify locked.
- **Linux evdev path untested with real input** (`pt-node`, `sys/linux_presence.rs`): hub has no
  keyboard or mouse, agent-world's AT keyboard needs root. Opening and watching works as root
  ("evdev watching 1 devices"); reading an actual keystroke was not exercised, and logind's
  Present branches run only against a stand-in `loginctl` in the e2e tests. Right: type on a real
  seat once.
- **Linux idle needs a long-lived Detector** (`pt-node`): logind gives IdleHint, and
  IdleHint=no is reported PRESENT with idle null (desktops that never set it, such as bare X or
  sway, stay PRESENT forever, the safe direction). GNOME sets IdleHint after its idle-delay and
  IdleSinceHint is the moment it was set, so idle is underestimated by that delay. `/dev/input`
  needs root or the `input` group (the system service is root). One-shot `presence` cannot use
  evdev (no history); `watch-presence` and the future service can. Right: record IdleSinceHint
  changes over time and add the X11/Wayland session-bus readers if a desktop box needs them.
- **Windows GPUs from the registry class key** (`pt-node`): ghost entries from replaced drivers are
  deduped by (vendor, device, name) keeping the newest driver; VRAM is
  `HardwareInformation.qwMemorySize` (a 4 GiB cap on old drivers). Right: DXGI/`nvidia-smi` for
  live GPU numbers when GPU sampling arrives.
- **Linux GPU names** (`pt-node`): from `pci.ids` when hwdata is installed (else the raw ids), SoC
  GPUs report only the kernel driver (`linlondp` on the CIX board), VRAM only for amdgpu and
  nvidia-smi. Right: per-vendor readers with the sample's utilisation and temperature.
- **Job priority** (`pt-node`): `lower_to_idle` on Windows sets the class on one process; children
  started later inherit it, running children are not touched. Right: set it on the job's Job
  Object (`JOBOBJECT_BASIC_LIMIT_INFORMATION.PriorityClass`) when the job runner exists. On Linux
  it walks the process tree once; a cgroup scope with `cpu.weight`/`io.weight` is the durable form.
- **Windows build path**: no Windows SDK libs on the dev hub, so there is no cross-build; both
  Windows targets build natively on officewin (VS Build Tools 14.44 incl. ARM64 libs). x64:
  `cargo build --release -p protectorate-node`; arm64: `cargo +stable build --release --target
  aarch64-pc-windows-msvc -p protectorate-node` with `%USERPROFILE%\.cargo\bin` first on PATH
  (the default toolchain lacks the arm64 std). Source goes over as a tar pushed by scp; the
  arm64 `.exe` has never been run (no ARM Windows machine). Right: a `make release` step that runs
  these over ssh, and cargo-xwin or a vendored SDK sysroot if officewin should not be required.
- **No macOS or Android code paths** in `pt-node` yet; the crate does not compile there.
- **pt-json record! keys** (crates/pt-json): JSON keys are the Rust field names (`r#type` is
  `type`), with no rename, so camelCase foreign JSON (Codex app-server) goes through `Value`
  accessors. Right: an optional `field: T as "key"` when the first driver needs typed decoding.
- **pt-json tagged_enum!** (crates/pt-json): the tag key is always `type` and variants are
  struct-like only. Right: a configurable tag and newtype variants wrapping a `record!` type,
  once an API needs them.
- **pt-json FromJson borrows** (crates/pt-json): `from_json(&Value)` clones every string.
  Fast: an owned `from_json_value(Value)` that moves strings out, if large driver payloads show
  up in profiles.
- **pt-json numbers and maps** (crates/pt-json): each number is a heap `Box<str>` and object
  lookups are linear. Fast: inline storage for short number text, and an index for large
  objects if a hot path decodes many-keyed objects by key. No pretty writer yet either.
- **Claude supervised approvals ride a hidden flag** (`pt-drivers`, claude.rs):
  `--permission-prompt-tool stdio --permission-mode manual` works with Claude Code 2.1.316
  (e2e `supervised_approval`: `control_request can_use_tool` arrives, `control_response allow`
  runs the tool), but the flag is only named in `--permission-prompts` help. `allow_session`
  sends the request's `permission_suggestions` as `updatedPermissions`, untested. Right:
  `doctor` reruns that e2e after every Claude update; if it breaks, supervised Claude becomes
  allowedTools-only (`--permission-prompts none` plus `--allowedTools`).
- **Approval answers are unconfirmed except on Codex** (`pt-drivers`): only Codex acknowledges
  an answer (`serverRequest/resolved`, emitted as `approval_decision` with `confirmed: true`).
  Claude and Grok answers stay `confirmed: false`, so per SPEC 12a item 4 the hub must keep
  them Uncertain. Right: take the matching `tool_result` (or its absence at `turn_settled`) as
  the harness's evidence.
- **Claude prompt ack is matched in order** (`pt-drivers`, claude.rs): the
  `--replay-user-messages` echo carries no client id, so echoes pop the oldest unechoed request
  id. Safe while only one send is in flight per sleeve (SPEC 12b item 3). Right: key by a
  client id if Claude ever echoes one.
- **Claude session_info before init** (`pt-drivers`): `system/init` arrives only after the first
  input, so the driver reports the assigned (`--session-id`) or resumed id at spawn and checks it
  at init and at each `result`; a mismatch is a protocol error, and the hub kills (G14).
- **Untested permission paths** (`pt-drivers`): supervised Codex (`approvalPolicy untrusted`,
  `sandbox workspace-write`, likely to clash with bwrap) and supervised Grok (no
  `--always-approve`) have mappings but no e2e run yet, to save quota. Unsupervised Codex is
  `never` plus `danger-full-access`: the hub's sandbox is the boundary. Pi has no gate, so a
  supervised Pi spawn is refused. Right: one e2e each before supervised sleeves ship.
- **Pi extension dialogs are cancelled** (`pt-drivers`, pi.rs): `extension_ui_request`
  select/confirm/input/editor gets `cancelled: true` plus a notice. Right: route confirm and
  select to `approval_request` if the owner runs a Pi extension gate.
- **Cost and usage gaps** (`pt-drivers`): Grok reports `costUsdTicks` in an undocumented unit and
  Codex reports no cost, so `cost_usd` is None for both; Grok has no context window in the
  prompt reply. Codex per-turn usage is the difference of `thread/tokenUsage` totals. Right:
  fill from the quota readers once their units are known.
- **Error classes from text** (`pt-drivers`, event.rs `classify`): Codex uses `codexErrorInfo`
  and Claude `api_error_status` when present; everything else is keyword matching on the
  harness's message, and an unexpected exit is `crash` unless the stderr tail says auth or
  quota. Right: structured fields as each harness exposes them; collect real auth and quota
  failures into the fuzz seeds.
- **Shutdown kills only the direct child** (`pt-drivers`, session.rs): `Session::shutdown`
  closes stdin, waits, then `Child::kill`; descendants (MCP servers, shells) are the hub's
  systemd scope to reap (`KillScope`). The exit waiter polls `try_wait` every 50 ms. Right:
  wire the scope kill into shutdown once pt-sandbox lands, and a pidfd wait instead of polling.
- **No driver-side handshake timeout** (`pt-drivers`): a Codex or Grok handshake that never
  replies leaves the session without `session_info`; the hub's G5 (Booting 60 s) covers it.
  Explicit compaction is not exposed either (native only, SPEC 3.7). Right: `compact()` per
  harness (Codex `thread/compact/start`, Pi `compact`) when the hub asks for it.
- **Text block ids** (`pt-drivers`, emit.rs): a late authoritative end for an already-closed
  block is dropped (Pi ends thinking after its text began), but a late *delta* for a closed id
  would open a second block with the same id. Grok sends no message id, so its blocks are
  numbered per turn and tool call. Right: per-harness block identity from content indexes if
  either shows up in real streams.
