# DEV

Branch and release rules.

## Branches

- `master` is the release branch. It stays clean: never committed to directly, only fast-forwarded from `dev-hotschmoe` at a release.
- `dev-hotschmoe` is the working branch.
- Feature branches (`feat/*`, `fix/*`) start from `dev-hotschmoe` and merge back into it with `git merge --no-ff` after all checks pass.

Generated code must be byte-identical everywhere. The pinned generators are buf 1.74.0, Dart protoc_plugin 25.1.0 and connectrpc 2.0.0; Flutter is pinned to 3.44.1 in the hosted workflows. Upgrade the generators together with regenerated output. `make gen` writes the Dart client (`app/lib/gen`), the descriptor sets the Rust wire layer embeds (`rust/crates/protectorate-wire/assets/`) and the MCP tool catalog; `make check-gen` fails when any of them is stale.

Checks before every merge: `make check` (fmt, clippy `-D warnings`, `make test`, `make app-check`, check-version), `make check-gen` when protos change, `bash scripts/test-rust-installers.sh` when installers change, and the platform builds relevant to the change. `make test` needs `flutter pub get` in `app/` once for the Flutter client scenarios. CI runs these checks and builds every release asset on pushes to `master` and `dev-hotschmoe` and on pull requests. Tests are real-binary e2e scenarios and fuzz targets only; lanes and host-only scenarios are in [docs/testing.md](docs/testing.md).

## Coordinating sleeves

Give each sleeve a branch and a complete brief with acceptance checks. Land
shared proto and persisted-state changes first, then rebase dependent branches;
for parallel schema work, reserve field numbers per branch before dispatch.
Commit schema changes together with `make gen` output.

Choose a merge order from those dependencies, merge one branch at a time with
`--no-ff`, and check the combined result. Run `make check` on the host before
merging each branch. Runtime scenarios that need systemd user scopes must run
on the host; a sleeve's skipped scenarios do not establish that the runtime
passes. Read each sleeve's report for completed work, skipped checks and gaps
before accepting it.

## Research references and access

`references/` (gitignored clones) is read-only research material, not the current
implementation. Relevant owner-provided sources are
`references/t3code` (native model catalogs and model/effort selection) and
`references/secondary/pi/packages/durable` (checkpoint/replay design). They
may exist only in the owner's hub checkout and are not guaranteed to be
included in a fresh clone or mounted inside a sleeve.

Check the visible workspace before using a reference. If it is unavailable,
report the missing path and request an explicit read-only reference mount or
an approved workspace containing it; do not work around denied access.
Distinguish inspected reference behavior, current Protectorate behavior, and
proposed changes. In particular, durable intent records do not make external
effects exactly-once or snapshot another harness's native session.

## Release

The version comes from release tags (`make version` prints it); nothing else defines it. `git describe` matches `v[0-9]*`, excluding the rolling `dev` tag.

1. On `dev-hotschmoe`, with `make check` and `make check-gen` passing, commit the release documentation and generated output. The installer selects the latest stable release by default; `--version` pins the installed release.
2. `git switch master && git merge --ff-only dev-hotschmoe`
3. `git tag vX.Y.Z` on master, then `git push origin master vX.Y.Z`.
4. The `release` workflow (`.github/workflows/release.yml`) gates publication on the workspace and installer checks, the Linux amd64 and arm64 bundles (`protectorate-rs`, sleeve runner, sleeve and node sandbox helpers, `protectorate`, `protectorate-mcpbridge`), the native Windows amd64 and arm64 node, CLI and MCP bridge, the Termux arm64 node cross-build with pinned NDK r28, and the Flutter desktop and web builds (`scripts/package-rust-release.sh` assembles and checks them). It publishes those binaries, `protectorate-web-<version>.tar.gz`, the Windows desktop zip, `VERSION`, and `SHA256SUMS` as a GitHub Release, using only the default `GITHUB_TOKEN`. The Android app is not part of the hosted release.

Every green push to `dev-hotschmoe` publishes the same required assets to a rolling prerelease tagged `dev`, after Rust and Flutter checks and cross-platform builds finish. The release name and checksummed `VERSION` asset contain the `git describe` version, never the moving tag. Publication is serialized; a delayed older green build cannot replace a newer published build. Stable uses GitHub's latest non-prerelease and never selects `dev`. Termux node installation and runtime still require a device trial; the hosted lane verifies its cross-link only.

## Install

Hub (Linux x86_64 or aarch64, per user, no sudo; re-run to upgrade):

```
curl -fsSL https://raw.githubusercontent.com/hotschmoe/protectorate/vX.Y.Z/install-hub.sh | bash
```

Flags go after `bash -s --`: `--channel stable|dev`, `--version`, `--release-repo`, `--release-dir`, `--home`, `--listen`, `--tls-listen`, `--tls-name`, `--public-url`, `--dry-run`, and `--no-start`. The installer verifies every required binary and the Flutter web archive against `SHA256SUMS`, then stages them in `${XDG_DATA_HOME:-~/.local/share}/protectorate/releases/<version>/` with an atomic `current` pointer. It writes and enables the `protectorate-hub` user unit; the hub creates its CA on first start. A working systemd user manager, systemd-run, Git, OpenSSL, Python 3 and curl are prerequisites. See [docs/install.md](docs/install.md).

Nodes: open the hub's web app, NODES, ADD NODE, and paste the line it shows on the new machine.
