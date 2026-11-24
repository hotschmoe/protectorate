const std = @import("std");
const builtin = @import("builtin");
const tiger = @import("../tiger.zig");
const version = @import("../version.zig");
const msg = @import("../net/msg.zig");

var prev_cpu_init: std.atomic.Value(bool) = .init(false);
var prev_cpu_busy: std.atomic.Value(u64) = .init(0);
var prev_cpu_total: std.atomic.Value(u64) = .init(0);

const WinSys = if (builtin.os.tag == .windows) struct {
    const w = std.os.windows;
    const KEY_READ: w.DWORD = 0x20019;
    const REG_SZ: w.DWORD = 1;
    const REG_DWORD: w.DWORD = 4;
    const REG_QWORD: w.DWORD = 11;
    const DRIVE_FIXED: u32 = 3;
    const DRIVE_REMOTE: u32 = 4;
    const hklm: w.HANDLE = @ptrFromInt(@as(usize, @bitCast(@as(isize, @as(i32, @bitCast(@as(u32, 0x80000002)))))));

    const MEMORYSTATUSEX = extern struct {
        dwLength: w.DWORD = @sizeOf(MEMORYSTATUSEX),
        dwMemoryLoad: w.DWORD = 0,
        ullTotalPhys: u64 = 0,
        ullAvailPhys: u64 = 0,
        ullTotalPageFile: u64 = 0,
        ullAvailPageFile: u64 = 0,
        ullTotalVirtual: u64 = 0,
        ullAvailVirtual: u64 = 0,
        ullAvailExtendedVirtual: u64 = 0,
    };

    const FILETIME = extern struct {
        dwLowDateTime: w.DWORD = 0,
        dwHighDateTime: w.DWORD = 0,
        fn to_u64(self: FILETIME) u64 {
            return (@as(u64, self.dwHighDateTime) << 32) | self.dwLowDateTime;
        }
    };

    const LASTINPUTINFO = extern struct {
        cbSize: w.DWORD = @sizeOf(LASTINPUTINFO),
        dwTime: w.DWORD = 0,
    };

    const WTS_SESSION_INFOW = extern struct {
        SessionId: w.DWORD,
        pWinStationName: ?[*:0]const u16,
        State: i32,
    };

    const PROCESS_INFORMATION = extern struct {
        hProcess: w.HANDLE,
        hThread: w.HANDLE,
        dwProcessId: w.DWORD,
        dwThreadId: w.DWORD,
    };

    extern "kernel32" fn GetComputerNameW(lpBuffer: [*]u16, nSize: *w.DWORD) callconv(.winapi) c_int;
    extern "kernel32" fn GlobalMemoryStatusEx(lpBuffer: *MEMORYSTATUSEX) callconv(.winapi) c_int;
    extern "kernel32" fn GetSystemTimes(lpIdleTime: *FILETIME, lpKernelTime: *FILETIME, lpUserTime: *FILETIME) callconv(.winapi) c_int;
    extern "kernel32" fn GetLogicalDriveStringsW(nBufferLength: w.DWORD, lpBuffer: [*]u16) callconv(.winapi) w.DWORD;
    extern "kernel32" fn GetDriveTypeW(lpRootPathName: [*:0]const u16) callconv(.winapi) u32;
    extern "kernel32" fn GetDiskFreeSpaceExW(
        lpDirectoryName: [*:0]const u16,
        lpFreeBytesAvailableToCaller: *u64,
        lpTotalNumberOfBytes: *u64,
        lpTotalNumberOfFreeBytes: *u64,
    ) callconv(.winapi) c_int;
    extern "kernel32" fn GetTickCount() callconv(.winapi) w.DWORD;
    extern "kernel32" fn WTSGetActiveConsoleSessionId() callconv(.winapi) w.DWORD;
    extern "kernel32" fn ProcessIdToSessionId(dwProcessId: w.DWORD, pSessionId: *w.DWORD) callconv(.winapi) c_int;
    extern "kernel32" fn GetCurrentProcessId() callconv(.winapi) w.DWORD;
    extern "kernel32" fn LoadLibraryA(lpLibFileName: [*:0]const u8) callconv(.winapi) ?w.HANDLE;
    extern "kernel32" fn GetProcAddress(hModule: w.HANDLE, lpProcName: [*:0]const u8) callconv(.winapi) ?*const anyopaque;
    extern "kernel32" fn CreatePipe(
        hReadPipe: *w.HANDLE,
        hWritePipe: *w.HANDLE,
        lpPipeAttributes: ?*w.SECURITY_ATTRIBUTES,
        nSize: w.DWORD,
    ) callconv(.winapi) c_int;
    extern "kernel32" fn SetHandleInformation(hObject: w.HANDLE, dwMask: w.DWORD, dwFlags: w.DWORD) callconv(.winapi) c_int;
    extern "kernel32" fn ReadFile(
        hFile: w.HANDLE,
        lpBuffer: [*]u8,
        nNumberOfBytesToRead: w.DWORD,
        lpNumberOfBytesRead: ?*w.DWORD,
        lpOverlapped: ?*anyopaque,
    ) callconv(.winapi) c_int;
    extern "kernel32" fn WaitForSingleObject(hHandle: w.HANDLE, dwMilliseconds: w.DWORD) callconv(.winapi) w.DWORD;
    extern "kernel32" fn TerminateProcess(hProcess: w.HANDLE, uExitCode: c_uint) callconv(.winapi) c_int;

    extern "advapi32" fn RegOpenKeyExW(
        hKey: w.HANDLE,
        lpSubKey: [*:0]const u16,
        ulOptions: w.DWORD,
        samDesired: w.DWORD,
        phkResult: *w.HANDLE,
    ) callconv(.winapi) i32;
    extern "advapi32" fn RegQueryValueExW(
        hKey: w.HANDLE,
        lpValueName: [*:0]const u16,
        lpReserved: ?*w.DWORD,
        lpType: ?*w.DWORD,
        lpData: ?[*]u8,
        lpcbData: *w.DWORD,
    ) callconv(.winapi) i32;
    extern "advapi32" fn RegCloseKey(hKey: w.HANDLE) callconv(.winapi) i32;
    extern "advapi32" fn CreateProcessAsUserW(
        hToken: w.HANDLE,
        lpApplicationName: ?[*:0]const u16,
        lpCommandLine: ?[*:0]u16,
        lpProcessAttributes: ?*w.SECURITY_ATTRIBUTES,
        lpThreadAttributes: ?*w.SECURITY_ATTRIBUTES,
        bInheritHandles: w.BOOL,
        dwCreationFlags: w.DWORD,
        lpEnvironment: ?*const anyopaque,
        lpCurrentDirectory: ?[*:0]const u16,
        lpStartupInfo: *w.STARTUPINFOW,
        lpProcessInformation: *PROCESS_INFORMATION,
    ) callconv(.winapi) c_int;

    extern "user32" fn GetLastInputInfo(plii: *LASTINPUTINFO) callconv(.winapi) c_int;

    const FnWTSEnumerateSessionsW = *const fn (?w.HANDLE, w.DWORD, w.DWORD, *?[*]const WTS_SESSION_INFOW, *w.DWORD) callconv(.winapi) c_int;
    const FnWTSQuerySessionInformationW = *const fn (?w.HANDLE, w.DWORD, c_int, *?[*]const u8, *w.DWORD) callconv(.winapi) c_int;
    const FnWTSFreeMemory = *const fn (?*const anyopaque) callconv(.winapi) void;
    const FnWTSQueryUserToken = *const fn (w.DWORD, *?w.HANDLE) callconv(.winapi) c_int;

    const WtsApi = struct {
        enum_sessions: FnWTSEnumerateSessionsW,
        query_session: FnWTSQuerySessionInformationW,
        free_mem: FnWTSFreeMemory,
        query_user_token: FnWTSQueryUserToken,
    };

    fn load_wts() ?WtsApi {
        const mod = LoadLibraryA("wtsapi32.dll") orelse return null;
        const p_enum = GetProcAddress(mod, "WTSEnumerateSessionsW") orelse return null;
        const p_q = GetProcAddress(mod, "WTSQuerySessionInformationW") orelse return null;
        const p_free = GetProcAddress(mod, "WTSFreeMemory") orelse return null;
        const p_tok = GetProcAddress(mod, "WTSQueryUserToken") orelse return null;
        return .{
            .enum_sessions = @ptrCast(@alignCast(p_enum)),
            .query_session = @ptrCast(@alignCast(p_q)),
            .free_mem = @ptrCast(@alignCast(p_free)),
            .query_user_token = @ptrCast(@alignCast(p_tok)),
        };
    }
} else struct {};

