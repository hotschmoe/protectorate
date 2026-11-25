const std = @import("std");
const builtin = @import("builtin");
const tiger = @import("tiger.zig");
const linux = std.os.linux;
const process = std.process;
const Io = std.Io;

var hub_environ: process.Environ = undefined;

pub fn io_for_serve(init: process.Init, vtable: *Io.VTable) Io {
    if (comptime builtin.os.tag != .linux) return init.io;
    hub_environ = init.minimal.environ;
    vtable.* = init.io.vtable.*;
    vtable.processSpawn = spawn;
    return .{ .userdata = init.io.userdata, .vtable = vtable };
}

fn spawn(_: ?*anyopaque, options: process.SpawnOptions) process.SpawnError!process.Child {
    return spawn_linux(options, null, null) catch |err| switch (err) {
        error.Timeout => unreachable,
        else => |spawn_err| return spawn_err,
    };
}

pub fn spawn_until(io: Io, options: process.SpawnOptions, deadline: Io.Clock.Timestamp) !process.Child {
    if (deadline.durationFromNow(io).raw.nanoseconds <= 0) return error.Timeout;
    if (comptime builtin.os.tag == .linux) {
        if (io.vtable.processSpawn == &spawn) return spawn_linux(options, io, deadline);
    }
    return process.spawn(io, options);
}

fn spawn_linux(
    options: process.SpawnOptions,
    io: ?Io,
    deadline: ?Io.Clock.Timestamp,
) (process.SpawnError || error{Timeout})!process.Child {
    tiger.assert(options.argv.len > 0);
    tiger.assert(options.exe == .detect and options.expand_arg0 == .no_expand);
    tiger.assert(options.inherit_dirs.len == 0 and options.inherit_files.len == 0);
    tiger.assert(options.uid == null and options.gid == null and !options.start_suspended);
    tiger.assert(options.progress_node.index == .none);

    var arena_state = std.heap.ArenaAllocator.init(std.heap.page_allocator);
    defer arena_state.deinit();
    const arena = arena_state.allocator();
    const argv = try arena.allocSentinel(?[*:0]const u8, options.argv.len, null);
    for (argv, options.argv) |*out, arg| out.* = try arena.dupeSentinel(u8, arg, 0);
    const cwd = switch (options.cwd) {
        .inherit => null,
        .path => |path| try arena.dupeSentinel(u8, path, 0),
        .dir => unreachable,
    };
    const env = if (options.environ_map) |map|
        try map.createPosixBlock(arena, .{ .zig_progress_fd = -1 })
    else
        try hub_environ.createPosixBlock(arena, .{ .zig_progress_fd = -1 });
    const path = hub_environ.getPosix("PATH") orelse std.Io.Threaded.default_PATH;

    const streams = [_]process.SpawnOptions.StdIo{ options.stdin, options.stdout, options.stderr };
    var parent_fds: [streams.len]?linux.fd_t = @splat(null);
    errdefer for (parent_fds) |fd| close(fd);
    var child_fds: [streams.len]?linux.fd_t = @splat(null);
    defer for (child_fds) |fd| close(fd);
    for (streams, 0..) |stream, i| {
        switch (stream) {
            .pipe => {
                const fds = try pipe();
                const child_end: usize = @intFromBool(i != 0);
                child_fds[i] = fds[child_end];
                parent_fds[i] = fds[1 - child_end];
            },
            .ignore => child_fds[i] = try above_stdio(try fd_result(
                linux.openat(linux.AT.FDCWD, "/dev/null", .{ .ACCMODE = .RDWR, .CLOEXEC = true }, 0),
            )),
            .inherit => {},
            .file, .close => unreachable,
        }
    }

    const report = try pipe();
    defer close(report[0]);
    const pid = blk: {
        defer close(report[1]);
        const result = linux.fork();
        switch (linux.errno(result)) {
            .SUCCESS => {},
            .AGAIN, .NOMEM => return error.SystemResources,
            .NOSYS => return error.OperationUnsupported,
            else => |err| return std.posix.unexpectedErrno(err),
        }
        if (result == 0) {
            const err = exec_child(argv, cwd, env, path, child_fds, options.pgid);
            const code = @intFromError(err);
            const bytes = std.mem.asBytes(&code);
            while (linux.errno(linux.write(report[1], bytes, bytes.len)) == .INTR) {}
            linux.exit_group(1);
        }
        break :blk @as(linux.pid_t, @intCast(result));
    };

    errdefer {
        _ = linux.kill(pid, .KILL);
        if (options.pgid == 0) _ = linux.kill(-pid, .KILL);
        reap_failed(pid);
    }
    var code: @TypeOf(@intFromError(error.FileNotFound)) = undefined;
    const bytes = std.mem.asBytes(&code);
    while (true) {
        if (deadline) |limit| try wait_for_exec(io.?, report[0], limit);
        const result = linux.read(report[0], bytes, bytes.len);
        switch (linux.errno(result)) {
            .INTR => continue,
            .SUCCESS => {
                if (result == 0) break;
                tiger.assert(result == bytes.len);
                return @errorCast(@errorFromInt(code));
            },
            else => |err| return std.posix.unexpectedErrno(err),
        }
    }

    return .{
        .id = pid,
        .thread_handle = {},
        .stdin = as_file(parent_fds[0]),
        .stdout = as_file(parent_fds[1]),
        .stderr = as_file(parent_fds[2]),
        .request_resource_usage_statistics = options.request_resource_usage_statistics,
    };
}

