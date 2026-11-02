# Concurrency survey (branch `next`, 906adcaa)

Scope: every shared-state primitive in `crates/` (tests and fuzz excluded), what is wrong with
them today, and where actors, bounded queues, sharding or lock-free code would or would not pay
at our scale (about 20 sleeves, 4 nodes, 1-3 clients). Nothing was built or run. Every cost
figure below is an estimate from reading the code, not a measurement.

## 0. Verdict in one paragraph

The hub already has the shape the owner is asking for. One core thread owns `State` and reads
one input channel (SPEC principle 3). Each sleeve and each node gets its own executor thread
with its own inbox. The stream log has a group-commit writer thread. WebSocket fan-out pulls
from a bounded ring and falls back to a snapshot when a client lags, which is already the
right backpressure policy. The fleet poller paces itself and has no queue. Everything else is
a plain `Mutex` around a small cache, held for microseconds, and should stay that way.
Sharding or lock-free structures have no measured reason anywhere: the hottest lock (the bus
ring) is hot because of an O(ring) scan, not because of contention. The real problems are:

- one queue that grows without bound (polls to an unreachable node);
- one busy-poll loop in every live incarnation (the pump, 10 ms);
- a few locks held across file reads or a thread join, one of them reachable from the core
  thread;
- no measurement of the one queue that matters (core latency and depth).

crossbeam is not in the tree (`Cargo.lock` has `libc` as its only registry crate, as
DEPENDENCIES.md lists) and nothing below needs it. std `mpsc` has used the crossbeam
algorithm since Rust 1.67, and the one place that wants `select` (the pump) is fixed by
merging its two inboxes into one.

## 1. Inventory

Notes on the columns. "Held" is the critical section length. "Across" names I/O, a syscall,
a blocking channel op or another lock taken while this one is held. "->" shows nesting, outer
lock first.

### 1.1 pt-hub (the hub process)

