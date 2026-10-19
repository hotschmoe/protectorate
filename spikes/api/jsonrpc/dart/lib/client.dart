import 'dart:async';
import 'dart:convert';

import 'package:http/http.dart' as http;

class RpcException implements Exception {
  RpcException(this.code, this.message);
  final int code;
  final String message;
  @override
  String toString() => 'RpcException($code): $message';
}

class Sleeve {
  Sleeve({required this.id, required this.name, required this.harness, required this.state});
  factory Sleeve.fromJson(Map<String, dynamic> j) => Sleeve(
        id: j['id'] as String,
        name: j['name'] as String,
        harness: j['harness'] as String,
        state: j['state'] as String,
      );
  final String id, name, harness, state;
}

class Event {
  Event({required this.seq, required this.type, required this.sleeveId, required this.message});
  factory Event.fromJson(Map<String, dynamic> j) => Event(
        seq: (j['seq'] as num?)?.toInt() ?? 0,
        type: j['type'] as String,
        sleeveId: (j['sleeve_id'] as String?) ?? '',
        message: (j['message'] as String?) ?? '',
      );
  final int seq;
  final String type, sleeveId, message;
}

class Client {
  Client(this.base, [http.Client? c]) : _http = c ?? http.Client();
  final String base;
  final http.Client _http;
  int _id = 0;

  Future<Map<String, dynamic>> _call(String method, Map<String, dynamic> params) async {
    final res = await _http.post(
      Uri.parse('$base/rpc'),
      headers: {'Content-Type': 'application/json'},
      body: jsonEncode({'jsonrpc': '2.0', 'id': ++_id, 'method': method, 'params': params}),
    );
    final body = jsonDecode(res.body) as Map<String, dynamic>;
    final err = body['error'] as Map<String, dynamic>?;
    if (err != null) throw RpcException(err['code'] as int, err['message'] as String);
    return body['result'] as Map<String, dynamic>;
  }

  Future<List<Sleeve>> list() async {
    final r = await _call('sleeve.list', {});
    return [for (final s in (r['sleeves'] as List? ?? [])) Sleeve.fromJson(s as Map<String, dynamic>)];
  }

  Future<Sleeve> spawn(String name, {String harness = ''}) async {
    final r = await _call('sleeve.spawn', {'name': name, if (harness.isNotEmpty) 'harness': harness});
    return Sleeve.fromJson(r['sleeve'] as Map<String, dynamic>);
  }

  Stream<Event> subscribe({String sleeveId = ''}) async* {
    final req = http.Request('GET', Uri.parse('$base/events').replace(queryParameters: {if (sleeveId.isNotEmpty) 'sleeve_id': sleeveId}));
    final res = await _http.send(req);
    await for (final line in res.stream.transform(utf8.decoder).transform(const LineSplitter())) {
      if (line.isEmpty) continue;
      yield Event.fromJson(jsonDecode(line) as Map<String, dynamic>);
    }
  }
}
