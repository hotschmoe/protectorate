const std = @import("std");
const builtin = @import("builtin");
const tiger = @import("../tiger.zig");
const version = @import("../version.zig");
const json_util = @import("../json_util.zig");
const noise = @import("../net/noise.zig");
const frame = @import("../net/frame.zig");
const msg = @import("../net/msg.zig");
const transport = @import("../net/transport.zig");
const u_ver = @import("../update/version.zig");
const u_store = @import("../update/store.zig");
const sys = @import("sys.zig");
const runner = @import("runner.zig");

pub const NodeConfig = struct {
    node_id: []const u8,
    name: []const u8,
    hub_public_key: []const u8,
    hub_fingerprint: []const u8 = "",
    addrs: []const []const u8,
    authorized: bool = false,
};

pub fn resolve_node_home(
    gpa: std.mem.Allocator,
    env: *const std.process.Environ.Map,
    override: ?[]const u8,
) ![]u8 {
    if (override) |o| return gpa.dupe(u8, o);
    if (env.get("PROTECTORATE_NODE_HOME")) |e| return gpa.dupe(u8, e);
    if (env.get("HOME")) |h| return std.fs.path.join(gpa, &.{ h, ".local/share/protectorate-node" });
    if (env.get("USERPROFILE")) |h| return std.fs.path.join(gpa, &.{ h, ".protectorate-node" });
    return gpa.dupe(u8, ".protectorate-node");
}

pub fn load_or_create_key(gpa: std.mem.Allocator, io: std.Io, home_path: []const u8) !noise.KeyPair {
    var dir = try std.Io.Dir.cwd().createDirPathOpen(io, home_path, .{});
    defer dir.close(io);

    if (dir.readFileAlloc(io, "node_key.hex", gpa, .limited(512))) |raw| {
        defer gpa.free(raw);
        const tr = std.mem.trim(u8, raw, " \t\r\n");
        if (tr.len == 64) {
            var secret: [32]u8 = undefined;
            if (std.fmt.hexToBytes(&secret, tr)) |_| {
                return noise.KeyPair.from_secret(secret);
            } else |_| {}
        }
    } else |_| {}

    const kp = noise.KeyPair.generate(io);
    const hex = hex_encode32(kp.secret);
    var f = try dir.createFile(io, "node_key.tmp", .{ .read = true, .truncate = true });
    defer f.close(io);
    if (comptime builtin.os.tag != .windows) {
        f.setPermissions(io, .fromMode(0o600)) catch {};
    }
    try f.writePositionalAll(io, &hex, 0);
    try f.sync(io);
    try std.Io.Dir.rename(dir, "node_key.tmp", dir, "node_key.hex", io);
    return kp;
}

fn hex_encode32(bytes: [32]u8) [64]u8 {
    const chars = "0123456789abcdef";
    var out: [64]u8 = undefined;
    for (bytes, 0..) |b, i| {
        out[i * 2] = chars[b >> 4];
        out[i * 2 + 1] = chars[b & 0x0f];
    }
    return out;
}

pub fn load_config(arena: std.mem.Allocator, io: std.Io, home_path: []const u8) !NodeConfig {
    const cfg_path = try std.fs.path.join(arena, &.{ home_path, "config.json" });
    const raw = try std.Io.Dir.cwd().readFileAlloc(io, cfg_path, arena, .limited(1 << 20));
    const val = try std.json.parseFromSliceLeaky(std.json.Value, arena, raw, .{});
    return std.json.parseFromValueLeaky(NodeConfig, arena, val, .{
        .ignore_unknown_fields = true,
        .allocate = .alloc_always,
    });
}

pub fn save_config(gpa: std.mem.Allocator, io: std.Io, home_path: []const u8, cfg: NodeConfig) !void {
    var dir = try std.Io.Dir.cwd().createDirPathOpen(io, home_path, .{});
    defer dir.close(io);
    const json = try std.json.Stringify.valueAlloc(gpa, cfg, .{ .whitespace = .indent_2 });
    defer gpa.free(json);
    try dir.writeFile(io, .{ .sub_path = "config.json.tmp", .data = json });
    try std.Io.Dir.rename(dir, "config.json.tmp", dir, "config.json", io);
}

