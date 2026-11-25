const bounded = @import("../net/bounded.zig");

/// Pool sizes a simulation run may shrink below the production constants, which stay comptime.
pub const Sizes = struct {
    slabs: usize = bounded.slab_count,
    bus_slots: usize = bounded.fleet_slots,
    rpc_per_peer: usize = bounded.rpc_slots_per_node,
    request_slots: usize = 16,

    pub fn tiny(self: Sizes) bool {
        return self.slabs != bounded.slab_count or self.bus_slots != bounded.fleet_slots or
            self.rpc_per_peer != bounded.rpc_slots_per_node or self.request_slots != 16;
    }
};
