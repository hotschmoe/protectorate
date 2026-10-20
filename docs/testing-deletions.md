# Phase 2 deletion record

140 files deleted. The owner explicitly authorized `git rm` in the wave-2 dispatch. Executed `git rm -- <listed paths>` on 2026-10-20, completed by 18:43:26 +0800; archive/ and references/ were excluded. The same paths are recorded in the commit body.

## app/test: 15

- `app/test/add_remove_node_test.dart`
- `app/test/audit_test.dart`
- `app/test/chat_screens_test.dart`
- `app/test/chat_test.dart`
- `app/test/desk_test.dart`
- `app/test/messages_test.dart`
- `app/test/model_selection_test.dart`
- `app/test/nodes_test.dart`
- `app/test/notes_test.dart`
- `app/test/orchestrator_tab_test.dart`
- `app/test/pairing_test.dart`
- `app/test/start_work_test.dart`
- `app/test/version_skew_test.dart`
- `app/test/vfd_test.dart`
- `app/test/widget_test.dart`

## cmd/protectorate: 22

- `cmd/protectorate/backlog_test.go`
- `cmd/protectorate/chat_test.go`
- `cmd/protectorate/desk_policy_test.go`
- `cmd/protectorate/desk_test.go`
- `cmd/protectorate/diagnostics_e2e_test.go`
- `cmd/protectorate/e2e_environment_test.go`
- `cmd/protectorate/hub_backup_test.go`
- `cmd/protectorate/install_hub_test.go`
- `cmd/protectorate/job_test.go`
- `cmd/protectorate/main_test.go`
- `cmd/protectorate/mcp_test.go`
- `cmd/protectorate/msg_test.go`
- `cmd/protectorate/node_install_test.go`
- `cmd/protectorate/node_test.go`
- `cmd/protectorate/node_update_test.go`
- `cmd/protectorate/orchestrator_test.go`
- `cmd/protectorate/service_test.go`
- `cmd/protectorate/sleeve_test.go`
- `cmd/protectorate/tailnet_test.go`
- `cmd/protectorate/token_test.go`
- `cmd/protectorate/version_test.go`
- `cmd/protectorate/who_test.go`

## internal/auth: 2

- `internal/auth/audit_test.go`
- `internal/auth/auth_test.go`

## internal/backlog: 1

- `internal/backlog/backlog_test.go`

## internal/backup: 1

- `internal/backup/backup_test.go`

## internal/chatlog: 1

- `internal/chatlog/chatlog_test.go`

## internal/desk: 3

- `internal/desk/desk_test.go`
- `internal/desk/input_type_test.go`
- `internal/desk/policy_test.go`

## internal/directory: 2

- `internal/directory/directory_test.go`
- `internal/directory/refs_test.go`

## internal/fsutil: 1

- `internal/fsutil/fsutil_test.go`

## internal/handoff: 5

- `internal/handoff/handoff_test.go`
- `internal/handoff/testdata/claude.jsonl`
- `internal/handoff/testdata/codex.jsonl`
- `internal/handoff/testdata/grok.jsonl`
- `internal/handoff/testdata/pi.jsonl`

## internal/hub: 32

- `internal/hub/auth_test.go`
- `internal/hub/backlog_test.go`
- `internal/hub/deps_test.go`
- `internal/hub/deskpolicy_test.go`
- `internal/hub/deskuse_test.go`
- `internal/hub/envoy_test.go`
- `internal/hub/gpubusy_test.go`
- `internal/hub/grants_test.go`
- `internal/hub/hub_test.go`
- `internal/hub/identity_test.go`
- `internal/hub/install_test.go`
- `internal/hub/invite_test.go`
- `internal/hub/jobkeys_test.go`
- `internal/hub/jobs_test.go`
- `internal/hub/jobsource_test.go`
- `internal/hub/lease_queue_test.go`
- `internal/hub/leases_test.go`
- `internal/hub/localmodel_test.go`
- `internal/hub/nodeops_test.go`
- `internal/hub/nodes_test.go`
- `internal/hub/nodeupdate_test.go`
- `internal/hub/pair_test.go`
- `internal/hub/quota_test.go`
- `internal/hub/resleeve_test.go`
- `internal/hub/selection_test.go`
- `internal/hub/sessions_test.go`
- `internal/hub/sleeves_test.go`
- `internal/hub/suggest_test.go`
- `internal/hub/tailnet_test.go`
- `internal/hub/testdata/install.ps1.golden`
- `internal/hub/testdata/install.sh.golden`
- `internal/hub/workspaces_test.go`

## internal/jobtest: 1

- `internal/jobtest/jobtest.go`

## internal/keylog: 1

- `internal/keylog/keylog_test.go`

## internal/mailbox: 1

- `internal/mailbox/mailbox_test.go`

## internal/mcpbridge: 2

- `internal/mcpbridge/mcpbridge_test.go`
- `internal/mcpbridge/testdata/tools.golden.json`

## internal/node: 16

- `internal/node/accel_live_test.go`
- `internal/node/accel_test.go`
- `internal/node/accel_windows_test.go`
- `internal/node/deskguard_test.go`
- `internal/node/files_test.go`
- `internal/node/jobs_test.go`
- `internal/node/node_test.go`
- `internal/node/process_unix_test.go`
- `internal/node/runner_test.go`
- `internal/node/sandbox_unix_test.go`
- `internal/node/testdata/pci.ids`
- `internal/node/testdata/sysfs/b70-nas-server.txt`
- `internal/node/testdata/sysfs/intel-mixed.txt`
- `internal/node/testdata/sysfs/kidsbox.txt`
- `internal/node/testdata/sysfs/ms-r1.txt`
- `internal/node/update_test.go`

## internal/orchestrator: 2

- `internal/orchestrator/orchestrator_test.go`
- `internal/orchestrator/testdata/AGENTS.golden.md`

## internal/quota: 1

- `internal/quota/quota_test.go`

## internal/session: 2

- `internal/session/dbg_test.go`
- `internal/session/session_test.go`

## internal/sleeve: 14

- `internal/sleeve/layout_test.go`
- `internal/sleeve/login_test.go`
- `internal/sleeve/mode_test.go`
- `internal/sleeve/plan_test.go`
- `internal/sleeve/rename_test.go`
- `internal/sleeve/selection_test.go`
- `internal/sleeve/testdata/claude.argv`
- `internal/sleeve/testdata/codex.argv`
- `internal/sleeve/testdata/envoy.argv`
- `internal/sleeve/testdata/grok.argv`
- `internal/sleeve/testdata/pi.argv`
- `internal/sleeve/testdata/shell-interactive.argv`
- `internal/sleeve/testdata/shell.argv`
- `internal/sleeve/workspaces_test.go`

## internal/svcgen: 11

- `internal/svcgen/e2e_environment_test.go`
- `internal/svcgen/svcgen_test.go`
- `internal/svcgen/testdata/hub-system.service`
- `internal/svcgen/testdata/hub-user.service`
- `internal/svcgen/testdata/node-boot-home.xml`
- `internal/svcgen/testdata/node-logon-elevated.xml`
- `internal/svcgen/testdata/node-logon.xml`
- `internal/svcgen/testdata/node-service.cmd`
- `internal/svcgen/testdata/node-user.service`
- `internal/svcgen/testdata/termux-boot`
- `internal/svcgen/testdata/termux-run`

## internal/tailnet: 2

- `internal/tailnet/status_test.go`
- `internal/tailnet/tailnet_test.go`

## internal/xfer: 1

- `internal/xfer/xfer_test.go`

## scripts: 1

- `scripts/e2e-scopes.py`
