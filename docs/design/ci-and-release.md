# Background gates, releases and a hub-host node

Proposal for owner review, 2026-11-18. No behavior here is implemented or authorized by this
file. SPEC.md wins until amended. This proposes changes to SPEC sections 2, 6, 7 and 12b:
section 7's `make test` cross-arch rule becomes a declared background gate; section 6's local
release and D36's unsigned release change to the release protocol below. Section 12b's two-node
matching rule remains for Rust binaries; the client exception below needs a SPEC amendment.
Engineering.md supersedes repo testing rules:
at most ten blocking end-to-end tests per project, with other checks in the major gate. Migration
of existing tests and commands is separate work, not performed by this proposal.

## Phases

Each phase ships independently after the needed SPEC amendments; later phases are not prerequisites.
The boundary against executing repository code with hub secrets is hard from the start.

1. **Background gate and culprit finder.** Ship the declaration, fleet scheduling, journaled results,
   bounded investigation, cancellation and reviewable fix branch without release credentials. Defer
   automated merges and releases to later phases.
2. **Merge to dev.** Add fixed hub-side Git, fleet rerun of the blocking set and compare-and-swap
   update of `dev-hotschmoe`. Keep release preparation and publication manual until phase 3.
3. **Release.** Add prepare, stable approval, fixed signing and publication, fleet builds, and the
   never-execute-artifacts rule. The first release may use the existing rollback-sensitive journal
   audit and an isolated fixed APK signer; separately retained append-only audit and a hardened APK
   signer/validator follow as hardening. Until then show the audit rollback limitation and isolate
   the signer with only the APK key, no hub state or network. Neither deferral permits repo-supplied
   signer code, arbitrary signing requests or executing artifacts on the hub.
4. **Hub-host node.** Ship only after the separate identities, fail-closed isolation, explicit device
   grants and negative access checks below work. Further device types can follow independently.

## Gate declaration and scheduling

Each owner-registered repository has a root `protectorate-ci.json` at the tested commit, parsed
with `pt_json`. Example:

```json
{
  "version": 1,
  "branch": "dev-hotschmoe",
  "every_commits": 15,
  "nightly_utc": "18:00",
  "blocking": [
    {"id": "main-path", "argv": ["make", "test-main"], "timeout_secs": 180}
  ],
  "heavy": [
    {"id": "linux-e2e", "argv": ["make", "test-e2e"],
     "node": "linux-amd64", "timeout_secs": 3600},
    {"id": "fuzz-seeds", "argv": ["sh", "scripts/replay-seeds.sh"],
     "node": "linux-amd64", "timeout_secs": 3600}
  ]
}
```

IDs are unique, stable test selectors; each entry must run one independently rerunnable scenario
or a suite that reports individually selectable failed IDs. Version 1 treats each entry as one
scenario, so a suite must be split into entries to narrow a failure. At most ten blocking entries,
each one real-binary e2e, are allowed; the owner and agents run all before committing. A command
can include its build. Heavy entries may include fuzz replay and long tests. The hub validates
schema, limits, ID uniqueness, selector availability and node class; invalid or missing declaration
is visible as configuration error, never green. Commands run with repository root as cwd and no
interpolation by the hub. A repository controls its own code and declaration, not credentials,
node enrollment, allocation limits, or release policy. The hub registry maps repo ID to an
owner-configured local source/mirror and allowed node classes; it does not discover arbitrary repos.

One journaled cursor per (repo, branch) counts first-parent commits since the last scheduled
snapshot, not commits pushed or jobs completed. On reaching N (10–20 inclusive), schedule the
current observed tip once, then reset the cursor to that tip; a nightly tick also schedules the
current tip if it differs from the last scheduled tip. Initial registration schedules the current tip
and establishes the cursor. Poll local registered refs and accept explicit refresh, including local
commits; no GitHub webhook is required. Coalesce advances during a run into one pending latest tip;
recalculate from the scheduled tip on completion. Runs are keyed by (repo, branch, full commit,
declaration digest); repeated observations and nightly ticks do not duplicate them. Different
branches have independent cursors, but only registered branch refs are watched. If a ref rewinds or
force-pushes so the cursor is not its first-parent ancestor, journal a new epoch and schedule its
new tip; never count abandoned history or cancel an already running immutable snapshot. Missing
objects, deleted refs and invalid config produce a visible hold, not a pass. Store source objects by
full hash until runs and investigations finish. Immutable checkouts and config come from that hash.

