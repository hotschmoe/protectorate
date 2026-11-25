const std = @import("std");
const builtin = @import("builtin");
const tiger = @import("../tiger.zig");
const msg = @import("../net/msg.zig");
const limits = @import("../net/limits.zig");
const s_exec = @import("../sandbox/exec.zig");
const admission = @import("admission.zig");

const Sha256 = std.crypto.hash.sha2.Sha256;

const ArtifactRec = struct {
    rel_path: []const u8,
    abs_path: []const u8,
    size_bytes: u64,
    sha256: [64]u8,
};

const JobEntry = struct {
    gpa: std.mem.Allocator,
    io: std.Io,
    runner: *Runner,
    id: []const u8,
    job_dir_path: []const u8,
    log_path: []const u8,
    cwd: []const u8,
    argv: []const []const u8,
    decl_artifacts: []const []const u8,
    timeout_secs: ?u64,

    exclusive: bool = false,
    slots: u32 = 1,
    long_slots: u32 = 0,
    mem_bytes: u64 = 0,
    lease_ttl_ms: ?i64 = null,
    wait_ms: ?i64 = null,
    parent_epoch: ?u64 = null,

    mu: std.Io.Mutex = .init,
    done_cond: std.Io.Condition = .init,
    state: msg.JobState = .queued,
    exit_code: ?i32 = null,
    err_msg: ?[]const u8 = null,
    held_reason: ?[]const u8 = null,
    log_bytes: u64 = 0,
    artifacts: std.ArrayList(ArtifactRec) = .empty,
    child_id: ?std.process.Child.Id = null,
    guard: s_exec.ProcessGuard = .{},
    guard_active: bool = false,
    fence_confirmed: bool = true,
    boot_id: ?[]u8 = null,
    epoch: ?u64 = null,
    lease_id: ?[]u8 = null,
    collecting: bool = false,
    cancel_requested: std.atomic.Value(bool) = .init(false),
    thread: ?std.Thread = null,
    started_ms: i64,

    fn info_snapshot(self: *JobEntry, arena: std.mem.Allocator) !msg.JobInfo {
        self.mu.lockUncancelable(self.io);
        defer self.mu.unlock(self.io);

        var arts = try arena.alloc(msg.JobArtifact, self.artifacts.items.len);
        for (self.artifacts.items, 0..) |ar, i| {
            arts[i] = .{
                .path = try arena.dupe(u8, ar.rel_path),
                .size_bytes = ar.size_bytes,
                .sha256 = try arena.dupe(u8, &ar.sha256),
            };
        }
        return .{
            .job_id = try arena.dupe(u8, self.id),
            .state = self.state,
            .exit_code = self.exit_code,
            .@"error" = if (self.err_msg) |e| try arena.dupe(u8, e) else null,
            .log_bytes = self.log_bytes,
            .artifacts = arts,
            .held_reason = if (self.held_reason) |h| try arena.dupe(u8, h) else null,
            .boot_id = if (self.boot_id) |b| try arena.dupe(u8, b) else null,
            .epoch = self.epoch,
            .lease_id = if (self.lease_id) |l| try arena.dupe(u8, l) else null,
            .collecting = self.collecting,
        };
    }

    fn deinit(self: *JobEntry) void {
        if (self.thread) |t| t.join();
        self.gpa.free(self.id);
        self.gpa.free(self.job_dir_path);
        self.gpa.free(self.log_path);
        self.gpa.free(self.cwd);
        for (self.argv) |arg| self.gpa.free(arg);
        self.gpa.free(self.argv);
        for (self.decl_artifacts) |a| self.gpa.free(a);
        self.gpa.free(self.decl_artifacts);
        if (self.err_msg) |e| self.gpa.free(e);
        if (self.boot_id) |b| self.gpa.free(b);
        if (self.lease_id) |l| self.gpa.free(l);
        for (self.artifacts.items) |ar| {
            self.gpa.free(ar.rel_path);
            self.gpa.free(ar.abs_path);
        }
        self.artifacts.deinit(self.gpa);
        self.gpa.destroy(self);
    }
};

