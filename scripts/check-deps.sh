#!/bin/sh
# Fails when Cargo.lock holds a registry crate that DEPENDENCIES.md does not list at that version.
set -eu
cd "$(dirname "$0")/.."
[ -f Cargo.lock ] || { echo "check-deps: no Cargo.lock"; exit 1; }
missing=$(awk '
  /^\[\[package\]\]/ { if (name != "" && src != "") print name, ver; name=""; ver=""; src="" }
  /^name = /    { gsub(/"/, "", $3); name=$3 }
  /^version = / { gsub(/"/, "", $3); ver=$3 }
  /^source = /  { src=$3 }
  END { if (name != "" && src != "") print name, ver }
' Cargo.lock | sort -u | while read -r name ver; do
  grep -Eq "^\| ${name} \| ${ver} \|" DEPENDENCIES.md || echo "  ${name} ${ver}"
done)
if [ -n "$missing" ]; then
  echo "check-deps: registry crates not listed in DEPENDENCIES.md:"
  echo "$missing"
  exit 1
fi
echo "check-deps: ok"
