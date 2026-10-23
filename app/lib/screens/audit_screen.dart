import 'dart:async';

import 'package:connectrpc/connect.dart';
import 'package:flutter/material.dart';

import '../gen/protectorate/v1/audit.pb.dart';
import '../hubs/hub_client.dart';
import '../theme/tokens.dart';
import '../vfd/lamp.dart';
import '../vfd/panel.dart';

const auditTableBreakpoint = 900.0;

String shortProcedure(String p) =>
    p.startsWith('/protectorate.v1.') ? p.substring(17) : p;

Tone outcomeTone(String outcome) => switch (outcome) {
  'ok' => Tone.ok,
  'denied' => Tone.warn,
  'error' => Tone.alarm,
  _ => Tone.dim,
};

String two(int n) => n.toString().padLeft(2, '0');

// HH:MM:SS today, otherwise the date in front.
String auditTime(DateTime t, DateTime now) {
  final l = t.toLocal();
  final n = now.toLocal();
  final clock = '${two(l.hour)}:${two(l.minute)}:${two(l.second)}';
  if (l.year == n.year && l.month == n.month && l.day == n.day) return clock;
  return '${l.year}-${two(l.month)}-${two(l.day)} $clock';
}

String auditFailure(Object e) {
  if (e is ConnectException && e.code == Code.permissionDenied) {
    return 'The audit log is for the owner token only; this device holds '
        'another token.';
  }
  return '$e';
}

// The hub's audit log for the owner: who did what, from where, to what, and
// how it ended. Newest first, a page at a time.
class AuditScreen extends StatefulWidget {
  const AuditScreen({super.key, required this.client, this.now});

  final HubClient client;
  final DateTime Function()? now;

  @override
  State<AuditScreen> createState() => _AuditScreenState();
}

class _AuditScreenState extends State<AuditScreen> {
  final _actor = TextEditingController();
  final _procedure = TextEditingController();
  final _subject = TextEditingController();
  String _outcome = '';
  List<AuditEntry> _entries = const [];
  String _next = '';
  bool _loading = true;
  Object? _error;
  Timer? _debounce;
  int _generation = 0;
  String _last = '||';

  DateTime _now() => (widget.now ?? DateTime.now)();

  @override
  void initState() {
    super.initState();
    for (final c in [_actor, _procedure, _subject]) {
      c.addListener(_typed);
    }
    _load();
  }

  @override
  void dispose() {
    _debounce?.cancel();
    _actor.dispose();
    _procedure.dispose();
    _subject.dispose();
    super.dispose();
  }

  void _typed() {
    final key = '${_actor.text}|${_procedure.text}|${_subject.text}';
    if (key == _last) return;
    _last = key;
    _debounce?.cancel();
    _debounce = Timer(const Duration(milliseconds: 400), _load);
  }

  ListAuditEntriesRequest _request(String token) => ListAuditEntriesRequest(
    limit: 100,
    pageToken: token,
    actor: _actor.text.trim(),
    procedure: _procedure.text.trim(),
    subject: _subject.text.trim(),
    outcome: _outcome,
  );

  // A new filter starts over; replies to an older filter are dropped.
  Future<void> _load({bool older = false}) async {
    final gen = older ? _generation : ++_generation;
    setState(() => _loading = true);
    try {
      final r = await widget.client.audit.listAuditEntries(
        _request(older ? _next : ''),
      );
      if (!mounted || gen != _generation) return;
      setState(() {
        _entries = older ? [..._entries, ...r.entries] : r.entries;
        _next = r.nextPageToken;
        _error = null;
      });
    } on Object catch (e) {
      if (mounted && gen == _generation) setState(() => _error = e);
    } finally {
      if (mounted && gen == _generation) setState(() => _loading = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final denied =
        _error is ConnectException &&
        (_error! as ConnectException).code == Code.permissionDenied;
    return Scaffold(
      appBar: AppBar(
        title: const Text('Audit log'),
        actions: [
          IconButton(
            key: const ValueKey('audit-refresh'),
            tooltip: 'Refresh',
            icon: const Icon(Icons.refresh),
            onPressed: _loading ? null : _load,
          ),
        ],
      ),
      body: LayoutBuilder(
        builder: (context, box) {
          final table = box.maxWidth >= auditTableBreakpoint;
          Widget field(String key, String label, TextEditingController c) =>
              SizedBox(
                width: table ? 220 : (box.maxWidth - 64 - Tokens.gap) / 2,
                child: TextField(
                  key: ValueKey(key),
                  controller: c,
                  style: t.data(t.p.emph),
                  decoration: InputDecoration(labelText: label),
                ),
              );
          return RefreshIndicator(
            onRefresh: _load,
            child: ListView(
              physics: const AlwaysScrollableScrollPhysics(),
              padding: const EdgeInsets.all(Tokens.gap),
              children: [
                Center(
                  child: ConstrainedBox(
                    constraints: const BoxConstraints(
                      maxWidth: Tokens.boardMaxWidth,
                    ),
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.stretch,
                      children: [
                        Panel(
                          legend: 'FILTER',
                          child: Wrap(
                            spacing: Tokens.gap,
                            runSpacing: Tokens.gapS,
                            crossAxisAlignment: WrapCrossAlignment.center,
                            children: [
                              field('audit-actor', 'Actor', _actor),
                              field('audit-procedure', 'Procedure', _procedure),
                              field('audit-subject', 'Subject', _subject),
                              ChoiceKeys<String>(
                                options: const [
                                  ('', 'ALL'),
                                  ('ok', 'OK'),
                                  ('denied', 'DENIED'),
                                  ('error', 'ERROR'),
                                ],
                                selected: _outcome,
                                onSelected: (v) {
                                  setState(() => _outcome = v);
                                  _load();
                                },
                              ),
                            ],
                          ),
                        ),
                        const SizedBox(height: Tokens.gap),
                        if (_error != null)
                          Padding(
                            padding: const EdgeInsets.only(bottom: Tokens.gap),
                            child: NoteStrip(
                              auditFailure(_error!),
                              tone: denied ? Tone.warn : Tone.alarm,
                            ),
                          ),
                        if (!denied)
                          Panel(
                            legend: 'ENTRIES  NEWEST FIRST',
                            trailing: Text(
                              _next.isEmpty
                                  ? '${_entries.length}'
                                  : '${_entries.length}+',
                              style: t.legend(),
                            ),
                            padding: const EdgeInsets.all(4),
                            child: Column(
                              crossAxisAlignment: CrossAxisAlignment.stretch,
                              children: [
                                SizedBox(
                                  height: 3,
                                  child: _loading ? const BusyBar() : null,
                                ),
                                if (table) const _HeadRow(),
                                if (_entries.isEmpty && !_loading)
                                  Padding(
                                    padding: const EdgeInsets.all(12),
                                    child: Text(
                                      'No entries match.',
                                      style: t.small(),
                                    ),
                                  ),
                                for (var i = 0; i < _entries.length; i++)
                                  AuditRow(
                                    key: ValueKey('audit-$i'),
                                    index: i,
                                    entry: _entries[i],
                                    now: _now(),
                                    table: table,
                                  ),
                                if (_next.isNotEmpty)
                                  Padding(
                                    padding: const EdgeInsets.all(8),
                                    child: Align(
                                      alignment: Alignment.center,
                                      child: OutlinedButton(
                                        key: const ValueKey('audit-older'),
                                        onPressed: _loading
                                            ? null
                                            : () => _load(older: true),
                                        child: const Text('LOAD OLDER'),
                                      ),
                                    ),
                                  ),
                              ],
                            ),
                          ),
                      ],
                    ),
                  ),
                ),
              ],
            ),
          );
        },
      ),
    );
  }
}

const _flex = [3, 3, 4, 2, 4];

class _HeadRow extends StatelessWidget {
  const _HeadRow();

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    const heads = ['TIME', 'ACTOR', 'PROCEDURE', 'SUBJECT', 'OUTCOME'];
    return Padding(
      padding: const EdgeInsets.fromLTRB(8, 6, 8, 6),
      child: Row(
        children: [
          for (var i = 0; i < heads.length; i++)
            Expanded(
              flex: _flex[i],
              child: Text(heads[i], style: t.legend()),
            ),
        ],
      ),
    );
  }
}

