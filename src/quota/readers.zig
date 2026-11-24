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
    env: ?*const std.process.Environ.Map,
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

    var first_local_key: []const u8 = "";

    if (user_home) |home| {
        if (try read_json_rel(arena, io, home, ".claude/.credentials.json")) |v| {
            if (json_util.value_get(v, "claudeAiOauth")) |oauth| {
                if (json_util.value_nonempty_str(oauth, "accessToken")) |tok| {
                    claude_ok = true;
                    const plan = json_util.value_str(oauth, "subscriptionType") orelse "pro";
                    claude_q.plan = try arena.dupe(u8, plan);
                    const exp_ms = json_util.value_i64(oauth, "expiresAt") orelse 0;
                    if (exp_ms > 0 and now_ms >= exp_ms) {
                        claude_q.reading = .@"error";
                        claude_q.note = "oauth token expired";
                    } else {
                        claude_q.reading = .ok;
                        claude_q.note = "credentials present";
                        if (poll_remote) {
                            poll_claude(arena, io, tok, &claude_q, now_ms);
                        }
                    }
                }
            }
        }

        if (try read_json_rel(arena, io, home, ".codex/auth.json")) |v| {
            var account_id: []const u8 = "";
            var from_tokens = false;
            const tok = blk: {
                if (json_util.value_get(v, "tokens")) |t| {
                    if (json_util.value_nonempty_str(t, "access_token")) |s| {
                        from_tokens = true;
                        account_id = json_util.value_str(t, "account_id") orelse "";
                        break :blk s;
                    }
                }
                break :blk json_util.value_nonempty_str(v, "OPENAI_API_KEY");
            };
            if (tok) |access| {
                codex_ok = true;
                codex_q.plan = "plus";
                codex_q.reading = .ok;
                codex_q.note = "credentials present";
                if (poll_remote and from_tokens) {
                    poll_codex(arena, io, access, account_id, &codex_q, now_ms);
                }
            }
        }

        if (try read_json_rel(arena, io, home, ".grok/auth.json")) |v| {
            if (pick_grok_login(v)) |gl| {
                grok_ok = true;
                grok_q.plan = "grok";
                if (gl.expires_at_ms != null and now_ms >= gl.expires_at_ms.?) {
                    grok_q.reading = .@"error";
                    grok_q.note = "grok login expired";
                } else {
                    grok_q.reading = .ok;
                    grok_q.note = "credentials present";
                    if (poll_remote) {
                        poll_grok(arena, io, gl.key, &grok_q, now_ms);
                    }
                }
            }
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
                        if (first_local_key.len == 0 and base.len > 0) {
                            first_local_key = json_util.value_str(p_val, "apiKey") orelse "";
                        }
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
                if (poll_remote) {
                    poll_local(arena, io, env, pi_models.items, first_local_key, &local_q, now_ms);
                }
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

const GrokLogin = struct {
    key: []const u8,
    expires_at_ms: ?i64 = null,
};

fn pick_grok_login(v: std.json.Value) ?GrokLogin {
    if (v != .object) return null;
    const prefixes = [_][]const u8{ "https://auth.x.ai::", "https://accounts.x.ai/sign-in" };
    for (prefixes) |pref| {
        var it = v.object.iterator();
        while (it.next()) |e| {
            if (!std.mem.startsWith(u8, e.key_ptr.*, pref)) continue;
            const entry = e.value_ptr.*;
            const key = json_util.value_nonempty_str(entry, "key") orelse continue;
            const exp = if (json_util.value_str(entry, "expires_at")) |s| parse_rfc3339_ms(s) else null;
            return .{ .key = key, .expires_at_ms = exp };
        }
    }
    return null;
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
            .{ .name = "user-agent", .value = "protectorate" },
        },
        .response_writer = &out.writer,
    }) catch |err| {
        out_q.reading = .@"error";
        out_q.note = @errorName(err);
        return;
    };
    if (res.status == .unauthorized or res.status == .forbidden) {
        out_q.reading = .@"error";
        out_q.note = "oauth unauthorized";
        return;
    }
    if (res.status != .ok) {
        out_q.reading = .stale;
        out_q.note = std.fmt.allocPrint(arena, "oauth usage http {d}", .{@intFromEnum(res.status)}) catch "oauth usage http error";
        return;
    }
    const v = std.json.parseFromSliceLeaky(std.json.Value, arena, out.written(), .{}) catch {
        out_q.reading = .@"error";
        out_q.note = "bad usage json";
        return;
    };
    var wins: std.ArrayList(select.Window) = .empty;
    const pairs = [_]struct { field: []const u8, label: []const u8, len_ms: i64 }{
        .{ .field = "five_hour", .label = "5h", .len_ms = 5 * select.hour_ms },
        .{ .field = "seven_day", .label = "wk", .len_ms = 7 * 24 * select.hour_ms },
        .{ .field = "seven_day_sonnet", .label = "wk sonnet", .len_ms = 7 * 24 * select.hour_ms },
        .{ .field = "seven_day_opus", .label = "wk opus", .len_ms = 7 * 24 * select.hour_ms },
    };
    for (pairs) |pair| {
        if (json_util.value_object(v, pair.field)) |wobj| {
            if (json_util.value_f64(wobj, "utilization")) |u| {
                if (std.math.isFinite(u) and u >= 0.0) {
                    const resets = if (json_util.value_str(wobj, "resets_at")) |rs| parse_rfc3339_ms(rs) else null;
                    wins.append(arena, .{
                        .label = pair.label,
                        .used_pct = u,
                        .resets_at_ms = resets,
                        .length_ms = pair.len_ms,
                    }) catch {};
                }
            }
        }
    }
    if (json_util.value_array(v, "limits")) |limits| {
        for (limits) |lim| {
            const kind = json_util.value_str(lim, "kind") orelse "";
            if (!std.mem.eql(u8, kind, "weekly_scoped")) continue;
            const scope = json_util.value_object(lim, "scope") orelse continue;
            const mod = json_util.value_object(scope, "model") orelse continue;
            const dname = json_util.value_nonempty_str(mod, "display_name") orelse continue;
            const pct = json_util.value_f64(lim, "percent") orelse continue;
            if (!std.math.isFinite(pct) or pct < 0.0) continue;
            const lower_name = ascii_lower_alloc(arena, dname) catch continue;
            const lbl = std.fmt.allocPrint(arena, "wk {s}", .{lower_name}) catch continue;
            var dup = false;
            for (wins.items) |w_prev| {
                if (std.mem.eql(u8, w_prev.label, lbl)) {
                    dup = true;
                    break;
                }
            }
            if (dup) continue;
            const resets = if (json_util.value_str(lim, "resets_at")) |rs| parse_rfc3339_ms(rs) else null;
            wins.append(arena, .{
                .label = lbl,
                .used_pct = pct,
                .resets_at_ms = resets,
                .length_ms = 7 * 24 * select.hour_ms,
            }) catch {};
        }
    }
    out_q.windows = wins.items;
    out_q.fetched_at_ms = now_ms;
    if (wins.items.len == 0) {
        out_q.reading = .@"error";
        out_q.note = "no usage windows in response";
    } else {
        out_q.reading = .ok;
        out_q.note = "";
    }
}

