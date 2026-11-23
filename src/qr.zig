const std = @import("std");
const tiger = @import("tiger.zig");

pub const Ecc = enum {
    low,
    medium,

    fn ordinal(self: Ecc) usize {
        return switch (self) {
            .low => 0,
            .medium => 1,
        };
    }

    fn format_bits(self: Ecc) u32 {
        return switch (self) {
            .low => 1,
            .medium => 0,
        };
    }
};

pub const min_version: u8 = 1;
pub const max_version: u8 = 40;
const max_side: usize = 40 * 4 + 17;
const max_cells: usize = max_side * max_side;
const bitset_words: usize = (max_cells + 63) / 64;
const max_raw_bytes: usize = 3706;

const penalty_n1: i32 = 3;
const penalty_n2: i32 = 3;
const penalty_n3: i32 = 40;
const penalty_n4: i32 = 10;

const ecc_codewords_per_block = [2][41]u8{
    .{ 0, 7, 10, 15, 20, 26, 18, 20, 24, 30, 18, 20, 24, 26, 30, 22, 24, 28, 30, 28, 28, 28, 28, 30, 30, 26, 28, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30 },
    .{ 0, 10, 16, 26, 18, 24, 16, 18, 22, 22, 26, 30, 22, 22, 24, 24, 28, 28, 26, 26, 26, 26, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28 },
};

const ecc_blocks = [2][41]u8{
    .{ 0, 1, 1, 1, 1, 1, 2, 2, 2, 2, 4, 4, 4, 4, 4, 6, 6, 6, 6, 7, 8, 8, 9, 9, 10, 12, 12, 12, 13, 14, 15, 16, 17, 18, 19, 19, 20, 21, 22, 24, 25 },
    .{ 0, 1, 1, 1, 2, 2, 4, 4, 4, 5, 5, 5, 8, 9, 9, 10, 10, 11, 13, 14, 16, 17, 17, 18, 20, 21, 23, 25, 26, 28, 29, 31, 33, 35, 37, 38, 40, 43, 45, 47, 49 },
};

const BitSet = struct {
    words: [bitset_words]u64 = @splat(0),

    fn get(self: *const BitSet, i: usize) bool {
        tiger.assert(i < max_cells);
        return (self.words[i / 64] & (@as(u64, 1) << @intCast(i % 64))) != 0;
    }

    fn set(self: *BitSet, i: usize, value: bool) void {
        tiger.assert(i < max_cells);
        const mask = @as(u64, 1) << @intCast(i % 64);
        if (value) {
            self.words[i / 64] |= mask;
        } else {
            self.words[i / 64] &= ~mask;
        }
    }

    fn toggle(self: *BitSet, i: usize) void {
        tiger.assert(i < max_cells);
        self.words[i / 64] ^= @as(u64, 1) << @intCast(i % 64);
    }

    fn count_ones(self: *const BitSet, cells: usize) i32 {
        const words_used = (cells + 63) / 64;
        var sum: i32 = 0;
        for (self.words[0..words_used]) |w| {
            sum += @intCast(@popCount(w));
        }
        return sum;
    }
};

pub const QrCode = struct {
    version: u8,
    size: i32,
    mask: u8,
    modules: BitSet,

    pub fn dark(self: *const QrCode, x: i32, y: i32) bool {
        if (x < 0 or x >= self.size or y < 0 or y >= self.size) return false;
        const ux: usize = @intCast(x);
        const uy: usize = @intCast(y);
        const usize_side: usize = @intCast(self.size);
        return self.modules.get(uy * usize_side + ux);
    }

    pub fn to_terminal_alloc(self: *const QrCode, gpa: std.mem.Allocator, quiet: u8, ansi: bool) error{OutOfMemory}![]u8 {
        var out: std.ArrayList(u8) = .empty;
        errdefer out.deinit(gpa);
        const q: i32 = @intCast(quiet);
        const lo: i32 = -q;
        const hi: i32 = self.size + q;
        var y: i32 = lo;
        while (y < hi) : (y += 2) {
            if (ansi) try out.appendSlice(gpa, "\x1b[97;40m");
            var x: i32 = lo;
            while (x < hi) : (x += 1) {
                const top_light = y < hi and !self.dark(x, y);
                const bot_light = (y + 1) < hi and !self.dark(x, y + 1);
                if (top_light and bot_light) {
                    try out.appendSlice(gpa, "\xe2\x96\x88");
                } else if (top_light and !bot_light) {
                    try out.appendSlice(gpa, "\xe2\x96\x80");
                } else if (!top_light and bot_light) {
                    try out.appendSlice(gpa, "\xe2\x96\x84");
                } else {
                    try out.append(gpa, ' ');
                }
            }
            if (ansi) try out.appendSlice(gpa, "\x1b[0m");
            try out.append(gpa, '\n');
        }
        return out.toOwnedSlice(gpa);
    }
};

