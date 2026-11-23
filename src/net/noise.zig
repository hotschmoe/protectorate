const std = @import("std");
const tiger = @import("../tiger.zig");

const X25519 = std.crypto.dh.X25519;
const ChaCha20Poly1305 = std.crypto.aead.chacha_poly.ChaCha20Poly1305;
const Blake2s256 = std.crypto.hash.blake2.Blake2s256;
const Sha256 = std.crypto.hash.sha2.Sha256;

pub const protocol_name = "Noise_IK_25519_ChaChaPoly_BLAKE2s";
pub const prologue = "protectorate-fleet-v1";
pub const max_record_plaintext: usize = 32 * 1024;
pub const tag_len: usize = 16;
pub const max_record_wire: usize = 2 + max_record_plaintext + tag_len;
pub const max_session_bytes: u64 = 1 << 30;
pub const max_session_ms: u64 = 24 * 60 * 60 * 1000;
pub const msg1_len: usize = 32 + 32 + tag_len + tag_len; // e(32) + enc(s:32+16) + enc(empty:16) = 96
pub const msg2_len: usize = 32 + tag_len; // e(32) + enc(empty:16) = 48

pub const KeyPair = struct {
    public: [32]u8,
    secret: [32]u8,

    pub fn generate(io: std.Io) KeyPair {
        const kp = X25519.KeyPair.generate(io);
        return .{ .public = kp.public_key, .secret = kp.secret_key };
    }

    pub fn generate_deterministic(seed: [32]u8) KeyPair {
        const kp = X25519.KeyPair.generateDeterministic(seed);
        return .{ .public = kp.public_key, .secret = kp.secret_key };
    }

    pub fn from_secret(secret: [32]u8) error{IdentityElement}!KeyPair {
        const pub_key = X25519.recoverPublicKey(secret);
        return .{ .public = pub_key, .secret = secret };
    }

    pub fn zeroize(self: *KeyPair) void {
        std.crypto.secureZero(u8, &self.secret);
    }
};

pub fn fingerprint_hex(pub_key: [32]u8) [64]u8 {
    var digest: [32]u8 = undefined;
    Sha256.hash(&pub_key, &digest, .{});
    const chars = "0123456789abcdef";
    var out: [64]u8 = undefined;
    for (digest, 0..) |b, i| {
        out[i * 2] = chars[b >> 4];
        out[i * 2 + 1] = chars[b & 0x0f];
    }
    return out;
}

pub fn fingerprint_prefixed(pub_key: [32]u8) [7 + 64]u8 {
    var out: [71]u8 = undefined;
    @memcpy(out[0..7], "sha256:");
    const hex = fingerprint_hex(pub_key);
    @memcpy(out[7..71], &hex);
    return out;
}

pub const NoiseError = error{
    IdentityElement,
    AuthenticationFailed,
    BadLength,
    UnknownPeer,
    NonceExhausted,
    SessionExpired,
};