fn poll_codex(
    arena: std.mem.Allocator,
    io: std.Io,
    token: []const u8,
    account_id: []const u8,
    out_q: *select.Quota,
    now_ms: i64,
) void {
    const auth = std.fmt.allocPrint(arena, "Bearer {s}", .{token}) catch return;
    var hdrs_buf: [4]std.http.Header = undefined;
    var hdr_len: usize = 0;
    hdrs_buf[hdr_len] = .{ .name = "authorization", .value = auth };
    hdr_len += 1;
    hdrs_buf[hdr_len] = .{ .name = "accept", .value = "application/json" };
    hdr_len += 1;
    hdrs_buf[hdr_len] = .{ .name = "user-agent", .value = "protectorate" };
    hdr_len += 1;
    if (account_id.len > 0) {
        hdrs_buf[hdr_len] = .{ .name = "chatgpt-account-id", .value = account_id };
        hdr_len += 1;
    }

    var client: std.http.Client = .{ .allocator = arena, .io = io };
    defer client.deinit();
    var out: std.Io.Writer.Allocating = .init(arena);
    const res = client.fetch(.{
        .location = .{ .url = "https://chatgpt.com/backend-api/wham/usage" },
        .method = .GET,
        .extra_headers = hdrs_buf[0..hdr_len],
        .response_writer = &out.writer,
    }) catch |err| {
        out_q.reading = .@"error";
        out_q.note = @errorName(err);
        return;
    };
    if (res.status == .unauthorized or res.status == .forbidden) {
        out_q.reading = .@"error";
        out_q.note = "codex login unauthorized";
        return;
    }
    if (res.status != .ok) {
        out_q.reading = .stale;
        out_q.note = std.fmt.allocPrint(arena, "wham usage http {d}", .{@intFromEnum(res.status)}) catch "wham usage http error";
        return;
    }
    const v = std.json.parseFromSliceLeaky(std.json.Value, arena, out.written(), .{}) catch {
        out_q.reading = .@"error";
        out_q.note = "bad usage json";
        return;
    };
    if (json_util.value_nonempty_str(v, "plan_type")) |pt| {
        out_q.plan = arena.dupe(u8, pt) catch out_q.plan;
    }

    const rl = json_util.value_object(v, "rate_limit");
    const cr = json_util.value_object(v, "credits");
    const rrc = json_util.value_object(v, "rate_limit_reset_credits");
    if (rl != null or cr != null or rrc != null) {
        var sc: select.Credits = .{};
        if (rl) |rlo| {
            sc.allowed = json_util.value_bool(rlo, "allowed");
            sc.limit_reached = json_util.value_bool(rlo, "limit_reached");
        }
        if (cr) |cro| {
            sc.has_credits = json_util.value_bool(cro, "has_credits");
            sc.overage_limit_reached = json_util.value_bool(cro, "overage_limit_reached");
            if (json_util.value_f64(cro, "balance")) |bal| {
                if (std.math.isFinite(bal) and bal >= 0.0) sc.balance = bal;
            }
        }
        if (rrc) |rrco| {
            if (json_util.value_u64(rrco, "available_count")) |cnt| {
                sc.reset_available_count = @intCast(@min(cnt, std.math.maxInt(u32)));
            }
        }
        out_q.credits = sc;
    }

    var wins: std.ArrayList(select.Window) = .empty;
    if (rl) |rlo| {
        for ([_][]const u8{ "primary_window", "secondary_window" }) |key| {
            const wobj = json_util.value_object(rlo, key) orelse continue;
            const pct = json_util.value_f64(wobj, "used_percent") orelse continue;
            if (!std.math.isFinite(pct) or pct < 0.0) continue;
            const secs = json_util.value_i64(wobj, "limit_window_seconds") orelse 5 * 3600;
            if (secs <= 0) continue;
            const reset_s = json_util.value_i64(wobj, "reset_at") orelse 0;
            const resets_ms: ?i64 = if (reset_s > 0) reset_s *| 1000 else null;
            const lbl = window_label_from_secs(arena, secs);
            wins.append(arena, .{
                .label = lbl,
                .used_pct = pct,
                .resets_at_ms = resets_ms,
                .length_ms = secs *| 1000,
            }) catch {};
        }
    }
    out_q.windows = wins.items;
    out_q.fetched_at_ms = now_ms;
    if (wins.items.len == 0) {
        out_q.reading = .@"error";
        out_q.note = "no rate limit windows";
    } else {
        out_q.reading = .ok;
        out_q.note = "";
    }
}