pub fn max_bytes(ecc: Ecc) usize {
    return (data_codewords(max_version, ecc) * 8 - 4 - 16) / 8;
}

pub fn encode(data: []const u8, ecc: Ecc) error{TooLong}!QrCode {
    var version: u8 = min_version;
    const used_bits = while (true) {
        const count_bits = byte_count_bits(version);
        const fits_count = data.len < (@as(usize, 1) << @intCast(count_bits));
        const used = 4 + @as(usize, count_bits) + data.len *| 8;
        if (fits_count and used <= data_codewords(version, ecc) * 8) break used;
        if (version == max_version) return error.TooLong;
        version += 1;
    };

    const capacity = data_codewords(version, ecc) * 8;
    var bits: Bits = .{};
    bits.push(0b0100, 4);
    bits.push(@intCast(data.len), byte_count_bits(version));
    for (data) |b| bits.push(b, 8);
    tiger.assert(bits.len == used_bits);
    bits.push(0, @intCast(@min(capacity - bits.len, 4)));
    bits.push(0, @intCast((0 -% bits.len) & 7));
    const pad_bytes = [2]u8{ 0xEC, 0x11 };
    var pad_idx: usize = 0;
    while (bits.len < capacity) : (pad_idx = (pad_idx + 1) % 2) {
        bits.push(pad_bytes[pad_idx], 8);
    }
    return build_qr(version, ecc, bits.bytes.slice());
}

const Bits = struct {
    bytes: tiger.BoundedArray(u8, max_raw_bytes) = tiger.BoundedArray(u8, max_raw_bytes).init(),
    len: usize = 0,

    fn push(self: *Bits, val: u32, n: u8) void {
        tiger.assert(n <= 31);
        tiger.assert((val >> @intCast(n)) == 0);
        var i: usize = n;
        while (i > 0) {
            i -= 1;
            if (self.len % 8 == 0) {
                self.bytes.append_assume_capacity(0);
            }
            if (((val >> @intCast(i)) & 1) != 0) {
                const last = self.bytes.len - 1;
                self.bytes.buffer[last] |= @as(u8, 0x80) >> @intCast(self.len % 8);
            }
            self.len += 1;
        }
    }
};

fn byte_count_bits(version: u8) u8 {
    return if (version <= 9) 8 else 16;
}

fn bit(x: u32, i: i32) bool {
    return ((x >> @intCast(i)) & 1) != 0;
}

fn raw_data_modules(version: u8) usize {
    const v: usize = version;
    var result: usize = (16 * v + 128) * v + 64;
    if (v >= 2) {
        const n = v / 7 + 2;
        result -= (25 * n - 10) * n - 55;
        if (v >= 7) result -= 36;
    }
    return result;
}

fn table_lookup(t: *const [2][41]u8, version: u8, ecc: Ecc) usize {
    return t[ecc.ordinal()][version];
}

fn data_codewords(version: u8, ecc: Ecc) usize {
    return raw_data_modules(version) / 8 -
        table_lookup(&ecc_codewords_per_block, version, ecc) * table_lookup(&ecc_blocks, version, ecc);
}

fn build_qr(version: u8, ecc: Ecc, data: []const u8) QrCode {
    const size: i32 = @as(i32, version) * 4 + 17;
    var b: Builder = .{ .size = size };
    b.draw_function_patterns(version, ecc);
    var all: [max_raw_bytes]u8 = undefined;
    const all_len = add_ecc_and_interleave(version, ecc, data, &all);
    b.draw_codewords(all[0..all_len]);

    var best_penalty: i32 = std.math.maxInt(i32);
    var best_mask: u8 = 0;
    for (0..8) |m_idx| {
        const mask: u8 = @intCast(m_idx);
        b.apply_mask(mask);
        b.draw_format_bits(ecc, mask);
        const penalty = b.penalty_score();
        if (penalty < best_penalty) {
            best_penalty = penalty;
            best_mask = mask;
        }
        b.apply_mask(mask);
    }
    b.apply_mask(best_mask);
    b.draw_format_bits(ecc, best_mask);
    return .{
        .version = version,
        .size = size,
        .mask = best_mask,
        .modules = b.modules,
    };
}

