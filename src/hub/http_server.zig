const std = @import("std");
const tiger = @import("../tiger.zig");
const version = @import("../version.zig");
const schema = @import("../api/schema.zig");
const web_assets = @import("web_assets");
const home = @import("home.zig");
const runtime = @import("runtime.zig");
const api_dispatch = @import("api_dispatch.zig");

pub const HttpServer = struct {
    gpa: std.mem.Allocator,
    io: std.Io,
    rt: *runtime.Runtime,
    host: []const u8,
    port: u16,
    actual_port: std.atomic.Value(u16) = .init(0),
    stopping: std.atomic.Value(bool) = .init(false),
    srv: ?std.Io.net.Server = null,
    accept_thread: ?std.Thread = null,

    pub fn start(gpa: std.mem.Allocator, io: std.Io, rt: *runtime.Runtime, listen_addr: []const u8) !*HttpServer {
        const hp = parse_host_port(listen_addr, 7470);
        const hs = try gpa.create(HttpServer);
        errdefer gpa.destroy(hs);

        hs.* = .{
            .gpa = gpa,
            .io = io,
            .rt = rt,
            .host = try gpa.dupe(u8, hp.host),
            .port = hp.port,
        };
        errdefer gpa.free(hs.host);

        const addr = try std.Io.net.IpAddress.parse(hs.host, hs.port);
        var srv = try addr.listen(io, .{ .reuse_address = true });
        errdefer srv.deinit(io);
        hs.actual_port.store(srv.socket.address.getPort(), .release);
        hs.srv = srv;
        hs.accept_thread = try std.Thread.spawn(.{}, accept_loop, .{hs});
        return hs;
    }

    pub fn listen_port(self: *const HttpServer) u16 {
        return self.actual_port.load(.acquire);
    }

    pub fn stop_and_destroy(self: *HttpServer) void {
        self.stopping.store(true, .release);
        const port = self.actual_port.load(.acquire);
        if (port != 0) {
            const wake_host = if (std.mem.eql(u8, self.host, "0.0.0.0"))
                "127.0.0.1"
            else if (std.mem.eql(u8, self.host, "::"))
                "::1"
            else
                self.host;
            if (std.Io.net.IpAddress.parse(wake_host, port)) |a| {
                if (a.connect(self.io, .{ .mode = .stream, .protocol = .tcp })) |st| st.close(self.io) else |_| {}
            } else |_| {}
        }
        if (self.accept_thread) |t| t.join();
        if (self.srv) |*s| {
            s.deinit(self.io);
            self.srv = null;
        }
        self.gpa.free(self.host);
        self.gpa.destroy(self);
    }

    fn accept_loop(self: *HttpServer) void {
        while (!self.stopping.load(.acquire)) {
            const s = &(self.srv orelse break);
            const stream = s.accept(self.io) catch {
                if (self.stopping.load(.acquire)) break;
                continue;
            };
            if (self.stopping.load(.acquire)) {
                stream.close(self.io);
                break;
            }
            const arg = self.gpa.create(ConnArg) catch {
                stream.close(self.io);
                continue;
            };
            arg.* = .{ .hs = self, .stream = stream };
            const t = std.Thread.spawn(.{}, conn_thread, .{arg}) catch {
                self.gpa.destroy(arg);
                stream.close(self.io);
                continue;
            };
            t.detach();
        }
    }
};

const ConnArg = struct {
    hs: *HttpServer,
    stream: std.Io.net.Stream,
};

const cors_headers = [_]std.http.Header{
    .{ .name = "access-control-allow-origin", .value = "*" },
    .{ .name = "access-control-allow-methods", .value = "GET, POST, OPTIONS" },
    .{ .name = "access-control-allow-headers", .value = "Authorization, Content-Type" },
};

