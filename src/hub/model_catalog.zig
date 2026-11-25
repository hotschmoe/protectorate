const std = @import("std");
const schema = @import("../api/schema.zig");
const json = @import("../json_util.zig");

const ttl_ms = 60_000;
const probe_ms = 2_500;
const output_max = 512 * 1024;
const model_max = 512;
const codex_page_max = 8;
const claude_models = [_]schema.ModelChoice{
    .{ .id = "sonnet", .label = "Sonnet" },
    .{ .id = "opus", .label = "Opus" },
    .{ .id = "haiku", .label = "Haiku" },
    .{ .id = "fable", .label = "Fable" },
    .{ .id = "opusplan", .label = "Opus plan" },
};
const effort_standard = &[_][]const u8{ "low", "medium", "high", "xhigh" };
const effort_claude = &[_][]const u8{ "low", "medium", "high", "xhigh", "max" };
const effort_pi = &[_][]const u8{ "off", "minimal", "low", "medium", "high", "xhigh", "max" };

pub const Cache = struct {
    mu: std.Io.Mutex = .init,
    arena: ?std.heap.ArenaAllocator = null,
    rows: []const schema.HarnessModels = &.{},
    expires_ms: i64 = 0,

    pub fn deinit(self: *Cache) void {
        if (self.arena) |*arena| arena.deinit();
    }

    pub fn encode(
        self: *Cache,
        gpa: std.mem.Allocator,
        io: std.Io,
        env: *const std.process.Environ.Map,
        harness: ?schema.Harness,
    ) ![]u8 {
        self.mu.lockUncancelable(io);
        defer self.mu.unlock(io);
        if (self.arena == null or std.Io.Clock.awake.now(io).toMilliseconds() >= self.expires_ms) {
            var arena = std.heap.ArenaAllocator.init(gpa);
            errdefer arena.deinit();
            const a = arena.allocator();
            const rows = try a.alloc(schema.HarnessModels, 6);
            rows[0] = try claude(a, io, env);
            rows[1] = codex(a, io, env) catch .{ .harness = .codex };
            rows[2] = grok(a, io, env) catch .{ .harness = .grok, .efforts = effort_standard };
            rows[3] = pi(a, io, env) catch .{ .harness = .pi_vendor, .efforts = effort_pi };
            rows[4] = rows[3];
            rows[4].harness = .pi_local;
            rows[5] = antigravity(a, io, env) catch .{ .harness = .antigravity, .efforts = effort_claude };
            self.deinit();
            self.arena = arena;
            self.rows = rows;
            self.expires_ms = std.Io.Clock.awake.now(io).toMilliseconds() + ttl_ms;
        }
        var rows = self.rows;
        if (harness) |target| {
            for (self.rows, 0..) |row, i| {
                if (row.harness != target) continue;
                rows = self.rows[i..][0..1];
                break;
            }
        }
        return schema.encode_alloc(gpa, schema.ModelCatalog{ .harnesses = rows });
    }
};

fn read_settings(a: std.mem.Allocator, io: std.Io, root: []const u8, relative: []const u8) std.json.Value {
    const path = std.fs.path.join(a, &.{ root, relative }) catch return .null;
    const raw = std.Io.Dir.cwd().readFileAlloc(io, path, a, .limited(output_max)) catch return .null;
    return std.json.parseFromSliceLeaky(std.json.Value, a, raw, .{}) catch .null;
}

fn claude(a: std.mem.Allocator, io: std.Io, env: *const std.process.Environ.Map) !schema.HarnessModels {
    const settings = if (env.get("CLAUDE_CONFIG_DIR")) |root|
        read_settings(a, io, root, "settings.json")
    else if (env.get("HOME")) |root|
        read_settings(a, io, root, ".claude/settings.json")
    else
        .null;
    const default_model = env.get("ANTHROPIC_MODEL") orelse json.value_str(settings, "model");
    const per_model = json.value_object(settings, "modelSettings") orelse .null;
    const default_settings = json.value_object(per_model, default_model orelse "") orelse .null;
    const default_effort = env.get("CLAUDE_CODE_EFFORT_LEVEL") orelse
        json.value_str(default_settings, "effortLevel") orelse json.value_str(settings, "effortLevel");
    var models: std.ArrayList(schema.ModelChoice) = .empty;
    for (claude_models) |choice| {
        var entry = choice;
        if (default_model) |id| entry.is_default = std.mem.eql(u8, id, choice.id);
        const model_settings = json.value_object(per_model, choice.id) orelse .null;
        entry.default_effort = json.value_str(model_settings, "effortLevel");
        try models.append(a, entry);
    }
    if (default_model) |id| {
        for (models.items) |entry| {
            if (entry.is_default) break;
        } else try models.append(a, .{ .id = id, .label = id, .is_default = true });
    }
    return .{ .harness = .claude, .models = models.items, .efforts = effort_claude, .default_effort = default_effort };
}

