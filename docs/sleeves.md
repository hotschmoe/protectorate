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
directories and every `hub serve --reference-dir`, without walking the home.
Other safe absolute directories can be mounted explicitly.

Toolchains are mounted read-only from narrow tool roots the hub discovers
(`~/.local/{bin,lib,opt,go}`, `~/go/bin`, `~/.cargo/bin`, `~/.rustup/toolchains`,
the Pub tool directories and NVM's Node versions) plus `serve --terminal-tool-root DIR` and the per-harness
`--<harness>-tool-root DIR`. Caches start empty and private in the sleeve home
(`GOCACHE`, `GOMODCACHE`, `PUB_CACHE`, `CARGO_HOME`, `RUSTUP_HOME`,
`XDG_CACHE_HOME`); nothing is copied from the owner at launch. To share
downloads, publish a read-only generation once and pass its printed path to
`serve`:

```sh
protectorate-rs hub publish-flutter-cache --home STATE --flutter-sdk-root SDK   # --flutter-sdk-root SDK --flutter-cache-lower PATH
protectorate-rs hub publish-flutter-tools --home STATE --flutter-sdk-root SDK   # --flutter-tools-lower PATH
protectorate-rs hub publish-pub-cache --home STATE --pub-cache-root ~/.pub-cache   # --pub-cache-lower PATH
protectorate-rs hub publish-go-mod-cache --home STATE --go-mod-cache-root ~/go/pkg/mod   # --go-mod-cache-lower PATH
```

Each sleeve then gets a private overlay upper over the generation; writes
never reach the owner's caches or another sleeve. Byte caps, exclusions and
verified workloads are in [rust-sandbox.md](rust-sandbox.md#current-launcher-and-limits).
`protectorate api call HubService/GetSleeveRuntime '{}'` (MCP
`hub_get_sleeve_runtime`) reports the sandbox helper and overlay support.

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

A sleeve's first launch seeds `$HOME/.gitconfig` in its private home with the
owner's global git `user.name` and `user.email` (`git config --global` as the
hub user), so agents commit with a real identity; nothing is seeded unless
both are set, and a sleeve's own `.gitconfig` is never replaced. The hub never
changes global or workspace git config. Caller-provided `GIT_AUTHOR_*` and
`GIT_COMMITTER_*` override the identity. Claude sleeves also get
`DISABLE_AUTOUPDATER=1` (its install directories are read-only in the
sandbox), and terminal Claude a `.claude.json` that skips first-run onboarding.

Idle removal applies only to chat sleeves and defaults off. After continuous
IDLE with no pending or queued mail for `--exit-after-idle`, the hub closes the
harness, kills its scope, revokes its identity and publishes the normal sleeve
exit event. Working turns, approvals, compaction, hub takeovers and pending
mail prevent removal. The home and workspace remain on disk. The timeout and
mounts survive hub restarts and resleeves; the idle clock restarts after a hub
restart.

## Build concurrency

A CPU quota does not change the core count toolchains detect, so the sandbox
sizes their worker pools to it: `--cpu 250` sets `GOMAXPROCS=2`,
`GOFLAGS=-p=2`, `CARGO_BUILD_JOBS=2` and `FLUTTER_TEST_CONCURRENCY=2`
(fractional cores round down, at least one; unlimited CPU sets nothing). A
value passed with `--env` wins. Flutter reads no variable itself; pass
`--concurrency="$FLUTTER_TEST_CONCURRENCY"` to `flutter test`. Several heavy
builds at once still share the sleeve's memory cap.

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

## Terminal sleeve status

The hub reads the bottom non-empty lines of each running terminal sleeve's
screen once a second and classifies it as working, blocked (a dialog waits for
the owner) or idle, with a small per-harness rule table in
`protectorate-session/src/activity.rs` for claude, codex, grok and pi. The first
matching rule wins and a blocking dialog outranks the working indicator. A
change, once two scans agree, is announced as `sleeve.activity` and appears as
`terminal` on the sleeve, in the fleet cards and in the `sleeve ls` state.
`protectorate sleeve status <name> --explain` reads the screen now and lists
every rule, which one decided, and the lines examined. A screen no rule
recognizes reads as unknown. When a harness changes its TUI, add the new pane
to `rust/fuzz/corpus/terminal_pane/` and adjust the rule.

## Harness pinning

Every launch records, per mode and harness (`chat/claude`, `terminal/shell`),
the SHA-256 of the harness executable that runs and of its profile: tool
roots, read-only mounts, limits, environment, the egress allowlist and the
sleeve token's scopes (its MCP tool allowlist). `sleeve show` and the API
report them as `harness_sha256` and `profile_sha256`. When a later spawn,
resleeve or restart of the same sleeve would launch something different,
for example after the harness updated itself, the hub compares before it
stops anything: a launch the owner asked for is recorded as
`harness.pin_changed` in the audit log and announced as `sleeve.pin_changed`;
one Envoy, a sleeve or the hub itself (recovery after a restart, Envoy woken
by mail) asked for opens an approval request (`harness.pin`, see
[auth.md](auth.md#approval-gates)) naming the old and new hashes and does not
launch until the owner approves exactly those hashes.

## Token budgets

`sleeve spawn --budget N` (and `sleeve envoy --budget N`) caps a chat run at
N weighted tokens, counted from the usage the harness reports through its
headless protocol: input + output + 0.1 x cache reads + 1.25 x cache writes,
the provider pricing of a cached prefix. At 90% the hub posts a notice in the
chat and audits `budget.warning`; at 100% it interrupts the running turn,
posts a notice, audits `budget.exhausted`, announces `sleeve.budget` and
refuses further turns, mail injection included. A resleeve or restart keeps
the run's count; respawning starts a new run. A sleeve that Envoy or another
sleeve spawns gets at most its parent's remaining budget (all of it when it
asks for none), its usage counts against the parent's run too, and a parent
with nothing left cannot spawn. Terminal sleeves have no usage reports and
take no budget.

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
data, even with `--yes`. The hub reports the Go build cache size instead of
forcing cleanup during builds; Go trims its own cache by age.

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
deletion goes through descriptor-anchored paths.

## Running runtime tests inside a sleeve

Opt in when spawning a development sleeve:

```sh
protectorate sleeve spawn runtime-dev --workspace /absolute/checkout \
  --harness codex --nested --cpu 200 --mem 5G
```

`--nested` requires explicit CPU and memory limits and cgroup delegation on
the host (cgroup v2 with delegated cpu, memory and pids controllers and
unprivileged user namespaces); without them the spawn fails, never falling
back to an unsandboxed run. The option is persisted, reported by `sleeve
show` and the API's `nested` field, and kept across hub restarts and
resleeves.

The outer systemd scope uses `Delegate=yes`; its CPU, memory and task
ceilings stay outside the sandbox. Only a delegated child cgroup is mounted
writable, inside the sleeve's own cgroup namespace, so a child sandbox started
inside can set its own limits but never raise or reach the outer ones.
Killing the scope kills every descendant. Inside the sleeve, run the suite
against its own checkout with temporary files on disk, since `/tmp` is a
tmpfs charged to the memory limit:

```sh
mkdir -p "$HOME/.cache/pt" && TMPDIR="$HOME/.cache/pt" make test
```

`protectorate-hub/tests/nested_sleeve_workload.rs` starts a real nested sleeve
through the hub, builds a sibling module under a private overlay, and checks
delegated child cgroup control, secret denial and cleanup of the scope and
mount staging.
