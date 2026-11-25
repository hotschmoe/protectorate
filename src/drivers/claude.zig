const std = @import("std");
const tiger = @import("../tiger.zig");
const json_util = @import("../json_util.zig");
const proto = @import("protocol.zig");

pub const Provider = enum { anthropic, bedrock, vertex, foundry };

const Pending = struct {
    request_id: []const u8,
    uuid: []const u8,
};

pub const Claude = struct {
    session_id: []const u8,
    uuid_base: []const u8,
    model: ?[]const u8 = null,
    provider: ?[]const u8 = null,
    effort: ?[]const u8 = null,
    supervised: bool = false,
    sent: u64 = 0,
    next_control: u64 = 0,
    running: bool = false,
    running_request_id: ?[]const u8 = null,
    announced: bool = false,
    compacting: bool = false,
    context_tokens: ?u64 = null,
    unechoed: std.ArrayList(Pending) = .empty,
    done_uuids: std.ArrayList([]const u8) = .empty,
    approvals: std.StringHashMapUnmanaged(bool) = .empty,
    denied: std.StringHashMapUnmanaged(void) = .empty,

    pub fn build_argv(arena: std.mem.Allocator, spec: proto.SpawnSpec) ![]const []const u8 {
        var argv: std.ArrayList([]const u8) = .empty;
        try argv.appendSlice(arena, &.{
            spec.program,
            "-p",
            "--verbose",
            "--output-format",
            "stream-json",
            "--input-format",
            "stream-json",
            "--include-partial-messages",
        });
        if (spec.resume_id) |rid| {
            try argv.appendSlice(arena, &.{ "--resume", rid });
        } else if (spec.session_id) |sid| {
            try argv.appendSlice(arena, &.{ "--session-id", sid });
        }
        if (spec.model) |m| {
            try argv.appendSlice(arena, &.{ "--model", m });
        }
        if (spec.provider) |id| {
            const provider = std.meta.stringToEnum(Provider, id) orelse return error.UnsupportedProvider;
            const settings = try std.json.Stringify.valueAlloc(arena, .{ .env = .{
                .CLAUDE_CODE_USE_BEDROCK = if (provider == .bedrock) "1" else "0",
                .CLAUDE_CODE_USE_VERTEX = if (provider == .vertex) "1" else "0",
                .CLAUDE_CODE_USE_FOUNDRY = if (provider == .foundry) "1" else "0",
            } }, .{});
            try argv.appendSlice(arena, &.{ "--settings", settings });
        }
        if (spec.effort) |e| {
            try argv.appendSlice(arena, &.{ "--effort", e });
        }
        switch (spec.permission) {
            .unsupervised => try argv.append(arena, "--dangerously-skip-permissions"),
            .supervised => try argv.appendSlice(arena, &.{ "--permission-prompt-tool", "stdio" }),
        }
        try argv.appendSlice(arena, spec.extra_args);
        return argv.items;
    }

    pub fn start(self: *Claude, out: *proto.Out) !void {
        try out.emit(.{ .session_info = .{
            .harness = .claude,
            .native_id = self.session_id,
            .model = self.model,
            .provider = self.provider,
            .effort = self.effort,
        } });
    }

    pub fn on_line(self: *Claude, arena: std.mem.Allocator, line: []const u8, out: *proto.Out) !void {
        const trimmed = std.mem.trim(u8, line, " \t\r\n");
        if (trimmed.len == 0) return;
        const msg = std.json.parseFromSliceLeaky(std.json.Value, arena, trimmed, .{}) catch {
            out.stray += 1;
            return;
        };
        if (msg != .object) {
            out.stray += 1;
            return;
        }
        if (json_util.value_get(msg, "parent_tool_use_id")) |pt| {
            if (pt != .null) return;
        }
        const kind = json_util.value_str(msg, "type") orelse {
            out.stray += 1;
            return;
        };

        if (std.mem.eql(u8, kind, "system")) {
            try self.on_system(msg, out);
        } else if (std.mem.eql(u8, kind, "stream_event")) {
            try self.ensure_run(out);
            try self.on_stream_event(arena, msg, out);
        } else if (std.mem.eql(u8, kind, "assistant")) {
            try self.ensure_run(out);
            try self.on_assistant(arena, msg, out);
        } else if (std.mem.eql(u8, kind, "user")) {
            try self.on_user(arena, msg, out);
        } else if (std.mem.eql(u8, kind, "control_request")) {
            try self.on_control_request(arena, msg, out);
        } else if (std.mem.eql(u8, kind, "result")) {
            try self.on_result(arena, msg, out);
        } else if (std.mem.eql(u8, kind, "control_response")) {
            // Interrupt acknowledgment handled gracefully.
        } else if (std.mem.eql(u8, kind, "control_cancel_request") or
            std.mem.eql(u8, kind, "rate_limit_event") or
            std.mem.eql(u8, kind, "keep_alive") or
            std.mem.eql(u8, kind, "tool_progress") or
            std.mem.eql(u8, kind, "auth_status"))
        {} else {
            try out.unmapped(kind);
        }
    }

    fn ensure_run(self: *Claude, out: *proto.Out) !void {
        if (!self.running) {
            self.running = true;
            self.announced = true;
            try out.emit(.{ .turn_started = .{
                .request_id = self.running_request_id,
                .native_turn_id = null,
            } });
        } else if (!self.announced) {
            self.announced = true;
            try out.emit(.{ .turn_started = .{
                .request_id = self.running_request_id,
                .native_turn_id = null,
            } });
        }
    }

    fn on_system(self: *Claude, msg: std.json.Value, out: *proto.Out) !void {
        const sub = json_util.value_str(msg, "subtype") orelse "";
        if (std.mem.eql(u8, sub, "init")) {
            const sid = json_util.value_str(msg, "session_id") orelse self.session_id;
            self.session_id = try out.alloc.dupe(u8, sid);
            if (json_util.value_str(msg, "model")) |m| self.model = try out.alloc.dupe(u8, m);
            try out.emit(.{ .session_info = .{
                .harness = .claude,
                .native_id = self.session_id,
                .model = self.model,
                .provider = self.provider,
                .effort = self.effort,
                .cwd = json_util.value_str(msg, "cwd"),
            } });
        } else if (std.mem.eql(u8, sub, "compact_boundary")) {
            self.compacting = false;
            const meta = json_util.value_get(msg, "compact_metadata") orelse .null;
            try out.emit(.{ .compaction = .{
                .trigger = json_util.value_str(meta, "trigger"),
                .tokens_before = json_util.value_u64(meta, "pre_tokens"),
                .tokens_after = null,
            } });
        }
    }

    fn on_stream_event(self: *Claude, arena: std.mem.Allocator, msg: std.json.Value, out: *proto.Out) !void {
        _ = self;
        _ = arena;
        const ev = json_util.value_get(msg, "event") orelse return;
        const etype = json_util.value_str(ev, "type") orelse return;
        if (std.mem.eql(u8, etype, "content_block_delta")) {
            const d = json_util.value_get(ev, "delta") orelse return;
            const dt = json_util.value_str(d, "type") orelse return;
            const idx_n = json_util.value_u64(ev, "index") orelse 0;
            var kbuf: [32]u8 = undefined;
            const key = std.fmt.bufPrint(&kbuf, "claude-blk-{d}", .{idx_n}) catch "claude-blk-0";
            if (std.mem.eql(u8, dt, "text_delta")) {
                if (json_util.value_str(d, "text")) |t| try out.delta(false, key, t);
            } else if (std.mem.eql(u8, dt, "thinking_delta")) {
                if (json_util.value_str(d, "thinking")) |t| try out.delta(true, key, t);
            }
        }
    }

    fn on_assistant(self: *Claude, arena: std.mem.Allocator, msg: std.json.Value, out: *proto.Out) !void {
        _ = arena;
        const m = json_util.value_get(msg, "message") orelse return;
        if (json_util.value_get(m, "usage")) |u| {
            const total = (out.count_u64("input_tokens", json_util.value_get(u, "input_tokens") orelse .null) orelse 0) +|
                (out.count_u64("cache_creation", json_util.value_get(u, "cache_creation_input_tokens") orelse .null) orelse 0) +|
                (out.count_u64("cache_read", json_util.value_get(u, "cache_read_input_tokens") orelse .null) orelse 0);
            if (total > 0) self.context_tokens = total;
        }
        const msg_id = json_util.value_str(m, "id") orelse "claude-msg";
        const content = json_util.value_array(m, "content") orelse return;
        for (content, 0..) |part, idx| {
            const pt = json_util.value_str(part, "type") orelse continue;
            var kbuf: [64]u8 = undefined;
            const key = std.fmt.bufPrint(&kbuf, "{s}-{d}", .{ msg_id, idx }) catch msg_id;
            if (std.mem.eql(u8, pt, "text")) {
                if (json_util.value_str(part, "text")) |t| try out.finish_text(false, key, t);
            } else if (std.mem.eql(u8, pt, "thinking")) {
                if (json_util.value_str(part, "thinking")) |t| try out.finish_text(true, key, t);
            } else if (std.mem.eql(u8, pt, "tool_use")) {
                const tid = json_util.value_str(part, "id") orelse "";
                const tname = json_util.value_str(part, "name") orelse "tool";
                const tinp = json_util.value_get(part, "input") orelse .null;
                try out.tool_call(tid, tool_kind(tname), tname, tinp);
                if (edit_path(tname, tinp)) |ep| {
                    var fd = try out.alloc.alloc(proto.FileDelta, 1);
                    fd[0] = .{ .path = try out.alloc.dupe(u8, ep), .op = "edit" };
                    try out.emit(.{ .file_change = .{ .tool_id = try out.alloc.dupe(u8, tid), .files = fd } });
                }
            }
        }
    }

    fn on_user(self: *Claude, arena: std.mem.Allocator, msg: std.json.Value, out: *proto.Out) !void {
        const inner = json_util.value_get(msg, "message") orelse return;
        const content = json_util.value_get(inner, "content") orelse .null;
        if (json_util.value_bool(msg, "isReplay") == true) {
            const uuid = json_util.value_str(msg, "uuid") orelse "";
            for (self.unechoed.items, 0..) |p, idx| {
                if (std.mem.eql(u8, p.uuid, uuid)) {
                    const req_id = p.request_id;
                    _ = self.unechoed.orderedRemove(idx);
                    try self.done_uuids.append(out.alloc, p.uuid);
                    try out.emit(.{ .prompt_ack = .{
                        .request_id = req_id,
                        .ack = .confirmed,
                        .native_turn_id = p.uuid,
                    } });
                    self.running = true;
                    self.running_request_id = req_id;
                    self.announced = true;
                    try out.emit(.{ .turn_started = .{ .request_id = req_id, .native_turn_id = p.uuid } });
                    return;
                }
            }
            return;
        }

        if (content != .array) return;
        for (content.array.items) |part| {
            const pt = json_util.value_str(part, "type") orelse continue;
            if (!std.mem.eql(u8, pt, "tool_result")) continue;
            try self.ensure_run(out);
            const tid = json_util.value_str(part, "tool_use_id") orelse "";
            const txt = try proto.content_text_alloc(arena, json_util.value_get(part, "content") orelse .null);
            const bo = try proto.bounded_output(out.alloc, txt);
            const is_err = json_util.value_bool(part, "is_error") == true;
            const status: proto.ToolStatus = if (!is_err)
                .completed
            else if (self.denied.remove(tid))
                .declined
            else
                .failed;
            try out.emit(.{ .tool_result = .{
                .id = try out.alloc.dupe(u8, tid),
                .status = status,
                .output = bo.text,
                .truncated = bo.truncated,
            } });
        }
    }

    fn on_control_request(self: *Claude, arena: std.mem.Allocator, msg: std.json.Value, out: *proto.Out) !void {
        _ = arena;
        const req_id = json_util.value_str(msg, "request_id") orelse {
            try out.emit_error(.protocol, "claude control_request without request_id");
            return;
        };
        if (req_id.len == 0) {
            try out.emit_error(.protocol, "claude control_request with empty request_id");
            return;
        }
        const req = json_util.value_get(msg, "request") orelse .null;
        const sub = json_util.value_str(req, "subtype") orelse "";
        if (!std.mem.eql(u8, sub, "can_use_tool")) {
            const err_msg = try std.fmt.allocPrint(out.alloc, "protectorate does not handle {s}", .{sub});
            try out.write_value(.{
                .type = "control_response",
                .response = .{ .subtype = "error", .request_id = req_id, .@"error" = err_msg },
            }, null);
            return;
        }
        const tname = json_util.value_str(req, "tool_name") orelse "tool";
        const tinp = json_util.value_get(req, "input") orelse .null;
        const sum = try proto.summarize_alloc(out.alloc, tname, tinp);
        const raw_inp = if (tinp == .null) null else try json_util.stringify_alloc(out.alloc, tinp);
        const dup_id = try out.alloc.dupe(u8, req_id);
        const opts = try out.alloc.dupe(proto.Decision, &.{ .allow_once, .deny, .cancel });
        try out.emit(.{ .approval_request = .{
            .id = dup_id,
            .tool = tool_kind(tname),
            .name = try out.alloc.dupe(u8, tname),
            .summary = sum,
            .input_json = raw_inp,
            .options = opts,
        } });
        try self.approvals.put(out.alloc, dup_id, true);
        if (!self.supervised) {
            try self.respond_approval(dup_id, .allow_once, out);
        }
    }

    fn on_result(self: *Claude, arena: std.mem.Allocator, msg: std.json.Value, out: *proto.Out) !void {
        _ = arena;
        const is_err = json_util.value_bool(msg, "is_error") == true;
        const sub = json_util.value_str(msg, "subtype") orelse "";
        const stop: proto.Stop = if (std.mem.eql(u8, sub, "error_max_turns"))
            .max_turns
        else if (is_err)
            .@"error"
        else
            .end_turn;

        if (is_err) {
            if (json_util.value_str(msg, "result")) |err_txt| {
                if (err_txt.len > 0) try out.emit_error(proto.classify_error(err_txt), err_txt);
            }
        }

        const u = json_util.value_get(msg, "usage") orelse .null;
        const usage = proto.Usage{
            .input = out.count_u64("input_tokens", json_util.value_get(u, "input_tokens") orelse .null) orelse 0,
            .output = out.count_u64("output_tokens", json_util.value_get(u, "output_tokens") orelse .null) orelse 0,
            .cache_read = out.count_u64("cache_read", json_util.value_get(u, "cache_read_input_tokens") orelse .null) orelse 0,
            .cache_write = out.count_u64("cache_write", json_util.value_get(u, "cache_creation_input_tokens") orelse .null) orelse 0,
            .cost_usd = json_util.value_f64(msg, "total_cost_usd"),
            .context_tokens = self.context_tokens,
        };
        try out.emit(.{ .turn_settled = .{
            .stop = stop,
            .native_stop = if (sub.len > 0) try out.alloc.dupe(u8, sub) else null,
            .usage = usage,
            .duration_ms = json_util.value_u64(msg, "duration_ms"),
        } });
        self.running = false;
        self.running_request_id = null;
        self.announced = false;
        self.approvals.clearRetainingCapacity();
        self.denied.clearRetainingCapacity();
    }

    pub fn send_user(
        self: *Claude,
        request_id: []const u8,
        text: []const u8,
        steer: ?[]const u8,
        out: *proto.Out,
    ) !void {
        if (steer != null) return error.Unsupported;
        if (self.running or self.unechoed.items.len > 0 or self.compacting) return error.Busy;
        self.sent += 1;
        const uuid = try std.fmt.allocPrint(out.alloc, "{s}{x:0>12}", .{ self.uuid_base, self.sent });
        try out.write_value(.{
            .type = "user",
            .message = .{ .role = "user", .content = text },
            .uuid = uuid,
        }, request_id);
        try self.unechoed.append(out.alloc, .{
            .request_id = try out.alloc.dupe(u8, request_id),
            .uuid = uuid,
        });
    }

    pub fn respond_approval(self: *Claude, id: []const u8, decision: proto.Decision, out: *proto.Out) !void {
        if (!self.approvals.contains(id)) return error.UnknownApproval;
        _ = self.approvals.remove(id);
        const allow = decision == .allow_once or decision == .allow_session;
        if (!allow) {
            try self.denied.put(out.alloc, try out.alloc.dupe(u8, id), {});
        }
        try out.write_value(.{
            .type = "control_response",
            .response = .{
                .subtype = "success",
                .request_id = id,
                .response = .{
                    .behavior = if (allow) "allow" else "deny",
                    .message = if (allow) null else "Denied by owner",
                },
            },
        }, null);
        try out.emit(.{ .approval_decision = .{
            .id = try out.alloc.dupe(u8, id),
            .decision = decision,
            .by = if (self.supervised) "owner" else "autonomous",
            .confirmed = true,
        } });
    }

    pub fn interrupt(self: *Claude, out: *proto.Out) !void {
        if (!self.running and self.unechoed.items.len == 0) return;
        self.next_control += 1;
        const id = try std.fmt.allocPrint(out.alloc, "pt-interrupt-{d}", .{self.next_control});
        try out.write_value(.{
            .type = "control_request",
            .request_id = id,
            .request = .{ .subtype = "interrupt" },
        }, null);
    }

    pub fn compact(self: *Claude, out: *proto.Out) !void {
        if (self.running or self.unechoed.items.len > 0 or self.compacting) return error.Busy;
        try out.write_value(.{
            .type = "user",
            .message = .{ .role = "user", .content = "/compact" },
        }, null);
        self.compacting = true;
    }
};

