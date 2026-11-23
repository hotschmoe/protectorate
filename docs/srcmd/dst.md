# Deterministic Simulation Testing (`src/dst/`)

Robustness rests on `vopr` (`src/dst/vopr.zig`), a Viewstamped-Replication-style deterministic simulator inspired by TigerBeetle. No unit test files exist; instead, `vopr` fuzz-drives the real `core`, `journal`, and `net` code under injected faults across hundreds of PRNG seeds.

## Simulated Components

- **`src/dst/sim_storage.zig`**: Modelssector-level disk writes, fsync durability barriers, bitflips, incomplete torn-tail writes on un-fsynced crashes, and mid-log corruption detection (`journal/codec.zig`).
- **`src/dst/sim_net.zig`**: Fault-injecting network model over real `Noise_IK` (`net/noise.zig`) handshakes, `pt-frame` (`net/frame.zig`) splitting, message reordering, packet drops, duplicate delivery, and tamper verification.
- **`src/dst/oracle.zig`**: `CoreWorld` + invariant checker asserting on every single step:
  - No overlapping active workspace claims.
  - Approval generation fencing consistency.
  - D9 flag cleanup whenever a sleeve settles, exits, stops, parks, or is removed.
  - Parent/child hierarchy bounds (`max_children`) and continuation accounting.
  - Job state machine conservation (no ghost running jobs on offline/removed nodes past lease abandonment, no illegal terminal transitions).
  - Full crash-replay equivalence: resetting `State` to empty and replaying the exact committed journal bytes must reproduce an identical canonical `State.state_hash(gpa)`.

## Reproducing Failures

```sh
zig build -Doptimize=safe dst -- --seeds 300 --steps 1500
zig build dst -- --seed 41 --steps 1500 --verbose
```
