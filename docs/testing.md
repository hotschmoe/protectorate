# Testing

Rules: [testing-policy.md](testing-policy.md).

## Everyday lanes

```sh
make test       # cargo test --locked --workspace: every e2e scenario plus every fuzz seed
make check      # fmt, clippy -D warnings, make test, flutter analyze, check-version
```

`make test` needs Linux with unprivileged user namespaces, tmux and git. The
scenarios live in `rust/crates/*/tests/`. A crate with several scenario
files builds them into one test binary, `e2e` (`tests/e2e.rs` declares each
file as a module), so the hub links once per crate and the scenarios run in
parallel threads. Libraries and binaries have no unit-test harness
(`test = false`) and no doctests, except the two compile-checked examples in
`protectorate-mcpbridge`. `protectorate-cli --test chat_commands` keeps its own
binary because it has its own `main` and doubles as the sandbox helper. A
scenario's name is `<file>::<test>`, so `cargo test -p protectorate-hub
--test e2e governance::` runs one file.

- `protectorate-server`: the real `protectorate-rs` binary (backup and
  restore, harness drivers, the checked-in Flutter client against a Rust hub,
  node job replay after a hub SIGKILL).
- `protectorate-cli`: the real `protectorate` binary against a real hub,
  node and private tmux server, plus the installed-companion forwarding.
- `protectorate-hub`: the hub's HTTP API with real sandboxed sleeves, sleeve
  runner, tmux, git, protocol peers for each harness and an in-process node
  agent.
- `protectorate-node`: node jobs, files and git sources through the sandbox
  helper over the node API.
- `protectorate-sandbox`: real mount namespaces, cgroups, overlays and
  supervisor death.
- `protectorate-mcpbridge`: the real bridge binary and its committed tool
  catalog.
- `protectorate-node-windows`: the Windows binary (Windows hosts only).

## Build speed

Incremental compilation stays on for local `make test` and `make check`; the
hosted workflows set `CARGO_INCREMENTAL=0` themselves. Setting it locally
costs about 2.4x the CPU to rebuild after a hub edit, and switching it
between runs recompiles every workspace crate.

The default `cc` linker on aarch64 Linux is GNU ld. Linking with lld cuts
link CPU after a hub edit from about 23 s to 7 s and the wall time of
`cargo build --workspace --tests` from 8-10 s to 5-7 s. It is a per-user setting, so hosts
without lld are unaffected; with lld installed, add to `~/.cargo/config.toml`
(x86_64 Linux already links with rust-lld by default):

```toml
[target.aarch64-unknown-linux-gnu]
rustflags = ["-C", "link-arg=-fuse-ld=lld"]
```

Changing rustflags rebuilds everything once.

Prefer `--workspace` builds and select tests by name. A `-p <crate>` build
resolves features for that crate alone, so a crate whose features differ from
the workspace build is recompiled under a new hash and stored twice. Cargo
never removes stale artifacts from `target/`; reclaim space with `cargo clean`.

To keep that rare, every third-party dependency is declared once in the root
`[workspace.dependencies]` with the union of the features the workspace uses,
and members write `dep = { workspace = true }`. Add a feature to the root
entry, not to a member. Two features are enabled per crate on purpose so the
Windows and Android nodes stay lean: axum `ws`/`json` (from `protectorate-wire`
off Windows and Android, and `protectorate-hub`) and chrono `clock` (members
that need the clock, and `protectorate-node` off Android). What still differs
between `-p` and `--workspace` builds is features that only other third-party
crates enable (memchr, once_cell, serde_core, syn, rustix, subtle, crc32fast and
libc defaults); only the small crates beneath them rebuild.

## Host-only lanes

These are `#[ignore]`d because they need host state; run them by name with
`cargo test -p <crate> --test e2e <file>:: -- --ignored`:

- `protectorate-server`, `service_job_restart::`: a working systemd user
  manager.
- `protectorate-sleeve-runner`, `pub_offline::`, `go_mod_offline::` or
  `flutter_offline::`, and `protectorate-hub`, `terminal_sleeve::`: published
  cache generations passed as `PROTECTORATE_TEST_PUB_LOWER`,
  `PROTECTORATE_TEST_GO_MOD_LOWER` or the `PROTECTORATE_TEST_FLUTTER_*`
  variables.
- `protectorate-hub`, `sleeve_resleeve::`: an installed, authenticated Pi
  (`PROTECTORATE_LIVE_PI`, `PROTECTORATE_LIVE_PI_AUTH`), copied only into the
  test's private hub state.

## Fuzzing

Targets are plain `fn(&[u8])` in `rust/crates/protectorate-fuzz`; each has a
corpus under `rust/fuzz/corpus/<target>/`. `make test` replays every committed
seed through its target (`cargo test -p protectorate-fuzz`), and fails if a
target has no seeds or a corpus has no target.

Campaigns use libFuzzer through cargo-fuzz on nightly; `rust/fuzz` is its
own workspace, outside the default build:

```sh
rustup toolchain install nightly --profile minimal
cargo install cargo-fuzz --locked
cargo +nightly fuzz list --fuzz-dir rust/fuzz
t=wire_json; mkdir -p /tmp/fz/$t
cargo +nightly fuzz run --fuzz-dir rust/fuzz $t /tmp/fz/$t rust/fuzz/corpus/$t \
  -- -max_total_time=120 -timeout=30 -rss_limit_mb=4096
```

The scratch directory comes first so new inputs stay out of the committed
corpus. The four driver targets spawn a process per input and run slowly.
A crash leaves its input in `rust/fuzz/artifacts/<target>/`; reproduce it
with `cargo +nightly fuzz run --fuzz-dir rust/fuzz <target> <file>`. Commit
the fix together with the input copied into `rust/fuzz/corpus/<target>/`
under a descriptive name, so the replay keeps catching it.

To add a target, write a `pub fn(&[u8])` in protectorate-fuzz, list it in
`TARGETS`, add `rust/fuzz/fuzz_targets/<target>.rs` with its `[[bin]]` entry,
and create `rust/fuzz/corpus/<target>/` with at least one seed.
