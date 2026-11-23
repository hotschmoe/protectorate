const std = @import("std");
const builtin = @import("builtin");
const tiger = @import("../tiger.zig");
const json_util = @import("../json_util.zig");
const version = @import("version.zig");

pub const ReleaseInfo = struct {
    tag: []const u8,
    version: version.Version,
    asset_url: []const u8,
    asset_name: []const u8,
    sums_url: []const u8,
};

pub const Probation = struct {
    active: []const u8,
    previous: ?[]const u8 = null,
    healthy: bool = false,
};

pub fn host_asset_suffix() []const u8 {
    return switch (builtin.os.tag) {
        .linux => switch (builtin.cpu.arch) {
            .aarch64 => "aarch64-linux-musl.tar.gz",
            else => "x86_64-linux-musl.tar.gz",
        },
        .macos => "aarch64-macos.tar.gz",
        .windows => "x86_64-windows-gnu.zip",
        else => "unknown",
    };
}

pub fn check_latest(
    arena: std.mem.Allocator,
    io: std.Io,
    repo: []const u8,
    channel: version.Channel,
    current_str: []const u8,
) !?ReleaseInfo {
    const cur = version.Version.parse_tag(current_str) orelse version.Version{ .major = 0, .minor = 0, .patch = 0 };
    const url = try std.fmt.allocPrint(arena, "https://api.github.com/repos/{s}/releases?per_page=10", .{repo});

    var client: std.http.Client = .{ .allocator = arena, .io = io };
    defer client.deinit();
    var out: std.Io.Writer.Allocating = .init(arena);
    const res = client.fetch(.{
        .location = .{ .url = url },
        .method = .GET,
        .extra_headers = &.{
            .{ .name = "user-agent", .value = "protectorate" },
            .{ .name = "accept", .value = "application/vnd.github+json" },
        },
        .response_writer = &out.writer,
    }) catch return null;
    if (res.status != .ok) return null;

    const root = std.json.parseFromSliceLeaky(std.json.Value, arena, out.written(), .{}) catch return null;
    if (root != .array) return null;

    const suffix = host_asset_suffix();
    var best: ?ReleaseInfo = null;

    for (root.array.items) |rel| {
        if (json_util.value_bool(rel, "draft") == true) continue;
        const tag = json_util.value_str(rel, "tag_name") orelse continue;
        const parsed_v = version.Version.parse_tag(tag) orelse continue;
        const baseline = if (best) |b| b.version else cur;
        if (!parsed_v.is_newer(baseline, channel)) continue;

        const assets = json_util.value_array(rel, "assets") orelse continue;
        var asset_url: ?[]const u8 = null;
        var asset_name: ?[]const u8 = null;
        var sums_url: ?[]const u8 = null;
        for (assets) |a| {
            const name = json_util.value_str(a, "name") orelse continue;
            const dl = json_util.value_str(a, "browser_download_url") orelse continue;
            if (std.mem.eql(u8, name, "SHA256SUMS")) sums_url = dl;
            if (std.mem.endsWith(u8, name, suffix)) {
                asset_url = dl;
                asset_name = name;
            }
        }
        if (asset_url != null and sums_url != null and asset_name != null) {
            best = .{
                .tag = try arena.dupe(u8, tag),
                .version = parsed_v,
                .asset_url = try arena.dupe(u8, asset_url.?),
                .asset_name = try arena.dupe(u8, asset_name.?),
                .sums_url = try arena.dupe(u8, sums_url.?),
            };
        }
    }
    return best;
}

