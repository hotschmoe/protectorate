const builtin = @import("builtin");

/// The completion backend for this target; every one implements the contract in io.zig plus wake, listen and close_now.
pub const Backend = switch (builtin.os.tag) {
    .linux => @import("io_epoll.zig").Epoll,
    .windows => @import("io_iocp.zig").Iocp,
    .macos, .freebsd, .netbsd, .openbsd, .dragonfly => @import("io_kqueue.zig").Kqueue,
    else => @compileError("no network completion backend for this target"),
};
