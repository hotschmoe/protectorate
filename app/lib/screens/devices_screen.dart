import 'dart:async';

import 'package:connectrpc/connect.dart';
import 'package:flutter/material.dart';

import '../gen/protectorate/v1/device.pb.dart';
import '../gen/protectorate/v1/events.pb.dart';
import '../hubs/event_feed.dart';
import '../hubs/hub_client.dart';
import '../theme/tokens.dart';
import '../vfd/panel.dart';

class DevicesScreen extends StatefulWidget {
  const DevicesScreen({
    super.key,
    required this.client,
    this.feed,
    this.embedded = false,
  });

  final HubClient client;
  final EventFeed? feed;
  final bool embedded;

  @override
  State<DevicesScreen> createState() => _DevicesScreenState();
}

class _DevicesScreenState extends State<DevicesScreen> {
  final _abort = CancelableSignal();
  StreamSubscription<Event>? _events;
  List<DeviceInfo> _devices = const [];
  Object? _error;
  bool _loading = false;
  String? _busy;

  @override
  void initState() {
    super.initState();
    _events = widget.feed?.incoming.listen((event) {
      if (event.kind.startsWith('device_auth.') ||
          event.kind.startsWith('devices.')) {
        _load();
      }
    });
    _load();
  }

  @override
  void dispose() {
    _abort.cancel();
    _events?.cancel();
    super.dispose();
  }

  Future<void> _load() async {
    if (_loading) return;
    setState(() => _loading = true);
    try {
      final response = await widget.client.devices.listDevices(
        ListDevicesRequest(),
        signal: TimeoutSignal(const Duration(seconds: 8), parent: _abort),
      );
      if (mounted) {
        setState(() {
          _devices = response.devices;
          _error = null;
        });
      }
    } on Object catch (error) {
      if (mounted) setState(() => _error = error);
    } finally {
      if (mounted) setState(() => _loading = false);
    }
  }

  Future<void> _rename(DeviceInfo device) async {
    final name = TextEditingController(text: device.name);
    final result = await showDialog<String>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Rename device'),
        content: TextField(
          controller: name,
          autofocus: true,
          decoration: const InputDecoration(labelText: 'Device name'),
          onSubmitted: (value) {
            if (value.trim().isNotEmpty) {
              Navigator.of(context).pop(value.trim());
            }
          },
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(context).pop(),
            child: const Text('CANCEL'),
          ),
          FilledButton(
            onPressed: () {
              if (name.text.trim().isNotEmpty) {
                Navigator.of(context).pop(name.text.trim());
              }
            },
            child: const Text('RENAME'),
          ),
        ],
      ),
    );
    name.dispose();
    if (result == null || !mounted) return;
    await _change(
      device,
      () => widget.client.devices.renameDevice(
        RenameDeviceRequest(id: device.id, name: result),
        signal: TimeoutSignal(const Duration(seconds: 8), parent: _abort),
      ),
    );
  }

  Future<void> _revoke(DeviceInfo device) async {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text('Revoke ${device.name}?'),
        content: const Text(
          'This device will lose access immediately, including open streams. If this is your current device, reconnect and approve it again to regain access.',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(context).pop(false),
            child: const Text('CANCEL'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(context).pop(true),
            child: const Text('REVOKE ACCESS'),
          ),
        ],
      ),
    );
    if (confirmed != true || !mounted) return;
    await _change(
      device,
      () => widget.client.devices.revokeDevice(
        RevokeDeviceRequest(id: device.id),
        signal: TimeoutSignal(const Duration(seconds: 8), parent: _abort),
      ),
    );
  }

  Future<void> _change(
    DeviceInfo device,
    Future<Object> Function() change,
  ) async {
    setState(() {
      _busy = device.id;
      _error = null;
    });
    try {
      await change();
      await _load();
    } on Object catch (error) {
      if (mounted) setState(() => _error = error);
    } finally {
      if (mounted) setState(() => _busy = null);
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final body = RefreshIndicator(
      onRefresh: _load,
      child: ListView(
        physics: const AlwaysScrollableScrollPhysics(),
        padding: const EdgeInsets.all(Tokens.gap),
        children: [
          if (widget.embedded)
            Row(
              children: [
                Expanded(
                  child: Text('${_devices.length} DEVICES', style: t.legend()),
                ),
                IconButton(
                  tooltip: 'Refresh devices',
                  onPressed: _loading ? null : _load,
                  icon: const Icon(Icons.refresh),
                ),
              ],
            ),
          if (_loading) const LinearProgressIndicator(),
          if (_error != null)
            NoteStrip('Could not load devices: $_error', tone: Tone.alarm),
          Center(
            child: ConstrainedBox(
              constraints: const BoxConstraints(
                maxWidth: Tokens.contentMaxWidth,
              ),
              child: _devicePanel(t),
            ),
          ),
        ],
      ),
    );
    if (widget.embedded) return body;
    return Scaffold(
      appBar: AppBar(
        title: const Text('Devices'),
        actions: [
          IconButton(
            tooltip: 'Refresh devices',
            onPressed: _loading ? null : _load,
            icon: const Icon(Icons.refresh),
          ),
        ],
      ),
      body: body,
    );
  }

  Widget _devicePanel(Tokens t) => Panel(
    legend: 'DEVICES · FULL ACCESS',
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          'Devices can approve other devices and revoke access. Compare the code before approving a request.',
          style: t.small(),
        ),
        if (_devices.isEmpty && !_loading)
          Padding(
            padding: const EdgeInsets.symmetric(vertical: 24),
            child: Text('No approved devices yet.', style: t.small()),
          ),
        for (final device in _devices) ...[
          const Divider(height: 24),
          Text(device.name, style: t.title()),
          const SizedBox(height: 4),
          Text('${device.platform} · FULL', style: t.legend(t.p.ok)),
          const SizedBox(height: 6),
          Text(
            device.hasLastUsedAt()
                ? 'Last used ${device.lastUsedAt.toDateTime().toLocal()}'
                : 'Not used yet',
            style: t.small(),
          ),
          if (device.hasApprovedAt())
            Text(
              'Approved ${device.approvedAt.toDateTime().toLocal()} by ${device.approvedBy}',
              style: t.small(),
            ),
          Wrap(
            spacing: 8,
            children: [
              TextButton(
                onPressed: _busy != null ? null : () => _rename(device),
                child: const Text('RENAME'),
              ),
              TextButton(
                onPressed: _busy != null ? null : () => _revoke(device),
                child: const Text('REVOKE'),
              ),
            ],
          ),
        ],
      ],
    ),
  );
}
