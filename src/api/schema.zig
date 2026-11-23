const std = @import("std");
const tiger = @import("../tiger.zig");
const json_util = @import("../json_util.zig");

pub const TimeMs = struct {
    ms: i64 = 0,

    pub fn jsonStringify(self: TimeMs, jw: anytype) !void {
        try jw.write(self.ms);
    }

    pub fn jsonParseFromValue(
        _: std.mem.Allocator,
        source: std.json.Value,
        _: std.json.ParseOptions,
    ) std.json.ParseFromValueError!TimeMs {
        return switch (source) {
            .integer => |n| .{ .ms = n },
            .float => |f| blk: {
                if (!std.math.isFinite(f) or @trunc(f) != f) return error.InvalidNumber;
                if (f < -9223372036854775808.0 or f > 9223372036854774784.0) return error.Overflow;
                break :blk .{ .ms = @intFromFloat(f) };
            },
            else => error.UnexpectedToken,
        };
    }
};

pub const JsonValue = struct {
    val: std.json.Value = .null,

    pub fn jsonStringify(self: JsonValue, jw: anytype) !void {
        try jw.write(self.val);
    }

    pub fn jsonParseFromValue(
        allocator: std.mem.Allocator,
        source: std.json.Value,
        _: std.json.ParseOptions,
    ) std.json.ParseFromValueError!JsonValue {
        return .{ .val = try json_util.clone_value(allocator, source) };
    }
};

fn wire_enum_parse(comptime E: type, source: std.json.Value) std.json.ParseFromValueError!E {
    return switch (source) {
        .string => |s| std.meta.stringToEnum(E, s) orelse .unknown,
        else => error.UnexpectedToken,
    };
}

pub const AckKind = enum {
    confirmed,
    rejected,
    unknown,
    pub fn jsonParseFromValue(a: std.mem.Allocator, s: std.json.Value, o: std.json.ParseOptions) !AckKind {
        _ = a;
        _ = o;
        return wire_enum_parse(AckKind, s);
    }
};

pub const ActivityState = enum {
    booting,
    idle,
    delivering,
    working,
    blocked,
    interrupting,
    compacting,
    ended,
    unknown,
    pub fn jsonParseFromValue(a: std.mem.Allocator, s: std.json.Value, o: std.json.ParseOptions) !ActivityState {
        _ = a;
        _ = o;
        return wire_enum_parse(ActivityState, s);
    }
};

pub const ApprovalKind = enum {
    tool,
    action,
    unknown,
    pub fn jsonParseFromValue(a: std.mem.Allocator, s: std.json.Value, o: std.json.ParseOptions) !ApprovalKind {
        _ = a;
        _ = o;
        return wire_enum_parse(ApprovalKind, s);
    }
};

pub const ApprovalState = enum {
    pending,
    approved,
    denied,
    expired,
    cancelled,
    fenced,
    abandoned,
    unknown,
    pub fn jsonParseFromValue(a: std.mem.Allocator, s: std.json.Value, o: std.json.ParseOptions) !ApprovalState {
        _ = a;
        _ = o;
        return wire_enum_parse(ApprovalState, s);
    }
};

pub const DecidedBy = enum {
    owner,
    envoy,
    rule,
    autonomous,
    unknown,
    pub fn jsonParseFromValue(a: std.mem.Allocator, s: std.json.Value, o: std.json.ParseOptions) !DecidedBy {
        _ = a;
        _ = o;
        return wire_enum_parse(DecidedBy, s);
    }
};

pub const Decision = enum {
    allow_once,
    allow_session,
    deny,
    unknown,
    pub fn jsonParseFromValue(a: std.mem.Allocator, s: std.json.Value, o: std.json.ParseOptions) !Decision {
        _ = a;
        _ = o;
        return wire_enum_parse(Decision, s);
    }
};

pub const Delivery = enum {
    queue,
    steer,
    unknown,
    pub fn jsonParseFromValue(a: std.mem.Allocator, s: std.json.Value, o: std.json.ParseOptions) !Delivery {
        _ = a;
        _ = o;
        return wire_enum_parse(Delivery, s);
    }
};

pub const ErrorClass = enum {
    auth,
    quota,
    rate_limit,
    protocol,
    crash,
    other,
    unknown,
    pub fn jsonParseFromValue(a: std.mem.Allocator, s: std.json.Value, o: std.json.ParseOptions) !ErrorClass {
        _ = a;
        _ = o;
        return wire_enum_parse(ErrorClass, s);
    }
};

pub const ErrorCode = enum {
    unauthenticated,
    not_found,
    invalid,
    forbidden,
    conflict,
    unavailable,
    timeout,
    internal,
    unknown,

    pub fn http_status(self: ErrorCode) u16 {
        return switch (self) {
            .unauthenticated => 401,
            .forbidden => 403,
            .not_found => 404,
            .conflict => 409,
            .invalid => 400,
            .timeout => 504,
            .unavailable => 503,
            .internal, .unknown => 500,
        };
    }

    pub fn jsonParseFromValue(a: std.mem.Allocator, s: std.json.Value, o: std.json.ParseOptions) !ErrorCode {
        _ = a;
        _ = o;
        return wire_enum_parse(ErrorCode, s);
    }
};

pub const EventKind = enum {
    mail,
    sleeve,
    approval,
    job,
    node,
    guardrail,
    hub,
    unknown,
    pub fn jsonParseFromValue(a: std.mem.Allocator, s: std.json.Value, o: std.json.ParseOptions) !EventKind {
        _ = a;
        _ = o;
        return wire_enum_parse(EventKind, s);
    }
};

pub const Harness = enum {
    claude,
    codex,
    grok,
    pi_vendor,
    pi_local,
    antigravity,
    unknown,
    pub fn jsonParseFromValue(a: std.mem.Allocator, s: std.json.Value, o: std.json.ParseOptions) !Harness {
        _ = a;
        _ = o;
        return wire_enum_parse(Harness, s);
    }
};

pub const HarnessUpdateState = enum {
    started,
    deferred,
    skipped,
    unknown,
    pub fn jsonParseFromValue(a: std.mem.Allocator, s: std.json.Value, o: std.json.ParseOptions) !HarnessUpdateState {
        _ = a;
        _ = o;
        return wire_enum_parse(HarnessUpdateState, s);
    }
};

pub const HistoryMode = enum {
    all,
    any,
    phrase,
    unknown,
    pub fn jsonParseFromValue(a: std.mem.Allocator, s: std.json.Value, o: std.json.ParseOptions) !HistoryMode {
        _ = a;
        _ = o;
        return wire_enum_parse(HistoryMode, s);
    }
};

pub const HistoryRole = enum {
    user,
    assistant,
    tool_call,
    tool_error,
    compaction,
    unknown,
    pub fn jsonParseFromValue(a: std.mem.Allocator, s: std.json.Value, o: std.json.ParseOptions) !HistoryRole {
        _ = a;
        _ = o;
        return wire_enum_parse(HistoryRole, s);
    }
};

