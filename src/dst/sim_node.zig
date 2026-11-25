const std = @import("std");
const tiger = @import("../tiger.zig");
const io = @import("../net/io.zig");
const noise = @import("../net/noise.zig");
const frame = @import("../net/frame.zig");
const msg = @import("../net/msg.zig");
const bounded = @import("../net/bounded.zig");
const Sizes = @import("sizes.zig").Sizes;
const fleet_bus = @import("../net/fleet_bus.zig");
const node_loop = @import("../net/node_loop.zig");
const Sim = @import("io_sim.zig").Sim;

const Bus = fleet_bus.FleetBus(Sim);
const Handle = fleet_bus.Handle;

const NoSync = struct {
    pub fn lock(_: *NoSync) void {}
    pub fn unlock(_: *NoSync) void {}
};
const Core = node_loop.NodeLoop(Sim, NoSync);

const fake_workers = 3;
const calls_max = bounded.rpc_slots_per_node;
const hold_steps = 1200;
const abandon_steps = 3000;

const Reply = struct { peer: Handle, id: u64 };
const Replies = bounded.Ring(Reply, 64);
const Kind = enum(u64) { quick, chunked, large, slow, abandon, stop };

fn kind_of(x: u64) Kind {
    return switch (x % 8) {
        0, 1 => .quick,
        2, 3 => .chunked,
        4 => .large,
        5 => .slow,
        6 => .abandon,
        else => .stop,
    };
}

fn chunk_len(x: u64) usize {
    return switch (kind_of(x)) {
        .chunked => 1 + @as(usize, @intCast((x * 977) % 6000)),
        .large => 150 * 1024 + @as(usize, @intCast(x % 1000)),
        else => 0,
    };
}

fn chunk_byte(x: u64, i: usize) u8 {
    return @truncate(x +% @as(u64, @intCast(i)) *% 31);
}

const Worker = struct {
    work: ?node_loop.Work = null,
    x: u64 = 0,
    left: usize = 0,
};

const Ctx = struct {
    core: *Core,
    endpoints: [2]io.Endpoint,
    resolved: usize = 0,
    resolves: usize = 0,
    connects: usize = 0,
    disconnects: usize = 0,
    healthy: usize = 0,
    cleanups: usize = 0,
    hello_ok: bool = true,
    hello_key: [32]u8 = undefined,
    allowed: [32]u8,
    replacement_identity: bool = false,

    fn hello_reply(_: *anyopaque, payload: []const u8) bool {
        var scratch: [4096]u8 = undefined;
        var fba = std.heap.FixedBufferAllocator.init(&scratch);
        const parsed = msg.decode_arena(msg.HelloReply, fba.allocator(), payload) catch return false;
        return parsed.ok and parsed.accepted_protocol == msg.protocol_version;
    }

    fn task_done(context: *anyopaque, task: node_loop.Task, id: u64) void {
        const self: *Ctx = @ptrCast(@alignCast(context));
        switch (task) {
            .resolve => {
                self.resolves += 1;
                self.core.link.set_endpoints(id, self.endpoints[0..2]);
            },
            .healthy => self.healthy += 1,
            .cleanup => self.cleanups += 1,
            .telemetry, .request => {},
        }
    }

    fn connected(context: *anyopaque) void {
        const self: *Ctx = @ptrCast(@alignCast(context));
        self.connects += 1;
    }

    fn disconnected(context: *anyopaque) void {
        const self: *Ctx = @ptrCast(@alignCast(context));
        self.disconnects += 1;
    }

    fn notify(_: *anyopaque) void {}

    fn on_hello(context: *anyopaque, _: u8, key: [32]u8, payload: []const u8, out: []u8) ?fleet_bus.HelloResult {
        const self: *Ctx = @ptrCast(@alignCast(context));
        var scratch: [8192]u8 = undefined;
        var fba = std.heap.FixedBufferAllocator.init(&scratch);
        const request = msg.decode_arena(msg.Request, fba.allocator(), payload) catch return null;
        if (request != .hello or request.hello.protocol != msg.protocol_version) return null;
        if (!std.mem.eql(u8, &key, &self.allowed)) return null;
        const encoded = msg.encode_fixed(out, msg.HelloReply{ .ok = true, .hub_version = "dst", .node_name = "node" }) catch return null;
        return .{ .identity = key, .reply_len = encoded.len };
    }
};

