const std = @import("std");
const schema = @import("../api/schema.zig");

pub fn print_out(io: std.Io, comptime fmt: []const u8, args: anytype) !void {
    var buf: [4096]u8 = undefined;
    var w = std.Io.File.stdout().writer(io, &buf);
    try w.interface.print(fmt, args);
    try w.interface.flush();
}

pub fn print_error_or_raw(
    arena: std.mem.Allocator,
    io: std.Io,
    status: u16,
    body: []const u8,
    json_out: bool,
) !void {
    if (!json_out) {
        if (schema.decode_arena(schema.ErrorBody, arena, body)) |eb| {
            try print_out(io, "error ({d} {s}): {s}\n", .{ status, @tagName(eb.@"error".code), eb.@"error".message });
            return;
        } else |_| {}
    }
    try print_out(io, "{s}\n", .{body});
}

pub fn print_doctor(io: std.Io, rep: schema.DoctorReport) !void {
    try print_out(io, "protectorate doctor  os={s}  arch={s}  sandbox={s}  gh={s}\n", .{
        rep.os,
        rep.arch,
        if (rep.sandbox_ok) "ok" else "UNAVAILABLE",
        if (rep.gh_logged_in) "logged_in" else "no",
    });
    if (rep.sleeve_devices.len > 0) {
        try print_out(io, "devices:", .{});
        for (rep.sleeve_devices) |d| try print_out(io, " [{s}]", .{d});
        try print_out(io, "\n", .{});
    }
    try print_out(io, "{s:<12} {s:<10} {s:<28} {s}\n", .{ "HARNESS", "STATUS", "VERSION", "NOTE" });
    for (rep.harnesses) |h| {
        const st: []const u8 = if (h.eligible) "ready" else if (h.present) "installed" else "missing";
        const ver: []const u8 = if (h.version.len > 0) h.version else "-";
        const note: []const u8 = if (!h.present)
            h.install
        else if (h.reasons.len > 0)
            h.reasons[0]
        else
            "ok";
        try print_out(io, "{s:<12} {s:<10} {s:<28} {s}\n", .{ h.name, st, ver, note });
    }
    if (rep.problems.len > 0) {
        try print_out(io, "\nProblems:\n", .{});
        for (rep.problems) |p| try print_out(io, "  - {s}\n", .{p});
    }
}

pub fn print_doctor_json(arena: std.mem.Allocator, io: std.Io, body: []const u8) !void {
    const rep = try schema.decode_arena(schema.DoctorReport, arena, body);
    try print_doctor(io, rep);
}

pub fn print_hub_info(arena: std.mem.Allocator, io: std.Io, body: []const u8) !void {
    const info = try schema.decode_arena(schema.HubInfo, arena, body);
    try print_out(io, "hub={s}  version={s}  started_at_ms={d}  artifact_cap={d}\n", .{
        info.name,
        info.version,
        info.started_at_ms.ms,
        info.job_artifact_max_bytes,
    });
    try print_out(io, "fleet_telemetry_dropped={d}  fleet_refused={d}\n", .{ info.fleet_telemetry_dropped, info.fleet_refused });
    if (info.restore_error) |re| {
        try print_out(io, "WARNING restore_error: {s}\n", .{re});
    }
    if (info.inventory) |inv| {
        try print_out(io, "host={s}  os={s}  arch={s}  cpus={d}  ram={d}GiB\n", .{
            inv.hostname,
            inv.os,
            inv.arch,
            inv.cpu_count,
            inv.ram_bytes / (1024 * 1024 * 1024),
        });
    }
}

