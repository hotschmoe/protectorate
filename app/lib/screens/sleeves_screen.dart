import 'dart:async';

import 'package:flutter/material.dart';

import '../gen/protectorate/v1/sleeve.pb.dart';
import '../hubs/event_feed.dart';
import '../hubs/hub_client.dart';
import '../hubs/hub_profile.dart';
import '../nodes/node_view.dart';
import '../gen/protectorate/v1/chat.pb.dart';
import '../chat/badges.dart';
import '../gen/protectorate/v1/node.pb.dart';
import '../sleeves/node_fence.dart';
import '../sleeves/sleeve_card.dart';
import '../sleeves/sleeve_info.dart';
import '../sleeves/spawn_dialog.dart';
import '../theme/tokens.dart';
import '../vfd/lamp.dart';
import '../vfd/meter.dart';
import '../vfd/panel.dart';
import 'chat_screen.dart';
import 'sessions_tab.dart';
import 'terminal_screen.dart';

class SleevesScreen extends StatefulWidget {
  const SleevesScreen({
    super.key,
    required this.hub,
    required this.client,
    required this.feed,
    this.embedded = false,
    this.openSession,
  });

  final HubProfile hub;
  final HubClient client;
  final EventFeed feed;

  // The hub shell's SLEEVES tab: no app bar, a node header and the entry to
  // raw sessions. Otherwise the page reached from the hub node's detail.
  final bool embedded;

  // A raw tmux session to open on arrival (deep links).
  final String? openSession;

  @override
  State<SleevesScreen> createState() => _SleevesScreenState();
}

class _SleevesScreenState extends State<SleevesScreen> {
  List<Sleeve> _sleeves = const [];
  List<SleeveGroup> _groups = const [];
  Object? _error;
  bool _loading = true;
  StreamSubscription<dynamic>? _sub;
  Timer? _poll;
  String? _selected;
  (ChatState, int, int)? _live;

