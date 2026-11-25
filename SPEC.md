# Protectorate SPEC (next)

Status: decided 2026-11-01 (owner answers in `docs/sessions/2026-11-01-decisions.md`); development
has started. Motto: **make it work, make it right, make it fast**, in that order. A shortcut taken to
make something work is logged in `REVISIT.md` with what "right" would be. This file states the design intent for the rebuilt Protectorate on branch
`next`. The reasoning, citations and rejected alternatives are in `docs/design/`. The owner's own words
are in `docs/sessions/2026-11-01-redesign.md`. When this file and a design report disagree, this file
wins, and the conflict is listed in section 12.

| Report | Covers |
|---|---|
| `docs/design/network-trust-language.md` | embedded headscale, one-way trust, enrollment, TLS, language (historical, see 3.2) |
| `docs/design/sleeves-state-machines.md` | sleeve tree, isolation, harnesses, panes, Pi, guardrails, state machines |
| `docs/design/history-continuation.md` | history index, resume, continuation brief, mainplane findings |
| `docs/design/nodes-ci-deliverables-verus.md` | node daemon, presence, jobs, git/CI, deliverables, doctor, install, Verus, UI |
| `docs/design/tigerstyle-network.md` | the accepted direction for the network internals: one loop per process, fixed slots and pools, a simulated IO twin under VOPR |

**Zig rewrite (2026-11).** The program is now one Zig 0.17.1 program in `src/`, standard library
only (`build.zig.zon` has no dependencies). Rust, cargo, crates, headscale, tsnet, the Go member,
DERP and Verus are gone from the build and from the running system. Where this file still names
them, a line starting "Superseded (2026-11, Zig rewrite)" says what is true now, and the original
text stays as the record of the decision. Design the code has not reached yet is left as written.
`docs/design/README.md` marks which design reports and sessions are current.

Where the old crates went: `pt-json` is `std.json` plus `src/json_util.zig`; `pt-hash` is `std`
hashing (CRC-32C in `src/journal/codec.zig`, SHA-256 in `src/update/version.zig`); `pt-http` is
`std.http` with the hub's server in `src/hub/http_server.zig` and the CLI client in
`src/cli/client.zig`; `pt-journal` is `src/journal/`; `pt-core` is `src/core/`; `pt-api` and
`pt-apigen` are `src/api/schema.zig` and `src/tools/apigen.zig`; `pt-drivers` is `src/drivers/`;
`pt-history` is `src/history/`; `pt-sandbox` is `src/sandbox/`; `pt-frame` is `src/net/`;
`pt-node` is `src/node/`. Every file has a tier L0 to L5 and a group in `src/tools/levels.zig`, and
`make check-levels` fails an import that points upward.

## 1. What it is

A self-hosted software factory on one hub machine. The owner, Envoy (our orchestrator agent) and
vendor general agents (GrokBot, Claude Duet, ChatGPT Dot) ask for coding work, and sleeves do it.
**The product is chat-first:** the owner mostly talks to Envoy, and Envoy talks to the sleeves.
Opening a sleeve's own chat is routine; dropping into its real terminal is an advanced, last-resort
view. A sleeve is
a vendor coding CLI or Pi, running untouched in a light sandbox, bound to one directory. The hub also
reaches a fleet of untrusted machines (nodes) to inventory them, maintain them and run build and test
jobs across architectures. It is a software factory only: no email, calendar or errand agent. General
agents are the vendors' products, and they reach Protectorate only by talking to Envoy, through a
CLI on the hub machine (section 3.1).

## 2. Principles

1. **Deterministic first.** Anything code can decide, code decides: dispatch, quota ranking, retries,
   probes, guardrails, resleeves, scheduling, mail delivery, briefs. An LLM decides only what needs
   judgment (section 7). This keeps the hub robust and saves tokens.
2. **Hands-off toward harnesses.** We use only a harness's TUI or official headless protocol and its
   own config and auth. We never wrap, patch, proxy or re-prompt it, never install hooks into its
   config, and never type into a TUI on our own.
3. **One writer.** One core task owns all orchestration state. Each event goes through a pure
   `apply(state, event) -> (state, intents)`, is journaled with group commit, then executed.
4. **Every device is untrusted.** The hub may reach and execute on nodes. A node reaches the hub
   through one door, the fleet listener, and what it sends there is named and bounded:
   - one connection that the node dials (TCP, to an address in its join token), wrapped in a
     `Noise_IK_25519_ChaChaPoly_BLAKE2s` handshake against pinned static keys; a key the owner has
     not approved is dropped before any frame is read;
   - from an approved node, the heartbeat `Hello` (inventory, presence, load) and answers to
     requests the hub sent; every other node-initiated request is ignored;
   - the hub sends every job, lease and update RPC over that same connection.

   A node never reaches the hub API or its web listeners: those are separate listeners (loopback by
   default, or a trusted proxy) and need a token. Every byte from a node is parsed as hostile, and
   a node's job result is that node's claim, not proof (sections 12a, 12b).

   Superseded (2026-11, Zig rewrite): the earlier text said "a node cannot open a flow to the hub"
   and listed headscale control and DERP traffic, WireGuard packets to a hub member, and answers
   on connections the hub opened. Nodes now dial the hub; the one-way rule is kept as "the hub
   accepts nothing from a node but its heartbeat and its answers, on an authenticated connection".
5. **Record before effect.** Every write to a harness or node is an effect record, fsynced before the
   first byte leaves. An unconfirmed write is Uncertain, never silently retried or dropped.
6. **Fewest binaries and fewest dependencies we don't own.** Two binaries plus the Flutter apps.
   Our own crates can be as many as the design wants. Third-party code is the supply-chain risk:
   - a third-party crate, Go module or Dart package needs a reason that a few hundred lines of our
     own code cannot meet;
   - every one is pinned exactly (`=x.y.z`, a Go tag plus go.sum, an exact pubspec version), and
     nothing updates without a hand-picked bump;
   - where a dependency is large or only partly needed, we fork it under our control and trim it;
   - no Python or Node in the product;
   - the allowed list and its reasons live in `DEPENDENCIES.md`, and `make check` fails on a crate
     that is not listed.

   Superseded (2026-11, Zig rewrite): there are no crates, Go modules or Zig packages, so nothing
   to pin or fork: `build.zig.zon` has `.dependencies = .{}` and the program uses `std` only.
   Dart packages stay pinned in `app/pubspec.yaml`. `make check` has no dependency step; its
   `check-levels` step enforces the tier rules and rejects `test` blocks.
7. **LAN only.** `protectorate.hotschmoe.com` resolves and answers only on the LAN and over the
   owner's WireGuard. WAN access is a later, separate design.
8. **Repo rules carry over** from `dev-hotschmoe` CLAUDE.md: no emojis, full migrations with no
   shims, real-binary e2e and fuzz tests only, no hand-edited generated code.
   Superseded (2026-11, Zig rewrite): the tests are `scripts/zig-e2e.sh` (real binaries) and the
   VOPR deterministic simulator in `src/dst/` (`make dst`); there are no fuzz targets. Builds use
   `-Doptimize=safe` and `src/tiger.zig` assertions fail fast.
9. **No Verus** (D40, section 10). Tests are proportionate: real-binary e2e where behaviour broke
   or is security-relevant, and fuzz seeds that earn their keep. Superseded (2026-11, Zig
   rewrite): VOPR replaces the fuzz seeds (section 4, Testing).
10. **Threat model: outsiders, not our own agents** (owner, 2026-11-01; revisit as needed). The
    one fear is people outside the owner's network gaining access to it. Agents breaking things
    on the owner's machines (deleted directories, broken settings, a bad node job) is accepted:
    all of it is fixable and replaceable. Human involvement and friction are kept to a minimum.
    - **Full permissions.** Envoy and sleeves run harnesses unsupervised and run any node job,
      as root, unasked. No approval gate is added for what agents do to files, settings or
      nodes.
    - **Isolation is between agents.** The sandbox keeps sleeves out of each other's
      workspaces, sockets and processes, so one agent cannot disturb another. It is not meant
      to protect the machine from a sleeve.
    - **The outsider boundary is kept, and needs no human.** The realistic path for an outsider
      is content that steers an agent: a prompt injection, or a vendor agent's message. So:
      - no sleeve can widen who reaches the network: pairing devices, minting tokens,
        enrolling or approving nodes, and deciding approvals are owner-only methods;
      - credentials and keys (`~/.ssh`, other harnesses' logins, hub tokens, the hub's fleet key)
        stay masked from sleeves;
      - D9 still applies: in a turn a vendor agent or a D9-marked sleeve started, Envoy's
        removals, mass spawns and node jobs wait for the owner. This is the one remaining approval.
        A crash keeps the origin and the spawn count; the exit does not clear them, and the
        continuation of that turn does not start the count over. A usage-limit or auth settle
        keeps them too. The brief of an Envoy resleeve is sent as the owner and does not clear
        that origin or that count. Any other settle clears both. The core journals both, so a
        hub restart keeps them.

## 3. Layers

### 3.1 Clients

| Client | Reaches the hub by | Credential |
|---|---|---|
| Flutter desktop (Linux, Windows), Android, web | HTTPS through Traefik | `pd1.` device token |
| Owner CLI on the hub, Envoy and sleeves | loopback | `pc1.` capability (sleeves), owner token (CLI) |
| Vendor general agents (GrokBot, Claude Duet, ChatGPT Dot) | installed **on the hub machine**, running `protectorate envoy ...` through their own shell tool, or the stdio `protectorate mcp --envoy` | the local **envoy-only** capability: it can talk to Envoy and read Envoy's replies, nothing else |

Superseded (2026-11, Zig rewrite): credentials are random hex strings with no `pd1.` or `pc1.`
prefix. The owner token, the envoy-only token and each paired device's token are in
`<home>/auth.json`, and the CLI reads the owner token from there. The CLI and Envoy reach the hub
over HTTP at `listen_addr` (default `127.0.0.1:7470`); there is no unix-socket API. Only Envoy
is given a token inside its sandbox, so per-sleeve `pc1.` capabilities are not issued yet (REVISIT,
"Scopes are coarse"). The proxy listener is `--proxy-listen ADDR` with one or more
`--trusted-proxy IP`; connections from any other address are closed.

- **Vendor general agents talk to Envoy, never to the API** (D9, D6 changed). They get no hub
  token and no network path. The owner installs the vendor app on the hub, logs it in, and tells it
  to use `protectorate envoy send|wait|read`. The narrow stdio MCP (`protectorate mcp --envoy`)
  offers the same three tools for apps that prefer MCP. We write no per-vendor connector.
- **Envoy holds the power.** Envoy (a root sleeve) uses the full CLI over loopback for sleeves,
  nodes and jobs. When a general agent asks for something big, heavy or dangerous, Envoy raises an
  approval that reaches the owner's phone and waits for it. A code rule makes this mandatory for a
  list of actions in a turn a vendor agent started (removing sleeves, node jobs, mass spawns, pushes
  to protected branches), so it does not rest on Envoy's judgment alone (principle 10).
- **The CLI is the API.** Every CLI command maps onto one hub API method. Envoy and shell-capable
  sleeves use the CLI.
