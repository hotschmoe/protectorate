const std = @import("std");
const tiger = @import("../tiger.zig");
const version = @import("../version.zig");
const schema = @import("../api/schema.zig");
const web_assets = @import("web_assets");
const home = @import("home.zig");
const runtime = @import("runtime.zig");
const api_dispatch = @import("api_dispatch.zig");
const limits = @import("../net/limits.zig");

const busy_body = "server busy, retry shortly\n";
const busy_response = std.fmt.comptimePrint(
    "HTTP/1.1 503 Service Unavailable\r\n" ++
        "connection: close\r\n" ++
        "retry-after: 1\r\n" ++
        "access-control-allow-origin: *\r\n" ++
        "content-type: text/plain\r\n" ++
        "content-length: {d}\r\n\r\n{s}",
    .{ busy_body.len, busy_body },
);

pub const HttpServer = struct {
    gpa: std.mem.Allocator,
    io: std.Io,
    rt: *runtime.Runtime,
    host: []const u8,
    port: u16,
    trusted_proxies: []std.Io.net.IpAddress = &.{},
    actual_port: std.atomic.Value(u16) = .init(0),
    stopping: std.atomic.Value(bool) = .init(false),
    srv: ?std.Io.net.Server = null,
    accept_thread: ?std.Thread = null,
    conns_mu: std.Io.Mutex = .init,
    conns_cond: std.Io.Condition = .init,
    conns: ?*ConnArg = null,
    gate: limits.ConnGate = .init(limits.http_conns_max),

    pub fn start(gpa: std.mem.Allocator, io: std.Io, rt: *runtime.Runtime, listen_addr: []const u8) !*HttpServer {
        return start_with_options(gpa, io, rt, listen_addr, &.{});
    }

    pub fn start_with_options(
        gpa: std.mem.Allocator,
        io: std.Io,
        rt: *runtime.Runtime,
        listen_addr: []const u8,
        trusted_proxy_strs: []const []const u8,
    ) !*HttpServer {
        const hp = parse_host_port(listen_addr, 7470);
        const hs = try gpa.create(HttpServer);
        errdefer gpa.destroy(hs);

        const proxies = try gpa.alloc(std.Io.net.IpAddress, trusted_proxy_strs.len);
        errdefer gpa.free(proxies);
        for (trusted_proxy_strs, 0..) |ip_str, idx| {
            proxies[idx] = try std.Io.net.IpAddress.parse(ip_str, 0);
        }

        hs.* = .{
            .gpa = gpa,
            .io = io,
            .rt = rt,
            .host = try gpa.dupe(u8, hp.host),
            .port = hp.port,
            .trusted_proxies = proxies,
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
        self.rt.stream_hub.wake_all();
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
        self.conns_mu.lockUncancelable(self.io);
        var current = self.conns;
        while (current) |conn| : (current = conn.next) {
            conn.stream.shutdown(self.io, .both) catch {};
        }
        while (self.conns != null) self.conns_cond.waitUncancelable(self.io, &self.conns_mu);
        self.conns_mu.unlock(self.io);
        self.gpa.free(self.trusted_proxies);
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
            if (self.trusted_proxies.len > 0 and !ip_matches_any(stream.socket.address, self.trusted_proxies)) {
                stream.close(self.io);
                continue;
            }
            switch (self.gate.try_enter()) {
                .admitted => {},
                .refused_first => {
                    std.debug.print("http: {d} connections open, answering 503 to new ones\n", .{self.gate.max});
                    self.refuse_busy(stream);
                    continue;
                },
                .refused => {
                    self.refuse_busy(stream);
                    continue;
                },
            }
            const arg = self.gpa.create(ConnArg) catch {
                self.gate.leave();
                stream.close(self.io);
                continue;
            };
            arg.* = .{ .hs = self, .stream = stream };
            self.conns_mu.lockUncancelable(self.io);
            arg.next = self.conns;
            self.conns = arg;
            self.conns_mu.unlock(self.io);
            const t = std.Thread.spawn(.{}, conn_thread, .{arg}) catch {
                arg.finish();
                continue;
            };
            t.detach();
        }
    }

    fn refuse_busy(self: *HttpServer, stream: std.Io.net.Stream) void {
        var wbuf: [busy_response.len]u8 = undefined;
        var w = stream.writer(self.io, &wbuf);
        w.interface.writeAll(busy_response) catch {};
        w.interface.flush() catch {};
        stream.shutdown(self.io, .send) catch {};
        stream.close(self.io);
    }
};