const Expect = struct {
    id: u64 = 0,
    x: u64 = 0,
    peer: Handle = undefined,
    chunks: usize = 0,
    chunk_bytes: usize = 0,
    done: bool = true,
};

pub fn run(gpa: std.mem.Allocator, seed: u64, sizes: Sizes) !void {
    var arena: bounded.StaticAllocator = .{ .parent = gpa };
    const allocator = arena.allocator();
    const sim = try allocator.create(Sim);
    sim.* = Sim.init(seed);
    const pool = try allocator.create(bounded.SlabPool);
    pool.* = try bounded.SlabPool.init_limited(allocator, sizes.slabs);
    const bus = try allocator.create(Bus);
    const core = try allocator.create(Core);
    const ctx = try allocator.create(Ctx);
    const sync = try allocator.create(NoSync);
    const chunk_reply = try allocator.alloc(u8, 256 * 1024);

    var key_seed: [32]u8 = undefined;
    sim.prng.random().bytes(&key_seed);
    const hub_key = noise.KeyPair.generate_deterministic(key_seed);
    sim.prng.random().bytes(&key_seed);
    const node_key = noise.KeyPair.generate_deterministic(key_seed);
    const endpoint: io.Endpoint = .{ .address = @splat(0), .port = 7475, .family = .ipv4 };
    const missing: io.Endpoint = .{ .address = @splat(0), .port = 7476, .family = .ipv4 };
    if (seed % 2 == 0) sim.route(missing).connect = .timeout;
    if (seed % 8 == 3) sim.transfer_max = 1;
    const listener = sim.listen(endpoint) orelse return error.NoSocket;
    ctx.* = .{ .core = core, .endpoints = .{ missing, endpoint }, .allowed = node_key.public };
    try bus.init(sim, pool, listener, hub_key, &.{node_key.public}, .{ .context = ctx, .hello = Ctx.on_hello });
    var hello_buffer: [1024]u8 = undefined;
    const hello = try msg.encode_fixed(&hello_buffer, msg.Request{ .hello = .{ .node_id = "node", .node_version = "dst" } });
    try core.init(allocator, sim, pool, sync, &.{}, node_key, hub_key.public, hello, .{
        .context = ctx,
        .hello_reply = Ctx.hello_reply,
        .task_done = Ctx.task_done,
        .connected = Ctx.connected,
        .disconnected = Ctx.disconnected,
        .notify = Ctx.notify,
        .resolve = true,
    });
    core.slot_limit = sizes.request_slots;
    bus.slot_limit = sizes.bus_slots;
    arena.freeze();

    var workers: [fake_workers]Worker = @splat(.{});
    var calls: [calls_max]Expect = @splat(.{});
    var peer: ?Handle = null;
    var replies: Replies = .{};
    var next_x: u64 = 8 * (seed % 10_000);
    var busy_seen: usize = 0;
    var gone_seen: usize = 0;
    var delivered: usize = 0;
    const Phase = enum { flood, drain_flood, crash, outage, reconnect, check_stale, stop, shutdown };
    var plan: [calls_max]u64 = undefined;
    var plan_len: usize = 0;
    var plan_pos: usize = 0;
    var phase: Phase = .flood;
    var phase_step: usize = 0;
    var stop_reply = false;
    var stop_sent = false;
    var stop_finished = false;
    var stale_requests: usize = 0;
    var dropped_before: u64 = 0;
    var step: usize = 0;
    errdefer std.debug.print("node sim: seed={d} step={d} phase={s} connects={d} disconnects={d} busy={d} gone={d} delivered={d} free={d}\n", .{ seed, step, @tagName(phase), ctx.connects, ctx.disconnects, busy_seen, gone_seen, delivered, core.free_slots() });
    while (step < 60_000) : (step += 1) {
        phase_step += 1;
        if (sim.now() >= 500) sim.transfer_max = 4096;
        try bus.drive();
        core.turn();
        try step_workers(core, &workers, sim, &stop_finished);
        if (bus.event()) |event| switch (event) {
            .connected => |connected| {
                peer = connected;
                bus.consume(event);
            },
            .disconnected => |gone| {
                if (peer != null and std.meta.eql(peer.?, gone)) peer = null;
                bus.consume(event);
            },
            .request => |request| {
                try replies.push(.{ .peer = request.peer, .id = request.id });
                bus.consume(event);
            },
            .chunk => |chunk| {
                const call = find(&calls, chunk.peer, chunk.id) orelse return error.StaleChunk;
                const offset = call.chunk_bytes;
                if (offset + chunk.size > chunk_len(call.x)) return error.CorruptChunk;
                try bus.read(chunk.peer, chunk.serial, 0, chunk_reply[0..chunk.size]);
                for (chunk_reply[0..chunk.size], 0..) |byte, i| if (byte != chunk_byte(call.x, offset + i)) return error.CorruptChunk;
                call.chunk_bytes += chunk.size;
                call.chunks += 1;
                bus.consume(event);
            },
            .terminal => |terminal| {
                const call = find(&calls, terminal.peer, terminal.id) orelse return error.StaleTerminal;
                if (call.done) return error.DuplicateTerminal;
                call.done = true;
                switch (terminal.result) {
                    .response, .end => {
                        const want = chunk_len(call.x);
                        if (call.chunk_bytes != want) return error.ShortReply;
                        var body: [128]u8 = undefined;
                        if (terminal.size > body.len) return error.CorruptReply;
                        try bus.read(terminal.peer, terminal.serial, 0, body[0..terminal.size]);
                        var scratch: [512]u8 = undefined;
                        var fba = std.heap.FixedBufferAllocator.init(&scratch);
                        const parsed = std.json.parseFromSliceLeaky(msg.Ping, fba.allocator(), body[0..terminal.size], .{ .ignore_unknown_fields = true }) catch return error.CorruptReply;
                        if (@as(u64, @intCast(parsed.now_ms)) != call.x) return error.WrongReply;
                        if (want > 0 and terminal.result != .end) return error.WrongKind;
                        delivered += 1;
                        if (kind_of(call.x) == .stop) stop_reply = true;
                    },
                    .remote_error => {
                        if (call.chunk_bytes != 0) return error.ChunkedError;
                        var body: [192]u8 = undefined;
                        if (terminal.size > body.len) return error.CorruptReply;
                        try bus.read(terminal.peer, terminal.serial, 0, body[0..terminal.size]);
                        if (std.mem.indexOf(u8, body[0..terminal.size], "busy") == null) return error.UnexpectedRemoteError;
                        busy_seen += 1;
                    },
                    .peer_gone, .peer_replaced => gone_seen += 1,
                    .timeout => return error.Timeout,
                }
                bus.consume(event);
            },
        };
        try flush_replies(bus, &replies);
        core.assert_invariants();
        bus.assert_invariants();
        pool.assert_conservation();
        for (&calls) |*call| {
            if (!call.done and call.peer.generation != 0 and peer != null and !std.meta.eql(call.peer, peer.?) and phase == .flood) return error.CallOnStalePeer;
        }

        if (plan_pos < plan_len and peer != null) {
            if (try issue(bus, sim, peer.?, &calls[plan_pos], plan[plan_pos])) plan_pos += 1;
        }
        const settled = plan_pos == plan_len and all_done(&calls);
        switch (phase) {
            .flood => if (peer != null and ctx.connects >= 1) {
                // sixteen slow calls against fourteen request slots: the last two must be refused as busy
                for (&plan) |*x| {
                    next_x = slow_x(next_x);
                    x.* = next_x;
                }
                plan_len = calls_max;
                plan_pos = 0;
                phase = .drain_flood;
                phase_step = 0;
            },
            .drain_flood => if (settled and plan_len > 0) {
                if (core.busy_replies == 0 or busy_seen == 0) return error.NoBackpressure;
                for (&plan, 0..) |*x, i| {
                    next_x = if (i % 2 == 0) abandon_x(next_x) else slow_x(next_x);
                    x.* = next_x;
                }
                plan_len = calls_max;
                plan_pos = 0;
                phase = .crash;
                phase_step = 0;
            },
            .crash => if (plan_pos < plan_len) {
                phase_step = 0;
            } else if (phase_step > 30 + seed % 40) {
                stale_requests = core.count_requests();
                dropped_before = core.dropped_replies;
                sim.crash(endpoint);
                phase = .outage;
                phase_step = 0;
            },
            .outage => {
                if (phase_step > 120 + seed % 100) {
                    sim.restart(endpoint);
                    phase = .reconnect;
                    phase_step = 0;
                }
            },
            .reconnect => if (peer != null and ctx.connects >= 2 and settled and core.free_slots() == sizes.request_slots) {
                if (ctx.disconnects == 0) return error.NoDisconnect;
                if (stale_requests == 0 or gone_seen == 0) return error.NothingInFlight;
                if (core.dropped_replies < dropped_before + stale_requests) return error.StaleReplyNotDropped;
                if (ctx.cleanups == 0) return error.OrphanNotCleaned;
                for (&plan, 0..) |*x, i| {
                    next_x = if (i % 3 == 0) quick_x(next_x) else if (i % 3 == 1) chunked_x(next_x) else large_x(next_x);
                    x.* = next_x;
                }
                plan_len = calls_max;
                plan_pos = 0;
                phase = .check_stale;
                phase_step = 0;
            },
            .check_stale => if (settled) {
                next_x = stop_x(next_x);
                plan[0] = next_x;
                plan_len = 1;
                plan_pos = 0;
                phase = .stop;
                phase_step = 0;
            },
            .stop => {
                if (core.stop_reply_sent and (!stop_finished or core.outbox.len != 0 or !core.link.flushed())) return error.StopBeforeFlush;
                if (stop_reply) stop_sent = true;
                if (core.stop_reply_sent and stop_sent) {
                    core.shutdown();
                    phase = .shutdown;
                    phase_step = 0;
                }
            },
            .shutdown => {
                while (replies.pop()) |_| {}
                if (core.done() and !bus.done()) bus.shutdown();
                if (core.done() and bus.done() and workers_idle(&workers)) {
                    core.drain();
                    break;
                }
            },
        }
        sim.advance();
    } else return error.Stalled;

    while (bus.event()) |event| bus.consume(event);
    var drain_steps: usize = 0;
    while (!bus.done() and drain_steps < 5000) : (drain_steps += 1) {
        try bus.drive();
        core.turn();
        while (bus.event()) |event| bus.consume(event);
        pool.assert_conservation();
        sim.advance();
    }
    if (!bus.done() or !core.quiescent()) return error.ShutdownStalled;
    if (ctx.resolves < ctx.connects) return error.NotResolvedBeforeRace;
    if (ctx.healthy != 0) return error.UnexpectedTask;
    sim.assert_drained();
    pool.assert_conservation();
    tiger.assert(pool.free_count == pool.limit);
    arena.begin_deinit();
    core.deinit(allocator);
    bus.deinit();
    allocator.destroy(sync);
    allocator.destroy(ctx);
    allocator.destroy(core);
    allocator.destroy(bus);
    allocator.free(chunk_reply);
    pool.deinit(allocator);
    allocator.destroy(pool);
    allocator.destroy(sim);
    arena.finish_deinit();
}

