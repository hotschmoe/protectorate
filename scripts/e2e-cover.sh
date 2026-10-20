#!/bin/sh
set -eu

mkdir -p bin/e2e-cover
run_dir=$(mktemp -d "$(pwd)/bin/e2e-cover/run.XXXXXX")
mkdir "$run_dir/data"
"${MAKE:-make}" --no-print-directory e2e-cover-build E2E_COVER_BINARY="$run_dir/protectorate"
status=0
env -u PROTECTORATE_TOKEN_FILE -u PROTECTORATE_HUB -u PROTECTORATE_TOKEN \
    PROT_E2E=1 PROT_E2E_BINARY="$run_dir/protectorate" GOCOVERDIR="$run_dir/data" \
    go test -count=1 -tags "$TAILSCALE_TAGS" ./tests/e2e >"$run_dir/tests.log" 2>&1 || status=$?
cat "$run_dir/tests.log"
go tool covdata percent -i="$run_dir/data" | tee "$run_dir/packages.txt"
go tool covdata textfmt -i="$run_dir/data" -o="$run_dir/coverage.out"
go tool cover -func="$run_dir/coverage.out" >"$run_dir/functions.txt"
printf 'Coverage artifacts: %s\n' "$run_dir"
exit "$status"
