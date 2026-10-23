import 'package:connectrpc/connect.dart';
import 'package:connectrpc/web.dart' as impl;
import 'package:web_socket_channel/web_socket_channel.dart';

// The browser verifies the hub with its own trust store, so the hub CA must
// be installed there.
HttpClient createPlatformHttpClient({String caPem = ''}) =>
    impl.createHttpClient();

WebSocketChannel connectWebSocket(
  Uri uri,
  Iterable<String> protocols, {
  String caPem = '',
}) => WebSocketChannel.connect(uri, protocols: protocols);
