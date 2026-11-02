# Adversarial review: Grok, 2026-11-01

Read-only run of `grok -p --permission-mode plan` over SPEC.md, docs/design/ and the session doc. Accepted fixes are in SPEC.md section 12b.

**15 findings, most severe first.** Read-only review of `SPEC.md`, the four `docs/design/` reports, and the owner’s `docs/sessions/2026-11-01-redesign.md`.

---

### 1. HIGH — Unsafe prompts can be retried after Uncertain (I8 is bypassable)

**Section:** `sleeves-state-machines.md` §2.c (`NotApplied → Intent`), I8, kind table (`SendPrompt`/`Steer` = Unsafe). `SPEC.md` §2.5, §4.

**Failure:** I8 forbids `Uncertain → Intent` for Unsafe effects. The table still allows `Uncertain → Probing → NotApplied → Intent(attempt+1)`. Probe for a vendor send is “harness ack, else native state” (Codex marked `[I]`). A lost ack, late persist, or the 15 s ACP ack timeout can yield `ProbeNotApplied` after stdin already got the prompt. Retry runs tools twice. That is the “auto-resending uncertain prompts” alternative §5 rejects.

**Fix:** Close I8 over the whole graph. Unsafe leaves Uncertain only to Confirmed, Fenced, or Abandoned. `ProbeNotApplied` on Unsafe is Unknowable. ACP ack is first `session/update`, not a 15 s wall.

---

### 2. HIGH — Two machines both start incarnations; dropping PDEATHSIG leaves gen alive

**Section:** sleeves §2.a (`Recovering → Decide(Resume) → Launching(g+1)`), §2.d (resume-class “sleeve goes to Launching(g+1)”), I5, §1.6 P8. `SPEC.md` §3.3 vs §4 G3.

**Failure:** `IncarnationExited` / `HubRecovered` is consumed by both `sleeve::apply` and `guardrail::apply`. There is no composed event algebra. The fuzz plan (“one match arm per table row”) encodes a double `StartIncarnation(g+1)`. I5 is apply-time only.

P8 drops PDEATHSIG, so a SIGKILLed hub can restart while gen g still has tasks. §2.a has no row for `HubRecovered` with probe(g) present. Recovery either sticks in Recovering or starts g+1 beside a live g.

SPEC §3.3 also says every crash native-resumes, dropping G3’s flap limit and forced resleeve.

**Fix:** One composed crash transition: `KillScope`, probe empty, then `StartIncarnation(g+1)`. Keep PDEATHSIG except across the planned `execve`. Executors drop intents whose `g` is stale. Put G3 in SPEC §3.3.

---

### 3. HIGH — Activity goes Idle while DeliverNext is in flight

**Section:** sleeves §2.b (`TurnSettled → Idle` + `DeliverNext`; `Idle + Send(queue)` emits another `DeliverNext`), I9, §1.10.

**Failure:** Apply is the single writer; executors are async. After `TurnSettled` the sleeve is Idle with a `SendPrompt` not yet on the wire.

- If m1 was dequeued: `Send(m2)` sees an empty queue, goes Working, emits a second prompt. Two Unsafe sends in flight.
- If m1 was not dequeued: `Send(m2)` emits a second `DeliverNext` for m1. `DeliverNext` is not in the kind table and has no id rule, so I9 does not bind it.

Same on `Ready` and `CompactDone`. `AckTimeout` does not move activity, so effect can be Uncertain while activity is Working or Idle-and-delivering.

**Fix:** Stay `Working`/`Delivering{id}` until the send is terminal. Dequeue in apply, keyed by message `request_id`. One non-terminal `DeliverNext` at a time.

---

### 4. HIGH — “Nodes never send to the hub” does not hold

**Section:** Owner Message 1 (“NODES cannot send anything to HUB”). `SPEC.md` §2.4 vs §3.2. `network-trust-language.md` §3.2 disco ping, §4.4 items 1–4, §6, `:7480` bind, risks 9 and 11.

**Failure:**

1. The node disco ping is a **node-initiated** message to the hub member, kept on purpose (`a26e576`). CallMeMaybe/handshakes already reach the hub member. SPEC §2.4 is false as written.
2. Control and DERP run on the hub host. Traefik hides `/register`, `/auth`, `/swagger`, `/verify`; those routes still exist on `192.168.10.44:7480` (`app.go:544-610`, verified). A node that dials the LAN IP skips the path filter. Headscale RCE can mint `tag:hub` and dial every `:7475`, care machines included. The sandbox keeps hub secrets off the child; it does not keep the fleet.
3. The WireGuard filter accepts every non-SYN TCP segment with no conntrack (`filter.go:532`, verified). Node bytes reach the hub’s gVisor stack without a hub-originated SYN.
4. Hub-served `node.sh` (finding 14) is an unauthenticated node→hub GET before enroll.

