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
import 'theme/tokens.dart';
import 'vfd/panel.dart';

class DeepLinks {
  const DeepLinks({this.initial, this.stream = const Stream.empty()});

  final Uri? initial;
  final Stream<Uri> stream;

  static Future<DeepLinks> platform() async {
    if (kIsWeb) return DeepLinks(initial: Uri.base);
    final links = AppLinks();
    return DeepLinks(
      initial: await links.getInitialLink(),
      stream: links.uriLinkStream,
    );
  }
}

void main() {
  var firstFrameRendered = false;
  var startupFailed = false;
  late final Zone startupZone;

  void showStartupError(Object error, StackTrace stack) {
    FlutterError.presentError(
      FlutterErrorDetails(
        exception: error,
        stack: stack,
        library: 'Protectorate startup',
      ),
    );
    if (startupFailed) return;
    startupFailed = true;
    startupZone.run(() => runApp(_StartupErrorApp(error: error)));
  }

  runZonedGuarded(
    () async {
      startupZone = Zone.current;
      try {
        WidgetsFlutterBinding.ensureInitialized();
        final previousErrorHandler = FlutterError.onError;
        FlutterError.onError = (details) {
          if (!firstFrameRendered) {
            showStartupError(
              details.exception,
              details.stack ?? StackTrace.current,
            );
          } else if (previousErrorHandler != null) {
            previousErrorHandler(details);
          } else {
            FlutterError.presentError(details);
          }
        };
        final prefs = await SharedPreferences.getInstance();
        final app = ProtectorateApp(
          store: HubStore(prefs),
          links: await DeepLinks.platform(),
        );
        if (startupFailed) return;
        runApp(app);
        WidgetsBinding.instance.addPostFrameCallback((_) {
          firstFrameRendered = true;
        });
      } on Object catch (error, stack) {
        showStartupError(error, stack);
      }
    },
    (error, stack) {
      if (!firstFrameRendered) {
        showStartupError(error, stack);
      } else {
        Zone.current.handleUncaughtError(error, stack);
      }
    },
  );
}

class _StartupErrorApp extends StatelessWidget {
  const _StartupErrorApp({required this.error});

  final Object error;

  @override
  Widget build(BuildContext context) {
    final home = Builder(
      builder: (context) {
        final t = Tokens.of(context);
        return Scaffold(
          key: const ValueKey('startup-error'),
          body: SafeArea(
            child: Center(
              child: SingleChildScrollView(
                padding: const EdgeInsets.all(Tokens.gapL),
                child: ConstrainedBox(
                  constraints: const BoxConstraints(
                    maxWidth: Tokens.contentMaxWidth,
                  ),
                  child: Panel(
                    legend: 'STARTUP ERROR',
                    tone: Tone.alarm,
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        Text(
                          'Could not start Protectorate',
                          style: t.title(t.p.alarm),
                        ),
                        const SizedBox(height: Tokens.gap),
                        SelectableText(
                          '$error',
                          key: const ValueKey('startup-error-message'),
                          style: t.body(),
                        ),
                        const SizedBox(height: Tokens.gap),
                        Text('Restart the app to try again.', style: t.small()),
                      ],
                    ),
                  ),
                ),
              ),
            ),
          ),
        );
      },
    );
    return MaterialApp(
      title: 'Protectorate',
      theme: appTheme(Brightness.light),
      darkTheme: appTheme(Brightness.dark),
      routes: {Navigator.defaultRouteName: (_) => home},
      onGenerateInitialRoutes: (_) => [
        MaterialPageRoute<void>(
          settings: const RouteSettings(name: Navigator.defaultRouteName),
          builder: (_) => home,
        ),
      ],
    );
  }
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
    final profile = existing != null && existing.deviceId.isNotEmpty
        ? existing
        : await connectHub(
            nav.context,
            clientFactory: _factory,
            save: widget.store.upsert,
            existing: existing,
            baseUrl: req.baseUrl,
            caPem: req.caPem,
          );
    if (profile == null) return;
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
    final home = HubsScreen(store: widget.store, clientFactory: _factory);
    return MaterialApp(
      navigatorKey: _navKey,
      title: 'Protectorate',
      themeMode: ThemeMode.system,
      theme: appTheme(Brightness.light),
      darkTheme: appTheme(Brightness.dark),
      builder: (context, child) => WarmUp(child: child ?? const SizedBox()),
      routes: {Navigator.defaultRouteName: (_) => home},
      onGenerateInitialRoutes: (_) => [
        MaterialPageRoute<void>(
          settings: const RouteSettings(name: Navigator.defaultRouteName),
          builder: (_) => home,
        ),
      ],
    );
  }
}
