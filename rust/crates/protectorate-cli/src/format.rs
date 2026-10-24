//! Human output: tab-aligned columns, `-` for empty values and timestamps in
//! local time.
use chrono::{DateTime, Local, LocalResult, TimeZone};
use protectorate_wire::{v1, Code, Result, RpcError};
use std::{
    io::Write,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Default)]
pub(crate) struct ChatRenderer {
    open_id: String,
    open_thinking: bool,
    open_text: bool,
}

impl ChatRenderer {
    pub(crate) fn flush(&mut self, output: &mut dyn Write) -> Result<()> {
        if self.open_text {
            output.write_all(b"\n").map_err(write_error)?;
            self.open_text = false;
        }
        Ok(())
    }

    pub(crate) fn render(&mut self, output: &mut dyn Write, event: &v1::ChatEvent) -> Result<()> {
        use v1::chat_event::Body;
        let time = stamp(event.time.as_ref());
        let Some(body) = &event.body else {
            return Ok(());
        };
        let (kind, detail) = match body {
            Body::Text(part) => return self.text(output, &time, part),
            Body::User(user) => ("user", format!("{}: {}", user.from, user.text)),
            Body::ToolCall(call) => ("toolCall", format!("{} {}", call.name, call.input_json)),
            Body::ToolResult(result) => ("toolResult", result.output.clone()),
            Body::Approval(approval) => (
                "approval",
                format!("{} [{}] {}", approval.id, approval.kind, approval.summary),
            ),
            Body::ApprovalResolved(resolved) => (
                "approvalResolved",
                format!(
                    "{} allow={} by {}",
                    resolved.id, resolved.allow, resolved.by
                ),
            ),
            Body::Usage(usage) => ("usage", compact_json(usage)?),
            Body::Settled(settled) => ("settled", settled.stop_reason.clone()),
            Body::Compaction(compaction) => ("compaction", compact_json(compaction)?),
            Body::Error(error) => ("error", error.text.clone()),
            Body::State(change) => ("state", chat_state_label(change.state()).to_owned()),
            Body::Settings(settings) => ("settings", compact_json(settings)?),
            Body::Report(report) => (
                "report",
                format!("{}: {}", result_status(report), report.summary),
            ),
            Body::Handoff(handoff) => ("handoff", compact_json(handoff)?),
        };
        self.flush(output)?;
        writeln!(
            output,
            "{time} {kind:<9} {}",
            detail.split_whitespace().collect::<Vec<_>>().join(" ")
        )
        .map_err(write_error)
    }

    /// Assistant text streams onto one line per message and channel.
    fn text(&mut self, output: &mut dyn Write, time: &str, part: &v1::AssistantText) -> Result<()> {
        if !self.open_text || self.open_id != part.message_id || self.open_thinking != part.thinking
        {
            self.flush(output)?;
            let label = if part.thinking {
                "thinking"
            } else {
                "assistant"
            };
            write!(output, "{time} {label:<9} ").map_err(write_error)?;
            self.open_text = true;
            self.open_id.clone_from(&part.message_id);
            self.open_thinking = part.thinking;
        }
        output.write_all(part.delta.as_bytes()).map_err(write_error)
    }
}

fn write_error(error: std::io::Error) -> RpcError {
    RpcError::new(Code::Internal, format!("write chat event: {error}"))
}

/// A message as compact protobuf JSON text.
fn compact_json<T: protectorate_wire::ReflectMessage>(message: &T) -> Result<String> {
    String::from_utf8(protectorate_wire::encode(message)?)
        .map_err(|error| RpcError::new(Code::Internal, format!("encode JSON: {error}")))
}

fn project_owners(project: &v1::DirectoryProject) -> String {
    or_dash(
        project
            .owners
            .iter()
            .map(|owner| owner.sleeve.as_str())
            .collect::<Vec<_>>()
            .join(","),
    )
}

fn project_edges(edges: &[v1::ProjectEdge]) -> String {
    or_dash(
        edges
            .iter()
            .map(|edge| edge.display.as_str())
            .collect::<Vec<_>>()
            .join(","),
    )
}

pub(crate) fn projects_list_text(response: &v1::ListProjectsResponse) -> String {
    let mut out = String::from("PROJECT\tSLEEVES\tDEPENDS ON\tDEPENDENTS\n");
    for project in &response.projects {
        out.push_str(&format!(
            "{}\t{}\t{}\t{}\n",
            project.display,
            project_owners(project),
            project_edges(&project.depends_on),
            project_edges(&project.dependents)
        ));
    }
    out
}

pub(crate) fn projects_detail_text(response: &v1::ListProjectsResponse) -> String {
    response
        .projects
        .iter()
        .map(project_text)
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn project_text(project: &v1::DirectoryProject) -> String {
    let mut out = format!("project\t{}\nid\t{}\n", project.display, project.id);
    if !project.default_branch.is_empty() || !project.latest_tag.is_empty() {
        out.push_str(&format!(
            "release\tbranch {}, latest tag {}\n",
            or_dash(project.default_branch.clone()),
            or_dash(project.latest_tag.clone())
        ));
    }
    if project.owners.is_empty() {
        out.push_str("sleeve\t(none live)\n");
    }
    for owner in &project.owners {
        let branch = if owner.branch.is_empty() {
            "(detached)"
        } else {
            &owner.branch
        };
        let head = owner.head.get(..8).unwrap_or(&owner.head);
        out.push_str(&format!(
            "sleeve\t{}\t{branch} {head}\t{}\n",
            owner.sleeve, owner.workspace
        ));
    }
    out.push_str(&format!(
        "provides\t{}\n",
        or_dash(project.provides.join(", "))
    ));
    out.push_str(&format!(
        "dependencies\t{} external\n",
        project.dependency_count
    ));
    for (edges, label) in [
        (&project.depends_on, "depends on"),
        (&project.dependents, "dependent"),
    ] {
        for edge in edges {
            out.push_str(&format!(
                "{label}\t{}\tvia {}\n",
                edge.display,
                edge.via.join(", ")
            ));
        }
    }
    out
}

pub(crate) fn event_text(event: &v1::Event) -> Result<String> {
    let payload = match &event.payload {
        Some(payload) => String::from_utf8(protectorate_wire::encode(payload)?)
            .map_err(|error| RpcError::new(Code::Internal, format!("encode payload: {error}")))?,
        None => "{}".to_owned(),
    };
    Ok(format!(
        "{}\t{}\t{}\t{payload}\n",
        stamp(event.time.as_ref()),
        event.kind,
        event.subject,
    ))
}

const NODE_COLUMNS: &str = "NAME\tOS/ARCH\tVERSION\tCPU\tGPU\tMEM\tDISK\tCONT\tFRESHNESS\tALERTS";

fn token_scopes(token: &v1::TokenInfo) -> String {
    format!("[{}]", token.scopes.join(" "))
}

/// "revoked", "expired" or "active" at `now` in Unix milliseconds.
fn token_state(token: &v1::TokenInfo, now: i64) -> &'static str {
    if token.revoked_at.is_some() {
        "revoked"
    } else if unix_millis(token.expires_at.as_ref()).is_some_and(|time| time <= now) {
        "expired"
    } else {
        "active"
    }
}

pub(crate) fn token_created_text(response: &v1::CreateTokenResponse) -> String {
    let token = response.token.clone().unwrap_or_default();
    format!("id\t{}\nname\t{}\nscopes\t{}\nexpires\t{}\nsecret\t{}\n\nThis is the only time the secret is shown. Renew it before it expires: protectorate --token-file FILE token renew.\n", token.id, token.name, token_scopes(&token), stamp(token.expires_at.as_ref()), response.secret)
}

pub(crate) fn token_list_text(response: &v1::ListTokensResponse) -> String {
    let mut out = "ID\tNAME\tSTATE\tEXPIRES\tSCOPES\tTASK\n".to_owned();
    let now = protectorate_fsutil::unix_millis();
    for token in &response.tokens {
        let task = if token.task.is_empty() {
            "-".into()
        } else {
            format!("{} (by {})", token.task, token.minted_by)
        };
        out.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t{task}\n",
            token.id,
            token.name,
            token_state(token, now),
            stamp(token.expires_at.as_ref()),
            token_scopes(token),
        ));
    }
    out
}

pub(crate) fn orchestrator_status_text(
    sleeve: Option<&v1::Sleeve>,
    token: Option<&v1::TokenInfo>,
    now: i64,
) -> String {
    let mut out = String::new();
    if let Some(sleeve) = sleeve {
        row(
            &mut out,
            "sleeve",
            format!("{} ({})", sleeve.name, sleeve_row_state(sleeve)),
        );
        row(&mut out, "harness", harness_label(sleeve));
        row(&mut out, "workspace", sleeve.workspace.clone());
        row(&mut out, "started", stamp(sleeve.created_at.as_ref()));
    } else {
        row(
            &mut out,
            "sleeve",
            "not created (run: protectorate orchestrator spawn)".into(),
        );
    }
    if let Some(token) = token {
        row(
            &mut out,
            "token",
            format!("{} ({})", token.id, token_state(token, now)),
        );
        row(&mut out, "scopes", token.scopes.join(", "));
        row(
            &mut out,
            "expires",
            match &token.expires_at {
                None => "never".to_owned(),
                at => stamp(at.as_ref()),
            },
        );
    } else {
        row(&mut out, "token", "none".into());
    }
    out
}

