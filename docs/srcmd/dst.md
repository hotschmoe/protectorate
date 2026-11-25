# Deterministic Simulation Testing (`src/dst/`)

Robustness rests on `vopr` (`src/dst/vopr.zig`), a Viewstamped-Replication-style deterministic simulator inspired by TigerBeetle. No unit test files exist; instead, `vopr` fuzz-drives the real `core`, `journal`, and `net` code under injected faults across hundreds of PRNG seeds.

## Simulated Components

- **`src/dst/sim_storage.zig`**: Models sector-level disk writes, fsync durability barriers, bitflips, incomplete torn-tail writes on un-fsynced crashes, and mid-log corruption detection (`journal/codec.zig`).
- **`src/dst/io_sim.zig` and `src/dst/sim_fleet.zig`**: Seeded completion IO drives one real fleet bus and two to six node links with a frozen network allocator. Simulated streams preserve byte order; transfers can be short, completions delayed, connects refused or timed out, and consumers slowed. Partitions stall and then reset streams; endpoints can crash and restart. Explicit corruption faults alter a Noise handshake and a record to exercise authentication failure; an authenticated malformed frame exercises framing rejection. The workload races viable endpoints, exchanges fragmented encrypted requests and chunked replies, samples maximum-sized frames spanning five slabs, fills the RPC table, times out a call, replaces a duplicate identity, revokes a key during a handshake, and shuts down with a handshake in flight. It services connection events independently and checks slot and slab conservation, request parity, terminal results, generation fencing, replacement disconnect suppression, cancellation, and reconnect followed by a successful RPC. After a fault stops, each initial node must reconnect and finish its RPC within the 30,000 ms bound for that phase; the tamper/reconnect phase has a separate 20,000 ms bound. Failures print a recent event trace and VOPR's reproduction command.
- **`src/dst/sim_http.zig`**: the HTTP wire code and the stream ring, no IO. Chunked bodies encoded with random chunk sizes, extensions and trailers decode to the original bytes however the input is split and however small the output window is, stop at the exact end of the message and never overrun on corrupted framing. Request heads built with random casing and spacing parse to the right fields, the end of the head is found at the same place however the bytes arrive, and mutated heads never panic or return slices outside the buffer. Masked WebSocket frames split at random points reassemble to the original payloads, while unmasked, reserved-bit and oversized control frames are rejected. The record ring is run with a hook that fires after every word access, and at a random share of those points the other side runs: a reader executes while the writer is mid-append (and while it evicts), or a whole append executes while a reader is mid-copy. Whatever the interleaving, a reader that is told a record is valid has read exactly the bytes written, a lagged report always has the tail past the reader, and `find_after` only returns the record right after the sequence asked for. `ws_out.Out` (the live WebSocket sender) is driven with random publication, ping replies and a close request, a send buffer of random size and sends that take random prefixes; the bytes it emits are parsed as frames and every data frame must carry exactly its record, which fails if a control frame is ever inserted inside a data frame. The strict-parsing rules (repeated headers, bare line feeds) are checked too.
- **`src/dst/oracle.zig`**: `CoreWorld` + invariant checker asserting on every single step:
  - No overlapping active workspace claims.
  - Approval generation fencing consistency.
  - D9 flag cleanup whenever a sleeve settles, exits, stops, parks, or is removed.
  - Parent/child hierarchy bounds (`max_children`) and continuation accounting.
  - Job state machine conservation (no ghost running jobs on offline/removed nodes past lease abandonment, no illegal terminal transitions).
  - Full crash-replay equivalence: resetting `State` to empty and replaying the exact committed journal bytes must reproduce an identical canonical `State.state_hash(gpa)`.
  - Resleeve replay with recorded quota picks, recorded parks, and exits that predate quota selection.

## Reproducing Failures

```sh
zig build -Doptimize=safe dst -- --seeds 300 --steps 1500
zig build dst -- --seed 41 --steps 1500 --verbose
zig build -Doptimize=safe dst -- --seeds 300 --steps 1500 --fleet-long
```
