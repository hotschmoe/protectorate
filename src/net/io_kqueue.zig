const std = @import("std");
const builtin = @import("builtin");
const c = std.c;
const tiger = @import("../tiger.zig");
const io = @import("io.zig");
const bounded = @import("bounded.zig");

/// The macOS and BSD readiness backend: the same shape as io_epoll, with kevent for readiness and a pipe for wake.
pub const Kqueue = struct {
    const operations_max = 512;
    comptime {
        tiger.comptime_assert(operations_max >= 3 * bounded.fleet_slots + 3, "fleet operation capacity");
    }
    const Pending = struct {
        op: *io.Operation,
        cancelled: bool = false,
        connect_fd: ?c_int = null,
        connecting: bool = false,
        connect_ready: bool = false,
        serial: u64,
    };
    const no_signal = if (@hasDecl(c.MSG, "NOSIGNAL")) c.MSG.NOSIGNAL else 0;

    fd: c_int,
    wake_read: c_int,
    wake_write: c_int,
    pending: [operations_max]Pending = undefined,
    pending_count: usize = 0,
    next_serial: u64 = 0,

    pub fn init() !Kqueue {
        const queue = c.kqueue();
        if (queue < 0) return error.KqueueCreateFailed;
        errdefer _ = c.close(queue);
        var ends: [2]c_int = undefined;
        if (c.pipe(&ends) != 0) return error.KqueueCreateFailed;
        errdefer {
            _ = c.close(ends[0]);
            _ = c.close(ends[1]);
        }
        if (!prepare(ends[0]) or !prepare(ends[1])) return error.KqueueCreateFailed;
        const change = [1]c.Kevent{.{ .ident = @intCast(ends[0]), .filter = c.EVFILT.READ, .flags = c.EV.ADD | c.EV.CLEAR, .fflags = 0, .data = 0, .udata = 0 }};
        var none: [0]c.Kevent = undefined;
        if (c.kevent(queue, &change, 1, &none, 0, null) < 0) return error.KqueueCreateFailed;
        return .{ .fd = queue, .wake_read = ends[0], .wake_write = ends[1] };
    }

    pub fn deinit(self: *Kqueue) void {
        tiger.assert(self.pending_count == 0);
        _ = c.close(self.wake_read);
        _ = c.close(self.wake_write);
        _ = c.close(self.fd);
    }

    /// Makes a blocked or future poll return at once. Safe to call from any thread while the backend lives.
    pub fn wake(self: *const Kqueue) void {
        const one: u8 = 1;
        _ = c.write(self.wake_write, @ptrCast(&one), 1);
    }

    fn prepare(fd: c_int) bool {
        const flags = c.fcntl(fd, c.F.GETFL);
        if (flags < 0) return false;
        const nonblock: c_int = @bitCast(c.O{ .NONBLOCK = true });
        if (c.fcntl(fd, c.F.SETFL, flags | nonblock) < 0) return false;
        return c.fcntl(fd, c.F.SETFD, @as(c_int, c.FD_CLOEXEC)) >= 0;
    }

    fn address_bytes(endpoint: io.Endpoint) struct { v4: c.sockaddr.in, v6: c.sockaddr.in6 } {
        return .{
            .v4 = .{ .port = std.mem.nativeToBig(u16, endpoint.port), .addr = @bitCast(endpoint.address[0..4].*) },
            .v6 = .{ .port = std.mem.nativeToBig(u16, endpoint.port), .flowinfo = 0, .addr = endpoint.address, .scope_id = 0 },
        };
    }

    /// Opens a nonblocking listening socket and reports the port it bound.
    pub fn listen(endpoint: io.Endpoint, backlog: u32) !struct { socket: io.Socket, port: u16 } {
        const family: c_uint = if (endpoint.family == .ipv4) c.AF.INET else c.AF.INET6;
        const created = c.socket(family, c.SOCK.STREAM, 0);
        if (created < 0) return error.ListenFailed;
        errdefer _ = c.close(created);
        if (!prepare(created)) return error.ListenFailed;
        const one: c_int = 1;
        if (c.setsockopt(created, c.SOL.SOCKET, c.SO.REUSEADDR, &one, @sizeOf(c_int)) != 0) return error.ListenFailed;
        const addresses = address_bytes(endpoint);
        var port: u16 = endpoint.port;
        if (endpoint.family == .ipv4) {
            if (c.bind(created, @ptrCast(&addresses.v4), @sizeOf(c.sockaddr.in)) != 0) return error.ListenFailed;
            var bound: c.sockaddr.in = undefined;
            var size: c.socklen_t = @sizeOf(c.sockaddr.in);
            if (c.getsockname(created, @ptrCast(&bound), &size) != 0) return error.ListenFailed;
            port = std.mem.bigToNative(u16, bound.port);
        } else {
            if (c.bind(created, @ptrCast(&addresses.v6), @sizeOf(c.sockaddr.in6)) != 0) return error.ListenFailed;
            var bound: c.sockaddr.in6 = undefined;
            var size: c.socklen_t = @sizeOf(c.sockaddr.in6);
            if (c.getsockname(created, @ptrCast(&bound), &size) != 0) return error.ListenFailed;
            port = std.mem.bigToNative(u16, bound.port);
        }
        if (c.listen(created, backlog) != 0) return error.ListenFailed;
        return .{ .socket = socket(created), .port = port };
    }

    /// Closes a socket that no operation owns, such as a listener whose bus never started.
    pub fn close_now(handle: io.Socket) void {
        _ = c.close(descriptor(handle));
    }

    /// Best-effort nonblocking write of a short reply on a socket no operation owns.
    pub fn send_now(handle: io.Socket, bytes: []const u8) void {
        _ = c.send(descriptor(handle), bytes.ptr, bytes.len, no_signal | c.MSG.DONTWAIT);
    }

    pub fn shutdown_write(handle: io.Socket) void {
        _ = c.shutdown(descriptor(handle), c.SHUT.WR);
    }

    pub fn peer(handle: io.Socket) ?io.Endpoint {
        var storage: [128]u8 align(8) = undefined;
        var size: c.socklen_t = storage.len;
        if (c.getpeername(descriptor(handle), @ptrCast(&storage), &size) != 0) return null;
        const family: c_int = @intCast(@as(*const c.sockaddr, @ptrCast(&storage)).family);
        var out: io.Endpoint = .{ .address = @splat(0), .port = 0, .family = .ipv4 };
        if (family == c.AF.INET) {
            const v4: *const c.sockaddr.in = @ptrCast(&storage);
            @memcpy(out.address[0..4], std.mem.asBytes(&v4.addr)[0..4]);
            return out;
        }
        if (family == c.AF.INET6) {
            const v6: *const c.sockaddr.in6 = @ptrCast(&storage);
            out.address = v6.addr;
            out.family = .ipv6;
            return out;
        }
        return null;
    }

    pub fn now(_: *const Kqueue) u64 {
        var ts: c.timespec = undefined;
        tiger.assert(c.clock_gettime(.MONOTONIC, &ts) == 0);
        return @as(u64, @intCast(ts.sec)) * 1000 + @as(u64, @intCast(@divFloor(ts.nsec, 1_000_000)));
    }

    pub fn random(_: *Kqueue) u64 {
        var bytes: [8]u8 = undefined;
        c.arc4random_buf(&bytes, bytes.len);
        return std.mem.readInt(u64, &bytes, .little);
    }

    pub fn submit(self: *Kqueue, op: *io.Operation, request: io.Operation.Request) error{ResourceExhausted}!void {
        if (self.pending_count == self.pending.len) return error.ResourceExhausted;
        tiger.assert(self.next_serial < std.math.maxInt(u64));
        self.next_serial += 1;
        op.begin(request);
        self.pending[self.pending_count] = .{ .op = op, .serial = self.next_serial };
        self.pending_count += 1;
    }

    pub fn cancel(self: *Kqueue, op: *io.Operation) void {
        for (self.pending[0..self.pending_count]) |*pending| {
            if (pending.op == op) {
                pending.cancelled = true;
                return;
            }
        }
    }

    fn descriptor(value: io.Socket) c_int {
        return @intCast(@backingInt(value));
    }

    fn socket(fd: c_int) io.Socket {
        return @fromBackingInt(@intCast(@as(u64, @intCast(fd))));
    }

    fn failure(err: c.E) io.Failure {
        return switch (err) {
            .CONNREFUSED => error.Refused,
            .TIMEDOUT => error.TimedOut,
            .CONNRESET, .PIPE, .CONNABORTED => error.Reset,
            .BADF, .NOTCONN => error.Closed,
            .NETUNREACH, .HOSTUNREACH => error.Unreachable,
            .NOBUFS, .NOMEM, .MFILE, .NFILE => error.ResourceExhausted,
            else => error.Unexpected,
        };
    }

    fn configure(fd: c_int) bool {
        const one: c_int = 1;
        const idle: c_int = 10;
        if (c.setsockopt(fd, c.SOL.SOCKET, c.SO.KEEPALIVE, &one, @sizeOf(c_int)) != 0) return false;
        if (c.setsockopt(fd, c.IPPROTO.TCP, c.TCP.NODELAY, &one, @sizeOf(c_int)) != 0) return false;
        if (@hasDecl(c.SO, "NOSIGPIPE")) {
            if (c.setsockopt(fd, c.SOL.SOCKET, c.SO.NOSIGPIPE, &one, @sizeOf(c_int)) != 0) return false;
        }
        if (@hasDecl(c.TCP, "KEEPALIVE")) _ = c.setsockopt(fd, c.IPPROTO.TCP, c.TCP.KEEPALIVE, &idle, @sizeOf(c_int));
        return true;
    }

    fn has_other(self: *const Kqueue, pending: *const Pending, fd: c_int) bool {
        for (self.pending[0..self.pending_count]) |*other| {
            if (other == pending) continue;
            const other_fd: ?c_int = switch (other.op.request) {
                .accept => |s| descriptor(s),
                .recv => |r| descriptor(r.socket),
                .send => |s| descriptor(s.socket),
                .connect => other.connect_fd,
                else => null,
            };
            if (other_fd != null and other_fd.? == fd) return true;
        }
        return false;
    }

    fn attempt(self: *Kqueue, pending: *Pending) ?io.Result {
        if (pending.cancelled) {
            if (pending.connect_fd) |fd| _ = c.close(fd);
            return .{ .failed = error.Cancelled };
        }
        switch (pending.op.request) {
            .timeout => |deadline| return if (self.now() >= deadline) .{ .timed_out = {} } else null,
            .close => |s| {
                const fd = descriptor(s);
                if (self.has_other(pending, fd)) return null;
                const rc = c.close(fd);
                return if (rc == 0) .{ .closed = {} } else .{ .failed = failure(c.errno(rc)) };
            },
            .accept => |s| {
                const rc = c.accept(descriptor(s), null, null);
                if (rc < 0) {
                    const err = c.errno(rc);
                    if (err == .AGAIN or err == .INTR or err == .CONNABORTED) return null;
                    return .{ .failed = failure(err) };
                }
                if (!prepare(rc) or !configure(rc)) {
                    _ = c.close(rc);
                    return .{ .failed = error.Unexpected };
                }
                return .{ .accepted = socket(rc) };
            },
            .recv => |r| {
                const rc = c.recv(descriptor(r.socket), r.buffer.ptr, r.buffer.len, 0);
                if (rc < 0) {
                    const err = c.errno(rc);
                    if (err == .AGAIN or err == .INTR) return null;
                    return .{ .failed = failure(err) };
                }
                return .{ .received = @intCast(rc) };
            },
            .send => |s| {
                const rc = c.send(descriptor(s.socket), s.buffer.ptr, s.buffer.len, no_signal);
                if (rc < 0) {
                    const err = c.errno(rc);
                    if (err == .AGAIN or err == .INTR) return null;
                    return .{ .failed = failure(err) };
                }
                return .{ .sent = @intCast(rc) };
            },
            .connect => |endpoint| {
                if (pending.connect_fd == null) {
                    const family: c_uint = if (endpoint.family == .ipv4) c.AF.INET else c.AF.INET6;
                    const created = c.socket(family, c.SOCK.STREAM, 0);
                    if (created < 0) return .{ .failed = failure(c.errno(created)) };
                    pending.connect_fd = created;
                    if (!prepare(created) or !configure(created)) {
                        _ = c.close(created);
                        pending.connect_fd = null;
                        return .{ .failed = error.Unexpected };
                    }
                    const addresses = address_bytes(endpoint);
                    const rc = if (endpoint.family == .ipv4)
                        c.connect(created, @ptrCast(&addresses.v4), @sizeOf(c.sockaddr.in))
                    else
                        c.connect(created, @ptrCast(&addresses.v6), @sizeOf(c.sockaddr.in6));
                    if (rc != 0) {
                        const err = c.errno(rc);
                        if (err == .INPROGRESS or err == .ALREADY or err == .INTR) {
                            pending.connecting = true;
                            return null;
                        }
                        _ = c.close(created);
                        pending.connect_fd = null;
                        return .{ .failed = failure(err) };
                    }
                } else if (pending.connecting) {
                    if (!pending.connect_ready) return null;
                    var socket_error: c_int = 0;
                    var size: c.socklen_t = @sizeOf(c_int);
                    const rc = c.getsockopt(pending.connect_fd.?, c.SOL.SOCKET, c.SO.ERROR, &socket_error, &size);
                    if (rc != 0) socket_error = @backingInt(c.errno(rc));
                    if (socket_error != 0) {
                        const err: c.E = @fromBackingInt(@intCast(socket_error));
                        _ = c.close(pending.connect_fd.?);
                        pending.connect_fd = null;
                        return .{ .failed = failure(err) };
                    }
                }
                const fd = pending.connect_fd.?;
                pending.connect_fd = null;
                return .{ .connected = socket(fd) };
            },
        }
    }

    pub fn poll(self: *Kqueue, max_wait_ms: u32) !void {
        var index: usize = 0;
        var completed = false;
        const serial = self.next_serial;
        while (index < self.pending_count) {
            if (self.pending[index].serial > serial) {
                index += 1;
                continue;
            }
            const result = self.attempt(&self.pending[index]);
            if (result) |value| {
                const op = self.pending[index].op;
                self.pending_count -= 1;
                self.pending[index] = self.pending[self.pending_count];
                op.complete(value);
                completed = true;
            } else index += 1;
        }
        var wait_ms: u64 = if (completed) 0 else max_wait_ms;
        const now_ms = self.now();
        var changes: [2 * operations_max]c.Kevent = undefined;
        var change_count: usize = 0;
        for (self.pending[0..self.pending_count]) |pending| {
            switch (pending.op.request) {
                .accept => |s| change_count = add_change(&changes, change_count, descriptor(s), c.EVFILT.READ),
                .recv => |r| change_count = add_change(&changes, change_count, descriptor(r.socket), c.EVFILT.READ),
                .send => |s| change_count = add_change(&changes, change_count, descriptor(s.socket), c.EVFILT.WRITE),
                .connect => if (pending.connect_fd) |fd| {
                    change_count = add_change(&changes, change_count, fd, c.EVFILT.WRITE);
                },
                .timeout => |deadline| wait_ms = @min(wait_ms, deadline -| now_ms),
                .close => {},
            }
        }
        var events: [64]c.Kevent = undefined;
        const timeout: c.timespec = .{ .sec = @intCast(wait_ms / 1000), .nsec = @intCast((wait_ms % 1000) * 1_000_000) };
        const rc = c.kevent(self.fd, &changes, @intCast(change_count), &events, events.len, &timeout);
        if (rc < 0) {
            if (c.errno(rc) == .INTR) return;
            return error.KqueueWaitFailed;
        }
        for (events[0..@intCast(rc)]) |event| {
            if (event.flags & c.EV.ERROR != 0) continue;
            if (event.ident == @as(usize, @intCast(self.wake_read))) {
                var drained: [64]u8 = undefined;
                while (c.read(self.wake_read, &drained, drained.len) > 0) {}
                continue;
            }
            if (event.filter != c.EVFILT.WRITE) continue;
            for (self.pending[0..self.pending_count]) |*pending| {
                if (pending.connect_fd != null and @as(usize, @intCast(pending.connect_fd.?)) == event.ident) pending.connect_ready = true;
            }
        }
    }

    fn add_change(changes: *[2 * operations_max]c.Kevent, count: usize, fd: c_int, filter: anytype) usize {
        changes[count] = .{ .ident = @intCast(fd), .filter = filter, .flags = c.EV.ADD | c.EV.ONESHOT, .fflags = 0, .data = 0, .udata = 0 };
        return count + 1;
    }
};
