# Memory, handoff and messaging: options (P7)

Status: decision document, not code. Options 2026-10-19, decisions 2026-10-20 (section 4).
Ethos: harnesses run untouched (their own project-context files and interfaces
only). The orchestration layer may be as robust as needed.

## 1. The problems, separated

| | Problem | Solved today? |
|---|---|---|
| a | Continuity inside one harness across restarts | Yes. Native resume keeps the same id in all four (`spikes/drivers/`): `claude --resume`, `thread/resume`, `session/load`, `pi --session`. `{harness, cwd, session_id}` suffices. |
| b | Switch harness mid-task (quota low) | No. Session ids and stores are per vendor (`~/.claude/projects/*.jsonl`, `~/.codex/sessions/..`, `~/.grok/sessions/..`, `~/.pi/agent/sessions/..`). The new harness starts blind. |
| c | Long-term per-workspace knowledge (decisions, learnings) | Partly. Each harness reads its context file (CLAUDE.md / AGENTS.md); nothing writes to it on purpose. |
| d | Messaging: orchestrator <-> sleeves, sleeve <-> sleeve | No. Archive needlecast never got a backend (V1_GAP: 5%). Hub has EventService (hub -> clients) only. |

Key point: (a) is done, (b) is the real design question, (c) and (d) are
different problems that the archive fused into one `.cstack/` directory.

Constraint specific to (b): a switch happens because quota is low or gone, so
the outgoing harness often cannot take another turn. A design that needs the
outgoing model to write its own handoff fails exactly when it is needed.

## 2. Options

### 2.1 cortical-stack `.cstack/` (CURRENT/PLAN/MEMORY/INBOX/OUTBOX)
- Spec: markdown files the agent maintains; Claude hooks load them at start
  and nag at stop; V2/V3 add a beads-fork CLI, graphs, compaction, decay.
- Archive use: sidecar parsed the files for `/status`; envoy routed
  INBOX/OUTBOX. Needlecast routing and cstack parsing were never finished.
- What broke / why it is weak: state is only as fresh as the agent's
  discipline (stop hooks are Claude-only; other harnesses need prompting, which
  violates "untouched"); the agent that must checkpoint is the one out of
  quota; PLAN.md duplicates the hub backlog; V3 is a memory product (beads
  port, SQLite index) and both outside reviewers recommended cutting it.
  Message-in-file (OUTBOX read and cleared) has no delivery or wake semantics.
- Verdict: keep the idea "plain files, git-visible", drop the format, the
  CLI and the sidecar.

### 2.2 HANDOFF.md written at switch time (both reviewers)
- One short file per switch: goal, state, files touched, test status, next
  action, source `{harness, session_id}`, pointer to the native transcript.
- Pros: written when needed, so never stale; no standing discipline; any
  harness reads a file; tiny.
- Cons: summary quality depends on the writer; a lossy view of the work.
- Verdict: the core of the answer to (b).

### 2.3 Harness-native memory
- CLAUDE.md / AGENTS.md (already generated per spawn by the hub, see
  `docs/orchestrator.md`), Claude auto-memory, Codex AGENTS.md hierarchy, pi
  context files and skills.
- Good for (c): durable, read by every harness at start with no prompting.
  Inconsistent for writes (Claude memory is per-vendor and private; none is
  shared across harnesses).
- Verdict: use as the delivery channel (read side), not as the store.
  One shared `AGENTS.md` with `CLAUDE.md` as symlink already works.

### 2.4 Transcript export / convert between harnesses
- Feasible mechanically (all are JSONL), but the formats differ: tool-call
  schemas, thinking blocks (signed/encrypted, not portable), reasoning items,
  compaction entries. Injecting a foreign-written file into another vendor's
  store is undocumented and breaks on upgrades (versions here: 2.1.303,
  0.157.1, 1.0.52, 1.3.0). pi itself lists "cross-provider session handoff" as
  a hard part of custom providers.
- Lossy at best, fragile at worst, and it touches harness internals.
- Verdict: do not convert. Do read the outgoing transcript (read-only) as an
  input to a summary (2.2). Keep native stores as the full-fidelity record.

