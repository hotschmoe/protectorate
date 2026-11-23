const std = @import("std");
const tiger = @import("../tiger.zig");
const version = @import("../version.zig");
const qr = @import("../qr.zig");
const schema = @import("../api/schema.zig");
const doctor = @import("../doctor/doctor.zig");
const client = @import("client.zig");
const mcp = @import("mcp.zig");

pub const ServeOpts = struct {
    home_opt: ?[]const u8 = null,
    listen_opt: ?[]const u8 = null,
    fleet_opt: ?[]const u8 = null,
    dev_token_opt: ?[]const u8 = null,
    no_sandbox: bool = false,
};

pub const TokenOpts = struct {
    home_opt: ?[]const u8 = null,
    dev_token_opt: ?[]const u8 = null,
};

pub const Action = union(enum) {
    exit: u8,
    serve: ServeOpts,
    token: TokenOpts,
};

pub fn dispatch_cli(init: std.process.Init, arena: std.mem.Allocator) !Action {
    const args = try init.minimal.args.toSlice(arena);

    var home_opt: ?[]const u8 = null;
    var listen_opt: ?[]const u8 = null;
    var fleet_opt: ?[]const u8 = null;
    var url_opt: ?[]const u8 = null;
    var token_opt: ?[]const u8 = null;
    var dev_token_opt: ?[]const u8 = null;
    var no_sandbox = false;
    var envoy_mode = false;
    var json_out = false;
    var pos: std.ArrayList([]const u8) = .empty;

    var i: usize = 1;
    while (i < args.len) : (i += 1) {
        const arg = args[i];
        if (std.mem.eql(u8, arg, "--")) {
            while (i < args.len) : (i += 1) {
                try pos.append(arena, args[i]);
            }
            break;
        } else if (std.mem.eql(u8, arg, "--version") or std.mem.eql(u8, arg, "-V")) {
            try print_out(init.io, "protectorate {s} ({s})\n", .{ version.version, version.git_commit });
            return .{ .exit = 0 };
        } else if (std.mem.eql(u8, arg, "--help") or std.mem.eql(u8, arg, "-h")) {
            try print_usage(init.io);
            return .{ .exit = 0 };
        } else if (std.mem.eql(u8, arg, "--envoy")) {
            envoy_mode = true;
        } else if (std.mem.eql(u8, arg, "--no-sandbox")) {
            no_sandbox = true;
        } else if (std.mem.eql(u8, arg, "--json")) {
            json_out = true;
        } else if (std.mem.eql(u8, arg, "--home") and i + 1 < args.len) {
            i += 1;
            home_opt = args[i];
        } else if (std.mem.eql(u8, arg, "--listen") and i + 1 < args.len) {
            i += 1;
            listen_opt = args[i];
        } else if (std.mem.eql(u8, arg, "--fleet-listen") and i + 1 < args.len) {
            i += 1;
            fleet_opt = args[i];
        } else if (std.mem.eql(u8, arg, "--url") and i + 1 < args.len) {
            i += 1;
            url_opt = args[i];
        } else if (std.mem.eql(u8, arg, "--token") and i + 1 < args.len) {
            i += 1;
            token_opt = args[i];
        } else if (std.mem.eql(u8, arg, "--dev-token") and i + 1 < args.len) {
            i += 1;
            dev_token_opt = args[i];
        } else {
            try pos.append(arena, arg);
        }
    }

    if (envoy_mode) {
        return .{ .exit = try mcp.run(init.gpa, init.io, init.environ_map, url_opt, token_opt, home_opt) };
    }
    if (pos.items.len == 0) {
        try print_usage(init.io);
        return .{ .exit = 0 };
    }

    const sub = pos.items[0];
    if (std.mem.eql(u8, sub, "mcp")) {
        return .{ .exit = try mcp.run(init.gpa, init.io, init.environ_map, url_opt, token_opt, home_opt) };
    }
    if (std.mem.eql(u8, sub, "serve")) {
        return .{ .serve = .{
            .home_opt = home_opt,
            .listen_opt = listen_opt,
            .fleet_opt = fleet_opt,
            .dev_token_opt = dev_token_opt,
            .no_sandbox = no_sandbox,
        } };
    }
    if (std.mem.eql(u8, sub, "token")) {
        return .{ .token = .{
            .home_opt = home_opt,
            .dev_token_opt = dev_token_opt,
        } };
    }
    if (std.mem.eql(u8, sub, "doctor")) {
        const rep = try doctor.run_checks(arena, init.io, init.environ_map.get("HOME"));
        if (json_out) {
            const j = try std.json.Stringify.valueAlloc(arena, rep, .{ .whitespace = .indent_2 });
            try print_out(init.io, "{s}\n", .{j});
        } else {
            try print_doctor(init.io, rep);
        }
        return .{ .exit = 0 };
    }

    var cl = try client.Client.resolve(init.gpa, init.io, init.environ_map, url_opt, token_opt, home_opt);
    defer cl.deinit();

    if (std.mem.eql(u8, sub, "status")) {
        return .{ .exit = try call_and_render(arena, init.io, &cl, "GetState", "{}", json_out, print_status) };
    }
    if (std.mem.eql(u8, sub, "pair")) {
        const name = if (pos.items.len > 1) pos.items[1] else "device";
        const req = try schema.encode_alloc(arena, schema.AddDeviceRequest{ .name = name });
        const res = cl.call_raw(arena, "AddDevice", req) catch |err| {
            try print_out(init.io, "error: {s}\n", .{@errorName(err)});
            return .{ .exit = 1 };
        };
        if (json_out or res.status >= 300) {
            try print_error_or_raw(arena, init.io, res.status, res.body, json_out);
            return .{ .exit = if (res.status < 300) 0 else 1 };
        }
        if (schema.decode_arena(schema.PairingCode, arena, res.body)) |pc| {
            try print_out(init.io, "device={s}  code={s}  expires_at_ms={d}\n", .{ pc.name, pc.code, pc.expires_at_ms.ms });
            if (qr.encode(pc.code, .medium)) |qcode| {
                const art = try qcode.to_terminal_alloc(arena, 2, false);
                try print_out(init.io, "{s}\n", .{art});
            } else |_| {}
        } else |_| {
            try print_out(init.io, "{s}\n", .{res.body});
        }
        return .{ .exit = 0 };
    }
    if (std.mem.eql(u8, sub, "rpc") and pos.items.len >= 2) {
        const method = pos.items[1];
        const body = if (pos.items.len >= 3) pos.items[2] else "{}";
        return .{ .exit = try call_and_print(arena, init.io, &cl, method, body) };
    }
    if (std.mem.eql(u8, sub, "sleeve") and pos.items.len >= 2) {
        const act = pos.items[1];
        if (std.mem.eql(u8, act, "list")) {
            return .{ .exit = try call_and_render(arena, init.io, &cl, "ListSleeves", "{}", json_out, print_sleeve_list) };
        }
        if (std.mem.eql(u8, act, "stop") and pos.items.len >= 3) {
            const req = try schema.encode_alloc(arena, schema.SleeveRef{ .sleeve_id = pos.items[2] });
            return .{ .exit = try call_and_print(arena, init.io, &cl, "StopSleeve", req) };
        }
        if (std.mem.eql(u8, act, "rm") and pos.items.len >= 3) {
            const req = try schema.encode_alloc(arena, schema.SleeveRef{ .sleeve_id = pos.items[2] });
            return .{ .exit = try call_and_print(arena, init.io, &cl, "RemoveSleeve", req) };
        }
        if (std.mem.eql(u8, act, "transcript") and pos.items.len >= 3) {
            const req = try schema.encode_alloc(arena, schema.GetTranscriptRequest{ .sleeve_id = pos.items[2] });
            return .{ .exit = try call_and_render(arena, init.io, &cl, "GetTranscript", req, json_out, print_transcript) };
        }
        if (std.mem.eql(u8, act, "prompt") and pos.items.len >= 4) {
            const req = try schema.encode_alloc(arena, schema.SendMessageRequest{
                .sleeve_id = pos.items[2],
                .text = pos.items[3],
            });
            return .{ .exit = try call_and_print(arena, init.io, &cl, "SendMessage", req) };
        }
        if (std.mem.eql(u8, act, "attach") and pos.items.len >= 3) {
            return .{ .exit = try cmd_sleeve_attach(arena, init, &cl, pos.items[2]) };
        }
    }
    if (std.mem.eql(u8, sub, "node") and pos.items.len >= 2) {
        const act = pos.items[1];
        if (std.mem.eql(u8, act, "list")) {
            return .{ .exit = try call_and_render(arena, init.io, &cl, "ListNodes", "{}", json_out, print_node_list) };
        }
        if (std.mem.eql(u8, act, "join-token")) {
            const res = try cl.call_raw(arena, "GetEnrollment", "{}");
            try print_out(init.io, "{s}\n", .{res.body});
            if (!json_out and res.status == 200) {
                if (schema.decode_arena(schema.Enrollment, arena, res.body)) |en| {
                    if (qr.encode(en.control_key, .medium)) |qcode| {
                        const art = try qcode.to_terminal_alloc(arena, 2, false);
                        try print_out(init.io, "{s}\n", .{art});
                    } else |_| {}
                } else |_| {}
            }
            return .{ .exit = if (res.status < 300) 0 else 1 };
        }
        if (std.mem.eql(u8, act, "add") and pos.items.len >= 3) {
            const req = try schema.encode_alloc(arena, schema.AddNodeRequest{ .registration = pos.items[2] });
            return .{ .exit = try call_and_print(arena, init.io, &cl, "AddNode", req) };
        }
        if (std.mem.eql(u8, act, "approve") and pos.items.len >= 3) {
            const req = try schema.encode_alloc(arena, schema.ApproveNodeRequest{
                .auth_id = pos.items[2],
                .name = if (pos.items.len >= 4) pos.items[3] else null,
            });
            return .{ .exit = try call_and_print(arena, init.io, &cl, "ApproveNode", req) };
        }
        if (std.mem.eql(u8, act, "update")) {
            return .{ .exit = try cmd_node_update(arena, init.io, &cl, pos.items[2..], json_out) };
        }
    }
    if (std.mem.eql(u8, sub, "hub") and pos.items.len >= 2) {
        if (std.mem.eql(u8, pos.items[1], "update")) {
            return .{ .exit = try cmd_hub_update(arena, init.io, &cl, pos.items[2..], json_out) };
        }
    }
    if (std.mem.eql(u8, sub, "harness") and pos.items.len >= 2) {
        if (std.mem.eql(u8, pos.items[1], "update")) {
            const req = try schema.encode_alloc(arena, schema.UpdateHarnessesRequest{ .programs = pos.items[2..] });
            return .{ .exit = try call_and_render(arena, init.io, &cl, "UpdateHarnesses", req, json_out, print_harness_updates) };
        }
    }
    if (std.mem.eql(u8, sub, "job") and pos.items.len >= 2) {
        const act = pos.items[1];
        if (std.mem.eql(u8, act, "list")) {
            return .{ .exit = try call_and_render(arena, init.io, &cl, "ListJobs", "{}", json_out, print_job_list) };
        }
        if (std.mem.eql(u8, act, "get") and pos.items.len >= 3) {
            const req = try schema.encode_alloc(arena, schema.JobRef{ .job_id = pos.items[2] });
            return .{ .exit = try call_and_render(arena, init.io, &cl, "GetJob", req, json_out, print_job_single) };
        }
        if (std.mem.eql(u8, act, "run")) {
            return .{ .exit = try cmd_job_run(arena, init.io, &cl, pos.items[2..], json_out) };
        }
        if (std.mem.eql(u8, act, "cancel") and pos.items.len >= 3) {
            const req = try schema.encode_alloc(arena, schema.JobRef{ .job_id = pos.items[2] });
            return .{ .exit = try call_and_print(arena, init.io, &cl, "CancelJob", req) };
        }
        if (std.mem.eql(u8, act, "logs") and pos.items.len >= 3) {
            const req = try schema.encode_alloc(arena, schema.GetJobLogRequest{ .job_id = pos.items[2] });
            return .{ .exit = try call_and_print(arena, init.io, &cl, "GetJobLog", req) };
        }
        if (std.mem.eql(u8, act, "artifact") and pos.items.len >= 4) {
            return .{ .exit = try cmd_job_artifact(arena, init.io, &cl, pos.items[2..], json_out) };
        }
    }
    if (std.mem.eql(u8, sub, "lease") and pos.items.len >= 2) {
        return .{ .exit = try cmd_lease(arena, init.io, &cl, pos.items[1..], json_out) };
    }
    if (std.mem.eql(u8, sub, "history") and pos.items.len >= 3 and std.mem.eql(u8, pos.items[1], "search")) {
        const req = try schema.encode_alloc(arena, schema.SearchHistoryRequest{ .query = pos.items[2] });
        return .{ .exit = try call_and_render(arena, init.io, &cl, "SearchHistory", req, json_out, print_history_hits) };
    }
    if (std.mem.eql(u8, sub, "quota")) {
        return .{ .exit = try call_and_render(arena, init.io, &cl, "ListQuotas", "{}", json_out, print_quota_list) };
    }

    try print_usage(init.io);
    return .{ .exit = 2 };
}

