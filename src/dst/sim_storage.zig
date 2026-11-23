const std = @import("std");
const tiger = @import("../tiger.zig");
const jcodec = @import("../journal/codec.zig");
const ccodec = @import("../core/codec.zig");
const event = @import("../core/event.zig");
const hseg = @import("../history/segment.zig");
const extract = @import("../history/extract.zig");

pub const SimDisk = struct {
    synced: std.ArrayList(u8) = .empty,
    dirty: std.ArrayList(u8) = .empty,

    pub fn deinit(self: *SimDisk, gpa: std.mem.Allocator) void {
        self.synced.deinit(gpa);
        self.dirty.deinit(gpa);
    }

    pub fn append(self: *SimDisk, gpa: std.mem.Allocator, bytes: []const u8, sync_now: bool) !void {
        if (sync_now) {
            try self.synced.appendSlice(gpa, self.dirty.items);
            self.dirty.clearRetainingCapacity();
            try self.synced.appendSlice(gpa, bytes);
        } else {
            try self.dirty.appendSlice(gpa, bytes);
        }
    }

    pub fn crash_power_loss(self: *SimDisk, rng: std.Random) void {
        if (self.dirty.items.len == 0) return;
        const mode = rng.uintLessThan(u8, 3);
        switch (mode) {
            0 => self.dirty.clearRetainingCapacity(),
            1 => {
                const cut = rng.uintLessThan(usize, self.dirty.items.len);
                self.dirty.items.len = cut;
            },
            2 => {
                const idx = rng.uintLessThan(usize, self.dirty.items.len);
                self.dirty.items[idx] ^= (@as(u8, 1) << rng.int(u3));
            },
            else => unreachable,
        }
    }

    pub fn view_alloc(self: *const SimDisk, gpa: std.mem.Allocator) ![]u8 {
        var out = try gpa.alloc(u8, self.synced.items.len + self.dirty.items.len);
        @memcpy(out[0..self.synced.items.len], self.synced.items);
        @memcpy(out[self.synced.items.len..], self.dirty.items);
        return out;
    }
};

