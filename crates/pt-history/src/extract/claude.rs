//! Claude Code: `~/.claude/projects/<cwd>/<session>.jsonl`. Sidechain (subagent) and meta
//! records are skipped; usage is `input + cache_creation + cache_read` of each model call.

use pt_json::Value;

use super::{Item, State, is_wrapped, text_of, tool_call};
use crate::Role;

pub fn line(v: &Value, st: &mut State, out: &mut Vec<Item>) -> bool {
    let Some(kind) = v["type"].as_str() else {
        return false;
    };
    if !matches!(kind, "user" | "assistant" | "system") {
        return true;
    }
    if v["isSidechain"].as_bool() == Some(true) || v["isMeta"].as_bool() == Some(true) {
        return true;
    }
    st.stamp(&v["timestamp"]);
    match kind {
        "user" => user(v, st, out),
        "assistant" => assistant(v, st, out),
        _ => {
            if v["subtype"].as_str() == Some("compact_boundary") {
                st.compacted(out);
                if let Some(t) = v["compactMetadata"]["postTokens"].as_u64() {
                    st.usage(out, t, None);
                }
            }
        }
    }
    true
}

fn user(v: &Value, st: &mut State, out: &mut Vec<Item>) {
    let content = &v["message"]["content"];
    if v["isCompactSummary"].as_bool() == Some(true) {
        st.push(out, Role::Compaction, &text_of(content), None);
        return;
    }
    let mut prompt = String::new();
    match content {
        Value::String(s) if !is_wrapped(s) => prompt.push_str(s),
        Value::Array(blocks) => {
            for b in blocks {
                match b["type"].as_str() {
                    Some("text") => {
                        if let Some(t) = b["text"].as_str().filter(|t| !is_wrapped(t)) {
                            if !prompt.is_empty() {
                                prompt.push('\n');
                            }
                            prompt.push_str(t);
                        }
                    }
                    Some("tool_result") if b["is_error"].as_bool() == Some(true) => {
                        st.push(out, Role::ToolError, &text_of(&b["content"]), None);
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }
    st.push(out, Role::User, &prompt, None);
}

fn assistant(v: &Value, st: &mut State, out: &mut Vec<Item>) {
    let m = &v["message"];
    let u = &m["usage"];
    let tokens = [
        "input_tokens",
        "cache_creation_input_tokens",
        "cache_read_input_tokens",
    ]
    .iter()
    .filter_map(|k| u[*k].as_u64())
    .try_fold(0u64, u64::checked_add);
    if let Some(tokens) = tokens {
        st.usage(out, tokens, None);
    }
    let Some(blocks) = m["content"].as_array() else {
        return;
    };
    for b in blocks {
        match b["type"].as_str() {
            Some("text") => {
                if let Some(t) = b["text"].as_str() {
                    st.push(out, Role::Assistant, t, None);
                }
            }
            Some("tool_use") => {
                let name = b["name"].as_str().unwrap_or("tool");
                let (text, files) = tool_call(name, &b["input"]);
                st.push_files(out, Role::ToolCall, &text, Some(name), files);
            }
            _ => {}
        }
    }
}
