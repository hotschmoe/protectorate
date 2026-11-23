const std = @import("std");
const tiger = @import("../tiger.zig");
const jcodec = @import("codec.zig");
const event = @import("../core/event.zig");
const ccodec = @import("../core/codec.zig");
const state = @import("../core/state.zig");
const intent = @import("../core/intent.zig");
const apply = @import("../core/apply.zig");

pub const journal_magic = "PTJOURNL";
pub const snap_magic = "PTSNAPSH";
pub const header_version: u32 = 1;
pub const header_len: usize = 12;
pub const max_segment_bytes: u64 = 64 << 20;

pub const StoreError = error{
    Locked,
    BadHeader,
    JournalCorrupted,
    OutOfMemory,
};

pub const JournalStore = struct {
    gpa: std.mem.Allocator,
    io: std.Io,
    dir: std.Io.Dir,
    lock_file: std.Io.File,
    cur_file: std.Io.File,
    cur_seg_id: u64,
    cur_size: u64,

    pub fn open(gpa: std.mem.Allocator, io: std.Io, home_dir: std.Io.Dir) !JournalStore {
        try home_dir.createDirPath(io, "journal");
        var jdir = try home_dir.openDir(io, "journal", .{ .iterate = true });
        errdefer jdir.close(io);

        var lf = try jdir.createFile(io, "lock", .{ .read = true, .truncate = false });
        errdefer lf.close(io);
        const got_lock = try lf.tryLock(io, .exclusive);
        if (!got_lock) return error.Locked;

        var seg_names: std.ArrayList([]const u8) = .empty;
        defer {
            for (seg_names.items) |n| gpa.free(n);
            seg_names.deinit(gpa);
        }

        var iter = jdir.iterate();
        while (try iter.next(io)) |entry| {
            if (entry.kind != .file) continue;
            if (std.mem.startsWith(u8, entry.name, "segment-")) {
                try seg_names.append(gpa, try gpa.dupe(u8, entry.name));
            }
        }

        std.mem.sort([]const u8, seg_names.items, {}, struct {
            fn less(_: void, a: []const u8, b: []const u8) bool {
                return std.mem.lessThan(u8, a, b);
            }
        }.less);

        var last_seg_id: u64 = 0;
        for (seg_names.items, 0..) |name, idx| {
            const is_last = (idx + 1 == seg_names.items.len);
            const seg_id = parse_seg_id(name) orelse return error.BadHeader;
            if (seg_id > last_seg_id) last_seg_id = seg_id;

            var f = try jdir.openFile(io, name, .{ .mode = .read_write });
            defer f.close(io);
            const st = try f.stat(io);
            if (st.size < header_len) {
                if (is_last and st.size == 0) {
                    var hdr: [header_len]u8 = undefined;
                    @memcpy(hdr[0..8], journal_magic);
                    std.mem.writeInt(u32, hdr[8..12], header_version, .little);
                    try f.writePositionalAll(io, &hdr, 0);
                    try f.sync(io);
                    continue;
                }
                return error.BadHeader;
            }
            const raw = try jdir.readFileAlloc(io, name, gpa, .limited(256 << 20));
            defer gpa.free(raw);
            if (!std.mem.eql(u8, raw[0..8], journal_magic)) return error.BadHeader;
            if (std.mem.readInt(u32, raw[8..12], .little) != header_version) return error.BadHeader;

            const body = raw[header_len..];
            const dec = try jcodec.decode_frames(gpa, body);
            defer gpa.free(dec.frames);
            const intact_after = dec.stop == .crc_mismatch and has_intact_frame_after(body, dec.valid_len);
            switch (jcodec.recovery(dec.valid_len, dec.stop, intact_after)) {
                .clean => {},
                .torn => |t| {
                    if (!is_last) return error.JournalCorrupted;
                    const trunc_len: u64 = header_len + t.prefix;
                    try f.setLength(io, trunc_len);
                    try f.sync(io);
                },
                .corrupt => return error.JournalCorrupted,
            }
        }

        if (last_seg_id == 0) {
            last_seg_id = 1;
            var sname_buf: [32]u8 = undefined;
            const sname = std.fmt.bufPrint(&sname_buf, "segment-{d:0>12}", .{last_seg_id}) catch unreachable;
            var cf = try jdir.createFile(io, sname, .{ .read = true, .truncate = true });
            errdefer cf.close(io);
            var hdr: [header_len]u8 = undefined;
            @memcpy(hdr[0..8], journal_magic);
            std.mem.writeInt(u32, hdr[8..12], header_version, .little);
            try cf.writePositionalAll(io, &hdr, 0);
            try cf.sync(io);
            return .{
                .gpa = gpa,
                .io = io,
                .dir = jdir,
                .lock_file = lf,
                .cur_file = cf,
                .cur_seg_id = last_seg_id,
                .cur_size = header_len,
            };
        }

        var sname_buf: [32]u8 = undefined;
        const sname = std.fmt.bufPrint(&sname_buf, "segment-{d:0>12}", .{last_seg_id}) catch unreachable;
        const cf = try jdir.openFile(io, sname, .{ .mode = .read_write });
        const st = try cf.stat(io);
        return .{
            .gpa = gpa,
            .io = io,
            .dir = jdir,
            .lock_file = lf,
            .cur_file = cf,
            .cur_seg_id = last_seg_id,
            .cur_size = st.size,
        };
    }

    pub fn close(self: *JournalStore) void {
        self.cur_file.close(self.io);
        self.lock_file.unlock(self.io);
        self.lock_file.close(self.io);
        self.dir.close(self.io);
    }

    pub fn append(self: *JournalStore, se: event.StampedEvent) !void {
        if (self.cur_size >= max_segment_bytes) {
            try self.rotate_segment();
        }
        var ev_buf: std.ArrayList(u8) = .empty;
        defer ev_buf.deinit(self.gpa);
        try ccodec.encode_stamped(self.gpa, se, &ev_buf);

        var frame_buf: std.ArrayList(u8) = .empty;
        defer frame_buf.deinit(self.gpa);
        try jcodec.encode_frame(self.gpa, ev_buf.items, &frame_buf);

        try self.cur_file.writePositionalAll(self.io, frame_buf.items, self.cur_size);
        try self.cur_file.sync(self.io);
        self.cur_size += frame_buf.items.len;
    }

    fn rotate_segment(self: *JournalStore) !void {
        const next_id = self.cur_seg_id + 1;
        var sname_buf: [32]u8 = undefined;
        const sname = std.fmt.bufPrint(&sname_buf, "segment-{d:0>12}", .{next_id}) catch unreachable;
        var nf = try self.dir.createFile(self.io, sname, .{ .read = true, .truncate = true });
        errdefer nf.close(self.io);
        var hdr: [header_len]u8 = undefined;
        @memcpy(hdr[0..8], journal_magic);
        std.mem.writeInt(u32, hdr[8..12], header_version, .little);
        try nf.writePositionalAll(self.io, &hdr, 0);
        try nf.sync(self.io);
        self.cur_file.close(self.io);
        self.cur_file = nf;
        self.cur_seg_id = next_id;
        self.cur_size = header_len;
    }

    pub fn replay_into(self: *JournalStore, st: *state.State) !void {
        var seg_names: std.ArrayList([]const u8) = .empty;
        defer {
            for (seg_names.items) |n| self.gpa.free(n);
            seg_names.deinit(self.gpa);
        }

        var iter = self.dir.iterate();
        while (try iter.next(self.io)) |entry| {
            if (entry.kind != .file) continue;
            if (std.mem.startsWith(u8, entry.name, "segment-")) {
                try seg_names.append(self.gpa, try self.gpa.dupe(u8, entry.name));
            }
        }
        std.mem.sort([]const u8, seg_names.items, {}, struct {
            fn less(_: void, a: []const u8, b: []const u8) bool {
                return std.mem.lessThan(u8, a, b);
            }
        }.less);

        var arena_st = std.heap.ArenaAllocator.init(self.gpa);
        defer arena_st.deinit();
        var intents: std.ArrayList(intent.Intent) = .empty;
        defer intents.deinit(self.gpa);

        for (seg_names.items) |name| {
            const raw = try self.dir.readFileAlloc(self.io, name, self.gpa, .limited(256 << 20));
            defer self.gpa.free(raw);
            if (raw.len < header_len) return error.BadHeader;
            const dec = try jcodec.decode_frames(self.gpa, raw[header_len..]);
            defer gpa_free_frames(self.gpa, dec.frames);

            for (dec.frames) |payload| {
                _ = arena_st.reset(.retain_capacity);
                const se = try ccodec.decode_stamped(arena_st.allocator(), payload);
                intents.clearRetainingCapacity();
                try apply.apply(self.gpa, st, se, &intents);
            }
        }
    }

    pub fn write_snapshot(self: *JournalStore, seq: u64, data: []const u8) !void {
        var name_buf: [40]u8 = undefined;
        const tmp_name = std.fmt.bufPrint(&name_buf, "snapshot-{d:0>12}.tmp", .{seq}) catch unreachable;
        var final_buf: [32]u8 = undefined;
        const final_name = std.fmt.bufPrint(&final_buf, "snapshot-{d:0>12}", .{seq}) catch unreachable;

        var f = try self.dir.createFile(self.io, tmp_name, .{ .read = true, .truncate = true });
        defer f.close(self.io);
        var hdr: [header_len]u8 = undefined;
        @memcpy(hdr[0..8], snap_magic);
        std.mem.writeInt(u32, hdr[8..12], header_version, .little);
        try f.writePositionalAll(self.io, &hdr, 0);
        try f.writePositionalAll(self.io, data, header_len);
        try f.sync(self.io);
        try std.Io.Dir.rename(self.dir, tmp_name, self.dir, final_name, self.io);
    }

    pub fn copy_for_update(self: *JournalStore, dst_path: []const u8) !void {
        const sname_buf = try std.fmt.allocPrint(self.gpa, "segment-{d:0>12}", .{self.cur_seg_id});
        defer self.gpa.free(sname_buf);
        const raw = try self.dir.readFileAlloc(self.io, sname_buf, self.gpa, .limited(256 << 20));
        defer self.gpa.free(raw);
        try std.Io.Dir.cwd().writeFile(self.io, .{ .sub_path = dst_path, .data = raw });
    }
};

fn gpa_free_frames(gpa: std.mem.Allocator, frames: [][]const u8) void {
    gpa.free(frames);
}

fn parse_seg_id(name: []const u8) ?u64 {
    if (!std.mem.startsWith(u8, name, "segment-")) return null;
    return std.fmt.parseInt(u64, name["segment-".len..], 10) catch null;
}

fn has_intact_frame_after(body: []const u8, bad_at: usize) bool {
    if (bad_at + jcodec.frame_header >= body.len) return false;
    var scan = bad_at + 1;
    while (scan + jcodec.frame_header <= body.len) : (scan += 1) {
        if (jcodec.complete_len(body[scan..])) |flen| {
            const payload = body[scan + jcodec.frame_header .. scan + flen];
            const crc = std.mem.readInt(u32, body[scan + 4 ..][0..4], .little);
            if (jcodec.crc32c(payload) == crc) return true;
        }
    }
    return false;
}
