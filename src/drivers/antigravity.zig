const std = @import("std");
const json_util = @import("../json_util.zig");
const proto = @import("protocol.zig");

pub const Antigravity = struct {
    session_id: []const u8,
    model: ?[]const u8 = null,
    effort: ?[]const u8 = null,
    supervised: bool = false,
    turn_active: bool = false,
    turn_acked: bool = false,
    turn_emitted_text: bool = false,
    turn_request_id: ?[]const u8 = null,
    turn_counter: u64 = 0,
    context_tokens: ?u64 = null,
    prev_input: u64 = 0,
    prev_output: u64 = 0,
    prev_cache_read: u64 = 0,
    prev_duration_ms: u64 = 0,
    emitted_tools: std.AutoHashMapUnmanaged(u64, void) = .empty,

    pub fn build_argv(arena: std.mem.Allocator, spec: proto.SpawnSpec) ![]const []const u8 {
        var argv: std.ArrayList([]const u8) = .empty;
        try argv.appendSlice(arena, &.{
            spec.program,
            "--input-format",
            "stream-json",
            "--output-format",
            "stream-json",
        });
        if (spec.resume_id) |rid| {
            try argv.appendSlice(arena, &.{ "--conversation", rid });
        }
        if (spec.model) |m| {
            try argv.appendSlice(arena, &.{ "--model", m });
        }
        if (spec.effort) |e| {
            try argv.appendSlice(arena, &.{ "--effort", e });
        }
        switch (spec.permission) {
            .unsupervised => try argv.append(arena, "--dangerously-skip-permissions"),
            .supervised => try argv.appendSlice(arena, &.{ "--mode", "accept-edits" }),
        }
        try argv.appendSlice(arena, spec.extra_args);
        return argv.items;
    }

    pub fn start(self: *Antigravity, out: *proto.Out) !void {
        try out.emit(.{ .session_info = .{
            .harness = .antigravity,
            .native_id = self.session_id,
            .model = self.model,
            .effort = self.effort,
        } });
    }

    pub fn on_line(self: *Antigravity, arena: std.mem.Allocator, line: []const u8, out: *proto.Out) !void {
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
        const ev = json_util.value_str(msg, "event") orelse {
            out.stray += 1;
            return;
        };

        if (json_util.value_str(msg, "conversation_id")) |cid| {
            if (cid.len > 0) self.session_id = try out.alloc.dupe(u8, cid);
        }

        if (std.mem.eql(u8, ev, "init")) {
            try self.on_init(msg, out);
        } else if (std.mem.eql(u8, ev, "step_update")) {
            try self.on_step_update(arena, msg, out);
        } else if (std.mem.eql(u8, ev, "result")) {
            try self.on_result(msg, out);
        } else {
            try out.unmapped(ev);
        }
    }

    fn on_init(self: *Antigravity, msg: std.json.Value, out: *proto.Out) !void {
        const init_obj = json_util.value_get(msg, "init") orelse .null;
        if (json_util.value_str(init_obj, "model")) |m| {
            if (m.len > 0) self.model = try out.alloc.dupe(u8, m);
        }
        try out.emit(.{ .session_info = .{
            .harness = .antigravity,
            .native_id = self.session_id,
            .model = self.model,
            .effort = self.effort,
            .cwd = json_util.value_str(init_obj, "cwd"),
        } });
    }

    fn ensure_turn_started(self: *Antigravity, out: *proto.Out) !void {
        if (!self.turn_active) {
            self.turn_counter += 1;
            self.turn_active = true;
            self.turn_emitted_text = false;
            self.emitted_tools.clearRetainingCapacity();
        }
        if (!self.turn_acked) {
            self.turn_acked = true;
            if (self.turn_request_id) |req_id| {
                try out.emit(.{ .prompt_ack = .{
                    .request_id = req_id,
                    .ack = .confirmed,
                } });
            }
            try out.emit(.{ .turn_started = .{
                .request_id = self.turn_request_id,
                .native_turn_id = null,
            } });
        }
    }

    fn on_step_update(self: *Antigravity, arena: std.mem.Allocator, msg: std.json.Value, out: *proto.Out) !void {
        const su = json_util.value_get(msg, "step_update") orelse return;
        if (su != .object) return;
        if (json_util.value_str(su, "conversation_id")) |cid| {
            if (cid.len > 0) self.session_id = try out.alloc.dupe(u8, cid);
        }

        const step_idx = json_util.value_u64(su, "step_index") orelse 0;
        const st_str = json_util.value_str(su, "state") orelse "";
        const is_done = std.mem.eql(u8, st_str, "DONE");
        const step_type = json_util.value_str(su, "step_type") orelse "";

        if (std.mem.eql(u8, step_type, "user_input")) {
            try self.ensure_turn_started(out);
            return;
        }
        if (std.mem.eql(u8, step_type, "checkpoint")) {
            return;
        }

        try self.ensure_turn_started(out);

        if (std.mem.eql(u8, step_type, "agent_response") or
            std.mem.eql(u8, step_type, "thinking") or
            std.mem.eql(u8, step_type, "thought"))
        {
            const reasoning = !std.mem.eql(u8, step_type, "agent_response");
            if (json_util.value_get(su, "usage")) |u| {
                const inp = out.count_u64("input_tokens", json_util.value_get(u, "input_tokens") orelse .null) orelse 0;
                const cr = out.count_u64("cache_read_tokens", json_util.value_get(u, "cache_read_tokens") orelse .null) orelse 0;
                const tot = out.count_u64("total_tokens", json_util.value_get(u, "total_tokens") orelse .null) orelse 0;
                const ctx = @max(inp +| cr, tot);
                if (ctx > 0) self.context_tokens = ctx;
            }
            if (json_util.value_str(su, "text_delta")) |td| {
                if (td.len > 0) {
                    const key = try std.fmt.allocPrint(out.alloc, "agy-step-{d}-{d}", .{ self.turn_counter, step_idx });
                    try out.delta(reasoning, key, td);
                    if (!reasoning) self.turn_emitted_text = true;
                }
            }
            if (is_done) {
                try out.flush_open();
            }
            return;
        }

        if (std.mem.eql(u8, step_type, "tool")) {
            const tinfo = json_util.value_get(su, "tool_info") orelse .null;
            const tname = json_util.value_str(su, "tool_name") orelse
                json_util.value_str(tinfo, "name") orelse
                "tool";
            const params = json_util.value_get(tinfo, "parameters") orelse .null;
            const tid = try std.fmt.allocPrint(arena, "agy-tool-{d}-{d}", .{ self.turn_counter, step_idx });

            if (!self.emitted_tools.contains(step_idx)) {
                try self.emitted_tools.put(out.alloc, step_idx, {});
                try out.tool_call(tid, tool_kind(tname), tname, params);
                if (edit_path(tname, params)) |ep| {
                    var fd = try out.alloc.alloc(proto.FileDelta, 1);
                    fd[0] = .{ .path = try out.alloc.dupe(u8, ep), .op = "edit" };
                    try out.emit(.{ .file_change = .{ .tool_id = try out.alloc.dupe(u8, tid), .files = fd } });
                }
            }

            if (is_done) {
                const err_obj = json_util.value_get(tinfo, "error");
                const has_err = err_obj != null and err_obj.? != .null;
                const raw_out = if (has_err)
                    (json_util.value_str(err_obj.?, "message") orelse json_util.value_str(tinfo, "output") orelse "tool error")
                else
                    (json_util.value_str(tinfo, "output") orelse "");
                const bo = try proto.bounded_output(out.alloc, raw_out);
                const dur_ms: ?u64 = if (json_util.value_f64(su, "duration_seconds")) |ds|
                    if (ds >= 0.0) @as(u64, @intFromFloat(ds * 1000.0)) else null
                else
                    null;

                try out.emit(.{ .tool_result = .{
                    .id = try out.alloc.dupe(u8, tid),
                    .status = if (has_err) .failed else .completed,
                    .output = bo.text,
                    .truncated = bo.truncated,
                    .duration_ms = dur_ms,
                } });
            }
        }
    }

    fn on_result(self: *Antigravity, msg: std.json.Value, out: *proto.Out) !void {
        const res = json_util.value_get(msg, "result") orelse .null;
        if (json_util.value_str(res, "conversation_id")) |cid| {
            if (cid.len > 0) self.session_id = try out.alloc.dupe(u8, cid);
        }

        try self.ensure_turn_started(out);

        if (!self.turn_emitted_text) {
            if (json_util.value_str(res, "response")) |resp_txt| {
                if (resp_txt.len > 0) {
                    const rkey = try std.fmt.allocPrint(out.alloc, "agy-res-{d}", .{self.turn_counter});
                    try out.finish_text(false, rkey, resp_txt);
                    self.turn_emitted_text = true;
                }
            }
        }

        const st = json_util.value_str(res, "status") orelse "SUCCESS";
        if (json_util.value_str(res, "error")) |err_txt| {
            if (err_txt.len > 0) {
                try out.emit_error(proto.classify_error(err_txt), err_txt);
            }
        }

        const u = json_util.value_get(res, "usage") orelse .null;
        const cum_in = out.count_u64("input_tokens", json_util.value_get(u, "input_tokens") orelse .null) orelse 0;
        const cum_out = out.count_u64("output_tokens", json_util.value_get(u, "output_tokens") orelse .null) orelse 0;
        const cum_cr = out.count_u64("cache_read_tokens", json_util.value_get(u, "cache_read_tokens") orelse .null) orelse 0;
        const cum_tot = out.count_u64("total_tokens", json_util.value_get(u, "total_tokens") orelse .null) orelse 0;

        const d_in = delta_cum(&self.prev_input, cum_in);
        const d_out = delta_cum(&self.prev_output, cum_out);
        const d_cr = delta_cum(&self.prev_cache_read, cum_cr);

        const cum_dur_ms: ?u64 = if (json_util.value_f64(res, "duration_seconds")) |ds|
            if (ds >= 0.0) @as(u64, @intFromFloat(ds * 1000.0)) else null
        else
            null;
        const d_dur: ?u64 = if (cum_dur_ms) |cm| delta_cum(&self.prev_duration_ms, cm) else null;

        const stop: proto.Stop = if (std.mem.eql(u8, st, "SUCCESS"))
            .end_turn
        else if (std.mem.eql(u8, st, "CANCELED") or std.mem.eql(u8, st, "INTERRUPTED"))
            .cancelled
        else if (std.mem.eql(u8, st, "ERROR") or std.mem.eql(u8, st, "INVALID"))
            .@"error"
        else
            .other;

        try out.emit(.{ .turn_settled = .{
            .stop = stop,
            .native_stop = try out.alloc.dupe(u8, st),
            .usage = .{
                .input = d_in,
                .output = d_out,
                .cache_read = d_cr,
                .context_tokens = self.context_tokens orelse if (cum_tot > 0) cum_tot else null,
            },
            .duration_ms = d_dur,
        } });

        self.turn_active = false;
        self.turn_acked = false;
        self.turn_emitted_text = false;
        self.turn_request_id = null;
        self.emitted_tools.clearRetainingCapacity();
    }

    pub fn send_user(
        self: *Antigravity,
        request_id: []const u8,
        text: []const u8,
        steer: ?[]const u8,
        out: *proto.Out,
    ) !void {
        if (steer != null) return error.Unsupported;
        if (self.turn_active) return error.Busy;
        try out.write_value(.{
            .event = "user",
            .message = .{ .content = text },
        }, request_id);
        self.turn_counter += 1;
        self.turn_active = true;
        self.turn_acked = false;
        self.turn_emitted_text = false;
        self.turn_request_id = try out.alloc.dupe(u8, request_id);
        self.emitted_tools.clearRetainingCapacity();
    }

    pub fn respond_approval(self: *Antigravity, id: []const u8, decision: proto.Decision, out: *proto.Out) !void {
        _ = self;
        _ = id;
        _ = decision;
        _ = out;
        return error.UnknownApproval;
    }

    pub fn interrupt(self: *Antigravity, out: *proto.Out) !void {
        // Interrupting agy uses OS signals (SIGINT via Session.interrupt); writing control_request would exit with code 2.
        _ = self;
        _ = out;
    }

    pub fn compact(self: *Antigravity, out: *proto.Out) !void {
        _ = self;
        _ = out;
        return error.Unsupported;
    }
};

