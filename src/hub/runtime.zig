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
const limits = @import("../net/limits.zig");
const home = @import("home.zig");
const transcript = @import("transcript.zig");
const stream = @import("stream.zig");
const brief = @import("brief.zig");
const git = @import("git.zig");
const fleet = @import("fleet.zig");
const model_catalog = @import("model_catalog.zig");
const gauge = @import("../net/gauge.zig");

const log_mirror_max: u64 = 1024 * 1024;
const status_failures_max: u8 = 5;
const no_driver_exit = std.math.maxInt(u8);

pub const spawn_harnesses = [_]model.Harness{ .claude, .codex, .grok, .pi_local };

const small_task_cost_usd = 0.05;
const medium_task_cost_usd = 0.50;
const default_task_size: q_select.TaskSize = .medium;

/// How long close_and_destroy waits for background workers (dispatch, cancel and update calls): the
/// fleet is shut down first so their calls fail at once; a worker still running after this is stuck in
/// blocking work, and the process ends without freeing what it uses.
const teardown_wait_ms: i64 = 15_000;
/// How long ending the process without teardown waits for the lock that guards the sleeve table.
const fence_lock_wait_ms: i64 = 2_000;

pub const NodeTelemetry = struct {
    inventory: ?msg.Inventory = null,
    sample: ?msg.Sample = null,
    presence: msg.Presence = .{ .state = .unknown },
    boot_id: []const u8 = "",
    quarantined: ?[]const u8 = null,
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
    timeout_secs: ?u64 = null,
    exclusive: bool = false,
    slots: u32 = 1,
    long_slots: u32 = 0,
    mem_bytes: u64 = 0,
    wait_ms: ?i64 = null,
    lease_ttl_ms: ?i64 = null,
    parent_epoch: ?u64 = null,
    boot_id: ?[]const u8 = null,
    epoch: ?u64 = null,
    lease_id: ?[]const u8 = null,
    status_failures: u8 = 0,
};

const DriverExit = packed struct(u8) {
    cause: model.ExitCause,
    requested: bool = false,
    reserved: u4 = 0,
};

const DriverHandle = struct {
    g: u64,
    sleeve_id: []const u8,
    rt: *Runtime,
    sess: *d_session.Session,
    guard: s_exec.ProcessGuard = .{},
    turn_id: u64 = 0,
    unknown_tool_calls: u64 = 0,
    harness: model.Harness,
    memory_max_bytes: ?u64 = null,
    oom_notice_pending: bool = false,
    memory_prompt_pending: std.atomic.Value(bool) = .init(false),
    interrupt_exit: std.atomic.Value(bool) = .init(false),
    pending_exit: std.atomic.Value(u8) = .init(no_driver_exit),
    quota_exit_cause: std.atomic.Value(u8) = .init(no_driver_exit),
    handoff_pending: std.atomic.Value(bool) = .init(false),
    users: usize = 0,

    fn driver_exit(self: *const DriverHandle) ?DriverExit {
        const pending = self.pending_exit.load(.acquire);
        return if (pending == no_driver_exit) null else @bitCast(pending);
    }

    fn publish_exit(self: *DriverHandle, exit: DriverExit) void {
        _ = self.pending_exit.cmpxchgStrong(no_driver_exit, @bitCast(exit), .release, .monotonic);
    }
};

pub const RuntimeOptions = struct {
    home_override: ?[]const u8 = null,
    listen_override: ?[]const u8 = null,
    proxy_listen_override: ?[]const u8 = null,
    trusted_proxies_override: []const []const u8 = &.{},
    fleet_listen_override: ?[]const u8 = null,
    dev_token_override: ?[]const u8 = null,
    no_sandbox: bool = false,
};

