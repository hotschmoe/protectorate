const std = @import("std");
const builtin = @import("builtin");
const u_store = @import("../update/store.zig");

var sup_stop: std.atomic.Value(bool) = .init(false);

fn handle_sup_sig(_: std.posix.SIG) callconv(.c) void {
    sup_stop.store(true, .release);
}

fn install_sup_signals() void {
    if (comptime builtin.os.tag == .linux or builtin.os.tag == .macos) {
        const act: std.posix.Sigaction = .{
            .handler = .{ .handler = handle_sup_sig },
            .mask = std.posix.sigemptyset(),
            .flags = 0,
        };
        std.posix.sigaction(.INT, &act, null);
        std.posix.sigaction(.TERM, &act, null);
    }
}

pub fn cmd_probation(
    gpa: std.mem.Allocator,
    io: std.Io,
    home_path: []const u8,
    retry_rollback: bool,
) !u8 {
    var arena_st = std.heap.ArenaAllocator.init(gpa);
    defer arena_st.deinit();
    const a = arena_st.allocator();

    const rel_dir_path = try std.fs.path.join(a, &.{ home_path, "releases" });
    const prob_path = try std.fs.path.join(a, &.{ rel_dir_path, "probation.json" });

    var obuf: [1024]u8 = undefined;
    var w = std.Io.File.stdout().writer(io, &obuf);

    const raw = std.Io.Dir.cwd().readFileAlloc(io, prob_path, a, .limited(64 * 1024)) catch {
        try w.interface.writeAll("no probation marker present (ok)\n");
        try w.interface.flush();
        return 0;
    };

    const val = std.json.parseFromSliceLeaky(std.json.Value, a, raw, .{}) catch {
        try w.interface.writeAll("unparseable probation marker\n");
        try w.interface.flush();
        return 1;
    };
    const active = if (val == .object and val.object.get("active") != null and val.object.get("active").? == .string)
        val.object.get("active").?.string
    else
        "-";
    const healthy = if (val == .object and val.object.get("healthy") != null and val.object.get("healthy").? == .bool)
        val.object.get("healthy").?.bool
    else
        true;

    if (!healthy or retry_rollback) {
        if (u_store.rollback(gpa, io, rel_dir_path)) |target| {
            defer gpa.free(target);
            try w.interface.print("rolled back from {s} to {s}\n", .{ active, target });
            try w.interface.flush();
            return 0;
        } else |_| {}
    }
    try w.interface.print("active={s} healthy={}\n", .{ active, healthy });
    try w.interface.flush();
    return 0;
}

pub fn cmd_supervise(
    gpa: std.mem.Allocator,
    io: std.Io,
    env: *const std.process.Environ.Map,
    home_path: []const u8,
) !u8 {
    install_sup_signals();
    var backoff_ms: i64 = 500;
    while (!sup_stop.load(.acquire)) {
        _ = cmd_probation(gpa, io, home_path, false) catch 1;
        const exe = resolve_node_bin(gpa, io, home_path) catch break;
        defer gpa.free(exe);

        var child = std.process.spawn(io, .{
            .argv = &.{ exe, "serve", "--home", home_path },
            .environ_map = env,
        }) catch {
            io.sleep(.fromMilliseconds(backoff_ms), .awake) catch break;
            backoff_ms = @min(backoff_ms * 2, 10_000);
            continue;
        };
        const start_ms = std.Io.Clock.awake.now(io).toMilliseconds();
        _ = child.wait(io) catch {};
        if (sup_stop.load(.acquire)) break;
        const ran_ms = std.Io.Clock.awake.now(io).toMilliseconds() - start_ms;
        backoff_ms = if (ran_ms > 30_000) 500 else @min(backoff_ms * 2, 10_000);
        io.sleep(.fromMilliseconds(backoff_ms), .awake) catch break;
    }
    return 0;
}

