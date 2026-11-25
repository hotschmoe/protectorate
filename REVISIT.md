# REVISIT

Shortcuts taken to make something work, and what "right" (then "fast") would be. Newest last.
Each entry: where, what was done, what right looks like. The Rust-era list is in
`.archive/REVISIT-rust-era-20261124.md` (local, gitignored).

- **Release signing** (SPEC 6): no signing key; updates trust SHA256SUMS over HTTPS
  (`src/update/store.zig`, `src/update/version.zig` `verify_sha256`). The Android APK is signed
  locally by `scripts/release.sh`, but the binaries and SHA256SUMS are not. Right: an offline-held
  key and a signature checked against a key embedded in the binary.
- **Windows idle time through a probe process** (`src/node/sys.zig`, `probe_windows_console_idle`,
  `windows_idle_probe_json`; `src/main_node.zig` `idle-probe`): `WTSINFOEXW.LastInputTime` is 0 for
  the console session even from LocalSystem (checked on officewin through a SYSTEM scheduled task),
  so the service starts `protectorate-node idle-probe` in the console session (WTSQueryUserToken +
  CreateProcessAsUserW on `winsta0\default`, no window, 3 s timeout then TerminateProcess) and reads
  its GetLastInputInfo as one JSON line; lock state stays WTS's. The answer is cached for 10 s. From
  an ordinary admin (ssh) WTSQueryUserToken is refused (1314), so `presence` there stays UNKNOWN
  unless locked. The lock flag (`SessionFlags` 0 = locked) was only seen unlocked. Right: verify the
  locked state.
- **Linux evdev path untested with real input** (`src/node/sys.zig`, `scan_evdev`, `poll_evdev`):
  hub has no keyboard or mouse, agent-world's AT keyboard needs root. Reading an actual keystroke
  was not exercised, and the logind branches (`query_logind`) are not run against a real session in
  the e2e (`scripts/zig-e2e.sh` only uses the `PROTECTORATE_NODE_TEST_PRESENCE` override). Right:
  type on a real seat once.
- **Linux idle needs a long-lived detector** (`src/node/sys.zig`, `PresenceDetector`,
  `query_logind`): without readable evdev devices presence falls back to logind, and IdleHint=no is
  reported PRESENT with idle null (desktops that never set it, such as bare X or sway, stay PRESENT
  forever, the safe direction). GNOME sets IdleHint after its idle-delay and IdleSinceHint is the
  moment it was set, so idle is underestimated by that delay. `/dev/input` needs root or the `input`
  group (the system service is root). One-shot `presence` cannot use evdev (no history);
  `watch-presence` and `serve` keep a detector. Right: record IdleSinceHint changes over time and
  add the X11/Wayland session-bus readers if a desktop box needs them.
- **Windows GPUs from the registry class key** (`src/node/sys.zig`, `collect_gpus`): entries are
  read from `Control\Class\{4d36e968-...}\0000..0015` with no dedupe, so ghost entries from replaced
  drivers can show up; VRAM is `HardwareInformation.qwMemorySize` with `MemorySize` as fallback (a 4
  GiB cap on old drivers). Right: dedupe by (vendor, device, name) keeping the newest driver, and
  DXGI/`nvidia-smi` for live GPU numbers when GPU sampling arrives.
- **Linux GPU names** (`src/node/sys.zig`, `collect_gpus`): `nvidia-smi` when present, else
  `/sys/class/drm` where the name is the raw `PCI_ID` (no `pci.ids` lookup), SoC GPUs report only
  the kernel driver, VRAM only for amdgpu and nvidia-smi, utilisation only from nvidia-smi. Right:
  per-vendor readers with the sample's utilisation and temperature.
- **macOS node has only a Linux and Windows reader set** (`src/node/sys.zig`): `aarch64-macos`
  builds and ships (`build.zig` `release-all`, `scripts/release.sh`), and `src/node/service.zig` has
  a macOS branch, but presence always returns AWAY (`headless`) and the OS version, CPU model and
  RAM readers cover Linux and Windows only. There is no Android node. Right: macOS readers (IOKit or
  `sysctl`) and a decision on Android.
- **Sandbox through bwrap** (`src/sandbox/spec.zig` `build_bwrap_argv`, `src/sandbox/exec.zig`
  `probe_bwrap`, `src/hub/runtime.zig`): the hub probes bwrap once at startup and falls back to a
  Landlock guard (`build_guard_argv`, `probe_landlock`) when bwrap is missing; only the pid
  namespace is unshared. Right: our own small namespaces helper (SPEC 3.4), which also removes the
  host dependency on bwrap's version.
- **Sandbox masks skip missing paths and mask files with /dev/null** (`src/sandbox/spec.zig`
  `default_masks`, `src/hub/runtime.zig`): bwrap cannot create a mountpoint on the read-only root,
  so a credential dir that appears later in the owner's home is not masked until the next
  incarnation. The mask list (`default_mask_rels`) is also home-relative guesses; a harness with a
  new credential location needs an entry there.
- **Other roots are hidden only if the hub masks them** (SPEC 3.4 light isolation;
  `src/hub/runtime.zig`): the read-only `/` shows every other workspace, and the hub does not yet
  push other sleeves' claims onto `masks` (not a child's own root, which holds its git dirs; a View
  child needs its root visible). A claim made after a launch stays visible until the next
  incarnation, like the credential masks.
