import 'package:flutter/material.dart';

import '../gen/protectorate/v1/node.pb.dart';
import '../gen/protectorate/v1/sleeve.pb.dart';
import '../theme/tokens.dart';
import '../vfd/meter.dart';

String formatBytes(num bytes) {
  const units = ['B', 'KB', 'MB', 'GB', 'TB', 'PB'];
  var v = bytes.toDouble();
  var i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  final digits = i == 0 || v >= 100 ? 0 : (v >= 10 ? 1 : 2);
  return '${v.toStringAsFixed(digits)} ${units[i]}';
}

String formatAge(Duration d) {
  if (d.isNegative || d.inSeconds < 1) return 'now';
  if (d.inSeconds < 90) return '${d.inSeconds}s ago';
  if (d.inMinutes < 90) return '${d.inMinutes}m ago';
  if (d.inHours < 48) return '${d.inHours}h ago';
  return '${d.inDays}d ago';
}

String formatUptime(int seconds) {
  final d = Duration(seconds: seconds);
  if (d.inDays > 0) return '${d.inDays}d ${d.inHours % 24}h';
  if (d.inHours > 0) return '${d.inHours}h ${d.inMinutes % 60}m';
  return '${d.inMinutes}m';
}

String formatPercent(double v) => '${v.toStringAsFixed(v >= 100 ? 0 : 1)}%';

Duration statusAge(NodeStatus s, DateTime receivedAt, DateTime now) {
  final base = s.hasAge()
      ? Duration(
          seconds: s.age.seconds.toInt(),
          microseconds: s.age.nanos ~/ 1000,
        )
      : Duration.zero;
  final since = now.difference(receivedAt);
  return base + (since.isNegative ? Duration.zero : since);
}

FilesystemMetrics? worstFilesystem(NodeMetrics m) {
  FilesystemMetrics? w;
  for (final f in m.filesystems) {
    if (w == null || f.usedPercent > w.usedPercent) w = f;
  }
  return w;
}

class GpuSummary {
  const GpuSummary({
    required this.count,
    required this.utilPercent,
    required this.vramUsed,
    required this.vramTotal,
  });

  final int count;
  final double? utilPercent;
  final int vramUsed;
  final int vramTotal;

  double? get vramPercent => vramTotal > 0 ? vramUsed * 100 / vramTotal : null;
}

GpuSummary? summarizeGpus(NodeMetrics m) {
  if (m.gpus.isEmpty) return null;
  double? util;
  var used = 0;
  var total = 0;
  for (final g in m.gpus) {
    if (g.utilizationPercent >= 0 &&
        (util == null || g.utilizationPercent > util)) {
      util = g.utilizationPercent;
    }
    used += g.memoryUsedBytes.toInt();
    total += g.memoryTotalBytes.toInt();
  }
  return GpuSummary(
    count: m.gpus.length,
    utilPercent: util,
    vramUsed: used,
    vramTotal: total,
  );
}

enum OffenderKind { container, engine, watchedDir, sleeve }

class Offender {
  const Offender(this.kind, this.label, this.bytes, [this.detail = '']);

  final OffenderKind kind;
  final String label;
  final int bytes;
  final String detail;
}

List<Offender> topOffenders(NodeMetrics m, {int limit = 8}) {
  final out = <Offender>[];
  var perContainer = false;
  for (final c in m.engine.containers) {
    final b = c.writableLayerBytes.toInt();
    if (b > 0) {
      perContainer = true;
      out.add(Offender(OffenderKind.container, c.name, b, 'writable layer'));
    }
  }
  if (m.engine.hasDisk()) {
    final d = m.engine.disk;
    void bucket(String label, DiskBucket b) {
      final size = b.sizeBytes.toInt();
      if (size > 0) {
        out.add(
          Offender(
            OffenderKind.engine,
            label,
            size,
            '${b.reclaimableBytes.toInt() > 0 ? '${formatBytes(b.reclaimableBytes.toInt())} reclaimable, ' : ''}${b.count} items',
          ),
        );
      }
    }

    bucket('images', d.images);
    bucket('build cache', d.buildCache);
    bucket('volumes', d.volumes);
    if (!perContainer) bucket('containers', d.containers);
  }
  for (final s in m.sleeves) {
    final b = s.usage.workspaceDiskBytes.toInt();
    if (b > 0) {
      out.add(Offender(OffenderKind.sleeve, s.name, b, 'workspace'));
    }
  }
  for (final w in m.watchDirs) {
    final size = w.sizeBytes.toInt();
    if (size > 0) {
      out.add(
        Offender(
          OffenderKind.watchedDir,
          w.path,
          size,
          w.complete ? '' : 'partial, lower bound',
        ),
      );
    }
  }
  out.sort((a, b) => b.bytes.compareTo(a.bytes));
  return out.take(limit).toList();
}

List<ContainerMetrics> containersBySize(NodeMetrics m) {
  final list = [...m.engine.containers];
  list.sort((a, b) {
    final c = b.writableLayerBytes.compareTo(a.writableLayerBytes);
    return c != 0 ? c : b.memoryUsedBytes.compareTo(a.memoryUsedBytes);
  });
  return list;
}