pub const JobState = enum {
    queued,
    submitting,
    submit_uncertain,
    running,
    collecting,
    succeeded,
    failed,
    interrupted,
    lost,
    cancelling,
    cancelled,
    abandoned,
    unknown,
    pub fn jsonParseFromValue(a: std.mem.Allocator, s: std.json.Value, o: std.json.ParseOptions) !JobState {
        _ = a;
        _ = o;
        return wire_enum_parse(JobState, s);
    }
};

pub const Level = enum {
    info,
    warn,
    alarm,
    unknown,
    pub fn jsonParseFromValue(a: std.mem.Allocator, s: std.json.Value, o: std.json.ParseOptions) !Level {
        _ = a;
        _ = o;
        return wire_enum_parse(Level, s);
    }
};

pub const Lifecycle = enum {
    provisioning,
    launching,
    running,
    recovering,
    resleeving,
    stopping,
    stopped,
    parked,
    removed,
    unknown,
    pub fn jsonParseFromValue(a: std.mem.Allocator, s: std.json.Value, o: std.json.ParseOptions) !Lifecycle {
        _ = a;
        _ = o;
        return wire_enum_parse(Lifecycle, s);
    }
};

pub const Mode = enum {
    headless,
    pane,
    unknown,
    pub fn jsonParseFromValue(a: std.mem.Allocator, s: std.json.Value, o: std.json.ParseOptions) !Mode {
        _ = a;
        _ = o;
        return wire_enum_parse(Mode, s);
    }
};

pub const NodeJobs = enum {
    any,
    owner,
    none,
    unknown,
    pub fn jsonParseFromValue(a: std.mem.Allocator, s: std.json.Value, o: std.json.ParseOptions) !NodeJobs {
        _ = a;
        _ = o;
        return wire_enum_parse(NodeJobs, s);
    }
};

pub const OnPresent = enum {
    idle,
    kill,
    unknown,
    pub fn jsonParseFromValue(a: std.mem.Allocator, s: std.json.Value, o: std.json.ParseOptions) !OnPresent {
        _ = a;
        _ = o;
        return wire_enum_parse(OnPresent, s);
    }
};

pub const PresenceState = enum {
    present,
    away,
    unknown,
    pub fn jsonParseFromValue(a: std.mem.Allocator, s: std.json.Value, o: std.json.ParseOptions) !PresenceState {
        _ = a;
        _ = o;
        return wire_enum_parse(PresenceState, s);
    }
};

pub const QuotaReading = enum {
    unknown,
    ok,
    stale,
    @"error",
    pub fn jsonParseFromValue(a: std.mem.Allocator, s: std.json.Value, o: std.json.ParseOptions) !QuotaReading {
        _ = a;
        _ = o;
        return wire_enum_parse(QuotaReading, s);
    }
};

pub const ReleaseChannel = enum {
    stable,
    dev,
    unknown,
    pub fn jsonParseFromValue(a: std.mem.Allocator, s: std.json.Value, o: std.json.ParseOptions) !ReleaseChannel {
        _ = a;
        _ = o;
        return wire_enum_parse(ReleaseChannel, s);
    }
};

pub const SleeveRole = enum {
    envoy,
    root,
    child,
    unknown,
    pub fn jsonParseFromValue(a: std.mem.Allocator, s: std.json.Value, o: std.json.ParseOptions) !SleeveRole {
        _ = a;
        _ = o;
        return wire_enum_parse(SleeveRole, s);
    }
};

pub const Stop = enum {
    end_turn,
    cancelled,
    @"error",
    max_turns,
    other,
    unknown,
    pub fn jsonParseFromValue(a: std.mem.Allocator, s: std.json.Value, o: std.json.ParseOptions) !Stop {
        _ = a;
        _ = o;
        return wire_enum_parse(Stop, s);
    }
};

pub const Subscription = enum {
    claude,
    codex,
    grok,
    local,
    api,
    unknown,
    pub fn jsonParseFromValue(a: std.mem.Allocator, s: std.json.Value, o: std.json.ParseOptions) !Subscription {
        _ = a;
        _ = o;
        return wire_enum_parse(Subscription, s);
    }
};

pub const ToolKind = enum {
    command,
    read,
    edit,
    search,
    web,
    mcp,
    subagent,
    other,
    unknown,
    pub fn jsonParseFromValue(a: std.mem.Allocator, s: std.json.Value, o: std.json.ParseOptions) !ToolKind {
        _ = a;
        _ = o;
        return wire_enum_parse(ToolKind, s);
    }
};

pub const ToolStatus = enum {
    completed,
    failed,
    declined,
    cancelled,
    unknown,
    pub fn jsonParseFromValue(a: std.mem.Allocator, s: std.json.Value, o: std.json.ParseOptions) !ToolStatus {
        _ = a;
        _ = o;
        return wire_enum_parse(ToolStatus, s);
    }
};

pub const TurnStatusState = enum {
    done,
    continuing,
    blocked,
    unknown,
    pub fn jsonParseFromValue(a: std.mem.Allocator, s: std.json.Value, o: std.json.ParseOptions) !TurnStatusState {
        _ = a;
        _ = o;
        return wire_enum_parse(TurnStatusState, s);
    }
};

pub const WaitUntil = enum {
    idle,
    done,
    blocked,
    report,
    ended,
    parked,
    unknown,
    pub fn jsonParseFromValue(a: std.mem.Allocator, s: std.json.Value, o: std.json.ParseOptions) !WaitUntil {
        _ = a;
        _ = o;
        return wire_enum_parse(WaitUntil, s);
    }
};

pub const WorkspaceView = enum {
    worktree,
    readonly,
    unknown,
    pub fn jsonParseFromValue(a: std.mem.Allocator, s: std.json.Value, o: std.json.ParseOptions) !WorkspaceView {
        _ = a;
        _ = o;
        return wire_enum_parse(WorkspaceView, s);
    }
};

pub const Accepted = struct {
    approval_id: ?[]const u8 = null,
};

pub const Activity = struct {
    state: ActivityState,
    since_ms: TimeMs,
    seen: bool = false,
    request_id: ?[]const u8 = null,
};

pub const AddDeviceRequest = struct {
    name: []const u8,
};

pub const AddNodeRequest = struct {
    registration: []const u8,
};

pub const ApiError = struct {
    code: ErrorCode,
    message: []const u8,
};

pub const Approval = struct {
    id: []const u8,
    sleeve_id: []const u8,
    kind: ApprovalKind,
    title: []const u8,
    summary: []const u8,
    detail: []const u8 = "",
    options: []const Decision = &.{},
    requested_at_ms: TimeMs,
    timeout_at_ms: ?TimeMs = null,
    state: ApprovalState,
    decision: ?Decision = null,
    decided_by: ?DecidedBy = null,
    decided_at_ms: ?TimeMs = null,
    note: []const u8 = "",
};

