const std = @import("std");
const tiger = @import("../tiger.zig");
const version = @import("../version.zig");
const noise = @import("../net/noise.zig");
const frame = @import("../net/frame.zig");
const msg = @import("../net/msg.zig");
const transport = @import("../net/transport.zig");

const Sha256 = std.crypto.hash.sha2.Sha256;

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
        if (self.refs.fetchSub(1, .acq_rel) == 1) {
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

    pub fn stop_and_deinit(self: *FleetServer) void {
        self.stopping.store(true, .release);
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

        self.peers_mu.lockUncancelable(self.io);
        var vals: std.ArrayList(*PeerSession) = .empty;
        var it = self.peers.iterator();
        while (it.next()) |e| {
            self.gpa.free(e.key_ptr.*);
            vals.append(self.gpa, e.value_ptr.*) catch {};
        }
        self.peers.deinit(self.gpa);
        self.peers_mu.unlock(self.io);

        for (vals.items) |ps| {
            ps.closed.store(true, .release);
            ps.conn.shutdown();
            ps.release();
        }
        vals.deinit(self.gpa);
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
        const res = try ps.call_rpc(.{ .fetch_log = .{
            .job_id = job_id,
            .offset = offset,
            .limit = limit,
        } }, 10_000);
        defer {
            self.gpa.free(res.payload);
            self.gpa.free(res.chunks);
        }
        if (res.kind == .err) return error.RemoteError;
        const le = try msg.decode_arena(msg.LogEnd, arena, res.payload);
        return .{
            .data = try arena.dupe(u8, res.chunks),
            .next_offset = le.next_offset,
            .eof = le.eof,
        };
    }

    pub fn fetch_artifact(
        self: *FleetServer,
        arena: std.mem.Allocator,
        node_id: []const u8,
        job_id: []const u8,
        rel_path: []const u8,
        artifacts_root: []const u8,
    ) !msg.JobArtifact {
        const ps = self.get_peer(node_id) orelse return error.NodeOffline;
        defer ps.release();

        var full_data: std.ArrayList(u8) = .empty;
        var off: u64 = 0;
        var expected_sha: []const u8 = "";
        var total_size: u64 = 0;

        while (true) {
            const res = try ps.call_rpc(.{ .fetch_artifact = .{
                .job_id = job_id,
                .path = rel_path,
                .offset = off,
                .limit = 256 * 1024,
            } }, 20_000);
            defer {
                self.gpa.free(res.payload);
                self.gpa.free(res.chunks);
            }
            if (res.kind == .err) return error.RemoteError;
            try full_data.appendSlice(arena, res.chunks);
            const ae = try msg.decode_arena(msg.ArtifactEnd, arena, res.payload);
            off = ae.next_offset;
            expected_sha = ae.sha256;
            total_size = ae.size_bytes;
            if (ae.eof) break;
        }

        var digest: [32]u8 = undefined;
        Sha256.hash(full_data.items, &digest, .{});
        const chars = "0123456789abcdef";
        var hex: [64]u8 = undefined;
        for (digest, 0..) |b, i| {
            hex[i * 2] = chars[b >> 4];
            hex[i * 2 + 1] = chars[b & 0x0f];
        }
        if (expected_sha.len == 64 and !std.mem.eql(u8, &hex, expected_sha)) {
            return error.ChecksumMismatch;
        }

        const job_art_dir = try std.fs.path.join(arena, &.{ artifacts_root, job_id });
        try std.Io.Dir.cwd().createDirPath(self.io, job_art_dir);
        const dest = try std.fs.path.join(arena, &.{ job_art_dir, rel_path });
        if (std.fs.path.dirname(dest)) |pd| {
            try std.Io.Dir.cwd().createDirPath(self.io, pd);
        }
        try std.Io.Dir.cwd().writeFile(self.io, .{ .sub_path = dest, .data = full_data.items });

        return .{
            .path = try arena.dupe(u8, rel_path),
            .size_bytes = total_size,
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
            const ctx = self.gpa.create(ConnArg) catch {
                stream.close(self.io);
                continue;
            };
            ctx.* = .{ .fleet = self, .stream = stream };
            const t = std.Thread.spawn(.{}, peer_conn_thread, .{ctx}) catch {
                self.gpa.destroy(ctx);
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
};

fn peer_conn_thread(arg: *ConnArg) void {
    const self = arg.fleet;
    const stream = arg.stream;
    self.gpa.destroy(arg);

    var arena_st = std.heap.ArenaAllocator.init(self.gpa);
    defer arena_st.deinit();
    const arena = arena_st.allocator();

    const peers_allowed = self.hooks.get_allowed_peers(self.hooks.ctx, arena);
    if (peers_allowed.len == 0) {
        stream.close(self.io);
        return;
    }
    const keys = arena.alloc([32]u8, peers_allowed.len) catch {
        stream.close(self.io);
        return;
    };
    for (peers_allowed, 0..) |ap, i| keys[i] = ap.pub_key;

    var conn = transport.NoiseConn.accept_on_chan(self.gpa, self.io, .{ .tcp = stream }, self.hub_kp, keys) catch {
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
        conn.deinit();
        return;
    };

    var hello_frame = conn.recv_frame() catch {
        conn.deinit();
        return;
    };
    defer hello_frame.deinit(self.gpa);

    if (hello_frame.kind != .request) {
        conn.deinit();
        return;
    }
    const h_req = msg.decode_arena(msg.Request, arena, hello_frame.payload) catch {
        conn.deinit();
        return;
    };
    const hello = switch (h_req) {
        .hello => |h| h,
        else => {
            conn.deinit();
            return;
        },
    };

    conn.send_json(.response, hello_frame.id, msg.HelloReply{
        .ok = true,
        .hub_version = version.version,
        .node_name = peer.name,
    }) catch {
        conn.deinit();
        return;
    };

    const ps = self.gpa.create(PeerSession) catch {
        conn.deinit();
        return;
    };
    ps.* = .{
        .gpa = self.gpa,
        .io = self.io,
        .node_id = self.gpa.dupe(u8, peer.node_id) catch {
            self.gpa.destroy(ps);
            conn.deinit();
            return;
        },
        .name = self.gpa.dupe(u8, peer.name) catch {
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
        const old = self.peers.fetchRemove(ps.node_id);
        const kdup = self.gpa.dupe(u8, ps.node_id) catch {
            self.peers_mu.unlock(self.io);
            ps.release();
            return;
        };
        self.peers.put(self.gpa, kdup, ps) catch {
            self.gpa.free(kdup);
            self.peers_mu.unlock(self.io);
            ps.release();
            return;
        };
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
                    pc.chunks.appendSlice(pc.gpa, f.payload) catch {};
                    pc.mu.unlock(self.io);
                }
                ps.pending_mu.unlock(self.io);
            },
            .response, .end, .err => {
                ps.pending_mu.lockUncancelable(self.io);
                const call = ps.pending.get(f.id);
                if (call) |pc| {
                    pc.mu.lockUncancelable(self.io);
                    pc.kind = f.kind;
                    if (pc.payload.len > 0) pc.gpa.free(pc.payload);
                    pc.payload = pc.gpa.dupe(u8, f.payload) catch &.{};
                    pc.done = true;
                    pc.cond.signal(self.io);
                    pc.mu.unlock(self.io);
                }
                ps.pending_mu.unlock(self.io);
            },
        }
    }

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
    }
    ps.closed.store(true, .release);
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
