const std = @import("std");

const Tier = enum(u8) {
    l0 = 0,
    l1 = 1,
    l2 = 2,
    l3 = 3,
    l4 = 4,
    l5 = 5,

    fn label(self: Tier) []const u8 {
        return switch (self) {
            .l0 => "L0",
            .l1 => "L1",
            .l2 => "L2",
            .l3 => "L3",
            .l4 => "L4",
            .l5 => "L5",
        };
    }
};

const Group = enum {
    root,
    net,
    journal,
    history,
    core,
    quota,
    update,
    api,
    drivers,
    sandbox,
    doctor,
    node,
    hub,
    cli,
    dst,
    tools,
};

const Classification = struct {
    tier: Tier,
    group: Group,
};

const SourceFile = struct {
    rel_path: []const u8,
    class: Classification,
    imports: [][]const u8,
};

fn classify(rel_path: []const u8) ?Classification {
    if (std.mem.eql(u8, rel_path, "tiger.zig") or
        std.mem.eql(u8, rel_path, "version.zig"))
    {
        return .{ .tier = .l0, .group = .root };
    }
    if (std.mem.eql(u8, rel_path, "child_process.zig") or
        std.mem.eql(u8, rel_path, "qr.zig") or
        std.mem.eql(u8, rel_path, "json_util.zig"))
    {
        return .{ .tier = .l1, .group = .root };
    }
    if (std.mem.eql(u8, rel_path, "net/frame.zig") or
        std.mem.eql(u8, rel_path, "net/noise.zig") or
        std.mem.eql(u8, rel_path, "net/limits.zig") or
        std.mem.eql(u8, rel_path, "net/io.zig") or
        std.mem.eql(u8, rel_path, "net/io_epoll.zig") or
        std.mem.eql(u8, rel_path, "net/bounded.zig"))
    {
        return .{ .tier = .l1, .group = .net };
    }
    if (std.mem.eql(u8, rel_path, "journal/codec.zig")) {
        return .{ .tier = .l1, .group = .journal };
    }
    if (std.mem.eql(u8, rel_path, "history/token.zig")) {
        return .{ .tier = .l1, .group = .history };
    }
    if (std.mem.eql(u8, rel_path, "core/model.zig")) {
        return .{ .tier = .l1, .group = .core };
    }
    if (std.mem.eql(u8, rel_path, "quota/select.zig")) {
        return .{ .tier = .l1, .group = .quota };
    }
    if (std.mem.eql(u8, rel_path, "update/version.zig")) {
        return .{ .tier = .l1, .group = .update };
    }
    if (std.mem.eql(u8, rel_path, "api/schema.zig")) {
        return .{ .tier = .l2, .group = .api };
    }
    if (std.mem.eql(u8, rel_path, "git_process.zig")) {
        return .{ .tier = .l2, .group = .root };
    }
    if (std.mem.eql(u8, rel_path, "net/msg.zig") or std.mem.eql(u8, rel_path, "net/channel.zig") or
        std.mem.eql(u8, rel_path, "net/handshake.zig") or std.mem.eql(u8, rel_path, "net/fleet_bus.zig") or
        std.mem.eql(u8, rel_path, "net/node_link.zig"))
    {
        return .{ .tier = .l2, .group = .net };
    }
    if (std.mem.startsWith(u8, rel_path, "core/")) {
        return .{ .tier = .l2, .group = .core };
    }
    if (std.mem.eql(u8, rel_path, "drivers/session.zig")) {
        return .{ .tier = .l3, .group = .drivers };
    }
    if (std.mem.startsWith(u8, rel_path, "drivers/")) {
        return .{ .tier = .l2, .group = .drivers };
    }
    if (std.mem.eql(u8, rel_path, "history/index.zig")) {
        return .{ .tier = .l3, .group = .history };
    }
    if (std.mem.startsWith(u8, rel_path, "history/")) {
        return .{ .tier = .l2, .group = .history };
    }
    if (std.mem.eql(u8, rel_path, "sandbox/spec.zig")) {
        return .{ .tier = .l2, .group = .sandbox };
    }
    if (std.mem.eql(u8, rel_path, "journal/store.zig")) {
        return .{ .tier = .l3, .group = .journal };
    }
    if (std.mem.eql(u8, rel_path, "quota/readers.zig")) {
        return .{ .tier = .l3, .group = .quota };
    }
    if (std.mem.eql(u8, rel_path, "sandbox/exec.zig")) {
        return .{ .tier = .l3, .group = .sandbox };
    }
    if (std.mem.eql(u8, rel_path, "net/transport.zig")) {
        return .{ .tier = .l3, .group = .net };
    }
    if (std.mem.eql(u8, rel_path, "update/store.zig")) {
        return .{ .tier = .l3, .group = .update };
    }
    if (std.mem.eql(u8, rel_path, "doctor/doctor.zig")) {
        return .{ .tier = .l3, .group = .doctor };
    }
    if (std.mem.startsWith(u8, rel_path, "node/")) {
        return .{ .tier = .l4, .group = .node };
    }
    if (std.mem.startsWith(u8, rel_path, "hub/")) {
        return .{ .tier = .l4, .group = .hub };
    }
    if (std.mem.startsWith(u8, rel_path, "cli/")) {
        return .{ .tier = .l4, .group = .cli };
    }
    if (std.mem.eql(u8, rel_path, "main_hub.zig") or
        std.mem.eql(u8, rel_path, "main_node.zig") or
        std.mem.eql(u8, rel_path, "main_levels.zig") or
        std.mem.eql(u8, rel_path, "main_apigen.zig") or
        std.mem.eql(u8, rel_path, "main_embed_web.zig") or
        std.mem.eql(u8, rel_path, "main_vopr.zig"))
    {
        return .{ .tier = .l5, .group = .root };
    }
    if (std.mem.startsWith(u8, rel_path, "dst/")) {
        return .{ .tier = .l5, .group = .dst };
    }
    if (std.mem.startsWith(u8, rel_path, "tools/")) {
        return .{ .tier = .l5, .group = .tools };
    }
    return null;
}