fn slow_x(from: u64) u64 {
    return advance_to(from, 5);
}

fn abandon_x(from: u64) u64 {
    return advance_to(from, 6);
}

fn quick_x(from: u64) u64 {
    return advance_to(from, 0);
}

fn chunked_x(from: u64) u64 {
    return advance_to(from, 2);
}

fn large_x(from: u64) u64 {
    return advance_to(from, 4);
}

fn stop_x(from: u64) u64 {
    return advance_to(from, 7);
}

fn advance_to(from: u64, residue: u64) u64 {
    var x = from + 1;
    while (x % 8 != residue) x += 1;
    return x;
}

fn find(calls: *[calls_max]Expect, peer: Handle, id: u64) ?*Expect {
    for (calls) |*call| {
        if (!call.done and call.id == id and std.meta.eql(call.peer, peer)) return call;
    }
    return null;
}

fn all_done(calls: *[calls_max]Expect) bool {
    for (calls) |call| if (!call.done) return false;
    return true;
}

fn workers_idle(workers: *[fake_workers]Worker) bool {
    for (workers) |worker| if (worker.work != null) return false;
    return true;
}

fn issue(bus: *Bus, sim: *Sim, peer: Handle, call: *Expect, x: u64) !bool {
    tiger.assert(call.done);
    var body: [128]u8 = undefined;
    const request = try msg.encode_fixed(&body, msg.Request{ .ping = .{ .now_ms = @intCast(x) } });
    const id = bus.call(peer, request, sim.now() + 60_000) catch |err| switch (err) {
        error.Busy, error.Full => return false,
        else => return err,
    };
    call.* = .{ .id = id, .x = x, .peer = peer, .done = false };
    return true;
}

