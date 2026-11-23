const std = @import("std");

pub fn assert(ok: bool) void {
    if (!ok) @panic("tiger: assertion failed");
}

pub fn assert_fmt(ok: bool, comptime fmt: []const u8, args: anytype) void {
    if (!ok) std.debug.panic(fmt, args);
}

pub fn comptime_assert(comptime ok: bool, comptime msg: []const u8) void {
    if (!ok) @compileError(msg);
}

pub fn copy_disjoint(comptime T: type, dst: []T, src: []const T) void {
    assert(dst.len == src.len);
    const d0 = @intFromPtr(dst.ptr);
    const d1 = d0 + dst.len * @sizeOf(T);
    const s0 = @intFromPtr(src.ptr);
    const s1 = s0 + src.len * @sizeOf(T);
    assert(d1 <= s0 or s1 <= d0);
    @memcpy(dst, src);
}

pub fn add_sat(a: u64, b: u64) u64 {
    return a +| b;
}

pub fn sub_sat(a: u64, b: u64) u64 {
    return a -| b;
}

pub fn BoundedArray(comptime T: type, comptime capacity: usize) type {
    comptime_assert(capacity > 0, "BoundedArray capacity must be positive");
    return struct {
        const Self = @This();
        buffer: [capacity]T = undefined,
        len: usize = 0,

        pub fn init() Self {
            return .{};
        }

        pub fn slice(self: *const Self) []const T {
            assert(self.len <= capacity);
            return self.buffer[0..self.len];
        }

        pub fn slice_mut(self: *Self) []T {
            assert(self.len <= capacity);
            return self.buffer[0..self.len];
        }

        pub fn append(self: *Self, item: T) error{Overflow}!void {
            if (self.len >= capacity) return error.Overflow;
            self.buffer[self.len] = item;
            self.len += 1;
        }

        pub fn append_assume_capacity(self: *Self, item: T) void {
            assert(self.len < capacity);
            self.buffer[self.len] = item;
            self.len += 1;
        }

        pub fn append_slice(self: *Self, items: []const T) error{Overflow}!void {
            if (items.len > capacity - self.len) return error.Overflow;
            for (items) |item| {
                self.buffer[self.len] = item;
                self.len += 1;
            }
        }

        pub fn clear(self: *Self) void {
            self.len = 0;
        }

        pub fn pop(self: *Self) ?T {
            if (self.len == 0) return null;
            self.len -= 1;
            return self.buffer[self.len];
        }

        pub fn swap_remove(self: *Self, index: usize) T {
            assert(index < self.len);
            const removed = self.buffer[index];
            self.len -= 1;
            if (index < self.len) {
                self.buffer[index] = self.buffer[self.len];
            }
            return removed;
        }
    };
}