pub(crate) fn token_renewed_text(response: &v1::RenewTokenResponse) -> String {
    format!(
        "{}\nexpires\t{}\n",
        response.secret,
        stamp(
            response
                .token
                .as_ref()
                .and_then(|token| token.expires_at.as_ref())
        )
    )
}

pub(crate) fn token_revoked_text(response: &v1::RevokeTokenResponse) -> String {
    let token = response.token.clone().unwrap_or_default();
    format!("revoked {} ({})\n", token.id, token.name)
}

/// `devices ls`: one row per trusted device.
pub(crate) fn device_list_text(response: &v1::ListDevicesResponse) -> String {
    let mut out = "ID\tNAME\tPLATFORM\tLAST USED\tAPPROVED\tAPPROVER\n".to_owned();
    for device in &response.devices {
        let last_used = match &device.last_used_at {
            None => "never".into(),
            at => stamp(at.as_ref()),
        };
        let approved = match &device.approved_at {
            None => "unknown".into(),
            at => stamp(at.as_ref()),
        };
        out.push_str(&format!(
            "{}\t{}\t{}\t{last_used}\t{approved}\t{}\n",
            device.id,
            device.name,
            or_dash(device.platform.clone()),
            or_dash(device.approved_by.clone())
        ));
    }
    out
}

/// `approvals ls`: one row per gated agent call.
pub(crate) fn approvals_text(response: &v1::ListApprovalsResponse) -> String {
    let mut out = "ID\tSTATE\tACTOR\tCALL\tREASON\tEXPIRES\n".to_owned();
    for approval in &response.approvals {
        let mut call = approval
            .procedure
            .trim_start_matches("/protectorate.v1.")
            .to_owned();
        if !approval.subject.is_empty() {
            call.push(' ');
            call.push_str(&approval.subject);
        }
        let mut reason = approval.reason.clone();
        if !approval.detail.is_empty() {
            reason = format!("{reason}: {}", approval.detail);
        }
        out.push_str(&format!(
            "{}\t{}\t{}\t{call}\t{reason}\t{}\n",
            approval.id,
            word(approval.state().as_str_name(), "APPROVAL_STATE_"),
            approval.actor,
            stamp(approval.expires_at.as_ref())
        ));
    }
    out
}

/// `devices pending`: one row per pairing request still awaiting a decision.
pub(crate) fn device_pending_text(response: &v1::ListDeviceAuthResponse) -> String {
    let mut out = "CODE\tNAME\tPLATFORM\tREQUESTING IP\tEXPIRES\n".to_owned();
    for request in &response.requests {
        out.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\n",
            request.user_code,
            request.name,
            or_dash(request.platform.clone()),
            or_dash(request.requesting_ip.clone()),
            stamp(request.expires_at.as_ref())
        ));
    }
    out
}

pub(crate) fn grant_admin_text(
    response: &v1::GrantServiceAdminResponse,
    node: &str,
    out: Option<&std::path::Path>,
) -> String {
    let token = response.token.clone().unwrap_or_default();
    let expires = match &token.expires_at {
        None => "never".into(),
        at => stamp(at.as_ref()),
    };
    let use_command = match out {
        Some(path) => format!("protectorate --token-file {}", path.display()),
        None => format!("PROTECTORATE_TOKEN={} protectorate", response.secret),
    };
    let mut output = format!("grant\t{} ({})\ntask\t{}\nscopes\t{}\nexpires\t{expires}\n\nUse: {use_command} job run --on {node} [--admin] -- <cmd>\n", token.id, token.name, token.task, token_scopes(&token));
    if out.is_none() {
        output.push_str("This is the only time the secret is shown.\n");
    }
    output
}

pub(crate) fn audit_entries_text(
    response: &v1::ListAuditEntriesResponse,
    chronological: bool,
) -> String {
    let mut output = String::new();
    let entries: Box<dyn Iterator<Item = &v1::AuditEntry>> = if chronological {
        Box::new(response.entries.iter().rev())
    } else {
        Box::new(response.entries.iter())
    };
    for entry in entries {
        output.push_str(&format!(
            "{}  {}  {}  {}  {}",
            stamp(entry.time.as_ref()),
            entry.actor,
            entry.procedure,
            entry.subject,
            entry.outcome
        ));
        if !entry.code.is_empty() {
            output.push_str(&format!(" ({})", entry.code));
        }
        if !entry.detail.is_empty() {
            output.push_str(&format!("  {}", entry.detail));
        }
        if !entry.task.is_empty() {
            output.push_str(&format!("  [task: {}]", entry.task));
        }
        if !entry.run_id.is_empty() {
            output.push_str(&format!("  [run: {}]", entry.run_id));
        }
        output.push('\n');
    }
    if !chronological && !response.next_page_token.is_empty() {
        output.push_str(&format!("next page: {}\n", response.next_page_token));
    }
    output
}

pub(crate) fn audit_verify_text(report: &v1::VerifyAuditResponse) -> String {
    let head = report.head.clone().unwrap_or_default();
    let mut out = format!(
        "audit chain ok: {} entries, head {} {}\n",
        report.entries, head.seq, head.hash
    );
    if report.beyond_head > 0 {
        out.push_str(&format!(
            "{} entries after the recorded head; the next write records them\n",
            report.beyond_head
        ));
    }
    out
}

/// How many output lines `sleeve show` keeps from an exited session.
pub(crate) const SHOW_TAIL_LINES: usize = 20;

pub(crate) fn status_text(
    status: &v1::GetStatusResponse,
    runtime: Option<&v1::GetSleeveRuntimeResponse>,
) -> String {
    let mut out = String::new();
    row(&mut out, "version", status.version.clone());
    optional_row(&mut out, "available", status.available_version.clone());
    optional_row(&mut out, "channel", status.update_channel.clone());
    optional_row(&mut out, "release notes", status.release_notes_url.clone());
    row(&mut out, "hostname", status.hostname.clone());
    row(&mut out, "platform", os_arch(&status.os, &status.arch));
    row(
        &mut out,
        "uptime",
        go_duration(status.uptime_seconds as f64),
    );
    row(&mut out, "started", stamp(status.started_at.as_ref()));
    if let Some(runtime) = runtime {
        row(&mut out, "sleeve runtime", runtime.runtime.clone());
        optional_row(&mut out, "helper", runtime.helper_path.clone());
        row(&mut out, "overlays", runtime.overlays.to_string());
    }
    align(&out)
}

fn quota_freshness(quota: &v1::ProviderQuota) -> String {
    match quota.freshness() {
        v1::Freshness::Fresh => "fresh".into(),
        v1::Freshness::Stale => format!(
            "STALE ({} old)",
            seconds(quota.age.as_ref())
                .map(fmt_minutes)
                .unwrap_or_else(|| "?".into())
        ),
        _ => "UNKNOWN".into(),
    }
}

pub(crate) fn quota_text(response: &v1::GetQuotaResponse) -> String {
    let mut out =
        String::from("PROVIDER\tPLAN\tWINDOW\tUSED\tRESETS IN\tPROJ. UNUSED (est.)\tFRESHNESS\n");
    let now = protectorate_fsutil::unix_millis();
    let mut notes = Vec::new();
    for quota in &response.quotas {
        let provider = &quota.provider;
        let plan = or_dash(quota.plan.clone());
        let freshness = quota_freshness(quota);
        if quota.windows.is_empty() {
            out.push_str(&format!("{provider}\t{plan}\t-\t?\t-\t-\t{freshness}\n"));
        }
        for window in &quota.windows {
            let resets = unix_millis(window.resets_at.as_ref())
                .map(|at| fmt_minutes((at - now) as f64 / 1000.0))
                .unwrap_or_else(|| "-".into());
            let projected = window
                .estimated_unused_percent_at_reset
                .map(|value| format!("~{value:.0}%"))
                .unwrap_or_else(|| "-".into());
            out.push_str(&format!(
                "{provider}\t{plan}\t{}\t{:.1}%\t{resets}\t{projected}\t{freshness}\n",
                window.name, window.used_percent
            ));
        }
        for credit in &quota.resets {
            let expiration = unix_millis(credit.expires_at.as_ref())
                .map(|at| format!(", expires in {}", fmt_minutes((at - now) as f64 / 1000.0)))
                .unwrap_or_default();
            notes.push(format!(
                "{provider}: {} reset credit(s) [{}]{expiration}",
                credit.count, credit.kind
            ));
        }
        if !quota.models.is_empty() {
            notes.push(format!("{provider}: models {}", quota.models.join(", ")));
        }
        if !quota.error.is_empty() {
            notes.push(format!("{provider}: last read failed: {}", quota.error));
        }
    }
    if !notes.is_empty() {
        out.push('\n');
        for note in notes {
            out.push_str(&note);
            out.push('\n');
        }
    }
    align(&out)
}

pub(crate) fn suggest_text(response: &v1::SuggestResponse) -> String {
    let mut out = String::from("RANK\tHARNESS\tPICK\tSCORE\tUSABLE\tEXPIRING\tRESETS IN\tREASON\n");
    for suggestion in &response.suggestions {
        let percent = |value: Option<f64>| {
            value
                .map(|value| format!("{value:.0}%"))
                .unwrap_or_else(|| "-".into())
        };
        let resets = seconds(suggestion.resets_in.as_ref())
            .map(fmt_minutes)
            .unwrap_or_else(|| "-".into());
        out.push_str(&format!(
            "{}\t{}\t{}\t{:.1}\t{}\t{}\t{resets}\t{}\n",
            suggestion.rank,
            word(suggestion.harness().as_str_name(), "HARNESS_"),
            if suggestion.recommended { "yes" } else { "no" },
            suggestion.score,
            percent(suggestion.usable_percent),
            percent(suggestion.expiring_percent),
            suggestion.reason
        ));
    }
    align(&out)
}