- **Who may mail whom.** A sleeve may send to itself, its children, its parent, and Envoy, and to
  nothing else: not a sibling, not another tree, and not an owner-only method. The command is
  `protectorate sleeve send <name> <text>` (`envoy` is Envoy's name; the parent's name is
  `$PROTECTORATE_PARENT`). `envoy send` is the vendor-agent door and refuses a sleeve.
  A sibling is reached through the parent. `sleeve send`, `envoy send`, and `sleeve spawn`
  accept `--message-file FILE` (or `-` for stdin) for long or code-bearing messages; it
  replaces the text arguments or spawn's `--message`. Mail is an ordinary `Send` (queue or steer;
  a pane gets `MailDeliver`) with the same marking as any sleeve
  message: the text starts with `[from sleeve <name>]` and the request id is `mail:<id>:...`.
  Send answers whether delivery began or the message was queued, and the queue length at
  acceptance. List and show display the oldest queue age. `sleeve inbox [<sleeve>]` reads
  queued messages in order, with sender, enqueue time and text, without consuming them; a
  sleeve credential can inspect its own queue, and its own messages in another reachable
  sleeve's queue; the owner can inspect any queue. `sleeve inbox <sleeve> --drop` journals
  removal of the caller's queued prompts. `sleeve send <sleeve> --supersede` atomically
  replaces those prompts with the new send; a refusal drops nothing. Neither affects other
  senders, in-flight messages or continuations, and supersede cannot steer.
  At a headless turn boundary, consecutive queued prompts are delivered in FIFO order as one
  prompt, separated by blank lines and retaining their normal sender headers. A non-prompt
  continuation ends the batch. For Envoy, a change between vendor-agent and other senders
  also ends the batch, preserving the vendor-agent transcript's privacy boundary.
  Every batched message shares the delivery effect's outcome;
  an unwritten retry keeps the batch. Steers still enter the running turn. The origin is the
  last sender in the strictest applicable class: D9-origin senders first, then the sleeve's
  parent or Envoy, then other senders. D9 restrictions apply when any sender has D9 origin;
  a parent or Envoy selected as origin receives the normal turn report. A journaled `MailBatching` marker
  enables this behavior once on upgrade; replay before the marker uses the former single-message
  delivery. Future delivery-semantics changes likewise need a journaled marker so older
  events retain their original effect allocations.
  One tree may land at most 8 messages on Envoy in 10 minutes, counted on the tree's root. Upward
  mail to a parent who is not Envoy has the same bound per sender. Mail to Envoy spends only the
  tree's budget. The core journals the events and replay rebuilds the times. A refusal says when
  the window reopens. Removing a child and spawning another under the same parent starts a new
  parent-mail budget; the parent drives that loop. A turn with D9 origin that spawns a sleeve or
  sends to one marks that sleeve. The mark is journaled, permanent for that sleeve id (a resleeve
  does not clear it; a new sleeve with the same name is a new id and is unmarked), and inherited
  by its children. A send from a marked sleeve marks the recipient, so the mark crosses a relay;
  replay applies the same rule. `sleeve show` reports the mark. Mail from a marked sleeve starts
  an Envoy turn with D9 origin, so the same removals, node jobs and mass spawns wait for the
  owner. Mail from an unmarked sleeve does not. `Wait` accepts several sleeves, waits for all by
  default or any with `--any`, and retains each reached result during the wait. It returns failure
  when the requested condition becomes impossible or the deadline passes. `Wait --until report` returns when the waited-on
  sleeve has mailed after the cursor, and it returns that report's seq. `--after-seq` sets the
  cursor; without it the cursor is the waiter's newest message to the sleeve, so the same report
  releases a later wait again. The core's continuation of a turn is not mail
  for that cursor: it is not a report, and it does not move the cursor. A waiter who has never
  messaged the sleeve counts all of its past upward mail. The owner's or Envoy's wait counts
  mail to the parent or to Envoy. A parent's wait
  counts only mail to that parent. A wait on Envoy is invalid, because Envoy never sends as a
  sleeve. A newly spawned sleeve's first prompt names `$PROTECTORATE_PARENT` (when it has one),
  `$PROTECTORATE_ENVOY` and `$PROTECTORATE_MAIL`, and a resleeve repeats them in the continuation
  brief.
- **Turn reports.** When a sleeve settles an owner turn that Envoy or its parent opened, the core
  asks for a report and the hub journals it (`Report`), queued to that party as mail from the
  sleeve with request id `report:<sleeve>:<turn>`, once per turn. Code writes it from the
  transcript and git, with no model: how the turn ended, tokens and context, branch, head and
  uncommitted files, commits and diffstat since the task's base commit (the workspace's HEAD at
  the sleeve's first launch), the turn's last test command and its result, notices, and the final
  message clipped at 2000 bytes with the transcript range for the rest. It skips the mail bound
  since the recipient opened the turn, and carries the D9 mark like any mail from the sleeve.
  It includes the declared status when the final message ends in `STATUS: DONE`,
  `STATUS: CONTINUING <remaining work>` or `STATUS: BLOCKED <reason>` (the latter two need a
  nonempty detail). Sleeve API views expose the latest settled turn's status, turn number and
  detail; list displays its state and up to 80 characters of its detail. A turn without a valid
  declaration clears the previous status.
  New owner-turn settle observations journal the declaration together with the outcome, so the
  core reconstructs it on replay without consulting the transcript.
  Each newly stored transcript settle also carries its own structured status in the API, so
  clients can inspect earlier turns after a later turn changes the sleeve's current status.
  The report carries its sleeve, turn and status as structured inbox and transcript data,
  captured when the turn settled and journaled with the report. Batched deliveries retain the
  metadata of each report.
  Spawn, send and start accept `--until-done N` for a nonnegative N. A CONTINUING settle sends a
  fixed continuation as the requesting party, at most N rounds. DONE, BLOCKED, a missing
  declaration, usage limit, auth failure, another party's message, stop or park ends the loop.
  The request, settle and allocated rounds are journaled; replay preserves their count. An
  unconfirmed round remains Uncertain, counts against N, and is not silently retried; the
  requester is told it may have applied. Exactly-once delivery is not promised because a
  harness write and its acknowledgement cannot be recorded atomically. A bounded continuation
  is not mail for report cursors.
  A usage-limit or auth settle reports after its continuation. Envoy reports to no one.
- **Envoy's context ceiling.** When an Envoy turn settles with its context at 500k tokens or
  more, the hub sends the core a `Compact` for Envoy; the core takes it once Envoy is idle with
  nothing queued, and the driver sends the harness's own compaction (`/compact` for Claude Code).
  The harness may compact earlier on its own. The compaction is not a turn: it ends in a
  compaction entry, or a `compaction_failed` notice when the harness kept its context, and either
  returns Envoy to idle.
- The web build is embedded in the hub binary and served from the same origin.

### 3.2 Network

- **The fleet link is our own and lives in `src/net/` and `src/hub/fleet.zig`.** Superseded
  (2026-11, Zig rewrite): the earlier design embedded official headscale (v0.29.7) as a sandboxed
  child process, ran a Go tsnet `member` beside the hub and every node, and reached nodes over a
  WireGuard tailnet with headscale's DERP relays, with a one-rule policy `tag:hub -> tag:node:7475`.
  All of that is gone: no headscale, no tsnet, no Go member, no DERP, no tailnet. `go/` is kept
  for reference and is not built. The accepted direction for the network internals (one loop per
  process, fixed slots and pools, simulated IO under VOPR) is
  [`docs/design/tigerstyle-network.md`](docs/design/tigerstyle-network.md); today's code is
  thread-per-connection, and that document lists the gaps.
- **Link:** a node dials the hub's fleet listener (`--fleet-listen`, default `0.0.0.0:7475`).
  - Crypto: `Noise_IK_25519_ChaChaPoly_BLAKE2s`, prologue `protectorate-fleet-v1`, static X25519
    keys on both sides, the node pinning the hub's key from its join token. Records are at most
    32 KiB; a session fails at 1 GiB or 24 h and the node redials.
  - Frames: `[u32 len][u8 kind][u64 id][payload]`, at most 1 MiB; the hub's request ids are odd and
    the node's are even.
  - Roaming: the join token carries every address the hub advertises (`advertise_addrs` in
    `config.json`: LAN, WireGuard, DNS names). The node races them with a 150 ms stagger, sends a
    heartbeat every 5 s, redials after 15 s of silence, and a new connection for the same key
    replaces the old one. The hub drops sessions silent for 20 s.
  - `serve --stdio` on the node runs the same session over stdin and stdout, for a bridge such as
    ssh; the hub has no stdio acceptor yet.
- **Policy:** the hub accepts a handshake only from a key in its approved-node list (journaled
  `node_registered` then `node_approved`; removed nodes drop out). An unknown key fails the
  handshake and is dropped before any frame is read. After `Hello` the hub reads nothing from a
  node but heartbeats and answers (section 2, principle 4).
- **Remaining surface, by design:** the fleet listener's handshake and frame parser run before a
  peer is known. Records, frames, replies and artifacts are size-capped, and the fleet listener
  holds at most 64 connections (`src/net/limits.zig`).
- **TLS:** Traefik on stronghold terminates TLS for `protectorate.hotschmoe.com` with its existing
  certificate, and forwards everything to the hub's trusted-proxy listener (`--proxy-listen`,
  `--trusted-proxy`). The fleet link needs no TLS and does not go through Traefik: Noise encrypts
  and authenticates it, so nodes dial the hub's fleet port directly.
