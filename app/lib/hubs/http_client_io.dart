import 'dart:convert';
import 'dart:io' as io;

import 'package:connectrpc/connect.dart';
import 'package:connectrpc/io.dart' as impl;
import 'package:web_socket_channel/io.dart';
import 'package:web_socket_channel/web_socket_channel.dart';

// A hub with a private CA is trusted through that CA only, with normal host
// name checks; without one the platform roots apply.
io.HttpClient _client(String caPem) => io.HttpClient(
  context: caPem.isEmpty
      ? null
      : (io.SecurityContext(withTrustedRoots: false)
          ..setTrustedCertificatesBytes(utf8.encode(caPem))),
)..connectionTimeout = const Duration(seconds: 8);

HttpClient createPlatformHttpClient({String caPem = ''}) =>
    impl.createHttpClient(_client(caPem));

WebSocketChannel connectWebSocket(
  Uri uri,
  Iterable<String> protocols, {
  String caPem = '',
}) => IOWebSocketChannel.connect(
  uri,
  protocols: protocols,
  customClient: _client(caPem),
);
