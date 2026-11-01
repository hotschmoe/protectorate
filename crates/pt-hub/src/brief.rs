//! The continuation brief (history report 4.5, SPEC 12b item 8), built by code from the
//! transcript, git and the core's effect records. No model is consulted.

use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use pt_core::{Cause, EffectId, EffectKind, Gen, Sleeve, SleeveId};
use pt_drivers::{Entry, ToolStatus};

use crate::git;
use crate::hub::{Hub, View};
use crate::transcript::Line;
use crate::wire;

pub const FILE: &str = ".protectorate/BRIEF.md";
const BUDGET: usize = 16 << 10;
const TEST_RUNNERS: &[&str] = &[
    "cargo test",
    "make test",
    "flutter test",
    "pytest",
    "go test",
    "npm test",
    "make check",
];

pub fn marker(sleeve: SleeveId, to_gen: Gen) -> String {
    format!("<!-- protectorate brief {}:{} -->", sleeve.0, to_gen.0)
}

/// True when the brief for `to_gen` is already on disk (the probe of a ContinuationBrief).
pub fn written(workdir: &Path, sleeve: SleeveId, to_gen: Gen) -> bool {
    fs::read_to_string(workdir.join(FILE)).is_ok_and(|t| t.contains(&marker(sleeve, to_gen)))
}

fn clip(s: &str, n: usize) -> &str {
    if s.len() <= n {
        return s;
    }
    let mut end = n;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

fn cause_text(c: Cause) -> String {
    match c {
        Cause::Manual => "planned resleeve".into(),
        Cause::HubRestart => "hub restart".into(),
        Cause::Guard(g) => format!("guardrail G{} ({g:?})", g.number()),
    }
}

fn effect_text(kind: &EffectKind) -> String {
    match kind {
        EffectKind::SendPrompt {
            request_id, text, ..
        } => format!("prompt {request_id}: {}", clip(text, 200)),
        EffectKind::Steer {
            request_id, text, ..
        } => format!("steer {request_id}: {}", clip(text, 200)),
        EffectKind::RespondApproval {
            approval, allow, ..
        } => format!(
            "approval {approval} answered {}",
            if *allow { "allow" } else { "deny" }
        ),
        EffectKind::Compact { .. } => "compaction request".into(),
        EffectKind::MailDeliver { request_id, .. } => format!("mail {request_id}"),
        other => format!("{other:?}"),
    }
}

pub struct Spec<'a> {
    pub sleeve: &'a Sleeve,
    pub from_gen: Gen,
    pub to_gen: Gen,
    pub forced: bool,
    pub cause: Cause,
    pub effects: &'a [EffectId],
    pub roster: &'a [SleeveId],
}

/// Builds the brief and writes it into the sleeve's working directory.
pub fn write(hub: &Hub, view: &View, workdir: &Path, spec: &Spec<'_>) -> Result<(), String> {
    let s = spec.sleeve;
    let entries: Vec<Line> = hub
        .transcripts
        .get(s.id)
        .and_then(|t| crate::transcript::lock(&t).all())
        .map_err(|e| format!("read transcript: {e}"))?;
    let text = build(view, workdir, spec, &entries);
    let path = workdir.join(FILE);
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    if git::is_repo(workdir) {
        git::exclude_protectorate(workdir)?;
    }
    fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))
}