pub(crate) fn node_list_text(response: &v1::ListNodesResponse) -> String {
    let nodes = &response.nodes;
    let hub_version = nodes
        .iter()
        .find(|status| status.node.as_ref().is_some_and(|node| node.local))
        .and_then(|status| status.metrics.as_ref())
        .map(|metrics| metrics.agent_version.clone())
        .unwrap_or_default();
    let mut out = String::from(NODE_COLUMNS);
    out.push('\n');
    let mut skew = false;
    for status in nodes {
        let node = status.node.clone().unwrap_or_default();
        let metrics = status.metrics.as_ref();
        let mut name = node.name.clone();
        if node.local {
            name.push('*');
        }
        let version = match metrics {
            Some(metrics) => version_cell(&metrics.agent_version, &hub_version),
            None => "-".to_owned(),
        };
        skew = skew || version.ends_with("(!)");
        let dash = || "-".to_owned();
        let line = [
            name,
            os_arch(&node.os, &node.arch),
            version,
            metrics
                .and_then(|m| m.cpu.as_ref())
                .map(|cpu| percent(cpu.usage_percent))
                .unwrap_or_else(dash),
            metrics.map(|m| gpu_summary(&m.gpus)).unwrap_or_else(dash),
            metrics
                .and_then(|m| m.memory.as_ref())
                .map(|memory| percent(memory.used_percent))
                .unwrap_or_else(dash),
            metrics.and_then(primary_disk).unwrap_or_else(dash),
            metrics
                .and_then(|m| m.engine.as_ref())
                .map(|engine| engine.containers.len().to_string())
                .unwrap_or_else(dash),
            freshness_label(status),
            alert_label(status),
        ]
        .join("\t");
        out.push_str(&line);
        out.push('\n');
    }
    out.push_str("\n* the hub's own node\n");
    if skew {
        out.push_str(&format!(
            "(!) differs from the hub's {hub_version}: protectorate node update <node> | --all\n"
        ));
    }
    align(&out)
}

pub(crate) fn node_show_text(status: &v1::NodeStatus) -> String {
    let node = status.node.clone().unwrap_or_default();
    let mut out = String::new();
    row(&mut out, "name", node.name);
    row(&mut out, "address", node.address);
    row(&mut out, "tier", node.tier);
    row(&mut out, "ssh alias", node.ssh_alias);
    row(&mut out, "tags", or_dash(node.tags.join(", ")));
    row(&mut out, "notes", node.notes);
    row(&mut out, "freshness", freshness_label(status));
    optional_row(&mut out, "last error", status.error.clone());
    for alert in &status.alerts {
        row(&mut out, "ALERT", alert.message.clone());
    }
    for busy in &status.busy_gpus {
        row(
            &mut out,
            &format!("gpu {} busy", busy.index),
            format!("{} (placement skips it; ask before using it)", busy.reason),
        );
    }
    if let Some(metrics) = &status.metrics {
        write_metrics(&mut out, metrics);
    }
    align(&out)
}

pub(crate) fn node_invite_text(response: &v1::InviteNodeResponse) -> String {
    let name = response
        .node
        .as_ref()
        .map(|node| node.name.as_str())
        .unwrap_or_default();
    let install = format!(
        "PROTECTORATE_JOIN_CODE={} protectorate-rs node install --hub {} --ca-cert ca.pem --name {}\n",
        sh_word(&response.code),
        sh_word(&response.hub_url),
        sh_word(name),
    );
    if response.commands.is_empty() {
        return format!(
            "{name} is invited. Fetch {}/ca.pem, check its SHA-256 is {}, then on the node:\n  {install}Keep the code private.\n",
            response.hub_url, response.ca_sha256,
        );
    }
    let mut out = String::new();
    for command in &response.commands {
        out.push_str(&format!(
            "{} ({}):\n  {}\n\n",
            command
                .platform()
                .as_str_name()
                .trim_start_matches("INSTALL_PLATFORM_"),
            command.shell,
            command.command
        ));
    }
    out.push_str(&format!(
        "Manual, with the Rust server and node sandbox helper already on the machine and\n{}/ca.pem saved as ca.pem after checking its SHA-256 is {}:\n  {install}",
        response.hub_url, response.ca_sha256,
    ));
    out
}

