import 'package:connectrpc/connect.dart';
import 'package:fixnum/fixnum.dart';
import 'package:flutter/material.dart';
import 'package:protobuf/well_known_types/google/protobuf/duration.pb.dart'
    as pb;

import '../gen/protectorate/v1/desk.pb.dart';
import '../gen/protectorate/v1/node.pb.dart';
import '../theme/tokens.dart';
import '../vfd/lamp.dart';
import '../vfd/meter.dart';
import '../vfd/panel.dart';
import 'node_view.dart';

bool isWindowsNode(NodeStatus s) {
  final os = s.hasMetrics() && s.metrics.os.isNotEmpty
      ? s.metrics.os
      : s.node.os;
  return os == 'windows';
}

// The desktop's state in a word, as `node ls` prints it: who holds it, or
// why agents may or may not have it now.
(String, Tone) deskStatus(DeskState? s, String holder) {
  if (holder.isNotEmpty) {
    final who = holder.startsWith('auto:') ? holder.substring(5) : holder;
    return ('IN USE BY $who', Tone.info);
  }
  if (s == null) return ('NOT REPORTED', Tone.dim);
  return switch (s.reason) {
    DeskReason.DESK_REASON_AVAILABLE =>
      s.hasPolicy() ? ('AWAY', Tone.ok) : ('AVAILABLE', Tone.ok),
    DeskReason.DESK_REASON_OUTSIDE_WINDOW => ('OUTSIDE WINDOW', Tone.dim),
    DeskReason.DESK_REASON_USER_ACTIVE => ('USER ACTIVE', Tone.warn),
    DeskReason.DESK_REASON_NOT_INTERACTIVE => ('NOT INTERACTIVE', Tone.alarm),
    DeskReason.DESK_REASON_LOCKED => ('SCREEN LOCKED', Tone.warn),
    DeskReason.DESK_REASON_UNSUPPORTED => ('UNSUPPORTED', Tone.dim),
    _ => ('UNKNOWN', Tone.dim),
  };
}

String formatDuration(Duration d) {
  if (d.inMinutes < 1) return '${d.inSeconds}s';
  final h = d.inHours;
  final m = d.inMinutes % 60;
  if (h == 0) return '${m}m';
  return m == 0 ? '${h}h' : '${h}h${m}m';
}

Duration durationOf(pb.Duration d) =>
    Duration(seconds: d.seconds.toInt(), microseconds: d.nanos ~/ 1000);

String policyLine(DeskPolicy p) => [
  'window ${p.window}',
  p.timezone.isEmpty ? 'node local time' : p.timezone,
  'idle ${formatDuration(durationOf(p.idle))}',
  if (p.lockAfter) 'lock after',
].join(', ');

String inputsLabel(List<DeskUse> u) =>
    u.map((e) => '${e.count} by ${e.caller}').join(', ');

final _windowRe = RegExp(r'^([01]\d|2[0-3]):[0-5]\d-([01]\d|2[0-3]):[0-5]\d$');

// The policy the form describes, or the sentence that says what is wrong.
(DeskPolicy?, String?) formPolicy({
  required String window,
  required String idleMinutes,
  required String timezone,
  required bool lockAfter,
}) {
  final w = window.trim();
  if (!_windowRe.hasMatch(w)) {
    return (null, 'The window is HH:MM-HH:MM, for example 19:00-07:00.');
  }
  final idle = int.tryParse(idleMinutes.trim());
  if (idle == null || idle <= 0) {
    return (null, 'Idle is a whole number of minutes, at least 1.');
  }
  return (
    DeskPolicy(
      window: w,
      idle: pb.Duration(seconds: Int64(idle * 60)),
      timezone: timezone.trim(),
      lockAfter: lockAfter,
    ),
    null,
  );
}

// The screenshot width to ask for: the pane's device pixels, rounded up so a
// small resize does not change it, within what is worth sending.
int screenshotWidth(double logicalWidth, double devicePixelRatio) {
  final px = (logicalWidth * devicePixelRatio / 160).ceil() * 160;
  return px.clamp(320, 1920);
}

String deskFailure(Object e, String what) {
  if (e is ConnectException) {
    return switch (e.code) {
      Code.permissionDenied =>
        '$what needs the owner token or a desk_* grant on this device.',
      Code.unimplemented => 'This node has no desktop driver.',
      _ => e.message,
    };
  }
  return '$e';
}

// The desktop at a glance on a node's detail: availability, holder and
// today's input, and the way in.
class DeskPanel extends StatelessWidget {
  const DeskPanel({
    super.key,
    required this.status,
    required this.now,
    this.onOpen,
  });

  final NodeStatus status;
  final DateTime now;
  final VoidCallback? onOpen;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final s = status.hasDesk() ? status.desk : null;
    final (label, tone) = deskStatus(s, status.deskHolder);
    final total = status.deskInputs.fold<int>(0, (n, u) => n + u.count);
    return Panel(
      legend: 'DESK',
      trailing: LampLegend(
        key: const ValueKey('desk-status'),
        label: label,
        tone: tone,
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          if (s != null && s.detail.isNotEmpty) Text(s.detail, style: t.data()),
          Text(
            s != null && s.hasPolicy()
                ? 'Policy: ${policyLine(s.policy)}'
                : 'No policy: agents may use the desktop at any time.',
            style: t.small(),
          ),
          Text(
            total == 0
                ? 'No desk input today.'
                : 'Input today: ${inputsLabel(status.deskInputs)}',
            style: t.small(total == 0 ? null : t.p.lit),
          ),
          if (onOpen != null)
            Align(
              alignment: Alignment.centerRight,
              child: Padding(
                padding: const EdgeInsets.only(top: Tokens.gapS),
                child: OutlinedButton(
                  key: const ValueKey('open-desk'),
                  onPressed: onOpen,
                  child: const Text('OPEN DESK'),
                ),
              ),
            ),
        ],
      ),
    );
  }
}

