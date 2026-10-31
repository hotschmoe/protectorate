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
- **SHA-256 is portable Rust** (pt-hash): about 235 MB/s on the aarch64 dev host, near
  coreutils `sha256sum`. The aarch64 SHA2 instructions (`vsha256hq_u32` and friends in
  `std::arch::aarch64`, plus `is_aarch64_feature_detected!("sha2")`) would give several GB/s, but
  calling a `#[target_feature]` function from generic code needs `unsafe`, which pt-hash forbids.
  Right: once a release-sum or content-addressing path is measured as slow, add the accelerated
  compress behind one audited unsafe call (in pt-hash with a narrow exemption, or in pt-sys).
- **No CRC-32C yet** (pt-hash): SPEC 15 lists it for the journal; it lands with pt-journal.
- **pt-http is Unix-only** (std `UnixStream`), and outside Linux `peer_cred` is Unsupported, so
  unix connections are refused there. Right: `getpeereid` on the BSDs and macOS; a Windows build
  gates the unix listener out if a Windows binary ever needs pt-http.
- **Unix socket mode is set after bind** (pt-http `bind_unix`): a connect can race the chmod to
  0600. The hub checks peer uid anyway. Right: bind inside a 0700 directory the hub owns. Bind also
  fails on an existing path, so the hub must decide about stale sockets after a crash.
- **No Date header** in pt-http responses (RFC 9110 wants one from servers with a clock).
- **pt-http client opens one connection per request** (`Connection: close`) and a unix connect
  has no timeout. Right, if the CLI ever gets chatty: keep-alive reuse for idempotent requests.
- **WebSocket extras left out** (pt-http): no permessage-deflate and no subprotocol negotiation
  helper (a handler can set `Sec-WebSocket-Protocol` itself). Origin checks are the hub's job.
- **pt-http is about 1950 lines** (about 1700 without blank and comment lines) against a 1500-line
  target. Right: trim when the hub's real use shows which parts can go.
- **pt-sandbox: no automatic fallback without a scope.** `systemd-run --user --scope` works here
  only with `XDG_RUNTIME_DIR` set (the builder sets it; it fails with "No medium found" without it
  and needs a user manager, so a headless box needs `loginctl enable-linger`). `SandboxSpec.scope =
  false` runs bwrap alone, but nothing probes and flips it yet, and then there are no limits and
  no `cgroup.kill`. Right: the hub probes once at startup and records it in `doctor.json`.
- **pt-sandbox masks skip missing paths and mask files with /dev/null.** bwrap cannot create a
  mountpoint on the read-only root, so a credential dir that appears later in the owner's home is
  not masked until the next incarnation. The mask list is also home-relative guesses; a harness
  with a new credential location needs an entry in `default_masks`.
- **pt-sandbox has no fuzz target** for `resolve_git_dirs` (the one parser). Right: add one with
  the other crates' fuzz targets.
- **Own namespaces helper instead of bwrap** stays a later option (host dependency on bwrap's
  version; 0.13.1 here).
