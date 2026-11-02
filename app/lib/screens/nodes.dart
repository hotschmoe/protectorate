import 'package:flutter/material.dart';

import '../api/api.g.dart';
import '../derived.dart';
import '../icons.dart';
import '../scope.dart';
import '../theme.dart';
import '../widgets.dart';

(String, Color) presenceWord(Palette p, Presence pr) {
  final idle = pr.idleSecs == null ? '' : ' ${span(Duration(seconds: pr.idleSecs!))}';
  return switch (pr.state) {
    PresenceState.present => ('Present$idle', p.accent),
    PresenceState.away => ('Away$idle', p.ok),
    PresenceState.unknown => ('Unknown', p.faint),
  };
}

(String, Color) onlineWord(Palette p, Node n, DateTime now) => n.online
    ? ('Online', p.ok)
    : ('Offline ${span(now.difference(n.lastSeen))}', p.alarm);

String gpuShort(Node n) {
  final gpus = n.inventory?.gpus ?? const <Gpu>[];
  if (gpus.isEmpty) return '-';
  final g = gpus.first;
  final vram = g.vramBytes == null ? '' : ' ${bytes(g.vramBytes)}';
  return '${g.name.replaceFirst('NVIDIA GeForce ', '').replaceFirst('Graphics ', '')}$vram';
}

int nodeOrder(Node a, Node b) => a.name.compareTo(b.name);

double? cpuLoad(Node n) {
  final c = n.sample?.cpuPct;
  return n.online && c != null ? c / 100 : null;
}

class NodesScreen extends StatelessWidget {
  const NodesScreen({super.key});

