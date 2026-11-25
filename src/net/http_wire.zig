const std = @import("std");
const tiger = @import("../tiger.zig");

pub const head_max: usize = 16 * 1024;
pub const control_payload_max: usize = 125;
const trailer_max: usize = 2048;
const chunk_line_max: usize = 256;

pub const Method = enum { get, head, post, options, other };

pub const Request = struct {
    method: Method,
    target: []const u8,
    http11: bool,
    content_length: ?u64 = null,
    chunked: bool = false,
    close: bool = false,
    keep_alive: bool = false,
    expect_continue: bool = false,
    upgrade_websocket: bool = false,
    connection_upgrade: bool = false,
    authorization: ?[]const u8 = null,
    accept_gzip: bool = false,
    origin: ?[]const u8 = null,
    host: ?[]const u8 = null,
    duplicate_origin_or_host: bool = false,
    ws_key: ?[]const u8 = null,
    ws_version: ?[]const u8 = null,

    pub fn persistent(self: *const Request) bool {
        if (self.close) return false;
        return self.http11 or self.keep_alive;
    }

    pub fn websocket_key(self: *const Request) ?[]const u8 {
        if (!self.upgrade_websocket or !self.connection_upgrade) return null;
        const version = self.ws_version orelse return null;
        if (!std.mem.eql(u8, version, "13")) return null;
        const key = self.ws_key orelse return null;
        return if (key.len == 0) null else key;
    }
};

pub const ParseError = error{ BadRequest, VersionNotSupported, NotImplemented };

/// Offset just past the blank line that ends a request head, searching only bytes at or after from - 3.
pub fn find_head_end(buf: []const u8, from: usize) ?usize {
    const start = from -| 3;
    const found = std.mem.find(u8, buf[start..], "\r\n\r\n") orelse return null;
    return start + found + 4;
}

fn is_token_char(c: u8) bool {
    return switch (c) {
        'a'...'z', 'A'...'Z', '0'...'9', '!', '#', '$', '%', '&', '\'', '*', '+', '-', '.', '^', '_', '`', '|', '~' => true,
        else => false,
    };
}

fn has_token(value: []const u8, token: []const u8) bool {
    var it = std.mem.splitScalar(u8, value, ',');
    while (it.next()) |part| {
        if (std.ascii.eqlIgnoreCase(std.mem.trim(u8, part, " \t"), token)) return true;
    }
    return false;
}

pub fn contains_ci(hay: []const u8, needle: []const u8) bool {
    if (needle.len == 0) return true;
    if (needle.len > hay.len) return false;
    var i: usize = 0;
    while (i + needle.len <= hay.len) : (i += 1) {
        if (std.ascii.eqlIgnoreCase(hay[i .. i + needle.len], needle)) return true;
    }
    return false;
}

fn parse_method(token: []const u8) ParseError!Method {
    if (token.len == 0) return error.BadRequest;
    for (token) |c| if (!is_token_char(c)) return error.BadRequest;
    if (std.mem.eql(u8, token, "GET")) return .get;
    if (std.mem.eql(u8, token, "HEAD")) return .head;
    if (std.mem.eql(u8, token, "POST")) return .post;
    if (std.mem.eql(u8, token, "OPTIONS")) return .options;
    return .other;
}

