# REVISIT

Shortcuts taken to make something work, and what "right" (then "fast") would be. Newest last.
Each entry: where, what was done, what right looks like.

- **Release signing** (SPEC 6): no signing key; updates trust SHA256SUMS over HTTPS. Right: an
  offline-held key and a signature checked against a key embedded in the binary.
- **Outbound HTTPS through curl** (hub, nodes): no Rust TLS stack. Right: decide between
  curl forever, or a forked and trimmed rustls when curl's absence or output parsing bites.
- **Sandbox through bwrap**: right is our own small namespaces helper (SPEC 3.4).
- **Windows idle time through a probe process** (`pt-node`, `sys/windows_presence.rs`):
  `WTSINFOEXW.LastInputTime` is 0 for the console session even from LocalSystem (checked on
  officewin through a SYSTEM scheduled task), so the service starts `protectorate-node
  idle-probe` in the console session (WTSQueryUserToken + CreateProcessAsUserW on
  `winsta0\default`, windowless, kill-on-close job, 5 s timeout) and reads its
  GetLastInputInfo as one JSON line; lock state stays WTS's. One process per detection (every
  `--presence-interval` while jobs run, and per hub Presence call). From an ordinary admin (ssh)
  WTSQueryUserToken is refused (1314), so `presence` there stays UNKNOWN unless locked. The lock
  flag (`SessionFlags` 0 = locked) was only seen unlocked. Right: verify locked, and cache the
  probe's answer for a few seconds if its cost shows.
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
  (Windows checked on officewin: a running cmd and ping and a ping started later all read Idle.)
- **Windows build path**: no Windows SDK libs on the dev hub, so there is no cross-build; both
  Windows targets build natively on officewin (VS Build Tools 14.44 incl. ARM64 libs), now as the
  `windows` step of `make release-assets`: the workspace goes over as a tar pushed by scp into
  `%USERPROFILE%\pt-release\src`, `cargo +stable build -p protectorate-node` builds both targets
  with `%USERPROFILE%\.cargo\bin` first on PATH (the system Rust lacks the arm64 std), and the
  `.exe` files come back as base64 over ssh, checked against the remote sha256. That is rustup's
  stable (1.96 there) while Linux builds with 1.99.1, and the arm64 `.exe` has never been run (the
  snapdragon laptop was off). Right: one pinned toolchain (`rust-toolchain.toml`) on both sides, and
  cargo-xwin or a vendored SDK sysroot if officewin, the owner's desktop, should not be required.
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
- **pt-http client `read_timeout` is a whole-request deadline** (sending the request and reading
  all of the response) but keeps its per-read name, so callers in pt-hub, pt-fleet and
  protectorate did not change with it. Right: rename it `timeout` in one commit with its callers.
- **pt-http is about 2050 lines** (about 1780 without blank and comment lines) against a 1500-line
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
- **App settings live in memory on desktop and Android** (`app/lib/local_io.dart`): the web build
  keeps hub URL, device token and device id in `localStorage` (`local_web.dart`), but the
  `dart:io` side forgets them on restart, so a phone pairs again each launch. Theme is not kept
  anywhere. Right: a small file in the app support directory through `dart:io`, without
  path_provider or shared_preferences.
- **Linux desktop bundle is built but not released**: `flutter build linux --release` succeeds on
  the dev host (arm64) and was not run beyond that. Right: add the Linux bundle to the release.
- **Chat text is plain monospace** (`app/lib/chat.dart`): no Markdown, code fences show as typed.
  Right: our own small renderer for fences and lists if plain text reads badly.
- **App tables don't sort** (nodes report 7, rule 8: sort by clicking the header rule). Right: a
  sort key per column in the shared table rows.
- **F-keys in the web build**: the browser may keep some keys (F12 always opens devtools; F5 and
  F11 depend on the browser). Right: check per browser; the `===>` command line covers every
  F-key screen meanwhile.
- **Journal retention** (pt-journal): every snapshot and segment is kept forever, because product
  code deletes nothing yet (RULE 1). Right: a retention rule (keep the last N snapshots and their
  segments, archive the rest into backups) once the owner approves deleting old segments. The
  stream log is bounded on purpose (below): its messages are derived output, not the record.
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
  `env`. There is no `cpu.max`/`memory.max`/`pids.max`, no namespaces and no `mode` (sandboxed,
  plain, admin). Right: design report 1.3.
