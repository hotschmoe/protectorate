# Architecture

Read `PROBLEMS.md` first; this document describes the system built to solve
those problems. Decisions and their history: [the Rust direction
record](docs/2026-10-24-rust-direction.md).

Guiding rules:
- **Hands-off toward the harness.** Each LLM runs its own CLI untouched, driven
  only through its TUI or its official headless protocol, with its own config
  and auth. We never wrap, patch, proxy or re-prompt it.
- **Correct, robust and fast everywhere else.** The hub, sleeves, nodes and API
  may be as sophisticated as correctness requires; "less code" is not a reason
  to pick a weaker design there. Speculative abstractions are.

## Shape

```
 CLIENTS (one hub API)
 +-----------+ +----------------------+ +------------+ +--------------------+
 | CLI       | | Flutter app          | | MCP bridge | | Envoy: a sleeve    |
 | (--json)  | | desktop, Android, web| |            | | driving the CLI    |
 +-----+-----+ +----------+-----------+ +-----+------+ +---------+----------+
       +------------------+---------+---------+------------------+
                                    | Connect JSON over HTTPS (hub CA)
                                    | + WebSocket terminals
                       +------------v-------------+
                       | HUB  protectorate-rs     |  quota, sleeves, chat,
                       | serve                    |  sessions, nodes, leases,
                       +----+----------------+----+  jobs, mail, audit
                            |                |  mutual TLS, hub dials node :7475
           sleeves on the hub machine     +--v------------------------------+
           (namespace sandbox, one        | node agent: Linux, Windows,     |
            harness per workspace)        | Termux (no sleeves off the hub) |
                                          +---------------------------------+
```

The hub is a host binary, not a container: tmux, sandboxes, host metrics and
the CA are simpler on the host, and a container holding the Docker socket
isolates nothing.

## Binaries

| Binary | Crate | Role |
|---|---|---|
| `protectorate-rs` | protectorate-server | `serve` (the hub), `node join/install/serve/doctor/uninstall`, `hub backup/restore/publish-*`, `api call` |
| `protectorate` | protectorate-cli | every client command (`protectorate --help`); forwards `hub ...` to the server and `mcp` to the bridge. Mounted at `/opt/protectorate/bin/protectorate` inside sleeves |
| `protectorate-mcpbridge` | protectorate-mcpbridge | MCP tools over stdio, loopback HTTP or HTTPS ([mcp](docs/mcp.md)) |
| `protectorate-sleeve-runner` | protectorate-sleeve-runner | tmux pane process of a terminal sleeve; owns its sandbox for the session |
| `protectorate-sleeve-sandbox-helper`, `protectorate-node-sandbox-helper` | protectorate-sleeve-runner, protectorate-node | the re-exec helper that builds namespaces and execs the workload |
| `protectorate-rs-windows` | protectorate-node-windows | Windows node, shipped as `protectorate-rs-windows-<arch>.exe` and installed as `protectorate-rs.exe` |
| `protectorate-rs-termux` | protectorate-node-termux | Android/Termux node, shipped as `protectorate-rs-android-arm64` |

The Linux release bundle holds the first six for amd64 and arm64; the server
refuses to serve without the runner and sleeve helper beside it.

## Crates

`rust/crates/`, one Cargo workspace (40 crates):

