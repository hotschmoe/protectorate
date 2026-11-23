# Fleet Node Daemon (`src/node/`)

`protectorate-node` runs standalone on Linux (`x86_64`, `aarch64`), Windows (`x86_64`), and macOS (`aarch64`) with zero runtime dependencies.

## Lifecycle & Subcommands

- `join <ptjoin1:...>`: Decodes the base64url enrollment token, pins the hub X25519 public key + fingerprint, creates a `0o600` private key (`node_key.hex`) if absent, and outputs the node's static public key for owner approval.
- `authorize`: Tests the `Noise_IK` handshake + `Hello` RPC against the hub and marks `authorized = true` once approved.
- `serve [--stdio]`: Connects to the hub over TCP (with Happy-Eyeballs roaming dialing across `addrs` and exponential backoff reconnect) or raw stdin/stdout (`--stdio`).

## Telemetry & Self-Updates (`src/node/sys.zig`, `src/update/store.zig`)

`node/sys.zig` collects cross-platform hardware inventory, live CPU/RAM samples, mounted/fixed disks (`statfs` on Linux, `GetLogicalDriveStringsW`/`GetDiskFreeSpaceExW` on Windows), GPUs (`nvidia-smi`/DRM sysfs on Linux, Display Class registry on Windows), harness/toolchain probes (including `agy`), and user presence (`loginctl` on Linux, `GetLastInputInfo` on Windows, or `PROTECTORATE_NODE_TEST_PRESENCE` file override). `update/store.zig` verifies SHA-256 checksums, stages `.tar.gz` or `.zip` (`std.zip.extract`) releases, supports chunked `.upload_update` and `.update` RPCs on the node, and marks probation releases healthy after clean journal replay on hub startup.

## Execution, Admission Authority & Artifacts (`src/node/admission.zig`, `src/node/runner.zig`, `src/sandbox/exec.zig`)

`AdmissionManager` wraps `core/admission.zig` (`Authority`) with a 500ms tick loop, out-of-lock fencing callbacks, and a local Unix domain socket server (`<node_home>/admission.sock` on POSIX, queryable via `protectorate-node [--sock <path>] lease ...`). Each accepted `SubmitJob` acquires an admission lease (or queues in FIFO order until capacity opens), injects `PROTECTORATE_JOB_ID`, `PROTECTORATE_BOOT_ID`, `PROTECTORATE_LEASE_ID`, `PROTECTORATE_LEASE_EPOCH`, and `PROTECTORATE_ADMISSION_SOCK`, and executes inside a fenced `ProcessGuard` (Linux cgroup v2 `cgroup.kill`/`cgroup.events` with `/proc` fallback, Windows Job Objects with `ActiveProcesses == 0` verification, or POSIX process group kill). Completed jobs enter `.collecting` until the hub fetches artifacts and sends `AckJob`, while stdout/stderr stream into `output.log` and declared artifacts verify end-to-end with SHA-256.