- **Enrollment, "scan to join":**
  1. The owner runs `protectorate node join-token` on the hub. It prints the `ptjoin1:` token (the
     base64url of the hub's public key, its `sha256:` fingerprint and its addresses) and a QR code
     of it.
  2. On the new machine, `protectorate-node join TOKEN` pins the hub, creates the node's key
     (mode 0600) and prints the node's public key and fingerprint. Nothing leaves the node.
  3. The owner adds that key on the hub with `protectorate node add NAME:NODE_KEY` and approves it
     with `protectorate node approve AUTH_ID [--name N] [--fingerprint FP]` (a wrong fingerprint is
     refused). `protectorate-node authorize` or `serve` then connects.

  No secret leaves the hub, and nothing on the node calls a hub endpoint before approval. The app
  cannot scan the QR code yet (D5).

### 3.3 Hub

One Linux machine, one `protectorate` process. It holds:

- Tick processing tracks Uncertain and Probing effects, active jobs, eligible sleeves and pending approvals in id order; replay rebuilds these derived indexes from the same state transitions. Terminal records remain available, including finished jobs with retryable node writes.
- **The journal and state:** sleeves, incarnations, activity, queues, effects, guardrails, jobs,
  nodes and approvals. These are all owned by the core task.
- **Headless first, panes on demand.** Every sleeve runs headless by default, through the
  harness's official protocol, and its transcript is the chat the app shows. tmux goes away. A
  **terminal takeover** is the advanced view: at a turn boundary the hub ends the headless
  incarnation and starts the harness TUI in a hub-owned PTY with the harness's own resume flag on the
  same session. Handing back works the same way in reverse. Only a taken-over sleeve has a PTY, a
  raw byte ring and a VT screen model. Antigravity, which is pane-only (D13), is the one exception.
- **Events and waits:** `Wait(until idle|done|blocked|report|ended|parked)` reads the state channel.
- **Quota:** read-only readers per subscription, which feed dispatch.
- **Mail:** delivery, re-delivery and escalation. Who may send is in section 3.1. `Wait --until
  report` returns the newest upward report's seq. `--after-seq` moves the cursor; without it the
  cursor is the waiter's newest message to the sleeve, and a waiter who never messaged it counts
  all of its past upward mail. The core's continuation of a turn is not mail
  for that cursor: it is not a report, and it does not move the cursor. The owner's or Envoy's
  wait counts mail to the parent or to Envoy. A parent's wait counts only mail to that parent.
  A wait on Envoy is invalid, because Envoy never sends as a sleeve.
- **Approvals:** routing and timeouts.
- **History:** an index over native session logs (section 3.7).
- **Snapshots and backups**, and **audit**.

Planned updates re-exec the hub in place and pass the pane fds across, so sleeves survive. After a
crash, every incarnation goes to Recovering and resumes natively.

### 3.4 Sleeves

- **Sleeve and incarnation.** A sleeve is the durable identity: name, workspace claim, parent link,
  resleeve policy, budget and queue. An incarnation is one harness process tree with a per-sleeve
  generation number `g`. Mode (pane or headless), harness, provider and model belong to the
  incarnation, so a resleeve can change all of them.
- **Tree rules:**
  - One root per directory, never two. Writable claims never overlap. A spawn that overlaps one
    is refused with the sleeve and the claim it overlaps.
  - Children are depth 1, at most 8 live per root. A child gets its own git worktree and branch by
    default, or a read-only view of the root.
  - **New project roots.** The owner, a paired device and Envoy may
    `sleeve spawn <name> --new-path DIR`. A sleeve may not (it spawns only its own children), and
    a vendor agent cannot call `SpawnSleeve`. The hub creates `DIR` and `git init`s it on branch
    `main` with one empty commit (the owner's `user.name` and `user.email`, or `protectorate`
    when a field is unset), then claims it as a root. `DIR` must be a new path strictly inside
    `projects_dir` (`config.json`; empty, the default, turns this off, and `/` is refused): a
    relative path under that directory, or an absolute path inside it, including one written
    through the configured spelling when `projects_dir` is a symlink. No `.` or `..` component,
    no symlink under it, and not inside the hub's home. A trailing slash is accepted. A path
    the hub cannot see is refused with `the hub cannot see DIR`. A directory a live root holds
    is refused with that sleeve's name. A repository with no commit is refused and told to
    remove it or to commit before `--path`, because `--path` would adopt a root that cannot
    branch children. Any other directory that already exists is left for `--path` to adopt.
    A name already in use, a name the core would refuse, or a harness with no headless mode,
    is refused before the directory
    is created and before the D9 gate, so that refusal does not count. An Envoy spawn counts
    against its D9 turn once the core applies it; one the owner denies, or that times out at
    the gate, does not. Envoy's spawns pass the gate one at a time. A refused spawn removes the directory only when no live claim overlaps it
    and it is still that
    empty commit; otherwise the directory stays and the error says so. A spawn the core does not
    answer, or whose reply is lost, keeps the directory without that check.
  - Stopping or removing the root stops every child in the same step. A root crash, resume,
    resleeve or park leaves the children running.
  - Native subagents run inside the incarnation, and we don't manage them.
- **Light isolation** (it prevents accidents; it is not a security boundary against a hostile
  agent):
  - user, mount, PID, IPC and UTS namespaces;
  - the host root bound read-only, with the workspace and a private home writable;
  - a sized `/tmp`, with disk-backed per-sleeve build and temporary scratch under the hub home,
    mounted at a short path and removed when the sleeve is removed; launch refuses scratch on
    tmpfs or ramfs;
  - host device nodes named in `sleeve_devices` (for example `/dev/aipu` and `/dev/mali0`)
    bound with `--dev-bind` when the path exists; a missing path is skipped and logged, and
    the sleeve still starts. Config changes require a hub restart. The hub polls configured
    paths for a new resolved filesystem node; when one appears, each running sleeve records a
    notice naming it and relaunches with a native session resume, immediately if idle or at
    the next turn boundary if working. A path missing at hub startup is watched too. A device
    removed and still absent is not rebound until it returns. Shared mount propagation does
    not replace this: a bind of an individual node pins the old inode, while propagating a
    host directory would expose unlisted devices and requires a separately managed shared
    allowlist mount and propagation setup. Host supplementary groups still apply
    even when sandbox ownership appears as nobody:nogroup. Doctor and each launch report the
    resolved node, owner, group, mode and hub read/write access without opening it, plus host
    process holders; the first prompt lists bound devices and holders;
  - tmpfs masks over the hub's state, `~/.ssh` and the owner's own credentials; every
    harness's shared hub login (`<home>/logins/`) and the owner's cargo registry, Go and pub
    caches bound read-write;
  - a systemd scope per incarnation with a memory limit in bytes from `sleeve_memory_max`
    (default 4 GiB), overridden by sleeve name in `sleeve_memory_max_overrides`; swap is
    disabled and build parallelism follows the CPU quota. Configuration is read at hub start
    and applied at launch. A later sleeve with the same name gets the same override.
    `sleeve show` exposes the incarnation limit in bytes, and `PROTECTORATE_MEMORY_MAX`
    gives the configured value inside the sandbox. A new non-Envoy sleeve’s first prompt
    reports the limit and tells sleeves needing more to ask Envoy; the owner raises `sleeve_memory_max_overrides`.
    Scope OOM notices and the next non-Envoy prompt name the limit and override key.
    The hub transcript and continuation brief retain the request text without the hub’s
    prompt preamble, which is still sent to the harness. With scopes
    disabled no limit is enforced. The scope uses `OOMPolicy=continue` and `cgroup.kill`,
    plus the orphan reaper. A child the kernel kills for memory does not stop the scope.
    The guardrail requires a scope OOM kill and a launcher exit by SIGKILL or code 137.

  Superseded (2026-11, Zig rewrite): `src/sandbox/` builds the sandbox as bubblewrap
  (`--unshare-pid`, `--die-with-parent`, the host root read-only, the workspace and git
  directories bound, a cleared environment with an allow-list, a tmpfs `/tmp`, and
  `<home>/run/<sleeve>/tmp` as `TMPDIR`) or, where bwrap is missing, a Landlock write-containment
  re-exec (`__guard`). `--no-sandbox` turns both off. Credential paths under the owner's home
  (`~/.ssh`, `~/.config/gh`, `~/.aws`, other harnesses' directories and similar) are masked; the
  sleeve's own harness directory stays writable and `HOME` is the owner's, so sleeves share the
  owner's logins and caches. There is no `<home>/logins/` store, no private home, no per-sleeve
  systemd scope, no memory limit and no OOM watcher (REVISIT). A sleeve's processes are tracked in a
  fenced cgroup, or by process group and tag where cgroups are not delegated, so stop and remove
  can kill them. `sleeve_devices` in `config.json` is polled and reported by `doctor` but the
  devices are not bound into the sandbox yet.

  Cut from today's sandbox: the network namespace, the egress proxy, cache overlays, nested cgroup
  delegation and the pin-approval gate. Sleeves share the host network and the owner's download
  caches. Size: about 4.8k lines today, about 1.2k after.
- **Harnesses:**

| Harness | Interface |
|---|---|
| Claude Code | headless stream-json (with `--replay-user-messages` for prompt ack), or pane |
| Codex | headless `codex app-server`, or pane |
| Grok Build | headless `grok agent stdio` (ACP, with `--always-approve` when unsupervised), or pane |
| Antigravity CLI | replaces Gemini CLI, which is dropped (D35). Pane only, until approvals work headless and plan policy is clear. A standalone binary is required; if it only ships through npm, it waits. Superseded (2026-11, Zig rewrite): `src/drivers/antigravity.zig` runs `agy` headless with `--input-format stream-json --output-format stream-json`, like the other harnesses |
| Pi, two sleeve kinds | headless `pi --mode rpc`, run as the standalone binary so the hub needs no Node. **`pi_vendor`** offers any login or API key Pi supports, from Pi's own provider catalog: xAI, OpenAI (ChatGPT sign-in), DeepSeek, Kimi/Moonshot, GLM (Z.ai), MiniMax, Qwen, OpenRouter and more. The sleeve picks provider and model. **`pi_local`** points at a local OpenAI-compatible endpoint. Anthropic through Pi bills as extra usage and is labelled so. Gemini and Antigravity logins are not offered (removed upstream, ban risk) |

- **Default tools:** none beyond what each harness ships, plus our CLI mounted in the sandbox.
  Native tools are not replaced.
- **Harness installs and updates are native** (D33). Each harness is installed by its vendor's
  official method, a standalone binary wherever one is offered, and updates through its own
  updater (its built-in auto-update or its own `update` command). We never pin, patch or repackage
  a harness. Rules in code around that:
  - an update never touches a running sleeve: the running process keeps its binary, and the next
    launch or resume picks up the new one;
  - the hub runs a harness's own update command only when no sleeve of that harness is mid-turn;
    it logs the start and end of each wait and changes of the blocking sleeve; a long turn can
    defer an update indefinitely;
  - `doctor` records the version and install method per harness, so a behavior change can be
    traced to the version that brought it;
  - a failed launch after an update marks that harness ineligible in `doctor.json` until a
    launch succeeds, and resleeve skips it.
- **The update check runs once every 24 hours** per harness, through the harness's own command.
  On the dev hub on 2026-11-01 they are `claude update`, `codex update`, `grok update` and
  `pi update self`. A harness without one is updated by its official installer, re-run.
- **Pi is a standalone binary, not an npm package** (D34), updated with `pi update self`. If that
  command can't update a standalone install, the hub fetches Pi's GitHub release itself, verifies the
  checksum and stages it beside the old one.
- **Codex** runs as its standalone release binary, not through the npm wrapper.

### 3.5 Nodes

- **One kind of node** (D32). Presence detection holds work while someone is at the machine.
  The owner can journal a per-node job policy with `node set <node> --jobs any|owner|none`
  and an optional `--note TEXT`. The default `any` permits jobs from all callers, `owner`
  permits only owner jobs, and `none` permits none. Explicit node requests are refused with
  the policy and note; arch placement skips nodes that refuse the caller. Changes also apply
  to queued jobs, and survive hub restart. Inventory and rollout are unaffected.
- **One node binary** per OS. It runs as a system service on Linux and as a LocalSystem service with a
  runner account on Windows. It also builds for Android (Termux), and for macOS once a Mac exists.
  All RPCs are hub-initiated, over the connection the node dials to the hub's fleet port (7475 by
  default), not on a tailnet port. Superseded (2026-11, Zig rewrite): releases build the node for Linux
  (amd64, arm64), Windows (amd64, arm64) and macOS (arm64); there is no Android node build.
- **Inventory**, every 10 minutes: arch, CPU, RAM, GPU, disks. **Samples**, every 15 or 60 s: load,
  memory, disk.
- **Human present:** PRESENT, AWAY or UNKNOWN, with idle time, lock state and the source used.
  - Linux: logind, then `/dev/input`.
  - Windows: `WTSQuerySessionInformation` from the service.
  - macOS: `HIDIdleTime`.
  - UNKNOWN counts as present, so a node whose detector fails takes no jobs. `doctor` reports a
    node that stays UNKNOWN.
  - Remote logins (ssh, the hub's own job sessions) are not presence; only local seat input and
    lock state are.
- **Jobs** (maintenance, cross-arch build and test):
  - Human-readable `job ls` shows the first command line, capped at 80 characters with `...`
    when shortened; `job show` and JSON retain the full command.
  - the node journals each job before exec, and limits each one: 1 h by default, 64 MiB of log,
    and a per-job cgroup or Job Object;
  - the hub pulls output as length-prefixed frames;
  - each fetched artifact is capped by `job_artifact_max_bytes` on the hub (default 64 MiB),
    and a refused artifact reports the effective cap;
  - cancelling a finished job reports its state and exit code;
  - `hub info` reports the effective artifact cap; a job held for present or unknown presence
    reports the reason in `job show` and once on stderr for `job run --wait`;
  - a job never re-runs by itself;
  - scheduling, which is code, weighs arch, toolchain, load and warm cache, and only among nodes
    that are AWAY;
  - the node itself refuses to start a job while PRESENT or UNKNOWN, so a stale hub view cannot
    put work on a busy machine. The owner can override one job explicitly
    (`--even-if-present`), which is how urgent maintenance reaches a machine in use (PROBLEMS P2);
  - when a person arrives mid-job, the job drops to idle priority (nice 19 and idle I/O class on
    Linux, IDLE_PRIORITY_CLASS on Windows) and keeps running. It is not killed, because jobs
    never re-run by themselves. A job may declare `on_present: kill` instead.
- **Leases** (PROBLEMS P9: reserve a GPU with a TTL) are not in the first build. When they come,
  they will be a hub-side table that expires unless renewed, not a node feature (D30).

### 3.6 Inputs

Inputs are the owner (app, CLI), Envoy, vendor general agents (MCP/CLI) and sleeve mail, all through
the same dispatch code. There are no external sources such as email, calendar or chat.

### 3.7 History

- **Three tiers, cheapest first:**
  1. **Native resume** with the harness's official flag, when the session still exists and the same
     harness is wanted.
  2. A **continuation brief** for every other case, built by code in under a second:
     - task, where it stopped, git state, files touched, test status, open items;
     - effects that may have been applied;
     - pre-run search hits.

     Planned resleeves also wait for an optional LLM summary; forced resleeves skip it. Handoff and
     forced resleeve are one flow. The file is `.protectorate/BRIEF.md`. Superseded (2026-11, Zig rewrite):
     the hub writes `<home>/briefs/<sleeve>.md` (`src/hub/brief.zig`), outside the workspace, with the
     cause and a short summary; the fuller brief and the optional LLM summary are not built.
  3. **Search:** `protectorate history search|read` over a keyword index. The brief already carries
     the top hits, because the forced-resleeve target (pi_local) is the weakest reader.
- **Storage:** our own keyword index, `src/history/` (the old crate `pt-history`), in a directory of checksummed,
  append-only segments plus a manifest of source cursors. It is a derived, rebuildable index;
  native logs stay authoritative and are only read.
  - Reason: SQLite FTS5 through a binding would bring a third-party package plus bundled C SQLite
    for one feature, against the minimal-dependency rule (D37). Owner decision 2026-11-01; this
    overrides `docs/design/history-continuation.md` 1.4, 4.2 and 4.7.
  - What it does: a tokenizer for paths and identifiers (camelCase and snake_case parts),
    positional postings, BM25, all-terms and phrase queries, and filters by sleeve, harness,
    session, time and role. Ingest tails each log by byte offset; small segments are merged.
    A source's cursor, drop watermark and budget share one key, the sleeve and the path.
    An index that still uses an older key fails to open and is rebuilt from the logs. A
    native session id selects a log only under that sleeve's home. A sleeve credential
    searches and reads only docs tagged with its own sleeve id (D26). Envoy and the owner
    are unrestricted. A parent still reads a child's
    live transcript (`get_transcript`, through `reaches`): that is supervision of a running
    child, separate from this history search.
    `history read` applies the caller's sleeve scope before it copies text, then stops at 200
    docs or 8 MiB of source text and says when the range was cut. The next page starts after
    the last doc id (`--after`). Tool paths are kept to the
    same budget as tool text (512 bytes). `history_budget_bytes` in the owner config
    (default 1 GiB) is read when the hub starts. A value under 1 MiB, including 0, is
    refused and the hub does not start. Exactly 1 MiB is allowed. A missing key keeps
    the default. Each sleeve has that budget, and so
    does each source. The budget counts each doc's source text plus a per-doc overhead (its
    session, source, tool, paths, and a fixed header). An unsleeved source drops its own
    oldest docs once it is past the budget. A sleeved source does not: past a sleeve's
    budget, the oldest docs of that sleeve are dropped, whichever of its logs they came
    from. Newest means ingest order. A rebuild ingests each sleeve's log files oldest
    first, by mtime, so an older log is not ranked above a newer session. The transcript
    counts as the newest file. A full sleeve does not
    stop the others.
    `doctor` and `hub info` report `history capped: <sleeve id>` once eviction has dropped
    any of that sleeve's docs. The name stays while the sleeve is under the same budget.
    It leaves when that sleeve is removed, when its sources are gone, or when a later start
    finds the total under a raised budget. The set is filled when the index opens. A commit writes the manifest when the index changed. It removes
    segments whose docs are all dead in their own manifest update, and deletes those files
    only after that rename. It then rewrites a segment whose dead bytes are at least its
    live bytes, and some docs are still live, in a later manifest update; the old file goes
    after that rename. A rewrite failure leaves the
    dead-segment removal in place. A failed commit removes the segment files that attempt
    created, and the hub retries it with backoff. Opening an index removes segment files the
    manifest does not name, warns when it does that because the manifest is missing, and
    warns and still opens when reclaim fails. A reclaim failure after a durable commit is
    the same warning, and the commit stands.
  - Measured on this hub (2026-11-01, release build): 1 GiB of the newest real logs ingested
    in 1.9 s (540 MB/s, 48 MB peak RSS) into an 11 MB index; queries take 0.05 to 0.7 ms.
  - Fallback: SQLite with FTS5, if our index proves insufficient (REVISIT.md).
- **Vectors:** off until keyword search proves insufficient. When turned on, they come from an
  external llama.cpp server over HTTP, stored as f32 blobs with brute-force cosine in Zig (bge-small
  first). No new dependency.
- **Compaction:** native for now. The index records context usage per session, which a later
  "end the session and continue with a brief" replacement needs. That is mainplane's model, and the
  only one compatible with hands-off. Mainplane itself has no index or vectors: its agent runs `rg`
  over old logs because it owns the loop.

## 4. State machines

Full tables, invariants and mermaid diagrams are in `docs/design/sleeves-state-machines.md`
section 2. Diagrams are also in the Claude Doc
https://claude.ai/artifact/EBXmFg7wokeJTvSNNJSLwd.

| Machine | States |
|---|---|
| Sleeve lifecycle | Provisioning, Launching(g), Running(g), Recovering, Resleeving, Stopping, Stopped, Parked, Removed |
| Activity (per incarnation) | Booting, Idle{seen}, Delivering{id}, Working, Blocked, Interrupting, Compacting, Unknown, Ended |
| Effect record | Intent, Sent, Confirmed, Rejected, Uncertain, Probing, NotApplied, Failed, Fenced, Abandoned |
| Guardrail and resleeve | Armed, Interrupting, Resuming, Quiescing, Capturing, Selecting, Launching, Briefing, Cooldown, Parked |
| Node job | Queued, Submitting, SubmitUncertain, Running, Collecting, Succeeded, Failed, Interrupted, Lost, Cancelling, Cancelled, Abandoned |

- **Fencing:** every observation and effect carries `g` (sleeves) or `attempt` (jobs). Anything stale
  is dropped.
- **Guardrails G1 to G14** are pure predicates:
  - quota exhausted, auth failed, crash, OOM, start failed;
  - silent, loop, turn wallclock, approval timeout;
  - budget, forbidden path, disk limit;
  - interrupt unresponsive, protocol desync.
- **Guardrail actions:** interrupt, resume (at most twice in 10 min), resleeve, or park. Automatic
  resleeves go to headless mode, walking the sleeve's ordered `resleeve_to` list (default
  `[pi_local]`). A third resleeve within an hour parks the sleeve. When that trip is an auth
  failure, the park reason names the login that was rejected (`flapping (<harness> login rejected)`).
  Any other flap stays `flapping`. The hourly limit itself is unchanged.
