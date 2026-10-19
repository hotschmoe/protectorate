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

## Networking: embedded tsnet + Headscale (spike 1 done: `spikes/tailnet/`)

Confirmed on linux/arm64, Windows AMD64 (officewin) and Termux-in-docker:
- a tagged preauth join takes about 2 seconds, with no root and no TUN,
- a `WhoIs` rejection of non-hub callers works,
- state survives a restart.

Costs:
- about 15 MB of binary,
- about 40 MB of idle RSS per node,
- tsnet and Headscale versions must be upgraded together (Headscale rejects
  clients older than v1.80).

Before production we need:
- Headscale's embedded DERP + STUN (UDP 3478) on the hub,
- TLS on the control URL,
- a key-expiry policy for tagged nodes,
- a real-device Android test (Android 11+ restricts netlink).

### Headscale lifecycle (`internal/tailnet`)

Headscale runs as a **supervised child process of `hub serve`** from a pinned
binary (v0.29.5) in `$PROTECTORATE_HOME/bin`, not a container. Reasons: the hub
is already a host binary with no Docker dependency; the release is one static
binary, so `hub tailnet install` can fetch it and verify its sha256 against
both the published `checksums.txt` and a checksum compiled into our binary;
the admin API key and unix socket stay on the host filesystem; and Docker
would add a daemon dependency and an extra failure domain to the one process
every node depends on. The supervisor restarts it with 1s..30s backoff,
SIGTERMs it on hub stop (SIGKILL after 10s) and appends its output to
`tailnet/headscale.log` (not rotated; use logrotate with copytruncate). If the
hub is SIGKILLed the child is not reaped: run the hub under systemd
(`KillMode=control-group`).

```
protectorate hub tailnet install
protectorate hub tailnet init --server-url https://hs.example.com [--pool build] [--key-expiry-days 180]
protectorate hub serve                       # starts headscale + joins as tag:hub
protectorate node invite <name> [--tier server|family] [--pool build] [--expires 1h]
protectorate node join --control URL --key KEY [--name N]   # on the node, then `node serve`
protectorate node revoke|renew <name>
protectorate client invite --name phone      # stock Tailscale app, tag:client
```

- Config lives in `tailnet/config.json`; headscale's `headscale.yaml` (JSON,
  which is valid YAML) and `policy.json` are regenerated from it on every start.
  Embedded DERP + STUN are on, Tailscale's public DERP map is off
  (`--public-derp` adds it). Use `http://LAN-IP:7480` for LAN-only; for
  anything off-LAN give headscale `--tls-cert/--tls-key` or terminate TLS in
  a reverse proxy, and open UDP 3478 for STUN.
- ACL (everything else is denied): `tag:hub -> tag:node:7475,22`,
  `tag:client -> tag:hub:7470`, and `tag:pool-X -> tag:pool-X:*` for each
  configured pool. Pools are the knob that lets chosen nodes reach each
  other (`init --pool`, `node invite --pool`). `tag:tier-server|family`
  records the trust tier on the node for hub policy.
- Enrollment keys are single-use, tagged and short-lived, created through
  headscale's REST API with an API key (0600, auto-recreated on expiry).
- Key expiry: headscale leaves tagged nodes unexpiring, so the hub applies
  `key_expiry_days` (default 180) to every `tag:node`/`tag:client` node once a
  minute; `node renew` extends one, an expired node needs `node revoke` and a
  fresh invite. The hub's own node never expires.
- The hub API is also served on the tailnet (port 7470) for `tag:hub` and
  `tag:client` callers, authorized by WhoIs instead of the bearer token
  (terminal WebSockets still require the token). `tag:node` is denied by ACL
  and by the authorizer. Node agents accept only `tag:hub`.
- Right after a join the netmap may not yet hold every peer, so the hub's
  tailnet `http.Client` retries "no such host" lookups for up to 15s.

SSH stays as the bootstrap and break-glass path (`spikes/ssh-agentless/`).

Verified end to end on 2026-10-19 with the real binaries:
- `hub tailnet install/init` and `hub serve` (supervised Headscale with
  embedded DERP on the LAN);
- `node invite officewin`, then `node join` and `node serve` on Windows
  AMD64 from a temp dir, with no installs and no firewall changes;
- the hub's `node ls` and `node show` reported officewin's live CPU, memory and
  NTFS/FAT32 disks over the tailnet;
- `node revoke` removed it.

Finding: on Windows, a process started over an SSH session **keeps running**
after the session closes. A long-running `node serve` there should be a
proper Windows service (or a scheduled task), and tests must stop it
explicitly.

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

## Workspaces (sleeves) and isolation (spike 2 done: `spikes/isolation/`)

A sleeve is one workspace plus one harness session. It's isolated so agents
can't see or kill each other's processes (P8), with cgroup CPU/RAM limits.

Measured on the hub (linux/arm64, rootless):

| Runtime | Cold start | Overhead | Limits | Harness from host | Use |
|---------|-----------|----------|--------|-------------------|-----|
| **bwrap in `systemd-run --user --scope`** | ~111 ms | ~2.7 MB cgroup | CPU + memory enforced | yes, unmodified | **default on Linux** |
| Docker | 530-700 ms | ~120 MB daemon + shim | enforced | yes (ro mounts) | fallback: macOS/Windows hosts, per-sleeve networking |
| Firecracker | ~50 ms to userspace, ~1 s to systemd | 45-100 MB | VM size | no (own OS image) | later: untrusted work (donated public repos, P6) |
| plain `unshare` | 66 ms | - | none | - | rejected: exposes all of `$HOME` |