fn sh_word(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn write_metrics(out: &mut String, metrics: &v1::NodeMetrics) {
    out.push('\n');
    row(out, "hostname", metrics.hostname.clone());
    row(
        out,
        "platform",
        format!(
            "{} {}",
            os_arch(&metrics.os, &metrics.arch),
            metrics.platform
        )
        .trim()
        .to_owned(),
    );
    row(out, "agent", metrics.agent_version.clone());
    row(out, "uptime", fmt_minutes(metrics.uptime_seconds as f64));
    if let Some(cpu) = &metrics.cpu {
        let mut line = format!(
            "{:.1}% of {} logical cores",
            cpu.usage_percent, cpu.logical_cores
        );
        if let (Some(load1), Some(load5), Some(load15)) = (cpu.load1, cpu.load5, cpu.load15) {
            line.push_str(&format!(", load {load1:.2} {load5:.2} {load15:.2}"));
        }
        row(out, "cpu", line);
    }
    if let Some(jobs) = &metrics.jobs {
        let mut line = format!("sandbox {}", jobs.sandbox);
        if jobs.admin.is_empty() {
            line.push_str(", no admin jobs");
        } else {
            line.push_str(&format!(", admin via {}", jobs.admin));
        }
        if !jobs.note.is_empty() {
            line.push_str(&format!(" ({})", jobs.note));
        }
        row(out, "jobs", line);
    }
    if let Some(memory) = &metrics.memory {
        row(
            out,
            "memory",
            format!(
                "{:.1}% ({} of {} used, {} available)",
                memory.used_percent,
                bytes(memory.used_bytes),
                bytes(memory.total_bytes),
                bytes(memory.available_bytes)
            ),
        );
        if memory.swap_total_bytes > 0 {
            row(
                out,
                "swap",
                format!(
                    "{:.1}% ({} of {})",
                    memory.swap_used_percent,
                    bytes(memory.swap_used_bytes),
                    bytes(memory.swap_total_bytes)
                ),
            );
        }
    }
    if !metrics.filesystems.is_empty() {
        out.push_str("\nFILESYSTEM\tTYPE\tTOTAL\tUSED\tFREE\tUSE%\n");
        for filesystem in &metrics.filesystems {
            out.push_str(&format!(
                "{}\t{}\t{}\t{}\t{}\t{:.1}%\n",
                filesystem.mount,
                filesystem.fstype,
                bytes(filesystem.total_bytes),
                bytes(filesystem.used_bytes),
                bytes(filesystem.free_bytes),
                filesystem.used_percent
            ));
        }
    }
    if !metrics.gpus.is_empty() {
        out.push_str("\nGPU\tNAME\tTYPE\tDRIVER\tUTIL\tVRAM\tTEMP\n");
        for gpu in &metrics.gpus {
            let (kind, vram) = if gpu.integrated {
                ("integrated", "shared".to_owned())
            } else {
                (
                    "discrete",
                    or_dash(vram_label(std::slice::from_ref(gpu)).unwrap_or_default()),
                )
            };
            // Zero is what an agent without a sensor reports.
            let temp = if gpu.temperature_celsius <= 0.0 {
                "-".to_owned()
            } else {
                format!("{:.0}C", gpu.temperature_celsius)
            };
            out.push_str(&format!(
                "{}\t{}\t{kind}\t{}\t{}\t{vram}\t{temp}\n",
                gpu.index,
                format!("{} {}", gpu.vendor, gpu.name).trim(),
                or_dash(gpu.driver.clone()),
                util(gpu.utilization_percent),
            ));
        }
    }
    if !metrics.npus.is_empty() {
        out.push_str("\nNPU\tNAME\tDRIVER\tUTIL\tCLOCK\n");
        for (index, npu) in metrics.npus.iter().enumerate() {
            let clock = if npu.frequency_hz > 0 {
                format!("{:.0} MHz", npu.frequency_hz as f64 / 1e6)
            } else {
                "-".to_owned()
            };
            out.push_str(&format!(
                "{index}\t{}\t{}\t{}\t{clock}\n",
                format!("{} {}", npu.vendor, npu.name).trim(),
                or_dash(npu.driver.clone()),
                util(npu.utilization_percent),
            ));
        }
    }
    if let Some(engine) = &metrics.engine {
        write_engine(out, engine);
    }
    if !metrics.watch_dirs.is_empty() {
        out.push_str("\nWATCHED DIR\tSIZE\tMEASURED\tNOTE\n");
        for dir in &metrics.watch_dirs {
            let note = if !dir.error.is_empty() {
                dir.error.clone()
            } else if !dir.complete {
                "lower bound (time budget)".to_owned()
            } else {
                String::new()
            };
            out.push_str(&format!(
                "{}\t{}\t{} ago\t{note}\n",
                dir.path,
                bytes(dir.size_bytes.max(0) as u64),
                fmt_minutes(age_since(dir.measured_at.as_ref())),
            ));
        }
    }
    if !metrics.warnings.is_empty() {
        out.push('\n');
    }
    for warning in &metrics.warnings {
        row(out, "warning", warning.clone());
    }
}

fn write_engine(out: &mut String, engine: &v1::ContainerEngine) {
    row(
        out,
        "container engine",
        format!("{} {}", engine.kind, engine.version),
    );
    match &engine.disk {
        Some(disk) => {
            out.push_str(&format!(
                "\nENGINE DISK (measured {} ago)\tCOUNT\tSIZE\tRECLAIMABLE\n",
                fmt_minutes(age_since(engine.disk_measured_at.as_ref()))
            ));
            for (name, bucket) in [
                ("images", &disk.images),
                ("containers", &disk.containers),
                ("volumes", &disk.volumes),
                ("build cache", &disk.build_cache),
            ] {
                let bucket = bucket.clone().unwrap_or_default();
                out.push_str(&format!(
                    "{name}\t{}\t{}\t{}\n",
                    bucket.count,
                    bytes(bucket.size_bytes.max(0) as u64),
                    bytes(bucket.reclaimable_bytes.max(0) as u64)
                ));
            }
        }
        None => row(out, "engine disk", "not measured yet".to_owned()),
    }
    if engine.containers.is_empty() {
        return;
    }
    out.push_str("\nCONTAINER\tIMAGE\tSTATE\tCPU\tMEM\tMEM%\tWRITABLE\tAGE\n");
    for container in &engine.containers {
        let age = match &container.created_at {
            None => "-".to_owned(),
            at => fmt_minutes(age_since(at.as_ref())),
        };
        let writable = bytes(container.writable_layer_bytes.max(0) as u64);
        let mut cells = vec![
            container.name.clone(),
            container.image.clone(),
            container.state.clone(),
        ];
        if container.state != "running" {
            cells.extend(["-".into(), "-".into(), "-".into(), writable, age]);
        } else if !container.stats_error.is_empty() {
            cells.push(format!("stats unavailable: {}", container.stats_error));
        } else {
            cells.extend([
                format!("{:.1}%", container.cpu_percent),
                bytes(container.memory_used_bytes),
                format!("{:.1}%", container.memory_percent),
                writable,
                age,
            ]);
        }
        out.push_str(&cells.join("\t"));
        out.push('\n');
    }
}

pub(crate) fn local_metrics_text(metrics: &v1::NodeMetrics) -> String {
    let mut out = String::new();
    write_metrics(&mut out, metrics);
    if out.starts_with('\n') {
        out.remove(0);
    }
    align(&out)
}

pub(crate) fn note_list_text(response: &v1::ListNotesResponse) -> String {
    let mut out = String::new();
    for note in &response.notes {
        out.push_str(&note_line(note));
        out.push('\n');
    }
    align(&out)
}

pub(crate) fn note_text(note: &v1::Note) -> String {
    let mut out = String::new();
    row(&mut out, "id", note.id.clone());
    row(&mut out, "title", note.title.clone());
    optional_row(&mut out, "project", note.project.clone());
    row(&mut out, "source", note.source.clone());
    row(&mut out, "state", note.state.clone());
    if let Some(completion) = &note.completion {
        row(&mut out, "done at", minute(completion.done_at.as_ref()));
        row(&mut out, "done by", completion.done_by.clone());
        optional_row(&mut out, "resolution", completion.resolution.clone());
    }
    row(&mut out, "pinned", note.pinned.to_string());
    row(&mut out, "created", minute(note.created_at.as_ref()));
    row(&mut out, "updated", minute(note.updated_at.as_ref()));
    let body = note.body.trim_end_matches('\n').replace('\t', "    ");
    if !body.is_empty() {
        out.push('\n');
        out.push_str(&body);
        out.push('\n');
    }
    align(&out)
}

fn note_line(note: &v1::Note) -> String {
    let title = if note.title.is_empty() {
        first_line(&note.body, 80)
    } else {
        note.title.clone()
    };
    [
        note.id.clone(),
        or_dash(note.state.clone()),
        if note.pinned { "pin" } else { "-" }.to_owned(),
        minute(note.updated_at.as_ref()),
        or_dash(note.project.clone()),
        or_dash(note.source.clone()),
        title,
    ]
    .join("\t")
}

/// One note record, the way `backlog add`, `done` and `reopen` print the
/// note the hub just created or changed.
pub(crate) fn note_record_text(note: &v1::Note) -> String {
    align(&note_line(note))
}

pub(crate) fn lease_list_text(response: &v1::ListLeasesResponse, now: i64) -> String {
    let mut out = String::from("ID\tRESOURCE\tHOLDER\tEXPIRES IN\n");
    for lease in &response.leases {
        let expires = if lease.state() == v1::LeaseState::Queued {
            format!("queued #{}", lease.queue_position)
        } else {
            let expires = unix_millis(lease.expires_at.as_ref()).unwrap_or(now);
            go_duration((expires - now) as f64 / 1000.0)
        };
        out.push_str(&format!(
            "{}\t{}\t{}\t{expires}\n",
            lease.id, lease.resource, lease.holder
        ));
    }
    align(&out)
}

pub(crate) fn lease_text(lease: &v1::Lease) -> String {
    let mut out = String::new();
    if lease.state() == v1::LeaseState::Queued {
        row(&mut out, "ticket", lease.id.clone());
        row(&mut out, "resource", lease.resource.clone());
        row(&mut out, "holder", lease.holder.clone());
        row(&mut out, "position", lease.queue_position.to_string());
    } else {
        row(&mut out, "lease", lease.id.clone());
        row(&mut out, "resource", lease.resource.clone());
        row(&mut out, "holder", lease.holder.clone());
        row(&mut out, "expires", stamp(lease.expires_at.as_ref()));
    }
    align(&out)
}

pub(crate) fn job_list_text(response: &v1::ListJobsResponse, now: i64) -> String {
    let mut out = String::from("ID\tSTATE\tNODE\tAGE\tRUN\tCOMMAND\n");
    for job in &response.jobs {
        let age = match unix_millis(job.submitted_at.as_ref()) {
            Some(millis) => fmt_minutes((now - millis) as f64 / 1000.0),
            None => "-".to_owned(),
        };
        out.push_str(&format!(
            "{}\t{}\t{}\t{age}\t{}\t{}\n",
            job.id,
            job_state_label(job),
            or_dash(job.node.clone()),
            go_duration(seconds(job.run_time.as_ref()).unwrap_or_default()),
            truncate(&job.command.join(" "), 48),
        ));
    }
    align(&out)
}

pub(crate) fn job_text(job: &v1::Job) -> String {
    let mut out = String::new();
    row(&mut out, "id", job.id.clone());
    row(&mut out, "state", job_state_label(job));
    row(&mut out, "node", job.node.clone());
    row(&mut out, "command", job.command.join(" "));
    row(&mut out, "mode", job_mode_label(job));
    row(&mut out, "isolation", job.isolation.clone());
    optional_row(&mut out, "task", job.task.clone());
    optional_row(&mut out, "source", job_source_label(job));
    row(&mut out, "submitted", stamp(job.submitted_at.as_ref()));
    row(&mut out, "started", stamp(job.started_at.as_ref()));
    row(&mut out, "finished", stamp(job.finished_at.as_ref()));
    if let Some(gpu) = job.gpu_index {
        row(&mut out, "gpu", gpu.to_string());
    }
    // A job that has not run yet has no exit code.
    if job.exit_code != 0
        || !matches!(
            job.state(),
            v1::JobState::Unspecified | v1::JobState::Preparing | v1::JobState::Running
        )
    {
        row(&mut out, "exit code", job.exit_code.to_string());
    }
    optional_row(&mut out, "error", job.error.clone());
    row(
        &mut out,
        "source sent",
        format!(
            "{} in {} (warm worktree: {})",
            bytes(job.source_bytes),
            go_duration(seconds(job.transfer_time.as_ref()).unwrap_or_default()),
            job.workspace_reused
        ),
    );
    row(
        &mut out,
        "run time",
        go_duration(seconds(job.run_time.as_ref()).unwrap_or_default()),
    );
    row(&mut out, "log", bytes(job.log_bytes));
    for artifact in &job.artifacts {
        row(
            &mut out,
            "artifact",
            format!("{} ({})", artifact.path, bytes(artifact.size_bytes)),
        );
    }
    align(&out)
}

pub(crate) fn job_cancel_text(job: &v1::Job) -> String {
    format!("job {} {}\n", job.id, job_state_label(job))
}

/// The one-line summary `job run` prints after the log. Durations are rounded
/// to a millisecond (`1m2.5s`).
pub(crate) fn job_summary_line(job: &v1::Job) -> String {
    let mut line = format!("job {} {}", job.id, job_state_label(job));
    if matches!(job.state(), v1::JobState::Succeeded | v1::JobState::Failed) {
        line.push_str(&format!(" (exit {})", job.exit_code));
    }
    line.push_str(&format!(
        " on {}: run {}",
        job.node,
        round_go_duration(seconds(job.run_time.as_ref()).unwrap_or_default())
    ));
    if job
        .source
        .as_ref()
        .is_some_and(|source| source.kind.is_some())
    {
        line.push_str(&format!(
            ", source {} in {}",
            bytes(job.source_bytes),
            round_go_duration(seconds(job.transfer_time.as_ref()).unwrap_or_default())
        ));
        if job.workspace_reused {
            line.push_str(", warm worktree");
        }
    }
    if !job.artifacts.is_empty() {
        line.push_str(&format!(
            ", {} artifact(s) {}",
            job.artifacts.len(),
            bytes(job.artifact_bytes)
        ));
    }
    if !job.error.is_empty() {
        line.push_str(": ");
        line.push_str(&job.error);
    }
    line
}

/// `time.Duration.String` after rounding to the nearest millisecond.
fn round_go_duration(seconds: f64) -> String {
    if !seconds.is_finite() {
        return "0s".to_owned();
    }
    let millis = (seconds * 1000.0).round();
    if !millis.is_finite() {
        return "0s".to_owned();
    }
    go_millis(millis as i64)
}

fn go_millis(millis: i64) -> String {
    let negative = millis < 0;
    let millis = millis.unsigned_abs();
    let mut text = if negative {
        "-".to_owned()
    } else {
        String::new()
    };
    if millis < 1000 {
        text.push_str(&format!("{millis}ms"));
        if millis == 0 {
            return "0s".to_owned();
        }
        return text;
    }
    let mut seconds = millis / 1000;
    let fraction = millis % 1000;
    let hours = seconds / 3600;
    seconds %= 3600;
    let minutes = seconds / 60;
    seconds %= 60;
    if hours > 0 {
        text.push_str(&format!("{hours}h"));
    }
    if hours > 0 || minutes > 0 {
        text.push_str(&format!("{minutes}m"));
    }
    if fraction == 0 {
        text.push_str(&format!("{seconds}s"));
    } else {
        let mut frac = format!("{fraction:03}");
        while frac.ends_with('0') {
            frac.pop();
        }
        text.push_str(&format!("{seconds}.{frac}s"));
    }
    text
}

pub(crate) fn chat_send_text(response: &v1::SendResponse) -> String {
    format!("seq\t{}\n", response.seq)
}

/// `chat wait` rows: the sleeve and its state, then the summary
/// when the wait ended on a report.
pub(crate) fn chat_wait_text(response: &v1::WaitResponse) -> String {
    match &response.result {
        None => format!("{}\tidle\n", response.sleeve),
        Some(result) => format!(
            "{}\t{}\n{}\n",
            response.sleeve,
            result_status(result),
            result.summary
        ),
    }
}

/// What `msg send` prints: the message the hub created, then what the hub did
/// around it - a retry that matched an earlier request, mail waiting for a
/// project's sleeves, and the backlog note a request was filed as.
pub(crate) fn msg_send_text(response: &v1::MessageServiceSendResponse) -> String {
    let message = response.message.clone().unwrap_or_default();
    let mut out = String::new();
    row(&mut out, "message", message.id);
    row(&mut out, "thread", message.thread_id);
    if response.duplicate {
        out.push_str("note\tsame request id as an earlier send; nothing was sent again\n");
    }
    if response.waiting {
        out.push_str("waiting\tno live recipient yet; it is delivered when one appears\n");
    }
    optional_row(&mut out, "note", message.note_id);
    align(&out)
}

/// What `msg reply` prints: the answer, its thread and who it went to.
pub(crate) fn msg_reply_text(message: &v1::MailMessage) -> String {
    let mut out = String::new();
    row(&mut out, "message", message.id.clone());
    row(&mut out, "thread", message.thread_id.clone());
    row(&mut out, "to", message.to.clone());
    align(&out)
}

pub(crate) fn msg_ack_text(message: &v1::MailMessage) -> String {
    align(&format!("acked\t{}\n", message.id))
}

/// One block per message: a header row with its id, thread, direction, kind
/// and state, then the body indented. The hub's own guidance heads the list.
pub(crate) fn mail_text(messages: &[v1::MailMessage]) -> String {
    if messages.is_empty() {
        return "no messages\n".to_owned();
    }
    let mut out = String::new();
    out.push_str(protectorate_mailbox::CONCISE_GUIDANCE);
    out.push('\n');
    for message in messages {
        out.push_str(&format!(
            "{}\t{}\t{} -> {}\t{}{}\t{}\n",
            message.id,
            message.thread_id,
            message.from,
            message.to,
            message_kind_label(message),
            mail_flags(message),
            stamp(message.created_at.as_ref()),
        ));
        for line in message.body.trim_end_matches('\n').split('\n') {
            out.push_str(&format!("\t  {line}\n"));
        }
    }
    align(&out)
}

fn message_kind_label(message: &v1::MailMessage) -> String {
    match message.kind() {
        v1::MessageKind::Unspecified => "note".to_owned(),
        kind => word(kind.as_str_name(), "MESSAGE_KIND_"),
    }
}

/// The state appended to a message's kind: who has to ack, who already did,
/// who was shown it, and whether it was escalated.
fn mail_flags(message: &v1::MailMessage) -> String {
    let mut flags = String::new();
    if message.ack_required {
        flags.push_str(" ack-required");
    }
    for delivery in &message.deliveries {
        if delivery.acked_at.is_some() {
            flags.push_str(&format!(" acked:{}", delivery.recipient));
        } else if delivery.delivered_at.is_some() {
            flags.push_str(&format!(" delivered:{}", delivery.recipient));
        }
    }
    if message.escalated_at.is_some() {
        flags.push_str(" escalated");
    }
    flags
}

/// `msg threads` rows, most recently active first.
pub(crate) fn thread_list_text(response: &v1::ListThreadsResponse) -> String {
    if response.threads.is_empty() {
        return "no threads\n".to_owned();
    }
    let mut out = String::new();
    for thread in &response.threads {
        let flags: Vec<String> = [
            (thread.unread, "unread"),
            (thread.open_acks, "open-acks"),
            (thread.escalated, "escalated"),
        ]
        .into_iter()
        .filter(|(count, _)| *count > 0)
        .map(|(count, name)| format!("{count} {name}"))
        .collect();
        out.push_str(&format!(
            "{}\t{} msgs\t{}\t{}\t{}\n",
            thread.thread_id,
            thread.message_count,
            thread.participants.join(","),
            flags.join(", "),
            stamp(
                thread
                    .last
                    .as_ref()
                    .and_then(|last| last.created_at.as_ref())
            ),
        ));
    }
    align(&out)
}

/// What `msg wait` prints: the message the wait ended on, or the timeout.
pub(crate) fn msg_wait_text(response: &v1::MessageServiceWaitResponse) -> String {
    if response.timed_out {
        return "timed out\n".to_owned();
    }
    mail_text(&[response.message.clone().unwrap_or_default()])
}

pub(crate) fn sleeve_workspaces_text(response: &v1::ListWorkspacesResponse) -> String {
    let mut out = String::from("NAME\tBRANCH\tREMOTE\tSLEEVE\tNAME REQUIRED\tPATH\tREAD ONLY\n");
    for workspace in &response.workspaces {
        let mut sleeve = workspace.sleeve.clone();
        if workspace.busy {
            sleeve.push_str(" (running)");
        }
        out.push_str(&format!(
            "{}\t{}\t{}\t{sleeve}\t{}\t{}\t{}\n",
            workspace.name,
            workspace.git_branch,
            workspace.git_remote,
            workspace.required_name,
            workspace.path,
            workspace.read_only,
        ));
    }
    align(&out)
}

pub(crate) fn sleeve_browse_text(response: &v1::BrowseWorkspacesResponse) -> String {
    let mut out = String::from("NAME\tGIT\tBRANCH\tREMOTE\tACCEPTABLE\tREASON\tPATH\n");
    for directory in &response.directories {
        out.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            directory.name,
            directory.is_git,
            directory.git_branch,
            directory.git_remote,
            directory.acceptable,
            directory.reason,
            directory.path,
        ));
    }
    if response.truncated {
        out.push_str("Listing truncated\n");
    }
    align(&out)
}

