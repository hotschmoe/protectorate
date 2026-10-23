# Work on nodes: tiers, jobs, admin, sandboxes, toolchains

Built unless marked **(design)**.

## Principles

- Agents do not SSH into machines and work however they like. They submit
  **jobs through the hub**, which picks the node, checks the capability,
  audits and streams output.
- Shell and files first. Windows desktop control is shelved; see tag `shelf/windows-desk`.
- Anything that can be decided by code is: placement, grants, toolchain
  detection.

## Two tiers

| Tier | Meaning | Jobs and leases | Admin (sudo) | Examples |
|---|---|---|---|---|
| `shared` | part of the shared compute pool | yes, sandboxed | only with a grant | agentworld, officepc, build boxes |
| `care` | a machine we maintain for a family member | never scheduled, never leased | only for a task the owner started | kidsbox |

The tier is recorded in the inventory (`nodes.json`, `hosts.md`) and set with
`node invite --tier`, `node add --tier` or `protectorate node tier <name>
shared|care` (owner only, `NodeService.SetNodeTier`). Enforcement, in the hub:

- Selectors (jobs and leases) never pick a `care` node; the refusal lists it
  as `tier care`.
- Leases never cover a `care` node or one of its GPUs, whoever asks
  (failed_precondition).
- A job naming a `care` node and a file transfer to or from one need the
  owner or a capability with `node_admin:<glob>` matching the node
  (permission_denied otherwise, with the command that mints a grant).
  `node_admin:<glob>` admits no RPC by itself and `*` does not include it.
- A care node runs plain jobs only when its agent was installed with
  `--allow-plain-jobs`.

## Jobs

`protectorate job run --on <node|lease|selector>` runs in a job directory or a
warm git worktree; `--git repo@commit` ships only the missing objects,
`--artifact` ships files. Selection prefers fresh, less loaded nodes and warm
worktrees. A job on a `shared` node runs sandboxed, a `care` node takes jobs
only under an owner's grant, and elevation exists only as `--admin` under a
grant.

