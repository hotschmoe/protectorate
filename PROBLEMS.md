# Problems

What hurts today, what we think the fix looks like, and what is still open.
This is a thinking document, not a spec. Started 2026-10-19.

Status key: `[idea]` the user's proposed direction, `[decided]` settled in
discussion, `[note]` discussion and suggestions, `[open]` unanswered questions
for the interview, `[done]` built (see ARCHITECTURE.md).

---

## Ethos (applies to everything below)

- **Let the harness do the work.** Claude Code, Codex, Grok, Antigravity and pi
  are each built by teams tuning them for their own models. We do not modify
  them, wrap their prompts, or try to out-think them.
- **Lightweight means hands-off toward the harnesses.** Each LLM uses its own
  harness exactly as its vendor built it. We don't proxy model traffic, inject
  prompts, replace tools or screen-scrape. We only use the harness's own
  interfaces (its TUI, or its official headless protocol) and its own
  config/auth.
- **The orchestration layer above the harnesses is built to be correct,
  robust and fast**, however much engineering that takes. That covers
  workspaces, isolation, networking, nodes, scheduling, the API and the UI.
- **One pane of glass** over accounts, machines, sessions and work.
- **Everything is drivable from the CLI.** Anything the UI can do, the CLI can
  do, with machine-readable output. That lets an MCP server or bridge (Claude,
  Grok bot, Dots, Muse, ...) drive Protectorate later, either *as* the
  orchestrator or by talking *to* the orchestrator, which in turn drives each
  sleeve.

---

## P1. No visibility into subscription usage

**Problem.** I pay for Claude, Grok, Codex and Antigravity, and I run a capable
local LLM through the pi harness. For each subscription I don't know:
- how much usage is left (percent),
- when it resets,
- whether I have any reset credits I could redeem.

So tokens expire unused while I hold back on other accounts.

`[idea]` A single dashboard listing every subscription with remaining %, reset
time and redeemable resets, so I can spend each allotment before it resets.

`[note]`
- This is the most self-contained problem and probably the best first thing to
  ship. It is useful on day one, even with nothing else built.
- Data sources per provider (research 2026-10-19):

  | Provider | Windows | How to read it | Redeemable resets |
  |----------|---------|----------------|-------------------|
  | Claude | 5h, 7d, per-model weekly, extra usage | Official: the status line JSON `rate_limits.{five_hour,seven_day}`. Undocumented: `GET api.anthropic.com/api/oauth/usage` (beta header, `user:profile` scope). | Occasional "Reset for free", redeemed only in web/Desktop Settings > Usage |
  | Codex | primary + secondary, credits | `codex app-server` JSON-RPC `account/rateLimits/read` (schema unstable). Undocumented: `chatgpt.com/backend-api/wham/usage`. | **Banked reset credits** from grants and referrals that expire after about 30 days. Redeemed via `/usage` in the CLI. Most likely what "resets on account" means. |
  | Grok | one weekly shared pool | Undocumented `cli-chat-proxy.grok.com/v1/billing?format=credits` | One-time reset coupons (`GetRemainingResets`) |
  | Antigravity | quota fraction + reset | **Local only:** `agy -p /usage --output-format json`, or the local language server. Do not call Google's remote endpoints from our code (ban risk). | unknown |

- **CodexBar** (`references/CodexBar`) already aggregates all four and
  documents each data source in `docs/<provider>.md`. It's the key reference
  for P1. It's a Swift macOS menu-bar app, so we borrow its data sources, not
  its code.
  ccusage and claude-monitor only total local logs, not server-side quota.
- Redeeming a reset can't be undone, so the dashboard should show resets but
  only redeem one on explicit confirmation.
- The local LLM has no quota. Its useful metrics are availability, current load
  and tokens/sec. It works as the "free" fallback tier.

`[done]` `protectorate quota` and the app's QUOTA view read every provider
(protectorate-quota); `quota suggest` ranks spare capacity.

`[open]` Do the Codex banked resets match what you see? Claude has reset credits
too (protectorate-quota reads them; t3code shows how to redeem them, see
docs/2026-10-24-rust-direction.md section 9). Grok is unconfirmed.

---

## P2. Logging in on every machine

**Problem.** I have a headless main dev machine, plus a desktop, laptop, media
server, GPU server, and my wife's and kids' computers. I log coding CLIs into
these for maintenance: NAS dismounts, rebuilding game servers and docker
containers, installing things. The harnesses log you out after too many
machines or a week of inactivity, so I keep logging in again, for example on
the media server.

