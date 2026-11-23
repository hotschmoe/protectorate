const std = @import("std");
const tiger = @import("../tiger.zig");
const token = @import("token.zig");
const extract = @import("extract.zig");

pub const Crc32c = std.hash.crc.@"CRC-32/ISCSI";
pub const seg_magic = "PTHISTSG";
pub const footer_magic = "PTHX";
pub const seg_version: u32 = 1;
pub const seg_header_len: usize = 64;
pub const seg_footer_len: usize = 16;

pub const snippet_max_bytes: usize = 240;
pub const bm25_k1: f64 = 1.2;
pub const bm25_b: f64 = 0.75;
pub const brief_max_terms: usize = 12;

pub const SegMeta = struct {
    id: u64,
    time_ms: u64,
    offset: u64,
    turn: u32,
    len: u32,
    text_off: u64,
    text_len: u32,
    session_sym: u32,
    sleeve_sym: ?u32,
    source_sym: u32,
    tool_sym: ?u32,
    harness: ?extract.Harness,
    role: extract.Role,
    file_syms: []const u32,
};

pub const PostEntry = struct {
    off: u64,
    len: u32,
    df: u32,
};

pub const PostingHit = struct {
    doc_idx: u32,
    positions: []const u32,
};

pub const Segment = struct {
    strings: [][]const u8,
    docs: []SegMeta,
    terms: [][]const u8,
    posts: []PostEntry,
    postings_bytes: []const u8,
    texts_bytes: []const u8,

    pub fn deinit(self: *Segment, gpa: std.mem.Allocator) void {
        for (self.strings) |s| gpa.free(s);
        gpa.free(self.strings);
        for (self.docs) |d| gpa.free(d.file_syms);
        gpa.free(self.docs);
        for (self.terms) |t| gpa.free(t);
        gpa.free(self.terms);
        gpa.free(self.posts);
        gpa.free(self.postings_bytes);
        gpa.free(self.texts_bytes);
    }

    pub fn sym(self: *const Segment, id: u32) []const u8 {
        return self.strings[id];
    }

    pub fn doc_text(self: *const Segment, d: SegMeta) []const u8 {
        const start: usize = @intCast(d.text_off);
        return self.texts_bytes[start .. start + d.text_len];
    }

    pub fn find_term(self: *const Segment, term: []const u8) ?PostEntry {
        var lo: usize = 0;
        var hi: usize = self.terms.len;
        while (lo < hi) {
            const mid = lo + (hi - lo) / 2;
            switch (std.mem.order(u8, self.terms[mid], term)) {
                .eq => return self.posts[mid],
                .lt => lo = mid + 1,
                .gt => hi = mid,
            }
        }
        return null;
    }

    pub fn postings_slice(self: *const Segment, p: PostEntry) []const u8 {
        const start: usize = @intCast(p.off);
        return self.postings_bytes[start .. start + p.len];
    }
};

pub const SegmentInputDoc = struct {
    id: u64,
    time_ms: u64,
    offset: u64,
    turn: u32,
    session: []const u8,
    sleeve: ?[]const u8,
    source: []const u8,
    tool: ?[]const u8,
    harness: ?extract.Harness,
    role: extract.Role,
    files: []const []const u8,
    text: []const u8,
};

fn put_uvar(gpa: std.mem.Allocator, out: *std.ArrayList(u8), v_init: u64) !void {
    var v = v_init;
    while (true) {
        const b: u8 = @intCast(v & 0x7f);
        v >>= 7;
        if (v == 0) {
            try out.append(gpa, b);
            break;
        }
        try out.append(gpa, b | 0x80);
    }
}

fn put_u32(gpa: std.mem.Allocator, out: *std.ArrayList(u8), v: u32) !void {
    var buf: [4]u8 = undefined;
    std.mem.writeInt(u32, &buf, v, .little);
    try out.appendSlice(gpa, &buf);
}

fn harness_to_code(h: ?extract.Harness) u64 {
    return if (h) |hv| @as(u64, @intFromEnum(hv)) + 1 else 0;
}

fn code_to_harness(c: u64) !?extract.Harness {
    return switch (c) {
        0 => null,
        1 => .claude,
        2 => .codex,
        3 => .grok,
        4 => .pi_vendor,
        5 => .pi_local,
        6 => .antigravity,
        else => error.CorruptSegment,
    };
}

