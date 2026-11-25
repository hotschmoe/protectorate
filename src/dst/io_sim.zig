const std = @import("std");
const tiger = @import("../tiger.zig");
const io = @import("../net/io.zig");

pub const Sim = struct {
    const socket_limit = 256;
    const operation_limit = 512;
    const buffer_size = 4096;
    pub const ConnectFault = enum { none, refuse, timeout };
    pub const Route = struct {
        endpoint: io.Endpoint,
        connect: ConnectFault = .none,
        partition_until: u64 = 0,
        partition_reset_at: u64 = 0,
        crashed: bool = false,
        slow: bool = false,
    };
    const SocketState = struct {
        generation: u32 = 0,
        state: enum { free, listener, stream } = .free,
        endpoint: io.Endpoint = undefined,
        peer: ?io.Socket = null,
        incoming: [buffer_size]u8 = undefined,
        head: usize = 0,
        len: usize = 0,
        accepted: ?io.Socket = null,
        reset: bool = false,
        tamper_after: ?usize = null,
    };
    const Pending = struct {
        operation: ?*io.Operation = null,
        due: u64 = 0,
        cancelled: bool = false,
    };

    sockets: [socket_limit]SocketState = @splat(.{}),
    pending: [operation_limit]Pending = @splat(.{}),
    pending_count: usize = 0,
    prng: std.Random.DefaultPrng,
    tick: u64 = 0,
    partitioned: bool = false,
    transfer_max: usize = 4096,
    routes: [128]?Route = @splat(null),
    route_count: usize = 0,

    pub fn route(self: *Sim, endpoint: io.Endpoint) *Route {
        for (self.routes[0..self.route_count]) |*entry| {
            if (entry.*) |*value| {
                if (std.meta.eql(value.endpoint, endpoint)) return value;
            }
        }
        if (self.route_count == self.routes.len) @panic("simulated route capacity exceeded");
        const entry = &self.routes[self.route_count];
        self.route_count += 1;
        entry.* = .{ .endpoint = endpoint };
        return &entry.*.?;
    }

    pub fn partition(self: *Sim, endpoint: io.Endpoint, duration: u64, reset_after: u64) void {
        const r = self.route(endpoint);
        r.partition_until = self.tick +| duration;
        r.partition_reset_at = self.tick +| @max(1, reset_after);
    }

    pub fn crash(self: *Sim, endpoint: io.Endpoint) void {
        self.route(endpoint).crashed = true;
        for (&self.sockets) |*socket| {
            if (socket.state == .stream and std.meta.eql(socket.endpoint, endpoint)) socket.reset = true;
        }
    }

    pub fn restart(self: *Sim, endpoint: io.Endpoint) void {
        self.route(endpoint).crashed = false;
    }

    pub fn tamper_send(self: *Sim, socket: io.Socket, after: usize) void {
        const stream = self.get(socket) orelse @panic("invalid simulated stream");
        tiger.assert(stream.state == .stream);
        stream.tamper_after = after;
    }

    pub fn init(seed: u64) Sim {
        return .{ .prng = std.Random.DefaultPrng.init(seed) };
    }

    pub fn now(self: *const Sim) u64 {
        return self.tick;
    }

    pub fn random(self: *Sim) u64 {
        return self.prng.random().int(u64);
    }

    fn handle(index: usize, generation: u32) io.Socket {
        return @fromBackingInt(@intCast((@as(u64, generation) << 32) | @as(u64, @intCast(index))));
    }

    fn get(self: *Sim, socket: io.Socket) ?*SocketState {
        const value = @backingInt(socket);
        const index: usize = @intCast(value & 0xffffffff);
        if (index >= socket_limit) return null;
        const state = &self.sockets[index];
        if (state.state == .free or state.generation != value >> 32) return null;
        return state;
    }

    fn allocate(self: *Sim, state: @FieldType(SocketState, "state")) ?io.Socket {
        for (&self.sockets, 0..) |*socket, i| {
            if (socket.state != .free) continue;
            const generation = socket.generation +% 1;
            if (generation == 0) continue;
            socket.* = .{ .generation = generation, .state = state };
            return handle(i, generation);
        }
        return null;
    }

    pub fn listen(self: *Sim, endpoint: io.Endpoint) ?io.Socket {
        const socket = self.allocate(.listener) orelse return null;
        self.get(socket).?.endpoint = endpoint;
        return socket;
    }

    pub fn submit(self: *Sim, op: *io.Operation, request: io.Operation.Request) error{ResourceExhausted}!void {
        if (self.pending_count == operation_limit) return error.ResourceExhausted;
        for (self.pending[0..self.pending_count]) |other| {
            const existing = other.operation.?;
            switch (request) {
                .recv => |rw| switch (existing.request) {
                    .recv => |prev| tiger.assert(prev.socket != rw.socket),
                    else => {},
                },
                .send => |rw| switch (existing.request) {
                    .send => |prev| tiger.assert(prev.socket != rw.socket),
                    else => {},
                },
                else => {},
            }
        }
        op.begin(request);
        const delay: u64 = switch (request) {
            .connect => |endpoint| if (self.route(endpoint).connect == .timeout) 100 else 1 + self.random() % 3,
            else => 1 + self.random() % 3,
        };
        self.pending[self.pending_count] = .{ .operation = op, .due = self.tick +| delay };
        self.pending_count += 1;
    }

    pub fn cancel(self: *Sim, op: *io.Operation) void {
        for (self.pending[0..self.pending_count]) |*pending| {
            if (pending.operation == op) {
                pending.cancelled = true;
                return;
            }
        }
    }

    pub fn assert_drained(self: *const Sim) void {
        tiger.assert(self.pending_count == 0);
        for (self.sockets) |socket| tiger.assert(socket.state == .free);
    }

    pub fn advance(self: *Sim) void {
        self.tick += 1;
        for (self.routes[0..self.route_count]) |entry| {
            const r = entry orelse continue;
            if (r.partition_reset_at == 0 or self.tick < r.partition_reset_at or self.tick >= r.partition_until) continue;
            for (&self.sockets) |*socket| {
                if (socket.state == .stream and std.meta.eql(socket.endpoint, r.endpoint)) socket.reset = true;
            }
        }
        var index: usize = 0;
        while (index < self.pending_count) {
            const pending = self.pending[index];
            const op = pending.operation.?;
            if (pending.due > self.tick) {
                index += 1;
                continue;
            }
            const result: ?io.Result = if (pending.cancelled) .{ .failed = error.Cancelled } else self.attempt(op.request);
            if (result) |done| {
                self.pending_count -= 1;
                self.pending[index] = self.pending[self.pending_count];
                self.pending[self.pending_count] = .{};
                op.complete(done);
            } else index += 1;
        }
    }

    fn attempt(self: *Sim, request: io.Operation.Request) ?io.Result {
        switch (request) {
            .accept => |socket| {
                const listener = self.get(socket) orelse return .{ .failed = error.Closed };
                if (listener.state != .listener) return .{ .failed = error.Closed };
                const accepted = listener.accepted orelse return null;
                listener.accepted = null;
                return .{ .accepted = accepted };
            },
            .connect => |endpoint| {
                if (self.partitioned) return null;
                const r = self.route(endpoint);
                if (r.crashed or r.connect == .refuse) return .{ .failed = error.Refused };
                if (r.connect == .timeout) return .{ .failed = error.TimedOut };
                if (self.tick < r.partition_until) return null;
                for (&self.sockets) |*listener| {
                    if (listener.state != .listener or !std.meta.eql(listener.endpoint, endpoint)) continue;
                    if (listener.accepted != null) return null;
                    const client = self.allocate(.stream) orelse return .{ .failed = error.ResourceExhausted };
                    const server = self.allocate(.stream) orelse {
                        self.get(client).?.state = .free;
                        return .{ .failed = error.ResourceExhausted };
                    };
                    self.get(client).?.peer = server;
                    self.get(server).?.peer = client;
                    self.get(client).?.endpoint = endpoint;
                    self.get(server).?.endpoint = endpoint;
                    listener.accepted = server;
                    return .{ .connected = client };
                }
                return .{ .failed = error.Refused };
            },
            .recv => |recv| {
                const socket = self.get(recv.socket) orelse return .{ .failed = error.Closed };
                if (socket.state != .stream) return .{ .failed = error.Closed };
                if (socket.reset) return .{ .failed = error.Reset };
                const r = self.route(socket.endpoint);
                if (self.partitioned or self.tick < r.partition_until) return null;
                if (r.slow and self.tick % 8 != 0) return null;
                if (socket.len == 0) return null;
                tiger.assert(self.transfer_max > 0);
                const count = @min(recv.buffer.len, @min(socket.len, 1 + self.random() % self.transfer_max));
                const first = @min(count, buffer_size - socket.head);
                @memcpy(recv.buffer[0..first], socket.incoming[socket.head..][0..first]);
                @memcpy(recv.buffer[first..count], socket.incoming[0 .. count - first]);
                socket.head = (socket.head + count) % buffer_size;
                socket.len -= count;
                return .{ .received = count };
            },
            .send => |send| {
                const socket = self.get(send.socket) orelse return .{ .failed = error.Closed };
                if (socket.state != .stream) return .{ .failed = error.Closed };
                if (socket.reset) return .{ .failed = error.Reset };
                if (self.partitioned) return null;
                const r = self.route(socket.endpoint);
                if (self.tick < r.partition_until) return null;
                const peer = self.get(socket.peer orelse return .{ .failed = error.Closed }) orelse return .{ .failed = error.Reset };
                if (peer.len == buffer_size) return null;
                tiger.assert(self.transfer_max > 0);
                const count = @min(send.buffer.len, @min(buffer_size - peer.len, 1 + self.random() % self.transfer_max));
                const tail = (peer.head + peer.len) % buffer_size;
                const first = @min(count, buffer_size - tail);
                @memcpy(peer.incoming[tail..][0..first], send.buffer[0..first]);
                @memcpy(peer.incoming[0 .. count - first], send.buffer[first..count]);
                if (socket.tamper_after) |after| {
                    if (after < count) {
                        peer.incoming[(tail + after) % buffer_size] ^= 1;
                        socket.tamper_after = null;
                    } else socket.tamper_after = after - count;
                }
                peer.len += count;
                return .{ .sent = count };
            },
            .close => |socket_handle| {
                const socket = self.get(socket_handle) orelse return .{ .failed = error.Closed };
                if (socket.accepted) |accepted| {
                    if (self.get(accepted)) |unaccepted| {
                        if (unaccepted.peer) |peer_handle| if (self.get(peer_handle)) |peer| {
                            peer.reset = true;
                        };
                        unaccepted.state = .free;
                    }
                }
                if (socket.peer) |peer_handle| if (self.get(peer_handle)) |peer| {
                    peer.reset = true;
                };
                socket.state = .free;
                return .{ .closed = {} };
            },
            .timeout => |deadline| if (self.tick >= deadline) {
                return .{ .timed_out = {} };
            } else return null,
        }
    }
};
