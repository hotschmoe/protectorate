import 'package:flutter/material.dart';

import '../gen/protectorate/v1/node.pb.dart';
import '../gen/protectorate/v1/sleeve.pb.dart';
import '../mail/thread_view.dart' show orchestratorSleeve;
import '../nodes/node_view.dart';
import '../screens/nodes_tab.dart';
import '../theme/tokens.dart';
import '../vfd/lamp.dart';
import '../vfd/panel.dart';

// The sleeves of one node.
class SleeveGroup {
  const SleeveGroup(this.name, this.node, this.sleeves);

  final String name;
  final NodeStatus? node;
  final List<Sleeve> sleeves;

  bool get local => node?.node.local ?? true;
}

// Fences for the hub's node first, then the others by name. A node other than
// the hub's has a fence only when it reports sleeves; hubSleeves is what the
// hub's SleeveService lists. Without a node list the hub gets one fence.
List<SleeveGroup> groupSleeves(
  List<NodeStatus> nodes,
  List<Sleeve> hubSleeves,
  String hubName,
) {
  final local = nodes.where((n) => n.node.local).firstOrNull;
  // Envoy is pinned first in the hub's fence; the rest keep the hub's order.
  final pinned = [
    ...hubSleeves.where((s) => s.name == orchestratorSleeve),
    ...hubSleeves.where((s) => s.name != orchestratorSleeve),
  ];
  final out = [
    SleeveGroup(local?.node.name ?? hubName, local, pinned),
    for (final n in nodes.where(
      (n) => !n.node.local && n.hasMetrics() && n.metrics.sleeves.isNotEmpty,
    ))
      SleeveGroup(n.node.name, n, n.metrics.sleeves),
  ];
  out.sort((a, b) {
    if (a.local != b.local) return a.local ? -1 : 1;
    return a.name.compareTo(b.name);
  });
  return out;
}

// A bordered panel holding one node's sleeves.
class NodeFence extends StatelessWidget {
  const NodeFence({
    super.key,
    required this.group,
    required this.children,
    this.onNew,
  });

  final SleeveGroup group;
  final List<Widget> children;
  final VoidCallback? onNew;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final n = group.node;
    final down = n?.freshness == NodeFreshness.NODE_FRESHNESS_UNREACHABLE;
    final running = group.sleeves
        .where((s) => s.state == SleeveState.SLEEVE_STATE_RUNNING)
        .length;
    final total = group.sleeves.length;
    final fence = Panel(
      legend: group.name.toUpperCase(),
      tone: down ? Tone.alarm : null,
      padding: const EdgeInsets.all(Tokens.gap),
      trailing: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (group.local) ...[
            const Annunciator(label: 'HUB', tone: Tone.info),
            const SizedBox(width: 8),
          ],
          if (n != null && isCareNode(n)) ...[
            const Annunciator(label: 'CARE', tone: Tone.info),
            const SizedBox(width: 8),
          ],
          if (n != null) ...[
            FreshnessLamp(
              freshness: n.freshness,
              age: Duration(seconds: n.age.seconds.toInt()),
            ),
            const SizedBox(width: 8),
          ],
          if (onNew != null && !down)
            IconButton(
              key: ValueKey('fence-new-${group.name}'),
              tooltip: 'New sleeve on ${group.name}',
              icon: const Icon(Icons.add),
              onPressed: onNew,
            ),
        ],
      ),
      child: children.isEmpty
          ? Text(
              down
                  ? 'UNREACHABLE. NO SLEEVES KNOWN.'
                  : 'NO SLEEVES. START ONE WITH +.',
              style: t.legend(),
            )
          : Column(
              children: [
                Align(
                  alignment: Alignment.centerLeft,
                  child: Text(
                    '$total SLEEVES, $running RUNNING',
                    key: ValueKey('fence-count-${group.name}'),
                    style: t.legend(),
                  ),
                ),
                const SizedBox(height: Tokens.gap),
                for (var i = 0; i < children.length; i++) ...[
                  if (i > 0) const SizedBox(height: Tokens.gap),
                  children[i],
                ],
              ],
            ),
    );
    return Opacity(
      key: ValueKey('fence-${group.name}'),
      opacity: down ? 0.55 : 1,
      child: fence,
    );
  }
}
