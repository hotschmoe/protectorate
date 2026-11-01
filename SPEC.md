# Protectorate SPEC (next)

Status: decided 2026-11-01 (owner answers in `docs/sessions/2026-11-01-decisions.md`); development
has started. Motto: **make it work, make it right, make it fast**, in that order. A shortcut taken to
make something work is logged in `REVISIT.md` with what "right" would be. This file states the design intent for the rebuilt Protectorate on branch
`next`. The reasoning, citations and rejected alternatives are in `docs/design/`. The owner's own words
are in `docs/sessions/2026-11-01-redesign.md`. When this file and a design report disagree, this file
wins, and the conflict is listed in section 12.

| Report | Covers |
|---|---|
| `docs/design/network-trust-language.md` | embedded headscale, one-way trust, enrollment, TLS, language |
| `docs/design/sleeves-state-machines.md` | sleeve tree, isolation, harnesses, panes, Pi, guardrails, state machines |
| `docs/design/history-continuation.md` | history index, resume, continuation brief, mainplane findings |
| `docs/design/nodes-ci-deliverables-verus.md` | node daemon, presence, jobs, git/CI, deliverables, doctor, install, Verus, UI |

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
4. **Every device is untrusted.** The hub may reach and execute on nodes. A node cannot open a
   flow to the hub API or to any hub listener. What a node does send is named and bounded:
   - control and DERP traffic to headscale, a separate sandboxed process holding none of the
     hub's secrets;
   - WireGuard and disco packets to the hub member, which has no listener;
   - answers on connections the hub opened.

   Every byte from a node is parsed as hostile, and a node's job result is that node's claim, not
   proof (sections 12a, 12b).
5. **Record before effect.** Every write to a harness or node is an effect record, fsynced before the
   first byte leaves. An unconfirmed write is Uncertain, never silently retried or dropped.
6. **Fewest binaries and fewest dependencies we don't own.** Two binaries plus the Flutter apps.
   Our own crates can be as many as the design wants. Third-party code is the supply-chain risk:
   - a third-party crate, Go module or Dart package needs a reason that a few hundred lines of our
     own code cannot meet;
   - every one is pinned exactly (`=x.y.z`, a Go tag plus go.sum, an exact pubspec version), and
     nothing updates without a hand-picked bump;
   - where a dependency is large or only partly needed, we fork it under our control and trim it;
   - no Python or Node in the product (the pi_durable experiment is the one exception, D14);
   - the allowed list and its reasons live in `DEPENDENCIES.md`, and `make check` fails on a crate
     that is not listed.
7. **LAN only.** `protectorate.hotschmoe.com` resolves and answers only on the LAN and over the
   owner's WireGuard. WAN access is a later, separate design.
8. **Repo rules carry over** from `dev-hotschmoe` CLAUDE.md: no emojis, full migrations with no
   shims, real-binary e2e and fuzz tests only, no hand-edited generated code.
9. **Verus in narrow scope** under the owner's eight rules (section 10).

## 3. Layers

### 3.1 Clients

| Client | Reaches the hub by | Credential |
|---|---|---|
| Flutter desktop (Linux, Windows), Android, web | HTTPS through Traefik | `pd1.` device token |
| Owner CLI on the hub, Envoy and sleeves | loopback | `pc1.` capability (sleeves), owner token (CLI) |
| Vendor general agents (GrokBot, Claude Duet, ChatGPT Dot) | installed **on the hub machine**, running `protectorate envoy ...` through their own shell tool, or the stdio `protectorate mcp --envoy` | the local **envoy-only** capability: it can talk to Envoy and read Envoy's replies, nothing else |

- **Vendor general agents talk to Envoy, never to the API** (D9, D6 changed). They get no hub
  token and no network path. The owner installs the vendor app on the hub, logs it in, and tells it
  to use `protectorate envoy send|wait|read`. The narrow stdio MCP (`protectorate mcp --envoy`)
  offers the same three tools for apps that prefer MCP. We write no per-vendor connector.
