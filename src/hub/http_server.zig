const std = @import("std");
const builtin = @import("builtin");
const tiger = @import("../tiger.zig");
const version = @import("../version.zig");
const schema = @import("../api/schema.zig");
const web_assets = @import("web_assets");
const home = @import("home.zig");
const runtime = @import("runtime.zig");
const api_dispatch = @import("api_dispatch.zig");
const stream = @import("stream.zig");
const netio = @import("../net/io.zig");
const Backend = @import("../net/backend.zig").Backend;
const bounded = @import("../net/bounded.zig");
const gauge = @import("../net/gauge.zig");
const TickGuard = @import("../net/tick_guard.zig").TickGuard;
const limits = @import("../net/limits.zig");
const wire = @import("../net/http_wire.zig");
const ws_out = @import("../net/ws_out.zig");

pub const listeners_max = 2;
pub const slots_per_listener = limits.http_conns_max;
pub const slot_count = listeners_max * slots_per_listener;
pub const rbuf_bytes = wire.head_max;
pub const sbuf_bytes = 16 * 1024;
pub const body_small_bytes = 16 * 1024;
pub const body_large_bytes = 4 << 20;
pub const body_large_count = 2;
pub const transfer_bytes = 64 * 1024;
pub const transfer_count = 16;
pub const gunzip_count = 4;
pub const regular_workers = 6;
pub const regular_queue = 32;
pub const long_workers = 8;
pub const io_workers = 2;
pub const completions_max = 128;

const gunzip_piece = 12 * 1024;
const gunzip_front = 16;
const backlog = 128;
const head_timeout_ms = 10_000;
const idle_timeout_ms = 120_000;
const body_progress_ms = 30_000;
const body_total_ms = 120_000;
const send_timeout_ms = 30_000;
const linger_ms = 2_000;
const linger_bytes_max = 1 << 20;
const ws_ping_ms = 15_000;
const resync_wait_ms = 30_000;
const resyncs_max = 8;
const accept_retry_ms = 100;
const shutdown_wait_ms = 15_000;

comptime {
    tiger.comptime_assert(sbuf_bytes >= gunzip_front + gunzip_piece + 8, "a gunzip piece and its framing fit the send buffer");
    tiger.comptime_assert(slot_count + 2 <= completions_max, "every slot can hold one completion");
    tiger.comptime_assert(transfer_count <= std.math.maxInt(u8) and gunzip_count <= std.math.maxInt(u8), "indexes fit a byte");
    tiger.comptime_assert(body_small_bytes <= body_large_bytes, "large bodies are larger");
    tiger.comptime_assert(slot_count * 3 + listeners_max <= 256, "operations fit the backend");
}

const busy_body = "server busy, retry shortly\n";
const busy_response = std.fmt.comptimePrint(
    "HTTP/1.1 503 Service Unavailable\r\n" ++
        "connection: close\r\n" ++
        "retry-after: 1\r\n" ++
        "access-control-allow-origin: *\r\n" ++
        "content-type: text/plain\r\n" ++
        "content-length: {d}\r\n\r\n{s}",
    .{ busy_body.len, busy_body },
);

const json_type = "application/json";
const text_type = "text/plain";
const unauthorized_body = "{\"error\":{\"code\":\"unauthenticated\",\"message\":\"unauthorized\"}}";
const asset_missing_body = "{\"error\":{\"code\":\"not_found\",\"message\":\"client asset not found\"}}";
const api_missing_body = "{\"error\":{\"code\":\"not_found\",\"message\":\"not found\"}}";
const dispatch_failed_body = "{\"error\":{\"code\":\"internal\",\"message\":\"dispatch failed\"}}";
const cors_preflight = "access-control-allow-methods: GET, POST, OPTIONS\r\naccess-control-allow-headers: Authorization, Content-Type\r\n";

const long_methods = [_][]const u8{
    "Wait",            "EnvoyWait",   "AcquireLease", "UpdateNodes", "UpdateHub",
    "UpdateHarnesses", "SpawnSleeve", "RemoveSleeve", "GitPush",     "RunJob",
};

fn is_long_method(name: []const u8) bool {
    inline for (long_methods) |m| {
        if (std.mem.eql(u8, m, name)) return true;
    }
    return false;
}

const JobKind = enum { rpc, rpc_long, events, snapshot, client, read };

const Job = struct { slot: u16, generation: u32, kind: JobKind };

const Reply = struct {
    status: u16,
    content_type: []const u8,
    body: []const u8,
    owned: bool,
};

const Done = union(enum) {
    reply: Reply,
    events: struct { snapshot: []u8, live: []u8, seq: u64 },
    file: struct { file: std.Io.File, size: u64, content_type: []const u8 },
    piece: usize,
    failed,
};

const Completion = struct { slot: u16, generation: u32, done: Done };

fn Pool(comptime worker_count: usize, comptime queue_len: usize) type {
    return struct {
        const Self = @This();
        mu: std.Io.Mutex = .init,
        cond: std.Io.Condition = .init,
        queue: bounded.Ring(Job, queue_len) = .{},
        idle: usize = 0,
        stopping: bool = false,
        strict: bool = false,
        gauge: gauge.Gauge = .init(worker_count + queue_len, 0),
        threads: [worker_count]?std.Thread = @splat(null),

        fn start(self: *Self, server: *HttpServer) !void {
            errdefer self.stop(server.io);
            for (&self.threads) |*slot| slot.* = try std.Thread.spawn(.{}, work, .{ self, server });
        }

        /// A strict pool refuses work unless an idle worker can take it at once.
        fn submit(self: *Self, io: std.Io, job: Job) bool {
            self.mu.lockUncancelable(io);
            defer self.mu.unlock(io);
            if (self.stopping) return false;
            if ((self.strict and self.queue.len >= self.idle) or self.queue.len == queue_len) {
                self.gauge.refuse();
                return false;
            }
            self.queue.push(job) catch unreachable;
            self.gauge.set(self.queue.len + worker_count - @min(self.idle, worker_count));
            self.cond.signal(io);
            return true;
        }

        fn stop(self: *Self, io: std.Io) void {
            self.mu.lockUncancelable(io);
            self.stopping = true;
            self.cond.broadcast(io);
            self.mu.unlock(io);
            for (&self.threads) |*slot| {
                if (slot.*) |t| t.join();
                slot.* = null;
            }
        }

        fn work(self: *Self, server: *HttpServer) void {
            const io = server.io;
            while (true) {
                self.mu.lockUncancelable(io);
                while (self.queue.len == 0 and !self.stopping) {
                    self.idle += 1;
                    self.cond.waitUncancelable(io, &self.mu);
                    self.idle -= 1;
                }
                const job = self.queue.pop() orelse {
                    self.mu.unlock(io);
                    return;
                };
                self.mu.unlock(io);
                server.execute(job);
            }
        }
    };
}

const Mode = enum { free, head, body, working, responding, ws, draining, closing };
const SourceKind = enum { none, mem, gunzip, file };
const WsStage = enum { preface, snapshot, live_msg, live, resync_wait, resync_ready, closing_frame };

const RecvTarget = enum { head, body, stage };

const WsIn = struct {
    have_header: bool = false,
    fragmenting: bool = false,
    dead: bool = false,
    opcode: wire.Opcode = .continuation,
    mask: [4]u8 = undefined,
    left: u64 = 0,
    offset: u64 = 0,
    control: [wire.control_payload_max]u8 = undefined,
    control_len: usize = 0,
};

const WsOut = ws_out.Out(stream.Ring);

const Listener = struct {
    server: *HttpServer,
    index: u8,
    socket: netio.Socket,
    port: u16,
    host: []const u8,
    first_slot: usize,
    filtered: bool,
    op: netio.Operation,
    accepting: bool = false,
    closed: bool = false,
    retry_at: u64 = 0,
    refusing: bool = false,
};

