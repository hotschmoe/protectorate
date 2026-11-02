# Architecture cleanup, 2026-11-03

The owner asked for five pieces of architectural cleanup and one design question (verbatim, abridged):

1. "do we do linear imports? ie no circular imports between files and crates ... furthest files out have no imports"
2. A review of the "macrorepo with nanoservices" post.
3. "find all of the implicit state machines in my app -> make them explicit state machine (within reason)"
4. "make the code lockfree/shard the locks and for shared state use the actor/message passing pattern w/ backpressure"
5. Data-oriented design: "replace those hash tables with a flat array ... pointers an index into the array ... pack the most commonly used data into upper bits of the index", "caching is cope for a slow design ... delete all existing caches", "structure of arrays", "stack allocated [lists] with an inline capacity", "use a bitset instead", "review arena and stack allocated cleanup".
6. "we can actually move to SSH or similar and move to full rust IF we have addressing fix ... design this and review later BEFORE actually replacing headscale".

This page holds the answers and the decisions. The evidence is in four reports beside it:

| Report | Author | What it covers |
|---|---|---|
| [survey-state-machines.md](survey-state-machines.md) | Opus | 12 ranked items, 10 of them bugs with interleavings |
| [survey-concurrency.md](survey-concurrency.md) | Opus | every lock, channel and thread; 6 ranked items |
| [survey-data-layout.md](survey-data-layout.md) | GPT-6.1 Terra | every map, cache, small list and hot path; 7 ranked items |
| [network-rust-only.md](network-rust-only.md) | GPT-6.1 Terra | replacing headscale and member, for review; nothing is replaced |

Each item below says what lands now and what waits. "Lands" means it is on a `pt/*` branch that merges into `next` after the gate.

## 1. Linear imports (levels)

**Industry names.**
- John Lakos calls this *levelization* (Large-Scale C++ Software Design, 1996): every component has a level, and a component uses only components on lower levels.
- Robert C. Martin calls the rule the *Acyclic Dependencies Principle*.
- SICP calls the result *stratified design*.

The owner's "furthest files out have no imports" is level 0.

**What Rust guards.**
- Cargo refuses a cycle between crates, so the 23 crates were already a DAG.
- Nothing guards modules inside a crate. Two files can `use` each other freely, and they did:
  - 15 of pt-hub's 28 modules formed one cycle;
  - 6 other crates had smaller cycles;
  - about 190 places used an item that their own crate root (lib.rs) defines, which is an upward import.

**The rule now enforced** (`crates/pt-levels`, `make check-levels`, part of `make check`):
- Under each parent (a crate root, or a module with children), sibling modules use each other without a cycle.
- No module uses an item its crate root or parent module defines. The root sits on top: it declares, re-exports and calls; it is not a place to keep types that children import.
- `pt-levels --print crates` lists every crate's modules by level.

The checker is about 600 lines of std-only Rust. It reads `use`, `crate::`/`super::`/`self::` paths, `pub use` re-exports and exported macros.