The real invariant is “nodes cannot open a flow to the hub API.” That is not what the owner asked for, and not what SPEC §2.4 says.

**Fix:** Rewrite §2.4 to the actual rule. Bind headscale to stronghold-only or loopback+Traefik. Drop the disco ping once e2e shows it is unnecessary, or name it as an exception. Do not ship hub-served `node.sh`.

---

### 5. HIGH — Cutting egress leaves PATs and harness creds on the host network

**Section:** `SPEC.md` §3.4, D12, D20, §12. sleeves §1.4. nodes §2 (PAT as `GH_TOKEN`; “egress proxy admits github.com”).

**Failure:** The PAT story in the nodes report assumes an allowlist proxy. SPEC deletes the proxy and the netns. Every sleeve then has a push token, a live harness login, and the host network (LAN: NAS, Traefik, `:7480`; WAN: GitHub). Hub sleeves with secrets are less isolated than node jobs (nodes §1.3 still uses netns). Node job output is an instruction channel into that process (network §4.4.8).

**Fix:** Keep a narrow egress allowlist, or drop D20 and let only Envoy push. Do not put `GH_TOKEN` in a process that can reach the WAN.

---

### 6. HIGH — Resleeve can go Running without the continuation prompt

**Section:** sleeves §2.d (`Briefing → Uncertain/Fenced → Cooldown`, “no resend”). `SPEC.md` §3.7. history §4.4.

**Failure:** The prompt that tells the new harness to read `BRIEF.md` is Unsafe `SendPrompt`. On timeout the guardrail still Cooldown and marks Running. Forced resleeve to `pi_local` is the case that needs that prompt. `DeliverQueue` may then fire some other message as the first turn.

**Fix:** Key the continuation send (`resleeve:<name>:<g+1>`) and block Running until Confirmed, or until the native log contains that prompt. Unknowable → Park and mail.

---

### 7. HIGH — Brief template drops Uncertain/Fenced effects (SPEC vs history)

**Section:** `SPEC.md` §3.7. sleeves §2.d and §4 (`effects`, `roster`). history §4.5 template (no effects, no children).

**Failure:** SPEC wins on conflict, but the only concrete template is history §4.5. Implementing it omits the one thing a successor must not redo. Child sleeves left running across a root crash (R4/D7) also vanish from the brief.

**Fix:** Add required `## Effects that may have been applied` and `## Child roster` to §4.5. List this in SPEC §12.

---

### 8. HIGH — Node job machine ignores hub crash; Cancelling never times out

**Section:** sleeves §2.e, §2.c `NodeSubmit`. nodes §1.3. `SPEC.md` §4.

**Failure:** Hub dies after Submit Intent fsync, before Dispatched. Effect goes Uncertain; job stays `Submitting` until ack/nack/timeout. If the timer is not re-armed from the journal, the job is stuck: lost if the node never saw it, uncollected if it ran. `Cancelling` has no tick, so a hostile/hung node pins a scheduler slot. Node “journals before exec” does not say fsync; crash between write and exec can look like “unknown job” and the Keyed retry starts a second process.

**Fix:** `HubRecovered` maps `Submitting → SubmitUncertain` and reissues `Status`. Cancelling deadline then Lost/Abandoned. Fsync the node journal before exec. Probe must distinguish never-seen from journaled-and-dead.

---

### 9. HIGH — Owner’s general-agent client is CLI; spec ships unverified remote MCP only

**Section:** Owner Message 1 (GrokBot/Claude Duet/Dot “will use cli (or mcp?)”). `SPEC.md` §3.1, D9. nodes §3 (“I could not verify”).

**Failure:** The differentiator is vendor general agents dispatching into local sleeves. Spec answers with streamable-HTTP MCP and no user-machine CLI. Documented MCP for Claude Code/Codex/Desktop is **stdio**. GrokBot/Duet/Dot were not checked. If they cannot speak remote MCP, the owner’s input path does not exist, and D9 removed the fallback.

**Fix:** Verify each named client before locking D9. Ship a thin user-machine CLI over HTTPS with an agent token. Do not claim this in SPEC §1 until one real client connects.

---

### 10. MEDIUM — Executors can run intents after gen has moved

**Section:** `SPEC.md` §2.3–2.5. sleeves §1.10 (“executors”, group commit). I5, I7.

**Failure:** Intents come from a pure function and go to a pool. Nothing says `StartIncarnation(g+1)` waits for empty scope g, or that fsync happens before the first byte. Reordering double-starts a harness. Fsync-after-dispatch violates record-before-effect and feeds finding 1.

