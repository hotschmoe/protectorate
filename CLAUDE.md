# CLAUDE.md

Guidance for agents working on this repository. Read SPEC.md first; it is the design and wins
over everything in `docs/design/`. Shortcuts taken to make something work go in REVISIT.md.
Reference clones are in `/home/hotschmoe/github/protectorate/references/` (read-only, gitignored).

## Branches and releases

- Work happens on `dev-hotschmoe` (or a short-lived branch from it, merged back). It reaches
  `master` by a pull request. Never force-push either branch.
- `master` is what is released: every release tag (`vX.Y.Z` or `vX.Y.Z-PRE`, the one version
  source) is on a `master` commit. After a pull request merges, fast-forward `dev-hotschmoe` to
  `master` (`git pull --ff-only origin master`) before new work.
- A release is cut on `master`, locally: tag HEAD, `make release-assets`, `make release-publish`
  (`PUBLISH=1`). No CI builds or tags: the signing key and the arm64 build node are here.

## RULE 1 - ABSOLUTE (DO NOT EVER VIOLATE THIS)

You may NOT delete any file or directory unless the owner explicitly gives the exact command **in
this session**. This includes files you just created. If you think something should be removed,
stop and ask. Forbidden without that explicit approval: `git reset --hard`, `git clean -fd`,
`rm -rf`, `git worktree remove`, `git branch -D`, or any command that can delete or
overwrite code or data. Overwriting a file you created in this task with Write/Edit is fine.

## Code discipline

- NO EMOJIS anywhere.
- Hands-off toward harnesses: never wrap, patch, proxy or re-prompt an AI CLI. Use only its own
  interfaces (TUI or official headless protocol) and its own config and auth.
- **Third-party code is the supply-chain risk.** The Zig program uses `std` only and
  `build.zig.zon` has no dependencies. A package we don't own needs an entry in DEPENDENCIES.md
  with its reason, pinned exactly. Prefer a few hundred lines of our own code over a new dependency.
- Make it work, make it right, make it fast. A shortcut is fine if it is logged in REVISIT.md.
- No abstractions without a second real implementation. Minimal comments, only where they add what
  the code cannot say.
- No bulk-modifying scripts (codemods, giant sed/regex refactors).
- Full migrations only: no compat shims, `_legacy`/`_old` names or "remove later" code.
- Errors carry context at each layer; cancellation propagates.
- Follow [ENGINEERING.md](https://github.com/hotschmoe/hotschmoe-setup/blob/master/ENGINEERING.md): no unit
  tests; at most ten real-binary e2e scenarios block work. Run `make check` for the blocking
  gate. Reliability comes from the real-binary e2e script and the VOPR deterministic simulator.
- TigerStyle: fail-fast assertions through `src/tiger.zig`, no `test` blocks (`make check-levels`
  rejects them), builds in safe mode (`-Doptimize=safe`) so bounds and overflow checks stay on.

## Layout and commands

```
src/           the Zig program; main_*.zig are entry points (hub, node, vopr, levels, apigen, embed_web)
  core/ journal/ history/ quota/ update/ net/ api/   state machines, storage, wire formats
  drivers/ sandbox/ doctor/ node/ hub/ cli/          effects, daemons, the CLI
  dst/ tools/                                        VOPR simulator, codegen, web packer, level checker
  (every file has a tier L0-L5 and a group in src/tools/levels.zig; imports only go downward)
app/           Flutter client (desktop, Android, web)
scripts/       installers, release.sh, zig-e2e.sh, app-e2e.sh
go/            old headscale and tsnet glue, kept for reference; not built or used (DEPENDENCIES.md)
docs/          install.md, operate.md, srcmd/ (per-module notes), design/ and sessions/ (historical)
```

```
make build           # zig build (safe mode, version and commit stamped, web assets packed from WEB_DIR)
make release-all     # cross-compile the Linux, Windows and macOS binaries
make check-levels    # module tier rules and no test blocks
make gen / check-gen # write or verify app/lib/api/api.g.dart from src/api/schema.zig (never hand-edit it)
make dst             # VOPR deterministic simulation, 300 seeds
make e2e             # scripts/zig-e2e.sh against zig-out/bin (run make build first)
make web             # flutter build web (app-check runs flutter analyze)
make version         # the version builds carry: git describe of the v* tag, the one source of truth
make check           # check-levels, check-gen, app-check, build, dst, release-all, e2e
make release-assets  # also release-snapshot and release-publish (docs/install.md)
```

## Build hygiene (this machine: 12 cores, 30 GiB RAM, no swap)

- Each worktree has its own `.zig-cache` and `zig-out` (gitignored); never build into another
  agent's. Only the global Zig cache is shared.
- Pass `-j4` to direct `zig build` calls so parallel agents don't starve each other.
- Run make with `ZIG=zig` (the Zig on PATH, 0.17.1) and give an absolute `WEB_DIR`
  (`-Dweb-dir`) when the worktree is not your working directory.
- Don't run `make web` or `flutter build` in loops; one at a time.
