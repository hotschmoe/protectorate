const std = @import("std");
const builtin = @import("builtin");
const tiger = @import("../tiger.zig");
const proto = @import("protocol.zig");
const claude = @import("claude.zig");
const codex = @import("codex.zig");
const grok = @import("grok.zig");
const pi = @import("pi.zig");
const antigravity = @import("antigravity.zig");
const spec = @import("../sandbox/spec.zig");

pub const OnEventFn = *const fn (ctx: ?*anyopaque, ev: proto.DriverEvent) void;
pub const OnExitFn = *const fn (ctx: ?*anyopaque, g: u64, exit_code: ?u8, stderr_tail: []const u8) void;

pub const Config = struct {
    harness: proto.Harness,
    g: u64,
    program: []const u8,
    cwd: []const u8,
    model: ?[]const u8 = null,
    provider: ?[]const u8 = null,
    effort: ?[]const u8 = null,
    resume_id: ?[]const u8 = null,
    session_id: ?[]const u8 = null,
    permission: proto.Permission = .unsupervised,
    extra_args: []const []const u8 = &.{},
    sandbox_mode: spec.GuardMode = .none,
    guard_exe: ?[]const u8 = null,
    sandbox: ?spec.SandboxSpec = null,
    environ_map: ?*const std.process.Environ.Map = null,
    on_event: OnEventFn,
    on_exit: OnExitFn,
    ctx: ?*anyopaque = null,
};

const DriverImpl = union(enum) {
    claude: claude.Claude,
    codex: codex.Codex,
    grok: grok.Grok,
    pi: pi.Pi,
    antigravity: antigravity.Antigravity,
};

