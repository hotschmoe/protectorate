const std = @import("std");
const tiger = @import("../tiger.zig");
const frame = @import("frame.zig");
const noise = @import("noise.zig");
const msg = @import("msg.zig");

pub const Chan = union(enum) {
    tcp: std.Io.net.Stream,
    stdio: struct {
        in: std.Io.File,
        out: std.Io.File,
    },
};

pub const TransportError = error{
    EndOfStream,
    RecordTooLarge,
    SessionExpired,
} || noise.NoiseError || frame.FrameError || std.Io.Writer.Error || std.Io.Reader.Error;

pub const Endpoint = struct {
    host: []const u8,
    port: u16,
};

pub const DialResult = struct {
    conn: NoiseConn,
    addr_index: usize,
};

pub const NoiseConn = struct {
    gpa: std.mem.Allocator,
    io: std.Io,
    chan: Chan,
    sess: noise.Session,
    dec: frame.Decoder = .{},
    tx_buf: [4096]u8 = undefined,

    pub fn dial_tcp(
        gpa: std.mem.Allocator,
        io: std.Io,
        host: []const u8,
        port: u16,
        my_static: noise.KeyPair,
        peer_static: [32]u8,
    ) !NoiseConn {
        return dial_tcp_timeout(gpa, io, host, port, 5_000, my_static, peer_static);
    }

    pub fn dial_tcp_timeout(
        gpa: std.mem.Allocator,
        io: std.Io,
        host: []const u8,
        port: u16,
        timeout_ms: i64,
        my_static: noise.KeyPair,
        peer_static: [32]u8,
    ) !NoiseConn {
        const res = try dial_any_tcp(gpa, io, &.{.{ .host = host, .port = port }}, timeout_ms, my_static, peer_static);
        return res.conn;
    }

    fn dial_tcp_blocking(
        gpa: std.mem.Allocator,
        io: std.Io,
        host: []const u8,
        port: u16,
        my_static: noise.KeyPair,
        peer_static: [32]u8,
    ) !NoiseConn {
        var stream = if (std.Io.net.IpAddress.parse(host, port)) |addr|
            try addr.connect(io, .{ .mode = .stream, .protocol = .tcp })
        else |_| blk: {
            const hn = try std.Io.net.HostName.init(host);
            break :blk try hn.connect(io, port, .{ .mode = .stream, .protocol = .tcp });
        };
        errdefer stream.close(io);
        return dial_on_chan(gpa, io, .{ .tcp = stream }, my_static, peer_static);
    }

    pub fn dial_any_tcp(
        gpa: std.mem.Allocator,
        io: std.Io,
        endpoints: []const Endpoint,
        timeout_ms: i64,
        my_static: noise.KeyPair,
        peer_static: [32]u8,
    ) !DialResult {
        if (endpoints.len == 0) return error.NoAddresses;
        if (endpoints.len == 1 and timeout_ms <= 0) {
            return .{
                .conn = try dial_tcp_blocking(gpa, io, endpoints[0].host, endpoints[0].port, my_static, peer_static),
                .addr_index = 0,
            };
        }

        const count = @min(endpoints.len, 16);
        const RaceState = struct {
            gpa: std.mem.Allocator,
            io: std.Io,
            my_static: noise.KeyPair,
            peer_static: [32]u8,
            hosts: [][]u8,
            ports: []u16,
            total: usize,
            mu: std.Io.Mutex = .init,
            cond: std.Io.Condition = .init,
            aborted: std.atomic.Value(bool) = .init(false),
            winner: ?DialResult = null,
            winner_taken: bool = false,
            finished: usize = 0,
            refs: std.atomic.Value(usize),

            fn create(
                r_gpa: std.mem.Allocator,
                r_io: std.Io,
                r_endpoints: []const Endpoint,
                r_my_static: noise.KeyPair,
                r_peer_static: [32]u8,
            ) !*@This() {
                const n = @min(r_endpoints.len, 16);
                const s = try r_gpa.create(@This());
                errdefer r_gpa.destroy(s);
                const h_buf = try r_gpa.alloc([]u8, n);
                var h_init: usize = 0;
                errdefer {
                    for (h_buf[0..h_init]) |h| r_gpa.free(h);
                    r_gpa.free(h_buf);
                }
                const p_buf = try r_gpa.alloc(u16, n);
                errdefer r_gpa.free(p_buf);
                for (r_endpoints[0..n], 0..) |ep, i| {
                    h_buf[i] = try r_gpa.dupe(u8, ep.host);
                    h_init += 1;
                    p_buf[i] = ep.port;
                }
                s.* = .{
                    .gpa = r_gpa,
                    .io = r_io,
                    .my_static = r_my_static,
                    .peer_static = r_peer_static,
                    .hosts = h_buf,
                    .ports = p_buf,
                    .total = n,
                    .refs = .init(1),
                };
                return s;
            }

            fn release(self: *@This()) void {
                if (self.refs.fetchSub(1, .acq_rel) == 1) {
                    if (self.winner) |*w| {
                        w.conn.deinit();
                    }
                    for (self.hosts) |h| self.gpa.free(h);
                    self.gpa.free(self.hosts);
                    self.gpa.free(self.ports);
                    self.gpa.destroy(self);
                }
            }
        };

        const st = try RaceState.create(gpa, io, endpoints, my_static, peer_static);
        defer st.release();

        const WorkerArg = struct {
            state: *RaceState,
            idx: usize,
        };
        const worker_fn = struct {
            fn run(warg: WorkerArg) void {
                const rs = warg.state;
                defer rs.release();

                const delay_ms = warg.idx * 150;
                var slept: usize = 0;
                while (slept < delay_ms and !rs.aborted.load(.acquire)) : (slept += 25) {
                    rs.io.sleep(.fromMilliseconds(25), .awake) catch break;
                }

                if (!rs.aborted.load(.acquire)) {
                    if (dial_tcp_blocking(rs.gpa, rs.io, rs.hosts[warg.idx], rs.ports[warg.idx], rs.my_static, rs.peer_static)) |c| {
                        var conn = c;
                        rs.mu.lockUncancelable(rs.io);
                        if (!rs.winner_taken and rs.winner == null) {
                            rs.winner = .{ .conn = conn, .addr_index = warg.idx };
                            rs.aborted.store(true, .release);
                            rs.finished += 1;
                            rs.cond.broadcast(rs.io);
                            rs.mu.unlock(rs.io);
                            return;
                        }
                        rs.finished += 1;
                        rs.cond.broadcast(rs.io);
                        rs.mu.unlock(rs.io);
                        conn.deinit();
                        return;
                    } else |_| {}
                }

                rs.mu.lockUncancelable(rs.io);
                rs.finished += 1;
                rs.cond.broadcast(rs.io);
                rs.mu.unlock(rs.io);
            }
        }.run;

        var spawned: usize = 0;
        for (0..count) |i| {
            _ = st.refs.fetchAdd(1, .acq_rel);
            const t = std.Thread.spawn(.{}, worker_fn, .{WorkerArg{ .state = st, .idx = i }}) catch {
                _ = st.refs.fetchSub(1, .acq_rel);
                continue;
            };
            t.detach();
            spawned += 1;
        }
        if (spawned == 0) return error.ConnectFailed;

        const deadline_ms = if (timeout_ms > 0)
            std.Io.Clock.awake.now(io).toMilliseconds() + timeout_ms
        else
            std.math.maxInt(i64);

        st.mu.lockUncancelable(io);
        while (st.winner == null and st.finished < spawned) {
            const now_ms = std.Io.Clock.awake.now(io).toMilliseconds();
            const rem_ms = deadline_ms - now_ms;
            if (rem_ms <= 0) break;
            st.cond.waitTimeout(io, &st.mu, .{
                .duration = .{ .raw = .fromMilliseconds(rem_ms), .clock = .awake },
            }) catch {};
        }
        const won = st.winner;
        st.winner = null;
        st.winner_taken = true;
        st.aborted.store(true, .release);
        st.mu.unlock(io);

        return won orelse error.ConnectFailed;
    }

    pub fn dial_stdio(
        gpa: std.mem.Allocator,
        io: std.Io,
        in_file: std.Io.File,
        out_file: std.Io.File,
        my_static: noise.KeyPair,
        peer_static: [32]u8,
    ) !NoiseConn {
        return dial_on_chan(gpa, io, .{ .stdio = .{ .in = in_file, .out = out_file } }, my_static, peer_static);
    }

    pub fn accept_stdio(
        gpa: std.mem.Allocator,
        io: std.Io,
        in_file: std.Io.File,
        out_file: std.Io.File,
        my_static: noise.KeyPair,
        allowed_peers: []const [32]u8,
    ) !NoiseConn {
        return accept_on_chan(gpa, io, .{ .stdio = .{ .in = in_file, .out = out_file } }, my_static, allowed_peers);
    }

    pub fn dial_on_chan(
        gpa: std.mem.Allocator,
        io: std.Io,
        chan: Chan,
        my_static: noise.KeyPair,
        peer_static: [32]u8,
    ) !NoiseConn {
        const eph = noise.KeyPair.generate(io);
        var msg1: [noise.msg1_len]u8 = undefined;
        var hs = try noise.InitiatorHandshake.start(my_static, eph, peer_static, &msg1);

        var conn = NoiseConn{
            .gpa = gpa,
            .io = io,
            .chan = chan,
            .sess = undefined,
        };
        try conn.write_all(&msg1);

        var msg2: [noise.msg2_len]u8 = undefined;
        try conn.read_exact(&msg2);

        const now_ms: u64 = @intCast(@max(0, std.Io.Clock.real.now(io).toMilliseconds()));
        conn.sess = try hs.finish(&msg2, now_ms);
        return conn;
    }

    pub fn accept_on_chan(
        gpa: std.mem.Allocator,
        io: std.Io,
        chan: Chan,
        my_static: noise.KeyPair,
        allowed_peers: []const [32]u8,
    ) !NoiseConn {
        var conn = NoiseConn{
            .gpa = gpa,
            .io = io,
            .chan = chan,
            .sess = undefined,
        };
        var msg1: [noise.msg1_len]u8 = undefined;
        try conn.read_exact(&msg1);

        const eph = noise.KeyPair.generate(io);
        var msg2: [noise.msg2_len]u8 = undefined;
        const now_ms: u64 = @intCast(@max(0, std.Io.Clock.real.now(io).toMilliseconds()));
        conn.sess = try noise.responder_accept_any(my_static, eph, allowed_peers, &msg1, &msg2, now_ms);

        try conn.write_all(&msg2);
        return conn;
    }

    pub fn deinit(self: *NoiseConn) void {
        self.sess.deinit();
        self.dec.deinit(self.gpa);
        switch (self.chan) {
            .tcp => |s| s.close(self.io),
            .stdio => {},
        }
    }

    pub fn shutdown(self: *NoiseConn) void {
        switch (self.chan) {
            .tcp => |s| s.shutdown(self.io, .both) catch {},
            .stdio => {},
        }
    }

    fn write_all(self: *NoiseConn, bytes: []const u8) !void {
        switch (self.chan) {
            .tcp => |s| {
                var w = s.writer(self.io, &self.tx_buf);
                try w.interface.writeAll(bytes);
                try w.interface.flush();
            },
            .stdio => |io_files| {
                var w = io_files.out.writer(self.io, &self.tx_buf);
                try w.interface.writeAll(bytes);
                try w.interface.flush();
            },
        }
    }

    fn read_exact(self: *NoiseConn, dst: []u8) !void {
        switch (self.chan) {
            .tcp => |s| {
                var r = s.reader(self.io, &.{});
                try r.interface.readSliceAll(dst);
            },
            .stdio => |io_files| {
                var r = io_files.in.reader(self.io, &.{});
                try r.interface.readSliceAll(dst);
            },
        }
    }

    pub fn send_frame(self: *NoiseConn, kind: frame.Kind, id: u64, payload: []const u8) !void {
        const now_ms: u64 = @intCast(@max(0, std.Io.Clock.real.now(self.io).toMilliseconds()));
        if (self.sess.expired(now_ms)) return error.SessionExpired;

        const encoded = try frame.encode_alloc(self.gpa, kind, id, payload);
        defer self.gpa.free(encoded);

        var rec_buf: [noise.max_record_wire]u8 = undefined;
        var off: usize = 0;
        while (off < encoded.len) {
            const n = @min(noise.max_record_plaintext, encoded.len - off);
            const sealed = try self.sess.seal_record(encoded[off .. off + n], &rec_buf);
            try self.write_all(sealed);
            off += n;
        }
    }

    pub fn send_json(self: *NoiseConn, kind: frame.Kind, id: u64, value: anytype) !void {
        const bytes = try msg.encode_alloc(self.gpa, value);
        defer self.gpa.free(bytes);
        try self.send_frame(kind, id, bytes);
    }

    pub fn recv_frame(self: *NoiseConn) !frame.Frame {
        var cipher_buf: [noise.max_record_plaintext + noise.tag_len]u8 = undefined;
        var plain_buf: [noise.max_record_plaintext]u8 = undefined;

        while (true) {
            if (try self.dec.next_frame(self.gpa)) |f| return f;

            const now_ms: u64 = @intCast(@max(0, std.Io.Clock.real.now(self.io).toMilliseconds()));
            if (self.sess.expired(now_ms)) return error.SessionExpired;

            var len_hdr: [2]u8 = undefined;
            try self.read_exact(&len_hdr);
            const rlen: usize = std.mem.readInt(u16, &len_hdr, .big);
            if (rlen <= noise.tag_len or rlen > noise.max_record_plaintext + noise.tag_len) {
                return error.RecordTooLarge;
            }
            try self.read_exact(cipher_buf[0..rlen]);
            const plain = try self.sess.open_record(cipher_buf[0..rlen], &plain_buf);
            try self.dec.feed(self.gpa, plain);
        }
    }
};