| Crate | Purpose |
|---|---|
| audit | monthly hash-chained JSONL audit log |
| auth | credentials, scopes and grant decisions shared by hub, CLI and terminal |
| backlog | `notes.json` store |
| backup | verified `.pbackup` snapshots and restore |
| chatlog | per-sleeve append-only chat events, replay and live watch |
| claude-driver, codex-driver, grok-driver, pi-driver | one harness's headless protocol inside a caller-supplied sandbox |
| cli | the client CLI |
| connect-client | blocking Connect JSON client (CLI, hub-to-node over mutual TLS) |
| directory | workspace discovery, project dependency graph, release observations |
| fsutil | crash-safe file replacement |
| fuzz | fuzz targets; `rust/fuzz` runs them under libFuzzer |
| handoff | resleeve facts, native transcripts, confined handoff files |
| hub | the hub's Connect services over `PROTECTORATE_HOME` |
| identity | devices and capabilities the hub issued |
| inventory | node inventory (`nodes.json`, `hosts.md`) |
| lease | persistent leases and priority/FIFO tickets |
| mailbox | append-only JSONL message store |
| mcpbridge | MCP bridge |
| node | node agent: metrics, file transfer, jobs, updates behind mutual TLS |
| node-client | verified file push and pull |
| node-metrics | CPU, GPU, NPU, memory, filesystem probes (Linux, Windows) |
| node-monitor | polls nodes, keeps last metrics for status and placement |
| node-termux, node-windows | platform node executables |
| pairing | pairing links and terminal QR |
| quota | provider quota reads, caching and harness ranking |
| release | GitHub release metadata, verified bundles, web archive extraction |
| sandbox | validated Linux sandbox plans, launcher and egress proxy |
| server | `protectorate-rs` entry points |
| session | tmux sessions |
| sleeve-layout | unused (sleeve homes are prepared by hub and sleeve-runner); pending deletion |
| sleeve-registry | durable sleeve lifecycle records |
| sleeve-runner | terminal sleeve runner and sandbox helper |
| svcgen | systemd, Windows task and Termux service definitions |
| tls | hub CA, node certificates, private-address listener rule |
| wire | descriptor-driven protobuf JSON and Connect unary endpoints |
| xfer | bounded, verified file and directory transfer archives |

## API

`proto/protectorate/v1/*.proto` is the only API definition (19 services, 100
RPCs). `make gen` runs buf, writes the Dart client to `app/lib/gen` and the
descriptor sets to `rust/crates/protectorate-wire/assets/`; the Rust side has
no generated code. The hub serves Connect's JSON protocol over HTTP/1.1
straight from the embedded descriptor, so `curl -d '{}'` against
`/protectorate.v1.HubService/GetStatus` works.

- RPC, not REST: each method maps to a CLI command and an MCP tool
  (`SleeveService.SpawnSleeve` = `sleeve spawn` = `sleeve_spawn_sleeve`).
- Event streams are server-streaming RPCs. `--json` prints proto-JSON as is.
- Proto options carry policy: `NO_SIDE_EFFECTS` (the `read` scope),
  `owner_only_rpc`, `device_allowed`, `destructive` (approval gates, MCP hints).
- Exception: interactive terminals use a binary WebSocket,
  `/ws/terminal/{name}`, attached to tmux.

## Network and identity

Protectorate builds no network and never listens on the internet. The routers
own connectivity (site-to-site WireGuard between sites, phones as road
warriors); protectorate authenticates every peer itself, because network
location is not identity. Details: [install](docs/install.md#network-model),
[auth](docs/auth.md).

- Every listener binds an explicit private address (loopback, RFC 1918,
  100.64.0.0/10, fc00::/7, fe80::/10); wildcard and public addresses are refused.
- The hub serves plain HTTP on loopback `--listen` (local tools, sleeves, the
  break-glass owner token) and HTTPS on each `--tls-listen` with a certificate
  from its own CA (`HOME/ca/`).
- Devices (app, remote CLI) pair through owner approval and sign every request
  with a P-256 key that never leaves them (Android Keystore, non-extractable
  WebCrypto, 0600 key file). No bearer tokens on clients, no `?token=`.
- Sleeves, Envoy, admin grants and scripts hold hub-minted HMAC capabilities
  (`pc1.` macaroons): scoped, about one hour, renewable, attenuable offline,
  revocable by lineage. Delegated mints never exceed the minter's scopes.
- Nodes join with a single-use code, get a certificate from the hub CA, and
  accept only the hub's client certificate. The hub dials nodes (never the
  reverse) on 7475 at the address the node reported. Node-initiated
  connections are a recorded future option.

## Sleeves

A sleeve is one workspace plus one harness session in a sandbox, so agents
cannot see or kill each other's processes (P8). Sleeves run only on the hub
machine. Details: [sleeves](docs/sleeves.md), [sandbox](docs/rust-sandbox.md),
[chat](docs/chat.md).

