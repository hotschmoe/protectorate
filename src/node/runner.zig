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
    proc_exited: std.atomic.Value(bool) = .init(false),
    kill_issued: bool = false,
    starting: bool = true,
    log_ready: bool = false,
    start_cond: std.Io.Condition = .init,
    run_started_ms: i64 = 0,
    last_renew_ms: i64 = 0,

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
    supervisor: ?std.Thread = null,
    supervisor_stop: std.atomic.Value(bool) = .init(false),

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

    /// Kills every running job's process tree without freeing anything: the normal teardown's effect on jobs, for a
    /// process that must exit while a worker may still reach the runner.
    pub fn fence_all(self: *Runner) void {
        self.mu.lockUncancelable(self.io);
        var list: std.ArrayList(*JobEntry) = .empty;
        defer list.deinit(self.gpa);
        var it = self.jobs.iterator();
        while (it.next()) |e| list.append(self.gpa, e.value_ptr.*) catch break;
        self.mu.unlock(self.io);
        for (list.items) |job| cancel_live(job);
    }

    pub fn deinit(self: *Runner) void {
        self.supervisor_stop.store(true, .release);
        if (self.supervisor) |t| t.join();
        self.mu.lockUncancelable(self.io);
        var list: std.ArrayList(*JobEntry) = .empty;
        var it = self.jobs.iterator();
        while (it.next()) |e| {
            list.append(self.gpa, e.value_ptr.*) catch {};
        }
        self.jobs.deinit(self.gpa);
        self.mu.unlock(self.io);

        for (list.items) |job| {
            cancel_live(job);
            job.deinit();
        }
        list.deinit(self.gpa);
        self.gpa.free(self.jobs_dir_path);
        self.gpa.destroy(self);
    }

    pub fn submit(self: *Runner, arena: std.mem.Allocator, req: msg.SubmitJob) !msg.JobInfo {
        if (req.argv.len == 0) return error.EmptyArgv;

        if (try self.known(arena, req.job_id)) |info| return info;

        const je = try self.make_entry(req);
        self.mu.lockUncancelable(self.io);
        if (self.jobs.get(req.job_id) != null) {
            self.mu.unlock(self.io);
            je.deinit();
            return (try self.known(arena, req.job_id)).?;
        }
        self.jobs.put(self.gpa, je.id, je) catch |err| {
            self.mu.unlock(self.io);
            je.deinit();
            return err;
        };
        self.mu.unlock(self.io);

        // From here the entry is published and is never freed before deinit: a failure turns it into a failed job.
        self.start_entry(je) catch |err| self.fail_start(je, err);
        self.ensure_supervisor();
        return je.info_snapshot(arena);
    }

    /// A job already in the table, once it has left its starting state (waiting a bounded time for that).
    fn known(self: *Runner, arena: std.mem.Allocator, job_id: []const u8) !?msg.JobInfo {
        self.mu.lockUncancelable(self.io);
        const existing = self.jobs.get(job_id);
        self.mu.unlock(self.io);
        const job = existing orelse return null;
        var waited: i64 = 0;
        job.mu.lockUncancelable(self.io);
        while (job.starting and waited < start_wait_ms) : (waited += 100) {
            job.start_cond.waitTimeout(self.io, &job.mu, .{
                .duration = .{ .raw = .fromMilliseconds(100), .clock = .awake },
            }) catch {};
        }
        job.mu.unlock(self.io);
        return try job.info_snapshot(arena);
    }

    fn start_entry(self: *Runner, je: *JobEntry) !void {
        try std.Io.Dir.cwd().createDirPath(self.io, je.job_dir_path);
        try std.Io.Dir.cwd().writeFile(self.io, .{ .sub_path = je.log_path, .data = "" });
        je.mu.lockUncancelable(self.io);
        je.log_ready = true;
        je.state = if (self.admission_mgr != null) .queued else .running;
        je.held_reason = if (self.admission_mgr != null) "admission_queue" else null;
        je.mu.unlock(self.io);
        je.thread = try std.Thread.spawn(.{}, run_job_thread, .{je});
        je.mu.lockUncancelable(self.io);
        je.starting = false;
        je.start_cond.broadcast(self.io);
        je.mu.unlock(self.io);
    }

    fn fail_start(self: *Runner, je: *JobEntry, err: anyerror) void {
        var note_buf: [128]u8 = undefined;
        const note = std.fmt.bufPrint(&note_buf, "node: job did not start: {s}\n", .{@errorName(err)}) catch "node: job did not start\n";
        je.mu.lockUncancelable(self.io);
        defer je.mu.unlock(self.io);
        if (je.log_ready) {
            std.Io.Dir.cwd().writeFile(self.io, .{ .sub_path = je.log_path, .data = note }) catch {};
            je.log_bytes = note.len;
        }
        je.state = .failed;
        je.held_reason = null;
        je.err_msg = je.gpa.dupe(u8, @errorName(err)) catch null;
        je.starting = false;
        je.start_cond.broadcast(self.io);
        je.done_cond.broadcast(self.io);
    }

    fn make_entry(self: *Runner, req: msg.SubmitJob) !*JobEntry {
        const job_dir = try std.fs.path.join(self.gpa, &.{ self.jobs_dir_path, req.job_id });
        errdefer self.gpa.free(job_dir);

        const log_path = try std.fs.path.join(self.gpa, &.{ job_dir, "output.log" });
        errdefer self.gpa.free(log_path);

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
            .state = .queued,
            .held_reason = "starting",
            .started_ms = std.Io.Clock.real.now(self.io).toMilliseconds(),
        };

        return je;
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
        cancel_live(job);
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

        var f = std.Io.Dir.cwd().openFile(self.io, job.log_path, .{}) catch {
            job.mu.lockUncancelable(self.io);
            const starting = job.starting;
            job.mu.unlock(self.io);
            if (starting) return .{ .data = "", .eof = false, .next_offset = offset };
            return null;
        };
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

    fn ensure_supervisor(self: *Runner) void {
        self.mu.lockUncancelable(self.io);
        defer self.mu.unlock(self.io);
        if (self.supervisor == null) self.supervisor = std.Thread.spawn(.{}, supervise_jobs, .{self}) catch null;
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
            job.kill_issued = true;
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
        }, job.id, null) catch |err| {
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
        job.run_started_ms = std.Io.Clock.awake.now(job.io).toMilliseconds();
        job.last_renew_ms = job.run_started_ms;
        if (job.cancel_requested.load(.acquire)) {
            job.guard.terminate();
        }
        job.mu.unlock(job.io);
    }

    const log_file = std.Io.Dir.cwd().openFile(job.io, job.log_path, .{ .mode = .read_write }) catch null;
    defer if (log_file) |lf| lf.close(job.io);

    pump_output(job, &child, log_file);

    const term = child.wait(job.io) catch null;
    job.mu.lockUncancelable(job.io);
    job.proc_exited.store(true, .release);
    job.mu.unlock(job.io);

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
    // The wait status decides: a process that ended on its own keeps its own result, and only one our kill ended is cancelled.
    const killed = if (term) |t| (if (comptime builtin.os.tag == .windows) job.kill_issued else t == .signal) else true;
    if (job.cancel_requested.load(.acquire) and killed) {
        job.state = .cancelled;
    } else if (term) |t| {
        switch (t) {
            .exited => |code| {
                job.exit_code = @intCast(code);
                job.state = if (code == 0) .succeeded else .failed;
            },
            .signal => |sig| {
                job.exit_code = 128 + @as(i32, @intCast(@backingInt(sig)));
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

const pipe_chunk = 4096;
const start_wait_ms = 5_000;

/// Copies a job's stdout and stderr into its log from this one thread until both pipes close.
fn pump_output(job: *JobEntry, child: *std.process.Child, log_file: ?std.Io.File) void {
    const io = job.io;
    var buffers: [2][pipe_chunk]u8 = undefined;
    var vectors: [2][1][]u8 = undefined;
    var storage: [2]std.Io.Operation.Storage = undefined;
    var batch = std.Io.Batch.init(&storage);
    defer batch.cancel(io);
    var open: [2]bool = .{ false, false };
    const files = [2]?std.Io.File{ child.stdout, child.stderr };
    for (files, 0..) |maybe, i| {
        const file = maybe orelse continue;
        vectors[i] = .{&buffers[i]};
        batch.addAt(@intCast(i), .{ .file_read_streaming = .{ .file = file, .data = &vectors[i] } });
        open[i] = true;
    }
    while (open[0] or open[1]) {
        batch.awaitConcurrent(io, .none) catch return;
        while (batch.next()) |completion| {
            const i = completion.index;
            const n = completion.result.file_read_streaming catch 0;
            if (n == 0) {
                open[i] = false;
                continue;
            }
            job.mu.lockUncancelable(io);
            if (log_file) |lf| {
                lf.writePositionalAll(io, buffers[i][0..n], job.log_bytes) catch {};
            }
            job.log_bytes += n;
            job.mu.unlock(io);
            batch.addAt(i, .{ .file_read_streaming = .{ .file = files[i].?, .data = &vectors[i] } });
        }
    }
}

const supervise_tick_ms = 250;

/// One thread for every running job: enforces job timeouts and keeps admission leases renewed.
fn supervise_jobs(self: *Runner) void {
    var arena_st = std.heap.ArenaAllocator.init(self.gpa);
    defer arena_st.deinit();
    while (!self.supervisor_stop.load(.acquire)) {
        self.io.sleep(.fromMilliseconds(supervise_tick_ms), .awake) catch return;
        _ = arena_st.reset(.retain_capacity);
        const arena = arena_st.allocator();
        var live: std.ArrayList(*JobEntry) = .empty;
        self.mu.lockUncancelable(self.io);
        var it = self.jobs.iterator();
        while (it.next()) |e| live.append(arena, e.value_ptr.*) catch break;
        self.mu.unlock(self.io);
        for (live.items) |job| supervise_one(self, arena, job);
    }
}

/// Cancels a job only while its process has not exited, so a slow fence or artifact collection cannot turn a finished run into a cancelled one.
fn cancel_live(job: *JobEntry) void {
    job.mu.lockUncancelable(job.io);
    const live = !job.proc_exited.load(.acquire);
    if (live) {
        job.cancel_requested.store(true, .release);
        job.kill_issued = true;
    }
    job.mu.unlock(job.io);
    if (live) kill_job_proc(job);
}

fn supervise_one(self: *Runner, arena: std.mem.Allocator, job: *JobEntry) void {
    if (job.cancel_requested.load(.acquire) or job.proc_exited.load(.acquire)) return;
    job.mu.lockUncancelable(job.io);
    const active = job.guard_active;
    const started = job.run_started_ms;
    const last_renew = job.last_renew_ms;
    const lease_id = job.lease_id;
    const epoch = job.epoch;
    job.mu.unlock(job.io);
    if (!active) return;
    const now = std.Io.Clock.awake.now(job.io).toMilliseconds();
    if (job.timeout_secs) |ts| {
        if (now - started >= @as(i64, @intCast(ts * 1000))) return cancel_live(job);
    }
    const am = self.admission_mgr orelse return;
    const lid = lease_id orelse return;
    const ep = epoch orelse return;
    const ttl = job.lease_ttl_ms orelse 30_000;
    const interval_ms = @max(@as(i64, 1_000), @min(@as(i64, 10_000), @divFloor(ttl, 3)));
    if (now - last_renew < interval_ms) return;
    const rep = am.renew(arena, .{ .lease_id = lid, .epoch = ep }) catch null;
    if (rep == null or !rep.?.ok) return cancel_live(job);
    job.mu.lockUncancelable(job.io);
    job.last_renew_ms = now;
    job.mu.unlock(job.io);
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
