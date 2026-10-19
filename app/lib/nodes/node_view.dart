import 'package:flutter/material.dart';

import '../gen/protectorate/v1/node.pb.dart';
import '../gen/protectorate/v1/sleeve.pb.dart';

const warnPercent = 75.0;
const critPercent = 90.0;

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

class Severity {
  const Severity(this.fill, this.container, this.onContainer);

  final Color fill;
  final Color container;
  final Color onContainer;
}

Severity severityFor(BuildContext context, double? percent) {
  final cs = Theme.of(context).colorScheme;
  final dark = Theme.of(context).brightness == Brightness.dark;
  if (percent != null && percent >= critPercent) {
    return Severity(cs.error, cs.errorContainer, cs.onErrorContainer);
  }
  if (percent != null && percent >= warnPercent) {
    return dark
        ? const Severity(
            Color(0xFFFFB74D),
            Color(0xFF5C3A00),
            Color(0xFFFFE0B2),
          )
        : const Severity(
            Color(0xFFB26A00),
            Color(0xFFFFE0B2),
            Color(0xFF4A2C00),
          );
  }
  return Severity(cs.primary, cs.secondaryContainer, cs.onSecondaryContainer);
}

class PercentBar extends StatelessWidget {
  const PercentBar({
    super.key,
    required this.label,
    required this.percent,
    required this.trailing,
    this.dim = false,
    this.labelWidth = 44,
  });

  final String label;
  final double? percent;
  final String trailing;
  final bool dim;
  final double labelWidth;

  @override
  Widget build(BuildContext context) {
    final cs = Theme.of(context).colorScheme;
    final sev = severityFor(context, percent);
    final style = Theme.of(context).textTheme.bodySmall!;
    final fg = dim ? cs.onSurfaceVariant.withValues(alpha: 0.7) : cs.onSurface;
    final fill = dim ? cs.outline : sev.fill;
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 2),
      child: Row(
        children: [
          SizedBox(
            width: labelWidth,
            child: Text(
              label,
              style: style.copyWith(color: cs.onSurfaceVariant),
            ),
          ),
          Expanded(
            child: ClipRRect(
              borderRadius: BorderRadius.circular(3),
              child: SizedBox(
                height: 8,
                child: LinearProgressIndicator(
                  value: ((percent ?? 0) / 100).clamp(0.0, 1.0),
                  color: fill,
                  backgroundColor: cs.surfaceContainerHighest,
                ),
              ),
            ),
          ),
          const SizedBox(width: 8),
          Flexible(
            flex: 0,
            child: ConstrainedBox(
              constraints: const BoxConstraints(minWidth: 96),
              child: Text(
                trailing,
                textAlign: TextAlign.end,
                overflow: TextOverflow.ellipsis,
                style: style.copyWith(
                  color: fg,
                  fontWeight: (percent ?? 0) >= critPercent && !dim
                      ? FontWeight.w700
                      : null,
                  fontFeatures: const [FontFeature.tabularFigures()],
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }
}

class StatusBadge extends StatelessWidget {
  const StatusBadge({super.key, required this.text, required this.sev});

  final String text;
  final Severity sev;

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
      decoration: BoxDecoration(
        color: sev.container,
        borderRadius: BorderRadius.circular(4),
      ),
      child: Text(
        text,
        style: Theme.of(context).textTheme.labelSmall!.copyWith(
          color: sev.onContainer,
          fontWeight: FontWeight.w700,
        ),
      ),
    );
  }
}

String harnessLabel(Harness h) =>
    h.name.replaceFirst('HARNESS_', '').toLowerCase();

class SleeveRow extends StatelessWidget {
  const SleeveRow({super.key, required this.sleeve, this.dim = false});

  final Sleeve sleeve;
  final bool dim;

  @override
  Widget build(BuildContext context) {
    final text = Theme.of(context).textTheme;
    final cs = Theme.of(context).colorScheme;
    final u = sleeve.usage;
    final exited = sleeve.state == SleeveState.SLEEVE_STATE_EXITED;
    final limit = sleeve.limits.memoryBytes.toInt();
    final memPct = limit > 0
        ? u.memoryCurrentBytes.toInt() * 100 / limit
        : null;
    return Padding(
      key: ValueKey('sleeve-${sleeve.name}'),
      padding: const EdgeInsets.only(bottom: 8),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Expanded(
                child: Text(
                  '${sleeve.name}  ${harnessLabel(sleeve.harness)}${exited ? '  (exited)' : ''}',
                  overflow: TextOverflow.ellipsis,
                ),
              ),
              Text(
                'ws ${u.workspaceDiskBytes > 0 ? formatBytes(u.workspaceDiskBytes.toInt()) : '-'}',
                style: text.bodySmall,
              ),
            ],
          ),
          if (!exited) ...[
            PercentBar(
              label: 'CPU',
              percent: sleeve.limits.cpuPercent > 0
                  ? u.cpuPercent * 100 / sleeve.limits.cpuPercent
                  : null,
              trailing:
                  '${u.cpuPercent.toStringAsFixed(0)}%${sleeve.limits.cpuPercent > 0 ? ' / ${sleeve.limits.cpuPercent}%' : ''}',
              dim: dim,
            ),
            PercentBar(
              label: 'MEM',
              percent: memPct,
              trailing:
                  '${formatBytes(u.memoryCurrentBytes.toInt())}${limit > 0 ? ' / ${formatBytes(limit)}' : ''}',
              dim: dim,
            ),
            Text(
              'peak ${formatBytes(u.memoryPeakBytes.toInt())}  ${u.pids} pids${u.oomKills > 0 ? '  ${u.oomKills} OOM kills' : ''}',
              style: text.bodySmall?.copyWith(
                color: u.oomKills > 0 ? cs.error : cs.onSurfaceVariant,
              ),
            ),
          ],
        ],
      ),
    );
  }
}