pub const CipherState = struct {
    k: [32]u8,
    n: u64 = 0,
    has_key: bool = false,

    fn make_nonce(n: u64) [12]u8 {
        var nonce: [12]u8 = @splat(0);
        std.mem.writeInt(u64, nonce[4..12], n, .little);
        return nonce;
    }

    pub fn encrypt_with_ad(
        self: *CipherState,
        ad: []const u8,
        plaintext: []const u8,
        out: []u8,
    ) NoiseError![]u8 {
        tiger.assert(out.len >= plaintext.len + if (self.has_key) tag_len else 0);
        if (!self.has_key) {
            @memcpy(out[0..plaintext.len], plaintext);
            return out[0..plaintext.len];
        }
        if (self.n == std.math.maxInt(u64)) return error.NonceExhausted;
        const nonce = make_nonce(self.n);
        var tag: [tag_len]u8 = undefined;
        ChaCha20Poly1305.encrypt(out[0..plaintext.len], &tag, plaintext, ad, nonce, self.k);
        @memcpy(out[plaintext.len .. plaintext.len + tag_len], &tag);
        self.n += 1;
        return out[0 .. plaintext.len + tag_len];
    }

    pub fn decrypt_with_ad(
        self: *CipherState,
        ad: []const u8,
        ciphertext: []const u8,
        out: []u8,
    ) NoiseError![]u8 {
        if (!self.has_key) {
            tiger.assert(out.len >= ciphertext.len);
            @memcpy(out[0..ciphertext.len], ciphertext);
            return out[0..ciphertext.len];
        }
        if (ciphertext.len < tag_len) return error.AuthenticationFailed;
        if (self.n == std.math.maxInt(u64)) return error.NonceExhausted;
        const ct_len = ciphertext.len - tag_len;
        tiger.assert(out.len >= ct_len);
        const nonce = make_nonce(self.n);
        var tag: [tag_len]u8 = undefined;
        @memcpy(&tag, ciphertext[ct_len..]);
        ChaCha20Poly1305.decrypt(out[0..ct_len], ciphertext[0..ct_len], tag, ad, nonce, self.k) catch {
            return error.AuthenticationFailed;
        };
        self.n += 1;
        return out[0..ct_len];
    }

    pub fn zeroize(self: *CipherState) void {
        std.crypto.secureZero(u8, &self.k);
        self.has_key = false;
    }
};

fn blake2s_hmac(key: []const u8, data1: []const u8, data2: []const u8, out: *[32]u8) void {
    var ipad: [64]u8 = @splat(0x36);
    var opad: [64]u8 = @splat(0x5c);
    var key_block: [64]u8 = @splat(0);
    if (key.len > 64) {
        var hashed: [32]u8 = undefined;
        Blake2s256.hash(key, &hashed, .{});
        @memcpy(key_block[0..32], &hashed);
        std.crypto.secureZero(u8, &hashed);
    } else {
        @memcpy(key_block[0..key.len], key);
    }
    for (0..64) |i| {
        ipad[i] ^= key_block[i];
        opad[i] ^= key_block[i];
    }
    var inner_hash: [32]u8 = undefined;
    var h_in = Blake2s256.init(.{});
    h_in.update(&ipad);
    h_in.update(data1);
    h_in.update(data2);
    h_in.final(&inner_hash);

    var h_out = Blake2s256.init(.{});
    h_out.update(&opad);
    h_out.update(&inner_hash);
    h_out.final(out);

    std.crypto.secureZero(u8, &key_block);
    std.crypto.secureZero(u8, &ipad);
    std.crypto.secureZero(u8, &opad);
    std.crypto.secureZero(u8, &inner_hash);
}

fn hkdf2(ck: [32]u8, ikm: []const u8, out1: *[32]u8, out2: *[32]u8) void {
    var temp_key: [32]u8 = undefined;
    blake2s_hmac(&ck, ikm, &.{}, &temp_key);
    blake2s_hmac(&temp_key, &.{}, &[_]u8{0x01}, out1);
    blake2s_hmac(&temp_key, out1, &[_]u8{0x02}, out2);
    std.crypto.secureZero(u8, &temp_key);
}

