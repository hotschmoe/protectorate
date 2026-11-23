const std = @import("std");
const tiger = @import("../tiger.zig");
const model = @import("../core/model.zig");
const event = @import("../core/event.zig");
const intent = @import("../core/intent.zig");
const state = @import("../core/state.zig");
const apply = @import("../core/apply.zig");
const ccodec = @import("../core/codec.zig");
const select = @import("../quota/select.zig");
const extract = @import("../history/extract.zig");
const drv_proto = @import("../drivers/protocol.zig");
const drv_claude = @import("../drivers/claude.zig");
const drv_codex = @import("../drivers/codex.zig");
const drv_grok = @import("../drivers/grok.zig");
const drv_pi = @import("../drivers/pi.zig");
const drv_agy = @import("../drivers/antigravity.zig");
const sbox = @import("../sandbox/spec.zig");
const upd_ver = @import("../update/version.zig");
const qr = @import("../qr.zig");
const schema = @import("../api/schema.zig");

const CoreWorld = struct {
    gpa: std.mem.Allocator,
    st: state.State,
    rng: std.Random,
    now_ms: i64 = 1_700_000_000_000,
    next_seq: u64 = 1,
    next_id: u64 = 1,
    log_blobs: std.ArrayList([]u8) = .empty,
    sleeve_ids: std.ArrayList([]const u8) = .empty,
    approval_ids: std.ArrayList([]const u8) = .empty,
    node_ids: std.ArrayList([]const u8) = .empty,
    job_ids: std.ArrayList([]const u8) = .empty,
    envoy_created: bool = false,

    fn init(gpa: std.mem.Allocator, rng: std.Random) CoreWorld {
        return .{
            .gpa = gpa,
            .st = state.State.init(gpa),
            .rng = rng,
        };
    }

    fn deinit(self: *CoreWorld) void {
        for (self.log_blobs.items) |b| self.gpa.free(b);
        self.log_blobs.deinit(self.gpa);
        for (self.sleeve_ids.items) |s| self.gpa.free(s);
        self.sleeve_ids.deinit(self.gpa);
        for (self.approval_ids.items) |s| self.gpa.free(s);
        self.approval_ids.deinit(self.gpa);
        for (self.node_ids.items) |s| self.gpa.free(s);
        self.node_ids.deinit(self.gpa);
        for (self.job_ids.items) |s| self.gpa.free(s);
        self.job_ids.deinit(self.gpa);
        self.st.deinit();
    }

    fn apply_checked(self: *CoreWorld, ev: event.Event, intents: *std.ArrayList(intent.Intent)) !bool {
        const se = event.StampedEvent{
            .seq = self.next_seq,
            .ts_ms = self.now_ms,
            .event = ev,
        };
        var blob: std.ArrayList(u8) = .empty;
        errdefer blob.deinit(self.gpa);
        try ccodec.encode_stamped(self.gpa, se, &blob);

        var dec_arena = std.heap.ArenaAllocator.init(self.gpa);
        defer dec_arena.deinit();
        const decoded = try ccodec.decode_stamped(dec_arena.allocator(), blob.items);
        tiger.assert(decoded.seq == se.seq and decoded.ts_ms == se.ts_ms);
        tiger.assert(std.meta.activeTag(decoded.event) == std.meta.activeTag(se.event));

        apply.apply(self.gpa, &self.st, se, intents) catch |err| switch (err) {
            error.OutOfMemory => return error.OutOfMemory,
            else => {
                blob.deinit(self.gpa);
                return false;
            },
        };
        try self.log_blobs.append(self.gpa, try blob.toOwnedSlice(self.gpa));
        self.next_seq += 1;
        check_invariants(self.gpa, &self.st);
        return true;
    }

    fn drive_intents(self: *CoreWorld, intents: []const intent.Intent) !void {
        var depth: usize = 0;
        var queue: std.ArrayList(intent.Intent) = .empty;
        defer queue.deinit(self.gpa);
        try queue.appendSlice(self.gpa, intents);

        while (queue.items.len > 0 and depth < 16) : (depth += 1) {
            const it = queue.orderedRemove(0);
            var sub_intents: std.ArrayList(intent.Intent) = .empty;
            defer sub_intents.deinit(self.gpa);
            self.now_ms += 10;

            switch (it) {
                .provision_worktree => |p| {
                    if (self.rng.uintLessThan(u8, 12) == 0) {
                        _ = try self.apply_checked(.{ .provision_failed = .{ .sleeve_id = p.sleeve_id, .reason = "sim disk full" } }, &sub_intents);
                    } else {
                        _ = try self.apply_checked(.{ .provisioned = .{ .sleeve_id = p.sleeve_id, .branch = "pt/sim" } }, &sub_intents);
                    }
                },
                .launch_incarnation => |l| {
                    if (try self.apply_checked(.{ .launched = .{
                        .sleeve_id = l.sleeve_id,
                        .g = l.g,
                        .harness = l.harness,
                        .mode = l.mode,
                        .model_name = l.model_name,
                        .provider = l.provider,
                        .effort = l.effort,
                    } }, &sub_intents)) {
                        _ = try self.apply_checked(.{ .session_bound = .{
                            .sleeve_id = l.sleeve_id,
                            .g = l.g,
                            .native_id = "sim-native-1",
                        } }, &sub_intents);
                    }
                },
                .deliver_prompt => |dp| {
                    _ = try self.apply_checked(.{ .prompt_delivered = .{ .sleeve_id = dp.sleeve_id, .g = dp.g, .request_id = dp.request_id } }, &sub_intents);
                    _ = try self.apply_checked(.{ .prompt_acked = .{ .sleeve_id = dp.sleeve_id, .g = dp.g, .request_id = dp.request_id, .confirmed = true } }, &sub_intents);
                    const is_mail = std.mem.startsWith(u8, dp.request_id, "mail-");
                    _ = try self.apply_checked(.{ .turn_started = .{ .sleeve_id = dp.sleeve_id, .g = dp.g, .request_id = dp.request_id, .from_agent_mail = is_mail } }, &sub_intents);
                },
                .kill_incarnation => |k| {
                    _ = try self.apply_checked(.{ .exited = .{
                        .sleeve_id = k.sleeve_id,
                        .g = k.g,
                        .cause = .killed,
                        .requested = true,
                    } }, &sub_intents);
                },
                .interrupt_incarnation => |k| {
                    _ = try self.apply_checked(.{ .exited = .{
                        .sleeve_id = k.sleeve_id,
                        .g = k.g,
                        .cause = .killed,
                        .requested = false,
                    } }, &sub_intents);
                },
                .dispatch_job => |dj| {
                    const attempt: u32 = if (self.st.jobs.get(dj.job_id)) |j| j.attempt else 1;
                    _ = try self.apply_checked(.{ .job_submitted = .{ .job_id = dj.job_id, .node_id = dj.node_id, .attempt = attempt } }, &sub_intents);
                },
                .verify_job => |vj| {
                    const still = self.rng.boolean();
                    _ = try self.apply_checked(.{ .job_verified = .{ .job_id = vj.job_id, .still_running = still, .exit_code = if (still) null else @as(i32, 0) } }, &sub_intents);
                },
                else => {},
            }
            try queue.appendSlice(self.gpa, sub_intents.items);
        }
    }

    fn step(self: *CoreWorld) !void {
        var intents: std.ArrayList(intent.Intent) = .empty;
        defer intents.deinit(self.gpa);

        self.now_ms += 100 + @as(i64, self.rng.uintLessThan(u32, 40_000));
        const op = self.rng.uintLessThan(u8, 16);

        switch (op) {
            0, 1 => {
                if (self.sleeve_ids.items.len >= 24) {
                    const sid = self.sleeve_ids.items[self.rng.uintLessThan(usize, self.sleeve_ids.items.len)];
                    if (self.st.sleeves.get(sid)) |sl| {
                        if (sl.lifecycle == .stopped or sl.lifecycle == .parked) {
                            const t: [model.max_targets]model.Harness = .{ .pi_local, .claude, .codex };
                            _ = try self.apply_checked(.{ .start_requested = .{
                                .sleeve_id = sid,
                                .targets = t,
                                .target_len = 3,
                                .mode = .headless,
                                .model_name = "hotschmoe-dd",
                                .provider = "hotschmoe-local",
                            } }, &intents);
                        }
                    }
                    try self.drive_intents(intents.items);
                    return;
                }
                // Create envoy once, then roots & children; occasionally test overlap & child limits.
                const id = try std.fmt.allocPrint(self.gpa, "sl-{d}", .{self.next_id});
                self.next_id += 1;
                const role: model.SleeveRole = if (!self.envoy_created) .envoy else if (self.rng.boolean() and self.sleeve_ids.items.len > 0) .child else .root;
                const parent: ?[]const u8 = if (role == .child) self.sleeve_ids.items[self.rng.uintLessThan(usize, self.sleeve_ids.items.len)] else null;
                const layout: model.Layout = switch (self.rng.uintLessThan(u8, 3)) {
                    0 => .worktree,
                    1 => .readonly,
                    else => .shared,
                };
                const path = if (self.rng.uintLessThan(u8, 6) == 0 and self.sleeve_ids.items.len > 0)
                    try self.gpa.dupe(u8, "/tmp/ws-shared")
                else
                    try std.fmt.allocPrint(self.gpa, "/tmp/ws-{s}", .{id});
                defer self.gpa.free(path);

                const ok = try self.apply_checked(.{ .sleeve_created = .{
                    .sleeve_id = id,
                    .name = id,
                    .role = role,
                    .path = path,
                    .workspace_id = "ws-1",
                    .parent_id = parent,
                    .layout = layout,
                    .budget_limit = if (role == .child) 4 else null,
                    .until_done = if (role == .child and self.rng.boolean()) 2 else 0,
                } }, &intents);
                if (ok) {
                    if (role == .envoy) self.envoy_created = true;
                    try self.sleeve_ids.append(self.gpa, id);
                    // Start newly created sleeve immediately.
                    const t: [model.max_targets]model.Harness = .{ .pi_local, .claude, .codex };
                    _ = try self.apply_checked(.{ .start_requested = .{
                        .sleeve_id = id,
                        .targets = t,
                        .target_len = 3,
                        .mode = .headless,
                        .model_name = "hotschmoe-dd",
                        .provider = "hotschmoe-local",
                    } }, &intents);
                } else {
                    self.gpa.free(id);
                }
            },
            2, 3 => {
                if (self.sleeve_ids.items.len == 0) return;
                const sid = self.sleeve_ids.items[self.rng.uintLessThan(usize, self.sleeve_ids.items.len)];
                const is_mail = self.rng.boolean();
                const req = try std.fmt.allocPrint(self.gpa, "{s}-{d}", .{ if (is_mail) "mail" else "own", self.next_id });
                defer self.gpa.free(req);
                self.next_id += 1;
                _ = try self.apply_checked(.{ .prompt_enqueued = .{
                    .sleeve_id = sid,
                    .request_id = req,
                    .from = if (is_mail) "sl-worker" else "owner",
                    .kind = if (is_mail) .mail else .prompt,
                    .mode = if (self.rng.uintLessThan(u8, 4) == 0) .steer else .queue,
                    .text = "do step work",
                    .supersede = self.rng.uintLessThan(u8, 8) == 0,
                    .until_done = if (self.rng.uintLessThan(u8, 5) == 0) 2 else null,
                } }, &intents);
            },
            4 => {
                if (self.sleeve_ids.items.len == 0) return;
                const sid = self.sleeve_ids.items[self.rng.uintLessThan(usize, self.sleeve_ids.items.len)];
                if (self.st.sleeves.get(sid)) |sl| {
                    if (sl.inc) |inc| {
                        const ctx: u64 = if (sl.role == .envoy and self.rng.uintLessThan(u8, 3) == 0) model.envoy_context_ceiling + 100 else 1200;
                        _ = try self.apply_checked(.{ .turn_settled = .{
                            .sleeve_id = sid,
                            .g = inc.g,
                            .turn_end = .end_turn,
                            .declared_status = if (self.rng.boolean()) .done else .continuing,
                            .context_tokens = ctx,
                            .context_window = 200_000,
                        } }, &intents);
                    }
                }
            },
            5 => {
                // Loop detection exercise (G6).
                if (self.sleeve_ids.items.len == 0) return;
                const sid = self.sleeve_ids.items[self.rng.uintLessThan(usize, self.sleeve_ids.items.len)];
                if (self.st.sleeves.get(sid)) |sl| {
                    if (sl.inc) |inc| {
                        _ = try self.apply_checked(.{ .tool_activity = .{
                            .sleeve_id = sid,
                            .g = inc.g,
                            .tool_sig = "bash:repeat_same",
                        } }, &intents);
                    }
                }
            },
            6 => {
                // Request or decide approval (including D9 non-owner denial test).
                if (self.sleeve_ids.items.len == 0) return;
                const sid = self.sleeve_ids.items[self.rng.uintLessThan(usize, self.sleeve_ids.items.len)];
                if (self.approval_ids.items.len > 0 and self.rng.boolean()) {
                    const aid = self.approval_ids.items[self.rng.uintLessThan(usize, self.approval_ids.items.len)];
                    // Try invalid envoy approve on D9 sometimes: oracle verifies state rejects!
                    if (self.rng.uintLessThan(u8, 4) == 0) {
                        _ = try self.apply_checked(.{ .approval_decided = .{
                            .approval_id = aid,
                            .verdict = .allow_once,
                            .by = .envoy,
                        } }, &intents);
                    } else {
                        _ = try self.apply_checked(.{ .approval_decided = .{
                            .approval_id = aid,
                            .verdict = if (self.rng.boolean()) .allow_once else .deny,
                            .by = .owner,
                        } }, &intents);
                    }
                } else if (self.st.sleeves.get(sid)) |sl| {
                    if (sl.inc) |inc| {
                        const aid = try std.fmt.allocPrint(self.gpa, "ap-{d}", .{self.next_id});
                        self.next_id += 1;
                        if (try self.apply_checked(.{ .approval_requested = .{
                            .approval_id = aid,
                            .sleeve_id = sid,
                            .g = inc.g,
                            .kind = .tool,
                            .title = "bash",
                            .summary = "run command",
                            .is_d9 = sl.d9,
                        } }, &intents)) {
                            try self.approval_ids.append(self.gpa, aid);
                        } else {
                            self.gpa.free(aid);
                        }
                    }
                }
            },
            7 => {
                // Resleeve or crash exit (G2/G3/G4).
                if (self.sleeve_ids.items.len == 0) return;
                const sid = self.sleeve_ids.items[self.rng.uintLessThan(usize, self.sleeve_ids.items.len)];
                if (self.rng.boolean()) {
                    _ = try self.apply_checked(.{ .resleeve_requested = .{
                        .sleeve_id = sid,
                        .harness = .claude,
                        .by = if (self.rng.boolean()) .rule else .owner,
                        .cause = .quota,
                    } }, &intents);
                } else if (self.st.sleeves.get(sid)) |sl| {
                    if (sl.inc) |inc| {
                        const causes = [_]model.ExitCause{ .crash, .oom, .quota_exhausted, .auth_error };
                        _ = try self.apply_checked(.{ .exited = .{
                            .sleeve_id = sid,
                            .g = inc.g,
                            .cause = causes[self.rng.uintLessThan(usize, causes.len)],
                            .requested = false,
                        } }, &intents);
                    }
                }
            },
            8 => {
                // Node registration, approval, connection, presence update.
                if (self.node_ids.items.len < 4 or self.rng.uintLessThan(u8, 3) == 0) {
                    const nid = try std.fmt.allocPrint(self.gpa, "nd-{d}", .{self.next_id});
                    self.next_id += 1;
                    var pk: [32]u8 = undefined;
                    self.rng.bytes(&pk);
                    if (try self.apply_checked(.{ .node_registered = .{ .node_id = nid, .name = nid, .pub_key = pk } }, &intents)) {
                        try self.node_ids.append(self.gpa, nid);
                        _ = try self.apply_checked(.{ .node_approved = .{ .node_id = nid, .name = nid } }, &intents);
                        _ = try self.apply_checked(.{ .node_connected = .{ .node_id = nid, .arch = "x86_64", .version = "v0.7.0" } }, &intents);
                    } else {
                        self.gpa.free(nid);
                    }
                } else {
                    const nid = self.node_ids.items[self.rng.uintLessThan(usize, self.node_ids.items.len)];
                    _ = try self.apply_checked(.{ .node_presence_updated = .{
                        .node_id = nid,
                        .presence = if (self.rng.boolean()) .away else .present,
                    } }, &intents);
                }
            },
            9, 10 => {
                // Queue or settle jobs.
                if (self.job_ids.items.len > 0 and self.rng.boolean()) {
                    const jid = self.job_ids.items[self.rng.uintLessThan(usize, self.job_ids.items.len)];
                    if (self.st.jobs.get(jid)) |job| {
                        if (job.state == .running or job.state == .submitting) {
                            if (self.rng.boolean()) {
                                _ = try self.apply_checked(.{ .job_completed = .{ .job_id = jid, .attempt = job.attempt, .exit_code = 0 } }, &intents);
                            } else {
                                _ = try self.apply_checked(.{ .job_failed = .{ .job_id = jid, .attempt = job.attempt, .err = "exit 1" } }, &intents);
                            }
                        }
                    }
                } else {
                    const jid = try std.fmt.allocPrint(self.gpa, "job-{d}", .{self.next_id});
                    self.next_id += 1;
                    if (try self.apply_checked(.{ .job_queued = .{
                        .job_id = jid,
                        .requested_by = "owner",
                        .on_present = if (self.rng.boolean()) .kill else .idle,
                        .even_if_present = self.rng.boolean(),
                    } }, &intents)) {
                        try self.job_ids.append(self.gpa, jid);
                    } else {
                        self.gpa.free(jid);
                    }
                }
            },
            11 => {
                // Advance time enough to occasionally hit job leases (5m) or silence (15m).
                self.now_ms += if (self.rng.boolean()) model.job_lease_ms + 100 else model.silent_ms + 100;
                _ = try self.apply_checked(.{ .tick = .{} }, &intents);
            },
            12 => {
                if (self.sleeve_ids.items.len > 0) {
                    const sid = self.sleeve_ids.items[self.rng.uintLessThan(usize, self.sleeve_ids.items.len)];
                    _ = try self.apply_checked(.{ .stop_requested = .{ .sleeve_id = sid, .by = .owner } }, &intents);
                }
            },
            13 => {
                if (self.sleeve_ids.items.len > 1) {
                    const sid = self.sleeve_ids.items[1 + self.rng.uintLessThan(usize, self.sleeve_ids.items.len - 1)];
                    _ = try self.apply_checked(.{ .removed = .{ .sleeve_id = sid } }, &intents);
                }
            },
            else => {
                _ = try self.apply_checked(.{ .tick = .{} }, &intents);
            },
        }

        try self.drive_intents(intents.items);
    }

    fn verify_replay_equals_live(self: *const CoreWorld) !void {
        var replayed = state.State.init(self.gpa);
        defer replayed.deinit();

        var arena = std.heap.ArenaAllocator.init(self.gpa);
        defer arena.deinit();

        var dummy_intents: std.ArrayList(intent.Intent) = .empty;
        defer dummy_intents.deinit(self.gpa);

        for (self.log_blobs.items) |blob| {
            _ = arena.reset(.retain_capacity);
            const se = try ccodec.decode_stamped(arena.allocator(), blob);
            dummy_intents.clearRetainingCapacity();
            try apply.apply(self.gpa, &replayed, se, &dummy_intents);
        }

        const h_live = try self.st.state_hash(self.gpa);
        const h_replay = try replayed.state_hash(self.gpa);
        tiger.assert_fmt(h_live == h_replay, "replay divergence: live=0x{x} replay=0x{x}", .{ h_live, h_replay });
    }
};