- **Auto-continue:** after a native resume of a headless turn that a crash ended, the core sends
  one short continuation prompt, keyed to that owner turn. A relaunch that is a fresh session,
  because that incarnation never acked a prompt, does not continue: the owner and the parent
  get `TurnNeedsPrompt`. A turn gets at most one, across
  generations: a second crash of the same turn does not send another, including after the resume
  window, and an unsent continuation keeps its key when a later generation sends it. A later
  owner message is a new turn and can be continued once. A turn the continuation itself started
  is not continued again; the owner and the parent get `TurnNeedsPrompt`. An owner interrupt, or
  a guardrail that interrupts (silent, loop, turn wallclock), is not continued and gets no
  warning, including when a protocol desync kills the sleeve during that interrupt.
  The hub journals a `Continue` event and the effect is allocated only when that
  event is applied, so a journal recorded before the event existed replays to the same effects.
  The prompt is sent as the party that opened that owner turn. An agent steer, or mail from a
  D9-marked sleeve, replaces that party, which keeps D9 on the continued turn and reports a
  rejection to that party. A continuation does not clear an origin the turn already has, and it
  does not reset the spawn count. An exit keeps that origin too. A usage-limit or auth settle
  keeps the origin and the count, and the Envoy resleeve brief is sent as the owner, so it does
  not clear them. An agent
  continuation with no recorded name is labeled
  `agent:`, so D9 still applies. The owner
  still hears when the prompt cannot be sent.
  It is not sent after a planned stop, a hub restart of an idle sleeve, or a manual resleeve (the
  brief is that continuation). A stop, park, or resleeve also drops a continuation still queued,
  including one fencing put back because it never left, and the owner and the parent get
  `TurnNeedsPrompt`. An idle crash that ends in a park or a resleeve drops a debt with no warning.
  A debt still held when recovery finds that incarnation already idle
  and armed is dropped with no warning: it reached Ready and nothing was sent after that.
  A resume that is still booting keeps the debt.
  A `Continue` that cannot be delivered drops the debt and warns. A crash of an incarnation that
  already reached idle does not carry the debt. A protocol desync while that incarnation is idle
  drops the debt the same way, and the owner is warned when the resume does not replace it. A
  desync during boot keeps a debt. When the prompt cannot be sent, the owner and
  the parent get `TurnNeedsPrompt`. A settle of that same owner turn drops the debt with no
  warning, including one recorded after resume while the incarnation is already idle, so a later
  hub start does not send `TurnNeedsPrompt` for a turn the journal already settled.
- **Testing:** each machine is a pure function with a fuzz target. The target checks invariants I1
  to I10 against a table-driven reference model and checks `replay(journal) == live`.
  Superseded (2026-11, Zig rewrite): there are no fuzz targets. VOPR (`src/dst/oracle.zig`)
  drives the real core, journal and Noise/frame code for hundreds of seeds under injected faults
  (torn writes, bit flips, drops, reordering, tampering) and asserts after every step: no
  overlapping claims, approval and D9 cleanup, child bounds, the job state machine, and that
  replaying the committed journal bytes into an empty state gives an identical state hash
  (`make dst`; `docs/srcmd/dst.md`).

