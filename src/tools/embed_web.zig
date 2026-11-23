const std = @import("std");

fn content_type_for(path: []const u8) []const u8 {
    if (std.mem.endsWith(u8, path, ".html")) return "text/html; charset=utf-8";
    if (std.mem.endsWith(u8, path, ".js") or std.mem.endsWith(u8, path, ".mjs")) return "application/javascript; charset=utf-8";
    if (std.mem.endsWith(u8, path, ".css")) return "text/css; charset=utf-8";
    if (std.mem.endsWith(u8, path, ".json") or std.mem.endsWith(u8, path, ".manifest")) return "application/json; charset=utf-8";
    if (std.mem.endsWith(u8, path, ".wasm")) return "application/wasm";
    if (std.mem.endsWith(u8, path, ".png")) return "image/png";
    if (std.mem.endsWith(u8, path, ".svg")) return "image/svg+xml";
    if (std.mem.endsWith(u8, path, ".ico")) return "image/x-icon";
    if (std.mem.endsWith(u8, path, ".ttf")) return "font/ttf";
    if (std.mem.endsWith(u8, path, ".otf")) return "font/otf";
    if (std.mem.endsWith(u8, path, ".woff")) return "font/woff";
    if (std.mem.endsWith(u8, path, ".woff2")) return "font/woff2";
    return "application/octet-stream";
}

fn normalize_slashes(gpa: std.mem.Allocator, p: []const u8) ![]const u8 {
    const out = try gpa.dupe(u8, p);
    for (out) |*c| {
        if (c.* == '\\') c.* = '/';
    }
    return out;
}

pub fn main(init: std.process.Init) !u8 {
    var arena_state = std.heap.ArenaAllocator.init(init.gpa);
    defer arena_state.deinit();
    const arena = arena_state.allocator();

    const args = try init.minimal.args.toSlice(arena);
    var in_dir: ?[]const u8 = null;
    var out_dir: ?[]const u8 = null;

    var i: usize = 1;
    while (i < args.len) : (i += 1) {
        if (std.mem.eql(u8, args[i], "--in") and i + 1 < args.len) {
            i += 1;
            in_dir = args[i];
        } else if (std.mem.eql(u8, args[i], "--out") and i + 1 < args.len) {
            i += 1;
            out_dir = args[i];
        }
    }

    const od_path = out_dir orelse return 1;
    var out_root = try std.Io.Dir.cwd().createDirPathOpen(init.io, od_path, .{});
    defer out_root.close(init.io);

    const comp_ptr = try arena.create(std.compress.flate.Compress);
    const comp_buf = try arena.alloc(u8, std.compress.flate.max_window_len);

    const AssetRec = struct {
        rel: []const u8,
        embed_rel: []const u8,
        gzipped: bool,
    };
    var entries: std.ArrayList(AssetRec) = .empty;

    if (in_dir) |id_path| {
        var in_root_opt: ?std.Io.Dir = std.Io.Dir.cwd().openDir(init.io, id_path, .{ .iterate = true }) catch null;
        if (in_root_opt) |*in_root| {
            defer in_root.close(init.io);
            var walker = try in_root.walk(arena);
            defer walker.deinit();

            while (try walker.next(init.io)) |entry| {
                if (entry.kind != .file) continue;
                const rel = try normalize_slashes(arena, entry.path);
                if (std.mem.eql(u8, rel, ".last_build_id")) continue;
                const data = try in_root.readFileAlloc(init.io, rel, arena, .limited(64 * 1024 * 1024));

                var gz_aw = try std.Io.Writer.Allocating.initCapacity(arena, @max(@as(usize, 4096), data.len / 2));
                comp_ptr.* = try std.compress.flate.Compress.init(&gz_aw.writer, comp_buf, .gzip, .level_6);
                try comp_ptr.writer.writeAll(data);
                try comp_ptr.finish();

                const gz_bytes = gz_aw.written();
                const use_gz = gz_bytes.len + 64 < data.len;
                const payload = if (use_gz) gz_bytes else data;
                const dest_rel = if (use_gz)
                    try std.fmt.allocPrint(arena, "files/{s}.gz", .{rel})
                else
                    try std.fmt.allocPrint(arena, "files/{s}", .{rel});

                if (std.mem.findScalarLast(u8, dest_rel, '/')) |slash| {
                    try out_root.createDirPath(init.io, dest_rel[0..slash]);
                }
                try out_root.writeFile(init.io, .{ .sub_path = dest_rel, .data = payload });
                try entries.append(arena, .{
                    .rel = rel,
                    .embed_rel = dest_rel,
                    .gzipped = use_gz,
                });
            }
        }
    }

    std.mem.sort(AssetRec, entries.items, {}, struct {
        fn less(_: void, a: AssetRec, b: AssetRec) bool {
            return std.mem.lessThan(u8, a.rel, b.rel);
        }
    }.less);

    var aw: std.Io.Writer.Allocating = .init(arena);
    const w = &aw.writer;
    try w.writeAll(
        \\const std = @import("std");
        \\
        \\pub const Asset = struct {
        \\    path: []const u8,
        \\    content_type: []const u8,
        \\    bytes: []const u8,
        \\    gzipped: bool,
        \\};
        \\
        \\pub const assets = [_]Asset{
        \\
    );
    for (entries.items) |rec| {
        const ct = content_type_for(rec.rel);
        try w.writeAll("    .{ .path = ");
        try std.json.Stringify.encodeJsonString(rec.rel, .{}, w);
        try w.writeAll(", .content_type = ");
        try std.json.Stringify.encodeJsonString(ct, .{}, w);
        try w.writeAll(", .bytes = @embedFile(");
        try std.json.Stringify.encodeJsonString(rec.embed_rel, .{}, w);
        try w.print("), .gzipped = {} }},\n", .{rec.gzipped});
    }
    try w.writeAll(
        \\};
        \\
        \\pub fn find(path: []const u8) ?Asset {
        \\    const clean = if (std.mem.startsWith(u8, path, "/")) path[1..] else path;
        \\    const target = if (clean.len == 0) "index.html" else clean;
        \\    for (assets) |a| {
        \\        if (std.mem.eql(u8, a.path, target)) return a;
        \\    }
        \\    return null;
        \\}
        \\
    );

    try out_root.writeFile(init.io, .{
        .sub_path = "assets.zig",
        .data = aw.written(),
    });
    return 0;
}
