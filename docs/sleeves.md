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
`pub-global_packages` or `go-build`. Upper/work must share a filesystem. Home
disk/cache accounting and pruning cover this tree, excluding the host lowers
and shared Go snapshot.

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

With overlays, `GOCACHE=$HOME/.cache/go-build` layers private writes over one
immutable shared snapshot at `<state>/cache/go-build-lower`. The first hub
startup that finds an owner cache copies it once with `cp --reflink=auto`,
checks output SHA-256 hashes, quarantines incomplete outputs/indexes and
atomically publishes the snapshot. On ext4 without reflinks this costs one
extra shared copy plus hashing I/O at hub startup; sleeve spawns do no copying.
Later starts reuse it. Newly warmed owner entries are not automatically
imported. Snapshot mtimes are set ahead to prevent Go's cache-hit mtime
refresh from copying whole archives into the upper. New sleeve entries keep
normal Go trimming. Missing/incomplete entries become cache misses. Without
overlays, Go keeps its existing private build cache.

The kernel documents that changing an underlying filesystem during an overlay
mount has [undefined behavior](https://docs.kernel.org/filesystems/overlayfs.html#changes-to-underlying-filesystems).
Go's cache is **not** strictly add-only with atomic renames: its
[implementation](https://go.dev/src/cmd/go/internal/cache/cache.go) rewrites
action indexes in place, updates mtimes on reads and trims old entries. A
frozen copy isolates these changes. Sleeve overlay writes cannot poison the
snapshot, owner cache or another sleeve. The runtime scenario replaces a host
output after snapshot publication and checks warm reuse with no compiler
invocation. Discovered owner cache trees and their parents cannot be selected
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
Never modify the shared Go lower while sleeves use it.

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