const Slot = struct {
    server: *HttpServer,
    listener: u8 = 0,
    mode: Mode = .free,
    generation: u32 = 0,
    sock: netio.Socket = undefined,
    sock_open: bool = false,
    recv_op: netio.Operation = undefined,
    send_op: netio.Operation = undefined,
    close_op: netio.Operation = undefined,
    recv_inflight: bool = false,
    send_inflight: bool = false,
    close_inflight: bool = false,
    job_inflight: bool = false,

    rbuf: []u8,
    sbuf: []u8,
    body_small: []u8,
    rlen: usize = 0,
    rpos: usize = 0,
    stage_len: usize = 0,
    recv_target: RecvTarget = .head,
    scanned: usize = 0,
    slen: usize = 0,
    send_slice: []const u8 = &.{},
    deadline: u64 = 0,
    hard_deadline: u64 = 0,
    requests_done: u32 = 0,
    drained: usize = 0,

    req: wire.Request = undefined,
    path: []const u8 = "",
    query: []const u8 = "",
    head_only: bool = false,
    close_after: bool = false,
    interim_pending: bool = false,

    body: []u8 = &.{},
    body_len: usize = 0,
    body_large: ?u8 = null,
    chunk: wire.Chunked = .{},
    job_kind: JobKind = .rpc,

    src: SourceKind = .none,
    mem_data: []const u8 = &.{},
    mem_off: usize = 0,
    mem_owned: bool = false,
    gunzip: ?u8 = null,
    gunzip_done: bool = false,
    file: ?std.Io.File = null,
    file_size: u64 = 0,
    file_off: u64 = 0,
    file_reading: bool = false,
    transfer: ?u8 = null,

    ws_stage: WsStage = .live,
    ws_snapshot: []u8 = &.{},
    ws_live: []u8 = &.{},
    ws_seq: u64 = 0,
    out: WsOut = .{},
    ws_resyncs: u8 = 0,
    ws_in: WsIn = .{},
};

const Gauges = struct {
    slots: gauge.Gauge = .init(slot_count, 0),
    large: gauge.Gauge = .init(body_large_count, 0),
    transfers: gauge.Gauge = .init(transfer_count, 0),
    lagged: gauge.Gauge = .init(slot_count, 0),
};

const Gunzip = struct {
    input: std.Io.Reader,
    decoder: std.compress.flate.Decompress,
    window: [std.compress.flate.max_window_len]u8,
};

pub const Config = struct {
    primary: []const u8,
    proxy: ?[]const u8 = null,
    trusted_proxies: []const []const u8 = &.{},
};

