# DEV

Branch and release rules.

## Branches

- `master` is the release branch. It stays clean: never committed to directly, only fast-forwarded from `dev-hotschmoe` at a release.
- `dev-hotschmoe` is the working branch.
- Feature branches (`feat/*`, `fix/*`) start from `dev-hotschmoe` and merge back into it with `git merge --no-ff` after all checks pass.

Generated code must be byte-identical everywhere, so the generators are pinned, and CI and the release workflow install exactly these: buf 1.74.0, protoc-gen-go v1.36.12, protoc-gen-connect-go v1.21.1, Dart protoc_plugin 25.1.0 and connectrpc 2.0.0, Flutter 3.44.1. Upgrade them together, locally and in `.github/workflows/`, in one commit with the regenerated output.

Checks before every commit and before every merge: `make test` (tagged vet, fuzz seeds and real-binary end-to-end scenarios), `gofmt -l cmd internal` printing nothing, `make check-gen`, `make cross`, `make app-check`, `make check-version`. `make check` runs them all. `make test` builds the tagged binary first, clears inherited hub credentials and enables `PROT_E2E=1`; `make app-check` runs Flutter analysis. Optional tailnet and host runtime lanes are documented in docs/testing.md. CI runs the same on pushes to `master` and `dev-hotschmoe` and on pull requests.

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

The version is the git tag (`make version` prints it); nothing else defines it.

1. On `dev-hotschmoe`, with all checks passing, set `DEFAULT_VERSION` in `install-hub.sh` and the pinned tag in the README install line to the new tag, and commit.
2. `git switch master && git merge --ff-only dev-hotschmoe`
3. `git tag vX.Y.Z` on master, then `git push origin master vX.Y.Z`.
4. The `release` workflow (`.github/workflows/release.yml`) runs the checks, builds the five binaries (`protectorate-{linux,windows}-{amd64,arm64}[.exe]`, `protectorate-android-arm64`), `protectorate-web-<version>.tar.gz`, the Android APK when it builds (optional), and `SHA256SUMS`, and publishes them as a GitHub Release. It uses only the default `GITHUB_TOKEN`.

## Install

Hub (Linux x86_64 or aarch64, per user, no sudo; re-run to upgrade):

```
curl -fsSL https://raw.githubusercontent.com/hotschmoe/protectorate/vX.Y.Z/install-hub.sh | bash
```

Flags go after `bash -s --`: `--version`, `--listen`, `--public-url`, `--home`, `--dry-run`. It verifies every download against `SHA256SUMS`, installs `~/.local/bin/protectorate`, the web client in `~/.local/share/protectorate/web` and the other platforms' builds in `~/.local/share/protectorate/bin`, writes and enables the `protectorate-hub` user unit, and runs `hub tailnet install`. Missing prerequisites (bwrap, tmux, git, a systemd user manager) are listed with package names, never installed. See `docs/install.md`.

Nodes: open the hub's web app, NODES, ADD NODE, and paste the line it shows on the new machine.
