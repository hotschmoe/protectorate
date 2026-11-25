const std = @import("std");
const json_util = @import("../json_util.zig");
const proto = @import("protocol.zig");

const RpcKind = union(enum) {
    state,
    prompt: []const u8,
    steer: struct { req_id: []const u8, native_id: []const u8 },
    abort,
    compact,
};

const Turn = struct {
    request_id: ?[]const u8 = null,
    native_id: []const u8,
    answered: bool = false,
    started: bool = false,
    errored: bool = false,
    usage: proto.Usage = .{},
    cost: f64 = 0.0,
};

pub const Pi = struct {
    session_id: []const u8,
    model: ?[]const u8 = null,
    provider: ?[]const u8 = null,
    ready: bool = false,
    turn_counter: u64 = 0,
    msg_n: u64 = 0,
    next_id: u64 = 1,
    turn: ?Turn = null,
    pending_rpc: std.StringHashMapUnmanaged(RpcKind) = .empty,

    pub fn build_argv(arena: std.mem.Allocator, spec: proto.SpawnSpec) ![]const []const u8 {
        var argv: std.ArrayList([]const u8) = .empty;
        try argv.appendSlice(arena, &.{ spec.program, "--mode", "rpc" });
        if (spec.resume_id) |rid| {
            try argv.appendSlice(arena, &.{ "--session", rid });
        } else if (spec.session_id) |sid| {
            try argv.appendSlice(arena, &.{ "--session-id", sid });
        }
        if (spec.provider) |p| try argv.appendSlice(arena, &.{ "--provider", p });
        if (spec.model) |m| try argv.appendSlice(arena, &.{ "--model", m });
        if (spec.effort) |e| try argv.appendSlice(arena, &.{ "--thinking", e });
        try argv.appendSlice(arena, spec.extra_args);
        return argv.items;
    }

    pub fn start(self: *Pi, out: *proto.Out) !void {
        try self.command("get_state", .{}, .state, null, out);
    }

    fn command(self: *Pi, cmd_type: []const u8, payload: anytype, kind: RpcKind, req_id: ?[]const u8, out: *proto.Out) !void {
        const id = try std.fmt.allocPrint(out.alloc, "pi-{d}", .{self.next_id});
        self.next_id += 1;
        try self.pending_rpc.put(out.alloc, id, kind);
        var buf: std.Io.Writer.Allocating = .init(out.alloc);
        var jw: std.json.Stringify = .{ .writer = &buf.writer };
        try jw.beginObject();
        try jw.objectField("id");
        try jw.write(id);
        try jw.objectField("type");
        try jw.write(cmd_type);
        const P = @TypeOf(payload);
        const pinfo = @typeInfo(P).@"struct";
        inline for (pinfo.field_names, pinfo.field_types) |fnm, ft| {
            if (ft == void) continue;
            try jw.objectField(fnm);
            try jw.write(@field(payload, fnm));
        }
        try jw.endObject();
        try out.write_raw_line(buf.written(), req_id);
    }

    fn ensure_turn(self: *Pi, out: *proto.Out) !*Turn {
        if (self.turn == null) {
            self.turn_counter += 1;
            const nid = try std.fmt.allocPrint(out.alloc, "pi-turn-{d}", .{self.turn_counter});
            self.turn = .{ .native_id = nid, .answered = true };
        }
        return &self.turn.?;
    }

    pub fn on_line(self: *Pi, arena: std.mem.Allocator, line: []const u8, out: *proto.Out) !void {
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
        const kind = json_util.value_str(msg, "type") orelse {
            out.stray += 1;
            return;
        };

        if (std.mem.eql(u8, kind, "response")) {
            try self.on_response(msg, out);
        } else if (std.mem.eql(u8, kind, "agent_start")) {
            const t = try self.ensure_turn(out);
            if (!t.started) {
                t.started = true;
                try out.emit(.{ .turn_started = .{ .request_id = t.request_id, .native_turn_id = t.native_id } });
            }
        } else if (std.mem.eql(u8, kind, "message_start")) {
            const inner = json_util.value_get(msg, "message") orelse .null;
            if (std.mem.eql(u8, json_util.value_str(inner, "role") orelse "", "assistant")) {
                self.msg_n += 1;
            }
        } else if (std.mem.eql(u8, kind, "message_update")) {
            try self.on_message_update(msg, out);
        } else if (std.mem.eql(u8, kind, "message_end")) {
            try self.on_message_end(msg, out);
        } else if (std.mem.eql(u8, kind, "tool_execution_start")) {
            const id = json_util.value_str(msg, "toolCallId") orelse "";
            const name = json_util.value_str(msg, "toolName") orelse "tool";
            const args = json_util.value_get(msg, "args") orelse .null;
            try out.tool_call(id, tool_kind(name), name, args);
            if (std.mem.eql(u8, name, "edit") or std.mem.eql(u8, name, "write")) {
                if (json_util.value_str(args, "path")) |path| {
                    var fd = try out.alloc.alloc(proto.FileDelta, 1);
                    fd[0] = .{ .path = try out.alloc.dupe(u8, path), .op = try out.alloc.dupe(u8, name) };
                    try out.emit(.{ .file_change = .{ .tool_id = try out.alloc.dupe(u8, id), .files = fd } });
                }
            }
        } else if (std.mem.eql(u8, kind, "tool_execution_end")) {
            const res = json_util.value_get(msg, "result") orelse .null;
            const txt = try proto.content_text_alloc(arena, json_util.value_get(res, "content") orelse .null);
            const bo = try proto.bounded_output(out.alloc, txt);
            try out.emit(.{ .tool_result = .{
                .id = try out.alloc.dupe(u8, json_util.value_str(msg, "toolCallId") orelse ""),
                .status = if (json_util.value_bool(msg, "isError") == true) .failed else .completed,
                .output = bo.text,
                .truncated = bo.truncated,
                .duration_ms = json_util.value_u64(msg, "durationMs"),
            } });
        } else if (std.mem.eql(u8, kind, "agent_settled") or std.mem.eql(u8, kind, "agent_end")) {
            try self.on_settled(msg, out);
        } else if (std.mem.eql(u8, kind, "compaction_end")) {
            if (json_util.value_get(msg, "result")) |r| {
                if (r == .object) {
                    try out.emit(.{ .compaction = .{
                        .trigger = json_util.value_str(msg, "reason"),
                        .tokens_before = json_util.value_u64(r, "tokensBefore"),
                        .tokens_after = json_util.value_u64(r, "estimatedTokensAfter"),
                    } });
                    return;
                }
            }
            if (json_util.value_str(msg, "errorMessage")) |err_m| {
                try out.notice("compaction", err_m);
            }
        } else if (std.mem.eql(u8, kind, "auto_retry_start")) {
            try out.notice("retry", json_util.value_str(msg, "errorMessage") orelse "retrying");
        } else if (std.mem.eql(u8, kind, "auto_retry_end")) {
            if (json_util.value_bool(msg, "success") == false) {
                const txt = json_util.value_str(msg, "finalError") orelse "pi retries exhausted";
                if (self.turn) |*t| t.errored = true;
                try out.emit_error(proto.classify_error(txt), txt);
            }
        } else if (std.mem.eql(u8, kind, "extension_ui_request")) {
            const method = json_util.value_str(msg, "method") orelse "";
            if (std.mem.eql(u8, method, "select") or std.mem.eql(u8, method, "confirm") or std.mem.eql(u8, method, "input") or std.mem.eql(u8, method, "editor")) {
                try out.write_value(.{ .type = "extension_ui_response", .id = json_util.value_get(msg, "id") orelse .null, .cancelled = true }, null);
            }
        }
    }

    fn on_response(self: *Pi, msg: std.json.Value, out: *proto.Out) !void {
        const id = json_util.value_str(msg, "id") orelse return;
        const kind = self.pending_rpc.get(id) orelse return;
        _ = self.pending_rpc.remove(id);
        const ok = json_util.value_bool(msg, "success") == true;
        const data = json_util.value_get(msg, "data") orelse .null;
        const err_msg = json_util.value_str(msg, "error") orelse "pi command failed";

        switch (kind) {
            .state => {
                if (!ok) {
                    out.refused = true;
                    try out.emit_error(.protocol, err_msg);
                    return;
                }
                self.ready = true;
                const sid = json_util.value_str(data, "sessionId") orelse self.session_id;
                self.session_id = try out.alloc.dupe(u8, sid);
                const m = json_util.value_get(data, "model") orelse .null;
                const model_id = if (m == .object) json_util.value_str(m, "id") else json_util.value_str(data, "model");
                const prov = if (m == .object) json_util.value_str(m, "provider") else self.provider;
                const harness: proto.Harness = if (self.provider) |pv| (if (std.mem.eql(u8, pv, "hotschmoe-local") or std.mem.eql(u8, pv, "ollama")) .pi_local else .pi_vendor) else .pi_local;
                try out.emit(.{ .session_info = .{
                    .harness = harness,
                    .native_id = self.session_id,
                    .model = if (model_id) |mid| try out.alloc.dupe(u8, mid) else self.model,
                    .provider = if (prov) |pv| try out.alloc.dupe(u8, pv) else self.provider,
                    .effort = if (json_util.value_nonempty_str(data, "thinkingLevel")) |level| try out.alloc.dupe(u8, level) else null,
                } });
            },
            .prompt => |req_id| {
                if (!ok) {
                    self.turn = null;
                    try out.emit(.{ .prompt_ack = .{
                        .request_id = req_id,
                        .ack = .rejected,
                        .detail = try out.alloc.dupe(u8, err_msg),
                    } });
                } else {
                    if (self.turn) |*t| t.answered = true;
                    const nid = if (self.turn) |t| t.native_id else null;
                    try out.emit(.{ .prompt_ack = .{
                        .request_id = req_id,
                        .ack = .confirmed,
                        .native_turn_id = nid,
                    } });
                }
            },
            .steer => |s| {
                try out.emit(.{ .prompt_ack = .{
                    .request_id = s.req_id,
                    .ack = if (ok) .confirmed else .rejected,
                    .native_turn_id = if (ok) s.native_id else null,
                    .detail = if (ok) null else try out.alloc.dupe(u8, err_msg),
                } });
            },
            .abort, .compact => {},
        }
    }

    fn on_message_update(self: *Pi, msg: std.json.Value, out: *proto.Out) !void {
        const ev = json_util.value_get(msg, "assistantMessageEvent") orelse return;
        const etype = json_util.value_str(ev, "type") orelse return;
        const delta = json_util.value_str(ev, "delta") orelse "";
        const idx = json_util.value_u64(ev, "contentIndex") orelse 0;
        const key = try std.fmt.allocPrint(out.alloc, "pi-{d}-{d}", .{ self.msg_n, idx });
        if (std.mem.eql(u8, etype, "text_delta")) {
            try out.delta(false, key, delta);
        } else if (std.mem.eql(u8, etype, "thinking_delta")) {
            try out.delta(true, key, delta);
        } else if (std.mem.eql(u8, etype, "error")) {
            if (self.turn) |*t| t.errored = true;
            const err_m = json_util.value_str(ev, "errorMessage") orelse "pi assistant error";
            try out.emit_error(proto.classify_error(err_m), err_m);
        }
    }

    fn on_message_end(self: *Pi, msg: std.json.Value, out: *proto.Out) !void {
        const inner = json_util.value_get(msg, "message") orelse return;
        if (!std.mem.eql(u8, json_util.value_str(inner, "role") orelse "", "assistant")) return;
        if (json_util.value_get(inner, "usage")) |u| {
            if (self.turn) |*t| {
                t.usage.input +|= json_util.value_u64(u, "input") orelse 0;
                t.usage.output +|= json_util.value_u64(u, "output") orelse 0;
                t.usage.cache_read +|= json_util.value_u64(u, "cacheRead") orelse 0;
                t.usage.cache_write +|= json_util.value_u64(u, "cacheWrite") orelse 0;
                if (json_util.value_u64(u, "totalTokens")) |ctx| t.usage.context_tokens = ctx;
                if (json_util.value_get(u, "cost")) |cost| {
                    if (json_util.value_f64(cost, "total")) |ct| t.cost += ct;
                }
            }
        }
        try out.flush_open();
    }

    fn on_settled(self: *Pi, msg: std.json.Value, out: *proto.Out) !void {
        if (self.turn) |t| {
            if (t.request_id != null and !t.answered) {
                try out.notice("stale_settle", "pi settled before answering prompt");
                return;
            }
        }
        const turn = self.turn orelse return;
        self.turn = null;
        if (!turn.started) {
            try out.emit(.{ .turn_started = .{ .request_id = turn.request_id, .native_turn_id = turn.native_id } });
        }
        const aborted = json_util.value_bool(msg, "aborted") == true;
        const stop: proto.Stop = if (aborted) .cancelled else if (turn.errored) .@"error" else .end_turn;
        var usage = turn.usage;
        if (turn.cost > 0.0) usage.cost_usd = turn.cost;
        try out.emit(.{ .turn_settled = .{
            .stop = stop,
            .native_stop = if (aborted) "aborted" else "settled",
            .usage = usage,
        } });
    }

    pub fn send_user(self: *Pi, request_id: []const u8, text: []const u8, steer: ?[]const u8, out: *proto.Out) !void {
        if (!self.ready) return error.NotReady;
        if (steer) |expected| {
            const t = self.turn orelse return error.TurnEnded;
            if (!std.mem.eql(u8, t.native_id, expected)) return error.TurnEnded;
            try self.command("steer", .{ .message = text }, .{ .steer = .{
                .req_id = try out.alloc.dupe(u8, request_id),
                .native_id = try out.alloc.dupe(u8, expected),
            } }, request_id, out);
            return;
        }
        if (self.turn != null) return error.Busy;
        self.turn_counter += 1;
        const nid = try std.fmt.allocPrint(out.alloc, "pi-turn-{d}", .{self.turn_counter});
        const req_dup = try out.alloc.dupe(u8, request_id);
        try self.command("prompt", .{ .message = text }, .{ .prompt = req_dup }, request_id, out);
        self.turn = .{ .request_id = req_dup, .native_id = nid };
    }

    pub fn respond_approval(self: *Pi, id: []const u8, decision: proto.Decision, out: *proto.Out) !void {
        _ = self;
        _ = id;
        _ = decision;
        _ = out;
        return error.UnknownApproval;
    }

    pub fn interrupt(self: *Pi, out: *proto.Out) !void {
        if (self.turn != null) {
            try self.command("abort", .{}, .abort, null, out);
        }
    }

    pub fn compact(self: *Pi, out: *proto.Out) !void {
        if (!self.ready) return error.NotReady;
        if (self.turn != null) return error.Busy;
        try self.command("compact", .{}, .compact, null, out);
    }
};

fn tool_kind(name: []const u8) proto.ToolKind {
    if (std.mem.eql(u8, name, "bash")) return .command;
    if (std.mem.eql(u8, name, "read")) return .read;
    if (std.mem.eql(u8, name, "edit") or std.mem.eql(u8, name, "write")) return .edit;
    if (std.mem.eql(u8, name, "grep") or std.mem.eql(u8, name, "find") or std.mem.eql(u8, name, "ls")) return .search;
    if (std.mem.startsWith(u8, name, "mcp")) return .mcp;
    return .other;
}
