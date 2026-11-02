# Implicit state machines in the Rust workspace

Survey of the `next` branch at 906adcaa, read-only.
"Implicit" means the phase lives in loose bools, Options, stamps, marker files or string prefixes,
or an explicit enum exists but transitions are scattered or impossible states are representable.
Evidence: **[V]** read in code; **[I]** inferred, with what would confirm it. Line numbers are
`crates/<crate>/src/...` at that commit.

## Ranked DO items

Ranked by value/cost. "Bug" means a concrete interleaving is in the section; "tidy" means no
defect, the change only removes representable illegal states.

| Id | What | Kind | Cost | Section |
|---|---|---|---|---|
| D1 | D9 turn state (`EnvoyTurn`) moves into the core as `OwnerTurn`, journaled; message kind typed instead of `continue:`/`ga:`/`resleeve:` prefixes | bug: a hub restart resets the D9 spawn count and drops the origin across a resleeve [V] | M | 2.1, 1.3 |
| D2 | `ParkReason::Ineligible { harness }` replaces the in-memory `Doctor.gated` map | bug: Envoy parked by doctor never restarts after any hub restart [V] | S | 3.2 |
| D3 | Hub member enrollment marked by our own `hub-enrolled` file, not tsnet's `tailscaled.state` | bug: interrupted first enrollment leaves the fleet Down for good [I, high] | S | 4.3 |
| D4 | One `Sleeve::settled_idle()` for both idle checks in api.rs | bug: `wait --until idle` returns just before a continuation starts [V] | XS | 2.2 |
| D5 | Node job: one `finish()` for terminal transitions, cancel intent as data, per-state fields into `JobState` | bug: cancel writes leak forever and are re-probed every start; cancel outcome depends on the lease [V] | M | 1.6 |
| D6 | Core learns harness-started turns (`Obs::TurnBegan`); `Busy`/`TurnEnded` from a driver requeue instead of reject | bug: a message is rejected by a millisecond race [V] | S-M | 5.2 |
| D7 | pt-fleet child supervisor: `Want { Run, Pause, Stop }` + `Phase { Waiting, Spawning, Running{..}, Done }` | bug: `pause()` can return while headscale is spawning, so the pre-upgrade snapshot can be torn [V] | S | 4.2 |
| D8 | Node approval: delete the headscale node if a step after `register` fails | bug: half-approved node no API can remove [V] | XS | 4.4 |
| D9 | Claude driver tracks turns by echo order, with foreign (harness-started) turns | bug: a self-started Claude turn settles our prompt's turn [I: confirm with one recorded trace] | S | 5.1 |
| D10 | Probation phase: one pure `phase()` classifier over the on-disk record; `Verdict::Kept` | bug-adjacent: 9 hand-copied predicates disagree; one data-risk path [V] | M | 3.1 |
| D11 | Core `Life::Resleeving(Resleeve)` owns resleeve data; delete `Life::Recovering`, `Guard.forced`, `Sleeve.receipt`, dead `Action::Resume` | tidy: removes 4 stale-data fields and every (life, guard) pair match | M | 1.1 |
| D12 | Core `Proc::{Starting, Up}(Inc)` owns per-incarnation data; `Activity` variants carry their stamps; `Unknown(cause)` replaces both desync bools | tidy, plus one pane G8 inconsistency | M-L | 1.2 |

Deliberately not proposed: a generic state-machine type, trait or macro; persisting a `phase`
field in `probation.json` (3.1 explains why it would be dropped across releases); enums for
small, correct local loops (node waiter, update wait loop, poller).

## 1. pt-core: the explicit machines, and the implicit ones inside them

pt-core already has the five SPEC section 4 machines as enums (`Life`, `Activity`, `GuardState`,
`EffectState`, `JobState`; `crates/pt-core/src/state.rs:123-399`), one pure `apply`
(`lib.rs:31`), and a strong fuzz harness: `fuzz/src/check.rs` checks Life and Effect steps against
edge tables plus I1-I10, and `fuzz/src/owed.rs` (831 lines) is an independent reference for the
auto-continue rule. The journal holds events only (`codec.rs:44`), so every State change below is
journal-compatible: replay rebuilds the new shape. That makes core refactors cheap to verify.

What is implicit is the product of the enums with ~25 loose fields on `Sleeve`
(`state.rs:31-113`) and `Guard` (`state.rs:198-217`).

### 1.1 Sleeve = Life x Proc x Activity x GuardState + loose fields

```
Life        Provisioning Launching Running Recovering Resleeving Stopping{remove} Stopped Parked(r) Removed
Proc        Down | Starting(eff) | Up
Activity    Booting Idle{seen} Delivering{eff} Working Blocked{approvals} Interrupting Compacting Unknown Ended
GuardState  Armed Interrupting Resuming Quiescing{..} Capturing{brief} Selecting Launching Briefing{eff} Cooldown{until} Parked
```

Legal product as the code actually maintains it:

| GuardState | Life it occurs with | Set at |
|---|---|---|
| Quiescing, Capturing, Selecting, Launching, Briefing | Resleeving only | guard.rs:270, 322, 365, 401; sleeve.rs:1725 |
| Parked | Parked(r) only | sleeve.rs:680-681 (set together) |
| Interrupting | Running | guard.rs:174 |
| Resuming | Launching | guard.rs:229 then sleeve.rs:292 |
| Armed, Cooldown | anything else | sleeve.rs:616, 780, 1313, 1456, 1706 |

| Proc | Activity |
|---|---|
| Down | Ended (sleeve.rs:327-328, 461) |
| Starting | Booting (sleeve.rs:283) |
| Up | anything but Ended |

