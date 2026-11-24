# Fleet Networking (`src/net/`)

The Zig rewrite eliminates all Go, Headscale, Tailscale/`tsnet`, and external tunnel dependencies in favor of a self-contained cryptographic fleet protocol implemented directly over Zig `std`.

## Protocol Stack

1. **Cipher Suite (`src/net/noise.zig`)**: `Noise_IK_25519_ChaChaPoly_BLAKE2s` with prologue `"protectorate-fleet-v1"`. Provides forward-secure, mutually authenticated 1-RTT sessions using static X25519 keys. Session records cap at 32 KiB plaintext + 16-byte Poly1305 tag (`max_record_wire = 32786`), with automatic expiry boundaries (1 GiB / 24h).
2. **Framing & RPC (`src/net/frame.zig`, `src/net/msg.zig`)**: Length-prefixed frames (`[u32 len][u8 kind][u64 id][payload]`) distinguishing `.request`, `.response`, `.chunk`, `.end`, and `.err`. Hub-initiated RPCs use odd IDs (`1, 3, 5, ...`); node-initiated requests use even IDs (`2, 4, 6, ...`), avoiding stream head-of-line ID collisions.
3. **Transports & Roaming (`src/net/transport.zig`, `src/node/serve.zig`, `src/hub/fleet.zig`)**: Uniform `NoiseConn` over TCP (`std.Io.net.Stream`, default hub listener `0.0.0.0:7475`) or `--stdio` pipes (`std.Io.File`) for SSH/process bridging without opening firewall ports. `NoiseConn.dial_any_tcp` races candidate hub endpoints (LAN IPs, WireGuard IPs, DNS names such as `protectorate.hotschmoe.com`) with 150 ms Happy-Eyeballs stagger and bounded timeouts so offline LAN routes never block WireGuard roaming. Nodes send 5 s heartbeats and recycle the link if no frame arrives within 15 s; `FleetServer` shuts down stale sessions (`sweep_stale`) and suppresses stale-socket `on_disconnected` callbacks when a newly roamed socket replaces an older session for the same static key.

Each `PeerSession` has one reference owned by its connection thread and one by its map entry; RPC callers and stale sweeps hold additional temporary references. Removing or replacing a map entry releases only the map reference, and the thread releases its own after its final use. Fleet shutdown stops acceptance, shuts down every accepted socket including handshakes, and waits for connection threads and their hooks before freeing the server. Runtime shutdown wakes fleet RPCs and waits for its detached workers before freeing shared state.

Dial races use a process-lifetime I/O backend and page allocator: a losing dial may remain blocked after the caller's deadline or after another dial wins. Each worker owns a race-state reference, and a winning connection transfers to the caller's I/O and allocator before return.

## Two-Step Node Enrollment

1. Hub exports `ptjoin1:<base64url>` (`GetEnrollment`, displayable as ASCII QR via `src/qr.zig`) containing `hub_public_key`, `hub_fingerprint` (`sha256:...`), and `addrs`.
2. Node runs `protectorate-node join <ptjoin1:...>`, generates/loads its `0o600` X25519 keypair, saves hub identity pin + addresses, and prints its public key + fingerprint.
3. Unapproved nodes fail Noise_IK responder validation (`UnknownPeer`) and are dropped before accepting any `Hello` or `Job` frame. Once owner runs `AddNode` + `ApproveNode`, node static key enters `allowed_peers` and `protectorate-node authorize` / `serve` connects and begins heartbeating.