fn cmd_sleeve_attach(
    arena: std.mem.Allocator,
    init: std.process.Init,
    cl: *const client.Client,
    target_arg: []const u8,
) !u8 {
    const ls_res = cl.call_raw(arena, "ListSleeves", "{}") catch |err| {
        try print_out(init.io, "error: {s}\n", .{@errorName(err)});
        return 1;
    };
    if (ls_res.status >= 300) {
        try print_error_or_raw(arena, init.io, ls_res.status, ls_res.body, false);
        return 1;
    }
    const ls = schema.decode_arena(schema.SleeveList, arena, ls_res.body) catch {
        try print_out(init.io, "failed to parse sleeve list\n", .{});
        return 1;
    };

    var chosen: ?schema.Sleeve = null;
    for (ls.sleeves) |sv| {
        if (std.mem.eql(u8, sv.id, target_arg) or std.mem.eql(u8, sv.name, target_arg)) {
            chosen = sv;
            break;
        }
    }
    const sv = chosen orelse {
        try print_out(init.io, "sleeve not found: {s}\n", .{target_arg});
        return 1;
    };

    var harness: schema.Harness = if (sv.resleeve_to.len > 0) sv.resleeve_to[0] else .claude;
    var native_id: ?[]const u8 = null;
    var model: ?[]const u8 = null;
    var provider: ?[]const u8 = null;
    var effort: ?[]const u8 = null;
    if (sv.incarnation) |inc| {
        harness = inc.harness;
        native_id = inc.native_id;
        if (inc.model.len > 0) model = inc.model;
        if (inc.provider.len > 0) provider = inc.provider;
        effort = inc.effort;
    } else {
        const tx_req = try schema.encode_alloc(arena, schema.GetTranscriptRequest{ .sleeve_id = sv.id });
        if (cl.call_raw(arena, "GetTranscript", tx_req)) |tx_res| {
            if (tx_res.status < 300) {
                if (schema.decode_arena(schema.Transcript, arena, tx_res.body)) |tx| {
                    var idx = tx.entries.len;
                    while (idx > 0) {
                        idx -= 1;
                        switch (tx.entries[idx].entry) {
                            .session_info => |si| {
                                harness = si.harness;
                                native_id = si.native_id;
                                model = si.model;
                                provider = si.provider;
                                effort = si.effort;
                                break;
                            },
                            else => {},
                        }
                    }
                } else |_| {}
            }
        } else |_| {}
    }

    const to_req = try schema.encode_alloc(arena, schema.SleeveRef{ .sleeve_id = sv.id });
    const to_res = cl.call_raw(arena, "TakeoverTerminal", to_req) catch |err| {
        try print_out(init.io, "takeover failed: {s}\n", .{@errorName(err)});
        return 1;
    };
    if (to_res.status >= 300) {
        try print_error_or_raw(arena, init.io, to_res.status, to_res.body, false);
        return 1;
    }

    var argv: std.ArrayList([]const u8) = .empty;
    switch (harness) {
        .claude => {
            try argv.append(arena, "claude");
            if (native_id) |nid| try argv.appendSlice(arena, &.{ "--resume", nid });
            if (model) |m| try argv.appendSlice(arena, &.{ "--model", m });
            if (effort) |e| try argv.appendSlice(arena, &.{ "--effort", e });
        },
        .codex => {
            try argv.append(arena, "codex");
            if (native_id) |nid| try argv.appendSlice(arena, &.{ "resume", nid });
            if (model) |m| try argv.appendSlice(arena, &.{ "-m", m });
        },
        .grok => {
            try argv.append(arena, "grok");
            if (model) |m| try argv.appendSlice(arena, &.{ "--model", m });
        },
        .pi_vendor, .pi_local => {
            try argv.append(arena, "pi");
            if (native_id) |nid| try argv.appendSlice(arena, &.{ "--session", nid });
            if (provider) |p| try argv.appendSlice(arena, &.{ "--provider", p });
            if (model) |m| try argv.appendSlice(arena, &.{ "--model", m });
            if (effort) |e| try argv.appendSlice(arena, &.{ "--thinking", e });
        },
        .antigravity => {
            try argv.append(arena, "agy");
            if (native_id) |nid| try argv.appendSlice(arena, &.{ "--conversation", nid });
            if (model) |m| try argv.appendSlice(arena, &.{ "--model", m });
            if (effort) |e| try argv.appendSlice(arena, &.{ "--effort", e });
        },
        .unknown => {
            try argv.append(arena, "claude");
        },
    }

    try print_out(init.io, "attaching to {s} ({s}) via {s} in {s}\n", .{
        sv.id,
        sv.name,
        @tagName(harness),
        sv.path,
    });

    var child = std.process.spawn(init.io, .{
        .argv = argv.items,
        .cwd = .{ .path = sv.path },
        .environ_map = init.environ_map,
    }) catch |err| {
        try print_out(init.io, "failed to launch {s}: {s}\n", .{ argv.items[0], @errorName(err) });
        return 1;
    };
    const term = child.wait(init.io) catch return 1;
    return switch (term) {
        .exited => |c| c,
        else => 1,
    };
}

