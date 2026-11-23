const std = @import("std");
const builtin = @import("builtin");
const tiger = @import("../tiger.zig");
const spec = @import("spec.zig");

const WinJob = if (builtin.os.tag == .windows) struct {
    const w = std.os.windows;

    const JOBOBJECT_BASIC_LIMIT_INFORMATION = extern struct {
        PerProcessUserTimeLimit: i64 = 0,
        PerJobUserTimeLimit: i64 = 0,
        LimitFlags: w.DWORD = 0,
        MinimumWorkingSetSize: w.SIZE_T = 0,
        MaximumWorkingSetSize: w.SIZE_T = 0,
        ActiveProcessLimit: w.DWORD = 0,
        Affinity: w.ULONG_PTR = 0,
        PriorityClass: w.DWORD = 0,
        SchedulingClass: w.DWORD = 0,
    };

    const IO_COUNTERS = extern struct {
        ReadOperationCount: u64 = 0,
        WriteOperationCount: u64 = 0,
        OtherOperationCount: u64 = 0,
        ReadTransferCount: u64 = 0,
        WriteTransferCount: u64 = 0,
        OtherTransferCount: u64 = 0,
    };

    const JOBOBJECT_EXTENDED_LIMIT_INFORMATION = extern struct {
        BasicLimitInformation: JOBOBJECT_BASIC_LIMIT_INFORMATION = .{},
        IoInfo: IO_COUNTERS = .{},
        ProcessMemoryLimit: w.SIZE_T = 0,
        JobMemoryLimit: w.SIZE_T = 0,
        PeakProcessMemoryUsed: w.SIZE_T = 0,
        PeakJobMemoryUsed: w.SIZE_T = 0,
    };

    const JobObjectExtendedLimitInformation: c_int = 9;
    const JOB_OBJECT_LIMIT_JOB_MEMORY: w.DWORD = 0x00000200;
    const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: w.DWORD = 0x00002000;

    extern "kernel32" fn CreateJobObjectW(
        lpJobAttributes: ?*anyopaque,
        lpName: ?[*:0]const u16,
    ) callconv(.winapi) ?w.HANDLE;

    extern "kernel32" fn SetInformationJobObject(
        hJob: w.HANDLE,
        JobObjectInformationClass: c_int,
        lpJobObjectInformation: *const anyopaque,
        cbJobObjectInformationLength: w.DWORD,
    ) callconv(.winapi) c_int;

    extern "kernel32" fn AssignProcessToJobObject(
        hJob: w.HANDLE,
        hProcess: w.HANDLE,
    ) callconv(.winapi) c_int;

    extern "kernel32" fn TerminateJobObject(
        hJob: w.HANDLE,
        uExitCode: c_uint,
    ) callconv(.winapi) c_int;
} else struct {};

pub const ProcessGuard = struct {
    pgid: if (builtin.os.tag == .windows) void else ?std.posix.pid_t = if (builtin.os.tag == .windows) {} else null,
    job: if (builtin.os.tag == .windows) ?std.os.windows.HANDLE else void = if (builtin.os.tag == .windows) null else {},

    pub fn init(memory_max_bytes: ?u64) ProcessGuard {
        if (comptime builtin.os.tag == .windows) {
            const j = WinJob.CreateJobObjectW(null, null) orelse return .{};
            var info: WinJob.JOBOBJECT_EXTENDED_LIMIT_INFORMATION = .{};
            info.BasicLimitInformation.LimitFlags = WinJob.JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if (memory_max_bytes) |lim| {
                if (lim > 0) {
                    info.BasicLimitInformation.LimitFlags |= WinJob.JOB_OBJECT_LIMIT_JOB_MEMORY;
                    info.JobMemoryLimit = @intCast(@min(lim, std.math.maxInt(WinJob.w.SIZE_T)));
                }
            }
            if (WinJob.SetInformationJobObject(
                j,
                WinJob.JobObjectExtendedLimitInformation,
                &info,
                @sizeOf(WinJob.JOBOBJECT_EXTENDED_LIMIT_INFORMATION),
            ) == 0) {
                std.os.windows.CloseHandle(j);
                return .{};
            }
            return .{ .job = j };
        }
        return .{};
    }

    pub fn attach(self: *ProcessGuard, child_id: ?std.process.Child.Id) void {
        const cid = child_id orelse return;
        if (comptime builtin.os.tag == .windows) {
            if (self.job) |j| {
                _ = WinJob.AssignProcessToJobObject(j, cid);
            }
        } else {
            self.pgid = cid;
        }
    }

    pub fn terminate(self: *ProcessGuard) void {
        if (comptime builtin.os.tag == .windows) {
            if (self.job) |j| {
                _ = WinJob.TerminateJobObject(j, 1);
            }
        } else {
            if (self.pgid) |pid| {
                std.posix.kill(-pid, .KILL) catch {};
                std.posix.kill(pid, .KILL) catch {};
            }
        }
    }

    pub fn deinit(self: *ProcessGuard) void {
        if (comptime builtin.os.tag == .windows) {
            if (self.job) |j| {
                std.os.windows.CloseHandle(j);
                self.job = null;
            }
        } else {
            self.pgid = null;
        }
    }
};

