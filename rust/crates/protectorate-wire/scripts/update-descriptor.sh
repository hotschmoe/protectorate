#!/bin/sh
set -eu
repo=$(CDPATH='' cd -- "$(dirname -- "$0")/../../../.." && pwd)
cd "$repo"
asset=rust/crates/protectorate-wire/assets/protectorate.binpb
source_asset=rust/crates/protectorate-wire/assets/protectorate-source.binpb
if [ "${1-}" = --check ]; then
  descriptor_tmp=$(mktemp -d)
  trap 'rm -rf "$descriptor_tmp"' EXIT HUP INT TERM
  buf build --as-file-descriptor-set --exclude-source-info -o "$descriptor_tmp/protectorate.binpb"
  buf build --as-file-descriptor-set -o "$descriptor_tmp/protectorate-source.binpb"
  cmp "$asset" "$descriptor_tmp/protectorate.binpb"
  cmp "$source_asset" "$descriptor_tmp/protectorate-source.binpb"
else
  buf build --as-file-descriptor-set --exclude-source-info -o "$asset"
  buf build --as-file-descriptor-set -o "$source_asset"
fi
