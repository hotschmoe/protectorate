const std = @import("std");
const tiger = @import("../tiger.zig");
const json_util = @import("../json_util.zig");

pub const protocol_version: u32 = 3;

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

    pub fn to_wire(self: JobState) []const u8 {
        return @tagName(self);
    }

    pub fn from_wire(s: []const u8) ?JobState {
        return std.meta.stringToEnum(JobState, s);
    }
};

pub const PresenceState = enum {
    present,
    away,
    unknown,

    pub fn to_wire(self: PresenceState) []const u8 {
        return @tagName(self);
    }

    pub fn from_wire(s: []const u8) PresenceState {
        return std.meta.stringToEnum(PresenceState, s) orelse .unknown;
    }
};

pub const OnPresent = enum {
    idle,
    kill,

    pub fn to_wire(self: OnPresent) []const u8 {
        return @tagName(self);
    }

    pub fn from_wire(s: []const u8) ?OnPresent {
        return std.meta.stringToEnum(OnPresent, s);
    }
};

pub const Isolation = enum {
    bwrap,
    systemd_scope,
    unsandboxed,

    pub fn to_wire(self: Isolation) []const u8 {
        return @tagName(self);
    }

    pub fn from_wire(s: []const u8) ?Isolation {
        return std.meta.stringToEnum(Isolation, s);
    }
};

pub const ErrorCode = enum {
    unknown_request,
    unsupported_protocol,
    busy,
    not_found,
    invalid,
    forbidden,
    conflict,
    io,
    timeout,
    internal,

    pub fn to_wire(self: ErrorCode) []const u8 {
        return @tagName(self);
    }

    pub fn from_wire(s: []const u8) ErrorCode {
        return std.meta.stringToEnum(ErrorCode, s) orelse .internal;
    }
};

pub const Gpu = struct {
    name: []const u8,
    driver: []const u8,
    vram_bytes: ?u64 = null,
    util_pct: ?f64 = null,
};

pub const Disk = struct {
    mount: []const u8,
    size_bytes: u64,
    free_bytes: u64,
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
    cpu_count: u32,
    ram_bytes: u64,
    gpus: []const Gpu = &.{},
    disks: []const Disk = &.{},
    toolchains: []const Toolchain = &.{},
    collected_at_ms: i64,
};

pub const Sample = struct {
    sampled_at_ms: i64,
    cpu_pct: ?f64 = null,
    load1: ?f64 = null,
    mem_avail_bytes: ?u64 = null,
    swap_used_bytes: ?u64 = null,
};

pub const Presence = struct {
    state: PresenceState,
    idle_secs: ?u64 = null,
    locked: ?bool = null,
    source: []const u8 = "",
    sampled_at_ms: ?i64 = null,
};

pub const JobArtifact = struct {
    path: []const u8,
    size_bytes: ?u64 = null,
    sha256: ?[]const u8 = null,
};

pub const Hello = struct {
    protocol: u32 = protocol_version,
    node_id: []const u8,
    node_version: []const u8,
    addrs: []const []const u8 = &.{},
    inventory: ?Inventory = null,
    sample: ?Sample = null,
    presence: Presence = .{ .state = .unknown },
};

pub const HelloReply = struct {
    ok: bool,
    accepted_protocol: u32 = protocol_version,
    hub_version: []const u8 = "",
    node_name: []const u8 = "",
};

pub const SubmitJob = struct {
    job_id: []const u8,
    argv: []const []const u8,
    cwd: ?[]const u8 = null,
    artifacts: []const []const u8 = &.{},
    even_if_present: bool = false,
    on_present: OnPresent = .idle,
    timeout_secs: ?u64 = null,
};

pub const JobStatus = struct {
    job_id: []const u8,
    attempt: ?u32 = null,
};

pub const JobRef = struct {
    job_id: []const u8,
};

pub const FetchLog = struct {
    job_id: []const u8,
    offset: u64 = 0,
    limit: ?usize = null,
};

pub const FetchArtifact = struct {
    job_id: []const u8,
    path: []const u8,
    offset: u64 = 0,
    limit: ?usize = null,
};

pub const Update = struct {
    version: ?[]const u8 = null,
    channel: ?[]const u8 = null,
    url: ?[]const u8 = null,
    sha256: ?[]const u8 = null,
    probation_secs: ?u64 = null,
    rollout_id: ?[]const u8 = null,
};

pub const UpdateReply = struct {
    ok: bool,
    from: []const u8,
    to: []const u8,
    detail: []const u8 = "",
    restarting: bool = false,
};

pub const UploadUpdate = struct {
    rollout_id: []const u8,
    version: []const u8,
    sha256: []const u8,
    offset: u64,
    total_bytes: u64,
    hex: []const u8,
    eof: bool,
};

pub const UploadUpdateReply = struct {
    received_bytes: u64,
    ready: bool,
    sha256: ?[]const u8 = null,
};

pub const JobInfo = struct {
    job_id: []const u8,
    state: JobState,
    exit_code: ?i32 = null,
    @"error": ?[]const u8 = null,
    log_bytes: u64 = 0,
    artifacts: []const JobArtifact = &.{},
    held_reason: ?[]const u8 = null,
};

