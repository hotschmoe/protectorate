import 'dart:js_interop';

@JS('localStorage')
external _Storage get _storage;

extension type _Storage._(JSObject _) implements JSObject {
  external JSString? getItem(JSString key);
  external void setItem(JSString key, JSString value);
  external void removeItem(JSString key);
}

@JS('history')
external _History get _history;

extension type _History._(JSObject _) implements JSObject {
  external void replaceState(JSAny? data, JSString unused, JSString url);
}

const _prefix = 'protectorate.';

Future<void> initSettings() async {}

/// Storage can be off (private windows, blocked site data); the app then forgets on reload.
String? loadSetting(String key) {
  try {
    return _storage.getItem('$_prefix$key'.toJS)?.toDart;
  } catch (_) {
    return null;
  }
}

void saveSetting(String key, String? value) {
  try {
    if (value == null || value.isEmpty) {
      _storage.removeItem('$_prefix$key'.toJS);
    } else {
      _storage.setItem('$_prefix$key'.toJS, value.toJS);
    }
  } catch (_) {}
}

String? pageOrigin() {
  final u = Uri.base;
  return u.scheme == 'http' || u.scheme == 'https' ? u.origin : null;
}

void dropQueryParam(String key) {
  final u = Uri.base;
  if (!u.queryParameters.containsKey(key)) return;
  final q = {...u.queryParameters}..remove(key);
  final query = q.isEmpty ? '' : '?${Uri(queryParameters: q).query}';
  final fragment = u.hasFragment ? '#${u.fragment}' : '';
  try {
    _history.replaceState(null, ''.toJS, '${u.path}$query$fragment'.toJS);
  } catch (_) {}
}
