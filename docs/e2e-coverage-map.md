# End-to-end coverage map

Audited against `38c346d`, the RPC descriptors, the CLI command tree, and
`testing-deletions.md`. `git show 187a012:<path>` was used to recover the
deleted job/request-key, file-transfer, node inventory, desk-policy and
service-generation assertions. This map distinguishes successful behavior
from authorization rejection and help/catalog enumeration. An RPC handler
at 0% can still have its authorization interceptor tested.

Scenario abbreviations below refer to these functions:

| Label | Scenario |
|---|---|
| Cluster | `TestBinaryClusterRestartAndRestore`, including security, lease queue, git, output, diagnostics, transport and update subscenarios |
| CLI/MCP | `TestBinaryCLIAndMCP` |
| Browser | `TestBinaryWorkspaceBrowser` |
| Cancel | `TestBinaryJobCancellationAndIdempotency` |
| Remove | `TestBinaryNodeDrainAndRemoval` |
| Transfer | `TestBinaryTransferFailureAtomicity` |
| State | `TestBinaryStateMigrationAndPolicyRestart` |
| Stream | `TestBinaryStreamAuthorizationAndRevocation` |
| Credentials | `TestBinaryNodeCredentialBoundaries` |
| Service | `TestBinaryGeneratedServiceExecution` |
| Artifact | `TestBinaryArtifactJobAndReplay` |

Every unary hub RPC has a no-scope denial probe in Cluster. Every streaming
hub RPC and every node-agent method has a denial probe in Stream; the latter
uses a hub owner token against the separate node bearer authority. These
probes do not imply functional coverage of an operation. Host scenarios may
skip inside a sleeve. The coverage percentages in `testing.md` measure only
the real binary in `tests/e2e`, excluding the in-package driver/runtime lanes.

## Every RPC

All methods of every service in `proto/protectorate/v1` are listed here.
Methods in the remaining-gap column have denial coverage as described above,
but no successful behavioral scenario unless an optional lane is named.

| Service | Successful behavior and scenario | Remaining functional gaps |
|---|---|---|
| HubService | GetStatus: Cluster, CLI/MCP | None |
| NodeAgentService | GetMetrics: every cluster; PrepareJob, UploadJobData, RunJob, AttachJob, ListJobs: Cluster and Artifact; CancelJob: Cancel, Remove; PutFile, GetFile: Transfer; ApplyUpdate: Cluster; GetDeskState: State (unsupported Linux desktop) | WatchMetrics; DeskAction on a real desktop; Uninstall/service deregistration |
| NodeService | ListNodes, GetNode, AddNode: cluster setup, CLI/MCP, Service; RemoveNode: Remove; PutFile, GetFile: Transfer, Credentials; ApplyUpdate, UpdateNode: Cluster; SetDeskPolicy, GetDeskPolicy: State; SetNodeTier: Cluster, Credentials | InviteNode, WatchNodeSetup (tailnet enrollment); physical desktop enforcement |
| JobService | SubmitJob, GetJob: Cluster, Cancel, Artifact; ListJobs: Artifact; CancelJob: Cancel; WatchJob: CLI/MCP and Artifact through CLI execution/replay | CLI signal cancellation while submission is in flight; lost-node recovery deadline; multiple execution slots; same-repo serialization under contention |
| LeaseService | AcquireLease, RenewLease, ReleaseLease, ListLeases: Cluster, Remove, State | Claim-window expiry and GPU contention need deterministic hardware/time lanes |
| TokenService | CreateToken, UpdateTokenScopes, RevokeToken: Cluster, Stream, Credentials | ListTokens positive listing; last-used flush timing |
| GrantService | Admin: Cluster, Credentials | External node-admin elevation on Windows |
| AuditService | None; Cluster reads persisted audit entries and checks scope-change/redaction effects | ListAuditEntries pagination/filtering |
| EventService | Subscribe: Stream; host `TestChatEndToEnd`, `TestMailEscalation` | Slow event consumers/backpressure |
| MessageService | Send, Inbox, Ack: Cluster; Wait: CLI/MCP cancellation and host `TestMailWakeWaitE2E`; Reply, Thread: host `TestMailReplyKeepsThreadAndAcks`; ListThreads: host `TestMailListThreads` | Delivery/redelivery/ack/escalation host scenarios skip in sleeves |
| DirectoryService | Project routing and dependency discovery: host `TestMailProjectAddressing` | ListProjects, GetProject API projections |
| SessionService | CreateSession, KillSession: Cluster authenticated terminal replay | ListSessions, RespawnSession, CapturePane full lifecycle; bidirectional terminal input/resize |
| SleeveService | BrowseWorkspaces, ListSleeves, GetSleeve: Browser; SpawnSleeve, KillSleeve: Cluster host runtime; Envoy delivery: host `TestEnvoyMailQueue`, `TestEnvoyMailAfterOwner` | SpawnEnvoy RPC, ListWorkspaces; real ResleeveSleeve; spawn branch/brief, result, disk/prune and nested runtime delegated to wave 4 |
| ChatService | Watch, Send, Interrupt, Respond, Compact: host `TestChatEndToEnd`, `TestChatSupervisedPermissionMode`, `TestChatSendRequestIDsSurviveHubRestart` | ListModels, SetSelection hub path (native driver selection has separate process scenarios); wave-4 wait/report |
| BacklogService | None | ListNotes, GetNote, CreateNote, UpdateNote, DeleteNote; note state belongs to w4-notes |
| QuotaService | Provider probing starts in every cluster; no successful API assertion | GetQuota, Suggest; real provider freshness/quota ranking |
| TailnetService | Lower transport/enrollment: optional `TestTailnetEndToEnd`, `TestNodeRestartRecovery`, `TestDERPRelay` | GetStatus hub projection; full binary invitation/client enrollment/service lifecycle |
| DeskService | None on a physical desktop; policy persistence uses NodeService | Screenshot, ListWindows, Foreground, Click, Move, Type, Key, Run (Windows interactive lane) |

