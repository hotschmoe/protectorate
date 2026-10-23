import 'dart:math';
import 'dart:typed_data';

import 'package:crypto/crypto.dart';

import 'device_key.dart';

/// A P-256 key held in memory, for platforms without a key store the app can
/// use (desktop builds and the Dart scenario harness). Signatures use RFC
/// 6979 nonces, so signing needs no randomness. Not constant time: the key
/// is only as safe as the storage it is kept in.
class SoftwareKey implements DeviceKey {
  SoftwareKey(this._d) : publicKey = _encode(_multiply(_g, _d));

  factory SoftwareKey.generate() {
    final random = Random.secure();
    while (true) {
      final d = _int(List<int>.generate(32, (_) => random.nextInt(256)));
      if (d > BigInt.zero && d < _n) return SoftwareKey(d);
    }
  }

  /// The private scalar as 32 big-endian bytes, for storing.
  factory SoftwareKey.fromBytes(List<int> bytes) => SoftwareKey(_int(bytes));

  final BigInt _d;

  @override
  final Uint8List publicKey;

  Uint8List get privateBytes => Uint8List.fromList(_bytes(_d));

  @override
  Future<Uint8List> sign(Uint8List message) async {
    final hash = sha256.convert(message).bytes;
    final e = _int(hash) % _n;
    for (final k in _nonces(hash)) {
      final r = _multiply(_g, k).$1 % _n;
      if (r == BigInt.zero) continue;
      final s = (k.modInverse(_n) * (e + r * _d)) % _n;
      if (s == BigInt.zero) continue;
      return Uint8List.fromList([..._bytes(r), ..._bytes(s)]);
    }
    throw StateError('RFC 6979 nonces are endless');
  }

  // RFC 6979 section 3.2 with HMAC-SHA256 and a 256-bit order.
  Iterable<BigInt> _nonces(List<int> hash) sync* {
    final x = _bytes(_d);
    final h = _bytes(_int(hash) % _n);
    var v = List<int>.filled(32, 1);
    var k = List<int>.filled(32, 0);
    List<int> mac(List<int> key, List<int> data) =>
        Hmac(sha256, key).convert(data).bytes;
    k = mac(k, [...v, 0, ...x, ...h]);
    v = mac(k, v);
    k = mac(k, [...v, 1, ...x, ...h]);
    v = mac(k, v);
    while (true) {
      v = mac(k, v);
      final candidate = _int(v);
      if (candidate > BigInt.zero && candidate < _n) yield candidate;
      k = mac(k, [...v, 0]);
      v = mac(k, v);
    }
  }
}

final _p = BigInt.parse(
  'ffffffff00000001000000000000000000000000ffffffffffffffffffffffff',
  radix: 16,
);
final _n = BigInt.parse(
  'ffffffff00000000ffffffffffffffffbce6faada7179e84f3b9cac2fc632551',
  radix: 16,
);
final (BigInt, BigInt) _g = (
  BigInt.parse(
    '6b17d1f2e12c4247f8bce6e563a440f277037d812deb33a0f4a13945d898c296',
    radix: 16,
  ),
  BigInt.parse(
    '4fe342e2fe1a7f9b8ee7eb4a7c0f9e162bce33576b315ececbb6406837bf51f5',
    radix: 16,
  ),
);

BigInt _int(List<int> bytes) =>
    bytes.fold(BigInt.zero, (value, b) => (value << 8) | BigInt.from(b));

List<int> _bytes(BigInt value) => [
  for (var i = 31; i >= 0; i--) ((value >> (8 * i)) & BigInt.from(255)).toInt(),
];

Uint8List _encode((BigInt, BigInt) point) =>
    Uint8List.fromList([4, ..._bytes(point.$1), ..._bytes(point.$2)]);

// Jacobian coordinates (X, Y, Z) for x = X/Z^2, y = Y/Z^3; Z = 0 is infinity.
typedef _Jacobian = (BigInt, BigInt, BigInt);

_Jacobian _double(_Jacobian a) {
  final (x, y, z) = a;
  if (z == BigInt.zero || y == BigInt.zero) {
    return (BigInt.one, BigInt.one, BigInt.zero);
  }
  final yy = y * y % _p;
  final s = BigInt.from(4) * x * yy % _p;
  final zz = z * z % _p;
  // a = -3: M = 3 (X - Z^2)(X + Z^2).
  final m = BigInt.from(3) * (x - zz) * (x + zz) % _p;
  final x3 = (m * m - BigInt.two * s) % _p;
  final y3 = (m * (s - x3) - BigInt.from(8) * yy * yy) % _p;
  final z3 = BigInt.two * y * z % _p;
  return (x3, y3, z3);
}

_Jacobian _add(_Jacobian a, _Jacobian b) {
  final (x1, y1, z1) = a;
  final (x2, y2, z2) = b;
  if (z1 == BigInt.zero) return b;
  if (z2 == BigInt.zero) return a;
  final z1z1 = z1 * z1 % _p;
  final z2z2 = z2 * z2 % _p;
  final u1 = x1 * z2z2 % _p;
  final u2 = x2 * z1z1 % _p;
  final s1 = y1 * z2 * z2z2 % _p;
  final s2 = y2 * z1 * z1z1 % _p;
  if (u1 == u2) {
    return s1 == s2 ? _double(a) : (BigInt.one, BigInt.one, BigInt.zero);
  }
  final h = (u2 - u1) % _p;
  final r = (s2 - s1) % _p;
  final hh = h * h % _p;
  final hhh = h * hh % _p;
  final v = u1 * hh % _p;
  final x3 = (r * r - hhh - BigInt.two * v) % _p;
  final y3 = (r * (v - x3) - s1 * hhh) % _p;
  final z3 = h * z1 * z2 % _p;
  return (x3, y3, z3);
}

(BigInt, BigInt) _multiply((BigInt, BigInt) point, BigInt scalar) {
  _Jacobian result = (BigInt.one, BigInt.one, BigInt.zero);
  final _Jacobian base = (point.$1, point.$2, BigInt.one);
  for (var i = scalar.bitLength - 1; i >= 0; i--) {
    result = _double(result);
    if ((scalar >> i) & BigInt.one == BigInt.one) result = _add(result, base);
  }
  final (x, y, z) = result;
  final zi = z.modInverse(_p);
  final zi2 = zi * zi % _p;
  return (x * zi2 % _p, y * zi2 * zi % _p);
}
