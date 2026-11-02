//! `protectorate <noun> <verb>`: one hub API method per command, a table or a line for people,
//! the API's own JSON under `--json`.

use std::time::Duration;

use pt_api::{self as api, EpochMs, method as m};
use pt_json::ToJson;

use crate::{Args, Client, Fail};

/// Methods a D9 approval may hold: the CLI waits as long as the hub may.
const APPROVAL_WAIT: Duration = Duration::from_secs(3600);

struct Out {
    json: bool,
}

impl Out {
    fn show<T: ToJson>(&self, v: &T, human: impl FnOnce(&T)) {
        if self.json {
            println!("{}", v.to_json());
        } else {
            human(v);
        }
    }
}

/// Columns padded to their widest cell; the last one is not padded.
fn table(head: &[&str], rows: &[Vec<String>]) {
    let mut w: Vec<usize> = head.iter().map(|h| h.len()).collect();
    for r in rows {
        for (i, c) in r.iter().enumerate() {
            w[i] = w[i].max(c.chars().count());
        }
    }
    let line = |cells: &mut dyn Iterator<Item = String>| {
        let cells: Vec<String> = cells.collect();
        let mut s = String::new();
        for (i, c) in cells.iter().enumerate() {
            if i + 1 == cells.len() {
                s.push_str(c);
            } else {
                s.push_str(&format!("{c:<width$}  ", width = w[i]));
            }
        }
        println!("{}", s.trim_end());
    };
    line(&mut head.iter().map(|h| h.to_string()));
    for r in rows {
        line(&mut r.iter().cloned());
    }
}

fn ago(t: EpochMs) -> String {
    let now = EpochMs::now().0;
    if t.0 == 0 {
        return "-".into();
    }
    let s = now.saturating_sub(t.0) / 1000;
    match s {
        0..60 => format!("{s}s ago"),
        60..3600 => format!("{}m ago", s / 60),
        3600..86400 => format!("{}h ago", s / 3600),
        _ => format!("{}d ago", s / 86400),
    }
}

fn opt<T: std::fmt::Display>(v: &Option<T>) -> String {
    v.as_ref().map_or_else(|| "-".into(), |x| x.to_string())
}

fn harness(s: &str) -> Result<api::Harness, String> {
    pt_json::from_str(&format!("{s:?}")).map_err(|_| {
        format!("unknown harness {s:?} (claude, codex, grok, pi_vendor, pi_local, antigravity)")
    })
}

fn word_enum<T: pt_json::FromJson>(s: &str, what: &str) -> Result<T, String> {
    pt_json::from_str(&format!("{:?}", s.replace('-', "_")))
        .map_err(|_| format!("unknown {what} {s:?}"))
}

fn secs(a: &Args, default: u64) -> Result<u64, String> {
    Ok(a.num("--timeout")?.unwrap_or(default))
}

pub fn sleeve_rows(sleeves: &[api::Sleeve]) {
    let rows: Vec<Vec<String>> = sleeves
        .iter()
        .map(|s| {
            let inc = s.incarnation.as_ref();
            vec![
                s.id.clone(),
                s.name.clone(),
                s.role.to_string(),
                match &s.park_reason {
                    Some(r) => format!("{} ({r})", s.lifecycle),
                    None => s.lifecycle.to_string(),
                },
                inc.map_or("-".into(), |i| i.activity.state.to_string()),
                inc.map_or("-".into(), |i| i.harness.to_string()),
                inc.map_or("-".into(), |i| format!("g{}", i.g)),
                s.queued.to_string(),
                s.path.clone(),
            ]
        })
        .collect();
    table(
        &[
            "ID",
            "NAME",
            "ROLE",
            "LIFECYCLE",
            "ACTIVITY",
            "HARNESS",
            "G",
            "QUEUED",
            "PATH",
        ],
        &rows,
    );
}

fn first_line(s: &str, n: usize) -> String {
    let l = s.lines().next().unwrap_or("");
    match l.char_indices().nth(n) {
        Some((at, _)) => format!("{}...", &l[..at]),
        None => l.to_string(),
    }
}

pub fn entry_line(e: &api::ChatEntry) -> String {
    use api::Entry as E;
    let body = match &e.entry {
        E::SessionInfo {
            harness,
            native_id,
            model,
            ..
        } => format!("session: {harness} {} ({native_id})", opt(model)),
        E::UserMessage { from, text, .. } => format!("{from}: {text}"),
        E::PromptAck {
            request_id, ack, ..
        } => format!("ack: {request_id} {ack}"),
        E::AssistantText { text, .. } => format!("assistant: {text}"),
        E::Reasoning { text, .. } => format!("reasoning: {}", first_line(text, 200)),
        E::ToolCall { name, summary, .. } => format!("tool {name}: {summary}"),
        E::ToolResult {
            id, status, output, ..
        } => format!("result {id} {status}: {}", first_line(output, 200)),
        E::FileChange { files, .. } => {
            let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
            format!("changed: {}", paths.join(", "))
        }
        E::ApprovalRequest {
            approval_id,
            title,
            summary,
        } => format!("approval {approval_id}: {title}: {summary}"),
        E::ApprovalDecision {
            approval_id,
            decision,
            by,
        } => format!("approval {approval_id}: {decision} by {by}"),
        E::TurnStarted { .. } => "turn started".into(),
        E::TurnSettled { stop, usage, .. } => format!(
            "turn settled: {stop}, {} in / {} out tokens",
            usage.input, usage.output
        ),
        E::Error { kind, text } => format!("error {kind}: {text}"),
        E::Compaction { tokens_before, .. } => {
            format!("compacted ({} tokens before)", opt(tokens_before))
        }
        E::Notice { level, text, .. } => format!("notice {level}: {text}"),
        E::Exited { code, signal, .. } => {
            format!("exited (code {}, signal {})", opt(code), opt(signal))
        }
    };
    format!("[{}] {body}", e.seq)
}

