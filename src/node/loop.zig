const std = @import("std");
const tiger = @import("../tiger.zig");
const version = @import("../version.zig");
const io_net = @import("../net/io.zig");
const noise = @import("../net/noise.zig");
const frame = @import("../net/frame.zig");
const msg = @import("../net/msg.zig");
const bounded = @import("../net/bounded.zig");
const limits = @import("../net/limits.zig");
const node_link = @import("../net/node_link.zig");
const node_loop = @import("../net/node_loop.zig");
const backend_mod = @import("../net/backend.zig");
const u_ver = @import("../update/version.zig");
const u_store = @import("../update/store.zig");
const sys = @import("sys.zig");
const admission = @import("admission.zig");
const runner = @import("runner.zig");

const Backend = backend_mod.Backend;

pub const workers_max = 14;
pub const waiting_leases_max = 10;
const worker_stack_bytes = 4 * 1024 * 1024;
const worker_idle_ms = 10_000;
const telemetry_connected_ms = 2_500;
const telemetry_idle_ms = 10_000;
const probe_wait_ms = 8_000;
const addr_endpoints_max = 4;
const hello_max = limits.fleet_hello_max;

comptime {
    tiger.comptime_assert(waiting_leases_max + 4 <= workers_max, "control requests keep workers");
    tiger.comptime_assert(hello_max <= frame.max_payload, "a hello fits one frame");
}

pub const Sync = struct {
    io: std.Io,
    mu: std.Io.Mutex = .init,
    cond: std.Io.Condition = .init,

    pub fn lock(self: *Sync) void {
        self.mu.lockUncancelable(self.io);
    }

    pub fn unlock(self: *Sync) void {
        self.mu.unlock(self.io);
    }
};

const Core = node_loop.NodeLoop(Backend, Sync);

pub const Config = struct {
    gpa: std.mem.Allocator,
    io: std.Io,
    env: ?*const std.process.Environ.Map,
    home_path: []const u8,
    node_id: []const u8,
    addrs: []const []const u8,
    key: noise.KeyPair,
    hub_key: [32]u8,
    inventory: msg.Inventory,
    adm: *admission.AdmissionManager,
    run_mgr: *runner.Runner,
    presence: *sys.PresenceDetector,
};

pub fn split_host_port(addr: []const u8, default_port: u16) ?struct { host: []const u8, port: u16 } {
    const tr = std.mem.trim(u8, addr, " \t");
    if (tr.len == 0) return null;
    if (tr[0] == '[') {
        const end_b = std.mem.findScalar(u8, tr, ']') orelse return null;
        const host = tr[1..end_b];
        if (end_b + 1 < tr.len and tr[end_b + 1] == ':') {
            const p = std.fmt.parseInt(u16, tr[end_b + 2 ..], 10) catch return null;
            return .{ .host = host, .port = p };
        }
        return .{ .host = host, .port = default_port };
    }
    if (std.mem.findScalarLast(u8, tr, ':')) |colon| {
        if (std.mem.findScalar(u8, tr, ':') == colon) {
            const p = std.fmt.parseInt(u16, tr[colon + 1 ..], 10) catch return null;
            return .{ .host = tr[0..colon], .port = p };
        }
    }
    return .{ .host = tr, .port = default_port };
}

fn to_endpoint(address: std.Io.net.IpAddress) io_net.Endpoint {
    return switch (address) {
        .ip4 => |a| blk: {
            var bytes: [16]u8 = @splat(0);
            @memcpy(bytes[0..4], &a.bytes);
            break :blk .{ .address = bytes, .port = a.port, .family = .ipv4 };
        },
        .ip6 => |a| .{ .address = a.bytes, .port = a.port, .family = .ipv6 },
    };
}