pub const JoinPayload = struct {
    hub_public_key: []const u8,
    hub_fingerprint: []const u8 = "",
    addrs: []const []const u8 = &.{},
};

pub fn decode_join_token(arena: std.mem.Allocator, raw_tok: []const u8) !JoinPayload {
    const prefix = "ptjoin1:";
    if (!std.mem.startsWith(u8, raw_tok, prefix)) return error.BadJoinToken;
    const b64 = raw_tok[prefix.len..];
    const dec_len = try std.base64.url_safe_no_pad.Decoder.calcSizeForSlice(b64);
    const buf = try arena.alloc(u8, dec_len);
    try std.base64.url_safe_no_pad.Decoder.decode(buf, b64);
    const val = try std.json.parseFromSliceLeaky(std.json.Value, arena, buf, .{});
    return std.json.parseFromValueLeaky(JoinPayload, arena, val, .{
        .ignore_unknown_fields = true,
        .allocate = .alloc_always,
    });
}

pub fn cmd_join(
    gpa: std.mem.Allocator,
    io: std.Io,
    home_path: []const u8,
    token_opt: ?[]const u8,
    hub_addr_opt: ?[]const u8,
    hub_key_opt: ?[]const u8,
    name_opt: ?[]const u8,
) !u8 {
    var arena_st = std.heap.ArenaAllocator.init(gpa);
    defer arena_st.deinit();
    const arena = arena_st.allocator();

    var kp = try load_or_create_key(gpa, io, home_path);
    defer kp.zeroize();

    const node_pub_hex = hex_encode32(kp.public);
    const node_fp = noise.fingerprint_prefixed(kp.public);
    var hn_buf: [256]u8 = undefined;
    const node_name = name_opt orelse sys.default_hostname(&hn_buf);

    var hub_pub_hex: []const u8 = "";
    var hub_fp: []const u8 = "";
    var addrs: []const []const u8 = &.{};

    if (token_opt) |tok| {
        const jp = try decode_join_token(arena, tok);
        hub_pub_hex = jp.hub_public_key;
        hub_fp = jp.hub_fingerprint;
        addrs = jp.addrs;
    } else if (hub_addr_opt != null and hub_key_opt != null) {
        hub_pub_hex = hub_key_opt.?;
        addrs = &.{hub_addr_opt.?};
    } else {
        try print_err(io, "usage: protectorate-node join <ptjoin1:...> [--name <name>]\n", .{});
        return 2;
    }

    var hub_pub: [32]u8 = undefined;
    if (hub_pub_hex.len != 64) return error.InvalidHubKey;
    _ = try std.fmt.hexToBytes(&hub_pub, hub_pub_hex);
    var hub_fp_buf: [71]u8 = undefined;
    if (hub_fp.len == 0) {
        hub_fp_buf = noise.fingerprint_prefixed(hub_pub);
        hub_fp = &hub_fp_buf;
    }

    const cfg: NodeConfig = .{
        .node_id = &node_pub_hex,
        .name = node_name,
        .hub_public_key = hub_pub_hex,
        .hub_fingerprint = hub_fp,
        .addrs = addrs,
        .authorized = false,
    };
    try save_config(gpa, io, home_path, cfg);

    var obuf: [2048]u8 = undefined;
    var w = std.Io.File.stdout().writer(io, &obuf);
    try w.interface.print(
        \\node configured
        \\  name:            {s}
        \\  node_key:        {s}
        \\  node_fp:         {s}
        \\  hub_fingerprint: {s}
        \\  home:            {s}
        \\
    , .{ node_name, node_pub_hex, node_fp, hub_fp, home_path });
    try w.interface.flush();
    return 0;
}

