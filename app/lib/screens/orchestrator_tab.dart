import 'dart:async';

import 'package:flutter/material.dart';

import '../gen/protectorate/v1/chat.pb.dart';
import '../gen/protectorate/v1/message.pb.dart';
import '../gen/protectorate/v1/sleeve.pb.dart';
import '../hubs/event_feed.dart';
import '../hubs/hub_client.dart';
import '../mail/thread_view.dart';
import '../sleeves/sleeve_info.dart';
import '../sleeves/model_selection.dart';
import '../theme/tokens.dart';
import '../vfd/lamp.dart';
import 'chat_screen.dart';
import 'messages_tab.dart';

// The harnesses Envoy can run on; UNSPECIFIED lets the hub pick by quota.
const _envoyHarnesses = [
  Harness.HARNESS_UNSPECIFIED,
  Harness.HARNESS_CLAUDE,
  Harness.HARNESS_CODEX,
  Harness.HARNESS_GROK,
  Harness.HARNESS_PI,
];

String _harnessLabel(Harness h) => h == Harness.HARNESS_UNSPECIFIED
    ? 'auto'
    : h.name.replaceFirst('HARNESS_', '').toLowerCase();

class _HarnessPicker extends StatelessWidget {
  const _HarnessPicker({
    super.key,
    required this.value,
    required this.onChanged,
  });

  final Harness value;
  final void Function(Harness) onChanged;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    return DropdownButton<Harness>(
      value: value,
      underline: const SizedBox.shrink(),
      onChanged: (v) {
        if (v != null) onChanged(v);
      },
      items: [
        for (final h in _envoyHarnesses)
          DropdownMenuItem(
            key: ValueKey('harness-option-${_harnessLabel(h)}'),
            value: h,
            child: Text(
              _harnessLabel(h).toUpperCase(),
              style: t.data(t.p.emph),
            ),
          ),
      ],
    );
  }
}

// The always-open chat with the orchestrator. It talks to Envoy unless the
// owner picks another chat sleeve; terminal sleeves have no chat to show.
class OrchestratorTab extends StatefulWidget {
  const OrchestratorTab({
    super.key,
    required this.client,
    required this.feed,
    required this.selected,
    required this.onSelect,
    required this.mail,
    required this.onMail,
    this.envoyOnly = false,
  });

  final HubClient client;
  final EventFeed feed;
  final String selected;
  final void Function(String name) onSelect;
  final bool mail;
  final void Function(bool mail) onMail;
  final bool envoyOnly;

  @override
  State<OrchestratorTab> createState() => _OrchestratorTabState();
}

class _OrchestratorTabState extends State<OrchestratorTab> {
  List<Sleeve> _sleeves = const [];
  Object? _error;
  bool _loading = true;
  StreamSubscription<dynamic>? _sub;
  Timer? _poll;
  Timer? _mailDebounce;
  int _unread = 0;

  @override
  void initState() {
    super.initState();
    _sub = widget.feed.incoming.listen((e) {
      if (e.kind.startsWith('sleeve.')) _refresh();
      if (!widget.envoyOnly && isMailEvent(e)) {
        _mailDebounce?.cancel();
        _mailDebounce = Timer(const Duration(milliseconds: 300), _refreshMail);
      }
    });
    _poll = Timer.periodic(const Duration(seconds: 10), (_) => _refresh());
    _refresh();
    if (!widget.envoyOnly) _refreshMail();
  }

