import 'dart:async';

import 'package:flutter/material.dart';
import 'package:fixnum/fixnum.dart';

import '../gen/protectorate/v1/sleeve.pb.dart';
import '../hubs/event_feed.dart';
import '../hubs/hub_client.dart';
import '../hubs/hub_profile.dart';
import '../nodes/node_view.dart';
import '../gen/protectorate/v1/chat.pb.dart';
import '../chat/badges.dart';
import '../sleeves/sleeve_card.dart';
import '../sleeves/sleeve_info.dart';
import '../theme/tokens.dart';
import '../vfd/lamp.dart';
import '../vfd/meter.dart';
import '../vfd/panel.dart';
import 'chat_screen.dart';
import 'terminal_screen.dart';

final _sleeveName = RegExp(r'^[a-z0-9][a-z0-9-]{0,39}$');

class SleevesScreen extends StatefulWidget {
  const SleevesScreen({
    super.key,
    required this.hub,
    required this.client,
    required this.feed,
  });

  final HubProfile hub;
  final HubClient client;
  final EventFeed feed;

  @override
  State<SleevesScreen> createState() => _SleevesScreenState();
}

class _SleevesScreenState extends State<SleevesScreen> {
  List<Sleeve> _sleeves = const [];
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
      if (mounted) {
        setState(() {
          _sleeves = r.sleeves;
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
    final req = await showDialog<SpawnSleeveRequest>(
      context: context,
      builder: (_) => const SpawnSleeveDialog(),
    );
    if (req == null) return;
    try {
      await widget.client.sleeves.spawnSleeve(req);
      await _refresh();
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
    final spawn = wide
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
      appBar: AppBar(
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
      floatingActionButton: wide
          ? null
          : FloatingActionButton.extended(
              key: const ValueKey('spawn-sleeve'),
              heroTag: null,
              onPressed: _spawn,
              icon: const Icon(Icons.add),
              label: const Text('SPAWN'),
            ),
      body: _loading
          ? const BusyBar()
          : Column(
              children: [
                if (_error != null)
                  Padding(
                    padding: const EdgeInsets.fromLTRB(12, 12, 12, 0),
                    child: NoteStrip('$_error', tone: Tone.alarm),
                  ),
                Expanded(
                  child: _sleeves.isEmpty
                      ? Center(child: Text('NO SLEEVES', style: t.legend()))
                      : wide
                      ? _multiPane(t)
                      : _list(),
                ),
              ],
            ),
    );
  }

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
          children: [
            for (final s in _sleeves)
              SizedBox(
                width: w,
                child: SleeveCard(
                  sleeve: s,
                  now: DateTime.now(),
                  onOpen: () => _open(s),
                  onKill: () => _kill(s),
                ),
              ),
          ],
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
              for (final s in _sleeves)
                Padding(
                  padding: const EdgeInsets.only(bottom: Tokens.gap),
                  child: SleeveCard(
                    sleeve: s,
                    now: DateTime.now(),
                    compact: true,
                    selected: s.name == cur?.name,
                    onOpen: () => _select(s),
                    onKill: () => _kill(s),
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

class SpawnSleeveDialog extends StatefulWidget {
  const SpawnSleeveDialog({super.key});

  @override
  State<SpawnSleeveDialog> createState() => _SpawnSleeveDialogState();
}

class _SpawnSleeveDialogState extends State<SpawnSleeveDialog> {
  final _form = GlobalKey<FormState>();
  final _name = TextEditingController();
  final _workspace = TextEditingController();
  final _cpu = TextEditingController();
  final _mem = TextEditingController();
  Harness _harness = Harness.HARNESS_CLAUDE;

  @override
  void dispose() {
    _name.dispose();
    _workspace.dispose();
    _cpu.dispose();
    _mem.dispose();
    super.dispose();
  }

  String? _optionalInt(String? v) {
    final t = (v ?? '').trim();
    if (t.isEmpty) return null;
    final n = int.tryParse(t);
    return n == null || n < 0 ? 'whole number' : null;
  }

  void _submit() {
    if (!_form.currentState!.validate()) return;
    final cpu = int.tryParse(_cpu.text.trim()) ?? 0;
    final memMb = int.tryParse(_mem.text.trim()) ?? 0;
    Navigator.of(context).pop(
      SpawnSleeveRequest(
        name: _name.text.trim(),
        workspace: _workspace.text.trim(),
        harness: _harness,
        limits: SleeveLimits(cpuPercent: cpu, memoryBytes: Int64(memMb) << 20),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      title: const Text('Spawn sleeve'),
      content: SingleChildScrollView(
        child: Form(
          key: _form,
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              TextFormField(
                key: const ValueKey('sleeve-name'),
                controller: _name,
                decoration: const InputDecoration(labelText: 'Name'),
                validator: (v) => _sleeveName.hasMatch((v ?? '').trim())
                    ? null
                    : 'a-z, 0-9 and -, up to 40',
              ),
              TextFormField(
                key: const ValueKey('sleeve-workspace'),
                controller: _workspace,
                decoration: const InputDecoration(
                  labelText: 'Workspace path or git URL',
                ),
                validator: (v) => (v ?? '').trim().isEmpty ? 'required' : null,
              ),
              DropdownButtonFormField<Harness>(
                initialValue: _harness,
                decoration: const InputDecoration(labelText: 'Harness'),
                items: [
                  for (final h in Harness.values.where(
                    (h) => h != Harness.HARNESS_UNSPECIFIED,
                  ))
                    DropdownMenuItem(value: h, child: Text(harnessLabel(h))),
                ],
                onChanged: (h) => setState(() => _harness = h ?? _harness),
              ),
              TextFormField(
                key: const ValueKey('sleeve-cpu'),
                controller: _cpu,
                keyboardType: TextInputType.number,
                decoration: const InputDecoration(
                  labelText: 'CPU limit %',
                  helperText: '100 = one core, empty = unlimited',
                ),
                validator: _optionalInt,
              ),
              TextFormField(
                key: const ValueKey('sleeve-mem'),
                controller: _mem,
                keyboardType: TextInputType.number,
                decoration: const InputDecoration(
                  labelText: 'Memory limit MB',
                  helperText: 'empty = unlimited',
                ),
                validator: _optionalInt,
              ),
            ],
          ),
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(),
          child: const Text('CANCEL'),
        ),
        FilledButton(
          key: const ValueKey('sleeve-spawn'),
          onPressed: _submit,
          child: const Text('SPAWN'),
        ),
      ],
    );
  }
}