pub fn cmd_authorize(
    gpa: std.mem.Allocator,
    io: std.Io,
    env: *const std.process.Environ.Map,
    home_path: []const u8,
) !u8 {
    var arena_st = std.heap.ArenaAllocator.init(gpa);
    defer arena_st.deinit();
    const arena = arena_st.allocator();

    var cfg = try load_config(arena, io, home_path);
    var kp = try load_or_create_key(gpa, io, home_path);
    defer kp.zeroize();

    var hub_pub: [32]u8 = undefined;
    if (cfg.hub_public_key.len != 64) return error.InvalidHubKey;
    _ = try std.fmt.hexToBytes(&hub_pub, cfg.hub_public_key);

    const inv = try sys.collect_inventory(arena, io, version.version);
    var eps: std.ArrayList(transport.Endpoint) = .empty;
    for (cfg.addrs) |addr_str| {
        const hp = split_host_port(addr_str, 7475) orelse continue;
        try eps.append(arena, .{ .host = hp.host, .port = hp.port });
    }
    if (eps.items.len > 0) {
        if (transport.NoiseConn.dial_any_tcp(gpa, io, eps.items, 3_000, kp, hub_pub)) |res_conn| {
            var conn = res_conn.conn;
            defer conn.deinit();

            try send_hello_sync(gpa, arena, &conn, &cfg, inv, io, env);
            cfg.authorized = true;
            try save_config(gpa, io, home_path, cfg);
            const chosen = eps.items[res_conn.addr_index];
            var obuf: [512]u8 = undefined;
            var w = std.Io.File.stdout().writer(io, &obuf);
            try w.interface.print("authorized against {s}:{d} (hub {s})\n", .{ chosen.host, chosen.port, cfg.hub_fingerprint });
            try w.interface.flush();
            return 0;
        } else |err| {
            try print_err(io, "authorize dial failed: {s}\n", .{@errorName(err)});
        }
    }
    try print_err(io, "authorization failed across configured hub addresses\n", .{});
    return 1;
}

pub fn cmd_status(gpa: std.mem.Allocator, io: std.Io, home_path: []const u8) !u8 {
    var arena_st = std.heap.ArenaAllocator.init(gpa);
    defer arena_st.deinit();
    const arena = arena_st.allocator();

    var kp = try load_or_create_key(gpa, io, home_path);
    defer kp.zeroize();
    const pub_hex = hex_encode32(kp.public);
    const cfg_opt: ?NodeConfig = load_config(arena, io, home_path) catch null;

    var obuf: [1024]u8 = undefined;
    var w = std.Io.File.stdout().writer(io, &obuf);
    try w.interface.print("node_key={s} version={s}\n", .{ pub_hex, version.version });
    if (cfg_opt) |cfg| {
        try w.interface.print("name={s} authorized={} hub_fp={s} addrs={d}\n", .{
            cfg.name,
            cfg.authorized,
            cfg.hub_fingerprint,
            cfg.addrs.len,
        });
    } else {
        try w.interface.print("unconfigured (run protectorate-node join ...)\n", .{});
    }
    try w.interface.flush();
    return 0;
}

pub fn cmd_serve(
    gpa: std.mem.Allocator,
    io: std.Io,
    env: ?*const std.process.Environ.Map,
    home_path: []const u8,
    stdio_mode: bool,
) !u8 {
    var arena_st = std.heap.ArenaAllocator.init(gpa);
    defer arena_st.deinit();
    const arena = arena_st.allocator();

    const cfg = try load_config(arena, io, home_path);
    var kp = try load_or_create_key(gpa, io, home_path);
    defer kp.zeroize();

    var hub_pub: [32]u8 = undefined;
    if (cfg.hub_public_key.len != 64) return error.InvalidHubKey;
    _ = try std.fmt.hexToBytes(&hub_pub, cfg.hub_public_key);

    var run_mgr = try runner.Runner.init(gpa, io, home_path);
    defer run_mgr.deinit();

    const inv = try sys.collect_inventory(arena, io, version.version);

    if (stdio_mode) {
        var conn = try transport.NoiseConn.dial_stdio(
            gpa,
            io,
            std.Io.File.stdin(),
            std.Io.File.stdout(),
            kp,
            hub_pub,
        );
        defer conn.deinit();
        try run_session(gpa, io, home_path, env, &conn, &cfg, inv, &run_mgr);
        return 0;
    }

    var eps: std.ArrayList(transport.Endpoint) = .empty;
    for (cfg.addrs) |addr_str| {
        const hp = split_host_port(addr_str, 7475) orelse continue;
        try eps.append(arena, .{ .host = hp.host, .port = hp.port });
    }

    var backoff_ms: i64 = 500;
    while (true) {
        if (eps.items.len > 0) {
            if (transport.NoiseConn.dial_any_tcp(gpa, io, eps.items, 3_000, kp, hub_pub)) |res_conn| {
                var conn = res_conn.conn;
                backoff_ms = 500;
                const chosen = eps.items[res_conn.addr_index];
                run_session(gpa, io, home_path, env, &conn, &cfg, inv, &run_mgr) catch |err| {
                    print_err(io, "session {s}:{d} ended: {s}\n", .{ chosen.host, chosen.port, @errorName(err) }) catch {};
                };
                conn.deinit();
            } else |err| {
                print_err(io, "connect across {d} addr(s) failed: {s}\n", .{ eps.items.len, @errorName(err) }) catch {};
                backoff_ms = @min(backoff_ms * 2, 5_000);
            }
        }
        io.sleep(.fromMilliseconds(backoff_ms), .awake) catch break;
    }
    return 0;
}

