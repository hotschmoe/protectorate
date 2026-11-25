const std = @import("std");
const tiger = @import("../tiger.zig");
const wire = @import("http_wire.zig");

/// The server side of one live WebSocket: reads records from a ring and stages frames into the caller's
/// send buffer, one send at a time. A data frame's payload is never interrupted: ping replies and close
/// frames wait for the end of the frame in progress, so the bytes on the wire always parse as whole frames.
pub fn Out(comptime Ring: type) type {
    return struct {
        const Self = @This();
        const Record = struct { pos: u64, next: u64, len: usize, sent: usize };

        pub const Step = union(enum) {
            /// n bytes of the send buffer are ready to send.
            staged: usize,
            /// n bytes ready, ending in a close frame: close the transport once they are sent.
            close_staged: usize,
            idle,
            /// The cursor is gone from the ring (or at a gap): send a fresh snapshot.
            resync,
            /// A frame already partly sent was overwritten in the ring: drop the connection.
            abort,
        };

        cursor: u64 = 0,
        record: ?Record = null,
        pong: [wire.control_payload_max]u8 = undefined,
        pong_len: ?usize = null,
        close: ?struct { code: ?u16 } = null,
        ping_at: u64 = 0,

        pub fn queue_pong(self: *Self, payload: []const u8) void {
            tiger.assert(payload.len <= self.pong.len);
            @memcpy(self.pong[0..payload.len], payload);
            self.pong_len = payload.len;
        }

        /// The first request wins: after a close, nothing else is sent.
        pub fn queue_close(self: *Self, code: ?u16) void {
            if (self.close == null) self.close = .{ .code = code };
        }

        fn frame(sbuf: []u8, at: usize, opcode: wire.Opcode, payload: []const u8) usize {
            var header: [10]u8 = undefined;
            const h = wire.ws_server_header(&header, opcode, payload.len);
            @memcpy(sbuf[at..][0..h.len], h);
            @memcpy(sbuf[at + h.len ..][0..payload.len], payload);
            return h.len + payload.len;
        }

        /// A close or pong frame, if one is waiting and no data frame is in progress.
        pub fn stage_control(self: *Self, sbuf: []u8) ?Step {
            if (self.record != null) return null;
            if (self.close) |c| {
                var payload: [2]u8 = undefined;
                if (c.code) |code| std.mem.writeInt(u16, &payload, code, .big);
                return .{ .close_staged = frame(sbuf, 0, .close, if (c.code != null) payload[0..2] else "") };
            }
            if (self.pong_len) |len| {
                self.pong_len = null;
                var payload: [wire.control_payload_max]u8 = undefined;
                @memcpy(payload[0..len], self.pong[0..len]);
                return .{ .staged = frame(sbuf, 0, .pong, payload[0..len]) };
            }
            return null;
        }

        /// Stages the next send. Call it only when everything staged before has been sent.
        pub fn stage(self: *Self, ring: *Ring, sbuf: []u8, now: u64, ping_ms: u64) Step {
            if (self.stage_control(sbuf)) |step| return step;
            var staged: usize = 0;
            var started_at: usize = 0;
            var started_here = false;
            while (true) {
                if (self.record == null) {
                    switch (ring.peek(self.cursor)) {
                        .empty => break,
                        .lagged => {
                            if (staged == 0) return .resync;
                            break;
                        },
                        .record => |r| {
                            if (r.len == 0) {
                                if (staged == 0) return .resync;
                                break;
                            }
                            var header: [10]u8 = undefined;
                            const h = wire.ws_server_header(&header, .text, r.len);
                            if (staged > 0 and h.len + r.len > sbuf.len - staged) break;
                            started_at = staged;
                            started_here = true;
                            @memcpy(sbuf[staged..][0..h.len], h);
                            staged += h.len;
                            self.record = .{ .pos = self.cursor, .next = r.next, .len = r.len, .sent = 0 };
                        },
                    }
                }
                const record = &self.record.?;
                const n = @min(record.len - record.sent, sbuf.len - staged);
                if (!ring.read(record.pos, record.sent, sbuf[staged..][0..n])) {
                    if (!started_here or record.sent != 0) return .abort;
                    staged = started_at;
                    self.record = null;
                    break;
                }
                staged += n;
                record.sent += n;
                started_here = false;
                if (record.sent < record.len) break;
                self.cursor = record.next;
                self.record = null;
            }
            if (staged == 0) {
                if (self.record != null or now < self.ping_at) return .idle;
                staged = frame(sbuf, 0, .ping, "");
            }
            self.ping_at = now + ping_ms;
            return .{ .staged = staged };
        }
    };
}