pub(crate) fn sleeve_list_text(response: &v1::ListSleevesResponse) -> String {
    let mut out = String::from(
        "NAME\tHARNESS\tMODEL\tEFFORT\tSTATE\tCPU\tMEM\tPEAK\tHOME\tLIMITS\tWORKSPACE\tPARENT\tBRANCH\n",
    );
    for sleeve in &response.sleeves {
        let usage = sleeve.usage.clone().unwrap_or_default();
        let (model, effort) = match &sleeve.chat {
            Some(chat) => (or_unknown(&chat.model), or_unknown(&chat.reasoning_effort)),
            None => ("-".to_owned(), "-".to_owned()),
        };
        out.push_str(
            &[
                sleeve.name.clone(),
                harness_label(sleeve),
                model,
                effort,
                sleeve_row_state(sleeve),
                percent(usage.cpu_percent),
                bytes(usage.memory_current_bytes),
                bytes(usage.memory_peak_bytes),
                bytes(usage.home_disk_bytes),
                limit_label(sleeve),
                sleeve.workspace.clone(),
                sleeve.parent.clone(),
                sleeve.git_branch.clone(),
            ]
            .join("\t"),
        );
        out.push('\n');
    }
    align(&out)
}

/// The STATE column of a session: "running", "exited(3)" or
/// "exited(signal 9)".
fn session_state_label(session: &v1::Session) -> String {
    if session.state() != v1::SessionState::Exited {
        "running".to_owned()
    } else if session.exit_signal > 0 {
        format!("exited(signal {})", session.exit_signal)
    } else {
        format!("exited({})", session.exit_code)
    }
}