pub const ApprovalList = struct {
    approvals: []const Approval = &.{},
};

pub const ApproveNodeRequest = struct {
    auth_id: []const u8,
    name: ?[]const u8 = null,
    fingerprint: ?[]const u8 = null,
};

pub const ApprovedNode = struct {
    node_id: []const u8,
    name: []const u8,
    hub_public_key: []const u8 = "",
    hub_fingerprint: []const u8 = "",
    addrs: []const []const u8 = &.{},
};

pub const DecideRequest = struct {
    approval_id: []const u8,
    decision: Decision,
};

pub const Device = struct {
    id: []const u8,
    name: []const u8,
    created_at_ms: TimeMs,
    last_seen_ms: ?TimeMs = null,
};

pub const DeviceList = struct {
    devices: []const Device = &.{},
};

pub const DeviceRef = struct {
    device_id: []const u8,
};

pub const Disk = struct {
    mount: []const u8,
    size_bytes: u64,
    free_bytes: u64,
};

pub const DoctorHarness = struct {
    name: []const u8,
    present: bool,
    path: []const u8 = "",
    version: []const u8 = "",
    install: []const u8 = "",
    login: bool = false,
    eligible: bool = false,
    reasons: []const []const u8 = &.{},
    update: []const []const u8 = &.{},
    updated_at_ms: ?TimeMs = null,
    update_note: []const u8 = "",
};

pub const DoctorReport = struct {
    checked_at_ms: TimeMs,
    os: []const u8,
    arch: []const u8,
    harnesses: []const DoctorHarness = &.{},
    gh_logged_in: bool = false,
    gh_account: []const u8 = "",
    sandbox_ok: bool = false,
    sleeve_devices: []const []const u8 = &.{},
    problems: []const []const u8 = &.{},
};

pub const Empty = struct {};

pub const PendingNode = struct {
    auth_id: []const u8,
    name: []const u8,
    control_key: []const u8,
    seen_at_ms: TimeMs,
    expires_at_ms: TimeMs,
    key_matches: bool,
    fingerprint: []const u8 = "",
    addrs: []const []const u8 = &.{},
};

pub const Enrollment = struct {
    server_url: []const u8,
    control_key: []const u8,
    join_command: []const u8,
    pending: []const PendingNode = &.{},
    hub_fingerprint: []const u8 = "",
    hub_public_key: []const u8 = "",
    addrs: []const []const u8 = &.{},
};

pub const TurnStatus = struct {
    turn: u64,
    state: TurnStatusState,
    detail: ?[]const u8 = null,
};

pub const QueuedTurnReport = struct {
    sleeve_id: []const u8,
    turn: u64,
    status: ?TurnStatus = null,
};

pub const Usage = struct {
    input: u64 = 0,
    output: u64 = 0,
    cache_read: u64 = 0,
    cache_write: u64 = 0,
    cost_usd: ?f64 = null,
    context_tokens: ?u64 = null,
    context_window: ?u64 = null,
};

pub const FileDelta = struct {
    path: []const u8,
    op: ?[]const u8 = null,
    added: ?u64 = null,
    removed: ?u64 = null,
};

pub const EntrySessionInfo = struct {
    harness: Harness,
    mode: Mode,
    native_id: []const u8,
    model: ?[]const u8 = null,
    provider: ?[]const u8 = null,
    effort: ?[]const u8 = null,
};

pub const EntryUserMessage = struct {
    request_id: []const u8,
    from: []const u8,
    text: []const u8,
    steer: bool = false,
    reports: []const QueuedTurnReport = &.{},
};

pub const EntryPromptAck = struct {
    request_id: []const u8,
    ack: AckKind,
    detail: ?[]const u8 = null,
};

pub const EntryAssistantText = struct {
    id: []const u8,
    text: []const u8,
    delta: bool = false,
};

pub const EntryReasoning = struct {
    id: []const u8,
    text: []const u8,
    delta: bool = false,
};

pub const EntryToolCall = struct {
    id: []const u8,
    tool: ToolKind,
    name: []const u8,
    summary: []const u8,
    input: JsonValue = .{},
    truncated: bool = false,
};

pub const EntryToolResult = struct {
    id: []const u8,
    status: ToolStatus,
    output: []const u8,
    truncated: bool = false,
    duration_ms: ?u64 = null,
};

pub const EntryFileChange = struct {
    tool_id: ?[]const u8 = null,
    files: []const FileDelta = &.{},
    diff_ref: ?[]const u8 = null,
};

pub const EntryApprovalRequest = struct {
    approval_id: []const u8,
    title: []const u8,
    summary: []const u8,
};

pub const EntryApprovalDecision = struct {
    approval_id: []const u8,
    decision: Decision,
    by: DecidedBy,
};

pub const EntryTurnStarted = struct {
    request_id: ?[]const u8 = null,
};

pub const EntryTurnSettled = struct {
    stop: Stop,
    native_stop: ?[]const u8 = null,
    usage: Usage = .{},
    duration_ms: ?u64 = null,
    status: ?TurnStatus = null,
};

pub const EntryError = struct {
    kind: ErrorClass,
    text: []const u8,
};

pub const EntryCompaction = struct {
    trigger: ?[]const u8 = null,
    tokens_before: ?u64 = null,
    tokens_after: ?u64 = null,
};

pub const EntryNotice = struct {
    level: Level,
    code: ?[]const u8 = null,
    text: []const u8,
};

pub const EntryExited = struct {
    code: ?i32 = null,
    signal: ?i32 = null,
    requested: bool = false,
    stderr_tail: []const u8 = "",
};

pub const Entry = union(enum) {
    session_info: EntrySessionInfo,
    user_message: EntryUserMessage,
    prompt_ack: EntryPromptAck,
    assistant_text: EntryAssistantText,
    reasoning: EntryReasoning,
    tool_call: EntryToolCall,
    tool_result: EntryToolResult,
    file_change: EntryFileChange,
    approval_request: EntryApprovalRequest,
    approval_decision: EntryApprovalDecision,
    turn_started: EntryTurnStarted,
    turn_settled: EntryTurnSettled,
    @"error": EntryError,
    compaction: EntryCompaction,
    notice: EntryNotice,
    exited: EntryExited,
    unknown: JsonValue,

    pub fn jsonStringify(self: Entry, jw: anytype) !void {
        switch (self) {
            .unknown => |j| try jw.write(j.val),
            inline else => |v, tag| try write_tagged_struct(@tagName(tag), v, jw),
        }
    }

    pub fn jsonParseFromValue(
        allocator: std.mem.Allocator,
        source: std.json.Value,
        options: std.json.ParseOptions,
    ) !Entry {
        if (source != .object) return error.UnexpectedToken;
        const t = json_util.value_str(source, "type") orelse return .{ .unknown = .{ .val = try json_util.clone_value(allocator, source) } };
        const opts: std.json.ParseOptions = .{ .ignore_unknown_fields = true, .allocate = options.allocate };
        inline for (@typeInfo(Entry).@"union".field_names, @typeInfo(Entry).@"union".field_types) |f_name, f_type| {
            if (comptime std.mem.eql(u8, f_name, "unknown")) continue;
            if (std.mem.eql(u8, f_name, t)) {
                return @unionInit(Entry, f_name, try std.json.parseFromValueLeaky(f_type, allocator, source, opts));
            }
        }
        return .{ .unknown = .{ .val = try json_util.clone_value(allocator, source) } };
    }
};

