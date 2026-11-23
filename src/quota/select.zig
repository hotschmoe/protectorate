const std = @import("std");
const tiger = @import("../tiger.zig");

pub const stale_grace_ms: i64 = 600_000;
pub const urgent_half_hours: f64 = 48.0;
pub const unknown_reset_hours: f64 = 168.0;
pub const min_elapsed_fraction: f64 = 0.05;
pub const hour_ms: i64 = 3_600_000;
pub const unknown_window_ms: i64 = 5 * hour_ms;

pub const Subscription = enum {
    claude,
    codex,
    grok,
    local,
    api,
    unknown,
};

pub const Reading = enum {
    unknown,
    ok,
    stale,
    @"error",
};

pub const Window = struct {
    label: []const u8,
    used_pct: ?f64 = null,
    resets_at_ms: ?i64 = null,
    length_ms: ?i64 = null,
};

pub const Credits = struct {
    allowed: ?bool = null,
    limit_reached: ?bool = null,
    has_credits: ?bool = null,
    balance: ?f64 = null,
    overage_limit_reached: ?bool = null,
    reset_available_count: ?u32 = null,
    per_hour: ?f64 = null,
};

pub const Quota = struct {
    subscription: Subscription,
    plan: []const u8 = "",
    reading: Reading = .unknown,
    fetched_at_ms: i64 = 0,
    windows: []const Window = &.{},
    credits: ?Credits = null,
    note: []const u8 = "",
};

pub const Harness = enum {
    claude,
    codex,
    grok,
    pi_vendor,
    pi_local,
    antigravity,

    pub fn subscription(self: Harness) Subscription {
        return switch (self) {
            .claude => .claude,
            .codex => .codex,
            .grok => .grok,
            .pi_vendor => .api,
            .pi_local => .local,
            .antigravity => .api,
        };
    }

    pub fn is_subscribed(self: Harness) bool {
        return switch (self) {
            .claude, .codex, .grok => true,
            else => false,
        };
    }
};

pub const TaskSize = enum {
    small,
    medium,
    large,

    pub fn min_usable_pct(self: TaskSize) f64 {
        return switch (self) {
            .small => 5.0,
            .medium => 15.0,
            .large => 30.0,
        };
    }
};

pub const Candidate = struct {
    harness: Harness,
    model: ?[]const u8 = null,
    provider: ?[]const u8 = null,
    eligible: bool = true,
};

pub const Decision = struct {
    index: usize,
    candidate: Candidate,
    score: f64,
    usable_pct: f64,
    degraded: bool,
    reason: []const u8,
};

pub const SelectError = error{
    NoCandidates,
    AllIneligible,
    AllExhausted,
    NoMeteredTarget,
};

pub const Availability = enum {
    available,
    exhausted,
    unknown,
    reset,
    unmetered,
};

const families_list = [_][]const u8{ "sonnet", "opus", "haiku" };

fn fold_alnum(s: []const u8, buf: []u8) []const u8 {
    var len: usize = 0;
    for (s) |c| {
        if (std.ascii.isAlphanumeric(c)) {
            if (len < buf.len) {
                buf[len] = std.ascii.toLower(c);
                len += 1;
            }
        }
    }
    return buf[0..len];
}

fn contains_slice(haystack: []const u8, needle: []const u8) bool {
    if (needle.len == 0) return true;
    return std.mem.find(u8, haystack, needle) != null;
}

fn unspecified(model: ?[]const u8) bool {
    const m = model orelse return true;
    var buf: [128]u8 = undefined;
    const folded = fold_alnum(m, &buf);
    return folded.len == 0 or std.mem.eql(u8, folded, "default");
}

fn has_family(folded: []const u8, family: []const u8) bool {
    if (contains_slice(folded, "opusplan")) {
        return std.mem.eql(u8, family, "opus") or std.mem.eql(u8, family, "sonnet");
    }
    return contains_slice(folded, family);
}

fn any_family(folded: []const u8) bool {
    if (contains_slice(folded, "opusplan")) return true;
    for (families_list) |fam| {
        if (contains_slice(folded, fam)) return true;
    }
    return false;
}