  @override
  void initState() {
    super.initState();
    _sub = widget.feed.incoming.listen((e) {
      if (e.kind.startsWith('sleeve.')) _refresh();
    });
    _poll = Timer.periodic(const Duration(seconds: 10), (_) => _refresh());
    _refresh();
    final target = widget.openSession;
    if (target != null) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted) _rawSessions(target);
      });
    }
  }

  void _rawSessions([String? open]) {
    Navigator.of(context).push(
      MaterialPageRoute<void>(
        builder: (_) => RawSessionsScreen(
          hub: widget.hub,
          client: widget.client,
          openSession: open,
        ),
      ),
    );
  }

  @override
  void dispose() {
    _sub?.cancel();
    _poll?.cancel();
    super.dispose();
  }

  Future<void> _refresh() async {
    try {
      final r = await widget.client.sleeves.listSleeves(ListSleevesRequest());
      var nodes = const <NodeStatus>[];
      if (widget.embedded) {
        try {
          nodes = (await widget.client.node.listNodes(
            ListNodesRequest(),
          )).nodes;
        } on Object {
          // The hub's own fence still shows.
        }
      }
      final groups = groupSleeves(nodes, r.sleeves, widget.hub.name);
      if (mounted) {
        setState(() {
          _groups = groups;
          _sleeves = [for (final g in groups) ...g.sleeves];
          _error = null;
        });
      }
    } on Object catch (e) {
      if (mounted) setState(() => _error = e);
    } finally {
      if (mounted) setState(() => _loading = false);
    }
  }

  void _fail(Object e) {
    if (!mounted) return;
    ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text('$e')));
  }

  Future<void> _spawn() async {
    try {
      final s = await startSleeve(context, widget.client);
      if (s == null) return;
      await _refresh();
      if (mounted) _open(s);
    } on Object catch (e) {
      _fail(e);
    }
  }

  Future<void> _kill(Sleeve s) async {
    final ok = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: Text('Kill sleeve ${s.name}?'),
        content: const Text(
          'This stops the harness and every process in its sandbox. '
          'The workspace and the sleeve home directory are kept on disk.',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(ctx).pop(false),
            child: const Text('CANCEL'),
          ),
          FilledButton(
            key: const ValueKey('confirm-kill'),
            style: FilledButton.styleFrom(
              backgroundColor: Tokens.of(ctx).p.alarm,
            ),
            onPressed: () => Navigator.of(ctx).pop(true),
            child: const Text('KILL'),
          ),
        ],
      ),
    );
    if (ok != true) return;
    try {
      await widget.client.sleeves.killSleeve(KillSleeveRequest(name: s.name));
      await _refresh();
    } on Object catch (e) {
      _fail(e);
    }
  }

  void _open(Sleeve s) {
    Navigator.of(context).push(
      MaterialPageRoute<void>(
        builder: (_) => effectiveMode(s) == SleeveMode.SLEEVE_MODE_CHAT
            ? ChatScreen(hub: widget.hub, client: widget.client, sleeve: s)
            : TerminalScreen(
                hub: widget.hub,
                client: widget.client,
                session: s.session,
              ),
      ),
    );
  }

  bool _chatPane(Sleeve s) =>
      effectiveMode(s) == SleeveMode.SLEEVE_MODE_CHAT &&
      s.state == SleeveState.SLEEVE_STATE_RUNNING;

  Sleeve? get _current {
    final pick = _sleeves.where((s) => s.name == _selected).firstOrNull;
    return pick ?? _sleeves.where(_chatPane).firstOrNull;
  }

  void _select(Sleeve s) {
    if (!_chatPane(s)) return _open(s);
    setState(() {
      _selected = s.name;
      _live = null;
    });
  }

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final wide = MediaQuery.sizeOf(context).width >= multiPaneBreakpoint;
    final spawn = wide && !widget.embedded
        ? Padding(
            padding: const EdgeInsets.only(right: Tokens.gap),
            child: FilledButton(
              key: const ValueKey('spawn-sleeve'),
              onPressed: _spawn,
              child: const Text('SPAWN'),
            ),
          )
        : null;
    return Scaffold(
      appBar: widget.embedded
          ? null
          : AppBar(
              title: Text('Sleeves'),
              actions: [
                if (!_loading)
                  Padding(
                    padding: const EdgeInsets.only(right: Tokens.gap),
                    child: Center(
                      child: Text(
                        '${_sleeves.length} ON THIS HUB',
                        style: t.legend(),
                      ),
                    ),
                  ),
                ?spawn,
              ],
            ),
      floatingActionButton: wide && !widget.embedded
          ? null
          : FloatingActionButton.extended(
              key: const ValueKey('spawn-sleeve'),
              heroTag: null,
              onPressed: _spawn,
              icon: const Icon(Icons.add),
              label: Text(widget.embedded ? 'NEW SLEEVE' : 'SPAWN'),
            ),
      body: _loading
          ? const BusyBar()
          : Column(
              children: [
                if (widget.embedded) _nodeHeader(),
                if (_error != null)
                  Padding(
                    padding: const EdgeInsets.fromLTRB(12, 12, 12, 0),
                    child: NoteStrip('$_error', tone: Tone.alarm),
                  ),
                Expanded(
                  child: _sleeves.isEmpty && !widget.embedded
                      ? Center(child: Text('NO SLEEVES', style: t.legend()))
                      : wide
                      ? _multiPane(t)
                      : _list(),
                ),
              ],
            ),
    );
  }

  Widget _nodeHeader() => Padding(
    padding: const EdgeInsets.fromLTRB(Tokens.gap, 4, 4, 0),
    child: Align(
      alignment: Alignment.centerRight,
      child: TextButton(
        key: const ValueKey('raw-sessions'),
        onPressed: _rawSessions,
        child: const Text('RAW SESSIONS'),
      ),
    ),
  );

  SleeveCard _card(Sleeve s, {bool compact = false, bool selected = false}) =>
      SleeveCard(
        sleeve: s,
        now: DateTime.now(),
        compact: compact,
        selected: selected,
        onOpen: () => compact ? _select(s) : _open(s),
        onKill: () => _kill(s),
      );

  Widget _fence(SleeveGroup g, List<Widget> cards) =>
      NodeFence(group: g, onNew: g.local ? _spawn : null, children: cards);

  Widget _list() => LayoutBuilder(
    builder: (context, box) {
      const pad = Tokens.gap;
      final cols = (box.maxWidth / 440).floor().clamp(1, 3);
      final w = (box.maxWidth - 2 * pad - (cols - 1) * pad) / cols;
      return SingleChildScrollView(
        padding: const EdgeInsets.fromLTRB(pad, pad, pad, 96),
        child: Wrap(
          spacing: pad,
          runSpacing: pad,
          children: widget.embedded
              ? [
                  for (final g in _groups)
                    SizedBox(
                      width: w,
                      child: _fence(g, [for (final s in g.sleeves) _card(s)]),
                    ),
                ]
              : [for (final s in _sleeves) SizedBox(width: w, child: _card(s))],
        ),
      );
    },
  );

  Widget _multiPane(Tokens t) {
    final cur = _current;
    return Row(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        SizedBox(
          width: 340,
          child: ListView(
            padding: const EdgeInsets.all(Tokens.gap),
            children: [
              if (widget.embedded)
                for (final g in _groups)
                  Padding(
                    padding: const EdgeInsets.only(bottom: Tokens.gap),
                    child: _fence(g, [
                      for (final s in g.sleeves)
                        _card(s, compact: true, selected: s.name == cur?.name),
                    ]),
                  )
              else
                for (final s in _sleeves)
                  Padding(
                    padding: const EdgeInsets.only(bottom: Tokens.gap),
                    child: _card(
                      s,
                      compact: true,
                      selected: s.name == cur?.name,
                    ),
                  ),
            ],
          ),
        ),
        VerticalDivider(width: 1, color: t.p.bezel),
        Expanded(
          child: cur == null || !_chatPane(cur)
              ? Center(child: Text('NO CHAT SLEEVE RUNNING', style: t.legend()))
              : ChatView(
                  key: ValueKey('chat-${cur.name}'),
                  client: widget.client,
                  sleeve: cur,
                  onState: (st, tokens, window) =>
                      setState(() => _live = (st, tokens, window)),
                ),
        ),
        VerticalDivider(width: 1, color: t.p.bezel),
        SizedBox(
          width: 300,
          child: cur == null
              ? const SizedBox.shrink()
              : SleeveStatusPane(
                  sleeve: cur,
                  live: _live,
                  onKill: () => _kill(cur),
                  onPopOut: () => _open(cur),
                ),
        ),
      ],
    );
  }
}

