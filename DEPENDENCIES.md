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
| libfuzzer-sys | 0.4.13 | cargo-fuzz harness (crates/*/fuzz) |
| arbitrary | 1.5.0 | libfuzzer-sys dependency |
| cc | 1.10.0 | libfuzzer-sys build (compiles libFuzzer) |
| find-msvc-tools | 0.1.15 | cc dependency |
| jobserver | 0.1.35 | cc dependency |
| shlex | 2.0.1 | cc dependency |
| getrandom | 0.4.3 | jobserver dependency |
| r-efi | 6.0.0 | getrandom dependency (UEFI only) |
| libc | 0.2.191 | jobserver and getrandom dependency |
| cfg-if | 1.0.5 | getrandom and sha2 dependency |
| sha1 | 0.10.6 | reference for pt-hash's SHA-1 (pt-hash fuzz) |
| sha2 | 0.10.9 | reference for pt-hash's SHA-256 (pt-hash fuzz) |
| digest | 0.10.7 | sha1 and sha2 dependency |
| block-buffer | 0.10.4 | digest dependency |
| crypto-common | 0.1.7 | digest dependency |
| generic-array | 0.14.7 | digest dependency |
| typenum | 1.20.1 | generic-array dependency |
| version_check | 0.9.5 | generic-array build |
| cpufeatures | 0.2.17 | sha1 and sha2 dependency |

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
