import 'package:flutter/material.dart';

import '../api/api.g.dart';
import '../derived.dart';
import '../scope.dart';
import '../theme.dart';
import '../widgets.dart';

/// Rows of the workspaces > sleeves > children tree, in display order.
List<(Workspace, Sleeve?, bool last)> treeRows(HubState s) {
  final rows = <(Workspace, Sleeve?, bool)>[];
  for (final w in s.workspaces) {
    rows.add((w, null, false));
    final roots = s.sleeves.where((x) => x.workspaceId == w.id && x.parentId == null);
    for (final r in roots) {
      rows.add((w, r, false));
      final kids = s.sleeves.where((x) => x.parentId == r.id).toList();
      for (var i = 0; i < kids.length; i++) {
        rows.add((w, kids[i], i == kids.length - 1));
      }
    }
  }
  return rows;
}

String treeStatusWord(HubState s) {
  var working = 0;
  var blocked = 0;
  for (final x in s.sleeves) {
    if (x.lifecycle != Lifecycle.running) continue;
    if (x.activity == ActivityState.working) working++;
    if (x.activity == ActivityState.blocked) blocked++;
  }
  return '$working WORKING  $blocked BLOCKED';
}

/// The tree pane: the left column on desktop, the F2 screen on the phone.
class SleeveTree extends StatelessWidget {
  const SleeveTree({super.key, this.selected});

  final String? selected;

  @override
  Widget build(BuildContext context) {
    final app = AppScope.of(context);
    final p = pal(context);
    final rows = treeRows(app.state);
    return ListView.builder(
      itemCount: rows.length,
      itemBuilder: (c, i) {
        final (w, s, last) = rows[i];
        if (s == null) {
          final n = app.state.sleeves.where((x) => x.workspaceId == w.id).length;
          return Padding(
            padding: EdgeInsets.only(top: i == 0 ? 0 : 4),
            child: Row(children: [
              Expanded(child: T(w.name, color: p.fg, bold: true)),
              T('$n sleeve${n == 1 ? '' : 's'}', color: p.dim),
            ]),
          );
        }
        return _SleeveRow(sleeve: s, child: s.parentId != null, last: last, selected: s.id == selected);
      },
    );
  }
}

class _SleeveRow extends StatefulWidget {
  const _SleeveRow({required this.sleeve, required this.child, required this.last, required this.selected});

  final Sleeve sleeve;
  final bool child;
  final bool last;
  final bool selected;

  @override
  State<_SleeveRow> createState() => _SleeveRowState();
}

class _SleeveRowState extends State<_SleeveRow> {
  bool _hot = false;

  @override
  Widget build(BuildContext context) {
    final app = AppScope.of(context);
    final p = pal(context);
    final s = widget.sleeve;
    final st = sleeveStatus(p, s);
    final sel = widget.selected;
    final nameColor = sel ? p.info : p.fg;
    return MouseRegion(
      cursor: SystemMouseCursors.click,
      onEnter: (_) => setState(() => _hot = true),
      onExit: (_) => setState(() => _hot = false),
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTap: () => s.role == SleeveRole.envoy ? app.go(Screen.envoy) : app.openSleeve(s.id),
        child: Container(
          color: sel || _hot ? p.hotFill : null,
          child: Row(children: [
            Cells(2, child: T(sel ? '>' : st.glyph, color: sel ? p.info : st.color)),
            const Cells(2, child: SizedBox()),
            if (widget.child) Branch(last: widget.last),
            Expanded(child: T(s.name, color: nameColor)),
            Cells(10, child: T(harnessName(s.incarnation?.harness ?? Harness.unknown), color: p.dim)),
            Cells(10, child: Line([statusSpan(context, st)])),
          ]),
        ),
      ),
    );
  }
}

/// The F2 screen on desktop: every sleeve with its incarnation in fixed columns.
class SleevesTable extends StatelessWidget {
  const SleevesTable({super.key});

  @override
  Widget build(BuildContext context) {
    final app = AppScope.of(context);
    final p = pal(context);
    final rows = treeRows(app.state).where((r) => r.$2 != null).toList();
    Widget head(String t, int n, {bool right = false}) =>
        Cells(n, right: right, child: T(t, color: p.dim));
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Row(children: [
          head('', 2),
          Expanded(child: T('NAME', color: p.dim)),
          head('WORKSPACE', 14),
          head('HARNESS', 10),
          head('MODEL', 22),
          head('G', 4, right: true),
          head('', 2),
          head('LIFECYCLE', 13),
          head('ACTIVITY', 14),
          head('SINCE', 8, right: true),
          head('CTX', 7, right: true),
        ]),
        Container(height: 1, color: p.edge),
        Expanded(
          child: Tick(builder: (c, now) => ListView.builder(
            itemCount: rows.length,
            itemBuilder: (c, i) {
              final (w, s, last) = rows[i];
              final x = s!;
              final inc = x.incarnation;
              final st = sleeveStatus(p, x);
              final act = inc?.activity;
              return GestureDetector(
                behavior: HitTestBehavior.opaque,
                onTap: () => x.role == SleeveRole.envoy ? app.go(Screen.envoy) : app.openSleeve(x.id),
                child: MouseRegion(
                  cursor: SystemMouseCursors.click,
                  child: Container(
                    color: i.isOdd ? p.bar : null,
                    child: Row(children: [
                      Cells(2, child: T(st.glyph, color: st.color)),
                      if (x.parentId != null) Branch(last: last),
                      Expanded(child: T(x.name)),
                      Cells(14, child: T(w.name, color: p.dim)),
                      Cells(10, child: T(harnessName(inc?.harness ?? Harness.unknown))),
                      Cells(22, child: T(inc?.model ?? '-', color: p.dim)),
                      Cells(4, right: true, child: T('${inc?.g ?? '-'}')),
                      const Cells(2, child: SizedBox()),
                      Cells(13, child: T(x.lifecycle.wire.toUpperCase(),
                          color: x.lifecycle == Lifecycle.running ? p.dim : st.color)),
                      Cells(14, child: Line([statusSpan(c, st)])),
                      Cells(8, right: true, child: T(act == null ? '-' : span(now.difference(act.since)), color: p.dim)),
                      Cells(7, right: true, child: T(inc == null ? '-' : tokens(inc.contextTokens), color: p.dim)),
                    ]),
                  ),
                ),
              );
            },
          )),
        ),
      ],
    );
  }
}

/// A tree connector two cells wide, drawn as 1 px rules so it joins across rows.
class Branch extends StatelessWidget {
  const Branch({super.key, required this.last});

  final bool last;

  @override
  Widget build(BuildContext context) => CustomPaint(
    size: const Size(kCell * 2, kRow),
    painter: _BranchPainter(pal(context).edge, last),
  );
}

class _BranchPainter extends CustomPainter {
  _BranchPainter(this.color, this.last);

  final Color color;
  final bool last;

  @override
  void paint(Canvas canvas, Size size) {
    final paint = Paint()
      ..color = color
      ..strokeWidth = 1;
    final x = (kCell / 2).floorToDouble() + 0.5;
    final y = (size.height / 2).floorToDouble() + 0.5;
    canvas.drawLine(Offset(x, 0), Offset(x, last ? y : size.height), paint);
    canvas.drawLine(Offset(x, y), Offset(size.width - 2, y), paint);
  }

  @override
  bool shouldRepaint(_BranchPainter old) => old.color != color || old.last != last;
}