Every isolated variant blocked `kill -9` on processes outside the sandbox.

Design:
- **tmux stays on the host** (the existing session manager and socket). A
  sleeve session's pane command is
  `systemd-run --user --scope --unit=prot-sleeve-<name> -p MemoryMax=.. -p CPUQuota=.. -p OOMPolicy=continue -- bwrap ... -- <harness>`.
  The harness and everything it spawns live inside the sandbox. The hub
  attaches, captures and streams exactly as with plain sessions.
- Mounts:
  - the workspace is read-write;
  - toolchains and harness installs (`~/.local`, `~/.nvm`, ...) are read-only
    at the same paths;
  - each sleeve has its own config dirs;
  - shared credential **dirs** are laid out per the credential table below.
- Known gap: bwrap shares the host network, so two sleeves' dev servers can
  collide on ports. Docker covers per-sleeve networking if that becomes a hard
  requirement.
- Windows and Android nodes run no sleeves. They're build/test targets only.

## Harness sessions: attached and dispatched (spike: `spikes/drivers/`)

Pending the user's confirmation that a harness's official headless mode
counts as "its own harness":

- **Attached:** the normal TUI under tmux, in a sleeve. This is what the user
  types into from the phone.
- **Dispatched** (backlog work, delegation): the harness's own headless
  protocol, supervised by the hub.

| Harness | Protocol | Settled signal | Approval |
|---------|----------|----------------|----------|
| Claude | `claude -p` stream-json in/out, `--permission-prompt-tool stdio` | `result` | `control_request can_use_tool` |
| Codex | app-server JSON-RPC, **via the user's daemon socket** | `turn/completed` | `item/*/requestApproval` |
| Grok | `grok agent stdio` (ACP) | `session/prompt` response | `session/request_permission` |
| pi | `pi --mode rpc` | `agent_settled` | none built in |

- **Handoff** works in both directions through native resume (`--resume`,
  `codex resume`, `pi --session`). `{harness, cwd, session_id}` is enough to
  resume.
- **One owner per session id:** the hub enforces it. Only Codex enforces it
  itself; the others let two writers corrupt the conversation.
- **Codex:** its TUI runs through a user-level daemon that keeps the thread
  locked, so the hub must drive that daemon's socket, not a private
  `codex app-server`.
- **Grok:** it honors the user's `permission_mode` config. Sending
  `/always-approve off` on session open turns the gate on without changing
  any config.
- **pi:** it has no approval gate. Either treat it as ungated and rely on
  sleeve isolation (preferred, keeps the harness untouched) or load an approval
  extension.
- **One protocol for all?** Not ACP: Claude, Codex and pi would each need a
  third-party adapter. Use native drivers behind one small Go interface:
  `Open(cwd, id) -> Session{Send, Events, Respond, Interrupt, Close}`.

## Orchestrator

- The orchestrator ("Poe") is **a sleeve**, not engine code. Its harness is
  whichever subscription has quota left, and its tool is the `protectorate`
  CLI with a scoped hub token.
- External bridges (MCP for Claude, Grok bot, Dots, Muse) use the same scoped
  token mechanism. "AI as orchestrator" and "AI talking to the orchestrator"
  are the same path.

## Credentials (spike 3 done: `spikes/credentials/`)

- Log in once on the hub. Each provider has one canonical credential dir on
  the hub, shared live by every sleeve, never copied. The docker test showed a
  copy keeps the old token after a refresh rotates it.
- Never bind-mount a single file. A rename-over fails with `EBUSY` (shown in
  the docker test). Mount the shared credential **directory** into each sleeve.
- Share only the auth. Config, sessions and history stay per sleeve.

| CLI | Per-sleeve config | Shared credential | Notes |
|-----|-------------------|-------------------|-------|
| Claude | `CLAUDE_CONFIG_DIR` | `CLAUDE_SECURESTORAGE_CONFIG_DIR` = shared dir | Native split. Temp+rename save; `mkdir .oauth_refresh.lock` sits in the shared dir, so refresh is serialized across sleeves. Don't symlink (it reads with `O_NOFOLLOW`). |
| Codex | `CODEX_HOME` | `auth.json` symlinked into the shared dir | In-place `O_TRUNC` write, so the symlink survives. No cross-process lock: a simultaneous refresh can hit `refresh_token_reused`, and Codex recovers on the next 401. |
| Grok | `GROK_HOME` | `auth.json` **and** `auth.json.lock` symlinked into the shared dir | Writes temp+rename next to the symlink's target; `flock` on the lock file is only shared if the lock is symlinked too. |
| pi | `PI_CODING_AGENT_SESSION_DIR` | `PI_CODING_AGENT_DIR` = shared dir | Uses a `proper-lockfile` lock dir created beside `auth.json`, so the whole agent dir must be shared. Sessions are split out via the env var. |
| Antigravity | ? | ? | Not installed. Measure later. |

The hub's sleeve runtime builds this layout. Each CLI still reads and writes
its own files its own way, so the harness stays untouched. Remote nodes can't
bind-mount: the candidates there are Grok's `GROK_AUTH_PROVIDER_COMMAND` and
Claude's `CLAUDE_CODE_OAUTH_TOKEN`, both still to be evaluated.

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
