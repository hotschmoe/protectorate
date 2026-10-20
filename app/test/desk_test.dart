import 'dart:async';
import 'dart:convert';

import 'package:connectrpc/connect.dart';
import 'package:connectrpc/test.dart';
import 'package:fixnum/fixnum.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:protobuf/well_known_types/google/protobuf/duration.pb.dart'
    as pb;
import 'package:protobuf/well_known_types/google/protobuf/timestamp.pb.dart';
import 'package:protectorate/gen/protectorate/v1/desk.connect.spec.dart';
import 'package:protectorate/gen/protectorate/v1/desk.pb.dart';
import 'package:protectorate/gen/protectorate/v1/events.pb.dart';
import 'package:protectorate/gen/protectorate/v1/node.connect.spec.dart';
import 'package:protectorate/gen/protectorate/v1/node.pb.dart';
import 'package:protectorate/hubs/event_feed.dart';
import 'package:protectorate/hubs/hub_client.dart';
import 'package:protectorate/nodes/desk_view.dart';
import 'package:protectorate/screens/desk_screen.dart';
import 'package:protectorate/screens/node_detail_screen.dart';
import 'package:protectorate/theme/tokens.dart';

final _t0 = DateTime.utc(2026, 10, 20, 22);
final _png = base64Decode(
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==',
);

DeskPolicy _policy() => DeskPolicy(
  window: '19:00-07:00',
  idle: pb.Duration(seconds: Int64(900)),
  lockAfter: true,
);

NodeStatus _node({String holder = '', bool policy = true}) => NodeStatus(
  node: NodeInfo(name: 'office-pc', os: 'windows'),
  freshness: NodeFreshness.NODE_FRESHNESS_FRESH,
  metrics: NodeMetrics(os: 'windows', arch: 'amd64'),
  desk: DeskState(
    available: true,
    reason: DeskReason.DESK_REASON_AVAILABLE,
    detail: 'inside the window, idle 42m',
    lastInput: Timestamp.fromDateTime(
      _t0.subtract(const Duration(minutes: 42)),
    ),
    policy: policy ? _policy() : null,
  ),
  deskHolder: holder,
  deskInputs: [
    DeskUse(caller: 'token:sleeve:cad', count: 42),
    DeskUse(caller: 'owner', count: 3),
  ],
);

class _Hub {
  final shots = <ScreenshotRequest>[];
  final policies = <SetDeskPolicyRequest>[];
  bool owner = true;

  HubClient client() => HubClient.withTransport(
    FakeTransportBuilder()
        .unary(NodeService.getNode, (GetNodeRequest r, _) => _node())
        .unary(DeskService.screenshot, (ScreenshotRequest r, _) {
          shots.add(r);
          return ScreenshotResponse(png: _png, width: 2560, height: 1440);
        })
        .unary(
          DeskService.listWindows,
          (ListWindowsRequest r, _) => ListWindowsResponse(
            windows: [
              DeskWindow(
                title: 'ArchiCAD 27',
                width: 2560,
                height: 1400,
                pid: 42,
              ),
              DeskWindow(title: 'Mail', pid: 7, minimized: true),
            ],
          ),
        )
        .unary(
          DeskService.foreground,
          (ForegroundRequest r, _) => ForegroundResponse(
            title: 'ArchiCAD 27',
            pid: 42,
            process: 'ArchiCAD.exe',
            integrity: 'high',
            agentIntegrity: 'medium',
            blocks: true,
          ),
        )
        .unary(NodeService.setDeskPolicy, (SetDeskPolicyRequest r, _) {
          if (!owner) throw ConnectException(Code.permissionDenied, 'owner');
          policies.add(r);
          return DeskPolicyStatus(
            policy: r.hasPolicy() ? r.policy : null,
            state: DeskState(reason: DeskReason.DESK_REASON_AVAILABLE),
          );
        })
        .build(),
  );
}

