const std = @import("std");
const tiger = @import("../tiger.zig");
const wire = @import("../net/http_wire.zig");
const record_ring = @import("../net/record_ring.zig");
const ws_out = @import("../net/ws_out.zig");

const Prng = std.Random.DefaultPrng;

pub fn run(gpa: std.mem.Allocator, seed: u64) !void {
    var prng = Prng.init(seed);
    const random = prng.random();
    try chunked_rounds(gpa, random);
    try head_rounds(gpa, random);
    try websocket_rounds(gpa, random);
    try ring_rounds(gpa, random);
    try out_rounds(gpa, random);
    try strict_rounds(gpa);
}

fn fill_random(random: std.Random, buf: []u8) void {
    random.bytes(buf);
}

fn chunked_rounds(gpa: std.mem.Allocator, random: std.Random) !void {
    for (0..24) |_| {
        const body = try gpa.alloc(u8, random.uintLessThan(usize, 6000));
        defer gpa.free(body);
        fill_random(random, body);

        var encoded: std.ArrayList(u8) = .empty;
        defer encoded.deinit(gpa);
        var at: usize = 0;
        while (at < body.len) {
            const n = @min(body.len - at, 1 + random.uintLessThan(usize, 700));
            try encoded.print(gpa, "{x}", .{n});
            if (random.boolean()) try encoded.appendSlice(gpa, ";ext=1");
            try encoded.appendSlice(gpa, "\r\n");
            try encoded.appendSlice(gpa, body[at..][0..n]);
            try encoded.appendSlice(gpa, "\r\n");
            at += n;
        }
        try encoded.appendSlice(gpa, "0\r\n");
        if (random.boolean()) try encoded.appendSlice(gpa, "x-trailer: 1\r\n");
        try encoded.appendSlice(gpa, "\r\n");
        const message_len = encoded.items.len;
        const tail = random.uintLessThan(usize, 40);
        for (0..tail) |_| try encoded.append(gpa, random.int(u8));

        var decoder: wire.Chunked = .{};
        var out: std.ArrayList(u8) = .empty;
        defer out.deinit(gpa);
        var window = try gpa.alloc(u8, 1 + random.uintLessThan(usize, 300));
        defer gpa.free(window);
        var consumed_total: usize = 0;
        var done = false;
        var guard: usize = 0;
        while (!done) : (guard += 1) {
            tiger.assert(guard < 200_000);
            const piece = @min(encoded.items.len - consumed_total, 1 + random.uintLessThan(usize, 90));
            if (piece == 0) return error.ChunkedStalled;
            const step = decoder.feed(encoded.items[consumed_total..][0..piece], window) catch return error.ChunkedRejectedValidInput;
            tiger.assert(step.consumed <= piece and step.written <= window.len);
            try out.appendSlice(gpa, window[0..step.written]);
            consumed_total += step.consumed;
            switch (step.status) {
                .done => done = true,
                .more => tiger.assert(step.consumed == piece),
                .full => {},
            }
        }
        if (consumed_total != message_len) return error.ChunkedConsumedWrongLength;
        if (!std.mem.eql(u8, out.items, body)) return error.ChunkedBodyMismatch;

        const bad = try gpa.dupe(u8, encoded.items[0..message_len]);
        defer gpa.free(bad);
        bad[random.uintLessThan(usize, bad.len)] = random.int(u8);
        var broken: wire.Chunked = .{};
        var scratch: [128]u8 = undefined;
        var pos: usize = 0;
        while (pos < bad.len) {
            const piece = @min(bad.len - pos, 1 + random.uintLessThan(usize, 50));
            const step = broken.feed(bad[pos..][0..piece], &scratch) catch break;
            tiger.assert(step.consumed <= piece and step.written <= scratch.len);
            pos += step.consumed;
            if (step.status == .done) break;
            if (step.consumed == 0 and step.status != .full) break;
        }
    }
}

const header_names = [_][]const u8{ "Host", "Origin", "Authorization", "Accept-Encoding", "Connection", "Upgrade", "Content-Type", "User-Agent", "X-Forwarded-For" };