const SessionCtx = struct {
    gpa: std.mem.Allocator,
    io: std.Io,
    home_path: []const u8,
    env: ?*const std.process.Environ.Map,
    conn: *transport.NoiseConn,
    tx_mu: std.Io.Mutex = .init,
    closed: std.atomic.Value(bool) = .init(false),
    last_rx_ms: std.atomic.Value(i64),
    cfg: *const NodeConfig,
};

fn send_hello_sync(
    gpa: std.mem.Allocator,
    arena: std.mem.Allocator,
    conn: *transport.NoiseConn,
    cfg: *const NodeConfig,
    inv: msg.Inventory,
    io: std.Io,
    env: ?*const std.process.Environ.Map,
) !void {
    const req: msg.Request = .{ .hello = .{
        .node_id = cfg.node_id,
        .node_version = version.version,
        .addrs = cfg.addrs,
        .inventory = inv,
        .sample = sys.collect_sample(io),
        .presence = sys.collect_presence(io, env),
    } };
    try conn.send_json(.request, 2, req);
    var rep = try conn.recv_frame();
    defer rep.deinit(gpa);
    if (rep.kind == .err) return error.RejectedByHub;
    if (rep.kind != .response) return error.UnexpectedFrame;
    const hr = try msg.decode_arena(msg.HelloReply, arena, rep.payload);
    if (!hr.ok) return error.RejectedByHub;
}

