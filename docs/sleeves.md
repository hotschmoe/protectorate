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
The SDK is read-only. At startup the hub resolves bwrap from PATH, probes its
version and help once, and pins that executable for launches. `protectorate
status` prints the path, version and overlay capability. For JSON, use
`protectorate api call HubService/GetSleeveRuntime '{}'` (MCP
`hub_get_sleeve_runtime`).

With overlay support, every workspace gets copy-on-write Flutter `bin/cache`
and existing Pub hosted, hosted-hashes, git and global package caches. There
is no per-sleeve seed copy. Host directories are lower layers; writes go only
to the sleeve upper. `PUB_CACHE` links to these same overlays so global tools
and project commands share private changes. Persistent upper/work pairs live
at `<state>/sleeves/<name>/home/.cache/overlays/<cache>/{upper,work}`; cache
is `flutter`, `pub-hosted`, `pub-hosted-hashes`, `pub-git`,
`pub-global_packages` or `go-build/<snapshot-generation>`. Upper/work must
share a filesystem. Home disk/cache accounting and pruning cover this tree;
shared Go generations are inventoried separately by prune.

Overlay options require bwrap 0.9 or later and a kernel that permits
unprivileged overlay mounts. Debian 12's bwrap 0.8 keeps the copy fallback:
only Dart workspaces (a `pubspec.yaml` at the root or one directory down) get
SDK/Pub copies, unless that sleeve already has them. Copies use reflinks when
supported; otherwise the first spawn copies about 2.5 GB. An advertised
overlay option with a kernel/filesystem mount failure is a launch error.

On Debian 12, build a newer bwrap locally with Meson, Ninja, a C compiler and
pkg-config installed. From the upstream 0.13.1 source:

```sh
git clone --branch v0.13.1 --depth 1 https://github.com/containers/bubblewrap.git
cd bubblewrap
meson setup build --prefix="$HOME/.local" -Dman=disabled -Dselinux=disabled \
  -Dbash_completion=disabled -Dzsh_completion=disabled
meson compile -C build
meson install -C build
export PATH="$HOME/.local/bin:$PATH"
bwrap --version
```

Put this PATH in the hub service environment and restart the hub. The local
installation leaves `/usr/bin/bwrap` available for the fallback.

`PUB_CACHE=$HOME/.cache/pub` and `GOMODCACHE=$HOME/.cache/go-mod` are writable
per sleeve. Owner Go caches are discovered with `go env GOCACHE GOMODCACHE`.
Modules stay private and use the discovered host download cache through a
read-only file proxy, then the public proxy. This avoids overlaying download
locks and partial module downloads.

With overlays, `GOCACHE=$HOME/.cache/go-build` layers private writes over a
generation at `<state>/cache/go-build/snapshot-*/data`. Preparation runs in
the background at every hub start and again after 24 hours. It never blocks
startup: before a generation is ready sleeves get a private GOCACHE, logged
at launch; a previous published generation can serve launches during refresh.
The hub imports action indexes used in the last five days, newest first,
up to `hub serve --sleeve-go-cache-max 20G` by default (`0` disables sharing).
The limit includes unique referenced output lengths and copied index bytes.
It hardlinks content-addressed `*-d` outputs and copies only the 175-byte
`*-a` indexes. There is no output copy or full hash pass. Each referenced
output's size is checked before and after linking; missing, malformed and
size-mismatched entries are skipped. A different filesystem or hardlink
failure disables sharing with a log; it never falls back to copying outputs.

Publication atomically switches `current.json` after completing a generation.
Running sleeves keep their mounted generation. Upper/work pairs include its
name, so respawning against a new lower never reuses an upper built against
a different lower. The hub keeps trusted references outside writable homes,
checks pending launches and live scopes before deletion, and conservatively
retains generations when a live sleeve's reference is missing or corrupt.
Unreferenced old generations are collected after refresh; explicit cache
prune can also remove an unreferenced current generation. Status RPC fields
report preparation, errors, selected lower, age, logical bytes, entry count
and the configured limit. Without overlays, Go keeps its private build cache.

