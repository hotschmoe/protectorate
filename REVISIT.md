# REVISIT

Shortcuts taken to make something work, and what "right" (then "fast") would be. Newest last.
Each entry: where, what was done, what right looks like.

- **Release signing** (SPEC 6): no signing key; updates trust SHA256SUMS over HTTPS. Right: an
  offline-held key and a signature checked against a key embedded in the binary.
- **Outbound HTTPS through curl** (hub, nodes): no Rust TLS stack. Right: decide between
  curl forever, or a forked and trimmed rustls when curl's absence or output parsing bites.
- **Sandbox through bwrap**: right is our own small namespaces helper (SPEC 3.4).
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
- **Job priority** (`pt-node`): jobs on Windows are lowered through their Job Object's priority
  class (all processes, running and later). The `lower-idle PID` command still sets one process
  only. On Linux lowering walks the process tree (plus the scope's `cgroup.procs`) once, so
  processes forked afterwards keep normal priority, and nothing raises a job back when the person
  leaves. Right: a cgroup with `cpu.weight`/`io.weight` set to idle, which covers later children.
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
- **Node job journal as JSON lines** (`pt-node`, `jobs/journal.rs`): one fsync per entry, no
  CRC, no group commit, never compacted, so it grows with every job. A torn last line is
  dropped; any other bad line stops the agent. Right: pt-journal (length and CRC frames, group
  commit, torn-tail recovery), with compaction that drops collected jobs.
- **Linux jobs without systemd-run** (`pt-node`, `sys/linux_job.rs`): when a trial
  `systemd-run --scope` fails, jobs run in a plain process group. A job that calls `setsid`
  escapes cancel, timeout and lowering, and nothing outlives a dead agent's timeout. `--no-scope`
  forces this mode (the tests use it). Right: the system service's delegated cgroup subtree
  (design report 1.3), one cgroup per job with `cgroup.kill`.
- **Job isolation is limits only** (`pt-node`): a job gets its timeout, a capped log and a scope,
  process group or Job Object, and runs as the agent's user with the agent's environment plus its
  `env`. There is no `cpu.max`/`memory.max`/`pids.max`, no namespaces, no Windows runner account
  and no `mode` (sandboxed, plain, admin). Right: design report 1.3.
- **Node Update RPC is a stub** (`protectorate-node serve`): `Update{version, url, sha256}` answers
  `unsupported`. Right: SPEC 6a: download from GitHub through curl, check SHA256SUMS, stage, keep
  the previous binary, restart through the service manager, roll back after 10 min without hub
  contact; or take verified bytes streamed over the hub's connection.
- **Who may speak on the agent's listener** (`protectorate-node serve`): the unix socket is mode
  0600, so `member` must run as the agent's user; `--listen-tcp` (Windows, tests) trusts any
  local process to write an honest `tag:hub` header. Right: on Linux an installer-made group for
  the socket; on Windows a named pipe whose ACL admits only member's account.
- **Windows jobs** (`pt-node`, `sys/windows_job.rs`): cancel and timeout call
  `TerminateJobObject` at once (no grace period). After an agent restart every running job is
  Lost: kill-on-close ended it, or it ended unobserved, and the two cannot be told apart. The serve
  e2e tests are Linux only; Windows was checked by hand on officewin. Right: a console-control
  stop with a grace period for console jobs, a launch-time marker in the job (exit-code file) to
  tell the cases apart, and the e2e suite ported with `cmd /c` scripts.
- **Presence while a job runs** (`pt-node`, `jobs/mod.rs`): only PRESENT acts (lower or kill),
  polled every `--presence-interval` (15 s default); UNKNOWN mid-job changes nothing, although it
  blocks a start. Right: decide whether a detector failing mid-job should lower jobs too.
- **Tombstones have no bound** (`pt-node`): every (job, attempt) and its log stay until
  `AckCollected` (SPEC 12a item 5), so a hub that never acks fills the node's disk. Right: `doctor`
  reports old uncollected jobs and the hub acks after collection as part of its job machine.
- **Node ids and journal epochs from std's RandomState** (`pt-node`, `random_id`): SipHash keyed
  by the OS once per process, not a CSPRNG. Fine for ids; never use it for secrets. Right: read
  `getrandom`/`BCryptGenRandom` directly if a secret is ever needed on the node.
- **`serve --stdio` starts its own resident agent** (`protectorate-node`, `stdio.rs`): an ssh
  session is a splice into the agent on `<state>/agent.sock`, which the first session spawns
  detached (no service manager) and which exits after `--idle-exit` (300 s) with no job and no
  connection; a connection arriving in the instant of that exit is cut. An agent serving the same
  state dir through `--forward-socket` elsewhere holds the lock, so a stdio session then fails.
  Under an sshd without PAM there is no logind session or user bus, so `systemd-run --user` fails
  and jobs fall back to process groups. Not on Windows. Right: once SSH or member is chosen, the
  system service owns the agent and always listens on `agent.sock`; `--stdio` stays a splice.