const SymmetricState = struct {
    h: [32]u8,
    ck: [32]u8,
    cs: CipherState = .{ .k = @splat(0), .has_key = false },

    fn init(proto: []const u8) SymmetricState {
        var h: [32]u8 = @splat(0);
        if (proto.len <= 32) {
            @memcpy(h[0..proto.len], proto);
        } else {
            Blake2s256.hash(proto, &h, .{});
        }
        return .{ .h = h, .ck = h };
    }

    fn mix_hash(self: *SymmetricState, data: []const u8) void {
        var ctx = Blake2s256.init(.{});
        ctx.update(&self.h);
        ctx.update(data);
        ctx.final(&self.h);
    }

    fn mix_key(self: *SymmetricState, ikm: []const u8) void {
        var new_ck: [32]u8 = undefined;
        var temp_k: [32]u8 = undefined;
        hkdf2(self.ck, ikm, &new_ck, &temp_k);
        self.ck = new_ck;
        self.cs = .{ .k = temp_k, .n = 0, .has_key = true };
        std.crypto.secureZero(u8, &new_ck);
        std.crypto.secureZero(u8, &temp_k);
    }

    fn encrypt_and_hash(self: *SymmetricState, plaintext: []const u8, out: []u8) NoiseError![]u8 {
        const res = try self.cs.encrypt_with_ad(&self.h, plaintext, out);
        self.mix_hash(res);
        return res;
    }

    fn decrypt_and_hash(self: *SymmetricState, ciphertext: []const u8, out: []u8) NoiseError![]u8 {
        const res = try self.cs.decrypt_with_ad(&self.h, ciphertext, out);
        self.mix_hash(ciphertext);
        return res;
    }

    const SplitPair = struct { c1: CipherState, c2: CipherState };

    fn split(self: *SymmetricState) SplitPair {
        var k1: [32]u8 = undefined;
        var k2: [32]u8 = undefined;
        hkdf2(self.ck, &.{}, &k1, &k2);
        const res: SplitPair = .{
            .c1 = CipherState{ .k = k1, .n = 0, .has_key = true },
            .c2 = CipherState{ .k = k2, .n = 0, .has_key = true },
        };
        std.crypto.secureZero(u8, &k1);
        std.crypto.secureZero(u8, &k2);
        return res;
    }

    fn zeroize(self: *SymmetricState) void {
        std.crypto.secureZero(u8, &self.ck);
        self.cs.zeroize();
    }
};

pub const Session = struct {
    tx: CipherState,
    rx: CipherState,
    handshake_hash: [32]u8,
    peer_static: [32]u8,
    tx_bytes: u64 = 0,
    rx_bytes: u64 = 0,
    started_ms: u64 = 0,

    pub fn deinit(self: *Session) void {
        self.tx.zeroize();
        self.rx.zeroize();
    }

    pub fn expired(self: *const Session, now_ms: u64) bool {
        if (self.tx_bytes >= max_session_bytes or self.rx_bytes >= max_session_bytes) return true;
        if (now_ms >= self.started_ms and now_ms - self.started_ms >= max_session_ms) return true;
        return false;
    }

    pub fn seal_record(self: *Session, plaintext: []const u8, out: []u8) NoiseError![]u8 {
        if (plaintext.len == 0 or plaintext.len > max_record_plaintext) return error.BadLength;
        tiger.assert(out.len >= 2 + plaintext.len + tag_len);
        const cipher_slice = try self.tx.encrypt_with_ad(&.{}, plaintext, out[2..]);
        const wire_len: u16 = @intCast(cipher_slice.len);
        std.mem.writeInt(u16, out[0..2], wire_len, .big);
        self.tx_bytes += plaintext.len;
        return out[0 .. 2 + cipher_slice.len];
    }

    pub fn open_record(self: *Session, cipher_payload: []const u8, out: []u8) NoiseError![]u8 {
        if (cipher_payload.len <= tag_len or cipher_payload.len > max_record_plaintext + tag_len) {
            return error.BadLength;
        }
        const plain = try self.rx.decrypt_with_ad(&.{}, cipher_payload, out);
        self.rx_bytes += plain.len;
        return plain;
    }
};