pub const Runner = struct {
    gpa: std.mem.Allocator,
    io: std.Io,
    env: ?*const std.process.Environ.Map,
    admission_mgr: ?*admission.AdmissionManager,
    jobs_dir_path: []const u8,
    mu: std.Io.Mutex = .init,
    jobs: std.StringHashMapUnmanaged(*JobEntry) = .empty,

    pub fn init(
        gpa: std.mem.Allocator,
        io: std.Io,
        home_path: []const u8,
        env: ?*const std.process.Environ.Map,
        adm_mgr: ?*admission.AdmissionManager,
    ) !*Runner {
        const jdir = try std.fs.path.join(gpa, &.{ home_path, "jobs" });
        errdefer gpa.free(jdir);
        try std.Io.Dir.cwd().createDirPath(io, jdir);
        const self = try gpa.create(Runner);
        self.* = .{
            .gpa = gpa,
            .io = io,
            .env = env,
            .admission_mgr = adm_mgr,
            .jobs_dir_path = jdir,
        };
        if (adm_mgr) |am| {
            am.set_fence_callback(self, fence_job_cb);
        }
        return self;
    }

    pub fn deinit(self: *Runner) void {
        self.mu.lockUncancelable(self.io);
        var list: std.ArrayList(*JobEntry) = .empty;
        var it = self.jobs.iterator();
        while (it.next()) |e| {
            list.append(self.gpa, e.value_ptr.*) catch {};
        }
        self.jobs.deinit(self.gpa);
        self.mu.unlock(self.io);

        for (list.items) |job| {
            job.cancel_requested.store(true, .release);
            kill_job_proc(job);
            job.deinit();
        }
        list.deinit(self.gpa);
        self.gpa.free(self.jobs_dir_path);
        self.gpa.destroy(self);
    }

    pub fn submit(self: *Runner, arena: std.mem.Allocator, req: msg.SubmitJob) !msg.JobInfo {
        if (req.argv.len == 0) return error.EmptyArgv;

        self.mu.lockUncancelable(self.io);
        if (self.jobs.get(req.job_id)) |existing| {
            self.mu.unlock(self.io);
            return existing.info_snapshot(arena);
        }
        self.mu.unlock(self.io);

        const job_dir = try std.fs.path.join(self.gpa, &.{ self.jobs_dir_path, req.job_id });
        errdefer self.gpa.free(job_dir);
        try std.Io.Dir.cwd().createDirPath(self.io, job_dir);

        const log_path = try std.fs.path.join(self.gpa, &.{ job_dir, "output.log" });
        errdefer self.gpa.free(log_path);
        try std.Io.Dir.cwd().writeFile(self.io, .{ .sub_path = log_path, .data = "" });

        const cwd_str = if (req.cwd) |c|
            try self.gpa.dupe(u8, c)
        else
            try self.gpa.dupe(u8, job_dir);
        errdefer self.gpa.free(cwd_str);

        var argv_dup = try self.gpa.alloc([]const u8, req.argv.len);
        var argc_built: usize = 0;
        errdefer {
            for (argv_dup[0..argc_built]) |s| self.gpa.free(s);
            self.gpa.free(argv_dup);
        }
        for (req.argv, 0..) |s, i| {
            argv_dup[i] = try self.gpa.dupe(u8, s);
            argc_built += 1;
        }

        var arts_dup = try self.gpa.alloc([]const u8, req.artifacts.len);
        var artc_built: usize = 0;
        errdefer {
            for (arts_dup[0..artc_built]) |s| self.gpa.free(s);
            self.gpa.free(arts_dup);
        }
        for (req.artifacts, 0..) |s, i| {
            arts_dup[i] = try self.gpa.dupe(u8, s);
            artc_built += 1;
        }

        const je = try self.gpa.create(JobEntry);
        errdefer self.gpa.destroy(je);
        je.* = .{
            .gpa = self.gpa,
            .io = self.io,
            .runner = self,
            .id = try self.gpa.dupe(u8, req.job_id),
            .job_dir_path = job_dir,
            .log_path = log_path,
            .cwd = cwd_str,
            .argv = argv_dup,
            .decl_artifacts = arts_dup,
            .timeout_secs = req.timeout_secs,
            .exclusive = req.exclusive,
            .slots = req.slots,
            .long_slots = req.long_slots,
            .mem_bytes = req.mem_bytes,
            .lease_ttl_ms = req.lease_ttl_ms,
            .wait_ms = req.wait_ms,
            .parent_epoch = req.parent_epoch,
            .state = if (self.admission_mgr != null) .queued else .running,
            .held_reason = if (self.admission_mgr != null) "admission_queue" else null,
            .started_ms = std.Io.Clock.real.now(self.io).toMilliseconds(),
        };

        self.mu.lockUncancelable(self.io);
        try self.jobs.put(self.gpa, je.id, je);
        self.mu.unlock(self.io);

        je.thread = try std.Thread.spawn(.{}, run_job_thread, .{je});
        return je.info_snapshot(arena);
    }

    pub fn get_status(self: *Runner, arena: std.mem.Allocator, job_id: []const u8) !?msg.JobInfo {
        self.mu.lockUncancelable(self.io);
        const je = self.jobs.get(job_id);
        self.mu.unlock(self.io);
        const job = je orelse return null;
        self.renew_if_collecting(arena, job);
        return try job.info_snapshot(arena);
    }

    pub fn list_all(self: *Runner, arena: std.mem.Allocator) ![]msg.JobInfo {
        self.mu.lockUncancelable(self.io);
        var refs: std.ArrayList(*JobEntry) = .empty;
        defer refs.deinit(arena);
        var it = self.jobs.iterator();
        while (it.next()) |e| {
            try refs.append(arena, e.value_ptr.*);
        }
        self.mu.unlock(self.io);

        var out = try arena.alloc(msg.JobInfo, refs.items.len);
        for (refs.items, 0..) |je, i| {
            out[i] = try je.info_snapshot(arena);
        }
        return out;
    }

    pub fn cancel(self: *Runner, arena: std.mem.Allocator, job_id: []const u8) !?msg.JobInfo {
        self.mu.lockUncancelable(self.io);
        const je = self.jobs.get(job_id);
        self.mu.unlock(self.io);
        const job = je orelse return null;
        job.cancel_requested.store(true, .release);
        kill_job_proc(job);
        return try job.info_snapshot(arena);
    }

    pub fn ack_job(self: *Runner, arena: std.mem.Allocator, job_id: []const u8) !?msg.AckJobReply {
        self.mu.lockUncancelable(self.io);
        const je = self.jobs.get(job_id);
        self.mu.unlock(self.io);
        const job = je orelse return null;

        var lid_opt: ?[]u8 = null;
        var ep_opt: ?u64 = null;
        {
            job.mu.lockUncancelable(self.io);
            if (job.lease_id) |lid| lid_opt = self.gpa.dupe(u8, lid) catch null;
            ep_opt = job.epoch;
            job.collecting = false;
            job.mu.unlock(self.io);
        }
        defer if (lid_opt) |l| self.gpa.free(l);

        if (self.admission_mgr) |am| {
            if (lid_opt != null and ep_opt != null) {
                const rr = try am.release(arena, lid_opt.?, ep_opt.?);
                return .{
                    .ok = rr.ok,
                    .fenced = rr.fenced,
                    .quarantined = rr.quarantined,
                };
            }
            return .{
                .ok = true,
                .fenced = true,
                .quarantined = am.quarantined_copy(arena),
            };
        }
        return .{ .ok = true, .fenced = true };
    }

    pub fn read_log(
        self: *Runner,
        arena: std.mem.Allocator,
        job_id: []const u8,
        offset: u64,
        limit: usize,
    ) !?struct { data: []const u8, eof: bool, next_offset: u64 } {
        self.mu.lockUncancelable(self.io);
        const je = self.jobs.get(job_id);
        self.mu.unlock(self.io);
        const job = je orelse return null;
        self.renew_if_collecting(arena, job);

        var f = std.Io.Dir.cwd().openFile(self.io, job.log_path, .{}) catch return null;
        defer f.close(self.io);
        const st = try f.stat(self.io);
        if (offset >= st.size) {
            job.mu.lockUncancelable(self.io);
            const finished = is_terminal(job.state);
            job.mu.unlock(self.io);
            return .{ .data = "", .eof = finished, .next_offset = st.size };
        }
        const avail: usize = @intCast(@min(st.size - offset, @as(u64, @min(limit, limits.piece_max))));
        const buf = try arena.alloc(u8, avail);
        const got = try f.readPositionalAll(self.io, buf, offset);
        const next_off = offset + got;
        job.mu.lockUncancelable(self.io);
        const finished = is_terminal(job.state) and next_off >= st.size;
        job.mu.unlock(self.io);
        return .{ .data = buf[0..got], .eof = finished, .next_offset = next_off };
    }

    pub fn read_artifact(
        self: *Runner,
        arena: std.mem.Allocator,
        job_id: []const u8,
        rel_path: []const u8,
        offset: u64,
        limit: usize,
    ) !?struct { data: []const u8, eof: bool, next_offset: u64, size_bytes: u64, sha256: []const u8 } {
        self.mu.lockUncancelable(self.io);
        const je = self.jobs.get(job_id);
        self.mu.unlock(self.io);
        const job = je orelse return null;
        self.renew_if_collecting(arena, job);

        job.mu.lockUncancelable(self.io);
        var match_abs: ?[]const u8 = null;
        var sha_hex: [64]u8 = undefined;
        var total_size: u64 = 0;
        for (job.artifacts.items) |ar| {
            if (std.mem.eql(u8, ar.rel_path, rel_path)) {
                match_abs = try arena.dupe(u8, ar.abs_path);
                sha_hex = ar.sha256;
                total_size = ar.size_bytes;
                break;
            }
        }
        job.mu.unlock(self.io);

        const abs = match_abs orelse return null;
        var f = std.Io.Dir.cwd().openFile(self.io, abs, .{}) catch return null;
        defer f.close(self.io);
        const hex_dup = try arena.dupe(u8, &sha_hex);
        if (offset >= total_size) {
            return .{ .data = "", .eof = true, .next_offset = total_size, .size_bytes = total_size, .sha256 = hex_dup };
        }
        const avail: usize = @intCast(@min(total_size - offset, @as(u64, @min(limit, limits.piece_max))));
        const buf = try arena.alloc(u8, avail);
        const got = try f.readPositionalAll(self.io, buf, offset);
        const next_off = offset + got;
        return .{
            .data = buf[0..got],
            .eof = next_off >= total_size,
            .next_offset = next_off,
            .size_bytes = total_size,
            .sha256 = hex_dup,
        };
    }

    fn renew_if_collecting(self: *Runner, arena: std.mem.Allocator, job: *JobEntry) void {
        const am = self.admission_mgr orelse return;
        var lid_opt: ?[]const u8 = null;
        var ep_opt: ?u64 = null;
        {
            job.mu.lockUncancelable(self.io);
            if (job.collecting and job.lease_id != null and job.epoch != null) {
                lid_opt = arena.dupe(u8, job.lease_id.?) catch null;
                ep_opt = job.epoch;
            }
            job.mu.unlock(self.io);
        }
        if (lid_opt != null and ep_opt != null) {
            _ = am.renew(arena, .{ .lease_id = lid_opt.?, .epoch = ep_opt.? }) catch {};
        }
    }

    fn fence_job_cb(
        ctx: ?*anyopaque,
        io: std.Io,
        lease_id: []const u8,
        epoch: u64,
        job_id_opt: ?[]const u8,
    ) bool {
        const self: *Runner = @ptrCast(@alignCast(ctx orelse return true));
        const jid = job_id_opt orelse return true;
        _ = lease_id;
        _ = epoch;

        self.mu.lockUncancelable(io);
        const je = self.jobs.get(jid);
        self.mu.unlock(io);
        const job = je orelse return true;

        job.cancel_requested.store(true, .release);
        job.mu.lockUncancelable(io);
        defer job.mu.unlock(io);
        if (job.guard_active) {
            job.guard.terminate();
            const deadline = std.Io.Clock.awake.now(io).toMilliseconds() + 3_500;
            while (job.guard_active) {
                const now = std.Io.Clock.awake.now(io).toMilliseconds();
                if (now >= deadline) return false;
                job.done_cond.waitTimeout(io, &job.mu, .{
                    .duration = .{ .raw = .fromMilliseconds(@min(200, deadline - now)), .clock = .awake },
                }) catch {};
            }
        }
        return job.fence_confirmed;
    }
};