pub fn gates(label: []const u8, model: ?[]const u8) bool {
    if (!std.mem.startsWith(u8, label, "wk ")) return true;
    const scope = std.mem.trim(u8, label[3..], " \t");
    if (scope.len == 0) return true;
    if (unspecified(model)) return false;

    var scope_buf: [128]u8 = undefined;
    const scope_folded = fold_alnum(scope, &scope_buf);
    var model_buf: [128]u8 = undefined;
    const model_folded = fold_alnum(model.?, &model_buf);

    if (!any_family(scope_folded)) {
        return scope_folded.len >= 3 and contains_slice(model_folded, scope_folded);
    }
    for (families_list) |fam| {
        if (has_family(scope_folded, fam) and has_family(model_folded, fam)) return true;
    }
    return false;
}

pub fn window_length_ms(label: []const u8) ?i64 {
    var it = std.mem.tokenizeAny(u8, label, " \t\r\n");
    while (it.next()) |tok| {
        if (std.mem.eql(u8, tok, "wk") or std.mem.eql(u8, tok, "weekly")) return 7 * 24 * hour_ms;
        if (std.mem.eql(u8, tok, "day") or std.mem.eql(u8, tok, "daily")) return 24 * hour_ms;
        if (std.mem.eql(u8, tok, "mo") or std.mem.eql(u8, tok, "monthly")) return 30 * 24 * hour_ms;
        if (tok.len >= 2) {
            const unit = tok[tok.len - 1];
            const n = std.fmt.parseInt(i64, tok[0 .. tok.len - 1], 10) catch continue;
            if (n <= 0) continue;
            switch (unit) {
                'm' => return std.math.mul(i64, n, 60_000) catch null,
                'h' => return std.math.mul(i64, n, hour_ms) catch null,
                'd' => return std.math.mul(i64, n, 24 * hour_ms) catch null,
                else => {},
            }
        }
    }
    return null;
}

fn reset_at(w: Window) ?i64 {
    const r = w.resets_at_ms orelse return null;
    return if (r > 0) r else null;
}

fn window_bound_ms(w: Window) i64 {
    if (w.length_ms) |ms| {
        if (ms > 0) return ms;
    }
    return window_length_ms(w.label) orelse unknown_window_ms;
}

fn projected_unused(w: Window, used: f64, fetched_ms: i64, now_ms: i64) ?f64 {
    const len_ms: f64 = blk: {
        if (w.length_ms) |ms| {
            if (ms > 0) break :blk @floatFromInt(ms);
        }
        const parsed = window_length_ms(w.label) orelse return null;
        break :blk @floatFromInt(parsed);
    };
    const resets = reset_at(w) orelse return null;
    if (now_ms > resets) return null;
    const start: f64 = @as(f64, @floatFromInt(resets)) - len_ms;
    const frac: f64 = (@as(f64, @floatFromInt(fetched_ms)) - start) / len_ms;
    if (frac < min_elapsed_fraction or frac > 1.0) return null;
    return std.math.clamp(100.0 - used / frac, 0.0, 100.0);
}

fn stale_limit_passed(q: *const Quota, w: Window, now_ms: i64, min_pct: ?f64, used: f64) bool {
    const min = min_pct orelse return false;
    if (q.reading != .stale or reset_at(w) != null) return false;
    const remaining = std.math.clamp(100.0 - used, 0.0, 100.0);
    return remaining < min and (now_ms -| q.fetched_at_ms) >= window_bound_ms(w);
}

fn past_blocks(q: *const Quota, now_ms: i64, min_pct: f64, model: ?[]const u8) bool {
    const fetched = q.fetched_at_ms;
    for (q.windows) |w| {
        const used = w.used_pct orelse continue;
        if (!std.math.isFinite(used)) continue;
        if (!gates(w.label, model)) continue;
        const reset = reset_at(w) orelse continue;
        const remaining = std.math.clamp(100.0 - used, 0.0, 100.0);
        if (reset > now_ms or remaining >= min_pct) continue;
        if (fetched < reset) {
            if (now_ms < reset +| stale_grace_ms) return true;
        } else {
            if (fetched < reset +| stale_grace_ms and now_ms < fetched +| stale_grace_ms) return true;
        }
    }
    return false;
}