fn is_external_import(name: []const u8) bool {
    return std.mem.eql(u8, name, "std") or
        std.mem.eql(u8, name, "builtin") or
        std.mem.eql(u8, name, "build_options") or
        std.mem.eql(u8, name, "web_assets");
}

fn resolve_rel_path(gpa: std.mem.Allocator, from_file: []const u8, import_str: []const u8) ![]const u8 {
    var stack: std.ArrayList([]const u8) = .empty;
    defer stack.deinit(gpa);

    if (std.mem.findScalarLast(u8, from_file, '/')) |slash| {
        var it = std.mem.splitScalar(u8, from_file[0..slash], '/');
        while (it.next()) |seg| {
            if (seg.len > 0) try stack.append(gpa, seg);
        }
    }

    var it = std.mem.splitScalar(u8, import_str, '/');
    while (it.next()) |seg| {
        if (seg.len == 0 or std.mem.eql(u8, seg, ".")) continue;
        if (std.mem.eql(u8, seg, "..")) {
            if (stack.items.len == 0) return error.EscapesSrcRoot;
            _ = stack.pop();
        } else {
            try stack.append(gpa, seg);
        }
    }

    return std.mem.join(gpa, "/", stack.items);
}

fn normalize_slashes(gpa: std.mem.Allocator, p: []const u8) ![]const u8 {
    const out = try gpa.dupe(u8, p);
    for (out) |*c| {
        if (c.* == '\\') c.* = '/';
    }
    return out;
}

fn parse_imports_and_check_tests(
    gpa: std.mem.Allocator,
    rel_path: []const u8,
    src_bytes: [:0]const u8,
    errors: *usize,
) ![][]const u8 {
    var imports: std.ArrayList([]const u8) = .empty;
    errdefer imports.deinit(gpa);

    var tok = std.zig.Tokenizer.init(src_bytes);
    var prev2: std.zig.Token.Tag = .eof;
    var prev1: std.zig.Token.Tag = .eof;
    var prev2_loc: std.zig.Token.Loc = .{ .start = 0, .end = 0 };

    while (true) {
        const t = tok.next();
        if (t.tag == .eof) break;

        if (t.tag == .keyword_test) {
            std.debug.print("levels: FORBIDDEN unit test keyword in {s}\n", .{rel_path});
            errors.* += 1;
        }

        if (prev2 == .builtin and prev1 == .l_paren and t.tag == .string_literal) {
            const builtin_name = src_bytes[prev2_loc.start..prev2_loc.end];
            if (std.mem.eql(u8, builtin_name, "@import")) {
                const raw = src_bytes[t.loc.start..t.loc.end];
                if (raw.len >= 2 and raw[0] == '"' and raw[raw.len - 1] == '"') {
                    const inner = raw[1 .. raw.len - 1];
                    if (!is_external_import(inner)) {
                        const resolved = resolve_rel_path(gpa, rel_path, inner) catch {
                            std.debug.print("levels: import {s} in {s} escapes src root\n", .{ inner, rel_path });
                            errors.* += 1;
                            prev2 = prev1;
                            prev1 = t.tag;
                            continue;
                        };
                        try imports.append(gpa, resolved);
                    }
                }
            }
        }

        prev2 = prev1;
        prev2_loc = if (prev1 == .builtin) prev2_loc else t.loc;
        if (t.tag == .builtin) {
            prev2_loc = t.loc;
        }
        prev1 = t.tag;
    }

    return imports.toOwnedSlice(gpa);
}