## 5. Code decides, Envoy decides

| Code | Envoy |
|---|---|
| dispatch by quota (at spawn and at guardrail time only), retries, probes, guardrails, resleeves and target choice, mail, cascades, budgets, approval timeouts, node scheduling, brief facts | goal decomposition, briefs, repo and workspace choice, task-size estimates, reading reports, reviewing diffs, answering sleeves, what spare quota should go to |

Envoy is an ordinary root sleeve. It never ranks harnesses, retries, or runs a guardrail resleeve.
Code never resleeves a healthy idle sleeve just to chase quota, because that busts its prompt cache.

## 6. Deliverables and build

```
protectorate-{linux-amd64,linux-arm64,macos-arm64,windows-amd64.exe,windows-arm64.exe}
                                       hub: serve | CLI | mcp | doctor; embeds the Flutter web build
protectorate-node-{linux-amd64,linux-arm64,macos-arm64,windows-amd64.exe,windows-arm64.exe}
                                       node: join | authorize | serve | supervise | install-service |
                                       uninstall | probation | status | doctor
protectorate-desktop-windows-{amd64,arm64}.zip, protectorate-android.apk
install.sh, install-node.sh, install-node.ps1, VERSION, SHA256SUMS
```

Superseded (2026-11, Zig rewrite): the block above is the asset list `scripts/release.sh` builds.
The earlier list had a Linux desktop build, an Android node and a `member` and headscale embedded
in every binary; none of those exist now. Linux binaries are static (musl).

- **Language: Zig 0.17.1 and Dart.** Zig, standard library only, for the hub, the node and the
  tools (`src/`); Dart for the Flutter app. Superseded (2026-11, Zig rewrite): this was "Rust and
  Go", with Zig only as the C toolchain behind cargo-zigbuild and Go only for headscale and the
  tsnet `member`. `zig build release-all` cross-compiles every target from one machine.
- The sleeve runner and sandbox helper are subcommands that re-exec `/proc/self/exe`.
- **Releases are built locally, not in GitHub Actions** (owner, 2026-11-01). `make release` builds
  every asset on the dev hub and on nodes over ssh for targets it cannot cross-build. It writes
  `SHA256SUMS`, and `gh release create` uploads them. GitHub Actions runs no build, so a compromised
  runner or action cannot inject code into a release.
- **No signing key for now.** Updates verify each asset's sha256 against `SHA256SUMS`, fetched over
  HTTPS from the same release. That catches corruption and truncation, but not someone who can
  replace release assets, which takes the owner's GitHub account. GitHub immutable releases
  (assets locked once published), where available, narrow that further. Signing stays in
  `REVISIT.md`.
- **Uninstall** removes exactly what the install manifest lists.

### 6a. Updates from GitHub releases

Updating should be one command or one button, and never a site visit. Both hub and nodes fetch
straight from GitHub releases, on the stable or dev channel, as today.

- **Verification:** every update checks the asset's sha256 against the release's `SHA256SUMS`
  before anything is staged (no signing key for now; section 6).
- **As built (2026-11, Zig rewrite):** the update code is `src/update/store.zig`, using
  `std.http.Client` (no `curl`). `hub update` stages the release in `<home>/releases/<version>/`,
  moves `current` and `previous` (links, plus `current.txt` and `previous.txt`) and writes
  `probation.json`; it does not restart the hub or re-exec it, so the service restarts it and sleeves
  resume as after a crash (D17 is not built). The service runs `releases/previous/protectorate hub
  probation` before each start, which switches back a release that was started twice without
  being marked healthy; the hub marks itself healthy once its journal has replayed. `hub update
  --rollback` only switches the links. There are no journal snapshots, update-aside quarantine or
  `rollback-open` marker yet, so the snapshot and restore text below is design not built. A node
  fetches the release from GitHub itself, and if that fails the hub downloads it, checks it and
  streams it over the fleet link in 128 KiB pieces; the node exits, its service or supervisor
  starts the new release, and the node marks it healthy once the hub has answered its first
  `Hello`. A node release that starts twice without that is switched back by `probation`.
- **Hub:**
  - `protectorate hub update [--version]`, or the app's UPDATE key, downloads the release and stages it
    in `releases/<version>/`.
  - Before switching, the hub snapshots the journal at a durable batch boundary. The probation
    record stores `next_sleeve` from that snapshot.
  - The hub re-execs in place (D17), so sleeves keep running. The previous release stays on disk.
  - If the new hub fails its health check within 2 minutes, prestart (the previous release)
    restores the snapshots before switching `current` back. The whole journal directory is
    replaced. Journal events after the journal snapshot
    are lost. Sleeve directories with an id at or above the recorded `next_sleeve` move to
    `<home>/update-aside/<time>-<pid>/` and are not deleted. Worktree checkouts and gitdirs
    stay; prestart lists a sleeve that recorded `worktree.path` and stops those sleeves'
    scopes. Remove a leftover checkout and run `git worktree prune` in the root repository.
    Until then the checkout holds `pt/<name>`, so a new worktree for that name is refused.
    The branch survives, and a new sleeve with the same name starts from those commits once
    the checkout is gone. A new worktree does not adopt a path that already exists.
  - Manual `hub update --rollback` runs the restore offline before switching. It keeps the journal
    when the previous release can decode every intact frame; a torn final frame is ignored, the same way the journal opens. Otherwise
    it restores the journal snapshot and loses events after that snapshot. There is no separate
    event-tag declaration: the previous binary's codec is the decision, and prestart must be that
    binary. A rollback without a matching journal snapshot is refused. A
    new update and a manual rollback both require snapshot-aware rollback in the target release,
    which runs prestart.
  - A restore that fails before any live file is replaced keeps the trial release. The next start
    shows the error on `hub info` and raises an alarm. Repair the snapshot, stop the hub, and
    retry with `hub probation --retry-rollback`. That retry keeps a manual rollback request.
    A restore that fails after a live file was replaced does not keep the trial: prestart exits
    and the next prestart continues the restore. Prestart writes `releases/rollback-open`,
    synced, before that first change and removes it only when the rollback finishes. A later
    failure while that file remains continues the restore, including a validation failure
    that replaces nothing this time and a manual rollback of a release that was already
    confirmed. The marker applies while that probation names the current release. A new
    trial is refused while the marker exists. Prestart takes the journal lock before
    finishing an interrupted journal swap, when the journal directory
    is present, and before judging. While the hub holds that lock, `hub probation` prints
    the probation state and exits 0. It does not judge, restore, or count a start.
    `--retry-rollback` refuses while the hub holds the journal. If the trial binary itself cannot
    start, service needs the repair: neither an unreadable journal nor a failed binary can serve.
- **Nodes, triggered by the hub:**
  - The hub sends `Update(version)` over its own connection. The node downloads the release from
    GitHub itself: outbound to GitHub, not to the hub.
  - The node verifies the release, stages it, keeps the previous binary, and restarts through its
    service manager.
  - **Self-rollback:** if the hub has not contacted the node within 10 minutes of the restart, the
    node reverts to the previous binary. Hub contact is the only proof that matters, because the
    node can't call the hub. Superseded (2026-11, Zig rewrite): the node does dial the hub; its
    first answered `Hello` on the new release is the contact, and rollback is by start count (a
    release started twice without that contact is switched back), not by a 10-minute timer.
  - A node without internet gets the verified bytes streamed by the hub over the node's own
    fleet connection.
- **Order:** hub first, then one canary node per OS and arch, then the rest. `Hello` carries a
  protocol number (`protocol_version`, `src/net/msg.zig`) that nothing checks yet, so a release
  that changes the wire format has to update nodes before the hub.
  Node update runs OS/architecture groups independently, preferring an online idle canary. A
  busy deferred canary gives way to the next candidate; a failed canary skips the rest of its
  group, whose remaining rows are labeled `SKIPPED`. A running job holds its node for up to 30 seconds, then the
  update is deferred. Offline nodes are deferred. The CLI labels deferred results `DEFERRED`
  with the reason. The journal keeps the target and probation window; `node show` displays the
  target, a newer rollout replaces it, and `node update --cancel <node>` drops it. A pending
  attempt cancelled before dispatch is labeled `CANCELLED`. On the node's
  return the hub attempts the same update and confirmation once. The attempt is journaled before
  sending; after a hub crash an interrupted attempt is confirmed by Hello and Presence on the
  target, or remains pending as an uncertain outcome without automatic retry.
  Progress is published as node events and the
  CLI prints it before the final table. A lost update reply can be confirmed by Hello and
  Presence on the target release. `--all --no-wait` accepts the rollout in the background;
  its rollout id identifies the events; a hub restart stops an unfinished rollout, while its
  unattempted targets remain pending. Named-node `--no-wait` still waits for the switch but
  does not wait for the node to return.
- **Apps, from the hub's release:** `HubInfo.clients` lists the client builds the running
  release's `SHA256SUMS` names (`protectorate-android.apk`,
  `protectorate-desktop-windows-{amd64,arm64}.zip`); a hub not started from `releases/current`
  lists none. `GET /api/client/<name>` serves one to the owner and paired devices only. The hub
  fetches it from its own release on the first request, checks it against the local
  `SHA256SUMS` and keeps it in `<home>/clients/<version>/`, dropping older versions' builds.
  When the hub's version differs from the app's and lists the app's build, the app shows a
  banner: Install downloads it with the device token and installs it (Android through
  PackageInstaller, which asks the owner to confirm; Windows through a detached script that
  replaces the files beside the exe once the app exits, elevated only when that directory needs
  it, and starts the app again). On web the hub serves the app, so the banner offers Reload. A
  dismissal lasts until the app restarts or the hub's version changes.

### 6b. Moving the hub to a new machine

Not needed now; it is designed so it stays possible. **Moving the hub is a restore.** The same
backup that recovers a dead hub moves a live one.

1. **One root.** Everything the hub owns lives under `$PROTECTORATE_HOME`:
   - the journal and state;
   - `auth.json`: the hub's fleet key, the tokens and the paired devices (the `tailnet/` directory
     of the headscale design is gone);
   - history and doctor caches;
   - sleeve homes, which hold the harness logins and native session logs.

   Superseded (2026-11, Zig rewrite): `hub export` and `hub import` are not built. What a backup
   has to hold is in `docs/operate.md` ("Where state lives": `auth.json`, `config.json`, `journal/`,
   `transcripts/`, `briefs/`, `artifacts/`, `worktrees/`, `envoy/`). `history/` is derived. There is
   no `doctor.json` cache and sleeves have no private home.

   Workspaces live in the owner's own directories. The export records their paths and git remotes.
2. **Export:** `protectorate hub export`.
   - It quiesces every sleeve at a turn boundary (the resleeve machine's Quiescing, without the
     relaunch) and stops the fleet listener.
   - It writes a `.pbackup`, and pushes or bundles every workspace's
     unpushed branches.
   - The old hub then writes a `migrated` tombstone and refuses to start the fleet listener again.
     Two hubs with the same fleet key would split the fleet.
3. **Import:** install the hub with the one-liner on the new machine, then run
   `protectorate hub import <file>`. The architecture may differ, because every on-disk format is
   arch-independent.
4. **Cut over:** point Traefik's route, and the name or address the nodes dial (`advertise_addrs`),
   at the new hub. Node trust needs no change: the new hub holds the same fleet key (restored from
   `auth.json`), so the key each node pinned still matches. No re-enrollment.
