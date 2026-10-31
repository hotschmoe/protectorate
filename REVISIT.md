# REVISIT

Shortcuts taken to make something work, and what "right" (then "fast") would be. Newest last.
Each entry: where, what was done, what right looks like.

- **Release signing** (SPEC 6): no signing key; updates trust SHA256SUMS over HTTPS. Right: an
  offline-held key and a signature checked against a key embedded in the binary.
- **Outbound HTTPS through curl** (hub, nodes): no Rust TLS stack. Right: decide between
  curl forever, or a forked and trimmed rustls when curl's absence or output parsing bites.
- **Sandbox through bwrap**: right is our own small namespaces helper (SPEC 3.4).
- **Journal retention** (pt-journal): every snapshot and segment is kept forever, because product
  code deletes nothing yet (RULE 1). Right: a retention rule (keep the last N snapshots and their
  segments, archive the rest into backups) once the owner approves deleting old segments.
- **CRC-32C in pt-journal** (SPEC 15 lists it under `pt-hash`): a plain one-table loop. Right: move
  it to `pt-hash` when that crate exists; fast: slicing-by-8 or the SSE4.2/ARMv8 CRC instruction.
