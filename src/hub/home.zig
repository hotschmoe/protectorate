const std = @import("std");
const builtin = @import("builtin");
const tiger = @import("../tiger.zig");
const json_util = @import("../json_util.zig");
const noise = @import("../net/noise.zig");
const schema = @import("../api/schema.zig");

pub const DeviceRecord = struct {
    id: []const u8,
    name: []const u8,
    token: []const u8,
    created_at_ms: i64,
    last_seen_ms: ?i64 = null,
};

pub const PairingRecord = struct {
    name: []const u8,
    code: []const u8,
    expires_at_ms: i64,
};

pub const ConfigFile = struct {
    hub_name: []const u8 = "protectorate",
    listen_addr: []const u8 = "127.0.0.1:7470",
    fleet_listen: []const u8 = "0.0.0.0:7475",
    advertise_addrs: []const []const u8 = &.{},
    no_sandbox: bool = false,
    update_repo: []const u8 = "hotschmoe/protectorate",
    update_channel: []const u8 = "stable",
    d9_spawn_limit: u32 = 3,
    d9_approval_timeout_secs: u64 = 300,
    sleeve_devices: []const []const u8 = &.{},
};

const AuthFile = struct {
    owner_token: []const u8,
    envoy_token: []const u8,
    hub_secret_hex: []const u8,
    devices: []const DeviceRecord = &.{},
    pairing_codes: []const PairingRecord = &.{},
};

pub const Principal = union(enum) {
    owner,
    envoy,
    device: []const u8,
    sleeve: []const u8,
};