fn wait_for_exec(io: Io, fd: linux.fd_t, deadline: Io.Clock.Timestamp) !void {
    var fds = [1]linux.pollfd{.{ .fd = fd, .events = linux.POLL.IN, .revents = 0 }};
    while (true) {
        const remaining_ns = deadline.durationFromNow(io).raw.nanoseconds;
        if (remaining_ns <= 0) return error.Timeout;
        const remaining_ms = @min(std.math.maxInt(i32), @divFloor(remaining_ns - 1, std.time.ns_per_ms) + 1);
        const rc = linux.poll(&fds, fds.len, @intCast(remaining_ms));
        switch (linux.errno(rc)) {
            .SUCCESS => if (rc != 0) return,
            .INTR => {},
            else => |err| return std.posix.unexpectedErrno(err),
        }
    }
}

fn exec_child(
    argv: [:null]const ?[*:0]const u8,
    cwd: ?[:0]const u8,
    env: process.Environ.PosixBlock,
    path: []const u8,
    child_fds: [3]?linux.fd_t,
    pgid: ?linux.pid_t,
) process.SpawnError {
    if (cwd) |dir| switch (linux.errno(linux.chdir(dir))) {
        .SUCCESS => {},
        else => |err| return path_error(err),
    };
    for (child_fds, 0..) |maybe_fd, i| {
        const fd = maybe_fd orelse continue;
        while (true) switch (linux.errno(linux.dup2(fd, @intCast(i)))) {
            .SUCCESS => break,
            .INTR, .BUSY => continue,
            else => return error.Unexpected,
        };
    }
    if (pgid) |group| switch (linux.errno(linux.setpgid(0, group))) {
        .SUCCESS => {},
        .INVAL => return error.InvalidProcessGroupId,
        .PERM => return error.PermissionDenied,
        else => return error.Unexpected,
    };

    const name = std.mem.span(argv[0].?);
    if (std.mem.indexOfScalar(u8, name, '/') != null) {
        return path_error(linux.errno(linux.execve(name, argv.ptr, env.slice.ptr)));
    }
    var paths = std.mem.tokenizeScalar(u8, path, ':');
    var last_error: process.SpawnError = error.FileNotFound;
    var buf: [std.fs.max_path_bytes]u8 = undefined;
    while (paths.next()) |dir| {
        const candidate = std.fmt.bufPrintSentinel(&buf, "{s}/{s}", .{ dir, name }, 0) catch return error.NameTooLong;
        const err = path_error(linux.errno(linux.execve(candidate, argv.ptr, env.slice.ptr)));
        switch (err) {
            error.FileNotFound, error.NotDir => if (last_error != error.AccessDenied) {
                last_error = err;
            },
            error.AccessDenied => last_error = err,
            else => return err,
        }
    }
    return last_error;
}

fn reap_failed(pid: linux.pid_t) void {
    var info: linux.siginfo_t = undefined;
    while (true) switch (linux.errno(linux.waitid(.PID, pid, &info, linux.W.EXITED, null))) {
        .INTR => continue,
        .SUCCESS => return,
        else => unreachable,
    };
}

