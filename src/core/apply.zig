const std = @import("std");
const tiger = @import("../tiger.zig");
const model = @import("model.zig");
const event = @import("event.zig");
const intent = @import("intent.zig");
const state = @import("state.zig");

pub const ApplyError = error{
    NonMonotonicSeq,
    DuplicateId,
    NotFound,
    WorkspaceConflict,
    ChildLimitExceeded,
    InvalidState,
    OutOfMemory,
};

pub fn apply(
    gpa: std.mem.Allocator,
    st: *state.State,
    se: event.StampedEvent,
    intents: *std.ArrayList(intent.Intent),
) ApplyError!void {
    if (se.seq != st.event_seq + 1) return error.NonMonotonicSeq;
    const prev_seq = st.event_seq;
    st.event_seq = se.seq;
    errdefer st.event_seq = prev_seq;
    const now = se.ts_ms;
    const a = st.alloc();

    switch (se.event) {
        .sleeve_created => |e| {
            if (st.sleeves.contains(e.sleeve_id)) return error.DuplicateId;
            if (!model.path_ok(e.path)) return error.InvalidState;
            if (st.active_claim_conflict(e.sleeve_id, e.path, e.layout) != null) {
                return error.WorkspaceConflict;
            }
            if (e.parent_id) |pid| {
                const parent = st.sleeves.getPtr(pid) orelse return error.NotFound;
                if (parent.children_active >= model.max_children) return error.ChildLimitExceeded;
                parent.children_active += 1;
            }
            const id = try st.dupe_str(e.sleeve_id);
            const path = try st.dupe_str(e.path);
            try st.sleeves.put(a, id, .{
                .id = id,
                .name = try st.dupe_str(e.name),
                .role = e.role,
                .path = path,
                .workspace_id = try st.dupe_str(e.workspace_id),
                .parent_id = try st.dupe_opt(e.parent_id),
                .layout = e.layout,
                .branch = try st.dupe_opt(e.branch),
                .lifecycle = .provisioning,
                .budget_limit = e.budget_limit,
                .until_done_left = e.until_done,
            });
            try intents.append(gpa, .{ .provision_worktree = .{
                .sleeve_id = id,
                .path = path,
                .layout = e.layout,
            } });
        },
        .provisioned => |e| {
            const sl = st.sleeves.getPtr(e.sleeve_id) orelse return error.NotFound;
            if (sl.lifecycle != .provisioning) return;
            sl.branch = try st.dupe_opt(e.branch);
            sl.lifecycle = .stopped;
        },
        .provision_failed => |e| {
            const sl = st.sleeves.getPtr(e.sleeve_id) orelse return error.NotFound;
            sl.quota_launch = null;
            sl.lifecycle = .parked;
            sl.park_reason = .provision_failed;
            try intents.append(gpa, .{ .emit_notice = .{
                .sleeve_id = sl.id,
                .code = "provision_failed",
                .text = try st.dupe_str(e.reason),
            } });
        },
        .start_requested => |e| {
            const sl = st.sleeves.getPtr(e.sleeve_id) orelse return error.NotFound;
            if (sl.lifecycle == .removed) return error.InvalidState;
            if (e.target_len == 0 or e.target_len > model.max_targets) return error.InvalidState;
            if (sl.inc != null) return;
            if (st.active_claim_conflict(sl.id, sl.path, sl.layout) != null) return error.WorkspaceConflict;
            for (0..e.target_len) |i| sl.targets[i] = e.targets[i];
            sl.target_len = e.target_len;
            sl.preferred_mode = e.mode;
            sl.preferred_model = try st.dupe_opt(e.model_name);
            sl.preferred_provider = try st.dupe_opt(e.provider);
            sl.preferred_effort = try st.dupe_opt(e.effort);
            if (e.until_done) |ud| sl.until_done_left = ud;
            sl.quota_launch = null;
            sl.lifecycle = .launching;
            if (e.selection_reason) |reason| {
                try intents.append(gpa, .{ .record_quota_selection = .{
                    .sleeve_id = sl.id,
                    .source = .spawn,
                    .choice = .{
                        .harness = e.targets[0],
                        .model_name = sl.preferred_model,
                        .provider = sl.preferred_provider,
                        .reason = reason,
                    },
                } });
            }
            const next_g = sl.next_g + 1;
            try intents.append(gpa, .{ .write_brief = .{
                .sleeve_id = sl.id,
                .path = sl.path,
                .g = next_g,
                .cause = .manual,
            } });
            try intents.append(gpa, .{ .launch_incarnation = .{
                .sleeve_id = sl.id,
                .g = next_g,
                .harness = sl.targets[0],
                .mode = sl.preferred_mode,
                .model_name = sl.preferred_model orelse "",
                .provider = sl.preferred_provider orelse "",
                .effort = sl.preferred_effort,
            } });
        },
        .launched => |e| {
            const sl = st.sleeves.getPtr(e.sleeve_id) orelse return error.NotFound;
            if (sl.lifecycle == .removed) return error.InvalidState;
            if (st.active_claim_conflict(sl.id, sl.path, sl.layout) != null) return error.WorkspaceConflict;
            if (e.g <= sl.next_g) return error.InvalidState;
            fence_sleeve_approvals(st, sl.id, e.g);
            sl.quota_launch = null;
            sl.next_g = e.g;
            sl.lifecycle = .running;
            sl.park_reason = null;
            sl.inc = .{
                .g = e.g,
                .harness = e.harness,
                .mode = e.mode,
                .model_name = try st.dupe_str(e.model_name),
                .provider = try st.dupe_str(e.provider),
                .effort = try st.dupe_opt(e.effort),
                .activity = .booting,
                .started_at_ms = now,
                .last_event_ms = now,
            };
        },
        .session_bound => |e| {
            const sl = st.sleeves.getPtr(e.sleeve_id) orelse return error.NotFound;
            const inc = &(sl.inc orelse return);
            if (inc.g != e.g) return;
            inc.native_id = try st.dupe_str(e.native_id);
            if (inc.activity == .booting) {
                inc.activity = .idle;
                try maybe_deliver_next(gpa, st, sl, now, intents);
            }
        },
        .prompt_enqueued => |e| {
            const sl = st.sleeves.getPtr(e.sleeve_id) orelse return error.NotFound;
            if (sl.lifecycle == .removed or sl.lifecycle == .parked) return error.InvalidState;
            if (sl.role == .envoy and e.kind == .mail) {
                if (st.envoy_mail_win.count_in_window(now, model.envoy_mail_window_ms) >= model.envoy_mail_limit) {
                    try intents.append(gpa, .{ .emit_notice = .{
                        .sleeve_id = sl.id,
                        .code = "g12_envoy_mail_rate",
                        .text = "envoy mail rate limit reached; message dropped",
                    } });
                    return;
                }
                st.envoy_mail_win.push(now);
            }
            if (e.until_done) |ud| sl.until_done_left = ud;
            if (e.supersede) {
                sl.queued.clearRetainingCapacity();
                if (sl.inc) |*inc| {
                    if (inc.activity == .working or inc.activity == .delivering) {
                        inc.activity = .interrupting;
                        inc.stop_requested_ms = now;
                        try intents.append(gpa, .{ .interrupt_incarnation = .{ .sleeve_id = sl.id, .g = inc.g } });
                    }
                }
            }
            try sl.queued.append(a, .{
                .request_id = try st.dupe_str(e.request_id),
                .from = try st.dupe_str(e.from),
                .kind = e.kind,
                .mode = e.mode,
                .text = try st.dupe_str(e.text),
                .queued_at_ms = now,
                .until_done = e.until_done,
            });
            try maybe_deliver_next(gpa, st, sl, now, intents);
        },
        .prompt_delivered => |e| {
            const sl = st.sleeves.getPtr(e.sleeve_id) orelse return error.NotFound;
            const inc = &(sl.inc orelse return);
            if (inc.g != e.g) return;
            inc.last_event_ms = now;
            sl.oom_notice_pending = false;
        },
        .prompt_acked => |e| {
            const sl = st.sleeves.getPtr(e.sleeve_id) orelse return error.NotFound;
            const inc = &(sl.inc orelse return);
            if (inc.g != e.g) return;
            inc.last_event_ms = now;
            if (!e.confirmed) {
                if (inc.activity == .delivering) inc.activity = .idle;
                inc.delivered_request_id = null;
                inc.delivered_kind = null;
                try intents.append(gpa, .{ .emit_notice = .{
                    .sleeve_id = sl.id,
                    .code = "prompt_rejected",
                    .text = try st.dupe_opt(e.detail) orelse "prompt rejected",
                } });
            }
        },
        .turn_started => |e| {
            const sl = st.sleeves.getPtr(e.sleeve_id) orelse return error.NotFound;
            const inc = &(sl.inc orelse return);
            if (inc.g != e.g) return;
            if (inc.activity != .interrupting) inc.activity = .working;
            inc.turn += 1;
            inc.turn_started_ms = now;
            inc.last_event_ms = now;
            inc.tool_repeat_count = 0;
            inc.last_tool_sig = null;
            inc.turn_is_agent_mail = e.from_agent_mail or switch (inc.delivered_kind orelse .prompt) {
                .mail, .report, .agent_mail => true,
                .prompt, .system => false,
            };
            if (sl.role == .envoy) {
                sl.d9 = inc.turn_is_agent_mail;
            }
        },
        .tool_activity => |e| {
            const sl = st.sleeves.getPtr(e.sleeve_id) orelse return error.NotFound;
            const inc = &(sl.inc orelse return);
            if (inc.g != e.g) return;
            inc.last_event_ms = now;
            if (inc.last_tool_sig) |prev| {
                if (std.mem.eql(u8, prev, e.tool_sig)) {
                    inc.tool_repeat_count += 1;
                } else {
                    inc.last_tool_sig = try st.dupe_str(e.tool_sig);
                    inc.tool_repeat_count = 1;
                }
            } else {
                inc.last_tool_sig = try st.dupe_str(e.tool_sig);
                inc.tool_repeat_count = 1;
            }
            if (inc.tool_repeat_count >= model.loop_repeats and inc.activity != .interrupting) {
                inc.activity = .interrupting;
                inc.stop_requested_ms = now;
                try intents.append(gpa, .{ .emit_notice = .{
                    .sleeve_id = sl.id,
                    .code = "g6_loop_detection",
                    .text = "interrupting repeating tool loop",
                } });
                try intents.append(gpa, .{ .interrupt_incarnation = .{ .sleeve_id = sl.id, .g = inc.g } });
            }
        },
        .compacted => |e| {
            const sl = st.sleeves.getPtr(e.sleeve_id) orelse return error.NotFound;
            const inc = &(sl.inc orelse return);
            if (inc.g != e.g) return;
            inc.context_tokens = e.tokens_after;
            inc.last_event_ms = now;
        },
        .interrupted => |e| {
            const sl = st.sleeves.getPtr(e.sleeve_id) orelse return error.NotFound;
            const inc = &(sl.inc orelse return);
            if (inc.g != e.g or (inc.activity != .working and inc.activity != .delivering)) return;
            inc.activity = .interrupting;
            inc.stop_requested_ms = now;
            try intents.append(gpa, .{ .interrupt_incarnation = .{ .sleeve_id = sl.id, .g = inc.g } });
        },
        .turn_settled => |e| {
            const sl = st.sleeves.getPtr(e.sleeve_id) orelse return error.NotFound;
            const inc = &(sl.inc orelse return);
            if (inc.g != e.g or inc.classified_exit != null) return;
            inc.classified_exit = if (e.quota_handoff) |handoff| .{
                .cause = handoff.cause,
                .selection = if (handoff.selection) |selection| .{
                    .harness = selection.harness,
                    .model_name = try st.dupe_opt(selection.model_name),
                    .provider = try st.dupe_opt(selection.provider),
                    .reason = try st.dupe_str(selection.reason),
                } else null,
            } else null;
            const exiting_after_interrupt = inc.activity == .interrupting and inc.harness == .antigravity;
            inc.activity = if (exiting_after_interrupt) .interrupting else if (e.quota_handoff != null) .ended else .idle;
            if (sl.lifecycle == .running) inc.stop_requested_ms = null;
            inc.turn_started_ms = null;
            inc.last_event_ms = now;
            inc.context_tokens = e.context_tokens;
            inc.context_window = e.context_window;
            inc.delivered_request_id = null;
            inc.delivered_kind = null;
            inc.turn_is_agent_mail = false;
            sl.budget_used += 1;
            if (sl.role == .envoy) sl.d9 = false;
            fence_sleeve_approvals(st, sl.id, inc.g + 1);
            if (e.declared_status) |ds| {
                sl.last_status = ds;
                if (ds == .done) sl.until_done_left = 0;
            }
            if (sl.role == .child and sl.owed_continuations > 0) {
                sl.owed_continuations -= 1;
            }
            if (sl.parent_id) |pid| {
                if (sl.role == .child) {
                    try intents.append(gpa, .{ .notify_parent = .{
                        .parent_id = pid,
                        .child_id = sl.id,
                        .turn = inc.turn,
                        .status = e.declared_status,
                    } });
                }
            }
            if (e.quota_handoff == null and sl.role == .envoy and e.context_tokens >= model.envoy_context_ceiling) {
                try intents.append(gpa, .{ .compact_incarnation = .{ .sleeve_id = sl.id, .g = inc.g } });
            }
            if (sl.budget_limit) |lim| {
                if (sl.budget_used >= lim and sl.until_done_left > 0) {
                    sl.until_done_left = 0;
                    try intents.append(gpa, .{ .emit_notice = .{
                        .sleeve_id = sl.id,
                        .code = "g11_child_budget",
                        .text = "budget limit reached; stopping auto-continue",
                    } });
                }
            }
            if (exiting_after_interrupt or e.quota_handoff != null) return;
            if (sl.queued.items.len > 0) {
                try maybe_deliver_next(gpa, st, sl, now, intents);
            } else if (sl.until_done_left > 0 and e.turn_end == .end_turn) {
                sl.until_done_left -= 1;
                if (sl.role == .child) sl.owed_continuations += 1;
                const req_id = try std.fmt.allocPrint(a, "ud-{d}-{d}", .{ inc.g, inc.turn + 1 });
                try sl.queued.append(a, .{
                    .request_id = req_id,
                    .from = "system",
                    .kind = .system,
                    .mode = .queue,
                    .text = "Continue until done, or report status blocked/done.",
                    .queued_at_ms = now,
                });
                try maybe_deliver_next(gpa, st, sl, now, intents);
            }
        },
        .stop_requested => |e| {
            _ = e.by;
            const sl = st.sleeves.getPtr(e.sleeve_id) orelse return error.NotFound;
            if (sl.lifecycle == .stopped or sl.lifecycle == .removed) return;
            sl.quota_launch = null;
            sl.lifecycle = .stopping;
            sl.queued.clearRetainingCapacity();
            sl.until_done_left = 0;
            if (sl.inc) |*inc| {
                inc.activity = .interrupting;
                inc.stop_requested_ms = now;
                try intents.append(gpa, .{ .kill_incarnation = .{ .sleeve_id = sl.id, .g = inc.g } });
            } else {
                sl.lifecycle = .stopped;
            }
        },
        .stopped => |e| {
            const sl = st.sleeves.getPtr(e.sleeve_id) orelse return error.NotFound;
            sl.quota_launch = null;
            sl.lifecycle = .stopped;
            sl.inc = null;
            sl.d9 = false;
        },
        .resleeve_requested => |e| {
            const sl = st.sleeves.getPtr(e.sleeve_id) orelse return error.NotFound;
            if (sl.lifecycle == .removed) return error.InvalidState;
            if (st.active_claim_conflict(sl.id, sl.path, sl.layout) != null) return error.WorkspaceConflict;
            sl.quota_launch = null;
            if (sl.resleeve_win.count_in_window(now, model.resleeve_window_ms) >= model.resleeve_limit and e.by != .owner) {
                if (sl.inc) |inc| {
                    try intents.append(gpa, .{ .kill_incarnation = .{ .sleeve_id = sl.id, .g = inc.g } });
                }
                sl.lifecycle = .parked;
                sl.park_reason = .resleeve_rate_exceeded;
                sl.inc = null;
                sl.d9 = false;
                return;
            }
            sl.resleeve_win.push(now);
            sl.lifecycle = .resleeving;
            const current = if (e.inherit_preferences and sl.inc != null and sl.inc.?.harness == e.harness)
                sl.inc
            else
                null;
            const preferred = e.inherit_preferences and sl.target_len > 0 and sl.targets[0] == e.harness;
            const effort = e.effort orelse (if (current) |inc| inc.effort else if (preferred) sl.preferred_effort else null);
            const launch = state.PendingLaunch{
                .harness = e.harness,
                .model_name = (try st.dupe_opt(e.model_name) orelse
                    (if (current) |inc| inc.model_name else if (preferred) sl.preferred_model else null)) orelse "",
                .provider = (try st.dupe_opt(e.provider) orelse
                    (if (current) |inc| inc.provider else if (preferred) sl.preferred_provider else null)) orelse "",
                .effort = if (effort) |value| if (value.len == 0) null else try st.dupe_str(value) else null,
                .cause = e.cause,
            };
            if (sl.inc) |*inc| {
                inc.next_launch = launch;
                inc.stop_requested_ms = now;
                try intents.append(gpa, .{ .kill_incarnation = .{ .sleeve_id = sl.id, .g = inc.g } });
            } else {
                const ng = sl.next_g + 1;
                try intents.append(gpa, .{ .write_brief = .{ .sleeve_id = sl.id, .path = sl.path, .g = ng, .cause = launch.cause } });
                try intents.append(gpa, .{ .launch_incarnation = .{
                    .sleeve_id = sl.id,
                    .g = ng,
                    .harness = launch.harness,
                    .mode = sl.preferred_mode,
                    .model_name = launch.model_name,
                    .provider = launch.provider,
                    .effort = launch.effort,
                } });
            }
        },
        .exit_classified => |e| {
            const sl = st.sleeves.getPtr(e.sleeve_id) orelse return error.NotFound;
            const inc = &(sl.inc orelse return);
            if (inc.g != e.g or sl.lifecycle != .running or inc.classified_exit != null) return;
            inc.classified_exit = .{ .cause = e.cause };
            inc.activity = .ended;
        },
        .exited => |e| {
            const sl = st.sleeves.getPtr(e.sleeve_id) orelse return error.NotFound;
            const inc = sl.inc orelse {
                if (sl.quota_launch) |launch| {
                    if (launch.g == e.g) {
                        sl.quota_launch = null;
                        sl.lifecycle = .parked;
                        sl.park_reason = .provision_failed;
                    }
                }
                return;
            };
            if (inc.g != e.g) return;
            const pending_launch = inc.next_launch;
            sl.inc = null;
            sl.d9 = false;
            fence_sleeve_approvals(st, sl.id, inc.g + 1);

            if (sl.lifecycle == .stopping) {
                sl.lifecycle = .stopped;
                return;
            }
            if (pending_launch) |nl| {
                const ng = sl.next_g + 1;
                try intents.append(gpa, .{ .write_brief = .{ .sleeve_id = sl.id, .path = sl.path, .g = ng, .cause = nl.cause } });
                try intents.append(gpa, .{ .launch_incarnation = .{
                    .sleeve_id = sl.id,
                    .g = ng,
                    .harness = nl.harness,
                    .mode = sl.preferred_mode,
                    .model_name = nl.model_name,
                    .provider = nl.provider,
                    .effort = nl.effort,
                } });
                return;
            }
            const interrupt_exit = e.requested and inc.harness == .antigravity;
            if (!interrupt_exit and (e.requested or e.cause == .clean)) {
                sl.lifecycle = .stopped;
                return;
            }

            const quota_exit = !interrupt_exit and (e.cause == .quota_exhausted or e.cause == .auth_error);
            const selection = if (quota_exit)
                e.quota_selection orelse if (inc.classified_exit) |handoff| handoff.selection else null
            else
                null;
            if (selection) |picked| try intents.append(gpa, .{ .record_quota_selection = .{
                .sleeve_id = sl.id,
                .source = .guardrail,
                .choice = picked,
            } });

            if (e.cause == .oom) {
                const previous = sl.last_oom_ms;
                sl.last_oom_ms = now;
                sl.oom_notice_pending = sl.role != .envoy;
                if (previous) |at| {
                    if (now >= at and now -| at <= model.oom_window_ms) {
                        sl.lifecycle = .parked;
                        sl.park_reason = .oom_rate_exceeded;
                        return;
                    }
                }
            } else if (!interrupt_exit) {
                sl.strike_win.push(now);
                if (sl.strike_win.count_in_window(now, model.strike_window_ms) >= model.strike_limit) {
                    sl.lifecycle = .parked;
                    sl.park_reason = .strike_rate_exceeded;
                    return;
                }
            }
            if (interrupt_exit or e.cause == .crash or e.cause == .oom or e.cause == .killed) {
                if (sl.resume_win.count_in_window(now, model.resume_window_ms) < model.resume_limit) {
                    sl.resume_win.push(now);
                    sl.lifecycle = .recovering;
                    const ng = sl.next_g + 1;
                    try intents.append(gpa, .{ .launch_incarnation = .{
                        .sleeve_id = sl.id,
                        .g = ng,
                        .harness = inc.harness,
                        .mode = inc.mode,
                        .model_name = inc.model_name,
                        .provider = inc.provider,
                        .effort = inc.effort,
                        .resume_id = inc.native_id,
                    } });
                    return;
                }
                sl.lifecycle = .parked;
                sl.park_reason = .resume_rate_exceeded;
                return;
            }
            if (quota_exit) {
                const target: ?model.Harness = if (selection) |s| s.harness else blk: {
                    if (sl.target_len > 1) {
                        for (sl.targets[0..sl.target_len]) |h| {
                            if (h != inc.harness) break :blk h;
                        }
                    }
                    break :blk null;
                };
                if (target) |harness| {
                    sl.resleeve_win.push(now);
                    if (sl.resleeve_win.count_in_window(now, model.resleeve_window_ms) > model.resleeve_limit) {
                        sl.lifecycle = .parked;
                        sl.park_reason = .resleeve_rate_exceeded;
                        return;
                    }
                    sl.lifecycle = .resleeving;
                    const ng = sl.next_g + 1;
                    const launch: intent.LaunchIncarnation = .{
                        .sleeve_id = sl.id,
                        .g = ng,
                        .harness = harness,
                        .mode = if (selection == null) sl.preferred_mode else .headless,
                        .model_name = (if (selection) |picked| try st.dupe_opt(picked.model_name) else sl.preferred_model) orelse "",
                        .provider = (if (selection) |picked| try st.dupe_opt(picked.provider) else sl.preferred_provider) orelse "",
                        .effort = if (selection == null) sl.preferred_effort else null,
                    };
                    if (selection != null) sl.quota_launch = launch;
                    try intents.append(gpa, .{ .write_brief = .{ .sleeve_id = sl.id, .path = sl.path, .g = ng, .cause = .quota } });
                    try intents.append(gpa, .{ .launch_incarnation = launch });
                    return;
                }
                sl.lifecycle = .parked;
                sl.park_reason = .no_fallback_target;
            }
        },
        .parked => |e| {
            const sl = st.sleeves.getPtr(e.sleeve_id) orelse return error.NotFound;
            sl.quota_launch = null;
            sl.lifecycle = .parked;
            sl.park_reason = e.reason;
            sl.inc = null;
            sl.d9 = false;
        },
        .removed => |e| {
            const sl = st.sleeves.getPtr(e.sleeve_id) orelse return error.NotFound;
            if (sl.lifecycle == .removed) return;
            if (sl.parent_id) |pid| {
                if (st.sleeves.getPtr(pid)) |parent| {
                    if (parent.children_active > 0) parent.children_active -= 1;
                }
            }
            const layout = sl.layout;
            const path = sl.path;
            sl.quota_launch = null;
            sl.lifecycle = .removed;
            sl.park_reason = .removed;
            sl.inc = null;
            sl.d9 = false;
            sl.queued.clearRetainingCapacity();
            if (layout == .worktree) {
                try intents.append(gpa, .{ .remove_worktree = .{ .sleeve_id = sl.id, .path = path } });
            }
        },
        .drop_queued => |e| {
            const sl = st.sleeves.getPtr(e.sleeve_id) orelse return error.NotFound;
            sl.queued.clearRetainingCapacity();
        },
        .approval_requested => |e| {
            if (st.approvals.contains(e.approval_id)) return error.DuplicateId;
            const aid = try st.dupe_str(e.approval_id);
            try st.approvals.put(a, aid, .{
                .id = aid,
                .sleeve_id = try st.dupe_str(e.sleeve_id),
                .g = e.g,
                .kind = e.kind,
                .title = try st.dupe_str(e.title),
                .summary = try st.dupe_str(e.summary),
                .is_d9 = e.is_d9,
                .requested_at_ms = now,
                .timeout_at_ms = now + e.timeout_ms,
                .state = .pending,
            });
            if (st.sleeves.getPtr(e.sleeve_id)) |sl| {
                if (sl.inc) |*inc| {
                    if (inc.g == e.g and e.kind == .tool and inc.activity == .working) inc.activity = .blocked;
                }
            }
        },
        .approval_decided => |e| {
            const app = st.approvals.getPtr(e.approval_id) orelse return error.NotFound;
            if (app.state != .pending) return error.InvalidState;
            if (e.verdict != .deny and app.is_d9 and e.by != .owner) {
                return error.InvalidState;
            }
            if (st.sleeves.getPtr(app.sleeve_id)) |sl| {
                if (sl.inc == null or sl.inc.?.g != app.g) {
                    app.state = .fenced;
                    app.decided_at_ms = now;
                    return;
                }
                if (app.kind == .tool and sl.inc.?.activity == .blocked) sl.inc.?.activity = .working;
            }
            app.state = if (e.verdict == .deny) .denied else .approved;
            app.verdict = e.verdict;
            app.decided_by = e.by;
            app.decided_at_ms = now;
            try intents.append(gpa, .{ .respond_approval = .{
                .approval_id = app.id,
                .sleeve_id = app.sleeve_id,
                .g = app.g,
                .verdict = e.verdict,
            } });
        },
        .approval_expired => |e| {
            const app = st.approvals.getPtr(e.approval_id) orelse return error.NotFound;
            if (app.state != .pending) return;
            app.state = .expired;
            app.decided_at_ms = now;
            if (st.sleeves.getPtr(app.sleeve_id)) |sl| {
                if (sl.inc) |inc| {
                    if (inc.g == app.g) {
                        try intents.append(gpa, .{ .respond_approval = .{
                            .approval_id = app.id,
                            .sleeve_id = app.sleeve_id,
                            .g = app.g,
                            .verdict = .deny,
                        } });
                    }
                }
            }
        },
        .node_registered => |e| {
            if (st.nodes.contains(e.node_id)) return error.DuplicateId;
            const nid = try st.dupe_str(e.node_id);
            try st.nodes.put(a, nid, .{
                .id = nid,
                .name = try st.dupe_str(e.name),
                .pub_key = e.pub_key,
                .last_seen_ms = now,
            });
        },
        .node_approved => |e| {
            const nd = st.nodes.getPtr(e.node_id) orelse return error.NotFound;
            nd.approved = true;
            nd.name = try st.dupe_str(e.name);
        },
        .node_connected => |e| {
            const nd = st.nodes.getPtr(e.node_id) orelse return error.NotFound;
            if (nd.removed) return error.InvalidState;
            nd.online = true;
            nd.arch = try st.dupe_str(e.arch);
            nd.version = try st.dupe_str(e.version);
            nd.last_seen_ms = now;
            try dispatch_ready_jobs(gpa, st, now, intents);
        },
        .node_disconnected => |e| {
            _ = e.reason;
            const nd = st.nodes.getPtr(e.node_id) orelse return error.NotFound;
            nd.online = false;
        },
        .node_removed => |e| {
            const nd = st.nodes.getPtr(e.node_id) orelse return error.NotFound;
            nd.removed = true;
            nd.online = false;
        },
        .node_renamed => |e| {
            const nd = st.nodes.getPtr(e.node_id) orelse return error.NotFound;
            nd.name = try st.dupe_str(e.name);
        },
        .node_jobs_updated => |e| {
            const nd = st.nodes.getPtr(e.node_id) orelse return error.NotFound;
            nd.jobs_policy = e.jobs;
            try dispatch_ready_jobs(gpa, st, now, intents);
        },
        .node_presence_updated => |e| {
            const nd = st.nodes.getPtr(e.node_id) orelse return error.NotFound;
            nd.presence = e.presence;
            if (e.presence == .present) {
                var jit = st.jobs.iterator();
                while (jit.next()) |je| {
                    const job = je.value_ptr;
                    if (job.assigned_node) |an| {
                        if (std.mem.eql(u8, an, nd.id) and
                            job.state == .running and
                            !job.even_if_present and
                            job.on_present == .kill)
                        {
                            job.state = .cancelling;
                            job.state_at_ms = now;
                            try intents.append(gpa, .{ .cancel_job = .{ .job_id = job.id, .node_id = nd.id } });
                        }
                    }
                }
            }
            try dispatch_ready_jobs(gpa, st, now, intents);
        },
        .job_queued => |e| {
            if (st.jobs.contains(e.job_id)) return error.DuplicateId;
            const jid = try st.dupe_str(e.job_id);
            try st.jobs.put(a, jid, .{
                .id = jid,
                .target_node = try st.dupe_opt(e.target_node),
                .target_arch = try st.dupe_opt(e.target_arch),
                .requested_by = try st.dupe_str(e.requested_by),
                .on_present = e.on_present,
                .even_if_present = e.even_if_present,
                .maintenance = e.maintenance,
                .state = .queued,
                .queued_at_ms = now,
                .state_at_ms = now,
            });
            try dispatch_ready_jobs(gpa, st, now, intents);
        },
        .job_submitted => |e| {
            const job = st.jobs.getPtr(e.job_id) orelse return error.NotFound;
            if (job.state.is_terminal()) return;
            job.state = .running;
            job.assigned_node = try st.dupe_str(e.node_id);
            job.attempt = e.attempt;
            job.state_at_ms = now;
        },
        .job_submit_uncertain => |e| {
            const job = st.jobs.getPtr(e.job_id) orelse return error.NotFound;
            if (job.state.is_terminal()) return;
            job.state = .submit_uncertain;
            job.assigned_node = try st.dupe_str(e.node_id);
            job.attempt = e.attempt;
            job.state_at_ms = now;
        },
        .job_started => |e| {
            const job = st.jobs.getPtr(e.job_id) orelse return error.NotFound;
            if (job.state.is_terminal()) return;
            job.state = .running;
            job.attempt = e.attempt;
            job.state_at_ms = now;
        },
        .job_completed => |e| {
            const job = st.jobs.getPtr(e.job_id) orelse return error.NotFound;
            if (job.state.is_terminal()) return;
            job.state = if (e.exit_code == 0) .succeeded else .failed;
            job.exit_code = e.exit_code;
            job.state_at_ms = now;
        },
        .job_failed => |e| {
            const job = st.jobs.getPtr(e.job_id) orelse return error.NotFound;
            if (job.state.is_terminal()) return;
            job.state = .failed;
            job.err = try st.dupe_str(e.err);
            job.state_at_ms = now;
            if (job.assigned_node) |nid| {
                if (st.nodes.getPtr(nid)) |nd| {
                    nd.excluded_until_ms = now + model.exclude_for_ms;
                }
            }
        },
        .job_interrupted => |e| {
            const job = st.jobs.getPtr(e.job_id) orelse return error.NotFound;
            if (job.state.is_terminal()) return;
            job.state = .interrupted;
            job.state_at_ms = now;
        },
        .job_lost => |e| {
            const job = st.jobs.getPtr(e.job_id) orelse return error.NotFound;
            if (job.state.is_terminal()) return;
            job.state = .lost;
            job.lost_at_ms = now;
            job.state_at_ms = now;
        },
        .job_cancel_requested => |e| {
            _ = e.by;
            const job = st.jobs.getPtr(e.job_id) orelse return error.NotFound;
            if (job.state.is_terminal()) return;
            if (job.state == .queued) {
                job.state = .cancelled;
                job.state_at_ms = now;
            } else if (job.assigned_node) |nid| {
                job.state = .cancelling;
                job.state_at_ms = now;
                try intents.append(gpa, .{ .cancel_job = .{ .job_id = job.id, .node_id = nid } });
            }
        },
        .job_cancelled => |e| {
            const job = st.jobs.getPtr(e.job_id) orelse return error.NotFound;
            job.state = .cancelled;
            job.state_at_ms = now;
        },
        .job_abandoned => |e| {
            const job = st.jobs.getPtr(e.job_id) orelse return error.NotFound;
            job.state = .abandoned;
            job.err = try st.dupe_str(e.reason);
            job.state_at_ms = now;
        },
        .job_verified => |e| {
            const job = st.jobs.getPtr(e.job_id) orelse return error.NotFound;
            if (job.state.is_terminal()) return;
            if (e.still_running) {
                job.state = .running;
                job.verify_probes = 0;
                job.state_at_ms = now;
            } else if (e.exit_code) |ec| {
                job.state = if (ec == 0) .succeeded else .failed;
                job.exit_code = ec;
                job.state_at_ms = now;
            } else {
                job.state = .lost;
                job.lost_at_ms = now;
                job.state_at_ms = now;
            }
        },
        .rollout_started => |e| {
            if (st.rollouts.contains(e.rollout_id)) return error.DuplicateId;
            const rid = try st.dupe_str(e.rollout_id);
            const ver_s = try st.dupe_str(e.version);
            var rec = state.RolloutRecord{ .id = rid, .version = ver_s };
            for (e.node_ids) |nid| {
                const dn = try st.dupe_str(nid);
                try rec.nodes.append(a, .{ .node_id = dn });
                try intents.append(gpa, .{ .send_node_update = .{ .rollout_id = rid, .node_id = dn, .version = ver_s } });
            }
            try st.rollouts.put(a, rid, rec);
        },
        .rollout_node_result => |e| {
            const ro = st.rollouts.getPtr(e.rollout_id) orelse return error.NotFound;
            for (ro.nodes.items) |*n| {
                if (std.mem.eql(u8, n.node_id, e.node_id)) {
                    n.done = true;
                    n.ok = e.ok;
                    n.detail = try st.dupe_str(e.detail);
                    break;
                }
            }
        },
        .rollout_cancelled => |e| {
            const ro = st.rollouts.getPtr(e.rollout_id) orelse return error.NotFound;
            ro.cancelled = true;
        },
        .tick => try handle_tick(gpa, st, now, intents),
    }
}

