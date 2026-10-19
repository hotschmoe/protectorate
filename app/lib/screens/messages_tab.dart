import 'dart:async';

import 'package:flutter/material.dart';

import '../gen/protectorate/v1/directory.pb.dart';
import '../gen/protectorate/v1/events.pb.dart';
import '../gen/protectorate/v1/message.pb.dart';
import '../hubs/event_feed.dart';
import '../hubs/hub_client.dart';
import '../mail/thread_view.dart';
import '../nodes/node_view.dart';
import '../theme/tokens.dart';
import '../vfd/lamp.dart';
import '../vfd/panel.dart';

const mailPaneBreakpoint = 1000.0;

// The hub's mailbox: every thread between sleeves, the orchestrator and the
// owner, newest first, with what is still open.
class MessagesTab extends StatefulWidget {
  const MessagesTab({
    super.key,
    required this.client,
    required this.feed,
    this.now,
  });

  final HubClient client;
  final EventFeed feed;
  final DateTime Function()? now;

  @override
  State<MessagesTab> createState() => _MessagesTabState();
}

class _MessagesTabState extends State<MessagesTab> {
  List<ThreadSummary> _threads = const [];
  List<DirectoryProject> _projects = const [];
  String _sleeve = '';
  String _project = '';
  String? _selected;
  Object? _error;
  bool _loading = true;
  bool _wide = false;
  StreamSubscription<Event>? _sub;
  Timer? _debounce;

  DateTime _now() => (widget.now ?? DateTime.now)();

  @override
  void initState() {
    super.initState();
    _sub = widget.feed.incoming.listen((e) {
      if (!isMailEvent(e)) return;
      _debounce?.cancel();
      _debounce = Timer(const Duration(milliseconds: 300), _refresh);
    });
    _refresh();
    _loadProjects();
  }

  @override
  void dispose() {
    _sub?.cancel();
    _debounce?.cancel();
    super.dispose();
  }

  Future<void> _refresh() async {
    try {
      final r = await widget.client.messages.listThreads(
        ListThreadsRequest(sleeve: _sleeve, project: _project),
      );
      if (mounted) {
        setState(() {
          _threads = r.threads;
          _error = null;
        });
      }
    } on Object catch (e) {
      if (mounted) setState(() => _error = e);
    } finally {
      if (mounted) setState(() => _loading = false);
    }
  }

  Future<void> _loadProjects() async {
    try {
      final r = await widget.client.directory.listProjects(
        ListProjectsRequest(),
      );
      if (mounted) setState(() => _projects = r.projects);
    } on Object {
      return;
    }
  }

  List<String> get _sleeves {
    final out = <String>{
      for (final p in _projects)
        for (final o in p.owners) o.sleeve,
      for (final th in _threads)
        for (final p in th.participants)
          if (p != ownerAddress && p != hubSender && !p.startsWith('project:'))
            p,
      if (_sleeve.isNotEmpty) _sleeve,
    }.toList()..sort();
    return out;
  }

  void _filter({String? sleeve, String? project}) {
    setState(() {
      _sleeve = sleeve ?? _sleeve;
      _project = project ?? _project;
      _loading = true;
    });
    _refresh();
  }

  Future<void> _compose() async {
    final sent = await showDialog<String>(
      context: context,
      builder: (_) => ComposeDialog(
        client: widget.client,
        sleeves: _sleeves,
        projects: _projects,
      ),
    );
    if (sent == null || !mounted) return;
    await _refresh();
    if (mounted) _open(sent);
  }

