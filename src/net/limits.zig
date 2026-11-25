const std = @import("std");
const tiger = @import("../tiger.zig");
const frame = @import("frame.zig");

pub const piece_max: usize = 256 * 1024;
pub const artifact_max_bytes: u64 = 64 << 20;
pub const rpc_chunks_max: usize = piece_max;
pub const http_conns_max: u32 = 32;
pub const fleet_hello_max: usize = 64 * 1024;
pub const fleet_control_reply_max: usize = frame.max_payload;

comptime {
    tiger.comptime_assert(piece_max <= frame.max_payload, "a piece must fit one frame");
    tiger.comptime_assert(rpc_chunks_max >= piece_max, "a call must hold one full piece");
    tiger.comptime_assert(artifact_max_bytes >= piece_max, "the artifact cap must hold one piece");
}

pub const Admit = enum { admitted, refused, refused_first };

/// Counts live connections against a ceiling. Only the accept thread calls try_enter.
pub const ConnGate = struct {
    max: u32,
    active: std.atomic.Value(u32) = .init(0),
    refused_total: std.atomic.Value(u64) = .init(0),
    refusing: std.atomic.Value(bool) = .init(false),

    pub fn init(max: u32) ConnGate {
        tiger.assert(max > 0);
        return .{ .max = max };
    }

    /// refused_first marks the first refusal of a burst; a burst ends at the next admission.
    pub fn try_enter(self: *ConnGate) Admit {
        const prev = self.active.fetchAdd(1, .acq_rel);
        if (prev >= self.max) {
            _ = self.active.fetchSub(1, .acq_rel);
            _ = self.refused_total.fetchAdd(1, .monotonic);
            return if (self.refusing.swap(true, .acq_rel)) .refused else .refused_first;
        }
        self.refusing.store(false, .release);
        return .admitted;
    }

    pub fn leave(self: *ConnGate) void {
        const prev = self.active.fetchSub(1, .acq_rel);
        tiger.assert(prev > 0);
    }
};
