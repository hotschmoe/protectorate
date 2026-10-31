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
| libfuzzer-sys | 0.4.13 | the libFuzzer runtime behind `cargo fuzz` (every `crates/*/fuzz/`) |
| arbitrary | 1.5.0 | transitive: libfuzzer-sys |
| cc | 1.10.0 | transitive: builds libFuzzer for libfuzzer-sys |
| find-msvc-tools | 0.1.15 | transitive: cc |
| jobserver | 0.1.35 | transitive: cc |
| shlex | 2.0.1 | transitive: cc |
| getrandom | 0.4.3 | transitive: jobserver |
| r-efi | 6.0.0 | transitive: getrandom (UEFI targets only) |
| cfg-if | 1.0.5 | transitive: getrandom |
| libc | 0.2.191 | transitive: jobserver, getrandom |
| crc32c | 0.6.8 | reference implementation for the pt-journal `crc_diff` differential target |
| rustc_version | 0.4.1 | transitive: crc32c build script |
| semver | 1.0.28 | transitive: rustc_version |

## Go

| Module | Version | Reason |
|---|---|---|

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
