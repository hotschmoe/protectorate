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
  `CreateJob`, `CancelJob`, `AbandonJob`, `RequeueJob`. Each gets exactly one `Intent::Reply`.
- Effect reports, carrying the record's `attempt`: `Dispatched` (sent before the first byte),
  `Acked` (with the native turn id for prompts), `Nacked` (definitive: nothing applied), `Lost`
  (timeout or channel lost), `Probed`.
- `Launched` is the launch receipt (scope, pid), journaled as soon as spawn returns (12a item 2).
- Answers to read-only intents: `ScopeProbed`, `TargetChosen`, `NodeChosen`, `JobStatus`
  (also the answer to a `Probe` of a node kind; `node` is the node that answered, and a report
  from any node but the job's current one is dropped), `ArtifactsChecked`.
- `Observed { sleeve, g, obs }` from drivers and pane screen rules; `Exited { sleeve, g }` once
  the scope is confirmed empty. A stale `g` is dropped.
- `Tick`, `HubRecovered`.

## Intents

- `Execute { effect, attempt, kind }`: perform an effect record's write. `Probe` and
  `ProbeScope`: find out whether it landed (the probe of `StartIncarnation` is `ProbeScope`).
- `SelectTarget`: run the pure `select_target` (outside the core) and answer `TargetChosen`.
  `ScheduleJob`: run node scheduling and answer `NodeChosen`.
- `PollJob`, `ReadLog`, `FetchArtifacts`, `Checkpoint`: reads and git checkpoints, no record.
- `Notify`, `Reply`: for people, Envoy and sleeves.

## Execution rules

- Per sleeve (and per job) the executor is serial. Immediately before the syscall it re-checks
  the record in the current state: same `attempt`, still `Intent`, and for incarnation kinds the
  sleeve's `g` unchanged. Otherwise it drops the intent.
- Report `Dispatched` before writing. An Unsafe kind (prompts, steers, approvals, compaction)
  is never retried by any path: the core sends it once, and the hub must never resend it.
- `Exited` means the scope `pt-<sleeve>-g<g>` has no tasks, not merely that a pipe closed.
- Effects never assumed: until an `Acked`, `Launched` or other evidence arrives, the core treats a
  write as possibly applied (Uncertain) and never as done.

Thresholds are in `limits`. The fuzz targets under `fuzz/` check I1 to I10 after every step, and
`fuzz/seeds/sleeve_core/` holds the scenario scripts (format in `fuzz/src/script.rs`).