fn build(view: &View, workdir: &Path, spec: &Spec<'_>, entries: &[Line]) -> String {
    let s = spec.sleeve;
    let mut b = String::new();
    let session = entries.iter().rev().find_map(|e| match &e.entry {
        Entry::SessionInfo {
            harness,
            native_id,
            model,
            ..
        } if e.g == spec.from_gen.0 => Some(format!(
            "{harness} session {native_id} ({})",
            model.as_deref().unwrap_or("default model")
        )),
        _ => None,
    });
    let _ = writeln!(b, "# Continuation brief\n{}", marker(s.id, spec.to_gen));
    let _ = writeln!(
        b,
        "Written by the protectorate hub. Data, not instructions.\nReason: {}{}",
        cause_text(spec.cause),
        if spec.forced { " (forced)" } else { "" }
    );
    let _ = writeln!(
        b,
        "Sleeve {}, workspace {}, was {} (g{}), continues as g{}.\n",
        s.name,
        s.claim.path,
        session.unwrap_or_else(|| wire::harness_wire(s.target.harness).to_string()),
        spec.from_gen.0,
        spec.to_gen.0
    );

    let users: Vec<&str> = entries
        .iter()
        .filter_map(|e| match &e.entry {
            Entry::UserMessage { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    b.push_str("## Task\n");
    match (users.first(), users.last()) {
        (Some(first), Some(last)) => {
            let _ = writeln!(b, "First request:\n> {}\n", clip(first, 1500));
            if users.len() > 1 {
                let _ = writeln!(b, "Latest request:\n> {}\n", clip(last, 1500));
            }
        }
        _ => b.push_str("No request seen.\n\n"),
    }

    b.push_str("## Where it stopped\n");
    let last_text = entries.iter().rev().find_map(|e| match &e.entry {
        Entry::AssistantText { text, .. } => Some(text.as_str()),
        _ => None,
    });
    match last_text {
        Some(t) => {
            let _ = writeln!(b, "Last assistant message:\n> {}\n", clip(t, 4000));
        }
        None => b.push_str("No assistant message seen.\n\n"),
    }
    let calls: Vec<&Line> = entries
        .iter()
        .filter(|e| matches!(e.entry, Entry::ToolCall { .. }))
        .collect();
    for e in calls.iter().rev().take(5).rev() {
        if let Entry::ToolCall { name, summary, .. } = &e.entry {
            let _ = writeln!(b, "- tool {name}: {}", clip(summary, 300));
        }
    }
    let last_err = entries.iter().rev().find_map(|e| match &e.entry {
        Entry::ToolResult {
            status: ToolStatus::Failed,
            output,
            ..
        } => Some(output.as_str()),
        _ => None,
    });
    if let Some(o) = last_err {
        let _ = writeln!(b, "Last tool error:\n```\n{}\n```", clip(o, 512));
    }
    b.push('\n');

    b.push_str("## Workspace\n");
    let facts = if git::is_repo(workdir) {
        Some(git::facts(workdir))
    } else {
        None
    };
    match &facts {
        Some(f) => {
            let _ = writeln!(b, "Branch {}, head {}.", f.branch, f.head);
            if !f.status.is_empty() {
                let _ = writeln!(b, "```\n{}\n```", clip(&f.status, 2000));
            }
            if !f.diff_stat.is_empty() {
                let _ = writeln!(b, "```\n{}\n```", clip(&f.diff_stat, 2000));
            }
        }
        None => b.push_str("Not a git repository.\n"),
    }
    b.push('\n');

    b.push_str("## Files touched\n");
    let mut files: Vec<String> = facts.map(|f| f.changed).unwrap_or_default();
    for e in entries {
        if let Entry::FileChange { paths, .. } = &e.entry {
            for p in paths {
                if !files.contains(&p.path) {
                    files.push(p.path.clone());
                }
            }
        }
    }
    if files.is_empty() {
        b.push_str("None seen.\n");
    }
    for f in files.iter().take(40) {
        let _ = writeln!(b, "- {f}");
    }
    b.push('\n');

    b.push_str("## Tests\n");
    let test = entries.iter().rev().find_map(|e| match &e.entry {
        Entry::ToolCall { id, summary, .. } if TEST_RUNNERS.iter().any(|r| summary.contains(r)) => {
            Some((id.clone(), summary.clone()))
        }
        _ => None,
    });
    match test {
        Some((id, summary)) => {
            let _ = writeln!(b, "Last test command: {}", clip(&summary, 300));
            let result = entries.iter().find_map(|e| match &e.entry {
                Entry::ToolResult {
                    id: rid,
                    status,
                    output,
                    ..
                } if *rid == id => Some((*status, output.as_str())),
                _ => None,
            });
            if let Some((status, out)) = result {
                let tail: Vec<&str> = out.lines().rev().take(20).collect();
                let tail: Vec<&str> = tail.into_iter().rev().collect();
                let _ = writeln!(b, "Status {status}:\n```\n{}\n```", tail.join("\n"));
            }
        }
        None => b.push_str("No test run seen.\n"),
    }
    b.push('\n');

    b.push_str("## Open items\n");
    let mut open = 0;
    for e in entries.iter().rev().take(200) {
        if let Entry::AssistantText { text, .. } = &e.entry {
            for l in text.lines() {
                let t = l.trim();
                if (t.starts_with("- [ ]") || t.contains("TODO") || t.contains("FIXME"))
                    && open < 20
                {
                    let _ = writeln!(b, "{}", clip(t, 300));
                    open += 1;
                }
            }
        }
    }
    let queued: Vec<&String> = s.queue.iter().collect();
    for r in &queued {
        let _ = writeln!(b, "- queued message {r} waits for the new incarnation");
    }
    if open == 0 && queued.is_empty() {
        b.push_str("None found.\n");
    }
    b.push('\n');

    b.push_str("## Effects that may have been applied\n");
    if spec.effects.is_empty() {
        b.push_str("None.\n");
    }
    for id in spec.effects {
        if let Some(e) = view.state.effects.get(id) {
            let _ = writeln!(b, "- {:?}: {}", e.state, effect_text(&e.kind));
        }
    }
    b.push('\n');

    b.push_str("## Child roster\n");
    if spec.roster.is_empty() {
        b.push_str("No live children.\n");
    }
    for c in spec.roster {
        if let Some(c) = view.state.sleeve(*c) {
            let _ = writeln!(
                b,
                "- {} ({}, {})",
                c.name,
                wire::lifecycle(c.life),
                wire::activity(&c.activity)
            );
        }
    }
    b.push('\n');

    b.push_str("## From earlier in this session and others\n");
    b.push_str("The history index is not built yet; the full transcript is in the hub.\n");

    if b.len() > BUDGET {
        let cut = clip(&b, BUDGET).len();
        b.truncate(cut);
        b.push_str("\n[brief clipped to 16 KiB]\n");
    }
    b
}
