import 'dart:io' as io;

import 'package:connectrpc/connect.dart';
import 'package:connectrpc/io.dart' as impl;
import 'package:web_socket_channel/io.dart';
import 'package:web_socket_channel/web_socket_channel.dart';

io.HttpClient _client() =>
    io.HttpClient()..connectionTimeout = const Duration(seconds: 8);

HttpClient createPlatformHttpClient() => impl.createHttpClient(_client());

WebSocketChannel connectWebSocket(
  Uri uri,
  Iterable<String> protocols,
  String token,
) => IOWebSocketChannel.connect(
  uri,
  protocols: protocols,
  headers: {'authorization': 'Bearer $token'},
  customClient: _client(),
);
