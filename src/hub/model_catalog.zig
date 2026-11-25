const std = @import("std");
const builtin = @import("builtin");
const child_process = @import("../child_process.zig");
const ProcessGuard = @import("../sandbox/exec.zig").ProcessGuard;
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

const fallbacks = [_]schema.HarnessModels{
    .{ .harness = .claude, .models = &claude_models, .efforts = effort_claude },
    .{ .harness = .codex },
    .{ .harness = .grok, .efforts = effort_standard },
    .{ .harness = .pi_vendor, .efforts = effort_pi },
    .{ .harness = .pi_local, .efforts = effort_pi },
    .{ .harness = .antigravity, .efforts = effort_claude },
};

const Snapshot = struct {
    mu: std.Io.Mutex = .init,
    encoded: ?[]u8 = null,
    expires_ms: i64 = 0,
    refreshing: bool = false,

    fn claim(self: *Snapshot, io: std.Io) bool {
        self.mu.lockUncancelable(io);
        defer self.mu.unlock(io);
        if (self.refreshing or std.Io.Clock.awake.now(io).toMilliseconds() < self.expires_ms) return false;
        self.refreshing = true;
        return true;
    }

    fn refresh(self: *Snapshot, gpa: std.mem.Allocator, io: std.Io, env: *const std.process.Environ.Map, harness: schema.Harness) void {
        var arena = std.heap.ArenaAllocator.init(gpa);
        defer arena.deinit();
        const a = arena.allocator();
        const result = switch (harness) {
            .claude => claude(a, io, env),
            .codex => codex(a, io, env),
            .grok => grok(a, io, env),
            .pi_vendor, .pi_local => pi(a, io, env, harness),
            .antigravity => antigravity(a, io, env),
            .unknown => unreachable,
        };
        const encoded = if (result) |row| schema.encode_alloc(gpa, row) catch null else |_| null;

        self.mu.lockUncancelable(io);
        const previous = if (encoded != null) self.encoded else null;
        if (encoded) |row| self.encoded = row;
        self.expires_ms = std.Io.Clock.awake.now(io).toMilliseconds() + ttl_ms;
        self.refreshing = false;
        self.mu.unlock(io);
        if (previous) |row| gpa.free(row);
    }

    fn write(self: *Snapshot, io: std.Io, jw: *std.json.Stringify, fallback: schema.HarnessModels) !void {
        self.mu.lockUncancelable(io);
        defer self.mu.unlock(io);
        if (self.encoded) |encoded| {
            try jw.beginWriteRaw();
            try jw.writer.writeAll(encoded);
            jw.endWriteRaw();
        } else {
            try jw.write(fallback);
        }
    }
};

pub const Cache = struct {
    entries: [fallbacks.len]Snapshot = @splat(.{}),

    pub fn deinit(self: *Cache, gpa: std.mem.Allocator) void {
        for (&self.entries) |*entry| {
            if (entry.encoded) |row| gpa.free(row);
        }
    }

    pub fn encode(
        self: *Cache,
        gpa: std.mem.Allocator,
        io: std.Io,
        env: *const std.process.Environ.Map,
        harness: ?schema.Harness,
    ) ![]u8 {
        var entries: []Snapshot = &self.entries;
        var selected: []const schema.HarnessModels = &fallbacks;
        if (harness) |target| {
            for (fallbacks, 0..) |row, i| {
                if (row.harness != target) continue;
                entries = entries[i..][0..1];
                selected = selected[i..][0..1];
                break;
            }
        }

        var group: std.Io.Group = .init;
        defer group.cancel(io);
        for (entries, selected) |*entry, row| {
            if (!entry.claim(io)) continue;
            if (entries.len == 1) {
                entry.refresh(gpa, io, env, row.harness);
            } else {
                group.concurrent(io, Snapshot.refresh, .{ entry, gpa, io, env, row.harness }) catch {
                    entry.mu.lockUncancelable(io);
                    entry.refreshing = false;
                    entry.mu.unlock(io);
                };
            }
        }
        try group.await(io);

        var output: std.Io.Writer.Allocating = .init(gpa);
        defer output.deinit();
        var jw: std.json.Stringify = .{ .writer = &output.writer };
        try jw.beginObject();
        try jw.objectField("harnesses");
        try jw.beginArray();
        for (entries, selected) |*entry, row| try entry.write(io, &jw, row);
        try jw.endArray();
        try jw.endObject();
        return output.toOwnedSlice();
    }
};

