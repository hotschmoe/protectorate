const std = @import("std");
const tiger = @import("../tiger.zig");
const json_util = @import("../json_util.zig");
const proto = @import("protocol.zig");

fn tool_input(arena: std.mem.Allocator, item: std.json.Value) !std.json.Value {
    if (item != .object) return .null;
    var input: std.json.ObjectMap = .empty;
    var it = item.object.iterator();
    while (it.next()) |field| {
        const key = field.key_ptr.*;
        if (std.mem.eql(u8, key, "id") or std.mem.eql(u8, key, "status") or
            std.mem.eql(u8, key, "processId") or std.mem.eql(u8, key, "aggregatedOutput") or
            std.mem.eql(u8, key, "exitCode") or std.mem.eql(u8, key, "durationMs")) continue;
        try input.put(arena, key, field.value_ptr.*);
    }
    return .{ .object = input };
}

const RpcKind = union(enum) {
    initialize,
    thread_open,
    turn_start: []const u8,
    steer: []const u8,
    interrupt,
};

pub const Codex = struct {
    cwd: []const u8,
    model: ?[]const u8 = null,
    provider: ?[]const u8 = null,
    effort: ?[]const u8 = null,
    resume_id: ?[]const u8 = null,
    supervised: bool = false,
    thread_id: ?[]const u8 = null,
    turn_native: ?[]const u8 = null,
    turn_request_id: ?[]const u8 = null,
    turn_active: bool = false,
    turn_started: bool = false,
    interrupt_wanted: bool = false,
    next_id: u64 = 1,
    pending_rpc: std.AutoHashMapUnmanaged(u64, RpcKind) = .empty,
    approvals: std.StringHashMapUnmanaged([]const u8) = .empty,

    pub fn build_argv(arena: std.mem.Allocator, spec: proto.SpawnSpec) ![]const []const u8 {
        var argv: std.ArrayList([]const u8) = .empty;
        try argv.appendSlice(arena, &.{ spec.program, "app-server" });
        for ([_]struct { key: []const u8, value: ?[]const u8 }{
            .{ .key = "model", .value = spec.model },
            .{ .key = "model_provider", .value = spec.provider },
            .{ .key = "model_reasoning_effort", .value = spec.effort },
        }) |setting| {
            const value = setting.value orelse continue;
            const quoted = try std.json.Stringify.valueAlloc(arena, value, .{});
            try argv.appendSlice(arena, &.{ "-c", try std.fmt.allocPrint(arena, "{s}={s}", .{ setting.key, quoted }) });
        }
        try argv.appendSlice(arena, spec.extra_args);
        return argv.items;
    }

    pub fn start(self: *Codex, out: *proto.Out) !void {
        try self.call("initialize", .{
            .clientInfo = .{ .name = "protectorate", .title = "Protectorate", .version = "0.7.0" },
            .capabilities = .{
                .experimentalApi = true,
                .optOutNotificationMethods = [_][]const u8{
                    "turn/diff/updated",
                    "item/commandExecution/outputDelta",
                    "item/fileChange/outputDelta",
                },
            },
        }, .initialize, out);
    }

    fn call(self: *Codex, method: []const u8, params: anytype, kind: RpcKind, out: *proto.Out) !void {
        const id = self.next_id;
        self.next_id += 1;
        try self.pending_rpc.put(out.alloc, id, kind);
        try out.write_value(.{ .id = id, .method = method, .params = params }, null);
    }

    pub fn on_line(self: *Codex, arena: std.mem.Allocator, line: []const u8, out: *proto.Out) !void {
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
            try self.on_server_request(msg, method_opt.?, out);
        } else if (method_opt != null) {
            try self.on_note(arena, method_opt.?, json_util.value_get(msg, "params") orelse .null, out);
        } else if (has_id) {
            try self.on_reply(msg, out);
        } else {
            try out.emit_error(.protocol, "codex message missing method and id");
        }
    }

    fn on_reply(self: *Codex, msg: std.json.Value, out: *proto.Out) !void {
        const id_val = json_util.value_get(msg, "id") orelse return;
        const id = json_util.as_u64(id_val) orelse return;
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
                const approval_pol = if (self.supervised) "untrusted" else "never";
                if (self.resume_id) |rid| {
                    try self.call("thread/resume", .{
                        .threadId = rid,
                        .cwd = self.cwd,
                        .model = self.model,
                        .modelProvider = self.provider,
                        .approvalPolicy = approval_pol,
                    }, .thread_open, out);
                } else {
                    try self.call("thread/start", .{
                        .cwd = self.cwd,
                        .model = self.model,
                        .modelProvider = self.provider,
                        .approvalPolicy = approval_pol,
                    }, .thread_open, out);
                }
            },
            .thread_open => {
                if (failed) {
                    out.refused = true;
                    try out.emit_error(.protocol, err_msg);
                    return;
                }
                const thread_obj = json_util.value_get(result, "thread") orelse .null;
                const tid = json_util.value_str(thread_obj, "id") orelse {
                    out.refused = true;
                    try out.emit_error(.protocol, "codex thread response missing id");
                    return;
                };
                self.thread_id = try out.alloc.dupe(u8, tid);
                try out.emit(.{ .session_info = .{
                    .harness = .codex,
                    .native_id = self.thread_id.?,
                    .model = json_util.value_str(result, "model") orelse self.model,
                    .provider = json_util.value_str(result, "modelProvider") orelse self.provider,
                    .effort = self.effort orelse json_util.value_str(result, "reasoningEffort"),
                    .cwd = json_util.value_str(result, "cwd") orelse self.cwd,
                } });
            },
            .turn_start => |req_id| {
                if (failed) {
                    self.turn_active = false;
                    self.turn_request_id = null;
                    try out.emit(.{ .prompt_ack = .{
                        .request_id = req_id,
                        .ack = .rejected,
                        .detail = try out.alloc.dupe(u8, err_msg),
                    } });
                    return;
                }
                const turn_obj = json_util.value_get(result, "turn") orelse .null;
                const native = json_util.value_str(turn_obj, "id") orelse {
                    try out.emit_error(.protocol, "codex turn/start missing turn id");
                    return;
                };
                const dup_nat = try out.alloc.dupe(u8, native);
                self.turn_native = dup_nat;
                try out.emit(.{ .prompt_ack = .{
                    .request_id = req_id,
                    .ack = .confirmed,
                    .native_turn_id = dup_nat,
                } });
                if (self.interrupt_wanted) {
                    self.interrupt_wanted = false;
                    try self.send_interrupt(out);
                }
            },
            .steer => |req_id| {
                if (failed) {
                    try out.emit(.{ .prompt_ack = .{
                        .request_id = req_id,
                        .ack = .rejected,
                        .detail = try out.alloc.dupe(u8, err_msg),
                    } });
                } else {
                    try out.emit(.{ .prompt_ack = .{
                        .request_id = req_id,
                        .ack = .confirmed,
                        .native_turn_id = json_util.value_str(result, "turnId"),
                    } });
                }
            },
            .interrupt => {},
        }
    }

    fn on_server_request(self: *Codex, msg: std.json.Value, method: []const u8, out: *proto.Out) !void {
        const id_val = json_util.value_get(msg, "id") orelse return;
        const raw_id = try json_util.stringify_alloc(out.alloc, id_val);
        const p = json_util.value_get(msg, "params") orelse .null;
        const kind: proto.ToolKind = if (std.mem.eql(u8, method, "item/commandExecution/requestApproval"))
            .command
        else if (std.mem.eql(u8, method, "item/fileChange/requestApproval"))
            .edit
        else if (std.mem.eql(u8, method, "item/permissions/requestApproval"))
            .other
        else {
            try out.write_value(.{
                .id = id_val,
                .@"error" = .{ .code = -32601, .message = "unsupported approval method" },
            }, null);
            return;
        };

        const app_id = try std.fmt.allocPrint(out.alloc, "codex-{s}", .{raw_id});
        const name = if (kind == .command) "shell" else if (kind == .edit) "apply_patch" else "permissions";
        const sum = try proto.summarize_alloc(out.alloc, name, p);
        const inp = try json_util.stringify_alloc(out.alloc, p);
        const opts = try out.alloc.dupe(proto.Decision, &.{ .allow_once, .allow_session, .deny, .cancel });
        try out.emit(.{ .approval_request = .{
            .id = app_id,
            .tool = kind,
            .name = name,
            .summary = sum,
            .input_json = inp,
            .options = opts,
        } });
        try self.approvals.put(out.alloc, app_id, raw_id);
        if (!self.supervised) {
            try self.respond_approval(app_id, .allow_once, out);
        }
    }

    fn on_note(self: *Codex, arena: std.mem.Allocator, method: []const u8, p: std.json.Value, out: *proto.Out) !void {
        if (std.mem.eql(u8, method, "turn/started")) {
            if (!self.turn_started) {
                self.turn_started = true;
                self.turn_active = true;
                try out.emit(.{ .turn_started = .{
                    .request_id = self.turn_request_id,
                    .native_turn_id = self.turn_native,
                } });
            }
        } else if (std.mem.eql(u8, method, "item/agentMessage/delta")) {
            const id = json_util.value_str(p, "itemId") orelse "msg";
            const d = json_util.value_str(p, "delta") orelse "";
            try out.delta(false, id, d);
        } else if (std.mem.eql(u8, method, "item/reasoning/summaryTextDelta") or std.mem.eql(u8, method, "item/reasoning/textDelta")) {
            const id = json_util.value_str(p, "itemId") orelse "reason";
            const d = json_util.value_str(p, "delta") orelse "";
            try out.delta(true, id, d);
        } else if (std.mem.eql(u8, method, "item/started")) {
            const item = json_util.value_get(p, "item") orelse return;
            const id = json_util.value_str(item, "id") orelse "";
            const itype = json_util.value_str(item, "type") orelse "";
            const input = try tool_input(arena, item);
            if (std.mem.eql(u8, itype, "commandExecution")) {
                try out.tool_call(id, .command, "shell", input);
            } else if (std.mem.eql(u8, itype, "fileChange")) {
                try out.tool_call(id, .edit, "apply_patch", input);
            } else if (std.mem.eql(u8, itype, "mcpToolCall")) {
                try out.tool_call(id, .mcp, json_util.value_str(item, "tool") orelse "mcp", input);
            } else if (std.mem.eql(u8, itype, "webSearch")) {
                try out.tool_call(id, .web, "web_search", input);
            }
        } else if (std.mem.eql(u8, method, "item/completed")) {
            const item = json_util.value_get(p, "item") orelse return;
            const id = json_util.value_str(item, "id") orelse "";
            const itype = json_util.value_str(item, "type") orelse "";
            if (std.mem.eql(u8, itype, "agentMessage")) {
                try out.finish_text(false, id, json_util.value_str(item, "text") orelse "");
            } else if (std.mem.eql(u8, itype, "reasoning")) {
                const txt = try proto.content_text_alloc(arena, json_util.value_get(item, "summary") orelse .null);
                try out.finish_text(true, id, txt);
            } else if (std.mem.eql(u8, itype, "contextCompaction")) {
                try out.emit(.{ .compaction = .{} });
            } else if (std.mem.eql(u8, itype, "commandExecution") or std.mem.eql(u8, itype, "fileChange") or std.mem.eql(u8, itype, "mcpToolCall")) {
                const st_s = json_util.value_str(item, "status") orelse "";
                var st: proto.ToolStatus = if (std.mem.eql(u8, st_s, "failed")) .failed else if (std.mem.eql(u8, st_s, "declined")) .declined else .completed;
                if (json_util.value_i64(item, "exitCode")) |ec| {
                    if (ec != 0 and st == .completed) st = .failed;
                }
                const out_txt = json_util.value_str(item, "aggregatedOutput") orelse json_util.value_str(item, "output") orelse "";
                const bo = try proto.bounded_output(out.alloc, out_txt);
                try out.emit(.{ .tool_result = .{
                    .id = try out.alloc.dupe(u8, id),
                    .status = st,
                    .output = bo.text,
                    .truncated = bo.truncated,
                    .duration_ms = json_util.value_u64(item, "durationMs"),
                } });
            }
        } else if (std.mem.eql(u8, method, "turn/completed")) {
            const turn = json_util.value_get(p, "turn") orelse .null;
            const status = json_util.value_str(turn, "status") orelse "completed";
            const stop: proto.Stop = if (std.mem.eql(u8, status, "interrupted"))
                .cancelled
            else if (std.mem.eql(u8, status, "failed"))
                .@"error"
            else
                .end_turn;
            try out.emit(.{ .turn_settled = .{
                .stop = stop,
                .native_stop = try out.alloc.dupe(u8, status),
            } });
            self.turn_active = false;
            self.turn_started = false;
            self.turn_request_id = null;
            self.turn_native = null;
        } else if (std.mem.eql(u8, method, "error")) {
            const err_obj = json_util.value_get(p, "error") orelse p;
            const txt = json_util.value_str(err_obj, "message") orelse "codex error";
            try out.emit_error(proto.classify_error(txt), txt);
        }
    }

    pub fn send_user(
        self: *Codex,
        request_id: []const u8,
        text: []const u8,
        steer: ?[]const u8,
        out: *proto.Out,
    ) !void {
        const tid = self.thread_id orelse return error.NotReady;
        const input = [_]struct { type: []const u8, text: []const u8 }{.{ .type = "text", .text = text }};
        const req_dup = try out.alloc.dupe(u8, request_id);
        if (steer) |expected| {
            const cur = self.turn_native orelse return error.TurnEnded;
            if (!std.mem.eql(u8, cur, expected)) return error.TurnEnded;
            try self.call("turn/steer", .{
                .threadId = tid,
                .input = &input,
                .expectedTurnId = expected,
            }, .{ .steer = req_dup }, out);
            return;
        }
        if (self.turn_active) return error.Busy;
        try self.call("turn/start", .{
            .threadId = tid,
            .input = &input,
            .clientUserMessageId = req_dup,
            .effort = self.effort,
        }, .{ .turn_start = req_dup }, out);
        self.turn_active = true;
        self.turn_started = false;
        self.turn_request_id = req_dup;
    }

    pub fn respond_approval(self: *Codex, id: []const u8, decision: proto.Decision, out: *proto.Out) !void {
        const raw_id = self.approvals.get(id) orelse return error.UnknownApproval;
        _ = self.approvals.remove(id);
        const ans: []const u8 = switch (decision) {
            .allow_once => "accept",
            .allow_session => "acceptForSession",
            .deny => "decline",
            .cancel => "cancel",
        };
        const line = try std.fmt.allocPrint(out.alloc, "{{\"id\":{s},\"result\":{{\"decision\":\"{s}\"}}}}", .{ raw_id, ans });
        try out.write_raw_line(line, null);
        try out.emit(.{ .approval_decision = .{
            .id = try out.alloc.dupe(u8, id),
            .decision = decision,
            .by = if (self.supervised) "owner" else "autonomous",
            .confirmed = true,
        } });
    }

    pub fn interrupt(self: *Codex, out: *proto.Out) !void {
        if (!self.turn_active) return;
        if (self.turn_native == null) {
            self.interrupt_wanted = true;
            return;
        }
        try self.send_interrupt(out);
    }

    fn send_interrupt(self: *Codex, out: *proto.Out) !void {
        const tid = self.thread_id orelse return;
        const turn_id = self.turn_native orelse return;
        try self.call("turn/interrupt", .{ .threadId = tid, .turnId = turn_id }, .interrupt, out);
    }
};