fn head_rounds(gpa: std.mem.Allocator, random: std.Random) !void {
    for (0..40) |_| {
        var head: std.ArrayList(u8) = .empty;
        defer head.deinit(gpa);
        const methods = [_][]const u8{ "GET", "POST", "HEAD", "OPTIONS", "PUT" };
        const method = methods[random.uintLessThan(usize, methods.len)];
        const length: u64 = random.uintLessThan(u64, 1 << 22);
        const chunked = random.boolean();
        const token_len = 1 + random.uintLessThan(usize, 80);
        const token = try gpa.alloc(u8, token_len);
        defer gpa.free(token);
        for (token) |*c| c.* = "abcdef0123456789"[random.uintLessThan(usize, 16)];
        try head.print(gpa, "{s} /api/Thing?x=1 HTTP/1.1\r\n", .{method});
        try head.print(gpa, "{s}: example.test:8080\r\n", .{if (random.boolean()) "Host" else "hOsT"});
        try head.print(gpa, "AUTHORIZATION:  Bearer {s}  \r\n", .{token});
        if (chunked) {
            try head.appendSlice(gpa, "Transfer-Encoding: chunked\r\n");
        } else {
            try head.print(gpa, "content-length: {d}\r\n", .{length});
        }
        for (0..random.uintLessThan(usize, 12)) |_| {
            const name = header_names[random.uintLessThan(usize, header_names.len)];
            if (std.mem.eql(u8, name, "Host") or std.mem.eql(u8, name, "Authorization") or std.mem.eql(u8, name, "Origin") or std.mem.eql(u8, name, "Connection") or std.mem.eql(u8, name, "Upgrade")) continue;
            try head.print(gpa, "{s}: v{d}\r\n", .{ name, random.int(u16) });
        }
        try head.appendSlice(gpa, "\r\n");

        var upto: usize = 0;
        var found: ?usize = null;
        var scanned: usize = 0;
        while (found == null) {
            upto = @min(head.items.len, upto + 1 + random.uintLessThan(usize, 60));
            found = wire.find_head_end(head.items[0..upto], scanned);
            scanned = upto;
            if (found) |end| if (end != head.items.len) return error.HeadEndEarly;
            if (upto == head.items.len and found == null) return error.HeadEndMissed;
        }
        const req = wire.parse_head(head.items) catch return error.HeadRejectedValidInput;
        if (req.chunked != chunked) return error.HeadChunkedMismatch;
        if (!chunked and req.content_length != length) return error.HeadLengthMismatch;
        if (!std.mem.eql(u8, req.authorization orelse return error.HeadTokenMissing, token)) return error.HeadTokenMismatch;
        if (!std.mem.eql(u8, req.host orelse return error.HeadHostMissing, "example.test:8080")) return error.HeadHostMismatch;

        const mutated = try gpa.dupe(u8, head.items);
        defer gpa.free(mutated);
        for (0..1 + random.uintLessThan(usize, 4)) |_| mutated[random.uintLessThan(usize, mutated.len)] = random.int(u8);
        if (wire.find_head_end(mutated, 0)) |end| {
            if (wire.parse_head(mutated[0..end])) |parsed| {
                const lo = @intFromPtr(mutated.ptr);
                const hi = lo + mutated.len;
                const t = @intFromPtr(parsed.target.ptr);
                tiger.assert(t >= lo and t + parsed.target.len <= hi);
                if (parsed.authorization) |a| tiger.assert(@intFromPtr(a.ptr) >= lo and @intFromPtr(a.ptr) + a.len <= hi);
            } else |_| {}
        }
    }
}