const multiPaneBreakpoint = 1200.0;

// The selected sleeve's instruments beside its chat.
class SleeveStatusPane extends StatelessWidget {
  const SleeveStatusPane({
    super.key,
    required this.sleeve,
    this.onKill,
    this.onPopOut,
    this.live,
  });

  final Sleeve sleeve;
  final (ChatState, int, int)? live;
  final VoidCallback? onKill;
  final VoidCallback? onPopOut;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final c = sleeve.hasChat() ? sleeve.chat : ChatStatus();
    final tokens = live?.$2 ?? c.contextTokens.toInt();
    final window = live?.$3 ?? c.contextWindow.toInt();
    final u = sleeve.usage;
    final cpuLimit = sleeve.limits.cpuPercent;
    final memLimit = sleeve.limits.memoryBytes.toInt();
    final pct = window > 0 ? tokens * 100 / window : null;
    return ListView(
      padding: const EdgeInsets.all(Tokens.gap),
      children: [
        Panel(
          legend: 'CONTEXT',
          child: Column(
            children: [
              ArcGauge(
                percent: pct,
                value: pct == null ? '--' : pct.round().toString(),
                unit: '%',
                caption: window > 0
                    ? '${formatTokens(tokens)} / ${formatTokens(window)}'
                    : 'WINDOW UNKNOWN',
                size: 180,
              ),
              const SizedBox(height: 6),
              StateLamp(state: live?.$1 ?? sleeveState(sleeve), big: true),
            ],
          ),
        ),
        const SizedBox(height: Tokens.gap),
        Panel(
          legend: 'SANDBOX',
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              MeterRow(
                label: 'CPU',
                labelChars: 3,
                percent: cpuLimit > 0 ? u.cpuPercent * 100 / cpuLimit : null,
                value:
                    '${u.cpuPercent.toStringAsFixed(0)}%${cpuLimit > 0 ? ' / $cpuLimit%' : ''}',
                valueChars: 11,
              ),
              MeterRow(
                label: 'MEM',
                labelChars: 3,
                percent: memLimit > 0
                    ? u.memoryCurrentBytes.toInt() * 100 / memLimit
                    : null,
                value: formatBytes(u.memoryCurrentBytes.toInt()),
                valueChars: 8,
              ),
              const SizedBox(height: 8),
              Text('WORKSPACE', style: t.legend()),
              const SizedBox(height: 2),
              Text(sleeve.workspace, style: t.small(t.p.lit)),
              if (u.oomKills > 0)
                Text('${u.oomKills} OOM kills', style: t.small(t.p.alarm)),
            ],
          ),
        ),
        if (onKill != null && onPopOut != null) ...[
          const SizedBox(height: Tokens.gap),
          Row(
            children: [
              Expanded(
                child: OutlinedButton(
                  key: ValueKey('kill-${sleeve.name}'),
                  style: OutlinedButton.styleFrom(
                    foregroundColor: t.p.alarm,
                    side: BorderSide(color: t.p.alarm.withValues(alpha: 0.6)),
                  ),
                  onPressed: onKill,
                  child: const Text('KILL'),
                ),
              ),
              const SizedBox(width: 8),
              Expanded(
                child: OutlinedButton(
                  key: ValueKey('open-${sleeve.name}'),
                  onPressed: onPopOut,
                  child: const Text('FULL SCREEN'),
                ),
              ),
            ],
          ),
        ],
      ],
    );
  }
}