## Every CLI command

The baseline `testdata/cli_help.json` tree is walked by CLI/MCP. Help alone
does not count as a behavioral test. Parent/group commands in this table
have help coverage; leaf commands are explicitly divided below. Aliases
share implementations: sleeves/sleeve, sessions/session, jobs/job,
leases/lease and message/msg (message inbox also has an execution assertion).

| Command/group | Behavioral execution | Help-only or remaining gap |
|---|---|---|
| root, help | CLI/MCP global flags, errors and passthrough; help walks the entire tree | Hidden `__complete`, `__completeNoDesc` and shell completion runtime |
| api | None | call |
| audit | Persisted audit checked in Cluster | tail |
| backlog | None | add, edit, ls, pin, rm, show (w4-notes) |
| chat | Native chat HTTP host scenarios exercise the underlying RPCs | compact, interrupt, models, respond, send, settings, watch CLI rendering |
| client | None | invite, register |
| completion | None | bash, fish, powershell, zsh |
| desk | policy set/off: State | policy show; desk's screenshot/windows/foreground/click/move/type/key/run actions |
| driver | Official protocol process scenarios in `internal/driver` | run CLI |
| events | Event stream checked by Stream | events CLI |
| grant | GrantService exercised in Cluster/Credentials | admin CLI |
| hub | serve, backup, restore: Cluster; failed serve/migration: State | tailnet init, tailnet install |
| job | run: CLI/MCP, Cancel, Artifact; cancel: Cancel; logs, ls, show: Artifact | test-cross (CLI/MCP only checks missing arguments); CLI signal/lease-renew failure paths |
| lease | RPC lifecycle in Cluster, State, Remove | acquire (CLI/MCP only rejects invalid TTL), ls, release, renew |
| login | None | login/provider authorization |
| mcp | stdio and authenticated HTTP, filtering/cancellation: CLI/MCP | None for transport; most tool effects are tested through RPC rather than MCP |
| msg | inbox: CLI/MCP | ack, reply, send, thread, threads, wait CLI (host scenarios cover their RPCs) |
| node | add, rm: CLI/MCP, Remove, Credentials; put/get: Cluster, Transfer, Service, Credentials; migrate-tiers: State; serve: every cluster; show: Service; update: Cluster | doctor, install, invite, join, ls, metrics, renew, revoke, tier, uninstall CLI; full tailnet/service lifecycle |
| orchestrator | Host Envoy scenarios exercise the runtime | scopes, spawn, status CLI |
| pair | None | pair |
| quota | No API success assertion | quota, suggest |
| service | termux generates executable script, refuses overwrite, boots a real node with quoted paths, restarts with file persistence: Service | systemd, windows-task, install-windows; actual Android boot/runit registration |
| session | Underlying terminal create/kill and WebSocket replay: Cluster | attach, capture, kill, ls, new, respawn CLI |
| sleeve | browse, ls, show: Browser | attach, kill, resleeve, spawn, workspaces CLI; wave-4 features excluded |
| status | CLI/MCP JSON/global flag assertions | Text rendering |
| tailnet | Optional transport lane | status CLI |
| token | RPC create/scopes/revoke in Cluster | create, ls, revoke, scopes CLI |
| version | Node staged-update version checks: Cluster | Standalone human-readable rendering |
| who | Host project addressing exercises directory internals | who CLI |

## Every MCP tool

