const std = @import("std");
const windows = std.os.windows;
const tiger = @import("../tiger.zig");
const io = @import("io.zig");
const bounded = @import("bounded.zig");

const SOCKET = usize;
const invalid_socket: SOCKET = std.math.maxInt(usize);
const BOOL = i32;
const HANDLE = windows.HANDLE;

const OVERLAPPED = extern struct {
    Internal: usize = 0,
    InternalHigh: usize = 0,
    Offset: u32 = 0,
    OffsetHigh: u32 = 0,
    hEvent: ?*anyopaque = null,
};

const OVERLAPPED_ENTRY = extern struct {
    key: usize,
    overlapped: ?*OVERLAPPED,
    internal: usize,
    bytes: u32,
};

const WSABUF = extern struct {
    len: u32,
    buf: ?[*]u8,
};

const GUID = extern struct {
    data1: u32,
    data2: u16,
    data3: u16,
    data4: [8]u8,
};

const TcpKeepalive = extern struct {
    onoff: u32,
    time_ms: u32,
    interval_ms: u32,
};

const AcceptEx = *const fn (SOCKET, SOCKET, *anyopaque, u32, u32, u32, *u32, *OVERLAPPED) callconv(.winapi) BOOL;
const ConnectEx = *const fn (SOCKET, *const anyopaque, c_int, ?*anyopaque, u32, ?*u32, *OVERLAPPED) callconv(.winapi) BOOL;

extern "ws2_32" fn WSAStartup(version: u16, data: *anyopaque) callconv(.winapi) c_int;
extern "ws2_32" fn WSACleanup() callconv(.winapi) c_int;
extern "ws2_32" fn WSASocketW(af: c_int, kind: c_int, protocol: c_int, info: ?*anyopaque, group: u32, flags: u32) callconv(.winapi) SOCKET;
extern "ws2_32" fn closesocket(s: SOCKET) callconv(.winapi) c_int;
extern "ws2_32" fn bind(s: SOCKET, name: *const anyopaque, len: c_int) callconv(.winapi) c_int;
const winsock = struct {
    extern "ws2_32" fn listen(s: SOCKET, backlog: c_int) callconv(.winapi) c_int;
};
extern "ws2_32" fn getsockname(s: SOCKET, name: *anyopaque, len: *c_int) callconv(.winapi) c_int;
extern "ws2_32" fn setsockopt(s: SOCKET, level: c_int, name: c_int, value: *const anyopaque, len: c_int) callconv(.winapi) c_int;
extern "ws2_32" fn WSAIoctl(s: SOCKET, code: u32, in: ?*const anyopaque, in_len: u32, out: ?*anyopaque, out_len: u32, returned: *u32, overlapped: ?*OVERLAPPED, routine: ?*anyopaque) callconv(.winapi) c_int;
extern "ws2_32" fn WSARecv(s: SOCKET, buffers: *WSABUF, count: u32, received: ?*u32, flags: *u32, overlapped: *OVERLAPPED, routine: ?*anyopaque) callconv(.winapi) c_int;
extern "ws2_32" fn WSASend(s: SOCKET, buffers: *WSABUF, count: u32, sent: ?*u32, flags: u32, overlapped: *OVERLAPPED, routine: ?*anyopaque) callconv(.winapi) c_int;
extern "ws2_32" fn WSAGetLastError() callconv(.winapi) c_int;
extern "ws2_32" fn WSAGetOverlappedResult(s: SOCKET, overlapped: *OVERLAPPED, transferred: *u32, wait: BOOL, flags: *u32) callconv(.winapi) BOOL;
extern "kernel32" fn CreateIoCompletionPort(file: HANDLE, port: ?HANDLE, key: usize, threads: u32) callconv(.winapi) ?HANDLE;
extern "kernel32" fn GetQueuedCompletionStatusEx(port: HANDLE, entries: [*]OVERLAPPED_ENTRY, count: u32, removed: *u32, wait_ms: u32, alertable: BOOL) callconv(.winapi) BOOL;
extern "kernel32" fn PostQueuedCompletionStatus(port: HANDLE, bytes: u32, key: usize, overlapped: ?*OVERLAPPED) callconv(.winapi) BOOL;
extern "kernel32" fn CancelIoEx(file: HANDLE, overlapped: ?*OVERLAPPED) callconv(.winapi) BOOL;
extern "kernel32" fn GetLastError() callconv(.winapi) u32;
extern "kernel32" fn QueryPerformanceCounter(value: *i64) callconv(.winapi) BOOL;
extern "kernel32" fn QueryPerformanceFrequency(value: *i64) callconv(.winapi) BOOL;
extern "advapi32" fn SystemFunction036(buffer: *anyopaque, len: u32) callconv(.winapi) u8;