const LinuxStatfs = extern struct {
    f_type: isize,
    f_bsize: isize,
    f_blocks: u64,
    f_bfree: u64,
    f_bavail: u64,
    f_files: u64,
    f_ffree: u64,
    f_fsid: [2]i32,
    f_namelen: isize,
    f_frsize: isize,
    f_flags: isize,
    f_spare: [4]isize,
};

pub fn default_hostname(buf: *[256]u8) []const u8 {
    if (comptime builtin.os.tag == .windows) {
        var wbuf: [256]u16 = undefined;
        var sz: u32 = wbuf.len;
        if (WinSys.GetComputerNameW(&wbuf, &sz) != 0 and sz > 0) {
            const n = std.unicode.utf16LeToUtf8(buf, wbuf[0..@min(sz, wbuf.len)]) catch 0;
            if (n > 0) return buf[0..n];
        }
        return "windows-node";
    } else {
        var pbuf: [std.posix.HOST_NAME_MAX]u8 = undefined;
        if (std.posix.gethostname(&pbuf)) |hn| {
            if (hn.len > 0) {
                const n = @min(hn.len, buf.len);
                @memcpy(buf[0..n], hn[0..n]);
                return buf[0..n];
            }
        } else |_| {}
    }
    return switch (builtin.os.tag) {
        .macos => "macos-node",
        else => "linux-node",
    };
}

pub fn collect_inventory(arena: std.mem.Allocator, io: std.Io, _: []const u8) !msg.Inventory {
    var hn_buf: [256]u8 = undefined;
    const hostname = try arena.dupe(u8, default_hostname(&hn_buf));
    const os_ver = try read_os_version(arena, io);
    const cpu_model = try read_cpu_model(arena, io);
    const cpu_count: u32 = @intCast(@max(@as(usize, 1), std.Thread.getCpuCount() catch 1));
    const ram_bytes = read_total_ram(io);

    var toolchains: std.ArrayList(msg.Toolchain) = .empty;
    const tool_names = [_][]const u8{ "git", "zig", "pi", "claude", "codex", "grok", "agy", "bwrap", "flutter" };
    for (tool_names) |tn| {
        if (probe_tool_version(arena, io, tn)) |ver| {
            try toolchains.append(arena, .{ .name = tn, .version = ver });
        }
    }

    const gpus = collect_gpus(arena, io);
    const disks = collect_disks(arena, io);
    const now_ms: i64 = std.Io.Clock.real.now(io).toMilliseconds();
    return .{
        .hostname = hostname,
        .os = @tagName(builtin.os.tag),
        .os_version = os_ver,
        .arch = @tagName(builtin.cpu.arch),
        .cpu_model = cpu_model,
        .cpu_count = cpu_count,
        .ram_bytes = ram_bytes,
        .gpus = gpus,
        .disks = disks,
        .toolchains = toolchains.items,
        .collected_at_ms = now_ms,
    };
}

pub fn collect_sample(io: std.Io) msg.Sample {
    const now_ms: i64 = std.Io.Clock.real.now(io).toMilliseconds();
    var cpu_pct: ?f64 = null;
    var load1: ?f64 = null;
    var mem_avail: ?u64 = null;
    var swap_used: ?u64 = null;

    if (comptime builtin.os.tag == .linux) {
        var sbuf: [2048]u8 = undefined;
        if (read_small_file(io, "/proc/stat", &sbuf)) |stxt| {
            if (parse_proc_stat(stxt)) |cur| {
                cpu_pct = update_cpu_pct(cur.busy, cur.total);
            }
        }
        var lbuf: [128]u8 = undefined;
        if (read_small_file(io, "/proc/loadavg", &lbuf)) |ltxt| {
            var it = std.mem.tokenizeAny(u8, ltxt, " \t\r\n");
            if (it.next()) |first| {
                load1 = std.fmt.parseFloat(f64, first) catch null;
            }
        }
        var mbuf: [4096]u8 = undefined;
        if (read_small_file(io, "/proc/meminfo", &mbuf)) |mtxt| {
            var swap_total: ?u64 = null;
            var swap_free: ?u64 = null;
            var lines = std.mem.splitScalar(u8, mtxt, '\n');
            while (lines.next()) |line| {
                if (std.mem.startsWith(u8, line, "MemAvailable:")) {
                    mem_avail = parse_kb_line(line["MemAvailable:".len..]);
                } else if (std.mem.startsWith(u8, line, "SwapTotal:")) {
                    swap_total = parse_kb_line(line["SwapTotal:".len..]);
                } else if (std.mem.startsWith(u8, line, "SwapFree:")) {
                    swap_free = parse_kb_line(line["SwapFree:".len..]);
                }
            }
            if (swap_total != null and swap_free != null and swap_total.? >= swap_free.?) {
                swap_used = swap_total.? - swap_free.?;
            }
        }
    } else if (comptime builtin.os.tag == .windows) {
        var ms: WinSys.MEMORYSTATUSEX = .{};
        if (WinSys.GlobalMemoryStatusEx(&ms) != 0) {
            mem_avail = ms.ullAvailPhys;
            if (ms.ullTotalPageFile >= ms.ullAvailPageFile) {
                const page_used = ms.ullTotalPageFile - ms.ullAvailPageFile;
                const phys_used = if (ms.ullTotalPhys >= ms.ullAvailPhys) ms.ullTotalPhys - ms.ullAvailPhys else 0;
                swap_used = if (page_used >= phys_used) page_used - phys_used else 0;
            }
        }
        var ft_idle: WinSys.FILETIME = .{};
        var ft_kern: WinSys.FILETIME = .{};
        var ft_user: WinSys.FILETIME = .{};
        if (WinSys.GetSystemTimes(&ft_idle, &ft_kern, &ft_user) != 0) {
            const idle = ft_idle.to_u64();
            const total = ft_kern.to_u64() +| ft_user.to_u64();
            const busy = if (total >= idle) total - idle else 0;
            cpu_pct = update_cpu_pct(busy, total);
        }
    }

    return .{
        .sampled_at_ms = now_ms,
        .cpu_pct = cpu_pct,
        .load1 = load1,
        .mem_avail_bytes = mem_avail,
        .swap_used_bytes = swap_used,
    };
}