- **Envoy holds the power.** Envoy (a root sleeve) uses the full CLI over loopback for sleeves,
  nodes and jobs. When a general agent asks for something big, heavy or dangerous, Envoy raises an
  approval that reaches the owner's phone and waits for it. A code rule makes this mandatory for a
  list of actions (removing sleeves, node maintenance jobs, pushes to protected branches), so it does
  not rest on Envoy's judgment alone.
- **The CLI is the API.** Every CLI command maps onto one hub API method. Envoy and shell-capable
  sleeves use the CLI.
- The web build is embedded in the hub binary and served from the same origin.

### 3.2 Network

- **Official headscale, pinned (v0.29.7 at the time of writing) and unmodified**, built in CI from
  upstream source. The hub binary embeds it (gzipped, about 18 MB) and runs it as a supervised,
  sandboxed **child process** from a sealed memfd. It is inside the hub binary, not inside the hub
  process: headscale's `Serve` takes over signals and calls `log.Fatal`.
- Administration goes through headscale's own CLI over its unix socket with `-o json`. No API key
  is ever created.
- **Go is limited to one module, `go/`**, with two mains, both `CGO_ENABLED=0`:
  - `cmd/headscale`: the upstream main;
  - `cmd/member`: our tsnet glue, about 500 lines.
- The hub and every node run `member` (userspace tsnet: no TUN, no root, no tailscaled) as a child
  of the Rust process.
- **Policy:** one rule in a static embedded file, `tag:hub -> tag:node:7475`. Nodes cannot request
  tags. The hub member only dials and has peerAPI compiled out. The hub API never listens on the
  tailnet; the listener check refuses `100.64.0.0/10`.
- **Remaining surface, by design:** nodes must reach headscale's control and DERP endpoints. That is
  why headscale runs sandboxed with none of the hub's secrets. Its compromise would mean control of
  the nodes, but not of the hub's state or tokens.
- **TLS:** Traefik on stronghold terminates TLS for `protectorate.hotschmoe.com` with its existing
  certificate.
  - One router sends headscale's control and DERP paths to headscale.
  - Everything else goes to the hub's trusted-proxy listener.
  - DERP is headscale's own, with an IP-pinned map and STUN off. No public DERP.
  - The ts2021 and DERP upgrades through Traefik are the first thing to prove.
- **Enrollment, "scan to join":**
  1. The README one-liner installs the node and runs `protectorate-node join <url>`.
  2. The node shows a QR code of its headscale registration plus a control-key fingerprint.
  3. The owner scans or pastes it in the app, names the node and approves.
  4. The hub checks the fingerprint, then registers, tags and renames the node through the
     headscale CLI.

  No secret leaves the hub, and nothing on the node calls a hub endpoint.

### 3.3 Hub

One Linux machine, one `protectorate` process plus its headscale and member children. It holds:

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
- **Mail:** delivery, re-delivery and escalation.
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
  - One root per directory, never two. Writable claims never overlap.
  - Children are depth 1, at most 8 live per root. A child gets its own git worktree and branch by
    default, or a read-only view of the root.
  - Stopping or removing the root stops every child in the same step. A root crash, resume,
    resleeve or park leaves the children running.
  - Native subagents run inside the incarnation, and we don't manage them.
- **Light isolation** (it prevents accidents; it is not a security boundary against a hostile
  agent):
  - user, mount, PID, IPC and UTS namespaces;
  - the host root bound read-only, with the workspace and a private home writable;
  - a sized `/tmp`;
  - tmpfs masks over the hub's state, `~/.ssh` and other harnesses' credentials;
  - a systemd scope per incarnation with limits and `cgroup.kill`, plus the orphan reaper.

  Cut from today's sandbox: the network namespace, the egress proxy, cache overlays, nested cgroup
  delegation and the pin-approval gate. Sleeves share the host network and the owner's download
  caches. Size: about 4.8k lines today, about 1.2k after.
- **Harnesses:**

