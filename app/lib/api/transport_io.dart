import 'dart:async';
import 'dart:convert';
import 'dart:io';

Future<({int status, String body})> postJson(Uri url, String body, String? token) async {
  final client = HttpClient()..connectionTimeout = const Duration(seconds: 10);
  try {
    final req = await client.postUrl(url);
    req.headers.contentType = ContentType.json;
    if (token != null) req.headers.set(HttpHeaders.authorizationHeader, 'Bearer $token');
    req.add(utf8.encode(body));
    final resp = await req.close();
    final text = await resp.transform(utf8.decoder).join();
    return (status: resp.statusCode, body: text);
  } finally {
    client.close();
  }
}

Stream<String> openSocket(Uri url, String? token) {
  WebSocket? socket;
  var cancelled = false;
  late final StreamController<String> out;
  out = StreamController<String>(
    onListen: () async {
      try {
        final ws = await WebSocket.connect(
          url.replace(scheme: url.scheme == 'https' ? 'wss' : 'ws').toString(),
          headers: {if (token != null) HttpHeaders.authorizationHeader: 'Bearer $token'},
        );
        if (cancelled) {
          await ws.close();
          return;
        }
        socket = ws..pingInterval = const Duration(seconds: 20);
        ws.listen(
          (m) {
            if (m is String) out.add(m);
          },
          onError: out.addError,
          onDone: out.close,
          cancelOnError: true,
        );
      } catch (e, s) {
        out.addError(e, s);
        await out.close();
      }
    },
    onCancel: () async {
      cancelled = true;
      await socket?.close();
    },
  );
  return out.stream;
}