The complete 55-tool catalog is enumerated by CLI/MCP over both protocols.
`hub_get_status` is called successfully; `message_wait` is canceled while a
real hub call is pending. Other listed tools have catalog/schema/filter
coverage only through MCP; their RPC behavioral coverage is above. Owner-only
and node-local methods and streaming methods are excluded from the catalog.
CLI/MCP checks that `token_create_token` and `node_add_node` cannot be exposed.

Each prefix below combines with every listed suffix to give an exact tool
name, for example `job_` + `cancel_job` = `job_cancel_job`.

| Prefix | Every suffix |
|---|---|
| backlog_ | create_note, delete_note, get_note, list_notes, update_note |
| chat_ | compact, interrupt, list_models, respond, send, set_selection |
| desk_ | click, foreground, key, list_windows, move, run, screenshot, type |
| directory_ | get_project, list_projects |
| grant_ | admin |
| hub_ | get_status |
| job_ | cancel_job, get_job, list_jobs, submit_job |
| lease_ | acquire_lease, list_leases, release_lease, renew_lease |
| message_ | ack, inbox, list_threads, reply, send, thread, wait |
| node_ | get_desk_policy, get_node, list_nodes, put_file |
| quota_ | get_quota, suggest |
| session_ | capture_pane, kill_session, list_sessions |
| sleeve_ | browse_workspaces, get_sleeve, kill_sleeve, list_sleeves, list_workspaces, resleeve_sleeve, spawn_sleeve |
| tailnet_ | get_status |

`FuzzMCPRequest` checks malformed JSON-RPC envelopes and request identity;
`FuzzMCPArguments` checks protobuf JSON roundtrips for job, transfer and lease
arguments, including oneofs, bytes and 64-bit offsets. Scope parsing has
`FuzzScopeNormalization`.

## Persisted files and directories

Paths are relative to the private hub/node home unless described otherwise.
Opaque provider-owned state and third-party SQLite/tsnet files are grouped
by their owning directory; this is not a claim to cover their internal format.

