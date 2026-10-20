# Testing policy

Only substantive end-to-end scenarios and fuzz targets belong in the active
suite. No unit, widget, golden-output or implementation-shape tests. A real
transport alone is insufficient: assert a consequential behavior across the
production boundary. Deterministic executable peers speaking each harness's
official protocol are allowed; installed AI CLIs are never wrapped or patched.
The archive and references are read-only and outside this suite.

Run `make test`: it builds the binary with `TAILSCALE_TAGS`, runs tagged vet,
then runs all fuzz seed corpora and end-to-end tests with `PROT_E2E=1` and
`PROT_E2E_BINARY` set to the built binary. The shared `internal/testenv.E2E`
entry point clears inherited `PROTECTORATE_TOKEN_FILE`, `PROTECTORATE_HUB`
and `PROTECTORATE_TOKEN` itself, including when tests run inside a sleeve.
CI uses this entry point. Release additionally runs `tests/e2e` against its
cross-built linux/amd64 binary with the same release feature tags.
`make app-check` runs `flutter analyze`; no Dart unit/widget tests remain.

`TestBinaryPhoneEmergencyStop` exercises the phone's direct hub workflow with
a scoped device token, real shell sleeve process trees and Envoy on an official
protocol peer. It checks read-only denial, non-Envoy cgroup teardown and token
revocation, then a streamed reply from the surviving Envoy. It repeats the
stop with Envoy unavailable. This lane requires the host runtime or delegated
nested runtime; it never calls the running hub.

`TestBinaryTerminalLauncher` sends more than 40 KiB of literal shell arguments
through a real terminal sleeve, checks argument fidelity and environment
privacy, then restarts the hub and respawns that sleeve. Terminal launches use
a private, atomically written launcher so tmux does not have to transport the
full delegated namespace command. For nested runs, use a short disk-backed
temporary root such as `TMPDIR="$HOME/.cache/pt"` to keep binary and cache
fixtures off tmpfs. The binary harness isolates tmux sockets in short private
directories under `/tmp`, independently of that root. The deeper nested
scenario uses private synthetic SDK/cache lower trees to avoid stacking an
overlay on the development sleeve's already overlaid SDK caches; it keeps
all real namespace, cgroup, OOM and CPU throttling checks and rejects skips.

The native Android shell has four tabs; web and native desktop retain the full
shell at every viewport width. Build both shipping clients with `make apk` and
`make app-build-web`. For visual review against a throwaway real hub, run:

```sh
E2E_PHONE=1 scripts/e2e-screens.sh
scripts/e2e-screens.sh
```

The phone command builds debug web with `DEBUG_PHONE_SHELL=true`; the override
is gated by `kDebugMode` and cannot enable the phone shell in a release web
build. It captures 360 and 412 px widths in light and dark themes under
`e2e-out/phone-*.png`, including the Envoy start state, streamed markdown/code
and a pinned permission request. Its private executable speaks Claude's
official protocol without invoking an installed AI CLI or executing requested
commands. These screenshots verify the client approval display; they do not
establish provider-specific supervised behavior. The default command captures
the full responsive web shell at 412 and 1280 px. Both commands use private
hub state and scrub inherited sleeve credentials. `E2E_ONLY` filters captures,
`E2E_OUT` selects the output directory, and `CHROMIUM` selects the browser.
Authentication uses a real CLI device login, approval by the temporary hub's
owner and saved browser profiles injected through Chromium's debugging API.
No credential appears in the pairing URL and there is no production bypass.
The hub keeps a shared approval banner above its nested navigator, so pending
devices remain actionable on tabs, node details and other pushed pages.
Phone Envoy captures type an unsent draft, switch to NODES and back, and assert
the editor retained the text. NODES captures use the actual bottom tab button.
A separate node-detail capture checks the pending device banner on a pushed
page; the temporary device request stays unapproved during these scenes.

Run `make test-race` to build `bin/protectorate-race` with `-race`, the same
feature tags, trimpath and version ldflags as `make build`, and run the full
Go e2e suite and fuzz seeds with `go test -race`. It forces `CGO_ENABLED=1`
and fails with an explicit diagnostic if the configured C compiler is missing.
`go test -race` alone does not instrument an ordinary supplied or fallback
application binary; use this target to cover both sides of the process boundary.
CI runs it in a separate Ubuntu job; release builds remain non-race.

On this two-core linux/arm64 sleeve, the passing warm-cache target took
114.10s wall time, with 98.63s in `tests/e2e` (18.68s for the non-race binary
scenarios). The initial cold-cache attempt
took 744.75s including module downloads and compilation; it exposed a
care-tier scenario's transient-node-readiness assumption, fixed before the
passing run. `make check` keeps its existing non-race lane: adding nearly two
minutes to each warm local check is left to the explicit race target and CI.