**Fix:** Per-sleeve serial execution. Recheck `g`/`attempt` immediately before the syscall. Barrier: fsync, then release intents.

Unsure whether they meant a pool; the text says “executors” and “group commit” with no barrier.

---

### 11. MEDIUM — Parent Park/G10 does not stop children; G11 misses shared `.git`

**Section:** sleeves R3–R4, G10, G11, risk 3. `SPEC.md` §3.4, D7–D8.

**Failure:** Root G10 Parks the root and leaves children Running. Child budget is capped at spawn, not live; children keep spending. G11 is a git diff including `.git/hooks/**`, but worktree children share the real gitdir; hook writes do not appear in `git diff`. A child can install a hook that later runs in the parent.

**Fix:** Live remaining-budget cap; G10/G12 cascade Stop (or freeze sends). G11 walks the resolved gitdir. Mask `.git/hooks` in the sandbox.

---

### 12. MEDIUM — Envoy `task_size` is an LLM input to harness ranking

**Section:** Owner: deterministic things in code. `SPEC.md` §2.1, §5, §8. sleeves §1.8 (`select_target(task_size, …)`; “Envoy never ranks”).

**Failure:** Targets below 5/15/30% unused quota are skipped. Inflating `task_size` forces expensive subscriptions; deflating it steers onto `pi_local`. That is ranking by LLM.

**Fix:** Code owns size (last-N cost, sleeve budget, or an owner-set class). Fuzz `select_target` with no model-produced field.

---

### 13. MEDIUM — `history.read` never decided; logs carry secrets

**Section:** history §4.1, §4.4, §6, owner decision 6. `SPEC.md` D21 answers a different question (whose logs to index).

**Failure:** Native logs contain tokens. MCP `history search|read` with scope `history.read` is unspecified. Default “CLI in every sleeve” lets a child read other sessions. SPEC D1–D25 never picks who gets the scope.

**Fix:** D26: `history.read` for Envoy and the continued sleeve’s own `run_id` only. Wider search is owner-class.

---

### 14. MEDIUM — Device-code-on-phone dropped; nodes report still has node→hub install

**Section:** Owner: “device code on phone was kinda nice.” nodes §5 (`curl https://protectorate.hotschmoe.com/node.sh | sh -s -- WXYZ-1234`). network §5 scan-to-join, rejects typed codes because they need an unauthenticated node→hub endpoint; decision 11 still open. `SPEC.md` §3.2, §12 does not resolve installer source.

**Failure:** Two enroll/install designs remain. Implementing nodes §5 creates the node→hub GET the network report refused, and contradicts SPEC “nothing on the node calls a hub endpoint.” Scan-to-join also inverts the UX the owner liked, recorded as the investigators’ D5, not an owner choice.

**Fix:** SPEC §12: GitHub-tagged installer + signatures; scan-to-join; system service. Stamp nodes §5 obsolete. If phone-shows-code is required, redeem it on an already hub-opened connection after tagging.

---

### 15. MEDIUM — Leftovers that will be implemented wrong

- **Pane `Unknown` vs G14.** §2.b mermaid: Unknown on “desync or no screen rule.” Table: only `ProtocolDesync` trips G14 (KillScope). A TUI update that breaks a manifest can kill the pane if someone codes the mermaid. Split `NoMatch` from `Desync`.
- **Gemini is a first-class harness** (`SPEC.md` §3.4) **with no history reader** (history §4.3 “gemini later”). Auto-resleeve of Gemini ships a brief with no log facts. Extractor first, or exclude from `resleeve_to`.
- **`doctor` still checks `sandbox.tmux` and `egress.<host>`** (nodes §4) after both were cut. Rewrite the table.
- **Mail/approvals have no machine.** SPEC §3.3 lists mail delivery/re-delivery; §4 has five machines. Duplicate/dropped mail across crash is untested. Add a 2.c kind or a sixth machine.
- **pi_durable adds Node ≥ 22.19** (sleeves §1.7, D14) against “fewest binaries” and “rust+pi_durable.” Keep the SIGKILL gate; Bun pi stays default.

---

### Checked and not counted

- Empty `tagOwners: {"tag:hub": []}`: network report marks `[I]`; headscale test `tag-with-empty-owners-is-valid` suggests it parses. Still run `headscale policy check`.
- TCP gRPC on `127.0.0.1:50443` does **not** listen with `grpc_allow_insecure: false` and no TLS (`app.go:805`, verified). Unix socket is the admin path.
- STUN-off LAN paths, gVisor throughput, Traefik ts2021/DERP: already listed as untested risks.
- Local vectors deferred (D22) is a preference delay, not a correctness hole, if keyword search ships.