  @override
  Widget build(BuildContext context) {
    final app = AppScope.of(context);
    final p = pal(context);
    final phone = Look.of(context).phone;
    final nodes = [...app.state.nodes]..sort(nodeOrder);
    if (nodes.isEmpty) {
      return const EmptyState('No nodes yet', glyph: Glyph.nodes,
          detail: 'Install protectorate-node on a machine to add it.');
    }
    if (phone) {
      return Tick(builder: (c, now) => ListView.separated(
        padding: const EdgeInsets.only(bottom: 16),
        itemCount: nodes.length,
        separatorBuilder: (c, i) => const SizedBox(height: 8),
        itemBuilder: (c, i) => _PhoneNodeCard(node: nodes[i], now: now),
      ));
    }
    return Wide(
      min: 1060,
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 12),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            TableHead([
              const Expanded(child: Text('Name')),
              headCell('Arch', 70),
              headCell('OS', 80),
              headCell('CPU', 48, right: true),
              headCell('RAM', 64, right: true),
              const SizedBox(width: 20),
              headCell('GPU', 170),
              headCell('Disk /', 140),
              const SizedBox(width: 16),
              headCell('Load', 140),
              const SizedBox(width: 16),
              headCell('Presence', 110),
              headCell('State', 110),
            ]),
            Expanded(
              child: Tick(builder: (c, now) => ListView.builder(
                itemCount: nodes.length,
                itemBuilder: (c, i) {
                  final n = nodes[i];
                  final (pw, pc) = presenceWord(p, n.presence);
                  final (ow, oc) = onlineWord(p, n, now);
                  final inv = n.inventory;
                  return ListRow(
                    height: 44,
                    onTap: () => app.openNode(n.id),
                    children: [
                      Expanded(child: Row(children: [
                        Dot(n.online ? p.ok : p.alarm),
                        const SizedBox(width: 10),
                        Flexible(child: T(n.name, weight: FontWeight.w500)),
                      ])),
                      Cell(70, child: T(inv?.arch ?? '-', mono: true, size: 12, color: p.dim)),
                      Cell(80, child: T(inv?.os ?? '-', color: p.dim)),
                      Cell(48, right: true, child: T(inv == null ? '-' : '${inv.cpuCount}', color: p.dim)),
                      Cell(64, right: true, child: T(inv == null ? '-' : bytes(inv.ramBytes), color: p.dim)),
                      const SizedBox(width: 20),
                      Cell(170, child: T(gpuShort(n), color: p.dim)),
                      Cell(140, child: Meter(
                          inv == null || inv.disks.isEmpty ? null : inv.disks.first.usedFraction, width: 90)),
                      const SizedBox(width: 16),
                      Cell(140, child: Meter(cpuLoad(n), width: 90)),
                      const SizedBox(width: 16),
                      Cell(110, child: T(pw, color: pc)),
                      Cell(110, child: Align(alignment: Alignment.centerLeft,
                          child: Pill(n.online ? ow : 'Offline', color: oc, dot: true))),
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

class _PhoneNodeCard extends StatelessWidget {
  const _PhoneNodeCard({required this.node, required this.now});

  final Node node;
  final DateTime now;

  @override
  Widget build(BuildContext context) {
    final app = AppScope.of(context);
    final p = pal(context);
    final n = node;
    final (pw, pc) = presenceWord(p, n.presence);
    final (ow, oc) = onlineWord(p, n, now);
    return Hover(
      onTap: () => app.openNode(n.id),
      builder: (c, hot, down, focus) => Container(
        padding: const EdgeInsets.all(12),
        decoration: BoxDecoration(
          color: hot ? p.raised : p.surface,
          borderRadius: BorderRadius.circular(kCardRadius),
          border: Border.all(color: p.line),
        ),
        child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          Row(children: [
            Dot(n.online ? p.ok : p.alarm),
            const SizedBox(width: 8),
            Expanded(child: Line([
              sp(context, n.name, weight: FontWeight.w500),
              sp(context, '  ${n.inventory?.arch ?? '-'} ${n.inventory?.os ?? ''}', color: p.faint, size: kSmall),
            ])),
            T(ow, color: oc, size: kSmall),
          ]),
          const SizedBox(height: 10),
          Row(children: [
            const SizedBox(width: 40, child: Label('Load')),
            Expanded(child: Meter(cpuLoad(n), width: null)),
            const SizedBox(width: 16),
            T(pw, color: pc, size: kSmall),
          ]),
        ]),
      ),
    );
  }
}

/// Nodes beside Envoy's chat: name, load and presence.
class NodesMini extends StatelessWidget {
  const NodesMini({super.key});

  @override
  Widget build(BuildContext context) {
    final app = AppScope.of(context);
    final p = pal(context);
    final nodes = [...app.state.nodes]..sort(nodeOrder);
    if (nodes.isEmpty) return const EmptyState('No nodes', compact: true);
    return Tick(builder: (c, now) => Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        for (final n in nodes)
          Hover(
            onTap: () => app.openNode(n.id),
            builder: (c, hot, down, focus) => Container(
              height: 28,
              padding: const EdgeInsets.symmetric(horizontal: 8),
              decoration: BoxDecoration(
                color: hot ? p.hover : null,
                borderRadius: BorderRadius.circular(kRadius),
              ),
              child: Row(children: [
                Dot(n.online ? p.ok : p.alarm, size: 6),
                const SizedBox(width: 8),
                Expanded(child: T(n.name, color: hot ? p.fg : p.dim)),
                if (n.online) ...[
                  Meter(cpuLoad(n), width: 48),
                  const SizedBox(width: 8),
                  SizedBox(width: 56, child: T(presenceWord(p, n.presence).$1.split(' ').first,
                      color: presenceWord(p, n.presence).$2, size: kSmall)),
                ] else
                  T('Offline ${span(now.difference(n.lastSeen))}', color: p.alarm, size: kSmall),
              ]),
            ),
          ),
      ],
    ));
  }
}

class NodeDetail extends StatelessWidget {
  const NodeDetail({super.key, required this.nodeId});

  final String nodeId;

  @override
  Widget build(BuildContext context) {
    final app = AppScope.of(context);
    final p = pal(context);
    final phone = Look.of(context).phone;
    final n = app.state.node(nodeId);
    if (n == null) return EmptyState('No node $nodeId', glyph: Glyph.nodes);
    final inv = n.inventory;
    final jobs = app.state.jobs.where((j) => j.nodeId == n.id).toList()
      ..sort((a, b) => b.queuedAt.compareTo(a.queuedAt));
    return Tick(builder: (c, now) {
      final (pw, pc) = presenceWord(p, n.presence);
      final (ow, oc) = onlineWord(p, n, now);
      final pr = n.presence;
      final inventory = Panel(
        title: 'Inventory',
        trailing: inv == null ? null : T('collected ${span(now.difference(inv.collectedAt))} ago',
            color: p.faint, size: kSmall),
        child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          if (inv == null)
            const EmptyState('No inventory yet', compact: true)
          else ...[
            KV('Host', T('${inv.hostname}   ${inv.os} ${inv.osVersion}', maxLines: 2)),
            KV('Arch', T(inv.arch, mono: true, size: 12.5)),
            KV('CPU', T('${inv.cpuModel}, ${inv.cpuCount} threads', maxLines: 2)),
            KV('Memory', T(bytes(inv.ramBytes))),
          ],
          KV('Node', T(n.nodeVersion, mono: true, size: 12.5, color: p.dim)),
          KV('State', T(ow, color: oc)),
        ]),
      );
      final sample = Panel(
        title: 'Now',
        child: n.sample == null || !n.online
            ? const EmptyState('No current sample', compact: true)
            : Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
                KV('CPU', Meter(cpuLoad(n), width: 160)),
                KV('Load 1m', T(n.sample!.load1?.toStringAsFixed(2) ?? '-')),
                KV('Memory', Wrap(spacing: 12, crossAxisAlignment: WrapCrossAlignment.center, children: [
                  Meter(n.memUsedFraction, width: 160),
                  T('${bytes(n.sample!.memAvailBytes)} free of ${bytes(inv?.ramBytes)}', color: p.dim),
                ])),
              ]),
      );
      final presence = Panel(
        title: 'Presence',
        child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          KV('State', T(pw, color: pc)),
          KV('Locked', T(pr.locked == null ? 'unknown' : (pr.locked! ? 'yes' : 'no'))),
          KV('Source', T(pr.source.isEmpty ? '-' : pr.source, mono: true, size: 12.5)),
          KV('Jobs', T(switch (pr.state) {
            PresenceState.away => 'Takes jobs',
            PresenceState.present => 'Refuses jobs (someone is at it)',
            PresenceState.unknown => 'Refuses jobs (unknown counts as present)',
          }, color: p.dim, maxLines: 2)),
        ]),
      );
      final disks = Panel(
        title: 'Disks',
        child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          if (inv == null || inv.disks.isEmpty) const EmptyState('No disks reported', compact: true),
          for (final d in inv?.disks ?? const <Disk>[])
            KV(d.mount, Wrap(spacing: 12, crossAxisAlignment: WrapCrossAlignment.center, children: [
              Meter(d.usedFraction, width: 160),
              T('${bytes(d.freeBytes)} free of ${bytes(d.sizeBytes)}', color: p.dim),
            ])),
        ]),
      );
      final gpus = Panel(
        title: 'GPUs',
        child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          if (inv == null || inv.gpus.isEmpty) const EmptyState('None', compact: true),
          for (final g in inv?.gpus ?? const <Gpu>[])
            KV(g.driver.isEmpty ? '-' : g.driver.split(' ').first, Line([
              sp(context, g.name),
              if (g.vramBytes != null) sp(context, '   ${bytes(g.vramBytes)} VRAM', color: p.dim),
              if (g.utilPct != null && n.online) sp(context, '   ${g.utilPct!.round()}% busy', color: p.dim),
            ], maxLines: 2)),
        ]),
      );
      final tools = Panel(
        title: 'Toolchains',
        child: (inv?.toolchains ?? const <Toolchain>[]).isEmpty
            ? const EmptyState('None reported', compact: true)
            : Wrap(spacing: 8, runSpacing: 8, children: [
                for (final t in inv!.toolchains)
                  Container(
                    padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 3),
                    decoration: BoxDecoration(
                      color: p.raised,
                      borderRadius: BorderRadius.circular(kRadius),
                    ),
                    child: Line([
                      sp(context, t.name, size: kSmall, weight: FontWeight.w500),
                      sp(context, ' ${t.version}', color: p.dim, mono: true, size: 11.5),
                    ]),
                  ),
              ]),
      );
      final jobsPanel = Panel(
        title: 'Recent jobs',
        trailing: T('${jobs.length}', color: p.faint, size: kSmall),
        padding: const EdgeInsets.fromLTRB(16, 16, 16, 8),
        child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          if (jobs.isEmpty) const EmptyState('No jobs on this node', compact: true),
          for (final j in jobs) _JobRow(job: j, now: now),
        ]),
      );
      Widget pair(Widget a, Widget b) => phone
          ? Column(children: [a, const SizedBox(height: 12), b])
          : IntrinsicHeight(
              child: Row(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
                Expanded(child: a),
                const SizedBox(width: 12),
                Expanded(child: b),
              ]),
            );
      return ListView(
        padding: const EdgeInsets.symmetric(vertical: 20),
        children: [
          pair(inventory, sample),
          const SizedBox(height: 12),
          pair(presence, disks),
          const SizedBox(height: 12),
          pair(gpus, tools),
          const SizedBox(height: 12),
          jobsPanel,
        ],
      );
    });
  }
}