fn mask_frame(gpa: std.mem.Allocator, out: *std.ArrayList(u8), random: std.Random, opcode: u4, fin: bool, payload: []const u8) !void {
    try out.append(gpa, (if (fin) @as(u8, 0x80) else 0) | opcode);
    if (payload.len < 126) {
        try out.append(gpa, 0x80 | @as(u8, @intCast(payload.len)));
    } else if (payload.len <= std.math.maxInt(u16)) {
        try out.append(gpa, 0x80 | 126);
        try out.appendSlice(gpa, &std.mem.toBytes(std.mem.nativeToBig(u16, @intCast(payload.len))));
    } else {
        try out.append(gpa, 0x80 | 127);
        try out.appendSlice(gpa, &std.mem.toBytes(std.mem.nativeToBig(u64, payload.len)));
    }
    var mask: [4]u8 = undefined;
    random.bytes(&mask);
    try out.appendSlice(gpa, &mask);
    for (payload, 0..) |b, i| try out.append(gpa, b ^ mask[i & 3]);
}

fn websocket_rounds(gpa: std.mem.Allocator, random: std.Random) !void {
    for (0..12) |_| {
        var stream: std.ArrayList(u8) = .empty;
        defer stream.deinit(gpa);
        var expected: std.ArrayList(struct { opcode: u4, payload: []u8 }) = .empty;
        defer {
            for (expected.items) |e| gpa.free(e.payload);
            expected.deinit(gpa);
        }
        for (0..1 + random.uintLessThan(usize, 8)) |_| {
            const kinds = [_]u4{ 1, 2, 8, 9, 10, 0 };
            const opcode = kinds[random.uintLessThan(usize, kinds.len)];
            const limit: usize = if (opcode >= 8) 125 else 70_000;
            const payload = try gpa.alloc(u8, random.uintLessThan(usize, limit));
            fill_random(random, payload);
            try mask_frame(gpa, &stream, random, opcode, true, payload);
            try expected.append(gpa, .{ .opcode = opcode, .payload = payload });
        }
        var at: usize = 0;
        var frame_index: usize = 0;
        var pending: std.ArrayList(u8) = .empty;
        defer pending.deinit(gpa);
        var left: u64 = 0;
        var header: ?wire.WsHeader = null;
        var offset: u64 = 0;
        var decoded: std.ArrayList(u8) = .empty;
        defer decoded.deinit(gpa);
        while (at < stream.items.len or pending.items.len > 0) {
            const piece = @min(stream.items.len - at, 1 + random.uintLessThan(usize, 200));
            try pending.appendSlice(gpa, stream.items[at..][0..piece]);
            at += piece;
            var pos: usize = 0;
            while (pos < pending.items.len) {
                if (header == null) {
                    const parsed = wire.ws_parse(pending.items[pos..]) catch return error.WsRejectedValidFrame;
                    const h = parsed orelse break;
                    header = h;
                    pos += h.len;
                    left = h.payload;
                    offset = 0;
                    decoded.clearRetainingCapacity();
                } else {
                    const n: usize = @intCast(@min(@as(u64, pending.items.len - pos), left));
                    const start = decoded.items.len;
                    try decoded.appendSlice(gpa, pending.items[pos..][0..n]);
                    wire.ws_unmask(decoded.items[start..], header.?.mask, offset);
                    offset += n;
                    left -= n;
                    pos += n;
                }
                if (header != null and left == 0) {
                    const want = expected.items[frame_index];
                    if (@backingInt(header.?.opcode) != want.opcode or !std.mem.eql(u8, decoded.items, want.payload)) return error.WsFrameMismatch;
                    frame_index += 1;
                    header = null;
                }
            }
            const keep = pending.items.len - pos;
            std.mem.copyForwards(u8, pending.items[0..keep], pending.items[pos..]);
            pending.items.len = keep;
            if (at == stream.items.len and keep == 0) break;
            if (piece == 0 and keep > 0) return error.WsStalled;
        }
        if (frame_index != expected.items.len) return error.WsFramesMissing;

        const unmasked = [_]u8{ 0x81, 0x02, 'h', 'i' };
        if (wire.ws_parse(&unmasked)) |_| return error.WsAcceptedUnmasked else |_| {}
        const reserved = [_]u8{ 0xc1, 0x80, 0, 0, 0, 0 };
        if (wire.ws_parse(&reserved)) |_| return error.WsAcceptedReserved else |_| {}
        const long_ping = [_]u8{ 0x89, 0xfe, 0x00, 0x7e, 0, 0, 0, 0 };
        if (wire.ws_parse(&long_ping)) |_| return error.WsAcceptedLongControl else |_| {}

        var out: [10]u8 = undefined;
        const len = random.uintLessThan(u64, 1 << 20);
        const h = wire.ws_server_header(&out, .text, len);
        const decoded_len: u64 = switch (h[1]) {
            126 => std.mem.readInt(u16, h[2..4], .big),
            127 => std.mem.readInt(u64, h[2..10], .big),
            else => h[1],
        };
        if (decoded_len != len or h[0] != 0x81) return error.WsServerHeaderMismatch;
    }
}