| Location | Primitive | Guards | Who takes it | Held | Across / nesting |
|---|---|---|---|---|---|
| serve.rs:212, hub.rs:110 | `mpsc::channel::<Input>` (UNBOUNDED) | core inbox | producers: API threads (`command`, 15 s reply timeout api.rs:25), pumps, sleeve executors, node executors, scheduler, tick, fleet, approvals, **the core itself** (`exec::refuse` exec.rs:157 runs on the core thread) | n/a | consumer `core::run` batches up to 512 inputs, then one fsync (core.rs:21-73) |
| hub.rs:198, 212 | per-command `mpsc::channel` | one reply | API thread waits with `recv_timeout`; core sends | n/a | none |
| hub.rs:111 | `Mutex<Arc<View>>` | published view (RCU-style swap) | core writes once per batch; every reader clones the Arc | ns | none |
| core.rs:92 | `state.clone()` per batch | the view copy | core | O(State); records are never pruned (REVISIT "Ticks are journaled and every batch clones the State") | none |
| bus.rs:62 | `Mutex<Ring>` + `Condvar cond` + `Condvar work` | stream ring (8192 items), queued log records, retained events | every `push`/`push_delta` (pumps, core, API, fleet, quota, hardware, doctor, update), writer thread, every WS subscriber (`since`), every `Wait` (`wait_after`) | short, but `push` runs `to_json().to_string()` under it (bus.rs:217), and `since` scans all 8192 items (bus.rs:352-357) | **leaf lock**: nothing is taken under it. Writer appends and fdatasyncs **outside** it (bus.rs:161) |
| bus.rs:149 | writer thread `pt-stream` | group commit of the stream log | one thread | n/a | `work.wait`; compaction rewrites the log on this thread (bus.rs:176) |
| transcript.rs:231 | `Mutex<HashMap<SleeveId, Arc<Mutex<Store>>>>` | open transcripts | pump (every entry via `Hub::record`/`delta`), core (`wire::sleeve` -> `stats` on every changed sleeve, core.rs:184; notices core.rs:329), API, approvals, brief | a lookup, **or a whole-file `Store::open`** (transcript.rs:242-249 reads and parses the file under the map lock) | file I/O under a global lock |
| transcript.rs:247 | per-sleeve `Mutex<Store>` | append fd, seq, tail of 4096, stats | pump, core (`stats`), API (`after`/`last`/`all`), brief (`all`, brief.rs:98), approvals | append is one `write_all`, no fsync; **`after`/`all` read the whole file under it when the request is older than the tail** (transcript.rs:192-222) | Store -> bus.ring (hub.rs:245-253: the push happens under the store so stream order is transcript order) |
| hub.rs:92 | `Mutex<EnvoyTurn>` | D9 origin and spawn count | sleeve executor (exec.rs:1158), pump (exec.rs:1584), API (`rule::gate` rule.rs:32, api.rs:958) | ns | none. In memory, not journaled (REVISIT, line 200) |
| hub.rs:95 | `Mutex<HubInfo>` | hub info | hardware threads, update, API | ns | info -> bus.ring (hardware.rs:22-28) |
| hub.rs:107 | `Mutex<()>` `update_lock` | one update at a time | update API (`try_lock`, update.rs:127) | whole update (download, verify, swap) | a gate, used only with `try_lock`; correct |
| hub.rs:109 | `Mutex<Option<Sender<Stop>>>` | serve's stop channel | set once in serve.rs:367; update threads clone it | ns | none |
| hub.rs:56 | `AtomicBool` `Fault.fired` | test fault | pump | n/a | fine |
| devices.rs:51 | `Mutex<Inner>` | device list, last-seen writes | every authenticated request (`check`, api.rs:86), WS `still()` once a second, pairing API | ns, **plus a private-file write at most every `SEEN_WRITE_MS` per device** (devices.rs:186-190) | file write under lock, kept on purpose (file order = memory order) |
| jobs.rs:177 | `Mutex<BTreeMap<(JobId,u32),Meta>>` | job log length, start time, artifact list cache | core (`jobs::view`, core.rs:189), node executors, API | ns; the first access does `fs::metadata` / `fs::read(artifacts.json)` under it (jobs.rs:199, 258-264) | small I/O under lock; entries are never removed |
| jobs.rs:414 | `mpsc::channel` -> `pt-sched` | ScheduleJob intents | core sends; the scheduler uses `recv_timeout(2 s)` as its retry timer | n/a | fine |
| jobs.rs:444 | per-node `mpsc::channel` (UNBOUNDED) -> `pt-node-<id>` | node intents | core sends; node executor does blocking calls (dial up to 35 s, member.rs:22; call 30 s, jobs.rs:43; `wait_up` up to 120 s, jobs.rs:42) | n/a | **see P1** |
| exec.rs:102 | per-sleeve `mpsc::channel` (UNBOUNDED) -> `pt-exec-<id>` | sleeve intents | core sends; executor blocks in launch, provision, `wait_ack` (30 s, exec.rs:37,1111), `scope_live` | n/a | head-of-line: see M2 |
| exec.rs:1015 | per-incarnation `mpsc::channel<Order>` + driver `Receiver<Event>` -> `pt-pump-<id>-g<g>` | executor reports and prompt expectations, driver events | executor `submit`/`expect` then `rx.recv()` for the ack (exec.rs:470-497) | n/a | **pump polls both every 10 ms (exec.rs:40, 1274): see P2** |
| exec.rs:354-399 | `OomWatch` thread + 2 channels | memory.events watcher | one per incarnation | n/a | `poll_priority` with a 100 ms timeout, to see the stop channel (10 wakeups/s each) |
| exec.rs:306 | `wait_scope_empty` | scope teardown | pump end, watcher threads | sleep 100 ms, then 1 s | poll loop (no event from systemd without more FFI) |
| exec.rs:181 | transient threads `pt-stop`, `pt-kill`, `pt-watch`, `pt-clean-scratch` | one-shot work | executor | n/a | fine |
| http.rs:270-280 | WS reader thread + `Arc<AtomicBool>` | close detection | one per WS | n/a | the fan-out loop checks the flag every second (`since(cursor, 1 s)`) |
| doctor.rs:31-42 | `Mutex<Option<Arc<Report>>>`+`Condvar`, `Mutex<()> running`, `Mutex<bool> refresh`+`Condvar wake`, `Mutex gated`, `Mutex net`, `Mutex unpacked` | doctor report and run gate | doctor thread, executors (`gate_launch` waits up to FIRST_WAIT), API | `running` for a whole doctor run (seconds); `unpacked` across a one-time unpack | running -> net/report/gated -> bus.ring (via `hub.event`) |
| harness_update.rs:47-51 | `Mutex last`, `Mutex wanted`+`Condvar wake`, `Mutex running` | updater state | updater thread, API | ns | fine |
| fleet.rs:53-57 | `Mutex<Slot>`+`Condvar`, `Mutex streamed` | fleet up/down, last streamed node | fleet start thread, node executors (`wait_up`), scheduler, API, poller callback | ns | (pt-fleet views) -> streamed -> bus.ring |
| quota.rs:19 | `Mutex<Published>` | last quota rows | quota thread, API | ns | last -> bus.ring (quota.rs:141-146) |
| history.rs:85-93 | `Mutex<Result<Index>>`, `Mutex capped`, `Mutex harnesses`, `Mutex passes` | history index | history thread (1 s pass, history.rs:27), brief (`catch_up`), API search/read | `ix` across one source ingest or one search (index I/O); `passes` across a whole pass | passes -> ix -> capped (documented history.rs:262) |
| log.rs:15-16 | `AtomicU8 LEVEL`, `OnceLock<Instant>` | log level, start time | everyone | n/a | fine |
| serve.rs:297 | `pt-tick` thread, `sleep(10 s)` then `submit(Tick)` | the clock | one thread | n/a | fine: a timer, not polling |
| serve.rs:360 | `pt-signals` thread | SIGTERM | one thread | n/a | fine |
| hardware.rs, quota.rs, history.rs, update.rs:1472 | timer threads (`sleep`) | sampling, quota reads, ingest, probation watch | one thread each | n/a | fine: timers |
| update.rs:1298-1318 | **flock** `journal/lock` (`File::try_lock`) | journal between processes (hub vs `hub probation`, restore) | serve, prestart, retry-restore | process lifetime | taken before the release-store lock (update.rs:1345) |