const Builder = struct {
    size: i32,
    modules: BitSet = .{},
    function: BitSet = .{},

    fn set_function(self: *Builder, x: i32, y: i32, dark: bool) void {
        const usize_side: usize = @intCast(self.size);
        const i = @as(usize, @intCast(y)) * usize_side + @as(usize, @intCast(x));
        self.modules.set(i, dark);
        self.function.set(i, true);
    }

    fn draw_function_patterns(self: *Builder, version: u8, ecc: Ecc) void {
        var i: i32 = 0;
        while (i < self.size) : (i += 1) {
            self.set_function(6, i, @mod(i, 2) == 0);
            self.set_function(i, 6, @mod(i, 2) == 0);
        }
        self.draw_finder(3, 3);
        self.draw_finder(self.size - 4, 3);
        self.draw_finder(3, self.size - 4);
        var pos_buf: [7]i32 = undefined;
        const pos = alignment_positions(version, self.size, &pos_buf);
        for (pos, 0..) |py, idx_y| {
            for (pos, 0..) |px, idx_x| {
                const corner = (idx_y == 0 and idx_x == 0) or
                    (idx_y == 0 and idx_x == pos.len - 1) or
                    (idx_y == pos.len - 1 and idx_x == 0);
                if (!corner) self.draw_alignment(py, px);
            }
        }
        self.draw_format_bits(ecc, 0);
        self.draw_version(version);
    }

    fn draw_format_bits(self: *Builder, ecc: Ecc, mask: u8) void {
        const data = (ecc.format_bits() << 3) | @as(u32, mask);
        var rem = data;
        for (0..10) |_| {
            rem = (rem << 1) ^ ((rem >> 9) * 0x537);
        }
        const bits = ((data << 10) | rem) ^ 0x5412;
        for (0..6) |idx| {
            self.set_function(8, @intCast(idx), bit(bits, @intCast(idx)));
        }
        self.set_function(8, 7, bit(bits, 6));
        self.set_function(8, 8, bit(bits, 7));
        self.set_function(7, 8, bit(bits, 8));
        for (9..15) |idx| {
            self.set_function(14 - @as(i32, @intCast(idx)), 8, bit(bits, @intCast(idx)));
        }
        for (0..8) |idx| {
            self.set_function(self.size - 1 - @as(i32, @intCast(idx)), 8, bit(bits, @intCast(idx)));
        }
        for (8..15) |idx| {
            self.set_function(8, self.size - 15 + @as(i32, @intCast(idx)), bit(bits, @intCast(idx)));
        }
        self.set_function(8, self.size - 8, true);
    }

    fn draw_version(self: *Builder, version: u8) void {
        if (version < 7) return;
        const data: u32 = version;
        var rem = data;
        for (0..12) |_| {
            rem = (rem << 1) ^ ((rem >> 11) * 0x1F25);
        }
        const bits = (data << 12) | rem;
        for (0..18) |idx| {
            const d = bit(bits, @intCast(idx));
            const a = self.size - 11 + @as(i32, @intCast(idx % 3));
            const b_pos: i32 = @intCast(idx / 3);
            self.set_function(a, b_pos, d);
            self.set_function(b_pos, a, d);
        }
    }

    fn draw_finder(self: *Builder, x: i32, y: i32) void {
        var dy: i32 = -4;
        while (dy <= 4) : (dy += 1) {
            var dx: i32 = -4;
            while (dx <= 4) : (dx += 1) {
                const xx = x + dx;
                const yy = y + dy;
                if (xx >= 0 and xx < self.size and yy >= 0 and yy < self.size) {
                    const dist = @max(@abs(dx), @abs(dy));
                    self.set_function(xx, yy, dist != 2 and dist != 4);
                }
            }
        }
    }

    fn draw_alignment(self: *Builder, x: i32, y: i32) void {
        var dy: i32 = -2;
        while (dy <= 2) : (dy += 1) {
            var dx: i32 = -2;
            while (dx <= 2) : (dx += 1) {
                self.set_function(x + dx, y + dy, @max(@abs(dx), @abs(dy)) != 1);
            }
        }
    }

    fn draw_codewords(self: *Builder, data: []const u8) void {
        var i: usize = 0;
        var right: i32 = self.size - 1;
        while (right >= 1) : (right -= 2) {
            if (right == 6) right = 5;
            var vert: i32 = 0;
            while (vert < self.size) : (vert += 1) {
                var j: i32 = 0;
                while (j < 2) : (j += 1) {
                    const x = right - j;
                    const upward = ((right + 1) & 2) == 0;
                    const y = if (upward) self.size - 1 - vert else vert;
                    const idx = @as(usize, @intCast(y)) * @as(usize, @intCast(self.size)) + @as(usize, @intCast(x));
                    if (!self.function.get(idx) and i < data.len * 8) {
                        self.modules.set(idx, bit(data[i >> 3], 7 - @as(i32, @intCast(i & 7))));
                        i += 1;
                    }
                }
            }
        }
        tiger.assert(i == data.len * 8);
    }

    fn apply_mask(self: *Builder, mask: u8) void {
        var y: i32 = 0;
        while (y < self.size) : (y += 1) {
            var x: i32 = 0;
            while (x < self.size) : (x += 1) {
                const invert = switch (mask) {
                    0 => @mod(x + y, 2) == 0,
                    1 => @mod(y, 2) == 0,
                    2 => @mod(x, 3) == 0,
                    3 => @mod(x + y, 3) == 0,
                    4 => @mod(@divFloor(x, 3) + @divFloor(y, 2), 2) == 0,
                    5 => @mod(x * y, 2) + @mod(x * y, 3) == 0,
                    6 => @mod(@mod(x * y, 2) + @mod(x * y, 3), 2) == 0,
                    else => @mod(@mod(x + y, 2) + @mod(x * y, 3), 2) == 0,
                };
                const idx = @as(usize, @intCast(y)) * @as(usize, @intCast(self.size)) + @as(usize, @intCast(x));
                if (invert and !self.function.get(idx)) {
                    self.modules.toggle(idx);
                }
            }
        }
    }

    fn module(self: *const Builder, x: i32, y: i32) bool {
        return self.modules.get(@as(usize, @intCast(y)) * @as(usize, @intCast(self.size)) + @as(usize, @intCast(x)));
    }

    fn penalty_score(self: *const Builder) i32 {
        var result: i32 = 0;
        for ([2]bool{ false, true }) |transpose| {
            var a: i32 = 0;
            while (a < self.size) : (a += 1) {
                var color = false;
                var run: i32 = 0;
                var history: FinderPenalty = .{ .size = self.size };
                var b: i32 = 0;
                while (b < self.size) : (b += 1) {
                    const m = if (transpose) self.module(a, b) else self.module(b, a);
                    if (m == color) {
                        run += 1;
                        if (run == 5) {
                            result += penalty_n1;
                        } else if (run > 5) {
                            result += 1;
                        }
                    } else {
                        history.add(run);
                        if (!color) result += history.count_patterns() * penalty_n3;
                        color = m;
                        run = 1;
                    }
                }
                result += history.terminate_and_count(color, run) * penalty_n3;
            }
        }
        var y: i32 = 0;
        while (y < self.size - 1) : (y += 1) {
            var x: i32 = 0;
            while (x < self.size - 1) : (x += 1) {
                const c = self.module(x, y);
                if (c == self.module(x + 1, y) and c == self.module(x, y + 1) and c == self.module(x + 1, y + 1)) {
                    result += penalty_n2;
                }
            }
        }
        const side: usize = @intCast(self.size);
        const total = self.size * self.size;
        const dark = self.modules.count_ones(side * side);
        const diff = @as(i32, @intCast(@abs(dark * 20 - total * 10)));
        const k = @divTrunc(diff + total - 1, total) - 1;
        return result + k * penalty_n4;
    }
};