- **Runner account deferred: owner decision pending** (`pt-node`, `sys/windows_job.rs`; D11):
  Windows jobs run as the agent's account, so as LocalSystem under the service, inside a Job
  Object with kill-on-close, all JOB_OBJECT_UILIMIT_* restrictions and the idle priority class
  once a person is present. No `protectorate-runner` account is created and no logon rights are
  granted. Right: the owner decides; then a low-privilege local account that jobs run as.
- **Node Update RPC is a stub** (`protectorate-node serve`): `Update{version, url, sha256}` answers
  `unsupported`. Right: SPEC 6a: download from GitHub through curl, check SHA256SUMS, stage, keep
  the previous binary, restart through the service manager, roll back after 10 min without hub
  contact; or take verified bytes streamed over the hub's connection.
- **Who may speak on the agent's listener** (`protectorate-node serve`): the unix socket is mode
  0600, so `member` must run as the agent's user; `--listen-tcp` (Windows, tests) trusts any
  local process to write an honest `tag:hub` header; on Windows that includes every job, which
  runs as SYSTEM (runner account deferred). Right: on Linux an installer-made group for the
  socket; on Windows a named pipe whose ACL admits only member's account.
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
- **Resleeve selection is quota-weighted only among `resleeve_to`** (`pt-hub`, select.rs): the
  candidates that are headless, not excluded, installed and not `ineligible` in `doctor.json` go
  to `pt_quota::select_target` with task size Medium and no health list; with no fresh quota the
  first in the owner's order wins. A new sleeve always names its harness (`SpawnSleeve.harness` is
  required), so nothing else is chosen by quota, and G1 does not read quota yet. Right: the task
  size from the sleeve's history (D27), node and probe health, and an optional harness on spawn.
- **Protocol 1 nodes keep the old job path** (`pt-hub`, jobs.rs `TOKEN_PROTOCOL`, `cat_artifact`):
  a node whose Hello says protocol 1 (agent-world and msr1-kids until they are upgraded) gets no
  submit-token check, and its artifacts come back through `cat -- <path>` collection jobs whose
  log is the file (64 MiB cap; exact only because a successful `cat` is silent). Right: once
  both nodes run protocol 2, remove `cat_artifact` and every `protocol < TOKEN_PROTOCOL` branch,
  and refuse a protocol 1 node.
- **FetchArtifact reads one file at a time and hashes it twice** (`pt-node` `read_artifact`,
  `pt-hub` `fetch_artifact`): the hub asks 4 MiB slices; at the end the node re-reads the whole
  file for its SHA-256 and the hub compares it with the hash of what it received, so a file that
  changed between slices fails the check and is fetched again (3 tries). Files over 64 MiB fail.
  The opened file's own path is checked again only on Linux (`/proc/self/fd`); elsewhere the
  canonical-path check stands alone, so a directory swapped between check and open could lead
  outside. Right: a bounded archive when jobs return trees, and `GetFinalPathNameByHandleW` on
  Windows.
- **A held (job, attempt) with another token fails the new job** (`pt-hub` jobs.rs `run_job`):
  the node answers a submit of a (job, attempt) it holds with the job it holds; when that job's
  token is not the submit's, the hub ignores the status and reports the submit refused as
  invalid, so the job fails and the other submit's job stays on the node, unacknowledged. That
  happens only when node job ids repeat (a hub on an older journal with the same hub tag). Right:
  fine; an owner command to drop such a foreign tombstone if one ever shows up.
- **A node's own job directories can outlive a crash** (`pt-node` jobs): a job without `cwd`
  runs in `work/<job>.<attempt>/`, removed with its tombstone on AckCollected; an agent killed
  between the `collected` line and the removal leaves it. Right: sweep `work/` against the
  journal at open.
- **The scheduler picks the least-loaded eligible node** (`pt-hub`, jobs.rs `choose`): online,
  hello seen, presence AWAY (unless `even_if_present`), not excluded, then lowest load1; retried
  every 2 s. No per-node concurrency limit beyond one hub call at a time per node, no fairness
  between requesters. Right: node slots from the inventory and a queue per requester.
- **Hub job logs are refreshed only on the core's Tick** (`pt-hub`, jobs.rs): a running job's log
  is pulled when pt-core polls it (every 10 s Tick), so `job log` lags by up to a Tick. Right: a
  log follow RPC on the node that pushes appended bytes.