fn check_invariants(gpa: std.mem.Allocator, st: *const state.State) void {
    // I1 + I2: Generation monotonicity, single active writable workspace claim, D9 invariant.
    var active_writable: std.ArrayList([]const u8) = .empty;
    defer active_writable.deinit(gpa);

    var sit = st.sleeves.iterator();
    while (sit.next()) |e| {
        const sl = e.value_ptr.*;
        if (sl.inc) |inc| {
            tiger.assert(inc.g > 0 and inc.g <= sl.next_g);
            if (sl.role == .envoy and inc.activity == .working and inc.turn_is_agent_mail) {
                tiger.assert(sl.d9);
            }
        }
        if (sl.role == .envoy and (sl.inc == null or sl.inc.?.activity == .idle)) {
            tiger.assert(!sl.d9);
        }
        if (sl.lifecycle != .removed and sl.lifecycle != .stopped and sl.lifecycle != .parked and sl.layout == .worktree) {
            for (active_writable.items) |prev_p| {
                tiger.assert(!model.paths_overlap(prev_p, sl.path));
            }
            active_writable.append(gpa, sl.path) catch unreachable;
        }
    }

    // I3: No pending tool approval for a superseded generation.
    var ait = st.approvals.iterator();
    while (ait.next()) |e| {
        const app = e.value_ptr.*;
        if (app.state == .pending) {
            if (st.sleeves.get(app.sleeve_id)) |sl| {
                if (sl.inc) |inc| {
                    tiger.assert(app.g <= inc.g);
                }
            }
        }
    }

    // I5: Child counts match actual non-removed children (O(N) single pass).
    var counts: std.StringHashMapUnmanaged(u32) = .empty;
    defer counts.deinit(gpa);
    var cit = st.sleeves.iterator();
    while (cit.next()) |ce| {
        const ch = ce.value_ptr.*;
        if (ch.lifecycle != .removed) {
            if (ch.parent_id) |pid| {
                const gop = counts.getOrPut(gpa, pid) catch unreachable;
                if (gop.found_existing) {
                    gop.value_ptr.* += 1;
                } else {
                    gop.value_ptr.* = 1;
                }
            }
        }
    }
    var sit2 = st.sleeves.iterator();
    while (sit2.next()) |e| {
        const parent = e.value_ptr.*;
        const actual = counts.get(parent.id) orelse 0;
        tiger.assert(actual == parent.children_active);
        tiger.assert(actual <= model.max_children);
    }
}

