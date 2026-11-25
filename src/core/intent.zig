const model = @import("model.zig");

pub const Intent = union(enum) {
    provision_worktree: struct {
        sleeve_id: []const u8,
        path: []const u8,
        layout: model.Layout,
    },
    remove_worktree: struct {
        sleeve_id: []const u8,
        path: []const u8,
    },
    write_brief: struct {
        sleeve_id: []const u8,
        path: []const u8,
        g: model.Gen,
        cause: model.Cause,
    },
    launch_incarnation: struct {
        sleeve_id: []const u8,
        g: model.Gen,
        harness: model.Harness,
        mode: model.Mode,
        model_name: []const u8,
        provider: []const u8,
        effort: ?[]const u8,
        resume_id: ?[]const u8 = null,
    },
    deliver_prompt: struct {
        sleeve_id: []const u8,
        g: model.Gen,
        request_id: []const u8,
        from: []const u8 = "owner",
        text: []const u8,
        steer: bool,
    },
    interrupt_incarnation: struct {
        sleeve_id: []const u8,
        g: model.Gen,
    },
    kill_incarnation: struct {
        sleeve_id: []const u8,
        g: model.Gen,
        recover: bool = false,
    },
    compact_incarnation: struct {
        sleeve_id: []const u8,
        g: model.Gen,
    },
    respond_approval: struct {
        approval_id: []const u8,
        sleeve_id: []const u8,
        g: model.Gen,
        verdict: model.Verdict,
    },
    dispatch_job: struct {
        job_id: []const u8,
        node_id: []const u8,
    },
    verify_job: struct {
        job_id: []const u8,
        node_id: []const u8,
        attempt: u32,
    },
    cancel_job: struct {
        job_id: []const u8,
        node_id: []const u8,
    },
    notify_parent: struct {
        parent_id: []const u8,
        child_id: []const u8,
        turn: u64,
        status: ?model.DeclaredStatus,
    },
    emit_notice: struct {
        sleeve_id: ?[]const u8,
        code: []const u8,
        text: []const u8,
    },
    send_node_update: struct {
        rollout_id: []const u8,
        node_id: []const u8,
        version: []const u8,
    },
};
