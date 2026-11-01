import 'package:flutter/material.dart';

import '../chat.dart';
import '../api/api.g.dart';
import '../derived.dart';
import '../scope.dart';
import '../theme.dart';
import '../widgets.dart';

/// Everything pending, from every sleeve and Envoy, oldest first; recent decisions below.
class ApprovalsScreen extends StatelessWidget {
  const ApprovalsScreen({super.key});

  @override
  Widget build(BuildContext context) {
    final app = AppScope.of(context);
    final p = pal(context);
    final pending = app.state.pending..sort((a, b) => a.requestedAt.compareTo(b.requestedAt));
    final done = app.state.approvals.where((a) => a.state != ApprovalState.pending).toList()
      ..sort((a, b) => (b.decidedAt ?? b.requestedAt).compareTo(a.decidedAt ?? a.requestedAt));
    return ListView(
      children: [
        if (pending.isEmpty) T('(none pending)', color: p.dim),
        for (final a in pending)
          Padding(
            padding: const EdgeInsets.only(bottom: 8),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                _From(a: a),
                ApprovalBlock(approvalId: a.id),
              ],
            ),
          ),
        const HRule(label: 'RECENT'),
        if (done.isEmpty) T('(none)', color: p.dim),
        for (final a in done.take(20)) ApprovalBlock(approvalId: a.id),
      ],
    );
  }
}

class _From extends StatelessWidget {
  const _From({required this.a});

  final Approval a;

  @override
  Widget build(BuildContext context) {
    final app = AppScope.of(context);
    final p = pal(context);
    final s = app.state.sleeve(a.sleeveId);
    return Row(children: [
      T('${hhmm(a.requestedAt)} ', color: p.rule),
      Btn(s?.name ?? a.sleeveId, brackets: false, color: p.info, onTap: s == null
          ? null
          : () => s.role == SleeveRole.envoy ? app.go(Screen.envoy) : app.openSleeve(s.id)),
    ]);
  }
}