`[idea]` One central machine holds all logins, credentials and work. It has SSH
keys to every other machine and does maintenance remotely. No extra logins on
other machines. There's a list of machines with add and remove.

`[note]`
- This fits well with "let the harness do the work". The agent runs on the
  central machine with its own login and just uses `ssh host '...'` as a tool.
  The harnesses are already good at this. The main pieces we need are an
  inventory (host, OS/arch, user, how to reach it, notes) and key distribution.
- Plain sshd works on all five platforms: Windows ships OpenSSH Server, and
  Android can run sshd under Termux. So P2 can work without our own daemon
  installed anywhere.
- **Security.** The central machine becomes the crown jewels: every
  subscription plus a key into every machine in the house, kids' PCs included.
  Worth considering:
  - per-host or per-purpose keys, or short-lived SSH certificates from our own
    small CA (`ssh-keygen -s`), so revoking one machine is one step,
  - an audit log of every remote command an agent ran,
  - an approval step for the family machines, versus auto-approve for the
    servers.
- Generate the inventory as a skill/context file so any harness knows which
  hosts exist and how to reach them without us wiring anything in.

`[decided]` Agents get admin on the other machines.

`[done]` The hub holds every login; nodes run a small agent and take work as
hub jobs, never SSH sessions from sleeves. Gates built so far:
- a node accepts only the hub's client certificate from the hub CA (mutual
  TLS); being on the network grants nothing,
- `care` machines are never scheduled, and admin work anywhere needs a
  short-lived, task-scoped `node_admin` grant,
- agent calls that are destructive, mint admin grants or run admin jobs wait
  for the owner's approval from the phone or CLI,
- every remote command run under a grant is in a hash-chained audit log in
  hub state, which sleeves cannot reach.

`[open]` (revisit) Still to weigh:
- a per-host allowlist of commands that run without approval,
- extra confirmation for destructive command classes inside a job (rm,
  disk/partition, service removal, user/credential changes),
- short-lived SSH certificates for interactive needs,
- keeping the audit head on a machine the hub user cannot write.

---

## P3. Five architectures, idle hardware, and reliance on GitHub Actions

**Problem.** I use Windows x86, Windows ARM, Linux x86, Linux ARM and Android
often. Cross-compiling and cross-testing goes through GitHub Actions, while my
own idle machines sit unused.

`[idea]` Like the SSH list, keep a registry of hardware devices that builds and
tests can be sent to. That spreads big builds onto idle machines and tests
across real architectures.

`[note]`
- Suggest **job-level dispatch, not a distributed compiler.** "Run
  `cargo test` on win-arm at commit abc123 and give me logs + artifacts" is
  simple and covers both cross-arch testing and offloading. Splitting one build
  across machines (distcc, sccache-dist, Bazel remote execution) is a much
  bigger problem. Leave that to existing tools if it's ever needed.
- Rough job flow:
  ```
  agent --> hub: run(job, target=win-arm, commit=abc123)
  hub   --> node: git bundle / fetch abc123, run command
  node  --> hub: streamed logs, exit code, artifacts
  hub   --> agent: result
  ```
- This is where SSH alone stops being enough. Live CPU/GPU/RAM reporting and
  reservations (see P8) need something resident on each node, either a small
  daemon or a container.
- Android as a target: Termux (native ARM64 Linux userland) or `adb` from a
  Linux node. Its role is probably "run tests", not "build host".

`[done]` Job dispatch and leases are implemented: see "Nodes, leases and jobs"
in ARCHITECTURE.md (`protectorate lease ...`, `protectorate job run|test-cross`).

`[decided]` The main goal is **using idle resources** (offloading build
throughput). Cross-arch testing is a bonus.

`[note]` What this implies:
- Scheduling matters: pick the least-loaded node that has the right toolchain
  and enough free RAM, and avoid machines someone is actively using (the
  family PCs).
- Toolchains are the hard part of offloading. Every node needs the right
  compiler, SDK and dependencies. Container images (the build image carries
  the toolchain) solve this on Linux nodes and are a point in favor of
  "nodes run containers". Windows and Android nodes would need native
  toolchains.
- Warm caches matter as much as raw CPU. A node that keeps a persistent
  `target/` or build cache per repo beats a faster cold one, so prefer sending
  repeat jobs to the same node.