const Scored = struct {
    score: f64,
    usable: f64,
};

fn score_quota(q: *const Quota, now_ms: i64, model: ?[]const u8, min_pct: ?f64) ?Scored {
    var min_usable: f64 = std.math.inf(f64);
    var count: usize = 0;

    for (q.windows) |w| {
        const used = w.used_pct orelse continue;
        if (!std.math.isFinite(used)) continue;
        if (!gates(w.label, model)) continue;
        if (stale_limit_passed(q, w, now_ms, min_pct, used)) continue;
        if (reset_at(w)) |r| {
            if (r <= now_ms) continue;
        }
        const rem = std.math.clamp(100.0 - used, 0.0, 100.0);
        if (rem < min_usable) min_usable = rem;
        count += 1;
    }
    if (count == 0) return null;

    var best_score: ?f64 = null;
    for (q.windows) |w| {
        const used = w.used_pct orelse continue;
        if (!std.math.isFinite(used)) continue;
        if (!gates(w.label, model)) continue;
        if (stale_limit_passed(q, w, now_ms, min_pct, used)) continue;
        if (reset_at(w)) |r| {
            if (r <= now_ms) continue;
        }
        const rem = std.math.clamp(100.0 - used, 0.0, 100.0);
        const proj = projected_unused(w, used, q.fetched_at_ms, now_ms) orelse rem;
        const expiring = @min(@min(proj, rem), min_usable);
        const hours: f64 = if (reset_at(w)) |r|
            @as(f64, @floatFromInt(r - now_ms)) / @as(f64, @floatFromInt(hour_ms))
        else
            unknown_reset_hours;
        const sc = expiring / (1.0 + hours / urgent_half_hours);
        if (best_score == null or sc > best_score.?) {
            best_score = sc;
        }
    }
    return if (best_score) |sc| .{ .score = sc, .usable = min_usable } else null;
}

fn other_scopes_only(q: *const Quota, model: ?[]const u8) bool {
    var any = false;
    for (q.windows) |w| {
        const used = w.used_pct orelse continue;
        if (!std.math.isFinite(used)) continue;
        any = true;
        if (gates(w.label, model)) return false;
    }
    return any;
}

fn counted_reset(q: *const Quota, now_ms: i64, model: ?[]const u8) bool {
    for (q.windows) |w| {
        const used = w.used_pct orelse continue;
        if (!std.math.isFinite(used)) continue;
        if (!gates(w.label, model)) continue;
        if (reset_at(w)) |r| {
            if (r <= now_ms) return true;
        }
    }
    return false;
}

fn find_quota(quotas: []const Quota, sub: Subscription) ?*const Quota {
    for (quotas) |*q| {
        if (q.subscription == sub) return q;
    }
    return null;
}

fn credits_blocked(q: *const Quota) bool {
    const cr = q.credits orelse return false;
    if (cr.limit_reached == true) return true;
    if (cr.allowed == false) return true;
    if (cr.has_credits == false and cr.overage_limit_reached == true) return true;
    return false;
}

pub fn spare_trigger(used_pct: f64, task_size: TaskSize) bool {
    return (100.0 - used_pct) >= task_size.min_usable_pct();
}

pub fn candidate_availability(
    now_ms: i64,
    size: TaskSize,
    cand: Candidate,
    quotas: []const Quota,
) Availability {
    if (cand.harness == .pi_local) {
        const q = find_quota(quotas, .local) orelse return .unknown;
        return switch (q.reading) {
            .ok => .available,
            .stale, .@"error" => .exhausted,
            .unknown => .unknown,
        };
    }
    if (!cand.harness.is_subscribed()) return .unmetered;
    const q = find_quota(quotas, cand.harness.subscription()) orelse return .unknown;
    if (q.reading == .@"error") return .exhausted;
    if (q.reading != .ok and q.reading != .stale) return .unknown;
    if (credits_blocked(q)) return .exhausted;
    if (other_scopes_only(q, cand.model)) return .unknown;
    const min = size.min_usable_pct();
    if (past_blocks(q, now_ms, min, cand.model)) return .exhausted;
    if (score_quota(q, now_ms, cand.model, min)) |s| {
        return if (s.usable < min) .exhausted else .available;
    }
    if (counted_reset(q, now_ms, cand.model)) return .reset;
    return .unknown;
}