pub fn print_status(arena: std.mem.Allocator, io: std.Io, body: []const u8) !void {
    const st = try schema.decode_arena(schema.HubState, arena, body);
    try print_out(io, "hub={s}  version={s}  seq={d}  sleeves={d}  nodes={d}  jobs={d}  approvals={d}\n", .{
        st.hub.name,
        st.hub.version,
        st.seq,
        st.sleeves.len,
        st.nodes.len,
        st.jobs.len,
        st.approvals.len,
    });
    if (st.hub.restore_error) |re| {
        try print_out(io, "WARNING restore_error: {s}\n", .{re});
    }
    if (st.sleeves.len > 0) {
        try print_out(io, "\n", .{});
        render_sleeves(io, st.sleeves) catch {};
    }
    if (st.nodes.len > 0) {
        try print_out(io, "\n", .{});
        render_nodes(io, st.nodes) catch {};
    }
    if (st.approvals.len > 0) {
        var pending_any = false;
        for (st.approvals) |ap| {
            if (ap.state == .pending) {
                pending_any = true;
                break;
            }
        }
        if (pending_any) {
            try print_out(io, "\n", .{});
            try render_approvals(io, st.approvals, true);
        }
    }
    if (st.jobs.len > 0) {
        try print_out(io, "\n", .{});
        render_jobs(io, st.jobs) catch {};
    }
}

pub fn print_sleeve_list(arena: std.mem.Allocator, io: std.Io, body: []const u8) !void {
    const ls = try schema.decode_arena(schema.SleeveList, arena, body);
    if (ls.sleeves.len == 0) {
        try print_out(io, "no sleeves\n", .{});
        return;
    }
    try render_sleeves(io, ls.sleeves);
}

pub fn print_model_catalog(arena: std.mem.Allocator, io: std.Io, body: []const u8) !void {
    const catalog = try schema.decode_arena(schema.ModelCatalog, arena, body);
    for (catalog.harnesses) |h| {
        try print_out(io, "{s} (efforts: {s}; default effort: {s})\n", .{
            @tagName(h.harness),
            if (h.efforts.len == 0) "-" else try std.mem.join(arena, ", ", h.efforts),
            h.default_effort orelse "native",
        });
        if (h.models.len == 0) try print_out(io, "  No catalog entries. Use --model to enter an ID.\n", .{});
        for (h.models) |m| {
            try print_out(io, "  {s}{s}{s}  {s}{s}\n", .{
                m.provider orelse "",
                if (m.provider != null) "/" else "",
                m.id,
                m.label,
                if (m.is_default) " (default)" else "",
            });
        }
    }
}

pub fn render_sleeves(io: std.Io, sleeves: []const schema.Sleeve) !void {
    try print_out(io, "{s:<16} {s:<14} {s:<7} {s:<10} {s:<12} {s:<10} {s:<5} {s}\n", .{
        "ID", "NAME", "ROLE", "STATE", "HARNESS", "ACTIVITY", "Q", "PATH",
    });
    for (sleeves) |sv| {
        const h_s: []const u8 = if (sv.incarnation) |inc| @tagName(inc.harness) else if (sv.resleeve_to.len > 0) @tagName(sv.resleeve_to[0]) else "-";
        const act_s: []const u8 = if (sv.incarnation) |inc| @tagName(inc.activity.state) else "-";
        try print_out(io, "{s:<16} {s:<14} {s:<7} {s:<10} {s:<12} {s:<10} {d:<5} {s}", .{
            sv.id,
            sv.name,
            @tagName(sv.role),
            @tagName(sv.lifecycle),
            h_s,
            act_s,
            sv.queued,
            sv.path,
        });
        if (sv.branch) |b| try print_out(io, " ({s})", .{b});
        if (sv.d9) try print_out(io, " [d9]", .{});
        try print_out(io, "\n", .{});
    }
}

pub fn print_sleeve_detail(io: std.Io, sv: schema.Sleeve) !void {
    try render_sleeves(io, &.{sv});
    if (sv.parent_id) |pid| try print_out(io, "parent={s}\n", .{pid});
    if (sv.incarnation) |inc| {
        try print_out(
            io,
            "incarnation g={d} harness={s} sub={s} model={s} turn={d} ctx={d}/{d} mem_limit={?d}\n",
            .{
                inc.g,
                @tagName(inc.harness),
                @tagName(inc.subscription),
                if (inc.model.len > 0) inc.model else "-",
                inc.turn,
                inc.context_tokens,
                inc.context_window,
                inc.memory_max_bytes,
            },
        );
    }
}

