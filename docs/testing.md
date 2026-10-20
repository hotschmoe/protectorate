# Testing policy and migration plan

Phase 1 adds coverage and an inventory; it removes no existing tests. Phase 2 requires owner approval of the function manifest below, plus demonstrated replacement coverage. The inventory is a conservative migration proposal, not a claim that every replacement already exists. In particular, a shared scenario title does not prove that scenario checks every invariant in its source tests.

An acceptable regression test crosses a real process or transport boundary and asserts an observable failure that matters: an unauthorized operation is refused, a restarted job runs once and retains its output, a restored token retains its scope, or a sandbox cannot expose another sleeve's credentials. Use the actual binary, hub HTTP handlers, node agent, git, tmux, bwrap, or tailnet. An HTTP server returning canned success responses does not make a UI or service unit test end to end. AI protocol peers may be deterministic test executables speaking the official protocol; do not modify or proxy an installed AI CLI. A real hub with fake driver sessions alone does not validate the driver or sandbox.

Fuzz targets are for parsers and normalization boundaries. They must assert an invariant such as lossless environment encoding or rejection of another sleeve's state, rather than only "does not panic." Run seed corpora on commits and time-bounded mutation campaigns separately. Do not add dependencies for a test framework.

CONVERT means retain the behavior but move its assertions into the named scenario. DELETE means an implementation-shape assertion is unsuitable; the reason column identifies the externally visible coverage. KEEP-E2E is intentionally conservative: workflows already exercising real processes or real hub/node HTTP remain until their replacements demonstrate equivalent protection. TestMain is retained infrastructure. The provisional classifications are generated from explicit keep/delete decisions and scenario mappings; they still require assertion-by-assertion review before removal.

## Coherent binary harness

`tests/e2e/harness_test.go` builds or accepts one real binary and launches it as a hub and independent node on ephemeral IPv4 loopback ports. The node keeps its own jobs and files; restarting the hub does not restart the node. Each child gets an explicit environment with a temporary HOME, temporary PROTECTORATE_HOME, private tmux socket, node bearer, no inherited provider credentials and no proxy. A supplied binary is copied into the private ~/.local/bin so mount planning never binds a shared /tmp directory. Tests never connect to the owner's hub or alter the owner's state. All HTTP calls and subprocesses have contexts and timeouts. SIGTERM shutdown is bounded, escalates to kill and reaps every child. Failure reports retain child logs until test cleanup.

Run from the repository root, using the installed Go toolchain on PATH:

```sh
GOMAXPROCS=2 go build -p 2 -o /tmp/protectorate-test-binary ./cmd/protectorate
PROT_E2E=1 PROT_E2E_BINARY=/tmp/protectorate-test-binary GOMAXPROCS=2 go test -count=1 -p 2 -v ./tests/e2e
GOMAXPROCS=2 go test -p 2 ./internal/sleeve -run '^Fuzz'
GOMAXPROCS=2 go test -p 2 ./internal/sleeve -run '^$' -fuzz '^FuzzWorkspaceBoundary$' -fuzztime=10s -parallel=2
GOMAXPROCS=2 go test -p 2 ./internal/sleeve -run '^$' -fuzz '^FuzzEnvironmentArgumentEncoding$' -fuzztime=10s -parallel=2
```

`PROT_E2E=1` is the commit-lane entry point; without it the binary scenario skips explicitly. Compilation has a separate five-minute budget; the process workflow has a 75-second deadline. Externally supplied binaries must be absolute paths and must be built from the checkout under review. The loopback bind address is reserved then released before child startup; port collisions fail with process logs and can be retried by the runner.

The runtime subscenario checks executables and actually probes the systemd user manager and nested bwrap. Missing tmux/git/bwrap/systemd-run, unavailable namespaces, or no user manager produce explicit skips of that subscenario. The core HTTP/job/file/backup scenario remains runnable inside a sleeve. Privileged or host-only scenarios must be separately gated, not quietly ignored after a failing assertion. The existing tailnet lane remains:

```sh
PROT_TAILNET_IT=1 GOMAXPROCS=2 go test -count=1 -p 2 -v ./internal/tailnet -run 'Test(TailnetEndToEnd|NodeRestartRecovery|DERPRelay|BackupRestoreWithLiveHeadscale)$'
```

That lane needs the pinned headscale binary and working network facilities. It must use private headscale and tsnet state; never borrow the running hub. See its existing integration fixture for prerequisite skips.

## Coverage ledger

Implemented in this phase:

- `TestBinaryClusterRestartAndRestore`: actual hub/node binary processes, scoped read versus token escalation, invalid bearer, revocation, exclusive lease persistence, owner mailbox persistence/deduplication/ack, more than 1 MiB binary file roundtrip, traversal/absolute/symlink transfer refusal, remote command/artifact execution, graceful hub restart mid-job, request-id persistence, exactly-once replayed log fragments, real git bundle updates with warm untracked cache preservation, lease priority/FIFO/claim across restart, offline backup and restore followed by querying the restored hub, corrupt archive rejection and refusal to restore over populated state.
- Its `sleeve isolation` subscenario: actual bwrap/systemd/tmux shell writes through the workspace mount, then rejects state/other-sleeve/credential workspaces. Host prerequisites are probed, never assumed. Its nested, host-only chat check uses a private Claude stream-json executable to verify text folding and idempotent replay through real sandbox/driver/hub HTTP; it is unexecuted here because the user manager is unavailable.
- `FuzzWorkspaceBoundary`: canonical path and ownership/credential-prefix invariants for the mount admission boundary. This does not replace filesystem symlink tests: symlink resolution happens before CheckWorkspace.
- `FuzzScopeNormalization`: successful scope parsing must preserve the requested authority exactly, deduplicate it, remain stable on replay and never admit node_admin grants through ordinary scopes.
- `FuzzConfinedFilePaths`: actual filesystem symlink and missing-ancestor confinement invariants for node transfer paths.
- `FuzzEnvironmentArgumentEncoding`: lossless NUL-separated bwrap argument encoding of valid process environments, including hostile shell metacharacters and non-ASCII bytes.

Existing retained integration workflows provide real git bundle/warm worktree execution, process-tree cancellation, timeouts, HTTP streaming, tmux sessions, actual sandbox resource limits, and optional tailnet/headscale/DERP recovery. Their current package fixture costs remain in the baseline; retaining them does not meet the final runtime goal by itself.

Outstanding replacement assertions, which block the corresponding CONVERT removals:

| Scenario | Required assertions beyond the implemented workflow |
|---|---|
| AUTH | scope grammar and expiry, grant ownership, parent/child sleeve rights, audit redaction and deny-by-default across all methods |
| SLEEVE | resolved symlink escapes, credential binary chains, toolchains, git identity, OOM, concurrent spawns, failed-start cleanup and mount permissions |
| MAIL / LEASE | authenticated sleeve sender, idle-only delivery, reply threading, request defaults, redelivery/escalation, Envoy wakeup, project routing, grant notifications, claim expiry and lease expiry after downtime |
| JOB | bundle byte/prerequisite invariants, cancel before submit, stream reconnect offsets, care-tier/admin refusal, malicious zip/artifact paths and node concurrency |
| NODE | real staged binary replacement, health confirmation/rollback, installer checksum failure, isolated service registration/uninstall and directory archive traversal |
| CHAT / HANDOFF | official-protocol fake subprocess peers for each provider, live versus persisted folding, disconnect/resume, approvals, busy interruption, native compaction, selection, two-harness handoff |
| APP | real-hub Flutter transport and UI flows, including Dart ChatTranscript live/replay folding and offline reconnect; Flutter is unavailable in this sleeve |
| BACKUP | live SQLite/headscale snapshot, corrupt/truncated/duplicate/path-traversal archives, manifest tampering and refusal to overwrite nonempty state |
| TERMINAL / DESK | authenticated WebSocket replay, host-terminal rights, desktop consent and revocation with real display facilities |
| QUOTA / MCP / WEB / BACKLOG | binary transports with local protocol peers; freshness/errors, tools, pairing, install/download handlers and concurrent persisted notes |

These are explicit remaining work, not implicit coverage from one broad workflow. The initial request to cover every CONVERT item and make all old fixture failures disappear cannot be claimed complete in this phase while those assertions and fixture migrations remain outstanding.

## Runtime and sleeve limitations

Owner baseline: 515 Go Test/Fuzz/Example functions, roughly 24k Go test lines plus 6.4k Dart test lines; race suite 277s with hub 184s. These are supplied baseline measurements, not reproduced race numbers. New inventory measurements use `go test -json` without race and record function elapsed times, including explicit skip/fail status. Cold compilation is excluded from the 90-second execution goal.