Representable but illegal: `Running + Selecting`, `Parked(r) + Armed`, `Stopped + Briefing`,
`Proc::Up + Ended`, `Proc::Down + Working`. Code compensates by matching pairs in ad-hoc order:
`start_failed` matches `(life, guard.state)` (sleeve.rs:338-350, two identical arms), `exited`
matches life then guard (sleeve.rs:493-504), `ready` matches the pair (sleeve.rs:1703-1728),
`trip` checks life then an `armed` derived from guard (guard.rs:121-130),
`deliverable` checks five fields (sleeve.rs:1127-1139), and `start_incarnation` only writes
`life = Launching` when guard is not Launching (sleeve.rs:292).

**Life::Recovering is never observable between events.** Every path that sets it
(sleeve.rs:342, 346, 508) calls `after_death` or `begin_resleeve` in the same apply, and every
branch of `after_death` (guard.rs:193-233) leaves for Launching, Resleeving or Parked. The only
other entry, `trip`'s `Action::Resume` arm (guard.rs:186), is dead: nothing trips Crash or Oom
(the 13 `trip(` call sites name none of them). Yet `Recovering` is an API value
(pt-api/src/types.rs:28, pt-hub/src/wire.rs:83) clients can never see, and check.rs needs
multi-hop edges (`MAX_HOPS`, check.rs:7) partly to tolerate it.

**Resleeve data outlives the resleeve.** `Guard.cause`, `chosen`, `chosen_effort`, `tried`
(state.rs:207-214) are only meaningful while Life is Resleeving, but their validity is maintained
by hand at six sites with different subsets: `stop` clears `chosen`+`tried` (sleeve.rs:617-618),
`park_quiet` clears `chosen` only (:682), `start_cmd` clears all four (:781-784),
`on_brief_prompt` clears `tried`,`chosen`,`cause` (:1317-1319), `begin_resleeve_quiet` clears
`chosen_effort` for non-manual causes (guard.rs:251). No bug found today: every path back to a
launch re-sets what it reads. But `chosen_effort` doubles as a parameter channel: `start_cmd`
writes it (sleeve.rs:783) so `start_incarnation` can `take()` it three calls later
(sleeve.rs:273). Dead fields: `Guard.forced` is written (guard.rs:325) and never read (the
effect carries `forced`), `Sleeve.receipt` is written (sleeve.rs:279, 360) and never read by
core or hub.

**Proposed (DO, D10 in the ranking):**

```rust
pub enum Life {
    Provisioning { effect: EffectId },          // replaces Sleeve.provision
    Launching, Running,
    Resleeving(Resleeve),
    Stopping { remove: bool, provision: Option<EffectId> },
    Stopped, Parked(ParkReason), Removed,
}
pub struct Resleeve { phase: Phase, cause: Cause, tried: Vec<Target>,
                      chosen: Option<Target>, effort: Option<String> }
pub enum Phase { Quiescing { deadline: Millis, nudged: bool }, Capturing { brief: EffectId },
                 Selecting, Launching, Briefing { effect: EffectId } }
pub enum GuardState { Armed, Interrupting, Resuming, Cooldown { until: Millis } }
```

`Guard` keeps only the rate windows (`resumes`, `resleeves`, `strikes`, `ooms`, `since_gen`).
Delete `Recovering` (core, pt-api, wire, check.rs table, SPEC 4 table), `Guard.forced`,
`Sleeve.receipt`, the dead `Action::Resume` arm; `after_death`'s precondition becomes its
callers' contract. Pass `effort` to `start_incarnation` as an argument. Why DO: removes four
stale-data fields and every pair-match, and the type then states SPEC 4's "resleeve phases belong
to Resleeving". Cost: ~36 `guard.state` sites in core, 17 in the fuzz crate, 0 in the hub;
`check.rs`/`owed.rs` already compare whole states, so the existing seeds verify it.

### 1.2 The incarnation: 14 fields reset by hand

Per-generation fields on `Sleeve`: `receipt`, `turn`, `desync_during_turn`, `desync_while_idle`,
`stop_at`, `kill_at`, `kill_cause`, `probing`, `boot_at`, `progress_at`, `turn_at`, `blocked_at`,
`interrupt_at`, `loop_hash`, `loop_count`. `start_incarnation` resets them one by one
(sleeve.rs:279-291); `exited` resets a different subset (:462-467) and must read the desync flags
before `after_death` because "a new incarnation clears both desync flags" (:513). Forgetting one
field in a future edit is the failure mode; nothing types it.

The desync pair and `turn` encode one fact three ways ("this incarnation died mid-turn"):
- `desync_during_turn` and `desync_while_idle` (state.rs:62-64) can both be true; only the
  `Desync` handler sets them, always together with `activity = Unknown` (sleeve.rs:1491-1515),
  and headless `Unknown` is absorbing until the exit (no observation leaves it: Ready needs
  Booting, TurnSettled needs Working/Blocked/Interrupting, Screen is pane-only).
- `turn: Option<String>` stays set through Interrupting, which two comments have to warn about
  (sleeve.rs:449-450, 1494-1497), and in `exited` `turn.is_some()` with `was == Unknown` is
  exactly `desync_during_turn` (sleeve.rs:451-455).

**Proposed (DO, D11 in the ranking):**

```rust
pub enum Proc { Down, Starting { effect: EffectId, inc: Inc }, Up(Inc) }
pub struct Inc { boot_at, progress_at, loop_: Option<(u64, u32)>,
                 teardown: Teardown, probing: bool, activity: Activity }
pub enum Teardown { None, Stopping { at: Millis }, Killing { at: Millis, cause: Option<Cause> } }
pub enum Activity {
    Booting, Idle { seen: bool }, Delivering { effect: EffectId },
    Working(Turn), Blocked { turn: Turn, approvals: Vec<String>, since: Millis },
    Interrupting { since: Millis }, Compacting,
    Unknown(Lost),                       // Lost::Screen | DesyncInTurn | DesyncIdle | DesyncOther
}
pub struct Turn { native: Option<String>, at: Millis }
```

