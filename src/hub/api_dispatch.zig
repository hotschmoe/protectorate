const std = @import("std");
const tiger = @import("../tiger.zig");
const schema = @import("../api/schema.zig");
const event = @import("../core/event.zig");
const model = @import("../core/model.zig");
const msg = @import("../net/msg.zig");
const noise = @import("../net/noise.zig");
const h_seg = @import("../history/segment.zig");
const h_ext = @import("../history/extract.zig");
const u_ver = @import("../update/version.zig");
const u_store = @import("../update/store.zig");
const doctor = @import("../doctor/doctor.zig");
const home = @import("home.zig");
const git = @import("git.zig");
const fleet = @import("fleet.zig");
const runtime = @import("runtime.zig");

const envoy_md = @embedFile("envoy.md");

fn expand_tilde_alloc(a: std.mem.Allocator, home_opt: ?[]const u8, p: []const u8) ![]const u8 {
    var trimmed = std.mem.trim(u8, p, " \t\r\n");
    while (trimmed.len > 1 and trimmed[trimmed.len - 1] == '/') {
        trimmed = trimmed[0 .. trimmed.len - 1];
    }
    if (std.mem.eql(u8, trimmed, "~")) {
        if (home_opt) |h| return a.dupe(u8, std.mem.trimEnd(u8, h, "/"));
    } else if (std.mem.startsWith(u8, trimmed, "~/")) {
        if (home_opt) |h| return std.fs.path.join(a, &.{ h, trimmed[2..] });
    }
    return a.dupe(u8, trimmed);
}

pub const DispatchResult = struct {
    status: u16,
    body_json: []u8,
};

fn ok_json(gpa: std.mem.Allocator, value: anytype) !DispatchResult {
    return .{
        .status = 200,
        .body_json = try schema.encode_alloc(gpa, value),
    };
}

fn err_json(gpa: std.mem.Allocator, code: schema.ErrorCode, message: []const u8) !DispatchResult {
    const body = schema.ErrorBody{ .@"error" = .{ .code = code, .message = message } };
    return .{
        .status = code.http_status(),
        .body_json = try schema.encode_alloc(gpa, body),
    };
}