fn probe_timeout(io: std.Io) std.Io.Timeout {
    const timeout: std.Io.Timeout = .{ .duration = .{ .raw = .fromMilliseconds(probe_ms), .clock = .awake } };
    return timeout.toDeadline(io);
}

fn codex_request(
    a: std.mem.Allocator,
    io: std.Io,
    stdin: std.Io.File,
    output: *std.Io.File.MultiReader,
    deadline: std.Io.Timeout,
    id: u64,
    method: []const u8,
    params: anytype,
) !std.json.Value {
    const request = try schema.encode_alloc(a, .{ .id = id, .method = method, .params = params });
    try stdin.writeStreamingAll(io, try std.mem.concat(a, u8, &.{ request, "\n" }));
    return read_reply(a, output, deadline, id);
}

fn read_reply(a: std.mem.Allocator, output: *std.Io.File.MultiReader, deadline: std.Io.Timeout, id: u64) !std.json.Value {
    const reader = output.reader(0);
    var read_bytes: usize = 0;
    while (true) {
        const buffer = reader.buffered();
        if (read_bytes + buffer.len > output_max) return error.StreamTooLong;
        if (std.mem.indexOfScalar(u8, buffer, '\n')) |end| {
            const reply = try std.json.parseFromSliceLeaky(std.json.Value, a, buffer[0..end], .{ .allocate = .alloc_always });
            reader.toss(end + 1);
            read_bytes += end + 1;
            if (json.value_u64(reply, "id") != id) continue;
            return json.value_object(reply, "result") orelse error.ModelQueryFailed;
        }
        try output.fill(1024, deadline);
    }
}

fn codex(a: std.mem.Allocator, io: std.Io, env: *const std.process.Environ.Map) !schema.HarnessModels {
    const deadline = probe_timeout(io);
    var child = try std.process.spawn(io, .{
        .argv = &.{ "codex", "app-server" },
        .environ_map = env,
        .cwd = .{ .path = env.get("HOME") orelse "/" },
        .stdin = .pipe,
        .stdout = .pipe,
        .stderr = .ignore,
    });
    defer child.kill(io);
    var buffer: std.Io.File.MultiReader.Buffer(1) = undefined;
    var output: std.Io.File.MultiReader = undefined;
    output.init(a, io, buffer.toStreams(), &.{child.stdout.?});
    defer output.deinit();
    const stdin = child.stdin.?;
    _ = try codex_request(a, io, stdin, &output, deadline, 1, "initialize", .{
        .clientInfo = .{ .name = "protectorate", .version = "0.7.0" },
    });
    try stdin.writeStreamingAll(io, "{\"method\":\"initialized\"}\n");
    const config_reply = codex_request(a, io, stdin, &output, deadline, 2, "config/read", .{ .includeLayers = false }) catch .null;
    const config = json.value_object(config_reply, "config") orelse .null;
    const default_model = json.value_nonempty_str(config, "model");
    const default_effort = json.value_nonempty_str(config, "model_reasoning_effort");

    var models: std.ArrayList(schema.ModelChoice) = .empty;
    var row: schema.HarnessModels = .{ .harness = .codex };
    var efforts_all: std.ArrayList([]const u8) = .empty;
    var cursor: ?[]const u8 = null;
    for (0..codex_page_max) |page| {
        const reply = try codex_request(a, io, stdin, &output, deadline, page + 3, "model/list", .{
            .cursor = cursor,
            .limit = @as(u32, 100),
            .includeHidden = false,
        });
        for (json.value_array(reply, "data") orelse return error.ModelQueryFailed) |item| {
            if (models.items.len == model_max) return error.TooManyModels;
            const id = json.value_nonempty_str(item, "model") orelse continue;
            var efforts: std.ArrayList([]const u8) = .empty;
            for (json.value_array(item, "supportedReasoningEfforts") orelse &.{}) |effort| {
                const level = json.value_nonempty_str(effort, "reasoningEffort") orelse continue;
                try efforts.append(a, level);
                for (efforts_all.items) |known| {
                    if (std.mem.eql(u8, level, known)) break;
                } else try efforts_all.append(a, level);
            }
            const choice: schema.ModelChoice = .{
                .id = id,
                .label = json.value_nonempty_str(item, "displayName") orelse id,
                .is_default = if (default_model) |configured| std.mem.eql(u8, configured, id) else json.value_bool(item, "isDefault") orelse false,
                .efforts = efforts.items,
                .default_effort = json.value_nonempty_str(item, "defaultReasoningEffort"),
            };
            if (choice.is_default) {
                row.default_effort = default_effort orelse choice.default_effort;
            }
            try models.append(a, choice);
        }
        cursor = json.value_nonempty_str(reply, "nextCursor");
        if (cursor == null) break;
    } else return error.TooManyModelPages;
    if (default_model) |id| {
        for (models.items) |m| {
            if (m.is_default) break;
        } else {
            try models.append(a, .{ .id = id, .label = id, .is_default = true });
            row.default_effort = default_effort;
        }
    }
    row.models = models.items;
    row.efforts = efforts_all.items;
    return row;
}

