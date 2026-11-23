const std = @import("std");
const tiger = @import("../tiger.zig");
const json_util = @import("../json_util.zig");
const select = @import("select.zig");

pub const PiModel = struct {
    provider: []const u8,
    id: []const u8,
    name: []const u8,
    base_url: []const u8 = "",
    reasoning: bool = false,
};

pub const Report = struct {
    quotas: []const select.Quota,
    pi_models: []const PiModel,
    claude_present: bool,
    codex_present: bool,
    grok_present: bool,
};

pub fn read_all(
    arena: std.mem.Allocator,
    io: std.Io,
    user_home: ?[]const u8,
    now_ms: i64,
    poll_remote: bool,
) !Report {
    var quotas: std.ArrayList(select.Quota) = .empty;
    var pi_models: std.ArrayList(PiModel) = .empty;

    var claude_q: select.Quota = .{
        .subscription = .claude,
        .reading = .unknown,
        .fetched_at_ms = now_ms,
        .note = "not configured",
    };
    var codex_q: select.Quota = .{
        .subscription = .codex,
        .reading = .unknown,
        .fetched_at_ms = now_ms,
        .note = "not configured",
    };
    var grok_q: select.Quota = .{
        .subscription = .grok,
        .reading = .unknown,
        .fetched_at_ms = now_ms,
        .note = "not configured",
    };
    var local_q: select.Quota = .{
        .subscription = .local,
        .reading = .unknown,
        .fetched_at_ms = now_ms,
        .note = "no local pi providers",
    };

    var claude_ok = false;
    var codex_ok = false;
    var grok_ok = false;

    if (user_home) |home| {
        if (try read_json_rel(arena, io, home, ".claude/.credentials.json")) |v| {
            if (json_util.value_get(v, "claudeAiOauth")) |oauth| {
                if (json_util.value_str(oauth, "accessToken")) |tok| {
                    claude_ok = true;
                    const plan = json_util.value_str(oauth, "subscriptionType") orelse "pro";
                    claude_q.plan = try arena.dupe(u8, plan);
                    claude_q.reading = .ok;
                    claude_q.note = "credentials present";
                    if (poll_remote) {
                        poll_claude(arena, io, tok, &claude_q, now_ms);
                    }
                }
            }
        }

        if (try read_json_rel(arena, io, home, ".codex/auth.json")) |v| {
            const tok = blk: {
                if (json_util.value_get(v, "tokens")) |t| {
                    if (json_util.value_str(t, "access_token")) |s| break :blk s;
                }
                break :blk json_util.value_str(v, "OPENAI_API_KEY");
            };
            if (tok) |access| {
                codex_ok = true;
                codex_q.plan = "plus";
                codex_q.reading = .ok;
                codex_q.note = "credentials present";
                if (poll_remote and json_util.value_get(v, "tokens") != null) {
                    poll_codex(arena, io, access, &codex_q, now_ms);
                }
            }
        }

        if (try read_json_rel(arena, io, home, ".grok/auth.json")) |_| {
            grok_ok = true;
            grok_q.plan = "grok";
            grok_q.reading = .ok;
            grok_q.note = "credentials present";
        } else if (try read_json_rel(arena, io, home, ".grok/user-settings.json")) |_| {
            grok_ok = true;
            grok_q.plan = "grok";
            grok_q.reading = .ok;
            grok_q.note = "settings present";
        }

        if (try read_json_rel(arena, io, home, ".pi/agent/models.json")) |v| {
            if (json_util.value_get(v, "providers")) |provs| {
                if (provs == .object) {
                    var pit = provs.object.iterator();
                    while (pit.next()) |pe| {
                        const p_name = pe.key_ptr.*;
                        const p_val = pe.value_ptr.*;
                        const base = json_util.value_str(p_val, "baseUrl") orelse "";
                        const models = json_util.value_array(p_val, "models") orelse continue;
                        for (models) |m| {
                            const mid = json_util.value_str(m, "id") orelse continue;
                            const mname = json_util.value_str(m, "name") orelse mid;
                            const reas = json_util.value_bool(m, "reasoning") == true;
                            try pi_models.append(arena, .{
                                .provider = try arena.dupe(u8, p_name),
                                .id = try arena.dupe(u8, mid),
                                .name = try arena.dupe(u8, mname),
                                .base_url = try arena.dupe(u8, base),
                                .reasoning = reas,
                            });
                        }
                    }
                }
            }
            if (pi_models.items.len > 0) {
                local_q.plan = try arena.dupe(u8, pi_models.items[0].provider);
                local_q.reading = .ok;
                local_q.note = try arena.dupe(u8, pi_models.items[0].id);
            }
        }
    }

    try quotas.append(arena, claude_q);
    try quotas.append(arena, codex_q);
    try quotas.append(arena, grok_q);
    try quotas.append(arena, local_q);

    return .{
        .quotas = quotas.items,
        .pi_models = pi_models.items,
        .claude_present = claude_ok,
        .codex_present = codex_ok,
        .grok_present = grok_ok,
    };
}