- **The host runtime dir is not hidden** (`src/sandbox/spec.zig`, `src/hub/runtime.zig`): the
  `runtime` mount covers only the sleeve's own tmp dir, not the host `$XDG_RUNTIME_DIR`, and the
  network namespace is shared, so the user bus, keyring, ssh-agent, gpg-agent and pipewire sockets
  and abstract unix sockets (X11 `@/tmp/.X11-unix`, other users' buses) stay reachable from a
  sleeve. Right: a private runtime dir, a network namespace or the own helper.
- **The docker socket is reachable** (SPEC 2, principle 10): the owner's supplementary groups
  survive, there is no user namespace and `/` is bound read-only, so `/run/docker.sock` is root on
  the host from any sleeve; the threat model accepts that. Right: revisit with the threat model.
- **Claude supervised approvals ride a hidden flag** (`src/drivers/claude.zig`, `build_argv`,
  `on_control_request`, `respond_approval`): supervised Claude uses `--permission-prompt-tool stdio`
  (`control_request can_use_tool` in, `control_response allow` or `deny` out), but the flag is only
  named in Claude Code's own help and no e2e covers it (`scripts/zig-e2e.sh` runs real pi only).
  `allow_session` is answered as a plain allow, with no `updatedPermissions`. Right: an e2e that
  runs after every Claude update; if it breaks, supervised Claude becomes allowedTools-only.
- **Approval answers are marked confirmed when written** (`src/drivers/claude.zig`, `codex.zig`,
  `grok.zig`, `respond_approval`): each driver emits `approval_decision` with `confirmed: true` as
  soon as it writes the answer, without any acknowledgement from the harness (Codex sends
  `serverRequest/resolved`, which is not used). Per SPEC 12a item 4 an unacknowledged answer should
  stay Uncertain. Right: take the matching `tool_result` (or its absence at `turn_settled`) or
  Codex's `serverRequest/resolved` as the harness's evidence.
- **Claude prompt ack rides an undocumented echo** (`src/drivers/claude.zig`, `send_user`,
  `on_user`): each stdin message carries a `uuid` we make (`uuid_base` plus a counter), and the
  `isReplay` user message that comes back with it confirms exactly its own message. If Claude stops
  echoing our uuid, no prompt is ever confirmed. Right: an e2e that runs after every Claude update.
- **Untested permission paths** (`src/drivers/codex.zig`, `grok.zig`, `session.zig`): supervised
  Codex (`approvalPolicy untrusted`) and supervised Grok (`session/request_permission`) have
  mappings but no e2e run, and no e2e runs Codex, Grok or Claude at all, to save quota. Pi has no
  gate, so `permission` has no effect on a Pi spawn. Grok decisions never widen: allow-once without
  an `allow_once` option cancels, and deny cancels. Right: one e2e each before supervised sleeves
  ship.
- **Pi extension dialogs are cancelled** (`src/drivers/pi.zig`, `extension_ui_request`): select,
  confirm, input and editor requests get `cancelled: true`. Right: route confirm and select to
  `approval_request` if the owner runs a Pi extension gate.
- **Cost and usage gaps** (`src/drivers/codex.zig`, `grok.zig`, `protocol.zig` `Usage`): only Claude
  (`total_cost_usd`) and Pi report `cost_usd`; the Codex and Grok drivers report no usage at all,
  and `context_window` is never filled. Right: fill from the quota readers (`src/quota/readers.zig`)
  once their units are known.
- **Error classes from text** (`src/drivers/protocol.zig`, `classify_error`): every driver error is
  classified by keyword matching on the harness's message (auth, quota, rate limit, crash, else
  other); structured fields such as Codex `codexErrorInfo` or Claude `api_error_status` are not
  read. Right: use structured fields as each harness exposes them.
- **No driver-side handshake timeout or compaction for some harnesses** (`src/drivers/codex.zig`,
  `grok.zig`, `session.zig` `compact`): a Codex or Grok handshake that never replies leaves the
  session without `session_info`; the core's `start_timeout_ms` (60 s, `src/core/model.zig`) covers
  it. Explicit compaction exists for Claude and Pi only; Codex, Grok and Antigravity return
  `Unsupported`. Right: `compact()` per harness (Codex `thread/compact/start`) when the hub asks for
  it.
- **Pi turn ids are ours** (`src/drivers/pi.zig`, `ensure_turn`, `send_user`): Pi's rpc names no
  turn, so acks and `turn_started` carry `pi-turn-<n>` from a per-session counter, and a steer must
  name the running one. A steer Pi queued just as its run ended stays in Pi's own queue (Pi's
  behavior), so it can surface in the next run. Right: Pi's own turn id if rpc gains one.
- **Linux desktop bundle is built but not released**: `flutter build linux --release` succeeds on
  the dev host (arm64) and was not run beyond that; `scripts/release.sh` builds only the APK and the
  Windows desktop zips. Right: add the Linux bundle to the release.
- **Chat Markdown is a small subset** (`app/lib/chat.dart`): fences (diffs coloured), headings,
  lists, quotes, inline code, bold, italics and links; no tables, no syntax highlighting, links are
  shown as text with the URL in parentheses and are not clickable. Right: tables and clickable links
  when transcripts need them.
- **App tables don't sort** (nodes report 7, rule 8: sort by clicking the header rule): the table
  heads (`TableHead`, `app/lib/widgets.dart`) are static and rows keep a fixed order (`nodeOrder` in
  `app/lib/screens/nodes.dart`). Right: a sort key per column in the shared table rows.
- **F-keys in the web build**: the browser may keep some keys (F12 always opens devtools; F5 and F11
  depend on the browser). Right: check per browser; the sidebar and the command palette (Esc,
  Ctrl+K; `app/lib/shell.dart`) cover every F-key screen meanwhile.
- **Journal retention** (`src/journal/store.zig`): every segment (rotated at 64 MiB,
  `max_segment_bytes`) is kept forever, because nothing prunes them. `write_snapshot` exists but
  nothing calls it, and `replay_into` reads only segments. Right: a retention rule (keep the last N
  snapshots and their segments, archive the rest into backups) once the owner approves deleting old
  segments. The stream log is bounded on purpose (below): its messages are derived output, not the
  record.
- **State records are kept forever** (`src/core/state.zig`, `src/core/apply.zig`): every approval,
  job and rollout stays in the state, and removed sleeves and nodes are only flagged. There is no
  State encoding (`src/core/codec.zig` encodes events only), so the journal is only ever replayed
  from the start of the first segment (`JournalStore.replay_into`). Right: a versioned State codec
  for `JournalStore.write_snapshot`, then pruning terminal records once they are reported and older
  than the snapshot.
- **Hub polls every in-flight job every second** (`src/hub/runtime.zig`, `poll_inflight_jobs`,
  `tick_loop`): each job that is not terminal and has a node gets a status call once a second, with
  no backoff, whatever its lease. A lease that expires (`job_lease_ms`, 5 min, `handle_tick` in
  `src/core/apply.zig`) emits `verify_job`, whose executor (`verify_job_async`) does nothing. Right:
  per-job backoff driven by the lease, and a real verify probe.
- **Linux job fencing falls back to a process group** (`src/sandbox/exec.zig`, `ProcessGuard`,
  `create_job_cgroup`): the node makes one cgroup per job under its own cgroup (`cgroup.kill`,
  `memory.max`). When that cgroup is not writable it uses the process group plus a `/proc` scan for
  `PROTECTORATE_JOB_ID` in each environment, so a job that calls `setsid` and clears its environment
  escapes cancel, timeout and the fence. The unit `install-service` writes (`src/node/service.zig`)
  has no `Delegate=`. Right: the system service's delegated cgroup subtree, one cgroup per job with
  `cgroup.kill`, and no fallback.
- **Job isolation is limits only** (`src/node/runner.zig`): a job gets its timeout, an optional
  `mem_bytes` limit (cgroup `memory.max`, or the Job Object memory limit on Windows) and the process
  guard, and runs as the agent's user with the agent's environment plus the `PROTECTORATE_*` ids.
  The log is not capped (`pump_fn` appends all output). There is no `cpu.max` or `pids.max`, no
  namespaces and no `mode` (sandboxed, plain, admin). Right:
  `docs/design/nodes-ci-deliverables-verus.md` 1.3.
- **No runner account** (`src/node/service.zig`, `src/sandbox/exec.zig`; D11): jobs run as the
  agent's account, so as root for a system systemd unit and LocalSystem for the Windows service made
  by `sc.exe create`, inside a Job Object with kill-on-close. SPEC 2 principle 10 (2026-11-01)
  accepts this, so no low-privilege account is planned. Right: revisit with the threat model.
- **Windows jobs** (`src/sandbox/exec.zig`, `ProcessGuard`): cancel and timeout call
  `TerminateJobObject` at once (no grace period). A node restart ends every running job
  (kill-on-close) and the node keeps no record of them afterwards. The node job e2e scenarios
  (`scripts/zig-e2e.sh`) are Linux only; Windows was checked by hand. Right: a console-control stop
  with a grace period for console jobs, a launch-time marker in the job (exit-code file) to tell a
  restart from a normal end, and the e2e suite ported with `cmd /c` scripts.
