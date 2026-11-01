import 'dart:async';
import 'dart:js_interop';

@JS('fetch')
external JSPromise<_Response> _fetch(JSString url, JSObject init);

extension type _Response._(JSObject _) implements JSObject {
  external int get status;
  external JSPromise<JSString> text();
}

@JS('WebSocket')
extension type _WebSocket._(JSObject _) implements JSObject {
  external factory _WebSocket(JSString url);
  external set onmessage(JSFunction? f);
  external set onerror(JSFunction? f);
  external set onclose(JSFunction? f);
  external void close();
}

extension type _MessageEvent._(JSObject _) implements JSObject {
  external JSAny? get data;
}

Future<({int status, String body})> postJson(Uri url, String body, String? token) async {
  final init = {
    'method': 'POST',
    'headers': {
      'Content-Type': 'application/json',
      if (token != null) 'Authorization': 'Bearer $token',
    },
    'body': body,
  }.jsify()! as JSObject;
  final resp = await _fetch(url.toString().toJS, init).toDart;
  final text = await resp.text().toDart;
  return (status: resp.status, body: text.toDart);
}

/// A browser cannot set headers on a WebSocket, so the token travels as `access_token`.
Stream<String> openSocket(Uri url, String? token) {
  _WebSocket? socket;
  late final StreamController<String> out;
  out = StreamController<String>(
    onListen: () {
      final u = url.replace(
        scheme: url.scheme == 'https' ? 'wss' : 'ws',
        queryParameters: {...url.queryParameters, 'access_token': ?token},
      );
      final ws = _WebSocket(u.toString().toJS);
      socket = ws;
      ws.onmessage = ((_MessageEvent e) {
        final d = e.data;
        if (d.isA<JSString>()) out.add((d as JSString).toDart);
      }).toJS;
      ws.onerror = ((JSObject _) => out.addError(StateError('websocket error at $url'))).toJS;
      ws.onclose = ((JSObject _) => unawaited(out.close())).toJS;
    },
    onCancel: () => socket?.close(),
  );
  return out.stream;
}