pub fn print_inbox(arena: std.mem.Allocator, io: std.Io, body: []const u8) !void {
    const ib = try schema.decode_arena(schema.Inbox, arena, body);
    if (ib.messages.len == 0) {
        try print_out(io, "inbox empty\n", .{});
        return;
    }
    for (ib.messages, 0..) |m, i| {
        try print_out(io, "[{d}] from={s} at={d}: {s}\n", .{ i, m.sender, m.queued_at_ms.ms, m.text });
    }
}

pub fn print_wait_result(arena: std.mem.Allocator, io: std.Io, body: []const u8) !void {
    const wr = try schema.decode_arena(schema.WaitResult, arena, body);
    try print_out(io, "reached={}  sleeves={d}\n", .{ wr.reached, wr.sleeves.len });
    for (wr.sleeves) |ws| {
        try print_out(io, "  {s} ({s}) reached={} lifecycle={s}\n", .{
            ws.sleeve.id,
            ws.sleeve.name,
            ws.reached,
            @tagName(ws.sleeve.lifecycle),
        });
    }
}

pub fn print_transcript(arena: std.mem.Allocator, io: std.Io, body: []const u8) !void {
    const tx = try schema.decode_arena(schema.Transcript, arena, body);
    if (tx.entries.len == 0) {
        try print_out(io, "no transcript entries for {s}\n", .{tx.sleeve_id});
        return;
    }
    for (tx.entries) |ce| {
        switch (ce.entry) {
            .session_info => |s| {
                try print_out(io, "[#{d} g{d}.t{d}] session harness={s} id={s}", .{ ce.seq, ce.g, ce.turn, @tagName(s.harness), s.native_id });
                if (s.model) |model| try print_out(io, " model={s}", .{model});
                if (s.effort) |effort| try print_out(io, " effort={s}", .{effort});
                if (s.effort_notice) |notice| try print_out(io, "  {s}", .{notice});
                try print_out(io, "\n", .{});
            },
            .user_message => |u| try print_out(io, "[#{d} g{d}.t{d}] {s}: {s}\n", .{ ce.seq, ce.g, ce.turn, u.from, u.text }),
            .prompt_ack => |p| try print_out(io, "[#{d} g{d}.t{d}] ack({s}) {s}\n", .{ ce.seq, ce.g, ce.turn, @tagName(p.ack), p.detail orelse "" }),
            .assistant_text => |a| try print_out(io, "[#{d} g{d}.t{d}] assistant: {s}\n", .{ ce.seq, ce.g, ce.turn, a.text }),
            .reasoning => |r| try print_out(io, "[#{d} g{d}.t{d}] thinking: {s}\n", .{ ce.seq, ce.g, ce.turn, r.text }),
            .tool_call => |t| try print_out(io, "[#{d} g{d}.t{d}] tool {s}: {s}\n", .{ ce.seq, ce.g, ce.turn, t.name, t.summary }),
            .tool_result => |t| try print_out(io, "[#{d} g{d}.t{d}] tool_result ({s}): {s}\n", .{ ce.seq, ce.g, ce.turn, @tagName(t.status), t.output }),
            .file_change => |f| try print_out(io, "[#{d} g{d}.t{d}] files ({d})\n", .{ ce.seq, ce.g, ce.turn, f.files.len }),
            .approval_request => |ap| try print_out(io, "[#{d} g{d}.t{d}] approval_req {s}: {s}\n", .{ ce.seq, ce.g, ce.turn, ap.title, ap.summary }),
            .approval_decision => |ad| try print_out(io, "[#{d} g{d}.t{d}] approval_dec {s}: {s}\n", .{ ce.seq, ce.g, ce.turn, ad.approval_id, @tagName(ad.decision) }),
            .turn_started => try print_out(io, "[#{d} g{d}.t{d}] turn_started\n", .{ ce.seq, ce.g, ce.turn }),
            .turn_settled => |ts| try print_out(io, "[#{d} g{d}.t{d}] turn_settled stop={s}\n", .{ ce.seq, ce.g, ce.turn, @tagName(ts.stop) }),
            .@"error" => |e| try print_out(io, "[#{d} g{d}.t{d}] error ({s}): {s}\n", .{ ce.seq, ce.g, ce.turn, @tagName(e.kind), e.text }),
            .compaction => try print_out(io, "[#{d} g{d}.t{d}] compaction\n", .{ ce.seq, ce.g, ce.turn }),
            .notice => |n| try print_out(io, "[#{d} g{d}.t{d}] notice ({s}): {s}\n", .{ ce.seq, ce.g, ce.turn, @tagName(n.level), n.text }),
            .exited => |x| try print_out(io, "[#{d} g{d}.t{d}] exited code={?d}\n", .{ ce.seq, ce.g, ce.turn, x.code }),
            .unknown => try print_out(io, "[#{d} g{d}.t{d}] <unknown>\n", .{ ce.seq, ce.g, ce.turn }),
        }
    }
}

