const std = @import("std");
const tiger = @import("../tiger.zig");
const schema = @import("../api/schema.zig");
const model = @import("../core/model.zig");
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

fn err_json(gpa: std.mem.Allocator, code: schema.ErrorCode, msg: []const u8) !DispatchResult {
    const body = schema.ErrorBody{ .@"error" = .{ .code = code, .message = msg } };
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
        const sid = req.sleeve_id orelse "envoy";
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
        rt.commit(.{ .drop_queued = .{ .sleeve_id = req.sleeve_id } }) catch return err_json(rt.gpa, .not_found, "sleeve not found");
        return ok_json(rt.gpa, schema.Empty{});
    } else if (std.mem.eql(u8, method, "GetTranscript")) {
        const req = schema.decode_arena(schema.GetTranscriptRequest, a, body) catch return err_json(rt.gpa, .invalid, "bad request");
        const tx = try rt.transcripts.read(a, req.sleeve_id, req.from_seq, req.limit);
        return ok_json(rt.gpa, tx);
    } else if (std.mem.eql(u8, method, "SpawnSleeve")) {
        const req = schema.decode_arena(schema.SpawnSleeveRequest, a, body) catch return err_json(rt.gpa, .invalid, "bad request");
        const path = req.new_path orelse req.path orelse return err_json(rt.gpa, .invalid, "path required");
        const sid = try rt.gen_id(a, "slv");
        const ws_id = try rt.gen_id(a, "ws");
        const layout: model.Layout = if (req.view) |v| switch (v) {
            .readonly => .readonly,
            .worktree => .worktree,
            .unknown => .shared,
        } else .shared;
        try rt.register_workspace(ws_id, req.name, path);
        rt.commit(.{ .sleeve_created = .{
            .sleeve_id = sid,
            .name = req.name,
            .role = if (req.parent_id != null) .child else .root,
            .path = path,
            .workspace_id = ws_id,
            .parent_id = req.parent_id,
            .layout = layout,
            .budget_limit = req.budget,
            .until_done = req.until_done orelse 0,
        } }) catch |err| return map_commit_err(rt.gpa, err);

        var targets: [model.max_targets]model.Harness = undefined;
        targets[0] = runtime.schema_to_model_harness(req.harness);
        var tlen: u8 = 1;
        for (req.resleeve_to) |rh| {
            if (tlen >= model.max_targets) break;
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
            .model_name = req.model,
            .provider = req.provider,
            .effort = req.effort,
            .until_done = req.until_done,
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
        var targets: [model.max_targets]model.Harness = undefined;
        targets[0] = if (req.target) |h| runtime.schema_to_model_harness(h) else .pi_local;
        rt.commit(.{ .start_requested = .{
            .sleeve_id = req.sleeve_id,
            .targets = targets,
            .target_len = 1,
            .mode = .headless,
            .model_name = req.model,
            .provider = req.provider,
            .effort = req.effort,
            .until_done = req.until_done,
        } }) catch |err| return map_commit_err(rt.gpa, err);
        return ok_json(rt.gpa, schema.Accepted{});
    } else if (std.mem.eql(u8, method, "SendMessage")) {
        const req = schema.decode_arena(schema.SendMessageRequest, a, body) catch return err_json(rt.gpa, .invalid, "bad request");
        const rid = try rt.gen_id(a, "req");
        const mode: model.SendMode = if (req.delivery == .steer) .steer else .queue;
        rt.commit(.{ .prompt_enqueued = .{
            .sleeve_id = req.sleeve_id,
            .request_id = rid,
            .from = if (principal != null and principal.? == .envoy) "envoy" else "owner",
            .kind = if (principal != null and principal.? == .envoy) .mail else .prompt,
            .mode = mode,
            .text = req.text,
            .supersede = req.supersede,
            .until_done = req.until_done,
        } }) catch |err| return map_commit_err(rt.gpa, err);
        const sv = try rt.view_sleeve(a, req.sleeve_id);
        return ok_json(rt.gpa, schema.Sent{
            .request_id = rid,
            .queued = true,
            .queue_length = if (sv) |s| s.queued else 0,
        });
    } else if (std.mem.eql(u8, method, "Interrupt")) {
        const req = schema.decode_arena(schema.SleeveRef, a, body) catch return err_json(rt.gpa, .invalid, "bad request");
        const sv = (try rt.view_sleeve(a, req.sleeve_id)) orelse return err_json(rt.gpa, .not_found, "sleeve not found");
        if (sv.incarnation) |inc| {
            rt.commit(.{ .interrupted = .{ .sleeve_id = req.sleeve_id, .g = inc.g, .by = .owner } }) catch {};
        }
        return ok_json(rt.gpa, schema.Empty{});
    } else if (std.mem.eql(u8, method, "StopSleeve")) {
        const req = schema.decode_arena(schema.SleeveRef, a, body) catch return err_json(rt.gpa, .invalid, "bad request");
        rt.commit(.{ .stop_requested = .{ .sleeve_id = req.sleeve_id, .by = .owner } }) catch |err| return map_commit_err(rt.gpa, err);
        return ok_json(rt.gpa, schema.Accepted{});
    } else if (std.mem.eql(u8, method, "RemoveSleeve")) {
        const req = schema.decode_arena(schema.SleeveRef, a, body) catch return err_json(rt.gpa, .invalid, "bad request");
        rt.commit(.{ .removed = .{ .sleeve_id = req.sleeve_id } }) catch |err| return map_commit_err(rt.gpa, err);
        return ok_json(rt.gpa, schema.Accepted{});
    } else if (std.mem.eql(u8, method, "Resleeve")) {
        const req = schema.decode_arena(schema.ResleeveRequest, a, body) catch return err_json(rt.gpa, .invalid, "bad request");
        rt.commit(.{ .resleeve_requested = .{
            .sleeve_id = req.sleeve_id,
            .harness = runtime.schema_to_model_harness(req.target),
            .model_name = req.model,
            .provider = req.provider,
            .effort = req.effort,
            .by = .owner,
            .cause = .manual,
        } }) catch |err| return map_commit_err(rt.gpa, err);
        return ok_json(rt.gpa, schema.Accepted{});
    } else if (std.mem.eql(u8, method, "TakeoverTerminal")) {
        return err_json(rt.gpa, .unavailable, "takeover terminal is not implemented");
    } else if (std.mem.eql(u8, method, "GitPush")) {
        const req = schema.decode_arena(schema.GitPushRequest, a, body) catch return err_json(rt.gpa, .invalid, "bad request");
        const sv = (try rt.view_sleeve(a, req.sleeve_id)) orelse return err_json(rt.gpa, .not_found, "sleeve not found");
        const res = git.push(a, rt.io, req.sleeve_id, sv.path, req.remote, req.branch, req.force) catch |err| {
            return err_json(rt.gpa, .invalid, @errorName(err));
        };
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
        const nd = (try rt.view_node(a, req.node_id)) orelse return err_json(rt.gpa, .not_found, "node not found");
        return ok_json(rt.gpa, nd);
    } else if (std.mem.eql(u8, method, "SetNodeJobs")) {
        const req = schema.decode_arena(schema.SetNodeJobsRequest, a, body) catch return err_json(rt.gpa, .invalid, "bad request");
        const jp: model.NodeJobs = switch (req.jobs) {
            .any => .any,
            .owner => .owner,
            .none, .unknown => .none,
        };
        rt.commit(.{ .node_jobs_updated = .{ .node_id = req.node_id, .jobs = jp, .note = req.note } }) catch |err| return map_commit_err(rt.gpa, err);
        const nd = (try rt.view_node(a, req.node_id)) orelse return err_json(rt.gpa, .not_found, "node not found");
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
        rt.fleet_srv.disconnect_node(req.node_id);
        rt.commit(.{ .node_removed = .{ .node_id = req.node_id } }) catch |err| return map_commit_err(rt.gpa, err);
        return ok_json(rt.gpa, schema.RemovedNode{ .node_id = req.node_id, .name = req.node_id });
    } else if (std.mem.eql(u8, method, "RenameNode")) {
        const req = schema.decode_arena(schema.RenameNodeRequest, a, body) catch return err_json(rt.gpa, .invalid, "bad request");
        rt.commit(.{ .node_renamed = .{ .node_id = req.node_id, .name = req.name } }) catch |err| return map_commit_err(rt.gpa, err);
        return ok_json(rt.gpa, schema.RenamedNode{ .node_id = req.node_id, .from = req.node_id, .to = req.name });
    } else if (std.mem.eql(u8, method, "RunJob")) {
        const req = schema.decode_arena(schema.RunJobRequest, a, body) catch return err_json(rt.gpa, .invalid, "bad request");
        if (req.argv.len == 0) return err_json(rt.gpa, .invalid, "argv must not be empty");
        const jid = try rt.gen_id(a, "job");
        const jid_d = try rt.gpa.dupe(u8, jid);
        const log_path = try std.fmt.allocPrint(rt.gpa, "{s}/{s}.log", .{ rt.hub_home.artifacts_path, jid });
        const ca = rt.core_state.alloc();

        const argv_c = try ca.alloc([]const u8, req.argv.len);
        for (req.argv, 0..) |s, i| argv_c[i] = try ca.dupe(u8, s);
        const arts_c = try ca.alloc([]const u8, req.artifacts.len);
        for (req.artifacts, 0..) |s, i| arts_c[i] = try ca.dupe(u8, s);

        const target_copy: schema.JobTarget = switch (req.target) {
            .node => |n| .{ .node = .{ .node_id = try ca.dupe(u8, n.node_id) } },
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
            });
        }

        const tnode: ?[]const u8 = switch (req.target) {
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
        var list: std.ArrayList(schema.Job) = .empty;
        for (st.jobs) |j| {
            if (req.node_id) |want| {
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
        rt.refresh_quotas_sync();
        rt.meta_mu.lockUncancelable(rt.io);
        const qs = rt.cached_quotas;
        rt.meta_mu.unlock(rt.io);
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
        rt.commit(.{ .prompt_enqueued = .{
            .sleeve_id = req.agent orelse "envoy",
            .request_id = rid,
            .from = "owner",
            .kind = .prompt,
            .mode = .queue,
            .text = req.text,
        } }) catch |err| return map_commit_err(rt.gpa, err);
        return ok_json(rt.gpa, schema.Sent{ .request_id = rid, .queued = true, .queue_length = 1 });
    } else if (std.mem.eql(u8, method, "EnvoyWait")) {
        const req = schema.decode_arena(schema.EnvoyWaitRequest, a, body) catch schema.EnvoyWaitRequest{};
        return ok_json(rt.gpa, schema.EnvoyReply{
            .request_id = req.request_id orelse "",
            .settled = true,
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
            for (req.sleeve_ids) |sid| {
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
            const ws = try rt.stream_hub.wait_since(a, cursor, @min(500, deadline - now));
            cursor = ws.last_seq;
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
        const rep = try doctor.run_checks(a, rt.io, rt.env.get("HOME"));
        return ok_json(rt.gpa, rep);
    } else if (std.mem.eql(u8, method, "UpdateHarnesses")) {
        const req = schema.decode_arena(schema.UpdateHarnessesRequest, a, body) catch schema.UpdateHarnessesRequest{};
        var list: std.ArrayList(schema.HarnessUpdate) = .empty;
        for (req.programs) |prog| {
            try list.append(a, .{ .program = prog, .state = .skipped, .detail = "manual update required" });
        }
        return ok_json(rt.gpa, schema.HarnessUpdates{ .programs = list.items });
    } else if (std.mem.eql(u8, method, "UpdateHub")) {
        const req = schema.decode_arena(schema.UpdateHubRequest, a, body) catch schema.UpdateHubRequest{};
        if (req.rollback) {
            const target = u_store.rollback(a, rt.io, rt.hub_home.releases_path) catch {
                return err_json(rt.gpa, .not_found, "no previous release");
            };
            return ok_json(rt.gpa, schema.HubUpdate{ .target = target, .available = true });
        }
        const ch: u_ver.Channel = if (req.channel != null and req.channel.? == .dev) .dev else .stable;
        const latest = try u_store.check_latest(a, rt.io, rt.hub_home.config.update_repo, ch, @import("../version.zig").version);
        if (latest) |rel| {
            if (!req.check) {
                try u_store.stage_and_apply(rt.gpa, rt.io, rt.hub_home.releases_path, rel);
            }
            return ok_json(rt.gpa, schema.HubUpdate{
                .current = @import("../version.zig").version,
                .target = rel.tag,
                .available = true,
            });
        }
        return ok_json(rt.gpa, schema.HubUpdate{
            .current = @import("../version.zig").version,
            .target = @import("../version.zig").version,
            .available = false,
        });
    } else if (std.mem.eql(u8, method, "UpdateNodes")) {
        const req = schema.decode_arena(schema.UpdateNodesRequest, a, body) catch schema.UpdateNodesRequest{};
        return ok_json(rt.gpa, schema.NodeUpdates{
            .version = req.version orelse @import("../version.zig").version,
            .nodes = &.{},
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
