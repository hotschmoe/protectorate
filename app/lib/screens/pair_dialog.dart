import 'package:flutter/material.dart';

import '../hubs/connection.dart';
import '../hubs/hub_client.dart';
import '../hubs/hub_profile.dart';
import '../hubs/pairing.dart';
import '../theme/tokens.dart';
import '../vfd/panel.dart';

/// Connects with a pasted pairing link or a hub URL and token. Given a
/// `token`, the dialog connects at once.
Future<HubProfile?> connectHub(
  BuildContext context, {
  required HubClient Function(HubProfile) clientFactory,
  required Future<void> Function(HubProfile) save,
  HubProfile? existing,
  String? baseUrl,
  String token = '',
}) => showDialog<HubProfile>(
  context: context,
  barrierDismissible: false,
  builder: (_) => _ConnectDialog(
    clientFactory: clientFactory,
    save: save,
    existing: existing,
    baseUrl: baseUrl,
    token: token,
  ),
);

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
    this.token = '',
  });

  final HubClient Function(HubProfile) clientFactory;
  final Future<void> Function(HubProfile) save;
  final HubProfile? existing;
  final String? baseUrl;
  final String token;

  @override
  State<_ConnectDialog> createState() => _ConnectDialogState();
}

class _ConnectDialogState extends State<_ConnectDialog> {
  final _form = GlobalKey<FormState>();
  late final _name = TextEditingController(text: widget.existing?.name ?? '');
  late final _url = TextEditingController(
    text: widget.baseUrl ?? widget.existing?.baseUrl ?? '',
  );
  late final _token = TextEditingController(
    text: widget.token.isNotEmpty ? widget.token : widget.existing?.token ?? '',
  );
  bool _working = false;
  String? _error;

  @override
  void initState() {
    super.initState();
    if (widget.token.isNotEmpty) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted) _connect();
      });
    }
  }

  @override
  void dispose() {
    _name.dispose();
    _url.dispose();
    _token.dispose();
    super.dispose();
  }

  // A pasted link fills both fields, so its token never shows in clear text.
  void _absorbLink(String value) {
    final uri = Uri.tryParse(value.trim());
    final link = uri == null ? null : parsePairing(uri);
    if (link == null || link.token.isEmpty) return;
    _url.text = link.baseUrl;
    _token.text = link.token;
  }

  Future<void> _connect() async {
    if (!_form.currentState!.validate()) return;
    final baseUrl = _normalizeUrl(_url.text);
    final profile = HubProfile(
      id:
          widget.existing?.id ??
          DateTime.now().microsecondsSinceEpoch.toString(),
      name: _name.text.trim().isEmpty
          ? Uri.parse(baseUrl).host
          : _name.text.trim(),
      baseUrl: baseUrl,
      token: _token.text.trim(),
    );
    setState(() {
      _working = true;
      _error = null;
    });
    try {
      await probeHub(widget.clientFactory(profile).hub, baseUrl);
      if (!mounted) return;
      await widget.save(profile);
      if (mounted) Navigator.of(context).pop(profile);
    } on Object catch (error) {
      if (mounted) {
        setState(() {
          _working = false;
          _error = '$error';
        });
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
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
                TextFormField(
                  key: const ValueKey('hub-url'),
                  controller: _url,
                  enabled: !_working,
                  keyboardType: TextInputType.url,
                  autocorrect: false,
                  onChanged: _absorbLink,
                  decoration: const InputDecoration(
                    labelText: 'Pairing link or hub URL',
                    hintText: 'https://hub.example',
                    helperText:
                        'Paste the link from protectorate devices add or from '
                        'DEVICES on another device.',
                    helperMaxLines: 2,
                  ),
                  validator: (value) =>
                      validateBaseUrl(_normalizeUrl(value ?? '')),
                ),
                const SizedBox(height: 8),
                TextFormField(
                  key: const ValueKey('hub-token'),
                  controller: _token,
                  enabled: !_working,
                  obscureText: true,
                  autocorrect: false,
                  enableSuggestions: false,
                  onChanged: _absorbLink,
                  decoration: const InputDecoration(
                    labelText: 'Device token',
                    hintText: 'pd1.',
                  ),
                  validator: (value) {
                    final token = (value ?? '').trim();
                    if (token.isEmpty) return 'Paste the device token';
                    if (token.contains(RegExp(r'\s'))) {
                      return 'A token has no spaces';
                    }
                    return null;
                  },
                ),
                TextFormField(
                  key: const ValueKey('hub-name'),
                  controller: _name,
                  enabled: !_working,
                  decoration: const InputDecoration(
                    labelText: 'Hub label (optional)',
                  ),
                ),
                if (_working) ...[
                  const SizedBox(height: 16),
                  const LinearProgressIndicator(),
                  const SizedBox(height: 8),
                  Text('Checking the token with the hub...', style: t.small()),
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
          onPressed: _working ? null : _connect,
          child: Text(
            _working
                ? 'CONNECTING'
                : _error != null
                ? 'TRY AGAIN'
                : 'CONNECT',
          ),
        ),
      ],
    );
  }
}
