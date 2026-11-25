const std = @import("std");
const tiger = @import("../tiger.zig");
const schema = @import("../api/schema.zig");

pub const RecentCosts = struct {
    const sample_count_max = 9;

    samples: tiger.BoundedArray(Sample, sample_count_max) = .{},

    const Sample = struct {
        at_ms: i64,
        cost: f64,
    };

    fn add(self: *RecentCosts, sample: Sample) void {
        const samples = self.samples.slice();
        var index: usize = 0;
        while (index < samples.len and
            (samples[index].at_ms > sample.at_ms or
                (samples[index].at_ms == sample.at_ms and samples[index].cost >= sample.cost))) : (index += 1)
        {}
        if (index == self.samples.buffer.len) return;
        if (self.samples.len < self.samples.buffer.len) self.samples.len += 1;
        @memmove(self.samples.buffer[index + 1 .. self.samples.len], self.samples.buffer[index .. self.samples.len - 1]);
        self.samples.buffer[index] = sample;
    }

    pub fn median(self: *RecentCosts) ?f64 {
        const samples = self.samples.slice_mut();
        if (samples.len == 0) return null;
        std.mem.sort(Sample, samples, {}, struct {
            fn less_than(_: void, lhs: Sample, rhs: Sample) bool {
                return lhs.cost < rhs.cost;
            }
        }.less_than);
        return samples[(samples.len - 1) / 2].cost / 2 + samples[samples.len / 2].cost / 2;
    }
};

const FileEntry = struct {
    mu: std.Io.Mutex = .init,
    next_seq: u64 = 1,
    path: []const u8,
};

