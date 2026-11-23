const std = @import("std");
const tiger = @import("../tiger.zig");
const json_util = @import("../json_util.zig");
const extract = @import("extract.zig");
const segment = @import("segment.zig");

pub const SourceKind = enum {
    claude,
    codex,
    pi,
    grok,
    hub,
};

pub const SourceEntry = struct {
    path: []const u8,
    source: []const u8,
    sleeve: ?[]const u8 = null,
    harness: ?extract.Harness = null,
    kind: SourceKind,
    offset: u64 = 0,
};

pub const SegFile = struct {
    id: u64,
    filename: []const u8,
    size_bytes: u64,
    seg: segment.Segment,
};

pub const Hit = struct {
    doc_id: u64,
    sleeve_id: []const u8,
    sleeve: []const u8,
    harness: ?extract.Harness,
    session: []const u8,
    turn: u32,
    role: extract.Role,
    time_ms: u64,
    tool: ?[]const u8,
    files: []const []const u8,
    path: []const u8,
    offset: u64,
    score: f64,
    snippet: []const u8,
    highlights: []const segment.HighlightSpan,
};

pub const HitsResult = struct {
    hits: []const Hit,
    indexed_docs: u64,
};

pub const DocView = struct {
    doc_id: u64,
    sleeve_id: []const u8,
    sleeve: []const u8,
    harness: ?extract.Harness,
    session: []const u8,
    turn: u32,
    role: extract.Role,
    time_ms: u64,
    tool: ?[]const u8,
    files: []const []const u8,
    path: []const u8,
    offset: u64,
    text: []const u8,
};

pub const ReadResult = struct {
    docs: []const DocView,
    more: bool,
};

