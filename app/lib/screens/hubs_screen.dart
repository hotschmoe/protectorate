import 'package:flutter/material.dart';

import '../hubs/hub_client.dart';
import '../hubs/hub_profile.dart';
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

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('Hubs')),
      floatingActionButton: FloatingActionButton.extended(
        heroTag: null,
        key: const ValueKey('add-hub'),
        onPressed: () => _edit(context),
        icon: const Icon(Icons.add),
        label: const Text('Add hub'),
      ),
      body: ListenableBuilder(
        listenable: store,
        builder: (context, _) {
          final hubs = store.hubs;
          if (hubs.isEmpty) {
            return const Center(child: Text('No hubs yet. Add one to begin.'));
          }
          return ListView(
            children: [
              for (final hub in hubs)
                ListTile(
                  leading: const Icon(Icons.hub),
                  title: Text(hub.name),
                  subtitle: Text(hub.baseUrl),
                  onTap: () => Navigator.of(context).push(
                    MaterialPageRoute<void>(
                      builder: (_) =>
                          HubShell(hub: hub, clientFactory: clientFactory),
                    ),
                  ),
                  trailing: Row(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      IconButton(
                        tooltip: 'Edit',
                        icon: const Icon(Icons.edit),
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
            ],
          );
        },
      ),
    );
  }
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
          child: const Text('Cancel'),
        ),
        FilledButton(
          key: const ValueKey('hub-save'),
          onPressed: _save,
          child: const Text('Save'),
        ),
      ],
    );
  }
}