pub fn run_storage_sim(gpa: std.mem.Allocator, seed: u64, steps: usize) !void {
    var prng = std.Random.DefaultPrng.init(seed);
    const rng = prng.random();

    var disk = SimDisk{};
    defer disk.deinit(gpa);

    var synced_count: usize = 0;
    var ev_buf: std.ArrayList(u8) = .empty;
    defer ev_buf.deinit(gpa);
    var frame_buf: std.ArrayList(u8) = .empty;
    defer frame_buf.deinit(gpa);

    const n_frames = @max(@as(usize, 4), @min(steps / 10, 64));
    for (0..n_frames) |i| {
        ev_buf.clearRetainingCapacity();
        frame_buf.clearRetainingCapacity();

        const se = event.StampedEvent{
            .seq = i + 1,
            .ts_ms = @intCast(1_700_000_000_000 + i * 100),
            .event = if ((i % 2) == 0) .{ .stopped = .{ .sleeve_id = "sl-1" } } else .{ .tick = .{} },
        };
        try ccodec.encode_stamped(gpa, se, &ev_buf);
        try jcodec.encode_frame(gpa, ev_buf.items, &frame_buf);

        const do_sync = rng.boolean();
        try disk.append(gpa, frame_buf.items, do_sync);
        if (do_sync) {
            synced_count += 1;
        }
        if ((i % 5) == 4) {
            // Simulate sudden crash on unsynced buffer.
            disk.crash_power_loss(rng);
            const raw = try disk.view_alloc(gpa);
            defer gpa.free(raw);

            const dec = try jcodec.decode_frames(gpa, raw);
            defer gpa.free(dec.frames);
            tiger.assert(dec.frames.len >= synced_count);

            const rec = jcodec.recovery(dec.valid_len, dec.stop, false);
            switch (rec) {
                .clean => |n| tiger.assert(n == raw.len),
                .torn => |t| {
                    tiger.assert(t.prefix == dec.valid_len);
                    tiger.assert(t.prefix >= disk.synced.items.len or t.stop != .crc_mismatch);
                },
                .corrupt => tiger.assert(false),
            }

            // Simulate truncation recovery on torn tail.
            disk.synced.items.len = @min(disk.synced.items.len, dec.valid_len);
            if (dec.valid_len > disk.synced.items.len) {
                try disk.synced.appendSlice(gpa, raw[disk.synced.items.len..dec.valid_len]);
            }
            disk.dirty.clearRetainingCapacity();
            synced_count = dec.frames.len;

            var arena_st = std.heap.ArenaAllocator.init(gpa);
            defer arena_st.deinit();
            for (dec.frames) |payload| {
                _ = try ccodec.decode_stamped(arena_st.allocator(), payload);
            }
        }
    }

    // Also test mid-log corruption detection (valid header + intact frame after corrupted CRC frame).
    var two_frames: std.ArrayList(u8) = .empty;
    defer two_frames.deinit(gpa);
    try jcodec.encode_frame(gpa, "alpha-frame", &two_frames);
    const second_off = two_frames.items.len;
    try jcodec.encode_frame(gpa, "beta-frame-ok", &two_frames);
    two_frames.items[second_off - 2] ^= 0xff; // flip payload byte in frame 1 before frame 2
    const dec2 = try jcodec.decode_frames(gpa, two_frames.items);
    defer gpa.free(dec2.frames);
    tiger.assert(dec2.stop == .crc_mismatch);
    const rec2 = jcodec.recovery(dec2.valid_len, dec2.stop, true);
    switch (rec2) {
        .corrupt => {},
        else => tiger.assert(false),
    }

    // History segment roundtrip + corruption resilience.
    const docs = [_]hseg.SegmentInputDoc{
        .{
            .id = 10,
            .time_ms = 1_700_000_000_100,
            .offset = 0,
            .turn = 1,
            .session = "sess-a",
            .sleeve = "sl-root",
            .source = "/tmp/sess.jsonl",
            .tool = null,
            .harness = .pi_local,
            .role = .assistant,
            .files = &.{"src/core/apply.zig"},
            .text = "Implementing noiseHandshake and history BM25 index in protectorate.",
        },
        .{
            .id = 11,
            .time_ms = 1_700_000_000_200,
            .offset = 128,
            .turn = 2,
            .session = "sess-a",
            .sleeve = "sl-child",
            .source = "/tmp/sess.jsonl",
            .tool = "bash",
            .harness = .claude,
            .role = .tool_call,
            .files = &.{"build.zig"},
            .text = "zig build check-levels",
        },
    };
    const encoded = try hseg.encode_segment(gpa, &docs);
    defer gpa.free(encoded);

    var seg = try hseg.decode_segment(gpa, encoded);
    defer seg.deinit(gpa);
    tiger.assert(seg.docs.len == 2);

    // Mutate random positions inside a copy of segment and assert zero panics and proper rejection.
    const mutated = try gpa.dupe(u8, encoded);
    defer gpa.free(mutated);
    for (0..8) |_| {
        const flip_at = rng.uintLessThan(usize, mutated.len);
        const orig = mutated[flip_at];
        mutated[flip_at] ^= 0x5a;
        if (hseg.decode_segment(gpa, mutated)) |*bad_seg| {
            @constCast(bad_seg).deinit(gpa);
        } else |_| {}
        mutated[flip_at] = orig;
    }
    if (mutated.len > 4) {
        if (hseg.decode_segment(gpa, mutated[0 .. mutated.len - 3])) |*trunc_seg| {
            @constCast(trunc_seg).deinit(gpa);
            tiger.assert(false);
        } else |_| {}
    }

    // BM25 & Phrase query verification on decoded segment.
    var q_arena = std.heap.ArenaAllocator.init(gpa);
    defer q_arena.deinit();
    const qa = q_arena.allocator();
    const seg_slice = [_]hseg.Segment{seg};
    const hits_all = try hseg.bm25_search(qa, &seg_slice, "noiseHandshake", .all, .{}, 10);
    tiger.assert(hits_all.len == 1 and hits_all[0].doc_id == 10);
    const hits_phrase = try hseg.bm25_search(qa, &seg_slice, "zig build check", .phrase, .{ .sleeves = &.{"sl-child"} }, 10);
    tiger.assert(hits_phrase.len == 1 and hits_phrase[0].doc_id == 11);
    const hits_file = try hseg.bm25_search(qa, &seg_slice, "build zig", .phrase, .{}, 10);
    tiger.assert(hits_file.len == 1 and hits_file[0].doc_id == 11);
    const hits_boundary = try hseg.bm25_search(qa, &seg_slice, "levels build", .phrase, .{}, 10);
    tiger.assert(hits_boundary.len == 0);
    const hits_miss = try hseg.bm25_search(qa, &seg_slice, "check zig", .phrase, .{}, 10);
    tiger.assert(hits_miss.len == 0);
}
