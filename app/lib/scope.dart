import 'package:flutter/widgets.dart';

import 'hub.dart';
import 'api/api.g.dart';

enum Screen { envoy, sleeves, approvals, nodes, quota, events, settings, sleeve, node }

/// What every screen reaches for: the hub, navigation and the message line.
class AppScope extends InheritedWidget {
  const AppScope({
    super.key,
    required this.hub,
    required this.state,
    required this.go,
    required this.openSleeve,
    required this.openNode,
    required this.say,
    required this.dark,
    required this.setDark,
    required this.hubUrl,
    required this.hubToken,
    required this.connect,
    required this.pair,
    required super.child,
  });

  final HubClient hub;
  final HubState state;
  final void Function(Screen) go;
  final void Function(String sleeveId) openSleeve;
  final void Function(String nodeId) openNode;
  final void Function(String text, {Level level}) say;
  final bool dark;
  final void Function(bool) setDark;

  /// The hub the app talks to; empty means the scripted FakeHub.
  final String hubUrl;
  final String hubToken;
  final void Function(String url, String token) connect;

  /// Trades a code from `protectorate device add` for this device's token; the device's name.
  final Future<String> Function(String code) pair;

  static AppScope of(BuildContext context) {
    final s = context.dependOnInheritedWidgetOfExactType<AppScope>();
    assert(s != null, 'no AppScope above this widget');
    return s!;
  }

  /// Runs a hub action and puts its failure on the message line.
  Future<void> act(Future<void> Function() f, {String? done}) async {
    try {
      await f();
      if (done != null) say(done);
    } catch (e) {
      say(e is StateError ? e.message : '$e', level: Level.alarm);
    }
  }

  @override
  bool updateShouldNotify(AppScope old) =>
      old.state != state || old.dark != dark || old.hub != hub || old.hubUrl != hubUrl;
}
