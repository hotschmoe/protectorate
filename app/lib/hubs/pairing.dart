import 'package:flutter/foundation.dart';

import 'hub_profile.dart';

@immutable
class PairingRequest {
  const PairingRequest({
    required this.baseUrl,
    this.token = '',
    this.tab,
    this.session,
  });

  final String baseUrl;
  // Empty when the link only opens a hub this app already holds a token for.
  final String token;
  final String? tab;
  final String? session;
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
  return PairingRequest(
    baseUrl: base,
    token: (q['token'] ?? '').trim(),
    tab: q['tab'],
    session: q['session'],
  );
}

/// Parses the web link `<base>/#/pair?url=..&token=..` and the app link
/// `protectorate://pair?url=..&token=..`.
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
  return _fromQuery(q, q['url'] ?? uri.origin);
}

/// The same links `protectorate devices add --url` prints.
({String web, String app}) pairLinks(String baseUrl, String token) {
  final base = _trimSlashes(baseUrl);
  final query =
      'url=${Uri.encodeComponent(base)}&token=${Uri.encodeComponent(token)}';
  return (web: '$base/#/pair?$query', app: 'protectorate://pair?$query');
}
