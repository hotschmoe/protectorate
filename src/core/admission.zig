const std = @import("std");
const tiger = @import("../tiger.zig");
const model = @import("model.zig");

pub const max_queue: usize = 256;
pub const max_grants: usize = 256;
pub const mem_round_bytes: u64 = 256 * 1024 * 1024;
pub const default_ttl_ms: i64 = 30_000;
pub const min_ttl_ms: i64 = 1_000;
pub const max_ttl_ms: i64 = 3_600_000;

pub const HoldMode = enum {
    shared,
    exclusive,
};

pub const GrantState = enum {
    active,
    collecting,
    fencing,
};

pub const FenceReason = enum {
    expired,
    released,
    parent_revoked,
    cancelled,
    job_done,
};

pub const CapacityConfig = struct {
    slots: u32 = 8,
    long_slots: u32 = 2,
    mem_margin_bytes: u64 = 512 * 1024 * 1024,
    total_ram_bytes: u64 = 0,
    max_queue: u32 = max_queue,
    default_ttl_ms: i64 = default_ttl_ms,
};

pub const Resources = struct {
    slots: u32 = 0,
    long_slots: u32 = 0,
    mem_bytes: u64 = 0,

    pub fn normalized(self: Resources) Resources {
        if (self.mem_bytes == 0) return self;
        const unit = mem_round_bytes;
        const rem = self.mem_bytes % unit;
        const rounded = if (rem == 0) self.mem_bytes else self.mem_bytes +| (unit - rem);
        return .{
            .slots = self.slots,
            .long_slots = self.long_slots,
            .mem_bytes = rounded,
        };
    }
};

pub const Ticket = struct {
    seq: u64,
    ticket_id: []u8,
    holder_id: []u8,
    job_id: ?[]u8 = null,
    mode: HoldMode,
    res: Resources,
    ttl_ms: i64,
    enqueued_ms: i64,
    wait_deadline_ms: i64,
    lease_deadline_ms: i64,

    fn deinit(self: *Ticket, gpa: std.mem.Allocator) void {
        gpa.free(self.ticket_id);
        gpa.free(self.holder_id);
        if (self.job_id) |j| gpa.free(j);
    }
};

pub const Grant = struct {
    lease_id: []u8,
    ticket_id: ?[]u8 = null,
    epoch: u64,
    parent_epoch: ?u64 = null,
    holder_id: []u8,
    job_id: ?[]u8 = null,
    mode: HoldMode,
    state: GrantState,
    res: Resources,
    ttl_ms: i64,
    granted_ms: i64,
    deadline_ms: i64,
    fence_started_ms: ?i64 = null,

    fn deinit(self: *Grant, gpa: std.mem.Allocator) void {
        gpa.free(self.lease_id);
        if (self.ticket_id) |tid| gpa.free(tid);
        gpa.free(self.holder_id);
        if (self.job_id) |j| gpa.free(j);
    }
};

pub const Action = union(enum) {
    fence_grant: struct {
        lease_id: []u8,
        epoch: u64,
        job_id: ?[]u8,
        reason: FenceReason,
    },
    grant_admitted: struct {
        ticket_id: []u8,
        lease_id: []u8,
        epoch: u64,
        holder_id: []u8,
    },
    ticket_refused: struct {
        ticket_id: []u8,
        reason: []u8,
    },

    pub fn deinit(self: *Action, gpa: std.mem.Allocator) void {
        switch (self.*) {
            .fence_grant => |fg| {
                gpa.free(fg.lease_id);
                if (fg.job_id) |j| gpa.free(j);
            },
            .grant_admitted => |ga| {
                gpa.free(ga.ticket_id);
                gpa.free(ga.lease_id);
                gpa.free(ga.holder_id);
            },
            .ticket_refused => |tr| {
                gpa.free(tr.ticket_id);
                gpa.free(tr.reason);
            },
        }
    }
};

