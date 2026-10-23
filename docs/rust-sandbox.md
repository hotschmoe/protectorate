# Rust sandbox runtime

The Rust backend has a first small Linux launcher for Protectorate's workloads.
The boundary is the selected workspace, private sleeve home, selected
provider credentials, and explicitly granted references. The launcher will
replace the `bwrap` invocation in sleeves and shared-node jobs. It does not
need to reproduce `bwrap` arguments or Go's persisted launch format.

## Evidence from this machine

The 2026-10-20 [dogfood record](dogfood-2026-10-20.md) and the 2026-10-19
through 2026-10-22 sleeve chat logs under
`~/.protectorate/sleeves/*/chat.jsonl` show the following constraints:

| Observed problem | Requirement for the Rust runtime |
|---|---|
| A worker could not build sibling modules because the sandbox showed only its checkout. | Allow a named set of sibling repositories as read-only references at their real paths. Keep write access confined to the task workspace. Report the grants in the sleeve record. |
| Runtime tests inside an ordinary sleeve skipped when the systemd user cgroup was absent. | Provide an explicit nested-test profile with a delegated cgroup subtree and nested user, PID and cgroup namespaces. Report a failed prerequisite as a failed test, never a passing skip. |
| A Git source mask bound from `/dev/null` was unreadable under a nodev bind. | Mask files with a private, ordinary empty file. Include a real read check in the launch test. |
| An old spawn copied roughly 2.4 GB of Flutter/Pub cache; a later overlay exposed `work/work` with unusable permissions to disk sampling and backup. | Share immutable cache lowers with per-sleeve writable uppers when the host supports it. Keep overlay work directories outside user-visible home scans and backup, enforce private permissions, and verify the mount with a small write/read probe. Use a private cache if the optional shared cache is unavailable. |
| A host Go build cache was 111 GB; a design that copied and hashed it at startup was too costly. | Bound any cache snapshot by recent use and byte budget. Prepare it in the background and never make hub startup wait for it. |
| A real Go build of this repository inside the Rust launcher failed writing its temporary work directory: `/tmp` was a fixed 256 MiB tmpfs, while the bubblewrap path left it at the kernel default. | Size the workload's temporary filesystem from its own memory allowance instead of a fixed constant, and keep it bounded. |
| The journal OOM-killed several live sleeve scopes during builds, and the sleeve chat identified the cause: a 7.3 GiB build target directory on the `/tmp` tmpfs consumed the whole cgroup memory allowance. | tmpfs pages are charged to the cgroup and cannot be reclaimed without swap. The temporary ceiling must leave a floor for the workload's own memory, and `/tmp` and `/var/tmp` must share one ceiling. This prevents the reproduced double-ceiling OOM case and allows a full temporary filesystem to return `ENOSPC` while that reserved memory remains available. A bind mount keeps `/var/tmp` usable for explicit child mounts. |
| Several current per-sleeve overlay homes on this host are 5–7 GiB. | Keep per-sleeve cache accounting and pruning independent of the overlay work directories and report the space in sleeve status. |
| PATH in the hub unit omitted a required tool; hidden ancestor Git metadata changed warm-build actions. | Resolve executable and tool mounts from the hub's real environment during planning, record the chosen paths, and test one build in the exact sandbox view. Make source Git metadata exposure explicit. |
| Coding harnesses use `bash -lc`, which resets PATH through `/etc/profile`; the seeded sleeve profile reads `PROTECTORATE_SLEEVE_PATH`. | Set that variable from the validated sandbox PATH for sleeve workloads. Verify both values inside a real sandbox. |
| The current hub journal records successful bwrap 0.13.1 overlay and nested capability probes, while chat logs record launch and test failures that the journal alone does not explain. | Keep launch diagnostics and capability probes attached to the sleeve or job result, with a short reason when a requirement fails. |

