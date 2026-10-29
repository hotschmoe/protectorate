# Architecture

Read `PROBLEMS.md` first; this document describes the system built to solve
those problems. Decisions and their history: [the Rust direction
record](docs/2026-10-24-rust-direction.md).

Guiding rules:
- **Hands-off toward the harness.** Each LLM runs its own CLI untouched, driven
  only through its TUI or its official headless protocol, with its own config
  and auth. We never wrap, patch, proxy or re-prompt it.
- **Correct, robust and fast everywhere else.** The hub, sleeves and API
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
                                    | Connect JSON over loopback HTTP, or
                                    | HTTPS through the reverse proxy
                                    | + WebSocket terminals
                       +------------v-------------+
                       | HUB  protectorate-rs     |  quota, sleeves, chat,
                       | serve                    |  sessions, mail, audit
                       +------------+-------------+
                                    |
                   sleeves on the hub machine
                   (namespace sandbox, one harness per workspace)
```

The hub is a host binary, not a container: tmux and sandboxes are simpler
on the host, and a container holding the Docker socket isolates
nothing. Everything runs on that one machine.

## Binaries

| Binary | Crate | Role |
|---|---|---|
| `protectorate-rs` | protectorate-server | `serve` (the hub), `hub backup/restore/publish-*`, `api call` |
| `protectorate` | protectorate-cli | every client command (`protectorate --help`); forwards `hub ...` to the server and `mcp` to the bridge. Mounted at `/opt/protectorate/bin/protectorate` inside sleeves |
| `protectorate-mcpbridge` | protectorate-mcpbridge | MCP tools over stdio, loopback HTTP or HTTPS ([mcp](docs/mcp.md)) |
| `protectorate-sleeve-runner` | protectorate-sleeve-runner | tmux pane process of a terminal sleeve; owns its sandbox for the session |
| `protectorate-sleeve-sandbox-helper` | protectorate-sleeve-runner | the re-exec helper that builds namespaces and execs the workload |

The Linux release bundle holds all five for amd64 and arm64; the server
refuses to serve without the runner and sleeve helper beside it.

## Crates

`rust/crates/`, one Cargo workspace (29 crates):

| Crate | Purpose |
|---|---|
| audit | monthly hash-chained JSONL audit log |
| auth | credentials, scopes and grant decisions shared by hub, CLI and terminal |
| backlog | `notes.json` store |
| backup | verified `.pbackup` snapshots and restore |
| chatlog | per-sleeve append-only chat events, replay and live watch |
| claude-driver, codex-driver, grok-driver, pi-driver | one harness's headless protocol inside a caller-supplied sandbox |
| cli | the client CLI |
| connect-client | blocking Connect JSON client (CLI, MCP bridge, `api call`) |
| directory | workspace discovery, project dependency graph, release observations |
| fsutil | crash-safe file replacement |
| fuzz | fuzz targets; `rust/fuzz` runs them under libFuzzer |
| handoff | resleeve facts, native transcripts, confined handoff files |
| hub | the hub's Connect services over `PROTECTORATE_HOME` |
| identity | devices and capabilities the hub issued |
| mailbox | append-only JSONL message store |
| mcpbridge | MCP bridge |
| pairing | pairing links and terminal QR |
| protogen | writes the prost message types and typed procedure constants from the descriptor set |
| quota | provider quota reads, caching and harness ranking |
| release | GitHub release metadata, verified hub bundles, web archive extraction |
| sandbox | validated Linux sandbox plans, launcher and egress proxy |
| server | `protectorate-rs` entry points |
| session | tmux sessions |
| sleeve-registry | durable sleeve lifecycle records |
| sleeve-runner | terminal sleeve runner and sandbox helper |
| wire | descriptor-driven protobuf JSON and Connect unary endpoints, the private-address listener rule |

## API

`proto/protectorate/v1/*.proto` is the only API definition (14 services, 65
RPCs). `make gen` runs buf, writes the Dart client to `app/lib/gen` and the
descriptor sets to `rust/crates/protectorate-wire/assets/`, and
`protectorate-protogen` writes prost message types and typed procedure
constants to `protectorate-wire/src/v1.rs` from that descriptor. The hub serves Connect's JSON protocol over HTTP/1.1
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
  break-glass owner token) and on each `--proxy-listen` for the reverse proxy
  that terminates TLS with a public certificate (Traefik on stronghold in
  production). A proxy listener closes every connection that does not come
  from a `--trusted-proxy` address and never accepts the owner token.
- Devices (app, remote CLI) are named bearer tokens (`pd1.`) that the owner or
  another device adds; the client keeps the token, the hub only its SHA-256.
  Revoking a device closes its streams and terminals. No `?token=`.
- Sleeves, Envoy and scripts hold hub-minted HMAC capabilities
  (`pc1.` macaroons): scoped, about one hour, renewable, attenuable offline,
  revocable by lineage. Delegated mints never exceed the minter's scopes.

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
- **Ownership:** one sleeve per directory. A launch is refused when its
  workspace is, contains or lies inside another sleeve's, chat and terminal
  alike; an exited sleeve keeps its directory until it is killed. Sub-sleeves
  get their own branch clone.
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

## Coordination and mail

Deterministic hub code, no model ([coordination](docs/coordination.md)).
Each sleeve's capability is its identity, so senders are authenticated. The directory derives projects and dependency edges from git
remotes and manifests. The mailbox is an append-only JSONL log; mail reaches
an idle chat sleeve as an ordinary user message, unacked requests are
re-delivered once and then escalated to Envoy. Tags and default-branch
advances become notices to dependent projects.

Envoy, the orchestrator, is an ordinary chat sleeve (`sleeve envoy`) whose
tool is the CLI with a capability ([orchestrator](docs/orchestrator.md)).
External MCP clients use the same capability path.

## Governance

- **Audit:** every mutating RPC, denial, terminal attach goes to
  `audit/audit-YYYY-MM.jsonl` as a hash chain (`seq`, `prev`, `hash`, head in
  `audit/head.json`); `audit verify` walks it.
- **run_id:** every sleeve run gets one id carried by its audit entries,
  mail, approvals and checkpoints; `audit ls --run` follows a run.
- **Approvals:** destructive RPCs and pin changes
  requested by an agent fail with `aborted` and open a request; the owner
  approves on a device or with `approvals approve`, and the identical call
  then runs once.

## Hang detection and restarts

A hung hub (deadlock, wedged runtime, stuck blocking pool) is
restarted by its service manager, which only reacts to exits, so the process
kills itself. `serve` runs a watchdog thread (`protectorate-server/src/
watchdog.rs`) that expects progress from: a heartbeat task on the main
runtime, a `spawn_blocking` round trip, and on the hub the identity,
sleeve-registry, mailbox and directory locks (each probed by its own
thread taking the lock and releasing it). A
probe that has not completed for 90 s (`PROTECTORATE_WATCHDOG_TIMEOUT`, for
example `5s`, only shortens the bound for tests) makes it write the stalled
probe and a thread list to stderr and `_exit(70)`, with no destructor that
could take the stuck lock. 90 s is 60 times the 1.5 s fsync tail seen on the
dev host. A late watchdog wake-up (SIGSTOP, debugger, frozen cgroup) is
treated as a freeze and forgiven, not as a hang. Status 70 is a failure
everywhere: systemd `Restart=on-failure` (`StartLimitIntervalSec=0`, so no run
of restarts can leave the unit stopped). systemd `WatchdogSec=` is not used: it adds
`Type=notify`, startup and update-trial handshakes across the guardian and its
child, and a second hang definition, for no case the in-process watchdog misses
(a process stuck in uninterruptible I/O cannot be killed by either).

## Quota (P1)

Read-only readers for Claude, Codex, Grok, Antigravity (local `agy` only) and
the local LLM; they never refresh tokens or redeem resets. `quota suggest`
ranks harnesses by fresh spare capacity.

## Releases, updates and backups

- GitHub releases carry the Linux amd64 and arm64 hub bundles, the web
  archive, the Windows desktop zip, `VERSION` and `SHA256SUMS`. Stable is the
  latest release, dev the rolling `dev` prerelease ([DEV](DEV.md)).
- The installer and the hub verify every asset against `SHA256SUMS` and stage it
  under `releases/<version>/` with atomic `current` and `previous` links.
- `hub update` (`ReleaseService.UpdateHub`, owner or device) downloads and
  verifies the host's hub bundle and the web archive, writes a trial marker and restarts; the next start confirms the
  trial or restores the previous release.
- `hub backup` writes a verified `.pbackup` of state (identity,
  sleeves, directory, mail, notes, audit, handoffs, chat logs; no workspaces, credentials or binaries); `serve --backup-dir
  --backup-every` schedules it; `hub restore` fills an empty home.

## State

Plain files under `$PROTECTORATE_HOME` (default `~/.protectorate`), atomic
replacement, mode 0600:

```
token            owner break-glass token (loopback only)
identity.json    capability key, device token hashes, lineages
sleeves.json     sleeve records; sleeves/<name>/ homes and chat.jsonl
directory.json  mailbox.jsonl  notes.json
audit/           audit chain
handoffs/        archived HANDOFF.md per resleeve
helpers/         checkpoint stores (not backed up)
workspaces/      managed clones; orchestrator/ is Envoy's workspace
```

## Flutter client

`app/` is one Flutter codebase for desktop, Android and web (served by the
hub from the release's web archive). It talks Connect JSON through the
generated Dart client in `app/lib/gen` and sends its device token with each
request. Desktop and web share a responsive shell (overview, sleeves and chat,
terminals, quota, mail, notes, devices, audit); Android is Envoy-first
(ENVOY, QUOTAS, DEVICES) with an emergency stop that kills every sleeve
directly. Visual design: [design](docs/design.md).