| State | Scenario/evidence | Remaining gap |
|---|---|---|
| token, tokens.json | Cluster restore/revoke/expiry/scope changes; Stream revocation across restart | Last-used flush and malformed token store |
| audit/audit-YYYY-MM.jsonl | Cluster redaction/scope-change audit | Rotation, pagination and malformed/truncated replay |
| nodes.json, nodes.json.pre-tiers, hosts.md | State migration, exact original backup, refusal to overwrite, tags/tier preservation; Remove persistence | Malformed/duplicate inventory and failed atomic writes |
| leases.json | Cluster queue priority/FIFO/claim across restart; State expiry; Remove held/queued cleanup | Lost claim window, GPU conflicts |
| jobs/request-ids.jsonl | Cluster success dedupe; Cancel tombstones, failed-submit retry, concurrent submissions and canceled restart replay | Hour-long TTL sweep, truncated journal recovery |
| jobs/ID/job.json, job.log, artifacts/ | Cluster restart mid-job/exact output/restore; Artifact executable permissions, binary bytes and secret redaction; Cancel terminal-state persistence | Lost node, partial artifact restart, bounded-log overflow |
| sleeves.json | Browser loads exited record; host Cluster ownership; `TestKillRestoresRecordWhenSaveFails` | Wave-4 schema/runtime changes excluded |
| sleeves/NAME/chat.jsonl, rotated segments and request-id journal | Host `TestChatEndToEnd`, `TestChatSendRequestIDsSurviveHubRestart` | Real-binary transcript replay cannot run in this sleeve |
| sleeves/NAME/env.args; sleeves/NAME/home/.protectorate-token; seeded profiles/config/auth links | Host runtime/ownership/toolchain scenarios; environment/workspace/namespace fuzz targets | Live provider refresh and Windows/Android layouts |
| directory.json | Host project-addressing/dependency scenarios; every cluster opens the store | Restart with advanced refs and interrupted writes |
| mailbox.jsonl | Cluster dedupe/ack/restart; host delivery/thread/escalation scenarios | Torn final append and compaction failures |
| notes.json | Store opened by cluster; no functional mutation here | Backlog CRUD/state delegated to w4-notes |
| desk-policies.json; node desk-policy.json | State set/invalid replacement/denied clear/restart/clear persistence | Physical activity/locked-window revocation |
| desk-use.json | Host `TestDeskEndedNotice` exercises notices, not persisted desk-use reload | Active input accounting across restart |
| handoffs/NAME/*; workspace .protectorate/HANDOFF.md and .git/info/exclude | No live resleeve replacement | Two-harness handoff, jailed git facts, transcript parsing/truncation |
| node-files/* and .protectorate-put-* staging | Transfer corruption/offset/archive refusal, cleanup, directory modes, skipped symlinks, overwrite; Credentials care gates | Transfer TTL sweep and interrupted process during rename |
| node-jobs/repos/*, jobs/* upload.part/upload, runs/* out.log/artifacts | Cluster git/incremental bundles/warm worktrees and restart attachment; Artifact directory/file upload and workdir confinement | Node restart deliberately loses runs; contention and ZIP parser fuzzing |
| binary.update, binary.old, binary.failed, staged candidate | Cluster update checksum/version refusal, confirmed replacement and rollback | Power loss at each swap step, Windows replacement |
| runner.json and Windows DPAPI/account state | No Linux-loopback replacement | Windows runner lifecycle |
| tailnet/config.json, headscale.yaml, policy.json, db.sqlite and WAL/SHM, noise_private.key; node.json and tsnet state; derp_cert.pem, derp_cert_key.pem, derpmap.json | Optional tailnet enrollment/restart/relay; `TestBackupRestoreWithLiveHeadscale` | Full CLI enrollment/revoke/uninstall; these lanes were not run in this sleeve |
| tailnet/apikey, headscale.log and rotations, backup-request.lock, database.lock; backups/*.tar.zst and .sha256 plus MANIFEST.json | Cluster backup corruption/content/path refusal and restore; optional live SQLite backup | Crash during snapshot, key renewal and log rotation; opaque ca/ payload is backed up but has no current producer |
| Generated systemd units, Windows tasks/service wrapper, Termux run/boot scripts | Service executes generated run script via POSIX sh, preserves literal paths, refuses overwrite | Service manager registration/uninstall and boot scripts on their target OS |
| Flutter platform preferences, `hubs` key (profile URLs/tokens) | No active real-hub UI scenario | Pairing persistence, profile replacement/removal and malformed preferences |
| CLI grant `--out` credential files and desktop screenshot outputs | Private token files are consumed by Credentials | Grant file mode/output and screenshot writes on an interactive Windows node |
| auth/ provider stores; harness-native sessions; .cstack/ workspace memory | Host credential-layout and driver session scenarios | Provider login/refresh and external memory format intentionally remain provider/workspace owned |

## Risk ranking and deleted coverage

The selected work closes overlapping behavior gaps, not one test per RPC.
The first four priorities address authorization, destructive actions,
persistence, and bytes crossing a trust boundary.

| Priority / deleted areas | Replacement and remaining risk |
|---|---|
| 1. auth, hub auth/identity/grants/nodeops, node server | Stream adds every streaming gate and node bearer separation; Credentials adds redirect credential confinement and care file gates; Cluster retains owner/grant/scoped denial. Terminal host/readonly distinctions still need bidirectional host scenarios. |
| 2. hub jobs/jobkeys/remove, node process/jobs | Cancel adds dedupe under contention, pre-submit tombstone restart, failed retry, whole-tree kill, terminal idempotency, timeout and capacity recovery. Remove adds refused/aborted drain, scheduling exclusion, cancel/removal and held/queued lease cleanup. Node uninstall remains a host lane. |
| 3. hub nodes/deskpolicy, fsutil, keylog | State adds retired-tier migration and safety-policy persistence, denied/invalid mutations, expiry and clear restart. Atomic-write faults, journal truncation and physical desktop consent remain gaps. |
| 4. node files, xfer, hub jobsource/jobs | Transfer and `FuzzTransferArchive` cover checksum/size/framing failure atomicity, traversal/link refusal and cleanup. Artifact covers ZIP source execution, selected workdir, source/returned modes, secret redaction and restart replay. ZIP parser fuzzing and transfer sweeps remain gaps. |
| 5. mcpbridge, CLI parsers | CLI/MCP retains real stdio/HTTP/filter/cancel behavior; two MCP fuzz targets add malformed envelope and strict argument boundaries. Full CLI mutation surfaces and per-tool effects remain gaps. |
| 6. svcgen, install/update, tailnet/headscaledb/backup | Service replaces text-only generator checks with real generated-script execution/restart. Cluster covers update and backup integrity; optional live tailnet covers transport/SQLite. Actual service registration, enrollment/uninstall and Windows paths remain gaps. |
| 7. mailbox, directory, chatlog, driver, session, orchestrator | Existing host mailbox/chat/runtime and official-protocol subprocess scenarios remain. Real resleeve/native transcript, terminal respawn and overload recovery are not covered by this loopback measurement. |
| 8. backlog, sleeve runtime/layout/selection, handoff | Note state, branch/brief, disk/prune, result/wait, effort and nested runtime are being rebuilt by other sleeves and were excluded. |
| 9. quota/local-model, node accelerator metrics, desk input, Flutter UI and scripts | Live provider ranking, actual hardware/Windows desktop consent, real-hub Flutter flows and cross-OS installers remain explicit gaps. Deleted widget/golden/shape tests are not replacement evidence. |
