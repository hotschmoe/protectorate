//! Grok: `~/.grok/sessions/<cwd>/<session>/chat_history.jsonl`. Records carry no time; the
//! index stamps the session's start (`summary.json`) before the first line. A real prompt has
//! `prompt_index`; other user records are injected context.

use pt_json::Value;

use super::{Item, State, text_of, tool_call};
use crate::Role;

pub fn line(v: &Value, st: &mut State, out: &mut Vec<Item>) -> bool {
    match v["type"].as_str() {
        Some("user") => {
            if !v["prompt_index"].is_null() {
                let text = text_of(&v["content"]);
                let t = text.trim();
                let t = t.strip_prefix("<user_query>").unwrap_or(t);
                let t = t.strip_suffix("</user_query>").unwrap_or(t);
                st.push(out, Role::User, t, None);
            }
        }
        Some("assistant") => {
            st.push(out, Role::Assistant, &text_of(&v["content"]), None);
            for c in v["tool_calls"].as_array().unwrap_or_default() {
                let name = c["name"].as_str().unwrap_or("tool");
                let (text, files) = tool_call(name, &c["arguments"]);
                st.push_files(out, Role::ToolCall, &text, Some(name), files);
            }
        }
        Some(_) => {}
        None => return false,
    }
    true
}