The kernel documents that changing an underlying filesystem during an overlay
mount has [undefined behavior](https://docs.kernel.org/filesystems/overlayfs.html#changes-to-underlying-filesystems).
The inspected Go 1.27.2
[cache.go](https://go.dev/src/cmd/go/internal/cache/cache.go) rewrites action
indexes in place (`putIndexEntry`), which is why indexes are copied. Its
`copyFile` verifies an existing output's content hash and otherwise writes
the bytes associated with that content-addressed name; ordinary successful
rewrites have identical content. Trimming unlinks names (`trimSubdir`), so a
snapshot's output hardlink survives. `GetFile` checks referenced output size,
matching the import validation. The real-binary scenario verifies separate
index inodes, shared output inodes, index rewrite isolation and survival of
owner unlinking; the runtime lane verifies a warm build without compilation.

The generation has its own directory namespace, but hardlinked output inodes
can still receive owner mtime updates or identical-content rewrites. This
accepts ordinary Go cache operations rather than claiming fully immutable
lower inodes despite the kernel's general caveat. Corruption, interrupted
output writes or manual in-place mutation are outside that assumption; Go's
size checks turn missing/incomplete outputs into cache misses, and users
should stop affected sleeves before manually altering cached data. The hub
does not change owner mtimes. Go's hourly use-time refresh can copy older
outputs into a sleeve's upper; those bytes count toward its cache usage and
remain private. Sleeve writes cannot poison the owner cache, shared lower
or another sleeve. Discovered owner cache trees and their parents cannot be selected
as writable workspaces, preventing a workspace bind from bypassing isolation.
Flutter/Pub lowers remain live host directories: prewarm them
first and stop affected sleeves before owner SDK/package updates. The runtime
scenario checks byte-for-byte preservation by a sleeve, not coherence during
concurrent owner SDK/package changes.

`make gen`, `make app-check`, and Go commands use these paths; no SDK download
into `/tmp` is needed. Existing sleeves keep their mounts until respawn. Old
private copies remain until explicitly dropped: reclaim an old home with
`protectorate sleeve kill NAME --drop-caches` before respawning. No state
conversion is needed. Use that cache drop before switching an existing home
between overlay and copy modes. Private cache creation is confined to the
sleeve home; a cache symlink pointing outside it makes respawn fail safely.
Never manually modify published Go generations while sleeves use them.

Codex's native package contains `codex-package.json` (`layoutVersion: 1`),
with `entrypoint`, `pathDir` and `resourcesDir` relative to the package root.
The installed 0.163.0 npm launcher additionally detects npm/pnpm/bun/Vite+
ownership, sets `CODEX_MANAGED_PACKAGE_ROOT` and exactly one
`CODEX_MANAGED_BY_*`, and clears the competing manager variables. Codex's
[install-context implementation](https://github.com/openai/codex/blob/rust-v0.163.0/codex-rs/install-context/src/lib.rs)
uses this information for installation/update behavior. These requirements
are outside the three-path manifest contract. Consequently this change keeps
the official launcher; bypassing it is blocked pending a complete published
launch contract, rather than reproducing its package-manager heuristics.

An isolated 0.163.0 app-server measurement inside a sleeve, with private fresh
CODEX_HOME, initialized stdio and a two-second settle, measured 319,332 KiB RSS
for the launcher tree (Node 51,340 KiB, native 242,584 KiB, remaining Git
helpers) and 273,100 KiB for a directly launched native tree (native 240,000
KiB plus transient Git helpers). This is an investigative comparison, not
an implemented saving or a host runtime before/after benchmark. Both were
stopped after sampling. `codex features list` reports `multi_agent stable true`
under the sleeve CODEX_HOME; the private seed does not disable it. Use native
harness subagents for parallel work in one workspace, and another sleeve for
a separate branch, resource limits or isolation. See the official
[subagent guidance](https://learn.chatgpt.com/docs/agent-configuration/subagents).

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

## What the hub puts in front of a harness

Native prompt caches match on a prefix, so anything volatile near the start of
a conversation makes every session pay a cache write. The hub injects only:

- The brief, sent as the first user message exactly as the caller wrote it.
- The resleeve prompt, a fixed sentence naming `.protectorate/HANDOFF.md`. The
  timestamped, per-run handoff is read by the harness as a tool result, after
  the prefix.
- Mail turns: the message text with its ids and thread first, then fixed
  guidance. They are appended mid-conversation and never rewrite earlier turns.
- Envoy's `AGENTS.md` (`CLAUDE.md` links to it): built from the hub URL, the
  granted scopes and the method table, so it is byte-identical across
  respawns and new tokens. `hosts.md`, which does change, is only named there
  and read on demand.
- Seeded harness config (`.claude.json`, `config.toml`) and environment
  variables; no system prompt is added or appended, and no timestamps, node
  names, lease ids or random ids are placed in any of them.

Values that must change per run (the token, the sleeve name, the date) live in
files or environment variables a harness reads on demand, never in the
injected text. Keep it that way: put anything new that varies last, or in a
file. `chat status` and `sleeve show` report each session's cache use; a
resumed session whose first call read nothing from the cache is flagged.

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

Cache prune also inventories generations as `shared-go-cache`, reporting
logical retained bytes separately from sleeve homes. Hardlinks share storage
with the owner and other generations, so these are not incremental allocated
disk bytes. Each generation respects the import cap; total retained bytes can
exceed it while running sleeves hold older generations. Mounted generations
are always kept, including after hub restart. Removing a snapshot unlinks
only its own names, never the owner's links. Sleeve cache cleanup includes
all generation-specific upper/work directories.

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
protectorate sleeve prune --workspaces --ignored --yes
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
data, even with `--yes`. Within a sleeve's writable cache, Go supports native
age-based trimming rather than a configurable size bound: it scans at most
daily and expires entries unused
for five days ([Go cache implementation](https://go.dev/src/cmd/go/internal/cache/cache.go)).
The hub reports the Go cache size instead of forcing cleanup during builds.

Workspace candidates are plain directories immediately under the hub's own
`workspaces/` clone namespace. Edit-in-place checkouts elsewhere, symlinked
paths, non-git directories, linked worktrees, shallow clones, stashes, submodules and
repositories with object alternates are kept. A selected clone must have no
staged, unstaged, untracked or ignored files; assume-unchanged/skip-worktree
flags cannot conceal edits. `--ignored` excuses files the clone's tracked
`.gitignore` rules ignore, such as build output; other untracked files still
keep it, and so does any untracked `.gitignore`, which could otherwise hide
real work as ignored. Every local branch tip, local tag and HEAD must
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
to running a command outside the sandbox. The hub probes `unshare --user
--map-current-user --cgroup` at startup and refuses `--nested` with the reason
when it fails; kernels without swap accounting simply skip the child's swap
limit.

Inside the new sleeve, build and run against its own checkout:

```sh
mkdir -p "$HOME/.cache/pt"
TMPDIR="$HOME/.cache/pt" make test
env -u PROTECTORATE_TOKEN_FILE -u PROTECTORATE_HUB -u PROTECTORATE_TOKEN \
  TMPDIR="$HOME/.cache/pt" \
  PROT_E2E=1 PROT_E2E_BINARY="$PWD/bin/protectorate" \
  go test -count=1 -v ./tests/e2e -run '^TestBinaryNestedRuntime$'
```

Using the sleeve home for test temporary files keeps binary and cache fixtures
on disk. The sandbox's `/tmp` is tmpfs and its contents count toward the memory
limit. The binary harness puts only tmux sockets in separate private
`/tmp/pt-tmux-*` directories, stops each server, then removes its socket
directory. Long disk-backed test paths therefore cannot exceed Unix socket
path limits.

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

That scenario explicitly mounts the installed Go toolchain and its private
executable peers, and seeds private Pub lower trees. It excludes unused Flutter
caches to avoid stacking another overlay above the development sleeve's SDK.
The selected real bwrap, production hub/CLI, namespace and cgroup checks stay
unchanged, and the inner run fails on any skip. Its binary and cache fixtures
use its private home as disk-backed temporary storage.

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