fn cmd_hub_update(
    arena: std.mem.Allocator,
    io: std.Io,
    cl: *const client.Client,
    rest: []const []const u8,
    json_out: bool,
) !u8 {
    var req = schema.UpdateHubRequest{};
    var idx: usize = 0;
    while (idx < rest.len) : (idx += 1) {
        const a = rest[idx];
        if (std.mem.eql(u8, a, "--check")) {
            req.check = true;
        } else if (std.mem.eql(u8, a, "--rollback")) {
            req.rollback = true;
        } else if (std.mem.eql(u8, a, "--channel") and idx + 1 < rest.len) {
            idx += 1;
            req.channel = if (std.mem.eql(u8, rest[idx], "dev")) .dev else .stable;
        } else if (std.mem.eql(u8, a, "--version") and idx + 1 < rest.len) {
            idx += 1;
            req.version = rest[idx];
        }
    }
    const body = try schema.encode_alloc(arena, req);
    return call_and_render(arena, io, cl, "UpdateHub", body, json_out, print_hub_update);
}

fn cmd_node_update(
    arena: std.mem.Allocator,
    io: std.Io,
    cl: *const client.Client,
    rest: []const []const u8,
    json_out: bool,
) !u8 {
    var req = schema.UpdateNodesRequest{};
    var node_list: std.ArrayList([]const u8) = .empty;
    var idx: usize = 0;
    while (idx < rest.len) : (idx += 1) {
        const a = rest[idx];
        if (std.mem.eql(u8, a, "--all")) {
            req.all = true;
        } else if (std.mem.eql(u8, a, "--no-wait")) {
            req.no_wait = true;
        } else if (std.mem.eql(u8, a, "--cancel") and idx + 1 < rest.len) {
            idx += 1;
            req.cancel = rest[idx];
        } else if (std.mem.eql(u8, a, "--version") and idx + 1 < rest.len) {
            idx += 1;
            req.version = rest[idx];
        } else if (std.mem.eql(u8, a, "--channel") and idx + 1 < rest.len) {
            idx += 1;
            req.channel = if (std.mem.eql(u8, rest[idx], "dev")) .dev else .stable;
        } else if (std.mem.eql(u8, a, "--probation") and idx + 1 < rest.len) {
            idx += 1;
            req.probation_secs = std.fmt.parseInt(u64, rest[idx], 10) catch null;
        } else {
            try node_list.append(arena, a);
        }
    }
    req.nodes = node_list.items;
    if (!req.all and req.cancel == null and req.nodes.len == 0) req.all = true;
    const body = try schema.encode_alloc(arena, req);
    return call_and_render(arena, io, cl, "UpdateNodes", body, json_out, print_node_updates);
}