The race lane uses `GORACE=halt_on_error=1 atexit_sleep_ms=0`: the first
detected race terminates the process, and short-lived CLI calls do not each
sleep a second on exit. The binary harness also writes race reports to files
named for each process with PID suffixes in each scenario's temporary directory.
After shutting down its processes it checks those files and all reachable
`.log` files, including hub/node/MCP stderr and node job output, plus sleeve
`chat.jsonl` logs (and rotated copies) containing captured driver stderr. CLI
combined output is checked even for expected command failures. Any `WARNING: DATA RACE`
fails the owning scenario and prints the process/log name and full report,
including when the scenario's assertions passed. These log checks also run
with non-race binaries, so reports from separately instrumented helpers count.
Remaining tmux panes are captured and checked before the private server stops.

`TestBinaryRaceReports` builds a deliberately racing Go peer in the race lane
and runs otherwise passing scenarios in subprocesses. It requires failures
with complete reports from hub and node logs, CLI output, redirected CLI race
files, and a successful artifact job executed by the real node agent.
Where tmux is installed, it also checks a terminal helper whose final screen
contains a race report.
The detector only covers executed code paths: omitted e2e workflows, unexercised
interleavings, non-Go/uninstrumented helpers and unreachable child output are
not covered. Runtime and tailnet scenarios still require their documented host
prerequisites and opt-in lane; skips inside a sleeve do not verify those paths.

Inside a nested sleeve, use disk-backed temporary storage such as
`mkdir -p "$HOME/.cache/pt"` followed by `TMPDIR="$HOME/.cache/pt" make check`.
Runtime helpers keep tmux sockets in separate short private directories.

`PROT_E2E=1 go test ./tests/e2e` also works, building one binary if necessary.
Supply an absolute `PROT_E2E_BINARY` to avoid that build. This is the single
gate for real-binary/process scenarios in `tests/e2e`, the hub chat/runtime
harness, `internal/sleeve` and `internal/tailnet`. In-package scenarios drive
their own production entry points and do not need the CLI binary path. Driver
protocol fixtures and fuzz targets run without a gate; they need no installed
AI CLI, provider login or runtime. Runtime capability checks may additionally
skip when systemd user scopes or namespaces are unavailable. No manual
credential unsets are required.

Build time is separate from the execution budget. A cluster has a 75-second process deadline and a
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

`TestBinaryDeviceAuthorization` drives unauthenticated device authorization
against the real hub binary, then uses approved tokens across production
authorization and event streams. It covers pending and live request events,
case-insensitive owner approval, device approval, denial, expiry, polling
slow-down, single delivery of secrets, owner-only operation refusal, scoped
and unauthenticated approver refusal, existing paired devices, rename,
revoked credentials across restart and immediate closure of a live
device stream. It runs a second CLI process with a private empty state home,
approves its displayed code using `devices approve`, and proves the saved
login works in a subsequent CLI command. `devices ls`, `deny`, `rename` and
`revoke` also run as real commands. Audit and event checks reject leaked
device codes and token secrets.
Reserved sleeve/orchestrator device names, renaming to a reserved identity and
changing a device's Full scopes are refused. The device sends to the owner's
mailbox, the owner sees it, and the device acknowledges it; attempting another
named mailbox is denied.

`TestBinaryDeviceAuthorizationLimits` binds HTTP clients to distinct loopback
source IPs to exercise both per-IP and global start/poll limits and the
100-request pending cap, including malformed-body attempts that must consume
the start budget. It sends a forged `X-Forwarded-For` header from an
untrusted peer; the authorization scenario checks the real TCP IP is shown.
The device authorization clock advances through a private timestamp file
selected by `PROT_E2E_DEVICE_AUTH_CLOCK_FILE`, accepted only with `PROT_E2E=1`.
This makes ten-minute expiry and rate-window reset deterministic while all
requests still traverse the real server. Real CLI polling uses its production
interval. Run these directly after `make build`:

```sh
PROT_E2E=1 PROT_E2E_BINARY="$PWD/bin/protectorate" \
  go test -count=1 -v ./tests/e2e -run '^TestBinaryDeviceAuthorization'
```

`TestBinaryAppHubConnection` also runs the Dart client's production
`authorizeDevice` helper against the real hub: owner approval, one-time secret
delivery, denial and cancellation, alongside connection probing and useful
unreachable-hub errors. It requires Dart and resolved app dependencies.