`Activity::Ended` disappears (it is `Proc::Down`), as do both desync bools, `Sleeve.turn` and the
five `*_at` stamps; `start_incarnation` builds a fresh `Inc`, `exited` moves the old one out with
`mem::replace` and matches on it. The G5/G6/G8/G9/G13 predicates (guard.rs:93-116) become single
matches. Minor real inconsistency this fixes: pane `Verdict::Working` restarts `turn_at` when
coming from Blocked (sleeve.rs:1746-1748) while headless `ApprovalResolved` keeps it
(sleeve.rs:1424-1428), so a pane that keeps hitting approval prompts never reaches G8 (notify-only
for panes, so low impact). Cost: ~60 `activity` sites in core, 12 in the hub (wire.rs:161-167,
api.rs:1895-1953, core.rs:84, brief.rs:397, harness_update.rs:94), 29 in fuzz. Do it after 1.1;
both are mechanical and the fuzz seeds cover them.

### 1.3 Message kind encoded in request-id strings

Request ids are hub-made tagged strings: `owner:`, `envoy:`, `mail:<id>:`, `ga:<agent>:`
(api.rs:946-953), `continue:<sid>:<seq>` (sleeve.rs:1559), `resleeve:<name>:<g>`
(sleeve.rs:1712). Both crates parse the prefix back to decide behaviour:
sleeve.rs:25 (agent name), 1181 (does the prompt open an owner turn), 1243, 1590, 1596;
hub.rs:311-313 (D9 label); exec.rs:1120, 1157, 1161 (steer vs prompt, D9 origin, resumed);
api.rs:1775-1777, 1819, 1894, 1966, 2002. That is 16 sites where a string prefix stands for a
variant. Not a hole today (clients never choose request ids, `new_request` builds them), but the
"is this a continuation" bit is load-bearing for D9 and for one-continuation-per-turn, and
`turn_from`/`turn_agent` (state.rs:69-71) are a second copy of `ga:<agent>:` parsed at dequeue.

**Proposed (DO, part of D1 in the ranking; it is the core half of the EnvoyTurn fix in 2.1):**
`Message { kind: MsgKind, agent: Option<String>, .. }` with
`enum MsgKind { Prompt, Continuation { turn: u64 }, Brief }`, and the same `kind` on
`EffectKind::SendPrompt` so exec.rs reads a field. `turn_seq`, `turn_from`, `turn_agent`,
`auto_continue` become one `OwnerTurn { seq, from: Party, agent: Option<String>, owes: bool,
spawns: u32 }` (2.1 adds `spawns`). The transcript's `Entry::UserMessage` keeps its request id
for display; give it the same `kind` so api.rs stops parsing (fresh install, no compat needed).
Leave the continuation rule itself alone: its 12 fix commits
(`git log -- crates/pt-core/src/sleeve.rs | grep -i continu`) are spec complexity, and `owed.rs`
already pins it.

### 1.4 Effect record: leave it

One owner (`effect_report`, effect.rs:56-86, a `(state, report)` match), plus `evidence`,
`not_applied`, `unknowable`, `resend_or_give_up`, `probe` that each set one documented edge, all
checked against `EFFECT_EDGES` in check.rs:322-335. `unknowable: u32` is a per-record counter,
not a phase. SKIP.

### 1.5 Approval: state plus two Options

`Approval { state, decision: Option<Decision>, orphan: Option<Orphan> }` (state.rs:276-289) with
`Decision.response: Option<EffectId>`. Legal: Pending has no decision; Decided, Expired, Fenced
have one; Cancelled has none; Abandoned has either (owner decision at approval.rs:139, none at
:337); `response` is Some only for a Decided tool approval; `orphan` matters only while Pending.
`decide` guards the Pending-only rule with a `debug_assert` (approval.rs:30), while
`withdraw_tool_approvals` (:267) and `on_response` (:304) write `state` directly, bypassing it.
No bug found; `approval_steps` in check.rs covers the edges. The hub silently maps a
`Decided` with `decision: None` to Denied (pt-hub approvals.rs:34-35), a case the type allows.
**MAYBE:** `enum ApprovalState { Pending { orphan: Option<Orphan> }, Decided(Decision),
Expired(Decision), Fenced(Decision), Cancelled, Abandoned(Option<Decision>) }`. Small (371-line
file plus hub approvals.rs/wire readers), but no defect to fix; do it only alongside other work
in that file.

### 1.6 Node job: explicit enum, but facts outside it and no single terminal path (D5)

`JobState` is explicit (state.rs:368-381), but `Job` (state.rs:339-366) keeps state-scoped facts
beside it, and terminal transitions are spread over ~11 `set_job` sites (job.rs:289, 341, 345,
353, 403, 435, 462, 466, 494, 503, 551).

| Field | Meaningful in | Problem |
|---|---|---|
| `needs_new_attempt` | SubmitUncertain only (readers job.rs:490, 516 both test `state == SubmitUncertain && flag`) | cleared only by `requeue_job` (:532); the Refused branch (:295-305) and `on_submit` Failed/Rejected (:214-229) go back to Queued with it still `true`; if that attempt re-enters SubmitUncertain (Submitting then HubRecovered, :583) abandon/requeue are allowed before the probe answers. Needs an epoch-changed Unknown then a Refused: rare |
| `exit`, `verify_failures` | Collecting | `Failed(Exit(code))` already carries the code |
| `lost_at` | Lost and Collecting (one loss episode across bounces) | fine: an episode, not a phase |
| "owner cancelled" | Cancelling, and Lost reached from Cancelling | not stored; inferred from whether NodeCancel records exist (job.rs:318) |

```
Cancelling --lease--> Lost --Running-----> Cancelling      (cancel inferred from effects)
                      Lost --Exited------> Collecting -> Succeeded|Failed   (cancel forgotten)
                      Lost --Cancelled---> Cancelled       (cancel writes left open)
                      Lost --Interrupted-> Interrupted     (cancel writes left open)
Cancelling --Exited|Cancelled|Interrupted--> Cancelled     (closes them: the only goal_reached site, job.rs:355)
```