/// head holds the request line and the headers plus the closing blank line; the result borrows from it.
pub fn parse_head(head: []const u8) ParseError!Request {
    tiger.assert(std.mem.endsWith(u8, head, "\r\n\r\n"));
    var lines = std.mem.splitSequence(u8, head[0 .. head.len - 4], "\r\n");
    const request_line = lines.next().?;
    if (std.mem.indexOfAny(u8, request_line, "\r\n\x00") != null) return error.BadRequest;
    var parts = std.mem.splitScalar(u8, request_line, ' ');
    const method = try parse_method(parts.next() orelse return error.BadRequest);
    const target = parts.next() orelse return error.BadRequest;
    const version = parts.next() orelse return error.BadRequest;
    if (parts.next() != null) return error.BadRequest;
    if (target.len == 0 or target[0] != '/') return error.BadRequest;
    for (target) |c| if (c <= 0x20 or c == 0x7f) return error.BadRequest;
    var req: Request = .{ .method = method, .target = target, .http11 = false };
    if (std.mem.eql(u8, version, "HTTP/1.1")) {
        req.http11 = true;
    } else if (!std.mem.eql(u8, version, "HTTP/1.0")) {
        return if (std.mem.startsWith(u8, version, "HTTP/")) error.VersionNotSupported else error.BadRequest;
    }

    while (lines.next()) |line| {
        if (std.mem.indexOfAny(u8, line, "\r\n\x00") != null) return error.BadRequest;
        const colon = std.mem.findScalar(u8, line, ':') orelse return error.BadRequest;
        const name = line[0..colon];
        if (name.len == 0) return error.BadRequest;
        for (name) |c| if (!is_token_char(c)) return error.BadRequest;
        const value = std.mem.trim(u8, line[colon + 1 ..], " \t");
        if (std.ascii.eqlIgnoreCase(name, "content-length")) {
            if (value.len == 0 or value.len > 19) return error.BadRequest;
            for (value) |c| if (c < '0' or c > '9') return error.BadRequest;
            const parsed = std.fmt.parseInt(u64, value, 10) catch return error.BadRequest;
            if (req.content_length) |previous| if (previous != parsed) return error.BadRequest;
            req.content_length = parsed;
        } else if (std.ascii.eqlIgnoreCase(name, "transfer-encoding")) {
            if (!std.ascii.eqlIgnoreCase(value, "chunked")) return error.NotImplemented;
            req.chunked = true;
        } else if (std.ascii.eqlIgnoreCase(name, "connection")) {
            if (has_token(value, "close")) req.close = true;
            if (has_token(value, "keep-alive")) req.keep_alive = true;
            if (has_token(value, "upgrade")) req.connection_upgrade = true;
        } else if (std.ascii.eqlIgnoreCase(name, "upgrade")) {
            if (has_token(value, "websocket")) req.upgrade_websocket = true;
        } else if (std.ascii.eqlIgnoreCase(name, "expect")) {
            if (std.ascii.eqlIgnoreCase(value, "100-continue")) req.expect_continue = true;
        } else if (std.ascii.eqlIgnoreCase(name, "authorization")) {
            if (value.len >= 7 and std.ascii.eqlIgnoreCase(value[0..6], "bearer") and (value[6] == ' ' or value[6] == '\t')) {
                req.authorization = std.mem.trim(u8, value[7..], " \t");
            }
        } else if (std.ascii.eqlIgnoreCase(name, "accept-encoding")) {
            if (contains_ci(value, "gzip")) req.accept_gzip = true;
        } else if (std.ascii.eqlIgnoreCase(name, "origin")) {
            if (req.origin != null) req.duplicate_origin_or_host = true;
            req.origin = value;
        } else if (std.ascii.eqlIgnoreCase(name, "host")) {
            if (req.host != null) req.duplicate_origin_or_host = true;
            req.host = value;
        } else if (std.ascii.eqlIgnoreCase(name, "sec-websocket-key")) {
            req.ws_key = value;
        } else if (std.ascii.eqlIgnoreCase(name, "sec-websocket-version")) {
            req.ws_version = value;
        }
    }
    if (req.chunked and req.content_length != null) return error.BadRequest;
    return req;
}

/// Incremental decoder for a chunked request body. Chunk extensions and trailers are skipped within fixed bounds.
pub const Chunked = struct {
    pub const Error = error{BadChunk};
    pub const Status = enum { more, done, full };
    pub const Step = struct { consumed: usize, written: usize, status: Status };

    const State = enum { size, extension, size_lf, data, data_cr, data_lf, trailer, trailer_line, trailer_lf, end_lf, finished };

    state: State = .size,
    remaining: u64 = 0,
    size: u64 = 0,
    digits: u8 = 0,
    line: usize = 0,
    trailer: usize = 0,

    /// Decodes from input into out. status is full when data is waiting and out has no room.
    pub fn feed(self: *Chunked, input: []const u8, out: []u8) Error!Step {
        var i: usize = 0;
        var w: usize = 0;
        while (i < input.len) {
            switch (self.state) {
                .size => {
                    const c = input[i];
                    if (std.fmt.charToDigit(c, 16)) |digit| {
                        if (self.digits == 16 or self.size > (std.math.maxInt(u64) >> 4)) return error.BadChunk;
                        self.size = self.size * 16 + digit;
                        self.digits += 1;
                    } else |_| {
                        if (self.digits == 0) return error.BadChunk;
                        if (c == ';') {
                            self.state = .extension;
                            self.line = 0;
                        } else if (c == '\r') self.state = .size_lf else return error.BadChunk;
                    }
                    i += 1;
                },
                .extension => {
                    const c = input[i];
                    i += 1;
                    self.line += 1;
                    if (self.line > chunk_line_max) return error.BadChunk;
                    if (c == '\r') self.state = .size_lf;
                },
                .size_lf => {
                    if (input[i] != '\n') return error.BadChunk;
                    i += 1;
                    if (self.size == 0) {
                        self.state = .trailer;
                        self.trailer = 0;
                    } else {
                        self.remaining = self.size;
                        self.state = .data;
                    }
                },
                .data => {
                    const room = out.len - w;
                    if (room == 0) return .{ .consumed = i, .written = w, .status = .full };
                    const n = @min(@min(input.len - i, room), self.remaining);
                    @memcpy(out[w..][0..n], input[i..][0..n]);
                    i += n;
                    w += n;
                    self.remaining -= n;
                    if (self.remaining == 0) self.state = .data_cr;
                },
                .data_cr => {
                    if (input[i] != '\r') return error.BadChunk;
                    i += 1;
                    self.state = .data_lf;
                },
                .data_lf => {
                    if (input[i] != '\n') return error.BadChunk;
                    i += 1;
                    self.size = 0;
                    self.digits = 0;
                    self.state = .size;
                },
                .trailer => {
                    if (input[i] == '\r') {
                        i += 1;
                        self.state = .end_lf;
                    } else self.state = .trailer_line;
                },
                .trailer_line => {
                    const c = input[i];
                    i += 1;
                    self.trailer += 1;
                    if (self.trailer > trailer_max) return error.BadChunk;
                    if (c == '\r') self.state = .trailer_lf;
                },
                .trailer_lf => {
                    if (input[i] != '\n') return error.BadChunk;
                    i += 1;
                    self.state = .trailer;
                },
                .end_lf => {
                    if (input[i] != '\n') return error.BadChunk;
                    i += 1;
                    self.state = .finished;
                    return .{ .consumed = i, .written = w, .status = .done };
                },
                .finished => return .{ .consumed = i, .written = w, .status = .done },
            }
        }
        return .{ .consumed = i, .written = w, .status = if (self.state == .finished) .done else .more };
    }
};

