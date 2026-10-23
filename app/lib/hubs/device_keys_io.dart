import 'dart:io';

import 'package:flutter/services.dart';
import 'package:shared_preferences/shared_preferences.dart';

import 'device_key.dart';
import 'software_key.dart';

// Android keeps the private key in the Keystore, where it cannot be
// exported. Desktop builds have no such store the app can rely on, so their
// key is kept with the app's preferences, like any other local secret.
const _keystore = MethodChannel('protectorate/device_key');

String _alias(String profile) => 'protectorate-hub-$profile';

class _KeystoreKey implements DeviceKey {
  _KeystoreKey(this._alias, Uint8List encoded)
    // The Keystore hands out X.509 SubjectPublicKeyInfo; the point is its tail.
    : publicKey = Uint8List.sublistView(encoded, encoded.length - 65);

  final String _alias;

  @override
  final Uint8List publicKey;

  @override
  Future<Uint8List> sign(Uint8List message) async => (await _keystore
      .invokeMethod<Uint8List>('sign', {'alias': _alias, 'message': message}))!;
}

Future<DeviceKey?> load(String profile) async {
  if (Platform.isAndroid) {
    final encoded = await _keystore.invokeMethod<Uint8List>(
      'publicKey',
      _alias(profile),
    );
    return encoded == null ? null : _KeystoreKey(_alias(profile), encoded);
  }
  final stored = (await SharedPreferences.getInstance()).getString(
    _alias(profile),
  );
  if (stored == null || stored.length != 64) return null;
  return SoftwareKey.fromBytes([
    for (var i = 0; i < 64; i += 2)
      int.parse(stored.substring(i, i + 2), radix: 16),
  ]);
}

Future<DeviceKey> create(String profile) async {
  if (Platform.isAndroid) {
    final encoded = await _keystore.invokeMethod<Uint8List>(
      'create',
      _alias(profile),
    );
    return _KeystoreKey(_alias(profile), encoded!);
  }
  final key = SoftwareKey.generate();
  final saved = await (await SharedPreferences.getInstance()).setString(
    _alias(profile),
    key.privateBytes.map((b) => b.toRadixString(16).padLeft(2, '0')).join(),
  );
  if (!saved) throw StateError('Could not save the device key');
  return key;
}

Future<void> forget(String profile) async {
  if (Platform.isAndroid) {
    await _keystore.invokeMethod<void>('delete', _alias(profile));
    return;
  }
  await (await SharedPreferences.getInstance()).remove(_alias(profile));
}
