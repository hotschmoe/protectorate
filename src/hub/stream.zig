const std = @import("std");
const builtin = @import("builtin");
const tiger = @import("../tiger.zig");
const schema = @import("../api/schema.zig");
const record_ring = @import("../net/record_ring.zig");

pub const ring_bytes: usize = 8 * 1024 * 1024;
pub const record_max: usize = 3 * 1024 * 1024;

pub const Ring = record_ring.RecordRing(ring_bytes, record_ring.NoHook);
pub const Peek = record_ring.Peek;

comptime {
    tiger.comptime_assert(record_max + record_ring.header_len + 7 <= ring_bytes / 2, "the ring holds at least two maximum records");
}

/// The sequence-numbered log of encoded stream messages. Publishers serialize on a spin lock for the
/// length of one encode and copy; readers (the HTTP loop) take no lock. A record with no payload marks
/// a message too large to keep, or one that failed to encode: readers treat it as a gap and resync from
/// a snapshot, as they do when they fall more than one ring behind.
pub const StreamHub = struct {
    gpa: std.mem.Allocator,
    io: std.Io,
    ring: Ring,
    last_seq: std.atomic.Value(u64) = .init(0),
    notify: std.atomic.Value(u32) = .init(0),
    writing: std.atomic.Mutex = .unlocked,
    closed: std.atomic.Value(bool) = .init(false),
    wake_fd: std.atomic.Value(i32) = .init(-1),

    pub fn init(gpa: std.mem.Allocator, io: std.Io) !StreamHub {
        return .{ .gpa = gpa, .io = io, .ring = .init(try gpa.alloc(std.atomic.Value(u64), Ring.words), .{}) };
    }

    pub fn deinit(self: *StreamHub) void {
        self.wake_all();
        self.set_wake_fd(-1);
        self.gpa.free(self.ring.data);
        self.ring.data = &.{};
    }

    /// Takes ownership of a duplicate of the loop's wakeup descriptor: every publication writes to it. Pass -1 to detach.
    pub fn set_wake_fd(self: *StreamHub, fd: i32) void {
        const previous = self.wake_fd.swap(fd, .acq_rel);
        if (comptime builtin.os.tag == .linux) {
            if (previous >= 0) _ = std.os.linux.close(previous);
        }
    }

    pub fn wake_all(self: *StreamHub) void {
        self.closed.store(true, .release);
        _ = self.notify.fetchAdd(1, .release);
        std.Io.futexWake(self.io, u32, &self.notify.raw, std.math.maxInt(u32));
        self.poke();
    }

    pub fn current_seq(self: *const StreamHub) u64 {
        return self.last_seq.load(.acquire);
    }

    fn poke(self: *StreamHub) void {
        if (comptime builtin.os.tag != .linux) return;
        const fd = self.wake_fd.load(.acquire);
        if (fd < 0) return;
        const one: u64 = 1;
        _ = std.os.linux.write(fd, std.mem.asBytes(&one).ptr, @sizeOf(u64));
    }

    pub fn publish_raw(self: *StreamHub, json_msg_with_seq_placeholder: schema.HubStreamMessage) void {
        var msg = json_msg_with_seq_placeholder;
        while (!self.writing.tryLock()) std.atomic.spinLoopHint();
        const seq = self.last_seq.load(.monotonic) + 1;
        switch (msg) {
            .unknown => {},
            inline else => |*payload| payload.seq = seq,
        }
        const raw: ?[]u8 = schema.encode_alloc(self.gpa, msg) catch null;
        const keep: []const u8 = if (raw) |r| (if (r.len <= record_max) r else &.{}) else &.{};
        self.ring.append(seq, keep);
        self.last_seq.store(seq, .release);
        self.writing.unlock();
        if (raw) |r| self.gpa.free(r);
        _ = self.notify.fetchAdd(1, .release);
        std.Io.futexWake(self.io, u32, &self.notify.raw, std.math.maxInt(u32));
        self.poke();
    }

    /// Waits until a message above after_seq exists, the timeout passes or the hub closes, and returns the newest sequence.
    pub fn wait_change(self: *StreamHub, after_seq: u64, timeout_ms: i64) error{StreamClosed}!u64 {
        const deadline = std.Io.Clock.awake.now(self.io).toMilliseconds() + @max(timeout_ms, 0);
        while (true) {
            const word = self.notify.load(.acquire);
            if (self.closed.load(.acquire)) return error.StreamClosed;
            const seq = self.last_seq.load(.acquire);
            if (seq > after_seq) return seq;
            const left = deadline - std.Io.Clock.awake.now(self.io).toMilliseconds();
            if (left <= 0) return seq;
            std.Io.futexWaitTimeout(self.io, u32, &self.notify.raw, word, .{
                .duration = .{ .raw = .fromMilliseconds(left), .clock = .awake },
            }) catch {};
        }
    }
};