| Harness | Interface |
|---|---|
| Claude Code | headless stream-json (with `--replay-user-messages` for prompt ack), or pane |
| Codex | headless `codex app-server`, or pane |
| Grok Build | headless `grok agent stdio` (ACP, with `--always-approve` when unsupervised), or pane |
| Antigravity CLI | replaces Gemini CLI, which is dropped (D35). Pane only, until approvals work headless and plan policy is clear. A standalone binary is required; if it only ships through npm, it waits |
| Pi, two sleeve kinds | headless `pi --mode rpc`, run as the standalone binary so the hub needs no Node. **`pi_vendor`** offers any login or API key Pi supports, from Pi's own provider catalog: xAI, OpenAI (ChatGPT sign-in), DeepSeek, Kimi/Moonshot, GLM (Z.ai), MiniMax, Qwen, OpenRouter and more. The sleeve picks provider and model. **`pi_local`** points at a local OpenAI-compatible endpoint. Anthropic through Pi bills as extra usage and is labelled so. Gemini and Antigravity logins are not offered (removed upstream, ban risk) |
| Pi Durable | a time-boxed experiment: a ~500-line TypeScript bridge on `pi/durable` (Node >= 22.19), adopted only if it passes the kill tests in the sleeves report 1.7 |

- **Default tools:** none beyond what each harness ships, plus our CLI mounted in the sandbox.
  Native tools are not replaced.
- **Harness installs and updates are native** (D33). Each harness is installed by its vendor's
  official method, a standalone binary wherever one is offered, and updates through its own
  updater (its built-in auto-update or its own `update` command). We never pin, patch or repackage
  a harness. Rules in code around that:
  - an update never touches a running sleeve: the running process keeps its binary, and the next
    launch or resume picks up the new one;
  - the hub runs a harness's own update command only when no sleeve of that harness is mid-turn;
  - `doctor` records the version and install method per harness, so a behavior change can be
    traced to the version that brought it;
  - a failed launch after an update marks that harness ineligible in `doctor.json` until a
    launch succeeds, and resleeve skips it.
- **The update check runs once every 24 hours** per harness, through the harness's own command.
  On the dev hub on 2026-11-01 they are `claude update`, `codex update`, `grok update` and
  `pi update self`. A harness without one is updated by its official installer, re-run.
- **Pi is a standalone binary, not an npm package** (D34), updated with `pi update self`. If that
  command can't update a standalone install, the hub fetches Pi's GitHub release itself, verifies the
  checksum and stages it beside the old one. Pi Durable is the one path that needs Node, and Node is
  accepted for it (D14).
- **Codex** runs as its standalone release binary, not through the npm wrapper.

### 3.5 Nodes

- **One kind of node** (D32). Node tiers are gone: the only reason for them was to keep jobs off
  machines a person is using, and presence detection does that directly. Every node takes jobs
  when idle and refuses them while someone is at it.
- **One node binary** per OS. It runs as a system service on Linux and as a LocalSystem service with a
  runner account on Windows. It also builds for Android (Termux), and for macOS once a Mac exists.
  All RPCs are hub-initiated on tailnet port 7475.
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
  - the node journals each job before exec, and limits each one: 1 h by default, 64 MiB of log,
    and a per-job cgroup or Job Object;
  - the hub pulls output as length-prefixed frames;
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
     forced resleeve are one flow. The file is `.protectorate/BRIEF.md`.
  3. **Search:** `protectorate history search|read` over a keyword index. The brief already carries
     the top hits, because the forced-resleeve target (pi_local) is the weakest reader.
- **Storage:** our own keyword index, crate `pt-history`, in a directory of checksummed,
  append-only segments plus a manifest of source cursors. It is a derived, rebuildable index;
  native logs stay authoritative and are only read.
  - Reason: SQLite FTS5 through `rusqlite` would bring a third-party crate plus bundled C SQLite
    for one feature, against the minimal-dependency rule (D37). Owner decision 2026-11-01; this
    overrides `docs/design/history-continuation.md` 1.4, 4.2 and 4.7.
  - What it does: a tokenizer for paths and identifiers (camelCase and snake_case parts),
    positional postings, BM25, all-terms and phrase queries, and filters by sleeve, harness,
    session, time and role. Ingest tails each log by byte offset; small segments are merged.
  - Measured on this hub (2026-11-01, release build): 1 GiB of the newest real logs ingested
    in 1.9 s (540 MB/s, 48 MB peak RSS) into an 11 MB index; queries take 0.05 to 0.7 ms.
  - Fallback: `rusqlite` with FTS5, if our index proves insufficient (REVISIT.md).
