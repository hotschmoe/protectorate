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
Notes exercise create/read/update/delete, optional false/empty updates, pinned
ordering, project/query filtering and exact records after hub restart and
backup restore. Backup checks restore saved jobs/tokens/notes and refuse corrupt, truncated,
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