List<Sleeve> sleevesBySize(NodeMetrics m) {
  final list = [...m.sleeves];
  list.sort(
    (a, b) => b.usage.workspaceDiskBytes.compareTo(a.usage.workspaceDiskBytes),
  );
  return list;
}

class AlertBadge {
  const AlertBadge(this.label, this.critical);

  final String label;
  final bool critical;
}

List<AlertBadge> alertBadges(NodeStatus s) {
  final out = <AlertBadge>[];
  for (final a in s.alerts) {
    switch (a.kind) {
      case AlertKind.ALERT_KIND_DISK_PERCENT:
        out.add(
          AlertBadge(
            'DISK ${a.value.round()}%${a.subject.isEmpty ? '' : ' ${a.subject}'}',
            a.value >= critPercent,
          ),
        );
      case AlertKind.ALERT_KIND_MEM_PRESSURE:
        out.add(AlertBadge('MEM ${a.value.round()}%', a.value >= critPercent));
      case AlertKind.ALERT_KIND_UNREACHABLE:
        out.add(const AlertBadge('DOWN', true));
      default:
        if (a.message.isNotEmpty) out.add(AlertBadge(a.message, false));
    }
  }
  return out;
}

String harnessLabel(Harness h) =>
    h.name.replaceFirst('HARNESS_', '').toLowerCase();

// Samples a client has seen while watching, per node; the hub keeps no
// history, so graphs start empty and fill as polls arrive.
class NodeHistory {
  NodeHistory({this.capacity = 72});

  final int capacity;
  final _cpu = <String, List<double?>>{};
  final _mem = <String, List<double?>>{};
  final _last = <String, DateTime>{};

  void record(NodeStatus s) {
    if (!s.hasMetrics() ||
        s.freshness == NodeFreshness.NODE_FRESHNESS_UNREACHABLE) {
      return;
    }
    final name = s.node.name;
    final m = s.metrics;
    if (m.hasCollectedAt()) {
      final at = m.collectedAt.toDateTime();
      if (_last[name] == at) return;
      _last[name] = at;
    }
    void add(Map<String, List<double?>> into, double v) {
      final list = into.putIfAbsent(name, () => []);
      list.add(v);
      if (list.length > capacity) list.removeAt(0);
    }

    add(_cpu, m.cpu.usagePercent);
    add(_mem, m.memory.usedPercent);
  }

  void recordAll(Iterable<NodeStatus> nodes) => nodes.forEach(record);

  List<double?> cpu(String name) => List.unmodifiable(_cpu[name] ?? const []);
  List<double?> mem(String name) => List.unmodifiable(_mem[name] ?? const []);
}

class SleeveRow extends StatelessWidget {
  const SleeveRow({super.key, required this.sleeve, this.dim = false});

  final Sleeve sleeve;
  final bool dim;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final u = sleeve.usage;
    final exited = sleeve.state == SleeveState.SLEEVE_STATE_EXITED;
    final limit = sleeve.limits.memoryBytes.toInt();
    final memPct = limit > 0
        ? u.memoryCurrentBytes.toInt() * 100 / limit
        : null;
    final accent = t.harness(sleeve.harness).fg;
    return Padding(
      key: ValueKey('sleeve-${sleeve.name}'),
      padding: const EdgeInsets.symmetric(vertical: 6),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Expanded(
                child: Text.rich(
                  TextSpan(
                    children: [
                      TextSpan(text: sleeve.name, style: t.data(t.p.emph)),
                      TextSpan(
                        text: '  ${harnessLabel(sleeve.harness).toUpperCase()}',
                        style: t.legend(accent),
                      ),
                      if (exited)
                        TextSpan(text: '  EXITED', style: t.legend(t.p.dim)),
                    ],
                  ),
                  overflow: TextOverflow.ellipsis,
                ),
              ),
              Text(
                'ws ${u.workspaceDiskBytes > 0 ? formatBytes(u.workspaceDiskBytes.toInt()) : '-'}',
                style: t.small(),
              ),
            ],
          ),
          if (!exited) ...[
            const SizedBox(height: 2),
            MeterRow(
              label: 'CPU',
              percent: sleeve.limits.cpuPercent > 0
                  ? u.cpuPercent * 100 / sleeve.limits.cpuPercent
                  : null,
              value:
                  '${u.cpuPercent.toStringAsFixed(0)}%${sleeve.limits.cpuPercent > 0 ? ' / ${sleeve.limits.cpuPercent}%' : ''}',
              dim: dim,
              valueChars: 17,
            ),
            MeterRow(
              label: 'MEM',
              percent: memPct,
              value:
                  '${formatBytes(u.memoryCurrentBytes.toInt())}${limit > 0 ? ' / ${formatBytes(limit)}' : ''}',
              dim: dim,
              valueChars: 17,
            ),
            Text(
              'peak ${formatBytes(u.memoryPeakBytes.toInt())}  ${u.pids} pids${u.oomKills > 0 ? '  ${u.oomKills} OOM kills' : ''}',
              style: t.small(u.oomKills > 0 ? t.p.alarm : null),
            ),
          ],
        ],
      ),
    );
  }
}