fn entries(t: &api::Transcript) {
    for e in &t.entries {
        println!("{}", entry_line(e));
    }
}

fn approval_rows(list: &[api::Approval]) {
    let rows: Vec<Vec<String>> = list
        .iter()
        .map(|a| {
            vec![
                a.id.clone(),
                a.kind.to_string(),
                a.state.to_string(),
                a.sleeve_id.clone(),
                ago(a.requested_at_ms),
                a.title.clone(),
            ]
        })
        .collect();
    table(&["ID", "KIND", "STATE", "SLEEVE", "ASKED", "TITLE"], &rows);
}

/// The node it runs on, or before the scheduler picks one, the node or arch it asked for.
fn job_line(j: &api::Job) -> String {
    let place = match (&j.node_id, &j.target) {
        (Some(n), _) => format!("on {n}"),
        (None, api::JobTarget::Node { node_id }) => format!("for node {node_id}"),
        (None, api::JobTarget::Arch { arch }) => format!("for any {arch} node"),
    };
    let mut s = format!(
        "{} {} {place} by {}: {}",
        j.id,
        j.state,
        j.requested_by,
        j.argv.join(" ")
    );
    if let Some(e) = &j.error {
        s.push_str(&format!(" ({e})"));
    }
    s
}

/// Long enough for a download of every asset on a slow link.
const UPDATE_WAIT: Duration = Duration::from_secs(45 * 60);

fn channel(a: &Args) -> Result<Option<api::ReleaseChannel>, String> {
    a.flag("--channel")
        .map(|c| word_enum(c, "channel (stable, dev)"))
        .transpose()
}

fn hub_cmd(c: &Client, a: &Args, o: &Out) -> Result<(), Fail> {
    match a.word(1, "a hub command (info, state, update)")? {
        "update" => {
            let req = api::UpdateHubRequest {
                check: a.has("--check"),
                channel: channel(a)?,
                version: a.flag("--version").map(String::from),
                rollback: a.has("--rollback"),
            };
            let v = c.call_for::<m::UpdateHub>(&req, UPDATE_WAIT)?;
            o.show(&v, |u| {
                println!(
                    "{} -> {}: {}",
                    u.current.as_deref().unwrap_or("-"),
                    u.target,
                    u.note
                );
                if u.restarting {
                    println!(
                        "the hub restarts now and keeps {} once its health check passes (2 min); `protectorate hub info` shows it",
                        u.target
                    );
                }
            });
        }
        "info" => {
            let v = c.call::<m::GetHubInfo>(&api::Empty {})?;
            o.show(&v, |h| {
                println!(
                    "{} {}, up since {}",
                    h.name,
                    h.version,
                    ago(h.started_at_ms)
                );
                if h.history_capped {
                    println!("history capped");
                }
            });
        }
        "state" => {
            let v = c.call::<m::GetState>(&api::Empty {})?;
            o.show(&v, |s| {
                println!(
                    "{} {} at seq {}: {} sleeves, {} approvals pending, {} nodes, {} events",
                    s.hub.name,
                    s.hub.version,
                    s.seq,
                    s.sleeves.len(),
                    s.approvals
                        .iter()
                        .filter(|a| a.state == api::ApprovalState::Pending)
                        .count(),
                    s.nodes.len(),
                    s.events.len()
                );
                sleeve_rows(&s.sleeves);
            });
        }
        other => return Err(Fail::Usage(format!("unknown hub command {other:?}"))),
    }
    Ok(())
}

fn sleeve_ref(a: &Args) -> Result<api::SleeveRef, Fail> {
    Ok(api::SleeveRef {
        sleeve_id: a.word(2, "a sleeve")?.to_string(),
    })
}

