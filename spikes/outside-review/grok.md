Date: 2026-10-19
Command: grok -p "<prompt>" --permission-mode plan --cwd /home/hotschmoe/github/protectorate
Prompt: Read PROBLEMS.md and ARCHITECTURE.md in this repo (and skim archive/ and references/ as needed). The ethos is: lightweight is everything; let each vendor's harness do the AI work. Critique this architecture hard. What is over-engineered? What would you cut? What is missing? Propose at least one materially different, lighter architecture variant and argue for it. Specifically weigh in on: Connect-RPC vs simpler JSON; tsnet+Headscale vs plain WireGuard vs SSH-only; containers vs lighter sandboxes vs microVMs for sleeves; hub as host binary vs container; Go+Flutter vs all-Rust; orchestrator-as-a-sleeve; cross-harness memory. Also: anything in the reference projects (t3code, factory, pi, mcp_agent_mail) we should steal or that already solves part of this. Keep it under 900 words, concrete. IMPORTANT: this is a read-only review. Do NOT create, modify or delete any files; only read and answer in your final message.

----
I'll start by reading the architecture docs and skimming the archive and reference projects so the critique is grounded in what's actually here.I'll skim the archive's previous design and the reference projects next so the critique can name what to steal versus rebuild.The archive and t3code already overlap a lot of this; I'll pull their remote, isolation, and memory pieces next.The current architecture still ships a platform. The ethos is a pane of glass over quota, machines, tmux, and a backlog. Connect-RPC, tsnet+Headscale, a node daemon on five OSes, a sleeve runtime interface, Flutter, and an MCP bridge are five products. Ship the pane.

## Over-engineered

**Connect-RPC.** The 1:1 method = CLI = MCP mapping is the design. Protobuf is not. You already except terminals to WebSocket. MCP wants JSON Schema. Flutter can POST JSON. `buf generate`, committed `gen/`, and Dart stubs are a second toolchain for ~15 methods. Spike 4 is the answer: skip it.

**tsnet + Headscale.** The hub has a public IP, so NAT traversal is not your problem. Headscale is an extra process whose death darkens the house. `WhoIs` as identity couples auth to the mesh. t3code treats Tailscale as a route and keeps identity on the environment; copy that split. Embedding userspace WireGuard in every binary (Windows, Termux) is spike 1 because it will hurt.

**Node daemon on every machine.** P2 already says sshd exists on all five targets. Kids' PCs and Termux do not want `protectorate node serve`. Live metrics and job dispatch (P3/P9) can wait. The `spikes/ssh-agentless` forced-command probe is the node.

**Containers as default sleeves.** P8 needs a PID namespace so agents cannot see each other. Docker was the archive: socket mount, root-equivalent, fat images. Windows and Android never get this runtime, so the “small interface” is a fiction until those hosts exist.

**Flutter on day one.** P4 is “type in tmux from the phone.” A web terminal on the hub does that. Native extra-keys and notifications come after attach works.

**Sidecar HTTP inside each sleeve, needlecast, microVMs, leases, job scheduler.** Archive leftovers and year-two work. cortical-stack V3 (beads fork, graphs, compaction) is the memory product PROBLEMS.md already rejected.

## Cut

Connect/protobuf/buf. tsnet+Headscale. Node agent as default. Container runtime as default. Flutter until a phone browser can attach. Per-sleeve sidecar. Needlecast protocol. MicroVMs. Runtime interface with four implementations. Beads-shaped `.cstack/`.

Keep: host hub binary, tmux as session truth, files as state, CLI-first, orchestrator drives via CLI, credential-dir mounts (never single files), quota readers.

## Missing

P2’s open security: allowlisted commands, phone approval for family boxes, append-only audit on a machine the agent cannot write. ARCHITECTURE.md has an SSH CA and nothing else.

Web terminal first.

Resleeve handoff. `.cstack/CURRENT.md` is not “what Codex did while Claude was asleep.”