pub fn cmd_install_service(
    gpa: std.mem.Allocator,
    io: std.Io,
    env: *const std.process.Environ.Map,
    home_path: []const u8,
    user_mode: bool,
) !u8 {
    var arena_st = std.heap.ArenaAllocator.init(gpa);
    defer arena_st.deinit();
    const a = arena_st.allocator();

    const exe = try std.process.executablePathAlloc(io, a);
    var obuf: [1024]u8 = undefined;
    var w = std.Io.File.stdout().writer(io, &obuf);

    if (comptime builtin.os.tag == .linux) {
        const is_user = user_mode or (std.os.linux.getuid() != 0);
        const unit_dir = if (is_user) blk: {
            const h = env.get("HOME") orelse return error.MissingHome;
            break :blk try std.fs.path.join(a, &.{ h, ".config/systemd/user" });
        } else try a.dupe(u8, "/etc/systemd/system");

        var d = try std.Io.Dir.cwd().createDirPathOpen(io, unit_dir, .{});
        d.close(io);

        const unit_path = try std.fs.path.join(a, &.{ unit_dir, "protectorate-node.service" });
        const content = try std.fmt.allocPrint(
            a,
            \\[Unit]
            \\Description=Protectorate Node Daemon
            \\After=network-online.target
            \\
            \\[Service]
            \\ExecStart={s} supervise --home {s}
            \\Restart=always
            \\RestartSec=2
            \\
            \\[Install]
            \\WantedBy={s}
            \\
        ,
            .{ exe, home_path, if (is_user) "default.target" else "multi-user.target" },
        );
        try std.Io.Dir.cwd().writeFile(io, .{ .sub_path = unit_path, .data = content });

        if (is_user) {
            _ = run_quiet(a, io, &.{ "systemctl", "--user", "daemon-reload" });
            _ = run_quiet(a, io, &.{ "systemctl", "--user", "enable", "--now", "protectorate-node.service" });
        } else {
            _ = run_quiet(a, io, &.{ "systemctl", "daemon-reload" });
            _ = run_quiet(a, io, &.{ "systemctl", "enable", "--now", "protectorate-node.service" });
        }

        try write_manifest(a, io, home_path, unit_path, is_user);
        try w.interface.print("installed systemd unit: {s}\n", .{unit_path});
        try w.interface.flush();
        return 0;
    } else if (comptime builtin.os.tag == .macos) {
        const h = env.get("HOME") orelse return error.MissingHome;
        const agents_dir = try std.fs.path.join(a, &.{ h, "Library/LaunchAgents" });
        var d = try std.Io.Dir.cwd().createDirPathOpen(io, agents_dir, .{});
        d.close(io);
        const plist_path = try std.fs.path.join(a, &.{ agents_dir, "com.protectorate.node.plist" });
        const plist = try std.fmt.allocPrint(
            a,
            \\<?xml version="1.0" encoding="UTF-8"?>
            \\<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
            \\<plist version="1.0"><dict>
            \\  <key>Label</key><string>com.protectorate.node</string>
            \\  <key>ProgramArguments</key><array>
            \\    <string>{s}</string><string>supervise</string><string>--home</string><string>{s}</string>
            \\  </array>
            \\  <key>KeepAlive</key><true/>
            \\  <key>RunAtLoad</key><true/>
            \\</dict></plist>
            \\
        ,
            .{ exe, home_path },
        );
        try std.Io.Dir.cwd().writeFile(io, .{ .sub_path = plist_path, .data = plist });
        _ = run_quiet(a, io, &.{ "launchctl", "load", "-w", plist_path });
        try write_manifest(a, io, home_path, plist_path, true);
        try w.interface.print("installed launchd agent: {s}\n", .{plist_path});
        try w.interface.flush();
        return 0;
    } else if (comptime builtin.os.tag == .windows) {
        const bin_arg = try std.fmt.allocPrint(a, "binPath= \"\\\"{s}\\\" supervise --home \\\"{s}\\\"\"", .{ exe, home_path });
        _ = run_quiet(a, io, &.{ "sc.exe", "create", "ProtectorateNode", bin_arg, "start=", "auto" });
        _ = run_quiet(a, io, &.{ "sc.exe", "start", "ProtectorateNode" });
        try write_manifest(a, io, home_path, "ProtectorateNode", false);
        try w.interface.writeAll("installed Windows service ProtectorateNode\n");
        try w.interface.flush();
        return 0;
    }
    return 1;
}