pub const PresenceDetector = struct {
    const EvDev = struct {
        fd: std.os.linux.fd_t,
        path_buf: [64]u8 = undefined,
        path_len: usize = 0,

        fn path(self: *const EvDev) []const u8 {
            return self.path_buf[0..self.path_len];
        }
    };

    linux_fds: if (builtin.os.tag == .linux) [32]EvDev else void = if (builtin.os.tag == .linux) undefined else {},
    linux_fd_count: usize = 0,
    linux_denied: usize = 0,
    linux_scanned: bool = false,
    linux_sys_ok: bool = false,
    linux_last_scan_ms: i64 = 0,
    linux_started_ms: i64 = 0,
    linux_last_event_ms: ?i64 = null,

    win_cached_sess: ?u32 = null,
    win_cached_idle_ms: u64 = 0,
    win_cached_at_ms: i64 = 0,

    pub fn init(io: std.Io) PresenceDetector {
        const now_ms: i64 = std.Io.Clock.awake.now(io).toMilliseconds();
        return .{ .linux_started_ms = now_ms };
    }

    pub fn deinit(self: *PresenceDetector, _: std.Io) void {
        if (comptime builtin.os.tag == .linux) {
            for (self.linux_fds[0..self.linux_fd_count]) |*d| {
                _ = std.os.linux.close(d.fd);
            }
            self.linux_fd_count = 0;
        }
    }

    pub fn collect(self: *PresenceDetector, io: std.Io, env: ?*const std.process.Environ.Map) msg.Presence {
        const now_ms: i64 = std.Io.Clock.real.now(io).toMilliseconds();
        if (env) |e| {
            if (e.get("PROTECTORATE_NODE_TEST_PRESENCE")) |ov_path| {
                var buf: [64]u8 = undefined;
                const state: msg.PresenceState = if (read_small_file(io, ov_path, &buf)) |raw| blk: {
                    const tr = std.mem.trim(u8, raw, " \t\r\n");
                    if (std.mem.eql(u8, tr, "PRESENT")) break :blk .present;
                    if (std.mem.eql(u8, tr, "AWAY")) break :blk .away;
                    break :blk .unknown;
                } else .unknown;
                return .{
                    .state = state,
                    .source = "test-override",
                    .sampled_at_ms = now_ms,
                };
            }
        }
        if (comptime builtin.os.tag == .windows) {
            return self.collect_windows(io, now_ms);
        }
        if (comptime builtin.os.tag == .linux) {
            return self.collect_linux(io, now_ms);
        }
        return .{
            .state = .away,
            .source = "headless",
            .sampled_at_ms = now_ms,
        };
    }

    fn collect_windows(self: *PresenceDetector, io: std.Io, now_ms: i64) msg.Presence {
        if (comptime builtin.os.tag != .windows) unreachable;
        const wts = WinSys.load_wts() orelse {
            return .{ .state = .unknown, .source = "wts", .sampled_at_ms = now_ms };
        };
        const console = WinSys.WTSGetActiveConsoleSessionId();
        if (console == 0xFFFF_FFFF) {
            return .{ .state = .away, .source = "wts", .sampled_at_ms = now_ms };
        }
        var sess_ptr: ?[*]const WinSys.WTS_SESSION_INFOW = null;
        var sess_cnt: WinSys.w.DWORD = 0;
        if (wts.enum_sessions(null, 0, 1, &sess_ptr, &sess_cnt) == 0 or sess_ptr == null) {
            return .{ .state = .unknown, .source = "wts", .sampled_at_ms = now_ms };
        }
        var found_state: ?i32 = null;
        for (sess_ptr.?[0..sess_cnt]) |rec| {
            if (rec.SessionId == console) {
                found_state = rec.State;
                break;
            }
        }
        wts.free_mem(@ptrCast(sess_ptr.?));
        const s_state = found_state orelse {
            return .{ .state = .unknown, .source = "wts", .sampled_at_ms = now_ms };
        };
        if (s_state != 0 and s_state != 1) {
            return .{ .state = .away, .source = "wts", .sampled_at_ms = now_ms };
        }

        var raw_ptr: ?[*]const u8 = null;
        var raw_len: WinSys.w.DWORD = 0;
        if (wts.query_session(null, console, 25, &raw_ptr, &raw_len) == 0 or raw_ptr == null) {
            return .{ .state = .unknown, .source = "wts", .sampled_at_ms = now_ms };
        }
        defer wts.free_mem(@ptrCast(raw_ptr.?));
        if (raw_len < 208) {
            return .{ .state = .unknown, .source = "wts", .sampled_at_ms = now_ms };
        }
        const raw = raw_ptr.?[0..raw_len];
        const lvl = std.mem.readInt(u32, raw[0..4], .little);
        if (lvl != 1) {
            return .{ .state = .unknown, .source = "wts", .sampled_at_ms = now_ms };
        }
        const flags = std.mem.readInt(i32, raw[16..20], .little);
        const locked: ?bool = switch (flags) {
            0 => true,
            1 => false,
            else => null,
        };
        const user_first_u16 = std.mem.readInt(u16, raw[86..88], .little);
        if (user_first_u16 == 0) {
            return .{ .state = .away, .source = "wts", .sampled_at_ms = now_ms };
        }
        const last_input = std.mem.readInt(i64, raw[192..200], .little);
        const current_time = std.mem.readInt(i64, raw[200..208], .little);

        var idle_secs: ?u64 = null;
        var source: []const u8 = "wts";

        if (last_input > 0) {
            if (last_input > current_time) {
                return .{ .state = .unknown, .locked = locked, .source = "wts", .sampled_at_ms = now_ms };
            }
            idle_secs = @intCast(@divTrunc(current_time - last_input, 10_000_000));
        } else {
            var own_sess: WinSys.w.DWORD = 0xFFFF_FFFF;
            if (WinSys.ProcessIdToSessionId(WinSys.GetCurrentProcessId(), &own_sess) != 0 and own_sess == console) {
                source = "wts+getlastinputinfo";
                var lii: WinSys.LASTINPUTINFO = .{};
                if (WinSys.GetLastInputInfo(&lii) != 0) {
                    const elapsed_ms = WinSys.GetTickCount() -% lii.dwTime;
                    idle_secs = elapsed_ms / 1000;
                }
            } else {
                source = "wts+idle-probe";
                const awake_ms = std.Io.Clock.awake.now(io).toMilliseconds();
                if (self.win_cached_sess != null and self.win_cached_sess.? == console and awake_ms - self.win_cached_at_ms < 10_000) {
                    const extra: u64 = @intCast(@max(@as(i64, 0), awake_ms - self.win_cached_at_ms));
                    idle_secs = (self.win_cached_idle_ms + extra) / 1000;
                } else if (probe_windows_console_idle(io, wts, console)) |ms| {
                    self.win_cached_sess = console;
                    self.win_cached_idle_ms = ms;
                    self.win_cached_at_ms = awake_ms;
                    idle_secs = ms / 1000;
                }
            }
        }

        if (idle_secs == null and locked != true) {
            return .{
                .state = .unknown,
                .locked = locked,
                .source = source,
                .sampled_at_ms = now_ms,
            };
        }
        const is_away = (locked == true and (idle_secs == null or idle_secs.? >= 15)) or
            (idle_secs != null and idle_secs.? >= 300);
        return .{
            .state = if (is_away) .away else .present,
            .idle_secs = idle_secs,
            .locked = locked,
            .source = source,
            .sampled_at_ms = now_ms,
        };
    }

    fn collect_linux(self: *PresenceDetector, io: std.Io, now_ms: i64) msg.Presence {
        if (comptime builtin.os.tag != .linux) unreachable;
        const awake_ms = std.Io.Clock.awake.now(io).toMilliseconds();
        if (!self.linux_scanned or awake_ms - self.linux_last_scan_ms >= 30_000) {
            self.scan_evdev(io, awake_ms);
        }
        self.poll_evdev(awake_ms);

        const lg = query_logind(io, now_ms);
        if (lg.kind == .absent) {
            return .{
                .state = .away,
                .source = lg.source,
                .sampled_at_ms = now_ms,
            };
        }

        if (self.linux_fd_count > 0) {
            if (self.linux_last_event_ms) |le| {
                const idle: u64 = @intCast(@divTrunc(@max(@as(i64, 0), awake_ms - le), 1000));
                const is_away = (lg.locked == true and idle >= 15) or idle >= 300;
                return .{
                    .state = if (is_away) .away else .present,
                    .idle_secs = idle,
                    .locked = lg.locked,
                    .source = "evdev",
                    .sampled_at_ms = now_ms,
                };
            }
            const quiet: u64 = @intCast(@divTrunc(@max(@as(i64, 0), awake_ms - self.linux_started_ms), 1000));
            if (quiet >= 300) {
                return .{
                    .state = .away,
                    .idle_secs = quiet,
                    .locked = lg.locked,
                    .source = "evdev",
                    .sampled_at_ms = now_ms,
                };
            }
            if (lg.locked == true) {
                return .{
                    .state = .away,
                    .idle_secs = lg.idle_secs,
                    .locked = true,
                    .source = lg.source,
                    .sampled_at_ms = now_ms,
                };
            }
            return .{
                .state = .present,
                .idle_secs = lg.idle_secs,
                .locked = lg.locked,
                .source = lg.source,
                .sampled_at_ms = now_ms,
            };
        }

        if (self.linux_sys_ok and self.linux_denied == 0) {
            return .{
                .state = .away,
                .locked = lg.locked,
                .source = "no-input",
                .sampled_at_ms = now_ms,
            };
        }

        if (lg.kind == .present) {
            const is_away = (lg.locked == true and (lg.idle_secs == null or lg.idle_secs.? >= 15)) or
                (lg.idle_secs != null and lg.idle_secs.? >= 300);
            return .{
                .state = if (is_away) .away else .present,
                .idle_secs = lg.idle_secs,
                .locked = lg.locked,
                .source = lg.source,
                .sampled_at_ms = now_ms,
            };
        }

        return .{
            .state = .away,
            .source = "headless",
            .sampled_at_ms = now_ms,
        };
    }

    fn scan_evdev(self: *PresenceDetector, io: std.Io, awake_ms: i64) void {
        if (comptime builtin.os.tag != .linux) return;
        self.linux_last_scan_ms = awake_ms;
        self.linux_scanned = true;
        self.linux_denied = 0;

        var dir = std.Io.Dir.cwd().openDir(io, "/sys/class/input", .{ .iterate = true }) catch {
            self.linux_sys_ok = false;
            return;
        };
        defer dir.close(io);
        self.linux_sys_ok = true;

        var it = dir.iterate();
        while (it.next(io) catch null) |entry| {
            if (!std.mem.startsWith(u8, entry.name, "event")) continue;
            const tail = entry.name["event".len..];
            if (tail.len == 0) continue;
            var all_digits = true;
            for (tail) |c| {
                if (c < '0' or c > '9') {
                    all_digits = false;
                    break;
                }
            }
            if (!all_digits) continue;
            if (!is_human_evdev_device(io, entry.name)) continue;

            var pbuf: [64]u8 = undefined;
            const dev_path = std.fmt.bufPrint(&pbuf, "/dev/input/{s}", .{entry.name}) catch continue;
            var dup = false;
            for (self.linux_fds[0..self.linux_fd_count]) |*d| {
                if (std.mem.eql(u8, d.path(), dev_path)) {
                    dup = true;
                    break;
                }
            }
            if (dup) continue;
            if (self.linux_fd_count >= self.linux_fds.len) break;

            var pz: [65]u8 = undefined;
            @memcpy(pz[0..dev_path.len], dev_path);
            pz[dev_path.len] = 0;
            const rc = std.os.linux.open(@ptrCast(&pz), .{
                .ACCMODE = .RDONLY,
                .NONBLOCK = true,
                .CLOEXEC = true,
            }, 0);
            const eno = std.os.linux.errno(rc);
            if (eno == .SUCCESS) {
                var item = EvDev{
                    .fd = @intCast(@as(isize, @bitCast(rc))),
                    .path_len = dev_path.len,
                };
                @memcpy(item.path_buf[0..dev_path.len], dev_path);
                self.linux_fds[self.linux_fd_count] = item;
                self.linux_fd_count += 1;
            } else if (eno == .ACCES or eno == .PERM) {
                self.linux_denied += 1;
            }
        }
    }

    fn poll_evdev(self: *PresenceDetector, awake_ms: i64) void {
        if (comptime builtin.os.tag != .linux) return;
        var seen = false;
        var buf: [24 * 32]u8 = undefined;
        var idx: usize = 0;
        while (idx < self.linux_fd_count) {
            var remove = false;
            while (true) {
                const rc = std.os.linux.read(self.linux_fds[idx].fd, &buf, buf.len);
                const eno = std.os.linux.errno(rc);
                if (eno == .AGAIN) break;
                if (eno == .INTR) continue;
                if (eno != .SUCCESS) {
                    remove = true;
                    break;
                }
                if (rc == 0) {
                    remove = true;
                    break;
                }
                const n: usize = rc;
                var off: usize = 0;
                while (off + 24 <= n) : (off += 24) {
                    const ty = std.mem.readInt(u16, buf[off + 16 .. off + 18][0..2], .little);
                    if (ty >= 1 and ty <= 3) seen = true;
                }
            }
            if (remove) {
                _ = std.os.linux.close(self.linux_fds[idx].fd);
                self.linux_fd_count -= 1;
                if (idx < self.linux_fd_count) {
                    self.linux_fds[idx] = self.linux_fds[self.linux_fd_count];
                }
            } else {
                idx += 1;
            }
        }
        if (seen) self.linux_last_event_ms = awake_ms;
    }
};

