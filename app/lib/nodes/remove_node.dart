import 'package:connectrpc/connect.dart';
import 'package:flutter/material.dart';

import '../gen/protectorate/v1/node.pb.dart';
import '../hubs/hub_client.dart';
import '../theme/tokens.dart';
import '../vfd/lamp.dart';
import '../vfd/panel.dart';
import 'add_node.dart' show inviteError;

String removePhaseLabel(RemovePhase p) => switch (p) {
  RemovePhase.REMOVE_PHASE_DRAINING => 'DRAINING',
  RemovePhase.REMOVE_PHASE_UNINSTALLING => 'UNINSTALL',
  RemovePhase.REMOVE_PHASE_REVOKING => 'REVOKED',
  RemovePhase.REMOVE_PHASE_REMOVED => 'REMOVED',
  _ => '',
};

// Resolves true once the node is gone from the hub.
Future<bool> showRemoveNode(
  BuildContext context, {
  required HubClient client,
  required NodeStatus status,
}) async =>
    await showDialog<bool>(
      context: context,
      barrierDismissible: false,
      builder: (_) => RemoveNodeDialog(client: client, status: status),
    ) ??
    false;

class RemoveNodeDialog extends StatefulWidget {
  const RemoveNodeDialog({
    super.key,
    required this.client,
    required this.status,
  });

  final HubClient client;
  final NodeStatus status;

  @override
  State<RemoveNodeDialog> createState() => _RemoveNodeDialogState();
}

class _RemoveNodeDialogState extends State<RemoveNodeDialog> {
  final _confirm = TextEditingController();
  final _abort = CancelableSignal();
  RemoveJobs _jobs = RemoveJobs.REMOVE_JOBS_WAIT;
  late bool _uninstall = _online;
  final _steps = <RemoveNodeProgress>[];
  String? _error;
  bool _started = false;
  bool _done = false;

  String get _name => widget.status.node.name;
  bool get _pending => widget.status.node.invited;
  bool get _online =>
      widget.status.freshness == NodeFreshness.NODE_FRESHNESS_FRESH;
  bool get _armed => _pending || _confirm.text.trim() == _name;

  @override
  void dispose() {
    _abort.cancel();
    _confirm.dispose();
    super.dispose();
  }

  void _start() {
    setState(() {
      _started = true;
      _error = null;
      _steps.clear();
    });
    widget.client.node
        .removeNode(
          RemoveNodeRequest(
            name: _name,
            jobs: _jobs,
            uninstall: !_pending && _uninstall,
          ),
          signal: _abort,
        )
        .listen(
          (p) {
            if (mounted) setState(() => _steps.add(p));
          },
          onError: (Object e) {
            if (mounted) setState(() => _error = inviteError(e));
          },
          onDone: () {
            if (!mounted || _error != null) return;
            final removed =
                _steps.isNotEmpty &&
                _steps.last.phase == RemovePhase.REMOVE_PHASE_REMOVED;
            setState(() => _done = removed);
          },
          cancelOnError: true,
        );
  }

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final running = _started && !_done && _error == null;
    return AlertDialog(
      title: Text(_pending ? 'Cancel invite' : 'Remove $_name'),
      content: SizedBox(
        width: 520,
        child: SingleChildScrollView(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              if (!_started) ..._options(t) else ..._progress(t, running),
              if (_error != null) ...[
                const SizedBox(height: Tokens.gap),
                NoteStrip(_error!, tone: Tone.alarm),
              ],
            ],
          ),
        ),
      ),
      actions: [
        TextButton(
          key: const ValueKey('remove-close'),
          onPressed: running ? null : () => Navigator.of(context).pop(_done),
          child: Text(_done ? 'DONE' : (_pending ? 'KEEP' : 'CANCEL')),
        ),
        if (!_done)
          FilledButton(
            key: const ValueKey('remove-go'),
            style: FilledButton.styleFrom(
              backgroundColor: t.p.alarm,
              foregroundColor: t.p.onKey,
            ),
            onPressed: _armed && !running ? _start : null,
            child: Text(
              _error != null
                  ? 'RETRY'
                  : (_pending ? 'CANCEL INVITE' : 'REMOVE NODE'),
            ),
          ),
      ],
    );
  }

  List<Widget> _options(Tokens t) {
    if (_pending) {
      return [
        Text(
          'The key for $_name stops working and the pending entry is removed. Nothing was installed yet, or it never reached the hub.',
          style: t.body(),
        ),
      ];
    }
    return [
      NoteStrip(
        'No new jobs or leases go to $_name. Its tailnet enrollment is revoked, so it cannot rejoin without a new invite, and its inventory entry is removed.',
        tone: Tone.alarm,
      ),
      const SizedBox(height: Tokens.gapL),
      Text('RUNNING JOBS', style: t.legend()),
      const SizedBox(height: 4),
      ChoiceKeys<RemoveJobs>(
        key: const ValueKey('remove-jobs'),
        options: const [
          (RemoveJobs.REMOVE_JOBS_WAIT, 'LET THEM FINISH'),
          (RemoveJobs.REMOVE_JOBS_CANCEL, 'CANCEL THEM'),
        ],
        selected: _jobs,
        onSelected: (v) => setState(() => _jobs = v),
      ),
      const SizedBox(height: Tokens.gap),
      Text('ON THE MACHINE', style: t.legend()),
      const SizedBox(height: 4),
      ChoiceKeys<bool>(
        key: const ValueKey('remove-uninstall'),
        options: const [(true, 'ALSO UNINSTALL'), (false, 'LEAVE INSTALLED')],
        selected: _uninstall && _online,
        onSelected: _online ? (v) => setState(() => _uninstall = v) : null,
      ),
      const SizedBox(height: 4),
      Text(
        _online
            ? 'Uninstall unregisters the agent\'s startup and stops it. It deletes nothing; the binary and state stay for you to delete.'
            : '$_name is offline, so it cannot uninstall itself. Run protectorate node uninstall on it later; it deletes nothing.',
        style: t.small(),
      ),
      const SizedBox(height: Tokens.gapL),
      TextField(
        key: const ValueKey('remove-confirm'),
        controller: _confirm,
        autocorrect: false,
        enableSuggestions: false,
        style: t.data(t.p.emph),
        decoration: InputDecoration(labelText: 'Type $_name to confirm'),
        onChanged: (_) => setState(() {}),
      ),
    ];
  }

  List<Widget> _progress(Tokens t, bool running) => [
    for (final s in _steps)
      Padding(
        padding: const EdgeInsets.only(bottom: Tokens.gapS),
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            SizedBox(
              width: 112,
              child: LampLegend(
                label: removePhaseLabel(s.phase),
                tone: s.phase == RemovePhase.REMOVE_PHASE_REMOVED
                    ? Tone.ok
                    : Tone.info,
              ),
            ),
            Expanded(
              child: Text(
                s.runningJobs > 0
                    ? '${s.message} (${s.runningJobs} running)'
                    : s.message,
                style: t.small(),
              ),
            ),
          ],
        ),
      ),
    if (running) const BusyBar(tone: Tone.info),
  ];
}
