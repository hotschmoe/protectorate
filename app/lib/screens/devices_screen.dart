import 'dart:async';

import 'package:connectrpc/connect.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../format.dart';
import '../gen/protectorate/v1/device.pb.dart';
import '../gen/protectorate/v1/events.pb.dart';
import '../hubs/connection.dart';
import '../hubs/event_feed.dart';
import '../hubs/hub_client.dart';
import '../hubs/hub_profile.dart';
import '../hubs/pairing.dart';
import '../theme/tokens.dart';
import '../vfd/panel.dart';

class DevicesScreen extends StatefulWidget {
  const DevicesScreen({
    super.key,
    required this.client,
    required this.hub,
    this.feed,
    this.embedded = false,
  });

  final HubClient client;
  final HubProfile hub;
  final EventFeed? feed;
  final bool embedded;

  @override
  State<DevicesScreen> createState() => _DevicesScreenState();
}

class _DevicesScreenState extends State<DevicesScreen> {
  final _abort = CancelableSignal();
  late final _self = tokenDeviceId(widget.hub.token);
  StreamSubscription<Event>? _events;
  List<DeviceInfo> _devices = const [];
  Object? _error;
  bool _loading = false;
  String? _busy;

  @override
  void initState() {
    super.initState();
    _events = widget.feed?.incoming.listen((event) {
      if (event.kind.startsWith('devices.')) _load();
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

  Future<String?> _askName(
    String title,
    String action, [
    String name = '',
  ]) async {
    final field = TextEditingController(text: name);
    final result = await showDialog<String>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text(title),
        content: TextField(
          key: const ValueKey('device-name'),
          controller: field,
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
            key: const ValueKey('device-name-confirm'),
            onPressed: () {
              if (field.text.trim().isNotEmpty) {
                Navigator.of(context).pop(field.text.trim());
              }
            },
            child: Text(action),
          ),
        ],
      ),
    );
    field.dispose();
    return result;
  }

  Future<void> _add() async {
    final name = await _askName('Add a device', 'ADD DEVICE');
    if (name == null || !mounted) return;
    setState(() {
      _busy = '';
      _error = null;
    });
    try {
      final added = await widget.client.devices.addDevice(
        AddDeviceRequest(name: name),
        signal: TimeoutSignal(const Duration(seconds: 8), parent: _abort),
      );
      if (!mounted) return;
      await showDialog<void>(
        context: context,
        barrierDismissible: false,
        builder: (_) => _AddedDeviceDialog(
          name: added.device.name,
          token: added.token,
          baseUrl: widget.hub.baseUrl,
        ),
      );
      await _load();
    } on Object catch (error) {
      if (mounted) setState(() => _error = error);
    } finally {
      if (mounted) setState(() => _busy = null);
    }
  }

  Future<void> _rename(DeviceInfo device) async {
    final result = await _askName('Rename device', 'RENAME', device.name);
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
        content: Text(
          device.id == _self
              ? 'This is the device you are using. It loses access immediately, including open streams. To reconnect, add it again from another device or with protectorate devices add on the hub.'
              : 'This device loses access immediately, including open streams. Its token stops working for good.',
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
          'Each device holds its own token with full access. Devices can add other devices and revoke any of them.',
          style: t.small(),
        ),
        const SizedBox(height: 12),
        FilledButton.icon(
          key: const ValueKey('add-device'),
          onPressed: _busy != null ? null : _add,
          icon: const Icon(Icons.add),
          label: const Text('ADD DEVICE'),
        ),
        if (_devices.isEmpty && !_loading)
          Padding(
            padding: const EdgeInsets.symmetric(vertical: 24),
            child: Text('No devices yet.', style: t.small()),
          ),
        for (final device in _devices) ...[
          const Divider(height: 24),
          Text(device.name, style: t.title()),
          const SizedBox(height: 4),
          Text(
            device.id == _self ? 'THIS DEVICE · FULL' : 'FULL',
            style: t.legend(t.p.ok),
          ),
          const SizedBox(height: 6),
          Text(
            device.hasLastUsedAt()
                ? 'Last used ${_age(device.lastUsedAt.toDateTime())}'
                : 'Not used yet',
            style: t.small(),
          ),
          Text(
            [
              if (device.hasAddedAt())
                'Added ${_age(device.addedAt.toDateTime())}',
              if (device.addedBy.isNotEmpty) 'by ${device.addedBy}',
            ].join(' '),
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

  String _age(DateTime time) => formatAge(DateTime.now().difference(time));
}

class _AddedDeviceDialog extends StatelessWidget {
  const _AddedDeviceDialog({
    required this.name,
    required this.token,
    required this.baseUrl,
  });

  final String name;
  final String token;
  final String baseUrl;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final links = pairLinks(baseUrl, token);
    Widget secret(String label, String value, String key) => Padding(
      padding: const EdgeInsets.only(top: 12),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(label, style: t.legend()),
          Row(
            children: [
              Expanded(
                child: SelectableText(
                  value,
                  key: ValueKey(key),
                  style: t.data(),
                ),
              ),
              IconButton(
                tooltip: 'Copy $label',
                icon: const Icon(Icons.copy),
                onPressed: () async {
                  await Clipboard.setData(ClipboardData(text: value));
                  if (context.mounted) {
                    ScaffoldMessenger.maybeOf(
                      context,
                    )?.showSnackBar(SnackBar(content: Text('Copied $label')));
                  }
                },
              ),
            ],
          ),
        ],
      ),
    );
    return AlertDialog(
      title: Text('$name added'),
      content: SizedBox(
        width: 480,
        child: SingleChildScrollView(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                'Open a link on the new device, or paste it into ADD HUB there. The token is shown only now and gives full access: send it only over a channel you trust.',
                style: t.small(),
              ),
              secret('WEB LINK', links.web, 'added-device-web'),
              secret('APP LINK', links.app, 'added-device-app'),
              secret('TOKEN', token, 'added-device-token'),
            ],
          ),
        ),
      ),
      actions: [
        FilledButton(
          key: const ValueKey('added-device-done'),
          onPressed: () => Navigator.of(context).pop(),
          child: const Text('DONE'),
        ),
      ],
    );
  }
}