fn tool_kind(name: []const u8) proto.ToolKind {
    if (std.mem.eql(u8, name, "Bash") or std.mem.eql(u8, name, "BashOutput") or std.mem.eql(u8, name, "KillShell")) return .command;
    if (std.mem.eql(u8, name, "Read") or std.mem.eql(u8, name, "NotebookRead") or std.mem.eql(u8, name, "LS")) return .read;
    if (std.mem.eql(u8, name, "Edit") or std.mem.eql(u8, name, "MultiEdit") or std.mem.eql(u8, name, "Write") or std.mem.eql(u8, name, "NotebookEdit")) return .edit;
    if (std.mem.eql(u8, name, "Grep") or std.mem.eql(u8, name, "Glob") or std.mem.eql(u8, name, "ToolSearch")) return .search;
    if (std.mem.eql(u8, name, "WebFetch") or std.mem.eql(u8, name, "WebSearch")) return .web;
    if (std.mem.eql(u8, name, "Task") or std.mem.eql(u8, name, "Agent")) return .subagent;
    if (std.mem.startsWith(u8, name, "mcp__")) return .mcp;
    return .other;
}

fn edit_path(name: []const u8, input: std.json.Value) ?[]const u8 {
    if (std.mem.eql(u8, name, "Edit") or std.mem.eql(u8, name, "MultiEdit") or std.mem.eql(u8, name, "Write")) {
        return json_util.value_str(input, "file_path");
    }
    if (std.mem.eql(u8, name, "NotebookEdit")) {
        return json_util.value_str(input, "notebook_path");
    }
    return null;
}