pub fn print_node_list(arena: std.mem.Allocator, io: std.Io, body: []const u8) !void {
    const nl = try schema.decode_arena(schema.NodeList, arena, body);
    if (nl.nodes.len == 0) {
        try print_out(io, "no nodes\n", .{});
        return;
    }
    try render_nodes(io, nl.nodes);
}

pub fn print_node_single(arena: std.mem.Allocator, io: std.Io, body: []const u8) !void {
    const n = try schema.decode_arena(schema.Node, arena, body);
    try render_nodes(io, &.{n});
    if (n.boot_id.len > 0) try print_out(io, "boot_id={s}\n", .{n.boot_id});
    if (n.quarantined) |q| try print_out(io, "quarantined={s}\n", .{q});
    if (n.jobs_note) |jn| try print_out(io, "jobs_note={s}\n", .{jn});
}

pub fn render_nodes(io: std.Io, nodes: []const schema.Node) !void {
    try print_out(io, "{s:<16} {s:<14} {s:<7} {s:<10} {s:<8} {s:<9} {s:<6} {s:<7} {s}\n", .{
        "ID", "NAME", "ONLINE", "VERSION", "ARCH", "PRESENCE", "JOBS", "CPU%", "HW",
    });
    for (nodes) |n| {
        var cpu_buf: [16]u8 = undefined;
        const cpu_s: []const u8 = if (n.sample) |smp|
            (if (smp.cpu_pct) |c| std.fmt.bufPrint(&cpu_buf, "{d:.1}%", .{c}) catch "-" else "-")
        else
            "-";
        const arch_s: []const u8 = if (n.inventory) |inv| inv.arch else "-";
        try print_out(io, "{s:<16} {s:<14} {s:<7} {s:<10} {s:<8} {s:<9} {s:<6} {s:<7}", .{
            n.id,
            n.name,
            if (n.online) "online" else "offline",
            n.node_version,
            arch_s,
            @tagName(n.presence.state),
            @tagName(n.jobs),
            cpu_s,
        });
        if (n.inventory) |inv| {
            try print_out(io, "{d}c/{d}GiB", .{ inv.cpu_count, inv.ram_bytes / (1024 * 1024 * 1024) });
            if (inv.disks.len > 0) {
                try print_out(io, " disk={s}:{d}GiBfree", .{ inv.disks[0].mount, inv.disks[0].free_bytes / (1024 * 1024 * 1024) });
            }
            if (inv.gpus.len > 0) {
                try print_out(io, " gpu={s}", .{inv.gpus[0].name});
            }
        } else {
            try print_out(io, "-", .{});
        }
        try print_out(io, "\n", .{});
    }
}

pub fn print_job_list(arena: std.mem.Allocator, io: std.Io, body: []const u8) !void {
    const jl = try schema.decode_arena(schema.JobList, arena, body);
    if (jl.jobs.len == 0) {
        try print_out(io, "no jobs\n", .{});
        return;
    }
    try render_jobs(io, jl.jobs);
}

