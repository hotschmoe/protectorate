# Work on nodes: tiers, jobs, admin, sandboxes, toolchains

Status: design, agreed in direction on 2026-10-20 and open to revisit. Not
built yet unless marked **(built)**. Gap 1 (jobs on nodes are too powerful) is
closed by the tier rename, care enforcement, admin grants, the Linux and
Windows job sandboxes and busy-GPU placement below; see "Live proof" and
"Rollout" at the end.

## Principles

- Agents do not SSH into machines and work however they like. They submit
  **jobs through the hub**, which picks the node, checks the token, audits and
  streams output. SSH is an escape hatch for interactive needs, through
  short-lived hub-signed certificates (P11), never long-lived keys in sleeves.
- Shell and files first. Windows desktop control is shelved; see tag `shelf/windows-desk`.
- Anything that can be decided by code is: placement, offload, grants,
  toolchain detection.

## Two tiers

| Tier | Meaning | Jobs and leases | Admin (sudo/elevated) | Examples |
|---|---|---|---|---|
| `shared` | part of the shared compute pool | yes, sandboxed | only with a grant | agentworld, officepc, build boxes |
| `care` | a machine we maintain for a family member | never scheduled, never leased | only for a task the owner started | kidsbox |

A `care` node can never be relied on for work: the scheduler and lease
selection skip it, and `--on <care node>` is refused unless the token carries
an explicit grant for that node. "Pool" stays the name of tailnet
reachability groups (`tag:pool-X`), which is a different thing.

**(built)** The tiers were named `server|family` until 2026-10-20 and every
node was enrolled as `server`. They are now `shared|care` everywhere: the
inventory (`nodes.json`, `hosts.md`), the tailnet tags `tag:tier-shared|care`
(the policy owns only these), `node invite --tier`, `node add --tier`, the
`allow_care` request fields and `--allow-care` flags, `node show`, and the
app. `protectorate node tier <name> shared|care` (owner only,
`NodeService.SetNodeTier`) retags the machine through headscale's API, keeping
its other tags and dropping any other `tag:tier-*`, then records the tier;
the hub's own node is always `shared`, and an invited node that has not
joined is re-invited with `--tier` instead. An inventory that still names `server` or
`family` is refused on load with the command that migrates it, `protectorate
node migrate-tiers` (owner-local; see Rollout). Headscale keeps a retired
`tag:tier-server` on a machine after the policy stops owning it, so nodes
keep working until `node tier` retags them.

**(built)** Enforcement, in the hub:

- Selectors (jobs and leases) never pick a `care` node; the refusal lists it
  as `tier care`.
- Leases never cover a `care` node or one of its GPUs, whoever asks
  (failed_precondition).
- A job naming a `care` node (`--on kidsbox`, or a lease on one taken before
  it became `care`) and a file transfer to or from one need the owner token or a token with `node_admin:<glob>` matching the node
  (permission_denied otherwise, with the command that mints a grant). Owner
  only RPCs (`node update`) are unaffected.
- The `allow_care` fields and `--allow-care` flags are gone: the grant is the
  explicit opt-in. `node_admin:<glob>` is a `path.Match` glob over node names
  and admits no RPC by itself; `*` does not include it.

## Jobs (built)

`protectorate job run --on <node|lease|selector>` runs in a job directory or a
warm git worktree; `--git repo@commit` ships only the missing objects,
`--artifact` ships files. Selection prefers fresh, less loaded nodes and warm
worktrees. Until 2026-10-20 every job ran as the node agent's own user,
unsandboxed, so the job scope was shell access on every node the caller could
target, and on `kidsbox` (agent `alarm`, passwordless sudo) effectively root.
Now a job on a `shared` node runs sandboxed where the node can (bwrap on
Linux, the runner account on Windows), a `care` node takes jobs only under an
owner's grant for it, and elevation exists only as `--admin` under a grant.