fn run_list(a: std.mem.Allocator, io: std.Io, env: *const std.process.Environ.Map, argv: []const []const u8) ![]const u8 {
    const result = try std.process.run(a, io, .{
        .argv = argv,
        .cwd = .{ .path = env.get("HOME") orelse "/" },
        .environ_map = env,
        .stdout_limit = .limited(output_max),
        .stderr_limit = .limited(output_max),
        .timeout = probe_timeout(io),
    });
    if (result.term != .exited or result.term.exited != 0) return error.ModelQueryFailed;
    return result.stdout;
}

fn pi(a: std.mem.Allocator, io: std.Io, env: *const std.process.Environ.Map) !schema.HarnessModels {
    const settings = if (env.get("PI_CODING_AGENT_DIR")) |root|
        read_settings(a, io, root, "settings.json")
    else if (env.get("HOME")) |root|
        read_settings(a, io, root, ".pi/agent/settings.json")
    else
        .null;
    const provider = json.value_nonempty_str(settings, "defaultProvider") orelse "";
    const default_model = json.value_nonempty_str(settings, "defaultModel") orelse "";
    const default_effort = json.value_nonempty_str(settings, "defaultThinkingLevel") orelse "medium";
    const model_efforts = json.value_object(settings, "modelThinkingLevels") orelse .null;

    const raw = try run_list(a, io, env, &.{ "pi", "--list-models" });
    var models: std.ArrayList(schema.ModelChoice) = .empty;
    var lines = std.mem.splitScalar(u8, raw, '\n');
    var saw_header = false;
    while (lines.next()) |line| {
        var cols = std.mem.tokenizeAny(u8, line, " \t\r");
        const source = cols.next() orelse continue;
        const id = cols.next() orelse continue;
        if (std.mem.eql(u8, source, "provider") and std.mem.eql(u8, id, "model")) {
            saw_header = true;
            continue;
        }
        if (!saw_header) continue;
        _ = cols.next() orelse continue;
        _ = cols.next() orelse continue;
        const thinking = cols.next() orelse continue;
        if (models.items.len == model_max) return error.TooManyModels;
        const qualified = try std.fmt.allocPrint(a, "{s}/{s}", .{ source, id });
        const can_think = std.mem.eql(u8, thinking, "yes");
        try models.append(a, .{
            .id = id,
            .label = qualified,
            .provider = source,
            .is_default = std.mem.eql(u8, source, provider) and std.mem.eql(u8, id, default_model),
            .efforts = if (can_think) effort_pi else &.{"off"},
            .default_effort = if (can_think) json.value_nonempty_str(model_efforts, qualified) orelse default_effort else "off",
        });
    }
    var row: schema.HarnessModels = .{ .harness = .pi_vendor, .models = models.items, .efforts = effort_pi, .default_effort = default_effort };
    for (models.items) |m| {
        if (!m.is_default) continue;
        row.default_effort = m.default_effort;
        break;
    }
    return row;
}