  void _open(String thread) {
    if (_wide) {
      setState(() => _selected = thread);
      return;
    }
    Navigator.of(context).push(
      MaterialPageRoute<void>(
        builder: (_) => Scaffold(
          appBar: AppBar(title: Text('Thread $thread')),
          body: SafeArea(
            child: ThreadView(
              client: widget.client,
              feed: widget.feed,
              threadId: thread,
              onChanged: _refresh,
              now: widget.now,
            ),
          ),
        ),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(
      builder: (context, box) {
        final wide = _wide = box.maxWidth >= mailPaneBreakpoint;
        final list = _ThreadList(
          threads: _threads,
          loading: _loading,
          error: _error,
          now: _now(),
          selected: wide ? _selected : null,
          sleeves: _sleeves,
          projects: _projects,
          sleeve: _sleeve,
          project: _project,
          onFilter: _filter,
          onOpen: _open,
          onRefresh: _refresh,
          onCompose: _compose,
        );
        if (!wide) return list;
        final t = Tokens.of(context);
        final sel = _selected;
        return Row(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            SizedBox(width: 440, child: list),
            VerticalDivider(width: 1, color: t.p.bezel),
            Expanded(
              child: sel == null
                  ? Center(child: Text('SELECT A THREAD', style: t.legend()))
                  : ThreadView(
                      key: ValueKey('thread-$sel'),
                      client: widget.client,
                      feed: widget.feed,
                      threadId: sel,
                      onChanged: _refresh,
                      now: widget.now,
                    ),
            ),
          ],
        );
      },
    );
  }
}

class _ThreadList extends StatelessWidget {
  const _ThreadList({
    required this.threads,
    required this.loading,
    required this.error,
    required this.now,
    required this.selected,
    required this.sleeves,
    required this.projects,
    required this.sleeve,
    required this.project,
    required this.onFilter,
    required this.onOpen,
    required this.onRefresh,
    required this.onCompose,
  });

  final List<ThreadSummary> threads;
  final bool loading;
  final Object? error;
  final DateTime now;
  final String? selected;
  final List<String> sleeves;
  final List<DirectoryProject> projects;
  final String sleeve;
  final String project;
  final void Function({String? sleeve, String? project}) onFilter;
  final ValueChanged<String> onOpen;
  final Future<void> Function() onRefresh;
  final VoidCallback onCompose;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final unread = threads.fold<int>(0, (n, th) => n + th.unread);
    final projectName = projects
        .where((p) => p.id == project)
        .map((p) => p.display)
        .firstOrNull;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(
            Tokens.gap,
            Tokens.gap,
            Tokens.gap,
            0,
          ),
          child: Row(
            children: [
              Expanded(
                child: Wrap(
                  spacing: 6,
                  runSpacing: 6,
                  children: [
                    _FilterKey(
                      key: const ValueKey('filter-sleeve'),
                      legend: 'SLEEVE',
                      value: sleeve,
                      options: [('', 'ALL'), for (final s in sleeves) (s, s)],
                      onSelected: (v) => onFilter(sleeve: v),
                    ),
                    _FilterKey(
                      key: const ValueKey('filter-project'),
                      legend: 'PROJECT',
                      value: projectName ?? project,
                      options: [
                        ('', 'ALL'),
                        for (final p in projects) (p.id, p.display),
                      ],
                      onSelected: (v) => onFilter(project: v),
                    ),
                  ],
                ),
              ),
              const SizedBox(width: 8),
              FilledButton(
                key: const ValueKey('mail-new'),
                onPressed: onCompose,
                child: const Text('NEW'),
              ),
            ],
          ),
        ),
        if (loading) const BusyBar(),
        if (error != null)
          Padding(
            padding: const EdgeInsets.fromLTRB(12, 12, 12, 0),
            child: NoteStrip('$error', tone: Tone.alarm),
          ),
        Expanded(
          child: RefreshIndicator(
            onRefresh: onRefresh,
            child: ListView(
              physics: const AlwaysScrollableScrollPhysics(),
              padding: const EdgeInsets.all(Tokens.gap),
              children: [
                Center(
                  child: ConstrainedBox(
                    constraints: const BoxConstraints(
                      maxWidth: Tokens.contentMaxWidth,
                    ),
                    child: Panel(
                      legend: 'THREADS',
                      trailing: unread > 0
                          ? LampLegend(label: '$unread UNREAD', tone: Tone.info)
                          : Text('${threads.length}', style: t.legend()),
                      padding: const EdgeInsets.all(4),
                      child: threads.isEmpty
                          ? Padding(
                              padding: const EdgeInsets.all(12),
                              child: Text(
                                loading
                                    ? ''
                                    : sleeve.isEmpty && project.isEmpty
                                    ? 'No messages yet. Sleeves, the orchestrator and you write here.'
                                    : 'No threads match the filter.',
                                style: t.small(),
                              ),
                            )
                          : Column(
                              children: [
                                for (var i = 0; i < threads.length; i++)
                                  ThreadRow(
                                    key: ValueKey(
                                      'thread-row-${threads[i].threadId}',
                                    ),
                                    index: i,
                                    thread: threads[i],
                                    now: now,
                                    selected: threads[i].threadId == selected,
                                    onTap: () => onOpen(threads[i].threadId),
                                  ),
                              ],
                            ),
                    ),
                  ),
                ),
              ],
            ),
          ),
        ),
      ],
    );
  }
}

