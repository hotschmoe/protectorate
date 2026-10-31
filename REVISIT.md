# REVISIT

Shortcuts taken to make something work, and what "right" (then "fast") would be. Newest last.
Each entry: where, what was done, what right looks like.

- **Release signing** (SPEC 6): no signing key; updates trust SHA256SUMS over HTTPS. Right: an
  offline-held key and a signature checked against a key embedded in the binary.
- **Outbound HTTPS through curl** (hub, nodes): no Rust TLS stack. Right: decide between
  curl forever, or a forked and trimmed rustls when curl's absence or output parsing bites.
- **Sandbox through bwrap**: right is our own small namespaces helper (SPEC 3.4).
- **SHA-256 is portable Rust** (pt-hash): about 235 MB/s on the aarch64 dev host, near
  coreutils `sha256sum`. The aarch64 SHA2 instructions (`vsha256hq_u32` and friends in
  `std::arch::aarch64`, plus `is_aarch64_feature_detected!("sha2")`) would give several GB/s, but
  calling a `#[target_feature]` function from generic code needs `unsafe`, which pt-hash forbids.
  Right: once a release-sum or content-addressing path is measured as slow, add the accelerated
  compress behind one audited unsafe call (in pt-hash with a narrow exemption, or in pt-sys).
- **No CRC-32C yet** (pt-hash): SPEC 15 lists it for the journal; it lands with pt-journal.
- **pt-http is Unix-only** (std `UnixStream`), and outside Linux `peer_cred` is Unsupported, so
  unix connections are refused there. Right: `getpeereid` on the BSDs and macOS; a Windows build
  gates the unix listener out if a Windows binary ever needs pt-http.
- **Unix socket mode is set after bind** (pt-http `bind_unix`): a connect can race the chmod to
  0600. The hub checks peer uid anyway. Right: bind inside a 0700 directory the hub owns. Bind also
  fails on an existing path, so the hub must decide about stale sockets after a crash.
- **No Date header** in pt-http responses (RFC 9110 wants one from servers with a clock).
- **pt-http client opens one connection per request** (`Connection: close`) and a unix connect
  has no timeout. Right, if the CLI ever gets chatty: keep-alive reuse for idempotent requests.
- **WebSocket extras left out** (pt-http): no permessage-deflate and no subprotocol negotiation
  helper (a handler can set `Sec-WebSocket-Protocol` itself). Origin checks are the hub's job.
- **pt-http is about 1950 lines** (about 1700 without blank and comment lines) against a 1500-line
  target. Right: trim when the hub's real use shows which parts can go.
