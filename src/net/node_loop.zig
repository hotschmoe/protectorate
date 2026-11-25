const std = @import("std");
const tiger = @import("../tiger.zig");
const io = @import("io.zig");
const noise = @import("noise.zig");
const frame = @import("frame.zig");
const msg = @import("msg.zig");
const bounded = @import("bounded.zig");
const limits = @import("limits.zig");
const node_link = @import("node_link.zig");

pub const work_slots = 16;
pub const task_reserve = 2;
pub const request_max = 288 * 1024;
pub const control_slots = 8;
const control_max = 192;
const passes_max = 4;
const flush_wait_ms = 2_000;

comptime {
    tiger.comptime_assert(work_slots <= std.math.maxInt(u8), "slot index fits a ring entry");
    tiger.comptime_assert(task_reserve < work_slots, "requests keep slots");
    tiger.comptime_assert(limits.piece_max <= frame.max_payload, "a piece fits one frame");
}

pub const Task = enum(u8) { request, resolve, telemetry, healthy, cleanup };

/// One request or task as a worker sees it. The worker fills the reply fields and hands it back with finish_work.
pub const Work = struct {
    index: u8,
    id: u64,
    task: Task,
    request: []const u8,
    chunk: []u8,
    payload: []u8,
    abandoned: *const std.atomic.Value(bool),
    kind: frame.Kind = .response,
    chunk_len: usize = 0,
    payload_len: usize = 0,
    then_stop: bool = false,
    no_reply: bool = false,
    /// The reply carries a side effect, such as a granted lease, that must be undone if the reply is never sent:
    /// the dropped reply's payload comes back to a worker as a cleanup task.
    cleanup: bool = false,
};

pub const Embed = struct {
    context: *anyopaque,
    hello_reply: *const fn (*anyopaque, []const u8) bool,
    task_done: *const fn (*anyopaque, Task, u64) void,
    connected: *const fn (*anyopaque) void,
    disconnected: *const fn (*anyopaque) void,
    notify: *const fn (*anyopaque) void,
    resolve: bool,
};

pub const Buffers = struct {
    request: [request_max]u8,
    chunk: [limits.piece_max]u8,
    payload: [frame.max_payload]u8,
};

const Slot = struct {
    state: enum { free, working, sending, awaiting } = .free,
    mark: u64 = 0,
    task: Task = .request,
    generation: u64 = 0,
    id: u64 = 0,
    request_len: usize = 0,
    abandoned: std.atomic.Value(bool) = .init(false),
    kind: frame.Kind = .response,
    chunk_len: usize = 0,
    payload_len: usize = 0,
    then_stop: bool = false,
    no_reply: bool = false,
    cleanup: bool = false,
    chunk_sent: bool = false,
};

const Control = struct {
    generation: u64,
    id: u64,
    len: usize,
    bytes: [control_max]u8,
};

