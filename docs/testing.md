# Testing policy

Only substantive end-to-end scenarios and fuzz targets belong in the active
suite. No unit, widget, golden-output or implementation-shape tests. A real
transport alone is insufficient: assert a consequential behavior across the
production boundary. Deterministic executable peers speaking each harness's
official protocol are allowed; installed AI CLIs are never wrapped or patched.
The archive and references are read-only and outside this suite.

Run `make test`: it builds the binary with `TAILSCALE_TAGS`, runs tagged vet,
then runs all fuzz seed corpora and end-to-end tests with `PROT_E2E=1` and
`PROT_E2E_BINARY` set to the built binary. It clears inherited
`PROTECTORATE_TOKEN_FILE`, `PROTECTORATE_HUB` and `PROTECTORATE_TOKEN`.
CI uses this entry point. Release additionally runs `tests/e2e` against its
cross-built linux/amd64 binary with the same release feature tags.
`make app-check` runs `flutter analyze`; no Dart unit/widget tests remain.

`PROT_E2E=1 go test ./tests/e2e` also works, building one binary if necessary.
Supply an absolute `PROT_E2E_BINARY` to avoid that build. Build time is separate
from the execution budget. A cluster has a 75-second process deadline and a
five-minute compilation deadline; every subprocess is reaped after bounded
shutdown. Each child gets explicit private HOME, state, credentials, node
bearer and tmux directory. Tests never use the running hub or provider logins.
The node executes a private binary copy; its updater uses the production
supervised-exit path with the harness acting as supervisor, preventing detached
replacement processes from escaping cleanup.

`TestBinaryClusterRestartAndRestore` covers token escalation/refusal and
revocation, expiry, persisted widening/narrowing, deny-by-default across every unary hub
method and audit redaction, care-tier plain
and wrong-node grant denial, owner grant acceptance, selector exclusion,
mail persistence/dedupe/ack, exclusive leases, priority/FIFO/claim across restart,
large binary transfer and job output, traversal and symlink refusal, git
bundles and warm worktrees, artifacts, hub restart mid-job and exactly-once
log replay, diagnostics authorization/profiles, MCP initialize/tools/list/call,
authenticated terminal WebSocket replay, node download checksums, staged update
refusal, replacement/health confirmation and unconfirmed-start rollback.
Backup checks restore saved jobs/tokens and refuse corrupt, truncated,
duplicate, traversal, symlink, content-tampered and nonempty destinations.

The loopback hub has no tailnet: `/install.sh` must return 503. The production
installer template is separately run as a real shell process against that
hub's `/dl/` endpoints; a checksum peer corrupts only the digest response and
the script must refuse to execute the download. Full enrollment and service
registration/uninstall require the optional tailnet/host lane; this loopback
scenario does not claim to test them.

On hosts with systemd user manager, bwrap namespaces, tmux and git, the binary
scenario also checks sandbox isolation, persisted ownership, cross-sleeve kill
denial and official Claude chat folding. In-package hub scenarios retain
real sandbox/driver/HTTP chat approval, interruption, compaction, replay,
authenticated idle-only mailbox delivery, wait/wake, threading, routing,
redelivery/escalation, grant notices and idle reap. They need package internals
for the delivery clock and coordination passes. Driver scenarios launch real
stdio peers for Codex app-server, Claude stream-json, Grok ACP and pi RPC,
including resume, approvals, interruption, usage, tools, selection, compaction,
process exit and ownership. Sleeve scenarios retain real cgroup limits,
lifecycle, warm git clone, toolchain/cache and jailed repository execution.

