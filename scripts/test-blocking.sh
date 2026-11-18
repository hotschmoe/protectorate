#!/bin/sh
set -eu

PROTECTORATE_BUILD_VERSION=${PROTECTORATE_BUILD_VERSION:-dev} cargo build --locked --workspace

run() {
    PROTECTORATE_BUILD_VERSION=${PROTECTORATE_BUILD_VERSION:-dev} cargo test --locked -p "$1" --test "$2" "$3" -- --exact
}

run protectorate hub upward_mail
run protectorate hub d9_spawn_limit_holds_across_a_hub_restart
run protectorate fleet hub_runs_the_fleet_and_node_jobs
run protectorate update node_update_admission_lost_reply_and_confirmation
run pt-sandbox sandbox masks_hide_credentials_and_filesystem_is_read_only_outside_workspace
run protectorate api api_surface