fn pattern(seq: u64, i: usize) u8 {
    return @truncate((seq *% 0x9e3779b97f4a7c15 >> 7) +% i *% 31);
}

const Interleave = struct {
    const Hook = struct {
        ctx: *Interleave,
        pub fn point(self: *Hook) void {
            self.ctx.point();
        }
    };
    const Ring = record_ring.RecordRing(4096, Hook);
    const Entry = struct { pos: u64, seq: u64, len: usize };
    const Side = enum { none, writer, reader };

    gpa: std.mem.Allocator,
    random: std.Random,
    ring: Ring = undefined,
    entries: std.ArrayList(Entry) = .empty,
    head: u64 = 0,
    seq: u64 = 0,
    depth: u8 = 0,
    side: Side = .none,
    failure: ?anyerror = null,
    cursor: [3]u64 = @splat(0),
    write_buf: [2048]u8 = undefined,
    read_buf: [2048]u8 = undefined,
    preempted: u64 = 0,
    overwritten: u64 = 0,

    fn fail(self: *Interleave, err: anyerror) void {
        if (self.failure == null) self.failure = err;
    }

    /// Runs at every word access of the ring. The other side of the ring runs here a part of the time, which is
    /// the writer overwriting what a reader is copying, or a reader looking while a record is being written.
    fn point(self: *Interleave) void {
        if (self.depth != 0 or self.failure != null or self.random.uintLessThan(u8, 6) != 0) return;
        self.depth = 1;
        defer self.depth = 0;
        self.preempted += 1;
        const was = self.side;
        defer self.side = was;
        switch (was) {
            .writer => self.reader_op(self.random.uintLessThan(usize, self.cursor.len)),
            .reader => self.append(),
            .none => {},
        }
    }

    fn entry_at(self: *const Interleave, pos: u64) ?Entry {
        for (self.entries.items) |e| if (e.pos == pos) return e;
        return null;
    }

    fn append(self: *Interleave) void {
        self.side = .writer;
        defer self.side = .none;
        const len = if (self.random.uintLessThan(u8, 12) == 0) 0 else self.random.uintLessThan(usize, 2000);
        self.seq += 1;
        for (self.write_buf[0..len], 0..) |*b, i| b.* = pattern(self.seq, i);
        const pos = self.head;
        self.ring.append(self.seq, self.write_buf[0..len]);
        self.entries.append(self.gpa, .{ .pos = pos, .seq = self.seq, .len = len }) catch return self.fail(error.OutOfMemory);
        self.head += record_ring.header_len + ((len + 7) & ~@as(usize, 7));
    }

    fn reader_op(self: *Interleave, r: usize) void {
        const prior = self.side;
        self.side = .reader;
        defer self.side = prior;
        if (self.random.boolean()) {
            switch (self.ring.peek(self.cursor[r])) {
                .empty => if (self.cursor[r] > self.head) self.fail(error.RingEmptyPastHead),
                .lagged => {
                    if (self.cursor[r] >= self.ring.tail.load(.acquire)) self.fail(error.RingLaggedButKept);
                    self.overwritten += 1;
                    self.cursor[r] = self.ring.tail.load(.acquire);
                },
                .record => |rec| {
                    const e = self.entry_at(self.cursor[r]) orelse return self.fail(error.RingRecordUnknown);
                    if (e.seq != rec.seq or e.len != rec.len) return self.fail(error.RingRecordMismatch);
                    var offset: usize = 0;
                    while (offset < e.len) {
                        const n = @min(e.len - offset, 1 + self.random.uintLessThan(usize, 300));
                        if (!self.ring.read(self.cursor[r], offset, self.read_buf[0..n])) {
                            self.overwritten += 1;
                            self.cursor[r] = self.ring.tail.load(.acquire);
                            return;
                        }
                        for (self.read_buf[0..n], 0..) |b, i| if (b != pattern(e.seq, offset + i)) return self.fail(error.RingReadCorrupt);
                        offset += n;
                    }
                    self.cursor[r] = rec.next;
                },
            }
        } else {
            const want = if (self.seq == 0) 0 else self.random.uintLessThan(u64, self.seq + 1);
            if (self.ring.find_after(want)) |pos| {
                if (self.entry_at(pos)) |e| {
                    if (e.seq != want + 1) return self.fail(error.RingFindAfterWrong);
                } else if (pos > self.head) return self.fail(error.RingFindAfterPastHead);
                self.cursor[r] = pos;
            }
        }
    }
};

