# Protectorate

A self-hosted software factory on one hub machine. You talk to Envoy, our orchestrator agent, and
Envoy talks to sleeves. A sleeve is a vendor coding CLI (Claude Code, Codex, Grok Build,
Antigravity) or Pi, running untouched in a light sandbox and bound to one directory. The hub also
reaches a fleet of untrusted machines (nodes) to inventory them and run build, test and
maintenance jobs across architectures. It is a software factory only: no email, calendar or
errand agent.

Protectorate is one Zig 0.17.1 program that uses only the Zig standard library: no external Zig
packages, and no Go, Rust, headscale or tsnet at build or run time. [SPEC.md](SPEC.md) is the
design; [REVISIT.md](REVISIT.md) lists the shortcuts taken so far and what "right" would be;
[DEPENDENCIES.md](DEPENDENCIES.md) lists what the build and runtime rely on.

## How it works

- **One writer.** All state sits behind one state lock. Each event goes through a pure
  `apply(state, event)`, is journaled (CRC32C frames, fsynced), then executed. Every
  write to a harness or a node is recorded before it is sent.
- **Headless first.** Sleeves run through each harness's official headless protocol, and the
  transcript is the chat the app shows. We never wrap, patch or re-prompt a harness.
- **Fleet link.** A node dials the hub's fleet listener (`--fleet-listen`) over TCP, or over a
  stdio pipe for ssh, with a `Noise_IK_25519_ChaChaPoly_BLAKE2s` handshake against pinned keys.
  A node joins with a `ptjoin1:` token, the owner approves its key, and unapproved keys are
  dropped before any frame is read. The hub sends the job and update RPCs over that link.
- **Two binaries.** `protectorate` (hub, CLI, stdio MCP for Envoy) and `protectorate-node`. The
  Flutter web app is packed into the hub binary at build time.
- **Testing.** No unit tests. Reliability comes from the VOPR deterministic simulator
  (`src/dst/`) and a real-binary e2e script.

## Layout

```
src/       the program; every file has a tier (L0-L5) checked by src/tools/levels.zig
  core/ journal/ history/ quota/ update/ net/ api/    state machines, storage, wire formats, schema
  drivers/ sandbox/ doctor/ node/ hub/ cli/           harness drivers, sandbox, daemons, the CLI
  dst/ tools/                                         VOPR simulator, codegen, web packer, level checker
app/       Flutter client (desktop, Android, web); app/lib/api/api.g.dart is generated
scripts/   installers, release.sh, zig-e2e.sh, app-e2e.sh
docs/      install.md, operate.md, srcmd/ (notes per module group), design/ and sessions/ (historical)
go/        old headscale and tsnet glue, kept for reference; not built or used
```

## Build and test

You need Zig 0.17.1. Flutter (for the web app and `make app-check`) is needed only to embed the
UI and to run `make check`; Dart is needed for `make gen` and `make check-gen`.

```
make build           # zig build: protectorate and protectorate-node into zig-out/bin (safe mode)
make release-all     # cross-compile Linux (amd64, arm64), Windows and macOS binaries
make web             # flutter build web; run it before make build to embed the UI (WEB_DIR=app/build/web)
make check-levels    # module tier rules; no test blocks allowed
make gen / check-gen # write or verify app/lib/api/api.g.dart from src/api/schema.zig
make dst             # VOPR deterministic simulation (300 seeds, 1500 steps)
make e2e             # scripts/zig-e2e.sh against the binaries in zig-out/bin
make check           # check-levels, check-gen, app-check, build, dst, release-all, e2e
make version         # the version every build here carries: the git tag (docs/install.md, Versioning)
```

A single failing simulation replays with `zig build dst -- --seed N --steps M --verbose`. The e2e
script starts real hubs and nodes on ephemeral ports with `--no-sandbox` in temp homes, so it
needs `curl`, `git`, `python3` and (for its Pi scenarios) the settings in a gitignored
`.env.e2e` at the repo root that the Makefile reads. `scripts/app-e2e.sh` drives the web app in
headless Chromium.

## Run

```
zig-out/bin/protectorate serve --home DIR      # the hub; prints the owner and envoy-only tokens once
zig-out/bin/protectorate pair NAME             # a pairing code for a device
zig-out/bin/protectorate status | doctor | quota
zig-out/bin/protectorate sleeve | node | job | approval | history | envoy ...
zig-out/bin/protectorate node join-token       # on the hub: the join token for a new node
zig-out/bin/protectorate-node join TOKEN       # on a node; then the owner runs: protectorate node approve ID
zig-out/bin/protectorate-node authorize        # on the node, once approved; then: protectorate-node serve
```

`protectorate` with no arguments prints the full command list. To install a hub or a node, see
[docs/install.md](docs/install.md). Day-to-day commands are in [docs/operate.md](docs/operate.md).
The notes per module group are in [docs/srcmd/](docs/srcmd/README.md).

## Release

Releases are built and published locally, not in CI. Tag a clean commit on `master`, then
`make release-assets` and `make release-publish` (`PUBLISH=1` to upload). Details and the
asset list are in [docs/install.md](docs/install.md#release-assets).