pub fn print_job_single(arena: std.mem.Allocator, io: std.Io, body: []const u8) !void {
    const j = try schema.decode_arena(schema.Job, arena, body);
    try render_jobs(io, &.{j});
}

pub fn print_job_log(arena: std.mem.Allocator, io: std.Io, body: []const u8) !void {
    const jl = try schema.decode_arena(schema.JobLog, arena, body);
    if (jl.text.len > 0) try print_out(io, "{s}", .{jl.text});
}

pub fn print_node_admission(arena: std.mem.Allocator, io: std.Io, body: []const u8) !void {
    const na = try schema.decode_arena(schema.NodeAdmission, arena, body);
    try print_out(
        io,
        "node={s} boot={s} next_epoch={d} slots={d}/{d} long={d}/{d} mem_reserved={d}MiB mem_avail={d}MiB",
        .{
            na.node_id,
            na.boot_id,
            na.next_epoch,
            na.slots_used,
            na.slots_total,
            na.long_slots_used,
            na.long_slots_total,
            na.mem_reserved_bytes / (1024 * 1024),
            na.mem_avail_bytes / (1024 * 1024),
        },
    );
    if (na.quarantined) |q| try print_out(io, " QUARANTINED={s}", .{q});
    try print_out(io, "\n", .{});
    if (na.grants.len > 0) {
        try print_out(io, "LEASES ({d}):\n", .{na.grants.len});
        for (na.grants) |g| {
            try print_out(io, "  {s} ep={d} parent={?d} holder={s} job={s} mode={s} state={s} slots={d} long={d}\n", .{
                g.lease_id,
                g.epoch,
                g.parent_epoch,
                g.holder_id,
                g.job_id orelse "-",
                @tagName(g.mode),
                @tagName(g.state),
                g.slots,
                g.long_slots,
            });
        }
    }
    if (na.queue.len > 0) {
        try print_out(io, "QUEUE ({d}):\n", .{na.queue.len});
        for (na.queue) |t| {
            try print_out(io, "  {s} seq={d} holder={s} mode={s} slots={d} long={d}\n", .{
                t.ticket_id,
                t.seq,
                t.holder_id,
                @tagName(t.mode),
                t.slots,
                t.long_slots,
            });
        }
    }
}

pub fn render_jobs(io: std.Io, jobs: []const schema.Job) !void {
    try print_out(io, "{s:<16} {s:<11} {s:<14} {s:<5} {s:<6} {s:<7} {s}\n", .{
        "ID", "STATE", "NODE", "TRY", "EXIT", "EPOCH", "COMMAND",
    });
    for (jobs) |j| {
        var ec_buf: [16]u8 = undefined;
        const ec_s: []const u8 = if (j.exit_code) |ec| std.fmt.bufPrint(&ec_buf, "{d}", .{ec}) catch "-" else "-";
        var ep_buf: [24]u8 = undefined;
        const ep_s: []const u8 = if (j.epoch) |ep| std.fmt.bufPrint(&ep_buf, "{d}", .{ep}) catch "-" else "-";
        const node_s: []const u8 = j.node_id orelse switch (j.target) {
            .node => |n| n.node_id,
            .arch => |ar| ar.arch,
            .unknown => "-",
        };
        try print_out(io, "{s:<16} {s:<11} {s:<14} {d:<5} {s:<6} {s:<7}", .{
            j.id,
            @tagName(j.state),
            node_s,
            j.attempt,
            ec_s,
            ep_s,
        });
        for (j.argv, 0..) |arg, idx| {
            try print_out(io, "{s}{s}", .{ if (idx == 0) "" else " ", arg });
        }
        if (j.held_reason) |hr| try print_out(io, " (held={s})", .{hr});
        if (j.@"error") |er| try print_out(io, " [{s}]", .{er});
        try print_out(io, "\n", .{});
    }
}

pub fn print_approval_list(arena: std.mem.Allocator, io: std.Io, body: []const u8) !void {
    const al = try schema.decode_arena(schema.ApprovalList, arena, body);
    if (al.approvals.len == 0) {
        try print_out(io, "no approvals\n", .{});
        return;
    }
    try render_approvals(io, al.approvals, false);
}