### 1.2 Libraries used by the hub

| Location | Primitive | Guards | Who | Held | Across / nesting |
|---|---|---|---|---|---|
| pt-drivers session.rs:43 | `Mutex<Core>` + `Condvar changed` | protocol state, acks, stdin queue counter | stdout reader (per line, **parses the JSON under it**, session.rs:336-356), stdin writer, executor (`send_user`, `steer`, `wait_ack`, `interrupt`), wait_exit | per line | `events.send` (unbounded) and `stdin.send` (unbounded, but capped at `MAX_QUEUED` bytes with a "harness stopped reading" failure, session.rs:84-93) under it; Core -> child (`release` kills under Core) |
| session.rs:45-46 | `Mutex<Child>`, `Mutex<VecDeque<u8>>` stderr ring | child handle, stderr tail | wait_exit (`try_wait` every 50 ms, session.rs:395-403), kill paths, stderr reader | ns | leaf |
| session.rs:181-231 | 4 threads per session (stdin, stdout, stderr, wait) + `events` channel (UNBOUNDED) + `stdin` channel (bytes-capped) | harness I/O | | | |
| pt-http server.rs:67-75 | `AtomicU8 phase`, `AtomicBool stopping`, `Mutex deadline`, `Mutex<HashMap> conns` | connection slots, cap 256 (server.rs:43) | accept loop, connection threads, shutdown | ns | shutdown drain polls every `poll` (shutdown only) |
| pt-http server.rs:253 | thread per connection | | | | write timeout 30 s (server.rs:49, 306) applies to WS writes too |
| pt-http ws.rs:286 | `Arc<Mutex<WriterState>>` (WsWriter) | whole-frame writes | fan-out thread, reader thread (pong, close) | **across `write_all`**, up to the 30 s write timeout | the only point of this lock is atomic frames; fine for one writer and pongs |
| pt-fleet poller.rs:75-78 | `AtomicBool stop`, `Mutex views`, `Mutex links`, per-link `Mutex<Info>` + `AtomicBool stop` | node views | manage thread, one `fleet-node-<id>` thread per node, scheduler, node executors, API | ns; the poll thread **calls `publish` (hub callback -> streamed -> bus.ring) under `views`** (poller.rs:363-369) | views -> link.info, views -> (hub) streamed -> bus.ring |
| pt-fleet lib.rs:155-158 | `Mutex<()> enroll`, `Mutex<()> nodes` | serialize headscale writes | manage (every list), API rename/remove/enroll | **across headscale CLI calls** (subprocess, `CLI_TIMEOUT`) **and, in `refresh`, across `t.join()` of a gone node's poll thread** (poller.rs:285) | nodes -> links -> link.info; nodes -> views |
| pt-fleet poller.rs:177 | `nap`: `sleep(100 ms)` loop on AtomicBools | stop latency | manage and each node thread | n/a | busy-ish: 10 wakeups/s per thread |
| pt-fleet child.rs:55 | `Mutex<Flags>` + `Condvar` | headscale/member supervisor | supervisor thread (`try_wait` + health every 250 ms), callers of `wait_healthy`, `pause`, `resume` | ns | fine (std has no `Child::wait_timeout`) |
| pt-fleet headscale.rs:335-350, member.rs:95 | reader threads + `try_wait` every 10 ms | one CLI call | | | fine: short-lived |
| pt-quota cache.rs:76-115 | `thread::scope` + one detached reader thread per due slot + `pending` receiver | a stuck read stays pending; it is never re-spawned while pending | quota thread | | bounded; good |
| pt-update store.rs:418-428 | **flock** `releases/.lock` (blocking `File::lock`) | release store between processes and threads (flock is per open file description, so threads exclude each other too) | update threads, prestart | one state change | taken after the journal lock |
| pt-update store.rs:126 | `AtomicU64 SEQ` | temp names | | | fine |
| pt-history index.rs:267 | **flock** index lock (`try_lock`) | index between the hub and `probe` | | process lifetime | fine |
| pt-embed lib.rs:74 | `static Mutex<()>` | one unpack at a time | doctor, fleet | across an unpack | fine |
| pt-sys unix.rs:167-172 | `AtomicI32` x3, `static Mutex<Option<TermFlag>>` | signal pipe | signal handler, `wait_term` | | fine: async-signal-safe atomics |
| pt-doctor lib.rs:327, 569; pt-node inventory.rs:11, 58 | `thread::scope` | parallel probes | | | fork-join; fine |