**Bug [V].** After cancel, lease expiry (job.rs:560) and a late Exited/Cancelled/Interrupted, the
attempt's NodeCancel records are never closed: `resend_node_writes` only resends in
Cancelling/Lost/Abandoned (job.rs:116-119), `wanted()` is false outside Cancelling
(effect.rs:243), and `tick_effects` skips node effects (effect.rs:189). They stay
Uncertain/Probing forever, and `recover_effects` (effect.rs:316-334) re-probes them on every hub
start; the answer falls into `_ => {}` (job.rs:359). The same cancel also ends as Cancelled or as
Succeeded/Failed depending only on whether the node answered inside the lease.

**Proposed.** One `fn finish(&mut self, id, end: JobState)` that every terminal site calls: it
closes the attempt's open NodeCancel writes (`goal_reached`), emits the collected ack where
needed, and sends `JobFinished`. Store the cancel as data (`Job.cancel: Option<EffectId>` or
`Lost { cancelling: bool }`) so (Lost, Exited) with a cancel goes to Cancelled like the
Cancelling arm. Move `needs_new_attempt` into `SubmitUncertain { epoch_changed: bool }` and
`exit`/`verify_failures` into `Collecting { exit, verify_failures }`. JobState is not in the
codec, so no journal change. ~100 lines in job.rs/effect.rs plus the hub's job wire matches; one
fuzz seed (cancel, lease, Exited).

Related, MAYBE: the submit epoch comes from the poller's Hello (pt-hub jobs.rs:501), the submit
goes over the executor's own connection with its own Hello (jobs.rs:731). A node restart inside
one poll interval plus a lost submit reply sets `needs_new_attempt` for a job the node never saw:
safe (no double run) but needs a manual requeue. Fix only if seen: the executor refuses to
dispatch when its connection's epoch differs from the recorded one.

### 1.7 Dead or never-observable states

- `Activity::Compacting` / `EventKind::Compact` / `EffectKind::Compact`: the executor always
  Nacks it ("explicit compaction is not exposed by the drivers", pt-hub exec.rs:738-740), so the
  state is entered and failed in consecutive applies. MAYBE: delete the command, effect and
  variant until a driver supports it (owner call: is explicit compaction coming soon?).
- `Life::Recovering`, `Guard.forced`, `Sleeve.receipt`, `trip`'s `Action::Resume` arm: see 1.1.

### 1.8 Doc drift (no code change)

- design doc 2.d, Briefing row "effect Uncertain or Fenced -> Cooldown, no resend" is superseded
  by SPEC 12b item 7 (park); code parks (sleeve.rs:1327-1336). Fix the doc row.
- design doc 2.b `Working{turn, native_turn?, compacting}` vs code `Working` with
  `Sleeve.turn` and a separate `Compacting`; 1.2 above brings code closer to the doc.
- SPEC 4 lists `Recovering`; drop it with 1.1.

## 2. pt-hub orchestration around the core

### 2.1 `EnvoyTurn`: D9 origin and spawn count, unjournaled (D1)

`Mutex<EnvoyTurn { origin: Option<String>, spawns: u64 }>` (pt-hub hub.rs:80-85, 92), created as
`Default` at hub.rs:151. Writers, on three threads:

| Site | Trigger | origin | spawns |
|---|---|---|---|
| exec.rs:1161-1170 `note_envoy_origin`, plain prompt | new turn | agent or marked-mail label, else None | 0 |
| same, `continue:`/`resleeve:` prompt (string prefix test, :1161) | resumed turn | replaced only if the label is D9 | kept |
| same, steer | taint | replaced only if D9 | kept |
| exec.rs:1553, 1583 on `TurnSettled` | settle | None unless UsageLimit/AuthFailed | 0 unless UsageLimit/AuthFailed |
| rule.rs:40 `gate(Spawn)` while origin is Some | attempt | - | +1 (denied attempts count) |

The core already journals the same fact as `Sleeve.turn_from`/`turn_agent` (pt-core state.rs:69-71,
written at dequeue sleeve.rs:1181-1187 and steer). The hub mirrors it by hand in memory.

**Bugs [V], both violate SPEC 2 principle 10 / SPEC 4 ("the continuation of that turn does not
start the count over"; "the Envoy resleeve brief is sent as the owner, so it does not clear
them"):**
1. Envoy in a D9 turn with origin `agent:x`, spawns at the limit. The hub restarts (update re-exec
   or crash): `EnvoyTurn::default()`. Recovery kills and resumes the incarnation, `Continue`
   follows, `note_envoy_origin` (resumed) restores the origin from `from_label`, which reads the
   journaled `turn_agent` (hub.rs:313-318), but spawns stays 0. Envoy gets `d9_spawn_limit` more
   unasked spawns.
2. A D9 turn settles UsageLimit; the origin is kept on purpose (exec.rs:1553); G1 resleeve is
   journaled. The hub restarts: origin None. The brief prompt `resleeve:envoy:<g>` has no message,
   labels as owner, and resumed=true keeps None. The brief turn runs ungated and `marks_d9`
   (api.rs:958) stops marking its spawns and sends, while the core's `turn_from` still says Agent.

No e2e test covers the origin across a restart (durable.rs:136 covers only a pending approval).
Open owner question: any settle other than UsageLimit/AuthFailed clears the origin, including
`Interrupted` and `Error` (exec.rs:1553), so a D9 turn quiesced by an interrupt and then resleeved
gets an ungated brief turn; hub.rs:79 says "a finished settle" clears. Decide which is meant.

**Proposed.** Delete `EnvoyTurn`, `note_envoy_origin` and the `clear_envoy_turn` plumbing. With
the `OwnerTurn` of 1.3 in the core:
- D9 origin is a pure function of the view: `d9_origin(&State, envoy) -> Option<String>` from
  `OwnerTurn.from`/`agent` (Agent, or a `Sleeve(x)` with `x.d9`).
- Clear-on-settle moves to the core's TurnSettled arm: reset on Done (and Interrupted/Error if
  the owner says so), never on UsageLimit/AuthFailed; a crash, the exit, a continuation and the
  brief never touch it, which is SPEC's rule stated once.