// One thread in the list: who, the last word, and its lamps.
class ThreadRow extends StatelessWidget {
  const ThreadRow({
    super.key,
    required this.index,
    required this.thread,
    required this.now,
    required this.onTap,
    this.selected = false,
  });

  final int index;
  final ThreadSummary thread;
  final DateTime now;
  final VoidCallback onTap;
  final bool selected;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final th = thread;
    final last = th.last;
    final age = last.hasCreatedAt()
        ? formatAge(now.difference(last.createdAt.toDateTime()))
        : '';
    final row = BarRow(
      index: index,
      padding: const EdgeInsets.fromLTRB(8, 8, 8, 8),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Padding(
            padding: const EdgeInsets.only(top: 4),
            child: Lamp(
              key: ValueKey('unread-${th.threadId}'),
              tone: Tone.info,
              on: th.unread > 0,
            ),
          ),
          const SizedBox(width: 10),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  children: [
                    Expanded(
                      child: Text(
                        th.participants.map(partyLabel).join(', '),
                        style: t
                            .data(th.unread > 0 ? t.p.emph : t.p.lit)
                            .copyWith(
                              fontWeight: th.unread > 0
                                  ? FontWeight.w700
                                  : null,
                            ),
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                      ),
                    ),
                    const SizedBox(width: 8),
                    Text(age, style: t.small()),
                  ],
                ),
                const SizedBox(height: 2),
                Text.rich(
                  TextSpan(
                    children: [
                      TextSpan(
                        text: '${last.from}: ',
                        style: t.small(t.p.legend),
                      ),
                      TextSpan(text: firstLine(last.body), style: t.small()),
                    ],
                  ),
                  maxLines: 2,
                  overflow: TextOverflow.ellipsis,
                ),
                const SizedBox(height: 6),
                Wrap(
                  spacing: 6,
                  runSpacing: 6,
                  children: [
                    Text(
                      '${th.threadId}  ${th.messageCount} MSG',
                      style: t.legend(),
                    ),
                    if (th.unread > 0)
                      Annunciator(
                        label: '${th.unread} UNREAD',
                        tone: Tone.info,
                      ),
                    if (th.openAcks > 0)
                      Annunciator(
                        key: ValueKey('acks-${th.threadId}'),
                        label: '${th.openAcks} ACK DUE',
                        tone: Tone.warn,
                      ),
                    if (th.escalated > 0)
                      Annunciator(
                        key: ValueKey('escalated-${th.threadId}'),
                        label: 'ESCALATED',
                        tone: Tone.alarm,
                      ),
                  ],
                ),
              ],
            ),
          ),
        ],
      ),
    );
    return Semantics(
      button: true,
      selected: selected,
      child: InkWell(
        borderRadius: BorderRadius.circular(Tokens.innerRadius),
        onTap: onTap,
        child: selected
            ? DecoratedBox(
                decoration: BoxDecoration(
                  color: t.tint(t.p.info),
                  border: Border(left: BorderSide(color: t.p.info, width: 2)),
                ),
                child: row,
              )
            : row,
      ),
    );
  }
}

class _FilterKey extends StatelessWidget {
  const _FilterKey({
    super.key,
    required this.legend,
    required this.value,
    required this.options,
    required this.onSelected,
  });