const FinderPenalty = struct {
    size: i32,
    history: [7]i32 = @splat(0),

    fn add(self: *FinderPenalty, raw_run: i32) void {
        var run = raw_run;
        if (self.history[0] == 0) run += self.size;
        var idx: usize = 6;
        while (idx > 0) : (idx -= 1) {
            self.history[idx] = self.history[idx - 1];
        }
        self.history[0] = run;
    }

    fn count_patterns(self: *const FinderPenalty) i32 {
        const h = self.history;
        const n = h[1];
        const core = n > 0 and h[2] == n and h[3] == n * 3 and h[4] == n and h[5] == n;
        const a: i32 = if (core and h[0] >= n * 4 and h[6] >= n) 1 else 0;
        const b: i32 = if (core and h[6] >= n * 4 and h[0] >= n) 1 else 0;
        return a + b;
    }

    fn terminate_and_count(self: *FinderPenalty, color: bool, raw_run: i32) i32 {
        var run = raw_run;
        if (color) {
            self.add(run);
            run = 0;
        }
        run += self.size;
        self.add(run);
        return self.count_patterns();
    }
};

fn alignment_positions(version: u8, size: i32, out: *[7]i32) []const i32 {
    if (version == 1) return out[0..0];
    const n: usize = @as(usize, version) / 7 + 2;
    const step: i32 = if (version == 32)
        26
    else
        @divTrunc(@as(i32, version) * 4 + @as(i32, @intCast(n)) * 2 + 1, @as(i32, @intCast(n)) * 2 - 2) * 2;
    var i: usize = 0;
    while (i < n - 1) : (i += 1) {
        out[n - 1 - i] = size - 7 - @as(i32, @intCast(i)) * step;
    }
    out[0] = 6;
    return out[0..n];
}