const LogindResult = struct {
    kind: enum { absent, present, failed },
    idle_secs: ?u64 = null,
    locked: ?bool = null,
    source: []const u8 = "headless",
};

fn query_logind(io: std.Io, now_ms: i64) LogindResult {
    var sbuf: [8192]u8 = undefined;
    var fba = std.heap.FixedBufferAllocator.init(&sbuf);
    const a = fba.allocator();
    const list = std.process.run(a, io, .{
        .argv = &.{ "loginctl", "list-sessions", "--no-legend" },
        .stdout_limit = .limited(4096),
        .stderr_limit = .limited(1024),
    }) catch return .{ .kind = .failed };
    if (!list.term.success()) return .{ .kind = .failed };

    const now_usec: u64 = @intCast(@max(@as(i64, 0), now_ms) *| 1000);
    var active_count: usize = 0;
    var all_locked = true;
    var min_idle: ?u64 = null;

    var lines = std.mem.splitScalar(u8, list.stdout, '\n');
    while (lines.next()) |raw_ln| {
        const ln = std.mem.trim(u8, raw_ln, " \t\r");
        if (ln.len == 0) continue;
        var cols = std.mem.tokenizeAny(u8, ln, " \t");
        const sid = cols.next() orelse continue;

        const sh = std.process.run(a, io, .{
            .argv = &.{
                "loginctl", "show-session", sid,
                "-p",       "Seat",
                "-p",       "Class",
                "-p",       "Remote",
                "-p",       "State",
                "-p",       "Active",
                "-p",       "LockedHint",
                "-p",       "IdleHint",
                "-p",       "IdleSinceHint",
            },
            .stdout_limit = .limited(2048),
            .stderr_limit = .limited(512),
        }) catch continue;
        if (!sh.term.success()) continue;

        var seat: []const u8 = "";
        var class_s: []const u8 = "";
        var remote = false;
        var state_s: []const u8 = "";
        var active = false;
        var locked = false;
        var idle_hint = false;
        var idle_since_usec: u64 = 0;

        var pl = std.mem.splitScalar(u8, sh.stdout, '\n');
        while (pl.next()) |kv| {
            const tr = std.mem.trim(u8, kv, " \t\r");
            if (std.mem.startsWith(u8, tr, "Seat=")) seat = tr["Seat=".len..];
            if (std.mem.startsWith(u8, tr, "Class=")) class_s = tr["Class=".len..];
            if (std.mem.eql(u8, tr, "Remote=yes")) remote = true;
            if (std.mem.startsWith(u8, tr, "State=")) state_s = tr["State=".len..];
            if (std.mem.eql(u8, tr, "Active=yes")) active = true;
            if (std.mem.eql(u8, tr, "LockedHint=yes")) locked = true;
            if (std.mem.eql(u8, tr, "IdleHint=yes")) idle_hint = true;
            if (std.mem.startsWith(u8, tr, "IdleSinceHint=")) {
                idle_since_usec = std.fmt.parseInt(u64, tr["IdleSinceHint=".len..], 10) catch 0;
            }
        }
        if (seat.len == 0 or remote or std.mem.eql(u8, state_s, "closing")) continue;
        if (std.mem.eql(u8, class_s, "greeter")) continue;
        if (!active) continue;

        active_count += 1;
        if (!locked) all_locked = false;
        if (idle_hint and idle_since_usec > 0 and now_usec >= idle_since_usec) {
            const idle_s = (now_usec - idle_since_usec) / 1_000_000;
            min_idle = if (min_idle) |m| @min(m, idle_s) else idle_s;
        }
    }

    if (active_count == 0) {
        return .{ .kind = .absent, .source = "logind" };
    }
    return .{
        .kind = .present,
        .idle_secs = min_idle,
        .locked = all_locked,
        .source = "logind",
    };
}