class AuditRow extends StatelessWidget {
  const AuditRow({
    super.key,
    required this.index,
    required this.entry,
    required this.now,
    required this.table,
  });

  final int index;
  final AuditEntry entry;
  final DateTime now;
  final bool table;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final e = entry;
    final time = e.hasTime() ? auditTime(e.time.toDateTime(), now) : '-';
    final tone = outcomeTone(e.outcome);
    final outcome = Annunciator(
      label: e.outcome.toUpperCase(),
      tone: tone,
      lit: e.outcome != 'ok',
    );
    final code = e.outcome == 'ok' ? '' : e.code;
    Widget cell(String s, int i, {TextStyle? style}) => Expanded(
      flex: _flex[i],
      child: Padding(
        padding: const EdgeInsets.only(right: 8),
        child: Text(
          s.isEmpty ? '-' : s,
          style: style ?? t.data(),
          maxLines: 1,
          overflow: TextOverflow.ellipsis,
        ),
      ),
    );
    if (table) {
      return BarRow(
        index: index,
        child: Row(
          children: [
            cell(time, 0, style: t.small()),
            cell(e.actor, 1, style: t.data(t.p.emph)),
            cell(shortProcedure(e.procedure), 2),
            cell(
              e.task.isEmpty ? e.subject : '${e.subject} [task: ${e.task}]',
              3,
              style: t.data(t.p.emph),
            ),
            Expanded(
              flex: _flex[4],
              child: Row(
                children: [
                  outcome,
                  if (code.isNotEmpty) ...[
                    const SizedBox(width: 6),
                    Flexible(
                      child: Text(
                        code,
                        style: t.small(t.tone(tone)),
                        overflow: TextOverflow.ellipsis,
                      ),
                    ),
                  ],
                ],
              ),
            ),
          ],
        ),
      );
    }
    return BarRow(
      index: index,
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 8),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Expanded(
                child: Text(
                  shortProcedure(e.procedure),
                  style: t.data(),
                  overflow: TextOverflow.ellipsis,
                ),
              ),
              const SizedBox(width: 8),
              outcome,
            ],
          ),
          const SizedBox(height: 4),
          Text.rich(
            TextSpan(
              children: [
                TextSpan(text: e.actor, style: t.small(t.p.emph)),
                if (e.subject.isNotEmpty) ...[
                  TextSpan(text: '  >  ', style: t.small(t.p.legend)),
                  TextSpan(text: e.subject, style: t.small(t.p.emph)),
                ],
              ],
            ),
            overflow: TextOverflow.ellipsis,
          ),
          Text.rich(
            TextSpan(
              children: [
                TextSpan(text: time, style: t.small()),
                if (code.isNotEmpty)
                  TextSpan(text: '  $code', style: t.small(t.tone(tone))),
              ],
            ),
          ),
          if (e.task.isNotEmpty || e.detail.isNotEmpty)
            Text(
              [
                if (e.task.isNotEmpty) 'task: ${e.task}',
                if (e.detail.isNotEmpty) e.detail,
              ].join('  '),
              key: ValueKey('audit-note-$index'),
              style: t.small(),
              maxLines: 2,
              overflow: TextOverflow.ellipsis,
            ),
        ],
      ),
    );
  }
}
