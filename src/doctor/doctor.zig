const std = @import("std");
const builtin = @import("builtin");
const tiger = @import("../tiger.zig");
const schema = @import("../api/schema.zig");

pub fn run_checks(
    arena: std.mem.Allocator,
    io: std.Io,
    user_home: ?[]const u8,
) !schema.DoctorReport {
    const now_ms: i64 = std.Io.Clock.real.now(io).toMilliseconds();

    var harnesses: std.ArrayList(schema.DoctorHarness) = .empty;
    var problems: std.ArrayList([]const u8) = .empty;

    const specs = [_]struct { name: []const u8, cred_rel: []const u8, install: []const u8 }{
        .{ .name = "claude", .cred_rel = ".claude/.credentials.json", .install = "npm i -g @anthropic-ai/claude-code" },
        .{ .name = "codex", .cred_rel = ".codex/auth.json", .install = "npm i -g @openai/codex" },
        .{ .name = "grok", .cred_rel = ".grok/auth.json", .install = "npm i -g @vibe-kit/grok-cli" },
        .{ .name = "pi", .cred_rel = ".pi/agent/models.json", .install = "npm i -g @mariozechner/pi-coding-agent" },
    };

    for (specs) |sp| {
        const ver = probe_version(arena, io, &.{ sp.name, "--version" });
        const present = ver != null;
        var login = false;
        if (user_home) |h| {
            const p = std.fs.path.join(arena, &.{ h, sp.cred_rel }) catch "";
            if (p.len > 0) {
                if (std.Io.Dir.cwd().statFile(io, p, .{})) |_| {
                    login = true;
                } else |_| {}
            }
        }
        var reasons: std.ArrayList([]const u8) = .empty;
        if (!present) try reasons.append(arena, "binary not on PATH");
        if (!login) try reasons.append(arena, "credentials not found");

        try harnesses.append(arena, .{
            .name = sp.name,
            .present = present,
            .path = if (present) sp.name else "",
            .version = ver orelse "",
            .install = sp.install,
            .login = login,
            .eligible = present and login,
            .reasons = reasons.items,
        });
    }

    const gh_ok = probe_ok(arena, io, &.{ "gh", "auth", "status" });
    const sandbox_ok = if (builtin.os.tag == .linux) probe_ok(arena, io, &.{ "bwrap", "--version" }) else false;
    if (builtin.os.tag == .linux and !sandbox_ok) {
        try problems.append(arena, "bubblewrap (bwrap) not found");
    }
    const git_ok = probe_ok(arena, io, &.{ "git", "--version" });
    if (!git_ok) {
        try problems.append(arena, "git binary not found");
    }

    return .{
        .checked_at_ms = .{ .ms = now_ms },
        .os = @tagName(builtin.os.tag),
        .arch = @tagName(builtin.cpu.arch),
        .harnesses = harnesses.items,
        .gh_logged_in = gh_ok,
        .sandbox_ok = sandbox_ok,
        .problems = problems.items,
    };
}

fn probe_version(arena: std.mem.Allocator, io: std.Io, argv: []const []const u8) ?[]const u8 {
    const res = std.process.run(arena, io, .{
        .argv = argv,
        .stdout_limit = .limited(4096),
        .stderr_limit = .limited(4096),
    }) catch return null;
    if (!res.term.success()) return null;
    const raw = if (res.stdout.len > 0) res.stdout else res.stderr;
    const first_line = std.mem.sliceTo(raw, '\n');
    return arena.dupe(u8, std.mem.trim(u8, first_line, " \t\r")) catch null;
}

fn probe_ok(arena: std.mem.Allocator, io: std.Io, argv: []const []const u8) bool {
    const res = std.process.run(arena, io, .{
        .argv = argv,
        .stdout_limit = .limited(8192),
        .stderr_limit = .limited(8192),
    }) catch return false;
    return res.term.success();
}
