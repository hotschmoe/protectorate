const std = @import("std");
const tiger = @import("../tiger.zig");

pub const MountKind = enum {
    ro,
    rw,
    mask_dir,
    mask_file,
    runtime,
};

pub const GuardMode = enum {
    none,
    bwrap,
    landlock,
};

pub const stripped_nested_env_keys = [_][]const u8{
    "CLAUDECODE",
    "CLAUDE_CODE_SSE_PORT",
    "CLAUDE_CODE_ENTRYPOINT",
    "CODEX_THREAD_ID",
};

pub const Mount = struct {
    path: []const u8,
    kind: MountKind,
};

pub const EnvVar = struct {
    key: []const u8,
    value: []const u8,
};

pub const SandboxSpec = struct {
    workspace: []const u8,
    readonly: bool = false,
    host_home: ?[]const u8 = null,
    runtime_dir: ?[]const u8 = null,
    git_dirs: []const []const u8 = &.{},
    extra_ro: []const []const u8 = &.{},
    extra_rw: []const []const u8 = &.{},
    masks: []const Mount = &.{},
    env_allow: []const []const u8 = &default_env_allow_keys,
    env_set: []const EnvVar = &.{},
    memory_max_bytes: ?u64 = null,
};

pub const default_env_allow_keys = [_][]const u8{
    "PATH",
    "LANG",
    "LANGUAGE",
    "LC_ALL",
    "LC_CTYPE",
    "TERM",
    "COLORTERM",
    "TZ",
    "USER",
    "LOGNAME",
    "SHELL",
};

const default_mask_rels = [_]struct { rel: []const u8, is_dir: bool }{
    .{ .rel = ".ssh", .is_dir = true },
    .{ .rel = ".claude", .is_dir = true },
    .{ .rel = ".claude.json", .is_dir = false },
    .{ .rel = ".codex", .is_dir = true },
    .{ .rel = ".grok", .is_dir = true },
    .{ .rel = ".gemini", .is_dir = true },
    .{ .rel = ".pi", .is_dir = true },
    .{ .rel = ".config/gh", .is_dir = true },
    .{ .rel = ".git-credentials", .is_dir = false },
    .{ .rel = ".config/git/credentials", .is_dir = false },
    .{ .rel = ".netrc", .is_dir = false },
    .{ .rel = ".gnupg", .is_dir = true },
    .{ .rel = ".aws", .is_dir = true },
    .{ .rel = ".config/gcloud", .is_dir = true },
    .{ .rel = ".azure", .is_dir = true },
    .{ .rel = ".docker", .is_dir = true },
    .{ .rel = ".kube", .is_dir = true },
};

pub const BuildError = error{
    InvalidPath,
    GitDirMasked,
    OutOfMemory,
};

pub fn is_clean_abs(p: []const u8) bool {
    if (p.len == 0 or p[0] != '/') return false;
    if (p.len > 1 and p[p.len - 1] == '/') return false;
    var it = std.mem.splitScalar(u8, p[1..], '/');
    while (it.next()) |seg| {
        if (seg.len == 0) return false;
        if (std.mem.eql(u8, seg, ".") or std.mem.eql(u8, seg, "..")) return false;
    }
    return true;
}

pub fn paths_overlap(a: []const u8, b: []const u8) bool {
    if (!is_clean_abs(a) or !is_clean_abs(b)) return false;
    return is_within(a, b) or is_within(b, a);
}

fn is_within(child: []const u8, parent: []const u8) bool {
    if (std.mem.eql(u8, child, parent)) return true;
    if (std.mem.eql(u8, parent, "/")) return true;
    if (child.len <= parent.len) return false;
    return std.mem.startsWith(u8, child, parent) and child[parent.len] == '/';
}

fn path_depth(p: []const u8) usize {
    if (std.mem.eql(u8, p, "/")) return 0;
    var count: usize = 0;
    var it = std.mem.splitScalar(u8, p[1..], '/');
    while (it.next()) |seg| {
        if (seg.len > 0) count += 1;
    }
    return count;
}

pub fn default_masks(gpa: std.mem.Allocator, host_home: []const u8) BuildError![]Mount {
    if (!is_clean_abs(host_home)) return error.InvalidPath;
    var out = try gpa.alloc(Mount, default_mask_rels.len);
    errdefer gpa.free(out);
    var init_n: usize = 0;
    errdefer {
        for (out[0..init_n]) |m| gpa.free(m.path);
    }
    for (default_mask_rels, 0..) |item, i| {
        out[i] = .{
            .path = try std.fmt.allocPrint(gpa, "{s}/{s}", .{ host_home, item.rel }),
            .kind = if (item.is_dir) .mask_dir else .mask_file,
        };
        init_n += 1;
    }
    return out;
}