pub const ChatEntry = struct {
    seq: u64,
    g: u64,
    turn: u64,
    at_ms: TimeMs,
    entry: Entry,
};

pub const EnvoyReadRequest = struct {
    from_seq: u64 = 0,
    limit: ?u64 = null,
};

pub const EnvoyReply = struct {
    request_id: []const u8,
    settled: bool,
    text: []const u8 = "",
    seq: u64 = 0,
};

pub const EnvoySendRequest = struct {
    text: []const u8,
    agent: ?[]const u8 = null,
};

pub const EnvoyToken = struct {
    token: []const u8,
};

pub const EnvoyWaitRequest = struct {
    request_id: ?[]const u8 = null,
    timeout_ms: u64 = 30_000,
};

pub const ErrorBody = struct {
    @"error": ApiError,
};

pub const EventList = struct {
    events: []const HubEvent = &.{},
};

pub const GetDoctorRequest = struct {
    refresh: bool = false,
};

pub const GetJobArtifactRequest = struct {
    job_id: []const u8,
    path: []const u8,
    offset: u64 = 0,
    limit: ?u64 = null,
};

pub const GetJobLogRequest = struct {
    job_id: []const u8,
    offset: u64 = 0,
    limit: ?u64 = null,
};

pub const GetTranscriptRequest = struct {
    sleeve_id: []const u8,
    from_seq: ?u64 = null,
    limit: ?u64 = null,
};

pub const GitPushRequest = struct {
    sleeve_id: []const u8,
    remote: ?[]const u8 = null,
    branch: ?[]const u8 = null,
    force: bool = false,
};

pub const GitPushResult = struct {
    sleeve_id: []const u8,
    remote: []const u8,
    branch: []const u8,
    commit: []const u8 = "",
    approval_id: ?[]const u8 = null,
    pushed: bool = false,
};

pub const Gpu = struct {
    name: []const u8,
    driver: []const u8,
    vram_bytes: ?u64 = null,
    util_pct: ?f64 = null,
};

pub const HarnessUpdate = struct {
    program: []const u8,
    state: HarnessUpdateState,
    detail: []const u8 = "",
};

pub const HarnessUpdates = struct {
    programs: []const HarnessUpdate = &.{},
};

pub const HistoryDoc = struct {
    doc_id: u64,
    sleeve_id: []const u8,
    sleeve: []const u8,
    harness: ?Harness = null,
    session: []const u8,
    turn: u64,
    role: HistoryRole,
    time_ms: TimeMs,
    tool: ?[]const u8 = null,
    files: []const []const u8 = &.{},
    path: []const u8,
    offset: u64,
};

pub const HistorySpan = struct {
    start: u64,
    end: u64,
};

pub const HistoryHit = struct {
    doc: HistoryDoc,
    score: f64,
    snippet: []const u8,
    highlights: []const HistorySpan = &.{},
};

pub const HistoryHits = struct {
    hits: []const HistoryHit = &.{},
    indexed_docs: u64 = 0,
};

pub const HistoryText = struct {
    doc: HistoryDoc,
    text: []const u8,
};

pub const HistoryRead = struct {
    docs: []const HistoryText = &.{},
    more: bool = false,
};

pub const HubEvent = struct {
    seq: u64,
    at_ms: TimeMs,
    kind: EventKind,
    source: []const u8,
    target: ?[]const u8 = null,
    text: []const u8,
    read: bool = false,
    level: Level = .info,
};

pub const Toolchain = struct {
    name: []const u8,
    version: []const u8,
};

pub const Inventory = struct {
    hostname: []const u8,
    os: []const u8,
    os_version: []const u8,
    arch: []const u8,
    cpu_model: []const u8,
    cpu_count: u64,
    ram_bytes: u64,
    gpus: []const Gpu = &.{},
    disks: []const Disk = &.{},
    toolchains: []const Toolchain = &.{},
    collected_at_ms: TimeMs,
};

pub const Sample = struct {
    sampled_at_ms: TimeMs,
    cpu_pct: ?f64 = null,
    load1: ?f64 = null,
    mem_avail_bytes: ?u64 = null,
    swap_used_bytes: ?u64 = null,
};

pub const HubInfo = struct {
    name: []const u8,
    version: []const u8,
    started_at_ms: TimeMs,
    inventory: ?Inventory = null,
    sample: ?Sample = null,
    history_capped: []const []const u8 = &.{},
    restore_error: ?[]const u8 = null,
    clients: []const []const u8 = &.{},
    job_artifact_max_bytes: u64 = 0,
};

pub const Workspace = struct {
    id: []const u8,
    name: []const u8,
    path: []const u8,
};

pub const Incarnation = struct {
    g: u64,
    memory_max_bytes: ?u64 = null,
    mode: Mode,
    harness: Harness,
    subscription: Subscription,
    provider: []const u8 = "",
    model: []const u8 = "",
    effort: ?[]const u8 = null,
    native_id: ?[]const u8 = null,
    activity: Activity,
    started_at_ms: TimeMs,
    turn: u64 = 0,
    context_tokens: u64 = 0,
    context_window: u64 = 0,
    cache_ratio: ?f64 = null,
};

pub const Sleeve = struct {
    id: []const u8,
    name: []const u8,
    workspace_id: []const u8,
    parent_id: ?[]const u8 = null,
    role: SleeveRole,
    lifecycle: Lifecycle,
    park_reason: ?[]const u8 = null,
    path: []const u8,
    resleeve_to: []const Harness = &.{},
    branch: ?[]const u8 = null,
    view: ?WorkspaceView = null,
    incarnation: ?Incarnation = null,
    queued: u64 = 0,
    oldest_queued_at_ms: ?TimeMs = null,
    budget_used: ?u64 = null,
    budget_limit: ?u64 = null,
    d9: bool = false,
    last_status: ?TurnStatus = null,
    warning: ?[]const u8 = null,
};

pub const Presence = struct {
    state: PresenceState,
    idle_secs: ?u64 = null,
    locked: ?bool = null,
    source: []const u8 = "",
    sampled_at_ms: ?TimeMs = null,
};