fn ring_rounds(gpa: std.mem.Allocator, random: std.Random) !void {
    const storage = try gpa.alloc(std.atomic.Value(u64), Interleave.Ring.words);
    defer gpa.free(storage);
    var sim: Interleave = .{ .gpa = gpa, .random = random };
    defer sim.entries.deinit(gpa);
    sim.ring = Interleave.Ring.init(storage, .{ .ctx = &sim });
    for (0..500) |_| {
        if (random.boolean()) {
            sim.append();
        } else {
            sim.side = .reader;
            sim.reader_op(random.uintLessThan(usize, sim.cursor.len));
            sim.side = .none;
        }
        if (sim.failure) |err| return err;
        if (sim.ring.head_pos() != sim.head) return error.RingHeadMismatch;
    }
    if (sim.preempted == 0) return error.RingNeverInterleaved;
}

const Wire = struct {
    const Ring = record_ring.RecordRing(8192, record_ring.NoHook);
    ring: *Ring,
    seq: u64 = 0,
    expect_seq: u64 = 1,
    frame_open: bool = false,
    data: bool = false,
    left: u64 = 0,
    at: usize = 0,
    opcode: u8 = 0,
    hdr: [10]u8 = undefined,
    hdr_len: usize = 0,
    closed: bool = false,
    data_frames: usize = 0,
    controls: usize = 0,

    /// Parses server frames out of the bytes as they are sent; payload of a data frame must be the record's bytes.
    fn feed(self: *Wire, bytes: []const u8) !void {
        var i: usize = 0;
        while (i < bytes.len) {
            if (self.closed) return error.WsBytesAfterClose;
            if (!self.frame_open) {
                self.hdr[self.hdr_len] = bytes[i];
                self.hdr_len += 1;
                i += 1;
                if (self.hdr_len < 2) continue;
                const code = self.hdr[1] & 0x7f;
                const need: usize = if (code == 126) 4 else if (code == 127) 10 else 2;
                if (self.hdr_len < need) continue;
                self.opcode = self.hdr[0] & 0x0f;
                if (self.hdr[0] & 0x80 == 0 or self.hdr[1] & 0x80 != 0) return error.WsBadServerHeader;
                self.left = if (code == 126) std.mem.readInt(u16, self.hdr[2..4], .big) else if (code == 127) std.mem.readInt(u64, self.hdr[2..10], .big) else code;
                self.hdr_len = 0;
                self.frame_open = true;
                self.at = 0;
                self.data = self.opcode == 1;
                if (!self.data and self.opcode != 8 and self.opcode != 9 and self.opcode != 10) return error.WsBadOpcode;
            } else {
                const n: usize = @intCast(@min(@as(u64, bytes.len - i), self.left));
                if (self.data) {
                    for (bytes[i..][0..n], 0..) |b, k| if (b != pattern(self.expect_seq, self.at + k)) return error.WsDataFrameInterrupted;
                }
                self.at += n;
                self.left -= n;
                i += n;
            }
            if (self.frame_open and self.left == 0) {
                self.frame_open = false;
                if (self.data) {
                    self.data_frames += 1;
                    self.expect_seq += 1;
                } else {
                    self.controls += 1;
                    if (self.opcode == 8) self.closed = true;
                }
            }
        }
    }
};