fn cmd_job_run(
    arena: std.mem.Allocator,
    io: std.Io,
    cl: *const client.Client,
    rest: []const []const u8,
    json_out: bool,
) !u8 {
    var node_opt: ?[]const u8 = null;
    var arch_opt: ?[]const u8 = null;
    var cwd_opt: ?[]const u8 = null;
    var exclusive = false;
    var slots: u32 = 1;
    var long_slots: u32 = 0;
    var mem_bytes: u64 = 0;
    var parent_epoch: ?u64 = null;
    var ttl_ms: ?u64 = null;
    var wait_ms: ?u64 = null;
    var timeout_secs: ?u64 = null;
    var even_if_present = false;
    var on_present: schema.OnPresent = .idle;
    var maintenance = false;
    var wait_done = false;
    var arts: std.ArrayList([]const u8) = .empty;
    var argv: std.ArrayList([]const u8) = .empty;

    var idx: usize = 0;
    var in_argv = false;
    while (idx < rest.len) : (idx += 1) {
        const a = rest[idx];
        if (in_argv) {
            try argv.append(arena, a);
            continue;
        }
        if (std.mem.eql(u8, a, "--")) {
            in_argv = true;
        } else if (std.mem.eql(u8, a, "--node") and idx + 1 < rest.len) {
            idx += 1;
            node_opt = rest[idx];
        } else if (std.mem.eql(u8, a, "--arch") and idx + 1 < rest.len) {
            idx += 1;
            arch_opt = rest[idx];
        } else if (std.mem.eql(u8, a, "--cwd") and idx + 1 < rest.len) {
            idx += 1;
            cwd_opt = rest[idx];
        } else if (std.mem.eql(u8, a, "--artifact") and idx + 1 < rest.len) {
            idx += 1;
            try arts.append(arena, rest[idx]);
        } else if (std.mem.eql(u8, a, "--exclusive")) {
            exclusive = true;
        } else if (std.mem.eql(u8, a, "--slots") and idx + 1 < rest.len) {
            idx += 1;
            slots = std.fmt.parseInt(u32, rest[idx], 10) catch 1;
        } else if (std.mem.eql(u8, a, "--long-slots") and idx + 1 < rest.len) {
            idx += 1;
            long_slots = std.fmt.parseInt(u32, rest[idx], 10) catch 0;
        } else if (std.mem.eql(u8, a, "--mem-mib") and idx + 1 < rest.len) {
            idx += 1;
            const mib = std.fmt.parseInt(u64, rest[idx], 10) catch 0;
            mem_bytes = mib *| (1024 * 1024);
        } else if (std.mem.eql(u8, a, "--parent-epoch") and idx + 1 < rest.len) {
            idx += 1;
            parent_epoch = std.fmt.parseInt(u64, rest[idx], 10) catch null;
        } else if (std.mem.eql(u8, a, "--ttl-ms") and idx + 1 < rest.len) {
            idx += 1;
            ttl_ms = std.fmt.parseInt(u64, rest[idx], 10) catch null;
        } else if (std.mem.eql(u8, a, "--wait-ms") and idx + 1 < rest.len) {
            idx += 1;
            wait_ms = std.fmt.parseInt(u64, rest[idx], 10) catch null;
        } else if (std.mem.eql(u8, a, "--timeout") and idx + 1 < rest.len) {
            idx += 1;
            timeout_secs = std.fmt.parseInt(u64, rest[idx], 10) catch null;
        } else if (std.mem.eql(u8, a, "--even-if-present")) {
            even_if_present = true;
        } else if (std.mem.eql(u8, a, "--on-present") and idx + 1 < rest.len) {
            idx += 1;
            on_present = if (std.mem.eql(u8, rest[idx], "kill")) .kill else .idle;
        } else if (std.mem.eql(u8, a, "--maintenance")) {
            maintenance = true;
        } else if (std.mem.eql(u8, a, "--wait")) {
            wait_done = true;
        } else {
            in_argv = true;
            try argv.append(arena, a);
        }
    }
    if (argv.items.len == 0) {
        try print_out(io, "usage: protectorate job run [--node <id> | --arch <arch>] [--exclusive] [--slots <n>] [--long-slots <n>] [--mem-mib <mib>] [--parent-epoch <ep>] [--wait] -- <argv...>\n", .{});
        return 2;
    }

    const target: schema.JobTarget = if (node_opt) |n|
        .{ .node = .{ .node_id = n } }
    else
        .{ .arch = .{ .arch = arch_opt orelse "x86_64" } };

    const req = try schema.encode_alloc(arena, schema.RunJobRequest{
        .target = target,
        .argv = argv.items,
        .cwd = cwd_opt,
        .artifacts = arts.items,
        .even_if_present = even_if_present,
        .on_present = on_present,
        .maintenance = maintenance,
        .timeout_secs = timeout_secs,
        .exclusive = exclusive,
        .slots = slots,
        .long_slots = long_slots,
        .mem_bytes = mem_bytes,
        .wait_ms = wait_ms,
        .lease_ttl_ms = ttl_ms,
        .parent_epoch = parent_epoch,
    });

    const res = cl.call_raw(arena, "RunJob", req) catch |err| {
        try print_out(io, "error: {s}\n", .{@errorName(err)});
        return 1;
    };
    if (res.status >= 300) {
        if (json_out) {
            try print_out(io, "{s}\n", .{res.body});
        } else {
            try print_error_or_raw(arena, io, res.status, res.body, false);
        }
        return 1;
    }
    if (!wait_done) {
        if (json_out) {
            try print_out(io, "{s}\n", .{res.body});
        } else {
            try print_job_single(arena, io, res.body);
        }
        return 0;
    }

    var job = try schema.decode_arena(schema.Job, arena, res.body);
    var latest_body: []const u8 = res.body;
    const poll_req = try schema.encode_alloc(arena, schema.JobRef{ .job_id = job.id });
    while (true) {
        const done = switch (job.state) {
            .succeeded, .failed, .interrupted, .lost, .cancelled, .abandoned => true,
            else => false,
        };
        if (done) break;
        io.sleep(.fromMilliseconds(250), .awake) catch break;
        const gres = cl.call_raw(arena, "GetJob", poll_req) catch continue;
        if (gres.status >= 300) continue;
        latest_body = gres.body;
        job = schema.decode_arena(schema.Job, arena, gres.body) catch continue;
    }

    if (json_out) {
        try print_out(io, "{s}\n", .{latest_body});
    } else {
        try print_job_single(arena, io, latest_body);
    }
    return if (job.state == .succeeded) 0 else 1;
}