pub const Node = struct {
    id: []const u8,
    name: []const u8,
    online: bool,
    last_seen_ms: TimeMs,
    node_version: []const u8,
    pending_update: ?[]const u8 = null,
    @"error": ?[]const u8 = null,
    inventory: ?Inventory = null,
    sample: ?Sample = null,
    presence: Presence,
    jobs: NodeJobs = .any,
    jobs_note: ?[]const u8 = null,
};

pub const JobTargetNode = struct {
    node_id: []const u8,
};

pub const JobTargetArch = struct {
    arch: []const u8,
};

pub const JobTarget = union(enum) {
    node: JobTargetNode,
    arch: JobTargetArch,
    unknown: JsonValue,

    pub fn jsonStringify(self: JobTarget, jw: anytype) !void {
        switch (self) {
            .unknown => |j| try jw.write(j.val),
            inline else => |v, tag| try write_tagged_struct(@tagName(tag), v, jw),
        }
    }

    pub fn jsonParseFromValue(
        allocator: std.mem.Allocator,
        source: std.json.Value,
        options: std.json.ParseOptions,
    ) !JobTarget {
        if (source != .object) return error.UnexpectedToken;
        const t = json_util.value_str(source, "type") orelse return .{ .unknown = .{ .val = try json_util.clone_value(allocator, source) } };
        const opts: std.json.ParseOptions = .{ .ignore_unknown_fields = true, .allocate = options.allocate };
        if (std.mem.eql(u8, t, "node")) {
            return .{ .node = try std.json.parseFromValueLeaky(JobTargetNode, allocator, source, opts) };
        } else if (std.mem.eql(u8, t, "arch")) {
            return .{ .arch = try std.json.parseFromValueLeaky(JobTargetArch, allocator, source, opts) };
        }
        return .{ .unknown = .{ .val = try json_util.clone_value(allocator, source) } };
    }
};

pub const JobArtifact = struct {
    path: []const u8,
    size_bytes: ?u64 = null,
    sha256: ?[]const u8 = null,
};

pub const Job = struct {
    id: []const u8,
    node_id: ?[]const u8 = null,
    target: JobTarget,
    argv: []const []const u8 = &.{},
    state: JobState,
    requested_by: []const u8,
    attempt: u64 = 0,
    even_if_present: bool = false,
    on_present: OnPresent = .idle,
    approval_id: ?[]const u8 = null,
    queued_at_ms: TimeMs,
    started_at_ms: ?TimeMs = null,
    ended_at_ms: ?TimeMs = null,
    exit_code: ?i32 = null,
    @"error": ?[]const u8 = null,
    held_reason: ?[]const u8 = null,
    log_bytes: u64 = 0,
    cwd: ?[]const u8 = null,
    artifacts: []const JobArtifact = &.{},
};

pub const QuotaCredits = struct {
    allowed: ?bool = null,
    limit_reached: ?bool = null,
    has_credits: ?bool = null,
    balance: ?f64 = null,
    overage_limit_reached: ?bool = null,
    reset_available_count: ?u64 = null,
    per_hour: ?f64 = null,
};

pub const QuotaWindow = struct {
    label: []const u8,
    used_pct: ?f64 = null,
    resets_at_ms: ?TimeMs = null,
    length_ms: ?u64 = null,
};

pub const Quota = struct {
    subscription: Subscription,
    plan: []const u8 = "",
    reading: QuotaReading = .unknown,
    fetched_at_ms: TimeMs,
    windows: []const QuotaWindow = &.{},
    credits: ?QuotaCredits = null,
    note: []const u8 = "",
};

pub const HubState = struct {
    seq: u64,
    hub: HubInfo,
    workspaces: []const Workspace = &.{},
    sleeves: []const Sleeve = &.{},
    approvals: []const Approval = &.{},
    nodes: []const Node = &.{},
    jobs: []const Job = &.{},
    quotas: []const Quota = &.{},
    events: []const HubEvent = &.{},
};

pub const HubUpdate = struct {
    current: ?[]const u8 = null,
    target: []const u8,
    available: bool,
    restarting: bool = false,
    note: []const u8 = "",
};

pub const QueuedMessage = struct {
    sender: []const u8,
    queued_at_ms: TimeMs,
    text: []const u8,
    report: ?QueuedTurnReport = null,
};

pub const Inbox = struct {
    messages: []const QueuedMessage = &.{},
};

pub const InboxRequest = struct {
    sleeve_id: ?[]const u8 = null,
};

pub const JobArtifactData = struct {
    job_id: []const u8,
    path: []const u8,
    offset: u64,
    next_offset: u64,
    size_bytes: u64,
    sha256: []const u8,
    hex: []const u8,
    eof: bool,
};

pub const JobList = struct {
    jobs: []const Job = &.{},
    scoped_to_caller: bool = false,
};

pub const JobLog = struct {
    job_id: []const u8,
    offset: u64,
    next_offset: u64,
    text: []const u8,
    eof: bool,
};

pub const JobRef = struct {
    job_id: []const u8,
};

pub const ListApprovalsRequest = struct {
    pending_only: bool = false,
};

pub const ListEventsRequest = struct {
    from_seq: ?u64 = null,
    limit: ?u64 = null,
};

pub const ListJobsRequest = struct {
    node_id: ?[]const u8 = null,
    limit: ?u64 = null,
};

pub const MarkEventsReadRequest = struct {
    up_to_seq: u64,
};

pub const NodeList = struct {
    nodes: []const Node = &.{},
};

pub const NodeRef = struct {
    node_id: []const u8,
};

pub const NodeUpdate = struct {
    node_id: []const u8,
    name: []const u8,
    from: []const u8,
    to: []const u8,
    ok: bool,
    detail: []const u8 = "",
};

pub const NodeUpdates = struct {
    version: []const u8,
    nodes: []const NodeUpdate = &.{},
    rollout_id: ?[]const u8 = null,
};

pub const PairDeviceRequest = struct {
    code: []const u8,
};

pub const PairedDevice = struct {
    device: Device,
    token: []const u8,
    expires_at_ms: ?TimeMs = null,
};

pub const PairingCode = struct {
    name: []const u8,
    code: []const u8,
    expires_at_ms: TimeMs,
};

pub const QuotaList = struct {
    quotas: []const Quota = &.{},
};

pub const ReadHistoryRequest = struct {
    doc_id: ?u64 = null,
    session: ?[]const u8 = null,
    from_turn: u64 = 0,
    to_turn: ?u64 = null,
    after_doc: ?u64 = null,
};

pub const RemoveNodeRequest = struct {
    node_id: []const u8,
    force: bool = false,
};

pub const RemovedNode = struct {
    node_id: []const u8,
    name: []const u8,
    failed_jobs: []const []const u8 = &.{},
    orphaned_jobs: []const []const u8 = &.{},
    record_only: bool = false,
};

pub const RenameNodeRequest = struct {
    node_id: []const u8,
    name: []const u8,
};