The hub implements scheduling and state as part of the existing single-writer core, not a separate
binary: it already owns the journal, node jobs, capability checks and sleeve lifecycle. This avoids a
second scheduler and reconciliation protocol while reusing the node protocol. Heavy commands execute
only in fleet jobs, never on the hub host, including when that host is registered as a node; the
hub-host node is reserved for explicit device jobs, not gates or release builds. The scheduler
uses existing node presence, resource limits and fencing, with one active gate per repo/branch and
one pending tip; unavailable nodes leave work queued without blocking commits. Cap defaults at two
concurrent heavy jobs globally. Jobs do not receive hub secrets. Node results are claims, with node,
commit, declaration digest, command, timestamps, exit status and bounded log/artifact references.

Protectorate's heavy e2e tests require bwrap, unprivileged user namespaces and a systemd user
manager. Today's node system service and its jobs run as root under the system manager. For a
configured test node, the owner provisions a dedicated non-root test account with lingering enabled,
a private work directory and a running user manager. The root node service starts the gate job with
fixed, owner-configured credentials and environment for that account, including its
`XDG_RUNTIME_DIR` and user bus; it verifies the account, manager, bwrap and namespace prerequisites
before dispatch. The job runs as that account in its user manager's scope, never as root. Missing
prerequisites hold the job visibly, without a root or process-group fallback. The account has no
hub credentials; the node remains untrusted and results are node claims. Other projects may use
other explicitly provisioned node profiles.

`GateRegister`, `GateRun`, `GateList`, `GateShow`, `GateCancel`, `GatePause` and `GateResume`
are proposed API methods, exposed as `protectorate gate register|run|list|show|cancel|pause|resume`
and app controls. Owner configures registration; Envoy can run and inspect; owner and Envoy may
cancel or pause. Cancellation stops queued and active jobs and associated investigation; it is
idempotent and records incomplete outcomes. Journal `GateScheduled`, `GateStarted`,
`GateTestFinished`, `GateFinished`, `GatePaused`, `GateCancelled`, and `GateEpochChanged`
before effects as appropriate. `event list` links the run, commit and each failing test; gate show
and the app show queued/running/green/failed/inconclusive/cancelled, progress, logs, node claims,
last green and the investigation. Replay reconstructs pending work; an uncertain dispatch is
reconciled with existing job fencing, never blindly rerun. A gate is green only if every required
heavy entry succeeds; a cancellation, timeout, lost node or missing test is inconclusive.

### Finding and fixing a culprit

On a reproducible heavy test failure, the core starts one culprit-finder child sleeve per
(repo, branch, epoch, failed test ID, bad commit), linked in `sleeve show` and the app. The brief
names the immutable failure, exact test ID, node class, logs, declaration digest, branch tip, last
comparable green and budgets. The sleeve requests fleet jobs through the hub; it never runs the
heavy command on the hub. First rerun the failure at the bad revision twice; mixed outcomes mean
flaky/inconclusive. Then test the first and second first-parent predecessors, followed by a binary
search between a confirmed good and bad revision. At every candidate use that commit's declaration
and the same test ID and node class. A missing or materially changed selector, build failure before
the test, infrastructure error or timeout is untestable, not evidence of a regression. Skip it and
report an ambiguous range if no unique boundary remains. Repeat candidate outcomes twice; mixed
results terminate as flaky. This is a first-parent integration culprit, not proof of root cause or
of monotonic behavior. Multiple failures get independent investigations, deduplicated by the key.

Default limits: 50 first-parent commits of history, 20 candidate revisions, two fleet nodes at a
time, four hours wall time, and two hours per job; earlier good must be within that range. If no
confirmed good, resources, budget or consistent outcome, stop inconclusive with evidence. A sleeve
may commit a proposed fix on its own branch based on the current working tip, run the blocking set
and the failing heavy test, and report the branch, base and evidence for review. It cannot declare
itself green or directly merge; after phase 2 Envoy may request the separate merge protocol. A changed branch tip
requires revalidation. Journal `InvestigationStarted`, `InvestigationSampled` (revision, attempts,
node, result), `CulpritIdentified` or `InvestigationInconclusive`, `FixProposed`, and
`InvestigationStopped`. The owner sees the evidence and stop control on the gate, event list,
`sleeve show`, and app; cancellation kills jobs and stops the sleeve. A restart resumes only after
reconciling jobs and the sleeve; no second investigation is spawned for the same key.

