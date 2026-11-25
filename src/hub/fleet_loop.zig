const std = @import("std");
const tiger = @import("../tiger.zig");
const version = @import("../version.zig");
const io = @import("../net/io.zig");
const backend_mod = @import("../net/backend.zig");
const noise = @import("../net/noise.zig");
const frame = @import("../net/frame.zig");
const msg = @import("../net/msg.zig");
const bounded = @import("../net/bounded.zig");
const Gauge = @import("../net/gauge.zig").Gauge;
const TickGuard = @import("../net/tick_guard.zig").TickGuard;
const limits = @import("../net/limits.zig");
const fleet_bus = @import("../net/fleet_bus.zig");

const Backend = backend_mod.Backend;
const Bus = fleet_bus.FleetBus(Backend);

pub const call_slots = 64;
pub const commands_max = 128;
pub const events_max = 32;
pub const node_id_max = 64;
pub const node_name_max = 128;
pub const peers_max = fleet_bus.allowed_max;
pub const long_calls_per_peer = 8;
const reply_max = 1024;
const events_per_slot_max = 32;
const passes_max = 8;
const backlog = 128;

comptime {
    tiger.comptime_assert(call_slots <= std.math.maxInt(u8), "call slot index fits a command");
    tiger.comptime_assert(limits.fleet_hello_max <= frame.max_payload, "a hello fits one frame");
    tiger.comptime_assert(long_calls_per_peer < bounded.rpc_slots_per_node, "control calls keep RPC slots");
}

pub const AllowedPeer = struct {
    node_id: []const u8,
    name: []const u8,
    pub_key: [32]u8,
};

pub const CallError = error{ NodeOffline, Timeout, Busy, ReplyTooLarge, Invalid };

pub const Reply = struct {
    kind: frame.Kind,
    chunks: []const u8,
    payload: []const u8,
};

const Outcome = union(enum) {
    reply: struct { kind: frame.Kind, chunk_len: usize, payload_len: usize },
    failed: CallError,
};

const Call = struct {
    node_id: [node_id_max]u8 = undefined,
    node_len: usize = 0,
    request: []const u8 = &.{},
    reply: []u8 = &.{},
    chunk_cap: usize = 0,
    timeout_ms: u32 = 0,
    long: bool = false,

    phase: enum { idle, queued, inflight } = .idle,
    peer: fleet_bus.Handle = undefined,
    rpc: u64 = 0,
    deadline: u64 = 0,
    chunk_len: usize = 0,
    overflow: bool = false,

    mu: std.Io.Mutex = .init,
    cond: std.Io.Condition = .init,
    done: bool = false,
    outcome: Outcome = undefined,
};

const Command = union(enum) {
    call: u8,
    disconnect: struct { id: [node_id_max]u8, len: u8 },
};

pub const Event = struct {
    kind: enum { connected, telemetry, disconnected },
    id_len: u8,
    id: [node_id_max]u8,
    len: u32,
    payload: [limits.fleet_hello_max]u8,

    pub fn node_id(self: *const Event) []const u8 {
        return self.id[0..self.id_len];
    }

    pub fn body(self: *const Event) []const u8 {
        return self.payload[0..self.len];
    }
};

const EventRing = struct {
    items: [events_max]Event,
    head: usize,
    len: usize,

    fn full(self: *const EventRing) bool {
        return self.len == events_max;
    }

    fn tail(self: *EventRing) *Event {
        tiger.assert(!self.full());
        return &self.items[(self.head + self.len) % events_max];
    }
};

const Peer = struct {
    key: [32]u8,
    id_len: u8,
    name_len: u8,
    id: [node_id_max]u8,
    name: [node_name_max]u8,
};

const Peers = struct {
    count: usize,
    entries: [peers_max]Peer,
};

const SlotInfo = struct {
    used: bool = false,
    key: [32]u8 = undefined,
    id_len: u8 = 0,
    id: [node_id_max]u8 = undefined,
};

const PendingReply = struct {
    active: bool = false,
    generation: u64 = 0,
    id: u64 = 0,
    kind: frame.Kind = .response,
    len: usize = 0,
    buf: [reply_max]u8 = undefined,
};

const Stash = struct {
    used: bool,
    len: usize,
    payload: [limits.fleet_hello_max]u8,
};

