//! Every CLI scenario but chat_commands in one test binary, so the hub and
//! CLI link once and the scenarios run in parallel threads. chat_commands
//! keeps its own binary: it has its own main and doubles as the sandbox
//! helper the hub launches sleeves through.
mod common;

mod approvals;
mod audit_chain;
mod backlog_mutations;
mod completion;
mod device_login;
mod devices_lifecycle;
mod grant_audit;
mod harness_login_prefix;
mod hub_local_alias;
mod job_run;
mod job_test_cross;
mod mailbox;
mod mcp_alias;
mod node_doctor_alias;
mod node_install_alias;
mod node_join_alias;
mod node_lease_audit;
mod node_local_metrics;
mod node_serve_alias;
mod node_transfer;
mod node_uninstall_alias;
mod report_sequence;
mod service_generation;
mod session_attach;
mod session_lifecycle;
mod sleeve_status;
mod terminal_attach;
mod token_control;
