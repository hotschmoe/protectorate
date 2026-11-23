# Fleet Node Daemon (`src/node/`)

`protectorate-node` runs standalone on Linux (`x86_64`, `aarch64`), Windows (`x86_64`), and macOS (`aarch64`) with zero runtime dependencies.

## Lifecycle & Subcommands

- `join <ptjoin1:...>`: Decodes the base64url enrollment token, pins the hub X25519 public key + fingerprint, creates a `0o600` private key (`node_key.hex`) if absent, and outputs the node's static public key for owner approval.
- `authorize`: Tests the `Noise_IK` handshake + `Hello` RPC against the hub and marks `authorized = true` once approved.
- `serve [--stdio]`: Connects to the hub over TCP (with Happy-Eyeballs roaming dialing across `addrs` and exponential backoff reconnect) or raw stdin/stdout (`--stdio`).

## Telemetry & Self-Updates (`src/node/sys.zig`, `src/update/store.zig`)

`node/sys.zig` collects cross-platform hardware inventory, live CPU/RAM samples, mounted/fixed disks (`statfs` on Linux, `GetLogicalDriveStringsW`/`GetDiskFreeSpaceExW` on Windows), GPUs (`nvidia-smi`/DRM sysfs on Linux, Display Class registry on Windows), harness/toolchain probes (including `agy`), and user presence (`loginctl` on Linux, `GetLastInputInfo` on Windows, or `PROTECTORATE_NODE_TEST_PRESENCE` file override). `update/store.zig` verifies SHA-256 checksums, stages `.tar.gz` or `.zip` (`std.zip.extract`) releases, supports chunked `.upload_update` and `.update` RPCs on the node, and marks probation releases healthy after clean journal replay on hub startup.

## Execution & Artifacts (`src/node/runner.zig`)

Each accepted `SubmitJob` executes in its own working directory (`<node-home>/jobs/<job_id>/`) and POSIX process group (`pgid = 0`) so cancellation (`CancelJob`) terminates process trees cleanly. Stdout and stderr stream incrementally into `output.log` for `FetchLog`, and declared output artifacts are hashed with SHA-256 and transferred in 256 KiB chunks via `FetchArtifact`, re-verified end-to-end by `hub/fleet.zig`.