pub fn probe_landlock() bool {
    if (comptime builtin.os.tag != .linux) return false;
    const rc = std.os.linux.syscall3(.landlock_create_ruleset, 0, 0, 1);
    return std.os.linux.errno(rc) == .SUCCESS and rc >= 1;
}

const LandlockRulesetAttr = extern struct {
    handled_access_fs: u64,
};

const LandlockPathBeneathAttr = extern struct {
    allowed_access: u64,
    parent_fd: std.os.linux.fd_t,
};

pub fn run_guard_trampoline(init: std.process.Init, args: []const []const u8) !u8 {
    var arena_st = std.heap.ArenaAllocator.init(init.gpa);
    defer arena_st.deinit();
    const arena = arena_st.allocator();

    var cwd_opt: ?[]const u8 = null;
    var readonly = false;
    var mem_max_opt: ?u64 = null;
    var rw_paths: std.ArrayList([]const u8) = .empty;
    var inner_start: ?usize = null;

    var i: usize = 0;
    while (i < args.len) : (i += 1) {
        const arg = args[i];
        if (std.mem.eql(u8, arg, "--")) {
            inner_start = i + 1;
            break;
        } else if (std.mem.eql(u8, arg, "--cwd") and i + 1 < args.len) {
            i += 1;
            cwd_opt = args[i];
        } else if (std.mem.eql(u8, arg, "--readonly")) {
            readonly = true;
        } else if (std.mem.startsWith(u8, arg, "--mem=")) {
            mem_max_opt = std.fmt.parseInt(u64, arg["--mem=".len..], 10) catch null;
        } else if (std.mem.eql(u8, arg, "--rw") and i + 1 < args.len) {
            i += 1;
            if (!readonly) try rw_paths.append(arena, args[i]);
        } else if (std.mem.eql(u8, arg, "--tmp") and i + 1 < args.len) {
            i += 1;
            try rw_paths.append(arena, args[i]);
        }
    }

    const start_idx = inner_start orelse return 2;
    if (start_idx >= args.len) return 2;
    const inner_argv = args[start_idx..];

    if (comptime builtin.os.tag == .linux) {
        apply_linux_guard(arena, cwd_opt, mem_max_opt, rw_paths.items) catch |err| {
            var ebuf: [256]u8 = undefined;
            var ew = std.Io.File.stderr().writer(init.io, &ebuf);
            ew.interface.print("protectorate __guard failed: {s}\n", .{@errorName(err)}) catch {};
            ew.interface.flush() catch {};
            return 1;
        };
    }

    if (comptime builtin.os.tag == .windows) {
        var child = try std.process.spawn(init.io, .{
            .argv = inner_argv,
            .cwd = if (cwd_opt) |c| .{ .path = c } else .inherit,
            .environ_map = init.environ_map,
        });
        const term = try child.wait(init.io);
        return switch (term) {
            .exited => |code| code,
            else => 1,
        };
    }

    return std.process.replace(init.io, .{
        .argv = inner_argv,
        .cwd = if (cwd_opt) |c| .{ .path = c } else .inherit,
        .environ_map = init.environ_map,
    });
}