  final String legend;
  final String value;
  final List<(String, String)> options;
  final ValueChanged<String> onSelected;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final set = value.isNotEmpty;
    final c = set ? t.p.info : t.p.legend;
    return PopupMenuButton<String>(
      tooltip: 'Filter by ${legend.toLowerCase()}',
      onSelected: onSelected,
      itemBuilder: (_) => [
        for (final (v, label) in options)
          PopupMenuItem(
            key: ValueKey('filter-$legend-$label'),
            value: v,
            child: Text(label),
          ),
      ],
      child: ConstrainedBox(
        constraints: const BoxConstraints(minHeight: Tokens.touch),
        child: Align(
          widthFactor: 1,
          child: Container(
            padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 6),
            decoration: BoxDecoration(
              color: set ? t.tint(c) : null,
              border: Border.all(color: set ? c : t.p.bezelHi),
              borderRadius: BorderRadius.circular(Tokens.innerRadius),
            ),
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                Text(legend, style: t.legend(c)),
                const SizedBox(width: 8),
                ConstrainedBox(
                  constraints: const BoxConstraints(maxWidth: 140),
                  child: Text(
                    set ? value : 'ALL',
                    style: t.small(set ? t.p.emph : t.p.dim),
                    overflow: TextOverflow.ellipsis,
                  ),
                ),
                Icon(Icons.arrow_drop_down, size: 18, color: c),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

// A new thread, written as whoever the token names. Pops with the thread id.
class ComposeDialog extends StatefulWidget {
  const ComposeDialog({
    super.key,
    required this.client,
    required this.sleeves,
    required this.projects,
  });

  final HubClient client;
  final List<String> sleeves;
  final List<DirectoryProject> projects;

  @override
  State<ComposeDialog> createState() => _ComposeDialogState();
}

class _ComposeDialogState extends State<ComposeDialog> {
  final _to = TextEditingController();
  final _body = TextEditingController();
  MessageKind _kind = MessageKind.MESSAGE_KIND_NOTE;
  bool _sending = false;
  String? _error;
  final _requestId = newRequestId();

  @override
  void initState() {
    super.initState();
    _to.addListener(() => setState(() {}));
  }

  @override
  void dispose() {
    _to.dispose();
    _body.dispose();
    super.dispose();
  }

  List<String> get _addresses => [
    'orchestrator',
    for (final s in widget.sleeves)
      if (s != orchestratorSleeve) s,
    for (final p in widget.projects) 'project:${p.display}',
  ];

  Future<void> _send() async {
    final to = _to.text.trim();
    final body = _body.text.trim();
    if (to.isEmpty || body.isEmpty) {
      setState(() => _error = 'An address and a message are required.');
      return;
    }
    setState(() {
      _sending = true;
      _error = null;
    });
    try {
      final r = await widget.client.messages.send(
        MessageServiceSendRequest(
          to: to,
          body: body,
          kind: _kind,
          requestId: _requestId,
        ),
      );
      if (mounted) Navigator.of(context).pop(r.message.threadId);
    } on Object catch (e) {
      if (mounted) {
        setState(() {
          _sending = false;
          _error = mailFailure(e);
        });
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    return AlertDialog(
      title: const Text('New message'),
      scrollable: true,
      content: SizedBox(
        width: 480,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            TextField(
              key: const ValueKey('compose-to'),
              controller: _to,
              decoration: const InputDecoration(
                labelText: 'To: sleeve, project:<name> or orchestrator',
              ),
            ),
            const SizedBox(height: 8),
            ChoiceKeys<String>(
              options: [for (final a in _addresses) (a, a)],
              selected: _to.text.trim(),
              onSelected: _sending ? null : (v) => _to.text = v,
            ),
            const SizedBox(height: 12),
            Text('KIND', style: t.legend()),
            const SizedBox(height: 4),
            ChoiceKeys<MessageKind>(
              options: const [
                (MessageKind.MESSAGE_KIND_NOTE, 'NOTE'),
                (MessageKind.MESSAGE_KIND_REQUEST, 'REQUEST'),
              ],
              selected: _kind,
              onSelected: _sending ? null : (v) => setState(() => _kind = v),
            ),
            Text(
              _kind == MessageKind.MESSAGE_KIND_REQUEST
                  ? 'A request asks for an ack. To a project with no live sleeve it also becomes a backlog item.'
                  : 'A note is information; nobody is asked to act.',
              style: t.small(),
            ),
            const SizedBox(height: 12),
            TextField(
              key: const ValueKey('compose-body'),
              controller: _body,
              minLines: 3,
              maxLines: 10,
              style: t.body(t.p.emph),
              decoration: const InputDecoration(labelText: 'Message'),
            ),
            if (_error != null) ...[
              const SizedBox(height: 12),
              NoteStrip(_error!, tone: Tone.alarm),
            ],
          ],
        ),
      ),
      actions: [
        TextButton(
          onPressed: _sending ? null : () => Navigator.of(context).pop(),
          child: const Text('CANCEL'),
        ),
        FilledButton(
          key: const ValueKey('compose-send'),
          onPressed: _sending ? null : _send,
          child: const Text('SEND'),
        ),
      ],
    );
  }
}