/// Turns the saved hub addresses into endpoints, resolving names. Blocks on DNS, so never call it on the loop thread.
pub fn resolve(io: std.Io, addrs: []const []const u8, out: *[node_link.candidate_limit]io_net.Endpoint) usize {
    var count: usize = 0;
    for (addrs) |text| {
        const hp = split_host_port(text, 7475) orelse continue;
        if (std.Io.net.IpAddress.parse(hp.host, hp.port)) |address| {
            count = add_endpoint(out, count, to_endpoint(address));
            continue;
        } else |_| {}
        const host = std.Io.net.HostName.init(hp.host) catch continue;
        var buffer: [32]std.Io.net.HostName.LookupResult = undefined;
        var queue: std.Io.Queue(std.Io.net.HostName.LookupResult) = .init(&buffer);
        host.lookup(io, &queue, .{ .port = hp.port }) catch continue;
        var taken: usize = 0;
        while (queue.getOneUncancelable(io)) |item| switch (item) {
            .address => |address| if (taken < addr_endpoints_max) {
                count = add_endpoint(out, count, to_endpoint(address));
                taken += 1;
            },
            .canonical_name => {},
        } else |_| {}
    }
    return count;
}

fn add_endpoint(out: *[node_link.candidate_limit]io_net.Endpoint, count: usize, endpoint: io_net.Endpoint) usize {
    if (count == out.len) return count;
    for (out[0..count]) |existing| if (std.meta.eql(existing, endpoint)) return count;
    out[count] = endpoint;
    return count + 1;
}

pub const HelloSource = struct {
    io: std.Io,
    env: ?*const std.process.Environ.Map,
    node_id: []const u8,
    addrs: []const []const u8,
    inventory: msg.Inventory,
    adm: ?*admission.AdmissionManager,
    presence: ?*sys.PresenceDetector,

    /// Encodes a hello into out; the first hello of a session carries the inventory, later ones are heartbeats.
    pub fn build(self: *const HelloSource, arena: std.mem.Allocator, with_inventory: bool, out: []u8) ![]const u8 {
        const boot_id = if (self.adm) |adm| try adm.boot_id(arena) else (sys.read_boot_id(arena, self.io) catch "boot-0");
        const quarantined = if (self.adm) |adm| adm.quarantined_copy(arena) else null;
        const presence = if (self.presence) |pd| pd.collect(self.io, self.env) else sys.collect_presence(self.io, self.env);
        const request: msg.Request = .{ .hello = .{
            .node_id = self.node_id,
            .node_version = version.version,
            .boot_id = boot_id,
            .quarantined = quarantined,
            .addrs = self.addrs,
            .inventory = if (with_inventory) self.inventory else null,
            .sample = sys.collect_sample(self.io),
            .presence = presence,
        } };
        return msg.encode_fixed(out, request) catch error.HelloTooLarge;
    }
};

fn hello_accepted(payload: []const u8) bool {
    var scratch: [4096]u8 = undefined;
    var fba = std.heap.FixedBufferAllocator.init(&scratch);
    const parsed = msg.decode_arena(msg.HelloReply, fba.allocator(), payload) catch return false;
    return parsed.ok;
}

/// Connects once and reports whether the hub accepted a hello: the node's `authorize`.
pub fn probe(source: *const HelloSource, key: noise.KeyPair, hub_key: [32]u8) !bool {
    const Link = node_link.NodeLink(Backend);
    var arena_st = std.heap.ArenaAllocator.init(std.heap.page_allocator);
    defer arena_st.deinit();
    const arena = arena_st.allocator();
    const hello_buffer = try arena.alloc(u8, hello_max);
    const hello = try source.build(arena, true, hello_buffer);

    var endpoints: [node_link.candidate_limit]io_net.Endpoint = undefined;
    const count = resolve(source.io, source.addrs, &endpoints);
    if (count == 0) return false;

    var backend = try Backend.init();
    defer backend.deinit();
    var pool = try bounded.SlabPool.init(std.heap.page_allocator);
    defer pool.deinit(std.heap.page_allocator);
    const link = try std.heap.page_allocator.create(Link);
    defer std.heap.page_allocator.destroy(link);
    const Accept = struct {
        fn reply(_: *anyopaque, payload: []const u8) bool {
            return hello_accepted(payload);
        }
    };
    var unused: u8 = 0;
    try link.init(&backend, &pool, endpoints[0..count], key, hub_key, hello, .{ .context = &unused, .hello_reply = Accept.reply });
    defer link.deinit();

    const deadline = backend.now() + probe_wait_ms;
    var accepted = false;
    var stopping = false;
    while (true) {
        try link.drive();
        while (link.event()) |event| {
            if (event == .connected) accepted = true;
            link.consume(event);
        }
        if (!stopping and (accepted or backend.now() >= deadline)) {
            stopping = true;
            link.shutdown();
        }
        if (stopping and link.done()) break;
        try backend.poll(bounded.tick_ms);
    }
    return accepted;
}