fn sleeve_cmd(c: &Client, a: &Args, o: &Out) -> Result<(), Fail> {
    let accepted = |v: &api::Accepted, what: &str| match &v.approval_id {
        Some(id) => println!("{what} (owner approved {id})"),
        None => println!("{what}"),
    };
    match a.word(1, "a sleeve command")? {
        "list" => {
            let v = c.call::<m::ListSleeves>(&api::Empty {})?;
            o.show(&v, |v| sleeve_rows(&v.sleeves));
        }
        "show" => {
            let want = a.word(2, "a sleeve")?;
            let v = c.call::<m::ListSleeves>(&api::Empty {})?;
            let s = v
                .sleeves
                .into_iter()
                .find(|s| s.id == want || s.name == want)
                .ok_or_else(|| Fail::Other(format!("no sleeve {want:?}")))?;
            o.show(&s, |s| {
                sleeve_rows(std::slice::from_ref(s));
                if let Some(i) = &s.incarnation {
                    println!(
                        "g{} {} {} {}, turn {}, context {}/{}, session {}",
                        i.g,
                        i.harness,
                        i.provider,
                        i.model,
                        i.turn,
                        i.context_tokens,
                        i.context_window,
                        opt(&i.native_id)
                    );
                }
                if let Some(b) = &s.branch {
                    println!("branch {b}");
                }
            });
        }
        "spawn" => {
            let harness_name = a.flag("--harness").unwrap_or("claude");
            let path = a.flag("--path").map(|p| {
                std::fs::canonicalize(p)
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|_| p.to_string())
            });
            let resleeve_to = match a.flag("--resleeve-to") {
                Some(l) => l.split(',').map(harness).collect::<Result<Vec<_>, _>>()?,
                None => Vec::new(),
            };
            let req = api::SpawnSleeveRequest {
                name: a.word(2, "a sleeve name")?.to_string(),
                harness: harness(harness_name)?,
                path,
                parent_id: a.flag("--parent").map(str::to_string),
                view: a.has("--readonly").then_some(api::WorkspaceView::Readonly),
                mode: api::Mode::Headless,
                model: a.flag("--model").map(str::to_string),
                provider: a.flag("--provider").map(str::to_string),
                effort: a.flag("--effort").map(str::to_string),
                resleeve_to,
                message: a.flag("--message").map(str::to_string),
                budget: a.num("--budget")?,
            };
            let v = c.call_for::<m::SpawnSleeve>(&req, APPROVAL_WAIT)?;
            o.show(&v, |s| {
                println!("spawned {} (id {}) at {}", s.name, s.id, s.path)
            });
        }
        "send" => {
            let req = api::SendMessageRequest {
                sleeve_id: a.word(2, "a sleeve")?.to_string(),
                text: a.rest(3, "the message text")?,
                delivery: if a.has("--steer") {
                    api::Delivery::Steer
                } else {
                    api::Delivery::Queue
                },
            };
            let v = c.call::<m::SendMessage>(&req)?;
            o.show(&v, |v| println!("{}", v.request_id));
        }
        "interrupt" => {
            let v = c.call::<m::Interrupt>(&sleeve_ref(a)?)?;
            o.show(&v, |_| println!("interrupted"));
        }
        "stop" => {
            let v = c.call::<m::StopSleeve>(&sleeve_ref(a)?)?;
            o.show(&v, |v| accepted(v, "stopping"));
        }
        "rm" => {
            let v = c.call_for::<m::RemoveSleeve>(&sleeve_ref(a)?, APPROVAL_WAIT)?;
            o.show(&v, |v| accepted(v, "removing"));
        }
        "takeover" => {
            let v = c.call::<m::TakeoverTerminal>(&sleeve_ref(a)?)?;
            o.show(&v, |_| println!("taken over"));
        }
        "start" => {
            let req = api::StartSleeveRequest {
                sleeve_id: a.word(2, "a sleeve")?.to_string(),
                target: a.flag("--harness").map(harness).transpose()?,
                model: a.flag("--model").map(str::to_string),
                provider: a.flag("--provider").map(str::to_string),
            };
            let v = c.call::<m::StartSleeve>(&req)?;
            o.show(&v, |v| accepted(v, "starting"));
        }
        "resleeve" => {
            let req = api::ResleeveRequest {
                sleeve_id: a.word(2, "a sleeve")?.to_string(),
                target: harness(
                    a.flag("--harness")
                        .ok_or("--harness is required".to_string())?,
                )?,
                model: a.flag("--model").map(str::to_string),
                provider: a.flag("--provider").map(str::to_string),
            };
            let v = c.call::<m::Resleeve>(&req)?;
            o.show(&v, |v| accepted(v, "resleeving"));
        }
        "transcript" => {
            let req = api::GetTranscriptRequest {
                sleeve_id: a.word(2, "a sleeve")?.to_string(),
                from_seq: a.num("--from")?,
                limit: a.num("--limit")?,
            };
            let v = c.call::<m::GetTranscript>(&req)?;
            o.show(&v, entries);
        }
        "wait" => {
            let t = secs(a, 600)?;
            let req = api::WaitRequest {
                sleeve_id: a.word(2, "a sleeve")?.to_string(),
                until: word_enum(a.flag("--until").unwrap_or("idle"), "--until")?,
                timeout_ms: t * 1000,
            };
            let v = c.call_for::<m::Wait>(&req, Duration::from_secs(t))?;
            o.show(&v, |v| {
                let life = v.sleeve.lifecycle;
                let act = v
                    .sleeve
                    .incarnation
                    .as_ref()
                    .map_or("-".into(), |i| i.activity.state.to_string());
                println!(
                    "{}: {life}, {act}",
                    if v.reached { "reached" } else { "not reached" }
                );
            });
            if !v.reached {
                return Err(Fail::Other(format!(
                    "wait: {} did not reach {}",
                    v.sleeve.name, req.until
                )));
            }
        }
        other => return Err(Fail::Usage(format!("unknown sleeve command {other:?}"))),
    }
    Ok(())
}

