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
- **pt-sandbox: other roots are hidden only if the hub masks them** (SPEC 3.4 light isolation).
  The read-only `/` shows every other workspace; the hub is to push every other sleeve's claim
  onto `masks` (not a child's own root, which holds its git dirs; a View child needs its root
  visible). A claim made after a launch stays visible until the next incarnation, like the
  credential masks.
- **pt-sandbox: the runtime dir is private and empty.** The host's `$XDG_RUNTIME_DIR` is covered
  by a 0700 tmpfs, so the user bus and the systemd user manager (which would start units outside
  the scope and sandbox) are gone, and so are the keyring, ssh-agent, gpg-agent and pipewire
  sockets. Abstract unix sockets still reach the host, since the network namespace is shared
  (X11 `@/tmp/.X11-unix`, other users' buses). Right: a network namespace or the own helper.
- **pt-sandbox: the docker socket is reachable** (SPEC 2, principle 10). The owner's supplementary
  groups survive the user namespace, so `/run/docker.sock` is root on the host from any sleeve; the
  threat model accepts that. Right: revisit with the threat model.
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
- **Claude prompt ack rides an undocumented echo** (`pt-drivers`, claude.rs): each stdin
  message carries a `uuid` we make (a random per-incarnation prefix plus a counter), and the
  `--replay-user-messages` echo returns it (checked on Claude Code 2.1.316), so an echo
  confirms exactly its own message; a late echo of a settled one is a notice, an unknown one a
  protocol error. If Claude stops echoing our uuid, no prompt is ever confirmed (the e2e
  `ready_resume_tool` catches it). Right: `doctor` reruns that e2e after every Claude update.
- **Claude session_info before init** (`pt-drivers`): `system/init` arrives only after the first
  input, so the driver reports the assigned (`--session-id`) or resumed id at spawn and checks it
  at init and at each `result`; a mismatch is a protocol error, and the hub kills (G14).
- **Untested permission paths** (`pt-drivers`): supervised Codex (`approvalPolicy untrusted`,
  `sandbox workspace-write`, likely to clash with bwrap) and supervised Grok (no
  `--always-approve`) have mappings but no e2e run yet, to save quota. Unsupervised Codex is
  `never` plus `danger-full-access`: the hub's sandbox is the boundary. Pi has no gate, so a
  supervised Pi spawn is refused. Grok decisions never widen: allow-once without an
  `allow_once` option is refused (unsupervised it cancels), deny without `reject_once`
  cancels instead of `reject_always`. Right: one e2e each before supervised sleeves ship.
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
  block is dropped (Pi ends thinking after its text began). A delta for a closed id (a steer's
  ack or a notice landed mid-block, as in the Pi `steer` e2e) opens a continuation block
  `<id>~<n>` that keeps only the streamed text; ids are remembered for the last 64 blocks.
  Blocks are cut at 1 MiB with a marker, and live deltas stop there. Grok sends no message id,
  so its blocks are numbered per turn and tool call. Right: per-harness block identity from
  content indexes if either shows up in real streams.
- **Pi turn ids are ours** (`pt-drivers`, pi.rs): Pi's rpc names no turn, so acks and
  `turn_started` carry `pi-turn-<n>`, counted per incarnation, and a steer must name the
  running one or gets `TurnEnded`. A steer Pi queued just as its run ended stays in Pi's own
  queue (Pi's behavior), so it can surface in the next run. A settle before Pi answered our
  prompt is taken as the previous run's and dropped. Right: Pi's own turn id if rpc gains one.
- **Grok prompt identity from `_meta.promptId`** (`pt-drivers`, grok.rs): `session/update`,
  `_x.ai/queue/changed` (`runningPromptId`) and the prompt reply name Grok's prompt id, an
  x.ai extension outside ACP. Notices naming one of the last 16 ended prompts, or another
  prompt than the running one, are dropped. Right: ACP's own prompt id if it gains one.
- **Linux desktop bundle is built but not released**: `flutter build linux --release` succeeds on
  the dev host (arm64) and was not run beyond that. Right: add the Linux bundle to the release.
- **Chat Markdown is a small subset** (`app/lib/chat.dart`): fences (diffs coloured), headings,
  lists, quotes, inline code, bold, italics and links; no tables, no syntax highlighting, links are
  not clickable. Right: tables and clickable links when transcripts need them.
- **App tables don't sort** (nodes report 7, rule 8: sort by clicking the header rule). Right: a
  sort key per column in the shared table rows.
- **F-keys in the web build**: the browser may keep some keys (F12 always opens devtools; F5 and
  F11 depend on the browser). Right: check per browser; the sidebar and the command palette
  (Esc, Ctrl+K) cover every F-key screen meanwhile.
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
  escapes cancel, timeout and lowering, and nothing outlives a dead agent's timeout. After a
  restart the reaper kills only members it can prove (pidfds taken while the stopped leader holds
  the group id), so the members of a job whose leader already died live on. `--no-scope`
  forces this mode (the tests use it). Right: the system service's delegated cgroup subtree
  (design report 1.3), one cgroup per job with `cgroup.kill`.
- **Job isolation is limits only** (`pt-node`): a job gets its timeout, a capped log and a scope,
  process group or Job Object, and runs as the agent's user with the agent's environment plus its
  `env`. There is no `cpu.max`/`memory.max`/`pids.max`, no namespaces and no `mode` (sandboxed,
  plain, admin). Right: design report 1.3.
- **No runner account** (`pt-node`, `sys/windows_job.rs`; D11): jobs run as the agent's account,
  so as root on Linux and LocalSystem on Windows, inside a Job Object with kill-on-close, all
  JOB_OBJECT_UILIMIT_* restrictions and the idle priority class once a person is present. SPEC 2
  principle 10 (2026-11-01) accepts this, so no low-privilege account is planned. Right: revisit
  with the threat model.
- **A node fetches its update from GitHub itself** (`protectorate-node` update.rs): `Update{version,
  api, repo}` has the node call the releases API and download its asset through its own curl, so
  a node without outbound internet cannot update, and every node spends unauthenticated API
  calls. A node with running jobs answers `busy` instead of draining. Right: the hub streams the
  verified bytes over its connection (`FetchArtifact` in reverse) when a node has no route out.
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
  revisit; 2026-11-02, after reading network-without-go.md: hole punching and relay are required,
  so SSH comes back only with an answer for both). Plain OpenSSH would carry the same frames: the hub runs `ssh node`, and the node's
  authorized_keys pins the hub's key to `restrict,command="protectorate-node serve --stdio"`.
  Nothing on the hub would listen for nodes, so headscale's control and DERP, the one surface nodes
  reach today, would disappear, as would the whole `go/` tree, headscale's 49 MB and member's 18 MB.
  `serve --stdio` already works behind real sshd on msr1-kids and agent-world (with a resident agent
  so jobs outlive the session). What SSH loses is NAT traversal, roaming and stable addresses (DHCP
  churn; the snapdragon laptop moved .176 to .172). The combined form is SSH as the protocol, with
  the official tailscale client on any machine the hub cannot route to.
- **SSH enrollment and key rotation** (`docs/design/network-without-go.md`): the isolated SSH
  proof of concept uses pre-provisioned keys. Right: owner-only enrollment with an authenticated
  out-of-band exchange in both directions, durable node IDs, host-key pinning on every dial,
  revocation, and an overlap and recovery procedure for both host and hub keys.
- **SSH on Windows and service privilege separation** (`protectorate-node`, `stdio.rs`): the
  existing stdio splice only works on Unix, and a system service and its sshd account cannot
  automatically share a mode-0600 socket. Right: authenticated, access-controlled local IPC and
  a long-lived service on each supported OS, with Windows forced-command and presence/job behavior
  proved on an actual Windows node; verify macOS and Termux separately.
- **SSH for nodes outside the routed private networks** (`docs/design/network-without-go.md`):
  plain SSH does not do NAT traversal or relay. Right: prefer the owner's private WireGuard or
  tailnet; if a separate private jump host is needed, design and test its restricted reverse
  tunnels, pins, failure recovery and isolation from hub listeners before using it.
- **One-way trust on physical networks** (`docs/design/network-without-go.md`): SSH authenticates
  the node connection but does not stop a compromised node from initiating unrelated LAN, VPN or
  proxy connections to the hub. Right: enforce the source zones, return traffic and proxy policy
  end to end, including IPv6 and spoofing, before retiring the tailnet policy.
- **Other sleeves' workspaces are masked as of the launch** (`pt-hub`, exec.rs
  `other_claims`): a sandbox masks every live claim but its parent's and its children's, read
  from the view when the incarnation starts. A root spawned later stays visible to sandboxes
  already running until they relaunch, and a workspace outside every claim (the owner's other
  repos) is readable. Right: a deny-by-default view of the filesystem (only the claim, its git
  dirs and the toolchains bound in).