pub const RenamedNode = struct {
    node_id: []const u8,
    from: []const u8,
    to: []const u8,
};

pub const ResleeveRequest = struct {
    sleeve_id: []const u8,
    target: Harness,
    model: ?[]const u8 = null,
    provider: ?[]const u8 = null,
    effort: ?[]const u8 = null,
};

pub const RunJobRequest = struct {
    target: JobTarget,
    argv: []const []const u8,
    cwd: ?[]const u8 = null,
    artifacts: []const []const u8 = &.{},
    even_if_present: bool = false,
    on_present: OnPresent = .idle,
    maintenance: bool = false,
    timeout_secs: ?u64 = null,
};

pub const SearchHistoryRequest = struct {
    query: []const u8,
    mode: HistoryMode = .all,
    sleeves: []const []const u8 = &.{},
    harness: ?Harness = null,
    session: ?[]const u8 = null,
    since_ms: ?TimeMs = null,
    until_ms: ?TimeMs = null,
    roles: []const HistoryRole = &.{},
    limit: ?u64 = null,
};

pub const SendMessageRequest = struct {
    sleeve_id: []const u8,
    text: []const u8,
    delivery: Delivery = .queue,
    supersede: bool = false,
    until_done: ?u64 = null,
};

pub const Sent = struct {
    request_id: []const u8,
    queued: bool,
    queue_length: u64,
};

pub const SetNodeJobsRequest = struct {
    node_id: []const u8,
    jobs: NodeJobs,
    note: ?[]const u8 = null,
};

pub const SleeveList = struct {
    sleeves: []const Sleeve = &.{},
};

pub const SleeveRef = struct {
    sleeve_id: []const u8,
};

pub const SpawnSleeveRequest = struct {
    name: []const u8,
    harness: Harness,
    path: ?[]const u8 = null,
    new_path: ?[]const u8 = null,
    parent_id: ?[]const u8 = null,
    view: ?WorkspaceView = null,
    mode: Mode = .headless,
    model: ?[]const u8 = null,
    provider: ?[]const u8 = null,
    effort: ?[]const u8 = null,
    resleeve_to: []const Harness = &.{},
    message: ?[]const u8 = null,
    budget: ?u64 = null,
    until_done: ?u64 = null,
};

pub const StartSleeveRequest = struct {
    sleeve_id: []const u8,
    target: ?Harness = null,
    model: ?[]const u8 = null,
    provider: ?[]const u8 = null,
    effort: ?[]const u8 = null,
    until_done: ?u64 = null,
};

pub const Transcript = struct {
    sleeve_id: []const u8,
    entries: []const ChatEntry = &.{},
    more: bool = false,
};

pub const UpdateHarnessesRequest = struct {
    programs: []const []const u8 = &.{},
};

pub const UpdateHubRequest = struct {
    check: bool = false,
    channel: ?ReleaseChannel = null,
    version: ?[]const u8 = null,
    rollback: bool = false,
};

pub const UpdateNodesRequest = struct {
    cancel: ?[]const u8 = null,
    nodes: []const []const u8 = &.{},
    all: bool = false,
    version: ?[]const u8 = null,
    channel: ?ReleaseChannel = null,
    probation_secs: ?u64 = null,
    no_wait: bool = false,
    rollout_id: ?[]const u8 = null,
};

pub const WaitSleeveResult = struct {
    reached: bool,
    sleeve: Sleeve,
    seq: ?u64 = null,
};

pub const WaitResult = struct {
    reached: bool,
    sleeves: []const WaitSleeveResult = &.{},
};

pub const WaitRequest = struct {
    sleeve_ids: []const []const u8 = &.{},
    until: WaitUntil,
    any: bool = false,
    timeout_ms: u64 = 30_000,
    after_seq: ?u64 = null,
};

pub const HubStreamMessageSnapshot = struct {
    seq: u64,
    state: HubState,
};

pub const HubStreamMessageLive = struct {
    seq: u64,
};

pub const HubStreamMessageWorkspace = struct {
    seq: u64,
    workspace: Workspace,
};

pub const HubStreamMessageSleeve = struct {
    seq: u64,
    sleeve: Sleeve,
};

pub const HubStreamMessageSleeveRemoved = struct {
    seq: u64,
    id: []const u8,
};

pub const HubStreamMessageEntry = struct {
    seq: u64,
    sleeve_id: []const u8,
    entry: ChatEntry,
};

pub const HubStreamMessageApproval = struct {
    seq: u64,
    approval: Approval,
};

pub const HubStreamMessageHub = struct {
    seq: u64,
    hub: HubInfo,
};

pub const HubStreamMessageNode = struct {
    seq: u64,
    node: Node,
};

pub const HubStreamMessageNodeRemoved = struct {
    seq: u64,
    id: []const u8,
};

pub const HubStreamMessageJob = struct {
    seq: u64,
    job: Job,
};

pub const HubStreamMessageQuota = struct {
    seq: u64,
    quota: Quota,
};

pub const HubStreamMessageEvent = struct {
    seq: u64,
    event: HubEvent,
};

pub const HubStreamMessage = union(enum) {
    snapshot: HubStreamMessageSnapshot,
    live: HubStreamMessageLive,
    workspace: HubStreamMessageWorkspace,
    sleeve: HubStreamMessageSleeve,
    sleeve_removed: HubStreamMessageSleeveRemoved,
    entry: HubStreamMessageEntry,
    approval: HubStreamMessageApproval,
    hub: HubStreamMessageHub,
    node: HubStreamMessageNode,
    node_removed: HubStreamMessageNodeRemoved,
    job: HubStreamMessageJob,
    quota: HubStreamMessageQuota,
    event: HubStreamMessageEvent,
    unknown: JsonValue,

    pub fn jsonStringify(self: HubStreamMessage, jw: anytype) !void {
        switch (self) {
            .unknown => |j| try jw.write(j.val),
            inline else => |v, tag| try write_tagged_struct(@tagName(tag), v, jw),
        }
    }

    pub fn jsonParseFromValue(
        allocator: std.mem.Allocator,
        source: std.json.Value,
        options: std.json.ParseOptions,
    ) !HubStreamMessage {
        if (source != .object) return error.UnexpectedToken;
        const t = json_util.value_str(source, "type") orelse return .{ .unknown = .{ .val = try json_util.clone_value(allocator, source) } };
        const opts: std.json.ParseOptions = .{ .ignore_unknown_fields = true, .allocate = options.allocate };
        inline for (@typeInfo(HubStreamMessage).@"union".field_names, @typeInfo(HubStreamMessage).@"union".field_types) |f_name, f_type| {
            if (comptime std.mem.eql(u8, f_name, "unknown")) continue;
            if (std.mem.eql(u8, f_name, t)) {
                return @unionInit(HubStreamMessage, f_name, try std.json.parseFromValueLeaky(f_type, allocator, source, opts));
            }
        }
        return .{ .unknown = .{ .val = try json_util.clone_value(allocator, source) } };
    }
};

