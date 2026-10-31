import 'package:flutter/widgets.dart';

import 'hub.dart';
import 'model.dart';

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
  bool updateShouldNotify(AppScope old) => old.state != state || old.dark != dark;
}
