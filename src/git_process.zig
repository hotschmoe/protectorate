const std = @import("std");
const builtin = @import("builtin");
const child_process = @import("child_process.zig");
const tiger = @import("tiger.zig");

const metadata_timeout: std.Io.Duration = .fromSeconds(5);
const worktree_add_timeout: std.Io.Duration = .fromSeconds(60);
const worktree_remove_timeout: std.Io.Duration = .fromSeconds(30);
const push_timeout: std.Io.Duration = .fromSeconds(120);

pub const Operation = enum {
    version,
    check_worktree,
    common_dir,
    current_branch,
    head_commit,
    worktree_add,
    worktree_remove,
    push,

    fn timeout(op: Operation) std.Io.Duration {
        return switch (op) {
            .version, .check_worktree, .common_dir, .current_branch, .head_commit => metadata_timeout,
            .worktree_add => worktree_add_timeout,
            .worktree_remove => worktree_remove_timeout,
            .push => push_timeout,
        };
    }

    fn timeout_error(op: Operation) anyerror {
        return switch (op) {
            .version => error.GitVersionTimedOut,
            .check_worktree => error.GitCheckWorktreeTimedOut,
            .common_dir => error.GitCommonDirTimedOut,
            .current_branch => error.GitCurrentBranchTimedOut,
            .head_commit => error.GitHeadCommitTimedOut,
            .worktree_add => error.GitWorktreeAddTimedOut,
            .worktree_remove => error.GitWorktreeRemoveTimedOut,
            .push => error.GitPushTimedOut,
        };
    }
};

pub fn run(
    gpa: std.mem.Allocator,
    io: std.Io,
    op: Operation,
    workdir: ?[]const u8,
    args: []const []const u8,
    output_limit: usize,
) !std.process.RunResult {
    const hooks_path = if (builtin.os.tag == .windows) "core.hooksPath=NUL" else "core.hooksPath=/dev/null";
    var argv: tiger.BoundedArray([]const u8, 16) = .{};
    try argv.append_slice(&.{ "git", "-c", hooks_path, "-c", "core.fsmonitor=false" });
    if (workdir) |dir| try argv.append_slice(&.{ "-C", dir });
    try argv.append_slice(args);

    const deadline: std.Io.Clock.Timestamp = .fromNow(io, .{ .clock = .awake, .raw = op.timeout() });
    return run_until(gpa, io, argv.slice(), deadline, output_limit) catch |err| switch (err) {
        error.Timeout => {
            std.log.warn("git {s} in {s} timed out after {d} ms", .{
                @tagName(op), workdir orelse "the current directory", op.timeout().toMilliseconds(),
            });
            return op.timeout_error();
        },
        else => return err,
    };
}

fn run_until(
    gpa: std.mem.Allocator,
    io: std.Io,
    argv: []const []const u8,
    deadline: std.Io.Clock.Timestamp,
    output_limit: usize,
) !std.process.RunResult {
    var child = try std.process.spawn(io, .{
        .argv = argv,
        .stdin = .ignore,
        .stdout = .pipe,
        .stderr = .pipe,
        .pgid = if (builtin.os.tag == .windows) null else 0,
        .create_no_window = true,
    });
    defer if (child.id) |id| {
        if (comptime builtin.os.tag != .windows) std.posix.kill(-id, .KILL) catch {};
        child.kill(io);
    };

    {
        var storage: std.Io.File.MultiReader.Buffer(2) = undefined;
        var reader: std.Io.File.MultiReader = undefined;
        reader.init(gpa, io, storage.toStreams(), &.{ child.stdout.?, child.stderr.? });
        defer reader.deinit();

        while (reader.fill(64, .{ .deadline = deadline })) |_| {
            if (reader.reader(0).bufferedLen() > output_limit or reader.reader(1).bufferedLen() > output_limit) {
                return error.StreamTooLong;
            }
            if (deadline.durationFromNow(io).raw.nanoseconds <= 0) return error.Timeout;
        } else |err| switch (err) {
            error.EndOfStream => {},
            else => return err,
        }
        try reader.checkAnyError();

        try child_process.wait_for_exit(io, child.id.?, deadline);
        const term = try child.wait(io);
        const stdout = try reader.toOwnedSlice(0);
        errdefer gpa.free(stdout);
        return .{ .term = term, .stdout = stdout, .stderr = try reader.toOwnedSlice(1) };
    }
}