const LauncherJob = struct {
    run: *const fn (*anyopaque) void,
    ctx: *anyopaque,
    done: bool = false,
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
    memory_warning_logged: std.atomic.Value(bool) = .init(false),

    state_mu: std.Io.Mutex = .init,
    journal: store.JournalStore,
    core_state: state.State,

    meta_mu: std.Io.Mutex = .init,
    driver_launch_mu: std.Io.Mutex = .init,
    driver_users_cond: std.Io.Condition = .init,
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
    cached_quotas: []const q_select.Quota = &.{},
    cached_local_model: ?q_readers.LocalModel = null,
    cached_quotas_arena: std.heap.ArenaAllocator,
    model_catalog: model_catalog.Cache = .{},
    quotas_remote_polled: std.atomic.Value(bool) = .init(false),
    restore_error: ?[]const u8 = null,
    /// The release this process started as (current.txt at open) and the lock that keeps an update from
    /// changing the release between the check and the write when the hub marks itself healthy.
    running_release: ?[]const u8 = null,
    release_mu: std.Io.Mutex = .init,

    transcripts: transcript.TranscriptStore,
    history: h_index.HistoryIndex,
    stream_hub: stream.StreamHub,
    fleet_srv: fleet.FleetServer,
    pool_source_lock: std.atomic.Mutex = .unlocked,
    pool_source_context: ?*const anyopaque = null,
    pool_source: ?*const fn (*const anyopaque, *std.ArrayList(gauge.Reading), std.mem.Allocator) std.mem.Allocator.Error!void = null,

    http_listen: []const u8,
    proxy_listen: ?[]const u8 = null,
    trusted_proxies: []const []const u8 = &.{},
    fleet_listen: []const u8,
    stopping: std.atomic.Value(bool) = .init(false),
    tick_thread: ?std.Thread = null,
    quota_thread: ?std.Thread = null,
    history_thread: ?std.Thread = null,
    launcher_thread: ?std.Thread = null,
    launcher_mu: std.Io.Mutex = .init,
    launcher_cond: std.Io.Condition = .init,
    launcher_job: ?*LauncherJob = null,
    launcher_closed: bool = false,
    shutdown_requested: std.atomic.Value(bool) = .init(false),
    workers_mu: std.Io.Mutex = .init,
    workers_cond: std.Io.Condition = .init,
    active_workers: usize = 0,

    fn enter_worker(self: *Runtime) bool {
        self.workers_mu.lockUncancelable(self.io);
        defer self.workers_mu.unlock(self.io);
        if (self.stopping.load(.acquire)) return false;
        self.active_workers += 1;
        return true;
    }

    fn exit_worker(self: *Runtime) void {
        self.workers_mu.lockUncancelable(self.io);
        tiger.assert(self.active_workers > 0);
        self.active_workers -= 1;
        if (self.active_workers == 0) self.workers_cond.broadcast(self.io);
        self.workers_mu.unlock(self.io);
    }

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

        const running_release = u_store.current_release(gpa, io, hh.releases_path);
        errdefer if (running_release) |tag| gpa.free(tag);

        const http_listen = try gpa.dupe(u8, opts.listen_override orelse hh.config.listen_addr);
        errdefer gpa.free(http_listen);
        const proxy_listen: ?[]const u8 = if (opts.proxy_listen_override orelse hh.config.proxy_listen) |pl|
            try gpa.dupe(u8, pl)
        else
            null;
        errdefer if (proxy_listen) |pl| gpa.free(pl);
        const src_proxies = if (opts.trusted_proxies_override.len > 0) opts.trusted_proxies_override else hh.config.trusted_proxies;
        const trusted_proxies = try gpa.alloc([]const u8, src_proxies.len);
        var prox_init: usize = 0;
        errdefer {
            for (trusted_proxies[0..prox_init]) |tp| gpa.free(tp);
            gpa.free(trusted_proxies);
        }
        for (src_proxies, 0..) |tp, idx| {
            trusted_proxies[idx] = try gpa.dupe(u8, tp);
            prox_init += 1;
        }
        const fleet_listen = try gpa.dupe(u8, opts.fleet_listen_override orelse hh.config.fleet_listen);
        errdefer gpa.free(fleet_listen);

        var root_dir = try std.Io.Dir.cwd().openDir(io, hh.root_path, .{});
        defer root_dir.close(io);

        var tx_store = try transcript.TranscriptStore.init(gpa, io, hh.transcripts_path);
        errdefer tx_store.deinit();

        var h_idx = try h_index.HistoryIndex.open(gpa, io, hh.history_path);
        errdefer h_idx.deinit();

        var hub_stream = try stream.StreamHub.init(gpa, io);
        errdefer hub_stream.deinit();

        const rt = try gpa.create(Runtime);
        errdefer gpa.destroy(rt);

        const bwrap_ok = if (opts.no_sandbox or hh.config.no_sandbox) false else s_exec.probe_bwrap(gpa, io);
        const landlock_ok = if (opts.no_sandbox or hh.config.no_sandbox) false else s_exec.probe_landlock();

        const journal = try store.JournalStore.open(gpa, io, root_dir);
        rt.* = .{
            .gpa = gpa,
            .io = io,
            .env = env,
            .hub_home = hh,
            .started_at_ms = std.Io.Clock.real.now(io).toMilliseconds(),
            .no_sandbox = opts.no_sandbox or hh.config.no_sandbox,
            .bwrap_ok = bwrap_ok,
            .landlock_ok = landlock_ok,
            .journal = journal,
            .core_state = state.State.init(gpa),
            .cached_quotas_arena = std.heap.ArenaAllocator.init(gpa),
            .running_release = running_release,
            .transcripts = tx_store,
            .history = h_idx,
            .stream_hub = hub_stream,
            .fleet_srv = undefined,
            .http_listen = http_listen,
            .proxy_listen = proxy_listen,
            .trusted_proxies = trusted_proxies,
            .fleet_listen = fleet_listen,
        };

        errdefer rt.journal.close();
        errdefer rt.core_state.deinit();

        rt.journal.replay_into(&rt.core_state, rt, replay_quota_events) catch |err| {
            rt.restore_error = @errorName(err);
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

        rt.init_sleeve_devices();
        rt.load_host_info();
        rt.refresh_quotas_sync(false);
        rt.launcher_thread = try std.Thread.spawn(.{}, launcher_loop, .{rt});
        errdefer {
            rt.stopping.store(true, .release);
            rt.launcher_mu.lockUncancelable(rt.io);
            rt.launcher_cond.broadcast(rt.io);
            rt.launcher_mu.unlock(rt.io);
            rt.launcher_thread.?.join();
        }
        try rt.recover_after_replay();
        _ = rt.history.ingest_roots(null, hh.transcripts_path);

        rt.tick_thread = try std.Thread.spawn(.{}, tick_loop, .{rt});
        if (env.get("PROTECTORATE_NO_REMOTE_QUOTA") == null and env.get("PROTECTORATE_NO_HOST_HISTORY") == null) {
            rt.quota_thread = std.Thread.spawn(.{}, quota_bg_loop, .{rt}) catch null;
        }
        if (env.get("PROTECTORATE_NO_HOST_HISTORY") == null) {
            rt.history_thread = std.Thread.spawn(.{}, history_bg_loop, .{rt}) catch null;
        }
        return rt;
    }

    /// A child dies with the thread that forked it (PDEATHSIG), so sleeves are forked from this thread, which lives as long as the hub.
    fn launcher_loop(self: *Runtime) void {
        self.launcher_mu.lockUncancelable(self.io);
        while (true) {
            if (self.launcher_job) |job| {
                self.launcher_mu.unlock(self.io);
                job.run(job.ctx);
                self.launcher_mu.lockUncancelable(self.io);
                job.done = true;
                self.launcher_job = null;
                self.launcher_cond.broadcast(self.io);
            } else if (self.stopping.load(.acquire)) {
                break;
            } else {
                self.launcher_cond.waitUncancelable(self.io, &self.launcher_mu);
            }
        }
        self.launcher_closed = true;
        self.launcher_cond.broadcast(self.io);
        self.launcher_mu.unlock(self.io);
    }

    fn run_on_launcher(self: *Runtime, job: *LauncherJob) !void {
        self.launcher_mu.lockUncancelable(self.io);
        defer self.launcher_mu.unlock(self.io);
        while (self.launcher_job != null and !self.launcher_closed) self.launcher_cond.waitUncancelable(self.io, &self.launcher_mu);
        if (self.launcher_closed) return error.HubStopping;
        self.launcher_job = job;
        self.launcher_cond.broadcast(self.io);
        while (!job.done) self.launcher_cond.waitUncancelable(self.io, &self.launcher_mu);
    }

    fn quota_bg_loop(self: *Runtime) void {
        self.refresh_quotas_sync(true);
        while (!self.stopping.load(.acquire)) {
            var i: usize = 0;
            while (i < 120 and !self.stopping.load(.acquire)) : (i += 1) {
                self.io.sleep(.fromMilliseconds(1_000), .awake) catch break;
            }
            if (self.stopping.load(.acquire)) break;
            self.refresh_quotas_sync(true);
        }
    }

    fn history_bg_loop(self: *Runtime) void {
        _ = self.history.ingest_roots_cancellable(
            self.env.get("HOME"),
            self.hub_home.transcripts_path,
            &self.stopping,
        );
    }

    /// Ends the release's probation. Call it only once the listeners are bound and the workers are
    /// running: a start that fails before this leaves the release unhealthy so the next start rolls it back.
    pub fn mark_healthy(self: *Runtime) void {
        if (self.restore_error != null) return;
        self.release_mu.lockUncancelable(self.io);
        defer self.release_mu.unlock(self.io);
        u_store.mark_healthy_if(self.gpa, self.io, self.hub_home.releases_path, self.running_release orelse return) catch {};
    }

    /// For a process that must end without teardown because a worker may still use runtime state: kills
    /// every sleeve's process tree, confirms it and removes its cgroup scope, as destroy_driver does, but
    /// frees and deinitializes nothing. It takes the one lock normal teardown takes (meta_mu) and gives up on
    /// it after fence_lock_wait_ms, in which case the sleeves are left to end with the parent.
    pub fn end_process_fenced(self: *Runtime) noreturn {
        const give_up = std.Io.Clock.awake.now(self.io).toMilliseconds() + fence_lock_wait_ms;
        while (!self.meta_mu.tryLock()) {
            if (std.Io.Clock.awake.now(self.io).toMilliseconds() >= give_up) {
                std.debug.print("runtime: could not lock the sleeve table in {d} ms; sleeves are left to end with the process\n", .{fence_lock_wait_ms});
                std.process.exit(0);
            }
            self.io.sleep(.fromMilliseconds(10), .awake) catch {};
        }
        var drivers = self.sessions.valueIterator();
        while (drivers.next()) |dh| {
            _ = dh.*.guard.fence_and_confirm(self.io, 500);
            dh.*.guard.remove_cgroup_scope();
        }
        std.process.exit(0);
    }

    pub fn close_and_destroy(self: *Runtime) void {
        self.model_catalog.deinit(self.gpa);
        self.stopping.store(true, .release);
        self.stream_hub.wake_all();
        self.fleet_srv.shutdown();
        self.launcher_mu.lockUncancelable(self.io);
        self.launcher_cond.broadcast(self.io);
        self.launcher_mu.unlock(self.io);
        if (self.tick_thread) |t| t.join();
        if (self.launcher_thread) |t| t.join();
        if (self.quota_thread) |t| t.join();
        if (self.history_thread) |t| t.join();
        self.workers_mu.lockUncancelable(self.io);
        const give_up = std.Io.Clock.awake.now(self.io).toMilliseconds() + teardown_wait_ms;
        while (self.active_workers > 0) {
            const left = give_up - std.Io.Clock.awake.now(self.io).toMilliseconds();
            if (left <= 0) {
                std.debug.print("runtime: {d} background worker(s) still running after {d} ms; fencing the sleeves and ending the process without tearing the runtime down (every journal commit is already synced)\n", .{ self.active_workers, teardown_wait_ms });
                self.end_process_fenced();
            }
            self.workers_cond.waitTimeout(self.io, &self.workers_mu, .{
                .duration = .{ .raw = .fromMilliseconds(left), .clock = .awake },
            }) catch {};
        }
        self.workers_mu.unlock(self.io);
        self.fleet_srv.stop_and_deinit();

        self.meta_mu.lockUncancelable(self.io);
        var sessions = self.sessions;
        self.sessions = .empty;
        self.meta_mu.unlock(self.io);
        var drivers = sessions.valueIterator();
        while (drivers.next()) |dh| self.destroy_driver(dh.*, null);
        sessions.deinit(self.gpa);

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
        if (self.proxy_listen) |pl| self.gpa.free(pl);
        for (self.trusted_proxies) |tp| self.gpa.free(tp);
        self.gpa.free(self.trusted_proxies);
        self.gpa.free(self.fleet_listen);
        if (self.running_release) |tag| self.gpa.free(tag);
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

        const PendingExit = struct { sleeve_id: []const u8, g: model.Gen, cause: model.ExitCause };
        var pending_exits: std.ArrayList(PendingExit) = .empty;
        const PendingLaunch = struct { path: []const u8, launch: intent.LaunchIncarnation };
        var pending_launches: std.ArrayList(PendingLaunch) = .empty;
        var stop_ids: std.ArrayList([]const u8) = .empty;
        var ws_entries: std.ArrayList(schema.Workspace) = .empty;
        var offline_nodes: std.ArrayList([]const u8) = .empty;

        {
            self.state_mu.lockUncancelable(self.io);
            defer self.state_mu.unlock(self.io);
            var sit = self.core_state.sleeves.iterator();
            while (sit.next()) |e| {
                const sl = e.value_ptr.*;
                if (sl.lifecycle == .running and sl.inc != null and sl.inc.?.classified_exit != null) {
                    const inc = sl.inc.?;
                    try pending_exits.append(a, .{ .sleeve_id = sl.id, .g = inc.g, .cause = inc.classified_exit.?.cause });
                } else if (sl.quota_launch) |launch| {
                    try pending_launches.append(a, .{ .path = sl.path, .launch = launch });
                } else if (sl.lifecycle != .stopped and sl.lifecycle != .parked and sl.lifecycle != .removed) {
                    try stop_ids.append(a, sl.id);
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
        for (pending_exits.items) |pending| {
            try self.complete_driver_exit(a, pending.sleeve_id, pending.g, pending.cause, false);
        }
        for (pending_launches.items) |pending| {
            self.execute_intent(a, .{ .write_brief = .{
                .sleeve_id = pending.launch.sleeve_id,
                .path = pending.path,
                .g = pending.launch.g,
                .cause = .quota,
            } });
            self.execute_intent(a, .{ .launch_incarnation = pending.launch });
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

    pub fn copy_cached_quotas(self: *Runtime, arena: std.mem.Allocator) ![]schema.Quota {
        self.meta_mu.lockUncancelable(self.io);
        defer self.meta_mu.unlock(self.io);
        return quotas_to_schema(arena, self.cached_quotas);
    }

    fn workspace_root(self: *Runtime, sleeve: state.SleeveState) state.SleeveState {
        var root = sleeve;
        while (root.parent_id) |parent| {
            root = self.core_state.sleeves.get(parent) orelse break;
        }
        return root;
    }

    fn workspace_task_size(
        self: *Runtime,
        arena: std.mem.Allocator,
        sleeve_id: ?[]const u8,
        path: []const u8,
    ) !q_select.TaskSize {
        var sleeve_ids: std.ArrayList([]const u8) = .empty;
        {
            self.state_mu.lockUncancelable(self.io);
            defer self.state_mu.unlock(self.io);
            const root = if (sleeve_id) |id| self.core_state.sleeves.get(id) else null;
            const workspace_path = if (root) |sl| self.workspace_root(sl).path else path;
            var sleeves = self.core_state.sleeves.valueIterator();
            while (sleeves.next()) |sl| {
                const other_root = self.workspace_root(sl.*);
                if (std.mem.eql(u8, workspace_path, other_root.path)) try sleeve_ids.append(arena, sl.id);
            }
        }
        var costs: transcript.RecentCosts = .{};
        for (sleeve_ids.items) |id| {
            self.transcripts.collect_costs(arena, id, &costs) catch |err| {
                if (err == error.OutOfMemory) return err;
                std.log.warn("task size: transcript {s}: {s}", .{ id, @errorName(err) });
            };
        }
        const cost = costs.median() orelse return default_task_size;
        if (cost < small_task_cost_usd) return .small;
        if (cost < medium_task_cost_usd) return .medium;
        return .large;
    }

    fn rank_guardrail_targets(self: *Runtime, arena: std.mem.Allocator, sleeve_id: []const u8, g: model.Gen) !?event.QuotaSelection {
        var targets: [model.max_targets]model.Harness = undefined;
        var target_len: usize = 0;
        var current: model.Harness = undefined;
        {
            self.state_mu.lockUncancelable(self.io);
            defer self.state_mu.unlock(self.io);
            const sl = self.core_state.sleeves.get(sleeve_id) orelse return null;
            const inc = sl.inc orelse return null;
            if (inc.g != g or sl.lifecycle != .running) return null;
            if (inc.classified_exit) |handoff| {
                if (handoff.selection != null) return null;
            }
            current = inc.harness;
            if (sl.target_len > 1) {
                target_len = sl.target_len - 1;
                @memcpy(targets[0..target_len], sl.targets[1..sl.target_len]);
            }
        }
        if (target_len == 0) return .{ .reason = "no configured alternatives" };
        return try self.choose_target(arena, targets[0..target_len], current, sleeve_id, "", null, false);
    }

    fn complete_driver_exit(self: *Runtime, arena: std.mem.Allocator, sleeve_id: []const u8, g: model.Gen, cause: model.ExitCause, requested: bool) !void {
        const selection: ?event.QuotaSelection = if (!requested and (cause == .quota_exhausted or cause == .auth_error))
            self.rank_guardrail_targets(arena, sleeve_id, g) catch event.QuotaSelection{ .reason = "quota selection unavailable" }
        else
            null;
        try self.commit(.{ .exited = .{
            .sleeve_id = sleeve_id,
            .g = g,
            .cause = cause,
            .requested = requested,
            .quota_selection = selection,
        } });
    }

    pub fn choose_target(
        self: *Runtime,
        arena: std.mem.Allocator,
        allowed: []const model.Harness,
        exclude: ?model.Harness,
        sleeve_id: ?[]const u8,
        path: []const u8,
        requested_size: ?schema.TaskSize,
        envoy: bool,
    ) !event.QuotaSelection {
        const requested: ?q_select.TaskSize = if (requested_size) |s|
            std.meta.stringToEnum(q_select.TaskSize, @tagName(s))
        else
            null;
        const size: q_select.TaskSize = blk: {
            if (!envoy and requested != null) break :blk requested.?;
            const measured = try self.workspace_task_size(arena, sleeve_id, path);
            const nudge = requested orelse break :blk measured;
            const baseline: i8 = @backingInt(measured);
            const clamped = std.math.clamp(@as(i8, @backingInt(nudge)), baseline - 1, baseline + 1);
            break :blk @fromBackingInt(@intCast(clamped));
        };

        const health = try doctor.check_harnesses(arena, self.io, self.env.get("HOME"));
        var candidates: [spawn_harnesses.len]q_select.Candidate = undefined;
        tiger.assert(allowed.len <= candidates.len);
        for (allowed, candidates[0..allowed.len]) |h, *candidate| {
            candidate.* = .{
                .harness = std.meta.stringToEnum(q_select.Harness, @tagName(h)).?,
                .eligible = h != exclude and h != .antigravity and doctor_eligible(health, h),
            };
        }

        self.meta_mu.lockUncancelable(self.io);
        defer self.meta_mu.unlock(self.io);
        if (self.cached_local_model == null) {
            for (candidates[0..allowed.len]) |*candidate| {
                if (candidate.harness == .pi_local) candidate.eligible = false;
            }
        }
        const now = std.Io.Clock.real.now(self.io).toMilliseconds();
        var snapshot: [std.enums.values(q_select.Subscription).len]q_select.Quota = undefined;
        const quotas = snapshot[0..self.cached_quotas.len];
        for (self.cached_quotas, quotas) |q, *row| {
            row.* = q;
            if (q.reading == .ok and now -| q.fetched_at_ms >= q_select.stale_grace_ms) row.reading = .stale;
        }
        const decision = q_select.select_with_fallback(candidates[0..allowed.len], quotas, size, now) catch |err| {
            return .{ .reason = switch (err) {
                error.NoCandidates, error.AllIneligible => "no eligible harness with a runnable configuration",
                error.NoMeteredTarget => "no metered harness available",
                error.AllExhausted => "eligible harnesses are short of usable quota or have failed readings",
            } };
        };
        const harness = allowed[decision.index];
        const local = if (harness == .pi_local) self.cached_local_model else null;
        return .{
            .harness = harness,
            .model_name = if (local) |m| try arena.dupe(u8, m.id) else null,
            .provider = if (local) |m| try arena.dupe(u8, m.provider) else null,
            .reason = try std.fmt.allocPrint(arena, "{s} for a {s} task", .{ decision.reason, @tagName(size) }),
        };
    }

    fn replay_quota_events(self: *Runtime, at_ms: i64, intents: []const intent.Intent) void {
        for (intents) |it| {
            if (it == .record_quota_selection) self.record_target_selection(it.record_quota_selection, at_ms, false);
        }
    }

    fn record_target_selection(self: *Runtime, record: @FieldType(intent.Intent, "record_quota_selection"), at_ms: i64, publish: bool) void {
        const source = switch (record.source) {
            .spawn => "spawn",
            .guardrail => "guardrail resleeve",
        };
        const choice = record.choice;
        const harness = choice.harness orelse {
            self.record_hub_event_at(.sleeve, source, record.sleeve_id, choice.reason, .warn, at_ms, publish);
            return;
        };
        const model_name = choice.model_name orelse "";
        const text = std.fmt.allocPrint(self.gpa, "{s}{s}{s} ({s})", .{
            @tagName(harness),
            if (model_name.len > 0) "/" else "",
            model_name,
            choice.reason,
        }) catch return;
        defer self.gpa.free(text);
        self.record_hub_event_at(.sleeve, source, record.sleeve_id, text, .info, at_ms, publish);
    }

    fn quotas_to_schema(arena: std.mem.Allocator, src: []const q_select.Quota) ![]schema.Quota {
        const out = try arena.alloc(schema.Quota, src.len);
        for (src, out) |q, *dest| {
            const windows = try arena.alloc(schema.QuotaWindow, q.windows.len);
            for (q.windows, windows) |w, *window| {
                window.* = .{
                    .label = try arena.dupe(u8, w.label),
                    .used_pct = w.used_pct,
                    .resets_at_ms = if (w.resets_at_ms) |r| .{ .ms = r } else null,
                    .length_ms = if (w.length_ms) |l| @intCast(l) else null,
                };
            }
            dest.* = .{
                .subscription = switch (q.subscription) {
                    .claude => .claude,
                    .codex => .codex,
                    .grok => .grok,
                    .antigravity => .antigravity,
                    .local => .local,
                    .api => .api,
                    .unknown => .unknown,
                },
                .plan = try arena.dupe(u8, q.plan),
                .reading = switch (q.reading) {
                    .unknown => .unknown,
                    .ok => .ok,
                    .stale => .stale,
                    .@"error" => .@"error",
                },
                .fetched_at_ms = .{ .ms = q.fetched_at_ms },
                .windows = windows,
                .credits = if (q.credits) |cr| .{
                    .allowed = cr.allowed,
                    .limit_reached = cr.limit_reached,
                    .has_credits = cr.has_credits,
                    .balance = cr.balance,
                    .overage_limit_reached = cr.overage_limit_reached,
                    .reset_available_count = if (cr.reset_available_count) |count| @as(u64, count) else null,
                    .per_hour = cr.per_hour,
                } else null,
                .note = try arena.dupe(u8, q.note),
            };
        }
        return out;
    }

    pub fn refresh_quotas_sync(self: *Runtime, poll_remote: bool) void {
        var arena_st = std.heap.ArenaAllocator.init(self.gpa);
        const a = arena_st.allocator();
        const now = std.Io.Clock.real.now(self.io).toMilliseconds();
        const rep = q_readers.read_all(a, self.io, self.env, self.env.get("HOME"), now, poll_remote, self.hub_home.config.quota_local_provider) catch {
            arena_st.deinit();
            return;
        };
        const out = a.dupe(q_select.Quota, rep.quotas) catch {
            arena_st.deinit();
            return;
        };

        var pub_arena_st = std.heap.ArenaAllocator.init(self.gpa);
        defer pub_arena_st.deinit();

        self.meta_mu.lockUncancelable(self.io);
        const merge_now = std.Io.Clock.real.now(self.io).toMilliseconds();
        for (out) |*q| {
            if (q.reading != .stale or q.windows.len != 0) continue;
            for (self.cached_quotas) |old| {
                if (old.subscription != q.subscription or old.windows.len == 0) continue;
                if (old.fetched_at_ms <= 0 or (merge_now -| old.fetched_at_ms) >= 24 * 60 * 60 * 1000) break;
                const retained = a.alloc(q_select.Window, old.windows.len) catch break;
                var count: usize = 0;
                for (old.windows) |w| {
                    if (w.resets_at_ms) |reset| {
                        if (reset <= merge_now) continue;
                    }
                    retained[count] = w;
                    retained[count].label = a.dupe(u8, w.label) catch break;
                    count += 1;
                }
                if (count == 0) break;
                q.plan = a.dupe(u8, old.plan) catch break;
                q.windows = retained[0..count];
                q.credits = old.credits;
                q.fetched_at_ms = old.fetched_at_ms;
                break;
            }
        }
        self.cached_quotas_arena.deinit();
        self.cached_quotas_arena = arena_st;
        self.cached_quotas = out;
        self.cached_local_model = rep.local_model;
        if (poll_remote) {
            self.quotas_remote_polled.store(true, .release);
        }
        const pub_qs = quotas_to_schema(pub_arena_st.allocator(), out) catch &.{};
        self.meta_mu.unlock(self.io);

        for (pub_qs) |pq| {
            self.stream_hub.publish_raw(.{ .quota = .{ .seq = 0, .quota = pq } });
        }
    }

    pub fn commit(self: *Runtime, ev: event.Event) !void {
        try self.commit_with_handoff_generation(ev, null);
    }

    fn commit_with_handoff_generation(self: *Runtime, ev: event.Event, handoff_g: ?model.Gen) !void {
        var intent_arena_st = std.heap.ArenaAllocator.init(self.gpa);
        defer intent_arena_st.deinit();
        const ia = intent_arena_st.allocator();

        var intents: std.ArrayList(intent.Intent) = .empty;
        defer intents.deinit(self.gpa);

        const now_ms: i64 = std.Io.Clock.real.now(self.io).toMilliseconds();

        {
            self.state_mu.lockUncancelable(self.io);
            defer self.state_mu.unlock(self.io);

            if (handoff_g) |g| {
                const sl = self.core_state.sleeves.get(ev.resleeve_requested.sleeve_id) orelse return;
                const inc = sl.inc orelse return;
                if (sl.lifecycle != .running or inc.g != g or inc.classified_exit != null) return;
            }
            if (ev == .launched) {
                const sl = self.core_state.sleeves.get(ev.launched.sleeve_id) orelse return error.NotFound;
                switch (sl.lifecycle) {
                    .launching, .recovering, .resleeving => {},
                    else => return error.InvalidState,
                }
            }
            const seq = self.core_state.event_seq + 1;
            const se: event.StampedEvent = .{
                .seq = seq,
                .ts_ms = now_ms,
                .event = ev,
            };

            try apply.apply(self.gpa, &self.core_state, se, &intents);
            self.journal.append(se) catch |err| {
                std.debug.panic("journal append failed at seq {d}: {s}", .{ seq, @errorName(err) });
            };
        }

        for (intents.items) |it| {
            if (it == .record_quota_selection) {
                self.record_target_selection(it.record_quota_selection, now_ms, true);
            } else {
                self.execute_intent(ia, it);
            }
        }

        switch (ev) {
            .node_approved, .node_removed, .node_renamed => self.fleet_srv.publish_peers(),
            else => {},
        }
        self.publish_event_updates(ia, ev);
    }

    fn execute_intent(self: *Runtime, ia: std.mem.Allocator, it: intent.Intent) void {
        switch (it) {
            .record_quota_selection => unreachable,
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
                const br = git.current_branch(ia, self.io, p.path) catch |err| {
                    self.commit(.{ .provision_failed = .{ .sleeve_id = p.sleeve_id, .reason = @errorName(err) } }) catch {};
                    return;
                };
                self.commit(.{ .provisioned = .{ .sleeve_id = p.sleeve_id, .branch = br } }) catch {};
            },
            .remove_worktree => |rw| {
                self.stop_driver_for(rw.sleeve_id);
                git.remove_worktree(self.gpa, self.io, rw.path) catch |err| {
                    self.record_hub_event(.sleeve, "git", rw.sleeve_id, @errorName(err), .warn);
                };
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
                    defer self.release_driver(h);
                    if (h.g == dp.g) {
                        const with_memory_note = h.memory_prompt_pending.swap(false, .acq_rel);
                        const text = if (with_memory_note) memory_prompt(ia, h, dp.text) catch dp.text else dp.text;
                        h.sess.send_user(dp.request_id, text, if (dp.steer) "" else null) catch |err| {
                            if (with_memory_note) h.memory_prompt_pending.store(true, .release);
                            self.record_hub_event(.sleeve, "driver", dp.sleeve_id, @errorName(err), .warn);
                            return;
                        };
                        const at_ms = std.Io.Clock.real.now(self.io).toMilliseconds();
                        const ce = self.transcripts.append(dp.sleeve_id, dp.g, h.turn_id, at_ms, .{
                            .user_message = .{
                                .request_id = dp.request_id,
                                .from = dp.from,
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
                    defer self.release_driver(dh);
                    if (dh.g == ii.g) {
                        if (dh.harness == .antigravity) dh.interrupt_exit.store(true, .release);
                        dh.sess.interrupt() catch {};
                        if (dh.harness == .antigravity) _ = dh.guard.fence_and_confirm(self.io, 500);
                    }
                }
            },
            .kill_incarnation => |ki| {
                _ = self.stop_driver_generation(ki.sleeve_id, ki.g, .{
                    .cause = .killed,
                    .requested = !ki.recover,
                });
            },
            .respond_approval => |ra| {
                if (self.get_driver(ra.sleeve_id)) |dh| {
                    defer self.release_driver(dh);
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
                const summary = if (wb.cause == .handoff) "Context ceiling reached; auto-handoff" else "Auto handoff";
                brief.write_handoff(self.gpa, self.io, self.hub_home.briefs_path, wb.sleeve_id, wb.g, wb.cause, summary) catch {};
            },
            .dispatch_job => |dj| {
                self.dispatch_job_async(dj.job_id, dj.node_id);
            },
            .verify_job => |vj| {
                self.verify_job_async(vj.job_id, vj.node_id);
            },
            .cancel_job => |cj| {
                self.cancel_job_async(cj.job_id, cj.node_id, true) catch |err|
                    self.log_cancel_failure(cj.job_id, cj.node_id, err);
            },
            .emit_notice => |en| {
                self.record_hub_event(.guardrail, en.code, en.sleeve_id, en.text, .warn);
                if (en.sleeve_id) |sid| {
                    if (self.get_driver(sid)) |dh| {
                        defer self.release_driver(dh);
                        const ce = self.transcripts.append(sid, dh.g, dh.turn_id, std.Io.Clock.real.now(self.io).toMilliseconds(), .{
                            .notice = .{ .level = .warn, .code = en.code, .text = en.text },
                        }) catch null;
                        if (ce) |c| self.stream_hub.publish_raw(.{ .entry = .{ .seq = 0, .sleeve_id = sid, .entry = c } });
                    }
                }
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
                    defer self.release_driver(dh);
                    if (dh.g == ci.g) {
                        if (dh.sess.compact()) |_| {} else |err| {
                            if (err == error.Unsupported) {
                                dh.handoff_pending.store(true, .release);
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

    fn memory_prompt(arena: std.mem.Allocator, h: *const DriverHandle, text: []const u8) ![]const u8 {
        const advice = "Ask Envoy if you need more memory. The owner can raise sleeve_memory_max_overrides.";
        const oom_note = if (h.oom_notice_pending) "The previous incarnation was killed after a scope OOM. " else "";
        if (h.memory_max_bytes) |limit| {
            return std.fmt.allocPrint(arena, "{s}This incarnation's memory limit is {d} bytes. {s}\n\n{s}", .{
                oom_note, limit, advice, text,
            });
        }
        return std.fmt.allocPrint(arena, "{s}{s}\n\n{s}", .{ oom_note, advice, text });
    }

    fn launch_driver(self: *Runtime, li: anytype) !void {
        const effort = if (li.effort) |e| if (e.len == 0) null else e else null;
        const dh = blk: {
            self.driver_launch_mu.lockUncancelable(self.io);
            defer self.driver_launch_mu.unlock(self.io);
            {
                self.meta_mu.lockUncancelable(self.io);
                defer self.meta_mu.unlock(self.io);
                if (self.stopping.load(.acquire) or self.sessions.contains(li.sleeve_id)) return;
                try self.sessions.ensureUnusedCapacity(self.gpa, 1);
            }
            const Spawn = struct {
                rt: *Runtime,
                li: @TypeOf(li),
                effort: ?[]const u8,
                result: anyerror!*DriverHandle = error.HubStopping,

                fn run(ctx: *anyopaque) void {
                    const s: *@This() = @ptrCast(@alignCast(ctx));
                    s.result = s.rt.spawn_driver(s.li, s.effort);
                }
            };
            var spawn: Spawn = .{ .rt = self, .li = li, .effort = effort };
            var job: LauncherJob = .{ .run = Spawn.run, .ctx = &spawn };
            try self.run_on_launcher(&job);
            const dh = try spawn.result;
            self.meta_mu.lockUncancelable(self.io);
            defer self.meta_mu.unlock(self.io);
            dh.users = 1;
            self.sessions.putAssumeCapacityNoClobber(dh.sleeve_id, dh);
            break :blk dh;
        };
        const launched = self.commit(.{ .launched = .{
            .sleeve_id = li.sleeve_id,
            .g = li.g,
            .harness = li.harness,
            .mode = li.mode,
            .model_name = li.model_name,
            .provider = li.provider,
            .effort = effort,
        } });
        const started = if (launched) |_| dh.sess.begin() else |err| err;
        self.release_driver(dh);
        started catch |err| {
            self.record_hub_event(.sleeve, "hub", li.sleeve_id, @errorName(err), .alarm);
            _ = self.stop_driver_generation(li.sleeve_id, li.g, .{ .cause = .crash });
        };
    }

    fn spawn_driver(self: *Runtime, li: anytype, effort: ?[]const u8) !*DriverHandle {
        var arena_st = std.heap.ArenaAllocator.init(self.gpa);
        defer arena_st.deinit();
        const a = arena_st.allocator();
        const process_tag = try std.fmt.allocPrint(a, "sleeve-{d}-{s}-{d}", .{ self.started_at_ms, li.sleeve_id, li.g });

        var sl_path: []const u8 = "";
        var sl_layout: model.Layout = .shared;
        var is_envoy = false;
        var configured_limit: ?u64 = null;
        var oom_notice_pending = false;
        {
            self.state_mu.lockUncancelable(self.io);
            defer self.state_mu.unlock(self.io);
            const sl = self.core_state.sleeves.get(li.sleeve_id) orelse return error.NotFound;
            if (li.g <= sl.next_g) return error.InvalidState;
            sl_path = try a.dupe(u8, sl.path);
            sl_layout = sl.layout;
            is_envoy = (sl.role == .envoy);
            configured_limit = self.hub_home.config.sleeve_memory_limit(sl.name);
            oom_notice_pending = sl.oom_notice_pending;
        }

        var applied_limit: ?u64 = null;
        var guard = fenced: {
            if (self.hub_home.config.scopes) {
                if (configured_limit) |limit| {
                    if (s_exec.ProcessGuard.init_sleeve(self.gpa, self.io, process_tag, limit)) |limited| {
                        applied_limit = limit;
                        break :fenced limited;
                    } else |err| {
                        if (!self.memory_warning_logged.swap(true, .acq_rel)) {
                            std.log.warn("sleeve memory limits unavailable ({s}); using process fencing without a memory limit", .{@errorName(err)});
                        }
                    }
                }
            }
            break :fenced s_exec.ProcessGuard.init_fenced(self.gpa, self.io, process_tag, null);
        };
        errdefer guard.deinit();
        const cgroup_path = if (applied_limit != null) guard.sleeve_cgroup() else null;

        const prog = harness_program(li.harness);
        const p_harness: d_proto.Harness = switch (li.harness) {
            .claude => .claude,
            .codex => .codex,
            .grok => .grok,
            .pi_vendor => .pi_vendor,
            .pi_local => .pi_local,
            .antigravity => .antigravity,
        };

        const hub_url = try std.fmt.allocPrint(a, "http://{s}", .{self.http_listen});
        var guard_mode: s_spec.GuardMode = .none;
        var guard_exe: ?[]const u8 = if (cgroup_path != null) try std.process.executablePathAlloc(self.io, a) else null;
        var sb_spec: ?s_spec.SandboxSpec = null;
        var child_env = try self.env.clone(a);
        const claude_effort = if (li.harness == .claude) effort else null;
        if (claude_effort) |level| try child_env.put("CLAUDE_CODE_EFFORT_LEVEL", level);
        _ = child_env.swapRemove("PROTECTORATE_MEMORY_MAX");
        const memory_env = if (configured_limit) |limit| try std.fmt.allocPrint(a, "{d}", .{limit}) else null;
        if (memory_env) |value| try child_env.put("PROTECTORATE_MEMORY_MAX", value);
        try child_env.put("PROTECTORATE_SLEEVE", li.sleeve_id);
        try child_env.put("PROTECTORATE_WORKSPACE", sl_path);
        try child_env.put("PROTECTORATE_ENVOY", "envoy");
        try child_env.put("PROTECTORATE_JOB_ID", process_tag);
        try child_env.put("PROTECTORATE_URL", hub_url);
        if (is_envoy) {
            try child_env.put("PROTECTORATE_TOKEN", self.hub_home.envoy_token);
        }
        if (!self.no_sandbox) {
            const sleeve_tmp = try std.fs.path.join(a, &.{ self.hub_home.root_path, "run", li.sleeve_id, "tmp" });
            try std.Io.Dir.cwd().createDirPath(self.io, sleeve_tmp);
            for (s_spec.stripped_nested_env_keys) |k| {
                _ = child_env.swapRemove(k);
            }
            try child_env.put("TMPDIR", sleeve_tmp);
            try child_env.put("TEMP", sleeve_tmp);
            try child_env.put("TMP", sleeve_tmp);

            if (builtin.os.tag == .linux) {
                guard_mode = if (self.bwrap_ok) .bwrap else if (self.landlock_ok) .landlock else .none;
                const workspace = try std.Io.Dir.cwd().realPathFileAlloc(self.io, sl_path, a);
                const git_dirs = try s_exec.resolve_git_dirs(a, self.io, workspace);
                if (git_dirs != null and guard_mode != .bwrap) {
                    return error.GitMetadataProtectionRequiresBubblewrap;
                }
                if (guard_mode != .none) {
                    sl_path = workspace;
                    try child_env.put("PROTECTORATE_WORKSPACE", sl_path);
                    if (guard_mode == .landlock and guard_exe == null) {
                        guard_exe = try std.process.executablePathAlloc(self.io, a);
                    }
                    const git_mounts = if (git_dirs) |dirs|
                        try s_exec.prepare_git_mounts(a, self.io, dirs, sl_layout == .readonly)
                    else
                        &.{};
                    const host_home = self.env.get("HOME");

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
                        .{ .key = "PROTECTORATE_ENVOY", .value = "envoy" },
                        .{ .key = "PROTECTORATE_URL", .value = hub_url },
                        .{ .key = "PROTECTORATE_JOB_ID", .value = process_tag },
                    });
                    if (is_envoy) {
                        try env_set.append(a, .{ .key = "PROTECTORATE_TOKEN", .value = self.hub_home.envoy_token });
                    }
                    if (claude_effort) |level| try env_set.append(a, .{ .key = "CLAUDE_CODE_EFFORT_LEVEL", .value = level });
                    if (memory_env) |value| try env_set.append(a, .{ .key = "PROTECTORATE_MEMORY_MAX", .value = value });

                    sb_spec = .{
                        .workspace = sl_path,
                        .readonly = (sl_layout == .readonly),
                        .host_home = null,
                        .runtime_dir = sleeve_tmp,
                        .git_mounts = git_mounts,
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
            .harness = li.harness,
            .sess = undefined,
            .guard = guard,
            .memory_max_bytes = applied_limit,
            .oom_notice_pending = oom_notice_pending,
            .memory_prompt_pending = .init(!is_envoy and (oom_notice_pending or (li.g == 1 and applied_limit != null))),
        };
        guard = .{};
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
            .effort = effort,
            .resume_id = li.resume_id,
            .session_id = sess_id,
            .sandbox_mode = guard_mode,
            .guard_exe = guard_exe,
            .sandbox = sb_spec,
            .cgroup_path = cgroup_path,
            .environ_map = &child_env,
            .on_event = on_driver_event,
            .on_exit = on_driver_exit,
            .ctx = dh,
        });
        if (cgroup_path != null) {
            dh.guard.attach_sleeve(dh.sess.child_id());
        } else {
            dh.guard.attach_with_io(self.io, dh.sess.child_id());
        }

        return dh;
    }

    fn get_driver(self: *Runtime, sleeve_id: []const u8) ?*DriverHandle {
        self.meta_mu.lockUncancelable(self.io);
        defer self.meta_mu.unlock(self.io);
        const dh = self.sessions.get(sleeve_id) orelse return null;
        dh.users += 1;
        return dh;
    }

    fn release_driver(self: *Runtime, dh: *DriverHandle) void {
        self.meta_mu.lockUncancelable(self.io);
        defer self.meta_mu.unlock(self.io);
        tiger.assert(dh.users > 0);
        dh.users -= 1;
        if (dh.users == 0) self.driver_users_cond.broadcast(self.io);
    }

    fn stop_driver_for(self: *Runtime, sleeve_id: []const u8) void {
        _ = self.stop_driver_generation(sleeve_id, null, null);
    }

    fn stop_driver_generation(self: *Runtime, sleeve_id: []const u8, expected_g: ?u64, report: ?DriverExit) bool {
        const dh = blk: {
            self.meta_mu.lockUncancelable(self.io);
            defer self.meta_mu.unlock(self.io);
            const dh = self.sessions.get(sleeve_id) orelse return false;
            if (expected_g) |g| if (dh.g != g) return false;
            const removed = self.sessions.remove(sleeve_id);
            tiger.assert(removed);
            break :blk dh;
        };
        self.destroy_driver(dh, report);
        return true;
    }

    fn destroy_driver(self: *Runtime, dh: *DriverHandle, report: ?DriverExit) void {
        dh.sess.request_stop();
        _ = dh.guard.fence_and_confirm(self.io, 500);
        self.meta_mu.lockUncancelable(self.io);
        while (dh.users > 0) self.driver_users_cond.waitUncancelable(self.io, &self.meta_mu);
        self.meta_mu.unlock(self.io);
        dh.sess.kill_and_destroy();
        if (report) |fallback| {
            const observed = dh.driver_exit() orelse fallback;
            const requested = fallback.requested or observed.requested;
            var arena = std.heap.ArenaAllocator.init(self.gpa);
            defer arena.deinit();
            self.complete_driver_exit(arena.allocator(), dh.sleeve_id, dh.g, observed.cause, requested) catch |err| {
                std.log.warn("reporting exit for {s} generation {d}: {s}", .{ dh.sleeve_id, dh.g, @errorName(err) });
            };
            if (observed.cause == .oom and !requested) self.record_sleeve_oom(dh);
        }
        dh.guard.deinit();
        self.gpa.free(dh.sleeve_id);
        self.gpa.destroy(dh);
    }

    fn on_driver_event(ctx: ?*anyopaque, ev: d_proto.DriverEvent) void {
        const dh: *DriverHandle = @ptrCast(@alignCast(ctx orelse return));
        const self = dh.rt;

        switch (ev.entry) {
            .session_info => |si| {
                const ce = self.transcripts.append(dh.sleeve_id, dh.g, dh.turn_id, ev.time_ms, .{
                    .session_info = .{
                        .harness = map_proto_harness(si.harness),
                        .mode = .headless,
                        .native_id = si.native_id,
                        .model = si.model,
                        .provider = si.provider,
                        .effort = si.effort,
                        .effort_notice = si.effort_notice,
                    },
                }) catch null;
                self.commit(.{ .session_bound = .{
                    .sleeve_id = dh.sleeve_id,
                    .g = dh.g,
                    .native_id = si.native_id,
                } }) catch {};
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
                var input_arena = std.heap.ArenaAllocator.init(self.gpa);
                defer input_arena.deinit();
                dh.unknown_tool_calls += 1;
                const unknown_sig = std.fmt.allocPrint(input_arena.allocator(), "unknown:{d}", .{dh.unknown_tool_calls}) catch return;
                self.commit(.{ .tool_activity = .{
                    .sleeve_id = dh.sleeve_id,
                    .g = dh.g,
                    .tool_sig = tc.signature orelse unknown_sig,
                } }) catch {};
                const ce = self.transcripts.append(dh.sleeve_id, dh.g, dh.turn_id, ev.time_ms, .{
                    .tool_call = .{
                        .id = tc.id,
                        .tool = map_tool_kind(tc.tool),
                        .name = tc.name,
                        .summary = tc.summary,
                        .input = .{ .val = if (tc.input_json) |raw| (std.json.parseFromSliceLeaky(std.json.Value, input_arena.allocator(), raw, .{}) catch .null) else .null },
                        .truncated = tc.truncated,
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
                if (dh.interrupt_exit.load(.acquire) or dh.driver_exit() != null) return;
                const handoff: ?model.ExitCause = blk: {
                    if (ts.stop == .@"error") {
                        const cause = dh.quota_exit_cause.load(.acquire);
                        if (cause != no_driver_exit) break :blk @fromBackingInt(@intCast(cause));
                    }
                    dh.quota_exit_cause.store(no_driver_exit, .release);
                    break :blk null;
                };
                const end_m: model.TurnEnd = switch (ts.stop) {
                    .end_turn => .end_turn,
                    .cancelled => .cancelled,
                    .@"error" => .@"error",
                    .max_turns => .max_turns,
                    .other => .other,
                };
                var arena = std.heap.ArenaAllocator.init(self.gpa);
                defer arena.deinit();
                const pending: ?event.ClassifiedExit = if (handoff) |cause| .{
                    .cause = cause,
                    .selection = self.rank_guardrail_targets(arena.allocator(), dh.sleeve_id, dh.g) catch
                        event.QuotaSelection{ .reason = "quota selection unavailable" },
                } else null;
                self.commit(.{ .turn_settled = .{
                    .sleeve_id = dh.sleeve_id,
                    .g = dh.g,
                    .turn_end = end_m,
                    .context_tokens = ts.usage.context_tokens orelse 0,
                    .context_window = ts.usage.context_window orelse 0,
                    .quota_handoff = pending,
                } }) catch |err| {
                    std.log.warn("settling turn for {s}: {s}", .{ dh.sleeve_id, @errorName(err) });
                    return;
                };
                if (handoff) |cause| dh.publish_exit(.{ .cause = cause });
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
                if (quota_exit_cause(er.kind)) |cause| dh.quota_exit_cause.store(@backingInt(cause), .release);
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
        tiger.assert(g == dh.g);
        const self = dh.rt;
        const at_ms = std.Io.Clock.real.now(self.io).toMilliseconds();
        const pending = dh.driver_exit();
        const exit = pending orelse self.classify_driver_exit(dh, exit_code, stderr_tail);
        if (pending == null and !exit.requested) {
            switch (exit.cause) {
                .quota_exhausted, .auth_error, .oom => self.commit(.{ .exit_classified = .{
                    .sleeve_id = dh.sleeve_id,
                    .g = dh.g,
                    .cause = exit.cause,
                } }) catch |err| std.log.warn("classifying exit for {s}: {s}", .{ dh.sleeve_id, @errorName(err) }),
                else => {},
            }
        }
        if (exit.requested) {
            self.commit(.{ .turn_settled = .{
                .sleeve_id = dh.sleeve_id,
                .g = dh.g,
                .turn_end = .cancelled,
                .context_tokens = 0,
                .context_window = 0,
            } }) catch {};
            const settled = self.transcripts.append(dh.sleeve_id, dh.g, dh.turn_id, at_ms, .{
                .turn_settled = .{ .stop = .cancelled, .native_stop = "interrupted" },
            }) catch null;
            if (settled) |c| self.stream_hub.publish_raw(.{ .entry = .{ .seq = 0, .sleeve_id = dh.sleeve_id, .entry = c } });
        }
        const ce = self.transcripts.append(dh.sleeve_id, dh.g, dh.turn_id, at_ms, .{
            .exited = .{
                .code = if (exit_code) |c| @intCast(c) else null,
                .requested = exit.requested,
                .stderr_tail = stderr_tail,
            },
        }) catch null;
        if (ce) |c| self.stream_hub.publish_raw(.{ .entry = .{ .seq = 0, .sleeve_id = dh.sleeve_id, .entry = c } });

        dh.publish_exit(exit);
    }

    fn classify_driver_exit(self: *Runtime, dh: *const DriverHandle, exit_code: ?u8, stderr_tail: []const u8) DriverExit {
        const requested = dh.interrupt_exit.load(.acquire);
        const quota_cause = dh.quota_exit_cause.load(.acquire);
        var cause: model.ExitCause = if (quota_cause != no_driver_exit)
            @fromBackingInt(@intCast(quota_cause))
        else if (exit_code == 0)
            .clean
        else
            quota_exit_cause(d_proto.classify_error(stderr_tail)) orelse .crash;
        if (!requested and exit_code == 137 and dh.memory_max_bytes != null) {
            const killed = dh.guard.sleeve_oom_killed(self.io) catch |err| blk: {
                std.log.warn("read sleeve {s} memory.events after exit: {s}", .{ dh.sleeve_id, @errorName(err) });
                break :blk false;
            };
            if (killed) cause = .oom;
        }
        return .{ .cause = cause, .requested = requested };
    }

    fn record_sleeve_oom(self: *Runtime, dh: *const DriverHandle) void {
        var buf: [256]u8 = undefined;
        const notice = std.fmt.bufPrint(&buf, "Scope OOM at the {d} byte memory limit. Ask Envoy if you need more memory. The owner can raise sleeve_memory_max_overrides for this sleeve.", .{dh.memory_max_bytes.?}) catch unreachable;
        self.record_hub_event(.guardrail, "oom", dh.sleeve_id, notice, .warn);
        const at_ms = std.Io.Clock.real.now(self.io).toMilliseconds();
        const entry = self.transcripts.append(dh.sleeve_id, dh.g, dh.turn_id, at_ms, .{
            .notice = .{ .level = .warn, .code = "oom", .text = notice },
        }) catch null;
        if (entry) |e| self.stream_hub.publish_raw(.{ .entry = .{ .seq = 0, .sleeve_id = dh.sleeve_id, .entry = e } });
    }

    fn reap_driver_exits(self: *Runtime) void {
        while (true) {
            const pending = blk: {
                self.meta_mu.lockUncancelable(self.io);
                defer self.meta_mu.unlock(self.io);
                var it = self.sessions.valueIterator();
                while (it.next()) |entry| {
                    const dh = entry.*;
                    if (dh.driver_exit()) |exit| {
                        const removed = self.sessions.remove(dh.sleeve_id);
                        tiger.assert(removed);
                        break :blk .{ .dh = dh, .exit = exit };
                    }
                    dh.guard.drain_sleeve_after_exit(self.io);
                }
                return;
            };
            self.destroy_driver(pending.dh, pending.exit);
        }
    }

    fn process_driver_handoffs(self: *Runtime) void {
        while (true) {
            const handoff = blk: {
                self.meta_mu.lockUncancelable(self.io);
                defer self.meta_mu.unlock(self.io);
                var it = self.sessions.valueIterator();
                while (it.next()) |entry| {
                    const dh = entry.*;
                    if (!dh.handoff_pending.load(.acquire)) continue;
                    const sid = self.gpa.dupe(u8, dh.sleeve_id) catch return;
                    dh.handoff_pending.store(false, .release);
                    break :blk .{ .sid = sid, .g = dh.g, .harness = dh.harness };
                }
                return;
            };
            defer self.gpa.free(handoff.sid);
            self.commit_with_handoff_generation(.{ .resleeve_requested = .{
                .sleeve_id = handoff.sid,
                .harness = handoff.harness,
                .by = .system,
                .cause = .handoff,
            } }, handoff.g) catch {};
        }
    }

    fn dispatch_job_async(self: *Runtime, job_id: []const u8, node_id: []const u8) void {
        if (!self.enter_worker()) return;
        var spawned = false;
        defer if (!spawned) self.exit_worker();
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
                    const rt = a_ptr.rt;
                    rt.gpa.free(a_ptr.job_id);
                    rt.gpa.free(a_ptr.node_id);
                    rt.gpa.destroy(a_ptr);
                    rt.exit_worker();
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
                            .timeout_secs = jm.timeout_secs,
                            .exclusive = jm.exclusive,
                            .slots = jm.slots,
                            .long_slots = jm.long_slots,
                            .mem_bytes = jm.mem_bytes,
                            .wait_ms = jm.wait_ms,
                            .lease_ttl_ms = jm.lease_ttl_ms,
                            .parent_epoch = jm.parent_epoch,
                        };
                        found = true;
                    }
                    a_ptr.rt.meta_mu.unlock(a_ptr.rt.io);
                }
                if (!found) return;

                const rep = a_ptr.rt.fleet_srv.submit_job(arena, a_ptr.node_id, sj) catch |err| {
                    a_ptr.rt.commit(.{ .job_failed = .{
                        .job_id = a_ptr.job_id,
                        .attempt = 1,
                        .err = @errorName(err),
                    } }) catch {};
                    return;
                };
                {
                    a_ptr.rt.meta_mu.lockUncancelable(a_ptr.rt.io);
                    if (a_ptr.rt.job_meta.getPtr(a_ptr.job_id)) |jm| {
                        const ca = a_ptr.rt.core_state.alloc();
                        if (rep.boot_id) |bid| jm.boot_id = ca.dupe(u8, bid) catch jm.boot_id;
                        if (rep.epoch) |ep| jm.epoch = ep;
                        if (rep.lease_id) |lid| jm.lease_id = ca.dupe(u8, lid) catch jm.lease_id;
                        jm.held_reason = if (rep.held_reason) |hr| (ca.dupe(u8, hr) catch jm.held_reason) else null;
                    }
                    a_ptr.rt.meta_mu.unlock(a_ptr.rt.io);
                }
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
        spawned = true;
        t.detach();
    }

    fn verify_job_async(self: *Runtime, job_id: []const u8, node_id: []const u8) void {
        _ = self;
        _ = job_id;
        _ = node_id;
    }

    fn log_cancel_failure(self: *Runtime, job_id: []const u8, node_id: []const u8, err: anyerror) void {
        var buf: [256]u8 = undefined;
        const reason = std.fmt.bufPrint(&buf, "job cancel failed: {s}", .{@errorName(err)}) catch "job cancel failed";
        self.record_hub_event(.node, node_id, job_id, reason, .warn);
    }

    fn cancel_job_async(self: *Runtime, job_id: []const u8, node_id: []const u8, settle_on_error: bool) !void {
        if (!self.enter_worker()) return error.ShuttingDown;
        errdefer self.exit_worker();
        const Arg = struct { rt: *Runtime, job_id: []const u8, node_id: []const u8, settle_on_error: bool };
        const arg = try self.gpa.create(Arg);
        errdefer self.gpa.destroy(arg);
        const job_id_dup = try self.gpa.dupe(u8, job_id);
        errdefer self.gpa.free(job_id_dup);
        const node_id_dup = try self.gpa.dupe(u8, node_id);
        errdefer self.gpa.free(node_id_dup);
        arg.* = .{
            .rt = self,
            .job_id = job_id_dup,
            .node_id = node_id_dup,
            .settle_on_error = settle_on_error,
        };
        const t = try std.Thread.spawn(.{}, struct {
            fn run(a_ptr: *Arg) void {
                defer {
                    const rt = a_ptr.rt;
                    rt.gpa.free(a_ptr.job_id);
                    rt.gpa.free(a_ptr.node_id);
                    rt.gpa.destroy(a_ptr);
                    rt.exit_worker();
                }
                var arena_st = std.heap.ArenaAllocator.init(a_ptr.rt.gpa);
                defer arena_st.deinit();
                _ = a_ptr.rt.fleet_srv.cancel_job(arena_st.allocator(), a_ptr.node_id, a_ptr.job_id) catch |err| {
                    a_ptr.rt.log_cancel_failure(a_ptr.job_id, a_ptr.node_id, err);
                    if (a_ptr.settle_on_error) a_ptr.rt.commit(.{ .job_cancelled = .{ .job_id = a_ptr.job_id } }) catch {};
                    return;
                };
            }
        }.run, .{arg});
        t.detach();
    }

    /// Pulls up to log_mirror_max bytes of a node job log from offset 0, one piece per call.
    fn fetch_log_mirror(self: *Runtime, arena: std.mem.Allocator, node_id: []const u8, job_id: []const u8) !struct { data: []const u8, next_offset: u64 } {
        var data: std.ArrayList(u8) = .empty;
        var off: u64 = 0;
        while (off < log_mirror_max) {
            const want: usize = @intCast(@min(@as(u64, limits.piece_max), log_mirror_max - off));
            const res = try self.fleet_srv.fetch_log(arena, node_id, job_id, off, want);
            try data.appendSlice(arena, res.data);
            off = res.next_offset;
            if (res.data.len < want or res.eof) break;
        }
        return .{ .data = data.items, .next_offset = off };
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
            const info = self.fleet_srv.job_status(a, pr.node_id, pr.id) catch |err| {
                self.note_status_failure(a, pr.node_id, pr.id, err);
                continue;
            };
            {
                self.meta_mu.lockUncancelable(self.io);
                if (self.job_meta.getPtr(pr.id)) |jm| {
                    jm.status_failures = 0;
                    const ca = self.core_state.alloc();
                    if (info.boot_id) |bid| jm.boot_id = ca.dupe(u8, bid) catch jm.boot_id;
                    if (info.epoch) |ep| jm.epoch = ep;
                    if (info.lease_id) |lid| jm.lease_id = ca.dupe(u8, lid) catch jm.lease_id;
                    jm.held_reason = if (info.held_reason) |hr| (ca.dupe(u8, hr) catch jm.held_reason) else null;
                }
                self.meta_mu.unlock(self.io);
            }
            if (info.log_bytes > 0) {
                if (self.fetch_log_mirror(a, pr.node_id, pr.id)) |lres| {
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

            if (info.state == .succeeded or info.state == .failed or info.state == .cancelled or info.state == .interrupted or info.state == .lost) {
                var arts_list: std.ArrayList(schema.JobArtifact) = .empty;
                var collection_ok = true;
                var collection_err: ?[]const u8 = null;
                for (info.artifacts) |ar| {
                    const declared = ar.size_bytes orelse {
                        collection_ok = false;
                        if (collection_err == null) collection_err = std.fmt.allocPrint(a, "artifact {s} has no declared size", .{ar.path}) catch null;
                        continue;
                    };
                    if (self.fleet_srv.fetch_artifact(a, pr.node_id, pr.id, ar.path, self.hub_home.artifacts_path, declared)) |verified| {
                        arts_list.append(a, .{
                            .path = self.core_state.dupe_str(verified.path) catch ar.path,
                            .size_bytes = verified.size_bytes,
                            .sha256 = if (verified.sha256) |s| (self.core_state.dupe_str(s) catch s) else null,
                        }) catch {};
                    } else |err| {
                        collection_ok = false;
                        if (collection_err == null) collection_err = switch (err) {
                            error.ArtifactTooLarge => std.fmt.allocPrint(
                                a,
                                "artifact {s} ({d} bytes) exceeds the {d} byte artifact cap",
                                .{ ar.path, declared, limits.artifact_max_bytes },
                            ) catch null,
                            else => std.fmt.allocPrint(a, "artifact {s} collection failed: {s}", .{ ar.path, @errorName(err) }) catch null,
                        };
                    }
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
                _ = self.fleet_srv.ack_job(a, pr.node_id, pr.id) catch {};
                if (info.state == .cancelled) {
                    self.commit(.{ .job_cancelled = .{ .job_id = pr.id } }) catch {};
                } else if (info.state == .interrupted) {
                    self.commit(.{ .job_interrupted = .{ .job_id = pr.id, .attempt = 1 } }) catch {};
                } else if (info.state == .lost) {
                    self.commit(.{ .job_failed = .{ .job_id = pr.id, .attempt = 1, .err = info.@"error" orelse "lost on node" } }) catch {};
                } else if (info.state == .succeeded and !collection_ok) {
                    self.commit(.{ .job_failed = .{ .job_id = pr.id, .attempt = 1, .err = collection_err orelse "artifact collection failed" } }) catch {};
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
            self.reap_driver_exits();
            self.process_driver_handoffs();
            self.poll_inflight_jobs();
            if (ticks % 5 == 0) {
                self.poll_sleeve_devices();
            }
            if (ticks % 10 == 0) {
                self.commit(.{ .tick = .{} }) catch {};
            }
        }
    }

    /// A node that cannot answer a job's status (too large to encode, undecodable) would otherwise be retried forever.
    fn note_status_failure(self: *Runtime, arena: std.mem.Allocator, node_id: []const u8, job_id: []const u8, err: anyerror) void {
        switch (err) {
            error.NodeOffline, error.Timeout, error.Busy => return,
            else => {},
        }
        var failures: u8 = 0;
        self.meta_mu.lockUncancelable(self.io);
        if (self.job_meta.getPtr(job_id)) |jm| {
            jm.status_failures +|= 1;
            failures = jm.status_failures;
        }
        self.meta_mu.unlock(self.io);
        if (failures != status_failures_max) return;
        const reason = std.fmt.allocPrint(arena, "job status unreadable: {s}", .{@errorName(err)}) catch "job status unreadable";
        self.record_hub_event(.node, node_id, job_id, reason, .warn);
        self.commit(.{ .job_failed = .{ .job_id = job_id, .attempt = 1, .err = reason } }) catch {};
        self.cancel_job_async(job_id, node_id, false) catch |cancel_err| self.log_cancel_failure(job_id, node_id, cancel_err);
    }

    fn hook_allowed_peers(ctx: ?*anyopaque, arena: std.mem.Allocator) ?[]fleet.AllowedPeer {
        const self: *Runtime = @ptrCast(@alignCast(ctx orelse return null));
        self.state_mu.lockUncancelable(self.io);
        defer self.state_mu.unlock(self.io);

        var out: std.ArrayList(fleet.AllowedPeer) = .empty;
        var it = self.core_state.nodes.iterator();
        while (it.next()) |e| {
            const n = e.value_ptr.*;
            if (n.approved and !n.removed) {
                out.append(arena, .{
                    .node_id = arena.dupe(u8, n.id) catch return null,
                    .name = arena.dupe(u8, n.name) catch return null,
                    .pub_key = n.pub_key,
                }) catch return null;
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
        if (hello.boot_id.len > 0) tel.boot_id = a.dupe(u8, hello.boot_id) catch tel.boot_id;
        tel.quarantined = if (hello.quarantined) |q| (a.dupe(u8, q) catch tel.quarantined) else null;
        tel.presence = .{
            .state = hello.presence.state,
            .idle_secs = hello.presence.idle_secs,
            .locked = hello.presence.locked,
            .source = a.dupe(u8, hello.presence.source) catch "",
            .sampled_at_ms = hello.presence.sampled_at_ms,
        };
        gop.value_ptr.* = tel;
    }

    pub fn resolve_node_id(self: *Runtime, arena: std.mem.Allocator, id_or_name: []const u8) ?[]const u8 {
        self.state_mu.lockUncancelable(self.io);
        defer self.state_mu.unlock(self.io);
        if (self.core_state.nodes.contains(id_or_name)) {
            return arena.dupe(u8, id_or_name) catch null;
        }
        var it = self.core_state.nodes.iterator();
        while (it.next()) |e| {
            const n = e.value_ptr.*;
            if (std.mem.eql(u8, n.name, id_or_name)) {
                return arena.dupe(u8, n.id) catch null;
            }
        }
        return null;
    }

    pub fn resolve_sleeve_id(self: *Runtime, arena: std.mem.Allocator, id_or_name: []const u8) ?[]const u8 {
        self.state_mu.lockUncancelable(self.io);
        defer self.state_mu.unlock(self.io);
        if (self.core_state.sleeves.contains(id_or_name)) {
            return arena.dupe(u8, id_or_name) catch null;
        }
        var fallback: ?[]const u8 = null;
        var it = self.core_state.sleeves.iterator();
        while (it.next()) |e| {
            const sl = e.value_ptr.*;
            if (std.mem.eql(u8, sl.name, id_or_name)) {
                if (sl.lifecycle != .removed) {
                    return arena.dupe(u8, sl.id) catch null;
                }
                if (fallback == null) fallback = sl.id;
            }
        }
        if (fallback) |fb| return arena.dupe(u8, fb) catch null;
        return null;
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
        self.record_hub_event_at(kind, source, target, text, level, now, true);
    }

    fn record_hub_event_at(
        self: *Runtime,
        kind: schema.EventKind,
        source: []const u8,
        target: ?[]const u8,
        text: []const u8,
        level: schema.Level,
        at_ms: i64,
        publish: bool,
    ) void {
        self.meta_mu.lockUncancelable(self.io);
        defer self.meta_mu.unlock(self.io);
        const seq = self.next_hub_event_seq;
        self.next_hub_event_seq += 1;
        const ev: schema.HubEvent = .{
            .seq = seq,
            .at_ms = .{ .ms = at_ms },
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
        if (publish) self.stream_hub.publish_raw(.{ .event = .{ .seq = 0, .event = ev } });
    }

    fn publish_event_updates(self: *Runtime, arena: std.mem.Allocator, ev: event.Event) void {
        switch (ev) {
            inline .sleeve_created, .provisioned, .provision_failed, .start_requested, .launched, .session_bound, .prompt_enqueued, .prompt_delivered, .turn_started, .turn_settled, .exit_classified, .stop_requested, .stopped, .resleeve_requested, .parked, .exited => |s| {
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
            .clients = try self.list_client_assets(arena),
            .job_artifact_max_bytes = limits.artifact_max_bytes,
            .pools = try self.view_pools(arena),
        };
    }

    /// The HTTP server hands its pool gauges to hub info through this; detach (null) before freeing them.
    pub fn set_pool_source(self: *Runtime, context: ?*const anyopaque, source: ?*const fn (*const anyopaque, *std.ArrayList(gauge.Reading), std.mem.Allocator) std.mem.Allocator.Error!void) void {
        while (!self.pool_source_lock.tryLock()) std.atomic.spinLoopHint();
        defer self.pool_source_lock.unlock();
        self.pool_source_context = context;
        self.pool_source = source;
    }

    fn view_pools(self: *Runtime, arena: std.mem.Allocator) ![]const schema.PoolStat {
        var readings: std.ArrayList(gauge.Reading) = .empty;
        try self.fleet_srv.pools(&readings, arena);
        {
            while (!self.pool_source_lock.tryLock()) std.atomic.spinLoopHint();
            defer self.pool_source_lock.unlock();
            if (self.pool_source) |source| try source(self.pool_source_context.?, &readings, arena);
        }
        const out = try arena.alloc(schema.PoolStat, readings.items.len);
        for (readings.items, out) |r, *o| o.* = .{
            .name = r.name,
            .size = r.size,
            .reserved = r.reserved,
            .used = r.used,
            .high_water = r.high_water,
            .refused = r.refused,
        };
        return out;
    }

    /// Return known client bundles staged under `<clients>/<version>/` or `<clients>/current/`.
    pub fn list_client_assets(self: *Runtime, arena: std.mem.Allocator) ![]const []const u8 {
        var found: std.ArrayList([]const u8) = .empty;
        for (u_store.known_client_assets) |asset_name| {
            if (try self.resolve_client_asset_path(arena, asset_name) != null) {
                try found.append(arena, asset_name);
            }
        }
        return found.items;
    }

    /// Resolve a client download filename safely under `<clients>/<version>/` or `<clients>/current/`.
    pub fn resolve_client_asset_path(self: *Runtime, arena: std.mem.Allocator, name: []const u8) !?[]const u8 {
        if (name.len == 0 or name[0] == '.') return null;
        if (std.mem.findScalar(u8, name, '/') != null or std.mem.findScalar(u8, name, '\\') != null) return null;
        if (std.mem.find(u8, name, "..") != null) return null;

        const ver_p = try std.fs.path.join(arena, &.{ self.hub_home.clients_path, version.version, name });
        if (std.Io.Dir.cwd().statFile(self.io, ver_p, .{})) |st| {
            if (st.kind == .file) return ver_p;
        } else |_| {}

        const cur_p = try std.fs.path.join(arena, &.{ self.hub_home.clients_path, "current", name });
        if (std.Io.Dir.cwd().statFile(self.io, cur_p, .{})) |st| {
            if (st.kind == .file) return cur_p;
        } else |_| {}

        return null;
    }

    pub fn view_sleeve(self: *Runtime, arena: std.mem.Allocator, sleeve_id: []const u8) !?schema.Sleeve {
        self.state_mu.lockUncancelable(self.io);
        defer self.state_mu.unlock(self.io);
        const sl = self.core_state.sleeves.get(sleeve_id) orelse return null;
        return try sleeve_to_schema(arena, sl, self.sleeve_memory_limit(sl));
    }

    fn sleeve_memory_limit(self: *Runtime, sl: state.SleeveState) ?u64 {
        const inc = sl.inc orelse return null;
        self.meta_mu.lockUncancelable(self.io);
        defer self.meta_mu.unlock(self.io);
        const dh = self.sessions.get(sl.id) orelse return null;
        return if (dh.g == inc.g) dh.memory_max_bytes else null;
    }

    pub fn view_state(self: *Runtime, arena: std.mem.Allocator) !schema.HubState {
        const seq = self.stream_hub.current_seq();
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
                const sl = e.value_ptr.*;
                try sleeves_list.append(arena, try sleeve_to_schema(arena, sl, self.sleeve_memory_limit(sl)));
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
            q_list = try quotas_to_schema(arena, self.cached_quotas);
        }

        return .{
            .seq = seq,
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
            .boot_id = if (tel) |t| try arena.dupe(u8, t.boot_id) else "",
            .quarantined = if (tel) |t| (if (t.quarantined) |q| try arena.dupe(u8, q) else null) else null,
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
            .boot_id = if (jm) |m| m.boot_id else null,
            .epoch = if (jm) |m| m.epoch else null,
            .lease_id = if (jm) |m| m.lease_id else null,
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
        const wait_secs = std.math.clamp(self.hub_home.config.d9_approval_timeout_secs, 1, 600);
        try self.commit(.{ .approval_requested = .{
            .approval_id = app_id,
            .sleeve_id = "envoy",
            .g = cur_g,
            .kind = .action,
            .title = title,
            .summary = summary,
            .is_d9 = true,
            .timeout_ms = @intCast(wait_secs * 1_000),
        } });
        self.record_hub_event(.approval, "d9", "envoy", summary, .warn);

        var cursor: u64 = 0;
        while (true) {
            const deadline = blk: {
                self.state_mu.lockUncancelable(self.io);
                defer self.state_mu.unlock(self.io);
                const ap = self.core_state.approvals.get(app_id) orelse return error.ApprovalDenied;
                break :blk switch (ap.state) {
                    .approved => return try arena.dupe(u8, app_id),
                    .denied, .cancelled, .fenced, .abandoned => return error.ApprovalDenied,
                    .pending, .expired => ap.timeout_at_ms,
                };
            };
            const now = std.Io.Clock.real.now(self.io).toMilliseconds();
            if (now >= deadline) {
                self.commit(.{ .approval_expired = .{ .approval_id = app_id } }) catch {};
                return error.ApprovalTimeout;
            }
            cursor = try self.stream_hub.wait_change(cursor, @min(500, deadline - now));
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
            const deadline = std.Io.Clock.real.now(self.io).toMilliseconds() + 90_000;
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
                cursor = try self.stream_hub.wait_change(cursor, @min(500, deadline - now));
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
        if (!self.enter_worker()) return;
        var spawned = false;
        defer if (!spawned) self.exit_worker();
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
                    const rt = a_ptr.rt;
                    rt.gpa.free(a_ptr.rollout_id);
                    rt.gpa.free(a_ptr.node_id);
                    rt.gpa.free(a_ptr.target_ver);
                    rt.gpa.destroy(a_ptr);
                    rt.exit_worker();
                }
                var arena_st = std.heap.ArenaAllocator.init(a_ptr.rt.gpa);
                defer arena_st.deinit();
                const arena = arena_st.allocator();

                const ch_str: []const u8 = if (std.mem.findScalar(u8, a_ptr.target_ver, '-') != null)
                    "dev"
                else
                    a_ptr.rt.hub_home.config.update_channel;
                const rep = a_ptr.rt.fleet_srv.send_update(arena, a_ptr.node_id, .{
                    .version = a_ptr.target_ver,
                    .channel = ch_str,
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
                if (!rep.ok) {
                    if (a_ptr.rt.push_node_update_from_hub(arena, a_ptr.rollout_id, a_ptr.node_id, a_ptr.target_ver, ch_str)) |_| {
                        a_ptr.rt.commit(.{ .rollout_node_result = .{
                            .rollout_id = a_ptr.rollout_id,
                            .node_id = a_ptr.node_id,
                            .ok = true,
                            .detail = "uploaded from hub",
                        } }) catch {};
                        return;
                    } else |_| {}
                }
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
        spawned = true;
        t.detach();
    }

    fn push_node_update_from_hub(
        self: *Runtime,
        arena: std.mem.Allocator,
        rollout_id: []const u8,
        node_id: []const u8,
        target_ver: []const u8,
        ch_str: []const u8,
    ) !void {
        var node_os: []const u8 = "linux";
        var node_arch: []const u8 = "";
        {
            self.meta_mu.lockUncancelable(self.io);
            defer self.meta_mu.unlock(self.io);
            if (self.node_tele.get(node_id)) |nt| {
                if (nt.inventory) |inv| {
                    if (inv.os.len > 0) node_os = try arena.dupe(u8, inv.os);
                    if (inv.arch.len > 0) node_arch = try arena.dupe(u8, inv.arch);
                }
            }
        }
        var node_ver: []const u8 = "0.0.0";
        {
            self.state_mu.lockUncancelable(self.io);
            defer self.state_mu.unlock(self.io);
            if (self.core_state.nodes.get(node_id)) |nr| {
                if (node_arch.len == 0 and nr.arch.len > 0) node_arch = try arena.dupe(u8, nr.arch);
                if (nr.version.len > 0) node_ver = try arena.dupe(u8, nr.version);
            }
        }
        const want_asset = u_store.asset_name_for(.node, node_os, node_arch) orelse return error.UnsupportedTarget;
        const ch_enum: u_ver.Channel = if (std.mem.eql(u8, ch_str, "dev")) .dev else .stable;
        const rel = (try u_store.check_latest_for_asset(
            arena,
            self.io,
            self.hub_home.config.update_repo,
            ch_enum,
            node_ver,
            want_asset,
            target_ver,
        )) orelse return error.ReleaseNotFound;

        const dl = try u_store.download_release_asset(self.gpa, self.io, rel);
        defer self.gpa.free(dl.bytes);

        const hex_chars = "0123456789abcdef";
        const chunk_max: usize = 128 * 1024;
        const hex_buf = try arena.alloc(u8, chunk_max * 2);
        var off: usize = 0;
        var ready = false;
        while (off < dl.bytes.len or dl.bytes.len == 0) {
            const end = @min(off + chunk_max, dl.bytes.len);
            const slice = dl.bytes[off..end];
            for (slice, 0..) |b, i| {
                hex_buf[i * 2] = hex_chars[b >> 4];
                hex_buf[i * 2 + 1] = hex_chars[b & 0x0f];
            }
            const up_rep = try self.fleet_srv.upload_update(arena, node_id, .{
                .rollout_id = rollout_id,
                .version = rel.tag,
                .sha256 = &dl.sha256_hex,
                .offset = off,
                .total_bytes = dl.bytes.len,
                .hex = hex_buf[0 .. slice.len * 2],
                .eof = (end == dl.bytes.len),
            });
            off = end;
            ready = up_rep.ready;
            if (dl.bytes.len == 0) break;
        }
        if (!ready) return error.UploadIncomplete;

        const act = try self.fleet_srv.send_update(arena, node_id, .{
            .version = rel.tag,
            .channel = ch_str,
            .rollout_id = rollout_id,
        });
        if (!act.ok) return error.ActivateFailed;
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

fn harness_program(harness: model.Harness) []const u8 {
    return switch (harness) {
        .claude => "claude",
        .codex => "codex",
        .grok => "grok",
        .pi_vendor, .pi_local => "pi",
        .antigravity => "agy",
    };
}

fn doctor_eligible(health: []const schema.DoctorHarness, harness: model.Harness) bool {
    for (health) |check| {
        if (std.mem.eql(u8, check.name, harness_program(harness))) return check.eligible;
    }
    return false;
}

fn quota_exit_cause(kind: d_proto.ErrorClass) ?model.ExitCause {
    return switch (kind) {
        .quota => .quota_exhausted,
        .auth => .auth_error,
        else => null,
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

pub fn model_to_schema_harness(h: model.Harness) schema.Harness {
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

fn sleeve_to_schema(arena: std.mem.Allocator, sl: state.SleeveState, memory_max_bytes: ?u64) !schema.Sleeve {
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
            .pi_vendor => .api,
            .antigravity => .antigravity,
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
        .memory_max_bytes = memory_max_bytes,
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