- **Presence while a job runs** (`src/core/apply.zig`, `node_presence_updated`): only PRESENT acts,
  and only on running jobs with `on_present` kill, which the hub cancels. An `idle` job is never
  lowered (the node has no priority change on either OS). UNKNOWN changes nothing mid-job and does
  not block a start either (`dispatch_ready_jobs` skips only PRESENT nodes). Right: decide whether a
  detector failing mid-job should lower jobs too, and whether UNKNOWN should block a start.
- **Node jobs have no bound** (`src/node/runner.zig`): every job stays in the node's in-memory table
  and its `jobs/<id>/` directory (log, and the working directory when the job has no `cwd`) stays on
  disk; `ack_job` only releases the lease and deletes nothing. The hub acks after collection
  (`poll_inflight_jobs`), so the growth is the node's own. Right: remove the entry and directory on
  ack after a retention period, and have `doctor` report old uncollected jobs.
- **The sleeve sandbox hides only credentials, not other workspaces** (`src/hub/runtime.zig`,
  `launch_driver`; `src/sandbox/spec.zig`, `build_bwrap_argv`): `/` is bound read-only, the sleeve's
  workspace and git dirs read-write, and only the default credential paths under the host home
  (`default_masks`) are masked. Other sleeves' worktrees and the owner's other repos stay readable
  (not writable). The Landlock fallback (`__guard`) restricts writes only. Right: a deny-by-default
  view of the filesystem (only the claim, its git dirs and the toolchains bound in).
- **Sleeves share the owner's own harness login** (`src/hub/runtime.zig`, `launch_driver`): a
  sleeve's home is the owner's home, and its harness directory (`~/.claude`, `~/.codex`, `~/.grok`,
  `~/.pi` or `~/.gemini`) is bound read-write, so concurrent sleeves and the owner's own CLI may
  refresh the same token at once. That has not been seen to conflict and is not tested against the
  real CLIs. The hub never renews a login: quota (`src/quota/readers.zig`) only reads the files, and
  an idle-expired login shows as expired until some CLI session runs. Right: renewal through each
  CLI's own non-chat command, once one is found.
- **The Envoy token is in bwrap's argv** (`src/hub/runtime.zig`, `launch_driver`;
  `src/sandbox/spec.zig`, `build_bwrap_argv`): Envoy's `PROTECTORATE_TOKEN` goes in as `--setenv`,
  so `/proc/<pid>/cmdline` of the outer bwrap shows it to host processes of the same uid. Right:
  hand the environment over a file descriptor or file through the `__guard` helper
  (`src/sandbox/exec.zig`).
- **Hub Git runs outside the sandbox** (`src/hub/git.zig`): Git still runs as the hub user and
  honors configuration for filters, credential helpers, and transports. A read-only sandbox
  runner or narrower configuration policy would contain those effects.
- **Provision has no completion barrier** (`src/hub/git.zig`, `provision_worktree`): an existing
  worktree directory counts as provisioned, so a directory left by a cut-short `worktree add` is
  reused as it is. If both `worktree add` forms fail in a git repo, the function falls back to
  creating an empty directory instead of failing. Right: a marker written after the layout is whole,
  and a failed provision when git refuses.
- **FetchArtifact buffers the whole file and hashes it once** (`src/node/runner.zig`
  `read_artifact`, `collect_artifacts`; `src/hub/fleet.zig` `fetch_artifact`): the node hashes each
  declared artifact when the job ends; the hub reads 256 KiB slices into memory, compares the
  SHA-256 at the end and writes the file. There is no size limit and no retry, and a file changed
  after collection fails with `ChecksumMismatch`. Only single files are collected, and the path
  check is lexical (no absolute path, no `..`), so a symlink inside the job's directory is followed.
  Right: a bounded archive when jobs return trees, a size limit, and opening without following
  symlinks.
- **The scheduler picks the first eligible node by id** (`src/core/apply.zig`,
  `dispatch_ready_jobs`): approved, online, not removed, not excluded, `jobs_policy` not none,
  presence not PRESENT (unless `even_if_present`), matching target node and arch; the lowest node id
  wins, with no look at load or free slots. It runs on job and node events and on every 10 s Tick. A
  node's own admission queue (`src/node/admission.zig`) holds the excess; there is no fairness
  between requesters. Right: node slots from the inventory and a queue per requester.
- **Hub job logs are mirrored by re-fetching** (`src/hub/runtime.zig`, `poll_inflight_jobs`): while
  a job runs, each poll fetches the first 1 MiB of the node's log from offset 0 and rewrites the
  hub's copy, so `job log` lags up to a second and anything past 1 MiB never reaches the hub.
  `GetJobLog` (`src/hub/api_dispatch.zig`) reads the whole mirror, up to 16 MiB. Right: a log follow
  RPC that pushes appended bytes, fetched by offset.
- **Hub job metadata is memory only** (`src/hub/runtime.zig`, `JobMeta` in `job_meta`): the request
  record (argv, cwd, declared artifacts, limits), the artifact results, the log path and the lease
  ids live only in memory, while the journaled `JobRecord` holds just the state. After a hub restart
  `GetJobLog` and `job show` no longer find the job's data. Right: journal the request and the
  artifact results.
- **Artifact collection is one attempt** (`src/hub/runtime.zig`, `poll_inflight_jobs`): when a
  finished job has an artifact that cannot be fetched, even from a dropped connection, a succeeded
  job is failed with "artifact collection failed" and the hub acks it anyway. There is no command to
  retry the collection. The node keeps the files, since nothing there is deleted. Right: an explicit
  collection retry that keeps the original attempt and never re-runs the job.
- **D9 covers four calls and the turn, not the request chain** (`src/hub/runtime.zig`, `gate_d9`;
  `src/core/apply.zig`): while Envoy's turn is flagged `d9` (the turn was started by agent mail, or
  by mail or a report from a sleeve), Envoy's own `sleeve rm`, `job run`, git push and spawns past
  `d9_spawn_limit` (default 3 per turn) wait for an owner approval (`d9_approval_timeout_secs`,
  default 300, capped at 600; then the approval expires). The flag clears when the turn settles, so
  a background process Envoy started in that turn acts as the owner's afterwards, and a sleeve Envoy
  prompted on the agent's behalf is not gated at all. Outside such a turn no node job waits for the
  owner: a node runs every job with its own account, and SPEC 2 principle 10 accepts that. Right:
  carry the origin through messages Envoy sends (a request chain), and add new pushing calls to the
  list when the API grows them.
- **Vendor agents read the whole Envoy transcript** (`src/hub/api_dispatch.zig`, `EnvoyRead`,
  `extract_envoy_reply`): `EnvoyRead` with the envoy-only token returns the unfiltered Envoy
  transcript, with owner and sleeve messages, tools and reasoning. All agents share the one token,
  so they see each other's exchanges. Each read and each `EnvoyWait` reads the whole transcript
  file. Right: a filtered view of an agent's own exchanges, a token per agent, and an index of agent
  turns if transcripts grow large.
