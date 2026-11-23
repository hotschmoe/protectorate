const std = @import("std");
const tiger = @import("../tiger.zig");
const schema = @import("../api/schema.zig");

pub fn provision_worktree(
    gpa: std.mem.Allocator,
    io: std.Io,
    base_repo: []const u8,
    wt_path: []const u8,
    branch_name: []const u8,
) !void {
    if (std.Io.Dir.cwd().openDir(io, wt_path, .{})) |dir_val| {
        var d = dir_val;
        d.close(io);
        return;
    } else |_| {}

    const check_git = std.process.run(gpa, io, .{
        .argv = &.{ "git", "-C", base_repo, "rev-parse", "--is-inside-work-tree" },
        .stdout_limit = .limited(4096),
        .stderr_limit = .limited(4096),
    }) catch null;
    if (check_git) |cg| {
        const is_git = cg.term.success();
        gpa.free(cg.stdout);
        gpa.free(cg.stderr);
        if (is_git) {
            const res = try std.process.run(gpa, io, .{
                .argv = &.{ "git", "-C", base_repo, "worktree", "add", "-b", branch_name, wt_path },
                .stdout_limit = .limited(64 * 1024),
                .stderr_limit = .limited(64 * 1024),
            });
            defer {
                gpa.free(res.stdout);
                gpa.free(res.stderr);
            }
            if (res.term.success()) return;
        }
    }
    try std.Io.Dir.cwd().createDirPath(io, wt_path);
}

pub fn remove_worktree(gpa: std.mem.Allocator, io: std.Io, wt_path: []const u8) void {
    const res = std.process.run(gpa, io, .{
        .argv = &.{ "git", "-C", wt_path, "worktree", "remove", "--force", wt_path },
        .stdout_limit = .limited(16 * 1024),
        .stderr_limit = .limited(16 * 1024),
    }) catch return;
    gpa.free(res.stdout);
    gpa.free(res.stderr);
}

pub fn current_branch(arena: std.mem.Allocator, io: std.Io, workdir: []const u8) ?[]const u8 {
    const res = std.process.run(arena, io, .{
        .argv = &.{ "git", "-C", workdir, "rev-parse", "--abbrev-ref", "HEAD" },
        .stdout_limit = .limited(4096),
        .stderr_limit = .limited(4096),
    }) catch return null;
    if (!res.term.success()) return null;
    const tr = std.mem.trim(u8, res.stdout, " \t\r\n");
    if (tr.len == 0) return null;
    return arena.dupe(u8, tr) catch null;
}

pub fn head_commit(arena: std.mem.Allocator, io: std.Io, workdir: []const u8) ?[]const u8 {
    const res = std.process.run(arena, io, .{
        .argv = &.{ "git", "-C", workdir, "rev-parse", "HEAD" },
        .stdout_limit = .limited(4096),
        .stderr_limit = .limited(4096),
    }) catch return null;
    if (!res.term.success()) return null;
    const tr = std.mem.trim(u8, res.stdout, " \t\r\n");
    if (tr.len == 0) return null;
    return arena.dupe(u8, tr) catch null;
}

pub fn push(
    arena: std.mem.Allocator,
    io: std.Io,
    sleeve_id: []const u8,
    workdir: []const u8,
    remote_opt: ?[]const u8,
    branch_opt: ?[]const u8,
    force: bool,
) !schema.GitPushResult {
    const remote = remote_opt orelse "origin";
    const branch = branch_opt orelse current_branch(arena, io, workdir) orelse return error.NotABranch;
    const commit = head_commit(arena, io, workdir) orelse "";

    var argv: std.ArrayList([]const u8) = .empty;
    try argv.appendSlice(arena, &.{ "git", "-C", workdir, "push" });
    if (force) try argv.append(arena, "--force-with-lease");
    try argv.appendSlice(arena, &.{ remote, branch });

    const res = try std.process.run(arena, io, .{
        .argv = argv.items,
        .stdout_limit = .limited(64 * 1024),
        .stderr_limit = .limited(64 * 1024),
    });
    if (!res.term.success()) {
        return .{
            .sleeve_id = sleeve_id,
            .remote = remote,
            .branch = branch,
            .commit = commit,
            .pushed = false,
        };
    }
    return .{
        .sleeve_id = sleeve_id,
        .remote = remote,
        .branch = branch,
        .commit = commit,
        .pushed = true,
    };
}