- `OwnerTurn.spawns` increments when an Envoy Spawn applies in a D9 turn (needs a `by_envoy`
  field on `SpawnSpec`, one codec tag as was done for `d9`). It resets where `turn_seq` increments.
  Denied attempts then stop counting, which is arguably more correct; say so in SPEC.
- `rule::gate` reads the count from the view and holds one hub mutex across gate and submit for
  Envoy spawns, so two concurrent spawns cannot both pass.
Replay then gives D9 across restarts for free and `sleeve_core` fuzz sees it. Cost: one codec
field, about -60 lines hub, +30 core, one e2e test (spawn limit across `hub.restart()` in a
continued turn, and one across a resleeve).

### 2.2 "Idle" derived twice with different rules (D4)

`ask_envoy` (api.rs:1891-1895): Idle and no `auto_continue` and no `continue:` queued, with a
comment that the view is published before the batch's intents run. `reached`, used by
`sleeve wait --until idle` (api.rs:1943-1949): Idle and empty queue and Running, no
`auto_continue` check. **Race [V]:** after a crash, resume reaches Ready (sleeve.rs:1700-1707),
`offer_auto_continue` emits `Intent::AutoContinue` (sleeve.rs:1626), the executor submits
`Continue` (exec.rs:606-607) for a later batch; in between the published view is Running + Idle +
empty queue + `auto_continue: Some`, so `wait --until idle` returns just before the continuation
starts. **Proposed:** `Sleeve::settled_idle()` in pt-core (Running, Idle, empty queue, no owed
continuation, `!probing`), used by both. One commit, no codec change.

### 2.3 Hub-area MAYBE / SKIP

| Item | Now | Verdict |
|---|---|---|
| Brief marker (brief.rs:34-41, 110, 205; probe exec.rs:806) | marker is the first line, `fs::write` is not atomic or fsynced, so a torn write keeps the marker and the probe says Applied for a truncated brief | MAYBE: temp + fsync + rename, marker last. Needs a crash inside one 16 KiB write |
| History `capped`, three copies (pt-history index.rs:70, 212, sync at 550, 1316, 1372, 1384, 1398, 1453, 1477; pt-hub history.rs:90, 227-239) | per-source bool + derived set + hub copy, hand-synced; no bug found | MAYBE: one persisted `BTreeSet<CapKey>` with `CapKey = Sleeve(String) \| Source(String)`, -50 lines |
| `open_index` (pt-hub history.rs:115-142) | every error except WouldBlock/PermissionDenied counts as corruption, so ENOSPC/EIO at open triggers a rebuild | MAYBE, one line: match `InvalidData` only |
| Core loop batching (core.rs:23-127) | fixed phase order, `batch_n == 0` the only phase variable | SKIP |
| Approvals hub side (approvals.rs) | waits on the view; restart becomes `Orphan::HubRestarted` | SKIP (core shape in 1.5) |
| Quota cache `Slot` (pt-quota cache.rs:43-50) | only `settle` (:133-155) writes it; `pending` only after a timeout | SKIP |
| Journal (pt-journal lib.rs) | `poisoned` checked on every write, fuzzed | SKIP |
| Provision markers (git.rs:221-235, exec.rs:816-853) | started marker before the effect, fsynced barrier after; probe reads only the barrier | SKIP, correct |
| Devices, serve ordering (devices.rs; serve.rs:357-367) | `hub.stop` set after bind, so a `restart_soon` in the first milliseconds logs "no serve loop" | SKIP |

## 3. Updates, probation, doctor

### 3.1 Release probation record (D10)

The trial phase is never stored. It is derived from five fields of `Probation`
(pt-update store.rs:53-74: `contacted_ms`, `rolled_back_ms`, `manual_rollback`,
`restore_failed`, `reason`), the `current`/`previous` symlinks and the `releases/rollback-open`
marker (store.rs:1148-1192), with the predicate re-written by hand at nine sites:
`open_probation` (store.rs:847-850), `judge` (:1028-1059, :1059 re-tests `rolled_back_ms`,
dead), `confirm` (:872), `switch_on_probation` (:780), `request_manual_rollback` (:939, :951),
`retry_restore` (:989-990), `start_attempt_with_restore` (:910), pt-hub `probation_report`
(update.rs:1272-1286), `probation_at_start` (update.rs:385-389), `announce_restore_failure`
(update.rs:1430-1437), and serve.rs:119-127 re-reads the record to tell "confirmed" from "kept
after a failed restore".

```
NotSwitched (crash before switch; judge re-points previous on every call, never terminal)
Trial{deadline, starts} --confirm--> Confirmed --manual rollback--> RollbackDue{manual}
Trial --request/deadline/starts>max--> RollbackDue{reason}
RollbackDue --restore ok--> RolledBack (marker cleared)
RollbackDue --restore failed, no marker--> Kept{error} --retry--> RollbackDue
RollbackDue --restore partial or marker--> RestoreIncomplete --retry--> ... --> RolledBack
```

Representable but illegal: `rolled_back` with `restore_failed`; `manual` with neither
`contacted` nor `reason`; a marker for a record that no longer names `current`
(`open_rollback_for`, store.rs:1170, filters it).

Evidence:
1. `probation_at_start` skips the record when `contacted_ms` is Some. An incomplete manual
   rollback of a confirmed release (contacted, manual, reason, marker, live files half replaced)
   then starts a direct `hub serve` on half-restored data; only prestart's Err (store.rs:918)
   protects it. `judge`, `probation_report` and `open_probation` classify that state three
   different ways. Low likelihood (serve without prestart) but on a data-loss path.