- **Vectors:** off until keyword search proves insufficient. When turned on, they come from an
  external llama.cpp server over HTTP, stored as f32 blobs with brute-force cosine in Rust (bge-small
  first). No new Rust dependency.
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
  `[pi_local]`). A third resleeve within an hour parks the sleeve.
- **Testing:** each machine is a pure function with a fuzz target. The target checks invariants I1
  to I10 against a table-driven reference model and checks `replay(journal) == live`.

## 5. Code decides, Envoy decides

| Code | Envoy |
|---|---|
| dispatch by quota (at spawn and at guardrail time only), retries, probes, guardrails, resleeves and target choice, mail, cascades, budgets, approval timeouts, node scheduling, brief facts | goal decomposition, briefs, repo and workspace choice, task-size estimates, reading reports, reviewing diffs, answering sleeves, what spare quota should go to |

Envoy is an ordinary root sleeve. It never ranks harnesses, retries, or runs a guardrail resleeve.
Code never resleeves a healthy idle sleeve just to chase quota, because that busts its prompt cache.

## 6. Deliverables and build

```
protectorate-linux-{amd64,arm64}       hub: serve | cli | mcp | sleeve-runner | sandbox-helper | doctor
                                       embeds: Flutter web build, headscale, member
protectorate-node-{linux-amd64,linux-arm64,windows-amd64,windows-arm64,android-arm64}
                                       node: serve | join | uninstall | doctor; embeds member
protectorate-desktop-{linux,windows}-{amd64,arm64}, protectorate-android.apk
install.sh, install-node.sh, install-node.ps1, VERSION, SHA256SUMS
```

- **Language: Rust and Go.** Rust for everything except `go/`, and Dart for the Flutter app. Zig is
  used only as the C toolchain behind cargo-zigbuild for glibc 2.31 linking. Neither the hub nor the
  node needs cgo. Go is only the official headscale and our `member` glue on tailscale's tsnet.
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
- **Hub:**
  - `protectorate update [--version]`, or the app's UPDATE key, downloads the release and stages it
    in `releases/<version>/`.
  - Headscale's database is snapshotted first, because its migrations are one-way.
  - The hub re-execs in place (D17), so sleeves keep running. The previous release stays on disk.
  - If the new hub fails its health check within 2 minutes, a supervisor wrapper switches the
    `current` pointer back and restarts it.
- **Nodes, triggered by the hub:**
  - The hub sends `Update(version)` over its own connection. The node downloads the release from
    GitHub itself: outbound to GitHub, not to the hub.
  - The node verifies the release, stages it, keeps the previous binary, and restarts through its
    service manager.
  - **Self-rollback:** if the hub has not contacted the node within 10 minutes of the restart, the
    node reverts to the previous binary. Hub contact is the only proof that matters, because the
    node can't call the hub.
  - A node without internet gets the verified bytes streamed by the hub over a hub-opened
    connection, so the one-way rule still holds.
- **Order:** hub first, then one canary node per OS and arch, then the rest. Headscale accepts
  clients up to 10 minor versions old, and `member` shares headscale's `tailscale.com` version
  through one go.mod, so a fleet mid-rollout keeps working.
- **Desktop and Android apps** are not updated by the hub. The app shows "hub is vX, app is vY"
  and links to the release.

### 6b. Moving the hub to a new machine

Not needed now; it is designed so it stays possible. **Moving the hub is a restore.** The same
backup that recovers a dead hub moves a live one.

1. **One root.** Everything the hub owns lives under `$PROTECTORATE_HOME`:
   - the journal and state;
   - `tailnet/`: headscale's DB, noise key and DERP key, and the hub member's keys;
   - token hashes, history and doctor caches;
   - sleeve homes, which hold the harness logins and native session logs.

   Workspaces live in the owner's own directories. The export records their paths and git remotes.
2. **Export:** `protectorate hub export`.
   - It quiesces every sleeve at a turn boundary (the resleeve machine's Quiescing, without the
     relaunch) and stops headscale.
   - It writes a `.pbackup` (Verus-checked format), and pushes or bundles every workspace's
     unpushed branches.
   - The old hub then writes a `migrated` tombstone and refuses to start headscale again. Two
     control servers with the same noise key would split the fleet.
