//! Every CLI scenario but chat_commands in one test binary, so the hub and
//! CLI link once and the scenarios run in parallel threads. chat_commands
//! keeps its own binary: it has its own main and doubles as the sandbox
//! helper the hub launches sleeves through.
mod common;

mod approvals;
mod audit_chain;
mod audit_query;
mod backlog_mutations;
mod completion;
mod devices_lifecycle;
mod harness_login_prefix;
mod hub_local_alias;
mod mailbox;
mod mcp_alias;
mod report_sequence;
mod session_attach;
mod session_lifecycle;
mod sleeve_status;
mod terminal_attach;
mod token_control;
