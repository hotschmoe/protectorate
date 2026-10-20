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
