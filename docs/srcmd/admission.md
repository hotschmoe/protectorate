# Node Admission Authority (`src/core/admission.zig`, `src/node/admission.zig`)

Protectorate nodes enforce a single authoritative admission controller (`Authority` at L2, `AdmissionManager` at L4) governing concurrent and exclusive workloads, capacity leases, and process-tree fencing.

## Core Invariants

1. **Boot-Scoped Monotonic Epochs**: Every `protectorate-node` startup establishes a `boot_id` (`src/node/sys.zig`: `/proc/sys/kernel/random/boot_id` + per-process boot entropy). Each granted lease receives a strictly increasing 64-bit `epoch` scoped to that `boot_id`. Stale epochs or mismatched `boot_id`s fail `verify_epoch` immediately.
2. **Exclusive vs. Shared Holds + Strict FIFO Queue**:
   - `shared` holds run concurrently while slots, long-running slots, and `MemAvailable` margin allow.
   - `exclusive` holds require zero active root holds and prevent subsequent root holds (even small shared requests) from jumping ahead of queued waiters.
   - Waiting requests hold bounded FIFO tickets that expire cleanly if `now >= wait_deadline_ms` or if the waiter disconnects without renewing.
3. **Child Borrowing Under Parent Lifetime**: A job or holder operating under an active root lease may acquire shared child borrows (`parent_epoch = <root_epoch>`). Child deadlines never exceed the parent's remaining lifetime, and releasing or expiring the parent immediately revokes and fences every descendant borrow without deadlocking against an `exclusive` waiter behind the parent.
4. **Fence-Before-Reuse & Fail-Closed Quarantine**:
   - Expiring or releasing a lease moves the grant to `.fencing`, terminating its entire process tree (`src/sandbox/exec.zig` `ProcessGuard`: cgroup v2 `cgroup.kill` + `cgroup.events` `populated 0` and `/proc/<pid>` scan on Linux, `TerminateJobObject` + `QueryInformationJobObject` `ActiveProcesses == 0` on Windows, `-pgid` `SIGKILL` + signal-0 poll on macOS).
   - Held capacity is NOT returned to waiting jobs or leases until `confirm_fence` observes zero surviving processes.
   - If fencing fails past timeout, the node enters `quarantined` state, refuses all new grants, surfaces `quarantined` in `Hello` heartbeats and `Node`/`NodeAdmission` APIs, and requires explicit owner inspection (`ClearNodeQuarantine` / `protectorate lease unquarantine`).
5. **Artifact Collection Boundary**: Completed jobs transition their lease into `.collecting` so the holder stops incurring command runtime timeouts while `hub/runtime.zig` fetches declared artifacts over Noise_IK, then calls `ack_job` to fence and release capacity immediately upon receipt.

## Control Planes

- **Local Socket (`<node_home>/admission.sock` on POSIX)**: Exposed to on-node child processes via `PROTECTORATE_ADMISSION_SOCK`, along with `PROTECTORATE_JOB_ID`, `PROTECTORATE_BOOT_ID`, `PROTECTORATE_LEASE_ID`, and `PROTECTORATE_LEASE_EPOCH`. Accessible locally via `protectorate-node [--sock <path>] lease (list|acquire|renew|release|verify|unquarantine)`.
- **Hub RPC & CLI**: `AcquireLease`, `RenewLease`, `ReleaseLease`, `VerifyEpoch`, `GetNodeAdmission`, `ClearNodeQuarantine`, plus extended `RunJob` admission fields (`exclusive`, `slots`, `long_slots`, `mem_bytes`, `wait_ms`, `lease_ttl_ms`, `parent_epoch`) and CLI subcommands `protectorate job run|artifact` and `protectorate lease ...`.
