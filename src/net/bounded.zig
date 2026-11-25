const std = @import("std");
const tiger = @import("../tiger.zig");

pub const fleet_slots = 64;
pub const rpc_slots_per_node = 16;
pub const slab_bytes = 256 * 1024;
pub const slab_count = 64;
pub const control_reserve = 8;
pub const frame_slab_limit = (1_048_580 + slab_bytes - 1) / slab_bytes;
pub const tick_ms = 50;

comptime {
    tiger.comptime_assert(control_reserve < slab_count, "control reserve must fit");
    tiger.comptime_assert(control_reserve > frame_slab_limit, "reserve must admit a maximum frame");
    tiger.comptime_assert(slab_count > control_reserve + 2 * frame_slab_limit, "pool must admit a send and receive frame");
    tiger.comptime_assert(slab_count * slab_bytes == 16 * 1024 * 1024, "slab budget");
}

pub const StaticAllocator = struct {
    parent: std.mem.Allocator,
    state: enum { init, static, deinit } = .init,
    live: usize = 0,

    pub fn allocator(self: *StaticAllocator) std.mem.Allocator {
        return .{ .ptr = self, .vtable = &vtable };
    }

    pub fn freeze(self: *StaticAllocator) void {
        tiger.assert(self.state == .init);
        self.state = .static;
    }

    pub fn begin_deinit(self: *StaticAllocator) void {
        tiger.assert(self.state == .static);
        self.state = .deinit;
    }

    pub fn finish_deinit(self: *const StaticAllocator) void {
        tiger.assert(self.state == .deinit and self.live == 0);
    }

    const vtable: std.mem.Allocator.VTable = .{
        .alloc = alloc,
        .resize = resize,
        .remap = remap,
        .free = free,
    };

    fn alloc(ctx: *anyopaque, len: usize, alignment: std.mem.Alignment, ra: usize) ?[*]u8 {
        const self: *StaticAllocator = @ptrCast(@alignCast(ctx));
        tiger.assert(self.state == .init);
        const result = self.parent.rawAlloc(len, alignment, ra) orelse return null;
        self.live += 1;
        return result;
    }

    fn resize(ctx: *anyopaque, memory: []u8, alignment: std.mem.Alignment, len: usize, ra: usize) bool {
        const self: *StaticAllocator = @ptrCast(@alignCast(ctx));
        tiger.assert(self.state == .init);
        return self.parent.rawResize(memory, alignment, len, ra);
    }

    fn remap(ctx: *anyopaque, memory: []u8, alignment: std.mem.Alignment, len: usize, ra: usize) ?[*]u8 {
        const self: *StaticAllocator = @ptrCast(@alignCast(ctx));
        tiger.assert(self.state == .init);
        return self.parent.rawRemap(memory, alignment, len, ra);
    }

    fn free(ctx: *anyopaque, memory: []u8, alignment: std.mem.Alignment, ra: usize) void {
        const self: *StaticAllocator = @ptrCast(@alignCast(ctx));
        tiger.assert(self.state != .static and self.live > 0);
        self.live -= 1;
        self.parent.rawFree(memory, alignment, ra);
    }
};

pub fn Ring(comptime T: type, comptime capacity: usize) type {
    tiger.comptime_assert(capacity > 0, "ring capacity must be positive");
    return struct {
        const Self = @This();
        items: [capacity]T = undefined,
        head: usize = 0,
        len: usize = 0,

        pub fn push(self: *Self, item: T) error{Full}!void {
            if (self.len == capacity) return error.Full;
            self.items[(self.head + self.len) % capacity] = item;
            self.len += 1;
        }

        pub fn pop(self: *Self) ?T {
            if (self.len == 0) return null;
            const item = self.items[self.head];
            self.head = (self.head + 1) % capacity;
            self.len -= 1;
            return item;
        }
    };
}