fn poll_grok(
    arena: std.mem.Allocator,
    io: std.Io,
    key: []const u8,
    out_q: *select.Quota,
    now_ms: i64,
) void {
    const auth = std.fmt.allocPrint(arena, "Bearer {s}", .{key}) catch return;
    var client: std.http.Client = .{ .allocator = arena, .io = io };
    defer client.deinit();
    var out: std.Io.Writer.Allocating = .init(arena);
    const res = client.fetch(.{
        .location = .{ .url = "https://cli-chat-proxy.grok.com/v1/billing?format=credits" },
        .method = .GET,
        .extra_headers = &.{
            .{ .name = "authorization", .value = auth },
            .{ .name = "x-xai-token-auth", .value = "xai-grok-cli" },
            .{ .name = "accept", .value = "application/json" },
            .{ .name = "user-agent", .value = "protectorate" },
        },
        .response_writer = &out.writer,
    }) catch |err| {
        out_q.reading = .@"error";
        out_q.note = @errorName(err);
        return;
    };
    if (res.status == .unauthorized or res.status == .forbidden) {
        out_q.reading = .@"error";
        out_q.note = "grok login expired (401)";
        return;
    }
    if (res.status != .ok) {
        out_q.reading = .stale;
        out_q.note = std.fmt.allocPrint(arena, "grok billing http {d}", .{@intFromEnum(res.status)}) catch "grok billing http error";
        return;
    }
    const v = std.json.parseFromSliceLeaky(std.json.Value, arena, out.written(), .{}) catch {
        out_q.reading = .@"error";
        out_q.note = "bad billing json";
        return;
    };
    const cfg = json_util.value_object(v, "config") orelse {
        out_q.reading = .@"error";
        out_q.note = "billing missing config";
        return;
    };
    if (json_util.value_nonempty_str(v, "subscriptionTier") orelse json_util.value_nonempty_str(cfg, "subscriptionTier")) |tier| {
        out_q.plan = arena.dupe(u8, tier) catch out_q.plan;
    }
    const start_ms = if (json_util.value_object(cfg, "currentPeriod")) |cp|
        (if (json_util.value_str(cp, "start")) |s| parse_rfc3339_ms(s) else null)
    else if (json_util.value_str(cfg, "billingPeriodStart")) |s|
        parse_rfc3339_ms(s)
    else
        null;
    const end_ms = if (json_util.value_object(cfg, "currentPeriod")) |cp|
        (if (json_util.value_str(cp, "end")) |s| parse_rfc3339_ms(s) else null)
    else if (json_util.value_str(cfg, "billingPeriodEnd")) |s|
        parse_rfc3339_ms(s)
    else
        null;

    var pct_opt = json_util.value_f64(cfg, "creditUsagePercent");
    if (pct_opt == null) {
        if (json_util.value_object(cfg, "onDemandUsed")) |uo| {
            if (json_util.value_object(cfg, "onDemandCap")) |co| {
                const used = json_util.value_f64(uo, "val");
                const cap = json_util.value_f64(co, "val");
                if (used != null and cap != null and cap.? > 0.0) {
                    pct_opt = used.? / cap.? * 100.0;
                }
            }
        }
    }
    const pct = pct_opt orelse {
        out_q.reading = .@"error";
        out_q.note = "billing missing usage percent";
        return;
    };
    const lbl = grok_period_label(start_ms, end_ms);
    const len_ms: ?i64 = if (start_ms != null and end_ms != null and end_ms.? > start_ms.?) end_ms.? - start_ms.? else null;
    const wins = arena.alloc(select.Window, 1) catch return;
    wins[0] = .{
        .label = lbl,
        .used_pct = pct,
        .resets_at_ms = end_ms,
        .length_ms = len_ms,
    };
    out_q.windows = wins;
    out_q.fetched_at_ms = now_ms;
    out_q.reading = .ok;
    out_q.note = "";
}

