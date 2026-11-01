#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

if ! command -v verus >/dev/null 2>&1 && [[ -f "$HOME/tools/verus-env.sh" ]]; then
    # shellcheck source=/dev/null
    source "$HOME/tools/verus-env.sh"
fi
if ! command -v verus >/dev/null 2>&1; then
    echo 'verify: Verus is not on PATH; source your Verus environment first' >&2
    exit 1
fi

pinned=0.2026.10.26.d17fbd0
version=$(verus --version | awk '$1 == "Version:" { print $2 }')
if [[ "$version" != "$pinned" ]]; then
    echo "verify: expected Verus $pinned, got ${version:-unknown}" >&2
    exit 1
fi

# The frame crate's message module depends on pt-json. Compile that dependency with the same
# toolchain as Verus so the verifier can type-check the real crate root and select its codec.
toolchain=$(verus --version | awk '$1 == "Toolchain:" { print $2 }')
target_dir=${CARGO_TARGET_DIR:-target}
mkdir -p "$target_dir/verus-deps"
rustup run "$toolchain" rustc --crate-name pt_json --crate-type lib --edition 2024 \
    --out-dir "$target_dir/verus-deps" crates/pt-json/src/lib.rs

# Verus provides its own vstd. The same feature is optional in Cargo and is off in normal builds.
# Journal I/O and frame messages are outside this proof run; see REVISIT.md for the remaining scope.
for limit in 20 10; do
    for module in codec crc32c; do
        echo "verify: pt-journal::$module, rlimit $limit"
        verus --crate-name pt_journal --crate-type lib --edition 2024 \
            --cfg 'feature="verus"' --verify-module "$module" --no-cheating \
            --rlimit "$limit" --time --triggers-mode silent crates/pt-journal/src/lib.rs
    done
    echo "verify: pt-frame::codec, rlimit $limit"
    verus --crate-name pt_frame --crate-type lib --edition 2024 \
        --cfg 'feature="verus"' --extern "pt_json=$target_dir/verus-deps/libpt_json.rlib" \
        --verify-module codec --no-cheating --rlimit "$limit" --time \
        --triggers-mode silent crates/pt-frame/src/lib.rs
done
