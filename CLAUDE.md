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
- Tests are real-binary e2e (`crates/*/tests/*.rs` driving built binaries or real sockets/files)
  or fuzz targets (`fuzz/`), never unit tests (`#[cfg(test)]` modules are not allowed).
- `#![forbid(unsafe_code)]` in every crate except the explicitly exempted OS FFI crates.

## Layout and commands

```
crates/        Rust workspace (every crate here is ours; package names start with pt- or protectorate)
fuzz/          cargo-fuzz targets (outside the workspace; dev-only deps allowed)
go/            the only Go: official headscale main and our tsnet `member`
app/           Flutter client (desktop, Android, web)
```

```
make build       # debug build of the workspace
make release     # release binaries for the host
make test        # e2e tests
make check       # fmt --check, clippy -D warnings, test, check-deps
make check-deps  # fails on any third-party crate not listed in DEPENDENCIES.md
```

## Build hygiene (this machine: 12 cores, 30 GiB RAM, no swap)

- Use your own target dir, never another agent's: `export CARGO_TARGET_DIR=/home/hotschmoe/github/protectorate/target/next-<task>` (a 10 GiB tmpfs shared by all agents; keep it small).
- `export CARGO_BUILD_JOBS=4` so parallel agents don't starve each other.
- Build with `--workspace` (never `-p`), debug unless you are measuring.
- Don't run `flutter build` or `go build` of headscale in loops; one at a time.