fn is_terminal(st: msg.JobState) bool {
    return switch (st) {
        .succeeded, .failed, .interrupted, .cancelled, .abandoned, .lost => true,
        else => false,
    };
}

fn kill_job_proc(job: *JobEntry) void {
    job.mu.lockUncancelable(job.io);
    job.guard.terminate();
    job.mu.unlock(job.io);
}

fn run_job_thread(job: *JobEntry) void {
    var arena_st = std.heap.ArenaAllocator.init(job.gpa);
    defer arena_st.deinit();
    const arena = arena_st.allocator();

    var granted_lid: ?[]u8 = null;
    var granted_ep: ?u64 = null;
    var granted_boot: ?[]u8 = null;

    if (job.runner.admission_mgr) |am| {
        const wait_ms: i64 = job.wait_ms orelse if (job.timeout_secs) |ts| @intCast(@min(ts * 1000, 3_600_000)) else 3_600_000;
        const rep = am.acquire_blocking(arena, .{
            .holder_id = job.id,
            .job_id = job.id,
            .exclusive = job.exclusive,
            .slots = job.slots,
            .long_slots = job.long_slots,
            .mem_bytes = job.mem_bytes,
            .ttl_ms = job.lease_ttl_ms,
            .wait_ms = wait_ms,
            .parent_epoch = job.parent_epoch,
        }, job.id) catch |err| {
            job.mu.lockUncancelable(job.io);
            job.state = .failed;
            job.err_msg = job.gpa.dupe(u8, @errorName(err)) catch null;
            job.held_reason = "admission_error";
            job.mu.unlock(job.io);
            return;
        };

        if (!rep.ok) {
            job.mu.lockUncancelable(job.io);
            job.state = if (job.cancel_requested.load(.acquire)) .cancelled else .failed;
            job.err_msg = job.gpa.dupe(u8, rep.reason) catch null;
            job.held_reason = "admission_refused";
            job.mu.unlock(job.io);
            return;
        }

        granted_lid = job.gpa.dupe(u8, rep.lease_id) catch null;
        granted_ep = rep.epoch;
        granted_boot = job.gpa.dupe(u8, rep.boot_id) catch null;

        job.mu.lockUncancelable(job.io);
        job.lease_id = granted_lid;
        job.epoch = granted_ep;
        job.boot_id = granted_boot;
        job.held_reason = null;
        if (job.cancel_requested.load(.acquire)) {
            job.state = .cancelled;
            job.mu.unlock(job.io);
            if (granted_lid != null and granted_ep != null) {
                _ = am.release(arena, granted_lid.?, granted_ep.?) catch {};
            }
            return;
        }
        job.state = .running;
        job.mu.unlock(job.io);
    }

    var child_env = if (job.runner.env) |base_env|
        (base_env.clone(arena) catch std.process.Environ.Map.init(arena))
    else
        std.process.Environ.Map.init(arena);
    child_env.put("PROTECTORATE_JOB_ID", job.id) catch {};
    if (granted_boot) |bid| child_env.put("PROTECTORATE_BOOT_ID", bid) catch {};
    if (granted_lid) |lid| child_env.put("PROTECTORATE_LEASE_ID", lid) catch {};
    if (granted_ep) |ep| {
        if (std.fmt.allocPrint(arena, "{d}", .{ep})) |es| {
            child_env.put("PROTECTORATE_LEASE_EPOCH", es) catch {};
        } else |_| {}
    }
    if (job.runner.admission_mgr) |am| {
        if (am.socket_path()) |sp| {
            child_env.put("PROTECTORATE_ADMISSION_SOCK", sp) catch {};
        }
    }

    var guard = s_exec.ProcessGuard.init_fenced(
        job.gpa,
        job.io,
        job.id,
        if (job.mem_bytes > 0) job.mem_bytes else null,
    );

    var child = std.process.spawn(job.io, .{
        .argv = job.argv,
        .cwd = .{ .path = job.cwd },
        .environ_map = &child_env,
        .stdin = .ignore,
        .stdout = .pipe,
        .stderr = .pipe,
        .pgid = if (comptime builtin.os.tag != .windows) 0 else null,
    }) catch |err| {
        guard.deinit();
        if (job.runner.admission_mgr) |am| {
            if (granted_lid != null and granted_ep != null) {
                _ = am.release(arena, granted_lid.?, granted_ep.?) catch {};
            }
        }
        job.mu.lockUncancelable(job.io);
        job.state = .failed;
        job.err_msg = job.gpa.dupe(u8, @errorName(err)) catch null;
        job.mu.unlock(job.io);
        return;
    };

    guard.attach_with_io(job.io, child.id);
    {
        job.mu.lockUncancelable(job.io);
        job.child_id = child.id;
        job.guard = guard;
        job.guard_active = true;
        if (job.cancel_requested.load(.acquire)) {
            job.guard.terminate();
        }
        job.mu.unlock(job.io);
    }

    var proc_done: std.atomic.Value(bool) = .init(false);
    const RenewCtx = struct {
        job: *JobEntry,
        proc_done: *std.atomic.Value(bool),
        lid: ?[]const u8,
        ep: ?u64,
    };
    const rctx = RenewCtx{
        .job = job,
        .proc_done = &proc_done,
        .lid = granted_lid,
        .ep = granted_ep,
    };
    const renew_fn = struct {
        fn run(rc: RenewCtx) void {
            const ttl = rc.job.lease_ttl_ms orelse 30_000;
            const interval_ms = @max(@as(i64, 1_000), @min(@as(i64, 10_000), @divFloor(ttl, 3)));
            var last_renew = std.Io.Clock.awake.now(rc.job.io).toMilliseconds();
            const start_ms = last_renew;
            var a_st = std.heap.ArenaAllocator.init(rc.job.gpa);
            defer a_st.deinit();

            while (!rc.proc_done.load(.acquire)) {
                rc.job.io.sleep(.fromMilliseconds(250), .awake) catch break;
                if (rc.proc_done.load(.acquire)) break;
                const now = std.Io.Clock.awake.now(rc.job.io).toMilliseconds();

                if (rc.job.timeout_secs) |ts| {
                    if (now - start_ms >= @as(i64, @intCast(ts * 1000))) {
                        rc.job.cancel_requested.store(true, .release);
                        kill_job_proc(rc.job);
                        break;
                    }
                }
                if (rc.job.runner.admission_mgr) |am| {
                    if (rc.lid != null and rc.ep != null and now - last_renew >= interval_ms) {
                        _ = a_st.reset(.retain_capacity);
                        const rep = am.renew(a_st.allocator(), .{ .lease_id = rc.lid.?, .epoch = rc.ep.? }) catch null;
                        if (rep == null or !rep.?.ok) {
                            rc.job.cancel_requested.store(true, .release);
                            kill_job_proc(rc.job);
                            break;
                        }
                        last_renew = now;
                    }
                }
            }
        }
    }.run;
    const renew_t = std.Thread.spawn(.{}, renew_fn, .{rctx}) catch null;

    const log_file = std.Io.Dir.cwd().openFile(job.io, job.log_path, .{ .mode = .read_write }) catch null;
    defer if (log_file) |lf| lf.close(job.io);

    const PumpCtx = struct {
        job: *JobEntry,
        src: std.Io.File,
        log_f: ?std.Io.File,
    };

    const pump_fn = struct {
        fn run(pctx: PumpCtx) void {
            var tmp: [4096]u8 = undefined;
            const s: []u8 = &tmp;
            const bufs = [_][]u8{s};
            while (true) {
                const n = pctx.src.readStreaming(pctx.job.io, &bufs) catch break;
                if (n == 0) break;
                pctx.job.mu.lockUncancelable(pctx.job.io);
                if (pctx.log_f) |lf| {
                    lf.writePositionalAll(pctx.job.io, tmp[0..n], pctx.job.log_bytes) catch {};
                }
                pctx.job.log_bytes += n;
                pctx.job.mu.unlock(pctx.job.io);
            }
        }
    }.run;

    const out_t = if (child.stdout) |out_f|
        std.Thread.spawn(.{}, pump_fn, .{PumpCtx{ .job = job, .src = out_f, .log_f = log_file }}) catch null
    else
        null;
    const err_t = if (child.stderr) |err_f|
        std.Thread.spawn(.{}, pump_fn, .{PumpCtx{ .job = job, .src = err_f, .log_f = log_file }}) catch null
    else
        null;

    if (out_t) |t| t.join();
    if (err_t) |t| t.join();

    const term = child.wait(job.io) catch null;
    proc_done.store(true, .release);
    if (renew_t) |t| t.join();

    var fence_guard: s_exec.ProcessGuard = .{};
    {
        job.mu.lockUncancelable(job.io);
        fence_guard = job.guard;
        job.mu.unlock(job.io);
    }
    const fenced_ok = fence_guard.fence_and_confirm(job.io, 3_000);

    collect_artifacts(job);

    if (!fenced_ok) {
        if (job.runner.admission_mgr) |am| {
            if (granted_lid != null and granted_ep != null) {
                am.fail_fence_now(granted_lid.?, granted_ep.?, "job descendant failed to fence");
            }
        }
    } else if (job.runner.admission_mgr) |am| {
        if (granted_lid != null and granted_ep != null) {
            am.begin_collecting(granted_lid.?, granted_ep.?);
        }
    }

    job.mu.lockUncancelable(job.io);
    job.child_id = null;
    job.guard = .{};
    job.guard_active = false;
    job.fence_confirmed = fenced_ok;
    if (job.runner.admission_mgr != null and fenced_ok and granted_lid != null) {
        job.collecting = true;
    }
    if (job.cancel_requested.load(.acquire)) {
        job.state = .cancelled;
    } else if (term) |t| {
        switch (t) {
            .exited => |code| {
                job.exit_code = @intCast(code);
                job.state = if (code == 0) .succeeded else .failed;
            },
            .signal => |sig| {
                job.exit_code = 128 + @as(i32, @intCast(@intFromEnum(sig)));
                job.state = .interrupted;
            },
            else => job.state = .failed,
        }
    } else {
        job.state = .failed;
    }
    job.done_cond.broadcast(job.io);
    job.mu.unlock(job.io);
    fence_guard.deinit();
}

