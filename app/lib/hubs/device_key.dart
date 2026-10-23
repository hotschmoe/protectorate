import 'dart:convert';
import 'dart:math';
import 'dart:typed_data';

import 'package:connectrpc/connect.dart';
import 'package:crypto/crypto.dart';

/// A device's P-256 key. The hub stores the public half when it approves the
/// device and verifies a signature on every request; the private half never
/// leaves the platform key store.
abstract interface class DeviceKey {
  /// The uncompressed public point, 65 bytes.
  Uint8List get publicKey;

  /// An ECDSA P-256 SHA-256 signature, raw r||s or DER.
  Future<Uint8List> sign(Uint8List message);
}

/// The device ID the hub derives from a public key.
String deviceId(Uint8List publicKey) => sha256
    .convert(publicKey)
    .bytes
    .take(8)
    .map((b) => b.toRadixString(16).padLeft(2, '0'))
    .join();

final _random = Random.secure();

String _base64Url(List<int> bytes) =>
    base64Url.encode(bytes).replaceAll('=', '');

/// A fresh proof for one request: `<device>.<seconds>.<nonce>.<signature>`
/// over `<METHOD>\n<path>\n<seconds>\n<nonce>`.
Future<String> deviceProof(DeviceKey key, String method, String path) async {
  final time = DateTime.now().millisecondsSinceEpoch ~/ 1000;
  final nonce = _base64Url(List<int>.generate(16, (_) => _random.nextInt(256)));
  final signature = await key.sign(
    Uint8List.fromList(utf8.encode('$method\n$path\n$time\n$nonce')),
  );
  return '${deviceId(key.publicKey)}.$time.$nonce.${_base64Url(signature)}';
}

/// Signs every call with the device key, once the hub has approved it.
Interceptor proofInterceptor(Future<DeviceKey?> key) {
  return <I extends Object, O extends Object>(AnyFn<I, O> next) =>
      (Request<I, O> req) async {
        final device = await key;
        if (device != null) {
          final path = Uri.parse(req.url).path;
          req.headers.set('authorization', [
            'Proof ${await deviceProof(device, 'POST', path)}',
          ]);
        }
        return next(req);
      };
}
