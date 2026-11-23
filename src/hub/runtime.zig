const std = @import("std");
const builtin = @import("builtin");
const tiger = @import("../tiger.zig");
const version = @import("../version.zig");
const json_util = @import("../json_util.zig");
const model = @import("../core/model.zig");
const event = @import("../core/event.zig");
const intent = @import("../core/intent.zig");
const state = @import("../core/state.zig");
const apply = @import("../core/apply.zig");
const store = @import("../journal/store.zig");
const h_index = @import("../history/index.zig");
const q_readers = @import("../quota/readers.zig");
const q_select = @import("../quota/select.zig");
const s_spec = @import("../sandbox/spec.zig");
const s_exec = @import("../sandbox/exec.zig");
const d_proto = @import("../drivers/protocol.zig");
const d_session = @import("../drivers/session.zig");
const u_ver = @import("../update/version.zig");
const u_store = @import("../update/store.zig");
const doctor = @import("../doctor/doctor.zig");
const schema = @import("../api/schema.zig");
const noise = @import("../net/noise.zig");
const msg = @import("../net/msg.zig");
const home = @import("home.zig");
const transcript = @import("transcript.zig");
const stream = @import("stream.zig");
const brief = @import("brief.zig");
const git = @import("git.zig");
const fleet = @import("fleet.zig");

pub const NodeTelemetry = struct {
    inventory: ?msg.Inventory = null,
    sample: ?msg.Sample = null,
    presence: msg.Presence = .{ .state = .unknown },
    last_error: ?[]const u8 = null,
    pending_update: ?[]const u8 = null,
};

pub const PendingReg = struct {
    auth_id: []const u8,
    name: []const u8,
    pub_key: [32]u8,
    pub_hex: [64]u8,
    seen_at_ms: i64,
    expires_at_ms: i64,
    addrs: []const []const u8,
};

pub const JobMeta = struct {
    target: schema.JobTarget,
    argv: []const []const u8,
    cwd: ?[]const u8,
    decl_artifacts: []const []const u8,
    artifacts: []schema.JobArtifact = &.{},
    log_path: []const u8,
    log_bytes: u64 = 0,
    started_at_ms: ?i64 = null,
    ended_at_ms: ?i64 = null,
    held_reason: ?[]const u8 = null,
    approval_id: ?[]const u8 = null,
};

const DriverHandle = struct {
    g: u64,
    sleeve_id: []const u8,
    rt: *Runtime,
    sess: *d_session.Session,
    guard: s_exec.ProcessGuard = .{},
    turn_id: u64 = 0,
};

pub const RuntimeOptions = struct {
    home_override: ?[]const u8 = null,
    listen_override: ?[]const u8 = null,
    fleet_listen_override: ?[]const u8 = null,
    dev_token_override: ?[]const u8 = null,
    no_sandbox: bool = false,
};

