#!/bin/sh
# One local-model endpoint serves the model tests, so they run serially.
set -eu

jobs=${PT_TEST_JOBS:-$(( $(nproc) / 2 > 1 ? $(nproc) / 2 : 1 ))}
target=${CARGO_TARGET_DIR:-target}
logs=$target/test-logs
times=$target/test-times
mkdir -p "$logs"
rm -f "$logs"/*.log "$logs"/failed "$logs"/times.new
touch "$times" "$logs/failed" "$logs/times.new"
rm -rf --one-file-system "$target/tmp"

# Doctests are left out: the workspace has none, and building them costs a rustdoc per crate.
PROTECTORATE_BUILD_VERSION=${PROTECTORATE_BUILD_VERSION:-dev} cargo test --locked --workspace \
    --features ssh-poc --tests --no-run --message-format=json-render-diagnostics |
    grep '"reason":"compiler-artifact"' | grep '"profile":{[^}]*"test":true' |
    sed -n 's/.*"manifest_path":"\([^"]*\)".*"kind":\["\([^"]*\)".*"executable":"\([^"]*\)".*/\1 \2 \3/p' >"$logs/list"
[ -s "$logs/list" ] || { echo "test: no test binaries" >&2; exit 1; }

# The package, target kind and name identify a binary across rebuilds.
while read -r manifest kind exe; do
    key=$(basename "$(dirname "$manifest")")/$kind-$(basename "$exe" | sed 's/-[0-9a-f]*$//')
    t=$(awk -v k="$key" '$2 == k { print $1; exit }' "$times")
    echo "${t:-999999} $key $manifest $exe"
done <"$logs/list" | sort -rn | cut -d' ' -f2- >"$logs/order"

: >"$logs/model"
: >"$logs/rest"
while read -r key manifest exe; do
    src=$(dirname "$manifest")/tests/$(basename "$exe" | sed 's/-[0-9a-f]*$//').rs
    if [ -f "$src" ] && grep -qE 'have_runtime\(|PT_E2E_PI_' "$src"; then
        echo "$key $manifest $exe" >>"$logs/model"
    else
        echo "$key $manifest $exe" >>"$logs/rest"
    fi
done <"$logs/order"

# shellcheck disable=SC2016
run='
    logs=$0 key=$1 manifest=$2 exe=$3
    log=$logs/$(echo "$key" | tr / .).log
    start=$(date +%s)
    if (cd "$(dirname "$manifest")" && "$exe") >"$log" 2>&1; then
        echo "$(($(date +%s) - start)) $key" >>"$logs/times.new"
    else
        echo "$key" >>"$logs/failed"
    fi
'
xargs -L 1 sh -c "$run" "$logs" <"$logs/model" &
lane=$!
xargs -P "$(( jobs > 2 ? jobs - 1 : 1 ))" -L 1 sh -c "$run" "$logs" <"$logs/rest"
wait "$lane"
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
