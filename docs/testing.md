# Testing

Rules: [testing-policy.md](testing-policy.md).

## Everyday lanes

```sh
make test       # cargo test --locked --workspace: every e2e scenario plus every fuzz seed
make check      # fmt, clippy -D warnings, make test, flutter analyze, check-version
```

`make test` needs Linux with unprivileged user namespaces, tmux and git. The
scenarios live in `rust/crates/*/tests/`:

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

## Host-only lanes

These are `#[ignore]`d because they need host state; run them by name with
`cargo test -p <crate> --test <file> -- --ignored`:

- `protectorate-server --test service_job_restart`: a working systemd user
  manager.
- `protectorate-sleeve-runner --test pub_offline|go_mod_offline|flutter_offline`
  and `protectorate-hub --test terminal_sleeve`: published cache generations
  passed as `PROTECTORATE_TEST_PUB_LOWER`, `PROTECTORATE_TEST_GO_MOD_LOWER` or
  the `PROTECTORATE_TEST_FLUTTER_*` variables.
- `protectorate-hub --test sleeve_resleeve`: an installed, authenticated Pi
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
