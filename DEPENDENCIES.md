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

`go/go.mod` adds nothing of its own: its direct requires (headscale, plus termcolor, zerolog and
tailscale.com, which headscale's main and `member` import) and every indirect module sit at the
versions headscale v0.29.7's go.mod selects, pinned by the committed `go/go.sum`. Its `go 1.27.3`
line is headscale's minimum toolchain.

| Module | Version | Reason |
|---|---|---|
| github.com/juanfont/headscale | v0.29.7 | the tailnet control server, built unmodified from its own main (SPEC 3.2) |
| tailscale.com | v1.104.4 (pinned by headscale v0.29.7) | tsnet for `member`: userspace WireGuard, DERP, NAT traversal; the same version headscale is built against |

## Dart

`app/pubspec.yaml` depends on the Flutter SDK alone (no flutter_lints, no flutter_test: lint rules
are listed in `app/analysis_options.yaml`). The packages below come in through the SDK, which pins
them; they are not ours to bump and change only with a Flutter upgrade (now 3.44.1).

| Package | Version | Reason |
|---|---|---|
| characters | 1.4.1 | transitive, Flutter SDK (grapheme clusters for text editing) |
| collection | 1.19.1 | transitive, Flutter SDK |
| material_color_utilities | 0.13.0 | transitive, Flutter SDK (Material colour schemes) |
| meta | 1.18.0 | transitive, Flutter SDK (annotations) |
| vector_math | 2.2.0 | transitive, Flutter SDK (transforms) |

Bundled asset, not a package: JetBrains Mono 2.304 Regular and Bold (`app/assets/fonts/`, SIL OFL
1.1, licence file beside it), the one typeface of the UI (SPEC 9).

## System tools the product calls

| Tool | Where | Reason |
|---|---|---|
| bwrap | hub | light sleeve sandbox (REVISIT: own namespaces helper) |
| systemd-run | hub, Linux nodes | per-incarnation and per-job scopes with limits |
| curl | hub, nodes | outbound HTTPS (releases, quota readers) instead of a Rust TLS stack (REVISIT) |
| git | hub | workspaces, worktrees, pushes |
