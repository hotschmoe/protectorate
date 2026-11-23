const std = @import("std");
const builtin = @import("builtin");
const tiger = @import("../tiger.zig");
const spec = @import("spec.zig");

pub fn probe_bwrap(gpa: std.mem.Allocator, io: std.Io) bool {
    if (builtin.os.tag != .linux) return false;
    const res = std.process.run(gpa, io, .{
        .argv = &.{ "bwrap", "--version" },
        .stdout_limit = .limited(4096),
        .stderr_limit = .limited(4096),
    }) catch return false;
    gpa.free(res.stdout);
    gpa.free(res.stderr);
    return res.term.success();
}

pub fn probe_systemd_scope(gpa: std.mem.Allocator, io: std.Io) bool {
    if (builtin.os.tag != .linux) return false;
    const res = std.process.run(gpa, io, .{
        .argv = &.{ "systemd-run", "--user", "--version" },
        .stdout_limit = .limited(4096),
        .stderr_limit = .limited(4096),
    }) catch return false;
    gpa.free(res.stdout);
    gpa.free(res.stderr);
    return res.term.success();
}

pub fn resolve_git_dirs(gpa: std.mem.Allocator, io: std.Io, workdir: []const u8) ![]const []const u8 {
    var out: std.ArrayList([]const u8) = .empty;
    errdefer {
        for (out.items) |p| gpa.free(p);
        out.deinit(gpa);
    }

    const res = std.process.run(gpa, io, .{
        .argv = &.{ "git", "-C", workdir, "rev-parse", "--git-dir", "--git-common-dir" },
        .stdout_limit = .limited(64 * 1024),
        .stderr_limit = .limited(16 * 1024),
    }) catch return try out.toOwnedSlice(gpa);
    defer {
        gpa.free(res.stdout);
        gpa.free(res.stderr);
    }
    if (!res.term.success()) return try out.toOwnedSlice(gpa);

    var lines = std.mem.splitScalar(u8, res.stdout, '\n');
    while (lines.next()) |raw| {
        const trimmed = std.mem.trim(u8, raw, " \t\r");
        if (trimmed.len == 0) continue;
        const joined = if (std.fs.path.isAbsolute(trimmed))
            try gpa.dupe(u8, trimmed)
        else
            try std.fs.path.join(gpa, &.{ workdir, trimmed });
        defer gpa.free(joined);

        const real_z = std.Io.Dir.cwd().realPathFileAlloc(io, joined, gpa) catch continue;
        const clean = try gpa.dupe(u8, real_z);
        gpa.free(real_z);
        if (!spec.is_clean_abs(clean)) {
            gpa.free(clean);
            continue;
        }
        var dup = false;
        for (out.items) |prev| {
            if (std.mem.eql(u8, prev, clean)) {
                dup = true;
                break;
            }
        }
        if (dup) {
            gpa.free(clean);
        } else {
            try out.append(gpa, clean);
        }
    }
    return try out.toOwnedSlice(gpa);
}

pub fn kill_scope(gpa: std.mem.Allocator, io: std.Io, scope_name: []const u8) void {
    if (builtin.os.tag != .linux) return;
    if (scope_name.len == 0) return;
    const res = std.process.run(gpa, io, .{
        .argv = &.{ "systemctl", "--user", "stop", scope_name },
        .stdout_limit = .limited(4096),
        .stderr_limit = .limited(4096),
    }) catch return;
    gpa.free(res.stdout);
    gpa.free(res.stderr);
}