3. **Import:** install the hub with the one-liner on the new machine, then run
   `protectorate hub import <file>`. The architecture may differ, because every on-disk format is
   arch-independent.
4. **Cut over:** point Traefik's two routes and the hub's firewall rule at the new address. Node
   trust needs no change: the nodes reach the same hostname, headscale presents the same noise
   key, and the hub member keeps its node key and `tag:hub`. No re-enrollment.
5. **Resume:** sleeves come back through Recovering. Native resume works where the session logs
   moved with the sleeve homes; otherwise they get a continuation brief. `doctor` re-validates
   every harness login and mails you any that need a re-login.

## 7. Git, CI and doctor

- **GitHub stays the origin.** `gh` is installed and authed on the hub, and the hub alone holds
  the push token. Sleeves push through `protectorate git push` (12b item 6) and get a read-only
  token for `gh` queries. The hub keeps bare mirrors as backup. No Forgejo, GitLab or Woodpecker.
- **Cross-arch CI** is a hub rule: dispatch `make test` as a node job to each required arch. Plain
  binaries; containers only when a job asks for them.
- **`doctor`**, on hub and node, checks:
  - harness CLIs present, with versions and install method (D33);
  - credentials valid, through the read-only quota readers, never printed;
  - `gh` auth;
  - sandbox prerequisites, disk and clock.

  Output is fixed JSON, cached in `doctor.json`, and it gates harness eligibility for dispatch and
  resleeve. Failures mail the owner.

## 8. Quota and dispatch

Quota readers stay read-only per subscription. `select_target(task_size, allow, mode, exclude,
quota, health)` is pure code, using fresh data only. It weighs projected unused quota at reset with a
48 h urgency half-life and falls back to the local model. "Dispatch by quota" is this function,
called at spawn and at guardrail time. Code derives the task-size class, Envoy may nudge it one
class, and code clamps it (D27).

**Spare quota (PROBLEMS P6).** A code rule triggers: projected unused capacity at reset is above X%
and no high-priority work is queued. When it fires, the next backlog item goes to that harness.
Envoy picks which item; code decides when and where. Work on public repos lands in a review inbox
(a branch on your fork plus a summary), and nothing public goes out without you (D29).

## 9. UI

Amber on dark, cleaner, closer to an IBM 3270/System/370 console than a mobile app:

- single-size monospace and 1 px rules;
- no rounded cards, bezels, glow or seven-segment digits;
- character meters;
- a function-key legend and a `===>` command line.

Chat comes first (D38). The main screen is Envoy's chat, with a workspaces/sleeves tree beside it.
Choosing a sleeve opens its chat log, built from the headless transcript, in the same simple pane
layout herdr uses. Approvals appear inline in the chat and on the phone. Mail and events, nodes, and
quota each have their own screen. A taken-over terminal (xterm.dart) is reached from a sleeve's
chat, never as the default view. The ten rules and an ASCII layout are in the
nodes report section 7.

## 10. Verus

The owner's rules apply as written (see the session doc), with one proposed deviation that needs
the owner's approval (D19): FFI `unsafe` in the sandbox and presence crates.

Verus is phased (D28), so its cost is measured before it spreads:

| Phase | Modules | Why |
|---|---|---|
| 1 | journal codec and torn-tail recovery; node frame codec | the hub's durability rests on the first; the second is the only parser fed by hostile machines |
| 2 | PTY ring and attach offsets; harness line framer; session-log tail reader with its UTF-8 clipping | offset arithmetic with no reference implementation |
| 3, if phase 1 and 2 proofs stay cheap to maintain | `SHA256SUMS`/manifest parser; artifact `xfer` bounds; Connect streaming envelope; backup format | untrusted or long-lived bytes, but small and well fuzzed already |
| dropped | presence window math; audit-chain walk; brief budget arithmetic | a `saturating_sub` or a loop over a list; fuzzing and overflow checks are enough |

**Excluded:** the state machines (the tables are a perfect reference), per-harness JSON extractors
(differential fuzzing against real logs instead), UI and glue.

**Build shape:**
- Specs go in `cfg_attr(verus_keep_ghost, verus_spec(..))`, with `vstd` an optional feature, so plain
  cargo builds without Verus.
