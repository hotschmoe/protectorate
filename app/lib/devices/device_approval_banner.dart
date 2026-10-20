import 'dart:async';

import 'package:connectrpc/connect.dart';
import 'package:flutter/material.dart';

import '../gen/protectorate/v1/device.pb.dart';
import '../gen/protectorate/v1/events.pb.dart';
import '../hubs/event_feed.dart';
import '../hubs/hub_client.dart';
import '../theme/tokens.dart';
import '../vfd/panel.dart';

class DeviceApprovalBanner extends StatefulWidget {
  const DeviceApprovalBanner({
    super.key,
    required this.client,
    required this.feed,
  });

  final HubClient client;
  final EventFeed feed;

  @override
  State<DeviceApprovalBanner> createState() => _DeviceApprovalBannerState();
}

class _DeviceApprovalBannerState extends State<DeviceApprovalBanner> {
  List<DeviceAuthInfo> _requests = const [];
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
      if (event.kind.startsWith('device_auth.')) _load();
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
      final response = await widget.client.devices.listDeviceAuth(
        ListDeviceAuthRequest(),
        signal: TimeoutSignal(const Duration(seconds: 8), parent: _abort),
      );
      if (mounted) setState(() => _requests = response.requests);
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

  Future<void> _decide(DeviceAuthInfo request, bool approve) async {
    if (approve) {
      final matches = await showDialog<bool>(
        context: context,
        builder: (context) => AlertDialog(
          title: const Text('Does the code match?'),
          content: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text('${request.name} (${request.platform}) wants Full access.'),
              const SizedBox(height: 12),
              SelectableText(
                request.userCode,
                style: Tokens.of(context).data().copyWith(fontSize: 30),
              ),
              const SizedBox(height: 12),
              Text(
                'Approve only if this exact code is shown on the device you are connecting. Request from ${request.requestingIp}.',
              ),
            ],
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.of(context).pop(false),
              child: const Text('CANCEL'),
            ),
            FilledButton(
              onPressed: () => Navigator.of(context).pop(true),
              child: const Text('CODE MATCHES · APPROVE'),
            ),
          ],
        ),
      );
      if (matches != true || !mounted) return;
    }
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final signal = TimeoutSignal(const Duration(seconds: 8), parent: _abort);
      if (approve) {
        await widget.client.devices.approveDeviceAuth(
          ApproveDeviceAuthRequest(userCode: request.userCode),
          signal: signal,
        );
      } else {
        await widget.client.devices.denyDeviceAuth(
          DenyDeviceAuthRequest(userCode: request.userCode),
          signal: signal,
        );
      }
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
    return Material(
      color: t.p.frame,
      child: Padding(
        padding: const EdgeInsets.all(Tokens.gap),
        child: Panel(
          legend:
              'DEVICE APPROVAL${pending.length > 1 ? ' · ${pending.length} PENDING' : ''}',
          tone: Tone.warn,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            mainAxisSize: MainAxisSize.min,
            children: [
              Text('${request.name} wants access', style: t.title()),
              const SizedBox(height: 4),
              Text(
                '${request.platform} · ${request.requestingIp}',
                style: t.small(),
              ),
              const SizedBox(height: 8),
              Wrap(
                spacing: 12,
                runSpacing: 8,
                crossAxisAlignment: WrapCrossAlignment.center,
                children: [
                  SelectableText(
                    request.userCode,
                    style: t
                        .data(t.p.info)
                        .copyWith(fontSize: 24, letterSpacing: 2),
                  ),
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
