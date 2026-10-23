<img src="docs/logo.svg" alt="" width="96">

# Protectorate

One pane of glass over your coding-agent subscriptions, machines, sessions and
work. A Rust hub, node agent, CLI and MCP bridge; a Flutter client for
desktop, Android and web.

## Start here

| Task | Guide |
|------|-------|
| Install the hub and nodes, connect a client | [Install](docs/install.md) |
| Start a sandboxed chat, choose a workspace, inspect model defaults | [Chat](docs/chat.md) |
| Start Envoy or change its scopes | [Orchestrator](docs/orchestrator.md) |
| Send work, acknowledge requests, understand delegation | [Coordination](docs/coordination.md) |
| Understand devices, capabilities, approvals and audit | [Authentication](docs/auth.md) |
| Develop, test, release | [Development](DEV.md), [Testing](docs/testing.md) |

## Ethos

- **Hands-off toward harnesses:** each LLM runs its own CLI untouched. We never
  wrap, patch, proxy or re-prompt it.
- **Correct, robust and fast everywhere else:** the hub, sleeves, networking
  and nodes are engineered as seriously as they need to be.
- **Everything is drivable from the CLI** (`--json` everywhere), so MCP bridges
  and an orchestrator agent can drive it too.

## What it does

| Area | Commands |
|------|----------|
| Subscription quota (P1) | `quota [--refresh]`, `quota suggest` |
| Sleeves (P7, P8): a harness in a namespace sandbox with egress allowlist, checkpoints, disk limits, budgets | `sleeve spawn/ls/show/attach/kill/resleeve/checkpoint/prune` |
| Chat ([chat](docs/chat.md)) | `chat send/watch/wait/interrupt/respond/compact/status` |
| Sessions (P4) | `session ls/new/attach/capture/kill` (owner only, unsandboxed) |
| Coordination ([coordination](docs/coordination.md)) | `who`, `msg send/reply/ack/inbox/thread/wait`, `lease acquire --wait` |
| Nodes (P2, P9) | `node invite/ls/show/add/rm/tier/update/put/get`, app ADD NODE |
| Jobs (P3) | `job run/ls/show/logs/cancel/test-cross`, `lease acquire/ls/renew/release`, `grant admin` |
| Orchestrator | `sleeve envoy`, `orchestrator status` |
| Notes | `backlog add/ls/show/edit/done/rm/pin` |
| Auth and audit | `login`, `pair`, `devices`, `token`, `approvals`, `audit ls/tail/verify` |
| Ops | `hub update`, `hub backup/restore`, `service systemd/windows-task/install-windows/termux`, `node doctor` |
| MCP ([mcp](docs/mcp.md)) | `mcp [--allow GLOB] [--http ADDR]` |

Windows desktop control is shelved (tag `shelf/windows-desk`).

## Install

Hub (Linux amd64 or arm64, per user, no sudo):

```
curl -fsSLO https://raw.githubusercontent.com/hotschmoe/protectorate/master/install-hub.sh
bash install-hub.sh --tls-listen 192.168.22.5:7470 --tls-name stronghold.lan
```

Then add nodes from the app (NODES, ADD NODE) or `protectorate node invite
NAME`, and pair the phone with `protectorate pair --app`. The
[install guide](docs/install.md) covers the network model (private addresses,
router WireGuard, hub CA and mutual TLS), Windows and Termux nodes, and the
recommended router firewall.

The CLI is `protectorate-cli` in the release directory
(`~/.local/share/protectorate/releases/current/`); the docs call it
`protectorate`, which is also its name inside sleeves.

## Build

Read [DEV.md](DEV.md) first.

```
make gen        # buf generate: Dart client and the Rust descriptor set, both committed
make check-gen  # fails if generated code is stale
make build      # release binaries for this host into target/release
make test       # real-binary e2e scenarios and fuzz seeds
make check      # fmt, clippy -D warnings, test, app-check, check-version
make app-check  # flutter analyze
```

## Run

The installer runs the hub as the `protectorate-hub` user service. On the hub:

```
protectorate login codex                        # once per harness, on the hub; --status lists them
protectorate quota
protectorate sleeve spawn alice --workspace https://github.com/owner/project.git --harness codex --mem 8G
protectorate chat send alice "Read the repository instructions and report the current state."
protectorate chat watch alice
```

A clone is placed at `$PROTECTORATE_HOME/workspaces/alice` on the hub. An
existing hub directory can also be selected; the sleeve edits it directly. See
[workspace safety](docs/chat.md#workspaces) before selecting a checkout.

## Docs

- [PROBLEMS.md](PROBLEMS.md): what hurts and why
- [ARCHITECTURE.md](ARCHITECTURE.md): the system and the decisions behind it
- [docs/2026-10-24-rust-direction.md](docs/2026-10-24-rust-direction.md): the Rust direction record
- [docs/install.md](docs/install.md), [docs/auth.md](docs/auth.md), [docs/sleeves.md](docs/sleeves.md), [docs/rust-sandbox.md](docs/rust-sandbox.md), [docs/node-work.md](docs/node-work.md), [docs/mcp.md](docs/mcp.md), [docs/backlog.md](docs/backlog.md), [docs/testing.md](docs/testing.md)
