import 'dart:async';

import 'package:connectrpc/connect.dart';
import 'package:flutter/material.dart';

import '../gen/protectorate/v1/approval.pb.dart';
import '../gen/protectorate/v1/events.pb.dart';
import '../hubs/event_feed.dart';
import '../hubs/hub_client.dart';
import '../theme/tokens.dart';
import '../vfd/panel.dart';

/// Agent calls the hub holds for the owner: destructive RPCs, admin grants
/// and jobs, and launches whose harness or profile changed.
class ActionApprovalBanner extends StatefulWidget {
  const ActionApprovalBanner({
    super.key,
    required this.client,
    required this.feed,
  });

  final HubClient client;
  final EventFeed feed;

  @override
  State<ActionApprovalBanner> createState() => _ActionApprovalBannerState();
}

class _ActionApprovalBannerState extends State<ActionApprovalBanner> {
  List<Approval> _requests = const [];
  StreamSubscription<Event>? _events;
  Timer? _sweep;
  final _abort = CancelableSignal();
  bool _loading = false;
  bool _reload = false;
  bool _busy = false;
  Object? _error;

  @override
  void initState() {
    super.initState();
    _events = widget.feed.incoming.listen((event) {
      if (event.kind.startsWith('approval.')) _load();
    });
    _sweep = Timer.periodic(const Duration(seconds: 15), (_) => _load());
    _load();
  }

  @override
  void dispose() {
    _abort.cancel();
    _events?.cancel();
    _sweep?.cancel();
    super.dispose();
  }

  Future<void> _load() async {
    if (_loading) {
      _reload = true;
      return;
    }
    _loading = true;
    try {
      final response = await widget.client.approvals.listApprovals(
        ListApprovalsRequest(),
        signal: TimeoutSignal(const Duration(seconds: 8), parent: _abort),
      );
      if (mounted) setState(() => _requests = response.approvals);
    } on Object {
      // A disconnected feed already reports connection failures in the shell.
    } finally {
      _loading = false;
      if (_reload && mounted) {
        _reload = false;
        _load();
      }
    }
  }

  Future<void> _decide(Approval request, bool approve) async {
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      await widget.client.approvals.decideApproval(
        DecideApprovalRequest(id: request.id, approve: approve),
        signal: TimeoutSignal(const Duration(seconds: 8), parent: _abort),
      );
      await _load();
    } on Object catch (error) {
      if (mounted) setState(() => _error = error);
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final pending = _requests
        .where(
          (request) => request.expiresAt.toDateTime().isAfter(DateTime.now()),
        )
        .toList();
    if (pending.isEmpty) return const SizedBox.shrink();
    final request = pending.first;
    final t = Tokens.of(context);
    final call = request.procedure.replaceFirst('/protectorate.v1.', '');
    return Material(
      color: t.p.frame,
      child: Padding(
        padding: const EdgeInsets.all(Tokens.gap),
        child: Panel(
          legend:
              'ACTION APPROVAL${pending.length > 1 ? ' · ${pending.length} PENDING' : ''}',
          tone: Tone.warn,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            mainAxisSize: MainAxisSize.min,
            children: [
              Text(
                '${request.actor} wants $call ${request.subject}',
                style: t.title(),
              ),
              const SizedBox(height: 4),
              Text(
                [
                  request.reason,
                  if (request.detail.isNotEmpty) request.detail,
                  if (request.runId.isNotEmpty) 'run ${request.runId}',
                ].join(' · '),
                style: t.small(),
              ),
              const SizedBox(height: 8),
              Wrap(
                spacing: 12,
                runSpacing: 8,
                children: [
                  FilledButton(
                    onPressed: _busy ? null : () => _decide(request, true),
                    child: const Text('APPROVE'),
                  ),
                  TextButton(
                    onPressed: _busy ? null : () => _decide(request, false),
                    child: const Text('DENY'),
                  ),
                ],
              ),
              if (_error != null)
                Text(
                  'Could not update request: $_error',
                  style: t.small(t.p.alarm),
                ),
            ],
          ),
        ),
      ),
    );
  }
}
