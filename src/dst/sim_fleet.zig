const std = @import("std");
const tiger = @import("../tiger.zig");
const io = @import("../net/io.zig");
const noise = @import("../net/noise.zig");
const frame = @import("../net/frame.zig");
const msg = @import("../net/msg.zig");
const bounded = @import("../net/bounded.zig");
const Sim = @import("io_sim.zig").Sim;
const Bus = @import("../net/fleet_bus.zig").FleetBus(Sim);
const Link = @import("../net/node_link.zig").NodeLink(Sim);

const reply = "{\"ok\":true,\"accepted_protocol\":3,\"hub_version\":\"dst\",\"node_name\":\"node\"}";
const ping = "{\"type\":\"ping\",\"now_ms\":1}";
const submit = "{\"type\":\"submit_job\",\"job_id\":\"job-dst-1\",\"argv\":[\"echo\",\"hello-fleet\"],\"artifacts\":[\"out/bin\"],\"even_if_present\":true,\"on_present\":\"idle\"}";
const pong = "{\"now_ms\":1}";
const error_reply = "{\"code\":\"busy\",\"message\":\"simulation\"}";
const HubReplies = bounded.Ring(struct { peer: @import("../net/fleet_bus.zig").Handle, id: u64 }, 128);

fn flush_hub_replies(bus: *Bus, replies: *HubReplies) !void {
    const ready = replies.len;
    for (0..ready) |_| {
        const pending = replies.pop().?;
        bus.send(pending.peer, .response, pending.id, reply) catch |err| switch (err) {
            error.Busy, error.Full => try replies.push(pending),
            error.PeerGone => {},
            else => return err,
        };
    }
    try bus.drive();
}

fn chunk_count(seed: u64, index: usize, long_stream: bool, all_large: bool) usize {
    if (long_stream and (index == 0 or all_large)) return 3;
    return 3 + index + @as(usize, @intCast((seed >> @intCast(index * 4)) & 3));
}

fn chunk_length(seed: u64, index: usize, long_stream: bool, all_large: bool) usize {
    if (long_stream and (index == 0 or all_large)) return frame.max_payload;
    return 1024 + @as(usize, @intCast((seed >> @intCast(index * 7)) % 3073));
}

const Trace = struct {
    const Entry = struct { tick: u64, event: []const u8, source: usize };
    entries: [32]Entry = undefined,
    count: usize = 0,

    fn add(self: *Trace, tick: u64, event: []const u8, source: usize) void {
        self.entries[self.count % self.entries.len] = .{ .tick = tick, .event = event, .source = source };
        self.count += 1;
    }

    fn print(self: *const Trace, seed: u64) void {
        std.debug.print("fleet trace seed={d}:\n", .{seed});
        const start = self.count - @min(self.count, self.entries.len);
        for (start..self.count) |i| {
            const entry = self.entries[i % self.entries.len];
            std.debug.print("  t={d} {s} {d}\n", .{ entry.tick, entry.event, entry.source });
        }
    }
};

var active_trace: ?*const Trace = null;
var active_trace_seed: u64 = 0;

pub fn print_panic_trace() void {
    if (active_trace) |trace| trace.print(active_trace_seed);
}

const Timer = struct {
    count: usize = 0,
    result: ?io.Result = null,

    fn complete(context: *anyopaque, _: *io.Operation, result: io.Result) void {
        const self: *Timer = @ptrCast(@alignCast(context));
        self.count += 1;
        self.result = result;
    }
};

const Context = struct {
    original: [32]u8,
    replacement: [32]u8,
    allowed: []const [32]u8,

    fn on_hello(context: *anyopaque, key: [32]u8, payload: []const u8, out: []u8) ?@import("../net/fleet_bus.zig").HelloResult {
        const self: *Context = @ptrCast(@alignCast(context));
        var scratch: [65536]u8 = undefined;
        var fba = std.heap.FixedBufferAllocator.init(&scratch);
        const request = msg.decode_arena(msg.Request, fba.allocator(), payload) catch return null;
        if (request != .hello or request.hello.protocol != msg.protocol_version) return null;
        var index: ?usize = null;
        for (self.allowed, 0..) |allowed_key, i| if (std.mem.eql(u8, &allowed_key, &key)) {
            index = i;
            break;
        };
        if (std.mem.eql(u8, &key, &self.replacement)) index = 0;
        var name_buffer: [32]u8 = undefined;
        const name = std.fmt.bufPrint(&name_buffer, "node-{d}", .{index orelse return null}) catch return null;
        if (!std.mem.eql(u8, request.hello.node_id, name)) return null;
        const encoded = msg.encode_fixed(out, msg.HelloReply{ .ok = true, .hub_version = "dst", .node_name = name }) catch return null;
        return .{ .identity = if (std.mem.eql(u8, &key, &self.replacement)) self.original else key, .reply_len = encoded.len };
    }
    fn on_reply(_: *anyopaque, payload: []const u8) bool {
        var scratch: [4096]u8 = undefined;
        var fba = std.heap.FixedBufferAllocator.init(&scratch);
        const parsed = msg.decode_arena(msg.HelloReply, fba.allocator(), payload) catch return false;
        return parsed.ok and parsed.accepted_protocol == msg.protocol_version;
    }
};