pub fn cmd_uninstall(
    gpa: std.mem.Allocator,
    io: std.Io,
    env: *const std.process.Environ.Map,
    home_path: []const u8,
    user_mode: bool,
) !u8 {
    var arena_st = std.heap.ArenaAllocator.init(gpa);
    defer arena_st.deinit();
    const a = arena_st.allocator();

    var obuf: [1024]u8 = undefined;
    var w = std.Io.File.stdout().writer(io, &obuf);
    const man_path = try std.fs.path.join(a, &.{ home_path, "install-manifest.json" });

    if (comptime builtin.os.tag == .linux) {
        const is_user = user_mode or (std.os.linux.getuid() != 0);
        if (is_user) {
            _ = run_quiet(a, io, &.{ "systemctl", "--user", "disable", "--now", "protectorate-node.service" });
        } else {
            _ = run_quiet(a, io, &.{ "systemctl", "disable", "--now", "protectorate-node.service" });
        }
        if (is_user) {
            if (env.get("HOME")) |h| {
                const up = try std.fs.path.join(a, &.{ h, ".config/systemd/user/protectorate-node.service" });
                std.Io.Dir.cwd().deleteFile(io, up) catch {};
            }
        } else {
            std.Io.Dir.cwd().deleteFile(io, "/etc/systemd/system/protectorate-node.service") catch {};
        }
        std.Io.Dir.cwd().deleteFile(io, man_path) catch {};
        try w.interface.writeAll("uninstalled protectorate-node systemd service\n");
        try w.interface.flush();
        return 0;
    } else if (comptime builtin.os.tag == .macos) {
        if (env.get("HOME")) |h| {
            const plist_path = try std.fs.path.join(a, &.{ h, "Library/LaunchAgents/com.protectorate.node.plist" });
            _ = run_quiet(a, io, &.{ "launchctl", "unload", "-w", plist_path });
            std.Io.Dir.cwd().deleteFile(io, plist_path) catch {};
        }
        std.Io.Dir.cwd().deleteFile(io, man_path) catch {};
        try w.interface.writeAll("uninstalled protectorate-node launchd agent\n");
        try w.interface.flush();
        return 0;
    } else if (comptime builtin.os.tag == .windows) {
        _ = run_quiet(a, io, &.{ "sc.exe", "stop", "ProtectorateNode" });
        _ = run_quiet(a, io, &.{ "sc.exe", "delete", "ProtectorateNode" });
        std.Io.Dir.cwd().deleteFile(io, man_path) catch {};
        try w.interface.writeAll("uninstalled Windows service ProtectorateNode\n");
        try w.interface.flush();
        return 0;
    }
    return 1;
}

fn resolve_node_bin(gpa: std.mem.Allocator, io: std.Io, home_path: []const u8) ![]u8 {
    const exe_name = if (comptime builtin.os.tag == .windows) "protectorate-node.exe" else "protectorate-node";
    const cand = try std.fs.path.join(gpa, &.{ home_path, "releases", "current", exe_name });
    if (std.Io.Dir.cwd().statFile(io, cand, .{})) |_| {
        return cand;
    } else |_| {
        gpa.free(cand);
    }
    const self_z = try std.process.executablePathAlloc(io, gpa);
    defer gpa.free(self_z);
    return gpa.dupe(u8, self_z);
}

fn write_manifest(
    arena: std.mem.Allocator,
    io: std.Io,
    home_path: []const u8,
    unit_path: []const u8,
    user_mode: bool,
) !void {
    var d = try std.Io.Dir.cwd().createDirPathOpen(io, home_path, .{});
    d.close(io);
    const man_path = try std.fs.path.join(arena, &.{ home_path, "install-manifest.json" });
    const json = try std.json.Stringify.valueAlloc(arena, .{
        .unit_path = unit_path,
        .user_mode = user_mode,
    }, .{ .whitespace = .indent_2 });
    try std.Io.Dir.cwd().writeFile(io, .{ .sub_path = man_path, .data = json });
}

fn run_quiet(arena: std.mem.Allocator, io: std.Io, argv: []const []const u8) bool {
    const res = std.process.run(arena, io, .{
        .argv = argv,
        .stdout_limit = .limited(8192),
        .stderr_limit = .limited(8192),
    }) catch return false;
    return res.term.success();
}
