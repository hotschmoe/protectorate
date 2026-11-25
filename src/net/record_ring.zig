const std = @import("std");
const tiger = @import("../tiger.zig");

pub const header_len: usize = 16;

pub const Peek = union(enum) {
    empty,
    lagged,
    record: struct { seq: u64, len: u32, next: u64 },
};

pub const NoHook = struct {
    pub inline fn point(_: *NoHook) void {}
};

fn padded(len: usize) usize {
    return (len + 7) & ~@as(usize, 7);
}

/// A log of length-prefixed records in a fixed ring of 64-bit words. One writer at a time appends and
/// evicts the oldest records to make room; readers take no lock. Every access to the storage is an
/// atomic word access, so a reader that is overtaken reads defined (if stale or mixed) values, never a
/// data race: the writer moves tail past a record before it stores any word of its space, with release
/// stores, and a reader copies with acquire loads and then checks tail again, so a copy that saw any
/// overwriting word also sees the new tail and reports lagged. Positions only grow and are multiples
/// of eight at record boundaries. Hook.point runs after each word access so a simulation can interleave
/// a second side between any two of them.
pub fn RecordRing(comptime capacity: usize, comptime Hook: type) type {
    tiger.comptime_assert(capacity % 8 == 0 and capacity > 4 * header_len and capacity <= std.math.maxInt(u32), "ring capacity");
    return struct {
        const Self = @This();
        pub const bytes = capacity;
        pub const words = capacity / 8;

        data: []std.atomic.Value(u64),
        head: std.atomic.Value(u64) = .init(0),
        tail: std.atomic.Value(u64) = .init(0),
        hook: Hook,

        pub fn init(data: []std.atomic.Value(u64), hook: Hook) Self {
            tiger.assert(data.len == words);
            return .{ .data = data, .hook = hook };
        }

        /// Caller serializes writers. A record, header and padding included, must fit in half the ring.
        pub fn append(self: *Self, seq: u64, payload: []const u8) void {
            const total: u64 = header_len + padded(payload.len);
            tiger.assert(total <= capacity / 2);
            const head = self.head.load(.monotonic);
            var tail = self.tail.load(.monotonic);
            const original = tail;
            while (head + total - tail > capacity) {
                var header: [header_len]u8 = undefined;
                self.copy_in(tail, &header);
                tail += header_len + padded(std.mem.readInt(u64, header[0..8], .little));
            }
            if (tail != original) {
                self.tail.store(tail, .seq_cst);
                self.hook.point();
            }
            var header: [header_len]u8 = undefined;
            std.mem.writeInt(u64, header[0..8], payload.len, .little);
            std.mem.writeInt(u64, header[8..16], seq, .little);
            self.copy_to(head, &header);
            self.copy_to(head + header_len, payload);
            self.hook.point();
            self.head.store(head + total, .release);
        }

        fn copy_to(self: *Self, pos: u64, src: []const u8) void {
            tiger.assert(pos % 8 == 0);
            var word: usize = @intCast((pos / 8) % words);
            var at: usize = 0;
            while (at < src.len) : (at += 8) {
                var value: u64 = 0;
                const n = @min(8, src.len - at);
                @memcpy(std.mem.asBytes(&value)[0..n], src[at..][0..n]);
                self.data[word].store(value, .release);
                self.hook.point();
                word = (word + 1) % words;
            }
        }

        fn copy_in(self: *Self, pos: u64, dst: []u8) void {
            var at: usize = 0;
            var p = pos;
            while (at < dst.len) {
                const word: usize = @intCast((p / 8) % words);
                const skip: usize = @intCast(p % 8);
                const value = self.data[word].load(.acquire);
                self.hook.point();
                const n = @min(8 - skip, dst.len - at);
                @memcpy(dst[at..][0..n], std.mem.asBytes(&value)[skip..][0..n]);
                at += n;
                p += n;
            }
        }

        fn valid(self: *Self, pos: u64) bool {
            return self.tail.load(.acquire) <= pos;
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
            const len = std.mem.readInt(u64, header[0..8], .little);
            return .{ .record = .{ .seq = std.mem.readInt(u64, header[8..16], .little), .len = @intCast(len), .next = pos + header_len + padded(len) } };
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
