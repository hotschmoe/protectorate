const std = @import("std");
const git_process = @import("../git_process.zig");
const schema = @import("../api/schema.zig");

pub fn provision_worktree(
    gpa: std.mem.Allocator,
    io: std.Io,
    base_repo: []const u8,
    wt_path: []const u8,
    branch_name: []const u8,
) !void {
    if (std.Io.Dir.cwd().openDir(io, wt_path, .{})) |dir| {
        dir.close(io);
        return;
    } else |_| {}

    if (try revision(gpa, io, .check_worktree, base_repo, &.{ "rev-parse", "--is-inside-work-tree" })) |inside| {
        defer gpa.free(inside);
        if (std.mem.eql(u8, inside, "true")) {
            if (std.fs.path.dirname(wt_path)) |parent| {
                try std.Io.Dir.cwd().createDirPath(io, parent);
            }
            if (try worktree_add(gpa, io, base_repo, &.{ "worktree", "add", "-b", branch_name, "--", wt_path })) return;
            if (try worktree_add(gpa, io, base_repo, &.{ "worktree", "add", "--", wt_path, branch_name })) return;
        }
    }
    try std.Io.Dir.cwd().createDirPath(io, wt_path);
}

fn worktree_add(gpa: std.mem.Allocator, io: std.Io, base_repo: []const u8, args: []const []const u8) !bool {
    const res = try git_process.run(gpa, io, .worktree_add, base_repo, args, 64 * 1024);
    defer gpa.free(res.stdout);
    defer gpa.free(res.stderr);
    return res.term.success();
}

pub fn remove_worktree(gpa: std.mem.Allocator, io: std.Io, wt_path: []const u8) !void {
    const common_dir = try revision(gpa, io, .common_dir, wt_path, &.{
        "rev-parse", "--path-format=absolute", "--git-common-dir",
    });
    defer if (common_dir) |dir| gpa.free(dir);

    const res = git_process.run(gpa, io, .worktree_remove, common_dir orelse wt_path, &.{
        "worktree", "remove", "--force", "--", wt_path,
    }, 16 * 1024) catch |err| switch (err) {
        error.FileNotFound => {
            try std.Io.Dir.cwd().deleteTree(io, wt_path);
            return;
        },
        else => return err,
    };
    defer gpa.free(res.stdout);
    defer gpa.free(res.stderr);
    if (!res.term.success()) try std.Io.Dir.cwd().deleteTree(io, wt_path);
}

fn revision(
    gpa: std.mem.Allocator,
    io: std.Io,
    op: git_process.Operation,
    workdir: []const u8,
    args: []const []const u8,
) !?[]const u8 {
    const res = git_process.run(gpa, io, op, workdir, args, 8192) catch |err| switch (err) {
        error.FileNotFound => return null,
        else => return err,
    };
    defer gpa.free(res.stdout);
    defer gpa.free(res.stderr);
    if (!res.term.success()) return null;
    const value = std.mem.trimEnd(u8, res.stdout, "\r\n");
    if (value.len == 0) return null;
    return try gpa.dupe(u8, value);
}

pub fn current_branch(gpa: std.mem.Allocator, io: std.Io, workdir: []const u8) !?[]const u8 {
    return revision(gpa, io, .current_branch, workdir, &.{ "rev-parse", "--abbrev-ref", "HEAD" });
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
    const branch = branch_opt orelse (try current_branch(arena, io, workdir)) orelse return error.NotABranch;
    const commit = (try revision(arena, io, .head_commit, workdir, &.{ "rev-parse", "HEAD" })) orelse "";

    const args: []const []const u8 = if (force)
        &.{ "push", "--force-with-lease", "--", remote, branch }
    else
        &.{ "push", "--", remote, branch };

    const res = try git_process.run(arena, io, .push, workdir, args, 64 * 1024);
    defer arena.free(res.stdout);
    defer arena.free(res.stderr);
    return .{
        .sleeve_id = sleeve_id,
        .remote = remote,
        .branch = branch,
        .commit = commit,
        .pushed = res.term.success(),
    };
}