fn run_session(
    gpa: std.mem.Allocator,
    io: std.Io,
    home_path: []const u8,
    env: ?*const std.process.Environ.Map,
    conn: *transport.NoiseConn,
    cfg: *const NodeConfig,
    inv: msg.Inventory,
    run_mgr: *runner.Runner,
) !void {
    var hello_arena_st = std.heap.ArenaAllocator.init(gpa);
    defer hello_arena_st.deinit();
    try send_hello_sync(gpa, hello_arena_st.allocator(), conn, cfg, inv, io, env);

    var sctx = SessionCtx{
        .gpa = gpa,
        .io = io,
        .home_path = home_path,
        .env = env,
        .conn = conn,
        .last_rx_ms = .init(std.Io.Clock.awake.now(io).toMilliseconds()),
        .cfg = cfg,
    };

    const hb_fn = struct {
        fn run(ctx: *SessionCtx) void {
            var next_id: u64 = 4;
            while (!ctx.closed.load(.acquire)) {
                var slept: u32 = 0;
                while (slept < 5_000 and !ctx.closed.load(.acquire)) : (slept += 250) {
                    ctx.io.sleep(.fromMilliseconds(250), .awake) catch break;
                }
                if (ctx.closed.load(.acquire)) break;
                const now_ms = std.Io.Clock.awake.now(ctx.io).toMilliseconds();
                if (now_ms - ctx.last_rx_ms.load(.acquire) > 15_000) {
                    ctx.closed.store(true, .release);
                    ctx.conn.shutdown();
                    break;
                }
                const req: msg.Request = .{ .hello = .{
                    .node_id = ctx.cfg.node_id,
                    .node_version = version.version,
                    .addrs = ctx.cfg.addrs,
                    .sample = sys.collect_sample(ctx.io),
                    .presence = sys.collect_presence(ctx.io, ctx.env),
                } };
                ctx.tx_mu.lockUncancelable(ctx.io);
                ctx.conn.send_json(.request, next_id, req) catch {
                    ctx.tx_mu.unlock(ctx.io);
                    ctx.closed.store(true, .release);
                    ctx.conn.shutdown();
                    break;
                };
                ctx.tx_mu.unlock(ctx.io);
                next_id +%= 2;
            }
        }
    }.run;

    const hb_t = try std.Thread.spawn(.{}, hb_fn, .{&sctx});
    defer {
        sctx.closed.store(true, .release);
        hb_t.join();
    }

    var req_arena_st = std.heap.ArenaAllocator.init(gpa);
    defer req_arena_st.deinit();

    while (true) {
        var f = try conn.recv_frame();
        defer f.deinit(gpa);
        sctx.last_rx_ms.store(std.Io.Clock.awake.now(io).toMilliseconds(), .release);

        if (f.kind != .request) continue;
        _ = req_arena_st.reset(.retain_capacity);
        const arena = req_arena_st.allocator();

        const req = msg.decode_arena(msg.Request, arena, f.payload) catch {
            sctx.tx_mu.lockUncancelable(io);
            defer sctx.tx_mu.unlock(io);
            try conn.send_json(.err, f.id, msg.ErrorReply{
                .code = .invalid,
                .message = "bad request json",
            });
            continue;
        };

        handle_request(arena, &sctx, run_mgr, f.id, req) catch |err| {
            sctx.tx_mu.lockUncancelable(io);
            defer sctx.tx_mu.unlock(io);
            try conn.send_json(.err, f.id, msg.ErrorReply{
                .code = .internal,
                .message = @errorName(err),
            });
        };
    }
}