5. **Resume:** sleeves come back through Recovering. Native resume works where the session logs
   moved with the sleeve homes; otherwise they get a continuation brief. `doctor` re-validates
   every harness login and mails you any that need a re-login.

## 7. Git, CI and doctor

- **GitHub stays the origin.** `gh` is installed and authed on the hub, and the hub alone holds
  the push token. Sleeves push through `protectorate sleeve push <id>` (12b item 6, where the design calls it
  `protectorate git push`) and get a read-only token for `gh` queries. The hub keeps bare mirrors as backup. No Forgejo, GitLab or Woodpecker.
- **Cross-arch CI** is a hub rule: dispatch `make e2e` and `make dst` as node jobs to each
  required arch. Plain binaries; containers only when a job asks for them.
- **`doctor`**, on hub and node, checks:
  - harness CLIs present, with versions and install method (D33);
  - credentials valid, through the read-only quota readers, never printed;
  - `gh` auth;
  - sandbox prerequisites, disk and clock.

  Output is fixed JSON, cached in `doctor.json`, and it gates harness eligibility for dispatch and
  resleeve. Failures mail the owner.

  Superseded (2026-11, Zig rewrite): `src/doctor/doctor.zig` runs on each `GetDoctor` call (and
  `protectorate-node doctor`) and reports per harness whether it is present, its version, its install
  hint and whether it is `eligible`, plus bwrap or Landlock availability, `git` and `gh` login and the
  configured `sleeve_devices`. It is not cached in `doctor.json`, it does not check clock or disk,
  and it mails no one.

## 8. Quota and dispatch

Superseded (2026-11, Zig rewrite): `select_target` and its helpers are in `src/quota/select.zig`, but
the hub does not call them yet; a quota or auth exit resleeves to the next harness in the sleeve's
`resleeve_to` list (`src/core/apply.zig`), and nothing computes the task-size class (D27).

Quota readers stay read-only per subscription. `select_target(task_size, allow, mode, exclude,
quota, health)` is pure code, using fresh data only. It weighs projected unused quota at reset with a
48 h urgency half-life and falls back to the local model. "Dispatch by quota" is this function,
called at spawn and at guardrail time. Code derives the task-size class, Envoy may nudge it one
class, and code clamps it (D27).

A stale reading keeps the windows of its last good fetch. The resleeve fallback treats those
windows like a fresh reading's: under the task's usable minimum the subscription is exhausted,
and at or above it the fallback may still choose that harness, ahead of a candidate with no
reading. A stale window that is under the minimum and names no reset stops blocking once the
row is older than that window's length, or five hours when the length is unknown. It is then
an unknown reading and may be spent. `select_target` itself stays on fresh rows. The local endpoint has no percent
windows. A fresh reading may be chosen; a stale or failed one may not. A local row that was
never read may still be spent.

A candidate with no reading may still be spent. That is `unknown`: no login file, the reader
is not running, the login is expired, or the hub has not read yet (including just after a
restart; readings are not journaled). For a Claude, Codex, or Grok OAuth login, a rejection
(HTTP 401) or a missing usage scope is the same, because that CLI refreshes the login and can
only do so if a sleeve launches. The scope case is the provider's structured error: `error`
is `insufficient_scope` or `invalid_scope`, or `error` is an object whose `type` or `code` is
`permission_error`, `insufficient_scope`, or `invalid_scope`. A 403 whose text merely says
permission is a failed reading, and so is a wrong key on the local endpoint. The harness CLI
refreshes the token; the reader does not. A
failed reading may not. That is `error`: the body did not parse, some other HTTP status came
back, or the
read timed out and left no earlier windows. The park reason names what blocks: a failed
reading, a stale row that is short of quota, or a local endpoint whose last reading is stale
or failed. A fresh row under the minimum still says it is short of fresh quota. A window whose reset time has passed still blocks while its
remaining percent is under the task minimum. That grace is the cache's stale window, ten
minutes, and it is the only such constant. Fetched before the reset, it lasts that long
after the reset. Fetched with the reset already behind, but by less than that, it
lasts that long after the fetch: a body that says the window is full and the reset was a few
seconds ago is still that limit. A stamp already long past when the row was fetched counts as
reset, and so does a window once the grace has ended. The fallback may then spend the row
because its windows have reset. That is not a missing reading. A reset at or before the epoch
is no reset time, and the percent still counts. Codex readings also carry the vendor's credit
and limit flags, balance, and available reset credits. When the limit is reached and credits
are available, the row shows the balance and the estimated hourly decrease between consecutive
readings; the rate is not a vendor claim. Envoy receives one queued notice when Codex begins
or stops spending credits; its journaled message records the last reported state across hub
restarts. Unknown and stale readings do not change that state. Spawn, start and resleeve onto
Codex succeed while it uses credits but return a warning, naming an eligible harness with fresh
quota headroom when one exists. Optional `quota_credit_floor`, unset by default, parks a new
Codex incarnation before launch when the last known balance is strictly below the configured
non-negative value, even when that reading is stale. No known balance leaves it unblocked.
The reason is `credit_floor`; running incarnations continue, and an owner may start a parked
sleeve after a replenished balance is read. This decision is journaled for replay. Model-scoped Claude windows
(`wk <name>`) stay on the quota row
and gate a candidate of the same family (opus, sonnet, haiku), ignoring case and version
(`Sonnet 4.5` gates `sonnet` and `claude-sonnet-4-5`, and does not gate Opus). `opusplan`
counts as both Opus and Sonnet, so that model is gated by either cap, and a cap whose name
contains `opusplan` gates either model. A candidate with no model, and a candidate whose
model is `default`, are not blocked by a model cap. On the fresh pick, a cap under the usable
minimum only ranks that candidate below an otherwise equal one that is not under it. When that
candidate is chosen or parked, the reason names the tightest such cap as `; <label> under the minimum`.
A scope that is not one of those families still gates when its name is at least three letters
and the model name contains it. The account windows always gate
(`5h`, `wk`), as does the single credit or rate-limit window of the other subscriptions. A
row whose only windows are caps for other models is an unknown reading for that candidate
and may be spent. `pi_vendor` and Antigravity have no
meter and are never chosen on this path. A candidate list that is only those is not reported
as short of quota. The pace projection uses a vendor window length when
the reading included one (Codex stores `limit_window_seconds`). The label stays what is shown
and can be coarser (`119` seconds is `1m`). A minute label is still a length when the reading
did not name one.

A read that finishes after its deadline is kept and published on the next refresh, stamped
with the time the read finished. Its age counts from that finish, not from the refresh that
collects it, so a late body does not look new. A late failure does not count again toward the backoff. Each
request inside that read gets only the time still left, so a follow-up call cannot discard a
body that already arrived.

**Spare quota (PROBLEMS P6).** A code rule triggers: projected unused capacity at reset is above X%
and no high-priority work is queued. When it fires, the next backlog item goes to that harness.
Envoy picks which item; code decides when and where. Work on public repos lands in a review inbox
(a branch on your fork plus a summary), and nothing public goes out without you (D29).

## 9. UI

Clean amber on dark, professional, a desktop tool rather than a mobile app or a TUI (owner,
2026-11-02: "cleaner but NOT a TUI ... we are using flutter so lets use it"). The model is a polished
developer tool (Linear, Raycast, VS Code, Zed): calm, dense, keyboard friendly.

- Inter for the interface and JetBrains Mono for code, transcripts of tools, ids and output; a small
  type scale (12, 13, 14, 16), sentence case, no all-caps console look and no box drawing;
- graphite surfaces with hairline borders and restrained radii (6 for controls, 8 for cards); no
  bezels, glow, gradients or seven-segment digits;
- amber is the accent and the brand; every other colour carries a meaning (info links and selection,
  ok healthy or active, warn waiting on you, alarm failed or unreachable);
- thin bar meters with zone colours, status dots and pills;
- a persistent sidebar (navigation, the workspaces/sleeves tree, hub status and quota at a glance),
  F-keys bound to the screens, and a command palette (Esc or Ctrl+K) that takes the same commands as
  the CLI;
- hover, focus and pressed states and tooltips on everything clickable;
- below 720 px a phone layout: a top bar, the sidebar as a drawer, the same screens.

A light theme exists for daylight; dark is the default.

Chat comes first (D38). The main screen is Envoy's chat, with a workspaces/sleeves tree beside it.
Choosing a sleeve opens its chat log, built from the headless transcript, in the same simple pane
layout herdr uses. Approvals appear inline in the chat and on the phone. Mail and events, nodes, and
quota each have their own screen. A taken-over terminal (xterm.dart) is reached from a sleeve's
chat, never as the default view. The ten rules and an ASCII layout are in the
nodes report section 7.

## 10. Verus (removed)