pub const HubHome = struct {
    gpa: std.mem.Allocator,
    io: std.Io,
    root_path: []const u8,
    journal_path: []const u8,
    transcripts_path: []const u8,
    history_path: []const u8,
    artifacts_path: []const u8,
    briefs_path: []const u8,
    releases_path: []const u8,

    mu: std.Io.Mutex = .init,
    hub_keypair: noise.KeyPair,
    owner_token: []const u8,
    envoy_token: []const u8,
    hub_secret_hex: []const u8,
    devices: std.ArrayList(DeviceRecord) = .empty,
    pairing_codes: std.ArrayList(PairingRecord) = .empty,
    sleeve_tokens: std.StringHashMapUnmanaged([]const u8) = .empty,
    config: ConfigFile,

    pub fn resolve_path(
        gpa: std.mem.Allocator,
        env: *const std.process.Environ.Map,
        override: ?[]const u8,
    ) ![]u8 {
        if (override) |o| return gpa.dupe(u8, o);
        if (env.get("PROTECTORATE_HOME")) |e| return gpa.dupe(u8, e);
        if (env.get("HOME")) |h| return std.fs.path.join(gpa, &.{ h, ".local/share/protectorate" });
        if (env.get("USERPROFILE")) |h| return std.fs.path.join(gpa, &.{ h, ".protectorate" });
        return gpa.dupe(u8, ".protectorate");
    }

    pub fn open(
        gpa: std.mem.Allocator,
        io: std.Io,
        root_path: []const u8,
        dev_token_override: ?[]const u8,
    ) !HubHome {
        var root_dir = try std.Io.Dir.cwd().createDirPathOpen(io, root_path, .{});
        defer root_dir.close(io);

        const journal_path = try std.fs.path.join(gpa, &.{ root_path, "journal" });
        errdefer gpa.free(journal_path);
        const transcripts_path = try std.fs.path.join(gpa, &.{ root_path, "transcripts" });
        errdefer gpa.free(transcripts_path);
        const history_path = try std.fs.path.join(gpa, &.{ root_path, "history" });
        errdefer gpa.free(history_path);
        const artifacts_path = try std.fs.path.join(gpa, &.{ root_path, "artifacts" });
        errdefer gpa.free(artifacts_path);
        const briefs_path = try std.fs.path.join(gpa, &.{ root_path, "briefs" });
        errdefer gpa.free(briefs_path);
        const releases_path = try std.fs.path.join(gpa, &.{ root_path, "releases" });
        errdefer gpa.free(releases_path);

        for ([_][]const u8{ "journal", "transcripts", "history", "artifacts", "briefs", "releases" }) |sub| {
            try root_dir.createDirPath(io, sub);
        }

        const cfg = load_or_create_config(gpa, io, root_dir);

        var home = HubHome{
            .gpa = gpa,
            .io = io,
            .root_path = try gpa.dupe(u8, root_path),
            .journal_path = journal_path,
            .transcripts_path = transcripts_path,
            .history_path = history_path,
            .artifacts_path = artifacts_path,
            .briefs_path = briefs_path,
            .releases_path = releases_path,
            .hub_keypair = undefined,
            .owner_token = "",
            .envoy_token = "",
            .hub_secret_hex = "",
            .config = cfg,
        };
        try home.load_or_create_auth(root_dir, dev_token_override);
        return home;
    }

    pub fn deinit(self: *HubHome) void {
        self.hub_keypair.zeroize();
        self.gpa.free(self.root_path);
        self.gpa.free(self.journal_path);
        self.gpa.free(self.transcripts_path);
        self.gpa.free(self.history_path);
        self.gpa.free(self.artifacts_path);
        self.gpa.free(self.briefs_path);
        self.gpa.free(self.releases_path);
        self.gpa.free(self.owner_token);
        self.gpa.free(self.envoy_token);
        self.gpa.free(self.hub_secret_hex);
        for (self.devices.items) |d| {
            self.gpa.free(d.id);
            self.gpa.free(d.name);
            self.gpa.free(d.token);
        }
        self.devices.deinit(self.gpa);
        for (self.pairing_codes.items) |p| {
            self.gpa.free(p.name);
            self.gpa.free(p.code);
        }
        self.pairing_codes.deinit(self.gpa);
        var sit = self.sleeve_tokens.iterator();
        while (sit.next()) |e| {
            self.gpa.free(e.key_ptr.*);
            self.gpa.free(e.value_ptr.*);
        }
        self.sleeve_tokens.deinit(self.gpa);
        self.gpa.free(self.config.hub_name);
        self.gpa.free(self.config.listen_addr);
        self.gpa.free(self.config.fleet_listen);
        for (self.config.advertise_addrs) |a| self.gpa.free(a);
        self.gpa.free(self.config.advertise_addrs);
        self.gpa.free(self.config.update_repo);
        self.gpa.free(self.config.update_channel);
        for (self.config.sleeve_devices) |d| self.gpa.free(d);
        self.gpa.free(self.config.sleeve_devices);
    }

    fn load_or_create_auth(self: *HubHome, root_dir: std.Io.Dir, dev_token: ?[]const u8) !void {
        if (root_dir.readFileAlloc(self.io, "auth.json", self.gpa, .limited(2 << 20))) |raw| {
            defer self.gpa.free(raw);
            var arena_st = std.heap.ArenaAllocator.init(self.gpa);
            defer arena_st.deinit();
            if (std.json.parseFromSliceLeaky(std.json.Value, arena_st.allocator(), raw, .{})) |val| {
                if (std.json.parseFromValueLeaky(AuthFile, arena_st.allocator(), val, .{
                    .ignore_unknown_fields = true,
                    .allocate = .alloc_always,
                })) |af| {
                    if (af.hub_secret_hex.len == 64) {
                        var sec: [32]u8 = undefined;
                        if (std.fmt.hexToBytes(&sec, af.hub_secret_hex)) |_| {
                            self.hub_keypair = try noise.KeyPair.from_secret(sec);
                            self.hub_secret_hex = try self.gpa.dupe(u8, af.hub_secret_hex);
                            self.owner_token = if (dev_token) |dt| try self.gpa.dupe(u8, dt) else try self.gpa.dupe(u8, af.owner_token);
                            self.envoy_token = try self.gpa.dupe(u8, af.envoy_token);
                            for (af.devices) |d| {
                                try self.devices.append(self.gpa, .{
                                    .id = try self.gpa.dupe(u8, d.id),
                                    .name = try self.gpa.dupe(u8, d.name),
                                    .token = try self.gpa.dupe(u8, d.token),
                                    .created_at_ms = d.created_at_ms,
                                    .last_seen_ms = d.last_seen_ms,
                                });
                            }
                            for (af.pairing_codes) |p| {
                                try self.pairing_codes.append(self.gpa, .{
                                    .name = try self.gpa.dupe(u8, p.name),
                                    .code = try self.gpa.dupe(u8, p.code),
                                    .expires_at_ms = p.expires_at_ms,
                                });
                            }
                            return;
                        } else |_| {}
                    }
                } else |_| {}
            } else |_| {}
        } else |_| {}

        self.hub_keypair = noise.KeyPair.generate(self.io);
        const sec_hex = hex32(self.hub_keypair.secret);
        self.hub_secret_hex = try self.gpa.dupe(u8, &sec_hex);
        self.owner_token = if (dev_token) |dt| try self.gpa.dupe(u8, dt) else try random_token_alloc(self.gpa, self.io);
        self.envoy_token = try random_token_alloc(self.gpa, self.io);
        try self.save_auth_locked(root_dir);
    }

    fn save_auth_locked(self: *const HubHome, root_dir: std.Io.Dir) !void {
        const af: AuthFile = .{
            .owner_token = self.owner_token,
            .envoy_token = self.envoy_token,
            .hub_secret_hex = self.hub_secret_hex,
            .devices = self.devices.items,
            .pairing_codes = self.pairing_codes.items,
        };
        const json = try std.json.Stringify.valueAlloc(self.gpa, af, .{ .whitespace = .indent_2 });
        defer self.gpa.free(json);
        var f = try root_dir.createFile(self.io, "auth.json.tmp", .{ .read = true, .truncate = true });
        defer f.close(self.io);
        if (comptime builtin.os.tag != .windows) {
            f.setPermissions(self.io, .fromMode(0o600)) catch {};
        }
        try f.writePositionalAll(self.io, json, 0);
        try f.sync(self.io);
        try std.Io.Dir.rename(root_dir, "auth.json.tmp", root_dir, "auth.json", self.io);
    }

    pub fn save_auth(self: *HubHome) !void {
        self.mu.lockUncancelable(self.io);
        defer self.mu.unlock(self.io);
        var root_dir = try std.Io.Dir.cwd().openDir(self.io, self.root_path, .{});
        defer root_dir.close(self.io);
        try self.save_auth_locked(root_dir);
    }

    pub fn authorize(self: *HubHome, token: []const u8) ?Principal {
        self.mu.lockUncancelable(self.io);
        defer self.mu.unlock(self.io);
        if (const_time_eql(token, self.owner_token)) return .owner;
        if (const_time_eql(token, self.envoy_token)) return .envoy;
        for (self.devices.items) |*d| {
            if (const_time_eql(token, d.token)) {
                d.last_seen_ms = std.Io.Clock.real.now(self.io).toMilliseconds();
                return .{ .device = d.id };
            }
        }
        if (self.sleeve_tokens.get(token)) |sid| {
            return .{ .sleeve = sid };
        }
        return null;
    }

    pub fn create_pairing(self: *HubHome, name: []const u8, now_ms: i64) !schema.PairingCode {
        self.mu.lockUncancelable(self.io);
        defer self.mu.unlock(self.io);
        const code = try random_short_code_alloc(self.gpa, self.io);
        errdefer self.gpa.free(code);
        const n_dup = try self.gpa.dupe(u8, name);
        errdefer self.gpa.free(n_dup);
        const exp = now_ms + 10 * 60_000;
        try self.pairing_codes.append(self.gpa, .{
            .name = n_dup,
            .code = code,
            .expires_at_ms = exp,
        });
        var root_dir = try std.Io.Dir.cwd().openDir(self.io, self.root_path, .{});
        defer root_dir.close(self.io);
        try self.save_auth_locked(root_dir);
        return .{
            .name = n_dup,
            .code = code,
            .expires_at_ms = .{ .ms = exp },
        };
    }

    pub fn claim_pairing(self: *HubHome, arena: std.mem.Allocator, code: []const u8, now_ms: i64) !?schema.PairedDevice {
        self.mu.lockUncancelable(self.io);
        defer self.mu.unlock(self.io);

        var found_idx: ?usize = null;
        for (self.pairing_codes.items, 0..) |p, idx| {
            if (std.mem.eql(u8, p.code, code) and p.expires_at_ms >= now_ms) {
                found_idx = idx;
                break;
            }
        }
        const idx = found_idx orelse return null;
        const p = self.pairing_codes.orderedRemove(idx);
        defer {
            self.gpa.free(p.name);
            self.gpa.free(p.code);
        }

        const dev_id = try std.fmt.allocPrint(self.gpa, "dev-{d}-{d}", .{ now_ms, self.devices.items.len + 1 });
        errdefer self.gpa.free(dev_id);
        const dev_tok = try random_token_alloc(self.gpa, self.io);
        errdefer self.gpa.free(dev_tok);
        const dev_name = try self.gpa.dupe(u8, p.name);
        errdefer self.gpa.free(dev_name);

        try self.devices.append(self.gpa, .{
            .id = dev_id,
            .name = dev_name,
            .token = dev_tok,
            .created_at_ms = now_ms,
            .last_seen_ms = now_ms,
        });

        var root_dir = try std.Io.Dir.cwd().openDir(self.io, self.root_path, .{});
        defer root_dir.close(self.io);
        try self.save_auth_locked(root_dir);

        return .{
            .device = .{
                .id = try arena.dupe(u8, dev_id),
                .name = try arena.dupe(u8, dev_name),
                .created_at_ms = .{ .ms = now_ms },
                .last_seen_ms = .{ .ms = now_ms },
            },
            .token = try arena.dupe(u8, dev_tok),
        };
    }

    pub fn revoke_device(self: *HubHome, device_id: []const u8) !bool {
        self.mu.lockUncancelable(self.io);
        defer self.mu.unlock(self.io);
        for (self.devices.items, 0..) |d, i| {
            if (std.mem.eql(u8, d.id, device_id)) {
                const rem = self.devices.orderedRemove(i);
                self.gpa.free(rem.id);
                self.gpa.free(rem.name);
                self.gpa.free(rem.token);
                var root_dir = try std.Io.Dir.cwd().openDir(self.io, self.root_path, .{});
                defer root_dir.close(self.io);
                try self.save_auth_locked(root_dir);
                return true;
            }
        }
        return false;
    }

    pub fn list_devices(self: *HubHome, arena: std.mem.Allocator) ![]schema.Device {
        self.mu.lockUncancelable(self.io);
        defer self.mu.unlock(self.io);
        var out = try arena.alloc(schema.Device, self.devices.items.len);
        for (self.devices.items, 0..) |d, i| {
            out[i] = .{
                .id = try arena.dupe(u8, d.id),
                .name = try arena.dupe(u8, d.name),
                .created_at_ms = .{ .ms = d.created_at_ms },
                .last_seen_ms = if (d.last_seen_ms) |m| .{ .ms = m } else null,
            };
        }
        return out;
    }
};