Inventory as a skill file any harness can read. That matches the ethos better than injecting MCP into every CLI.

Windows/Android isolation: say “no sleeves there,” or the runtime interface is a lie.

P1 as a vertical slice that shells out to CodexBar. Do not re-scrape four undocumented quota APIs in v1.

## Lighter variant: SSH hub + files + tmux

```
phone/desktop --HTTPS--> hub binary
                           | tmux sessions (one harness per workspace)
                           | systemd-run --scope + unshare --pid   (Linux hub only)
                           | ssh -i cert  (nodes; no daemon)
                           |
~/.protectorate/{hub.yaml,nodes.yaml,backlog.md,ca/,quota.json}
~/sleeves/<name>/.cstack/{CURRENT,PLAN,INBOX,OUTBOX,HANDOFF}.md
```

API: HTTP POST `/v1/<command>` with `encoding/json`. CLI is the same handlers. MCP is a thin wrapper (stdio or HTTP) over that. `curl` works.

**Connect vs JSON.** JSON. Schema later as OpenAPI if drift hurts.

**tsnet vs WireGuard vs SSH.** SSH for nodes. Clients hit the hub’s public IP (or the office WG you already have). If the phone needs a private path, run stock Tailscale **once on the hub**, as ops, not as a library. Plain `wireguard-go` hub-and-spoke is the fallback if you must own a tunnel. Do not run Headscale until you have nodes that cannot dial out.

**Sleeves.** `systemd-run -p MemoryMax= -p CPUQuota=` plus `unshare --pid --fork --mount-proc` on the Linux hub. Factory’s bubblewrap/Seatbelt is the next step if you need filesystem hide. Containers only when P3 needs a toolchain image on a Linux build box. Firecracker only for untrusted public-repo donate.

**Hub binary vs container.** Host binary. Correct. Keep.

**Go+Flutter vs all-Rust.** Go hub. Serve xterm.js from the same binary (the archive already had this). Flutter when extra-keys become the pain. All-Rust UI loses Android. Do not take on Dart and protobuf for a UI you can defer.

**Orchestrator-as-a-sleeve.** Keep the idea, drop the isolation. Poe is a privileged tmux session with `protectorate` on PATH and a scoped token. A sleeve that can spawn/kill other sleeves is not a sleeve; you will punch holes until it is the hub. Same path for MCP bridges: token + CLI.

**Cross-harness memory.** Four markdown files plus `HANDOFF.md` written at switch. Resume the native harness thread if you return; inject a delta summary if you do not. Do not share session history (you already noted `/workspace` collisions). Do not build compaction or graphs.

## Steal (do not fork)

**t3code:** route list vs identity; Tailscale as an endpoint; web terminal first; MCP with scoped expiring tokens; provider switch as an explicit handoff artifact; pairing codes. Leave the Effect.ts event store, Clerk, and Cloudflare relay.

**Factory:** bubblewrap per-command sandbox. Not Droid (that is a harness).

**pi:** `pi-env` is the node you want if SSH commands are not enough: tiny remote daemon, credentials stay on the hub, works on Termux/Windows. `--json`/RPC for driving. pi-durable’s persist-before-display, forks, child tasks: conventions, not a library. Do not run everything through pi (P5).

**mcp_agent_mail:** memorable identities, inboxes, advisory file leases. MCP as the bus. Skip Git-commit-per-message, SQLite, Beads installer. They already abandoned the Python server.

**CodexBar:** P1. `codexbar usage --json` until it is insufficient.

**tincan:** skip; hosted chat fights files-on-disk.

**archive:** keep CLI-first, files as DB, bind-mount-directories, tmux. Kill envoy-in-Docker, sidecar, Traefik, Gitea, needlecast.

Ship order: quota dashboard that calls CodexBar → `nodes.yaml` + SSH certs + a generated `hosts.md` skill → tmux WebSocket in the hub → PID-namespace scopes on the hub → optional metrics probe over SSH. Everything else waits until one of those hurts.
