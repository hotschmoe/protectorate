import 'package:flutter/material.dart';

import '../model.dart';
import '../scope.dart';
import '../theme.dart';
import '../widgets.dart';

(String, Color) presenceWord(Palette p, Presence pr) {
  final idle = pr.idleSecs == null ? '' : ' ${span(Duration(seconds: pr.idleSecs!))}';
  return switch (pr.state) {
    PresenceState.present => ('PRESENT$idle', p.lit),
    PresenceState.away => ('AWAY$idle', p.ok),
    PresenceState.unknown => ('UNKNOWN', p.dim),
  };
}

(String, Color) onlineWord(Palette p, Node n, DateTime now) => n.online
    ? ('ONLINE', p.ok)
    : ('OFFLINE ${span(now.difference(n.lastSeen))}', p.alarm);

String gpuShort(Node n) {
  if (n.inventory.gpus.isEmpty) return '-';
  final g = n.inventory.gpus.first;
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
    final phone = Term.of(context).phone;
    final nodes = [...app.state.nodes]..sort(nodeOrder);
    if (nodes.isEmpty) return T('(none)', color: p.dim);
    if (phone) {
      return Tick(builder: (c, now) => ListView(children: [
        for (final n in nodes) _PhoneNodeRow(node: n, now: now),
      ]));
    }
    Widget head(String t, int n, {bool right = false}) =>
        Cells(n, right: right, child: T(t, color: p.dim));
    return Wide(
      cells: 126,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(children: [
            head('NAME', 12), head('ARCH', 7), head('OS', 9), head('CPU', 5, right: true),
            head('RAM', 6, right: true), const Cells(2, child: SizedBox()), head('GPU', 22),
            head('DISK /', 17), head('LOAD', 19), head('PRESENCE', 15),
            Expanded(child: T('STATE', color: p.dim)),
          ]),
          Container(height: 1, color: p.edge),
          Expanded(
            child: Tick(builder: (c, now) => ListView.builder(
              itemCount: nodes.length,
              itemBuilder: (c, i) {
                final n = nodes[i];
                final (pw, pc) = presenceWord(p, n.presence);
                final (ow, oc) = onlineWord(p, n, now);
                final inv = n.inventory;
                return GestureDetector(
                  behavior: HitTestBehavior.opaque,
                  onTap: () => app.openNode(n.id),
                  child: MouseRegion(
                    cursor: SystemMouseCursors.click,
                    child: Container(
                      color: i.isOdd ? p.bar : null,
                      child: Row(children: [
                        Cells(12, child: T(n.name, color: p.lit)),
                        Cells(7, child: T(inv.arch)),
                        Cells(9, child: T(inv.os, color: p.dim)),
                        Cells(5, right: true, child: T('${inv.cpuCount}c')),
                        Cells(6, right: true, child: T(bytes(inv.ramBytes))),
                        const Cells(2, child: SizedBox()),
                        Cells(22, child: T(gpuShort(n), color: p.dim)),
                        Cells(17, child: Meter(inv.disks.isEmpty ? null : inv.disks.first.usedFraction, cells: 8)),
                        Cells(19, child: Meter(cpuLoad(n), cells: 10)),
                        Cells(15, child: T(pw, color: pc)),
                        Expanded(child: T(ow, color: oc)),
                      ]),
                    ),
                  ),
                );
              },
            )),
          ),
        ],
      ),
    );
  }
}

class _PhoneNodeRow extends StatelessWidget {
  const _PhoneNodeRow({required this.node, required this.now});

  final Node node;
  final DateTime now;

  @override
  Widget build(BuildContext context) {
    final app = AppScope.of(context);
    final p = pal(context);
    final n = node;
    final (pw, pc) = presenceWord(p, n.presence);
    final (ow, oc) = onlineWord(p, n, now);
    return GestureDetector(
      behavior: HitTestBehavior.opaque,
      onTap: () => app.openNode(n.id),
      child: Padding(
        padding: const EdgeInsets.only(bottom: 6),
        child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Row(children: [
            Expanded(child: Line([
              sp(context, n.name, color: p.lit),
              sp(context, '  ${n.inventory.arch} ${n.inventory.os}', color: p.dim),
            ])),
            T(ow, color: oc),
          ]),
          Row(children: [
            T('load ', color: p.dim),
            Meter(cpuLoad(n), cells: 10),
            const Spacer(),
            T(pw, color: pc),
          ]),
        ]),
      ),
    );
  }
}

/// The compact NODES pane in the desktop left column.
class NodesMini extends StatelessWidget {
  const NodesMini({super.key});