pub fn deinit_actions(gpa: std.mem.Allocator, list: *std.ArrayList(Action)) void {
    for (list.items) |*a| a.deinit(gpa);
    list.deinit(gpa);
}

pub const AcquireResult = union(enum) {
    granted: struct {
        lease_id: []const u8,
        epoch: u64,
        deadline_ms: i64,
    },
    queued: struct {
        ticket_id: []const u8,
        seq: u64,
        position: usize,
    },
    refused: []const u8,
};

pub const Authority = struct {
    gpa: std.mem.Allocator,
    boot_id: []u8,
    cfg: CapacityConfig,
    next_epoch: u64 = 1,
    next_seq: u64 = 1,
    next_id: u64 = 1,
    quarantined_reason: ?[]u8 = null,
    grants: std.ArrayList(Grant) = .empty,
    queue: std.ArrayList(Ticket) = .empty,

    pub fn init(gpa: std.mem.Allocator, boot_id: []const u8, cfg: CapacityConfig) !Authority {
        tiger.assert(boot_id.len > 0);
        return .{
            .gpa = gpa,
            .boot_id = try gpa.dupe(u8, boot_id),
            .cfg = cfg,
        };
    }

    pub fn deinit(self: *Authority) void {
        for (self.grants.items) |*g| g.deinit(self.gpa);
        self.grants.deinit(self.gpa);
        for (self.queue.items) |*t| t.deinit(self.gpa);
        self.queue.deinit(self.gpa);
        if (self.quarantined_reason) |r| self.gpa.free(r);
        self.gpa.free(self.boot_id);
    }

    pub fn quarantined(self: *const Authority) ?[]const u8 {
        return self.quarantined_reason;
    }

    pub fn clear_quarantine(self: *Authority) bool {
        for (self.grants.items) |g| {
            if (g.state == .fencing) return false;
        }
        if (self.quarantined_reason) |r| {
            self.gpa.free(r);
            self.quarantined_reason = null;
        }
        return true;
    }

    pub fn used_slots(self: *const Authority) u32 {
        var total: u32 = 0;
        for (self.grants.items) |g| total +|= g.res.slots;
        return total;
    }

    pub fn used_long_slots(self: *const Authority) u32 {
        var total: u32 = 0;
        for (self.grants.items) |g| total +|= g.res.long_slots;
        return total;
    }

    pub fn reserved_mem_bytes(self: *const Authority) u64 {
        var total: u64 = 0;
        for (self.grants.items) |g| total +|= g.res.mem_bytes;
        return total;
    }

    pub fn is_statically_satisfiable(self: *const Authority, raw_res: Resources) bool {
        const res = raw_res.normalized();
        if (res.slots > self.cfg.slots) return false;
        if (res.long_slots > self.cfg.long_slots) return false;
        if (self.cfg.total_ram_bytes > 0 and res.mem_bytes > 0) {
            if (res.mem_bytes +| self.cfg.mem_margin_bytes > self.cfg.total_ram_bytes) return false;
        }
        return true;
    }

    pub fn can_fit(self: *const Authority, raw_res: Resources, mem_avail_bytes: u64) bool {
        const res = raw_res.normalized();
        if (self.grants.items.len >= max_grants) return false;
        if (self.used_slots() +| res.slots > self.cfg.slots) return false;
        if (self.used_long_slots() +| res.long_slots > self.cfg.long_slots) return false;
        if (res.mem_bytes > 0) {
            const need = self.cfg.mem_margin_bytes +| self.reserved_mem_bytes() +| res.mem_bytes;
            if (mem_avail_bytes < need) return false;
        }
        return true;
    }

    fn mode_compatible(self: *const Authority, mode: HoldMode) bool {
        return switch (mode) {
            .exclusive => self.grants.items.len == 0,
            .shared => blk: {
                for (self.grants.items) |g| {
                    if (g.mode == .exclusive) break :blk false;
                }
                break :blk true;
            },
        };
    }

    fn clamp_ttl(self: *const Authority, ttl_opt: ?i64) i64 {
        const raw = ttl_opt orelse self.cfg.default_ttl_ms;
        return @min(max_ttl_ms, @max(min_ttl_ms, raw));
    }

    fn next_lease_id(self: *Authority) ![]u8 {
        const n = self.next_id;
        self.next_id += 1;
        return std.fmt.allocPrint(self.gpa, "lease-{d}", .{n});
    }

    fn next_ticket_id(self: *Authority) ![]u8 {
        const n = self.next_id;
        self.next_id += 1;
        return std.fmt.allocPrint(self.gpa, "tkt-{d}", .{n});
    }

    pub fn enqueue_or_grant(
        self: *Authority,
        holder_id: []const u8,
        job_id: ?[]const u8,
        mode: HoldMode,
        raw_res: Resources,
        ttl_ms_opt: ?i64,
        wait_ms_opt: ?i64,
        now_ms: i64,
        mem_avail_bytes: u64,
        actions: *std.ArrayList(Action),
    ) !AcquireResult {
        if (holder_id.len == 0) return .{ .refused = "empty holder_id" };
        if (self.quarantined_reason != null) return .{ .refused = "quarantined" };

        const res = raw_res.normalized();
        if (!self.is_statically_satisfiable(res)) {
            return .{ .refused = "exceeds node capacity" };
        }

        const ttl_ms = self.clamp_ttl(ttl_ms_opt);

        try self.admit_from_queue(now_ms, mem_avail_bytes, actions);

        if (self.queue.items.len == 0 and self.mode_compatible(mode) and self.can_fit(res, mem_avail_bytes)) {
            const g = try self.insert_grant(null, null, holder_id, job_id, mode, res, ttl_ms, now_ms + ttl_ms, now_ms);
            return .{ .granted = .{
                .lease_id = g.lease_id,
                .epoch = g.epoch,
                .deadline_ms = g.deadline_ms,
            } };
        }

        const q_cap = @min(max_queue, self.cfg.max_queue);
        if (self.queue.items.len >= q_cap) return .{ .refused = "queue_full" };

        const wait_ms = @max(@as(i64, 0), wait_ms_opt orelse ttl_ms);
        if (wait_ms == 0) return .{ .refused = "busy" };

        const tid = try self.next_ticket_id();
        errdefer self.gpa.free(tid);
        const hdup = try self.gpa.dupe(u8, holder_id);
        errdefer self.gpa.free(hdup);
        const jdup = if (job_id) |j| try self.gpa.dupe(u8, j) else null;
        errdefer if (jdup) |j| self.gpa.free(j);

        const seq = self.next_seq;
        self.next_seq += 1;
        try self.queue.append(self.gpa, .{
            .seq = seq,
            .ticket_id = tid,
            .holder_id = hdup,
            .job_id = jdup,
            .mode = mode,
            .res = res,
            .ttl_ms = ttl_ms,
            .enqueued_ms = now_ms,
            .wait_deadline_ms = now_ms +| wait_ms,
            .lease_deadline_ms = now_ms +| ttl_ms,
        });
        const pos = self.queue.items.len - 1;
        return .{ .queued = .{
            .ticket_id = self.queue.items[pos].ticket_id,
            .seq = seq,
            .position = pos,
        } };
    }

    pub fn acquire_child(
        self: *Authority,
        parent_epoch: u64,
        holder_id: []const u8,
        job_id: ?[]const u8,
        raw_res: Resources,
        ttl_ms_opt: ?i64,
        now_ms: i64,
        mem_avail_bytes: u64,
    ) !AcquireResult {
        if (holder_id.len == 0) return .{ .refused = "empty holder_id" };
        if (self.quarantined_reason != null) return .{ .refused = "quarantined" };

        const res = raw_res.normalized();
        if (!self.is_statically_satisfiable(res)) {
            return .{ .refused = "exceeds node capacity" };
        }

        const p = self.find_grant_by_epoch(parent_epoch) orelse return .{ .refused = "parent not found" };
        if (p.parent_epoch != null) return .{ .refused = "parent is itself a child" };
        if (p.state != .active or now_ms >= p.deadline_ms) return .{ .refused = "parent inactive or expired" };

        if (!self.can_fit(res, mem_avail_bytes)) return .{ .refused = "insufficient capacity" };

        const ttl_ms = self.clamp_ttl(ttl_ms_opt);
        const capped_deadline = @min(now_ms +| ttl_ms, p.deadline_ms);
        const g = try self.insert_grant(parent_epoch, null, holder_id, job_id, .shared, res, ttl_ms, capped_deadline, now_ms);
        return .{ .granted = .{
            .lease_id = g.lease_id,
            .epoch = g.epoch,
            .deadline_ms = g.deadline_ms,
        } };
    }

    pub fn renew_lease(self: *Authority, lease_id: []const u8, epoch: u64, now_ms: i64) bool {
        if (self.quarantined_reason != null) return false;
        const g = self.find_grant_mut(lease_id, epoch) orelse return false;
        if (g.state == .fencing) return false;
        if (now_ms >= g.deadline_ms) return false;

        if (g.parent_epoch) |pe| {
            const p = self.find_grant_by_epoch(pe) orelse return false;
            if (p.state != .active or now_ms >= p.deadline_ms) return false;
            g.deadline_ms = @min(now_ms +| g.ttl_ms, p.deadline_ms);
        } else {
            g.deadline_ms = now_ms +| g.ttl_ms;
        }
        return true;
    }

    pub fn renew_ticket(self: *Authority, ticket_id: []const u8, now_ms: i64) bool {
        for (self.queue.items) |*t| {
            if (std.mem.eql(u8, t.ticket_id, ticket_id)) {
                if (now_ms >= t.wait_deadline_ms or now_ms >= t.lease_deadline_ms) return false;
                t.lease_deadline_ms = now_ms +| t.ttl_ms;
                return true;
            }
        }
        return false;
    }

    pub fn cancel_ticket(self: *Authority, ticket_id: []const u8) bool {
        for (self.queue.items, 0..) |*t, idx| {
            if (std.mem.eql(u8, t.ticket_id, ticket_id)) {
                var removed = self.queue.orderedRemove(idx);
                removed.deinit(self.gpa);
                return true;
            }
        }
        return false;
    }

    pub fn begin_collecting(
        self: *Authority,
        lease_id: []const u8,
        epoch: u64,
        now_ms: i64,
        actions: *std.ArrayList(Action),
    ) !bool {
        var new_deadline: i64 = undefined;
        {
            const g_ro = self.find_grant_mut(lease_id, epoch) orelse return false;
            if (g_ro.state == .fencing) return false;
            if (now_ms >= g_ro.deadline_ms) return false;
            if (g_ro.parent_epoch) |pe| {
                const p = self.find_grant_by_epoch(pe) orelse return false;
                if (p.state != .active or now_ms >= p.deadline_ms) return false;
                new_deadline = @min(now_ms +| g_ro.ttl_ms, p.deadline_ms);
            } else {
                new_deadline = now_ms +| g_ro.ttl_ms;
            }
        }
        const g = self.find_grant_mut(lease_id, epoch) orelse return false;
        g.state = .collecting;
        g.deadline_ms = new_deadline;
        if (g.parent_epoch == null) {
            try self.fence_children_locked(epoch, now_ms, actions);
        }
        return true;
    }

    pub fn release_lease(
        self: *Authority,
        lease_id: []const u8,
        epoch: u64,
        now_ms: i64,
        actions: *std.ArrayList(Action),
    ) !bool {
        const g = self.find_grant_mut(lease_id, epoch) orelse return false;
        if (g.state == .fencing) return true;
        g.state = .fencing;
        g.fence_started_ms = now_ms;
        try self.emit_fence_action(g.*, .released, actions);
        if (g.parent_epoch == null) {
            try self.fence_children_locked(epoch, now_ms, actions);
        }
        return true;
    }

    pub fn confirm_fence(
        self: *Authority,
        lease_id: []const u8,
        epoch: u64,
        now_ms: i64,
        mem_avail_bytes: u64,
        actions: *std.ArrayList(Action),
    ) !bool {
        var found_idx: ?usize = null;
        for (self.grants.items, 0..) |g, idx| {
            if (std.mem.eql(u8, g.lease_id, lease_id) and g.epoch == epoch and g.state == .fencing) {
                found_idx = idx;
                break;
            }
        }
        const idx = found_idx orelse return false;
        var removed = self.grants.orderedRemove(idx);
        removed.deinit(self.gpa);
        try self.admit_from_queue(now_ms, mem_avail_bytes, actions);
        return true;
    }

    pub fn fail_fence(self: *Authority, lease_id: []const u8, epoch: u64, reason: []const u8) !void {
        var parent_ep: ?u64 = null;
        if (self.find_grant_mut(lease_id, epoch)) |g| {
            g.state = .fencing;
            if (g.parent_epoch == null) parent_ep = g.epoch;
        }
        if (parent_ep) |pe| {
            for (self.grants.items) |*cg| {
                if (cg.parent_epoch != null and cg.parent_epoch.? == pe and cg.state != .fencing) {
                    cg.state = .fencing;
                }
            }
        }
        if (self.quarantined_reason) |old| self.gpa.free(old);
        self.quarantined_reason = try self.gpa.dupe(u8, reason);
    }

    pub fn tick(
        self: *Authority,
        now_ms: i64,
        mem_avail_bytes: u64,
        actions: *std.ArrayList(Action),
    ) !void {
        var expired_parents: std.ArrayList(u64) = .empty;
        defer expired_parents.deinit(self.gpa);

        for (self.grants.items) |*g| {
            if (g.state != .fencing and now_ms >= g.deadline_ms) {
                g.state = .fencing;
                g.fence_started_ms = now_ms;
                try self.emit_fence_action(g.*, .expired, actions);
                if (g.parent_epoch == null) {
                    try expired_parents.append(self.gpa, g.epoch);
                }
            }
        }
        for (expired_parents.items) |pe| {
            try self.fence_children_locked(pe, now_ms, actions);
        }

        var qi: usize = 0;
        while (qi < self.queue.items.len) {
            const t = self.queue.items[qi];
            if (now_ms >= t.wait_deadline_ms or now_ms >= t.lease_deadline_ms) {
                const reason: []const u8 = if (now_ms >= t.wait_deadline_ms) "wait_timeout" else "ticket_expired";
                try actions.append(self.gpa, .{ .ticket_refused = .{
                    .ticket_id = try self.gpa.dupe(u8, t.ticket_id),
                    .reason = try self.gpa.dupe(u8, reason),
                } });
                var rem = self.queue.orderedRemove(qi);
                rem.deinit(self.gpa);
            } else {
                qi += 1;
            }
        }

        try self.admit_from_queue(now_ms, mem_avail_bytes, actions);
    }

    pub fn verify_epoch(self: *const Authority, boot_id: []const u8, epoch: u64, now_ms: i64) bool {
        if (self.quarantined_reason != null) return false;
        if (!std.mem.eql(u8, boot_id, self.boot_id)) return false;
        const g = self.find_grant_by_epoch(epoch) orelse return false;
        if (g.state == .fencing) return false;
        if (now_ms >= g.deadline_ms) return false;
        if (g.parent_epoch) |pe| {
            const p = self.find_grant_by_epoch(pe) orelse return false;
            if (p.state != .active or now_ms >= p.deadline_ms) return false;
        }
        return true;
    }

    pub fn find_grant_by_epoch(self: *const Authority, epoch: u64) ?Grant {
        for (self.grants.items) |g| {
            if (g.epoch == epoch) return g;
        }
        return null;
    }

    pub fn find_grant_by_id(self: *const Authority, lease_id: []const u8) ?Grant {
        for (self.grants.items) |g| {
            if (std.mem.eql(u8, g.lease_id, lease_id)) return g;
        }
        return null;
    }

    pub fn find_grant_by_ticket(self: *const Authority, tid: []const u8) ?Grant {
        for (self.grants.items) |g| {
            if (g.ticket_id) |t| {
                if (std.mem.eql(u8, t, tid)) return g;
            }
        }
        return null;
    }

    pub fn find_ticket_by_id(self: *const Authority, tid: []const u8) ?Ticket {
        for (self.queue.items) |t| {
            if (std.mem.eql(u8, t.ticket_id, tid)) return t;
        }
        return null;
    }

    fn find_grant_mut(self: *Authority, lease_id: []const u8, epoch: u64) ?*Grant {
        for (self.grants.items) |*g| {
            if (std.mem.eql(u8, g.lease_id, lease_id) and g.epoch == epoch) return g;
        }
        return null;
    }

    fn insert_grant(
        self: *Authority,
        parent_epoch: ?u64,
        ticket_id_opt: ?[]const u8,
        holder_id: []const u8,
        job_id: ?[]const u8,
        mode: HoldMode,
        res: Resources,
        ttl_ms: i64,
        deadline_ms: i64,
        now_ms: i64,
    ) !*const Grant {
        const lid = try self.next_lease_id();
        errdefer self.gpa.free(lid);
        const tdup = if (ticket_id_opt) |t| try self.gpa.dupe(u8, t) else null;
        errdefer if (tdup) |t| self.gpa.free(t);
        const hdup = try self.gpa.dupe(u8, holder_id);
        errdefer self.gpa.free(hdup);
        const jdup = if (job_id) |j| try self.gpa.dupe(u8, j) else null;
        errdefer if (jdup) |j| self.gpa.free(j);

        const epoch = self.next_epoch;
        self.next_epoch += 1;

        try self.grants.append(self.gpa, .{
            .lease_id = lid,
            .ticket_id = tdup,
            .epoch = epoch,
            .parent_epoch = parent_epoch,
            .holder_id = hdup,
            .job_id = jdup,
            .mode = mode,
            .state = .active,
            .res = res,
            .ttl_ms = ttl_ms,
            .granted_ms = now_ms,
            .deadline_ms = deadline_ms,
        });
        return &self.grants.items[self.grants.items.len - 1];
    }

    fn emit_fence_action(
        self: *Authority,
        g: Grant,
        reason: FenceReason,
        actions: *std.ArrayList(Action),
    ) !void {
        try actions.append(self.gpa, .{ .fence_grant = .{
            .lease_id = try self.gpa.dupe(u8, g.lease_id),
            .epoch = g.epoch,
            .job_id = if (g.job_id) |j| try self.gpa.dupe(u8, j) else null,
            .reason = reason,
        } });
    }

    fn fence_children_locked(
        self: *Authority,
        parent_epoch: u64,
        now_ms: i64,
        actions: *std.ArrayList(Action),
    ) !void {
        for (self.grants.items) |*cg| {
            if (cg.parent_epoch != null and cg.parent_epoch.? == parent_epoch and cg.state != .fencing) {
                cg.state = .fencing;
                cg.fence_started_ms = now_ms;
                try self.emit_fence_action(cg.*, .parent_revoked, actions);
            }
        }
    }

    fn admit_from_queue(
        self: *Authority,
        now_ms: i64,
        mem_avail_bytes: u64,
        actions: *std.ArrayList(Action),
    ) !void {
        if (self.quarantined_reason != null) return;

        while (self.queue.items.len > 0) {
            const head = self.queue.items[0];
            if (now_ms >= head.wait_deadline_ms or now_ms >= head.lease_deadline_ms) {
                const reason: []const u8 = if (now_ms >= head.wait_deadline_ms) "wait_timeout" else "ticket_expired";
                try actions.append(self.gpa, .{ .ticket_refused = .{
                    .ticket_id = try self.gpa.dupe(u8, head.ticket_id),
                    .reason = try self.gpa.dupe(u8, reason),
                } });
                var rem = self.queue.orderedRemove(0);
                rem.deinit(self.gpa);
                continue;
            }
            if (!self.is_statically_satisfiable(head.res)) {
                try actions.append(self.gpa, .{ .ticket_refused = .{
                    .ticket_id = try self.gpa.dupe(u8, head.ticket_id),
                    .reason = try self.gpa.dupe(u8, "exceeds node capacity"),
                } });
                var rem = self.queue.orderedRemove(0);
                rem.deinit(self.gpa);
                continue;
            }
            if (!self.mode_compatible(head.mode) or !self.can_fit(head.res, mem_avail_bytes)) {
                break;
            }

            var popped = self.queue.orderedRemove(0);
            defer popped.deinit(self.gpa);

            const g = try self.insert_grant(
                null,
                popped.ticket_id,
                popped.holder_id,
                popped.job_id,
                popped.mode,
                popped.res,
                popped.ttl_ms,
                now_ms +| popped.ttl_ms,
                now_ms,
            );
            try actions.append(self.gpa, .{ .grant_admitted = .{
                .ticket_id = try self.gpa.dupe(u8, popped.ticket_id),
                .lease_id = try self.gpa.dupe(u8, g.lease_id),
                .epoch = g.epoch,
                .holder_id = try self.gpa.dupe(u8, g.holder_id),
            } });

            if (popped.mode == .exclusive) break;
        }
    }

    pub fn state_hash(self: *const Authority) u64 {
        var h = std.hash.Wyhash.init(0x7127);
        h.update(self.boot_id);
        h.update(std.mem.asBytes(&self.next_epoch));
        h.update(std.mem.asBytes(&self.next_seq));
        if (self.quarantined_reason) |qr| {
            h.update(&.{1});
            h.update(qr);
        } else {
            h.update(&.{0});
        }
        const glen = self.grants.items.len;
        h.update(std.mem.asBytes(&glen));
        for (self.grants.items) |g| {
            h.update(g.lease_id);
            h.update(std.mem.asBytes(&g.epoch));
            const has_pe: u8 = if (g.parent_epoch != null) 1 else 0;
            h.update(&.{has_pe});
            if (g.parent_epoch) |pe| h.update(std.mem.asBytes(&pe));
            h.update(g.holder_id);
            const m: u8 = @intFromEnum(g.mode);
            const st: u8 = @intFromEnum(g.state);
            h.update(&.{ m, st });
            h.update(std.mem.asBytes(&g.res.slots));
            h.update(std.mem.asBytes(&g.res.long_slots));
            h.update(std.mem.asBytes(&g.res.mem_bytes));
            h.update(std.mem.asBytes(&g.deadline_ms));
        }
        const qlen = self.queue.items.len;
        h.update(std.mem.asBytes(&qlen));
        for (self.queue.items) |t| {
            h.update(t.ticket_id);
            h.update(std.mem.asBytes(&t.seq));
            h.update(t.holder_id);
            const m: u8 = @intFromEnum(t.mode);
            h.update(&.{m});
            h.update(std.mem.asBytes(&t.res.slots));
            h.update(std.mem.asBytes(&t.res.long_slots));
            h.update(std.mem.asBytes(&t.res.mem_bytes));
        }
        return h.final();
    }
};