  @override
  void dispose() {
    _sub?.cancel();
    _poll?.cancel();
    _mailDebounce?.cancel();
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

  Future<void> _refreshMail() async {
    try {
      final r = await widget.client.messages.listThreads(ListThreadsRequest());
      final n = r.threads.fold<int>(0, (n, th) => n + th.unread);
      if (mounted) setState(() => _unread = n);
    } on Object {
      return;
    }
  }

  Future<void> _startEnvoy(Harness harness, ModelSelection selection) async {
    await widget.client.sleeves.spawnEnvoy(
      SpawnEnvoyRequest(harness: harness, selection: selection),
    );
    await _refresh();
  }

  void _toast(Object e) {
    if (!mounted) return;
    ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text('$e')));
  }

  Future<void> _respawn(Sleeve current) async {
    final ok = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('RESPAWN ENVOY'),
        content: const Text(
          'This stops Envoy and starts a fresh session on the same harness. '
          'Its conversation is not carried over and the prompt cache has to '
          'be rebuilt, which costs tokens on the next message.',
        ),
        actions: [
          TextButton(
            key: const ValueKey('respawn-cancel'),
            onPressed: () => Navigator.pop(context, false),
            child: const Text('CANCEL'),
          ),
          FilledButton(
            key: const ValueKey('respawn-confirm'),
            onPressed: () => Navigator.pop(context, true),
            child: const Text('RESPAWN'),
          ),
        ],
      ),
    );
    if (ok != true) return;
    try {
      await widget.client.sleeves.spawnEnvoy(
        SpawnEnvoyRequest(
          harness: current.harness,
          selection: current.chat.settings.requested,
        ),
      );
      await _refresh();
    } on Object catch (e) {
      _toast(e);
    }
  }

  Future<void> _resleeve(Sleeve current) async {
    var to = Harness.HARNESS_UNSPECIFIED;
    var selection = ModelSelection();
    var nativeDefaults = false;
    final ok = await showDialog<bool>(
      context: context,
      builder: (context) => StatefulBuilder(
        builder: (context, setState) => AlertDialog(
          title: const Text('RESLEEVE ENVOY'),
          content: SingleChildScrollView(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                const Text(
                  'Moves Envoy to another harness with a handoff summary. The '
                  'new harness starts a new session, so its prompt cache is '
                  'cold on the first message.',
                ),
                _HarnessPicker(
                  key: const ValueKey('resleeve-harness'),
                  value: to,
                  onChanged: (h) => setState(() {
                    to = h;
                    selection = ModelSelection();
                    nativeDefaults = false;
                  }),
                ),
                ModelSelectionFields(
                  client: widget.client,
                  harness: to,
                  selection: selection,
                  onChanged: (value) => setState(() {
                    selection = value;
                    nativeDefaults = false;
                  }),
                ),
                CheckboxListTile(
                  value: nativeDefaults,
                  title: const Text(
                    'Use target native defaults (discard current selection)',
                  ),
                  onChanged: (v) => setState(() {
                    nativeDefaults = v ?? false;
                    if (nativeDefaults) selection = ModelSelection();
                  }),
                ),
              ],
            ),
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(context, false),
              child: const Text('CANCEL'),
            ),
            FilledButton(
              key: const ValueKey('resleeve-confirm'),
              onPressed: () => Navigator.pop(context, true),
              child: const Text('RESLEEVE'),
            ),
          ],
        ),
      ),
    );
    if (ok != true) return;
    try {
      await widget.client.sleeves.resleeveSleeve(
        ResleeveSleeveRequest(
          name: current.name,
          to: to,
          nativeDefaults: nativeDefaults,
          selection: selection.model.isEmpty && selection.effort.isEmpty
              ? null
              : selection,
        ),
      );
      await _refresh();
    } on Object catch (e) {
      _toast(e);
    }
  }

  // Envoy first, then the other chat sleeves by name.
  List<Sleeve> get _chatSleeves {
    final out = _sleeves
        .where((s) => effectiveMode(s) == SleeveMode.SLEEVE_MODE_CHAT)
        .toList();
    out.sort((a, b) {
      if (a.name == orchestratorSleeve) return -1;
      if (b.name == orchestratorSleeve) return 1;
      return a.name.compareTo(b.name);
    });
    return out;
  }

  @override
  Widget build(BuildContext context) {
    if (widget.envoyOnly) return _chat(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        _ModeSwitch(
          mail: widget.mail,
          unread: _unread,
          onChanged: (m) {
            widget.onMail(m);
            if (!m) _refreshMail();
          },
        ),
        Expanded(
          child: widget.mail
              ? MessagesTab(client: widget.client, feed: widget.feed)
              : _chat(context),
        ),
      ],
    );
  }

  Widget _chat(BuildContext context) {
    final chat = _chatSleeves;
    Sleeve? current;
    for (final s in chat) {
      if (s.name == widget.selected &&
          (!widget.envoyOnly || s.state == SleeveState.SLEEVE_STATE_RUNNING)) {
        current = s;
      }
    }
    final names = [
      if (!chat.any((s) => s.name == orchestratorSleeve)) orchestratorSleeve,
      ...chat.map((s) => s.name),
    ];
    if (!names.contains(widget.selected)) names.add(widget.selected);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (!widget.envoyOnly)
          _Selector(
            names: names,
            selected: widget.selected,
            missing: {
              for (final n in names)
                if (!chat.any((s) => s.name == n)) n,
            },
            onSelect: widget.onSelect,
          ),
        Expanded(
          child: current != null
              ? ChatView(
                  key: ValueKey(
                    '${current.name}/${current.harness.value}/'
                    '${current.createdAt.seconds}:${current.createdAt.nanos}',
                  ),
                  client: widget.client,
                  sleeve: current,
                  phone: widget.envoyOnly,
                  actions: current.name == orchestratorSleeve
                      ? [
                          ChatAction(
                            id: 'respawn',
                            label: 'RESPAWN',
                            run: () => _respawn(current!),
                          ),
                          ChatAction(
                            id: 'resleeve',
                            label: 'RESLEEVE',
                            run: () => _resleeve(current!),
                          ),
                        ]
                      : const [],
                )
              : _loading
              ? const SizedBox.shrink()
              : _NotRunning(
                  client: widget.client,
                  name: widget.selected,
                  error: _error,
                  onRetry: _refresh,
                  onStart: _startEnvoy,
                  phone: widget.envoyOnly,
                ),
        ),
      ],
    );
  }
}