pub const Runtime = struct {
    gpa: std.mem.Allocator,
    io: std.Io,
    env: *const std.process.Environ.Map,
    hub_home: home.HubHome,
    started_at_ms: i64,
    no_sandbox: bool,
    bwrap_ok: bool,
    landlock_ok: bool,

    state_mu: std.Io.Mutex = .init,
    journal: store.JournalStore,
    core_state: state.State,

    meta_mu: std.Io.Mutex = .init,
    workspaces: std.StringHashMapUnmanaged(schema.Workspace) = .empty,
    sessions: std.StringHashMapUnmanaged(*DriverHandle) = .empty,
    worktree_bases: std.StringHashMapUnmanaged([]const u8) = .empty,
    device_inodes: std.StringHashMapUnmanaged(?u64) = .empty,
    pending_nodes: std.StringHashMapUnmanaged(PendingReg) = .empty,
    node_tele: std.StringHashMapUnmanaged(NodeTelemetry) = .empty,
    job_meta: std.StringHashMapUnmanaged(JobMeta) = .empty,
    hub_events: std.ArrayList(schema.HubEvent) = .empty,
    next_hub_event_seq: u64 = 1,
    d9_turn_id: u64 = 0,
    d9_turn_spawns: u32 = 0,
    next_id_seq: std.atomic.Value(u64) = .init(1),
    host_inventory: ?msg.Inventory = null,
    cached_quotas: []schema.Quota = &.{},
    cached_quotas_arena: std.heap.ArenaAllocator,
    restore_error: ?[]const u8 = null,

    transcripts: transcript.TranscriptStore,
    history: h_index.HistoryIndex,
    stream_hub: stream.StreamHub,
    fleet_srv: fleet.FleetServer,

    http_listen: []const u8,
    fleet_listen: []const u8,
    stopping: std.atomic.Value(bool) = .init(false),
    tick_thread: ?std.Thread = null,
    history_thread: ?std.Thread = null,
    shutdown_requested: std.atomic.Value(bool) = .init(false),

    pub fn open(
        gpa: std.mem.Allocator,
        io: std.Io,
        env: *const std.process.Environ.Map,
        opts: RuntimeOptions,
    ) !*Runtime {
        const home_path = try home.HubHome.resolve_path(gpa, env, opts.home_override);
        defer gpa.free(home_path);

        var hh = try home.HubHome.open(gpa, io, home_path, opts.dev_token_override);
        errdefer hh.deinit();

        const http_listen = try gpa.dupe(u8, opts.listen_override orelse hh.config.listen_addr);
        errdefer gpa.free(http_listen);
        const fleet_listen = try gpa.dupe(u8, opts.fleet_listen_override orelse hh.config.fleet_listen);
        errdefer gpa.free(fleet_listen);

        var root_dir = try std.Io.Dir.cwd().openDir(io, hh.root_path, .{});
        defer root_dir.close(io);

        var jstore = try store.JournalStore.open(gpa, io, root_dir);
        errdefer jstore.close();

        var st = state.State.init(gpa);
        errdefer st.deinit();

        var rest_err: ?[]const u8 = null;
        jstore.replay_into(&st) catch |err| {
            rest_err = @errorName(err);
        };

        var tx_store = try transcript.TranscriptStore.init(gpa, io, hh.transcripts_path);
        errdefer tx_store.deinit();

        var h_idx = try h_index.HistoryIndex.open(gpa, io, hh.history_path);
        errdefer h_idx.deinit();

        const rt = try gpa.create(Runtime);
        errdefer gpa.destroy(rt);

        const bwrap_ok = if (opts.no_sandbox or hh.config.no_sandbox) false else s_exec.probe_bwrap(gpa, io);
        const landlock_ok = if (opts.no_sandbox or hh.config.no_sandbox) false else s_exec.probe_landlock();

        rt.* = .{
            .gpa = gpa,
            .io = io,
            .env = env,
            .hub_home = hh,
            .started_at_ms = std.Io.Clock.real.now(io).toMilliseconds(),
            .no_sandbox = opts.no_sandbox or hh.config.no_sandbox,
            .bwrap_ok = bwrap_ok,
            .landlock_ok = landlock_ok,
            .journal = jstore,
            .core_state = st,
            .cached_quotas_arena = std.heap.ArenaAllocator.init(gpa),
            .restore_error = rest_err,
            .transcripts = tx_store,
            .history = h_idx,
            .stream_hub = stream.StreamHub.init(gpa, io),
            .fleet_srv = undefined,
            .http_listen = http_listen,
            .fleet_listen = fleet_listen,
        };

        rt.fleet_srv = try fleet.FleetServer.init(gpa, io, hh.hub_keypair, fleet_listen, .{
            .ctx = rt,
            .get_allowed_peers = hook_allowed_peers,
            .on_connected = hook_connected,
            .on_telemetry = hook_telemetry,
            .on_disconnected = hook_disconnected,
        });
        try rt.fleet_srv.start();
        errdefer rt.fleet_srv.stop_and_deinit();

        try rt.recover_after_replay();
        if (rest_err == null) {
            u_store.mark_healthy(gpa, io, hh.releases_path) catch {};
        }
        rt.init_sleeve_devices();
        rt.load_host_info();
        rt.refresh_quotas_sync();
        _ = rt.history.ingest_roots(null, hh.transcripts_path);

        rt.tick_thread = try std.Thread.spawn(.{}, tick_loop, .{rt});
        if (env.get("PROTECTORATE_NO_HOST_HISTORY") == null) {
            rt.history_thread = std.Thread.spawn(.{}, history_bg_loop, .{rt}) catch null;
        }
        return rt;
    }

    fn history_bg_loop(self: *Runtime) void {
        _ = self.history.ingest_roots_cancellable(
            self.env.get("HOME"),
            self.hub_home.transcripts_path,
            &self.stopping,
        );
    }

    pub fn close_and_destroy(self: *Runtime) void {
        self.stopping.store(true, .release);
        self.stream_hub.wake_all();
        if (self.tick_thread) |t| t.join();
        if (self.history_thread) |t| t.join();

        var handles: std.ArrayList(*DriverHandle) = .empty;
        {
            self.meta_mu.lockUncancelable(self.io);
            var sit = self.sessions.iterator();
            while (sit.next()) |e| {
                handles.append(self.gpa, e.value_ptr.*) catch {};
                self.gpa.free(e.key_ptr.*);
            }
            self.sessions.deinit(self.gpa);
            self.meta_mu.unlock(self.io);
        }
        for (handles.items) |dh| {
            dh.guard.terminate();
            dh.sess.kill_and_destroy();
            dh.guard.deinit();
            self.gpa.free(dh.sleeve_id);
            self.gpa.destroy(dh);
        }
        handles.deinit(self.gpa);

        self.fleet_srv.stop_and_deinit();
        self.stream_hub.deinit();
        self.history.deinit();
        self.transcripts.deinit();

        {
            self.meta_mu.lockUncancelable(self.io);
            self.workspaces.deinit(self.gpa);
            var wbit = self.worktree_bases.iterator();
            while (wbit.next()) |e| {
                self.gpa.free(e.key_ptr.*);
                self.gpa.free(e.value_ptr.*);
            }
            self.worktree_bases.deinit(self.gpa);
            var dit = self.device_inodes.iterator();
            while (dit.next()) |e| {
                self.gpa.free(e.key_ptr.*);
            }
            self.device_inodes.deinit(self.gpa);
            var pit = self.pending_nodes.iterator();
            while (pit.next()) |e| {
                self.gpa.free(e.key_ptr.*);
                self.gpa.free(e.value_ptr.name);
                for (e.value_ptr.addrs) |a| self.gpa.free(a);
                self.gpa.free(e.value_ptr.addrs);
            }
            self.pending_nodes.deinit(self.gpa);
            self.node_tele.deinit(self.gpa);
            var jit = self.job_meta.iterator();
            while (jit.next()) |e| {
                self.gpa.free(e.key_ptr.*);
                self.gpa.free(e.value_ptr.log_path);
            }
            self.job_meta.deinit(self.gpa);
            for (self.hub_events.items) |ev| {
                self.gpa.free(ev.source);
                if (ev.target) |t| self.gpa.free(t);
                self.gpa.free(ev.text);
            }
            self.hub_events.deinit(self.gpa);
            self.cached_quotas_arena.deinit();
            self.meta_mu.unlock(self.io);
        }

        self.state_mu.lockUncancelable(self.io);
        self.journal.close();
        self.core_state.deinit();
        self.state_mu.unlock(self.io);

        self.hub_home.deinit();
        self.gpa.free(self.http_listen);
        self.gpa.free(self.fleet_listen);
        self.gpa.destroy(self);
    }

    pub fn gen_id(self: *Runtime, arena: std.mem.Allocator, prefix: []const u8) ![]const u8 {
        const n = self.next_id_seq.fetchAdd(1, .acq_rel);
        const now = std.Io.Clock.real.now(self.io).toMilliseconds();
        return std.fmt.allocPrint(arena, "{s}-{d}-{d}", .{ prefix, now, n });
    }

    fn recover_after_replay(self: *Runtime) !void {
        var arena_st = std.heap.ArenaAllocator.init(self.gpa);
        defer arena_st.deinit();
        const a = arena_st.allocator();

        var stop_ids: std.ArrayList([]const u8) = .empty;
        var ws_entries: std.ArrayList(schema.Workspace) = .empty;
        var offline_nodes: std.ArrayList([]const u8) = .empty;

        {
            self.state_mu.lockUncancelable(self.io);
            defer self.state_mu.unlock(self.io);
            var sit = self.core_state.sleeves.iterator();
            while (sit.next()) |e| {
                const sl = e.value_ptr.*;
                if (sl.lifecycle != .stopped and sl.lifecycle != .parked and sl.lifecycle != .removed) {
                    try stop_ids.append(a, try a.dupe(u8, sl.id));
                }
                if (sl.lifecycle != .removed) {
                    try ws_entries.append(a, .{
                        .id = try a.dupe(u8, sl.workspace_id),
                        .name = try a.dupe(u8, std.fs.path.basename(sl.path)),
                        .path = try a.dupe(u8, sl.path),
                    });
                }
            }
            var nit = self.core_state.nodes.iterator();
            while (nit.next()) |e| {
                if (e.value_ptr.online) {
                    try offline_nodes.append(a, try a.dupe(u8, e.value_ptr.id));
                }
            }
        }

        for (ws_entries.items) |ws| {
            self.register_workspace(ws.id, ws.name, ws.path) catch {};
        }
        for (stop_ids.items) |sid| {
            self.commit(.{ .stopped = .{ .sleeve_id = sid } }) catch {};
        }
        for (offline_nodes.items) |nid| {
            self.commit(.{ .node_disconnected = .{ .node_id = nid, .reason = "hub restarted" } }) catch {};
        }
    }

    pub fn register_workspace(self: *Runtime, ws_id: []const u8, name: []const u8, path: []const u8) !void {
        self.meta_mu.lockUncancelable(self.io);
        defer self.meta_mu.unlock(self.io);
        if (self.workspaces.contains(ws_id)) return;
        const id_d = try self.core_state.dupe_str(ws_id);
        const name_d = try self.core_state.dupe_str(name);
        const path_d = try self.core_state.dupe_str(path);
        try self.workspaces.put(self.gpa, id_d, .{ .id = id_d, .name = name_d, .path = path_d });
    }

    pub fn load_host_info(self: *Runtime) void {
        _ = self;
    }

    pub fn refresh_quotas_sync(self: *Runtime) void {
        var arena_st = std.heap.ArenaAllocator.init(self.gpa);
        const now = std.Io.Clock.real.now(self.io).toMilliseconds();
        const rep = q_readers.read_all(arena_st.allocator(), self.io, self.env.get("HOME"), now, false) catch {
            arena_st.deinit();
            return;
        };

        self.meta_mu.lockUncancelable(self.io);
        self.cached_quotas_arena.deinit();
        self.cached_quotas_arena = arena_st;
        const a = self.cached_quotas_arena.allocator();
        const out: []schema.Quota = a.alloc(schema.Quota, rep.quotas.len) catch &.{};
        for (rep.quotas, 0..) |q, i| {
            const wins: []schema.QuotaWindow = a.alloc(schema.QuotaWindow, q.windows.len) catch &.{};
            for (q.windows, 0..) |w, wi| {
                wins[wi] = .{
                    .label = w.label,
                    .used_pct = w.used_pct,
                    .resets_at_ms = if (w.resets_at_ms) |r| .{ .ms = r } else null,
                    .length_ms = if (w.length_ms) |l| @intCast(l) else null,
                };
            }
            out[i] = .{
                .subscription = switch (q.subscription) {
                    .claude => .claude,
                    .codex => .codex,
                    .grok => .grok,
                    .local => .local,
                    .api => .api,
                    .unknown => .unknown,
                },
                .plan = q.plan,
                .reading = switch (q.reading) {
                    .unknown => .unknown,
                    .ok => .ok,
                    .stale => .stale,
                    .@"error" => .@"error",
                },
                .fetched_at_ms = .{ .ms = q.fetched_at_ms },
                .windows = wins,
                .note = q.note,
            };
        }
        self.cached_quotas = out;
        self.meta_mu.unlock(self.io);
    }

    pub fn commit(self: *Runtime, ev: event.Event) !void {
        var intent_arena_st = std.heap.ArenaAllocator.init(self.gpa);
        defer intent_arena_st.deinit();
        const ia = intent_arena_st.allocator();

        var intents: std.ArrayList(intent.Intent) = .empty;
        defer intents.deinit(self.gpa);

        const now_ms: i64 = std.Io.Clock.real.now(self.io).toMilliseconds();

        {
            self.state_mu.lockUncancelable(self.io);
            defer self.state_mu.unlock(self.io);

            const seq = self.core_state.event_seq + 1;
            const se: event.StampedEvent = .{
                .seq = seq,
                .ts_ms = now_ms,
                .event = ev,
            };

            try apply.apply(self.gpa, &self.core_state, se, &intents);
            self.journal.append(se) catch {};
        }

        for (intents.items) |it| {
            self.execute_intent(ia, it);
        }

        self.publish_event_updates(ia, ev);
    }

    fn execute_intent(self: *Runtime, ia: std.mem.Allocator, it: intent.Intent) void {
        switch (it) {
            .provision_worktree => |p| {
                var base_repo: []const u8 = p.path;
                {
                    self.meta_mu.lockUncancelable(self.io);
                    if (self.worktree_bases.get(p.sleeve_id)) |b| {
                        base_repo = ia.dupe(u8, b) catch p.path;
                    }
                    self.meta_mu.unlock(self.io);
                }
                var branch_name: []const u8 = "main";
                {
                    self.state_mu.lockUncancelable(self.io);
                    if (self.core_state.sleeves.get(p.sleeve_id)) |sl| {
                        if (sl.branch) |br| branch_name = ia.dupe(u8, br) catch "main";
                    }
                    self.state_mu.unlock(self.io);
                }
                git.provision_worktree(self.gpa, self.io, base_repo, p.path, branch_name) catch |err| {
                    self.commit(.{ .provision_failed = .{ .sleeve_id = p.sleeve_id, .reason = @errorName(err) } }) catch {};
                    return;
                };
                const br = git.current_branch(ia, self.io, p.path);
                self.commit(.{ .provisioned = .{ .sleeve_id = p.sleeve_id, .branch = br } }) catch {};
            },
            .remove_worktree => |rw| {
                self.stop_driver_for(rw.sleeve_id);
                git.remove_worktree(self.gpa, self.io, rw.path);
            },
            .launch_incarnation => |li| {
                self.launch_driver(li) catch |err| {
                    self.record_hub_event(.sleeve, "hub", li.sleeve_id, @errorName(err), .alarm);
                    self.commit(.{ .exited = .{
                        .sleeve_id = li.sleeve_id,
                        .g = li.g,
                        .cause = .crash,
                        .requested = false,
                    } }) catch {};
                };
            },
            .deliver_prompt => |dp| {
                const dh = self.get_driver(dp.sleeve_id);
                if (dh) |h| {
                    if (h.g == dp.g) {
                        h.sess.send_user(dp.request_id, dp.text, if (dp.steer) "" else null) catch |err| {
                            self.record_hub_event(.sleeve, "driver", dp.sleeve_id, @errorName(err), .warn);
                            return;
                        };
                        const at_ms = std.Io.Clock.real.now(self.io).toMilliseconds();
                        const ce = self.transcripts.append(dp.sleeve_id, dp.g, h.turn_id, at_ms, .{
                            .user_message = .{
                                .request_id = dp.request_id,
                                .from = "owner",
                                .text = dp.text,
                                .steer = dp.steer,
                            },
                        }) catch null;
                        if (ce) |c| {
                            self.stream_hub.publish_raw(.{ .entry = .{
                                .seq = 0,
                                .sleeve_id = dp.sleeve_id,
                                .entry = c,
                            } });
                        }
                        self.commit(.{ .prompt_delivered = .{
                            .sleeve_id = dp.sleeve_id,
                            .g = dp.g,
                            .request_id = dp.request_id,
                        } }) catch {};
                    }
                }
            },
            .interrupt_incarnation => |ii| {
                if (self.get_driver(ii.sleeve_id)) |dh| {
                    if (dh.g == ii.g) dh.sess.interrupt() catch {};
                }
            },
            .kill_incarnation => |ki| {
                self.stop_driver_for(ki.sleeve_id);
                self.commit(.{ .stopped = .{ .sleeve_id = ki.sleeve_id } }) catch {};
            },
            .respond_approval => |ra| {
                if (self.get_driver(ra.sleeve_id)) |dh| {
                    if (dh.g == ra.g) {
                        const dec: d_proto.Decision = switch (ra.verdict) {
                            .allow_once => .allow_once,
                            .allow_session => .allow_session,
                            .deny => .deny,
                        };
                        dh.sess.respond_approval(ra.approval_id, dec) catch {};
                    }
                }
            },
            .write_brief => |wb| {
                brief.write_handoff(self.gpa, self.io, self.hub_home.briefs_path, wb.sleeve_id, wb.g, wb.cause, "Auto handoff") catch {};
            },
            .dispatch_job => |dj| {
                self.dispatch_job_async(dj.job_id, dj.node_id);
            },
            .verify_job => |vj| {
                self.verify_job_async(vj.job_id, vj.node_id);
            },
            .cancel_job => |cj| {
                self.cancel_job_async(cj.job_id, cj.node_id);
            },
            .emit_notice => |en| {
                self.record_hub_event(.guardrail, en.code, en.sleeve_id, en.text, .warn);
            },
            .notify_parent => |np| {
                var deliver = false;
                var child_name: []const u8 = np.child_id;
                {
                    self.state_mu.lockUncancelable(self.io);
                    if (self.core_state.sleeves.get(np.parent_id)) |psl| {
                        if (psl.lifecycle != .removed and psl.lifecycle != .parked) deliver = true;
                    }
                    if (self.core_state.sleeves.get(np.child_id)) |csl| {
                        child_name = ia.dupe(u8, csl.name) catch np.child_id;
                    }
                    self.state_mu.unlock(self.io);
                }
                if (deliver) {
                    const rid = self.gen_id(ia, "rep") catch "";
                    const st_str: []const u8 = if (np.status) |s| s.to_wire() else "settled";
                    const txt = std.fmt.allocPrint(ia, "[child report from {s}] turn={d} status={s}", .{
                        child_name,
                        np.turn,
                        st_str,
                    }) catch "[child report]";
                    self.commit(.{ .prompt_enqueued = .{
                        .sleeve_id = np.parent_id,
                        .request_id = rid,
                        .from = child_name,
                        .kind = .report,
                        .mode = .queue,
                        .text = txt,
                    } }) catch {};
                }
            },
            .compact_incarnation => |ci| {
                if (self.get_driver(ci.sleeve_id)) |dh| {
                    if (dh.g == ci.g) {
                        if (dh.sess.compact()) |_| {} else |err| {
                            if (err == error.Unsupported) {
                                var cur_h: ?model.Harness = null;
                                {
                                    self.state_mu.lockUncancelable(self.io);
                                    if (self.core_state.sleeves.get(ci.sleeve_id)) |sl| {
                                        if (sl.inc) |inc| cur_h = inc.harness;
                                    }
                                    self.state_mu.unlock(self.io);
                                }
                                if (cur_h) |h| {
                                    brief.write_handoff(
                                        self.gpa,
                                        self.io,
                                        self.hub_home.briefs_path,
                                        ci.sleeve_id,
                                        ci.g,
                                        .handoff,
                                        "Context ceiling reached; auto-handoff",
                                    ) catch {};
                                    self.commit(.{ .resleeve_requested = .{
                                        .sleeve_id = ci.sleeve_id,
                                        .harness = h,
                                        .by = .system,
                                        .cause = .handoff,
                                    } }) catch {};
                                }
                            } else {
                                self.record_hub_event(.sleeve, "compact", ci.sleeve_id, @errorName(err), .warn);
                            }
                        }
                    }
                }
            },
            .send_node_update => |snu| {
                self.send_node_update_async(snu.rollout_id, snu.node_id, snu.version);
            },
        }
    }

    fn launch_driver(self: *Runtime, li: anytype) !void {
        var arena_st = std.heap.ArenaAllocator.init(self.gpa);
        defer arena_st.deinit();
        const a = arena_st.allocator();

        var sl_path: []const u8 = "";
        var sl_layout: model.Layout = .shared;
        {
            self.state_mu.lockUncancelable(self.io);
            defer self.state_mu.unlock(self.io);
            const sl = self.core_state.sleeves.get(li.sleeve_id) orelse return error.NotFound;
            sl_path = try a.dupe(u8, sl.path);
            sl_layout = sl.layout;
        }

        const prog: []const u8 = switch (li.harness) {
            .claude => "claude",
            .codex => "codex",
            .grok => "grok",
            .pi_vendor, .pi_local => "pi",
            .antigravity => "agy",
        };
        const p_harness: d_proto.Harness = switch (li.harness) {
            .claude => .claude,
            .codex => .codex,
            .grok => .grok,
            .pi_vendor => .pi_vendor,
            .pi_local => .pi_local,
            .antigravity => .antigravity,
        };

        var guard_mode: s_spec.GuardMode = .none;
        var guard_exe: ?[]const u8 = null;
        var sb_spec: ?s_spec.SandboxSpec = null;
        var child_env = try self.env.clone(a);
        if (!self.no_sandbox) {
            const sleeve_tmp = try std.fs.path.join(a, &.{ self.hub_home.root_path, "run", li.sleeve_id, "tmp" });
            try std.Io.Dir.cwd().createDirPath(self.io, sleeve_tmp);
            for (s_spec.stripped_nested_env_keys) |k| {
                _ = child_env.swapRemove(k);
            }
            try child_env.put("TMPDIR", sleeve_tmp);
            try child_env.put("TEMP", sleeve_tmp);
            try child_env.put("TMP", sleeve_tmp);
            try child_env.put("PROTECTORATE_SLEEVE", li.sleeve_id);
            try child_env.put("PROTECTORATE_WORKSPACE", sl_path);

            if (builtin.os.tag == .linux) {
                guard_mode = if (self.bwrap_ok) .bwrap else if (self.landlock_ok) .landlock else .none;
                if (guard_mode != .none) {
                    if (guard_mode == .landlock) {
                        guard_exe = try std.process.executablePathAlloc(self.io, a);
                    }
                    const host_home = self.env.get("HOME");
                    const git_dirs = try s_exec.resolve_git_dirs(a, self.io, sl_path);

                    var masks: std.ArrayList(s_spec.Mount) = .empty;
                    var extra_rw: std.ArrayList([]const u8) = .empty;
                    if (host_home) |hh| {
                        const def_masks = try s_spec.default_masks(a, hh);
                        const keep_rel: []const u8 = switch (li.harness) {
                            .claude => ".claude",
                            .codex => ".codex",
                            .grok => ".grok",
                            .pi_vendor, .pi_local => ".pi",
                            .antigravity => ".gemini",
                        };
                        for (def_masks) |m| {
                            const tail = std.fs.path.basename(m.path);
                            if (std.mem.eql(u8, tail, keep_rel)) {
                                try extra_rw.append(a, m.path);
                                continue;
                            }
                            if (std.Io.Dir.cwd().statFile(self.io, m.path, .{})) |_| {
                                try masks.append(a, m);
                            } else |_| {
                                if (std.Io.Dir.cwd().openDir(self.io, m.path, .{})) |dval| {
                                    var d = dval;
                                    d.close(self.io);
                                    try masks.append(a, m);
                                } else |_| {}
                            }
                        }
                    }

                    var env_set: std.ArrayList(s_spec.EnvVar) = .empty;
                    for (s_spec.default_env_allow_keys) |ek| {
                        if (self.env.get(ek)) |ev| {
                            try env_set.append(a, .{ .key = ek, .value = ev });
                        }
                    }
                    if (host_home) |hh| {
                        try env_set.append(a, .{ .key = "HOME", .value = hh });
                    }
                    for ([_][]const u8{ "SSL_CERT_FILE", "SSL_CERT_DIR", "NODE_EXTRA_CA_CERTS", "XDG_CONFIG_HOME", "XDG_DATA_HOME" }) |ek| {
                        if (self.env.get(ek)) |ev| {
                            try env_set.append(a, .{ .key = ek, .value = ev });
                        }
                    }
                    try env_set.appendSlice(a, &.{
                        .{ .key = "TMPDIR", .value = sleeve_tmp },
                        .{ .key = "TEMP", .value = sleeve_tmp },
                        .{ .key = "TMP", .value = sleeve_tmp },
                        .{ .key = "PROTECTORATE_SLEEVE", .value = li.sleeve_id },
                        .{ .key = "PROTECTORATE_WORKSPACE", .value = sl_path },
                    });

                    sb_spec = .{
                        .workspace = sl_path,
                        .readonly = (sl_layout == .readonly),
                        .host_home = null,
                        .runtime_dir = sleeve_tmp,
                        .git_dirs = git_dirs,
                        .extra_rw = extra_rw.items,
                        .masks = masks.items,
                        .env_set = env_set.items,
                    };
                }
            }
        }

        const dh = try self.gpa.create(DriverHandle);
        errdefer self.gpa.destroy(dh);
        const sid_dup = try self.gpa.dupe(u8, li.sleeve_id);
        errdefer self.gpa.free(sid_dup);

        dh.* = .{
            .g = li.g,
            .sleeve_id = sid_dup,
            .rt = self,
            .sess = undefined,
            .guard = s_exec.ProcessGuard.init(null),
        };
        errdefer dh.guard.deinit();

        const sess_id = try std.fmt.allocPrint(a, "{s}-{d}", .{ li.sleeve_id, li.g });
        const mod_opt: ?[]const u8 = if (li.model_name.len > 0) li.model_name else null;
        const prov_opt: ?[]const u8 = if (li.provider.len > 0) li.provider else null;

        dh.sess = try d_session.Session.spawn(self.gpa, self.io, .{
            .harness = p_harness,
            .g = li.g,
            .program = prog,
            .cwd = sl_path,
            .model = mod_opt,
            .provider = prov_opt,
            .effort = li.effort,
            .session_id = sess_id,
            .sandbox_mode = guard_mode,
            .guard_exe = guard_exe,
            .sandbox = sb_spec,
            .environ_map = &child_env,
            .on_event = on_driver_event,
            .on_exit = on_driver_exit,
            .ctx = dh,
        });
        dh.guard.attach(dh.sess.child_id());

        self.stop_driver_for(li.sleeve_id);
        {
            self.meta_mu.lockUncancelable(self.io);
            defer self.meta_mu.unlock(self.io);
            const kdup = try self.gpa.dupe(u8, li.sleeve_id);
            try self.sessions.put(self.gpa, kdup, dh);
        }

        try self.commit(.{ .launched = .{
            .sleeve_id = li.sleeve_id,
            .g = li.g,
            .harness = li.harness,
            .mode = li.mode,
            .model_name = li.model_name,
            .provider = li.provider,
            .effort = li.effort,
        } });
        try dh.sess.begin();
    }

    fn get_driver(self: *Runtime, sleeve_id: []const u8) ?*DriverHandle {
        self.meta_mu.lockUncancelable(self.io);
        defer self.meta_mu.unlock(self.io);
        return self.sessions.get(sleeve_id);
    }

    fn stop_driver_for(self: *Runtime, sleeve_id: []const u8) void {
        var removed: ?*DriverHandle = null;
        {
            self.meta_mu.lockUncancelable(self.io);
            if (self.sessions.fetchRemove(sleeve_id)) |kv| {
                self.gpa.free(kv.key);
                removed = kv.value;
            }
            self.meta_mu.unlock(self.io);
        }
        if (removed) |dh| {
            dh.guard.terminate();
            dh.sess.kill_and_destroy();
            dh.guard.deinit();
            self.gpa.free(dh.sleeve_id);
            self.gpa.destroy(dh);
        }
    }

    fn on_driver_event(ctx: ?*anyopaque, ev: d_proto.DriverEvent) void {
        const dh: *DriverHandle = @ptrCast(@alignCast(ctx orelse return));
        const self = dh.rt;

        switch (ev.entry) {
            .session_info => |si| {
                const nid = si.native_id;
                self.commit(.{ .session_bound = .{
                    .sleeve_id = dh.sleeve_id,
                    .g = dh.g,
                    .native_id = nid,
                } }) catch {};
                const ce = self.transcripts.append(dh.sleeve_id, dh.g, dh.turn_id, ev.time_ms, .{
                    .session_info = .{
                        .harness = map_proto_harness(si.harness),
                        .mode = .headless,
                        .native_id = si.native_id,
                        .model = si.model,
                        .provider = si.provider,
                        .effort = si.effort,
                    },
                }) catch null;
                if (ce) |c| self.stream_hub.publish_raw(.{ .entry = .{ .seq = 0, .sleeve_id = dh.sleeve_id, .entry = c } });
            },
            .prompt_ack => |pa| {
                self.commit(.{ .prompt_acked = .{
                    .sleeve_id = dh.sleeve_id,
                    .g = dh.g,
                    .request_id = pa.request_id,
                    .confirmed = (pa.ack == .confirmed),
                    .detail = pa.detail,
                } }) catch {};
            },
            .turn_started => |ts| {
                dh.turn_id += 1;
                self.commit(.{ .turn_started = .{
                    .sleeve_id = dh.sleeve_id,
                    .g = dh.g,
                    .request_id = ts.request_id,
                } }) catch {};
                const ce = self.transcripts.append(dh.sleeve_id, dh.g, dh.turn_id, ev.time_ms, .{
                    .turn_started = .{ .request_id = ts.request_id },
                }) catch null;
                if (ce) |c| self.stream_hub.publish_raw(.{ .entry = .{ .seq = 0, .sleeve_id = dh.sleeve_id, .entry = c } });
            },
            .assistant_text => |at| {
                const ce = self.transcripts.append(dh.sleeve_id, dh.g, dh.turn_id, ev.time_ms, .{
                    .assistant_text = .{
                        .id = at.id,
                        .text = at.text,
                        .delta = at.delta,
                    },
                }) catch null;
                if (ce) |c| self.stream_hub.publish_raw(.{ .entry = .{ .seq = 0, .sleeve_id = dh.sleeve_id, .entry = c } });
            },
            .reasoning => |rs| {
                const ce = self.transcripts.append(dh.sleeve_id, dh.g, dh.turn_id, ev.time_ms, .{
                    .reasoning = .{ .id = rs.id, .text = rs.text, .delta = rs.delta },
                }) catch null;
                if (ce) |c| self.stream_hub.publish_raw(.{ .entry = .{ .seq = 0, .sleeve_id = dh.sleeve_id, .entry = c } });
            },
            .tool_call => |tc| {
                self.commit(.{ .tool_activity = .{
                    .sleeve_id = dh.sleeve_id,
                    .g = dh.g,
                    .tool_sig = tc.name,
                } }) catch {};
                const ce = self.transcripts.append(dh.sleeve_id, dh.g, dh.turn_id, ev.time_ms, .{
                    .tool_call = .{
                        .id = tc.id,
                        .tool = map_tool_kind(tc.tool),
                        .name = tc.name,
                        .summary = tc.summary,
                    },
                }) catch null;
                if (ce) |c| self.stream_hub.publish_raw(.{ .entry = .{ .seq = 0, .sleeve_id = dh.sleeve_id, .entry = c } });
            },
            .tool_result => |tr| {
                const ce = self.transcripts.append(dh.sleeve_id, dh.g, dh.turn_id, ev.time_ms, .{
                    .tool_result = .{
                        .id = tr.id,
                        .status = switch (tr.status) {
                            .completed => .completed,
                            .failed => .failed,
                            .declined => .declined,
                            .cancelled => .cancelled,
                        },
                        .output = tr.output,
                        .truncated = tr.truncated,
                        .duration_ms = tr.duration_ms,
                    },
                }) catch null;
                if (ce) |c| self.stream_hub.publish_raw(.{ .entry = .{ .seq = 0, .sleeve_id = dh.sleeve_id, .entry = c } });
            },
            .turn_settled => |ts| {
                const end_m: model.TurnEnd = switch (ts.stop) {
                    .end_turn => .end_turn,
                    .cancelled => .cancelled,
                    .@"error" => .@"error",
                    .max_turns => .max_turns,
                    .other => .other,
                };
                self.commit(.{ .turn_settled = .{
                    .sleeve_id = dh.sleeve_id,
                    .g = dh.g,
                    .turn_end = end_m,
                    .context_tokens = ts.usage.context_tokens orelse 0,
                    .context_window = ts.usage.context_window orelse 0,
                } }) catch {};
                const ce = self.transcripts.append(dh.sleeve_id, dh.g, dh.turn_id, ev.time_ms, .{
                    .turn_settled = .{
                        .stop = switch (ts.stop) {
                            .end_turn => .end_turn,
                            .cancelled => .cancelled,
                            .@"error" => .@"error",
                            .max_turns => .max_turns,
                            .other => .other,
                        },
                        .native_stop = ts.native_stop,
                        .usage = .{
                            .input = ts.usage.input,
                            .output = ts.usage.output,
                            .cache_read = ts.usage.cache_read,
                            .cache_write = ts.usage.cache_write,
                            .cost_usd = ts.usage.cost_usd,
                            .context_tokens = ts.usage.context_tokens,
                            .context_window = ts.usage.context_window,
                        },
                        .duration_ms = ts.duration_ms,
                    },
                }) catch null;
                if (ce) |c| self.stream_hub.publish_raw(.{ .entry = .{ .seq = 0, .sleeve_id = dh.sleeve_id, .entry = c } });
            },
            .approval_request => |ar| {
                self.commit(.{ .approval_requested = .{
                    .approval_id = ar.id,
                    .sleeve_id = dh.sleeve_id,
                    .g = dh.g,
                    .kind = .tool,
                    .title = ar.name,
                    .summary = ar.summary,
                } }) catch {};
            },
            .@"error" => |er| {
                const ce = self.transcripts.append(dh.sleeve_id, dh.g, dh.turn_id, ev.time_ms, .{
                    .@"error" = .{
                        .kind = switch (er.kind) {
                            .auth => .auth,
                            .quota => .quota,
                            .rate_limit => .rate_limit,
                            .protocol => .protocol,
                            .crash => .crash,
                            .other => .other,
                        },
                        .text = er.text,
                    },
                }) catch null;
                if (ce) |c| self.stream_hub.publish_raw(.{ .entry = .{ .seq = 0, .sleeve_id = dh.sleeve_id, .entry = c } });
            },
            .compaction => |cp| {
                self.commit(.{ .compacted = .{
                    .sleeve_id = dh.sleeve_id,
                    .g = dh.g,
                    .tokens_after = cp.tokens_after orelse 0,
                } }) catch {};
                const ce = self.transcripts.append(dh.sleeve_id, dh.g, dh.turn_id, ev.time_ms, .{
                    .compaction = .{
                        .trigger = cp.trigger,
                        .tokens_before = cp.tokens_before,
                        .tokens_after = cp.tokens_after,
                    },
                }) catch null;
                if (ce) |c| self.stream_hub.publish_raw(.{ .entry = .{ .seq = 0, .sleeve_id = dh.sleeve_id, .entry = c } });
            },
            else => {},
        }
    }

    fn on_driver_exit(ctx: ?*anyopaque, g: u64, exit_code: ?u8, stderr_tail: []const u8) void {
        const dh: *DriverHandle = @ptrCast(@alignCast(ctx orelse return));
        const self = dh.rt;
        const at_ms = std.Io.Clock.real.now(self.io).toMilliseconds();
        const ce = self.transcripts.append(dh.sleeve_id, dh.g, dh.turn_id, at_ms, .{
            .exited = .{
                .code = if (exit_code) |c| @intCast(c) else null,
                .stderr_tail = stderr_tail,
            },
        }) catch null;
        if (ce) |c| self.stream_hub.publish_raw(.{ .entry = .{ .seq = 0, .sleeve_id = dh.sleeve_id, .entry = c } });

        const cause: model.ExitCause = if (exit_code == 0) .clean else .crash;
        self.commit(.{ .exited = .{
            .sleeve_id = dh.sleeve_id,
            .g = g,
            .cause = cause,
            .requested = self.stopping.load(.acquire),
        } }) catch {};
    }

    fn dispatch_job_async(self: *Runtime, job_id: []const u8, node_id: []const u8) void {
        const Arg = struct { rt: *Runtime, job_id: []const u8, node_id: []const u8 };
        const arg = self.gpa.create(Arg) catch return;
        arg.* = .{
            .rt = self,
            .job_id = self.gpa.dupe(u8, job_id) catch {
                self.gpa.destroy(arg);
                return;
            },
            .node_id = self.gpa.dupe(u8, node_id) catch {
                self.gpa.free(arg.job_id);
                self.gpa.destroy(arg);
                return;
            },
        };
        const t = std.Thread.spawn(.{}, struct {
            fn run(a_ptr: *Arg) void {
                defer {
                    a_ptr.rt.gpa.free(a_ptr.job_id);
                    a_ptr.rt.gpa.free(a_ptr.node_id);
                    a_ptr.rt.gpa.destroy(a_ptr);
                }
                var arena_st = std.heap.ArenaAllocator.init(a_ptr.rt.gpa);
                defer arena_st.deinit();
                const arena = arena_st.allocator();

                var sj: msg.SubmitJob = undefined;
                var found = false;
                {
                    a_ptr.rt.meta_mu.lockUncancelable(a_ptr.rt.io);
                    if (a_ptr.rt.job_meta.get(a_ptr.job_id)) |jm| {
                        sj = .{
                            .job_id = try_dupe(arena, a_ptr.job_id),
                            .argv = dupe_str_slice(arena, jm.argv),
                            .cwd = if (jm.cwd) |c| try_dupe(arena, c) else null,
                            .artifacts = dupe_str_slice(arena, jm.decl_artifacts),
                        };
                        found = true;
                    }
                    a_ptr.rt.meta_mu.unlock(a_ptr.rt.io);
                }
                if (!found) return;

                _ = a_ptr.rt.fleet_srv.submit_job(arena, a_ptr.node_id, sj) catch |err| {
                    a_ptr.rt.commit(.{ .job_failed = .{
                        .job_id = a_ptr.job_id,
                        .attempt = 1,
                        .err = @errorName(err),
                    } }) catch {};
                    return;
                };
                a_ptr.rt.commit(.{ .job_submitted = .{
                    .job_id = a_ptr.job_id,
                    .node_id = a_ptr.node_id,
                    .attempt = 1,
                } }) catch {};
            }
        }.run, .{arg}) catch {
            self.gpa.free(arg.job_id);
            self.gpa.free(arg.node_id);
            self.gpa.destroy(arg);
            return;
        };
        t.detach();
    }

    fn verify_job_async(self: *Runtime, job_id: []const u8, node_id: []const u8) void {
        _ = self;
        _ = job_id;
        _ = node_id;
    }

    fn cancel_job_async(self: *Runtime, job_id: []const u8, node_id: []const u8) void {
        const Arg = struct { rt: *Runtime, job_id: []const u8, node_id: []const u8 };
        const arg = self.gpa.create(Arg) catch return;
        arg.* = .{
            .rt = self,
            .job_id = self.gpa.dupe(u8, job_id) catch {
                self.gpa.destroy(arg);
                return;
            },
            .node_id = self.gpa.dupe(u8, node_id) catch {
                self.gpa.free(arg.job_id);
                self.gpa.destroy(arg);
                return;
            },
        };
        const t = std.Thread.spawn(.{}, struct {
            fn run(a_ptr: *Arg) void {
                defer {
                    a_ptr.rt.gpa.free(a_ptr.job_id);
                    a_ptr.rt.gpa.free(a_ptr.node_id);
                    a_ptr.rt.gpa.destroy(a_ptr);
                }
                var arena_st = std.heap.ArenaAllocator.init(a_ptr.rt.gpa);
                defer arena_st.deinit();
                _ = a_ptr.rt.fleet_srv.cancel_job(arena_st.allocator(), a_ptr.node_id, a_ptr.job_id) catch {};
                a_ptr.rt.commit(.{ .job_cancelled = .{ .job_id = a_ptr.job_id } }) catch {};
            }
        }.run, .{arg}) catch {
            self.gpa.free(arg.job_id);
            self.gpa.free(arg.node_id);
            self.gpa.destroy(arg);
            return;
        };
        t.detach();
    }

    fn poll_inflight_jobs(self: *Runtime) void {
        var arena_st = std.heap.ArenaAllocator.init(self.gpa);
        defer arena_st.deinit();
        const a = arena_st.allocator();

        const Pair = struct { id: []const u8, node_id: []const u8 };
        var pairs: std.ArrayList(Pair) = .empty;
        {
            self.state_mu.lockUncancelable(self.io);
            defer self.state_mu.unlock(self.io);
            var jit = self.core_state.jobs.iterator();
            while (jit.next()) |je| {
                const j = je.value_ptr.*;
                if (!j.state.is_terminal() and j.assigned_node != null) {
                    pairs.append(a, .{
                        .id = a.dupe(u8, j.id) catch continue,
                        .node_id = a.dupe(u8, j.assigned_node.?) catch continue,
                    }) catch continue;
                }
            }
        }

        for (pairs.items) |pr| {
            const info = self.fleet_srv.job_status(a, pr.node_id, pr.id) catch continue;
            if (info.log_bytes > 0) {
                if (self.fleet_srv.fetch_log(a, pr.node_id, pr.id, 0, 1024 * 1024)) |lres| {
                    var lp_opt: ?[]const u8 = null;
                    self.meta_mu.lockUncancelable(self.io);
                    if (self.job_meta.getPtr(pr.id)) |jm| {
                        lp_opt = a.dupe(u8, jm.log_path) catch null;
                        jm.log_bytes = lres.next_offset;
                    }
                    self.meta_mu.unlock(self.io);
                    if (lp_opt) |lp| {
                        std.Io.Dir.cwd().writeFile(self.io, .{ .sub_path = lp, .data = lres.data }) catch {};
                    }
                } else |_| {}
            }

            if (info.state == .succeeded or info.state == .failed or info.state == .cancelled or info.state == .interrupted) {
                var arts_list: std.ArrayList(schema.JobArtifact) = .empty;
                for (info.artifacts) |ar| {
                    if (self.fleet_srv.fetch_artifact(a, pr.node_id, pr.id, ar.path, self.hub_home.artifacts_path)) |verified| {
                        arts_list.append(a, .{
                            .path = self.core_state.dupe_str(verified.path) catch ar.path,
                            .size_bytes = verified.size_bytes,
                            .sha256 = if (verified.sha256) |s| (self.core_state.dupe_str(s) catch s) else null,
                        }) catch {};
                    } else |_| {}
                }
                {
                    self.meta_mu.lockUncancelable(self.io);
                    if (self.job_meta.getPtr(pr.id)) |jm| {
                        const a_alloc = self.core_state.alloc();
                        if (a_alloc.alloc(schema.JobArtifact, arts_list.items.len)) |slice| {
                            @memcpy(slice, arts_list.items);
                            jm.artifacts = slice;
                        } else |_| {}
                        jm.ended_at_ms = std.Io.Clock.real.now(self.io).toMilliseconds();
                    }
                    self.meta_mu.unlock(self.io);
                }
                if (info.state == .cancelled) {
                    self.commit(.{ .job_cancelled = .{ .job_id = pr.id } }) catch {};
                } else if (info.state == .interrupted) {
                    self.commit(.{ .job_interrupted = .{ .job_id = pr.id, .attempt = 1 } }) catch {};
                } else if (info.exit_code) |ec| {
                    self.commit(.{ .job_completed = .{ .job_id = pr.id, .attempt = 1, .exit_code = ec } }) catch {};
                } else {
                    self.commit(.{ .job_failed = .{ .job_id = pr.id, .attempt = 1, .err = info.@"error" orelse "failed" } }) catch {};
                }
            }
        }
    }

    fn tick_loop(self: *Runtime) void {
        var ticks: u64 = 0;
        while (!self.stopping.load(.acquire)) {
            self.io.sleep(.fromMilliseconds(1_000), .awake) catch break;
            if (self.stopping.load(.acquire)) break;
            ticks += 1;
            self.poll_inflight_jobs();
            if (ticks % 5 == 0) {
                self.fleet_srv.sweep_stale(20_000);
                self.poll_sleeve_devices();
            }
            if (ticks % 10 == 0) {
                self.commit(.{ .tick = .{} }) catch {};
            }
        }
    }

    fn hook_allowed_peers(ctx: ?*anyopaque, arena: std.mem.Allocator) []fleet.AllowedPeer {
        const self: *Runtime = @ptrCast(@alignCast(ctx orelse return &.{}));
        self.state_mu.lockUncancelable(self.io);
        defer self.state_mu.unlock(self.io);

        var out: std.ArrayList(fleet.AllowedPeer) = .empty;
        var it = self.core_state.nodes.iterator();
        while (it.next()) |e| {
            const n = e.value_ptr.*;
            if (n.approved and !n.removed) {
                out.append(arena, .{
                    .node_id = arena.dupe(u8, n.id) catch continue,
                    .name = arena.dupe(u8, n.name) catch continue,
                    .pub_key = n.pub_key,
                }) catch continue;
            }
        }
        return out.items;
    }

    fn hook_connected(ctx: ?*anyopaque, node_id: []const u8, hello: msg.Hello) void {
        const self: *Runtime = @ptrCast(@alignCast(ctx orelse return));
        self.store_telemetry(node_id, hello);
        const arch = if (hello.inventory) |inv| inv.arch else "unknown";
        self.commit(.{ .node_connected = .{
            .node_id = node_id,
            .arch = arch,
            .version = hello.node_version,
        } }) catch {};
    }

    fn hook_telemetry(ctx: ?*anyopaque, node_id: []const u8, hello: msg.Hello) void {
        const self: *Runtime = @ptrCast(@alignCast(ctx orelse return));
        self.store_telemetry(node_id, hello);
        const p: model.ProbeResult = switch (hello.presence.state) {
            .present => .present,
            .away => .away,
            .unknown => .unknown,
        };
        self.commit(.{ .node_presence_updated = .{
            .node_id = node_id,
            .presence = p,
        } }) catch {};
    }

    fn hook_disconnected(ctx: ?*anyopaque, node_id: []const u8, reason: []const u8) void {
        const self: *Runtime = @ptrCast(@alignCast(ctx orelse return));
        self.commit(.{ .node_disconnected = .{
            .node_id = node_id,
            .reason = reason,
        } }) catch {};
    }

    fn store_telemetry(self: *Runtime, node_id: []const u8, hello: msg.Hello) void {
        const a = self.core_state.alloc();
        self.meta_mu.lockUncancelable(self.io);
        defer self.meta_mu.unlock(self.io);

        const gop = self.node_tele.getOrPut(self.gpa, node_id) catch return;
        if (!gop.found_existing) {
            gop.key_ptr.* = a.dupe(u8, node_id) catch return;
            gop.value_ptr.* = NodeTelemetry{};
        }
        var tel = gop.value_ptr.*;
        if (hello.inventory) |inv| {
            const tc: []msg.Toolchain = a.alloc(msg.Toolchain, inv.toolchains.len) catch &.{};
            for (inv.toolchains, 0..) |t, i| {
                tc[i] = .{
                    .name = a.dupe(u8, t.name) catch "",
                    .version = a.dupe(u8, t.version) catch "",
                };
            }
            const gpus: []msg.Gpu = a.alloc(msg.Gpu, inv.gpus.len) catch &.{};
            for (inv.gpus, 0..) |g, i| {
                gpus[i] = .{
                    .name = a.dupe(u8, g.name) catch "",
                    .driver = a.dupe(u8, g.driver) catch "",
                    .vram_bytes = g.vram_bytes,
                    .util_pct = g.util_pct,
                };
            }
            const disks: []msg.Disk = a.alloc(msg.Disk, inv.disks.len) catch &.{};
            for (inv.disks, 0..) |d, i| {
                disks[i] = .{
                    .mount = a.dupe(u8, d.mount) catch "",
                    .size_bytes = d.size_bytes,
                    .free_bytes = d.free_bytes,
                };
            }
            tel.inventory = .{
                .hostname = a.dupe(u8, inv.hostname) catch "",
                .os = a.dupe(u8, inv.os) catch "",
                .os_version = a.dupe(u8, inv.os_version) catch "",
                .arch = a.dupe(u8, inv.arch) catch "",
                .cpu_model = a.dupe(u8, inv.cpu_model) catch "",
                .cpu_count = inv.cpu_count,
                .ram_bytes = inv.ram_bytes,
                .gpus = gpus,
                .disks = disks,
                .toolchains = tc,
                .collected_at_ms = inv.collected_at_ms,
            };
        }
        if (hello.sample) |smp| tel.sample = smp;
        tel.presence = .{
            .state = hello.presence.state,
            .idle_secs = hello.presence.idle_secs,
            .locked = hello.presence.locked,
            .source = a.dupe(u8, hello.presence.source) catch "",
            .sampled_at_ms = hello.presence.sampled_at_ms,
        };
        gop.value_ptr.* = tel;
    }

    pub fn record_hub_event(
        self: *Runtime,
        kind: schema.EventKind,
        source: []const u8,
        target: ?[]const u8,
        text: []const u8,
        level: schema.Level,
    ) void {
        const now = std.Io.Clock.real.now(self.io).toMilliseconds();
        self.meta_mu.lockUncancelable(self.io);
        const seq = self.next_hub_event_seq;
        self.next_hub_event_seq += 1;
        const ev: schema.HubEvent = .{
            .seq = seq,
            .at_ms = .{ .ms = now },
            .kind = kind,
            .source = self.gpa.dupe(u8, source) catch "",
            .target = if (target) |t| (self.gpa.dupe(u8, t) catch null) else null,
            .text = self.gpa.dupe(u8, text) catch "",
            .level = level,
        };
        if (self.hub_events.items.len >= 512) {
            const old = self.hub_events.orderedRemove(0);
            self.gpa.free(old.source);
            if (old.target) |t| self.gpa.free(t);
            self.gpa.free(old.text);
        }
        self.hub_events.append(self.gpa, ev) catch {};
        self.meta_mu.unlock(self.io);
        self.stream_hub.publish_raw(.{ .event = .{ .seq = 0, .event = ev } });
    }

    fn publish_event_updates(self: *Runtime, arena: std.mem.Allocator, ev: event.Event) void {
        switch (ev) {
            inline .sleeve_created, .provisioned, .provision_failed, .start_requested, .launched, .session_bound, .prompt_enqueued, .prompt_delivered, .turn_started, .turn_settled, .stop_requested, .stopped, .resleeve_requested, .parked => |s| {
                if (self.view_sleeve(arena, s.sleeve_id) catch null) |sv| {
                    self.stream_hub.publish_raw(.{ .sleeve = .{ .seq = 0, .sleeve = sv } });
                }
            },
            .removed => |r| {
                self.stream_hub.publish_raw(.{ .sleeve_removed = .{ .seq = 0, .id = r.sleeve_id } });
            },
            inline .node_approved, .node_connected, .node_disconnected, .node_renamed, .node_jobs_updated, .node_presence_updated => |n| {
                if (self.view_node(arena, n.node_id) catch null) |nv| {
                    self.stream_hub.publish_raw(.{ .node = .{ .seq = 0, .node = nv } });
                }
            },
            .node_removed => |n| {
                self.stream_hub.publish_raw(.{ .node_removed = .{ .seq = 0, .id = n.node_id } });
            },
            inline .job_queued, .job_submitted, .job_completed, .job_failed, .job_interrupted, .job_lost, .job_cancel_requested, .job_cancelled, .job_abandoned => |j| {
                if (self.view_job(arena, j.job_id) catch null) |jv| {
                    self.stream_hub.publish_raw(.{ .job = .{ .seq = 0, .job = jv } });
                }
            },
            inline .approval_requested, .approval_decided => |a| {
                if (self.view_approval(arena, a.approval_id) catch null) |av| {
                    self.stream_hub.publish_raw(.{ .approval = .{ .seq = 0, .approval = av } });
                }
            },
            .approval_expired => |a| {
                if (self.view_approval(arena, a.approval_id) catch null) |av| {
                    self.stream_hub.publish_raw(.{ .approval = .{ .seq = 0, .approval = av } });
                }
            },
            .rollout_started => |r| {
                self.record_hub_event(.node, "rollout", r.rollout_id, r.version, .info);
            },
            .rollout_node_result => |r| {
                self.record_hub_event(.node, r.node_id, r.rollout_id, r.detail, if (r.ok) .info else .warn);
            },
            .rollout_cancelled => |r| {
                self.record_hub_event(.node, "rollout", r.rollout_id, "cancelled", .warn);
            },
            else => {},
        }
    }

    pub fn view_hub_info(self: *Runtime, arena: std.mem.Allocator) !schema.HubInfo {
        var hn_buf: [256]u8 = undefined;
        const name = if (comptime builtin.os.tag != .windows) blk: {
            var pbuf: [std.posix.HOST_NAME_MAX]u8 = undefined;
            if (std.posix.gethostname(&pbuf)) |hn| {
                @memcpy(hn_buf[0..hn.len], hn);
                break :blk hn_buf[0..hn.len];
            } else |_| break :blk self.hub_home.config.hub_name;
        } else self.hub_home.config.hub_name;
        return .{
            .name = try arena.dupe(u8, name),
            .version = version.version,
            .started_at_ms = .{ .ms = self.started_at_ms },
            .restore_error = self.restore_error,
            .job_artifact_max_bytes = 64 << 20,
        };
    }

    pub fn view_sleeve(self: *Runtime, arena: std.mem.Allocator, sleeve_id: []const u8) !?schema.Sleeve {
        self.state_mu.lockUncancelable(self.io);
        defer self.state_mu.unlock(self.io);
        const sl = self.core_state.sleeves.get(sleeve_id) orelse return null;
        return try sleeve_to_schema(arena, sl);
    }

    pub fn view_state(self: *Runtime, arena: std.mem.Allocator) !schema.HubState {
        var sleeves_list: std.ArrayList(schema.Sleeve) = .empty;
        var approvals_list: std.ArrayList(schema.Approval) = .empty;
        var nodes_list: std.ArrayList(schema.Node) = .empty;
        var jobs_list: std.ArrayList(schema.Job) = .empty;

        {
            self.state_mu.lockUncancelable(self.io);
            defer self.state_mu.unlock(self.io);

            var sit = self.core_state.sleeves.iterator();
            while (sit.next()) |e| {
                if (e.value_ptr.lifecycle == .removed) continue;
                try sleeves_list.append(arena, try sleeve_to_schema(arena, e.value_ptr.*));
            }
            var ait = self.core_state.approvals.iterator();
            while (ait.next()) |e| {
                try approvals_list.append(arena, try approval_to_schema(arena, e.value_ptr.*));
            }
            var nit = self.core_state.nodes.iterator();
            while (nit.next()) |e| {
                if (e.value_ptr.removed or !e.value_ptr.approved) continue;
                try nodes_list.append(arena, try self.node_to_schema_unlocked(arena, e.value_ptr.*));
            }
            var jit = self.core_state.jobs.iterator();
            while (jit.next()) |e| {
                try jobs_list.append(arena, try self.job_to_schema_unlocked(arena, e.value_ptr.*));
            }
        }

        var ws_list: std.ArrayList(schema.Workspace) = .empty;
        var ev_list: std.ArrayList(schema.HubEvent) = .empty;
        var q_list: []schema.Quota = &.{};
        {
            self.meta_mu.lockUncancelable(self.io);
            defer self.meta_mu.unlock(self.io);
            var wit = self.workspaces.iterator();
            while (wit.next()) |e| {
                try ws_list.append(arena, .{
                    .id = try arena.dupe(u8, e.value_ptr.id),
                    .name = try arena.dupe(u8, e.value_ptr.name),
                    .path = try arena.dupe(u8, e.value_ptr.path),
                });
            }
            try ev_list.appendSlice(arena, self.hub_events.items);
            q_list = self.cached_quotas;
        }

        return .{
            .seq = self.stream_hub.current_seq(),
            .hub = try self.view_hub_info(arena),
            .workspaces = ws_list.items,
            .sleeves = sleeves_list.items,
            .approvals = approvals_list.items,
            .nodes = nodes_list.items,
            .jobs = jobs_list.items,
            .quotas = q_list,
            .events = ev_list.items,
        };
    }

    pub fn view_node(self: *Runtime, arena: std.mem.Allocator, node_id: []const u8) !?schema.Node {
        self.state_mu.lockUncancelable(self.io);
        defer self.state_mu.unlock(self.io);
        const nr = self.core_state.nodes.get(node_id) orelse return null;
        if (nr.removed or !nr.approved) return null;
        return try self.node_to_schema_unlocked(arena, nr);
    }

    pub fn view_job(self: *Runtime, arena: std.mem.Allocator, job_id: []const u8) !?schema.Job {
        self.state_mu.lockUncancelable(self.io);
        defer self.state_mu.unlock(self.io);
        const jr = self.core_state.jobs.get(job_id) orelse return null;
        return try self.job_to_schema_unlocked(arena, jr);
    }

    pub fn view_approval(self: *Runtime, arena: std.mem.Allocator, approval_id: []const u8) !?schema.Approval {
        self.state_mu.lockUncancelable(self.io);
        defer self.state_mu.unlock(self.io);
        const ar = self.core_state.approvals.get(approval_id) orelse return null;
        return try approval_to_schema(arena, ar);
    }

    fn node_to_schema_unlocked(self: *Runtime, arena: std.mem.Allocator, n: state.NodeRecord) !schema.Node {
        self.meta_mu.lockUncancelable(self.io);
        const tel = self.node_tele.get(n.id);
        self.meta_mu.unlock(self.io);

        var inv_s: ?schema.Inventory = null;
        var smp_s: ?schema.Sample = null;
        var pres_s: schema.Presence = .{
            .state = switch (n.presence) {
                .present => .present,
                .away => .away,
                .unknown => .unknown,
            },
        };

        if (tel) |t| {
            if (t.inventory) |inv| {
                const tcs = try arena.alloc(schema.Toolchain, inv.toolchains.len);
                for (inv.toolchains, 0..) |tc, i| tcs[i] = .{ .name = tc.name, .version = tc.version };
                const gpus = try arena.alloc(schema.Gpu, inv.gpus.len);
                for (inv.gpus, 0..) |g, i| gpus[i] = .{
                    .name = g.name,
                    .driver = g.driver,
                    .vram_bytes = g.vram_bytes,
                    .util_pct = g.util_pct,
                };
                const disks = try arena.alloc(schema.Disk, inv.disks.len);
                for (inv.disks, 0..) |d, i| disks[i] = .{
                    .mount = d.mount,
                    .size_bytes = d.size_bytes,
                    .free_bytes = d.free_bytes,
                };
                inv_s = .{
                    .hostname = inv.hostname,
                    .os = inv.os,
                    .os_version = inv.os_version,
                    .arch = inv.arch,
                    .cpu_model = inv.cpu_model,
                    .cpu_count = inv.cpu_count,
                    .ram_bytes = inv.ram_bytes,
                    .gpus = gpus,
                    .disks = disks,
                    .toolchains = tcs,
                    .collected_at_ms = .{ .ms = inv.collected_at_ms },
                };
            }
            if (t.sample) |smp| {
                smp_s = .{
                    .sampled_at_ms = .{ .ms = smp.sampled_at_ms },
                    .cpu_pct = smp.cpu_pct,
                    .load1 = smp.load1,
                    .mem_avail_bytes = smp.mem_avail_bytes,
                    .swap_used_bytes = smp.swap_used_bytes,
                };
            }
            pres_s = .{
                .state = switch (t.presence.state) {
                    .present => .present,
                    .away => .away,
                    .unknown => .unknown,
                },
                .idle_secs = t.presence.idle_secs,
                .locked = t.presence.locked,
                .source = t.presence.source,
                .sampled_at_ms = if (t.presence.sampled_at_ms) |m| .{ .ms = m } else null,
            };
        }

        return .{
            .id = try arena.dupe(u8, n.id),
            .name = try arena.dupe(u8, n.name),
            .online = n.online,
            .last_seen_ms = .{ .ms = n.last_seen_ms },
            .node_version = try arena.dupe(u8, n.version),
            .inventory = inv_s,
            .sample = smp_s,
            .presence = pres_s,
            .jobs = switch (n.jobs_policy) {
                .any => .any,
                .owner => .owner,
                .none => .none,
            },
        };
    }

    fn job_to_schema_unlocked(self: *Runtime, arena: std.mem.Allocator, j: state.JobRecord) !schema.Job {
        self.meta_mu.lockUncancelable(self.io);
        const jm = self.job_meta.get(j.id);
        self.meta_mu.unlock(self.io);

        const target: schema.JobTarget = if (jm) |m|
            m.target
        else if (j.target_node) |nid|
            .{ .node = .{ .node_id = try arena.dupe(u8, nid) } }
        else
            .{ .arch = .{ .arch = try arena.dupe(u8, j.target_arch orelse "x86_64") } };

        return .{
            .id = try arena.dupe(u8, j.id),
            .node_id = if (j.assigned_node) |an| try arena.dupe(u8, an) else null,
            .target = target,
            .argv = if (jm) |m| dupe_str_slice(arena, m.argv) else &.{},
            .state = switch (j.state) {
                .queued => .queued,
                .submitting => .submitting,
                .submit_uncertain => .submit_uncertain,
                .running => .running,
                .collecting => .collecting,
                .succeeded => .succeeded,
                .failed => .failed,
                .interrupted => .interrupted,
                .lost => .lost,
                .cancelling => .cancelling,
                .cancelled => .cancelled,
                .abandoned => .abandoned,
            },
            .requested_by = try arena.dupe(u8, j.requested_by),
            .attempt = j.attempt,
            .even_if_present = j.even_if_present,
            .on_present = switch (j.on_present) {
                .idle => .idle,
                .kill => .kill,
            },
            .approval_id = if (jm) |m| m.approval_id else null,
            .queued_at_ms = .{ .ms = j.queued_at_ms },
            .started_at_ms = if (jm) |m| (if (m.started_at_ms) |s| .{ .ms = s } else null) else null,
            .ended_at_ms = if (jm) |m| (if (m.ended_at_ms) |e| .{ .ms = e } else null) else null,
            .exit_code = j.exit_code,
            .@"error" = if (j.err) |e| try arena.dupe(u8, e) else null,
            .held_reason = if (jm) |m| m.held_reason else null,
            .log_bytes = if (jm) |m| m.log_bytes else 0,
            .cwd = if (jm) |m| m.cwd else null,
            .artifacts = if (jm) |m| m.artifacts else &.{},
        };
    }

    pub fn register_worktree_base(self: *Runtime, sleeve_id: []const u8, base_repo: []const u8) !void {
        self.meta_mu.lockUncancelable(self.io);
        defer self.meta_mu.unlock(self.io);
        const v_dup = try self.gpa.dupe(u8, base_repo);
        errdefer self.gpa.free(v_dup);
        const gop = try self.worktree_bases.getOrPut(self.gpa, sleeve_id);
        if (gop.found_existing) {
            self.gpa.free(gop.value_ptr.*);
            gop.value_ptr.* = v_dup;
        } else {
            errdefer _ = self.worktree_bases.remove(sleeve_id);
            gop.key_ptr.* = try self.gpa.dupe(u8, sleeve_id);
            gop.value_ptr.* = v_dup;
        }
    }

    pub fn takeover_sleeve(self: *Runtime, arena: std.mem.Allocator, sleeve_id: []const u8) !void {
        _ = arena;
        var cur_g: model.Gen = 0;
        var need_stop = false;
        {
            self.state_mu.lockUncancelable(self.io);
            defer self.state_mu.unlock(self.io);
            const sl = self.core_state.sleeves.get(sleeve_id) orelse return error.NotFound;
            if (sl.lifecycle == .removed) return error.InvalidState;
            cur_g = if (sl.inc) |inc| inc.g else sl.next_g;
            need_stop = sl.lifecycle != .stopped and sl.lifecycle != .parked;
        }

        const at_ms = std.Io.Clock.real.now(self.io).toMilliseconds();
        const ce = self.transcripts.append(sleeve_id, cur_g, 0, at_ms, .{
            .notice = .{
                .level = .info,
                .code = "takeover",
                .text = "terminal takeover initiated; releasing managed driver",
            },
        }) catch null;
        if (ce) |c| {
            self.stream_hub.publish_raw(.{ .entry = .{ .seq = 0, .sleeve_id = sleeve_id, .entry = c } });
        }
        self.record_hub_event(.sleeve, "takeover", sleeve_id, "terminal takeover handoff", .info);

        if (need_stop) {
            brief.write_handoff(
                self.gpa,
                self.io,
                self.hub_home.briefs_path,
                sleeve_id,
                cur_g,
                .manual,
                "Terminal takeover handoff",
            ) catch {};
            try self.commit(.{ .stop_requested = .{ .sleeve_id = sleeve_id, .by = .owner } });
        }
    }

    pub const D9Action = enum {
        spawn_sleeve,
        remove_sleeve,
        run_job,
        git_push,
    };

    pub fn gate_d9(
        self: *Runtime,
        arena: std.mem.Allocator,
        principal: ?home.Principal,
        action: D9Action,
        summary: []const u8,
    ) !?[]const u8 {
        if (principal == null or principal.? != .envoy) return null;

        var envoy_d9 = false;
        var cur_g: model.Gen = 0;
        var cur_turn: u64 = 0;
        {
            self.state_mu.lockUncancelable(self.io);
            defer self.state_mu.unlock(self.io);
            if (self.core_state.sleeves.get("envoy")) |sl| {
                envoy_d9 = sl.d9;
                if (sl.inc) |inc| {
                    cur_g = inc.g;
                    cur_turn = inc.turn;
                }
            }
        }
        if (!envoy_d9) return null;

        if (action == .spawn_sleeve) {
            self.meta_mu.lockUncancelable(self.io);
            defer self.meta_mu.unlock(self.io);
            if (self.d9_turn_id != cur_turn) {
                self.d9_turn_id = cur_turn;
                self.d9_turn_spawns = 0;
            }
            if (self.d9_turn_spawns < self.hub_home.config.d9_spawn_limit) {
                self.d9_turn_spawns += 1;
                return null;
            }
        }

        const app_id = try self.gen_id(arena, "d9");
        const title: []const u8 = switch (action) {
            .spawn_sleeve => "Envoy spawn over D9 limit",
            .remove_sleeve => "Envoy remove sleeve (D9)",
            .run_job => "Envoy run job (D9)",
            .git_push => "Envoy git push (D9)",
        };
        try self.commit(.{ .approval_requested = .{
            .approval_id = app_id,
            .sleeve_id = "envoy",
            .g = cur_g,
            .kind = .action,
            .title = title,
            .summary = summary,
            .is_d9 = true,
        } });
        self.record_hub_event(.approval, "d9", "envoy", summary, .warn);

        const wait_secs = @max(@as(u64, 1), @min(self.hub_home.config.d9_approval_timeout_secs, 600));
        const deadline = std.Io.Clock.real.now(self.io).toMilliseconds() + @as(i64, @intCast(wait_secs * 1_000));
        var cursor: u64 = 0;
        while (true) {
            {
                self.state_mu.lockUncancelable(self.io);
                defer self.state_mu.unlock(self.io);
                if (self.core_state.approvals.get(app_id)) |ap| {
                    switch (ap.state) {
                        .approved => return try arena.dupe(u8, app_id),
                        .denied, .cancelled, .fenced, .abandoned, .expired => return error.ApprovalDenied,
                        .pending => {},
                    }
                } else return error.ApprovalDenied;
            }
            const now = std.Io.Clock.real.now(self.io).toMilliseconds();
            if (now >= deadline) {
                self.commit(.{ .approval_expired = .{ .approval_id = app_id } }) catch {};
                return error.ApprovalTimeout;
            }
            const ws = try self.stream_hub.wait_since(arena, cursor, @min(500, deadline - now));
            cursor = ws.last_seq;
        }
    }

    pub fn update_harnesses(
        self: *Runtime,
        arena: std.mem.Allocator,
        programs: []const []const u8,
    ) ![]schema.HarnessUpdate {
        const rep = try doctor.run_checks(arena, self.io, self.env.get("HOME"));
        var list: std.ArrayList(schema.HarnessUpdate) = .empty;
        for (rep.harnesses) |h| {
            if (programs.len > 0) {
                var matched = false;
                for (programs) |want| {
                    if (std.mem.eql(u8, want, h.name) or (h.path.len > 0 and std.mem.eql(u8, want, h.path))) {
                        matched = true;
                        break;
                    }
                }
                if (!matched) continue;
            }
            if (!h.present) {
                try list.append(arena, .{ .program = h.name, .state = .skipped, .detail = "not installed" });
                continue;
            }
            if (h.update.len == 0) {
                try list.append(arena, .{ .program = h.name, .state = .skipped, .detail = "no update command" });
                continue;
            }
            var in_use = false;
            {
                self.state_mu.lockUncancelable(self.io);
                defer self.state_mu.unlock(self.io);
                var sit = self.core_state.sleeves.iterator();
                while (sit.next()) |se| {
                    const sl = se.value_ptr.*;
                    if (sl.lifecycle == .running or sl.lifecycle == .launching or sl.lifecycle == .recovering) {
                        if (sl.inc) |inc| {
                            const match_h = switch (inc.harness) {
                                .claude => std.mem.eql(u8, h.name, "claude"),
                                .codex => std.mem.eql(u8, h.name, "codex"),
                                .grok => std.mem.eql(u8, h.name, "grok"),
                                .antigravity => std.mem.eql(u8, h.name, "antigravity"),
                                .pi_vendor, .pi_local => std.mem.eql(u8, h.name, "pi"),
                            };
                            if (match_h) {
                                in_use = true;
                                break;
                            }
                        }
                    }
                }
            }
            if (in_use) {
                try list.append(arena, .{ .program = h.name, .state = .deferred, .detail = "active sleeve using harness" });
                continue;
            }
            const res = std.process.run(arena, self.io, .{
                .argv = h.update,
                .stdout_limit = .limited(64 * 1024),
                .stderr_limit = .limited(64 * 1024),
            }) catch |err| {
                const detail = @errorName(err);
                try list.append(arena, .{ .program = h.name, .state = .skipped, .detail = detail });
                self.record_hub_event(.hub, "harness", h.name, detail, .warn);
                continue;
            };
            const raw = if (res.stdout.len > 0) res.stdout else res.stderr;
            const first_line = std.mem.trim(u8, std.mem.sliceTo(raw, '\n'), " \t\r");
            if (res.term.success()) {
                const detail = if (std.mem.find(u8, raw, "up to date") != null or
                    std.mem.find(u8, raw, "already") != null or
                    std.mem.find(u8, raw, "latest") != null)
                    "up to date"
                else if (first_line.len > 0)
                    try arena.dupe(u8, first_line)
                else
                    "updated";
                try list.append(arena, .{ .program = h.name, .state = .started, .detail = detail });
                self.record_hub_event(.hub, "harness", h.name, detail, .info);
            } else {
                const base = if (first_line.len > 0) first_line else "update command failed";
                const detail = try std.fmt.allocPrint(arena, "failed: {s}", .{base});
                try list.append(arena, .{ .program = h.name, .state = .skipped, .detail = detail });
                self.record_hub_event(.hub, "harness", h.name, detail, .warn);
            }
        }
        return list.items;
    }

    pub fn start_and_wait_node_rollout(
        self: *Runtime,
        arena: std.mem.Allocator,
        req: schema.UpdateNodesRequest,
    ) !schema.NodeUpdates {
        const target_ver = req.version orelse version.version;
        var targets: std.ArrayList([]const u8) = .empty;
        {
            self.state_mu.lockUncancelable(self.io);
            defer self.state_mu.unlock(self.io);
            if (req.all or req.nodes.len == 0) {
                var nit = self.core_state.nodes.iterator();
                while (nit.next()) |e| {
                    const n = e.value_ptr.*;
                    if (n.approved and !n.removed) {
                        try targets.append(arena, try arena.dupe(u8, n.id));
                    }
                }
            } else {
                for (req.nodes) |want| {
                    var nit = self.core_state.nodes.iterator();
                    while (nit.next()) |e| {
                        const n = e.value_ptr.*;
                        if (!n.approved or n.removed) continue;
                        if (std.mem.eql(u8, n.id, want) or std.mem.eql(u8, n.name, want)) {
                            try targets.append(arena, try arena.dupe(u8, n.id));
                            break;
                        }
                    }
                }
            }
        }
        if (targets.items.len == 0) {
            return .{ .version = target_ver, .nodes = &.{} };
        }

        const rid = req.rollout_id orelse try self.gen_id(arena, "ro");
        const cursor_start = self.stream_hub.current_seq();
        try self.commit(.{ .rollout_started = .{
            .rollout_id = rid,
            .version = target_ver,
            .node_ids = targets.items,
        } });

        if (!req.no_wait) {
            const deadline = std.Io.Clock.real.now(self.io).toMilliseconds() + 30_000;
            var cursor = cursor_start;
            while (true) {
                var all_done = true;
                {
                    self.state_mu.lockUncancelable(self.io);
                    defer self.state_mu.unlock(self.io);
                    if (self.core_state.rollouts.get(rid)) |ro| {
                        if (!ro.cancelled) {
                            for (ro.nodes.items) |rn| {
                                if (!rn.done) {
                                    all_done = false;
                                    break;
                                }
                            }
                        }
                    }
                }
                if (all_done) break;
                const now = std.Io.Clock.real.now(self.io).toMilliseconds();
                if (now >= deadline) break;
                const ws = try self.stream_hub.wait_since(arena, cursor, @min(500, deadline - now));
                cursor = ws.last_seq;
            }
        }

        const nodes_out = try self.collect_rollout_updates(arena, rid);
        return .{
            .version = target_ver,
            .nodes = nodes_out,
            .rollout_id = rid,
        };
    }

    pub fn collect_rollout_updates(
        self: *Runtime,
        arena: std.mem.Allocator,
        rollout_id: []const u8,
    ) ![]schema.NodeUpdate {
        self.state_mu.lockUncancelable(self.io);
        defer self.state_mu.unlock(self.io);
        const ro = self.core_state.rollouts.get(rollout_id) orelse return &.{};
        var out: std.ArrayList(schema.NodeUpdate) = .empty;
        for (ro.nodes.items) |rn| {
            const nr = self.core_state.nodes.get(rn.node_id);
            const nname = if (nr) |n| n.name else rn.node_id;
            const nver = if (nr) |n| n.version else "";
            const detail = if (!rn.done)
                (if (ro.cancelled) "cancelled" else "in_progress")
            else
                rn.detail;
            try out.append(arena, .{
                .node_id = try arena.dupe(u8, rn.node_id),
                .name = try arena.dupe(u8, nname),
                .from = try arena.dupe(u8, nver),
                .to = try arena.dupe(u8, ro.version),
                .ok = rn.ok,
                .detail = try arena.dupe(u8, detail),
            });
        }
        return out.items;
    }

    fn send_node_update_async(self: *Runtime, rollout_id: []const u8, node_id: []const u8, target_ver: []const u8) void {
        const Arg = struct { rt: *Runtime, rollout_id: []const u8, node_id: []const u8, target_ver: []const u8 };
        const arg = self.gpa.create(Arg) catch return;
        arg.* = .{
            .rt = self,
            .rollout_id = self.gpa.dupe(u8, rollout_id) catch {
                self.gpa.destroy(arg);
                return;
            },
            .node_id = self.gpa.dupe(u8, node_id) catch {
                self.gpa.free(arg.rollout_id);
                self.gpa.destroy(arg);
                return;
            },
            .target_ver = self.gpa.dupe(u8, target_ver) catch {
                self.gpa.free(arg.rollout_id);
                self.gpa.free(arg.node_id);
                self.gpa.destroy(arg);
                return;
            },
        };
        const t = std.Thread.spawn(.{}, struct {
            fn run(a_ptr: *Arg) void {
                defer {
                    a_ptr.rt.gpa.free(a_ptr.rollout_id);
                    a_ptr.rt.gpa.free(a_ptr.node_id);
                    a_ptr.rt.gpa.free(a_ptr.target_ver);
                    a_ptr.rt.gpa.destroy(a_ptr);
                }
                var arena_st = std.heap.ArenaAllocator.init(a_ptr.rt.gpa);
                defer arena_st.deinit();
                const arena = arena_st.allocator();

                const ch = a_ptr.rt.hub_home.config.update_channel;
                const rep = a_ptr.rt.fleet_srv.send_update(arena, a_ptr.node_id, .{
                    .version = a_ptr.target_ver,
                    .channel = ch,
                    .rollout_id = a_ptr.rollout_id,
                }) catch |err| {
                    a_ptr.rt.commit(.{ .rollout_node_result = .{
                        .rollout_id = a_ptr.rollout_id,
                        .node_id = a_ptr.node_id,
                        .ok = false,
                        .detail = @errorName(err),
                    } }) catch {};
                    return;
                };
                a_ptr.rt.commit(.{ .rollout_node_result = .{
                    .rollout_id = a_ptr.rollout_id,
                    .node_id = a_ptr.node_id,
                    .ok = rep.ok,
                    .detail = rep.detail,
                } }) catch {};
            }
        }.run, .{arg}) catch {
            self.gpa.free(arg.rollout_id);
            self.gpa.free(arg.node_id);
            self.gpa.free(arg.target_ver);
            self.gpa.destroy(arg);
            return;
        };
        t.detach();
    }

    fn init_sleeve_devices(self: *Runtime) void {
        if (self.hub_home.config.sleeve_devices.len == 0) return;
        self.meta_mu.lockUncancelable(self.io);
        defer self.meta_mu.unlock(self.io);
        for (self.hub_home.config.sleeve_devices) |dp| {
            const k = self.gpa.dupe(u8, dp) catch continue;
            self.device_inodes.put(self.gpa, k, stat_device_inode(self.io, dp)) catch {
                self.gpa.free(k);
            };
        }
    }

    fn poll_sleeve_devices(self: *Runtime) void {
        if (self.hub_home.config.sleeve_devices.len == 0) return;
        var ev_msgs: [16]struct { path: []const u8, code: []const u8, level: schema.Level } = undefined;
        var ev_n: usize = 0;
        {
            self.meta_mu.lockUncancelable(self.io);
            defer self.meta_mu.unlock(self.io);
            for (self.hub_home.config.sleeve_devices) |dp| {
                const cur = stat_device_inode(self.io, dp);
                const gop = self.device_inodes.getOrPut(self.gpa, dp) catch continue;
                if (!gop.found_existing) {
                    gop.key_ptr.* = self.gpa.dupe(u8, dp) catch dp;
                    gop.value_ptr.* = cur;
                    continue;
                }
                const prev = gop.value_ptr.*;
                if (prev != cur) {
                    gop.value_ptr.* = cur;
                    if (ev_n < ev_msgs.len) {
                        if (prev == null and cur != null) {
                            ev_msgs[ev_n] = .{ .path = dp, .code = "device_attached", .level = .info };
                            ev_n += 1;
                        } else if (prev != null and cur == null) {
                            ev_msgs[ev_n] = .{ .path = dp, .code = "device_detached", .level = .warn };
                            ev_n += 1;
                        } else if (prev != null and cur != null) {
                            ev_msgs[ev_n] = .{ .path = dp, .code = "device_reattached", .level = .info };
                            ev_n += 1;
                        }
                    }
                }
            }
        }
        for (ev_msgs[0..ev_n]) |em| {
            self.record_hub_event(.guardrail, em.code, em.path, em.code, em.level);
        }
    }

    pub fn stat_device_inode(io: std.Io, path: []const u8) ?u64 {
        const st = std.Io.Dir.cwd().statFile(io, path, .{}) catch return null;
        return @intCast(st.inode);
    }

    pub fn advertise_addrs(self: *Runtime, arena: std.mem.Allocator) ![]const []const u8 {
        if (self.hub_home.config.advertise_addrs.len > 0) {
            return dupe_str_slice(arena, self.hub_home.config.advertise_addrs);
        }
        const port = self.fleet_srv.listen_port();
        const a1 = try std.fmt.allocPrint(arena, "127.0.0.1:{d}", .{port});
        const slice = try arena.alloc([]const u8, 1);
        slice[0] = a1;
        return slice;
    }
};