fn read_json_rel(arena: std.mem.Allocator, io: std.Io, home: []const u8, rel: []const u8) !?std.json.Value {
    const p = try std.fs.path.join(arena, &.{ home, rel });
    const raw = std.Io.Dir.cwd().readFileAlloc(io, p, arena, .limited(2 << 20)) catch return null;
    return std.json.parseFromSliceLeaky(std.json.Value, arena, raw, .{}) catch null;
}

fn poll_claude(
    arena: std.mem.Allocator,
    io: std.Io,
    token: []const u8,
    out_q: *select.Quota,
    now_ms: i64,
) void {
    const auth = std.fmt.allocPrint(arena, "Bearer {s}", .{token}) catch return;
    var client: std.http.Client = .{ .allocator = arena, .io = io };
    defer client.deinit();
    var out: std.Io.Writer.Allocating = .init(arena);
    const res = client.fetch(.{
        .location = .{ .url = "https://api.anthropic.com/api/oauth/usage" },
        .method = .GET,
        .extra_headers = &.{
            .{ .name = "authorization", .value = auth },
            .{ .name = "anthropic-beta", .value = "oauth-2025-04-20" },
            .{ .name = "accept", .value = "application/json" },
        },
        .response_writer = &out.writer,
    }) catch |err| {
        out_q.reading = .@"error";
        out_q.note = @errorName(err);
        return;
    };
    if (res.status != .ok) {
        out_q.reading = .stale;
        out_q.note = "oauth usage http error";
        return;
    }
    const v = std.json.parseFromSliceLeaky(std.json.Value, arena, out.written(), .{}) catch {
        out_q.reading = .@"error";
        return;
    };
    var wins: std.ArrayList(select.Window) = .empty;
    const pairs = [_]struct { field: []const u8, label: []const u8, len_ms: i64 }{
        .{ .field = "five_hour", .label = "5h", .len_ms = 5 * select.hour_ms },
        .{ .field = "seven_day", .label = "7d", .len_ms = 7 * 24 * select.hour_ms },
        .{ .field = "seven_day_sonnet", .label = "7d_sonnet", .len_ms = 7 * 24 * select.hour_ms },
    };
    for (pairs) |pair| {
        if (json_util.value_get(v, pair.field)) |wobj| {
            if (json_util.value_f64(wobj, "utilization")) |u| {
                wins.append(arena, .{
                    .label = pair.label,
                    .used_pct = u,
                    .length_ms = pair.len_ms,
                }) catch {};
            }
        }
    }
    out_q.windows = wins.items;
    out_q.fetched_at_ms = now_ms;
    out_q.reading = .ok;
    out_q.note = "oauth live";
}

fn poll_codex(
    arena: std.mem.Allocator,
    io: std.Io,
    token: []const u8,
    out_q: *select.Quota,
    now_ms: i64,
) void {
    const auth = std.fmt.allocPrint(arena, "Bearer {s}", .{token}) catch return;
    var client: std.http.Client = .{ .allocator = arena, .io = io };
    defer client.deinit();
    var out: std.Io.Writer.Allocating = .init(arena);
    const res = client.fetch(.{
        .location = .{ .url = "https://chatgpt.com/backend-api/wham/usage" },
        .method = .GET,
        .extra_headers = &.{
            .{ .name = "authorization", .value = auth },
            .{ .name = "accept", .value = "application/json" },
        },
        .response_writer = &out.writer,
    }) catch |err| {
        out_q.reading = .@"error";
        out_q.note = @errorName(err);
        return;
    };
    if (res.status != .ok) {
        out_q.reading = .stale;
        out_q.note = "wham usage http error";
        return;
    }
    const v = std.json.parseFromSliceLeaky(std.json.Value, arena, out.written(), .{}) catch {
        out_q.reading = .@"error";
        return;
    };
    var wins: std.ArrayList(select.Window) = .empty;
    if (json_util.value_get(v, "rate_limit")) |rl| {
        if (json_util.value_get(rl, "primary_window")) |pw| {
            if (json_util.value_f64(pw, "used_percent")) |u| {
                wins.append(arena, .{ .label = "5h", .used_pct = u, .length_ms = 5 * select.hour_ms }) catch {};
            }
        }
        if (json_util.value_get(rl, "secondary_window")) |sw| {
            if (json_util.value_f64(sw, "used_percent")) |u| {
                wins.append(arena, .{ .label = "7d", .used_pct = u, .length_ms = 7 * 24 * select.hour_ms }) catch {};
            }
        }
    }
    out_q.windows = wins.items;
    out_q.fetched_at_ms = now_ms;
    out_q.reading = .ok;
    out_q.note = "wham live";
}
