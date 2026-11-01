# pt-fleet

The hub's side of the fleet (SPEC 3.2, 3.5): headscale and the hub `member` as supervised
children, enrollment, the node RPC client and the poller. std threads only; Linux.

```rust
let fleet = Fleet::start(Config::lan_direct(dir, headscale_bin, member_bin, listen))?;
```

`Fleet::start` writes `<dir>/headscale/` (`dir` is the tailnet dir: the hub passes
`<hub home>/tailnet` or `--fleet-dir`) (config as JSON, IP-pinned DERP map with STUN
off, `go/policy.hujson`, sqlite, Noise key, a relative unix socket), runs headscale inside bwrap
(only that dir writable, nothing else of `dir` visible), waits for `/health`, makes the
`fleet` user, enrolls the hub member once (`tag:hub`), then runs `member --mode hub`. Both
children restart with backoff (1 s doubling to 30 s) and are health-checked every 5 s.
`Config` fields set production values: `server_url` (Traefik's https URL), `derp` (host, IPv4,
port, `tls`), `trusted_proxies`. `shutdown()` stops member then headscale and returns once
both exited; dropping the last `Fleet` clone also stops them.

| Call | What |
|---|---|
| `admin()` | headscale's CLI, `-o json`: `nodes`, `node`, `register`, `tag`, `rename`, `expire`, `delete` (owner removal only) |
| `approve(auth_id, name, fingerprint)` | checks the fingerprint against headscale's `/key`, then register, `tag:node`, rename; records the pin in `enrolled.json` |
| `record_pending(Registration)`, `pending()` | what the owner scanned or pasted but has not approved; headscale keeps registrations 15 min in memory and cannot list them |
| `enrolled()` | approved nodes and the control key each pinned |
| `Registration::parse(text)` | the QR payload `{"v":1,"auth_id":...,"control_key":"sha256:...","name":...}` |
| `connect(node)` | `NodeClient` to `<tailnet ip or name>:7475` through member's DIAL |
| `status()` | member STATUS: backend state, control key, peers (online, direct `cur_addr` or DERP `relay`) |
| `poll(PollConfig, publish)` | `Poller`; `publish(FleetEvent::Node(Box<pt_api::Node>) / Removed(id))` |
| `snapshot_headscale(label)` | stop headscale, copy db and keys to `<dir>/snapshots/<ms>-<label>/`, restart |
| `control_fingerprint()` | `sha256:<hex>` of headscale's Noise key, as members pin it |
| `headscale_state()`, `member_state()`, `kill_child(which)` | supervisor state; kill for tests and doctor |

`NodeClient` methods, one request at a time with a per-call deadline (`set_timeout`, 30 s
default): `hello`, `inventory`, `sample`, `presence`, `submit_job`, `job_status`, `fetch_log`
(bytes plus `LogEnd`), `cancel_job`, `ack_collected`, `list_jobs`. A node's error frame is
`Error::Node(ErrorReply)`; a timeout or protocol error kills the connection (dial again).

`Poller` (defaults): presence every 15 s, samples every 60 s, inventory every 10 min, node list
every 15 s, one thread and one connection per node. `pt_api::Node.id` is headscale's node id;
`online` is member's peer online flag and the last round succeeding; `last_seen_ms` is the last
good round. `nodes()` and `node(id)` also give `NodeView.hello` (the agent's own node id,
version and journal epoch) and the last error.

Files under `<dir>/`: `headscale/`, `member/` (tsnet state and control-key pin),
`member.sock` (a fresh `member-N.sock` when a killed member left one), `member.log`,
`pending.json`, `enrolled.json`, `snapshots/`.

`Embedded { name, sha256, gz }` is a binary the hub carries gzipped; `unpack(dir)` writes it once
to `<dir>/<name>-<sha256>` (mode 0755) through the system `gzip -dc` and checks the SHA-256 on
every start, so a corrupted or replaced copy is unpacked again, never run.

Test: `cargo test --workspace --test local` (needs bwrap and `go/bin`, built by
`scripts/net-build.sh host` when Go is on PATH). Real machines: `examples/fleet.rs`.
