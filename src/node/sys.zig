const std = @import("std");
const builtin = @import("builtin");
const tiger = @import("../tiger.zig");
const version = @import("../version.zig");
const msg = @import("../net/msg.zig");

pub fn default_hostname(buf: *[256]u8) []const u8 {
    if (comptime builtin.os.tag != .windows) {
        var pbuf: [std.posix.HOST_NAME_MAX]u8 = undefined;
        if (std.posix.gethostname(&pbuf)) |hn| {
            if (hn.len > 0) {
                const n = @min(hn.len, buf.len);
                @memcpy(buf[0..n], hn[0..n]);
                return buf[0..n];
            }
        } else |_| {}
    }
    return switch (builtin.os.tag) {
        .windows => "windows-node",
        .macos => "macos-node",
        else => "linux-node",
    };
}

pub fn collect_inventory(arena: std.mem.Allocator, io: std.Io, _: []const u8) !msg.Inventory {
    var hn_buf: [256]u8 = undefined;
    const hostname = try arena.dupe(u8, default_hostname(&hn_buf));
    const os_ver = try read_os_version(arena, io);
    const cpu_model = try read_cpu_model(arena, io);
    const cpu_count: u32 = @intCast(@max(@as(usize, 1), std.Thread.getCpuCount() catch 1));
    const ram_bytes: u64 = std.process.totalSystemMemory() catch 0;

    var toolchains: std.ArrayList(msg.Toolchain) = .empty;
    const tool_names = [_][]const u8{ "git", "zig", "pi", "claude", "codex", "grok", "bwrap", "flutter" };
    for (tool_names) |tn| {
        if (probe_tool_version(arena, io, tn)) |ver| {
            try toolchains.append(arena, .{ .name = tn, .version = ver });
        }
    }

    const now_ms: i64 = std.Io.Clock.real.now(io).toMilliseconds();
    return .{
        .hostname = hostname,
        .os = @tagName(builtin.os.tag),
        .os_version = os_ver,
        .arch = @tagName(builtin.cpu.arch),
        .cpu_model = cpu_model,
        .cpu_count = cpu_count,
        .ram_bytes = ram_bytes,
        .gpus = &.{},
        .disks = &.{},
        .toolchains = toolchains.items,
        .collected_at_ms = now_ms,
    };
}

pub fn collect_sample(io: std.Io) msg.Sample {
    const now_ms: i64 = std.Io.Clock.real.now(io).toMilliseconds();
    var load1: ?f64 = null;
    var mem_avail: ?u64 = null;
    var swap_used: ?u64 = null;

    if (comptime builtin.os.tag == .linux) {
        var lbuf: [128]u8 = undefined;
        if (read_small_file(io, "/proc/loadavg", &lbuf)) |ltxt| {
            var it = std.mem.tokenizeAny(u8, ltxt, " \t\r\n");
            if (it.next()) |first| {
                load1 = std.fmt.parseFloat(f64, first) catch null;
            }
        }
        var mbuf: [4096]u8 = undefined;
        if (read_small_file(io, "/proc/meminfo", &mbuf)) |mtxt| {
            var swap_total: ?u64 = null;
            var swap_free: ?u64 = null;
            var lines = std.mem.splitScalar(u8, mtxt, '\n');
            while (lines.next()) |line| {
                if (std.mem.startsWith(u8, line, "MemAvailable:")) {
                    mem_avail = parse_kb_line(line["MemAvailable:".len..]);
                } else if (std.mem.startsWith(u8, line, "SwapTotal:")) {
                    swap_total = parse_kb_line(line["SwapTotal:".len..]);
                } else if (std.mem.startsWith(u8, line, "SwapFree:")) {
                    swap_free = parse_kb_line(line["SwapFree:".len..]);
                }
            }
            if (swap_total != null and swap_free != null and swap_total.? >= swap_free.?) {
                swap_used = swap_total.? - swap_free.?;
            }
        }
    }

    return .{
        .sampled_at_ms = now_ms,
        .load1 = load1,
        .mem_avail_bytes = mem_avail,
        .swap_used_bytes = swap_used,
    };
}

pub fn collect_presence(io: std.Io) msg.Presence {
    const now_ms: i64 = std.Io.Clock.real.now(io).toMilliseconds();
    return .{
        .state = .away,
        .source = "headless",
        .sampled_at_ms = now_ms,
    };
}

fn read_os_version(arena: std.mem.Allocator, io: std.Io) ![]const u8 {
    if (comptime builtin.os.tag == .linux) {
        var buf: [2048]u8 = undefined;
        if (read_small_file(io, "/etc/os-release", &buf)) |txt| {
            var lines = std.mem.splitScalar(u8, txt, '\n');
            while (lines.next()) |line| {
                if (std.mem.startsWith(u8, line, "PRETTY_NAME=")) {
                    const val = std.mem.trim(u8, line["PRETTY_NAME=".len..], " \t\r\"");
                    if (val.len > 0) return arena.dupe(u8, val);
                }
            }
        }
    }
    return arena.dupe(u8, @tagName(builtin.os.tag));
}

fn read_cpu_model(arena: std.mem.Allocator, io: std.Io) ![]const u8 {
    if (comptime builtin.os.tag == .linux) {
        var buf: [8192]u8 = undefined;
        if (read_small_file(io, "/proc/cpuinfo", &buf)) |txt| {
            var lines = std.mem.splitScalar(u8, txt, '\n');
            while (lines.next()) |line| {
                if (std.mem.startsWith(u8, line, "model name")) {
                    if (std.mem.findScalar(u8, line, ':')) |colon| {
                        const val = std.mem.trim(u8, line[colon + 1 ..], " \t\r");
                        if (val.len > 0) return arena.dupe(u8, val);
                    }
                }
            }
        }
    }
    return arena.dupe(u8, @tagName(builtin.cpu.arch));
}

fn read_small_file(io: std.Io, path: []const u8, buf: []u8) ?[]const u8 {
    var f = std.Io.Dir.cwd().openFile(io, path, .{}) catch return null;
    defer f.close(io);
    const n = f.readPositionalAll(io, buf, 0) catch return null;
    return buf[0..n];
}

fn parse_kb_line(tail: []const u8) ?u64 {
    var it = std.mem.tokenizeAny(u8, tail, " \t\r\n");
    const num_s = it.next() orelse return null;
    const kb = std.fmt.parseInt(u64, num_s, 10) catch return null;
    return kb *| 1024;
}

fn probe_tool_version(arena: std.mem.Allocator, io: std.Io, name: []const u8) ?[]const u8 {
    const arg = if (std.mem.eql(u8, name, "zig")) "version" else "--version";
    const res = std.process.run(arena, io, .{
        .argv = &.{ name, arg },
        .stdout_limit = .limited(2048),
        .stderr_limit = .limited(2048),
    }) catch return null;
    if (!res.term.success()) return null;
    const raw = if (res.stdout.len > 0) res.stdout else res.stderr;
    const line = std.mem.trim(u8, std.mem.sliceTo(raw, '\n'), " \t\r");
    return arena.dupe(u8, line) catch null;
}