pub fn ws_accept(key: []const u8, out: *[28]u8) void {
    var sha = std.crypto.hash.Sha1.init(.{});
    sha.update(key);
    sha.update("258EAFA5-E914-47DA-95CA-C5AB0DC85B11");
    var digest: [20]u8 = undefined;
    sha.final(&digest);
    _ = std.base64.standard.Encoder.encode(out, &digest);
}

pub const Opcode = enum(u4) { continuation = 0, text = 1, binary = 2, close = 8, ping = 9, pong = 10, _ };

pub const WsHeader = struct {
    len: usize,
    opcode: Opcode,
    fin: bool,
    mask: [4]u8,
    payload: u64,
};

pub const WsError = error{Protocol};

/// Parses a client frame header; null means more bytes are needed. Client frames must be masked.
pub fn ws_parse(buf: []const u8) WsError!?WsHeader {
    if (buf.len < 2) return null;
    const b0 = buf[0];
    const b1 = buf[1];
    if (b0 & 0x70 != 0) return error.Protocol;
    const opcode: Opcode = @fromBackingInt(@intCast(@as(u4, @truncate(b0))));
    switch (opcode) {
        .continuation, .text, .binary, .close, .ping, .pong => {},
        else => return error.Protocol,
    }
    const fin = b0 & 0x80 != 0;
    if (b1 & 0x80 == 0) return error.Protocol;
    var payload: u64 = b1 & 0x7f;
    var at: usize = 2;
    if (payload == 126) {
        if (buf.len < 4) return null;
        payload = std.mem.readInt(u16, buf[2..4], .big);
        at = 4;
    } else if (payload == 127) {
        if (buf.len < 10) return null;
        payload = std.mem.readInt(u64, buf[2..10], .big);
        if (payload >> 63 != 0) return error.Protocol;
        at = 10;
    }
    const control = @backingInt(opcode) >= 8;
    if (control and (!fin or payload > control_payload_max)) return error.Protocol;
    if (buf.len < at + 4) return null;
    return .{ .len = at + 4, .opcode = opcode, .fin = fin, .mask = buf[at..][0..4].*, .payload = payload };
}

pub fn ws_unmask(data: []u8, mask: [4]u8, offset: u64) void {
    for (data, 0..) |*byte, i| byte.* ^= mask[@intCast((offset + i) & 3)];
}

pub fn ws_server_header(out: *[10]u8, opcode: Opcode, payload: u64) []const u8 {
    out[0] = 0x80 | @as(u8, @backingInt(opcode));
    if (payload < 126) {
        out[1] = @intCast(payload);
        return out[0..2];
    }
    if (payload <= std.math.maxInt(u16)) {
        out[1] = 126;
        std.mem.writeInt(u16, out[2..4], @intCast(payload), .big);
        return out[0..4];
    }
    out[1] = 127;
    std.mem.writeInt(u64, out[2..10], payload, .big);
    return out[0..10];
}