fn try_dupe(a: std.mem.Allocator, s: []const u8) []const u8 {
    return a.dupe(u8, s) catch s;
}

fn dupe_str_slice(a: std.mem.Allocator, items: []const []const u8) []const []const u8 {
    const out = a.alloc([]const u8, items.len) catch return &.{};
    for (items, 0..) |it, i| out[i] = a.dupe(u8, it) catch it;
    return out;
}

fn map_proto_harness(h: d_proto.Harness) schema.Harness {
    return switch (h) {
        .claude => .claude,
        .codex => .codex,
        .grok => .grok,
        .pi_vendor => .pi_vendor,
        .pi_local => .pi_local,
        .antigravity => .antigravity,
    };
}

pub fn schema_to_model_harness(h: schema.Harness) model.Harness {
    return switch (h) {
        .claude => .claude,
        .codex => .codex,
        .grok => .grok,
        .pi_vendor => .pi_vendor,
        .pi_local => .pi_local,
        .antigravity => .antigravity,
        .unknown => .pi_local,
    };
}

fn model_to_schema_harness(h: model.Harness) schema.Harness {
    return switch (h) {
        .claude => .claude,
        .codex => .codex,
        .grok => .grok,
        .pi_vendor => .pi_vendor,
        .pi_local => .pi_local,
        .antigravity => .antigravity,
    };
}

