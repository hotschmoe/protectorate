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
  node job replay after a hub SIGKILL, crash and full-disk durability; see
  below).
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

## Crash and full-disk lane

`protectorate-server`, `crash_consistency::` and `disk_full::`, part of the
default run (about 15 s and 6 s).

- `crash_consistency::acknowledged_mutations_survive_hub_sigkill_at_random_points`
  runs a real hub with two nodes and six clients that mutate mail, notes,
  tokens, device pairings, inventory, jobs, sleeves and leases while scheduled
  and manual backups run. The hub (and every other round the node) is
  SIGKILLed after a random delay and restarted on the same home. Each restart
  must come up, keep every change a client saw succeed (an acknowledged ack,
  cancel, approval or request id included), hold no half-applied record,
  verify its audit chain, leave no dead writer's temp file, and every backup
  archive must restore. A call that failed around the kill may or may not
  have applied, and the test accepts both.
- `crash_consistency::torn_log_tails_and_stray_temp_files_do_not_stop_the_hub`
  adds what SIGKILL cannot: a half-written last line in `mailbox.jsonl` and
  the audit log, and temp files of dead and live writers.
- `disk_full::full_disk_fails_writes_cleanly_keeps_reads_and_recovers_without_restart`
  runs a real hub with its home on a 3 MiB tmpfs that the test mounts in its
  own unprivileged user and mount namespace (`unshare --user --map-root-user
  --mount`; the hub joins it with `nsenter --preserve-credentials`), so the
  disk fills with real ENOSPC and no root. Writes of every store, a sleeve
  spawn, a job, a lease and a backup must fail with an error that names the
  failed write, change nothing in memory or on disk, and leave no torn log
  line or archive; reads keep working; a restart on the full disk comes up;
  after space returns the same process takes writes again, including the
  names that failed, with no restart.

The kill delays and the order of operations come from a seed printed at the
start (`crash_consistency: PROTECTORATE_CRASH_SEED=...`). Replay a failure with
`PROTECTORATE_CRASH_SEED=<seed>`; the kill points still depend on timing, so a
replay narrows rather than fixes the interleaving. More rounds:
`PROTECTORATE_CRASH_ITERATIONS=40` (default 4).

Not covered: power loss. The kernel keeps SIGKILLed writers' page cache, so
the tests cannot show a missing fsync; they show a missing write, a torn
line, an unhandled leftover or a store that refuses to load. The
audit log is appended without fsync. Chat logs of live sleeves are
not exercised on a full disk (they need a harness peer).

## Build speed

Incremental compilation stays on for local `make test` and `make check`; the
hosted workflows set `CARGO_INCREMENTAL=0` themselves. Setting it locally
costs about 2.4x the CPU to rebuild after a hub edit, and switching it
between runs recompiles every workspace crate.

The default `cc` linker on aarch64 Linux is GNU ld. On the 12-core dev host,
`cargo build --workspace --tests` after touching the hub takes:

| Linker | Wall | CPU |
|---|---|---|
| GNU ld 2.40 | 7.9 s | 26.5 s |
| lld 14 (Debian) | 3.9 s | 14.2 s |
| rust-lld 22 (toolchain, `-B .../gcc-ld`) | 3.7 s | 14.5 s |
| mold 1.10 (Debian) | 3.4 s | 6.3 s |

The linker is a per-user setting, so hosts without mold are unaffected. With
mold installed (`apt install mold`), add to `~/.cargo/config.toml` (x86_64
Linux already links with rust-lld by default):

```toml
[target.aarch64-unknown-linux-gnu]
rustflags = ["-C", "link-arg=-fuse-ld=mold"]
```

Changing rustflags rebuilds everything once; a `RUSTFLAGS` environment
variable replaces this setting.

`target/` can live on a size-capped tmpfs. A cold `cargo build --workspace
--tests` plus clippy writes 2.7 GB and needs about 1 GB of memory beyond that;
rebuild times match the SSD, but builds stop writing tens of GB a day to it
and stop competing with the hub's fsyncs. A full tmpfs fails the build with
ENOSPC instead of exhausting memory. The dev host mounts one from
`/etc/fstab` with earlyoom as a backstop that prefers build processes:

```
tmpfs /path/to/protectorate/target tmpfs rw,nosuid,nodev,noatime,size=10g,mode=0755,uid=<you>,gid=<you> 0 0
```

Prefer `--workspace` builds and select tests by name. A `-p <crate>` build
resolves features for that crate alone, so a crate whose features differ from
the workspace build is recompiled under a new hash and stored twice. Cargo
never removes stale artifacts from `target/`; reclaim space with `cargo clean`.
Every worktree has its own `target/`, so clean it before removing a merged
worktree.

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
