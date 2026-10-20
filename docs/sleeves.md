# Sleeve development environments

Finish each brief inside its sleeve with
`protectorate report --status complete|partial|blocked --summary-file result.md`.
Coordinators can run `protectorate chat wait worker1 worker2 --until report
--timeout 30m` to receive each marked summary as it arrives. `sleeve show`,
`sleeve ls --json` and the app's REPORT button retain the current result across
mail turns and hub restarts. See [chat results and completion](chat.md#results-and-completion)
for status semantics, idle waits and the `chat_wait` MCP tool.

Start in an existing directory with `protectorate sleeve browse --json` to
list the hub's browser roots, then `protectorate sleeve browse /absolute/root`
to list its subdirectories. Pass the selected path to
`protectorate sleeve spawn worker --workspace /absolute/checkout --harness codex`.
The directory is edited in place. The app's NEW SLEEVE dialog offers the same
browser alongside known workspaces, a typed absolute path and a clone URL.

## Dispatching onto a branch

```sh
protectorate sleeve spawn worker --workspace /absolute/checkout \
  --branch dev-hotschmoe --new-branch feat/task --brief task.md --harness codex
```

Branch options create a fresh clone under the hub's workspaces directory;
the source checkout is never edited. `--branch` selects the starting branch.
Without it, `--new-branch` starts from a local checkout's current branch, or
a URL's default branch. A URL always clones; a local directory without either
branch option retains the edit-in-place behavior. Branch dispatch refuses an
existing destination rather than switching a workspace containing earlier work.
To respawn an exited sleeve, select its existing clone without branch flags;
its base record is retained. Killing a sleeve removes its record.

For a local source, `origin` uses its credential-stripped origin URL (and is
absent if the source has no origin). The `base` remote points at its Git
directory, available read-only inside the sandbox at the same host path.
Linked worktrees include their common Git directory. Source configs, fetch
metadata and reflogs are masked; the source working files are not exposed.
The CLI reads `--brief FILE` on the caller's machine and sends its text with
the spawn request. It becomes the first chat message before spawn succeeds;
delivery failure stops the sleeve. Briefs require chat mode. The app offers
the same branch fields and an optional brief text field.

The sleeve's base repository, branch and last observed tip survive restarts
and resleeves and appear in `sleeve show` and `sleeve ls --json`. The hub polls
loose and packed refs every second. After two seconds of quiet (at most ten
seconds for a continuous burst), an advance produces one mailbox notice with
up to ten commit subjects and `git fetch base && git rebase base/BRANCH`.
Advances already in the sleeve's own history do not notify it. Rewinds and
branch deletion do not generate notices. The hub reads history in a read-only
sandbox; it never runs source repository programs on the host.

Existing sleeves gain no base automatically. Restart the upgraded hub and
dispatch new sleeves with branch options; no persisted-state conversion is needed.

Configure roots with repeatable `hub serve --workspace-root DIR`. Without the
flag, the owner's `~/github` is the sole root when it exists; otherwise there
are no roots. Explicit roots replace that default. Roots must exist and are
resolved at startup. Browsing resolves symlinks and refuses paths outside the
roots, skips hidden entries and files, and returns at most 500 directories
(scanning at most 10,000 entries), with a truncation flag when a limit is hit.
Escaping symlinks are omitted. Each entry includes git details and the
workspace safety check's rejection reason. Rejected parents can still be
opened to reach acceptable children; they cannot be selected as workspaces.
Roots limit discovery, not the existing typed-path Spawn authority.

`sleeve ls`, `sleeve show` and the SLEEVES cards show the current workspace
branch, refreshed at read time. Git details are read directly from `.git`
files without executing git on the hub. Worktrees and non-git directories
have an empty branch; detached checkouts also have no branch name.

Discover available workspaces and reference directories with
`protectorate sleeve workspaces --json`. Entries with `readOnly: true` are
reference directories suitable for `--ro`:

```sh
protectorate sleeve spawn worker --workspace /absolute/checkout --harness codex \
  --ro /home/owner/github/protectorate/references \
  --ro /absolute/research:/references --cpu 200 --mem 5G --exit-after-idle 5m
```

Paths refer to the hub. The default sandbox destination is the host path.
Mounts must be directories, cannot expose credentials or hub state, and are
checked after resolving symlinks. A read-only mount can cover a workspace's
subdirectory. Reference discovery checks known workspaces' `references`
directories and every `hub serve --sleeve-reference-dir`, without walking the home.
Other safe absolute directories can be mounted explicitly.

The hub discovers Flutter from `PROTECTORATE_FLUTTER_SDK`, `flutter` on PATH,
or `~/development/flutter`, in that order. Set the variable in the hub service's
environment for another installation and restart the hub. Custom hub tool-dir
lists replace automatic discovery; include the SDK and Pub tool directories.
Install and prewarm the SDK and global code generators on the hub before
starting workers. Sleeves expose Flutter, Dart, and `~/.pub-cache/bin` on PATH.
The SDK is read-only. A Dart workspace (a `pubspec.yaml` at its root or one
directory down) also gets a sleeve-local copy of `bin/cache`, and of Pub's
hosted/git package caches and global package metadata when global Pub tools
are available. Copies use reflinks where supported; elsewhere the first spawn
of a sleeve copies about 2.5 GB, and later spawns of that sleeve reuse it.

`PUB_CACHE=$HOME/.cache/pub` and `GOMODCACHE=$HOME/.cache/go-mod` are writable
per sleeve. Go first tries the host's existing module download cache through a
file proxy, then the public proxy. New dependencies still require downloads,
which stay in the sleeve home. `make gen`, `make app-check`, and Go commands
use these paths; no SDK download into `/tmp` is needed.

Each sleeve seeds its own `$HOME/.gitconfig` with the hub owner's global git
name and email. Missing fields fall back to the literal settings in
`~/github/protectorate/.git/config`, with includes disabled. Configure the
owner's identity before spawning if neither source supplies it:

```sh
git config --global user.name 'Owner Name'
git config --global user.email 'owner@example.com'
```

Existing sleeve-local identity files are preserved. The hub never changes
global or workspace git config. Attribute assisted commits with an explicit
trailer, for example
`Co-Authored-By: Codex (gpt-6.1-sol) <noreply@openai.com>`.
Caller-provided `GIT_AUTHOR_*` and `GIT_COMMITTER_*` can override identity.

CPU limits also set build concurrency: `--cpu 200` defaults to two Go workers,
`GOFLAGS=-p=2`, and two Flutter test workers through `make app-check`. Fractional
cores round down, with a minimum of one; unlimited CPU defaults to one worker.
Override with repeatable `--env GOMAXPROCS=1`, `--env GOFLAGS=-p=1`, or
`--env FLUTTER_TEST_CONCURRENCY=1`. For direct Flutter/Dart test invocations,
pass `--concurrency="$FLUTTER_TEST_CONCURRENCY"`. These limits apply per command;
running several heavy builds at once still shares the sleeve's memory cap.

Idle removal applies only to chat sleeves and defaults off. After continuous
IDLE with no pending or queued mail for `--exit-after-idle`, the hub closes the
harness, kills its scope, revokes its identity and publishes the normal sleeve
exit event. Working turns, approvals, compaction, hub takeovers and pending
mail prevent removal. The home and workspace remain on disk. The timeout and
mounts survive hub restarts and resleeves; the idle clock restarts after a hub
restart. No persisted-state conversion is required for this release. Restart
the updated hub and respawn existing sleeves to opt into mounts or idle removal.

```sh
systemctl --user restart protectorate-hub
```

## Disk

`sleeve ls` includes HOME; `sleeve show` and sleeve cards report total home
bytes, regenerable cache bytes, the remaining bytes and Go build cache bytes.
Sizes sum regular-file lengths without following symlinks (not allocated
blocks; reflinks and sparse files can differ). One background sampler walks
workspaces and homes every five minutes, including exited/killed sleeves;
values are zero until the first sample. Killed records remain inspectable
until respawn. Old leftover homes without records appear as killed sleeves
with unknown harness/workspace. Unknown cache directories count as remaining
data, never as safe-to-drop caches.

The cache allowlist is `$HOME/.cache/go-build`, `.cache/go-mod`, `.cache/pub`,
`.cache/flutter` (the copied Flutter `bin/cache`) and `.cache/overlays`.
Overlay bytes include each cache's sleeve-local `upper` and `work` directories;
`.cache/overlays/go-build` also contributes to the Go build cache total. Host
lower directories are outside the allowlist and are never followed through
symlinks during accounting or cleanup. Harness auth, sessions,
history and config are preserved by cache cleanup. No harness cache directory
is currently declared safe to drop. Shared provider credentials are outside
these homes and are never candidates. Downloaded modules may be read-only;
prune handles those permissions. Cache directories are recreated or seeded
on the next spawn. Writable caches are private to each sleeve.

Prune inventories non-running homes and the hub's workspace namespace, prints
sizes and keep/remove reasons, and removes nothing without `--yes`. With no
category, only caches are selected. These are owner-only, audited RPCs:

```sh
protectorate sleeve prune
protectorate sleeve prune --caches --yes
protectorate sleeve prune --homes
protectorate sleeve prune --homes --yes
protectorate sleeve prune --workspaces
protectorate sleeve prune --workspaces --yes
protectorate sleeve prune --caches --workspaces --yes
protectorate sleeve kill worker --drop-caches
```

`--homes` deletes the entire home, including local auth links, sessions,
history and config. It preserves the sleeve record and the hub's separate
chat logs. `kill --drop-caches` first stops the sleeve and revokes its token,
then drops only the cache allowlist; other kill callers retain their existing
permissions, but cache dropping requires the owner token.

Running sleeves, their workspaces and their reference mounts are always kept.
`protectorate sleeve prune --caches --running` inventories them but keeps their
data, even with `--yes`. Go supports native age-based trimming, not a
configurable size bound: it scans at most daily and expires entries unused
for five days ([Go cache implementation](https://go.dev/src/cmd/go/internal/cache/cache.go)).
The hub reports the Go cache size instead of forcing cleanup during builds.

Workspace candidates are plain directories immediately under the hub's own
`workspaces/` clone namespace. Edit-in-place checkouts elsewhere, symlinked
paths, non-git directories, linked worktrees, shallow clones, stashes, submodules and
repositories with object alternates are kept. A selected clone must have no
staged, unstaged, untracked or ignored files; assume-unchanged/skip-worktree
flags cannot conceal edits. Every local branch tip, local tag and HEAD must
be reachable from origin refs fetched by the hub with a 30-second timeout.
Local origins must resolve outside the hub clone namespace. Unavailable or
unverifiable origins mean keep. Private HTTPS origins requiring credential
helpers are kept; verification disables helpers and interactive prompts.

Verification runs Git only against fresh temporary bare metadata, imports
literal refs, and reads the clone's objects without loading its config,
index flags, attributes configuration, hooks, filters or fsmonitor. A local
source checkout or bare remote is also read through fresh trusted metadata,
so upload-pack cannot execute its config. The environment is explicit, Git
hooks and fsmonitor are disabled, and protocols are restricted to file,
HTTP(S), SSH and native Git. SSH ignores user config, disallows prompts and
requires a known host key. Unsupported metadata or fetch failure prints a
keep reason. A reserved sleeve name prevents spawn/cleanup races; filesystem
deletion is confined with Go's rooted filesystem operations.

No state conversion is required. Restart the updated hub to enable sampling,
the new CLI/RPCs, and discovery of pre-upgrade leftover homes.

## Running runtime tests inside a sleeve

Opt in when spawning a development sleeve:

```sh
protectorate sleeve spawn runtime-dev --workspace /absolute/checkout \
  --harness codex --nested --cpu 200 --mem 5G
```

`--nested` requires explicit CPU and memory limits. The option is persisted,
reported by `sleeve show` and the API's `nested` field, and retained across hub
restarts and resleeves. Existing sleeves need to be respawned with the flag;
there is no state conversion. Ordinary sleeves retain their existing profile.
The host needs cgroup v2 with delegated cpu, memory and pids controllers,
unprivileged user namespaces, bwrap 0.8 or newer, and util-linux `unshare` and
`flock` (plus GNU coreutils and findutils).
Missing delegation or namespace support fails the launch; it never falls back
to running a command outside the sandbox.

Inside the new sleeve, build and run against its own checkout:

```sh
mkdir -p "$HOME/.cache/protectorate-tests"
TMPDIR="$HOME/.cache/protectorate-tests" make test
env -u PROTECTORATE_TOKEN_FILE -u PROTECTORATE_HUB -u PROTECTORATE_TOKEN \
  PROT_E2E=1 PROT_E2E_BINARY="$PWD/bin/protectorate" \
  go test -count=1 -v ./tests/e2e -run '^TestBinaryNestedRuntime$'
```

Using the sleeve home for test temporary files keeps SDK-copy fixtures on
disk. The sandbox's `/tmp` is tmpfs and its contents count toward the memory
limit.

The runtime automatically discovers the private cgroup mount at
`/run/protectorate/cgroup`. It creates child cgroups there and runs nested
bwrap without the host systemd user manager or its bus. Each child gets the
usual fresh filesystem, PID, IPC and UTS namespaces, its own home and hub
state, and a private cgroup namespace. Nested-enabled children may delegate
again; children without the flag receive no cgroup mount and root their cgroup
namespace in a leaf below their hidden resource limits. This also confines a
child that mounts a new cgroup filesystem, without relying on `nsdelegate`.
Network access follows the parent's existing shared network profile.

The outer systemd scope uses `Delegate=yes`. Its CPU, memory, swap and pid
ceilings remain outside the sandbox. Only a `delegated/` child is mounted
writable, and the cgroup namespace is rooted there. A `run/` leaf holds the
parent workload so controllers can be enabled on the otherwise empty
delegation root. Subprocesses enter their child cgroup after limits are
installed and before nested bwrap executes their command. Raising a visible
child limit cannot raise the hidden ancestor's limit. The outer scope also
bounds delegation to 256 descendant cgroups and 16 levels. Killing a scope
kills its entire descendant tree; child launch supervisors reap empty cgroups.

The sleeve's test runner executes inside bwrap, with the kernel enforcing
hierarchical limits and namespace confinement. The host test
`TestBinaryNestedRuntime` copies the selected production binary and the e2e
runner into a temporary workspace, starts a real `--nested` sleeve, and runs
the runtime scenario there. It checks persisted opt-in, ownership, restart,
chat, hidden parent state, actual memory OOM and CPU throttling, unchanged
outer limits, and descendant cleanup. The same test can run inside an already
nested-enabled sleeve and exercises another sandbox level.

Design references: [systemd cgroup delegation](https://systemd.io/CGROUP_DELEGATION/),
[Linux 6.6 cgroup v2 delegation and namespaces](https://www.kernel.org/doc/html/v6.6/admin-guide/cgroup-v2.html),
and [bwrap 0.8 namespace options](https://github.com/containers/bubblewrap/blob/v0.8.0/bubblewrap.c).
The namespace probes in the original non-delegated development sleeve passed
with bwrap 0.8.0; the writable cgroup path must be verified by the host e2e.

The follow-up probes also passed with bwrap 0.13.1 selected from PATH inside
that same sleeve: nested user, PID and cgroup namespaces, tmux, and two stacked
`--tmp-overlay` layers. Child overlay writes stayed out of the parent layer,
and parent writes stayed out of the original lower directory. No relaxed
security flags were needed. These overlay capabilities do not supply cgroup
delegation; the hidden ancestor limits and private subtree remain necessary.
The binary e2e harness copies the selected bwrap into each private HOME and
checks its version inside the nested sleeve, so a user-installed binary is
not replaced silently by the system binary. See the
[bwrap 0.13.1 implementation](https://github.com/containers/bubblewrap/blob/v0.13.1/bubblewrap.c).