fn add_ecc_and_interleave(version: u8, ecc: Ecc, data: []const u8, out: *[max_raw_bytes]u8) usize {
    tiger.assert(data.len == data_codewords(version, ecc));
    const blocks_n = table_lookup(&ecc_blocks, version, ecc);
    const ecc_len = table_lookup(&ecc_codewords_per_block, version, ecc);
    const raw = raw_data_modules(version) / 8;
    const short_n = blocks_n - (raw % blocks_n);
    const short_len = raw / blocks_n;
    var divisor: [30]u8 = undefined;
    rs_divisor(ecc_len, divisor[0..ecc_len]);

    var blocks: [70][155]u8 = undefined;
    var k: usize = 0;
    for (0..blocks_n) |i| {
        const data_len = short_len - ecc_len + @as(usize, @intFromBool(i >= short_n));
        @memcpy(blocks[i][0..data_len], data[k .. k + data_len]);
        k += data_len;
        var rem: [30]u8 = undefined;
        rs_remainder(blocks[i][0..data_len], divisor[0..ecc_len], rem[0..ecc_len]);
        var write_idx = data_len;
        if (i < short_n) {
            blocks[i][write_idx] = 0;
            write_idx += 1;
        }
        @memcpy(blocks[i][write_idx .. write_idx + ecc_len], rem[0..ecc_len]);
    }

    var out_len: usize = 0;
    for (0..short_len + 1) |i| {
        for (0..blocks_n) |j| {
            if (i != short_len - ecc_len or j >= short_n) {
                out[out_len] = blocks[j][i];
                out_len += 1;
            }
        }
    }
    tiger.assert(out_len == raw);
    return out_len;
}

fn rs_divisor(degree: usize, result: []u8) void {
    @memset(result, 0);
    result[degree - 1] = 1;
    var root: u8 = 1;
    for (0..degree) |_| {
        for (0..degree) |j| {
            result[j] = rs_multiply(result[j], root);
            if (j + 1 < degree) result[j] ^= result[j + 1];
        }
        root = rs_multiply(root, 0x02);
    }
}

fn rs_remainder(data: []const u8, divisor: []const u8, result: []u8) void {
    @memset(result, 0);
    for (data) |b| {
        const factor = b ^ result[0];
        for (0..divisor.len - 1) |i| {
            result[i] = result[i + 1];
        }
        result[divisor.len - 1] = 0;
        for (0..divisor.len) |i| {
            result[i] ^= rs_multiply(divisor[i], factor);
        }
    }
}

fn rs_multiply(x: u8, y: u8) u8 {
    var z: u8 = 0;
    var i: usize = 8;
    while (i > 0) {
        i -= 1;
        z = (z << 1) ^ ((z >> 7) * 0x1D);
        z ^= ((y >> @intCast(i)) & 1) * x;
    }
    return z;
}
