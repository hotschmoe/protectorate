//! The hub's per-sleeve transcript: normalized `pt_drivers::Event` lines. A `session_info`
//! entry switches the session and harness for the entries after it.

use pt_drivers::{Entry, Event, ToolStatus};

use super::item::{Item, State};
use super::text::tool_call;
use crate::doc::Role;
use crate::tail::clip;

pub fn line(bytes: &[u8], st: &mut State, out: &mut Vec<Item>) -> bool {
    let Ok(e) = pt_json::from_bytes::<Event>(bytes) else {
        return false;
    };
    st.time_ms = e.time_ms;
    match e.entry {
        Entry::SessionInfo {
            harness, native_id, ..
        } => {
            st.harness = Some(harness);
            st.session = clip(&native_id, 1024).to_owned();
        }
        Entry::UserMessage { text, .. } => st.push(out, Role::User, &text, None),
        Entry::AssistantText {
            text,
            r#final: true,
            ..
        } => st.push(out, Role::Assistant, &text, None),
        Entry::ToolCall {
            name,
            summary,
            input,
            ..
        } => {
            let (_, files) = input
                .as_ref()
                .map(|i| tool_call(&name, i))
                .unwrap_or_default();
            st.push_files(
                out,
                Role::ToolCall,
                &format!("{name} {summary}"),
                Some(&name),
                files,
            );
        }
        Entry::FileChange { paths, .. } => {
            let files: Vec<String> = paths.into_iter().map(|p| p.path).collect();
            st.push_files(
                out,
                Role::ToolCall,
                &files.join(" "),
                Some("file_change"),
                files,
            );
        }
        Entry::ToolResult {
            status: ToolStatus::Failed,
            output,
            ..
        } => st.push(out, Role::ToolError, &output, None),
        Entry::Error { text, .. } => st.push(out, Role::ToolError, &text, None),
        Entry::TurnSettled { usage, .. } => {
            if let Some(t) = usage.context_tokens {
                st.usage(out, t, usage.context_window);
            }
        }
        Entry::Compaction { .. } => st.compacted(out),
        _ => {}
    }
    true
}