const SymIntern = struct {
    list: std.ArrayList([]const u8) = .empty,
    map: std.StringHashMapUnmanaged(u32) = .empty,

    fn deinit(self: *SymIntern, gpa: std.mem.Allocator) void {
        self.list.deinit(gpa);
        self.map.deinit(gpa);
    }

    fn intern(self: *SymIntern, gpa: std.mem.Allocator, s: []const u8) !u32 {
        if (self.map.get(s)) |id| return id;
        const id: u32 = @intCast(self.list.items.len);
        try self.list.append(gpa, s);
        try self.map.put(gpa, s, id);
        return id;
    }
};

pub fn encode_segment(gpa: std.mem.Allocator, docs_in: []const SegmentInputDoc) ![]u8 {
    var syms = SymIntern{};
    defer syms.deinit(gpa);

    var s_docs: std.ArrayList(u8) = .empty;
    defer s_docs.deinit(gpa);
    var s_texts: std.ArrayList(u8) = .empty;
    defer s_texts.deinit(gpa);

    const DocPos = struct { doc_idx: u32, pos: u32 };
    var term_map: std.StringHashMapUnmanaged(std.ArrayList(DocPos)) = .empty;
    defer {
        var it = term_map.iterator();
        while (it.next()) |e| {
            gpa.free(e.key_ptr.*);
            e.value_ptr.deinit(gpa);
        }
        term_map.deinit(gpa);
    }

    const TokAcc = struct {
        gpa: std.mem.Allocator,
        doc_idx: u32,
        base_pos: u32 = 0,
        map: *std.StringHashMapUnmanaged(std.ArrayList(DocPos)),
        fn on_tok(ctx: @This(), t: token.Tok) anyerror!void {
            const p = ctx.base_pos + t.pos;
            if (ctx.map.getPtr(t.term)) |lst| {
                try lst.append(ctx.gpa, .{ .doc_idx = ctx.doc_idx, .pos = p });
            } else {
                const k = try ctx.gpa.dupe(u8, t.term);
                errdefer ctx.gpa.free(k);
                var lst: std.ArrayList(DocPos) = .empty;
                try lst.append(ctx.gpa, .{ .doc_idx = ctx.doc_idx, .pos = p });
                try ctx.map.put(ctx.gpa, k, lst);
            }
        }
    };

    var prev_id: ?u64 = null;
    for (docs_in, 0..) |d, di| {
        const doc_idx: u32 = @intCast(di);
        const id_delta = if (prev_id) |pid| blk: {
            tiger.assert(d.id > pid);
            break :blk d.id - pid;
        } else d.id;
        prev_id = d.id;

        var acc = TokAcc{ .gpa = gpa, .doc_idx = doc_idx, .base_pos = 0, .map = &term_map };
        const npos = try token.tokenize(d.text, acc, TokAcc.on_tok);
        acc.base_pos += npos + 1;

        for (d.files) |f| {
            const fnpos = try token.tokenize(f, acc, TokAcc.on_tok);
            acc.base_pos += fnpos + 1;
        }

        const text_len: u32 = @intCast(d.text.len);
        try s_texts.appendSlice(gpa, d.text);

        const s_session = try syms.intern(gpa, d.session);
        const s_sleeve: ?u32 = if (d.sleeve) |sl| try syms.intern(gpa, sl) else null;
        const s_source = try syms.intern(gpa, d.source);
        const s_tool: ?u32 = if (d.tool) |tl| try syms.intern(gpa, tl) else null;

        try put_uvar(gpa, &s_docs, id_delta);
        try put_uvar(gpa, &s_docs, d.time_ms);
        try put_uvar(gpa, &s_docs, d.offset);
        try put_u32(gpa, &s_docs, d.turn);
        try put_u32(gpa, &s_docs, @max(acc.base_pos, 1));
        try put_u32(gpa, &s_docs, text_len);
        try put_uvar(gpa, &s_docs, s_session + 1);
        try put_uvar(gpa, &s_docs, if (s_sleeve) |v| v + 1 else 0);
        try put_uvar(gpa, &s_docs, s_source + 1);
        try put_uvar(gpa, &s_docs, if (s_tool) |v| v + 1 else 0);
        try put_uvar(gpa, &s_docs, harness_to_code(d.harness));
        try put_uvar(gpa, &s_docs, @intFromEnum(d.role));
        try put_uvar(gpa, &s_docs, d.files.len);
        for (d.files) |f| {
            const sf = try syms.intern(gpa, f);
            try put_uvar(gpa, &s_docs, sf + 1);
        }
        // Note: if npos == 0 with non-empty text, max(npos,1) keeps len valid for bounds.
        if (npos == 0 and d.text.len > 0) {
            const k = "empty";
            _ = k;
        }
    }

    var s_syms: std.ArrayList(u8) = .empty;
    defer s_syms.deinit(gpa);
    for (syms.list.items) |s| {
        try put_uvar(gpa, &s_syms, s.len);
        try s_syms.appendSlice(gpa, s);
    }

    var sorted_terms: std.ArrayList([]const u8) = .empty;
    defer sorted_terms.deinit(gpa);
    var kit = term_map.keyIterator();
    while (kit.next()) |kp| {
        try sorted_terms.append(gpa, kp.*);
    }
    std.mem.sort([]const u8, sorted_terms.items, {}, struct {
        fn less(_: void, a: []const u8, b: []const u8) bool {
            return std.mem.lessThan(u8, a, b);
        }
    }.less);

    var s_dict: std.ArrayList(u8) = .empty;
    defer s_dict.deinit(gpa);
    var s_post: std.ArrayList(u8) = .empty;
    defer s_post.deinit(gpa);

    for (sorted_terms.items) |t| {
        const occs = term_map.get(t).?;
        const start_off = s_post.items.len;

        var df: u32 = 0;
        var idx: usize = 0;
        var prev_doc: ?u32 = null;
        while (idx < occs.items.len) {
            const d_id = occs.items[idx].doc_idx;
            var j = idx + 1;
            while (j < occs.items.len and occs.items[j].doc_idx == d_id) : (j += 1) {}

            // Deduplicate and sort positions for this doc.
            var doc_pos: std.ArrayList(u32) = .empty;
            defer doc_pos.deinit(gpa);
            for (occs.items[idx..j]) |o| {
                try doc_pos.append(gpa, o.pos);
            }
            std.mem.sort(u32, doc_pos.items, {}, std.sort.asc(u32));

            var uniq_len: usize = 0;
            for (doc_pos.items) |p| {
                if (uniq_len == 0 or doc_pos.items[uniq_len - 1] != p) {
                    doc_pos.items[uniq_len] = p;
                    uniq_len += 1;
                }
            }

            const delta_d: u32 = if (prev_doc) |pd| d_id - pd else d_id;
            prev_doc = d_id;
            df += 1;

            try put_u32(gpa, &s_post, delta_d);
            try put_u32(gpa, &s_post, @intCast(uniq_len));
            var prev_p: u32 = 0;
            for (doc_pos.items[0..uniq_len], 0..) |p, pi| {
                const pv: u32 = if (pi == 0) p else p - prev_p;
                try put_u32(gpa, &s_post, pv);
                prev_p = p;
            }
            idx = j;
        }

        const plen: u32 = @intCast(s_post.items.len - start_off);
        try put_uvar(gpa, &s_dict, t.len);
        try s_dict.appendSlice(gpa, t);
        try put_u32(gpa, &s_dict, df);
        try put_u32(gpa, &s_dict, plen);
    }

    const total_body: usize = seg_header_len + s_syms.items.len + s_docs.items.len + s_dict.items.len + s_post.items.len + s_texts.items.len;
    var out: std.ArrayList(u8) = .empty;
    errdefer out.deinit(gpa);
    try out.ensureTotalCapacity(gpa, total_body + seg_footer_len);

    var hdr: [seg_header_len]u8 = @splat(0);
    @memcpy(hdr[0..8], seg_magic);
    std.mem.writeInt(u32, hdr[8..12], seg_version, .little);
    std.mem.writeInt(u32, hdr[12..16], @intCast(syms.list.items.len), .little);
    std.mem.writeInt(u32, hdr[16..20], @intCast(docs_in.len), .little);
    std.mem.writeInt(u32, hdr[20..24], @intCast(sorted_terms.items.len), .little);
    std.mem.writeInt(u64, hdr[24..32], s_syms.items.len, .little);
    std.mem.writeInt(u64, hdr[32..40], s_docs.items.len, .little);
    std.mem.writeInt(u64, hdr[40..48], s_dict.items.len, .little);
    std.mem.writeInt(u64, hdr[48..56], s_post.items.len, .little);
    std.mem.writeInt(u64, hdr[56..64], s_texts.items.len, .little);

    out.appendSliceAssumeCapacity(&hdr);
    out.appendSliceAssumeCapacity(s_syms.items);
    out.appendSliceAssumeCapacity(s_docs.items);
    out.appendSliceAssumeCapacity(s_dict.items);
    out.appendSliceAssumeCapacity(s_post.items);
    out.appendSliceAssumeCapacity(s_texts.items);

    const crc = Crc32c.hash(out.items);
    var ftr: [seg_footer_len]u8 = undefined;
    std.mem.writeInt(u64, ftr[0..8], total_body, .little);
    std.mem.writeInt(u32, ftr[8..12], crc, .little);
    @memcpy(ftr[12..16], footer_magic);
    out.appendSliceAssumeCapacity(&ftr);

    return out.toOwnedSlice(gpa);
}

