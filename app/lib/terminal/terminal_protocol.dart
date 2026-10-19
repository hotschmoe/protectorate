import 'dart:convert';
import 'dart:typed_data';

const msgData = 0x30;
const msgResize = 0x31;

const terminalSubprotocol = 'protectorate.terminal.v1';

List<String> terminalSubprotocols(String token) =>
    [terminalSubprotocol, if (token.isNotEmpty) 'token.$token'];

Uint8List encodeData(String text) {
  final body = utf8.encode(text);
  return Uint8List(body.length + 1)
    ..[0] = msgData
    ..setRange(1, body.length + 1, body);
}

Uint8List encodeResize(int cols, int rows) {
  final body = utf8.encode(jsonEncode({'cols': cols, 'rows': rows}));
  return Uint8List(body.length + 1)
    ..[0] = msgResize
    ..setRange(1, body.length + 1, body);
}

Uint8List? decodeData(Object? frame) {
  if (frame is! List<int> || frame.isEmpty || frame[0] != msgData) return null;
  return Uint8List.fromList(frame.sublist(1));
}
