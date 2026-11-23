const std = @import("std");
const tiger = @import("../tiger.zig");
const model = @import("../core/model.zig");

pub fn write_handoff(
    gpa: std.mem.Allocator,
    io: std.Io,
    briefs_dir: []const u8,
    sleeve_id: []const u8,
    g: model.Gen,
    cause: model.Cause,
    summary: []const u8,
) !void {
    var dir = try std.Io.Dir.cwd().createDirPathOpen(io, briefs_dir, .{});
    defer dir.close(io);

    const fname = try std.fmt.allocPrint(gpa, "{s}.md", .{sleeve_id});
    defer gpa.free(fname);
    const tmp_name = try std.fmt.allocPrint(gpa, "{s}.md.tmp", .{sleeve_id});
    defer gpa.free(tmp_name);

    const content = try std.fmt.allocPrint(
        gpa,
        "# Sleeve Handoff Brief ({s})\n\nGeneration: {d}\nCause: {s}\n\n{s}\n",
        .{ sleeve_id, g, @tagName(cause), summary },
    );
    defer gpa.free(content);

    try dir.writeFile(io, .{ .sub_path = tmp_name, .data = content });
    try std.Io.Dir.rename(dir, tmp_name, dir, fname, io);
}

pub fn read_brief(arena: std.mem.Allocator, io: std.Io, briefs_dir: []const u8, sleeve_id: []const u8) ?[]const u8 {
    const p = std.fmt.allocPrint(arena, "{s}/{s}.md", .{ briefs_dir, sleeve_id }) catch return null;
    return std.Io.Dir.cwd().readFileAlloc(io, p, arena, .limited(1 << 20)) catch null;
}