### 2.5 pi-durable as substrate
- Provides: a library for building a harness. Conversations are immutable
  entries, atomic commits, tasks with checkpoints, `reset("handoff note")`,
  compaction, forks, child tasks, SQLite/JSONL storage, request-id idempotence.
- It stores pi conversations. It cannot hold a Claude/Codex/Grok session and
  nothing in it ingests foreign transcripts. Experimental, API unstable.
- Helps non-pi harnesses: no. Worth stealing: `reset(handoffNote)` as the
  model for "new context seeded by a note", idempotent request ids for hub
  messages, tasks-with-checkpoints if the hub orchestrator becomes long-running.
- Verdict: not a memory layer. Revisit only if pi becomes the single
  harness (P5).

### 2.6 Messaging: MCP server vs hub-native
- mcp_agent_mail (superseded Python version; Rust successor): identities,
  inbox/outbox, threads, advisory file leases, Git + SQLite. Reaches any
  MCP-capable harness with no wrapper. Cost: another service, and agents must
  be prompted to poll; idle harnesses are not woken.
- tincan: same shape, hosted; its HARNESS_DELIVERY table shows the real
  hard part is waking an idle harness, solved per harness with hooks, app-server
  or SDK controllers.
- Hub-native: we already own the wake path. Dispatched sessions take `Send`
  through the official driver; attached TUIs accept typed input via tmux;
  EventService gives pub/sub; `lease_*` tools and scoped tokens exist; the
  CLI is already inside every sleeve and documented in AGENTS.md.
  Add `MessageService` (Send/Inbox/Ack, persisted as files or SQLite on the
  hub) and `protectorate msg send|inbox` plus an MCP tool of the same name via
  the existing MCP bridge.
- Verdict: hub-native, because identity, scopes, audit and wake delivery are
  already ours. Borrow agent_mail's inbox/thread/lease vocabulary; skip its
  server.

### 2.7 Git as substrate
- Commits + `git notes` + a `handoff/*` branch or file under version control.
- Pros: durable, diffable, travels with the repo across nodes; WIP commit
  at switch is a good fact source. Cons: notes are poorly supported by agents
  and by push/pull defaults; pollutes history; not every workspace is a clean
  repo; handoff text is transient.
- Verdict: use git for facts (status, diff, last commits) in the handoff;
  keep the handoff itself out of the repo (`.git/info/exclude`), archived
  under the hub's state dir.

## 3. Recommendation

Split by problem:
- (a) native resume, nothing new.
- (b) hub-composed HANDOFF.md + native store pointers.
- (c) outside AGENTS.md (see decision 4 in section 4); store still open.
- (d) open (decision 5); hub-native delivery is the strongest candidate.

### Resleeve flow

```
 quota.updated (Claude low)           hub (overseer or rule)
        |                                   |
        v                                   v
  decide switch --------------------> 1. freeze: interrupt, mark session
                                         owned-by-hub (one owner per id)
                                      2. compose facts (no model needed):
                                         git status/diff --stat, last commits,
                                         backlog item, last N user msgs and
                                         final assistant msg read from the
                                         outgoing native jsonl, session ids
                                      3. summary: best subscription with quota
                                         (fallback local pi, user notified)
                                         reads facts + tail and fills Goal /
                                         State / Unfinished / Next / Gotchas
                                      4. overwrite <ws>/.protectorate/HANDOFF.md
                                         (+ copy in ~/.protectorate/handoffs/)
                                      5. spawn new sleeve, same workspace,
                                         harness chosen by quota suggest
                                      6. Send first prompt via driver:
                                         "Continue the task. Read
                                          .protectorate/HANDOFF.md first."
```

- Who writes: the hub always writes the factual part (cannot fail when quota
  is gone). A separate summarizer writes the narrative. The outgoing harness
  is never asked.
- Where: `<workspace>/.protectorate/HANDOFF.md`, excluded from git; archived
  copies on the hub. Same path for every harness and node.