fn is_human_evdev_device(io: std.Io, event_name: []const u8) bool {
    var link_path_buf: [128]u8 = undefined;
    const sys_path = std.fmt.bufPrint(&link_path_buf, "/sys/class/input/{s}", .{event_name}) catch return false;
    var target_buf: [256]u8 = undefined;
    if (std.Io.Dir.readLinkAbsolute(io, sys_path, &target_buf)) |tlen| {
        if (std.mem.find(u8, target_buf[0..tlen], "/devices/virtual/") != null) return false;
    } else |_| {}

    var ev_path_buf: [128]u8 = undefined;
    var key_path_buf: [128]u8 = undefined;
    const ev_p = std.fmt.bufPrint(&ev_path_buf, "/sys/class/input/{s}/device/capabilities/ev", .{event_name}) catch return false;
    const key_p = std.fmt.bufPrint(&key_path_buf, "/sys/class/input/{s}/device/capabilities/key", .{event_name}) catch return false;

    var ev_buf: [256]u8 = undefined;
    const ev_txt = read_small_file(io, ev_p, &ev_buf) orelse "";
    var key_buf: [512]u8 = undefined;
    const key_txt = read_small_file(io, key_p, &key_buf) orelse "";

    var ev_words: [16]u64 = undefined;
    const ev_len = parse_hex_cap_words(ev_txt, &ev_words);
    const ev0: u64 = if (ev_len > 0) ev_words[0] else 0;

    var key_words: [32]u64 = undefined;
    const key_len = parse_hex_cap_words(key_txt, &key_words);

    const has_rel = (ev0 & (@as(u64, 1) << 2)) != 0;
    const has_key = (ev0 & (@as(u64, 1) << 1)) != 0 and
        (cap_has_bit(key_words[0..key_len], 30) or cap_has_bit(key_words[0..key_len], 330));
    const has_abs = (ev0 & (@as(u64, 1) << 3)) != 0 and cap_has_bit(key_words[0..key_len], 330);
    return has_rel or has_key or has_abs;
}

fn parse_hex_cap_words(txt: []const u8, out: []u64) usize {
    var tmp: [32]u64 = undefined;
    var n: usize = 0;
    var it = std.mem.tokenizeAny(u8, txt, " \t\r\n");
    while (it.next()) |w| {
        if (n >= tmp.len or n >= out.len) break;
        tmp[n] = std.fmt.parseInt(u64, w, 16) catch 0;
        n += 1;
    }
    for (0..n) |i| {
        out[i] = tmp[n - 1 - i];
    }
    return n;
}

fn cap_has_bit(words: []const u64, bit: usize) bool {
    const idx = bit / 64;
    if (idx >= words.len) return false;
    return ((words[idx] >> @intCast(bit % 64)) & 1) == 1;
}

fn probe_windows_console_idle(io: std.Io, wts: WinSys.WtsApi, session: WinSys.w.DWORD) ?u64 {
    if (comptime builtin.os.tag != .windows) return null;
    var utok_opt: ?WinSys.w.HANDLE = null;
    if (wts.query_user_token(session, &utok_opt) == 0 or utok_opt == null) return null;
    const utok = utok_opt.?;
    defer WinSys.w.CloseHandle(utok);

    var abuf: [4096]u8 = undefined;
    var fba = std.heap.FixedBufferAllocator.init(&abuf);
    const a = fba.allocator();
    const exe_utf8 = std.process.executablePathAlloc(io, a) catch return null;
    const cmd_utf8 = std.fmt.allocPrint(a, "\"{s}\" idle-probe", .{exe_utf8}) catch return null;
    const cmd_w = std.unicode.utf8ToUtf16LeAllocZ(a, cmd_utf8) catch return null;
    const desk_w = std.unicode.utf8ToUtf16LeAllocZ(a, "winsta0\\default") catch return null;

    var sa = WinSys.w.SECURITY_ATTRIBUTES{
        .nLength = @sizeOf(WinSys.w.SECURITY_ATTRIBUTES),
        .lpSecurityDescriptor = null,
        .bInheritHandle = .TRUE,
    };
    var read_h: WinSys.w.HANDLE = undefined;
    var write_h: WinSys.w.HANDLE = undefined;
    if (WinSys.CreatePipe(&read_h, &write_h, &sa, 4096) == 0) return null;
    var read_closed = false;
    defer if (!read_closed) WinSys.w.CloseHandle(read_h);
    _ = WinSys.SetHandleInformation(read_h, 1, 0);

    var si = std.mem.zeroes(WinSys.w.STARTUPINFOW);
    si.cb = @sizeOf(WinSys.w.STARTUPINFOW);
    si.lpDesktop = @constCast(desk_w.ptr);
    si.dwFlags = WinSys.w.STARTF_USESTDHANDLES;
    si.hStdOutput = write_h;

    var pi = std.mem.zeroes(WinSys.PROCESS_INFORMATION);
    const create_no_window: WinSys.w.DWORD = 0x0800_0000;
    const ok = WinSys.CreateProcessAsUserW(
        utok,
        null,
        cmd_w.ptr,
        null,
        null,
        .TRUE,
        create_no_window,
        null,
        null,
        &si,
        &pi,
    );
    WinSys.w.CloseHandle(write_h);
    if (ok == 0) return null;
    defer WinSys.w.CloseHandle(pi.hThread);
    defer WinSys.w.CloseHandle(pi.hProcess);

    const wait_rc = WinSys.WaitForSingleObject(pi.hProcess, 3_000);
    if (wait_rc != 0) {
        _ = WinSys.TerminateProcess(pi.hProcess, 1);
        return null;
    }

    var out_buf: [512]u8 = undefined;
    var got: usize = 0;
    while (got < out_buf.len) {
        var n_read: WinSys.w.DWORD = 0;
        if (WinSys.ReadFile(read_h, out_buf[got..].ptr, @intCast(out_buf.len - got), &n_read, null) == 0 or n_read == 0) break;
        got += n_read;
    }
    WinSys.w.CloseHandle(read_h);
    read_closed = true;

    const text = std.mem.trim(u8, out_buf[0..got], " \t\r\n");
    if (text.len == 0) return null;
    const parsed = std.json.parseFromSliceLeaky(struct {
        session: u32 = 0xFFFF_FFFF,
        idle_ms: ?u64 = null,
    }, a, text, .{ .ignore_unknown_fields = true }) catch return null;
    if (parsed.session != session) return null;
    return parsed.idle_ms;
}

