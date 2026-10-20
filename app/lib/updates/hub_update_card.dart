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
import '../theme/tokens.dart';
import '../vfd/lamp.dart';
import '../vfd/meter.dart';
import '../vfd/panel.dart';
import 'open_release_io.dart'
    if (dart.library.js_interop) 'open_release_web.dart';

class HubUpdateCard extends StatefulWidget {
  const HubUpdateCard({super.key, required this.client, required this.feed});

  final HubClient client;
  final EventFeed feed;

  @override
  State<HubUpdateCard> createState() => _HubUpdateCardState();
}

class _HubUpdateCardState extends State<HubUpdateCard> {
  GetStatusResponse? _status;
  ReleaseInfo? _release;
  UpdateHubProgress? _progress;
  Object? _error;
  String? _result;
  Tone _resultTone = Tone.ok;
  bool _loading = false;
  bool _updating = false;
  StreamSubscription<Event>? _events;
  Timer? _poll;
  final _abort = CancelableSignal();

  @override
  void initState() {
    super.initState();
    _events = widget.feed.incoming.listen((event) {
      if (event.kind.startsWith('release.') ||
          event.kind.startsWith('hub.update')) {
        unawaited(_load());
      }
    });
    _poll = Timer.periodic(const Duration(minutes: 1), (_) => _load());
    _load(check: true);
  }

  @override
  void dispose() {
    _events?.cancel();
    _poll?.cancel();
    _abort.cancel();
    super.dispose();
  }

  Future<void> _load({bool check = false}) async {
    if (_loading || _updating) return;
    setState(() => _loading = true);
    final signal = TimeoutSignal(const Duration(seconds: 15), parent: _abort);
    try {
      final status = await widget.client.hub.getStatus(
        GetStatusRequest(),
        signal: signal,
      );
      if (!mounted) return;
      setState(() => _status = status);
      if (check || _release == null) {
        final release = await widget.client.releases.checkRelease(
          CheckReleaseRequest(),
          signal: signal,
        );
        if (!mounted) return;
        setState(() => _release = release);
        final refreshed = await widget.client.hub.getStatus(
          GetStatusRequest(),
          signal: signal,
        );
        if (!mounted) return;
        setState(() => _status = refreshed);
      }
      if (mounted && check) setState(() => _error = null);
    } on Object catch (error) {
      if (mounted) setState(() => _error = 'Checking hub releases: $error');
    } finally {
      if (mounted) setState(() => _loading = false);
    }
  }

  Future<void> _update() async {
    if (_updating || !(_release?.canUpdate ?? false)) return;
    setState(() {
      _updating = true;
      _progress = null;
      _result = null;
      _error = null;
    });
    var restarting = false;
    var target = _status?.availableVersion ?? _release?.version ?? '';
    try {
      try {
        await for (final progress in widget.client.releases.updateHub(
          UpdateHubRequest(
            channel: _status?.updateChannel ?? _release?.channel,
          ),
          signal: _abort,
        )) {
          if (!mounted) return;
          setState(() => _progress = progress);
          if (progress.version.isNotEmpty) target = progress.version;
          restarting =
              restarting ||
              progress.phase == UpdatePhase.UPDATE_PHASE_RESTARTING;
          if (progress.phase == UpdatePhase.UPDATE_PHASE_ROLLED_BACK) {
            setState(() {
              _result = 'The hub rolled back to its previous release.';
              _resultTone = Tone.alarm;
            });
            return;
          }
          if (progress.phase == UpdatePhase.UPDATE_PHASE_SKIPPED) {
            setState(() {
              _result = 'The hub already runs ${progress.version}.';
              _resultTone = Tone.info;
            });
            return;
          }
        }
      } on Object {
        if (!restarting) rethrow;
      }
      if (!restarting &&
          _progress?.phase != UpdatePhase.UPDATE_PHASE_CONFIRMED) {
        throw StateError('The update ended before the hub restarted.');
      }
      await _waitForHub(target);
    } on Object catch (error) {
      if (mounted) setState(() => _error = 'Updating hub: $error');
    } finally {
      if (mounted) {
        setState(() => _updating = false);
        unawaited(_load());
      }
    }
  }

