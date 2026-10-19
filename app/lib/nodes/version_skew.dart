import 'package:flutter/material.dart';

import '../gen/protectorate/v1/node.pb.dart';
import '../theme/tokens.dart';
import '../vfd/lamp.dart';

// An unknown (empty) version never counts as a difference.
bool versionsDiffer(String a, String b) =>
    a.isNotEmpty && b.isNotEmpty && a != b;

String nodeVersion(NodeStatus s) =>
    s.hasMetrics() ? s.metrics.agentVersion : '';

// Invited nodes have not joined yet, so they report no version.
bool nodeSkewed(NodeStatus s, String hubVersion) =>
    s.freshness != NodeFreshness.NODE_FRESHNESS_INVITED &&
    versionsDiffer(nodeVersion(s), hubVersion);

// The versions in play: the hub's, once known, and this build's.
class Versions {
  const Versions({this.hub = '', this.app = ''});

  final String hub;
  final String app;
}

class VersionSkew {
  const VersionSkew({required this.app, required this.nodes});

  // The app build differs from the hub's.
  final bool app;
  final List<String> nodes;

  bool get any => app || nodes.isNotEmpty;
}

VersionSkew versionSkew({
  required String hubVersion,
  required String appVersion,
  required Iterable<NodeStatus> nodes,
}) => VersionSkew(
  app: versionsDiffer(appVersion, hubVersion),
  nodes: [
    for (final n in nodes)
      if (nodeSkewed(n, hubVersion)) n.node.name,
  ],
);

String skewMessage(VersionSkew skew, Versions v) => skew.any
    ? [
        if (skew.app) 'This app is ${v.app}; the hub is ${v.hub}.',
        if (skew.nodes.isNotEmpty)
          'Nodes not at ${v.hub}: ${skew.nodes.join(', ')}.',
      ].join(' ')
    : 'App and nodes match the hub.';

// Both versions of a skewed node, small enough for a card or a board row.
class SkewBadge extends StatelessWidget {
  const SkewBadge({
    super.key,
    required this.nodeVersion,
    required this.hubVersion,
  });

  final String nodeVersion;
  final String hubVersion;

  @override
  Widget build(BuildContext context) => Semantics(
    label: 'version skew: node $nodeVersion, hub $hubVersion',
    excludeSemantics: true,
    child: Annunciator(
      label: 'SKEW $nodeVersion / HUB $hubVersion',
      tone: Tone.warn,
    ),
  );
}
