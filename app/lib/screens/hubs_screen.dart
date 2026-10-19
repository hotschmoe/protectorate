import 'package:flutter/material.dart';

import '../hubs/hub_client.dart';
import '../hubs/hub_profile.dart';
import '../theme/tokens.dart';
import '../vfd/panel.dart';
import 'hub_shell.dart';

class HubsScreen extends StatelessWidget {
  const HubsScreen({
    super.key,
    required this.store,
    required this.clientFactory,
  });

  final HubStore store;
  final HubClient Function(HubProfile) clientFactory;

  Future<void> _edit(BuildContext context, [HubProfile? existing]) async {
    final result = await showDialog<HubProfile>(
      context: context,
      builder: (_) => _HubDialog(existing: existing),
    );
    if (result != null) await store.upsert(result);
  }

  void _open(BuildContext context, HubProfile hub) =>
      Navigator.of(context).push(
        MaterialPageRoute<void>(
          builder: (_) => HubShell(hub: hub, clientFactory: clientFactory),
        ),
      );

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    return Scaffold(
      appBar: AppBar(title: const Text('Protectorate')),
      floatingActionButton: FloatingActionButton.extended(
        heroTag: null,
        key: const ValueKey('add-hub'),
        onPressed: () => _edit(context),
        icon: const Icon(Icons.add),
        label: const Text('ADD HUB'),
      ),
      body: ListenableBuilder(
        listenable: store,
        builder: (context, _) {
          final hubs = store.hubs;
          return ListView(
            padding: const EdgeInsets.fromLTRB(
              Tokens.gap,
              Tokens.gapL,
              Tokens.gap,
              96,
            ),
            children: [
              Center(
                child: ConstrainedBox(
                  constraints: const BoxConstraints(
                    maxWidth: Tokens.contentMaxWidth,
                  ),
                  child: Panel(
                    legend: 'HUBS',
                    trailing: Text('${hubs.length} PAIRED', style: t.legend()),
                    padding: const EdgeInsets.all(4),
                    child: hubs.isEmpty
                        ? Padding(
                            padding: const EdgeInsets.all(20),
                            child: Column(
                              children: [
                                Text('NO HUBS', style: t.legend()),
                                const SizedBox(height: 8),
                                Text(
                                  'No hubs yet. Add one, or scan the QR code from protectorate pair.',
                                  textAlign: TextAlign.center,
                                  style: t.small(),
                                ),
                              ],
                            ),
                          )
                        : Column(
                            children: [
                              for (var i = 0; i < hubs.length; i++)
                                _row(context, t, i, hubs[i]),
                            ],
                          ),
                  ),
                ),
              ),
              const SizedBox(height: Tokens.gap),
              Center(
                child: ConstrainedBox(
                  constraints: const BoxConstraints(
                    maxWidth: Tokens.contentMaxWidth,
                  ),
                  child: Panel(
                    legend: 'PAIRING',
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          'On the hub, run the command below and scan its QR '
                          'code, or open the link it prints on this device.',
                          style: t.small(),
                        ),
                        const SizedBox(height: 10),
                        Container(
                          padding: const EdgeInsets.symmetric(
                            horizontal: 12,
                            vertical: 10,
                          ),
                          decoration: BoxDecoration(
                            color: t.p.bar,
                            border: Border.all(color: t.p.bezel),
                            borderRadius: BorderRadius.circular(
                              Tokens.innerRadius,
                            ),
                          ),
                          child: Text.rich(
                            TextSpan(
                              children: [
                                TextSpan(text: '> ', style: t.data(t.p.info)),
                                TextSpan(
                                  text: 'protectorate pair',
                                  style: t.data(t.p.emph),
                                ),
                              ],
                            ),
                          ),
                        ),
                      ],
                    ),
                  ),
                ),
              ),
            ],
          );
        },
      ),
    );
  }

  Widget _row(BuildContext context, Tokens t, int i, HubProfile hub) => InkWell(
    key: ValueKey('hub-${hub.id}'),
    borderRadius: BorderRadius.circular(Tokens.innerRadius),
    onTap: () => _open(context, hub),
    child: BarRow(
      index: i,
      padding: const EdgeInsets.fromLTRB(14, 8, 0, 8),
      child: Row(
        children: [
          Icon(Icons.hub_outlined, color: t.p.lit, size: 22),
          const SizedBox(width: 14),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(hub.name, style: t.title()),
                const SizedBox(height: 2),
                Text(
                  hub.baseUrl,
                  style: t.small(),
                  overflow: TextOverflow.ellipsis,
                ),
                if (hub.token.isEmpty)
                  Text('NO TOKEN', style: t.legend(t.p.warn)),
              ],
            ),
          ),
          IconButton(
            tooltip: 'Edit',
            icon: const Icon(Icons.edit_outlined),
            onPressed: () => _edit(context, hub),
          ),
          IconButton(
            tooltip: 'Remove',
            icon: const Icon(Icons.delete_outline),
            onPressed: () => store.remove(hub.id),
          ),
        ],
      ),
    ),
  );
}

class _HubDialog extends StatefulWidget {
  const _HubDialog({this.existing});

  final HubProfile? existing;

  @override
  State<_HubDialog> createState() => _HubDialogState();
}

class _HubDialogState extends State<_HubDialog> {
  final _form = GlobalKey<FormState>();
  late final _name = TextEditingController(text: widget.existing?.name);
  late final _url = TextEditingController(
    text: widget.existing?.baseUrl ?? 'http://',
  );
  late final _token = TextEditingController(text: widget.existing?.token);

  @override
  void dispose() {
    _name.dispose();
    _url.dispose();
    _token.dispose();
    super.dispose();
  }

  void _save() {
    if (!_form.currentState!.validate()) return;
    var url = _url.text.trim();
    while (url.endsWith('/')) {
      url = url.substring(0, url.length - 1);
    }
    Navigator.of(context).pop(
      HubProfile(
        id:
            widget.existing?.id ??
            DateTime.now().microsecondsSinceEpoch.toString(),
        name: _name.text.trim(),
        baseUrl: url,
        token: _token.text.trim(),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      title: Text(widget.existing == null ? 'Add hub' : 'Edit hub'),
      content: Form(
        key: _form,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            TextFormField(
              key: const ValueKey('hub-name'),
              controller: _name,
              decoration: const InputDecoration(labelText: 'Name'),
              validator: (v) => (v ?? '').trim().isEmpty ? 'Required' : null,
            ),
            TextFormField(
              key: const ValueKey('hub-url'),
              controller: _url,
              keyboardType: TextInputType.url,
              decoration: const InputDecoration(labelText: 'Base URL'),
              validator: (v) => validateBaseUrl(v ?? ''),
            ),
            TextFormField(
              key: const ValueKey('hub-token'),
              controller: _token,
              obscureText: true,
              decoration: const InputDecoration(labelText: 'Token'),
            ),
          ],
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(),
          child: const Text('CANCEL'),
        ),
        FilledButton(
          key: const ValueKey('hub-save'),
          onPressed: _save,
          child: const Text('SAVE'),
        ),
      ],
    );
  }
}