pub fn build_mounts(gpa: std.mem.Allocator, spec: SandboxSpec) BuildError![]Mount {
    if (!is_clean_abs(spec.workspace)) return error.InvalidPath;

    var list: std.ArrayList(Mount) = .empty;
    errdefer list.deinit(gpa);

    try list.append(gpa, .{
        .path = spec.workspace,
        .kind = if (spec.readonly) .ro else .rw,
    });

    for (spec.extra_ro) |p| {
        if (!is_clean_abs(p)) return error.InvalidPath;
        try list.append(gpa, .{ .path = p, .kind = .ro });
    }
    for (spec.extra_rw) |p| {
        if (!is_clean_abs(p)) return error.InvalidPath;
        try list.append(gpa, .{ .path = p, .kind = if (spec.readonly) .ro else .rw });
    }

    for (spec.masks) |m| {
        if (!is_clean_abs(m.path)) return error.InvalidPath;
        try list.append(gpa, m);
    }

    if (spec.runtime_dir) |rd| {
        if (!is_clean_abs(rd)) return error.InvalidPath;
        try list.append(gpa, .{ .path = rd, .kind = .runtime });
    }

    for (spec.git_dirs) |g| {
        if (!is_clean_abs(g)) return error.InvalidPath;
        for (spec.masks) |m| {
            if (paths_overlap(g, m.path)) return error.GitDirMasked;
        }
        try list.append(gpa, .{ .path = g, .kind = if (spec.readonly) .ro else .rw });
    }

    std.mem.sort(Mount, list.items, {}, struct {
        fn less(_: void, a: Mount, b: Mount) bool {
            const da = path_depth(a.path);
            const db = path_depth(b.path);
            if (da != db) return da < db;
            return std.mem.lessThan(u8, a.path, b.path);
        }
    }.less);

    return list.toOwnedSlice(gpa);
}

pub fn build_bwrap_argv(
    gpa: std.mem.Allocator,
    spec: SandboxSpec,
    inner_argv: []const []const u8,
) BuildError![][]const u8 {
    tiger.assert(inner_argv.len > 0);
    const mounts = try build_mounts(gpa, spec);
    defer gpa.free(mounts);

    var args: std.ArrayList([]const u8) = .empty;
    errdefer args.deinit(gpa);

    try args.appendSlice(gpa, &.{
        "bwrap",
        "--unshare-pid",
        "--die-with-parent",
        "--ro-bind",
        "/",
        "/",
        "--dev",
        "/dev",
        "--proc",
        "/proc",
        "--tmpfs",
        "/tmp",
        "--clearenv",
    });

    for (spec.env_set) |ev| {
        try args.appendSlice(gpa, &.{ "--setenv", ev.key, ev.value });
    }

    for (mounts) |m| {
        switch (m.kind) {
            .ro => try args.appendSlice(gpa, &.{ "--ro-bind", m.path, m.path }),
            .rw => try args.appendSlice(gpa, &.{ "--bind", m.path, m.path }),
            .mask_dir, .runtime => try args.appendSlice(gpa, &.{ "--tmpfs", m.path }),
            .mask_file => try args.appendSlice(gpa, &.{ "--ro-bind", "/dev/null", m.path }),
        }
    }

    try args.appendSlice(gpa, &.{ "--chdir", spec.workspace, "--" });
    try args.appendSlice(gpa, inner_argv);
    return args.toOwnedSlice(gpa);
}

pub fn build_guard_argv(
    gpa: std.mem.Allocator,
    self_exe: []const u8,
    spec: SandboxSpec,
    inner_argv: []const []const u8,
) BuildError![][]const u8 {
    tiger.assert(inner_argv.len > 0);
    tiger.assert(self_exe.len > 0);
    const mounts = try build_mounts(gpa, spec);
    defer gpa.free(mounts);

    var args: std.ArrayList([]const u8) = .empty;
    errdefer free_guard_argv(gpa, args.items);

    try args.appendSlice(gpa, &.{
        self_exe,
        "__guard",
        "--cwd",
        spec.workspace,
    });
    if (spec.readonly) {
        try args.append(gpa, "--readonly");
    }
    if (spec.memory_max_bytes) |lim| {
        const mem_flag = try std.fmt.allocPrint(gpa, "--mem={d}", .{lim});
        try args.append(gpa, mem_flag);
    }

    for (mounts) |m| {
        switch (m.kind) {
            .rw => try args.appendSlice(gpa, &.{ "--rw", m.path }),
            .runtime => try args.appendSlice(gpa, &.{ "--tmp", m.path }),
            .ro, .mask_dir, .mask_file => {},
        }
    }

    try args.append(gpa, "--");
    try args.appendSlice(gpa, inner_argv);
    return args.toOwnedSlice(gpa);
}

pub fn free_guard_argv(gpa: std.mem.Allocator, argv: []const []const u8) void {
    for (argv) |s| {
        if (std.mem.startsWith(u8, s, "--mem=")) {
            gpa.free(s);
        }
    }
    gpa.free(argv);
}
