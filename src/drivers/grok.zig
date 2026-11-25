const std = @import("std");
const json_util = @import("../json_util.zig");
const proto = @import("protocol.zig");

const RpcKind = union(enum) {
    initialize,
    session_open,
    model_set,
    effort_set,
    prompt: []const u8,
};

pub const Grok = struct {
    cwd: []const u8,
    model: ?[]const u8 = null,
    effort: ?[]const u8 = null,
    effective_model: ?[]const u8 = null,
    effective_effort: ?[]const u8 = null,
    resume_id: ?[]const u8 = null,
    supervised: bool = false,
    session_id: ?[]const u8 = null,
    turn_request_id: ?[]const u8 = null,
    turn_active: bool = false,
    acked: bool = false,
    next_id: u64 = 1,
    segment: u64 = 0,
    turns: u64 = 0,
    pending_rpc: std.AutoHashMapUnmanaged(u64, RpcKind) = .empty,
    approvals: std.StringHashMapUnmanaged(struct { rpc_id: []const u8, allow_opt: ?[]const u8 }) = .empty,

    pub fn build_argv(arena: std.mem.Allocator, spec: proto.SpawnSpec) ![]const []const u8 {
        var argv: std.ArrayList([]const u8) = .empty;
        try argv.appendSlice(arena, &.{ spec.program, "agent", "stdio" });
        try argv.appendSlice(arena, spec.extra_args);
        return argv.items;
    }

    pub fn start(self: *Grok, out: *proto.Out) !void {
        try self.call("initialize", .{
            .protocolVersion = @as(u32, 1),
            .clientCapabilities = .{},
            .clientInfo = .{ .name = "protectorate", .version = "0.7.0" },
        }, .initialize, out);
    }

    fn call(self: *Grok, method: []const u8, params: anytype, kind: RpcKind, out: *proto.Out) !void {
        const id = self.next_id;
        self.next_id += 1;
        try self.pending_rpc.put(out.alloc, id, kind);
        try out.write_value(.{ .jsonrpc = "2.0", .id = id, .method = method, .params = params }, null);
    }

    fn configure(self: *Grok, key: []const u8, value: []const u8, kind: RpcKind, out: *proto.Out) !void {
        try self.call("session/set_config_option", .{
            .sessionId = self.session_id.?,
            .configId = key,
            .value = value,
        }, kind, out);
    }

    fn configure_effort(self: *Grok, out: *proto.Out) !void {
        if (self.effort) |effort| {
            return self.configure("reasoning_effort", effort, .effort_set, out);
        }
        try self.session_info(out);
    }

    fn session_info(self: *Grok, out: *proto.Out) !void {
        const effort_notice = if (self.effort) |requested| notice: {
            if (self.effective_effort) |effective| {
                if (std.mem.eql(u8, requested, effective)) break :notice null;
                break :notice try std.fmt.allocPrint(out.alloc, "Grok used {s} effort after {s} was requested.", .{ effective, requested });
            }
            break :notice try std.fmt.allocPrint(out.alloc, "Grok did not report an active effort after {s} was requested.", .{requested});
        } else null;
        try out.emit(.{ .session_info = .{
            .harness = .grok,
            .native_id = self.session_id.?,
            .model = self.effective_model,
            .effort = self.effective_effort,
            .effort_notice = effort_notice,
            .cwd = self.cwd,
        } });
    }

    fn read_options(self: *Grok, result: std.json.Value, a: std.mem.Allocator) !void {
        const models = json_util.value_object(result, "models") orelse .null;
        var model = json_util.value_nonempty_str(models, "currentModelId");
        var effort: ?[]const u8 = null;
        for (json_util.value_array(result, "configOptions") orelse &.{}) |option| {
            const key = json_util.value_str(option, "id") orelse continue;
            const current = json_util.value_nonempty_str(option, "currentValue") orelse continue;
            if (std.mem.eql(u8, key, "model")) {
                model = current;
            } else if (std.mem.eql(u8, key, "reasoning_effort")) {
                effort = current;
            }
        }
        if (model) |id| self.effective_model = try a.dupe(u8, id);
        self.effective_effort = if (effort) |level| try a.dupe(u8, level) else null;
    }

    pub fn on_line(self: *Grok, arena: std.mem.Allocator, line: []const u8, out: *proto.Out) !void {
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
        const method_opt = json_util.value_str(msg, "method");
        const id_val = json_util.value_get(msg, "id");
        const has_id = id_val != null and id_val.? != .null;

        if (method_opt != null and has_id) {
            try self.on_request(msg, method_opt.?, out);
        } else if (method_opt != null) {
            try self.on_note(method_opt.?, json_util.value_get(msg, "params") orelse .null, out);
        } else if (has_id) {
            try self.on_reply(msg, out);
        } else {
            try out.emit_error(.protocol, "grok message missing method and id");
        }
    }

    fn on_reply(self: *Grok, msg: std.json.Value, out: *proto.Out) !void {
        const id = json_util.as_u64(json_util.value_get(msg, "id") orelse return) orelse return;
        const kind = self.pending_rpc.get(id) orelse return;
        _ = self.pending_rpc.remove(id);

        const err_obj = json_util.value_get(msg, "error");
        const failed = err_obj != null and err_obj.? != .null;
        const err_msg = if (failed) (json_util.value_str(err_obj.?, "message") orelse "rpc error") else "";
        const result = json_util.value_get(msg, "result") orelse .null;

        switch (kind) {
            .initialize => {
                if (failed) {
                    out.refused = true;
                    try out.emit_error(.protocol, err_msg);
                    return;
                }
                if (self.resume_id) |rid| {
                    try self.call("session/load", .{ .sessionId = rid, .cwd = self.cwd, .mcpServers = [_]std.json.Value{} }, .session_open, out);
                } else {
                    try self.call("session/new", .{ .cwd = self.cwd, .mcpServers = [_]std.json.Value{} }, .session_open, out);
                }
            },
            .session_open => {
                if (failed) {
                    out.refused = true;
                    try out.emit_error(.protocol, err_msg);
                    return;
                }
                const sid = json_util.value_str(result, "sessionId") orelse self.resume_id orelse {
                    out.refused = true;
                    try out.emit_error(.protocol, "grok session missing sessionId");
                    return;
                };
                self.session_id = try out.alloc.dupe(u8, sid);
                try self.read_options(result, out.alloc);
                if (self.model) |id_model| {
                    try self.configure("model", id_model, .model_set, out);
                } else {
                    try self.configure_effort(out);
                }
            },
            .model_set, .effort_set => {
                if (failed) {
                    out.refused = true;
                    try out.emit_error(.protocol, err_msg);
                    return;
                }
                try self.read_options(result, out.alloc);
                if (kind == .model_set) {
                    try self.configure_effort(out);
                } else {
                    try self.session_info(out);
                }
            },
            .prompt => |req_id| {
                self.ack(out);
                if (failed) {
                    try out.emit_error(proto.classify_error(err_msg), err_msg);
                    try out.emit(.{ .turn_settled = .{ .stop = .@"error", .native_stop = "error" } });
                } else {
                    const reason = json_util.value_str(result, "stopReason") orelse "end_turn";
                    const stop: proto.Stop = if (std.mem.eql(u8, reason, "cancelled")) .cancelled else .end_turn;
                    try out.emit(.{ .turn_settled = .{ .stop = stop, .native_stop = try out.alloc.dupe(u8, reason) } });
                }
                self.turn_active = false;
                self.turn_request_id = null;
                self.acked = false;
                _ = req_id;
            },
        }
    }

    fn ack(self: *Grok, out: *proto.Out) void {
        if (self.acked) return;
        self.acked = true;
        if (self.turn_request_id) |req_id| {
            out.emit(.{ .prompt_ack = .{ .request_id = req_id, .ack = .confirmed } }) catch {};
            out.emit(.{ .turn_started = .{ .request_id = req_id } }) catch {};
        }
    }

    fn on_request(self: *Grok, msg: std.json.Value, method: []const u8, out: *proto.Out) !void {
        const id_val = json_util.value_get(msg, "id") orelse return;
        const raw_id = try json_util.stringify_alloc(out.alloc, id_val);
        if (!std.mem.eql(u8, method, "session/request_permission")) {
            const line = try std.fmt.allocPrint(out.alloc, "{{\"jsonrpc\":\"2.0\",\"id\":{s},\"error\":{{\"code\":-32601,\"message\":\"unsupported\"}}}}", .{raw_id});
            try out.write_raw_line(line, null);
            return;
        }
        const p = json_util.value_get(msg, "params") orelse .null;
        const app_id = try std.fmt.allocPrint(out.alloc, "grok-{s}", .{raw_id});
        var allow_opt: ?[]const u8 = null;
        if (json_util.value_array(p, "options")) |opts| {
            for (opts) |o| {
                if (std.mem.eql(u8, json_util.value_str(o, "kind") orelse "", "allow_once")) {
                    allow_opt = json_util.value_str(o, "optionId");
                }
            }
        }
        const tc = json_util.value_get(p, "toolCall") orelse .null;
        const name = json_util.value_str(tc, "title") orelse "tool";
        const decisions = try out.alloc.dupe(proto.Decision, &.{ .allow_once, .deny, .cancel });
        try out.emit(.{ .approval_request = .{
            .id = app_id,
            .tool = .other,
            .name = try out.alloc.dupe(u8, name),
            .summary = try out.alloc.dupe(u8, name),
            .options = decisions,
        } });
        try self.approvals.put(out.alloc, app_id, .{
            .rpc_id = raw_id,
            .allow_opt = if (allow_opt) |ao| try out.alloc.dupe(u8, ao) else null,
        });
        if (!self.supervised) {
            try self.respond_approval(app_id, .allow_once, out);
        }
    }

    fn on_note(self: *Grok, method: []const u8, p: std.json.Value, out: *proto.Out) !void {
        if (!std.mem.eql(u8, method, "session/update")) return;
        const u = json_util.value_get(p, "update") orelse return;
        const kind = json_util.value_str(u, "sessionUpdate") orelse return;
        if (std.mem.eql(u8, kind, "agent_message_chunk") or std.mem.eql(u8, kind, "agent_thought_chunk")) {
            self.ack(out);
            const content = json_util.value_get(u, "content") orelse .null;
            const txt = json_util.value_str(content, "text") orelse "";
            const key = try std.fmt.allocPrint(out.alloc, "grok-{d}-{d}", .{ self.turns, self.segment });
            try out.delta(std.mem.eql(u8, kind, "agent_thought_chunk"), key, txt);
        } else if (std.mem.eql(u8, kind, "tool_call")) {
            self.ack(out);
            self.segment += 1;
            const id = json_util.value_str(u, "toolCallId") orelse "";
            const title = json_util.value_str(u, "title") orelse "tool";
            try out.tool_call(id, .other, title, json_util.value_get(u, "rawInput") orelse .null);
        } else if (std.mem.eql(u8, kind, "tool_call_update")) {
            const st = json_util.value_str(u, "status") orelse "";
            if (std.mem.eql(u8, st, "completed") or std.mem.eql(u8, st, "failed")) {
                const id = json_util.value_str(u, "toolCallId") orelse "";
                const res = json_util.value_str(u, "title") orelse st;
                try out.emit(.{ .tool_result = .{
                    .id = try out.alloc.dupe(u8, id),
                    .status = if (std.mem.eql(u8, st, "failed")) .failed else .completed,
                    .output = try out.alloc.dupe(u8, res),
                    .truncated = false,
                } });
            }
        }
    }

    pub fn send_user(self: *Grok, request_id: []const u8, text: []const u8, steer: ?[]const u8, out: *proto.Out) !void {
        const sid = self.session_id orelse return error.NotReady;
        if (steer != null) return error.Unsupported;
        if (self.turn_active) return error.Busy;
        const req_dup = try out.alloc.dupe(u8, request_id);
        const prompt = [_]struct { type: []const u8, text: []const u8 }{.{ .type = "text", .text = text }};
        try self.call("session/prompt", .{ .sessionId = sid, .prompt = &prompt }, .{ .prompt = req_dup }, out);
        self.turns += 1;
        self.turn_active = true;
        self.acked = false;
        self.turn_request_id = req_dup;
    }

    pub fn respond_approval(self: *Grok, id: []const u8, decision: proto.Decision, out: *proto.Out) !void {
        const app = self.approvals.get(id) orelse return error.UnknownApproval;
        _ = self.approvals.remove(id);
        const line = if ((decision == .allow_once or decision == .allow_session) and app.allow_opt != null)
            try std.fmt.allocPrint(out.alloc, "{{\"jsonrpc\":\"2.0\",\"id\":{s},\"result\":{{\"outcome\":{{\"outcome\":\"selected\",\"optionId\":\"{s}\"}}}}}}", .{ app.rpc_id, app.allow_opt.? })
        else
            try std.fmt.allocPrint(out.alloc, "{{\"jsonrpc\":\"2.0\",\"id\":{s},\"result\":{{\"outcome\":{{\"outcome\":\"cancelled\"}}}}}}", .{app.rpc_id});
        try out.write_raw_line(line, null);
        try out.emit(.{ .approval_decision = .{
            .id = try out.alloc.dupe(u8, id),
            .decision = decision,
            .by = if (self.supervised) "owner" else "autonomous",
            .confirmed = true,
        } });
    }

    pub fn interrupt(self: *Grok, out: *proto.Out) !void {
        const sid = self.session_id orelse return;
        if (!self.turn_active) return;
        try out.write_value(.{
            .jsonrpc = "2.0",
            .method = "session/cancel",
            .params = .{ .sessionId = sid },
        }, null);
    }
};