fn write_tagged_struct(tag_name: []const u8, v: anytype, jw: anytype) !void {
    try jw.beginObject();
    try jw.objectField("type");
    try jw.write(tag_name);
    const info = @typeInfo(@TypeOf(v)).@"struct";
    inline for (info.field_names, info.field_types) |f_name, f_type| {
        if (f_type == void) continue;
        try jw.objectField(f_name);
        try jw.write(@field(v, f_name));
    }
    try jw.endObject();
}

pub const Scope = enum {
    owner,
    sleeve,
    envoy_only,
    public,

    pub fn to_wire(self: Scope) []const u8 {
        return @tagName(self);
    }
};

pub const MethodSpec = struct {
    name: []const u8,
    dart_name: []const u8,
    scope: Scope,
    Req: type,
    Resp: type,
    default_empty: bool = false,
    doc: []const u8 = "",
};

pub const methods = [_]MethodSpec{
    .{ .name = "GetState", .dart_name = "getState", .scope = .owner, .Req = Empty, .Resp = HubState, .default_empty = true, .doc = "The full snapshot." },
    .{ .name = "ListSleeves", .dart_name = "listSleeves", .scope = .sleeve, .Req = Empty, .Resp = SleeveList, .default_empty = true },
    .{ .name = "GetInbox", .dart_name = "getInbox", .scope = .sleeve, .Req = InboxRequest, .Resp = Inbox },
    .{ .name = "DropQueued", .dart_name = "dropQueued", .scope = .sleeve, .Req = SleeveRef, .Resp = Empty },
    .{ .name = "GetTranscript", .dart_name = "getTranscript", .scope = .sleeve, .Req = GetTranscriptRequest, .Resp = Transcript },
    .{ .name = "SpawnSleeve", .dart_name = "spawnSleeve", .scope = .sleeve, .Req = SpawnSleeveRequest, .Resp = Sleeve },
    .{ .name = "SendMessage", .dart_name = "sendMessage", .scope = .sleeve, .Req = SendMessageRequest, .Resp = Sent },
    .{ .name = "Interrupt", .dart_name = "interrupt", .scope = .sleeve, .Req = SleeveRef, .Resp = Empty },
    .{ .name = "StopSleeve", .dart_name = "stopSleeve", .scope = .sleeve, .Req = SleeveRef, .Resp = Accepted, .doc = "Stops the sleeve and, for a root, every child in the same step." },
    .{ .name = "RemoveSleeve", .dart_name = "removeSleeve", .scope = .sleeve, .Req = SleeveRef, .Resp = Accepted, .doc = "Needs the owner's approval when Envoy asks during a turn an agent message started (D9)." },
    .{ .name = "StartSleeve", .dart_name = "startSleeve", .scope = .sleeve, .Req = StartSleeveRequest, .Resp = Accepted },
    .{ .name = "Resleeve", .dart_name = "resleeve", .scope = .sleeve, .Req = ResleeveRequest, .Resp = Accepted },
    .{ .name = "TakeoverTerminal", .dart_name = "takeoverTerminal", .scope = .owner, .Req = SleeveRef, .Resp = Empty, .doc = "Not built yet: answers unavailable." },
    .{ .name = "GitPush", .dart_name = "gitPush", .scope = .sleeve, .Req = GitPushRequest, .Resp = GitPushResult, .doc = "Push a worktree branch, guarded by owner approval." },
    .{ .name = "ListApprovals", .dart_name = "listApprovals", .scope = .owner, .Req = ListApprovalsRequest, .Resp = ApprovalList },
    .{ .name = "Decide", .dart_name = "decide", .scope = .owner, .Req = DecideRequest, .Resp = Approval },
    .{ .name = "ListNodes", .dart_name = "listNodes", .scope = .sleeve, .Req = Empty, .Resp = NodeList, .default_empty = true },
    .{ .name = "GetNode", .dart_name = "getNode", .scope = .sleeve, .Req = NodeRef, .Resp = Node },
    .{ .name = "SetNodeJobs", .dart_name = "setNodeJobs", .scope = .owner, .Req = SetNodeJobsRequest, .Resp = Node },
    .{ .name = "GetEnrollment", .dart_name = "getEnrollment", .scope = .owner, .Req = Empty, .Resp = Enrollment, .default_empty = true },
    .{ .name = "AddNode", .dart_name = "addNode", .scope = .owner, .Req = AddNodeRequest, .Resp = PendingNode },
    .{ .name = "ApproveNode", .dart_name = "approveNode", .scope = .owner, .Req = ApproveNodeRequest, .Resp = ApprovedNode },
    .{ .name = "RemoveNode", .dart_name = "removeNode", .scope = .owner, .Req = RemoveNodeRequest, .Resp = RemovedNode },
    .{ .name = "RenameNode", .dart_name = "renameNode", .scope = .owner, .Req = RenameNodeRequest, .Resp = RenamedNode },
    .{ .name = "RunJob", .dart_name = "runJob", .scope = .sleeve, .Req = RunJobRequest, .Resp = Job },
    .{ .name = "ListJobs", .dart_name = "listJobs", .scope = .sleeve, .Req = ListJobsRequest, .Resp = JobList },
    .{ .name = "GetJob", .dart_name = "getJob", .scope = .sleeve, .Req = JobRef, .Resp = Job },
    .{ .name = "GetJobLog", .dart_name = "getJobLog", .scope = .sleeve, .Req = GetJobLogRequest, .Resp = JobLog },
    .{ .name = "GetJobArtifact", .dart_name = "getJobArtifact", .scope = .sleeve, .Req = GetJobArtifactRequest, .Resp = JobArtifactData },
    .{ .name = "CancelJob", .dart_name = "cancelJob", .scope = .sleeve, .Req = JobRef, .Resp = Job },
    .{ .name = "ListQuotas", .dart_name = "listQuotas", .scope = .sleeve, .Req = Empty, .Resp = QuotaList, .default_empty = true },
    .{ .name = "ListEvents", .dart_name = "listEvents", .scope = .sleeve, .Req = ListEventsRequest, .Resp = EventList },
    .{ .name = "MarkEventsRead", .dart_name = "markEventsRead", .scope = .owner, .Req = MarkEventsReadRequest, .Resp = Empty },
    .{ .name = "EnvoySend", .dart_name = "envoySend", .scope = .envoy_only, .Req = EnvoySendRequest, .Resp = Sent },
    .{ .name = "EnvoyWait", .dart_name = "envoyWait", .scope = .envoy_only, .Req = EnvoyWaitRequest, .Resp = EnvoyReply },
    .{ .name = "EnvoyRead", .dart_name = "envoyRead", .scope = .envoy_only, .Req = EnvoyReadRequest, .Resp = Transcript },
    .{ .name = "GetEnvoyToken", .dart_name = "getEnvoyToken", .scope = .owner, .Req = Empty, .Resp = EnvoyToken, .default_empty = true },
    .{ .name = "Wait", .dart_name = "wait", .scope = .sleeve, .Req = WaitRequest, .Resp = WaitResult },
    .{ .name = "SearchHistory", .dart_name = "searchHistory", .scope = .sleeve, .Req = SearchHistoryRequest, .Resp = HistoryHits },
    .{ .name = "ReadHistory", .dart_name = "readHistory", .scope = .sleeve, .Req = ReadHistoryRequest, .Resp = HistoryRead },
    .{ .name = "GetHubInfo", .dart_name = "getHubInfo", .scope = .sleeve, .Req = Empty, .Resp = HubInfo, .default_empty = true },
    .{ .name = "GetDoctor", .dart_name = "getDoctor", .scope = .sleeve, .Req = GetDoctorRequest, .Resp = DoctorReport },
    .{ .name = "UpdateHarnesses", .dart_name = "updateHarnesses", .scope = .owner, .Req = UpdateHarnessesRequest, .Resp = HarnessUpdates },
    .{ .name = "UpdateHub", .dart_name = "updateHub", .scope = .owner, .Req = UpdateHubRequest, .Resp = HubUpdate },
    .{ .name = "UpdateNodes", .dart_name = "updateNodes", .scope = .owner, .Req = UpdateNodesRequest, .Resp = NodeUpdates },
    .{ .name = "ListDevices", .dart_name = "listDevices", .scope = .owner, .Req = Empty, .Resp = DeviceList, .default_empty = true },
    .{ .name = "AddDevice", .dart_name = "addDevice", .scope = .owner, .Req = AddDeviceRequest, .Resp = PairingCode },
    .{ .name = "PairDevice", .dart_name = "pairDevice", .scope = .public, .Req = PairDeviceRequest, .Resp = PairedDevice },
    .{ .name = "RevokeDevice", .dart_name = "revokeDevice", .scope = .owner, .Req = DeviceRef, .Resp = Empty },
};

