const std = @import("std");
const builtin = @import("builtin");
const noise = @import("../net/noise.zig");
const frame = @import("../net/frame.zig");
const msg = @import("../net/msg.zig");
const io_net = @import("../net/io.zig");
const limits = @import("../net/limits.zig");
const artifact_sink = @import("artifact_sink.zig");
const fleet_loop = @import("fleet_loop.zig");

pub const AllowedPeer = fleet_loop.AllowedPeer;

/// The fleet loop runs on the Linux epoll backend; other targets build the hub but cannot start its fleet.
const supported = builtin.os.tag == .linux;
const Loop = if (supported) fleet_loop.Loop else void;
pub const Hooks = struct {
    ctx: ?*anyopaque,
    get_allowed_peers: *const fn (ctx: ?*anyopaque, arena: std.mem.Allocator) ?[]AllowedPeer,
    on_connected: *const fn (ctx: ?*anyopaque, node_id: []const u8, hello: msg.Hello) void,
    on_telemetry: *const fn (ctx: ?*anyopaque, node_id: []const u8, hello: msg.Hello) void,
    on_disconnected: *const fn (ctx: ?*anyopaque, node_id: []const u8, reason: []const u8) void,
};

const Reply = struct {
    server: *FleetServer,
    slot: u8,
    buffer: []u8,
    kind: frame.Kind,
    chunks: []const u8,
    payload: []const u8,

    fn deinit(self: *Reply) void {
        FleetServer.reply_free(self.buffer);
        if (comptime supported) self.server.loop.?.release(self.slot);
    }
};