Measured here on two cores, with warm caches: the initial non-race suite had a 145.50s JSON event span and six failures (five CLI fixtures inherited this sleeve's scoped token-file environment; systemd verification lacked a runtime directory). New TestMain setup clears inherited CLI transport credentials and supplies a private runtime directory for offline systemd-analyze verification. The complete non-race suite with PROT_E2E=1 then passed with a 59.57s event span, hub 44.43s. The final expanded binary scenario passed separately in 4.61s; the whole-suite measurement preceded that small assertion expansion. Time before the first JSON event is excluded; concurrent package builds can still contribute to the initial baseline span. The successful rerun used warm caches. All four five-second fuzz campaigns passed (48,939 workspace, 78,216 environment, 9,260 confinement and 8,523 scope executions).

The owner's 277s race baseline remains the only race-suite measurement; do not compare it directly with this non-race run. Phase 2's target is less than 90s on two cores with a warm binary/cache, excluding optional tailnet/desktop/host lanes. Expected before/after: approximately 60s non-race before deletion, at most 90s after migration (target, not a measured deletion result); race before 277s supplied, race after unmeasured. Do not add function elapsed times to estimate wall time: package concurrency and shared setup matter.

Known sandbox failures from the dispatch were TestCaps, TestCareTierEnforcement, TestUpdateNodeStreamsProgress, TestAdminJobs, TestArtifactDirectory* and TestFileTransferThroughHub. Those failures did not reproduce in this run. No existing test source was changed or filtered. Existing host-dependent scenarios still need prerequisite review during migration; the new runtime subscenario explicitly skips because this sleeve has no systemd user manager. Never suppress failures with a blanket test filter or package-wide TestMain escape.

Regenerate this document's inventory and exact function manifest after source merges:

```sh
GOMAXPROCS=2 go test -p 2 -json ./... > /tmp/protectorate-tests.json
python3 scripts/test-inventory.py --go-results /tmp/protectorate-tests.json
```

The generator reads test sources and writes only `docs/testing.md`; its policy preamble is maintained in `docs/testing-policy.md`. Unavailable Dart runtimes remain explicitly unmeasured. Source locations identify registered Dart test/testWidgets callbacks rather than just `main()`.


## Inventory

Snapshot counts: CONVERT=640, DELETE=10, KEEP-E2E=48, KEEP-FUZZ=4; total=702. TestMain rows are infrastructure, not scenarios.

Runtime is the observed non-race test-function elapsed time, including skips. It excludes build time and is not a race-suite prediction. Unknown means unmeasured, never zero. Dart callback labels include source lines to distinguish repeated labels.

| File | Function / callback | Exercises | Runtime | Verdict | Replacement / reason |
|---|---|---|---|---|---|
| cmd/protectorate/backlog_test.go | TestBacklogCLI | Backlog CLI | 0.120s pass | CONVERT | BACKLOG: authenticated CRUD, persistence and concurrent updates; preserve the named behavior as a scenario assertion |
| cmd/protectorate/chat_test.go | TestChatRenderer | Chat Renderer | 0.000s pass | CONVERT | CHAT: official protocol subprocesses, approval, interruption, compaction and replay; preserve the named behavior as a scenario assertion |
| cmd/protectorate/desk_policy_test.go | TestDeskStatusLabel | Desk Status Label | 0.000s pass | CONVERT | DESK: isolated node desktop consent, lease, revocation and input fencing; preserve the named behavior as a scenario assertion |
| cmd/protectorate/desk_policy_test.go | TestPolicyCheck | Policy Check | 0.000s pass | CONVERT | DESK: isolated node desktop consent, lease, revocation and input fencing; preserve the named behavior as a scenario assertion |
| cmd/protectorate/desk_test.go | TestParseDeskArgs | Parse Desk Args | 0.000s pass | CONVERT | DESK: isolated node desktop consent, lease, revocation and input fencing; preserve the named behavior as a scenario assertion |
| cmd/protectorate/desk_test.go | TestPutGetRoundTrip | Put Get Round Trip | 0.250s pass | CONVERT | DESK: isolated node desktop consent, lease, revocation and input fencing; preserve the named behavior as a scenario assertion |
| cmd/protectorate/e2e_environment_test.go | TestMain | Main | not measured | KEEP-E2E | runner infrastructure; retain while any package tests use it |
| cmd/protectorate/hub_backup_test.go | TestHubBackupAndRestoreCommands | Hub Backup And Restore Commands | 0.000s pass | CONVERT | CLI: real binary API commands, JSON output, cancellation and error exit codes; preserve the named behavior as a scenario assertion |
| cmd/protectorate/install_hub_test.go | TestInstallHub | Install Hub | 1.360s pass | CONVERT | WEB: binary serves SPA, bootstrap, pairing, install scripts and downloads; preserve the named behavior as a scenario assertion |
| cmd/protectorate/job_test.go | TestParseTarget | Parse Target | 0.000s pass | CONVERT | JOB: real agent git/artifact execution, cancel, idempotency and hub restart; preserve the named behavior as a scenario assertion |
| cmd/protectorate/job_test.go | TestParseGitSource | Parse Git Source | 0.000s pass | CONVERT | JOB: real agent git/artifact execution, cancel, idempotency and hub restart; preserve the named behavior as a scenario assertion |
| cmd/protectorate/job_test.go | TestParseEnv | Parse Env | 0.000s pass | CONVERT | JOB: real agent git/artifact execution, cancel, idempotency and hub restart; preserve the named behavior as a scenario assertion |
| cmd/protectorate/job_test.go | TestJobExitStatus | Job Exit Status | 0.000s pass | CONVERT | JOB: real agent git/artifact execution, cancel, idempotency and hub restart; preserve the named behavior as a scenario assertion |
| cmd/protectorate/job_test.go | TestKeepLease | Keep Lease | 0.000s pass | CONVERT | LEASE: exclusivity, FIFO, expiry/renewal and restart; preserve the named behavior as a scenario assertion |
| cmd/protectorate/job_test.go | TestTestCrossReleasesLease | Test Cross Releases Lease | 1.840s pass | CONVERT | LEASE: exclusivity, FIFO, expiry/renewal and restart; preserve the named behavior as a scenario assertion |
| cmd/protectorate/job_test.go | TestTestCrossLeaseLifecycle | Test Cross Lease Lifecycle | 3.940s pass | CONVERT | LEASE: exclusivity, FIFO, expiry/renewal and restart; preserve the named behavior as a scenario assertion |
| cmd/protectorate/job_test.go | TestFollowJobResumesAcrossHubRestart | Follow Job Resumes Across Hub Restart | 3.010s pass | CONVERT | JOB: real agent git/artifact execution, cancel, idempotency and hub restart; preserve the named behavior as a scenario assertion |
| cmd/protectorate/main_test.go | TestCLI | CLI | 0.040s pass | CONVERT | CLI: real binary API commands, JSON output, cancellation and error exit codes; preserve the named behavior as a scenario assertion |
| cmd/protectorate/main_test.go | TestWriteQuota | Write Quota | 0.000s pass | CONVERT | QUOTA: hub polls local provider HTTP fixtures and exposes freshness/errors; preserve the named behavior as a scenario assertion |
| cmd/protectorate/main_test.go | TestWriteSuggest | Write Suggest | 0.000s pass | CONVERT | CLI: real binary API commands, JSON output, cancellation and error exit codes; preserve the named behavior as a scenario assertion |
| cmd/protectorate/main_test.go | TestLoginStatus | Login Status | 0.010s pass | CONVERT | CLI: real binary API commands, JSON output, cancellation and error exit codes; preserve the named behavior as a scenario assertion |
| cmd/protectorate/main_test.go | TestInstalledDir | Installed Dir | 0.000s pass | CONVERT | WEB: binary serves SPA, bootstrap, pairing, install scripts and downloads; preserve the named behavior as a scenario assertion |
| cmd/protectorate/mcp_test.go | TestCheckMCPListen | Check MCPListen | 0.000s pass | CONVERT | MCP: binary stdio handshake, tools, scopes and streamed errors through hub; preserve the named behavior as a scenario assertion |
| cmd/protectorate/msg_test.go | TestWriteMail | Write Mail | 0.000s pass | CONVERT | MAIL: authenticated send/reply/ack, idle delivery, FIFO notices and restart; preserve the named behavior as a scenario assertion |
| cmd/protectorate/node_install_test.go | TestPlanInstall | Plan Install | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| cmd/protectorate/node_install_test.go | TestWindowsTaskHasNoShellWrapper | Windows Task Has No Shell Wrapper | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| cmd/protectorate/node_install_test.go | TestPlanUninstallDeletesNothing | Plan Uninstall Deletes Nothing | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| cmd/protectorate/node_install_test.go | TestResolveKey | Resolve Key | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| cmd/protectorate/node_install_test.go | TestInstallDryRunChangesNothing | Install Dry Run Changes Nothing | 0.060s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| cmd/protectorate/node_install_test.go | TestInstallRunner | Install Runner | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| cmd/protectorate/node_install_test.go | TestDoctor | Doctor | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| cmd/protectorate/node_install_test.go | TestDoctorParsers | Doctor Parsers | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| cmd/protectorate/node_install_test.go | TestUpdateHelpers | Update Helpers | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| cmd/protectorate/node_install_test.go | TestLingerNote | Linger Note | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| cmd/protectorate/node_test.go | TestNodeCLI | Node CLI | 1.010s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| cmd/protectorate/node_test.go | TestWriteNodeList | Write Node List | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| cmd/protectorate/node_test.go | TestGpuSummary | Gpu Summary | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| cmd/protectorate/node_test.go | TestWriteMetricsAccelerators | Write Metrics Accelerators | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| cmd/protectorate/node_test.go | TestFmtBytes | Fmt Bytes | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| cmd/protectorate/node_test.go | TestNodeToken | Node Token | 0.000s pass | CONVERT | AUTH: scoped bearer, denial, revoke, expiry, audit and restart; preserve the named behavior as a scenario assertion |
| cmd/protectorate/node_test.go | TestNoSecretFlags | No Secret Flags | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| cmd/protectorate/node_test.go | TestNodeMigrateTiers | Node Migrate Tiers | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| cmd/protectorate/node_update_test.go | TestUpdateAllSkipsUnreachable | Update All Skips Unreachable | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| cmd/protectorate/orchestrator_test.go | TestCommandDocsAndWorkflow | Command Docs And Workflow | 0.000s pass | CONVERT | CLI: real binary API commands, JSON output, cancellation and error exit codes; preserve the named behavior as a scenario assertion |
| cmd/protectorate/orchestrator_test.go | TestToolDocsMatchCatalog | Tool Docs Match Catalog | 0.010s pass | CONVERT | CLI: real binary API commands, JSON output, cancellation and error exit codes; preserve the named behavior as a scenario assertion |
| cmd/protectorate/orchestrator_test.go | TestTokenFileEnv | Token File Env | 0.000s pass | CONVERT | AUTH: scoped bearer, denial, revoke, expiry, audit and restart; preserve the named behavior as a scenario assertion |
| cmd/protectorate/orchestrator_test.go | TestTokenFileIsReadOnEveryCall | Token File Is Read On Every Call | 0.000s pass | CONVERT | AUTH: scoped bearer, denial, revoke, expiry, audit and restart; preserve the named behavior as a scenario assertion |
| cmd/protectorate/service_test.go | TestServiceWriteNoOverwrite | Service Write No Overwrite | 0.000s pass | CONVERT | CLI: real binary API commands, JSON output, cancellation and error exit codes; preserve the named behavior as a scenario assertion |
| cmd/protectorate/service_test.go | TestServicePrintAndErrors | Service Print And Errors | 0.000s pass | CONVERT | CLI: real binary API commands, JSON output, cancellation and error exit codes; preserve the named behavior as a scenario assertion |
| cmd/protectorate/sleeve_test.go | TestParseSize | Parse Size | 0.000s pass | CONVERT | SLEEVE: real sandbox lifecycle, secret boundaries, symlinks, git clone, OOM and toolchains; preserve the named behavior as a scenario assertion |
| cmd/protectorate/sleeve_test.go | TestExitLabels | Exit Labels | 0.000s pass | CONVERT | SLEEVE: real sandbox lifecycle, secret boundaries, symlinks, git clone, OOM and toolchains; preserve the named behavior as a scenario assertion |
| cmd/protectorate/tailnet_test.go | TestWriteTailnetStatus | Write Tailnet Status | 0.000s pass | CONVERT | TAILNET: isolated headscale, two nodes, ACL, relay and restart; preserve the named behavior as a scenario assertion |
| cmd/protectorate/tailnet_test.go | TestNodeJoinKeyInputs | Node Join Key Inputs | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| cmd/protectorate/token_test.go | TestTokenCLI | Token CLI | 0.010s pass | CONVERT | AUTH: scoped bearer, denial, revoke, expiry, audit and restart; preserve the named behavior as a scenario assertion |
| cmd/protectorate/token_test.go | TestPairMintsScopedDeviceToken | Pair Mints Scoped Device Token | 0.010s pass | CONVERT | AUTH: scoped bearer, denial, revoke, expiry, audit and restart; preserve the named behavior as a scenario assertion |
| cmd/protectorate/version_test.go | TestVersionDerivation | Version Derivation | 0.820s pass | CONVERT | CLI: real binary API commands, JSON output, cancellation and error exit codes; preserve the named behavior as a scenario assertion |
| cmd/protectorate/version_test.go | TestAppBuildsCarryTheVersion | App Builds Carry The Version | 0.070s pass | CONVERT | CLI: real binary API commands, JSON output, cancellation and error exit codes; preserve the named behavior as a scenario assertion |
| cmd/protectorate/version_test.go | TestCheckVersion | Check Version | 3.610s pass | CONVERT | CLI: real binary API commands, JSON output, cancellation and error exit codes; preserve the named behavior as a scenario assertion |
| cmd/protectorate/who_test.go | TestWriteProjects | Write Projects | 0.000s pass | CONVERT | CLI: real binary API commands, JSON output, cancellation and error exit codes; preserve the named behavior as a scenario assertion |
| internal/auth/audit_test.go | TestReadPageAcrossFiles | Read Page Across Files | 0.020s pass | CONVERT | AUTH: scoped bearer, denial, revoke, expiry, audit and restart; preserve the named behavior as a scenario assertion |
| internal/auth/audit_test.go | TestReadPageSkipsATornLastLine | Read Page Skips ATorn Last Line | 0.000s pass | CONVERT | AUTH: scoped bearer, denial, revoke, expiry, audit and restart; preserve the named behavior as a scenario assertion |
| internal/auth/auth_test.go | TestSnake | Snake | 0.000s pass | CONVERT | AUTH: scoped bearer, denial, revoke, expiry, audit and restart; preserve the named behavior as a scenario assertion |
| internal/auth/auth_test.go | TestCatalog | Catalog | 0.010s pass | CONVERT | AUTH: scoped bearer, denial, revoke, expiry, audit and restart; preserve the named behavior as a scenario assertion |
| internal/auth/auth_test.go | TestValidateScopes | Validate Scopes | 0.000s pass | CONVERT | AUTH: scoped bearer, denial, revoke, expiry, audit and restart; preserve the named behavior as a scenario assertion |
| internal/auth/auth_test.go | TestCreateGrant | Create Grant | 0.000s pass | CONVERT | AUTH: scoped bearer, denial, revoke, expiry, audit and restart; preserve the named behavior as a scenario assertion |
| internal/auth/auth_test.go | TestAdminOn | Admin On | 0.000s pass | CONVERT | AUTH: scoped bearer, denial, revoke, expiry, audit and restart; preserve the named behavior as a scenario assertion |
| internal/auth/auth_test.go | TestAuthorizeProcedure | Authorize Procedure | 0.000s pass | CONVERT | AUTH: scoped bearer, denial, revoke, expiry, audit and restart; preserve the named behavior as a scenario assertion |
| internal/auth/auth_test.go | TestAuthorizeTerminal | Authorize Terminal | 0.000s pass | CONVERT | AUTH: scoped bearer, denial, revoke, expiry, audit and restart; preserve the named behavior as a scenario assertion |
| internal/auth/auth_test.go | TestAuthenticateLifecycle | Authenticate Lifecycle | 0.000s pass | CONVERT | AUTH: scoped bearer, denial, revoke, expiry, audit and restart; preserve the named behavior as a scenario assertion |
| internal/auth/auth_test.go | TestEmptyOwnerTokenNeverMatches | Empty Owner Token Never Matches | 0.000s pass | CONVERT | AUTH: scoped bearer, denial, revoke, expiry, audit and restart; preserve the named behavior as a scenario assertion |
| internal/auth/auth_test.go | TestCreateValidation | Create Validation | 0.000s pass | CONVERT | AUTH: scoped bearer, denial, revoke, expiry, audit and restart; preserve the named behavior as a scenario assertion |
| internal/auth/auth_test.go | TestStorePersistenceAndNoSecrets | Store Persistence And No Secrets | 0.000s pass | CONVERT | AUTH: scoped bearer, denial, revoke, expiry, audit and restart; preserve the named behavior as a scenario assertion |
| internal/auth/auth_test.go | TestStoreCorruptFileFailsClosed | Store Corrupt File Fails Closed | 0.000s pass | CONVERT | AUTH: scoped bearer, denial, revoke, expiry, audit and restart; preserve the named behavior as a scenario assertion |
| internal/auth/auth_test.go | TestStoreConcurrent | Store Concurrent | 0.010s pass | CONVERT | AUTH: scoped bearer, denial, revoke, expiry, audit and restart; preserve the named behavior as a scenario assertion |
| internal/auth/auth_test.go | TestAuditAppendAndTail | Audit Append And Tail | 0.000s pass | CONVERT | AUTH: scoped bearer, denial, revoke, expiry, audit and restart; preserve the named behavior as a scenario assertion |
| internal/auth/auth_test.go | TestAuditFailureReported | Audit Failure Reported | 0.000s pass | CONVERT | AUTH: scoped bearer, denial, revoke, expiry, audit and restart; preserve the named behavior as a scenario assertion |
| internal/auth/auth_test.go | TestRecordOutcomes | Record Outcomes | 0.000s pass | CONVERT | AUTH: scoped bearer, denial, revoke, expiry, audit and restart; preserve the named behavior as a scenario assertion |
| internal/auth/auth_test.go | TestSubject | Subject | 0.000s pass | CONVERT | AUTH: scoped bearer, denial, revoke, expiry, audit and restart; preserve the named behavior as a scenario assertion |
| internal/auth/scopes_fuzz_test.go | FuzzScopeNormalization | Scope Normalization | 0.000s pass | KEEP-FUZZ | seed corpus on commits; bounded campaigns separately |
| internal/backlog/backlog_test.go | TestCreateValidation | Create Validation | 0.000s pass | CONVERT | BACKLOG: authenticated CRUD, concurrent writes and hub restart persistence; preserve the named behavior as a scenario assertion |
| internal/backlog/backlog_test.go | TestUpdate | Update | 0.000s pass | CONVERT | BACKLOG: authenticated CRUD, concurrent writes and hub restart persistence; preserve the named behavior as a scenario assertion |
| internal/backlog/backlog_test.go | TestListOrderAndFilter | List Order And Filter | 0.000s pass | CONVERT | BACKLOG: authenticated CRUD, concurrent writes and hub restart persistence; preserve the named behavior as a scenario assertion |
| internal/backlog/backlog_test.go | TestDeleteAndPersistence | Delete And Persistence | 0.000s pass | CONVERT | BACKLOG: authenticated CRUD, concurrent writes and hub restart persistence; preserve the named behavior as a scenario assertion |
| internal/backlog/backlog_test.go | TestConcurrentStores | Concurrent Stores | 0.000s pass | CONVERT | BACKLOG: authenticated CRUD, concurrent writes and hub restart persistence; preserve the named behavior as a scenario assertion |
| internal/backlog/backlog_test.go | TestMigratesLegacyBacklog | Migrates Legacy Backlog | 0.000s pass | CONVERT | BACKLOG: authenticated CRUD, concurrent writes and hub restart persistence; preserve the named behavior as a scenario assertion |
| internal/backlog/backlog_test.go | TestEmptyHubHasNoNotesFile | Empty Hub Has No Notes File | 0.000s pass | CONVERT | BACKLOG: authenticated CRUD, concurrent writes and hub restart persistence; preserve the named behavior as a scenario assertion |
| internal/backup/backup_test.go | TestRoundTripWhileDatabaseIsBusy | Round Trip While Database Is Busy | 0.060s pass | CONVERT | BACKUP: live hub snapshot, restore, tamper rejection and headscale integrity; preserve the named behavior as a scenario assertion |
| internal/backup/backup_test.go | TestRestoreRefusesNonEmptyHome | Restore Refuses Non Empty Home | 0.030s pass | CONVERT | BACKUP: live hub snapshot, restore, tamper rejection and headscale integrity; preserve the named behavior as a scenario assertion |
| internal/backup/backup_test.go | TestRestoreRejectsBadArchives | Restore Rejects Bad Archives | 0.090s pass | CONVERT | BACKUP: live hub snapshot, restore, tamper rejection and headscale integrity; preserve the named behavior as a scenario assertion |
| internal/backup/backup_test.go | TestManifestListsEverythingOnce | Manifest Lists Everything Once | 0.000s pass | CONVERT | BACKUP: live hub snapshot, restore, tamper rejection and headscale integrity; preserve the named behavior as a scenario assertion |
| internal/backup/backup_test.go | TestLatest | Latest | 0.000s pass | CONVERT | BACKUP: live hub snapshot, restore, tamper rejection and headscale integrity; preserve the named behavior as a scenario assertion |
| internal/backup/backup_test.go | TestRunSchedulesBackups | Run Schedules Backups | 8.510s pass | CONVERT | BACKUP: live hub snapshot, restore, tamper rejection and headscale integrity; preserve the named behavior as a scenario assertion |
| internal/chatlog/chatlog_test.go | TestSeqAndCoalescing | Seq And Coalescing | 0.010s pass | CONVERT | CHAT: live stream versus restart replay, partial writes and slow consumer reconnect; preserve the named behavior as a scenario assertion |
| internal/chatlog/chatlog_test.go | TestSeqIsContiguous | Seq Is Contiguous | 0.000s pass | CONVERT | CHAT: live stream versus restart replay, partial writes and slow consumer reconnect; preserve the named behavior as a scenario assertion |
| internal/chatlog/chatlog_test.go | TestReopenKeepsEventsAndCutsPartialLine | Reopen Keeps Events And Cuts Partial Line | 0.000s pass | CONVERT | CHAT: live stream versus restart replay, partial writes and slow consumer reconnect; preserve the named behavior as a scenario assertion |
| internal/chatlog/chatlog_test.go | TestCorruptLogRefused | Corrupt Log Refused | 0.000s pass | CONVERT | CHAT: live stream versus restart replay, partial writes and slow consumer reconnect; preserve the named behavior as a scenario assertion |
| internal/chatlog/chatlog_test.go | TestWatchWhileMessageOpen | Watch While Message Open | 0.020s pass | CONVERT | CHAT: live stream versus restart replay, partial writes and slow consumer reconnect; preserve the named behavior as a scenario assertion |
| internal/chatlog/chatlog_test.go | TestWatchEndsOnCloseAndContext | Watch Ends On Close And Context | 0.000s pass | CONVERT | CHAT: live stream versus restart replay, partial writes and slow consumer reconnect; preserve the named behavior as a scenario assertion |
| internal/chatlog/chatlog_test.go | TestSlowWatcherIsCut | Slow Watcher Is Cut | 0.030s pass | CONVERT | CHAT: live stream versus restart replay, partial writes and slow consumer reconnect; preserve the named behavior as a scenario assertion |
| internal/chatlog/chatlog_test.go | TestNoGapNoDuplicateAcrossReplayLiveBoundary | No Gap No Duplicate Across Replay Live Boundary | 0.160s pass | CONVERT | CHAT: live stream versus restart replay, partial writes and slow consumer reconnect; preserve the named behavior as a scenario assertion |
| internal/desk/desk_test.go | TestParseChord | Parse Chord | 0.000s pass | CONVERT | DESK: isolated node desktop consent, lease, revocation and input fencing; preserve the named behavior as a scenario assertion |
| internal/desk/desk_test.go | TestParseButton | Parse Button | 0.000s pass | CONVERT | DESK: isolated node desktop consent, lease, revocation and input fencing; preserve the named behavior as a scenario assertion |
| internal/desk/desk_test.go | TestEncodePNG | Encode PNG | 0.000s pass | CONVERT | DESK: isolated node desktop consent, lease, revocation and input fencing; preserve the named behavior as a scenario assertion |
| internal/desk/desk_test.go | TestDownscale | Downscale | 0.020s pass | CONVERT | DESK: isolated node desktop consent, lease, revocation and input fencing; preserve the named behavior as a scenario assertion |
| internal/desk/desk_test.go | TestCheckInteractive | Check Interactive | 0.000s pass | CONVERT | DESK: isolated node desktop consent, lease, revocation and input fencing; preserve the named behavior as a scenario assertion |
| internal/desk/desk_test.go | TestClassifyInputDesktop | Classify Input Desktop | 0.000s pass | CONVERT | DESK: isolated node desktop consent, lease, revocation and input fencing; preserve the named behavior as a scenario assertion |
| internal/desk/desk_test.go | TestCheckReach | Check Reach | 0.000s pass | CONVERT | DESK: isolated node desktop consent, lease, revocation and input fencing; preserve the named behavior as a scenario assertion |
| internal/desk/input_type_test.go | TestTypeGroups | Type Groups | 0.000s pass | CONVERT | DESK: isolated node desktop consent, lease, revocation and input fencing; preserve the named behavior as a scenario assertion |
| internal/desk/policy_test.go | TestParseWindow | Parse Window | 0.000s pass | CONVERT | DESK: isolated node desktop consent, lease, revocation and input fencing; preserve the named behavior as a scenario assertion |
| internal/desk/policy_test.go | TestInWindow | In Window | 0.010s pass | CONVERT | DESK: isolated node desktop consent, lease, revocation and input fencing; preserve the named behavior as a scenario assertion |
| internal/desk/policy_test.go | TestEvaluate | Evaluate | 0.000s pass | CONVERT | DESK: isolated node desktop consent, lease, revocation and input fencing; preserve the named behavior as a scenario assertion |
| internal/desk/policy_test.go | TestInjectedFlags | Injected Flags | 0.000s pass | CONVERT | DESK: isolated node desktop consent, lease, revocation and input fencing; preserve the named behavior as a scenario assertion |
| internal/desk/policy_test.go | TestTracker | Tracker | 0.000s pass | CONVERT | DESK: isolated node desktop consent, lease, revocation and input fencing; preserve the named behavior as a scenario assertion |
| internal/directory/directory_test.go | TestParseManifests | Parse Manifests | 0.000s pass | CONVERT | MAIL: real repositories, project resolution and dependency notices; preserve the named behavior as a scenario assertion |
| internal/directory/directory_test.go | TestParseTOMLValues | Parse TOMLValues | 0.000s pass | CONVERT | MAIL: real repositories, project resolution and dependency notices; preserve the named behavior as a scenario assertion |
| internal/directory/directory_test.go | TestNormalizeRemote | Normalize Remote | 0.000s pass | CONVERT | MAIL: real repositories, project resolution and dependency notices; preserve the named behavior as a scenario assertion |
| internal/directory/directory_test.go | TestReadManifestsLayout | Read Manifests Layout | 0.010s pass | CONVERT | MAIL: real repositories, project resolution and dependency notices; preserve the named behavior as a scenario assertion |
| internal/directory/directory_test.go | TestEdgesAndLookup | Edges And Lookup | 0.000s pass | CONVERT | MAIL: real repositories, project resolution and dependency notices; preserve the named behavior as a scenario assertion |
| internal/directory/directory_test.go | TestDirectoryPersistsAndKeepsAbsentProjects | Directory Persists And Keeps Absent Projects | 0.000s pass | CONVERT | MAIL: real repositories, project resolution and dependency notices; preserve the named behavior as a scenario assertion |
| internal/directory/directory_test.go | TestInspect | Inspect | 0.110s pass | CONVERT | MAIL: real repositories, project resolution and dependency notices; preserve the named behavior as a scenario assertion |
| internal/directory/refs_test.go | TestChanges | Changes | 0.000s pass | CONVERT | MAIL: real repositories, project resolution and dependency notices; preserve the named behavior as a scenario assertion |
| internal/directory/refs_test.go | TestMerge | Merge | 0.000s pass | CONVERT | MAIL: real repositories, project resolution and dependency notices; preserve the named behavior as a scenario assertion |
| internal/directory/refs_test.go | TestReadRefsAndDescribe | Read Refs And Describe | 0.380s pass | CONVERT | MAIL: real repositories, project resolution and dependency notices; preserve the named behavior as a scenario assertion |
| internal/directory/refs_test.go | TestDescribeRunsNoRepositoryPrograms | Describe Runs No Repository Programs | 0.520s pass | CONVERT | MAIL: real repositories, project resolution and dependency notices; preserve the named behavior as a scenario assertion |
| internal/driver/driver_test.go | TestNewSession | New Session | 0.080s pass | CONVERT | CHAT: official protocol subprocesses, approval, interruption, compaction and replay; preserve the named behavior as a scenario assertion |
| internal/driver/driver_test.go | TestEventDetails | Event Details | 0.090s pass | DELETE | implementation/output-shape assertion; validate externally visible behavior in CHAT: official protocol subprocesses, approval, interruption, compaction and replay |
| internal/driver/driver_test.go | TestResume | Resume | 0.030s pass | CONVERT | CHAT: official protocol subprocesses, approval, interruption, compaction and replay; preserve the named behavior as a scenario assertion |
| internal/driver/driver_test.go | TestApproval | Approval | 0.230s pass | CONVERT | CHAT: official protocol subprocesses, approval, interruption, compaction and replay; preserve the named behavior as a scenario assertion |
| internal/driver/driver_test.go | TestInterruptAndSteer | Interrupt And Steer | 0.130s pass | CONVERT | CHAT: official protocol subprocesses, approval, interruption, compaction and replay; preserve the named behavior as a scenario assertion |
| internal/driver/driver_test.go | TestExitMidTurn | Exit Mid Turn | 0.120s pass | CONVERT | CHAT: official protocol subprocesses, approval, interruption, compaction and replay; preserve the named behavior as a scenario assertion |
| internal/driver/driver_test.go | TestCodexResumeLocked | Codex Resume Locked | 0.050s pass | CONVERT | CHAT: official protocol subprocesses, approval, interruption, compaction and replay; preserve the named behavior as a scenario assertion |
| internal/driver/driver_test.go | TestOpenErrors | Open Errors | 0.000s pass | CONVERT | CHAT: official protocol subprocesses, approval, interruption, compaction and replay; preserve the named behavior as a scenario assertion |
| internal/driver/driver_test.go | TestCaps | Caps | 0.000s pass | CONVERT | CHAT: official protocol subprocesses, approval, interruption, compaction and replay; preserve the named behavior as a scenario assertion |
| internal/driver/driver_test.go | TestStderrBounded | Stderr Bounded | 0.030s pass | CONVERT | CHAT: official protocol subprocesses, approval, interruption, compaction and replay; preserve the named behavior as a scenario assertion |
| internal/driver/driver_test.go | TestCloseEscalates | Close Escalates | 0.070s pass | CONVERT | CHAT: official protocol subprocesses, approval, interruption, compaction and replay; preserve the named behavior as a scenario assertion |
| internal/driver/driver_test.go | TestOwners | Owners | 0.000s pass | CONVERT | CHAT: official protocol subprocesses, approval, interruption, compaction and replay; preserve the named behavior as a scenario assertion |
| internal/driver/driver_test.go | TestOwnersOpenDriver | Owners Open Driver | 0.010s pass | CONVERT | CHAT: official protocol subprocesses, approval, interruption, compaction and replay; preserve the named behavior as a scenario assertion |
| internal/driver/driver_test.go | TestCommandWrapsHarness | Command Wraps Harness | 0.000s pass | CONVERT | CHAT: official protocol subprocesses, approval, interruption, compaction and replay; preserve the named behavior as a scenario assertion |
| internal/driver/driver_test.go | TestUsageAndToolResults | Usage And Tool Results | 0.110s pass | CONVERT | CHAT: official protocol subprocesses, approval, interruption, compaction and replay; preserve the named behavior as a scenario assertion |
| internal/driver/driver_test.go | TestThinkingAndMessageIDs | Thinking And Message IDs | 0.080s pass | CONVERT | CHAT: official protocol subprocesses, approval, interruption, compaction and replay; preserve the named behavior as a scenario assertion |
| internal/driver/driver_test.go | TestCompact | Compact | 0.100s pass | CONVERT | CHAT: official protocol subprocesses, approval, interruption, compaction and replay; preserve the named behavior as a scenario assertion |
| internal/driver/driver_test.go | TestCompactBusy | Compact Busy | 0.010s pass | CONVERT | CHAT: official protocol subprocesses, approval, interruption, compaction and replay; preserve the named behavior as a scenario assertion |
| internal/driver/driver_test.go | TestNativeAutoCompaction | Native Auto Compaction | 0.020s pass | CONVERT | CHAT: official protocol subprocesses, approval, interruption, compaction and replay; preserve the named behavior as a scenario assertion |
| internal/driver/driver_test.go | TestAutonomousFlags | Autonomous Flags | 0.010s pass | CONVERT | CHAT: official protocol subprocesses, approval, interruption, compaction and replay; preserve the named behavior as a scenario assertion |
| internal/driver/driver_test.go | TestCodexAutonomousLeavesPolicyToConfig | Codex Autonomous Leaves Policy To Config | 0.100s pass | CONVERT | CHAT: official protocol subprocesses, approval, interruption, compaction and replay; preserve the named behavior as a scenario assertion |
| internal/driver/driver_test.go | TestSyntheticMessageReportsNoUsage | Synthetic Message Reports No Usage | 0.040s pass | CONVERT | CHAT: official protocol subprocesses, approval, interruption, compaction and replay; preserve the named behavior as a scenario assertion |
| internal/driver/fake_test.go | TestMain | Main | not measured | KEEP-E2E | runner infrastructure; retain while any package tests use it |
| internal/driver/selection_test.go | TestCodexSelectionProtocol | Codex Selection Protocol | 0.090s pass | CONVERT | CHAT: official protocol subprocesses, approval, interruption, compaction and replay; preserve the named behavior as a scenario assertion |
| internal/driver/selection_test.go | TestSelectionUnsupportedAndNativeDefaults | Selection Unsupported And Native Defaults | 0.020s pass | CONVERT | CHAT: official protocol subprocesses, approval, interruption, compaction and replay; preserve the named behavior as a scenario assertion |
| internal/driver/selection_test.go | TestCodexSelectionAtOpenAndResume | Codex Selection At Open And Resume | 0.010s pass | CONVERT | CHAT: official protocol subprocesses, approval, interruption, compaction and replay; preserve the named behavior as a scenario assertion |
| internal/driver/selection_test.go | TestSelectionUsesNativeConfirmation | Selection Uses Native Confirmation | 0.000s pass | CONVERT | CHAT: official protocol subprocesses, approval, interruption, compaction and replay; preserve the named behavior as a scenario assertion |
| internal/fsutil/fsutil_test.go | TestWriteFile | Write File | 0.000s pass | CONVERT | BACKUP: permission-preserving atomic persistence and interrupted writes; preserve the named behavior as a scenario assertion |
| internal/fsutil/fsutil_test.go | TestWriteFileMissingDir | Write File Missing Dir | 0.000s pass | CONVERT | BACKUP: permission-preserving atomic persistence and interrupted writes; preserve the named behavior as a scenario assertion |
| internal/handoff/handoff_test.go | TestReadTranscript | Read Transcript | 0.000s pass | CONVERT | HANDOFF: real two-harness resleeve with git facts and persisted handoff; preserve the named behavior as a scenario assertion |
| internal/handoff/handoff_test.go | TestReadTranscriptRefuses | Read Transcript Refuses | 0.000s pass | CONVERT | HANDOFF: real two-harness resleeve with git facts and persisted handoff; preserve the named behavior as a scenario assertion |
| internal/handoff/handoff_test.go | TestReadTranscriptTail | Read Transcript Tail | 0.270s pass | CONVERT | HANDOFF: real two-harness resleeve with git facts and persisted handoff; preserve the named behavior as a scenario assertion |
| internal/handoff/handoff_test.go | TestMarkdownBudget | Markdown Budget | 0.000s pass | CONVERT | HANDOFF: real two-harness resleeve with git facts and persisted handoff; preserve the named behavior as a scenario assertion |
| internal/handoff/handoff_test.go | TestDocumentSummaryNotes | Document Summary Notes | 0.000s pass | CONVERT | HANDOFF: real two-harness resleeve with git facts and persisted handoff; preserve the named behavior as a scenario assertion |
| internal/handoff/handoff_test.go | TestWrite | Write | 0.000s pass | CONVERT | HANDOFF: real two-harness resleeve with git facts and persisted handoff; preserve the named behavior as a scenario assertion |
| internal/handoff/handoff_test.go | TestExclude | Exclude | 0.200s pass | CONVERT | HANDOFF: real two-harness resleeve with git facts and persisted handoff; preserve the named behavior as a scenario assertion |
| internal/handoff/handoff_test.go | TestReadGitFacts | Read Git Facts | 0.000s pass | CONVERT | HANDOFF: real two-harness resleeve with git facts and persisted handoff; preserve the named behavior as a scenario assertion |
| internal/hub/auth_test.go | TestUnaryScopeEnforcement | Unary Scope Enforcement | 0.200s pass | CONVERT | AUTH: scoped bearer, denial, revoke, expiry, audit and restart; preserve the named behavior as a scenario assertion |
| internal/hub/auth_test.go | TestStreamScopeEnforcement | Stream Scope Enforcement | 0.060s pass | CONVERT | AUTH: scoped bearer, denial, revoke, expiry, audit and restart; preserve the named behavior as a scenario assertion |
| internal/hub/auth_test.go | TestTokenServiceOwnerOnly | Token Service Owner Only | 0.020s pass | CONVERT | AUTH: scoped bearer, denial, revoke, expiry, audit and restart; preserve the named behavior as a scenario assertion |
| internal/hub/auth_test.go | TestAuditCoversMutationsNotReads | Audit Covers Mutations Not Reads | 0.060s pass | CONVERT | AUTH: scoped bearer, denial, revoke, expiry, audit and restart; preserve the named behavior as a scenario assertion |
| internal/hub/auth_test.go | TestTerminalScopeEnforcement | Terminal Scope Enforcement | 0.370s pass | CONVERT | AUTH: scoped bearer, denial, revoke, expiry, audit and restart; preserve the named behavior as a scenario assertion |
| internal/hub/auth_test.go | TestTailnetPathComposesIdentityAndToken | Tailnet Path Composes Identity And Token | 0.040s pass | CONVERT | AUTH: scoped bearer, denial, revoke, expiry, audit and restart; preserve the named behavior as a scenario assertion |
| internal/hub/auth_test.go | TestDefaultHubHasOwnerAuthAndNoDiskWrites | Default Hub Has Owner Auth And No Disk Writes | 0.010s pass | CONVERT | AUTH: scoped bearer, denial, revoke, expiry, audit and restart; preserve the named behavior as a scenario assertion |
| internal/hub/auth_test.go | TestSpawnSleeveReservesOrchestratorName | Spawn Sleeve Reserves Orchestrator Name | 0.000s pass | CONVERT | AUTH: scoped bearer, denial, revoke, expiry, audit and restart; preserve the named behavior as a scenario assertion |
| internal/hub/auth_test.go | TestChatScopes | Chat Scopes | 0.000s pass | CONVERT | AUTH: scoped bearer, denial, revoke, expiry, audit and restart; preserve the named behavior as a scenario assertion |
| internal/hub/auth_test.go | TestListAuditEntriesOwnerOnly | List Audit Entries Owner Only | 0.060s pass | CONVERT | AUTH: scoped bearer, denial, revoke, expiry, audit and restart; preserve the named behavior as a scenario assertion |
| internal/hub/backlog_test.go | TestNotesService | Notes Service | 0.060s pass | CONVERT | BACKLOG: authenticated CRUD, persistence and concurrent updates; preserve the named behavior as a scenario assertion |
| internal/hub/chat_test.go | TestChatEndToEnd | Chat End To End | 0.000s skip | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/hub/chat_test.go | TestChatShellAndMissing | Chat Shell And Missing | 0.000s skip | CONVERT | CHAT: official protocol subprocesses, approval, interruption, compaction and replay; preserve the named behavior as a scenario assertion |
| internal/hub/chat_test.go | TestChatSupervisedPermissionMode | Chat Supervised Permission Mode | 0.000s pass | CONVERT | CHAT: official protocol subprocesses, approval, interruption, compaction and replay; preserve the named behavior as a scenario assertion |
| internal/hub/chat_test.go | TestChatTerminalSleeveHasNoChat | Chat Terminal Sleeve Has No Chat | 0.000s skip | CONVERT | CHAT: official protocol subprocesses, approval, interruption, compaction and replay; preserve the named behavior as a scenario assertion |
| internal/hub/chat_test.go | TestSleeveTokenReadsOnlyItself | Sleeve Token Reads Only Itself | 0.000s skip | CONVERT | AUTH: scoped bearer, denial, revoke, expiry, audit and restart; preserve the named behavior as a scenario assertion |
| internal/hub/chat_test.go | TestChatSendRequestIDsSurviveHubRestart | Chat Send Request IDs Survive Hub Restart | 0.000s skip | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/hub/chat_test.go | TestChatHubRestartMidTurnFailsThenResumes | Chat Hub Restart Mid Turn Fails Then Resumes | 0.010s pass | CONVERT | CHAT: official protocol subprocesses, approval, interruption, compaction and replay; preserve the named behavior as a scenario assertion |
| internal/hub/deps_test.go | TestDependencyNotices | Dependency Notices | 0.000s skip | CONVERT | CLI: real binary API commands, JSON output, cancellation and error exit codes; preserve the named behavior as a scenario assertion |
| internal/hub/deskpolicy_test.go | TestDeskPolicyFlow | Desk Policy Flow | 0.310s pass | CONVERT | DESK: isolated node desktop consent, lease, revocation and input fencing; preserve the named behavior as a scenario assertion |
| internal/hub/deskpolicy_test.go | TestDeskPolicyPersistence | Desk Policy Persistence | 0.000s pass | CONVERT | DESK: isolated node desktop consent, lease, revocation and input fencing; preserve the named behavior as a scenario assertion |
| internal/hub/deskuse_test.go | TestDeskUseCountsPerNodeAndCaller | Desk Use Counts Per Node And Caller | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/hub/deskuse_test.go | TestDeskUseSurvivesRestartWithinTheDay | Desk Use Survives Restart Within The Day | 0.000s pass | CONVERT | DESK: isolated node desktop consent, lease, revocation and input fencing; preserve the named behavior as a scenario assertion |
| internal/hub/envoy_test.go | TestSpawnEnvoyOwnerOnly | Spawn Envoy Owner Only | 0.000s skip | CONVERT | MAIL: Envoy identity, wakeup, grants and mailbox delivery; preserve the named behavior as a scenario assertion |
| internal/hub/envoy_test.go | TestSpawnEnvoyAndRenew | Spawn Envoy And Renew | 0.000s skip | CONVERT | MAIL: Envoy identity, wakeup, grants and mailbox delivery; preserve the named behavior as a scenario assertion |
| internal/hub/envoy_test.go | TestPlanEnvoyRenewal | Plan Envoy Renewal | 0.000s pass | CONVERT | MAIL: Envoy identity, wakeup, grants and mailbox delivery; preserve the named behavior as a scenario assertion |
| internal/hub/envoy_test.go | TestMigrateEnvoyTokens | Migrate Envoy Tokens | 0.010s pass | CONVERT | AUTH: scoped bearer, denial, revoke, expiry, audit and restart; preserve the named behavior as a scenario assertion |
| internal/hub/gpubusy_test.go | TestBusyGPUs | Busy GPUs | 0.000s pass | CONVERT | CLI: real binary API commands, JSON output, cancellation and error exit codes; preserve the named behavior as a scenario assertion |
| internal/hub/gpubusy_test.go | TestNodeFitsSkipsBusyGPUs | Node Fits Skips Busy GPUs | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/hub/gpubusy_test.go | TestBusyGPURefusals | Busy GPURefusals | 0.250s pass | CONVERT | CLI: real binary API commands, JSON output, cancellation and error exit codes; preserve the named behavior as a scenario assertion |
| internal/hub/grants_test.go | TestGrantAdminMinting | Grant Admin Minting | 0.370s pass | CONVERT | AUTH: scoped bearer, denial, revoke, expiry, audit and restart; preserve the named behavior as a scenario assertion |
| internal/hub/grants_test.go | TestAdminJobs | Admin Jobs | 0.620s pass | CONVERT | JOB: real agent git/artifact execution, cancel, idempotency and hub restart; preserve the named behavior as a scenario assertion |
| internal/hub/hub_test.go | TestGetStatusGeneratedClient | Get Status Generated Client | 0.010s pass | CONVERT | CLI: real binary API commands, JSON output, cancellation and error exit codes; preserve the named behavior as a scenario assertion |
| internal/hub/hub_test.go | TestRawJSON | Raw JSON | 0.030s pass | CONVERT | CLI: real binary API commands, JSON output, cancellation and error exit codes; preserve the named behavior as a scenario assertion |
| internal/hub/hub_test.go | TestAuthRejectedTyped | Auth Rejected Typed | 0.020s pass | CONVERT | AUTH: scoped bearer, denial, revoke, expiry, audit and restart; preserve the named behavior as a scenario assertion |
| internal/hub/hub_test.go | TestSubscribeHeartbeats | Subscribe Heartbeats | 0.100s pass | CONVERT | CLI: real binary API commands, JSON output, cancellation and error exit codes; preserve the named behavior as a scenario assertion |
| internal/hub/hub_test.go | TestServeHTTP2Cleartext | Serve HTTP2 Cleartext | 0.010s pass | CONVERT | CLI: real binary API commands, JSON output, cancellation and error exit codes; preserve the named behavior as a scenario assertion |
| internal/hub/hub_test.go | TestToken | Token | 0.000s pass | CONVERT | AUTH: scoped bearer, denial, revoke, expiry, audit and restart; preserve the named behavior as a scenario assertion |
| internal/hub/hub_test.go | TestWebDir | Web Dir | 0.060s pass | CONVERT | WEB: binary serves SPA, bootstrap, pairing, install scripts and downloads; preserve the named behavior as a scenario assertion |
| internal/hub/hub_test.go | TestServeListenerFailureReturns | Serve Listener Failure Returns | 0.030s pass | CONVERT | CLI: real binary API commands, JSON output, cancellation and error exit codes; preserve the named behavior as a scenario assertion |
| internal/hub/identity_test.go | TestSenderOf | Sender Of | 0.030s pass | CONVERT | CLI: real binary API commands, JSON output, cancellation and error exit codes; preserve the named behavior as a scenario assertion |
| internal/hub/identity_test.go | TestLocalURL | Local URL | 0.000s pass | CONVERT | CLI: real binary API commands, JSON output, cancellation and error exit codes; preserve the named behavior as a scenario assertion |
| internal/hub/identity_test.go | TestSleeveIdentity | Sleeve Identity | 0.010s skip | CONVERT | SLEEVE: real sandbox lifecycle, secret boundaries, symlinks, git clone, OOM and toolchains; preserve the named behavior as a scenario assertion |
| internal/hub/install_test.go | TestInstallScriptGolden | Install Script Golden | 0.010s pass | CONVERT | WEB: binary serves SPA, bootstrap, pairing, install scripts and downloads; preserve the named behavior as a scenario assertion |
| internal/hub/install_test.go | TestInstallScriptQuotesItsURLs | Install Script Quotes Its URLs | 0.000s pass | CONVERT | WEB: binary serves SPA, bootstrap, pairing, install scripts and downloads; preserve the named behavior as a scenario assertion |
| internal/hub/install_test.go | TestInstallShTarget | Install Sh Target | 0.680s pass | CONVERT | WEB: binary serves SPA, bootstrap, pairing, install scripts and downloads; preserve the named behavior as a scenario assertion |
| internal/hub/install_test.go | TestInstallPs1Arch | Install Ps1 Arch | 0.000s skip | CONVERT | WEB: binary serves SPA, bootstrap, pairing, install scripts and downloads; preserve the named behavior as a scenario assertion |
| internal/hub/install_test.go | TestQuoting | Quoting | 0.000s pass | CONVERT | WEB: binary serves SPA, bootstrap, pairing, install scripts and downloads; preserve the named behavior as a scenario assertion |
| internal/hub/install_test.go | TestInstallCommands | Install Commands | 0.000s pass | CONVERT | WEB: binary serves SPA, bootstrap, pairing, install scripts and downloads; preserve the named behavior as a scenario assertion |
| internal/hub/install_test.go | TestInstallBase | Install Base | 0.000s pass | CONVERT | WEB: binary serves SPA, bootstrap, pairing, install scripts and downloads; preserve the named behavior as a scenario assertion |
| internal/hub/install_test.go | TestInstallEndpoints | Install Endpoints | 0.000s pass | CONVERT | WEB: binary serves SPA, bootstrap, pairing, install scripts and downloads; preserve the named behavior as a scenario assertion |
| internal/hub/invite_test.go | TestInviteNodeChecks | Invite Node Checks | 0.180s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/hub/invite_test.go | TestInviteNodeRecordsAndReissues | Invite Node Records And Reissues | 0.110s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/hub/invite_test.go | TestWatchNodeSetup | Watch Node Setup | 0.130s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/hub/invite_test.go | TestSetupChecks | Setup Checks | 0.000s pass | CONVERT | CLI: real binary API commands, JSON output, cancellation and error exit codes; preserve the named behavior as a scenario assertion |
| internal/hub/invite_test.go | TestRemoveNodeOwnerOnly | Remove Node Owner Only | 0.130s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/hub/invite_test.go | TestRemoveNodeCancelsPendingInvite | Remove Node Cancels Pending Invite | 0.120s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/hub/invite_test.go | TestRemoveNodeDrains | Remove Node Drains | 5.490s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/hub/invite_test.go | TestSetNodeTier | Set Node Tier | 0.080s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/hub/jobkeys_test.go | TestSubmitKeyDedupe | Submit Key Dedupe | 0.000s pass | CONVERT | JOB: real agent git/artifact execution, cancel, idempotency and hub restart; preserve the named behavior as a scenario assertion |
| internal/hub/jobkeys_test.go | TestSubmitKeyValidation | Submit Key Validation | 0.000s pass | CONVERT | JOB: real agent git/artifact execution, cancel, idempotency and hub restart; preserve the named behavior as a scenario assertion |
| internal/hub/jobkeys_test.go | TestSubmitKeyConcurrentDuplicatesShareOneJob | Submit Key Concurrent Duplicates Share One Job | 0.050s pass | CONVERT | JOB: real agent git/artifact execution, cancel, idempotency and hub restart; preserve the named behavior as a scenario assertion |
| internal/hub/jobkeys_test.go | TestCancelByKeyRacesSubmit | Cancel By Key Races Submit | 0.100s pass | CONVERT | JOB: real agent git/artifact execution, cancel, idempotency and hub restart; preserve the named behavior as a scenario assertion |
| internal/hub/jobkeys_test.go | TestCancelKeyTombstoneExpires | Cancel Key Tombstone Expires | 0.000s pass | CONVERT | JOB: real agent git/artifact execution, cancel, idempotency and hub restart; preserve the named behavior as a scenario assertion |
| internal/hub/jobkeys_test.go | TestSubmitKeyWaitHonorsContext | Submit Key Wait Honors Context | 0.000s pass | CONVERT | JOB: real agent git/artifact execution, cancel, idempotency and hub restart; preserve the named behavior as a scenario assertion |
| internal/hub/jobkeys_test.go | TestExpiredLeaseIsReselectable | Expired Lease Is Reselectable | 0.000s pass | CONVERT | LEASE: exclusivity, FIFO, expiry/renewal and restart; preserve the named behavior as a scenario assertion |
| internal/hub/jobkeys_test.go | TestSubmitRequestIDEndToEnd | Submit Request IDEnd To End | 0.310s pass | CONVERT | JOB: real agent git/artifact execution, cancel, idempotency and hub restart; preserve the named behavior as a scenario assertion |
| internal/hub/jobs_test.go | TestMain | Main | not measured | KEEP-E2E | runner infrastructure; retain while any package tests use it |
| internal/hub/jobs_test.go | TestGitJobWarmCacheAndIncrementalBundle | Git Job Warm Cache And Incremental Bundle | 0.950s pass | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/hub/jobs_test.go | TestArtifactJobAndArtifactUpload | Artifact Job And Artifact Upload | 0.570s pass | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/hub/jobs_test.go | TestJobOnHubsOwnNode | Job On Hubs Own Node | 0.330s pass | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/hub/jobs_test.go | TestJobFailureAndEnv | Job Failure And Env | 0.320s pass | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/hub/jobs_test.go | TestJobCancelKillsProcessTree | Job Cancel Kills Process Tree | 0.390s pass | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/hub/jobs_test.go | TestJobTimeout | Job Timeout | 1.050s pass | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/hub/jobs_test.go | TestNodeConcurrencyLimit | Node Concurrency Limit | 1.920s pass | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/hub/jobs_test.go | TestConcurrencyTwoSlots | Concurrency Two Slots | 1.870s pass | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/hub/jobs_test.go | TestSamRepoJobsSerialize | Sam Repo Jobs Serialize | 1.530s pass | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/hub/jobs_test.go | TestWatchJobReplayAndFollow | Watch Job Replay And Follow | 0.800s pass | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/hub/jobs_test.go | TestSubmitValidation | Submit Validation | 0.290s pass | CONVERT | JOB: real agent git/artifact execution, cancel, idempotency and hub restart; preserve the named behavior as a scenario assertion |
| internal/hub/jobs_test.go | TestJobSurvivesHubRestart | Job Survives Hub Restart | 2.400s pass | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/hub/jobs_test.go | TestSandboxedJobSurvivesHubRestart | Sandboxed Job Survives Hub Restart | 0.390s skip | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/hub/jobs_test.go | TestRecoveredJobCanBeCanceled | Recovered Job Can Be Canceled | 0.860s pass | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/hub/jobs_test.go | TestRestartReconcilesRecordsWithNodes | Restart Reconciles Records With Nodes | 0.960s pass | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/hub/jobs_test.go | TestShutdownLeavesRemoteJobsRunningAndStopsLocalOnes | Shutdown Leaves Remote Jobs Running And Stops Local Ones | 0.370s pass | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/hub/jobs_test.go | TestJobEventsAndScopes | Job Events And Scopes | 0.360s pass | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/hub/jobs_test.go | TestJobAndLeaseScopesAndAudit | Job And Lease Scopes And Audit | 0.430s pass | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/hub/jobs_test.go | TestWarmNodes | Warm Nodes | 0.000s pass | CONVERT | JOB: real agent git/artifact execution, cancel, idempotency and hub restart; preserve the named behavior as a scenario assertion |
| internal/hub/jobs_test.go | TestArtifactDirectoryRunsInWorkdirAndKeepsModes | Artifact Directory Runs In Workdir And Keeps Modes | 1.550s pass | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/hub/jobs_test.go | TestCareTierEnforcement | Care Tier Enforcement | 0.550s pass | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/hub/jobs_test.go | TestSubmitRequestIDsSurviveHubRestart | Submit Request IDs Survive Hub Restart | 0.710s pass | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/hub/jobsource_test.go | TestBundleIncremental | Bundle Incremental | 0.440s pass | CONVERT | JOB: real agent git/artifact execution, cancel, idempotency and hub restart; preserve the named behavior as a scenario assertion |
| internal/hub/jobsource_test.go | TestUsablePrerequisites | Usable Prerequisites | 0.310s pass | CONVERT | JOB: real agent git/artifact execution, cancel, idempotency and hub restart; preserve the named behavior as a scenario assertion |
| internal/hub/jobsource_test.go | TestResolveRepoCommit | Resolve Repo Commit | 0.140s pass | CONVERT | JOB: real agent git/artifact execution, cancel, idempotency and hub restart; preserve the named behavior as a scenario assertion |
| internal/hub/jobsource_test.go | TestRepoKey | Repo Key | 0.000s pass | CONVERT | JOB: real agent git/artifact execution, cancel, idempotency and hub restart; preserve the named behavior as a scenario assertion |
| internal/hub/lease_queue_test.go | TestLeaseQueueOrderAndClaim | Lease Queue Order And Claim | 0.000s pass | CONVERT | LEASE: exclusivity, FIFO, expiry/renewal and restart; preserve the named behavior as a scenario assertion |
| internal/hub/lease_queue_test.go | TestLeaseQueueOnlyForOptIn | Lease Queue Only For Opt In | 0.000s pass | CONVERT | LEASE: exclusivity, FIFO, expiry/renewal and restart; preserve the named behavior as a scenario assertion |
| internal/hub/lease_queue_test.go | TestLeaseQueueResourcesAreIndependent | Lease Queue Resources Are Independent | 0.000s pass | CONVERT | LEASE: exclusivity, FIFO, expiry/renewal and restart; preserve the named behavior as a scenario assertion |
| internal/hub/lease_queue_test.go | TestLeaseQueueCancelPersistAndExpire | Lease Queue Cancel Persist And Expire | 0.000s pass | CONVERT | LEASE: exclusivity, FIFO, expiry/renewal and restart; preserve the named behavior as a scenario assertion |
| internal/hub/lease_queue_test.go | TestLeaseQueueOverRPC | Lease Queue Over RPC | 0.310s pass | CONVERT | LEASE: exclusivity, FIFO, expiry/renewal and restart; preserve the named behavior as a scenario assertion |
| internal/hub/leases_test.go | TestLeaseExclusivityAndExpiry | Lease Exclusivity And Expiry | 0.000s pass | CONVERT | LEASE: exclusivity, FIFO, expiry/renewal and restart; preserve the named behavior as a scenario assertion |
| internal/hub/leases_test.go | TestLeaseRunSweepsExpired | Lease Run Sweeps Expired | 1.010s pass | CONVERT | LEASE: exclusivity, FIFO, expiry/renewal and restart; preserve the named behavior as a scenario assertion |
| internal/hub/leases_test.go | TestLeaseValidationAndRelease | Lease Validation And Release | 0.000s pass | CONVERT | LEASE: exclusivity, FIFO, expiry/renewal and restart; preserve the named behavior as a scenario assertion |
| internal/hub/leases_test.go | TestLeasePersistence | Lease Persistence | 0.000s pass | CONVERT | LEASE: exclusivity, FIFO, expiry/renewal and restart; preserve the named behavior as a scenario assertion |
| internal/hub/leases_test.go | TestLeaseLoadErrors | Lease Load Errors | 0.010s pass | CONVERT | LEASE: exclusivity, FIFO, expiry/renewal and restart; preserve the named behavior as a scenario assertion |
| internal/hub/leases_test.go | TestPickNode | Pick Node | 0.000s pass | CONVERT | LEASE: exclusivity, FIFO, expiry/renewal and restart; preserve the named behavior as a scenario assertion |
| internal/hub/leases_test.go | TestNodeFitsAccelerators | Node Fits Accelerators | 0.000s pass | CONVERT | LEASE: exclusivity, FIFO, expiry/renewal and restart; preserve the named behavior as a scenario assertion |
| internal/hub/leases_test.go | TestParseResource | Parse Resource | 0.000s pass | CONVERT | LEASE: exclusivity, FIFO, expiry/renewal and restart; preserve the named behavior as a scenario assertion |
| internal/hub/leases_test.go | TestLeaseServiceEndToEnd | Lease Service End To End | 0.430s pass | CONVERT | LEASE: exclusivity, FIFO, expiry/renewal and restart; preserve the named behavior as a scenario assertion |
| internal/hub/leases_test.go | TestLeasesSurviveHubRestart | Leases Survive Hub Restart | 0.300s pass | CONVERT | LEASE: exclusivity, FIFO, expiry/renewal and restart; preserve the named behavior as a scenario assertion |
| internal/hub/leases_test.go | TestLeaseTTLHoldsAcrossRestart | Lease TTLHolds Across Restart | 0.000s pass | CONVERT | LEASE: exclusivity, FIFO, expiry/renewal and restart; preserve the named behavior as a scenario assertion |
| internal/hub/localmodel_test.go | TestServedLocally | Served Locally | 0.000s pass | CONVERT | CLI: real binary API commands, JSON output, cancellation and error exit codes; preserve the named behavior as a scenario assertion |
| internal/hub/mail_test.go | TestRenderMessage | Render Message | 0.000s pass | DELETE | implementation/output-shape assertion; validate externally visible behavior in MAIL: authenticated send/reply/ack, idle delivery, FIFO notices and restart |
| internal/hub/mail_test.go | TestMailWaitsForIdleAndIsAuthenticated | Mail Waits For Idle And Is Authenticated | 0.000s skip | CONVERT | MAIL: authenticated send/reply/ack, idle delivery, FIFO notices and restart; preserve the named behavior as a scenario assertion |
| internal/hub/mail_test.go | TestMailReplyKeepsThreadAndAcks | Mail Reply Keeps Thread And Acks | 0.000s skip | CONVERT | MAIL: authenticated send/reply/ack, idle delivery, FIFO notices and restart; preserve the named behavior as a scenario assertion |
| internal/hub/mail_test.go | TestMailSendValidationAndDedupe | Mail Send Validation And Dedupe | 0.000s skip | CONVERT | MAIL: authenticated send/reply/ack, idle delivery, FIFO notices and restart; preserve the named behavior as a scenario assertion |
| internal/hub/mail_test.go | TestMailSurvivesRestartAndTerminalSleevesUseInbox | Mail Survives Restart And Terminal Sleeves Use Inbox | 0.000s skip | CONVERT | MAIL: authenticated send/reply/ack, idle delivery, FIFO notices and restart; preserve the named behavior as a scenario assertion |
| internal/hub/mail_test.go | TestMailEscalation | Mail Escalation | 0.000s pass | CONVERT | MAIL: authenticated send/reply/ack, idle delivery, FIFO notices and restart; preserve the named behavior as a scenario assertion |
| internal/hub/mail_test.go | TestMailProjectAddressing | Mail Project Addressing | 0.000s skip | CONVERT | MAIL: authenticated send/reply/ack, idle delivery, FIFO notices and restart; preserve the named behavior as a scenario assertion |
| internal/hub/mail_test.go | TestLeaseGrantSendsNotice | Lease Grant Sends Notice | 0.000s skip | CONVERT | LEASE: exclusivity, FIFO, expiry/renewal and restart; preserve the named behavior as a scenario assertion |
| internal/hub/mail_test.go | TestMailRequestAcksByDefault | Mail Request Acks By Default | 0.000s skip | CONVERT | MAIL: authenticated send/reply/ack, idle delivery, FIFO notices and restart; preserve the named behavior as a scenario assertion |
| internal/hub/mail_test.go | TestMailListThreads | Mail List Threads | 0.000s skip | CONVERT | MAIL: authenticated send/reply/ack, idle delivery, FIFO notices and restart; preserve the named behavior as a scenario assertion |
| internal/hub/mail_test.go | TestDeskEndedNotice | Desk Ended Notice | 0.000s skip | CONVERT | MAIL: authenticated send/reply/ack, idle delivery, FIFO notices and restart; preserve the named behavior as a scenario assertion |
| internal/hub/mail_test.go | TestEnvoyMailQueue | Envoy Mail Queue | 0.010s pass | CONVERT | MAIL: authenticated send/reply/ack, idle delivery, FIFO notices and restart; preserve the named behavior as a scenario assertion |
| internal/hub/mail_test.go | TestEnvoyQueueRidesAlongWithOwner | Envoy Queue Rides Along With Owner | 0.000s skip | CONVERT | MAIL: authenticated send/reply/ack, idle delivery, FIFO notices and restart; preserve the named behavior as a scenario assertion |
| internal/hub/nodeops_test.go | TestFileTransferThroughHub | File Transfer Through Hub | 0.750s pass | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/hub/nodeops_test.go | TestFileScopesAndAudit | File Scopes And Audit | 0.380s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/hub/nodeops_test.go | TestDeskAutoLease | Desk Auto Lease | 0.280s pass | CONVERT | LEASE: exclusivity, FIFO, expiry/renewal and restart; preserve the named behavior as a scenario assertion |
| internal/hub/nodeops_test.go | TestDeskLeaseRule | Desk Lease Rule | 0.290s pass | CONVERT | LEASE: exclusivity, FIFO, expiry/renewal and restart; preserve the named behavior as a scenario assertion |
| internal/hub/nodeops_test.go | TestApplyUpdateRelay | Apply Update Relay | 0.310s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/hub/nodes_test.go | TestInventory | Inventory | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/hub/nodes_test.go | TestInventoryLoadErrors | Inventory Load Errors | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/hub/nodes_test.go | TestAgentURL | Agent URL | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/hub/nodes_test.go | TestAlerts | Alerts | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/hub/nodes_test.go | TestMonitorFreshness | Monitor Freshness | 0.260s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/hub/nodes_test.go | TestNodeServiceEndToEnd | Node Service End To End | 0.290s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/hub/nodes_test.go | TestInvitedNodeLifecycle | Invited Node Lifecycle | 0.260s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/hub/nodes_test.go | TestPollRecoversFromDeadPooledConnection | Poll Recovers From Dead Pooled Connection | 1.260s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/hub/nodes_test.go | TestPollAllKeepsNodeAddedMidRound | Poll All Keeps Node Added Mid Round | 0.250s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/hub/nodes_test.go | TestValidNodeAddr | Valid Node Addr | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/hub/nodes_test.go | TestNodeTokenNotSentOnRedirect | Node Token Not Sent On Redirect | 0.010s pass | CONVERT | AUTH: scoped bearer, denial, revoke, expiry, audit and restart; preserve the named behavior as a scenario assertion |
| internal/hub/nodes_test.go | TestMigrateTiers | Migrate Tiers | 0.010s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/hub/nodeupdate_test.go | TestPickBinary | Pick Binary | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/hub/nodeupdate_test.go | TestNeedsUpdate | Needs Update | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/hub/nodeupdate_test.go | TestUpdateStep | Update Step | 0.000s pass | DELETE | implementation/output-shape assertion; validate externally visible behavior in NODE: real agent install/update/rollback, file confinement and transfer |
| internal/hub/nodeupdate_test.go | TestUpdateNodeStreamsProgress | Update Node Streams Progress | 2.770s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/hub/pair_test.go | TestNewPairLinks | New Pair Links | 0.000s pass | CONVERT | CLI: real binary API commands, JSON output, cancellation and error exit codes; preserve the named behavior as a scenario assertion |
| internal/hub/pair_test.go | TestNewPairLinksRejectsBadURL | New Pair Links Rejects Bad URL | 0.000s pass | CONVERT | CLI: real binary API commands, JSON output, cancellation and error exit codes; preserve the named behavior as a scenario assertion |
| internal/hub/pair_test.go | TestWriteQR | Write QR | 0.000s pass | CONVERT | CLI: real binary API commands, JSON output, cancellation and error exit codes; preserve the named behavior as a scenario assertion |
| internal/hub/quota_test.go | TestFreshness | Freshness | 0.000s pass | CONVERT | QUOTA: hub polls local provider HTTP fixtures and exposes freshness/errors; preserve the named behavior as a scenario assertion |
| internal/hub/quota_test.go | TestStaleByAgeAlone | Stale By Age Alone | 0.000s pass | CONVERT | QUOTA: hub polls local provider HTTP fixtures and exposes freshness/errors; preserve the named behavior as a scenario assertion |
| internal/hub/quota_test.go | TestRefreshRateLimit | Refresh Rate Limit | 0.000s pass | CONVERT | QUOTA: hub polls local provider HTTP fixtures and exposes freshness/errors; preserve the named behavior as a scenario assertion |
| internal/hub/quota_test.go | TestConcurrentRefreshCollapses | Concurrent Refresh Collapses | 0.050s pass | CONVERT | QUOTA: hub polls local provider HTTP fixtures and exposes freshness/errors; preserve the named behavior as a scenario assertion |
| internal/hub/quota_test.go | TestCancelledRefreshNotRecorded | Cancelled Refresh Not Recorded | 0.000s pass | CONVERT | QUOTA: hub polls local provider HTTP fixtures and exposes freshness/errors; preserve the named behavior as a scenario assertion |
| internal/hub/quota_test.go | TestProjection | Projection | 0.000s pass | CONVERT | QUOTA: hub polls local provider HTTP fixtures and exposes freshness/errors; preserve the named behavior as a scenario assertion |
| internal/hub/quota_test.go | TestWindowLength | Window Length | 0.000s pass | CONVERT | QUOTA: hub polls local provider HTTP fixtures and exposes freshness/errors; preserve the named behavior as a scenario assertion |
| internal/hub/quota_test.go | TestGetQuotaService | Get Quota Service | 0.000s pass | CONVERT | QUOTA: hub polls local provider HTTP fixtures and exposes freshness/errors; preserve the named behavior as a scenario assertion |
| internal/hub/quota_test.go | TestQuotaUpdatedEvent | Quota Updated Event | 0.050s pass | CONVERT | QUOTA: hub polls local provider HTTP fixtures and exposes freshness/errors; preserve the named behavior as a scenario assertion |
| internal/hub/quota_test.go | TestStrictJSONRejectsUnknownFields | Strict JSONRejects Unknown Fields | 0.010s pass | CONVERT | QUOTA: hub polls local provider HTTP fixtures and exposes freshness/errors; preserve the named behavior as a scenario assertion |
| internal/hub/quota_test.go | TestStrictJSONTypedError | Strict JSONTyped Error | 0.000s pass | CONVERT | QUOTA: hub polls local provider HTTP fixtures and exposes freshness/errors; preserve the named behavior as a scenario assertion |
| internal/hub/resleeve_test.go | TestResleeveHandsOffToANewHarness | Resleeve Hands Off To ANew Harness | 0.010s pass | CONVERT | SLEEVE: real sandbox lifecycle, secret boundaries, symlinks, git clone, OOM and toolchains; preserve the named behavior as a scenario assertion |
| internal/hub/resleeve_test.go | TestResleeveRefuses | Resleeve Refuses | 0.000s skip | CONVERT | SLEEVE: real sandbox lifecycle, secret boundaries, symlinks, git clone, OOM and toolchains; preserve the named behavior as a scenario assertion |
| internal/hub/resleeve_test.go | TestResleeveTarget | Resleeve Target | 0.000s pass | CONVERT | SLEEVE: real sandbox lifecycle, secret boundaries, symlinks, git clone, OOM and toolchains; preserve the named behavior as a scenario assertion |
| internal/hub/selection_test.go | TestSelectionRequiresIdle | Selection Requires Idle | 0.000s pass | CONVERT | CLI: real binary API commands, JSON output, cancellation and error exit codes; preserve the named behavior as a scenario assertion |
| internal/hub/selection_test.go | TestSelectionStatusReportsRequestedAndEffective | Selection Status Reports Requested And Effective | 0.000s pass | CONVERT | CLI: real binary API commands, JSON output, cancellation and error exit codes; preserve the named behavior as a scenario assertion |
| internal/hub/sessions_test.go | TestSessionRPC | Session RPC | 0.130s pass | CONVERT | TERMINAL: real tmux lifecycle, WebSocket I/O, exit and respawn; preserve the named behavior as a scenario assertion |
| internal/hub/sessions_test.go | TestSessionEvents | Session Events | 0.100s pass | CONVERT | TERMINAL: real tmux lifecycle, WebSocket I/O, exit and respawn; preserve the named behavior as a scenario assertion |
| internal/hub/sessions_test.go | TestSessionExitEvents | Session Exit Events | 0.820s pass | CONVERT | TERMINAL: real tmux lifecycle, WebSocket I/O, exit and respawn; preserve the named behavior as a scenario assertion |
| internal/hub/sessions_test.go | TestDeadRetentionSweep | Dead Retention Sweep | 0.970s pass | CONVERT | TERMINAL: real tmux lifecycle, WebSocket I/O, exit and respawn; preserve the named behavior as a scenario assertion |
| internal/hub/sessions_test.go | TestDeadRetentionDisabled | Dead Retention Disabled | 1.270s pass | CONVERT | TERMINAL: real tmux lifecycle, WebSocket I/O, exit and respawn; preserve the named behavior as a scenario assertion |
| internal/hub/sessions_test.go | TestTerminalAuthAndErrors | Terminal Auth And Errors | 0.120s pass | CONVERT | AUTH: scoped bearer, denial, revoke, expiry, audit and restart; preserve the named behavior as a scenario assertion |
| internal/hub/sessions_test.go | TestTerminalAttachLifecycle | Terminal Attach Lifecycle | 0.580s pass | CONVERT | TERMINAL: real tmux lifecycle, WebSocket I/O, exit and respawn; preserve the named behavior as a scenario assertion |
| internal/hub/sessions_test.go | TestTerminalExitedSessionIsReadOnly | Terminal Exited Session Is Read Only | 0.580s pass | CONVERT | TERMINAL: real tmux lifecycle, WebSocket I/O, exit and respawn; preserve the named behavior as a scenario assertion |
| internal/hub/sessions_test.go | TestTerminalSessionExit | Terminal Session Exit | 0.580s pass | CONVERT | TERMINAL: real tmux lifecycle, WebSocket I/O, exit and respawn; preserve the named behavior as a scenario assertion |
| internal/hub/sleeves_test.go | TestSleeveRPC | Sleeve RPC | 0.000s skip | CONVERT | SLEEVE: real sandbox lifecycle, secret boundaries, symlinks, git clone, OOM and toolchains; preserve the named behavior as a scenario assertion |
| internal/hub/suggest_test.go | TestRankHarnesses | Rank Harnesses | 0.000s pass | CONVERT | CLI: real binary API commands, JSON output, cancellation and error exit codes; preserve the named behavior as a scenario assertion |
| internal/hub/suggest_test.go | TestRankHarnessesNeverCountsMissingDataAsSpare | Rank Harnesses Never Counts Missing Data As Spare | 0.000s pass | CONVERT | CLI: real binary API commands, JSON output, cancellation and error exit codes; preserve the named behavior as a scenario assertion |
| internal/hub/suggest_test.go | TestRankHarnessesLocalReason | Rank Harnesses Local Reason | 0.010s pass | CONVERT | CLI: real binary API commands, JSON output, cancellation and error exit codes; preserve the named behavior as a scenario assertion |
| internal/hub/suggest_test.go | TestRankHarnessesReasonText | Rank Harnesses Reason Text | 0.000s pass | CONVERT | CLI: real binary API commands, JSON output, cancellation and error exit codes; preserve the named behavior as a scenario assertion |
| internal/hub/suggest_test.go | TestSuggestService | Suggest Service | 0.010s pass | CONVERT | CLI: real binary API commands, JSON output, cancellation and error exit codes; preserve the named behavior as a scenario assertion |
| internal/hub/tailnet_test.go | TestTailnetGetStatus | Tailnet Get Status | 0.010s pass | CONVERT | TAILNET: isolated headscale, two nodes, ACL, relay and restart; preserve the named behavior as a scenario assertion |
| internal/hub/workspaces_test.go | TestListWorkspaces | List Workspaces | 0.010s pass | CONVERT | CLI: real binary API commands, JSON output, cancellation and error exit codes; preserve the named behavior as a scenario assertion |
| internal/keylog/keylog_test.go | TestReloadCompactsAndExpires | Reload Compacts And Expires | 0.010s pass | CONVERT | DESK: real input activity monitoring gates desktop consent and revocation; preserve the named behavior as a scenario assertion |
| internal/keylog/keylog_test.go | TestAppendSurvivesReopenAndCompactsWhenBloated | Append Survives Reopen And Compacts When Bloated | 0.010s pass | CONVERT | DESK: real input activity monitoring gates desktop consent and revocation; preserve the named behavior as a scenario assertion |
| internal/mailbox/mailbox_test.go | TestSendDedupeAndThreads | Send Dedupe And Threads | 0.010s pass | CONVERT | MAIL: authenticated send/reply/ack, idle delivery, FIFO notices and restart; preserve the named behavior as a scenario assertion |
| internal/mailbox/mailbox_test.go | TestInboxAndAck | Inbox And Ack | 0.000s pass | CONVERT | MAIL: authenticated send/reply/ack, idle delivery, FIFO notices and restart; preserve the named behavior as a scenario assertion |
| internal/mailbox/mailbox_test.go | TestPersistenceAcrossRestart | Persistence Across Restart | 0.000s pass | CONVERT | MAIL: authenticated send/reply/ack, idle delivery, FIFO notices and restart; preserve the named behavior as a scenario assertion |
| internal/mailbox/mailbox_test.go | TestResolveOnce | Resolve Once | 0.000s pass | CONVERT | MAIL: authenticated send/reply/ack, idle delivery, FIFO notices and restart; preserve the named behavior as a scenario assertion |
| internal/mailbox/mailbox_test.go | TestOverdueTimeline | Overdue Timeline | 0.000s pass | CONVERT | MAIL: authenticated send/reply/ack, idle delivery, FIFO notices and restart; preserve the named behavior as a scenario assertion |
| internal/mailbox/mailbox_test.go | TestOpenRequests | Open Requests | 0.000s pass | CONVERT | MAIL: authenticated send/reply/ack, idle delivery, FIFO notices and restart; preserve the named behavior as a scenario assertion |
| internal/mailbox/mailbox_test.go | TestInvolving | Involving | 0.000s pass | CONVERT | MAIL: authenticated send/reply/ack, idle delivery, FIFO notices and restart; preserve the named behavior as a scenario assertion |
| internal/mailbox/mailbox_test.go | TestThreadsSummary | Threads Summary | 0.000s pass | CONVERT | MAIL: authenticated send/reply/ack, idle delivery, FIFO notices and restart; preserve the named behavior as a scenario assertion |
| internal/mcpbridge/mcpbridge_test.go | TestEveryRPCHasDescription | Every RPCHas Description | 0.040s pass | CONVERT | MCP: binary stdio handshake, tools, scopes and streamed errors through hub; preserve the named behavior as a scenario assertion |
| internal/mcpbridge/mcpbridge_test.go | TestOwnerOnlyServicesNotExposed | Owner Only Services Not Exposed | 0.010s pass | CONVERT | MCP: binary stdio handshake, tools, scopes and streamed errors through hub; preserve the named behavior as a scenario assertion |
| internal/mcpbridge/mcpbridge_test.go | TestImageMatchesGeneratedCode | Image Matches Generated Code | 0.020s pass | CONVERT | MCP: binary stdio handshake, tools, scopes and streamed errors through hub; preserve the named behavior as a scenario assertion |
| internal/mcpbridge/mcpbridge_test.go | TestToolsListGolden | Tools List Golden | 0.080s pass | CONVERT | MCP: binary stdio handshake, tools, scopes and streamed errors through hub; preserve the named behavior as a scenario assertion |
| internal/mcpbridge/mcpbridge_test.go | TestSchemaShapes | Schema Shapes | 0.040s pass | CONVERT | MCP: binary stdio handshake, tools, scopes and streamed errors through hub; preserve the named behavior as a scenario assertion |
| internal/mcpbridge/mcpbridge_test.go | TestCallTools | Call Tools | 0.070s pass | CONVERT | MCP: binary stdio handshake, tools, scopes and streamed errors through hub; preserve the named behavior as a scenario assertion |
| internal/mcpbridge/mcpbridge_test.go | TestFilter | Filter | 0.150s pass | CONVERT | MCP: binary stdio handshake, tools, scopes and streamed errors through hub; preserve the named behavior as a scenario assertion |
| internal/mcpbridge/mcpbridge_test.go | TestStreamableHTTP | Streamable HTTP | 0.050s pass | CONVERT | MCP: binary stdio handshake, tools, scopes and streamed errors through hub; preserve the named behavior as a scenario assertion |
| internal/node/accel_live_test.go | TestAccelLive | Accel Live | 0.300s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/accel_test.go | TestSysfsInventory | Sysfs Inventory | 0.070s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/accel_test.go | TestSysfsEmpty | Sysfs Empty | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/accel_test.go | TestWinAccels | Win Accels | 0.010s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/accel_test.go | TestMergeGPUs | Merge GPUs | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/accel_test.go | TestParseXPUSMI | Parse XPUSMI | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/accel_windows_test.go | TestListDevicesWindows | List Devices Windows | not measured | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/deskguard_test.go | TestGuardCheck | Guard Check | 0.010s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/deskguard_test.go | TestGuardRefusesInputCommands | Guard Refuses Input Commands | 0.010s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/deskguard_test.go | TestGuardStopsTypingWhenUserReturns | Guard Stops Typing When User Returns | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/deskguard_test.go | TestGuardLocking | Guard Locking | 0.030s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/deskguard_test.go | TestGuardPolicyPersistence | Guard Policy Persistence | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/deskguard_test.go | TestBlockedInputIsFailedPrecondition | Blocked Input Is Failed Precondition | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/files_fuzz_test.go | FuzzConfinedFilePaths | Confined File Paths | 0.010s pass | KEEP-FUZZ | seed corpus on commits; bounded campaigns separately |
| internal/node/files_test.go | TestFilesResolve | Files Resolve | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/files_test.go | TestFilesPutProtocol | Files Put Protocol | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/files_test.go | TestDeskErrorsAndValidation | Desk Errors And Validation | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/jobs_test.go | TestMain | Main | not measured | KEEP-E2E | runner infrastructure; retain while any package tests use it |
| internal/node/jobs_test.go | TestNewJobsClearsStaleScratchButKeepsRepos | New Jobs Clears Stale Scratch But Keeps Repos | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/jobs_test.go | TestUpload | Upload | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/jobs_test.go | TestValidateRun | Validate Run | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/jobs_test.go | TestMatchArtifacts | Match Artifacts | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/jobs_test.go | TestRunOutputExitAndEnv | Run Output Exit And Env | 0.020s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/jobs_test.go | TestRunTimeoutKillsTree | Run Timeout Kills Tree | 0.320s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/jobs_test.go | TestRunStubbornProcessIsKilledAfterGrace | Run Stubborn Process Is Killed After Grace | 3.520s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/jobs_test.go | TestRunSurvivesDisconnectAndAttachResumes | Run Survives Disconnect And Attach Resumes | 1.020s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/jobs_test.go | TestAttachErrors | Attach Errors | 0.050s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/jobs_test.go | TestFinishedJobsAreSweptAfterRetention | Finished Jobs Are Swept After Retention | 0.030s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/jobs_test.go | TestRunWaitsForSlotAndCancelWhileQueued | Run Waits For Slot And Cancel While Queued | 1.050s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/jobs_test.go | TestRunDuplicateJobID | Run Duplicate Job ID | 0.010s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/jobs_test.go | TestRunGitWithoutBundleOrCommitFails | Run Git Without Bundle Or Commit Fails | 0.040s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/jobs_test.go | TestRunLogIsCapped | Run Log Is Capped | 0.420s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/jobs_test.go | TestAgentJobRPCsNeedAuthAndEnablement | Agent Job RPCs Need Auth And Enablement | 0.020s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/jobs_test.go | TestExtractZipRefusesUnsafeEntries | Extract Zip Refuses Unsafe Entries | 0.010s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/node_test.go | TestParseNvidiaSMI | Parse Nvidia SMI | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/node_test.go | TestParseROCmSMI | Parse ROCm SMI | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/node_test.go | TestParseDF | Parse DF | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/node_test.go | TestApplyStats | Apply Stats | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/node_test.go | TestNegotiateAPI | Negotiate API | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/node_test.go | TestCPUBusyPercent | CPUBusy Percent | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/node_test.go | TestKeepPartition | Keep Partition | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/node_test.go | TestDedupeFilesystems | Dedupe Filesystems | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/node_test.go | TestDirSize | Dir Size | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/node_test.go | TestEngineMetrics | Engine Metrics | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/node_test.go | TestEngineAbsent | Engine Absent | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/node_test.go | TestCollectLocal | Collect Local | 0.250s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/node_test.go | TestAgent | Agent | 0.350s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/node_test.go | TestAndroidSystemMount | Android System Mount | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/node_test.go | TestFSProberBlockedMount | FSProber Blocked Mount | 0.030s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/node_test.go | TestMarkPrimary | Mark Primary | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/process_unix_test.go | TestCommandFor | Command For | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/runner_test.go | TestWindowsCommandLine | Windows Command Line | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/runner_test.go | TestMergeEnvAndBlock | Merge Env And Block | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/runner_test.go | TestCPURate | CPURate | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/runner_test.go | TestRunnerPassword | Runner Password | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/runner_test.go | TestRunnerPlanText | Runner Plan Text | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/runner_test.go | TestReadRunnerConfig | Read Runner Config | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/sandbox_unix_test.go | TestSandboxPlan | Sandbox Plan | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/sandbox_unix_test.go | TestSandboxedJobLive | Sandboxed Job Live | 0.010s skip | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/sandbox_unix_test.go | TestSandboxedJobLimitsAndCancelLive | Sandboxed Job Limits And Cancel Live | 0.000s skip | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/sandbox_unix_test.go | TestSandboxJobDirUnderHomeLeavesNoMountPoints | Sandbox Job Dir Under Home Leaves No Mount Points | 0.000s skip | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/update_test.go | TestSwapAndRollbackPlans | Swap And Rollback Plans | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/update_test.go | TestDecideStartup | Decide Startup | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/update_test.go | TestPlanRestart | Plan Restart | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/update_test.go | TestRunOpsUndoesRenames | Run Ops Undoes Renames | 0.000s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/update_test.go | TestApplyUpdateEndToEnd | Apply Update End To End | 0.520s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/update_test.go | TestUnconfirmedUpdateRollsBack | Unconfirmed Update Rolls Back | 0.520s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/node/update_test.go | TestApplyUpdateRefusals | Apply Update Refusals | 0.050s pass | CONVERT | NODE: real agent install/update/rollback, file confinement and transfer; preserve the named behavior as a scenario assertion |
| internal/orchestrator/orchestrator_test.go | TestDefaultScopes | Default Scopes | 0.030s pass | CONVERT | MAIL: Envoy identity, wakeup, grants and mailbox delivery; preserve the named behavior as a scenario assertion |
| internal/orchestrator/orchestrator_test.go | TestDefaultScopesDropDestructive | Default Scopes Drop Destructive | 0.000s pass | CONVERT | MAIL: Envoy identity, wakeup, grants and mailbox delivery; preserve the named behavior as a scenario assertion |
| internal/orchestrator/orchestrator_test.go | TestRenderAgentsGolden | Render Agents Golden | 0.010s pass | CONVERT | MAIL: Envoy identity, wakeup, grants and mailbox delivery; preserve the named behavior as a scenario assertion |
| internal/orchestrator/orchestrator_test.go | TestRenderAgentsStableOrdering | Render Agents Stable Ordering | 0.000s pass | CONVERT | MAIL: Envoy identity, wakeup, grants and mailbox delivery; preserve the named behavior as a scenario assertion |
| internal/orchestrator/orchestrator_test.go | TestSpawnMintsAndRevokesOnRespawn | Spawn Mints And Revokes On Respawn | 0.090s pass | CONVERT | MAIL: Envoy identity, wakeup, grants and mailbox delivery; preserve the named behavior as a scenario assertion |
| internal/orchestrator/orchestrator_test.go | TestSpawnFailureRevokesNewToken | Spawn Failure Revokes New Token | 0.040s pass | CONVERT | MAIL: Envoy identity, wakeup, grants and mailbox delivery; preserve the named behavior as a scenario assertion |
| internal/orchestrator/orchestrator_test.go | TestSpawnCustomScopesAndHarness | Spawn Custom Scopes And Harness | 0.020s pass | CONVERT | MAIL: Envoy identity, wakeup, grants and mailbox delivery; preserve the named behavior as a scenario assertion |
| internal/orchestrator/orchestrator_test.go | TestSpawnAutoPicksSuggestion | Spawn Auto Picks Suggestion | 0.020s pass | CONVERT | MAIL: Envoy identity, wakeup, grants and mailbox delivery; preserve the named behavior as a scenario assertion |
| internal/orchestrator/orchestrator_test.go | TestWriteContextMovesForeignClaudeMdAside | Write Context Moves Foreign Claude Md Aside | 0.000s pass | CONVERT | MAIL: Envoy identity, wakeup, grants and mailbox delivery; preserve the named behavior as a scenario assertion |
| internal/orchestrator/orchestrator_test.go | TestSpawnContextIsCacheStable | Spawn Context Is Cache Stable | 0.030s pass | CONVERT | MAIL: Envoy identity, wakeup, grants and mailbox delivery; preserve the named behavior as a scenario assertion |
| internal/quota/quota_test.go | TestClaude | Claude | 0.020s pass | CONVERT | QUOTA: hub polls local provider HTTP fixtures and exposes freshness/errors; preserve the named behavior as a scenario assertion |
| internal/quota/quota_test.go | TestCodexAppServer | Codex App Server | 0.010s pass | CONVERT | QUOTA: hub polls local provider HTTP fixtures and exposes freshness/errors; preserve the named behavior as a scenario assertion |
| internal/quota/quota_test.go | TestCodexWhamFallbackAndExpiry | Codex Wham Fallback And Expiry | 0.000s pass | CONVERT | QUOTA: hub polls local provider HTTP fixtures and exposes freshness/errors; preserve the named behavior as a scenario assertion |
| internal/quota/quota_test.go | TestGrok | Grok | 0.010s pass | CONVERT | QUOTA: hub polls local provider HTTP fixtures and exposes freshness/errors; preserve the named behavior as a scenario assertion |
| internal/quota/quota_test.go | TestAntigravityParse | Antigravity Parse | 0.000s pass | CONVERT | QUOTA: hub polls local provider HTTP fixtures and exposes freshness/errors; preserve the named behavior as a scenario assertion |
| internal/quota/quota_test.go | TestLocal | Local | 0.000s pass | CONVERT | QUOTA: hub polls local provider HTTP fixtures and exposes freshness/errors; preserve the named behavior as a scenario assertion |
| internal/quota/quota_test.go | TestReadAll | Read All | 0.110s pass | CONVERT | QUOTA: hub polls local provider HTTP fixtures and exposes freshness/errors; preserve the named behavior as a scenario assertion |
| internal/session/dbg_test.go | TestInstantExitsGetStatus | Instant Exits Get Status | 2.200s pass | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/session/session_test.go | TestMain | Main | not measured | KEEP-E2E | runner infrastructure; retain while any package tests use it |
| internal/session/session_test.go | TestValidName | Valid Name | 0.000s pass | CONVERT | TERMINAL: real tmux lifecycle, WebSocket I/O, exit and respawn; preserve the named behavior as a scenario assertion |
| internal/session/session_test.go | TestParseList | Parse List | 0.000s pass | CONVERT | TERMINAL: real tmux lifecycle, WebSocket I/O, exit and respawn; preserve the named behavior as a scenario assertion |
| internal/session/session_test.go | TestEscapeArg | Escape Arg | 0.000s pass | CONVERT | TERMINAL: real tmux lifecycle, WebSocket I/O, exit and respawn; preserve the named behavior as a scenario assertion |
| internal/session/session_test.go | TestStripDeadBanner | Strip Dead Banner | 0.000s pass | CONVERT | TERMINAL: real tmux lifecycle, WebSocket I/O, exit and respawn; preserve the named behavior as a scenario assertion |
| internal/session/session_test.go | TestTail | Tail | 0.000s pass | CONVERT | TERMINAL: real tmux lifecycle, WebSocket I/O, exit and respawn; preserve the named behavior as a scenario assertion |
| internal/session/session_test.go | TestManagerLifecycle | Manager Lifecycle | 0.110s pass | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/session/session_test.go | TestSessionExit | Session Exit | 0.420s pass | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/session/session_test.go | TestRespawn | Respawn | 0.170s pass | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/session/session_test.go | TestAttachCmdExitedIsReadOnly | Attach Cmd Exited Is Read Only | 0.100s pass | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/session/session_test.go | TestCaptureClampsLines | Capture Clamps Lines | 0.020s pass | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/session/session_test.go | TestRunLimitsOutput | Run Limits Output | 0.020s pass | CONVERT | TERMINAL: real tmux lifecycle, WebSocket I/O, exit and respawn; preserve the named behavior as a scenario assertion |
| internal/sleeve/git_test.go | TestGitRunsRepositoryProgramsInAJail | Git Runs Repository Programs In AJail | 0.280s pass | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/sleeve/layout_test.go | TestSharedAuthVisibleAcrossSleeves | Shared Auth Visible Across Sleeves | 0.000s pass | CONVERT | SLEEVE: real sandbox lifecycle, secret boundaries, symlinks, git clone, OOM and toolchains; preserve the named behavior as a scenario assertion |
| internal/sleeve/layout_test.go | TestLayoutApply | Layout Apply | 0.000s pass | CONVERT | SLEEVE: real sandbox lifecycle, secret boundaries, symlinks, git clone, OOM and toolchains; preserve the named behavior as a scenario assertion |
| internal/sleeve/layout_test.go | TestLayoutEnv | Layout Env | 0.000s pass | CONVERT | SLEEVE: real sandbox lifecycle, secret boundaries, symlinks, git clone, OOM and toolchains; preserve the named behavior as a scenario assertion |
| internal/sleeve/layout_test.go | TestCodexConfigSeed | Codex Config Seed | 0.000s pass | CONVERT | SLEEVE: real sandbox lifecycle, secret boundaries, symlinks, git clone, OOM and toolchains; preserve the named behavior as a scenario assertion |
| internal/sleeve/layout_test.go | TestProfileKeepsSleevePathInLoginShells | Profile Keeps Sleeve Path In Login Shells | 0.020s pass | CONVERT | SLEEVE: real sandbox lifecycle, secret boundaries, symlinks, git clone, OOM and toolchains; preserve the named behavior as a scenario assertion |
| internal/sleeve/login_test.go | TestLoginPlan | Login Plan | 0.000s pass | CONVERT | SLEEVE: real sandbox lifecycle, secret boundaries, symlinks, git clone, OOM and toolchains; preserve the named behavior as a scenario assertion |
| internal/sleeve/login_test.go | TestLoggedIn | Logged In | 0.000s pass | CONVERT | SLEEVE: real sandbox lifecycle, secret boundaries, symlinks, git clone, OOM and toolchains; preserve the named behavior as a scenario assertion |
| internal/sleeve/manager_test.go | TestMain | Main | not measured | KEEP-E2E | runner infrastructure; retain while any package tests use it |
| internal/sleeve/manager_test.go | TestSleeveLifecycle | Sleeve Lifecycle | 0.000s skip | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/sleeve/manager_test.go | TestMemoryLimitEnforced | Memory Limit Enforced | 0.000s skip | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/sleeve/manager_test.go | TestSleeveExitKeepsSession | Sleeve Exit Keeps Session | 0.000s skip | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/sleeve/manager_test.go | TestSleeveExitReasons | Sleeve Exit Reasons | 0.000s skip | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/sleeve/manager_test.go | TestSpawnClonesGitURL | Spawn Clones Git URL | 0.000s skip | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/sleeve/manager_test.go | TestSpawnRejectsBadRequests | Spawn Rejects Bad Requests | 0.000s skip | CONVERT | SLEEVE: real sandbox lifecycle, secret boundaries, symlinks, git clone, OOM and toolchains; preserve the named behavior as a scenario assertion |
| internal/sleeve/manager_test.go | TestKillRestoresRecordWhenSaveFails | Kill Restores Record When Save Fails | 0.000s skip | CONVERT | SLEEVE: real sandbox lifecycle, secret boundaries, symlinks, git clone, OOM and toolchains; preserve the named behavior as a scenario assertion |
| internal/sleeve/manager_test.go | TestFailedStartLeavesNothingRunning | Failed Start Leaves Nothing Running | 0.000s skip | CONVERT | SLEEVE: real sandbox lifecycle, secret boundaries, symlinks, git clone, OOM and toolchains; preserve the named behavior as a scenario assertion |
| internal/sleeve/mode_test.go | TestResolveMode | Resolve Mode | 0.000s pass | CONVERT | SLEEVE: real sandbox lifecycle, secret boundaries, symlinks, git clone, OOM and toolchains; preserve the named behavior as a scenario assertion |
| internal/sleeve/mode_test.go | TestTerminalArgs | Terminal Args | 0.000s pass | DELETE | implementation/output-shape assertion; validate externally visible behavior in SLEEVE: real sandbox lifecycle, secret boundaries, symlinks, git clone, OOM and toolchains |
| internal/sleeve/mode_test.go | TestChatSleeveLifecycle | Chat Sleeve Lifecycle | 0.000s skip | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/sleeve/path_fuzz_test.go | FuzzWorkspaceBoundary | Workspace Boundary | 0.000s pass | KEEP-FUZZ | seed corpus on commits; bounded campaigns separately |
| internal/sleeve/path_fuzz_test.go | FuzzEnvironmentArgumentEncoding | Environment Argument Encoding | 0.000s pass | KEEP-FUZZ | seed corpus on commits; bounded campaigns separately |
| internal/sleeve/plan_test.go | TestPlanGolden | Plan Golden | 0.010s pass | DELETE | implementation/output-shape assertion; validate externally visible behavior in SLEEVE: real sandbox lifecycle, secret boundaries, symlinks, git clone, OOM and toolchains |
| internal/sleeve/plan_test.go | TestPlanRejects | Plan Rejects | 0.000s pass | CONVERT | SLEEVE: real sandbox lifecycle, secret boundaries, symlinks, git clone, OOM and toolchains; preserve the named behavior as a scenario assertion |
| internal/sleeve/plan_test.go | TestWithin | Within | 0.000s pass | CONVERT | SLEEVE: real sandbox lifecycle, secret boundaries, symlinks, git clone, OOM and toolchains; preserve the named behavior as a scenario assertion |
| internal/sleeve/plan_test.go | TestParseHarness | Parse Harness | 0.000s pass | CONVERT | SLEEVE: real sandbox lifecycle, secret boundaries, symlinks, git clone, OOM and toolchains; preserve the named behavior as a scenario assertion |
| internal/sleeve/plan_test.go | TestLocateFollowsSymlinkChain | Locate Follows Symlink Chain | 0.000s pass | CONVERT | SLEEVE: real sandbox lifecycle, secret boundaries, symlinks, git clone, OOM and toolchains; preserve the named behavior as a scenario assertion |
| internal/sleeve/plan_test.go | TestAddBinaryDir | Add Binary Dir | 0.000s pass | CONVERT | SLEEVE: real sandbox lifecycle, secret boundaries, symlinks, git clone, OOM and toolchains; preserve the named behavior as a scenario assertion |
| internal/sleeve/plan_test.go | TestCheckWorkspaceStateOwnership | Check Workspace State Ownership | 0.000s pass | CONVERT | SLEEVE: real sandbox lifecycle, secret boundaries, symlinks, git clone, OOM and toolchains; preserve the named behavior as a scenario assertion |
| internal/sleeve/plan_test.go | TestCheckWorkspaceProtectedTargets | Check Workspace Protected Targets | 0.000s pass | CONVERT | SLEEVE: real sandbox lifecycle, secret boundaries, symlinks, git clone, OOM and toolchains; preserve the named behavior as a scenario assertion |
| internal/sleeve/plan_test.go | TestDefaultConfigCanonicalizesPaths | Default Config Canonicalizes Paths | 0.000s pass | CONVERT | SLEEVE: real sandbox lifecycle, secret boundaries, symlinks, git clone, OOM and toolchains; preserve the named behavior as a scenario assertion |
| internal/sleeve/plan_test.go | TestDefaultToolDirsAreNarrow | Default Tool Dirs Are Narrow | 0.000s pass | DELETE | implementation/output-shape assertion; validate externally visible behavior in SLEEVE: real sandbox lifecycle, secret boundaries, symlinks, git clone, OOM and toolchains |
| internal/sleeve/plan_test.go | TestPlanMasksCredentialFiles | Plan Masks Credential Files | 0.000s pass | CONVERT | SLEEVE: real sandbox lifecycle, secret boundaries, symlinks, git clone, OOM and toolchains; preserve the named behavior as a scenario assertion |
| internal/sleeve/plan_test.go | TestLocateCredentialDirChains | Locate Credential Dir Chains | 0.000s pass | CONVERT | SLEEVE: real sandbox lifecycle, secret boundaries, symlinks, git clone, OOM and toolchains; preserve the named behavior as a scenario assertion |
| internal/sleeve/plan_test.go | TestPlanKeepsEnvOutOfArgv | Plan Keeps Env Out Of Argv | 0.020s pass | CONVERT | SLEEVE: real sandbox lifecycle, secret boundaries, symlinks, git clone, OOM and toolchains; preserve the named behavior as a scenario assertion |
| internal/sleeve/plan_test.go | TestEnvFileRoundTrip | Env File Round Trip | 0.000s pass | CONVERT | SLEEVE: real sandbox lifecycle, secret boundaries, symlinks, git clone, OOM and toolchains; preserve the named behavior as a scenario assertion |
| internal/sleeve/plan_test.go | TestLaunchKeepsEnvAndArgs | Launch Keeps Env And Args | 0.000s pass | CONVERT | SLEEVE: real sandbox lifecycle, secret boundaries, symlinks, git clone, OOM and toolchains; preserve the named behavior as a scenario assertion |
| internal/sleeve/rename_test.go | TestRenameRecord | Rename Record | 0.000s pass | CONVERT | SLEEVE: real sandbox lifecycle, secret boundaries, symlinks, git clone, OOM and toolchains; preserve the named behavior as a scenario assertion |
| internal/sleeve/selection_test.go | TestSelectionPersistence | Selection Persistence | 0.000s pass | CONVERT | SLEEVE: real sandbox lifecycle, secret boundaries, symlinks, git clone, OOM and toolchains; preserve the named behavior as a scenario assertion |
| internal/sleeve/selection_test.go | TestCatalogHasNoWorkspaceOrRealHomeMount | Catalog Has No Workspace Or Real Home Mount | 0.010s pass | DELETE | implementation/output-shape assertion; validate externally visible behavior in SLEEVE: real sandbox lifecycle, secret boundaries, symlinks, git clone, OOM and toolchains |
| internal/sleeve/selection_test.go | TestSelectionPersistenceFailureKeepsPreviousRecord | Selection Persistence Failure Keeps Previous Record | 0.000s pass | CONVERT | SLEEVE: real sandbox lifecycle, secret boundaries, symlinks, git clone, OOM and toolchains; preserve the named behavior as a scenario assertion |
| internal/sleeve/workspaces_test.go | TestGitDetails | Git Details | 0.000s pass | CONVERT | SLEEVE: real sandbox lifecycle, secret boundaries, symlinks, git clone, OOM and toolchains; preserve the named behavior as a scenario assertion |
| internal/sleeve/workspaces_test.go | TestWorkspaces | Workspaces | 0.000s pass | CONVERT | SLEEVE: real sandbox lifecycle, secret boundaries, symlinks, git clone, OOM and toolchains; preserve the named behavior as a scenario assertion |
| internal/svcgen/e2e_environment_test.go | TestMain | Main | not measured | KEEP-E2E | runner infrastructure; retain while any package tests use it |
| internal/svcgen/svcgen_test.go | TestGolden | Golden | 0.000s pass | CONVERT | INSTALL: generated service definitions, offline systemd verification and isolated service lifecycle; preserve the named behavior as a scenario assertion |
| internal/svcgen/svcgen_test.go | TestValidation | Validation | 0.000s pass | CONVERT | INSTALL: generated service definitions, offline systemd verification and isolated service lifecycle; preserve the named behavior as a scenario assertion |
| internal/svcgen/svcgen_test.go | TestUTF16 | UTF16 | 0.000s pass | CONVERT | INSTALL: generated service definitions, offline systemd verification and isolated service lifecycle; preserve the named behavior as a scenario assertion |
| internal/svcgen/svcgen_test.go | TestSystemdVerify | Systemd Verify | 0.300s pass | CONVERT | INSTALL: generated service definitions, offline systemd verification and isolated service lifecycle; preserve the named behavior as a scenario assertion |
| internal/tailnet/backup_it_test.go | TestBackupRestoreWithLiveHeadscale | Backup Restore With Live Headscale | 0.000s skip | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/tailnet/integration_test.go | TestTailnetEndToEnd | Tailnet End To End | 0.000s skip | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/tailnet/integration_test.go | TestNodeRestartRecovery | Node Restart Recovery | 0.000s skip | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/tailnet/integration_test.go | TestDERPRelay | DERPRelay | 0.000s skip | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/tailnet/status_test.go | TestStatusProto | Status Proto | 0.000s pass | DELETE | implementation/output-shape assertion; validate externally visible behavior in TAILNET: isolated headscale, two nodes, ACL, relay and restart |
| internal/tailnet/tailnet_test.go | TestConfigValidate | Config Validate | 0.000s pass | CONVERT | TAILNET: isolated headscale, two nodes, ACL, relay and restart; preserve the named behavior as a scenario assertion |
| internal/tailnet/tailnet_test.go | TestHeadscaleConfig | Headscale Config | 0.000s pass | DELETE | implementation/output-shape assertion; validate externally visible behavior in TAILNET: isolated headscale, two nodes, ACL, relay and restart |
| internal/tailnet/tailnet_test.go | TestDERPMapAndCert | DERPMap And Cert | 0.050s pass | CONVERT | TAILNET: isolated headscale, two nodes, ACL, relay and restart; preserve the named behavior as a scenario assertion |
| internal/tailnet/tailnet_test.go | TestPolicy | Policy | 0.000s pass | DELETE | implementation/output-shape assertion; validate externally visible behavior in TAILNET: isolated headscale, two nodes, ACL, relay and restart |
| internal/tailnet/tailnet_test.go | TestSanitizeLabel | Sanitize Label | 0.000s pass | CONVERT | TAILNET: isolated headscale, two nodes, ACL, relay and restart; preserve the named behavior as a scenario assertion |
| internal/tailnet/tailnet_test.go | TestIdentify | Identify | 0.000s pass | CONVERT | TAILNET: isolated headscale, two nodes, ACL, relay and restart; preserve the named behavior as a scenario assertion |
| internal/tailnet/tailnet_test.go | TestRetryDial | Retry Dial | 2.200s pass | CONVERT | TAILNET: isolated headscale, two nodes, ACL, relay and restart; preserve the named behavior as a scenario assertion |
| internal/tailnet/tailnet_test.go | TestRetryDialContextCancel | Retry Dial Context Cancel | 0.000s pass | CONVERT | TAILNET: isolated headscale, two nodes, ACL, relay and restart; preserve the named behavior as a scenario assertion |
| internal/tailnet/tailnet_test.go | TestParseChecksum | Parse Checksum | 0.000s pass | CONVERT | TAILNET: isolated headscale, two nodes, ACL, relay and restart; preserve the named behavior as a scenario assertion |
| internal/tailnet/tailnet_test.go | TestInstall | Install | 0.020s pass | CONVERT | TAILNET: isolated headscale, two nodes, ACL, relay and restart; preserve the named behavior as a scenario assertion |
| internal/tailnet/tailnet_test.go | TestPinnedChecksumMismatch | Pinned Checksum Mismatch | 0.000s pass | CONVERT | TAILNET: isolated headscale, two nodes, ACL, relay and restart; preserve the named behavior as a scenario assertion |
| internal/tailnet/tailnet_test.go | TestConfigSaveLoad | Config Save Load | 0.000s pass | CONVERT | TAILNET: isolated headscale, two nodes, ACL, relay and restart; preserve the named behavior as a scenario assertion |
| internal/tailnet/tailnet_test.go | TestSocketPathShort | Socket Path Short | 0.000s pass | CONVERT | TAILNET: isolated headscale, two nodes, ACL, relay and restart; preserve the named behavior as a scenario assertion |
| internal/tailnet/tailnet_test.go | TestRotatingLog | Rotating Log | 0.000s pass | CONVERT | TAILNET: isolated headscale, two nodes, ACL, relay and restart; preserve the named behavior as a scenario assertion |
| internal/tailnet/tailnet_test.go | TestNodeEnrolledWith | Node Enrolled With | 0.000s pass | CONVERT | TAILNET: isolated headscale, two nodes, ACL, relay and restart; preserve the named behavior as a scenario assertion |
| internal/tailnet/tailnet_test.go | TestTransportBoundsEveryPhase | Transport Bounds Every Phase | 0.100s pass | CONVERT | TAILNET: isolated headscale, two nodes, ACL, relay and restart; preserve the named behavior as a scenario assertion |
| internal/tailnet/tailnet_test.go | TestHubAddr | Hub Addr | 0.000s pass | CONVERT | TAILNET: isolated headscale, two nodes, ACL, relay and restart; preserve the named behavior as a scenario assertion |
| internal/tailnet/tailnet_test.go | TestQuietDERPLog | Quiet DERPLog | 0.000s pass | CONVERT | TAILNET: isolated headscale, two nodes, ACL, relay and restart; preserve the named behavior as a scenario assertion |
| internal/tailnet/tailnet_test.go | TestRetier | Retier | 0.000s pass | CONVERT | TAILNET: isolated headscale, two nodes, ACL, relay and restart; preserve the named behavior as a scenario assertion |
| internal/xfer/xfer_test.go | TestFileRoundTrip | File Round Trip | 0.290s pass | CONVERT | TRANSFER: binary hub/node chunks, compression, SHA-256 and malicious archive rejection; preserve the named behavior as a scenario assertion |
| internal/xfer/xfer_test.go | TestCommitVerification | Commit Verification | 0.000s pass | CONVERT | TRANSFER: binary hub/node chunks, compression, SHA-256 and malicious archive rejection; preserve the named behavior as a scenario assertion |
| internal/xfer/xfer_test.go | TestOverwrite | Overwrite | 0.000s pass | CONVERT | TRANSFER: binary hub/node chunks, compression, SHA-256 and malicious archive rejection; preserve the named behavior as a scenario assertion |
| internal/xfer/xfer_test.go | TestDirRoundTrip | Dir Round Trip | 0.000s pass | CONVERT | TRANSFER: binary hub/node chunks, compression, SHA-256 and malicious archive rejection; preserve the named behavior as a scenario assertion |
| internal/xfer/xfer_test.go | TestUnpackRejectsUnsafeEntries | Unpack Rejects Unsafe Entries | 0.000s pass | CONVERT | TRANSFER: binary hub/node chunks, compression, SHA-256 and malicious archive rejection; preserve the named behavior as a scenario assertion |
| internal/xfer/xfer_test.go | TestWriteRejectsBadChunks | Write Rejects Bad Chunks | 0.000s pass | CONVERT | TRANSFER: binary hub/node chunks, compression, SHA-256 and malicious archive rejection; preserve the named behavior as a scenario assertion |
| tests/e2e/workflow_test.go | TestBinaryClusterRestartAndRestore | Binary Cluster Restart And Restore | 4.420s pass | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| app/test/add_remove_node_test.dart | test: the platform follows the machine (L67) | the platform follows the machine | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; add remove node |
| app/test/add_remove_node_test.dart | test: node names follow the hub rule (L78) | node names follow the hub rule | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; add remove node |
| app/test/add_remove_node_test.dart | testWidgets: invites, shows the line for the machine and follows setup (L96) | invites, shows the line for the machine and follows setup | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; add remove node |
| app/test/add_remove_node_test.dart | testWidgets: a phone shows the line as a QR code too (L210) | a phone shows the line as a QR code too | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; add remove node |
| app/test/add_remove_node_test.dart | testWidgets: a token that is not the owner is told what to do (L245) | a token that is not the owner is told what to do | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; add remove node |
| app/test/add_remove_node_test.dart | testWidgets: needs the name typed, then drains, uninstalls and removes (L280) | needs the name typed, then drains, uninstalls and removes | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; add remove node |
| app/test/add_remove_node_test.dart | testWidgets: an offline node cannot be asked to uninstall (L356) | an offline node cannot be asked to uninstall | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; add remove node |
| app/test/add_remove_node_test.dart | testWidgets: a pending invite is cancelled without typing (L385) | a pending invite is cancelled without typing | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; add remove node |
| app/test/add_remove_node_test.dart | testWidgets: a refusal is shown and can be retried (L414) | a refusal is shown and can be retried | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; add remove node |
| app/test/add_remove_node_test.dart | testWidgets: a pending node offers reissue and cancel (L447) | a pending node offers reissue and cancel | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; add remove node |
| app/test/add_remove_node_test.dart | testWidgets: a pending card reads PENDING (L470) | a pending card reads PENDING | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; add remove node |
| app/test/audit_test.dart | test: labels (L66) | labels | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; audit |
| app/test/audit_test.dart | testWidgets: entries, outcomes and paging at $width (L90) | entries, outcomes and paging at $width | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; audit |
| app/test/audit_test.dart | testWidgets: filters go to the hub and start over (L113) | filters go to the hub and start over | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; audit |
| app/test/audit_test.dart | testWidgets: a token that is not the owner is told so (L131) | a token that is not the owner is told so | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; audit |
| app/test/chat_screens_test.dart | test: screenshots skipped without SCREENSHOT_DIR (L1152) | screenshots skipped without SCREENSHOT_DIR | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; chat live/replay equivalence and reconnect |
| app/test/chat_screens_test.dart | testWidgets: chat $tag (L1161) | chat $tag | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; chat live/replay equivalence and reconnect |
| app/test/chat_screens_test.dart | testWidgets: sleeves $tag (L1195) | sleeves $tag | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; chat live/replay equivalence and reconnect |
| app/test/chat_screens_test.dart | testWidgets: $tab $tag (L1288) | $tab $tag | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; chat live/replay equivalence and reconnect |
| app/test/chat_screens_test.dart | testWidgets: notes-editor $tag (L1301) | notes-editor $tag | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; chat live/replay equivalence and reconnect |
| app/test/chat_screens_test.dart | testWidgets: thread $tag (L1316) | thread $tag | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; chat live/replay equivalence and reconnect |
| app/test/chat_screens_test.dart | testWidgets: desk $tag (L1331) | desk $tag | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; chat live/replay equivalence and reconnect |
| app/test/chat_screens_test.dart | testWidgets: audit $tag (L1370) | audit $tag | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; chat live/replay equivalence and reconnect |
| app/test/chat_screens_test.dart | testWidgets: add-node $tag (L1395) | add-node $tag | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; chat live/replay equivalence and reconnect |
| app/test/chat_screens_test.dart | testWidgets: remove-node $tag (L1475) | remove-node $tag | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; chat live/replay equivalence and reconnect |
| app/test/chat_screens_test.dart | testWidgets: node $node $tag (L1535) | node $node $tag | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; chat live/replay equivalence and reconnect |
| app/test/chat_screens_test.dart | testWidgets: hubs $tag (L1571) | hubs $tag | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; chat live/replay equivalence and reconnect |
| app/test/chat_screens_test.dart | testWidgets: terminal $tag (L1609) | terminal $tag | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; chat live/replay equivalence and reconnect |
| app/test/chat_test.dart | test: deltas share their message seq and append; a new seq starts one (L43) | deltas share their message seq and append; a new seq starts one | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; chat live/replay equivalence and reconnect |
| app/test/chat_test.dart | test: thinking and answer with the same id stay separate (L53) | thinking and answer with the same id stay separate | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; chat live/replay equivalence and reconnect |
| app/test/chat_test.dart | test: a coalesced replay folds to the same transcript as the stream (L60) | a coalesced replay folds to the same transcript as the stream | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; chat live/replay equivalence and reconnect |
| app/test/chat_test.dart | test: tool results pair with their call by id (L71) | tool results pair with their call by id | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; chat live/replay equivalence and reconnect |
| app/test/chat_test.dart | test: approvals drive state until resolved (L110) | approvals drive state until resolved | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; chat live/replay equivalence and reconnect |
| app/test/chat_test.dart | test: usage, compaction and duplicate seq (L135) | usage, compaction and duplicate seq | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; chat live/replay equivalence and reconnect |
| app/test/chat_test.dart | test: a handoff marks where the harness changed (L165) | a handoff marks where the harness changed | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; chat live/replay equivalence and reconnect |
| app/test/chat_test.dart | test: status priority (L189) | status priority | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; chat live/replay equivalence and reconnect |
| app/test/chat_test.dart | test: local model rule (L227) | local model rule | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; chat live/replay equivalence and reconnect |
| app/test/chat_test.dart | test: fence splitting and tool summaries (L261) | fence splitting and tool summaries | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; chat live/replay equivalence and reconnect |
| app/test/chat_test.dart | test: reconnect mid-message resumes one earlier and replaces the text (L271) | reconnect mid-message resumes one earlier and replaces the text | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; chat live/replay equivalence and reconnect |
| app/test/chat_test.dart | test: dispose aborts the watch call (L306) | dispose aborts the watch call | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; chat live/replay equivalence and reconnect |
| app/test/chat_test.dart | test: send retries keep one request_id (L328) | send retries keep one request_id | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; chat live/replay equivalence and reconnect |
| app/test/chat_test.dart | test: a permanent send error is not retried (L350) | a permanent send error is not retried | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; chat live/replay equivalence and reconnect |
| app/test/chat_test.dart | testWidgets: renders the transcript and sends, stops, responds (L385) | renders the transcript and sends, stops, responds | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; chat live/replay equivalence and reconnect |
| app/test/chat_test.dart | testWidgets: compact enabled when idle and shows local model badge (L484) | compact enabled when idle and shows local model badge | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; chat live/replay equivalence and reconnect |
| app/test/chat_test.dart | testWidgets: shows model, status, context, usage and local warning (L543) | shows model, status, context, usage and local warning | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; chat live/replay equivalence and reconnect |
| app/test/chat_test.dart | testWidgets: a local summary is flagged on the card (L581) | a local summary is flagged on the card | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; chat live/replay equivalence and reconnect |
| app/test/chat_test.dart | testWidgets: pi on a hosted open model has no warning (L606) | pi on a hosted open model has no warning | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; chat live/replay equivalence and reconnect |
| app/test/chat_test.dart | testWidgets: terminal-mode shell has no warning and opens terminal (L623) | terminal-mode shell has no warning and opens terminal | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; chat live/replay equivalence and reconnect |
| app/test/desk_test.dart | test: status words (L129) | status words | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; desk |
| app/test/desk_test.dart | test: policy form (L168) | policy form | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; desk |
| app/test/desk_test.dart | test: screenshot width follows the pane (L199) | screenshot width follows the pane | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; desk |
| app/test/desk_test.dart | testWidgets: the desk shows screen, windows, foreground and use (L211) | the desk shows screen, windows, foreground and use | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; desk |
| app/test/desk_test.dart | testWidgets: the owner sets and clears the policy (L241) | the owner sets and clears the policy | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; desk |
| app/test/desk_test.dart | testWidgets: a device without the owner token is told so (L274) | a device without the owner token is told so | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; desk |
| app/test/desk_test.dart | testWidgets: node detail offers the desk only for Windows nodes (L285) | node detail offers the desk only for Windows nodes | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; desk |
| app/test/messages_test.dart | test: delivery states (L165) | delivery states | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; messages |
| app/test/messages_test.dart | test: targets, labels and permissions (L190) | targets, labels and permissions | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; messages |
| app/test/messages_test.dart | testWidgets: threads show participants, last word and their lamps (L225) | threads show participants, last word and their lamps | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; messages |
| app/test/messages_test.dart | testWidgets: opening a thread shows deliveries and marks it read (L240) | opening a thread shows deliveries and marks it read | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; messages |
| app/test/messages_test.dart | testWidgets: a token without message scopes is told what it needs (L282) | a token without message scopes is told what it needs | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; messages |
| app/test/messages_test.dart | testWidgets: wide layout keeps the list beside the open thread (L296) | wide layout keeps the list beside the open thread | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; messages |
| app/test/messages_test.dart | testWidgets: filters go to the hub and mail events refresh the list (L306) | filters go to the hub and mail events refresh the list | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; messages |
| app/test/messages_test.dart | testWidgets: a new message goes out with its kind and opens its thread (L340) | a new message goes out with its kind and opens its thread | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; messages |
| app/test/model_selection_test.dart | testWidgets: native catalog controls model-specific efforts and sends selected settings (L33) | native catalog controls model-specific efforts and sends selected settings | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; model selection |
| app/test/model_selection_test.dart | testWidgets: unsupported harness explains native defaults without invented options (L96) | unsupported harness explains native defaults without invented options | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; model selection |
| app/test/model_selection_test.dart | test: settings replay updates effective effort independently of request (L116) | settings replay updates effective effort independently of request | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; model selection |
| app/test/nodes_test.dart | testWidgets: fresh node shows live numbers and no alerts (L93) | fresh node shows live numbers and no alerts | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; nodes |
| app/test/nodes_test.dart | testWidgets: tier badge marks care nodes only (L118) | tier badge marks care nodes only | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; nodes |
| app/test/nodes_test.dart | testWidgets: stale node is labeled with its age (L141) | stale node is labeled with its age | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; nodes |
| app/test/nodes_test.dart | testWidgets: unreachable node hides numbers (L159) | unreachable node hides numbers | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; nodes |
| app/test/nodes_test.dart | testWidgets: alert badges for disk and memory (L183) | alert badges for disk and memory | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; nodes |
| app/test/nodes_test.dart | testWidgets: lists worst first and refreshes on node events (L211) | lists worst first and refreshes on node events | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; nodes |
| app/test/nodes_test.dart | test: ranks containers, engine buckets and watched dirs together (L267) | ranks containers, engine buckets and watched dirs together | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; nodes |
| app/test/nodes_test.dart | test: engine containers bucket counts when no per-container data (L294) | engine containers bucket counts when no per-container data | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; nodes |
| app/test/nodes_test.dart | test: limit and ordering (L304) | limit and ordering | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; nodes |
| app/test/nodes_test.dart | testWidgets: detail shows sections and a disabled cleanup note (L314) | detail shows sections and a disabled cleanup note | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; nodes |
| app/test/nodes_test.dart | testWidgets: detail says how the node isolates jobs (L346) | detail says how the node isolates jobs | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; nodes |
| app/test/nodes_test.dart | test: sleeve workspaces count as offenders (L376) | sleeve workspaces count as offenders | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; nodes |
| app/test/nodes_test.dart | testWidgets: sleeves screen lists and kills only after confirm (L393) | sleeves screen lists and kills only after confirm | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; nodes |
| app/test/nodes_test.dart | testWidgets: card lists integrated GPU and NPU without fake meters (L477) | card lists integrated GPU and NPU without fake meters | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; nodes |
| app/test/nodes_test.dart | testWidgets: detail shows unknown VRAM use as dashes, not spare (L503) | detail shows unknown VRAM use as dashes, not spare | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; nodes |
| app/test/nodes_test.dart | test: summary uses the first GPU and never invents VRAM use (L547) | summary uses the first GPU and never invents VRAM use | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; nodes |
| app/test/nodes_test.dart | test: busy comes from the hub, by GPU index (L597) | busy comes from the hub, by GPU index | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; nodes |
| app/test/nodes_test.dart | test: holders come from live GPU leases of the node (L606) | holders come from live GPU leases of the node | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; nodes |
| app/test/nodes_test.dart | testWidgets: cards light BUSY per GPU and name the holder (L621) | cards light BUSY per GPU and name the holder | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; nodes |
| app/test/notes_test.dart | testWidgets: waits out the pause and sends only what changed (L162) | waits out the pause and sends only what changed | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; notes |
| app/test/notes_test.dart | testWidgets: creates on the first text and updates after (L185) | creates on the first text and updates after | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; notes |
| app/test/notes_test.dart | testWidgets: stays dirty while the hub is down and retries (L203) | stays dirty while the hub is down and retries | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; notes |
| app/test/notes_test.dart | testWidgets: a rejected save is not retried (L223) | a rejected save is not retried | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; notes |
| app/test/notes_test.dart | testWidgets: local edits win over the hub copy (L237) | local edits win over the hub copy | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; notes |
| app/test/notes_test.dart | testWidgets: lists pinned first, then newest, with tags and sources (L253) | lists pinned first, then newest, with tags and sources | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; notes |
| app/test/notes_test.dart | testWidgets: a tag chip filters the list (L272) | a tag chip filters the list | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; notes |
| app/test/notes_test.dart | testWidgets: phone: the editor replaces the list and autosaves (L283) | phone: the editor replaces the list and autosaves | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; notes |
| app/test/notes_test.dart | testWidgets: a tag can be picked from the existing ones (L307) | a tag can be picked from the existing ones | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; notes |
| app/test/notes_test.dart | testWidgets: closing the editor does not drop an unsaved edit (L321) | closing the editor does not drop an unsaved edit | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; notes |
| app/test/notes_test.dart | testWidgets: a new note is created once it has text (L336) | a new note is created once it has text | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; notes |
| app/test/notes_test.dart | testWidgets: pinning saves right away (L353) | pinning saves right away | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; notes |
| app/test/notes_test.dart | testWidgets: offline edits show and retry until they land (L367) | offline edits show and retry until they land | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; notes |
| app/test/notes_test.dart | testWidgets: delete asks first (L384) | delete asks first | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; notes |
| app/test/notes_test.dart | testWidgets: desktop: list and editor sit side by side (L406) | desktop: list and editor sit side by side | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; notes |
| app/test/notes_test.dart | testWidgets: a note the orchestrator changed shows up in an open editor (L425) | a note the orchestrator changed shows up in an open editor | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; notes |
| app/test/orchestrator_tab_test.dart | testWidgets: switches between chat and mail and badges unread mail (L94) | switches between chat and mail and badges unread mail | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; chat live/replay equivalence and reconnect |
| app/test/orchestrator_tab_test.dart | testWidgets: starts envoy from the empty state, on auto or a harness (L116) | starts envoy from the empty state, on auto or a harness | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; orchestrator tab |
| app/test/orchestrator_tab_test.dart | testWidgets: start can pick a harness (L133) | start can pick a harness | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; orchestrator tab |
| app/test/orchestrator_tab_test.dart | testWidgets: respawn asks first and warns about the cache (L144) | respawn asks first and warns about the cache | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; orchestrator tab |
| app/test/orchestrator_tab_test.dart | testWidgets: resleeve moves envoy to the chosen harness (L168) | resleeve moves envoy to the chosen harness | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; orchestrator tab |
| app/test/orchestrator_tab_test.dart | testWidgets: other sleeves get no envoy actions (L185) | other sleeves get no envoy actions | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; orchestrator tab |
| app/test/orchestrator_tab_test.dart | testWidgets: table callback: c.$1 (L214) | table callback: c.$1 | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; orchestrator tab |
| app/test/orchestrator_tab_test.dart | testWidgets: unknown expiry is cold (L220) | unknown expiry is cold | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; orchestrator tab |
| app/test/orchestrator_tab_test.dart | testWidgets: shows next to the context meter for a claude sleeve (L225) | shows next to the context meter for a claude sleeve | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; orchestrator tab |
| app/test/orchestrator_tab_test.dart | testWidgets: talks to envoy and lists only chat sleeves, envoy first (L248) | talks to envoy and lists only chat sleeves, envoy first | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; chat live/replay equivalence and reconnect |
| app/test/orchestrator_tab_test.dart | test: envoy is pinned first in the hub fence (L276) | envoy is pinned first in the hub fence | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; orchestrator tab |
| app/test/orchestrator_tab_test.dart | testWidgets: the envoy card carries an ENVOY badge (L285) | the envoy card carries an ENVOY badge | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; orchestrator tab |
| app/test/pairing_test.dart | test: web fragment link (L13) | web fragment link | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; pairing |
| app/test/pairing_test.dart | test: custom scheme link (L26) | custom scheme link | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; pairing |
| app/test/pairing_test.dart | test: name defaults to host (L38) | name defaults to host | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; pairing |
| app/test/pairing_test.dart | test: rejects other links (L43) | rejects other links | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; pairing |
| app/test/pairing_test.dart | testWidgets: confirm saves the hub and opens it (L77) | confirm saves the hub and opens it | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; pairing |
| app/test/pairing_test.dart | testWidgets: cancel saves nothing (L91) | cancel saves nothing | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; pairing |
| app/test/start_work_test.dart | test: table callback: input (L47) | table callback: input | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; start work |
| app/test/start_work_test.dart | testWidgets: picks an existing workspace and names the sleeve after it (L52) | picks an existing workspace and names the sleeve after it | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; start work |
| app/test/start_work_test.dart | testWidgets: a hub-owned workspace fixes the sleeve name (L96) | a hub-owned workspace fixes the sleeve name | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; start work |
| app/test/start_work_test.dart | testWidgets: clone suggests a name from the URL (L116) | clone suggests a name from the URL | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; start work |
| app/test/start_work_test.dart | testWidgets: path must be absolute and clone must not be a path (L158) | path must be absolute and clone must not be a path | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; start work |
| app/test/start_work_test.dart | test: the hub node leads, nodes without sleeves get no fence (L198) | the hub node leads, nodes without sleeves get no fence | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; start work |
| app/test/start_work_test.dart | testWidgets: each node has a fence holding its own sleeves (L259) | each node has a fence holding its own sleeves | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; start work |
| app/test/start_work_test.dart | testWidgets: the fence plus starts a sleeve on the picked workspace (L286) | the fence plus starts a sleeve on the picked workspace | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; start work |
| app/test/version_skew_test.dart | test: versionsDiffer ignores unknown versions (L89) | versionsDiffer ignores unknown versions | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; version skew |
| app/test/version_skew_test.dart | test: versionSkew (L103) | versionSkew | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; version skew |
| app/test/version_skew_test.dart | testWidgets: annunciator is dark when everything matches (L168) | annunciator is dark when everything matches | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; version skew |
| app/test/version_skew_test.dart | testWidgets: a node behind lights it and badges its row (L174) | a node behind lights it and badges its row | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; version skew |
| app/test/version_skew_test.dart | testWidgets: an app that differs from the hub lights it alone (L187) | an app that differs from the hub lights it alone | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; version skew |
| app/test/version_skew_test.dart | testWidgets: card badge carries both versions (L196) | card badge carries both versions | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; version skew |
| app/test/version_skew_test.dart | testWidgets: detail shows both versions and offers the update (L217) | detail shows both versions and offers the update | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; version skew |
| app/test/version_skew_test.dart | test: labels and fractions (L252) | labels and fractions | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; version skew |
| app/test/version_skew_test.dart | testWidgets: dialog follows the stream to confirmation (L303) | dialog follows the stream to confirmation | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; version skew |
| app/test/version_skew_test.dart | testWidgets: dialog shows a refusal (L356) | dialog shows a refusal | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; version skew |
| app/test/vfd_test.dart | test: lit segment count for a value (L36) | lit segment count for a value | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; vfd |
| app/test/vfd_test.dart | test: segment zones follow the warn and critical thresholds (L53) | segment zones follow the warn and critical thresholds | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; vfd |
| app/test/vfd_test.dart | test: level tone and readout helpers (L60) | level tone and readout helpers | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; vfd |
| app/test/vfd_test.dart | testWidgets: meter fills to the value with threshold colours (L80) | meter fills to the value with threshold colours | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; vfd |
| app/test/vfd_test.dart | testWidgets: stale meter dims every lit segment (L90) | stale meter dims every lit segment | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; vfd |
| app/test/vfd_test.dart | testWidgets: unknown meter lights nothing (L96) | unknown meter lights nothing | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; vfd |
| app/test/vfd_test.dart | testWidgets: meter animates its fill, or jumps with reduced motion (L107) | meter animates its fill, or jumps with reduced motion | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; vfd |
| app/test/vfd_test.dart | testWidgets: meter row value never truncates (L121) | meter row value never truncates | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; vfd |
| app/test/vfd_test.dart | testWidgets: attention lamp blinks, and holds a ring with reduced motion (L134) | attention lamp blinks, and holds a ring with reduced motion | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; vfd |
| app/test/vfd_test.dart | test: only multi-day windows resetting within a day count as soon (L160) | only multi-day windows resetting within a day count as soon | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; vfd |
| app/test/vfd_test.dart | testWidgets: a new beat flickers the lamp once; reduced motion does not (L182) | a new beat flickers the lamp once; reduced motion does not | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; vfd |
| app/test/vfd_test.dart | testWidgets: steady lamp schedules no frames (L216) | steady lamp schedules no frames | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; vfd |
| app/test/vfd_test.dart | testWidgets: readout exposes its value to accessibility (L222) | readout exposes its value to accessibility | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; vfd |
| app/test/vfd_test.dart | testWidgets: light theme meters use ink, not phosphor (L234) | light theme meters use ink, not phosphor | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; vfd |
| app/test/widget_test.dart | test: round trips multiple hubs through shared_preferences (L102) | round trips multiple hubs through shared_preferences | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; widget |
| app/test/widget_test.dart | test: terminal uri maps scheme and path (L125) | terminal uri maps scheme and path | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; widget |
| app/test/widget_test.dart | test: url validation (L136) | url validation | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; widget |
| app/test/widget_test.dart | testWidgets: phone width uses bottom navigation (L163) | phone width uses bottom navigation | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; widget |
| app/test/widget_test.dart | testWidgets: five destinations in order, on phone and desktop (L196) | five destinations in order, on phone and desktop | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; widget |
| app/test/widget_test.dart | testWidgets: desktop width uses navigation rail (L225) | desktop width uses navigation rail | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; widget |
| app/test/widget_test.dart | test: bearer interceptor sends the token (L240) | bearer interceptor sends the token | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; widget |
| app/test/widget_test.dart | test: reconnects with backoff after the stream ends (L248) | reconnects with backoff after the stream ends | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; widget |
| app/test/widget_test.dart | test: aborts the call on retry and stop (L263) | aborts the call on retry and stop | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; widget |
| app/test/widget_test.dart | test: quota event payload decodes (L282) | quota event payload decodes | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; widget |
| app/test/widget_test.dart | test: sticky ctrl + c is 0x03 and is consumed (L297) | sticky ctrl + c is 0x03 and is consumed | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; widget |
| app/test/widget_test.dart | test: ctrl with a bar key sends the plain sequence and clears (L305) | ctrl with a bar key sends the plain sequence and clears | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; widget |
| app/test/widget_test.dart | testWidgets: bar buttons emit keys; Ctrl button is sticky (L313) | bar buttons emit keys; Ctrl button is sticky | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; widget |
| app/test/widget_test.dart | test: terminal framing (L332) | terminal framing | not measured: Flutter unavailable | CONVERT | APP: Flutter client against real loopback hub; widget |

## Phase-2 exact removal manifest

This is a proposed function-level manifest, not authorization to delete. Every CONVERT entry is blocked until its named scenario assertions run and pass on the host. Keep shared helpers and TestMain until their last caller moves. No entire-file `git rm` is proposed: helpers cross file boundaries, so deleting whole files now would break retained workflows. Remove unused imports along with each approved function.

Counts by package (CONVERT plus DELETE):


### app/test: 165 functions/callbacks


`app/test/add_remove_node_test.dart` — remove only these functions/callbacks:

- `test: the platform follows the machine (L67)`
- `test: node names follow the hub rule (L78)`
- `testWidgets: invites, shows the line for the machine and follows setup (L96)`
- `testWidgets: a phone shows the line as a QR code too (L210)`
- `testWidgets: a token that is not the owner is told what to do (L245)`
- `testWidgets: needs the name typed, then drains, uninstalls and removes (L280)`
- `testWidgets: an offline node cannot be asked to uninstall (L356)`
- `testWidgets: a pending invite is cancelled without typing (L385)`
- `testWidgets: a refusal is shown and can be retried (L414)`
- `testWidgets: a pending node offers reissue and cancel (L447)`
- `testWidgets: a pending card reads PENDING (L470)`

`app/test/audit_test.dart` — remove only these functions/callbacks:

- `test: labels (L66)`
- `testWidgets: entries, outcomes and paging at $width (L90)`
- `testWidgets: filters go to the hub and start over (L113)`
- `testWidgets: a token that is not the owner is told so (L131)`

`app/test/chat_screens_test.dart` — remove only these functions/callbacks:

- `test: screenshots skipped without SCREENSHOT_DIR (L1152)`
- `testWidgets: chat $tag (L1161)`
- `testWidgets: sleeves $tag (L1195)`
- `testWidgets: $tab $tag (L1288)`
- `testWidgets: notes-editor $tag (L1301)`
- `testWidgets: thread $tag (L1316)`
- `testWidgets: desk $tag (L1331)`
- `testWidgets: audit $tag (L1370)`
- `testWidgets: add-node $tag (L1395)`
- `testWidgets: remove-node $tag (L1475)`
- `testWidgets: node $node $tag (L1535)`
- `testWidgets: hubs $tag (L1571)`
- `testWidgets: terminal $tag (L1609)`

`app/test/chat_test.dart` — remove only these functions/callbacks:

- `test: deltas share their message seq and append; a new seq starts one (L43)`
- `test: thinking and answer with the same id stay separate (L53)`
- `test: a coalesced replay folds to the same transcript as the stream (L60)`
- `test: tool results pair with their call by id (L71)`
- `test: approvals drive state until resolved (L110)`
- `test: usage, compaction and duplicate seq (L135)`
- `test: a handoff marks where the harness changed (L165)`
- `test: status priority (L189)`
- `test: local model rule (L227)`
- `test: fence splitting and tool summaries (L261)`
- `test: reconnect mid-message resumes one earlier and replaces the text (L271)`
- `test: dispose aborts the watch call (L306)`
- `test: send retries keep one request_id (L328)`
- `test: a permanent send error is not retried (L350)`
- `testWidgets: renders the transcript and sends, stops, responds (L385)`
- `testWidgets: compact enabled when idle and shows local model badge (L484)`
- `testWidgets: shows model, status, context, usage and local warning (L543)`
- `testWidgets: a local summary is flagged on the card (L581)`
- `testWidgets: pi on a hosted open model has no warning (L606)`
- `testWidgets: terminal-mode shell has no warning and opens terminal (L623)`

`app/test/desk_test.dart` — remove only these functions/callbacks:

- `test: status words (L129)`
- `test: policy form (L168)`
- `test: screenshot width follows the pane (L199)`
- `testWidgets: the desk shows screen, windows, foreground and use (L211)`
- `testWidgets: the owner sets and clears the policy (L241)`
- `testWidgets: a device without the owner token is told so (L274)`
- `testWidgets: node detail offers the desk only for Windows nodes (L285)`

`app/test/messages_test.dart` — remove only these functions/callbacks:

- `test: delivery states (L165)`
- `test: targets, labels and permissions (L190)`
- `testWidgets: threads show participants, last word and their lamps (L225)`
- `testWidgets: opening a thread shows deliveries and marks it read (L240)`
- `testWidgets: a token without message scopes is told what it needs (L282)`
- `testWidgets: wide layout keeps the list beside the open thread (L296)`
- `testWidgets: filters go to the hub and mail events refresh the list (L306)`
- `testWidgets: a new message goes out with its kind and opens its thread (L340)`

`app/test/model_selection_test.dart` — remove only these functions/callbacks:

- `testWidgets: native catalog controls model-specific efforts and sends selected settings (L33)`
- `testWidgets: unsupported harness explains native defaults without invented options (L96)`
- `test: settings replay updates effective effort independently of request (L116)`

`app/test/nodes_test.dart` — remove only these functions/callbacks:

- `testWidgets: fresh node shows live numbers and no alerts (L93)`
- `testWidgets: tier badge marks care nodes only (L118)`
- `testWidgets: stale node is labeled with its age (L141)`
- `testWidgets: unreachable node hides numbers (L159)`
- `testWidgets: alert badges for disk and memory (L183)`
- `testWidgets: lists worst first and refreshes on node events (L211)`
- `test: ranks containers, engine buckets and watched dirs together (L267)`
- `test: engine containers bucket counts when no per-container data (L294)`
- `test: limit and ordering (L304)`
- `testWidgets: detail shows sections and a disabled cleanup note (L314)`
- `testWidgets: detail says how the node isolates jobs (L346)`
- `test: sleeve workspaces count as offenders (L376)`
- `testWidgets: sleeves screen lists and kills only after confirm (L393)`
- `testWidgets: card lists integrated GPU and NPU without fake meters (L477)`
- `testWidgets: detail shows unknown VRAM use as dashes, not spare (L503)`
- `test: summary uses the first GPU and never invents VRAM use (L547)`
- `test: busy comes from the hub, by GPU index (L597)`
- `test: holders come from live GPU leases of the node (L606)`
- `testWidgets: cards light BUSY per GPU and name the holder (L621)`

`app/test/notes_test.dart` — remove only these functions/callbacks:

- `testWidgets: waits out the pause and sends only what changed (L162)`
- `testWidgets: creates on the first text and updates after (L185)`
- `testWidgets: stays dirty while the hub is down and retries (L203)`
- `testWidgets: a rejected save is not retried (L223)`
- `testWidgets: local edits win over the hub copy (L237)`
- `testWidgets: lists pinned first, then newest, with tags and sources (L253)`
- `testWidgets: a tag chip filters the list (L272)`
- `testWidgets: phone: the editor replaces the list and autosaves (L283)`
- `testWidgets: a tag can be picked from the existing ones (L307)`
- `testWidgets: closing the editor does not drop an unsaved edit (L321)`
- `testWidgets: a new note is created once it has text (L336)`
- `testWidgets: pinning saves right away (L353)`
- `testWidgets: offline edits show and retry until they land (L367)`
- `testWidgets: delete asks first (L384)`
- `testWidgets: desktop: list and editor sit side by side (L406)`
- `testWidgets: a note the orchestrator changed shows up in an open editor (L425)`

`app/test/orchestrator_tab_test.dart` — remove only these functions/callbacks:

- `testWidgets: switches between chat and mail and badges unread mail (L94)`
- `testWidgets: starts envoy from the empty state, on auto or a harness (L116)`
- `testWidgets: start can pick a harness (L133)`
- `testWidgets: respawn asks first and warns about the cache (L144)`
- `testWidgets: resleeve moves envoy to the chosen harness (L168)`
- `testWidgets: other sleeves get no envoy actions (L185)`
- `testWidgets: table callback: c.$1 (L214)`
- `testWidgets: unknown expiry is cold (L220)`
- `testWidgets: shows next to the context meter for a claude sleeve (L225)`
- `testWidgets: talks to envoy and lists only chat sleeves, envoy first (L248)`
- `test: envoy is pinned first in the hub fence (L276)`
- `testWidgets: the envoy card carries an ENVOY badge (L285)`

`app/test/pairing_test.dart` — remove only these functions/callbacks:

- `test: web fragment link (L13)`
- `test: custom scheme link (L26)`
- `test: name defaults to host (L38)`
- `test: rejects other links (L43)`
- `testWidgets: confirm saves the hub and opens it (L77)`
- `testWidgets: cancel saves nothing (L91)`

`app/test/start_work_test.dart` — remove only these functions/callbacks:

- `test: table callback: input (L47)`
- `testWidgets: picks an existing workspace and names the sleeve after it (L52)`
- `testWidgets: a hub-owned workspace fixes the sleeve name (L96)`
- `testWidgets: clone suggests a name from the URL (L116)`
- `testWidgets: path must be absolute and clone must not be a path (L158)`
- `test: the hub node leads, nodes without sleeves get no fence (L198)`
- `testWidgets: each node has a fence holding its own sleeves (L259)`
- `testWidgets: the fence plus starts a sleeve on the picked workspace (L286)`

`app/test/version_skew_test.dart` — remove only these functions/callbacks:

- `test: versionsDiffer ignores unknown versions (L89)`
- `test: versionSkew (L103)`
- `testWidgets: annunciator is dark when everything matches (L168)`
- `testWidgets: a node behind lights it and badges its row (L174)`
- `testWidgets: an app that differs from the hub lights it alone (L187)`
- `testWidgets: card badge carries both versions (L196)`
- `testWidgets: detail shows both versions and offers the update (L217)`
- `test: labels and fractions (L252)`
- `testWidgets: dialog follows the stream to confirmation (L303)`
- `testWidgets: dialog shows a refusal (L356)`

`app/test/vfd_test.dart` — remove only these functions/callbacks:

- `test: lit segment count for a value (L36)`
- `test: segment zones follow the warn and critical thresholds (L53)`
- `test: level tone and readout helpers (L60)`
- `testWidgets: meter fills to the value with threshold colours (L80)`
- `testWidgets: stale meter dims every lit segment (L90)`
- `testWidgets: unknown meter lights nothing (L96)`
- `testWidgets: meter animates its fill, or jumps with reduced motion (L107)`
- `testWidgets: meter row value never truncates (L121)`
- `testWidgets: attention lamp blinks, and holds a ring with reduced motion (L134)`
- `test: only multi-day windows resetting within a day count as soon (L160)`
- `testWidgets: a new beat flickers the lamp once; reduced motion does not (L182)`
- `testWidgets: steady lamp schedules no frames (L216)`
- `testWidgets: readout exposes its value to accessibility (L222)`
- `testWidgets: light theme meters use ink, not phosphor (L234)`

`app/test/widget_test.dart` — remove only these functions/callbacks:

- `test: round trips multiple hubs through shared_preferences (L102)`
- `test: terminal uri maps scheme and path (L125)`
- `test: url validation (L136)`
- `testWidgets: phone width uses bottom navigation (L163)`
- `testWidgets: five destinations in order, on phone and desktop (L196)`
- `testWidgets: desktop width uses navigation rail (L225)`
- `test: bearer interceptor sends the token (L240)`
- `test: reconnects with backoff after the stream ends (L248)`
- `test: aborts the call on retry and stop (L263)`
- `test: quota event payload decodes (L282)`
- `test: sticky ctrl + c is 0x03 and is consumed (L297)`
- `test: ctrl with a bar key sends the plain sequence and clears (L305)`
- `testWidgets: bar buttons emit keys; Ctrl button is sticky (L313)`
- `test: terminal framing (L332)`

### cmd/protectorate: 58 functions/callbacks


`cmd/protectorate/backlog_test.go` — remove only these functions/callbacks:

- `TestBacklogCLI`

`cmd/protectorate/chat_test.go` — remove only these functions/callbacks:

- `TestChatRenderer`

`cmd/protectorate/desk_policy_test.go` — remove only these functions/callbacks:

- `TestDeskStatusLabel`
- `TestPolicyCheck`

`cmd/protectorate/desk_test.go` — remove only these functions/callbacks:

- `TestParseDeskArgs`
- `TestPutGetRoundTrip`

`cmd/protectorate/hub_backup_test.go` — remove only these functions/callbacks:

- `TestHubBackupAndRestoreCommands`

`cmd/protectorate/install_hub_test.go` — remove only these functions/callbacks:

- `TestInstallHub`

`cmd/protectorate/job_test.go` — remove only these functions/callbacks:

- `TestParseTarget`
- `TestParseGitSource`
- `TestParseEnv`
- `TestJobExitStatus`
- `TestKeepLease`
- `TestTestCrossReleasesLease`
- `TestTestCrossLeaseLifecycle`
- `TestFollowJobResumesAcrossHubRestart`

`cmd/protectorate/main_test.go` — remove only these functions/callbacks:

- `TestCLI`
- `TestWriteQuota`
- `TestWriteSuggest`
- `TestLoginStatus`
- `TestInstalledDir`

`cmd/protectorate/mcp_test.go` — remove only these functions/callbacks:

- `TestCheckMCPListen`

`cmd/protectorate/msg_test.go` — remove only these functions/callbacks:

- `TestWriteMail`

`cmd/protectorate/node_install_test.go` — remove only these functions/callbacks:

- `TestPlanInstall`
- `TestWindowsTaskHasNoShellWrapper`
- `TestPlanUninstallDeletesNothing`
- `TestResolveKey`
- `TestInstallDryRunChangesNothing`
- `TestInstallRunner`
- `TestDoctor`
- `TestDoctorParsers`
- `TestUpdateHelpers`
- `TestLingerNote`

`cmd/protectorate/node_test.go` — remove only these functions/callbacks:

- `TestNodeCLI`
- `TestWriteNodeList`
- `TestGpuSummary`
- `TestWriteMetricsAccelerators`
- `TestFmtBytes`
- `TestNodeToken`
- `TestNoSecretFlags`
- `TestNodeMigrateTiers`

`cmd/protectorate/node_update_test.go` — remove only these functions/callbacks:

- `TestUpdateAllSkipsUnreachable`

`cmd/protectorate/orchestrator_test.go` — remove only these functions/callbacks:

- `TestCommandDocsAndWorkflow`
- `TestToolDocsMatchCatalog`
- `TestTokenFileEnv`
- `TestTokenFileIsReadOnEveryCall`

`cmd/protectorate/service_test.go` — remove only these functions/callbacks:

- `TestServiceWriteNoOverwrite`
- `TestServicePrintAndErrors`

`cmd/protectorate/sleeve_test.go` — remove only these functions/callbacks:

- `TestParseSize`
- `TestExitLabels`

`cmd/protectorate/tailnet_test.go` — remove only these functions/callbacks:

- `TestWriteTailnetStatus`
- `TestNodeJoinKeyInputs`

`cmd/protectorate/token_test.go` — remove only these functions/callbacks:

- `TestTokenCLI`
- `TestPairMintsScopedDeviceToken`

`cmd/protectorate/version_test.go` — remove only these functions/callbacks:

- `TestVersionDerivation`
- `TestAppBuildsCarryTheVersion`
- `TestCheckVersion`

`cmd/protectorate/who_test.go` — remove only these functions/callbacks:

- `TestWriteProjects`

### internal/auth: 19 functions/callbacks


`internal/auth/audit_test.go` — remove only these functions/callbacks:

- `TestReadPageAcrossFiles`
- `TestReadPageSkipsATornLastLine`

`internal/auth/auth_test.go` — remove only these functions/callbacks:

- `TestSnake`
- `TestCatalog`
- `TestValidateScopes`
- `TestCreateGrant`
- `TestAdminOn`
- `TestAuthorizeProcedure`
- `TestAuthorizeTerminal`
- `TestAuthenticateLifecycle`
- `TestEmptyOwnerTokenNeverMatches`
- `TestCreateValidation`
- `TestStorePersistenceAndNoSecrets`
- `TestStoreCorruptFileFailsClosed`
- `TestStoreConcurrent`
- `TestAuditAppendAndTail`
- `TestAuditFailureReported`
- `TestRecordOutcomes`
- `TestSubject`

### internal/backlog: 7 functions/callbacks


`internal/backlog/backlog_test.go` — remove only these functions/callbacks:

- `TestCreateValidation`
- `TestUpdate`
- `TestListOrderAndFilter`
- `TestDeleteAndPersistence`
- `TestConcurrentStores`
- `TestMigratesLegacyBacklog`
- `TestEmptyHubHasNoNotesFile`

### internal/backup: 6 functions/callbacks


`internal/backup/backup_test.go` — remove only these functions/callbacks:

- `TestRoundTripWhileDatabaseIsBusy`
- `TestRestoreRefusesNonEmptyHome`
- `TestRestoreRejectsBadArchives`
- `TestManifestListsEverythingOnce`
- `TestLatest`
- `TestRunSchedulesBackups`

### internal/chatlog: 8 functions/callbacks


`internal/chatlog/chatlog_test.go` — remove only these functions/callbacks:

- `TestSeqAndCoalescing`
- `TestSeqIsContiguous`
- `TestReopenKeepsEventsAndCutsPartialLine`
- `TestCorruptLogRefused`
- `TestWatchWhileMessageOpen`
- `TestWatchEndsOnCloseAndContext`
- `TestSlowWatcherIsCut`
- `TestNoGapNoDuplicateAcrossReplayLiveBoundary`

### internal/desk: 13 functions/callbacks


`internal/desk/desk_test.go` — remove only these functions/callbacks:

- `TestParseChord`
- `TestParseButton`
- `TestEncodePNG`
- `TestDownscale`
- `TestCheckInteractive`
- `TestClassifyInputDesktop`
- `TestCheckReach`

`internal/desk/input_type_test.go` — remove only these functions/callbacks:

- `TestTypeGroups`

`internal/desk/policy_test.go` — remove only these functions/callbacks:

- `TestParseWindow`
- `TestInWindow`
- `TestEvaluate`
- `TestInjectedFlags`
- `TestTracker`

### internal/directory: 11 functions/callbacks


`internal/directory/directory_test.go` — remove only these functions/callbacks:

- `TestParseManifests`
- `TestParseTOMLValues`
- `TestNormalizeRemote`
- `TestReadManifestsLayout`
- `TestEdgesAndLookup`
- `TestDirectoryPersistsAndKeepsAbsentProjects`
- `TestInspect`

`internal/directory/refs_test.go` — remove only these functions/callbacks:

- `TestChanges`
- `TestMerge`
- `TestReadRefsAndDescribe`
- `TestDescribeRunsNoRepositoryPrograms`

### internal/driver: 26 functions/callbacks


`internal/driver/driver_test.go` — remove only these functions/callbacks:

- `TestNewSession`
- `TestEventDetails`
- `TestResume`
- `TestApproval`
- `TestInterruptAndSteer`
- `TestExitMidTurn`
- `TestCodexResumeLocked`
- `TestOpenErrors`
- `TestCaps`
- `TestStderrBounded`
- `TestCloseEscalates`
- `TestOwners`
- `TestOwnersOpenDriver`
- `TestCommandWrapsHarness`
- `TestUsageAndToolResults`
- `TestThinkingAndMessageIDs`
- `TestCompact`
- `TestCompactBusy`
- `TestNativeAutoCompaction`
- `TestAutonomousFlags`
- `TestCodexAutonomousLeavesPolicyToConfig`
- `TestSyntheticMessageReportsNoUsage`

`internal/driver/selection_test.go` — remove only these functions/callbacks:

- `TestCodexSelectionProtocol`
- `TestSelectionUnsupportedAndNativeDefaults`
- `TestCodexSelectionAtOpenAndResume`
- `TestSelectionUsesNativeConfirmation`

### internal/fsutil: 2 functions/callbacks


`internal/fsutil/fsutil_test.go` — remove only these functions/callbacks:

- `TestWriteFile`
- `TestWriteFileMissingDir`

### internal/handoff: 8 functions/callbacks


`internal/handoff/handoff_test.go` — remove only these functions/callbacks:

- `TestReadTranscript`
- `TestReadTranscriptRefuses`
- `TestReadTranscriptTail`
- `TestMarkdownBudget`
- `TestDocumentSummaryNotes`
- `TestWrite`
- `TestExclude`
- `TestReadGitFacts`

### internal/hub: 157 functions/callbacks


`internal/hub/auth_test.go` — remove only these functions/callbacks:

- `TestUnaryScopeEnforcement`
- `TestStreamScopeEnforcement`
- `TestTokenServiceOwnerOnly`
- `TestAuditCoversMutationsNotReads`
- `TestTerminalScopeEnforcement`
- `TestTailnetPathComposesIdentityAndToken`
- `TestDefaultHubHasOwnerAuthAndNoDiskWrites`
- `TestSpawnSleeveReservesOrchestratorName`
- `TestChatScopes`
- `TestListAuditEntriesOwnerOnly`

`internal/hub/backlog_test.go` — remove only these functions/callbacks:

- `TestNotesService`

`internal/hub/chat_test.go` — remove only these functions/callbacks:

- `TestChatShellAndMissing`
- `TestChatSupervisedPermissionMode`
- `TestChatTerminalSleeveHasNoChat`
- `TestSleeveTokenReadsOnlyItself`
- `TestChatHubRestartMidTurnFailsThenResumes`

`internal/hub/deps_test.go` — remove only these functions/callbacks:

- `TestDependencyNotices`

`internal/hub/deskpolicy_test.go` — remove only these functions/callbacks:

- `TestDeskPolicyFlow`
- `TestDeskPolicyPersistence`

`internal/hub/deskuse_test.go` — remove only these functions/callbacks:

- `TestDeskUseCountsPerNodeAndCaller`
- `TestDeskUseSurvivesRestartWithinTheDay`

`internal/hub/envoy_test.go` — remove only these functions/callbacks:

- `TestSpawnEnvoyOwnerOnly`
- `TestSpawnEnvoyAndRenew`
- `TestPlanEnvoyRenewal`
- `TestMigrateEnvoyTokens`

`internal/hub/gpubusy_test.go` — remove only these functions/callbacks:

- `TestBusyGPUs`
- `TestNodeFitsSkipsBusyGPUs`
- `TestBusyGPURefusals`

`internal/hub/grants_test.go` — remove only these functions/callbacks:

- `TestGrantAdminMinting`
- `TestAdminJobs`

`internal/hub/hub_test.go` — remove only these functions/callbacks:

- `TestGetStatusGeneratedClient`
- `TestRawJSON`
- `TestAuthRejectedTyped`
- `TestSubscribeHeartbeats`
- `TestServeHTTP2Cleartext`
- `TestToken`
- `TestWebDir`
- `TestServeListenerFailureReturns`

`internal/hub/identity_test.go` — remove only these functions/callbacks:

- `TestSenderOf`
- `TestLocalURL`
- `TestSleeveIdentity`

`internal/hub/install_test.go` — remove only these functions/callbacks:

- `TestInstallScriptGolden`
- `TestInstallScriptQuotesItsURLs`
- `TestInstallShTarget`
- `TestInstallPs1Arch`
- `TestQuoting`
- `TestInstallCommands`
- `TestInstallBase`
- `TestInstallEndpoints`

`internal/hub/invite_test.go` — remove only these functions/callbacks:

- `TestInviteNodeChecks`
- `TestInviteNodeRecordsAndReissues`
- `TestWatchNodeSetup`
- `TestSetupChecks`
- `TestRemoveNodeOwnerOnly`
- `TestRemoveNodeCancelsPendingInvite`
- `TestRemoveNodeDrains`
- `TestSetNodeTier`

`internal/hub/jobkeys_test.go` — remove only these functions/callbacks:

- `TestSubmitKeyDedupe`
- `TestSubmitKeyValidation`
- `TestSubmitKeyConcurrentDuplicatesShareOneJob`
- `TestCancelByKeyRacesSubmit`
- `TestCancelKeyTombstoneExpires`
- `TestSubmitKeyWaitHonorsContext`
- `TestExpiredLeaseIsReselectable`
- `TestSubmitRequestIDEndToEnd`

`internal/hub/jobs_test.go` — remove only these functions/callbacks:

- `TestSubmitValidation`
- `TestWarmNodes`

`internal/hub/jobsource_test.go` — remove only these functions/callbacks:

- `TestBundleIncremental`
- `TestUsablePrerequisites`
- `TestResolveRepoCommit`
- `TestRepoKey`

`internal/hub/lease_queue_test.go` — remove only these functions/callbacks:

- `TestLeaseQueueOrderAndClaim`
- `TestLeaseQueueOnlyForOptIn`
- `TestLeaseQueueResourcesAreIndependent`
- `TestLeaseQueueCancelPersistAndExpire`
- `TestLeaseQueueOverRPC`

`internal/hub/leases_test.go` — remove only these functions/callbacks:

- `TestLeaseExclusivityAndExpiry`
- `TestLeaseRunSweepsExpired`
- `TestLeaseValidationAndRelease`
- `TestLeasePersistence`
- `TestLeaseLoadErrors`
- `TestPickNode`
- `TestNodeFitsAccelerators`
- `TestParseResource`
- `TestLeaseServiceEndToEnd`
- `TestLeasesSurviveHubRestart`
- `TestLeaseTTLHoldsAcrossRestart`

`internal/hub/localmodel_test.go` — remove only these functions/callbacks:

- `TestServedLocally`

`internal/hub/mail_test.go` — remove only these functions/callbacks:

- `TestRenderMessage`
- `TestMailWaitsForIdleAndIsAuthenticated`
- `TestMailReplyKeepsThreadAndAcks`
- `TestMailSendValidationAndDedupe`
- `TestMailSurvivesRestartAndTerminalSleevesUseInbox`
- `TestMailEscalation`
- `TestMailProjectAddressing`
- `TestLeaseGrantSendsNotice`
- `TestMailRequestAcksByDefault`
- `TestMailListThreads`
- `TestDeskEndedNotice`
- `TestEnvoyMailQueue`
- `TestEnvoyQueueRidesAlongWithOwner`

`internal/hub/nodeops_test.go` — remove only these functions/callbacks:

- `TestFileScopesAndAudit`
- `TestDeskAutoLease`
- `TestDeskLeaseRule`
- `TestApplyUpdateRelay`

`internal/hub/nodes_test.go` — remove only these functions/callbacks:

- `TestInventory`
- `TestInventoryLoadErrors`
- `TestAgentURL`
- `TestAlerts`
- `TestMonitorFreshness`
- `TestNodeServiceEndToEnd`
- `TestInvitedNodeLifecycle`
- `TestPollRecoversFromDeadPooledConnection`
- `TestPollAllKeepsNodeAddedMidRound`
- `TestValidNodeAddr`
- `TestNodeTokenNotSentOnRedirect`
- `TestMigrateTiers`

`internal/hub/nodeupdate_test.go` — remove only these functions/callbacks:

- `TestPickBinary`
- `TestNeedsUpdate`
- `TestUpdateStep`
- `TestUpdateNodeStreamsProgress`

`internal/hub/pair_test.go` — remove only these functions/callbacks:

- `TestNewPairLinks`
- `TestNewPairLinksRejectsBadURL`
- `TestWriteQR`

`internal/hub/quota_test.go` — remove only these functions/callbacks:

- `TestFreshness`
- `TestStaleByAgeAlone`
- `TestRefreshRateLimit`
- `TestConcurrentRefreshCollapses`
- `TestCancelledRefreshNotRecorded`
- `TestProjection`
- `TestWindowLength`
- `TestGetQuotaService`
- `TestQuotaUpdatedEvent`
- `TestStrictJSONRejectsUnknownFields`
- `TestStrictJSONTypedError`

`internal/hub/resleeve_test.go` — remove only these functions/callbacks:

- `TestResleeveHandsOffToANewHarness`
- `TestResleeveRefuses`
- `TestResleeveTarget`

`internal/hub/selection_test.go` — remove only these functions/callbacks:

- `TestSelectionRequiresIdle`
- `TestSelectionStatusReportsRequestedAndEffective`

`internal/hub/sessions_test.go` — remove only these functions/callbacks:

- `TestSessionRPC`
- `TestSessionEvents`
- `TestSessionExitEvents`
- `TestDeadRetentionSweep`
- `TestDeadRetentionDisabled`
- `TestTerminalAuthAndErrors`
- `TestTerminalAttachLifecycle`
- `TestTerminalExitedSessionIsReadOnly`
- `TestTerminalSessionExit`

`internal/hub/sleeves_test.go` — remove only these functions/callbacks:

- `TestSleeveRPC`

`internal/hub/suggest_test.go` — remove only these functions/callbacks:

- `TestRankHarnesses`
- `TestRankHarnessesNeverCountsMissingDataAsSpare`
- `TestRankHarnessesLocalReason`
- `TestRankHarnessesReasonText`
- `TestSuggestService`

`internal/hub/tailnet_test.go` — remove only these functions/callbacks:

- `TestTailnetGetStatus`

`internal/hub/workspaces_test.go` — remove only these functions/callbacks:

- `TestListWorkspaces`

### internal/keylog: 2 functions/callbacks


`internal/keylog/keylog_test.go` — remove only these functions/callbacks:

- `TestReloadCompactsAndExpires`
- `TestAppendSurvivesReopenAndCompactsWhenBloated`

### internal/mailbox: 8 functions/callbacks


`internal/mailbox/mailbox_test.go` — remove only these functions/callbacks:

- `TestSendDedupeAndThreads`
- `TestInboxAndAck`
- `TestPersistenceAcrossRestart`
- `TestResolveOnce`
- `TestOverdueTimeline`
- `TestOpenRequests`
- `TestInvolving`
- `TestThreadsSummary`

### internal/mcpbridge: 8 functions/callbacks


`internal/mcpbridge/mcpbridge_test.go` — remove only these functions/callbacks:

- `TestEveryRPCHasDescription`
- `TestOwnerOnlyServicesNotExposed`
- `TestImageMatchesGeneratedCode`
- `TestToolsListGolden`
- `TestSchemaShapes`
- `TestCallTools`
- `TestFilter`
- `TestStreamableHTTP`

### internal/node: 66 functions/callbacks


`internal/node/accel_live_test.go` — remove only these functions/callbacks:

- `TestAccelLive`

`internal/node/accel_test.go` — remove only these functions/callbacks:

- `TestSysfsInventory`
- `TestSysfsEmpty`
- `TestWinAccels`
- `TestMergeGPUs`
- `TestParseXPUSMI`

`internal/node/accel_windows_test.go` — remove only these functions/callbacks:

- `TestListDevicesWindows`

`internal/node/deskguard_test.go` — remove only these functions/callbacks:

- `TestGuardCheck`
- `TestGuardRefusesInputCommands`
- `TestGuardStopsTypingWhenUserReturns`
- `TestGuardLocking`
- `TestGuardPolicyPersistence`
- `TestBlockedInputIsFailedPrecondition`

`internal/node/files_test.go` — remove only these functions/callbacks:

- `TestFilesResolve`
- `TestFilesPutProtocol`
- `TestDeskErrorsAndValidation`

`internal/node/jobs_test.go` — remove only these functions/callbacks:

- `TestNewJobsClearsStaleScratchButKeepsRepos`
- `TestUpload`
- `TestValidateRun`
- `TestMatchArtifacts`
- `TestRunOutputExitAndEnv`
- `TestRunTimeoutKillsTree`
- `TestRunStubbornProcessIsKilledAfterGrace`
- `TestRunSurvivesDisconnectAndAttachResumes`
- `TestAttachErrors`
- `TestFinishedJobsAreSweptAfterRetention`
- `TestRunWaitsForSlotAndCancelWhileQueued`
- `TestRunDuplicateJobID`
- `TestRunGitWithoutBundleOrCommitFails`
- `TestRunLogIsCapped`
- `TestAgentJobRPCsNeedAuthAndEnablement`
- `TestExtractZipRefusesUnsafeEntries`

`internal/node/node_test.go` — remove only these functions/callbacks:

- `TestParseNvidiaSMI`
- `TestParseROCmSMI`
- `TestParseDF`
- `TestApplyStats`
- `TestNegotiateAPI`
- `TestCPUBusyPercent`
- `TestKeepPartition`
- `TestDedupeFilesystems`
- `TestDirSize`
- `TestEngineMetrics`
- `TestEngineAbsent`
- `TestCollectLocal`
- `TestAgent`
- `TestAndroidSystemMount`
- `TestFSProberBlockedMount`
- `TestMarkPrimary`

`internal/node/process_unix_test.go` — remove only these functions/callbacks:

- `TestCommandFor`

`internal/node/runner_test.go` — remove only these functions/callbacks:

- `TestWindowsCommandLine`
- `TestMergeEnvAndBlock`
- `TestCPURate`
- `TestRunnerPassword`
- `TestRunnerPlanText`
- `TestReadRunnerConfig`

`internal/node/sandbox_unix_test.go` — remove only these functions/callbacks:

- `TestSandboxPlan`
- `TestSandboxedJobLive`
- `TestSandboxedJobLimitsAndCancelLive`
- `TestSandboxJobDirUnderHomeLeavesNoMountPoints`

`internal/node/update_test.go` — remove only these functions/callbacks:

- `TestSwapAndRollbackPlans`
- `TestDecideStartup`
- `TestPlanRestart`
- `TestRunOpsUndoesRenames`
- `TestApplyUpdateEndToEnd`
- `TestUnconfirmedUpdateRollsBack`
- `TestApplyUpdateRefusals`

### internal/orchestrator: 10 functions/callbacks


`internal/orchestrator/orchestrator_test.go` — remove only these functions/callbacks:

- `TestDefaultScopes`
- `TestDefaultScopesDropDestructive`
- `TestRenderAgentsGolden`
- `TestRenderAgentsStableOrdering`
- `TestSpawnMintsAndRevokesOnRespawn`
- `TestSpawnFailureRevokesNewToken`
- `TestSpawnCustomScopesAndHarness`
- `TestSpawnAutoPicksSuggestion`
- `TestWriteContextMovesForeignClaudeMdAside`
- `TestSpawnContextIsCacheStable`

### internal/quota: 7 functions/callbacks


`internal/quota/quota_test.go` — remove only these functions/callbacks:

- `TestClaude`
- `TestCodexAppServer`
- `TestCodexWhamFallbackAndExpiry`
- `TestGrok`
- `TestAntigravityParse`
- `TestLocal`
- `TestReadAll`

### internal/session: 6 functions/callbacks


`internal/session/session_test.go` — remove only these functions/callbacks:

- `TestValidName`
- `TestParseList`
- `TestEscapeArg`
- `TestStripDeadBanner`
- `TestTail`
- `TestRunLimitsOutput`

### internal/sleeve: 33 functions/callbacks


`internal/sleeve/layout_test.go` — remove only these functions/callbacks:

- `TestSharedAuthVisibleAcrossSleeves`
- `TestLayoutApply`
- `TestLayoutEnv`
- `TestCodexConfigSeed`
- `TestProfileKeepsSleevePathInLoginShells`

`internal/sleeve/login_test.go` — remove only these functions/callbacks:

- `TestLoginPlan`
- `TestLoggedIn`

`internal/sleeve/manager_test.go` — remove only these functions/callbacks:

- `TestSpawnRejectsBadRequests`
- `TestKillRestoresRecordWhenSaveFails`
- `TestFailedStartLeavesNothingRunning`

`internal/sleeve/mode_test.go` — remove only these functions/callbacks:

- `TestResolveMode`
- `TestTerminalArgs`

`internal/sleeve/plan_test.go` — remove only these functions/callbacks:

- `TestPlanGolden`
- `TestPlanRejects`
- `TestWithin`
- `TestParseHarness`
- `TestLocateFollowsSymlinkChain`
- `TestAddBinaryDir`
- `TestCheckWorkspaceStateOwnership`
- `TestCheckWorkspaceProtectedTargets`
- `TestDefaultConfigCanonicalizesPaths`
- `TestDefaultToolDirsAreNarrow`
- `TestPlanMasksCredentialFiles`
- `TestLocateCredentialDirChains`
- `TestPlanKeepsEnvOutOfArgv`
- `TestEnvFileRoundTrip`
- `TestLaunchKeepsEnvAndArgs`

`internal/sleeve/rename_test.go` — remove only these functions/callbacks:

- `TestRenameRecord`

`internal/sleeve/selection_test.go` — remove only these functions/callbacks:

- `TestSelectionPersistence`
- `TestCatalogHasNoWorkspaceOrRealHomeMount`
- `TestSelectionPersistenceFailureKeepsPreviousRecord`

`internal/sleeve/workspaces_test.go` — remove only these functions/callbacks:

- `TestGitDetails`
- `TestWorkspaces`

### internal/svcgen: 4 functions/callbacks


`internal/svcgen/svcgen_test.go` — remove only these functions/callbacks:

- `TestGolden`
- `TestValidation`
- `TestUTF16`
- `TestSystemdVerify`

### internal/tailnet: 20 functions/callbacks


`internal/tailnet/status_test.go` — remove only these functions/callbacks:

- `TestStatusProto`

`internal/tailnet/tailnet_test.go` — remove only these functions/callbacks:

- `TestConfigValidate`
- `TestHeadscaleConfig`
- `TestDERPMapAndCert`
- `TestPolicy`
- `TestSanitizeLabel`
- `TestIdentify`
- `TestRetryDial`
- `TestRetryDialContextCancel`
- `TestParseChecksum`
- `TestInstall`
- `TestPinnedChecksumMismatch`
- `TestConfigSaveLoad`
- `TestSocketPathShort`
- `TestRotatingLog`
- `TestNodeEnrolledWith`
- `TestTransportBoundsEveryPhase`
- `TestHubAddr`
- `TestQuietDERPLog`
- `TestRetier`

### internal/xfer: 6 functions/callbacks


`internal/xfer/xfer_test.go` — remove only these functions/callbacks:

- `TestFileRoundTrip`
- `TestCommitVerification`
- `TestOverwrite`
- `TestDirRoundTrip`
- `TestUnpackRejectsUnsafeEntries`
- `TestWriteRejectsBadChunks`
