# Protectorate

One pane of glass over your coding-agent subscriptions, machines, sessions and
work. A single Go binary is the hub, the node agent and the CLI. Flutter
clients (desktop, Android, web) talk to the same API.

## Start here

| Task | Guide |
|------|-------|
| Install the hub and connect a client | [Install](docs/install.md) |
| Start a sandboxed chat, choose a workspace, inspect model defaults | [Chat](docs/chat.md) |
| Start Envoy or change its scopes | [Orchestrator](docs/orchestrator.md) |
| Send work, acknowledge requests, understand delegation | [Coordination](docs/coordination.md) |
| Understand owner authority and scoped tokens | [Authentication](docs/auth.md) |
| Develop, select a branch, or inspect research references | [Development](DEV.md) |

## Ethos

- **Hands-off toward harnesses:** each LLM runs its own CLI untouched. We never
  wrap, patch, proxy or re-prompt it.
- **Correct, robust and fast everywhere else:** the hub, sleeves, networking
  and nodes are engineered as seriously as they need to be.
- **Everything is drivable from the CLI** (`--json` everywhere), so MCP bridges
  and an orchestrator agent can drive it too.

## What works

| Area | Commands | Notes |
|------|----------|-------|
| Subscription quota (P1) | `quota [--refresh]`, `quota suggest` | Claude, Codex, Grok, Antigravity, local LLM; resets, reset credits, projected unused; never refreshes tokens or redeems resets |
| Sessions (P4) | `session ls/new/attach/capture/kill` | tmux on a dedicated socket; WebSocket terminal at `/ws/terminal/{name}` |
| Sleeves (P7, P8) | `sleeve spawn/ls/workspaces/show/attach/kill` | bwrap + systemd user scope: PID isolation, CPU/memory caps, shared live credentials |
| Chat (`docs/chat.md`) | `chat watch/send/interrupt/respond/compact`, `sleeve spawn --mode` | a sleeve's harness driven through its headless protocol inside the sandbox, with a replayable event log any number of clients can watch |
| Coordination (`docs/coordination.md`) | `who`, `msg send/reply/ack/inbox/thread`, `lease acquire --wait` | per-sleeve identity, a project directory from git remotes and manifests, a mailbox delivered into idle chats, a lease queue, dependency notices; all deterministic hub code |
| Nodes (P2, P3, P9) | `node ls/show/add/rm/serve/metrics` | CPU, discrete and integrated GPUs, NPUs, memory, every filesystem, Docker disk, per-container and per-sleeve usage |
| Tailnet | `hub tailnet install/init`, `tailnet status`, `node invite/join/revoke`, `client invite` | embedded tsnet + supervised Headscale; in-process DERP relay over TLS with a pinned self-signed cert; WhoIs gating; verified direct and relay-only with a Windows node |
| Jobs (P3) | `job run/ls/show/logs/cancel/test-cross`, `lease acquire/ls/renew/release` | run commands on idle nodes with warm git worktrees; idempotent submit; renewed short leases; Windows process-tree kill |
| Files and desktop (P11, `docs/desk.md`) | `node put/get`, `desk <node> screenshot/windows/click/move/type/key/run` | zstd-adaptive, sha256-verified, atomic file copies confined to a node file root; Windows screen and input from the logged-in session behind a `desk:<node>` lease |
| Orchestrator | `orchestrator spawn/status` | an agent in a sleeve ("envoy") with a scoped token, driving the hub through the CLI |
| Notes (backlog) | `backlog add/ls/show/edit/rm/pin` | the owner's notes, kept on the hub, editable in the app and by the orchestrator |
| Auth | `token create/ls/revoke`, `audit tail` | scoped, expiring tokens at every entry point; unsandboxed shells (raw sessions, host terminals) are owner-only unless granted `terminal:host`; append-only audit log |
| Clients | `pair`, `mcp`, `hub serve --web` | QR pairing with a scoped device token; MCP tools generated from the API |
| Ops | `service systemd/windows-task/install-windows/termux` | generates service definitions, never installs them |
| Nodes | `node install/uninstall/doctor/update` | one-command node setup per OS, dry-run first; uninstall deletes nothing; update pushes a build, swaps it with one rollback binary |
| Add and remove nodes (`docs/install.md`) | app ADD NODE / REMOVE NODE, `node invite`, `node rm [--jobs wait\|cancel] [--uninstall]` | the hub serves `/install.sh`, `/install.ps1` and sha256-checked builds under `/dl/`; a single-use key from the environment, never a URL; live INVITED, JOINED, FIRST METRICS, READY; removal drains, revokes the tailnet enrollment and deletes nothing on the machine |

## Install

Hub, on Linux (x86_64 or aarch64), per user, no sudo; re-run to upgrade:

```
curl -fsSL https://raw.githubusercontent.com/hotschmoe/protectorate/v0.4.0/install-hub.sh | bash
```

It verifies the release against `SHA256SUMS`, lists missing prerequisites (bwrap, tmux, git, systemd user manager) instead of installing them, and enables a `protectorate-hub` user unit. Nodes join from the web app (NODES, ADD NODE). Details in [docs/install.md](docs/install.md); branch and release rules in [DEV.md](DEV.md).

## Build

Contributors: read [DEV.md](DEV.md) first. Development starts from
`dev-hotschmoe`; a fresh clone normally starts on the release branch, `master`.

```
make gen        # buf generate -> gen/ (Go) and app/lib/gen/ (Dart), both committed
make check-gen  # fails if generated code is stale
make build      # bin/protectorate
make test       # tagged vet, fuzz seeds and real-binary e2e
make cross      # linux/windows amd64+arm64, android/arm64 into bin/
make app-check  # flutter analyze + test
```

## Run

```
protectorate hub serve [--web app/build/web]   # --web defaults to ~/.local/share/protectorate/web when installed; owner token at ~/.protectorate/token
protectorate login codex                        # once per harness, on the hub; --status lists them
protectorate quota
protectorate sleeve spawn alice --workspace https://github.com/owner/project.git --harness codex --mem 8G
protectorate chat send alice "Read the repository instructions and report the current state."
protectorate chat watch alice
protectorate pair                               # QR code for the phone app
```

Replace the example URL with a repository you can read. A clone is placed at
`$PROTECTORATE_HOME/workspaces/alice` on the hub. An existing hub directory can
also be selected; the sleeve edits it directly. See [workspace safety](docs/chat.md#workspaces)
before selecting a checkout. Raw `session new` commands run outside the sleeve
sandbox and require owner authority.

## Docs

- [PROBLEMS.md](PROBLEMS.md): what hurts and why
- [ARCHITECTURE.md](ARCHITECTURE.md): the system and the decisions behind it
- [docs/install.md](docs/install.md): hub, node and phone setup
- [docs/node-work.md](docs/node-work.md) (tiers, jobs, admin grants, sandboxes, toolchains: design), [docs/desk.md](docs/desk.md), [docs/auth.md](docs/auth.md), [docs/mcp.md](docs/mcp.md), [docs/backlog.md](docs/backlog.md), [docs/coordination.md](docs/coordination.md)
- `spikes/`: the experiments behind each decision, with measurements