const Reader = struct {
    b: []const u8,
    i: usize = 0,

    fn done(self: *const Reader) bool {
        return self.i >= self.b.len;
    }

    fn bytes(self: *Reader, n: usize) ![]const u8 {
        if (n > self.b.len - self.i) return error.CorruptSegment;
        const s = self.b[self.i .. self.i + n];
        self.i += n;
        return s;
    }

    fn u32_val(self: *Reader) !u32 {
        const s = try self.bytes(4);
        return std.mem.readInt(u32, s[0..4], .little);
    }

    fn var_u64(self: *Reader) !u64 {
        var out: u64 = 0;
        var shift: u6 = 0;
        while (true) {
            const b = (try self.bytes(1))[0];
            out |= @as(u64, b & 0x7f) << shift;
            if ((b & 0x80) == 0) return out;
            if (shift >= 56) return error.CorruptSegment;
            shift += 7;
        }
    }

    fn sym(self: *Reader, nstr: u32, optional: bool) !?u32 {
        const v = try self.var_u64();
        if (v == 0) {
            return if (optional) null else error.CorruptSegment;
        }
        const idx = v - 1;
        if (idx >= nstr) return error.CorruptSegment;
        return @intCast(idx);
    }
};

pub fn walk_postings(
    gpa: std.mem.Allocator,
    post_bytes: []const u8,
    ndocs: u32,
) ![]PostingHit {
    var hits: std.ArrayList(PostingHit) = .empty;
    errdefer {
        for (hits.items) |h| gpa.free(h.positions);
        hits.deinit(gpa);
    }
    var r = Reader{ .b = post_bytes };
    var cur_doc: ?u32 = null;
    while (!r.done()) {
        const delta = try r.u32_val();
        const d: u32 = if (cur_doc) |pd| blk: {
            if (delta == 0) return error.CorruptSegment;
            break :blk std.math.add(u32, pd, delta) catch return error.CorruptSegment;
        } else delta;
        if (d >= ndocs) return error.CorruptSegment;
        const tf = try r.u32_val();
        if (tf == 0 or @as(usize, tf) > (r.b.len - r.i) / 4) return error.CorruptSegment;
        var pos_list = try gpa.alloc(u32, tf);
        errdefer gpa.free(pos_list);
        var cur_p: u32 = 0;
        for (0..tf) |k| {
            const v = try r.u32_val();
            cur_p = if (k == 0) v else blk: {
                if (v == 0) return error.CorruptSegment;
                break :blk std.math.add(u32, cur_p, v) catch return error.CorruptSegment;
            };
            pos_list[k] = cur_p;
        }
        try hits.append(gpa, .{ .doc_idx = d, .positions = pos_list });
        cur_doc = d;
    }
    return hits.toOwnedSlice(gpa);
}