fn check_allowed_edge(src_file: SourceFile, dst_file: SourceFile) ?[]const u8 {
    if (src_file.class.group == .dst) {
        if (dst_file.class.group == .dst) return null;
        if (@intFromEnum(dst_file.class.tier) <= @intFromEnum(Tier.l2)) return null;
        return "dst module may only import L0-L2 or dst/*";
    }
    if (src_file.class.group == .tools) {
        if (dst_file.class.group == .tools) return null;
        if (@intFromEnum(dst_file.class.tier) <= @intFromEnum(Tier.l2)) return null;
        return "tools module may only import L0-L2 or tools/*";
    }

    if (src_file.class.tier == .l5 and src_file.class.group == .root) {
        return null;
    }

    const src_tier = @intFromEnum(src_file.class.tier);
    const dst_tier = @intFromEnum(dst_file.class.tier);
    if (dst_tier > src_tier) {
        return "upward import to higher layer";
    }
    if (dst_tier == src_tier and dst_file.class.group != src_file.class.group) {
        return "cross-subsystem horizontal import within same layer";
    }
    if (src_file.class.tier == .l0) {
        return "L0 module cannot import local modules";
    }
    return null;
}

fn has_cycle(
    idx: usize,
    files: []const SourceFile,
    map: *const std.StringHashMapUnmanaged(usize),
    color: []u8,
) bool {
    if (color[idx] == 2) return false;
    if (color[idx] == 1) return true;
    color[idx] = 1;
    for (files[idx].imports) |imp| {
        if (map.get(imp)) |dst_idx| {
            if (has_cycle(dst_idx, files, map, color)) {
                std.debug.print("levels: cycle edge {s} -> {s}\n", .{ files[idx].rel_path, files[dst_idx].rel_path });
                return true;
            }
        }
    }
    color[idx] = 2;
    return false;
}

pub fn main(init: std.process.Init) !u8 {
    var arena_state = std.heap.ArenaAllocator.init(init.gpa);
    defer arena_state.deinit();
    const arena = arena_state.allocator();

    const args = try init.minimal.args.toSlice(arena);
    var root_path: []const u8 = "src";
    var print_table = false;

    var i: usize = 1;
    while (i < args.len) : (i += 1) {
        if (std.mem.eql(u8, args[i], "--root") and i + 1 < args.len) {
            i += 1;
            root_path = args[i];
        } else if (std.mem.eql(u8, args[i], "--print")) {
            print_table = true;
        }
    }

    var dir = try std.Io.Dir.cwd().openDir(init.io, root_path, .{ .iterate = true });
    defer dir.close(init.io);

    var walker = try dir.walk(arena);
    defer walker.deinit();

    var paths: std.ArrayList([]const u8) = .empty;
    while (try walker.next(init.io)) |entry| {
        if (entry.kind != .file) continue;
        if (!std.mem.endsWith(u8, entry.path, ".zig")) continue;
        const norm = try normalize_slashes(arena, entry.path);
        try paths.append(arena, norm);
    }

    std.mem.sort([]const u8, paths.items, {}, struct {
        fn less(_: void, a: []const u8, b: []const u8) bool {
            return std.mem.lessThan(u8, a, b);
        }
    }.less);

    var errors: usize = 0;
    var files: std.ArrayList(SourceFile) = .empty;
    var index_map: std.StringHashMapUnmanaged(usize) = .empty;

    for (paths.items) |rel_path| {
        const cls = classify(rel_path) orelse {
            std.debug.print("levels: unclassified file in src: {s}\n", .{rel_path});
            errors += 1;
            continue;
        };
        const bytes = try dir.readFileAllocOptions(
            init.io,
            rel_path,
            arena,
            .limited(8 * 1024 * 1024),
            .of(u8),
            0,
        );
        const imps = try parse_imports_and_check_tests(arena, rel_path, bytes, &errors);
        const idx = files.items.len;
        try files.append(arena, .{
            .rel_path = rel_path,
            .class = cls,
            .imports = imps,
        });
        try index_map.put(arena, rel_path, idx);
    }

    for (files.items) |src_file| {
        for (src_file.imports) |imp| {
            const dst_idx = index_map.get(imp) orelse {
                std.debug.print("levels: missing import target {s} from {s}\n", .{ imp, src_file.rel_path });
                errors += 1;
                continue;
            };
            const dst_file = files.items[dst_idx];
            if (check_allowed_edge(src_file, dst_file)) |reason| {
                std.debug.print(
                    "levels: violation ({s}) {s} ({s}) -> {s} ({s})\n",
                    .{ reason, src_file.rel_path, src_file.class.tier.label(), dst_file.rel_path, dst_file.class.tier.label() },
                );
                errors += 1;
            }
        }
    }

    const color = try arena.alloc(u8, files.items.len);
    @memset(color, 0);
    for (0..files.items.len) |idx| {
        if (color[idx] == 0) {
            if (has_cycle(idx, files.items, &index_map, color)) {
                errors += 1;
            }
        }
    }

    if (print_table) {
        var out_buf: [4096]u8 = undefined;
        var out = std.Io.File.stdout().writer(init.io, &out_buf);
        for (files.items) |f| {
            try out.interface.print("{s}  {s}\n", .{ f.class.tier.label(), f.rel_path });
        }
        try out.interface.flush();
    }

    if (errors > 0) {
        std.debug.print("levels: failed with {d} error(s)\n", .{errors});
        return 1;
    }
    return 0;
}