pub const HttpServer = struct {
    gpa: std.mem.Allocator,
    io: std.Io,
    rt: *runtime.Runtime,
    static: *bounded.StaticAllocator,
    backend: *Backend,
    listeners: [listeners_max]Listener = undefined,
    listener_count: usize = 0,
    proxies: []std.Io.net.IpAddress,
    slots: []Slot,
    memory: []u8,
    large: [body_large_count][]u8 = undefined,
    large_used: [body_large_count]bool = @splat(false),
    transfers: []u8,
    transfer_used: [transfer_count]bool = @splat(false),
    gunzips: []Gunzip,
    gunzip_used: [gunzip_count]bool = @splat(false),

    now: u64 = 0,
    gauges: Gauges = .{},
    guard: TickGuard = .{ .name = "http" },
    stop_requested: std.atomic.Value(bool) = .init(false),
    stopping: std.atomic.Value(bool) = .init(false),
    abandoned: bool = false,
    loop_thread: ?std.Thread = null,

    regular: Pool(regular_workers, regular_queue) = .{},
    long: Pool(long_workers, 1) = .{ .strict = true },
    reader: Pool(io_workers, transfer_count) = .{},

    cmu: std.Io.Mutex = .init,
    completions: bounded.Ring(Completion, completions_max) = .{},

    pub const reserved_bytes: usize = @sizeOf(HttpServer) + @sizeOf(Backend) + @sizeOf(bounded.StaticAllocator) +
        slot_count * (@sizeOf(Slot) + rbuf_bytes + sbuf_bytes + body_small_bytes) +
        body_large_count * body_large_bytes + transfer_count * transfer_bytes + gunzip_count * @sizeOf(Gunzip) +
        stream.ring_bytes;

    pub fn start(gpa: std.mem.Allocator, io: std.Io, rt: *runtime.Runtime, config: Config) !*HttpServer {
        const static = try gpa.create(bounded.StaticAllocator);
        errdefer gpa.destroy(static);
        static.* = .{ .parent = gpa };
        const a = static.allocator();

        const self = try a.create(HttpServer);
        errdefer a.destroy(self);
        const backend = try a.create(Backend);
        errdefer a.destroy(backend);
        backend.* = try Backend.init();
        errdefer backend.deinit();
        const slots = try a.alloc(Slot, slot_count);
        errdefer a.free(slots);
        const memory = try a.alloc(u8, slot_count * (rbuf_bytes + sbuf_bytes + body_small_bytes));
        errdefer a.free(memory);
        const transfers = try a.alloc(u8, transfer_count * transfer_bytes);
        errdefer a.free(transfers);
        const gunzips = try a.alloc(Gunzip, gunzip_count);
        errdefer a.free(gunzips);
        const proxies = try a.alloc(std.Io.net.IpAddress, config.trusted_proxies.len);
        errdefer a.free(proxies);
        for (config.trusted_proxies, 0..) |text, i| proxies[i] = try std.Io.net.IpAddress.parse(text, 0);

        self.* = .{
            .gpa = gpa,
            .io = io,
            .rt = rt,
            .static = static,
            .backend = backend,
            .proxies = proxies,
            .slots = slots,
            .memory = memory,
            .transfers = transfers,
            .gunzips = gunzips,
        };
        var large_made: usize = 0;
        errdefer for (self.large[0..large_made]) |buffer| a.free(buffer);
        while (large_made < body_large_count) : (large_made += 1) self.large[large_made] = try a.alloc(u8, body_large_bytes);

        const per_slot = rbuf_bytes + sbuf_bytes + body_small_bytes;
        for (slots, 0..) |*slot, i| {
            const base = i * per_slot;
            slot.* = .{
                .server = self,
                .rbuf = memory[base..][0..rbuf_bytes],
                .sbuf = memory[base + rbuf_bytes ..][0..sbuf_bytes],
                .body_small = memory[base + rbuf_bytes + sbuf_bytes ..][0..body_small_bytes],
            };
            slot.recv_op = .{ .context = slot, .callback = on_recv };
            slot.send_op = .{ .context = slot, .callback = on_send };
            slot.close_op = .{ .context = slot, .callback = on_close };
        }

        const addresses = [listeners_max]?[]const u8{ config.primary, config.proxy };
        errdefer for (self.listeners[0..self.listener_count]) |l| {
            Backend.close_now(l.socket);
            a.free(l.host);
        };
        for (addresses, 0..) |maybe, index| {
            const text = maybe orelse continue;
            const hp = parse_host_port(text, 7470);
            const address = try std.Io.net.IpAddress.parse(hp.host, hp.port);
            const endpoint: netio.Endpoint = switch (address) {
                .ip4 => |v4| blk: {
                    var bytes: [16]u8 = @splat(0);
                    @memcpy(bytes[0..4], &v4.bytes);
                    break :blk .{ .address = bytes, .port = v4.port, .family = .ipv4 };
                },
                .ip6 => |v6| .{ .address = v6.bytes, .port = v6.port, .family = .ipv6 },
            };
            const bound = try Backend.listen(endpoint, backlog);
            errdefer Backend.close_now(bound.socket);
            const dup_host = try a.dupe(u8, hp.host);
            const l = &self.listeners[self.listener_count];
            l.* = .{
                .server = self,
                .index = @intCast(self.listener_count),
                .socket = bound.socket,
                .port = bound.port,
                .host = dup_host,
                .first_slot = index * slots_per_listener,
                .filtered = index == 1,
                .op = undefined,
            };
            l.op = .{ .context = l, .callback = on_accept };
            self.listener_count += 1;
        }

        rt.stream_hub.set_waker(backend, wake_backend);
        errdefer rt.stream_hub.set_waker(null, null);
        rt.set_pool_source(self, read_pools);
        errdefer rt.set_pool_source(null, null);
        tiger.assert(reserved_bytes <= 64 * 1024 * 1024);
        static.freeze();

        try self.regular.start(self);
        errdefer self.regular.stop(io);
        try self.long.start(self);
        errdefer self.long.stop(io);
        try self.reader.start(self);
        errdefer self.reader.stop(io);
        self.loop_thread = try std.Thread.spawn(.{}, run, .{self});
        return self;
    }

    pub fn listen_port(self: *const HttpServer, index: usize) u16 {
        return self.listeners[index].port;
    }

    pub fn host(self: *const HttpServer, index: usize) []const u8 {
        return self.listeners[index].host;
    }

    pub fn has_listener(self: *const HttpServer, index: usize) bool {
        return index < self.listener_count;
    }

    /// True when a worker did not finish within shutdown_wait_ms: nothing was freed, and the caller must end
    /// the process without destroying anything a worker can still touch (the runtime).
    pub fn stop_and_destroy(self: *HttpServer) bool {
        self.stopping.store(true, .release);
        self.rt.stream_hub.wake_all();
        self.stop_requested.store(true, .release);
        self.backend.wake();
        if (self.loop_thread) |t| t.join();
        if (self.abandoned) return true;
        self.rt.stream_hub.set_waker(null, null);
        self.rt.set_pool_source(null, null);
        self.regular.stop(self.io);
        self.long.stop(self.io);
        self.reader.stop(self.io);

        const static = self.static;
        const gpa = self.gpa;
        const a = static.allocator();
        static.begin_deinit();
        for (self.listeners[0..self.listener_count]) |l| a.free(l.host);
        for (self.large) |buffer| a.free(buffer);
        a.free(self.proxies);
        a.free(self.gunzips);
        a.free(self.transfers);
        a.free(self.memory);
        a.free(self.slots);
        const backend = self.backend;
        backend.deinit();
        a.destroy(backend);
        a.destroy(self);
        static.finish_deinit();
        gpa.destroy(static);
        return false;
    }

    fn run(self: *HttpServer) void {
        var stopping = false;
        var stop_began: u64 = 0;
        while (true) {
            self.now = self.backend.now();
            if (!stopping and self.stop_requested.load(.acquire)) {
                stopping = true;
                stop_began = self.now;
            }
            self.turn(stopping);
            if (stopping and self.drained()) break;
            if (stopping and self.now - stop_began >= shutdown_wait_ms) {
                self.abandon();
                return;
            }
            self.backend.poll(bounded.tick_ms) catch |err| std.debug.panic("http loop: poll failed: {s}", .{@errorName(err)});
        }
        tiger.assert(self.backend.pending_count == 0);
    }

    /// A worker stuck in blocking work that no shutdown path can cancel must not hold the hub's exit.
    /// The server's memory and threads are left as they are (the process is ending), so the worker
    /// never touches freed slots.
    fn abandon(self: *HttpServer) void {
        for (self.slots) |*slot| {
            if (!slot.job_inflight) continue;
            const stuck: []const u8 = if (slot.job_kind == .read or slot.job_kind == .snapshot) "(stream or download piece)" else slot.path;
            std.debug.print("http: shutdown gave up after {d} ms waiting for a worker still running {s} {s}\n", .{ shutdown_wait_ms, @tagName(slot.job_kind), stuck });
        }
        self.abandoned = true;
    }

    fn drained(self: *const HttpServer) bool {
        for (self.listeners[0..self.listener_count]) |l| {
            if (l.accepting or !l.closed) return false;
        }
        for (self.slots) |*slot| if (slot.mode != .free) return false;
        return true;
    }

    fn turn(self: *HttpServer, stopping: bool) void {
        self.guard.begin(self.backend);
        self.take_completions();
        self.guard.mark(self.backend, "completions");
        for (self.listeners[0..self.listener_count]) |*l| self.service_listener(l, stopping);
        self.guard.mark(self.backend, "listeners");
        var open: usize = 0;
        var lagging: usize = 0;
        for (self.slots) |*slot| {
            if (stopping and slot.mode != .free and slot.mode != .closing) self.begin_close(slot);
            const mode = slot.mode;
            self.service(slot);
            self.guard.mark(self.backend, @tagName(mode));
            if (slot.mode != .free) open += 1;
            if (slot.mode == .ws and (slot.ws_stage == .resync_wait or slot.ws_stage == .resync_ready)) lagging += 1;
        }
        self.gauges.slots.set(open);
        self.gauges.lagged.set(lagging);
        self.guard.end(self.backend, "gauges");
    }

    fn read_pools(context: *const anyopaque, out: *std.ArrayList(gauge.Reading), arena: std.mem.Allocator) std.mem.Allocator.Error!void {
        const self: *const HttpServer = @ptrCast(@alignCast(context));
        try out.append(arena, self.gauges.slots.read("http_slots"));
        try out.append(arena, self.gauges.large.read("http_large_bodies"));
        try out.append(arena, self.gauges.transfers.read("http_transfers"));
        try out.append(arena, self.regular.gauge.read("http_workers_regular"));
        try out.append(arena, self.long.gauge.read("http_workers_long"));
        try out.append(arena, self.reader.gauge.read("http_workers_reader"));
        try out.append(arena, self.gauges.lagged.read("stream_lagged_readers"));
    }

    fn service_listener(self: *HttpServer, l: *Listener, stopping: bool) void {
        if (stopping) {
            if (l.accepting) {
                self.backend.cancel(&l.op);
            } else if (!l.closed) {
                Backend.close_now(l.socket);
                l.closed = true;
            }
            return;
        }
        if (l.accepting or self.now < l.retry_at) return;
        self.backend.submit(&l.op, .{ .accept = l.socket }) catch {
            l.retry_at = self.now + accept_retry_ms;
            return;
        };
        l.accepting = true;
    }

    fn on_accept(context: *anyopaque, _: *netio.Operation, result: netio.Result) void {
        const l: *Listener = @ptrCast(@alignCast(context));
        const self = l.server;
        l.accepting = false;
        switch (result) {
            .accepted => |sock| self.admit(l, sock),
            .failed => |err| {
                if (err != error.Cancelled) l.retry_at = self.now + accept_retry_ms;
            },
            else => unreachable,
        }
    }

    fn admit(self: *HttpServer, l: *Listener, sock: netio.Socket) void {
        if (self.stopping.load(.acquire)) {
            Backend.close_now(sock);
            return;
        }
        if (l.filtered and self.proxies.len > 0) {
            const peer = peer_address(sock) orelse {
                Backend.close_now(sock);
                return;
            };
            if (!ip_matches_any(peer, self.proxies)) {
                Backend.close_now(sock);
                return;
            }
        }
        const slot = for (self.slots[l.first_slot..][0..slots_per_listener]) |*candidate| {
            if (candidate.mode == .free) break candidate;
        } else {
            if (!l.refusing) {
                l.refusing = true;
                std.debug.print("http: {d} connections open on listener {d}, answering 503 to new ones\n", .{ slots_per_listener, l.index });
            }
            self.gauges.slots.refuse();
            Backend.send_now(sock, busy_response);
            Backend.shutdown_write(sock);
            Backend.close_now(sock);
            return;
        };
        l.refusing = false;
        self.reset(slot, .head);
        slot.listener = l.index;
        slot.sock = sock;
        slot.sock_open = true;
        slot.deadline = self.now + head_timeout_ms;
    }

    fn reset(self: *HttpServer, slot: *Slot, mode: Mode) void {
        tiger.assert(!slot.recv_inflight and !slot.send_inflight and !slot.close_inflight and !slot.job_inflight);
        const next_generation = if (mode == .free) slot.generation +% 1 else slot.generation;
        const fresh: Slot = .{
            .server = self,
            .generation = next_generation,
            .mode = mode,
            .recv_op = slot.recv_op,
            .send_op = slot.send_op,
            .close_op = slot.close_op,
            .rbuf = slot.rbuf,
            .sbuf = slot.sbuf,
            .body_small = slot.body_small,
        };
        slot.* = fresh;
    }

    fn slot_index(self: *const HttpServer, slot: *const Slot) u16 {
        return @intCast((@intFromPtr(slot) - @intFromPtr(self.slots.ptr)) / @sizeOf(Slot));
    }

    fn service(self: *HttpServer, slot: *Slot) void {
        switch (slot.mode) {
            .free => {},
            .closing => self.progress_close(slot),
            else => {
                if (slot.deadline != 0 and self.now >= slot.deadline) return self.begin_close(slot);
                if (slot.hard_deadline != 0 and self.now >= slot.hard_deadline) return self.begin_close(slot);
                switch (slot.mode) {
                    .head => self.service_head(slot),
                    .body => self.service_body(slot),
                    .working => {},
                    .responding => self.pump_response(slot),
                    .ws => self.service_ws(slot),
                    .draining => self.service_draining(slot),
                    .free, .closing => unreachable,
                }
            },
        }
    }

    fn begin_close(self: *HttpServer, slot: *Slot) void {
        if (slot.mode == .closing or slot.mode == .free) return;
        slot.mode = .closing;
        slot.deadline = 0;
        slot.hard_deadline = 0;
        if (slot.recv_inflight) self.backend.cancel(&slot.recv_op);
        if (slot.send_inflight) self.backend.cancel(&slot.send_op);
    }

    fn progress_close(self: *HttpServer, slot: *Slot) void {
        if (slot.recv_inflight or slot.send_inflight or slot.job_inflight or slot.close_inflight) return;
        if (!slot.sock_open) return self.reset(slot, .free);
        self.release_resources(slot);
        self.backend.submit(&slot.close_op, .{ .close = slot.sock }) catch {
            Backend.close_now(slot.sock);
            slot.sock_open = false;
            return self.reset(slot, .free);
        };
        slot.close_inflight = true;
    }

    fn on_close(context: *anyopaque, _: *netio.Operation, _: netio.Result) void {
        const slot: *Slot = @ptrCast(@alignCast(context));
        slot.close_inflight = false;
        slot.sock_open = false;
        slot.server.reset(slot, .free);
    }

    fn release_body(self: *HttpServer, slot: *Slot) void {
        if (slot.body_large) |index| {
            self.large_used[index] = false;
            slot.body_large = null;
            self.gauges.large.set(count_used(&self.large_used));
        }
        slot.body = &.{};
        slot.body_len = 0;
    }

    fn release_source(self: *HttpServer, slot: *Slot) void {
        if (slot.mem_owned) self.gpa.free(slot.mem_data);
        slot.mem_owned = false;
        slot.mem_data = &.{};
        slot.mem_off = 0;
        if (slot.gunzip) |index| self.gunzip_used[index] = false;
        slot.gunzip = null;
        slot.gunzip_done = false;
        if (slot.file) |file| file.close(self.io);
        slot.file = null;
        slot.file_reading = false;
        if (slot.transfer) |index| {
            self.transfer_used[index] = false;
            self.gauges.transfers.set(count_used(&self.transfer_used));
        }
        slot.transfer = null;
        slot.src = .none;
    }

    fn release_resources(self: *HttpServer, slot: *Slot) void {
        self.release_body(slot);
        self.release_source(slot);
        if (slot.ws_snapshot.len > 0) self.gpa.free(slot.ws_snapshot);
        if (slot.ws_live.len > 0) self.gpa.free(slot.ws_live);
        slot.ws_snapshot = &.{};
        slot.ws_live = &.{};
    }

    fn post_recv(self: *HttpServer, slot: *Slot) void {
        if (slot.recv_inflight or slot.rlen == slot.rbuf.len) return;
        self.submit_recv(slot, .head, slot.rbuf[slot.rlen..]);
    }

    fn submit_recv(self: *HttpServer, slot: *Slot, target: RecvTarget, buffer: []u8) void {
        tiger.assert(!slot.recv_inflight and buffer.len > 0);
        self.backend.submit(&slot.recv_op, .{ .recv = .{ .socket = slot.sock, .buffer = buffer } }) catch return self.begin_close(slot);
        slot.recv_target = target;
        slot.recv_inflight = true;
    }

    fn post_send(self: *HttpServer, slot: *Slot) void {
        if (slot.send_inflight or slot.send_slice.len == 0) return;
        self.backend.submit(&slot.send_op, .{ .send = .{ .socket = slot.sock, .buffer = slot.send_slice } }) catch return self.begin_close(slot);
        slot.send_inflight = true;
        slot.deadline = self.now + send_timeout_ms;
    }

    fn on_recv(context: *anyopaque, _: *netio.Operation, result: netio.Result) void {
        const slot: *Slot = @ptrCast(@alignCast(context));
        const self = slot.server;
        slot.recv_inflight = false;
        switch (result) {
            .received => |n| {
                if (n == 0) return self.begin_close(slot);
                switch (slot.mode) {
                    .closing => {},
                    .draining => {
                        slot.drained += n;
                        slot.rlen = 0;
                    },
                    .body => {
                        switch (slot.recv_target) {
                            .body => slot.body_len += n,
                            .stage => slot.stage_len += n,
                            .head => slot.rlen += n,
                        }
                        slot.deadline = self.now + body_progress_ms;
                    },
                    else => slot.rlen += n,
                }
            },
            .failed => self.begin_close(slot),
            else => unreachable,
        }
    }

    fn on_send(context: *anyopaque, _: *netio.Operation, result: netio.Result) void {
        const slot: *Slot = @ptrCast(@alignCast(context));
        const self = slot.server;
        slot.send_inflight = false;
        switch (result) {
            .sent => |n| {
                slot.send_slice = slot.send_slice[n..];
                if (n > 0 and (slot.mode == .responding or slot.mode == .ws)) slot.deadline = self.now + send_timeout_ms;
            },
            .failed => self.begin_close(slot),
            else => unreachable,
        }
    }

    fn service_head(self: *HttpServer, slot: *Slot) void {
        if (slot.rlen == 0) return self.post_recv(slot);
        if (slot.hard_deadline == 0) {
            slot.hard_deadline = self.now + head_timeout_ms;
            slot.deadline = 0;
        }
        if (wire.find_head_end(slot.rbuf[0..slot.rlen], slot.scanned)) |end| {
            slot.req = wire.parse_head(slot.rbuf[0..end]) catch |err| {
                return switch (err) {
                    error.BadRequest => self.respond_error(slot, 400, "bad request"),
                    error.VersionNotSupported => self.respond_error(slot, 505, "http version not supported"),
                    error.NotImplemented => self.respond_error(slot, 501, "transfer encoding not supported"),
                };
            };
            slot.rpos = end;
            slot.deadline = 0;
            slot.hard_deadline = 0;
            self.route(slot);
        } else {
            slot.scanned = slot.rlen;
            if (slot.rlen == slot.rbuf.len) return self.respond_error(slot, 431, "request header fields too large");
            self.post_recv(slot);
        }
    }

    fn route(self: *HttpServer, slot: *Slot) void {
        const req = &slot.req;
        const target = req.target;
        const mark = std.mem.findScalar(u8, target, '?');
        slot.path = if (mark) |i| target[0..i] else target;
        slot.query = if (mark) |i| target[i + 1 ..] else "";
        slot.head_only = req.method == .head;
        slot.close_after = !req.persistent();
        const path = slot.path;
        const is_get = req.method == .get or req.method == .head;

        if (req.method == .post and std.mem.startsWith(u8, path, "/api/")) return self.begin_body(slot);
        if (req.chunked or (req.content_length orelse 0) > 0) slot.close_after = true;

        if (req.method == .options) return self.respond(slot, 204, null, "", false, cors_preflight);
        if (is_get and std.mem.eql(u8, path, "/health")) {
            var buffer: [128]u8 = undefined;
            const body = std.fmt.bufPrint(&buffer, "{{\"ok\":true,\"version\":\"{s}\"}}", .{version.version}) catch unreachable;
            return self.respond(slot, 200, json_type, body, false, "");
        }
        if (req.method == .get and std.mem.eql(u8, path, "/api/events")) return self.submit_job(slot, .events);
        if (req.method == .get and std.mem.startsWith(u8, path, "/api/client/")) return self.submit_job(slot, .client);
        if (std.mem.startsWith(u8, path, "/api/")) return self.respond(slot, 404, json_type, api_missing_body, false, "");
        if (is_get) return self.serve_asset(slot, path);
        self.respond(slot, 404, text_type, "not found", false, "");
    }

    fn serve_asset(self: *HttpServer, slot: *Slot, path: []const u8) void {
        const asset = web_assets.find(path) orelse web_assets.find("index.html") orelse
            return self.respond(slot, 404, text_type, "not found", false, "");
        if (!asset.gzipped) return self.respond(slot, 200, asset.content_type, asset.bytes, false, "");
        if (slot.req.accept_gzip) {
            return self.respond(slot, 200, asset.content_type, asset.bytes, false, "content-encoding: gzip\r\nvary: Accept-Encoding\r\n");
        }
        const index = for (&self.gunzip_used, 0..) |used, i| {
            if (!used) break i;
        } else return self.respond_error(slot, 503, busy_body);
        self.gunzip_used[index] = true;
        const g = &self.gunzips[index];
        g.input = .fixed(asset.bytes);
        g.decoder = .init(&g.input, .gzip, &g.window);
        slot.gunzip = @intCast(index);
        slot.gunzip_done = false;
        self.stage_head(slot, 200, asset.content_type, null, true, "vary: Accept-Encoding\r\n");
        slot.src = if (slot.head_only) .none else .gunzip;
        self.start_response(slot);
    }

    fn stage_head(self: *HttpServer, slot: *Slot, status: u16, content_type: ?[]const u8, length: ?u64, chunked: bool, extra: []const u8) void {
        _ = self;
        var w: std.Io.Writer = .fixed(slot.sbuf);
        const phrase = @as(std.http.Status, @fromBackingInt(@intCast(status))).phrase() orelse "";
        w.print("HTTP/1.1 {d} {s}\r\n", .{ status, phrase }) catch unreachable;
        if (content_type) |ct| w.print("content-type: {s}\r\n", .{ct}) catch unreachable;
        if (length) |n| w.print("content-length: {d}\r\n", .{n}) catch unreachable;
        if (chunked) w.writeAll("transfer-encoding: chunked\r\n") catch unreachable;
        w.writeAll("access-control-allow-origin: *\r\n") catch unreachable;
        w.writeAll(extra) catch unreachable;
        if (slot.close_after) w.writeAll("connection: close\r\n") catch unreachable;
        w.writeAll("\r\n") catch unreachable;
        slot.slen = w.end;
    }

    fn start_response(self: *HttpServer, slot: *Slot) void {
        slot.mode = .responding;
        slot.send_slice = slot.sbuf[0..slot.slen];
        slot.slen = 0;
        slot.deadline = self.now + send_timeout_ms;
        slot.hard_deadline = 0;
        self.pump_response(slot);
    }

    fn respond(self: *HttpServer, slot: *Slot, status: u16, content_type: ?[]const u8, body: []const u8, owned: bool, extra: []const u8) void {
        const bodiless = status == 204 or slot.head_only;
        self.stage_head(slot, status, content_type, if (status == 204) null else body.len, false, extra);
        slot.src = .none;
        if (bodiless) {
            if (owned) self.gpa.free(body);
        } else if (body.len <= slot.sbuf.len - slot.slen) {
            @memcpy(slot.sbuf[slot.slen..][0..body.len], body);
            slot.slen += body.len;
            if (owned) self.gpa.free(body);
        } else {
            slot.src = .mem;
            slot.mem_data = body;
            slot.mem_off = 0;
            slot.mem_owned = owned;
        }
        self.start_response(slot);
    }

    fn respond_error(self: *HttpServer, slot: *Slot, status: u16, text: []const u8) void {
        slot.close_after = true;
        slot.head_only = false;
        self.respond(slot, status, text_type, text, false, "");
    }

    fn pump_response(self: *HttpServer, slot: *Slot) void {
        if (slot.send_inflight) return;
        if (slot.send_slice.len > 0) return self.post_send(slot);
        switch (slot.src) {
            .none => self.finish_response(slot),
            .mem => {
                if (slot.mem_off >= slot.mem_data.len) return self.finish_response(slot);
                slot.send_slice = slot.mem_data[slot.mem_off..];
                slot.mem_off = slot.mem_data.len;
                self.post_send(slot);
            },
            .gunzip => self.step_gunzip(slot),
            .file => self.step_file(slot),
        }
    }

    fn step_gunzip(self: *HttpServer, slot: *Slot) void {
        if (slot.gunzip_done) return self.finish_response(slot);
        const g = &self.gunzips[slot.gunzip.?];
        const data = slot.sbuf[gunzip_front..][0..gunzip_piece];
        const n = g.decoder.reader.readSliceShort(data) catch return self.begin_close(slot);
        if (n == 0) {
            slot.gunzip_done = true;
            const tail = "0\r\n\r\n";
            @memcpy(slot.sbuf[0..tail.len], tail);
            slot.send_slice = slot.sbuf[0..tail.len];
        } else {
            var line: [gunzip_front]u8 = undefined;
            const hex = std.fmt.bufPrint(&line, "{x}\r\n", .{n}) catch unreachable;
            const first = gunzip_front - hex.len;
            @memcpy(slot.sbuf[first..gunzip_front], hex);
            slot.sbuf[gunzip_front + n] = '\r';
            slot.sbuf[gunzip_front + n + 1] = '\n';
            slot.send_slice = slot.sbuf[first .. gunzip_front + n + 2];
        }
        self.post_send(slot);
    }

    fn step_file(self: *HttpServer, slot: *Slot) void {
        if (slot.file_reading) return;
        if (slot.file_off >= slot.file_size) return self.finish_response(slot);
        slot.file_reading = true;
        if (!self.submit_aux(slot, .read)) slot.file_reading = false;
    }

    fn take_transfer(self: *HttpServer) ?u8 {
        for (&self.transfer_used, 0..) |*used, i| {
            if (used.*) continue;
            used.* = true;
            self.gauges.transfers.set(count_used(&self.transfer_used));
            return @intCast(i);
        }
        self.gauges.transfers.refuse();
        return null;
    }

    fn begin_download(self: *HttpServer, slot: *Slot, file: std.Io.File, size: u64, content_type: []const u8) void {
        const transfer = self.take_transfer() orelse {
            file.close(self.io);
            return self.respond(slot, 503, text_type, busy_body, false, "retry-after: 1\r\n");
        };
        slot.file = file;
        slot.file_size = size;
        slot.file_off = 0;
        slot.file_reading = false;
        slot.transfer = transfer;
        self.stage_head(slot, 200, content_type, size, false, "");
        slot.src = .file;
        self.start_response(slot);
    }

    fn finish_response(self: *HttpServer, slot: *Slot) void {
        self.release_source(slot);
        slot.requests_done += 1;
        if (slot.close_after) {
            Backend.shutdown_write(slot.sock);
            slot.mode = .draining;
            slot.rlen = 0;
            slot.drained = 0;
            slot.deadline = self.now + linger_ms;
            slot.hard_deadline = 0;
            return;
        }
        const left = slot.rlen - slot.rpos;
        std.mem.copyForwards(u8, slot.rbuf[0..left], slot.rbuf[slot.rpos..slot.rlen]);
        slot.rlen = left;
        slot.rpos = 0;
        slot.scanned = 0;
        slot.mode = .head;
        slot.head_only = false;
        slot.hard_deadline = 0;
        slot.deadline = self.now + idle_timeout_ms;
        self.service_head(slot);
    }

    fn service_draining(self: *HttpServer, slot: *Slot) void {
        if (slot.drained > linger_bytes_max) return self.begin_close(slot);
        slot.rlen = 0;
        self.post_recv(slot);
    }

    fn begin_body(self: *HttpServer, slot: *Slot) void {
        const req = &slot.req;
        slot.body_len = 0;
        slot.chunk = .{};
        slot.body = slot.body_small;
        slot.body_large = null;
        if (req.content_length) |length| {
            if (length > body_large_bytes) return self.respond_error(slot, 413, "request body too large");
            if (length > slot.body_small.len and !self.take_large(slot)) return self.respond_error(slot, 503, busy_body);
        }
        slot.mode = .body;
        slot.deadline = self.now + body_progress_ms;
        slot.hard_deadline = self.now + body_total_ms;
        if (req.expect_continue) {
            slot.send_slice = "HTTP/1.1 100 Continue\r\n\r\n";
            self.post_send(slot);
        }
        self.service_body(slot);
    }

    fn take_large(self: *HttpServer, slot: *Slot) bool {
        for (&self.large_used, 0..) |*used, i| {
            if (used.*) continue;
            used.* = true;
            slot.body_large = @intCast(i);
            slot.body = self.large[i];
            self.gauges.large.set(count_used(&self.large_used));
            return true;
        }
        self.gauges.large.refuse();
        return false;
    }

    fn service_body(self: *HttpServer, slot: *Slot) void {
        if (slot.send_slice.len > 0) return self.post_send(slot);
        if (slot.recv_inflight) return;
        while (true) {
            if (!slot.req.chunked) {
                const length: usize = @intCast(slot.req.content_length orelse 0);
                if (slot.body_len == length) return self.body_complete(slot);
                if (slot.rpos < slot.rlen) {
                    const n = @min(slot.rlen - slot.rpos, length - slot.body_len);
                    @memcpy(slot.body[slot.body_len..][0..n], slot.rbuf[slot.rpos..][0..n]);
                    slot.body_len += n;
                    slot.rpos += n;
                    continue;
                }
                return self.submit_recv(slot, .body, slot.body[slot.body_len..length]);
            }
            const from_head = slot.rpos < slot.rlen;
            const input = if (from_head) slot.rbuf[slot.rpos..slot.rlen] else slot.sbuf[0..slot.stage_len];
            if (input.len == 0) return self.submit_recv(slot, .stage, slot.sbuf);
            const step = slot.chunk.feed(input, slot.body[slot.body_len..]) catch return self.respond_error(slot, 400, "bad chunked body");
            slot.body_len += step.written;
            if (from_head) {
                slot.rpos += step.consumed;
            } else {
                std.mem.copyForwards(u8, slot.sbuf[0 .. slot.stage_len - step.consumed], slot.sbuf[step.consumed..slot.stage_len]);
                slot.stage_len -= step.consumed;
            }
            switch (step.status) {
                .done => {
                    if (!from_head and slot.stage_len > 0) slot.close_after = true;
                    slot.stage_len = 0;
                    return self.body_complete(slot);
                },
                .full => {
                    if (slot.body_large != null) return self.respond_error(slot, 413, "request body too large");
                    const kept = slot.body_len;
                    const small = slot.body_small;
                    if (!self.take_large(slot)) return self.respond_error(slot, 503, busy_body);
                    @memcpy(slot.body[0..kept], small[0..kept]);
                },
                .more => {},
            }
        }
    }

    fn body_complete(self: *HttpServer, slot: *Slot) void {
        const name = slot.path["/api/".len..];
        self.submit_job(slot, if (is_long_method(name)) .rpc_long else .rpc);
    }

    fn submit_job(self: *HttpServer, slot: *Slot, kind: JobKind) void {
        slot.job_kind = kind;
        slot.job_inflight = true;
        slot.mode = .working;
        slot.deadline = 0;
        slot.hard_deadline = 0;
        const job: Job = .{ .slot = self.slot_index(slot), .generation = slot.generation, .kind = kind };
        const accepted = switch (kind) {
            .rpc_long => self.long.submit(self.io, job),
            else => self.regular.submit(self.io, job),
        };
        if (accepted) return;
        slot.job_inflight = false;
        self.release_body(slot);
        self.respond(slot, 503, text_type, busy_body, false, "retry-after: 1\r\n");
    }

    fn submit_aux(self: *HttpServer, slot: *Slot, kind: JobKind) bool {
        tiger.assert(!slot.job_inflight);
        slot.job_kind = kind;
        const job: Job = .{ .slot = self.slot_index(slot), .generation = slot.generation, .kind = kind };
        const accepted = switch (kind) {
            .read => self.reader.submit(self.io, job),
            else => self.regular.submit(self.io, job),
        };
        if (accepted) slot.job_inflight = true;
        return accepted;
    }

    fn execute(self: *HttpServer, job: Job) void {
        const slot = &self.slots[job.slot];
        var arena_state = std.heap.ArenaAllocator.init(self.gpa);
        defer arena_state.deinit();
        const arena = arena_state.allocator();
        const done: Done = if (self.stopping.load(.acquire)) .failed else switch (job.kind) {
            .rpc, .rpc_long => self.run_rpc(slot),
            .events => self.run_events(slot, arena),
            .snapshot => self.build_events(arena) catch .failed,
            .client => self.run_client(slot, arena),
            .read => self.run_read(slot),
        };
        self.complete(job, done);
    }

    fn complete(self: *HttpServer, job: Job, done: Done) void {
        self.cmu.lockUncancelable(self.io);
        self.completions.push(.{ .slot = job.slot, .generation = job.generation, .done = done }) catch unreachable;
        self.cmu.unlock(self.io);
        self.backend.wake();
    }

    fn canned(status: u16, content_type: []const u8, body: []const u8) Done {
        return .{ .reply = .{ .status = status, .content_type = content_type, .body = body, .owned = false } };
    }

    fn principal_of(self: *HttpServer, slot: *const Slot) ?home.Principal {
        const token = token_of(slot) orelse return null;
        return self.rt.hub_home.authorize(token);
    }

    fn run_rpc(self: *HttpServer, slot: *Slot) Done {
        const principal = self.principal_of(slot);
        const name = slot.path["/api/".len..];
        const result = api_dispatch.dispatch(self.rt, principal, name, slot.body[0..slot.body_len]) catch
            return canned(500, json_type, dispatch_failed_body);
        return .{ .reply = .{ .status = result.status, .content_type = json_type, .body = result.body_json, .owned = true } };
    }

    fn run_events(self: *HttpServer, slot: *Slot, arena: std.mem.Allocator) Done {
        if (self.principal_of(slot) == null) return canned(401, json_type, unauthorized_body);
        if (slot.req.websocket_key() == null) return canned(400, text_type, "upgrade required");
        if (slot.req.origin) |origin| {
            if (slot.req.duplicate_origin_or_host or !origin_matches_host(origin, slot.req.host)) return canned(403, text_type, "forbidden");
        }
        return self.build_events(arena) catch .failed;
    }

    fn build_events(self: *HttpServer, arena: std.mem.Allocator) !Done {
        const state_snap = try self.rt.view_state(arena);
        const snapshot = try schema.encode_alloc(self.gpa, schema.HubStreamMessage{ .snapshot = .{
            .seq = state_snap.seq,
            .state = state_snap,
        } });
        errdefer self.gpa.free(snapshot);
        const live = try schema.encode_alloc(self.gpa, schema.HubStreamMessage{ .live = .{ .seq = state_snap.seq } });
        return .{ .events = .{ .snapshot = snapshot, .live = live, .seq = state_snap.seq } };
    }

    fn run_client(self: *HttpServer, slot: *Slot, arena: std.mem.Allocator) Done {
        if (self.principal_of(slot) == null) return canned(401, json_type, unauthorized_body);
        const name = slot.path["/api/client/".len..];
        const asset_path = (self.rt.resolve_client_asset_path(arena, name) catch null) orelse return canned(404, json_type, asset_missing_body);
        const file = std.Io.Dir.cwd().openFile(self.io, asset_path, .{}) catch return canned(404, json_type, asset_missing_body);
        const st = file.stat(self.io) catch {
            file.close(self.io);
            return .failed;
        };
        const content_type: []const u8 = if (std.mem.endsWith(u8, name, ".apk"))
            "application/vnd.android.package-archive"
        else if (std.mem.endsWith(u8, name, ".zip"))
            "application/zip"
        else
            "application/octet-stream";
        return .{ .file = .{ .file = file, .size = st.size, .content_type = content_type } };
    }

    fn run_read(self: *HttpServer, slot: *Slot) Done {
        const buffer = self.transfers[@as(usize, slot.transfer.?) * transfer_bytes ..][0..transfer_bytes];
        const want: usize = @intCast(@min(@as(u64, transfer_bytes), slot.file_size - slot.file_off));
        const got = slot.file.?.readPositionalAll(self.io, buffer[0..want], slot.file_off) catch return .failed;
        return .{ .piece = got };
    }

    fn take_completions(self: *HttpServer) void {
        while (true) {
            self.cmu.lockUncancelable(self.io);
            const next = self.completions.pop();
            self.cmu.unlock(self.io);
            const item = next orelse return;
            const slot = &self.slots[item.slot];
            tiger.assert(slot.generation == item.generation and slot.job_inflight);
            slot.job_inflight = false;
            self.apply(slot, item.done);
        }
    }

    fn discard(self: *HttpServer, done: Done) void {
        switch (done) {
            .reply => |r| if (r.owned) self.gpa.free(r.body),
            .events => |e| {
                self.gpa.free(e.snapshot);
                self.gpa.free(e.live);
            },
            .file => |f| f.file.close(self.io),
            .piece, .failed => {},
        }
    }

    fn apply(self: *HttpServer, slot: *Slot, done: Done) void {
        const kind = slot.job_kind;
        if (kind == .rpc or kind == .rpc_long) self.release_body(slot);
        if (slot.mode == .closing) return self.discard(done);
        switch (kind) {
            .rpc, .rpc_long, .events, .client => switch (done) {
                .reply => |r| self.respond(slot, r.status, r.content_type, r.body, r.owned, ""),
                .events => |e| self.begin_ws(slot, e.snapshot, e.live, e.seq),
                .file => |f| self.begin_download(slot, f.file, f.size, f.content_type),
                .failed => self.respond(slot, 500, json_type, dispatch_failed_body, false, ""),
                .piece => unreachable,
            },
            .snapshot => switch (done) {
                .events => |e| self.resync_ready(slot, e.snapshot, e.live, e.seq),
                .failed => self.begin_close(slot),
                else => unreachable,
            },
            .read => switch (done) {
                .piece => |n| {
                    slot.file_reading = false;
                    if (n == 0) return self.begin_close(slot);
                    slot.file_off += n;
                    slot.send_slice = self.transfers[@as(usize, slot.transfer.?) * transfer_bytes ..][0..n];
                    self.pump_response(slot);
                },
                .failed => self.begin_close(slot),
                else => unreachable,
            },
        }
    }

    fn stage_frame(self: *HttpServer, slot: *Slot, at: usize, opcode: wire.Opcode, payload: []const u8) usize {
        _ = self;
        var header: [10]u8 = undefined;
        const h = wire.ws_server_header(&header, opcode, payload.len);
        @memcpy(slot.sbuf[at..][0..h.len], h);
        @memcpy(slot.sbuf[at + h.len ..][0..payload.len], payload);
        return h.len + payload.len;
    }

    fn begin_ws(self: *HttpServer, slot: *Slot, snapshot: []u8, live: []u8, seq: u64) void {
        var accept: [28]u8 = undefined;
        wire.ws_accept(slot.req.ws_key.?, &accept);
        var w: std.Io.Writer = .fixed(slot.sbuf);
        w.print("HTTP/1.1 101 Switching Protocols\r\nupgrade: websocket\r\nconnection: Upgrade\r\nsec-websocket-accept: {s}\r\n\r\n", .{accept}) catch unreachable;
        slot.slen = w.end;
        const left = slot.rlen - slot.rpos;
        std.mem.copyForwards(u8, slot.rbuf[0..left], slot.rbuf[slot.rpos..slot.rlen]);
        slot.rlen = left;
        slot.rpos = 0;
        slot.close_after = false;
        slot.mode = .ws;
        slot.ws_in = .{};
        slot.out = .{ .ping_at = self.now + ws_ping_ms };
        slot.ws_resyncs = 0;
        self.stage_snapshot(slot, snapshot, live, seq);
        self.pump_ws(slot);
    }

    fn stage_snapshot(self: *HttpServer, slot: *Slot, snapshot: []u8, live: []u8, seq: u64) void {
        slot.ws_snapshot = snapshot;
        slot.ws_live = live;
        slot.ws_seq = seq;
        var header: [10]u8 = undefined;
        const h = wire.ws_server_header(&header, .text, snapshot.len);
        @memcpy(slot.sbuf[slot.slen..][0..h.len], h);
        slot.slen += h.len;
        slot.send_slice = slot.sbuf[0..slot.slen];
        slot.slen = 0;
        slot.ws_stage = .preface;
        slot.deadline = self.now + send_timeout_ms;
    }

    fn resync_ready(self: *HttpServer, slot: *Slot, snapshot: []u8, live: []u8, seq: u64) void {
        if (slot.ws_stage != .resync_wait) {
            self.gpa.free(snapshot);
            self.gpa.free(live);
            return;
        }
        tiger.assert(slot.ws_snapshot.len == 0);
        slot.hard_deadline = 0;
        slot.ws_snapshot = snapshot;
        slot.ws_live = live;
        slot.ws_seq = seq;
        slot.ws_stage = .resync_ready;
        self.pump_ws(slot);
    }

    fn start_resync(self: *HttpServer, slot: *Slot) void {
        slot.ws_resyncs += 1;
        self.gauges.lagged.refuse();
        if (slot.ws_resyncs > resyncs_max) return self.begin_close(slot);
        slot.out.record = null;
        slot.ws_stage = .resync_wait;
        slot.hard_deadline = self.now + resync_wait_ms;
        _ = self.submit_aux(slot, .snapshot);
    }

    fn service_ws(self: *HttpServer, slot: *Slot) void {
        self.process_ws_input(slot);
        if (slot.mode != .ws) return;
        self.post_recv(slot);
        self.pump_ws(slot);
    }

    fn ws_fail(slot: *Slot) void {
        slot.out.queue_close(1002);
        slot.ws_in.dead = true;
    }

    fn close_code_valid(code: u16) bool {
        return switch (code) {
            1000...1003, 1007...1011, 3000...4999 => true,
            else => false,
        };
    }

    fn process_ws_input(self: *HttpServer, slot: *Slot) void {
        _ = self;
        const in = &slot.ws_in;
        if (in.dead) {
            slot.rlen = 0;
            return;
        }
        var pos: usize = 0;
        while (pos < slot.rlen and !in.dead) {
            if (!in.have_header) {
                const parsed = wire.ws_parse(slot.rbuf[pos..slot.rlen]) catch {
                    ws_fail(slot);
                    break;
                };
                const h = parsed orelse break;
                pos += h.len;
                switch (h.opcode) {
                    .text, .binary => {
                        if (in.fragmenting) {
                            ws_fail(slot);
                            break;
                        }
                        in.fragmenting = !h.fin;
                    },
                    .continuation => {
                        if (!in.fragmenting) {
                            ws_fail(slot);
                            break;
                        }
                        in.fragmenting = !h.fin;
                    },
                    else => {},
                }
                in.have_header = true;
                in.opcode = h.opcode;
                in.mask = h.mask;
                in.left = h.payload;
                in.offset = 0;
                in.control_len = 0;
            } else {
                const n: usize = @intCast(@min(@as(u64, slot.rlen - pos), in.left));
                if (@backingInt(in.opcode) >= 8) {
                    const dst = in.control[in.control_len..][0..n];
                    @memcpy(dst, slot.rbuf[pos..][0..n]);
                    wire.ws_unmask(dst, in.mask, in.offset);
                    in.control_len += n;
                }
                in.offset += n;
                in.left -= n;
                pos += n;
            }
            if (in.have_header and in.left == 0) {
                in.have_header = false;
                switch (in.opcode) {
                    .ping => slot.out.queue_pong(in.control[0..in.control_len]),
                    .close => {
                        in.dead = true;
                        if (in.control_len == 0) {
                            slot.out.queue_close(null);
                        } else if (in.control_len == 1) {
                            slot.out.queue_close(1002);
                        } else {
                            const code = std.mem.readInt(u16, in.control[0..2], .big);
                            slot.out.queue_close(if (close_code_valid(code)) code else 1002);
                        }
                    },
                    else => {},
                }
            }
        }
        const left = slot.rlen - pos;
        std.mem.copyForwards(u8, slot.rbuf[0..left], slot.rbuf[pos..slot.rlen]);
        slot.rlen = left;
        if (in.dead) slot.rlen = 0;
    }

    fn pump_ws(self: *HttpServer, slot: *Slot) void {
        while (slot.mode == .ws) {
            if (slot.send_inflight) return;
            if (slot.send_slice.len > 0) return self.post_send(slot);
            slot.deadline = 0;
            switch (slot.ws_stage) {
                .preface => {
                    slot.send_slice = slot.ws_snapshot;
                    slot.ws_stage = .snapshot;
                    return self.post_send(slot);
                },
                .snapshot => {
                    self.gpa.free(slot.ws_snapshot);
                    slot.ws_snapshot = &.{};
                    const n = self.stage_frame(slot, 0, .text, slot.ws_live);
                    self.gpa.free(slot.ws_live);
                    slot.ws_live = &.{};
                    slot.send_slice = slot.sbuf[0..n];
                    slot.ws_stage = .live_msg;
                    return self.post_send(slot);
                },
                .live_msg => {
                    slot.ws_stage = .live;
                    slot.out.cursor = self.rt.stream_hub.ring.find_after(slot.ws_seq) orelse return self.start_resync(slot);
                },
                .closing_frame => return self.begin_close(slot),
                .resync_wait => {
                    if (!slot.job_inflight) _ = self.submit_aux(slot, .snapshot);
                    if (slot.out.stage_control(slot.sbuf)) |step| self.send_staged(slot, step);
                    return;
                },
                .resync_ready => self.stage_snapshot(slot, slot.ws_snapshot, slot.ws_live, slot.ws_seq),
                .live => {
                    const step = slot.out.stage(&self.rt.stream_hub.ring, slot.sbuf, self.now, ws_ping_ms);
                    switch (step) {
                        .idle => return,
                        .resync => return self.start_resync(slot),
                        .abort => {
                            self.gauges.lagged.refuse();
                            return self.begin_close(slot);
                        },
                        else => return self.send_staged(slot, step),
                    }
                },
            }
        }
    }

    fn send_staged(self: *HttpServer, slot: *Slot, step: WsOut.Step) void {
        switch (step) {
            .staged => |n| slot.send_slice = slot.sbuf[0..n],
            .close_staged => |n| {
                slot.send_slice = slot.sbuf[0..n];
                slot.ws_stage = .closing_frame;
            },
            else => unreachable,
        }
        self.post_send(slot);
    }
};