  Future<void> _waitForHub(String target) async {
    final deadline = DateTime.now().add(const Duration(seconds: 90));
    final previous = _status;
    GetStatusResponse? observed;
    while (mounted && DateTime.now().isBefore(deadline)) {
      final probe = CancelableSignal(parent: _abort);
      try {
        final status = await widget.client.hub
            .getStatus(GetStatusRequest(), signal: probe)
            .timeout(const Duration(seconds: 4));
        observed = status;
        if (!mounted) return;
        if (target.isNotEmpty && status.version == target) {
          setState(() {
            _status = status;
            _result = 'Hub updated to ${status.version}.';
            _resultTone = Tone.ok;
          });
          return;
        }
        if (previous != null &&
            status.version == previous.version &&
            previous.hasStartedAt() &&
            status.hasStartedAt() &&
            status.startedAt.toDateTime().isAfter(
              previous.startedAt.toDateTime(),
            )) {
          setState(() {
            _status = status;
            _result =
                'The hub returned on its previous release ${status.version}; $target was not confirmed.';
            _resultTone = Tone.alarm;
          });
          return;
        }
      } on Object {
        if (!mounted) return;
      } finally {
        probe.cancel();
      }
      await Future<void>.delayed(const Duration(seconds: 2));
    }
    if (!mounted) return;
    throw StateError(
      observed == null
          ? 'The hub has not returned. Reconnect to check the update result.'
          : 'The hub is running ${observed.version}; $target was not confirmed. The update may have rolled back.',
    );
  }

  Future<void> _notes(String url) async {
    final uri = Uri.tryParse(url);
    if (uri == null ||
        !const {'https', 'http'}.contains(uri.scheme) ||
        uri.host.isEmpty ||
        uri.userInfo.isNotEmpty) {
      setState(() => _error = 'The release notes URL is invalid.');
      return;
    }
    try {
      await openRelease(uri);
    } on Object catch (error) {
      if (mounted) setState(() => _error = 'Opening release notes: $error');
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final status = _status;
    final release = _release;
    final available = status?.availableVersion ?? '';
    final channel = status?.updateChannel.isNotEmpty == true
        ? status!.updateChannel
        : release?.channel ?? 'stable';
    final notes = status?.releaseNotesUrl.isNotEmpty == true
        ? status!.releaseNotesUrl
        : release?.notesUrl ?? '';
    final newer = available.isNotEmpty && available != status?.version;
    final p = _progress;
    final progress = p == null
        ? null
        : UpdateNodeProgress(
            phase: p.phase,
            version: p.version,
            previousVersion: p.previousVersion,
            sentBytes: p.sentBytes,
            totalBytes: p.totalBytes,
          );
    return Panel(
      legend: 'HUB UPDATE',
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            newer
                ? '$available available on $channel'
                : status == null
                ? 'Checking hub version'
                : '${status.version} on $channel',
            style: t.data(t.p.emph),
          ),
          if (newer && status != null)
            Text('Currently ${status.version}', style: t.small()),
          const SizedBox(height: 8),
          Wrap(
            spacing: 8,
            runSpacing: 4,
            children: [
              if (release?.canUpdate == true)
                FilledButton.icon(
                  key: const ValueKey('update-hub'),
                  onPressed: newer && !_updating && !_loading ? _update : null,
                  icon: const Icon(Icons.system_update_alt, size: 18),
                  label: const Text('Update hub'),
                ),
              if (notes.isNotEmpty)
                TextButton(
                  onPressed: () => _notes(notes),
                  child: const Text('Release notes'),
                ),
              TextButton(
                onPressed: _updating || _loading
                    ? null
                    : () => _load(check: true),
                child: const Text('Check for updates'),
              ),
            ],
          ),
          if (_loading || _updating) ...[
            const SizedBox(height: 8),
            if (progress != null && updatePercent(progress) != null)
              SegmentMeter(percent: updatePercent(progress)!)
            else
              const BusyBar(),
          ],
          if (progress != null && _updating) ...[
            const SizedBox(height: 8),
            Text(updatePhaseLabel(progress), style: t.data()),
            if (p!.message.isNotEmpty) Text(p.message, style: t.small()),
          ],
          if (_result != null) ...[
            const SizedBox(height: 8),
            NoteStrip(_result!, tone: _resultTone),
          ],
          if (_error != null) ...[
            const SizedBox(height: 8),
            NoteStrip('$_error', tone: Tone.alarm),
          ],
        ],
      ),
    );
  }
}