fn collect_artifacts(job: *JobEntry) void {
    for (job.decl_artifacts) |rel| {
        if (std.fs.path.isAbsolute(rel)) continue;
        if (std.mem.findPos(u8, rel, 0, "..") != null) continue;
        const cand = std.fs.path.join(job.gpa, &.{ job.cwd, rel }) catch continue;
        var file_opt: ?std.Io.File = std.Io.Dir.cwd().openFile(job.io, cand, .{}) catch null;
        var chosen_path = cand;
        if (file_opt == null) {
            job.gpa.free(cand);
            const alt = std.fs.path.join(job.gpa, &.{ job.job_dir_path, rel }) catch continue;
            file_opt = std.Io.Dir.cwd().openFile(job.io, alt, .{}) catch {
                job.gpa.free(alt);
                continue;
            };
            chosen_path = alt;
        }
        defer file_opt.?.close(job.io);

        var hasher = Sha256.init(.{});
        var total: u64 = 0;
        var buf: [32 * 1024]u8 = undefined;
        while (true) {
            const n = file_opt.?.readPositionalAll(job.io, &buf, total) catch break;
            if (n == 0) break;
            hasher.update(buf[0..n]);
            total += n;
        }
        var digest: [32]u8 = undefined;
        hasher.final(&digest);
        const chars = "0123456789abcdef";
        var hex: [64]u8 = undefined;
        for (digest, 0..) |b, i| {
            hex[i * 2] = chars[b >> 4];
            hex[i * 2 + 1] = chars[b & 0x0f];
        }

        const rel_d = job.gpa.dupe(u8, rel) catch {
            job.gpa.free(chosen_path);
            continue;
        };
        job.mu.lockUncancelable(job.io);
        job.artifacts.append(job.gpa, .{
            .rel_path = rel_d,
            .abs_path = chosen_path,
            .size_bytes = total,
            .sha256 = hex,
        }) catch {
            job.gpa.free(rel_d);
            job.gpa.free(chosen_path);
        };
        job.mu.unlock(job.io);
    }
}