fn cmd_job_artifact(
    arena: std.mem.Allocator,
    io: std.Io,
    cl: *const client.Client,
    rest: []const []const u8,
    json_out: bool,
) !u8 {
    if (rest.len < 2) return 2;
    const job_id = rest[0];
    const rel_path = rest[1];
    var out_file: ?[]const u8 = null;
    var idx: usize = 2;
    while (idx < rest.len) : (idx += 1) {
        if (std.mem.eql(u8, rest[idx], "--out") and idx + 1 < rest.len) {
            idx += 1;
            out_file = rest[idx];
        }
    }

    if (json_out and out_file == null) {
        const req = try schema.encode_alloc(arena, schema.GetJobArtifactRequest{ .job_id = job_id, .path = rel_path });
        return call_and_print(arena, io, cl, "GetJobArtifact", req);
    }

    var all: std.ArrayList(u8) = .empty;
    var off: u64 = 0;
    while (true) {
        const req = try schema.encode_alloc(arena, schema.GetJobArtifactRequest{
            .job_id = job_id,
            .path = rel_path,
            .offset = off,
            .limit = 256 * 1024,
        });
        const res = cl.call_raw(arena, "GetJobArtifact", req) catch |err| {
            try print_out(io, "error: {s}\n", .{@errorName(err)});
            return 1;
        };
        if (res.status >= 300) {
            try print_error_or_raw(arena, io, res.status, res.body, false);
            return 1;
        }
        const chunk = try schema.decode_arena(schema.JobArtifactData, arena, res.body);
        if (chunk.hex.len > 0) {
            const raw = try arena.alloc(u8, chunk.hex.len / 2);
            _ = try std.fmt.hexToBytes(raw, chunk.hex);
            try all.appendSlice(arena, raw);
        }
        off = chunk.next_offset;
        if (chunk.eof or chunk.next_offset >= chunk.size_bytes) break;
    }

    if (out_file) |fp| {
        try std.Io.Dir.cwd().writeFile(io, .{ .sub_path = fp, .data = all.items });
        try print_out(io, "wrote {d} bytes to {s}\n", .{ all.items.len, fp });
    } else {
        var buf: [4096]u8 = undefined;
        var w = std.Io.File.stdout().writer(io, &buf);
        try w.interface.writeAll(all.items);
        try w.interface.flush();
    }
    return 0;
}