fn grok(a: std.mem.Allocator, io: std.Io, env: *const std.process.Environ.Map) !schema.HarnessModels {
    const deadline = probe_timeout(io);
    var child = try std.process.spawn(io, .{
        .argv = &.{ "grok", "--no-auto-update", "agent", "stdio" },
        .environ_map = env,
        .cwd = .{ .path = env.get("HOME") orelse "/" },
        .stdin = .pipe,
        .stdout = .pipe,
        .stderr = .ignore,
    });
    defer child.kill(io);
    var buffer: std.Io.File.MultiReader.Buffer(1) = undefined;
    var output: std.Io.File.MultiReader = undefined;
    output.init(a, io, buffer.toStreams(), &.{child.stdout.?});
    defer output.deinit();
    try child.stdin.?.writeStreamingAll(io, "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":1,\"clientCapabilities\":{}}}\n");
    const reply = try read_reply(a, &output, deadline, 1);
    const meta = json.value_object(reply, "_meta") orelse return error.ModelQueryFailed;
    const state = json.value_object(meta, "modelState") orelse return error.ModelQueryFailed;
    const current = json.value_str(state, "currentModelId") orelse "";
    var models: std.ArrayList(schema.ModelChoice) = .empty;
    var row: schema.HarnessModels = .{ .harness = .grok, .efforts = effort_standard };
    for (json.value_array(state, "availableModels") orelse return error.ModelQueryFailed) |item| {
        if (models.items.len == model_max) return error.TooManyModels;
        const id = json.value_nonempty_str(item, "modelId") orelse continue;
        const spec = json.value_object(item, "_meta") orelse .null;
        var efforts: std.ArrayList([]const u8) = .empty;
        for (json.value_array(spec, "reasoningEfforts") orelse &.{}) |effort| {
            try efforts.append(a, json.value_nonempty_str(effort, "value") orelse continue);
        }
        const choice: schema.ModelChoice = .{
            .id = id,
            .label = json.value_nonempty_str(item, "name") orelse id,
            .is_default = std.mem.eql(u8, id, current),
            .efforts = efforts.items,
            .default_effort = json.value_nonempty_str(spec, "reasoningEffort"),
        };
        if (choice.is_default) row.default_effort = choice.default_effort;
        try models.append(a, choice);
    }
    row.models = models.items;
    return row;
}

fn antigravity(a: std.mem.Allocator, io: std.Io, env: *const std.process.Environ.Map) !schema.HarnessModels {
    const settings = if (env.get("HOME")) |root| read_settings(a, io, root, ".gemini/antigravity-cli/settings.json") else .null;
    const configured = json.value_nonempty_str(settings, "model") orelse "";
    const raw = try run_list(a, io, env, &.{ "agy", "models" });
    var models: std.ArrayList(schema.ModelChoice) = .empty;
    var row: schema.HarnessModels = .{ .harness = .antigravity, .efforts = effort_claude };
    var lines = std.mem.splitScalar(u8, raw, '\n');
    while (lines.next()) |line| {
        const tab = std.mem.indexOfScalar(u8, line, '\t') orelse continue;
        const id = std.mem.trim(u8, line[0..tab], " \r");
        const label = std.mem.trim(u8, line[tab + 1 ..], " \r");
        if (id.len == 0 or label.len == 0) continue;
        if (models.items.len == model_max) return error.TooManyModels;
        const is_default = std.mem.eql(u8, configured, id) or std.mem.eql(u8, configured, label);
        try models.append(a, .{
            .id = id,
            .label = label,
            .is_default = is_default,
            .efforts = &.{},
            .default_effort = antigravity_effort(id),
        });
    }
    for (models.items) |*choice| {
        if (choice.default_effort) |level| {
            const family = choice.id[0 .. choice.id.len - level.len - 1];
            var efforts: std.ArrayList([]const u8) = .empty;
            for (effort_claude) |effort| {
                for (models.items) |candidate| {
                    if (candidate.id.len != family.len + 1 + effort.len) continue;
                    if (candidate.id[family.len] != '-' or !std.mem.startsWith(u8, candidate.id, family)) continue;
                    if (!std.mem.endsWith(u8, candidate.id, effort)) continue;
                    try efforts.append(a, effort);
                    break;
                }
            }
            choice.efforts = efforts.items;
        }
        if (choice.is_default) row.default_effort = choice.default_effort;
    }
    row.models = models.items;
    return row;
}

fn antigravity_effort(id: []const u8) ?[]const u8 {
    const sep = std.mem.lastIndexOfScalar(u8, id, '-') orelse return null;
    for (effort_claude) |level| {
        if (std.mem.eql(u8, id[sep + 1 ..], level)) return level;
    }
    return null;
}
