import 'package:connectrpc/connect.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';

import '../gen/protectorate/v1/device.pb.dart';
import '../hubs/device_auth.dart';
import '../hubs/hub_client.dart';
import '../hubs/hub_profile.dart';
import '../hubs/pairing.dart';
import '../theme/tokens.dart';
import '../vfd/panel.dart';

Future<HubProfile?> connectHub(
  BuildContext context, {
  required HubClient Function(HubProfile) clientFactory,
  required Future<void> Function(HubProfile) save,
  HubProfile? existing,
  String? baseUrl,
  String caPem = '',
}) => showDialog<HubProfile>(
  context: context,
  barrierDismissible: false,
  builder: (_) => _ConnectDialog(
    clientFactory: clientFactory,
    save: save,
    existing: existing,
    baseUrl: baseUrl,
    caPem: caPem,
  ),
);

String _platform() => kIsWeb ? 'web' : defaultTargetPlatform.name;

String _normalizeUrl(String value) {
  var result = value.trim();
  final link = Uri.tryParse(result);
  final pairing = link == null ? null : parsePairing(link);
  if (pairing != null) return pairing.baseUrl;
  if (!result.contains('://')) result = 'https://$result';
  while (result.endsWith('/')) {
    result = result.substring(0, result.length - 1);
  }
  return result;
}

class _ConnectDialog extends StatefulWidget {
  const _ConnectDialog({
    required this.clientFactory,
    required this.save,
    this.existing,
    this.baseUrl,
    this.caPem = '',
  });

  final HubClient Function(HubProfile) clientFactory;
  final Future<void> Function(HubProfile) save;
  final HubProfile? existing;
  final String? baseUrl;
  final String caPem;

  @override
  State<_ConnectDialog> createState() => _ConnectDialogState();
}

class _ConnectDialogState extends State<_ConnectDialog> {
  final _form = GlobalKey<FormState>();
  late final _name = TextEditingController(text: widget.existing?.name ?? '');
  late final _url = TextEditingController(
    text: widget.baseUrl ?? widget.existing?.baseUrl ?? '',
  );
  final _device = TextEditingController(text: '${_platform()} app');
  CancelableSignal? _abort;
  StartDeviceAuthResponse? _request;
  bool _working = false;
  String? _error;
  String? _approvedSecret;
  HubProfile? _connecting;

  @override
  void dispose() {
    _abort?.cancel();
    _name.dispose();
    _url.dispose();
    _device.dispose();
    super.dispose();
  }

  Future<void> _connect() async {
    if (!_form.currentState!.validate()) return;
    final baseUrl = _normalizeUrl(_url.text);
    final link = Uri.tryParse(_url.text.trim());
    final linked = link == null ? null : parsePairing(link);
    final caPem = linked?.caPem.isNotEmpty == true
        ? linked!.caPem
        : widget.caPem.isNotEmpty
        ? widget.caPem
        : widget.existing?.caPem ?? '';
    final profile =
        _connecting ??
        HubProfile(
          id:
              widget.existing?.id ??
              DateTime.now().microsecondsSinceEpoch.toString(),
          name: _name.text.trim().isEmpty
              ? Uri.parse(baseUrl).host
              : _name.text.trim(),
          baseUrl: baseUrl,
          caPem: caPem,
        );
    _connecting = profile;
    final abort = _abort = CancelableSignal();
    setState(() {
      _working = true;
      _error = null;
    });
    try {
      final secret = _approvedSecret ??= await authorizeDevice(
        widget.clientFactory(profile).devices,
        name: _device.text.trim(),
        platform: _platform(),
        signal: abort,
        onStarted: (request) {
          if (mounted) setState(() => _request = request);
        },
      );
      final approved = HubProfile(
        id: profile.id,
        name: profile.name,
        baseUrl: profile.baseUrl,
        token: secret,
        caPem: profile.caPem,
      );
      await widget.save(approved);
      if (mounted) Navigator.of(context).pop(approved);
    } on Object catch (error) {
      if (mounted && _abort == abort) {
        setState(() {
          _working = false;
          _error = error is ConnectException
              ? error.code == Code.resourceExhausted
                    ? 'The hub is busy. Wait a moment, then try again.'
                    : '${error.message} Check the hub address and your LAN connection.'
              : '$error';
        });
      }
    }
  }

