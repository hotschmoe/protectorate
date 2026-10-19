import 'dart:async';

import 'package:app_links/app_links.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:shared_preferences/shared_preferences.dart';

import 'hubs/hub_client.dart';
import 'hubs/hub_profile.dart';
import 'hubs/pairing.dart';
import 'screens/hub_shell.dart';
import 'screens/hubs_screen.dart';
import 'screens/pair_dialog.dart';

const _e2eAutoConfirm = bool.fromEnvironment('E2E_AUTOCONFIRM');

class DeepLinks {
  const DeepLinks({this.initial, this.stream = const Stream.empty()});

  final Uri? initial;
  final Stream<Uri> stream;

  static Future<DeepLinks> platform() async {
    if (kIsWeb) return DeepLinks(initial: Uri.base);
    try {
      final links = AppLinks();
      return DeepLinks(
        initial: await links.getInitialLink(),
        stream: links.uriLinkStream,
      );
    } on Object {
      return const DeepLinks();
    }
  }
}

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  final prefs = await SharedPreferences.getInstance();
  runApp(
    ProtectorateApp(store: HubStore(prefs), links: await DeepLinks.platform()),
  );
}

class ProtectorateApp extends StatefulWidget {
  const ProtectorateApp({
    super.key,
    required this.store,
    this.clientFactory,
    this.links = const DeepLinks(),
  });

  final HubStore store;
  final HubClient Function(HubProfile)? clientFactory;
  final DeepLinks links;

  @override
  State<ProtectorateApp> createState() => _ProtectorateAppState();
}

class _ProtectorateAppState extends State<ProtectorateApp> {
  final _navKey = GlobalKey<NavigatorState>();
  StreamSubscription<Uri>? _sub;

  HubClient Function(HubProfile) get _factory =>
      widget.clientFactory ?? HubClient.new;

  @override
  void initState() {
    super.initState();
    _sub = widget.links.stream.listen(_onLink);
    final initial = widget.links.initial;
    if (initial != null) {
      WidgetsBinding.instance.addPostFrameCallback((_) => _onLink(initial));
    }
  }

  @override
  void dispose() {
    _sub?.cancel();
    super.dispose();
  }

  Future<void> _onLink(Uri uri) async {
    final req = parsePairing(uri);
    final nav = _navKey.currentState;
    if (req == null || nav == null || !nav.mounted) return;
    if (kIsWeb) {
      unawaited(
        SystemNavigator.routeInformationUpdated(
          uri: Uri.parse('/'),
          replace: true,
        ),
      );
    }
    final existing = widget.store.hubs
        .where((h) => h.baseUrl == req.baseUrl)
        .firstOrNull;
    final ok =
        (_e2eAutoConfirm && req.autoConfirm) ||
        await confirmPairing(nav.context, req, update: existing != null);
    if (!ok || !nav.mounted) return;
    final profile = HubProfile(
      id: existing?.id ?? DateTime.now().microsecondsSinceEpoch.toString(),
      name: req.name,
      baseUrl: req.baseUrl,
      token: req.token,
    );
    await widget.store.upsert(profile);
    if (!nav.mounted) return;
    unawaited(
      nav.push(
        PageRouteBuilder<void>(
          transitionDuration: Duration.zero,
          reverseTransitionDuration: Duration.zero,
          pageBuilder: (_, _, _) => HubShell(
            hub: profile,
            clientFactory: _factory,
            initialTab: req.tab,
            openSession: req.session,
            openNode: req.node,
          ),
        ),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      navigatorKey: _navKey,
      title: 'Protectorate',
      themeMode: ThemeMode.system,
      theme: ThemeData(
        colorScheme: ColorScheme.fromSeed(seedColor: Colors.indigo),
        useMaterial3: true,
      ),
      darkTheme: ThemeData(
        colorScheme: ColorScheme.fromSeed(
          seedColor: Colors.indigo,
          brightness: Brightness.dark,
        ),
        useMaterial3: true,
      ),
      home: HubsScreen(store: widget.store, clientFactory: _factory),
    );
  }
}
