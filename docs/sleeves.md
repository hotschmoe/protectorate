# Sleeve development environments

Start in an existing directory with `protectorate sleeve browse --json` to
list the hub's browser roots, then `protectorate sleeve browse /absolute/root`
to list its subdirectories. Pass the selected path to
`protectorate sleeve spawn worker --workspace /absolute/checkout --harness codex`.
The directory is edited in place. The app's NEW SLEEVE dialog offers the same
browser alongside known workspaces, a typed absolute path and a clone URL.

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