const ConnArg = struct {
    hs: *HttpServer,
    stream: std.Io.net.Stream,
    next: ?*ConnArg = null,

    fn finish(self: *ConnArg) void {
        const hs = self.hs;
        hs.conns_mu.lockUncancelable(hs.io);
        var link = &hs.conns;
        while (link.* != self) link = &link.*.?.next;
        link.* = self.next;
        self.stream.close(hs.io);
        hs.gate.leave();
        hs.gpa.destroy(self);
        hs.conns_cond.broadcast(hs.io);
        hs.conns_mu.unlock(hs.io);
    }
};

const cors_headers = [_]std.http.Header{
    .{ .name = "access-control-allow-origin", .value = "*" },
    .{ .name = "access-control-allow-methods", .value = "GET, POST, OPTIONS" },
    .{ .name = "access-control-allow-headers", .value = "Authorization, Content-Type" },
};

fn conn_thread(arg: *ConnArg) void {
    const hs = arg.hs;
    const stream = arg.stream;
    defer arg.finish();

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
        var accept_gzip = false;
        var hit = req.iterateHeaders();
        while (hit.next()) |h| {
            if (std.ascii.eqlIgnoreCase(h.name, "authorization")) {
                if (std.mem.startsWith(u8, h.value, "Bearer ") or std.mem.startsWith(u8, h.value, "bearer ")) {
                    auth_token = arena.dupe(u8, std.mem.trim(u8, h.value[7..], " \t")) catch null;
                }
            } else if (std.ascii.eqlIgnoreCase(h.name, "accept-encoding")) {
                if (contains_ci(h.value, "gzip")) accept_gzip = true;
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
                handle_websocket(hs, &req, k) catch {};
                return;
            }
            req.respond("upgrade required", .{ .status = .bad_request }) catch break;
            continue;
        }

        if (method == .GET and std.mem.startsWith(u8, path, "/api/client/")) {
            const principal = if (auth_token) |t| hs.rt.hub_home.authorize(t) else null;
            if (principal == null) {
                req.respond("{\"error\":{\"code\":\"unauthenticated\",\"message\":\"unauthorized\"}}", .{
                    .status = .unauthorized,
                    .extra_headers = &.{
                        .{ .name = "content-type", .value = "application/json" },
                        .{ .name = "access-control-allow-origin", .value = "*" },
                    },
                }) catch break;
                continue;
            }
            const client_name = path["/api/client/".len..];
            const asset_path = (hs.rt.resolve_client_asset_path(arena, client_name) catch null) orelse {
                req.respond("{\"error\":{\"code\":\"not_found\",\"message\":\"client asset not found\"}}", .{
                    .status = .not_found,
                    .extra_headers = &.{
                        .{ .name = "content-type", .value = "application/json" },
                        .{ .name = "access-control-allow-origin", .value = "*" },
                    },
                }) catch break;
                continue;
            };
            var cf = std.Io.Dir.cwd().openFile(hs.io, asset_path, .{}) catch {
                req.respond("{\"error\":{\"code\":\"not_found\",\"message\":\"client asset not found\"}}", .{
                    .status = .not_found,
                    .extra_headers = &.{
                        .{ .name = "content-type", .value = "application/json" },
                        .{ .name = "access-control-allow-origin", .value = "*" },
                    },
                }) catch break;
                continue;
            };
            defer cf.close(hs.io);
            const st = cf.stat(hs.io) catch break;
            const ct: []const u8 = if (std.mem.endsWith(u8, client_name, ".apk"))
                "application/vnd.android.package-archive"
            else if (std.mem.endsWith(u8, client_name, ".zip"))
                "application/zip"
            else
                "application/octet-stream";

            var sbuf: [32 * 1024]u8 = undefined;
            var bw = req.respondStreaming(&sbuf, .{
                .content_length = st.size,
                .respond_options = .{
                    .status = .ok,
                    .extra_headers = &.{
                        .{ .name = "content-type", .value = ct },
                        .{ .name = "access-control-allow-origin", .value = "*" },
                    },
                },
            }) catch break;

            var fbuf: [32 * 1024]u8 = undefined;
            var off: u64 = 0;
            while (off < st.size) {
                const want: usize = @intCast(@min(@as(u64, fbuf.len), st.size - off));
                const got = cf.readPositionalAll(hs.io, fbuf[0..want], off) catch break;
                if (got == 0) break;
                bw.writer.writeAll(fbuf[0..got]) catch break;
                off += got;
            }
            if (off != st.size) break;
            bw.end() catch break;
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
            const status_e: std.http.Status = @fromBackingInt(@intCast(res.status));
            req.respond(res.body_json, .{
                .status = status_e,
                .extra_headers = &.{
                    .{ .name = "content-type", .value = "application/json" },
                    .{ .name = "access-control-allow-origin", .value = "*" },
                },
            }) catch break;
            continue;
        }

        if (std.mem.startsWith(u8, path, "/api/")) {
            req.respond("{\"error\":{\"code\":\"not_found\",\"message\":\"not found\"}}", .{
                .status = .not_found,
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
                if (!a.gzipped) {
                    req.respond(a.bytes, .{
                        .status = .ok,
                        .extra_headers = &.{
                            .{ .name = "content-type", .value = a.content_type },
                            .{ .name = "access-control-allow-origin", .value = "*" },
                        },
                    }) catch break;
                } else if (accept_gzip) {
                    req.respond(a.bytes, .{
                        .status = .ok,
                        .extra_headers = &.{
                            .{ .name = "content-type", .value = a.content_type },
                            .{ .name = "content-encoding", .value = "gzip" },
                            .{ .name = "vary", .value = "Accept-Encoding" },
                            .{ .name = "access-control-allow-origin", .value = "*" },
                        },
                    }) catch break;
                } else {
                    var in_r: std.Io.Reader = .fixed(a.bytes);
                    var win_buf: [std.compress.flate.max_window_len]u8 = undefined;
                    var dec: std.compress.flate.Decompress = .init(&in_r, .gzip, &win_buf);
                    const plain = dec.reader.allocRemaining(arena, .limited(64 << 20)) catch break;
                    req.respond(plain, .{
                        .status = .ok,
                        .extra_headers = &.{
                            .{ .name = "content-type", .value = a.content_type },
                            .{ .name = "vary", .value = "Accept-Encoding" },
                            .{ .name = "access-control-allow-origin", .value = "*" },
                        },
                    }) catch break;
                }
                continue;
            }
        }

        req.respond("not found", .{ .status = .not_found }) catch break;
    }
}

