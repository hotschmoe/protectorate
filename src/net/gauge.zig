const std = @import("std");

pub const Reading = struct {
    name: []const u8,
    size: u32,
    reserved: u32,
    used: u32,
    high_water: u32,
    refused: u64,
};

/// One pool's budget as seen from other threads: the owner sets the counters, anyone reads them.
pub const Gauge = struct {
    size: u32,
    reserved: u32,
    used: std.atomic.Value(u32) = .init(0),
    high: std.atomic.Value(u32) = .init(0),
    refused: std.atomic.Value(u64) = .init(0),

    pub fn init(size: u32, reserved: u32) Gauge {
        return .{ .size = size, .reserved = reserved };
    }

    pub fn set(self: *Gauge, used: usize) void {
        const value: u32 = @intCast(used);
        self.used.store(value, .monotonic);
        _ = self.high.fetchMax(value, .monotonic);
    }

    pub fn refuse(self: *Gauge) void {
        _ = self.refused.fetchAdd(1, .monotonic);
    }

    /// For a pool that keeps its own high-water mark and refusal count.
    pub fn publish(self: *Gauge, used: usize, high: usize, refused: u64) void {
        self.used.store(@intCast(used), .monotonic);
        self.high.store(@intCast(high), .monotonic);
        self.refused.store(refused, .monotonic);
    }

    pub fn read(self: *const Gauge, name: []const u8) Reading {
        return .{
            .name = name,
            .size = self.size,
            .reserved = self.reserved,
            .used = self.used.load(.monotonic),
            .high_water = self.high.load(.monotonic),
            .refused = self.refused.load(.monotonic),
        };
    }
};