pub fn windows_idle_probe_json(buf: []u8) ?[]const u8 {
    if (comptime builtin.os.tag != .windows) return null;
    var sid: WinSys.w.DWORD = 0;
    if (WinSys.ProcessIdToSessionId(WinSys.GetCurrentProcessId(), &sid) == 0) return null;
    var lii: WinSys.LASTINPUTINFO = .{};
    if (WinSys.GetLastInputInfo(&lii) == 0) return null;
    const elapsed_ms = WinSys.GetTickCount() -% lii.dwTime;
    return std.fmt.bufPrint(buf, "{{\"session\":{d},\"idle_ms\":{d}}}", .{ sid, elapsed_ms }) catch null;
}

pub fn collect_presence(io: std.Io, env: ?*const std.process.Environ.Map) msg.Presence {
    var det = PresenceDetector.init(io);
    defer det.deinit(io);
    return det.collect(io, env);
}

fn update_cpu_pct(busy: u64, total: u64) ?f64 {
    const had_prev = prev_cpu_init.swap(true, .acq_rel);
    const old_b = prev_cpu_busy.swap(busy, .acq_rel);
    const old_t = prev_cpu_total.swap(total, .acq_rel);
    if (!had_prev or total <= old_t) return null;
    const db = busy -| old_b;
    const dt = total - old_t;
    const raw = (@as(f64, @floatFromInt(db)) * 100.0) / @as(f64, @floatFromInt(dt));
    return @min(100.0, @max(0.0, raw));
}

fn parse_proc_stat(txt: []const u8) ?struct { busy: u64, total: u64 } {
    const first_nl = std.mem.findScalar(u8, txt, '\n') orelse txt.len;
    const first = txt[0..first_nl];
    if (!std.mem.startsWith(u8, first, "cpu ")) return null;
    var it = std.mem.tokenizeAny(u8, first["cpu ".len..], " \t\r");
    var idx: usize = 0;
    var total: u64 = 0;
    var idle_all: u64 = 0;
    while (it.next()) |tok| : (idx += 1) {
        const val = std.fmt.parseInt(u64, tok, 10) catch 0;
        total +|= val;
        if (idx == 3 or idx == 4) idle_all +|= val;
    }
    if (idx < 4) return null;
    return .{
        .busy = if (total >= idle_all) total - idle_all else 0,
        .total = total,
    };
}

fn read_proc_file(arena: std.mem.Allocator, io: std.Io, path: []const u8, max_bytes: usize) ?[]u8 {
    var f = std.Io.Dir.cwd().openFile(io, path, .{}) catch return null;
    defer f.close(io);
    var rbuf: [4096]u8 = undefined;
    var r = f.readerStreaming(io, &rbuf);
    return r.interface.allocRemaining(arena, .limited(max_bytes)) catch null;
}

fn collect_disks(arena: std.mem.Allocator, io: std.Io) []const msg.Disk {
    var out: std.ArrayList(msg.Disk) = .empty;
    if (comptime builtin.os.tag == .linux) {
        const raw = read_proc_file(arena, io, "/proc/mounts", 256 * 1024) orelse return &.{};
        var lines = std.mem.splitScalar(u8, raw, '\n');
        while (lines.next()) |ln| {
            if (out.items.len >= 16) break;
            var it = std.mem.tokenizeAny(u8, ln, " \t\r");
            _ = it.next() orelse continue;
            const raw_mnt = it.next() orelse continue;
            const fstype = it.next() orelse continue;
            const mnt = unescape_mount_alloc(arena, raw_mnt) catch continue;
            const keep = std.mem.eql(u8, mnt, "/") or
                std.mem.eql(u8, fstype, "ext4") or
                std.mem.eql(u8, fstype, "ext3") or
                std.mem.eql(u8, fstype, "xfs") or
                std.mem.eql(u8, fstype, "btrfs") or
                std.mem.eql(u8, fstype, "zfs") or
                std.mem.eql(u8, fstype, "vfat") or
                std.mem.eql(u8, fstype, "exfat") or
                std.mem.eql(u8, fstype, "ntfs") or
                std.mem.eql(u8, fstype, "fuseblk");
            if (!keep) continue;

            var dup = false;
            for (out.items) |prev| {
                if (std.mem.eql(u8, prev.mount, mnt)) {
                    dup = true;
                    break;
                }
            }
            if (dup) continue;

            const mnt_z = arena.dupeSentinel(u8, mnt, 0) catch continue;
            var st: LinuxStatfs = undefined;
            const rc = std.os.linux.syscall2(.statfs, @intFromPtr(mnt_z.ptr), @intFromPtr(&st));
            if (std.os.linux.errno(rc) != .SUCCESS) continue;
            const blk: u64 = if (st.f_frsize > 0) @intCast(st.f_frsize) else if (st.f_bsize > 0) @intCast(st.f_bsize) else 4096;
            const total_bytes = st.f_blocks *| blk;
            const free_bytes = st.f_bavail *| blk;
            if (total_bytes == 0) continue;
            out.append(arena, .{
                .mount = mnt,
                .size_bytes = total_bytes,
                .free_bytes = free_bytes,
            }) catch continue;
        }
    } else if (comptime builtin.os.tag == .windows) {
        var dbuf: [512]u16 = undefined;
        const len = WinSys.GetLogicalDriveStringsW(dbuf.len, &dbuf);
        var pos: usize = 0;
        while (pos < len and out.items.len < 16) {
            var end = pos;
            while (end < len and dbuf[end] != 0) : (end += 1) {}
            if (end == pos) break;
            const slice_z: [*:0]const u16 = @ptrCast(dbuf[pos..end].ptr);
            const dtype = WinSys.GetDriveTypeW(slice_z);
            if (dtype == WinSys.DRIVE_FIXED or dtype == WinSys.DRIVE_REMOTE) {
                var avail: u64 = 0;
                var total: u64 = 0;
                var total_free: u64 = 0;
                if (WinSys.GetDiskFreeSpaceExW(slice_z, &avail, &total, &total_free) != 0 and total > 0) {
                    const m_utf8 = std.unicode.utf16LeToUtf8Alloc(arena, dbuf[pos..end]) catch continue;
                    out.append(arena, .{
                        .mount = m_utf8,
                        .size_bytes = total,
                        .free_bytes = avail,
                    }) catch {};
                }
            }
            pos = end + 1;
        }
    }
    return out.items;
}

