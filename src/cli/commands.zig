const std = @import("std");
const tiger = @import("../tiger.zig");
const version = @import("../version.zig");
const qr = @import("../qr.zig");
const schema = @import("../api/schema.zig");
const doctor = @import("../doctor/doctor.zig");
const u_store = @import("../update/store.zig");
const client = @import("client.zig");
const mcp = @import("mcp.zig");
const render = @import("render.zig");

pub const print_out = render.print_out;

pub const ServeOpts = struct {
    home_opt: ?[]const u8 = null,
    listen_opt: ?[]const u8 = null,
    proxy_listen_opt: ?[]const u8 = null,
    trusted_proxies: []const []const u8 = &.{},
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
    var proxy_listen_opt: ?[]const u8 = null;
    var trusted_proxies: std.ArrayList([]const u8) = .empty;
    var fleet_opt: ?[]const u8 = null;
    var url_opt: ?[]const u8 = null;
    var token_opt: ?[]const u8 = null;
    var token_file_opt: ?[]const u8 = null;
    var as_name_opt: ?[]const u8 = null;
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
        } else if (pos.items.len == 0 and (std.mem.eql(u8, arg, "--version") or std.mem.eql(u8, arg, "-V"))) {
            try render.print_out(init.io, "protectorate {s} ({s})\n", .{ version.version, version.git_commit });
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
        } else if (std.mem.eql(u8, arg, "--proxy-listen") and i + 1 < args.len) {
            i += 1;
            proxy_listen_opt = args[i];
        } else if (std.mem.eql(u8, arg, "--trusted-proxy") and i + 1 < args.len) {
            i += 1;
            try trusted_proxies.append(arena, args[i]);
        } else if (std.mem.eql(u8, arg, "--fleet-listen") and i + 1 < args.len) {
            i += 1;
            fleet_opt = args[i];
        } else if (std.mem.eql(u8, arg, "--url") and i + 1 < args.len) {
            i += 1;
            url_opt = args[i];
        } else if (std.mem.eql(u8, arg, "--token") and i + 1 < args.len) {
            i += 1;
            token_opt = args[i];
        } else if (std.mem.eql(u8, arg, "--token-file") and i + 1 < args.len) {
            i += 1;
            token_file_opt = args[i];
        } else if (std.mem.eql(u8, arg, "--as") and i + 1 < args.len) {
            i += 1;
            as_name_opt = args[i];
        } else if (std.mem.eql(u8, arg, "--dev-token") and i + 1 < args.len) {
            i += 1;
            dev_token_opt = args[i];
        } else {
            try pos.append(arena, arg);
        }
    }

    if (envoy_mode and pos.items.len == 0) {
        return .{ .exit = try mcp.run(init.gpa, init.io, init.environ_map, .{
            .url = url_opt,
            .token = token_opt,
            .token_file = token_file_opt,
            .home = home_opt,
            .as_name = as_name_opt,
            .envoy_mode = true,
        }) };
    }
    if (pos.items.len == 0) {
        try print_usage(init.io);
        return .{ .exit = 0 };
    }

    const sub = pos.items[0];
    if (std.mem.eql(u8, sub, "version")) {
        try render.print_out(init.io, "protectorate {s} ({s})\n", .{ version.version, version.git_commit });
        return .{ .exit = 0 };
    }
    if (std.mem.eql(u8, sub, "mcp")) {
        return .{ .exit = try mcp.run(init.gpa, init.io, init.environ_map, .{
            .url = url_opt,
            .token = token_opt,
            .token_file = token_file_opt,
            .home = home_opt,
            .as_name = as_name_opt,
            .envoy_mode = envoy_mode,
        }) };
    }
    if (std.mem.eql(u8, sub, "serve")) {
        return .{ .serve = .{
            .home_opt = home_opt,
            .listen_opt = listen_opt,
            .proxy_listen_opt = proxy_listen_opt,
            .trusted_proxies = trusted_proxies.items,
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

    var cl = try client.Client.resolve_opts(init.gpa, init.io, init.environ_map, .{
        .url = url_opt,
        .token = token_opt,
        .token_file = token_file_opt,
        .home = home_opt,
    });
    defer cl.deinit();

    if (std.mem.eql(u8, sub, "doctor")) {
        return .{ .exit = try cmd_doctor(arena, init, &cl, pos.items[1..], json_out) };
    }
    if (std.mem.eql(u8, sub, "status")) {
        return .{ .exit = try call_and_render(arena, init.io, &cl, "GetState", "{}", json_out, render.print_status) };
    }
    if (std.mem.eql(u8, sub, "pair")) {
        const name = if (pos.items.len > 1) pos.items[1] else "device";
        return .{ .exit = try cmd_device_add(arena, init.io, &cl, name, json_out) };
    }
    if (std.mem.eql(u8, sub, "rpc") and pos.items.len >= 2) {
        const method = pos.items[1];
        const body = if (pos.items.len >= 3) pos.items[2] else "{}";
        return .{ .exit = try call_and_print(arena, init.io, &cl, method, body) };
    }
    if (std.mem.eql(u8, sub, "hub") and pos.items.len >= 2) {
        return .{ .exit = try cmd_hub(arena, init, &cl, home_opt, pos.items[1..], json_out) };
    }
    if (std.mem.eql(u8, sub, "harness") and pos.items.len >= 2) {
        return .{ .exit = try cmd_harness(arena, init, &cl, pos.items[1..], json_out) };
    }
    if (std.mem.eql(u8, sub, "sleeve") and pos.items.len >= 2) {
        return .{ .exit = try cmd_sleeve(arena, init, &cl, pos.items[1..], json_out) };
    }
    if (std.mem.eql(u8, sub, "node") and pos.items.len >= 2) {
        return .{ .exit = try cmd_node(arena, init.io, &cl, pos.items[1..], json_out) };
    }
    if (std.mem.eql(u8, sub, "job") and pos.items.len >= 2) {
        return .{ .exit = try cmd_job(arena, init.io, &cl, pos.items[1..], json_out) };
    }
    if (std.mem.eql(u8, sub, "lease") and pos.items.len >= 2) {
        return .{ .exit = try cmd_lease(arena, init.io, &cl, pos.items[1..], json_out) };
    }
    if (std.mem.eql(u8, sub, "approval") and pos.items.len >= 2) {
        return .{ .exit = try cmd_approval(arena, init.io, &cl, pos.items[1..], json_out) };
    }
    if (std.mem.eql(u8, sub, "event") and pos.items.len >= 2) {
        return .{ .exit = try cmd_event(arena, init.io, &cl, pos.items[1..], json_out) };
    }
    if (std.mem.eql(u8, sub, "envoy") and pos.items.len >= 2) {
        return .{ .exit = try cmd_envoy(arena, init.io, &cl, as_name_opt, pos.items[1..], json_out) };
    }
    if (std.mem.eql(u8, sub, "history") and pos.items.len >= 2) {
        return .{ .exit = try cmd_history(arena, init.io, &cl, pos.items[1..], json_out) };
    }
    if (std.mem.eql(u8, sub, "device") and pos.items.len >= 2) {
        return .{ .exit = try cmd_device(arena, init.io, &cl, pos.items[1..], json_out) };
    }
    if (std.mem.eql(u8, sub, "quota")) {
        return .{ .exit = try call_and_render(arena, init.io, &cl, "ListQuotas", "{}", json_out, render.print_quota_list) };
    }

    try print_usage(init.io);
    return .{ .exit = 2 };
}

fn cmd_doctor(
    arena: std.mem.Allocator,
    init: std.process.Init,
    cl: *const client.Client,
    rest: []const []const u8,
    json_out: bool,
) !u8 {
    var refresh = false;
    for (rest) |a| {
        if (std.mem.eql(u8, a, "--refresh")) refresh = true;
    }
    const req = try schema.encode_alloc(arena, schema.GetDoctorRequest{ .refresh = refresh });
    if (cl.call_raw(arena, "GetDoctor", req)) |res| {
        if (res.status < 300) {
            if (json_out) {
                try render.print_out(init.io, "{s}\n", .{res.body});
            } else {
                render.print_doctor_json(arena, init.io, res.body) catch {
                    try render.print_out(init.io, "{s}\n", .{res.body});
                };
            }
            return 0;
        }
    } else |_| {}

    const rep = try doctor.run_checks(arena, init.io, init.environ_map.get("HOME"));
    if (json_out) {
        const j = try std.json.Stringify.valueAlloc(arena, rep, .{ .whitespace = .indent_2 });
        try render.print_out(init.io, "{s}\n", .{j});
    } else {
        try render.print_doctor(init.io, rep);
    }
    return 0;
}

fn cmd_hub(
    arena: std.mem.Allocator,
    init: std.process.Init,
    cl: *const client.Client,
    home_opt: ?[]const u8,
    rest: []const []const u8,
    json_out: bool,
) !u8 {
    const act = rest[0];
    if (std.mem.eql(u8, act, "info")) {
        return call_and_render(arena, init.io, cl, "GetHubInfo", "{}", json_out, render.print_hub_info);
    }
    if (std.mem.eql(u8, act, "state")) {
        return call_and_render(arena, init.io, cl, "GetState", "{}", json_out, render.print_status);
    }
    if (std.mem.eql(u8, act, "update")) {
        return cmd_hub_update(arena, init.io, cl, rest[1..], json_out);
    }
    if (std.mem.eql(u8, act, "probation")) {
        var retry_rollback = false;
        for (rest[1..]) |a| {
            if (std.mem.eql(u8, a, "--retry-rollback")) retry_rollback = true;
        }
        const hp = try resolve_hub_home(arena, init.environ_map, home_opt);
        const rel_dir = try std.fs.path.join(arena, &.{ hp, "releases" });
        const out = try u_store.check_probation(init.gpa, arena, init.io, rel_dir, retry_rollback);
        switch (out) {
            .none => {
                try render.print_out(init.io, "{s}\n", .{if (json_out) "{\"status\":\"ok\",\"probation\":false}" else "no probation marker present (ok)"});
                return 0;
            },
            .invalid => {
                try render.print_out(init.io, "{s}\n", .{if (json_out) "{\"status\":\"error\",\"reason\":\"unparseable probation marker\"}" else "unparseable probation marker"});
                return 1;
            },
            .healthy => |h| {
                const text = if (json_out)
                    try std.json.Stringify.valueAlloc(arena, .{ .active = h.active, .healthy = true }, .{})
                else
                    try std.fmt.allocPrint(arena, "active={s} healthy=true", .{h.active});
                try render.print_out(init.io, "{s}\n", .{text});
                return 0;
            },
            .launch_pending => |lp| {
                const text = if (json_out)
                    try std.json.Stringify.valueAlloc(arena, .{
                        .active = lp.active,
                        .healthy = false,
                        .probation = "pending",
                        .attempts = lp.attempts,
                    }, .{})
                else
                    try std.fmt.allocPrint(arena, "active={s} healthy=false probation=pending (attempt {d})", .{ lp.active, lp.attempts });
                try render.print_out(init.io, "{s}\n", .{text});
                return 0;
            },
            .rolled_back => |rb| {
                const text = if (json_out)
                    try std.json.Stringify.valueAlloc(arena, .{ .rolled_back = true, .from = rb.from, .to = rb.to }, .{})
                else
                    try std.fmt.allocPrint(arena, "rolled back from {s} to {s} (probation failed)", .{ rb.from, rb.to });
                try render.print_out(init.io, "{s}\n", .{text});
                return 0;
            },
            .unhealthy_no_previous => |np| {
                const text = if (json_out)
                    try std.json.Stringify.valueAlloc(arena, .{ .active = np.active, .healthy = false, .rolled_back = false }, .{})
                else
                    try std.fmt.allocPrint(arena, "active={s} healthy=false (no previous release for rollback)", .{np.active});
                try render.print_out(init.io, "{s}\n", .{text});
                return 0;
            },
        }
    }
    try print_usage(init.io);
    return 2;
}

fn cmd_harness(
    arena: std.mem.Allocator,
    init: std.process.Init,
    cl: *const client.Client,
    rest: []const []const u8,
    json_out: bool,
) !u8 {
    const act = rest[0];
    if (std.mem.eql(u8, act, "update")) {
        const req = try schema.encode_alloc(arena, schema.UpdateHarnessesRequest{ .programs = rest[1..] });
        return call_and_render(arena, init.io, cl, "UpdateHarnesses", req, json_out, render.print_harness_updates);
    }
    if (std.mem.eql(u8, act, "login") and rest.len >= 2) {
        const prog = rest[1];
        const h = parse_harness(prog) orelse {
            try render.print_out(init.io, "unknown harness '{s}' (use claude|codex|grok|pi|agy)\n", .{prog});
            return 2;
        };
        var argv: std.ArrayList([]const u8) = .empty;
        switch (h) {
            .claude => try argv.append(arena, "claude"),
            .codex => try argv.appendSlice(arena, &.{ "codex", "login" }),
            .grok => try argv.append(arena, "grok"),
            .pi_vendor, .pi_local => try argv.append(arena, "pi"),
            .antigravity => try argv.append(arena, "agy"),
            .unknown => return 2,
        }
        var idx: usize = 2;
        if (idx < rest.len and std.mem.eql(u8, rest[idx], "--")) idx += 1;
        while (idx < rest.len) : (idx += 1) {
            try argv.append(arena, rest[idx]);
        }
        var child = std.process.spawn(init.io, .{
            .argv = argv.items,
            .environ_map = init.environ_map,
        }) catch |err| {
            try render.print_out(init.io, "failed to launch {s}: {s}\n", .{ argv.items[0], @errorName(err) });
            return 1;
        };
        const term = child.wait(init.io) catch return 1;
        return switch (term) {
            .exited => |c| @intCast(c),
            else => 1,
        };
    }
    try print_usage(init.io);
    return 2;
}

fn cmd_sleeve(
    arena: std.mem.Allocator,
    init: std.process.Init,
    cl: *const client.Client,
    rest: []const []const u8,
    json_out: bool,
) !u8 {
    const act = rest[0];
    if (std.mem.eql(u8, act, "list")) {
        return call_and_render(arena, init.io, cl, "ListSleeves", "{}", json_out, render.print_sleeve_list);
    }
    if (std.mem.eql(u8, act, "show") and rest.len >= 2) {
        const res = cl.call_raw(arena, "ListSleeves", "{}") catch |err| {
            try render.print_out(init.io, "error: {s}\n", .{@errorName(err)});
            return 1;
        };
        if (res.status >= 300) {
            try render.print_error_or_raw(arena, init.io, res.status, res.body, json_out);
            return 1;
        }
        const ls = try schema.decode_arena(schema.SleeveList, arena, res.body);
        for (ls.sleeves) |sv| {
            if (std.mem.eql(u8, sv.id, rest[1]) or std.mem.eql(u8, sv.name, rest[1])) {
                if (json_out) {
                    const j = try schema.encode_alloc(arena, sv);
                    try render.print_out(init.io, "{s}\n", .{j});
                } else {
                    try render.print_sleeve_detail(init.io, sv);
                }
                return 0;
            }
        }
        try render.print_out(init.io, "sleeve not found: {s}\n", .{rest[1]});
        return 1;
    }
    if (std.mem.eql(u8, act, "spawn") and rest.len >= 2) {
        return cmd_sleeve_spawn(arena, init.io, cl, rest[1..], json_out);
    }
    if ((std.mem.eql(u8, act, "send") or std.mem.eql(u8, act, "prompt")) and rest.len >= 3) {
        return cmd_sleeve_send(arena, init.io, cl, rest[1..]);
    }
    if (std.mem.eql(u8, act, "inbox")) {
        var target: ?[]const u8 = null;
        var drop = false;
        for (rest[1..]) |a| {
            if (std.mem.eql(u8, a, "--drop")) {
                drop = true;
            } else if (target == null) {
                target = a;
            }
        }
        const sid = target orelse "envoy";
        if (drop) {
            const req = try schema.encode_alloc(arena, schema.SleeveRef{ .sleeve_id = sid });
            return call_and_print(arena, init.io, cl, "DropQueued", req);
        }
        const req = try schema.encode_alloc(arena, schema.InboxRequest{ .sleeve_id = sid });
        return call_and_render(arena, init.io, cl, "GetInbox", req, json_out, render.print_inbox);
    }
    if (std.mem.eql(u8, act, "interrupt") and rest.len >= 2) {
        const req = try schema.encode_alloc(arena, schema.SleeveRef{ .sleeve_id = rest[1] });
        return call_and_print(arena, init.io, cl, "Interrupt", req);
    }
    if (std.mem.eql(u8, act, "stop") and rest.len >= 2) {
        const req = try schema.encode_alloc(arena, schema.SleeveRef{ .sleeve_id = rest[1] });
        return call_and_print(arena, init.io, cl, "StopSleeve", req);
    }
    if (std.mem.eql(u8, act, "rm") and rest.len >= 2) {
        const req = try schema.encode_alloc(arena, schema.SleeveRef{ .sleeve_id = rest[1] });
        return call_and_print(arena, init.io, cl, "RemoveSleeve", req);
    }
    if (std.mem.eql(u8, act, "start") and rest.len >= 2) {
        return cmd_sleeve_start(arena, init.io, cl, rest[1..]);
    }
    if (std.mem.eql(u8, act, "resleeve") and rest.len >= 2) {
        return cmd_sleeve_resleeve(arena, init.io, cl, rest[1..]);
    }
    if (std.mem.eql(u8, act, "takeover") and rest.len >= 2) {
        const req = try schema.encode_alloc(arena, schema.SleeveRef{ .sleeve_id = rest[1] });
        return call_and_print(arena, init.io, cl, "TakeoverTerminal", req);
    }
    if (std.mem.eql(u8, act, "attach") and rest.len >= 2) {
        return cmd_sleeve_attach(arena, init, cl, rest[1]);
    }
    if (std.mem.eql(u8, act, "transcript") and rest.len >= 2) {
        var from_seq: ?u64 = null;
        var limit: ?u64 = null;
        var idx: usize = 2;
        while (idx < rest.len) : (idx += 1) {
            if (std.mem.eql(u8, rest[idx], "--from") and idx + 1 < rest.len) {
                idx += 1;
                from_seq = std.fmt.parseInt(u64, rest[idx], 10) catch null;
            } else if (std.mem.eql(u8, rest[idx], "--limit") and idx + 1 < rest.len) {
                idx += 1;
                limit = std.fmt.parseInt(u64, rest[idx], 10) catch null;
            }
        }
        const req = try schema.encode_alloc(arena, schema.GetTranscriptRequest{
            .sleeve_id = rest[1],
            .from_seq = from_seq,
            .limit = limit,
        });
        return call_and_render(arena, init.io, cl, "GetTranscript", req, json_out, render.print_transcript);
    }
    if (std.mem.eql(u8, act, "wait") and rest.len >= 2) {
        return cmd_sleeve_wait(arena, init.io, cl, rest[1..], json_out);
    }
    if (std.mem.eql(u8, act, "push") and rest.len >= 2) {
        var remote: ?[]const u8 = null;
        var branch: ?[]const u8 = null;
        var force = false;
        var idx: usize = 2;
        while (idx < rest.len) : (idx += 1) {
            const a = rest[idx];
            if (std.mem.eql(u8, a, "--remote") and idx + 1 < rest.len) {
                idx += 1;
                remote = rest[idx];
            } else if (std.mem.eql(u8, a, "--branch") and idx + 1 < rest.len) {
                idx += 1;
                branch = rest[idx];
            } else if (std.mem.eql(u8, a, "--force")) {
                force = true;
            }
        }
        const req = try schema.encode_alloc(arena, schema.GitPushRequest{
            .sleeve_id = rest[1],
            .remote = remote,
            .branch = branch,
            .force = force,
        });
        return call_and_render(arena, init.io, cl, "GitPush", req, json_out, render.print_git_push);
    }
    try print_usage(init.io);
    return 2;
}

fn cmd_sleeve_spawn(
    arena: std.mem.Allocator,
    io: std.Io,
    cl: *const client.Client,
    rest: []const []const u8,
    json_out: bool,
) !u8 {
    var req = schema.SpawnSleeveRequest{ .name = rest[0], .harness = .claude };
    var resleeve_list: std.ArrayList(schema.Harness) = .empty;
    var idx: usize = 1;
    while (idx < rest.len) : (idx += 1) {
        const a = rest[idx];
        if (std.mem.eql(u8, a, "--path") and idx + 1 < rest.len) {
            idx += 1;
            req.path = rest[idx];
        } else if (std.mem.eql(u8, a, "--new-path") and idx + 1 < rest.len) {
            idx += 1;
            req.new_path = rest[idx];
        } else if (std.mem.eql(u8, a, "--parent") and idx + 1 < rest.len) {
            idx += 1;
            req.parent_id = rest[idx];
        } else if (std.mem.eql(u8, a, "--readonly")) {
            req.view = .readonly;
        } else if (std.mem.eql(u8, a, "--worktree")) {
            req.view = .worktree;
        } else if (std.mem.eql(u8, a, "--harness")) {
            idx += 1;
            if (idx == rest.len) return 2;
            req.auto_select = std.mem.eql(u8, rest[idx], "auto");
            req.harness = if (req.auto_select) .claude else parse_harness(rest[idx]) orelse return 2;
        } else if (std.mem.eql(u8, a, "--provider") and idx + 1 < rest.len) {
            idx += 1;
            req.provider = rest[idx];
        } else if (std.mem.eql(u8, a, "--model") and idx + 1 < rest.len) {
            idx += 1;
            req.model = rest[idx];
        } else if (std.mem.eql(u8, a, "--effort") and idx + 1 < rest.len) {
            idx += 1;
            req.effort = rest[idx];
        } else if (std.mem.eql(u8, a, "--resleeve-to") and idx + 1 < rest.len) {
            idx += 1;
            var parts = std.mem.splitScalar(u8, rest[idx], ',');
            while (parts.next()) |p| {
                const tr = std.mem.trim(u8, p, " \t");
                if (parse_harness(tr)) |rh| try resleeve_list.append(arena, rh);
            }
        } else if (std.mem.eql(u8, a, "--task-size")) {
            idx += 1;
            if (idx == rest.len) return 2;
            req.task_size = parse_task_size(rest[idx]) orelse return 2;
        } else if (std.mem.eql(u8, a, "--budget") and idx + 1 < rest.len) {
            idx += 1;
            req.budget = std.fmt.parseInt(u64, rest[idx], 10) catch null;
        } else if (std.mem.eql(u8, a, "--message") and idx + 1 < rest.len) {
            idx += 1;
            req.message = rest[idx];
        } else if (std.mem.eql(u8, a, "--message-file") and idx + 1 < rest.len) {
            idx += 1;
            req.message = try read_message_file(arena, io, rest[idx]);
        } else if (std.mem.eql(u8, a, "--until-done") and idx + 1 < rest.len) {
            idx += 1;
            req.until_done = std.fmt.parseInt(u64, rest[idx], 10) catch null;
        }
    }
    req.resleeve_to = resleeve_list.items;
    const body = try schema.encode_alloc(arena, req);
    if (json_out) return call_and_print(arena, io, cl, "SpawnSleeve", body);
    const res = cl.call_raw(arena, "SpawnSleeve", body) catch |err| {
        try render.print_out(io, "error: {s}\n", .{@errorName(err)});
        return 1;
    };
    if (res.status >= 300) {
        try render.print_error_or_raw(arena, io, res.status, res.body, false);
        return 1;
    }
    const sv = try schema.decode_arena(schema.Sleeve, arena, res.body);
    try render.print_sleeve_detail(io, sv);
    return 0;
}

fn cmd_sleeve_send(
    arena: std.mem.Allocator,
    io: std.Io,
    cl: *const client.Client,
    rest: []const []const u8,
) !u8 {
    const sid = rest[0];
    var delivery: schema.Delivery = .queue;
    var supersede = false;
    var until_done: ?u64 = null;
    var msg_file: ?[]const u8 = null;
    var words: std.ArrayList([]const u8) = .empty;
    var idx: usize = 1;
    while (idx < rest.len) : (idx += 1) {
        const a = rest[idx];
        if (std.mem.eql(u8, a, "--steer")) {
            delivery = .steer;
        } else if (std.mem.eql(u8, a, "--supersede")) {
            supersede = true;
        } else if (std.mem.eql(u8, a, "--until-done") and idx + 1 < rest.len) {
            idx += 1;
            until_done = std.fmt.parseInt(u64, rest[idx], 10) catch null;
        } else if (std.mem.eql(u8, a, "--message-file") and idx + 1 < rest.len) {
            idx += 1;
            msg_file = rest[idx];
        } else {
            try words.append(arena, a);
        }
    }
    const text = if (msg_file) |mf|
        try read_message_file(arena, io, mf)
    else
        try std.mem.join(arena, " ", words.items);
    if (text.len == 0) return 2;
    const req = try schema.encode_alloc(arena, schema.SendMessageRequest{
        .sleeve_id = sid,
        .text = text,
        .delivery = delivery,
        .supersede = supersede,
        .until_done = until_done,
    });
    return call_and_print(arena, io, cl, "SendMessage", req);
}

fn cmd_sleeve_start(
    arena: std.mem.Allocator,
    io: std.Io,
    cl: *const client.Client,
    rest: []const []const u8,
) !u8 {
    var req = schema.StartSleeveRequest{ .sleeve_id = rest[0] };
    var idx: usize = 1;
    while (idx < rest.len) : (idx += 1) {
        const a = rest[idx];
        if (std.mem.eql(u8, a, "--harness") and idx + 1 < rest.len) {
            idx += 1;
            req.target = parse_harness(rest[idx]) orelse return 2;
        } else if (std.mem.eql(u8, a, "--provider") and idx + 1 < rest.len) {
            idx += 1;
            req.provider = rest[idx];
        } else if (std.mem.eql(u8, a, "--model") and idx + 1 < rest.len) {
            idx += 1;
            req.model = rest[idx];
        } else if (std.mem.eql(u8, a, "--effort") and idx + 1 < rest.len) {
            idx += 1;
            req.effort = rest[idx];
        } else if (std.mem.eql(u8, a, "--until-done") and idx + 1 < rest.len) {
            idx += 1;
            req.until_done = std.fmt.parseInt(u64, rest[idx], 10) catch null;
        }
    }
    const body = try schema.encode_alloc(arena, req);
    return call_and_print(arena, io, cl, "StartSleeve", body);
}

fn cmd_sleeve_resleeve(
    arena: std.mem.Allocator,
    io: std.Io,
    cl: *const client.Client,
    rest: []const []const u8,
) !u8 {
    var target: ?schema.Harness = null;
    var model: ?[]const u8 = null;
    var provider: ?[]const u8 = null;
    var effort: ?[]const u8 = null;
    var idx: usize = 1;
    while (idx < rest.len) : (idx += 1) {
        const a = rest[idx];
        if (std.mem.eql(u8, a, "--harness") and idx + 1 < rest.len) {
            idx += 1;
            target = parse_harness(rest[idx]) orelse return 2;
        } else if (std.mem.eql(u8, a, "--provider") and idx + 1 < rest.len) {
            idx += 1;
            provider = rest[idx];
        } else if (std.mem.eql(u8, a, "--model") and idx + 1 < rest.len) {
            idx += 1;
            model = rest[idx];
        } else if (std.mem.eql(u8, a, "--effort") and idx + 1 < rest.len) {
            idx += 1;
            effort = rest[idx];
        }
    }
    const req = try schema.encode_alloc(arena, schema.ResleeveRequest{
        .sleeve_id = rest[0],
        .target = target orelse return 2,
        .model = model,
        .provider = provider,
        .effort = effort,
    });
    return call_and_print(arena, io, cl, "Resleeve", req);
}

fn cmd_sleeve_wait(
    arena: std.mem.Allocator,
    io: std.Io,
    cl: *const client.Client,
    rest: []const []const u8,
    json_out: bool,
) !u8 {
    var sids: std.ArrayList([]const u8) = .empty;
    var until: schema.WaitUntil = .done;
    var any = false;
    var after_seq: ?u64 = null;
    var timeout_ms: u64 = 30_000;
    var idx: usize = 0;
    while (idx < rest.len) : (idx += 1) {
        const a = rest[idx];
        if (std.mem.eql(u8, a, "--all")) {
            any = false;
        } else if (std.mem.eql(u8, a, "--any")) {
            any = true;
        } else if (std.mem.eql(u8, a, "--until") and idx + 1 < rest.len) {
            idx += 1;
            until = std.meta.stringToEnum(schema.WaitUntil, rest[idx]) orelse return 2;
        } else if (std.mem.eql(u8, a, "--after-seq") and idx + 1 < rest.len) {
            idx += 1;
            after_seq = std.fmt.parseInt(u64, rest[idx], 10) catch null;
        } else if (std.mem.eql(u8, a, "--timeout") and idx + 1 < rest.len) {
            idx += 1;
            const secs = std.fmt.parseInt(u64, rest[idx], 10) catch 30;
            timeout_ms = secs *| 1000;
        } else {
            try sids.append(arena, a);
        }
    }
    const req = try schema.encode_alloc(arena, schema.WaitRequest{
        .sleeve_ids = sids.items,
        .until = until,
        .any = any,
        .timeout_ms = timeout_ms,
        .after_seq = after_seq,
    });
    return call_and_render(arena, io, cl, "Wait", req, json_out, render.print_wait_result);
}

fn cmd_sleeve_attach(
    arena: std.mem.Allocator,
    init: std.process.Init,
    cl: *const client.Client,
    target_arg: []const u8,
) !u8 {
    const ls_res = cl.call_raw(arena, "ListSleeves", "{}") catch |err| {
        try render.print_out(init.io, "error: {s}\n", .{@errorName(err)});
        return 1;
    };
    if (ls_res.status >= 300) {
        try render.print_error_or_raw(arena, init.io, ls_res.status, ls_res.body, false);
        return 1;
    }
    const ls = schema.decode_arena(schema.SleeveList, arena, ls_res.body) catch {
        try render.print_out(init.io, "failed to parse sleeve list\n", .{});
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
        try render.print_out(init.io, "sleeve not found: {s}\n", .{target_arg});
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
        try render.print_out(init.io, "takeover failed: {s}\n", .{@errorName(err)});
        return 1;
    };
    if (to_res.status >= 300) {
        try render.print_error_or_raw(arena, init.io, to_res.status, to_res.body, false);
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
        .unknown => try argv.append(arena, "claude"),
    }

    try render.print_out(init.io, "attaching to {s} ({s}) via {s} in {s}\n", .{
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
        try render.print_out(init.io, "failed to launch {s}: {s}\n", .{ argv.items[0], @errorName(err) });
        return 1;
    };
    const term = child.wait(init.io) catch return 1;
    return switch (term) {
        .exited => |code| @intCast(code),
        else => 1,
    };
}

fn cmd_node(
    arena: std.mem.Allocator,
    io: std.Io,
    cl: *const client.Client,
    rest: []const []const u8,
    json_out: bool,
) !u8 {
    const act = rest[0];
    if (std.mem.eql(u8, act, "list")) {
        return call_and_render(arena, io, cl, "ListNodes", "{}", json_out, render.print_node_list);
    }
    if (std.mem.eql(u8, act, "show") and rest.len >= 2) {
        const req = try schema.encode_alloc(arena, schema.NodeRef{ .node_id = rest[1] });
        return call_and_render(arena, io, cl, "GetNode", req, json_out, render.print_node_single);
    }
    if (std.mem.eql(u8, act, "set") and rest.len >= 4) {
        var jobs: ?schema.NodeJobs = null;
        var note: ?[]const u8 = null;
        var idx: usize = 2;
        while (idx < rest.len) : (idx += 1) {
            if (std.mem.eql(u8, rest[idx], "--jobs") and idx + 1 < rest.len) {
                idx += 1;
                jobs = std.meta.stringToEnum(schema.NodeJobs, rest[idx]) orelse return 2;
            } else if (std.mem.eql(u8, rest[idx], "--note") and idx + 1 < rest.len) {
                idx += 1;
                note = rest[idx];
            }
        }
        const req = try schema.encode_alloc(arena, schema.SetNodeJobsRequest{
            .node_id = rest[1],
            .jobs = jobs orelse return 2,
            .note = note,
        });
        return call_and_render(arena, io, cl, "SetNodeJobs", req, json_out, render.print_node_single);
    }
    if (std.mem.eql(u8, act, "join-token")) {
        const res = try cl.call_raw(arena, "GetEnrollment", "{}");
        try render.print_out(io, "{s}\n", .{res.body});
        if (!json_out and res.status == 200) {
            if (schema.decode_arena(schema.Enrollment, arena, res.body)) |en| {
                if (qr.encode(en.control_key, .medium)) |qcode| {
                    const art = try qcode.to_terminal_alloc(arena, 2, false);
                    try render.print_out(io, "{s}\n", .{art});
                } else |_| {}
            } else |_| {}
        }
        return if (res.status < 300) 0 else 1;
    }
    if (std.mem.eql(u8, act, "add") and rest.len >= 2) {
        const req = try schema.encode_alloc(arena, schema.AddNodeRequest{ .registration = rest[1] });
        return call_and_print(arena, io, cl, "AddNode", req);
    }
    if (std.mem.eql(u8, act, "approve") and rest.len >= 2) {
        var name: ?[]const u8 = null;
        var fp: ?[]const u8 = null;
        var idx: usize = 2;
        while (idx < rest.len) : (idx += 1) {
            if (std.mem.eql(u8, rest[idx], "--name") and idx + 1 < rest.len) {
                idx += 1;
                name = rest[idx];
            } else if (std.mem.eql(u8, rest[idx], "--fingerprint") and idx + 1 < rest.len) {
                idx += 1;
                fp = rest[idx];
            } else if (name == null) {
                name = rest[idx];
            }
        }
        const req = try schema.encode_alloc(arena, schema.ApproveNodeRequest{
            .auth_id = rest[1],
            .name = name,
            .fingerprint = fp,
        });
        return call_and_print(arena, io, cl, "ApproveNode", req);
    }
    if (std.mem.eql(u8, act, "rename") and rest.len >= 3) {
        const req = try schema.encode_alloc(arena, schema.RenameNodeRequest{
            .node_id = rest[1],
            .name = rest[2],
        });
        return call_and_print(arena, io, cl, "RenameNode", req);
    }
    if (std.mem.eql(u8, act, "rm") and rest.len >= 2) {
        var force = false;
        for (rest[2..]) |a| {
            if (std.mem.eql(u8, a, "--force")) force = true;
        }
        const req = try schema.encode_alloc(arena, schema.RemoveNodeRequest{
            .node_id = rest[1],
            .force = force,
        });
        return call_and_print(arena, io, cl, "RemoveNode", req);
    }
    if (std.mem.eql(u8, act, "update")) {
        return cmd_node_update(arena, io, cl, rest[1..], json_out);
    }
    try print_usage(io);
    return 2;
}

fn cmd_job(
    arena: std.mem.Allocator,
    io: std.Io,
    cl: *const client.Client,
    rest: []const []const u8,
    json_out: bool,
) !u8 {
    const act = rest[0];
    if (std.mem.eql(u8, act, "list")) {
        var req = schema.ListJobsRequest{};
        var idx: usize = 1;
        while (idx < rest.len) : (idx += 1) {
            if (std.mem.eql(u8, rest[idx], "--node") and idx + 1 < rest.len) {
                idx += 1;
                req.node_id = rest[idx];
            } else if (std.mem.eql(u8, rest[idx], "--limit") and idx + 1 < rest.len) {
                idx += 1;
                req.limit = std.fmt.parseInt(u64, rest[idx], 10) catch null;
            }
        }
        const body = try schema.encode_alloc(arena, req);
        return call_and_render(arena, io, cl, "ListJobs", body, json_out, render.print_job_list);
    }
    if ((std.mem.eql(u8, act, "get") or std.mem.eql(u8, act, "show")) and rest.len >= 2) {
        const req = try schema.encode_alloc(arena, schema.JobRef{ .job_id = rest[1] });
        return call_and_render(arena, io, cl, "GetJob", req, json_out, render.print_job_single);
    }
    if (std.mem.eql(u8, act, "run")) {
        return cmd_job_run(arena, io, cl, rest[1..], json_out);
    }
    if (std.mem.eql(u8, act, "cancel") and rest.len >= 2) {
        const req = try schema.encode_alloc(arena, schema.JobRef{ .job_id = rest[1] });
        return call_and_print(arena, io, cl, "CancelJob", req);
    }
    if ((std.mem.eql(u8, act, "logs") or std.mem.eql(u8, act, "log")) and rest.len >= 2) {
        var req = schema.GetJobLogRequest{ .job_id = rest[1] };
        var idx: usize = 2;
        while (idx < rest.len) : (idx += 1) {
            if (std.mem.eql(u8, rest[idx], "--offset") and idx + 1 < rest.len) {
                idx += 1;
                req.offset = std.fmt.parseInt(u64, rest[idx], 10) catch 0;
            } else if (std.mem.eql(u8, rest[idx], "--limit") and idx + 1 < rest.len) {
                idx += 1;
                req.limit = std.fmt.parseInt(u64, rest[idx], 10) catch null;
            }
        }
        const body = try schema.encode_alloc(arena, req);
        return call_and_render(arena, io, cl, "GetJobLog", body, json_out, render.print_job_log);
    }
    if (std.mem.eql(u8, act, "artifact") and rest.len >= 3) {
        return cmd_job_artifact(arena, io, cl, rest[1..], json_out);
    }
    try print_usage(io);
    return 2;
}

fn cmd_approval(
    arena: std.mem.Allocator,
    io: std.Io,
    cl: *const client.Client,
    rest: []const []const u8,
    json_out: bool,
) !u8 {
    const act = rest[0];
    if (std.mem.eql(u8, act, "list")) {
        var pending = false;
        for (rest[1..]) |a| {
            if (std.mem.eql(u8, a, "--pending")) pending = true;
        }
        const req = try schema.encode_alloc(arena, schema.ListApprovalsRequest{ .pending_only = pending });
        return call_and_render(arena, io, cl, "ListApprovals", req, json_out, render.print_approval_list);
    }
    if (std.mem.eql(u8, act, "decide") and rest.len >= 3) {
        const d_str = rest[2];
        const dec: schema.Decision = if (std.mem.eql(u8, d_str, "allow") or std.mem.eql(u8, d_str, "allow-once") or std.mem.eql(u8, d_str, "allow_once"))
            .allow_once
        else if (std.mem.eql(u8, d_str, "allow-session") or std.mem.eql(u8, d_str, "allow_session"))
            .allow_session
        else if (std.mem.eql(u8, d_str, "deny"))
            .deny
        else
            return 2;
        const req = try schema.encode_alloc(arena, schema.DecideRequest{
            .approval_id = rest[1],
            .decision = dec,
        });
        return call_and_print(arena, io, cl, "Decide", req);
    }
    try print_usage(io);
    return 2;
}

fn cmd_event(
    arena: std.mem.Allocator,
    io: std.Io,
    cl: *const client.Client,
    rest: []const []const u8,
    json_out: bool,
) !u8 {
    const act = rest[0];
    if (std.mem.eql(u8, act, "list")) {
        var req = schema.ListEventsRequest{};
        var idx: usize = 1;
        while (idx < rest.len) : (idx += 1) {
            if (std.mem.eql(u8, rest[idx], "--from") and idx + 1 < rest.len) {
                idx += 1;
                req.from_seq = std.fmt.parseInt(u64, rest[idx], 10) catch null;
            } else if (std.mem.eql(u8, rest[idx], "--limit") and idx + 1 < rest.len) {
                idx += 1;
                req.limit = std.fmt.parseInt(u64, rest[idx], 10) catch null;
            }
        }
        const body = try schema.encode_alloc(arena, req);
        return call_and_render(arena, io, cl, "ListEvents", body, json_out, render.print_event_list);
    }
    if (std.mem.eql(u8, act, "read") and rest.len >= 2) {
        const seq = std.fmt.parseInt(u64, rest[1], 10) catch return 2;
        const req = try schema.encode_alloc(arena, schema.MarkEventsReadRequest{ .up_to_seq = seq });
        return call_and_print(arena, io, cl, "MarkEventsRead", req);
    }
    try print_usage(io);
    return 2;
}

fn cmd_envoy(
    arena: std.mem.Allocator,
    io: std.Io,
    cl: *const client.Client,
    default_as: ?[]const u8,
    rest: []const []const u8,
    json_out: bool,
) !u8 {
    const act = rest[0];
    if (std.mem.eql(u8, act, "token")) {
        const res = cl.call_raw(arena, "GetEnvoyToken", "{}") catch |err| {
            try render.print_out(io, "error: {s}\n", .{@errorName(err)});
            return 1;
        };
        if (json_out or res.status >= 300) {
            try render.print_error_or_raw(arena, io, res.status, res.body, json_out);
            return if (res.status < 300) 0 else 1;
        }
        const et = try schema.decode_arena(schema.EnvoyToken, arena, res.body);
        try render.print_out(io, "{s}\n", .{et.token});
        return 0;
    }
    if (std.mem.eql(u8, act, "send")) {
        var agent: ?[]const u8 = default_as;
        var msg_file: ?[]const u8 = null;
        var words: std.ArrayList([]const u8) = .empty;
        var idx: usize = 1;
        while (idx < rest.len) : (idx += 1) {
            const a = rest[idx];
            if (std.mem.eql(u8, a, "--as") and idx + 1 < rest.len) {
                idx += 1;
                agent = rest[idx];
            } else if (std.mem.eql(u8, a, "--message-file") and idx + 1 < rest.len) {
                idx += 1;
                msg_file = rest[idx];
            } else {
                try words.append(arena, a);
            }
        }
        const text = if (msg_file) |mf|
            try read_message_file(arena, io, mf)
        else
            try std.mem.join(arena, " ", words.items);
        if (text.len == 0) return 2;
        const req = try schema.encode_alloc(arena, schema.EnvoySendRequest{
            .text = text,
            .agent = agent,
        });
        return call_and_print(arena, io, cl, "EnvoySend", req);
    }
    if (std.mem.eql(u8, act, "wait")) {
        var req = schema.EnvoyWaitRequest{};
        var idx: usize = 1;
        while (idx < rest.len) : (idx += 1) {
            const a = rest[idx];
            if (std.mem.eql(u8, a, "--timeout") and idx + 1 < rest.len) {
                idx += 1;
                const secs = std.fmt.parseInt(u64, rest[idx], 10) catch 30;
                req.timeout_ms = secs *| 1000;
            } else if (req.request_id == null) {
                req.request_id = a;
            }
        }
        const body = try schema.encode_alloc(arena, req);
        return call_and_render(arena, io, cl, "EnvoyWait", body, json_out, render.print_envoy_reply);
    }
    if (std.mem.eql(u8, act, "read")) {
        var req = schema.EnvoyReadRequest{};
        var idx: usize = 1;
        while (idx < rest.len) : (idx += 1) {
            if (std.mem.eql(u8, rest[idx], "--from") and idx + 1 < rest.len) {
                idx += 1;
                req.from_seq = std.fmt.parseInt(u64, rest[idx], 10) catch 0;
            } else if (std.mem.eql(u8, rest[idx], "--limit") and idx + 1 < rest.len) {
                idx += 1;
                req.limit = std.fmt.parseInt(u64, rest[idx], 10) catch null;
            }
        }
        const body = try schema.encode_alloc(arena, req);
        return call_and_render(arena, io, cl, "EnvoyRead", body, json_out, render.print_transcript);
    }
    try print_usage(io);
    return 2;
}

fn cmd_history(
    arena: std.mem.Allocator,
    io: std.Io,
    cl: *const client.Client,
    rest: []const []const u8,
    json_out: bool,
) !u8 {
    const act = rest[0];
    if (std.mem.eql(u8, act, "search") and rest.len >= 2) {
        var words: std.ArrayList([]const u8) = .empty;
        var sleeves: std.ArrayList([]const u8) = .empty;
        var roles: std.ArrayList(schema.HistoryRole) = .empty;
        var mode: schema.HistoryMode = .all;
        var harness: ?schema.Harness = null;
        var session: ?[]const u8 = null;
        var since_ms: ?schema.TimeMs = null;
        var until_ms: ?schema.TimeMs = null;
        var limit: ?u64 = null;
        const now_ms = std.Io.Clock.real.now(io).toMilliseconds();

        var idx: usize = 1;
        while (idx < rest.len) : (idx += 1) {
            const a = rest[idx];
            if (std.mem.eql(u8, a, "--mode") and idx + 1 < rest.len) {
                idx += 1;
                mode = std.meta.stringToEnum(schema.HistoryMode, rest[idx]) orelse return 2;
            } else if (std.mem.eql(u8, a, "--sleeve") and idx + 1 < rest.len) {
                idx += 1;
                try sleeves.append(arena, rest[idx]);
            } else if (std.mem.eql(u8, a, "--harness") and idx + 1 < rest.len) {
                idx += 1;
                harness = parse_harness(rest[idx]) orelse return 2;
            } else if (std.mem.eql(u8, a, "--session") and idx + 1 < rest.len) {
                idx += 1;
                session = rest[idx];
            } else if (std.mem.eql(u8, a, "--role") and idx + 1 < rest.len) {
                idx += 1;
                const r_s = rest[idx];
                const role: schema.HistoryRole = if (std.mem.eql(u8, r_s, "user"))
                    .user
                else if (std.mem.eql(u8, r_s, "assistant"))
                    .assistant
                else if (std.mem.eql(u8, r_s, "tool-call") or std.mem.eql(u8, r_s, "tool_call"))
                    .tool_call
                else if (std.mem.eql(u8, r_s, "tool-error") or std.mem.eql(u8, r_s, "tool_error"))
                    .tool_error
                else if (std.mem.eql(u8, r_s, "compaction"))
                    .compaction
                else
                    return 2;
                try roles.append(arena, role);
            } else if (std.mem.eql(u8, a, "--since") and idx + 1 < rest.len) {
                idx += 1;
                if (parse_time_arg(rest[idx], now_ms)) |ms| since_ms = .{ .ms = ms };
            } else if (std.mem.eql(u8, a, "--until") and idx + 1 < rest.len) {
                idx += 1;
                if (parse_time_arg(rest[idx], now_ms)) |ms| until_ms = .{ .ms = ms };
            } else if (std.mem.eql(u8, a, "--limit") and idx + 1 < rest.len) {
                idx += 1;
                limit = std.fmt.parseInt(u64, rest[idx], 10) catch null;
            } else {
                try words.append(arena, a);
            }
        }
        const query = try std.mem.join(arena, " ", words.items);
        if (query.len == 0) return 2;
        const req = try schema.encode_alloc(arena, schema.SearchHistoryRequest{
            .query = query,
            .mode = mode,
            .sleeves = sleeves.items,
            .harness = harness,
            .session = session,
            .since_ms = since_ms,
            .until_ms = until_ms,
            .roles = roles.items,
            .limit = limit,
        });
        return call_and_render(arena, io, cl, "SearchHistory", req, json_out, render.print_history_hits);
    }
    if (std.mem.eql(u8, act, "read") and rest.len >= 2) {
        var req = schema.ReadHistoryRequest{};
        var idx: usize = 1;
        while (idx < rest.len) : (idx += 1) {
            const a = rest[idx];
            if (std.mem.eql(u8, a, "--doc") and idx + 1 < rest.len) {
                idx += 1;
                req.doc_id = std.fmt.parseInt(u64, rest[idx], 10) catch return 2;
            } else if (std.mem.eql(u8, a, "--from") and idx + 1 < rest.len) {
                idx += 1;
                req.from_turn = std.fmt.parseInt(u64, rest[idx], 10) catch 0;
            } else if (std.mem.eql(u8, a, "--to") and idx + 1 < rest.len) {
                idx += 1;
                req.to_turn = std.fmt.parseInt(u64, rest[idx], 10) catch null;
            } else if (std.mem.eql(u8, a, "--after") and idx + 1 < rest.len) {
                idx += 1;
                req.after_doc = std.fmt.parseInt(u64, rest[idx], 10) catch null;
            } else if (req.session == null) {
                req.session = a;
            }
        }
        const body = try schema.encode_alloc(arena, req);
        return call_and_render(arena, io, cl, "ReadHistory", body, json_out, render.print_history_read);
    }
    try print_usage(io);
    return 2;
}

fn cmd_device(
    arena: std.mem.Allocator,
    io: std.Io,
    cl: *const client.Client,
    rest: []const []const u8,
    json_out: bool,
) !u8 {
    const act = rest[0];
    if (std.mem.eql(u8, act, "list")) {
        return call_and_render(arena, io, cl, "ListDevices", "{}", json_out, render.print_device_list);
    }
    if (std.mem.eql(u8, act, "add") and rest.len >= 2) {
        return cmd_device_add(arena, io, cl, rest[1], json_out);
    }
    if (std.mem.eql(u8, act, "pair") and rest.len >= 2) {
        const req = try schema.encode_alloc(arena, schema.PairDeviceRequest{ .code = rest[1] });
        return call_and_print(arena, io, cl, "PairDevice", req);
    }
    if (std.mem.eql(u8, act, "revoke") and rest.len >= 2) {
        const req = try schema.encode_alloc(arena, schema.DeviceRef{ .device_id = rest[1] });
        return call_and_print(arena, io, cl, "RevokeDevice", req);
    }
    try print_usage(io);
    return 2;
}

fn cmd_device_add(
    arena: std.mem.Allocator,
    io: std.Io,
    cl: *const client.Client,
    name: []const u8,
    json_out: bool,
) !u8 {
    const req = try schema.encode_alloc(arena, schema.AddDeviceRequest{ .name = name });
    const res = cl.call_raw(arena, "AddDevice", req) catch |err| {
        try render.print_out(io, "error: {s}\n", .{@errorName(err)});
        return 1;
    };
    if (json_out or res.status >= 300) {
        try render.print_error_or_raw(arena, io, res.status, res.body, json_out);
        return if (res.status < 300) 0 else 1;
    }
    if (schema.decode_arena(schema.PairingCode, arena, res.body)) |pc| {
        try render.print_out(io, "device={s}  code={s}  expires_at_ms={d}\n", .{ pc.name, pc.code, pc.expires_at_ms.ms });
        if (qr.encode(pc.code, .medium)) |qcode| {
            const art = try qcode.to_terminal_alloc(arena, 2, false);
            try render.print_out(io, "{s}\n", .{art});
        } else |_| {}
    } else |_| {
        try render.print_out(io, "{s}\n", .{res.body});
    }
    return 0;
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
    return call_and_render(arena, io, cl, "UpdateHub", body, json_out, render.print_hub_update);
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
    return call_and_render(arena, io, cl, "UpdateNodes", body, json_out, render.print_node_updates);
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
        try render.print_out(io, "usage: protectorate job run [--node <id> | --arch <arch>] [--exclusive] [--slots <n>] [--long-slots <n>] [--mem-mib <mib>] [--parent-epoch <ep>] [--wait] -- <argv...>\n", .{});
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
        try render.print_out(io, "error: {s}\n", .{@errorName(err)});
        return 1;
    };
    if (res.status >= 300) {
        if (json_out) {
            try render.print_out(io, "{s}\n", .{res.body});
        } else {
            try render.print_error_or_raw(arena, io, res.status, res.body, false);
        }
        return 1;
    }
    if (!wait_done) {
        if (json_out) {
            try render.print_out(io, "{s}\n", .{res.body});
        } else {
            try render.print_job_single(arena, io, res.body);
        }
        return 0;
    }

    var job = try schema.decode_arena(schema.Job, arena, res.body);
    var latest_body: []const u8 = res.body;
    const poll_req = try schema.encode_alloc(arena, schema.JobRef{ .job_id = job.id });
    var unreach_start: ?i64 = null;
    while (true) {
        const done = switch (job.state) {
            .succeeded, .failed, .interrupted, .lost, .cancelled, .abandoned => true,
            else => false,
        };
        if (done) break;
        io.sleep(.fromMilliseconds(250), .awake) catch break;
        const gres = cl.call_raw(arena, "GetJob", poll_req) catch {
            const now = std.Io.Clock.awake.now(io).toMilliseconds();
            if (unreach_start == null) unreach_start = now;
            if (now - unreach_start.? >= 60_000) return 6;
            continue;
        };
        if (gres.status >= 300) {
            const now = std.Io.Clock.awake.now(io).toMilliseconds();
            if (unreach_start == null) unreach_start = now;
            if (now - unreach_start.? >= 60_000) return 6;
            continue;
        }
        unreach_start = null;
        latest_body = gres.body;
        job = schema.decode_arena(schema.Job, arena, gres.body) catch continue;
    }

    if (json_out) {
        try render.print_out(io, "{s}\n", .{latest_body});
    } else {
        try render.print_job_single(arena, io, latest_body);
    }
    return switch (job.state) {
        .succeeded => 0,
        .interrupted => 3,
        .lost => 5,
        .failed => if (job.exit_code) |ec| @intCast(@min(@as(i32, 255), @max(@as(i32, 1), ec))) else 1,
        else => 1,
    };
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
            try render.print_out(io, "error: {s}\n", .{@errorName(err)});
            return 1;
        };
        if (res.status >= 300) {
            try render.print_error_or_raw(arena, io, res.status, res.body, false);
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
        try render.print_out(io, "wrote {d} bytes to {s}\n", .{ all.items.len, fp });
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
        return call_and_render(arena, io, cl, "GetNodeAdmission", req, json_out, render.print_node_admission);
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
        try render.print_out(io, "error: {s}\n", .{@errorName(err)});
        return 1;
    };
    try render.print_out(io, "{s}\n", .{res.body});
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
        try render.print_out(io, "error: {s}\n", .{@errorName(err)});
        return 1;
    };
    if (json_out) {
        try render.print_out(io, "{s}\n", .{res.body});
        return if (res.status < 300) 0 else 1;
    }
    if (res.status >= 300) {
        try render.print_error_or_raw(arena, io, res.status, res.body, false);
        return 1;
    }
    formatter(arena, io, res.body) catch {
        try render.print_out(io, "{s}\n", .{res.body});
    };
    return 0;
}