fn pipe() process.SpawnError![2]linux.fd_t {
    var fds: [2]linux.fd_t = undefined;
    switch (linux.errno(linux.pipe2(&fds, .{ .CLOEXEC = true }))) {
        .SUCCESS => {},
        else => |err| return path_error(err),
    }
    const read_fd = above_stdio(fds[0]) catch |err| {
        close(fds[1]);
        return err;
    };
    errdefer close(read_fd);
    const write_fd = try above_stdio(fds[1]);
    return .{ read_fd, write_fd };
}

fn above_stdio(fd: linux.fd_t) process.SpawnError!linux.fd_t {
    if (fd > 2) return fd;
    defer close(fd);
    return fd_result(linux.fcntl(fd, linux.F.DUPFD_CLOEXEC, 3));
}

fn fd_result(result: usize) process.SpawnError!linux.fd_t {
    return switch (linux.errno(result)) {
        .SUCCESS => @intCast(result),
        else => |err| path_error(err),
    };
}

fn path_error(err: linux.E) process.SpawnError {
    return switch (err) {
        .NOENT => error.FileNotFound,
        .NOTDIR => error.NotDir,
        .ISDIR => error.IsDir,
        .ACCES => error.AccessDenied,
        .PERM => error.PermissionDenied,
        .INVAL, .NOEXEC, .LIBBAD => error.InvalidExe,
        .NAMETOOLONG => error.NameTooLong,
        .TXTBSY => error.FileBusy,
        .NODEV => error.NoDevice,
        .LOOP, .IO => error.FileSystem,
        .MFILE => error.ProcessFdQuotaExceeded,
        .NFILE => error.SystemFdQuotaExceeded,
        .@"2BIG", .NOMEM => error.SystemResources,
        else => error.Unexpected,
    };
}

fn as_file(fd: ?linux.fd_t) ?Io.File {
    return if (fd) |handle| .{ .handle = handle, .flags = .{ .nonblocking = false } } else null;
}

fn close(fd: ?linux.fd_t) void {
    if (fd) |handle| _ = linux.close(handle);
}

extern "c" fn waitid(id_type: c_int, id: std.posix.pid_t, info: *std.c.siginfo_t, options: c_int) c_int;

pub fn wait_for_exit(io: Io, id: process.Child.Id, deadline: Io.Clock.Timestamp) !void {
    const poll_interval: Io.Duration = .fromMilliseconds(10);
    while (true) {
        const remaining = deadline.durationFromNow(io).raw;
        if (remaining.nanoseconds <= 0) return error.Timeout;
        if (try has_exited(id)) return;
        try io.sleep(.{ .nanoseconds = @min(remaining.nanoseconds, poll_interval.nanoseconds) }, .awake);
    }
}

pub fn has_exited(id: process.Child.Id) !bool {
    switch (builtin.os.tag) {
        .linux => {
            var info: linux.siginfo_t = std.mem.zeroes(linux.siginfo_t);
            const flags = linux.W.EXITED | linux.W.NOHANG | linux.W.NOWAIT;
            return switch (linux.errno(linux.waitid(.PID, id, &info, flags, null))) {
                .SUCCESS => info.fields.common.first.piduid.pid != 0,
                .INTR => false,
                else => |err| std.posix.unexpectedErrno(err),
            };
        },
        .macos => {
            const p_pid = 1;
            const w_exited = 0x04;
            const w_nowait = 0x20;
            var info: std.c.siginfo_t = std.mem.zeroes(std.c.siginfo_t);
            return switch (std.posix.errno(waitid(p_pid, @intCast(id), &info, std.c.W.NOHANG | w_exited | w_nowait))) {
                .SUCCESS => info.pid != 0,
                .INTR => false,
                else => |err| std.posix.unexpectedErrno(err),
            };
        },
        .windows => {
            const windows = std.os.windows;
            const poll: windows.LARGE_INTEGER = 0;
            return switch (windows.ntdll.NtWaitForSingleObject(id, .FALSE, &poll)) {
                .WAIT_0 => true,
                .TIMEOUT => false,
                else => |status| windows.unexpectedStatus(status),
            };
        },
        else => @compileError("child exit polling requires a supported platform"),
    }
}