pub const FleetServer = struct {
    gpa: std.mem.Allocator,
    io: std.Io,
    hub_kp: noise.KeyPair,
    hooks: Hooks,
    host: []const u8,
    port: u16,
    actual_port: std.atomic.Value(u16) = .init(0),
    stopping: std.atomic.Value(bool) = .init(false),
    loop: ?*Loop = null,
    loop_thread: ?std.Thread = null,
    consumer_thread: ?std.Thread = null,
    publish_mu: std.Io.Mutex = .init,

    pub fn init(
        gpa: std.mem.Allocator,
        io: std.Io,
        hub_kp: noise.KeyPair,
        listen_addr: []const u8,
        hooks: Hooks,
    ) !FleetServer {
        const hp = parse_host_port(listen_addr, 7475);
        return .{
            .gpa = gpa,
            .io = io,
            .hub_kp = hub_kp,
            .hooks = hooks,
            .host = try gpa.dupe(u8, hp.host),
            .port = hp.port,
        };
    }

    pub fn start(self: *FleetServer) !void {
        if (comptime !supported) return error.FleetUnsupportedPlatform;
        const address = try std.Io.net.IpAddress.parse(self.host, self.port);
        const endpoint: io_net.Endpoint = switch (address) {
            .ip4 => |a| blk: {
                var bytes: [16]u8 = @splat(0);
                @memcpy(bytes[0..4], &a.bytes);
                break :blk .{ .address = bytes, .port = a.port, .family = .ipv4 };
            },
            .ip6 => |a| .{ .address = a.bytes, .port = a.port, .family = .ipv6 },
        };
        const loop = try fleet_loop.Loop.create(self.gpa, self.io, self.hub_kp, endpoint);
        errdefer loop.destroy();
        self.loop = loop;
        errdefer self.loop = null;
        self.actual_port.store(loop.port, .release);
        self.publish_peers();
        loop.freeze();
        self.consumer_thread = std.Thread.spawn(.{}, consume_events, .{self}) catch |err| {
            loop.request_stop();
            loop.run();
            return err;
        };
        self.loop_thread = std.Thread.spawn(.{}, fleet_loop.Loop.run, .{loop}) catch |err| {
            loop.request_stop();
            loop.run();
            self.consumer_thread.?.join();
            self.consumer_thread = null;
            return err;
        };
    }

    pub fn listen_port(self: *FleetServer) u16 {
        return self.actual_port.load(.acquire);
    }

    /// Hands the loop the current allowed-peer list; call after any change to which nodes may connect.
    pub fn publish_peers(self: *FleetServer) void {
        if (comptime !supported) return;
        const loop = self.loop orelse return;
        self.publish_mu.lockUncancelable(self.io);
        defer self.publish_mu.unlock(self.io);
        var arena_st = std.heap.ArenaAllocator.init(self.gpa);
        defer arena_st.deinit();
        const peers = self.hooks.get_allowed_peers(self.hooks.ctx, arena_st.allocator()) orelse {
            std.debug.print("fleet: could not build the allowed-peer list; keeping the previous one\n", .{});
            return;
        };
        loop.publish(peers);
    }

    pub fn shutdown(self: *FleetServer) void {
        if (comptime !supported) return;
        if (self.stopping.swap(true, .acq_rel)) return;
        const loop = self.loop orelse return;
        loop.request_stop();
        if (self.loop_thread) |t| t.join();
        self.loop_thread = null;
    }

    pub fn stop_and_deinit(self: *FleetServer) void {
        if (comptime supported) {
            self.shutdown();
            if (self.consumer_thread) |t| t.join();
            self.consumer_thread = null;
            if (self.loop) |loop| loop.destroy();
            self.loop = null;
        }
        self.gpa.free(self.host);
    }

    pub fn disconnect_node(self: *FleetServer, node_id: []const u8) void {
        if (comptime !supported) return;
        const loop = self.loop orelse return;
        loop.disconnect(node_id);
    }

    fn consume_events(self: *FleetServer) void {
        const loop = self.loop.?;
        const event = self.gpa.create(fleet_loop.Event) catch @panic("fleet event consumer: out of memory");
        defer self.gpa.destroy(event);
        var arena_st = std.heap.ArenaAllocator.init(self.gpa);
        defer arena_st.deinit();
        while (loop.next_event(event)) {
            _ = arena_st.reset(.retain_capacity);
            self.deliver(arena_st.allocator(), event);
        }
    }

    fn deliver(self: *FleetServer, arena: std.mem.Allocator, event: *const fleet_loop.Event) void {
        switch (event.kind) {
            .disconnected => self.hooks.on_disconnected(self.hooks.ctx, event.node_id(), "closed"),
            .connected, .telemetry => {
                const request = msg.decode_arena(msg.Request, arena, event.body()) catch {
                    std.debug.print("fleet: dropped an undecodable hello from {s}\n", .{event.node_id()});
                    return;
                };
                const hello = switch (request) {
                    .hello => |h| h,
                    else => return,
                };
                if (event.kind == .connected) {
                    self.hooks.on_connected(self.hooks.ctx, event.node_id(), hello);
                } else self.hooks.on_telemetry(self.hooks.ctx, event.node_id(), hello);
            },
        }
    }

    /// Delivery counters for hub info.
    pub fn stats(self: *FleetServer) struct { telemetry_dropped: u64, refused: u64 } {
        if (comptime !supported) return .{ .telemetry_dropped = 0, .refused = 0 };
        const loop = self.loop orelse return .{ .telemetry_dropped = 0, .refused = 0 };
        return .{ .telemetry_dropped = loop.telemetry_dropped.load(.monotonic), .refused = loop.refused.load(.monotonic) };
    }

    fn call(self: *FleetServer, node_id: []const u8, request: msg.Request, timeout_ms: u32, chunk_cap: usize, long: bool) !Reply {
        if (comptime !supported) return error.NodeOffline;
        const loop = self.loop orelse return error.NodeOffline;
        const slot = try loop.reserve();
        errdefer loop.release(slot);
        const encoded = try msg.encode_alloc(self.gpa, request);
        defer self.gpa.free(encoded);
        const buffer = try reply_buffer(chunk_cap + limits.fleet_control_reply_max);
        errdefer reply_free(buffer);
        const reply = try loop.call(slot, node_id, encoded, buffer, chunk_cap, timeout_ms, long);
        return .{ .server = self, .slot = slot, .buffer = buffer, .kind = reply.kind, .chunks = reply.chunks, .payload = reply.payload };
    }

    /// Reply buffers come straight from the page allocator and live only while their call slot is held, so at most
    /// one per slot is mapped; a frame-sized buffer costs only the pages a reply touches.
    fn reply_buffer(len: usize) ![]u8 {
        const pointer = std.heap.page_allocator.rawAlloc(len, .of(u8), @returnAddress()) orelse return error.OutOfMemory;
        return pointer[0..len];
    }

    fn reply_free(buffer: []u8) void {
        std.heap.page_allocator.rawFree(buffer, .of(u8), @returnAddress());
    }

    fn decode_reply(self: *FleetServer, comptime T: type, arena: std.mem.Allocator, node_id: []const u8, request: msg.Request, timeout_ms: u32) !T {
        return self.decode_reply_long(T, arena, node_id, request, timeout_ms, false);
    }

    fn decode_reply_long(self: *FleetServer, comptime T: type, arena: std.mem.Allocator, node_id: []const u8, request: msg.Request, timeout_ms: u32, long: bool) !T {
        var reply = try self.call(node_id, request, timeout_ms, 0, long);
        defer reply.deinit();
        if (reply.kind == .err) return error.RemoteError;
        return try msg.decode_arena(T, arena, reply.payload);
    }

    pub fn submit_job(self: *FleetServer, arena: std.mem.Allocator, node_id: []const u8, sj: msg.SubmitJob) !msg.JobInfo {
        return (try self.decode_reply(msg.JobReply, arena, node_id, .{ .submit_job = sj }, 15_000)).job;
    }

    pub fn job_status(self: *FleetServer, arena: std.mem.Allocator, node_id: []const u8, job_id: []const u8) !msg.JobInfo {
        return (try self.decode_reply(msg.JobReply, arena, node_id, .{ .job_status = .{ .job_id = job_id } }, 10_000)).job;
    }

    pub fn cancel_job(self: *FleetServer, arena: std.mem.Allocator, node_id: []const u8, job_id: []const u8) !msg.JobInfo {
        return (try self.decode_reply(msg.JobReply, arena, node_id, .{ .cancel_job = .{ .job_id = job_id } }, 10_000)).job;
    }

    pub fn fetch_log(
        self: *FleetServer,
        arena: std.mem.Allocator,
        node_id: []const u8,
        job_id: []const u8,
        offset: u64,
        limit: usize,
    ) !struct { data: []const u8, next_offset: u64, eof: bool } {
        const piece = @min(limit, limits.piece_max);
        var res = try self.call(node_id, .{ .fetch_log = .{
            .job_id = job_id,
            .offset = offset,
            .limit = piece,
        } }, 10_000, piece, false);
        defer res.deinit();
        if (res.kind == .err) return error.RemoteError;
        const le = try msg.decode_arena(msg.LogEnd, arena, res.payload);
        return .{
            .data = try arena.dupe(u8, res.chunks),
            .next_offset = le.next_offset,
            .eof = le.eof,
        };
    }

    /// Streams one artifact into artifacts_root/job_id/rel_path through a temporary file.
    /// declared_size is the size the node reported in the job status; it is refused above the cap
    /// before any transfer, and the received bytes are checked against it and the cap piece by piece.
    /// Each piece is one call; the sink writes it on this thread, never on the fleet loop.
    pub fn fetch_artifact(
        self: *FleetServer,
        arena: std.mem.Allocator,
        node_id: []const u8,
        job_id: []const u8,
        rel_path: []const u8,
        artifacts_root: []const u8,
        declared_size: u64,
    ) !msg.JobArtifact {
        var sink = try artifact_sink.Sink.open(
            self.io,
            arena,
            artifacts_root,
            job_id,
            rel_path,
            declared_size,
            limits.artifact_max_bytes,
        );
        errdefer sink.abort();

        var expected_sha: []const u8 = "";
        while (true) {
            var res = try self.call(node_id, .{ .fetch_artifact = .{
                .job_id = job_id,
                .path = rel_path,
                .offset = sink.written,
                .limit = limits.piece_max,
            } }, 20_000, limits.piece_max, false);
            defer res.deinit();
            if (res.kind == .err) return error.RemoteError;
            const ae = try msg.decode_arena(msg.ArtifactEnd, arena, res.payload);
            if (ae.size_bytes != declared_size) return error.ArtifactSizeMismatch;
            try sink.write(res.chunks);
            if (ae.next_offset != sink.written) return error.ArtifactSizeMismatch;
            expected_sha = ae.sha256;
            if (ae.eof) break;
            if (res.chunks.len == 0) return error.ArtifactSizeMismatch;
        }

        const hex = try sink.finish(expected_sha);
        return .{
            .path = try arena.dupe(u8, rel_path),
            .size_bytes = declared_size,
            .sha256 = try arena.dupe(u8, &hex),
        };
    }

    pub fn send_update(
        self: *FleetServer,
        arena: std.mem.Allocator,
        node_id: []const u8,
        upd: msg.Update,
    ) !msg.UpdateReply {
        return self.decode_reply(msg.UpdateReply, arena, node_id, .{ .update = upd }, 60_000);
    }

    pub fn upload_update(
        self: *FleetServer,
        arena: std.mem.Allocator,
        node_id: []const u8,
        up: msg.UploadUpdate,
    ) !msg.UploadUpdateReply {
        return self.decode_reply(msg.UploadUpdateReply, arena, node_id, .{ .upload_update = up }, 30_000);
    }

    pub fn ack_job(self: *FleetServer, arena: std.mem.Allocator, node_id: []const u8, job_id: []const u8) !msg.AckJobReply {
        return self.decode_reply(msg.AckJobReply, arena, node_id, .{ .ack_job = .{ .job_id = job_id } }, 10_000);
    }

    pub fn acquire_lease(
        self: *FleetServer,
        arena: std.mem.Allocator,
        node_id: []const u8,
        req: msg.AcquireLease,
    ) !msg.LeaseReply {
        const extra: u32 = if (req.wait_ms) |w| @intCast(@min(@as(i64, 300_000), @max(@as(i64, 0), w))) else 0;
        return self.decode_reply_long(msg.LeaseReply, arena, node_id, .{ .acquire_lease = req }, 15_000 + extra, extra > 0);
    }

    pub fn renew_lease(self: *FleetServer, arena: std.mem.Allocator, node_id: []const u8, req: msg.RenewLease) !msg.LeaseReply {
        return self.decode_reply(msg.LeaseReply, arena, node_id, .{ .renew_lease = req }, 10_000);
    }

    pub fn release_lease(self: *FleetServer, arena: std.mem.Allocator, node_id: []const u8, req: msg.ReleaseLease) !msg.ReleaseLeaseReply {
        return self.decode_reply(msg.ReleaseLeaseReply, arena, node_id, .{ .release_lease = req }, 15_000);
    }

    pub fn verify_epoch(self: *FleetServer, arena: std.mem.Allocator, node_id: []const u8, req: msg.VerifyEpoch) !msg.VerifyEpochReply {
        return self.decode_reply(msg.VerifyEpochReply, arena, node_id, .{ .verify_epoch = req }, 10_000);
    }

    pub fn list_leases(self: *FleetServer, arena: std.mem.Allocator, node_id: []const u8) !msg.AdmissionStatus {
        return self.decode_reply(msg.AdmissionStatus, arena, node_id, .{ .list_leases = .{} }, 10_000);
    }

    pub fn clear_quarantine(self: *FleetServer, arena: std.mem.Allocator, node_id: []const u8) !msg.ClearQuarantineReply {
        return self.decode_reply(msg.ClearQuarantineReply, arena, node_id, .{ .clear_quarantine = .{} }, 10_000);
    }

    pub fn build_join_token(arena: std.mem.Allocator, hub_pub: [32]u8, addrs: []const []const u8) ![]u8 {
        const chars = "0123456789abcdef";
        var hex: [64]u8 = undefined;
        for (hub_pub, 0..) |b, i| {
            hex[i * 2] = chars[b >> 4];
            hex[i * 2 + 1] = chars[b & 0x0f];
        }
        const fp = noise.fingerprint_prefixed(hub_pub);
        const payload = .{
            .hub_public_key = &hex,
            .hub_fingerprint = &fp,
            .addrs = addrs,
        };
        const json = try std.json.Stringify.valueAlloc(arena, payload, .{});
        const enc_len = std.base64.url_safe_no_pad.Encoder.calcSize(json.len);
        const b64 = try arena.alloc(u8, enc_len);
        _ = std.base64.url_safe_no_pad.Encoder.encode(b64, json);
        return std.fmt.allocPrint(arena, "ptjoin1:{s}", .{b64});
    }
};

fn parse_host_port(raw: []const u8, def_port: u16) struct { host: []const u8, port: u16 } {
    if (std.mem.findScalarLast(u8, raw, ':')) |colon| {
        if (std.fmt.parseInt(u16, raw[colon + 1 ..], 10)) |p| {
            return .{ .host = raw[0..colon], .port = p };
        } else |_| {}
    }
    return .{ .host = raw, .port = def_port };
}
