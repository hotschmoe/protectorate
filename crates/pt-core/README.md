# pt-core

The pure orchestration core: `apply(&mut State, &Event) -> Vec<Intent>`. It does no I/O and
reads no clock, so `replay(journal) == live state`. The hub's core thread is its only caller.

## The hub's loop

1. Turn harness output, scope watches, timers (a `Tick` every 10 s) and API commands into `Event`s,
   stamped with the hub clock in `Event::at`.
2. `apply` each event, in the order the core thread receives them.
3. Journal `codec::encode(&event)` with group commit (`pt_journal::Journal::append_batch`).
4. Only after that batch's fdatasync returns, release the batch's intents to the executors
   (SPEC principle 5, 12b item 4).
5. After a restart: replay the journal, then apply `HubRecovered` once, before any new input.

## Events

- Commands: `Spawn`, `Send` (queue or steer; mail is a `Send` from `Party::Sleeve`), `Interrupt`,
  `Compact`, `RespondApproval`, `Stop`, `Remove`, `Start`, `Resleeve`, `Abandon`, `Viewed`,
  `CreateJob`, `CancelJob`, `AbandonJob`, `RequeueJob`, `RequestApproval` (a code rule asks the
  owner, D9). Each gets exactly one `Intent::Reply`.
- Effect reports, carrying the record's `attempt`: `Dispatched` (sent before the first byte;
  it is answered `Accepted` only while the record is still `Intent`, so an executor that asks
  writes nothing the core has taken back),
  `Acked` (with the native turn id for prompts), `Nacked` (definitive: nothing applied), `Lost`
  (timeout or channel lost), `Probed`.
- `Launched` is the launch receipt (scope, pid), journaled as soon as spawn returns (12a item 2).
- Answers to read-only intents: `ScopeProbed`, `TargetChosen`, `NodeChosen`, `JobStatus`
  (also the answer to a `Probe` of a node kind; `node` is the node that answered, and a report
  from any node but the job's current one is dropped), `ArtifactsChecked`.
- `Observed { sleeve, g, obs }` from drivers and pane screen rules; `Exited { sleeve, g }` once
  the scope is confirmed empty. A stale `g` is dropped.
- `Tick`, `HubRecovered`.

## Approvals

`State::approvals` is the ledger: a harness's tool request (id `<sleeve>:<g>:<native>`) or an
action approval (`action:<n>`). Each is decided at most once (`RespondApproval` by the owner, or
a rule's denial on timeout). A tool decision becomes a `RespondApproval` record only while the
incarnation that asked is the sleeve's live headless one; otherwise it is recorded `Fenced` and
nothing is written. A pending approval whose incarnation died, or whose waiting API call ended
with the hub (`HubRecovered`), is marked `orphan`; an orphaned action approval decided later is
`Abandoned`. Deadlines are journaled times, so downtime counts and nothing restarts a timer.

## Intents

- `Execute { effect, attempt, kind }`: perform an effect record's write. `Probe` and
  `ProbeScope`: find out whether it landed (the probe of `StartIncarnation` is `ProbeScope`).
- `SelectTarget`: run the pure `select_target` (outside the core) and answer `TargetChosen`.
  `ScheduleJob`: run node scheduling and answer `NodeChosen`.
- `PollJob`, `ReadLog`, `FetchArtifacts`, `Checkpoint`: reads and git checkpoints, no record.
  `ReadLog { upto }` is the node's log length: the hub appends to its own copy from the length
  it holds.
- `Notify`, `Reply`: for people, Envoy and sleeves.

## Execution rules

- Per sleeve (and per job) the executor is serial. Immediately before the syscall it re-checks
  the record in the current state: same `attempt`, still `Intent`, and for incarnation kinds the
  sleeve's `g` unchanged. Otherwise it drops the intent.
- Report `Dispatched` before writing; node executors wait for its answer. A job cancelled while
  its submit is still `Intent` is `Cancelled` at once and the submit `Abandoned`, with no cancel
  sent. An Unsafe kind (prompts, steers, approvals, compaction)
  is never retried by any path: the core sends it once, and the hub must never resend it. A
  repeated `RespondApproval` while a response may apply answers `Duplicate` or is refused.
- A sleeve's observations and its executor's reports reach the core in one ordered stream:
  output read before a write's `Dispatched` is applied before it. Harness output confirms an
  in-flight prompt or compaction only once its `Dispatched` is in.
- `Exited` means the scope `pt-<sleeve>-g<g>` has no tasks, not merely that a pipe closed.
- Effects never assumed: until an `Acked`, `Launched` or other evidence arrives, the core treats a
  write as possibly applied (Uncertain) and never as done.
- Nothing waits forever. Each `Tick` sends an Uncertain Keyed or Idempotent record again while
  its owner wants it (3 attempts), and probes the rest again until the owner has been told. Node
  cancels and collected-acks are re-sent rather than probed, since a status cannot tell whether
  they landed. `Abandon` takes an Uncertain or Probing sleeve write; node writes belong to the
  job (`AbandonJob`, `RequeueJob`).

Thresholds are in `limits`. The fuzz targets under `fuzz/` check I1 to I10 after every step, and
`fuzz/seeds/sleeve_core/` holds the scenario scripts (format in `fuzz/src/script.rs`).
