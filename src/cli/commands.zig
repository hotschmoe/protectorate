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
        if (std.mem.eql(u8, arg, "--version") or std.mem.eql(u8, arg, "-V")) {
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
        const j = try std.json.Stringify.valueAlloc(arena, rep, .{ .whitespace = .indent_2 });
        try print_out(init.io, "{s}\n", .{j});
        return .{ .exit = 0 };
    }

    var cl = try client.Client.resolve(init.gpa, init.io, init.environ_map, url_opt, token_opt, home_opt);
    defer cl.deinit();

    if (std.mem.eql(u8, sub, "status")) {
        return .{ .exit = try call_and_print(arena, init.io, &cl, "GetState", "{}") };
    }
    if (std.mem.eql(u8, sub, "pair")) {
        const name = if (pos.items.len > 1) pos.items[1] else "device";
        const req = try schema.encode_alloc(arena, schema.AddDeviceRequest{ .name = name });
        const res = try cl.call_raw(arena, "AddDevice", req);
        try print_out(init.io, "{s}\n", .{res.body});
        if (!json_out and res.status == 200) {
            if (schema.decode_arena(schema.PairingCode, arena, res.body)) |pc| {
                if (qr.encode(pc.code, .medium)) |qcode| {
                    const art = try qcode.to_terminal_alloc(arena, 2, false);
                    try print_out(init.io, "{s}\n", .{art});
                } else |_| {}
            } else |_| {}
        }
        return .{ .exit = if (res.status < 300) 0 else 1 };
    }
    if (std.mem.eql(u8, sub, "rpc") and pos.items.len >= 2) {
        const method = pos.items[1];
        const body = if (pos.items.len >= 3) pos.items[2] else "{}";
        return .{ .exit = try call_and_print(arena, init.io, &cl, method, body) };
    }
    if (std.mem.eql(u8, sub, "sleeve") and pos.items.len >= 2) {
        const act = pos.items[1];
        if (std.mem.eql(u8, act, "list")) return .{ .exit = try call_and_print(arena, init.io, &cl, "ListSleeves", "{}") };
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
            return .{ .exit = try call_and_print(arena, init.io, &cl, "GetTranscript", req) };
        }
        if (std.mem.eql(u8, act, "prompt") and pos.items.len >= 4) {
            const req = try schema.encode_alloc(arena, schema.SendMessageRequest{
                .sleeve_id = pos.items[2],
                .text = pos.items[3],
            });
            return .{ .exit = try call_and_print(arena, init.io, &cl, "SendMessage", req) };
        }
    }
    if (std.mem.eql(u8, sub, "node") and pos.items.len >= 2) {
        const act = pos.items[1];
        if (std.mem.eql(u8, act, "list")) return .{ .exit = try call_and_print(arena, init.io, &cl, "ListNodes", "{}") };
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
    }
    if (std.mem.eql(u8, sub, "job") and pos.items.len >= 2) {
        const act = pos.items[1];
        if (std.mem.eql(u8, act, "list")) return .{ .exit = try call_and_print(arena, init.io, &cl, "ListJobs", "{}") };
        if (std.mem.eql(u8, act, "get") and pos.items.len >= 3) {
            const req = try schema.encode_alloc(arena, schema.JobRef{ .job_id = pos.items[2] });
            return .{ .exit = try call_and_print(arena, init.io, &cl, "GetJob", req) };
        }
        if (std.mem.eql(u8, act, "cancel") and pos.items.len >= 3) {
            const req = try schema.encode_alloc(arena, schema.JobRef{ .job_id = pos.items[2] });
            return .{ .exit = try call_and_print(arena, init.io, &cl, "CancelJob", req) };
        }
        if (std.mem.eql(u8, act, "logs") and pos.items.len >= 3) {
            const req = try schema.encode_alloc(arena, schema.GetJobLogRequest{ .job_id = pos.items[2] });
            return .{ .exit = try call_and_print(arena, init.io, &cl, "GetJobLog", req) };
        }
    }
    if (std.mem.eql(u8, sub, "history") and pos.items.len >= 3 and std.mem.eql(u8, pos.items[1], "search")) {
        const req = try schema.encode_alloc(arena, schema.SearchHistoryRequest{ .query = pos.items[2] });
        return .{ .exit = try call_and_print(arena, init.io, &cl, "SearchHistory", req) };
    }
    if (std.mem.eql(u8, sub, "quota")) {
        return .{ .exit = try call_and_print(arena, init.io, &cl, "ListQuotas", "{}") };
    }

    try print_usage(init.io);
    return .{ .exit = 2 };
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
        \\  protectorate status | doctor | pair [<name>] | token | quota
        \\  protectorate sleeve (list | stop <id> | rm <id> | prompt <id> <text> | transcript <id>)
        \\  protectorate node (list | join-token | add <reg> | approve <id> [<name>])
        \\  protectorate job (list | get <id> | cancel <id> | logs <id>)
        \\  protectorate history search <query>
        \\  protectorate rpc <Method> [<json>]
        \\  protectorate --envoy | mcp
        \\
    ,
        .{},
    );
}
