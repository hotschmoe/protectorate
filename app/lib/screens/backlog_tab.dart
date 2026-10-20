import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../gen/protectorate/v1/backlog.pb.dart';
import '../hubs/event_feed.dart';
import '../hubs/hub_client.dart';
import '../nodes/node_view.dart';
import '../notes/note_draft.dart';
import '../theme/tokens.dart';
import '../vfd/lamp.dart';
import '../vfd/panel.dart';

const _sideBySide = 720.0;
const _listWidth = 340.0;

String sourceLabel(String source) {
  if (source.isEmpty || source == 'owner') return '';
  if (source == 'hub' || source.startsWith('token:')) return 'from $source';
  return 'from sleeve $source';
}

class BacklogTab extends StatefulWidget {
  const BacklogTab({super.key, required this.client, required this.feed});

  final HubClient client;
  final EventFeed feed;

  @override
  State<BacklogTab> createState() => _BacklogTabState();
}

class _BacklogTabState extends State<BacklogTab> {
  List<Note> _notes = const [];
  Object? _error;
  bool _loading = true;
  bool _showDone = false;
  bool _changingState = false;
  String? _filter;
  NoteDraft? _open;
  final _background = <NoteDraft>{};
  StreamSubscription<dynamic>? _sub;

  @override
  void initState() {
    super.initState();
    _sub = widget.feed.incoming.listen((e) {
      if (e.kind.startsWith('note.')) _refresh();
    });
    _refresh();
  }

  @override
  void dispose() {
    _sub?.cancel();
    for (final d in {?_open, ..._background}) {
      if (d.hasUnsaved) unawaited(d.flush());
      d.dispose();
    }
    super.dispose();
  }

  NoteDraft _draft([Note? n]) =>
      NoteDraft(widget.client, note: n, onSaved: _saved);

  void _saved(Note n) {
    if (!mounted) return;
    setState(() {
      _notes = [
        for (final e in _notes)
          if (e.id != n.id) e,
        n,
      ];
    });
  }

  Future<void> _refresh() async {
    try {
      final r = await widget.client.backlog.listNotes(
        ListNotesRequest(state: 'all'),
      );
      if (!mounted) return;
      setState(() {
        _notes = r.notes;
        _error = null;
        final byId = {for (final n in r.notes) n.id: n};
        for (final d in {?_open, ..._background}) {
          final n = byId[d.id];
          if (n != null) d.adopt(n);
        }
        final open = _open;
        if (open != null &&
            open.id != null &&
            !open.hasUnsaved &&
            !byId.containsKey(open.id)) {
          _open = null;
          open.dispose();
        }
      });
    } on Object catch (e) {
      if (mounted) setState(() => _error = e);
    } finally {
      if (mounted) setState(() => _loading = false);
    }
  }

  List<Note> get _sorted {
    final shown = [
      for (final n in _notes)
        if ((_showDone || n.state != 'done') &&
            (_filter == null || n.project == _filter))
          n,
    ];
    shown.sort((a, b) {
      if (a.pinned != b.pinned) return a.pinned ? -1 : 1;
      return b.updatedAt.toDateTime().compareTo(a.updatedAt.toDateTime());
    });
    return shown;
  }

  List<String> get _tags => {
    for (final n in _notes)
      if (n.project.isNotEmpty) n.project,
  }.toList()..sort();

  void _close() {
    final d = _open;
    if (d == null) return;
    setState(() => _open = null);
    _retire(d);
  }

  void _select(Note n) {
    if (_open?.id == n.id) return;
    final held = _background.where((d) => d.id == n.id).firstOrNull;
    final previous = _open;
    setState(() {
      if (held != null) _background.remove(held);
      _open = held ?? _draft(n);
    });
    if (previous != null) _retire(previous);
  }

  // A draft with edits the hub has not taken yet keeps saving in the
  // background after its editor is gone.
  void _retire(NoteDraft d) {
    if (!d.hasUnsaved || d.state == SaveState.rejected || d.blank) {
      d.dispose();
      return;
    }
    _background.add(d);
    void settle() {
      if (d.hasUnsaved) return;
      d.removeListener(settle);
      _background.remove(d);
      scheduleMicrotask(d.dispose);
    }

    d.addListener(settle);
    unawaited(d.flush());
  }

  void _new() {
    final previous = _open;
    setState(() => _open = _draft());
    if (previous != null) _retire(previous);
  }

