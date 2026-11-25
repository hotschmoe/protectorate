import 'dart:async';

import 'package:flutter/material.dart';

import '../api/api.g.dart';
import '../derived.dart';
import '../icons.dart';
import '../scope.dart';
import '../theme.dart';
import '../widgets.dart';

const List<Harness> _harnessChoices = [
  Harness.claude,
  Harness.codex,
  Harness.piLocal,
  Harness.antigravity,
  Harness.grok,
];

/// Card shown on Envoy screen when no Envoy sleeve exists yet on the hub.
class StartEnvoyCard extends StatefulWidget {
  const StartEnvoyCard({super.key});

  @override
  State<StartEnvoyCard> createState() => _StartEnvoyCardState();
}

class _StartEnvoyCardState extends State<StartEnvoyCard> {
  Harness _harness = Harness.claude;
  bool _busy = false;

  Future<void> _start(AppScope app) async {
    if (_busy) return;
    setState(() => _busy = true);
    try {
      await app.act(() async {
        await app.hub.spawnSleeve(SpawnSleeveRequest(name: 'envoy', harness: _harness));
        app.go(Screen.envoy);
      }, done: 'Started Envoy (${harnessName(_harness)})');
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final app = AppScope.of(context);
    final p = pal(context);
    return Center(
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 460),
        child: Panel(
          title: 'No Envoy sleeve on this hub',
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              T(
                'Envoy coordinates workspaces, spawns child sleeves, and runs jobs across your nodes.',
                color: p.dim,
                maxLines: 3,
              ),
              const SizedBox(height: 14),
              const Label('Harness'),
              const SizedBox(height: 6),
              Buttons([
                for (final h in _harnessChoices)
                  Btn(
                    harnessName(h),
                    dense: true,
                    active: _harness == h,
                    onTap: _busy ? null : () => setState(() => _harness = h),
                  ),
              ]),
              const SizedBox(height: 16),
              Align(
                alignment: Alignment.centerLeft,
                child: Btn(
                  _busy ? 'Starting...' : 'Start Envoy',
                  kind: BtnKind.primary,
                  glyph: Glyph.envoy,
                  onTap: _busy ? null : () => unawaited(_start(app)),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

/// Modal dialog to spawn a new root, child, or envoy sleeve from the UI.
Future<void> showSpawnSleeveDialog(BuildContext context, AppScope app) {
  final look = Look.of(context);
  return showDialog<void>(
    context: context,
    builder: (ctx) => Look(p: look.p, phone: look.phone, child: _SpawnSleeveDialog(app: app)),
  );
}

class _SpawnSleeveDialog extends StatefulWidget {
  const _SpawnSleeveDialog({required this.app});

  final AppScope app;

  @override
  State<_SpawnSleeveDialog> createState() => _SpawnSleeveDialogState();
}

class _SpawnSleeveDialogState extends State<_SpawnSleeveDialog> {
  final _name = TextEditingController();
  final _path = TextEditingController();
  final _message = TextEditingController();
  Harness? _harness = Harness.claude;
  String? _parentId;
  bool _newDir = false;
  bool _busy = false;
  String? _error;

  @override
  void dispose() {
    _name.dispose();
    _path.dispose();
    _message.dispose();
    super.dispose();
  }

  Future<void> _submit() async {
    final name = _name.text.trim();
    final path = _path.text.trim();
    final msg = _message.text.trim();
    if (name.isEmpty) {
      setState(() => _error = 'Name is required');
      return;
    }
    final isEnvoy = _parentId == null && name == 'envoy';
    if (_parentId == null && !isEnvoy && path.isEmpty) {
      setState(() => _error = 'Directory path is required for a root sleeve');
      return;
    }
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final req = SpawnSleeveRequest(
        name: name,
        harness: _harness ?? Harness.claude,
        autoSelect: !isEnvoy && _harness == null,
        path: _parentId == null && !_newDir && path.isNotEmpty ? path : null,
        newPath: _parentId == null && _newDir && path.isNotEmpty ? path : null,
        parentId: _parentId,
        message: msg.isEmpty ? null : msg,
      );
      final s = await widget.app.hub.spawnSleeve(req);
      if (mounted) Navigator.of(context).pop();
      if (s.role == SleeveRole.envoy) {
        widget.app.go(Screen.envoy);
      } else {
        widget.app.openSleeve(s.id);
      }
      widget.app.say('Started sleeve ${s.name}');
    } catch (e) {
      if (mounted) {
        setState(() {
          _busy = false;
          _error = e is StateError ? e.message : '$e';
        });
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final p = pal(context);
    final roots = widget.app.state.sleeves
        .where((s) => s.parentId == null && s.role != SleeveRole.envoy)
        .toList();
    return Center(
      child: Material(
        color: Colors.transparent,
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 480),
          child: Padding(
            padding: const EdgeInsets.all(16),
            child: Panel(
              title: 'New sleeve',
              trailing: IconBtn(Glyph.close, tooltip: 'Close',
                  onTap: _busy ? null : () => Navigator.of(context).pop()),
              child: SingleChildScrollView(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    const Label('Name'),
                    const SizedBox(height: 4),
                    Field(controller: _name, hint: 'my-project (or envoy)', autofocus: true,
                        onSubmit: (_) => unawaited(_submit())),
                    if (roots.isNotEmpty) ...[
                      const SizedBox(height: 12),
                      const Label('Parent (optional, spawns a worktree child)'),
                      const SizedBox(height: 6),
                      Buttons([
                        Btn('None (root)', dense: true, active: _parentId == null,
                            onTap: _busy ? null : () => setState(() => _parentId = null)),
                        for (final r in roots)
                          Btn(r.name, dense: true, active: _parentId == r.id,
                              onTap: _busy ? null : () => setState(() => _parentId = r.id)),
                      ]),
                    ],
                    if (_parentId == null) ...[
                      const SizedBox(height: 12),
                      const Label('Directory path on hub'),
                      const SizedBox(height: 4),
                      Field(controller: _path, mono: true,
                          hint: '~/github/my-project (blank for envoy default)',
                          onSubmit: (_) => unawaited(_submit())),
                      const SizedBox(height: 6),
                      Buttons([
                        Btn('Existing path', dense: true, active: !_newDir,
                            onTap: _busy ? null : () => setState(() => _newDir = false)),
                        Btn('Create directory', dense: true, active: _newDir,
                            onTap: _busy ? null : () => setState(() => _newDir = true)),
                      ]),
                    ],
                    const SizedBox(height: 12),
                    const Label('Harness'),
                    const SizedBox(height: 6),
                    ValueListenableBuilder(
                      valueListenable: _name,
                      builder: (context, name, _) {
                        final isEnvoy = _parentId == null && name.text.trim() == 'envoy';
                        final activeHarness = isEnvoy ? _harness ?? Harness.claude : _harness;
                        return Buttons([
                          if (!isEnvoy)
                            Btn('Auto', dense: true, active: _harness == null,
                                onTap: _busy ? null : () => setState(() => _harness = null)),
                          for (final h in _harnessChoices)
                            Btn(harnessName(h), dense: true, active: activeHarness == h,
                                onTap: _busy ? null : () => setState(() => _harness = h)),
                        ]);
                      },
                    ),
                    const SizedBox(height: 12),
                    const Label('Initial prompt (optional)'),
                    const SizedBox(height: 4),
                    Field(controller: _message, hint: 'What should it start working on?',
                        onSubmit: (_) => unawaited(_submit())),
                    if (_error != null) ...[
                      const SizedBox(height: 10),
                      T(_error!, color: p.alarm, maxLines: 3),
                    ],
                    const SizedBox(height: 16),
                    Buttons([
                      Btn(_busy ? 'Starting...' : 'Spawn sleeve', kind: BtnKind.primary,
                          onTap: _busy ? null : () => unawaited(_submit())),
                      Btn('Cancel', kind: BtnKind.ghost,
                          onTap: _busy ? null : () => Navigator.of(context).pop()),
                    ]),
                  ],
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}

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
  return '$working working, $blocked blocked';
}

void _open(AppScope app, Sleeve s) =>
    s.role == SleeveRole.envoy ? app.go(Screen.envoy) : app.openSleeve(s.id);

/// The workspaces tree: the sidebar on desktop and the Sleeves screen on the phone.
class SleeveTree extends StatelessWidget {
  const SleeveTree({super.key, this.selected, this.onOpen, this.compact = false});

  final String? selected;
  final void Function(String sleeveId)? onOpen;

  /// The sidebar form: status as a dot, the word only when it needs you.
  final bool compact;

  @override
  Widget build(BuildContext context) {
    final app = AppScope.of(context);
    final p = pal(context);
    final rows = treeRows(app.state);
    if (rows.isEmpty) {
      if (compact) return const EmptyState('No workspaces yet', compact: true);
      return Column(
        mainAxisAlignment: MainAxisAlignment.center,
        children: [
          const EmptyState('No sleeves yet', glyph: Glyph.sleeves),
          Btn('New sleeve', kind: BtnKind.primary,
              onTap: () => unawaited(showSpawnSleeveDialog(context, app))),
        ],
      );
    }
    return ListView.builder(
      padding: EdgeInsets.fromLTRB(8, 0, 8, compact ? 8 : 16),
      itemCount: rows.length,
      itemBuilder: (c, i) {
        final (w, s, last) = rows[i];
        if (s == null) {
          final n = app.state.sleeves.where((x) => x.workspaceId == w.id).length;
          return Padding(
            padding: EdgeInsets.fromLTRB(12, i == 0 ? 4 : 14, 8, 4),
            child: Row(children: [
              Expanded(child: T(w.name, color: p.faint, weight: FontWeight.w500, size: 11.5)),
              T('$n', color: p.faint, size: 11),
            ]),
          );
        }
        return _SleeveRow(
          sleeve: s,
          child: s.parentId != null,
          last: last,
          selected: s.id == selected,
          compact: compact,
          onTap: () => onOpen == null ? _open(app, s) : onOpen!(s.id),
        );
      },
    );
  }
}

class _SleeveRow extends StatelessWidget {
  const _SleeveRow({
    required this.sleeve,
    required this.child,
    required this.last,
    required this.selected,
    required this.compact,
    required this.onTap,
  });

  final Sleeve sleeve;
  final bool child;
  final bool last;
  final bool selected;
  final bool compact;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final p = pal(context);
    final s = sleeve;
    final st = sleeveStatus(p, s);
    final h = compact ? 28.0 : 36.0;
    return Hover(
      onTap: onTap,
      tooltip: compact ? '${s.name}: ${st.word}' : null,
      builder: (c, hot, down, focus) => Container(
        height: h,
        padding: const EdgeInsets.only(left: 8, right: 8),
        decoration: BoxDecoration(
          color: selected ? p.tint(p.accent, 0.12) : (down ? p.press : hot ? p.hover : null),
          borderRadius: BorderRadius.circular(kRadius),
          border: focus ? Border.all(color: p.accent) : null,
        ),
        child: Row(children: [
          const SizedBox(width: 4),
          if (child) Branch(last: last, height: h),
          StatusDot(st),
          const SizedBox(width: 9),
          Expanded(
            child: T(s.name,
                color: selected ? p.fg : (hot ? p.fg : p.dim),
                weight: selected ? FontWeight.w500 : FontWeight.w400),
          ),
          if (compact && st.urgent)
            Pill(st.word, color: st.color, filled: true)
          else if (compact)
            T(harnessName(s.incarnation?.harness ?? Harness.unknown), color: p.faint, size: 11, mono: true)
          else ...[
            T(harnessName(s.incarnation?.harness ?? Harness.unknown), color: p.faint, size: kSmall, mono: true),
            const SizedBox(width: 10),
            StatusPill(st),
          ],
        ]),
      ),
    );
  }
}

/// The Sleeves screen on desktop: every sleeve with its incarnation in columns.
class SleevesTable extends StatelessWidget {
  const SleevesTable({super.key});

  @override
  Widget build(BuildContext context) {
    final app = AppScope.of(context);
    final p = pal(context);
    final rows = treeRows(app.state).where((r) => r.$2 != null).toList();
    if (rows.isEmpty) {
      return Column(
        mainAxisAlignment: MainAxisAlignment.center,
        children: [
          const EmptyState('No sleeves yet', glyph: Glyph.sleeves),
          Btn('New sleeve', kind: BtnKind.primary,
              onTap: () => unawaited(showSpawnSleeveDialog(context, app))),
        ],
      );
    }
    return Wide(
      min: 1000,
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 12),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            TableHead([
              const Expanded(child: Text('Name')),
              headCell('Workspace', 150),
              headCell('Harness', 96),
              headCell('Model', 190),
              headCell('Gen', 44, right: true),
              const SizedBox(width: 24),
              headCell('Lifecycle', 104),
              headCell('Activity', 130),
              headCell('Since', 76, right: true),
              headCell('Context', 76, right: true),
            ]),
            Expanded(
              child: Tick(builder: (c, now) => ListView.builder(
                itemCount: rows.length,
                itemBuilder: (c, i) {
                  final (w, s, last) = rows[i];
                  final x = s!;
                  final inc = x.incarnation;
                  final st = sleeveStatus(p, x);
                  final act = inc?.activity;
                  return ListRow(
                    onTap: () => _open(app, x),
                    children: [
                      Expanded(child: Row(children: [
                        if (x.parentId != null) Branch(last: last, height: 38),
                        StatusDot(st),
                        const SizedBox(width: 10),
                        Flexible(child: T(x.name, weight: FontWeight.w500)),
                        if (x.role == SleeveRole.envoy) ...[
                          const SizedBox(width: 8),
                          Pill('envoy', color: p.accent),
                        ],
                      ])),
                      Cell(150, child: T(w.name, color: p.dim)),
                      Cell(96, child: T(harnessName(inc?.harness ?? Harness.unknown), mono: true, size: 12)),
                      Cell(190, child: T(inc?.model ?? '-', mono: true, size: 12, color: p.dim)),
                      Cell(44, right: true, child: T('${inc?.g ?? '-'}', color: p.dim)),
                      const SizedBox(width: 24),
                      Cell(104, child: T(cap(x.lifecycle.wire),
                          color: x.lifecycle == Lifecycle.running ? p.dim : st.color)),
                      Cell(130, child: Align(alignment: Alignment.centerLeft, child: StatusPill(st))),
                      Cell(76, right: true, child: T(act == null ? '-' : span(now.difference(act.since)), color: p.dim)),
                      Cell(76, right: true, child: T(inc == null ? '-' : tokens(inc.contextTokens), color: p.dim)),
                    ],
                  );
                },
              )),
            ),
          ],
        ),
      ),
    );
  }
}

/// A tree connector drawn as hairlines so it joins across rows.
class Branch extends StatelessWidget {
  const Branch({super.key, required this.last, required this.height});

  final bool last;
  final double height;

  @override
  Widget build(BuildContext context) => CustomPaint(
    size: Size(18, height),
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
      ..strokeWidth = 1
      ..style = PaintingStyle.stroke;
    const x = 3.5;
    final y = (size.height / 2).floorToDouble() + 0.5;
    final path = Path()
      ..moveTo(x, -2)
      ..lineTo(x, y - 4)
      ..quadraticBezierTo(x, y, x + 4, y)
      ..lineTo(size.width - 4, y);
    canvas.drawPath(path, paint);
    if (!last) canvas.drawLine(Offset(x, y - 4), Offset(x, size.height + 2), paint);
  }

  @override
  bool shouldRepaint(_BranchPainter old) => old.color != color || old.last != last;
}