fn run_quota_oracle(seed: u64) void {
    var prng = std.Random.DefaultPrng.init(seed);
    const rng = prng.random();
    const now: i64 = 1_700_000_000_000;

    // 1. Model-scoped windows: opusplan matches both wk opus and wk sonnet.
    tiger.assert(select.gates("wk opus", "opusplan"));
    tiger.assert(select.gates("wk sonnet", "opusplan"));
    tiger.assert(!select.gates("wk sonnet", "opus-4-1"));
    tiger.assert(!select.gates("wk sonnet", "default"));
    tiger.assert(!select.gates("wk sonnet", null));
    tiger.assert(select.gates("5h", null));
    tiger.assert(select.window_length_ms("5h") == 5 * select.hour_ms);
    tiger.assert(select.window_length_ms("wk sonnet") == 7 * 24 * select.hour_ms);

    // 2. Unmetered pi_vendor/antigravity never auto-selected.
    const unmetered_only = [_]select.Candidate{
        .{ .harness = .pi_vendor, .model = "gpt-5" },
        .{ .harness = .antigravity, .model = "gemini" },
    };
    const empty_q = [_]select.Quota{};
    if (select.select_with_fallback(&unmetered_only, &empty_q, .medium, now)) |_| {
        tiger.assert(false);
    } else |err| {
        tiger.assert(err == error.NoMeteredTarget);
    }

    // 3. Stale grace window blocking vs expired.
    const used = 96.0 + @as(f64, @floatFromInt(rng.uintLessThan(u8, 4)));
    const reset_past = now - 60_000; // within 600s grace window!
    const win_blocking = [_]select.Window{.{ .label = "5h", .used_pct = used, .resets_at_ms = reset_past }};
    const q_stale = [_]select.Quota{
        .{ .subscription = .claude, .reading = .stale, .fetched_at_ms = now - 120_000, .windows = &win_blocking },
        .{ .subscription = .local, .reading = .ok, .fetched_at_ms = now },
    };
    const cands = [_]select.Candidate{
        .{ .harness = .claude, .model = "sonnet" },
        .{ .harness = .pi_local, .model = "hotschmoe-dd" },
    };
    const dec = select.select_target(&cands, &q_stale, .medium, now) catch unreachable;
    tiger.assert(dec.candidate.harness == .pi_local);
}