const AF_INET = 2;
const AF_INET6 = 23;
const SOCK_STREAM = 1;
const SOL_SOCKET: c_int = 0xffff;
const IPPROTO_TCP = 6;
const SO_REUSEADDR = 4;
const SO_KEEPALIVE = 8;
const SO_UPDATE_ACCEPT_CONTEXT = 0x700B;
const SO_UPDATE_CONNECT_CONTEXT = 0x7010;
const TCP_NODELAY = 1;
const SIO_GET_EXTENSION_FUNCTION_POINTER = 0xC8000006;
const SIO_KEEPALIVE_VALS = 0x98000004;
const WSA_FLAG_OVERLAPPED = 0x01;
const WSA_FLAG_NO_HANDLE_INHERIT = 0x80;
const socket_flags = WSA_FLAG_OVERLAPPED | WSA_FLAG_NO_HANDLE_INHERIT;
const wait_timeout = 258;
const wake_key: usize = 1;
const socket_key: usize = 2;
const address_space = 2 * (@sizeOf(windows.ws2_32.sockaddr.in6) + 16);
const guid_accept_ex: GUID = .{ .data1 = 0xb5367df1, .data2 = 0xcbac, .data3 = 0x11cf, .data4 = .{ 0x95, 0xca, 0x00, 0x80, 0x5f, 0x48, 0xa1, 0x92 } };
const guid_connect_ex: GUID = .{ .data1 = 0x25a207b9, .data2 = 0xddf3, .data3 = 0x4660, .data4 = .{ 0x8e, 0xe9, 0x76, 0xe5, 0x8c, 0x74, 0x06, 0x3e } };

