import 'dart:async';

import 'package:connectrpc/connect.dart';
import 'package:flutter/material.dart';

import '../gen/protectorate/v1/events.pb.dart';
import '../gen/protectorate/v1/hub.pb.dart';
import '../gen/protectorate/v1/node.pb.dart';
import '../gen/protectorate/v1/release.pb.dart';
import '../hubs/event_feed.dart';
import '../hubs/hub_client.dart';
import '../nodes/node_update.dart';
import '../nodes/version_skew.dart';
import '../theme/tokens.dart';
import '../vfd/lamp.dart';
import '../vfd/meter.dart';
import '../vfd/panel.dart';

class FleetUpdateCard extends StatefulWidget {
  const FleetUpdateCard({super.key, required this.client, required this.feed});

  final HubClient client;
  final EventFeed feed;

  @override
  State<FleetUpdateCard> createState() => _FleetUpdateCardState();
}

class _FleetUpdateCardState extends State<FleetUpdateCard> {
  final _abort = CancelableSignal();
  List<NodeStatus> _nodes = [];
  final Map<String, UpdateNodeProgress> _progress = {};
  final Map<String, String> _failures = {};
  String _version = '';
  bool _canUpdate = false;
  DateTime? _capabilityChecked;
  bool _updating = false;
  bool _loading = false;
  Object? _error;
  StreamSubscription<Event>? _events;
  Timer? _poll;
  Timer? _debounce;

  List<NodeStatus> get _pending => _nodes
      .where(
        (node) =>
            !node.node.local &&
            !node.node.invited &&
            !node.draining &&
            node.freshness != NodeFreshness.NODE_FRESHNESS_UNREACHABLE &&
            nodeSkewed(node, _version),
      )
      .toList();

  @override
  void initState() {
    super.initState();
    _events = widget.feed.incoming.listen((event) {
      if (event.kind.startsWith('node.') ||
          event.kind.startsWith('release.') ||
          event.kind.startsWith('hub.update')) {
        _debounce?.cancel();
        _debounce = Timer(const Duration(milliseconds: 300), _load);
      }
    });
    _poll = Timer.periodic(const Duration(seconds: 30), (_) => _load());
    _load();
  }

  @override
  void dispose() {
    _abort.cancel();
    _events?.cancel();
    _poll?.cancel();
    _debounce?.cancel();
    super.dispose();
  }

  Future<void> _load() async {
    if (_loading || _updating) return;
    setState(() => _loading = true);
    final signal = TimeoutSignal(const Duration(seconds: 15), parent: _abort);
    try {
      final status = await widget.client.hub.getStatus(
        GetStatusRequest(),
        signal: signal,
      );
      final nodes = await widget.client.node.listNodes(
        ListNodesRequest(),
        signal: signal,
      );
      if (!mounted) return;
      setState(() {
        _version = status.version;
        _nodes = nodes.nodes;
        _error = null;
      });
      if (_capabilityChecked == null ||
          DateTime.now().difference(_capabilityChecked!) >=
              const Duration(minutes: 5)) {
        final release = await widget.client.releases.checkRelease(
          CheckReleaseRequest(),
          signal: signal,
        );
        if (!mounted) return;
        setState(() {
          _canUpdate = release.canUpdate;
          _capabilityChecked = DateTime.now();
        });
      }
    } on Object catch (error) {
      if (mounted) setState(() => _error = error);
    } finally {
      if (mounted) setState(() => _loading = false);
    }
  }

  Future<void> _update() async {
    if (_updating || !_canUpdate) return;
    final nodes = _pending;
    setState(() {
      _updating = true;
      _progress.clear();
      _failures.clear();
      _error = null;
      for (final node in nodes) {
        _progress[node.node.name] = UpdateNodeProgress(message: 'Waiting');
      }
    });
    try {
      for (final node in nodes) {
        if (!mounted) return;
        final name = node.node.name;
        try {
          await for (final progress in widget.client.node.updateNode(
            UpdateNodeRequest(name: name),
            signal: _abort,
          )) {
            if (!mounted) return;
            setState(() => _progress[name] = progress);
          }
          final last = _progress[name]!;
          if (!updateFinal(last.phase)) {
            throw StateError('The update ended before confirmation.');
          }
          if (last.phase == UpdatePhase.UPDATE_PHASE_ROLLED_BACK) {
            setState(() => _failures[name] = 'Rolled back: ${last.message}');
          }
        } on Object catch (error) {
          if (!mounted) return;
          setState(() => _failures[name] = '$error');
        }
      }
    } finally {
      if (mounted) {
        setState(() => _updating = false);
        unawaited(_load());
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    if (!_canUpdate && _error == null) return const SizedBox.shrink();
    final count = _pending.length;
    final complete = _progress.values.where((p) => updateFinal(p.phase)).length;
    return Panel(
      legend: 'NODE UPDATES',
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Wrap(
            spacing: 12,
            runSpacing: 8,
            crossAxisAlignment: WrapCrossAlignment.center,
            children: [
              if (_canUpdate)
                FilledButton.icon(
                  key: const ValueKey('update-fleet'),
                  onPressed: count > 0 && !_updating && !_loading
                      ? _update
                      : null,
                  icon: const Icon(Icons.system_update_alt, size: 18),
                  label: Text('Update $count ${count == 1 ? 'node' : 'nodes'}'),
                ),
              if (_version.isNotEmpty)
                Text('To hub release $_version', style: t.small()),
            ],
          ),
          if (_updating) ...[const SizedBox(height: 8), const BusyBar()],
          if (_progress.isNotEmpty) ...[
            const SizedBox(height: 8),
            Text(
              _updating
                  ? '$complete of ${_progress.length} finished'
                  : '${_progress.length - _failures.length} updated, ${_failures.length} failed',
              style: t.small(),
            ),
            ConstrainedBox(
              constraints: const BoxConstraints(maxHeight: 180),
              child: SingleChildScrollView(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    for (final entry in _progress.entries)
                      Padding(
                        padding: const EdgeInsets.only(top: 6),
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.stretch,
                          children: [
                            Text(
                              '${entry.key}: ${_failures[entry.key] ?? (entry.value.phase == UpdatePhase.UPDATE_PHASE_UNSPECIFIED ? entry.value.message : updatePhaseLabel(entry.value))}',
                              style: t.small(
                                _failures.containsKey(entry.key)
                                    ? t.p.alarm
                                    : t.tone(updateTone(entry.value.phase)),
                              ),
                            ),
                            if (updatePercent(entry.value) case final percent?)
                              SegmentMeter(percent: percent),
                          ],
                        ),
                      ),
                  ],
                ),
              ),
            ),
          ],
          if (_error != null) ...[
            const SizedBox(height: 8),
            NoteStrip('Loading node updates: $_error', tone: Tone.alarm),
            TextButton(
              onPressed: _loading ? null : _load,
              child: const Text('Retry'),
            ),
          ],
        ],
      ),
    );
  }
}