fn approval_cmd(c: &Client, a: &Args, o: &Out) -> Result<(), Fail> {
    match a.word(1, "an approval command (list, decide)")? {
        "list" => {
            let req = api::ListApprovalsRequest {
                pending_only: a.has("--pending"),
            };
            let v = c.call::<m::ListApprovals>(&req)?;
            o.show(&v, |v| approval_rows(&v.approvals));
        }
        "decide" => {
            let decision = match a.word(3, "allow, allow-session or deny")? {
                "allow" | "allow-once" | "allow_once" => api::Decision::AllowOnce,
                "allow-session" | "allow_session" => api::Decision::AllowSession,
                "deny" => api::Decision::Deny,
                d => return Err(Fail::Usage(format!("unknown decision {d:?}"))),
            };
            let req = api::DecideRequest {
                approval_id: a.word(2, "an approval id")?.to_string(),
                decision,
            };
            let v = c.call::<m::Decide>(&req)?;
            o.show(&v, |v| match v.note.as_str() {
                "" => println!("{} {}", v.id, v.state),
                note => println!("{} {}: {note}", v.id, v.state),
            });
        }
        other => return Err(Fail::Usage(format!("unknown approval command {other:?}"))),
    }
    Ok(())
}

fn node_rows(nodes: &[api::Node]) {
    let rows: Vec<Vec<String>> = nodes
        .iter()
        .map(|n| {
            let inv = n.inventory.as_ref();
            vec![
                n.id.clone(),
                n.name.clone(),
                inv.map_or("-".into(), |i| format!("{}/{}", i.os, i.arch)),
                if n.online { "up" } else { "down" }.into(),
                n.presence.state.to_string(),
                ago(n.last_seen_ms),
                if n.node_version.is_empty() {
                    "-".into()
                } else {
                    n.node_version.clone()
                },
                n.error.clone().unwrap_or_default(),
            ]
        })
        .collect();
    table(
        &[
            "ID", "NAME", "OS/ARCH", "LINK", "PRESENCE", "SEEN", "VERSION", "ERROR",
        ],
        &rows,
    );
}

fn pending_line(p: &api::PendingNode) -> String {
    format!(
        "{} wants to join as {} (control key {}{}); approve with: protectorate node approve {}",
        p.auth_id,
        p.name,
        p.control_key,
        if p.key_matches {
            ", matches"
        } else {
            ", DOES NOT MATCH this hub"
        },
        p.auth_id
    )
}

fn node_cmd(c: &Client, a: &Args, o: &Out) -> Result<(), Fail> {
    match a.word(
        1,
        "a node command (ls, show, add, approve, rename, rm, update)",
    )? {
        "update" => {
            let req = api::UpdateNodesRequest {
                nodes: a.words.get(2..).unwrap_or(&[]).to_vec(),
                all: a.has("--all"),
                version: a.flag("--version").map(String::from),
                channel: channel(a)?,
                probation_secs: a.num("--probation")?,
                no_wait: a.has("--no-wait"),
            };
            if req.all == !req.nodes.is_empty() {
                return Err(Fail::Usage("node update needs node names or --all".into()));
            }
            let v = c.call_for::<m::UpdateNodes>(&req, Duration::from_secs(4 * 3600))?;
            o.show(&v, |u| {
                let rows: Vec<Vec<String>> = u
                    .nodes
                    .iter()
                    .map(|n| {
                        vec![
                            n.name.clone(),
                            n.from.clone(),
                            n.to.clone(),
                            if n.ok { "ok" } else { "FAILED" }.into(),
                            n.detail.clone(),
                        ]
                    })
                    .collect();
                table(&["NODE", "FROM", "TO", "", "DETAIL"], &rows);
            });
            if v.nodes.iter().any(|n| !n.ok) {
                return Err(Fail::Other("some nodes did not update".into()));
            }
        }
        "ls" | "list" => {
            let v = c.call::<m::ListNodes>(&api::Empty {})?;
            o.show(&v, |v| node_rows(&v.nodes));
        }
        "show" => {
            let req = api::NodeRef {
                node_id: a.word(2, "a node")?.to_string(),
            };
            let v = c.call::<m::GetNode>(&req)?;
            o.show(&v, |n| {
                node_rows(std::slice::from_ref(n));
                if let Some(i) = &n.inventory {
                    println!(
                        "host {} {} {}, {} {} threads, {} MiB RAM",
                        i.hostname,
                        i.os,
                        i.os_version,
                        i.cpu_model,
                        i.cpu_count,
                        i.ram_bytes >> 20
                    );
                }
                let p = &n.presence;
                println!(
                    "presence {} idle {} locked {} via {}",
                    p.state,
                    opt(&p.idle_secs),
                    opt(&p.locked),
                    if p.source.is_empty() { "-" } else { &p.source }
                );
            });
        }
        "add" => {
            if a.words.len() <= 2 {
                let v = c.call::<m::GetEnrollment>(&api::Empty {})?;
                o.show(&v, |e| {
                    println!("on the new machine, run:\n  {}", e.join_command);
                    println!("it shows the control key it saw; this hub's is {}", e.control_key);
                    println!(
                        "then paste its registration: protectorate node add '<text>', or run the approve line it prints"
                    );
                    for p in &e.pending {
                        println!("{}", pending_line(p));
                    }
                });
            } else {
                let req = api::AddNodeRequest {
                    registration: a.rest(2, "the registration text")?,
                };
                let v = c.call::<m::AddNode>(&req)?;
                o.show(&v, |p| println!("{}", pending_line(p)));
            }
        }
        "approve" => {
            let req = api::ApproveNodeRequest {
                auth_id: a.word(2, "an auth id")?.to_string(),
                name: a.flag("--name").map(String::from),
                fingerprint: a.flag("--fingerprint").map(String::from),
            };
            let v = c.call::<m::ApproveNode>(&req)?;
            o.show(&v, |n| {
                println!("approved {} as node {}", n.name, n.node_id)
            });
        }
        "rename" => {
            let req = api::RenameNodeRequest {
                node_id: a.word(2, "a node")?.to_string(),
                name: a.word(3, "the new name")?.to_string(),
            };
            let v = c.call::<m::RenameNode>(&req)?;
            o.show(&v, |n| {
                println!("renamed node {} from {} to {}", n.node_id, n.from, n.to)
            });
        }
        "rm" => {
            let req = api::RemoveNodeRequest {
                node_id: a.word(2, "a node")?.to_string(),
                force: a.has("--force"),
            };
            let v = c.call::<m::RemoveNode>(&req)?;
            o.show(&v, |n| {
                if n.record_only {
                    println!(
                        "removed {} (node {}): headscale had already forgotten it; dropped its enrollment record",
                        n.name, n.node_id
                    );
                } else {
                    println!(
                        "removed {} (node {}) from headscale; its tailnet key is revoked",
                        n.name, n.node_id
                    );
                }
                if !n.failed_jobs.is_empty() {
                    println!(
                        "queued jobs {} fail: node {} was removed",
                        n.failed_jobs.join(", "),
                        n.node_id
                    );
                }
                if !n.orphaned_jobs.is_empty() {
                    println!(
                        "jobs {} were in flight; they go lost, then abandoned",
                        n.orphaned_jobs.join(", ")
                    );
                }
            });
        }
        other => return Err(Fail::Usage(format!("unknown node command {other:?}"))),
    }
    Ok(())
}

