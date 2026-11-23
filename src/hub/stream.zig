const std = @import("std");
const tiger = @import("../tiger.zig");
const schema = @import("../api/schema.zig");

pub const Published = struct {
    seq: u64,
    json: []const u8,
};

pub const StreamHub = struct {
    gpa: std.mem.Allocator,
    io: std.Io,
    mu: std.Io.Mutex = .init,
    cond: std.Io.Condition = .init,
    closed: bool = false,
    next_seq: u64 = 1,
    ring_cap: usize = 512,
    items: std.ArrayList(Published) = .empty,

    pub fn init(gpa: std.mem.Allocator, io: std.Io) StreamHub {
        return .{ .gpa = gpa, .io = io };
    }

    pub fn deinit(self: *StreamHub) void {
        self.mu.lockUncancelable(self.io);
        self.closed = true;
        for (self.items.items) |it| self.gpa.free(it.json);
        self.items.deinit(self.gpa);
        self.cond.broadcast(self.io);
        self.mu.unlock(self.io);
    }

    pub fn wake_all(self: *StreamHub) void {
        self.mu.lockUncancelable(self.io);
        self.closed = true;
        self.cond.broadcast(self.io);
        self.mu.unlock(self.io);
    }

    pub fn current_seq(self: *StreamHub) u64 {
        self.mu.lockUncancelable(self.io);
        defer self.mu.unlock(self.io);
        return self.next_seq - 1;
    }

    pub fn publish_raw(self: *StreamHub, json_msg_with_seq_placeholder: schema.HubStreamMessage) void {
        var msg = json_msg_with_seq_placeholder;
        self.mu.lockUncancelable(self.io);
        const seq = self.next_seq;
        self.next_seq += 1;
        switch (msg) {
            .unknown => {},
            inline else => |*payload| payload.seq = seq,
        }
        const raw = schema.encode_alloc(self.gpa, msg) catch {
            self.mu.unlock(self.io);
            return;
        };
        if (self.items.items.len >= self.ring_cap) {
            const oldest = self.items.orderedRemove(0);
            self.gpa.free(oldest.json);
        }
        self.items.append(self.gpa, .{ .seq = seq, .json = raw }) catch {
            self.gpa.free(raw);
        };
        self.cond.broadcast(self.io);
        self.mu.unlock(self.io);
    }

    pub fn wait_since(
        self: *StreamHub,
        arena: std.mem.Allocator,
        after_seq: u64,
        timeout_ms: i64,
    ) !struct { msgs: [][]const u8, last_seq: u64, closed: bool } {
        self.mu.lockUncancelable(self.io);
        defer self.mu.unlock(self.io);

        if (!self.closed and self.latest_seq_locked() <= after_seq and timeout_ms > 0) {
            self.cond.waitTimeout(self.io, &self.mu, .{
                .duration = .{ .raw = .fromMilliseconds(timeout_ms), .clock = .awake },
            }) catch {};
        }

        var out: std.ArrayList([]const u8) = .empty;
        var last = after_seq;
        for (self.items.items) |it| {
            if (it.seq > after_seq) {
                try out.append(arena, try arena.dupe(u8, it.json));
                last = it.seq;
            }
        }
        return .{ .msgs = out.items, .last_seq = last, .closed = self.closed };
    }

    fn latest_seq_locked(self: *const StreamHub) u64 {
        if (self.items.items.len == 0) return self.next_seq - 1;
        return self.items.items[self.items.items.len - 1].seq;
    }
};