pub fn decode_segment(gpa: std.mem.Allocator, b: []const u8) !Segment {
    if (b.len < seg_header_len + seg_footer_len) return error.CorruptSegment;
    const body = b.len - seg_footer_len;
    if (!std.mem.eql(u8, b[b.len - 4 ..], footer_magic)) return error.CorruptSegment;
    if (std.mem.readInt(u64, b[body..][0..8], .little) != body) return error.CorruptSegment;
    if (Crc32c.hash(b[0..body]) != std.mem.readInt(u32, b[body + 8 ..][0..4], .little)) {
        return error.CorruptSegment;
    }
    if (!std.mem.eql(u8, b[0..8], seg_magic)) return error.CorruptSegment;
    if (std.mem.readInt(u32, b[8..12], .little) != seg_version) return error.CorruptSegment;

    const nstr = std.mem.readInt(u32, b[12..16], .little);
    const ndocs = std.mem.readInt(u32, b[16..20], .little);
    const nterms = std.mem.readInt(u32, b[20..24], .little);

    var starts: [5]usize = undefined;
    var ends: [5]usize = undefined;
    var at: u64 = seg_header_len;
    for (0..5) |k| {
        const l = std.mem.readInt(u64, b[24 + 8 * k ..][0..8], .little);
        const end = std.math.add(u64, at, l) catch return error.CorruptSegment;
        if (end > body) return error.CorruptSegment;
        starts[k] = @intCast(at);
        ends[k] = @intCast(end);
        at = end;
    }
    if (at != body) return error.CorruptSegment;

    const s0 = b[starts[0]..ends[0]];
    const s1 = b[starts[1]..ends[1]];
    const s2 = b[starts[2]..ends[2]];
    const s3 = b[starts[3]..ends[3]];
    const s4 = b[starts[4]..ends[4]];

    if (nstr > s0.len) return error.CorruptSegment;
    var strings = try std.ArrayList([]const u8).initCapacity(gpa, nstr);
    errdefer {
        for (strings.items) |s| gpa.free(s);
        strings.deinit(gpa);
    }
    var r0 = Reader{ .b = s0 };
    for (0..nstr) |_| {
        const slen = try r0.var_u64();
        if (slen > s0.len) return error.CorruptSegment;
        const s = try r0.bytes(@intCast(slen));
        if (!std.unicode.utf8ValidateSlice(s)) return error.CorruptSegment;
        strings.appendAssumeCapacity(try gpa.dupe(u8, s));
    }
    if (!r0.done()) return error.CorruptSegment;

    if (ndocs > s1.len) return error.CorruptSegment;
    var docs = try std.ArrayList(SegMeta).initCapacity(gpa, ndocs);
    errdefer {
        for (docs.items) |d| gpa.free(d.file_syms);
        docs.deinit(gpa);
    }
    var r1 = Reader{ .b = s1 };
    var text_off: u64 = 0;
    for (0..ndocs) |_| {
        const delta = try r1.var_u64();
        const id = if (docs.items.len == 0) delta else blk: {
            if (delta == 0) return error.CorruptSegment;
            break :blk std.math.add(u64, docs.items[docs.items.len - 1].id, delta) catch return error.CorruptSegment;
        };
        const time_ms = try r1.var_u64();
        const offset = try r1.var_u64();
        const turn = try r1.u32_val();
        const doc_len = try r1.u32_val();
        const text_len = try r1.u32_val();
        const text_end = std.math.add(u64, text_off, text_len) catch return error.CorruptSegment;
        if (text_end > s4.len) return error.CorruptSegment;
        const txt = s4[@intCast(text_off)..@intCast(text_end)];
        if (!std.unicode.utf8ValidateSlice(txt)) return error.CorruptSegment;
        const session_sym = (try r1.sym(nstr, false)).?;
        const sleeve_sym = try r1.sym(nstr, true);
        const source_sym = (try r1.sym(nstr, false)).?;
        const tool_sym = try r1.sym(nstr, true);
        const harness = try code_to_harness(try r1.var_u64());
        const role = extract.Role.from_code(try r1.var_u64()) orelse return error.CorruptSegment;
        const nfiles_u64 = try r1.var_u64();
        if (nfiles_u64 > r1.b.len - r1.i) return error.CorruptSegment;
        const nfiles: usize = @intCast(nfiles_u64);
        const files = try gpa.alloc(u32, nfiles);
        errdefer gpa.free(files);
        for (0..nfiles) |fi| {
            files[fi] = (try r1.sym(nstr, false)).?;
        }
        docs.appendAssumeCapacity(.{
            .id = id,
            .time_ms = time_ms,
            .offset = offset,
            .turn = turn,
            .len = doc_len,
            .text_off = text_off,
            .text_len = text_len,
            .session_sym = session_sym,
            .sleeve_sym = sleeve_sym,
            .source_sym = source_sym,
            .tool_sym = tool_sym,
            .harness = harness,
            .role = role,
            .file_syms = files,
        });
        text_off = text_end;
    }
    if (!r1.done() or text_off != s4.len) return error.CorruptSegment;

    if (nterms > s2.len) return error.CorruptSegment;
    var terms = try std.ArrayList([]const u8).initCapacity(gpa, nterms);
    errdefer {
        for (terms.items) |t| gpa.free(t);
        terms.deinit(gpa);
    }
    var posts = try std.ArrayList(PostEntry).initCapacity(gpa, nterms);
    errdefer posts.deinit(gpa);

    var r2 = Reader{ .b = s2 };
    var post_off: u64 = 0;
    for (0..nterms) |ti| {
        const tlen = try r2.var_u64();
        if (tlen == 0 or tlen > s2.len) return error.CorruptSegment;
        const tb = try r2.bytes(@intCast(tlen));
        if (!std.unicode.utf8ValidateSlice(tb)) return error.CorruptSegment;
        if (ti > 0 and !std.mem.lessThan(u8, terms.items[ti - 1], tb)) return error.CorruptSegment;
        const df = try r2.u32_val();
        const plen = try r2.u32_val();
        if (df == 0) return error.CorruptSegment;
        const post_end = std.math.add(u64, post_off, plen) catch return error.CorruptSegment;
        if (post_end > s3.len) return error.CorruptSegment;
        const phits = try walk_postings(gpa, s3[@intCast(post_off)..@intCast(post_end)], ndocs);
        defer {
            for (phits) |ph| gpa.free(ph.positions);
            gpa.free(phits);
        }
        if (phits.len != df) return error.CorruptSegment;
        terms.appendAssumeCapacity(try gpa.dupe(u8, tb));
        posts.appendAssumeCapacity(.{ .off = post_off, .len = plen, .df = df });
        post_off = post_end;
    }
    if (!r2.done() or post_off != s3.len) return error.CorruptSegment;

    return .{
        .strings = try strings.toOwnedSlice(gpa),
        .docs = try docs.toOwnedSlice(gpa),
        .terms = try terms.toOwnedSlice(gpa),
        .posts = try posts.toOwnedSlice(gpa),
        .postings_bytes = try gpa.dupe(u8, s3),
        .texts_bytes = try gpa.dupe(u8, s4),
    };
}

