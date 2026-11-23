const std = @import("std");
const tiger = @import("../tiger.zig");
const version = @import("../version.zig");
const json_util = @import("../json_util.zig");
const client = @import("client.zig");

pub const RunOpts = struct {
    url: ?[]const u8 = null,
    token: ?[]const u8 = null,
    token_file: ?[]const u8 = null,
    home: ?[]const u8 = null,
    as_name: ?[]const u8 = null,
    envoy_mode: bool = false,
};

const ToolDef = struct {
    name: []const u8,
    method: []const u8,
    desc: []const u8,
    required: []const []const u8 = &.{},
    props_json: []const u8,
};

const tools = [_]ToolDef{
    .{ .name = "status", .method = "GetHubInfo", .desc = "Get hub info and runtime status", .props_json = "{}" },
    .{ .name = "get_state", .method = "GetState", .desc = "Get full hub snapshot state", .props_json = "{}" },
    .{ .name = "list_sleeves", .method = "ListSleeves", .desc = "List active and stopped sleeves", .props_json = "{}" },
    .{
        .name = "spawn_sleeve",
        .method = "SpawnSleeve",
        .desc = "Create and start a sleeve in a workspace",
        .required = &.{ "name", "harness", "path" },
        .props_json = "{\"name\":{\"type\":\"string\"},\"harness\":{\"type\":\"string\"},\"path\":{\"type\":\"string\"},\"model\":{\"type\":\"string\"},\"provider\":{\"type\":\"string\"},\"message\":{\"type\":\"string\"}}",
    },
    .{
        .name = "send_message",
        .method = "SendMessage",
        .desc = "Send a message to a sleeve",
        .required = &.{ "sleeve_id", "text" },
        .props_json = "{\"sleeve_id\":{\"type\":\"string\"},\"text\":{\"type\":\"string\"},\"delivery\":{\"type\":\"string\"}}",
    },
    .{
        .name = "stop_sleeve",
        .method = "StopSleeve",
        .desc = "Stop a running sleeve",
        .required = &.{"sleeve_id"},
        .props_json = "{\"sleeve_id\":{\"type\":\"string\"}}",
    },
    .{
        .name = "get_transcript",
        .method = "GetTranscript",
        .desc = "Read sleeve transcript entries",
        .required = &.{"sleeve_id"},
        .props_json = "{\"sleeve_id\":{\"type\":\"string\"},\"from_seq\":{\"type\":\"integer\"},\"limit\":{\"type\":\"integer\"}}",
    },
    .{
        .name = "wait_sleeve",
        .method = "Wait",
        .desc = "Wait for sleeve activity condition",
        .required = &.{"until"},
        .props_json = "{\"sleeve_ids\":{\"type\":\"array\",\"items\":{\"type\":\"string\"}},\"until\":{\"type\":\"string\"},\"timeout_ms\":{\"type\":\"integer\"}}",
    },
    .{ .name = "list_nodes", .method = "ListNodes", .desc = "List enrolled fleet nodes", .props_json = "{}" },
    .{
        .name = "run_job",
        .method = "RunJob",
        .desc = "Submit a job to a fleet node",
        .required = &.{ "target", "argv" },
        .props_json = "{\"target\":{\"type\":\"object\"},\"argv\":{\"type\":\"array\",\"items\":{\"type\":\"string\"}},\"cwd\":{\"type\":\"string\"},\"artifacts\":{\"type\":\"array\",\"items\":{\"type\":\"string\"}},\"even_if_present\":{\"type\":\"boolean\"},\"exclusive\":{\"type\":\"boolean\"},\"slots\":{\"type\":\"integer\"},\"long_slots\":{\"type\":\"integer\"},\"mem_bytes\":{\"type\":\"integer\"},\"parent_epoch\":{\"type\":\"integer\"},\"ttl_ms\":{\"type\":\"integer\"},\"wait_ms\":{\"type\":\"integer\"}}",
    },
    .{
        .name = "get_job",
        .method = "GetJob",
        .desc = "Inspect job state and artifacts",
        .required = &.{"job_id"},
        .props_json = "{\"job_id\":{\"type\":\"string\"}}",
    },
    .{
        .name = "get_job_log",
        .method = "GetJobLog",
        .desc = "Read stdout/stderr log for a job",
        .required = &.{"job_id"},
        .props_json = "{\"job_id\":{\"type\":\"string\"},\"offset\":{\"type\":\"integer\"}}",
    },
    .{
        .name = "cancel_job",
        .method = "CancelJob",
        .desc = "Cancel a running or queued job",
        .required = &.{"job_id"},
        .props_json = "{\"job_id\":{\"type\":\"string\"}}",
    },
    .{
        .name = "node_admission",
        .method = "GetNodeAdmission",
        .desc = "Inspect a node's admission capacity, active leases, wait queue, and quarantine state",
        .required = &.{"node_id"},
        .props_json = "{\"node_id\":{\"type\":\"string\"}}",
    },
    .{
        .name = "acquire_lease",
        .method = "AcquireLease",
        .desc = "Acquire a shared or exclusive admission lease on a node",
        .required = &.{"node_id"},
        .props_json = "{\"node_id\":{\"type\":\"string\"},\"holder_id\":{\"type\":\"string\"},\"exclusive\":{\"type\":\"boolean\"},\"slots\":{\"type\":\"integer\"},\"long_slots\":{\"type\":\"integer\"},\"mem_bytes\":{\"type\":\"integer\"},\"ttl_ms\":{\"type\":\"integer\"},\"wait_ms\":{\"type\":\"integer\"},\"parent_epoch\":{\"type\":\"integer\"}}",
    },
    .{
        .name = "release_lease",
        .method = "ReleaseLease",
        .desc = "Release and fence an active admission lease",
        .required = &.{ "node_id", "lease_id", "epoch" },
        .props_json = "{\"node_id\":{\"type\":\"string\"},\"lease_id\":{\"type\":\"string\"},\"epoch\":{\"type\":\"integer\"}}",
    },
    .{
        .name = "verify_epoch",
        .method = "VerifyEpoch",
        .desc = "Verify whether a boot_id + epoch fencing token is still valid on a node",
        .required = &.{ "node_id", "boot_id", "epoch" },
        .props_json = "{\"node_id\":{\"type\":\"string\"},\"boot_id\":{\"type\":\"string\"},\"epoch\":{\"type\":\"integer\"}}",
    },
    .{
        .name = "search_history",
        .method = "SearchHistory",
        .desc = "BM25 search across hub and harness transcripts",
        .required = &.{"query"},
        .props_json = "{\"query\":{\"type\":\"string\"},\"limit\":{\"type\":\"integer\"}}",
    },
    .{ .name = "list_quotas", .method = "ListQuotas", .desc = "List cached harness subscription quotas", .props_json = "{}" },
    .{
        .name = "git_push",
        .method = "GitPush",
        .desc = "Push a sleeve git branch",
        .required = &.{"sleeve_id"},
        .props_json = "{\"sleeve_id\":{\"type\":\"string\"},\"remote\":{\"type\":\"string\"},\"branch\":{\"type\":\"string\"}}",
    },
    .{ .name = "doctor", .method = "GetDoctor", .desc = "Run host readiness diagnostics", .props_json = "{}" },
    .{
        .name = "envoy_send",
        .method = "EnvoySend",
        .desc = "Send a message to Envoy, returning request_id for envoy_wait",
        .required = &.{"text"},
        .props_json = "{\"text\":{\"type\":\"string\"},\"agent\":{\"type\":\"string\"}}",
    },
    .{
        .name = "envoy_wait",
        .method = "EnvoyWait",
        .desc = "Wait for Envoy's reply to a message",
        .props_json = "{\"request_id\":{\"type\":\"string\"},\"timeout_secs\":{\"type\":\"integer\"},\"timeout_ms\":{\"type\":\"integer\"}}",
    },
    .{
        .name = "envoy_read",
        .method = "EnvoyRead",
        .desc = "Read Envoy's transcript entries after a seq",
        .props_json = "{\"after_seq\":{\"type\":\"integer\"},\"from_seq\":{\"type\":\"integer\"},\"limit\":{\"type\":\"integer\"}}",
    },
};

