#!/usr/bin/env bash
set -euo pipefail

# Release binaries link against an old glibc so the hub runs on older
# distributions than the build host (Debian 12 has 2.36, Ubuntu 24.04 2.39).
glibc=2.31
version=${1:?usage: build-rust-dist.sh VERSION amd64|arm64}
arch=${2:?usage: build-rust-dist.sh VERSION amd64|arm64}
case "$arch" in
  amd64) triple=x86_64-unknown-linux-gnu ;;
  arm64) triple=aarch64-unknown-linux-gnu ;;
  *) echo "unsupported architecture: $arch" >&2; exit 2 ;;
esac
dist=${PROTECTORATE_RELEASE_DIST:-dist}
target_dir=${CARGO_TARGET_DIR:-target}

rustup target add "$triple" >/dev/null
PROTECTORATE_BUILD_VERSION="$version" cargo zigbuild --locked --profile dist --target "$triple.$glibc" \
  -p protectorate-server -p protectorate-sleeve-runner -p protectorate-cli -p protectorate-mcpbridge --bins
mkdir -p "$dist"
for component in protectorate-rs protectorate-sleeve-runner protectorate-sleeve-sandbox-helper protectorate protectorate-mcpbridge; do
  binary="$target_dir/$triple/dist/$component"
  needed=$(objdump -T "$binary" | grep -o 'GLIBC_[0-9.]*' | sed 's/GLIBC_//' | sort -uV | tail -1)
  [[ "$(printf '%s\n%s\n' "$needed" "$glibc" | sort -V | tail -1)" == "$glibc" ]] || {
    echo "$component needs glibc $needed, above the $glibc floor" >&2
    exit 1
  }
  cp -- "$binary" "$dist/$component-linux-$arch"
done
if [[ "$(uname -m)" == "${triple%%-*}" ]]; then
  reported=$("$dist/protectorate-rs-linux-$arch" version --json)
  [[ "$reported" == *'"version":"'"$version"'"'* ]] || { echo "binary reports $reported, expected $version" >&2; exit 1; }
fi
