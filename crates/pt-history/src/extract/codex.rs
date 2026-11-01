//! Codex: `~/.codex/sessions/YYYY/MM/DD/rollout-<time>-<session>.jsonl`. Messages and tool
//! calls come from `response_item`, usage from `token_count` events.

use pt_json::Value;

use super::{Item, State, is_wrapped, text_of, tool_call};
use crate::Role;

pub fn line(v: &Value, st: &mut State, out: &mut Vec<Item>) -> bool {
    st.stamp(&v["timestamp"]);
    let p = &v["payload"];
    match v["type"].as_str() {
        Some("response_item") => item(p, st, out),
        Some("event_msg") => match p["type"].as_str() {
            Some("token_count") => {
                let info = &p["info"];
                if let Some(t) = info["last_token_usage"]["input_tokens"].as_u64() {
                    st.usage(out, t, info["model_context_window"].as_u64());
                }
                true
            }
            Some("context_compacted") => {
                st.compacted(out);
                true
            }
            Some(_) => true,
            None => false,
        },
        Some("compacted") => {
            st.compacted(out);
            if let Some(m) = p["message"].as_str() {
                st.push(out, Role::Compaction, m, None);
            }
            true
        }
        Some(_) => true,
        None => false,
    }
}

fn item(p: &Value, st: &mut State, out: &mut Vec<Item>) -> bool {
    match p["type"].as_str() {
        Some("message") => {
            let text = text_of(&p["content"]);
            match p["role"].as_str() {
                Some("user") if !is_wrapped(&text) => st.push(out, Role::User, &text, None),
                Some("assistant") => st.push(out, Role::Assistant, &text, None),
                _ => {}
            }
        }
        Some("function_call" | "custom_tool_call") => {
            let name = p["name"].as_str().unwrap_or("tool");
            let input = match &p["arguments"] {
                Value::Null => &p["input"],
                a => a,
            };
            let (text, files) = tool_call(name, input);
            st.push_files(out, Role::ToolCall, &text, Some(name), files);
        }
        Some("local_shell_call") => {
            let (text, files) = tool_call("shell", &p["action"]);
            st.push_files(out, Role::ToolCall, &text, Some("shell"), files);
        }
        Some("function_call_output" | "custom_tool_call_output") => {
            let o = &p["output"];
            let text = match o {
                Value::Object(_) => text_of(&o["content"]),
                o => text_of(o),
            };
            if failed(&text) {
                st.push(out, Role::ToolError, &text, None);
            }
        }
        Some(_) => {}
        None => return false,
    }
    true
}

fn failed(text: &str) -> bool {
    if text.starts_with("Script failed") {
        return true;
    }
    text.lines().take(8).any(|l| {
        l.strip_prefix("Process exited with code ")
            .is_some_and(|c| c.trim() != "0")
    })
}
