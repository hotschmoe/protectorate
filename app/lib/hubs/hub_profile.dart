import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:shared_preferences/shared_preferences.dart';

@immutable
class HubProfile {
  const HubProfile({
    required this.id,
    required this.name,
    required this.baseUrl,
    required this.token,
  });

  final String id;
  final String name;
  final String baseUrl;
  // The bearer token for every call, normally a device token minted by
  // DeviceService.AddDevice.
  final String token;

  Uri get baseUri => Uri.parse(baseUrl);

  Uri terminalUri(String session, {bool readOnly = false}) {
    final base = baseUri;
    return base.replace(
      scheme: base.scheme == 'https' ? 'wss' : 'ws',
      pathSegments: [
        ...base.pathSegments.where((s) => s.isNotEmpty),
        'ws',
        'terminal',
        session,
      ],
      queryParameters: readOnly ? {'readonly': '1'} : null,
    );
  }

  Map<String, Object?> toJson() => {
    'id': id,
    'name': name,
    'baseUrl': baseUrl,
    'token': token,
  };

  factory HubProfile.fromJson(Map<String, Object?> json) => HubProfile(
    id: json['id'] as String,
    name: json['name'] as String,
    baseUrl: json['baseUrl'] as String,
    token: json['token'] as String,
  );

  @override
  bool operator ==(Object other) =>
      other is HubProfile &&
      other.id == id &&
      other.name == name &&
      other.baseUrl == baseUrl &&
      other.token == token;

  @override
  int get hashCode => Object.hash(id, name, baseUrl, token);
}

String? validateBaseUrl(String value) {
  final uri = Uri.tryParse(value.trim());
  if (uri == null ||
      !(uri.scheme == 'http' || uri.scheme == 'https') ||
      uri.host.isEmpty) {
    return 'Enter an http:// or https:// URL';
  }
  if (uri.userInfo.isNotEmpty || uri.hasQuery || uri.hasFragment) {
    return 'Use a base URL without credentials, a query or a fragment';
  }
  return null;
}

class HubStore extends ChangeNotifier {
  HubStore(this._prefs) : _hubs = _load(_prefs);

  static const _key = 'hubs';

  final SharedPreferences _prefs;
  List<HubProfile> _hubs;

  List<HubProfile> get hubs => List.unmodifiable(_hubs);

  static List<HubProfile> _load(SharedPreferences prefs) {
    final raw = prefs.getString(_key);
    if (raw == null) return [];
    try {
      return [
        for (final e in jsonDecode(raw) as List<dynamic>)
          HubProfile.fromJson((e as Map).cast<String, Object?>()),
      ];
    } on Object {
      return [];
    }
  }

  Future<void> upsert(HubProfile hub) async {
    final i = _hubs.indexWhere((h) => h.id == hub.id);
    _hubs = [..._hubs];
    if (i >= 0) {
      _hubs[i] = hub;
    } else {
      _hubs.add(hub);
    }
    await _save();
  }

  Future<void> remove(String id) async {
    _hubs = _hubs.where((h) => h.id != id).toList();
    await _save();
  }

  Future<void> _save() async {
    final saved = await _prefs.setString(
      _key,
      jsonEncode([for (final h in _hubs) h.toJson()]),
    );
    if (!saved) throw StateError('Could not save hub connection');
    notifyListeners();
  }
}
