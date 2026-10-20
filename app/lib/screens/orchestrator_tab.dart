import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../gen/protectorate/v1/chat.pb.dart';
import '../gen/protectorate/v1/message.pb.dart';
import '../gen/protectorate/v1/sleeve.pb.dart';
import '../hubs/event_feed.dart';
import '../hubs/hub_client.dart';
import '../mail/thread_view.dart';
import '../sleeves/sleeve_info.dart';
import '../theme/tokens.dart';
import '../vfd/lamp.dart';
import 'chat_screen.dart';
import 'messages_tab.dart';

const orchestratorSpawnCommand = 'protectorate orchestrator spawn';

// The always-open chat with the orchestrator. It talks to Poe unless the
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
  });

  final HubClient client;
  final EventFeed feed;
  final String selected;
  final void Function(String name) onSelect;
  final bool mail;
  final void Function(bool mail) onMail;

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
      if (isMailEvent(e)) {
        _mailDebounce?.cancel();
        _mailDebounce = Timer(const Duration(milliseconds: 300), _refreshMail);
      }
    });
    _poll = Timer.periodic(const Duration(seconds: 10), (_) => _refresh());
    _refresh();
    _refreshMail();
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

  // Poe first, then the other chat sleeves by name.
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
      if (s.name == widget.selected) current = s;
    }
    final names = [
      if (!chat.any((s) => s.name == orchestratorSleeve)) orchestratorSleeve,
      ...chat.map((s) => s.name),
    ];
    if (!names.contains(widget.selected)) names.add(widget.selected);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
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
                  key: ValueKey('${current.name}/${current.harness.value}'),
                  client: widget.client,
                  sleeve: current,
                )
              : _loading
              ? const SizedBox.shrink()
              : _NotRunning(
                  name: widget.selected,
                  error: _error,
                  onRetry: _refresh,
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

class _NotRunning extends StatelessWidget {
  const _NotRunning({
    required this.name,
    required this.error,
    required this.onRetry,
  });

  final String name;
  final Object? error;
  final VoidCallback onRetry;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final poe = name == orchestratorSleeve;
    return Center(
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
                    : poe
                    ? 'THE ORCHESTRATOR IS NOT RUNNING'
                    : 'SLEEVE ${name.toUpperCase()} IS NOT RUNNING',
                key: const ValueKey('orchestrator-empty'),
                style: t.legend(t.p.warn),
              ),
              const SizedBox(height: Tokens.gap),
              if (error != null)
                Text('$error', style: t.body())
              else if (poe) ...[
                Text(
                  'Poe is an ordinary chat sleeve named poe. Start it with the '
                  'owner token on the hub machine:',
                  style: t.body(),
                ),
                const SizedBox(height: Tokens.gap),
                Row(
                  children: [
                    Expanded(
                      child: SelectableText(
                        orchestratorSpawnCommand,
                        key: const ValueKey('spawn-command'),
                        style: t.data(t.p.emph),
                      ),
                    ),
                    IconButton(
                      key: const ValueKey('copy-spawn-command'),
                      tooltip: 'Copy command',
                      icon: const Icon(Icons.copy, size: 18),
                      onPressed: () => Clipboard.setData(
                        const ClipboardData(text: orchestratorSpawnCommand),
                      ),
                    ),
                  ],
                ),
              ] else
                Text('Pick another sleeve above.', style: t.body()),
              const SizedBox(height: Tokens.gap),
              OutlinedButton(
                key: const ValueKey('orchestrator-retry'),
                onPressed: onRetry,
                child: const Text('REFRESH'),
              ),
            ],
          ),
        ),
      ),
    );
  }
}
