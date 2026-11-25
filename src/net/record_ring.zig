const std = @import("std");
const tiger = @import("../tiger.zig");

pub const header_len: usize = 12;

pub const Peek = union(enum) {
    empty,
    lagged,
    record: struct { seq: u64, len: u32, next: u64 },
};

/// A log of length-prefixed records in a fixed byte ring. One writer at a time appends and evicts the
/// oldest records to make room; readers take no lock: they copy bytes out and then check that the record
/// was not overwritten, and a reader whose position was evicted sees lagged. Positions only grow.
pub fn RecordRing(comptime capacity: usize) type {
    tiger.comptime_assert(capacity > 2 * header_len and capacity <= std.math.maxInt(u32), "ring capacity");
    return struct {
        const Self = @This();
        pub const bytes = capacity;

        data: []u8,
        head: std.atomic.Value(u64) = .init(0),
        tail: std.atomic.Value(u64) = .init(0),

        pub fn init(data: []u8) Self {
            tiger.assert(data.len == capacity);
            return .{ .data = data };
        }

        /// Caller serializes writers. A record, header included, must fit in half the ring.
        pub fn append(self: *Self, seq: u64, payload: []const u8) void {
            const total: u64 = header_len + payload.len;
            tiger.assert(total <= capacity / 2);
            const head = self.head.load(.monotonic);
            var tail = self.tail.load(.monotonic);
            const original = tail;
            while (head + total - tail > capacity) {
                var header: [header_len]u8 = undefined;
                self.copy_in(tail, &header);
                tail += header_len + std.mem.readInt(u32, header[0..4], .little);
            }
            if (tail != original) _ = self.tail.swap(tail, .seq_cst);
            var header: [header_len]u8 = undefined;
            std.mem.writeInt(u32, header[0..4], @intCast(payload.len), .little);
            std.mem.writeInt(u64, header[4..12], seq, .little);
            self.copy_to(head, &header);
            self.copy_to(head + header_len, payload);
            self.head.store(head + total, .release);
        }

        fn copy_to(self: *Self, pos: u64, src: []const u8) void {
            const at: usize = @intCast(pos % capacity);
            const first = @min(src.len, capacity - at);
            @memcpy(self.data[at..][0..first], src[0..first]);
            @memcpy(self.data[0 .. src.len - first], src[first..]);
        }

        fn copy_in(self: *const Self, pos: u64, dst: []u8) void {
            const at: usize = @intCast(pos % capacity);
            const first = @min(dst.len, capacity - at);
            @memcpy(dst[0..first], self.data[at..][0..first]);
            @memcpy(dst[first..], self.data[0 .. dst.len - first]);
        }

        fn valid(self: *Self, pos: u64) bool {
            return self.tail.fetchAdd(0, .acq_rel) <= pos;
        }

        pub fn head_pos(self: *const Self) u64 {
            return self.head.load(.acquire);
        }

        /// Looks at the record that starts at pos, which must be a record boundary this reader has kept up to.
        pub fn peek(self: *Self, pos: u64) Peek {
            const head = self.head.load(.acquire);
            if (pos == head) return .empty;
            tiger.assert(pos < head);
            if (pos < self.tail.load(.acquire)) return .lagged;
            var header: [header_len]u8 = undefined;
            self.copy_in(pos, &header);
            if (!self.valid(pos)) return .lagged;
            const len = std.mem.readInt(u32, header[0..4], .little);
            return .{ .record = .{ .seq = std.mem.readInt(u64, header[4..12], .little), .len = len, .next = pos + header_len + len } };
        }

        /// Copies payload bytes of the record at pos, starting at offset. False means the record was overwritten.
        pub fn read(self: *Self, pos: u64, offset: usize, dst: []u8) bool {
            self.copy_in(pos + header_len + offset, dst);
            return self.valid(pos);
        }

        /// The position of the first record with a sequence above seq. Null means the ring cannot give the
        /// records right after seq: the oldest kept one is further ahead, or the ring kept moving under the scan.
        /// Records carry consecutive sequence numbers.
        pub fn find_after(self: *Self, seq: u64) ?u64 {
            var attempts: usize = 0;
            scan: while (attempts < 4) : (attempts += 1) {
                var pos = self.tail.load(.acquire);
                var first = true;
                while (true) {
                    switch (self.peek(pos)) {
                        .empty => return pos,
                        .lagged => continue :scan,
                        .record => |r| {
                            if (first and r.seq > seq + 1) return null;
                            first = false;
                            if (r.seq > seq) return pos;
                            pos = r.next;
                        },
                    }
                }
            }
            return null;
        }
    };
}
