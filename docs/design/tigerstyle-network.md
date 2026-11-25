# TigerStyle networking: investigation and rewrite proposal

Status: accepted 2026-11-24 with every recommendation in section 6; in progress. Phases 0, 1, 2 and 4
are done: the hub's fleet runs on one epoll loop (src/hub/fleet_loop.zig, docs/srcmd/network.md) and
its accept and per-peer threads are gone; its HTTP and WebSocket server runs on a second epoll loop
(src/hub/http_server.zig, docs/srcmd/network.md "HTTP and WebSocket loop"): fixed connection slots,
a bounded parser, worker pools for the runtime calls, streamed assets and downloads, and a lock-free
stream ring read by the loop, so the accept thread, the per-connection threads and the StreamHub
mutex and condition are gone. Phases 3 and 5 are outstanding.
SPEC.md wins where they differ.

Inputs: a read-only source inventory of src/net, src/hub/{fleet,http_server,stream,runtime}.zig,
src/node/{serve,runner,admission}.zig and src/drivers/session.zig against Zig 0.17.1, and a study of
TigerBeetle (references/tigerbeetle, 7d7d78c): docs/TIGER_STYLE.md, src/stdx/io/*, message_bus.zig,
message_pool.zig, message_buffer.zig, static_allocator.zig, testing/cluster/*.

## 1. Where we are

Concurrency is thread-per-thing with shared heap objects:

- Fleet: one accept thread, one detached thread per socket (handshake included), refcounted
  `PeerSession`, per-peer `tx_mu`/`pending_mu`, per-call `PendingCall` mutex and condition.
- HTTP: one accept thread per listener, one detached thread per connection; a WebSocket holds its
  thread for its whole life, blocking in `StreamHub.wait_since`.
- Dial: up to 16 detached dial workers per race, refcounted `RaceState`, never joined.
- Node: heartbeat thread per session, detached lease-acquire worker per request, a thread per job
  plus a lease worker and two pipe pumps per running job.
- Runtime: tick, quota and history threads, and detached dispatch, cancel and update workers that
  block on fleet RPCs.

No path has a thread ceiling. The fleet segfault fixed in v0.8.0-dev.4 came from this model: the
lifetimes depend on reference counts and shutdown waits, and one miscount freed a session under its
own thread. The fix holds, but this kind of bug can come back with every new detached thread.

Runtime allocation is everywhere on the network path: `ConnArg` per accept, handshake arenas,
duplicated IDs and map keys, a growing frame decoder plus a copy of each payload, a whole encoded
frame per send (plus the JSON encoding before it), growing chunk lists per RPC, per-request arenas,
a per-race page-allocated `RaceState`, and per-event JSON in the stream hub.

Bugs found while taking the inventory. These need fixing whatever happens to the rewrite:

1. **The artifact cap is not enforced.** The hub reports `artifact_cap=64 MiB`, but the fleet fetch
   loop (src/hub/fleet.zig, artifact collection) appends every 256 KiB piece to `full_data` and
   checks neither the declared size nor the running total.
2. **RPC chunk accumulation has no limit.** A peer can send any number of chunk frames for one
   pending call.
3. **The node log-range limit is never checked.** It is taken from the request
   (src/node/serve.zig) and used as given.
4. **Fleet and HTTP accept have no connection limit**, so a burst means a burst of threads.
5. The live hub shows the cost: 1 GiB resident after 16 h, a 3.7 GiB peak and 1.9 GiB in swap, on a
   fleet of 8 nodes.

DST does not cover any of this. `run_net_sim` (src/dst/sim_net.zig) checks the Noise and frame codec
(handshake, tampering, fragmentation). It never runs the fleet server or the node client, and it
injects no loss, partitions, resets or crashes.

## 2. Why not `std.Io.Evented`

In the installed Zig 0.17.1, the `Uring` and `Dispatch` vtables wire listen, accept and connect to
stubs that return `NetworkDown` (Uring.zig:774, :5053; Dispatch.zig:452). Some `Uring` network
paths still `@panic("TODO")`. There is no evented Windows backend. `Io.Batch` is a good model for
caller-owned operation storage, but its operation set has no accept or connect. So we write a thin
IO layer of our own and treat `std.Io` as a reference for its API, not a dependency. We revisit
this when std catches up.

## 3. Target design

### One loop per process, one owner

- A single network thread runs a completion loop and owns every connection slot, Noise state,
  timer, RPC slot and WebSocket cursor. No other thread dereferences any of them.
- Everything else talks to the loop through bounded queues of fixed-size commands and completions.
  Each one names its target by a `(slot, generation)` handle. A completion for a reused slot sees a
  different generation and is dropped.
- The work that stays blocking goes to a fixed, joined worker pool with a bounded queue: the
  journal and filesystem, DNS, quota HTTP, child processes and fencing. This is TigerBeetle's rule
  that nothing blocks the loop, adapted for a program that also spawns processes.

### Connection slots instead of refcounted sessions

`connections: [connections_max]Connection`, allocated once. Each slot has a state union:

`free -> accepting | connecting -> handshake -> hello -> established -> closing -> draining -> free`

- Each slot holds `recv_submitted` and `send_submitted`.
- A slot returns to `free` only when no operation is in flight and every worker has handed back
  any buffer it borrowed (TigerBeetle's terminate, join, close in message_bus.zig:1066-1140). A slot
  is never freed, only reused, so a use-after-free cannot happen.
- Peer replacement installs the new generation first, then moves the old one to `closing`. The
  old generation's disconnect is suppressed, and its outstanding RPCs complete exactly once with
  `peer_replaced`.
- One accept is in flight at a time. A spare slot exists so overflow can be closed promptly.

### Comptime IO with a simulated twin

`FleetBusType(comptime IO)` and `NodeClientType(comptime IO)` call only this interface:
`accept, connect, recv, send, close, cancel, timeout, now`, plus an injected PRNG.

Backends, in order of need:

| Backend | Target | Notes |
|---|---|---|
| `io_sim` | VOPR | seeded; models TCP as an ordered byte stream (loss appears as delay or reset, never as reordered bytes); short reads/writes, partitions, resets, crash/restart |
| `io_epoll` | Linux hub and nodes | readiness normalised to completions; io_uring is a later optimisation, not a prerequisite |
| `io_kqueue` | macOS node | same shape as epoll |
| `io_iocp` | Windows nodes | a nodes-only subset: connect, recv, send, timeout (no accept); tested on officewin and the laptop, not just cross-compiled |

Child-process pipes: POSIX pipe fds join the loop. On Windows, anonymous pipes are not IOCP
handles, so each running job keeps one bounded reader thread that hands off through a fixed
queue. This is the one deliberate thread exception, and it is documented in SRC.md.

### Static memory for the network subsystem

- A `StaticAllocator` of our own, about 80 lines from TigerBeetle's design. It has three states:
  init, static and deinit. `alloc`, `resize` and `remap` assert `state == .init`.
  The network subsystem gets its own instance and flips it to static once its pools exist. Any
  allocation after that panics in safe builds.
- A **frame slab pool**: fixed 256 KiB slabs with a free list and a single-threaded reference count
  (only the loop touches it, so it is just a counter). A 1 MiB frame is a chain of slabs. Part of
  the pool is reserved for control frames, so bulk traffic cannot starve heartbeats, cancels or
  closes.
- The pool size is computed at comptime by `frames_max()`, which sums every holder (receive per
  slot, send queue depth per slot, transfer windows, reserve) and asserts the sum stays above the
  deadlock threshold, as `messages_max()` does in TigerBeetle.
- Fixed RPC slot tables replace `PendingCall` and its condition variable. A caller submits a command
  and gets a completion. No thread waits on the RPC.

### Streaming instead of accumulating

- Artifacts, logs and updates move in chunks of at most 256 KiB with a bounded window per
  connection.
- Each chunk is written to a temporary file and hashed incrementally. The file is renamed into
  place only after its size and checksum match.
- The 64 MiB artifact cap is checked against both the declared size and the running byte count.
- A hub update streams the download from a bounded file buffer instead of keeping the whole
  release in memory. Staging, checksum, activation and probation behave as they do today.

### Backpressure, never silent drops

- **Full send queue:** stop producing. Pause the job pipe read or refuse new RPCs on that
  connection with a typed `busy`. Only heartbeats and other idempotent frames may be dropped.
- **Receive side:** do not submit the next recv until the consumer has taken the frame
  (TigerBeetle's suspend and resume).
- **WebSocket behind the 512-entry ring:** send an explicit resnapshot.
- **HTTP at capacity:** answer `503` when a slot is free to answer from, otherwise close; an
  oversized request gets `413`.
- **Fleet at capacity:** close the new socket, because there is no authenticated channel yet to
  report an error on, and count it.

### Time

- One monotonic tick drives everything; TigerBeetle uses 10 ms, and 50 ms is plenty here. A
  `Timeout` struct counts ticks and supports backoff and jitter.
- Today's values stay: a 5 s node heartbeat, a 15 s node receive timeout, a 20 s hub stale
  threshold, a 150 ms dial stagger over up to 16 candidates, and a 0.5 s to 5 s reconnect backoff.
- Handshakes get a 10 s deadline.
- TCP keepalive, `TCP_USER_TIMEOUT` and `nodelay` are set on every fleet socket.

## 4. Bounds and budget

These bounds are sized for a home and office fleet, not a datacenter. They are comptime constants,
asserted with `@sizeOf` checks at startup and stated in SRC.md.

| Resource | Bound | Reserved |
|---|---|---:|
| Fleet slots | 64 (48 established nodes, 16 for handshakes and replacements); 64 KiB recv and 64 KiB send ring plus 16 KiB state each | 9 MiB |
| HTTP/WS slots | 32 across listeners; 32 KiB header limit; 32 KiB recv and 32 KiB send ring plus 8 KiB state | 2.3 MiB |
| Frame slabs | 64 x 256 KiB, 8 of them reserved for control | 16 MiB |
| HTTP large bodies | 2 concurrent; a 4 MiB body plus 8 MiB parse scratch each | 24 MiB |
| RPC slots | 16 per node x 64 x 256 B | 0.25 MiB |
| Stream events | 512 x 8 KiB; anything larger is chunked or triggers a resnapshot | 4 MiB |
| Worker transfer buffers | 16 x 64 KiB | 1 MiB |
| Queues, timers, backend state | fixed allowance | 4 MiB |
| **Total** | | **about 61 MiB** |

The node needs much less: one hub connection, 16 dial attempts and a small slab pool.

This covers the network subsystem only. Core state, history, transcripts and admission stay on
the general allocator until a separate audit gives each of them a limit. Section 6 covers that.

## 5. Migration

The wire protocol (Noise IK, the 1 MiB frame limit, 32 KiB records, odd and even request IDs, and
the 128 KiB update upload pieces) does not change, so mixed-version fleets keep working during a
rollout. Every phase ships a working binary. Each phase is checked by the existing e2e scenarios
plus new VOPR seeds; no unit tests.

| Phase | Work | Retires | Rough size |
|---|---|---|---|
| 0. Bound today's code | Enforce the artifact cap; stream artifacts to disk with incremental hashing; limit chunks per RPC and the node log range; limit fleet and HTTP connections; add high-water metrics to `hub info` | whole-artifact accumulation | 500 to 1,000 lines |
| 1. IO contract and simulator | `io_sim`; fleet and node state machines in VOPR against the real Noise and frame code; invariants for slot and slab conservation, exactly-once RPC results, generation fencing, no stale disconnect, the artifact cap, and reconnect after a partition heals | the isolated `sim_net` codec check, folded into the new runs | 1,000 to 2,000 |
| 2. Hub fleet loop on epoll | Slots, slab pool, RPC slots; runtime dispatch, cancel, update and job polling become continuations instead of blocking on `PendingCall` | fleet accept and per-peer threads, `PeerSession` refcounts, pending condition variables | 2,000 to 4,000 |
| 3. Node loop and portable backends | kqueue and the IOCP subset; dial race, heartbeat and lease acquire move into the loop; a Windows pipe reader per job | `RaceState`, detached dial and lease workers, heartbeat thread, `tx_mu` | 2,500 to 5,000 |
| 4. HTTP and WebSocket | Bounded parsers, streamed assets, loop-owned publication and cursors; keep the Traefik trusted-proxy and upgrade behaviour | HTTP connection threads, `StreamHub` mutex and condition | 2,000 to 4,000 |
| 5. Prove it | Flip the network `StaticAllocator` to static; long VOPR runs with tiny pools in the background gate; move obsolete files to `.archive/` | | 1,000 to 2,000 |

Phase 0 is worth doing now on its own: it fixes real bugs and needs no design decisions. Phases 1
and 2 deliver most of the value, because the class of bug behind the segfault disappears from the
hub. Phases 3 and 4 can wait until they hurt.

## 6. Decisions for the owner

1. **Scope of "no runtime allocation".** Recommended: the network and transfer path is fully
   static (above). The rest of the runtime gets limits (`sleeves_max`, `jobs_max`, `nodes_max`, and
   bounded per-request arenas from a fixed pool) but keeps a general allocator for now. Making the
   whole process static would mean rewriting core state, JSON and the journal. That is a separate,
   larger project.
2. **epoll first, io_uring later.** Recommended. One loop thread on epoll is far more capacity than
   this fleet needs, and epoll avoids io_uring's kernel-version and container-policy problems.
3. **The runtime boundary.** Recommended: `commit` stays on its own thread behind a bounded command
   queue at first. Moving reducer ownership into the loop also needs journal durability split into
   a state machine, and that belongs in its own proposal.
4. **TigerBeetle code.** Reimplement from the design; do not copy. Its IO backends depend on its
   `stdx` and target Zig 0.16. Copying would make them third-party code under DEPENDENCIES.md and
   bring Apache-2.0 header obligations.

## 7. Risks

- **Windows socket and pipe cancellation.** Only real runs on officewin and the laptop validate it;
  cross-compiling proves nothing here.
- **A blocking call that slips into the loop stalls the whole fleet.** Mitigation: in safe builds
  the loop asserts its tick time stays under a bound, and an overrun names the slow operation.
- **WebSocket replay and large snapshots** need chunked encoding or pagination, or they collide with
  the 8 KiB event slot.
- **Self-update restart and probation ordering** has to survive the change of HTTP server.
