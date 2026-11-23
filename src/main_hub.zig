const std = @import("std");
const builtin = @import("builtin");
const commands = @import("cli/commands.zig");
const hub_home = @import("hub/home.zig");
const hub_rt = @import("hub/runtime.zig");
const hub_http = @import("hub/http_server.zig");

var sig_stop: std.atomic.Value(bool) = .init(false);

fn handle_sig(_: std.posix.SIG) callconv(.c) void {
    sig_stop.store(true, .release);
}

fn install_signal_handlers() void {
    if (comptime builtin.os.tag == .linux or builtin.os.tag == .macos) {
        const act: std.posix.Sigaction = .{
            .handler = .{ .handler = handle_sig },
            .mask = std.posix.sigemptyset(),
            .flags = 0,
        };
        std.posix.sigaction(.INT, &act, null);
        std.posix.sigaction(.TERM, &act, null);
    }
}

pub fn main(init: std.process.Init) !u8 {
    var arena_st = std.heap.ArenaAllocator.init(init.gpa);
    defer arena_st.deinit();

    switch (try commands.dispatch_cli(init, arena_st.allocator())) {
        .exit => |code| return code,
        .token => |t| {
            const hp = try hub_home.HubHome.resolve_path(init.gpa, init.environ_map, t.home_opt);
            defer init.gpa.free(hp);
            var hh = try hub_home.HubHome.open(init.gpa, init.io, hp, t.dev_token_opt);
            defer hh.deinit();
            try commands.print_out(init.io, "owner={s}\nenvoy={s}\n", .{ hh.owner_token, hh.envoy_token });
            return 0;
        },
        .serve => |s| {
            install_signal_handlers();
            const rt = try hub_rt.Runtime.open(init.gpa, init.io, init.environ_map, .{
                .home_override = s.home_opt,
                .listen_override = s.listen_opt,
                .fleet_listen_override = s.fleet_opt,
                .dev_token_override = s.dev_token_opt,
                .no_sandbox = s.no_sandbox,
            });
            defer rt.close_and_destroy();

            const hs = try hub_http.HttpServer.start(init.gpa, init.io, rt, rt.http_listen);
            defer hs.stop_and_destroy();

            try commands.print_out(
                init.io,
                "protectorate listening http=127.0.0.1:{d} fleet=0.0.0.0:{d} home={s}\n",
                .{ hs.listen_port(), rt.fleet_srv.listen_port(), rt.hub_home.root_path },
            );

            while (!sig_stop.load(.acquire) and !rt.shutdown_requested.load(.acquire)) {
                init.io.sleep(.fromMilliseconds(200), .awake) catch break;
            }
            return 0;
        },
    }
}