fn collect_gpus(arena: std.mem.Allocator, io: std.Io) []const msg.Gpu {
    var out: std.ArrayList(msg.Gpu) = .empty;
    if (comptime builtin.os.tag == .windows) {
        for (0..16) |idx| {
            const sub = std.fmt.allocPrint(arena, "SYSTEM\\CurrentControlSet\\Control\\Class\\{{4d36e968-e325-11ce-bfc1-08002be10318}}\\{d:0>4}", .{idx}) catch continue;
            const desc = win_reg_str(arena, sub, "DriverDesc") orelse continue;
            const drv = win_reg_str(arena, sub, "DriverVersion") orelse "";
            const vram = win_reg_u64(arena, sub, "HardwareInformation.qwMemorySize") orelse
                win_reg_u64(arena, sub, "HardwareInformation.MemorySize");
            out.append(arena, .{
                .name = desc,
                .driver = drv,
                .vram_bytes = vram,
            }) catch {};
        }
        return out.items;
    }

    const nvi = std.process.run(arena, io, .{
        .argv = &.{ "nvidia-smi", "--query-gpu=name,driver_version,memory.total,utilization.gpu", "--format=csv,noheader,nounits" },
        .stdout_limit = .limited(8192),
        .stderr_limit = .limited(2048),
    }) catch null;
    if (nvi) |res| {
        if (res.term.success() and std.mem.trim(u8, res.stdout, " \t\r\n").len > 0) {
            var lines = std.mem.splitScalar(u8, res.stdout, '\n');
            while (lines.next()) |raw_ln| {
                const ln = std.mem.trim(u8, raw_ln, " \t\r");
                if (ln.len == 0) continue;
                var cols = std.mem.splitScalar(u8, ln, ',');
                const name = std.mem.trim(u8, cols.next() orelse continue, " \t");
                const drv = std.mem.trim(u8, cols.next() orelse "", " \t");
                const mem_s = std.mem.trim(u8, cols.next() orelse "", " \t");
                const util_s = std.mem.trim(u8, cols.next() orelse "", " \t");
                const vram: ?u64 = if (std.fmt.parseInt(u64, mem_s, 10)) |mib| mib *| (1024 * 1024) else |_| null;
                const util: ?f64 = std.fmt.parseFloat(f64, util_s) catch null;
                out.append(arena, .{
                    .name = arena.dupe(u8, name) catch continue,
                    .driver = arena.dupe(u8, drv) catch "",
                    .vram_bytes = vram,
                    .util_pct = util,
                }) catch {};
            }
            if (out.items.len > 0) return out.items;
        }
    }

    if (comptime builtin.os.tag == .linux) {
        var drm_dir = std.Io.Dir.cwd().openDir(io, "/sys/class/drm", .{ .iterate = true }) catch return out.items;
        defer drm_dir.close(io);
        var it = drm_dir.iterate();
        while (it.next(io) catch null) |entry| {
            if (!std.mem.startsWith(u8, entry.name, "card")) continue;
            if (std.mem.findScalar(u8, entry.name, '-') != null) continue;
            const uev_path = std.fmt.allocPrint(arena, "/sys/class/drm/{s}/device/uevent", .{entry.name}) catch continue;
            var ubuf: [2048]u8 = undefined;
            const utxt = read_small_file(io, uev_path, &ubuf) orelse continue;
            var driver: []const u8 = "drm";
            var pci_id: []const u8 = entry.name;
            var ulines = std.mem.splitScalar(u8, utxt, '\n');
            while (ulines.next()) |ul| {
                const tr = std.mem.trim(u8, ul, " \t\r");
                if (std.mem.startsWith(u8, tr, "DRIVER=")) driver = tr["DRIVER=".len..];
                if (std.mem.startsWith(u8, tr, "PCI_ID=")) pci_id = tr["PCI_ID=".len..];
            }
            const vram_path = std.fmt.allocPrint(arena, "/sys/class/drm/{s}/device/mem_info_vram_total", .{entry.name}) catch "";
            var vbuf: [64]u8 = undefined;
            const vram: ?u64 = if (vram_path.len > 0) blk: {
                const vt = read_small_file(io, vram_path, &vbuf) orelse break :blk null;
                break :blk std.fmt.parseInt(u64, std.mem.trim(u8, vt, " \t\r\n"), 10) catch null;
            } else null;
            out.append(arena, .{
                .name = arena.dupe(u8, pci_id) catch continue,
                .driver = arena.dupe(u8, driver) catch "drm",
                .vram_bytes = vram,
            }) catch {};
        }
    }
    return out.items;
}

fn read_total_ram(io: std.Io) u64 {
    if (comptime builtin.os.tag == .windows) {
        var ms: WinSys.MEMORYSTATUSEX = .{};
        if (WinSys.GlobalMemoryStatusEx(&ms) != 0) return ms.ullTotalPhys;
        return 0;
    }
    if (comptime builtin.os.tag == .linux) {
        var mbuf: [4096]u8 = undefined;
        if (read_small_file(io, "/proc/meminfo", &mbuf)) |mtxt| {
            var lines = std.mem.splitScalar(u8, mtxt, '\n');
            while (lines.next()) |line| {
                if (std.mem.startsWith(u8, line, "MemTotal:")) {
                    if (parse_kb_line(line["MemTotal:".len..])) |v| return v;
                }
            }
        }
    }
    return std.process.totalSystemMemory() catch 0;
}

pub fn read_mem_avail_bytes(io: std.Io) u64 {
    if (comptime builtin.os.tag == .windows) {
        var ms: WinSys.MEMORYSTATUSEX = .{};
        if (WinSys.GlobalMemoryStatusEx(&ms) != 0) return ms.ullAvailPhys;
        return 0;
    }
    if (comptime builtin.os.tag == .linux) {
        var mbuf: [4096]u8 = undefined;
        if (read_small_file(io, "/proc/meminfo", &mbuf)) |mtxt| {
            var lines = std.mem.splitScalar(u8, mtxt, '\n');
            while (lines.next()) |line| {
                if (std.mem.startsWith(u8, line, "MemAvailable:")) {
                    if (parse_kb_line(line["MemAvailable:".len..])) |v| return v;
                }
            }
        }
    }
    return read_total_ram(io);
}

pub fn read_boot_id(arena: std.mem.Allocator, io: std.Io) ![]const u8 {
    if (comptime builtin.os.tag == .linux) {
        var bbuf: [128]u8 = undefined;
        if (read_small_file(io, "/proc/sys/kernel/random/boot_id", &bbuf)) |raw| {
            const tr = std.mem.trim(u8, raw, " \t\r\n");
            if (tr.len > 0) return arena.dupe(u8, tr);
        }
    }
    const real_s: i64 = @divFloor(std.Io.Clock.real.now(io).toMilliseconds(), 10_000);
    const awake_s: i64 = @divFloor(std.Io.Clock.awake.now(io).toMilliseconds(), 10_000);
    return std.fmt.allocPrint(arena, "boot-{d}", .{real_s - awake_s});
}