fn poll_local(
    arena: std.mem.Allocator,
    io: std.Io,
    env: ?*const std.process.Environ.Map,
    pi_models: []const PiModel,
    raw_api_key: []const u8,
    out_q: *select.Quota,
    now_ms: i64,
) void {
    var base: []const u8 = "";
    for (pi_models) |m| {
        if (m.base_url.len > 0) {
            base = m.base_url;
            break;
        }
    }
    if (base.len == 0) return;
    if (raw_api_key.len > 0 and raw_api_key[0] == '!') {
        out_q.reading = .stale;
        out_q.note = "apiKey uses command; health check skipped";
        return;
    }
    const trimmed = std.mem.trimEnd(u8, base, "/");
    const url = if (std.mem.endsWith(u8, trimmed, "/v1"))
        std.fmt.allocPrint(arena, "{s}/models", .{trimmed}) catch return
    else
        std.fmt.allocPrint(arena, "{s}/v1/models", .{trimmed}) catch return;

    var hdrs_buf: [2]std.http.Header = undefined;
    var hdr_len: usize = 0;
    hdrs_buf[hdr_len] = .{ .name = "accept", .value = "application/json" };
    hdr_len += 1;
    if (raw_api_key.len > 0) {
        const resolved_k = if (env) |e| (e.get(raw_api_key) orelse raw_api_key) else raw_api_key;
        const auth = std.fmt.allocPrint(arena, "Bearer {s}", .{resolved_k}) catch return;
        hdrs_buf[hdr_len] = .{ .name = "authorization", .value = auth };
        hdr_len += 1;
    }

    const t0 = std.Io.Clock.awake.now(io).toMilliseconds();
    var client: std.http.Client = .{ .allocator = arena, .io = io };
    defer client.deinit();
    var out: std.Io.Writer.Allocating = .init(arena);
    const res = client.fetch(.{
        .location = .{ .url = url },
        .method = .GET,
        .extra_headers = hdrs_buf[0..hdr_len],
        .response_writer = &out.writer,
    }) catch |err| {
        out_q.reading = .@"error";
        out_q.fetched_at_ms = now_ms;
        out_q.note = std.fmt.allocPrint(arena, "endpoint unreachable ({s})", .{@errorName(err)}) catch "endpoint unreachable";
        return;
    };
    const lat = @max(@as(i64, 0), std.Io.Clock.awake.now(io).toMilliseconds() - t0);
    if (res.status != .ok) {
        out_q.reading = .@"error";
        out_q.fetched_at_ms = now_ms;
        out_q.note = std.fmt.allocPrint(arena, "local /models http {d}", .{@intFromEnum(res.status)}) catch "local http error";
        return;
    }
    const v = std.json.parseFromSliceLeaky(std.json.Value, arena, out.written(), .{}) catch {
        out_q.reading = .@"error";
        out_q.fetched_at_ms = now_ms;
        out_q.note = "bad /models json";
        return;
    };
    const data = json_util.value_array(v, "data") orelse {
        out_q.reading = .@"error";
        out_q.fetched_at_ms = now_ms;
        out_q.note = "/models response missing data array";
        return;
    };
    var ids: std.ArrayList([]const u8) = .empty;
    for (data) |item| {
        if (json_util.value_nonempty_str(item, "id")) |mid| {
            ids.append(arena, mid) catch {};
        }
    }
    out_q.fetched_at_ms = now_ms;
    out_q.reading = .ok;
    const joined = std.mem.join(arena, ", ", ids.items) catch "";
    out_q.note = std.fmt.allocPrint(arena, "reachable in {d} ms; models: {s}", .{ lat, if (joined.len > 0) joined else "none" }) catch "reachable";
}