pub fn dispatch(
    rt: *runtime.Runtime,
    principal: ?home.Principal,
    method: []const u8,
    raw_body: []const u8,
) !DispatchResult {
    var arena_st = std.heap.ArenaAllocator.init(rt.gpa);
    defer arena_st.deinit();
    const a = arena_st.allocator();

    const scope_val = find_method_scope(method) orelse {
        return err_json(rt.gpa, .not_found, "unknown rpc method");
    };

    if (scope_val != .public) {
        const p = principal orelse return err_json(rt.gpa, .unauthenticated, "missing or invalid bearer token");
        switch (scope_val) {
            .owner => switch (p) {
                .owner, .device => {},
                else => return err_json(rt.gpa, .forbidden, "owner scope required"),
            },
            .envoy_only => switch (p) {
                .envoy, .owner, .device => {},
                else => return err_json(rt.gpa, .forbidden, "envoy scope required"),
            },
            .sleeve, .public => {},
        }
    }

    const body = if (std.mem.trim(u8, raw_body, " \t\r\n").len == 0) "{}" else raw_body;

    if (std.mem.eql(u8, method, "GetState")) {
        return ok_json(rt.gpa, try rt.view_state(a));
    } else if (std.mem.eql(u8, method, "GetHubInfo")) {
        return ok_json(rt.gpa, try rt.view_hub_info(a));
    } else if (std.mem.eql(u8, method, "ListSleeves")) {
        const st = try rt.view_state(a);
        return ok_json(rt.gpa, schema.SleeveList{ .sleeves = st.sleeves });
    } else if (std.mem.eql(u8, method, "GetInbox")) {
        const req = schema.decode_arena(schema.InboxRequest, a, body) catch return err_json(rt.gpa, .invalid, "bad request");
        const raw_sid = req.sleeve_id orelse "envoy";
        const sid = rt.resolve_sleeve_id(a, raw_sid) orelse raw_sid;
        rt.state_mu.lockUncancelable(rt.io);
        defer rt.state_mu.unlock(rt.io);
        const sl = rt.core_state.sleeves.get(sid) orelse return err_json(rt.gpa, .not_found, "sleeve not found");
        var msgs = try a.alloc(schema.QueuedMessage, sl.queued.items.len);
        for (sl.queued.items, 0..) |m, i| {
            msgs[i] = .{ .sender = m.from, .queued_at_ms = .{ .ms = m.queued_at_ms }, .text = m.text };
        }
        return ok_json(rt.gpa, schema.Inbox{ .messages = msgs });
    } else if (std.mem.eql(u8, method, "DropQueued")) {
        const req = schema.decode_arena(schema.SleeveRef, a, body) catch return err_json(rt.gpa, .invalid, "bad request");
        const sid = rt.resolve_sleeve_id(a, req.sleeve_id) orelse return err_json(rt.gpa, .not_found, "sleeve not found");
        rt.commit(.{ .drop_queued = .{ .sleeve_id = sid } }) catch return err_json(rt.gpa, .not_found, "sleeve not found");
        return ok_json(rt.gpa, schema.Empty{});
    } else if (std.mem.eql(u8, method, "GetTranscript")) {
        const req = schema.decode_arena(schema.GetTranscriptRequest, a, body) catch return err_json(rt.gpa, .invalid, "bad request");
        const sid = rt.resolve_sleeve_id(a, req.sleeve_id) orelse req.sleeve_id;
        const tx = try rt.transcripts.read(a, sid, req.from_seq, req.limit);
        return ok_json(rt.gpa, tx);
    } else if (std.mem.eql(u8, method, "SpawnSleeve")) {
        const req = schema.decode_arena(schema.SpawnSleeveRequest, a, body) catch return err_json(rt.gpa, .invalid, "bad request");
        if (req.harness == .unknown or req.task_size == .unknown) return err_json(rt.gpa, .invalid, "unknown harness or task size");
        const caller = principal.?;
        if (req.task_size != null and caller == .sleeve) return err_json(rt.gpa, .forbidden, "task size requires owner or Envoy scope");
        var parent_sid: ?[]const u8 = null;
        var parent_path: ?[]const u8 = null;
        if (req.parent_id) |raw_pid| {
            parent_sid = rt.resolve_sleeve_id(a, raw_pid) orelse return err_json(rt.gpa, .not_found, "parent sleeve not found");
            if (try rt.view_sleeve(a, parent_sid.?)) |psv| parent_path = psv.path;
        }
        const is_envoy = parent_sid == null and std.mem.eql(u8, req.name, "envoy");
        if (is_envoy) {
            if (req.auto_select) return err_json(rt.gpa, .invalid, "Envoy requires a fixed harness");
            rt.state_mu.lockUncancelable(rt.io);
            var dup_envoy = false;
            var sit = rt.core_state.sleeves.iterator();
            while (sit.next()) |e| {
                const sl = e.value_ptr.*;
                if (sl.lifecycle != .removed and (std.mem.eql(u8, sl.id, "envoy") or sl.role == .envoy)) {
                    dup_envoy = true;
                    break;
                }
            }
            rt.state_mu.unlock(rt.io);
            if (dup_envoy) return err_json(rt.gpa, .conflict, "envoy sleeve already exists");
        }
        const raw_base_path: []const u8 = if (req.path) |p|
            try expand_tilde_alloc(a, rt.env.get("HOME"), p)
        else if (req.new_path) |p|
            try expand_tilde_alloc(a, rt.env.get("HOME"), p)
        else if (parent_path) |p|
            p
        else if (is_envoy)
            try std.fs.path.join(a, &.{ rt.hub_home.root_path, "envoy" })
        else
            return err_json(rt.gpa, .invalid, "path or --parent required");

        if (is_envoy) {
            try std.Io.Dir.cwd().createDirPath(rt.io, raw_base_path);
            for ([_][]const u8{ "CLAUDE.md", "AGENTS.md" }) |fname| {
                const instr_p = try std.fs.path.join(a, &.{ raw_base_path, fname });
                if (std.Io.Dir.cwd().statFile(rt.io, instr_p, .{})) |_| {} else |_| {
                    std.Io.Dir.cwd().writeFile(rt.io, .{ .sub_path = instr_p, .data = envoy_md }) catch {};
                }
            }
        } else if (req.new_path != null) {
            try std.Io.Dir.cwd().createDirPath(rt.io, raw_base_path);
        } else if (req.path != null and parent_sid == null) {
            var d = std.Io.Dir.cwd().openDir(rt.io, raw_base_path, .{}) catch {
                return err_json(rt.gpa, .invalid, "path does not exist");
            };
            d.close(rt.io);
        }

        const choice: ?event.QuotaSelection = if (req.auto_select and req.harness == .claude and req.model == null and req.provider == null)
            try rt.choose_target(a, &runtime.spawn_harnesses, null, parent_sid, raw_base_path, req.task_size, caller == .envoy)
        else
            null;
        const harness = if (choice) |picked|
            picked.harness orelse return err_json(rt.gpa, .unavailable, picked.reason)
        else
            runtime.schema_to_model_harness(req.harness);

        if (rt.gate_d9(a, principal, .spawn_sleeve, req.name)) |_| {} else |err| {
            return map_d9_err(rt.gpa, err);
        }
        const sid = if (is_envoy) "envoy" else try rt.gen_id(a, "slv");
        const ws_id = if (is_envoy) "ws-envoy" else try rt.gen_id(a, "ws");
        const layout: model.Layout = if (req.view) |v| switch (v) {
            .readonly => .readonly,
            .worktree => .worktree,
            .unknown => .shared,
        } else if (parent_sid != null) .worktree else .shared;
        var sl_path: []const u8 = raw_base_path;
        var sl_branch: ?[]const u8 = null;
        if (layout == .worktree) {
            sl_path = if (req.new_path != null) raw_base_path else try std.fs.path.join(a, &.{ rt.hub_home.root_path, "worktrees", sid });
            sl_branch = try std.fmt.allocPrint(a, "pt/{s}", .{sid});
            try rt.register_worktree_base(sid, raw_base_path);
        }
        try rt.register_workspace(ws_id, req.name, raw_base_path);
        rt.commit(.{ .sleeve_created = .{
            .sleeve_id = sid,
            .name = req.name,
            .role = if (is_envoy) .envoy else if (parent_sid != null) .child else .root,
            .path = sl_path,
            .workspace_id = ws_id,
            .parent_id = parent_sid,
            .layout = layout,
            .branch = sl_branch,
            .budget_limit = req.budget,
            .until_done = req.until_done orelse 0,
        } }) catch |err| return map_commit_err(rt.gpa, err);

        var targets: [model.max_targets]model.Harness = undefined;
        targets[0] = harness;
        var tlen: u8 = 1;
        for (req.resleeve_to) |rh| {
            if (tlen >= model.max_targets) break;
            if (rh == .unknown) continue;
            targets[tlen] = runtime.schema_to_model_harness(rh);
            tlen += 1;
        }
        rt.commit(.{ .start_requested = .{
            .sleeve_id = sid,
            .targets = targets,
            .target_len = tlen,
            .mode = switch (req.mode) {
                .pane => .pane,
                else => .headless,
            },
            .model_name = if (choice) |picked| picked.model_name else req.model,
            .provider = if (choice) |picked| picked.provider else req.provider,
            .effort = req.effort,
            .until_done = req.until_done,
            .selection_reason = if (choice) |picked| picked.reason else null,
        } }) catch |err| return map_commit_err(rt.gpa, err);

        if (req.message) |msg_txt| {
            const rid = try rt.gen_id(a, "req");
            rt.commit(.{ .prompt_enqueued = .{
                .sleeve_id = sid,
                .request_id = rid,
                .from = "owner",
                .kind = .prompt,
                .mode = .queue,
                .text = msg_txt,
                .until_done = req.until_done,
            } }) catch {};
        }

        const sv = (try rt.view_sleeve(a, sid)) orelse return err_json(rt.gpa, .not_found, "sleeve missing");
        return ok_json(rt.gpa, sv);
    } else if (std.mem.eql(u8, method, "StartSleeve")) {
        const req = schema.decode_arena(schema.StartSleeveRequest, a, body) catch return err_json(rt.gpa, .invalid, "bad request");
        const sid = rt.resolve_sleeve_id(a, req.sleeve_id) orelse return err_json(rt.gpa, .not_found, "sleeve not found");
        var targets: [model.max_targets]model.Harness = undefined;
        var target_len: u8 = 1;
        {
            rt.state_mu.lockUncancelable(rt.io);
            defer rt.state_mu.unlock(rt.io);
            const sl = rt.core_state.sleeves.get(sid) orelse return err_json(rt.gpa, .not_found, "sleeve not found");
            const primary = if (sl.target_len > 0) sl.targets[0] else .claude;
            targets[0] = if (req.target) |h| runtime.schema_to_model_harness(h) else if (sl.inc) |inc| inc.harness else primary;
            for (sl.targets[1..@max(sl.target_len, 1)]) |h| {
                targets[target_len] = h;
                target_len += 1;
            }
        }
        rt.commit(.{ .start_requested = .{
            .sleeve_id = sid,
            .targets = targets,
            .target_len = target_len,
            .mode = .headless,
            .model_name = req.model,
            .provider = req.provider,
            .effort = req.effort,
            .until_done = req.until_done,
        } }) catch |err| return map_commit_err(rt.gpa, err);
        return ok_json(rt.gpa, schema.Accepted{});
    } else if (std.mem.eql(u8, method, "SendMessage")) {
        const req = schema.decode_arena(schema.SendMessageRequest, a, body) catch return err_json(rt.gpa, .invalid, "bad request");
        const sid = rt.resolve_sleeve_id(a, req.sleeve_id) orelse return err_json(rt.gpa, .not_found, "sleeve not found");
        const rid = try rt.gen_id(a, "req");
        const mode: model.SendMode = if (req.delivery == .steer) .steer else .queue;
        rt.commit(.{ .prompt_enqueued = .{
            .sleeve_id = sid,
            .request_id = rid,
            .from = if (principal != null and principal.? == .envoy) "envoy" else "owner",
            .kind = if (principal != null and principal.? == .envoy) .mail else .prompt,
            .mode = mode,
            .text = req.text,
            .supersede = req.supersede,
            .until_done = req.until_done,
        } }) catch |err| return map_commit_err(rt.gpa, err);
        const sv = try rt.view_sleeve(a, sid);
        return ok_json(rt.gpa, schema.Sent{
            .request_id = rid,
            .queued = true,
            .queue_length = if (sv) |s| s.queued else 0,
        });
    } else if (std.mem.eql(u8, method, "Interrupt")) {
        const req = schema.decode_arena(schema.SleeveRef, a, body) catch return err_json(rt.gpa, .invalid, "bad request");
        const sid = rt.resolve_sleeve_id(a, req.sleeve_id) orelse return err_json(rt.gpa, .not_found, "sleeve not found");
        const sv = (try rt.view_sleeve(a, sid)) orelse return err_json(rt.gpa, .not_found, "sleeve not found");
        if (sv.incarnation) |inc| {
            rt.commit(.{ .interrupted = .{ .sleeve_id = sid, .g = inc.g, .by = .owner } }) catch {};
        }
        return ok_json(rt.gpa, schema.Empty{});
    } else if (std.mem.eql(u8, method, "StopSleeve")) {
        const req = schema.decode_arena(schema.SleeveRef, a, body) catch return err_json(rt.gpa, .invalid, "bad request");
        const sid = rt.resolve_sleeve_id(a, req.sleeve_id) orelse return err_json(rt.gpa, .not_found, "sleeve not found");
        rt.commit(.{ .stop_requested = .{ .sleeve_id = sid, .by = .owner } }) catch |err| return map_commit_err(rt.gpa, err);
        return ok_json(rt.gpa, schema.Accepted{});
    } else if (std.mem.eql(u8, method, "RemoveSleeve")) {
        const req = schema.decode_arena(schema.SleeveRef, a, body) catch return err_json(rt.gpa, .invalid, "bad request");
        const sid = rt.resolve_sleeve_id(a, req.sleeve_id) orelse return err_json(rt.gpa, .not_found, "sleeve not found");
        if (rt.gate_d9(a, principal, .remove_sleeve, sid)) |_| {} else |err| {
            return map_d9_err(rt.gpa, err);
        }
        rt.commit(.{ .removed = .{ .sleeve_id = sid } }) catch |err| return map_commit_err(rt.gpa, err);
        return ok_json(rt.gpa, schema.Accepted{});
    } else if (std.mem.eql(u8, method, "Resleeve")) {
        const req = schema.decode_arena(schema.ResleeveRequest, a, body) catch return err_json(rt.gpa, .invalid, "bad request");
        if (req.target == .unknown) return err_json(rt.gpa, .invalid, "unknown harness");
        const sid = rt.resolve_sleeve_id(a, req.sleeve_id) orelse return err_json(rt.gpa, .not_found, "sleeve not found");
        rt.commit(.{ .resleeve_requested = .{
            .sleeve_id = sid,
            .harness = runtime.schema_to_model_harness(req.target),
            .model_name = req.model,
            .provider = req.provider,
            .effort = req.effort,
            .by = .owner,
            .cause = .manual,
        } }) catch |err| return map_commit_err(rt.gpa, err);
        return ok_json(rt.gpa, schema.Accepted{});
    } else if (std.mem.eql(u8, method, "TakeoverTerminal")) {
        const req = schema.decode_arena(schema.SleeveRef, a, body) catch return err_json(rt.gpa, .invalid, "bad request");
        const sid = rt.resolve_sleeve_id(a, req.sleeve_id) orelse return err_json(rt.gpa, .not_found, "sleeve not found");
        rt.takeover_sleeve(a, sid) catch |err| return map_commit_err(rt.gpa, err);
        return ok_json(rt.gpa, schema.Empty{});
    } else if (std.mem.eql(u8, method, "GitPush")) {
        const req = schema.decode_arena(schema.GitPushRequest, a, body) catch return err_json(rt.gpa, .invalid, "bad request");
        const sid = rt.resolve_sleeve_id(a, req.sleeve_id) orelse return err_json(rt.gpa, .not_found, "sleeve not found");
        const sv = (try rt.view_sleeve(a, sid)) orelse return err_json(rt.gpa, .not_found, "sleeve not found");
        const summary = try std.fmt.allocPrint(a, "{s} {s}", .{ req.remote orelse "origin", req.branch orelse (sv.branch orelse "HEAD") });
        const d9_app = rt.gate_d9(a, principal, .git_push, summary) catch |err| return map_d9_err(rt.gpa, err);
        var res = git.push(a, rt.io, sid, sv.path, req.remote, req.branch, req.force) catch |err| {
            return err_json(rt.gpa, switch (err) {
                error.GitCurrentBranchTimedOut, error.GitHeadCommitTimedOut, error.GitPushTimedOut => .timeout,
                else => .invalid,
            }, @errorName(err));
        };
        if (d9_app) |aid| res.approval_id = aid;
        return ok_json(rt.gpa, res);
    } else if (std.mem.eql(u8, method, "ListApprovals")) {
        const req = schema.decode_arena(schema.ListApprovalsRequest, a, body) catch schema.ListApprovalsRequest{};
        const st = try rt.view_state(a);
        var list: std.ArrayList(schema.Approval) = .empty;
        for (st.approvals) |ap| {
            if (req.pending_only and ap.state != .pending) continue;
            try list.append(a, ap);
        }
        return ok_json(rt.gpa, schema.ApprovalList{ .approvals = list.items });
    } else if (std.mem.eql(u8, method, "Decide")) {
        const req = schema.decode_arena(schema.DecideRequest, a, body) catch return err_json(rt.gpa, .invalid, "bad request");
        const v: model.Verdict = switch (req.decision) {
            .allow_once => .allow_once,
            .allow_session => .allow_session,
            .deny, .unknown => .deny,
        };
        rt.commit(.{ .approval_decided = .{ .approval_id = req.approval_id, .verdict = v, .by = .owner } }) catch |err| return map_commit_err(rt.gpa, err);
        const ap = (try rt.view_approval(a, req.approval_id)) orelse return err_json(rt.gpa, .not_found, "approval not found");
        return ok_json(rt.gpa, ap);
    } else if (std.mem.eql(u8, method, "ListNodes")) {
        const st = try rt.view_state(a);
        return ok_json(rt.gpa, schema.NodeList{ .nodes = st.nodes });
    } else if (std.mem.eql(u8, method, "GetNode")) {
        const req = schema.decode_arena(schema.NodeRef, a, body) catch return err_json(rt.gpa, .invalid, "bad request");
        const nid = rt.resolve_node_id(a, req.node_id) orelse return err_json(rt.gpa, .not_found, "node not found");
        const nd = (try rt.view_node(a, nid)) orelse return err_json(rt.gpa, .not_found, "node not found");
        return ok_json(rt.gpa, nd);
    } else if (std.mem.eql(u8, method, "SetNodeJobs")) {
        const req = schema.decode_arena(schema.SetNodeJobsRequest, a, body) catch return err_json(rt.gpa, .invalid, "bad request");
        const nid = rt.resolve_node_id(a, req.node_id) orelse return err_json(rt.gpa, .not_found, "node not found");
        const jp: model.NodeJobs = switch (req.jobs) {
            .any => .any,
            .owner => .owner,
            .none, .unknown => .none,
        };
        rt.commit(.{ .node_jobs_updated = .{ .node_id = nid, .jobs = jp, .note = req.note } }) catch |err| return map_commit_err(rt.gpa, err);
        const nd = (try rt.view_node(a, nid)) orelse return err_json(rt.gpa, .not_found, "node not found");
        return ok_json(rt.gpa, nd);
    } else if (std.mem.eql(u8, method, "GetEnrollment")) {
        const addrs = try rt.advertise_addrs(a);
        const tok = try fleet.FleetServer.build_join_token(a, rt.hub_home.hub_keypair.public, addrs);
        const hub_hex = hex32(rt.hub_home.hub_keypair.public);
        const fp = noise.fingerprint_prefixed(rt.hub_home.hub_keypair.public);
        const join_cmd = try std.fmt.allocPrint(a, "protectorate-node join {s} && protectorate-node authorize", .{tok});

        var pending_list: std.ArrayList(schema.PendingNode) = .empty;
        {
            rt.meta_mu.lockUncancelable(rt.io);
            defer rt.meta_mu.unlock(rt.io);
            var pit = rt.pending_nodes.iterator();
            while (pit.next()) |pe| {
                const pr = pe.value_ptr.*;
                const nfp = noise.fingerprint_prefixed(pr.pub_key);
                try pending_list.append(a, .{
                    .auth_id = pr.auth_id,
                    .name = pr.name,
                    .control_key = try a.dupe(u8, &pr.pub_hex),
                    .seen_at_ms = .{ .ms = pr.seen_at_ms },
                    .expires_at_ms = .{ .ms = pr.expires_at_ms },
                    .key_matches = true,
                    .fingerprint = try a.dupe(u8, &nfp),
                    .addrs = pr.addrs,
                });
            }
        }

        return ok_json(rt.gpa, schema.Enrollment{
            .server_url = if (addrs.len > 0) addrs[0] else "",
            .control_key = tok,
            .join_command = join_cmd,
            .pending = pending_list.items,
            .hub_fingerprint = &fp,
            .hub_public_key = &hub_hex,
            .addrs = addrs,
        });
    } else if (std.mem.eql(u8, method, "AddNode")) {
        const req = schema.decode_arena(schema.AddNodeRequest, a, body) catch return err_json(rt.gpa, .invalid, "bad request");
        const parsed = parse_registration(req.registration) orelse return err_json(rt.gpa, .invalid, "registration must be 64-hex node_key or name:64-hex");
        var pk: [32]u8 = undefined;
        _ = std.fmt.hexToBytes(&pk, &parsed.hex) catch return err_json(rt.gpa, .invalid, "invalid public key hex");

        const auth_id = try rt.gen_id(a, "node");
        const now = std.Io.Clock.real.now(rt.io).toMilliseconds();
        const fp = noise.fingerprint_prefixed(pk);

        rt.commit(.{ .node_registered = .{
            .node_id = auth_id,
            .name = parsed.name,
            .pub_key = pk,
        } }) catch |err| return map_commit_err(rt.gpa, err);

        {
            const aid_d = try rt.gpa.dupe(u8, auth_id);
            const name_d = try rt.gpa.dupe(u8, parsed.name);
            rt.meta_mu.lockUncancelable(rt.io);
            defer rt.meta_mu.unlock(rt.io);
            try rt.pending_nodes.put(rt.gpa, aid_d, .{
                .auth_id = aid_d,
                .name = name_d,
                .pub_key = pk,
                .pub_hex = parsed.hex,
                .seen_at_ms = now,
                .expires_at_ms = now + 3600_000,
                .addrs = &.{},
            });
        }

        return ok_json(rt.gpa, schema.PendingNode{
            .auth_id = auth_id,
            .name = parsed.name,
            .control_key = &parsed.hex,
            .seen_at_ms = .{ .ms = now },
            .expires_at_ms = .{ .ms = now + 3600_000 },
            .key_matches = true,
            .fingerprint = &fp,
        });
    } else if (std.mem.eql(u8, method, "ApproveNode")) {
        const req = schema.decode_arena(schema.ApproveNodeRequest, a, body) catch return err_json(rt.gpa, .invalid, "bad request");
        var chosen_name = req.name orelse "";
        {
            rt.meta_mu.lockUncancelable(rt.io);
            if (rt.pending_nodes.get(req.auth_id)) |kv| {
                if (req.fingerprint) |want_fp| {
                    if (want_fp.len > 0) {
                        const actual_fp = noise.fingerprint_prefixed(kv.pub_key);
                        if (!std.mem.eql(u8, want_fp, &actual_fp)) {
                            rt.meta_mu.unlock(rt.io);
                            return err_json(rt.gpa, .invalid, "fingerprint mismatch");
                        }
                    }
                }
            }
            if (rt.pending_nodes.fetchRemove(req.auth_id)) |kv| {
                if (chosen_name.len == 0) chosen_name = try a.dupe(u8, kv.value.name);
                rt.gpa.free(kv.key);
                rt.gpa.free(kv.value.name);
                rt.gpa.free(kv.value.addrs);
            }
            rt.meta_mu.unlock(rt.io);
        }
        if (chosen_name.len == 0) chosen_name = req.auth_id;

        rt.commit(.{ .node_approved = .{
            .node_id = req.auth_id,
            .name = chosen_name,
        } }) catch |err| return map_commit_err(rt.gpa, err);

        const addrs = try rt.advertise_addrs(a);
        const hub_hex = hex32(rt.hub_home.hub_keypair.public);
        const fp = noise.fingerprint_prefixed(rt.hub_home.hub_keypair.public);
        return ok_json(rt.gpa, schema.ApprovedNode{
            .node_id = req.auth_id,
            .name = chosen_name,
            .hub_public_key = &hub_hex,
            .hub_fingerprint = &fp,
            .addrs = addrs,
        });
    } else if (std.mem.eql(u8, method, "RemoveNode")) {
        const req = schema.decode_arena(schema.RemoveNodeRequest, a, body) catch return err_json(rt.gpa, .invalid, "bad request");
        const nid = rt.resolve_node_id(a, req.node_id) orelse return err_json(rt.gpa, .not_found, "node not found");
        const old_name = if (try rt.view_node(a, nid)) |nd| nd.name else nid;
        rt.commit(.{ .node_removed = .{ .node_id = nid } }) catch |err| return map_commit_err(rt.gpa, err);
        rt.fleet_srv.disconnect_node(nid);
        return ok_json(rt.gpa, schema.RemovedNode{ .node_id = nid, .name = old_name });
    } else if (std.mem.eql(u8, method, "RenameNode")) {
        const req = schema.decode_arena(schema.RenameNodeRequest, a, body) catch return err_json(rt.gpa, .invalid, "bad request");
        const nid = rt.resolve_node_id(a, req.node_id) orelse return err_json(rt.gpa, .not_found, "node not found");
        const old_name = if (try rt.view_node(a, nid)) |nd| nd.name else nid;
        rt.commit(.{ .node_renamed = .{ .node_id = nid, .name = req.name } }) catch |err| return map_commit_err(rt.gpa, err);
        return ok_json(rt.gpa, schema.RenamedNode{ .node_id = nid, .from = old_name, .to = req.name });
    } else if (std.mem.eql(u8, method, "RunJob")) {
        const req = schema.decode_arena(schema.RunJobRequest, a, body) catch return err_json(rt.gpa, .invalid, "bad request");
        if (req.argv.len == 0) return err_json(rt.gpa, .invalid, "argv must not be empty");
        const job_summary = try std.mem.join(a, " ", req.argv);
        if (rt.gate_d9(a, principal, .run_job, job_summary)) |_| {} else |err| {
            return map_d9_err(rt.gpa, err);
        }
        const jid = try rt.gen_id(a, "job");
        const jid_d = try rt.gpa.dupe(u8, jid);
        const log_path = try std.fmt.allocPrint(rt.gpa, "{s}/{s}.log", .{ rt.hub_home.artifacts_path, jid });
        const ca = rt.core_state.alloc();

        const argv_c = try ca.alloc([]const u8, req.argv.len);
        for (req.argv, 0..) |s, i| argv_c[i] = try ca.dupe(u8, s);
        const arts_c = try ca.alloc([]const u8, req.artifacts.len);
        for (req.artifacts, 0..) |s, i| arts_c[i] = try ca.dupe(u8, s);

        const target_copy: schema.JobTarget = switch (req.target) {
            .node => |n| .{ .node = .{ .node_id = rt.resolve_node_id(ca, n.node_id) orelse try ca.dupe(u8, n.node_id) } },
            .arch => |ar| .{ .arch = .{ .arch = try ca.dupe(u8, ar.arch) } },
            .unknown => .{ .arch = .{ .arch = "x86_64" } },
        };
        {
            rt.meta_mu.lockUncancelable(rt.io);
            defer rt.meta_mu.unlock(rt.io);
            try rt.job_meta.put(rt.gpa, jid_d, .{
                .target = target_copy,
                .argv = argv_c,
                .cwd = if (req.cwd) |c| try ca.dupe(u8, c) else null,
                .decl_artifacts = arts_c,
                .log_path = log_path,
                .started_at_ms = std.Io.Clock.real.now(rt.io).toMilliseconds(),
                .timeout_secs = req.timeout_secs,
                .exclusive = req.exclusive,
                .slots = if (req.slots == 0) 1 else req.slots,
                .long_slots = req.long_slots,
                .mem_bytes = req.mem_bytes,
                .wait_ms = if (req.wait_ms) |w| @intCast(w) else null,
                .lease_ttl_ms = if (req.lease_ttl_ms) |t| @intCast(t) else null,
                .parent_epoch = req.parent_epoch,
            });
        }

        const tnode: ?[]const u8 = switch (target_copy) {
            .node => |n| n.node_id,
            else => null,
        };
        const tarch: ?[]const u8 = switch (req.target) {
            .arch => |ar| ar.arch,
            else => null,
        };

        rt.commit(.{ .job_queued = .{
            .job_id = jid,
            .target_node = tnode,
            .target_arch = tarch,
            .requested_by = if (principal != null and principal.? == .envoy) "envoy" else "owner",
            .on_present = switch (req.on_present) {
                .kill => .kill,
                else => .idle,
            },
            .even_if_present = req.even_if_present,
            .maintenance = req.maintenance,
        } }) catch |err| return map_commit_err(rt.gpa, err);

        const jv = (try rt.view_job(a, jid)) orelse return err_json(rt.gpa, .not_found, "job missing");
        return ok_json(rt.gpa, jv);
    } else if (std.mem.eql(u8, method, "ListJobs")) {
        const req = schema.decode_arena(schema.ListJobsRequest, a, body) catch schema.ListJobsRequest{};
        const st = try rt.view_state(a);
        const want_id = if (req.node_id) |w| (rt.resolve_node_id(a, w) orelse w) else null;
        var list: std.ArrayList(schema.Job) = .empty;
        for (st.jobs) |j| {
            if (want_id) |want| {
                if (j.node_id == null or !std.mem.eql(u8, j.node_id.?, want)) continue;
            }
            try list.append(a, j);
            if (req.limit) |lim| {
                if (list.items.len >= lim) break;
            }
        }
        return ok_json(rt.gpa, schema.JobList{ .jobs = list.items });
    } else if (std.mem.eql(u8, method, "GetJob")) {
        const req = schema.decode_arena(schema.JobRef, a, body) catch return err_json(rt.gpa, .invalid, "bad request");
        const jv = (try rt.view_job(a, req.job_id)) orelse return err_json(rt.gpa, .not_found, "job not found");
        return ok_json(rt.gpa, jv);
    } else if (std.mem.eql(u8, method, "GetJobLog")) {
        const req = schema.decode_arena(schema.GetJobLogRequest, a, body) catch return err_json(rt.gpa, .invalid, "bad request");
        var lp: []const u8 = "";
        {
            rt.meta_mu.lockUncancelable(rt.io);
            if (rt.job_meta.get(req.job_id)) |jm| lp = try a.dupe(u8, jm.log_path);
            rt.meta_mu.unlock(rt.io);
        }
        if (lp.len == 0) return err_json(rt.gpa, .not_found, "job not found");
        const raw = std.Io.Dir.cwd().readFileAlloc(rt.io, lp, a, .limited(16 << 20)) catch "";
        const off: usize = @intCast(@min(req.offset, raw.len));
        const lim: usize = @intCast(@min(req.limit orelse raw.len, raw.len - off));
        return ok_json(rt.gpa, schema.JobLog{
            .job_id = req.job_id,
            .offset = off,
            .next_offset = off + lim,
            .text = raw[off .. off + lim],
            .eof = (off + lim >= raw.len),
        });
    } else if (std.mem.eql(u8, method, "GetJobArtifact")) {
        const req = schema.decode_arena(schema.GetJobArtifactRequest, a, body) catch return err_json(rt.gpa, .invalid, "bad request");
        if (std.fs.path.isAbsolute(req.path) or std.mem.findPos(u8, req.path, 0, "..") != null) {
            return err_json(rt.gpa, .invalid, "invalid artifact path");
        }
        const art_path = try std.fs.path.join(a, &.{ rt.hub_home.artifacts_path, req.job_id, req.path });
        const raw = std.Io.Dir.cwd().readFileAlloc(rt.io, art_path, a, .limited(64 << 20)) catch {
            return err_json(rt.gpa, .not_found, "artifact not found");
        };
        const off: usize = @intCast(@min(req.offset, raw.len));
        const lim: usize = @intCast(@min(req.limit orelse raw.len, raw.len - off));
        const slice = raw[off .. off + lim];

        var digest: [32]u8 = undefined;
        std.crypto.hash.sha2.Sha256.hash(raw, &digest, .{});
        const sha_hex = hex32(digest);

        const chars = "0123456789abcdef";
        var data_hex = try a.alloc(u8, slice.len * 2);
        for (slice, 0..) |b, i| {
            data_hex[i * 2] = chars[b >> 4];
            data_hex[i * 2 + 1] = chars[b & 0x0f];
        }

        return ok_json(rt.gpa, schema.JobArtifactData{
            .job_id = req.job_id,
            .path = req.path,
            .offset = off,
            .next_offset = off + lim,
            .size_bytes = raw.len,
            .sha256 = &sha_hex,
            .hex = data_hex,
            .eof = (off + lim >= raw.len),
        });
    } else if (std.mem.eql(u8, method, "CancelJob")) {
        const req = schema.decode_arena(schema.JobRef, a, body) catch return err_json(rt.gpa, .invalid, "bad request");
        rt.commit(.{ .job_cancel_requested = .{ .job_id = req.job_id, .by = .owner } }) catch |err| return map_commit_err(rt.gpa, err);
        const jv = (try rt.view_job(a, req.job_id)) orelse return err_json(rt.gpa, .not_found, "job not found");
        return ok_json(rt.gpa, jv);
    } else if (std.mem.eql(u8, method, "ListQuotas")) {
        const disable_remote = rt.env.get("PROTECTORATE_NO_REMOTE_QUOTA") != null or rt.env.get("PROTECTORATE_NO_HOST_HISTORY") != null;
        if (!disable_remote and !rt.quotas_remote_polled.load(.acquire)) {
            rt.refresh_quotas_sync(true);
        } else if (rt.cached_quotas.len == 0) {
            rt.refresh_quotas_sync(false);
        }
        const qs = try rt.copy_cached_quotas(a);
        return ok_json(rt.gpa, schema.QuotaList{ .quotas = qs });
    } else if (std.mem.eql(u8, method, "ListEvents")) {
        const req = schema.decode_arena(schema.ListEventsRequest, a, body) catch schema.ListEventsRequest{};
        rt.meta_mu.lockUncancelable(rt.io);
        defer rt.meta_mu.unlock(rt.io);
        var list: std.ArrayList(schema.HubEvent) = .empty;
        for (rt.hub_events.items) |ev| {
            if (req.from_seq) |fs| {
                if (ev.seq < fs) continue;
            }
            try list.append(a, ev);
            if (req.limit) |lim| {
                if (list.items.len >= lim) break;
            }
        }
        return ok_json(rt.gpa, schema.EventList{ .events = list.items });
    } else if (std.mem.eql(u8, method, "MarkEventsRead")) {
        const req = schema.decode_arena(schema.MarkEventsReadRequest, a, body) catch return err_json(rt.gpa, .invalid, "bad request");
        rt.meta_mu.lockUncancelable(rt.io);
        defer rt.meta_mu.unlock(rt.io);
        for (rt.hub_events.items) |*ev| {
            if (ev.seq <= req.up_to_seq) ev.read = true;
        }
        return ok_json(rt.gpa, schema.Empty{});
    } else if (std.mem.eql(u8, method, "EnvoySend")) {
        const req = schema.decode_arena(schema.EnvoySendRequest, a, body) catch return err_json(rt.gpa, .invalid, "bad request");
        const rid = try rt.gen_id(a, "envoy");
        const vendor = req.agent != null or (principal != null and principal.? == .envoy);
        const from = if (vendor) try std.fmt.allocPrint(a, "agent:{s}", .{req.agent orelse ""}) else "owner";
        rt.commit(.{ .prompt_enqueued = .{
            .sleeve_id = "envoy",
            .request_id = rid,
            .from = from,
            .kind = if (vendor) .agent_mail else .prompt,
            .mode = .queue,
            .text = if (vendor) try std.fmt.allocPrint(a, "[from {s}]\n{s}", .{ from, req.text }) else req.text,
        } }) catch |err| return map_commit_err(rt.gpa, err);
        return ok_json(rt.gpa, schema.Sent{ .request_id = rid, .queued = true, .queue_length = 1 });
    } else if (std.mem.eql(u8, method, "EnvoyWait")) {
        const req = schema.decode_arena(schema.EnvoyWaitRequest, a, body) catch schema.EnvoyWaitRequest{};
        const deadline = std.Io.Clock.real.now(rt.io).toMilliseconds() + @as(i64, @intCast(@min(req.timeout_ms, 60_000)));
        var cursor: u64 = 0;
        var settled = false;
        while (true) {
            {
                rt.state_mu.lockUncancelable(rt.io);
                defer rt.state_mu.unlock(rt.io);
                if (rt.core_state.sleeves.get("envoy")) |sl| {
                    if (req.request_id) |rid| {
                        var pending = false;
                        for (sl.queued.items) |qi| {
                            if (std.mem.eql(u8, qi.request_id, rid)) {
                                pending = true;
                                break;
                            }
                        }
                        if (!pending) {
                            if (sl.inc) |inc| {
                                const same_req = if (inc.delivered_request_id) |dr| std.mem.eql(u8, dr, rid) else false;
                                if (!same_req and inc.activity == .idle and sl.queued.items.len == 0) {
                                    settled = true;
                                }
                            } else {
                                settled = true;
                            }
                        }
                    } else {
                        settled = (sl.lifecycle == .stopped or sl.lifecycle == .parked) or
                            (sl.inc != null and sl.inc.?.activity == .idle and sl.queued.items.len == 0);
                    }
                } else {
                    settled = true;
                }
            }
            if (settled) break;
            const now = std.Io.Clock.real.now(rt.io).toMilliseconds();
            if (now >= deadline) break;
            cursor = try rt.stream_hub.wait_change(cursor, @min(500, deadline - now));
        }
        const reply_text = extract_envoy_reply(a, rt, req.request_id);
        return ok_json(rt.gpa, schema.EnvoyReply{
            .request_id = req.request_id orelse "",
            .settled = settled,
            .text = reply_text,
            .seq = rt.stream_hub.current_seq(),
        });
    } else if (std.mem.eql(u8, method, "EnvoyRead")) {
        const req = schema.decode_arena(schema.EnvoyReadRequest, a, body) catch schema.EnvoyReadRequest{};
        const tx = try rt.transcripts.read(a, "envoy", req.from_seq, req.limit);
        return ok_json(rt.gpa, tx);
    } else if (std.mem.eql(u8, method, "GetEnvoyToken")) {
        return ok_json(rt.gpa, schema.EnvoyToken{ .token = rt.hub_home.envoy_token });
    } else if (std.mem.eql(u8, method, "Wait")) {
        const req = schema.decode_arena(schema.WaitRequest, a, body) catch return err_json(rt.gpa, .invalid, "bad request");
        const deadline = std.Io.Clock.real.now(rt.io).toMilliseconds() + @as(i64, @intCast(@min(req.timeout_ms, 60_000)));
        var cursor = req.after_seq orelse 0;
        var res_sleeves: std.ArrayList(schema.WaitSleeveResult) = .empty;
        var reached = false;
        while (true) {
            res_sleeves.clearRetainingCapacity();
            var match_count: usize = 0;
            for (req.sleeve_ids) |raw_sid| {
                const sid = rt.resolve_sleeve_id(a, raw_sid) orelse raw_sid;
                const sv = (try rt.view_sleeve(a, sid)) orelse continue;
                const ok = switch (req.until) {
                    .idle => sv.lifecycle == .stopped or (sv.incarnation != null and sv.incarnation.?.activity.state == .idle and sv.queued == 0),
                    .done => sv.lifecycle == .stopped or (sv.incarnation != null and sv.incarnation.?.activity.state == .idle and sv.queued == 0),
                    .blocked => sv.incarnation != null and sv.incarnation.?.activity.state == .blocked,
                    .ended => sv.lifecycle == .stopped or sv.lifecycle == .removed,
                    .parked => sv.lifecycle == .parked,
                    .report, .unknown => sv.incarnation != null,
                };
                if (ok) match_count += 1;
                try res_sleeves.append(a, .{ .reached = ok, .sleeve = sv, .seq = rt.stream_hub.current_seq() });
            }
            if (req.sleeve_ids.len == 0 or (if (req.any) match_count > 0 else match_count == req.sleeve_ids.len)) {
                reached = true;
                break;
            }
            const now = std.Io.Clock.real.now(rt.io).toMilliseconds();
            if (now >= deadline) break;
            cursor = try rt.stream_hub.wait_change(cursor, @min(500, deadline - now));
        }
        return ok_json(rt.gpa, schema.WaitResult{ .reached = reached, .sleeves = res_sleeves.items });
    } else if (std.mem.eql(u8, method, "SearchHistory")) {
        const req = schema.decode_arena(schema.SearchHistoryRequest, a, body) catch return err_json(rt.gpa, .invalid, "bad request");
        _ = rt.history.ingest_roots(null, rt.hub_home.transcripts_path);
        const hmode: h_seg.QueryMode = switch (req.mode) {
            .any => .any,
            .phrase => .phrase,
            else => .all,
        };
        var hroles: std.ArrayList(h_ext.Role) = .empty;
        for (req.roles) |r| {
            switch (r) {
                .user => try hroles.append(a, .user),
                .assistant => try hroles.append(a, .assistant),
                .tool_call => try hroles.append(a, .tool_call),
                .tool_error => try hroles.append(a, .tool_error),
                .compaction => try hroles.append(a, .compaction),
                .unknown => {},
            }
        }
        const hhar: ?h_ext.Harness = if (req.harness) |hh| switch (hh) {
            .claude => .claude,
            .codex => .codex,
            .grok => .grok,
            .pi_vendor => .pi_vendor,
            .pi_local => .pi_local,
            .antigravity => .antigravity,
            .unknown => null,
        } else null;

        const res = try rt.history.search(a, req.query, hmode, .{
            .sleeves = req.sleeves,
            .session = req.session,
            .harness = hhar,
            .since_ms = if (req.since_ms) |s| @intCast(@max(0, s.ms)) else null,
            .until_ms = if (req.until_ms) |u| @intCast(@max(0, u.ms)) else null,
            .roles = hroles.items,
        }, @intCast(req.limit orelse 20));

        var hits = try a.alloc(schema.HistoryHit, res.hits.len);
        for (res.hits, 0..) |hit, i| {
            var spans = try a.alloc(schema.HistorySpan, hit.highlights.len);
            for (hit.highlights, 0..) |sp, si| spans[si] = .{ .start = sp.start, .end = sp.end };
            hits[i] = .{
                .doc = .{
                    .doc_id = hit.doc_id,
                    .sleeve_id = hit.sleeve_id,
                    .sleeve = hit.sleeve,
                    .harness = if (hit.harness) |hh| map_ext_harness(hh) else null,
                    .session = hit.session,
                    .turn = hit.turn,
                    .role = map_ext_role(hit.role),
                    .time_ms = .{ .ms = @intCast(hit.time_ms) },
                    .tool = hit.tool,
                    .files = hit.files,
                    .path = hit.path,
                    .offset = hit.offset,
                },
                .score = hit.score,
                .snippet = hit.snippet,
                .highlights = spans,
            };
        }
        return ok_json(rt.gpa, schema.HistoryHits{ .hits = hits, .indexed_docs = res.indexed_docs });
    } else if (std.mem.eql(u8, method, "ReadHistory")) {
        const req = schema.decode_arena(schema.ReadHistoryRequest, a, body) catch return err_json(rt.gpa, .invalid, "bad request");
        const rr = try rt.history.read_docs(a, req.doc_id, req.session, req.from_turn, req.to_turn, req.after_doc, 50);
        var docs = try a.alloc(schema.HistoryText, rr.docs.len);
        for (rr.docs, 0..) |d, i| {
            docs[i] = .{
                .doc = .{
                    .doc_id = d.doc_id,
                    .sleeve_id = d.sleeve_id,
                    .sleeve = d.sleeve,
                    .harness = if (d.harness) |hh| map_ext_harness(hh) else null,
                    .session = d.session,
                    .turn = d.turn,
                    .role = map_ext_role(d.role),
                    .time_ms = .{ .ms = @intCast(d.time_ms) },
                    .tool = d.tool,
                    .files = d.files,
                    .path = d.path,
                    .offset = d.offset,
                },
                .text = d.text,
            };
        }
        return ok_json(rt.gpa, schema.HistoryRead{ .docs = docs, .more = rr.more });
    } else if (std.mem.eql(u8, method, "GetDoctor")) {
        var rep = try doctor.run_checks(a, rt.io, rt.env.get("HOME"));
        if (rt.hub_home.config.sleeve_devices.len > 0) {
            var devs: std.ArrayList([]const u8) = .empty;
            var probs: std.ArrayList([]const u8) = .empty;
            try probs.appendSlice(a, rep.problems);
            for (rt.hub_home.config.sleeve_devices) |dp| {
                if (runtime.Runtime.stat_device_inode(rt.io, dp)) |ino| {
                    try devs.append(a, try std.fmt.allocPrint(a, "{s}: present (ino {d})", .{ dp, ino }));
                } else {
                    try devs.append(a, try std.fmt.allocPrint(a, "{s}: missing", .{dp}));
                    try probs.append(a, try std.fmt.allocPrint(a, "sleeve device missing: {s}", .{dp}));
                }
            }
            rep.sleeve_devices = devs.items;
            rep.problems = probs.items;
        }
        return ok_json(rt.gpa, rep);
    } else if (std.mem.eql(u8, method, "UpdateHarnesses")) {
        const req = schema.decode_arena(schema.UpdateHarnessesRequest, a, body) catch schema.UpdateHarnessesRequest{};
        const progs = try rt.update_harnesses(a, req.programs);
        return ok_json(rt.gpa, schema.HarnessUpdates{ .programs = progs });
    } else if (std.mem.eql(u8, method, "UpdateHub")) {
        const req = schema.decode_arena(schema.UpdateHubRequest, a, body) catch schema.UpdateHubRequest{};
        const cur_ver = @import("../version.zig").version;
        if (req.rollback) {
            rt.release_mu.lockUncancelable(rt.io);
            const rolled = u_store.rollback(a, rt.io, rt.hub_home.releases_path);
            rt.release_mu.unlock(rt.io);
            const target = rolled catch {
                return err_json(rt.gpa, .not_found, "no previous release");
            };
            rt.record_hub_event(.hub, "hub_update", target, "rolled back release", .warn);
            return ok_json(rt.gpa, schema.HubUpdate{ .current = cur_ver, .target = target, .available = true });
        }
        if (req.version) |want_ver| {
            if (std.mem.eql(u8, want_ver, cur_ver)) {
                return ok_json(rt.gpa, schema.HubUpdate{ .current = cur_ver, .target = cur_ver, .available = false });
            }
            const activated = if (req.check) false else blk: {
                rt.release_mu.lockUncancelable(rt.io);
                defer rt.release_mu.unlock(rt.io);
                break :blk u_store.activate_staged(rt.gpa, rt.io, rt.hub_home.releases_path, want_ver) catch false;
            };
            if (activated) {
                rt.record_hub_event(.hub, "hub_update", want_ver, "activated staged release", .info);
                return ok_json(rt.gpa, schema.HubUpdate{ .current = cur_ver, .target = want_ver, .available = true });
            }
        }
        const cfg_ch: u_ver.Channel = if (std.mem.eql(u8, rt.hub_home.config.update_channel, "dev")) .dev else .stable;
        const ch: u_ver.Channel = if (req.channel) |rc| (if (rc == .dev) .dev else .stable) else cfg_ch;
        const latest = u_store.check_latest(a, rt.io, rt.hub_home.config.update_repo, ch, cur_ver, .hub, req.version) catch |err| {
            return err_json(rt.gpa, .internal, @errorName(err));
        };
        if (latest) |rel| {
            if (!req.check) {
                u_store.stage_release(rt.gpa, rt.io, rt.hub_home.releases_path, rel, .hub) catch |err| {
                    return err_json(rt.gpa, .internal, @errorName(err));
                };
                rt.release_mu.lockUncancelable(rt.io);
                const applied = u_store.activate_staged(rt.gpa, rt.io, rt.hub_home.releases_path, rel.tag);
                rt.release_mu.unlock(rt.io);
                _ = applied catch |err| {
                    return err_json(rt.gpa, .internal, @errorName(err));
                };
                u_store.cache_release_clients(rt.gpa, rt.io, rt.hub_home.clients_path, rel) catch {};
                rt.record_hub_event(.hub, "hub_update", rel.tag, "staged hub update", .info);
            }
            return ok_json(rt.gpa, schema.HubUpdate{
                .current = cur_ver,
                .target = rel.tag,
                .available = true,
            });
        }
        return ok_json(rt.gpa, schema.HubUpdate{
            .current = cur_ver,
            .target = cur_ver,
            .available = false,
        });
    } else if (std.mem.eql(u8, method, "UpdateNodes")) {
        const req = schema.decode_arena(schema.UpdateNodesRequest, a, body) catch schema.UpdateNodesRequest{};
        const cur_ver = @import("../version.zig").version;
        if (req.cancel) |cid| {
            rt.commit(.{ .rollout_cancelled = .{ .rollout_id = cid } }) catch |err| return map_commit_err(rt.gpa, err);
            rt.record_hub_event(.node, "node_rollout", cid, "rollout cancelled", .warn);
            const updates = try rt.collect_rollout_updates(a, cid);
            return ok_json(rt.gpa, schema.NodeUpdates{
                .version = req.version orelse cur_ver,
                .nodes = updates,
                .rollout_id = cid,
            });
        }
        const res = try rt.start_and_wait_node_rollout(a, req);
        return ok_json(rt.gpa, res);
    } else if (std.mem.eql(u8, method, "AcquireLease")) {
        const req = schema.decode_arena(schema.AcquireLeaseRequest, a, body) catch return err_json(rt.gpa, .invalid, "bad request");
        const nid = rt.resolve_node_id(a, req.node_id) orelse return err_json(rt.gpa, .not_found, "node not found");
        const rep = rt.fleet_srv.acquire_lease(a, nid, .{
            .holder_id = req.holder_id,
            .exclusive = req.exclusive,
            .slots = if (req.slots == 0 and !req.exclusive) 1 else req.slots,
            .long_slots = req.long_slots,
            .mem_bytes = req.mem_bytes,
            .ttl_ms = if (req.ttl_ms) |t| @intCast(t) else null,
            .wait_ms = if (req.wait_ms) |w| @intCast(w) else null,
            .parent_epoch = req.parent_epoch,
        }) catch |err| return map_fleet_err(rt.gpa, err);
        return ok_json(rt.gpa, schema.LeaseGrant{
            .ok = rep.ok,
            .node_id = nid,
            .boot_id = rep.boot_id,
            .lease_id = rep.lease_id,
            .epoch = rep.epoch,
            .parent_epoch = rep.parent_epoch,
            .deadline_ms = if (rep.deadline_ms > 0) .{ .ms = rep.deadline_ms } else null,
            .reason = rep.reason,
        });
    } else if (std.mem.eql(u8, method, "RenewLease")) {
        const req = schema.decode_arena(schema.RenewLeaseRequest, a, body) catch return err_json(rt.gpa, .invalid, "bad request");
        const nid = rt.resolve_node_id(a, req.node_id) orelse return err_json(rt.gpa, .not_found, "node not found");
        const rep = rt.fleet_srv.renew_lease(a, nid, .{
            .lease_id = req.lease_id,
            .epoch = req.epoch,
        }) catch |err| return map_fleet_err(rt.gpa, err);
        return ok_json(rt.gpa, schema.LeaseGrant{
            .ok = rep.ok,
            .node_id = nid,
            .boot_id = rep.boot_id,
            .lease_id = rep.lease_id,
            .epoch = rep.epoch,
            .parent_epoch = rep.parent_epoch,
            .deadline_ms = if (rep.deadline_ms > 0) .{ .ms = rep.deadline_ms } else null,
            .reason = rep.reason,
        });
    } else if (std.mem.eql(u8, method, "ReleaseLease")) {
        const req = schema.decode_arena(schema.ReleaseLeaseRequest, a, body) catch return err_json(rt.gpa, .invalid, "bad request");
        const nid = rt.resolve_node_id(a, req.node_id) orelse return err_json(rt.gpa, .not_found, "node not found");
        const rep = rt.fleet_srv.release_lease(a, nid, .{
            .lease_id = req.lease_id,
            .epoch = req.epoch,
        }) catch |err| return map_fleet_err(rt.gpa, err);
        return ok_json(rt.gpa, schema.ReleaseLeaseResult{
            .ok = rep.ok,
            .fenced = rep.fenced,
            .quarantined = rep.quarantined,
        });
    } else if (std.mem.eql(u8, method, "VerifyEpoch")) {
        const req = schema.decode_arena(schema.VerifyEpochRequest, a, body) catch return err_json(rt.gpa, .invalid, "bad request");
        const nid = rt.resolve_node_id(a, req.node_id) orelse return err_json(rt.gpa, .not_found, "node not found");
        const rep = rt.fleet_srv.verify_epoch(a, nid, .{
            .boot_id = req.boot_id,
            .epoch = req.epoch,
        }) catch |err| return map_fleet_err(rt.gpa, err);
        return ok_json(rt.gpa, schema.VerifyEpochResult{
            .valid = rep.valid,
            .boot_id = rep.boot_id,
            .state = if (rep.state) |st| map_lease_state(st) else null,
            .expires_in_ms = rep.expires_in_ms,
            .quarantined = rep.quarantined,
            .reason = rep.reason,
        });
    } else if (std.mem.eql(u8, method, "GetNodeAdmission")) {
        const req = schema.decode_arena(schema.NodeRef, a, body) catch return err_json(rt.gpa, .invalid, "bad request");
        const nid = rt.resolve_node_id(a, req.node_id) orelse return err_json(rt.gpa, .not_found, "node not found");
        const st = rt.fleet_srv.list_leases(a, nid) catch |err| return map_fleet_err(rt.gpa, err);
        const grants = try a.alloc(schema.LeaseInfo, st.grants.len);
        for (st.grants, 0..) |g, i| {
            grants[i] = .{
                .lease_id = g.lease_id,
                .epoch = g.epoch,
                .parent_epoch = g.parent_epoch,
                .holder_id = g.holder_id,
                .job_id = g.job_id,
                .mode = if (g.mode == .exclusive) .exclusive else .shared,
                .state = map_lease_state(g.state),
                .slots = g.slots,
                .long_slots = g.long_slots,
                .mem_bytes = g.mem_bytes,
                .deadline_ms = .{ .ms = g.deadline_ms },
            };
        }
        const queue = try a.alloc(schema.TicketInfo, st.queue.len);
        for (st.queue, 0..) |t, i| {
            queue[i] = .{
                .ticket_id = t.ticket_id,
                .seq = t.seq,
                .holder_id = t.holder_id,
                .mode = if (t.mode == .exclusive) .exclusive else .shared,
                .slots = t.slots,
                .long_slots = t.long_slots,
                .mem_bytes = t.mem_bytes,
                .wait_deadline_ms = .{ .ms = t.wait_deadline_ms },
                .lease_deadline_ms = .{ .ms = t.lease_deadline_ms },
            };
        }
        return ok_json(rt.gpa, schema.NodeAdmission{
            .node_id = nid,
            .boot_id = st.boot_id,
            .next_epoch = st.next_epoch,
            .slots_total = st.slots_total,
            .slots_used = st.slots_used,
            .long_slots_total = st.long_slots_total,
            .long_slots_used = st.long_slots_used,
            .mem_margin_bytes = st.mem_margin_bytes,
            .mem_reserved_bytes = st.mem_reserved_bytes,
            .mem_avail_bytes = st.mem_avail_bytes,
            .quarantined = st.quarantined,
            .grants = grants,
            .queue = queue,
        });
    } else if (std.mem.eql(u8, method, "ClearNodeQuarantine")) {
        const req = schema.decode_arena(schema.NodeRef, a, body) catch return err_json(rt.gpa, .invalid, "bad request");
        const nid = rt.resolve_node_id(a, req.node_id) orelse return err_json(rt.gpa, .not_found, "node not found");
        const rep = rt.fleet_srv.clear_quarantine(a, nid) catch |err| return map_fleet_err(rt.gpa, err);
        if (rep.ok and rep.quarantined == null) {
            rt.meta_mu.lockUncancelable(rt.io);
            if (rt.node_tele.getPtr(nid)) |tel| tel.quarantined = null;
            rt.meta_mu.unlock(rt.io);
        }
        return ok_json(rt.gpa, schema.ClearQuarantineResult{
            .node_id = nid,
            .cleared = rep.cleared,
            .quarantined = rep.quarantined,
            .detail = rep.detail,
        });
    } else if (std.mem.eql(u8, method, "ListDevices")) {
        const devs = try rt.hub_home.list_devices(a);
        return ok_json(rt.gpa, schema.DeviceList{ .devices = devs });
    } else if (std.mem.eql(u8, method, "AddDevice")) {
        const req = schema.decode_arena(schema.AddDeviceRequest, a, body) catch return err_json(rt.gpa, .invalid, "bad request");
        const now = std.Io.Clock.real.now(rt.io).toMilliseconds();
        const pc = try rt.hub_home.create_pairing(req.name, now);
        return ok_json(rt.gpa, pc);
    } else if (std.mem.eql(u8, method, "PairDevice")) {
        const req = schema.decode_arena(schema.PairDeviceRequest, a, body) catch return err_json(rt.gpa, .invalid, "bad request");
        const now = std.Io.Clock.real.now(rt.io).toMilliseconds();
        const paired = (try rt.hub_home.claim_pairing(a, req.code, now)) orelse {
            return err_json(rt.gpa, .unauthenticated, "invalid or expired pairing code");
        };
        return ok_json(rt.gpa, paired);
    } else if (std.mem.eql(u8, method, "RevokeDevice")) {
        const req = schema.decode_arena(schema.DeviceRef, a, body) catch return err_json(rt.gpa, .invalid, "bad request");
        _ = try rt.hub_home.revoke_device(req.device_id);
        return ok_json(rt.gpa, schema.Empty{});
    }

    return err_json(rt.gpa, .not_found, "unhandled method");
}

