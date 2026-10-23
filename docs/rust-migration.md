# Rust migration (history)

The hub, node agent, CLI and MCP bridge were rewritten from Go to Rust in
October 2026. `master` up to tag `v0.4.3` holds the Go releases; the
`rust-yolo` branch removed Go entirely (`bc9b87a`) and then the tailnet
overlay, desktop control and unit tests. Hubs, nodes and devices are
installed fresh: nothing reads Go state.

Why and how, with every decision and the research behind it:
[2026-10-24-rust-direction.md](2026-10-24-rust-direction.md). The per-crate
parity log that used to live here is in git history
(`git show b8d2e67:docs/rust-migration.md`). The current design is
[ARCHITECTURE.md](../ARCHITECTURE.md).