fn grok_period_label(start_ms: ?i64, end_ms: ?i64) []const u8 {
    if (start_ms == null or end_ms == null or end_ms.? <= start_ms.?) return "credits";
    const days = @divTrunc(end_ms.? - start_ms.?, 86_400_000);
    if (days >= 6 and days <= 8) return "wk";
    if (days >= 27 and days <= 32) return "mo";
    return "credits";
}

fn window_label_from_secs(arena: std.mem.Allocator, secs: i64) []const u8 {
    if (secs == 5 * 3600) return "5h";
    if (secs == 7 * 24 * 3600) return "wk";
    const mins = @divTrunc(secs, 60);
    if (mins >= 60 * 24 and @mod(mins, 60 * 24) == 0) {
        return std.fmt.allocPrint(arena, "{d}d", .{@divTrunc(mins, 60 * 24)}) catch "win";
    }
    if (mins >= 60 and @mod(mins, 60) == 0) {
        return std.fmt.allocPrint(arena, "{d}h", .{@divTrunc(mins, 60)}) catch "win";
    }
    return std.fmt.allocPrint(arena, "{d}m", .{@max(@as(i64, 1), mins)}) catch "win";
}

fn ascii_lower_alloc(arena: std.mem.Allocator, s: []const u8) ![]u8 {
    const out = try arena.dupe(u8, s);
    for (out) |*c| {
        if (c.* >= 'A' and c.* <= 'Z') c.* += 'a' - 'A';
    }
    return out;
}