pub fn select_target(
    candidates: []const Candidate,
    quotas: []const Quota,
    task_size: TaskSize,
    now_ms: i64,
) SelectError!Decision {
    if (candidates.len == 0) return error.NoCandidates;
    var any_eligible = false;
    var best: ?Decision = null;
    const min = task_size.min_usable_pct();

    for (candidates, 0..) |cand, i| {
        if (!cand.eligible) continue;
        any_eligible = true;

        if (cand.harness == .pi_local) {
            const q = find_quota(quotas, .local) orelse continue;
            if (q.reading != .ok) continue;
            if (best == null) {
                best = .{
                    .index = i,
                    .candidate = cand,
                    .score = 0.0,
                    .usable_pct = 100.0,
                    .degraded = false,
                    .reason = "fresh local endpoint",
                };
            }
            continue;
        }

        if (!cand.harness.is_subscribed()) continue;
        const q = find_quota(quotas, cand.harness.subscription()) orelse continue;
        if (q.reading != .ok) continue;
        if (credits_blocked(q)) continue;
        if (other_scopes_only(q, cand.model)) continue;
        if (past_blocks(q, now_ms, min, cand.model)) continue;

        const sc = score_quota(q, now_ms, cand.model, min) orelse continue;
        if (sc.usable < min) continue;

        if (best == null or sc.score > best.?.score) {
            best = .{
                .index = i,
                .candidate = cand,
                .score = sc.score,
                .usable_pct = sc.usable,
                .degraded = false,
                .reason = "highest weighted expiring spare quota",
            };
        }
    }

    if (!any_eligible) return error.AllIneligible;
    return best orelse error.AllExhausted;
}

pub fn select_with_fallback(
    candidates: []const Candidate,
    quotas: []const Quota,
    task_size: TaskSize,
    now_ms: i64,
) SelectError!Decision {
    if (select_target(candidates, quotas, task_size, now_ms)) |d| {
        return d;
    } else |err| switch (err) {
        error.NoCandidates, error.AllIneligible => return err,
        error.AllExhausted, error.NoMeteredTarget => {},
    }

    for (candidates, 0..) |cand, i| {
        if (!cand.eligible) continue;
        if (candidate_availability(now_ms, task_size, cand, quotas) == .available) {
            const min = task_size.min_usable_pct();
            const q = find_quota(quotas, cand.harness.subscription());
            const usable = if (q) |qp|
                if (score_quota(qp, now_ms, cand.model, min)) |s| s.usable else min
            else
                100.0;
            return .{
                .index = i,
                .candidate = cand,
                .score = 0.0,
                .usable_pct = usable,
                .degraded = true,
                .reason = "first eligible whose stale reading still clears minimum",
            };
        }
    }

    for (candidates, 0..) |cand, i| {
        if (!cand.eligible) continue;
        const avail = candidate_availability(now_ms, task_size, cand, quotas);
        if (avail == .unknown or avail == .reset) {
            return .{
                .index = i,
                .candidate = cand,
                .score = 0.0,
                .usable_pct = 0.0,
                .degraded = true,
                .reason = if (avail == .reset)
                    "first eligible whose quota windows have reset"
                else
                    "first eligible without a fresh quota reading",
            };
        }
    }

    var all_unmetered = true;
    for (candidates) |cand| {
        if (!cand.eligible) continue;
        if (candidate_availability(now_ms, task_size, cand, quotas) != .unmetered) {
            all_unmetered = false;
            break;
        }
    }
    if (all_unmetered) return error.NoMeteredTarget;
    return error.AllExhausted;
}
