# Fleet Node Daemon (`src/node/`)

`protectorate-node` runs standalone on Linux (`x86_64`, `aarch64`), Windows (`x86_64`), and macOS (`aarch64`) with zero runtime dependencies.

## Lifecycle & Subcommands

- `join <ptjoin1:...>`: Decodes the base64url enrollment token, pins the hub X25519 public key + fingerprint, creates a `0o600` private key (`node_key.hex`) if absent, and outputs the node's static public key for owner approval.
- `authorize`: Tests the `Noise_IK` handshake + `Hello` RPC against the hub and marks `authorized = true` once approved.
- `serve`: Runs the node loop (network.md): it connects to the hub over TCP, racing the saved `addrs` (re-resolved before each attempt) with Happy-Eyeballs staggering, and reconnects with jittered exponential backoff. There is no stdio mode.
- `supervise`, `install-service [--user]`, `uninstall [--user]`, `probation [--retry-rollback]`: Cross-platform service installation (systemd user/system units, macOS launchd agents, Windows `sc.exe`), restart supervision, and release probation rollback checks.
- `authorize` dials once through the same link and exits 0 only when the hub accepts a hello.
- `inventory`, `sample`, `presence`, `watch-presence`: Standalone JSON diagnostics matching node heartbeat telemetry.

## Telemetry & Self-Updates (`src/node/sys.zig`, `src/update/store.zig`)

`node/sys.zig` collects cross-platform hardware inventory, live CPU/RAM samples, mounted/fixed disks (`statfs` on Linux, `GetLogicalDriveStringsW`/`GetDiskFreeSpaceExW` on Windows), GPUs (`nvidia-smi`/DRM sysfs on Linux, Display Class registry on Windows), harness/toolchain probes (including `agy`), and user presence (`loginctl` on Linux, `GetLastInputInfo` on Windows, or `PROTECTORATE_NODE_TEST_PRESENCE` file override). `update/store.zig` verifies SHA-256 checksums, stages `.tar.gz` or `.zip` (`std.zip.extract`) releases, supports chunked `.upload_update` and `.update` RPCs on the node, and marks probation releases healthy after clean journal replay on hub startup.

## Execution, Admission Authority & Artifacts (`src/node/admission.zig`, `src/node/runner.zig`, `src/sandbox/exec.zig`)

`AdmissionManager` wraps `core/admission.zig` (`Authority`) with a 500ms tick loop, out-of-lock fencing callbacks, and a local Unix domain socket server (`<node_home>/admission.sock` on POSIX, queryable via `protectorate-node [--sock <path>] lease ...`). Each accepted `SubmitJob` acquires an admission lease (or queues in FIFO order until capacity opens), injects `PROTECTORATE_JOB_ID`, `PROTECTORATE_BOOT_ID`, `PROTECTORATE_LEASE_ID`, `PROTECTORATE_LEASE_EPOCH`, and `PROTECTORATE_ADMISSION_SOCK`, and executes inside a fenced `ProcessGuard` (Linux cgroup v2 `cgroup.kill`/`cgroup.events` with `/proc` fallback, Windows Job Objects with `ActiveProcesses == 0` verification, or POSIX process group kill). One thread per job runs it: it waits for admission, spawns the process and then copies stdout and stderr into `output.log` through a single bounded batched read (4 KiB per pipe, the same on POSIX and Windows). One supervisor thread for the whole node, started with the first job, enforces job timeouts and renews admission leases every 250 ms tick. A job id is reserved in the runner's table under its lock before any file is touched, so concurrent retries of one submission return the same job. The supervisor stops enforcing timeouts and lease renewals once the process has exited. Hub requests that can block run on the node loop's worker pool, not on the job threads. Completed jobs enter `.collecting` until the hub fetches artifacts and sends `AckJob`, while stdout/stderr stream into `output.log` and declared artifacts verify end-to-end with SHA-256.

`FetchLog` and `FetchArtifact` reads are clamped on the node to one piece (256 KiB, `net/limits.zig`) whatever `limit` the request carries; see network.md for the bounds.