- How the new harness is told: the hub's first prompt is a plain user
  message naming the file. Normal input, so harnesses stay untouched and
  AGENTS.md carries nothing about it.
- Returning later to the original harness: native resume of its old session
  plus the newer HANDOFF.md as a delta note, not a replay.
- Rule: handoff is data to read, not authority. It never carries credentials
  or tokens, and the new sleeve's scopes come from its own token.

### First minimal step
1. `protectorate sleeve resleeve <name> [--to auto|harness]` doing steps 1, 2,
   4, 5, 6 with facts only (no summary model). Roughly one command, no new
   service.
2. Summary pass (step 3), then coordination (decision 5), then (c).

### What to measure
- Resume success: does the new harness's first turn reference the right file,
  branch and next action (manual rubric, 10 trials per harness pair).
- Re-discovery cost: tokens and tool calls until first correct edit, with
  handoff vs cold start.
- Handoff size and write time; fraction needing the summary pass.
- Failure modes: switch with stale/UNKNOWN quota, double owner of a session
  id, handoff ignored (harness did not read it).
- Message delivery latency to an idle sleeve, per harness.

## 4. Decisions (2026-10-20)

Principle: anything that can be deterministic is code, not a model call, so
it costs no tokens and repeats exactly. One code path, not fallbacks between
model behaviours. Never bust a harness's native prompt cache without reason.

1. **Outgoing harness is never asked.** Facts are always composed by the hub.
   A separate summarizer reads the tail of the outgoing native transcript
   (read-only) and writes summary plus unfinished tasks.
2. **Summarizer:** the subscription with the most spare quota, falling back
   to local pi. A local fallback is surfaced to the user (notification, and
   the sleeve card shows which model is active, colour coded).
3. **Nothing of ours lands in a worked-on repo.** `.protectorate/` is excluded
   by code (`.git/info/exclude`), handoffs are archived on the hub, and
   `HANDOFF.md` is overwritten on every resleeve so a stale one never
   confuses a later sleeve. Outside PRs work the same as internal ones.
4. **AGENTS.md is one tiny, project-agnostic rule set the user owns**, the same
   in every project, with `CLAUDE.md` a symlink to it. No project specifics, no
   hub-managed sections, no hooks or skills added to harnesses. Decisions and
   learnings (c) live elsewhere. Consequence: the orchestrator's generated
   role document (`internal/orchestrator/doc.go`) must move out of AGENTS.md.
5. **Coordination: open.** Candidates: an excluded mailbox file, GitHub issues
   with the orchestrator pinging the workspace, a small in-binary mailbox in
   the shape of mcp_agent_mail, or a separate coordinator (deterministic
   server, light model or a decision model on the local LLM server) so the
   orchestrator stays clear of routing.
   Proposal (2026-10-20, not yet agreed): the coordinator is hub code, not a
   sleeve. A directory of sleeves and projects (git remote and package names
   read from go.mod, package.json, Cargo.toml, pyproject); a hub mailbox
   (`protectorate msg send|reply|ack|inbox`) delivered into the target's chat
   at its next idle as an ordinary user message that says how to reply
   (appending does not bust the cache, and AGENTS.md needs nothing); unacked
   messages retry once, then escalate to the orchestrator. Hardware stays on
   leases with a queue, never negotiated between agents. Cross-project
   dependencies come from the manifests: an upstream release notifies
   dependents with facts (versions, commits, changed public files) or files a
   backlog item, and a downstream request becomes an upstream backlog item
   linked to a thread that closes on release. A decision model (local vLLM
   D3 or Jev) is optional and only for fuzzy calls: interrupt or wait, route
   an unaddressed request, merge duplicates.
6. **The orchestrator resleeves without confirmation** at this stage.
   Deterministic watchers ping it on quota levels, problems and idle. It needs
   tools that say whether an action busts the cache and whether to trigger the
   harness's native compaction first (at a turn boundary), and the summary may
   run in parallel while the outgoing sleeve winds down.

Runtime: sleeves stay bwrap in a systemd scope (no Docker); per-sleeve network
namespaces are being evaluated in `spikes/netns/`.