fn load_or_create_config(gpa: std.mem.Allocator, io: std.Io, root_dir: std.Io.Dir) ConfigFile {
    const def: ConfigFile = .{
        .hub_name = gpa.dupe(u8, "protectorate") catch "",
        .listen_addr = gpa.dupe(u8, "127.0.0.1:7470") catch "",
        .fleet_listen = gpa.dupe(u8, "0.0.0.0:7475") catch "",
        .advertise_addrs = gpa.alloc([]const u8, 0) catch &.{},
        .no_sandbox = false,
        .update_repo = gpa.dupe(u8, "hotschmoe/protectorate") catch "",
        .update_channel = gpa.dupe(u8, "stable") catch "",
    };

    const raw = root_dir.readFileAlloc(io, "config.json", gpa, .limited(1 << 20)) catch return def;
    defer gpa.free(raw);
    var arena_st = std.heap.ArenaAllocator.init(gpa);
    defer arena_st.deinit();
    const val = std.json.parseFromSliceLeaky(std.json.Value, arena_st.allocator(), raw, .{}) catch return def;
    const cf = std.json.parseFromValueLeaky(ConfigFile, arena_st.allocator(), val, .{
        .ignore_unknown_fields = true,
        .allocate = .alloc_always,
    }) catch return def;

    const adv = gpa.alloc([]const u8, cf.advertise_addrs.len) catch return def;
    for (cf.advertise_addrs, 0..) |a, i| adv[i] = gpa.dupe(u8, a) catch "";
    const devs = gpa.alloc([]const u8, cf.sleeve_devices.len) catch return def;
    for (cf.sleeve_devices, 0..) |d, i| devs[i] = gpa.dupe(u8, d) catch "";

    gpa.free(def.hub_name);
    gpa.free(def.listen_addr);
    gpa.free(def.fleet_listen);
    gpa.free(def.advertise_addrs);
    gpa.free(def.update_repo);
    gpa.free(def.update_channel);

    return .{
        .hub_name = gpa.dupe(u8, cf.hub_name) catch "",
        .listen_addr = gpa.dupe(u8, cf.listen_addr) catch "",
        .fleet_listen = gpa.dupe(u8, cf.fleet_listen) catch "",
        .advertise_addrs = adv,
        .no_sandbox = cf.no_sandbox,
        .update_repo = gpa.dupe(u8, cf.update_repo) catch "",
        .update_channel = gpa.dupe(u8, cf.update_channel) catch "",
        .d9_spawn_limit = cf.d9_spawn_limit,
        .d9_approval_timeout_secs = cf.d9_approval_timeout_secs,
        .sleeve_devices = devs,
    };
}

fn hex32(bytes: [32]u8) [64]u8 {
    const chars = "0123456789abcdef";
    var out: [64]u8 = undefined;
    for (bytes, 0..) |b, i| {
        out[i * 2] = chars[b >> 4];
        out[i * 2 + 1] = chars[b & 0x0f];
    }
    return out;
}

pub fn random_token_alloc(gpa: std.mem.Allocator, io: std.Io) ![]u8 {
    var raw: [32]u8 = undefined;
    io.random(&raw);
    const hex = hex32(raw);
    return gpa.dupe(u8, &hex);
}

fn random_short_code_alloc(gpa: std.mem.Allocator, io: std.Io) ![]u8 {
    var raw: [6]u8 = undefined;
    io.random(&raw);
    const alphabet = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    var out = try gpa.alloc(u8, 8);
    for (0..8) |i| {
        const b = raw[i % raw.len];
        out[i] = alphabet[b % alphabet.len];
    }
    return out;
}

fn const_time_eql(a: []const u8, b: []const u8) bool {
    if (a.len != b.len) return false;
    var diff: u8 = 0;
    for (a, b) |ca, cb| diff |= ca ^ cb;
    return diff == 0;
}