The current state file contains chat sleeves for Codex and Pi, terminal shell
sleeves, and several nested chat sleeves with 2–3 CPU cores and 5–8 GiB limits.
The Rust runtime must serve these actual profiles. The same isolation plan is
also used for shared-node jobs; care-node and admin jobs deliberately use a
different execution policy ([node work](node-work.md#sandboxes-on-nodes)).

## How sleeves are used

The live inventory has an owner-created Envoy orchestrator, task sleeves
created by Envoy or the owner, terminal sleeves used for interactive tools,
and nested sleeves that exercise the real launcher from within a bounded
parent. A chat sleeve runs its selected harness's headless protocol under hub
control; a terminal sleeve runs its TUI in tmux. The four harnesses are Codex,
Claude, Grok and Pi. Each gets a private home and harness configuration, while
the chosen provider login is shared through one narrow auth directory. Pi is
used for inexpensive, bounded work whose result can be reviewed. Resleeve
changes the chat harness on the same task workspace with a handoff, so the
workspace and chat state must outlive a harness process.

Tasks have one writable checkout, sometimes with named read-only sibling
repositories. The live Omarchy task had seven such references; the Rust
sibling-build probe now exercises this grant model. The hub also injects the
scoped sleeve identity and CLI access needed for mail, backlog, reports and
coordination. The runtime must preserve these product behaviors without
exposing the owner's other workspaces or login directories.
Branch-dispatched sleeves clone into a fresh managed workspace. Their `base`
remote points at a hub-owned Git view containing only sanitized configuration,
refs and objects. The source checkout's config, reflogs and fetch metadata are
not mounted. The hub refreshes packed refs in this view and sends one idle
mailbox notice when the source branch advances. The new branch starts with
tracked files checked out and a clean index; the live Go clone incident showed
why that is a required launch check.
The installed Grok binary on this host lives in `~/.grok/downloads`; that exact
directory may be configured as a read-only tool root, while the rest of
`~/.grok` remains protected. A live namespace test runs a binary from that
directory and confirms the sibling account file is hidden.

## Workload profiles

| Profile | Visible writable paths | Extra authority | Process and resource rules |
|---|---|---|---|
| Chat or terminal sleeve | Its workspace and private home | Its selected provider credential directory, read-only toolchain and named references | Private PID/IPC/UTS, mount and network namespaces; egress only through the allowlisting proxy; optional CPU/memory caps; terminate descendants with the supervisor |
| Nested runtime sleeve | Same as a sleeve | One delegated cgroup subtree, mounted inside its own cgroup namespace | Parent caps remain effective; child scopes cannot write parent controls |
| Shared-node job | Its job directory, warm worktree and private job home | Read-only system/tool directories; no hub or node identity | Same process boundary, with CPU/memory/task limits and cancellation of all descendants |
| Read-only helper | Private temporary home, optional scratch directory | Only the named source repositories, selected tools and inputs | Short deadline and no writable source mount |
| Care-node or admin job | As authorized by node job policy | Host access is explicit in the job record | Never label this execution as sandboxed |

Every sandbox has its own network namespace with only loopback; the next
section describes the egress proxy that is its only way out.

## Implementation shape

1. **Plan.** Build a typed plan from a workload profile and explicit grants.
   Resolve each source path with a file descriptor and reject symlink escapes,
   credential paths, overlapping writable grants, mount shadowing and a
   writable host home. Toolchain mounts are separate, preapproved narrow
   read-only grants (for example Cargo's executable directory and a Rust
   toolchain), never a blanket mount of a provider or package-manager home.
   Configured SDK roots such as Flutter may be mounted read-only, including
   their `bin` directories on the default PATH. The plan names the executable, environment, private
   home, mounts, cache policy, limits and whether nested delegation is
   required. It is inspectable before launch.
2. **Launch.** Re-exec a small Rust helper process. The async hub never calls
   namespace operations inside its own process. Pass the validated plan over
   a private descriptor, set up user/mount/PID/IPC/UTS namespaces and a fresh
   root, mount the selected directories, install a real `/dev` and `/proc`,
   drop setup privileges, clear inherited environment and descriptors, then
   exec the workload. Pin source paths before entering the new namespace,
   then reopen them in that namespace and compare device and inode before
   mounting. This is necessary because an open-tree clone of an FD from the
   old mount namespace failed on this host.
3. **Supervise.** Keep the user systemd scope for ordinary cgroup limits
   initially. Use the already delegated subtree for nested tests. The parent
   owns cancellation, exit status, diagnostics and descendant cleanup.
   Failing to establish a required boundary fails closed; the explicit
   care/admin policy is the only unsandboxed path.
4. **Prove.** Run a real nested sleeve that builds sibling modules, verifies
   masks and secret denial, mounts and writes each cache, launches a child
   sandbox under parent limits, and confirms cleanup after normal exit,
   cancellation and parent death. Run the shared-node job scenario separately.
   Cover this host's overlay and no-overlay configurations.

## Egress

A sandboxed workload has a private network namespace containing only
loopback; it cannot open a socket to the host or the network. A plan with
egress gets one host-side HTTP CONNECT proxy per sandbox, started by the
launcher in the supervising process (the hub for chat sleeves and helpers,
the sleeve runner for terminal sleeves, the node for jobs). If the proxy
cannot start, the launch fails. The proxy's Unix socket is bound read-only
into the sandbox at `/run/protectorate/egress/proxy`, and its host name is
unlinked once setup finishes, so a killed supervisor leaves nothing behind.
A small relay process, forked by the helper into the workload's PID
namespace with no capabilities and not dumpable, listens on `127.0.0.1:3128`
and forwards each connection to that socket. The launcher sets
`HTTPS_PROXY`, `HTTP_PROXY`, `ALL_PROXY` (and their lowercase forms) to it,
`NO_PROXY=localhost,127.0.0.1,::1`, and `NODE_USE_ENV_PROXY=1`, the switch
Node's own fetch needs.

The proxy accepts only `CONNECT host:port`. It never terminates TLS. A
target is admitted by, in order: the hub's own `host:port`; the API and
token hosts of the plan's provider credential (`Provider::api_hosts`);
operator entries from `hub serve --egress-allow` or `node serve
--egress-allow` (`host`, `host:port` or `*.domain`, port 443 by default);
and, for Pi only, the hub's `--local-model-host` names on any port. IP
literals are refused unless they are the hub or a local model host. The
proxy resolves the name on the host and connects only to an address its rule
permits: provider and operator rules reach only globally routable addresses,
so loopback, RFC 1918, CGNAT, link-local (including `169.254.169.254`),
unique-local, documentation, multicast and reserved ranges are refused. Each
decision (allow, deny, or allowed-but-unreachable) is appended to the hub
audit log as procedure `egress` with actor `sleeve:NAME`; node jobs write it
to the node's log. Plain HTTP requests through the proxy get `405`.

When the hub listens on loopback, its port is also relayed inside the
namespace to a plain tunnel to the hub, so `PROTECTORATE_HUB` stays
unchanged for the CLI and MCP bridge. A non-loopback hub is reached through
the proxy like any other destination. Git helpers have loopback only, and
node jobs default to an empty allowlist. SSH remotes cannot work; use HTTPS
remotes.

Each harness was run in a real sandbox with a dummy key on this host
(2026-10-24) and its traffic was observed at the proxy:

| Workload | Honors the proxy | Destinations seen |
|---|---|---|
| Claude Code 2.1.309 (native) | yes | `api.anthropic.com` (401); telemetry to `http-intake.logs.us5.datadoghq.com` denied |
| Codex (npm, Node 22.23) | yes | `chatgpt.com`, `api.openai.com` (401); `github.com`, `api.github.com`, `files.openai.com` denied |
| Grok 1.0.67 (native) | yes | `api.x.ai` (key rejected) |
| Pi 1.4.0 (Node 24.22) | yes | `api.anthropic.com` (401) |
| `git ls-remote https://github.com/...` | yes | `github.com` allowed with an operator entry, `403` without |

A tool that ignores the proxy variables simply has no route and fails to
connect; nothing leaves the namespace. `tests/egress.rs` in the sandbox crate
checks the allowed, IP-literal, metadata, non-allowlisted, private-address,
non-CONNECT, direct-connection and hub-relay cases in a real sandbox.

## Workspace checkpoints

The hub snapshots a sleeve's git workspace when a chat turn settles, before
a resleeve, and on `CreateCheckpoint` (`protectorate sleeve checkpoint NAME
create`, MCP `sleeve_create_checkpoint`). A checkpoint is a parentless
commit of the workspace's tracked and untracked non-ignored files, under
`refs/protectorate/checkpoints/N` in a private Git directory at
`STATE/helpers/checkpoint-NAME/home/store`, built with a private index
beside it. It is never part of the workspace repository, so no push can
carry it, and hub backups do not include `helpers/`. The newest 20 are kept
and unreferenced objects are pruned. An unchanged tree reuses the latest
checkpoint. The workspace's own `.git/info/exclude` is honored and
`.protectorate-token` is always excluded.

Git never runs on the host for this. Each operation runs in a
`CheckpointHelper` sandbox that sees only the workspace (writable, for
restore) and the store in its own private home, with no network and no
credentials. So the workspace's config, hooks, filters and symlinks cannot
make the hub execute or read anything, and the sleeve, which cannot see
`helpers/`, cannot read or rewrite its checkpoints. `ListCheckpoints`,
`DiffCheckpoint` (between two checkpoints, or to the live workspace, cut at
1 MiB) and `RestoreCheckpoint` are open to the owner, devices, the sleeve's
own token and the identity that spawned it. Restore first checkpoints the
current state and returns it as `before`, then resets the non-ignored files
to the checkpoint, removing files it lacks; ignored files and the
workspace's `.git` and index are untouched. A busy chat sleeve refuses a
restore. The Flutter client has no checkpoint view yet.

## Current launcher and limits

`protectorate-sandbox` now has a dedicated helper and direct Linux launcher.
It creates a fresh tmpfs root, user/mount/PID/IPC/UTS namespaces, read-only
system and selected tool mounts, a private home, ordinary empty-file masks,
`/dev` and `/proc`, a private network namespace, and a bounded transient
systemd user scope. Cancellation kills the scope and process group. Launch fails if
the helper, user namespaces, mount APIs, or user systemd manager are missing.

Live tests on this machine proved workspace/private-home writes, denial of
ungranted secrets, readable masks, read-only references, an SDK executable on
PATH, explicit PATH preservation including the login-shell profile variable,
and cancellation of a background child. A
separate live sleeve test compiled a C program in its workspace using a header
from one explicitly granted read-only sibling module. The test now uses two
managed clones under the hub state and resolves the dependency as `../lib`,
matching the failed dogfood build. Hub chat, terminal and Git-helper plans
derive the current managed clone name, while a separate read-only sibling grant
may expose only one named clone root at its real path or at a validated remapped
path such as `/deps/lib`. A live C build passes with both mount forms. Broad
state and auth
mounts and writable sibling grants remain rejected. Another live test killed
the supervisor and verified that its namespace init exited.
For a linked Git worktree, the read-only Git helper also mounts the external
gitdir and commondir named by its `.git` file. It coalesces nested directories
and rejects protected host data; ordinary chat and terminal sleeves retain
their narrower workspace view. A real sandbox test runs `git status` in a
linked worktree and sees its branch and dirty file.
An overlay test on this host verified a shared immutable lower, private copy-up,
upper/work directories in private per-workload state outside the home, removal
of the mount staging paths before exec, and rejection of an unusable overlay
target.
These tests fail when a required host capability is absent; they do not report
a skip as success.

Delegated nested cgroups now work in a live host test: the outer scope has CPU,
memory and task limits, while a child sandbox starts inside its delegated
subtree without a second user systemd manager. The child can set its own
cgroup controls and cannot reach the outer controls. The launcher now backs
shell and Pi, Claude, Codex and Grok terminal sleeves, chat sleeves for those
four harnesses, and shared-node jobs through the public Rust HTTP services.
The agent terminal launcher uses the configured executable, keeps caller
arguments, mounts only that provider's credentials, and disables Codex's
managed daemon auto-start. A real sandbox and HTTP test launches fake TUIs
for all four providers, verifies arguments and credential isolation, then
kills them. Real authenticated vendor TUI sessions remain untested. Live
server checks launched a shell sleeve, wrote in its
workspace, killed it, streamed output from a sandboxed node job, and launched
the installed Pi package through a sandboxed RPC handshake. A fake Pi protocol
test also exercises Send, Watch, model selection, compaction, Report, Wait,
scoped-token revocation and Kill. Fake-protocol HTTP tests cover supervised
approval and resume for Claude, Codex and Grok. A disposable authenticated Pi
turn also returned its requested answer through the public Rust hub path.
Chat sleeves also accept the explicit nested profile with CPU and memory
limits. A live fake-Pi HTTP test launches it and resleeves onto Codex while
retaining delegation.
A combined live public-hub test now starts a nested terminal sleeve and proves
the `../lib` build, private Flutter overlay copy-up, delegated child cgroup
control, secret denial and an active bounded systemd scope in the same
workload. It then kills the sleeve and confirms the scope is inactive and the
host mount staging directory is gone. That test exposed staging left behind
when tmux terminated the runner before its destructor ran; the launcher now
removes the empty staging directory immediately after the helper reports
ready, and fails the launch if removal fails.
Temporary space now scales with the workload instead of a fixed constant.
`/tmp` is a tmpfs and `/var/tmp` is a bind mount of it, so both share one
ceiling: half the workload's memory allowance, normally at least 512 MiB and
at most 4 GiB. Capped workloads keep at least half their memory available at
small limits, and otherwise reserve the larger of 64 MiB or a quarter of the
allowance. tmpfs pages are charged to the
cgroup and cannot be reclaimed without swap, so a ceiling equal to the whole
allowance lets a full temporary filesystem push the cgroup past its limit and
OOM-kill the workload instead of failing the write with `ENOSPC`. Other workload
memory can still exhaust the cgroup before the temporary ceiling. A workload
with no memory cap gets half the host's memory under the same ceiling. The
previous fixed 256 MiB cap made a real Go build of this repository fail inside
the sandbox with `no space left on device` writing its temporary work
directory, which peaks near 1.7 GiB on this host. A live test reads the
capacity of both paths for a capped, a small and an uncapped workload, shows
that a 900 MiB temporary write succeeds while a write past the ceiling through
`/var/tmp` fails with `ENOSPC`, and shows that a 256 MiB workload that fills
`/tmp` and then writes through `/var/tmp` survives with `ENOSPC` instead of
being OOM-killed.

Full live harness workloads and the remaining job policies need further work.
Terminal sleeves now receive a scoped token and the actual
hub URL. When the companion CLI exists, they see it on PATH via a narrow
read-only file bind; if its MCP bridge companion exists, that binary gets a
second file bind. A real-sandbox test runs `protectorate mcp --list` through
the two actual binaries inside a terminal sleeve. A self-hosting sleeve may bind the CLI binary from its
own writable checkout; the separate read-only bind does not make the checkout
copy immutable. A live terminal test checks the mount, token access, hub
restart, Kill revocation and background revocation after a natural exit. These
must pass before the Go `bwrap` path can be retired. Owner-only Kill with
`dropCaches` now removes named regenerable home caches and Rust overlay state
through descriptor-anchored paths while retaining the workspace, transcripts
and credentials.

Chat and terminal sleeves now create empty, private `.cache/go-build`,
`.cache/go-mod`, `.cache/pub`, `.cache/flutter`, `.cargo`, and `.rustup`
directories under their sleeve home. `GOCACHE`, `GOMODCACHE`, `PUB_CACHE`,
`CARGO_HOME`, `RUSTUP_HOME`, and `XDG_CACHE_HOME` point there. This avoids
copying the owner's large caches at launch. Directory creation rejects
symlinks in those cache paths. A disposable `dart pub get --offline` succeeded
with the installed read-only Flutter SDK and private Pub cache. A disposable
Rust crate also built offline inside the sandbox with `~/.cargo/bin` and
`~/.rustup/toolchains` explicitly configured as narrow read-only terminal
tool roots; rustup uses the installed default toolchain while writable state
stays private. The hub now discovers existing narrow user tool roots for chat
and terminal sleeves by default; repeatable `--terminal-tool-root DIR` options
add further trusted roots for terminal sleeves. Credential-bearing parent
directories stay unmounted.
When a published Pub cache overlay is enabled, its `bin` and global tools
replace the live owner Pub tool binds; the two mounts cannot overlap.
The default GOPATH grant is `~/go/bin`, never `~/go`: the owner's live
`~/go/pkg/mod/cache/vcs` may contain authenticated Git remote URLs. The
sandbox policy rejects a whole-`~/go` tool grant, and a real sleeve test checks
that an owner VCS config is hidden.

For repeated Go builds, `protectorate-rs hub publish-go-mod-cache --home STATE
--go-mod-cache-root OWNER/go/pkg/mod` explicitly snapshots only the Go module
download cache into a bounded, read-only generation. Pass its printed path to
`hub serve --go-mod-cache-lower PATH` to give each sleeve a private writable
overlay over those downloads. The publisher rejects unexpected files and
excludes VCS state, lock files, checksum-server state and the owner's build
cache. The default budget is 3 GiB and the hard limit is 8 GiB. A fresh
sandboxed sleeve built a Go program offline with normal parallelism from a
369 MB published snapshot; writes remained private. Terminal and chat sleeves
now allow up to 4096 tasks, matching the previous Go default that the cold
build needed.

Both chat and terminal home preparation now seeds `.profile` before launch.
The seed restores `PROTECTORATE_SLEEVE_PATH` after `/etc/profile` changes PATH
in harness commands run through `bash -lc`. A login-shell test checks the
actual PATH from a fresh home. A background disk sampler records workspace,
home, cache and Go build cache bytes in sleeve status without walking those
trees on a status request. It counts writable overlay uppers as cache data and
ignores overlay work directories. A scan from an old sleeve incarnation cannot
update a respawned sleeve with the same name.

The installed Flutter command writes into its SDK's `bin/cache` even for
`flutter --version`. A bare read-only SDK fails on `engine.stamp.tmp` and
`engine.realm`. To opt into a writable cache view, run
`protectorate-rs hub publish-flutter-cache --home STATE --flutter-sdk-root SDK`
outside hub startup. The command caps the copy at 3 GiB by default (adjustable
with `--max-bytes`, with an 8 GiB hard ceiling), rejects symlinks and special
files, hashes the content, and atomically publishes a read-only generation
under `STATE/cache/flutter-*`. It prints that generation path. Start the hub
with `--flutter-sdk-root SDK --flutter-cache-lower PRINTED_PATH`; the SDK is
mounted read-only and only `SDK/bin/cache` receives a per-sleeve overlay.
Flutter project creation also writes to
`SDK/packages/flutter_tools/.dart_tool`. Run
`protectorate-rs hub publish-flutter-tools --home STATE --flutter-sdk-root SDK`
to publish that small directory, then add `--flutter-tools-lower PRINTED_PATH`
to hub startup. Its default byte cap is 4 MiB, with a 64 MiB hard ceiling.
Both overlays keep upper and work directories in sleeve state outside the
visible home; `dropCaches` removes them after exit.

The live published `bin/cache` generation on this host was 1,879,534,327
bytes and took about three minutes to hash and copy with the debug binary.
The Flutter tools lower was about 44 KiB. Per-sleeve writable upper data was
2,382 bytes for `bin/cache` and 39,178 bytes for tools `.dart_tool` in the
`flutter create --no-pub` probe. In disposable sandboxes, `flutter --version`,
`dart --version`, `dart pub get --offline` for a trivial package, and
`flutter create --no-pub` passed. Full `flutter create --offline` could not
resolve `test 1.31.0` from an empty private Pub cache; it needs a package
fetch or a separately published Pub lower. A full Linux desktop executable
build has not been verified. Published generations remain until an operator
removes them after their sleeves stop. No large host Flutter/Pub or Go cache
snapshot occurs at sleeve startup.
To reuse offline Pub packages and global tools, publish the owner's cache
explicitly with `protectorate-rs hub publish-pub-cache --home STATE
--pub-cache-root /home/OWNER/.pub-cache`. The source must be the same absolute
`/home/OWNER/.pub-cache` path that corresponds to the hub's user home because
Pub-generated scripts and package configurations embed that path. The command
copies only `hosted`, `hosted-hashes`, `git`, `global_packages`, and `bin` into
an immutable, hashed generation; transient Pub state is excluded. Publication
has a 3 GiB default and 8 GiB hard byte cap, entry and depth caps, and rejects
symlinks and special files. Start the hub with `--pub-cache-lower PRINTED_PATH`.
Each chat and terminal sleeve overlays that lower at its private
`$HOME/.pub-cache`, sets `PUB_CACHE` to the same path, and adds its private
`bin` directory to PATH. Writes persist only in that sleeve's upper; no owner
home directory is mounted. Without a published lower, sleeves keep their
empty private `$HOME/.cache/pub` fallback.
On this host, a 649 MiB owner Pub cache was published explicitly. A live
sandboxed sleeve executed the installed `protoc-gen-dart` script, resolved
`collection 1.19.1` with `dart pub get --offline`, and wrote a private cache
entry without changing the published lower or owner cache.
In a second sleeve over that same lower, the first sleeve's cache marker was
absent while the cached package was readable; its own write stayed private.
With the published Flutter SDK cache, Flutter tools cache and Pub cache in one
state root, an ignored host integration test ran
`flutter create --offline --platforms=linux`
for a new project and `flutter build bundle --no-pub` for the host architecture
inside a 2-core, 6 GiB sleeve. It produced `pubspec.lock` and
`build/flutter_assets/AssetManifest.bin`. Writes to the Pub and SDK caches
appeared only in the sleeve's private uppers; the published generations and
owner caches did not gain those files. This verifies Flutter project creation
and asset-bundle compilation. A full Linux desktop executable build remains
untested. The test is `protectorate-sleeve-runner/tests/flutter_offline.rs` and
requires `PROTECTORATE_TEST_FLUTTER_SDK`, `PROTECTORATE_TEST_FLUTTER_LOWER`,
`PROTECTORATE_TEST_FLUTTER_TOOLS_LOWER` and `PROTECTORATE_TEST_PUB_LOWER`.
Native Windows work is separate: this launcher is Linux-only.