A sleeve spawned with `--nested --cpu 200 --mem 5G` runs these runtime lanes
through private delegated cgroups and nested bwrap. A normal sleeve still
skips them. `TestBinaryNestedRuntime` starts a real nested-enabled sleeve and
runs the selected production binary's runtime scenarios inside it, failing if
the inner run skips. It also verifies actual memory OOM, CPU throttling, hidden
parent state, persisted opt-in, unchanged outer limits and cgroup cleanup.
Private test homes retain the selected bwrap binary, and the nested scenario
asserts its version so a newer user-installed bwrap reaches the inner runtime.
This host e2e remains unverified in the original non-delegated development
sleeve; see [the runtime setup](sleeves.md#running-runtime-tests-inside-a-sleeve).

All surviving session-manager tests set `TMUX_TMPDIR` to a test temp directory
and kill their private server in cleanup. The binary harness does likewise.
A forced kill can leave a server within its private directory; it cannot leave
new sockets in the production `/tmp/tmux-UID` directory. Ordinary exits clean
up servers and temp directories. No production tmux defaults change.

Default resolved `~/.protectorate` keeps `prot-sleeve-<name>`. Other resolved
homes use `prot-<12 hex SHA-256 digits of home>-sleeve-<name>`; these prefixes
do not overlap the default namespace. Symlink aliases share the same prefix.
Before upgrading a custom-home hub, use its old CLI to stop each live sleeve:
`PROTECTORATE_HOME=/path/to/custom-home protectorate --hub URL sleeve kill NAME`, then restart the upgraded hub and
respawn. No migration is needed for the default home.

Optional tailnet lane (private Headscale, tsnet, DERP and live SQLite backup):

```sh
PROT_TAILNET_IT=1 go test -count=1 -timeout 15m -tags "$(make -s build-tags)" ./internal/tailnet
```

Fuzz seeds run on every commit. Mutation campaigns are separate, for example:

```sh
go test ./internal/sleeve -run '^$' -fuzz '^FuzzHomeScopeNamespace$' -fuzztime=10s -parallel=2
go test ./internal/auth -run '^$' -fuzz '^FuzzScopeNormalization$' -fuzztime=10s -parallel=2
```

The other targets enforce workspace authority, lossless environment encoding
and filesystem path confinement. Namespace fuzzing checks canonical/symlink
home equivalence, independent-home separation and exclusion from the default
scope prefix.

`TestBinaryWorkspaceBrowser` drives a real temporary hub and CLI through
default and explicit roots, drill-down, hidden/file filtering, symlink escape
refusal, result truncation, rejected-workspace reasons, credential-stripped
origins, paired-device acceptance and sleeve-identity denial. A persisted
exited sleeve verifies branch refresh in `sleeve ls` and `sleeve show`, in
text and JSON, without needing the host user manager.

Before/after baseline: wave 1 reported 59.57s non-race on two cores with warm
caches; the owner's earlier race baseline was 277s. Neither is remeasured here.
Phase 2's final tagged suite measured 14.02s from first to last Go JSON event
on two cores with warm caches, with 10.49s in the binary scenario. An earlier
run competing with untagged vet and Flutter analysis took 24.32s. Cold
compilation is excluded. The final suite passed; host runtime subscenarios and
tailnet were explicitly skipped in this sleeve. All five three-second fuzz
campaigns passed (15,482 scope, 8,001 confinement, 20,186 workspace, 69,736
environment and 82 namespace executions; a final relative-path namespace recheck added
eight executions. Filesystem namespace fuzzing is
intentionally slower).
Host-only runtime,
full enrollment/service lifecycle, display consent, real provider quota,
two-harness handoff and Flutter real-hub UI flows remain outside the measured
loopback lane. The deleted unit tests are not claimed as replacement evidence
for those flows. The coordinator must run host/runtime and tailnet lanes.

Regenerate the inventory with `python3 scripts/test-inventory.py --go-results
/tmp/protectorate-tests.json` after collecting tagged `go test -json ./...`
with the same PROT_E2E environment. The explicit KEEP list makes newly added
unreviewed tests appear as CONVERT rather than automatically accepting them.


## Inventory

Snapshot counts: KEEP-E2E=60, KEEP-FUZZ=5; total=65. TestMain rows are infrastructure, not scenarios.

Runtime is the observed non-race test-function elapsed time, including skips. It excludes build time and is not a race-suite prediction. Unknown means unmeasured, never zero. Dart callback labels include source lines to distinguish repeated labels.

| File | Function / callback | Exercises | Runtime | Verdict | Replacement / reason |
|---|---|---|---|---|---|
| internal/auth/scopes_fuzz_test.go | FuzzScopeNormalization | Scope Normalization | 0.010s pass | KEEP-FUZZ | seed corpus on commits; bounded campaigns separately |
| internal/driver/driver_test.go | TestNewSession | New Session | 0.020s pass | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/driver/driver_test.go | TestEventDetails | Event Details | 0.020s pass | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/driver/driver_test.go | TestResume | Resume | 0.020s pass | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/driver/driver_test.go | TestApproval | Approval | 0.060s pass | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/driver/driver_test.go | TestInterruptAndSteer | Interrupt And Steer | 0.040s pass | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/driver/driver_test.go | TestExitMidTurn | Exit Mid Turn | 0.060s pass | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/driver/driver_test.go | TestCodexResumeLocked | Codex Resume Locked | 0.000s pass | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/driver/driver_test.go | TestStderrBounded | Stderr Bounded | 0.000s pass | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/driver/driver_test.go | TestCloseEscalates | Close Escalates | 0.010s pass | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/driver/driver_test.go | TestOwnersOpenDriver | Owners Open Driver | 0.010s pass | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/driver/driver_test.go | TestCommandWrapsHarness | Command Wraps Harness | 0.000s pass | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/driver/driver_test.go | TestUsageAndToolResults | Usage And Tool Results | 0.010s pass | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/driver/driver_test.go | TestThinkingAndMessageIDs | Thinking And Message IDs | 0.000s pass | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/driver/driver_test.go | TestCompact | Compact | 0.020s pass | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/driver/driver_test.go | TestCompactBusy | Compact Busy | 0.000s pass | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/driver/driver_test.go | TestNativeAutoCompaction | Native Auto Compaction | 0.000s pass | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/driver/driver_test.go | TestAutonomousFlags | Autonomous Flags | 0.020s pass | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/driver/driver_test.go | TestCodexAutonomousLeavesPolicyToConfig | Codex Autonomous Leaves Policy To Config | 0.010s pass | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/driver/driver_test.go | TestSyntheticMessageReportsNoUsage | Synthetic Message Reports No Usage | 0.000s pass | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/driver/fake_test.go | TestMain | Main | not measured | KEEP-E2E | runner infrastructure; retain while any package tests use it |
| internal/driver/selection_test.go | TestCodexSelectionProtocol | Codex Selection Protocol | 0.010s pass | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/driver/selection_test.go | TestSelectionUnsupportedAndNativeDefaults | Selection Unsupported And Native Defaults | 0.010s pass | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/driver/selection_test.go | TestCodexSelectionAtOpenAndResume | Codex Selection At Open And Resume | 0.010s pass | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/driver/selection_test.go | TestSelectionUsesNativeConfirmation | Selection Uses Native Confirmation | 0.010s pass | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/hub/chat_test.go | TestChatEndToEnd | Chat End To End | 0.010s skip | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/hub/chat_test.go | TestChatShellAndMissing | Chat Shell And Missing | 0.020s skip | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/hub/chat_test.go | TestChatSupervisedPermissionMode | Chat Supervised Permission Mode | 0.040s pass | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/hub/chat_test.go | TestChatTerminalSleeveHasNoChat | Chat Terminal Sleeve Has No Chat | 0.010s skip | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/hub/chat_test.go | TestSleeveTokenReadsOnlyItself | Sleeve Token Reads Only Itself | 0.010s skip | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/hub/chat_test.go | TestChatSendRequestIDsSurviveHubRestart | Chat Send Request IDs Survive Hub Restart | 0.010s skip | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/hub/mail_test.go | TestMailWaitsForIdleAndIsAuthenticated | Mail Waits For Idle And Is Authenticated | 0.010s skip | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/hub/mail_test.go | TestMailReplyKeepsThreadAndAcks | Mail Reply Keeps Thread And Acks | 0.020s skip | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/hub/mail_test.go | TestMailSendValidationAndDedupe | Mail Send Validation And Dedupe | 0.010s skip | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/hub/mail_test.go | TestMailSurvivesRestartAndTerminalSleevesUseInbox | Mail Survives Restart And Terminal Sleeves Use Inbox | 0.010s skip | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/hub/mail_test.go | TestMailEscalation | Mail Escalation | 0.020s pass | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/hub/mail_test.go | TestMailProjectAddressing | Mail Project Addressing | 0.010s skip | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/hub/mail_test.go | TestLeaseGrantSendsNotice | Lease Grant Sends Notice | 0.010s skip | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/hub/mail_test.go | TestMailRequestAcksByDefault | Mail Request Acks By Default | 0.010s skip | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/hub/mail_test.go | TestMailListThreads | Mail List Threads | 0.010s skip | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/hub/mail_test.go | TestDeskEndedNotice | Desk Ended Notice | 0.010s skip | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/hub/mail_test.go | TestEnvoyMailQueue | Envoy Mail Queue | 0.060s pass | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/hub/mail_test.go | TestEnvoyMailAfterOwner | Envoy Mail After Owner | 0.010s skip | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/hub/mail_wake_e2e_test.go | TestMailWakeWaitE2E | Mail Wake Wait E2 E | 0.010s skip | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/hub/sleeve_runtime_e2e_test.go | TestSleeveReadOnlyMountHTTP | Sleeve Read Only Mount HTTP | 0.010s skip | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/hub/sleeve_runtime_e2e_test.go | TestSleeveIdleReapHTTP | Sleeve Idle Reap HTTP | 0.040s skip | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/node/files_fuzz_test.go | FuzzConfinedFilePaths | Confined File Paths | 0.000s pass | KEEP-FUZZ | seed corpus on commits; bounded campaigns separately |
| internal/sleeve/development_e2e_test.go | TestDevelopmentToolchainsEndToEnd | Development Toolchains End To End | 0.030s skip | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/sleeve/git_test.go | TestGitRunsRepositoryProgramsInAJail | Git Runs Repository Programs In AJail | 0.200s pass | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/sleeve/manager_test.go | TestSleeveLifecycle | Sleeve Lifecycle | 0.010s skip | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/sleeve/manager_test.go | TestMemoryLimitEnforced | Memory Limit Enforced | 0.010s skip | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/sleeve/manager_test.go | TestSleeveExitKeepsSession | Sleeve Exit Keeps Session | 0.010s skip | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/sleeve/manager_test.go | TestSleeveExitReasons | Sleeve Exit Reasons | 0.010s skip | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/sleeve/manager_test.go | TestSpawnClonesGitURL | Spawn Clones Git URL | 0.010s skip | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/sleeve/manager_test.go | TestKillRestoresRecordWhenSaveFails | Kill Restores Record When Save Fails | 0.010s skip | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/sleeve/path_fuzz_test.go | FuzzWorkspaceBoundary | Workspace Boundary | 0.000s pass | KEEP-FUZZ | seed corpus on commits; bounded campaigns separately |
| internal/sleeve/path_fuzz_test.go | FuzzEnvironmentArgumentEncoding | Environment Argument Encoding | 0.000s pass | KEEP-FUZZ | seed corpus on commits; bounded campaigns separately |
| internal/sleeve/path_fuzz_test.go | FuzzHomeScopeNamespace | Home Scope Namespace | 0.150s pass | KEEP-FUZZ | seed corpus on commits; bounded campaigns separately |
| internal/tailnet/backup_it_test.go | TestBackupRestoreWithLiveHeadscale | Backup Restore With Live Headscale | 0.000s skip | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/tailnet/integration_test.go | TestTailnetEndToEnd | Tailnet End To End | 0.000s skip | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/tailnet/integration_test.go | TestNodeRestartRecovery | Node Restart Recovery | 0.000s skip | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| internal/tailnet/integration_test.go | TestDERPRelay | DERPRelay | 0.000s skip | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| tests/e2e/workflow_test.go | TestBinaryClusterRestartAndRestore | Binary Cluster Restart And Restore | 10.490s pass | KEEP-E2E | existing real process/HTTP workflow; do not delete without replacement |
| tests/e2e/workspace_browser_test.go | TestBinaryWorkspaceBrowser | Real hub/CLI workspace discovery, confinement, identity denial and live branch refresh | unmeasured | KEEP-E2E | directory browser regression scenario |
| tests/e2e/nested_runtime_test.go | TestBinaryNestedRuntime | Real nested sleeve runs the selected production binary's isolation, ownership, restart, chat, OOM and CPU throttling scenarios; verifies hidden ancestor limits and subtree cleanup | host verification pending | KEEP-E2E | requires host runtime or a nested-enabled sleeve; fails on inner skips |

## Deleted files

See [the phase 2 deletion record](testing-deletions.md) for the exact paths grouped by package.