fn flush_replies(bus: *Bus, replies: *Replies) !void {
    const ready = replies.len;
    for (0..ready) |_| {
        const pending = replies.pop().?;
        bus.send(pending.peer, .response, pending.id, "{\"ok\":true,\"accepted_protocol\":3,\"hub_version\":\"dst\",\"node_name\":\"node\"}") catch |err| switch (err) {
            error.Busy, error.Full => try replies.push(pending),
            error.PeerGone => {},
            else => return err,
        };
    }
}

fn step_workers(core: *Core, workers: *[fake_workers]Worker, sim: *Sim, stop_finished: *bool) !void {
    for (workers) |*worker| {
        if (worker.work == null) {
            const work = core.next_work() orelse continue;
            worker.x = 0;
            worker.left = 1 + sim.random() % 4;
            if (work.task == .request) {
                var scratch: [1024]u8 = undefined;
                var fba = std.heap.FixedBufferAllocator.init(&scratch);
                const request = try msg.decode_arena(msg.Request, fba.allocator(), work.request);
                if (request != .ping) return error.BadRequest;
                worker.x = @intCast(request.ping.now_ms);
                worker.left = switch (kind_of(worker.x)) {
                    .slow => hold_steps,
                    .abandon => abandon_steps,
                    else => worker.left,
                };
            }
            worker.work = work;
            continue;
        }
        const work = &worker.work.?;
        if (worker.left > 0) worker.left -= 1;
        const abandoned = work.abandoned.load(.acquire);
        const finish = worker.left == 0 or (work.task == .request and kind_of(worker.x) == .abandon and abandoned);
        if (!finish) continue;
        if (work.task == .request) {
            const len = chunk_len(worker.x);
            for (work.chunk[0..len], 0..) |*byte, i| byte.* = chunk_byte(worker.x, i);
            work.chunk_len = len;
            const body = try msg.encode_fixed(work.payload, msg.Ping{ .now_ms = @intCast(worker.x) });
            work.payload_len = body.len;
            work.kind = if (len > 0) .end else .response;
            work.then_stop = kind_of(worker.x) == .stop;
            work.cleanup = kind_of(worker.x) == .abandon or kind_of(worker.x) == .quick;
            if (work.then_stop) stop_finished.* = true;
        }
        core.finish_work(work);
        worker.work = null;
    }
}