fn render_approvals(io: std.Io, approvals: []const schema.Approval, pending_only: bool) !void {
    try print_out(io, "{s:<18} {s:<9} {s:<8} {s:<14} {s}\n", .{ "APPROVAL", "STATE", "KIND", "SLEEVE", "TITLE" });
    for (approvals) |ap| {
        if (pending_only and ap.state != .pending) continue;
        try print_out(io, "{s:<18} {s:<9} {s:<8} {s:<14} {s}\n", .{
            ap.id,
            @tagName(ap.state),
            @tagName(ap.kind),
            ap.sleeve_id,
            ap.title,
        });
    }
}

pub fn print_event_list(arena: std.mem.Allocator, io: std.Io, body: []const u8) !void {
    const el = try schema.decode_arena(schema.EventList, arena, body);
    if (el.events.len == 0) {
        try print_out(io, "no events\n", .{});
        return;
    }
    try print_out(io, "{s:<6} {s:<10} {s:<6} {s:<12} {s}\n", .{ "SEQ", "KIND", "LEVEL", "SOURCE", "TEXT" });
    for (el.events) |ev| {
        try print_out(io, "{d:<6} {s:<10} {s:<6} {s:<12} {s}{s}\n", .{
            ev.seq,
            @tagName(ev.kind),
            @tagName(ev.level),
            ev.source,
            ev.text,
            if (ev.read) "" else " [unread]",
        });
    }
}

pub fn print_device_list(arena: std.mem.Allocator, io: std.Io, body: []const u8) !void {
    const dl = try schema.decode_arena(schema.DeviceList, arena, body);
    if (dl.devices.len == 0) {
        try print_out(io, "no devices\n", .{});
        return;
    }
    try print_out(io, "{s:<18} {s:<18} {s}\n", .{ "ID", "NAME", "CREATED_MS" });
    for (dl.devices) |d| {
        try print_out(io, "{s:<18} {s:<18} {d}\n", .{ d.id, d.name, d.created_at_ms.ms });
    }
}

pub fn print_envoy_reply(arena: std.mem.Allocator, io: std.Io, body: []const u8) !void {
    const er = try schema.decode_arena(schema.EnvoyReply, arena, body);
    try print_out(io, "request_id={s}  settled={}  seq={d}\n", .{ er.request_id, er.settled, er.seq });
    if (er.text.len > 0) try print_out(io, "{s}\n", .{er.text});
}

pub fn print_git_push(arena: std.mem.Allocator, io: std.Io, body: []const u8) !void {
    const gp = try schema.decode_arena(schema.GitPushResult, arena, body);
    try print_out(io, "sleeve={s}  remote={s}  branch={s}  pushed={}  commit={s}\n", .{
        gp.sleeve_id,
        gp.remote,
        gp.branch,
        gp.pushed,
        gp.commit,
    });
}

pub fn print_quota_list(arena: std.mem.Allocator, io: std.Io, body: []const u8) !void {
    const ql = try schema.decode_arena(schema.QuotaList, arena, body);
    if (ql.quotas.len == 0) {
        try print_out(io, "no quotas\n", .{});
        return;
    }
    try print_out(io, "{s:<13} {s}\n", .{ "SUB", "READING" });
    for (ql.quotas) |q| {
        try print_out(io, "{s:<13} {s}\n  {s}\n", .{
            @tagName(q.subscription),
            @tagName(q.reading),
            if (q.plan.len > 0) q.plan else "-",
        });
        const visible = if (q.subscription == .antigravity) @min(q.windows.len, 6) else q.windows.len;
        for (q.windows[0..visible]) |w| {
            if (w.used_pct) |pct| {
                const label = if (std.mem.startsWith(u8, w.label, "model ")) w.label[6..] else w.label;
                try print_out(io, "  {s}\n    {d:.0}% used\n", .{ label, pct });
                if (w.resets_at_ms) |reset| {
                    const now = std.Io.Clock.real.now(io).toMilliseconds();
                    const minutes = @divFloor(@max(@as(i64, 0), reset.ms -| now), 60_000);
                    try print_out(io, "    reset in {d}h {d}m\n", .{ @divFloor(minutes, 60), @mod(minutes, 60) });
                }
            }
        }
        if (q.windows.len > visible) {
            const hidden = q.windows.len - visible;
            try print_out(io, "  +{d} more model{s}\n", .{ hidden, if (hidden == 1) "" else "s" });
        }
        if (q.credits) |cr| {
            if (cr.balance) |bal| try print_out(io, "  balance={d:.2}\n", .{bal});
        }
        if (q.note.len > 0) try print_out(io, "  {s}\n", .{q.note});
    }
}