fn job_rows(jobs: &[api::Job]) {
    let rows: Vec<Vec<String>> = jobs
        .iter()
        .map(|j| {
            vec![
                j.id.clone(),
                j.state.to_string(),
                opt(&j.node_id),
                opt(&j.exit_code),
                j.requested_by.clone(),
                ago(j.queued_at_ms),
                j.argv.join(" "),
            ]
        })
        .collect();
    table(
        &["ID", "STATE", "NODE", "EXIT", "BY", "QUEUED", "COMMAND"],
        &rows,
    );
}

fn job_cmd(c: &Client, a: &Args, o: &Out) -> Result<(), Fail> {
    let job = |a: &Args| -> Result<api::JobRef, Fail> {
        Ok(api::JobRef {
            job_id: a.word(2, "a job")?.to_string(),
        })
    };
    match a.word(1, "a job command (run, ls, show, log, artifact, cancel)")? {
        "run" => {
            let target = match (a.flag("--node"), a.flag("--arch")) {
                (Some(n), None) => api::JobTarget::Node {
                    node_id: n.to_string(),
                },
                (None, Some(arch)) => api::JobTarget::Arch {
                    arch: arch.to_string(),
                },
                _ => return Err(Fail::Usage("job run needs --node or --arch".into())),
            };
            let req = api::RunJobRequest {
                target,
                argv: a.words.get(2..).unwrap_or(&[]).to_vec(),
                cwd: a.flag("--cwd").map(String::from),
                artifacts: a.all("--artifact"),
                even_if_present: a.has("--even-if-present"),
                on_present: word_enum(a.flag("--on-present").unwrap_or("idle"), "--on-present")?,
                maintenance: a.has("--maintenance"),
                timeout_secs: a.num("--timeout")?,
            };
            let v = c.call_for::<m::RunJob>(&req, APPROVAL_WAIT)?;
            o.show(&v, |j| println!("{}", job_line(j)));
        }
        "ls" | "list" => {
            let req = api::ListJobsRequest {
                node_id: a.flag("--node").map(String::from),
                limit: a.num("--limit")?,
            };
            let v = c.call::<m::ListJobs>(&req)?;
            o.show(&v, |v| job_rows(&v.jobs));
        }
        "show" => {
            let v = c.call::<m::GetJob>(&job(a)?)?;
            o.show(&v, |j| {
                println!("{}", job_line(j));
                println!(
                    "attempt {}, exit {}, log {} bytes",
                    j.attempt,
                    opt(&j.exit_code),
                    j.log_bytes
                );
                for f in &j.artifacts {
                    println!(
                        "artifact {}: {} bytes, sha256 {}",
                        f.path,
                        opt(&f.size_bytes),
                        opt(&f.sha256)
                    );
                }
            });
        }
        "cancel" => {
            let v = c.call::<m::CancelJob>(&job(a)?)?;
            o.show(&v, |j| println!("{}", job_line(j)));
        }
        "log" => {
            let req = api::GetJobLogRequest {
                job_id: a.word(2, "a job")?.to_string(),
                offset: a.num("--offset")?.unwrap_or(0),
                limit: a.num("--limit")?,
            };
            let v = c.call::<m::GetJobLog>(&req)?;
            o.show(&v, |l| print!("{}", l.text));
        }
        "artifact" => artifact(c, a, o)?,
        other => return Err(Fail::Usage(format!("unknown job command {other:?}"))),
    }
    Ok(())
}

