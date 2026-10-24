#!/usr/bin/env bash
set -euo pipefail

mode=${1:-check}
case "$mode" in
  check|build|release) ;;
  *) echo 'usage: build-rust-termux.sh [check|build|release]' >&2; exit 2 ;;
esac

ndk=${ANDROID_NDK_HOME:-}
if [[ -z "$ndk" || ! -d "$ndk/toolchains/llvm/prebuilt/linux-x86_64/sysroot" ]]; then
  echo 'set ANDROID_NDK_HOME to an installed Android NDK r28' >&2
  exit 2
fi
case "$ndk" in
  *' '*) echo 'Android NDK path with spaces is not supported by this build command' >&2; exit 2 ;;
esac

target=aarch64-linux-android
api=24
toolchain="$ndk/toolchains/llvm/prebuilt/linux-x86_64"
sysroot="$toolchain/sysroot"
export CARGO_INCREMENTAL=0

case "$(uname -m)" in
  x86_64)
    compiler="$toolchain/bin/aarch64-linux-android${api}-clang"
    export CC_aarch64_linux_android="$compiler"
    export AR_aarch64_linux_android="$toolchain/bin/llvm-ar"
    export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$compiler"
    ;;
  aarch64)
    compiler=$(command -v clang-19)
    export CC_aarch64_linux_android="$compiler"
    export AR_aarch64_linux_android="$(command -v llvm-ar-19)"
    export CFLAGS_aarch64_linux_android="--target=$target -D__ANDROID_API__=$api --sysroot=$sysroot"
    export RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }-C linker=$compiler -C link-arg=--target=$target -C link-arg=--sysroot=$sysroot -C link-arg=-B$sysroot/usr/lib/aarch64-linux-android/$api/ -C link-arg=-L$sysroot/usr/lib/aarch64-linux-android/$api -C link-arg=-resource-dir -C link-arg=$toolchain/lib/clang/19 -C link-arg=-rtlib=compiler-rt -C link-arg=-fuse-ld=lld"
    ;;
  *) echo 'Termux cross build is configured for x86_64 or aarch64 Linux hosts' >&2; exit 2 ;;
esac

if ! rustup target list --installed | grep -qx "$target"; then
  echo "install the Rust target first: rustup target add $target" >&2
  exit 2
fi

repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo"
if [[ "$mode" == release ]]; then
  cargo build --locked --profile dist --target "$target" -p protectorate-node-termux --bin protectorate-rs-termux
else
  cargo "$mode" --locked --target "$target" -p protectorate-node-termux --bin protectorate-rs-termux
fi