pub const QueryMode = enum {
    all,
    any,
    phrase,
};

pub const Filters = struct {
    sleeves: []const []const u8 = &.{},
    session: ?[]const u8 = null,
    harness: ?extract.Harness = null,
    since_ms: ?u64 = null,
    until_ms: ?u64 = null,
    roles: []const extract.Role = &.{},
};

pub const HighlightSpan = struct {
    start: usize,
    end: usize,
};

pub const SearchHit = struct {
    doc_id: u64,
    seg_idx: usize,
    doc_idx: u32,
    score: f64,
    snippet_text: []const u8,
    highlights: []const HighlightSpan,
};

pub fn parse_query_terms(arena: std.mem.Allocator, text: []const u8, mode: QueryMode) ![]const []const []const u8 {
    var out: std.ArrayList([]const []const u8) = .empty;
    var quote_idx: usize = 0;
    var chunks = std.mem.splitScalar(u8, text, '"');
    const RunAcc = struct {
        arena: std.mem.Allocator,
        runs: *std.ArrayList(std.ArrayList([]const u8)),
        fn cb(ctx: @This(), tok: token.Tok) anyerror!void {
            if (tok.whole) return;
            while (ctx.runs.items.len <= tok.run) {
                try ctx.runs.append(ctx.arena, .empty);
            }
            try ctx.runs.items[tok.run].append(ctx.arena, try ctx.arena.dupe(u8, tok.term));
        }
    };

    while (chunks.next()) |chunk| : (quote_idx += 1) {
        const quoted = (quote_idx % 2) == 1;
        var runs: std.ArrayList(std.ArrayList([]const u8)) = .empty;
        const acc = RunAcc{ .arena = arena, .runs = &runs };
        _ = try token.tokenize(chunk, acc, RunAcc.cb);

        if (quoted) {
            var phrase: std.ArrayList([]const u8) = .empty;
            for (runs.items) |r| {
                for (r.items) |w| try phrase.append(arena, w);
            }
            if (phrase.items.len > 0) try out.append(arena, phrase.items);
        } else {
            for (runs.items) |r| {
                if (r.items.len > 0) try out.append(arena, r.items);
            }
        }
    }

    if (mode == .phrase) {
        var all_words: std.ArrayList([]const u8) = .empty;
        for (out.items) |q| {
            for (q) |w| try all_words.append(arena, w);
        }
        if (all_words.items.len == 0) return &.{};
        const single = try arena.alloc([]const []const u8, 1);
        single[0] = all_words.items;
        return single;
    }

    return out.items;
}