  Future<void> _changeState(NoteDraft d) async {
    final done = d.noteState != 'done';
    String? resolution;
    if (done) {
      var entered = '';
      resolution = await showDialog<String>(
        context: context,
        builder: (context) => AlertDialog(
          title: Text('Mark note done', style: Tokens.of(context).title()),
          content: TextField(
            autofocus: true,
            onChanged: (value) => entered = value,
            maxLength: 200,
            inputFormatters: [_SingleLine()],
            style: Tokens.of(context).data(),
            decoration: const InputDecoration(
              labelText: 'Resolution (optional)',
              hintText: 'implemented in 8b98d58',
            ),
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.of(context).pop(),
              child: const Text('CANCEL'),
            ),
            FilledButton(
              onPressed: () => Navigator.of(context).pop(entered),
              child: const Text('DONE'),
            ),
          ],
        ),
      );
      if (resolution == null || !mounted) return;
    }
    if (!mounted) return;
    setState(() => _changingState = true);
    try {
      await d.flush();
      if (d.id == null || d.hasUnsaved) {
        throw StateError('Save the note before changing its state.');
      }
      final r = await widget.client.backlog.updateNote(
        UpdateNoteRequest(
          id: d.id,
          state: done ? 'done' : 'open',
          resolution: resolution,
        ),
      );
      if (!mounted) return;
      d.adopt(r.note);
      _saved(r.note);
    } on Object catch (e) {
      if (mounted) setState(() => _error = e);
    } finally {
      if (mounted) setState(() => _changingState = false);
    }
  }

  Future<void> _delete(NoteDraft d) async {
    final ok = await showDialog<bool>(
      context: context,
      builder: (_) => AlertDialog(
        title: const Text('Delete this note?'),
        content: Text(
          d.title.isEmpty ? 'It has no title.' : d.title,
          maxLines: 3,
          overflow: TextOverflow.ellipsis,
        ),
        actions: [
          TextButton(
            key: const ValueKey('note-delete-cancel'),
            onPressed: () => Navigator.of(context).pop(false),
            child: const Text('CANCEL'),
          ),
          FilledButton(
            key: const ValueKey('note-delete-confirm'),
            onPressed: () => Navigator.of(context).pop(true),
            child: const Text('DELETE'),
          ),
        ],
      ),
    );
    if (ok != true || !mounted) return;
    final id = d.id;
    try {
      if (id != null) {
        await widget.client.backlog.deleteNote(DeleteNoteRequest(id: id));
      }
    } on Object catch (e) {
      if (mounted) setState(() => _error = e);
      return;
    }
    if (!mounted) return;
    setState(() {
      _notes = [
        for (final n in _notes)
          if (n.id != id) n,
      ];
      _open = null;
    });
    d.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    return LayoutBuilder(
      builder: (context, box) {
        final wide = box.maxWidth >= _sideBySide;
        final editing = _open != null;
        final Widget content = wide
            ? Row(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  SizedBox(width: _listWidth, child: _list(t, wide)),
                  Expanded(child: _editorPane(t, wide)),
                ],
              )
            : editing
            ? _editorPane(t, wide)
            : _list(t, wide);
        return PopScope(
          canPop: wide || !editing,
          onPopInvokedWithResult: (didPop, _) {
            if (!didPop) _close();
          },
          child: Scaffold(
            floatingActionButton: wide || editing
                ? null
                : FloatingActionButton.extended(
                    heroTag: null,
                    key: const ValueKey('add-note'),
                    onPressed: _new,
                    icon: const Icon(Icons.add),
                    label: const Text('NEW NOTE'),
                  ),
            body: Column(
              children: [
                if (_loading) const BusyBar(),
                if (_error != null)
                  Padding(
                    padding: const EdgeInsets.fromLTRB(12, 12, 12, 0),
                    child: NoteStrip('$_error', tone: Tone.alarm),
                  ),
                Expanded(child: content),
              ],
            ),
          ),
        );
      },
    );
  }

  Widget _list(Tokens t, bool wide) {
    final notes = _sorted;
    final tags = _tags;
    return RefreshIndicator(
      onRefresh: _refresh,
      child: ListView(
        physics: const AlwaysScrollableScrollPhysics(),
        padding: EdgeInsets.fromLTRB(
          Tokens.gap,
          Tokens.gap,
          wide ? 0 : Tokens.gap,
          96,
        ),
        children: [
          SwitchListTile.adaptive(
            contentPadding: EdgeInsets.zero,
            title: Text('SHOW DONE', style: t.legend()),
            value: _showDone,
            onChanged: (value) => setState(() => _showDone = value),
          ),
          if (wide)
            Padding(
              padding: const EdgeInsets.only(bottom: Tokens.gap),
              child: FilledButton.icon(
                key: const ValueKey('add-note'),
                onPressed: _new,
                icon: const Icon(Icons.add),
                label: const Text('NEW NOTE'),
              ),
            ),
          if (tags.isNotEmpty)
            Padding(
              padding: const EdgeInsets.only(bottom: Tokens.gap),
              child: Wrap(
                spacing: 6,
                runSpacing: 6,
                children: [
                  TagChip(
                    key: const ValueKey('tag-all'),
                    label: 'ALL',
                    selected: _filter == null,
                    onTap: () => setState(() => _filter = null),
                  ),
                  for (final tag in tags)
                    TagChip(
                      key: ValueKey('tag-$tag'),
                      label: tag,
                      selected: _filter == tag,
                      onTap: () => setState(() => _filter = tag),
                    ),
                ],
              ),
            ),
          Center(
            child: ConstrainedBox(
              constraints: const BoxConstraints(
                maxWidth: Tokens.contentMaxWidth,
              ),
              child: Panel(
                padding: const EdgeInsets.all(4),
                legend: 'NOTES',
                trailing: Text('${notes.length}', style: t.legend()),
                child: notes.isEmpty
                    ? Padding(
                        padding: const EdgeInsets.all(24),
                        child: Center(
                          child: Text(
                            _loading
                                ? ''
                                : _filter != null
                                ? 'No notes tagged $_filter.'
                                : 'No ${_showDone ? '' : 'open '}notes. Start one.',
                            style: t.small(),
                          ),
                        ),
                      )
                    : Column(
                        crossAxisAlignment: CrossAxisAlignment.stretch,
                        children: [
                          for (var i = 0; i < notes.length; i++)
                            _row(t, i, notes[i], wide),
                        ],
                      ),
              ),
            ),
          ),
        ],
      ),
    );
  }

  Widget _row(Tokens t, int i, Note n, bool wide) {
    final selected = _open?.id == n.id;
    final heading = n.title.isNotEmpty ? n.title : _firstLine(n.body);
    final snippet = n.title.isEmpty ? '' : _firstLine(n.body);
    final from = sourceLabel(n.source);
    return InkWell(
      key: ValueKey('note-${n.id}'),
      borderRadius: BorderRadius.circular(Tokens.innerRadius),
      onTap: () => _select(n),
      child: BarRow(
        index: i,
        padding: const EdgeInsets.fromLTRB(10, 8, 10, 8),
        child: Container(
          decoration: selected && wide
              ? BoxDecoration(
                  border: Border(
                    left: BorderSide(color: t.tone(Tone.info), width: 2),
                  ),
                )
              : null,
          padding: EdgeInsets.only(left: selected && wide ? 8 : 0),
          constraints: const BoxConstraints(minHeight: 40),
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              if (n.pinned)
                Padding(
                  padding: const EdgeInsets.only(right: 8, top: 1),
                  child: Icon(
                    Icons.push_pin,
                    size: 14,
                    color: t.tone(Tone.warn),
                    semanticLabel: 'Pinned',
                  ),
                ),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      heading,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: t.data(t.p.emph),
                    ),
                    if (snippet.isNotEmpty)
                      Text(
                        snippet,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: t.small(),
                      ),
                    if (n.state == 'done' && n.completion.resolution.isNotEmpty)
                      Text(n.completion.resolution, style: t.small()),
                    const SizedBox(height: 4),
                    Wrap(
                      spacing: 8,
                      runSpacing: 4,
                      crossAxisAlignment: WrapCrossAlignment.center,
                      children: [
                        if (n.state == 'done')
                          Text('DONE', style: t.small(t.tone(Tone.ok))),
                        if (n.project.isNotEmpty) TagChip(label: n.project),
                        if (from.isNotEmpty)
                          Text(from, style: t.small(t.tone(Tone.info))),
                        Text(
                          formatAge(
                            DateTime.now().difference(n.updatedAt.toDateTime()),
                          ),
                          style: t.small(),
                        ),
                      ],
                    ),
                  ],
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }

  Widget _editorPane(Tokens t, bool wide) {
    final d = _open;
    if (d == null) {
      return Padding(
        padding: const EdgeInsets.all(Tokens.gap),
        child: Panel(
          legend: 'NOTE',
          child: SizedBox(
            height: 160,
            child: Center(
              child: Text('Pick a note or start a new one.', style: t.small()),
            ),
          ),
        ),
      );
    }
    return NoteEditor(
      key: ObjectKey(d),
      draft: d,
      tags: _tags,
      wide: wide,
      onBack: _close,
      onDelete: () => _delete(d),
      onChangeState: _changingState ? null : () => _changeState(d),
    );
  }
}