fn read_os_version(arena: std.mem.Allocator, io: std.Io) ![]const u8 {
    if (comptime builtin.os.tag == .windows) {
        const sub = "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion";
        var prod = win_reg_str(arena, sub, "ProductName") orelse "Windows";
        const build_s = win_reg_str(arena, sub, "CurrentBuild") orelse "0";
        const build_n = std.fmt.parseInt(u64, build_s, 10) catch 0;
        if (build_n >= 22000 and std.mem.startsWith(u8, prod, "Windows 10")) {
            prod = try std.fmt.allocPrint(arena, "Windows 11{s}", .{prod["Windows 10".len..]});
        }
        const disp = win_reg_str(arena, sub, "DisplayVersion") orelse "";
        return std.fmt.allocPrint(arena, "{s} {s} (build {s})", .{ prod, disp, build_s });
    }
    if (comptime builtin.os.tag == .linux) {
        var buf: [2048]u8 = undefined;
        if (read_small_file(io, "/etc/os-release", &buf)) |txt| {
            var lines = std.mem.splitScalar(u8, txt, '\n');
            while (lines.next()) |line| {
                if (std.mem.startsWith(u8, line, "PRETTY_NAME=")) {
                    const val = std.mem.trim(u8, line["PRETTY_NAME=".len..], " \t\r\"");
                    if (val.len > 0) return arena.dupe(u8, val);
                }
            }
        }
    }
    return arena.dupe(u8, @tagName(builtin.os.tag));
}

fn read_cpu_model(arena: std.mem.Allocator, io: std.Io) ![]const u8 {
    if (comptime builtin.os.tag == .windows) {
        if (win_reg_str(arena, "HARDWARE\\DESCRIPTION\\System\\CentralProcessor\\0", "ProcessorNameString")) |nm| {
            const tr = std.mem.trim(u8, nm, " \t\r\n");
            if (tr.len > 0) return arena.dupe(u8, tr);
        }
        return arena.dupe(u8, @tagName(builtin.cpu.arch));
    }
    if (comptime builtin.os.tag == .linux) {
        var buf: [8192]u8 = undefined;
        if (read_small_file(io, "/proc/cpuinfo", &buf)) |txt| {
            var lines = std.mem.splitScalar(u8, txt, '\n');
            while (lines.next()) |line| {
                if (std.mem.startsWith(u8, line, "model name")) {
                    if (std.mem.findScalar(u8, line, ':')) |colon| {
                        const val = std.mem.trim(u8, line[colon + 1 ..], " \t\r");
                        if (val.len > 0) return arena.dupe(u8, val);
                    }
                }
            }
        }
    }
    return arena.dupe(u8, @tagName(builtin.cpu.arch));
}

fn win_reg_str(arena: std.mem.Allocator, subkey: []const u8, value_name: []const u8) ?[]const u8 {
    if (comptime builtin.os.tag != .windows) return null;
    const sub_w = std.unicode.utf8ToUtf16LeAllocZ(arena, subkey) catch return null;
    const val_w = std.unicode.utf8ToUtf16LeAllocZ(arena, value_name) catch return null;
    var hkey: WinSys.w.HANDLE = undefined;
    if (WinSys.RegOpenKeyExW(WinSys.hklm, sub_w.ptr, 0, WinSys.KEY_READ, &hkey) != 0) return null;
    defer _ = WinSys.RegCloseKey(hkey);

    var buf16: [512]u16 align(@alignOf(u16)) = undefined;
    var typ: WinSys.w.DWORD = 0;
    var cb: WinSys.w.DWORD = buf16.len * 2;
    if (WinSys.RegQueryValueExW(hkey, val_w.ptr, null, &typ, @ptrCast(&buf16), &cb) != 0) return null;
    if (typ != WinSys.REG_SZ) return null;
    var wlen: usize = @min(@as(usize, cb) / 2, buf16.len);
    while (wlen > 0 and buf16[wlen - 1] == 0) wlen -= 1;
    return std.unicode.utf16LeToUtf8Alloc(arena, buf16[0..wlen]) catch null;
}

fn win_reg_u64(arena: std.mem.Allocator, subkey: []const u8, value_name: []const u8) ?u64 {
    if (comptime builtin.os.tag != .windows) return null;
    const sub_w = std.unicode.utf8ToUtf16LeAllocZ(arena, subkey) catch return null;
    const val_w = std.unicode.utf8ToUtf16LeAllocZ(arena, value_name) catch return null;
    var hkey: WinSys.w.HANDLE = undefined;
    if (WinSys.RegOpenKeyExW(WinSys.hklm, sub_w.ptr, 0, WinSys.KEY_READ, &hkey) != 0) return null;
    defer _ = WinSys.RegCloseKey(hkey);

    var raw: u64 align(@alignOf(u64)) = 0;
    var typ: WinSys.w.DWORD = 0;
    var cb: WinSys.w.DWORD = 8;
    if (WinSys.RegQueryValueExW(hkey, val_w.ptr, null, &typ, @ptrCast(&raw), &cb) != 0) return null;
    if (typ == WinSys.REG_DWORD and cb == 4) {
        var v32: u32 = 0;
        @memcpy(std.mem.asBytes(&v32), std.mem.asBytes(&raw)[0..4]);
        return v32;
    }
    if (typ == WinSys.REG_QWORD and cb == 8) return raw;
    return null;
}

fn unescape_mount_alloc(arena: std.mem.Allocator, raw: []const u8) ![]const u8 {
    var aw: std.ArrayList(u8) = .empty;
    var i: usize = 0;
    while (i < raw.len) {
        if (raw[i] == '\\' and i + 3 < raw.len and
            std.mem.eql(u8, raw[i .. i + 4], "\\040"))
        {
            try aw.append(arena, ' ');
            i += 4;
        } else {
            try aw.append(arena, raw[i]);
            i += 1;
        }
    }
    return aw.items;
}

fn read_small_file(io: std.Io, path: []const u8, buf: []u8) ?[]const u8 {
    var f = std.Io.Dir.cwd().openFile(io, path, .{}) catch return null;
    defer f.close(io);
    const n = f.readPositionalAll(io, buf, 0) catch return null;
    return buf[0..n];
}

fn parse_kb_line(tail: []const u8) ?u64 {
    var it = std.mem.tokenizeAny(u8, tail, " \t\r\n");
    const num_s = it.next() orelse return null;
    const kb = std.fmt.parseInt(u64, num_s, 10) catch return null;
    return kb *| 1024;
}

fn probe_tool_version(arena: std.mem.Allocator, io: std.Io, name: []const u8) ?[]const u8 {
    const arg = if (std.mem.eql(u8, name, "zig")) "version" else "--version";
    const res = std.process.run(arena, io, .{
        .argv = &.{ name, arg },
        .stdout_limit = .limited(2048),
        .stderr_limit = .limited(2048),
    }) catch return null;
    if (!res.term.success()) return null;
    const raw = if (res.stdout.len > 0) res.stdout else res.stderr;
    const line = std.mem.trim(u8, std.mem.sliceTo(raw, '\n'), " \t\r");
    return arena.dupe(u8, line) catch null;
}
