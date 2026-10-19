# Protectorate

One pane of glass over your coding-agent subscriptions, machines, sessions and
work. A single Go binary is the hub, the node agent and the CLI. Flutter
clients (desktop, Android, web) talk to the same API.

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
| Sleeves (P7, P8) | `sleeve spawn/ls/show/attach/kill` | bwrap + systemd user scope: PID isolation, CPU/memory caps, shared live credentials |
| Nodes (P2, P3, P9) | `node ls/show/add/rm/serve/metrics` | CPU, GPU, memory, every filesystem, Docker disk, per-container and per-sleeve usage |
| Tailnet | `hub tailnet install/init`, `tailnet status`, `node invite/join/revoke`, `client invite` | embedded tsnet + supervised Headscale; in-process DERP relay over TLS with a pinned self-signed cert; WhoIs gating; verified direct and relay-only with a Windows node |
| Jobs (P3) | `job run/ls/show/logs/cancel/test-cross`, `lease acquire/ls/renew/release` | run commands on idle nodes with warm git worktrees; idempotent submit; renewed short leases; Windows process-tree kill |
| Orchestrator | `orchestrator spawn/status` | an agent in a sleeve ("poe") with a scoped token, driving the hub through the CLI |
| Backlog (P6) | `backlog ls/add/claim/done/edit` | one markdown file on the hub; items link to GitHub issues |
| Auth | `token create/ls/revoke`, `audit tail` | scoped, expiring tokens at every entry point; unsandboxed shells (raw sessions, host terminals) are owner-only unless granted `terminal:host`; append-only audit log |
| Clients | `pair`, `mcp`, `hub serve --web` | QR pairing with a scoped device token; MCP tools generated from the API |
| Ops | `service systemd/windows-task/install-windows/termux` | generates service definitions, never installs them |

## Build

```
make gen        # buf generate -> gen/ (Go) and app/lib/gen/ (Dart), both committed
make check-gen  # fails if generated code is stale
make build      # bin/protectorate
make test       # go vet + go test -race
make cross      # linux/windows amd64+arm64, android/arm64 into bin/
make app-check  # flutter analyze + test
```

## Run

```
protectorate hub serve [--web app/build/web]   # owner token at ~/.protectorate/token
protectorate quota
protectorate session new demo -- claude
protectorate sleeve spawn alice --workspace ~/src/proj --harness codex --mem 8G
protectorate pair                               # QR code for the phone app
```

## Docs

- [PROBLEMS.md](PROBLEMS.md): what hurts and why
- [ARCHITECTURE.md](ARCHITECTURE.md): the system and the decisions behind it
- [docs/install.md](docs/install.md): hub, node and phone setup
- [docs/auth.md](docs/auth.md), [docs/mcp.md](docs/mcp.md), [docs/backlog.md](docs/backlog.md)
- `spikes/`: the experiments behind each decision, with measurements