class _ModeSwitch extends StatelessWidget {
  const _ModeSwitch({
    required this.mail,
    required this.unread,
    required this.onChanged,
  });

  final bool mail;
  final int unread;
  final void Function(bool mail) onChanged;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    return Container(
      padding: const EdgeInsets.symmetric(
        horizontal: Tokens.gapL,
        vertical: Tokens.gap,
      ),
      decoration: BoxDecoration(
        color: t.p.frame,
        border: Border(bottom: BorderSide(color: t.p.bezel)),
      ),
      child: Center(
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: Tokens.contentMaxWidth),
          child: Row(
            children: [
              SegmentedButton<bool>(
                key: const ValueKey('mode-switch'),
                showSelectedIcon: false,
                segments: [
                  const ButtonSegment(
                    value: false,
                    label: Text('CHAT', key: ValueKey('mode-chat')),
                  ),
                  ButtonSegment(
                    value: true,
                    label: Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        const Text('MAIL', key: ValueKey('mode-mail')),
                        if (unread > 0) ...[
                          const SizedBox(width: 8),
                          LampLegend(
                            key: const ValueKey('mail-unread'),
                            label: '$unread',
                            tone: Tone.info,
                          ),
                        ],
                      ],
                    ),
                  ),
                ],
                selected: {mail},
                onSelectionChanged: (v) => onChanged(v.first),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class _Selector extends StatelessWidget {
  const _Selector({
    required this.names,
    required this.selected,
    required this.missing,
    required this.onSelect,
  });

  final List<String> names;
  final String selected;
  final Set<String> missing;
  final void Function(String) onSelect;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    return Container(
      padding: const EdgeInsets.symmetric(
        horizontal: Tokens.gapL,
        vertical: Tokens.gap,
      ),
      decoration: BoxDecoration(
        color: t.p.frame,
        border: Border(bottom: BorderSide(color: t.p.bezel)),
      ),
      child: Center(
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: Tokens.contentMaxWidth),
          child: Row(
            children: [
              Text('SLEEVE', style: t.legend()),
              const SizedBox(width: Tokens.gapL),
              Expanded(
                child: DropdownButton<String>(
                  key: const ValueKey('sleeve-selector'),
                  isExpanded: true,
                  value: selected,
                  underline: const SizedBox.shrink(),
                  onChanged: (v) {
                    if (v != null) onSelect(v);
                  },
                  items: [
                    for (final n in names)
                      DropdownMenuItem(
                        key: ValueKey('sleeve-option-$n'),
                        value: n,
                        child: Text(
                          n == orchestratorSleeve
                              ? '$n (orchestrator)${missing.contains(n) ? ' - not running' : ''}'
                              : n,
                          overflow: TextOverflow.ellipsis,
                          style: t.data(t.p.emph),
                        ),
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
}

class _NotRunning extends StatefulWidget {
  const _NotRunning({
    required this.name,
    required this.error,
    required this.onRetry,
    required this.onStart,
    required this.client,
    this.phone = false,
  });

  final String name;
  final Object? error;
  final VoidCallback onRetry;
  final Future<void> Function(Harness, ModelSelection) onStart;
  final HubClient client;
  final bool phone;

  @override
  State<_NotRunning> createState() => _NotRunningState();
}

class _NotRunningState extends State<_NotRunning> {
  Harness _harness = Harness.HARNESS_UNSPECIFIED;
  ModelSelection _selection = ModelSelection();
  bool _starting = false;
  Object? _startError;

  Future<void> _start() async {
    setState(() {
      _starting = true;
      _startError = null;
    });
    try {
      await widget.onStart(_harness, _selection);
    } on Object catch (e) {
      if (mounted) setState(() => _startError = e);
    } finally {
      if (mounted) setState(() => _starting = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final name = widget.name;
    final error = widget.error;
    final envoy = name == orchestratorSleeve;
    return Align(
      alignment: widget.phone ? Alignment.bottomCenter : Alignment.center,
      child: SingleChildScrollView(
        padding: const EdgeInsets.all(Tokens.gapL),
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 520),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                error != null
                    ? 'CANNOT LIST SLEEVES'
                    : envoy
                    ? 'ENVOY IS NOT RUNNING'
                    : 'SLEEVE ${name.toUpperCase()} IS NOT RUNNING',
                key: const ValueKey('orchestrator-empty'),
                style: t.legend(t.p.warn),
              ),
              const SizedBox(height: Tokens.gap),
              if (error != null)
                Text('$error', style: t.body())
              else if (envoy) ...[
                Text(
                  widget.phone
                      ? 'Your point of contact for health, sleeves and new work. '
                            'Start Envoy to begin a conversation.'
                      : 'Envoy is the orchestrator: an ordinary chat sleeve that '
                            'drives the hub through the CLI. Start it on a harness, or '
                            'let the hub pick the one with the most spare quota.',
                  style: t.body(),
                ),
                const SizedBox(height: Tokens.gap),
                if (widget.phone)
                  FilledButton.icon(
                    key: const ValueKey('start-envoy'),
                    style: FilledButton.styleFrom(
                      minimumSize: const Size.fromHeight(48),
                    ),
                    onPressed: _starting ? null : _start,
                    icon: const Icon(Icons.support_agent),
                    label: Text(
                      _starting ? 'STARTING ENVOY...' : 'START ENVOY',
                    ),
                  )
                else
                  Wrap(
                    spacing: Tokens.gapL,
                    runSpacing: Tokens.gap,
                    crossAxisAlignment: WrapCrossAlignment.center,
                    children: [
                      Text('HARNESS', style: t.legend()),
                      _HarnessPicker(
                        key: const ValueKey('start-harness'),
                        value: _harness,
                        onChanged: (h) => setState(() {
                          _harness = h;
                          _selection = ModelSelection();
                        }),
                      ),
                      FilledButton(
                        key: const ValueKey('start-envoy'),
                        onPressed: _starting ? null : _start,
                        child: const Text('START ENVOY'),
                      ),
                    ],
                  ),
                if (!widget.phone)
                  ModelSelectionFields(
                    client: widget.client,
                    harness: _harness,
                    selection: _selection,
                    onChanged: (value) => setState(() => _selection = value),
                  ),
                if (_startError != null) ...[
                  const SizedBox(height: Tokens.gap),
                  Text(
                    '$_startError',
                    key: const ValueKey('start-error'),
                    style: t.body(t.p.warn),
                  ),
                ],
              ] else
                Text('Pick another sleeve above.', style: t.body()),
              const SizedBox(height: Tokens.gap),
              if (!widget.phone || error != null)
                OutlinedButton(
                  key: const ValueKey('orchestrator-retry'),
                  onPressed: widget.onRetry,
                  child: const Text('REFRESH'),
                ),
            ],
          ),
        ),
      ),
    );
  }
}