pub const Server = struct {
    const Task = node_loop.Task;

    cfg: Config,
    static: *bounded.StaticAllocator,
    backend: *Backend,
    pool: *bounded.SlabPool,
    sync: *Sync,
    core: *Core,
    source: HelloSource,
    full: *[hello_max]u8,
    full_len: usize,
    beat: *[hello_max]u8,
    beat_len: usize,
    staged_full: *[hello_max]u8,
    staged_beat: *[hello_max]u8,
    staged_full_len: usize = 0,
    staged_beat_len: usize = 0,
    staged_ok: bool = false,
    resolved: [node_link.candidate_limit]io_net.Endpoint = undefined,
    resolved_count: usize = 0,
    telemetry_inflight: bool = false,
    telemetry_next: u64 = 0,
    healthy_wanted: bool = false,
    threads: [workers_max]?std.Thread = @splat(null),
    alive: [workers_max]bool = @splat(false),
    live: usize = 0,
    idle: usize = 0,
    quitting: bool = false,
    stop_requested: std.atomic.Value(bool) = .init(false),
    waiting_leases: std.atomic.Value(u32) = .init(0),

    pub const reserved_bytes: usize = @sizeOf(Server) + @sizeOf(Backend) + @sizeOf(bounded.SlabPool) + bounded.slab_count * bounded.slab_bytes +
        @sizeOf(Core) + node_loop.work_slots * @sizeOf(node_loop.Buffers) + 4 * hello_max + @sizeOf(Sync) + @sizeOf(bounded.StaticAllocator);

    pub fn create(cfg: Config) !*Server {
        const parent = std.heap.page_allocator;
        const static = try parent.create(bounded.StaticAllocator);
        errdefer parent.destroy(static);
        static.* = .{ .parent = parent };
        const a = static.allocator();

        const self = try a.create(Server);
        errdefer a.destroy(self);
        const backend = try a.create(Backend);
        errdefer a.destroy(backend);
        backend.* = try Backend.init();
        errdefer backend.deinit();
        const pool = try a.create(bounded.SlabPool);
        errdefer a.destroy(pool);
        pool.* = try bounded.SlabPool.init(a);
        errdefer pool.deinit(a);
        const sync = try a.create(Sync);
        errdefer a.destroy(sync);
        sync.* = .{ .io = cfg.io };
        const core = try bounded.create_reserved(a, Core);
        errdefer bounded.destroy_reserved(a, Core, core);
        const full = try a.create([hello_max]u8);
        errdefer a.destroy(full);
        const beat = try a.create([hello_max]u8);
        errdefer a.destroy(beat);
        const staged_full = try a.create([hello_max]u8);
        errdefer a.destroy(staged_full);
        const staged_beat = try a.create([hello_max]u8);
        errdefer a.destroy(staged_beat);

        self.* = .{
            .cfg = cfg,
            .static = static,
            .backend = backend,
            .pool = pool,
            .sync = sync,
            .core = core,
            .source = .{
                .io = cfg.io,
                .env = cfg.env,
                .node_id = cfg.node_id,
                .addrs = cfg.addrs,
                .inventory = cfg.inventory,
                .adm = cfg.adm,
                .presence = cfg.presence,
            },
            .full = full,
            .full_len = 0,
            .beat = beat,
            .beat_len = 0,
            .staged_full = staged_full,
            .staged_beat = staged_beat,
        };
        {
            var arena_st = std.heap.ArenaAllocator.init(cfg.gpa);
            defer arena_st.deinit();
            self.full_len = (try self.source.build(arena_st.allocator(), true, full)).len;
            self.beat_len = (try self.source.build(arena_st.allocator(), false, beat)).len;
        }
        try core.init(a, backend, pool, sync, &.{}, cfg.key, cfg.hub_key, full[0..self.full_len], .{
            .context = self,
            .hello_reply = hello_reply,
            .task_done = task_done,
            .connected = connected,
            .disconnected = disconnected,
            .notify = notify,
            .resolve = true,
        });
        tiger.assert(reserved_bytes <= 96 * 1024 * 1024);
        static.freeze();
        return self;
    }

    pub fn destroy(self: *Server) void {
        const static = self.static;
        static.begin_deinit();
        const a = static.allocator();
        self.core.deinit(a);
        a.destroy(self.staged_beat);
        a.destroy(self.staged_full);
        a.destroy(self.beat);
        a.destroy(self.full);
        bounded.destroy_reserved(a, Core, self.core);
        a.destroy(self.sync);
        self.pool.deinit(a);
        a.destroy(self.pool);
        const backend = self.backend;
        backend.deinit();
        a.destroy(backend);
        a.destroy(self);
        static.finish_deinit();
        std.heap.page_allocator.destroy(static);
    }

    pub fn request_stop(self: *Server) void {
        self.stop_requested.store(true, .release);
        self.backend.wake();
    }

    /// Runs the node's one loop on the calling thread until it is asked to stop or an update has restarted it.
    pub fn run(self: *Server) void {
        var stopping = false;
        while (true) {
            if (!stopping and (self.stop_requested.load(.acquire) or self.core.stop_reply_sent)) {
                stopping = true;
                self.core.shutdown();
                self.adm_interrupt();
            }
            if (!stopping) self.schedule();
            self.core.turn();
            if (stopping and self.core.done()) break;
            const wait: u32 = if (self.core.stalled) 2 else bounded.tick_ms;
            self.backend.poll(wait) catch |err| std.debug.panic("node loop: backend poll failed: {s}", .{@errorName(err)});
        }
        self.stop_workers();
        self.core.drain();
        tiger.assert(self.core.quiescent());
    }

    fn adm_interrupt(self: *Server) void {
        self.cfg.adm.interrupt();
    }

    fn schedule(self: *Server) void {
        const link = &self.core.link;
        const established = link.phase == .established or link.phase == .hello;
        link.set_hello(if (established) self.beat[0..self.beat_len] else self.full[0..self.full_len]);
        const now = self.backend.now();
        if (!self.telemetry_inflight and now >= self.telemetry_next) {
            if (self.core.submit_task(.telemetry, 0)) self.telemetry_inflight = true else self.core.stalled = true;
        }
        if (self.healthy_wanted and self.core.submit_task(.healthy, 0)) self.healthy_wanted = false;
        self.kick();
    }

    fn kick(self: *Server) void {
        self.sync.lock();
        const queued = self.core.work.len;
        self.sync.unlock();
        if (queued > 0) notify(self);
    }

    fn hello_reply(_: *anyopaque, payload: []const u8) bool {
        return hello_accepted(payload);
    }

    fn connected(context: *anyopaque) void {
        const self: *Server = @ptrCast(@alignCast(context));
        self.healthy_wanted = true;
        std.debug.print("node: connected to the hub\n", .{});
    }

    fn disconnected(_: *anyopaque) void {
        std.debug.print("node: lost the hub; reconnecting\n", .{});
    }

    fn task_done(context: *anyopaque, task: Task, id: u64) void {
        const self: *Server = @ptrCast(@alignCast(context));
        switch (task) {
            .resolve => self.core.link.set_endpoints(id, self.resolved[0..self.resolved_count]),
            .telemetry => {
                self.telemetry_inflight = false;
                const established = self.core.link.phase == .established;
                self.telemetry_next = self.backend.now() + @as(u64, if (established) telemetry_connected_ms else telemetry_idle_ms);
                if (self.staged_ok) {
                    @memcpy(self.full[0..self.staged_full_len], self.staged_full[0..self.staged_full_len]);
                    self.full_len = self.staged_full_len;
                    @memcpy(self.beat[0..self.staged_beat_len], self.staged_beat[0..self.staged_beat_len]);
                    self.beat_len = self.staged_beat_len;
                }
            },
            .healthy, .request => {},
        }
    }

    fn notify(context: *anyopaque) void {
        const self: *Server = @ptrCast(@alignCast(context));
        self.sync.lock();
        const spawn = self.core.work.len > self.idle and self.live < workers_max and !self.quitting;
        self.sync.unlock();
        self.sync.cond.signal(self.cfg.io);
        if (spawn) self.spawn_worker();
    }

    fn spawn_worker(self: *Server) void {
        var slot: ?usize = null;
        self.sync.lock();
        for (self.alive, 0..) |running, i| {
            if (!running) {
                slot = i;
                break;
            }
        }
        self.sync.unlock();
        const index = slot orelse return;
        if (self.threads[index]) |old| old.join();
        self.threads[index] = null;
        self.sync.lock();
        self.alive[index] = true;
        self.live += 1;
        self.sync.unlock();
        self.threads[index] = std.Thread.spawn(.{ .stack_size = worker_stack_bytes }, worker_main, .{ self, index }) catch {
            self.sync.lock();
            self.alive[index] = false;
            self.live -= 1;
            self.sync.unlock();
            return;
        };
    }

    fn stop_workers(self: *Server) void {
        self.sync.lock();
        self.quitting = true;
        self.sync.unlock();
        self.sync.cond.broadcast(self.cfg.io);
        for (&self.threads) |*thread| {
            if (thread.*) |t| t.join();
            thread.* = null;
        }
    }

    fn worker_main(self: *Server, index: usize) void {
        var arena_st = std.heap.ArenaAllocator.init(self.cfg.gpa);
        defer arena_st.deinit();
        while (true) {
            var work = self.core.next_work() orelse {
                self.sync.lock();
                var expired = self.idle > 0 and self.core.work.len == 0;
                while (self.core.work.len == 0 and !self.quitting and !expired) {
                    self.idle += 1;
                    self.sync.cond.waitTimeout(self.cfg.io, &self.sync.mu, .{
                        .duration = .{ .raw = .fromMilliseconds(worker_idle_ms), .clock = .awake },
                    }) catch {
                        expired = true;
                    };
                    self.idle -= 1;
                }
                const quit = self.quitting or (expired and self.core.work.len == 0);
                if (quit) {
                    self.alive[index] = false;
                    self.live -= 1;
                }
                self.sync.unlock();
                if (quit) return;
                continue;
            };
            self.sync.lock();
            const quit = self.quitting;
            self.sync.unlock();
            if (!quit) {
                _ = arena_st.reset(.retain_capacity);
                self.handle(arena_st.allocator(), &work);
            } else work.no_reply = true;
            self.core.finish_work(&work);
            self.backend.wake();
            if (quit) {
                self.sync.lock();
                self.alive[index] = false;
                self.live -= 1;
                self.sync.unlock();
                return;
            }
        }
    }

    fn handle(self: *Server, arena: std.mem.Allocator, work: *node_loop.Work) void {
        switch (work.task) {
            .resolve => self.resolved_count = resolve(self.cfg.io, self.cfg.addrs, &self.resolved),
            .telemetry => self.refresh_hello(arena),
            .healthy => {
                const dir = std.fs.path.join(arena, &.{ self.cfg.home_path, "releases" }) catch return;
                u_store.mark_healthy(self.cfg.gpa, self.cfg.io, dir) catch {};
            },
            .request => self.serve(arena, work),
        }
    }

    fn refresh_hello(self: *Server, arena: std.mem.Allocator) void {
        self.staged_ok = false;
        const full = self.source.build(arena, true, self.staged_full) catch return;
        const beat = self.source.build(arena, false, self.staged_beat) catch return;
        self.staged_full_len = full.len;
        self.staged_beat_len = beat.len;
        self.staged_ok = true;
    }

    fn serve(self: *Server, arena: std.mem.Allocator, work: *node_loop.Work) void {
        const request = msg.decode_arena(msg.Request, arena, work.request) catch return fail(work, .invalid, "bad request json");
        self.dispatch(arena, work, request) catch |err| fail(work, .internal, @errorName(err));
    }

    fn dispatch(self: *Server, arena: std.mem.Allocator, w: *node_loop.Work, req: msg.Request) !void {
        const run_mgr = self.cfg.run_mgr;
        const adm = self.cfg.adm;
        switch (req) {
            .ping => |p| reply(w, .response, msg.Ping{ .now_ms = p.now_ms }),
            .submit_job => |sj| reply(w, .response, msg.JobReply{ .job = try run_mgr.submit(arena, sj) }),
            .job_status => |js| {
                if (try run_mgr.get_status(arena, js.job_id)) |info| {
                    reply(w, .response, msg.JobReply{ .job = info });
                } else fail(w, .not_found, "unknown job");
            },
            .cancel_job => |jr| {
                if (try run_mgr.cancel(arena, jr.job_id)) |info| {
                    reply(w, .response, msg.JobReply{ .job = info });
                } else fail(w, .not_found, "unknown job");
            },
            .list_jobs => reply(w, .response, msg.JobList{ .jobs = try run_mgr.list_all(arena) }),
            .fetch_log => |fl| {
                const max_chunk = @min(fl.limit orelse limits.piece_max, limits.piece_max);
                if (try run_mgr.read_log(arena, fl.job_id, fl.offset, max_chunk)) |res| {
                    take_chunk(w, res.data);
                    reply(w, .end, msg.LogEnd{ .eof = res.eof, .next_offset = res.next_offset });
                } else fail(w, .not_found, "log not found");
            },
            .fetch_artifact => |fa| {
                const max_chunk = @min(fa.limit orelse limits.piece_max, limits.piece_max);
                if (try run_mgr.read_artifact(arena, fa.job_id, fa.path, fa.offset, max_chunk)) |res| {
                    take_chunk(w, res.data);
                    reply(w, .end, msg.ArtifactEnd{
                        .eof = res.eof,
                        .next_offset = res.next_offset,
                        .size_bytes = res.size_bytes,
                        .sha256 = res.sha256,
                    });
                } else fail(w, .not_found, "artifact not found");
            },
            .update => |upd| try self.update(arena, w, upd),
            .upload_update => |uu| try self.upload_update(arena, w, uu),
            .ack_job => |jr| {
                if (try run_mgr.ack_job(arena, jr.job_id)) |rep| {
                    reply(w, .response, rep);
                } else fail(w, .not_found, "unknown job");
            },
            .acquire_lease => |al| {
                if (self.waiting_leases.fetchAdd(1, .acq_rel) >= waiting_leases_max) {
                    _ = self.waiting_leases.fetchSub(1, .acq_rel);
                    return fail(w, .busy, "too many lease acquisitions waiting");
                }
                defer _ = self.waiting_leases.fetchSub(1, .acq_rel);
                reply(w, .response, try adm.acquire_blocking(arena, al, al.job_id, w.abandoned));
            },
            .renew_lease => |rl| reply(w, .response, try adm.renew(arena, rl)),
            .release_lease => |rl| reply(w, .response, try adm.release(arena, rl.lease_id, rl.epoch)),
            .verify_epoch => |ve| reply(w, .response, try adm.verify_epoch(arena, ve)),
            .list_leases => reply(w, .response, try adm.status_snapshot(arena)),
            .clear_quarantine => reply(w, .response, try adm.clear_quarantine(arena)),
            .hello => w.no_reply = true,
        }
    }

    fn update(self: *Server, arena: std.mem.Allocator, w: *node_loop.Work, upd: msg.Update) !void {
        const gpa = self.cfg.gpa;
        const io = self.cfg.io;
        const rel_dir = try std.fs.path.join(arena, &.{ self.cfg.home_path, "releases" });
        const target = upd.version orelse version.version;
        var ok = true;
        var staged_new = false;
        var detail: []const u8 = "up to date";

        if (upd.url) |u| {
            const sha = upd.sha256 orelse return fail(w, .invalid, "update url requires sha256");
            u_store.stage_from_url(gpa, io, rel_dir, target, u, sha, .node) catch |err| {
                ok = false;
                detail = @errorName(err);
            };
            if (ok) {
                detail = "staged from url";
                staged_new = true;
            }
        } else if (upd.version != null and std.mem.eql(u8, target, version.version)) {
            detail = "already on requested release";
        } else if (upd.version != null and (u_store.activate_staged(gpa, io, rel_dir, target) catch false)) {
            detail = "activated staged release";
            staged_new = true;
        } else {
            const ch: u_ver.Channel = if (upd.channel != null and std.mem.eql(u8, upd.channel.?, "dev")) .dev else .stable;
            if (u_store.check_latest(arena, io, "hotschmoe/protectorate", ch, version.version, .node, upd.version)) |latest| {
                if (latest) |rel| {
                    u_store.stage_and_apply(gpa, io, rel_dir, rel, .node) catch |err| {
                        ok = false;
                        detail = @errorName(err);
                    };
                    if (ok) {
                        detail = "staged from github";
                        staged_new = true;
                    }
                } else {
                    detail = "already on latest release";
                }
            } else |err| {
                ok = false;
                detail = @errorName(err);
            }
        }

        reply(w, .response, msg.UpdateReply{
            .ok = ok,
            .from = version.version,
            .to = target,
            .detail = detail,
            .restarting = ok and staged_new,
        });
        w.then_stop = ok and staged_new;
    }

    fn upload_update(self: *Server, arena: std.mem.Allocator, w: *node_loop.Work, uu: msg.UploadUpdate) !void {
        const gpa = self.cfg.gpa;
        const io = self.cfg.io;
        if (!is_safe_name(uu.rollout_id) or !is_safe_name(uu.version)) return fail(w, .invalid, "invalid rollout_id or version");
        if ((uu.hex.len % 2) != 0) return fail(w, .invalid, "odd hex length");

        const rel_dir_path = try std.fs.path.join(arena, &.{ self.cfg.home_path, "releases" });
        var rel_dir = try std.Io.Dir.cwd().createDirPathOpen(io, rel_dir_path, .{});
        defer rel_dir.close(io);

        const part_name = try std.fmt.allocPrint(arena, "{s}.part", .{uu.rollout_id});
        const chunk = try arena.alloc(u8, uu.hex.len / 2);
        _ = std.fmt.hexToBytes(chunk, uu.hex) catch return fail(w, .invalid, "bad hex chunk");

        var pf = if (uu.offset == 0)
            try rel_dir.createFile(io, part_name, .{ .read = true, .truncate = true })
        else
            rel_dir.openFile(io, part_name, .{ .mode = .read_write }) catch
                try rel_dir.createFile(io, part_name, .{ .read = true, .truncate = false });
        errdefer pf.close(io);
        if (chunk.len > 0) {
            try pf.writePositionalAll(io, chunk, uu.offset);
        }
        const written = uu.offset + chunk.len;
        pf.close(io);

        var ready = false;
        if (uu.eof) {
            const full = try rel_dir.readFileAlloc(io, part_name, arena, .limited(128 << 20));
            if (!u_ver.verify_sha256(full, uu.sha256)) {
                rel_dir.deleteFile(io, part_name) catch {};
                return fail(w, .invalid, "upload sha256 mismatch");
            }
            const asset_name = u_store.host_asset_name(.node);
            try u_store.stage_archive_bytes(gpa, io, rel_dir_path, uu.version, asset_name, uu.sha256, full, .node);
            rel_dir.deleteFile(io, part_name) catch {};
            ready = true;
        }

        reply(w, .response, msg.UploadUpdateReply{
            .received_bytes = written,
            .ready = ready,
            .sha256 = if (ready) uu.sha256 else null,
        });
    }
};

fn reply(w: *node_loop.Work, kind: frame.Kind, value: anytype) void {
    const bytes = msg.encode_fixed(w.payload, value) catch return fail(w, .internal, "reply too large");
    w.kind = kind;
    w.payload_len = bytes.len;
}

fn fail(w: *node_loop.Work, code: msg.ErrorCode, text: []const u8) void {
    w.chunk_len = 0;
    const bytes = msg.encode_fixed(w.payload, msg.ErrorReply{ .code = code, .message = text }) catch unreachable;
    w.kind = .err;
    w.payload_len = bytes.len;
}

fn take_chunk(w: *node_loop.Work, data: []const u8) void {
    tiger.assert(data.len <= w.chunk.len);
    @memcpy(w.chunk[0..data.len], data);
    w.chunk_len = data.len;
}

fn is_safe_name(s: []const u8) bool {
    if (s.len == 0 or s.len > 128) return false;
    if (std.mem.eql(u8, s, ".") or std.mem.eql(u8, s, "..")) return false;
    for (s) |c| {
        if (c == '/' or c == '\\' or c == 0) return false;
    }
    return true;
}