/// Every slice of the artifact, checked against its SHA-256, to `--out` or stdout.
fn artifact(c: &Client, a: &Args, o: &Out) -> Result<(), Fail> {
    let job_id = a.word(2, "a job")?.to_string();
    let path = a.word(3, "an artifact path")?.to_string();
    let mut bytes = Vec::new();
    let mut last;
    loop {
        let req = api::GetJobArtifactRequest {
            job_id: job_id.clone(),
            path: path.clone(),
            offset: bytes.len() as u64,
            limit: None,
        };
        last = c.call::<m::GetJobArtifact>(&req)?;
        let chunk = pt_hash::from_hex(&last.hex)
            .map_err(|e| Fail::Other(format!("artifact bytes: {e:?}")))?;
        let empty = chunk.is_empty();
        bytes.extend(chunk);
        if last.eof || empty {
            break;
        }
    }
    let sha = pt_hash::to_hex(&pt_hash::sha256(&bytes));
    if sha != last.sha256 {
        return Err(Fail::Other(format!(
            "artifact {path}: got sha256 {sha}, the hub recorded {}",
            last.sha256
        )));
    }
    match a.flag("--out") {
        Some(f) => {
            std::fs::write(f, &bytes).map_err(|e| Fail::Other(format!("{f}: {e}")))?;
            last.hex = String::new();
            o.show(&last, |l| {
                println!(
                    "{} bytes, sha256 {}, written to {f}",
                    l.size_bytes, l.sha256
                )
            });
        }
        None => {
            use std::io::Write;
            std::io::stdout()
                .write_all(&bytes)
                .map_err(|e| Fail::Other(format!("stdout: {e}")))?;
        }
    }
    Ok(())
}

fn quota_cmd(c: &Client, a: &Args, o: &Out) -> Result<(), Fail> {
    match a.word(1, "a quota command (list)")? {
        "list" => {
            let v = c.call::<m::ListQuotas>(&api::Empty {})?;
            o.show(&v, |v| {
                let rows: Vec<Vec<String>> = v
                    .quotas
                    .iter()
                    .map(|q| {
                        let windows: Vec<String> = q
                            .windows
                            .iter()
                            .map(|w| format!("{} {}%", w.label, opt(&w.used_pct)))
                            .collect();
                        vec![
                            q.subscription.to_string(),
                            q.reading.to_string(),
                            if windows.is_empty() {
                                "-".into()
                            } else {
                                windows.join(", ")
                            },
                            q.note.clone(),
                        ]
                    })
                    .collect();
                table(&["SUBSCRIPTION", "READING", "WINDOWS", "NOTE"], &rows);
            });
        }
        other => return Err(Fail::Usage(format!("unknown quota command {other:?}"))),
    }
    Ok(())
}

fn event_cmd(c: &Client, a: &Args, o: &Out) -> Result<(), Fail> {
    match a.word(1, "an event command (list, read)")? {
        "list" => {
            let req = api::ListEventsRequest {
                from_seq: a.num("--from")?,
                limit: a.num("--limit")?,
            };
            let v = c.call::<m::ListEvents>(&req)?;
            o.show(&v, |v| {
                let rows: Vec<Vec<String>> = v
                    .events
                    .iter()
                    .map(|e| {
                        vec![
                            e.seq.to_string(),
                            ago(e.at_ms),
                            e.kind.to_string(),
                            e.level.to_string(),
                            if e.read { "read" } else { "new" }.into(),
                            format!("{} -> {}", e.source, opt(&e.target)),
                            e.text.clone(),
                        ]
                    })
                    .collect();
                table(
                    &["SEQ", "AT", "KIND", "LEVEL", "READ", "FROM", "TEXT"],
                    &rows,
                );
            });
        }
        "read" => {
            let up_to_seq = a
                .word(2, "a seq")?
                .parse()
                .map_err(|_| Fail::Usage("event read takes a seq".into()))?;
            let v = c.call::<m::MarkEventsRead>(&api::MarkEventsReadRequest { up_to_seq })?;
            o.show(&v, |_| println!("marked read up to {up_to_seq}"));
        }
        other => return Err(Fail::Usage(format!("unknown event command {other:?}"))),
    }
    Ok(())
}

/// `7d`, `3h`, `30m` or `90s` ago, or epoch milliseconds.
fn when(s: &str) -> Result<EpochMs, String> {
    let bad = || format!("bad time {s:?} (7d, 3h, 30m, 90s or epoch milliseconds)");
    let (n, unit) = s.split_at(s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len()));
    let n: u64 = n.parse().map_err(|_| bad())?;
    let secs = match unit {
        "" => return Ok(EpochMs(n)),
        "s" => 1,
        "m" => 60,
        "h" => 3600,
        "d" => 86400,
        _ => return Err(bad()),
    };
    Ok(EpochMs(
        EpochMs::now()
            .0
            .saturating_sub(n.saturating_mul(secs * 1000)),
    ))
}