A job ends as exited (success or the command's own status), killed, timed
out or canceled. Killed means a signal the node did not send ended the command,
which is how a resource limit or the OOM killer stops it (status 137).
`job run` exits 124 for a timeout, 130 for a cancel and 137 for a kill, so a
script can tell a timeout from a failure. Nothing retries a job on its own. A
timeout or cancel kills the whole process group, process tree (Windows) or
sandbox.

## Admin grants

- `protectorate grant admin --node kidsbox --ttl 4h --task "update kids
  machine" [--out FILE]` (`GrantService.Admin`, tool `grant_admin`) mints a
  capability named `grant:<node>` with scopes `job_*` and `node_admin:<node>`,
  default 4h, at most 24h, recording the task and who minted it (`token ls`
  shows both; `token revoke <id>` ends it early). `token create` refuses
  `node_admin` scopes.
- Who may mint: the owner, for any node or a glob over node names. Envoy
  (it holds `grant_admin`) only for one `shared` node by name, only on a hub
  started with `--orchestrator-admin-grants`, and each mint waits for the
  owner's approval. A grant Envoy minted never opens a `care` node, even if
  the node turns `care` later.
- `job run --admin` needs a named node or lease (not a selector), the owner or
  a grant matching the node, and a node that reports it can elevate: `sudo`
  when `sudo -n true` succeeds (probed once per agent start) or `root` when the
  agent is root. It runs `sudo -n --preserve-env=<the job's variable names> --
  <cmd>`, so values never reach the process list. Windows and Termux nodes
  report no admin jobs.
- Jobs record `mode` (sandboxed, plain, admin), the isolation the node
  reported and the grant's `task` (`job show`). A non-admin job on a `care`
  node runs plain, as the agent's user, because maintaining that machine
  (nmcli, journalctl) needs its real system.
- Every call made with a grant is audited, reads and streams included, with
  the grant's `task`; job lines carry the job id, node, mode and, for grant and
  admin jobs, the command (quoted, cut at 512 bytes; env values never).

Scenario: "update the kids' machine and check its wifi". The owner mints a
grant `job_*` + `node_admin:kidsbox` for 4h. The agent runs read-only checks
as plain jobs (`nmcli -f all dev wifi`, `journalctl -u NetworkManager --since
-1d`), then `job run --on kidsbox --admin -- pacman -Syu --noconfirm`, and
reports in chat. The grant expires by itself.

## Busy GPUs

Some `shared` nodes serve models: `b70-nas-server` usually has a container
holding its two Arc Pro B70s with a loaded LLM. The hub judges each GPU from
the node's own figures over its recent polls (protectorate-node-monitor) and
lists the busy ones in `NodeStatus.busy_gpus` (`node show` prints `gpu N
busy`; the app lights BUSY on that GPU). A GPU is busy when:

- more than half its VRAM is in use (where the driver reports use:
  nvidia-smi, rocm-smi, amdgpu, Windows PDH, xpu-smi), or
- its utilization stayed at or above 60% in every poll of the last 3 minutes
  (at least 3 polls spanning 2 minutes), or
- its temperature stayed at or above 75 C over the same window.

Selector jobs and leases skip a node with a busy discrete GPU and, when they
ask for a GPU, take only one that is not busy. A lease that names a busy GPU or
its node, and a job that names the node, are refused with the reason, the
node's running containers and the line to ask with: `protectorate msg send
orchestrator "please free <node> gpu <n> for ..."` (or `msg send owner`). The
decision to free a GPU is a person's, never the scheduler's. Admin jobs are
not refused. A refused lease is not queued.

On b70-nas-server the xe driver does not tell an unprivileged reader how much
VRAM is in use, so only load and heat count there; installing `xpu-smi` turns
the memory signal on. After a hub restart the load and heat signals need two
minutes of polls.

## Sandboxes on nodes

- **Linux `shared` nodes:** sandboxed jobs run in the same namespace sandbox
  as sleeves ([rust-sandbox.md](rust-sandbox.md)) through
  `protectorate-node-sandbox-helper`, in a systemd user scope with the
  `job run --cpu/--mem` limits: the job directory and warm worktree are the
  only writable binds, the node's job home replaces the real home (so the
  node's key, certificate and `~/.ssh` are not there), the environment is
  cleared, and the network is a private namespace whose egress proxy admits
  only `node serve --egress-allow` entries (none by default). A Linux node
  refuses to start without the helper beside it, and a launch without user
  namespaces or a user systemd manager fails instead of running unsandboxed.
- **Windows `shared` nodes** (often someone's real desktop): jobs run as a
  dedicated standard local account inside a Job Object (CPU and memory caps,
  whole-tree kill on cancel). Provision it once from the agent user's elevated
  interactive session with `protectorate-rs node install-runner --home DIR
  --job-root ABSOLUTE_DIR`; the password is sealed with user-scoped DPAPI.
  Until then the node reports no sandbox and refuses sandboxed jobs.
- **Termux:** no sandbox; plain jobs only with `--allow-plain-jobs`.
- Each node reports its isolation in metrics (`node show` `jobs` line, the
  app's JOBS panel, amber when unisolated), and each job records how it ran.
- Not Docker on Windows: Docker Desktop is a large resident daemon with
  licensing limits that only offers Linux, which Linux nodes already provide.
  Windows nodes exist for Windows-native work.

## Toolchains (design)

- **Not bundled** in our binary: toolchains are large, churn and vary in
  licence.
- **Detected:** each node would report the toolchains it has (`rustc`/`cargo`,
  `go`, `zig`, `node`, `python`, `dart`/`flutter`, MSVC via `vswhere`, `git`,
  with versions), and `job run --needs` would select on them.
- **Provisioned on demand:** a `--provision` flag would let a `shared` node
  install exactly what the repository pins through each language's own
  mechanism into a per-node cache, never on `care` nodes.

## GPUs and NPUs

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

Detection uses only what the OS already exposes:

- **Linux:** PCI display controllers (virtual and BMC adapters skipped),
  platform devices bound to a known SoC GPU or NPU driver, and the accel
  class, all from sysfs. Utilization from amdgpu's `gpu_busy_percent`, devfreq
  `load`, Intel NPU busy time, or the Intel render tile's idle residency.
  VRAM from amdgpu's sysfs and, for xe, the driver's memory-region query.
- **Windows:** Display and ComputeAccelerator devices from SetupAPI, opened
  through D3DKMT; utilization and dedicated VRAM from the PDH "GPU Engine" and
  "GPU Adapter Memory" counters. Software adapters are skipped.
- `nvidia-smi` and `rocm-smi` still report their cards; a sysfs or SetupAPI
  entry of the same vendor is then dropped.

Integrated means a SoC device, a PCI device on the root bus, or (Windows) an
adapter the kernel marks hybrid-integrated; it reports no VRAM. Unknown
utilization is -1 and unknown VRAM use is unset; neither counts as spare
capacity.

Requirements are substring matches on vendor or name, ignoring case:

    job run --on 'gpu:adreno' -- ...
    job run --on 'os=windows,npu:hexagon' -- ...
    lease acquire --gpu battlemage --gpu-vram 24G

With `gpu-vram` the reserved GPU must match `gpu:` as well. A GPU whose VRAM
use is unknown counts its whole VRAM as free; the lease, not the figure,
keeps two holders off one card.

## Not built

Toolchain reporting and `--needs`, `--provision`, a hub offload rule (leave
the hub out of selection while its CPU is saturated), and sleeves on nodes.
