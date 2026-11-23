const std = @import("std");
const s_exec = @import("sandbox/exec.zig");
const version = @import("version.zig");
const doctor = @import("doctor/doctor.zig");
const serve = @import("node/serve.zig");

pub fn main(init: std.process.Init) !u8 {
    var arena_st = std.heap.ArenaAllocator.init(init.gpa);
    defer arena_st.deinit();
    const arena = arena_st.allocator();

    const args = try init.minimal.args.toSlice(arena);
    if (args.len >= 2 and std.mem.eql(u8, args[1], "__guard")) {
        return try s_exec.run_guard_trampoline(init, args[2..]);
    }
    var home_override: ?[]const u8 = null;
    var stdio_mode = false;
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
        } else if (std.mem.eql(u8, arg, "--home") and i + 1 < args.len) {
            i += 1;
            home_override = args[i];
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
        return serve.cmd_authorize(init.gpa, init.io, home_path);
    } else if (std.mem.eql(u8, sub, "serve")) {
        return serve.cmd_serve(init.gpa, init.io, home_path, stdio_mode);
    } else if (std.mem.eql(u8, sub, "status")) {
        return serve.cmd_status(init.gpa, init.io, home_path);
    } else if (std.mem.eql(u8, sub, "doctor")) {
        const rep = try doctor.run_checks(arena, init.io, init.environ_map.get("HOME"));
        const json = try std.json.Stringify.valueAlloc(arena, rep, .{ .whitespace = .indent_2 });
        var w = std.Io.File.stdout().writer(init.io, &.{});
        try w.interface.writeAll(json);
        try w.interface.writeByte('\n');
        try w.interface.flush();
        return 0;
    }

    try print_usage(init.io);
    return 2;
}

fn print_usage(io: std.Io) !void {
    var buf: [512]u8 = undefined;
    var w = std.Io.File.stdout().writer(io, &buf);
    try w.interface.writeAll(
        \\protectorate-node - remote fleet node daemon
        \\
        \\usage:
        \\  protectorate-node join <ptjoin1:...> [--name <name>] [--home <dir>]
        \\  protectorate-node authorize [--home <dir>]
        \\  protectorate-node serve [--stdio] [--home <dir>]
        \\  protectorate-node status [--home <dir>]
        \\  protectorate-node doctor
        \\
    );
    try w.interface.flush();
}