fn history_head(d: &api::HistoryDoc) -> String {
    format!(
        "[doc {}] {} {} turn {} {}, {}, session {}",
        d.doc_id,
        if d.sleeve.is_empty() {
            &d.sleeve_id
        } else {
            &d.sleeve
        },
        opt(&d.harness),
        d.turn,
        d.role,
        ago(d.time_ms),
        d.session
    )
}

/// The snippet with each match in `**`.
fn marked(h: &api::HistoryHit) -> String {
    let (t, mut out, mut at) = (&h.snippet, String::new(), 0);
    for s in &h.highlights {
        let (start, end) = (s.start as usize, s.end as usize);
        let (Some(before), Some(word)) = (t.get(at..start), t.get(start..end)) else {
            continue;
        };
        out.push_str(before);
        out.push_str("**");
        out.push_str(word);
        out.push_str("**");
        at = end;
    }
    out.push_str(t.get(at..).unwrap_or(""));
    out
}

fn history_cmd(c: &Client, a: &Args, o: &Out) -> Result<(), Fail> {
    match a.word(1, "a history command (search, read)")? {
        "search" => {
            let req = api::SearchHistoryRequest {
                query: a.rest(2, "a query")?,
                mode: word_enum(a.flag("--mode").unwrap_or("all"), "--mode")?,
                sleeves: a.all("--sleeve"),
                harness: a.flag("--harness").map(harness).transpose()?,
                session: a.flag("--session").map(str::to_string),
                since_ms: a.flag("--since").map(when).transpose()?,
                until_ms: a.flag("--until").map(when).transpose()?,
                roles: a
                    .all("--role")
                    .iter()
                    .map(|r| word_enum(r, "--role"))
                    .collect::<Result<_, _>>()?,
                limit: a.num("--limit")?,
            };
            let v = c.call::<m::SearchHistory>(&req)?;
            o.show(&v, |v| {
                if v.hits.is_empty() {
                    println!("no hits ({} docs indexed)", v.indexed_docs);
                }
                for h in &v.hits {
                    println!("{}", history_head(&h.doc));
                    println!("    {}", marked(h));
                }
            });
        }
        "read" => {
            let req = match a.num("--doc")? {
                Some(id) => api::ReadHistoryRequest {
                    doc_id: Some(id),
                    session: None,
                    from_turn: 0,
                    to_turn: None,
                    after_doc: None,
                },
                None => api::ReadHistoryRequest {
                    doc_id: None,
                    session: Some(a.word(2, "a session id, or --doc ID")?.to_string()),
                    from_turn: a.num("--from")?.unwrap_or(0),
                    to_turn: a.num("--to")?,
                    after_doc: a.num("--after")?,
                },
            };
            let v = c.call::<m::ReadHistory>(&req)?;
            o.show(&v, |v| {
                for d in &v.docs {
                    println!("{} ({}:{})", history_head(&d.doc), d.doc.path, d.doc.offset);
                    if !d.doc.files.is_empty() {
                        println!("files: {}", d.doc.files.join(", "));
                    }
                    println!("{}\n", d.text);
                }
                if v.more && let Some(last) = v.docs.last() {
                    println!("[more: read on with --after {}]", last.doc.doc_id);
                }
            });
        }
        other => return Err(Fail::Usage(format!("unknown history command {other:?}"))),
    }
    Ok(())
}

fn envoy_cmd(c: &Client, a: &Args, o: &Out) -> Result<(), Fail> {
    match a.word(1, "an envoy command (send, wait, read, token)")? {
        "send" => {
            let req = api::EnvoySendRequest {
                text: a.rest(2, "the message text")?,
                agent: a.flag("--as").map(str::to_string),
            };
            let v = c.call::<m::EnvoySend>(&req)?;
            o.show(&v, |v| println!("{}", v.request_id));
        }
        "wait" => {
            let t = secs(a, 600)?;
            let req = api::EnvoyWaitRequest {
                request_id: a.words.get(2).cloned(),
                timeout_ms: t * 1000,
            };
            let v = c.call_for::<m::EnvoyWait>(&req, Duration::from_secs(t))?;
            o.show(&v, |v| {
                if v.settled {
                    println!("{}", v.text);
                } else {
                    eprintln!("not settled yet: {}", v.request_id);
                }
            });
            if !v.settled {
                return Err(Fail::Other(format!(
                    "envoy wait: {} not settled",
                    v.request_id
                )));
            }
        }
        "read" => {
            let req = api::EnvoyReadRequest {
                from_seq: a.num("--from")?.unwrap_or(0),
                limit: a.num("--limit")?,
            };
            let v = c.call::<m::EnvoyRead>(&req)?;
            o.show(&v, entries);
        }
        "token" => {
            let v = c.call::<m::GetEnvoyToken>(&api::Empty {})?;
            o.show(&v, |v| println!("{}", v.token));
        }
        other => return Err(Fail::Usage(format!("unknown envoy command {other:?}"))),
    }
    Ok(())
}

