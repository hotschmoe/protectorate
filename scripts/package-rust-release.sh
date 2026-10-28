#!/usr/bin/env bash
set -euo pipefail

version=${1:?usage: package-rust-release.sh VERSION}
[[ "$version" =~ ^[A-Za-z0-9][A-Za-z0-9._+-]*$ && ${#version} -le 160 ]] || {
  echo "invalid release version: $version" >&2
  exit 2
}
[[ "$version" != *-dirty ]] || { echo 'dirty release version' >&2; exit 2; }

dist=${PROTECTORATE_RELEASE_DIST:-dist}
web_dir=${PROTECTORATE_RELEASE_WEB_DIR:-app/build/web}
[[ -d "$dist" && ! -L "$dist" ]] || { echo "missing dist directory: $dist" >&2; exit 1; }
[[ -f "$web_dir/index.html" ]] || { echo "missing Flutter web build: $web_dir" >&2; exit 1; }
[[ ! -e "$dist/VERSION" && ! -e "$dist/SHA256SUMS" ]] || {
  echo 'release metadata already exists in dist' >&2
  exit 1
}

components=(protectorate-rs protectorate-sleeve-runner protectorate-sleeve-sandbox-helper protectorate protectorate-mcpbridge)
expected_assets=(VERSION)
for arch in amd64 arm64; do
  for component in "${components[@]}"; do
    name="$component-linux-$arch"
    expected_assets+=("$name")
    [[ -s "$dist/$name" && ! -L "$dist/$name" ]] || { echo "missing artifact: $dist/$name" >&2; exit 1; }
  done
done
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
