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