pub fn parse_rfc3339_ms(raw: []const u8) ?i64 {
    const s = std.mem.trim(u8, raw, " \t\r\n");
    if (s.len < 20) return null;
    if (s[4] != '-' or s[7] != '-' or (s[10] != 'T' and s[10] != 't' and s[10] != ' ')) return null;
    if (s[13] != ':' or s[16] != ':') return null;

    const year = std.fmt.parseInt(i64, s[0..4], 10) catch return null;
    const month = std.fmt.parseInt(i64, s[5..7], 10) catch return null;
    const day = std.fmt.parseInt(i64, s[8..10], 10) catch return null;
    const hour = std.fmt.parseInt(i64, s[11..13], 10) catch return null;
    const min = std.fmt.parseInt(i64, s[14..16], 10) catch return null;
    const sec = std.fmt.parseInt(i64, s[17..19], 10) catch return null;
    if (month < 1 or month > 12 or day < 1 or day > 31 or hour < 0 or hour > 23 or min < 0 or min > 59 or sec < 0 or sec > 60) return null;

    var pos: usize = 19;
    var frac_ms: i64 = 0;
    if (pos < s.len and s[pos] == '.') {
        pos += 1;
        var scale: i64 = 100;
        while (pos < s.len and s[pos] >= '0' and s[pos] <= '9') : (pos += 1) {
            if (scale > 0) {
                frac_ms += @as(i64, s[pos] - '0') * scale;
                scale = @divTrunc(scale, 10);
            }
        }
    }
    if (pos >= s.len) return null;

    var tz_offset_secs: i64 = 0;
    if (s[pos] == 'Z' or s[pos] == 'z') {
        if (pos + 1 != s.len) return null;
    } else if (s[pos] == '+' or s[pos] == '-') {
        const sign: i64 = if (s[pos] == '+') 1 else -1;
        const tz = s[pos + 1 ..];
        if (tz.len == 5 and tz[2] == ':') {
            const th = std.fmt.parseInt(i64, tz[0..2], 10) catch return null;
            const tm = std.fmt.parseInt(i64, tz[3..5], 10) catch return null;
            tz_offset_secs = sign * (th * 3600 + tm * 60);
        } else if (tz.len == 4) {
            const th = std.fmt.parseInt(i64, tz[0..2], 10) catch return null;
            const tm = std.fmt.parseInt(i64, tz[2..4], 10) catch return null;
            tz_offset_secs = sign * (th * 3600 + tm * 60);
        } else return null;
    } else return null;

    const y_adj: i64 = if (month <= 2) year - 1 else year;
    const era: i64 = @divFloor(y_adj, 400);
    const yoe: i64 = y_adj - era * 400;
    const m_adj: i64 = month + (if (month > 2) @as(i64, -3) else @as(i64, 9));
    const doy: i64 = @divFloor(153 * m_adj + 2, 5) + day - 1;
    const doe: i64 = yoe * 365 + @divFloor(yoe, 4) - @divFloor(yoe, 100) + doy;
    const days: i64 = era * 146097 + doe - 719468;
    const epoch_secs: i64 = days * 86400 + hour * 3600 + min * 60 + sec - tz_offset_secs;
    return epoch_secs * 1000 + frac_ms;
}