A job ends as exited (success or the command's own status), killed, timed
out or canceled. Killed means a signal the node did not send ended the command,
which is how a resource limit or the OOM killer stops it (the sandbox helper
reports it as status 137); the error names the signal. `job run` exits 124 for
a timeout, 130 for a cancel and 137 for a kill, so a script can tell a timeout
from a failure and not rerun it under the same limit. Nothing retries a job on
its own. A timeout or cancel kills the command's whole process group (plain
jobs), process tree (Windows) or sandbox.

## Admin grants

- `job run --admin` runs elevated (sudo on Linux, an elevated token on
  Windows), only with a token carrying `node_admin:<node>`.
- Grants are minted per task, short-lived (default 4h), name the node and the
  task, and every command is audited. Nothing has standing root anywhere.
- On `care` nodes a grant comes only from a task the owner started (you asked
  "update the kids' machine"); the orchestrator cannot grant itself admin on
  a `care` node.

**(built)**

- `protectorate grant admin --node kidsbox --ttl 4h --task "update kids
  machine" [--out FILE]` (`GrantService.Admin`, tool `grant_admin`) mints a
  token named `grant:<node>` with scopes `job_*` and `node_admin:<node>`,
  default 4h, at most 24h, recording the task and who minted it (`token ls`
  shows both; `token revoke <id>` ends it early). The secret is printed once,
  or written 0600 to `--out`. `node_admin` scopes exist only on grants:
  `token create` refuses them.
- Who may mint: the owner, for any node or a glob over node names. The
  orchestrator's token (it holds `grant_admin`) only for one `shared` node by
  name, and only on a hub started with `--orchestrator-admin-grants` (off by
  default). A grant the orchestrator minted never opens a `care` node, even
  if the node turns `care` later; only an owner-minted grant does. Anyone
  else is refused whatever their scopes.