- `verify.sh` runs a pinned Verus release at `--rlimit 20`, then again at `--rlimit 10`.
- `overflow-checks = true` in release for the parser and writer crates.
- `#![forbid(unsafe_code)]` everywhere except Verus-proved helpers and the explicitly exempted OS
  FFI crates (sandbox, presence).

## 11. Salvage map

| From | Take |
|---|---|
| `dev-hotschmoe` Rust | proto and wire generation, `pd1.`/`pc1.` auth, quota readers and `suggest.rs`, the four drivers (with the dispatch-before-record and Codex timeout fixes), the sandbox helper trimmed to the light profile, screen-rule tables, chatlog/handoff fact gathering, fuzz targets, release scripts |
| `4f199cc^` (before the fleet removal) | Rust node inventory and sample collectors, Windows runner account |
| `bc9b87a^` (Go tree) | lessons only: tailnet supervisor, DERP cert fix, disco ping on start, headscale backup gate |
| `references/herdr` | screen manifests (Apache-2.0), debounce, badges, the wait semantics, fd handoff on update |
| `references/agent` | uncertain-effect states, record before dispatch, steer vs queue, the pi_durable bridge shape |
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
   Lost, and a later status reconciles it. No state may wait forever on a node.
7. **Every non-terminal state has a recovery action.** Probing re-probes. Collecting re-fetches
   from its recorded offset. Cancelling re-sends the cancel. Provisioning waits for its completion
   barrier and then records or removes the worktree. Stop during Provisioning waits for that
   barrier too. The fuzz targets insert a hub crash at every journal boundary.
8. **The control-key pin is enforced on every start.** Before starting tsnet, `member` fetches
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
    headscale and member. A node is two: agent and member. The pi_durable experiment adds Node and
    a bridge. Any further runtime process needs an explicit decision.

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
5. **Headscale is reachable only from Traefik.** Headscale binds the hub's LAN address, and one
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
    spawn. In children, `.git/hooks` of the shared gitdir is mounted read-only, and G11 walks the
    resolved gitdir, not only the worktree diff.
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
| D1 | Headscale inside the hub binary but outside the process (sandboxed child) | yes |
| D2 | Build headscale from the pinned upstream source, or embed the release binary | official headscale, built locally from the pinned tag |
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
| D13 | Antigravity pane-only until headless approvals work | yes |
| D14 | Node.js on the hub for the pi_durable experiment | yes |
| D15 | Resleeve target: ordered list per sleeve, default `[pi_local]` | yes |
| D16 | Guardrail thresholds as listed; no auto-revert on forbidden-path edits | yes |
| D17 | Hub updates by re-exec in place, keeping sleeves alive | yes |
| D18 | The hub never types into a pane after launch | yes |
| D19 | FFI `unsafe` exemption to Verus rule 6 for sandbox and presence crates | yes |
| D20 | Push token location | hub only; sleeves push through `protectorate git push` |
| D21 | Index the owner's own interactive CLI logs too, or hub-launched sessions only | hub-launched only |
| D22 | Vectors off until keyword search falls short | yes |
| D23 | Brief file `.protectorate/BRIEF.md`, LLM summary for planned resleeves only | yes |
| D24 | `===>` command line and F-key legend in the app | yes |
| D25 | macOS node and desktop only once a Mac exists | yes |
| D26 | `history.read` scope | Envoy, plus a continued sleeve for its own run; wider is owner-only |
| D27 | Source of the task-size class for dispatch | code (median of the last N), Envoy may nudge one class, code clamps |
| D28 | Verus phasing: phase 1 is journal and frame codec; presence, audit and brief math dropped | yes |
| D29 | Backlog with a spare-quota trigger in code and a review inbox for public repos (PROBLEMS P6) | yes, after resleeve works |
| D30 | GPU/resource leases with a TTL (PROBLEMS P9) | later |
| D31 | A network spike before the core | **no separate spike**: build straight away and fix roadblocks as they come |
| D32 | One kind of node; presence gates jobs | yes |
| D33 | Harnesses install and update natively, never pinned | yes |
| D34 | Pi as a standalone binary, updated natively | yes; check every 24 h, `pi update self` preferred |
| D35 | Gemini CLI | **dropped**; Antigravity CLI replaces it, as a standalone binary |
| D36 | Release signing key | **none for now**; releases built locally and checked against SHA256SUMS; revisit |
| D37 | Third-party dependencies | as few as possible, pinned exactly, forked and trimmed when large; listed in `DEPENDENCIES.md` |
| D38 | Primary interface | chat with Envoy; sleeve chats next; terminal takeover is last resort |
| D39 | Hub-to-node transport: headscale and member, or plain SSH | **headscale for now**; SSH logged in REVISIT.md (`serve --stdio` stays as a tested path) |

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
6. **Hardening:** the first Verus modules (journal codec, node frame codec), the stdio
   `mcp --envoy`, terminal takeover, the remaining harnesses, and the pi_durable experiment.