`TestBinaryGitHubReleaseUpdates` builds one genuine versioned candidate and
starts a private fake GitHub API and asset server. It verifies stable, rolling
dev and pinned release selection, rejects a corrupt binary before activation,
and updates a real hub while preserving its arguments. Its release contains
all five platform binaries, checksums, VERSION and a web archive. The scenario
checks web/download serving, updates a real node from that release, rolls back
a candidate that reports its version but fails to serve, and retains only two
release directories. After a healthy standalone update, its guardian PID must
disappear after confirmation, catching unreaped watchdog processes across
re-exec. An inherited session invocation ID must still use standalone re-exec.
All release requests stay on loopback; no GitHub access
or installed service registration is needed. The rollback guardian PID comes
from the private hub update journal and is stopped during test cleanup.
The hub accepts `--release-api-url URL` for a GitHub-compatible API override;
the scenario supplies its loopback server, a private `--release-dir`, and
`--hub-update-timeout 3s`. Production defaults to `https://api.github.com`
and a 45-second hub health deadline.

`TestHubInstallerVerifiesFleetBeforeInstalling` runs the real shell installer
with real binary assets on a loopback release server. It checks stable and dev
versioned layouts, verifies every fleet asset, and requires a corrupt Android
checksum to stop all installation commands. Dry runs create no installation
directories.

The loopback hub has no tailnet: `/install.sh` must return 503. The production
installer template is separately run as a real shell process against that
hub's `/dl/` endpoints; a checksum peer corrupts only the digest response and
the script must refuse to execute the download. Full enrollment and service
registration/uninstall require the optional tailnet/host lane; this loopback
scenario does not claim to test them.

On hosts with systemd user manager, bwrap namespaces, tmux and git, the binary
scenario also checks sandbox isolation, persisted ownership, cross-sleeve kill
denial and official Claude chat folding. Its Codex protocol peer covers default
requested effort `high`, explicit effort precedence, unsupported-default
fallback across restart, resleeve onto Codex, Envoy, custom and empty hub defaults,
and the effective settings received by a native turn, without a Codex login.
In-package hub scenarios retain
real sandbox/driver/HTTP chat approval, interruption, compaction, replay,
authenticated idle-only mailbox delivery, wait/wake, threading, routing,
redelivery/escalation, grant notices and idle reap. They need package internals
for the delivery clock and coordination passes. Driver scenarios launch real
stdio peers for Codex app-server, Claude stream-json, Grok ACP and pi RPC,
including resume, approvals, interruption, usage, tools, selection, compaction,
process exit and ownership. Sleeve scenarios retain real cgroup limits,
lifecycle, warm git clone, toolchain/cache and jailed repository execution.

`TestBinarySleeveCaches` starts private real hubs and shell sleeves on hosts
with a systemd user manager or delegated nested runtime. Its overlay lane uses a non-Dart workspace,
writes Flutter/Pub/Go caches, verifies host lower bytes and paths are unchanged,
checks small persistent upper deltas and respawn reuse, and requires spawn
under one second. A second sleeve must still see clean cached data after the
first poisons an existing Go output. A private owner Go build prewarms the
cache; unlinking an owner output after snapshot publication must still allow
the sleeve build without compiler invocations. The copy lane puts `/usr/bin/bwrap` first on
PATH, requires its lack of overlay options, and exercises the writable Dart
cache copies and respawn persistence. Fixtures use private synthetic Flutter
and Pub trees plus the installed Go toolchain; provider logins are not used.
Capability detection and unreferenced-output rejection run before the runtime
gate. `TestBinaryGoSnapshotLifecycle` uses a sparse source exceeding 128 GiB
to check bounded newest-first imports, duplicate outputs, stale entries,
size mismatch rejection, executable outputs, hardlinked outputs versus copied
indexes, owner rewrites/unlinks, nonblocking startup, refresh across restart,
unreferenced generation cleanup and explicit pruning. Its runtime subtest
keeps mounted old generations across refresh/prune, including an unreadable
mount reference. `TestBinaryGoSnapshotHardlinkFailure` uses a second filesystem
and requires a logged private-cache fallback with no retained or copied data.
The metadata-heavy sparse fixture has 101,029 indexes and more than 128 GiB
of output lengths. On this two-core sleeve, background preparation took
5.334s while other checks ran; 100,009 imported indexes retained 957,025,679
logical bytes under a 1 GiB cap. This measures metadata work without reading
output data, not cold storage performance on the owner's actual 111 GB cache.
The runtime also refuses host cache workspaces and cache-directory symlinks
that would escape the private sleeve home during a respawn.

