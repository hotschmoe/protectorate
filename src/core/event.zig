const std = @import("std");
const model = @import("model.zig");

pub const Event = union(enum) {
    sleeve_created: struct {
        sleeve_id: []const u8,
        name: []const u8,
        role: model.SleeveRole,
        path: []const u8,
        workspace_id: []const u8,
        parent_id: ?[]const u8 = null,
        layout: model.Layout,
        branch: ?[]const u8 = null,
        budget_limit: ?u64 = null,
        until_done: u64 = 0,
    },
    provisioned: struct {
        sleeve_id: []const u8,
        branch: ?[]const u8 = null,
    },
    provision_failed: struct {
        sleeve_id: []const u8,
        reason: []const u8,
    },
    start_requested: struct {
        sleeve_id: []const u8,
        targets: [model.max_targets]model.Harness,
        target_len: u8,
        mode: model.Mode,
        model_name: ?[]const u8 = null,
        provider: ?[]const u8 = null,
        effort: ?[]const u8 = null,
        until_done: ?u64 = null,
    },
    launched: struct {
        sleeve_id: []const u8,
        g: model.Gen,
        harness: model.Harness,
        mode: model.Mode,
        model_name: []const u8,
        provider: []const u8,
        effort: ?[]const u8 = null,
    },
    session_bound: struct {
        sleeve_id: []const u8,
        g: model.Gen,
        native_id: []const u8,
    },
    prompt_enqueued: struct {
        sleeve_id: []const u8,
        request_id: []const u8,
        from: []const u8,
        kind: model.MsgKind,
        mode: model.SendMode,
        text: []const u8,
        supersede: bool = false,
        until_done: ?u64 = null,
    },
    prompt_delivered: struct {
        sleeve_id: []const u8,
        g: model.Gen,
        request_id: []const u8,
    },
    prompt_acked: struct {
        sleeve_id: []const u8,
        g: model.Gen,
        request_id: []const u8,
        confirmed: bool,
        detail: ?[]const u8 = null,
    },
    turn_started: struct {
        sleeve_id: []const u8,
        g: model.Gen,
        request_id: ?[]const u8 = null,
        from_agent_mail: bool = false,
    },
    turn_settled: struct {
        sleeve_id: []const u8,
        g: model.Gen,
        turn_end: model.TurnEnd,
        declared_status: ?model.DeclaredStatus = null,
        context_tokens: u64 = 0,
        context_window: u64 = 0,
    },
    tool_activity: struct {
        sleeve_id: []const u8,
        g: model.Gen,
        tool_sig: []const u8,
    },
    compacted: struct {
        sleeve_id: []const u8,
        g: model.Gen,
        tokens_after: u64,
    },
    interrupted: struct {
        sleeve_id: []const u8,
        g: model.Gen,
        by: model.Party,
    },
    exited: struct {
        sleeve_id: []const u8,
        g: model.Gen,
        cause: model.ExitCause,
        requested: bool,
    },
    stop_requested: struct {
        sleeve_id: []const u8,
        by: model.Party,
    },
    stopped: struct {
        sleeve_id: []const u8,
    },
    resleeve_requested: struct {
        sleeve_id: []const u8,
        harness: model.Harness,
        model_name: ?[]const u8 = null,
        provider: ?[]const u8 = null,
        effort: ?[]const u8 = null,
        by: model.Party,
        cause: model.Cause,
    },
    parked: struct {
        sleeve_id: []const u8,
        reason: model.ParkReason,
    },
    removed: struct {
        sleeve_id: []const u8,
    },
    drop_queued: struct {
        sleeve_id: []const u8,
    },
    approval_requested: struct {
        approval_id: []const u8,
        sleeve_id: []const u8,
        g: model.Gen,
        kind: model.ApprovalKind,
        title: []const u8,
        summary: []const u8,
        is_d9: bool = false,
        timeout_ms: model.Millis = model.approval_timeout_ms,
    },
    approval_decided: struct {
        approval_id: []const u8,
        verdict: model.Verdict,
        by: model.Party,
        note: []const u8 = "",
    },
    approval_expired: struct {
        approval_id: []const u8,
    },
    node_registered: struct {
        node_id: []const u8,
        name: []const u8,
        pub_key: [32]u8,
    },
    node_approved: struct {
        node_id: []const u8,
        name: []const u8,
    },
    node_connected: struct {
        node_id: []const u8,
        arch: []const u8,
        version: []const u8,
    },
    node_disconnected: struct {
        node_id: []const u8,
        reason: []const u8,
    },
    node_removed: struct {
        node_id: []const u8,
    },
    node_renamed: struct {
        node_id: []const u8,
        name: []const u8,
    },
    node_jobs_updated: struct {
        node_id: []const u8,
        jobs: model.NodeJobs,
        note: ?[]const u8 = null,
    },
    node_presence_updated: struct {
        node_id: []const u8,
        presence: model.ProbeResult,
    },
    job_queued: struct {
        job_id: []const u8,
        target_node: ?[]const u8 = null,
        target_arch: ?[]const u8 = null,
        requested_by: []const u8,
        on_present: model.OnPresent = .idle,
        even_if_present: bool = false,
        maintenance: bool = false,
    },
    job_submitted: struct {
        job_id: []const u8,
        node_id: []const u8,
        attempt: u32,
    },
    job_submit_uncertain: struct {
        job_id: []const u8,
        node_id: []const u8,
        attempt: u32,
    },
    job_started: struct {
        job_id: []const u8,
        attempt: u32,
    },
    job_completed: struct {
        job_id: []const u8,
        attempt: u32,
        exit_code: i32,
    },
    job_failed: struct {
        job_id: []const u8,
        attempt: u32,
        err: []const u8,
    },
    job_interrupted: struct {
        job_id: []const u8,
        attempt: u32,
    },
    job_lost: struct {
        job_id: []const u8,
        attempt: u32,
    },
    job_cancel_requested: struct {
        job_id: []const u8,
        by: model.Party,
    },
    job_cancelled: struct {
        job_id: []const u8,
    },
    job_abandoned: struct {
        job_id: []const u8,
        reason: []const u8,
    },
    job_verified: struct {
        job_id: []const u8,
        still_running: bool,
        exit_code: ?i32 = null,
    },
    rollout_started: struct {
        rollout_id: []const u8,
        version: []const u8,
        node_ids: []const []const u8,
    },
    rollout_node_result: struct {
        rollout_id: []const u8,
        node_id: []const u8,
        ok: bool,
        detail: []const u8,
    },
    rollout_cancelled: struct {
        rollout_id: []const u8,
    },
    tick: struct {},
};

pub const StampedEvent = struct {
    seq: u64,
    ts_ms: model.Millis,
    event: Event,
};