[git bisect run](https://git-scm.com/docs/git-bisect) supplies good/bad search and an explicit
untestable outcome; we add persistent scheduling and evidence. Chromium
[Findit](https://sites.google.com/chromium.org/cat/findit) reruns failed tests and searches the
regression range; we take targeted reruns and flake awareness, with much smaller budgets. LLVM's
[buildbots](https://llvm.org/docs/DeveloperPolicy.html#working-with-the-ci-system) provide varied
post-merge worker coverage; we take named configurations and visible worker results, not external
CI or a requirement to build every commit.

## Merge and release authority

Envoy receives narrow hub API capabilities, never a GitHub token, signing key, owner socket or
restart authority in its sandbox. Proposed `MergeRequest(repo, source_sha, expected_dev_sha)`,
`ReleasePrepare(repo, dev_sha, channel, version)`, `ReleaseApprove(id, manifest_digest)`,
`ReleasePublish(id)` and `ReleaseShow/Cancel(id)` map to CLI and app actions. Only the owner
can approve stable releases and configure repositories, token scope, signing keys or policy; Envoy
may request the rest. Existing D9 origin and maintenance approvals remain in force, including for
automated jobs and protected pushes; requests retain origin across restarts and spawned sleeves.
Other sleeves cannot call release or merge methods. The hub accepts only registered repos and fixed
branch names, validates full hashes and version syntax, rejects dirty/ambiguous inputs and uses
compare-and-swap ref updates. No arbitrary git arguments, executable paths, shell, hooks, git config,
credential helper, or repo-supplied release instructions run on the hub. Fixed hub-owned Git and
HTTP operations use a clean environment and disabled hooks/config; the scoped GitHub App/token can
access only configured repositories and necessary contents, pull-request and release permissions.
Branch protections remain effective. Audit every authorization and external write before effect.

Before a merge, the hub dispatches the entire declared blocking set as fleet jobs against the exact
source commit and its declared configuration, recording the node, test results and expected dev
SHA. All must pass; a missing test, failed job or unavailable node leaves the request unmerged.
The hub constructs and tests the exact proposed merge tree against that expected dev SHA as well;
it updates dev only by compare-and-swap if that SHA still matches. Conflicts, a moved ref, missing
proof or a rejected push stop for a new request. Results are node claims, not proof against a
compromised node. For a release, create a PR from the exact dev
SHA to master, record its number and head/base, verify required GitHub checks and merge through the
GitHub API with a head SHA precondition. If base/head changes, approvals or gates are stale: stop and
reprepare. Record the resulting master commit, fast-forward dev only if it still matches, and create
an immutable annotated tag on that master commit. Never force-push or move an existing tag. A tag or
PR collision requires reconciliation, not automatic overwrite.

A green release gate means all declared blocking and heavy entries on the exact candidate commit,
plus every configured release architecture, passed as recorded fleet jobs; no stale success, skipped
check, flake or infrastructure failure qualifies. The major gate still never blocks ordinary work:
release publication is a distinct action. Dev prereleases may proceed automatically after this green
gate; stable publication requires explicit owner approval of the exact master commit, tag, manifest
and artifact hashes after builds. D9 approvals still apply. Signing-key rotation, token scope changes,
new trusted nodes and policy changes are owner-only; failures and mismatches stop without fallback.

Build Rust binaries in secret-free fleet jobs from the immutable tagged commit on two distinct
nodes; require byte-identical unsigned outputs and record both node identities and digests. Independent
builds reduce but cannot eliminate collusion or a shared compromised toolchain; hashes prove byte
identity, not trustworthy source-to-binary correspondence. Reject mismatched Rust outputs.

OWNER: Permit client assets, including the Flutter APK, Windows zips and arm64 desktop build, to
come from a single node until reproducible across nodes. Record its identity and digest and mark each
such manifest entry `single-builder`; this amends SPEC 12b. Reproducible client assets should move
to the two-node rule.

Nodes get no signing credentials. The hub fetches
bounded regular files through the job artifact protocol, treats them as hostile bytes, rejects paths,
links, duplicate names, unexpected sizes/types and digest mismatches, and hashes the received bytes
itself. Fixed hub code constructs the allowlisted manifest and SHA256SUMS. It never executes,
unpacks, sources or dynamically loads a fetched artifact or repository script. Signing uses a
hub-installed, independently managed fixed signer with a narrow interface and secret accessible only
to that signer; Android APK signing initially uses the isolated fixed signer and existing Android
key. A hardened fixed signer/validator for hostile APK input follows the first release. No
repository-controlled signer, flags, provider, hooks or config. Sign the manifest and APK as required, hash final signed outputs, and publish exactly those
bytes with the scoped token. Clients must verify the manifest signature against a separately pinned
public key before accepting hashes; HTTPS-only sums are insufficient. Bootstrap and key rotation
must be designed before deployment. No signing of arbitrary blobs via the API.

Journal `MergeRequested/Completed/Failed`, `ReleasePrepared`, `ReleaseGateRecorded`,
`ReleaseBuildRecorded`, `ReleaseApprovalRequested/Granted`, `ReleaseSigned`,
`ReleasePublishStarted/Completed/Failed`, and `ReleaseUpdateStarted/Completed` with caller, origin,
refs, PR, tag, node IDs, hashes and effect IDs, never secrets. Fsync before external effects;
reconcile GitHub ref, PR, tag and asset state after a lost reply, marking uncertain on conflict rather
than retrying a non-idempotent action. Publish via draft, verify every uploaded byte/hash, then make
visible; partial draft and cancelled signed releases remain auditable, never overwrite published
assets. A failed merge/build/sign/publish or revoked approval stops the workflow and surfaces the
recovery action. Release state is serialized per repo. Cancelling cannot undo an already merged PR,
tag or publication. Once published, request the existing hub and node update paths and their
probation/rollback, with protocol compatibility ordering; failed canary stops its rollout group.
Rollback restores service versions/state under the existing documented limits, not Git history or an
immutable release. Journal snapshot rollback can lose recent release audit events. The first release uses the existing journal and exposes that limitation;
subsequent hardening adds an independently retained append-only hub-host audit log outside rollback
snapshots, with reconciliation on restart and fail-closed writes.

OWNER: Approve the proposed signing trust root, key custody and rotation, scoped GitHub permissions,
and release asset allowlist before implementing publication. Decide which repos, branches and release
architectures to register. Review the deferred APK signer and audit hardening. A green release gate
is mandatory in this proposal even for a prerelease; approve or change that policy.

## Node on the hub host

Today Linux `install-service` installs a system service with no `User=`, so it runs as root. Its
jobs inherit the agent identity; systemd scopes or process groups provide lifecycle and resource
control, not a filesystem or credential sandbox, and fallback to process groups exists. That service
must not be installed unchanged on the hub host.

Install a distinct `protectorate-local-node` system account with its own private state and home,
no login shell, no hub group, sudo, privileged capabilities or owner credentials. Run its node
service as that account and jobs as a separate unprivileged identity, with `NoNewPrivileges`, dropped capabilities, private devices
and a restricted filesystem namespace. Bind only a dedicated job work directory and explicitly
configured devices; no owner home, hub state, hub API socket, token, signing key, SSH agent, service
manager socket, shared caches or inherited file descriptors/environment. Deny access both by mount
namespace and Unix ownership/mode/ACL, including traversal through alternate paths and `/proc`;
make hub state and credential parents private to their owners. The node's own journal/member keys
must also be inaccessible to jobs: use a separate unprivileged job identity and job mount namespace,
with the service's controlled launcher dropping to that identity. A privileged systemd service manager
may launch the fixed sandbox profile, but the node and jobs cannot access its management socket or
choose arbitrary service properties. Fail closed if the profile, identity, device isolation or scope
cannot be established; no process-group fallback. Limit process, memory, CPU, disk and runtime; kill
all descendants on cancellation/restart. This is a new hard boundary for this node, stronger than the
ordinary sleeve or existing node job isolation; it does not claim to protect against kernel or
malicious device-driver exploits.

An owner-installed udev rule assigns the FTDI device on MS-R1 #1 to a dedicated device group by
stable `/dev/serial/by-id/` identity. Only the job identity gains that group; no broad `dialout`,
`disk`, `video`, owner or hub group. Resolve and verify the expected character device and major/minor
at launch, bind just that node, and fail closed on disappearance or a changed identity; replug needs
a fresh job. Explicit owner configuration grants each additional device and group. Device I/O can
affect the attached hardware; exclusivity and availability are visible in node/job status. Never
pass arbitrary device paths from a job request. Node enrollment and device policy remain owner-only.

For updates on the same machine serialize both rollouts and drain jobs. Normally update hub first,
wait for hub probation confirmation, then update the local node and wait for its confirmation before
other nodes; if the node protocol requires node-first, update the local node from the old hub first,
confirm compatibility and probation, then update the hub. Never switch both concurrently; hold jobs
through the transition. If either rolls back or becomes unreachable, stop rollout and show both
versions and recovery action. Hub restarts must not implicitly restart or grant access to the node;
the independent service manager and existing probation paths own each service. Real-device and
negative access checks are required before enrolling this node.

OWNER: Confirm the MS-R1 #1 device identity and allowed operations, and approve the local-node
isolation profile and update ordering after an end-to-end security review.