fn cmd_lease(
    arena: std.mem.Allocator,
    io: std.Io,
    cl: *const client.Client,
    rest: []const []const u8,
    json_out: bool,
) !u8 {
    if (rest.len < 2) {
        try print_usage(io);
        return 2;
    }
    const act = rest[0];
    const node_id = rest[1];
    if (std.mem.eql(u8, act, "list")) {
        const req = try schema.encode_alloc(arena, schema.NodeRef{ .node_id = node_id });
        return call_and_render(arena, io, cl, "GetNodeAdmission", req, json_out, print_node_admission);
    }
    if (std.mem.eql(u8, act, "unquarantine")) {
        const req = try schema.encode_alloc(arena, schema.NodeRef{ .node_id = node_id });
        return call_and_print(arena, io, cl, "ClearNodeQuarantine", req);
    }
    if (std.mem.eql(u8, act, "acquire")) {
        var holder: []const u8 = "owner";
        var exclusive = false;
        var slots: u32 = 1;
        var long_slots: u32 = 0;
        var mem_bytes: u64 = 0;
        var ttl_ms: ?u64 = null;
        var wait_ms: ?u64 = null;
        var parent_epoch: ?u64 = null;
        var idx: usize = 2;
        while (idx < rest.len) : (idx += 1) {
            const a = rest[idx];
            if (std.mem.eql(u8, a, "--holder") and idx + 1 < rest.len) {
                idx += 1;
                holder = rest[idx];
            } else if (std.mem.eql(u8, a, "--exclusive")) {
                exclusive = true;
            } else if (std.mem.eql(u8, a, "--slots") and idx + 1 < rest.len) {
                idx += 1;
                slots = std.fmt.parseInt(u32, rest[idx], 10) catch 1;
            } else if (std.mem.eql(u8, a, "--long-slots") and idx + 1 < rest.len) {
                idx += 1;
                long_slots = std.fmt.parseInt(u32, rest[idx], 10) catch 0;
            } else if (std.mem.eql(u8, a, "--mem-mib") and idx + 1 < rest.len) {
                idx += 1;
                mem_bytes = (std.fmt.parseInt(u64, rest[idx], 10) catch 0) *| (1024 * 1024);
            } else if (std.mem.eql(u8, a, "--ttl-ms") and idx + 1 < rest.len) {
                idx += 1;
                ttl_ms = std.fmt.parseInt(u64, rest[idx], 10) catch null;
            } else if (std.mem.eql(u8, a, "--wait-ms") and idx + 1 < rest.len) {
                idx += 1;
                wait_ms = std.fmt.parseInt(u64, rest[idx], 10) catch null;
            } else if (std.mem.eql(u8, a, "--parent-epoch") and idx + 1 < rest.len) {
                idx += 1;
                parent_epoch = std.fmt.parseInt(u64, rest[idx], 10) catch null;
            }
        }
        const req = try schema.encode_alloc(arena, schema.AcquireLeaseRequest{
            .node_id = node_id,
            .holder_id = holder,
            .exclusive = exclusive,
            .slots = slots,
            .long_slots = long_slots,
            .mem_bytes = mem_bytes,
            .ttl_ms = ttl_ms,
            .wait_ms = wait_ms,
            .parent_epoch = parent_epoch,
        });
        return call_and_print(arena, io, cl, "AcquireLease", req);
    }
    if (std.mem.eql(u8, act, "renew") and rest.len >= 4) {
        const ep = std.fmt.parseInt(u64, rest[3], 10) catch return 2;
        const req = try schema.encode_alloc(arena, schema.RenewLeaseRequest{
            .node_id = node_id,
            .lease_id = rest[2],
            .epoch = ep,
        });
        return call_and_print(arena, io, cl, "RenewLease", req);
    }
    if (std.mem.eql(u8, act, "release") and rest.len >= 4) {
        const ep = std.fmt.parseInt(u64, rest[3], 10) catch return 2;
        const req = try schema.encode_alloc(arena, schema.ReleaseLeaseRequest{
            .node_id = node_id,
            .lease_id = rest[2],
            .epoch = ep,
        });
        return call_and_print(arena, io, cl, "ReleaseLease", req);
    }
    if (std.mem.eql(u8, act, "verify") and rest.len >= 4) {
        const ep = std.fmt.parseInt(u64, rest[3], 10) catch return 2;
        const req = try schema.encode_alloc(arena, schema.VerifyEpochRequest{
            .node_id = node_id,
            .boot_id = rest[2],
            .epoch = ep,
        });
        return call_and_print(arena, io, cl, "VerifyEpoch", req);
    }
    try print_usage(io);
    return 2;
}

