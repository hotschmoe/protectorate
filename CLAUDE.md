# CLAUDE.md

Guidance for agents working on branch `next`. Read SPEC.md first; it is the design and wins over
everything in `docs/design/`. Shortcuts taken to make something work go in REVISIT.md. Reference
clones are in `/home/hotschmoe/github/protectorate/references/` (read-only).

## RULE 1 - ABSOLUTE (DO NOT EVER VIOLATE THIS)

You may NOT delete any file or directory unless the owner explicitly gives the exact command **in
this session**. This includes files you just created. If you think something should be removed,
stop and ask. Forbidden without that explicit approval: `git reset --hard`, `git clean -fd`,
`rm -rf`, `cargo clean`, `git worktree remove`, `git branch -D`, or any command that can delete or
overwrite code or data. Overwriting a file you created in this task with Write/Edit is fine.

## Code discipline

- NO EMOJIS anywhere.
- Hands-off toward harnesses: never wrap, patch, proxy or re-prompt an AI CLI. Use only its own
  interfaces (TUI or official headless protocol) and its own config and auth.
- **Third-party code is the supply-chain risk.** Our own crates are cheap; a crate we don't own
  needs an entry in DEPENDENCIES.md with its reason, pinned exactly (`=x.y.z`). `make check-deps`
  enforces this. Prefer a few hundred lines of our own code over a new dependency.
- Make it work, make it right, make it fast. A shortcut is fine if it is logged in REVISIT.md.
- No abstractions without a second real implementation. Minimal comments, only where they add what
  the code cannot say.
- No bulk-modifying scripts (codemods, giant sed/regex refactors).
- Full migrations only: no compat shims, `_legacy`/`_old` names or "remove later" code.
- Errors carry context at each layer; cancellation propagates.
- **No unit tests, ever** (owner: "unit tests are a waste and offered no value add"). Only:
  - **complex end-to-end tests** that would show a future regression: they run our built binaries
    (`protectorate`, `protectorate-node`) or real external programs (harness CLIs, curl, git,
    headscale, member) against real sockets, files and processes, and check a whole scenario
    across that boundary;
  - **fuzz targets** (`crates/<crate>/fuzz/`) for parsers and state machines, differential
    against a reference wherever one exists.

  A test that calls a library function with crafted inputs and checks its return value is a unit
  test, even under `tests/`. Put such edge cases into fuzz seeds instead. `#[cfg(test)]` modules
  are not allowed.
- `#![forbid(unsafe_code)]` in every crate except the explicitly exempted OS FFI crates.

## Layout and commands

```
crates/        Rust workspace (every crate here is ours; package names start with pt- or protectorate)
crates/X/fuzz/ cargo-fuzz targets per crate (own [workspace], outside ours; dev-only deps allowed; cargo +nightly fuzz)
go/            the only Go: official headscale main and our tsnet `member`
app/           Flutter client (desktop, Android, web)
```

```
make build       # debug build of the workspace
make release     # release binaries for the host
make test        # e2e test binaries side by side (PT_TEST_JOBS), then every fuzz seed replayed
make check       # fmt --check, clippy -D warnings, test, check-deps, check-gen, app-check, check-version
make version     # the version builds carry: git describe of the v* tag, the one source of truth
make check-deps  # fails on any third-party crate not listed in DEPENDENCIES.md
make gen         # writes app/lib/api/api.g.dart from crates/pt-api (never hand-edit it); needs dart
```

## Build hygiene (this machine: 12 cores, 30 GiB RAM, no swap)

- Use your own target dir, never another agent's: `export CARGO_TARGET_DIR=/home/hotschmoe/github/protectorate/target/next-<task>` (a 10 GiB tmpfs shared by all agents; keep it small).
- `export CARGO_BUILD_JOBS=4` so parallel agents don't starve each other.
- Build with `--workspace` (never `-p`), debug unless you are measuring.
- Don't run `flutter build` or `go build` of headscale in loops; one at a time.
