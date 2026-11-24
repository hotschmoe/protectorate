const std = @import("std");
const builtin = @import("builtin");
const u_store = @import("../update/store.zig");

const WinSvc = if (builtin.os.tag == .windows) struct {
    const w = std.os.windows;

    const SERVICE_STATUS = extern struct {
        dwServiceType: w.DWORD = 0,
        dwCurrentState: w.DWORD = 0,
        dwControlsAccepted: w.DWORD = 0,
        dwWin32ExitCode: w.DWORD = 0,
        dwServiceSpecificExitCode: w.DWORD = 0,
        dwCheckPoint: w.DWORD = 0,
        dwWaitHint: w.DWORD = 0,
    };

    const SERVICE_STATUS_HANDLE = ?*anyopaque;
    const LPSERVICE_MAIN_FUNCTIONW = *const fn (dwArgc: w.DWORD, lpszArgv: [*]const [*:0]const u16) callconv(.winapi) void;
    const LPHANDLER_FUNCTION_EX = *const fn (
        dwControl: w.DWORD,
        dwEventType: w.DWORD,
        lpEventData: ?*anyopaque,
        lpContext: ?*anyopaque,
    ) callconv(.winapi) w.DWORD;

    const SERVICE_TABLE_ENTRYW = extern struct {
        lpServiceName: ?[*:0]const u16,
        lpServiceProc: ?LPSERVICE_MAIN_FUNCTIONW,
    };

    const SERVICE_WIN32_OWN_PROCESS: w.DWORD = 0x00000010;
    const SERVICE_STOPPED: w.DWORD = 0x00000001;
    const SERVICE_START_PENDING: w.DWORD = 0x00000002;
    const SERVICE_STOP_PENDING: w.DWORD = 0x00000003;
    const SERVICE_RUNNING: w.DWORD = 0x00000004;
    const SERVICE_ACCEPT_STOP: w.DWORD = 0x00000001;
    const SERVICE_ACCEPT_SHUTDOWN: w.DWORD = 0x00000004;
    const SERVICE_CONTROL_STOP: w.DWORD = 0x00000001;
    const SERVICE_CONTROL_INTERROGATE: w.DWORD = 0x00000004;
    const SERVICE_CONTROL_SHUTDOWN: w.DWORD = 0x00000005;
    const ERROR_FAILED_SERVICE_CONTROLLER_CONNECT: w.DWORD = 1063;
    const ERROR_CALL_NOT_IMPLEMENTED: w.DWORD = 120;

    extern "advapi32" fn StartServiceCtrlDispatcherW(lpServiceStartTable: [*]const SERVICE_TABLE_ENTRYW) callconv(.winapi) c_int;
    extern "advapi32" fn RegisterServiceCtrlHandlerExW(
        lpServiceName: [*:0]const u16,
        lpHandlerProc: LPHANDLER_FUNCTION_EX,
        lpContext: ?*anyopaque,
    ) callconv(.winapi) SERVICE_STATUS_HANDLE;
    extern "advapi32" fn SetServiceStatus(hServiceStatus: SERVICE_STATUS_HANDLE, lpServiceStatus: *SERVICE_STATUS) callconv(.winapi) c_int;
    extern "kernel32" fn GetLastError() callconv(.winapi) w.DWORD;
    extern "kernel32" fn TerminateProcess(hProcess: w.HANDLE, uExitCode: c_uint) callconv(.winapi) c_int;
} else struct {};

const WinSupCtx = struct {
    gpa: std.mem.Allocator,
    io: std.Io,
    env: *const std.process.Environ.Map,
    home_path: []const u8,
};

var sup_stop: std.atomic.Value(bool) = .init(false);
var active_child_id: std.atomic.Value(usize) = .init(0);
var win_ctx: ?WinSupCtx = null;
var win_status_handle: if (builtin.os.tag == .windows) WinSvc.SERVICE_STATUS_HANDLE else void =
    if (builtin.os.tag == .windows) null else {};