fn out_rounds(gpa: std.mem.Allocator, random: std.Random) !void {
    const Out = ws_out.Out(Wire.Ring);
    const storage = try gpa.alloc(std.atomic.Value(u64), Wire.Ring.words);
    defer gpa.free(storage);
    var ring = Wire.Ring.init(storage, .{});
    var wire_state: Wire = .{ .ring = &ring };
    const sbuf = try gpa.alloc(u8, 160 + random.uintLessThan(usize, 1400));
    defer gpa.free(sbuf);
    const payload = try gpa.alloc(u8, 4096);
    defer gpa.free(payload);
    var out: Out = .{ .ping_at = 40 };
    var pending: []const u8 = &.{};
    var now: u64 = 0;
    var dead = false;
    var close_requested = false;
    for (0..1500) |_| {
        now += random.uintLessThan(u64, 20);
        switch (random.uintLessThan(u8, 10)) {
            0, 1 => {
                const len = 1 + random.uintLessThan(usize, 3000);
                wire_state.seq += 1;
                for (payload[0..len], 0..) |*b, i| b.* = pattern(wire_state.seq, i);
                ring.append(wire_state.seq, payload[0..len]);
            },
            2 => {
                var ping: [wire.control_payload_max]u8 = undefined;
                const n = random.uintLessThan(usize, ping.len + 1);
                random.bytes(ping[0..n]);
                out.queue_pong(ping[0..n]);
            },
            3 => if (!close_requested and random.uintLessThan(u8, 40) == 0) {
                close_requested = true;
                out.queue_close(if (random.boolean()) 1000 else null);
            },
            else => if (!dead) {
                if (pending.len == 0) {
                    switch (out.stage(&ring, sbuf, now, 50)) {
                        .staged => |n| pending = sbuf[0..n],
                        .close_staged => |n| pending = sbuf[0..n],
                        .idle => {},
                        .resync => {
                            out.record = null;
                            out.cursor = ring.head_pos();
                            wire_state.expect_seq = wire_state.seq + 1;
                        },
                        .abort => dead = true,
                    }
                } else {
                    const n = 1 + random.uintLessThan(usize, pending.len);
                    try wire_state.feed(pending[0..n]);
                    pending = pending[n..];
                    if (wire_state.closed) dead = true;
                }
            },
        }
    }
    if (!dead and wire_state.seq > 8 and wire_state.data_frames == 0) return error.WsNothingSent;
}

fn strict_rounds(gpa: std.mem.Allocator) !void {
    _ = gpa;
    const dup_cl = "POST /api/X HTTP/1.1\r\nContent-Length: 2\r\nContent-Length: 2\r\n\r\n";
    if (wire.parse_head(dup_cl)) |_| return error.DuplicateLengthAccepted else |_| {}
    const dup_te = "POST /api/X HTTP/1.1\r\nTransfer-Encoding: chunked\r\nTransfer-Encoding: chunked\r\n\r\n";
    if (wire.parse_head(dup_te)) |_| return error.DuplicateEncodingAccepted else |_| {}
    const both = "POST /api/X HTTP/1.1\r\nTransfer-Encoding: chunked\r\nContent-Length: 2\r\n\r\n";
    if (wire.parse_head(both)) |_| return error.LengthAndEncodingAccepted else |_| {}
    const cases = [_][]const u8{
        "5;a\nhello\r\n0\r\n\r\n",
        "5\nhello\r\n0\r\n\r\n",
        "5\r\nhello\n0\r\n\r\n",
        "0\r\nx: 1\n\r\n",
        "0\r\n\n",
        "0\r\nx: 1\r\n\n",
    };
    for (cases) |case| {
        var decoder: wire.Chunked = .{};
        var scratch: [32]u8 = undefined;
        const step = decoder.feed(case, &scratch) catch continue;
        if (step.status == .done) return error.BareLineFeedAccepted;
    }
}