What it cannot see:
- method calls through a split `impl` (`impl Core` across pt-core's modules);
- macro expansion beyond `$crate`.

It enforces file imports, which is what the owner asked for, not every semantic dependency.

**Status:** two branches fix every report.
- `pt/levels`: all crates except pt-hub. Shared types move from lib.rs into modules named for the concept, and lib.rs re-exports them, so no public path changes.
- `pt/levels-hub`: breaks the pt-hub cycle.
  - The cause is `Hub` as a context object: it holds every subsystem's state, and the subsystems take `&Hub`.
  - The fix layers it: state below `hub` with narrow parameters, orchestration above it, and api/core/serve on top.

## 2. Macrorepo with nanoservices

The post bundles three claims:

| Claim | Verdict for Protectorate |
|---|---|
| One repository for everything | **Yes, we have it.** Agents see every caller, one commit changes an API and all its users, one gate checks it all. With agents this matters more, not less. |
| Native, GC-free languages make services small and cheap | **Yes, we have it.** The post's low RSS comes from the runtime (no JVM heap or GC headroom), not from how finely the system is split. One Rust process gets the same per-request efficiency. |
| Split the system into many tiny deployable services | **No, not for a single-owner hub.** A network boundary between modules adds serialization, partial failure, retries, version skew, tracing and deployment order. A function call has none of these. 2-5k rps is a load one Rust process handles on one machine, so it is not evidence for the split. |

**The rule we already follow.**
- Split a **process** only when it needs a different failure domain, privilege, lifecycle, language or machine.
- Split **code** into crates and modules for everything else.

Protectorate's processes each exist for a reason:
- headscale and member are sandboxed children because headscale's `Serve` takes over signals and calls `log.Fatal`, and because they must hold none of the hub's secrets (SPEC 3.2);
- every sleeve runs in its own sandbox scope;
- the node agent runs on another machine.

Inside a process, crates are compile-time "nanoservices": explicit interfaces that cost nothing at run time, and now levelized inside too (section 1). The part of the post worth keeping is its premise: small units with explicit boundaries suit agents. We get that from crates and levels without paying for a network between them.

**Our numbers**, from the live hub on 2026-11-03 (`/proc/<pid>/smaps`):

| Process | RSS |
|---|---|
| hub (Rust, 46 threads) | **473 MB** (VmHWM 606 MB) |
| headscale (Go) | 59 MB |
| member (Go) | 43 MB |

The hub is far from the post's numbers, and splitting processes would not fix that.

- About **390 MB of the hub is glibc malloc arena heaps**: about 75 RW heap segments of 5-15 MB each, the pattern of per-thread arenas that keep freed memory. It is not live data.
- Its sources are many threads that each allocate, plus large transient buffers:
  - a whole transcript file read into a `Vec` on open and on old-page reads;
  - a deep clone of the core `State` on every journaled batch, including the ten-second tick.
- Section 5 has the fixes. Dropping the two Go children (section 6) saves about 100 MB more.

io_uring and zero-copy are not relevant at our load. A turn costs seconds of model latency, and the hub spends next to nothing on syscalls.

## 3. Implicit state machines

The core's machines are explicit already: sleeve life, activity, guard, approvals and jobs. Around them, the survey found phase state in loose bools, Options, string prefixes, marker files and in-memory maps that the journal does not see. Ten of the twelve items are bugs:

| Id | Fix | Bug it removes | Cost |
|---|---|---|---|
| D2 | `ParkReason::Ineligible { harness }` journaled; doctor's in-memory `gated` map stops deciding | Envoy parked by doctor **never restarts after any hub restart**, including every update | S |
| D4 | One `Sleeve::settled_idle()` in pt-core used by both idle checks | `sleeve wait --until idle` returns just before an auto-continue starts | XS |
| D3 | Our own `hub-enrolled` marker after tagging, not tsnet's state file | An interrupted first enrollment leaves the fleet Down for good (confirm by killing mid-enroll) | S |
| D8 | Delete the headscale node if approval fails after `register` | A half-approved node no API can remove | XS |
| D7 | pt-fleet child supervisor `Want`/`Phase` enums | `pause()` can return while headscale is spawning, so the pre-upgrade snapshot can be torn | S |
| D1 | Envoy's D9 turn (origin, spawn count) moves into the core, journaled; typed message kind replaces the `continue:`/`ga:`/`resleeve:` prefixes (16 sites) | A hub restart resets the D9 spawn count, and a resleeve brief loses the origin (the known REVISIT gap) | M |
| D5 | Node job: one `finish()` for terminal transitions, cancel as data | Cancel records leak and are re-probed on every start; outcome depends on lease timing | M |
| D6 | Core learns harness-started turns (`Obs::TurnBegan`); a `Busy` race requeues | A message is rejected by a millisecond race although nothing was written | S-M |
| D9 | Claude driver tracks turns by echo order | A self-started Claude turn can settle our prompt's turn (confirm with a trace first) | S |
| D10 | Probation phase: one pure `phase()` over the on-disk record | Nine hand-copied predicates disagree; one path can start on half-restored data | M |
| D11, D12 | `Life::Resleeving(Resleeve)` and `Proc::{Starting, Up}(Inc)` own their data; dead states go | Tidy only: removes representable illegal states | M-L |

The survey proposed no generic state-machine type, trait or macro. The house pattern stays the same: an enum that owns its per-state data, plus one function that owns the transitions.

**Lands now:** D2, D4, D3, D8, D7, then D1 (in that order).
**Waits:** D5, D6, D9, D10, D11 and D12 come after review of the first batch.

## 4. Locks, sharding and actors

**Verdict: the hub already has the asked-for shape.**
- One core thread owns `State` and reads one input channel.
- Every sleeve and node has an executor thread with its own inbox.
- The stream log has a group-commit writer.
- WebSocket fan-out pulls from a bounded ring. A lagging client gets a fresh snapshot, and a stalled one is cut off by a 30 s write timeout. That is the right backpressure policy.
- The rest are plain Mutexes held for microseconds with no I/O. An actor for each would add a thread, an enum and a reply channel per call, for no gain.
- Sharding and lock-free structures have no measured reason. The hottest lock (the bus ring) is hot because of an O(8192) scan, not contention.

Two queues stay unbounded on purpose, and bounding either would deadlock:
- **The core input:** the core thread submits to itself (`exec::refuse`), and core -> executor -> pump -> core is a cycle.
- **A pump's driver events:** a full queue would block the stdout reader while it holds the session lock.

Both are bounded end to end by their producers. We measure them instead (DO-5).

Real problems found, and the fixes that land:

| Id | Fix | Problem |
|---|---|---|
| DO-1 | Node executor coalesces repeated `PollJob`/`ReadLog`/`FetchArtifacts` per job to the latest; effects stay FIFO | While a node is unreachable, a poll every 10 s per job piles up behind 35-70 s dials: an **unbounded queue**, and an owner's cancel waits behind it |
| DO-2 | `Bus::since` takes an O(1) slice of the ring | Every subscriber wake scans up to 8192 items |
| DO-3 | Transcript files are read outside the store and map locks | Whole-file reads under locks the core thread takes every batch |
| DO-4 | Each pump blocks on one inbox (`PumpIn`) instead of polling | Every live pump wakes every 10 ms: about 2000 wakeups/s at 20 sleeves, idle |
| DO-5 | Core queue depth and batch time, logged on a threshold | The one queue that matters is not measured |
| DO-6 | Fleet poller waits on a condvar and joins outside `nodes_lock` | Owner rename/remove can block up to about 70 s behind a dying poll |

No lock-order inversion or deadlock was found. Poisoned locks cannot happen in release (`panic = "abort"`).

## 5. Data-oriented design

Answers to each question, for this codebase:

- **Hash tables to flat arrays, pointers to indexes.** Mostly **no**.
  - The ids are dense, never-reused `u64` counters, but removed sleeves stay as records and lifetime ids keep growing. A `Vec` indexed by id grows forever while about 20 entries are live.
  - Core maps are `BTreeMap`s because their iteration order is part of the journaled, deterministic behavior.
  - The history index already uses dense `u32` symbols and parallel arrays where it pays.
  - MAYBE, after measurement: `Vec`-backed core effects and jobs (append-only).
- **Packing data into the upper bits of an index.** **No.**
  - Generations are already a separate per-incarnation `u64` used for fencing.
  - Packing would add capacity and encoding rules with no measured win.
- **"Caching is cope; delete all caches."** The survey classified every copy:
  - **True caches that go:**
    - the job `log_bytes` length (also a correctness risk after a partial write);
    - the fleet `streamed` previous-value map (MAYBE).
  - **The transcript tail goes too** (up to 4,096 decoded lines per open sleeve, about 80,000 at 20 sleeves). Pages are read from the file instead: latest pages backwards, older pages forwards, stopping at the requested count. The first page (up to 2,000 entries) is timed before and after; the tail comes back only if that read turns out user-visibly slow.
  - **True caches that wait for measurement:**
    - the bus's serialized ring, a replay window that the stream log duplicates. Replacing it means reading resumes from the log, across compaction, for one to three clients;
    - the deep-cloned `State` in the published view. This one is the largest duplication, and DO-5's numbers decide its redesign: core-thread read requests, or `Arc<Sleeve>` with `make_mut`.
  - **Not caches; they stay:**
    - quota readings, doctor reports, device records and driver "previous value" state are observed state that cannot be recomputed;
    - the history index is the access path itself.
- **Structure of arrays.** **No** at our sizes.
  - `envoy_id` scans about 20 records.
  - History's dictionary is already parallel arrays.
- **Inline-capacity (stack) lists.** One real case: the history tokenizer keeps at most six ranges and allocates per word segment, so it becomes a fixed array with the existing fallback. Elsewhere, short `Vec`s are not on a path where allocation shows.
- **Bitsets.** The only `Vec<bool>`s are pt-qr's module and function masks: 177x177, about 63 KB, generated once at enrollment.
  - They become a small `u64`-word bitset, since the owner stated it as a rule. It is cold code, so the win is the rule, not speed.
  - Bool fields elsewhere are a handful of distinct facts, not scanned arrays.
- **Arenas.** **No.** JSON requests and history extraction build temporary trees and drop them, and `Vec` ownership already frees them deterministically. A safe arena would change `Value`'s ownership across the whole API.

**Lands now** (survey order):
- the bus ring slice (same as DO-2);
- history pages start at the cursor (`partition_point`) instead of rescanning;
- tokenizer without per-segment allocations;
- job `log_bytes` removed;
- bounded transcript reads (with DO-3);
- interned history source references;
- the transcript tail removed (above), and the pt-qr bitset;
- journal replay that applies events as they decode instead of building a second `Vec<Event>`;
- a cap on the hub's malloc arenas with `mallopt(M_ARENA_MAX)` through the `libc` crate already in the tree, measured before and after on a real hub.

## 6. Network without headscale (design only)

[network-rust-only.md](network-rust-only.md) is for the owner's review. Nothing is replaced. Its recommendation:

- **Identity is a pinned public key, not an address.** The hub keeps an owner-approved fleet directory: a stable node id, the key, and a short list of private candidate addresses:
  - the last verified address;
  - the laptop's **fixed address on the existing 192.168.22.0/24 WireGuard**;
  - an optional home-LAN DHCP reservation.

  The hub dials the candidates in order and accepts only the approved key. This solves the roaming laptop with no discovery and no reverse dial. mDNS, beacons and node-initiated connections are evaluated and deferred, since each needs a hub-side listener for node traffic.
- **Transport:** `pt-frame` over TCP with a Noise IK handshake (`snow`). It needs one Rust node service and no sshd. System OpenSSH carrying `pt-frame` is the alternative, with zero new crates and an existing proof of concept.
- **The tension to decide:** today `libc` is the only registry crate in the tree.
  - Noise brings in `snow`, whose published lock lists 139 entries; the selected set will be smaller, but it is real crypto supply chain.
  - OpenSSH brings in nothing but needs sshd set up and maintained on every node, Windows included.
  - Writing our own crypto, as pt-hash does for SHA, is not on the table for authentication.
- **Exit criteria before headscale goes:**
  - the real laptop is reachable on Wi-Fi and through WireGuard after moving;
  - wrong, revoked and rotated keys fail closed;
  - jobs survive connection loss at every stage;
  - updates reach a node with no internet;
  - Linux and Windows pass on real machines.

## 7. Questions for the owner

1. D9 (state-machine D1): should an `Interrupted` or `Error` settle clear Envoy's D9 origin? Today it does; SPEC says "a finished settle" clears it. D1 keeps today's behavior unless told otherwise.
2. Is explicit compaction coming? `Compact` is always refused today, so `Activity::Compacting` is dead code; D12 removes it unless it is wanted.
3. Network: Noise (`snow`, one self-contained Rust service) or system OpenSSH (zero crates, sshd per node)? Or keep headscale.
