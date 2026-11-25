const std = @import("std");
const tiger = @import("../tiger.zig");
const version = @import("../version.zig");
const noise = @import("../net/noise.zig");
const frame = @import("../net/frame.zig");
const msg = @import("../net/msg.zig");
const transport = @import("../net/transport.zig");
const limits = @import("../net/limits.zig");
const artifact_sink = @import("artifact_sink.zig");

pub const AllowedPeer = struct {
    node_id: []const u8,
    name: []const u8,
    pub_key: [32]u8,
};

pub const Hooks = struct {
    ctx: ?*anyopaque,
    get_allowed_peers: *const fn (ctx: ?*anyopaque, arena: std.mem.Allocator) []AllowedPeer,
    on_connected: *const fn (ctx: ?*anyopaque, node_id: []const u8, hello: msg.Hello) void,
    on_telemetry: *const fn (ctx: ?*anyopaque, node_id: []const u8, hello: msg.Hello) void,
    on_disconnected: *const fn (ctx: ?*anyopaque, node_id: []const u8, reason: []const u8) void,
};

const PendingCall = struct {
    gpa: std.mem.Allocator,
    io: std.Io,
    mu: std.Io.Mutex = .init,
    cond: std.Io.Condition = .init,
    done: bool = false,
    overflow: bool = false,
    kind: frame.Kind = .err,
    payload: []u8 = &.{},
    chunks: std.ArrayList(u8) = .empty,

    fn init(gpa: std.mem.Allocator, io: std.Io) PendingCall {
        return .{ .gpa = gpa, .io = io };
    }

    fn deinit(self: *PendingCall) void {
        if (self.payload.len > 0) self.gpa.free(self.payload);
        self.chunks.deinit(self.gpa);
    }

    fn wait_done(self: *PendingCall, timeout_ms: i64) bool {
        self.mu.lockUncancelable(self.io);
        defer self.mu.unlock(self.io);
        if (!self.done) {
            self.cond.waitTimeout(self.io, &self.mu, .{
                .duration = .{ .raw = .fromMilliseconds(timeout_ms), .clock = .awake },
            }) catch {};
        }
        return self.done;
    }
};