  @override
  Widget build(BuildContext context) {
    final app = AppScope.of(context);
    final p = pal(context);
    final nodes = [...app.state.nodes]..sort(nodeOrder);
    return Tick(builder: (c, now) => Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        for (final n in nodes)
          GestureDetector(
            behavior: HitTestBehavior.opaque,
            onTap: () => app.openNode(n.id),
            child: MouseRegion(
              cursor: SystemMouseCursors.click,
              child: Row(children: [
                Cells(11, child: T(n.name, color: n.online ? p.fg : p.alarm)),
                Cells(7, child: T(n.inventory.arch, color: p.dim)),
                Cells(15, child: Meter(cpuLoad(n), cells: 6)),
                Expanded(child: n.online
                    ? T(presenceWord(p, n.presence).$1.split(' ').first, color: presenceWord(p, n.presence).$2)
                    : T('OFFLINE', color: p.alarm)),
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
    final n = app.state.node(nodeId);
    if (n == null) return T('(no node $nodeId)', color: p.dim);
    final inv = n.inventory;
    final jobs = app.state.jobs.where((j) => j.nodeId == n.id).toList()
      ..sort((a, b) => b.queuedAt.compareTo(a.queuedAt));
    Widget kv(String k, Widget v) => Row(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [Cells(14, child: T(k, color: p.dim)), Expanded(child: v)],
    );
    return Tick(builder: (c, now) {
      final (pw, pc) = presenceWord(p, n.presence);
      final (ow, oc) = onlineWord(p, n, now);
      final pr = n.presence;
      return ListView(children: [
        Actions3270([Btn('BACK', color: p.dim, onTap: () => app.go(Screen.nodes))]),
        const HRule(label: 'INVENTORY'),
        kv('HOST', T('${inv.hostname}  ${inv.os} ${inv.osVersion}')),
        kv('ARCH', T(inv.arch)),
        kv('CPU', T('${inv.cpuModel}  ${inv.cpuCount} threads')),
        kv('RAM', T(bytes(inv.ramBytes))),
        kv('NODE', T('${n.nodeVersion}  inventory ${span(now.difference(inv.collectedAt))} ago', color: p.dim)),
        kv('STATE', T(ow, color: oc)),
        const HRule(label: 'SAMPLE'),
        if (n.sample == null || !n.online)
          T('(no current sample)', color: p.dim)
        else ...[
          kv('CPU', Meter(cpuLoad(n))),
          kv('LOAD 1M', T(n.sample!.load1?.toStringAsFixed(2) ?? '-')),
          kv('MEM', Row(children: [
            Meter(n.memUsedFraction),
            T('  ${bytes(n.sample!.memAvailBytes)} free of ${bytes(inv.ramBytes)}', color: p.dim),
          ])),
        ],
        const HRule(label: 'PRESENCE'),
        kv('STATE', T(pw, color: pc)),
        kv('LOCKED', T(pr.locked == null ? 'unknown' : (pr.locked! ? 'yes' : 'no'))),
        kv('SOURCE', T(pr.source.isEmpty ? '-' : pr.source)),
        kv('JOBS', T(switch (pr.state) {
          PresenceState.away => 'takes jobs',
          PresenceState.present => 'refuses jobs (someone is at it)',
          PresenceState.unknown => 'refuses jobs (UNKNOWN counts as present)',
        }, color: p.dim)),
        const HRule(label: 'DISKS'),
        for (final d in inv.disks)
          kv(d.mount, Row(children: [
            Meter(d.usedFraction),
            T('  ${bytes(d.freeBytes)} free of ${bytes(d.sizeBytes)}', color: p.dim),
          ])),
        const HRule(label: 'GPUS'),
        if (inv.gpus.isEmpty) T('(none)', color: p.dim),
        for (final g in inv.gpus)
          kv(g.driver.isEmpty ? '-' : g.driver.split(' ').first, Line([
            sp(context, g.name),
            if (g.vramBytes != null) sp(context, '  ${bytes(g.vramBytes)} VRAM', color: p.dim),
            if (g.utilPct != null && n.online) sp(context, '  util ${g.utilPct!.round()}%', color: p.dim),
          ])),
        const HRule(label: 'TOOLCHAINS'),
        Wrap(spacing: kCell * 3, children: [
          for (final t in inv.toolchains)
            Line([sp(context, t.name, color: p.dim), sp(context, ' ${t.version}')]),
        ]),
        HRule(label: 'RECENT JOBS', status: '${jobs.length}'),
        if (jobs.isEmpty) T('(none)', color: p.dim),
        for (final j in jobs) _JobRow(job: j, now: now),
      ]);
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
      _ => p.dim,
    };
    final word = switch (j.state) {
      JobState.submitUncertain => 'SUBMIT?',
      _ => j.state.name.toUpperCase(),
    };
    final start = j.startedAt;
    final dur = start == null ? '-' : span((j.endedAt ?? now).difference(start));
    final phone = Term.of(context).phone;
    return Row(children: [
      Cells(8, child: T(j.id, color: p.lit)),
      Cells(11, child: T(word, color: color)),
      Expanded(child: T(j.command)),
      if (!phone) Cells(14, child: T(j.requestedBy, color: p.dim)),
      Cells(6, right: true, child: T(j.exitCode == null ? '' : 'rc ${j.exitCode}', color: p.dim)),
      Cells(8, right: true, child: T(dur, color: p.dim)),
      if (!phone) Cells(7, right: true, child: T(hhmm(j.endedAt ?? j.queuedAt), color: p.rule)),
    ]);
  }
}
