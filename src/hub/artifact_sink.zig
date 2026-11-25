const std = @import("std");
const tiger = @import("../tiger.zig");

const Sha256 = std.crypto.hash.sha2.Sha256;

pub const Error = error{
    ArtifactTooLarge,
    ArtifactSizeMismatch,
    ChecksumMismatch,
    InvalidArtifactPath,
};

/// Receives one artifact piece by piece into a temporary file beside its destination,
/// hashing as it goes, and renames it into place only after size and digest match.
/// It does no network work, so an event loop can drive it as pieces arrive.
pub const Sink = struct {
    io: std.Io,
    file: std.Io.File,
    tmp_path: []const u8,
    dest_path: []const u8,
    hasher: Sha256,
    declared: u64,
    cap: u64,
    written: u64 = 0,
    closed: bool = false,

    pub fn open(
        io: std.Io,
        arena: std.mem.Allocator,
        artifacts_root: []const u8,
        job_id: []const u8,
        rel_path: []const u8,
        declared: u64,
        cap: u64,
    ) !Sink {
        if (declared > cap) return error.ArtifactTooLarge;
        if (rel_path.len == 0 or std.fs.path.isAbsolute(rel_path) or
            std.mem.findPos(u8, rel_path, 0, "..") != null)
        {
            return error.InvalidArtifactPath;
        }
        const job_dir = try std.fs.path.join(arena, &.{ artifacts_root, job_id });
        const dest = try std.fs.path.join(arena, &.{ job_dir, rel_path });
        const dest_dir = std.fs.path.dirname(dest) orelse job_dir;
        try std.Io.Dir.cwd().createDirPath(io, dest_dir);
        const tmp = try std.fmt.allocPrint(arena, "{s}.ptpart", .{dest});
        const file = try std.Io.Dir.cwd().createFile(io, tmp, .{ .truncate = true });
        return .{
            .io = io,
            .file = file,
            .tmp_path = tmp,
            .dest_path = dest,
            .hasher = Sha256.init(.{}),
            .declared = declared,
            .cap = cap,
        };
    }

    pub fn write(self: *Sink, piece: []const u8) !void {
        tiger.assert(!self.closed);
        tiger.assert(self.written <= self.declared and self.declared <= self.cap);
        if (piece.len > self.cap - self.written) return error.ArtifactTooLarge;
        if (piece.len > self.declared - self.written) return error.ArtifactSizeMismatch;
        try self.file.writePositionalAll(self.io, piece, self.written);
        self.hasher.update(piece);
        self.written += piece.len;
    }

    /// Verifies size and digest, renames into place and returns the lowercase hex digest.
    pub fn finish(self: *Sink, expected_sha: []const u8) ![64]u8 {
        tiger.assert(!self.closed);
        self.file.close(self.io);
        self.closed = true;
        if (self.written != self.declared) return error.ArtifactSizeMismatch;
        var digest: [32]u8 = undefined;
        self.hasher.final(&digest);
        const chars = "0123456789abcdef";
        var hex: [64]u8 = undefined;
        for (digest, 0..) |b, i| {
            hex[i * 2] = chars[b >> 4];
            hex[i * 2 + 1] = chars[b & 0x0f];
        }
        if (expected_sha.len == 64 and !std.mem.eql(u8, &hex, expected_sha)) {
            return error.ChecksumMismatch;
        }
        try std.Io.Dir.rename(std.Io.Dir.cwd(), self.tmp_path, std.Io.Dir.cwd(), self.dest_path, self.io);
        return hex;
    }

    /// Releases the file and removes the temporary. Safe after a failed finish; call at most once.
    pub fn abort(self: *Sink) void {
        if (!self.closed) {
            self.file.close(self.io);
            self.closed = true;
        }
        std.Io.Dir.cwd().deleteFile(self.io, self.tmp_path) catch {};
    }
};