pub const SlabPool = struct {
    pub const Index = u8;
    const none = std.math.maxInt(Index);
    data: []u8,
    next: [slab_count]Index = undefined,
    refs: [slab_count]u16 = @splat(0),
    head: Index = 0,
    free_count: usize = slab_count,

    pub fn init(allocator: std.mem.Allocator) !SlabPool {
        var self: SlabPool = .{ .data = try allocator.alloc(u8, slab_count * slab_bytes) };
        for (&self.next, 0..) |*next, i| next.* = if (i + 1 == slab_count) none else @intCast(i + 1);
        tiger.assert(self.data.len == slab_count * slab_bytes);
        return self;
    }

    pub fn deinit(self: *SlabPool, allocator: std.mem.Allocator) void {
        tiger.assert(self.free_count == slab_count);
        allocator.free(self.data);
        self.* = undefined;
    }

    pub fn acquire(self: *SlabPool, control: bool) ?Index {
        if (self.free_count == 0 or (!control and self.free_count <= control_reserve)) return null;
        const index = self.head;
        tiger.assert(index < slab_count and self.refs[index] == 0);
        self.head = self.next[index];
        self.refs[index] = 1;
        self.free_count -= 1;
        return index;
    }

    pub fn acquire_send(self: *SlabPool, control: bool) ?Index {
        const reserve: usize = if (control) 1 else control_reserve + frame_slab_limit;
        if (self.free_count <= reserve) return null;
        return self.acquire(true);
    }

    pub fn retain(self: *SlabPool, index: Index) void {
        tiger.assert(index < slab_count and self.refs[index] > 0 and self.refs[index] < std.math.maxInt(u16));
        self.refs[index] += 1;
    }

    pub fn release(self: *SlabPool, index: Index) void {
        tiger.assert(index < slab_count and self.refs[index] > 0);
        self.refs[index] -= 1;
        if (self.refs[index] == 0) {
            self.next[index] = self.head;
            self.head = index;
            self.free_count += 1;
        }
    }

    pub fn bytes(self: *SlabPool, index: Index) []u8 {
        tiger.assert(index < slab_count and self.refs[index] > 0);
        const start = @as(usize, index) * slab_bytes;
        return self.data[start .. start + slab_bytes];
    }

    pub fn assert_conservation(self: *const SlabPool) void {
        var free: usize = 0;
        for (self.refs) |refs| if (refs == 0) {
            free += 1;
        };
        tiger.assert(free == self.free_count);
        var index = self.head;
        var visited: usize = 0;
        while (index != none) {
            tiger.assert(index < slab_count and self.refs[index] == 0 and visited < slab_count);
            visited += 1;
            index = self.next[index];
        }
        tiger.assert(visited == free);
    }
};

pub const FrameBuffer = struct {
    pub const max_bytes = 1_048_580;
    const max_slabs = frame_slab_limit;
    slabs: [max_slabs]SlabPool.Index = undefined,
    count: usize = 0,
    len: usize = 0,

    pub fn reserve(self: *FrameBuffer, pool: *SlabPool, capacity: usize, control: bool) error{ Full, TooLarge }!void {
        if (capacity > max_bytes) return error.TooLarge;
        const required = (capacity + slab_bytes - 1) / slab_bytes;
        const original_count = self.count;
        while (self.count < required) {
            self.slabs[self.count] = pool.acquire(control) orelse {
                while (self.count > original_count) {
                    self.count -= 1;
                    pool.release(self.slabs[self.count]);
                }
                return error.Full;
            };
            self.count += 1;
        }
    }

    pub fn append(self: *FrameBuffer, pool: *SlabPool, source: []const u8, control: bool) error{ Full, TooLarge }!void {
        if (source.len > max_bytes - self.len) return error.TooLarge;
        try self.reserve(pool, self.len + source.len, control);
        var offset: usize = 0;
        while (offset < source.len) {
            const index = self.len / slab_bytes;
            const at = self.len % slab_bytes;
            const n = @min(source.len - offset, slab_bytes - at);
            @memcpy(pool.bytes(self.slabs[index])[at..][0..n], source[offset..][0..n]);
            self.len += n;
            offset += n;
        }
    }

    pub fn reserve_send(self: *FrameBuffer, pool: *SlabPool, capacity: usize, control: bool) error{ Full, TooLarge }!void {
        if (capacity > max_bytes) return error.TooLarge;
        const required = (capacity + slab_bytes - 1) / slab_bytes;
        const original_count = self.count;
        while (self.count < required) {
            self.slabs[self.count] = pool.acquire_send(control) orelse {
                while (self.count > original_count) {
                    self.count -= 1;
                    pool.release(self.slabs[self.count]);
                }
                return error.Full;
            };
            self.count += 1;
        }
    }

    pub fn read(self: *const FrameBuffer, pool: *SlabPool, offset: usize, destination: []u8) void {
        tiger.assert(offset <= self.len and destination.len <= self.len - offset);
        var copied: usize = 0;
        while (copied < destination.len) {
            const position = offset + copied;
            const at = position % slab_bytes;
            const n = @min(destination.len - copied, slab_bytes - at);
            @memcpy(destination[copied..][0..n], pool.bytes(self.slabs[position / slab_bytes])[at..][0..n]);
            copied += n;
        }
    }

    pub fn clear(self: *FrameBuffer, pool: *SlabPool) void {
        for (self.slabs[0..self.count]) |index| pool.release(index);
        self.count = 0;
        self.len = 0;
    }
};

pub const Timeout = struct {
    deadline: u64 = 0,
    delay: u64,
    maximum: u64,

    pub fn start(self: *Timeout, now: u64, random: u64) void {
        tiger.assert(self.delay > 0 and self.delay <= self.maximum);
        const jitter = random % (self.delay / 4 + 1);
        self.deadline = now +| @min(self.maximum, self.delay +| jitter);
    }

    pub fn backoff(self: *Timeout) void {
        self.delay = @min(self.maximum, self.delay *| 2);
    }

    pub fn expired(self: *const Timeout, now: u64) bool {
        return now >= self.deadline;
    }
};