fn handle_request(
    arena: std.mem.Allocator,
    sctx: *SessionCtx,
    run_mgr: *runner.Runner,
    req_id: u64,
    req: msg.Request,
) !void {
    switch (req) {
        .ping => |p| {
            sctx.tx_mu.lockUncancelable(sctx.io);
            defer sctx.tx_mu.unlock(sctx.io);
            try sctx.conn.send_json(.response, req_id, msg.Ping{ .now_ms = p.now_ms });
        },
        .submit_job => |sj| {
            const info = try run_mgr.submit(arena, sj);
            sctx.tx_mu.lockUncancelable(sctx.io);
            defer sctx.tx_mu.unlock(sctx.io);
            try sctx.conn.send_json(.response, req_id, msg.JobReply{ .job = info });
        },
        .job_status => |js| {
            if (try run_mgr.get_status(arena, js.job_id)) |info| {
                sctx.tx_mu.lockUncancelable(sctx.io);
                defer sctx.tx_mu.unlock(sctx.io);
                try sctx.conn.send_json(.response, req_id, msg.JobReply{ .job = info });
            } else {
                sctx.tx_mu.lockUncancelable(sctx.io);
                defer sctx.tx_mu.unlock(sctx.io);
                try sctx.conn.send_json(.err, req_id, msg.ErrorReply{ .code = .not_found, .message = "unknown job" });
            }
        },
        .cancel_job => |jr| {
            if (try run_mgr.cancel(arena, jr.job_id)) |info| {
                sctx.tx_mu.lockUncancelable(sctx.io);
                defer sctx.tx_mu.unlock(sctx.io);
                try sctx.conn.send_json(.response, req_id, msg.JobReply{ .job = info });
            } else {
                sctx.tx_mu.lockUncancelable(sctx.io);
                defer sctx.tx_mu.unlock(sctx.io);
                try sctx.conn.send_json(.err, req_id, msg.ErrorReply{ .code = .not_found, .message = "unknown job" });
            }
        },
        .list_jobs => {
            const jobs = try run_mgr.list_all(arena);
            sctx.tx_mu.lockUncancelable(sctx.io);
            defer sctx.tx_mu.unlock(sctx.io);
            try sctx.conn.send_json(.response, req_id, msg.JobList{ .jobs = jobs });
        },
        .fetch_log => |fl| {
            const max_chunk = fl.limit orelse 256 * 1024;
            if (try run_mgr.read_log(arena, fl.job_id, fl.offset, max_chunk)) |res| {
                sctx.tx_mu.lockUncancelable(sctx.io);
                defer sctx.tx_mu.unlock(sctx.io);
                if (res.data.len > 0) {
                    try sctx.conn.send_frame(.chunk, req_id, res.data);
                }
                try sctx.conn.send_json(.end, req_id, msg.LogEnd{ .eof = res.eof, .next_offset = res.next_offset });
            } else {
                sctx.tx_mu.lockUncancelable(sctx.io);
                defer sctx.tx_mu.unlock(sctx.io);
                try sctx.conn.send_json(.err, req_id, msg.ErrorReply{ .code = .not_found, .message = "log not found" });
            }
        },
        .fetch_artifact => |fa| {
            const max_chunk = fa.limit orelse 256 * 1024;
            if (try run_mgr.read_artifact(arena, fa.job_id, fa.path, fa.offset, max_chunk)) |res| {
                sctx.tx_mu.lockUncancelable(sctx.io);
                defer sctx.tx_mu.unlock(sctx.io);
                if (res.data.len > 0) {
                    try sctx.conn.send_frame(.chunk, req_id, res.data);
                }
                try sctx.conn.send_json(.end, req_id, msg.ArtifactEnd{
                    .eof = res.eof,
                    .next_offset = res.next_offset,
                    .size_bytes = res.size_bytes,
                    .sha256 = res.sha256,
                });
            } else {
                sctx.tx_mu.lockUncancelable(sctx.io);
                defer sctx.tx_mu.unlock(sctx.io);
                try sctx.conn.send_json(.err, req_id, msg.ErrorReply{ .code = .not_found, .message = "artifact not found" });
            }
        },
        .update => |upd| {
            const rel_dir = try std.fs.path.join(arena, &.{ sctx.home_path, "releases" });
            const target = upd.version orelse version.version;
            var ok = true;
            var detail: []const u8 = "up to date";

            if (upd.url) |u| {
                const sha = upd.sha256 orelse {
                    sctx.tx_mu.lockUncancelable(sctx.io);
                    defer sctx.tx_mu.unlock(sctx.io);
                    try sctx.conn.send_json(.err, req_id, msg.ErrorReply{
                        .code = .invalid,
                        .message = "update url requires sha256",
                    });
                    return;
                };
                u_store.stage_from_url(sctx.gpa, sctx.io, rel_dir, target, u, sha) catch |err| {
                    ok = false;
                    detail = @errorName(err);
                };
                if (ok) detail = "staged from url";
            } else if (upd.version != null and (u_store.activate_staged(sctx.gpa, sctx.io, rel_dir, target) catch false)) {
                detail = "activated staged release";
            } else {
                const ch: u_ver.Channel = if (upd.channel != null and std.mem.eql(u8, upd.channel.?, "dev")) .dev else .stable;
                const latest = u_store.check_latest(arena, sctx.io, "hotschmoe/protectorate", ch, version.version) catch null;
                if (latest) |rel| {
                    u_store.stage_and_apply(sctx.gpa, sctx.io, rel_dir, rel) catch |err| {
                        ok = false;
                        detail = @errorName(err);
                    };
                    if (ok) detail = "staged from github";
                } else {
                    detail = "already on latest release";
                }
            }

            sctx.tx_mu.lockUncancelable(sctx.io);
            defer sctx.tx_mu.unlock(sctx.io);
            try sctx.conn.send_json(.response, req_id, msg.UpdateReply{
                .ok = ok,
                .from = version.version,
                .to = target,
                .detail = detail,
                .restarting = false,
            });
        },
        .upload_update => |uu| {
            if (!is_safe_name(uu.rollout_id) or !is_safe_name(uu.version)) {
                sctx.tx_mu.lockUncancelable(sctx.io);
                defer sctx.tx_mu.unlock(sctx.io);
                try sctx.conn.send_json(.err, req_id, msg.ErrorReply{
                    .code = .invalid,
                    .message = "invalid rollout_id or version",
                });
                return;
            }
            if ((uu.hex.len % 2) != 0) {
                sctx.tx_mu.lockUncancelable(sctx.io);
                defer sctx.tx_mu.unlock(sctx.io);
                try sctx.conn.send_json(.err, req_id, msg.ErrorReply{
                    .code = .invalid,
                    .message = "odd hex length",
                });
                return;
            }

            const rel_dir_path = try std.fs.path.join(arena, &.{ sctx.home_path, "releases" });
            var rel_dir = try std.Io.Dir.cwd().createDirPathOpen(sctx.io, rel_dir_path, .{});
            defer rel_dir.close(sctx.io);

            const part_name = try std.fmt.allocPrint(arena, "{s}.part", .{uu.rollout_id});
            const chunk = try arena.alloc(u8, uu.hex.len / 2);
            _ = std.fmt.hexToBytes(chunk, uu.hex) catch {
                sctx.tx_mu.lockUncancelable(sctx.io);
                defer sctx.tx_mu.unlock(sctx.io);
                try sctx.conn.send_json(.err, req_id, msg.ErrorReply{
                    .code = .invalid,
                    .message = "bad hex chunk",
                });
                return;
            };

            var pf = if (uu.offset == 0)
                try rel_dir.createFile(sctx.io, part_name, .{ .read = true, .truncate = true })
            else
                rel_dir.openFile(sctx.io, part_name, .{ .mode = .read_write }) catch
                    try rel_dir.createFile(sctx.io, part_name, .{ .read = true, .truncate = false });
            errdefer pf.close(sctx.io);
            if (chunk.len > 0) {
                try pf.writePositionalAll(sctx.io, chunk, uu.offset);
            }
            const written = uu.offset + chunk.len;
            pf.close(sctx.io);

            var ready = false;
            if (uu.eof) {
                const full = try rel_dir.readFileAlloc(sctx.io, part_name, arena, .limited(128 << 20));
                if (!u_ver.verify_sha256(full, uu.sha256)) {
                    rel_dir.deleteFile(sctx.io, part_name) catch {};
                    sctx.tx_mu.lockUncancelable(sctx.io);
                    defer sctx.tx_mu.unlock(sctx.io);
                    try sctx.conn.send_json(.err, req_id, msg.ErrorReply{
                        .code = .invalid,
                        .message = "upload sha256 mismatch",
                    });
                    return;
                }
                const asset_name = try std.fmt.allocPrint(arena, "protectorate-node-{s}", .{u_store.host_asset_suffix()});
                try u_store.stage_archive_bytes(sctx.gpa, sctx.io, rel_dir_path, uu.version, asset_name, uu.sha256, full);
                rel_dir.deleteFile(sctx.io, part_name) catch {};
                ready = true;
            }

            sctx.tx_mu.lockUncancelable(sctx.io);
            defer sctx.tx_mu.unlock(sctx.io);
            try sctx.conn.send_json(.response, req_id, msg.UploadUpdateReply{
                .received_bytes = written,
                .ready = ready,
                .sha256 = if (ready) uu.sha256 else null,
            });
        },
        .hello => {},
    }
}

fn is_safe_name(s: []const u8) bool {
    if (s.len == 0 or s.len > 128) return false;
    if (std.mem.eql(u8, s, ".") or std.mem.eql(u8, s, "..")) return false;
    for (s) |c| {
        if (c == '/' or c == '\\' or c == 0) return false;
    }
    return true;
}

fn split_host_port(addr: []const u8, default_port: u16) ?struct { host: []const u8, port: u16 } {
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

fn print_err(io: std.Io, comptime fmt: []const u8, args: anytype) !void {
    var buf: [512]u8 = undefined;
    var w = std.Io.File.stderr().writer(io, &buf);
    try w.interface.print(fmt, args);
    try w.interface.flush();
}
