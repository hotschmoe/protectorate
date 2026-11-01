//! Pi: `~/.pi/agent/sessions/<cwd>/<time>_<session>.jsonl`, a tree of entries whose `message`
//! records carry user, assistant and tool result messages.

use pt_json::Value;

use super::{Item, State, text_of, tool_call};
use crate::Role;

pub fn line(v: &Value, st: &mut State, out: &mut Vec<Item>) -> bool {
    st.stamp(&v["timestamp"]);
    match v["type"].as_str() {
        Some("message") => message(&v["message"], st, out),
        Some("compaction") => {
            st.compacted(out);
            if let Some(s) = v["summary"].as_str() {
                st.push(out, Role::Compaction, s, None);
            }
            true
        }
        Some(_) => true,
        None => false,
    }
}

fn message(m: &Value, st: &mut State, out: &mut Vec<Item>) -> bool {
    match m["role"].as_str() {
        Some("user") => st.push(out, Role::User, &text_of(&m["content"]), None),
        Some("assistant") => {
            let u = &m["usage"];
            let tokens = ["input", "cacheRead", "cacheWrite"]
                .iter()
                .filter_map(|k| u[*k].as_u64())
                .sum();
            st.usage(out, tokens, None);
            for b in m["content"].as_array().unwrap_or_default() {
                match b["type"].as_str() {
                    Some("text") => {
                        if let Some(t) = b["text"].as_str() {
                            st.push(out, Role::Assistant, t, None);
                        }
                    }
                    Some("toolCall") => {
                        let name = b["name"].as_str().unwrap_or("tool");
                        let (text, files) = tool_call(name, &b["arguments"]);
                        st.push_files(out, Role::ToolCall, &text, Some(name), files);
                    }
                    _ => {}
                }
            }
        }
        Some("toolResult") => {
            if m["isError"].as_bool() == Some(true) {
                let tool = m["toolName"].as_str();
                st.push(out, Role::ToolError, &text_of(&m["content"]), tool);
            }
        }
        Some(_) => {}
        None => return false,
    }
    true
}
