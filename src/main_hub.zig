const std = @import("std");
const builtin = @import("builtin");
const s_exec = @import("sandbox/exec.zig");
const commands = @import("cli/commands.zig");
const hub_home = @import("hub/home.zig");
const hub_rt = @import("hub/runtime.zig");
const hub_http = @import("hub/http_server.zig");
const child_process = @import("child_process.zig");

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
    const arena = arena_st.allocator();

    const raw_args = try init.minimal.args.toSlice(arena);
    if (raw_args.len >= 2 and std.mem.eql(u8, raw_args[1], "__guard")) {
        return try s_exec.run_guard_trampoline(init, raw_args[2..]);
    }
    if (raw_args.len >= 2 and std.mem.eql(u8, raw_args[1], "__sleeve_cgroup")) {
        return try s_exec.run_sleeve_cgroup(init, raw_args[2..]);
    }

    switch (try commands.dispatch_cli(init, arena)) {
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
            var process_vtable: std.Io.VTable = undefined;
            const io = child_process.io_for_serve(init, &process_vtable);
            const rt = try hub_rt.Runtime.open(init.gpa, io, init.environ_map, .{
                .home_override = s.home_opt,
                .listen_override = s.listen_opt,
                .proxy_listen_override = s.proxy_listen_opt,
                .trusted_proxies_override = s.trusted_proxies,
                .fleet_listen_override = s.fleet_opt,
                .dev_token_override = s.dev_token_opt,
                .no_sandbox = s.no_sandbox,
            });
            defer rt.close_and_destroy();

            const hs = try hub_http.HttpServer.start(init.gpa, io, rt, rt.http_listen);
            defer hs.stop_and_destroy();
            defer rt.fleet_srv.shutdown();

            const proxy_hs = if (rt.proxy_listen) |pl|
                try hub_http.HttpServer.start_with_options(init.gpa, io, rt, pl, rt.trusted_proxies)
            else
                null;
            defer if (proxy_hs) |ph| ph.stop_and_destroy();
            defer rt.fleet_srv.shutdown();

            var proxy_buf: [192]u8 = undefined;
            const proxy_str = if (proxy_hs) |ph|
                std.fmt.bufPrint(&proxy_buf, " proxy={s}:{d}", .{ ph.host, ph.listen_port() }) catch ""
            else
                "";

            try commands.print_out(
                init.io,
                "protectorate listening http={s}:{d}{s} fleet=0.0.0.0:{d} home={s}\n",
                .{ hs.host, hs.listen_port(), proxy_str, rt.fleet_srv.listen_port(), rt.hub_home.root_path },
            );

            while (!sig_stop.load(.acquire) and !rt.shutdown_requested.load(.acquire)) {
                init.io.sleep(.fromMilliseconds(200), .awake) catch break;
            }
            return 0;
        },
    }
}