fn find_method_scope(name: []const u8) ?schema.Scope {
    inline for (schema.methods) |m| {
        if (std.mem.eql(u8, m.name, name)) return m.scope;
    }
    return null;
}

fn map_commit_err(gpa: std.mem.Allocator, err: anyerror) !DispatchResult {
    return switch (err) {
        error.NotFound => err_json(gpa, .not_found, @errorName(err)),
        error.DuplicateId, error.WorkspaceConflict, error.InvalidState => err_json(gpa, .conflict, @errorName(err)),
        else => err_json(gpa, .invalid, @errorName(err)),
    };
}

fn map_d9_err(gpa: std.mem.Allocator, err: anyerror) !DispatchResult {
    return switch (err) {
        error.ApprovalDenied => err_json(gpa, .forbidden, "owner denied D9 action"),
        error.ApprovalTimeout => err_json(gpa, .unavailable, "timed out waiting for D9 owner approval"),
        else => err_json(gpa, .internal, @errorName(err)),
    };
}

fn map_fleet_err(gpa: std.mem.Allocator, err: anyerror) !DispatchResult {
    return switch (err) {
        error.NodeOffline => err_json(gpa, .unavailable, "node is offline"),
        error.Busy => err_json(gpa, .unavailable, "fleet is busy"),
        error.RemoteError => err_json(gpa, .invalid, "node rejected request"),
        else => err_json(gpa, .internal, @errorName(err)),
    };
}

