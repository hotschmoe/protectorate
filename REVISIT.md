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
- **Control-key pin is checked by a separate fetch** (`go/cmd/member`): member fetches `/key` and
  compares it with the pin, then tsnet fetches `/key` again itself, so an active man in the middle
  (stronghold) can answer the two fetches differently. It catches a re-keyed or replaced headscale,
  not an attacker. Right: point tsnet at a loopback front inside member that answers `/key` with the
  pinned key and forwards the rest, so Noise IK fails against any other server, or an upstream
  pinned-server-key option.
- **dnsfallback can ask Tailscale's public DERPs** (`go/cmd/member`): if the control host name does
  not resolve, controlclient queries the bootstrap DNS of the DERP map baked into tailscale.com;
  `dnsfallback.SetCachePath` only merges into that map and there is no knob. The e2e uses IP
  literals, so it never hits this. Right: split-horizon DNS that cannot fail, or a control URL that
  needs no lookup.
- **Reverse proxy proven with a stand-in, not Traefik** (`scripts/net-e2e.sh`): ts2021 (`POST
  /ts2021`, `Upgrade: tailscale-control-protocol`) and DERP (`GET /derp`, `Upgrade: DERP`) both pass
  Go's `httputil.ReverseProxy` behind TLS with a path allowlist, and DERP-only traffic works through
  it. Open for stronghold: Traefik v3's entrypoint `respondingTimeouts.readTimeout` (60 s default)
  on hijacked upgrade connections, which would cut the long-lived control and DERP streams; HTTP/1.1
  to the backend for upgrades; `trusted_proxies` with stronghold's IP. Right: the same e2e phases
  against the real router.
- **A member starts only while control answers `/key`** (`go/cmd/member`): the pin check needs it,
  so a member restarted during a headscale or stronghold outage stays down until control is back.
  Right: decide whether a start with an unverifiable key may run on the cached netmap.