String _firstLine(String s) {
  for (final l in s.split('\n')) {
    final v = l.trim();
    if (v.isNotEmpty) return v;
  }
  return '';
}

class TagChip extends StatelessWidget {
  const TagChip({
    super.key,
    required this.label,
    this.selected = false,
    this.onTap,
  });

  final String label;
  final bool selected;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final c = selected ? t.tone(Tone.info) : t.p.legend;
    final chip = Container(
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 2),
      decoration: BoxDecoration(
        color: selected ? t.tint(c) : null,
        border: Border.all(color: c.withValues(alpha: selected ? 0.8 : 0.5)),
        borderRadius: BorderRadius.circular(Tokens.innerRadius + 2),
      ),
      child: Text(label, style: t.small(c)),
    );
    if (onTap == null) return chip;
    return Semantics(
      button: true,
      selected: selected,
      child: InkWell(
        borderRadius: BorderRadius.circular(Tokens.innerRadius + 2),
        onTap: onTap,
        child: ConstrainedBox(
          constraints: const BoxConstraints(minHeight: 32),
          child: Align(widthFactor: 1, child: chip),
        ),
      ),
    );
  }
}

class NoteEditor extends StatefulWidget {
  const NoteEditor({
    super.key,
    required this.draft,
    required this.tags,
    required this.wide,
    required this.onBack,
    required this.onDelete,
    required this.onChangeState,
  });

