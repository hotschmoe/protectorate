# Dependencies we don't own

Every third-party package in the product, pinned exactly, with the reason our own code can't do the
job. `make check-deps` fails when `Cargo.lock` holds a registry crate missing from the Rust table.
A bump is a hand-picked change to this file and `Cargo.lock` in the same commit.

## Rust (shipped)

| Crate | Version | Reason |
|---|---|---|
| libc | 0.2.191 | the system call interface on Unix (namespaces, prctl, signals, pty, statvfs, setpriority, ioprio) |

## Rust (optional Verus feature, never enabled in the normal build)

`vstd` is pinned exactly in both phase 1 crates. The remaining packages are its transitive
dependencies, locked for the optional feature; the verifier itself provides its own `vstd`.

| Crate | Version | Reason |
|---|---|---|
| vstd | 0.0.0-2026-10-25-0158 | specifications and ghost proof support for Verus |
| verus_builtin | 0.0.0-2026-10-11-0230 | vstd dependency |
| verus_builtin_macros | 0.0.0-2026-10-25-0158 | vstd dependency |
| verus_state_machines_macros | 0.0.0-2026-10-25-0158 | vstd dependency |
| verus_prettyplease | 0.0.0-2026-09-06-0133 | Verus macro dependency |
| verus_syn | 0.0.0-2026-09-06-0133 | Verus macro dependency |
| convert_case | 0.4.0 | Verus macro dependency |
| synstructure | 0.13.2 | Verus macro dependency |
| syn | 2.0.119 | Verus macro dependency |
| indexmap | 2.14.2 | Verus state machine macro dependency |
| equivalent | 1.0.3 | indexmap dependency |
| hashbrown | 0.17.1 | indexmap dependency |

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
| crc32c | 0.6.8 | reference implementation for the pt-journal `crc_diff` differential target |
| rustc_version | 0.4.1 | transitive: crc32c build script |
| semver | 1.0.28 | transitive: rustc_version; also the reference for pt-update's `tags` differential target |
| qrcodegen | 1.8.0 | reference encoder for pt-qr's `encode` differential target (pt-qr is a trimmed port of it) |
| miniz_oxide | 0.8.9 | reference inflate and deflate for pt-embed's `inflate` differential target |
| adler2 | 2.0.1 | miniz_oxide dependency |

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

The generated hub client (`app/lib/api/`) adds no package: dart:io carries HTTP and WebSocket on
desktop and Android, and the web build calls the browser's `fetch` and `WebSocket` through the
SDK's `dart:js_interop` instead of `package:web`. `make gen` and `make check-gen` format it with
the SDK's `dart format`.

Bundled assets, not packages (`app/assets/fonts/`, each under the SIL OFL 1.1 with its licence file
beside it; SPEC 9):

| Font | Version | Reason |
|---|---|---|
| Inter | 4.001 | the interface typeface; Regular, Medium and SemiBold are static instances (opsz 14) of InterVariable |
| JetBrains Mono | 2.304 | code, tool transcripts, ids and output; Regular and Bold |

## System tools the product calls

| Tool | Where | Reason |
|---|---|---|
| bwrap | hub | light sleeve sandbox (REVISIT: own namespaces helper) |
| systemd-run | hub, Linux nodes | per-incarnation and per-job scopes with limits |
| systemctl | Linux nodes | signal, query and stop per-job scopes |
| curl | hub, nodes | outbound HTTPS (releases, quota readers) instead of a Rust TLS stack (REVISIT); pt-quota passes URL and tokens on a stdin config, not argv |
| git | hub | workspaces, worktrees, pushes |
| gzip | hub | unpacks the embedded headscale and member (`gzip -dc`) and packs them at build (`gzip -n`) (REVISIT) |
| gh | hub | the push token's holder (SPEC 7); `doctor` runs `gh auth status` |
| df, timedatectl | hub, Linux nodes | `doctor`: free disk (`df -P`) and NTP sync |
| loginctl | hub install | `install.sh` enables linger so the user service starts at boot |
| sha256sum | installers | `install.sh` and `install-node.sh` check assets against SHA256SUMS |

## Build tools (release only, never shipped)

| Tool | Where | Reason |
|---|---|---|
| cargo-zigbuild, zig 0.16.0 | dev hub | amd64 Linux assets linked for glibc 2.31 (`make release-assets`) |
| Go (go.mod's toolchain) | dev hub | headscale and member, through `scripts/net-build.sh` |
| VS Build Tools 14.44, rustup stable | officewin | the Windows node assets, built natively over ssh (no SDK on the dev hub) |
