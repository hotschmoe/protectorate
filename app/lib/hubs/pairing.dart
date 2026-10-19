import 'package:flutter/foundation.dart';

import 'hub_profile.dart';

@immutable
class PairingRequest {
  const PairingRequest({
    required this.baseUrl,
    required this.name,
    required this.token,
    this.tab,
    this.session,
    this.node,
    this.autoConfirm = false,
  });

  final String baseUrl;
  final String name;
  final String token;
  final String? tab;
  final String? session;
  final String? node;
  final bool autoConfirm;
}

String _trimSlashes(String url) {
  var u = url.trim();
  while (u.endsWith('/')) {
    u = u.substring(0, u.length - 1);
  }
  return u;
}

PairingRequest? _fromQuery(Map<String, String> q, String baseUrl) {
  final base = _trimSlashes(baseUrl);
  if (validateBaseUrl(base) != null) return null;
  final name = (q['name'] ?? '').trim();
  return PairingRequest(
    baseUrl: base,
    name: name.isEmpty ? Uri.parse(base).host : name,
    token: q['token'] ?? '',
    tab: q['tab'],
    session: q['session'],
    node: q['node'],
    autoConfirm: q['confirm'] == 'auto',
  );
}

/// Parses `<origin>/#/pair?name=..&token=..` (web) and
/// `protectorate://pair?url=..&name=..&token=..` (native) links.
PairingRequest? parsePairing(Uri uri) {
  if (uri.scheme == 'protectorate') {
    if (uri.host != 'pair') return null;
    final url = uri.queryParameters['url'];
    return url == null ? null : _fromQuery(uri.queryParameters, url);
  }
  if (uri.scheme != 'http' && uri.scheme != 'https') return null;
  var frag = uri.fragment;
  if (frag.startsWith('/')) frag = frag.substring(1);
  final cut = frag.indexOf('?');
  if (cut < 0 || frag.substring(0, cut) != 'pair') return null;
  final Map<String, String> q;
  try {
    q = Uri.splitQueryString(frag.substring(cut + 1));
  } on FormatException {
    return null;
  }
  return _fromQuery(q, uri.origin);
}