- `job run --admin` (`SubmitJobRequest.admin`) needs a named node or lease
  (not a selector), the owner token or a grant matching the node, and a node
  that reports it can elevate (`JobIsolation.admin` in its metrics, `node
  show`): `sudo` when `sudo -n true` succeeds (probed once per agent start;
  it shows in the node's auth log), `root` when the agent is root, and on
  Windows `elevated` only when the agent itself runs elevated. Otherwise the
  submit fails with failed_precondition and the node's reason. Linux runs
  `sudo -n --preserve-env=<the job's variable names> -- <cmd>`, so values
  never reach the process list; Windows runs the command with the agent's
  elevated token in the job object.
- Jobs record `mode` (sandboxed, plain, admin), the `isolation` the node
  reported at start and the grant's `task` (`job show`). A non-admin job on
  a `care` node runs plain: as the agent's user, unsandboxed, because
  maintaining that machine (nmcli, journalctl) needs its real system.
- Audit: every call made with a grant is audited, reads and streams
  included, with the grant's `task` on the line. `SubmitJob` lines carry the
  job id as subject and a detail with the node and mode; admin jobs and jobs
  under a grant also carry the command (quoted, cut at 512 bytes; env values
  never). `audit tail` prints both.

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
The app lights a BUSY annunciator on each such GPU in node cards and detail
(half or more of its VRAM held, or 80 % load) and names the GPU's lease
holder when there is one.

**(built)** The hub judges each GPU from the node's own figures over its
recent polls (`internal/hub/gpubusy.go`) and lists the busy ones in
`NodeStatus.busy_gpus` (`node show` prints `gpu N busy`; the app's
`gpuBusy` reads it, lighting BUSY with the reason on that GPU's lines). A GPU
is busy when:

- more than half its VRAM is in use (the main signal where the driver
  reports use: nvidia-smi, rocm-smi, amdgpu, Windows PDH, xpu-smi), or
- its utilization stayed at or above 60% in every poll of the last 3 minutes
  (at least 3 polls spanning 2 minutes), or
- its temperature stayed at or above 75 C over the same window, for drivers
  whose other figures say little.

On b70-nas-server the xe driver does not tell an unprivileged reader how much
VRAM is in use, so only load and heat count there: a model that is loaded but
idle looks free. Installing `xpu-smi` there turns the memory signal on; until
then the owner can keep the GPUs with a long lease held for the model server.
After a hub restart the load and heat signals need two minutes of polls.

Selector jobs and leases skip a node with a busy discrete GPU (an integrated
GPU that drives a desktop does not count) and, when they ask for a GPU
(`gpu:`, `gpu-vram`), take only a GPU that is not busy. A lease that names a
busy GPU or its node, and a job that names the node, are refused
(failed_precondition) with the reason, the node's running containers (the
usual holders) and the line to ask with: `protectorate msg send orchestrator
"please free <node> gpu <n> for ..."` (or `msg send owner`). Admin jobs are
maintenance and are not refused. A refused lease is not queued: waiting
would decide for the person.

## Sandboxes on nodes

- **Linux `shared` nodes (built):** sandboxed jobs run in the sleeve sandbox:
  `internal/sleeve`'s `Box` and `BoxArgv` build both (the sleeve plan is now a
  box too) and `CheckBind` holds the shared secret-path rules. Each job is
  `systemd-run --user --scope --unit=prot-job-<id>` (with `-p MemoryMax`,
  `MemorySwapMax=0`, `CPUQuota` from `job run --cpu 200 --mem 4G`, i.e.
  `SubmitJobRequest.limits`, and TasksMax 4096) around bwrap with PID, IPC
  and UTS namespaces on a fresh root: the system directories and the narrow
  tool dirs read-only (credential files masked), the job directory and the
  warm worktree the only writable binds, and the node's job home
  (`<job-dir>/home`, kept across jobs for caches) in place of the user's
  real home, so `~/.ssh`, `~/.config`, `$PROTECTORATE_HOME` (node token,
  tailnet keys) and the rest of the home are not there. The environment is
  cleared (the agent's own variables never reach a job); HOME, PATH, USER,
  LANG, TERM=dumb, `GOMODCACHE=~/.cache/go-mod` (because `~/go` is a
  read-only tool dir) and the job's variables are set, values passed on
  fd 3 (`bwrap --args 3`) so they never appear in the process list. The
  network is the host's. A cancel or timeout kills the process group; bwrap's
  `--die-with-parent` and the PID namespace take every descendant with it,
  detached ones included.
- The agent decides once, at its first job or metrics poll: it needs Linux,
  bwrap, systemd-run, systemctl, the user manager's cgroup, a job directory
  outside the secret paths, and a trial `true` in a box that succeeds.
  Otherwise jobs run unsandboxed and the node says why in its metrics
  (`JobIsolation`: `sandbox none` and the reason; `node show` prints a
  `jobs` line, the app a JOBS panel, amber when unisolated). Termux nodes
  report `no bwrap sandbox on android`. Each job's record says how it ran
  (`isolation`: `bwrap, cpu 200%, memory 4096 MiB`, or `none: ...`).
- Admin jobs run unsandboxed by definition (they are root) and plain jobs on
  `care` nodes as the agent's user; the audit detail says `admin (elevated,
  unsandboxed)` or `plain (agent user, unsandboxed)`.
- **Windows `shared` nodes** (often someone's real desktop, with mail,
  browser sessions and BIMcloud under their user): jobs run as a dedicated
  low-privilege local account, `protectorate-runner`, that has no access to
  the user's profile, inside a **Windows Job Object** (CPU and memory caps,
  kill the whole tree on cancel: the Windows equivalent of a cgroup, so P8
  holds). Provision it once from the agent user's elevated interactive
  session with `node install-runner --home DIR --job-root ABSOLUTE_DIR`.
- **Windows Rust runner:** provisioning creates a random password and a new
  standard local account, then protects a separate shared job root with an
  ACL for SYSTEM, Administrators, the agent user and the runner. It seals the
  password with user-scoped DPAPI in `runner-rust.json` under the agent home.
  The node reports `sandbox: none` and rejects sandboxed jobs until this is
  configured. For a shared job, it creates a suspended runner process with
  `CreateProcessWithLogonW`, assigns it to a Job Object with CPU and memory
  limits, then resumes it. The process receives the runner's own environment
  plus the job variables. An isolated officewin trial verified the account,
  working directory, token-file denial and descendant cancellation. The
  native Windows Job Object tests verify CPU and memory settings.
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

## GPUs and NPUs (built)

Every node lists its discrete and integrated GPUs and its NPUs in its
metrics (`node show`, the node card and detail). The fleet is varied on
purpose, for hardware testing:

| Node | GPUs | NPU |
|---|---|---|
| ms-r1 (CIX P1, ACPI) | Mali-G720-Immortalis 10 cores, integrated, `mali` kbase | Zhouyi NPU, `armchina` |
| kidsbox (CIX P1, device tree) | Mali Valhall, integrated, `panthor` | Zhouyi NPU, `armchina` |
| snapdragon-laptop | Qualcomm Adreno X2-90, integrated | Qualcomm Hexagon NPU |
| officepc | Intel Arc Pro B50 (16 GB), Intel Graphics (integrated) | Intel AI Boost |
| b70-nas-server | 2x Intel Arc Pro B70 (Battlemage G31, 32 GB), `xe` | - |
| agentworld (VM) | - | - |

Detection uses only what the OS already exposes, on the slow interval for the
inventory and on every poll for live figures:

- **Linux:** PCI display controllers (virtual and BMC adapters skipped),
  platform devices bound to a known SoC GPU or NPU driver, and the accel
  class, all from sysfs; names from pci.ids or the driver (Mali's `gpuinfo`).
  Utilization from amdgpu's `gpu_busy_percent`, devfreq `load`, Intel NPU
  busy time, or for Intel GPUs the share of time the render tile left its
  idle state (`gtidle`, i915 `rc6_residency_ms`). VRAM from amdgpu's sysfs
  and, for xe, the driver's memory-region query on the render node; xe does
  not tell an unprivileged reader how much is in use, so that stays unknown
  unless `xpu-smi` is installed. Mali and the Zhouyi NPU report no
  utilization; the NPU reports its clock.
- **Windows:** present Display and ComputeAccelerator devices from SetupAPI
  and their driver keys, each opened through D3DKMT for its adapter LUID;
  utilization of GPUs and NPUs alike, and dedicated VRAM in use, from the
  PDH "GPU Engine" and "GPU Adapter Memory" counters keyed by that LUID.
  Software adapters (Parsec, indirect displays) are skipped.
- `nvidia-smi` and `rocm-smi` still report their cards; a sysfs or SetupAPI
  entry of the same vendor is then dropped.

Integrated means a SoC device, a PCI device on the root bus, or (Windows) an
adapter the kernel marks hybrid-integrated; an integrated GPU reports no VRAM.
Unknown utilization is -1 and unknown VRAM use is unset; neither is shown as
spare capacity.

Requirements are substring matches on vendor or name, ignoring case:

    job run --on 'gpu:adreno' -- ...
    job run --on 'os=windows,npu:hexagon' -- ...
    lease acquire --gpu battlemage --gpu-vram 24G

With `gpu-vram` the reserved GPU must match `gpu:` as well. A GPU whose VRAM
use is unknown counts its whole VRAM as free; the lease, not the figure,
keeps two holders off one card.

## Build order

1. Web Add node and Remove node (in progress).
2. Tier rename and enforcement (`shared|care`), `--admin` grants **(built)**.
3. Toolchain reporting and `--needs`.
4. Job sandboxes: Linux bwrap first, then the Windows runner account and Job
   Object **(built; Windows untried on a real machine)**.
5. `--provision`.
6. Hub offload rule.
7. Busy-GPU placement **(built)**.

## Live proof (2026-10-20)

On the hub machine (ms-r1), with the running hub left alone except for
read-only calls:

- Running hub, read-only: every node is still tier `server`; a probe job per
  Linux node found bwrap on ms-r1 and kidsbox but not on agentworld or
  b70-nas-server, systemd-run and a user manager delegating cpu, memory and
  pids everywhere, and `sudo -n` working on ms-r1, agentworld and kidsbox but
  not on b70-nas-server. officepc's agent runs at Medium integrity (not
  elevated), so it will report no admin jobs. b70-nas-server's B70s idled at
  0.02-0.04% busy and 52-55 C over three minutes, VRAM use unknown: not busy.
- A scratch hub on 127.0.0.1:17470 with the new code and two local agents
  (`shared1`, and `carebox` moved with `node tier carebox care`): a `job_*`
  token is refused on carebox (permission_denied) and selectors never place
  there; `lease acquire --node carebox` and `carebox:gpu0` are refused even
  for the owner; a sandboxed job on shared1 with `--cpu 50 --mem 256M` ran in
  `prot-job-<id>.scope` with 5 processes visible, no `/home/hotschmoe`, a clean
  environment and isolation `bwrap, cpu 50%, memory 256 MiB`; `token create
  --scope node_admin:carebox` is refused; `grant admin --node carebox --task
  "update kids machine"` minted `job_* node_admin:carebox` for 4h, under which
  a plain job ran as the agent user (`plain`) and `--admin` ran as uid 0
  through sudo (`isolation sudo`, task recorded); the grant is refused for
  admin on shared1, and `--admin` with a selector is refused; the orchestrator
  token (with `--orchestrator-admin-grants`) minted for shared1 but not for
  carebox or a glob; `token ls` shows the task and minter; the audit lines
  carry the task, the job id, the mode and, for grant and admin jobs, the
  command.

## Rollout

1. Stop the hub and run `protectorate node migrate-tiers` on the hub machine:
   it copies `nodes.json` to `nodes.json.pre-tiers` (refusing if that copy
   exists), rewrites `server` to `shared` and `family` to `care` atomically,
   regenerates `hosts.md` and prints the `node tier` commands for step 3. A
   new hub started on the old file refuses to load and names this command.
   Queued lease tickets and job records written with the old `allowFamily`
   field still load.
2. Start the new hub (`hub serve` as today; add `--orchestrator-admin-grants`
   only if the orchestrator should mint grants for shared nodes), with the web
   client rebuilt (`make app-build-web`) for the CARE, GPU BUSY and JOBS
   views. Respawn the orchestrator (`orchestrator spawn`) so its token gains
   `grant_admin`.
3. Retag every node on the tailnet: `protectorate node tier <name> shared`
   for agentworld, b70-nas-server, officepc and snapdragon-laptop, and
   `protectorate node tier kidsbox care`. Headscale keeps the retired
   `tag:tier-server` until then, so nothing breaks in between.
4. Update the nodes (`node update --all`): the new agents report
   `JobIsolation` (`node show` `jobs` line); until a node runs the new agent,
   admin jobs there are refused.
5. Optional, for sandboxes: install bubblewrap on agentworld and
   b70-nas-server (`sudo apt install bubblewrap`; on Ubuntu 26.04 the package's
   AppArmor profile allows its user namespaces), then restart their agents;
   on officepc (and snapdragon-laptop) run `protectorate node install --runner`
   once from an elevated PowerShell as the user the agent runs as, then sign
   out and in. Until then those nodes say `sandbox none` or `job-object`
   (unisolated).
6. For kidsbox work: `protectorate grant admin --node kidsbox --ttl 4h --task
   "..." --out grant.tok`, then `protectorate --token-file grant.tok job run
   --on kidsbox [--admin] -- ...`.
