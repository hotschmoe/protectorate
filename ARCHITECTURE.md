# Architecture

Decisions as of 2026-10-19. Read `PROBLEMS.md` first; this document describes
the system we build to solve those problems. Items marked `(spike)` are
decisions that depend on an experiment still running; see "Spikes" below.

Guiding rules:
- **Lightweight toward the harness.** Each LLM uses its own harness untouched.
  We don't wrap, patch, proxy or re-prompt it. We only use its own
  interfaces (TUI or official headless protocol) and its own config/auth.
- **Correct, robust and fast everywhere else.** The hub, workspaces,
  isolation, networking, nodes and API may be as sophisticated as correctness
  requires. "Less code" is not a reason to pick a weaker design here.

---

## Shape

```
 CLIENTS (all speak the same hub API)
 +-----------+ +-----------------+ +-----------------+ +-------------+ +---------------+
 | prot CLI  | | Flutter desktop | | Flutter Android | | MCP bridge  | | orchestrator  |
 | (--json)  | | (native)        | | (native)        | | Claude/Grok | | = a sleeve    |
 +-----+-----+ +--------+--------+ +--------+--------+ +------+------+ | using the CLI |
       |                |                   |                 |        +-------+-------+
       +----------------+---------+---------+-----------------+----------------+
                                  |  Connect-RPC (JSON over HTTP) + WebSocket terminals
                     +------------v-------------+
                     |  HUB  (headless binary)  |
                     |  accounts / quota        |
                     |  sessions (tmux)         |
                     |  sleeves (workspaces)    |
                     |  nodes / leases / jobs   |
                     |  backlog                 |
                     +---+------------------+---+
                         |                  |   tailnet (tsnet, embedded WireGuard)
              sleeves on the hub        +---v-------------------------------+
              (one harness per          | node agent (same binary)          |
               workspace, isolated)     | win-x86, win-arm, linux-x86/arm,  |
                                        | android (Termux)                  |
                                        +-----------------------------------+
```

## One binary

A single Go binary, `protectorate`, holds every role:

```
protectorate hub serve            # the engine, runs as a systemd service on the hub
protectorate node serve           # node agent on any machine (joins the tailnet)
protectorate node join <token>    # enroll a machine
protectorate quota                # every other subcommand is a CLI client of the hub API
protectorate sleeve ls|spawn|kill
protectorate session ls|new|attach
protectorate api call <Method> '<json>'   # generic escape hatch
```

- Go cross-compiles with `GOOS`/`GOARCH` to all five targets: linux/amd64,
  linux/arm64, windows/amd64, windows/arm64 and android/arm64 (Termux). No cgo
  in the core.
- **The hub is a binary on the host, not a container.** The archive's envoy
  container mounted the Docker socket, which is root-equivalent, so the
  container isolated nothing. It also needed `/proc` mounts for host stats. The
  hub manages SSH certs, tmux, host metrics and the tailnet, and all of that is
  simpler on the host.

## API: Connect-RPC, schema-first, JSON on the wire

- `proto/protectorate/v1/*.proto` is the **only** API definition. `buf generate`
  produces the Go server and client and the Dart client.
- The wire format defaults to JSON over plain HTTP/1.1 or HTTP/2:
  `curl -H 'Content-Type: application/json' -d '{}' http://hub:7470/protectorate.v1.HubService/GetStatus`
- RPC rather than REST: every method maps 1:1 to a CLI command and an MCP tool
  (`SleeveService.Spawn` = `protectorate sleeve spawn` = MCP `sleeve_spawn`).
- Event streams are server-streaming RPCs, which replace SSE.
- `--json` on any CLI command prints the proto-JSON response unchanged, so the
  CLI output, the API and MCP results have one shape.
- **Exception:** interactive terminals use a WebSocket in binary mode
  (`/ws/terminal/{session}`) attached to tmux. Connect's bidirectional
  streaming needs HTTP/2 and is awkward in Flutter, and a terminal is just a
  pipe.

## Networking: embedded tsnet + Headscale (spike)

- Every hub and node binary embeds `tailscale.com/tsnet`, a userspace
  WireGuard node. Joining needs no TUN device and no admin rights.
- The hub runs **Headscale** (self-hosted, open-source coordination server) as
  a managed child process or container. It's the one component that isn't
  ours.
