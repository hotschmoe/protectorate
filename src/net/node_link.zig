const std = @import("std");
const tiger = @import("../tiger.zig");
const io = @import("io.zig");
const noise = @import("noise.zig");
const frame = @import("frame.zig");
const bounded = @import("bounded.zig");
const channel = @import("channel.zig");
const handshake = @import("handshake.zig");

pub const candidate_limit = 16;
pub const Event = union(enum) {
    connected: u64,
    disconnected: u64,
    request: struct { generation: u64, id: u64, size: usize, serial: u64 },
};
pub const Hooks = struct {
    context: *anyopaque,
    hello_reply: *const fn (*anyopaque, []const u8) bool,
};

pub fn NodeLink(comptime IO: type) type {
    io.assert_contract(IO);
    return struct {
        const Self = @This();
        const Candidate = struct {
            state: enum { idle, connecting, handshake, winner, closing, draining, finished } = .idle,
            connect_op: io.Operation = undefined,
            connect_result: ?io.Result = null,
            close_op: io.Operation = undefined,
            close_result: ?io.Result = null,
            socket: io.Socket = undefined,
            hs: handshake.Handshake(IO) = undefined,
            has_hs: bool = false,
            deadline: u64 = 0,
        };
        const Phase = enum { waiting, racing, hello, established, closing, stopped };
        backend: *IO,
        pool: *bounded.SlabPool,
        endpoints: []const io.Endpoint,
        key: noise.KeyPair,
        hub_key: [32]u8,
        hello: []const u8,
        hooks: Hooks,
        candidates: [candidate_limit]Candidate = @splat(.{}),
        conn: channel.Channel(IO) = undefined,
        has_conn: bool = false,
        winner: ?usize = null,
        phase: Phase = .waiting,
        race_start: u64 = 0,
        next_race: u64 = 0,
        backoff: u64 = 500,
        last_rx: u64 = 0,
        rx_serial: u64 = 0,
        heartbeat_at: u64 = 0,
        next_id: u64 = 4,
        generation: u64 = 0,
        connected_event: bool = false,
        disconnected_event: bool = false,
        stopping: bool = false,
        scratch: [frame.max_payload]u8 = undefined,

        pub fn init(self: *Self, backend: *IO, pool: *bounded.SlabPool, endpoints: []const io.Endpoint, key: noise.KeyPair, hub_key: [32]u8, hello: []const u8, hooks: Hooks) !void {
            if (endpoints.len == 0 or endpoints.len > candidate_limit) return error.BadEndpoints;
            tiger.assert(@sizeOf(Self) <= 2 * 1024 * 1024);
            self.* = .{ .backend = backend, .pool = pool, .endpoints = endpoints, .key = key, .hub_key = hub_key, .hello = hello, .hooks = hooks };
            for (&self.candidates) |*candidate| {
                candidate.connect_op = .{ .context = candidate, .callback = connect_complete };
                candidate.close_op = .{ .context = candidate, .callback = close_complete };
            }
        }
        fn connect_complete(ctx: *anyopaque, _: *io.Operation, result: io.Result) void {
            const candidate: *Candidate = @ptrCast(@alignCast(ctx));
            tiger.assert(candidate.connect_result == null);
            candidate.connect_result = result;
        }
        fn close_complete(ctx: *anyopaque, _: *io.Operation, result: io.Result) void {
            const candidate: *Candidate = @ptrCast(@alignCast(ctx));
            tiger.assert(candidate.close_result == null);
            candidate.close_result = result;
        }
        fn cancel_candidate(self: *Self, candidate: *Candidate) void {
            switch (candidate.state) {
                .idle => candidate.state = .finished,
                .connecting => if (candidate.connect_op.state == .submitted) {
                    self.backend.cancel(&candidate.connect_op);
                },
                .handshake => {
                    candidate.hs.stop(self.backend);
                    candidate.state = .closing;
                },
                .winner => if (self.has_conn) {
                    self.conn.stop(self.backend);
                    candidate.state = .closing;
                },
                else => {},
            }
        }
        fn close_link(self: *Self) void {
            if (self.phase == .closing or self.phase == .stopped) return;
            if (self.phase == .established) self.disconnected_event = true;
            self.connected_event = false;
            self.phase = .closing;
            for (&self.candidates) |*candidate| self.cancel_candidate(candidate);
        }
        pub fn shutdown(self: *Self) void {
            self.stopping = true;
            self.close_link();
        }
        pub fn done(self: *const Self) bool {
            return self.phase == .stopped;
        }
        pub fn deinit(self: *Self) void {
            tiger.assert(self.done());
            self.key.zeroize();
        }
        pub fn set_hello(self: *Self, hello: []const u8) void {
            self.hello = hello;
        }
        pub fn event(self: *Self) ?Event {
            if (self.connected_event) return .{ .connected = self.generation };
            if (self.disconnected_event) return .{ .disconnected = self.generation };
            if (self.phase == .established and self.conn.frame_ready) {
                const hdr = self.conn.header(self.pool) catch unreachable;
                if (hdr.kind == .request) return .{ .request = .{ .generation = self.generation, .id = hdr.id, .size = hdr.payload_len, .serial = self.conn.rx_serial } };
            }
            return null;
        }
        pub fn read(self: *Self, generation: u64, serial: u64, offset: usize, out: []u8) !void {
            if (self.phase != .established or self.generation != generation or !self.conn.frame_ready or self.conn.rx_serial != serial) return error.NoFrame;
            const hdr = try self.conn.header(self.pool);
            if (offset > hdr.payload_len or out.len > hdr.payload_len - offset) return error.BadOffset;
            self.conn.read_payload(self.pool, offset, out);
        }
        pub fn consume(self: *Self, ev: Event) void {
            const current_event = self.event() orelse return;
            if (!std.meta.eql(ev, current_event)) return;
            switch (ev) {
                .connected => |generation| {
                    tiger.assert(generation == self.generation);
                    self.connected_event = false;
                },
                .disconnected => |generation| {
                    tiger.assert(generation == self.generation);
                    self.disconnected_event = false;
                },
                .request => |request| {
                    tiger.assert(request.generation == self.generation);
                    self.conn.take(self.pool);
                },
            }
        }
        pub fn send(self: *Self, generation: u64, kind: frame.Kind, id: u64, payload: []const u8) !void {
            if (self.phase != .established or self.generation != generation) return error.PeerGone;
            if (kind == .request or id % 2 != 1) return error.BadId;
            try self.conn.queue(self.pool, kind, id, payload, false);
        }
        pub fn drive(self: *Self) !void {
            const now = self.backend.now();
            if (self.phase == .waiting and !self.stopping and !self.disconnected_event and now >= self.next_race) {
                self.phase = .racing;
                self.race_start = now;
                for (&self.candidates, 0..) |*candidate, i| {
                    tiger.assert(candidate.connect_op.state == .idle and candidate.close_op.state == .idle);
                    candidate.state = if (i < self.endpoints.len) .idle else .finished;
                    candidate.connect_result = null;
                    candidate.close_result = null;
                }
            }
            for (&self.candidates, 0..) |*candidate, index| {
                switch (candidate.state) {
                    .idle => if (self.phase == .racing and now >= self.race_start +| index * 150) {
                        try self.backend.submit(&candidate.connect_op, .{ .connect = self.endpoints[index] });
                        candidate.deadline = now +| 10_000;
                        candidate.state = .connecting;
                    },
                    .connecting => {
                        if (candidate.connect_result) |result| {
                            candidate.connect_result = null;
                            switch (result) {
                                .connected => |socket| {
                                    candidate.socket = socket;
                                    if (self.phase == .racing) {
                                        candidate.hs.init_initiator(self.backend, socket, self.key, self.hub_key) catch {
                                            candidate.state = .closing;
                                            continue;
                                        };
                                        candidate.has_hs = true;
                                        candidate.deadline = now +| 10_000;
                                        candidate.state = .handshake;
                                    } else candidate.state = .closing;
                                },
                                else => candidate.state = .finished,
                            }
                        } else if (now >= candidate.deadline or self.phase != .racing) self.backend.cancel(&candidate.connect_op);
                    },
                    .handshake => {
                        if (now >= candidate.deadline or self.phase != .racing) {
                            self.cancel_candidate(candidate);
                        } else {
                            candidate.hs.drive(self.backend) catch {
                                self.cancel_candidate(candidate);
                                continue;
                            };
                            if (candidate.hs.ready()) {
                                var session = candidate.hs.take();
                                defer session.deinit();
                                candidate.hs.deinit();
                                candidate.has_hs = false;
                                self.conn.init(candidate.socket, session);
                                self.has_conn = true;
                                self.winner = index;
                                candidate.state = .winner;
                                self.phase = .hello;
                                self.last_rx = now;
                                self.rx_serial = 0;
                                self.generation +%= 1;
                                if (self.generation == 0) self.generation = 1;
                                self.conn.queue(self.pool, .request, 2, self.hello, true) catch {
                                    self.close_link();
                                    continue;
                                };
                                for (&self.candidates, 0..) |*other, i| if (i != index) self.cancel_candidate(other);
                            }
                        }
                    },
                    .winner => {},
                    .closing => {
                        if (self.winner != null and self.winner.? == index and self.has_conn) {
                            if (!self.conn.drained()) continue;
                        } else if (candidate.has_hs) {
                            if (!candidate.hs.drained()) continue;
                        }
                        try self.backend.submit(&candidate.close_op, .{ .close = candidate.socket });
                        if (self.winner != null and self.winner.? == index and self.has_conn) {
                            self.conn.deinit(self.pool);
                            self.has_conn = false;
                        } else if (candidate.has_hs) {
                            candidate.hs.deinit();
                            candidate.has_hs = false;
                        }
                        candidate.state = .draining;
                    },
                    .draining => if (candidate.close_result != null) {
                        candidate.close_result = null;
                        candidate.state = .finished;
                    },
                    .finished => {},
                }
            }
            if (self.phase == .hello or self.phase == .established) {
                if (now -| self.last_rx > 15_000) {
                    self.close_link();
                    return;
                }
                self.conn.drive(self.backend, self.pool) catch {
                    self.close_link();
                    return;
                };
                if (self.conn.frame_ready) {
                    if (self.rx_serial != self.conn.rx_serial) {
                        self.rx_serial = self.conn.rx_serial;
                        self.last_rx = now;
                    }
                    const hdr = self.conn.header(self.pool) catch {
                        self.close_link();
                        return;
                    };
                    if (self.phase == .hello) {
                        if (hdr.kind != .response or hdr.id != 2 or hdr.payload_len > self.scratch.len) {
                            self.close_link();
                            return;
                        }
                        self.conn.read_payload(self.pool, 0, self.scratch[0..hdr.payload_len]);
                        if (!self.hooks.hello_reply(self.hooks.context, self.scratch[0..hdr.payload_len])) {
                            self.close_link();
                            return;
                        }
                        self.conn.take(self.pool);
                        self.phase = .established;
                        self.backoff = 500;
                        self.heartbeat_at = now +| 5_000;
                        self.connected_event = true;
                    } else if (hdr.kind != .request and hdr.id % 2 == 0) {
                        self.conn.take(self.pool);
                    } else if (hdr.kind != .request or hdr.id % 2 != 1) {
                        self.close_link();
                        return;
                    }
                }
                if (self.phase == .established and now >= self.heartbeat_at) {
                    const queued = if (self.conn.queue(self.pool, .request, self.next_id, self.hello, true)) |_| true else |err| switch (err) {
                        error.Busy, error.Full => false,
                        else => {
                            self.close_link();
                            return;
                        },
                    };
                    if (queued) {
                        self.next_id +%= 2;
                        if (self.next_id == 0) self.next_id = 4;
                        self.heartbeat_at = now +| 5_000;
                    }
                }
            }
            if (self.phase == .racing or self.phase == .closing) {
                var all_finished = true;
                for (self.candidates) |candidate| if (candidate.state != .finished) {
                    all_finished = false;
                    break;
                };
                if (all_finished) {
                    self.winner = null;
                    if (self.stopping) self.phase = .stopped else {
                        self.next_race = now +| @min(5_000, self.backoff + self.backend.random() % (self.backoff / 4 + 1));
                        self.backoff = @min(5_000, self.backoff * 2);
                        self.phase = .waiting;
                    }
                }
            }
        }
    };
}