pub const TranscriptStore = struct {
    gpa: std.mem.Allocator,
    io: std.Io,
    dir_path: []const u8,
    map_mu: std.Io.Mutex = .init,
    entries: std.StringHashMapUnmanaged(*FileEntry) = .empty,

    pub fn init(gpa: std.mem.Allocator, io: std.Io, dir_path: []const u8) !TranscriptStore {
        try std.Io.Dir.cwd().createDirPath(io, dir_path);
        return .{
            .gpa = gpa,
            .io = io,
            .dir_path = try gpa.dupe(u8, dir_path),
        };
    }

    pub fn deinit(self: *TranscriptStore) void {
        var it = self.entries.iterator();
        while (it.next()) |e| {
            self.gpa.free(e.key_ptr.*);
            self.gpa.free(e.value_ptr.*.path);
            self.gpa.destroy(e.value_ptr.*);
        }
        self.entries.deinit(self.gpa);
        self.gpa.free(self.dir_path);
    }

    fn get_or_open(self: *TranscriptStore, sleeve_id: []const u8) !*FileEntry {
        self.map_mu.lockUncancelable(self.io);
        defer self.map_mu.unlock(self.io);
        if (self.entries.get(sleeve_id)) |fe| return fe;

        const path = try std.fmt.allocPrint(self.gpa, "{s}/{s}.jsonl", .{ self.dir_path, sleeve_id });
        errdefer self.gpa.free(path);

        var max_seq: u64 = 0;
        if (std.Io.Dir.cwd().readFileAlloc(self.io, path, self.gpa, .limited(64 << 20))) |raw| {
            defer self.gpa.free(raw);
            var lines = std.mem.splitScalar(u8, raw, '\n');
            while (lines.next()) |ln| {
                const tr = std.mem.trim(u8, ln, " \t\r");
                if (tr.len == 0) continue;
                const parsed = std.json.parseFromSlice(std.json.Value, self.gpa, tr, .{}) catch continue;
                defer parsed.deinit();
                if (parsed.value == .object) {
                    if (parsed.value.object.get("seq")) |sv| {
                        if (sv == .integer and sv.integer > 0) {
                            const s: u64 = @intCast(sv.integer);
                            if (s > max_seq) max_seq = s;
                        }
                    }
                }
            }
        } else |_| {}

        const fe = try self.gpa.create(FileEntry);
        fe.* = .{
            .next_seq = max_seq + 1,
            .path = path,
        };
        const kdup = try self.gpa.dupe(u8, sleeve_id);
        try self.entries.put(self.gpa, kdup, fe);
        return fe;
    }

    pub fn append(
        self: *TranscriptStore,
        sleeve_id: []const u8,
        g: u64,
        turn: u64,
        at_ms: i64,
        entry: schema.Entry,
    ) !schema.ChatEntry {
        const fe = try self.get_or_open(sleeve_id);
        fe.mu.lockUncancelable(self.io);
        defer fe.mu.unlock(self.io);

        const seq = fe.next_seq;
        fe.next_seq += 1;

        const chat_entry: schema.ChatEntry = .{
            .seq = seq,
            .g = g,
            .turn = turn,
            .at_ms = .{ .ms = at_ms },
            .entry = entry,
        };

        const line = try schema.encode_alloc(self.gpa, chat_entry);
        defer self.gpa.free(line);

        const with_nl = try self.gpa.alloc(u8, line.len + 1);
        defer self.gpa.free(with_nl);
        @memcpy(with_nl[0..line.len], line);
        with_nl[line.len] = '\n';

        var f = std.Io.Dir.cwd().openFile(self.io, fe.path, .{ .mode = .read_write }) catch
            try std.Io.Dir.cwd().createFile(self.io, fe.path, .{ .read = true, .truncate = false });
        defer f.close(self.io);
        const st = try f.stat(self.io);
        try f.writePositionalAll(self.io, with_nl, st.size);
        return chat_entry;
    }

    pub fn read(
        self: *TranscriptStore,
        arena: std.mem.Allocator,
        sleeve_id: []const u8,
        from_seq: ?u64,
        limit: ?u64,
    ) !schema.Transcript {
        const fe = try self.get_or_open(sleeve_id);
        fe.mu.lockUncancelable(self.io);
        defer fe.mu.unlock(self.io);

        const raw = std.Io.Dir.cwd().readFileAlloc(self.io, fe.path, arena, .limited(64 << 20)) catch {
            return .{ .sleeve_id = try arena.dupe(u8, sleeve_id) };
        };

        const min_seq = from_seq orelse 0;
        const max_n: usize = @intCast(limit orelse 500);
        var list: std.ArrayList(schema.ChatEntry) = .empty;
        var has_more = false;

        var lines = std.mem.splitScalar(u8, raw, '\n');
        while (lines.next()) |raw_ln| {
            const tr = std.mem.trim(u8, raw_ln, " \t\r");
            if (tr.len == 0) continue;
            const ce = schema.decode_arena(schema.ChatEntry, arena, tr) catch continue;
            if (ce.seq < min_seq) continue;
            if (list.items.len >= max_n) {
                has_more = true;
                break;
            }
            try list.append(arena, ce);
        }

        return .{
            .sleeve_id = try arena.dupe(u8, sleeve_id),
            .entries = list.items,
            .more = has_more,
        };
    }

    pub fn path_for(self: *TranscriptStore, arena: std.mem.Allocator, sleeve_id: []const u8) ![]u8 {
        return std.fmt.allocPrint(arena, "{s}/{s}.jsonl", .{ self.dir_path, sleeve_id });
    }

    pub fn collect_costs(self: *TranscriptStore, arena: std.mem.Allocator, sleeve_id: []const u8, costs: *RecentCosts) !void {
        const path = try self.path_for(arena, sleeve_id);
        var file = std.Io.Dir.cwd().openFile(self.io, path, .{}) catch |err| switch (err) {
            error.FileNotFound => return,
            else => return err,
        };
        defer file.close(self.io);
        var line_arena = std.heap.ArenaAllocator.init(self.gpa);
        defer line_arena.deinit();
        var buffer: [16 * 1024]u8 = undefined;
        var reader = file.reader(self.io, &buffer);
        while (true) {
            const line = reader.interface.takeDelimiter('\n') catch |err| switch (err) {
                error.StreamTooLong => {
                    _ = reader.interface.discardDelimiterInclusive('\n') catch break;
                    continue;
                },
                else => return err,
            } orelse break;
            if (std.mem.find(u8, line, "\"turn_settled\"") == null) continue;
            defer _ = line_arena.reset(.retain_capacity);
            const chat = schema.decode_arena(schema.ChatEntry, line_arena.allocator(), line) catch continue;
            if (chat.entry != .turn_settled or chat.entry.turn_settled.stop != .end_turn) continue;
            const cost = chat.entry.turn_settled.usage.cost_usd orelse continue;
            if (std.math.isFinite(cost) and cost >= 0) costs.add(.{ .at_ms = chat.at_ms.ms, .cost = cost });
        }
    }
};