fn handle_tick(
    gpa: std.mem.Allocator,
    st: *state.State,
    now: i64,
    intents: *std.ArrayList(intent.Intent),
) ApplyError!void {
    var sit = st.sleeves.iterator();
    while (sit.next()) |e| {
        const sl = e.value_ptr;
        const inc = &(sl.inc orelse continue);
        if (inc.stop_requested_ms) |srm| {
            if (now - srm >= model.stop_grace_ms) {
                inc.stop_requested_ms = null;
                try intents.append(gpa, .{ .kill_incarnation = .{ .sleeve_id = sl.id, .g = inc.g, .recover = sl.lifecycle == .running and inc.activity == .interrupting } });
            }
        }
        if (sl.lifecycle == .running and inc.activity == .booting and now - inc.started_at_ms >= model.silent_ms) {
            try intents.append(gpa, .{ .kill_incarnation = .{ .sleeve_id = sl.id, .g = inc.g, .recover = true } });
            continue;
        }
        if (inc.activity == .working) {
            if (now - inc.last_event_ms >= model.silent_ms) {
                try intents.append(gpa, .{ .emit_notice = .{
                    .sleeve_id = sl.id,
                    .code = "g5_silence_timeout",
                    .text = "incarnation silent beyond timeout",
                } });
                try intents.append(gpa, .{ .interrupt_incarnation = .{ .sleeve_id = sl.id, .g = inc.g } });
                inc.activity = .interrupting;
                inc.stop_requested_ms = now;
            } else if (inc.turn_started_ms) |ts| {
                if (now - ts >= model.turn_wallclock_ms) {
                    try intents.append(gpa, .{ .emit_notice = .{
                        .sleeve_id = sl.id,
                        .code = "g7_wallclock_timeout",
                        .text = "turn exceeded 90m wallclock",
                    } });
                    try intents.append(gpa, .{ .interrupt_incarnation = .{ .sleeve_id = sl.id, .g = inc.g } });
                    inc.activity = .interrupting;
                    inc.stop_requested_ms = now;
                }
            }
        }
    }

    var ait = st.approvals.iterator();
    while (ait.next()) |e| {
        const app = e.value_ptr;
        if (app.state == .pending and now >= app.timeout_at_ms) {
            app.state = .expired;
            app.decided_at_ms = now;
            try intents.append(gpa, .{ .respond_approval = .{
                .approval_id = app.id,
                .sleeve_id = app.sleeve_id,
                .g = app.g,
                .verdict = .deny,
            } });
        }
    }

    var jit = st.jobs.iterator();
    while (jit.next()) |e| {
        const job = e.value_ptr;
        if (job.state == .submitting or job.state == .submit_uncertain or job.state == .running) {
            if (now - job.state_at_ms >= model.job_lease_ms) {
                if (job.verify_probes < model.max_verify and job.assigned_node != null) {
                    job.verify_probes += 1;
                    job.state_at_ms = now;
                    try intents.append(gpa, .{ .verify_job = .{
                        .job_id = job.id,
                        .node_id = job.assigned_node.?,
                        .attempt = job.attempt,
                    } });
                } else {
                    job.state = .lost;
                    job.lost_at_ms = now;
                    job.state_at_ms = now;
                }
            }
        } else if (job.state == .lost) {
            if (job.lost_at_ms) |la| {
                if (now - la >= model.lost_abandon_ms) {
                    job.state = .abandoned;
                    job.state_at_ms = now;
                }
            }
        }
    }

    try dispatch_ready_jobs(gpa, st, now, intents);
}