- **One shared login per harness** (`pt-hub`, logins.rs): every sleeve of a harness uses the
  hub's one login in `<home>/logins/`, bound read-write, so concurrent sleeves may refresh the
  same token at once. That has not been seen to conflict, and the old Rust hub ran this way; it
  is not tested against the real CLIs. Codex and Grok follow a symlink from the sleeve's home to the store when they rewrite
  their login; a harness that replaced the file by rename would fork it again (not seen yet).
  Quota reads that same login and never refreshes it, so after a quiet spell with no sleeve on a
  harness (Claude's access token lasts about 8 hours) its row shows `login expired` until a
  sleeve runs. The copies an earlier hub seeded into sleeve homes stay where they are, except the
  files a link now replaces at that harness's next launch.
- **A harness program is bound by its resolved file only** (`pt-hub`, exec.rs): grok lives in
  the masked `~/.grok/downloads/`, so a launch binds the resolved program read-only. That is
  enough for grok, which fills an empty `~/.grok` itself (checked by hand, 1.0.70; no real Grok
  sleeve has run in the e2e). A vendor program that needs files beside it under a masked dir
  would not start. Right: fine until one does.
- **Same-uid callers on the owner's socket are the owner** (`pt-hub`, http.rs): sleeves never
  see that socket (`run/api.sock`); they get `run/sleeves/api.sock`, which takes only a sleeve
  capability (checked against the caller's scope cgroup when it has one), so with `scopes` off a
  sleeve still cannot act as the owner. On the owner's socket a caller without a token is the
  owner only when `/proc/<pid>/cgroup` of its SO_PEERCRED pid was read at the request and names
  no sleeve of this hub; an exited peer is refused. The pid is read per request, not at accept,
  so a peer that exits and whose pid is reused by an owner process in between would pass.
  A vendor general agent runs as the owner's user, so envoy-only holds only while it presents
  its `PROTECTORATE_TOKEN`. Right: identity captured at accept in pt-http (SO_PEERPIDFD or the
  cgroup read then), and vendor agents under their own uid.
- **The capability is in bwrap's argv** (`--setenv PROTECTORATE_TOKEN`), so `/proc/<pid>/cmdline`
  of the outer bwrap shows it to same-uid processes. Right: hand the environment over a file
  descriptor once the sandbox helper subcommand exists.
- **The sleeves' socket is reached through a read-only bind of `run/sleeves/`** (the
  directory, at its host path), not a fixed path like `/run/protectorate.sock`: bwrap cannot
  create a mount point on the read-only root, and a directory bind keeps a socket re-created by a
  restarted hub reachable. `run/bin/` (the CLI link, first on PATH) and the binary it names are
  bound read-only too, so a sleeve cannot replace or unlink what other sleeves use. Sleeves get
  `PROTECTORATE_SOCKET`. Right: the sandbox helper makes the fixed mount point, or pt-sandbox
  gains src-to-dst binds.
- **A View child runs in a scratch dir** (`pt-hub`, exec.rs `workdir`): `SandboxSpec` binds the
  workspace writable, so a read-only view's cwd is `sleeves/<id>/view` and the root is visible
  read-only at its own path through `/` (not when the root sits under a masked directory). Right:
  a read-only workspace mode in pt-sandbox.
- **Checkpoints are commit objects nobody points at** (`pt-hub`, git.rs): `commit-tree` over the
  working tree with HEAD as parent, through a private index into `sleeves/<id>/objects` (the
  repo's objects as an alternate), listed in `sleeves/<id>/checkpoints.jsonl`. No branch moves,
  because a commit on a root's branch would land on the owner's branch. Right: decide with the
  owner; likely a hidden ref namespace in a hub-owned bare mirror, plus a pruning rule.
- **Hub git has no timeout and no sandbox** (worktree add, checkpoint, brief facts). It runs
  nothing from the workspace: the real repo is read only through `rev-parse` and `worktree add
  --no-checkout` with hooks off and no system or global config, and every command that reads a
  work tree or an index runs against a shadow git dir of the hub's own (`sleeves/<id>/
  shadow.git`: our config, HEAD as a commit id, the repo's objects as an alternate), so filters,
  hooks, fsmonitor and textconv a sleeve configured are never defined there. A child's checkout
  is therefore raw blobs (an LFS file stays a pointer). Right: a timeout, and the read-only
  sandbox runner the old hub had if git itself ever needs containing.
- **Provision has a completion barrier file** (`pt-hub`, exec.rs): `sleeves/<id>/provisioned`
  is written (fsynced) after the layout is whole; the probe answers Applied only on it, and a
  retried provision finishes a registered but cut-short worktree (`read-tree --reset -u`). A
  directory left by an add cut short before git registered it is not removed; git refuses a
  non-empty one and the provision fails. Right: fine; record the barrier in the journal if the
  home is ever split from it.
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
  to quota selection with task size Medium and no health list; when a reading is unavailable the
  first unknown candidate in the owner's order can win, while known-exhausted candidates cannot.
  A new sleeve always names its harness (`SpawnSleeve.harness` is
  required), so nothing else is chosen by quota, and G1 does not read quota yet. Right: the task
  size from the sleeve's history (D27), node and probe health, and an optional harness on spawn.
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
  to gate. Outside such a turn no node job waits for the owner, `maintenance` or not: a node runs
  every job with its own account (root, or LocalSystem on Windows), and SPEC 2 principle 10
  accepts that. Right: carry the origin through messages Envoy sends (a request chain), and add
  pushes to the list when the API grows them.
- **Vendor agents read a filtered Envoy transcript** (`pt-hub`, api.rs `agent_transcript`):
  `EnvoyRead` with the envoy-only token shows each `ga:` message and the assistant text and
  settle of the turn it started, up to another sender's message; owner and sleeve messages,
  tools and reasoning are left out. All agents share the one token, so they see each other's
  exchanges, and an answer can still quote what Envoy learned elsewhere. It reads the whole
  transcript per call. Sleeves get none of `EnvoyRead` or `EnvoyWait`. Right: a token per
  agent, and an index of agent turns if transcripts grow large.
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
  clients; right if Envoy fans out: wait on the event stream instead. The idle keep-alive is
  pt-http's 60 s (a request being answered is not idle), so up to 256 idle clients can still hold
  every slot for a minute. Right: a pt-http option that closes the oldest idle connection when
  the slots run out, and a shorter idle limit for unauthenticated connections.
- **A revoked caller's event stream ends within a second** (`pt-hub`, http.rs `events`): the
  stream re-runs the caller's authentication once per bus wait (at most 1 s) and sends close
  1008 once it no longer yields the same principal. Right: a revocation notice on the bus that
  wakes every stream of that token at once.
- **`PT_TEST_FAULT=usage_limit:<sleeve name>`** (`pt-hub`, hub.rs `Fault`): a test-only knob.
  The named sleeve's first settled
  turn in this hub process is reported to the core as `TurnEnd::UsageLimit`, which trips G1 and
  forces a resleeve; the e2e `forced_resleeve_with_brief` uses it. The hub warns at start when it is set. Never set it in production.
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
- **TakeoverTerminal is a stub** (`pt-api`): it answers `unavailable`, and the app's Terminal
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
  observations and executor reports sharing one ordered stream. The hub meets it in the pump
  (`pt-hub`, exec.rs): a prompt's `Dispatched` is submitted when the driver's `user_message`
  for it comes through the event stream, so output read before the write is the previous
  turn's, and its `Acked` (with the native turn id from the ack or, failing that, from the
  `turn_started` that follows within 2 s) or `Nacked` when the ack does. Other executor reports
  are handed to the pump, which submits them after the harness events it has received; it wakes
  every 10 ms to look for them, since std channels have no select. A confirmed prompt whose turn
  id arrives later than 2 s stays unbound, so its turn cannot be steered. Right: drivers tag
  output with the native turn id where the protocol has one, and evidence must match the
  prompt's turn.
- **A prompt that never left waits for the next incarnation** (pt-core `return_unsent`,
  `deliverable`; `pt-hub` exec.rs `prompt`): after a restart nothing is delivered until the old
  scope is probed, and a prompt or steer still `Intent` when its incarnation is fenced goes back
  to the head of the queue. The executor leaves a prompt for an incarnation it has no live
  session for (or whose driver says it ended) unreported, so the fence finds it `Intent`. A
  prompt written to a harness that died before reading it is still "may have applied". Right:
  fine.
- **An unregistered systemd-run is found by its command line** (`pt-hub`, exec.rs
  `kill_unregistered`): before probing a scope it has no session for, the hub SIGKILLs any
  process of its uid whose argv is `systemd-run ... --unit <that scope>`, one a crashed hub
  started that has not yet become the sandbox, and waits for it to go; otherwise it could
  register the scope after the probe found none. Right: pt-sandbox registers the scope through
  the user manager's D-Bus API before forking the sandbox, so no unregistered launcher exists.
- **Cancel re-sent every Tick** (pt-core jobs): Cancelling re-sends an uncertain cancel at each
  10 s Tick until the lease ends. Right: the per-job backoff of node polling.
- **Torn batch versus corruption** (pt-journal, `open_segment`): a frame failing its CRC is cut as
  a torn append only when no intact frame follows it; otherwise open fails. A group-commit batch
  whose blocks reached disk out of order after a power loss also fails open, though none of it
  was acknowledged. The length is not checksummed, so a corrupted length is indistinguishable
  from a torn append and is cut. Right: a batch-end marker, so a torn batch is told apart from
  corruption, and a versioned format with protected headers.
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
- **Windows service through sc.exe** (`protectorate-node`, winservice.rs, supervise.rs): our own
  dispatcher and control handler (pt-node `sys/win32_service.rs`), but create, start, stop,
  delete and failure actions go through `sc.exe`, and the state dir is limited to SYSTEM and
  Administrators through `icacls.exe` (not recorded: the state dir is not ours to undo). Stop
  reports STOPPED and ends the supervisor, so the node, member and running jobs die by
  kill-on-close and the jobs read Lost after the next start. `uninstall` must run from a copy outside
  `%ProgramFiles%\protectorate-node` (Windows cannot delete the running exe). Tested on officewin
  with a sleeping stand-in for member (officewin must not join a fleet for this): install, member
  in the job, killing serve kills member and the SCM restarts serve, a job ran as SYSTEM, stop,
  uninstall left no service, files or process (that was `serve --service` before the
  supervisor). A reboot was not tried there. Right: our own SCM
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
  `supervised: ["claude"]` starts those harnesses with their approval gate on; only Claude's
  gate is run for real, by the driver (`supervised_approval`), and the hub's side of an approval
  across a restart by `d9_approval_across_restart` and the core fuzz seeds. (The paid hub test
  went away with per-hub logins: a suite hub has none.) Right: run Codex and Grok the same way.
- **Quota readers run on one thread** (`pt-hub`, quota.rs): a `pt_quota::Cache` (5 min TTL,
  stale after 10 min, backoff from 30 s) over the readers of installed harnesses, refreshed every
  `quota_poll_secs` (default 60; 0 never reads, which the e2e suite uses so no test hub reads
  the owner's vendor quotas); a changed row goes out as a `quota` message. Readings are not
  journaled, so a restart starts at `unknown`. Right: fine; persist the last good reading if a
  restart must keep dispatch weighting.
- **A stuck quota read can leave one worker behind** (`pt-quota`, cache.rs): publication has a
  deadline and cached rows age monotonically, but an OS-blocked file read cannot be cancelled;
  the cache allows at most one such worker per reader and retries only after it returns. Right:
  use a killable helper process if network filesystems make this a practical problem.
- **The web app embedded is about 17.7 MB** (`crates/protectorate/build.rs`; the bundled Inter fonts
  added about 1 MB on 2026-11-02): the release binary
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
- **A hub update restarts the process; it does not re-exec** (`pt-hub` update.rs; D17): the hub
  exits 75 and the service manager starts `releases/current`, so every sleeve's harness dies with
  it (bwrap `--die-with-parent`) and resumes as after a crash; there is no fd handoff. The new
  hub's health check is the core applying `HubRecovered` plus `GetHubInfo` on its own socket; it
  does not check the fleet or launch a sleeve. The headscale snapshot is taken only while the
  fleet is up. Old `releases/<version>` directories are never pruned. Right: re-exec with the
  listeners and sandboxes handed over, and a health check that includes a fleet round trip.
- **Release switches order their writes for crashes** (`pt-update` store.rs): probation and
  `previous` are each synced before `current` changes. Trial rollback restores the snapshot,
  syncs `current` back to the confirmed release, then records the outcome; `previous` stays
  confirmed. Debug-only `PT_TEST_CRASH_AT` exercises selected crash points. Right: fine.
- **Starting a just-written binary can fail with ETXTBSY** (`pt-update`, the node supervisor):
  on Linux a file still open for writing anywhere, including a descriptor a concurrent fork
  inherited for a moment before exec, cannot be executed. The update e2e hit it once under load
  (`node_update_admission_lost_reply_and_confirmation`, "Text file busy" at spawn); three reruns
  passed. A staged release is executed for its version check right after it is written. The
  update e2e's systemd stand-in now retries a busy start (`unbusy`), since its own test threads
  fork while others write wrappers; `release_switches_survive_a_crash_at_each_step` failed that
  way twice on 2026-11-02. Right: retry a start that fails with ETXTBSY a few times, briefly, in
  pt-update's version check and the node supervisor too.
- **Probation counts on wall-clock time and on the new binary's own watch** (`pt-update`): the
  units run `releases/previous/<bin> probation` before each start, which rolls back after
  `MAX_TRIAL_STARTS` (3) failed starts or a passed window, and the release on trial watches its
  own window. A new binary that starts and then hangs is never restarted, so it stays until the
  next start. The deadline is wall-clock milliseconds, so a clock jump moves it. Right: a
  systemd watchdog (`WatchdogSec`) the binaries ping, and a boot-relative deadline.
- **Windows release pointers are renamed aside** (`pt-update` `set_pointer_aside`): a rename
  cannot replace a directory symlink on Windows, so `current` and `previous` change in three steps
  (`.X-new` made, `X` to `.X-old`, `.X-new` to `X`); a crash between the renames leaves no `X`
  until the next `Store::open` (the supervisor's, before each start) finishes the switch. Tested
  only on Linux, where the debug knob `PT_TEST_POINTER_ASIDE=1` takes this path (with crash points
  `<pointer>-aside` and `<pointer>-swapped`); Windows directory-symlink rename and removal
  semantics are from the documentation, not a run. Right: run docs/windows-node-update.md's
  manual check on a spare Windows box, then a Windows e2e once a Windows runner exists.
- **The Windows service is a frozen supervisor** (`protectorate-node` supervise.rs, D11): the SCM
  runs `supervise --service` from the %ProgramFiles% copy made at install, which runs
  `releases\previous` probation (in a kill-on-close job, killed after 60 s; if it fails or hangs,
  the supervisor's own probation code counts the start and `current` starts anyway) and
  `releases\current` serve in a kill-on-close job with silent breakaway (the node's own children,
  including the idle probe in the console session, which may not share a job with session 0, go
  to jobs of their own), and restarts it 1 s after exit 75 and 5 s after any other. Known
  limitation: `node update` never replaces the supervisor and there is no remote command that
  does, so a bug in it is fixed only by a reinstall; installs from v0.7.0-dev.5 and older run
  `serve --service` directly and must be reinstalled once. The manual way out from the hub is a
  `--maintenance` job that reruns the installer through Task Scheduler (the job itself dies with
  the service it uninstalls); docs/windows-node-update.md has the steps, never run. A Windows
  node refuses an update to a binary whose `--version` lacks `supervise: 1`, so it cannot be
  downgraded below the supervisor. Never run on a real Windows node: the loop is e2e-tested on
  Linux only, and the breakaway, `CREATE_NO_WINDOW` children and stdio into node.log are
  compiled for Windows but untried. Right: the manual check in docs/windows-node-update.md on a
  spare box; a supervisor that updates itself only if a real need appears.
- **Node releases are state** (`protectorate-node install-service`): the unit runs
  `<state>/releases/current/protectorate-node`; install-service copies a binary that does not
  already run from there into `releases/<its version>/`. The manifest lists only the unit, so
  uninstall keeps every staged release with the rest of the state dir. Right: fine; a `prune`
  that keeps `current` and `previous` once disks fill.
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
  which do not exist yet. Antigravity's login location is unknown, so it is not checked. Disk
  comes from `df -P`, linger from `/var/lib/systemd/linger/<user>`, and each run starts one
  `systemd-run --user --scope true`. The hub runs it at start, every 6 h and on `doctor
  --refresh`; a hub with no cached report holds its first launch up to 60 s for the first run.
  New and resolved problems are hub events; nobody is mailed. A harness that fails to launch
  right after its update is not marked ineligible (only `--version`, login and sandbox are). The
  node has no `doctor` yet. Right: the quota-reader validity check, a node `doctor`, mail on new
  problems, and a launch failure after an update counted against the harness.
- **Harness updates run each program's own command, one at a time** (`pt-hub` harness_update.rs;
  D33, D34): `claude update`, `codex update`, `grok update` and `pi update self` every
  `harness_update_hours` (24; 0 runs only `harness update`), each only while no sleeve on a
  harness of that program is starting or mid-turn, retried every 3 s until then, so a sleeve that
  never idles holds it back indefinitely. `codex update` is kept for npm installs too: the npm
  launcher sets `CODEX_MANAGED_BY_NPM`, so the command updates through npm. A failed run is tried
  again only in the next period. Right: a ceiling on the deferral (stop accepting turns on that
  harness), and a retry with backoff after a failure.
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
- **History keeps a sleeve's newest lines** (`pt-history`, index.rs): each sleeve and each
  source may keep `history_budget_bytes` (default 1 GiB), counting each doc's source text
  plus its per-doc overhead. The hub reads that key once, at start. Past a
  source's budget, that source's oldest docs go.
  Past a sleeve's budget, the oldest docs of that sleeve go, from any of its logs, so a new
  session still gets indexed. `doctor` / `hub info` name each capped sleeve (`history capped: <id>`)
  and clear it once that sleeve is back under its budget. A sleeve at its
  budget does not stop the others. There is no cap across all sleeves. The measured ratio is
  1 GiB of logs to 11 MB of index.
  Right: raise the caps if a real log needs more of the index.
- **One source per session, decided per hub run** (`pt-hub`, history.rs): a headless session
  is in both its native log and the transcript; the native log wins once found, the transcript
  covers a session that ended without one, and the transcript waits at a live session's
  `session_info` until its log appears. The choice is per sleeve, and the cursor, the drop
  watermark and the budget share one key of sleeve plus path, so one sleeve's session id
  does not select another's log. Discover walks a
  harness's log root only while one of that sleeve's sessions is still undecided. A session
  already chosen as native is left out of the transcript even when that pass did not see the
  file, and a discover that returns a different path replaces the old one only when
  that path is gone, and then drops its docs.
  Decisions are not persisted, so a
  session indexed from the transcript whose log appears in a later run (a native resume of a
  session that had none) is indexed twice. Right: keep the choice per session in the index
  manifest.
- **A parent reads a child's live transcript** (`pt-hub`, api.rs `get_transcript`): `reaches`
  is the sleeve itself or a direct child, so a parent still receives the child's full
  transcript. That is supervision of a running child, and it is separate from history search,
  which a sleeve credential limits to its own sleeve id (D26). Right: keep it.
- **Pi must report the session id from argv** (`pt-drivers`, pi.rs): `get_state`'s `sessionId`
  has to equal the id the hub passed (`--session` on resume, `--session-id` on a new session).
  A different id is a protocol error. That becomes `Desync`, which trips G14
  (`ProtocolDesync`, a kill). The death then follows G3: at most two resumes in ten minutes,
  then a forced resleeve, and a third automatic resleeve within an hour parks the sleeve
  (`ParkReason::Flapping`). A Pi that mints a new id on resume takes that path. Right: keep
  the equality check.
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
  provider's own `baseUrl`, but plain HTTP is restricted to loopback. Right: fine while both are
  the owner's endpoints.
- **One account per native harness** (future feature, owner 2026-11-01; not a top priority):
  `pt_api::Subscription` is a fixed enum (claude, codex, grok, local, api), and each quota reader
  reads the hub's single login per harness (`<home>/logins/`). Two Grok
  accounts, for example, can't be told apart. Right: accounts as data. Each one is a named login
  (harness, a label, and the login home or config dir it lives in, such as `CLAUDE_CONFIG_DIR` or
  `CODEX_HOME` where the harness supports one). The label is probably the account's email, read
  from the login itself (owner's idea; to settle later). There is one quota row and reader per
  account, and the app's QUOTA screen lists every account. `select_target` weighs accounts rather than
  harnesses, and a sleeve's incarnation records the account it draws on.
- **Windows job identity, dropped** (`pt-node`, Windows job runner): the owner first chose
  (2026-11-01) ordinary jobs as LocalService or a restricted token, and maintenance jobs as
  SYSTEM. Two agent attempts at the token code were cut off by a safety classifier. The same
  day the owner's threat model (SPEC 2, principle 10) made it unnecessary, so jobs stay
  LocalSystem inside a Job Object. Right: revisit with the threat model.
- **Review coverage**: pt-core, pt-journal and pt-http have had an adversarial review, and pt-hub,
  pt-drivers and pt-sandbox one each by GPT-6.1-Terra (2026-11-01; its findings are being fixed).
  pt-quota, pt-history and pt-update have had none, and none of the fixes has been re-reviewed.
  Right: review each before it is trusted with more than the owner's own machines.
- **`pi_local` is unconfigured by default** (`pt-hub`, home.rs, doctor.rs): `pi_local_provider`
  and `pi_local_model` default to empty, and doctor marks `pi_local` ineligible
  until both are set, so the default `resleeve_to` (`["pi_local"]`) parks a sleeve that must
  resleeve. Right: offer the configured local model when there is one, or ask for it at setup.
- **Verus phase 1 boundaries** (`pt-journal`, `pt-frame`): the journal frame, CRC logic and pure
  recovery decision, and the node frame codec and sticky decoder errors, are verified. File I/O
  in `open_segment`, the frame reader's I/O, and `pt-frame::msg`/`pt-json` message parsing are not.
  The CRC proof covers the Castagnoli table and byte recurrence, not collision resistance. Verus
  trusts its specifications for Rust's standard collections and slices, including allocation behavior.
  Right: verify message parsing and model the file operations around the recovery decision, and
  maintain the existing differential fuzzing.
- **Incremental frame buffering can exceed one frame** (`pt-frame`, `Decoder::feed`): decoded
  payload length is capped at `MAX_PAYLOAD`, but a caller may feed an arbitrarily large slice,
  and the decoder copies all of it before parsing. `FrameReader` reads in 64 KiB chunks and drains
  frames between reads, but its initial `pending` input is also supplied by its caller. Right: a
  backpressure API or segmented input that bounds each allocation while preserving streams with
  many frames, then prove that bound. This allocation property is not covered by phase 1 yet.
- **Node protocol bump discipline** (`pt-frame`, `msg::PROTOCOL`): protocol 3 covers the Update
  change, and the hub refuses a mismatched Hello. Every incompatible wire change must bump the
  protocol. Right: detect incompatible message changes without a bump automatically in CI.
- **Four-hour fleet poller stall** (`pt-fleet`, d2dad77): the 2026-11-01 logs do not identify
  the blocked thread or prove a cause. Poll RPCs and member replies now have whole-call deadlines,
  and a finished node poll thread is restarted. Right: capture hub thread stacks and member logs
  if a stall recurs.
- **Release verification is not bound to an inode** (`pt-update`, store.rs): verified staged
  files can be replaced before the binary check or switch; agents on the owner's machines are
  trusted. Right: bind execution and switching to the verified inode if that threat model changes.
- **Enrollment record beside headscale, not one transaction** (`pt-fleet` `rename_node`,
  `remove_node`): headscale is written first, then `enrolled.json`; a rename whose record write
  fails is put back in headscale, a removal is finished by retrying it by id. Nothing compares
  the two on its own. Right: a doctor check that reports a record headscale does not list, or a
  name that differs.
- **Sleeves have no systemd user bus** (`pt-sandbox`): the runtime dir is a private empty tmpfs on
  purpose, so a project test that needs `systemd-run --user` fails inside a sleeve. Right: decide
  whether a sleeve gets its own user manager or a scope-local one, if such tests matter.
- **Deep target dirs overflow the unix socket path limit** (sleeves, e2e tests): a sleeve's
  workspace plus a test's own socket under `target/` can pass `sun_path` (107 bytes), so sleeves
  pick short dirs by hand. Right: a short per-sleeve `CARGO_TARGET_DIR` or a socket dir under
  `/tmp`, once a second case shows which.
- **Sleeve toolchain env is guessed from the owner's home** (`pt-hub` `toolchain.rs`): PATH tail,
  cache dirs and the Flutter SDK come from conventional locations, the hub's env and
  `sleeve_path`; a cargo `rustup` auto-install, `cargo install` and a second Flutter SDK are
  unsupported (read-only). The shared registry, Go and pub caches are one blast radius for all
  sleeves. Right: per-sleeve overlay caches seeded from the owner's, if a sleeve damages one.
- **Sleeve git identity is seeded once** (`toolchain.rs`): `~/.gitconfig` is written when absent,
  so a changed owner identity reaches only new sleeves. Right: rewrite a file that still carries
  the seeded content.
- **Doctor ignores API-key env vars** (`pt-doctor`): `ANTHROPIC_API_KEY`, `OPENAI_API_KEY` and
  `XAI_API_KEY` no longer count as a login, since sleeves never see them. Right: a deliberate
  API-key login per harness in `<home>/logins/` if API billing is wanted for a sleeve.
- **Effort is checked by a fixed list for Claude, Grok and Pi** (`pt-drivers` `vet_effort`): each
  CLI's documented levels are compiled in; a model that accepts fewer is the CLI's to refuse.
  Codex is checked against its `model/list` catalog after the handshake. A resleeve to another
  harness starts on that harness's default effort without a notice of its own (the view shows
  `effort`). Right: read each CLI's model catalog if a model-specific refusal costs a sleeve.
- **Rate limits are a failed turn, not a guardrail** (`pt-hub` `exec.rs`): a 429 or "overloaded"
  that survives the harness's own retries ends the turn as an error, the sleeve stays and idles,
  and the error entry is in the transcript; only quota (G1) and auth (G2) resleeve. The reset
  times in Claude's `rate_limit_event` and Codex's `account/rateLimits/updated` are still
  ignored. Right: read them to hold the queue until the reset instead of letting the next
  message hit the limit again.
- **Exit classification reads a stderr tail** (`pt-drivers` `session.rs`, `exec.rs`): a harness
  that dies unasked is a G2 or G1 resleeve when `classify` calls its last words or a prior error
  of the turn auth or quota, so a crash whose stderr merely mentions "credentials" resleeves
  instead of resuming. Right: harness-specific exit codes or structured exit reasons if a false
  positive costs a cache.
- **A refused handshake is a kill, not a verdict** (`pt-drivers`): Codex, Grok and Pi end their
  own process on an error reply to initialize, thread/session open or get_state, and the exit
  takes the normal unasked-exit path (StartFailed, or G2/G1 from the reply's class). A handshake
  timeout with no reply is still G5's 60 s.
- **pi_durable removed**: removed on 2026-11-02 before it was built. The owner wants to revisit
  it later; the design is in the git history.