pub fn run(
    gpa: std.mem.Allocator,
    io: std.Io,
    env: *const std.process.Environ.Map,
    opts: RunOpts,
) !u8 {
    var cl = try client.Client.resolve_opts(gpa, io, env, .{
        .url = opts.url,
        .token = opts.token,
        .token_file = opts.token_file,
        .home = opts.home,
        .prefer_envoy_token = opts.envoy_mode,
    });
    defer cl.deinit();

    var agent_label: ?[]u8 = if (opts.as_name) |a| try gpa.dupe(u8, a) else null;
    defer if (agent_label) |al| gpa.free(al);

    var in_buf: [16 * 1024]u8 = undefined;
    var in_r = std.Io.File.stdin().reader(io, &in_buf);
    var out_buf: [16 * 1024]u8 = undefined;
    var out_w = std.Io.File.stdout().writer(io, &out_buf);

    var line_aw: std.Io.Writer.Allocating = .init(gpa);
    defer line_aw.deinit();

    var arena_st = std.heap.ArenaAllocator.init(gpa);
    defer arena_st.deinit();

    while (true) {
        line_aw.clearRetainingCapacity();
        _ = in_r.interface.streamDelimiterLimit(&line_aw.writer, '\n', .limited(4 << 20)) catch break;
        const at_eof = blk: {
            const b = in_r.interface.peekByte() catch |err| switch (err) {
                error.EndOfStream => break :blk true,
                else => break,
            };
            if (b == '\n') in_r.interface.toss(1);
            break :blk false;
        };
        const raw = std.mem.trim(u8, line_aw.written(), " \t\r");
        if (raw.len > 0) {
            _ = arena_st.reset(.retain_capacity);
            if (handle_line(gpa, arena_st.allocator(), &cl, &agent_label, raw)) |resp_opt| {
                if (resp_opt) |resp| {
                    try out_w.interface.writeAll(resp);
                    try out_w.interface.writeByte('\n');
                    try out_w.interface.flush();
                }
            } else |_| {}
        }
        if (at_eof) break;
    }
    return 0;
}

