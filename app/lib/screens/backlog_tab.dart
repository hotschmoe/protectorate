import 'dart:async';

import 'package:flutter/material.dart';

import '../gen/protectorate/v1/backlog.pb.dart';
import '../hubs/event_feed.dart';
import '../hubs/hub_client.dart';

class BacklogTab extends StatefulWidget {
  const BacklogTab({super.key, required this.client, required this.feed});

  final HubClient client;
  final EventFeed feed;

  @override
  State<BacklogTab> createState() => _BacklogTabState();
}

class _BacklogTabState extends State<BacklogTab> {
  List<Item> _items = const [];
  Object? _error;
  bool _loading = true;
  StreamSubscription<dynamic>? _sub;

  @override
  void initState() {
    super.initState();
    _sub = widget.feed.incoming.listen((e) {
      if (e.kind.startsWith('backlog.')) _refresh();
    });
    _refresh();
  }

  @override
  void dispose() {
    _sub?.cancel();
    super.dispose();
  }

  Future<void> _refresh() async {
    try {
      final r = await widget.client.backlog.listItems(ListItemsRequest());
      if (mounted) {
        setState(() {
          _items = r.items;
          _error = null;
        });
      }
    } on Object catch (e) {
      if (mounted) setState(() => _error = e);
    } finally {
      if (mounted) setState(() => _loading = false);
    }
  }

  Future<void> _done(Item it) async {
    try {
      await widget.client.backlog.updateItem(
        UpdateItemRequest(id: it.id, state: ItemState.ITEM_STATE_DONE),
      );
      await _refresh();
    } on Object catch (e) {
      if (mounted) setState(() => _error = e);
    }
  }

  Future<void> _add() async {
    final projects = {for (final i in _items) i.project}.toList()..sort();
    final req = await showDialog<AddItemRequest>(
      context: context,
      builder: (_) => _AddItemDialog(projects: projects),
    );
    if (req == null) return;
    try {
      await widget.client.backlog.addItem(req);
      await _refresh();
    } on Object catch (e) {
      if (mounted) setState(() => _error = e);
    }
  }

  @override
  Widget build(BuildContext context) {
    final groups = <String, List<Item>>{};
    for (final it in _items) {
      groups.putIfAbsent(it.project, () => []).add(it);
    }
    final projects = groups.keys.toList()..sort();
    return Scaffold(
      floatingActionButton: FloatingActionButton.extended(
        key: const ValueKey('add-item'),
        onPressed: _add,
        icon: const Icon(Icons.add),
        label: const Text('Add item'),
      ),
      body: Column(
        children: [
          if (_loading) const LinearProgressIndicator(),
          if (_error != null)
            ListTile(
              leading: const Icon(Icons.error_outline),
              title: Text('$_error'),
            ),
          Expanded(
            child: RefreshIndicator(
              onRefresh: _refresh,
              child: ListView(
                physics: const AlwaysScrollableScrollPhysics(),
                padding: const EdgeInsets.only(bottom: 88),
                children: [
                  if (!_loading && _error == null && _items.isEmpty)
                    const Padding(
                      padding: EdgeInsets.all(32),
                      child: Center(
                        child: Text('Backlog is empty. Add an item to begin.'),
                      ),
                    ),
                  for (final p in projects) ...[
                    Padding(
                      padding: const EdgeInsets.fromLTRB(16, 16, 16, 4),
                      child: Text(
                        p.isEmpty ? '(no project)' : p,
                        style: Theme.of(context).textTheme.titleSmall,
                      ),
                    ),
                    for (final it in groups[p]!) _row(it),
                  ],
                ],
              ),
            ),
          ),
        ],
      ),
    );
  }

  Widget _row(Item it) {
    final done = it.state == ItemState.ITEM_STATE_DONE;
    final title = it.title.isEmpty ? it.issueUrl : it.title;
    final bits = [
      if (it.priority > 0) 'p${it.priority}',
      if (it.harness.isNotEmpty && it.harness != 'any') it.harness,
      if (it.hasClaim()) 'claimed by ${it.claim.by}',
    ];
    return ListTile(
      key: ValueKey('item-${it.id}'),
      title: Text(
        title,
        style: done
            ? const TextStyle(decoration: TextDecoration.lineThrough)
            : null,
      ),
      subtitle: bits.isEmpty ? null : Text(bits.join('  ')),
      leading: _StateChip(it.state),
      trailing: done
          ? null
          : IconButton(
              tooltip: 'Mark done',
              icon: const Icon(Icons.check_circle_outline),
              onPressed: () => _done(it),
            ),
    );
  }
}

class _StateChip extends StatelessWidget {
  const _StateChip(this.state);

  final ItemState state;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final (label, bg, fg) = switch (state) {
      ItemState.ITEM_STATE_TODO => (
        'todo',
        scheme.secondaryContainer,
        scheme.onSecondaryContainer,
      ),
      ItemState.ITEM_STATE_CLAIMED => (
        'claimed',
        scheme.tertiaryContainer,
        scheme.onTertiaryContainer,
      ),
      ItemState.ITEM_STATE_DONE => (
        'done',
        scheme.surfaceContainerHighest,
        scheme.onSurfaceVariant,
      ),
      _ => ('?', scheme.surfaceContainerHighest, scheme.onSurfaceVariant),
    };
    return Container(
      width: 64,
      padding: const EdgeInsets.symmetric(vertical: 4),
      alignment: Alignment.center,
      decoration: BoxDecoration(
        color: bg,
        borderRadius: BorderRadius.circular(12),
      ),
      child: Text(label, style: TextStyle(color: fg, fontSize: 12)),
    );
  }
}

class _AddItemDialog extends StatefulWidget {
  const _AddItemDialog({required this.projects});

  final List<String> projects;

  @override
  State<_AddItemDialog> createState() => _AddItemDialogState();
}

class _AddItemDialogState extends State<_AddItemDialog> {
  late final _project = TextEditingController(
    text: widget.projects.isEmpty ? '' : widget.projects.first,
  );
  final _title = TextEditingController();
  final _prio = TextEditingController();

  @override
  void dispose() {
    _project.dispose();
    _title.dispose();
    _prio.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      title: const Text('Add backlog item'),
      content: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          TextField(
            key: const ValueKey('item-project'),
            controller: _project,
            decoration: const InputDecoration(labelText: 'Project'),
          ),
          TextField(
            key: const ValueKey('item-title'),
            controller: _title,
            decoration: const InputDecoration(labelText: 'Title'),
          ),
          TextField(
            controller: _prio,
            keyboardType: TextInputType.number,
            decoration: const InputDecoration(labelText: 'Priority (1-9)'),
          ),
        ],
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(),
          child: const Text('Cancel'),
        ),
        FilledButton(
          key: const ValueKey('item-add'),
          onPressed: () {
            final project = _project.text.trim();
            final title = _title.text.trim();
            if (project.isEmpty || title.isEmpty) return;
            Navigator.of(context).pop(
              AddItemRequest(
                project: project,
                title: title,
                priority: int.tryParse(_prio.text.trim()) ?? 0,
              ),
            );
          },
          child: const Text('Add'),
        ),
      ],
    );
  }
}
