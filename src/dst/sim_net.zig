const std = @import("std");
const tiger = @import("../tiger.zig");
const noise = @import("../net/noise.zig");
const frame = @import("../net/frame.zig");
const msg = @import("../net/msg.zig");

fn step_rng_seed(rng: std.Random) [32]u8 {
    var s: [32]u8 = undefined;
    rng.bytes(&s);
    return s;
}

pub fn run_net_sim(gpa: std.mem.Allocator, seed: u64) !void {
    var prng = std.Random.DefaultPrng.init(seed);
    const rng = prng.random();

    const hub_s = noise.KeyPair.generate_deterministic(step_rng_seed(rng));
    const node_s = noise.KeyPair.generate_deterministic(step_rng_seed(rng));
    const wrong_s = noise.KeyPair.generate_deterministic(step_rng_seed(rng));
    const node_e = noise.KeyPair.generate_deterministic(step_rng_seed(rng));
    const hub_e = noise.KeyPair.generate_deterministic(step_rng_seed(rng));

    // Confirm fingerprint derivation consistency.
    const fp1 = noise.fingerprint_prefixed(node_s.public);
    const fp2 = noise.fingerprint_prefixed(node_s.public);
    tiger.assert(std.mem.eql(u8, &fp1, &fp2));

    var msg1: [noise.msg1_len]u8 = undefined;
    var init_hs = try noise.InitiatorHandshake.start(node_s, node_e, hub_s.public, &msg1);

    // Responder must reject unknown peer static key!
    var bad_msg2: [noise.msg2_len]u8 = undefined;
    if (noise.responder_accept(hub_s, hub_e, wrong_s.public, &msg1, &bad_msg2, 1000)) |*bad_sess| {
        @constCast(bad_sess).deinit();
        tiger.assert(false);
    } else |err| {
        tiger.assert(err == error.UnknownPeer);
    }

    // Tampered msg1 must fail authentication.
    var bad_msg1 = msg1;
    bad_msg1[rng.uintLessThan(usize, noise.msg1_len)] ^= 0x01;
    if (noise.responder_accept(hub_s, hub_e, node_s.public, &bad_msg1, &bad_msg2, 1000)) |*bad_sess| {
        @constCast(bad_sess).deinit();
        tiger.assert(false);
    } else |_| {}

    var msg2: [noise.msg2_len]u8 = undefined;
    var hub_sess = try noise.responder_accept(hub_s, noise.KeyPair.generate_deterministic(step_rng_seed(rng)), node_s.public, &msg1, &msg2, 1000);
    defer hub_sess.deinit();

    var node_sess = try init_hs.finish(&msg2, 1000);
    defer node_sess.deinit();

    tiger.assert(std.mem.eql(u8, &hub_sess.handshake_hash, &node_sess.handshake_hash));
    tiger.assert(!hub_sess.expired(1000));
    tiger.assert(hub_sess.expired(1000 + noise.max_session_ms));

    // Encode a fleet SubmitJob RPC into pt-frame, split into Noise records, deliver 1 byte at a time.
    var wire_frames: std.ArrayList(u8) = .empty;
    defer wire_frames.deinit(gpa);

    const req = msg.Request{
        .submit_job = .{
            .job_id = "job-dst-1",
            .argv = &.{ "echo", "hello-fleet" },
            .artifacts = &.{"out/bin"},
            .even_if_present = true,
            .on_present = .idle,
        },
    };
    const req_json = try msg.encode_alloc(gpa, req);
    defer gpa.free(req_json);

    try frame.encode_into(gpa, .request, 42, req_json, &wire_frames);

    // Also include an UploadUpdate frame to exercise multiple frames back-to-back.
    const ping_json = try msg.encode_alloc(gpa, msg.Request{ .ping = .{ .now_ms = 555 } });
    defer gpa.free(ping_json);
    try frame.encode_into(gpa, .request, 43, ping_json, &wire_frames);

    // Seal in small chunks to simulate multi-record Noise traffic, open on peer via 1-byte stream simulation.
    var sealed_stream: std.ArrayList(u8) = .empty;
    defer sealed_stream.deinit(gpa);

    var rec_buf: [noise.max_record_wire]u8 = undefined;
    var off: usize = 0;
    while (off < wire_frames.items.len) {
        const chunk_max = @min(@as(usize, 37), wire_frames.items.len - off);
        const take = 1 + rng.uintLessThan(usize, chunk_max);
        const sealed = try node_sess.seal_record(wire_frames.items[off .. off + take], &rec_buf);
        try sealed_stream.appendSlice(gpa, sealed);
        off += take;
    }

    // Tamper check on a copy of the first encrypted record.
    const tamper = try gpa.dupe(u8, sealed_stream.items);
    defer gpa.free(tamper);
    const rlen = std.mem.readInt(u16, tamper[0..2], .big);
    if (rlen > 0) {
        tamper[2 + rng.uintLessThan(usize, rlen)] ^= 0x80;
        var dummy_plain: [noise.max_record_plaintext]u8 = undefined;
        var tmp_rx = hub_sess.rx;
        if (tmp_rx.decrypt_with_ad(&.{}, tamper[2 .. 2 + rlen], &dummy_plain)) |_| {
            tiger.assert(false);
        } else |err| {
            tiger.assert(err == error.AuthenticationFailed);
        }
    }

    // Open all records sequentially and feed pt-frame Decoder 1 byte at a time.
    var dec = frame.Decoder{};
    defer dec.deinit(gpa);

    var decoded_count: usize = 0;
    var wire_pos: usize = 0;
    var plain_buf: [noise.max_record_plaintext]u8 = undefined;
    while (wire_pos < sealed_stream.items.len) {
        const rec_len = std.mem.readInt(u16, sealed_stream.items[wire_pos..][0..2], .big);
        wire_pos += 2;
        const plain = try hub_sess.open_record(sealed_stream.items[wire_pos .. wire_pos + rec_len], &plain_buf);
        wire_pos += rec_len;

        // 1-byte chunking feed into pt-frame Decoder.
        for (plain) |b| {
            try dec.feed(gpa, &.{b});
            while (try dec.next_frame(gpa)) |f| {
                var fm = f;
                defer fm.deinit(gpa);

                var arena_st = std.heap.ArenaAllocator.init(gpa);
                defer arena_st.deinit();
                const parsed = try msg.decode_arena(msg.Request, arena_st.allocator(), fm.payload);
                if (decoded_count == 0) {
                    tiger.assert(fm.id == 42 and fm.kind == .request);
                    tiger.assert(std.mem.eql(u8, parsed.submit_job.job_id, "job-dst-1"));
                } else {
                    tiger.assert(fm.id == 43 and fm.kind == .request);
                    tiger.assert(parsed.ping.now_ms == 555);
                }
                decoded_count += 1;
            }
        }
    }
    try dec.finish();
    tiger.assert(decoded_count == 2);

    // Invalid pt-frame headers must fail closed.
    var bad_dec = frame.Decoder{};
    defer bad_dec.deinit(gpa);
    var bad_hdr: [13]u8 = @splat(0);
    std.mem.writeInt(u32, bad_hdr[0..4], 3, .little); // < head_bytes (9)
    try bad_dec.feed(gpa, &bad_hdr);
    if (bad_dec.next_frame(gpa)) |_| {
        tiger.assert(false);
    } else |err| {
        tiger.assert(err == error.TooShort);
    }
}