fn fence_sleeve_approvals(st: *state.State, sleeve_id: []const u8, min_g: u64) void {
    var ait = st.approvals.iterator();
    while (ait.next()) |e| {
        const app = e.value_ptr;
        if (std.mem.eql(u8, app.sleeve_id, sleeve_id) and app.state == .pending and app.g < min_g) {
            app.state = .fenced;
        }
    }
}

fn maybe_deliver_next(
    gpa: std.mem.Allocator,
    st: *state.State,
    sl: *state.SleeveState,
    now: i64,
    intents: *std.ArrayList(intent.Intent),
) ApplyError!void {
    if (sl.queued.items.len == 0) return;
    const inc = &(sl.inc orelse return);
    if (inc.activity == .idle) {
        const first = sl.queued.orderedRemove(0);
        var text = first.text;
        var kind = first.kind;
        // Batch subsequent queued messages of compatible type when multiple are waiting.
        while (sl.queued.items.len > 0 and sl.queued.items[0].mode == .queue) {
            if ((first.kind == .agent_mail) != (sl.queued.items[0].kind == .agent_mail)) break;
            const nxt = sl.queued.orderedRemove(0);
            text = try std.fmt.allocPrint(st.alloc(), "{s}\n\n{s}", .{ text, nxt.text });
            if (nxt.kind == .mail or nxt.kind == .report) kind = nxt.kind;
        }
        inc.activity = .delivering;
        inc.delivered_request_id = first.request_id;
        inc.delivered_kind = kind;
        inc.last_event_ms = now;
        try intents.append(gpa, .{ .deliver_prompt = .{
            .sleeve_id = sl.id,
            .g = inc.g,
            .request_id = first.request_id,
            .from = if (first.kind == .agent_mail) first.from else "owner",
            .text = text,
            .steer = false,
        } });
        return;
    }

    if ((inc.activity == .working or inc.activity == .delivering) and sl.queued.items[0].mode == .steer) {
        const item = sl.queued.orderedRemove(0);
        inc.last_event_ms = now;
        try intents.append(gpa, .{ .deliver_prompt = .{
            .sleeve_id = sl.id,
            .g = inc.g,
            .request_id = item.request_id,
            .text = item.text,
            .steer = true,
        } });
    }
}