fn pass_filters(seg: *const Segment, m: SegMeta, f: Filters) bool {
    if (f.sleeves.len > 0) {
        const s_sym = m.sleeve_sym orelse return false;
        const sl_name = seg.sym(s_sym);
        var ok = false;
        for (f.sleeves) |want| {
            if (std.mem.eql(u8, sl_name, want)) {
                ok = true;
                break;
            }
        }
        if (!ok) return false;
    }
    if (f.session) |want_sess| {
        if (!std.mem.eql(u8, seg.sym(m.session_sym), want_sess)) return false;
    }
    if (f.harness) |h| {
        if (m.harness != h) return false;
    }
    if (f.since_ms) |since| {
        if (m.time_ms < since) return false;
    }
    if (f.until_ms) |until| {
        if (m.time_ms > until) return false;
    }
    if (f.roles.len > 0) {
        var ok = false;
        for (f.roles) |r| {
            if (m.role == r) {
                ok = true;
                break;
            }
        }
        if (!ok) return false;
    }
    return true;
}

const DocTf = struct { doc_idx: u32, tf: u32 };

fn match_qterm_in_seg(
    arena: std.mem.Allocator,
    seg: *const Segment,
    qterm: []const []const u8,
) ![]const DocTf {
    var res: std.ArrayList(DocTf) = .empty;
    if (qterm.len == 0) return &.{};
    if (qterm.len == 1) {
        const pe = seg.find_term(qterm[0]) orelse return &.{};
        const phits = try walk_postings(arena, seg.postings_slice(pe), @intCast(seg.docs.len));
        for (phits) |ph| {
            try res.append(arena, .{ .doc_idx = ph.doc_idx, .tf = @intCast(ph.positions.len) });
        }
        return res.items;
    }

    const first_pe = seg.find_term(qterm[0]) orelse return &.{};
    const first_hits = try walk_postings(arena, seg.postings_slice(first_pe), @intCast(seg.docs.len));
    if (first_hits.len == 0) return &.{};

    var part_hits = try arena.alloc([]const PostingHit, qterm.len);
    part_hits[0] = first_hits;
    for (qterm[1..], 1..) |term, k| {
        const pe = seg.find_term(term) orelse return &.{};
        part_hits[k] = try walk_postings(arena, seg.postings_slice(pe), @intCast(seg.docs.len));
    }

    for (first_hits) |fh| {
        var matched_count: u32 = 0;
        for (fh.positions) |p0| {
            var all_ok = true;
            for (1..qterm.len) |k| {
                const want: u32 = std.math.add(u32, p0, @intCast(k)) catch {
                    all_ok = false;
                    break;
                };
                if (!has_doc_pos(part_hits[k], fh.doc_idx, want)) {
                    all_ok = false;
                    break;
                }
            }
            if (all_ok) matched_count += 1;
        }
        if (matched_count > 0) {
            try res.append(arena, .{ .doc_idx = fh.doc_idx, .tf = matched_count });
        }
    }
    return res.items;
}