/// The Windows completion backend: sockets are overlapped and associated with one completion port, so an
/// operation maps directly to AcceptEx, ConnectEx, WSARecv or WSASend, and cancellation to CancelIoEx.
pub const Iocp = struct {
    const operations_max = 512;
    const listeners_max = 8;
    comptime {
        tiger.comptime_assert(operations_max >= 3 * bounded.fleet_slots + 3, "fleet operation capacity");
    }
    const State = enum { free, issued, immediate, timer, closing };
    const Pending = struct {
        overlapped: OVERLAPPED = .{},
        state: State = .free,
        op: *io.Operation = undefined,
        serial: u64 = 0,
        cancelled: bool = false,
        socket: SOCKET = invalid_socket,
        created: SOCKET = invalid_socket,
        buffer: WSABUF = undefined,
        address: [address_space]u8 align(8) = undefined,
        result: io.Result = undefined,
    };

    port: HANDLE,
    frequency: i64,
    accept_ex: AcceptEx,
    connect_ex: ConnectEx,
    pending: [operations_max]Pending = @splat(.{}),
    pending_count: usize = 0,
    next_serial: u64 = 0,
    associated: [listeners_max]SOCKET = @splat(invalid_socket),

    pub fn init() !Iocp {
        var data: [512]u8 align(8) = undefined;
        if (WSAStartup(0x0202, &data) != 0) return error.IocpCreateFailed;
        errdefer _ = WSACleanup();
        const port = CreateIoCompletionPort(windows.INVALID_HANDLE_VALUE, null, 0, 1) orelse return error.IocpCreateFailed;
        errdefer windows.CloseHandle(port);
        var frequency: i64 = 0;
        if (QueryPerformanceFrequency(&frequency) == 0 or frequency <= 0) return error.IocpCreateFailed;
        const probe = WSASocketW(AF_INET, SOCK_STREAM, IPPROTO_TCP, null, 0, socket_flags);
        if (probe == invalid_socket) return error.IocpCreateFailed;
        defer _ = closesocket(probe);
        return .{
            .port = port,
            .frequency = frequency,
            .accept_ex = try extension(AcceptEx, probe, &guid_accept_ex),
            .connect_ex = try extension(ConnectEx, probe, &guid_connect_ex),
        };
    }

    fn extension(comptime T: type, probe: SOCKET, guid: *const GUID) !T {
        var function: ?T = null;
        var returned: u32 = 0;
        if (WSAIoctl(probe, SIO_GET_EXTENSION_FUNCTION_POINTER, guid, @sizeOf(GUID), @ptrCast(&function), @sizeOf(?T), &returned, null, null) != 0) return error.IocpCreateFailed;
        return function orelse error.IocpCreateFailed;
    }

    pub fn deinit(self: *Iocp) void {
        tiger.assert(self.pending_count == 0);
        windows.CloseHandle(self.port);
        _ = WSACleanup();
    }

    /// Makes a blocked or future poll return at once. Safe to call from any thread while the backend lives.
    pub fn wake(self: *const Iocp) void {
        _ = PostQueuedCompletionStatus(self.port, 0, wake_key, null);
    }

    /// Opens a listening socket and reports the port it bound; it joins the completion port at its first accept.
    /// Winsock is started by init, so call this after init and before deinit.
    pub fn listen(endpoint: io.Endpoint, backlog: u32) !struct { socket: io.Socket, port: u16 } {
        const family: c_int = if (endpoint.family == .ipv4) AF_INET else AF_INET6;
        const created = WSASocketW(family, SOCK_STREAM, IPPROTO_TCP, null, 0, socket_flags);
        if (created == invalid_socket) return error.ListenFailed;
        errdefer _ = closesocket(created);
        const one: c_int = 1;
        if (setsockopt(created, SOL_SOCKET, SO_REUSEADDR, &one, @sizeOf(c_int)) != 0) return error.ListenFailed;
        var port: u16 = endpoint.port;
        if (endpoint.family == .ipv4) {
            const address = windows.ws2_32.sockaddr.in{ .port = std.mem.nativeToBig(u16, endpoint.port), .addr = @bitCast(endpoint.address[0..4].*) };
            if (bind(created, &address, @sizeOf(@TypeOf(address))) != 0) return error.ListenFailed;
            var bound: windows.ws2_32.sockaddr.in = undefined;
            var size: c_int = @sizeOf(@TypeOf(bound));
            if (getsockname(created, &bound, &size) != 0) return error.ListenFailed;
            port = std.mem.bigToNative(u16, bound.port);
        } else {
            const address = windows.ws2_32.sockaddr.in6{ .port = std.mem.nativeToBig(u16, endpoint.port), .flowinfo = 0, .addr = endpoint.address, .scope_id = 0 };
            if (bind(created, &address, @sizeOf(@TypeOf(address))) != 0) return error.ListenFailed;
            var bound: windows.ws2_32.sockaddr.in6 = undefined;
            var size: c_int = @sizeOf(@TypeOf(bound));
            if (getsockname(created, &bound, &size) != 0) return error.ListenFailed;
            port = std.mem.bigToNative(u16, bound.port);
        }
        if (winsock.listen(created, @intCast(backlog)) != 0) return error.ListenFailed;
        return .{ .socket = socket(created), .port = port };
    }

    /// Closes a socket that no operation owns, such as a listener whose bus never started.
    pub fn close_now(handle: io.Socket) void {
        _ = closesocket(descriptor(handle));
    }

    pub fn now(self: *const Iocp) u64 {
        var counter: i64 = 0;
        tiger.assert(QueryPerformanceCounter(&counter) != 0);
        return @intCast(@divFloor(@as(i128, counter) * 1000, self.frequency));
    }

    pub fn random(_: *Iocp) u64 {
        var bytes: [8]u8 = undefined;
        tiger.assert(SystemFunction036(&bytes, bytes.len) != 0);
        return std.mem.readInt(u64, &bytes, .little);
    }

    fn descriptor(value: io.Socket) SOCKET {
        return @intCast(@backingInt(value));
    }

    fn socket(handle: SOCKET) io.Socket {
        return @fromBackingInt(@intCast(handle));
    }

    fn as_handle(s: SOCKET) HANDLE {
        return @ptrFromInt(s);
    }

    fn failure(code: u32) io.Failure {
        return switch (code) {
            995 => error.Cancelled,
            10061, 1225 => error.Refused,
            10060, 121 => error.TimedOut,
            10054, 64, 1236, 10053, 10058, 1230 => error.Reset,
            10038, 10057 => error.Closed,
            10051, 10065, 1231, 1232, 1234, 10050 => error.Unreachable,
            10055, 10024, 1450, 8 => error.ResourceExhausted,
            else => error.Unexpected,
        };
    }

    fn last_failure() io.Failure {
        return failure(@intCast(WSAGetLastError()));
    }

    fn configure(s: SOCKET) bool {
        const one: c_int = 1;
        const keepalive: TcpKeepalive = .{ .onoff = 1, .time_ms = 10_000, .interval_ms = 5_000 };
        var returned: u32 = 0;
        return setsockopt(s, SOL_SOCKET, SO_KEEPALIVE, &one, @sizeOf(c_int)) == 0 and
            setsockopt(s, IPPROTO_TCP, TCP_NODELAY, &one, @sizeOf(c_int)) == 0 and
            WSAIoctl(s, SIO_KEEPALIVE_VALS, &keepalive, @sizeOf(TcpKeepalive), null, 0, &returned, null, null) == 0;
    }

    fn join_port(self: *Iocp, s: SOCKET) bool {
        return CreateIoCompletionPort(as_handle(s), self.port, socket_key, 0) != null;
    }

    fn join_listener(self: *Iocp, s: SOCKET) bool {
        for (self.associated) |known| if (known == s) return true;
        for (&self.associated) |*slot| {
            if (slot.* != invalid_socket) continue;
            if (!self.join_port(s)) return false;
            slot.* = s;
            return true;
        }
        return false;
    }

    fn forget_listener(self: *Iocp, s: SOCKET) void {
        for (&self.associated) |*slot| {
            if (slot.* == s) slot.* = invalid_socket;
        }
    }

    pub fn submit(self: *Iocp, op: *io.Operation, request: io.Operation.Request) error{ResourceExhausted}!void {
        const entry = for (&self.pending) |*candidate| {
            if (candidate.state == .free) break candidate;
        } else return error.ResourceExhausted;
        tiger.assert(self.next_serial < std.math.maxInt(u64));
        self.next_serial += 1;
        op.begin(request);
        entry.* = .{ .state = .issued, .op = op, .serial = self.next_serial };
        self.pending_count += 1;
        switch (request) {
            .accept => |listener| self.start_accept(entry, descriptor(listener)),
            .connect => |endpoint| self.start_connect(entry, endpoint),
            .recv => |r| {
                entry.socket = descriptor(r.socket);
                entry.buffer = .{ .len = @intCast(r.buffer.len), .buf = r.buffer.ptr };
                var flags: u32 = 0;
                if (WSARecv(entry.socket, &entry.buffer, 1, null, &flags, &entry.overlapped, null) != 0) self.check_pending(entry);
            },
            .send => |s| {
                entry.socket = descriptor(s.socket);
                entry.buffer = .{ .len = @intCast(s.buffer.len), .buf = @constCast(s.buffer.ptr) };
                if (WSASend(entry.socket, &entry.buffer, 1, null, 0, &entry.overlapped, null) != 0) self.check_pending(entry);
            },
            .timeout => entry.state = .timer,
            .close => |s| {
                entry.socket = descriptor(s);
                entry.state = .closing;
            },
        }
    }

    fn check_pending(_: *Iocp, entry: *Pending) void {
        const code = WSAGetLastError();
        if (code == 997) return;
        entry.state = .immediate;
        entry.result = .{ .failed = failure(@intCast(code)) };
    }

    fn fail_now(entry: *Pending, err: io.Failure) void {
        entry.state = .immediate;
        entry.result = .{ .failed = err };
    }

    fn start_accept(self: *Iocp, entry: *Pending, listener: SOCKET) void {
        entry.socket = listener;
        var storage: windows.ws2_32.sockaddr.storage = undefined;
        var size: c_int = @sizeOf(@TypeOf(storage));
        if (getsockname(listener, &storage, &size) != 0) return fail_now(entry, error.Closed);
        if (!self.join_listener(listener)) return fail_now(entry, error.ResourceExhausted);
        const created = WSASocketW(storage.family, SOCK_STREAM, IPPROTO_TCP, null, 0, socket_flags);
        if (created == invalid_socket) return fail_now(entry, last_failure());
        entry.created = created;
        var received: u32 = 0;
        const half: u32 = address_space / 2;
        if (self.accept_ex(listener, created, &entry.address, 0, half, half, &received, &entry.overlapped) == 0) {
            const code = WSAGetLastError();
            if (code != 997) {
                _ = closesocket(created);
                entry.created = invalid_socket;
                fail_now(entry, failure(@intCast(code)));
            }
        }
    }

    fn start_connect(self: *Iocp, entry: *Pending, endpoint: io.Endpoint) void {
        const family: c_int = if (endpoint.family == .ipv4) AF_INET else AF_INET6;
        const created = WSASocketW(family, SOCK_STREAM, IPPROTO_TCP, null, 0, socket_flags);
        if (created == invalid_socket) return fail_now(entry, last_failure());
        entry.created = created;
        entry.socket = created;
        const bound = if (endpoint.family == .ipv4) blk: {
            const any = windows.ws2_32.sockaddr.in{ .port = 0, .addr = 0 };
            break :blk bind(created, &any, @sizeOf(@TypeOf(any)));
        } else blk: {
            const any = windows.ws2_32.sockaddr.in6{ .port = 0, .flowinfo = 0, .addr = @splat(0), .scope_id = 0 };
            break :blk bind(created, &any, @sizeOf(@TypeOf(any)));
        };
        if (bound != 0 or !self.join_port(created) or !configure(created)) {
            const err = if (bound != 0) last_failure() else error.Unexpected;
            _ = closesocket(created);
            entry.created = invalid_socket;
            return fail_now(entry, err);
        }
        const issued = if (endpoint.family == .ipv4) blk: {
            const address = windows.ws2_32.sockaddr.in{ .port = std.mem.nativeToBig(u16, endpoint.port), .addr = @bitCast(endpoint.address[0..4].*) };
            break :blk self.connect_ex(created, &address, @sizeOf(@TypeOf(address)), null, 0, null, &entry.overlapped);
        } else blk: {
            const address = windows.ws2_32.sockaddr.in6{ .port = std.mem.nativeToBig(u16, endpoint.port), .flowinfo = 0, .addr = endpoint.address, .scope_id = 0 };
            break :blk self.connect_ex(created, &address, @sizeOf(@TypeOf(address)), null, 0, null, &entry.overlapped);
        };
        if (issued == 0) {
            const code = WSAGetLastError();
            if (code != 997) {
                _ = closesocket(created);
                entry.created = invalid_socket;
                fail_now(entry, failure(@intCast(code)));
            }
        }
    }

    pub fn cancel(self: *Iocp, op: *io.Operation) void {
        for (&self.pending) |*entry| {
            if (entry.state == .free or entry.op != op) continue;
            entry.cancelled = true;
            if (entry.state == .issued) _ = CancelIoEx(as_handle(entry.socket), &entry.overlapped);
            return;
        }
    }

    fn release(self: *Iocp, entry: *Pending) *io.Operation {
        const op = entry.op;
        entry.state = .free;
        self.pending_count -= 1;
        return op;
    }

    fn has_other(self: *const Iocp, entry: *const Pending) bool {
        for (&self.pending) |*other| {
            if (other == entry or other.state != .issued) continue;
            if (other.socket == entry.socket) return true;
        }
        return false;
    }

    fn run_deferred(self: *Iocp) bool {
        var completed = false;
        const serial = self.next_serial;
        const now_ms = self.now();
        for (&self.pending) |*entry| {
            if (entry.state == .free or entry.state == .issued or entry.serial > serial) continue;
            const result: io.Result = switch (entry.state) {
                .immediate => if (entry.cancelled) .{ .failed = error.Cancelled } else entry.result,
                .timer => blk: {
                    if (entry.cancelled) break :blk .{ .failed = error.Cancelled };
                    if (now_ms < entry.op.request.timeout) continue;
                    break :blk .{ .timed_out = {} };
                },
                .closing => blk: {
                    if (entry.cancelled) break :blk .{ .failed = error.Cancelled };
                    if (self.has_other(entry)) continue;
                    self.forget_listener(entry.socket);
                    break :blk if (closesocket(entry.socket) == 0) .{ .closed = {} } else .{ .failed = last_failure() };
                },
                .free, .issued => unreachable,
            };
            const op = self.release(entry);
            op.complete(result);
            completed = true;
        }
        return completed;
    }

    fn finish(self: *Iocp, entry: *Pending, bytes: u32) void {
        var result: io.Result = undefined;
        var transferred: u32 = 0;
        var flags: u32 = 0;
        const ok = WSAGetOverlappedResult(entry.socket, &entry.overlapped, &transferred, 0, &flags) != 0;
        if (ok) {
            result = self.succeed(entry, bytes);
        } else {
            const err: io.Failure = if (entry.cancelled) error.Cancelled else last_failure();
            if (entry.created != invalid_socket) _ = closesocket(entry.created);
            result = .{ .failed = err };
        }
        const op = self.release(entry);
        op.complete(result);
    }

    fn succeed(self: *Iocp, entry: *Pending, bytes: u32) io.Result {
        switch (entry.op.request) {
            .recv => return .{ .received = bytes },
            .send => return .{ .sent = bytes },
            .accept => {
                const created = entry.created;
                const listener = entry.socket;
                if (setsockopt(created, SOL_SOCKET, SO_UPDATE_ACCEPT_CONTEXT, &listener, @sizeOf(SOCKET)) != 0 or !self.join_port(created) or !configure(created)) {
                    _ = closesocket(created);
                    return .{ .failed = error.Unexpected };
                }
                return .{ .accepted = socket(created) };
            },
            .connect => {
                const created = entry.created;
                if (entry.cancelled or setsockopt(created, SOL_SOCKET, SO_UPDATE_CONNECT_CONTEXT, &@as(c_int, 0), 0) != 0) {
                    _ = closesocket(created);
                    return .{ .failed = if (entry.cancelled) error.Cancelled else error.Unexpected };
                }
                return .{ .connected = socket(created) };
            },
            .timeout, .close => unreachable,
        }
    }

    pub fn poll(self: *Iocp, max_wait_ms: u32) !void {
        const completed = self.run_deferred();
        var wait_ms: u64 = if (completed) 0 else max_wait_ms;
        const now_ms = self.now();
        for (&self.pending) |*entry| {
            if (entry.state == .timer) wait_ms = @min(wait_ms, entry.op.request.timeout -| now_ms);
        }
        var entries: [64]OVERLAPPED_ENTRY = undefined;
        var removed: u32 = 0;
        const wait: u32 = @intCast(@min(wait_ms, std.math.maxInt(u32) - 1));
        if (GetQueuedCompletionStatusEx(self.port, &entries, entries.len, &removed, wait, 0) == 0) {
            if (GetLastError() == wait_timeout) return;
            return error.IocpWaitFailed;
        }
        for (entries[0..removed]) |packet| {
            const overlapped = packet.overlapped orelse continue;
            const entry: *Pending = @fieldParentPtr("overlapped", overlapped);
            tiger.assert(entry.state == .issued);
            self.finish(entry, packet.bytes);
        }
    }
};