const PeerSession = struct {
    gpa: std.mem.Allocator,
    io: std.Io,
    node_id: []const u8,
    name: []const u8,
    conn: transport.NoiseConn,
    tx_mu: std.Io.Mutex = .init,
    pending_mu: std.Io.Mutex = .init,
    pending: std.AutoHashMapUnmanaged(u64, *PendingCall) = .empty,
    next_id: std.atomic.Value(u64) = .init(1),
    last_rx_ms: std.atomic.Value(i64),
    closed: std.atomic.Value(bool) = .init(false),
    refs: std.atomic.Value(usize) = .init(1),

    fn acquire(self: *PeerSession) void {
        _ = self.refs.fetchAdd(1, .acq_rel);
    }

    fn release(self: *PeerSession) void {
        const previous = self.refs.fetchSub(1, .acq_rel);
        tiger.assert(previous > 0);
        if (previous == 1) {
            self.conn.deinit();
            self.pending.deinit(self.gpa);
            self.gpa.free(self.node_id);
            self.gpa.free(self.name);
            self.gpa.destroy(self);
        }
    }

    fn alloc_rpc_id(self: *PeerSession) u64 {
        return self.next_id.fetchAdd(2, .acq_rel);
    }

    fn call_rpc(
        self: *PeerSession,
        req: msg.Request,
        timeout_ms: i64,
    ) !struct { kind: frame.Kind, payload: []u8, chunks: []u8 } {
        if (self.closed.load(.acquire)) return error.Disconnected;

        const id = self.alloc_rpc_id();
        var pc = PendingCall.init(self.gpa, self.io);
        errdefer pc.deinit();

        {
            self.pending_mu.lockUncancelable(self.io);
            defer self.pending_mu.unlock(self.io);
            if (self.closed.load(.acquire)) return error.Disconnected;
            try self.pending.put(self.gpa, id, &pc);
        }
        defer {
            self.pending_mu.lockUncancelable(self.io);
            _ = self.pending.remove(id);
            self.pending_mu.unlock(self.io);
        }

        {
            self.tx_mu.lockUncancelable(self.io);
            defer self.tx_mu.unlock(self.io);
            try self.conn.send_json(.request, id, req);
        }

        if (!pc.wait_done(timeout_ms)) {
            return error.Timeout;
        }
        if (pc.overflow) return error.ReplyTooLarge;
        const pay = pc.payload;
        pc.payload = &.{};
        const ch = try pc.chunks.toOwnedSlice(self.gpa);
        return .{ .kind = pc.kind, .payload = pay, .chunks = ch };
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
    srv: ?std.Io.net.Server = null,
    accept_thread: ?std.Thread = null,
    peers_mu: std.Io.Mutex = .init,
    peers: std.StringHashMapUnmanaged(*PeerSession) = .empty,
    conns_mu: std.Io.Mutex = .init,
    conns_cond: std.Io.Condition = .init,
    conns: ?*ConnArg = null,
    gate: limits.ConnGate = .init(limits.fleet_conns_max),

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
        const addr = try std.Io.net.IpAddress.parse(self.host, self.port);
        var srv = try addr.listen(self.io, .{ .reuse_address = true });
        errdefer srv.deinit(self.io);
        self.actual_port.store(srv.socket.address.getPort(), .release);
        self.srv = srv;
        self.accept_thread = try std.Thread.spawn(.{}, accept_loop, .{self});
    }

    pub fn listen_port(self: *FleetServer) u16 {
        return self.actual_port.load(.acquire);
    }

    pub fn shutdown(self: *FleetServer) void {
        if (self.stopping.swap(true, .acq_rel)) return;
        const port = self.actual_port.load(.acquire);
        if (port != 0) {
            const wake_host = if (std.mem.eql(u8, self.host, "0.0.0.0"))
                "127.0.0.1"
            else if (std.mem.eql(u8, self.host, "::"))
                "::1"
            else
                self.host;
            if (std.Io.net.IpAddress.parse(wake_host, port)) |a| {
                if (a.connect(self.io, .{ .mode = .stream, .protocol = .tcp })) |st| st.close(self.io) else |_| {}
            } else |_| {}
        }
        if (self.accept_thread) |t| t.join();
        if (self.srv) |*s| {
            s.deinit(self.io);
            self.srv = null;
        }
        self.conns_mu.lockUncancelable(self.io);
        var current = self.conns;
        while (current) |conn| : (current = conn.next) {
            if (conn.active) conn.stream.shutdown(self.io, .both) catch {};
        }
        self.conns_mu.unlock(self.io);
    }

    pub fn stop_and_deinit(self: *FleetServer) void {
        self.shutdown();
        self.conns_mu.lockUncancelable(self.io);
        while (self.conns != null) {
            self.conns_cond.waitUncancelable(self.io, &self.conns_mu);
        }
        self.conns_mu.unlock(self.io);
        tiger.assert(self.peers.count() == 0);
        self.peers.deinit(self.gpa);
        self.gpa.free(self.host);
    }

    pub fn sweep_stale(self: *FleetServer, max_idle_ms: i64) void {
        const now_ms = std.Io.Clock.awake.now(self.io).toMilliseconds();
        var stale: std.ArrayList(*PeerSession) = .empty;
        defer stale.deinit(self.gpa);

        {
            self.peers_mu.lockUncancelable(self.io);
            defer self.peers_mu.unlock(self.io);
            var it = self.peers.iterator();
            while (it.next()) |e| {
                const ps = e.value_ptr.*;
                if (now_ms - ps.last_rx_ms.load(.acquire) > max_idle_ms) {
                    ps.acquire();
                    stale.append(self.gpa, ps) catch ps.release();
                }
            }
        }

        for (stale.items) |ps| {
            ps.closed.store(true, .release);
            ps.conn.shutdown();
            ps.release();
        }
    }

    pub fn disconnect_node(self: *FleetServer, node_id: []const u8) void {
        self.peers_mu.lockUncancelable(self.io);
        const removed = self.peers.fetchRemove(node_id);
        self.peers_mu.unlock(self.io);
        if (removed) |kv| {
            self.gpa.free(kv.key);
            kv.value.closed.store(true, .release);
            kv.value.conn.shutdown();
            kv.value.release();
        }
    }

    fn get_peer(self: *FleetServer, node_id: []const u8) ?*PeerSession {
        self.peers_mu.lockUncancelable(self.io);
        defer self.peers_mu.unlock(self.io);
        if (self.stopping.load(.acquire)) return null;
        if (self.peers.get(node_id)) |ps| {
            ps.acquire();
            return ps;
        }
        return null;
    }

    pub fn submit_job(self: *FleetServer, arena: std.mem.Allocator, node_id: []const u8, sj: msg.SubmitJob) !msg.JobInfo {
        const ps = self.get_peer(node_id) orelse return error.NodeOffline;
        defer ps.release();
        const res = try ps.call_rpc(.{ .submit_job = sj }, 15_000);
        defer {
            self.gpa.free(res.payload);
            self.gpa.free(res.chunks);
        }
        if (res.kind == .err) return error.RemoteError;
        const rep = try msg.decode_arena(msg.JobReply, arena, res.payload);
        return rep.job;
    }

    pub fn job_status(self: *FleetServer, arena: std.mem.Allocator, node_id: []const u8, job_id: []const u8) !msg.JobInfo {
        const ps = self.get_peer(node_id) orelse return error.NodeOffline;
        defer ps.release();
        const res = try ps.call_rpc(.{ .job_status = .{ .job_id = job_id } }, 10_000);
        defer {
            self.gpa.free(res.payload);
            self.gpa.free(res.chunks);
        }
        if (res.kind == .err) return error.RemoteError;
        const rep = try msg.decode_arena(msg.JobReply, arena, res.payload);
        return rep.job;
    }

    pub fn cancel_job(self: *FleetServer, arena: std.mem.Allocator, node_id: []const u8, job_id: []const u8) !msg.JobInfo {
        const ps = self.get_peer(node_id) orelse return error.NodeOffline;
        defer ps.release();
        const res = try ps.call_rpc(.{ .cancel_job = .{ .job_id = job_id } }, 10_000);
        defer {
            self.gpa.free(res.payload);
            self.gpa.free(res.chunks);
        }
        if (res.kind == .err) return error.RemoteError;
        const rep = try msg.decode_arena(msg.JobReply, arena, res.payload);
        return rep.job;
    }

    pub fn fetch_log(
        self: *FleetServer,
        arena: std.mem.Allocator,
        node_id: []const u8,
        job_id: []const u8,
        offset: u64,
        limit: usize,
    ) !struct { data: []const u8, next_offset: u64, eof: bool } {
        const ps = self.get_peer(node_id) orelse return error.NodeOffline;
        defer ps.release();
        const piece = @min(limit, limits.piece_max);
        const res = try ps.call_rpc(.{ .fetch_log = .{
            .job_id = job_id,
            .offset = offset,
            .limit = piece,
        } }, 10_000);
        defer {
            self.gpa.free(res.payload);
            self.gpa.free(res.chunks);
        }
        if (res.kind == .err) return error.RemoteError;
        if (res.chunks.len > piece) return error.ReplyTooLarge;
        const le = try msg.decode_arena(msg.LogEnd, arena, res.payload);
        return .{
            .data = try arena.dupe(u8, res.chunks),
            .next_offset = le.next_offset,
            .eof = le.eof,
        };
    }

    /// Streams one artifact into `artifacts_root/job_id/rel_path` through a temporary file.
    /// `declared_size` is the size the node reported in the job status; it is refused above the cap
    /// before any transfer, and the received bytes are checked against it and the cap piece by piece.
    pub fn fetch_artifact(
        self: *FleetServer,
        arena: std.mem.Allocator,
        node_id: []const u8,
        job_id: []const u8,
        rel_path: []const u8,
        artifacts_root: []const u8,
        declared_size: u64,
    ) !msg.JobArtifact {
        const ps = self.get_peer(node_id) orelse return error.NodeOffline;
        defer ps.release();

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
            const res = try ps.call_rpc(.{ .fetch_artifact = .{
                .job_id = job_id,
                .path = rel_path,
                .offset = sink.written,
                .limit = limits.piece_max,
            } }, 20_000);
            defer {
                self.gpa.free(res.payload);
                self.gpa.free(res.chunks);
            }
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
        const ps = self.get_peer(node_id) orelse return error.NodeOffline;
        defer ps.release();
        const res = try ps.call_rpc(.{ .update = upd }, 60_000);
        defer {
            self.gpa.free(res.payload);
            self.gpa.free(res.chunks);
        }
        if (res.kind == .err) return error.RemoteError;
        return try msg.decode_arena(msg.UpdateReply, arena, res.payload);
    }

    pub fn upload_update(
        self: *FleetServer,
        arena: std.mem.Allocator,
        node_id: []const u8,
        up: msg.UploadUpdate,
    ) !msg.UploadUpdateReply {
        const ps = self.get_peer(node_id) orelse return error.NodeOffline;
        defer ps.release();
        const res = try ps.call_rpc(.{ .upload_update = up }, 30_000);
        defer {
            self.gpa.free(res.payload);
            self.gpa.free(res.chunks);
        }
        if (res.kind == .err) return error.RemoteError;
        return try msg.decode_arena(msg.UploadUpdateReply, arena, res.payload);
    }

    pub fn ack_job(self: *FleetServer, arena: std.mem.Allocator, node_id: []const u8, job_id: []const u8) !msg.AckJobReply {
        const ps = self.get_peer(node_id) orelse return error.NodeOffline;
        defer ps.release();
        const res = try ps.call_rpc(.{ .ack_job = .{ .job_id = job_id } }, 10_000);
        defer {
            self.gpa.free(res.payload);
            self.gpa.free(res.chunks);
        }
        if (res.kind == .err) return error.RemoteError;
        return try msg.decode_arena(msg.AckJobReply, arena, res.payload);
    }

    pub fn acquire_lease(
        self: *FleetServer,
        arena: std.mem.Allocator,
        node_id: []const u8,
        req: msg.AcquireLease,
    ) !msg.LeaseReply {
        const ps = self.get_peer(node_id) orelse return error.NodeOffline;
        defer ps.release();
        const extra: u32 = if (req.wait_ms) |w| @intCast(@min(@as(i64, 300_000), @max(@as(i64, 0), w))) else 0;
        const res = try ps.call_rpc(.{ .acquire_lease = req }, 15_000 + extra);
        defer {
            self.gpa.free(res.payload);
            self.gpa.free(res.chunks);
        }
        if (res.kind == .err) return error.RemoteError;
        return try msg.decode_arena(msg.LeaseReply, arena, res.payload);
    }

    pub fn renew_lease(self: *FleetServer, arena: std.mem.Allocator, node_id: []const u8, req: msg.RenewLease) !msg.LeaseReply {
        const ps = self.get_peer(node_id) orelse return error.NodeOffline;
        defer ps.release();
        const res = try ps.call_rpc(.{ .renew_lease = req }, 10_000);
        defer {
            self.gpa.free(res.payload);
            self.gpa.free(res.chunks);
        }
        if (res.kind == .err) return error.RemoteError;
        return try msg.decode_arena(msg.LeaseReply, arena, res.payload);
    }

    pub fn release_lease(self: *FleetServer, arena: std.mem.Allocator, node_id: []const u8, req: msg.ReleaseLease) !msg.ReleaseLeaseReply {
        const ps = self.get_peer(node_id) orelse return error.NodeOffline;
        defer ps.release();
        const res = try ps.call_rpc(.{ .release_lease = req }, 15_000);
        defer {
            self.gpa.free(res.payload);
            self.gpa.free(res.chunks);
        }
        if (res.kind == .err) return error.RemoteError;
        return try msg.decode_arena(msg.ReleaseLeaseReply, arena, res.payload);
    }

    pub fn verify_epoch(self: *FleetServer, arena: std.mem.Allocator, node_id: []const u8, req: msg.VerifyEpoch) !msg.VerifyEpochReply {
        const ps = self.get_peer(node_id) orelse return error.NodeOffline;
        defer ps.release();
        const res = try ps.call_rpc(.{ .verify_epoch = req }, 10_000);
        defer {
            self.gpa.free(res.payload);
            self.gpa.free(res.chunks);
        }
        if (res.kind == .err) return error.RemoteError;
        return try msg.decode_arena(msg.VerifyEpochReply, arena, res.payload);
    }

    pub fn list_leases(self: *FleetServer, arena: std.mem.Allocator, node_id: []const u8) !msg.AdmissionStatus {
        const ps = self.get_peer(node_id) orelse return error.NodeOffline;
        defer ps.release();
        const res = try ps.call_rpc(.{ .list_leases = .{} }, 10_000);
        defer {
            self.gpa.free(res.payload);
            self.gpa.free(res.chunks);
        }
        if (res.kind == .err) return error.RemoteError;
        return try msg.decode_arena(msg.AdmissionStatus, arena, res.payload);
    }

    pub fn clear_quarantine(self: *FleetServer, arena: std.mem.Allocator, node_id: []const u8) !msg.ClearQuarantineReply {
        const ps = self.get_peer(node_id) orelse return error.NodeOffline;
        defer ps.release();
        const res = try ps.call_rpc(.{ .clear_quarantine = .{} }, 10_000);
        defer {
            self.gpa.free(res.payload);
            self.gpa.free(res.chunks);
        }
        if (res.kind == .err) return error.RemoteError;
        return try msg.decode_arena(msg.ClearQuarantineReply, arena, res.payload);
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

    fn accept_loop(self: *FleetServer) void {
        while (!self.stopping.load(.acquire)) {
            const s = &(self.srv orelse break);
            const stream = s.accept(self.io) catch {
                if (self.stopping.load(.acquire)) break;
                continue;
            };
            if (self.stopping.load(.acquire)) {
                stream.close(self.io);
                break;
            }
            switch (self.gate.try_enter()) {
                .admitted => {},
                .refused_first => {
                    std.debug.print("fleet: {d} connections open, refusing new sockets\n", .{self.gate.max});
                    stream.close(self.io);
                    continue;
                },
                .refused => {
                    stream.close(self.io);
                    continue;
                },
            }
            const ctx = self.gpa.create(ConnArg) catch {
                self.gate.leave();
                stream.close(self.io);
                continue;
            };
            ctx.* = .{ .fleet = self, .stream = stream };
            self.conns_mu.lockUncancelable(self.io);
            ctx.next = self.conns;
            self.conns = ctx;
            self.conns_mu.unlock(self.io);
            const t = std.Thread.spawn(.{}, peer_conn_thread, .{ctx}) catch {
                ctx.finish();
                stream.close(self.io);
                continue;
            };
            t.detach();
        }
    }
};

const ConnArg = struct {
    fleet: *FleetServer,
    stream: std.Io.net.Stream,
    next: ?*ConnArg = null,
    active: bool = true,

    fn deactivate(self: *ConnArg) void {
        self.fleet.conns_mu.lockUncancelable(self.fleet.io);
        self.active = false;
        self.fleet.conns_mu.unlock(self.fleet.io);
    }

    fn finish(self: *ConnArg) void {
        const fleet = self.fleet;
        fleet.conns_mu.lockUncancelable(fleet.io);
        var link = &fleet.conns;
        while (link.* != self) link = &link.*.?.next;
        link.* = self.next;
        fleet.gate.leave();
        fleet.gpa.destroy(self);
        fleet.conns_cond.broadcast(fleet.io);
        fleet.conns_mu.unlock(fleet.io);
    }
};

fn peer_conn_thread(arg: *ConnArg) void {
    const self = arg.fleet;
    const stream = arg.stream;
    defer arg.finish();

    var arena_st = std.heap.ArenaAllocator.init(self.gpa);
    defer arena_st.deinit();
    const arena = arena_st.allocator();

    const peers_allowed = self.hooks.get_allowed_peers(self.hooks.ctx, arena);
    if (peers_allowed.len == 0) {
        arg.deactivate();
        stream.close(self.io);
        return;
    }
    const keys = arena.alloc([32]u8, peers_allowed.len) catch {
        arg.deactivate();
        stream.close(self.io);
        return;
    };
    for (peers_allowed, 0..) |ap, i| keys[i] = ap.pub_key;

    var conn = transport.NoiseConn.accept_on_chan(self.gpa, self.io, .{ .tcp = stream }, self.hub_kp, keys) catch {
        arg.deactivate();
        stream.close(self.io);
        return;
    };

    var matched: ?AllowedPeer = null;
    for (peers_allowed) |ap| {
        if (std.crypto.timing_safe.eql([32]u8, ap.pub_key, conn.sess.peer_static)) {
            matched = ap;
            break;
        }
    }
    const peer = matched orelse {
        arg.deactivate();
        conn.deinit();
        return;
    };

    var hello_frame = conn.recv_frame() catch {
        arg.deactivate();
        conn.deinit();
        return;
    };
    defer hello_frame.deinit(self.gpa);

    if (hello_frame.kind != .request) {
        arg.deactivate();
        conn.deinit();
        return;
    }
    const h_req = msg.decode_arena(msg.Request, arena, hello_frame.payload) catch {
        arg.deactivate();
        conn.deinit();
        return;
    };
    const hello = switch (h_req) {
        .hello => |h| h,
        else => {
            arg.deactivate();
            conn.deinit();
            return;
        },
    };

    conn.send_json(.response, hello_frame.id, msg.HelloReply{
        .ok = true,
        .hub_version = version.version,
        .node_name = peer.name,
    }) catch {
        arg.deactivate();
        conn.deinit();
        return;
    };

    const ps = self.gpa.create(PeerSession) catch {
        arg.deactivate();
        conn.deinit();
        return;
    };
    ps.* = .{
        .gpa = self.gpa,
        .io = self.io,
        .node_id = self.gpa.dupe(u8, peer.node_id) catch {
            arg.deactivate();
            self.gpa.destroy(ps);
            conn.deinit();
            return;
        },
        .name = self.gpa.dupe(u8, peer.name) catch {
            arg.deactivate();
            self.gpa.free(ps.node_id);
            self.gpa.destroy(ps);
            conn.deinit();
            return;
        },
        .conn = conn,
        .last_rx_ms = .init(std.Io.Clock.awake.now(self.io).toMilliseconds()),
    };

    {
        self.peers_mu.lockUncancelable(self.io);
        if (self.stopping.load(.acquire)) {
            self.peers_mu.unlock(self.io);
            arg.deactivate();
            ps.release();
            return;
        }
        const kdup = self.gpa.dupe(u8, ps.node_id) catch {
            self.peers_mu.unlock(self.io);
            arg.deactivate();
            ps.release();
            return;
        };
        self.peers.ensureUnusedCapacity(self.gpa, 1) catch {
            self.gpa.free(kdup);
            self.peers_mu.unlock(self.io);
            arg.deactivate();
            ps.release();
            return;
        };
        const old = self.peers.fetchRemove(ps.node_id);
        ps.acquire();
        self.peers.putAssumeCapacity(kdup, ps);
        self.peers_mu.unlock(self.io);
        if (old) |okv| {
            self.gpa.free(okv.key);
            okv.value.closed.store(true, .release);
            okv.value.conn.shutdown();
            okv.value.release();
        }
    }

    self.hooks.on_connected(self.hooks.ctx, ps.node_id, hello);

    var frame_arena_st = std.heap.ArenaAllocator.init(self.gpa);
    defer frame_arena_st.deinit();
    var disconnect_reason: []const u8 = "closed";

    while (!self.stopping.load(.acquire) and !ps.closed.load(.acquire)) {
        var f = ps.conn.recv_frame() catch |err| {
            disconnect_reason = @errorName(err);
            break;
        };
        defer f.deinit(self.gpa);
        ps.last_rx_ms.store(std.Io.Clock.awake.now(self.io).toMilliseconds(), .release);

        switch (f.kind) {
            .request => {
                _ = frame_arena_st.reset(.retain_capacity);
                if (msg.decode_arena(msg.Request, frame_arena_st.allocator(), f.payload)) |req| {
                    switch (req) {
                        .hello => |tel| {
                            self.hooks.on_telemetry(self.hooks.ctx, ps.node_id, tel);
                            ps.tx_mu.lockUncancelable(self.io);
                            ps.conn.send_json(.response, f.id, msg.HelloReply{
                                .ok = true,
                                .hub_version = version.version,
                                .node_name = ps.name,
                            }) catch {};
                            ps.tx_mu.unlock(self.io);
                        },
                        else => {},
                    }
                } else |_| {}
            },
            .chunk => {
                ps.pending_mu.lockUncancelable(self.io);
                const call = ps.pending.get(f.id);
                if (call) |pc| {
                    pc.mu.lockUncancelable(self.io);
                    if (!pc.overflow and !pc.done) {
                        if (f.payload.len > limits.rpc_chunks_max - pc.chunks.items.len) {
                            pc.overflow = true;
                        } else {
                            pc.chunks.appendSlice(pc.gpa, f.payload) catch {
                                pc.overflow = true;
                            };
                        }
                        if (pc.overflow) {
                            pc.chunks.clearAndFree(pc.gpa);
                            pc.done = true;
                            pc.cond.signal(self.io);
                        }
                    }
                    pc.mu.unlock(self.io);
                }
                ps.pending_mu.unlock(self.io);
            },
            .response, .end, .err => {
                ps.pending_mu.lockUncancelable(self.io);
                const call = ps.pending.get(f.id);
                if (call) |pc| {
                    pc.mu.lockUncancelable(self.io);
                    if (!pc.overflow) {
                        pc.kind = f.kind;
                        if (pc.payload.len > 0) pc.gpa.free(pc.payload);
                        pc.payload = pc.gpa.dupe(u8, f.payload) catch &.{};
                        pc.done = true;
                        pc.cond.signal(self.io);
                    }
                    pc.mu.unlock(self.io);
                }
                ps.pending_mu.unlock(self.io);
            },
        }
    }

    ps.closed.store(true, .release);
    ps.pending_mu.lockUncancelable(self.io);
    var pit = ps.pending.iterator();
    while (pit.next()) |pe| {
        const pc = pe.value_ptr.*;
        pc.mu.lockUncancelable(self.io);
        pc.kind = .err;
        pc.done = true;
        pc.cond.signal(self.io);
        pc.mu.unlock(self.io);
    }
    ps.pending_mu.unlock(self.io);

    var was_current = false;
    {
        self.peers_mu.lockUncancelable(self.io);
        if (self.peers.get(ps.node_id)) |cur| {
            if (cur == ps) {
                if (self.peers.fetchRemove(ps.node_id)) |kv| {
                    self.gpa.free(kv.key);
                    was_current = true;
                }
            }
        }
        self.peers_mu.unlock(self.io);
    }
    if (was_current) {
        self.hooks.on_disconnected(self.hooks.ctx, ps.node_id, disconnect_reason);
        ps.release();
    }
    arg.deactivate();
    ps.release();
}

fn parse_host_port(raw: []const u8, def_port: u16) struct { host: []const u8, port: u16 } {
    if (std.mem.findScalarLast(u8, raw, ':')) |colon| {
        if (std.fmt.parseInt(u16, raw[colon + 1 ..], 10)) |p| {
            return .{ .host = raw[0..colon], .port = p };
        } else |_| {}
    }
    return .{ .host = raw, .port = def_port };
}