pub fn print_history_hits(arena: std.mem.Allocator, io: std.Io, body: []const u8) !void {
    const hh = try schema.decode_arena(schema.HistoryHits, arena, body);
    try print_out(io, "hits={d}  indexed_docs={d}\n", .{ hh.hits.len, hh.indexed_docs });
    for (hh.hits) |h| {
        const har_s: []const u8 = if (h.doc.harness) |har| @tagName(har) else "-";
        try print_out(io, "  [{d:.2}] #{d} {s} ({s}) {s}: {s}\n", .{
            h.score,
            h.doc.doc_id,
            h.doc.sleeve,
            har_s,
            @tagName(h.doc.role),
            h.snippet,
        });
    }
}

pub fn print_history_read(arena: std.mem.Allocator, io: std.Io, body: []const u8) !void {
    const hr = try schema.decode_arena(schema.HistoryRead, arena, body);
    try print_out(io, "docs={d}  more={}\n", .{ hr.docs.len, hr.more });
    for (hr.docs) |d| {
        try print_out(io, "[#{d} {s} t{d} {s}] {s}\n", .{
            d.doc.doc_id,
            d.doc.sleeve,
            d.doc.turn,
            @tagName(d.doc.role),
            d.text,
        });
    }
}

pub fn print_hub_update(arena: std.mem.Allocator, io: std.Io, body: []const u8) !void {
    const hu = try schema.decode_arena(schema.HubUpdate, arena, body);
    try print_out(io, "current={s}  target={s}  available={}  restarting={}", .{
        hu.current orelse "-",
        hu.target,
        hu.available,
        hu.restarting,
    });
    if (hu.note.len > 0) try print_out(io, "  note={s}", .{hu.note});
    try print_out(io, "\n", .{});
}

pub fn print_node_updates(arena: std.mem.Allocator, io: std.Io, body: []const u8) !void {
    const nu = try schema.decode_arena(schema.NodeUpdates, arena, body);
    try print_out(io, "rollout={s}  target={s}  nodes={d}\n", .{
        nu.rollout_id orelse "-",
        nu.version,
        nu.nodes.len,
    });
    if (nu.nodes.len > 0) {
        try print_out(io, "{s:<16} {s:<14} {s:<10} {s:<10} {s:<5} {s}\n", .{
            "ID", "NAME", "FROM", "TO", "OK", "DETAIL",
        });
        for (nu.nodes) |n| {
            try print_out(io, "{s:<16} {s:<14} {s:<10} {s:<10} {s:<5} {s}\n", .{
                n.node_id,
                n.name,
                n.from,
                n.to,
                if (n.ok) "yes" else "no",
                n.detail,
            });
        }
    }
}

pub fn print_harness_updates(arena: std.mem.Allocator, io: std.Io, body: []const u8) !void {
    const hu = try schema.decode_arena(schema.HarnessUpdates, arena, body);
    if (hu.programs.len == 0) {
        try print_out(io, "no harnesses matched\n", .{});
        return;
    }
    try print_out(io, "{s:<14} {s:<12} {s}\n", .{ "PROGRAM", "STATE", "DETAIL" });
    for (hu.programs) |p| {
        try print_out(io, "{s:<14} {s:<12} {s}\n", .{ p.program, @tagName(p.state), p.detail });
    }
}
