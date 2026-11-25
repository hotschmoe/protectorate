const std = @import("std");
const linux = std.os.linux;
const tiger = @import("../tiger.zig");
const io = @import("io.zig");
const bounded = @import("bounded.zig");

pub const Epoll = struct {
    const operations_max = 512;
    const watches_max = 256;
    comptime {
        tiger.comptime_assert(operations_max >= 3 * bounded.fleet_slots + 3, "fleet operation capacity");
    }
    const Pending = struct {
        op: *io.Operation,
        cancelled: bool = false,
        connect_fd: ?i32 = null,
        connecting: bool = false,
        connect_ready: bool = false,
        serial: u64,
    };
    const Watch = struct { fd: i32, flags: u32, seen: bool = false };

    fd: i32,
    pending: [operations_max]Pending = undefined,
    pending_count: usize = 0,
    next_serial: u64 = 0,
    watches: [watches_max]Watch = undefined,
    watch_count: usize = 0,

    pub fn init() !Epoll {
        const result = linux.epoll_create1(linux.EPOLL.CLOEXEC);
        if (linux.errno(result) != .SUCCESS) return error.EpollCreateFailed;
        return .{ .fd = @intCast(result) };
    }

    pub fn deinit(self: *Epoll) void {
        tiger.assert(self.pending_count == 0);
        _ = linux.close(self.fd);
    }

    pub fn now(_: *const Epoll) u64 {
        var ts: linux.timespec = undefined;
        tiger.assert(linux.errno(linux.clock_gettime(.MONOTONIC, &ts)) == .SUCCESS);
        return @as(u64, @intCast(ts.sec)) * 1000 + @as(u64, @intCast(@divFloor(ts.nsec, 1_000_000)));
    }

    pub fn random(_: *Epoll) u64 {
        var bytes: [8]u8 = undefined;
        var offset: usize = 0;
        while (offset < bytes.len) {
            const result = linux.getrandom(bytes[offset..].ptr, bytes.len - offset, 1);
            switch (linux.errno(result)) {
                .SUCCESS => offset += result,
                .INTR => continue,
                else => @panic("getrandom failed"),
            }
        }
        return std.mem.readInt(u64, &bytes, .little);
    }

    pub fn submit(self: *Epoll, op: *io.Operation, request: io.Operation.Request) error{ResourceExhausted}!void {
        if (self.pending_count == self.pending.len) return error.ResourceExhausted;
        tiger.assert(self.next_serial < std.math.maxInt(u64));
        self.next_serial += 1;
        op.begin(request);
        self.pending[self.pending_count] = .{ .op = op, .serial = self.next_serial };
        self.pending_count += 1;
    }

    pub fn cancel(self: *Epoll, op: *io.Operation) void {
        for (self.pending[0..self.pending_count]) |*pending| {
            if (pending.op == op) {
                pending.cancelled = true;
                return;
            }
        }
    }

    fn descriptor(value: io.Socket) i32 {
        return @intCast(@backingInt(value));
    }

    fn socket(fd: i32) io.Socket {
        return @fromBackingInt(@intCast(@as(u64, @intCast(fd))));
    }

    fn failure(err: linux.E) io.Failure {
        return switch (err) {
            .CONNREFUSED => error.Refused,
            .TIMEDOUT => error.TimedOut,
            .CONNRESET, .PIPE => error.Reset,
            .BADF, .NOTCONN => error.Closed,
            .NETUNREACH, .HOSTUNREACH => error.Unreachable,
            .NOBUFS, .NOMEM, .MFILE, .NFILE => error.ResourceExhausted,
            else => error.Unexpected,
        };
    }

    fn configure(fd: i32) bool {
        const one: c_int = 1;
        const timeout: c_int = 20_000;
        return linux.errno(linux.setsockopt(fd, linux.SOL.SOCKET, linux.SO.KEEPALIVE, std.mem.asBytes(&one).ptr, @sizeOf(c_int))) == .SUCCESS and
            linux.errno(linux.setsockopt(fd, linux.IPPROTO.TCP, linux.TCP.NODELAY, std.mem.asBytes(&one).ptr, @sizeOf(c_int))) == .SUCCESS and
            linux.errno(linux.setsockopt(fd, linux.IPPROTO.TCP, linux.TCP.USER_TIMEOUT, std.mem.asBytes(&timeout).ptr, @sizeOf(c_int))) == .SUCCESS;
    }

    fn has_other(self: *const Epoll, pending: *const Pending, fd: i32) bool {
        for (self.pending[0..self.pending_count]) |*other| {
            if (other == pending) continue;
            const other_fd: ?i32 = switch (other.op.request) {
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

    fn attempt(self: *Epoll, pending: *Pending) ?io.Result {
        if (pending.cancelled) {
            if (pending.connect_fd) |fd| _ = linux.close(fd);
            return .{ .failed = error.Cancelled };
        }
        switch (pending.op.request) {
            .timeout => |deadline| return if (self.now() >= deadline) .{ .timed_out = {} } else null,
            .close => |s| {
                const fd = descriptor(s);
                if (self.has_other(pending, fd)) return null;
                const rc = linux.close(fd);
                return if (linux.errno(rc) == .SUCCESS) .{ .closed = {} } else .{ .failed = failure(linux.errno(rc)) };
            },
            .accept => |s| {
                const rc = linux.accept4(descriptor(s), null, null, linux.SOCK.NONBLOCK | linux.SOCK.CLOEXEC);
                const err = linux.errno(rc);
                if (err == .AGAIN or err == .INTR) return null;
                if (err != .SUCCESS) return .{ .failed = failure(err) };
                const fd: i32 = @intCast(rc);
                if (!configure(fd)) {
                    _ = linux.close(fd);
                    return .{ .failed = error.Unexpected };
                }
                return .{ .accepted = socket(fd) };
            },
            .recv => |r| {
                const rc = linux.recvfrom(descriptor(r.socket), r.buffer.ptr, r.buffer.len, 0, null, null);
                const err = linux.errno(rc);
                if (err == .AGAIN or err == .INTR) return null;
                return if (err == .SUCCESS) .{ .received = rc } else .{ .failed = failure(err) };
            },
            .send => |s| {
                const rc = linux.sendto(descriptor(s.socket), s.buffer.ptr, s.buffer.len, linux.MSG.NOSIGNAL, null, 0);
                const err = linux.errno(rc);
                if (err == .AGAIN or err == .INTR) return null;
                return if (err == .SUCCESS) .{ .sent = rc } else .{ .failed = failure(err) };
            },
            .connect => |endpoint| {
                if (pending.connect_fd == null) {
                    const family: u32 = if (endpoint.family == .ipv4) linux.AF.INET else linux.AF.INET6;
                    const result = linux.socket(family, linux.SOCK.STREAM | linux.SOCK.NONBLOCK | linux.SOCK.CLOEXEC, 0);
                    if (linux.errno(result) != .SUCCESS) return .{ .failed = failure(linux.errno(result)) };
                    const fd: i32 = @intCast(result);
                    pending.connect_fd = fd;
                    if (!configure(fd)) {
                        _ = linux.close(fd);
                        pending.connect_fd = null;
                        return .{ .failed = error.Unexpected };
                    }
                    const result_connect = if (endpoint.family == .ipv4) blk: {
                        const addr = linux.sockaddr.in{ .port = std.mem.nativeToBig(u16, endpoint.port), .addr = @bitCast(endpoint.address[0..4].*) };
                        break :blk linux.connect(fd, &addr, @sizeOf(@TypeOf(addr)));
                    } else blk: {
                        const addr = linux.sockaddr.in6{ .family = linux.AF.INET6, .port = std.mem.nativeToBig(u16, endpoint.port), .flowinfo = 0, .addr = endpoint.address, .scope_id = 0 };
                        break :blk linux.connect(fd, &addr, @sizeOf(@TypeOf(addr)));
                    };
                    const err = linux.errno(result_connect);
                    if (err == .INPROGRESS or err == .ALREADY or err == .INTR) {
                        pending.connecting = true;
                        return null;
                    }
                    if (err != .SUCCESS) {
                        _ = linux.close(fd);
                        pending.connect_fd = null;
                        return .{ .failed = failure(err) };
                    }
                } else if (pending.connecting) {
                    if (!pending.connect_ready) return null;
                    var socket_error: c_int = 0;
                    var size: linux.socklen_t = @sizeOf(c_int);
                    const rc = linux.getsockopt(pending.connect_fd.?, linux.SOL.SOCKET, linux.SO.ERROR, std.mem.asBytes(&socket_error).ptr, &size);
                    if (linux.errno(rc) != .SUCCESS) socket_error = @backingInt(linux.errno(rc));
                    if (socket_error != 0) {
                        const err: linux.E = @fromBackingInt(@intCast(socket_error));
                        _ = linux.close(pending.connect_fd.?);
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

    fn watch(self: *Epoll, fd: i32, flags: u32) !void {
        for (self.watches[0..self.watch_count]) |*entry| {
            if (entry.fd == fd) {
                if (!entry.seen) entry.flags = 0;
                entry.seen = true;
                entry.flags |= flags;
                return;
            }
        }
        if (self.watch_count == self.watches.len) return error.ResourceExhausted;
        self.watches[self.watch_count] = .{ .fd = fd, .flags = flags, .seen = true };
        self.watch_count += 1;
    }

    pub fn poll(self: *Epoll, max_wait_ms: u32) !void {
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
        for (self.watches[0..self.watch_count]) |*entry| entry.seen = false;
        var wait_ms: u64 = if (completed) 0 else max_wait_ms;
        const now_ms = self.now();
        for (self.pending[0..self.pending_count]) |pending| {
            switch (pending.op.request) {
                .accept => |s| try self.watch(descriptor(s), linux.EPOLL.IN),
                .recv => |r| try self.watch(descriptor(r.socket), linux.EPOLL.IN),
                .send => |s| try self.watch(descriptor(s.socket), linux.EPOLL.OUT),
                .connect => if (pending.connect_fd) |fd| try self.watch(fd, linux.EPOLL.OUT),
                .timeout => |deadline| wait_ms = @min(wait_ms, deadline -| now_ms),
                .close => {},
            }
        }
        index = 0;
        while (index < self.watch_count) {
            const entry = self.watches[index];
            if (!entry.seen) {
                _ = linux.epoll_ctl(self.fd, linux.EPOLL.CTL_DEL, entry.fd, null);
                self.watch_count -= 1;
                self.watches[index] = self.watches[self.watch_count];
                continue;
            }
            var event = linux.epoll_event{ .events = entry.flags, .data = .{ .fd = entry.fd } };
            const mod = linux.epoll_ctl(self.fd, linux.EPOLL.CTL_MOD, entry.fd, &event);
            if (linux.errno(mod) == .NOENT) {
                const add = linux.epoll_ctl(self.fd, linux.EPOLL.CTL_ADD, entry.fd, &event);
                if (linux.errno(add) != .SUCCESS) return error.EpollControlFailed;
            } else if (linux.errno(mod) != .SUCCESS) return error.EpollControlFailed;
            index += 1;
        }
        var events: [watches_max]linux.epoll_event = undefined;
        const rc = linux.epoll_wait(self.fd, &events, events.len, @intCast(@min(wait_ms, std.math.maxInt(i32))));
        if (linux.errno(rc) != .SUCCESS and linux.errno(rc) != .INTR) return error.EpollWaitFailed;
        if (linux.errno(rc) == .SUCCESS) for (events[0..rc]) |event| {
            for (self.pending[0..self.pending_count]) |*pending| {
                if (pending.connect_fd != null and pending.connect_fd.? == event.data.fd) pending.connect_ready = true;
            }
        };
    }
};
