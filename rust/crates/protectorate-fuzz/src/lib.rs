//! Fuzz targets: each takes arbitrary bytes, must not panic, and asserts the
//! invariants its parser promises. rust/fuzz drives them under libFuzzer and
//! tests/seeds.rs replays every committed corpus file on stable.
#![cfg(target_os = "linux")]

mod archives;
mod auth;
mod cli;
mod egress;
mod harness;
mod quota;
mod sandbox;
mod stores;
mod terminal;
mod wire;

pub use archives::{backup_archive, release_manifest, release_web_archive};
pub use auth::{capability_tokens, device_tokens};
pub use cli::{cli_args, mcp_stdio};
pub use egress::egress_connect;
pub use harness::{claude_stream, codex_rpc, grok_acp, pi_rpc};
pub use quota::quota_responses;
pub use sandbox::sandbox_plan;
pub use stores::{
    audit_log, backlog_notes, chatlog, directory_state, handoff_transcript, identity_store,
    mailbox_log,
};
pub use terminal::terminal_pane;
pub use wire::wire_json;

pub type Target = fn(&[u8]);

/// Every target by its corpus directory name under rust/fuzz/corpus.
pub const TARGETS: &[(&str, Target)] = &[
    ("audit_log", audit_log),
    ("backlog_notes", backlog_notes),
    ("backup_archive", backup_archive),
    ("capability_tokens", capability_tokens),
    ("chatlog", chatlog),
    ("claude_stream", claude_stream),
    ("cli_args", cli_args),
    ("codex_rpc", codex_rpc),
    ("device_tokens", device_tokens),
    ("directory_state", directory_state),
    ("egress_connect", egress_connect),
    ("grok_acp", grok_acp),
    ("handoff_transcript", handoff_transcript),
    ("identity_store", identity_store),
    ("mailbox_log", mailbox_log),
    ("mcp_stdio", mcp_stdio),
    ("pi_rpc", pi_rpc),
    ("quota_responses", quota_responses),
    ("release_manifest", release_manifest),
    ("release_web_archive", release_web_archive),
    ("sandbox_plan", sandbox_plan),
    ("terminal_pane", terminal_pane),
    ("wire_json", wire_json),
];

/// Splits an input into NUL-separated fields, padding missing ones with "".
fn fields(data: &[u8], count: usize) -> Vec<&[u8]> {
    let mut out: Vec<&[u8]> = data.splitn(count, |&b| b == 0).collect();
    out.resize(count, b"");
    out
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}