A sleeve spawned with `--nested --cpu 200 --mem 5G` runs these runtime lanes
through private delegated cgroups and nested bwrap. A normal sleeve still
skips them. `TestBinaryNestedRuntime` starts a real nested-enabled sleeve and
runs the selected production binary's runtime scenarios inside it, failing if
the inner run skips. It also verifies actual memory OOM, CPU throttling, hidden
parent state, persisted opt-in, unchanged outer limits and cgroup cleanup.
Private test homes retain the selected bwrap binary, and the nested scenario
asserts its version so a newer user-installed bwrap reaches the inner runtime.
The nested shell/protocol fixture explicitly mounts the installed Go toolchain
and its private executable peers. It excludes unused Flutter caches: adding a
third overlay over this sleeve's already overlaid SDK cache fails with
`EINVAL` on the current host. General cache overlay depth fallback remains a
runtime limitation; direct cache/toolchain scenarios still exercise the actual
SDK overlays. The inner runner keeps temporary files under its private home
on disk, avoiding the nested sandbox's memory-backed `/tmp`.
The scenario passed in a nested-enabled development sleeve in 15.39 seconds
with no inner skips; see [the runtime setup](sleeves.md#running-runtime-tests-inside-a-sleeve).

The nested runtime fixture seeds private synthetic SDK/cache lower trees to
avoid adding a third overlay above the development sleeve's existing caches.
The selected real bwrap, production binaries, namespace and cgroup checks stay
unchanged, including actual OOM, CPU throttling, hidden limits and cleanup.
Its inner run rejects skips; no manual SDK copy or production fallback is used.

All surviving session-manager tests set `TMUX_TMPDIR` to a test temp directory
and kill their private server in cleanup. The binary harness does likewise.
Socket directories use a short private `/tmp` path independently of disk-backed
`TMPDIR`, keeping Unix socket names below their roughly 108-byte limit. Only
the tiny sockets use `/tmp`; fixture homes and SDK copies remain on disk.
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
PROT_E2E=1 PROT_TAILNET_IT=1 go test -count=1 -timeout 15m -tags "$(make -s build-tags)" ./internal/tailnet
```

`PROT_TAILNET_IT=1` is an additional network-access opt-in: these scenarios
download the pinned Headscale binary and start private tailnet/DERP processes.
It does not replace `PROT_E2E`.

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
exited chat sleeve verifies branch refresh in `sleeve ls` and `sleeve show`, in
text and JSON, plus model/effort columns, read-only mounts, idle timeout and
persisted chat details. It deliberately inherits invalid sleeve credentials
to verify that the harness isolates them without needing the host user manager.

`TestBinaryNoteLifecycle` drives a real hub, CLI and stdio MCP bridge through
loading an existing note without a state, creating and closing with a
resolution, open-only defaults, done/all/project/query filters, invalid
updates, idempotent close, reopen and hub restarts in both states. It verifies
read-only denial and a sleeve identity with the production backlog scopes
closing a note with the same actor recorded in audit, then lists open notes
and closes an existing note through MCP. It needs no host user manager.

`TestBinarySleeveReports` uses persisted chat sleeve records and identity tokens
minted by the temporary hub, so no AI CLI or user manager is needed. It checks
own-sleeve reporting, cross-sleeve and owner/scoped forgery denial, status/size
validation, timeout and exit/kill failures, immediate idle waits, concurrent CLI
waits printing each summary as it lands, owner report and kill events, replacement
history, retained exited chat/terminal result replay without restoring chat state,
read-scoped waits, cross-sleeve wait denial, MCP wait-for-report and
restart persistence. The host-only binary chat subscenario also checks that
pending mail drains before idle, mail leaves a result intact, a retried brief
preserves it and a new brief clears it.

`TestBinarySleeveDiskPrune` boots the real hub with killed, leftover and running
records and real clones backed by a local bare origin and a local checkout.
It verifies home/cache/rest accounting, text and JSON dry runs, cache-only
cleanup, whole-home cleanup, owner-only cache dropping and audit records.
Clean published clones are removed; unpublished branches, staged work,
untracked files and edits hidden by assume-unchanged are kept. Running homes,
owner workspaces and auth/session/config files survive cache cleanup. Overlay
upper/work bytes count as cache usage, with Go overlay bytes included in the
Go total; prune and kill remove sleeve overlays while preserving host lower
directories, even when linked from an overlay upper directory. Clone
and origin fsmonitor/filter/upload-pack programs must never execute.

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

## Coverage of the real binary

Run `make e2e-cover` to build the tagged binary with Go statement
instrumentation and run `tests/e2e` against it. Each run keeps a fresh
`bin/e2e-cover/run.*/` directory containing the binary, raw `data/`,
`tests.log`, per-package `packages.txt`, `coverage.out` and
`functions.txt`. It prints per-package percentages and exits unsuccessfully
when the scenarios fail. Profiles from different runs are never merged.

The harness forwards `GOCOVERDIR` only when set and reaps processes before
canceling their private deadline contexts, so graceful hub/node exits write
their counters. It polls nodes every second: the previous 100 ms interval
implied a 200 ms stale window and rejected healthy nodes under instrumentation.
Forced kills cannot write final counters. These profiles measure binary
execution, excluding fuzz campaigns and the in-package host/driver scenarios.

| Area | Behavioral scenarios | Remaining known gaps |
|---|---|---|
| Authorization | `TestBinaryStreamAuthorizationAndRevocation`, `TestBinaryNodeCredentialBoundaries`, Cluster security | Terminal host/readonly input distinctions |
| Jobs and cancellation | `TestBinaryJobCancellationAndIdempotency`, `TestBinaryArtifactJobAndReplay`, Cluster restart/git | CLI signal during submit, lost-node deadline, execution contention |
| Destructive node operations | `TestBinaryNodeDrainAndRemoval` | Actual uninstall and tailnet revocation |
| Persistence and migration | `TestBinaryStateMigrationAndPolicyRestart`, Cluster backup/replay | Interrupted atomic writes, journal truncation |
| Transfer integrity | `TestBinaryTransferFailureAtomicity`, `FuzzTransferArchive` | Transfer expiry and ZIP parser fuzzing |
| MCP parsing | `TestBinaryCLIAndMCP`, `FuzzMCPRequest`, `FuzzMCPArguments` | Most per-tool effects are asserted through RPC |
| Generated services | `TestBinaryGeneratedServiceExecution` | Target OS boot/registration/uninstall |
| Mail/chat/runtime | Existing host HTTP and official protocol scenarios | Real sleeves/two-harness handoff skip here |
| Enrollment/desktop/UI | Optional tailnet/runtime lanes | Physical desktop, real providers, Flutter real-hub flows |

[The complete coverage map](e2e-coverage-map.md) accounts for every RPC,
CLI command, all 55 MCP tools, persisted files and the risky deleted-test
behaviors. Help/catalog enumeration and authorization-only probes are marked
separately from successful operations. Features owned by the wave-4 sleeves
remain excluded.

The three new fuzz targets run seeds in `make test`. Mutation campaigns:

```sh
go test ./internal/mcpbridge -run '^$' -fuzz '^FuzzMCPRequest$' -fuzztime=10s -parallel=2
go test ./internal/mcpbridge -run '^$' -fuzz '^FuzzMCPArguments$' -fuzztime=10s -parallel=2
go test ./internal/xfer -run '^$' -fuzz '^FuzzTransferArchive$' -fuzztime=10s -parallel=2
```

Measured on 2026-10-21 in the two-core sleeve, using the same production
source at `38c346d` and the corrected harness for both runs. Before runs the
three pre-existing binary scenarios; after includes eight new scenarios.
All scenarios passed (40.474s before, 34.729s after; these are coverage
snapshots, not comparable performance benchmarks).
Host runtime subscenarios skipped; the optional tailnet lane was not run.
Fuzz coverage is separate.

| Package (module prefix omitted) | Before | After |
|---|---:|---:|
| cmd/protectorate | 33.5% | 36.7% |
| gen/protectorate/v1 | 24.5% | 26.0% |
| gen/protectorate/v1/protectoratev1connect | 55.4% | 58.1% |
| internal/auth | 66.3% | 67.2% |
| internal/backlog | 0.4% | 0.4% |
| internal/backup | 59.5% | 59.5% |
| internal/chatlog | 0.0% | 0.0% |
| internal/codec | 66.7% | 66.7% |
| internal/desk | 1.0% | 14.6% |
| internal/diagnostics | 100.0% | 100.0% |
| internal/directory | 6.8% | 6.8% |
| internal/driver | 0.1% | 0.1% |
| internal/fsutil | 65.0% | 65.0% |
| internal/handoff | 0.0% | 0.0% |
| internal/headscaledb | 50.0% | 50.0% |
| internal/hub | 40.0% | 45.6% |
| internal/keylog | 69.6% | 69.6% |
| internal/mailbox | 48.9% | 42.1% |
| internal/mcpbridge | 62.4% | 62.4% |
| internal/node | 46.6% | 51.2% |
| internal/orchestrator | 0.0% | 0.0% |
| internal/quota | 10.6% | 10.6% |
| internal/session | 47.5% | 47.5% |
| internal/sleeve | 23.9% | 23.9% |
| internal/svcgen | 0.0% | 12.3% |
| internal/tailnet | 1.1% | 1.1% |
| internal/xfer | 37.0% | 77.3% |

Cancellation timing affects some counters: the final MCP wait cancellation
reached the client before the hub's mailbox Wait method, explaining the lower
mailbox percentage. These are observed profiles, not coverage thresholds.

Function profiles confirmed previously unreached paths now execute:
node job cancellation 64.3%, ZIP extraction 69.6%, tier migration 78.4%,
node draining 78.6% and generated Termux run scripts 90.9%. Driver/chat/handoff
gaps need separate process or host lanes. Optional
service-manager enrollment and physical desktop flows remain unmeasured.

Mutation campaigns passed: MCP requests 34,164 executions in five seconds,
MCP arguments 31,759 in five seconds, transfer archives 19 in ten seconds.
The archive target performs real filesystem staging/sync/cleanup and is much
slower; its seven seed archives also run on every commit.

Regenerate the inventory with `python3 scripts/test-inventory.py --go-results
/tmp/protectorate-tests.json` after collecting tagged `go test -json ./...`
with the same PROT_E2E environment. The explicit KEEP list makes newly added
unreviewed tests appear as CONVERT rather than automatically accepting them.


## Inventory

Snapshot counts: KEEP-E2E=83, KEEP-FUZZ=8; total=91. TestMain rows are infrastructure, not scenarios.

Runtime is the observed non-race test-function elapsed time, including skips, unless explicitly labelled race. It excludes build time and is not a race-suite prediction. Unknown means unmeasured, never zero. Dart callback labels include source lines to distinguish repeated labels.

| File | Function / callback | Exercises | Runtime | Verdict | Replacement / reason |
|---|---|---|---|---|---|
| tests/e2e/node_artifact_security_test.go | TestBinaryJobArtifactsStayInWorkspace | Real node job artifacts: explicit and wildcard symlink-parent escapes refused; regular output retained | Unknown | KEEP-E2E | Prevent host-file disclosure during artifact collection |
| tests/e2e/node_git_security_test.go | TestBinaryNodeRepositoryProgramsStayIsolated | Warm node checkout rejects job-written hooks; repository filters cannot read host credentials when sandboxing is available | Unknown | KEEP-E2E | Real binary jobs; filter authority subscenario needs bwrap and the user manager |
| tests/e2e/release_update_test.go | TestBinaryGitHubReleaseUpdates | Release channels, checksum refusal, hub restart arguments and failed-health rollback, release-backed web/downloads and node update, two-release retention | 75.20s pass | KEEP-E2E | Private fake GitHub server and real hub/node binaries; includes candidate build under concurrent cross-compilation |
| tests/e2e/install_release_test.go | TestHubInstallerVerifiesFleetBeforeInstalling | Shell installer verifies stable/dev fleet assets and versioned layout, refuses corruption before installation, dry run leaves installation absent | 4.38s pass | KEEP-E2E | Private loopback asset server and real production binary |
| tests/e2e/mcp_sessions_test.go | TestBinaryMCPRejectedInitializeKeepsSession | Rejected HTTP initialization leaves existing hub tools usable | Unknown | KEEP-E2E | Real bridge and hub; no canceled session left in service |
| tests/e2e/mcp_session_limits_test.go | TestBinaryMCPSessionsBoundedAndReusable | Session admission is bounded; existing tools work at capacity; DELETE releases capacity | Unknown | KEEP-E2E | Real HTTP bridge lifecycle under session churn |
| tests/e2e/mcp_pending_limits_test.go | TestBinaryMCPPendingCallsBoundedAndCancelable | Pending hub waits are bounded; cancellation and subsequent calls keep working | Unknown | KEEP-E2E | Real stdio bridge and hub mailbox under load |
| tests/e2e/app_connection_test.go | TestBinaryAppHubConnection | Production Dart hub probe against real binary: valid/invalid tokens, refused connection destination, missing-port guidance and stalled-server timeout | Unknown | KEEP-E2E | Requires Dart and resolved app dependencies (make app-check); no widgets |
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
| internal/driver/selection_test.go | TestCodexSelectionAtOpenAndResume | Native selection and unsupported hub-effort fallback on open/resume, confirmed by a real protocol-peer turn | unmeasured | KEEP-E2E | preserves requested effort while using native fallback |
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
| tests/e2e/chat_test.go | TestMain | Native-protocol executable peer for the binary runtime lane | unmeasured | KEEP-E2E | runner infrastructure; no installed Codex or provider login |
| tests/e2e/workflow_test.go | TestBinaryClusterRestartAndRestore | Binary cluster lifecycle, host runtime and Codex default/explicit/fallback effort, restart, resleeve and Envoy | unmeasured | KEEP-E2E | effort runtime lane uses a deterministic native-protocol peer, no provider login |
| tests/e2e/workspace_browser_test.go | TestBinaryWorkspaceBrowser | Real hub/CLI workspace discovery, confinement, identity denial, live branch refresh and complete sleeve details | unmeasured | KEEP-E2E | inherited credential isolation, model/effort columns, mounts and idle timeout |
| tests/e2e/backlog_test.go | TestBinaryNoteLifecycle | Real hub/CLI/MCP note completion, filters, reopen, persistence and sleeve authorization | unmeasured | KEEP-E2E | note lifecycle regression scenario |
| tests/e2e/cache_test.go | TestBinarySleeveCaches | Real hub/shell overlay and bwrap 0.8 fallback, lower integrity, delta size, warm Go reuse and respawn | host runtime required | KEEP-E2E | cache isolation and spawn regression scenario |
| tests/e2e/go_snapshot_test.go | TestBinaryGoSnapshotLifecycle | Real hub with a sparse 128 GiB owner cache, bounded hardlink imports, copied indexes, refresh/prune and mounted generation retention | unmeasured | KEEP-E2E | startup cost, lifetime and cache isolation regression scenario |
| tests/e2e/go_snapshot_test.go | TestBinaryGoSnapshotHardlinkFailure | Real hub with a cache on another filesystem refuses sharing and never copies outputs | unmeasured | KEEP-E2E | prevents expensive cross-filesystem copy fallback |
| tests/e2e/dispatch_test.go | TestBinaryBranchDispatch | Real CLI branch dispatch, atomic brief replay, credential-stripped remotes, jailed fetch and source write denial, packed refs, coalesced notices, restart dedupe and own-advance suppression | host runtime required | KEEP-E2E | deterministic Claude protocol peer; requires systemd user manager and bwrap |
| tests/e2e/dispatch_test.go | TestBinaryBaseNotices | Real hub with persisted worker, packed base refs, coalesced mail, restart dedupe, own-advance suppression and repository-program confinement | unmeasured | KEEP-E2E | exercises polling and mailbox without the host user manager |
| tests/e2e/report_test.go | TestBinarySleeveReports | Real hub/CLI self-report authorization, multi-sleeve event waits, history and restart persistence | unmeasured | KEEP-E2E | marked completion regression scenario |
| tests/e2e/sleeve_disk_test.go | TestBinarySleeveDiskPrune | Real hub/CLI disk accounting, safe Git prune, cache/overlay cleanup, host-lower and running-data protection, owner audit | unmeasured | KEEP-E2E | disk cleanup regression scenario |
| tests/e2e/nested_runtime_test.go | TestBinaryNestedRuntime | Real nested sleeve runs the selected production binary's isolation, ownership, restart, chat, OOM and CPU throttling scenarios; verifies hidden ancestor limits and subtree cleanup | 10.81s pass in delegated sleeve | KEEP-E2E | requires host runtime or a nested-enabled sleeve; fails on inner skips |
| tests/e2e/race_reports_test.go | TestBinaryRaceReports | Real detector reports fail otherwise passing subprocess scenarios, including successful node artifact jobs and retained terminal panes | 6.81s race; skips non-race | KEEP-E2E | prevents silent loss of subprocess race reports |
| tests/e2e/cli_mcp_test.go | TestBinaryCLIAndMCP | Binary CLI parsing, official MCP transports, filtering, authorization and cancellation | unmeasured | KEEP-E2E | existing scenario previously missing from this inventory |
| tests/e2e/artifact_job_test.go | TestBinaryArtifactJobAndReplay | Directory/file artifact execution, workdir confinement, binary output, executable modes, env redaction and restart replay | unmeasured | KEEP-E2E | replaces deleted artifact-source job coverage |
| tests/e2e/job_cancellation_test.go | TestBinaryJobCancellationAndIdempotency | Concurrent submits, persisted pre-submit cancel, failed retry, process-tree kill, timeout and capacity recovery | unmeasured | KEEP-E2E | replaces deleted job/request-key cancellation coverage |
| tests/e2e/node_credentials_test.go | TestBinaryNodeCredentialBoundaries | Redirect credential confinement, care file refusal and explicitly granted read | unmeasured | KEEP-E2E | replaces deleted node redirect and care transfer coverage |
| tests/e2e/node_removal_test.go | TestBinaryNodeDrainAndRemoval | Refused/aborted drain, placement exclusion, cancellation, held/queued lease cleanup and persistent removal | unmeasured | KEEP-E2E | destructive node lifecycle |
| tests/e2e/service_execution_test.go | TestBinaryGeneratedServiceExecution | Generate/refuse overwrite/execute/restart a real node service with literal quoted paths | unmeasured | KEEP-E2E | replaces text-only service generator assertions with execution |
| tests/e2e/state_migration_test.go | TestBinaryStateMigrationAndPolicyRestart | Tier migration, backup protection, desktop policy safety/persistence/clear and lease expiry | unmeasured | KEEP-E2E | replaces deleted migration and policy persistence coverage |
| tests/e2e/stream_authorization_test.go | TestBinaryStreamAuthorizationAndRevocation | Real event delivery, every hub stream denial, every node bearer gate and revoked restart | unmeasured | KEEP-E2E | closes streaming methods omitted by unary authorization matrix |
| tests/e2e/transfer_integrity_test.go | TestBinaryTransferFailureAtomicity | Digest/size/framing refusal, atomic overwrite, malicious archives, cleanup, directory modes and recovery | unmeasured | KEEP-E2E | replaces deleted transfer protocol/integrity coverage |
| internal/mcpbridge/transport_fuzz_test.go | FuzzMCPRequest | Untrusted JSON-RPC envelope and request identity | unmeasured | KEEP-FUZZ | seed corpus on commits; bounded campaigns separately |
| internal/mcpbridge/transport_fuzz_test.go | FuzzMCPArguments | Strict protobuf JSON arguments for jobs, transfers and leases | unmeasured | KEEP-FUZZ | seed corpus on commits; bounded campaigns separately |
| internal/xfer/archive_fuzz_test.go | FuzzTransferArchive | Tar extraction confinement, failure atomicity and staging cleanup | unmeasured | KEEP-FUZZ | seed corpus on commits; bounded campaigns separately |
| tests/e2e/cache_confinement_test.go | TestBinarySleeveCacheConfinement | Production bwrap cache seeding, cancellation cleanup and sleeve-written Flutter/Pub/parent symlinks cannot expose the private hub token on relaunch | 1.530s pass | KEEP-E2E | exercises filesystem isolation without a systemd user manager |
| tests/e2e/backup_manifest_test.go | TestBinaryBackupManifestCoverage | Real-binary restore rejects duplicate manifest entries hiding altered state and changed file permissions before creating the destination | 1.430s pass | KEEP-E2E | complete manifest verification regression |
| tests/e2e/quota_cleanup_test.go | TestBinaryQuotaReapsClosedStdoutPeer | Hub polls a synthetic Codex peer that closes stdout and ignores TERM, publishes quota promptly and reaps the process | 4.350s pass | KEEP-E2E | process lifetime, rather than stdout EOF, bounds cleanup |
| tests/e2e/workspace_metadata_test.go | TestBinaryWorkspaceMetadataIsBounded | Real hub list/browser reads refuse FIFO, oversized and escaping git metadata without hanging or leaking file contents | 1.270s pass | KEEP-E2E | hostile workspace metadata regression |
| tests/e2e/device_auth_test.go | TestBinaryDeviceAuthorization | Real hub, CLI and streams exercise approval, denial, expiry, migration, metadata, persistence, single secret delivery, mailbox access and revocation | pass | KEEP-E2E | device trust lifecycle and secret redaction |
| tests/e2e/device_auth_test.go | TestBinaryDeviceAuthorizationLimits | Real TCP source IPs exercise start/poll budgets, malformed requests and pending capacity | pass | KEEP-E2E | unauthenticated endpoint abuse limits |
| tests/e2e/phone_emergency_test.go | TestBinaryPhoneEmergencyStop | Scoped phone stops real worker process trees and revokes their tokens, preserves Envoy streaming, repeats with Envoy unavailable | 1.78s pass | KEEP-E2E | direct hub emergency action independent of Envoy |
| tests/e2e/terminal_launcher_test.go | TestBinaryTerminalLauncher | Large literal terminal argv and private environment survive launch, hub restart and supported sleeve respawn | 4.79s pass | KEEP-E2E | catches tmux message-size failure and shell argument injection |

## Deleted files

See [the phase 2 deletion record](testing-deletions.md) for the exact paths grouped by package.
