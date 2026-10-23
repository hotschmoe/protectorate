# Rust node on Termux

The Android app remains outside the Rust rewrite. A Termux node is part of the
fleet, so the node-only `protectorate-rs-termux` target provides `version --json`,
`node join`, `node install`, `node doctor`, `node uninstall`, and `node serve`
without linking the Linux hub or its namespace sandbox. `node install` copies
the binary to `$PREFIX/bin/protectorate-rs`, joins the hub, writes managed
termux-services and Termux:Boot scripts, and tries
to enable the service. `--dry-run` previews paths without changing them;
`--no-start` stages files without enabling the service. Uninstall disables the
service and leaves the binary, scripts, and state for manual inspection. A
hub-initiated uninstall checks the installer marker on the service run script
before invoking `$PREFIX/bin/sv-disable`.

Build or type-check from Linux with Rust's `aarch64-linux-android` target and
Android NDK r28:

```sh
rustup target add aarch64-linux-android
ANDROID_NDK_HOME="$ANDROID_HOME/ndk/28.2.13676358" scripts/build-rust-termux.sh check
ANDROID_NDK_HOME="$ANDROID_HOME/ndk/28.2.13676358" scripts/build-rust-termux.sh build
ANDROID_NDK_HOME="$ANDROID_HOME/ndk/28.2.13676358" scripts/build-rust-termux.sh release
```

The script uses NDK Clang on x86_64 hosts. On AArch64 hosts, it uses native
`clang-19` and `llvm-ar-19` with the NDK sysroot. The linked debug binary is
`target/aarch64-linux-android/debug/protectorate-rs-termux`. API level 24 is
the build baseline. The release binary is
`target/aarch64-linux-android/release/protectorate-rs-termux`. NDK versions and
paths are supplied by the builder; the example version is the one verified on
the development machine.

Join stores the node key, its hub-issued certificate and the hub CA under
`--home DIR/tls` and the joined hub, name and address in `DIR/node.json`. The
node serves the agent only over mutual TLS on that private address (port 7475
by default) and accepts only the hub's certificate. Jobs that run directly as
the Termux user require the explicit `--allow-plain-jobs` flag. The node
attaches the shared Rust binary updater. A guardian process supervises an
updated child, restarts it after a swap, and restores the previous binary if
the new binary exits or fails to confirm during its trial.

Current limits: namespace sandboxed jobs are unavailable under Termux, and
the node reports that capability as absent. Linux `/proc` metrics can be
unavailable or restricted on Android and are reported as probe warnings.
The cross-build checks compilation and linking; a real Termux device still
needs join, mutual TLS reachability, job, service registration, and update
rollback trials. Hub release asset selection for Android is tracked separately.