fn map_tool_kind(t: d_proto.ToolKind) schema.ToolKind {
    return switch (t) {
        .command => .command,
        .read => .read,
        .edit => .edit,
        .search => .search,
        .web => .web,
        .mcp => .mcp,
        .subagent => .subagent,
        .other => .other,
    };
}

fn sleeve_to_schema(arena: std.mem.Allocator, sl: state.SleeveState) !schema.Sleeve {
    var targets = try arena.alloc(schema.Harness, sl.target_len);
    for (0..sl.target_len) |i| targets[i] = model_to_schema_harness(sl.targets[i]);

    const inc_s: ?schema.Incarnation = if (sl.inc) |inc| .{
        .g = inc.g,
        .mode = switch (inc.mode) {
            .headless => .headless,
            .pane => .pane,
        },
        .harness = model_to_schema_harness(inc.harness),
        .subscription = switch (inc.harness) {
            .claude => .claude,
            .codex => .codex,
            .grok => .grok,
            .pi_local => .local,
            .pi_vendor, .antigravity => .api,
        },
        .provider = try arena.dupe(u8, inc.provider),
        .model = try arena.dupe(u8, inc.model_name),
        .effort = if (inc.effort) |e| try arena.dupe(u8, e) else null,
        .native_id = if (inc.native_id) |n| try arena.dupe(u8, n) else null,
        .activity = .{
            .state = switch (inc.activity) {
                .booting => .booting,
                .idle => .idle,
                .delivering => .delivering,
                .working => .working,
                .blocked => .blocked,
                .interrupting => .interrupting,
                .compacting => .compacting,
                .ended => .ended,
                .unknown => .unknown,
            },
            .since_ms = .{ .ms = inc.last_event_ms },
            .request_id = if (inc.delivered_request_id) |r| try arena.dupe(u8, r) else null,
        },
        .started_at_ms = .{ .ms = inc.started_at_ms },
        .turn = inc.turn,
        .context_tokens = inc.context_tokens,
        .context_window = inc.context_window,
    } else null;

    return .{
        .id = try arena.dupe(u8, sl.id),
        .name = try arena.dupe(u8, sl.name),
        .workspace_id = try arena.dupe(u8, sl.workspace_id),
        .parent_id = if (sl.parent_id) |p| try arena.dupe(u8, p) else null,
        .role = switch (sl.role) {
            .envoy => .envoy,
            .root => .root,
            .child => .child,
        },
        .lifecycle = switch (sl.lifecycle) {
            .provisioning => .provisioning,
            .launching => .launching,
            .running => .running,
            .recovering => .recovering,
            .resleeving => .resleeving,
            .stopping => .stopping,
            .stopped => .stopped,
            .parked => .parked,
            .removed => .removed,
        },
        .park_reason = if (sl.park_reason) |pr| @tagName(pr) else null,
        .path = try arena.dupe(u8, sl.path),
        .resleeve_to = targets,
        .branch = if (sl.branch) |b| try arena.dupe(u8, b) else null,
        .view = switch (sl.layout) {
            .worktree => .worktree,
            .readonly => .readonly,
            .shared => null,
        },
        .incarnation = inc_s,
        .queued = sl.queued.items.len,
        .oldest_queued_at_ms = if (sl.queued.items.len > 0) .{ .ms = sl.queued.items[0].queued_at_ms } else null,
        .budget_used = sl.budget_used,
        .budget_limit = sl.budget_limit,
        .d9 = sl.d9,
    };
}