---

## P4. Long-running sessions are unreachable from my phone

**Problem.** I keep several tmux coding sessions open for days on the main
machine and reach them from home over a WireGuard VPN to the office. My phone
can't reach them unless it's a Claude Code session with remote control enabled.

`[idea]` The single pane of glass (native desktop or web) plus an Android
companion app.

`[note]`
- tmux stays the source of truth for sessions. That's already how you work,
  it's harness-agnostic, and we don't need to own the process.
- A web terminal (xterm.js over a WebSocket attached to tmux) works on a phone
  browser right away, before any native app exists. t3code is a good reference
  for a web UI over coding sessions.
- A native Android app is worth it for what a browser does badly:
  notifications ("agent is waiting on you", "quota resets in 2h with 40%
  left"), quick replies, and approvals.
- If the UI is Flutter, one codebase covers desktop, Android and web, which
  argues for Flutter (see P7).
- The hub will usually be headless, so a desktop app is a *client* that
  connects to the hub, the same as the Android app. I prefer native desktop
  apps over web apps.

`[decided]` I **type in sessions on the phone**, not only watch.

`[note]` This raises the bar for mobile:
- Typing in a terminal through a mobile browser is painful (no Esc/Ctrl/Tab,
  the soft keyboard covers the screen). A real terminal needs an extra-keys bar
  like Termux or Blink: Esc, Ctrl, Tab, arrows, and harness-specific shortcuts.
- That makes the native Android app more important than a web view. Flutter
  has terminal widgets (for example `xterm.dart`) that we can attach to the
  same tmux-backed WebSocket stream.
- Useful extras: a compose box for longer prompts (type the whole message,
  then send it), plus voice input.

`[done]` The Flutter app (desktop, Android, web) reaches the hub over the
routers' WireGuard, with chat sleeves, xterm terminals on tmux, approvals and
an Envoy-first phone layout.

---

## P5. One harness for everything?

**Problem.** Each subscription probably works best in its own harness, but
running everything through pi (and pi-durable) would be simpler. Do Claude,
Codex, Grok and Antigravity subscriptions work in pi, and is that allowed?

`[note]` Evidence so far, from pi's own source (`references/secondary/pi`):
- **Anthropic.** pi still has Claude Pro/Max OAuth login, but since April 2026
  it shows a warning: *"Anthropic third-party usage draws from extra usage and
  is billed per token"* (`packages/coding-agent/CHANGELOG.md`). So a Claude
  subscription in pi does not use your plan's included usage. It bills as
  extra usage.
- **Google Antigravity / Gemini CLI.** pi *removed* its built-in OAuth support
  for both around late April 2026 (`packages/ai/CHANGELOG.md`).
- **OpenAI Codex / ChatGPT, xAI.** pi still ships OAuth flows for both
  (`packages/ai/src/auth/oauth/`).
Web research (2026-10-19, confidence in parentheses):

| Provider | Subscription in pi / third-party harness | Ban risk |
|----------|------------------------------------------|----------|
| Anthropic | Allowed, but bills as per-token "extra usage" since 2026-04-04, not plan quota. Plan OAuth is for "Claude Code and other native Anthropic applications". Note that Help Center wording on "third-party apps" conflicts and is ambiguous. (high) | Low if you pay extra usage. Real if a harness pretends to be Claude Code. |
| OpenAI | Explicitly allowed. "Sign in with ChatGPT" (DevDay 2026-09-29) shares plan usage with partner apps, and pi and OpenCode are launch partners. Per-app caps of 10-100% of weekly usage. pi's old Codex OAuth is now "legacy", so use the new flow. (high) | Low |
| xAI | Explicitly allowed. xAI announced SuperGrok OAuth in OpenCode and Kilo Code (2026-05), and reportedly T3 Code. (high) | Low |
| Google Antigravity | Prohibited. Accounts were banned in 2026-02, amnesty followed on 2026-02-27, and a second offence is a permanent ban. Use an AI Studio / Vertex API key instead. (high) | **High** |

So pi can legitimately run Codex, Grok and the local LLM on plan quota. Claude
and Antigravity stay in their own harnesses.

`[decided]` (2026-10-24) **Stay multi-harness.** It matches the ethos (each vendor tunes its own
harness), keeps every subscription's included usage, and avoids ban risk. pi is
the harness for the local LLM, and possibly for Codex/xAI if their terms
allow. That puts the switching logic in *our* layer (which harness gets the next
task) instead of inside one harness.

`[note]` on pi-durable: it's an experimental *library* for building a harness
whose transcript and state are committed to storage before display, so a
crashed process resumes mid-turn. It persists *pi's* conversations. It can't
hold a Claude Code or Codex session, so it isn't a cross-harness memory layer.
Cross-harness "resleeve" memory still has to be harness-agnostic files that any
CLI can read (the cortical-stack idea). Worth a deeper read for its design
ideas: forks, child tasks, task graph, handoff, compaction.

---

## P6. Unused tokens expire

**Problem.** When I'm busy, subscription allotments reset unused.

`[idea]` A low-priority todo list I can check off, and agents get dispatched to
those items when spare capacity exists. Items include project stretch goals,
fixing issues on my own repos, and "donating" tokens to public open-source repos
by attacking their issues. Example: 50% of Grok left a day before reset, so
spend it on a public repo.

`[note]`
- This builds on P1. With quota and reset times known, a simple rule triggers
  work: "projected unused capacity at reset > X% and no high-priority work in
  the queue => dispatch the next backlog item to that harness".
- **Caution for public repos.** Many maintainers are now hostile to unsolicited
  AI PRs. pi itself auto-closes issues and PRs from new contributors (see its
  README). Suggest that donated work always lands in a **review inbox** (a
  branch on your fork plus a summary) and nothing public goes out without your
  approval. Prefer repos that explicitly welcome it (labels like
  `help wanted` / `good first issue`, or a stated AI policy).
- Your own repos can be more automatic: open a draft PR, never merge.
- Each item should record which harness/model attempted it, so we learn which
  subscription is good at what.

`[decided]` Our own, simple, markdown-based backlog. Items can link to GitHub
issues instead of copying them. Beads is too heavy and invasive and is only a
reference for how others do it.

`[open]` One backlog file per project in the project repo, or a central
backlog on the hub that covers all projects (including public repos we don't
own)? A central backlog seems necessary for the "donate" case at least.

`[done]` Central notes on the hub (`protectorate backlog`, the app's NOTES,
docs/backlog.md). Quota-aware dispatch of notes is not built.

---

## P7. Architecture of the thing itself (revisiting the archive)

**Problem/idea.** The earlier design (`archive/`, now only in git history) used containers per
workspace for isolation, shared persistent memory (cortical stack) and
inter-sleeve messaging (needlecast). I still like it. Shared memory across
harnesses is what makes switching harness by remaining quota possible,
especially with an overseer dispatching across all of them.

Questions raised:
1. **Isolation unit:** containers, microVMs, or something else?
2. **Runtime/language:** 100% Rust (UI + core), or a mix such as Flutter UI
   plus a Go/Rust/Zig engine? Elixir/OTP for supervision?
3. **Agent messaging:** between agents and sub-agents, and across harnesses.
4. **Build vs fork:** fork Factory or t3code, or study them and build our own?
5. **Networking between machines:** own domains + public IPs, WireGuard,
   Tailscale, or our own tunnel?
6. **Node footprint:** a single binary that phones home, or a container any
   docker host can run?

`[note]` First thoughts, to discuss:
- **Isolation.** The PID-clobbering problem (P8) is exactly what PID
  namespaces solve, and containers give you that cheaply. MicroVMs
  (Firecracker, Cloud Hypervisor) mainly add a security boundary against
  hostile code, at the cost of more complexity, and aren't needed for your own
  agents on your own repos. Plain kernel namespaces in a `systemd-run --scope`
  are lighter still.
- **Elixir.** OTP supervision trees fit "many long-lived workers that crash and
  restart" well, but the BEAM runtime has to be on every node, which fights the
  single-tiny-binary goal. Static native binaries cross-compile to all five
  targets (Android via Termux) from one machine.
- **Language split.** Flutter is the only option that gives desktop + Android +
  web from one UI codebase with a mature mobile story. Rust-native UI (egui,
  Dioxus, Tauri) is weaker on Android, so: a Flutter UI plus a native engine.
- **Build vs fork.** Studying is cheap, and both are in `references/`. Forking
  takes on their stack (TypeScript/Node for t3code) and their assumptions. Given
  the "lightweight, ours" ethos, I lean toward building from scratch and
  borrowing ideas, but let's look at their internals first.
- **Networking.** Considered: Tailscale/Headscale, our own tunnel, nodes
  dialing out to the hub over TLS.
- **Messaging.** References: the archive's needlecast (markdown inbox/outbox),
  `mcp_agent_mail` (identities, inboxes, file-reservation leases; note the
  Python repo says it's superseded by `mcp_agent_mail_rust`), and Tincan
  (hosted rooms for cross-harness agent chat). An MCP server is the most
  harness-neutral way to give every CLI the same messaging tools.

`[decided]` (2026-10-24, see docs/2026-10-24-rust-direction.md):
1. Isolation: our own Linux namespace sandbox in a systemd scope, no
   containers or microVMs.
2. Language: Rust for hub, node, CLI and bridge; Flutter for every UI.
3. Messaging: hub-native mailbox and coordination, exposed through the CLI
   and MCP.
4. Build our own, borrowing ideas.
5. Networking: the routers own connectivity (site-to-site WireGuard, phones
   as road warriors); protectorate binds private addresses and authenticates
   with its own CA and mutual TLS. No overlay of our own.
6. Node: one native binary per platform, installed as a user service.

---

## P8. Agents clobber each other's processes

**Problem.** With five agents in five repos on one machine, an agent sees a
heavy build process, assumes it's its own stray process, and kills another
agent's PID.

`[idea]` Workspaces still need isolation from each other, even if containers
aren't the final answer.

`[note]` The minimum fix is a separate PID namespace per workspace, so an agent
can't *see* other agents' processes. Cgroups per workspace also give
per-workspace CPU/RAM limits and accounting for free.

`[done]` Every sleeve and shared-node job runs in its own user, mount, PID,
IPC, UTS and network namespaces inside a systemd scope with CPU, memory and
task limits (protectorate-sandbox).

---

## P9. Shared hardware dashboard with reservations

**Problem/idea.** Machines on the network report CPU, GPU and RAM. Agents can
see what's free, reserve it (for example a specific GPU), deploy work there,
and get results or build artifacts back.

`[note]`
- This extends P3: the node agent reports utilization, and the hub holds
  **leases** (resource, holder, TTL). A lease that isn't renewed expires, so a
  dead agent can't hold a GPU forever.
- Expose it to agents as tools (MCP or a CLI): `nodes list`,
  `reserve gpu --vram 24G`, `run --on gpu-server ...`, `release`.
- It can be built as a sequence: SSH inventory (P2), then a resident node agent
  with metrics, then reservations, then job dispatch.

`[decided]` The dashboard also shows **disk space**. Agents often pile up
hundreds of GB of build caches and images, so a bird's-eye view of CPU, GPU,
memory and disk for every node is how we spot that something is wrong.
**Per-sleeve/container resource usage** is shown too, as the archived design
did (host stats and parallel Docker stats).

`[done]` Node metrics (CPU, GPUs, NPUs, memory, every filesystem, Docker disk,
per-container and per-sleeve usage), leases with TTLs, busy-GPU placement and
`sleeve prune` (inventory first, removal only with `--yes`).

`[note]` What disk should break down into:
- per filesystem: used/free, with alert thresholds,
- per sleeve: workspace dir size, the container's writable layer, and
  build-cache dirs (`target/`, `node_modules/`, `~/.cache`, Go/Cargo caches),
- the container engine: images, build cache and volumes (`docker system df -v`),
- top offenders, ranked, with a one-click (or one-CLI) cleanup *proposal* that
  only runs after confirmation.

Directory sizes are expensive to compute, so measure them on a slow interval
in the background (or by watching filesystem events), not per request.

---

## P10. Large open models over API keys (DeepSeek, GLM, Kimi, ...)

**Problem.** Besides subscriptions, we want large open models (DeepSeek, GLM,
Kimi, Qwen, MiniMax and similar) through pay-per-token API keys. Do we add a
harness for each, or run them in an existing one?

`[note]` pi already covers this with no new harness. Its provider list
(`references/secondary/pi/packages/coding-agent/docs/providers.md`) includes
DeepSeek (`DEEPSEEK_API_KEY`), the ZAI/GLM coding plan (`ZAI_API_KEY`), Kimi
for Coding, Moonshot, MiniMax, Qwen Token Plan, OpenRouter, Together,
Fireworks, Groq and Cerebras, plus custom OpenAI-compatible endpoints (our
local LLM server). So: one harness, many providers, chosen per sleeve.

`[idea]` Keys live only on the hub (P2). pi can read a provider key from a
command (`"key": "!cmd"` in `auth.json`), so a sleeve's pi config can point at
`protectorate secret get <provider>`, scoped to that sleeve's token and
audited, and the key is never written into the sleeve.

`[note]` Consequences elsewhere:
- Quota (P1): API keys are spend, not windows. Readers report balance or spend
  where the provider has an endpoint (DeepSeek has a balance API), otherwise
  UNKNOWN.
- `[decided]` The app's "local model" warning keys on local serving, not on
  pi or the model name: DeepSeek via API is remote and strong. The hub sets
  `ChatStatus.local_model` from the provider's base URL (loopback, private
  or inventory host); the app only reads that flag. Cards and badges
  are coloured by `ChatStatus.provider` as well as harness.
- `[decided]` Quota for API-key providers waits until more models are added;
  until then they report UNKNOWN.
- `[open]` Some of these vendors also document their models behind Claude Code
  through an Anthropic-compatible base URL. That is official config, but it
  mixes vendors inside the Claude harness and its config dir. Default: pi.

---

## P11. Windows-only programs driven by agents (ArchiCAD, ENERCALC)

**Problem.** We will often use programs such as ArchiCAD and ENERCALC that
need Windows, with Claude or Grok driving them. Today that is SSH to the
Windows machine. Keep that, or build a tool into the single pane of glass?

`[note]` Two separate questions:
1. **Reaching the machine.** This is P2/P3: the hub holds the keys and reaches
   the node over the routers' network. Let the sleeve use plain `ssh` (its own tools),
   but with a short-lived SSH certificate the hub signs for that sleeve and
   that node, valid for the length of a lease, instead of a long-lived private
   key inside the sleeve. Windows OpenSSH accepts user CAs
   (`TrustedUserCAKeys`). Revocation is the lease expiring.
2. **Driving a GUI program.** Windows desktop control is shelved; see tag
   `shelf/windows-desk`. Prefer the program's own API, `job run` or `ssh`, and
   `node put/get` for files.

`[decided]` Layered, most deterministic first:
- **The program's own API** where it has one (ArchiCAD has a JSON interface
  on localhost and a Python package; ENERCALC's automation surface is
  unverified). Agents call it through `ssh` or `job run`. Repeatable and cheap.