  void _retry() {
    _abort?.cancel();
    if (_approvedSecret == null) {
      _request = null;
      _connecting = null;
    }
    _connect();
  }

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final request = _request;
    return AlertDialog(
      title: Text(
        widget.existing == null ? 'Connect to a hub' : 'Reconnect to hub',
      ),
      content: SizedBox(
        width: 440,
        child: Form(
          key: _form,
          child: SingleChildScrollView(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                if (request == null) ...[
                  TextFormField(
                    key: const ValueKey('hub-url'),
                    controller: _url,
                    enabled: !_working,
                    keyboardType: TextInputType.url,
                    autocorrect: false,
                    decoration: const InputDecoration(
                      labelText: 'Hub URL or QR link',
                      hintText: 'https://hub.example',
                      helperText:
                          'LAN hubs often use :7470. Scan a QR with your camera.',
                      helperMaxLines: 2,
                    ),
                    validator: (value) =>
                        validateBaseUrl(_normalizeUrl(value ?? '')),
                  ),
                  TextFormField(
                    key: const ValueKey('hub-name'),
                    controller: _name,
                    enabled: !_working,
                    decoration: const InputDecoration(
                      labelText: 'Hub label (optional)',
                    ),
                  ),
                  TextFormField(
                    key: const ValueKey('device-name'),
                    controller: _device,
                    enabled: !_working,
                    decoration: const InputDecoration(
                      labelText: 'This device name',
                    ),
                    validator: (value) => (value ?? '').trim().isEmpty
                        ? 'Name this device'
                        : null,
                  ),
                ] else ...[
                  Text('CONFIRM THIS CODE', style: t.legend()),
                  const SizedBox(height: 12),
                  Center(
                    child: SelectableText(
                      request.userCode,
                      key: const ValueKey('device-user-code'),
                      style: t
                          .data(t.p.info)
                          .copyWith(fontSize: 36, letterSpacing: 3),
                    ),
                  ),
                  const SizedBox(height: 16),
                  Text(
                    'On a trusted device, open ${_connecting!.name} and approve only if this code matches. This grants Full access.',
                    style: t.small(),
                  ),
                  const SizedBox(height: 12),
                  Text('First device? Run on the hub:', style: t.small()),
                  const SizedBox(height: 4),
                  SelectableText(
                    'protectorate devices approve ${request.userCode}',
                    style: t.data(),
                  ),
                  const SizedBox(height: 12),
                  Text(
                    'Expires at ${TimeOfDay.fromDateTime(request.expiresAt.toDateTime().toLocal()).format(context)}.',
                    style: t.small(),
                  ),
                  if (_working) ...[
                    const SizedBox(height: 12),
                    const LinearProgressIndicator(),
                    const SizedBox(height: 8),
                    Text(
                      _approvedSecret == null
                          ? 'Waiting for approval...'
                          : 'Approved. Saving connection...',
                      style: t.small(),
                    ),
                  ],
                ],
                if (_error != null) ...[
                  const SizedBox(height: 12),
                  NoteStrip(_error!, tone: Tone.alarm),
                ],
              ],
            ),
          ),
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(),
          child: const Text('CANCEL'),
        ),
        FilledButton(
          key: const ValueKey('hub-connect'),
          onPressed: _working ? null : _retry,
          child: Text(
            _working
                ? 'WAITING'
                : _error != null
                ? 'TRY AGAIN'
                : 'CONNECT',
          ),
        ),
      ],
    );
  }
}
