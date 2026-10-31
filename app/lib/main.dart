import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'fake_hub.dart';
import 'hub.dart';
import 'scope.dart';
import 'shell.dart';
import 'theme.dart';

void main() {
  LicenseRegistry.addLicense(() async* {
    final ofl = await rootBundle.loadString('assets/fonts/OFL.txt');
    yield LicenseEntryWithLineBreaks(const ['JetBrains Mono'], ofl);
  });
  runApp(App(hub: FakeHub()));
}

/// Deep links (`?screen=nodes&theme=light`, `?sleeve=la/app`, `?node=ms-r1`) open a screen
/// directly; on desktop and Android the query is empty.
class App extends StatefulWidget {
  const App({super.key, required this.hub});

  final HubClient hub;

  @override
  State<App> createState() => _AppState();
}

class _AppState extends State<App> {
  final Map<String, String> _q = Uri.base.queryParameters;
  late bool _dark = _q['theme'] != 'light';

  @override
  void dispose() {
    widget.hub.close();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final p = _dark ? Palette.amber : Palette.printer;
    final st = widget.hub.state;
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
        hub: widget.hub,
        dark: _dark,
        setDark: (v) => setState(() => _dark = v),
        initial: screen,
        initialSleeve: sleeveId,
        initialNode: nodeId,
      ),
    );
  }
}