fn call_and_print(
    arena: std.mem.Allocator,
    io: std.Io,
    cl: *const client.Client,
    method: []const u8,
    req_json: []const u8,
) !u8 {
    const res = cl.call_raw(arena, method, req_json) catch |err| {
        try print_out(io, "error: {s}\n", .{@errorName(err)});
        return 1;
    };
    try print_out(io, "{s}\n", .{res.body});
    return if (res.status < 300) 0 else 1;
}

fn call_and_render(
    arena: std.mem.Allocator,
    io: std.Io,
    cl: *const client.Client,
    method: []const u8,
    req_json: []const u8,
    json_out: bool,
    comptime formatter: anytype,
) !u8 {
    const res = cl.call_raw(arena, method, req_json) catch |err| {
        try print_out(io, "error: {s}\n", .{@errorName(err)});
        return 1;
    };
    if (json_out) {
        try print_out(io, "{s}\n", .{res.body});
        return if (res.status < 300) 0 else 1;
    }
    if (res.status >= 300) {
        try print_error_or_raw(arena, io, res.status, res.body, false);
        return 1;
    }
    formatter(arena, io, res.body) catch {
        try print_out(io, "{s}\n", .{res.body});
    };
    return 0;
}

fn print_error_or_raw(
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

fn print_doctor(io: std.Io, rep: schema.DoctorReport) !void {
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

fn print_status(arena: std.mem.Allocator, io: std.Io, body: []const u8) !void {
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
            try print_out(io, "\n{s:<18} {s:<8} {s:<14} {s}\n", .{ "APPROVAL", "KIND", "SLEEVE", "TITLE" });
            for (st.approvals) |ap| {
                if (ap.state != .pending) continue;
                try print_out(io, "{s:<18} {s:<8} {s:<14} {s}\n", .{ ap.id, @tagName(ap.kind), ap.sleeve_id, ap.title });
            }
        }
    }
    if (st.jobs.len > 0) {
        try print_out(io, "\n", .{});
        render_jobs(io, st.jobs) catch {};
    }
}

fn print_sleeve_list(arena: std.mem.Allocator, io: std.Io, body: []const u8) !void {
    const ls = try schema.decode_arena(schema.SleeveList, arena, body);
    if (ls.sleeves.len == 0) {
        try print_out(io, "no sleeves\n", .{});
        return;
    }
    try render_sleeves(io, ls.sleeves);
}