2. `watch_probation` (update.rs:1474-1479) re-reads `deadline_ms` and falls back to `0` on a read
   error; the first health check fails by design (`batch == 0`, :1447), so a failed read restarts
   at once and burns one of `MAX_TRIAL_STARTS = 3`. The caller already holds the record.
3. `retry_restore`'s "partial" test (`contacted None && reason Some`, store.rs:990) re-encodes
   what the marker already says.

**Why not a stored enum:** the record is written by the old hub, confirmed by the trial binary
and judged by the old binary's prestart; `record!` ignores unknown keys (pt-json macros.rs:4) and
`write_record` rewrites the whole record (store.rs:828-838), so a new `phase` key would be
dropped by the other release during exactly the update that adds it.

**Proposed:** keep the on-disk fields. Add one pure classifier in store.rs,
`fn phase(p: &Probation, current: Option<&str>, marker: bool) -> Phase` with
`enum Phase { Idle, NotSwitched, Trial { deadline_ms, starts }, Confirmed,
RollbackDue { reason, manual }, RestoreIncomplete { error }, Kept { error } }`, and route all nine
sites through it; add `Verdict::Kept { error }`; make NotSwitched terminal once (write
`rolled_back_ms`); `probation_at_start` returns the Trial deadline to `watch_probation` (no
re-read, no `0`). Writes stay where they are. ~120 lines, no format change, covered by the
existing update e2e tests (`partial_*_rollback_does_not_keep_the_trial`,
`release_switches_survive_a_crash_at_each_step`, the node trial tests).

### 3.2 Doctor launch gate: journaled park, in-memory reason (D2)

