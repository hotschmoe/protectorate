# Work on nodes: tiers, jobs, admin, sandboxes, toolchains

Status: design, agreed in direction on 2026-10-20 and open to revisit. Not
built yet unless marked **(built)**.

## Principles

- Agents do not SSH into machines and work however they like. They submit
  **jobs through the hub**, which picks the node, checks the token, audits and
  streams output. SSH is an escape hatch for interactive needs, through
  short-lived hub-signed certificates (P11), never long-lived keys in sleeves.
- Shell and files first, GUI last ([desk.md](desk.md)).
- Anything that can be decided by code is: placement, offload, grants,
  toolchain detection.

## Two tiers

| Tier | Meaning | Jobs and leases | Admin (sudo/elevated) | Examples |
|---|---|---|---|---|
| `shared` | part of the shared compute pool | yes, sandboxed | only with a grant | agentworld, officepc, build boxes |
| `care` | a machine we maintain for a family member | never scheduled, never leased | only for a task the owner started | kidsbox |

A `care` node can never be relied on for work: the scheduler and lease
selection skip it, and `--on <care node>` is refused unless the token carries
an explicit grant for that node. Today the tiers are named `server|family`
and every node was enrolled as `server`; the migration renames them to
`shared|care` (tags `tag:tier-shared|care`, flags, docs) in one change, and
`kidsbox` moves to `care`. "Pool" stays the name of tailnet reachability
groups (`tag:pool-X`), which is a different thing.

## Jobs today (built)

`protectorate job run --on <node|lease|selector>` runs as the node agent's own
user, unsandboxed, in a job directory or a warm git worktree; `--git
repo@commit` ships only the missing objects, `--artifact` ships files.
Selection prefers fresh, less loaded nodes and warm worktrees. The job scope
is therefore shell access on every node the caller can target
([auth.md](auth.md)); on `kidsbox`, whose agent runs as `alarm` with
passwordless sudo, that is effectively root. Tier enforcement below closes
this.

## Admin grants

- `job run --admin` runs elevated (sudo on Linux, an elevated token on
  Windows), only with a token carrying `node_admin:<node>`.
- Grants are minted per task, short-lived (default 4h), name the node and the
  task, and every command is audited. Nothing has standing root anywhere.
- On `care` nodes a grant comes only from a task the owner started (you asked
  "update the kids' machine"); the orchestrator cannot grant itself admin on
  a `care` node.

Scenario: "update the kids' machine and check its wifi". The hub mints a task
token `job_*` + `node_admin:kidsbox` for 4h. The agent runs read-only checks
as plain jobs (`nmcli -f all dev wifi`, `iw dev <if> link`,
`journalctl -u NetworkManager --since -1d`), then `job run --on kidsbox
--admin -- pacman -Syu --noconfirm`, and reports in chat. The grant expires by
itself.

## Offload and placement

Deterministic rule: when the hub's CPU stays above a threshold (default 80%
for 5 minutes) it leaves itself out of build selection. An agent submits
`job run --on 'shared,os=windows' --git repo@commit -- make` (or any
selector), and the hub picks the least loaded matching `shared` node, warm
worktrees winning ties.

## Busy GPUs (decided 2026-10-20)

Some `shared` nodes serve models: `b70-nas-server` usually has a container
holding its two Arc Pro B70s with a loaded LLM. A GPU counts as busy when
another process holds most of its memory or it is under sustained load
(an idle loaded model still holds VRAM, so memory is the main signal).
Automatic placement (selector jobs and leases) skips a node whose GPUs are
busy. A job or lease that names the node or the GPU explicitly is refused
with who holds it, and the caller is told to ask: a sleeve sends a request
to the orchestrator or the owner (mailbox, `docs/coordination.md`), and the
decision to free the GPUs is made by a person, never by the scheduler.

## Sandboxes on nodes

- **Linux `shared` nodes:** jobs run in the same bwrap + systemd user scope
  as sleeves (PID namespace, cgroup limits, only the job directory and tool
  dirs mounted). Reuse `internal/sleeve`'s plan.
- **Windows `shared` nodes** (often someone's real desktop, with mail,
  browser sessions and BIMcloud under their user): jobs run as a dedicated
  low-privilege local account, `protectorate-runner`, that has no access to
  the user's profile, inside a **Windows Job Object** (CPU and memory caps,
  kill the whole tree on cancel: the Windows equivalent of a cgroup, so P8
  holds). Creating the account is one elevated step at install
  (`node install --runner`).
- **Not Docker on Windows.** Docker Desktop runs Linux containers in a WSL2
  VM: a large resident daemon, Docker Desktop licensing for larger
  companies, and it only gives us Linux, which a Linux node already provides.
  Windows containers need Hyper-V or Server and multi-GB images for MSVC
  toolchains. Windows nodes exist for Windows-native work (MSVC builds,
  Windows tests, ArchiCAD), which the runner account handles natively; Linux
  builds go to Linux nodes. Windows Sandbox (a disposable VM) stays an option
  for untrusted builds only. WSL2 with bwrap inside is a later option if a
  Windows box is the only free compute.
- An interactive agent session on a node, when needed, is a sleeve on that
  node (later), not a raw shell.

## Toolchains

- **Not bundled** in our binary: Go is about 250 MB, Rust and Flutter over a
  GB each, versions churn, licences vary.
- **Detected:** each node probes what it has every few minutes (`go`, `zig`,
  `rustc`/`cargo`, `node`, `python`, `dart`/`flutter`, MSVC via `vswhere`,
  `git`, versions included) and reports it in its inventory; `node show` and
  `protectorate who` list it.
- **Required:** `job run --needs go>=1.27,zig` selects only nodes that have
  them.
- **Provisioned on demand:** `job run --provision` lets the node install
  exactly what the repository pins, through each language's own mechanism
  (`toolchain` in `go.mod` via GOTOOLCHAIN, `rust-toolchain.toml` via
  rustup, `.zig-version`, `.fvmrc` for Flutter), into a per-node cache, no
  sudo. Allowed on `shared` nodes (inside the runner account on Windows),
  never on `care` nodes.

## Build order

1. Web Add node and Remove node (in progress).
2. Tier rename and enforcement (`shared|care`), `--admin` grants.
3. Toolchain reporting and `--needs`.
4. Job sandboxes: Linux bwrap first, then the Windows runner account and Job
   Object.
5. `--provision`.
6. Hub offload rule.
7. Busy-GPU placement (after integrated GPU and NPU detection lands).