fn render_sleeves(io: std.Io, sleeves: []const schema.Sleeve) !void {
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

fn print_transcript(arena: std.mem.Allocator, io: std.Io, body: []const u8) !void {
    const tx = try schema.decode_arena(schema.Transcript, arena, body);
    if (tx.entries.len == 0) {
        try print_out(io, "no transcript entries for {s}\n", .{tx.sleeve_id});
        return;
    }
    for (tx.entries) |ce| {
        switch (ce.entry) {
            .session_info => |s| try print_out(io, "[#{d} g{d}.t{d}] session harness={s} id={s}\n", .{ ce.seq, ce.g, ce.turn, @tagName(s.harness), s.native_id }),
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

fn print_node_list(arena: std.mem.Allocator, io: std.Io, body: []const u8) !void {
    const nl = try schema.decode_arena(schema.NodeList, arena, body);
    if (nl.nodes.len == 0) {
        try print_out(io, "no nodes\n", .{});
        return;
    }
    try render_nodes(io, nl.nodes);
}

fn render_nodes(io: std.Io, nodes: []const schema.Node) !void {
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

fn print_job_list(arena: std.mem.Allocator, io: std.Io, body: []const u8) !void {
    const jl = try schema.decode_arena(schema.JobList, arena, body);
    if (jl.jobs.len == 0) {
        try print_out(io, "no jobs\n", .{});
        return;
    }
    try render_jobs(io, jl.jobs);
}

fn print_job_single(arena: std.mem.Allocator, io: std.Io, body: []const u8) !void {
    const j = try schema.decode_arena(schema.Job, arena, body);
    try render_jobs(io, &.{j});
}

fn print_node_admission(arena: std.mem.Allocator, io: std.Io, body: []const u8) !void {
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

fn render_jobs(io: std.Io, jobs: []const schema.Job) !void {
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

fn print_quota_list(arena: std.mem.Allocator, io: std.Io, body: []const u8) !void {
    const ql = try schema.decode_arena(schema.QuotaList, arena, body);
    if (ql.quotas.len == 0) {
        try print_out(io, "no quotas\n", .{});
        return;
    }
    try print_out(io, "{s:<12} {s:<10} {s:<14} {s}\n", .{ "SUB", "READING", "PLAN", "DETAIL" });
    for (ql.quotas) |q| {
        try print_out(io, "{s:<12} {s:<10} {s:<14}", .{
            @tagName(q.subscription),
            @tagName(q.reading),
            if (q.plan.len > 0) q.plan else "-",
        });
        for (q.windows) |w| {
            if (w.used_pct) |pct| {
                try print_out(io, "{s}={d:.0}% ", .{ w.label, pct });
            }
        }
        if (q.credits) |cr| {
            if (cr.balance) |bal| try print_out(io, "balance={d:.2} ", .{bal});
        }
        if (q.note.len > 0) try print_out(io, "{s}", .{q.note});
        try print_out(io, "\n", .{});
    }
}

fn print_history_hits(arena: std.mem.Allocator, io: std.Io, body: []const u8) !void {
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

fn print_hub_update(arena: std.mem.Allocator, io: std.Io, body: []const u8) !void {
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

fn print_node_updates(arena: std.mem.Allocator, io: std.Io, body: []const u8) !void {
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

fn print_harness_updates(arena: std.mem.Allocator, io: std.Io, body: []const u8) !void {
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

pub fn print_out(io: std.Io, comptime fmt: []const u8, args: anytype) !void {
    var buf: [4096]u8 = undefined;
    var w = std.Io.File.stdout().writer(io, &buf);
    try w.interface.print(fmt, args);
    try w.interface.flush();
}

fn print_usage(io: std.Io) !void {
    try print_out(
        io,
        \\protectorate - AI coding agent orchestrator & Noise_IK fleet hub
        \\
        \\usage:
        \\  protectorate serve [--home <dir>] [--listen <addr:port>] [--fleet-listen <addr:port>] [--no-sandbox] [--dev-token <tok>]
        \\  protectorate [--json] status | doctor | pair [<name>] | token | quota
        \\  protectorate [--json] sleeve (list | attach <id> | stop <id> | rm <id> | prompt <id> <text> | transcript <id>)
        \\  protectorate [--json] node (list | join-token | add <reg> | approve <id> [<name>] | update [--all | <id>...] [--version <v>] [--channel stable|dev] [--no-wait] [--cancel <rid>])
        \\  protectorate [--json] hub update [--check] [--rollback] [--channel stable|dev] [--version <v>]
        \\  protectorate [--json] harness update [<program>...]
        \\  protectorate [--json] job (list | get <id> | run [opts] -- <argv...> | cancel <id> | logs <id> | artifact <id> <path> [--out <file>])
        \\  protectorate [--json] lease (list <node> | acquire <node> [opts] | renew <node> <lid> <ep> | release <node> <lid> <ep> | verify <node> <boot> <ep> | unquarantine <node>)
        \\  protectorate [--json] history search <query>
        \\  protectorate rpc <Method> [<json>]
        \\  protectorate --envoy | mcp
        \\
    ,
        .{},
    );
}
