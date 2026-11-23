const std = @import("std");
const builtin = @import("builtin");
const s_exec = @import("sandbox/exec.zig");
const version = @import("version.zig");
const doctor = @import("doctor/doctor.zig");
const msg = @import("net/msg.zig");
const sys = @import("node/sys.zig");
const serve = @import("node/serve.zig");
const service = @import("node/service.zig");

pub fn main(init: std.process.Init) !u8 {
    var arena_st = std.heap.ArenaAllocator.init(init.gpa);
    defer arena_st.deinit();
    const arena = arena_st.allocator();

    const args = try init.minimal.args.toSlice(arena);
    if (args.len >= 2 and std.mem.eql(u8, args[1], "__guard")) {
        return try s_exec.run_guard_trampoline(init, args[2..]);
    }
    var home_override: ?[]const u8 = null;
    var sock_override: ?[]const u8 = null;
    var stdio_mode = false;
    var user_mode = false;
    var retry_rollback = false;
    var interval_secs: u64 = 2;
    var limit_opt: ?u64 = null;
    var name_opt: ?[]const u8 = null;
    var hub_addr_opt: ?[]const u8 = null;
    var hub_key_opt: ?[]const u8 = null;
    var positionals: std.ArrayList([]const u8) = .empty;

    var i: usize = 1;
    while (i < args.len) : (i += 1) {
        const arg = args[i];
        if (std.mem.eql(u8, arg, "--version") or std.mem.eql(u8, arg, "-V")) {
            var buf: [256]u8 = undefined;
            var w = std.Io.File.stdout().writer(init.io, &buf);
            try w.interface.print("protectorate-node {s} ({s})\n", .{ version.version, version.git_commit });
            try w.interface.flush();
            return 0;
        } else if (std.mem.eql(u8, arg, "--help") or std.mem.eql(u8, arg, "-h")) {
            try print_usage(init.io);
            return 0;
        } else if (std.mem.eql(u8, arg, "--stdio")) {
            stdio_mode = true;
        } else if (std.mem.eql(u8, arg, "--user")) {
            user_mode = true;
        } else if (std.mem.eql(u8, arg, "--retry-rollback")) {
            retry_rollback = true;
        } else if (std.mem.eql(u8, arg, "--interval") and i + 1 < args.len) {
            i += 1;
            interval_secs = @max(@as(u64, 1), std.fmt.parseInt(u64, args[i], 10) catch 2);
        } else if (std.mem.eql(u8, arg, "--limit") and i + 1 < args.len) {
            i += 1;
            limit_opt = std.fmt.parseInt(u64, args[i], 10) catch null;
        } else if (std.mem.eql(u8, arg, "--home") and i + 1 < args.len) {
            i += 1;
            home_override = args[i];
        } else if (std.mem.eql(u8, arg, "--sock") and i + 1 < args.len) {
            i += 1;
            sock_override = args[i];
        } else if (std.mem.eql(u8, arg, "--name") and i + 1 < args.len) {
            i += 1;
            name_opt = args[i];
        } else if (std.mem.eql(u8, arg, "--hub") and i + 1 < args.len) {
            i += 1;
            hub_addr_opt = args[i];
        } else if (std.mem.eql(u8, arg, "--hub-key") and i + 1 < args.len) {
            i += 1;
            hub_key_opt = args[i];
        } else {
            try positionals.append(arena, arg);
        }
    }

    if (positionals.items.len == 0) {
        try print_usage(init.io);
        return 0;
    }

    const home_path = try serve.resolve_node_home(init.gpa, init.environ_map, home_override);
    defer init.gpa.free(home_path);

    const sub = positionals.items[0];
    if (std.mem.eql(u8, sub, "join")) {
        const tok = if (positionals.items.len > 1) positionals.items[1] else null;
        return serve.cmd_join(init.gpa, init.io, home_path, tok, hub_addr_opt, hub_key_opt, name_opt);
    } else if (std.mem.eql(u8, sub, "authorize")) {
        return serve.cmd_authorize(init.gpa, init.io, init.environ_map, home_path);
    } else if (std.mem.eql(u8, sub, "serve")) {
        return serve.cmd_serve(init.gpa, init.io, init.environ_map, home_path, stdio_mode);
    } else if (std.mem.eql(u8, sub, "supervise")) {
        return service.cmd_supervise(init.gpa, init.io, init.environ_map, home_path);
    } else if (std.mem.eql(u8, sub, "install-service")) {
        return service.cmd_install_service(init.gpa, init.io, init.environ_map, home_path, user_mode);
    } else if (std.mem.eql(u8, sub, "uninstall")) {
        return service.cmd_uninstall(init.gpa, init.io, init.environ_map, home_path, user_mode);
    } else if (std.mem.eql(u8, sub, "probation")) {
        return service.cmd_probation(init.gpa, init.io, home_path, retry_rollback);
    } else if (std.mem.eql(u8, sub, "status")) {
        return serve.cmd_status(init.gpa, init.io, home_path);
    } else if (std.mem.eql(u8, sub, "doctor")) {
        const rep = try doctor.run_checks(arena, init.io, init.environ_map.get("HOME"));
        const json = try std.json.Stringify.valueAlloc(arena, rep, .{ .whitespace = .indent_2 });
        return write_stdout_line(init.io, json);
    } else if (std.mem.eql(u8, sub, "inventory")) {
        const inv = try sys.collect_inventory(arena, init.io, version.version);
        const json = try std.json.Stringify.valueAlloc(arena, inv, .{});
        return write_stdout_line(init.io, json);
    } else if (std.mem.eql(u8, sub, "sample")) {
        const smp = sys.collect_sample(init.io);
        const json = try std.json.Stringify.valueAlloc(arena, smp, .{});
        return write_stdout_line(init.io, json);
    } else if (std.mem.eql(u8, sub, "presence")) {
        const pr = sys.collect_presence(init.io, init.environ_map);
        const json = try std.json.Stringify.valueAlloc(arena, pr, .{});
        return write_stdout_line(init.io, json);
    } else if (std.mem.eql(u8, sub, "watch-presence")) {
        var count: u64 = 0;
        var last_state: ?msg.PresenceState = null;
        var last_locked: ?bool = null;
        var line_arena = std.heap.ArenaAllocator.init(init.gpa);
        defer line_arena.deinit();
        while (true) {
            const pr = sys.collect_presence(init.io, init.environ_map);
            if (last_state == null or last_state.? != pr.state or last_locked != pr.locked) {
                last_state = pr.state;
                last_locked = pr.locked;
                _ = line_arena.reset(.retain_capacity);
                const json = try std.json.Stringify.valueAlloc(line_arena.allocator(), pr, .{});
                _ = try write_stdout_line(init.io, json);
                count += 1;
                if (limit_opt) |lim| {
                    if (count >= lim) break;
                }
            }
            init.io.sleep(.fromMilliseconds(@intCast(interval_secs * 1000)), .awake) catch break;
        }
        return 0;
    } else if (std.mem.eql(u8, sub, "lease")) {
        return cmd_local_lease(arena, init, home_path, sock_override, positionals.items[1..]);
    }

    try print_usage(init.io);
    return 2;
}