Future<void> _pump(WidgetTester t, _Hub hub, {double width = 412}) async {
  t.view.physicalSize = Size(width, 2400);
  t.view.devicePixelRatio = 1;
  addTearDown(t.view.reset);
  final events = StreamController<Event>.broadcast();
  addTearDown(events.close);
  final feed = EventFeed(() => events.stream)..start();
  addTearDown(feed.dispose);
  await t.pumpWidget(
    MaterialApp(
      theme: appTheme(Brightness.dark),
      home: DeskScreen(
        client: hub.client(),
        feed: feed,
        name: 'office-pc',
        now: () => _t0,
      ),
    ),
  );
  await t.pump();
  await t.pump();
}

void main() {
  group('helpers', () {
    test('status words', () {
      DeskState st(DeskReason r, {bool policy = false}) =>
          DeskState(reason: r, policy: policy ? _policy() : null);
      final cases = [
        (null, '', ('NOT REPORTED', Tone.dim)),
        (st(DeskReason.DESK_REASON_AVAILABLE), '', ('AVAILABLE', Tone.ok)),
        (
          st(DeskReason.DESK_REASON_AVAILABLE, policy: true),
          '',
          ('AWAY', Tone.ok),
        ),
        (
          st(DeskReason.DESK_REASON_AVAILABLE),
          'auto:token:sleeve:cad',
          ('IN USE BY token:sleeve:cad', Tone.info),
        ),
        (
          st(DeskReason.DESK_REASON_USER_ACTIVE),
          '',
          ('USER ACTIVE', Tone.warn),
        ),
        (
          st(DeskReason.DESK_REASON_OUTSIDE_WINDOW),
          '',
          ('OUTSIDE WINDOW', Tone.dim),
        ),
        (st(DeskReason.DESK_REASON_LOCKED), '', ('SCREEN LOCKED', Tone.warn)),
        (
          st(DeskReason.DESK_REASON_NOT_INTERACTIVE),
          '',
          ('NOT INTERACTIVE', Tone.alarm),
        ),
        (st(DeskReason.DESK_REASON_UNKNOWN), '', ('UNKNOWN', Tone.dim)),
      ];
      for (final (s, holder, want) in cases) {
        expect(deskStatus(s, holder), want, reason: '${s?.reason} $holder');
      }
    });

    test('policy form', () {
      final cases = [
        ('19:00-07:00', '15', 'Europe/Berlin', true, null),
        ('7pm-7am', '15', '', false, 'HH:MM'),
        ('24:00-07:00', '15', '', false, 'HH:MM'),
        ('19:00-07:00', '0', '', false, 'minutes'),
        ('19:00-07:00', 'x', '', false, 'minutes'),
      ];
      for (final (window, idle, tz, lock, err) in cases) {
        final (p, e) = formPolicy(
          window: window,
          idleMinutes: idle,
          timezone: tz,
          lockAfter: lock,
        );
        if (err == null) {
          expect(p!.window, window);
          expect(p.idle.seconds.toInt(), 900);
          expect(p.timezone, tz);
          expect(p.lockAfter, lock);
        } else {
          expect(p, isNull);
          expect(e, contains(err));
        }
      }
      expect(
        policyLine(_policy()),
        'window 19:00-07:00, node local time, idle 15m, lock after',
      );
    });

    test('screenshot width follows the pane', () {
      for (final (w, dpr, want) in [
        (412.0, 2.625, 1120),
        (412.0, 1.0, 480),
        (100.0, 1.0, 320),
        (1200.0, 2.0, 1920),
      ]) {
        expect(screenshotWidth(w, dpr), want, reason: '$w x $dpr');
      }
    });
  });

  testWidgets('the desk shows screen, windows, foreground and use', (t) async {
    final hub = _Hub();
    await _pump(t, hub);
    expect(hub.shots, hasLength(1));
    expect(hub.shots.single.target.name, 'office-pc');
    expect(hub.shots.single.maxWidth, 480);
    expect(find.text('SCREEN  2560x1440'), findsOneWidget);
    expect(find.byKey(const ValueKey('view-only')), findsOneWidget);
    expect(find.text('AWAY'), findsOneWidget);
    expect(find.text('inside the window, idle 42m'), findsOneWidget);
    expect(find.text('physical input 42m ago'), findsOneWidget);
    expect(find.byKey(const ValueKey('fg-blocks')), findsOneWidget);
    expect(find.textContaining('integrity high'), findsOneWidget);
    expect(find.text('ArchiCAD 27'), findsNWidgets(2));
    expect(find.text('MIN'), findsOneWidget);
    expect(
      find.byKey(const ValueKey('input-token:sleeve:cad')),
      findsOneWidget,
    );
    expect(find.text('45'), findsOneWidget);
    for (final verb in ['CLICK', 'TYPE', 'KEY', 'RUN']) {
      expect(find.text(verb), findsNothing);
    }

    await t.pump(const Duration(seconds: 20));
    await t.pump();
    expect(hub.shots, hasLength(2));
    await t.pumpWidget(const SizedBox());
  });

  testWidgets('the owner sets and clears the policy', (t) async {
    final hub = _Hub();
    await _pump(t, hub);
    await t.enterText(find.byKey(const ValueKey('policy-window')), 'late');
    await t.ensureVisible(find.byKey(const ValueKey('policy-apply')));
    await t.tap(find.byKey(const ValueKey('policy-apply')));
    await t.pump();
    expect(find.textContaining('HH:MM-HH:MM'), findsOneWidget);
    expect(hub.policies, isEmpty);

    await t.enterText(
      find.byKey(const ValueKey('policy-window')),
      '20:00-06:00',
    );
    await t.enterText(find.byKey(const ValueKey('policy-idle')), '30');
    await t.tap(find.byKey(const ValueKey('choice-OFF')));
    await t.tap(find.byKey(const ValueKey('policy-apply')));
    await t.pump();
    await t.pump();
    final set = hub.policies.single;
    expect(set.name, 'office-pc');
    expect(set.policy.window, '20:00-06:00');
    expect(set.policy.idle.seconds.toInt(), 1800);
    expect(set.policy.lockAfter, isFalse);

    await t.ensureVisible(find.byKey(const ValueKey('policy-off')));
    await t.tap(find.byKey(const ValueKey('policy-off')));
    await t.pump();
    await t.pump();
    expect(hub.policies.last.hasPolicy(), isFalse);
    await t.pumpWidget(const SizedBox());
  });

  testWidgets('a device without the owner token is told so', (t) async {
    final hub = _Hub()..owner = false;
    await _pump(t, hub);
    await t.ensureVisible(find.byKey(const ValueKey('policy-apply')));
    await t.tap(find.byKey(const ValueKey('policy-apply')));
    await t.pump();
    await t.pump();
    expect(find.textContaining('needs the owner token'), findsOneWidget);
    await t.pumpWidget(const SizedBox());
  });

  testWidgets('node detail offers the desk only for Windows nodes', (t) async {
    for (final (os, shown) in [('windows', true), ('linux', false)]) {
      final s = _node(holder: 'auto:owner')
        ..node.os = os
        ..metrics.os = os;
      await t.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: NodeDetail(
              status: s,
              age: Duration.zero,
              now: _t0,
              onOpenDesk: () {},
            ),
          ),
        ),
      );
      expect(
        find.byKey(const ValueKey('open-desk')),
        shown ? findsOneWidget : findsNothing,
        reason: os,
      );
      if (shown) {
        expect(find.text('IN USE BY owner'), findsOneWidget);
        expect(
          find.textContaining('42 by token:sleeve:cad, 3 by owner'),
          findsOneWidget,
        );
      }
    }
  });
}
