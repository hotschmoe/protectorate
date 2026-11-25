const std = @import("std");
const builtin = @import("builtin");
const tiger = @import("../tiger.zig");
const version = @import("../version.zig");
const json_util = @import("../json_util.zig");
const noise = @import("../net/noise.zig");
const sys = @import("sys.zig");
const admission = @import("admission.zig");
const runner = @import("runner.zig");
const node_loop = @import("loop.zig");

pub const NodeConfig = struct {
    node_id: []const u8,
    name: []const u8,
    hub_public_key: []const u8,
    hub_fingerprint: []const u8 = "",
    addrs: []const []const u8,
    authorized: bool = false,
    slots: u32 = 8,
    long_slots: u32 = 2,
    mem_margin_bytes: u64 = 512 * 1024 * 1024,
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

    const source: node_loop.HelloSource = .{
        .io = io,
        .env = env,
        .node_id = cfg.node_id,
        .addrs = cfg.addrs,
        .inventory = try sys.collect_inventory(arena, io, version.version),
        .adm = null,
        .presence = null,
    };
    if (node_loop.probe(&source, kp, hub_pub)) |accepted| {
        if (accepted) {
            cfg.authorized = true;
            try save_config(gpa, io, home_path, cfg);
            var obuf: [512]u8 = undefined;
            var w = std.Io.File.stdout().writer(io, &obuf);
            try w.interface.print("authorized (hub {s})\n", .{cfg.hub_fingerprint});
            try w.interface.flush();
            return 0;
        }
    } else |err| {
        try print_err(io, "authorize failed: {s}\n", .{@errorName(err)});
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

    var adm_mgr = try admission.AdmissionManager.init(gpa, io, home_path, .{
        .slots = cfg.slots,
        .long_slots = cfg.long_slots,
        .mem_margin_bytes = cfg.mem_margin_bytes,
    });
    defer adm_mgr.deinit();
    try adm_mgr.start();

    var run_mgr = try runner.Runner.init(gpa, io, home_path, env, adm_mgr);
    defer run_mgr.deinit();

    const inv = try sys.collect_inventory(arena, io, version.version);

    var pres_det = sys.PresenceDetector.init(io);
    defer pres_det.deinit(io);

    const server = try node_loop.Server.create(.{
        .gpa = gpa,
        .io = io,
        .env = env,
        .home_path = home_path,
        .node_id = cfg.node_id,
        .addrs = cfg.addrs,
        .key = kp,
        .hub_key = hub_pub,
        .inventory = inv,
        .adm = adm_mgr,
        .run_mgr = run_mgr,
        .presence = &pres_det,
    });
    defer server.destroy();
    server.run();
    if (server.workers_abandoned) {
        // A detached worker may still reach the runner, admission, presence and config: do what teardown does to
        // running jobs, then end the process without destroying any of them.
        run_mgr.fence_all();
        std.process.exit(0);
    }
    return 0;
}

fn print_err(io: std.Io, comptime fmt: []const u8, args: anytype) !void {
    var buf: [512]u8 = undefined;
    var w = std.Io.File.stderr().writer(io, &buf);
    try w.interface.print(fmt, args);
    try w.interface.flush();
}