// Today's desktop input commands per caller, as bars against the busiest.
class DeskInputs extends StatelessWidget {
  const DeskInputs({super.key, required this.inputs});

  final List<DeskUse> inputs;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final most = inputs.fold<int>(0, (n, u) => u.count > n ? u.count : n);
    final total = inputs.fold<int>(0, (n, u) => n + u.count);
    final sorted = [...inputs]..sort((a, b) => b.count.compareTo(a.count));
    return Panel(
      legend: 'INPUT TODAY',
      trailing: Text('$total', style: t.legend()),
      child: sorted.isEmpty
          ? Text(
              'No click, move, type, key or run commands today.',
              style: t.small(),
            )
          : Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                for (final u in sorted)
                  Padding(
                    key: ValueKey('input-${u.caller}'),
                    padding: const EdgeInsets.symmetric(vertical: 3),
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          u.caller,
                          style: t.data(t.p.emph),
                          overflow: TextOverflow.ellipsis,
                        ),
                        Row(
                          children: [
                            Expanded(
                              child: SegmentMeter(
                                percent: most == 0 ? 0 : u.count * 100 / most,
                                tone: Tone.info,
                              ),
                            ),
                            SizedBox(
                              width: monoWidth(6, 13),
                              child: Text(
                                '${u.count}',
                                textAlign: TextAlign.end,
                                style: t.data(t.p.emph),
                              ),
                            ),
                          ],
                        ),
                      ],
                    ),
                  ),
                Padding(
                  padding: const EdgeInsets.only(top: 4),
                  child: Text(
                    'Counted by the hub per local day; nothing is limited.',
                    style: t.small(),
                  ),
                ),
              ],
            ),
    );
  }
}

// What would receive keystrokes now, and whether injected input reaches it.
class ForegroundPanel extends StatelessWidget {
  const ForegroundPanel({super.key, this.foreground, this.error});

  final ForegroundResponse? foreground;
  final String? error;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final f = foreground;
    return Panel(
      legend: 'FOREGROUND',
      trailing: f == null
          ? null
          : f.blocks
          ? const Annunciator(
              key: ValueKey('fg-blocks'),
              label: 'BLOCKS INPUT',
              tone: Tone.alarm,
            )
          : const LampLegend(
              key: ValueKey('fg-reaches'),
              label: 'INPUT REACHES',
              tone: Tone.ok,
            ),
      child: error != null
          ? Text(error!, style: t.small(t.p.alarm))
          : f == null
          ? Text('Not read yet.', style: t.small())
          : Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  f.title.isEmpty ? 'No window has the foreground.' : f.title,
                  style: t.data(t.p.emph),
                  overflow: TextOverflow.ellipsis,
                ),
                const SizedBox(height: 4),
                Text(
                  [
                    if (f.process.isNotEmpty) f.process,
                    if (f.pid > 0) 'pid ${f.pid}',
                    'integrity ${f.integrity}',
                    'agent ${f.agentIntegrity}',
                  ].join('  '),
                  style: t.small(f.blocks ? t.p.alarm : null),
                ),
                if (f.blocks)
                  Padding(
                    padding: const EdgeInsets.only(top: 6),
                    child: Text(
                      'Windows drops input into a higher-integrity window. '
                      'Dismiss it on the machine, or reinstall the agent '
                      'elevated.',
                      style: t.small(),
                    ),
                  ),
              ],
            ),
    );
  }
}

class WindowList extends StatelessWidget {
  const WindowList({
    super.key,
    required this.windows,
    this.foreground,
    this.error,
  });

  final List<DeskWindow> windows;
  final ForegroundResponse? foreground;
  final String? error;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final fg = foreground;
    return Panel(
      legend: 'WINDOWS  TOPMOST FIRST',
      trailing: Text('${windows.length}', style: t.legend()),
      padding: const EdgeInsets.all(4),
      child: error != null
          ? Padding(
              padding: const EdgeInsets.all(8),
              child: Text(error!, style: t.small(t.p.alarm)),
            )
          : windows.isEmpty
          ? Padding(
              padding: const EdgeInsets.all(8),
              child: Text('No visible windows.', style: t.small()),
            )
          : Column(
              children: [
                for (var i = 0; i < windows.length; i++)
                  BarRow(
                    key: ValueKey('window-$i'),
                    index: i,
                    child: Row(
                      children: [
                        Lamp(
                          tone: Tone.info,
                          size: 8,
                          on:
                              fg != null &&
                              fg.pid == windows[i].pid &&
                              fg.title == windows[i].title,
                        ),
                        const SizedBox(width: 10),
                        Expanded(
                          child: Text(
                            windows[i].title.isEmpty
                                ? '(untitled)'
                                : windows[i].title,
                            style: t.data(
                              windows[i].minimized ? t.p.dim : t.p.emph,
                            ),
                            overflow: TextOverflow.ellipsis,
                          ),
                        ),
                        if (windows[i].minimized) ...[
                          const SizedBox(width: 6),
                          Text('MIN', style: t.legend()),
                        ],
                        const SizedBox(width: 10),
                        Text(
                          [
                            if (windows[i].width > 0)
                              '${windows[i].width}x${windows[i].height}',
                            'pid ${windows[i].pid}',
                          ].join('  '),
                          style: t.small(),
                        ),
                      ],
                    ),
                  ),
              ],
            ),
    );
  }
}

String lastInputLabel(DeskState s, DateTime now) => s.hasLastInput()
    ? 'physical input ${formatAge(now.difference(s.lastInput.toDateTime()))}'
    : 'no physical input seen since the agent started';