fn parse_harness(raw: []const u8) ?schema.Harness {
    if (std.mem.eql(u8, raw, "claude")) return .claude;
    if (std.mem.eql(u8, raw, "codex")) return .codex;
    if (std.mem.eql(u8, raw, "grok")) return .grok;
    if (std.mem.eql(u8, raw, "pi_vendor")) return .pi_vendor;
    if (std.mem.eql(u8, raw, "pi_local") or std.mem.eql(u8, raw, "pi")) return .pi_local;
    if (std.mem.eql(u8, raw, "antigravity") or std.mem.eql(u8, raw, "agy")) return .antigravity;
    return null;
}

fn parse_task_size(raw: []const u8) ?schema.TaskSize {
    const size = std.meta.stringToEnum(schema.TaskSize, raw) orelse return null;
    return if (size == .unknown) null else size;
}

fn parse_time_arg(raw: []const u8, now_ms: i64) ?i64 {
    if (raw.len == 0) return null;
    const last = raw[raw.len - 1];
    const mult: i64 = switch (last) {
        's' => 1_000,
        'm' => 60_000,
        'h' => 3_600_000,
        'd' => 86_400_000,
        else => 0,
    };
    if (mult > 0 and raw.len > 1) {
        const v = std.fmt.parseInt(i64, raw[0 .. raw.len - 1], 10) catch return null;
        return now_ms - (v *| mult);
    }
    return std.fmt.parseInt(i64, raw, 10) catch null;
}

