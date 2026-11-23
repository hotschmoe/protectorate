# Fleet Node Daemon (`src/node/`)

`protectorate-node` runs standalone on Linux (`x86_64`, `aarch64`), Windows (`x86_64`), and macOS (`aarch64`) with zero runtime dependencies.

## Lifecycle & Subcommands

- `join <ptjoin1:...>`: Decodes the base64url enrollment token, pins the hub X25519 public key + fingerprint, creates a `0o600` private key (`node_key.hex`) if absent, and outputs the node's static public key for owner approval.
- `authorize`: Tests the `Noise_IK` handshake + `Hello` RPC against the hub and marks `authorized = true` once approved.
- `serve [--stdio]`: Connects to the hub over TCP (with exponential backoff reconnect across `addrs`) or raw stdin/stdout (`--stdio`).

## Execution & Artifacts (`src/node/runner.zig`)

Each accepted `SubmitJob` executes in its own working directory (`<node-home>/jobs/<job_id>/`) and POSIX process group (`pgid = 0`) so cancellation (`CancelJob`) terminates process trees cleanly. Stdout and stderr stream incrementally into `output.log` for `FetchLog`, and declared output artifacts are hashed with SHA-256 and transferred in 256 KiB chunks via `FetchArtifact`, re-verified end-to-end by `hub/fleet.zig`.
