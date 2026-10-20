# DEV

Branch and release rules.

## Branches

- `master` is the release branch. It stays clean: never committed to directly, only fast-forwarded from `dev-hotschmoe` at a release.
- `dev-hotschmoe` is the working branch.
- Feature branches (`feat/*`, `fix/*`) start from `dev-hotschmoe` and merge back into it with `git merge --no-ff` after all checks pass.

Generated code must be byte-identical everywhere, so the generators are pinned, and CI and the release workflow install exactly these: buf 1.74.0, protoc-gen-go v1.36.12, protoc-gen-connect-go v1.21.1, Dart protoc_plugin 25.1.0 and connectrpc 2.0.0, Flutter 3.44.1. Upgrade them together, locally and in `.github/workflows/`, in one commit with the regenerated output.

Checks before every commit and before every merge: `make test` (tagged vet, fuzz seeds and real-binary end-to-end scenarios), `gofmt -l cmd internal` printing nothing, `make check-gen`, `make cross`, `make app-check`, `make check-version`. `make check` runs them all. `make test` builds the tagged binary first, clears inherited hub credentials and enables `PROT_E2E=1`; `make app-check` runs Flutter analysis. Optional tailnet and host runtime lanes are documented in docs/testing.md. CI runs the same on pushes to `master` and `dev-hotschmoe` and on pull requests.

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

`archive/` and `references/` are read-only research material, not the current
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

1. On `dev-hotschmoe`, with all checks passing, set the pinned tag in the README and docs/install.md install lines to the new tag, and commit. The installer selects the latest stable release by default; `--version` pins the installed release.
2. `git switch master && git merge --ff-only dev-hotschmoe`
3. `git tag vX.Y.Z` on master, then `git push origin master vX.Y.Z`.
4. The `release` workflow (`.github/workflows/release.yml`) runs the checks, builds the five binaries (`protectorate-{linux,windows}-{amd64,arm64}[.exe]`, `protectorate-android-arm64`), `protectorate-web-<version>.tar.gz`, the Android APK when it builds (optional), `VERSION` and `SHA256SUMS`, and publishes them as a GitHub Release. It uses only the default `GITHUB_TOKEN`.

Every green push to `dev-hotschmoe` publishes the same required assets to a rolling prerelease tagged `dev`, after race, Go/Flutter checks and Windows desktop build finish. The release name and checksummed `VERSION` asset contain `make version`, never the moving tag. Publication is serialized; a delayed older green build cannot replace a newer published build. Stable uses GitHub's latest non-prerelease and never selects `dev`.

## Install

Hub (Linux x86_64 or aarch64, per user, no sudo; re-run to upgrade):

```
curl -fsSL https://raw.githubusercontent.com/hotschmoe/protectorate/vX.Y.Z/install-hub.sh | bash
```

Flags go after `bash -s --`: `--channel stable|dev`, `--version`, `--release-repo`, `--release-dir`, `--listen`, `--public-url`, `--home`, `--dry-run`. It verifies every download against `SHA256SUMS`, installs `~/.local/bin/protectorate`, and puts all platform binaries and `web/` into `${XDG_DATA_HOME:-~/.local/share}/protectorate/releases/<version>/` with an atomic `current` pointer. It writes and enables the `protectorate-hub` user unit and runs `hub tailnet install`. Existing units are preserved; remove old explicit `--web` and `--binary-dir` paths so they follow `current`. Missing prerequisites (bwrap, tmux, git, a systemd user manager) are listed with package names, never installed. See `docs/install.md`.

Nodes: open the hub's web app, NODES, ADD NODE, and paste the line it shows on the new machine.