pub const JobReply = struct {
    job: JobInfo,
};

pub const JobList = struct {
    jobs: []const JobInfo,
};

pub const LogEnd = struct {
    eof: bool,
    next_offset: u64,
};

pub const ArtifactEnd = struct {
    eof: bool,
    next_offset: u64,
    size_bytes: u64,
    sha256: []const u8,
};

pub const ErrorReply = struct {
    code: ErrorCode,
    message: []const u8,
};

pub const Ping = struct {
    now_ms: i64 = 0,
};

pub const Request = union(enum) {
    hello: Hello,
    ping: Ping,
    submit_job: SubmitJob,
    job_status: JobStatus,
    cancel_job: JobRef,
    list_jobs: struct {},
    fetch_log: FetchLog,
    fetch_artifact: FetchArtifact,
    update: Update,
    upload_update: UploadUpdate,

    pub fn jsonStringify(self: Request, jw: anytype) !void {
        try jw.beginObject();
        switch (self) {
            .hello => |v| {
                try jw.objectField("type");
                try jw.write("hello");
                try write_struct_fields(v, jw);
            },
            .ping => |v| {
                try jw.objectField("type");
                try jw.write("ping");
                try write_struct_fields(v, jw);
            },
            .submit_job => |v| {
                try jw.objectField("type");
                try jw.write("submit_job");
                try write_struct_fields(v, jw);
            },
            .job_status => |v| {
                try jw.objectField("type");
                try jw.write("job_status");
                try write_struct_fields(v, jw);
            },
            .cancel_job => |v| {
                try jw.objectField("type");
                try jw.write("cancel_job");
                try write_struct_fields(v, jw);
            },
            .list_jobs => {
                try jw.objectField("type");
                try jw.write("list_jobs");
            },
            .fetch_log => |v| {
                try jw.objectField("type");
                try jw.write("fetch_log");
                try write_struct_fields(v, jw);
            },
            .fetch_artifact => |v| {
                try jw.objectField("type");
                try jw.write("fetch_artifact");
                try write_struct_fields(v, jw);
            },
            .update => |v| {
                try jw.objectField("type");
                try jw.write("update");
                try write_struct_fields(v, jw);
            },
            .upload_update => |v| {
                try jw.objectField("type");
                try jw.write("upload_update");
                try write_struct_fields(v, jw);
            },
        }
        try jw.endObject();
    }

    pub fn jsonParseFromValue(
        allocator: std.mem.Allocator,
        source: std.json.Value,
        options: std.json.ParseOptions,
    ) !Request {
        if (source != .object) return error.UnexpectedToken;
        const tag_name = json_util.value_str(source, "type") orelse return error.MissingField;
        const opts: std.json.ParseOptions = .{
            .ignore_unknown_fields = true,
            .allocate = options.allocate,
        };
        if (std.mem.eql(u8, tag_name, "hello")) {
            return .{ .hello = try std.json.parseFromValueLeaky(Hello, allocator, source, opts) };
        } else if (std.mem.eql(u8, tag_name, "ping")) {
            return .{ .ping = try std.json.parseFromValueLeaky(Ping, allocator, source, opts) };
        } else if (std.mem.eql(u8, tag_name, "submit_job")) {
            return .{ .submit_job = try std.json.parseFromValueLeaky(SubmitJob, allocator, source, opts) };
        } else if (std.mem.eql(u8, tag_name, "job_status")) {
            return .{ .job_status = try std.json.parseFromValueLeaky(JobStatus, allocator, source, opts) };
        } else if (std.mem.eql(u8, tag_name, "cancel_job")) {
            return .{ .cancel_job = try std.json.parseFromValueLeaky(JobRef, allocator, source, opts) };
        } else if (std.mem.eql(u8, tag_name, "list_jobs")) {
            return .{ .list_jobs = .{} };
        } else if (std.mem.eql(u8, tag_name, "fetch_log")) {
            return .{ .fetch_log = try std.json.parseFromValueLeaky(FetchLog, allocator, source, opts) };
        } else if (std.mem.eql(u8, tag_name, "fetch_artifact")) {
            return .{ .fetch_artifact = try std.json.parseFromValueLeaky(FetchArtifact, allocator, source, opts) };
        } else if (std.mem.eql(u8, tag_name, "update")) {
            return .{ .update = try std.json.parseFromValueLeaky(Update, allocator, source, opts) };
        } else if (std.mem.eql(u8, tag_name, "upload_update")) {
            return .{ .upload_update = try std.json.parseFromValueLeaky(UploadUpdate, allocator, source, opts) };
        } else {
            return error.UnknownField;
        }
    }
};

fn write_struct_fields(v: anytype, jw: anytype) !void {
    const T = @TypeOf(v);
    const info = @typeInfo(T).@"struct";
    inline for (info.field_names, info.field_types) |f_name, f_type| {
        if (f_type == void) continue;
        try jw.objectField(f_name);
        try jw.write(@field(v, f_name));
    }
}

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
