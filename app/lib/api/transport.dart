// The two calls the generated `HubApi` needs, per platform: dart:io on desktop and Android, the
// browser's fetch and WebSocket through dart:js_interop on web.
//
//   Future<({int status, String body})> postJson(Uri url, String body, String? token)
//   Stream<String> openSocket(Uri url, String? token)
//
// `openSocket` connects on listen, emits text frames, ends when the socket closes and closes it
// on cancel. An http(s) URL becomes ws(s).
export 'transport_web.dart' if (dart.library.io) 'transport_io.dart';