fn handle_line(
    gpa: std.mem.Allocator,
    arena: std.mem.Allocator,
    cl: *const client.Client,
    agent_label: *?[]u8,
    line: []const u8,
) !?[]const u8 {
    const val = std.json.parseFromSliceLeaky(std.json.Value, arena, line, .{}) catch {
        return try jsonrpc_err(arena, .null, -32700, "parse error");
    };
    if (val != .object) return null;

    const id_val = val.object.get("id");
    const method = json_util.value_str(val, "method") orelse return null;

    if (id_val == null or id_val.? == .null) {
        return null;
    }
    const id_json = try std.json.Stringify.valueAlloc(arena, id_val.?, .{});

    if (std.mem.eql(u8, method, "initialize")) {
        if (agent_label.* == null) {
            if (json_util.value_get(val, "params")) |p| {
                if (json_util.value_get(p, "clientInfo")) |ci| {
                    if (json_util.value_str(ci, "name")) |cname| {
                        if (cname.len > 0) agent_label.* = gpa.dupe(u8, cname) catch null;
                    }
                }
            }
        }
        return try std.fmt.allocPrint(
            arena,
            "{{\"jsonrpc\":\"2.0\",\"id\":{s},\"result\":{{\"protocolVersion\":\"2024-11-05\",\"capabilities\":{{\"tools\":{{}}}},\"serverInfo\":{{\"name\":\"protectorate-envoy\",\"version\":\"{s}\"}}}}}}",
            .{ id_json, version.version },
        );
    }
    if (std.mem.eql(u8, method, "ping")) {
        return try std.fmt.allocPrint(arena, "{{\"jsonrpc\":\"2.0\",\"id\":{s},\"result\":{{}}}}", .{id_json});
    }
    if (std.mem.eql(u8, method, "tools/list")) {
        var aw: std.Io.Writer.Allocating = .init(arena);
        const w = &aw.writer;
        try w.print("{{\"jsonrpc\":\"2.0\",\"id\":{s},\"result\":{{\"tools\":[", .{id_json});
        for (tools, 0..) |t, i| {
            if (i > 0) try w.writeByte(',');
            try w.writeAll("{\"name\":");
            try std.json.Stringify.encodeJsonString(t.name, .{}, w);
            try w.writeAll(",\"description\":");
            try std.json.Stringify.encodeJsonString(t.desc, .{}, w);
            try w.writeAll(",\"inputSchema\":{\"type\":\"object\",\"properties\":");
            try w.writeAll(t.props_json);
            if (t.required.len > 0) {
                try w.writeAll(",\"required\":[");
                for (t.required, 0..) |req_k, ri| {
                    if (ri > 0) try w.writeByte(',');
                    try std.json.Stringify.encodeJsonString(req_k, .{}, w);
                }
                try w.writeByte(']');
            }
            try w.writeAll("}}");
        }
        try w.writeAll("]}}");
        return aw.written();
    }
    if (std.mem.eql(u8, method, "tools/call")) {
        const params = json_util.value_get(val, "params") orelse .null;
        const tool_name = json_util.value_str(params, "name") orelse {
            return try jsonrpc_err(arena, id_val.?, -32602, "missing tool name");
        };
        const args_val = json_util.value_get(params, "arguments");
        var args_json = if (args_val) |av| try std.json.Stringify.valueAlloc(arena, av, .{}) else "{}";

        var found: ?ToolDef = null;
        for (tools) |t| {
            if (std.mem.eql(u8, t.name, tool_name)) {
                found = t;
                break;
            }
        }
        const td = found orelse return try jsonrpc_err(arena, id_val.?, -32601, "unknown tool");

        if (std.mem.eql(u8, td.name, "envoy_send") and args_val != null and args_val.? == .object) {
            if (args_val.?.object.get("agent") == null) {
                if (agent_label.*) |al| {
                    const txt = json_util.value_str(args_val.?, "text") orelse "";
                    args_json = try std.json.Stringify.valueAlloc(arena, .{ .text = txt, .agent = al }, .{});
                }
            }
        } else if (std.mem.eql(u8, td.name, "envoy_wait") and args_val != null and args_val.? == .object) {
            if (args_val.?.object.get("timeout_ms") == null) {
                if (args_val.?.object.get("timeout_secs")) |ts| {
                    const secs: i64 = if (ts == .integer) ts.integer else 60;
                    const ms: i64 = @min(@as(i64, 60_000), @max(@as(i64, 1_000), secs * 1000));
                    args_json = try std.json.Stringify.valueAlloc(arena, .{
                        .request_id = json_util.value_str(args_val.?, "request_id"),
                        .timeout_ms = ms,
                    }, .{});
                }
            }
        } else if (std.mem.eql(u8, td.name, "envoy_read") and args_val != null and args_val.? == .object) {
            if (args_val.?.object.get("from_seq") == null) {
                if (args_val.?.object.get("after_seq")) |as| {
                    const fseq: i64 = if (as == .integer) as.integer else 0;
                    const lim: ?i64 = if (args_val.?.object.get("limit")) |lv| (if (lv == .integer) lv.integer else null) else null;
                    args_json = try std.json.Stringify.valueAlloc(arena, .{
                        .from_seq = fseq,
                        .limit = lim,
                    }, .{});
                }
            }
        }

        const res = cl.call_raw(arena, td.method, args_json) catch |err| {
            return try wrap_tool_result(arena, id_json, @errorName(err), true);
        };
        return try wrap_tool_result(arena, id_json, res.body, res.status >= 400);
    }

    return try jsonrpc_err(arena, id_val.?, -32601, "method not found");
}

fn wrap_tool_result(arena: std.mem.Allocator, id_json: []const u8, text: []const u8, is_err: bool) ![]const u8 {
    var aw: std.Io.Writer.Allocating = .init(arena);
    const w = &aw.writer;
    try w.print("{{\"jsonrpc\":\"2.0\",\"id\":{s},\"result\":{{\"content\":[{{\"type\":\"text\",\"text\":", .{id_json});
    try std.json.Stringify.encodeJsonString(text, .{}, w);
    try w.print("}}],\"isError\":{}}}", .{is_err});
    try w.writeByte('}');
    return aw.written();
}

fn jsonrpc_err(arena: std.mem.Allocator, id_val: std.json.Value, code: i32, msg: []const u8) ![]const u8 {
    const id_json = try std.json.Stringify.valueAlloc(arena, id_val, .{});
    var aw: std.Io.Writer.Allocating = .init(arena);
    const w = &aw.writer;
    try w.print("{{\"jsonrpc\":\"2.0\",\"id\":{s},\"error\":{{\"code\":{d},\"message\":", .{ id_json, code });
    try std.json.Stringify.encodeJsonString(msg, .{}, w);
    try w.writeAll("}}");
    return aw.written();
}
