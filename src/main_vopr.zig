const std = @import("std");
const vopr = @import("dst/vopr.zig");

pub const main = vopr.main;
pub const panic = std.debug.FullPanic(vopr.panic_handler);