fn write_stdout_line(io: std.Io, line: []const u8) !u8 {
    var buf: [4096]u8 = undefined;
    var w = std.Io.File.stdout().writer(io, &buf);
    try w.interface.writeAll(line);
    try w.interface.writeByte('\n');
    try w.interface.flush();
    return 0;
}

fn cmd_local_lease(
    arena: std.mem.Allocator,
    init: std.process.Init,
    home_path: []const u8,
    sock_override: ?[]const u8,
    rest: []const []const u8,
) !u8 {
    if (comptime builtin.os.tag == .windows) {
        var eb: [256]u8 = undefined;
        var ew = std.Io.File.stderr().writer(init.io, &eb);
        try ew.interface.writeAll("local admission socket is only available on POSIX nodes; use hub RPC on Windows\n");
        try ew.interface.flush();
        return 2;
    }
    if (rest.len == 0) {
        try print_usage(init.io);
        return 2;
    }

    const sock_path = sock_override orelse
        init.environ_map.get("PROTECTORATE_ADMISSION_SOCK") orelse
        try std.fs.path.join(arena, &.{ home_path, "admission.sock" });

    const op = rest[0];
    const req: msg.Request = if (std.mem.eql(u8, op, "list"))
        .{ .list_leases = .{} }
    else if (std.mem.eql(u8, op, "unquarantine"))
        .{ .clear_quarantine = .{} }
    else if (std.mem.eql(u8, op, "acquire")) blk: {
        var al = msg.AcquireLease{ .holder_id = "local" };
        var idx: usize = 1;
        while (idx < rest.len) : (idx += 1) {
            const f = rest[idx];
            if (std.mem.eql(u8, f, "--exclusive")) {
                al.exclusive = true;
            } else if (std.mem.eql(u8, f, "--holder") and idx + 1 < rest.len) {
                idx += 1;
                al.holder_id = rest[idx];
            } else if (std.mem.eql(u8, f, "--slots") and idx + 1 < rest.len) {
                idx += 1;
                al.slots = std.fmt.parseInt(u32, rest[idx], 10) catch 1;
            } else if (std.mem.eql(u8, f, "--long-slots") and idx + 1 < rest.len) {
                idx += 1;
                al.long_slots = std.fmt.parseInt(u32, rest[idx], 10) catch 0;
            } else if (std.mem.eql(u8, f, "--mem-mib") and idx + 1 < rest.len) {
                idx += 1;
                const mib = std.fmt.parseInt(u64, rest[idx], 10) catch 0;
                al.mem_bytes = mib * 1024 * 1024;
            } else if (std.mem.eql(u8, f, "--parent-epoch") and idx + 1 < rest.len) {
                idx += 1;
                al.parent_epoch = std.fmt.parseInt(u64, rest[idx], 10) catch null;
            } else if (std.mem.eql(u8, f, "--ttl-ms") and idx + 1 < rest.len) {
                idx += 1;
                al.ttl_ms = std.fmt.parseInt(i64, rest[idx], 10) catch null;
            } else if (std.mem.eql(u8, f, "--wait-ms") and idx + 1 < rest.len) {
                idx += 1;
                al.wait_ms = std.fmt.parseInt(i64, rest[idx], 10) catch null;
            }
        }
        break :blk .{ .acquire_lease = al };
    } else if (std.mem.eql(u8, op, "renew") and rest.len >= 3) blk: {
        const ep = std.fmt.parseInt(u64, rest[2], 10) catch return 2;
        break :blk .{ .renew_lease = .{ .lease_id = rest[1], .epoch = ep } };
    } else if (std.mem.eql(u8, op, "release") and rest.len >= 3) blk: {
        const ep = std.fmt.parseInt(u64, rest[2], 10) catch return 2;
        break :blk .{ .release_lease = .{ .lease_id = rest[1], .epoch = ep } };
    } else if (std.mem.eql(u8, op, "verify") and rest.len >= 3) blk: {
        const ep = std.fmt.parseInt(u64, rest[2], 10) catch return 2;
        break :blk .{ .verify_epoch = .{ .boot_id = rest[1], .epoch = ep } };
    } else {
        try print_usage(init.io);
        return 2;
    };

    const ua = try std.Io.net.UnixAddress.init(sock_path);
    var stream = try ua.connect(init.io);
    defer stream.close(init.io);

    var wbuf: [4096]u8 = undefined;
    var rbuf: [8192]u8 = undefined;
    var w = stream.writer(init.io, &wbuf);
    var r = stream.reader(init.io, &rbuf);

    const payload = try msg.encode_alloc(arena, req);
    try w.interface.writeAll(payload);
    try w.interface.writeByte('\n');
    try w.interface.flush();

    var line_aw: std.Io.Writer.Allocating = .init(arena);
    _ = r.interface.streamDelimiterLimit(&line_aw.writer, '\n', .limited(1 << 20)) catch {};
    const rep = std.mem.trim(u8, line_aw.written(), " \t\r\n");

    var obuf: [4096]u8 = undefined;
    var out_w = std.Io.File.stdout().writer(init.io, &obuf);
    try out_w.interface.print("{s}\n", .{rep});
    try out_w.interface.flush();

    if (std.mem.eql(u8, op, "verify")) {
        if (msg.decode_arena(msg.VerifyEpochReply, arena, rep)) |vr| {
            return if (vr.valid) 0 else 1;
        } else |_| return 1;
    }
    if (msg.decode_arena(msg.LeaseReply, arena, rep)) |lr| {
        if (std.mem.find(u8, rep, "\"lease_id\"") != null and !lr.ok) return 1;
    } else |_| {}
    return 0;
}

fn print_usage(io: std.Io) !void {
    var buf: [2048]u8 = undefined;
    var w = std.Io.File.stdout().writer(io, &buf);
    try w.interface.writeAll(
        \\protectorate-node - remote fleet node daemon
        \\
        \\usage:
        \\  protectorate-node join <ptjoin1:...> [--name <name>] [--home <dir>]
        \\  protectorate-node authorize [--home <dir>]
        \\  protectorate-node serve [--stdio] [--home <dir>]
        \\  protectorate-node supervise [--home <dir>]
        \\  protectorate-node install-service [--user] [--home <dir>]
        \\  protectorate-node uninstall [--user] [--home <dir>]
        \\  protectorate-node probation [--retry-rollback] [--home <dir>]
        \\  protectorate-node status [--home <dir>]
        \\  protectorate-node doctor | inventory | sample | presence | watch-presence [--interval <s>] [--limit <n>]
        \\  protectorate-node lease (list | acquire [...] | renew <id> <epoch> | release <id> <epoch> | verify <boot_id> <epoch> | unquarantine) [--home <dir>] [--sock <path>]
        \\
    );
    try w.interface.flush();
}