- **Sandbox:** a re-exec helper builds user, mount, PID, IPC, UTS and network
  namespaces on a fresh tmpfs root inside a `systemd-run --user --scope` with
  CPU, memory and task limits. Writable: the workspace and a private home.
  Read-only: system and narrow tool directories, named references. The
  harness's credential directory is shared live, never copied (one canonical
  login per provider on the hub; Claude's symlinked `.credentials.json`,
  Codex and Grok symlinked `auth.json`, Pi `PI_CODING_AGENT_DIR`). The home
  is seeded once with the owner's global git name and email. `--nested`
  delegates a cgroup subtree for runtime tests. No bwrap.
- **Egress:** the namespace has only loopback. A host-side CONNECT proxy per
  sandbox admits the hub, the provider's API hosts, `--egress-allow` entries
  and local model hosts; private, metadata and reserved ranges are refused and
  every decision is audited.
- **Modes:** chat (default) runs the harness's headless protocol and normalizes
  it into a replayable chat log; terminal runs the TUI under tmux
  (`protectorate-sleeve-runner`), with screen-rule status (working, blocked,
  idle). A sleeve keeps its mode for life.
- **Checkpoints:** parentless git commits of the workspace per settled turn,
  before resleeve and on demand, in a private store the sleeve cannot see;
  git runs only in a sandbox.
- **Disk limits:** `--disk` is enforced by a 10-second sampler that stops the
  sleeve at 100%.
- **Pinning:** each launch records the SHA-256 of the harness executable and
  of its profile; a change requested by an agent waits for owner approval.
- **Budgets:** `--budget` caps a chat run in cache-weighted tokens; children
  draw from the parent's remainder.
- **Resleeve** moves a chat sleeve to another harness: facts from git and the
  native transcript, a summary from a spare model, `.protectorate/HANDOFF.md`,
  one plain user message. The outgoing harness is never asked. A resleeved
  sleeve keeps its original spawner's scopes.
- **What reaches a harness:** the brief, the resleeve sentence, mail turns and
  seeded config, nothing volatile early in the prompt, so native prompt caches
  stay warm. Cache-read telemetry flags a cold first turn.

## Harness drivers

| Harness | Protocol | Autonomous mode inside a sleeve |
|---|---|---|
| Claude | `claude -p` stream-json, `--permission-prompt-tool stdio` | `--permission-mode bypassPermissions` |
| Codex | `codex app-server` over stdio | sleeve-owned `config.toml`, `approval_policy = "never"` |
| Grok | `grok agent stdio` (ACP) | `/always-approve on` |
| Pi | `pi --mode rpc` | no gate |

`--supervised` keeps the harness's asking mode so approvals reach
`ChatService.Respond`. Native compaction keeps each harness's defaults. One
owner per native session id.

## Nodes, leases and jobs

Nodes report metrics (CPU, GPUs, NPUs, memory, filesystems, containers) to the
hub's poller (P2, P9). Two tiers ([node work](docs/node-work.md)):
`shared` (the pool: leased and scheduled, sandboxed jobs) and `care` (a family
machine: never scheduled or leased; work only under an owner-minted
`node_admin` grant).

- **Leases** `(resource, holder, ttl)` in `leases.json`, swept every second,
  exclusive, at most 24h; an optional FIFO queue with priorities.
- **Jobs** (P3): the hub places by node, lease or selector (os, arch, free
  memory, GPU/NPU name and VRAM, no busy GPU, least loaded, warm worktree
  preferred), ships only missing git objects as a thin bundle (or an artifact
  file or directory), and the node runs the command in a warm worktree.
  Submit is idempotent by request id. Nodes keep output and results for an
  hour, so a restarted hub reattaches. `job test-cross` runs `cargo test` on a
  matching node.
- **Job isolation:** Linux nodes run sandboxed jobs in the same namespace
  sandbox; Windows nodes as a separate runner account in a Job Object; plain
  jobs (care nodes) need `--allow-plain-jobs`; `--admin` runs elevated
  (sudo or root, Linux nodes only) under a grant. Every job records how it ran.