fn map_lease_state(st: msg.LeaseState) schema.LeaseState {
    return switch (st) {
        .active => .active,
        .collecting => .collecting,
        .fencing => .fencing,
    };
}

fn extract_envoy_reply(arena: std.mem.Allocator, rt: *runtime.Runtime, request_id: ?[]const u8) []const u8 {
    const tx = rt.transcripts.read(arena, "envoy", 0, null) catch return "";
    if (tx.entries.len == 0) return "";
    var start_idx: usize = 0;
    if (request_id) |want_id| {
        if (want_id.len > 0) {
            var k = tx.entries.len;
            while (k > 0) {
                k -= 1;
                switch (tx.entries[k].entry) {
                    .user_message => |um| {
                        if (std.mem.eql(u8, um.request_id, want_id)) {
                            start_idx = k + 1;
                            break;
                        }
                    },
                    else => {},
                }
            }
        }
    } else {
        var k = tx.entries.len;
        while (k > 0) {
            k -= 1;
            switch (tx.entries[k].entry) {
                .turn_started => {
                    start_idx = k + 1;
                    break;
                },
                else => {},
            }
        }
    }
    var out: std.ArrayList(u8) = .empty;
    for (tx.entries[start_idx..]) |ce| {
        switch (ce.entry) {
            .assistant_text => |at| {
                out.appendSlice(arena, at.text) catch {};
            },
            else => {},
        }
    }
    return out.items;
}