fn read_message_file(arena: std.mem.Allocator, io: std.Io, path: []const u8) ![]const u8 {
    if (std.mem.eql(u8, path, "-")) {
        var out: std.ArrayList(u8) = .empty;
        var tmp: [4096]u8 = undefined;
        const s: []u8 = &tmp;
        const bufs = [_][]u8{s};
        const st = std.Io.File.stdin();
        while (true) {
            const n = st.readStreaming(io, &bufs) catch break;
            if (n == 0) break;
            try out.appendSlice(arena, tmp[0..n]);
            if (out.items.len >= 4 << 20) break;
        }
        return out.items;
    }
    return try std.Io.Dir.cwd().readFileAlloc(io, path, arena, .limited(4 << 20));
}

fn resolve_hub_home(arena: std.mem.Allocator, env: *const std.process.Environ.Map, override: ?[]const u8) ![]u8 {
    if (override) |o| return arena.dupe(u8, o);
    if (env.get("PROTECTORATE_HOME")) |e| return arena.dupe(u8, e);
    if (env.get("HOME")) |h| return std.fs.path.join(arena, &.{ h, ".local/share/protectorate" });
    if (env.get("USERPROFILE")) |h| return std.fs.path.join(arena, &.{ h, ".protectorate" });
    return arena.dupe(u8, ".protectorate");
}