fn dispatch_ready_jobs(
    gpa: std.mem.Allocator,
    st: *state.State,
    now: i64,
    intents: *std.ArrayList(intent.Intent),
) ApplyError!void {
    var job_keys: std.ArrayList([]const u8) = .empty;
    defer job_keys.deinit(gpa);
    var jit = st.jobs.keyIterator();
    while (jit.next()) |kp| try job_keys.append(gpa, kp.*);
    std.mem.sort([]const u8, job_keys.items, {}, struct {
        fn less(_: void, lhs: []const u8, rhs: []const u8) bool {
            return std.mem.lessThan(u8, lhs, rhs);
        }
    }.less);

    for (job_keys.items) |jid| {
        const job = st.jobs.getPtr(jid).?;
        if (job.state != .queued) continue;

        var chosen_node: ?*const state.NodeRecord = null;
        var nit = st.nodes.iterator();
        while (nit.next()) |ne| {
            const nd = ne.value_ptr;
            if (!nd.approved or nd.removed or !nd.online) continue;
            if (nd.excluded_until_ms > now) continue;
            if (nd.jobs_policy == .none) continue;
            if (!job.even_if_present and nd.presence == .present) continue;
            if (job.target_node) |tn| {
                if (!std.mem.eql(u8, nd.id, tn) and !std.mem.eql(u8, nd.name, tn)) continue;
            }
            if (job.target_arch) |ta| {
                if (!std.mem.eql(u8, nd.arch, ta)) continue;
            }
            if (chosen_node == null or std.mem.lessThan(u8, nd.id, chosen_node.?.id)) {
                chosen_node = nd;
            }
        }

        if (chosen_node) |nd| {
            job.state = .submitting;
            job.assigned_node = nd.id;
            job.attempt += 1;
            job.state_at_ms = now;
            try intents.append(gpa, .{ .dispatch_job = .{ .job_id = job.id, .node_id = nd.id } });
        }
    }
}
