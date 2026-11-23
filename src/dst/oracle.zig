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
const admission = @import("../core/admission.zig");

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
    rollout_ids: std.ArrayList([]const u8) = .empty,
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
        for (self.rollout_ids.items) |s| self.gpa.free(s);
        self.rollout_ids.deinit(self.gpa);
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
                .notify_parent => |np| {
                    if (self.st.sleeves.get(np.child_id)) |csl| {
                        tiger.assert(csl.parent_id != null and std.mem.eql(u8, csl.parent_id.?, np.parent_id));
                    }
                    if (self.st.sleeves.get(np.parent_id)) |psl| {
                        if (psl.lifecycle != .removed and psl.lifecycle != .parked) {
                            const req_id = try std.fmt.allocPrint(self.gpa, "mail-rep-{d}", .{self.next_id});
                            defer self.gpa.free(req_id);
                            self.next_id += 1;
                            _ = try self.apply_checked(.{ .prompt_enqueued = .{
                                .sleeve_id = np.parent_id,
                                .request_id = req_id,
                                .from = np.child_id,
                                .kind = .report,
                                .mode = .queue,
                                .text = "child report",
                            } }, &sub_intents);
                        }
                    }
                },
                .compact_incarnation => |ci| {
                    if (self.rng.boolean()) {
                        _ = try self.apply_checked(.{ .compacted = .{
                            .sleeve_id = ci.sleeve_id,
                            .g = ci.g,
                            .tokens_after = 50_000,
                        } }, &sub_intents);
                    } else {
                        _ = try self.apply_checked(.{ .resleeve_requested = .{
                            .sleeve_id = ci.sleeve_id,
                            .harness = .claude,
                            .by = .system,
                            .cause = .handoff,
                        } }, &sub_intents);
                    }
                },
                .send_node_update => |snu| {
                    if (self.rng.uintLessThan(u8, 4) != 0) {
                        _ = try self.apply_checked(.{ .rollout_node_result = .{
                            .rollout_id = snu.rollout_id,
                            .node_id = snu.node_id,
                            .ok = self.rng.uintLessThan(u8, 4) != 0,
                            .detail = "ok",
                        } }, &sub_intents);
                    }
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
                            .kind = if (sl.d9 and self.rng.boolean()) .action else .tool,
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
            14 => {
                // Rollout exercise: start rollout across approved nodes or cancel an existing rollout.
                if (self.rollout_ids.items.len > 0 and self.rng.boolean()) {
                    const rid = self.rollout_ids.items[self.rng.uintLessThan(usize, self.rollout_ids.items.len)];
                    _ = try self.apply_checked(.{ .rollout_cancelled = .{ .rollout_id = rid } }, &intents);
                } else if (self.node_ids.items.len > 0) {
                    const max_pick = @min(self.node_ids.items.len, 3);
                    const count = 1 + self.rng.uintLessThan(usize, max_pick);
                    const off = self.rng.uintLessThan(usize, self.node_ids.items.len);
                    var nbuf: [3][]const u8 = undefined;
                    for (0..count) |i| {
                        nbuf[i] = self.node_ids.items[(off + i) % self.node_ids.items.len];
                    }
                    const rid = try std.fmt.allocPrint(self.gpa, "ro-{d}", .{self.next_id});
                    self.next_id += 1;
                    if (try self.apply_checked(.{ .rollout_started = .{
                        .rollout_id = rid,
                        .version = "v0.8.0",
                        .node_ids = nbuf[0..count],
                    } }, &intents)) {
                        try self.rollout_ids.append(self.gpa, rid);
                    } else {
                        self.gpa.free(rid);
                    }
                }
            },
            else => {
                if (self.rng.uintLessThan(u8, 3) == 0) {
                    self.now_ms += model.approval_timeout_ms + 100;
                }
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

    // I3 + D9: No pending tool approval for superseded generation; D9 approval requires owner.
    var ait = st.approvals.iterator();
    while (ait.next()) |e| {
        const app = e.value_ptr.*;
        if (app.is_d9 and app.state == .approved) {
            tiger.assert(app.decided_by == .owner);
        }
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

    // I6: Rollouts reference known nodes.
    var rit = st.rollouts.iterator();
    while (rit.next()) |re| {
        const ro = re.value_ptr.*;
        tiger.assert(ro.id.len > 0);
        for (ro.nodes.items) |rn| {
            tiger.assert(st.nodes.contains(rn.node_id));
        }
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

fn check_admission_invariants(auth: *const admission.Authority, last_max_epoch: *u64) void {
    tiger.assert(auth.next_epoch >= last_max_epoch.*);
    last_max_epoch.* = auth.next_epoch;
    tiger.assert(auth.grants.items.len <= admission.max_grants);
    tiger.assert(auth.used_slots() <= auth.cfg.slots);
    tiger.assert(auth.used_long_slots() <= auth.cfg.long_slots);

    var excl_epoch: ?u64 = null;
    for (auth.grants.items, 0..) |g, i| {
        tiger.assert(g.epoch >= 1 and g.epoch < auth.next_epoch);
        for (auth.grants.items[i + 1 ..]) |other| {
            tiger.assert(g.epoch != other.epoch);
            tiger.assert(!std.mem.eql(u8, g.lease_id, other.lease_id));
        }
        if (g.mode == .exclusive) {
            tiger.assert(excl_epoch == null);
            excl_epoch = g.epoch;
        }
        if (g.parent_epoch) |pe| {
            if (g.state != .fencing) {
                const p = auth.find_grant_by_epoch(pe) orelse unreachable;
                tiger.assert(p.parent_epoch == null);
                tiger.assert(p.state == .active);
                tiger.assert(g.deadline_ms <= p.deadline_ms);
            }
        }
    }
    if (excl_epoch) |ee| {
        for (auth.grants.items) |g| {
            if (g.epoch != ee) {
                tiger.assert(g.parent_epoch != null and g.parent_epoch.? == ee);
            }
        }
    }
    var idx: usize = 1;
    while (idx < auth.queue.items.len) : (idx += 1) {
        tiger.assert(auth.queue.items[idx - 1].seq < auth.queue.items[idx].seq);
    }
}

fn run_admission_oracle(gpa: std.mem.Allocator, seed: u64) !void {
    var prng = std.Random.DefaultPrng.init(seed ^ 0xA01D_7127);
    const rng = prng.random();

    const cfg: admission.CapacityConfig = .{
        .slots = 2 + rng.uintLessThan(u32, 7),
        .long_slots = 1 + rng.uintLessThan(u32, 3),
        .mem_margin_bytes = 512 * 1024 * 1024,
        .total_ram_bytes = 8 * 1024 * 1024 * 1024,
        .max_queue = 64,
        .default_ttl_ms = 5_000,
    };
    var auth = try admission.Authority.init(gpa, "boot-sim-1", cfg);
    defer auth.deinit();

    const LeaseRef = struct { lease_id: []u8, epoch: u64 };
    var leases: std.ArrayList(LeaseRef) = .empty;
    defer {
        for (leases.items) |l| gpa.free(l.lease_id);
        leases.deinit(gpa);
    }
    var tickets: std.ArrayList([]u8) = .empty;
    defer {
        for (tickets.items) |t| gpa.free(t);
        tickets.deinit(gpa);
    }

    var now_ms: i64 = 1_000_000;
    var last_max_epoch: u64 = 1;

    for (0..120) |step_idx| {
        const mem_avail: u64 = if (rng.uintLessThan(u8, 10) == 0)
            512 * 1024 * 1024
        else
            6 * 1024 * 1024 * 1024;
        var actions: std.ArrayList(admission.Action) = .empty;
        defer admission.deinit_actions(gpa, &actions);

        const op = rng.uintLessThan(u8, 12);
        switch (op) {
            0, 1, 2 => {
                var hbuf: [32]u8 = undefined;
                const holder = std.fmt.bufPrint(&hbuf, "h-{d}", .{step_idx}) catch "h";
                const excl = rng.uintLessThan(u8, 5) == 0;
                const req_slots: u32 = if (excl) cfg.slots else 1 + rng.uintLessThan(u32, 2);
                const req_long: u32 = if (rng.uintLessThan(u8, 4) == 0) 1 else 0;
                const req_mem: u64 = if (rng.boolean()) 512 * 1024 * 1024 else 0;
                const wait_ms: i64 = if (rng.uintLessThan(u8, 3) == 0) 0 else 4_000 + @as(i64, rng.uintLessThan(u32, 12_000));
                const res = try auth.enqueue_or_grant(
                    holder,
                    null,
                    if (excl) .exclusive else .shared,
                    .{ .slots = req_slots, .long_slots = req_long, .mem_bytes = req_mem },
                    3_000 + @as(i64, rng.uintLessThan(u32, 6_000)),
                    wait_ms,
                    now_ms,
                    mem_avail,
                    &actions,
                );
                switch (res) {
                    .granted => |g| try leases.append(gpa, .{
                        .lease_id = try gpa.dupe(u8, g.lease_id),
                        .epoch = g.epoch,
                    }),
                    .queued => |q| try tickets.append(gpa, try gpa.dupe(u8, q.ticket_id)),
                    .refused => {},
                }
            },
            3 => {
                if (leases.items.len > 0) {
                    const pick = leases.items[rng.uintLessThan(usize, leases.items.len)];
                    const res = try auth.acquire_child(
                        pick.epoch,
                        "child",
                        null,
                        .{ .slots = 1 },
                        4_000,
                        now_ms,
                        mem_avail,
                    );
                    switch (res) {
                        .granted => |g| try leases.append(gpa, .{
                            .lease_id = try gpa.dupe(u8, g.lease_id),
                            .epoch = g.epoch,
                        }),
                        else => {},
                    }
                }
            },
            4 => {
                if (leases.items.len > 0) {
                    const pick = leases.items[rng.uintLessThan(usize, leases.items.len)];
                    const ok_verify = auth.verify_epoch(auth.boot_id, pick.epoch, now_ms);
                    if (ok_verify) {
                        const g = auth.find_grant_by_epoch(pick.epoch).?;
                        tiger.assert(g.state != .fencing and now_ms < g.deadline_ms and auth.quarantined() == null);
                    }
                    _ = auth.renew_lease(pick.lease_id, pick.epoch, now_ms);
                }
            },
            5 => {
                if (tickets.items.len > 0) {
                    const idx = rng.uintLessThan(usize, tickets.items.len);
                    if (rng.boolean()) {
                        _ = auth.renew_ticket(tickets.items[idx], now_ms);
                    } else {
                        _ = auth.cancel_ticket(tickets.items[idx]);
                        gpa.free(tickets.orderedRemove(idx));
                    }
                }
            },
            6 => {
                if (auth.grants.items.len > 0) {
                    const g = auth.grants.items[rng.uintLessThan(usize, auth.grants.items.len)];
                    _ = try auth.begin_collecting(g.lease_id, g.epoch, now_ms, &actions);
                }
            },
            7 => {
                if (auth.grants.items.len > 0) {
                    const g = auth.grants.items[rng.uintLessThan(usize, auth.grants.items.len)];
                    const pe = g.epoch;
                    _ = try auth.release_lease(g.lease_id, g.epoch, now_ms, &actions);
                    for (auth.grants.items) |cg| {
                        if (cg.parent_epoch != null and cg.parent_epoch.? == pe) {
                            tiger.assert(cg.state == .fencing);
                        }
                    }
                }
            },
            8 => {
                for (auth.grants.items) |g| {
                    if (g.state == .fencing) {
                        const lid = try gpa.dupe(u8, g.lease_id);
                        defer gpa.free(lid);
                        _ = try auth.confirm_fence(lid, g.epoch, now_ms, mem_avail, &actions);
                        break;
                    }
                }
            },
            9, 10 => {
                now_ms += 400 + @as(i64, rng.uintLessThan(u32, 3_600));
                try auth.tick(now_ms, mem_avail, &actions);
            },
            else => {
                if (rng.uintLessThan(u8, 6) == 0 and auth.grants.items.len > 0) {
                    const g = auth.grants.items[0];
                    const lid = try gpa.dupe(u8, g.lease_id);
                    defer gpa.free(lid);
                    try auth.fail_fence(lid, g.epoch, "sim fence timeout");
                    tiger.assert(auth.quarantined() != null);
                    tiger.assert(!auth.verify_epoch(auth.boot_id, g.epoch, now_ms));
                    var tmp_act: std.ArrayList(admission.Action) = .empty;
                    defer admission.deinit_actions(gpa, &tmp_act);
                    const qres = try auth.enqueue_or_grant("qchk", null, .shared, .{ .slots = 1 }, 5_000, 0, now_ms, mem_avail, &tmp_act);
                    switch (qres) {
                        .refused => |r| tiger.assert(std.mem.eql(u8, r, "quarantined")),
                        else => tiger.assert(false),
                    }
                    // Confirm remaining fencing grants before clearing quarantine.
                    while (true) {
                        var fence_target: ?LeaseRef = null;
                        for (auth.grants.items) |fg| {
                            if (fg.state == .fencing) {
                                fence_target = .{ .lease_id = try gpa.dupe(u8, fg.lease_id), .epoch = fg.epoch };
                                break;
                            }
                        }
                        if (fence_target) |ft| {
                            defer gpa.free(ft.lease_id);
                            _ = try auth.confirm_fence(ft.lease_id, ft.epoch, now_ms, mem_avail, &tmp_act);
                        } else break;
                    }
                    tiger.assert(auth.clear_quarantine());
                    tiger.assert(auth.quarantined() == null);
                }
            },
        }
        for (actions.items) |act| {
            switch (act) {
                .grant_admitted => |ga| try leases.append(gpa, .{
                    .lease_id = try gpa.dupe(u8, ga.lease_id),
                    .epoch = ga.epoch,
                }),
                else => {},
            }
        }
        check_admission_invariants(&auth, &last_max_epoch);
    }

    // Drain all leases and tickets, confirming zero leaked slots/memory.
    var drain_act: std.ArrayList(admission.Action) = .empty;
    defer admission.deinit_actions(gpa, &drain_act);
    now_ms += 100_000;
    try auth.tick(now_ms, 6 * 1024 * 1024 * 1024, &drain_act);
    while (auth.grants.items.len > 0) {
        const g = auth.grants.items[0];
        if (g.state != .fencing) {
            const lid = try gpa.dupe(u8, g.lease_id);
            defer gpa.free(lid);
            _ = try auth.release_lease(lid, g.epoch, now_ms, &drain_act);
        } else {
            const lid = try gpa.dupe(u8, g.lease_id);
            defer gpa.free(lid);
            _ = try auth.confirm_fence(lid, g.epoch, now_ms, 6 * 1024 * 1024 * 1024, &drain_act);
        }
    }
    try auth.tick(now_ms + 1, 6 * 1024 * 1024 * 1024, &drain_act);
    tiger.assert(auth.grants.items.len == 0);
    tiger.assert(auth.used_slots() == 0 and auth.used_long_slots() == 0 and auth.reserved_mem_bytes() == 0);

    // Simulated node reboot: old boot_id/epoch is rejected on new Authority instance.
    var post_boot = try admission.Authority.init(gpa, "boot-sim-2", cfg);
    defer post_boot.deinit();
    tiger.assert(!post_boot.verify_epoch("boot-sim-1", 1, now_ms));

    if (seed == 0 or (seed % 7) == 0) {
        try run_admission_t127_regression(gpa);
    }
}

fn run_admission_t127_regression(gpa: std.mem.Allocator) !void {
    const cfg: admission.CapacityConfig = .{
        .slots = 4,
        .long_slots = 1,
        .mem_margin_bytes = 512 * 1024 * 1024,
        .total_ram_bytes = 8 * 1024 * 1024 * 1024,
        .max_queue = 32,
        .default_ttl_ms = 5_000,
    };
    var auth = try admission.Authority.init(gpa, "boot-t127", cfg);
    defer auth.deinit();

    var actions: std.ArrayList(admission.Action) = .empty;
    defer admission.deinit_actions(gpa, &actions);
    const mem: u64 = 6 * 1024 * 1024 * 1024;

    const rA = try auth.enqueue_or_grant("holder-A", "job-A", .shared, .{ .slots = 1 }, 5_000, 10_000, 1_000, mem, &actions);
    const gA = rA.granted;
    tiger.assert(gA.epoch == 1 and gA.deadline_ms == 6_000);

    const rB = try auth.enqueue_or_grant("holder-B", "job-B", .exclusive, .{ .slots = 4 }, 5_000, 20_000, 1_000, mem, &actions);
    const tB = rB.queued;
    tiger.assert(tB.position == 0);

    // Negative control: fifo_bypass — shared holder C MUST queue behind exclusive waiter B even though 1 slot is free.
    const rC = try auth.enqueue_or_grant("holder-C", "job-C", .shared, .{ .slots = 1 }, 5_000, 20_000, 1_000, mem, &actions);
    const tC = rC.queued;
    tiger.assert(tC.position == 1);

    // Child borrow under active parent A MUST succeed while B waits, capped to parent's deadline.
    const rChild = try auth.acquire_child(gA.epoch, "child-of-A", null, .{ .slots = 1 }, 15_000, 2_000, mem);
    const gChild = rChild.granted;
    tiger.assert(gChild.epoch == 2 and gChild.deadline_ms == 6_000);
    tiger.assert(auth.verify_epoch("boot-t127", gChild.epoch, 2_000));

    // Save IDs before clock expiry / mutations.
    const lidA = try gpa.dupe(u8, gA.lease_id);
    defer gpa.free(lidA);
    const lidChild = try gpa.dupe(u8, gChild.lease_id);
    defer gpa.free(lidChild);
    const tidB = try gpa.dupe(u8, tB.ticket_id);
    defer gpa.free(tidB);
    const tidC = try gpa.dupe(u8, tC.ticket_id);
    defer gpa.free(tidC);

    // Live waiters heartbeat their queued tickets before initial ticket TTL elapses.
    tiger.assert(auth.renew_ticket(tidB, 3_500));
    tiger.assert(auth.renew_ticket(tidC, 3_500));

    // Negative control: immortal_lease — reaching deadline transitions both A and child to .fencing & rejects verify_epoch.
    try auth.tick(6_000, mem, &actions);
    tiger.assert(!auth.verify_epoch("boot-t127", 1, 6_000));
    tiger.assert(!auth.verify_epoch("boot-t127", 2, 6_000));
    tiger.assert(auth.find_grant_by_id(lidA).?.state == .fencing);
    tiger.assert(auth.find_grant_by_id(lidChild).?.state == .fencing);

    // Negative control: reuse_before_fence — B is NOT admitted until BOTH A and child finish confirm_fence.
    tiger.assert(auth.find_grant_by_ticket(tidB) == null);
    tiger.assert(try auth.confirm_fence(lidA, 1, 6_050, mem, &actions));
    tiger.assert(auth.find_grant_by_ticket(tidB) == null);

    tiger.assert(try auth.confirm_fence(lidChild, 2, 6_100, mem, &actions));
    const gB = auth.find_grant_by_ticket(tidB).?;
    tiger.assert(gB.epoch == 3 and gB.mode == .exclusive);
    tiger.assert(auth.find_grant_by_ticket(tidC) == null);

    const lidB = try gpa.dupe(u8, gB.lease_id);
    defer gpa.free(lidB);
    tiger.assert(try auth.release_lease(lidB, 3, 6_200, &actions));
    tiger.assert(auth.find_grant_by_ticket(tidC) == null);
    tiger.assert(try auth.confirm_fence(lidB, 3, 6_250, mem, &actions));
    const gC = auth.find_grant_by_ticket(tidC).?;
    tiger.assert(gC.epoch == 4 and gC.mode == .shared);

    const lidC = try gpa.dupe(u8, gC.lease_id);
    defer gpa.free(lidC);
    tiger.assert(try auth.release_lease(lidC, 4, 6_300, &actions));
    tiger.assert(try auth.confirm_fence(lidC, 4, 6_350, mem, &actions));
    tiger.assert(auth.grants.items.len == 0 and auth.queue.items.len == 0);
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
    try run_admission_oracle(gpa, seed);
    if (seed == 0 or (seed % 7) == 0) {
        try run_misc_oracle(gpa);
    }
}