pub const Gauges = struct {
    slots: Gauge = .init(bounded.fleet_slots, 0),
    slabs: Gauge = .init(bounded.slab_count, bounded.control_reserve),
    rpc: Gauge = .init(call_slots, 0),
    events: Gauge = .init(events_max, 0),
};

pub const Loop = struct {
    gpa: std.mem.Allocator,
    io: std.Io,
    static: *bounded.StaticAllocator,
    backend: *Backend,
    pool: *bounded.SlabPool,
    bus: *Bus,
    port: u16,

    peers: *Peers,
    keys: *[peers_max][32]u8,
    stash: *[bounded.fleet_slots]Stash,
    tmp: *[limits.fleet_hello_max]u8,
    info: [bounded.fleet_slots]SlotInfo = @splat(.{}),
    replies: [bounded.fleet_slots]PendingReply = @splat(.{}),
    stalled: bool = false,
    slots_high: usize = 0,
    guard: TickGuard = .{ .name = "fleet" },
    gauges: Gauges = .{},

    snap_mu: std.Io.Mutex = .init,
    staging: *Peers,
    peers_dirty: std.atomic.Value(bool) = .init(false),

    mu: std.Io.Mutex = .init,
    commands: bounded.Ring(Command, commands_max) = .{},
    busy: [call_slots]bool = @splat(false),
    closed: bool = false,
    calls: [call_slots]Call = @splat(.{}),
    stop_requested: std.atomic.Value(bool) = .init(false),

    ev_mu: std.Io.Mutex = .init,
    ev_cond: std.Io.Condition = .init,
    events: *EventRing,
    finished: std.atomic.Value(bool) = .init(false),

    pub const reserved_bytes: usize = @sizeOf(Loop) + @sizeOf(Backend) + @sizeOf(bounded.SlabPool) +
        bounded.slab_count * bounded.slab_bytes + @sizeOf(Bus) + @sizeOf(Peers) * 2 + @sizeOf([peers_max][32]u8) +
        @sizeOf([bounded.fleet_slots]Stash) + limits.fleet_hello_max + @sizeOf(EventRing) + @sizeOf(bounded.StaticAllocator);

    pub fn create(gpa: std.mem.Allocator, io_handle: std.Io, key: noise.KeyPair, endpoint: io.Endpoint) !*Loop {
        const static = try gpa.create(bounded.StaticAllocator);
        errdefer gpa.destroy(static);
        static.* = .{ .parent = gpa };
        const a = static.allocator();

        const self = try a.create(Loop);
        errdefer a.destroy(self);
        const backend = try a.create(Backend);
        errdefer a.destroy(backend);
        backend.* = try Backend.init();
        errdefer backend.deinit();
        const pool = try a.create(bounded.SlabPool);
        errdefer a.destroy(pool);
        pool.* = try bounded.SlabPool.init(a);
        errdefer pool.deinit(a);
        const bus = try a.create(Bus);
        errdefer a.destroy(bus);
        const peers = try a.create(Peers);
        errdefer a.destroy(peers);
        const staging = try a.create(Peers);
        errdefer a.destroy(staging);
        const keys = try a.create([peers_max][32]u8);
        errdefer a.destroy(keys);
        const stash = try a.create([bounded.fleet_slots]Stash);
        errdefer a.destroy(stash);
        const tmp = try a.create([limits.fleet_hello_max]u8);
        errdefer a.destroy(tmp);
        const events = try a.create(EventRing);
        errdefer a.destroy(events);
        peers.count = 0;
        staging.count = 0;
        for (stash) |*entry| entry.used = false;
        events.head = 0;
        events.len = 0;

        const listener = try Backend.listen(endpoint, backlog);
        errdefer Backend.close_now(listener.socket);

        self.* = .{
            .gpa = gpa,
            .io = io_handle,
            .static = static,
            .backend = backend,
            .pool = pool,
            .bus = bus,
            .port = listener.port,
            .peers = peers,
            .keys = keys,
            .stash = stash,
            .tmp = tmp,
            .staging = staging,
            .events = events,
        };
        try bus.init(backend, pool, listener.socket, key, &.{}, .{ .context = self, .hello = hook_hello });
        tiger.assert(reserved_bytes <= 48 * 1024 * 1024);
        return self;
    }

    pub fn freeze(self: *Loop) void {
        self.static.freeze();
    }

    pub fn destroy(self: *Loop) void {
        tiger.assert(self.finished.load(.acquire) or !self.bus.stopping);
        const static = self.static;
        const gpa = self.gpa;
        if (static.state == .init) static.freeze();
        static.begin_deinit();
        self.bus.deinit();
        const a = static.allocator();
        const backend = self.backend;
        backend.deinit();
        self.pool.deinit(a);
        a.destroy(self.pool);
        a.destroy(self.bus);
        a.destroy(self.peers);
        a.destroy(self.staging);
        a.destroy(self.keys);
        a.destroy(self.stash);
        a.destroy(self.tmp);
        a.destroy(self.events);
        a.destroy(backend);
        a.destroy(self);
        static.finish_deinit();
        gpa.destroy(static);
    }

    pub fn request_stop(self: *Loop) void {
        self.stop_requested.store(true, .release);
        self.backend.wake();
    }

    pub fn publish(self: *Loop, allowed: []const AllowedPeer) void {
        self.snap_mu.lockUncancelable(self.io);
        var count: usize = 0;
        for (allowed) |peer| {
            if (peer.node_id.len == 0 or peer.node_id.len > node_id_max or count == peers_max) continue;
            const entry = &self.staging.entries[count];
            entry.key = peer.pub_key;
            entry.id_len = @intCast(peer.node_id.len);
            @memcpy(entry.id[0..peer.node_id.len], peer.node_id);
            const name = peer.name[0..@min(peer.name.len, node_name_max)];
            entry.name_len = @intCast(name.len);
            @memcpy(entry.name[0..name.len], name);
            count += 1;
        }
        self.staging.count = count;
        self.snap_mu.unlock(self.io);
        self.peers_dirty.store(true, .release);
        self.backend.wake();
    }

    /// Takes a call slot; the caller sizes its reply buffer only after this succeeds and gives the slot back with release.
    pub fn reserve(self: *Loop) CallError!u8 {
        self.mu.lockUncancelable(self.io);
        defer self.mu.unlock(self.io);
        if (self.closed) return error.NodeOffline;
        var count: usize = 0;
        var found: ?u8 = null;
        for (&self.busy, 0..) |*used, i| {
            if (used.*) {
                count += 1;
            } else if (found == null) {
                found = @intCast(i);
            }
        }
        const index = found orelse {
            self.gauges.rpc.refuse();
            return error.Busy;
        };
        self.busy[index] = true;
        self.gauges.rpc.set(count + 1);
        return index;
    }

    pub fn release(self: *Loop, index: u8) void {
        self.mu.lockUncancelable(self.io);
        tiger.assert(self.busy[index]);
        self.busy[index] = false;
        var count: usize = 0;
        for (self.busy) |used| {
            if (used) count += 1;
        }
        self.gauges.rpc.set(count);
        self.mu.unlock(self.io);
    }

    /// Runs one call on a reserved slot; the slot stays reserved after it returns, until release.
    /// long marks a call that may wait on the node for minutes; each peer admits only long_calls_per_peer of them at once.
    pub fn call(self: *Loop, index: u8, node_id: []const u8, request: []const u8, reply: []u8, chunk_cap: usize, timeout_ms: u32, long: bool) CallError!Reply {
        if (node_id.len == 0 or node_id.len > node_id_max) return error.NodeOffline;
        if (chunk_cap > reply.len) return error.Invalid;
        {
            self.mu.lockUncancelable(self.io);
            defer self.mu.unlock(self.io);
            tiger.assert(self.busy[index]);
            if (self.closed) return error.NodeOffline;
            if (self.commands.len == commands_max) return error.Busy;
            const c = &self.calls[index];
            c.node_len = node_id.len;
            @memcpy(c.node_id[0..node_id.len], node_id);
            c.request = request;
            c.reply = reply;
            c.chunk_cap = chunk_cap;
            c.timeout_ms = timeout_ms;
            c.long = long;
            c.done = false;
            self.commands.push(.{ .call = index }) catch unreachable;
        }
        self.backend.wake();
        const c = &self.calls[index];
        c.mu.lockUncancelable(self.io);
        while (!c.done) c.cond.waitUncancelable(self.io, &c.mu);
        const outcome = c.outcome;
        c.mu.unlock(self.io);
        return switch (outcome) {
            .reply => |r| .{ .kind = r.kind, .chunks = reply[0..r.chunk_len], .payload = reply[chunk_cap..][0..r.payload_len] },
            .failed => |err| err,
        };
    }

    pub fn disconnect(self: *Loop, node_id: []const u8) void {
        if (node_id.len == 0 or node_id.len > node_id_max) return;
        var command: Command = .{ .disconnect = .{ .id = undefined, .len = @intCast(node_id.len) } };
        @memcpy(command.disconnect.id[0..node_id.len], node_id);
        var attempts: usize = 0;
        while (attempts < 200) : (attempts += 1) {
            self.mu.lockUncancelable(self.io);
            const pushed = !self.closed and (self.commands.push(command) catch null) != null;
            const closed = self.closed;
            self.mu.unlock(self.io);
            if (pushed) {
                self.backend.wake();
                return;
            }
            if (closed) return;
            self.io.sleep(.fromMilliseconds(5), .awake) catch return;
        }
        std.debug.print("fleet: disconnect command queue stayed full; {s} stays connected until it goes quiet\n", .{node_id});
    }

    pub fn next_event(self: *Loop, out: *Event) bool {
        self.ev_mu.lockUncancelable(self.io);
        defer self.ev_mu.unlock(self.io);
        while (self.events.len == 0) {
            if (self.finished.load(.acquire)) return false;
            self.ev_cond.waitUncancelable(self.io, &self.ev_mu);
        }
        const front = &self.events.items[self.events.head];
        out.kind = front.kind;
        out.id_len = front.id_len;
        out.id = front.id;
        out.len = front.len;
        @memcpy(out.payload[0..front.len], front.payload[0..front.len]);
        self.events.head = (self.events.head + 1) % events_max;
        self.events.len -= 1;
        self.gauges.events.set(self.events.len);
        return true;
    }

    pub fn run(self: *Loop) void {
        var stopping = false;
        while (true) {
            if (!stopping and self.stop_requested.load(.acquire)) {
                stopping = true;
                self.fail_calls(error.NodeOffline);
                self.bus.shutdown();
            }
            self.turn(stopping);
            if (stopping and self.bus.done()) break;
            const wait: u32 = if (self.stalled) 2 else bounded.tick_ms;
            self.backend.poll(wait) catch |err| std.debug.panic("fleet loop: backend poll failed: {s}", .{@errorName(err)});
        }
        self.mu.lockUncancelable(self.io);
        self.closed = true;
        while (self.commands.pop()) |command| switch (command) {
            .call => |index| self.complete(&self.calls[index], .{ .failed = error.NodeOffline }),
            .disconnect => {},
        };
        self.mu.unlock(self.io);
        tiger.assert(self.backend.pending_count == 0);
        self.ev_mu.lockUncancelable(self.io);
        self.finished.store(true, .release);
        self.ev_cond.broadcast(self.io);
        self.ev_mu.unlock(self.io);
    }

    fn turn(self: *Loop, stopping: bool) void {
        self.stalled = false;
        self.guard.begin(self.backend);
        if (!stopping) self.apply_peers();
        self.take_commands(stopping);
        self.guard.mark(self.backend, "peers and commands");
        var pass: usize = 0;
        while (pass < passes_max) : (pass += 1) {
            self.bus.drive() catch |err| std.debug.panic("fleet loop: bus drive failed: {s}", .{@errorName(err)});
            self.guard.mark(self.backend, "bus drive");
            var progress = self.flush_replies();
            self.guard.mark(self.backend, "flush replies");
            for (0..bounded.fleet_slots) |slot| {
                if (self.service_slot(slot)) progress = true;
            }
            self.guard.mark(self.backend, "service slots");
            if (self.retry_calls()) progress = true;
            self.release_stash();
            self.guard.mark(self.backend, "retry calls");
            if (!progress) break;
        }
        const in_use = self.bus.slots_in_use();
        self.slots_high = @max(self.slots_high, in_use);
        self.gauges.slots.publish(in_use, self.slots_high, self.bus.refused);
        self.gauges.slabs.publish(bounded.slab_count - self.pool.free_count, self.pool.high_water, self.pool.refused);
        if (pass == passes_max) self.stalled = true;
        self.guard.end(self.backend, "gauges");
    }

    fn apply_peers(self: *Loop) void {
        if (!self.peers_dirty.swap(false, .acq_rel)) return;
        self.snap_mu.lockUncancelable(self.io);
        const count = self.staging.count;
        @memcpy(self.peers.entries[0..count], self.staging.entries[0..count]);
        self.peers.count = count;
        self.snap_mu.unlock(self.io);
        for (self.peers.entries[0..count], 0..) |entry, i| self.keys[i] = entry.key;
        self.bus.set_allowed(self.keys[0..count]) catch unreachable;
    }

    fn take_commands(self: *Loop, stopping: bool) void {
        if (!stopping) self.apply_peers();
        while (true) {
            self.mu.lockUncancelable(self.io);
            const command = self.commands.pop();
            self.mu.unlock(self.io);
            switch (command orelse return) {
                .call => |index| {
                    const c = &self.calls[index];
                    tiger.assert(c.phase == .idle);
                    if (stopping) {
                        self.complete(c, .{ .failed = error.NodeOffline });
                        continue;
                    }
                    c.phase = .queued;
                    c.rpc = 0;
                    c.chunk_len = 0;
                    c.overflow = false;
                    c.deadline = self.backend.now() +| c.timeout_ms;
                    self.try_call(c);
                },
                .disconnect => |d| {
                    if (!stopping) self.apply_peers();
                    if (self.find_peer(d.id[0..d.len])) |peer| self.bus.disconnect_quiet(peer);
                },
            }
        }
    }

    fn find_peer(self: *Loop, node_id: []const u8) ?fleet_bus.Handle {
        for (&self.info, 0..) |*entry, i| {
            if (!entry.used or !std.mem.eql(u8, entry.id[0..entry.id_len], node_id)) continue;
            const peer = self.bus.current(entry.key) orelse continue;
            if (peer.slot == i) return peer;
        }
        return null;
    }

    fn complete(self: *Loop, c: *Call, outcome: Outcome) void {
        c.phase = .idle;
        c.mu.lockUncancelable(self.io);
        c.outcome = outcome;
        c.done = true;
        c.cond.signal(self.io);
        c.mu.unlock(self.io);
    }

    fn fail_calls(self: *Loop, err: CallError) void {
        for (&self.calls) |*c| {
            if (c.phase != .idle) self.complete(c, .{ .failed = err });
        }
    }

    fn try_call(self: *Loop, c: *Call) void {
        tiger.assert(c.phase == .queued);
        const peer = self.find_peer(c.node_id[0..c.node_len]) orelse return self.complete(c, .{ .failed = error.NodeOffline });
        if (c.long and self.long_inflight(peer) >= long_calls_per_peer) return self.complete(c, .{ .failed = error.Busy });
        if (self.bus.call(peer, c.request, c.deadline)) |rpc| {
            c.peer = peer;
            c.rpc = rpc;
            c.phase = .inflight;
        } else |err| switch (err) {
            error.Busy, error.Full => {
                if (self.backend.now() >= c.deadline) return self.complete(c, .{ .failed = error.Busy });
                self.stalled = true;
            },
            error.TooLarge => self.complete(c, .{ .failed = error.Invalid }),
            else => self.complete(c, .{ .failed = error.NodeOffline }),
        }
    }

    fn long_inflight(self: *Loop, peer: fleet_bus.Handle) usize {
        var count: usize = 0;
        for (&self.calls) |*other| {
            if (other.phase == .inflight and other.long and std.meta.eql(other.peer, peer)) count += 1;
        }
        return count;
    }

    fn retry_calls(self: *Loop) bool {
        var progress = false;
        for (&self.calls) |*c| {
            if (c.phase != .queued) continue;
            self.try_call(c);
            if (c.phase != .queued) progress = true;
        }
        return progress;
    }

    fn find_call(self: *Loop, peer: fleet_bus.Handle, rpc: u64) ?*Call {
        for (&self.calls) |*c| {
            if (c.phase == .inflight and c.rpc == rpc and std.meta.eql(c.peer, peer)) return c;
        }
        return null;
    }

    fn peer_name(self: *Loop, slot: usize) []const u8 {
        const info = &self.info[slot];
        for (self.peers.entries[0..self.peers.count]) |*entry| {
            if (std.mem.eql(u8, &entry.key, &info.key)) return entry.name[0..entry.name_len];
        }
        return "";
    }

    fn hook_hello(context: *anyopaque, slot: u8, key: [32]u8, payload: []const u8, out: []u8) ?fleet_bus.HelloResult {
        const self: *Loop = @ptrCast(@alignCast(context));
        if (payload.len > limits.fleet_hello_max or request_kind(payload) != .hello) return null;
        var index: ?usize = null;
        for (self.peers.entries[0..self.peers.count], 0..) |*entry, i| {
            if (std.mem.eql(u8, &entry.key, &key)) {
                index = i;
                break;
            }
        }
        const entry = &self.peers.entries[index orelse return null];
        const reply = msg.encode_fixed(out, msg.HelloReply{ .ok = true, .hub_version = version.version, .node_name = entry.name[0..entry.name_len] }) catch return null;
        const stash = &self.stash[slot];
        stash.used = true;
        @memcpy(stash.payload[0..payload.len], payload);
        stash.len = payload.len;
        const info = &self.info[slot];
        info.used = true;
        info.key = key;
        info.id_len = entry.id_len;
        info.id = entry.id;
        return .{ .identity = key, .reply_len = reply.len };
    }

    fn release_stash(self: *Loop) void {
        for (self.stash, 0..) |*entry, slot| {
            if (entry.used and !self.bus.slots[slot].connected_event) entry.used = false;
        }
    }

    const RequestKind = enum { hello, other, invalid };

    fn request_kind(payload: []const u8) RequestKind {
        const Probe = struct { type: []const u8 };
        var scratch: [2048]u8 = undefined;
        var fba = std.heap.FixedBufferAllocator.init(&scratch);
        const probe = std.json.parseFromSliceLeaky(Probe, fba.allocator(), payload, .{ .ignore_unknown_fields = true, .allocate = .alloc_if_needed }) catch return .invalid;
        return if (std.mem.eql(u8, probe.type, "hello")) .hello else .other;
    }

    fn push_event(self: *Loop, kind: @FieldType(Event, "kind"), node_id: []const u8, payload: []const u8) bool {
        self.ev_mu.lockUncancelable(self.io);
        defer self.ev_mu.unlock(self.io);
        if (self.events.full()) return false;
        const slot = self.events.tail();
        slot.kind = kind;
        slot.id_len = @intCast(node_id.len);
        @memcpy(slot.id[0..node_id.len], node_id);
        slot.len = @intCast(payload.len);
        @memcpy(slot.payload[0..payload.len], payload);
        self.events.len += 1;
        self.gauges.events.set(self.events.len);
        self.ev_cond.signal(self.io);
        return true;
    }

    fn queue_reply(self: *Loop, peer: fleet_bus.Handle, id: u64, kind: frame.Kind, payload: []const u8) void {
        const pending = &self.replies[peer.slot];
        tiger.assert(!pending.active and payload.len <= pending.buf.len);
        @memcpy(pending.buf[0..payload.len], payload);
        pending.* = .{ .active = true, .generation = peer.generation, .id = id, .kind = kind, .len = payload.len, .buf = pending.buf };
    }

    fn flush_replies(self: *Loop) bool {
        var progress = false;
        for (&self.replies, 0..) |*pending, slot| {
            if (!pending.active) continue;
            const peer: fleet_bus.Handle = .{ .slot = @intCast(slot), .generation = pending.generation };
            if (self.bus.send(peer, pending.kind, pending.id, pending.buf[0..pending.len])) |_| {
                pending.active = false;
                progress = true;
            } else |err| switch (err) {
                error.Busy, error.Full => self.stalled = true,
                else => {
                    pending.active = false;
                    progress = true;
                },
            }
        }
        return progress;
    }

    fn service_slot(self: *Loop, slot: usize) bool {
        var progress = false;
        var handled: usize = 0;
        while (handled < events_per_slot_max) : (handled += 1) {
            const ev = self.bus.event_for(slot) orelse break;
            const consumed = switch (ev) {
                .connected => |peer| self.on_connected(peer),
                .disconnected => |peer| self.on_disconnected(peer),
                .request => |r| self.on_request(r),
                .chunk => |c| self.on_chunk(c),
                .terminal => |t| self.on_terminal(t),
            };
            if (!consumed) {
                self.stalled = true;
                break;
            }
            self.bus.consume(ev);
            progress = true;
        }
        if (handled == events_per_slot_max) self.stalled = true;
        return progress;
    }

    fn on_connected(self: *Loop, peer: fleet_bus.Handle) bool {
        const info = &self.info[peer.slot];
        const entry = &self.stash[peer.slot];
        if (!entry.used) return true;
        if (!self.push_event(.connected, info.id[0..info.id_len], entry.payload[0..entry.len])) return false;
        entry.used = false;
        return true;
    }

    fn on_disconnected(self: *Loop, peer: fleet_bus.Handle) bool {
        const info = &self.info[peer.slot];
        if (self.bus.current(info.key)) |newer| {
            if (!std.meta.eql(newer, peer)) return true;
        }
        return self.push_event(.disconnected, info.id[0..info.id_len], &.{});
    }

    fn on_request(self: *Loop, r: anytype) bool {
        if (self.replies[r.peer.slot].active) return false;
        var buffer: [reply_max]u8 = undefined;
        if (r.size > limits.fleet_hello_max) {
            const body = msg.encode_fixed(&buffer, msg.ErrorReply{ .code = .invalid, .message = "request too large" }) catch unreachable;
            self.queue_reply(r.peer, r.id, .err, body);
            return true;
        }
        self.bus.read(r.peer, r.serial, 0, self.tmp[0..r.size]) catch return true;
        const payload = self.tmp[0..r.size];
        switch (request_kind(payload)) {
            .hello => {
                const info = &self.info[r.peer.slot];
                if (!self.push_event(.telemetry, info.id[0..info.id_len], payload)) self.gauges.events.refuse();
                const body = msg.encode_fixed(&buffer, msg.HelloReply{ .ok = true, .hub_version = version.version, .node_name = self.peer_name(r.peer.slot) }) catch
                    msg.encode_fixed(&buffer, msg.HelloReply{ .ok = true, .hub_version = version.version }) catch unreachable;
                self.queue_reply(r.peer, r.id, .response, body);
            },
            .other, .invalid => {
                const body = msg.encode_fixed(&buffer, msg.ErrorReply{ .code = .unknown_request, .message = "unsupported node request" }) catch unreachable;
                self.queue_reply(r.peer, r.id, .err, body);
            },
        }
        return true;
    }

    fn on_chunk(self: *Loop, chunk: anytype) bool {
        const c = self.find_call(chunk.peer, chunk.id) orelse return true;
        if (c.overflow) return true;
        if (chunk.size > c.chunk_cap - c.chunk_len) {
            c.overflow = true;
            return true;
        }
        self.bus.read(chunk.peer, chunk.serial, 0, c.reply[c.chunk_len..][0..chunk.size]) catch {
            c.overflow = true;
            return true;
        };
        c.chunk_len += chunk.size;
        return true;
    }

    fn on_terminal(self: *Loop, t: anytype) bool {
        const c = self.find_call(t.peer, t.id) orelse return true;
        switch (t.result) {
            .timeout => self.complete(c, .{ .failed = error.Timeout }),
            .peer_gone, .peer_replaced => self.complete(c, .{ .failed = error.NodeOffline }),
            .response, .end, .remote_error => {
                const room = c.reply.len - c.chunk_cap;
                if (c.overflow or t.size > room) return self.fail_call(c, error.ReplyTooLarge);
                self.bus.read(t.peer, t.serial, 0, c.reply[c.chunk_cap..][0..t.size]) catch return self.fail_call(c, error.NodeOffline);
                const kind: frame.Kind = switch (t.result) {
                    .response => .response,
                    .end => .end,
                    else => .err,
                };
                self.complete(c, .{ .reply = .{ .kind = kind, .chunk_len = c.chunk_len, .payload_len = t.size } });
            },
        }
        return true;
    }

    fn fail_call(self: *Loop, c: *Call, err: CallError) bool {
        self.complete(c, .{ .failed = err });
        return true;
    }
};