### 1.3 Node agent (protectorate-node + pt-node)

| Location | Primitive | Guards | Who | Held | Across / nesting |
|---|---|---|---|---|---|
| pt-node jobs/mod.rs:81 | `Mutex<Table>` (journal + jobs) | node job table | connection threads (submit/status/cancel/ack), waiter threads, presence watcher | **across journal fsyncs and the job spawn** (mod.rs:220, 258, 287, 319) | intended: admit, journal, spawn and receipt are one atomic step (SPEC 12a). Keep |
| pt-node jobs/mod.rs:104-108 | `AtomicBool` x4, `AtomicU64` in `Live` | cancel, kill, lower, log size | request side, presence watcher, waiter | n/a | fine: flags and counters |
| pt-node jobs/mod.rs:373-409 | waiter `try_wait` every 50 ms | one job | | | poll loop (MAYBE M1) |
| pt-node jobs/mod.rs:80, serve.rs:46 | `Arc<Mutex<Detector>>`, `Mutex<Sampler>` | presence, CPU sampler | Presence and Sample requests, watcher | across detection I/O | fine at 1 hub |
| pt-node jobs/mod.rs:794 | **flock** `agent.lock` (`try_lock`) | one agent per dir | | process lifetime | fine |
| protectorate-node serve.rs:48-50 | `AtomicUsize connections`, `AtomicBool dropped_submit` | idle exit, test knob | | | fine |
| protectorate-node serve.rs:275-296 | thread per connection, **no cap** | | | | M6 |
| protectorate-node serve.rs:226 | `idle_exit` sleep(200 ms) loop | | | | fine: a timer |
| protectorate-node update.rs:37-38 | `AtomicBool trial`, `Mutex<bool> updating` | one update | | | fine |
| pt-node sys/win32_service.rs:65-70 | statics: `OnceLock`, 3 `Mutex<Option<..>>`, 2 atomics, an mpsc `Sender` | SCM callback to the service body | SCM thread, body | ns | fine: FFI callback plumbing |

### 1.4 CLI (protectorate)

| Location | Primitive | Note |
|---|---|---|
| mcp.rs:33-35, 300-306 | `Mutex` agent/initialized/stdout; one thread per `tools/call`, no cap | M6 |
| cli.rs:835, 915 | `job wait` polls `GetJob` every 500 ms | a client; SKIP (S9) |

## 2. Problems found

### Real bugs

**P1. A node executor's inbox grows without bound while its node is unreachable.**
- What happens now: on every Tick (10 s, serve.rs:42) the core emits one `PollJob` for each
  job on that node that is SubmitUncertain, Running, Cancelling or **Lost** (pt-core
  job.rs:564-567). A Lost job keeps polling for 24 h (`LOST_ABANDON`, job.rs:15).
- Each poll to an offline node costs one dial of up to 35 s (member.rs:22), and when the call
  fails as disconnected, a second dial (`call` reconnects, jobs.rs:751-790).
- Polls arrive every 10 s and each takes about 35-70 s, so the unbounded channel (jobs.rs:444)
  grows by about 0.7 polls per job every tick: around 6000 per job over a day.
