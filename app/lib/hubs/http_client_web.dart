import 'package:connectrpc/connect.dart';
import 'package:connectrpc/web.dart' as impl;
import 'package:web_socket_channel/web_socket_channel.dart';

HttpClient createPlatformHttpClient() => impl.createHttpClient();

// A browser cannot set WebSocket headers, so the token rides along as a
// subprotocol the hub reads and never selects.
WebSocketChannel connectWebSocket(
  Uri uri,
  Iterable<String> protocols,
  String token,
) => WebSocketChannel.connect(uri, protocols: [...protocols, 'bearer.$token']);