## 15. Implementation choices (2026-11-01)

How the code is built, chosen to keep third-party code near zero (D37). Each choice that is a
shortcut is also in `REVISIT.md`.

- **No async runtime.** The hub serves a few clients and a few dozen sleeves, so it uses std
  threads and channels:
  - one core thread owns all state and runs `apply`;
  - each connection, child pipe and node link gets its own thread;
  - timeouts use deadlines on sockets and `poll`.

  This avoids tokio, hyper and axum. The owner allows them if they turn out to be needed
  (2026-11-01), "incredibly deliberate and cautious", but prefers a performant build without them.
  The fallback is a pinned, trimmed fork, chosen only after our own code has measurably failed.
- **Our own crates** for what we would otherwise import:

  | Crate | What it does |
  |---|---|
  | `pt-json` | JSON value, parser, writer, `ToJson`/`FromJson`; insertion-ordered objects so output is deterministic |
  | `pt-hash` | SHA-1 (WebSocket handshake), SHA-256 (release sums), CRC-32C (journal) |
  | `pt-http` | HTTP/1.1 server on TCP and unix sockets, WebSocket server, a small blocking client for loopback and unix sockets |
  | `pt-journal` | the append-only journal: length, CRC and payload frames, group-commit fsync, torn-tail recovery (Verus phase 1) |
  | `pt-core` | the pure state machines and `apply`, with no I/O |
  | `pt-api` | API types and the method table, declared once in a macro; `pt-apigen` writes the Dart models from it |
  | `pt-drivers` | headless harness adapters (Claude stream-json, Codex app-server, Grok ACP, Pi rpc) into one event stream |
  | `pt-history` | the history index (3.7): native session logs and hub transcripts into checksummed segments with BM25, in place of SQLite FTS5 |
  | `pt-sandbox` | the bwrap argument builder and systemd scopes |
  | `pt-frame` | the hub-node frame codec (Verus phase 1) |
  | `pt-node` | inventory, samples, presence and jobs (the node side) |

  Each is fuzzed against a reference where one exists: `serde_json`, `sha2` and `crc32c` are
  dev-only fuzz dependencies. There is no SQLite: `pt-history` replaces `rusqlite` (3.7).
- **The API** is JSON over HTTP:
  - `POST /api/<Method>` carries the request and response bodies;
  - `GET /api/events` is a WebSocket of state changes;
  - loopback and unix-socket listeners serve the CLI and sleeves; the trusted-proxy listener serves
    Traefik.

  There is no protobuf, buf or Connect. The Dart client is generated from `pt-api` and uses
  `dart:io` and `package:web` sockets.
- **Outbound HTTPS** (GitHub releases, quota readers) runs the system `curl`. There is no Rust TLS
  stack.
- **Sleeve sandbox** is bwrap plus a `systemd-run --user --scope` per incarnation.
- **Go** (`go/`): `cmd/headscale` builds the official headscale at the pinned tag, unmodified;
  `cmd/member` is our tsnet glue. `member` exposes a unix socket where the Rust side asks it to
  dial `node:7475` (hub), or where accepted tailnet connections are handed over (node).
- **Envoy** is a root sleeve running Claude Code headless in `$PROTECTORATE_HOME/envoy/`. That
  directory's own `CLAUDE.md` describes the `protectorate` CLI, so no worked-on repo is touched.