fn conn_thread(arg: *ConnArg) void {
    const hs = arg.hs;
    const stream = arg.stream;
    hs.gpa.destroy(arg);
    defer stream.close(hs.io);

    var rbuf: [32 * 1024]u8 = undefined;
    var wbuf: [32 * 1024]u8 = undefined;
    var net_r = stream.reader(hs.io, &rbuf);
    var net_w = stream.writer(hs.io, &wbuf);
    var server = std.http.Server.init(&net_r.interface, &net_w.interface);

    var req_arena_st = std.heap.ArenaAllocator.init(hs.gpa);
    defer req_arena_st.deinit();

    while (!hs.stopping.load(.acquire)) {
        _ = req_arena_st.reset(.retain_capacity);
        const arena = req_arena_st.allocator();

        var req = server.receiveHead() catch break;

        const method = req.head.method;
        const target = arena.dupe(u8, req.head.target) catch break;

        var auth_token: ?[]const u8 = null;
        var hit = req.iterateHeaders();
        while (hit.next()) |h| {
            if (std.ascii.eqlIgnoreCase(h.name, "authorization")) {
                if (std.mem.startsWith(u8, h.value, "Bearer ") or std.mem.startsWith(u8, h.value, "bearer ")) {
                    auth_token = arena.dupe(u8, std.mem.trim(u8, h.value[7..], " \t")) catch null;
                }
            }
        }
        const qmark = std.mem.findScalar(u8, target, '?');
        const path = if (qmark) |qi| target[0..qi] else target;
        const query = if (qmark) |qi| target[qi + 1 ..] else "";
        if (auth_token == null and query.len > 0) {
            auth_token = query_param(arena, query, "access_token") orelse query_param(arena, query, "token");
        }

        const upg = req.upgradeRequested();

        if (method == .OPTIONS) {
            req.respond("", .{
                .status = .no_content,
                .extra_headers = &cors_headers,
            }) catch break;
            continue;
        }

        if (method == .GET and std.mem.eql(u8, path, "/health")) {
            const body = std.fmt.allocPrint(arena, "{{\"ok\":true,\"version\":\"{s}\"}}", .{version.version}) catch break;
            req.respond(body, .{
                .status = .ok,
                .extra_headers = &.{
                    .{ .name = "content-type", .value = "application/json" },
                    .{ .name = "access-control-allow-origin", .value = "*" },
                },
            }) catch break;
            continue;
        }

        if (method == .GET and std.mem.eql(u8, path, "/api/events")) {
            const principal = if (auth_token) |t| hs.rt.hub_home.authorize(t) else null;
            if (principal == null) {
                req.respond("{\"error\":{\"code\":\"unauthenticated\",\"message\":\"unauthorized\"}}", .{
                    .status = .unauthorized,
                    .extra_headers = &.{.{ .name = "content-type", .value = "application/json" }},
                }) catch break;
                continue;
            }
            const ws_key = switch (upg) {
                .websocket => |k| k,
                else => null,
            };
            if (ws_key) |k| {
                const from_seq = if (query_param(arena, query, "from_seq")) |fs| std.fmt.parseInt(u64, fs, 10) catch null else null;
                handle_websocket(hs, &req, k, from_seq) catch {};
                return;
            }
            req.respond("upgrade required", .{ .status = .bad_request }) catch break;
            continue;
        }

        if (method == .POST and std.mem.startsWith(u8, path, "/api/")) {
            const rpc_name = path["/api/".len..];
            var body_buf: [16 * 1024]u8 = undefined;
            const body_reader = req.readerExpectContinue(&body_buf) catch break;
            const req_body = body_reader.allocRemaining(arena, .limited(4 << 20)) catch break;

            const principal = if (auth_token) |t| hs.rt.hub_home.authorize(t) else null;
            const res = api_dispatch.dispatch(hs.rt, principal, rpc_name, req_body) catch {
                req.respond("{\"error\":{\"code\":\"internal\",\"message\":\"dispatch failed\"}}", .{
                    .status = .internal_server_error,
                    .extra_headers = &.{
                        .{ .name = "content-type", .value = "application/json" },
                        .{ .name = "access-control-allow-origin", .value = "*" },
                    },
                }) catch break;
                continue;
            };
            defer hs.gpa.free(res.body_json);
            const status_e: std.http.Status = @enumFromInt(res.status);
            req.respond(res.body_json, .{
                .status = status_e,
                .extra_headers = &.{
                    .{ .name = "content-type", .value = "application/json" },
                    .{ .name = "access-control-allow-origin", .value = "*" },
                },
            }) catch break;
            continue;
        }

        if (method == .GET) {
            const asset = web_assets.find(path) orelse web_assets.find("index.html");
            if (asset) |a| {
                req.respond(a.bytes, .{
                    .status = .ok,
                    .extra_headers = &.{
                        .{ .name = "content-type", .value = a.content_type },
                        .{ .name = "access-control-allow-origin", .value = "*" },
                    },
                }) catch break;
                continue;
            }
        }

        req.respond("not found", .{ .status = .not_found }) catch break;
    }
}

fn handle_websocket(
    hs: *HttpServer,
    req: *std.http.Server.Request,
    key: []const u8,
    from_seq_opt: ?u64,
) !void {
    var ws = try req.respondWebSocket(.{ .key = key });
    try ws.flush();

    var ws_arena_st = std.heap.ArenaAllocator.init(hs.gpa);
    defer ws_arena_st.deinit();
    const wa = ws_arena_st.allocator();

    const state_snap = try hs.rt.view_state(wa);
    const snap_json = try schema.encode_alloc(hs.gpa, schema.HubStreamMessage{ .snapshot = .{
        .seq = state_snap.seq,
        .state = state_snap,
    } });
    defer hs.gpa.free(snap_json);
    try ws.writeMessage(snap_json, .text);

    const live_json = try schema.encode_alloc(hs.gpa, schema.HubStreamMessage{ .live = .{
        .seq = state_snap.seq,
    } });
    defer hs.gpa.free(live_json);
    try ws.writeMessage(live_json, .text);

    var cursor: u64 = from_seq_opt orelse state_snap.seq;
    while (!hs.stopping.load(.acquire) and !hs.rt.stopping.load(.acquire)) {
        _ = ws_arena_st.reset(.retain_capacity);
        const batch = try hs.rt.stream_hub.wait_since(ws_arena_st.allocator(), cursor, 15_000);
        if (batch.closed) break;
        if (batch.msgs.len == 0) {
            ws.writeMessage("", .ping) catch break;
            continue;
        }
        for (batch.msgs) |msg_json| {
            ws.writeMessage(msg_json, .text) catch return;
        }
        cursor = batch.last_seq;
    }
}

fn query_param(arena: std.mem.Allocator, query: []const u8, key: []const u8) ?[]const u8 {
    var it = std.mem.splitScalar(u8, query, '&');
    while (it.next()) |kv| {
        if (std.mem.findScalar(u8, kv, '=')) |eq| {
            if (std.mem.eql(u8, kv[0..eq], key)) {
                return arena.dupe(u8, kv[eq + 1 ..]) catch null;
            }
        }
    }
    return null;
}

fn parse_host_port(raw: []const u8, def_port: u16) struct { host: []const u8, port: u16 } {
    if (std.mem.findScalarLast(u8, raw, ':')) |colon| {
        if (std.fmt.parseInt(u16, raw[colon + 1 ..], 10)) |p| {
            return .{ .host = raw[0..colon], .port = p };
        } else |_| {}
    }
    return .{ .host = raw, .port = def_port };
}
