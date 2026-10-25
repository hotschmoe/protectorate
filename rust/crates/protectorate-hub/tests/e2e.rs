//! Every hub scenario in one test binary, so the hub links once and the
//! scenarios run in parallel threads.
mod support {
    pub mod capability;
    pub mod chatlog;
    pub mod executable;
    pub mod fleet;
    #[cfg(unix)]
    pub mod private_tmux;
}

mod audit;
mod audit_paging;
mod backup_restore;
mod branch_dispatch;
mod chat_claude;
mod chat_codex;
mod chat_grok;
mod chat_pi;
mod directory_scan;
mod distribution;
mod download;
mod governance;
mod grants;
mod hub;
mod jobs;
mod lease;
mod lease_selection;
mod mail;
mod mail_chat;
mod mail_owner;
mod nested_sleeve_workload;
mod node_files;
mod node_health;
mod node_inventory;
mod node_sequence;
mod node_uninstall_order;
mod node_update;
mod node_update_refusals;
mod nodes;
mod release;
mod remove_node;
mod session_events;
mod sleeve_caches;
mod sleeve_envoy;
mod sleeve_prune;
mod sleeve_resleeve;
mod sleeve_scopes;
mod terminal_activity;
mod terminal_sleeve;
mod terminal_ws;
mod wire_streams;
mod workspace_listing;
mod workspaces;
