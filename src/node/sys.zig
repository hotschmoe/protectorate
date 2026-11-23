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

    extern "user32" fn GetLastInputInfo(plii: *LASTINPUTINFO) callconv(.winapi) c_int;
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

pub fn collect_presence(io: std.Io, env: ?*const std.process.Environ.Map) msg.Presence {
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
        var lii: WinSys.LASTINPUTINFO = .{};
        if (WinSys.GetLastInputInfo(&lii) != 0) {
            const elapsed_ms = WinSys.GetTickCount() -% lii.dwTime;
            const idle_secs: u64 = elapsed_ms / 1000;
            return .{
                .state = if (idle_secs < 300) .present else .away,
                .idle_secs = idle_secs,
                .source = "win32-input",
                .sampled_at_ms = now_ms,
            };
        }
        return .{
            .state = .away,
            .source = "win32-service",
            .sampled_at_ms = now_ms,
        };
    }

    if (comptime builtin.os.tag == .linux) {
        var sbuf: [8192]u8 = undefined;
        var fba = std.heap.FixedBufferAllocator.init(&sbuf);
        const a = fba.allocator();
        const list = std.process.run(a, io, .{
            .argv = &.{ "loginctl", "list-sessions", "--no-legend" },
            .stdout_limit = .limited(4096),
            .stderr_limit = .limited(1024),
        }) catch null;
        if (list) |res| {
            if (res.term.success()) {
                var lines = std.mem.splitScalar(u8, res.stdout, '\n');
                while (lines.next()) |raw_ln| {
                    const ln = std.mem.trim(u8, raw_ln, " \t\r");
                    if (ln.len == 0) continue;
                    var cols = std.mem.tokenizeAny(u8, ln, " \t");
                    const sid = cols.next() orelse continue;
                    _ = cols.next() orelse continue;
                    _ = cols.next() orelse continue;
                    const seat = cols.next() orelse "";
                    if (!std.mem.startsWith(u8, seat, "seat")) continue;

                    const sh = std.process.run(a, io, .{
                        .argv = &.{ "loginctl", "show-session", sid, "-p", "Active", "-p", "LockedHint", "-p", "IdleHint" },
                        .stdout_limit = .limited(2048),
                        .stderr_limit = .limited(512),
                    }) catch continue;
                    if (!sh.term.success()) continue;
                    var active = false;
                    var locked = false;
                    var idle = false;
                    var pl = std.mem.splitScalar(u8, sh.stdout, '\n');
                    while (pl.next()) |kv| {
                        const tr = std.mem.trim(u8, kv, " \t\r");
                        if (std.mem.eql(u8, tr, "Active=yes")) active = true;
                        if (std.mem.eql(u8, tr, "LockedHint=yes")) locked = true;
                        if (std.mem.eql(u8, tr, "IdleHint=yes")) idle = true;
                    }
                    if (active) {
                        return .{
                            .state = if (locked or idle) .away else .present,
                            .locked = locked,
                            .source = "logind",
                            .sampled_at_ms = now_ms,
                        };
                    }
                }
            }
        }
    }

    return .{
        .state = .away,
        .source = "headless",
        .sampled_at_ms = now_ms,
    };
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
