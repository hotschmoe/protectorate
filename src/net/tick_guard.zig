const std = @import("std");
const builtin = @import("builtin");

pub const slow_tick_ms: u64 = 50;
const report_gap_ms: u64 = 5_000;
const enabled = builtin.mode == .debug or builtin.mode == .safe;

/// Times one loop turn in safe builds. A turn over slow_tick_ms is reported with its longest step, at most once per
/// report_gap_ms: a slow disk or a blocking call that slipped into a loop shows up in the log, never as a crash.
pub const TickGuard = struct {
    name: []const u8,
    began: u64 = 0,
    mark_at: u64 = 0,
    worst_ms: u64 = 0,
    worst_step: []const u8 = "",
    last_report: u64 = 0,
    reported: bool = false,
    slow_since_report: u64 = 0,

    pub fn begin(self: *TickGuard, backend: anytype) void {
        if (!enabled) return;
        const now = backend.now();
        self.began = now;
        self.mark_at = now;
        self.worst_ms = 0;
        self.worst_step = "";
    }

    /// Charges the time since the previous mark to step, the work that just finished.
    pub fn mark(self: *TickGuard, backend: anytype, step: []const u8) void {
        if (!enabled) return;
        const now = backend.now();
        const spent = now -| self.mark_at;
        self.mark_at = now;
        if (spent > self.worst_ms) {
            self.worst_ms = spent;
            self.worst_step = step;
        }
    }

    pub fn end(self: *TickGuard, backend: anytype, step: []const u8) void {
        if (!enabled) return;
        self.mark(backend, step);
        const now = self.mark_at;
        const total = now -| self.began;
        if (total <= slow_tick_ms) return;
        self.slow_since_report += 1;
        if (self.reported and now -| self.last_report < report_gap_ms) return;
        std.debug.print("{s} loop: a turn took {d} ms (bound {d}), longest step {s} {d} ms; {d} slow turn(s) since the last report\n", .{
            self.name, total, slow_tick_ms, self.worst_step, self.worst_ms, self.slow_since_report,
        });
        self.reported = true;
        self.last_report = now;
        self.slow_since_report = 0;
    }
};
