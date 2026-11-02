#!/bin/sh
# The workspace's test binaries side by side instead of one after another, each from its
# package's dir as `cargo test` runs it, slowest first by the last run's times. A failing
# binary's output is printed; every log stays in <target>/test-logs.
#
#   sh scripts/test.sh            PT_TEST_JOBS (default half the cores) binaries at once
set -eu

jobs=${PT_TEST_JOBS:-$(( $(nproc) / 2 > 1 ? $(nproc) / 2 : 1 ))}
target=${CARGO_TARGET_DIR:-target}
logs=$target/test-logs
times=$target/test-times
mkdir -p "$logs"
rm -f "$logs"/*.log "$logs"/failed "$logs"/times.new
touch "$times" "$logs/failed" "$logs/times.new"

# Doctests are left out: the workspace has none, and building them costs a rustdoc per crate.
cargo test --locked --workspace --tests --no-run --message-format=json-render-diagnostics |
    grep '"reason":"compiler-artifact"' | grep '"profile":{[^}]*"test":true' |
    sed -n 's/.*"manifest_path":"\([^"]*\)".*"kind":\["\([^"]*\)".*"executable":"\([^"]*\)".*/\1 \2 \3/p' >"$logs/list"
[ -s "$logs/list" ] || { echo "test: no test binaries" >&2; exit 1; }

# A binary's key outlives its hash: the package dir, the target kind (a lib and a bin can share
# a name) and the test name. Slowest first, so the long ones never start last; one the last run
# did not time counts as slowest.
while read -r manifest kind exe; do
    key=$(basename "$(dirname "$manifest")")/$kind-$(basename "$exe" | sed 's/-[0-9a-f]*$//')
    t=$(awk -v k="$key" '$2 == k { print $1; exit }' "$times")
    echo "${t:-999999} $key $manifest $exe"
done <"$logs/list" | sort -rn | cut -d' ' -f2- >"$logs/order"

# shellcheck disable=SC2016
xargs -P "$jobs" -L 1 sh -c '
    logs=$0 key=$1 manifest=$2 exe=$3
    log=$logs/$(echo "$key" | tr / .).log
    start=$(date +%s)
    if (cd "$(dirname "$manifest")" && "$exe") >"$log" 2>&1; then
        echo "$(($(date +%s) - start)) $key" >>"$logs/times.new"
    else
        echo "$key" >>"$logs/failed"
    fi
' "$logs" <"$logs/order"
mv "$logs/times.new" "$times"

total=$(wc -l <"$logs/order")
passed=$(cat "$logs"/*.log | awk '/^test result:/ { s += $4 } END { print s + 0 }')
if [ -s "$logs/failed" ]; then
    while read -r key; do
        log=$logs/$(echo "$key" | tr / .).log
        echo "==== $key failed (log $log)"
        tail -n 60 "$log"
    done <"$logs/failed"
    echo "test: $(wc -l <"$logs/failed") of $total binaries failed; $passed tests passed" >&2
    exit 1
fi
echo "test: $total binaries, $passed tests passed"