  final NoteDraft draft;
  final List<String> tags;
  final bool wide;
  final VoidCallback onBack;
  final VoidCallback onDelete;
  final VoidCallback? onChangeState;

  @override
  State<NoteEditor> createState() => _NoteEditorState();
}

class _NoteEditorState extends State<NoteEditor> {
  late final _title = TextEditingController(text: widget.draft.title);
  late final _body = TextEditingController(text: widget.draft.body);
  late final _project = TextEditingController(text: widget.draft.project);

  NoteDraft get _d => widget.draft;

  @override
  void initState() {
    super.initState();
    _d.addListener(_sync);
  }

  @override
  void dispose() {
    _d.removeListener(_sync);
    _title.dispose();
    _body.dispose();
    _project.dispose();
    super.dispose();
  }

  // Text the hub changed under an editor with no local edits.
  void _sync() {
    if (!mounted) return;
    if (_title.text != _d.title) _title.text = _d.title;
    if (_body.text != _d.body) _body.text = _d.body;
    if (_project.text != _d.project) _project.text = _d.project;
    setState(() {});
  }

  (String, Tone, bool) _status() => switch (_d.state) {
    SaveState.saved => ('SAVED', Tone.ok, true),
    SaveState.edited => ('EDITED', Tone.info, true),
    SaveState.saving => ('SAVING', Tone.info, true),
    SaveState.offline => ('OFFLINE, RETRYING', Tone.warn, true),
    SaveState.rejected => ('NOT SAVED', Tone.alarm, true),
  };

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final (label, tone, lit) = _status();
    final from = sourceLabel(_d.source);
    final suggestions = [
      for (final tag in widget.tags)
        if (tag != _d.project) tag,
    ];
    return LayoutBuilder(
      builder: (context, box) {
        final lines = ((box.maxHeight - 340) / 22).floor().clamp(5, 40);
        return SingleChildScrollView(
          padding: const EdgeInsets.all(Tokens.gap),
          child: Center(
            child: ConstrainedBox(
              constraints: const BoxConstraints(
                maxWidth: Tokens.contentMaxWidth,
              ),
              child: Panel(
                legend: _d.id == null ? 'NEW NOTE' : 'NOTE ${_d.id}',
                trailing: Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Annunciator(
                      key: const ValueKey('note-status'),
                      label: label,
                      tone: tone,
                      lit: lit,
                      blink: _d.state == SaveState.offline,
                    ),
                    IconButton(
                      key: const ValueKey('note-pin'),
                      tooltip: _d.pinned ? 'Unpin' : 'Pin to the top',
                      isSelected: _d.pinned,
                      icon: const Icon(Icons.push_pin_outlined),
                      selectedIcon: const Icon(Icons.push_pin),
                      onPressed: () => _d.edit(pinned: !_d.pinned),
                    ),
                    IconButton(
                      key: const ValueKey('note-delete'),
                      tooltip: 'Delete note',
                      icon: const Icon(Icons.delete_outline),
                      onPressed: widget.onDelete,
                    ),
                  ],
                ),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    Row(
                      children: [
                        if (!widget.wide)
                          IconButton(
                            key: const ValueKey('note-back'),
                            tooltip: 'Back to notes',
                            icon: const Icon(Icons.arrow_back),
                            onPressed: widget.onBack,
                          ),
                        Expanded(
                          child: TextField(
                            key: const ValueKey('note-title'),
                            controller: _title,
                            maxLength: 200,
                            maxLengthEnforcement: MaxLengthEnforcement.enforced,
                            inputFormatters: [_SingleLine()],
                            style: t.title(t.p.emph),
                            decoration: const InputDecoration(
                              hintText: 'Title',
                              counterText: '',
                            ),
                            textInputAction: TextInputAction.next,
                            onChanged: (v) => _d.edit(title: v),
                          ),
                        ),
                      ],
                    ),
                    const SizedBox(height: Tokens.gapS),
                    Wrap(
                      spacing: Tokens.gapS,
                      runSpacing: Tokens.gapS,
                      crossAxisAlignment: WrapCrossAlignment.center,
                      children: [
                        Text(_d.noteState.toUpperCase(), style: t.legend()),
                        TextButton.icon(
                          onPressed: widget.onChangeState,
                          icon: Icon(
                            _d.noteState == 'done' ? Icons.undo : Icons.check,
                          ),
                          label: Text(
                            _d.noteState == 'done' ? 'REOPEN' : 'DONE',
                          ),
                        ),
                      ],
                    ),
                    if (_d.completion case final completion?) ...[
                      Text(
                        'Done by ${completion.doneBy} at '
                        '${completion.doneAt.toDateTime().toLocal()}',
                        style: t.small(),
                      ),
                      if (completion.resolution.isNotEmpty)
                        Text(completion.resolution, style: t.data()),
                      const SizedBox(height: Tokens.gapS),
                    ],
                    TextField(
                      key: const ValueKey('note-body'),
                      controller: _body,
                      minLines: lines,
                      maxLines: null,
                      keyboardType: TextInputType.multiline,
                      style: t.body(t.p.emph),
                      decoration: const InputDecoration(
                        hintText: 'Write in markdown',
                      ),
                      onChanged: (v) => _d.edit(body: v),
                    ),
                    const SizedBox(height: Tokens.gapS),
                    TextField(
                      key: const ValueKey('note-project'),
                      controller: _project,
                      style: t.data(),
                      decoration: const InputDecoration(
                        labelText: 'Project tag',
                        hintText: 'owner/repo',
                      ),
                      inputFormatters: [_SingleLine()],
                      onChanged: (v) => _d.edit(project: v.trim()),
                    ),
                    if (suggestions.isNotEmpty)
                      Padding(
                        padding: const EdgeInsets.only(top: Tokens.gapS),
                        child: Wrap(
                          spacing: 6,
                          runSpacing: 6,
                          children: [
                            for (final tag in suggestions)
                              TagChip(
                                key: ValueKey('suggest-$tag'),
                                label: tag,
                                onTap: () {
                                  _project.text = tag;
                                  _d.edit(project: tag);
                                },
                              ),
                          ],
                        ),
                      ),
                    const SizedBox(height: Tokens.gapS),
                    Text(
                      [
                        if (from.isNotEmpty) from,
                        if (_d.updated != null)
                          'updated ${formatAge(DateTime.now().difference(_d.updated!))}',
                        if (_d.state == SaveState.rejected && _d.error != null)
                          _d.error!,
                      ].join('  '),
                      style: t.small(),
                    ),
                  ],
                ),
              ),
            ),
          ),
        );
      },
    );
  }
}

class _SingleLine extends TextInputFormatter {
  @override
  TextEditingValue formatEditUpdate(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) {
    if (!newValue.text.contains('\n')) return newValue;
    final text = newValue.text.replaceAll('\n', ' ');
    return newValue.copyWith(text: text);
  }
}