- **Who sent a message is mostly not carried to the harness** (`src/hub/runtime.zig`,
  `notify_parent`, `deliver_prompt`): a child's report reaches its parent as the text prefix `[child
  report from <name>] ...`, since a prompt has no sender field; mail from Envoy and owner messages
  arrive unprefixed, and the transcript's `user_message.from` is always "owner". Right: revisit if a
  harness grows a sender field, and record the real sender in the transcript entry.
- **The brief is a stub** (`src/hub/brief.zig`, `write_handoff`): one markdown file per sleeve with
  the generation, the cause and a fixed one-line summary ("Auto handoff", "Context ceiling reached;
  auto-handoff", "Terminal takeover handoff"). It has no workspace facts, no pre-run history hits,
  no planned-resleeve summary and no checkpoint id.
- **Ticks are journaled** (`src/hub/runtime.zig`, `tick_loop`, `commit`): a Tick event every 10 s is
  appended and fsynced, 8640 frames a day, and replay applies each one. Right: the State codec and
  snapshots (above).
- **Transcripts never rotate** (`src/hub/transcript.zig`): one JSONL per sleeve, and every read
  loads and parses the whole file to find its range. A file over 64 MiB reads as an empty
  transcript. Fast: an offset index.
- **Wait and EnvoyWait hold a connection thread** (`src/hub/api_dispatch.zig`,
  `src/hub/http_server.zig`): each call polls the stream hub for up to 60 s on its own request
  thread. The HTTP server starts one detached thread per connection (`accept_loop`, `conn_thread`)
  with no connection cap and no idle timeout, so idle or slow clients can pile up threads. Fine for
  a few clients. Right: a connection limit that closes the oldest idle connection, a shorter idle
  limit for unauthenticated connections, and an event-driven wait if Envoy fans out.
- **A revoked caller's event stream stays open** (`src/hub/http_server.zig`, `handle_websocket`):
  the token is checked once when `/api/events` is opened. `revoke_device` (`src/hub/home.zig`) stops
  the token working for new requests but does not end streams already open, which keep receiving
  until the client disconnects. Right: re-check the principal on each bus wait, or a revocation
  notice on the bus that closes every stream of that token at once.
- **RealHub is exercised only by `scripts/app-e2e.sh`** (`app/lib/real_hub.dart`): the web build
  against a real hub (pairing, Envoy chat, sleeves, a D9 approval denied from the app), outside
  `make e2e` because it needs Chrome and the local model (`make app-e2e`). Reconnect backoff is 1
  to 30 s; a transcript keeps at most 2000 entries (`_Tx.keep`) with no paging back. Right: add
  paging when a chat scrolls past its first entry, and run the script in CI once a runner has Chrome.
- **Pairing codes are unthrottled and short** (`src/hub/home.zig`, `create_pairing`,
  `claim_pairing`, `random_short_code_alloc`): `AddDevice` makes a ten-minute, single-use code of 8
  characters from a 32-letter alphabet, but only 6 random bytes feed it, so characters 7 and 8
  repeat characters 1 and 2 (about 30 bits). `PairDevice` is public (`src/api/schema.zig`) and has
  no attempt limit. Codes are saved in `auth.json` (mode 0600 off Windows) and survive a restart.
  Device tokens are stored there in plain text and compared in constant time. `last_seen_ms` is
  updated in memory on each request and reaches disk only at the next `auth.json` save. Right: 8
  independent random characters, a per-address attempt limit if the hub is ever reachable beyond the
  owner's networks, hashed device tokens, and a QR code in the app for scan to join.
- **TakeoverTerminal only stops the managed sleeve** (`src/hub/runtime.zig` `takeover_sleeve`,
  `src/cli/commands.zig` `cmd_sleeve_attach`): the hub writes a handoff brief, posts a notice and
  stops the incarnation, and `sleeve attach` then runs the harness's own TUI with its resume flag in
  the caller's terminal. There is no hub-owned PTY and no hand back, and the app's Terminal view
  (`app/lib/chat.dart`, `_TakeoverPlaceholder`) is a placeholder. The method's doc in
  `src/api/schema.zig` still says it answers unavailable. Right: the PTY handoff of SPEC 3.3 and an
  xterm.dart view (build stage 6).
- **Web events token in the URL** (`app/lib/api/transport_web.dart`, `src/hub/http_server.zig`): a
  browser cannot set `Authorization` on a WebSocket, so the web client sends `?access_token=` on
  `/api/events`. The hub accepts `access_token` or `token` in the query of any request, where a
  reverse proxy's access log may record it. Right: a short-lived ticket from a POST, or a
  same-origin cookie for the web build the hub serves.
- **Scopes are coarse** (`src/api/schema.zig` `Scope`, `src/hub/api_dispatch.zig`): owner, sleeve,
  envoy_only or public per method, and the check only tests the kind of caller. `owner` admits the
  owner token and paired devices, `envoy_only` adds Envoy, and `sleeve` admits any authenticated
  caller. There is no per-request subtree rule (own sleeve and children), and `sleeve_tokens` in
  `src/hub/home.zig` is never filled: only Envoy gets `PROTECTORATE_TOKEN` (`src/hub/runtime.zig`),
  so other sleeves are given no token to mail a parent or Envoy. Envoy cannot `Decide` (owner
  scope), though the design lists answering sleeves as Envoy's. Right: decide whether Envoy may
  answer tool approvals, issue per-sleeve tokens with the subtree rule, and add a mail method with
  its own routing rule.
- **Large integers on web** (`app/lib/api/api.g.dart`, `src/tools/apigen.zig` `dart_type_name`):
  u64, i64, u32, i32 and usize fields are all Dart `int`, which on web is a JS double, exact only to
  2^53. Seqs, sizes and epoch ms stay far below; a random 64-bit value would not. Right: send such
  fields as strings if one ever appears.
- **Job log text must be valid UTF-8** (`src/api/schema.zig` `JobLog`, `src/hub/api_dispatch.zig`
  `GetJobLog`): a log page is sliced at a byte offset and limit and returned as the `text` string.
  Zig's JSON writer emits a slice that is not valid UTF-8 as an array of byte values instead of a
  string, which happens for a binary log or a page boundary that splits a multibyte character, and
  the Dart client (`_string` in `api.g.dart`) then rejects the response. The hub reads at most 16
  MiB of a log, and a larger log reads as empty. Right: base64 bytes, or pages cut on character
  boundaries.
- **Torn tail versus corruption** (`src/journal/store.zig` `JournalStore.open`,
  `src/journal/codec.zig` `recovery`): a frame failing its CRC is cut as a torn append only when no
  intact frame follows it (`has_intact_frame_after`); otherwise open fails with `JournalCorrupted`.
  A torn tail in any segment but the last also fails open. The frame length is not covered by the
  CRC, so a corrupted length (zero, too large or past the end) is read as a torn append and cut,
  even with intact frames after it. The segment header is a magic and a version with no checksum.
  Each event is one frame with one fsync, so there is no batch to tell apart. Right: a checksummed
  length and header, so a torn append is told apart from corruption.
- **QR codes never decoded by a scanner** (`src/qr.zig`, `src/cli/commands.zig`): `device add` and
  `node join-token` print the code with `to_terminal_alloc`, and the app has no scanner yet. The
  only check is the DST oracle (`src/dst/oracle.zig`), which asserts a matrix of at least version 1
  and a non-empty terminal render, not that it decodes. Right: decode the terminal output with a
  real scanner (zbar or the app) in an e2e.
- **Linux `uninstall` is not exercised** (`src/node/service.zig`, `cmd_uninstall`): it disables the
  systemd unit by name and deletes the unit file and `install-manifest.json`, but no script in
  `scripts/` runs it and `docs/install.md` records no Linux run, since it deletes files. Right: run
  it on a throwaway VM (or a container with systemd) in an e2e, then reinstall.
- **Windows service through sc.exe** (`src/node/service.zig`, `cmd_install_service`,
  `cmd_uninstall`, `cmd_supervise`): our own dispatcher and control handler (`win_service_main`,
  `win_ctrl_handler`), but create, start, failure actions, stop and delete go through `sc.exe`. A
  stop control ends the supervised `serve` process with `TerminateProcess` (`stop_active_child`); it
  does not wait for running jobs. Install, run and uninstall were checked by hand, not through a hub
  (`docs/install.md`). Right: our own SCM calls if sc.exe handling bites, a reboot test on a spare
  box, and a stop that lets jobs finish or records them as interrupted.
- **Join and install-service do not restrict the Windows state dir** (`src/node/serve.zig`,
  `src/node/service.zig`): no code in `src/` calls icacls or sets a DACL, so the state dir (the
  node's key) keeps its inherited permissions. The default home is
  `%USERPROFILE%\.protectorate-node`, but a home under `%ProgramData%` would inherit read access for
  every user. `docs/windows-node-update.md` says `install-service` limits the dir to SYSTEM and
  Administrators, which the code does not do. Right: `join` and `install-service` set the private
  DACL themselves.
- **No fleet job e2e on a Windows node**: `scripts/zig-e2e.sh` only checks that the Windows hub and
  node binaries cross-build (PE32+), and `scripts/windows-client-update-e2e.ps1` covers the desktop
  client's updater, not node jobs. Right: a fleet job e2e on a Windows node.
- **Node installers are tested by dry run only** (`scripts/install-node.sh`,
  `scripts/install-node.ps1`): they fetch from the GitHub release, check `SHA256SUMS`, stage under
  `<home>/releases/<version>/`, then run `join` and `install-service`. `docs/install.md` records dry
  runs on real machines and `install.sh`'s real path against a scratch home with stand-in
  `systemctl` and `loginctl`; no real node install through an installer is recorded. Right: one real
  install each on a throwaway VM, then on a spare box.
- **Approvals are core state and kept forever** (`src/core/apply.zig`, `src/core/state.zig`): the
  ledger is journaled like every other record and never pruned. The hub never writes a snapshot
  (`write_snapshot` in `src/journal/store.zig` has no caller), so replay reads every segment.
  `allow_session` reaches the driver as given, but only Codex maps it to `acceptForSession`; Claude
  and Grok treat it as allow once. Right: prune terminal approvals once they are older than a
  snapshot, and keep session rules once a driver supports them.
- **A D9 call does not survive a restart** (`src/hub/runtime.zig` `gate_d9`, `src/core/apply.zig`):
  the API call waiting on an action approval holds its request thread (`d9_approval_timeout_secs`,
  300 s by default, at most 600) and dies with the hub. After a restart the approval stays pending
  until its timeout expires it, a decision on it finds no live incarnation and is recorded as
  `fenced`, and nothing runs; the agent must ask again. Right: fine while D9 calls are synchronous;
  a durable action queue if they ever become requests the hub carries out later.
- **An interrupted-turn notice can be lost across a crash** (`src/core/apply.zig`,
  `src/hub/runtime.zig` `commit`): notices are `emit_notice` intents run after the event is
  journaled, and `replay_into` (`src/journal/store.zig`) discards the intents it regenerates. If the
  hub crashes after the journal append but before the intent runs, the notice is never recreated.
  Right: journal notification delivery or re-derive it on recovery.
- **`supervised` is never selected by the hub** (`src/drivers/session.zig`, `src/hub/runtime.zig`):
  the drivers support a supervised permission mode (Claude `--permission-prompt-tool stdio`, Codex
  `untrusted`, Grok and Antigravity flags), but nothing in the hub or the API sets it, so every
  sleeve runs unsupervised. Only the DST oracle exercises Claude's supervised gate. Right: a hub
  setting that starts chosen harnesses supervised, then run Claude, Codex and Grok that way for
  real.
- **Quota readers run on one thread** (`src/hub/runtime.zig` `quota_bg_loop`, `refresh_quotas_sync`;
  `src/quota/readers.zig` `read_all`): one background thread reads every installed harness's quota
  every 120 s (fixed, not configurable) and publishes a `quota` message per row.
  `PROTECTORATE_NO_REMOTE_QUOTA` or `PROTECTORATE_NO_HOST_HISTORY` stops it, and
  `scripts/zig-e2e.sh` sets the latter so no test hub reads the owner's vendor quotas. A stale row
  with no windows keeps the previous windows if they are younger than 24 h and not yet reset.
  Readings are not journaled. Right: fine; persist the last good reading if a restart must keep
  dispatch weighting.
- **A stuck quota read blocks every reader** (`src/quota/readers.zig`): only the Antigravity reader
  has deadlines (5 s to connect, 10 s in all). The Claude, Codex, Grok and local readers call
  `std.http.Client.fetch` with none, and the single refresh thread runs the readers one after
  another, so one hung request stops all quota refreshes. Right: a deadline on every read, or a
  killable helper process if a hung endpoint becomes a practical problem.
- **The MCP bridge has tools only** (`protectorate mcp [--envoy]`, `src/cli/mcp.zig`):
  line-delimited JSON-RPC answering `initialize`, `ping`, `tools/list` and `tools/call`, with a
  fixed protocol version 2024-11-05 (the client's is not negotiated). No resources, prompts or
  notifications (messages without an id are ignored), and the agent name comes from `--as` or
  `clientInfo.name`. Right: grow it with what the vendor apps actually call.
- **Quota readers skip some extras** (`src/quota/readers.zig`): the Codex reset-credit count and
  credits are read, but there are no reset credits for Claude or Grok, no Claude extra-usage overage
  window (only the 5h, weekly and per-model weekly windows), and no Codex `app-server` path (it
  would launch the CLI, which may refresh a token; the wham usage endpoint is the only source).
  Right: add Claude and Grok reset credits when dispatch uses credits; revisit Codex `app-server` if
  wham breaks.
- **Antigravity quota uses an internal API** (`src/quota/readers.zig`): the model and tier endpoints are
  observed CLI interfaces, with no stable public quota contract. The reader treats each named model
  as a separate scope and uses the best available model when none is specified; it cannot infer
  shared pools or the duration of a reset window. Right: use a documented quota interface if agy
  exposes one, including explicit shared buckets and window durations.
- **Zig hub does not yet invoke quota ranking** (`src/hub/runtime.zig`, `src/quota/select.zig`): the
  selector accepts Antigravity readings, but the hub's resleeve path still follows the configured
  target order. Right: connect selection to the journaled dispatch decision without changing the
  pane-only restriction on Antigravity resleeves.
- **Quota selection constants are untuned** (`src/quota/select.zig`): usable-now minimums per task
  size (5, 15, 30 percent, `TaskSize.min_usable_pct`), the 48 h half-life (`urgent_half_hours`), the
  168 h stand-in for an unknown reset (`unknown_reset_hours`), the 10 min stale grace and the 5 h
  unknown window length are fixed constants. Right: tune from real dispatch history once a workspace
  has any.
- **A hub update stages and switches but does not restart or re-exec** (`src/hub/api_dispatch.zig`,
  `UpdateHub`; `src/update/store.zig`, `activate_staged`): the hub writes `releases/current.txt`,
  `previous.txt` and `probation.json` and replies with `restarting` false (the field is never set);
  the running process keeps the old binary until the service manager or the owner restarts it, and
  every sleeve's harness dies with that restart and resumes as after a crash. The new hub's health
  check is `mark_healthy` after journal replay (`src/hub/runtime.zig`) when there is no restore
  error; it does not check the fleet or launch a sleeve. Old `releases/<version>` directories are
  never pruned. Right: restart or re-exec on update with the listeners and sandboxes handed over, a
  health check that includes a fleet round trip, and a `prune` that keeps `current` and `previous`.
- **Probation counts starts, with no watchdog** (`src/update/store.zig`, `check_probation`,
  `mark_healthy`): `activate_staged` writes `probation.json` unhealthy with 0 attempts; the hub unit
  runs `releases/previous/protectorate hub probation` before each start (`scripts/install.sh`) and
  the node supervisor runs `cmd_probation` before each start (`src/node/service.zig`). The first
  start without `mark_healthy` is allowed and counted, and the next one rolls back to `previous`. A
  new binary that starts and then hangs is never restarted, so it stays until the next start. Right:
  a systemd watchdog (`WatchdogSec`) the binaries ping, and a boot-relative deadline.
- **The node supervisor is frozen** (`src/node/service.zig`, `cmd_install_service`,
  `supervise_loop`; D11): the systemd unit, launchd agent or Windows service runs `<installed exe>
  supervise --home <dir>`, and the supervisor starts the release named by `releases/current.txt`,
  restarting it with backoff after any exit. `node update` stages only into `releases/` and never
  replaces the installed supervisor binary, and no remote command does, so a bug in it is fixed only
  by a reinstall. The Windows service path (`win_service_main`) is cross-built but covered by no e2e
  (`scripts/zig-e2e.sh` runs on Linux and only checks that the Windows executables build). Right: a
  manual check of the supervise loop and an update on a spare Windows box, and a supervisor that
  updates itself only if a real need appears.
- **Node releases are state** (`scripts/install-node.sh`, `src/node/service.zig`, `cmd_uninstall`):
  the installer stages the binary in `<home>/releases/<version>/` and the supervisor runs the
  release `current.txt` names. Uninstall removes the unit and `install-manifest.json` only, so every
  staged release stays with the rest of the state dir. Right: fine; a `prune` that keeps `current`
  and `previous` once disks fill.
- **Linux desktop and Android-node assets are not built** (`scripts/release.sh`): the release
  carries `protectorate-android.apk` and `protectorate-desktop-windows-{amd64,arm64}.zip`, but not
  `protectorate-desktop-linux-{amd64,arm64}` or `protectorate-node-android-arm64` (SPEC 6);
  `release-all` in `build.zig` has no Android target. The arm64 Windows build runs as a hub job on
  an arm64 Windows node, so a release waits for that node to be online. Right: the Linux bundles
  (cmake, ninja and GTK 3 on the build host), and the Android node once the node has an Android
  target.
- **Release publishing does not check immutable releases** (`make release-publish`,
  `scripts/release.sh`, `publish`): it stops at a dry run unless `PUBLISH=1`, refuses a dirty tree,
  an untagged HEAD and a tag missing on origin, and uses `gh release create --verify-tag`; GitHub
  immutable releases are neither enabled nor checked. Right: enable immutable releases on the repo
  and have `publish` refuse a repo without them.
- **Installers read GitHub's JSON with grep** (`scripts/install.sh`, `scripts/install-node.sh`,
  `resolve_tag`): with no jq, a channel's newest tag comes from `"tag_name": "..."` matches, ordered
  by `sort -V` with the pre-release dash read as `~`; the API is unauthenticated (60 requests an
  hour per address), as is `check_latest` in `src/update/store.zig`. `--version TAG` skips the
  lookup. Right: fine while GitHub's format holds; an optional token through a curl config file,
  never argv, if rate limits bite.
- **Doctor checks presence, not validity** (`src/doctor/doctor.zig`, `run_checks`): a login counts
  when the harness's credential file or directory exists (`~/.claude/.credentials.json`,
  `~/.codex/auth.json`, `~/.grok/auth.json`, `~/.gemini`, `~/.pi/agent/models.json`); nothing checks
  that it is still valid. Right: a validity check, for example through the read-only quota readers
  (`src/quota/readers.zig`).
- **Harness updates run only on request, one program at a time** (`src/hub/runtime.zig`,
  `update_harnesses`): `harness update` runs `claude update`, `codex update`, `grok update`, `agy
  update` and `pi update self` through the hub, each only while no sleeve on a harness of that
  program is launching, running or recovering; a busy program is reported `deferred` and nothing
  retries it. There is no periodic run (`harness_update_hours` is not read by the hub). Right: a
  periodic run with a ceiling on the deferral (stop accepting turns on that harness) and a retry
  with backoff after a failure.
- **The version stamp comes from the build line** (`Makefile`, `build.zig`, `src/version.zig`):
  `make` takes `git describe --tags --match 'v[0-9]*' --always --dirty` once, when make starts, so a
  `-dirty` mark is the tree's at that moment; a plain `zig build` stamps `0.0.0-dev`. The installers
  check the staged binary's first `--version` line against the tag, and `scripts/release.sh` checks
  the host binaries. Right: fine; releases always set it.
- **Own keyword index instead of SQLite FTS5** (`src/history/segment.zig`, `src/history/index.zig`,
  SPEC 3.7): segments and BM25 are ours, to avoid linking C SQLite. Right if it proves insufficient
  (recall, query features, corruption we cannot explain, or size): SQLite with FTS5, pinned and
  listed in DEPENDENCIES.md.
- **Grok docs carry no time** (`src/history/extract.zig`, `extract_grok_line`): `chat_history.jsonl`
  records have no timestamp and the extractor reads none, so every Grok doc has `time_ms` 0 and time
  filters cannot place Grok docs. Grok context usage is not read at all. Right: the session start
  from `summary.json` `created_at`, then per-record times and usage from the session's
  `updates.jsonl` or `usage.json`.
- **Tool errors by text for Codex, none for Grok** (`src/history/extract.zig`, `extract_codex_line`,
  `extract_grok_line`): a Codex output counts as failed on "Process exited with code" or text
  starting "Script failed"; Grok tool results carry no error flag and are never indexed as errors.
  Right: a structured exit code if either log gains one.
- **Claude sidechain records are skipped** (`src/history/extract.zig`, `extract_claude_line`):
  records with `isSidechain` or `isMeta` are not indexed, so subagent work is not searchable. Right:
  index them with a parent link once a brief or search needs a subagent's work.
- **History has one global cap, not a per-sleeve budget** (`src/history/index.zig`,
  `evict_over_budget`): `max_bytes` is a hard-coded 512 MiB with no config key, and past it the
  oldest whole segments are deleted, whichever sleeve they belong to, keeping at least one segment.
  A busy sleeve can therefore push out the history of the others, and nothing reports that eviction
  happened. Right: a configured per-sleeve budget that evicts a sleeve's own oldest docs and names a
  capped sleeve in `doctor`.
- **Local quota key from Pi's models.json** (`src/quota/readers.zig`, `poll_local`): the first
  provider with a `baseUrl` supplies its `apiKey`, as a set environment variable's name or a
  literal, and it is sent to the first model's `baseUrl` over whatever scheme that URL uses, with no
  loopback restriction; a `!command` key is never run (the local row becomes `stale` with a note).
  Right: fine while both are the owner's endpoints.
- **One account per native harness** (future feature, owner 2026-11-01; not a top priority):
  `schema.Subscription` (`src/api/schema.zig`) is a fixed enum (claude, codex, grok, antigravity,
  local, api), and each quota reader (`src/quota/readers.zig`) reads the one login the hub user's
  home holds per harness. Two Grok accounts, for example, can't be told apart. Right: accounts as
  data. Each one is a named login (harness, a label, and the login home or config dir it lives in,
  such as `CLAUDE_CONFIG_DIR` or `CODEX_HOME` where the harness supports one). The label is probably
  the account's email, read from the login itself (owner's idea; to settle later). There is one
  quota row and reader per account, and the app's QUOTA screen lists every account. `select_target`
  (`src/quota/select.zig`) weighs accounts rather than harnesses, and a sleeve's incarnation records
  the account it draws on.
- **Windows jobs run as the service account** (`src/node/runner.zig`, `src/node/service.zig`):
  `sc.exe create` sets no account, so the service and the jobs it starts run as LocalSystem, with no
  restricted token and no Job Object. The owner first chose (2026-11-01) ordinary jobs as
  LocalService or a restricted token and maintenance jobs as SYSTEM; the owner's threat model (SPEC
  2, principle 10) made that unnecessary. Right: revisit with the threat model.
- **Incremental frame buffering can exceed one frame** (`src/net/frame.zig`, `Decoder.feed`):
  decoded payload length is capped at `max_payload`, but `feed` appends whatever slice a caller
  gives it to an unbounded buffer before `next_frame` parses; `src/net/transport.zig` feeds each
  decrypted message as it arrives. Right: a backpressure API or segmented input that bounds each
  allocation while preserving streams with many frames.
- **Node protocol version is declared but not enforced** (`src/net/msg.zig`, `protocol_version`;
  `src/hub/fleet.zig`): `Hello` carries `protocol` (3) and `HelloReply` an `accepted_protocol`, but
  the hub never compares them and the node never reads the reply's field, so a mismatched node
  connects. Every incompatible wire change must still bump the version. Right: refuse a mismatched
  `Hello`, and detect incompatible message changes without a bump automatically in CI.
- **Release verification is not bound to an inode** (`src/update/store.zig`, `stage_archive_bytes`,
  `activate_staged`): the SHA256SUMS check covers the downloaded bytes before they are written;
  `activate_staged` later switches to a staged `releases/<tag>` directory it only checks exists, so
  staged files can be replaced between staging and the switch. Agents on the owner's machines are
  trusted. Right: bind execution and switching to the verified files if that threat model changes.
- **Sleeves have no systemd user bus** (`src/sandbox/spec.zig`, `build_bwrap_argv`;
  `src/hub/runtime.zig`): under bwrap the sleeve runs with `--clearenv` and gets only the
  environment allowlist plus its own tmpfs `TMPDIR`, so `XDG_RUNTIME_DIR` and
  `DBUS_SESSION_BUS_ADDRESS` are absent and a project test that needs `systemd-run --user` fails
  inside a sleeve. Right: decide whether a sleeve gets its own user manager or a scope-local one, if
  such tests matter.
- **Doctor ignores API-key env vars** (`src/doctor/doctor.zig`): `ANTHROPIC_API_KEY`,
  `OPENAI_API_KEY` and `XAI_API_KEY` do not count as a login, since sleeves never see them (the
  sandbox environment is an allowlist). Right: a deliberate API-key login per harness if API billing
  is wanted for a sleeve.
- **Rate limits are a failed turn, not a guardrail** (`src/drivers/protocol.zig`, `classify_error`;
  `src/hub/runtime.zig`, `on_driver_event`): a 429 or "overloaded" error is classed `rate_limit` and
  recorded as an error entry in the transcript; it does not resleeve or hold the queue, and the next
  message can hit the limit again. Claude's `rate_limit_event` (`src/drivers/claude.zig`) is
  ignored, as are Codex's rate-limit updates. Right: read the reset times to hold the queue until
  the reset.
- **A refused handshake is a kill, not a verdict** (`src/drivers/session.zig`; `codex.zig`,
  `grok.zig`, `pi.zig`): Codex, Grok and Pi set `out.refused` on an error reply to initialize,
  thread/session open or get_state, and the session kills the child, so the exit takes the normal
  unasked-exit path with no exit code.
- **pi_durable removed**: removed on 2026-11-02 before it was built, and the Zig program has no such
  harness. The owner wants to revisit it later; the design is in the git history.
- **App self-update is simple and partly untried** (`src/hub/http_server.zig`, `GET
  /api/client/<name>`; `app/lib/client_update_io.dart`; `MainActivity.kt`): the hub streams the
  whole file with no Range support, so an interrupted download starts over. The Windows script
  copies the new files over the old ones with robocopy, so a copy that fails midway can leave a
  mixed install (the old exe is started again and the error goes to
  `%TEMP%\protectorate-update.log`); files the new build dropped stay. Android reports a failed
  install only as a toast, and the app learns nothing of the outcome. Windows was exercised on the
  Snapdragon laptop on 2026-11-04: detached PowerShell never ran, so the script starts with a normal
  launch and a `.ready` marker, and the downloaded dev.12 archive was installed manually and its
  executable hash verified. Android remains untried. Right: try the phone; then a staged
  side-by-side install with a pointer swap on Windows, and the session status back over the channel
  on Android, if failures show up.
- **Only Claude and Pi compact on request** (`src/drivers/session.zig`, `Session.compact`;
  `src/hub/runtime.zig`, `compact_incarnation`): the Claude driver writes `/compact` and reads
  `compact_boundary` as the compaction; Pi sends its `compact` command. Codex, Grok and Antigravity
  return `Unsupported`, so at the Envoy ceiling the hub writes a handoff brief and resleeves the
  same harness instead of compacting. Right: map each harness's own compaction, checked against the
  real CLI.
- **A compaction that never reports stays Compacting** (`src/drivers/claude.zig`): `compacting` is
  set by `compact` and cleared only by a `compact_boundary` system message, and `send_user` and
  `compact` return `Busy` while it is set; there is no failure notice and no timeout. Right: a
  G6-like timeout for Compacting.
- **The Envoy context ceiling counts the last model call's prompt** (`src/drivers/claude.zig`,
  `src/core/apply.zig`): Claude's input plus cache reads and writes of the turn's last assistant
  message become `context_tokens`, checked against `envoy_context_ceiling` (500k,
  `src/core/model.zig`) at each settle. A turn that grows past 500k mid-turn compacts only after it
  settles, unless the harness's own auto-compaction runs first. Right: fine while the harness
  auto-compacts below its window.
- **Background node rollouts** (`src/hub/runtime.zig`, `send_node_update_async`): `node update --all
  --no-wait` runs one detached thread per node; the rollout and its results are journaled, but
  nothing resumes in-flight nodes after a hub restart. Right: persist rollout progress and recover
  in-flight nodes after restart.
- **Sleeve process fencing without a delegated cgroup** (`src/sandbox/exec.zig`): the fallback finds
  the process group and processes carrying the sleeve tag. A descendant that starts a new group and
  removes its tag can escape that fallback; inherited stdout or stderr can also delay observing an
  unexpected parent exit. Right: use an isolated, delegated cgroup or equivalent process container
  for every incarnation and monitor the parent independently of its pipes.
- **Losing fleet dials can remain blocked** (`src/net/transport.zig`): a timed-out or losing dial
  retains its worker and process-lifetime I/O until the connection attempt or handshake returns.
  Right: cancel DNS, connect, and handshake operations and wait for each losing worker to exit.
- **The events WebSocket and CORS accept any Origin** (`src/hub/http_server.zig`, `conn_thread`
  for `/api/events`, `cors_headers`): the upgrade checks only the token (`Authorization` or
  `?access_token=`), and every response, errors included, carries `access-control-allow-origin: *`.
  With bearer tokens and no cookies a foreign page cannot ride a session, so this is low risk
  today. Right: an allow-list of the hub's own origin and the proxy's host, checked on the upgrade
  and in the CORS headers, before any cookie or ambient credential is added.
- **History and transcripts ignore the caller's reach** (`src/hub/api_dispatch.zig`,
  `GetTranscript`, `SearchHistory`, `ReadHistory`; `src/api/schema.zig`, scope `.sleeve`): these
  methods are open to every authenticated caller, the envoy-only token included, and none checks
  that the caller may reach the sleeve it names; `SearchHistory` takes `sleeves` only as a
  filter. SPEC 3.7 and D26 limit a sleeve to its own docs, with a parent reading a child's
  transcript, and Envoy and the owner unrestricted. This is the history side of "Scopes are
  coarse". Right: issue per-sleeve credentials, derive the caller's sleeve from them, and apply the
  reach rule inside these three methods before anything is read.
- **An artifact refusal excludes the node** (`src/hub/runtime.zig`, `poll_inflight_jobs`;
  `src/core/apply.zig` `.job_failed`): a job whose artifact is over the cap or fails collection
  commits `job_failed`, and the existing rule then keeps that node out of placement for a while,
  as for any failure. Right: decide whether a collection failure should count against the node.
- **The job log mirror re-reads from offset 0** (`src/hub/runtime.zig`, `fetch_log_mirror`):
  every poll pulls the first 1 MiB of a running job's log again in 256 KiB pieces. Right: append
  from the last offset.
- **HTTP connection refusals are counted but not shown** (`src/net/limits.zig`, `ConnGate`): the
  HTTP gates log one `std.debug.print` line per burst; their `refused_total` and high-water marks
  are not in `hub info` (the fleet's `fleet_refused` is). Right: expose them with the network
  budget (Phase 5 of docs/design/tigerstyle-network.md).
- **Git sleeve mounts require bubblewrap** (`src/sandbox/`): Landlock grants write access to
  whole trees and cannot exclude protected Git paths from the workspace grant. A Git checkout
  is refused when Landlock is the only Linux sandbox available.
- **The hub has no stdio acceptor** (`src/net/transport.zig`, `accept_stdio`): `protectorate-node
  serve --stdio` speaks the fleet protocol over a pipe for an ssh bridge, but nothing on the hub
  calls `accept_stdio`. Right: a hub command that runs the node over ssh and attaches the pipe,
  or drop the node mode.
- **Quota-weighted selection is not wired** (`src/quota/select.zig`, `select_target`): the hub
  reads quotas and shows them, but resleeve and spawn never call the selector. Right: call it
  where a harness is chosen, as SPEC describes.
- **The hub fleet starts only on Linux** (`src/hub/fleet.zig` `start`, `src/hub/fleet_loop.zig`): the
  loop runs on `src/net/io_epoll.zig`; Windows and macOS hub builds compile but `start` returns
  `error.FleetUnsupportedPlatform`, so `serve` cannot run there. Right: the kqueue and IOCP backends
  of Phase 3 behind one comptime selector.
- **Lease waits can hold every HTTP handler** (`src/hub/fleet_loop.zig` `long_calls_per_peer`,
  `src/hub/http_server.zig`): waiting lease acquisitions are capped at 8 per node and 64 call slots
  in all, but nothing relates that to the HTTP server's 32 connections, so a few nodes' waiters can
  fill them. Right: HTTP on the loop with continuations (Phase 4), so a waiting call holds no thread.
- **Fleet telemetry is dropped when the event ring is full** (`src/hub/fleet_loop.zig`, 32 events):
  counted in `hub info` as `fleet_telemetry_dropped`; connected and disconnected events wait in the
  bus instead. Right: coalesce telemetry per node (keep only the newest) so nothing is lost.
- **Peer snapshot ordering relies on one publisher** (`src/hub/fleet.zig` `publish_peers`,
  `publish_mu`): serialized publication stands in for a version number. Right: a version if a
  second publisher ever appears.
- **Revoked sessions close quietly** (`src/net/fleet_bus.zig` `set_allowed`): a session whose key
  left the allowed set closes with no disconnect event, which is right for `node_removed` but would
  hide any other reason a key leaves. Right: an event with the reason.
- **Fleet disconnect detail is lost** (`src/hub/fleet_loop.zig`): the disconnect reason is always
  "closed" (it was the error name), node ids longer than 64 bytes cannot connect, names are cut at
  128 bytes and a hello is at most 64 KiB. Right: carry the bus's terminal reason.
- **Fleet reply buffers come from the page allocator** (`src/hub/fleet.zig`): each call maps a
  1 MiB reply buffer after its slot is admitted (at most 64 at once). Right: the fixed pool of
  Phase 5, with the network `StaticAllocator` flipped to static.
- **`fetch_artifact` creates its temporary before checking the node** (`src/hub/fleet.zig`): an
  offline node costs a create-then-abort of `<dest>.ptpart`. Right: check presence first.
- **Zig 0.17.1 leaks a child when exec fails** (`src/child_process.zig`): `std.process.spawn` on POSIX
  forks, learns of a setup or exec error over its pipe and returns the error without waiting for the
  forked PID, so every failed launch (doctor probing a harness that is not on PATH, a missing pi)
  left a zombie; the live dev.4 hub had 92 after five hours. The hub's own spawn wrapper now reaps
  the failed child before returning. Right: report it upstream and drop the wrapper once std reaps it.