fn delta_cum(prev: *u64, cur: u64) u64 {
    const d = if (cur >= prev.*) cur - prev.* else cur;
    prev.* = cur;
    return d;
}

fn tool_kind(name: []const u8) proto.ToolKind {
    if (std.mem.eql(u8, name, "run_command")) return .command;
    if (std.mem.eql(u8, name, "read_file") or
        std.mem.eql(u8, name, "view_file") or
        std.mem.eql(u8, name, "view_file_outline") or
        std.mem.eql(u8, name, "view_code_item") or
        std.mem.eql(u8, name, "list_dir"))
    {
        return .read;
    }
    if (std.mem.eql(u8, name, "write_file") or
        std.mem.eql(u8, name, "write_to_file") or
        std.mem.eql(u8, name, "edit_file") or
        std.mem.eql(u8, name, "replace_file_content") or
        std.mem.eql(u8, name, "multi_replace_file_content"))
    {
        return .edit;
    }
    if (std.mem.eql(u8, name, "grep_search") or std.mem.eql(u8, name, "find_by_name")) return .search;
    if (std.mem.eql(u8, name, "search_web") or
        std.mem.eql(u8, name, "read_url_content") or
        std.mem.eql(u8, name, "open_browser_url"))
    {
        return .web;
    }
    if (std.mem.eql(u8, name, "invoke_subagent")) return .subagent;
    if (std.mem.startsWith(u8, name, "mcp_")) return .mcp;
    return .other;
}

fn edit_path(name: []const u8, params: std.json.Value) ?[]const u8 {
    if (tool_kind(name) != .edit) return null;
    return json_util.value_str(params, "Path") orelse
        json_util.value_str(params, "TargetFile") orelse
        json_util.value_str(params, "file_path") orelse
        json_util.value_str(params, "path");
}