fn token_of(slot: *const Slot) ?[]const u8 {
    if (slot.req.authorization) |token| return token;
    if (slot.query.len > 0) return query_param(slot.query, "access_token") orelse query_param(slot.query, "token");
    return null;
}

fn query_param(query: []const u8, key: []const u8) ?[]const u8 {
    var it = std.mem.splitScalar(u8, query, '&');
    while (it.next()) |kv| {
        if (std.mem.findScalar(u8, kv, '=')) |eq| {
            if (std.mem.eql(u8, kv[0..eq], key)) return kv[eq + 1 ..];
        }
    }
    return null;
}

fn count_used(used: []const bool) usize {
    var count: usize = 0;
    for (used) |u| {
        if (u) count += 1;
    }
    return count;
}

fn wake_backend(context: *anyopaque) void {
    const backend: *Backend = @ptrCast(@alignCast(context));
    backend.wake();
}

fn origin_matches_host(origin: []const u8, host: ?[]const u8) bool {
    const authority = if (std.mem.startsWith(u8, origin, "http://"))
        origin[7..]
    else if (std.mem.startsWith(u8, origin, "https://"))
        origin[8..]
    else
        return false;
    if (authority.len == 0 or std.mem.indexOfAny(u8, authority, "/?#@ \t\r\n") != null) return false;
    const uri = std.Uri.parse(origin) catch return false;
    if (uri.host == null or uri.user != null or !uri.path.isEmpty() or uri.query != null or uri.fragment != null) return false;
    return std.ascii.eqlIgnoreCase(authority, host orelse return false);
}

