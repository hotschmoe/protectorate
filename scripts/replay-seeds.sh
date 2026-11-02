#!/bin/sh
# Replays every committed fuzz seed through the real fuzz targets, so a fixed crash or a seed
# that encodes an edge case stays checked by `make test`. Each crates/*/fuzz is its own
# workspace; its targets are built with plain cargo (libFuzzer's runtime only executes the
# files it is handed, it does not fuzz) and every seed of the crate goes through every target
# of the crate. A panic aborts the target and fails the run.
#
# The seed layout of each crate is declared below, so a missing, empty or stray seed directory
# (or a new fuzz crate nobody declared) fails loudly instead of replaying nothing.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
target=${CARGO_TARGET_DIR:-$root/target}/fuzz-seeds
mkdir -p "$target"
export CARGO_TARGET_DIR=$target

# crate = the entries of its seeds/ directory: "files" for plain seed files, "-" for none
# (those targets are byte-structured and have no hand-written seeds yet), else the sorted
# names of its seed subdirectories.
layout() {
    case "$1" in
        pt-api) echo "parse" ;;
        pt-core) echo "sleeve_core" ;;
        pt-drivers) echo "claude codex grok pi" ;;
        pt-embed) echo "inflate" ;;
        pt-frame) echo "decode reply request roundtrip" ;;
        pt-hash) echo "-" ;;
        pt-history) echo "extract_lines segment tail tokenize" ;;
        pt-http) echo "request ws_frame" ;;
        pt-journal) echo "-" ;;
        pt-json) echo "files" ;;
        pt-qr) echo "encode" ;;
        pt-quota) echo "claude codex grok local select" ;;
        pt-update) echo "sums tags" ;;
        *) return 1 ;;
    esac
}

fail() {
    echo "replay-seeds: $*" >&2
    exit 1
}

crates="pt-api pt-core pt-drivers pt-embed pt-frame pt-hash pt-history pt-http pt-journal pt-json pt-qr pt-quota pt-update"

for dir in crates/*/fuzz; do
    crate=$(basename "$(dirname "$dir")")
    case " $crates " in
        *" $crate "*) ;;
        *) fail "$dir is a fuzz crate that is not declared in scripts/replay-seeds.sh" ;;
    esac
done

total=0
for crate in $crates; do
    dir=crates/$crate/fuzz
    [ -d "$dir" ] || fail "$dir is declared but missing"
    want=$(layout "$crate") || fail "$crate has no declared seed layout"
    seeds=$dir/seeds

    case "$want" in
        -)
            [ ! -e "$seeds" ] || fail "$seeds exists but the layout declares no seeds for $crate"
            ;;
        files)
            [ -d "$seeds" ] || fail "$seeds is missing"
            [ -n "$(find "$seeds" -type f)" ] || fail "$seeds has no seeds"
            [ -z "$(find "$seeds" -mindepth 1 ! -type f)" ] || fail "$seeds holds a non-file entry"
            ;;
        *)
            [ -d "$seeds" ] || fail "$seeds is missing"
            have=$(LC_ALL=C ls "$seeds" | tr '\n' ' ' | sed 's/ $//')
            [ "$have" = "$want" ] || fail "$seeds holds [$have], expected [$want]"
            for sub in $want; do
                [ -d "$seeds/$sub" ] || fail "$seeds/$sub is not a directory"
                [ -n "$(find "$seeds/$sub" -type f)" ] || fail "$seeds/$sub has no seeds"
            done
            ;;
    esac

    bins=$(awk '/^\[/ { inbin = ($0 == "[[bin]]") } inbin && /^name *=/ { gsub(/.*= *"|".*/, ""); print }' "$dir/Cargo.toml")
    [ -n "$bins" ] || fail "$dir/Cargo.toml declares no fuzz target"
    PROTECTORATE_BUILD_VERSION=${PROTECTORATE_BUILD_VERSION:-dev} cargo build --locked --quiet --manifest-path "$dir/Cargo.toml" --bins

    [ "$want" != "-" ] || continue
    n=$(find "$seeds" -type f | wc -l)
    for bin in $bins; do
        [ -x "$target/debug/$bin" ] || fail "$crate: target $bin was not built"
        find "$seeds" -type f -print0 | LC_ALL=C sort -z |
            xargs -0 "$target/debug/$bin" -timeout=120 -artifact_prefix="$target/" >"$target/replay.log" 2>&1 ||
            { tail -40 "$target/replay.log" >&2; fail "$crate $bin failed on a seed (log tail above)"; }
        total=$((total + n))
    done
    echo "replay-seeds: $crate: $n seeds through $(echo "$bins" | wc -w) targets"
done
[ "$total" -gt 0 ] || fail "no seed was replayed"
echo "replay-seeds: ok ($total target runs)"
