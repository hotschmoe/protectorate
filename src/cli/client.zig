const std = @import("std");
const tiger = @import("../tiger.zig");
const schema = @import("../api/schema.zig");

fn resolve_home_path(gpa: std.mem.Allocator, env: *const std.process.Environ.Map, override: ?[]const u8) ![]u8 {
    if (override) |o| return gpa.dupe(u8, o);
    if (env.get("PROTECTORATE_HOME")) |e| return gpa.dupe(u8, e);
    if (env.get("HOME")) |h| return std.fs.path.join(gpa, &.{ h, ".local/share/protectorate" });
    if (env.get("USERPROFILE")) |h| return std.fs.path.join(gpa, &.{ h, ".protectorate" });
    return gpa.dupe(u8, ".protectorate");
}

pub const Client = struct {
    gpa: std.mem.Allocator,
    io: std.Io,
    base_url: []const u8,
    token: ?[]const u8,

    pub fn resolve(
        gpa: std.mem.Allocator,
        io: std.Io,
        env: *const std.process.Environ.Map,
        url_override: ?[]const u8,
        token_override: ?[]const u8,
        home_override: ?[]const u8,
    ) !Client {
        var arena_st = std.heap.ArenaAllocator.init(gpa);
        defer arena_st.deinit();
        const a = arena_st.allocator();

        var resolved_url: ?[]const u8 = url_override orelse env.get("PROTECTORATE_URL");
        var resolved_tok: ?[]const u8 = token_override orelse env.get("PROTECTORATE_TOKEN");

        if (resolved_url == null or resolved_tok == null) {
            const hp = try resolve_home_path(gpa, env, home_override);
            defer gpa.free(hp);
            if (resolved_url == null) {
                const cfg_p = try std.fs.path.join(a, &.{ hp, "config.json" });
                if (std.Io.Dir.cwd().readFileAlloc(io, cfg_p, a, .limited(1 << 20))) |raw| {
                    if (std.json.parseFromSliceLeaky(std.json.Value, a, raw, .{})) |val| {
                        if (val == .object) {
                            if (val.object.get("listen_addr")) |la| {
                                if (la == .string) {
                                    resolved_url = try std.fmt.allocPrint(a, "http://{s}", .{la.string});
                                }
                            }
                        }
                    } else |_| {}
                } else |_| {}
            }
            if (resolved_tok == null) {
                const auth_p = try std.fs.path.join(a, &.{ hp, "auth.json" });
                if (std.Io.Dir.cwd().readFileAlloc(io, auth_p, a, .limited(1 << 20))) |raw| {
                    if (std.json.parseFromSliceLeaky(std.json.Value, a, raw, .{})) |val| {
                        if (val == .object) {
                            if (val.object.get("owner_token")) |ot| {
                                if (ot == .string) resolved_tok = ot.string;
                            }
                        }
                    } else |_| {}
                } else |_| {}
            }
        }

        const final_url = std.mem.trimEnd(u8, resolved_url orelse "http://127.0.0.1:7470", "/");
        return .{
            .gpa = gpa,
            .io = io,
            .base_url = try gpa.dupe(u8, final_url),
            .token = if (resolved_tok) |t| try gpa.dupe(u8, t) else null,
        };
    }

    pub fn deinit(self: *Client) void {
        self.gpa.free(self.base_url);
        if (self.token) |t| self.gpa.free(t);
    }

    pub fn call_raw(
        self: *const Client,
        arena: std.mem.Allocator,
        method: []const u8,
        req_json: []const u8,
    ) !struct { status: u16, body: []u8 } {
        const url = try std.fmt.allocPrint(arena, "{s}/api/{s}", .{ self.base_url, method });
        var client: std.http.Client = .{ .allocator = arena, .io = self.io };
        defer client.deinit();

        var hdrs_buf: [2]std.http.Header = undefined;
        var hdr_n: usize = 1;
        hdrs_buf[0] = .{ .name = "content-type", .value = "application/json" };
        if (self.token) |tok| {
            hdrs_buf[1] = .{
                .name = "authorization",
                .value = try std.fmt.allocPrint(arena, "Bearer {s}", .{tok}),
            };
            hdr_n = 2;
        }

        var out: std.Io.Writer.Allocating = .init(arena);
        const res = try client.fetch(.{
            .location = .{ .url = url },
            .method = .POST,
            .payload = req_json,
            .extra_headers = hdrs_buf[0..hdr_n],
            .response_writer = &out.writer,
        });
        return .{
            .status = @intFromEnum(res.status),
            .body = try arena.dupe(u8, out.written()),
        };
    }

    pub fn call(
        self: *const Client,
        arena: std.mem.Allocator,
        comptime Resp: type,
        method: []const u8,
        req_val: anytype,
    ) !Resp {
        const req_json = try schema.encode_alloc(arena, req_val);
        const res = try self.call_raw(arena, method, req_json);
        if (res.status < 200 or res.status > 299) return error.RpcError;
        return schema.decode_arena(Resp, arena, res.body);
    }
};