Phase 1 (D28) proved the journal codec, its torn-tail recovery rule, CRC-32C and the node frame
codec. The proofs confirmed code that was already correct and found nothing the fuzz targets and
e2e tests had not, so they were removed on 2026-11-02 (D40). The owner (verbatim): "we can also
take it easy on the fuzz testing - were not building a rocket for nasa (this goes the same for
verus, if we're not actually finding much we can just rip it out and update rust toolchain)".

What stays:
- `overflow-checks = true` in release for the parser and writer crates.
- `#![forbid(unsafe_code)]` everywhere except the explicitly exempted OS FFI crates (sandbox,
  presence; D19).
- No pinned Rust toolchain: the workspace builds on the current stable release.

Superseded (2026-11, Zig rewrite): the Rust rules above are gone with the Rust workspace. What
replaces them: every build is `-Doptimize=safe`, so bounds and overflow checks stay on in the
parser and writer code; `src/tiger.zig` assertions fail fast; and the only foreign calls
(`extern` declarations) are in `src/sandbox/exec.zig`, `src/node/sys.zig` and
`src/node/service.zig` (D19). There is no Verus and no fuzz harness; VOPR and the e2e script are the
tests (section 4).

## 11. Salvage map

Superseded (2026-11, Zig rewrite): this was the plan for rebuilding in Rust. The Zig program
reimplemented from it; the table is history, and the `references/` clones still apply.

| From | Take |
|---|---|
| `dev-hotschmoe` Rust | proto and wire generation, `pd1.`/`pc1.` auth, quota readers and `suggest.rs`, the four drivers (with the dispatch-before-record and Codex timeout fixes), the sandbox helper trimmed to the light profile, screen-rule tables, chatlog/handoff fact gathering, fuzz targets, release scripts |
| `4f199cc^` (before the fleet removal) | Rust node inventory and sample collectors, Windows runner account |
| `bc9b87a^` (Go tree) | lessons only: tailnet supervisor, DERP cert fix, disco ping on start, headscale backup gate |
| `references/herdr` | screen manifests (Apache-2.0), debounce, badges, the wait semantics, fd handoff on update |
| `references/agent` | uncertain-effect states, record before dispatch, steer vs queue |
| `references/mainplane` | continue-by-retrieval as the future compaction model; fixed member UDP port |
| upstream | headscale v0.29.7, tailscale.com via headscale's go.mod |
| `references/dsh-superagent-prototype`, `references/autoresearch`, `references/rea` | nothing for now: reading material only (owner, 2026-11-01) |

## 12. Conflicts between reports, resolved here

- **Enrollment:** scan to join (the network report) replaces the "code shown on the phone" sketch.
  That sketch needed an unauthenticated node-to-hub endpoint.
- **CLI off the hub:** the nodes report keeps the CLI on the hub only; the network report allows a
  CLI or MCP on user machines with an agent token. Resolved by the owner: the CLI stays on the hub,
  and vendor general agents are installed there too (D9).
- **Linux node service:** a system service (the nodes report, needed for `/dev/input` and cgroups)
  over a user service (the network report's install sketch). Decision D10.
- **Egress proxy:** the sleeves report cuts it, so sleeves reach the hub API on loopback directly,
  not "via the egress proxy" as the network report's principal table says.

## 12a. Corrections from adversarial review (Codex, 2026-11-01)

These rules override the design reports wherever the reports say otherwise. The full review is in
`docs/design/review-codex-2026-11-01.md`.

1. **Resleeve waits for death.** Quiescing moves to Capturing only once the old incarnation's
   scope is confirmed empty. That means a stop, then `KillScope` after 30 s, then a probe. Nothing
   is checkpointed or launched while the old process tree can still write to the workspace. This
   is I5, applied to resleeve.
2. **An absent scope is not "never started".** `StartIncarnation` writes a launch receipt (scope
   name and pid) into the journal as soon as the spawn returns. On recovery:
   - no receipt and no scope: NotApplied;
   - a receipt and no scope: started and gone, which goes to Recovering and native resume;
   - anything else: Uncertain.
3. **A rejected prompt returns activity to Idle.** A definitive Nack on `SendPrompt` moves
   Working back to Idle, reports the failure to the sender and delivers the next queued item.
   Activity follows the effect's resolution, not only harness events.
4. **A vanished approval is not an applied approval.** `RespondApproval` is Confirmed only by
   the harness's own acknowledgement or a matching decision in its state. "No longer pending"
   stays Uncertain.
5. **Node job tombstones outlive the hub's uncertainty.** The node keeps a tombstone for every
   `(job, attempt)` until the hub acknowledges collection, not for a fixed hour or count. An
   "unknown job" answer counts as never-seen only when the node's journal epoch matches the one in
   the hub's submit record. Otherwise the job is Uncertain and needs an explicit new attempt.
6. **Cancelling has a timeout.** If no status arrives within the 5 min lease, Cancelling moves to
   Lost, and a later status reconciles it. No state may wait forever on a node. A collection is
   abandoned 24 hours after the job is first lost, even when the node keeps reporting and
   collection starts again, and a failed local save fails that collection. The owner is told
   the job is lost once per episode, not again when that collection loses the link.
7. **Every non-terminal state has a recovery action.** Probing re-probes. Collecting re-fetches
   from its recorded offset. Cancelling re-sends the cancel. Provisioning waits for its completion
   barrier and then records or removes the worktree. Stop during Provisioning waits for that
   barrier too. The fuzz targets insert a hub crash at every journal boundary. Superseded (2026-11,
   Zig rewrite): VOPR crashes the simulated store and replays the journal instead
   (`src/dst/sim_storage.zig`).
8. **The control-key pin is enforced on every start.** Superseded (2026-11, Zig rewrite): the node
   pins the hub's Noise public key and fingerprint from its join token, and the handshake fails
   closed against any other key (`src/node/serve.zig`, `src/net/noise.zig`); there is no `/key`
   fetch. The original text: before starting tsnet, `member` fetches
   `/key` itself and compares it with the fingerprint pinned at enrollment. On a mismatch it fails
   closed and reports "control key changed", so it never silently re-registers.
9. **Installers are fetched by commit, not by tag.** The README one-liner uses the raw URL at a
   full commit SHA, which is immutable, and prints that script's sha256 beside the line. The
   script then checks `SHA256SUMS` before running anything it downloads.
10. **Node results are attestations.** A cross-arch CI status says "node X reported exit 0", with
    the node named. A release asset is built only by the hub, or by two nodes whose
    outputs are bit-identical.
11. **The light-isolation threat model is explicit.** The sandbox protects against accidents, not
    against a malicious agent or prompt injection. A sleeve can read unmasked host files, reach the
    LAN and write the shared download caches. The scoped PAT is limited to the repos that sleeve
    needs. If hostile code is ever in scope, D12 must be revisited before that work runs.
12. **Count runtime processes as well as deliverables.** A running hub is three processes: hub,
    headscale and member. A node is two: agent and member. Any further runtime process needs an
    explicit decision. Superseded (2026-11, Zig rewrite): the hub is one process and the node is
    one (a node run as a service has a supervisor beside it); sleeves and jobs are children.

## 12b. Corrections from adversarial review (Grok, 2026-11-01)

The full review is in `docs/design/review-grok-2026-11-01.md`. Findings that duplicate 12a are
not repeated here.

1. **Unsafe effects never retry, by any path.** An Unsafe effect leaves Uncertain only to
   Confirmed, Fenced or Abandoned. For Unsafe kinds, a "not applied" probe result counts as
   Unknowable. I8 now covers the whole graph, not only the direct Uncertain-to-Intent edge.
2. **One owner for starting incarnations.** The guardrail machine decides resume or resleeve,
   and the sleeve machine is the only one that emits `StartIncarnation`. On `HubRecovered`, if
   probe(g) still finds live tasks, the hub runs `KillScope(g)`, confirms the scope is empty, and
   only then starts g+1. A live g is never left beside a new g+1. SPEC section 3.3 "resumes
   natively" is subject to G3's limit of two resumes in 10 minutes, then a forced resleeve.
3. **Delivering is a state.** After `TurnSettled`, `Ready` or `CompactDone` with a non-empty
   queue, activity goes to `Delivering{id}`, not Idle. `apply` dequeues the message and keys the
   send by the message's `request_id`. Only one delivery is in flight per sleeve. Activity leaves
   Delivering only when that effect resolves.
4. **Executors are serial per sleeve, behind an fsync barrier.** Intents are released only after
   their journal batch is fsynced. Each executor re-checks `g` or `attempt` immediately before the
   syscall, and drops the intent if it is stale.
5. **Headscale is reachable only from Traefik.** Superseded (2026-11, Zig rewrite): there is no
   headscale; the fleet listener takes only Noise handshakes from approved keys (section 3.2). The
   original text: headscale binds the hub's LAN address, and one
   host-firewall rule on the hub admits only stronghold's IP to that port. A node dialling the LAN
   address directly cannot reach `/register`, `/swagger` or anything else.
6. **No GitHub token inside sleeves (D20 changed).** A sleeve asks the hub to push its branch
   with `protectorate git push`, and the hub, which holds the token, does the push. This is
   deterministic, audited and limited to the sleeve's own branch. `gh` read-only commands work
   with a read-only token. Sleeves keep the host network (D12), but there is no push token in that
   network-facing process.
7. **The resleeve brief prompt must land.** The continuation prompt is keyed
   `resleeve:<name>:<g+1>`. The sleeve reaches Running only once that prompt is Confirmed, or is
   found in the new session's native log. If that cannot be determined, the sleeve parks and the
   owner gets mail. No queued message goes first.
8. **The brief template gains two required sections:** "Effects that may have been applied" (the
   Uncertain and Fenced records) and "Child roster" (live children, their state and last report).
   History report section 4.5 is updated by this rule.
9. **Node jobs survive a hub crash.** On `HubRecovered`, Submitting becomes SubmitUncertain and a
   `Status` is reissued. The node fsyncs its journal before exec. Timers re-arm from the journal.
10. **Parking the root freezes its children.** G10 (budget) and G12 (disk) on a root stop sending
    to its children. A child's budget is checked live against the root's remainder, not only at
    spawn. For roots and children, each Git directory has `hooks/`, `config`, `config.worktree` and
    `info/` bound read-only; G11 walks the resolved child gitdir, not only the worktree diff.
11. **Task size is not an LLM ranking input (new D27).** `select_target` takes a size class from
    code: the median cost of the last N tasks in that workspace, or an owner-set class. Envoy may
    request one class up or down, and code clamps it.
12. **Who reads history (new D26).** `history.read` covers Envoy, plus a continued sleeve for its
    own `run_id`. Wider search is owner-only.
13. **The installer source is decided.** It is a GitHub raw URL at a commit SHA (12a item 9), with
    scan to join. The hub-served `node.sh` in the nodes report section 5 is superseded.
    Scan-to-join keeps the shape the owner liked: the new device shows something and the phone
    approves.
14. **Smaller fixes:**
    - Activity splits `NoMatch`, which goes to Unknown, from `Desync`, which trips G14. A changed
      TUI cannot get a pane killed.
    - Gemini CLI is dropped (D35); Antigravity is pane-only and stays out of `resleeve_to`.
    - `doctor` drops its tmux and egress checks.
    - Mail delivery becomes an effect kind (`MailDeliver`, Keyed by message id) under the effect
      machine.
15. **Vendor general agents get a client path that works today.** Superseded by the owner on
    2026-11-01: they run on the hub machine and only talk to Envoy (section 3.1). There is no
    remote MCP, no agent token and no `protectorate-node client`.

## 13. Decisions (owner, 2026-11-01)

All decided. The owner's answers are recorded in `docs/sessions/2026-11-01-decisions.md`.

| # | Decision | Outcome |
|---|---|---|
| D1 | Headscale inside the hub binary but outside the process (sandboxed child) | yes. Superseded (2026-11, Zig rewrite): no headscale; nodes dial the hub's fleet listener over Noise IK (3.2) |
| D2 | Build headscale from the pinned upstream source, or embed the release binary | official headscale, built locally from the pinned tag. Superseded (2026-11, Zig rewrite): nothing is embedded or built from Go; `go/` is unused |
| D3 | Split-horizon DNS for `protectorate.hotschmoe.com` at both sites and in the phone's WireGuard DNS | yes |
| D4 | Agent VLAN for nodes, excluded from Traefik's API router | later |
| D5 | Scan to join with a QR plugin in the app (paste also works) | yes |
| D6 | Agent tokens for vendor general agents | **dropped**: general agents get the envoy-only local capability instead |
| D7 | Children survive a root crash; die on root Stop/Remove | yes |
| D8 | Child workspace: git worktree by default | yes |
| D9 | Client path for vendor general agents | **on the hub machine**, `protectorate envoy ...` or the stdio `mcp --envoy`; talk to Envoy only; Envoy asks the owner before big, heavy or dangerous work |
| D10 | Linux node as a system service (one sudo at install) | yes |
| D11 | Windows node as LocalSystem plus runner account | yes |
| D12 | Cut the sandbox network namespace and egress proxy | yes |
| D13 | Antigravity pane-only until headless approvals work | yes. Superseded (2026-11, Zig rewrite): `src/drivers/antigravity.zig` runs `agy` headless (stream-json) |
| D15 | Resleeve target: ordered list per sleeve, default `[pi_local]` | yes |
| D16 | Guardrail thresholds as listed; no auto-revert on forbidden-path edits | yes |
| D17 | Hub updates by re-exec in place, keeping sleeves alive | yes. Superseded (2026-11, Zig rewrite): `hub update` stages the release and moves the `current` link; the service restarts the hub and sleeves resume as after a crash (6a) |
| D18 | The hub never types into a pane after launch | yes |
| D19 | FFI `unsafe` exemption to Verus rule 6 for sandbox and presence crates | yes. Superseded (2026-11, Zig rewrite): no Rust crates and no Verus; the foreign calls are the `extern` declarations in `src/sandbox/exec.zig`, `src/node/sys.zig` and `src/node/service.zig` (section 10) |
| D20 | Push token location | hub only; sleeves push through `protectorate git push`. Superseded (2026-11, Zig rewrite): the command is `protectorate sleeve push <id> [--remote R] [--branch B] [--force]` |
| D21 | Index the owner's own interactive CLI logs too, or hub-launched sessions only | hub-launched only |
| D22 | Vectors off until keyword search falls short | yes |
| D23 | Brief file `.protectorate/BRIEF.md`, LLM summary for planned resleeves only | yes. Superseded (2026-11, Zig rewrite): the hub writes `<home>/briefs/<sleeve>.md` and builds no LLM summary (3.7) |
| D24 | `===>` command line and F-key legend in the app | yes; since 2026-11-02 a command palette (Esc, Ctrl+K) and F-keys shown in the sidebar (SPEC 9) |
| D25 | macOS node and desktop only once a Mac exists | yes |
| D26 | `history.read` scope | Envoy, plus a continued sleeve for its own run; wider is owner-only |
| D27 | Source of the task-size class for dispatch | code (median of the last N), Envoy may nudge one class, code clamps. Superseded (2026-11, Zig rewrite): `src/quota/select.zig` takes a `TaskSize`, but nothing computes it and the hub does not call the selector yet; resleeve walks `resleeve_to` in order (section 8) |
| D28 | Verus phasing: phase 1 is journal and frame codec; presence, audit and brief math dropped | yes. Superseded (2026-11, Zig rewrite): no Verus; the journal and frame codecs are covered by VOPR (`src/dst/`) and the e2e script |
| D29 | Backlog with a spare-quota trigger in code and a review inbox for public repos (PROBLEMS P6) | yes, after resleeve works |
| D30 | GPU/resource leases with a TTL (PROBLEMS P9) | later |
| D31 | A network spike before the core | **no separate spike**: build straight away and fix roadblocks as they come |
| D32 | One kind of node; presence gates jobs | yes |
| D33 | Harnesses install and update natively, never pinned | yes |
| D34 | Pi as a standalone binary, updated natively | yes; check every 24 h, `pi update self` preferred |
| D35 | Gemini CLI | **dropped**; Antigravity CLI replaces it, as a standalone binary |
| D36 | Release signing key | **none for now**; releases built locally and checked against SHA256SUMS; revisit |
| D37 | Third-party dependencies | as few as possible, pinned exactly, forked and trimmed when large; listed in `DEPENDENCIES.md`. Superseded (2026-11, Zig rewrite): the Zig program has none: `std` only (principle 6) |
| D38 | Primary interface | chat with Envoy; sleeve chats next; terminal takeover is last resort |
| D39 | Hub-to-node transport: headscale and member, or plain SSH | **headscale for now**; SSH logged in REVISIT.md (`serve --stdio` stays as a tested path). Superseded (2026-11, Zig rewrite): neither: our own Noise IK link over TCP that the node dials (3.2); `serve --stdio` runs the same session over a pipe on the node side, with no hub-side stdio acceptor yet |
| D40 | Keep Verus past phase 1 | **removed** 2026-11-02: the proofs found nothing fuzzing and e2e had not; frees the Rust toolchain. Superseded (2026-11, Zig rewrite): there is no Rust toolchain at all now (section 10) |

## 14. Build order

No separate network spike (D31): roadblocks get fixed when they are hit. Stages may run in
parallel where they don't touch the same code. dev-hotschmoe is unused (owner, 2026-11-01), so
nothing has to stay compatible with it.

1. **Core:** the workspace skeleton, API schema, journal codec, `core::apply` for lifecycle,
   activity and effects, and the `sleeve_core` fuzz target with its reference model. No harness yet.
2. **Chat and one harness:** the headless Claude driver on the effect record, Envoy as a root
   sleeve, `protectorate envoy send|wait|read`, and the Flutter shell in the new look with Envoy's
   chat first. Terminal takeover comes after the other headless drivers.
3. **Resleeve:** guardrails, the history index, the continuation brief, `pi_local`, and forced
   resleeve end to end.
4. **Network:** the `go/` module, embedded headscale and member, Traefik routes, scan to join, and
   inventory and presence.
5. **Jobs:** node jobs, cross-arch CI, `doctor`.
6. **Hardening:** the stdio `mcp --envoy`, terminal takeover and the remaining harnesses.

Superseded (2026-11, Zig rewrite): all six stages were rebuilt in Zig, and stage 4 became the Noise IK
fleet link (3.2), not the `go/` module with headscale and member. The remaining network work is
`docs/design/tigerstyle-network.md`.

Proposed follow-on design, awaiting owner review: [background gates, releases and a hub-host node](docs/design/ci-and-release.md). Current SPEC rules remain in force until amended.

## 15. Implementation choices (2026-11-01)

How the code is built, chosen to keep third-party code near zero (D37). Each choice that is a
shortcut is also in `REVISIT.md`.

- **No async runtime.** The hub serves a few clients and a few dozen sleeves, so it uses std
  threads and channels:
  - one core thread owns all state and runs `apply`;
  - each connection, child pipe and node link gets its own thread;
  - timeouts use deadlines on sockets and `poll`.

  This avoids an async runtime and an HTTP framework (tokio, hyper and axum in the Rust plan). The owner allows them if they turn out to be needed
  (2026-11-01), "incredibly deliberate and cautious", but prefers a performant build without them.
  The fallback is a pinned, trimmed fork, chosen only after our own code has measurably failed.
- **Our own code** for what we would otherwise import. Superseded (2026-11, Zig rewrite): the crate
  table became the `src/` groups below. Every file has a tier, L0 (no local imports) to L5 (entry
  points, `src/dst/`, `src/tools/`), and imports only point down; `make check-levels` enforces it.

  | Group | What it does |
  |---|---|
  | `src/net/` | Noise IK (`noise.zig`), the frame codec (`frame.zig`), the node protocol messages (`msg.zig`), the TCP and stdio transport with the roaming dial (`transport.zig`) |
  | `src/journal/` | the append-only journal: length, CRC-32C and payload frames, torn-tail recovery, 64 MiB segments |
  | `src/core/` | the state machines and `apply`, with no I/O, plus the admission authority for node job leases |
  | `src/api/` | API types and the method table in `schema.zig`; `src/tools/apigen.zig` writes the Dart models from it |
  | `src/drivers/` | headless harness adapters (Claude, Codex, Grok, Pi, Antigravity) into one event stream, and the child session |
  | `src/history/` | the history index (3.7): native session logs and hub transcripts into checksummed segments with BM25, in place of SQLite FTS5 |
  | `src/quota/`, `src/update/`, `src/doctor/` | quota readers and the selector, release staging and probation, the doctor checks |
  | `src/sandbox/` | the bwrap and Landlock argument builders and the process guard |
  | `src/hub/`, `src/cli/`, `src/node/` | the hub daemon (HTTP and WebSocket server, fleet server, runtime), the CLI and stdio MCP, the node daemon (inventory, presence, jobs, supervisor) |
  | `src/dst/` | the VOPR simulator and its oracle |

  There is no SQLite: `src/history/` replaces it (3.7). Nothing is fuzzed against a reference;
  VOPR and the e2e script are the tests (section 4).
- **The API** is JSON over HTTP:
  - `POST /api/<Method>` carries the request and response bodies;
  - `GET /api/events` is a WebSocket of state changes. A client resumes with `from_seq`
    one greater than the last kept sequence it received. If more than 500 kept messages
    are missing, the cursor is unavailable, or no cursor is supplied, the hub sends a fresh
    snapshot instead of replaying the gap; its sequence identifies the cursor it covers.
    Streaming deltas repeat the last kept sequence and are not replayed. A `live` marker
    follows the replay or snapshot before newer stream messages; its sequence is the last
    kept sequence at that boundary and does not allocate a new sequence. If a live subscriber
    falls behind the ring, a snapshot and another `live` marker replace the lost gap. Clients
    publish the caught-up state only after `live` and discard an interrupted catch-up. Entry
    messages are sent only for sleeve IDs in the connection's comma-separated `transcripts`
    query parameter. Other messages retain their original sequence numbers, so clients must
    allow gaps. A client changes subscriptions by reconnecting and loads each open transcript
    after `live` to cover the changeover;
  - the loopback listener serves the CLI and sleeves; the trusted-proxy listener serves Traefik.

  Superseded (2026-11, Zig rewrite): there is no unix-socket listener. Every events connection
  starts with a `snapshot` and a `live` marker, whatever `from_seq` says; the hub then sends the
  messages in its ring of 512 whose seq is above `from_seq` (or above the snapshot's seq when none
  is given). The `transcripts` parameter is ignored, so entry messages for every sleeve are sent. A
  subscriber that falls behind the ring loses the gap with no new snapshot, and the stream's seq is
  in memory and restarts at 1 when the hub restarts (REVISIT, "Event stream sequence").

  There is no protobuf, buf or Connect. The Dart client is generated from `src/api/schema.zig`
  (`make gen`) and uses `dart:io`, or `dart:js_interop` fetch and WebSocket on the web.
- **Outbound HTTPS** (GitHub releases, quota readers) runs the system `curl`. There is no Rust TLS
  stack. Superseded (2026-11, Zig rewrite): it runs `std.http.Client` with the Zig standard library's
  TLS; nothing spawns `curl`.
- **Sleeve sandbox** is bwrap plus a `systemd-run --user --scope` per incarnation. Superseded
  (2026-11, Zig rewrite): see 3.4; there is no per-incarnation systemd scope, and the git
  directory protections in the next sentences are not built: the git directories are bound writable.
  Git directories of root checkouts and linked worktrees have `hooks/`, `config`,
  `config.worktree` and `info/` bound read-only, including the shared common directory.
  Missing entries are created empty before binding so they cannot be created by a sleeve;
  this leaves harmless empty metadata on the host. Symlinked protected entries are refused.
  Hooks and configuration can execute host commands. `info/attributes` can name a filter
  but cannot define one; read-only configuration prevents introducing a filter driver, and
  `info/` is also protected to prevent changes to host Git behavior. The remaining Git state
  stays writable for ordinary work. A root linked worktree resolves and validates its private
  and common Git directories just like a child. The common directory’s `worktrees/`
  is read-only except for the current worktree’s separately protected private gitdir, so
  another worktree’s configuration cannot be changed through the common directory.
- **Go** (`go/`): `cmd/headscale` builds the official headscale at the pinned tag, unmodified;
  `cmd/member` is our tsnet glue. `member` exposes a unix socket where the Rust side asks it to
  dial `node:7475` (hub), or where accepted tailnet connections are handed over (node).
  Superseded (2026-11, Zig rewrite): there is no Go in the build or at run time; `go/` stays only as
  reference. The node dials the hub through `src/net/transport.zig`.
- **Envoy** is a root sleeve running Claude Code headless in `$PROTECTORATE_HOME/envoy/`. That
  directory's own `CLAUDE.md` describes the `protectorate` CLI, so no worked-on repo is touched.

The blocking gate is `make check`; `make e2e` runs its real-binary scenarios and `make dst` the VOPR run. Superseded (2026-11, Zig rewrite): `make test`, `make test-heavy`, fuzz seed replay and the
`#[ignore]`d scenarios are gone; there is no background gate.
