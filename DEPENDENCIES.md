# Dependencies we don't own

Every third-party package in the product, pinned exactly, with the reason our own code can't do the
job. `make check-deps` fails when `Cargo.lock` holds a registry crate missing from the Rust table.
A bump is a hand-picked change to this file and `Cargo.lock` in the same commit.

## Rust (shipped)

| Crate | Version | Reason |
|---|---|---|
| libc | 0.2.191 | the system call interface on Unix (namespaces, prctl, signals, pty) |

## Rust (dev-only: fuzz targets, never in a shipped binary)

| Crate | Version | Reason |
|---|---|---|

## Go

`go/go.mod` requires only headscale; everything else is what headscale's own go.mod selects, pinned
by the committed `go/go.sum`. Its `go 1.27.3` line is headscale's minimum toolchain.

| Module | Version | Reason |
|---|---|---|
| github.com/juanfont/headscale | v0.29.7 | the tailnet control server, built unmodified from its own main (SPEC 3.2) |
| tailscale.com | v1.104.4 (pinned by headscale v0.29.7) | tsnet for `member`: userspace WireGuard, DERP, NAT traversal; the same version headscale is built against |

## Dart

| Package | Version | Reason |
|---|---|---|

## System tools the product calls

| Tool | Where | Reason |
|---|---|---|
| bwrap | hub | light sleeve sandbox (REVISIT: own namespaces helper) |
| systemd-run | hub, Linux nodes | per-incarnation and per-job scopes with limits |
| curl | hub, nodes | outbound HTTPS (releases, quota readers) instead of a Rust TLS stack (REVISIT) |
| git | hub | workspaces, worktrees, pushes |
