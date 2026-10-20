import 'package:connectrpc/connect.dart';
import 'package:flutter/material.dart';

import '../gen/protectorate/v1/node.pb.dart';
import '../hubs/hub_client.dart';
import '../theme/tokens.dart';
import '../vfd/lamp.dart';
import '../vfd/meter.dart';
import '../vfd/panel.dart';
import 'node_view.dart';

bool updateFinal(UpdatePhase p) => const {
  UpdatePhase.UPDATE_PHASE_SKIPPED,
  UpdatePhase.UPDATE_PHASE_CONFIRMED,
  UpdatePhase.UPDATE_PHASE_ROLLED_BACK,
}.contains(p);

String updatePhaseLabel(UpdateNodeProgress p) => switch (p.phase) {
  UpdatePhase.UPDATE_PHASE_SKIPPED => 'ALREADY CURRENT',
  UpdatePhase.UPDATE_PHASE_SENDING =>
    p.totalBytes > 0
        ? 'SENDING  ${formatBytes(p.sentBytes.toInt())} / ${formatBytes(p.totalBytes.toInt())}'
        : 'SENDING',
  UpdatePhase.UPDATE_PHASE_VERIFYING => 'VERIFYING',
  UpdatePhase.UPDATE_PHASE_SWAPPING =>
    'SWAPPED  ${p.previousVersion} > ${p.version}',
  UpdatePhase.UPDATE_PHASE_RESTARTING => 'RESTARTING',
  UpdatePhase.UPDATE_PHASE_CONFIRMED => 'CONFIRMED  ${p.version}',
  UpdatePhase.UPDATE_PHASE_ROLLED_BACK => 'ROLLED BACK',
  _ => '',
};

double? updatePercent(UpdateNodeProgress p) =>
    p.phase == UpdatePhase.UPDATE_PHASE_SENDING && p.totalBytes > 0
    ? p.sentBytes.toInt() * 100 / p.totalBytes.toInt()
    : null;

Tone updateTone(UpdatePhase p) => switch (p) {
  UpdatePhase.UPDATE_PHASE_CONFIRMED => Tone.ok,
  UpdatePhase.UPDATE_PHASE_ROLLED_BACK => Tone.alarm,
  UpdatePhase.UPDATE_PHASE_SKIPPED => Tone.info,
  _ => Tone.lit,
};

Future<void> showNodeUpdate(
  BuildContext context, {
  required HubClient client,
  required String node,
  required String nodeVersion,
  required String hubVersion,
  VoidCallback? onFinished,
}) => showDialog<void>(
  context: context,
  barrierDismissible: false,
  builder: (_) => NodeUpdateDialog(
    client: client,
    node: node,
    nodeVersion: nodeVersion,
    hubVersion: hubVersion,
    onFinished: onFinished,
  ),
);

class NodeUpdateDialog extends StatefulWidget {
  const NodeUpdateDialog({
    super.key,
    required this.client,
    required this.node,
    required this.nodeVersion,
    required this.hubVersion,
    this.onFinished,
  });

  final HubClient client;
  final String node;
  final String nodeVersion;
  final String hubVersion;
  final VoidCallback? onFinished;

  @override
  State<NodeUpdateDialog> createState() => _NodeUpdateDialogState();
}

class _NodeUpdateDialogState extends State<NodeUpdateDialog> {
  final _abort = CancelableSignal();
  UpdateNodeProgress? _last;
  Object? _error;
  bool _started = false;
  bool _done = false;

  void _start() {
    setState(() {
      _started = true;
      _error = null;
      _last = null;
    });
    widget.client.node
        .updateNode(UpdateNodeRequest(name: widget.node), signal: _abort)
        .listen(
          (p) {
            if (mounted) setState(() => _last = p);
          },
          onError: (Object e) {
            if (mounted) {
              setState(() {
                _error = e;
                _done = true;
              });
            }
          },
          onDone: () {
            if (!mounted) return;
            setState(() => _done = true);
            widget.onFinished?.call();
          },
          cancelOnError: true,
        );
  }

  @override
  void dispose() {
    _abort.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final p = _last;
    final failed = _error != null;
    return AlertDialog(
      title: Text('Update ${widget.node}'),
      content: SizedBox(
        width: 380,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('NODE ${widget.nodeVersion}', style: t.data(t.p.emph)),
            Text('HUB  ${widget.hubVersion}', style: t.data(t.p.emph)),
            const SizedBox(height: 14),
            if (!_started)
              const NoteStrip(
                'The hub sends its current release to the node, which verifies it, swaps it in and restarts. The node puts its old binary back if the hub does not reach the new one.',
                tone: Tone.info,
              )
            else ...[
              if (p != null)
                Text(
                  updatePhaseLabel(p),
                  key: const ValueKey('update-phase'),
                  style: t.data(t.tone(updateTone(p.phase))),
                ),
              const SizedBox(height: 8),
              if (updatePercent(p ?? UpdateNodeProgress()) case final pct?)
                SegmentMeter(percent: pct)
              else if (!_done)
                const BusyBar(),
              if (p != null && p.message.isNotEmpty) ...[
                const SizedBox(height: 8),
                Text(p.message, style: t.small()),
              ],
              if (failed) ...[
                const SizedBox(height: 8),
                NoteStrip('$_error', tone: Tone.alarm),
              ],
            ],
          ],
        ),
      ),
      actions: [
        TextButton(
          key: const ValueKey('update-close'),
          onPressed: _started && !_done
              ? null
              : () => Navigator.of(context).pop(),
          child: const Text('CLOSE'),
        ),
        if (!_started || failed)
          FilledButton(
            key: const ValueKey('update-start'),
            onPressed: _start,
            child: Text(failed ? 'RETRY' : 'UPDATE'),
          ),
      ],
    );
  }
}