pub(crate) fn session_list_text(response: &v1::ListSessionsResponse) -> String {
    let mut out = String::from("NAME\tSTATE\tCLIENTS\tSIZE\tACTIVE\tCWD\tCOMMAND\n");
    for session in &response.sessions {
        out.push_str(&format!(
            "{}\t{}\t{}\t{}x{}\t{}\t{}\t{}\n",
            session.name,
            session_state_label(session),
            session.attached_clients,
            session.pane_cols,
            session.pane_rows,
            local_clock(session.last_activity.as_ref()),
            session.cwd,
            session.command.join(" "),
        ));
    }
    align(&out)
}

/// `chat settings` rows: the effective model and effort, the
/// requested selection, and the native defaults the harness captured before
/// any override.
pub(crate) fn chat_settings_text(chat: &v1::ChatStatus) -> String {
    let settings = chat.settings.clone().unwrap_or_default();
    let requested = settings.requested.unwrap_or_default();
    let native = settings.native_defaults.unwrap_or_default();
    let filled = |value: &str, fallback: &str| {
        if value.is_empty() {
            fallback.to_owned()
        } else {
            value.to_owned()
        }
    };
    format!(
        "model\t{}\neffort\t{}\nrequested model\t{}\nrequested effort\t{}\nnative model\t{}\nnative effort\t{}\n",
        filled(&chat.model, "unknown"),
        filled(&chat.reasoning_effort, "unknown"),
        filled(&requested.model, "native default"),
        filled(&requested.effort, "native default"),
        filled(&native.model, "unknown"),
        filled(&native.effort, "unknown"),
    )
}

fn token_counts_text(counts: &v1::TokenCounts) -> String {
    format!(
        "{} calls, in {} out {} read {} write {}, {} read",
        counts.calls,
        counts.input_tokens,
        counts.output_tokens,
        counts.cache_read_tokens,
        counts.cache_write_tokens,
        percent(counts.read_ratio * 100.0),
    )
}

/// What a chat is doing, in words.
fn chat_state_label(state: v1::ChatState) -> &'static str {
    match state {
        v1::ChatState::Unspecified => "unknown",
        v1::ChatState::AwaitingApproval => "awaiting approval",
        v1::ChatState::Working => "working",
        v1::ChatState::Compacting => "compacting",
        v1::ChatState::Idle => "idle",
        v1::ChatState::Failed => "failed",
        v1::ChatState::Exited => "exited",
    }
}

/// Sequence, activity and prompt-cache rows of a chat status.
fn chat_tail_text(chat: &v1::ChatStatus) -> String {
    let mut out = String::new();
    row(&mut out, "last sequence", chat.last_seq.to_string());
    if chat.last_activity_at.is_some() {
        row(
            &mut out,
            "last activity",
            stamp(chat.last_activity_at.as_ref()),
        );
    }
    if let Some(cache) = &chat.cache {
        if let Some(total) = cache.total.as_ref().filter(|total| total.calls > 0) {
            row(&mut out, "cache total", token_counts_text(total));
        }
        if let Some(first) = &cache.first_call {
            let verdict = if cache.cold_resume {
                "resumed but nothing was read from the cache"
            } else if cache.resumed {
                "resumed"
            } else {
                "fresh"
            };
            row(
                &mut out,
                "cache first call",
                format!("{}, {verdict}", token_counts_text(first)),
            );
        }
    }
    out
}

/// What `chat status` prints: state, context size and cache telemetry.
pub(crate) fn chat_status_text(chat: &v1::ChatStatus) -> String {
    let mut out = String::new();
    row(
        &mut out,
        "chat state",
        chat_state_label(chat.state()).into(),
    );
    row(&mut out, "model", chat.model.clone());
    row(
        &mut out,
        "context tokens",
        format!("{}/{}", chat.context_tokens, chat.context_window),
    );
    out.push_str(&chat_tail_text(chat));
    align(&out)
}

/// A report's status word: complete, partial or blocked.
fn result_status(result: &v1::SleeveResult) -> String {
    word(result.status().as_str_name(), "REPORT_STATUS_")
}

pub(crate) fn sleeve_text(sleeve: &v1::Sleeve, tail: Option<&str>) -> String {
    let mut out = String::new();
    row(&mut out, "name", sleeve.name.clone());
    row(&mut out, "harness", harness_label(sleeve));
    row(&mut out, "state", sleeve_row_state(sleeve));
    row(&mut out, "workspace", sleeve.workspace.clone());
    row(&mut out, "session", sleeve.session.clone());
    row(&mut out, "branch", sleeve.git_branch.clone());
    if !sleeve.base_repo.is_empty() {
        row(&mut out, "base repo", sleeve.base_repo.clone());
        row(&mut out, "base branch", sleeve.base_branch.clone());
        row(&mut out, "base tip", sleeve.base_tip.clone());
    }
    row(&mut out, "parent", sleeve.parent.clone());
    optional_row(&mut out, "run", sleeve.run_id.clone());
    optional_row(&mut out, "harness sha256", sleeve.harness_sha256.clone());
    optional_row(&mut out, "profile sha256", sleeve.profile_sha256.clone());
    if sleeve.token_budget > 0 {
        row(
            &mut out,
            "token budget",
            format!(
                "{} of {} weighted tokens used",
                sleeve.tokens_used, sleeve.token_budget
            ),
        );
    }
    row(&mut out, "mode", mode_label(sleeve));
    if let Some(result) = &sleeve.result {
        out.push_str(&format!(
            "report\t{} (chat seq {}, {})\n{}\n",
            result_status(result),
            result.chat_seq,
            result.time.map(|time| time.to_string()).unwrap_or_default(),
            result.summary
        ));
    }
    row(&mut out, "nested", sleeve.nested.to_string());
    row(&mut out, "created", stamp(sleeve.created_at.as_ref()));
    if sleeve.exited_at.is_some() {
        row(&mut out, "exited", stamp(sleeve.exited_at.as_ref()));
    }
    let exited = sleeve.state() == v1::SleeveState::Exited;
    if exited {
        row(&mut out, "exit", exit_label(sleeve));
        row(&mut out, "exit signal", sleeve.exit_signal.to_string());
        row(&mut out, "oom killed", sleeve.oom_killed.to_string());
    }
    row(&mut out, "args", sleeve.args.join(" "));
    row(&mut out, "limits", limit_label(sleeve));
    row(
        &mut out,
        "exit after idle",
        go_duration(seconds(sleeve.exit_after_idle.as_ref()).unwrap_or_default()),
    );
    for mount in &sleeve.read_only {
        row(
            &mut out,
            "read-only mount",
            format!("{} -> {}", mount.source, mount.target),
        );
    }
    if sleeve.supervised {
        out.push_str("supervised\tyes (the harness asks; answer with chat respond)\n");
    } else {
        out.push_str("supervised\tno\n");
    }
    if let Some(chat) = &sleeve.chat {
        out.push_str(&chat_settings_text(chat));
        row(
            &mut out,
            "chat state",
            chat_state_label(chat.state()).into(),
        );
        row(&mut out, "harness session", chat.harness_session_id.clone());
        row(&mut out, "provider", chat.provider.clone());
        row(&mut out, "local model", chat.local_model.to_string());
        row(
            &mut out,
            "context tokens",
            format!("{}/{}", chat.context_tokens, chat.context_window),
        );
        out.push_str(&chat_tail_text(chat));
    }
    if let Some(handoff) = &sleeve.handoff {
        append_handoff(&mut out, handoff);
    }
    let usage = sleeve.usage.clone().unwrap_or_default();
    row(
        &mut out,
        "cpu",
        format!(
            "{:.1}% ({:.1}s total)",
            usage.cpu_percent, usage.cpu_seconds
        ),
    );
    row(
        &mut out,
        "memory",
        format!(
            "{} (peak {})",
            bytes(usage.memory_current_bytes),
            bytes(usage.memory_peak_bytes)
        ),
    );
    row(&mut out, "pids", usage.pids.to_string());
    row(&mut out, "oom kills", usage.oom_kills.to_string());
    row(
        &mut out,
        "workspace disk",
        bytes(usage.workspace_disk_bytes),
    );
    row(
        &mut out,
        "home disk",
        format!(
            "{} (cache {}, rest {})",
            bytes(usage.home_disk_bytes),
            bytes(usage.cache_disk_bytes),
            bytes(usage.home_disk_bytes.saturating_sub(usage.cache_disk_bytes))
        ),
    );
    row(
        &mut out,
        "go build cache",
        bytes(usage.go_build_cache_disk_bytes),
    );
    if usage.sampled_at.is_some() {
        row(&mut out, "usage sampled", stamp(usage.sampled_at.as_ref()));
    }
    if usage.disk_sampled_at.is_some() {
        row(
            &mut out,
            "disk sampled",
            stamp(usage.disk_sampled_at.as_ref()),
        );
    }
    match tail {
        Some(tail) => out.push_str(&format!(
            "\nlast {SHOW_TAIL_LINES} lines of output:\n{tail}\n"
        )),
        None if exited => out.push_str("\n(no output kept: the session is gone)\n"),
        None => {}
    }
    align(&out)
}