fn print_usage(io: std.Io) !void {
    try render.print_out(
        io,
        \\protectorate - AI coding agent orchestrator & Noise_IK fleet hub
        \\
        \\usage:
        \\  protectorate serve [--home <dir>] [--listen <addr:port>] [--proxy-listen <addr:port>] [--trusted-proxy <ip>]... [--fleet-listen <addr:port>] [--no-sandbox] [--dev-token <tok>]
        \\  protectorate [--json] version | status | doctor [--refresh] | pair [<name>] | token | quota [list]
        \\  protectorate [--json] hub (info | state | update [--check|--rollback] [--channel stable|dev] [--version <v>] | probation [--retry-rollback])
        \\  protectorate [--json] harness (update [<program>...] | login <claude|codex|grok|pi|agy> [-- <args...>])
        \\  protectorate [--json] sleeve (list | show <id> | spawn <name> [--harness <h|auto>] [opts] | send <id> <text> | inbox [<id>] [--drop] |
        \\                                interrupt <id> | stop <id> | rm <id> | start <id> [opts] | resleeve <id> --harness <h> |
        \\                                takeover <id> | attach <id> | transcript <id> [opts] | wait <id>... [opts] | push <id> [opts])
        \\  protectorate [--json] node (list | show <n> | set <n> --jobs any|owner|none [--note <txt>] | join-token |
        \\                              add <reg> | approve <id> [--name <n>] [--fingerprint <fp>] | rename <n> <new> | rm <n> | update [opts])
        \\  protectorate [--json] job (list [--node <n>] [--limit <k>] | get <id> | run [opts] -- <argv...> | cancel <id> | logs <id> [opts] | artifact <id> <path> [--out <f>])
        \\  protectorate [--json] lease (list <node> | acquire <node> [opts] | renew <node> <lid> <ep> | release <node> <lid> <ep> | verify <node> <boot> <ep> | unquarantine <node>)
        \\  protectorate [--json] approval (list [--pending] | decide <id> allow|allow-once|allow-session|deny)
        \\  protectorate [--json] event (list [--from <seq>] [--limit <n>] | read <seq>)
        \\  protectorate [--json] envoy (send <text> [--as <name>] | wait [<rid>] [--timeout <s>] | read [opts] | token)
        \\  protectorate [--json] history (search <query> [opts] | read (--doc <id> | <session>) [opts])
        \\  protectorate [--json] device (list | add <name> | pair <code> | revoke <id>)
        \\  protectorate rpc <Method> [<json>]
        \\  protectorate [--envoy] [--token-file <path>] [--as <name>] mcp
        \\
    ,
        .{},
    );
}