- **Files in, files out**: `node put` and `node get` move files and directories
  (compressed only where it shrinks, size and sha256 checked, atomic, confined
  to the node's file root); a batch/export runs as a job. Also deterministic.

`[open]` Which ArchiCAD and ENERCALC workflows are first, and whether they can
be done through APIs and files without the GUI.

`[later]` Always-on agent desktops on machines with Intel Arc Pro cards, which
support SR-IOV virtual GPUs: one or more Windows VMs (Hyper-V GPU partitioning,
or a Linux KVM host with SR-IOV vGPUs, or Windows Server with Remote Desktop
Services for several live sessions), each with its own always-logged-in agent
account and node agent, so agents drive a desktop while the user works on
theirs. To verify before building: which hypervisor the Arc Pro driver supports
for vGPU (SR-IOV support has been Linux/KVM first; Hyper-V GPU-P support is
unverified), ArchiCAD performance and licensing per VM, and how many seats a
card supports.

---

## Emerging shape

The problems reduce to a handful of primitives:

```
+------------------- HUB (central machine) ---------------------+
|  Accounts     subscriptions, credentials, quota, resets   P1 P5 |
|  Nodes        inventory, ssh, metrics, leases          P2 P3 P9 |
|  Sessions     tmux-backed harness sessions, web/mobile    P4    |
|  Workspaces   isolated (pid ns / container) + memory      P7 P8 |
|  Backlog      low-priority work, quota-aware dispatch     P6    |
+---------------------------------------------------------------+
        |  mutual TLS over the routers' WireGuard
        v
   node agents: win-x86, win-arm, linux-x86, linux-arm, android
```

Order (each step useful on its own):
1. Quota dashboard (P1) - done
2. Node inventory, exposed to harnesses (P2) - done, as hub jobs instead of SSH keys
3. Session viewer, web first and phone-reachable (P4) - done
4. Workspace isolation (P8) - done
5. Node agent: metrics, then leases, then job dispatch (P3, P9) - done
6. Quota-aware backlog dispatch (P6) - open: notes exist, dispatch does not
7. Cross-harness memory and messaging (P7) - done: mailbox, resleeve handoff
