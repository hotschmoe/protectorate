const std = @import("std");
const tiger = @import("../tiger.zig");

pub const Socket = enum(u64) { _ };
pub const Endpoint = struct {
    address: [16]u8,
    port: u16,
    family: enum { ipv4, ipv6 },
};

pub const Failure = error{ Cancelled, Refused, TimedOut, Reset, Closed, Unreachable, ResourceExhausted, Unexpected };
pub const Result = union(enum) {
    accepted: Socket,
    connected: Socket,
    received: usize,
    sent: usize,
    closed: void,
    timed_out: void,
    failed: Failure,
};

pub const Operation = struct {
    context: *anyopaque,
    callback: *const fn (*anyopaque, *Operation, Result) void,
    state: enum { idle, submitted } = .idle,
    request: Request = undefined,

    pub const Request = union(enum) {
        accept: Socket,
        connect: Endpoint,
        recv: struct { socket: Socket, buffer: []u8 },
        send: struct { socket: Socket, buffer: []const u8 },
        close: Socket,
        timeout: u64,
    };

    pub fn begin(self: *Operation, request: Request) void {
        tiger.assert(self.state == .idle);
        self.request = request;
        self.state = .submitted;
    }

    pub fn complete(self: *Operation, result: Result) void {
        tiger.assert(self.state == .submitted);
        self.state = .idle;
        self.callback(self.context, self, result);
    }
};

pub fn assert_contract(comptime IO: type) void {
    comptime {
        _ = IO.submit;
        _ = IO.cancel;
        _ = IO.now;
        _ = IO.random;
    }
}
