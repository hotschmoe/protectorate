#!/bin/sh
set -eu

target=${CARGO_TARGET_DIR:-target}
rm -rf --one-file-system "$target/tmp"

PROTECTORATE_BUILD_VERSION=${PROTECTORATE_BUILD_VERSION:-dev} cargo build --locked --workspace

run() {
    PROTECTORATE_BUILD_VERSION=${PROTECTORATE_BUILD_VERSION:-dev} cargo test --locked --workspace --test "$1" "$2" -- --exact
}

run hub upward_mail
run hub d9_spawn_limit_holds_across_a_hub_restart
run fleet hub_runs_the_fleet_and_node_jobs
run update node_update_admission_lost_reply_and_confirmation
run sandbox masks_hide_credentials_and_filesystem_is_read_only_outside_workspace
run sandbox worktree_hooks_are_read_only
run api api_surface