fn parse_registration(reg: []const u8) ?struct { name: []const u8, hex: [64]u8 } {
    const tr = std.mem.trim(u8, reg, " \t\r\n");
    if (std.mem.findScalar(u8, tr, ':')) |colon| {
        const nm = tr[0..colon];
        const k = tr[colon + 1 ..];
        if (nm.len > 0 and k.len == 64) {
            var h: [64]u8 = undefined;
            @memcpy(&h, k);
            return .{ .name = nm, .hex = h };
        }
    }
    if (tr.len == 64) {
        var h: [64]u8 = undefined;
        @memcpy(&h, tr);
        return .{ .name = tr[0..12], .hex = h };
    }
    return null;
}

fn hex32(bytes: [32]u8) [64]u8 {
    const chars = "0123456789abcdef";
    var out: [64]u8 = undefined;
    for (bytes, 0..) |b, i| {
        out[i * 2] = chars[b >> 4];
        out[i * 2 + 1] = chars[b & 0x0f];
    }
    return out;
}

fn map_ext_harness(h: h_ext.Harness) schema.Harness {
    return switch (h) {
        .claude => .claude,
        .codex => .codex,
        .grok => .grok,
        .pi_vendor => .pi_vendor,
        .pi_local => .pi_local,
        .antigravity => .antigravity,
    };
}

fn map_ext_role(r: h_ext.Role) schema.HistoryRole {
    return switch (r) {
        .user => .user,
        .assistant => .assistant,
        .tool_call => .tool_call,
        .tool_error => .tool_error,
        .compaction => .compaction,
    };
}
