# Rust migration

The Go binary remains the production hub, node agent, and CLI until the Rust
binary satisfies the existing behavior and runtime tests. Flutter remains the
client UI during this phase. The Android app is outside this migration phase.

The Linux sleeve and shared-node sandbox replacement is scoped from actual
workloads and recent host incidents in [rust-sandbox.md](rust-sandbox.md).

## Network

The hand-written tailnet (Headscale supervision, tsnet-style member, DERP,
WireGuard, userspace TCP; about 15,000 lines and an external Headscale binary)
is gone. The owner's routers provide site-to-site and road-warrior WireGuard;
Protectorate binds private addresses only and authenticates every peer with
certificates from the hub CA. Every hub and node is installed fresh: nodes and
devices pair again under this model, and no Go or tailnet state is imported.
See [install.md](install.md#network-model).

## Current Rust crates

| Crate | Go package | Status |
|---|---|---|
| `protectorate-auth` | `internal/auth/scope.go` | Scope validation and grant decisions |
| `protectorate-identity` | `internal/auth/store.go`, `device.go` | Token and device lifecycle ported; owner TokenService, task admin grants for the owner and an opt-in orchestrator on named shared nodes, scoped hub admission and DeviceService wired on loopback and authenticated TLS listeners. Real-binary device authorization covers pairing, approval, denial, CLI login and live event-stream revocation; rate-limit tests cover local and global caps, pending capacity across restart and a real window reset. All task-grant calls, including successful and denied read-only calls, are audited; file uploads record the first and final successful chunks and all failed chunks. Complete audit coverage remains pending |
| `protectorate-fsutil` | `internal/fsutil` | Atomic replacement and Windows retry helper |
| `protectorate-backlog` | `internal/backlog` | Notes persistence and CRUD ported; wired to Rust RPC hub |
| `protectorate-directory` | `internal/directory` | Workspace discovery, graph and refs ported; ListProjects and GetProject wired to Rust hub. Startup, periodic, spawn and kill scans update project owners; the periodic scan also reconciles unexpected exits. The scan now baselines refs, announces new tags and fast-forward default-branch changes to dependent projects, sends idempotent notices to live sleeves, files deduplicated backlog notes for idle dependents, and emits `project.released`. A real Git fixture covers baseline, notice, note, repeat scans and restart; a remote and manifest scan has an HTTP test |
| `protectorate-mailbox` | `internal/mailbox` | Durable message log and all seven MessageService RPCs wired and tested over HTTP, including replay, acting-as, idempotency and owner proxy inbox. Live sleeve validation, idle chat delivery, terminal reminders, escalation and restart reconciliation have HTTP tests; broader harness and long-running behavior need validation |
| `protectorate-audit` | `internal/auth/audit.go` | Audit append, rotation, filtering and pagination ported; owner ListAuditEntries and mutation completion/denial recording wired to the Rust hub. Node file downloads record an actor and node without file paths or contents; successful uploads record the first and final chunks, while intermediate successful chunks are omitted; terminal WebSocket attach records one outcome per attempt without tokens or frames. Public device authorization start is attributed to an unauthenticated actor, and polling does not create audit entries. Handler permission failures are recorded as errors; admission denials remain denied. A wire rejection observer records malformed timeout, content type, encoding, body and decode failures for known mutating procedures without headers or body data. Partial body deadlines and disconnects produce one outcome; process death before an audit write cannot |
| `protectorate-session` | `internal/session` | Tmux session lifecycle, capture and attach command ported; five RPCs wired to Rust hub and tested over HTTP. The hub bridges terminal WebSocket frames to tmux through a PTY; a private-socket test covers scoped authentication, read-only input suppression, writable shell input and reaping of detached tmux clients. A poller emits created, exited and removed events, including external sessions, and honors configurable dead retention; a real event-stream test covers these transitions |
| `protectorate-chatlog` | `internal/chatlog` | Replay, text coalescing and gap-free live watches ported; Pi ChatService Watch streams its normalized events and rejects a sleeve token watching another sleeve's chat. Exited sleeves can append and replay retained reports; the real-binary sleeve report scenario passes, including its MCP view |
| `protectorate-pi-driver` | Pi chat harness | Sandboxed duplex Pi RPC process with live state, send, interrupt and durable events. A disposable Rust hub completed an authenticated Pi model turn through Spawn, Wait and Kill; longer running workloads still need validation |
| `protectorate-claude-driver` | Claude chat harness | Sandboxed Claude Code stream-json process with supervised approval, send, interrupt, compaction, event logging and resume; fake-protocol HTTP tests pass. A real native sandbox trial spawned the installed Claude binary, settled a turn and reaped the child on shutdown. The vendor rejected both the sandboxed turn and a direct official CLI turn because the local OAuth session had expired, so authenticated model inference remains unverified |
| `protectorate-codex-driver` | Codex chat harness | Sandboxed Codex app-server process and protocol adapter with HTTP fake-protocol tests. A disposable Rust hub completed two authenticated turns with the current installed Codex binary, recorded text and usage, returned to idle, and reaped the child on shutdown. Default executable discovery also selected the current NVM installation ahead of an older `/usr/bin/codex` |
| `protectorate-grok-driver` | Grok chat harness | ACP stdio process adapter and hub chat sleeve with permission gate, approval, usage, replay and resume; fake-peer sandbox HTTP tests pass. A disposable Rust hub completed two authenticated Grok turns in the native sandbox and reported idle state with usage. A second live trial verified that normal guarded hub shutdown reaps the Grok child and sandbox helper |
| `protectorate-handoff` | `internal/handoff` | Transcript reading, bounded fact rendering and confined file handling ported. Resleeve runs a bounded Pi summary in a disposable sandbox; failure retains the Git and transcript fact handoff |
| `protectorate-svcgen` | `internal/svcgen` | Systemd, Windows and Termux service definitions are available through public `protectorate service systemd`, `windows-task`, `install-windows` and `termux` commands; generated hub units invoke the Rust server, file output does not overwrite existing definitions, and Termux `--install` writes only into a validated Termux prefix and home. The Android app is outside this rewrite |
| `protectorate-lease` | `internal/hub/leases.go` | Persistent leases, expiration and priority wait queue ported; all four LeaseService RPCs wired to the Rust hub and tested over HTTP. Queued grants send an idempotent mailbox notice to the waiting sleeve. Default selection requires a fresh node poll |
| `protectorate-quota` | `internal/quota`, `internal/hub/suggest.go` | Provider reads, cache and harness ranking ported; GetQuota and Suggest wired to the hub with startup and periodic refresh. A real Rust hub now passes the closed-stdout Codex peer cleanup E2E: quota is published only after its process is reaped. Windows quota readers now launch suspended into a kill-on-close Job Object; a native officewin test confirms cleanup terminates a grandchild that inherited stdout. |
| `protectorate-backup` | `internal/backup` | Verified Rust-native archive and restore flow includes identity, sleeves, job records/logs and published job artifact trees, desk policy/use state, archived handoffs and durable member enrollment keys; transient staging, homes, workspaces and harness credentials stay excluded. Backup refuses a snapshot when a retained job record references an artifact absent from the archive. Restore tests reopen the sleeve registry, desk policy, job record and artifact bytes through a real hub; a crate round-trip verifies archived job, handoff and member files. Rust CLI has `hub backup`/`hub restore`, and `serve --backup-dir` schedules snapshots. Broader installed-service recovery remains unverified |
| `protectorate-tls` | replaces the tailnet | Hub CA created on first hub start (key mode 0600, in backups); per-start hub certificate for `--tls-listen` addresses and `--tls-name` names; node certificates for a node-generated P-256 key at join, named `ID.NAME.node.protectorate.internal`; node listener requiring a hub-CA client certificate naming `hub.protectorate.internal`; in-place certificate renewal; the private-address listener rule. Real-binary tests cover join, mutual TLS, foreign-CA and node-certificate refusal, renewal, removal and refusal of public or unspecified listeners |
| `protectorate-node-metrics` | `internal/node/collector.go` | Linux CPU, memory, host and filesystem collection, GPU/NPU sysfs inventory, NVIDIA, AMD ROCm and Intel sysfs/xpu-smi gauges, bounded watched directory sizes and Docker Engine metrics. The hub's local node response now includes visible running sleeve records from its registry, including usage sampled by the sleeve runtime. Native Windows CPU, memory/page-file, host/uptime and logical-drive collection is wired into node startup; configured Docker TCP or local named-pipe endpoints and bounded NVIDIA `nvidia-smi` gauges are supported. Crate tests, an isolated Windows target check, and a native officewin `node metrics --local` trial pass: CPU count, physical memory, uptime, and primary-disk values agreed with Windows counters; Docker and NVIDIA were unavailable on that host. ROCm/Intel gauges have fixture tests but no matching hardware on this host; xe DRM ioctl VRAM fallback and broader Windows GPU/NPU inventory pending |
| `protectorate-inventory` | `internal/hub/inventory.go` | Persistent node records, tier changes, single-use join codes (stored as SHA-256 with expiry) and the current certificate id, expiry and public key per node; NodeService inventory, InviteNode, JoinNode, WatchNodeSetup and RemoveNode wired and tested over HTTP and real binaries. Removal drains new jobs and leases, waits for or cancels active jobs, releases leases and removes the record, which revokes the node certificate. The hub regenerates `hosts.md` from current inventory on startup and successful commits |
| `protectorate-node-monitor` | `internal/hub/nodes.go` | Bounded concurrent polls, freshness cache and node status wired to NodeService and lease admission. Its dialer reaches joined nodes over mutual TLS, checking the certificate name of the current join, and unjoined loopback agents (tests) over plain HTTP. GPU history marks immediate VRAM use and sustained load or heat in `busyGpus`; explicit GPU and whole-node leases refuse busy discrete GPUs |
| `protectorate-node` | `internal/node/agent.go`, `files.go` | Authenticated metrics, file transfer and job RPCs over Connect JSON. Sandboxed jobs stream output, replay by offset, cancel descendants, prepare/upload bounded sources, stage Git workspaces and collect artifacts with aggregate disk limits. Windows jobs now use a Job Object for descendant cleanup and limits; an explicitly provisioned local runner account executes shared sandboxed jobs from a separate ACL-protected root. Plain jobs remain an explicit care-node option. Native officewin process-tree tests verify Job Object descendant termination and CPU/memory limits. A disposable direct node reported sandbox:none and rejected an unconfigured shared job. An interactive officewin trial provisioned the runner, ran a sandboxed job as the separate account in its workspace, confirmed denial of node-token access, and canceled a parent and child process; it exposed and fixed an overlong account name and a Windows verbatim-path working directory. Explicit admin jobs run only when the node reports a working elevation path. Finished jobs survive restart; unfinished jobs reconcile as outcome unknown. Verified Linux ApplyUpdate has a real different-version 0.1.0 to 0.1.1 trial in an isolated systemd user unit; a failing 0.1.2 candidate rolled back while the node guardian remained the service main process. A disposable Rust hub, CLI, and systemd node completed hub-mediated binary transfer, guardian confirmation, repeat skip, and both different-version and forced same-version rollback; the CLI streamed progress and returned an error for each rollback. Installer-enrolled Linux updates now stage a verified versioned bundle, keep the prior release immutable, and confirm the new current pointer through the node guardian; shell and native install layouts passed isolated systemd trials. Hosted release delivery remains unverified. Authenticated Linux Uninstall disables the user service after its reply, exits cleanly and leaves the unit, binary and state in place; local `node uninstall --dry-run` previews this. Linux DeskAction/GetDeskState return authenticated explicit unsupported errors; a native Windows desktop runtime passed an isolated interactive read-only officewin trial; input remains unverified |
| `protectorate-desk-node` | Windows node desktop runtime | Native user32/GDI screenshot, window inspection and input actions behind persisted node policy; physical-input monitoring ignores injected events, and a five-second timer applies lock decisions without RPC traffic. A read-only local state inspection feeds `node doctor`. Linux reports unsupported. Linux tests, native Windows desk tests and strict Clippy pass. An isolated officewin InteractiveToken task verified console readiness, policy denial and later availability after physical idle, PNG screenshot dimensions, window enumeration and foreground integrity; no desktop input was sent. Live input and lock behavior remain unverified |
| `protectorate-desk-policy` | `internal/desk/policy.go`, `keys.go` | IANA time-zone access windows, idle/lock decisions and Windows key chords ported. Hub SetDeskPolicy/GetDeskPolicy validate and persist owner-only policy writes, show active desk lease holders and daily input counts, ask reachable Windows nodes for live state, and report explicit unsupported or unknown state when no desktop runtime answers. GetNode/ListNodes now include cached desk state, current holder and daily inputs for the Flutter node and desk screens; bounded background refresh and HTTP tests cover the projection. Hub DeskService forwards eight actions through node routes with exact-resource leases, two-minute auto leases, policy checks and HTTP dispatch tests. Rust Windows desktop input and locking still need a real workstation test |
| `protectorate-sleeve-layout` | `internal/sleeve/layout.go` | Per-harness private home directories, seeds, auth links and environment for Claude, Codex, Grok and Pi; fd-relative no-follow writes resist concurrent symlink swaps and home paths are confined to the sleeve namespace. Chat and terminal launches receive private Go, Pub, Cargo and rustup caches |
| `protectorate-sleeve-registry` | `internal/sleeve/sleeve.go`, `mode.go` | Durable sleeve lifecycle records with mode, harness, parent, nested limits, grants, chat/result and exit metadata; ListSleeves/GetSleeve wired and tested over HTTP. Background workspace, home and cache disk samples populate sleeve status. Shell terminal and Pi/Claude/Codex/Grok chat Spawn/Kill wired. Local branch and Git URL dispatch create fresh managed clones and remove them after failed launch; source Git config, reflogs and fetch metadata are hidden while `git fetch base` works. Base metadata survives respawn and resleeve. Owner SpawnEnvoy and guarded PruneSleeves have HTTP tests. Resleeve preserves the task workspace and history, rotates identity, writes a bounded fact and Pi summary handoff, and rolls back a failed target launch; source branch advance notices now queue one idle mailbox delivery with a recorded new base tip; hub-owned sanitized Git views support packed refs and SHA256 fetches while hiding source config, reflogs and fetch metadata. New-branch clones check out tracked files with a clean index. Owner `sleeve prune --workspaces` now deletes only stopped hub-managed clones after a bounded, isolated Git verification of clean tracked, untracked and ignored work and recoverability of every local object from an independent origin; eight real HTTP tests cover SHA1/SHA256, dirty and unpublished work, local origins and cross-sleeve references. Long-running branch behavior remains to validate |
| `protectorate-sleeve-runner` | `internal/sleeve/launch.go` | Sandboxed shell, Pi, Claude, Codex and Grok terminal runner under tmux with scoped hub token and narrow CLI/MCP companion file binds; public hub HTTP Spawn/Capture/Kill passed on this host. A real-sandbox terminal trial ran the actual Rust `protectorate mcp --list` and listed tools. Fake TUI integration covers all four agent harnesses, per-provider credential isolation, terminal arguments and Kill; real vendor TUI interaction remains unverified. Terminal identity revokes on Kill or a background exit reconciliation pass. Fresh chat and terminal homes seed a login profile so `bash -lc` retains the validated sleeve PATH. Narrow read-only tool roots and published Flutter SDK cache lowers feed private per-sleeve overlays; live Flutter, Dart, Pub and `flutter create --no-pub` probes passed |
| `protectorate-sandbox` | `internal/sleeve/plan.go`, `layout.go` | Typed plans plus direct Linux launcher; real host tests cover mounts, masks, secrets, SDK PATH, cancellation, parent death, a sibling-module build, private Flutter cache overlay copy-up, and a nested child sandbox under delegated cgroup controls. Wired to chat and terminal sleeves, read-only Git handoff helpers and node jobs. Chat sleeves now accept an explicit nested profile and preserve it through resleeve; a live fake-Pi-to-Codex HTTP test passes. A combined public-hub sleeve test now verifies dependent-module compilation, nested delegation, secret denial, overlay copy-up, bounded systemd scope and cleanup after Kill. The test exposed host staging left behind when tmux killed the runner; the launcher now removes that staging directory after the helper reports ready. Long-running harness workloads remain |
| `protectorate-release` | `internal/release/source.go` | Bounded network check/fetch, checksum verification, staged publish, safe web extraction and Unix symlink or Windows file pointers implemented. Rust server, sleeve runner/helper, node helper and CLI are fetched as a matched checksummed bundle without requiring Go executable assets. Hub CheckRelease and both same-version and different-version UpdateHub paths have HTTP tests. The initial-launch hub guardian keeps the service main PID, trials a candidate on the same listener, confirms its GetStatus version, and restores the prior release on failure; a disposable real process test covers handoff and rollback. The hub serves the current staged Linux Rust bundle components and checksums at bounded public `/dl` routes after verifying SHA256SUMS. Rust-first Linux hub and node installers verify the full bundle, preserve private tokens, stage systemd user units and keep Rust state separate from Go state; temporary-directory installer tests pass. A PowerShell installer verifies the pinned Windows Rust asset manifest, enrolls a user node, registers an interactive logon task, and offers an explicit care-node plain-job option. A dedicated Windows node-only Rust binary supplies join, serve, doctor and runner provisioning without pulling in the Unix hub; Windows CI runs a native installer verification after building it. An isolated native Windows trial on officewin built the node, CLI and MCP bridge, enrolled a Rust node with disposable Headscale, verified the logon task and doctor, replaced the process on reinstall, and confirmed uninstall removed both task and process. Installer fixture and native MCP tool listing pass; an isolated officewin shared-runner job and cancellation trial passed, while hosted release installation remains unverified. Release workflows gate Rust Linux amd64/arm64 bundles, Windows amd64/arm64 server/CLI/MCP bridge, Android arm64 Termux node, Flutter desktop/web, exact asset names and checksums; local package validation passes, while hosted Actions and publication remain unverified; a real compiled 0.1.0 to 0.1.1 hub trial and failed-candidate rollback passed in isolation. Windows node update has a separate installed guardian executable and a native officewin trial confirmed hub-mediated same-version replacement and failing-candidate rollback; installed-task update and hosted-release delivery remain to validate. Android and app updates remain |
| `protectorate-xfer` | `internal/xfer` | Bounded chunked file and directory transfer, optional zstd, SHA-256 verification, atomic Linux publish and Windows overwrite fallback; node RPCs and direct CLI push/pull wired; isolated native Windows Connect transfer test passed file and directory round trips, chunk crossing, overwrite refusal, and atomic download failure cases |
| `protectorate-connect-client` | CLI and hub HTTP calls | Bounded descriptor validated Connect JSON unary and server streaming client; raw API call and basic owner CLI commands wired. A no-token client is restricted to public device Start/Poll, and unary calls can retain bounded desktop lease headers for CLI notices. HTTPS clients can trust a private CA certificate from `PROTECTORATE_TLS_CA_CERT` with normal server-name verification; terminal sleeves receive the public certificate through a bounded environment value. Hub-to-node calls use mutual TLS with the hub certificate, the hub CA as the only root and the node certificate name of its current join, dialing the inventory address |
| `protectorate-mcpbridge` | `internal/mcpbridge` | Public `protectorate mcp` CLI forwards to a packaged stdio bridge for all 67 unary hub tools; its names and schemas match the Go catalog, including proto field descriptions. Read-only default filtering and comma-separated allow/deny patterns, hub-token auth, bounded JSON-RPC frames and concurrent tool calls are implemented. Modern stdio subscriptions acknowledge, permit other calls, and finish on cancellation. The public CLI resolves saved device and local Rust owner credentials. Streamable HTTP serves modern stateless requests and request-scoped subscription streams, with legacy sessions for older clients. The HTTP endpoint validates Origin and protocol headers when supplied, uses bearer auth, binds plain HTTP to loopback, and offers bounded HTTPS for remote clients with a private TLS key. Windows validates the key owner and DACL and rejects reparse paths; a native Windows TLS and bearer-auth trial passed. Real-binary MCP session, cancellation, catalog and HTTP tool scenarios pass. Cancellation now shuts down each call's isolated connection pool; a 320-call stalled-server churn test confirms sockets close and new calls proceed. DNS lookup and TCP connect already in progress remain bounded by the client timeout |
| `protectorate-cli` | `cmd/protectorate` | Owner status, quota usage and task-size suggestions, node list/show/invite/put/get, backlog list/show/add/done/reopen/edit/rm/pin, lease list/acquire/renew/release, grant admin, audit list/tail, token create/list/revoke/scopes, devices list/pending/approve/deny/revoke/rename, job list/show/cancel/run/logs, chat send/wait/watch/respond/interrupt/compact/models/settings, report, who and events, message send/inbox/reply/ack/thread/threads/wait, and sleeve list/workspaces/browse/show/kill/spawn/envoy/resleeve/prune/attach, and session list/new/kill/respawn/capture/attach over the Rust Connect API and terminal WebSocket; text and JSON output. Real HTTP tests cover quota refresh and suggestions, token, lease, grant, audit, backlog and chat control flows, including settings read and selection request shapes; real PTY-to-CLI and private tmux tests cover attach and detach. Report, who and event streaming have real HTTP dispatch tests. Session commands pass a real hub/private tmux lifecycle test; terminal attach has live PTY and WebSocket tests for writable input, read-only suppression, detach and missing-session errors. WSS attach trusts `PROTECTORATE_TLS_CA_PEM` or `PROTECTORATE_TLS_CA_CERT` with the same bounds and precedence as Connect; a private-certificate handshake test passes. Invite issues a single-use key and a Rust registration command. Job run submits by idempotency key, streams and resumes output, reports the remote exit code, and cancels by request ID on Unix SIGINT/SIGTERM or Windows Ctrl-C/Break; a real hub and node Unix integration test passes, and the Windows console-handler build passes natively. A native officewin console trial attached over the Rust hub's terminal WebSocket to a private tmux session: shell output rendered, typed input reached the session, Ctrl-] detached, read-only mode suppressed input, and a missing session returned HTTP 404. A disposable native officewin console trial against a Rust hub and node verified first Ctrl-C cancellation, persisted JOB_STATE_CANCELED and descendant exit; two Ctrl-Break events 10 ms apart exited the CLI with code 130 after requesting cancellation, and the hub again persisted JOB_STATE_CANCELED with the descendant gone. Node file and directory transfers use bounded chunks through the hub, verified atomic downloads, `--force` and `--no-compress`. Owner-only `sleeve kill --drop-caches` removes regenerable private caches. Node add, remove, tier and update commands use the Rust hub APIs with setup/update progress streams. Owner device management passes a real hub HTTP lifecycle test. Public new-device login and saved credential reuse pass eight real-hub tests, including approval, denial, expiry, token precedence and private files; pairing web/app links and terminal QR have focused tests. Desktop actions and policy commands are wired to the hub with request, rendering and parser tests; isolated native Windows read-only execution passed, while desktop input and lock remain unverified. Broader command behavior across supported targets still needs validation |
| `protectorate-pairing` | `internal/hub/pair.go` | Validated browser and app pairing links and terminal QR rendering, used by public `pair` CLI output |
| `protectorate-node-termux` | Termux node executable | An Android AArch64 binary cross-checks and links against NDK r28. It provides `version`, `node join`, and `node serve` over mutual TLS with explicit plain-job opt-in. Local install, doctor and uninstall plus guarded update/rollback are implemented. A real Termux device trial remains. See [Termux build and runtime notes](rust-termux.md) |
| `protectorate-node-client` | `internal/node/push.go`, CLI transfer | Verified file and directory push/pull using the Rust node RPCs; direct-node CLI commands and hub RPC forwarding wired. CLI `node put/get` now routes through the hub with full-chunk HTTP coverage |
| `protectorate-wire` | `proto/protectorate/v1`, `internal/codec` | Strict protobuf JSON and Connect unary/server streaming JSON HTTP transport. The current proto declares no client-streaming or bidirectional RPC, so neither transport is needed for present user-facing parity. All declared hub service RPCs are registered in the Rust hub |
| `protectorate-hub` | `internal/hub/hub.go`, `tokens.go`, `devices.go`, `directory.go`, `backlog.go`, `sessions.go`, `quota.go` | Authenticated hub basics plus shell, Pi, Claude, Codex and Grok sleeves; chat, audit query/write, task grants, lease-target and selector-placed JobService with file, directory and Git source shipping and verified artifact collection, hub file relay, mailbox delivery, release lookup/node update, and SleeveService workspace listing/browsing. `chat models --harness codex` queries Codex's native catalog in a temporary Rust sandbox without a registered sleeve; other harnesses report unsupported offline catalogs. HubService reports the Rust namespace launcher. Admin jobs require a named node or lease, node admin grant and fresh node capability; care jobs run plain. EventService streams heartbeat, note, device, quota, lease and message events. NodeService desk policy RPCs persist owner changes, and DeskService forwards eight actions through exact-resource leases and node-side policy enforcement. Persisted remote jobs reattach by output offset after restart, retry temporary route failures with a refreshed endpoint, and stop retrying after the original timeout plus ten minutes. a Linux systemd user-service trial verifies exact running-job replay after SIGKILL and manager restart. Local jobs resume after their sidecar endpoint is configured |
| `protectorate-server` | `cmd/protectorate` | The hub serves plain HTTP on its loopback `--listen` and HTTPS with its CA on each `--tls-listen`; every listener must be an explicit private address. `node join` creates the node key, redeems the invite code over TLS pinned to the hub CA and stores the certificate; `node serve` serves only mutual TLS to the hub on the joined address and swaps in renewed certificates. Shell, Pi, Claude, Codex and Grok sleeves, workspace browse roots and references, mailbox acknowledgement timeout, opt-in orchestrator admin grants, release source options, sandboxed node jobs, metrics watch/Docker flags, raw API call and backup/restore commands. Hub and node guardians remain service main processes while supervising candidate binaries. `node doctor` checks Linux service startup or the Windows scheduled task, the join state and certificate, and hub reachability over the pinned CA |

The real-binary branch dispatch scenario passes against both backends across
seven branch variants. The Rust hub resumes previously running chat sleeves
after restart with their retained harness session IDs and fresh scoped tokens;
base-branch notices continue after the restart. The sleeve's private Git view
can accept writes inside its mount namespace without changing the source
repository on the host, which the scenario checks before and after sleeve exit.

NodeStatus carries the hub's draining flag and disk and memory threshold
alerts. Focused HTTP tests cover the app-visible fields and drain behavior.
WatchNodeSetup also kicks an individual monitor poll every three seconds after
the node leaves INVITED, so a newly reachable node can move from JOINED to
READY without waiting for the hub's full fleet poll interval. A real HTTP
watch test starts with no cached metrics and reaches READY through that kick.
Successful polls now publish `node.metrics` with the same freshness, alert and
CPU, memory, GPU, container and worst-filesystem summary as Go. The first down
poll publishes `node.unreachable`, and a successful invited-node poll
publishes `node.joined` after fresh metrics are recorded. A live event
subscription test covers these transitions, recovery and payload consistency
with GetNode; Flutter's Nodes and Overview screens consume these events.
The hub now derives node staleness from its configured poll interval (twice
that interval), so a healthy node stays schedulable when operators use slower
polling. Focused server timing and live node-event tests pass.

A separate real-binary trial wrote a backlog note through a serving Rust hub,
created a backup while it was running, restored the archive into a second
state home, and read the same note through the restored hub's CLI. Both trial
listeners exited cleanly. This establishes archive reopen for live-created
state, not a concurrent-write snapshot guarantee or installed-service recovery.
Backups also retain cancellation-before-submit request IDs. A real hub test
cancels an unseen request, backs up while serving, restores into a new state
home, and verifies the reopened hub still rejects a late submit with no job.

Successful `SubmitJob` calls now create one audit entry with the assigned job ID
and mode; failed submissions are still audited. Admin and task-grant command
details are bounded to 512 UTF-8 bytes in that entry.
Successful `ApproveDeviceAuth` and `DenyDeviceAuth` calls now each record one
audit entry with the resolved device name and metadata. Public pairing start
entries record the direct transport IP on success and handler error; pairing
codes remain excluded from audit details.
Device rename and revoke entries use the stable token ID as their subject.
Successful `GrantService/Admin` calls record the canonical node and validated
task without the issued bearer secret. `UpdateTokenScopes` records the resolved
token ID and before/after scopes. Real HTTP tests verify one audit entry for
each success, failure and admission denial.
The Rust audit observer now reads only the selected protobuf fields for
subjects, upload offset and grant task. It no longer serializes entire
request bodies while auditing file chunks; a real hub/node upload test checks
the first, final and failed chunks, omission of successful middle chunks,
and absence of the file path and contents from audit entries. A separate
HTTP test verifies that AddNode keeps its nested node name as the subject.
`UpdateHub` records one entry with the resolved release channel and version,
including the path that signals the guardian to restart.
`SpawnEnvoy` records the resolved harness and whether an existing Envoy was
replaced, with one entry per success, failure or admission denial and no bearer
token in the audit detail; its real HTTP lifecycle test covers all three.
The Envoy token can report its own sleeve result through `ChatService/Report`,
which audits `envoy` as the subject. Owner and cross-sleeve reports remain
denied; the real HTTP Envoy lifecycle test covers those cases.
Stream deadline audit codes now match the Connect trailer when a producer is
aborted, including backpressured streams. A producer that finished before its
deadline keeps its recorded success if the client polls the body later; wire
tests cover both orderings.
The real-binary phone emergency stop scenario now passes with a scoped kill
token: read-only callers are denied, workers stop and lose their identities,
while a delegated sleeve still cannot kill another parent's sleeve after a hub
restart. Real-binary workspace browsing covers root filtering, Git metadata
redaction, bounded listings and identity boundaries. MCP real-binary checks
cover rejected initialization recovery, session capacity and cancellation of
pending calls.
The Envoy watcher refreshes its `hosts.md` and rotates its token when a quarter
of the original lifetime remains. The new token is written atomically, while
the old one works for five minutes to let an in-flight CLI call finish. A real
hub identity and file integration test covers rotation, grace and hosts refresh.
Chat `--exit-after-idle` is stored on the sleeve record. A one-second watcher
reaps only continuously idle chat sessions with no pending mail, excluding
Envoy and sleeves being resleeved; a real hub test covers busy, mail-pending
and eventual exit behavior.
Desk status refresh is started by a node-status or desk-policy request. The
first status reply may precede the remote refresh, with a later reply showing
the cached state. A periodic watcher checks policy nodes every three seconds
and other Windows nodes every thirty seconds. A policy node reporting its desk
unavailable releases its active desktop lease and sends the sleeve holder a
notice. Live lease requests and queued lease grants also check desk
availability; an HTTP test covers the state transition, queued lease and
notice. Live Windows input and lock behavior still need a workstation trial.

The Rust hub accepts `--node-interval`, `--quota-interval`, repeatable
`--local-llm-url`, `--local-model-host`, `--trusted-proxy`, `--public-url`,
`--dead-retention`, `--watch` and
`--tmux-socket` for polling, OpenAI-compatible local model quota status,
Pi model endpoint classification, trusted reverse proxy client IPs for device
pairing, hub directory sizes and sessions. Forwarded client IPs are used only
when the TCP peer matches a configured proxy IP. The configured public URL is
the default origin for node invites and downloaded installer scripts; callers
can still supply an explicit invite URL. When the hub runs from a verified staged
release, `node update` can fetch its matching Android ARM64 asset through the
release source; `--node-binary-dir` still supplies explicit binaries for local
trials. Desktop admission now permits care-tier desks, rejects idle policies
above 24 hours and drops leases when a node refuses an input action. Ordinary
desk lease release, expiry and policy refusal now send bounded best-effort
`leaseEnded` notifications to the node; real hub/node tests cover each path.

Run `make rust-check` for formatting, Clippy and workspace tests. The original
`tests/e2e` suite is a reference for user-visible behavior; Go storage formats
and wire compatibility are not migration requirements. Its `PROT_E2E_BINARY`
setting can point at the Rust CLI when `PROT_E2E_COMPANION_BINARY` names its
sibling server binary. With that opt-in fixture, real Rust binaries pass node
drain/removal, transfer failure atomicity, cancel-before-submit persistence and
job cancellation across hub restart. A Rust-specific real-binary trial now
proves that cancellation of a plain Linux node job reaps a TCP-owning child,
releases its port, and replays CANCELED under the same request ID after hub
restart. A separate live x86 trial used a Rust hub and private-CA TLS node on
`agent-world`: the hub canceled a sandboxed job after the remote shell spawned
a `sleep` child; the stream and persisted job reached CANCELED, and both remote
PIDs exited. The original Go scenario still assumes its child-PID log format;
the opt-in Go-suite fixture still needs a sandbox-descendant scenario.
Node Git source staging keeps the bare object cache and uploaded bundles under
node ownership and runs checkout against job-writable Git metadata under the
job's identity. Git creates source bundles on stdout; the node creates each
destination exclusively, preventing a prior symlink from redirecting its
write. Windows source bundles stay under node ownership with runner read-only
file ACLs. Linux shared jobs perform fetch and checkout inside the native
namespace; plain/admin jobs on a node with that sandbox stage there before
their requested command. Windows shared jobs stage through the dedicated
runner account. On Windows, node-owned job records and plain/admin workspaces
stay under the node profile while sandboxed workspaces live in the runner's
shared root. Git workspaces are separate for sandbox, plain and admin modes.
Windows file and archive uploads stay under node ownership until the runner
copies or validates and extracts them in its workspace.
A Linux node RPC test poisons a warm sandbox workspace with a Git smudge
filter, then proves that the next sandbox checkout executes it without reading
a host-only secret while a plain job sees a separate clean checkout. Tracked
repository scripts and warm reuse work. An isolated native officewin direct
node trial passed ordinary Git commands, warm reuse, the poisoned-filter
check, a clean plain checkout after sandbox poisoning, and file and archive
staging under the dedicated runner. Separate authenticated native trials ran
File and Git source jobs through the supported `protectorate-rs-windows.exe
node serve`. The File job confirmed the dedicated runner identity. The Git
trial uploaded a bundle, reused a warm workspace, and confined a poisoned
smudge filter without creating a node-only marker. Plain Git jobs on nodes
without a staging identity retain their explicitly same-user authority.
The Go real-binary artifact scenario now also passes with the Rust CLI and
server: an uploaded artifact executes `./run` from its staged workdir, returns
the exact binary output with executable permissions, keeps environment values
out of persisted job records, and replays logs after a hub restart. Explicit
relative executables are resolved inside the mounted job workspace before
sandbox launch. The scenario also backs up the live Rust hub, restores it into
a fresh home, restarts the restored hub, and checks the job state, log, exact
binary artifact bytes, and executable permissions. Restore rejects an otherwise
valid Rust archive if a job record references an artifact absent from its
manifest; a focused crate test covers this consistency check.
The Rust owner CLI has a local `version` command and a catalog-validated
authenticated unary `api call` for methods not yet given dedicated commands.
Shell completion fetches registered node names and active lease IDs for
`job run --on` and `job test-cross --on`, and node names for
`lease acquire --node`, including inline `--flag=value` forms.
`node invite --pool` accepts repeated flags and comma-separated pool names;
the Rust CLI sends each name separately through Connect.
The Rust CLI also accepts `orchestrator spawn` as an Envoy spawn alias,
`orchestrator status` to show the Envoy sleeve and latest orchestrator token,
and `orchestrator scopes --add/--remove` to change the newest active Envoy
token by exact ID without respawning it. The command covers the period when
the previous token remains valid after renewal; a real hub CLI test confirms
that only the new token changes. Status includes an empty state before first
spawn.
`chat wait` accepts several sleeve names, deduplicates them, waits concurrently,
and prints each completed result as it arrives. An error from one sleeve no
longer waits for unrelated sleeves to finish.
Chat sends reject whitespace-only text and malformed request IDs before a
harness sees them. Per-sleeve deduplication retains at most 10,000 successful
request IDs for one hour; restart reconstructs live IDs from durable chat
events. Pi HTTP tests cover validation and restart replay. Recovery streams the
append-only chat log once because it has no time index.
`protectorate completion` generates bash, fish, zsh and PowerShell scripts
from the Rust CLI's command catalog. Completion is static and needs updating
when commands or flags change; live resource values are not suggested.

`protectorate service termux` renders the termux-services (runit) run script
and the Termux:Boot script for `node serve` through the shared
`protectorate-svcgen` renderer. It prints both scripts under their target
headings by default, writes them as 0755 files with `--write DIR`
(`DIR/protectorate-node/run` and `DIR/boot/protectorate-node`), and never
replaces an existing file or writes through a symlink on any step below the
target root. `--install` accepts a validated Termux directory layout: `PREFIX` must
be an absolute `.../com.termux/files/usr` with no `.` or `..` step, `HOME` must
be the home directory beside that prefix, and `$PREFIX/bin/sh` must exist, so a
a host without that layout is refused with the failed check and a pointer to
`--write`. Both script destinations are checked before either is written. `--write` and `--install` are mutually exclusive, and the command
enables or starts nothing. Focused tests cover print output, quoted paths,
executable 0755 files, refusal to overwrite or follow a symlink, and each
rejected install environment; a real Termux device trial is still pending.
The retained Flutter client passes `flutter analyze` and a release web build
with the regenerated Dart protocol bindings. A disposable Rust hub served that
build on the API origin: Chromium opened a pairing link with its URL prefilled,
then displayed fresh node metrics; a Dart client completed live approval,
denial, cancellation and single-delivery pairing checks. A separate disposable
Rust hub browser trial now opens a saved hub in Chromium, clicks through the
Flutter approval banner and confirmation dialog, and verifies the pending
request clears and the public poll returns an approved secret. The banner is
not currently exposed in Chromium's accessibility tree after Flutter web
semantics are enabled; the pointer path passes, but screen-reader behavior
needs a navigation-level fix. An isolated trial exposed the banner after
rendering it inside the active nested route, but duplicated its visible UI;
the current layout was restored. Run
`python3 scripts/test-rust-browser-approval.py` after
building the Rust server and web
client; it retains its disposable fixture for inspection.

The installed Rust CLI forwards `node serve`, `hub publish-flutter-cache`,
`hub publish-flutter-tools` and `hub publish-pub-cache` to its matched server
companion. A real hub HTTP test spawned a terminal sleeve that used a published
Pub cache for `protoc-gen-dart` and offline dependency resolution; a chat
sleeve test confirmed private cache writes. With all three cache generations
published, a bounded Rust sleeve ran `flutter create --offline` and built a
Linux Flutter asset bundle. The full Flutter Linux arm64 release bundle also
built successfully in an isolated Debian container using the host SDK and
project; interactive desktop behavior remains untested. See
[rust-sandbox.md](rust-sandbox.md) for the cache setup and limits.

To run the current hub, build `protectorate-rs`, `protectorate-sleeve-runner` and `protectorate-sleeve-sandbox-helper` into the same directory, then pass `serve --home DIR --owner-token-file FILE`. The installed Pi, Claude, Codex and Grok executables are detected at `~/.local/bin/pi`, `~/.local/bin/claude`, `~/.local/bin/codex` (then the newest installed NVM Codex package, a PATH match or `/usr/bin/codex`) and `~/.local/bin/grok`, or set with `--pi-executable FILE`, `--claude-executable FILE`, `--codex-executable FILE` and `--grok-executable FILE`; corresponding `--*-tool-root DIR` flags grant additional read-only SDK roots. Pi auth belongs in the hub state at `auth/pi`. Claude uses a private `.claude` configuration in its sleeve home with a narrow symlink to the configured credential file. Grok's executable downloads get a narrow read-only SDK mount while its account state remains hidden. The hub gives each chat and terminal sleeve a scoped token file and its actual listener URL; terminal tokens revoke on Kill or a background exit reconciliation pass. If `protectorate-cli` is installed beside the server, terminal sleeves see it as `protectorate` on PATH via a single read-only file bind, including when built inside their writable checkout. `--workspace-root DIR` adds browseable roots; an existing `~/github` is added by default. `--reference-dir DIR` lists explicit read-only references. `--msg-ack-timeout DURATION` configures mailbox reminders and escalation (default `30m`). The node agent also needs `protectorate-node-sandbox-helper` beside the server; it joins a hub with `node join --hub URL --ca-cert FILE --code CODE` and then runs `node serve [--home DIR] [--file-root DIR] [--file-unconfined] [--watch-dir DIR] [--docker-host URL]`. The hub's own machine runs no node agent; join it as a node to run jobs there. The hub listens on loopback HTTP (`--listen`, default `127.0.0.1:7470`) and on each `--tls-listen` private address over HTTPS with its CA; sleeves and the guardian use the loopback listener. Set `PROTECTORATE_TLS_CA_CERT=/path/to/ca.pem` for a CLI on another machine. Name verification stays on.
Run `protectorate-cli login claude|codex|grok|pi --home HUB_STATE` on the hub
machine to place a vendor credential in the corresponding shared auth
directory. `login --status --home HUB_STATE` checks credential presence without
printing secrets. Global flags may precede `login`, including `--home DIR`;
`login` without a harness is the separate device pairing
flow. `protectorate-rs serve --web app/build/web` serves a local Flutter build
on the same origin as the API; without `--web`, it serves the verified current
release's web directory when available.
`protectorate-cli hub update [--channel stable|dev] [--version TAG]` streams
verified release progress and polls authenticated status after guardian
handoff, reporting confirmation or rollback.
Set `--allow-plain-jobs` on a care node to permit its explicit plain jobs.

Chat and terminal sleeves discover existing narrow user tool directories for Go, Cargo,
Rustup toolchains, Node, Dart and local binaries at hub startup. The sandbox
policy validates each root before mounting it and keeps credential-bearing
parent directories private. Use `--terminal-tool-root DIR` for an additional
trusted tool directory a sleeve must read while building in its checkout.
An explicit `hub publish-go-mod-cache` command can now publish only the
bounded Go module download cache for reuse through private sleeve overlays;
it never mounts the owner's VCS or build cache. A fresh sleeve built a module
dependent Go program offline from the published lower without manual
parallelism limits. The terminal and chat task ceiling is 4096, matching Go's
working default; the earlier Rust ceiling of 64 caused the cold build to fail.
With an owner `.pub-cache/bin` present, a separate real-server trial started a
terminal sleeve using an opt-in published Pub cache. Inside the sandbox,
`protoc-gen-dart` ran and `dart pub get --offline` resolved a package. A cache
write appeared only in the sleeve's private overlay, leaving the owner cache
and published lower unchanged.

`--orchestrator-admin-grants` explicitly permits the orchestrator envoy to mint
short-lived task grants for named shared nodes; without it only the owner can
mint them. `--release-repo OWNER/REPO`, `--update-channel stable|dev`,
`--release-api-url URL` and `--release-dir DIR` configure release lookup and
updates. On Linux amd64/arm64, UpdateHub stages the verified Rust bundle and
web assets, then ends its stream at `UPDATE_PHASE_RESTARTING`. The guardian
stops the old listener, starts the candidate, checks authenticated
`HubService/GetStatus` for the requested version, and leaves `current` on the
prior release throughout the trial. After health confirmation it activates the
candidate, retains the prior release, clears the trial marker and prunes older
releases. The CLI waits for both the requested version and
`GetStatus.release_confirmed`; seeing a candidate's version alone does not
complete the update. `GetStatus` also reports the configured update channel,
cached available version and release notes URL; a real HTTP test covers a fresh
check and offline cached status. Fetched Rust releases publish the stable
executable names used by the installed systemd unit with the verified,
versioned assets in one staged directory. The released Rust bundle supplies
its own sibling helpers;
the service unit starts the prior executable until guardian confirmation. An
isolated real-binary 0.1.0 to 0.1.1 trial confirmed the candidate on the same
listener with a stable guardian PID. A verified 0.1.2 candidate that exited
with code 19 then rolled back to 0.1.1 and cleared the trial marker.
A disposable installed Rust hub in a real systemd user unit also completed a
0.1.0 to 0.1.1 CLI update and rolled a failing 0.1.2 candidate back to 0.1.1;
the guardian MainPID stayed stable. Hosted-release and live-service updates
remain unverified. Windows node updates through the hub passed a disposable native trial; Windows hub self-update and Android app updates are not enabled.

The public `protectorate mcp` command forwards to its packaged sibling
`protectorate-mcpbridge`, preserving stdio and exit status. It resolves an
explicit token, a saved device credential, or the private local Rust hub owner
token. `--list` prints the tool catalog without contacting the hub. Only
read-only tools are offered by default; `--allow GLOB` selects a broader set
and `--deny GLOB` removes matches. The hub token still enforces its own scopes
on every call. `--http IP:PORT` serves the same tools over bearer-authenticated
Streamable HTTP on loopback; use a scoped token for this
endpoint. Direct invocation of the bridge still requires flag or environment
credentials. Remote clients can use `--https ADDR --tls-cert FILE --tls-key
FILE`; the same bearer guard applies over TLS, and the certificate must be
trusted by the client. HTTPS bounds handshakes and live connections; the key
file must be private. On Windows, the key owner and DACL are checked and
reparse paths are rejected; a native Windows TLS and bearer-auth trial passed.
`--http` remains loopback-only.

File transfers go through the hub (`node put/get`); a node accepts no client
but the hub. The hub polls local metrics and joined nodes every 30 seconds.
Addresses without a port use 7475.

`protectorate-rs node install --hub URL --ca-cert FILE` handles a local Rust node
installation. On Linux it stages the server and sandbox helper together,
joins once, and enables a systemd user unit;
`--dry-run` and fixture rollback tests pass. The Windows node-only binary
installs an interactive logon task with private state and restores a previous
managed task and binary if an update fails. The public Rust CLI forwards
`node install`, `node uninstall`, and the Windows-only `node install-runner` to
its sibling backend. The Windows uninstaller checks that the scheduled task is
ours before stopping and unregistering it, and retains node state and binaries.
An isolated native officewin Connect node transfer test now passes file and
directory upload/download, SHA-256 agreement, 4 MiB chunk crossing, refusal to
overwrite without `--force`, and replacement of existing files and nonempty
directories with force. It also verifies that incomplete or tampered download
streams leave an existing destination intact. The test uses the same Rust
transfer client and node agent as the public commands, with a disposable local
listener and file root.
The release download installers remain the path for fetching and verifying
published bundles before local installation.
An isolated Linux fixture installed the actual local Rust bundle, verified all
six Rust binaries and the generated systemd unit, then served an
authenticated `GetStatus` using that unit's `ExecStart`. Disposable installer
tests cover failed restart, failed first start and failed `daemon-reload` with
restoration of the prior release pointer, unit and service state. A separate
disposable real systemd user-unit trial passed installed CLI update and
rollback. Fetching a hosted release and updating the live service remain
unverified.
On agent-world x86_64, a source-matched bundle passed native build and
checksum staging. Unique transient systemd hub and node units served the
bundled CLI, authenticated node metrics, a fresh hub peer and a successful
remote job; both trial units stopped afterward while the existing Go node
service remained active. A separate first install retained a backlog note
across a transient hub stop/start. Follow-up isolated x86 trials updated the
installed hub and node from 0.1.0 to 0.1.1, rejected a failing 0.1.2 hub
candidate and a failing node candidate, and verified rollback, retained hub
note state and installed companion hashes. These trials used local bundles
rather than a hosted release.
An isolated current-source Linux AArch64 trial now covers both installed node
layouts through the public Rust hub and CLI. The shell installer staged 0.1.0;
`node update` transferred a complete manifest-verified 0.1.1 bundle, kept
`current` on 0.1.0 during the trial, then moved it to 0.1.1 after the guardian
observed the candidate's health. The prior binary's SHA-256 stayed unchanged,
the new binary matched `SHA256SUMS`, and the service MainPID stayed stable. A
0.1.2 candidate that reported its version but exited during startup rolled
back without changing `current`; a second 0.1.2 attempt worked despite the
retained first candidate directory and also rolled back. The same hub then
updated a separate node using the native `node install` layout, moving its
`current` pointer from `release-initial` to a new `release-0.1.1-*` directory
with the same hash and MainPID checks. A separate stopped-service trial
reopened an interrupted candidate marker and marked it rolled back while
serving the old release. The reusable gate is
`scripts/test-rust-installed-node-bundle-update.py`; evidence is retained in
`/tmp/protectorate-installed-node-layout-_0jooc00`. Both trial units stopped.
The release cache now completes a Rust bundle after a prior fetch cached only
one verified binary. It verifies existing files against the release checksums
before adding absent assets and refuses a modified cached binary or web file,
or an extra web file absent from the verified archive, without publishing other
bundle files. The release network suite covers those cases.
The shared release cache also keeps node update assets under architecture-qualified
names. A hub bundle alone receives the unversioned launch names, so fetching an
arm64 node bundle alongside an amd64 hub bundle cannot replace or conflict with
the hub executable. Both fetch orders pass the release network suite.
An isolated arm64-hub to emulated x86_64 installed-node trial fetched both
architectures from one cache and transferred all 122,220,457 bytes through the
public `node update` CLI. Raising the CLI's stream deadline above ten minutes
let the transfer complete. The node then spent more than 60 seconds verifying
the staged bundle under QEMU, and the hub's separate ApplyUpdate RPC deadline
expired with `timeout: global`. The installed pointer stayed on 0.1.0; no
update marker or candidate release was written. Both architecture asset sets
matched their source hashes, and the isolated services stopped. The hub RPC
deadline needs an update before the cross-architecture install can be claimed
as passing; a native x86_64 host trial is also still pending.
An additional isolated installed-bundle trial on agent-world x86_64 used the
source-matched 0.1.1 Rust binaries and a real transient systemd user unit. It
seeded a completed job record, log and artifact into Rust state, created a
backlog note over HTTP, backed up the running hub, restored into fresh state,
and served both the note and job from the restored hub. The artifact and log
matched byte-for-byte after restore and after systemd restarted that restored
hub following a main-process SIGKILL (PID 1086237 to 1086391). The production
Go node service remained active; this trial did not submit a live node job.
The bundle fixture now checks staging space before copying binaries. Its first
agent-world attempt filled `/tmp` before this check was added; the retained
trial directories occupy that tmpfs pending authorized cleanup. Running the
fixture with `TMPDIR` on the host filesystem passed.
An isolated LAN trial also started the Rust hub on a spare non-loopback port
with the existing IP-SAN certificate and completed a certificate-verified,
authenticated HTTPS `GetStatus`. The Go hub remained active. The remote reverse
proxy still uses the Go hub's plaintext upstream path; its HTTPS upstream and
certificate trust configuration could not be inspected over SSH, so production
cutover is not yet validated.

`protectorate-rs hub backup [--home DIR] [--to DIR]` writes a verified archive
and checksum sidecar. `hub restore FILE --home DIR` verifies and restores into
an empty directory. The hub can schedule backups with `serve --backup-dir DIR
[--backup-every 6h]`. The Rust archive format is independent of Go's format.

## Remaining backend work

1. Maintain parity for new service methods and keep Flutter's generated Dart
   client compatible. Add client streaming only if a user-facing RPC needs it.
2. Extend audit coverage to the remaining mutations and denial paths.
3. Validate mailbox coordination under long-running workloads and complete remaining
   sleeve, workspace, chat and MCP behavior.
4. Broaden live backup and installed-service recovery trials. A real-binary artifact job now survives a live hub backup and restored-hub restart with its bytes, mode, state and log intact. Validate admin jobs on each supported platform, broader CLI job commands,
   live desktop input and locking on a Windows workstation, and installed update and release
   handling on supported Linux and Windows targets.
   An installed Rust bundle trial also backs up a live hub, restores into fresh
   state, starts the restored hub from its generated service command and reads
   the same note over authenticated HTTP. An opt-in real systemd user-manager
   phase then starts that restored state as a unique transient unit, SIGKILLs
   only its main process, observes the manager restart with a changed PID and
   reads the retained note again. The debug bundle spent substantial CPU time
   validating release assets on each service start; broader installed-machine
   recovery remains to validate.
5. Validate mutual-TLS joins, address changes and certificate renewal on
   real Windows and Termux nodes across the routers' site-to-site WireGuard.
6. Replace the Go CLI and hub/node binaries, then run the existing real-binary
   end-to-end scenarios against Rust across supported targets.

Existing Go storage bytes are not a compatibility target. User-facing behavior
and Flutter API capability are the parity target. The UI rewrite begins only
after the Go backend replacement is stable.
`node metrics --local` collects this host's metrics without a hub, with optional
watched paths. `job test-cross` runs native `cargo test` on a selected node
through the Rust job transport, from the current committed Git worktree or an
explicit artifact directory. A disposable AArch64 hub and x86_64 node on
agent-world compiled and passed 45 native tests from four Protectorate crates
through both source paths. A warm Git rerun shipped no source and completed in
858 ms. `--artifact-glob` returned a Cargo.lock file whose SHA-256 matched on
the hub and node. This covers Linux cross-architecture execution with a plain
care node; broader platforms and deployment modes remain to validate.

The checked-in Flutter/Dart hub connection scenario now runs against a real
Rust hub binary in `protectorate-server/tests/flutter_client.rs`; it passed
locally with the installed Flutter SDK, including its device authorization and
connection error paths. A second Dart scenario uses the generated client to
create, filter, partially edit, complete, reopen and delete backlog notes on
that hub. CI prepares Flutter and requires these scenarios to run. The actual
Flutter `NoteDraft` controller now passes a real-hub test for debounced
creation, rapid edits, field clearing, pinning, duplicate avoidance and
flushing a pending edit. Widget navigation and broader Flutter screens still
need real-hub validation.

The Go real-binary harness also runs against the Rust CLI, hub and node with
isolated state. Its Flutter connection, generated Termux service, terminal
launcher, artifact confinement, artifact replay, backlog lifecycle, CLI/MCP,
branch dispatch, base notices, job cancellation and idempotency, stream
authorization and revocation, transfer failure atomicity, workspace browsing,
device authorization and limits, phone emergency stop, node credentials,
node drain and removal, sleeve reports, cluster restart/restore, bounded MCP
sessions and pending calls, and quota subprocess cleanup scenarios passed
locally. The Go nested-runtime
and race-report cases require bwrap internals or Go build metadata, so their
Rust equivalents need separate coverage. Artifact selection now skips escaping
symlink matches while preserving safe files, and opens selected Unix paths
relative to directory handles to reject parent replacement during copying.
An October 23 route audit accounted for all 103 proto RPC methods in Rust:
95 literal registrations and eight DeskService registrations through its
dispatch loop. This verifies route presence, not full behavioral parity.
`InviteNode` now includes Windows, Linux and Termux install commands consumed
by Flutter's Add Node dialog. The Termux command uses a Rust-only installer
and the checksummed Android arm64 release asset; missing or tampered assets
return HTTP 503. Focused hub tests cover the command fields, key rotation,
installer and download refusal paths. Flutter analysis passed with the local SDK.
An isolated test also executes the exact Linux and Termux commands returned by
`InviteNode` against checksum-verified assets. The Linux command passed with a
real Rust node binary in dry-run mode; the Termux command passed with a stubbed
ARM64 Termux environment. The Linux installer currently retains its downloaded
staging directory as a cache keyed by release manifest and architecture. It
checks the cached binaries against fresh hub checksums before reuse, so repeat
installs of one release do not download another full copy. A failed partial
download can still leave an unused cache directory.
Four unused direct dependency declarations were removed from the hub, Claude
driver and Pi driver manifests. The external packages remain
transitive dependencies elsewhere in the workspace.
After these changes, `make rust-check` and `make app-check` passed locally. A
parallel workspace run exposed a fake Grok peer test that sent a new prompt
after a text event but before the prior turn became idle; the test now waits
for that turn's settled event and passed ten repeated focused runs.
On Windows, artifact file opens use `FILE_FLAG_OPEN_REPARSE_POINT` and check the
opened handle's final path. An isolated native Windows trial passed ordinary
file copying and rejected both file and parent symlink replacements.
Managed sibling workspaces can now be granted read-only at a validated remapped
path such as `/deps/lib`; a real sandbox build passed with both the remapped and
original sibling paths. CLI `chat watch` and `chat respond` accept flags before
their names; `backlog add`, `backlog edit` and `lease renew` also accept
interspersed flags and positionals. `job logs --follow ID` now streams from a
real hub and node. Denied mutation audit
entries capture bounded decoded target names and IDs while omitting input text
and credentials; malformed or interrupted requests still leave one denial.
Mailbox waits now use async change notifications, so live waits and client
disconnects do not occupy blocking workers. An HTTP test holds 32 concurrent
waits with a one-thread blocking pool, verifies that unrelated blocking work
still proceeds, and confirms that every waiter wakes for the reply. Store tests
also cover cancellation and reply replay after restart.
The hub now sweeps expired device authorization requests at startup and every
minute. A durable-state and HTTP test confirms expired requests are removed
while active requests remain available; persistence errors enter the audit log.

Device login writes private 0700/0600 credential state on Unix. Windows device
and harness credential paths now create and verify protected ACLs and reject
reparse points; isolated Windows-target checks pass. A native officewin Rust
CLI device-login trial against a disposable Rust hub showed a pending request,
then completed after owner approval. A separate CLI process used the saved
credential for authenticated `status`; the same command with an empty profile
failed for lack of a bearer token. The saved credential had a protected ACL
granting only SYSTEM, Administrators, and the current user access. The
disposable hub state and private Windows profile are retained under
`protectorate-win-device-login-20261023l`, with sanitized
`device-login-evidence.json` on officewin. The trial hub and SSH tunnel were
stopped. A native Windows node enrollment and
task lifecycle passed in isolation; harness login on Windows remains
unverified.

An October 23 Rust real-binary run passed 30 applicable Go end-to-end scenarios
against isolated Rust CLI, hub and node processes in 277 seconds. It included
repository Git filter confinement, MCP, device authorization limits, cancellation,
transfer integrity and cluster restart. The Go `sleeves.json` disk-prune fixture
and Go `nodes.json` migration fixture are skipped for Rust because their on-disk
formats are outside the rewrite target; Rust-native prune and policy tests cover
the user-facing behavior. Flutter's checked-in Dart session client now passes
a real Rust hub test for create, list, exited status, pane capture, respawn and
removal. CLI completion now includes MCP options and desktop node/action paths.
Pi found quota JSON fields that Rust emitted at proto3 defaults where Go omits
them; Grok found quota age truncation at the stale boundary. Focused Rust HTTP
and cache tests now verify sparse field presence and fractional duration output.

The Linux Rust CLI now forwards `driver run` to a local server companion that
uses the existing Claude, Codex, Grok and Pi headless protocol adapters with
the caller's native authentication. Fake-peer tests cover Pi event streaming,
Claude approval, interrupt and early exit. Its direct host launch is explicit;
sandboxed sleeves still fail closed. A review found and fixed a process-group
PID reuse race during direct-driver descendant cleanup. On Windows, the shipped
CLI now forwards `driver run` to the node companion; an explicit direct-launch
plan starts the harness in a kill-on-close Job Object. Native officewin build
and tests passed, and a real Pi prompt replied `READY`. A real Claude run
reported expired OAuth and correctly exited nonzero. A native fake-peer test
covers Ctrl-Break and descendant cleanup; live Grok and Codex accounts on
Windows remain untested. Pending mail now reopens an
unexpectedly exited Envoy with its saved chat settings and a one-minute retry
interval; explicit kills and other exited sleeves remain stopped. The HTTP
test reproduced the pending-mail failure before the fix and passes afterward.
Flutter's Nodes tab also passes a real Rust hub widget trial for a care node's
card, unreachable status and detail navigation. Node invitation has its own
widget trial against a real Rust hub with a TLS listener in
`app/tool/node_invite_ui_e2e.dart`: Add Node issued a real join code with
the Linux, Windows and Termux install commands (each pinning the hub CA), the
copy button carried the command shown on screen, `WatchNodeSetup` lit the
INVITED stage with the hub's own message, the hub refused its own name inside
the dialog, the invited node listed as PENDING without an unreachable alert,
and reissue from node detail replaced the code. A phone-width run keeps the
dialog and its QR inside a 411-pixel viewport. A second trial took the code
from the displayed Linux command, joined a real Rust node over TLS pinned to
the hub CA, and started its agent. The widget observed INVITED, JOINED, then IN
SERVICE with every setup lamp lit; the node card showed LIVE with CPU metrics,
and `GetNode` reported fresh metrics and `invited=false`. All three widget
tests and focused Flutter analysis passed. The trial used `node join` and
`node serve` directly, so the displayed installer and a full device install
remain separate checks. The scenario is now a Linux-only real-hub test in
`protectorate-server/tests/flutter_client.rs`, run by the required Rust
workspace check.
The combined Flutter client test passed both real-hub cases locally.
For Windows setup, the Rust watch now waits at FIRST METRICS until the
desktop probe answers. A noninteractive agent reports ATTENTION with logon-task
guidance; an unknown desktop state is shown as a note. A real HTTP watch test
covers the pending, attention and ready transitions.
Rust `node doctor` now treats a noninteractive Windows session or a locked
desktop as a failed check and exits nonzero, matching the Go operator signal.
It reports an unavailable but valid desk policy as information. A focused
classification test and a native noninteractive officewin doctor run pass;
the latter exited 1 with session FAIL and screen not checked. A live locked
interactive desktop remains untested to avoid disrupting the workstation.
Windows desktop text input now plans ordinary characters with the foreground
keyboard layout, sends newline and tab as Return and Tab, and falls back to
Unicode input for characters the layout cannot type. Pure planner tests and
native officewin desk-crate tests, strict Clippy and compile checks pass. Live
editor input and lock remain untested on the occupied workstation.

An isolated real Pi terminal trial launched the official Pi TUI through the
Rust hub's Linux namespace sandbox, sent a prompt through its tmux pane,
observed Pi reply `READY`, and then killed the sleeve. The runner, sandbox
helper and Pi processes all exited. The production Go hub was untouched.
On an isolated Windows x64 workstation task, the Rust node guardian confirmed
a same-version installed-binary replacement and rolled back a candidate that
exited during startup; the restored binary hash matched the original. The
existing Go node task stayed running.
An isolated current-source officewin trial then moved a separate guardian's
node from 0.1.0 to 0.1.1. The running agent reported 0.1.1, the persisted
marker recorded `confirmed` after one start, and the installed binary's SHA-256
matched the 0.1.1 build while the guardian remained the 0.1.0 build. A 0.1.2
candidate that exited during startup rolled back to 0.1.1 with the same
installed hash and a `rolledback` marker. Both trial listeners and processes
stopped; the production Go task stayed running. The private fixture and JSON
evidence are retained at
`C:\Users\hotschmoe\protectorate-win-version-trial-20261023v`.
An additional isolated Rust hub and native officewin node trial exercised the
public `node update` CLI stream for 0.1.0 to 0.1.1. The hub sent all 28,089,344
bytes and reported `UPDATE_PHASE_CONFIRMED`; the running Windows child reported
0.1.1 and its executable SHA-256 matched the verified 0.1.1 candidate. The
persisted marker recorded `confirmed`, 0.1.0 to 0.1.1, after one start. A
scoped-thread desk refresh bug initially panicked the hub node monitor and
left `node show` marked unreachable despite successful direct metrics. A
runtime-handle fix and scoped-thread regression test restored fresh hub status
with 0.1.1 metrics and a desktop-state result. The separate fixture and
progress stream are retained in
`C:\Users\hotschmoe\protectorate-win-hub-update-20261023h` and
`/home/hotschmoe/protectorate-win-hub-update-20261023h`. An OpenSSH SFTP-mode
`scp` copy returned success with only 204,800 of 28,089,344 bytes; that
truncated copy was retained. A legacy `scp -O` copy verified by SHA-256 was
used as the hub candidate. Protectorate's own transfer completed and verified
the full candidate.

An installed scheduled-task trial then exercised the public update path with
a native 0.1.0 node. The Rust hub and CLI transferred a verified 0.1.1 binary
and reported `UPDATE_PHASE_CONFIRMED`; the guardian PID and hash stayed fixed
while the child restarted on the 0.1.1 hash. A separate 0.1.2 candidate exited
with code 23, and the public stream reported `UPDATE_PHASE_ROLLED_BACK`. The
child recovered on the 0.1.1 hash and the hub again reported fresh 0.1.1
metrics. The isolated task is disabled, its processes and listeners are stopped,
and the production Go task remained running. Trial records are retained under
`C:\Users\hotschmoe\protectorate-win-task-update-20261023t` and
`/home/hotschmoe/protectorate-win-task-update-20261023t`.