- Nodes serve their own Connect API on their tailnet address, and the hub calls
  them directly. No reverse channels.
- `WhoIs` on each request gives the caller's identity, so a node only obeys
  the hub. Headscale ACLs add network-level rules (family PCs can't reach each
  other, nodes can't reach the hub's admin API).
- Phones and laptops can join the same tailnet with the stock Tailscale app
  pointed at Headscale, so the office VPN isn't needed.
- **Fallback:** plain `wireguard-go` hub-and-spoke with our own key
  distribution. The hub has a public IP, so it needs no NAT traversal.

## Workspaces (sleeves) and isolation (spike)

- A sleeve is one workspace plus one harness session, isolated so agents can't
  see or kill each other's processes (P8). It also gets cgroup CPU/RAM limits.
- The default runtime is a container on Linux hosts, with the harness running
  under tmux inside it. The runtime sits behind a small interface so a microVM
  runtime could be added later for untrusted work (donated work on public
  repos, P6).
- Variants being measured: Docker, bubblewrap, `systemd-run` scopes + PID
  namespaces, Firecracker microVMs.

## Orchestrator

- The orchestrator ("Poe") is **a sleeve**, not engine code. Its harness is
  whichever subscription has quota left, and its tool is the `protectorate`
  CLI with a scoped hub token.
- External bridges (MCP for Claude, Grok bot, Dots, Muse) use the same scoped
  token mechanism. "AI as orchestrator" and "AI talking to the orchestrator"
  are the same path.

## Credentials (spike)

- Log in once on the hub. Each provider has one canonical credential on the
  hub, which every sleeve uses live, never as a copy. Copies go stale when a
  refresh rotates the token.
- Bind-mount **directories, never single files**. CLIs save with
  write-temp-then-rename, and renaming over a single-file bind mount fails
  with `EBUSY`.
- Share **only the auth**, not the whole config directory. Shared session
  history collides, because every sleeve's workspace is `/workspace`.
- Per CLI (Claude, Codex, Grok, Antigravity, pi): find which file holds the
  credential and how it's written, then pick a symlink or a mounted subfolder.

## Quota (P1)

Readers per provider, never refreshing tokens and never redeeming resets
without explicit confirmation:

| Provider | Source |
|----------|--------|
| Claude | `GET api.anthropic.com/api/oauth/usage` (read-only), or the status line `rate_limits` |
| Codex | `codex app-server` JSON-RPC `account/rateLimits/read` |
| Grok | `cli-chat-proxy.grok.com/v1/billing?format=credits` |
| Antigravity | local only: `agy -p /usage --output-format json` |
| local LLM | availability, load, tokens/sec |

## State

Plain files on the hub, so it stays debuggable and crash-resilient:

```
~/.protectorate/
  hub.yaml            # config
  nodes.yaml          # inventory (P2)
  backlog.md          # low-priority work; items may link to GitHub issues (P6)
  ca/                 # SSH CA for short-lived certs to nodes
  tailnet/            # tsnet + headscale state
```

Per-workspace memory stays in `.cstack/` (cortical-stack format).

## Repo layout

```
cmd/protectorate/        # main + cobra commands (thin; call internal/)
internal/hub/            # Connect service implementations
internal/node/           # node agent: metrics, job runner
internal/quota/          # provider quota readers
internal/sleeve/         # sleeve runtime interface + implementations
internal/session/        # tmux sessions + websocket terminal
internal/tailnet/        # tsnet wiring
proto/protectorate/v1/   # API schema
gen/                     # generated Go (committed)
app/                     # Flutter client (desktop + Android)
spikes/                  # experiments; each has a README with findings
```

## Spikes

| # | Question | Decides |
|---|----------|---------|
| 1 | Does tsnet + Headscale work across the platforms (incl. Termux, Windows), and what does it cost in binary size and complexity versus plain wireguard-go? | Networking |
| 2 | Docker vs bubblewrap vs systemd-run vs Firecracker: startup time, overhead, PID isolation, limits, ergonomics | Sleeve runtime |
| 3 | Per CLI, how is the credential stored and written? Does sharing it survive the CLI's save pattern? | Credential sharing |
| 4 | Connect-RPC vs plain JSON-RPC: friction for Go, Dart and MCP | API style (sanity check) |
