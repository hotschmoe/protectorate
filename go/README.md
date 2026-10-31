# go/

The only Go in Protectorate (SPEC 3.2), one module, `CGO_ENABLED=0`. Build with
`scripts/net-build.sh` (outputs `go/bin/<name>-<goos>-<goarch>`); test with `scripts/net-e2e.sh`.

- `cmd/headscale`: upstream headscale v0.29.7's main, verbatim. The hub runs it as a sandboxed child
  and administers it only through its CLI over the unix socket with `-o json`, never an API key.
- `cmd/member`: our tsnet glue. The hub runs `member --mode hub`, every node `member --mode node`,
  each as a child of the Rust process. Logs are short `key=value` lines on stderr.
- `policy.hujson`: the static tailnet policy, one rule `tag:hub -> tag:node:7475`, empty tagOwners.
- `e2e/nettest`: test tooling only (echo agent, TLS proxy standing in for Traefik, probe); never shipped.

## member

```
member --mode hub|node --state-dir DIR --control-url URL --hostname NAME
       [--socket PATH] [--forward PATH|127.0.0.1:PORT] [--udp-port N] [--print-registration] [--verbose]
```

- **Control-key pin.** Every start fetches `<control-url>/key?v=<capver>` and fingerprints the Noise
  key as `sha256:<hex of SHA-256 over its 32 raw bytes>` (`mkey:<hex>` decoded). The first start on
  an empty state dir writes `DIR/control-key.pin`; later starts fail closed with "control key
  changed" on a mismatch, or if state exists without a pin.
- **Enrollment.** `--print-registration` prints `{"event":"registration","auth_id":...,"auth_url":...,
  "control_key":...,"hostname":...,"mode":...}`, waits, prints `{"event":"registered",...}` once
  approved and exits 0. The Rust side shows auth_id and control_key as the QR, checks the key against
  headscale's own `/key`, then runs `headscale auth register --auth-id ID --user U -o json` and
  `headscale nodes tag -i ID -t tag:node -o json`. Started without the flag, an unregistered member
  exits ("not registered").
- **Hub socket** (`--socket`, mode 0600, never removed if it already exists). One line per connection:
  - `DIAL <host>:<port>\n` -> `OK\n` then raw bytes both ways, or `ERR <msg>\n`. host is a tailnet IP,
    hostname or first DNS label of a peer that control tagged `tag:node`; anything else is refused.
  - `STATUS\n` -> one JSON line: mode, backend_state, control_key, health, self, and peers with
    name, ips, tags, online, cur_addr (direct path), relay (DERP region), last_handshake, rx/tx bytes.
  A node member serves STATUS on `--socket` too, and answers DIAL with `ERR`: nodes never dial.
- **Node listener.** tailnet `:7475` only. Each connection is WhoIs-checked (control-assigned
  `tag:hub` or it is closed), then `--forward` gets one header line
  `{"node":...,"stable_id":...,"tags":[...],"ip":"100.x"}\n` before the spliced stream. The agent
  must re-check `tags`. The node also disco-pings the hub so a restarted member is found again.
- **What tsnet permits.** There is no runtime switch for peerAPI, so `ts_omit_peerapiserver` compiles
  it out and hub mode refuses a build that still has it. The hub never calls `Listen`; LocalAPI is
  in-memory. Logtail, portmapper (UPnP/NAT-PMP), captive-portal probes, c2n and the rest of the
  list in `net-build.sh` are compiled out. `--udp-port` fixes the WireGuard UDP port.