/// The node's single-owner core: it owns the link and every slot, and talks to workers only through two
/// bounded rings that the embedder's Sync guards. The embedder drives turn() on one thread and never blocks it.
pub fn NodeLoop(comptime IO: type, comptime Sync: type) type {
    io.assert_contract(IO);
    return struct {
        const Self = @This();
        pub const Link = node_link.NodeLink(IO);

        backend: *IO,
        sync: *Sync,
        embed: Embed,
        link: Link = undefined,
        buffers: [work_slots]*Buffers = undefined,
        slots: [work_slots]Slot = @splat(.{}),
        work: bounded.Ring(u8, work_slots) = .{},
        finished: bounded.Ring(u8, work_slots) = .{},
        outbox: bounded.Ring(u8, work_slots) = .{},
        awaiting: bounded.Ring(u8, work_slots) = .{},
        control: bounded.Ring(Control, control_slots) = .{},
        resolve_wanted: ?u64 = null,
        stop_flush: bool = false,
        stop_deadline: u64 = 0,
        stalled: bool = false,
        stopping: bool = false,
        stop_reply_sent: bool = false,
        busy_replies: u64 = 0,
        dropped_replies: u64 = 0,

        pub fn init(
            self: *Self,
            allocator: std.mem.Allocator,
            backend: *IO,
            pool: *bounded.SlabPool,
            sync: *Sync,
            endpoints: []const io.Endpoint,
            key: noise.KeyPair,
            hub_key: [32]u8,
            hello: []const u8,
            embed: Embed,
        ) !void {
            self.* = .{ .backend = backend, .sync = sync, .embed = embed };
            var made: usize = 0;
            errdefer for (self.buffers[0..made]) |buffer| bounded.destroy_reserved(allocator, Buffers, buffer);
            for (&self.buffers) |*buffer| {
                buffer.* = try bounded.create_reserved(allocator, Buffers);
                made += 1;
            }
            const hooks: node_link.Hooks = .{
                .context = self,
                .hello_reply = link_hello_reply,
                .resolve = if (embed.resolve) &link_resolve else null,
            };
            try self.link.init(backend, pool, endpoints, key, hub_key, hello, hooks);
        }

        pub fn deinit(self: *Self, allocator: std.mem.Allocator) void {
            tiger.assert(self.quiescent());
            self.link.deinit();
            for (self.buffers) |buffer| bounded.destroy_reserved(allocator, Buffers, buffer);
        }

        fn link_hello_reply(context: *anyopaque, payload: []const u8) bool {
            const self: *Self = @ptrCast(@alignCast(context));
            return self.embed.hello_reply(self.embed.context, payload);
        }

        fn link_resolve(context: *anyopaque, serial: u64) void {
            const self: *Self = @ptrCast(@alignCast(context));
            self.resolve_wanted = serial;
        }

        pub fn shutdown(self: *Self) void {
            self.stopping = true;
            self.link.shutdown();
            self.abandon_all();
        }

        pub fn done(self: *const Self) bool {
            return self.link.done();
        }

        /// True when no slot is held and nothing is queued: every worker has handed its slot back.
        pub fn quiescent(self: *const Self) bool {
            for (&self.slots) |*slot| if (slot.state != .free) return false;
            return self.work.len == 0 and self.finished.len == 0 and self.outbox.len == 0 and self.awaiting.len == 0;
        }

        /// Request slots still with a worker or waiting to send.
        pub fn count_requests(self: *const Self) usize {
            var count: usize = 0;
            for (&self.slots) |*slot| {
                if ((slot.state == .working or slot.state == .sending) and slot.task == .request) count += 1;
            }
            return count;
        }

        pub fn free_slots(self: *const Self) usize {
            var count: usize = 0;
            for (&self.slots) |*slot| {
                if (slot.state == .free) count += 1;
            }
            return count;
        }

        pub fn assert_invariants(self: *const Self) void {
            var working: usize = 0;
            var sending: usize = 0;
            var awaiting: usize = 0;
            for (&self.slots) |*slot| switch (slot.state) {
                .free => {},
                .working => working += 1,
                .sending => sending += 1,
                .awaiting => awaiting += 1,
            };
            tiger.assert(sending == self.outbox.len and awaiting == self.awaiting.len);
            tiger.assert(self.work.len + self.finished.len <= working);
        }

        pub fn turn(self: *Self) void {
            self.stalled = false;
            var pass: usize = 0;
            while (pass < passes_max) : (pass += 1) {
                self.link.drive() catch |err| std.debug.panic("node loop: link drive failed: {s}", .{@errorName(err)});
                var progress = self.take_finished();
                if (self.service_events()) progress = true;
                if (self.flush()) progress = true;
                if (self.settle()) progress = true;
                if (self.submit_resolve()) progress = true;
                if (!progress) break;
            }
            if (pass == passes_max) self.stalled = true;
            if (self.stop_flush and (self.link.flushed() or self.link.phase != .established or self.backend.now() >= self.stop_deadline)) {
                self.stop_flush = false;
                self.stop_reply_sent = true;
            }
        }

        /// A task needs a free slot but may use the ones requests may not.
        pub fn submit_task(self: *Self, task: Task, id: u64) bool {
            tiger.assert(task != .request);
            const index = self.free_slot(0) orelse return false;
            self.start(index, task, id, 0);
            return true;
        }

        fn free_slot(self: *Self, reserve: usize) ?u8 {
            if (self.free_slots() <= reserve) return null;
            for (&self.slots, 0..) |*slot, i| {
                if (slot.state == .free) return @intCast(i);
            }
            unreachable;
        }

        fn start(self: *Self, index: u8, task: Task, id: u64, request_len: usize) void {
            const slot = &self.slots[index];
            tiger.assert(slot.state == .free);
            slot.* = .{ .state = .working, .task = task, .generation = self.link.generation, .id = id, .request_len = request_len };
            self.sync.lock();
            self.work.push(index) catch unreachable;
            self.sync.unlock();
            self.embed.notify(self.embed.context);
        }

        fn submit_resolve(self: *Self) bool {
            const serial = self.resolve_wanted orelse return false;
            if (!self.link.resolving()) {
                self.resolve_wanted = null;
                return true;
            }
            if (!self.submit_task(.resolve, serial)) {
                self.stalled = true;
                return false;
            }
            self.resolve_wanted = null;
            return true;
        }

        /// Worker side: takes the oldest queued slot, or null.
        pub fn next_work(self: *Self) ?Work {
            self.sync.lock();
            const index = self.work.pop();
            self.sync.unlock();
            const i = index orelse return null;
            const slot = &self.slots[i];
            const buffer = self.buffers[i];
            return .{
                .index = i,
                .id = slot.id,
                .task = slot.task,
                .request = buffer.request[0..slot.request_len],
                .chunk = &buffer.chunk,
                .payload = &buffer.payload,
                .abandoned = &slot.abandoned,
            };
        }

        /// Worker side: records the reply and returns the slot to the loop. The embedder wakes the loop afterwards.
        pub fn finish_work(self: *Self, work: *const Work) void {
            tiger.assert(work.chunk_len <= limits.piece_max and work.payload_len <= frame.max_payload);
            const slot = &self.slots[work.index];
            slot.kind = work.kind;
            slot.chunk_len = work.chunk_len;
            slot.payload_len = work.payload_len;
            slot.then_stop = work.then_stop;
            slot.no_reply = work.no_reply;
            slot.cleanup = work.cleanup;
            self.sync.lock();
            self.finished.push(work.index) catch unreachable;
            self.sync.unlock();
        }

        /// After every worker has exited: drops queued work and takes back every finished slot.
        pub fn drain(self: *Self) void {
            self.sync.lock();
            while (self.work.pop()) |index| self.finished.push(index) catch unreachable;
            self.sync.unlock();
            _ = self.take_finished();
        }

        fn take_finished(self: *Self) bool {
            var progress = false;
            while (true) {
                self.sync.lock();
                const index = self.finished.pop();
                self.sync.unlock();
                const i = index orelse break;
                progress = true;
                const slot = &self.slots[i];
                tiger.assert(slot.state == .working);
                if (slot.task != .request) {
                    const task = slot.task;
                    const id = slot.id;
                    self.release(i);
                    self.embed.task_done(self.embed.context, task, id);
                    continue;
                }
                const live = self.link.phase == .established and slot.generation == self.link.generation;
                if (!live or slot.abandoned.load(.acquire) or slot.no_reply) {
                    if (slot.then_stop) self.stop_reply_sent = true;
                    self.drop(i);
                    continue;
                }
                slot.state = .sending;
                self.outbox.push(i) catch unreachable;
            }
            return progress;
        }

        /// Whether every byte of an awaiting slot's reply has been written to the socket on the link it was sent on.
        fn fully_sent(self: *const Self, index: u8) bool {
            const slot = &self.slots[index];
            return slot.generation == self.link.generation and self.link.conn.tx_done >= slot.mark;
        }

        /// A reply with a side effect keeps its slot until its last byte is written, so a disconnect before that
        /// still releases the effect; once written, only lease expiry covers a reply lost in flight.
        fn settle(self: *Self) bool {
            var progress = false;
            while (self.awaiting.len > 0) {
                const index = self.awaiting.items[self.awaiting.head];
                if (self.fully_sent(index)) {
                    self.release(self.awaiting.pop().?);
                } else if (self.slots[index].generation == self.link.generation and self.link.phase == .established) {
                    break;
                } else self.drop(self.awaiting.pop().?);
                progress = true;
            }
            return progress;
        }

        fn release(self: *Self, index: u8) void {
            self.slots[index] = .{};
        }

        /// A reply that will not be sent. One that carries a side effect goes back to a worker to be undone.
        fn drop(self: *Self, index: u8) void {
            const slot = &self.slots[index];
            if (!slot.no_reply) self.dropped_replies += 1;
            if (!slot.cleanup or self.stopping or slot.no_reply) return self.release(index);
            const buffer = self.buffers[index];
            const len = slot.payload_len;
            @memcpy(buffer.request[0..len], buffer.payload[0..len]);
            slot.* = .{ .state = .working, .task = .cleanup, .generation = self.link.generation, .request_len = len };
            self.sync.lock();
            self.work.push(index) catch unreachable;
            self.sync.unlock();
            self.embed.notify(self.embed.context);
        }

        fn abandon_all(self: *Self) void {
            for (&self.slots) |*slot| {
                if (slot.state == .working) slot.abandoned.store(true, .release);
            }
            while (self.outbox.pop()) |index| self.drop(index);
            while (self.awaiting.pop()) |index| {
                if (self.fully_sent(index)) self.release(index) else self.drop(index);
            }
            self.control = .{};
        }

        fn service_events(self: *Self) bool {
            var progress = false;
            while (self.link.event()) |event| {
                switch (event) {
                    .connected => |generation| {
                        tiger.assert(generation == self.link.generation);
                        self.link.consume(event);
                        self.embed.connected(self.embed.context);
                    },
                    .disconnected => {
                        self.abandon_all();
                        self.link.consume(event);
                        self.embed.disconnected(self.embed.context);
                    },
                    .request => |request| {
                        if (!self.on_request(request)) {
                            self.stalled = true;
                            break;
                        }
                        self.link.consume(event);
                    },
                }
                progress = true;
            }
            return progress;
        }

        fn on_request(self: *Self, request: anytype) bool {
            if (request.size > request_max) return self.refuse(request.generation, request.id, .invalid, "request too large");
            const index = self.free_slot(task_reserve) orelse {
                if (!self.refuse(request.generation, request.id, .busy, "node workers busy")) return false;
                self.busy_replies += 1;
                return true;
            };
            const buffer = self.buffers[index];
            self.link.read(request.generation, request.serial, 0, buffer.request[0..request.size]) catch return true;
            self.start(index, .request, request.id, request.size);
            return true;
        }

        fn refuse(self: *Self, generation: u64, id: u64, code: msg.ErrorCode, text: []const u8) bool {
            if (self.control.len == control_slots) return false;
            var entry: Control = .{ .generation = generation, .id = id, .len = 0, .bytes = undefined };
            const body = msg.encode_fixed(&entry.bytes, msg.ErrorReply{ .code = code, .message = text }) catch unreachable;
            entry.len = body.len;
            self.control.push(entry) catch unreachable;
            return true;
        }

        fn flush(self: *Self) bool {
            var progress = false;
            while (self.control.len > 0) {
                const entry = &self.control.items[self.control.head];
                if (self.send(entry.generation, .err, entry.id, entry.bytes[0..entry.len])) |sent| {
                    if (!sent) break;
                } else |_| {}
                _ = self.control.pop();
                progress = true;
            }
            while (self.outbox.len > 0) {
                const index = self.outbox.items[self.outbox.head];
                const slot = &self.slots[index];
                tiger.assert(slot.state == .sending);
                const buffer = self.buffers[index];
                const chunk = slot.chunk_len > 0 and !slot.chunk_sent;
                const kind: frame.Kind = if (chunk) .chunk else slot.kind;
                const payload: []const u8 = if (chunk) buffer.chunk[0..slot.chunk_len] else buffer.payload[0..slot.payload_len];
                if (self.send(slot.generation, kind, slot.id, payload)) |sent| {
                    if (!sent) break;
                    progress = true;
                    if (chunk) {
                        slot.chunk_sent = true;
                        continue;
                    }
                } else |_| {
                    if (slot.then_stop) self.stop_reply_sent = true;
                    _ = self.outbox.pop();
                    self.drop(index);
                    progress = true;
                    continue;
                }
                if (slot.then_stop) {
                    self.stop_flush = true;
                    self.stop_deadline = self.backend.now() +| flush_wait_ms;
                }
                _ = self.outbox.pop();
                progress = true;
                if (slot.cleanup) {
                    slot.state = .awaiting;
                    slot.mark = self.link.conn.tx_queued;
                    self.awaiting.push(index) catch unreachable;
                } else self.release(index);
            }
            return progress;
        }

        /// True when the frame was queued, false when the link is busy; an error means the reply can never be sent.
        fn send(self: *Self, generation: u64, kind: frame.Kind, id: u64, payload: []const u8) error{Gone}!bool {
            self.link.send(generation, kind, id, payload) catch |err| switch (err) {
                error.Busy, error.Full => {
                    self.stalled = true;
                    return false;
                },
                else => return error.Gone,
            };
            return true;
        }
    };
}