- **Files:** `node put/get`, sha256-verified, atomic, confined to the node's
  file root (P11). Windows desktop control is shelved (tag `shelf/windows-desk`).

## Coordination and mail

Deterministic hub code, no model ([coordination](docs/coordination.md)).
Each sleeve's capability is its identity, so senders and lease holders are
authenticated. The directory derives projects and dependency edges from git
remotes and manifests. The mailbox is an append-only JSONL log; mail reaches
an idle chat sleeve as an ordinary user message, unacked requests are
re-delivered once and then escalated to Envoy. Tags and default-branch
advances become notices to dependent projects.

Envoy, the orchestrator, is an ordinary chat sleeve (`sleeve envoy`) whose
tool is the CLI with a capability ([orchestrator](docs/orchestrator.md)).
External MCP clients use the same capability path.

## Governance

- **Audit:** every mutating RPC, denial, terminal attach and grant call goes to
  `audit/audit-YYYY-MM.jsonl` as a hash chain (`seq`, `prev`, `hash`, head in
  `audit/head.json`); `audit verify` walks it.
- **run_id:** every sleeve run gets one id carried by its audit entries, jobs,
  leases, mail, approvals and checkpoints; `audit ls --run` follows a run.
- **Approvals:** destructive RPCs, admin grants and jobs, and pin changes
  requested by an agent fail with `aborted` and open a request; the owner
  approves on a device or with `approvals approve`, and the identical call
  then runs once.

## Quota (P1)

Read-only readers for Claude, Codex, Grok, Antigravity (local `agy` only) and
the local LLM; they never refresh tokens or redeem resets. `quota suggest`
ranks harnesses by fresh spare capacity.

## Releases, updates and backups

- GitHub releases carry the Linux bundles, Windows and Termux nodes, the web
  archive, the Windows desktop zip, `VERSION` and `SHA256SUMS`. Stable is the
  latest release, dev the rolling `dev` prerelease ([DEV](DEV.md)).
- Installers and the hub verify every asset against `SHA256SUMS` and stage it
  under `releases/<version>/` with atomic `current` and `previous` links.
- `hub update` (`ReleaseService.UpdateHub`, owner or device) verifies the whole
  fleet release, writes a trial marker and restarts; the next start confirms
  the trial or restores the previous release. `node update` pushes the hub's
  release to a node, which keeps one rollback binary; Windows nodes update
  through a guardian copy of the executable.
- `hub backup` writes a verified `.pbackup` of state (identity, CA, inventory,
  sleeves, leases, directory, mail, notes, audit, job records, handoffs, chat
  logs; no workspaces, credentials or binaries); `serve --backup-dir
  --backup-every` schedules it; `hub restore` fills an empty home.

## State

Plain files under `$PROTECTORATE_HOME` (default `~/.protectorate`), atomic
replacement, mode 0600:

```
token            owner break-glass token (loopback only)
ca/              hub CA and key
identity.json    capability key, devices, lineages
nodes.json       inventory; hosts.md is generated from it
sleeves.json     sleeve records; sleeves/<name>/ homes and chat.jsonl
leases.json  directory.json  mailbox.jsonl  notes.json
jobs/            job records, logs, artifacts
audit/           audit chain
handoffs/        archived HANDOFF.md per resleeve
helpers/         checkpoint stores (not backed up)
workspaces/      managed clones; orchestrator/ is Envoy's workspace
```

A node keeps `node.json` and `tls/` in its own home.

## Flutter client

`app/` is one Flutter codebase for desktop, Android and web (served by the
hub from the release's web archive). It talks Connect JSON through the
generated Dart client in `app/lib/gen` and signs each request with its device
key. Desktop and web share a responsive shell (overview, sleeves and chat,
terminals, nodes, quota, mail, notes, devices, audit); Android is Envoy-first
(ENVOY, QUOTAS, NODES, DEVICES) with an emergency stop that kills every sleeve
directly. Visual design: [design](docs/design.md).