- **Hub job metadata outside the journal** (`pt-hub`, jobs.rs `JobStore`): the request record
  rides in the core's `JobSpec.command` (its digest is the spec's), but artifact results live in
  `<home>/jobs/<job>/<attempt>/artifacts.json` and the per-job view cache in memory. Right: fine
  while the core owns state; journal artifact results if `job show` must survive a lost home.
- **Fleet poll interval is one knob** (`pt-hub`, `fleet.poll_secs`, default 15): presence and the
  node list share it. Right: split them if presence must react faster than listings.
- **Embedded binaries are never pruned** (`pt-embed`): the hub unpacks headscale and member to
  `<home>/net/<name>-<sha256>`, the node member to `<state>/net/`, with our own inflate (after
  zlib's puff, a bit at a time); old versions stay, since nothing deletes. Build scripts gzip with
  the system `gzip -9`; officewin, which has none, gets member gzipped by release.sh. Right:
  prune old unpacked versions once the owner agrees to that rule; table-driven decoding if
  unpack time ever shows.
- **D9 covers three calls and the turn, not the request chain** (`pt-hub`, rule.rs): while
  Envoy's current turn was started (or steered) by an `agent:` message, Envoy's own `sleeve rm`,
  `job run` and spawns past `d9_spawn_limit` (default 3 per turn) wait for an owner approval
  (`d9_approval_timeout_secs`, default 900, then `timeout`). The origin is cleared when the turn
  settles, so a background process Envoy started in that turn acts as the owner's afterwards;
  a sleeve Envoy prompted on the agent's behalf is not gated at all; no push method exists yet
  to gate. A `maintenance` node job from any sleeve, Envoy included, always waits for the owner,
  whatever started the turn. Right: carry the origin through messages Envoy sends (a request
  chain), and add pushes to the list when the API grows them.
- **Who sent a message is a text prefix** (`pt-hub`, api.rs): mail reaches the harness as
  `[from sleeve <name>] ...` and agent requests as `[from agent:<name>] ...`, since a prompt has no
  other sender field; owner and Envoy messages are unprefixed. Right: revisit if a harness grows
  a sender field.
- **The brief has no summary** (`pt-hub`, brief.rs): facts plus pre-run history hits (the
  latest and first task prompts and the last tool error, through `pt_history::brief_query`, top
  3 each within 4 KiB, this sleeve only), but no planned-resleeve summary, no checkpoint id, and
  no 6 KiB small-target variant. Context usage is shown from the outgoing session's log; no
  guardrail reads it yet.
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
- **RealHub is exercised only by `scripts/app-e2e.sh`** (`app/lib/real_hub.dart`): the web build
  against a real hub (pairing, Envoy chat, sleeves, a D9 approval denied from the app), outside
  `make test` because it needs Chromium and the local model. Reconnect backoff is 1 to 30 s; a
  transcript loads its last 2000 entries with no paging back. Right: paging when a chat scrolls
  past its first entry, and the script in CI once a runner has Chromium.
- **Pairing codes are in memory and unthrottled** (`pt-hub`, devices.rs): `AddDevice` makes a
  ten-minute, single-use code (12 base32 characters, about 60 bits, with a small modulo bias);
  pending codes die with the hub; `PairDevice` is public and has no attempt limit. Tokens are
  stored as SHA-256 in `devices.json`; `last_seen` is written at most once a minute. Right: a
  per-address attempt limit if the hub is ever reachable beyond the owner's networks, and a QR
  code in the app for scan to join.
- **TakeoverTerminal is a stub** (`pt-api`): it answers `unavailable`, and the app's TERMINAL
  view is a placeholder. Right: the PTY handoff of SPEC 3.3 and an xterm.dart view (build
  stage 6).
- **Web events token in the URL** (`app/lib/api/transport_web.dart`): a browser cannot set
  `Authorization` on a WebSocket, so the web client sends `?access_token=` on `/api/events`,
  where Traefik's access log may record it. Right: a short-lived ticket from a POST, or a
  same-origin cookie for the web build the hub serves.
- **Scopes are coarse** (`pt-api`, `Scope`): owner, sleeve, envoy_only or public per method. The subtree
  rule for sleeve scope (own sleeve and children; Envoy reaches all) is the hub's job per
  request. Envoy cannot `Decide`, though the design lists answering sleeves as Envoy's; a child
  has no API path to mail its parent or Envoy (Wait `report` assumes one). Right: decide whether
  Envoy may answer tool approvals, and add a mail method with its own routing rule.
- **Large integers on web** (`app/lib/api/api.g.dart`): u64 fields are Dart `int`, which on web
  is a JS double, exact only to 2^53. Seqs, sizes and epoch ms stay far below; a random 64-bit
  value would not. Right: send such fields as strings if one ever appears.
- **Job log as lossy text** (`pt-api`, `JobLog`): log bytes go out as UTF-8 with invalid
  sequences replaced, so a binary log is not exact. Right: base64 bytes if exact logs matter.
- **check-gen needs the Dart SDK** (`Makefile`): `pt-apigen`'s output is formatted with
  `dart format`, so `make check` needs `dart` on PATH (or `DART=`). Right: fine while the app is
  built on the same host; emit formatter-stable Dart directly if CI ever lacks Flutter.
- **Node reports fenced by node, not by submit** (pt-core jobs): `JobStatus` names the node that
  answered, and a report from any node but the job's current one is dropped. A node chosen again
  for the same attempt, after its 10 min exclusion, is trusted to have no answer that old in
  flight. Right: a per-submit token in `NodeSubmit` that the node echoes in every status.
- **Prompt evidence by ordering** (pt-core, `prompt_evidence`): harness output confirms an
  in-flight prompt or compaction only once its `Dispatched` is in, which relies on a sleeve's
  observations and executor reports sharing one ordered stream. The hub meets it by handing a
  sleeve's executor reports to its pump (`pt-hub`, exec.rs), which submits them after the
  harness events it has received; the pump wakes every 10 ms to look for them, since std
  channels have no select. Right: drivers tag output with the native turn id where the protocol
  has one, and evidence must match the prompt's turn; a driver hook that puts the report into
  its own event channel would drop the poll.
- **Cancel re-sent every Tick** (pt-core jobs): Cancelling re-sends an uncertain cancel at each
  10 s Tick until the lease ends. Right: the per-job backoff of node polling.
- **Torn batch versus corruption** (pt-journal, `open_segment`): a frame failing its CRC is cut as
  a torn append only when no intact frame follows it; otherwise open fails. A group-commit batch
  whose blocks reached disk out of order after a power loss also fails open, though none of it
  was acknowledged. Right: a batch-end marker, so a torn batch is told apart from corruption.
- **Collecting lease** (pt-core jobs): an artifact check that does not answer within the lease
  makes the job Lost, and its next Exited report collects again, so a slow fetch can run twice.
  Right: fetch progress reported to the core, and the lease measured from it.
- **A binary built without Go has no member** (`protectorate`, `protectorate-node`): with no
  `go/bin` and no Go to build it, the hub has no fleet (`version` says so; the fleet e2e skips)
  and the node needs `--member PATH` (`--version` says so). The live nodes' units still pass
  `--member /usr/local/lib/protectorate-node/member`, which wins over the embedded one. Right:
  fine; a reinstall drops the flag.
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
- **Fleet logs never rotate**: `tailnet/headscale/headscale.log`, `tailnet/member.log` and the
  node's member log grow forever. Right: size-capped rotation shared with the hub's transcripts.
- **Stale sockets accumulate** (`pt-fleet`): a killed member leaves `member.sock`, so the next
  start uses `member-2.sock` and so on, and the stand-in hub makes `ctl-<pid>.sock` per run,
  because nothing here may unlink. Right: one path, unlinked at start after a failed connect (as
  the hub's own socket is), once the owner agrees to that rule for runtime files.
- **QR codes never decoded by a scanner**: zbarimg is not on the dev host and the app has no
  scanner yet; the `encode` fuzz target only proves pt-qr matches qrcodegen module for module.
  Right: decode `join`'s terminal output with a real scanner (zbar or the app) in an e2e.
- **Linux `uninstall` never ran** (`protectorate-node`, service.rs): it removes only manifest
  paths, but no test or machine has exercised it on Linux, since that deletes files. (Windows ran
  it on officewin, after a partial install and after a full one.) Right: run it on a throwaway VM
  (or a container with systemd) in an e2e, then reinstall.
- **Windows service through sc.exe** (`protectorate-node`, winservice.rs, serve.rs): our own
  dispatcher and control handler (pt-node `sys/win32_service.rs`), but create, start, stop,
  delete and failure actions go through `sc.exe`, and the state dir is limited to SYSTEM and
  Administrators through `icacls.exe` (not recorded: the state dir is not ours to undo). Stop
  reports STOPPED and ends the process, so member and running jobs die by kill-on-close and the
  jobs read Lost after the next start. `uninstall` must run from a copy outside
  `%ProgramFiles%\protectorate-node` (Windows cannot delete the running exe). Tested on officewin
  with a sleeping stand-in for member (officewin must not join a fleet for this): install, member
  in the job, killing serve kills member and the SCM restarts serve, a job ran as SYSTEM, stop,
  uninstall left no service, files or process. A reboot was not tried there. Right: our own SCM
  calls if sc.exe output parsing bites, a reboot test on a spare box, and stop that lets jobs
  finish or records them as interrupted.
- **`join` leaves the Windows state dir open until install-service** (`protectorate-node`):
  under `%ProgramData%` it inherits read access for every user (member keys), until
  install-service runs icacls. Right: join sets the private DACL itself.
- **No fleet job e2e on Windows yet**: presence now works from the service (AWAY through the
  idle-probe on officewin), and a job ran through the installed service's listener, but not
  through a hub and member. Right: a fleet job e2e on a Windows node.
- **The poller runs the headscale CLI every 15 s and a thread per node** (`pt-fleet`,
  poller.rs): fine for a home fleet. Right: headscale's gRPC over its unix socket, and one
  scheduler thread if the fleet ever reaches hundreds.
- **`Node.id` is headscale's node id** (`pt-fleet` view.rs): the API, `job run --node` (which also
  takes the name) and the core's job records use it; the agent's own id and journal epoch are
  only in `NodeView.hello`, and the hub derives its node-side job ids (`<hub tag>-j<n>`) and
  epoch from the hub's own journal. A reinstalled node keeps neither id. Right: fine until nodes
  are re-enrolled under the same name; then key nodes by the agent id.
- **headscale listens on the whole LAN** (stand-in hub at `192.168.10.44:7480`): no host firewall
  admits only the reverse proxy. Right: production binds where Traefik reaches it and a firewall
  rule admits only stronghold.
- **Stand-in hub** (`pt-fleet` examples/fleet.rs): agent-world, msr1-kids and officewin joined a
  fleet run by the example, home `~/.local/share/protectorate-fleet`. The hub now runs the fleet
  itself and can adopt that dir (`serve --fleet-dir ~/.local/share/protectorate-fleet/tailnet
  --fleet-listen 192.168.10.44:7480 --fleet-udp-port 7483`, after stopping the example), keeping
  the Noise key the nodes pin. Right: adopt it, then retire the example's `serve`.
- **Node installers ran in dry-run mode only** (`scripts/install-node.sh`, `install-node.ps1`):
  they fetch from the GitHub release (12b item 13 supersedes a hub-served installer), check
  SHA256SUMS, stage, then run `join` and `install-service`. install-node.sh was dry-run on
  agent-world (whose node is live; not root, so its 0700 state dir reads as unknown) and the ps1
  on officewin; install.sh's real path ran into a scratch home with stand-in `systemctl` and
  `loginctl`. No real node install, `join` from an installer, or real hub service start has run.
  Right: one real install each on a throwaway VM, then on a spare box.
- **`join` waits 15 minutes then gives up**: headscale keeps a registration that long in memory,
  so an unapproved node must run `join` again. Right: fine; the app's scan path approves in
  seconds.
- **Approvals are core state and kept forever** (pt-core approval.rs): the ledger is journaled
  like every other record and never pruned (see "pt-core records are kept forever"). A decision
  is recorded once: abandoning an Uncertain answer releases its record but not the decision, so
  the harness then waits until G8 or an interrupt. `allow_session` is recorded and sent as allow
  once, since `RespondApproval` carries only allow or deny. Right: a decision kind in the event
  and the record once a driver keeps session rules.
- **A D9 call does not survive a restart** (pt-core `recover_approvals`): the API call waiting on
  an action approval dies with the hub, so recovery marks the approval and a later decision is
  recorded as `abandoned` and runs nothing; the agent must ask again. One past its deadline at
  recovery ends `abandoned` with no decision. Right: fine while D9 calls are synchronous; a
  durable action queue if they ever become requests the hub carries out later.
- **A tool approval of a live incarnation outside G9 has no deadline** (pt-core
  `tick_approvals`): one asked in a pane, or left pending once the turn moved on without a
  settle, stays pending until the incarnation ends; its journaled deadline is shown but only G9
  answers the harness. Right: let the core time those out (cancel, nothing written) once panes
  report approvals.
- **The stream log is a side file, not the journal** (`pt-hub`, stream.rs): kept messages go to
  `<home>/stream.log` with one fdatasync per writer batch (a subscriber sees a message only
  once it is on disk, so seqs continue exactly after a crash), and it is rewritten through a
  temp file and rename once it holds twice what it keeps: the newest 10,000 messages or 7 days,
  whichever is more, plus hub events by the same rule. Entry messages repeat transcript JSON,
  so the file is larger than it needs to be. Right: store transcript references, or move the
  stream into journal snapshots once pt-journal has retention.
- **The first batch after a restart re-announces recent state** (`pt-hub`, core.rs): sleeves,
  and jobs and approvals pending or changed in the 10 minutes before the journal's last event,
  because a crash between the journal's fsync and the stream log's could lose their messages.
  A sleeve removed in that window is not re-announced. Right: journal the stream position with
  each batch, or a single fsync for both.
- **`supervised` is new and mostly untested in the hub** (`pt-hub`, exec.rs): config
  `supervised: ["claude"]` starts those harnesses with their approval gate on; only Claude is
  run end to end (`tool_approval_across_restart_paid`). Right: run Codex and Grok the same way.
- **Quota readers run on one thread** (`pt-hub`, quota.rs): a `pt_quota::Cache` (5 min TTL,
  stale after 10 min, backoff from 30 s) over the readers of installed harnesses, refreshed every
  `quota_poll_secs` (default 60; 0 never reads, which the e2e suite uses so no test hub reads
  the owner's vendor quotas); a changed row goes out as a `quota` message. Readings are not
  journaled, so a restart starts at `unknown`. Right: fine; persist the last good reading if a
  restart must keep dispatch weighting.
- **pi_durable shows as pi_local in the API** (`pt-hub`, wire.rs): the experiment has no API
  harness. Right: add one if it outlives the experiment.
- **The web app embedded is about 17.7 MB** (`crates/protectorate/build.rs`): the release binary
  is 19.4 MB with it and 1.7 MB without; with gzipped headscale and member too (25 MB) the
  release hub assets are 44.9 MB (arm64) and 47.8 MB (amd64). Debug symbols, the experimental CanvasKit and the skwasm
  renderers are left out; both CanvasKit variants stay (Chromium and the full one for other
  browsers, only Chromium tested). Files go out uncompressed with an ETag. Right: precompressed
  gzip copies with `Content-Encoding` and a check in Firefox and Safari.
- **The events WebSocket checks Origin against Host** (`pt-hub`, http.rs): a browser page from
  another origin is refused, so the web app works only from the hub that serves it (or a
  client that sends no Origin, like dart:io). Right: an allowed-origins setting if a separately
  hosted web app is ever wanted.
- **The MCP bridge has tools only** (`protectorate mcp --envoy`): envoy_send, envoy_wait and
  envoy_read over line-delimited JSON-RPC, protocol versions 2025-06-18, 2025-03-26 and
  2024-11-05; no resources, prompts or notifications, and the agent name comes from `--as` or
  `clientInfo.name`. Right: grow it with what the vendor apps actually call.
- **Quota readers drop the old extras** (`pt-quota`): no reset credits (Claude cedar_ember, Codex
  rate-limit-reset-credits, Grok GetRemainingResets and its protobuf walker), because
  `pt_api::Quota` has no field for them; no Claude `extra` overage window; no Codex `app-server`
  path (it would launch the CLI, which may refresh a token; the wham endpoint is the only source);
  no Antigravity reader (no `Subscription` for it, the `agy` CLI is not installed here, and
  Antigravity is pane-only, D13). Right: add a resets field to `Quota` when dispatch uses
  credits; revisit Codex `app-server` if wham breaks; Antigravity when it has a subscription.
- **Local endpoint address can still be configuration** (`pt-quota`): `local_url` (or the probe's
  `--local` and `PT_LOCAL_URL`) overrides the `baseUrl` of Pi's `pi_local_provider`, which is used
  when it is empty, so the two can still disagree when both are set. Right: drop `local_url` once
  every hub leaves it empty.
- **Quota selection constants are untuned** (`pt-quota`): usable-now minimums per task size
  (5, 15, 30 percent), the 48 h half-life and the 168 h stand-in for an unknown reset are the old
  dev-hotschmoe values; `TaskSize` thresholds and N are caller inputs with no defaults. Right: tune
  from real dispatch history once a workspace has any.
- **`select_target` takes `now_ms`** (`pt-quota`): the pure function needs a clock for time to
  reset, so it is the first argument. Pi with a vendor login (`PiVendor`) and Antigravity are never
  selected (no quota subscription behind them).
- **Quota fuzz runs wrote corpus into `fuzz/seeds`** (`pt-quota`): `cargo fuzz run <t> seeds/<t>`
  adds new inputs to the seed dir; only the hand-written seeds (named, not hex) are committed.
  Right: run with `corpus` as first dir and seeds as the second.
- **pt-update is not called yet** (`pt-update`): the library stages, switches, rolls back and
  keeps probation, but `protectorate update`, the hub's supervisor wrapper (`switch_checked`
  within `HUB_HEALTH_WINDOW`), the headscale DB snapshot before a hub update, and the node's
  `Update` RPC (stage, `switch_on_probation`, restart through the service manager,
  `hub_contacted` on each hub RPC, `check_probation` at start and every minute) are all still to
  write in the binaries. The hub-streamed bytes for a node without internet are not in it either.
- **Node self-rollback runs inside the new binary** (`pt-update` probation): a new node binary
  that crashes before its first `check_probation` loops under the service manager and never
  reverts. The deadline is wall-clock milliseconds, so a clock jump moves it. Right: the unit's
  `ExecStartPre` (or the Windows service) runs the check from `releases/previous`, the binary
  that is known to work, and the deadline counts from a monotonic boot-relative stamp.
- **Release pointers are Unix-only** (`pt-update` `set_pointer`): switching `current` and
  `previous` is a symlink plus rename; on Windows it returns an error. `install-node.ps1` makes
  the first `current` with `mklink /D` and refuses to run on an installed node. Right: a tested
  Windows swap (directory symlink replaced by `MoveFileEx`, or a pointer file read by the
  service) before the Windows node takes the `Update` RPC.
- **The node service does not run `releases/current`** (`protectorate-node install-service`):
  install-node.sh stages into `<state>/releases/<version>/` and runs `join` and `install-service`
  from there, but `install-service` copies the binary into
  `/usr/local/lib/protectorate-node`, so the unit would not follow a switch. Right: the unit's
  `ExecStart` is `<state>/releases/current/protectorate-node` and the manifest lists the releases
  dir.
- **arm64 Linux assets link against the build host's glibc** (`scripts/release.sh`): the task's
  native arm64 build needs glibc 2.34 (it links on the dev hub's Debian 12); amd64 is built by cargo-zigbuild
  for glibc 2.31. Right: `aarch64-unknown-linux-gnu.2.31` through zigbuild too, if an older arm64
  box ever runs the hub or a node.
- **APK, desktop and Android-node assets are hooks only** (`scripts/release.sh` step `app`):
  `protectorate-android.apk`, `protectorate-desktop-{linux,windows}-{amd64,arm64}` and
  `protectorate-node-android-arm64` (SPEC 6) are not built; the step says so, and `APP=1` fails.
  The hub assets do embed the web build (step `web`) and their arch's headscale and member, and
  every node asset its target's member.
  Right: `flutter build apk` with the existing signing recipe, the desktop bundles (the Linux one
  needs cmake, ninja and GTK 3 on the build host), and the node once `pt-node` has Android code
  paths.
- **Release publishing is untried** (`make release-publish`): it stops at a dry run unless
  `PUBLISH=1`, refuses a dirty tree, an untagged HEAD and a tag missing on origin, and uses
  `gh release create --verify-tag`; no release has been created from `next`. GitHub immutable
  releases (SPEC 6) are not checked. Right: enable immutable releases on the repo, then publish a
  first `v*-dev.1` with the owner.
- **Installers read GitHub's JSON with grep** (`install.sh`, `install-node.sh`): with no jq, a
  channel's newest tag comes from `"tag_name": "..."` matches, ordered by `sort -V` with the
  pre-release dash read as `~`; the API is unauthenticated (60 requests an hour per address),
  as is pt-update's. `--version TAG` skips the lookup. Right: fine while GitHub's format holds;
  an optional token through a curl config file, never argv, if rate limits bite.
- **Doctor checks presence, not validity** (`pt-doctor`): a login counts when its file is
  non-empty or its key variable is set; SPEC 7 wants validity through the read-only quota readers,
  which do not exist yet. Antigravity's login location is unknown, so it is not checked; the
  update commands are listed, never run (the 24 h `pi update self` schedule of D34 is not in);
  `codex update` is listed even for an npm install. Disk comes from `df -P`, linger from
  `/var/lib/systemd/linger/<user>`, and each run starts one `systemd-run --user --scope true`.
  Neither binary has a `doctor` subcommand yet (`pt-doctor-probe` prints the report) and failures
  mail nobody. Right: wire `doctor` into hub and node, add the quota-reader validity check, mail
  the owner on new problems.
- **The version stamp falls back to `git describe`** (`pt-version`): a build without
  `PROTECTORATE_BUILD_VERSION` (plain `cargo build` or `cargo test`) gets `git describe --tags
  --always --dirty`, recomputed when HEAD, its branch or the tags move, so a `-dirty` mark is the
  tree's at the last such move, not now. The installers check the staged binary's first
  `--version` line against the tag. Right: fine; releases always set it.
- **Own keyword index instead of SQLite FTS5** (`pt-history`, SPEC 3.7): segments, BM25 and
  merges are ours, to avoid `rusqlite` plus bundled C SQLite. Right if it proves insufficient
  (recall, query features, corruption we cannot explain, or size well past the measured 1 GiB of
  logs to 11 MB): `rusqlite` with FTS5, pinned and listed in DEPENDENCIES.md.
- **Grok docs carry the session start time** (`pt-history`, extract/grok.rs): `chat_history.jsonl`
  records have no timestamp, so every doc gets `summary.json` `created_at`, and time filters are
  per session for Grok. Grok context usage is not read at all. Right: per-record times and usage
  from the session's `updates.jsonl` or `usage.json`.
- **Tool errors by text for Codex, none for Grok** (`pt-history`): a Codex output counts as
  failed on "Process exited with code N" (N not 0) or "Script failed"; Grok tool results carry no
  error flag and are never indexed. Right: a structured exit code if either log gains one.
- **Claude subagent logs are skipped** (`pt-history`, discover.rs and claude.rs): sidechain records
  and `subagents/` files are not indexed. Right: index them with a parent link once a brief or
  search needs a subagent's work.
- **Merges re-tokenize stored text** (`pt-history`, index.rs): merging two segments decodes their
  texts and builds a new one from scratch. Fast: a k-way merge of postings, if merge time shows up.
- **Leftover segments after a crash mid-merge** (`pt-history`, index.rs): a crash after the
  manifest rename but before the merged segments are removed leaves unreferenced `seg-*` files.
  Right: sweep unreferenced segment files at open.
- **One source per session, decided per hub run** (`pt-hub`, history.rs): a headless session
  is in both its native log and the transcript; the native log wins once found, the transcript
  covers a session that ended without one, and the transcript waits at a live session's
  `session_info` until its log appears. Decisions are not persisted, so a session indexed from
  the transcript whose log appears in a later run (a native resume of a session that had none) is
  indexed twice. Right: keep the choice per session in the index manifest.
- **Native logs are found by session id under the sleeve's home** (`pt-hub`, history.rs): the
  drivers report the session id, not the log path, so the hub walks the harness's log root in the
  sleeve's private home (`pt_history::discover`), each second while a live session has no log
  yet. Right: the path from the driver where the protocol states it (Pi's `get_state`
  `sessionFile`, Codex's thread path).
- **A corrupt history index is moved aside, never pruned** (`pt-hub`, history.rs): at start an
  index that fails to open is renamed `<dir>.corrupt-<ms>` and rebuilt in the background; the
  copies accumulate. Right: drop them once the owner agrees to a rule for derived files. A
  rebuild reads every source again, so search answers partially until it catches up.
- **No history tools on `mcp --envoy`** (`protectorate`, mcp.rs): the bridge holds the
  envoy-only capability of vendor general agents (D9), and D26 gives history to Envoy and to
  sleeves for their own runs only, so `history_search`/`history_read` there would always be
  refused. Right: add them if the owner widens D26 to vendor agents.
- **Local quota key from Pi's models.json** (`pt-quota`, local.rs): the `pi_local_provider`
  entry's `apiKey`, as a set environment variable's name or a literal; a `!command` key is never
  run (the local row stays `unknown`). The key goes to `local_url` even when that is not the
  provider's own `baseUrl`. Right: fine while both are the owner's endpoints.
