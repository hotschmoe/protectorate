import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'api/api.g.dart';
import 'fake_hub.dart';
import 'hub.dart';
import 'local.dart';
import 'real_hub.dart';
import 'scope.dart';
import 'shell.dart';
import 'theme.dart';

void main() {
  LicenseRegistry.addLicense(() async* {
    final ofl = await rootBundle.loadString('assets/fonts/OFL.txt');
    yield LicenseEntryWithLineBreaks(const ['JetBrains Mono'], ofl);
  });
  runApp(const App());
}

/// Deep links (`?screen=nodes&theme=light`, `?sleeve=la/app`, `?node=build-arm64`) open a screen
/// directly; on desktop and Android the query is empty. On web the app talks to the hub that
/// served it unless `?hub=<url>` names another or `?hub=fake` asks for the scripted FakeHub;
/// `?pair=<code>` pairs this browser with a code from `protectorate device add`.
class App extends StatefulWidget {
  const App({super.key});

  @override
  State<App> createState() => _AppState();
}

class _AppState extends State<App> {
  final Map<String, String> _q = Uri.base.queryParameters;
  late bool _dark = _q['theme'] != 'light';
  late String _url = _q['hub'] == 'fake'
      ? ''
      : (_q['hub'] ?? loadSetting('hub_url') ?? pageOrigin() ?? '');
  late String _token = _q['hub'] == 'fake' ? '' : (loadSetting('device_token') ?? '');
  late HubClient _hub = _open(_url, _token);

  static HubClient _open(String url, String token) =>
      url.isEmpty ? FakeHub() : RealHub(url, token: token.isEmpty ? null : token);

  @override
  void initState() {
    super.initState();
    final code = _q['pair'];
    if (code != null && _url.isNotEmpty) {
      dropQueryParam('pair');
      unawaited(_pair(code).catchError((Object _) => ''));
    }
  }

  /// An empty URL means the scripted FakeHub. A real hub's URL and token are kept on the device.
  void _connect(String url, String token) {
    final u = url.trim();
    final t = token.trim();
    if (u.isNotEmpty) {
      saveSetting('hub_url', u == pageOrigin() ? null : u);
      saveSetting('device_token', t);
    }
    if (u == _url && t == _token) return;
    _hub.close();
    setState(() {
      _url = u;
      _token = t;
      _hub = _open(u, t);
    });
  }

  /// Trades a pairing code for this device's token and connects with it.
  Future<String> _pair(String code) async {
    if (_url.isEmpty) throw StateError('set the hub URL first');
    final r = await HubApi(Uri.parse(_url)).pairDevice(PairDeviceRequest(code: code.trim()));
    saveSetting('device_id', r.device.id);
    _connect(_url, r.token);
    return r.device.name;
  }

  @override
  void dispose() {
    _hub.close();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final p = _dark ? Palette.amber : Palette.printer;
    final st = _hub.state;
    final sleeve = _q['sleeve'];
    final node = _q['node'];
    final sleeveId = sleeve == null
        ? null
        : st.sleeves.where((s) => s.name == sleeve || s.id == sleeve).firstOrNull?.id;
    final nodeId = node == null
        ? null
        : st.nodes.where((n) => n.name == node || n.id == node).firstOrNull?.id;
    final screen = sleeveId != null
        ? Screen.sleeve
        : nodeId != null
        ? Screen.node
        : Screen.values.where((s) => s.name == _q['screen']).firstOrNull ?? Screen.envoy;
    return MaterialApp(
      title: 'Protectorate',
      debugShowCheckedModeBanner: false,
      theme: buildTheme(p),
      home: Shell(
        key: ObjectKey(_hub),
        hub: _hub,
        dark: _dark,
        setDark: (v) => setState(() => _dark = v),
        hubUrl: _url,
        hubToken: _token,
        connect: _connect,
        pair: _pair,
        initial: screen,
        initialSleeve: sleeveId,
        initialNode: nodeId,
      ),
    );
  }
}
