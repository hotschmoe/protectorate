import 'dart:async';

import 'package:flutter/material.dart';

import '../gen/protectorate/v1/backlog.pb.dart';
import '../hubs/event_feed.dart';
import '../hubs/hub_client.dart';
import '../theme/tokens.dart';
import '../vfd/lamp.dart';
import '../vfd/panel.dart';

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
    final t = Tokens.of(context);
    final groups = <String, List<Item>>{};
    for (final it in _items) {
      groups.putIfAbsent(it.project, () => []).add(it);
    }
    final projects = groups.keys.toList()..sort();
    return Scaffold(
      floatingActionButton: FloatingActionButton.extended(
        heroTag: null,
        key: const ValueKey('add-item'),
        onPressed: _add,
        icon: const Icon(Icons.add),
        label: const Text('ADD ITEM'),
      ),
      body: Column(
        children: [
          if (_loading) const BusyBar(),
          if (_error != null)
            Padding(
              padding: const EdgeInsets.fromLTRB(12, 12, 12, 0),
              child: NoteStrip('$_error', tone: Tone.alarm),
            ),
          Expanded(
            child: RefreshIndicator(
              onRefresh: _refresh,
              child: ListView(
                physics: const AlwaysScrollableScrollPhysics(),
                padding: const EdgeInsets.fromLTRB(
                  Tokens.gap,
                  Tokens.gap,
                  Tokens.gap,
                  96,
                ),
                children: [
                  if (!_loading && _error == null && _items.isEmpty)
                    Padding(
                      padding: const EdgeInsets.all(32),
                      child: Center(
                        child: Text(
                          'Backlog is empty. Add an item to begin.',
                          style: t.small(),
                        ),
                      ),
                    ),
                  for (final p in projects)
                    Center(
                      child: ConstrainedBox(
                        constraints: const BoxConstraints(
                          maxWidth: Tokens.contentMaxWidth,
                        ),
                        child: Padding(
                          padding: const EdgeInsets.only(bottom: Tokens.gap),
                          child: Panel(
                            padding: const EdgeInsets.all(4),
                            trailing: Text(
                              '${groups[p]!.where((i) => i.state != ItemState.ITEM_STATE_DONE).length} OPEN',
                              style: t.legend(),
                            ),
                            legend: '${groups[p]!.length} ITEMS',
                            child: Column(
                              crossAxisAlignment: CrossAxisAlignment.stretch,
                              children: [
                                Padding(
                                  padding: const EdgeInsets.fromLTRB(
                                    10,
                                    8,
                                    10,
                                    6,
                                  ),
                                  child: Text(
                                    p.isEmpty ? '(no project)' : p,
                                    style: t.title(),
                                  ),
                                ),
                                for (var i = 0; i < groups[p]!.length; i++)
                                  _row(t, i, groups[p]![i]),
                              ],
                            ),
                          ),
                        ),
                      ),
                    ),
                ],
              ),
            ),
          ),
        ],
      ),
    );
  }

  Widget _row(Tokens t, int i, Item it) {
    final done = it.state == ItemState.ITEM_STATE_DONE;
    final title = it.title.isEmpty ? it.issueUrl : it.title;
    final bits = [
      if (it.priority > 0) 'p${it.priority}',
      if (it.harness.isNotEmpty && it.harness != 'any') it.harness,
      if (it.hasClaim()) 'claimed by ${it.claim.by}',
    ];
    final (label, tone) = switch (it.state) {
      ItemState.ITEM_STATE_TODO => ('TODO', Tone.lit),
      ItemState.ITEM_STATE_CLAIMED => ('CLAIMED', Tone.info),
      ItemState.ITEM_STATE_DONE => ('DONE', Tone.ok),
      _ => ('?', Tone.dim),
    };
    return BarRow(
      key: ValueKey('item-${it.id}'),
      index: i,
      padding: const EdgeInsets.fromLTRB(10, 4, 0, 4),
      child: Row(
        children: [
          SizedBox(
            width: 82,
            child: Align(
              alignment: Alignment.centerLeft,
              child: Annunciator(label: label, tone: tone, lit: !done),
            ),
          ),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  title,
                  style: done
                      ? t
                            .data(t.p.dim)
                            .copyWith(decoration: TextDecoration.lineThrough)
                      : t.data(t.p.emph),
                ),
                if (bits.isNotEmpty) Text(bits.join('  '), style: t.small()),
              ],
            ),
          ),
          if (!done)
            IconButton(
              tooltip: 'Mark done',
              icon: const Icon(Icons.check_circle_outline),
              onPressed: () => _done(it),
            )
          else
            const SizedBox(width: Tokens.touch),
        ],
      ),
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
          child: const Text('CANCEL'),
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
          child: const Text('ADD'),
        ),
      ],
    );
  }
}