pub const HistoryIndex = struct {
    gpa: std.mem.Allocator,
    io: std.Io,
    dir: std.Io.Dir,
    dir_path: []const u8,
    mu: std.Io.Mutex = .init,
    next_doc_id: u64 = 1,
    next_seg_id: u64 = 1,
    max_bytes: u64 = 512 << 20,
    sources: std.StringHashMapUnmanaged(SourceEntry) = .empty,
    segs: std.ArrayList(SegFile) = .empty,

    pub fn open(gpa: std.mem.Allocator, io: std.Io, dir_path: []const u8) !HistoryIndex {
        try std.Io.Dir.cwd().createDirPath(io, dir_path);
        var dir = try std.Io.Dir.cwd().openDir(io, dir_path, .{ .iterate = true });
        errdefer dir.close(io);

        var idx = HistoryIndex{
            .gpa = gpa,
            .io = io,
            .dir = dir,
            .dir_path = try gpa.dupe(u8, dir_path),
        };
        errdefer idx.deinit();

        try idx.load_manifest();
        try idx.load_segments();
        return idx;
    }

    pub fn deinit(self: *HistoryIndex) void {
        var it = self.sources.iterator();
        while (it.next()) |e| {
            self.gpa.free(e.value_ptr.path);
            self.gpa.free(e.value_ptr.source);
            if (e.value_ptr.sleeve) |s| self.gpa.free(s);
        }
        self.sources.deinit(self.gpa);
        for (self.segs.items) |*sf| {
            self.gpa.free(sf.filename);
            sf.seg.deinit(self.gpa);
        }
        self.segs.deinit(self.gpa);
        self.gpa.free(self.dir_path);
        self.dir.close(self.io);
    }

    fn load_manifest(self: *HistoryIndex) !void {
        const raw = self.dir.readFileAlloc(self.io, "manifest.json", self.gpa, .limited(32 << 20)) catch return;
        defer self.gpa.free(raw);
        const parsed = std.json.parseFromSlice(std.json.Value, self.gpa, raw, .{}) catch return;
        defer parsed.deinit();
        const root = parsed.value;
        if (root != .object) return;
        if (json_util.value_u64(root, "next_doc_id")) |v| self.next_doc_id = @max(1, v);
        if (json_util.value_u64(root, "next_seg_id")) |v| self.next_seg_id = @max(1, v);
        const arr = json_util.value_array(root, "sources") orelse return;
        for (arr) |item| {
            const path = json_util.value_str(item, "path") orelse continue;
            const src = json_util.value_str(item, "source") orelse path;
            const slv = json_util.value_str(item, "sleeve");
            const h_str = json_util.value_str(item, "harness");
            const kind_str = json_util.value_str(item, "kind") orelse "hub";
            const off = json_util.value_u64(item, "offset") orelse 0;
            const k = std.meta.stringToEnum(SourceKind, kind_str) orelse .hub;
            const path_d = try self.gpa.dupe(u8, path);
            errdefer self.gpa.free(path_d);
            const src_d = try self.gpa.dupe(u8, src);
            errdefer self.gpa.free(src_d);
            const slv_d = if (slv) |s| try self.gpa.dupe(u8, s) else null;
            errdefer if (slv_d) |s| self.gpa.free(s);
            const gop = try self.sources.getOrPut(self.gpa, path_d);
            if (gop.found_existing) {
                self.gpa.free(path_d);
                self.gpa.free(gop.value_ptr.source);
                if (gop.value_ptr.sleeve) |s| self.gpa.free(s);
                gop.value_ptr.source = src_d;
                gop.value_ptr.sleeve = slv_d;
                gop.value_ptr.offset = off;
            } else {
                gop.value_ptr.* = .{
                    .path = path_d,
                    .source = src_d,
                    .sleeve = slv_d,
                    .harness = if (h_str) |hs| extract.Harness.from_wire(hs) else null,
                    .kind = k,
                    .offset = off,
                };
            }
        }
    }

    fn save_manifest(self: *HistoryIndex) !void {
        var aw: std.Io.Writer.Allocating = .init(self.gpa);
        defer aw.deinit();
        var jw: std.json.Stringify = .{ .writer = &aw.writer };
        try jw.beginObject();
        try jw.objectField("next_doc_id");
        try jw.write(self.next_doc_id);
        try jw.objectField("next_seg_id");
        try jw.write(self.next_seg_id);
        try jw.objectField("sources");
        try jw.beginArray();
        var it = self.sources.iterator();
        while (it.next()) |e| {
            const s = e.value_ptr.*;
            try jw.beginObject();
            try jw.objectField("path");
            try jw.write(s.path);
            try jw.objectField("source");
            try jw.write(s.source);
            if (s.sleeve) |slv| {
                try jw.objectField("sleeve");
                try jw.write(slv);
            }
            if (s.harness) |h| {
                try jw.objectField("harness");
                try jw.write(h.to_wire());
            }
            try jw.objectField("kind");
            try jw.write(@tagName(s.kind));
            try jw.objectField("offset");
            try jw.write(s.offset);
            try jw.endObject();
        }
        try jw.endArray();
        try jw.endObject();

        try self.dir.writeFile(self.io, .{ .sub_path = "manifest.json.tmp", .data = aw.written() });
        try std.Io.Dir.rename(self.dir, "manifest.json.tmp", self.dir, "manifest.json", self.io);
    }

    fn load_segments(self: *HistoryIndex) !void {
        var names: std.ArrayList([]const u8) = .empty;
        defer {
            for (names.items) |n| self.gpa.free(n);
            names.deinit(self.gpa);
        }
        var iter = self.dir.iterate();
        while (try iter.next(self.io)) |entry| {
            if (entry.kind != .file) continue;
            if (std.mem.startsWith(u8, entry.name, "seg-") and std.mem.endsWith(u8, entry.name, ".ptseg")) {
                try names.append(self.gpa, try self.gpa.dupe(u8, entry.name));
            }
        }
        std.mem.sort([]const u8, names.items, {}, struct {
            fn less(_: void, a: []const u8, b: []const u8) bool {
                return std.mem.lessThan(u8, a, b);
            }
        }.less);

        for (names.items) |name| {
            const raw = self.dir.readFileAlloc(self.io, name, self.gpa, .limited(128 << 20)) catch continue;
            defer self.gpa.free(raw);
            const seg = segment.decode_segment(self.gpa, raw) catch continue;
            const id = parse_seg_filename(name) orelse 0;
            if (id >= self.next_seg_id) self.next_seg_id = id + 1;
            for (seg.docs) |d| {
                if (d.id >= self.next_doc_id) self.next_doc_id = d.id + 1;
            }
            try self.segs.append(self.gpa, .{
                .id = id,
                .filename = try self.gpa.dupe(u8, name),
                .size_bytes = raw.len,
                .seg = seg,
            });
        }
    }

    pub fn ingest_file(
        self: *HistoryIndex,
        file_path: []const u8,
        sleeve: ?[]const u8,
        harness_opt: ?extract.Harness,
        kind: SourceKind,
    ) !u64 {
        self.mu.lockUncancelable(self.io);
        defer self.mu.unlock(self.io);

        var file = std.Io.Dir.cwd().openFile(self.io, file_path, .{}) catch return 0;
        defer file.close(self.io);
        const st = file.stat(self.io) catch return 0;

        var start_off: u64 = 0;
        if (self.sources.getPtr(file_path)) |prev| {
            if (st.size == prev.offset) return 0;
            if (st.size > prev.offset) start_off = prev.offset;
        }
        if (st.size <= start_off) return 0;

        const read_len: usize = @intCast(@min(st.size - start_off, 64 << 20));
        const buf = try self.gpa.alloc(u8, read_len);
        defer self.gpa.free(buf);
        const got = try file.readPositionalAll(self.io, buf, start_off);
        const slice = buf[0..got];
        const last_nl = std.mem.findScalarLast(u8, slice, '\n') orelse return 0;
        const complete = slice[0 .. last_nl + 1];
        const new_off = start_off + complete.len;

        var arena_st = std.heap.ArenaAllocator.init(self.gpa);
        defer arena_st.deinit();
        const arena = arena_st.allocator();

        var est: extract.ExtractState = .{
            .session = std.fs.path.stem(file_path),
            .harness = harness_opt,
        };
        var items: std.ArrayList(extract.Item) = .empty;
        var docs: std.ArrayList(segment.SegmentInputDoc) = .empty;

        var lines = std.mem.splitScalar(u8, complete, '\n');
        var line_off = start_off;
        while (lines.next()) |raw_line| {
            const cur_off = line_off;
            line_off += raw_line.len + 1;
            const line = std.mem.trim(u8, raw_line, " \t\r");
            if (line.len == 0) continue;
            const val = std.json.parseFromSliceLeaky(std.json.Value, arena, line, .{}) catch continue;
            if (val != .object) continue;

            items.clearRetainingCapacity();
            const ok = switch (kind) {
                .claude => try extract.extract_claude_line(arena, val, &est, &items),
                .codex => try extract.extract_codex_line(arena, val, &est, &items),
                .pi => try extract.extract_pi_line(arena, val, &est, &items),
                .grok => try extract.extract_grok_line(arena, val, &est, &items),
                .hub => try extract.extract_hub_line(arena, val, &est, &items),
            };
            if (!ok) continue;

            for (items.items) |item| {
                switch (item) {
                    .doc => |d| {
                        const doc_id = self.next_doc_id;
                        self.next_doc_id += 1;
                        try docs.append(arena, .{
                            .id = doc_id,
                            .time_ms = d.time_ms,
                            .offset = cur_off,
                            .turn = d.turn,
                            .session = d.session,
                            .sleeve = sleeve,
                            .source = file_path,
                            .tool = d.tool,
                            .harness = d.harness,
                            .role = d.role,
                            .files = d.files,
                            .text = d.text,
                        });
                    },
                    else => {},
                }
            }
        }

        if (docs.items.len > 0) {
            const raw_seg = try segment.encode_segment(self.gpa, docs.items);
            defer self.gpa.free(raw_seg);

            const seg_id = self.next_seg_id;
            self.next_seg_id += 1;
            var fn_buf: [40]u8 = undefined;
            const final_name = std.fmt.bufPrint(&fn_buf, "seg-{d:0>12}.ptseg", .{seg_id}) catch unreachable;
            var tmp_buf: [48]u8 = undefined;
            const tmp_name = std.fmt.bufPrint(&tmp_buf, "{s}.tmp", .{final_name}) catch unreachable;

            try self.dir.writeFile(self.io, .{ .sub_path = tmp_name, .data = raw_seg });
            try std.Io.Dir.rename(self.dir, tmp_name, self.dir, final_name, self.io);

            const dec = try segment.decode_segment(self.gpa, raw_seg);
            try self.segs.append(self.gpa, .{
                .id = seg_id,
                .filename = try self.gpa.dupe(u8, final_name),
                .size_bytes = raw_seg.len,
                .seg = dec,
            });
            self.evict_over_budget();
        }

        const key_dup = try self.gpa.dupe(u8, file_path);
        const gop = try self.sources.getOrPut(self.gpa, key_dup);
        if (gop.found_existing) {
            self.gpa.free(key_dup);
            gop.value_ptr.offset = new_off;
        } else {
            gop.value_ptr.* = .{
                .path = key_dup,
                .source = try self.gpa.dupe(u8, file_path),
                .sleeve = if (sleeve) |s| try self.gpa.dupe(u8, s) else null,
                .harness = harness_opt,
                .kind = kind,
                .offset = new_off,
            };
        }
        try self.save_manifest();
        return docs.items.len;
    }

    pub fn ingest_roots(self: *HistoryIndex, user_home: ?[]const u8, hub_transcripts_dir: ?[]const u8) u64 {
        return self.ingest_roots_cancellable(user_home, hub_transcripts_dir, null);
    }

    pub fn ingest_roots_cancellable(
        self: *HistoryIndex,
        user_home: ?[]const u8,
        hub_transcripts_dir: ?[]const u8,
        stop_flag: ?*const std.atomic.Value(bool),
    ) u64 {
        var added: u64 = 0;
        if (stop_flag) |sf| {
            if (sf.load(.acquire)) return added;
        }
        if (hub_transcripts_dir) |dir_p| {
            added += self.scan_dir(dir_p, .hub, null, true, 0, stop_flag);
        }
        if (user_home) |h| {
            var arena_st = std.heap.ArenaAllocator.init(self.gpa);
            defer arena_st.deinit();
            const a = arena_st.allocator();
            const roots = [_]struct { sub: []const u8, kind: SourceKind, harness: extract.Harness }{
                .{ .sub = ".claude/projects", .kind = .claude, .harness = .claude },
                .{ .sub = ".codex/sessions", .kind = .codex, .harness = .codex },
                .{ .sub = ".pi/agent/sessions", .kind = .pi, .harness = .pi_local },
                .{ .sub = ".grok", .kind = .grok, .harness = .grok },
            };
            for (roots) |r| {
                if (stop_flag) |sf| {
                    if (sf.load(.acquire)) break;
                }
                const full = std.fs.path.join(a, &.{ h, r.sub }) catch continue;
                added += self.scan_dir(full, r.kind, r.harness, false, 0, stop_flag);
            }
        }
        return added;
    }

    fn scan_dir(
        self: *HistoryIndex,
        dir_path: []const u8,
        kind: SourceKind,
        harness: ?extract.Harness,
        stem_is_sleeve: bool,
        depth: u8,
        stop_flag: ?*const std.atomic.Value(bool),
    ) u64 {
        if (depth > 4) return 0;
        if (stop_flag) |sf| {
            if (sf.load(.acquire)) return 0;
        }
        var dir = std.Io.Dir.cwd().openDir(self.io, dir_path, .{ .iterate = true }) catch return 0;
        defer dir.close(self.io);
        var added: u64 = 0;
        var it = dir.iterate();
        while (it.next(self.io) catch null) |entry| {
            if (stop_flag) |sf| {
                if (sf.load(.acquire)) break;
            }
            const full = std.fs.path.join(self.gpa, &.{ dir_path, entry.name }) catch continue;
            defer self.gpa.free(full);
            if (entry.kind == .directory) {
                added += self.scan_dir(full, kind, harness, stem_is_sleeve, depth + 1, stop_flag);
            } else if (entry.kind == .file and std.mem.endsWith(u8, entry.name, ".jsonl")) {
                const slv = if (stem_is_sleeve) std.fs.path.stem(entry.name) else null;
                added += self.ingest_file(full, slv, harness, kind) catch 0;
            }
        }
        return added;
    }

    fn evict_over_budget(self: *HistoryIndex) void {
        var total: u64 = 0;
        for (self.segs.items) |sf| total += sf.size_bytes;
        while (total > self.max_bytes and self.segs.items.len > 1) {
            var oldest = self.segs.orderedRemove(0);
            total -= oldest.size_bytes;
            self.dir.deleteFile(self.io, oldest.filename) catch {};
            self.gpa.free(oldest.filename);
            oldest.seg.deinit(self.gpa);
        }
    }

    pub fn indexed_doc_count(self: *HistoryIndex) u64 {
        self.mu.lockUncancelable(self.io);
        defer self.mu.unlock(self.io);
        var total: u64 = 0;
        for (self.segs.items) |sf| total += sf.seg.docs.len;
        return total;
    }

    pub fn search(
        self: *HistoryIndex,
        arena: std.mem.Allocator,
        query_text: []const u8,
        mode: segment.QueryMode,
        filters: segment.Filters,
        limit: usize,
    ) !HitsResult {
        self.mu.lockUncancelable(self.io);
        defer self.mu.unlock(self.io);

        var raw_segs = try arena.alloc(segment.Segment, self.segs.items.len);
        var total_docs: u64 = 0;
        for (self.segs.items, 0..) |sf, i| {
            raw_segs[i] = sf.seg;
            total_docs += sf.seg.docs.len;
        }
        const raw_hits = try segment.bm25_search(arena, raw_segs, query_text, mode, filters, limit);
        var out_hits = try arena.alloc(Hit, raw_hits.len);
        for (raw_hits, 0..) |rh, i| {
            const s = &raw_segs[rh.seg_idx];
            const d = s.docs[rh.doc_idx];
            var files = try arena.alloc([]const u8, d.file_syms.len);
            for (d.file_syms, 0..) |fs, fi| files[fi] = try arena.dupe(u8, s.sym(fs));
            const slv = if (d.sleeve_sym) |ss| try arena.dupe(u8, s.sym(ss)) else "";
            out_hits[i] = .{
                .doc_id = d.id,
                .sleeve_id = slv,
                .sleeve = slv,
                .harness = d.harness,
                .session = try arena.dupe(u8, s.sym(d.session_sym)),
                .turn = d.turn,
                .role = d.role,
                .time_ms = d.time_ms,
                .tool = if (d.tool_sym) |ts| try arena.dupe(u8, s.sym(ts)) else null,
                .files = files,
                .path = try arena.dupe(u8, s.sym(d.source_sym)),
                .offset = d.offset,
                .score = rh.score,
                .snippet = try arena.dupe(u8, rh.snippet_text),
                .highlights = try arena.dupe(segment.HighlightSpan, rh.highlights),
            };
        }
        return .{ .hits = out_hits, .indexed_docs = total_docs };
    }

    pub fn read_docs(
        self: *HistoryIndex,
        arena: std.mem.Allocator,
        doc_id: ?u64,
        session_filter: ?[]const u8,
        from_turn: u64,
        to_turn: ?u64,
        after_doc: ?u64,
        limit: usize,
    ) !ReadResult {
        self.mu.lockUncancelable(self.io);
        defer self.mu.unlock(self.io);

        var list: std.ArrayList(DocView) = .empty;
        var more = false;

        outer: for (self.segs.items) |*sf| {
            const s = &sf.seg;
            for (s.docs) |d| {
                if (doc_id) |want_id| {
                    if (d.id != want_id) continue;
                } else {
                    if (session_filter) |want_sess| {
                        if (!std.mem.eql(u8, s.sym(d.session_sym), want_sess)) continue;
                    }
                    if (d.turn < from_turn) continue;
                    if (to_turn) |tt| {
                        if (d.turn > tt) continue;
                    }
                    if (after_doc) |ad| {
                        if (d.id <= ad) continue;
                    }
                }
                if (list.items.len >= limit) {
                    more = true;
                    break :outer;
                }
                var files = try arena.alloc([]const u8, d.file_syms.len);
                for (d.file_syms, 0..) |fs, fi| files[fi] = try arena.dupe(u8, s.sym(fs));
                const slv = if (d.sleeve_sym) |ss| try arena.dupe(u8, s.sym(ss)) else "";
                try list.append(arena, .{
                    .doc_id = d.id,
                    .sleeve_id = slv,
                    .sleeve = slv,
                    .harness = d.harness,
                    .session = try arena.dupe(u8, s.sym(d.session_sym)),
                    .turn = d.turn,
                    .role = d.role,
                    .time_ms = d.time_ms,
                    .tool = if (d.tool_sym) |ts| try arena.dupe(u8, s.sym(ts)) else null,
                    .files = files,
                    .path = try arena.dupe(u8, s.sym(d.source_sym)),
                    .offset = d.offset,
                    .text = try arena.dupe(u8, s.doc_text(d)),
                });
            }
        }

        return .{ .docs = list.items, .more = more };
    }
};

fn parse_seg_filename(name: []const u8) ?u64 {
    if (!std.mem.startsWith(u8, name, "seg-") or !std.mem.endsWith(u8, name, ".ptseg")) return null;
    const mid = name["seg-".len .. name.len - ".ptseg".len];
    return std.fmt.parseInt(u64, mid, 10) catch null;
}
