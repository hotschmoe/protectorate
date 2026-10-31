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