fn run_drivers_oracle(gpa: std.mem.Allocator, seed: u64) !void {
    var prng = std.Random.DefaultPrng.init(seed);
    const rng = prng.random();

    var arena_st = std.heap.ArenaAllocator.init(gpa);
    defer arena_st.deinit();
    const a = arena_st.allocator();

    // 1. Claude handshake + delta coalescing + approval + settlement.
    {
        var cl = drv_claude.Claude{ .session_id = "s1", .uuid_base = "u-", .supervised = true };
        var out = drv_proto.Out.init(a, 1);
        try cl.start(&out);
        try cl.on_line(a, "{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"cs-1\",\"model\":\"claude-opus\"}", &out);
        try cl.send_user("req-1", "hi", null, &out);
        const sent_uuid = cl.unechoed.items[0].uuid;
        const echo = try std.fmt.allocPrint(a, "{{\"type\":\"user\",\"isReplay\":true,\"uuid\":\"{s}\",\"message\":{{\"role\":\"user\",\"content\":\"hi\"}}}}", .{sent_uuid});
        try cl.on_line(a, echo, &out);
        try cl.on_line(a, "{\"type\":\"stream_event\",\"event\":{\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hel\"}}}", &out);
        try cl.on_line(a, "{\"type\":\"stream_event\",\"event\":{\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"lo\"}}}", &out);
        try cl.on_line(a, "{\"type\":\"control_request\",\"request_id\":\"cr-1\",\"request\":{\"subtype\":\"can_use_tool\",\"tool_name\":\"Bash\",\"input\":{\"command\":\"ls\"}}}", &out);
        try cl.respond_approval("cr-1", .allow_once, &out);
        try cl.on_line(a, "{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"usage\":{\"input_tokens\":10,\"output_tokens\":2},\"duration_ms\":42}", &out);

        var saw_final_hello = false;
        for (out.events.items) |ev| {
            switch (ev.entry) {
                .assistant_text => |at| {
                    if (!at.delta and std.mem.eql(u8, at.text, "Hello")) saw_final_hello = true;
                },
                else => {},
            }
        }
        tiger.assert(saw_final_hello);
    }

    // 2. Codex initialize -> thread -> turn -> deltas -> settle.
    {
        var cx = drv_codex.Codex{ .cwd = "/tmp/ws" };
        var out = drv_proto.Out.init(a, 2);
        try cx.start(&out);
        try cx.on_line(a, "{\"id\":1,\"result\":{}}", &out);
        try cx.on_line(a, "{\"id\":2,\"result\":{\"thread\":{\"id\":\"th-1\"}}}", &out);
        try cx.send_user("r-1", "test", null, &out);
        try cx.on_line(a, "{\"id\":3,\"result\":{\"turn\":{\"id\":\"tu-1\"}}}", &out);
        try cx.on_line(a, "{\"method\":\"turn/started\",\"params\":{}}", &out);
        try cx.on_line(a, "{\"method\":\"item/agentMessage/delta\",\"params\":{\"itemId\":\"m1\",\"delta\":\"ok\"}}", &out);
        try cx.on_line(a, "{\"method\":\"turn/completed\",\"params\":{\"turn\":{\"status\":\"completed\"}}}", &out);
        tiger.assert(!cx.turn_active);
    }

    // 3. Pi RPC initialize -> prompt -> settle + malformed line resilience.
    {
        var pi = drv_pi.Pi{ .session_id = "pi-1", .provider = "hotschmoe-local" };
        var out = drv_proto.Out.init(a, 3);
        try pi.start(&out);
        try pi.on_line(a, "{\"type\":\"response\",\"id\":\"pi-1\",\"success\":true,\"data\":{\"sessionId\":\"pi-real\",\"model\":{\"id\":\"hotschmoe-dd\",\"provider\":\"hotschmoe-local\"}}}", &out);
        try pi.send_user("pr-1", "ping", null, &out);
        try pi.on_line(a, "{\"type\":\"response\",\"id\":\"pi-2\",\"success\":true}", &out);
        try pi.on_line(a, "{\"type\":\"agent_start\"}", &out);
        try pi.on_line(a, "{\"type\":\"message_start\",\"message\":{\"role\":\"assistant\"}}", &out);
        try pi.on_line(a, "{\"type\":\"message_update\",\"assistantMessageEvent\":{\"type\":\"text_delta\",\"contentIndex\":0,\"delta\":\"pong\"}}", &out);
        try pi.on_line(a, "{\"type\":\"message_end\",\"message\":{\"role\":\"assistant\",\"usage\":{\"input\":5,\"output\":1,\"totalTokens\":6}}}", &out);
        try pi.on_line(a, "{\"type\":\"agent_settled\",\"aborted\":false}", &out);
        tiger.assert(pi.turn == null);
    }

    // 4. Grok ACP + arbitrary corrupted lines across all 4 drivers.
    {
        var gr = drv_grok.Grok{ .cwd = "/tmp/ws" };
        var out = drv_proto.Out.init(a, 4);
        try gr.start(&out);
        try gr.on_line(a, "{\"id\":1,\"result\":{}}", &out);
        try gr.on_line(a, "{\"id\":2,\"result\":{\"sessionId\":\"gs-1\"}}", &out);

        const garbage = [_][]const u8{
            "",
            "{bad json",
            "[]",
            "{\"type\":123}",
            "{\"method\":null,\"id\":null}",
            "{\"type\":\"stream_event\",\"event\":{}}",
            "{\"type\":\"result\",\"is_error\":true,\"result\":\"401 unauthorized\"}",
        };
        var cl2 = drv_claude.Claude{ .session_id = "s", .uuid_base = "b" };
        var pi2 = drv_pi.Pi{ .session_id = "p" };
        var ag2 = drv_agy.Antigravity{ .session_id = "a" };
        for (0..12) |_| {
            const line = garbage[rng.uintLessThan(usize, garbage.len)];
            try cl2.on_line(a, line, &out);
            try gr.on_line(a, line, &out);
            try pi2.on_line(a, line, &out);
            try ag2.on_line(a, line, &out);
        }
    }

    // 5. Antigravity stream-json argv, multi-turn cumulative usage deltas, tools, and errors.
    {
        const ag_argv = try drv_agy.Antigravity.build_argv(a, .{
            .harness = .antigravity,
            .program = "agy",
            .cwd = "/tmp/ws",
            .model = "gemini-3.8-flash-medium",
            .effort = "medium",
            .resume_id = "conv-prev",
            .permission = .unsupervised,
        });
        var saw_in_fmt = false;
        var saw_out_fmt = false;
        for (ag_argv, 0..) |arg, idx| {
            tiger.assert(!std.mem.eql(u8, arg, "-p") and !std.mem.eql(u8, arg, "--print"));
            if (std.mem.eql(u8, arg, "--input-format") and idx + 1 < ag_argv.len) {
                saw_in_fmt = std.mem.eql(u8, ag_argv[idx + 1], "stream-json");
            }
            if (std.mem.eql(u8, arg, "--output-format") and idx + 1 < ag_argv.len) {
                saw_out_fmt = std.mem.eql(u8, ag_argv[idx + 1], "stream-json");
            }
        }
        tiger.assert(saw_in_fmt and saw_out_fmt);

        var ag = drv_agy.Antigravity{ .session_id = "ag-s1", .model = "gemini-3.8-flash-medium" };
        var out = drv_proto.Out.init(a, 5);
        try ag.start(&out);
        try ag.on_line(a, "{\"event\":\"init\",\"conversation_id\":\"conv-live\",\"init\":{\"cwd\":\"/tmp/ws\",\"model\":\"gemini-3.8-flash-medium\",\"permission_mode\":\"always-proceed\"}}", &out);
        tiger.assert(std.mem.eql(u8, ag.session_id, "conv-live"));

        try ag.send_user("ag-req-1", "edit a file", null, &out);
        tiger.assert(out.writes.items.len > 0);
        const w0 = out.writes.items[out.writes.items.len - 1].json_line;
        tiger.assert(std.mem.findPos(u8, w0, 0, "\"event\":\"user\"") != null);

        try ag.on_line(a, "{\"event\":\"step_update\",\"step_update\":{\"conversation_id\":\"conv-live\",\"step_index\":0,\"state\":\"DONE\",\"step_type\":\"user_input\"}}", &out);
        try ag.on_line(a, "{\"event\":\"step_update\",\"step_update\":{\"conversation_id\":\"conv-live\",\"step_index\":1,\"state\":\"DONE\",\"step_type\":\"tool\",\"tool_name\":\"write_to_file\",\"duration_seconds\":0.12,\"tool_info\":{\"name\":\"write_to_file\",\"parameters\":{\"TargetFile\":\"/tmp/ws/a.txt\"},\"output\":\"ok\"}}}", &out);
        try ag.on_line(a, "{\"event\":\"step_update\",\"step_update\":{\"conversation_id\":\"conv-live\",\"step_index\":2,\"state\":\"ACTIVE\",\"step_type\":\"agent_response\",\"text_delta\":\"Do\"}}", &out);
        try ag.on_line(a, "{\"event\":\"step_update\",\"step_update\":{\"conversation_id\":\"conv-live\",\"step_index\":2,\"state\":\"DONE\",\"step_type\":\"agent_response\",\"text_delta\":\"ne\"}}", &out);
        try ag.on_line(a, "{\"event\":\"step_update\",\"step_update\":{\"conversation_id\":\"conv-live\",\"step_index\":3,\"state\":\"DONE\",\"step_type\":\"checkpoint\"}}", &out);
        try ag.on_line(a, "{\"event\":\"result\",\"result\":{\"conversation_id\":\"conv-live\",\"status\":\"SUCCESS\",\"response\":\"Done\",\"duration_seconds\":1.5,\"num_turns\":1,\"usage\":{\"input_tokens\":100,\"output_tokens\":20,\"cache_read_tokens\":10,\"total_tokens\":120}}}", &out);
        tiger.assert(!ag.turn_active);

        // Turn 2 cumulative usage must emit delta (30 input, 10 output) not raw 130/30.
        try ag.send_user("ag-req-2", "fail check", null, &out);
        try ag.on_line(a, "{\"event\":\"result\",\"result\":{\"conversation_id\":\"conv-live\",\"status\":\"ERROR\",\"response\":\"\",\"error\":\"rate limit exceeded\",\"duration_seconds\":2.5,\"num_turns\":2,\"usage\":{\"input_tokens\":130,\"output_tokens\":30,\"cache_read_tokens\":25,\"total_tokens\":160}}}", &out);
        tiger.assert(!ag.turn_active);

        var settles: usize = 0;
        var saw_file_delta = false;
        for (out.events.items) |ev| {
            switch (ev.entry) {
                .file_change => |fc| {
                    if (fc.files.len > 0 and std.mem.eql(u8, fc.files[0].path, "/tmp/ws/a.txt")) {
                        saw_file_delta = true;
                    }
                },
                .turn_settled => |ts| {
                    settles += 1;
                    if (settles == 1) {
                        tiger.assert(ts.stop == .end_turn and ts.usage.input == 100 and ts.usage.output == 20 and ts.usage.cache_read == 10);
                    } else if (settles == 2) {
                        tiger.assert(ts.stop == .@"error" and ts.usage.input == 30 and ts.usage.output == 10 and ts.usage.cache_read == 15);
                        tiger.assert(ts.duration_ms.? == 1000);
                    }
                },
                else => {},
            }
        }
        tiger.assert(saw_file_delta and settles == 2);
    }
}