fn stop_active_child() void {
    const raw = active_child_id.load(.acquire);
    if (raw == 0) return;
    if (comptime builtin.os.tag == .windows) {
        _ = WinSvc.TerminateProcess(@ptrFromInt(raw), 0);
    } else if (comptime builtin.os.tag == .linux or builtin.os.tag == .macos) {
        std.posix.kill(@intCast(raw), .TERM) catch {};
    }
}

fn handle_sup_sig(_: std.posix.SIG) callconv(.c) void {
    sup_stop.store(true, .release);
    stop_active_child();
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

fn win_report_status(state: u32, controls: u32, exit_code: u32, wait_hint_ms: u32) void {
    if (comptime builtin.os.tag != .windows) return;
    if (win_status_handle) |h| {
        var st: WinSvc.SERVICE_STATUS = .{
            .dwServiceType = WinSvc.SERVICE_WIN32_OWN_PROCESS,
            .dwCurrentState = state,
            .dwControlsAccepted = controls,
            .dwWin32ExitCode = exit_code,
            .dwServiceSpecificExitCode = 0,
            .dwCheckPoint = 0,
            .dwWaitHint = wait_hint_ms,
        };
        _ = WinSvc.SetServiceStatus(h, &st);
    }
}

fn win_ctrl_handler(
    control: std.os.windows.DWORD,
    _: std.os.windows.DWORD,
    _: ?*anyopaque,
    _: ?*anyopaque,
) callconv(.winapi) std.os.windows.DWORD {
    switch (control) {
        WinSvc.SERVICE_CONTROL_STOP, WinSvc.SERVICE_CONTROL_SHUTDOWN => {
            sup_stop.store(true, .release);
            win_report_status(WinSvc.SERVICE_STOP_PENDING, 0, 0, 5000);
            stop_active_child();
            return 0;
        },
        WinSvc.SERVICE_CONTROL_INTERROGATE => return 0,
        else => return WinSvc.ERROR_CALL_NOT_IMPLEMENTED,
    }
}

fn win_service_main(
    _: std.os.windows.DWORD,
    _: [*]const [*:0]const u16,
) callconv(.winapi) void {
    const svc_name_w = std.unicode.utf8ToUtf16LeStringLiteral("ProtectorateNode");
    win_status_handle = WinSvc.RegisterServiceCtrlHandlerExW(svc_name_w, win_ctrl_handler, null);
    if (win_status_handle == null) return;
    win_report_status(WinSvc.SERVICE_RUNNING, WinSvc.SERVICE_ACCEPT_STOP | WinSvc.SERVICE_ACCEPT_SHUTDOWN, 0, 0);
    if (win_ctx) |ctx| {
        supervise_loop(ctx.gpa, ctx.io, ctx.env, ctx.home_path);
    }
    win_report_status(WinSvc.SERVICE_STOPPED, 0, 0, 0);
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

fn supervise_loop(
    gpa: std.mem.Allocator,
    io: std.Io,
    env: *const std.process.Environ.Map,
    home_path: []const u8,
) void {
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
        if (child.id) |cid| {
            const id_val: usize = if (comptime builtin.os.tag == .windows)
                @intFromPtr(cid)
            else
                @intCast(cid);
            active_child_id.store(id_val, .release);
        }
        const start_ms = std.Io.Clock.awake.now(io).toMilliseconds();
        _ = child.wait(io) catch {};
        active_child_id.store(0, .release);
        if (sup_stop.load(.acquire)) break;
        const ran_ms = std.Io.Clock.awake.now(io).toMilliseconds() - start_ms;
        backoff_ms = if (ran_ms > 30_000) 500 else @min(backoff_ms * 2, 10_000);
        io.sleep(.fromMilliseconds(backoff_ms), .awake) catch break;
    }
}

pub fn cmd_supervise(
    gpa: std.mem.Allocator,
    io: std.Io,
    env: *const std.process.Environ.Map,
    home_path: []const u8,
) !u8 {
    install_sup_signals();
    if (comptime builtin.os.tag == .windows) {
        win_ctx = .{ .gpa = gpa, .io = io, .env = env, .home_path = home_path };
        defer win_ctx = null;
        const svc_name_w = std.unicode.utf8ToUtf16LeStringLiteral("ProtectorateNode");
        const table = [_]WinSvc.SERVICE_TABLE_ENTRYW{
            .{ .lpServiceName = svc_name_w, .lpServiceProc = win_service_main },
            .{ .lpServiceName = null, .lpServiceProc = null },
        };
        if (WinSvc.StartServiceCtrlDispatcherW(&table) != 0) {
            return 0;
        }
        const err_code = WinSvc.GetLastError();
        if (err_code != WinSvc.ERROR_FAILED_SERVICE_CONTROLLER_CONNECT) {
            return 1;
        }
    }
    supervise_loop(gpa, io, env, home_path);
    return 0;
}

pub fn cmd_install_service(
    gpa: std.mem.Allocator,
    io: std.Io,
    env: *const std.process.Environ.Map,
    home_path: []const u8,
    user_mode: bool,
    service_name_opt: ?[]const u8,
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

        const base = service_name_opt orelse "protectorate-node";
        const unit_name = if (std.mem.endsWith(u8, base, ".service"))
            base
        else
            try std.fmt.allocPrint(a, "{s}.service", .{base});
        const unit_path = try std.fs.path.join(a, &.{ unit_dir, unit_name });
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
            if (!run_checked(a, io, &.{ "systemctl", "--user", "daemon-reload" })) return 1;
            if (!run_checked(a, io, &.{ "systemctl", "--user", "enable", "--now", unit_name })) return 1;
        } else {
            if (!run_checked(a, io, &.{ "systemctl", "daemon-reload" })) return 1;
            if (!run_checked(a, io, &.{ "systemctl", "enable", "--now", unit_name })) return 1;
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
        const label = service_name_opt orelse "com.protectorate.node";
        const plist_file = try std.fmt.allocPrint(a, "{s}.plist", .{label});
        const plist_path = try std.fs.path.join(a, &.{ agents_dir, plist_file });
        const plist = try std.fmt.allocPrint(
            a,
            \\<?xml version="1.0" encoding="UTF-8"?>
            \\<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
            \\<plist version="1.0"><dict>
            \\  <key>Label</key><string>{s}</string>
            \\  <key>ProgramArguments</key><array>
            \\    <string>{s}</string><string>supervise</string><string>--home</string><string>{s}</string>
            \\  </array>
            \\  <key>KeepAlive</key><true/>
            \\  <key>RunAtLoad</key><true/>
            \\</dict></plist>
            \\
        ,
            .{ label, exe, home_path },
        );
        try std.Io.Dir.cwd().writeFile(io, .{ .sub_path = plist_path, .data = plist });
        _ = run_quiet(a, io, &.{ "launchctl", "load", "-w", plist_path });
        try write_manifest(a, io, home_path, plist_path, true);
        try w.interface.print("installed launchd agent: {s}\n", .{plist_path});
        try w.interface.flush();
        return 0;
    } else if (comptime builtin.os.tag == .windows) {
        const svc_name = service_name_opt orelse "ProtectorateNode";
        const bin_val = try std.fmt.allocPrint(a, "\"{s}\" supervise --home \"{s}\"", .{ exe, home_path });
        if (!run_checked(a, io, &.{ "sc.exe", "create", svc_name, "binPath=", bin_val, "start=", "auto" })) {
            return 1;
        }
        _ = run_quiet(a, io, &.{ "sc.exe", "failure", svc_name, "reset=", "86400", "actions=", "restart/5000/restart/5000/restart/5000" });
        if (!run_checked(a, io, &.{ "sc.exe", "start", svc_name })) {
            return 1;
        }
        try write_manifest(a, io, home_path, svc_name, false);
        try w.interface.print("installed Windows service {s}\n", .{svc_name});
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
    service_name_opt: ?[]const u8,
) !u8 {
    var arena_st = std.heap.ArenaAllocator.init(gpa);
    defer arena_st.deinit();
    const a = arena_st.allocator();

    var obuf: [1024]u8 = undefined;
    var w = std.Io.File.stdout().writer(io, &obuf);
    const man_path = try std.fs.path.join(a, &.{ home_path, "install-manifest.json" });

    if (comptime builtin.os.tag == .linux) {
        const is_user = user_mode or (std.os.linux.getuid() != 0);
        const base = service_name_opt orelse "protectorate-node";
        const unit_name = if (std.mem.endsWith(u8, base, ".service"))
            base
        else
            try std.fmt.allocPrint(a, "{s}.service", .{base});
        if (is_user) {
            _ = run_quiet(a, io, &.{ "systemctl", "--user", "disable", "--now", unit_name });
        } else {
            _ = run_quiet(a, io, &.{ "systemctl", "disable", "--now", unit_name });
        }
        if (is_user) {
            if (env.get("HOME")) |h| {
                const up = try std.fs.path.join(a, &.{ h, ".config/systemd/user", unit_name });
                std.Io.Dir.cwd().deleteFile(io, up) catch {};
            }
        } else {
            const sp = try std.fs.path.join(a, &.{ "/etc/systemd/system", unit_name });
            std.Io.Dir.cwd().deleteFile(io, sp) catch {};
        }
        std.Io.Dir.cwd().deleteFile(io, man_path) catch {};
        try w.interface.print("uninstalled {s} systemd service\n", .{unit_name});
        try w.interface.flush();
        return 0;
    } else if (comptime builtin.os.tag == .macos) {
        if (env.get("HOME")) |h| {
            const label = service_name_opt orelse "com.protectorate.node";
            const plist_file = try std.fmt.allocPrint(a, "{s}.plist", .{label});
            const plist_path = try std.fs.path.join(a, &.{ h, "Library/LaunchAgents", plist_file });
            _ = run_quiet(a, io, &.{ "launchctl", "unload", "-w", plist_path });
            std.Io.Dir.cwd().deleteFile(io, plist_path) catch {};
        }
        std.Io.Dir.cwd().deleteFile(io, man_path) catch {};
        try w.interface.writeAll("uninstalled protectorate-node launchd agent\n");
        try w.interface.flush();
        return 0;
    } else if (comptime builtin.os.tag == .windows) {
        const svc_name = service_name_opt orelse "ProtectorateNode";
        _ = run_quiet(a, io, &.{ "sc.exe", "stop", svc_name });
        _ = run_quiet(a, io, &.{ "sc.exe", "delete", svc_name });
        std.Io.Dir.cwd().deleteFile(io, man_path) catch {};
        try w.interface.print("uninstalled Windows service {s}\n", .{svc_name});
        try w.interface.flush();
        return 0;
    }
    return 1;
}

fn resolve_node_bin(gpa: std.mem.Allocator, io: std.Io, home_path: []const u8) ![]u8 {
    const exe_name = if (comptime builtin.os.tag == .windows) "protectorate-node.exe" else "protectorate-node";
    const marker_path = try std.fs.path.join(gpa, &.{ home_path, "releases", "current.txt" });
    defer gpa.free(marker_path);
    if (std.Io.Dir.cwd().readFileAlloc(io, marker_path, gpa, .limited(4096))) |raw| {
        defer gpa.free(raw);
        const tag = std.mem.trim(u8, raw, " \t\r\n");
        if (tag.len > 0) {
            const tagged = try std.fs.path.join(gpa, &.{ home_path, "releases", tag, exe_name });
            if (std.Io.Dir.cwd().statFile(io, tagged, .{})) |_| {
                return tagged;
            } else |_| {
                gpa.free(tagged);
            }
        }
    } else |_| {}

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

fn run_checked(arena: std.mem.Allocator, io: std.Io, argv: []const []const u8) bool {
    const res = std.process.run(arena, io, .{
        .argv = argv,
        .stdout_limit = .limited(16384),
        .stderr_limit = .limited(16384),
    }) catch |err| {
        var eb: [256]u8 = undefined;
        var ew = std.Io.File.stderr().writer(io, &eb);
        ew.interface.print("command {s} failed: {s}\n", .{ argv[0], @errorName(err) }) catch {};
        ew.interface.flush() catch {};
        return false;
    };
    if (res.term.success()) return true;
    var eb: [512]u8 = undefined;
    var ew = std.Io.File.stderr().writer(io, &eb);
    ew.interface.print("command {s} failed:\n{s}\n{s}\n", .{ argv[0], res.stdout, res.stderr }) catch {};
    ew.interface.flush() catch {};
    return false;
}
