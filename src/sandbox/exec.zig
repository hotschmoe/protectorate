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

    const JobObjectBasicAccountingInformation: c_int = 1;
    const JobObjectExtendedLimitInformation: c_int = 9;
    const JOB_OBJECT_LIMIT_JOB_MEMORY: w.DWORD = 0x00000200;
    const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: w.DWORD = 0x00002000;

    const JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = extern struct {
        TotalUserTime: i64 = 0,
        TotalKernelTime: i64 = 0,
        ThisPeriodTotalUserTime: i64 = 0,
        ThisPeriodTotalKernelTime: i64 = 0,
        TotalPageFaultCount: w.DWORD = 0,
        TotalProcesses: w.DWORD = 0,
        ActiveProcesses: w.DWORD = 0,
        TotalTerminatedProcesses: w.DWORD = 0,
    };

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

    extern "kernel32" fn QueryInformationJobObject(
        hJob: ?w.HANDLE,
        JobObjectInformationClass: c_int,
        lpJobObjectInformation: *anyopaque,
        cbJobObjectInformationLength: w.DWORD,
        lpReturnLength: ?*w.DWORD,
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

var next_cg_seq: std.atomic.Value(u64) = .init(0);

pub const ProcessGuard = struct {
    pgid: if (builtin.os.tag == .windows) void else ?std.posix.pid_t = if (builtin.os.tag == .windows) {} else null,
    job: if (builtin.os.tag == .windows) ?std.os.windows.HANDLE else void = if (builtin.os.tag == .windows) null else {},
    gpa: ?std.mem.Allocator = null,
    tag: ?[]u8 = null,
    cgroup_path: if (builtin.os.tag == .linux) ?[]u8 else void = if (builtin.os.tag == .linux) null else {},
    launcher_pidfd: if (builtin.os.tag == .linux) ?std.posix.fd_t else void = if (builtin.os.tag == .linux) null else {},

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

    pub fn init_fenced(
        gpa: std.mem.Allocator,
        io: std.Io,
        tag: []const u8,
        memory_max_bytes: ?u64,
    ) ProcessGuard {
        var g = init(memory_max_bytes);
        g.gpa = gpa;
        g.tag = gpa.dupe(u8, tag) catch null;
        if (comptime builtin.os.tag == .linux) {
            g.cgroup_path = create_job_cgroup(gpa, io, tag, memory_max_bytes);
        }
        return g;
    }

    pub fn init_sleeve(
        gpa: std.mem.Allocator,
        io: std.Io,
        tag: []const u8,
        memory_max_bytes: u64,
    ) !ProcessGuard {
        if (comptime builtin.os.tag != .linux) return error.CgroupUnavailable;
        tiger.assert(memory_max_bytes > 0);
        var path_buf: [std.fs.max_path_bytes]u8 = undefined;
        const current = try current_cgroup_path(io, &path_buf);
        const root = if (std.mem.eql(u8, std.fs.path.basename(current), "protectorate-hub"))
            std.fs.path.dirname(current) orelse return error.CgroupUnavailable
        else
            current;
        var dir = try std.Io.Dir.cwd().openDir(io, root, .{});
        defer dir.close(io);
        const procs = try dir.openFile(io, "cgroup.procs", .{ .mode = .write_only });
        procs.close(io);
        try dir.writeFile(io, .{ .sub_path = "cgroup.subtree_control", .data = "+memory\n" });

        const seq = next_cg_seq.fetchAdd(1, .monotonic);
        const path = try std.fmt.allocPrint(gpa, "{s}/pt-sleeve-{d}-{d}", .{ root, std.os.linux.getpid(), seq });
        errdefer gpa.free(path);
        try std.Io.Dir.cwd().createDir(io, path, .default_dir);
        errdefer std.Io.Dir.deleteDirAbsolute(io, path) catch {};
        const cg = try std.Io.Dir.cwd().openDir(io, path, .{});
        defer cg.close(io);
        const kill = try cg.openFile(io, "cgroup.kill", .{ .mode = .write_only });
        kill.close(io);
        var limit_buf: [32]u8 = undefined;
        const limit = try std.fmt.bufPrint(&limit_buf, "{d}\n", .{memory_max_bytes});
        try cg.writeFile(io, .{ .sub_path = "memory.max", .data = limit });
        try cg.writeFile(io, .{ .sub_path = "memory.swap.max", .data = "0\n" });
        try cg.writeFile(io, .{ .sub_path = "memory.oom.group", .data = "0\n" });
        if (try oom_kill_count(io, cg) != 0) return error.CgroupAlreadyUsed;
        return .{
            .gpa = gpa,
            .tag = try gpa.dupe(u8, tag),
            .cgroup_path = path,
        };
    }

    pub fn sleeve_cgroup(self: *const ProcessGuard) ?[]const u8 {
        if (comptime builtin.os.tag != .linux) return null;
        return self.cgroup_path;
    }

    pub fn sleeve_oom_killed(self: *const ProcessGuard, io: std.Io) !bool {
        const path = self.sleeve_cgroup() orelse return false;
        const dir = try std.Io.Dir.cwd().openDir(io, path, .{});
        defer dir.close(io);
        return try oom_kill_count(io, dir) > 0;
    }

    pub fn attach_sleeve(self: *ProcessGuard, child_id: ?std.process.Child.Id) void {
        self.attach(child_id);
        if (comptime builtin.os.tag != .linux) return;
        const pid = child_id orelse return;
        const result = std.os.linux.pidfd_open(pid, 0);
        if (std.os.linux.errno(result) == .SUCCESS) self.launcher_pidfd = @intCast(result);
    }

    pub fn drain_sleeve_after_exit(self: *const ProcessGuard, io: std.Io) void {
        if (comptime builtin.os.tag != .linux) return;
        const fd = self.launcher_pidfd orelse return;
        var pfd: [1]std.os.linux.pollfd = .{.{ .fd = fd, .events = std.os.linux.POLL.IN, .revents = 0 }};
        if (std.os.linux.poll(&pfd, pfd.len, 0) != 1) return;

        const path = self.sleeve_cgroup().?;
        const dir = std.Io.Dir.cwd().openDir(io, path, .{}) catch return;
        defer dir.close(io);
        kill_cgroup(io, dir);
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

    pub fn attach_with_io(self: *ProcessGuard, io: std.Io, child_id: ?std.process.Child.Id) void {
        self.attach(child_id);
        if (comptime builtin.os.tag == .linux) {
            const cid = child_id orelse return;
            const cg = self.cgroup_path orelse return;
            var dir = std.Io.Dir.cwd().openDir(io, cg, .{}) catch return;
            defer dir.close(io);
            var buf: [32]u8 = undefined;
            const s = std.fmt.bufPrint(&buf, "{d}\n", .{cid}) catch return;
            dir.writeFile(io, .{ .sub_path = "cgroup.procs", .data = s }) catch {};
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

    pub fn fence_and_confirm(self: *ProcessGuard, io: std.Io, timeout_ms: i64) bool {
        if (comptime builtin.os.tag == .windows) {
            const j = self.job orelse return true;
            _ = WinJob.TerminateJobObject(j, 1);
            const deadline = std.Io.Clock.awake.now(io).toMilliseconds() + @max(@as(i64, 100), timeout_ms);
            while (true) {
                var acct: WinJob.JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = .{};
                const ok = WinJob.QueryInformationJobObject(
                    j,
                    WinJob.JobObjectBasicAccountingInformation,
                    &acct,
                    @sizeOf(WinJob.JOBOBJECT_BASIC_ACCOUNTING_INFORMATION),
                    null,
                );
                if (ok == 0) return false;
                if (acct.ActiveProcesses == 0) return true;
                if (std.Io.Clock.awake.now(io).toMilliseconds() >= deadline) return false;
                _ = WinJob.TerminateJobObject(j, 1);
                io.sleep(.fromMilliseconds(20), .awake) catch return false;
            }
        }

        const deadline = std.Io.Clock.awake.now(io).toMilliseconds() + @max(@as(i64, 100), timeout_ms);
        while (true) {
            self.kill_all_posix(io);
            if (!self.any_alive_posix(io)) return true;
            if (std.Io.Clock.awake.now(io).toMilliseconds() >= deadline) return false;
            io.sleep(.fromMilliseconds(25), .awake) catch return false;
        }
    }

    fn kill_all_posix(self: *ProcessGuard, io: std.Io) void {
        if (comptime builtin.os.tag == .windows) return;
        self.terminate();
        if (comptime builtin.os.tag == .linux) {
            if (self.cgroup_path) |cg| {
                if (std.Io.Dir.cwd().openDir(io, cg, .{})) |dir_val| {
                    var dir = dir_val;
                    defer dir.close(io);
                    kill_cgroup(io, dir);
                    var procs_buf: [8192]u8 = undefined;
                    if (read_small_file_in_dir(io, dir, "cgroup.procs", &procs_buf)) |raw| {
                        var it = std.mem.tokenizeAny(u8, raw, " \t\r\n");
                        while (it.next()) |tok| {
                            const pid = std.fmt.parseInt(std.posix.pid_t, tok, 10) catch continue;
                            if (pid > 1) std.posix.kill(pid, .KILL) catch {};
                        }
                    }
                } else |_| {}
            }
            _ = scan_and_kill_proc(io, self.pgid, self.tag, true);
        }
    }

    fn any_alive_posix(self: *ProcessGuard, io: std.Io) bool {
        if (comptime builtin.os.tag == .windows) return false;
        if (comptime builtin.os.tag == .linux) {
            if (self.cgroup_path) |cg| {
                if (std.Io.Dir.cwd().openDir(io, cg, .{})) |dir_val| {
                    var dir = dir_val;
                    defer dir.close(io);
                    var pbuf: [2048]u8 = undefined;
                    if (read_small_file_in_dir(io, dir, "cgroup.procs", &pbuf)) |raw| {
                        if (std.mem.trim(u8, raw, " \t\r\n").len > 0) return true;
                    }
                    var ebuf: [512]u8 = undefined;
                    if (read_small_file_in_dir(io, dir, "cgroup.events", &ebuf)) |ev| {
                        if (std.mem.findPos(u8, ev, 0, "populated 1") != null) return true;
                    }
                } else |_| {}
            }
            if (scan_and_kill_proc(io, self.pgid, self.tag, false)) return true;
            return false;
        }
        if (self.pgid) |pid| {
            if (std.posix.kill(-pid, @as(std.posix.SIG, @fromBackingInt(@intCast(0))))) |_| {
                return true;
            } else |_| {}
        }
        return false;
    }

    pub fn deinit(self: *ProcessGuard) void {
        if (comptime builtin.os.tag == .windows) {
            if (self.job) |j| {
                std.os.windows.CloseHandle(j);
                self.job = null;
            }
        } else {
            self.pgid = null;
            if (comptime builtin.os.tag == .linux) {
                if (self.launcher_pidfd) |fd| {
                    _ = std.os.linux.close(fd);
                    self.launcher_pidfd = null;
                }
                if (self.cgroup_path) |cg| {
                    var z_buf: [std.fs.max_path_bytes]u8 = undefined;
                    if (cg.len < z_buf.len) {
                        @memcpy(z_buf[0..cg.len], cg);
                        z_buf[cg.len] = 0;
                        _ = std.os.linux.rmdir(@ptrCast(&z_buf));
                    }
                    if (self.gpa) |gpa| {
                        gpa.free(cg);
                    }
                    self.cgroup_path = null;
                }
            }
        }
        if (self.gpa) |gpa| {
            if (self.tag) |t| gpa.free(t);
        }
        self.tag = null;
        self.gpa = null;
    }
};

fn kill_cgroup(io: std.Io, dir: std.Io.Dir) void {
    dir.writeFile(io, .{ .sub_path = "cgroup.kill", .data = "1\n" }) catch {};
}

fn current_cgroup_path(io: std.Io, buf: []u8) ![]const u8 {
    var membership_buf: [std.fs.max_path_bytes]u8 = undefined;
    const membership = read_small_file_abs(io, "/proc/self/cgroup", &membership_buf) orelse return error.CgroupUnavailable;
    var lines = std.mem.splitScalar(u8, membership, '\n');
    while (lines.next()) |line| {
        const path = std.mem.cutPrefix(u8, line, "0::") orelse continue;
        if (std.mem.eql(u8, path, "/")) return "/sys/fs/cgroup";
        if (!spec.is_clean_abs(path)) return error.CgroupUnavailable;
        return std.fmt.bufPrint(buf, "/sys/fs/cgroup{s}", .{path});
    }
    return error.CgroupUnavailable;
}

fn oom_kill_count(io: std.Io, dir: std.Io.Dir) !u64 {
    var buf: [512]u8 = undefined;
    const events = read_small_file_in_dir(io, dir, "memory.events", &buf) orelse return error.MemoryEventsUnreadable;
    var lines = std.mem.splitScalar(u8, events, '\n');
    while (lines.next()) |line| {
        const count = std.mem.cutPrefix(u8, line, "oom_kill ") orelse continue;
        return std.fmt.parseInt(u64, count, 10);
    }
    return error.OomKillCounterMissing;
}

pub fn run_sleeve_cgroup(init: std.process.Init, args: []const []const u8) !u8 {
    if (comptime builtin.os.tag != .linux) return error.CgroupUnavailable;
    if (args.len < 2 or !spec.is_clean_abs(args[0]) or
        !std.mem.startsWith(u8, args[0], "/sys/fs/cgroup/") or
        !std.mem.startsWith(u8, std.fs.path.basename(args[0]), "pt-sleeve-")) return error.InvalidCgroup;

    const dir = try std.Io.Dir.cwd().openDir(init.io, args[0], .{});
    defer dir.close(init.io);
    try dir.writeFile(init.io, .{ .sub_path = "cgroup.procs", .data = "0\n" });
    return std.process.replace(init.io, .{ .argv = args[1..] });
}

fn read_small_file_abs(io: std.Io, path: []const u8, buf: []u8) ?[]const u8 {
    var f = std.Io.Dir.cwd().openFile(io, path, .{}) catch return null;
    defer f.close(io);
    const n = f.readPositionalAll(io, buf, 0) catch return null;
    return buf[0..n];
}

fn read_small_file_in_dir(io: std.Io, dir: std.Io.Dir, sub_path: []const u8, buf: []u8) ?[]const u8 {
    var f = dir.openFile(io, sub_path, .{}) catch return null;
    defer f.close(io);
    const n = f.readPositionalAll(io, buf, 0) catch return null;
    return buf[0..n];
}

fn create_job_cgroup(
    gpa: std.mem.Allocator,
    io: std.Io,
    tag: []const u8,
    memory_max_bytes: ?u64,
) ?[]u8 {
    if (comptime builtin.os.tag != .linux) return null;
    var path_buf: [std.fs.max_path_bytes]u8 = undefined;
    const cur_dir = current_cgroup_path(io, &path_buf) catch return null;

    var safe_buf: [48]u8 = undefined;
    var safe_len: usize = 0;
    for (tag) |c| {
        if (safe_len >= safe_buf.len) break;
        const ok = (c >= 'a' and c <= 'z') or (c >= 'A' and c <= 'Z') or (c >= '0' and c <= '9') or c == '.' or c == '_' or c == '-';
        safe_buf[safe_len] = if (ok) c else '_';
        safe_len += 1;
    }
    if (safe_len == 0) {
        safe_buf[0..3].* = "job".*;
        safe_len = 3;
    }
    const safe_tag = safe_buf[0..safe_len];
    const seq = next_cg_seq.fetchAdd(1, .monotonic);

    const cand1 = std.fmt.allocPrint(gpa, "{s}/pt-{d}-{s}", .{ cur_dir, seq, safe_tag }) catch return null;
    if (try_open_job_cgroup(io, cand1, memory_max_bytes)) return cand1;
    gpa.free(cand1);

    if (std.fs.path.dirname(cur_dir)) |parent| {
        if (std.mem.startsWith(u8, parent, "/sys/fs/cgroup/")) {
            const cand2 = std.fmt.allocPrint(gpa, "{s}/pt-{d}-{s}", .{ parent, seq, safe_tag }) catch return null;
            if (try_open_job_cgroup(io, cand2, memory_max_bytes)) return cand2;
            gpa.free(cand2);
        }
    }
    return null;
}

fn try_open_job_cgroup(io: std.Io, path: []const u8, memory_max_bytes: ?u64) bool {
    var d = std.Io.Dir.cwd().createDirPathOpen(io, path, .{}) catch return false;
    defer d.close(io);
    var pf = d.openFile(io, "cgroup.procs", .{ .mode = .read_write }) catch {
        var z_buf: [512]u8 = undefined;
        if (path.len < z_buf.len) {
            @memcpy(z_buf[0..path.len], path);
            z_buf[path.len] = 0;
            _ = std.os.linux.rmdir(@ptrCast(&z_buf));
        }
        return false;
    };
    pf.close(io);
    if (memory_max_bytes) |lim| {
        if (lim > 0) {
            var mbuf: [32]u8 = undefined;
            if (std.fmt.bufPrint(&mbuf, "{d}\n", .{lim})) |ms| {
                d.writeFile(io, .{ .sub_path = "memory.max", .data = ms }) catch {};
            } else |_| {}
        }
    }
    return true;
}

fn scan_and_kill_proc(
    io: std.Io,
    pgid_opt: ?std.posix.pid_t,
    tag_opt: ?[]const u8,
    do_kill: bool,
) bool {
    if (comptime builtin.os.tag != .linux) return false;
    var proc_dir = std.Io.Dir.cwd().openDir(io, "/proc", .{ .iterate = true }) catch return false;
    defer proc_dir.close(io);

    const self_pid: std.posix.pid_t = @intCast(std.os.linux.getpid());
    var found_live = false;
    var it = proc_dir.iterate();
    while (it.next(io) catch null) |ent| {
        if (ent.kind != .directory) continue;
        const pid = std.fmt.parseInt(std.posix.pid_t, ent.name, 10) catch continue;
        if (pid <= 1 or pid == self_pid) continue;

        var stat_path_buf: [64]u8 = undefined;
        const stat_sub = std.fmt.bufPrint(&stat_path_buf, "{d}/stat", .{pid}) catch continue;
        var stat_buf: [1024]u8 = undefined;
        const stat_raw = read_small_file_in_dir(io, proc_dir, stat_sub, &stat_buf) orelse continue;
        const rp = std.mem.findScalarLast(u8, stat_raw, ')') orelse continue;
        if (rp + 1 >= stat_raw.len) continue;
        var tok_it = std.mem.tokenizeScalar(u8, std.mem.trimStart(u8, stat_raw[rp + 1 ..], " "), ' ');
        const state_tok = tok_it.next() orelse continue;
        if (state_tok.len > 0 and state_tok[0] == 'Z') continue;
        _ = tok_it.next() orelse continue; // ppid
        const pgrp_tok = tok_it.next() orelse continue;
        const pgrp = std.fmt.parseInt(std.posix.pid_t, pgrp_tok, 10) catch -1;

        var matches = false;
        if (pgid_opt) |want_pgid| {
            if (pgrp == want_pgid) matches = true;
        }
        if (!matches) {
            if (tag_opt) |tag| {
                var env_path_buf: [64]u8 = undefined;
                const env_sub = std.fmt.bufPrint(&env_path_buf, "{d}/environ", .{pid}) catch continue;
                var env_buf: [16 * 1024]u8 = undefined;
                if (read_small_file_in_dir(io, proc_dir, env_sub, &env_buf)) |eraw| {
                    var needle_buf: [192]u8 = undefined;
                    if (std.fmt.bufPrint(&needle_buf, "PROTECTORATE_JOB_ID={s}", .{tag})) |needle| {
                        var eit = std.mem.splitScalar(u8, eraw, 0);
                        while (eit.next()) |kv| {
                            if (std.mem.eql(u8, kv, needle)) {
                                matches = true;
                                break;
                            }
                        }
                    } else |_| {}
                }
            }
        }
        if (matches) {
            found_live = true;
            if (do_kill) {
                std.posix.kill(pid, .KILL) catch {};
            } else {
                return true;
            }
        }
    }
    return found_live;
}

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
