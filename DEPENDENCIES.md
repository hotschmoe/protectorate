# Dependencies we don't own

Every third-party package in the product, pinned exactly, with the reason our own code can't do the
job. `make check-deps` fails when `Cargo.lock` holds a registry crate missing from the Rust table.
A bump is a hand-picked change to this file and `Cargo.lock` in the same commit.

## Rust (shipped)

| Crate | Version | Reason |
|---|---|---|
| libc | 0.2.191 | the system call interface on Unix (namespaces, prctl, signals, pty, statvfs, setpriority, ioprio) |

## Rust (dev-only: fuzz targets, never in a shipped binary)

| Crate | Version | Reason |
|---|---|---|
| libfuzzer-sys | 0.4.13 | cargo-fuzz harness: libFuzzer and its Rust entry point (all fuzz targets) |
| arbitrary | 1.5.0 | libfuzzer-sys dependency |
| cc | 1.10.0 | libfuzzer-sys build: compiles libFuzzer |
| find-msvc-tools | 0.1.15 | cc dependency |
| jobserver | 0.1.35 | cc dependency |
| shlex | 2.0.1 | cc dependency |
| libc | 0.2.191 | jobserver dependency (also shipped, above) |
| getrandom | 0.4.3 | jobserver dependency (lockfile entry) |
| cfg-if | 1.0.5 | getrandom dependency |
| r-efi | 6.0.0 | getrandom dependency on UEFI targets (lockfile entry) |
| serde_json | 1.0.154 | reference parser for pt-json's parse_diff target (feature float_roundtrip) |
| serde | 1.0.229 | serde_json dependency |
| serde_core | 1.0.229 | serde_json dependency |
| serde_derive | 1.0.229 | serde dependency |
| proc-macro2 | 1.0.107 | serde_derive dependency |
| quote | 1.0.47 | serde_derive dependency |
| syn | 3.0.9 | serde_derive dependency |
| unicode-ident | 1.0.27 | proc-macro2 dependency |
| itoa | 1.0.18 | serde_json dependency |
| memchr | 2.8.3 | serde_json dependency |
| zmij | 1.0.23 | serde_json dependency (float formatting) |

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
| systemctl | Linux nodes | signal, query and stop per-job scopes |
| curl | hub, nodes | outbound HTTPS (releases, quota readers) instead of a Rust TLS stack (REVISIT) |
| git | hub | workspaces, worktrees, pushes |