pub fn stage_and_apply(
    gpa: std.mem.Allocator,
    io: std.Io,
    releases_dir_path: []const u8,
    info: ReleaseInfo,
) !void {
    var client: std.http.Client = .{ .allocator = gpa, .io = io };
    defer client.deinit();

    var sums_aw: std.Io.Writer.Allocating = .init(gpa);
    defer sums_aw.deinit();
    const sums_res = try client.fetch(.{
        .location = .{ .url = info.sums_url },
        .method = .GET,
        .extra_headers = &.{.{ .name = "user-agent", .value = "protectorate" }},
        .response_writer = &sums_aw.writer,
    });
    if (sums_res.status != .ok) return error.HttpFailed;

    const entries = try version.parse_sha256sums(gpa, sums_aw.written());
    defer {
        for (entries) |e| gpa.free(e.filename);
        gpa.free(entries);
    }

    var expected_hex: ?[64]u8 = null;
    for (entries) |e| {
        if (std.mem.eql(u8, e.filename, info.asset_name)) {
            expected_hex = e.sha256_hex;
            break;
        }
    }
    const want_hex = expected_hex orelse return error.MissingChecksum;

    var pkg_aw: std.Io.Writer.Allocating = .init(gpa);
    defer pkg_aw.deinit();
    const pkg_res = try client.fetch(.{
        .location = .{ .url = info.asset_url },
        .method = .GET,
        .extra_headers = &.{.{ .name = "user-agent", .value = "protectorate" }},
        .response_writer = &pkg_aw.writer,
    });
    if (pkg_res.status != .ok) return error.HttpFailed;

    try stage_archive_bytes(gpa, io, releases_dir_path, info.tag, info.asset_name, &want_hex, pkg_aw.written());
}

pub fn stage_from_url(
    gpa: std.mem.Allocator,
    io: std.Io,
    releases_dir_path: []const u8,
    tag: []const u8,
    url: []const u8,
    expected_hex: []const u8,
) !void {
    var client: std.http.Client = .{ .allocator = gpa, .io = io };
    defer client.deinit();

    var pkg_aw: std.Io.Writer.Allocating = .init(gpa);
    defer pkg_aw.deinit();
    const pkg_res = try client.fetch(.{
        .location = .{ .url = url },
        .method = .GET,
        .extra_headers = &.{.{ .name = "user-agent", .value = "protectorate" }},
        .response_writer = &pkg_aw.writer,
    });
    if (pkg_res.status != .ok) return error.HttpFailed;

    const clean_url = std.mem.sliceTo(url, '?');
    const asset_name = std.fs.path.basename(clean_url);
    try stage_archive_bytes(gpa, io, releases_dir_path, tag, asset_name, expected_hex, pkg_aw.written());
}

pub fn stage_archive_bytes(
    gpa: std.mem.Allocator,
    io: std.Io,
    releases_dir_path: []const u8,
    tag: []const u8,
    asset_name: []const u8,
    expected_hex: []const u8,
    bytes: []const u8,
) !void {
    if (!version.verify_sha256(bytes, expected_hex)) {
        return error.ChecksumMismatch;
    }

    var rel_dir = try std.Io.Dir.cwd().createDirPathOpen(io, releases_dir_path, .{});
    defer rel_dir.close(io);

    const stage_name = try std.fmt.allocPrint(gpa, "{s}.staging", .{tag});
    defer gpa.free(stage_name);
    var stage_dir = try rel_dir.createDirPathOpen(io, stage_name, .{});
    defer stage_dir.close(io);

    if (std.mem.endsWith(u8, asset_name, ".tar.gz") or std.mem.endsWith(u8, asset_name, ".tgz")) {
        var in_r = std.Io.Reader.fixed(bytes);
        var flate_buf: [std.compress.flate.max_window_len]u8 = undefined;
        var decomp: std.compress.flate.Decompress = .init(&in_r, .gzip, &flate_buf);
        try std.tar.extract(io, stage_dir, &decomp.reader, .{});
    } else if (std.mem.endsWith(u8, asset_name, ".zip")) {
        const tmp_zip = ".pt-stage.zip";
        try stage_dir.writeFile(io, .{ .sub_path = tmp_zip, .data = bytes });
        defer stage_dir.deleteFile(io, tmp_zip) catch {};
        var zf = try stage_dir.openFile(io, tmp_zip, .{});
        defer zf.close(io);
        var rbuf: [16 * 1024]u8 = undefined;
        var zfr = zf.reader(io, &rbuf);
        try std.zip.extract(stage_dir, &zfr, .{ .allow_backslashes = true });
    } else {
        try stage_dir.writeFile(io, .{ .sub_path = asset_name, .data = bytes });
    }

    rel_dir.deleteTree(io, tag) catch {};
    try std.Io.Dir.rename(rel_dir, stage_name, rel_dir, tag, io);
    _ = try activate_staged(gpa, io, releases_dir_path, tag);
}