pub const wire_enums = .{
    AckKind,
    ActivityState,
    ApprovalKind,
    ApprovalState,
    DecidedBy,
    Decision,
    Delivery,
    ErrorClass,
    ErrorCode,
    EventKind,
    Harness,
    HarnessUpdateState,
    HistoryMode,
    HistoryRole,
    JobState,
    Level,
    Lifecycle,
    Mode,
    NodeJobs,
    OnPresent,
    PresenceState,
    QuotaReading,
    ReleaseChannel,
    SleeveRole,
    Stop,
    Subscription,
    ToolKind,
    ToolStatus,
    TurnStatusState,
    WaitUntil,
    WorkspaceView,
};

pub const wire_unions = .{
    Entry,
    HubStreamMessage,
    JobTarget,
};

pub const wire_records = .{
    Accepted,
    Activity,
    AddDeviceRequest,
    AddNodeRequest,
    ApiError,
    Approval,
    ApprovalList,
    ApproveNodeRequest,
    ApprovedNode,
    ChatEntry,
    DecideRequest,
    Device,
    DeviceList,
    DeviceRef,
    Disk,
    DoctorHarness,
    DoctorReport,
    Empty,
    Enrollment,
    EntryApprovalDecision,
    EntryApprovalRequest,
    EntryAssistantText,
    EntryCompaction,
    EntryError,
    EntryExited,
    EntryFileChange,
    EntryNotice,
    EntryPromptAck,
    EntryReasoning,
    EntrySessionInfo,
    EntryToolCall,
    EntryToolResult,
    EntryTurnSettled,
    EntryTurnStarted,
    EntryUserMessage,
    EnvoyReadRequest,
    EnvoyReply,
    EnvoySendRequest,
    EnvoyToken,
    EnvoyWaitRequest,
    ErrorBody,
    EventList,
    FileDelta,
    GetDoctorRequest,
    GetJobArtifactRequest,
    GetJobLogRequest,
    GetTranscriptRequest,
    GitPushRequest,
    GitPushResult,
    Gpu,
    HarnessUpdate,
    HarnessUpdates,
    HistoryDoc,
    HistoryHit,
    HistoryHits,
    HistoryRead,
    HistorySpan,
    HistoryText,
    HubEvent,
    HubInfo,
    HubState,
    HubStreamMessageApproval,
    HubStreamMessageEntry,
    HubStreamMessageEvent,
    HubStreamMessageHub,
    HubStreamMessageJob,
    HubStreamMessageLive,
    HubStreamMessageNode,
    HubStreamMessageNodeRemoved,
    HubStreamMessageQuota,
    HubStreamMessageSleeve,
    HubStreamMessageSleeveRemoved,
    HubStreamMessageSnapshot,
    HubStreamMessageWorkspace,
    HubUpdate,
    Inbox,
    InboxRequest,
    Incarnation,
    Inventory,
    Job,
    JobArtifact,
    JobArtifactData,
    JobList,
    JobLog,
    JobRef,
    JobTargetArch,
    JobTargetNode,
    ListApprovalsRequest,
    ListEventsRequest,
    ListJobsRequest,
    MarkEventsReadRequest,
    Node,
    NodeList,
    NodeRef,
    NodeUpdate,
    NodeUpdates,
    PairDeviceRequest,
    PairedDevice,
    PairingCode,
    PendingNode,
    Presence,
    QueuedMessage,
    QueuedTurnReport,
    Quota,
    QuotaCredits,
    QuotaList,
    QuotaWindow,
    ReadHistoryRequest,
    RemoveNodeRequest,
    RemovedNode,
    RenameNodeRequest,
    RenamedNode,
    ResleeveRequest,
    RunJobRequest,
    Sample,
    SearchHistoryRequest,
    SendMessageRequest,
    Sent,
    SetNodeJobsRequest,
    Sleeve,
    SleeveList,
    SleeveRef,
    SpawnSleeveRequest,
    StartSleeveRequest,
    Toolchain,
    Transcript,
    TurnStatus,
    UpdateHarnessesRequest,
    UpdateHubRequest,
    UpdateNodesRequest,
    Usage,
    WaitRequest,
    WaitResult,
    WaitSleeveResult,
    Workspace,
};

pub fn encode_alloc(gpa: std.mem.Allocator, value: anytype) error{OutOfMemory}![]u8 {
    return std.json.Stringify.valueAlloc(gpa, value, .{});
}

pub fn decode_arena(comptime T: type, arena: std.mem.Allocator, bytes: []const u8) !T {
    const val = try std.json.parseFromSliceLeaky(std.json.Value, arena, bytes, .{});
    return std.json.parseFromValueLeaky(T, arena, val, .{
        .ignore_unknown_fields = true,
        .allocate = .alloc_always,
    });
}
