const std = @import("std");
const builtin = @import("builtin");
const tiger = @import("../tiger.zig");
const msg = @import("../net/msg.zig");

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
    id: []const u8,
    job_dir_path: []const u8,
    log_path: []const u8,
    cwd: []const u8,
    argv: []const []const u8,
    decl_artifacts: []const []const u8,
    timeout_secs: ?u64,

    mu: std.Io.Mutex = .init,
    state: msg.JobState = .running,
    exit_code: ?i32 = null,
    err_msg: ?[]const u8 = null,
    log_bytes: u64 = 0,
    artifacts: std.ArrayList(ArtifactRec) = .empty,
    child_id: ?std.process.Child.Id = null,
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
    jobs_dir_path: []const u8,
    mu: std.Io.Mutex = .init,
    jobs: std.StringHashMapUnmanaged(*JobEntry) = .empty,

    pub fn init(gpa: std.mem.Allocator, io: std.Io, home_path: []const u8) !Runner {
        const jdir = try std.fs.path.join(gpa, &.{ home_path, "jobs" });
        errdefer gpa.free(jdir);
        try std.Io.Dir.cwd().createDirPath(io, jdir);
        return .{
            .gpa = gpa,
            .io = io,
            .jobs_dir_path = jdir,
        };
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
            .id = try self.gpa.dupe(u8, req.job_id),
            .job_dir_path = job_dir,
            .log_path = log_path,
            .cwd = cwd_str,
            .argv = argv_dup,
            .decl_artifacts = arts_dup,
            .timeout_secs = req.timeout_secs,
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
        if (je == null) return null;
        return try je.?.info_snapshot(arena);
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

        var f = std.Io.Dir.cwd().openFile(self.io, job.log_path, .{}) catch return null;
        defer f.close(self.io);
        const st = try f.stat(self.io);
        if (offset >= st.size) {
            job.mu.lockUncancelable(self.io);
            const finished = is_terminal(job.state);
            job.mu.unlock(self.io);
            return .{ .data = "", .eof = finished, .next_offset = st.size };
        }
        const avail: usize = @intCast(@min(st.size - offset, @as(u64, limit)));
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
        const avail: usize = @intCast(@min(total_size - offset, @as(u64, limit)));
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
};

fn is_terminal(st: msg.JobState) bool {
    return switch (st) {
        .succeeded, .failed, .interrupted, .cancelled, .abandoned, .lost => true,
        else => false,
    };
}

fn kill_job_proc(job: *JobEntry) void {
    job.mu.lockUncancelable(job.io);
    const cid = job.child_id;
    job.mu.unlock(job.io);
    if (cid) |id| {
        if (comptime builtin.os.tag != .windows) {
            std.posix.kill(-id, .KILL) catch {};
            std.posix.kill(id, .KILL) catch {};
        }
    }
}

fn run_job_thread(job: *JobEntry) void {
    var child = std.process.spawn(job.io, .{
        .argv = job.argv,
        .cwd = .{ .path = job.cwd },
        .stdin = .ignore,
        .stdout = .pipe,
        .stderr = .pipe,
        .pgid = if (comptime builtin.os.tag != .windows) 0 else null,
    }) catch |err| {
        job.mu.lockUncancelable(job.io);
        job.state = .failed;
        job.err_msg = job.gpa.dupe(u8, @errorName(err)) catch null;
        job.mu.unlock(job.io);
        return;
    };

    job.mu.lockUncancelable(job.io);
    job.child_id = child.id;
    job.mu.unlock(job.io);

    const log_file = std.Io.Dir.cwd().openFile(job.io, job.log_path, .{ .mode = .read_write }) catch null;
    defer if (log_file) |lf| lf.close(job.io);

    const PumpCtx = struct {
        job: *JobEntry,
        src: std.Io.File,
        log_f: ?std.Io.File,
    };

    const pump_fn = struct {
        fn run(pctx: PumpCtx) void {
            var rbuf: [4096]u8 = undefined;
            var r = pctx.src.reader(pctx.job.io, &rbuf);
            var tmp: [4096]u8 = undefined;
            while (true) {
                const n = r.interface.readSliceShort(&tmp) catch break;
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
    collect_artifacts(job);

    job.mu.lockUncancelable(job.io);
    job.child_id = null;
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