fn peer_address(sock: netio.Socket) ?std.Io.net.IpAddress {
    const endpoint = Backend.peer(sock) orelse return null;
    return switch (endpoint.family) {
        .ipv4 => .{ .ip4 = .{ .bytes = endpoint.address[0..4].*, .port = 0 } },
        .ipv6 => .{ .ip6 = .{ .bytes = endpoint.address, .port = 0 } },
    };
}

fn normalize_ip4(addr: std.Io.net.IpAddress) ?[4]u8 {
    return switch (addr) {
        .ip4 => |v4| v4.bytes,
        .ip6 => |v6| blk: {
            for (v6.bytes[0..10]) |b| {
                if (b != 0) break :blk null;
            }
            if (v6.bytes[10] != 0xff or v6.bytes[11] != 0xff) break :blk null;
            break :blk v6.bytes[12..16].*;
        },
    };
}

fn ip_matches_any(peer: std.Io.net.IpAddress, allowed: []const std.Io.net.IpAddress) bool {
    const p4 = normalize_ip4(peer);
    for (allowed) |cand| {
        if (p4) |pv4| {
            if (normalize_ip4(cand)) |cv4| {
                if (std.mem.eql(u8, &pv4, &cv4)) return true;
                continue;
            }
        }
        if (peer == .ip6 and cand == .ip6) {
            if (std.mem.eql(u8, &peer.ip6.bytes, &cand.ip6.bytes)) return true;
        }
    }
    return false;
}

fn parse_host_port(raw: []const u8, def_port: u16) struct { host: []const u8, port: u16 } {
    if (std.mem.findScalarLast(u8, raw, ':')) |colon| {
        if (std.fmt.parseInt(u16, raw[colon + 1 ..], 10)) |p| {
            return .{ .host = raw[0..colon], .port = p };
        } else |_| {}
    }
    return .{ .host = raw, .port = def_port };
}
