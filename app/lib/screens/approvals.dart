import 'package:flutter/material.dart';

import '../chat.dart';
import '../api/api.g.dart';
import '../derived.dart';
import '../icons.dart';
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
    return Align(
      alignment: Alignment.topLeft,
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 900),
        child: ListView(
          padding: const EdgeInsets.symmetric(vertical: 20),
          children: [
            if (pending.isEmpty)
              const EmptyState('Nothing needs you', glyph: Glyph.approvals,
                  detail: 'Approvals from Envoy and the sleeves show up here and in their chats.'),
            for (final a in pending)
              Padding(
                padding: const EdgeInsets.only(bottom: 12),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    _From(a: a),
                    const SizedBox(height: 6),
                    ApprovalBlock(approvalId: a.id),
                  ],
                ),
              ),
            Section('Recent decisions', trailing: T('${done.length}', color: p.faint, size: kSmall)),
            if (done.isEmpty) const EmptyState('No decisions yet', compact: true),
            for (final a in done.take(20))
              Container(
                padding: const EdgeInsets.symmetric(vertical: 10),
                decoration: BoxDecoration(border: Border(top: BorderSide(color: p.line))),
                child: ApprovalBlock(approvalId: a.id),
              ),
          ],
        ),
      ),
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
    final name = s?.name ?? a.sleeveId;
    return Row(children: [
      T('From', color: p.faint, size: kSmall),
      const SizedBox(width: 6),
      Hover(
        onTap: s == null
            ? null
            : () => s.role == SleeveRole.envoy ? app.go(Screen.envoy) : app.openSleeve(s.id),
        tooltip: s == null ? null : 'Open $name',
        builder: (c, hot, down, focus) => Text(name,
            style: ts(c, color: p.info, size: kSmall, weight: FontWeight.w500).copyWith(
                decoration: hot ? TextDecoration.underline : null, decorationColor: p.info)),
      ),
      const SizedBox(width: 6),
      T('at ${hhmm(a.requestedAt)}', color: p.faint, size: kSmall),
    ]);
  }
}