fn apply_linux_guard(
    arena: std.mem.Allocator,
    cwd_opt: ?[]const u8,
    mem_max_opt: ?u64,
    rw_paths: []const []const u8,
) !void {
    const linux = std.os.linux;

    _ = linux.prctl(@intFromEnum(linux.PR.SET_PDEATHSIG), @intFromEnum(linux.SIG.KILL), 0, 0, 0);

    if (mem_max_opt) |lim| {
        const rl = linux.rlimit{
            .cur = @intCast(lim),
            .max = @intCast(lim),
        };
        _ = linux.setrlimit(.AS, &rl);
    }

    const abi_rc = linux.syscall3(.landlock_create_ruleset, 0, 0, 1);
    if (linux.errno(abi_rc) == .SUCCESS and abi_rc >= 1) {
        const fs_write_file: u64 = 1 << 1;
        const fs_remove_dir: u64 = 1 << 4;
        const fs_remove_file: u64 = 1 << 5;
        const fs_make_char: u64 = 1 << 6;
        const fs_make_dir: u64 = 1 << 7;
        const fs_make_reg: u64 = 1 << 8;
        const fs_make_sock: u64 = 1 << 9;
        const fs_make_fifo: u64 = 1 << 10;
        const fs_make_block: u64 = 1 << 11;
        const fs_make_sym: u64 = 1 << 12;
        const fs_refer: u64 = 1 << 13;
        const fs_truncate: u64 = 1 << 14;

        var dir_mask: u64 = fs_write_file |
            fs_remove_dir |
            fs_remove_file |
            fs_make_char |
            fs_make_dir |
            fs_make_reg |
            fs_make_sock |
            fs_make_fifo |
            fs_make_block |
            fs_make_sym;
        if (abi_rc >= 2) dir_mask |= fs_refer;
        if (abi_rc >= 3) dir_mask |= fs_truncate;

        var file_mask: u64 = fs_write_file;
        if (abi_rc >= 3) file_mask |= fs_truncate;

        var rs_attr = LandlockRulesetAttr{ .handled_access_fs = dir_mask };
        const rs_rc = linux.syscall3(.landlock_create_ruleset, @intFromPtr(&rs_attr), @sizeOf(LandlockRulesetAttr), 0);
        if (linux.errno(rs_rc) != .SUCCESS) return error.LandlockRulesetFailed;
        const rs_fd: linux.fd_t = @intCast(rs_rc);
        defer _ = linux.close(rs_fd);

        for (rw_paths) |p| {
            try add_landlock_path(arena, rs_fd, p, dir_mask, file_mask);
        }
        try add_landlock_path(arena, rs_fd, "/dev/null", dir_mask, file_mask);
        try add_landlock_path(arena, rs_fd, "/dev/shm", dir_mask, file_mask);

        if (linux.errno(linux.prctl(@intFromEnum(linux.PR.SET_NO_NEW_PRIVS), 1, 0, 0, 0)) != .SUCCESS) {
            return error.NoNewPrivsFailed;
        }
        const rest_rc = linux.syscall2(.landlock_restrict_self, @as(u32, @bitCast(rs_fd)), 0);
        if (linux.errno(rest_rc) != .SUCCESS) return error.LandlockRestrictFailed;
    }

    if (cwd_opt) |c| {
        const cwd_z = try arena.dupeSentinel(u8, c, 0);
        _ = linux.chdir(cwd_z);
    }
}