fn normalize_ip4(addr: std.Io.net.IpAddress) ?[4]u8 {
    return switch (addr) {
        .ip4 => |v4| v4.bytes,
        .ip6 => |v6| blk: {
            for (v6.bytes[0..10]) |b| {
                if (b != 0) break :blk null;
            }
            if (v6.bytes[10] != 0xff or v6.bytes[11] != 0xff) break :blk null;
            break :blk v6.bytes[12..16].*;
        },
    };
}

fn ip_matches_any(peer: std.Io.net.IpAddress, allowed: []const std.Io.net.IpAddress) bool {
    const p4 = normalize_ip4(peer);
    for (allowed) |cand| {
        if (p4) |pv4| {
            if (normalize_ip4(cand)) |cv4| {
                if (std.mem.eql(u8, &pv4, &cv4)) return true;
                continue;
            }
        }
        if (peer == .ip6 and cand == .ip6) {
            if (std.mem.eql(u8, &peer.ip6.bytes, &cand.ip6.bytes)) return true;
        }
    }
    return false;
}

fn contains_ci(hay: []const u8, needle: []const u8) bool {
    if (needle.len == 0) return true;
    if (needle.len > hay.len) return false;
    var i: usize = 0;
    while (i + needle.len <= hay.len) : (i += 1) {
        if (std.ascii.eqlIgnoreCase(hay[i .. i + needle.len], needle)) return true;
    }
    return false;
}

fn handle_websocket(
    hs: *HttpServer,
    req: *std.http.Server.Request,
    key: []const u8,
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

    var cursor: u64 = state_snap.seq;
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