pub const InitiatorHandshake = struct {
    ss: SymmetricState,
    s: KeyPair,
    e: KeyPair,
    rs: [32]u8,

    pub fn start(
        s: KeyPair,
        e: KeyPair,
        rs: [32]u8,
        out_msg1: *[msg1_len]u8,
    ) NoiseError!InitiatorHandshake {
        var ss = SymmetricState.init(protocol_name);
        errdefer ss.zeroize();
        ss.mix_hash(prologue);
        ss.mix_hash(&rs);

        @memcpy(out_msg1[0..32], &e.public);
        ss.mix_hash(&e.public);

        const es = X25519.scalarmult(e.secret, rs) catch return error.IdentityElement;
        ss.mix_key(&es);

        const enc_s = try ss.encrypt_and_hash(&s.public, out_msg1[32..80]);
        tiger.assert(enc_s.len == 48);

        const ss_dh = X25519.scalarmult(s.secret, rs) catch return error.IdentityElement;
        ss.mix_key(&ss_dh);

        const enc_p = try ss.encrypt_and_hash(&.{}, out_msg1[80..96]);
        tiger.assert(enc_p.len == 16);

        return .{ .ss = ss, .s = s, .e = e, .rs = rs };
    }

    pub fn finish(self: *InitiatorHandshake, msg2: []const u8, started_ms: u64) NoiseError!Session {
        defer {
            self.e.zeroize();
            self.ss.zeroize();
        }
        if (msg2.len != msg2_len) return error.BadLength;

        var re: [32]u8 = undefined;
        @memcpy(&re, msg2[0..32]);
        self.ss.mix_hash(&re);

        const ee = X25519.scalarmult(self.e.secret, re) catch return error.IdentityElement;
        self.ss.mix_key(&ee);

        const se = X25519.scalarmult(self.s.secret, re) catch return error.IdentityElement;
        self.ss.mix_key(&se);

        var dummy: [16]u8 = undefined;
        const p = try self.ss.decrypt_and_hash(msg2[32..48], &dummy);
        if (p.len != 0) return error.BadLength;

        const keys = self.ss.split();
        return .{
            .tx = keys.c1,
            .rx = keys.c2,
            .handshake_hash = self.ss.h,
            .peer_static = self.rs,
            .started_ms = started_ms,
        };
    }
};

pub fn responder_accept(
    s: KeyPair,
    e: KeyPair,
    expected_initiator_static: [32]u8,
    msg1: []const u8,
    out_msg2: *[msg2_len]u8,
    started_ms: u64,
) NoiseError!Session {
    const peers = [_][32]u8{expected_initiator_static};
    return responder_accept_any(s, e, &peers, msg1, out_msg2, started_ms);
}

pub fn responder_accept_any(
    s: KeyPair,
    e: KeyPair,
    allowed_peers: []const [32]u8,
    msg1: []const u8,
    out_msg2: *[msg2_len]u8,
    started_ms: u64,
) NoiseError!Session {
    var e_mut = e;
    defer e_mut.zeroize();

    if (msg1.len != msg1_len) return error.BadLength;

    var ss = SymmetricState.init(protocol_name);
    defer ss.zeroize();
    ss.mix_hash(prologue);
    ss.mix_hash(&s.public);

    var re: [32]u8 = undefined;
    @memcpy(&re, msg1[0..32]);
    ss.mix_hash(&re);

    const es = X25519.scalarmult(s.secret, re) catch return error.IdentityElement;
    ss.mix_key(&es);

    var rs: [32]u8 = undefined;
    const dec_s = try ss.decrypt_and_hash(msg1[32..80], &rs);
    if (dec_s.len != 32) return error.BadLength;

    const ss_dh = X25519.scalarmult(s.secret, rs) catch return error.IdentityElement;
    ss.mix_key(&ss_dh);

    var dummy: [16]u8 = undefined;
    const dec_p = try ss.decrypt_and_hash(msg1[80..96], &dummy);
    if (dec_p.len != 0) return error.BadLength;

    var matched = false;
    for (allowed_peers) |cand| {
        if (std.crypto.timing_safe.eql([32]u8, rs, cand)) {
            matched = true;
            break;
        }
    }
    if (!matched) {
        return error.UnknownPeer;
    }

    @memcpy(out_msg2[0..32], &e_mut.public);
    ss.mix_hash(&e_mut.public);

    const ee = X25519.scalarmult(e_mut.secret, re) catch return error.IdentityElement;
    ss.mix_key(&ee);

    const se = X25519.scalarmult(e_mut.secret, rs) catch return error.IdentityElement;
    ss.mix_key(&se);

    const enc_p = try ss.encrypt_and_hash(&.{}, out_msg2[32..48]);
    tiger.assert(enc_p.len == 16);

    const keys = ss.split();
    return .{
        .tx = keys.c2,
        .rx = keys.c1,
        .handshake_hash = ss.h,
        .peer_static = rs,
        .started_ms = started_ms,
    };
}
