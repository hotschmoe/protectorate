import 'package:connectrpc/connect.dart';
import 'package:connectrpc/test.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:protobuf/well_known_types/google/protobuf/timestamp.pb.dart';
import 'package:protectorate/gen/protectorate/v1/audit.connect.spec.dart';
import 'package:protectorate/gen/protectorate/v1/audit.pb.dart';
import 'package:protectorate/hubs/hub_client.dart';
import 'package:protectorate/screens/audit_screen.dart';
import 'package:protectorate/theme/tokens.dart';

final _t0 = DateTime.utc(2026, 10, 20, 12);

AuditEntry _entry(int i, {String outcome = 'ok', String code = ''}) =>
    AuditEntry(
      time: Timestamp.fromDateTime(_t0.subtract(Duration(minutes: i))),
      actor: i.isEven ? 'owner' : 'token:pixel',
      tailnet: i.isEven ? '' : 'pixel.tail0b1c.ts.net',
      procedure: '/protectorate.v1.SessionService/KillSession',
      subject: 'work-$i',
      outcome: outcome,
      code: code,
    );

class _Hub {
  final requests = <ListAuditEntriesRequest>[];
  bool owner = true;

  HubClient client() => HubClient.withTransport(
    FakeTransportBuilder().unary(AuditService.listAuditEntries, (
      ListAuditEntriesRequest r,
      _,
    ) {
      requests.add(r);
      if (!owner) throw ConnectException(Code.permissionDenied, 'owner');
      if (r.pageToken.isNotEmpty) {
        return ListAuditEntriesResponse(entries: [_entry(5)]);
      }
      return ListAuditEntriesResponse(
        entries: [
          _entry(0),
          _entry(1, outcome: 'denied', code: 'permission_denied'),
          _entry(2, outcome: 'error', code: 'internal'),
        ],
        nextPageToken: 'audit-2026-10.jsonl:40',
      );
    }).build(),
  );
}

Future<void> _pump(WidgetTester t, _Hub hub, {double width = 412}) async {
  t.view.physicalSize = Size(width, 1400);
  t.view.devicePixelRatio = 1;
  addTearDown(t.view.reset);
  await t.pumpWidget(
    MaterialApp(
      theme: appTheme(Brightness.dark),
      home: AuditScreen(client: hub.client(), now: () => _t0),
    ),
  );
  await t.pump();
  await t.pump();
}

void main() {
  test('labels', () {
    expect(
      shortProcedure('/protectorate.v1.SessionService/KillSession'),
      'SessionService/KillSession',
    );
    expect(shortProcedure('/ws/terminal/work'), '/ws/terminal/work');
    for (final (o, tone) in [
      ('ok', Tone.ok),
      ('denied', Tone.warn),
      ('error', Tone.alarm),
      ('', Tone.dim),
    ]) {
      expect(outcomeTone(o), tone);
    }
    final today = _t0.subtract(const Duration(minutes: 1));
    final l = today.toLocal();
    expect(auditTime(today, _t0), startsWith(two(l.hour)));
    expect(
      auditTime(_t0.subtract(const Duration(days: 3)), _t0),
      contains('2026-10-'),
    );
  });

  for (final width in [412.0, 1280.0]) {
    testWidgets('entries, outcomes and paging at $width', (t) async {
      final hub = _Hub();
      await _pump(t, hub, width: width);
      expect(hub.requests.single.pageToken, isEmpty);
      expect(hub.requests.single.limit, 100);
      expect(find.text('SessionService/KillSession'), findsNWidgets(3));
      expect(find.text('DENIED'), findsWidgets);
      expect(find.text('ERROR'), findsWidgets);
      expect(find.textContaining('permission_denied'), findsOneWidget);
      expect(find.textContaining('internal'), findsOneWidget);
      expect(find.text('OK'), findsWidgets);
      expect(find.textContaining('pixel.tail0b1c.ts.net'), findsOneWidget);
      expect(find.text('3+'), findsOneWidget);
      await t.ensureVisible(find.byKey(const ValueKey('audit-older')));
      await t.tap(find.byKey(const ValueKey('audit-older')));
      await t.pump();
      await t.pump();
      expect(hub.requests.last.pageToken, 'audit-2026-10.jsonl:40');
      expect(find.byKey(const ValueKey('audit-3')), findsOneWidget);
      expect(find.byKey(const ValueKey('audit-older')), findsNothing);
    });
  }

  testWidgets('filters go to the hub and start over', (t) async {
    final hub = _Hub();
    await _pump(t, hub);
    await t.tap(find.byKey(const ValueKey('choice-DENIED')));
    await t.pump();
    expect(hub.requests.last.outcome, 'denied');
    expect(hub.requests.last.pageToken, isEmpty);
    await t.enterText(find.byKey(const ValueKey('audit-actor')), 'pixel');
    await t.enterText(find.byKey(const ValueKey('audit-subject')), 'work');
    await t.pump(const Duration(milliseconds: 500));
    await t.pump();
    final r = hub.requests.last;
    expect(r.actor, 'pixel');
    expect(r.subject, 'work');
    expect(r.outcome, 'denied');
    expect(hub.requests.where((q) => q.actor == 'pixel'), hasLength(1));
  });

  testWidgets('a token that is not the owner is told so', (t) async {
    final hub = _Hub()..owner = false;
    await _pump(t, hub);
    expect(find.textContaining('owner token only'), findsOneWidget);
    expect(find.text('ENTRIES  NEWEST FIRST'), findsNothing);
  });
}