fn read_settings(a: std.mem.Allocator, io: std.Io, root: []const u8, relative: []const u8) std.json.Value {
    const path = std.fs.path.join(a, &.{ root, relative }) catch return .null;
    const cwd = std.Io.Dir.cwd();
    const metadata = cwd.statFile(io, path, .{}) catch return .null;
    if (metadata.kind != .file) return .null;
    const raw = cwd.readFileAlloc(io, path, a, .limited(output_max)) catch return .null;
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

const Probe = struct {
    child: std.process.Child,
    guard: ProcessGuard,
    deadline: std.Io.Clock.Timestamp,
    watchdog: if (builtin.os.tag == .windows) std.Io.Future(void) else void,

    fn start(
        io: std.Io,
        env: *const std.process.Environ.Map,
        argv: []const []const u8,
        deadline: std.Io.Clock.Timestamp,
        stdin: std.process.SpawnOptions.StdIo,
    ) !Probe {
        const windows = builtin.os.tag == .windows;
        var guard = ProcessGuard.init(null);
        errdefer guard.deinit();
        const home = env.get("HOME") orelse env.get("USERPROFILE") orelse "/";
        var child = try child_process.spawn_until(io, .{
            .argv = argv,
            .environ_map = env,
            .cwd = .{ .path = home },
            .stdin = stdin,
            .stdout = .pipe,
            .stderr = .ignore,
            .pgid = if (windows) null else 0,
            .start_suspended = windows,
            .create_no_window = true,
        }, deadline);
        errdefer {
            guard.terminate();
            child.kill(io);
        }
        if (comptime windows) {
            try guard.attach_suspended(&child);
        } else {
            guard.attach(child.id);
            if (child.stdin) |*pipe| {
                const flags: u32 = @bitCast(std.posix.O{ .NONBLOCK = true });
                const rc = std.posix.system.fcntl(pipe.handle, std.posix.F.SETFL, flags);
                if (std.posix.errno(rc) != .SUCCESS) return error.ProbePipeSetupFailed;
                pipe.flags.nonblocking = true;
            }
        }
        return .{
            .child = child,
            .guard = guard,
            .deadline = deadline,
            .watchdog = if (windows) try io.concurrent(kill_at_deadline, .{ io, deadline, guard }) else {},
        };
    }

    fn kill_at_deadline(io: std.Io, deadline: std.Io.Clock.Timestamp, guard: ProcessGuard) void {
        deadline.wait(io) catch return;
        var fence = guard;
        fence.terminate();
    }

    fn deinit(self: *Probe, io: std.Io) void {
        if (comptime builtin.os.tag == .windows) self.watchdog.cancel(io);
        if (self.child.id != null) {
            self.guard.terminate();
            self.child.kill(io);
        }
        self.guard.deinit();
    }

    fn wait(self: *Probe, io: std.Io) !std.process.Child.Term {
        try child_process.wait_for_exit(io, self.child.id.?, self.deadline);
        if (comptime builtin.os.tag == .windows) self.watchdog.cancel(io);
        self.guard.terminate();
        return self.child.wait(io);
    }

    fn write(self: *const Probe, io: std.Io, bytes: []const u8) !void {
        const pipe = self.child.stdin.?;
        if (comptime builtin.os.tag == .windows) {
            try check_deadline(io, self.deadline);
            return pipe.writeStreamingAll(io, bytes);
        }

        var sent: usize = 0;
        while (sent < bytes.len) {
            try check_deadline(io, self.deadline);
            const result = try io.operateTimeout(.{ .file_write_streaming = .{
                .file = pipe,
                .data = &.{bytes[sent..]},
            } }, .{ .deadline = self.deadline });
            sent += result.file_write_streaming catch |err| switch (err) {
                error.WouldBlock => continue,
                else => return err,
            };
        }
    }
};

fn probe_deadline(io: std.Io) std.Io.Clock.Timestamp {
    return .fromNow(io, .{ .raw = .fromMilliseconds(probe_ms), .clock = .awake });
}

fn check_deadline(io: std.Io, deadline: std.Io.Clock.Timestamp) !void {
    if (deadline.durationFromNow(io).raw.nanoseconds <= 0) return error.Timeout;
}

fn codex_request(
    a: std.mem.Allocator,
    io: std.Io,
    probe: *const Probe,
    output: *std.Io.File.MultiReader,
    id: u64,
    method: []const u8,
    params: anytype,
) !std.json.Value {
    const request = try schema.encode_alloc(a, .{ .id = id, .method = method, .params = params });
    try probe.write(io, try std.mem.concat(a, u8, &.{ request, "\n" }));
    const reply = try read_reply(a, io, output, probe.deadline, .{ .integer = @intCast(id) });
    return json.value_object(reply, "result") orelse error.ModelQueryFailed;
}

fn read_reply(
    a: std.mem.Allocator,
    io: std.Io,
    output: *std.Io.File.MultiReader,
    deadline: std.Io.Clock.Timestamp,
    id: std.json.Value,
) !std.json.Value {
    const reader = output.reader(0);
    var read_bytes: usize = 0;
    while (true) {
        try check_deadline(io, deadline);
        const buffer = reader.buffered();
        if (read_bytes + buffer.len > output_max) return error.StreamTooLong;
        if (std.mem.indexOfScalar(u8, buffer, '\n')) |end| {
            const reply = std.json.parseFromSliceLeaky(std.json.Value, a, buffer[0..end], .{ .allocate = .alloc_always }) catch .null;
            reader.toss(end + 1);
            read_bytes += end + 1;
            const matches = switch (id) {
                .integer => json.value_u64(reply, "id") == @as(u64, @intCast(id.integer)),
                .string => std.mem.eql(u8, json.value_str(reply, "id") orelse "", id.string),
                else => unreachable,
            };
            if (matches) return reply;
            continue;
        }
        try output.fill(1024, .{ .deadline = deadline });
    }
}

fn codex(a: std.mem.Allocator, io: std.Io, env: *const std.process.Environ.Map) !schema.HarnessModels {
    var probe = try Probe.start(io, env, &.{ "codex", "app-server" }, probe_deadline(io), .pipe);
    defer probe.deinit(io);
    var buffer: std.Io.File.MultiReader.Buffer(1) = undefined;
    var output: std.Io.File.MultiReader = undefined;
    output.init(a, io, buffer.toStreams(), &.{probe.child.stdout.?});
    defer output.deinit();
    _ = try codex_request(a, io, &probe, &output, 1, "initialize", .{
        .clientInfo = .{ .name = "protectorate", .version = "0.7.0" },
    });
    try probe.write(io, "{\"method\":\"initialized\"}\n");
    const config_reply = codex_request(a, io, &probe, &output, 2, "config/read", .{ .includeLayers = false }) catch .null;
    const config = json.value_object(config_reply, "config") orelse .null;
    const default_model = json.value_nonempty_str(config, "model");
    const default_effort = json.value_nonempty_str(config, "model_reasoning_effort");

    var models: std.ArrayList(schema.ModelChoice) = .empty;
    var row: schema.HarnessModels = .{ .harness = .codex };
    var efforts_all: std.ArrayList([]const u8) = .empty;
    var cursor: ?[]const u8 = null;
    for (0..codex_page_max) |page| {
        const reply = try codex_request(a, io, &probe, &output, page + 3, "model/list", .{
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

fn run_list(
    a: std.mem.Allocator,
    io: std.Io,
    env: *const std.process.Environ.Map,
    argv: []const []const u8,
    deadline: std.Io.Clock.Timestamp,
) ![]const u8 {
    var probe = try Probe.start(io, env, argv, deadline, .ignore);
    defer probe.deinit(io);
    var buffer: std.Io.File.MultiReader.Buffer(1) = undefined;
    var output: std.Io.File.MultiReader = undefined;
    output.init(a, io, buffer.toStreams(), &.{probe.child.stdout.?});
    defer output.deinit();
    while (output.fill(1024, .{ .deadline = deadline })) |_| {
        try check_deadline(io, deadline);
        if (output.reader(0).bufferedLen() > output_max) return error.StreamTooLong;
    } else |err| switch (err) {
        error.EndOfStream => {},
        else => return err,
    }
    try output.checkAnyError();
    const term = try probe.wait(io);
    if (term != .exited or term.exited != 0) return error.ModelQueryFailed;
    return output.toOwnedSlice(0);
}

const PiModel = struct {
    provider: []const u8,
    id: []const u8,
    efforts: []const []const u8,
};

const PiModels = struct {
    models: []const PiModel,
    state: std.json.Value = .null,
};

fn pi(
    a: std.mem.Allocator,
    io: std.Io,
    env: *const std.process.Environ.Map,
    harness: schema.Harness,
) !schema.HarnessModels {
    const deadline = probe_deadline(io);
    const listing = pi_rpc_models(a, io, env, deadline) catch try pi_list_models(a, io, env, deadline);

    const settings = if (env.get("PI_CODING_AGENT_DIR")) |root|
        read_settings(a, io, root, "settings.json")
    else if (env.get("HOME")) |root|
        read_settings(a, io, root, ".pi/agent/settings.json")
    else
        .null;
    const active_model = json.value_object(listing.state, "model") orelse .null;
    const provider = json.value_nonempty_str(active_model, "provider") orelse json.value_nonempty_str(settings, "defaultProvider") orelse "";
    const default_model = json.value_nonempty_str(active_model, "id") orelse json.value_nonempty_str(settings, "defaultModel") orelse "";
    const default_effort = json.value_nonempty_str(settings, "defaultThinkingLevel") orelse "medium";
    const model_efforts = json.value_object(settings, "modelThinkingLevels") orelse .null;

    const models = try a.alloc(schema.ModelChoice, listing.models.len);
    var row: schema.HarnessModels = .{ .harness = harness, .models = models, .efforts = effort_pi, .default_effort = default_effort };
    for (listing.models, models) |model, *choice| {
        const label = try std.fmt.allocPrint(a, "{s}/{s}", .{ model.provider, model.id });
        const is_default = std.mem.eql(u8, model.provider, provider) and std.mem.eql(u8, model.id, default_model);
        const configured = json.value_nonempty_str(model_efforts, label) orelse default_effort;
        const active_effort = if (is_default) json.value_nonempty_str(listing.state, "thinkingLevel") else null;
        const effort = active_effort orelse if (model.efforts.len == 1) model.efforts[0] else blk: {
            for (model.efforts) |level| {
                if (std.mem.eql(u8, level, configured)) break :blk level;
            }
            break :blk null;
        };
        choice.* = .{
            .id = model.id,
            .label = label,
            .provider = model.provider,
            .is_default = is_default,
            .efforts = model.efforts,
            .default_effort = effort,
        };
        if (is_default) row.default_effort = effort;
    }
    return row;
}

fn pi_request(
    a: std.mem.Allocator,
    io: std.Io,
    probe: *const Probe,
    output: *std.Io.File.MultiReader,
    command: []const u8,
) !std.json.Value {
    const request = try schema.encode_alloc(a, .{ .id = command, .type = command });
    try probe.write(io, try std.mem.concat(a, u8, &.{ request, "\n" }));
    const reply = try read_reply(a, io, output, probe.deadline, .{ .string = command });
    if (json.value_bool(reply, "success") != true) return error.ModelQueryFailed;
    return json.value_object(reply, "data") orelse error.ModelQueryFailed;
}

fn pi_rpc_models(a: std.mem.Allocator, io: std.Io, env: *const std.process.Environ.Map, deadline: std.Io.Clock.Timestamp) !PiModels {
    var probe = try Probe.start(io, env, &.{ "pi", "--mode", "rpc", "--no-session", "--offline" }, deadline, .pipe);
    defer probe.deinit(io);
    var buffer: std.Io.File.MultiReader.Buffer(1) = undefined;
    var output: std.Io.File.MultiReader = undefined;
    output.init(a, io, buffer.toStreams(), &.{probe.child.stdout.?});
    defer output.deinit();
    const reply = try pi_request(a, io, &probe, &output, "get_available_models");
    const available = json.value_array(reply, "models") orelse return error.ModelQueryFailed;
    if (available.len > model_max) return error.TooManyModels;

    var models: std.ArrayList(PiModel) = .empty;
    for (available) |model| {
        const provider = json.value_nonempty_str(model, "provider") orelse continue;
        const id = json.value_nonempty_str(model, "id") orelse continue;
        try models.append(a, .{ .provider = provider, .id = id, .efforts = try pi_efforts(a, model) });
    }
    const state = pi_request(a, io, &probe, &output, "get_state") catch .null;
    return .{ .models = models.items, .state = state };
}

fn pi_efforts(a: std.mem.Allocator, model: std.json.Value) ![]const []const u8 {
    const reasoning = json.value_bool(model, "reasoning") orelse return effort_pi;
    if (!reasoning) return &.{"off"};

    const mapping = json.value_object(model, "thinkingLevelMap") orelse .null;
    const standard_count = 5;
    var levels: std.ArrayList([]const u8) = .empty;
    for (effort_pi, 0..) |level, i| {
        if (json.value_get(mapping, level)) |configured| {
            if (configured == .null) continue;
        } else if (i >= standard_count) continue;
        try levels.append(a, level);
    }
    return levels.items;
}

fn pi_list_models(a: std.mem.Allocator, io: std.Io, env: *const std.process.Environ.Map, deadline: std.Io.Clock.Timestamp) !PiModels {
    const raw = try run_list(a, io, env, &.{ "pi", "--list-models" }, deadline);
    var models: std.ArrayList(PiModel) = .empty;
    var lines = std.mem.splitScalar(u8, raw, '\n');
    var saw_header = false;
    while (lines.next()) |line| {
        var cols = std.mem.tokenizeAny(u8, line, " \t\r");
        const provider = cols.next() orelse continue;
        const id = cols.next() orelse continue;
        if (std.mem.eql(u8, provider, "provider") and std.mem.eql(u8, id, "model")) {
            saw_header = true;
            continue;
        }
        if (!saw_header) continue;
        _ = cols.next() orelse continue;
        _ = cols.next() orelse continue;
        const thinking = cols.next() orelse continue;
        if (models.items.len == model_max) return error.TooManyModels;
        try models.append(a, .{
            .provider = provider,
            .id = id,
            .efforts = if (std.mem.eql(u8, thinking, "no")) &.{"off"} else effort_pi,
        });
    }
    return .{ .models = models.items };
}

fn grok(a: std.mem.Allocator, io: std.Io, env: *const std.process.Environ.Map) !schema.HarnessModels {
    var probe = try Probe.start(io, env, &.{ "grok", "--no-auto-update", "agent", "stdio" }, probe_deadline(io), .pipe);
    defer probe.deinit(io);
    var buffer: std.Io.File.MultiReader.Buffer(1) = undefined;
    var output: std.Io.File.MultiReader = undefined;
    output.init(a, io, buffer.toStreams(), &.{probe.child.stdout.?});
    defer output.deinit();
    try probe.write(io, "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":1,\"clientCapabilities\":{}}}\n");
    const reply = try read_reply(a, io, &output, probe.deadline, .{ .integer = 1 });
    const result = json.value_object(reply, "result") orelse return error.ModelQueryFailed;
    const meta = json.value_object(result, "_meta") orelse return error.ModelQueryFailed;
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
    const raw = try run_list(a, io, env, &.{ "agy", "models" }, probe_deadline(io));
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