- The cost is not memory. A real effect (an owner's `NodeCancel`) queues behind all of them,
  and when the node returns the executor works through thousands of stale polls before it.
- The failures themselves are harmless: a failed poll only logs (jobs.rs:1086).

**P2. Every live incarnation's pump busy-polls at 10 ms.**
- What happens now: `pump` (exec.rs:1262-1307) waits on the driver's `Receiver<Event>` with
  `recv_timeout(ORDER_POLL = 10 ms)` only so that it can `try_recv` the executor's `Order`
  channel.
- That is 100 wakeups/s per live incarnation, about 2000/s for 20 sleeves, even when every
  harness is idle. It is about three quarters of the hub's idle wakeups. The rest are
  wait_exit at 50 ms, the OOM watch at 100 ms and poller naps at 100 ms.
- A second, minor cost: the order between a report and the driver's events depends on the
  timing of that window. The pump drains events that arrived up to 10 ms after the report was
  sent.

**P3. Whole-file transcript reads under locks the core thread needs.**
- `Store::after` (outside the 4096-entry tail) and `Store::all` (any transcript over 4096
  entries) read and parse the whole JSONL file while holding the per-sleeve store lock
  (transcript.rs:192-222).
- Callers: `GetTranscript` with an old `from` (api.rs:341-353), the agent and Envoy transcript
  views (api.rs:1770, 1807, 1844) and the brief (brief.rs:98).
- `Transcripts::get` opens a store, which is again a whole-file read, under the global map
  lock (transcript.rs:242-249).
- The core thread takes both locks on every batch that changes a sleeve:
  `announce_changes` -> `wire::sleeve` -> `transcripts.stats` (core.rs:184, wire.rs:147).
- So a client paging an old transcript stalls the one writer for as long as the read takes.
  At "a few thousand entries" that is tens of ms; it grows with the never-rotated file (REVISIT
  "Transcripts never rotate").
- At startup the core's first batch announces every sleeve and opens every transcript
  serially.

**P4. `Poller::refresh` joins a gone node's thread while holding `nodes_lock`.**
- Where: poller.rs:279-290, under the guard taken at poller.rs:207.
- The joined thread may be mid-round: one or two dials of 35 s each.
- During that time owner `node rename` / `node remove` (poller.rs:126, 143) block. This is not a
  deadlock (poll threads never take `nodes_lock`; checked).

**P5. Minor race: `Transcripts::close` vs a still-running pump.**
- The core closes the transcript of a removed sleeve (core.rs:123) while its killed
  incarnation's pump may still be recording output.
- Here is the interleaving. The pump holds the old `Arc<Mutex<Store>>` for one append. Then an
  API read calls `get`, which opens a second `Store` and computes `next_seq` from the file.
  A later append through the new store reuses a seq the old one just wrote.
- The effect is limited to a duplicate seq in a removed sleeve's transcript. It is fixed for
  free by P3's open-outside-lock if `close` is dropped, or by closing only when the pump has
  ended (see DO-3).

### Checked and fine

- **Lock order.** There are no inversions. `bus.ring` is a strict leaf (nothing is locked
  inside a `push` closure). The other chains are each one-directional:
  - Store -> ring
  - info -> ring
  - quotas -> ring
  - fleet views -> streamed -> ring
  - doctor running -> net/report/gated -> ring
  - history passes -> ix -> capped
  - pt-fleet nodes -> links/views -> link.info
  - pt-drivers Core -> Child
  - journal flock -> release-store flock
- **Deadlocks.** None found. Bounding any of the existing queues would create one (S1, S2).
  The core never waits on an executor. Executors wait on the core only through `command`,
  which has a timeout, and on their own pump, which waits on nothing that waits for them.
- **Poisoned locks.** Every lock goes through an `into_inner` helper (transcript.rs:225,
  session.rs:62, poller.rs:172, lib.rs:344, child.rs:60, main.rs:194, pt-http server.rs:92,
  ws.rs). `[profile.release] panic = "abort"` (Cargo.toml:24) means a shipped binary dies on
  any panic, so poisoning never happens there. Debug builds, which the e2e tests run, unwind
  instead. A panicked pump or executor thread then leaves the hub running with a dead actor
  (M5).
- **WebSocket backpressure** (http.rs:263-334) is already correct:
  - producers never block;
  - each subscriber has its own thread, pulling from the 8192-item ring;
  - a subscriber that falls more than one ring behind gets `Gap` and then a fresh snapshot
    (coalesce to latest);
  - a stalled socket hits the 30 s write timeout and is disconnected;
  - authorization is re-checked every second;
  - the reader thread is unblocked by `writer.shutdown()` when the handler returns
    (pt-http server.rs:338-340).
- **Slow nodes.** These are isolated: one executor per node, one poller thread per node, so
  one stuck node never delays another. P1 is the only queue on this path.

## 3. Target design

### Owned state and actors (what exists, what changes)

| State | Owner thread | Inbox | Bound and policy | Change |
|---|---|---|---|---|
| `pt_core::State` | `pt-core` | `Input` (std unbounded) | **No bound** (S1); measure depth and batch time instead (DO-5) | gauge only |
| sleeve executor | `pt-exec-<id>` | `Intent` | unbounded; the producer is the core, and per-sleeve intents are bounded by the state machine (one effect per kind and generation, probes capped by `UNKNOWABLE_NOTIFY`) | none |
| node executor | `pt-node-<id>` | `Intent`, plus a local coalescing deque | observations are coalesced to the latest per (kind, job, attempt); effects stay FIFO | DO-1 |
| pump | `pt-pump-<id>-g<g>` | one `PumpIn` channel | unbounded: the producer rate is the harness's stdout and the executor, which waits for each ack | DO-4 |
| stream ring and log | `pt-stream` writer, plus subscribers pulling | `Ring.queued` | ring of 8192; a lagging subscriber gets Gap then a snapshot; a stalled one is disconnected by the 30 s write timeout | DO-2 (cost fix only) |
| fleet poll per node | `fleet-node-<id>` | none (self-paced) | no queue | DO-6 (stop latency, join) |

### Stays a plain Mutex (short, no I/O, simpler than an actor)

- `view` (an Arc swap);
- `envoy_turn`, `info`, `stop`;
- `jobs.meta`, `quotas.last`, `fleet.slot`/`streamed`;
- doctor and harness-update fields;
- `devices` (the file write under it is deliberate);
- pt-fleet `views`/`links`/`Info`;
- pt-http `conns`;
- pt-drivers `Child`/stderr;
- pt-node `Table` (atomic journal+spawn on purpose).

An actor for any of these would add a thread, an enum and a reply channel per call, for a lock
held for nanoseconds. The transcript store also stays a Mutex. It needs only its full-file
reads moved out (DO-3).

### Sharding and lock-free

There is no measured reason for either. The candidate locks are `bus.ring` (one writer thread,
about 20 pumps and 1-3 subscribers), `transcripts.map` (a hash lookup), and `view` (an Arc
clone). At this thread count std `Mutex` (futex, uncontended fast path) beats any lock-free
ring. A shard needs a cross-shard total order for stream seqs, which the bus must keep. The
only real cost on the ring is the O(8192) scan, which DO-2 removes. Atomics stay where they
are (flags, counters, signal pipe).

## 4. DO items, ranked by value/cost

### DO-1. Coalesce a node executor's inbox (fixes P1). About 40 lines in jobs.rs.

- Struct `NodeExec` (jobs.rs:663) gains `inbox: VecDeque<Intent>`. The thread is
  `pt-node-<id>`.
- The message enum stays `pt_core::Intent`.
- `run` changes like this:
  - It blocks (`recv` or `recv_timeout` to the next follow-up, as now) only while the inbox is
    empty.
  - Before handling each intent it drains `rx.try_recv()` into `enqueue`. Polls that arrive
    during a 35 s dial therefore fold into the queue.
- `enqueue` folds observation intents into one per key:
  - `PollJob` by (job, attempt);
  - `FetchArtifacts` by (job, attempt);
  - `ReadLog` by (job, attempt), where `upto` becomes `None` if either is `None`, else the max.
- `Execute` and `Probe` are never coalesced and stay FIFO. They carry effect records (SPEC
  principle 5). They are already bounded by `NODE_RESENDS`, and `exec::fresh` drops stale
  ones.
- Bound: at most 3 observations per live (job, attempt) on the node, plus outstanding effects.
  Policy: coalesce to latest.
- Optional, same commit: for observation intents only, skip the dial while
  `up.poller.node(id)` says `online == false` and no client is connected. The next Tick
  retries. Effects always dial.
- Test: extend the existing unreachable-node fleet e2e (crates/protectorate/tests/fleet.rs).
  Stop a node with a Running job, let ticks pass, bring it back, and check that a cancel lands
  within one round.

### DO-2. `Bus::since` takes an O(1) slice instead of scanning the ring. About 10 lines in bus.rs:346-359.

- Positions in `Ring.items` are contiguous (`add` assigns `next_pos` and pops the front;
  `open` numbers from 1).
- So the start index is `after + 1 - front.pos`, the end is `durable + 1 - front.pos`, and the
  result is `items.range(start..end).cloned()`, with the existing Gap test kept.
- Today every subscriber wake, about once per writer batch plus once a second per WS, walks
  all 8192 items under the lock that every pump's `record` takes.
- `resume` (bus.rs:261-266) uses the same arithmetic for its `find`.
- No new test: the WS resume and gap e2e already exercise it (crates/pt-http/tests,
  crates/protectorate/tests/hub.rs).

### DO-3. No whole-file transcript reads under a lock (fixes P3, and P5 with it). About 40 lines in transcript.rs, plus 5 call sites.

- `Transcripts::get`:
  - look up under the map lock;
  - on a miss, drop the lock, run `Store::open`, re-lock, and keep
    `map.entry(id).or_insert(s)`;
  - a racing loser is dropped. Its only possible write is a `\n` repair, and `read_all` skips
    an empty line.
- New `Transcripts::after(id, after, limit)` and `Transcripts::all(id)`:
  - under the store lock, answer from the tail when possible;
  - otherwise copy `path` and `next_seq`, release the lock, `read_all(path)`, and keep
    `seq < next_seq`.
  - Every line below that seq was fully written under the lock, so a torn tail being
    appended concurrently is above the cut.
- Remove `Store::after`, `Store::last` and `Store::all`. Migrate api.rs:341-353, api.rs:1770,
  1807, 1844 and brief.rs:98.
- P5: the core closes a removed sleeve's transcript only after its executor reports the pump
  ended. The simpler option is to drop `close` entirely, since an open fd per removed sleeve is
  negligible at about 20 sleeves. The owner picks.
- No new test: the transcript paging e2e (crates/protectorate/tests/hub.rs) covers both paths.

### DO-4. One inbox per pump, blocking (fixes P2). About 80 lines across pt-drivers and exec.rs.

- `pt_drivers::spawn<E: From<Event> + Send + 'static>(spec, events: Sender<E>) -> Result<Session, DriverError>`.
  - `Core.events` becomes `Box<dyn Fn(Event) + Send>`, wrapping `move |e| { let _ = tx.send(E::from(e)); }`.
  - The pt-drivers test helper (tests/common/mod.rs:152) passes an `mpsc::channel::<Event>()`;
    the reflexive `From` covers it.
- In exec.rs:
  - `enum PumpIn { Driver(pt_drivers::Event), Order(Ordered, Sender<()>) }` with
    `impl From<pt_drivers::Event> for PumpIn`;
  - `Live.order: Sender<PumpIn>`;
  - `pump(hub, run, inbox: Receiver<PumpIn>)`.
- The pump's wait:
  - when `held` is Some, wait with `recv_timeout(held.at + NATIVE_WAIT - now)`;
  - otherwise a plain `recv()`.
  - Remove `ORDER_POLL` and the nested `try_recv` drain.
- The loop ends on the driver's `Exited` event (wait_exit always emits it). It no longer ends
  on Disconnected, because the executor's Sender keeps the channel open. After the loop, drain
  the remaining `Order`s with `try_recv` and ack them as now, then drop the receiver.
- Ordering becomes exact FIFO of the sends. An `Expect` is enqueued before `send_user`, so it
  precedes that write's `UserMessage`, which is the invariant the comment at exec.rs:1258-1261
  wants.
- Bound: unbounded, by argument.
  - Producers are the harness's stdout rate and an executor that blocks on every ack.
  - A bounded `sync_channel` would block the stdout reader while it holds the session `Core`
    lock (session.rs:125), which also stalls `send_user` and `wait_ack`. Holding a lock across
    a blocking send is the one thing to avoid.
- Effect: about 2000 idle wakeups/s at 20 sleeves drop to 0.
- No new test: every driver e2e and the hub prompt and steer e2e run through the pump.

### DO-5. Measure the core. About 20 lines in hub.rs and core.rs.

- `Hub.input_depth: AtomicU64`: `fetch_add` in `submit`, `command` and `snapshot_journal`;
  the core `fetch_sub`s the batch size after draining.
- The core times each batch from `recv` to the last intent routed.
- `warning!`, rate-limited to once a minute, when a batch takes over 250 ms or the depth
  exceeds 10 000.
- No API change. This is the measured reason the REVISIT "every batch clones the State" item
  is waiting for, and the only honest answer to "should the core inbox be bounded".
- No test (log only).

### DO-6. pt-fleet poller: block instead of napping, and join outside `nodes_lock` (fixes P4). About 30 lines in poller.rs.

- `Shared` gains `gate: Mutex<()>` and `wake: Condvar`.
- `nap`:
  - `let g = lock(&gate)`;
  - loop: check the flags, then `wake.wait_timeout(g, left)`.
- Each place that sets a stop flag (`Poller::drop`, `remove`, `refresh`, the end of
  `manage`):
  - stores the flag;
  - then takes and drops `gate` (so a wakeup cannot be lost);
  - then calls `notify_all`.
- `refresh` returns the gone links. `manage` drops the `nodes_lock` guard, then joins each one,
  removes its view and publishes `Removed`, in the same order as now.
- Effect: 10 wakeups/s per poll thread drop to 0. Owner rename and remove no longer wait up to
  about 70 s behind a dying poll round.
- No new test: the node remove e2e (crates/protectorate/tests/nodes.rs) covers the path.

## 5. MAYBE

- **M1. One `pt_sys::wait_child(&mut Child, Option<Instant>)`** (pidfd + `poll` on Linux,
  `WaitForSingleObject` on Windows, a polling fallback elsewhere).
  - It would replace about 12 `try_wait` + sleep loops:
    - session.rs:395
    - child.rs:271, 328
    - headscale.rs:347
    - pt-node exec.rs:35, jobs/mod.rs:373
    - supervise.rs:264, 330
    - stdio.rs:34
    - store.rs:371
    - doctor probe.rs:83
    - windows_job.rs:102
  - pidfd code already exists in pt-node sys/linux.rs:35-60 and would move to pt-sys.
  - It simplifies the code and removes about 20 wakeups/s per sleeve. Every loop is correct
    today, so do it only if one of these files is being touched anyway.
- **M2. Move the 30 s ack timeout from the executor into the pump.**
  - Today `SleeveExec::prompt` blocks in `wait_ack` (exec.rs:1111), so an `Interrupt` or
    `StopIncarnation` for that sleeve waits up to 30 s behind it.
  - After DO-4 the pump owns a deadline loop. An `Expect` could carry
    `deadline = now + ACK_TIMEOUT`, and the pump would report `Lost` itself, which frees the
    executor at once.
  - It changes who reports the lost-ack outcome, so it needs the prompt-loss e2e re-checked.
    Worth it only if stop latency on a hung harness is seen in practice.
- **M3. Cheaper `View`** (`Arc<Sleeve>` values with `Arc::make_mut` in pt-core, or pruning).
  - Already in REVISIT. Decide from DO-5's numbers, not before.
- **M4. Waiters wake on kept messages only.** A second condvar in `Bus`, notified only when a
  batch had a kept message, so `Wait` callers stop waking on every streaming delta. This is
  cheap, but the waiters' work per wake is a state lookup. Only if DO-5 or profiling shows it.
- **M5. `panic = "abort"` in `[profile.dev]` too.** Then the e2e tests (debug binaries) fail
  the same way release does, and the `into_inner` poison handling is the same in both. The
  cost is that a panicking test hub gives no unwind trace in-process (the panic message still
  prints).
- **M6. Cap the node agent's connection threads** (protectorate-node serve.rs:275-296, none
  today; pt-http's 256 is the model) and the MCP bridge's per-`tools/call` threads
  (mcp.rs:300-306). Both sit behind trusted gates (member plus `HUB_TAG`, and the envoy-only
  stdio), so this is hygiene, not a hole.
- **M7. Parse a harness stdout line before taking the session `Core` lock** (session.rs:336-356).
  Only the stdout thread produces stdout messages, so order is kept. It shortens how long
  `send_user` and `wait_ack` wait behind a multi-MB tool result.

## 6. SKIP

- **S1. Bounding the core `Input` channel.**
  - The core thread itself submits to it: `exec::refuse` (exec.rs:157-176) runs on the core
    thread for any intent to a removed sleeve (exec.rs:100) and on a failed executor spawn
    (exec.rs:116-118). A full bounded queue would deadlock the core on itself.
  - There is also a cycle: core -> (bounded) executor -> pump -> `hub.submit` -> core.
  - Producers are internal and rate-limited, and a fsync stall stalls them all equally.
  - Measure instead (DO-5). Tick coalescing is not needed: at one per 10 s it cannot pile up.
- **S2. Bounding sleeve-executor channels or the driver event channel.**
  - Same cycle as S1, and the stdout lock issue covered under DO-4.
  - The natural bound (harness output rate) is already end-to-end.
- **S3. RwLock or an ArcSwap-like structure for `view`.** A Mutex around an Arc clone is
  already RCU-shaped. There is one writer, and readers hold the lock for one clone.
- **S4. Sharding `transcripts.map`, the bus ring, or `jobs.meta`.** Contention is not the
  problem; the O(n) scan (DO-2) and the I/O under the lock (DO-3) are.
- **S5. A lock-free ring or crossbeam.** Not in the tree, no measured need, and std `mpsc` is
  crossbeam's algorithm. `select` stops being needed after DO-4.
- **S6. Serializing outside the ring lock** (bus.rs:217). Seq order must equal ring order, so
  the fix needs a second "push" mutex. Entries are small at our scale; revisit only if a
  profile shows `to_json` under `ring`.
- **S7. Moving `envoy_turn` into the core.** This is a D9 correctness item already logged in
  REVISIT (hub restart resets the count). No cross-thread race was found: the pump clears the
  origin before the core can see Idle (exec.rs:1583-1591), so the next prompt's note is always
  later.
- **S8. An async runtime, or a thread pool for HTTP.** About 150 threads at 20 sleeves is fine,
  and pt-http already caps connections at 256 per listener.
- **S9. CLI `job wait` polling at 500 ms** (cli.rs:835). It is a client, and the `Wait` API or
  the event socket could replace it later without hub changes.
- **S10. Fleet `nodes_lock` held across headscale CLI calls.** This serializes headscale
  writes on purpose. P4 is the only part to fix.

## 7. Implementation order (each commit keeps `make check` green)

1. **DO-2** (bus.rs only). Trivial and isolated; run the WS resume/gap e2e.
2. **DO-3** (transcript.rs + api.rs + brief.rs, one commit, all callers migrated; decide P5:
   close-after-pump or drop `close`).
3. **DO-1** (jobs.rs; plus the fleet e2e for an offline node with a running job).
4. **DO-4** (pt-drivers `spawn` signature + its test helper + exec.rs pump, one commit, since
   the signature change has two callers).
5. **DO-5** (hub.rs + core.rs gauge). Run it on the live hub for a few days before deciding M3.
6. **DO-6** (poller.rs).

Batch the full `make check` at the end of 1-6, per the batch-check rule; run targeted tests per
commit.
