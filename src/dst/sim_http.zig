const std = @import("std");
const tiger = @import("../tiger.zig");
const wire = @import("../net/http_wire.zig");
const record_ring = @import("../net/record_ring.zig");

const Prng = std.Random.DefaultPrng;

pub fn run(gpa: std.mem.Allocator, seed: u64) !void {
    var prng = Prng.init(seed);
    const random = prng.random();
    try chunked_rounds(gpa, random);
    try head_rounds(gpa, random);
    try websocket_rounds(gpa, random);
    try ring_rounds(gpa, random);
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

const Model = struct {
    const Entry = struct { pos: u64, seq: u64, len: usize };
    entries: std.ArrayList(Entry) = .empty,
    first: usize = 0,
    head: u64 = 0,
};

fn pattern(seq: u64, i: usize) u8 {
    return @truncate((seq *% 0x9e3779b97f4a7c15 >> 7) +% i *% 31);
}

fn ring_rounds(gpa: std.mem.Allocator, random: std.Random) !void {
    const Ring = record_ring.RecordRing(4096);
    const storage = try gpa.alloc(u8, Ring.bytes);
    defer gpa.free(storage);
    var ring = Ring.init(storage);
    var model: Model = .{};
    defer model.entries.deinit(gpa);
    var seq: u64 = 0;
    var readers: [4]u64 = @splat(0);
    var scratch: [2048]u8 = undefined;
    for (0..400) |_| {
        switch (random.uintLessThan(u8, 3)) {
            0 => {
                const len = if (random.uintLessThan(u8, 10) == 0) 0 else random.uintLessThan(usize, Ring.bytes / 2 - record_ring.header_len);
                const payload = scratch[0..@min(len, scratch.len)];
                seq += 1;
                for (payload, 0..) |*b, i| b.* = pattern(seq, i);
                var big: std.ArrayList(u8) = .empty;
                defer big.deinit(gpa);
                const data: []const u8 = if (len <= scratch.len) payload else blk: {
                    try big.resize(gpa, len);
                    for (big.items, 0..) |*b, i| b.* = pattern(seq, i);
                    break :blk big.items;
                };
                ring.append(seq, data);
                try model.entries.append(gpa, .{ .pos = model.head, .seq = seq, .len = data.len });
                model.head += record_ring.header_len + data.len;
                while (model.head - model.entries.items[model.first].pos > Ring.bytes) model.first += 1;
                if (ring.head_pos() != model.head) return error.RingHeadMismatch;
                if (ring.tail.load(.acquire) != model.entries.items[model.first].pos) return error.RingTailMismatch;
            },
            1 => {
                const r = random.uintLessThan(usize, readers.len);
                const oldest = model.entries.items[model.first..];
                switch (ring.peek(readers[r])) {
                    .empty => if (readers[r] != model.head) return error.RingEmptyWrong,
                    .lagged => if (oldest.len == 0 or readers[r] >= oldest[0].pos) return error.RingLaggedWrong,
                    .record => |rec| {
                        var found: ?Model.Entry = null;
                        for (oldest) |e| {
                            if (e.pos == readers[r]) found = e;
                        }
                        const e = found orelse return error.RingRecordUnknown;
                        if (e.seq != rec.seq or e.len != rec.len) return error.RingRecordMismatch;
                        var offset: usize = 0;
                        while (offset < e.len) {
                            const n = @min(e.len - offset, scratch.len);
                            if (!ring.read(readers[r], offset, scratch[0..n])) return error.RingReadInvalid;
                            for (scratch[0..n], 0..) |b, i| if (b != pattern(e.seq, offset + i)) return error.RingReadCorrupt;
                            offset += n;
                        }
                        readers[r] = rec.next;
                    },
                }
            },
            else => {
                const want = if (seq == 0) 0 else random.uintLessThan(u64, seq + 1);
                const got = ring.find_after(want);
                const oldest = model.entries.items[model.first..];
                var expect: ?u64 = model.head;
                if (oldest.len > 0 and oldest[0].seq > want + 1) {
                    expect = null;
                } else for (oldest) |e| {
                    if (e.seq > want) {
                        expect = e.pos;
                        break;
                    }
                }
                if (got != expect) return error.RingFindAfterWrong;
                if (got) |pos| readers[random.uintLessThan(usize, readers.len)] = pos;
            },
        }
        for (&readers) |*pos| {
            const oldest = model.entries.items[model.first..];
            if (oldest.len > 0 and pos.* < oldest[0].pos and random.boolean()) pos.* = oldest[0].pos;
        }
    }
}
