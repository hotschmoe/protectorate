const std = @import("std");
const tiger = @import("../tiger.zig");

pub const max_run: usize = 200;
pub const max_part: usize = 48;
pub const max_humps: usize = 6;

pub const Tok = struct {
    term: []const u8,
    pos: u32,
    start: usize,
    end: usize,
    whole: bool,
    run: u32,
};

const Span = struct { start: usize, end: usize };

fn is_sep(cp: u21) bool {
    return cp == '_' or cp == '.' or cp == '/' or cp == '-';
}

fn is_ascii_alnum(c: u8) bool {
    return std.ascii.isAlphanumeric(c);
}

fn is_ascii_upper(c: u8) bool {
    return c >= 'A' and c <= 'Z';
}

fn is_ascii_lower(c: u8) bool {
    return c >= 'a' and c <= 'z';
}

fn is_word_cp(cp: u21) bool {
    if (cp <= 0x7f) {
        const c: u8 = @intCast(cp);
        return is_ascii_alnum(c) or c == '_' or c == '.' or c == '/' or c == '-';
    }
    return is_unicode_alnum(cp);
}

fn is_unicode_alnum(cp: u21) bool {
    if (cp < 0x80) return is_ascii_alnum(@intCast(cp));
    return (cp >= 0x00C0 and cp <= 0x02AF and cp != 0x00D7 and cp != 0x00F7) or
        (cp >= 0x0370 and cp <= 0x1FFF) or
        (cp >= 0x2C00 and cp <= 0xD7AF) or
        (cp >= 0xF900 and cp <= 0xFFFD);
}

fn next_cp(text: []const u8, idx: usize) ?struct { cp: u21, len: usize } {
    if (idx >= text.len) return null;
    const len = std.unicode.utf8ByteSequenceLength(text[idx]) catch {
        return .{ .cp = text[idx], .len = 1 };
    };
    if (idx + len > text.len) return .{ .cp = text[idx], .len = 1 };
    const cp = std.unicode.utf8Decode(text[idx .. idx + len]) catch {
        return .{ .cp = text[idx], .len = 1 };
    };
    return .{ .cp = cp, .len = len };
}

fn lower_into(src: []const u8, buf: []u8) []const u8 {
    var out_len: usize = 0;
    var i: usize = 0;
    while (i < src.len and out_len < buf.len) {
        const n = next_cp(src, i) orelse break;
        if (n.cp <= 0x7f) {
            buf[out_len] = std.ascii.toLower(@intCast(n.cp));
            out_len += 1;
        } else {
            const copy_len = @min(n.len, buf.len - out_len);
            @memcpy(buf[out_len .. out_len + copy_len], src[i .. i + copy_len]);
            out_len += copy_len;
        }
        i += n.len;
    }
    return buf[0..out_len];
}

fn humps(seg: []const u8, out: *[max_humps]Span) usize {
    if (seg.len == 0) return 0;
    var count: usize = 0;
    var start: usize = 0;
    var prev_c: u8 = seg[0];
    var i: usize = 1;
    while (i < seg.len) : (i += 1) {
        const c = seg[i];
        const next_lower = i + 1 < seg.len and is_ascii_lower(seg[i + 1]);
        const boundary = is_ascii_upper(c) and (is_ascii_lower(prev_c) or
            std.ascii.isDigit(prev_c) or
            (is_ascii_upper(prev_c) and next_lower));
        if (boundary) {
            if (count == max_humps - 1) {
                out[0] = .{ .start = 0, .end = seg.len };
                return 1;
            }
            out[count] = .{ .start = start, .end = i };
            count += 1;
            start = i;
        }
        prev_c = c;
    }
    out[count] = .{ .start = start, .end = seg.len };
    return count + 1;
}

pub fn tokenize(text: []const u8, ctx: anytype, comptime cb: fn (@TypeOf(ctx), Tok) anyerror!void) !u32 {
    var pos: u32 = 0;
    var run: u32 = 0;
    var parts: [max_humps]Span = undefined;
    var lower_buf: [max_part * 4]u8 = undefined;

    var i: usize = 0;
    while (i < text.len) {
        const first = next_cp(text, i) orelse break;
        if (!is_word_cp(first.cp)) {
            i += first.len;
            continue;
        }
        const run_start = i;
        var run_end = i;
        while (run_end < text.len) {
            const cur = next_cp(text, run_end) orelse break;
            if (!is_word_cp(cur.cp)) break;
            run_end += cur.len;
        }
        i = run_end;
        if (run_end - run_start <= max_run) {
            var seg_start: ?usize = null;
            var j = run_start;
            while (j <= run_end) {
                const sep = if (j == run_end) true else blk: {
                    const ch = next_cp(text, j) orelse break :blk true;
                    break :blk is_sep(ch.cp);
                };
                if (sep) {
                    if (seg_start) |sa| {
                        try emit_segment(text, sa, j, run, &pos, &lower_buf, &parts, ctx, cb);
                        seg_start = null;
                    }
                } else if (seg_start == null) {
                    seg_start = j;
                }
                if (j == run_end) break;
                const step = next_cp(text, j) orelse break;
                j += step.len;
            }
        }
        run += 1;
    }
    return pos;
}

fn emit_segment(
    text: []const u8,
    a: usize,
    b: usize,
    run: u32,
    pos: *u32,
    lower_buf: []u8,
    parts: *[max_humps]Span,
    ctx: anytype,
    comptime cb: fn (@TypeOf(ctx), Tok) anyerror!void,
) !void {
    const seg = text[a..b];
    const count = humps(seg, parts);
    const first_pos = pos.*;
    var emitted: usize = 0;
    for (parts[0..count]) |p| {
        if (p.end - p.start > max_part) continue;
        const term = lower_into(seg[p.start..p.end], lower_buf);
        try cb(ctx, .{
            .term = term,
            .pos = pos.*,
            .start = a + p.start,
            .end = a + p.end,
            .whole = false,
            .run = run,
        });
        pos.* += 1;
        emitted += 1;
    }
    if (count > 1 and emitted > 0 and seg.len <= max_part) {
        const term = lower_into(seg, lower_buf);
        try cb(ctx, .{
            .term = term,
            .pos = first_pos,
            .start = a,
            .end = b,
            .whole = true,
            .run = run,
        });
    }
}
