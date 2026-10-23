#!/usr/bin/env bash
set -euo pipefail

version=${1:?usage: package-rust-release.sh VERSION}
[[ "$version" =~ ^[A-Za-z0-9][A-Za-z0-9._+-]*$ && ${#version} -le 160 ]] || {
  echo "invalid release version: $version" >&2
  exit 2
}
[[ "$version" != *-dirty ]] || { echo 'dirty release version' >&2; exit 2; }

dist=${PROTECTORATE_RELEASE_DIST:-dist}
binary_dir=${PROTECTORATE_RELEASE_BINARY_DIR:-target/release}
web_dir=${PROTECTORATE_RELEASE_WEB_DIR:-app/build/web}
[[ -d "$dist" && ! -L "$dist" ]] || { echo "missing dist directory: $dist" >&2; exit 1; }
[[ -f "$web_dir/index.html" ]] || { echo "missing Flutter web build: $web_dir" >&2; exit 1; }
[[ ! -e "$dist/VERSION" && ! -e "$dist/SHA256SUMS" ]] || {
  echo 'release metadata already exists in dist' >&2
  exit 1
}

components=(protectorate-rs protectorate-sleeve-runner protectorate-sleeve-sandbox-helper protectorate-node-sandbox-helper protectorate-cli protectorate-mcpbridge)
expected_assets=(VERSION)
for arch in amd64 arm64; do
  for component in "${components[@]}"; do
    name="$component-linux-$arch"
    expected_assets+=("$name")
    if [[ "$arch" == amd64 ]]; then
      [[ -f "$binary_dir/$component" && ! -L "$binary_dir/$component" ]] || { echo "missing build: $binary_dir/$component" >&2; exit 1; }
      [[ ! -e "$dist/$name" ]] || { echo "asset already exists: $dist/$name" >&2; exit 1; }
      cp -- "$binary_dir/$component" "$dist/$name"
    else
      [[ -s "$dist/$name" && ! -L "$dist/$name" ]] || { echo "missing artifact: $dist/$name" >&2; exit 1; }
    fi
  done
done
for arch in amd64 arm64; do
  for component in protectorate-rs protectorate-cli protectorate-mcpbridge; do
    name="$component-windows-$arch.exe"
    expected_assets+=("$name")
    [[ -s "$dist/$name" && ! -L "$dist/$name" ]] || { echo "missing artifact: $dist/$name" >&2; exit 1; }
  done
done
python3 - "$dist" <<'PY'
from pathlib import Path
import struct
import sys

dist = Path(sys.argv[1])
for arch, machine in (("amd64", 0x8664), ("arm64", 0xAA64)):
    for component in ("protectorate-rs", "protectorate-cli", "protectorate-mcpbridge"):
        path = dist / f"{component}-windows-{arch}.exe"
        with path.open("rb") as binary:
            header = binary.read(64)
            if len(header) != 64 or header[:2] != b"MZ":
                raise SystemExit(f"invalid Windows executable: {path}")
            pe_offset = struct.unpack_from("<I", header, 0x3C)[0]
            if pe_offset < 64 or pe_offset > 1_048_576:
                raise SystemExit(f"invalid PE header offset: {path}")
            binary.seek(pe_offset)
            signature_and_machine = binary.read(6)
            if signature_and_machine[:4] != b"PE\0\0" or len(signature_and_machine) != 6:
                raise SystemExit(f"invalid PE header: {path}")
            actual = struct.unpack_from("<H", signature_and_machine, 4)[0]
            if actual != machine:
                raise SystemExit(f"wrong Windows architecture for {path}: 0x{actual:04x}")
PY
android=protectorate-rs-android-arm64
expected_assets+=("$android")
[[ -s "$dist/$android" && ! -L "$dist/$android" ]] || { echo "missing Termux node artifact: $dist/$android" >&2; exit 1; }
desktop="protectorate-desktop-windows-x64-$version.zip"
expected_assets+=("$desktop")
[[ -s "$dist/$desktop" && ! -L "$dist/$desktop" ]] || { echo "missing Flutter desktop artifact: $dist/$desktop" >&2; exit 1; }

archive="protectorate-web-$version.tar.gz"
expected_assets+=("$archive")
[[ ! -e "$dist/$archive" ]] || { echo "asset already exists: $dist/$archive" >&2; exit 1; }
tar -czf "$dist/$archive" -C "$web_dir" .
printf '%s\n' "$version" > "$dist/VERSION"

shopt -s nullglob
assets=("$dist"/*)
(( ${#assets[@]} > 0 )) || { echo 'empty release bundle' >&2; exit 1; }
for asset in "${assets[@]}"; do
  [[ -f "$asset" && ! -L "$asset" ]] || { echo "invalid asset: $asset" >&2; exit 1; }
  name=${asset##*/}
  found=0
  for expected in "${expected_assets[@]}"; do
    [[ "$name" != "$expected" ]] || { found=1; break; }
  done
  (( found )) || { echo "unexpected release asset: $name" >&2; exit 1; }
done
(cd "$dist" && sha256sum -- * > SHA256SUMS && sha256sum --check SHA256SUMS)