/// What `sleeve spawn` prints: the name, then how to drive the new sleeve.
pub(crate) fn sleeve_spawn_text(sleeve: &v1::Sleeve) -> String {
    let name = &sleeve.name;
    let next = if sleeve.mode() == v1::SleeveMode::Chat {
        format!("chat\tprotectorate chat send {name} \"...\"; protectorate chat watch {name}\n")
    } else {
        format!("attach\tprotectorate sleeve attach {name}\n")
    };
    align(&format!("spawned\t{name}\n{next}"))
}

/// What `sleeve envoy` prints. `replaced` chooses spawned or respawned.
pub(crate) fn sleeve_envoy_text(response: &v1::SpawnEnvoyResponse) -> String {
    let sleeve = response.sleeve.clone().unwrap_or_default();
    let token = response.token.clone().unwrap_or_default();
    let name = &sleeve.name;
    let verb = if response.replaced {
        "respawned"
    } else {
        "spawned"
    };
    align(&format!(
        "{verb}\t{name}\nharness\t{} ({})\nworkspace\t{}\ntoken\t{}\nscopes\t{}\nchat\tprotectorate chat watch {name}\n",
        harness_label(&sleeve),
        response.reason,
        response.workspace,
        token.id,
        token.scopes.join(", "),
    ))
}

/// What `sleeve resleeve` prints: the name and the handoff the hub made.
pub(crate) fn sleeve_resleeve_text(sleeve: &v1::Sleeve) -> String {
    let mut out = format!("resleeved\t{}\n", sleeve.name);
    if let Some(handoff) = &sleeve.handoff {
        append_handoff(&mut out, handoff);
    }
    align(&out)
}

/// What `sleeve prune` prints. `yes` is the flag the CLI sent: without it
/// the hub only inventoried, and the footer says so.
pub(crate) fn sleeve_prune_text(response: &v1::PruneSleevesResponse, yes: bool) -> String {
    let mut out = String::from("SLEEVE\tKIND\tSIZE\tACTION\tPATH\tREASON\n");
    for entry in &response.entries {
        let action = if entry.removed {
            "removed"
        } else if entry.eligible {
            "would remove"
        } else {
            "keep"
        };
        out.push_str(&format!(
            "{}\t{}\t{}\t{action}\t{}\t{}\n",
            entry.sleeve,
            entry.kind,
            bytes(entry.bytes),
            entry.path,
            entry.reason,
        ));
    }
    if !yes {
        out.push_str("Dry run; pass --yes to remove eligible entries.\n");
    }
    align(&out)
}

fn append_handoff(out: &mut String, handoff: &v1::Handoff) {
    row(
        out,
        "resleeved from",
        format!(
            "{} (session {}) to {} at {}",
            handoff.from_harness,
            handoff.from_session_id,
            handoff.to_harness,
            stamp(handoff.at.as_ref())
        ),
    );
    let by = [
        handoff.summarizer_harness.as_str(),
        handoff.summarizer_provider.as_str(),
        handoff.summarizer_model.as_str(),
    ]
    .join(" ");
    let by = by.split_whitespace().collect::<Vec<_>>().join(" ");
    if handoff.summarizer_harness.is_empty() {
        row(out, "summary", format!("none: {}", handoff.summary_error));
    } else if handoff.summarizer_local {
        row(
            out,
            "summary",
            format!("by local model {by} (check it against the facts)"),
        );
    } else {
        row(out, "summary", format!("by {by}"));
    }
    row(out, "handoff", handoff.path.clone());
    optional_row(out, "archive", handoff.archive_path.clone());
    if !handoff.summarizer_harness.is_empty() {
        optional_row(out, "summary error", handoff.summary_error.clone());
    }
}

/// The last `count` lines of a captured pane, with ANSI SGR escapes removed
/// and tabs expanded.
pub(crate) fn pane_tail(text: &str, count: usize) -> String {
    let text = strip_sgr(text).replace('\t', "    ");
    let lines: Vec<&str> = text
        .trim_end_matches([' ', '\t', '\r', '\n'])
        .split('\n')
        .collect();
    lines[lines.len().saturating_sub(count)..].join("\n")
}

/// Removes `ESC [ params m` sequences (`\x1b\[[0-9;]*m`).
fn strip_sgr(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("\u{1b}[") {
        let bytes = rest.as_bytes();
        let mut end = start + 2;
        while end < bytes.len() && (bytes[end].is_ascii_digit() || bytes[end] == b';') {
            end += 1;
        }
        if end < bytes.len() && bytes[end] == b'm' {
            out.push_str(&rest[..start]);
            rest = &rest[end + 1..];
        } else {
            out.push_str(&rest[..end]);
            rest = &rest[end..];
        }
    }
    out.push_str(rest);
    out
}

fn harness_label(sleeve: &v1::Sleeve) -> String {
    match sleeve.harness() {
        v1::Harness::Unspecified => "?".to_owned(),
        harness => word(harness.as_str_name(), "HARNESS_"),
    }
}

fn terminal_activity(activity: v1::TerminalActivity) -> String {
    word(activity.as_str_name(), "TERMINAL_ACTIVITY_")
}

/// What `sleeve status` prints: the activity and deciding rule, and with
/// `explain` every rule of the harness and the screen lines they read.
pub(crate) fn terminal_status_text(
    response: &v1::GetTerminalStatusResponse,
    explain: bool,
) -> String {
    let status = response.status.clone().unwrap_or_default();
    let activity = terminal_activity(status.activity());
    let mut out = String::new();
    row(
        &mut out,
        "activity",
        if activity.is_empty() {
            "unknown".to_owned()
        } else {
            activity
        },
    );
    optional_row(&mut out, "rule", status.rule);
    let mut out = align(&out);
    if explain {
        out.push_str("\nrules, first match decides\n");
        for rule in &response.rules {
            out.push_str(&format!(
                "  {}\t{}\t{}\n",
                if rule.matched { "match" } else { "-" },
                terminal_activity(rule.activity()),
                rule.id,
            ));
        }
        out.push_str("\nscreen lines read\n");
        for line in &response.lines {
            out.push_str(&format!("  {line}\n"));
        }
    }
    out
}

/// The STATE column: "killed", "running", "running (idle)", "exited",
/// "exited(3)" or "exited(137,oom)".
fn sleeve_row_state(sleeve: &v1::Sleeve) -> String {
    if sleeve.killed {
        return "killed".to_owned();
    }
    if sleeve.state() == v1::SleeveState::Running {
        let activity = sleeve
            .terminal
            .as_ref()
            .map(|terminal| terminal_activity(terminal.activity()))
            .unwrap_or_default();
        return if activity.is_empty() {
            "running".to_owned()
        } else {
            format!("running ({activity})")
        };
    }
    match sleeve.exit_code {
        None => "exited".to_owned(),
        Some(code) if sleeve.oom_killed => format!("exited({code},oom)"),
        Some(code) => format!("exited({code})"),
    }
}

/// The LIMITS column: "cpu/mem", each "-" when unlimited.
fn limit_label(sleeve: &v1::Sleeve) -> String {
    let limits = sleeve.limits.clone().unwrap_or_default();
    let cpu = if limits.cpu_percent > 0 {
        format!("{}%", limits.cpu_percent)
    } else {
        "-".to_owned()
    };
    let mem = if limits.memory_bytes > 0 {
        bytes(limits.memory_bytes)
    } else {
        "-".to_owned()
    };
    format!("{cpu}/{mem}")
}

fn mode_label(sleeve: &v1::Sleeve) -> String {
    match sleeve.mode() {
        v1::SleeveMode::Unspecified => "unspecified".to_owned(),
        mode => word(mode.as_str_name(), "SLEEVE_MODE_"),
    }
}

/// How an exited sleeve ended: "exit 3", "killed by signal 9 (exit 137)" or
/// "exit 137 (OOM killed)".
fn exit_label(sleeve: &v1::Sleeve) -> String {
    let signal = sleeve.exit_signal;
    let mut label = match sleeve.exit_code {
        None => "unknown".to_owned(),
        Some(code) if signal > 0 => format!("killed by signal {signal} (exit {code})"),
        Some(code) => format!("exit {code}"),
    };
    if sleeve.oom_killed {
        label.push_str(" (OOM killed)");
    }
    label
}

fn or_unknown(value: &str) -> String {
    if value.is_empty() {
        "unknown".to_owned()
    } else {
        value.to_owned()
    }
}

fn job_state_label(job: &v1::Job) -> String {
    match word(job.state().as_str_name(), "JOB_STATE_") {
        state if state.is_empty() => "-".to_owned(),
        state => state.replace('_', " "),
    }
}

fn job_mode_label(job: &v1::Job) -> String {
    match job.mode() {
        v1::JobMode::Unspecified => "sandboxed".to_owned(),
        mode => word(mode.as_str_name(), "JOB_MODE_"),
    }
}

fn job_source_label(job: &v1::Job) -> String {
    use v1::job_source::Kind;
    match job.source.as_ref().and_then(|source| source.kind.as_ref()) {
        Some(Kind::Git(git)) => format!("git {}@{}", git.repo, git.commit),
        Some(Kind::Artifact(artifact)) => format!("artifact {}", artifact.path),
        None => String::new(),
    }
}

fn truncate(text: &str, limit: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= limit {
        return text.to_owned();
    }
    format!("{}~", chars[..limit - 1].iter().collect::<String>())
}

fn or_dash(value: String) -> String {
    if value.is_empty() {
        "-".to_owned()
    } else {
        value
    }
}

fn row(out: &mut String, label: &str, value: String) {
    out.push_str(&format!("{label}\t{}\n", or_dash(value)));
}