fn run_misc_oracle(gpa: std.mem.Allocator) !void {
    // 1. Sandbox spec ordering & mask protection.
    const masks = try sbox.default_masks(gpa, "/home/alice");
    defer {
        for (masks) |m| gpa.free(m.path);
        gpa.free(masks);
    }
    const spec_ok = sbox.SandboxSpec{
        .workspace = "/home/alice/repo",
        .readonly = true,
        .masks = masks,
        .git_dirs = &.{"/home/alice/repo/.git"},
    };
    const argv = try sbox.build_bwrap_argv(gpa, spec_ok, &.{ "pi", "--mode", "rpc" });
    defer gpa.free(argv);
    var saw_ro_ws = false;
    for (argv, 0..) |arg, idx| {
        if (std.mem.eql(u8, arg, "/home/alice/repo") and idx > 0 and std.mem.eql(u8, argv[idx - 1], "--ro-bind")) {
            saw_ro_ws = true;
        }
        // Readonly spec must not emit --bind for workspace.
        if (std.mem.eql(u8, arg, "/home/alice/repo") and idx > 0) {
            tiger.assert(!std.mem.eql(u8, argv[idx - 1], "--bind"));
        }
    }
    tiger.assert(saw_ro_ws);

    var saw_gemini_mask = false;
    for (masks) |m| {
        if (std.mem.endsWith(u8, m.path, "/.gemini")) saw_gemini_mask = true;
    }
    tiger.assert(saw_gemini_mask);

    const guard_ro = try sbox.build_guard_argv(gpa, "/usr/bin/protectorate", .{
        .workspace = "/home/alice/repo",
        .readonly = true,
        .runtime_dir = "/tmp/run-ro",
        .git_dirs = &.{"/home/alice/repo/.git"},
        .memory_max_bytes = 512 * 1024 * 1024,
    }, &.{ "agy", "--input-format", "stream-json" });
    defer sbox.free_guard_argv(gpa, guard_ro);
    var ro_has_flag = false;
    var ro_has_ws_rw = false;
    var ro_has_tmp = false;
    for (guard_ro, 0..) |arg, idx| {
        if (std.mem.eql(u8, arg, "--readonly")) ro_has_flag = true;
        if (std.mem.eql(u8, arg, "--rw") and idx + 1 < guard_ro.len and std.mem.eql(u8, guard_ro[idx + 1], "/home/alice/repo")) {
            ro_has_ws_rw = true;
        }
        if (std.mem.eql(u8, arg, "--tmp") and idx + 1 < guard_ro.len and std.mem.eql(u8, guard_ro[idx + 1], "/tmp/run-ro")) {
            ro_has_tmp = true;
        }
    }
    tiger.assert(ro_has_flag and !ro_has_ws_rw and ro_has_tmp);

    const guard_rw = try sbox.build_guard_argv(gpa, "/usr/bin/protectorate", .{
        .workspace = "/home/alice/repo",
        .readonly = false,
        .git_dirs = &.{"/home/alice/repo/.git"},
    }, &.{ "agy", "--input-format", "stream-json" });
    defer sbox.free_guard_argv(gpa, guard_rw);
    var rw_has_ws = false;
    for (guard_rw, 0..) |arg, idx| {
        if (std.mem.eql(u8, arg, "--rw") and idx + 1 < guard_rw.len and std.mem.eql(u8, guard_rw[idx + 1], "/home/alice/repo")) {
            rw_has_ws = true;
        }
    }
    tiger.assert(rw_has_ws);

    // Git dir overlapping credential mask must fail closed.
    const spec_bad = sbox.SandboxSpec{
        .workspace = "/home/alice/repo",
        .masks = masks,
        .git_dirs = &.{"/home/alice/.ssh/gitdir"},
    };
    if (sbox.build_mounts(gpa, spec_bad)) |ms| {
        gpa.free(ms);
        tiger.assert(false);
    } else |err| {
        tiger.assert(err == error.GitDirMasked);
    }

    // 2. Update version + SHA256SUMS + verify.
    const v_stab = upd_ver.Version.parse_tag("v1.2.3").?;
    const v_dev = upd_ver.Version.parse_tag("v1.2.3-dev.99").?;
    tiger.assert(upd_ver.Version.order(v_stab, v_dev) == .gt);
    tiger.assert(!upd_ver.Version.is_newer(v_dev, upd_ver.Version.parse_tag("v1.2.2").?, .stable));
    tiger.assert(upd_ver.Version.is_newer(v_dev, upd_ver.Version.parse_tag("v1.2.2").?, .dev));

    const payload = "protectorate-release-binary";
    var digest: [32]u8 = undefined;
    std.crypto.hash.sha2.Sha256.hash(payload, &digest, .{});
    const hex_chars = "0123456789abcdef";
    var hex: [64]u8 = undefined;
    for (digest, 0..) |b, i| {
        hex[i * 2] = hex_chars[b >> 4];
        hex[i * 2 + 1] = hex_chars[b & 0x0f];
    }
    tiger.assert(upd_ver.verify_sha256(payload, &hex));
    tiger.assert(!upd_ver.verify_sha256("tampered", &hex));

    const sums_txt = try std.fmt.allocPrint(gpa, "{s}  protectorate-linux-x86_64.tar.gz\n", .{&hex});
    defer gpa.free(sums_txt);
    const sums = try upd_ver.parse_sha256sums(gpa, sums_txt);
    defer {
        for (sums) |s| gpa.free(s.filename);
        gpa.free(sums);
    }
    tiger.assert(sums.len == 1 and std.mem.eql(u8, sums[0].filename, "protectorate-linux-x86_64.tar.gz"));

    // 3. QR code encoding & terminal render.
    const mat = try qr.encode("ptjoin1:node-abc:0123456789abcdef", .low);
    tiger.assert(mat.size >= 21);
    const term = try mat.to_terminal_alloc(gpa, 1, false);
    defer gpa.free(term);
    tiger.assert(term.len > 0);

    // 4. API JSON roundtrip with unknown forward-compatible enum/union arms.
    var api_arena = std.heap.ArenaAllocator.init(gpa);
    defer api_arena.deinit();
    const aa = api_arena.allocator();
    const raw_entry = "{\"type\":\"future_block\",\"payload\":42}";
    const dec_e = try schema.decode_arena(schema.Entry, aa, raw_entry);
    switch (dec_e) {
        .unknown => {},
        else => tiger.assert(false),
    }

    // 5. Extractors line sanity.
    var ext_st = extract.ExtractState{ .harness = .pi_local, .session = "ps" };
    var items: std.ArrayList(extract.Item) = .empty;
    const pi_line = "{\"type\":\"message\",\"timestamp\":\"2026-11-22T01:02:03.000Z\",\"message\":{\"role\":\"user\",\"content\":\"hello index\"}}";
    const pv = try std.json.parseFromSliceLeaky(std.json.Value, aa, pi_line, .{});
    _ = try extract.extract_pi_line(aa, pv, &ext_st, &items);
    tiger.assert(items.items.len == 1 and items.items[0].doc.turn == 1);
}

pub fn run_seed(gpa: std.mem.Allocator, seed: u64, steps: usize) !void {
    var prng = std.Random.DefaultPrng.init(seed);
    var world = CoreWorld.init(gpa, prng.random());
    defer world.deinit();

    for (0..steps) |_| {
        try world.step();
    }
    try world.verify_replay_equals_live();

    run_quota_oracle(seed);
    try run_drivers_oracle(gpa, seed);
    if (seed == 0 or (seed % 7) == 0) {
        try run_misc_oracle(gpa);
    }
}
