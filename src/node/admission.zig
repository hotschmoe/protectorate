const std = @import("std");
const builtin = @import("builtin");
const tiger = @import("../tiger.zig");
const core_adm = @import("../core/admission.zig");
const msg = @import("../net/msg.zig");
const sys = @import("sys.zig");

pub const FenceFn = *const fn (
    ctx: ?*anyopaque,
    io: std.Io,
    lease_id: []const u8,
    epoch: u64,
    job_id: ?[]const u8,
) bool;

const FenceWork = struct {
    lease_id: []u8,
    epoch: u64,
    job_id: ?[]u8,
};

pub const AdmissionManager = struct {
    gpa: std.mem.Allocator,
    io: std.Io,
    home_path: []u8,
    sock_path: ?[]u8 = null,

    mu: std.Io.Mutex = .init,
    cond: std.Io.Condition = .init,
    auth: core_adm.Authority,

    fence_ctx: ?*anyopaque = null,
    fence_fn: ?FenceFn = null,

    stopping: std.atomic.Value(bool) = .init(false),
    tick_thread: ?std.Thread = null,
    sock_thread: ?std.Thread = null,
    sock_srv: if (builtin.os.tag != .windows) ?std.Io.net.Server else void = if (builtin.os.tag != .windows) null else {},

    pub fn init(
        gpa: std.mem.Allocator,
        io: std.Io,
        home_path: []const u8,
        cap_in: core_adm.CapacityConfig,
    ) !*AdmissionManager {
        var arena_st = std.heap.ArenaAllocator.init(gpa);
        defer arena_st.deinit();
        const a = arena_st.allocator();

        const node_boot_id = try sys.read_boot_id(a, io);
        var cap = cap_in;
        if (cap.total_ram_bytes == 0) {
            cap.total_ram_bytes = std.process.totalSystemMemory() catch 0;
        }

        var auth = try core_adm.Authority.init(gpa, node_boot_id, cap);
        errdefer auth.deinit();

        const hp_dup = try gpa.dupe(u8, home_path);
        errdefer gpa.free(hp_dup);

        const self = try gpa.create(AdmissionManager);
        errdefer gpa.destroy(self);

        self.* = .{
            .gpa = gpa,
            .io = io,
            .home_path = hp_dup,
            .auth = auth,
        };

        if (comptime builtin.os.tag != .windows) {
            const sp = try std.fs.path.join(gpa, &.{ home_path, "admission.sock" });
            if (sp.len <= std.Io.net.UnixAddress.max_len) {
                std.Io.Dir.cwd().deleteFile(io, sp) catch {};
                if (std.Io.net.UnixAddress.init(sp)) |ua| {
                    if (ua.listen(io, .{})) |srv| {
                        self.sock_path = sp;
                        self.sock_srv = srv;
                    } else |_| {
                        gpa.free(sp);
                    }
                } else |_| {
                    gpa.free(sp);
                }
            } else {
                gpa.free(sp);
            }
        }

        return self;
    }

    pub fn set_fence_callback(self: *AdmissionManager, ctx: ?*anyopaque, f: FenceFn) void {
        self.mu.lockUncancelable(self.io);
        defer self.mu.unlock(self.io);
        self.fence_ctx = ctx;
        self.fence_fn = f;
    }

    pub fn start(self: *AdmissionManager) !void {
        self.tick_thread = try std.Thread.spawn(.{}, tick_loop, .{self});
        if (comptime builtin.os.tag != .windows) {
            if (self.sock_srv != null) {
                self.sock_thread = std.Thread.spawn(.{}, sock_loop, .{self}) catch null;
            }
        }
    }

    /// Makes every waiting acquisition give up; deinit does the rest.
    pub fn interrupt(self: *AdmissionManager) void {
        self.stopping.store(true, .release);
        self.mu.lockUncancelable(self.io);
        self.cond.broadcast(self.io);
        self.mu.unlock(self.io);
    }

    pub fn deinit(self: *AdmissionManager) void {
        self.stopping.store(true, .release);
        {
            self.mu.lockUncancelable(self.io);
            self.cond.broadcast(self.io);
            self.mu.unlock(self.io);
        }

        if (comptime builtin.os.tag != .windows) {
            if (self.sock_path) |sp| {
                if (std.Io.net.UnixAddress.init(sp)) |ua| {
                    if (ua.connect(self.io)) |st| st.close(self.io) else |_| {}
                } else |_| {}
            }
        }
        if (self.sock_thread) |t| t.join();
        if (self.tick_thread) |t| t.join();

        if (comptime builtin.os.tag != .windows) {
            if (self.sock_srv) |*srv| {
                srv.deinit(self.io);
                self.sock_srv = null;
            }
            if (self.sock_path) |sp| {
                std.Io.Dir.cwd().deleteFile(self.io, sp) catch {};
                self.gpa.free(sp);
                self.sock_path = null;
            }
        }

        self.auth.deinit();
        self.gpa.free(self.home_path);
        self.gpa.destroy(self);
    }

    pub fn socket_path(self: *const AdmissionManager) ?[]const u8 {
        return self.sock_path;
    }

    pub fn boot_id(self: *AdmissionManager, arena: std.mem.Allocator) ![]const u8 {
        self.mu.lockUncancelable(self.io);
        defer self.mu.unlock(self.io);
        return arena.dupe(u8, self.auth.boot_id);
    }

    pub fn quarantined_copy(self: *AdmissionManager, arena: std.mem.Allocator) ?[]const u8 {
        self.mu.lockUncancelable(self.io);
        defer self.mu.unlock(self.io);
        if (self.auth.quarantined()) |q| {
            return arena.dupe(u8, q) catch null;
        }
        return null;
    }

    pub fn acquire_blocking(
        self: *AdmissionManager,
        arena: std.mem.Allocator,
        req: msg.AcquireLease,
        job_id: ?[]const u8,
        abandon: ?*const std.atomic.Value(bool),
    ) !msg.LeaseReply {
        const start_ms = std.Io.Clock.awake.now(self.io).toMilliseconds();
        const mem_avail = sys.read_mem_avail_bytes(self.io);
        const mode: core_adm.HoldMode = if (req.exclusive) .exclusive else .shared;
        const res: core_adm.Resources = .{
            .slots = req.slots,
            .long_slots = req.long_slots,
            .mem_bytes = req.mem_bytes,
        };
        const wait_ms = @max(@as(i64, 0), req.wait_ms orelse self.auth.cfg.default_ttl_ms);
        const ttl_ms = @max(core_adm.min_ttl_ms, req.ttl_ms orelse self.auth.cfg.default_ttl_ms);
        const wait_deadline = start_ms +| wait_ms;

        var ticket_id_own: ?[]u8 = null;
        defer if (ticket_id_own) |t| self.gpa.free(t);

        var fences_buf: []FenceWork = &.{};
        {
            self.mu.lockUncancelable(self.io);
            defer self.mu.unlock(self.io);

            if (req.parent_epoch) |pe| {
                const r = try self.auth.acquire_child(
                    pe,
                    req.holder_id,
                    job_id,
                    res,
                    req.ttl_ms,
                    start_ms,
                    mem_avail,
                );
                return switch (r) {
                    .granted => |g| .{
                        .ok = true,
                        .boot_id = try arena.dupe(u8, self.auth.boot_id),
                        .lease_id = try arena.dupe(u8, g.lease_id),
                        .epoch = g.epoch,
                        .parent_epoch = pe,
                        .deadline_ms = g.deadline_ms,
                    },
                    .refused => |reason| .{
                        .ok = false,
                        .boot_id = try arena.dupe(u8, self.auth.boot_id),
                        .reason = try arena.dupe(u8, reason),
                    },
                    .queued => unreachable,
                };
            }

            var actions: std.ArrayList(core_adm.Action) = .empty;
            defer core_adm.deinit_actions(self.gpa, &actions);

            const r = try self.auth.enqueue_or_grant(
                req.holder_id,
                job_id,
                mode,
                res,
                req.ttl_ms,
                wait_ms,
                start_ms,
                mem_avail,
                &actions,
            );

            var woke = false;
            fences_buf = try self.extract_fences(&actions, &woke);
            if (woke) self.cond.broadcast(self.io);

            switch (r) {
                .granted => |g| {
                    const out = msg.LeaseReply{
                        .ok = true,
                        .boot_id = try arena.dupe(u8, self.auth.boot_id),
                        .lease_id = try arena.dupe(u8, g.lease_id),
                        .epoch = g.epoch,
                        .deadline_ms = g.deadline_ms,
                    };
                    if (fences_buf.len == 0) return out;
                    self.mu.unlock(self.io);
                    _ = self.execute_fences(fences_buf);
                    self.mu.lockUncancelable(self.io);
                    return out;
                },
                .refused => |reason| {
                    const out = msg.LeaseReply{
                        .ok = false,
                        .boot_id = try arena.dupe(u8, self.auth.boot_id),
                        .reason = try arena.dupe(u8, reason),
                    };
                    if (fences_buf.len == 0) return out;
                    self.mu.unlock(self.io);
                    _ = self.execute_fences(fences_buf);
                    self.mu.lockUncancelable(self.io);
                    return out;
                },
                .queued => |q| {
                    ticket_id_own = try self.gpa.dupe(u8, q.ticket_id);
                },
            }
        }

        if (fences_buf.len > 0) {
            _ = self.execute_fences(fences_buf);
        }

        const tid = ticket_id_own orelse return .{ .ok = false, .reason = "missing ticket" };
        var last_renew_ms = start_ms;

        self.mu.lockUncancelable(self.io);
        defer self.mu.unlock(self.io);

        while (true) {
            if (self.auth.find_grant_by_ticket(tid)) |g| {
                return .{
                    .ok = true,
                    .boot_id = try arena.dupe(u8, self.auth.boot_id),
                    .lease_id = try arena.dupe(u8, g.lease_id),
                    .epoch = g.epoch,
                    .parent_epoch = g.parent_epoch,
                    .deadline_ms = g.deadline_ms,
                };
            }
            if (self.auth.find_ticket_by_id(tid) == null) {
                return .{
                    .ok = false,
                    .boot_id = try arena.dupe(u8, self.auth.boot_id),
                    .reason = try arena.dupe(u8, self.auth.quarantined() orelse "refused_or_expired"),
                };
            }
            if (self.stopping.load(.acquire)) {
                _ = self.auth.cancel_ticket(tid);
                return .{
                    .ok = false,
                    .boot_id = try arena.dupe(u8, self.auth.boot_id),
                    .reason = "node stopping",
                };
            }
            if (abandon) |flag| if (flag.load(.acquire)) {
                _ = self.auth.cancel_ticket(tid);
                return .{
                    .ok = false,
                    .boot_id = try arena.dupe(u8, self.auth.boot_id),
                    .reason = "hub disconnected",
                };
            };

            const now_ms = std.Io.Clock.awake.now(self.io).toMilliseconds();
            if (now_ms >= wait_deadline) {
                _ = self.auth.cancel_ticket(tid);
                return .{
                    .ok = false,
                    .boot_id = try arena.dupe(u8, self.auth.boot_id),
                    .reason = "wait_timeout",
                };
            }
            if (now_ms - last_renew_ms >= @divFloor(ttl_ms, 3)) {
                _ = self.auth.renew_ticket(tid, now_ms);
                last_renew_ms = now_ms;
            }

            const renew_iv = @max(@as(i64, 50), @divFloor(ttl_ms, 4));
            const step_ms = @min(@as(i64, 500), @min(renew_iv, @max(@as(i64, 50), wait_deadline - now_ms)));
            self.cond.waitTimeout(self.io, &self.mu, .{
                .duration = .{ .raw = .fromMilliseconds(step_ms), .clock = .awake },
            }) catch {};
        }
    }

    pub fn renew(self: *AdmissionManager, arena: std.mem.Allocator, req: msg.RenewLease) !msg.LeaseReply {
        const now_ms = std.Io.Clock.awake.now(self.io).toMilliseconds();
        self.mu.lockUncancelable(self.io);
        defer self.mu.unlock(self.io);

        if (!self.auth.renew_lease(req.lease_id, req.epoch, now_ms)) {
            return .{
                .ok = false,
                .boot_id = try arena.dupe(u8, self.auth.boot_id),
                .lease_id = try arena.dupe(u8, req.lease_id),
                .epoch = req.epoch,
                .reason = try arena.dupe(u8, self.auth.quarantined() orelse "expired_or_unknown"),
            };
        }
        const g = self.auth.find_grant_by_epoch(req.epoch).?;
        return .{
            .ok = true,
            .boot_id = try arena.dupe(u8, self.auth.boot_id),
            .lease_id = try arena.dupe(u8, g.lease_id),
            .epoch = g.epoch,
            .parent_epoch = g.parent_epoch,
            .deadline_ms = g.deadline_ms,
        };
    }

    pub fn begin_collecting(self: *AdmissionManager, lease_id: []const u8, epoch: u64) void {
        const now_ms = std.Io.Clock.awake.now(self.io).toMilliseconds();
        var fences: []FenceWork = &.{};
        {
            self.mu.lockUncancelable(self.io);
            defer self.mu.unlock(self.io);
            var actions: std.ArrayList(core_adm.Action) = .empty;
            defer core_adm.deinit_actions(self.gpa, &actions);
            _ = self.auth.begin_collecting(lease_id, epoch, now_ms, &actions) catch false;
            var woke = false;
            fences = self.extract_fences(&actions, &woke) catch &.{};
            self.cond.broadcast(self.io);
        }
        if (fences.len > 0) {
            _ = self.execute_fences(fences);
        }
    }

    pub fn release(
        self: *AdmissionManager,
        arena: std.mem.Allocator,
        lease_id: []const u8,
        epoch: u64,
    ) !msg.ReleaseLeaseReply {
        const now_ms = std.Io.Clock.awake.now(self.io).toMilliseconds();
        var fences: []FenceWork = &.{};
        var existed = false;
        {
            self.mu.lockUncancelable(self.io);
            defer self.mu.unlock(self.io);
            var actions: std.ArrayList(core_adm.Action) = .empty;
            defer core_adm.deinit_actions(self.gpa, &actions);
            existed = try self.auth.release_lease(lease_id, epoch, now_ms, &actions);
            var woke = false;
            fences = try self.extract_fences(&actions, &woke);
            self.cond.broadcast(self.io);
        }
        if (!existed) {
            return .{
                .ok = false,
                .fenced = false,
                .quarantined = self.quarantined_copy(arena),
            };
        }
        const fenced = if (fences.len > 0) self.execute_fences(fences) else true;
        return .{
            .ok = true,
            .fenced = fenced,
            .quarantined = self.quarantined_copy(arena),
        };
    }

    pub fn fail_fence_now(self: *AdmissionManager, lease_id: []const u8, epoch: u64, reason: []const u8) void {
        self.mu.lockUncancelable(self.io);
        self.auth.fail_fence(lease_id, epoch, reason) catch {};
        self.cond.broadcast(self.io);
        self.mu.unlock(self.io);
    }

    pub fn verify_epoch(self: *AdmissionManager, arena: std.mem.Allocator, req: msg.VerifyEpoch) !msg.VerifyEpochReply {
        const now_ms = std.Io.Clock.awake.now(self.io).toMilliseconds();
        self.mu.lockUncancelable(self.io);
        defer self.mu.unlock(self.io);

        const valid = self.auth.verify_epoch(req.boot_id, req.epoch, now_ms);
        const qr = if (self.auth.quarantined()) |q| try arena.dupe(u8, q) else null;
        if (valid) {
            const g = self.auth.find_grant_by_epoch(req.epoch).?;
            return .{
                .valid = true,
                .boot_id = try arena.dupe(u8, self.auth.boot_id),
                .state = switch (g.state) {
                    .active => .active,
                    .collecting => .collecting,
                    .fencing => .fencing,
                },
                .expires_in_ms = g.deadline_ms - now_ms,
                .quarantined = qr,
            };
        }
        const reason: []const u8 = if (qr != null)
            "quarantined"
        else if (!std.mem.eql(u8, req.boot_id, self.auth.boot_id))
            "boot_id_mismatch"
        else if (self.auth.find_grant_by_epoch(req.epoch)) |g|
            (if (g.state == .fencing) "fencing" else "expired_or_parent_inactive")
        else
            "unknown_epoch";

        return .{
            .valid = false,
            .boot_id = try arena.dupe(u8, self.auth.boot_id),
            .quarantined = qr,
            .reason = reason,
        };
    }

    pub fn status_snapshot(self: *AdmissionManager, arena: std.mem.Allocator) !msg.AdmissionStatus {
        const mem_avail = sys.read_mem_avail_bytes(self.io);
        self.mu.lockUncancelable(self.io);
        defer self.mu.unlock(self.io);

        var grants = try arena.alloc(msg.LeaseInfo, self.auth.grants.items.len);
        for (self.auth.grants.items, 0..) |g, i| {
            grants[i] = .{
                .lease_id = try arena.dupe(u8, g.lease_id),
                .epoch = g.epoch,
                .parent_epoch = g.parent_epoch,
                .holder_id = try arena.dupe(u8, g.holder_id),
                .job_id = if (g.job_id) |j| try arena.dupe(u8, j) else null,
                .mode = if (g.mode == .exclusive) .exclusive else .shared,
                .state = switch (g.state) {
                    .active => .active,
                    .collecting => .collecting,
                    .fencing => .fencing,
                },
                .slots = g.res.slots,
                .long_slots = g.res.long_slots,
                .mem_bytes = g.res.mem_bytes,
                .deadline_ms = g.deadline_ms,
            };
        }

        var queue = try arena.alloc(msg.TicketInfo, self.auth.queue.items.len);
        for (self.auth.queue.items, 0..) |t, i| {
            queue[i] = .{
                .ticket_id = try arena.dupe(u8, t.ticket_id),
                .seq = t.seq,
                .holder_id = try arena.dupe(u8, t.holder_id),
                .mode = if (t.mode == .exclusive) .exclusive else .shared,
                .slots = t.res.slots,
                .long_slots = t.res.long_slots,
                .mem_bytes = t.res.mem_bytes,
                .wait_deadline_ms = t.wait_deadline_ms,
                .lease_deadline_ms = t.lease_deadline_ms,
            };
        }

        return .{
            .boot_id = try arena.dupe(u8, self.auth.boot_id),
            .next_epoch = self.auth.next_epoch,
            .slots_total = self.auth.cfg.slots,
            .slots_used = self.auth.used_slots(),
            .long_slots_total = self.auth.cfg.long_slots,
            .long_slots_used = self.auth.used_long_slots(),
            .mem_margin_bytes = self.auth.cfg.mem_margin_bytes,
            .mem_reserved_bytes = self.auth.reserved_mem_bytes(),
            .mem_avail_bytes = mem_avail,
            .quarantined = if (self.auth.quarantined()) |q| try arena.dupe(u8, q) else null,
            .grants = grants,
            .queue = queue,
        };
    }

    pub fn clear_quarantine(self: *AdmissionManager, arena: std.mem.Allocator) !msg.ClearQuarantineReply {
        var stuck: std.ArrayList(FenceWork) = .empty;
        {
            self.mu.lockUncancelable(self.io);
            for (self.auth.grants.items) |g| {
                if (g.state == .fencing) {
                    stuck.append(self.gpa, .{
                        .lease_id = self.gpa.dupe(u8, g.lease_id) catch continue,
                        .epoch = g.epoch,
                        .job_id = if (g.job_id) |j| (self.gpa.dupe(u8, j) catch null) else null,
                    }) catch {};
                }
            }
            self.mu.unlock(self.io);
        }
        const fences = try stuck.toOwnedSlice(self.gpa);
        if (fences.len > 0) {
            if (!self.execute_fences(fences)) {
                return .{
                    .ok = false,
                    .cleared = false,
                    .quarantined = self.quarantined_copy(arena),
                    .detail = "fencing still incomplete; processes still live",
                };
            }
        }

        self.mu.lockUncancelable(self.io);
        const cleared = self.auth.clear_quarantine();
        self.cond.broadcast(self.io);
        self.mu.unlock(self.io);
        return .{
            .ok = true,
            .cleared = cleared,
            .quarantined = self.quarantined_copy(arena),
            .detail = if (cleared) "quarantine cleared" else "fencing grants remain",
        };
    }

    fn extract_fences(
        self: *AdmissionManager,
        actions: *std.ArrayList(core_adm.Action),
        woke: *bool,
    ) ![]FenceWork {
        var list: std.ArrayList(FenceWork) = .empty;
        errdefer {
            for (list.items) |it| {
                self.gpa.free(it.lease_id);
                if (it.job_id) |j| self.gpa.free(j);
            }
            list.deinit(self.gpa);
        }
        for (actions.items) |act| {
            switch (act) {
                .fence_grant => |fg| {
                    try list.append(self.gpa, .{
                        .lease_id = try self.gpa.dupe(u8, fg.lease_id),
                        .epoch = fg.epoch,
                        .job_id = if (fg.job_id) |j| try self.gpa.dupe(u8, j) else null,
                    });
                },
                .grant_admitted, .ticket_refused => {
                    woke.* = true;
                },
            }
        }
        return try list.toOwnedSlice(self.gpa);
    }

    fn execute_fences(self: *AdmissionManager, fences: []FenceWork) bool {
        defer {
            for (fences) |fi| {
                self.gpa.free(fi.lease_id);
                if (fi.job_id) |j| self.gpa.free(j);
            }
            self.gpa.free(fences);
        }

        var all_ok = true;
        for (fences) |fi| {
            const ok = if (self.fence_fn) |ff|
                ff(self.fence_ctx, self.io, fi.lease_id, fi.epoch, fi.job_id)
            else
                true;
            if (!ok) all_ok = false;

            const now2 = std.Io.Clock.awake.now(self.io).toMilliseconds();
            const mem2 = sys.read_mem_avail_bytes(self.io);
            self.mu.lockUncancelable(self.io);
            var post: std.ArrayList(core_adm.Action) = .empty;
            defer core_adm.deinit_actions(self.gpa, &post);
            if (ok) {
                _ = self.auth.confirm_fence(fi.lease_id, fi.epoch, now2, mem2, &post) catch {};
            } else {
                self.auth.fail_fence(fi.lease_id, fi.epoch, "process tree failed to fence") catch {};
            }
            self.cond.broadcast(self.io);
            self.mu.unlock(self.io);
        }
        return all_ok;
    }

    fn tick_loop(self: *AdmissionManager) void {
        while (!self.stopping.load(.acquire)) {
            self.io.sleep(.fromMilliseconds(500), .awake) catch break;
            if (self.stopping.load(.acquire)) break;

            const now_ms = std.Io.Clock.awake.now(self.io).toMilliseconds();
            const mem_avail = sys.read_mem_avail_bytes(self.io);

            var fences: []FenceWork = &.{};
            {
                self.mu.lockUncancelable(self.io);
                defer self.mu.unlock(self.io);
                var actions: std.ArrayList(core_adm.Action) = .empty;
                defer core_adm.deinit_actions(self.gpa, &actions);
                self.auth.tick(now_ms, mem_avail, &actions) catch {};
                var woke = false;
                fences = self.extract_fences(&actions, &woke) catch &.{};
                if (woke) self.cond.broadcast(self.io);
            }
            if (fences.len > 0) {
                _ = self.execute_fences(fences);
            }
        }
    }

    fn sock_loop(self: *AdmissionManager) void {
        if (comptime builtin.os.tag == .windows) return;
        const srv = &(self.sock_srv orelse return);
        while (!self.stopping.load(.acquire)) {
            var stream = srv.accept(self.io) catch {
                if (self.stopping.load(.acquire)) break;
                continue;
            };
            if (self.stopping.load(.acquire)) {
                stream.close(self.io);
                break;
            }
            self.handle_sock_conn(&stream);
            stream.close(self.io);
        }
    }

    fn handle_sock_conn(self: *AdmissionManager, stream: *std.Io.net.Stream) void {
        var arena_st = std.heap.ArenaAllocator.init(self.gpa);
        defer arena_st.deinit();
        const arena = arena_st.allocator();

        var rbuf: [8192]u8 = undefined;
        var wbuf: [8192]u8 = undefined;
        var r = stream.reader(self.io, &rbuf);
        var w = stream.writer(self.io, &wbuf);

        var line_aw: std.Io.Writer.Allocating = .init(arena);
        _ = r.interface.streamDelimiterLimit(&line_aw.writer, '\n', .limited(1 << 20)) catch return;
        const line = std.mem.trim(u8, line_aw.written(), " \t\r\n");
        if (line.len == 0) return;

        const req = msg.decode_arena(msg.Request, arena, line) catch {
            const err_j = msg.encode_alloc(arena, msg.ErrorReply{ .code = .invalid, .message = "bad request" }) catch return;
            w.interface.writeAll(err_j) catch return;
            w.interface.writeByte('\n') catch return;
            w.interface.flush() catch {};
            return;
        };

        const out_json: []u8 = switch (req) {
            .acquire_lease => |al| blk: {
                const rep = self.acquire_blocking(arena, al, al.job_id, null) catch |e|
                    break :blk msg.encode_alloc(arena, msg.ErrorReply{ .code = .internal, .message = @errorName(e) }) catch return;
                break :blk msg.encode_alloc(arena, rep) catch return;
            },
            .renew_lease => |rl| blk: {
                const rep = self.renew(arena, rl) catch |e|
                    break :blk msg.encode_alloc(arena, msg.ErrorReply{ .code = .internal, .message = @errorName(e) }) catch return;
                break :blk msg.encode_alloc(arena, rep) catch return;
            },
            .release_lease => |rl| blk: {
                const rep = self.release(arena, rl.lease_id, rl.epoch) catch |e|
                    break :blk msg.encode_alloc(arena, msg.ErrorReply{ .code = .internal, .message = @errorName(e) }) catch return;
                break :blk msg.encode_alloc(arena, rep) catch return;
            },
            .verify_epoch => |ve| blk: {
                const rep = self.verify_epoch(arena, ve) catch |e|
                    break :blk msg.encode_alloc(arena, msg.ErrorReply{ .code = .internal, .message = @errorName(e) }) catch return;
                break :blk msg.encode_alloc(arena, rep) catch return;
            },
            .list_leases => blk: {
                const st = self.status_snapshot(arena) catch |e|
                    break :blk msg.encode_alloc(arena, msg.ErrorReply{ .code = .internal, .message = @errorName(e) }) catch return;
                break :blk msg.encode_alloc(arena, st) catch return;
            },
            .clear_quarantine => blk: {
                const rep = self.clear_quarantine(arena) catch |e|
                    break :blk msg.encode_alloc(arena, msg.ErrorReply{ .code = .internal, .message = @errorName(e) }) catch return;
                break :blk msg.encode_alloc(arena, rep) catch return;
            },
            else => msg.encode_alloc(arena, msg.ErrorReply{ .code = .unknown_request, .message = "unsupported local op" }) catch return,
        };

        w.interface.writeAll(out_json) catch return;
        w.interface.writeByte('\n') catch return;
        w.interface.flush() catch {};
    }
};