fn approval_to_schema(arena: std.mem.Allocator, ar: state.ApprovalRecord) !schema.Approval {
    const opts = try arena.alloc(schema.Decision, 2);
    opts[0] = .allow_once;
    opts[1] = .deny;
    return .{
        .id = try arena.dupe(u8, ar.id),
        .sleeve_id = try arena.dupe(u8, ar.sleeve_id),
        .kind = switch (ar.kind) {
            .tool => .tool,
            .action => .action,
        },
        .title = try arena.dupe(u8, ar.title),
        .summary = try arena.dupe(u8, ar.summary),
        .options = opts,
        .requested_at_ms = .{ .ms = ar.requested_at_ms },
        .timeout_at_ms = .{ .ms = ar.timeout_at_ms },
        .state = switch (ar.state) {
            .pending => .pending,
            .approved => .approved,
            .denied => .denied,
            .expired => .expired,
            .cancelled => .cancelled,
            .fenced => .fenced,
            .abandoned => .abandoned,
        },
        .decision = if (ar.verdict) |v| switch (v) {
            .allow_once => .allow_once,
            .allow_session => .allow_session,
            .deny => .deny,
        } else null,
        .decided_by = if (ar.decided_by) |p| switch (p) {
            .owner => .owner,
            .envoy => .envoy,
            .rule => .rule,
            .system, .sleeve => .autonomous,
        } else null,
        .decided_at_ms = if (ar.decided_at_ms) |m| .{ .ms = m } else null,
    };
}
