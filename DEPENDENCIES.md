# Dependencies

The Zig 0.17.1 rewrite runs strictly on the Zig standard library (`std`) with **zero external Zig package dependencies**, eliminating Rust crates, Go modules, Headscale, Tailscale/`tsnet`, `curl`, and `gzip`.

## Zig (shipped)

| Package | Version | Reason |
|---|---|---|
| Zig `std` | 0.17.1 | Standard library only (Noise_IK, HTTP/WS server, TLS HTTP client, `flate`/`tar`, QR encoder, BM25 history index, CRC32C journal, VOPR DST) |

`build.zig.zon` intentionally has `.dependencies = .{}` (empty).

## Dart / Flutter (embedded Web UI & desktop/mobile app)

`app/pubspec.yaml` depends on the Flutter SDK alone (`app/analysis_options.yaml` holds lint rules):

| Package | Version | Reason |
|---|---|---|
| characters | 1.4.1 | Transitive via Flutter SDK (grapheme clusters) |
| collection | 1.19.1 | Transitive via Flutter SDK |
| material_color_utilities | 0.13.0 | Transitive via Flutter SDK |
| meta | 1.18.0 | Transitive via Flutter SDK |
| vector_math | 2.2.0 | Transitive via Flutter SDK |

`app/lib/api/` calls `dart:io` (desktop/Android) or `dart:js_interop` `fetch` + `WebSocket` (web). `zig build gen` regenerates `app/lib/api/api.g.dart` directly from `src/api/schema.zig`.

Bundled fonts under `app/assets/fonts/` (SIL OFL 1.1):
- Inter (`4.001`)
- JetBrains Mono (`2.304`)

## System Tools Invoked at Runtime

| Tool | Where | Reason |
|---|---|---|
| `bwrap` | Hub (Linux) | Namespace sandbox for local harness sleeves |
| `systemd-run`, `systemctl` | Hub / Linux nodes | Optional user-scope process lifecycle |
| `git` | Hub | Workspace status, worktrees, branch push (`GitPush`) |
| `gh` | Hub | Optional `doctor` GitHub CLI auth status probe |

## Removed vs. Legacy Architecture

- **Removed**: Go, `headscale`, `tailscale.com/tsnet`, `curl`, `gzip`, Cargo/Rust workspace crates (replaced by pure-Zig `Noise_IK_25519_ChaChaPoly_BLAKE2s`, `std.http.Client`, `std.compress.flate`, `std.tar.extract`, `vopr` DST).
- **Preserved**: Legacy `crates/` and `go/` directories remain intact in the repository for reference and post-mortem comparison; neither is required to build or run the Zig binaries.