pub const Session = struct {
    gpa: std.mem.Allocator,
    io: std.Io,
    g: u64,
    on_event: OnEventFn,
    on_exit: OnExitFn,
    ctx: ?*anyopaque,

    mu: std.Io.Mutex = .init,
    driver_arena: std.heap.ArenaAllocator,
    out: proto.Out,
    impl: DriverImpl,
    child: std.process.Child,
    stopping: std.atomic.Value(bool) = .init(false),
    stdout_done: std.atomic.Value(bool) = .init(false),

    stderr_mu: std.Io.Mutex = .init,
    stderr_buf: [16 * 1024]u8 = undefined,
    stderr_len: usize = 0,

    stdout_thread: ?std.Thread = null,
    stderr_thread: ?std.Thread = null,

    pub fn spawn(gpa: std.mem.Allocator, io: std.Io, cfg: Config) !*Session {
        const self = try gpa.create(Session);
        errdefer gpa.destroy(self);

        self.* = .{
            .gpa = gpa,
            .io = io,
            .g = cfg.g,
            .on_event = cfg.on_event,
            .on_exit = cfg.on_exit,
            .ctx = cfg.ctx,
            .driver_arena = std.heap.ArenaAllocator.init(gpa),
            .out = undefined,
            .impl = undefined,
            .child = undefined,
        };
        errdefer self.driver_arena.deinit();
        const a = self.driver_arena.allocator();
        self.out = proto.Out.init(a, cfg.g);

        const s_spec: proto.SpawnSpec = .{
            .harness = cfg.harness,
            .program = cfg.program,
            .cwd = cfg.cwd,
            .g = cfg.g,
            .model = cfg.model,
            .provider = cfg.provider,
            .effort = cfg.effort,
            .resume_id = cfg.resume_id,
            .session_id = cfg.session_id,
            .permission = cfg.permission,
            .extra_args = cfg.extra_args,
        };

        const raw_argv = switch (cfg.harness) {
            .claude => try claude.Claude.build_argv(a, s_spec),
            .codex => try codex.Codex.build_argv(a, s_spec),
            .grok => try grok.Grok.build_argv(a, s_spec),
            .pi_vendor, .pi_local => try pi.Pi.build_argv(a, s_spec),
            .antigravity => try antigravity.Antigravity.build_argv(a, s_spec),
        };

        const final_argv = switch (cfg.sandbox_mode) {
            .bwrap => if (builtin.os.tag == .linux and cfg.sandbox != null)
                try spec.build_bwrap_argv(a, cfg.sandbox.?, raw_argv)
            else
                raw_argv,
            .landlock => if (builtin.os.tag == .linux and cfg.sandbox != null and cfg.guard_exe != null)
                try spec.build_guard_argv(a, cfg.guard_exe.?, cfg.sandbox.?, raw_argv)
            else
                raw_argv,
            .none => raw_argv,
        };

        self.child = try std.process.spawn(io, .{
            .argv = final_argv,
            .cwd = .{ .path = cfg.cwd },
            .environ_map = cfg.environ_map,
            .stdin = .pipe,
            .stdout = .pipe,
            .stderr = .pipe,
            .pgid = if (builtin.os.tag != .windows) 0 else null,
        });

        const sess_id = try a.dupe(u8, cfg.session_id orelse "pt-sess");
        const cwd_d = try a.dupe(u8, cfg.cwd);
        const mod_d = if (cfg.model) |m| try a.dupe(u8, m) else null;
        const prov_d = if (cfg.provider) |p| try a.dupe(u8, p) else null;
        const eff_d = if (cfg.effort) |e| try a.dupe(u8, e) else null;
        const res_d = if (cfg.resume_id) |r| try a.dupe(u8, r) else null;

        self.impl = switch (cfg.harness) {
            .claude => .{ .claude = .{
                .session_id = sess_id,
                .uuid_base = "00000000-0000-4000-8000-",
                .model = mod_d,
                .effort = eff_d,
                .supervised = cfg.permission == .supervised,
            } },
            .codex => .{ .codex = .{
                .cwd = cwd_d,
                .model = mod_d,
                .effort = eff_d,
                .resume_id = res_d,
                .supervised = cfg.permission == .supervised,
            } },
            .grok => .{ .grok = .{
                .cwd = cwd_d,
                .model = mod_d,
                .resume_id = res_d,
                .supervised = cfg.permission == .supervised,
            } },
            .pi_vendor, .pi_local => .{ .pi = .{
                .session_id = sess_id,
                .model = mod_d,
                .provider = prov_d,
                .effort = eff_d,
            } },
            .antigravity => .{ .antigravity = .{
                .session_id = sess_id,
                .model = mod_d,
                .effort = eff_d,
                .supervised = cfg.permission == .supervised,
            } },
        };
        return self;
    }

    pub fn child_id(self: *Session) ?std.process.Child.Id {
        self.mu.lockUncancelable(self.io);
        defer self.mu.unlock(self.io);
        return self.child.id;
    }

    pub fn begin(self: *Session) !void {
        try self.start_protocol();
        self.stderr_thread = try std.Thread.spawn(.{}, stderr_loop, .{self});
        self.stdout_thread = try std.Thread.spawn(.{}, stdout_loop, .{self});
    }

    fn start_protocol(self: *Session) !void {
        self.mu.lockUncancelable(self.io);
        switch (self.impl) {
            .claude => |*c| try c.start(&self.out),
            .codex => |*c| try c.start(&self.out),
            .grok => |*c| try c.start(&self.out),
            .pi => |*p| try p.start(&self.out),
            .antigravity => |*ag| try ag.start(&self.out),
        }
        const evs = try self.take_pending_locked();
        self.mu.unlock(self.io);
        defer self.gpa.free(evs);
        for (evs) |ev| self.on_event(self.ctx, ev);
    }

    fn write_stdin_locked(self: *Session) !void {
        const stdin_f = self.child.stdin orelse return error.Closed;
        var wbuf: [4096]u8 = undefined;
        var w = stdin_f.writer(self.io, &wbuf);
        for (self.out.writes.items) |wr| {
            try w.interface.writeAll(wr.json_line);
            try w.interface.writeByte('\n');
        }
        try w.interface.flush();
        self.out.writes.clearRetainingCapacity();
    }

    fn take_pending_locked(self: *Session) ![]proto.DriverEvent {
        try self.write_stdin_locked();
        const items = try self.gpa.dupe(proto.DriverEvent, self.out.events.items);
        self.out.events.clearRetainingCapacity();
        return items;
    }

    pub fn send_user(self: *Session, request_id: []const u8, text: []const u8, steer: ?[]const u8) !void {
        const evs = blk: {
            self.mu.lockUncancelable(self.io);
            defer self.mu.unlock(self.io);
            self.out.now_ms = std.Io.Clock.real.now(self.io).toMilliseconds();
            switch (self.impl) {
                .claude => |*c| try c.send_user(request_id, text, steer, &self.out),
                .codex => |*c| try c.send_user(request_id, text, steer, &self.out),
                .grok => |*g| try g.send_user(request_id, text, steer, &self.out),
                .pi => |*p| try p.send_user(request_id, text, steer, &self.out),
                .antigravity => |*ag| try ag.send_user(request_id, text, steer, &self.out),
            }
            break :blk try self.take_pending_locked();
        };
        defer self.gpa.free(evs);
        for (evs) |ev| self.on_event(self.ctx, ev);
    }

    pub fn interrupt(self: *Session) !void {
        const evs = blk: {
            self.mu.lockUncancelable(self.io);
            defer self.mu.unlock(self.io);
            self.out.now_ms = std.Io.Clock.real.now(self.io).toMilliseconds();
            switch (self.impl) {
                .claude => |*c| try c.interrupt(&self.out),
                .codex => |*c| try c.interrupt(&self.out),
                .grok => |*g| try g.interrupt(&self.out),
                .pi => |*p| try p.interrupt(&self.out),
                .antigravity => |*ag| {
                    try ag.interrupt(&self.out);
                    if (comptime builtin.os.tag != .windows) {
                        if (self.child.id) |pid| {
                            std.posix.kill(-pid, .INT) catch {};
                            std.posix.kill(pid, .INT) catch {};
                        }
                    }
                },
            }
            break :blk try self.take_pending_locked();
        };
        defer self.gpa.free(evs);
        for (evs) |ev| self.on_event(self.ctx, ev);
    }

    pub fn respond_approval(self: *Session, id: []const u8, decision: proto.Decision) !void {
        const evs = blk: {
            self.mu.lockUncancelable(self.io);
            defer self.mu.unlock(self.io);
            self.out.now_ms = std.Io.Clock.real.now(self.io).toMilliseconds();
            switch (self.impl) {
                .claude => |*c| try c.respond_approval(id, decision, &self.out),
                .codex => |*c| try c.respond_approval(id, decision, &self.out),
                .grok => |*g| try g.respond_approval(id, decision, &self.out),
                .pi => |*p| try p.respond_approval(id, decision, &self.out),
                .antigravity => |*ag| try ag.respond_approval(id, decision, &self.out),
            }
            break :blk try self.take_pending_locked();
        };
        defer self.gpa.free(evs);
        for (evs) |ev| self.on_event(self.ctx, ev);
    }

    pub fn compact(self: *Session) !void {
        const evs = blk: {
            self.mu.lockUncancelable(self.io);
            defer self.mu.unlock(self.io);
            self.out.now_ms = std.Io.Clock.real.now(self.io).toMilliseconds();
            switch (self.impl) {
                .claude => |*c| try c.compact(&self.out),
                .pi => |*p| try p.compact(&self.out),
                .codex, .grok, .antigravity => return error.Unsupported,
            }
            break :blk try self.take_pending_locked();
        };
        defer self.gpa.free(evs);
        for (evs) |ev| self.on_event(self.ctx, ev);
    }

    pub fn kill_and_destroy(self: *Session) void {
        self.stopping.store(true, .release);
        self.mu.lockUncancelable(self.io);
        if (self.child.stdin) |f| {
            f.close(self.io);
            self.child.stdin = null;
        }
        if (builtin.os.tag != .windows) {
            if (self.child.id) |pid| {
                std.posix.kill(-pid, .KILL) catch {};
            }
        }
        self.child.kill(self.io);
        self.mu.unlock(self.io);

        if (self.stdout_thread) |t| t.join();
        if (self.stderr_thread) |t| t.join();
        self.driver_arena.deinit();
        self.gpa.destroy(self);
    }

    fn stdout_loop(self: *Session) void {
        defer self.stdout_done.store(true, .release);
        const stdout_f = self.child.stdout orelse return;
        var rbuf: [16 * 1024]u8 = undefined;
        var r = stdout_f.reader(self.io, &rbuf);
        var line_aw: std.Io.Writer.Allocating = .init(self.gpa);
        defer line_aw.deinit();

        while (!self.stopping.load(.acquire)) {
            line_aw.clearRetainingCapacity();
            _ = r.interface.streamDelimiterLimit(&line_aw.writer, '\n', .limited(16 << 20)) catch break;
            const at_eof = blk: {
                const b = r.interface.peekByte() catch |err| switch (err) {
                    error.EndOfStream => break :blk true,
                    else => break,
                };
                if (b == '\n') r.interface.toss(1);
                break :blk false;
            };
            const line = std.mem.trim(u8, line_aw.written(), "\r");
            if (line.len > 0) {
                const evs = self.process_line(line) catch &.{};
                if (evs.len > 0) {
                    defer self.gpa.free(evs);
                    for (evs) |ev| self.on_event(self.ctx, ev);
                }
            }
            if (at_eof) break;
        }

        if (!self.stopping.load(.acquire)) {
            const term = self.child.wait(self.io) catch null;
            const code: ?u8 = if (term) |t| switch (t) {
                .exited => |c| c,
                else => 1,
            } else null;
            if (self.stderr_thread) |t| {
                t.join();
                self.stderr_thread = null;
            }
            var tail_copy: [16 * 1024]u8 = undefined;
            self.stderr_mu.lockUncancelable(self.io);
            const n = self.stderr_len;
            @memcpy(tail_copy[0..n], self.stderr_buf[0..n]);
            self.stderr_mu.unlock(self.io);
            self.on_exit(self.ctx, self.g, code, tail_copy[0..n]);
        }
    }

    fn process_line(self: *Session, line: []const u8) ![]proto.DriverEvent {
        self.mu.lockUncancelable(self.io);
        defer self.mu.unlock(self.io);
        self.out.now_ms = std.Io.Clock.real.now(self.io).toMilliseconds();
        const a = self.driver_arena.allocator();
        switch (self.impl) {
            .claude => |*c| try c.on_line(a, line, &self.out),
            .codex => |*c| try c.on_line(a, line, &self.out),
            .grok => |*g| try g.on_line(a, line, &self.out),
            .pi => |*p| try p.on_line(a, line, &self.out),
            .antigravity => |*ag| try ag.on_line(a, line, &self.out),
        }
        return self.take_pending_locked();
    }

    fn stderr_loop(self: *Session) void {
        const stderr_f = self.child.stderr orelse return;
        var rbuf: [4096]u8 = undefined;
        var r = stderr_f.reader(self.io, &rbuf);
        var tmp: [2048]u8 = undefined;
        while (!self.stopping.load(.acquire)) {
            const n = r.interface.readSliceShort(&tmp) catch break;
            if (n == 0) break;
            self.stderr_mu.lockUncancelable(self.io);
            const keep = @min(n, self.stderr_buf.len);
            const incoming = tmp[n - keep .. n];
            if (self.stderr_len + incoming.len <= self.stderr_buf.len) {
                @memcpy(self.stderr_buf[self.stderr_len .. self.stderr_len + incoming.len], incoming);
                self.stderr_len += incoming.len;
            } else {
                const shift = (self.stderr_len + incoming.len) - self.stderr_buf.len;
                const rem = self.stderr_len - shift;
                @memmove(self.stderr_buf[0..rem], self.stderr_buf[shift..self.stderr_len]);
                @memcpy(self.stderr_buf[rem..], incoming);
                self.stderr_len = self.stderr_buf.len;
            }
            self.stderr_mu.unlock(self.io);
        }
    }
};
