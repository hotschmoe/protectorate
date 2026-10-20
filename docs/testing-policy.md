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