fn has_doc_pos(hits: []const PostingHit, doc_idx: u32, want_pos: u32) bool {
    var lo: usize = 0;
    var hi: usize = hits.len;
    while (lo < hi) {
        const mid = lo + (hi - lo) / 2;
        if (hits[mid].doc_idx == doc_idx) {
            var plo: usize = 0;
            var phi: usize = hits[mid].positions.len;
            while (plo < phi) {
                const pm = plo + (phi - plo) / 2;
                const pv = hits[mid].positions[pm];
                if (pv == want_pos) return true;
                if (pv < want_pos) plo = pm + 1 else phi = pm;
            }
            return false;
        } else if (hits[mid].doc_idx < doc_idx) {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    return false;
}

pub fn build_snippet(
    arena: std.mem.Allocator,
    text: []const u8,
    words: []const []const u8,
) !struct { text: []const u8, highlights: []const HighlightSpan } {
    var spans: std.ArrayList(HighlightSpan) = .empty;
    const Acc = struct {
        arena: std.mem.Allocator,
        words: []const []const u8,
        spans: *std.ArrayList(HighlightSpan),
        fn cb(ctx: @This(), tok: token.Tok) anyerror!void {
            for (ctx.words) |w| {
                if (std.mem.eql(u8, w, tok.term)) {
                    try ctx.spans.append(ctx.arena, .{ .start = tok.start, .end = tok.end });
                    break;
                }
            }
        }
    };
    _ = try token.tokenize(text, Acc{ .arena = arena, .words = words, .spans = &spans }, Acc.cb);

    std.mem.sort(HighlightSpan, spans.items, {}, struct {
        fn less(_: void, a: HighlightSpan, b: HighlightSpan) bool {
            return a.start < b.start;
        }
    }.less);

    var merged: std.ArrayList(HighlightSpan) = .empty;
    for (spans.items) |sp| {
        if (merged.items.len > 0 and sp.start <= merged.items[merged.items.len - 1].end) {
            merged.items[merged.items.len - 1].end = @max(merged.items[merged.items.len - 1].end, sp.end);
        } else {
            try merged.append(arena, sp);
        }
    }

    const lead = snippet_max_bytes / 6;
    var start: usize = 0;
    var best_n: usize = 0;
    for (merged.items, 0..) |m, i| {
        const from = m.start -| lead;
        var cnt: usize = 0;
        for (merged.items[i..]) |m2| {
            if (m2.end <= from + snippet_max_bytes) cnt += 1 else break;
        }
        if (cnt > best_n) {
            best_n = cnt;
            start = from;
        }
    }
    while (start > 0 and (text[start] & 0xc0) == 0x80) start -= 1;
    var end = @min(start + snippet_max_bytes, text.len);
    while (end > start and (text[end - 1] & 0xc0) == 0x80) end -= 1;
    if (end < text.len and (text[end] & 0xc0) == 0x80) {
        while (end > start and (text[end] & 0xc0) == 0x80) end -= 1;
    }

    const window = text[start..end];
    var flat = try arena.alloc(u8, window.len);
    for (window, 0..) |c, idx| {
        flat[idx] = if (c < 0x20 or c == ' ') ' ' else c;
    }

    var hl: std.ArrayList(HighlightSpan) = .empty;
    for (merged.items) |m| {
        if (m.start >= start and m.end <= end) {
            try hl.append(arena, .{ .start = m.start - start, .end = m.end - start });
        }
    }
    return .{ .text = flat, .highlights = hl.items };
}

pub fn bm25_search(
    arena: std.mem.Allocator,
    segs: []const Segment,
    query_text: []const u8,
    mode: QueryMode,
    filters: Filters,
    limit: usize,
) ![]SearchHit {
    if (limit == 0 or segs.len == 0) return &.{};
    const qterms = try parse_query_terms(arena, query_text, mode);
    if (qterms.len == 0) return &.{};

    var total_docs: u64 = 0;
    var total_len: u64 = 0;
    for (segs) |*s| {
        total_docs += s.docs.len;
        for (s.docs) |d| total_len += d.len;
    }
    if (total_docs == 0) return &.{};
    const n_f: f64 = @floatFromInt(total_docs);
    const avgdl: f64 = @max(@as(f64, @floatFromInt(total_len)) / n_f, 1.0);

    var per = try arena.alloc([]const []const DocTf, qterms.len);
    var dfs = try arena.alloc(u64, qterms.len);
    for (qterms, 0..) |qt, qi| {
        const seg_row = try arena.alloc([]const DocTf, segs.len);
        var df_sum: u64 = 0;
        for (segs, 0..) |*s, si| {
            seg_row[si] = try match_qterm_in_seg(arena, s, qt);
            df_sum += seg_row[si].len;
        }
        per[qi] = seg_row;
        dfs[qi] = df_sum;
    }

    const need: usize = if (mode == .any) 1 else qterms.len;
    var ranked: std.ArrayList(struct { score: f64, id: u64, si: usize, d: u32 }) = .empty;

    for (segs, 0..) |*s, si| {
        var acc: std.AutoHashMapUnmanaged(u32, struct { score: f64, hits: usize }) = .empty;
        for (0..qterms.len) |qi| {
            const df_f: f64 = @floatFromInt(dfs[qi]);
            const idf = @log(1.0 + (n_f - df_f + 0.5) / (df_f + 0.5));
            for (per[qi][si]) |m| {
                const doc_meta = s.docs[m.doc_idx];
                if (!pass_filters(s, doc_meta, filters)) continue;
                const tf_f: f64 = @floatFromInt(m.tf);
                const dl_f: f64 = @floatFromInt(doc_meta.len);
                const norm = bm25_k1 * (1.0 - bm25_b + bm25_b * dl_f / avgdl);
                const contrib = idf * tf_f * (bm25_k1 + 1.0) / (tf_f + norm);
                const gop = try acc.getOrPut(arena, m.doc_idx);
                if (!gop.found_existing) {
                    gop.value_ptr.* = .{ .score = contrib, .hits = 1 };
                } else {
                    gop.value_ptr.score += contrib;
                    gop.value_ptr.hits += 1;
                }
            }
        }
        var it = acc.iterator();
        while (it.next()) |e| {
            if (e.value_ptr.hits >= need) {
                const di = e.key_ptr.*;
                try ranked.append(arena, .{
                    .score = e.value_ptr.score,
                    .id = s.docs[di].id,
                    .si = si,
                    .d = di,
                });
            }
        }
    }

    std.mem.sort(@TypeOf(ranked.items[0]), ranked.items, {}, struct {
        fn less(_: void, a: @TypeOf(ranked.items[0]), b: @TypeOf(ranked.items[0])) bool {
            if (a.score != b.score) return a.score > b.score;
            return a.id > b.id;
        }
    }.less);

    const out_n = @min(limit, ranked.items.len);
    var words: std.ArrayList([]const u8) = .empty;
    for (qterms) |qt| {
        for (qt) |w| try words.append(arena, w);
    }

    var hits = try arena.alloc(SearchHit, out_n);
    for (ranked.items[0..out_n], 0..) |r, idx| {
        const meta = segs[r.si].docs[r.d];
        const txt = segs[r.si].doc_text(meta);
        const snip = try build_snippet(arena, txt, words.items);
        hits[idx] = .{
            .doc_id = meta.id,
            .seg_idx = r.si,
            .doc_idx = r.d,
            .score = r.score,
            .snippet_text = snip.text,
            .highlights = snip.highlights,
        };
    }
    return hits;
}