- **Windows UDP socket and the firewall prompt**: officewin (the owner's desktop) ran member
  DERP-only (`TS_DEBUG_ALWAYS_USE_DERP=1`), which binds no UDP socket, so no Defender Firewall prompt
  could appear. Whether a normal member's UDP bind prompts on a desktop is unverified. Right: try it
  on a spare Windows box; the installer adds a rule for the fixed `--udp-port`, or Windows nodes
  build with `ts_omit_udptransport` and relay through DERP.
- **tsnet close can hang** (`go/cmd/member`): with `TS_DEBUG_ALWAYS_USE_DERP`, wireguard-go's bind
  close waited forever, on stop and after `--print-registration`; member now gives Close 5 s and
  exits anyway. Right: an upstream fix.
- **`headscale version` reports our module version**, since headscale reads it from the main
  module's build info. Right: stamp it, or have the hub report the go.mod pin instead.
- **Android member is unverified at runtime**: `GOOS=android GOARCH=arm64 CGO_ENABLED=0` builds (a
  PIE using /system/bin/linker64, which picks tailscale's raw-binary netmon and dnsproxyd paths), so
  that is the Termux target rather than a static linux/arm64 build. No device has run it (netmon,
  DNS, SELinux exec). Right: a Termux trial before promising it.
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
- **App settings live in memory** (hub URL, device token, theme): lost on restart. Right: a small
  file in the app support directory through `dart:io` and `localStorage` through
  `dart:js_interop` on web, without path_provider or shared_preferences.
- **Linux desktop build unverified on the dev host**: it has no cmake, ninja or GTK 3 headers, so
  `flutter build linux` fails there. Right: install the toolchain on the build host and add the
  Linux bundle to the release.
- **Chat text is plain monospace** (`app/lib/chat.dart`): no Markdown, code fences show as typed.
  Right: our own small renderer for fences and lists if plain text reads badly.
- **App tables don't sort** (nodes report 7, rule 8: sort by clicking the header rule). Right: a
  sort key per column in the shared table rows.
- **F-keys in the web build**: the browser may keep some keys (F12 always opens devtools; F5 and
  F11 depend on the browser). Right: check per browser; the `===>` command line covers every
  F-key screen meanwhile.
- **Journal retention** (pt-journal): every snapshot and segment is kept forever, because product
  code deletes nothing yet (RULE 1). Right: a retention rule (keep the last N snapshots and their
  segments, archive the rest into backups) once the owner approves deleting old segments.
- **CRC-32C in pt-journal** (SPEC 15 lists it under `pt-hash`): a plain one-table loop. Right: move
  it to `pt-hash` when that crate exists; fast: slicing-by-8 or the SSE4.2/ARMv8 CRC instruction.
- **pt-core records are kept forever** (State): every effect record, message request id and job
  stays in the state, and there is no State encoding, so the journal is only ever replayed from
  the start of a segment. Right: a versioned State codec for `Journal::write_snapshot`, then
  pruning terminal records once they are reported and older than the snapshot.
- **Pane silence (G6)** is approximated: any Working screen verdict counts as progress, not "the
  screen changed". Right: the pane reader sends progress only on screen damage.
- **Node polling** (pt-core jobs): every Tick re-polls every Running, SubmitUncertain,
  Cancelling and Lost job. Right: per-job backoff driven by the lease.
- **sleeve_core reference model** (pt-core fuzz): transitions are checked as paths of at most 5
  edges through the lifecycle and effect tables, and the guardrail machine only through I10 and
  the scenario seeds. Right: one reference arm per table row for each machine, compared state by
  state.
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
- **SSH instead of headscale for hub-to-node** (owner, 2026-11-01: keep headscale for now and
  revisit). Plain OpenSSH would carry the same frames: the hub runs `ssh node`, and the node's
  authorized_keys pins the hub's key to `restrict,command="protectorate-node serve --stdio"`.
  Nothing on the hub would listen for nodes, so headscale's control and DERP, the one surface nodes
  reach today, would disappear, as would the whole `go/` tree, headscale's 49 MB and member's 18 MB.
  `serve --stdio` already works behind real sshd on msr1-kids and agent-world (with a resident agent
  so jobs outlive the session). What SSH loses is NAT traversal, roaming and stable addresses (DHCP
  churn; the snapdragon laptop moved .176 to .172). The combined form is SSH as the protocol, with
  the official tailscale client on any machine the hub cannot route to.
- **Sleeve credentials are seeded by copy** (`pt-hub`, exec.rs `credential_files`): at
  provisioning and before each launch the hub copies the owner's login files for the harness
  (`~/.claude/.credentials.json` and `~/.claude.json`, `~/.codex/auth.json`, `~/.grok/auth.json`,
  `~/.pi/agent/{auth,models,settings}.json`) into the sleeve's private home, once; a token the
  harness refreshes in one home does not reach the others, and `~/.claude.json` carries the
  owner's project list. Right: a doctor-guided login per sleeve home, or a shared read-only
  credential mount.
- **Same-uid callers on the unix socket are the owner** (`pt-hub`, http.rs): a process inside a
  sleeve scope is identified by its cgroup and gets that sleeve's capability whatever it sends,
  but a vendor general agent runs as the owner's user, so envoy-only holds only while it presents
  its `PROTECTORATE_TOKEN`. Right: vendor agents under their own uid, or an owner token required
  on the socket for anything that is not the owner's own shell.
- **The capability is in bwrap's argv** (`--setenv PROTECTORATE_TOKEN`), so `/proc/<pid>/cmdline`
  of the outer bwrap shows it to same-uid processes. Right: hand the environment over a file
  descriptor once the sandbox helper subcommand exists.
- **The hub socket is reached through a bind of `run/`** (the whole directory, at its host path),
  not a fixed path like `/run/protectorate.sock`: bwrap cannot create a mount point on the
  read-only root, and a directory bind keeps a socket re-created by a restarted hub reachable.
  Sleeves get `PROTECTORATE_SOCKET` and `run/bin/protectorate` (a link to the hub binary) on PATH.
  Right: the sandbox helper makes the fixed mount point, or pt-sandbox gains src-to-dst binds.
- **A View child runs in a scratch dir** (`pt-hub`, exec.rs `workdir`): `SandboxSpec` binds the
  workspace writable, so a read-only view's cwd is `sleeves/<id>/view` and the root is visible
  read-only at its own path through `/` (not when the root sits under a masked directory). Right:
  a read-only workspace mode in pt-sandbox.
- **Checkpoints are commit objects nobody points at** (`pt-hub`, git.rs): `commit-tree` over the
  working tree with HEAD as parent, through a private index into `sleeves/<id>/objects` (the
  repo's objects as an alternate), listed in `sleeves/<id>/checkpoints.jsonl`. No branch moves,
  because a commit on a root's branch would land on the owner's branch. Right: decide with the
  owner; likely a hidden ref namespace in a hub-owned bare mirror, plus a pruning rule.
- **Hub git has no timeout and no sandbox** (worktree add, checkpoint, brief facts). Right: the
  2 s read-only-sandbox runner the old hub had.
- **Approval answers are reported Lost** (`pt-hub`, exec.rs): `RespondApproval` is written, then
  left Uncertain, because only Codex confirms an answer; the harness's `approval_decision` entry
  becomes `ApprovalResolved` for activity. Right: Acked on a confirmed decision (Codex) or the
  matching `tool_result`.
- **No OOM exit cause**: `Exited` carries the exit code or Killed, so G4 never fires. Right: read
  the scope's `memory.events` `oom_kill` before reporting the exit.
- **A message sent during the post-restart probe is rejected** (pt-core `deliverable` ignores
  `Sleeve::probing`): a recovered Idle sleeve takes a `SendPrompt` before `ScopeProbed` answers,
  the executor holds no session for that g, and the Nack rejects the message (the sender is
  told). Right: `deliverable` is false while `probing`, so the message waits in the queue.
- **select_target is first-eligible** (`pt-hub`, select.rs): it walks `resleeve_to` in order,
  skipping a harness whose binary is missing or that `doctor.json` lists under `ineligible`. No
  quota weighting, no task-size class (SPEC 8, D27).
- **Node jobs are held, not run** (`pt-hub`, exec.rs jobs executor): the transport (SSH or the
  tailnet) is undecided, so `ScheduleJob` and the node effects are logged and never answered;
  no API method creates a job yet.
- **The D9 code rule is not in code**: removing sleeves, node maintenance and protected pushes
  asked for by a vendor agent rest on Envoy's CLAUDE.md. Right: the hub refuses those methods
  for a request whose chain starts at an `agent:` message unless an owner approval names it.
- **Who sent a message is a text prefix** (`pt-hub`, api.rs): mail reaches the harness as
  `[from sleeve <name>] ...` and agent requests as `[from agent:<name>] ...`, since a prompt has no
  other sender field; owner and Envoy messages are unprefixed. Right: revisit if a harness grows
  a sender field.
- **The brief is facts only** (`pt-hub`, brief.rs): no history search hits (no index yet), no
  planned-resleeve summary, no checkpoint id, and the 6 KiB small-target variant is missing.
- **Ticks are journaled and every batch clones the State** (`pt-hub`, core.rs): 8640 Tick frames
  a day, and the published view is a full copy that grows with the never-pruned records. Right:
  the State codec and snapshots (above), then a cheaper view (shared sub-trees or per-sleeve
  snapshots).
- **Transcripts never rotate** (`pt-hub`, transcript.rs): one JSONL per sleeve, a 4096-entry
  tail in memory, and a full read for older ranges and for `EnvoyWait`. Fast: an offset index.
- **Activity `since_ms` restarts with the hub**: it is tracked in the core thread, not journaled.
- **The stale unix socket is unlinked at start** (`pt-hub`, serve.rs): only when the path is a
  socket and the journal lock is held (no other hub can own it). This is the one file product
  code deletes; the owner may prefer a fresh socket name per start.
- **Wait and EnvoyWait hold a connection thread** for up to an hour, out of 256. Fine for a few
  clients; right if Envoy fans out: wait on the event stream instead.
- **`PT_TEST_FAULT=usage_limit:<sleeve name>`** (`pt-hub`, hub.rs `Fault`): the one test-only
  knob. The named sleeve's first settled turn in this hub process is reported to the core as
  `TurnEnd::UsageLimit`, which trips G1 and forces a resleeve; the e2e `forced_resleeve_with_brief`
  uses it. The hub warns at start when it is set. Never set it in production.
- **RealHub never ran against a hub** (`app/lib/real_hub.dart`): written over the generated
  `HubApi` before any hub serves the API; checked once against a throwaway dart:io server
  (snapshot, deltas, resume with `from_seq`, errors). Reconnect backoff is 1 to 30 s; a
  transcript loads its last 2000 entries with no paging back. Right: an e2e run of the app's
  client against the built hub, and paging when a chat scrolls past its first entry.
- **Device methods are placeholders** (`pt-api`): `ListDevices`, `PairDevice` and `RevokeDevice`
  exist in the table with guessed shapes and answer `unavailable`. Right: design pairing (scan
  to join for apps, how a `pd1.` token is issued and shown once, expiry) and reshape them.
- **TakeoverTerminal is a stub** (`pt-api`): it answers `unavailable`, and the app's TERMINAL
  view is a placeholder. Right: the PTY handoff of SPEC 3.3 and an xterm.dart view (build
  stage 6).
- **Web events token in the URL** (`app/lib/api/transport_web.dart`): a browser cannot set
  `Authorization` on a WebSocket, so the web client sends `?access_token=` on `/api/events`,
  where Traefik's access log may record it. Right: a short-lived ticket from a POST, or a
  same-origin cookie for the web build the hub serves.
- **Scopes are coarse** (`pt-api`, `Scope`): owner, sleeve or envoy_only per method. The subtree
  rule for sleeve scope (own sleeve and children; Envoy reaches all) is the hub's job per
  request. Envoy cannot `Decide`, though the design lists answering sleeves as Envoy's; a child
  has no API path to mail its parent or Envoy (Wait `report` assumes one). Right: decide whether
  Envoy may answer tool approvals, and add a mail method with its own routing rule.
- **Large integers on web** (`app/lib/api/api.g.dart`): u64 fields are Dart `int`, which on web
  is a JS double, exact only to 2^53. Seqs, sizes and epoch ms stay far below; a random 64-bit
  value would not. Right: send such fields as strings if one ever appears.
- **Job log as lossy text** (`pt-api`, `JobLog`): log bytes go out as UTF-8 with invalid
  sequences replaced, so a binary log is not exact. Right: base64 bytes if exact logs matter.
- **Web build served by the hub still starts on FakeHub** (`app/lib/main.dart`): the hub URL is
  empty until set in SETTINGS, and settings last for the session only. Right: default to the
  page's own origin on web when the hub serves it.
- **check-gen needs the Dart SDK** (`Makefile`): `pt-apigen`'s output is formatted with
  `dart format`, so `make check` needs `dart` on PATH (or `DART=`). Right: fine while the app is
  built on the same host; emit formatter-stable Dart directly if CI ever lacks Flutter.
- **headscale and member are not embedded** (`pt-fleet`, `protectorate-node`): `Config` takes
  both binary paths, and the node finds `member` next to itself (or `--member`). Right: the hub
  and node binaries carry them and unpack to a content-addressed path, so an install is one file.
- **LAN-direct DERP is plain HTTP** (`pt-fleet` `Config::lan_direct`, node `member::command`):
  with an `http://` server URL both sides set `TS_DEBUG_USE_DERP_HTTP=1`, tailscale's debug knob,
  and the DERP map pins the IPv4 with STUN off. Right: production goes through Traefik with TLS
  (`derp.tls`); keep the knob for LAN tests only.
- **headscale's bwrap has the host network** (`pt-fleet`, headscale.rs): user, pid, ipc, uts and
  cgroup namespaces, caps dropped, only its own dir writable, but no seccomp filter, no memory or
  pid limit, and it can reach anything the hub can. Right: the namespaces helper with a seccomp
  allowlist and a cgroup when the hub has a scope.
- **Child stop signals go through the `kill` binary** (`pt-fleet`, child.rs): SIGTERM, then
  SIGKILL after 10 s, and bwrap's child is found through /proc. Right: `pt_sys` grows a `kill`
  wrapper (libc is already there) and pidfds to close the pid-reuse race.
- **Fleet logs never rotate**: `tailnet/headscale.log`, `tailnet/member.log` and the node's
  member log grow forever. Right: size-capped rotation shared with the hub's transcripts.
- **Stale sockets accumulate** (`pt-fleet`): a killed member leaves `member.sock`, so the next
  start uses `member-2.sock` and so on, and the stand-in hub makes `ctl-<pid>.sock` per run,
  because nothing here may unlink. Right: one path, unlinked at start after a failed connect (as
  the hub's own socket is), once the owner agrees to that rule for runtime files.
- **QR codes never decoded by a scanner**: zbarimg is not on the dev host and the app has no
  scanner yet; the `encode` fuzz target only proves pt-qr matches qrcodegen module for module.
  Right: decode `join`'s terminal output with a real scanner (zbar or the app) in an e2e.
- **`uninstall` never ran** (`protectorate-node`, service.rs): it removes only manifest paths, but
  no test or machine has exercised it, since that deletes files. Right: run it on a throwaway VM
  (or a container with systemd) in an e2e, then reinstall.
- **No Windows service, and member can outlive serve** (`protectorate-node`): Windows runs
  `serve` in the foreground; if serve dies, member keeps running because it is not in a Job
  Object with kill-on-close. Right: a Windows service (runner account, D10) and member in a
  kill-on-close Job Object.
- **Windows jobs are refused while presence is unknown**: officewin's console session reports no
  LastInputTime to a non-interactive caller, so presence is `unknown`, treated as present, and
  the gate refused `uname`-style jobs (correct, but no Windows job ran through the fleet). Right:
  the presence fix above, then a fleet job e2e on Windows.
- **The poller runs the headscale CLI every 15 s and a thread per node** (`pt-fleet`,
  poller.rs): fine for a home fleet. Right: headscale's gRPC over its unix socket, and one
  scheduler thread if the fleet ever reaches hundreds.
- **`Node.id` is headscale's node id** (`pt-fleet` view.rs): the agent's own id and journal epoch
  are only in `NodeView.hello`. Right: decide which id the API exposes once the hub wires the
  poller; a reinstalled node keeps neither.
- **headscale listens on the whole LAN** (stand-in hub at `192.168.10.44:7480`): no host firewall
  admits only the reverse proxy. Right: production binds where Traefik reaches it and a firewall
  rule admits only stronghold.
- **Stand-in hub** (`pt-fleet` examples/fleet.rs): agent-world, msr1-kids and officewin joined a
  fleet run by the example, home `~/.local/share/protectorate-fleet`, until the hub embeds
  pt-fleet. Right: the hub takes over that `tailnet/` dir (the nodes pin its Noise key), or the
  nodes rejoin.
- **No node install script**: an owner copies `protectorate-node` and `member` to the machine and
  runs `join`, then `install-service`. Right: the hub serves both binaries and a one-line
  installer, checked by SHA256SUMS.
- **`join` waits 15 minutes then gives up**: headscale keeps a registration that long in memory,
  so an unapproved node must run `join` again. Right: fine; the app's scan path approves in
  seconds.