fn device_cmd(c: &Client, a: &Args, o: &Out) -> Result<(), Fail> {
    match a.word(1, "a device command (list, add, pair, revoke)")? {
        "list" => {
            let v = c.call::<m::ListDevices>(&api::Empty {})?;
            o.show(&v, |v| {
                let rows: Vec<Vec<String>> = v
                    .devices
                    .iter()
                    .map(|d| {
                        vec![
                            d.id.clone(),
                            d.name.clone(),
                            ago(d.created_at_ms),
                            d.last_seen_ms.map_or("never".into(), ago),
                        ]
                    })
                    .collect();
                table(&["ID", "NAME", "PAIRED", "SEEN"], &rows);
            });
        }
        "add" => {
            let req = api::AddDeviceRequest {
                name: a.rest(2, "a device name")?,
            };
            let v = c.call::<m::AddDevice>(&req)?;
            o.show(&v, |v| {
                println!("pairing code for {}: {}", v.name, v.code);
                println!(
                    "enter it in the app's SETTINGS, or open <hub URL>/?pair={} on the device;",
                    v.code
                );
                println!("it works once, for the next 10 minutes");
            });
        }
        "pair" => {
            let req = api::PairDeviceRequest {
                code: a.word(2, "a pairing code")?.to_string(),
            };
            let v = c.call::<m::PairDevice>(&req)?;
            o.show(&v, |v| {
                println!(
                    "paired {} ({}); token (shown once):",
                    v.device.name, v.device.id
                );
                println!("{}", v.token);
            });
        }
        "revoke" => {
            let req = api::DeviceRef {
                device_id: a.word(2, "a device id")?.to_string(),
            };
            let v = c.call::<m::RevokeDevice>(&req)?;
            o.show(&v, |_| println!("revoked {}", req.device_id));
        }
        other => return Err(Fail::Usage(format!("unknown device command {other:?}"))),
    }
    Ok(())
}

fn doctor_cmd(c: &Client, a: &Args, o: &Out) -> Result<(), Fail> {
    let req = api::GetDoctorRequest {
        refresh: a.has("--refresh"),
    };
    let v = c.call_for::<m::GetDoctor>(&req, Duration::from_secs(120))?;
    o.show(&v, |d| {
        println!(
            "{} {}, checked {}; gh {}; sandbox {}",
            d.os,
            d.arch,
            if d.checked_at_ms.0 == 0 {
                "never".into()
            } else {
                ago(d.checked_at_ms)
            },
            if d.gh_logged_in {
                format!("logged in as {}", d.gh_account)
            } else {
                "not logged in".into()
            },
            if d.sandbox_ok { "ok" } else { "NOT OK" }
        );
        let rows: Vec<Vec<String>> = d
            .harnesses
            .iter()
            .map(|h| {
                vec![
                    h.name.clone(),
                    if h.eligible { "yes" } else { "NO" }.into(),
                    if h.present {
                        format!("{} ({})", h.version, h.install)
                    } else {
                        "missing".into()
                    },
                    if h.login { "yes" } else { "no" }.into(),
                    h.updated_at_ms.map_or("never".into(), ago),
                    h.reasons.join("; "),
                ]
            })
            .collect();
        table(
            &["HARNESS", "ELIGIBLE", "VERSION", "LOGIN", "UPDATED", "WHY"],
            &rows,
        );
        for p in &d.problems {
            println!("problem: {p}");
        }
    });
    Ok(())
}

fn harness_cmd(c: &Client, a: &Args, o: &Out) -> Result<(), Fail> {
    match a.word(1, "a harness command (update or login)")? {
        "update" => {
            let req = api::UpdateHarnessesRequest {
                programs: a.words.get(2..).unwrap_or(&[]).to_vec(),
            };
            let v = c.call::<m::UpdateHarnesses>(&req)?;
            o.show(&v, |v| {
                let rows: Vec<Vec<String>> = v
                    .programs
                    .iter()
                    .map(|p| vec![p.program.clone(), p.state.to_string(), p.detail.clone()])
                    .collect();
                table(&["PROGRAM", "STATE", "DETAIL"], &rows);
                println!("each run ends in a hub event (`protectorate event list`)");
            });
        }
        other => return Err(Fail::Usage(format!("unknown harness command {other:?}"))),
    }
    Ok(())
}

pub fn run(c: &Client, a: &Args, json: bool) -> Result<(), Fail> {
    let o = Out { json };
    match a.word(0, "a command")? {
        "hub" => hub_cmd(c, a, &o),
        "sleeve" => sleeve_cmd(c, a, &o),
        "approval" => approval_cmd(c, a, &o),
        "node" => node_cmd(c, a, &o),
        "job" => job_cmd(c, a, &o),
        "quota" => quota_cmd(c, a, &o),
        "event" => event_cmd(c, a, &o),
        "envoy" => envoy_cmd(c, a, &o),
        "history" => history_cmd(c, a, &o),
        "device" => device_cmd(c, a, &o),
        "doctor" => doctor_cmd(c, a, &o),
        "harness" => harness_cmd(c, a, &o),
        other => Err(Fail::Usage(format!("unknown command {other:?}"))),
    }
}