"Envoy parked because doctor refused its harness" is split: `Life::Parked(ParkReason::NoTarget)`
in the journal (via `SelectTarget` seeing `envoy_gated`, exec.rs:576-579) and
`Doctor.gated: Mutex<BTreeMap<SleeveId, String>>` in memory (doctor.rs:38). `unpark_envoy`
requires both (doctor.rs:181). **Bug [V]:** after any hub restart `gated` is empty while the
journal still says `Parked(NoTarget)`, nothing re-gates at boot (serve.rs:219 only spawns a
missing Envoy), so `unpark_envoy` returns early forever, breaking doctor.rs:4-5 ("starts again
once a later run finds that harness eligible"). A hub update re-execs, so this follows the most
common restart. The e2e test `doctor_refuses_an_ineligible_harness_and_envoy_waits_for_it`
(protectorate tests/update.rs:2206) never restarts. **Proposed:** `ParkReason::Ineligible
{ harness }` journaled through `TargetChosen`; `unpark_envoy` matches it; `gated` stays as display
text or goes. One variant, one codec arm; extend that e2e test with a restart.

### 3.3 Update-area MAYBE / SKIP

| Item | Now | Verdict |
|---|---|---|
| Harness-update scheduler (harness_update.rs:45-52: `last`, `wanted`, `running`) | busy check is one-shot (:268) before a run of up to 15 min (:26, :278); nothing gates launches meanwhile (`harness_updates` unread in exec.rs), so a resume/spawn/resleeve can exec a harness its updater is replacing; a failed launch trips G5 and resleeves. Whether a mid-install exec fails depends on the vendor installer [I] | MAYBE: `gate_launch` (doctor.rs:285) waits while `running` names that program. Folding the three fields into one map: SKIP |
| `update_hub` snapshot cleanup (update.rs:219-353) | snapshot location tracked by `headscale_dir: Option` plus ~9 hand-placed `discard_dir` calls; correct. Snapshots are taken before `switch_on_probation` refuses an open trial (store.rs:763-768) | MAYBE: a guard value whose Drop discards unless kept; pre-check the trial before snapshotting |
| Node `Releases` (protectorate-node update.rs:33-52) | `trial: AtomicBool`, `updating: Mutex<bool>`, `Admission { switched }` RAII | SKIP; reuse 3.1's `phase()` in `Releases::open` |
| `update_one` wait loop (update.rs:1757-1785) | locals `gone`, `last` | SKIP |
| Pointer swap (store.rs:526-592), `Doctor` refresh flag, pt-doctor `Credentials { checked, present }` | explicit and crash-tested / ordinary condvar / wire snapshot | SKIP |

## 4. Fleet and nodes

### 4.1 Overview

| Item | Verdict |
|---|---|
| 4.2 child supervisor flags (pt-fleet child.rs) | DO (D7) |
| 4.3 hub member enrolled marker (pt-fleet member.rs:39-41) | DO (D3) |
| 4.4 node approval steps (pt-fleet enroll.rs:173-221) | DO (D8) |
| 4.5 `NodeExec` `pending_status` side channel, join resumability, install-service order, `FleetSlot::shutdown` while Starting | MAYBE |
| `NodeClient` `broken` + `disconnected`; node job table; waiter locals; `FleetSlot`; poller; scheduler `pending`; `supervise`; node member loop; presence | SKIP / leave |

### 4.2 pt-fleet child supervisor (D7)

`Flags { state: ChildState { pid: Option, healthy, starts, last_error }, paused, stopping, done }`
(child.rs:36-51) mixes requests (`paused`, `stopping`) with observations (`pid`, `healthy`,
`done`); the phase (Spawning, Running, Backoff, Paused, Done) is the position in `run()` plus
`pid`. Checked in ad-hoc order: `while f.paused && !f.stopping` (:186), `if f.stopping ||
f.paused` (:278), `if f.paused || Asked` (:209). Representable: `paused && stopping`,
`healthy && pid == None`. **Race [V]:** `pause()` (:124-131) waits only while `pid.is_some()`; if
`run()` is inside `spawn()` (:194, lock not held, pid not yet set at :262-264) after a crash or
backoff, `pause()` returns at once and `snapshot_headscale` (lib.rs:297-310) copies the state
while the new headscale opens its database and may migrate. That snapshot exists because
"migrations are one-way" (lib.rs:295). **Proposed:** under the same mutex,
`want: Want { Run, Pause, Stop }` (Stop dominates by construction) and
`phase: Phase { Waiting, Spawning, Running { pid, healthy }, Done }`; `run()` sets Spawning under
the lock before `spawn()`; `pause()` waits for Waiting or Done. ~40 lines.

### 4.3 Hub member "enrolled" = tsnet's state file exists (D3)

`enrolled(cfg)` is `state_dir/tailscaled.state` exists (member.rs:39-41) and `Fleet::start` runs
`member::enroll` only when it is missing (lib.rs:191-193). Steps: registration printed,
fingerprint checked, `admin.register`, registered printed, `admin.tag(TAG_HUB)`
(member.rs:103-120). `ensure_hub_tag` (lib.rs:227-239) repairs registered-but-untagged, nothing
repairs started-but-never-registered. **Bug [I, high confidence]:** tsnet's FileStore is
`Dir/tailscaled.state` (go/cmd/member/main.go:130-131) and tailscale writes the machine key there
at start, before registration. A first enrollment cut short before `register` (hub killed,
registration timeout, fingerprint mismatch) makes the next start skip enroll; member hits
NeedsLogin and exits (main.go:320), `wait_healthy` fails (lib.rs:214), the fleet is `Slot::Down`
on every start with no recovery. Confirm: kill the hub during first-start enrollment and look for
the file. **Proposed:** our own `hub-enrolled` file holding the node id, written after
`admin.tag` succeeds; absent means enroll (an existing unregistered key just re-prints its
registration, main.go:289-302). Fresh installs only, so no migration. ~10 lines.

### 4.4 Node approval is an unrecorded step sequence (D8)

`register` (enroll.rs:198), `tag(TAG_NODE)` (:199), `rename` (:203), write `enrolled.json`
(:205-214), prune `pending.json` (:215-219). Any `?` after `register` leaves a registered,
untagged node: a retry fails at `register` or "already exists" (:186-196), the poller filters on
`TAG_NODE` (poller.rs:210), and `remove_node` refuses an untagged node (enroll.rs:293-298). Only a
manual `headscale nodes delete` gets out. **Proposed:** on any failure after `register`,
`admin.delete(node.id)` and say so in the error; the node re-runs `join`. ~10 lines. Not a phase
record: cleanup is simpler than tracking.

### 4.5 Fleet MAYBE

- `NodeExec.pending_status: Option<(JobId, u32, JobReply)>` (pt-hub jobs.rs:636), filled by
  `run_job` (:1033) and the cancel closure (:881), read in `handle` (:933): a return value carried
  through `self`. No leak today. Return it instead, and keep `Option<(NodeClient, String)>` as one
  connection value (`client` + `epoch`, :629-640). ~20 lines, cleanup only.
- `protectorate-node join` (join.rs:62-118): phase in files (no `node.json` = not joined, member
  state dir = registration progress). Ended after approval but before `node.json` is written, a
  re-run sees member already Running print only `registered` (main.go:282-288), and
  `next_event("registration")` fails with a misleading "is URL reachable" (join.rs:146-151).
  Accept `registered` first when `node.json` is absent. ~15 lines.
- install-service (service.rs:177-183) writes the unit before the manifest that records it; a
  failed `save` leaves an unit that install refuses and uninstall cannot find. Swap the two
  writes (record before effect).
- `FleetSlot::shutdown` while Starting (pt-hub fleet.rs:116-128, 263-270) returns before the start
  thread tears its `Fleet` down. Matters only for an in-process restart [I]; add
  `Slot::Stopping` if seen.

## 5. Harness drivers and the exec pump

Context [V]: the core absorbs most driver quirks (Ready only from Booting, sleeve.rs:1694;
idempotent ApprovalResolved; TurnSettled outside a turn ignored, :1436-1445; Progress in Idle
ignored, :1386). The structural gap is that each driver keeps its own `turn: Option<Turn>`, a
second copy of Activity, and some drivers create turns the hub did not start (Codex
codex.rs:511-519, Pi pi.rs:250-255 and 390-400; Claude notices but does not track one,
claude.rs:276-283). The pump turns `Entry::TurnStarted { request_id: None }` into plain
`Obs::Progress` (exec.rs:1511-1514), so the core can be Idle while the harness is busy.

### 5.1 Claude: a self-started turn settles ours (D9)

`turn` is set when we send (claude.rs:581-595) and taken by any `result` (:471); a replayed
task-notification is dropped without recording a turn (:277-282). Interleaving: our turn A
settles; Claude starts task-notification turn B (driver `turn` None); the core is Idle and
delivers C, which Claude queues behind B; B's progress confirms C via `prompt_evidence`
(sleeve.rs:1533-1538); B's `result` takes C's Turn and the core settles "C" holding B's output;
C's real echo is a `stale_echo` (:322-323) and its output and settle arrive while Idle and are
ignored. [I] confirm with one recorded trace that has a background task. **Proposed:**
`pending: VecDeque<(request_id, uuid)>` for written-unechoed prompts and
`running: VecDeque<Run>` with `enum Run { Ours { request_id, interrupt: Interrupt }, Foreign }`,
`enum Interrupt { None, Sent(String), Accepted }` (replaces `interrupt: Option<String>` +
`interrupt_accepted: bool`, where accepted-with-no-id is representable). Each echo pushes,
`result` pops the front, `send_user` is Busy while `running` is non-empty, a foreign echo emits
`TurnStarted { request_id: None }`. One fuzz seed.

### 5.2 Busy and lost-steer races end as MessageRejected (D6)

exec.rs:1102-1108 maps every `DriverError` except `Ended` to `Dispatched` then `Nacked`, which
becomes `Rejected` and `MessageRejected` (sleeve.rs:1254-1256, 1353-1355), though nothing was
written: (a) `Busy` while the driver is in a foreign turn the core does not know about; (b)
`TurnEnded` on a steer the Codex/Pi driver saw complete (codex.rs:960, pi.rs:508) before the core
applied that settle. Design 2.b says such a steer goes to the head of the queue; `return_steer`
(sleeve.rs:1230) only covers steers still in the slot. **Proposed:** map a foreign
`TurnStarted` to a new `Obs::TurnBegan` (Idle to Working with no prompt effect, so the core stops
delivering and the later settle lands normally), and add an exec outcome `Unwritten` for
`Busy`/`TurnEnded` that puts the message back at the head of the queue. One Obs codec tag, one
report variant, owned by the existing `observed` and `effect_report`.

### 5.3 Driver MAYBE / SKIP

| Item | Now | Verdict |
|---|---|---|
| Codex `Turn` (codex.rs:52-58) | `native: Option`, `started`, `request_id: Option`, `interrupt_wanted`, `errored`; `started && native == None` is produced (:637-643) | MAYBE with D6: `enum Phase { Requested { request_id, interrupt_wanted }, Running { native, request_id: Option } }` + `errored` |
| Pi `Turn` (pi.rs:49-58) | `request_id: Option`, `answered`, `started`, `errored`, usage | MAYBE: `enum Origin { Ours { request_id, answered }, Foreign }` |
| Grok malformed prompt reply (grok.rs:232-258, 668-670) | turn taken, early return, no settle; interrupt then no-op, recovery waits for G6 (15 min) or G13 | MAYBE: keep the turn until a real settle |
| Session phase (pt-drivers session.rs:49-60: `stdin: Option`, `exited`, `stopping`) | `read_stdout` (:339-356) ignores `exited`, `wait_exit` gives up on the reader after 2 s (:406), so a grandchild holding the pipe yields entries after `Exited`, which the pump (exec.rs:1279) drops | MAYBE: `enum Phase { Live(Sender), StdinLost, Stopping, Exited }`, drop lines once Exited with one notice. ~30 lines |
| Pump error memory (exec.rs:1335-1350) | three `Option<ErrorClass>` for three questions; `last_error` cleared at TurnStarted (:1511), so a quota error before turn-start would settle as Error [I, no trace seen] | SKIP |
| Transcript `peek_seq` (transcript.rs:154-164, hub.rs:260-280) | notices from other threads (core.rs:329, approvals.rs:159, 214) can take the seq a streaming block expected; live view shows a split block until the final lands | MAYBE, cosmetic: reserve the seq when a block opens |
| Transcript `turn` counter vs `Stats.turns` | re-derive the core's `turn_seq`, drift on foreign turns, display only | SKIP |
| pt-http ws.rs (`Assembler.partial`, `WriterState.closed`, ping tracking), server.rs:61-67 `phase: AtomicU8` | explicit enough; benign shutdown race between `fill_buf` and BUSY (:323-327) | SKIP |
| stream.rs, bus.rs | ring counters, not lifecycles | SKIP |

## 6. Implementation order

Each commit keeps the build green and runs only the targeted e2e/fuzz seeds; full `make check`
once at the end of the batch.

1. **D2** `ParkReason::Ineligible { harness }`: core model + codec arm + guard, exec
   `TargetChosen`, doctor `unpark_envoy`; extend the doctor e2e test with a hub restart.
2. **D4** `Sleeve::settled_idle()` in pt-core; api.rs `reached` and `ask_envoy` use it.
3. **D3 + D8** pt-fleet enrollment: `hub-enrolled` marker; delete-on-failure in approval.
4. **D7** pt-fleet child supervisor `Want`/`Phase`.
5. **D1a** typed message kind (1.3): `MsgKind` on `Message` and `EffectKind::SendPrompt`, agent
   name as a field, transcript `UserMessage.kind`; delete every `starts_with("continue:")`/`ga:`/
   `resleeve:` test in core, exec.rs, hub.rs, api.rs. fuzz `check.rs`/`owed.rs` follow.
6. **D1b** `OwnerTurn` with `spawns` in the core, clear-on-settle in the core, `by_envoy` on
   `SpawnSpec` (codec tag), `d9_origin()`, serialized gate; delete `EnvoyTurn`,
   `note_envoy_origin`, `clear_envoy_turn`. New e2e: spawn limit across `hub.restart()` in a
   continued turn and across a resleeve. Settle the Interrupted/Error question with the owner
   first.
7. **D5** job `finish()`, cancel as data, `SubmitUncertain { epoch_changed }`,
   `Collecting { exit, verify_failures }`; hub job wire matches; fuzz seed cancel/lease/Exited.
8. **D6** `Obs::TurnBegan` (codec tag) + exec `Unwritten` outcome requeue; the Codex/Pi `Turn`
   enums (5.3) ride along if touched.
9. **D9** Claude echo-ordered turns, after confirming with a recorded trace; one fuzz seed.
10. **D10** probation `phase()` classifier and `Verdict::Kept`, all nine sites in one commit;
    run the update e2e suite.
11. **D11** `Life::Resleeving(Resleeve)`, `GuardState` down to Armed/Interrupting/Resuming/
    Cooldown, delete `Life::Recovering` (core, pt-api types.rs:28, wire.rs:83, `make gen` for
    the app, SPEC 4 table), `Guard.forced`, `Sleeve.receipt`, the dead `Action::Resume` arm;
    `effort` becomes a `start_incarnation` argument. Replay the `sleeve_core` seeds.
12. **D12** `Proc::{Starting, Up}(Inc)`, `Activity` variants with their stamps,
    `Unknown(cause)`; drop `Activity::Ended`, both desync bools, `Sleeve.turn`, the `*_at`
    stamps. Replay the seeds. Decide the 1.7 Compact question before this, since it removes or
    keeps a variant.