fn optional_row(out: &mut String, label: &str, value: String) {
    if !value.is_empty() {
        row(out, label, value);
    }
}

fn percent(value: f64) -> String {
    format!("{value:.0}%")
}

fn util(value: f64) -> String {
    if value < 0.0 {
        "-".to_owned()
    } else {
        format!("{value:.0}%")
    }
}

pub(crate) fn bytes(value: u64) -> String {
    const UNIT: f64 = 1024.0;
    let mut scaled = value as f64;
    if scaled < UNIT {
        return format!("{value}B");
    }
    for unit in ["KiB", "MiB", "GiB", "TiB", "PiB"] {
        scaled /= UNIT;
        if scaled < UNIT || unit == "PiB" {
            return format!("{scaled:.1}{unit}");
        }
    }
    String::new()
}

/// Whole seconds as `1h2m3s`, `2m3s` or `3s`.
fn go_duration(seconds: f64) -> String {
    let total = seconds.round();
    let negative = total < 0.0;
    let total = total.abs() as i64;
    let (hours, minutes, seconds) = (total / 3600, (total % 3600) / 60, total % 60);
    let text = if hours > 0 {
        format!("{hours}h{minutes}m{seconds}s")
    } else if minutes > 0 {
        format!("{minutes}m{seconds}s")
    } else {
        format!("{seconds}s")
    };
    if negative {
        format!("-{text}")
    } else {
        text
    }
}

/// A duration rounded to whole minutes, for node age.
fn fmt_minutes(seconds: f64) -> String {
    let minutes = (seconds / 60.0).round() as i64;
    if minutes <= 0 {
        return "0m".to_owned();
    }
    let hours = minutes / 60;
    if hours == 0 {
        return format!("{minutes}m");
    }
    if hours < 48 {
        return format!("{hours}h{:02}m", minutes % 60);
    }
    format!("{}d{:02}h", hours / 24, hours % 24)
}

fn version_cell(node_version: &str, hub_version: &str) -> String {
    match node_version {
        "" => "-".to_owned(),
        version if !hub_version.is_empty() && version != hub_version => format!("{version} (!)"),
        version => version.to_owned(),
    }
}

fn primary_disk(metrics: &v1::NodeMetrics) -> Option<String> {
    metrics
        .filesystems
        .iter()
        .find(|filesystem| filesystem.primary)
        .map(|filesystem| format!("{:.0}% {}", filesystem.used_percent, filesystem.mount))
}

fn vram_label(gpus: &[v1::GpuMetrics]) -> Option<String> {
    let mut used = 0u64;
    let mut total = 0u64;
    let mut known = true;
    for gpu in gpus {
        if gpu.integrated || gpu.memory_total_bytes == 0 {
            continue;
        }
        total += gpu.memory_total_bytes;
        match gpu.memory_used_bytes {
            Some(value) => used += value,
            None => known = false,
        }
    }
    if total == 0 {
        return None;
    }
    if known {
        Some(format!("{}/{}", bytes(used), bytes(total)))
    } else {
        Some(format!("?/{}", bytes(total)))
    }
}

fn gpu_summary(gpus: &[v1::GpuMetrics]) -> String {
    let Some(first) = gpus.first() else {
        return "-".to_owned();
    };
    let mut summary = format!("{}x {}", gpus.len(), util(first.utilization_percent));
    if let Some(vram) = vram_label(gpus) {
        summary.push(' ');
        summary.push_str(&vram);
    }
    summary
}

fn freshness_label(status: &v1::NodeStatus) -> String {
    let age = seconds(status.age.as_ref())
        .map(|age| format!(" ({} old)", fmt_minutes(age)))
        .unwrap_or_default();
    match status.freshness() {
        v1::NodeFreshness::Fresh => "fresh".to_owned(),
        v1::NodeFreshness::Stale => format!("STALE{age}"),
        v1::NodeFreshness::Invited => "invited, not yet joined".to_owned(),
        v1::NodeFreshness::Unreachable if status.metrics.is_some() => {
            format!("UNREACHABLE{age}")
        }
        v1::NodeFreshness::Unreachable => "UNREACHABLE (never seen)".to_owned(),
        v1::NodeFreshness::Unspecified => "pending".to_owned(),
    }
}

fn alert_label(status: &v1::NodeStatus) -> String {
    let parts: Vec<String> = status
        .alerts
        .iter()
        .filter_map(|alert| match alert.kind() {
            v1::AlertKind::DiskPercent => Some(format!("disk:{}", alert.subject)),
            v1::AlertKind::MemPressure => Some("mem".to_owned()),
            v1::AlertKind::Unreachable => Some("down".to_owned()),
            v1::AlertKind::Unspecified => None,
        })
        .collect();
    or_dash(parts.join(","))
}

fn first_line(text: &str, limit: usize) -> String {
    let line = text
        .trim()
        .split('\n')
        .next()
        .unwrap_or_default()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let chars: Vec<char> = line.chars().collect();
    if chars.len() > limit {
        format!("{}...", chars[..limit - 1].iter().collect::<String>())
    } else {
        line
    }
}

/// How long ago a hub timestamp is, in seconds; zero when absent.
fn age_since<T: Copy>(at: Option<&T>) -> f64
where
    SystemTime: TryFrom<T>,
{
    match unix_millis(at) {
        Some(millis) => (protectorate_fsutil::unix_millis() - millis) as f64 / 1000.0,
        None => 0.0,
    }
}

/// Unix milliseconds of a protobuf timestamp.
pub(crate) fn unix_millis<T: Copy>(at: Option<&T>) -> Option<i64>
where
    SystemTime: TryFrom<T>,
{
    let time = SystemTime::try_from(*at?).ok()?;
    Some(match time.duration_since(UNIX_EPOCH) {
        Ok(after) => after.as_millis() as i64,
        Err(before) => -(before.duration().as_millis() as i64),
    })
}

/// A protobuf timestamp rendered in this machine's local time, `-` when
/// absent.
fn local_time<T: Copy>(at: Option<&T>, render: impl Fn(DateTime<Local>) -> String) -> String
where
    SystemTime: TryFrom<T>,
{
    unix_millis(at)
        .and_then(|millis| match Local.timestamp_millis_opt(millis) {
            LocalResult::Single(time) => Some(render(time)),
            _ => None,
        })
        .unwrap_or_else(|| "-".to_owned())
}

fn stamp<T: Copy>(at: Option<&T>) -> String
where
    SystemTime: TryFrom<T>,
{
    local_time(at, |time| time.to_rfc3339())
}

pub(crate) fn minute<T: Copy>(at: Option<&T>) -> String
where
    SystemTime: TryFrom<T>,
{
    local_time(at, |time| time.format("%Y-%m-%d %H:%M").to_string())
}

/// HH:MM:SS.
fn local_clock<T: Copy>(at: Option<&T>) -> String
where
    SystemTime: TryFrom<T>,
{
    local_time(at, |time| time.format("%H:%M:%S").to_string())
}

/// Seconds of a protobuf duration; none when absent or negative.
fn seconds<T: Copy>(value: Option<&T>) -> Option<f64>
where
    std::time::Duration: TryFrom<T>,
{
    std::time::Duration::try_from(*value?)
        .ok()
        .map(|value| value.as_secs_f64())
}

/// A protobuf enum value name without its type prefix, lowercased; empty
/// for the unspecified value.
pub(crate) fn word(name: &str, prefix: &str) -> String {
    match name.strip_prefix(prefix).unwrap_or(name) {
        "UNSPECIFIED" => String::new(),
        rest => rest.to_ascii_lowercase(),
    }
}

fn os_arch(os: &str, arch: &str) -> String {
    if os.is_empty() {
        "-".to_owned()
    } else {
        format!("{os}/{arch}")
    }
}

/// Aligns tab-separated columns: consecutive column lines share widths, and a
/// line without a tab ends the block.
fn align(text: &str) -> String {
    let mut out = String::new();
    let mut block: Vec<Vec<String>> = Vec::new();
    for line in text.lines() {
        if line.contains('\t') {
            block.push(line.split('\t').map(str::to_string).collect());
        } else {
            flush(&mut block, &mut out);
            out.push_str(line);
            out.push('\n');
        }
    }
    flush(&mut block, &mut out);
    out
}

fn flush(block: &mut Vec<Vec<String>>, out: &mut String) {
    if block.is_empty() {
        return;
    }
    let columns = block.iter().map(|row| row.len()).max().unwrap_or(0);
    let mut widths = vec![0usize; columns];
    for row in block.iter() {
        // The text after a line's last tab is not a column.
        for (index, cell) in row.iter().enumerate().take(row.len().saturating_sub(1)) {
            widths[index] = widths[index].max(cell.chars().count());
        }
    }
    for row in block.drain(..) {
        let mut line = String::new();
        for (index, cell) in row.iter().enumerate() {
            line.push_str(cell);
            if index + 1 < row.len() {
                line.push_str(&" ".repeat(widths[index] - cell.chars().count() + 2));
            }
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
}

pub(crate) fn checkpoint_line(checkpoint: &v1::Checkpoint) -> String {
    let commit = &checkpoint.commit;
    format!(
        "{}\t{}\t{}\t{}\n",
        checkpoint.id,
        checkpoint
            .created_at
            .map(|at| at.to_string())
            .unwrap_or_default(),
        &commit[..commit.len().min(12)],
        checkpoint.reason
    )
}
