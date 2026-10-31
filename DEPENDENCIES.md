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

| Module | Version | Reason |
|---|---|---|

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