fn add_landlock_path(
    arena: std.mem.Allocator,
    rs_fd: std.os.linux.fd_t,
    path: []const u8,
    dir_mask: u64,
    file_mask: u64,
) !void {
    const linux = std.os.linux;
    if (path.len == 0) return;
    const p_z = try arena.dupeSentinel(u8, path, 0);
    const fd_rc = linux.open(p_z, .{ .PATH = true, .CLOEXEC = true }, 0);
    if (linux.errno(fd_rc) != .SUCCESS) return;
    const pfd: linux.fd_t = @intCast(fd_rc);
    defer _ = linux.close(pfd);

    var pb = LandlockPathBeneathAttr{
        .allowed_access = dir_mask,
        .parent_fd = pfd,
    };
    const rc1 = linux.syscall4(.landlock_add_rule, @as(u32, @bitCast(rs_fd)), 1, @intFromPtr(&pb), 0);
    if (linux.errno(rc1) == .INVAL) {
        pb.allowed_access = file_mask;
        const rc2 = linux.syscall4(.landlock_add_rule, @as(u32, @bitCast(rs_fd)), 1, @intFromPtr(&pb), 0);
        if (linux.errno(rc2) != .SUCCESS) return error.LandlockRuleFailed;
    } else if (linux.errno(rc1) != .SUCCESS) {
        return error.LandlockRuleFailed;
    }
}

pub fn probe_bwrap(gpa: std.mem.Allocator, io: std.Io) bool {
    if (builtin.os.tag != .linux) return false;
    const res = std.process.run(gpa, io, .{
        .argv = &.{ "bwrap", "--version" },
        .stdout_limit = .limited(4096),
        .stderr_limit = .limited(4096),
    }) catch return false;
    gpa.free(res.stdout);
    gpa.free(res.stderr);
    return res.term.success();
}

pub fn probe_systemd_scope(gpa: std.mem.Allocator, io: std.Io) bool {
    if (builtin.os.tag != .linux) return false;
    const res = std.process.run(gpa, io, .{
        .argv = &.{ "systemd-run", "--user", "--version" },
        .stdout_limit = .limited(4096),
        .stderr_limit = .limited(4096),
    }) catch return false;
    gpa.free(res.stdout);
    gpa.free(res.stderr);
    return res.term.success();
}

pub fn resolve_git_dirs(gpa: std.mem.Allocator, io: std.Io, workdir: []const u8) ![]const []const u8 {
    var out: std.ArrayList([]const u8) = .empty;
    errdefer {
        for (out.items) |p| gpa.free(p);
        out.deinit(gpa);
    }

    const res = std.process.run(gpa, io, .{
        .argv = &.{ "git", "-C", workdir, "rev-parse", "--git-dir", "--git-common-dir" },
        .stdout_limit = .limited(64 * 1024),
        .stderr_limit = .limited(16 * 1024),
    }) catch return try out.toOwnedSlice(gpa);
    defer {
        gpa.free(res.stdout);
        gpa.free(res.stderr);
    }
    if (!res.term.success()) return try out.toOwnedSlice(gpa);

    var lines = std.mem.splitScalar(u8, res.stdout, '\n');
    while (lines.next()) |raw| {
        const trimmed = std.mem.trim(u8, raw, " \t\r");
        if (trimmed.len == 0) continue;
        const joined = if (std.fs.path.isAbsolute(trimmed))
            try gpa.dupe(u8, trimmed)
        else
            try std.fs.path.join(gpa, &.{ workdir, trimmed });
        defer gpa.free(joined);

        const real_z = std.Io.Dir.cwd().realPathFileAlloc(io, joined, gpa) catch continue;
        const clean = try gpa.dupe(u8, real_z);
        gpa.free(real_z);
        if (!spec.is_clean_abs(clean)) {
            gpa.free(clean);
            continue;
        }
        var dup = false;
        for (out.items) |prev| {
            if (std.mem.eql(u8, prev, clean)) {
                dup = true;
                break;
            }
        }
        if (dup) {
            gpa.free(clean);
        } else {
            try out.append(gpa, clean);
        }
    }
    return try out.toOwnedSlice(gpa);
}

pub fn kill_scope(gpa: std.mem.Allocator, io: std.Io, scope_name: []const u8) void {
    if (builtin.os.tag != .linux) return;
    if (scope_name.len == 0) return;
    const res = std.process.run(gpa, io, .{
        .argv = &.{ "systemctl", "--user", "stop", scope_name },
        .stdout_limit = .limited(4096),
        .stderr_limit = .limited(4096),
    }) catch return;
    gpa.free(res.stdout);
    gpa.free(res.stderr);
}
