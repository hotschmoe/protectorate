import 'dart:async';
import 'dart:js_interop';
import 'dart:typed_data';

import 'package:web/web.dart' as web;

import 'device_key.dart';

// The browser keeps a non-extractable WebCrypto key; IndexedDB stores the key
// object itself, so script can sign with it but never read it out.
const _database = 'protectorate';
const _store = 'device-keys';

extension type _KeyPair(JSObject _) implements JSObject {
  external web.CryptoKey get publicKey;
  external web.CryptoKey get privateKey;
}

class _WebCryptoKey implements DeviceKey {
  _WebCryptoKey(this._private, this.publicKey);

  final web.CryptoKey _private;

  @override
  final Uint8List publicKey;

  @override
  Future<Uint8List> sign(Uint8List message) async {
    final signature = await web.window.crypto.subtle
        .sign(
          {'name': 'ECDSA', 'hash': 'SHA-256'}.jsify()!,
          _private,
          message.toJS,
        )
        .toDart;
    return (signature as JSArrayBuffer).toDart.asUint8List();
  }
}

Future<T?> _request<T extends JSAny?>(web.IDBRequest request) {
  final done = Completer<T?>();
  request.onsuccess = ((web.Event _) => done.complete(
    request.result as T?,
  )).toJS;
  request.onerror = ((web.Event _) => done.completeError(
    StateError('IndexedDB: ${request.error?.message}'),
  )).toJS;
  return done.future;
}

Future<web.IDBObjectStore> _open(String mode) async {
  final request = web.window.indexedDB.open(_database, 1);
  request.onupgradeneeded = ((web.Event _) {
    (request.result as web.IDBDatabase).createObjectStore(_store);
  }).toJS;
  final db = (await _request<web.IDBDatabase>(request))!;
  return db.transaction(_store.toJS, mode).objectStore(_store);
}

Future<DeviceKey> _fromPair(_KeyPair pair) async {
  final raw = await web.window.crypto.subtle
      .exportKey('raw', pair.publicKey)
      .toDart;
  return _WebCryptoKey(
    pair.privateKey,
    (raw as JSArrayBuffer).toDart.asUint8List(),
  );
}

Future<DeviceKey?> load(String profile) async {
  final pair = await _request<JSAny?>(
    (await _open('readonly')).get(profile.toJS),
  );
  return pair == null ? null : _fromPair(pair as _KeyPair);
}

Future<DeviceKey> create(String profile) async {
  final pair = await web.window.crypto.subtle
      .generateKey(
        {'name': 'ECDSA', 'namedCurve': 'P-256'}.jsify()!,
        false,
        ['sign'.toJS].toJS,
      )
      .toDart;
  await _request<JSAny?>((await _open('readwrite')).put(pair, profile.toJS));
  return _fromPair(pair as _KeyPair);
}

Future<void> forget(String profile) async {
  await _request<JSAny?>((await _open('readwrite')).delete(profile.toJS));
}