class _JobRow extends StatelessWidget {
  const _JobRow({required this.job, required this.now});

  final Job job;
  final DateTime now;

  @override
  Widget build(BuildContext context) {
    final p = pal(context);
    final j = job;
    final color = switch (j.state) {
      JobState.succeeded => p.ok,
      JobState.failed || JobState.lost || JobState.abandoned => p.alarm,
      JobState.running || JobState.collecting => p.ok,
      JobState.submitUncertain || JobState.cancelling => p.warn,
      _ => p.faint,
    };
    final word = switch (j.state) {
      JobState.submitUncertain => 'Submit?',
      _ => cap(j.state.wire),
    };
    final start = j.startedAt;
    final dur = start == null ? '-' : span((j.endedAt ?? now).difference(start));
    final phone = Look.of(context).phone;
    return Container(
      height: 34,
      decoration: BoxDecoration(border: Border(top: BorderSide(color: p.line.withValues(alpha: 0.6)))),
      child: Row(children: [
        SizedBox(width: 70, child: T(j.id, mono: true, size: 12, color: p.dim)),
        SizedBox(width: 100, child: Align(alignment: Alignment.centerLeft,
            child: Pill(word, color: color, dot: true))),
        Expanded(child: T(j.command, mono: true, size: 12)),
        if (!phone) SizedBox(width: 120, child: T(j.requestedBy, color: p.dim, size: kSmall)),
        SizedBox(width: 48, child: T(j.exitCode == null ? '' : 'rc ${j.exitCode}', color: p.dim,
            size: kSmall, align: TextAlign.right)),
        SizedBox(width: 64, child: T(dur, color: p.dim, size: kSmall, align: TextAlign.right)),
        if (!phone)
          SizedBox(width: 56, child: T(hhmm(j.endedAt ?? j.queuedAt), color: p.faint, size: kSmall,
              align: TextAlign.right)),
      ]),
    );
  }
}