pub fn run(gpa: std.mem.Allocator, seed: u64, fleet_long: bool) !void {
    var trace: Trace = .{};
    active_trace = &trace;
    active_trace_seed = seed;
    defer active_trace = null;
    run_inner(gpa, seed, fleet_long, &trace) catch |err| {
        trace.print(seed);
        return err;
    };
}

fn run_inner(gpa: std.mem.Allocator, seed: u64, fleet_long: bool, trace: *Trace) !void {
    const node_count: usize = 2 + @as(usize, @intCast(seed % 5));
    var arena: bounded.StaticAllocator = .{ .parent = gpa };
    const allocator = arena.allocator();
    const long_stream = fleet_long or seed % 64 == 0;
    const chunk = try allocator.alloc(u8, if (long_stream) frame.max_payload else 4096);
    const actual = try allocator.alloc(u8, chunk.len);
    var pool = try bounded.SlabPool.init(allocator);
    const sim = try allocator.create(Sim);
    sim.* = Sim.init(seed);
    const bus = try allocator.create(Bus);
    const duplicate = try allocator.create(Link);
    const rogue = try allocator.create(Link);
    const corrupt = try allocator.create(Link);
    var links: [6]*Link = undefined;
    var keys: [6]noise.KeyPair = undefined;
    var allowed: [7][32]u8 = undefined;
    var key_seed: [32]u8 = undefined;
    sim.prng.random().bytes(&key_seed);
    const hub_key = noise.KeyPair.generate_deterministic(key_seed);
    sim.prng.random().bytes(&key_seed);
    const rogue_key = noise.KeyPair.generate_deterministic(key_seed);
    for (0..node_count) |i| {
        sim.prng.random().bytes(&key_seed);
        keys[i] = noise.KeyPair.generate_deterministic(key_seed);
        allowed[i] = keys[i].public;
        links[i] = try allocator.create(Link);
    }
    sim.prng.random().bytes(&key_seed);
    const replacement_key = noise.KeyPair.generate_deterministic(key_seed);
    allowed[node_count] = replacement_key.public;
    const endpoint: io.Endpoint = .{ .address = @splat(0), .port = 7475, .family = .ipv4 };
    const missing: io.Endpoint = .{ .address = @splat(0), .port = 7476, .family = .ipv4 };
    const endpoints = [_]io.Endpoint{ missing, endpoint, endpoint };
    const listener = sim.listen(endpoint) orelse return error.NoSocket;
    if (seed % 2 == 0) sim.route(missing).connect = .timeout;
    if (seed % 4 == 1 and !fleet_long and seed % 64 != 0) sim.route(endpoint).slow = true;
    if (seed % 8 == 0) sim.transfer_max = 1;
    var context: Context = .{ .original = keys[0].public, .replacement = replacement_key.public, .allowed = allowed[0..node_count] };
    try bus.init(sim, &pool, listener, hub_key, allowed[0..node_count], .{ .context = &context, .hello = Context.on_hello });
    var hello_buffers: [6][4096]u8 = undefined;
    var hello_payloads: [6][]const u8 = undefined;
    for (0..node_count) |i| {
        var name_buffer: [32]u8 = undefined;
        const name = try std.fmt.bufPrint(&name_buffer, "node-{d}", .{i});
        hello_payloads[i] = try msg.encode_fixed(&hello_buffers[i], msg.Request{ .hello = .{ .node_id = name, .node_version = "dst" } });
        try links[i].init(sim, &pool, &endpoints, keys[i], hub_key.public, hello_payloads[i], .{ .context = &context, .hello_reply = Context.on_reply });
    }
    var timer: Timer = .{};
    var cancelled_timer: Timer = .{};
    var timer_op: io.Operation = .{ .context = &timer, .callback = Timer.complete };
    var cancelled_op: io.Operation = .{ .context = &cancelled_timer, .callback = Timer.complete };
    try sim.submit(&timer_op, .{ .timeout = sim.now() + 5 });
    try sim.submit(&cancelled_op, .{ .timeout = sim.now() + 100 });
    sim.cancel(&cancelled_op);
    arena.freeze();
    const Handle = @import("../net/fleet_bus.zig").Handle;
    var peers: [6]?Handle = @splat(null);
    var requests: [6]u64 = @splat(0);
    var completed: [6]bool = @splat(false);
    var pending_reply: [6]u64 = @splat(0);
    var hub_replies: HubReplies = .{};
    var chunks_left: [6]usize = @splat(0);
    var chunks_received: [6]usize = @splat(0);
    var generation: [6]u64 = @splat(0);
    var phase: enum { initial, partition, recovered } = .initial;
    var restart_at: ?u64 = null;
    var disconnected: [6]bool = @splat(false);
    var dial_raced = false;
    var race_drained = false;
    sim.prng.random().bytes(chunk);
    var step: usize = 0;
    while (step < 30_000) : (step += 1) {
        if (sim.now() >= 500) sim.transfer_max = 4096;
        if (restart_at) |deadline| if (sim.now() >= deadline) {
            sim.restart(endpoint);
            restart_at = null;
        };
        try bus.drive();
        for (links[0..node_count]) |link| try link.drive();
        if (seed % 8 == 0 and phase == .initial) {
            const first = links[0].candidates[1].state;
            const second = links[0].candidates[2].state;
            if (!dial_raced and first == .handshake and (second == .connecting or second == .handshake)) {
                dial_raced = true;
                trace.add(sim.now(), "dial_race", 1);
            }
            if (dial_raced and links[0].phase == .established) {
                if (links[0].winner) |winner| {
                    if (winner == 1 or winner == 2) {
                        const loser: usize = if (winner == 1) 2 else 1;
                        if (links[0].candidates[loser].state == .finished) race_drained = true;
                    }
                }
            }
        }
        if (phase != .partition) for (0..node_count) |i| {
            if (peers[i] == null) peers[i] = bus.current(keys[i].public);
        };
        if (bus.event_for(step % bounded.fleet_slots)) |event| {
            trace.add(sim.now(), @tagName(std.meta.activeTag(event)), 0);
            switch (event) {
                .connected => |peer| {
                    const identity = bus.identity(peer) orelse return error.MissingIdentity;
                    if (bus.current(identity) == null) return error.MissingIdentity;
                    bus.consume(event);
                },
                .request => |request| {
                    try hub_replies.push(.{ .peer = request.peer, .id = request.id });
                    bus.consume(event);
                },
                .chunk => |event_chunk| {
                    var found = false;
                    for (0..node_count) |i| if (peers[i] != null and std.meta.eql(peers[i].?, event_chunk.peer) and requests[i] == event_chunk.id) {
                        const chunk_size = chunk_length(seed, i, long_stream, fleet_long);
                        if (event_chunk.size != chunk_size) return error.CorruptChunk;
                        try bus.read(event_chunk.peer, event_chunk.serial, 0, actual[0..chunk_size]);
                        if (!std.mem.eql(u8, actual[0..chunk_size], chunk[0..chunk_size])) return error.CorruptChunk;
                        chunks_received[i] += 1;
                        found = true;
                        break;
                    };
                    if (!found) return error.StaleChunk;
                    bus.consume(event);
                },
                .terminal => |terminal| {
                    var found = false;
                    for (0..node_count) |i| if (peers[i] != null and std.meta.eql(peers[i].?, terminal.peer) and requests[i] == terminal.id) {
                        const is_error = phase == .initial and i + 1 == node_count and seed % 3 == 0;
                        if (completed[i]) return error.BadTerminal;
                        if (is_error) {
                            if (terminal.result != .remote_error or terminal.size != error_reply.len or chunks_received[i] != 0) return error.BadTerminal;
                            var error_payload: [error_reply.len]u8 = undefined;
                            try bus.read(terminal.peer, terminal.serial, 0, &error_payload);
                            if (!std.mem.eql(u8, &error_payload, error_reply)) return error.BadTerminal;
                        } else if (terminal.result != .end or chunks_received[i] != chunk_count(seed, i, long_stream, fleet_long)) return error.BadTerminal;
                        completed[i] = true;
                        found = true;
                        break;
                    };
                    if (!found) return error.StaleTerminal;
                    bus.consume(event);
                },
                .disconnected => |peer| {
                    if (phase != .partition) return error.UnexpectedDisconnect;
                    var found = false;
                    for (0..node_count) |i| if (peers[i] != null and std.meta.eql(peers[i].?, peer)) {
                        if (disconnected[i]) return error.DuplicateDisconnect;
                        disconnected[i] = true;
                        peers[i] = null;
                        found = true;
                        break;
                    };
                    if (!found) return error.StaleDisconnect;
                    bus.consume(event);
                },
            }
        }
        try flush_hub_replies(bus, &hub_replies);
        for (0..node_count) |i| {
            const link = links[i];
            if (link.event()) |event| {
                trace.add(sim.now(), @tagName(std.meta.activeTag(event)), i + 1);
                switch (event) {
                    .connected => |g| {
                        generation[i] = g;
                        link.consume(event);
                    },
                    .request => |request| {
                        if (pending_reply[i] == 0) {
                            if (request.size > 4096) return error.BadRequest;
                            var payload: [4096]u8 = undefined;
                            try link.read(request.generation, request.serial, 0, payload[0..request.size]);
                            var parse_scratch: [16384]u8 = undefined;
                            var fba = std.heap.FixedBufferAllocator.init(&parse_scratch);
                            const parsed = try msg.decode_arena(msg.Request, fba.allocator(), payload[0..request.size]);
                            if ((i + @as(usize, @intCast(seed % 2))) % 2 == 0) {
                                if (parsed != .submit_job or !std.mem.eql(u8, parsed.submit_job.job_id, "job-dst-1")) return error.BadRequest;
                            } else if (parsed != .ping or parsed.ping.now_ms != 1) return error.BadRequest;
                            pending_reply[i] = request.id;
                            chunks_left[i] = if (phase == .initial and i + 1 == node_count and seed % 3 == 0) 0 else chunk_count(seed, i, long_stream, fleet_long);
                            link.consume(event);
                        }
                    },
                    .disconnected => {
                        if (phase == .initial) return error.UnexpectedDisconnect;
                        link.consume(event);
                    },
                }
            }
            if (pending_reply[i] != 0) {
                const chunk_size = chunk_length(seed, i, long_stream, fleet_long);
                const is_error = phase == .initial and i + 1 == node_count and seed % 3 == 0;
                const kind: frame.Kind = if (is_error) .err else if (chunks_left[i] > 0) .chunk else .end;
                const payload: []const u8 = if (is_error) error_reply else if (chunks_left[i] > 0) chunk[0..chunk_size] else pong;
                const sent = if (link.send(generation[i], kind, pending_reply[i], payload)) |_| true else |err| switch (err) {
                    error.Busy, error.Full => false,
                    else => return err,
                };
                if (sent) {
                    if (chunks_left[i] > 0) chunks_left[i] -= 1 else pending_reply[i] = 0;
                }
            }
            if (phase != .partition) if (peers[i]) |peer| if (requests[i] == 0) {
                if (bus.call(peer, if ((i + @as(usize, @intCast(seed % 2))) % 2 == 0) submit else ping, sim.now() + 20_000)) |id| {
                    if (id % 2 != 1) return error.BadRequestId;
                    requests[i] = id;
                } else |err| switch (err) {
                    error.Busy, error.Full => {},
                    else => return err,
                }
            };
        }
        bus.assert_invariants();
        pool.assert_conservation();
        sim.advance();
        if (phase == .partition) {
            var all_disconnected = true;
            for (disconnected[0..node_count]) |value| if (!value) {
                all_disconnected = false;
                break;
            };
            if (all_disconnected) {
                requests = @splat(0);
                completed = @splat(false);
                chunks_received = @splat(0);
                pending_reply = @splat(0);
                chunks_left = @splat(0);
                phase = .recovered;
            }
            continue;
        }
        var all_done = true;
        for (completed[0..node_count]) |done| if (!done) {
            all_done = false;
            break;
        };
        if (all_done) {
            if (phase == .initial) {
                trace.add(sim.now(), "fault", 0);
                if (seed % 2 == 0) {
                    sim.partition(endpoint, 400, 10);
                } else {
                    sim.crash(endpoint);
                    restart_at = sim.now() + 400;
                }
                phase = .partition;
            } else break;
        }
    }
    if (step == 30_000) return error.FleetStalled;
    if (seed % 8 == 0 and (!dial_raced or !race_drained)) return error.DialRaceNotExercised;
    if (timer.count != 1 or timer.result == null or timer.result.? != .timed_out) return error.BadTimerCompletion;
    if (cancelled_timer.count != 1 or cancelled_timer.result == null) return error.BadCancellation;
    switch (cancelled_timer.result.?) {
        .failed => |err| if (err != error.Cancelled) return error.BadCancellation,
        else => return error.BadCancellation,
    }
    const old_peer = peers[0].?;
    try bus.set_allowed(allowed[0 .. node_count + 1]);
    var replaced_calls: [bounded.rpc_slots_per_node]u64 = @splat(0);
    var replaced_done: [bounded.rpc_slots_per_node]bool = @splat(false);
    var calls_started: usize = 0;
    var calls_done: usize = 0;
    var capacity_check_at: u64 = 0;
    var capacity_checked = false;
    var replacement_call: u64 = 0;
    var replacement_peer: ?Handle = null;
    var replacement_connected = false;
    var replacement_started = false;
    var replacement_terminal = false;
    var replacement_complete = false;
    var old_stopped = false;
    step = 0;
    while (step < 20_000 and !replacement_complete) : (step += 1) {
        try bus.drive();
        for (links[0..node_count]) |link| try link.drive();
        if (replacement_started) try duplicate.drive();
        if (bus.event()) |event| {
            trace.add(sim.now(), @tagName(std.meta.activeTag(event)), 0);
            switch (event) {
                .connected => |peer| {
                    if (!std.meta.eql(peer, old_peer) and bus.current(keys[0].public) != null and std.meta.eql(peer, bus.current(keys[0].public).?)) replacement_peer = peer;
                    bus.consume(event);
                },
                .disconnected => |peer| {
                    if (std.meta.eql(peer, old_peer)) return error.ReplacedDisconnect;
                    return error.UnexpectedDisconnect;
                },
                .terminal => |terminal| {
                    if (std.meta.eql(terminal.peer, old_peer)) {
                        var found = false;
                        for (replaced_calls, 0..) |id, i| if (id != 0 and id == terminal.id) {
                            const expected: @import("../net/fleet_bus.zig").Terminal = if (i == 0) .timeout else .peer_replaced;
                            if (replaced_done[i] or terminal.result != expected) return error.BadReplacement;
                            replaced_done[i] = true;
                            calls_done += 1;
                            replacement_terminal = calls_done == replaced_calls.len;
                            found = true;
                            break;
                        };
                        if (!found) return error.StaleTerminal;
                    } else if (replacement_peer != null and std.meta.eql(terminal.peer, replacement_peer.?) and terminal.id == replacement_call) {
                        if (terminal.result != .response) return error.BadReplacementReply;
                        replacement_complete = true;
                    } else return error.StaleTerminal;
                    bus.consume(event);
                },
                .request => |request| {
                    try hub_replies.push(.{ .peer = request.peer, .id = request.id });
                    bus.consume(event);
                },
                .chunk => return error.UnexpectedChunk,
            }
        }
        try flush_hub_replies(bus, &hub_replies);
        for (links[0..node_count], 0..) |link, i| if (link.event()) |event| switch (event) {
            .connected, .disconnected => link.consume(event),
            .request => if (i != 0) return error.UnexpectedRequest,
        };
        if (replacement_started) if (duplicate.event()) |event| switch (event) {
            .connected => {
                replacement_connected = true;
                duplicate.consume(event);
            },
            .disconnected => return error.UnexpectedDisconnect,
            .request => |request| {
                const sent = if (duplicate.send(request.generation, .response, request.id, pong)) |_| true else |err| switch (err) {
                    error.Busy, error.Full => false,
                    else => return err,
                };
                if (sent) duplicate.consume(event);
            },
        };
        if (calls_started < replaced_calls.len) {
            const deadline = sim.now() + if (calls_started == 0) @as(u64, 3_000) else 20_000;
            if (bus.call(old_peer, ping, deadline)) |id| {
                replaced_calls[calls_started] = id;
                calls_started += 1;
                if (calls_started == replaced_calls.len) capacity_check_at = sim.now() + 100;
            } else |err| switch (err) {
                error.Busy, error.Full => {},
                else => return err,
            }
        }
        if (calls_started == replaced_calls.len and !capacity_checked and sim.now() >= capacity_check_at) {
            if (bus.call(old_peer, ping, sim.now() + 10_000)) |_| return error.RpcCapacityExceeded else |err| {
                if (err != error.Busy) return err;
            }
            capacity_checked = true;
        }
        if (capacity_checked and replaced_done[0] and !replacement_started) {
            try duplicate.init(sim, &pool, &endpoints, replacement_key, hub_key.public, hello_payloads[0], .{ .context = &context, .hello_reply = Context.on_reply });
            replacement_started = true;
        }
        if (replacement_started and replacement_peer == null) {
            if (bus.current(keys[0].public)) |peer| {
                if (!std.meta.eql(peer, old_peer)) replacement_peer = peer;
            }
        }
        if (replacement_peer != null and !old_stopped) {
            links[0].shutdown();
            old_stopped = true;
        }
        if (replacement_connected and replacement_terminal and replacement_peer != null and replacement_call == 0) {
            if (bus.call(replacement_peer.?, ping, sim.now() + 10_000)) |id| {
                replacement_call = id;
            } else |err| switch (err) {
                error.Busy, error.Full => {},
                else => return err,
            }
        }
        bus.assert_invariants();
        pool.assert_conservation();
        sim.advance();
    }
    if (!replacement_complete or !replacement_terminal) return error.ReplacementStalled;
    const tampered_peer = replacement_peer.?;
    var tampered_call: u64 = 0;
    var tampered_terminal = false;
    var tampered_disconnect = false;
    var reconnected = false;
    var reconnected_peer: ?Handle = null;
    var liveness_call: u64 = 0;
    var liveness_done = false;
    step = 0;
    while (step < 20_000 and !liveness_done) : (step += 1) {
        try bus.drive();
        for (links[0..node_count]) |link| try link.drive();
        try duplicate.drive();
        if (bus.event()) |event| {
            trace.add(sim.now(), @tagName(std.meta.activeTag(event)), 0);
            switch (event) {
                .connected => |peer| {
                    if (!std.meta.eql(peer, tampered_peer) and bus.current(keys[0].public) != null and std.meta.eql(peer, bus.current(keys[0].public).?)) reconnected_peer = peer;
                    bus.consume(event);
                },
                .disconnected => |peer| {
                    if (!std.meta.eql(peer, tampered_peer) or tampered_disconnect) return error.StaleDisconnect;
                    tampered_disconnect = true;
                    bus.consume(event);
                },
                .terminal => |terminal| {
                    if (std.meta.eql(terminal.peer, tampered_peer) and terminal.id == tampered_call) {
                        if (tampered_terminal or terminal.result != .peer_gone) return error.TamperNotRejected;
                        tampered_terminal = true;
                    } else if (reconnected_peer != null and std.meta.eql(terminal.peer, reconnected_peer.?) and terminal.id == liveness_call) {
                        if (terminal.result != .response) return error.LivenessFailed;
                        liveness_done = true;
                    } else return error.StaleTerminal;
                    bus.consume(event);
                },
                .request => |request| {
                    try hub_replies.push(.{ .peer = request.peer, .id = request.id });
                    bus.consume(event);
                },
                .chunk => return error.UnexpectedChunk,
            }
        }
        try flush_hub_replies(bus, &hub_replies);
        for (links[0..node_count]) |link| if (link.event()) |event| switch (event) {
            .connected, .disconnected => link.consume(event),
            .request => return error.UnexpectedRequest,
        };
        if (duplicate.event()) |event| switch (event) {
            .connected => {
                reconnected = true;
                duplicate.consume(event);
            },
            .disconnected => duplicate.consume(event),
            .request => |request| {
                const sent = if (duplicate.send(request.generation, .response, request.id, pong)) |_| true else |err| switch (err) {
                    error.Busy, error.Full => false,
                    else => return err,
                };
                if (sent) duplicate.consume(event);
            },
        };
        if (tampered_call == 0) {
            if (bus.call(tampered_peer, ping, sim.now() + 10_000)) |id| {
                tampered_call = id;
                sim.tamper_send(bus.slots[tampered_peer.slot].socket, 2);
                trace.add(sim.now(), "record_tamper", 0);
            } else |err| switch (err) {
                error.Busy, error.Full => {},
                else => return err,
            }
        }
        if (tampered_disconnect and tampered_terminal and reconnected and reconnected_peer != null and liveness_call == 0) {
            if (bus.call(tampered_peer, ping, sim.now() + 10_000)) |_| return error.StaleGenerationAccepted else |err| {
                if (err != error.PeerGone) return err;
            }
            if (bus.call(reconnected_peer.?, ping, sim.now() + 10_000)) |id| {
                liveness_call = id;
            } else |err| switch (err) {
                error.Busy, error.Full => {},
                else => return err,
            }
        }
        bus.assert_invariants();
        pool.assert_conservation();
        sim.advance();
    }
    if (!liveness_done or !tampered_terminal or !tampered_disconnect) return error.TamperStalled;
    const live_peer = reconnected_peer.?;
    try corrupt.init(sim, &pool, &endpoints, keys[0], hub_key.public, hello_payloads[0], .{ .context = &context, .hello_reply = Context.on_reply });
    var handshake_corrupted = false;
    var handshake_rejected = false;
    step = 0;
    while (step < 10_000 and !corrupt.done()) : (step += 1) {
        try bus.drive();
        for (links[0..node_count]) |link| try link.drive();
        try duplicate.drive();
        try corrupt.drive();
        if (bus.event()) |event| {
            trace.add(sim.now(), @tagName(std.meta.activeTag(event)), 0);
            switch (event) {
                .request => |request| {
                    try hub_replies.push(.{ .peer = request.peer, .id = request.id });
                    bus.consume(event);
                },
                else => return error.UnexpectedFleetEvent,
            }
        }
        for (links[0..node_count]) |link| if (link.event()) |event| switch (event) {
            .connected, .disconnected => link.consume(event),
            .request => return error.UnexpectedRequest,
        };
        if (duplicate.event()) |event| switch (event) {
            .connected, .disconnected => duplicate.consume(event),
            .request => return error.UnexpectedRequest,
        };
        try flush_hub_replies(bus, &hub_replies);
        if (corrupt.event() != null or corrupt.generation != 0) return error.TamperedHandshakeAccepted;
        const candidate = &corrupt.candidates[1];
        if (!handshake_corrupted and candidate.state == .handshake) {
            sim.tamper_send(candidate.socket, 40);
            handshake_corrupted = true;
            trace.add(sim.now(), "handshake_tamper", 0);
        }
        if (handshake_corrupted and candidate.state == .finished and !handshake_rejected) {
            handshake_rejected = true;
            corrupt.shutdown();
        }
        bus.assert_invariants();
        pool.assert_conservation();
        sim.advance();
    }
    if (!handshake_rejected or !corrupt.done()) return error.TamperedHandshakeStalled;
    if (bus.current(keys[0].public) == null or !std.meta.eql(bus.current(keys[0].public).?, live_peer)) return error.TamperedHandshakeReplacedPeer;
    var malformed_call: u64 = 0;
    var malformed_request_seen = false;
    var malformed_sent = false;
    var malformed_terminal = false;
    var malformed_disconnect = false;
    var malformed_reconnected = false;
    var malformed_peer: ?Handle = null;
    var malformed_liveness_call: u64 = 0;
    var malformed_liveness_done = false;
    step = 0;
    while (step < 20_000 and !malformed_liveness_done) : (step += 1) {
        try bus.drive();
        for (links[0..node_count]) |link| try link.drive();
        try duplicate.drive();
        if (bus.event()) |event| {
            trace.add(sim.now(), @tagName(std.meta.activeTag(event)), 0);
            switch (event) {
                .connected => |peer| {
                    if (!std.meta.eql(peer, live_peer) and bus.current(keys[0].public) != null and std.meta.eql(peer, bus.current(keys[0].public).?)) malformed_peer = peer;
                    bus.consume(event);
                },
                .disconnected => |peer| {
                    if (!std.meta.eql(peer, live_peer) or malformed_disconnect) return error.StaleDisconnect;
                    malformed_disconnect = true;
                    bus.consume(event);
                },
                .terminal => |terminal| {
                    if (std.meta.eql(terminal.peer, live_peer) and terminal.id == malformed_call) {
                        if (malformed_terminal or terminal.result != .peer_gone) return error.MalformedFrameAccepted;
                        malformed_terminal = true;
                    } else if (malformed_peer != null and std.meta.eql(terminal.peer, malformed_peer.?) and terminal.id == malformed_liveness_call) {
                        if (terminal.result != .response) return error.LivenessFailed;
                        malformed_liveness_done = true;
                    } else return error.StaleTerminal;
                    bus.consume(event);
                },
                .request => |request| {
                    try hub_replies.push(.{ .peer = request.peer, .id = request.id });
                    bus.consume(event);
                },
                .chunk => return error.UnexpectedChunk,
            }
        }
        for (links[0..node_count]) |link| if (link.event()) |event| switch (event) {
            .connected, .disconnected => link.consume(event),
            .request => return error.UnexpectedRequest,
        };
        if (duplicate.event()) |event| switch (event) {
            .connected => {
                malformed_reconnected = true;
                duplicate.consume(event);
            },
            .disconnected => duplicate.consume(event),
            .request => |request| {
                if (!malformed_request_seen) {
                    if (request.id != malformed_call) return error.UnexpectedRequest;
                    malformed_request_seen = true;
                    duplicate.consume(event);
                } else {
                    const sent = if (duplicate.send(request.generation, .response, request.id, pong)) |_| true else |err| switch (err) {
                        error.Busy, error.Full => false,
                        else => return err,
                    };
                    if (sent) duplicate.consume(event);
                }
            },
        };
        try flush_hub_replies(bus, &hub_replies);
        if (malformed_call == 0) {
            if (bus.call(live_peer, ping, sim.now() + 10_000)) |id| {
                malformed_call = id;
            } else |err| switch (err) {
                error.Busy, error.Full => {},
                else => return err,
            }
        }
        if (malformed_request_seen and !malformed_sent) {
            const conn = &bus.slots[live_peer.slot].conn;
            if (conn.tx_frame.len == 0 and conn.tx_op.state == .idle and conn.tx_wire_pos == conn.tx_wire_len) {
                var invalid_length: [4]u8 = undefined;
                std.mem.writeInt(u32, &invalid_length, 3, .little);
                const wire = try conn.session.seal_record(&invalid_length, &conn.tx_wire);
                conn.tx_wire_len = wire.len;
                conn.tx_wire_pos = 0;
                malformed_sent = true;
                trace.add(sim.now(), "bad_frame", 0);
            }
        }
        if (malformed_disconnect and malformed_terminal and malformed_reconnected and malformed_peer != null and malformed_liveness_call == 0) {
            if (bus.call(malformed_peer.?, ping, sim.now() + 10_000)) |id| {
                malformed_liveness_call = id;
            } else |err| switch (err) {
                error.Busy, error.Full => {},
                else => return err,
            }
        }
        bus.assert_invariants();
        pool.assert_conservation();
        sim.advance();
    }
    if (!malformed_sent or !malformed_liveness_done) return error.MalformedFrameStalled;
    var revocation_keys: [8][32]u8 = undefined;
    @memcpy(revocation_keys[0 .. node_count + 1], allowed[0 .. node_count + 1]);
    revocation_keys[node_count + 1] = rogue_key.public;
    try bus.set_allowed(revocation_keys[0 .. node_count + 2]);
    try rogue.init(sim, &pool, &endpoints, rogue_key, hub_key.public, hello_payloads[0], .{ .context = &context, .hello_reply = Context.on_reply });
    var attempted = false;
    var rejected = false;
    var shutdown_handshake = false;
    step = 0;
    while (step < 10_000 and !shutdown_handshake) : (step += 1) {
        try bus.drive();
        for (links[0..node_count]) |link| try link.drive();
        try duplicate.drive();
        try rogue.drive();
        if (bus.event()) |event| {
            trace.add(sim.now(), @tagName(std.meta.activeTag(event)), 0);
            switch (event) {
                .request => |request| {
                    try hub_replies.push(.{ .peer = request.peer, .id = request.id });
                    bus.consume(event);
                },
                else => return error.UnexpectedFleetEvent,
            }
        }
        for (links[0..node_count]) |link| if (link.event()) |event| switch (event) {
            .connected, .disconnected => link.consume(event),
            .request => return error.UnexpectedRequest,
        };
        if (duplicate.event()) |event| switch (event) {
            .connected, .disconnected => duplicate.consume(event),
            .request => return error.UnexpectedRequest,
        };
        try flush_hub_replies(bus, &hub_replies);
        if (rogue.event() != null or rogue.generation != 0) return error.UnknownPeerAccepted;
        for (&bus.slots) |*slot| {
            if ((slot.state == .hello or slot.state == .established) and std.mem.eql(u8, &slot.peer, &rogue_key.public)) return error.UnknownPeerAccepted;
        }
        const candidate = &rogue.candidates[1];
        if (candidate.state == .handshake) {
            if (!attempted) {
                for (&bus.slots) |*slot| {
                    if (slot.state == .handshake) {
                        try bus.set_allowed(allowed[0 .. node_count + 1]);
                        attempted = true;
                        break;
                    }
                }
            }
            if (rejected and (candidate.hs.recv_op.state == .submitted or candidate.hs.send_op.state == .submitted)) shutdown_handshake = true;
        }
        if (attempted and candidate.state == .finished) rejected = true;
        bus.assert_invariants();
        pool.assert_conservation();
        sim.advance();
    }
    if (!rejected or !shutdown_handshake) return error.UnknownPeerStalled;
    bus.shutdown();
    for (links[0..node_count]) |link| link.shutdown();
    duplicate.shutdown();
    rogue.shutdown();
    step = 0;
    while (step < 20_000) : (step += 1) {
        try bus.drive();
        var all_done = bus.done();
        try duplicate.drive();
        if (!duplicate.done()) all_done = false;
        try rogue.drive();
        if (!rogue.done()) all_done = false;
        for (links[0..node_count]) |link| {
            try link.drive();
            if (!link.done()) all_done = false;
        }
        while (bus.event()) |event| bus.consume(event);
        for (links[0..node_count]) |link| while (link.event()) |event| link.consume(event);
        while (duplicate.event()) |event| duplicate.consume(event);
        while (rogue.event()) |event| rogue.consume(event);
        bus.assert_invariants();
        pool.assert_conservation();
        if (all_done) break;
        sim.advance();
    }
    if (step == 20_000) return error.ShutdownStalled;
    sim.assert_drained();
    pool.assert_conservation();
    tiger.assert(pool.free_count == bounded.slab_count);
    arena.begin_deinit();
    for (links[0..node_count]) |link| {
        link.deinit();
        allocator.destroy(link);
    }
    duplicate.deinit();
    rogue.deinit();
    corrupt.deinit();
    bus.deinit();
    allocator.destroy(duplicate);
    allocator.destroy(rogue);
    allocator.destroy(corrupt);
    allocator.destroy(bus);
    allocator.destroy(sim);
    allocator.free(actual);
    allocator.free(chunk);
    pool.deinit(allocator);
    arena.finish_deinit();
}