pub fn activate_staged(
    gpa: std.mem.Allocator,
    io: std.Io,
    releases_dir_path: []const u8,
    tag: []const u8,
) !bool {
    var rel_dir = std.Io.Dir.cwd().openDir(io, releases_dir_path, .{}) catch return false;
    defer rel_dir.close(io);

    var tag_dir = rel_dir.openDir(io, tag, .{}) catch return false;
    tag_dir.close(io);

    const prev_active = read_marker(gpa, io, rel_dir, "current.txt");
    defer if (prev_active) |p| gpa.free(p);

    if (prev_active) |p| {
        if (!std.mem.eql(u8, p, tag)) {
            try rel_dir.writeFile(io, .{ .sub_path = "previous.txt", .data = p });
        }
    }
    try rel_dir.writeFile(io, .{ .sub_path = "current.txt", .data = tag });

    if (builtin.os.tag != .windows) {
        rel_dir.deleteFile(io, "current") catch {};
        std.Io.Dir.symLink(rel_dir, io, tag, "current", .{ .is_directory = true }) catch {};
    }

    try write_probation(gpa, io, rel_dir, .{
        .active = tag,
        .previous = prev_active,
        .healthy = false,
    });
    return true;
}

pub fn mark_healthy(gpa: std.mem.Allocator, io: std.Io, releases_dir_path: []const u8) !void {
    var rel_dir = std.Io.Dir.cwd().openDir(io, releases_dir_path, .{}) catch return;
    defer rel_dir.close(io);
    const active = read_marker(gpa, io, rel_dir, "current.txt") orelse return;
    defer gpa.free(active);
    const prev = read_marker(gpa, io, rel_dir, "previous.txt");
    defer if (prev) |p| gpa.free(p);
    try write_probation(gpa, io, rel_dir, .{
        .active = active,
        .previous = prev,
        .healthy = true,
    });
}

pub fn rollback(gpa: std.mem.Allocator, io: std.Io, releases_dir_path: []const u8) ![]const u8 {
    var rel_dir = try std.Io.Dir.cwd().openDir(io, releases_dir_path, .{});
    defer rel_dir.close(io);
    const prev = read_marker(gpa, io, rel_dir, "previous.txt") orelse return error.NoPreviousRelease;
    errdefer gpa.free(prev);
    try rel_dir.writeFile(io, .{ .sub_path = "current.txt", .data = prev });
    if (builtin.os.tag != .windows) {
        rel_dir.deleteFile(io, "current") catch {};
        std.Io.Dir.symLink(rel_dir, io, prev, "current", .{ .is_directory = true }) catch {};
    }
    try write_probation(gpa, io, rel_dir, .{ .active = prev, .previous = null, .healthy = true });
    return prev;
}

fn read_marker(gpa: std.mem.Allocator, io: std.Io, dir: std.Io.Dir, name: []const u8) ?[]const u8 {
    const raw = dir.readFileAlloc(io, name, gpa, .limited(4096)) catch return null;
    defer gpa.free(raw);
    const tr = std.mem.trim(u8, raw, " \t\r\n");
    if (tr.len == 0) return null;
    return gpa.dupe(u8, tr) catch null;
}

fn write_probation(gpa: std.mem.Allocator, io: std.Io, dir: std.Io.Dir, p: Probation) !void {
    var aw: std.Io.Writer.Allocating = .init(gpa);
    defer aw.deinit();
    var jw: std.json.Stringify = .{ .writer = &aw.writer };
    try jw.beginObject();
    try jw.objectField("active");
    try jw.write(p.active);
    if (p.previous) |pr| {
        try jw.objectField("previous");
        try jw.write(pr);
    }
    try jw.objectField("healthy");
    try jw.write(p.healthy);
    try jw.endObject();
    try dir.writeFile(io, .{ .sub_path = "probation.json.tmp", .data = aw.written() });
    try std.Io.Dir.rename(dir, "probation.json.tmp", dir, "probation.json", io);
}
