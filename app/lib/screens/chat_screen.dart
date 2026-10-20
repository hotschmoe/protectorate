import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../chat/badges.dart';
import '../chat/chat_controller.dart';
import '../chat/message_widgets.dart';
import '../gen/protectorate/v1/chat.pb.dart';
import '../gen/protectorate/v1/sleeve.pb.dart';
import '../hubs/event_feed.dart';
import '../hubs/hub_client.dart';
import '../hubs/hub_profile.dart';
import '../sleeves/sleeve_info.dart';
import '../theme/tokens.dart';
import '../vfd/lamp.dart';
import 'sleeves_screen.dart';

class ChatScreen extends StatefulWidget {
  const ChatScreen({
    super.key,
    required this.hub,
    required this.client,
    required this.sleeve,
  });

  final HubProfile hub;
  final HubClient client;
  final Sleeve sleeve;

  @override
  State<ChatScreen> createState() => _ChatScreenState();
}

class _ChatScreenState extends State<ChatScreen> {
  (ChatState, int, int)? _live;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final wide = MediaQuery.sizeOf(context).width >= chatPaneBreakpoint;
    final chat = ChatView(
      client: widget.client,
      sleeve: widget.sleeve,
      onState: wide
          ? (st, tokens, window) => setState(() => _live = (st, tokens, window))
          : null,
    );
    return Scaffold(
      appBar: AppBar(title: Text(widget.sleeve.name)),
      body: SafeArea(
        child: wide
            ? Row(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  Expanded(child: chat),
                  VerticalDivider(width: 1, color: t.p.bezel),
                  SizedBox(
                    width: 300,
                    child: SleeveStatusPane(sleeve: widget.sleeve, live: _live),
                  ),
                ],
              )
            : chat,
      ),
    );
  }
}

const chatPaneBreakpoint = 1100.0;

enum _Menu { compact }

// The transcript, its status strip and the composer for one sleeve.
class ChatView extends StatefulWidget {
  const ChatView({
    super.key,
    required this.client,
    required this.sleeve,
    this.onState,
    this.actions = const [],
  });

  final HubClient client;
  final Sleeve sleeve;
  // Extra entries for the header menu, after COMPACT CONTEXT.
  final List<ChatAction> actions;
  final void Function(ChatState state, int tokens, int window)? onState;

  @override
  State<ChatView> createState() => _ChatViewState();
}

class ChatAction {
  const ChatAction({required this.id, required this.label, required this.run});

  final String id;
  final String label;
  final VoidCallback run;
}

class _ChatViewState extends State<ChatView> {
  late final ChatController _chat;
  final _input = TextEditingController();

  @override
  void initState() {
    super.initState();
    _chat = ChatController(
      client: widget.client.chat,
      sleeve: widget.sleeve.name,
    );
    if (widget.sleeve.hasChat()) _chat.transcript.seed(widget.sleeve.chat);
    _chat.addListener(_report);
    _chat.start();
  }

  // Deferred, because the controller can notify while the tree builds.
  void _report() {
    if (widget.onState == null) return;
    Future.microtask(() {
      if (!mounted) return;
      final tr = _chat.transcript;
      widget.onState?.call(_chat.state, tr.contextTokens, tr.contextWindow);
    });
  }

  @override
  void dispose() {
    _chat.removeListener(_report);
    _chat.dispose();
    _input.dispose();
    super.dispose();
  }

  void _fail(Object e) {
    if (!mounted) return;
    ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text('$e')));
  }

  Future<void> _run(Future<void> Function() f) async {
    try {
      await f();
    } on Object catch (e) {
      _fail(e);
    }
  }

  Future<void> _send() async {
    final text = _input.text.trim();
    if (text.isEmpty) return;
    _input.clear();
    try {
      await _chat.send(text);
    } on Object catch (e) {
      if (mounted && _input.text.isEmpty) _input.text = text;
      _fail(e);
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    return ListenableBuilder(
      listenable: _chat,
      builder: (context, _) {
        final tr = _chat.transcript;
        final model = widget.sleeve.hasChat() ? widget.sleeve.chat.model : '';
        final items = tr.items.reversed.toList();
        return Column(
          children: [
            _Header(
              sleeve: widget.sleeve,
              model: model,
              state: _chat.state,
              tokens: tr.contextTokens,
              window: tr.contextWindow,
              cacheExpires: tr.cacheExpires,
              menu: PopupMenuButton<Object>(
                key: const ValueKey('chat-menu'),
                tooltip: 'Chat actions',
                icon: Icon(Icons.more_vert, color: t.p.dim),
                onSelected: (m) {
                  if (m == _Menu.compact) {
                    _run(_chat.compact);
                  } else if (m is ChatAction) {
                    m.run();
                  }
                },
                itemBuilder: (_) => [
                  PopupMenuItem<Object>(
                    key: const ValueKey('menu-compact'),
                    value: _Menu.compact,
                    enabled: _chat.canCompact,
                    child: const Text('COMPACT CONTEXT'),
                  ),
                  for (final a in widget.actions)
                    PopupMenuItem<Object>(
                      key: ValueKey('menu-${a.id}'),
                      value: a,
                      child: Text(a.label),
                    ),
                ],
              ),
            ),
            if (_chat.feedState == FeedState.retrying)
              const BusyBar(key: ValueKey('reconnecting'), tone: Tone.warn),
            Expanded(
              child: Center(
                child: ConstrainedBox(
                  constraints: const BoxConstraints(
                    maxWidth: Tokens.contentMaxWidth,
                  ),
                  child: items.isEmpty
                      ? Center(
                          child: Text('NO MESSAGES YET', style: t.legend()),
                        )
                      : ListView.separated(
                          key: const ValueKey('transcript'),
                          reverse: true,
                          padding: const EdgeInsets.fromLTRB(
                            Tokens.gapL,
                            Tokens.gapL,
                            Tokens.gapL,
                            Tokens.gapL,
                          ),
                          itemCount: items.length,
                          separatorBuilder: (_, _) =>
                              const SizedBox(height: Tokens.gapL),
                          itemBuilder: (_, i) => buildChatItem(
                            items[i],
                            onRespond: (id, allow) =>
                                _run(() => _chat.respond(id, allow)),
                          ),
                        ),
                ),
              ),
            ),
            _Composer(
              controller: _input,
              working: _chat.working,
              onSend: _send,
              onStop: () => _run(_chat.interrupt),
            ),
          ],
        );
      },
    );
  }
}

class _Header extends StatelessWidget {
  const _Header({
    required this.sleeve,
    required this.model,
    required this.state,
    required this.tokens,
    required this.window,
    required this.cacheExpires,
    required this.menu,
  });

  final Sleeve sleeve;
  final String model;
  final ChatState state;
  final int tokens;
  final int window;
  final DateTime? cacheExpires;
  final Widget menu;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    return Container(
      width: double.infinity,
      padding: const EdgeInsets.symmetric(vertical: 10),
      decoration: BoxDecoration(
        color: t.p.frame,
        border: Border(bottom: BorderSide(color: t.p.bezel)),
      ),
      alignment: Alignment.center,
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: Tokens.contentMaxWidth),
        child: Padding(
          padding: const EdgeInsets.fromLTRB(Tokens.gapL, 0, 4, 0),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Row(
                children: [
                  Expanded(
                    child: Wrap(
                      spacing: 10,
                      runSpacing: 6,
                      crossAxisAlignment: WrapCrossAlignment.center,
                      children: [
                        HarnessBadge(
                          harness: sleeve.harness,
                          provider: sleeveProvider(sleeve),
                          local: isLocalModel(sleeve),
                        ),
                        if (model.isNotEmpty)
                          Text(model, style: t.data(t.p.emph)),
                        if (isLocalModel(sleeve)) const LocalModelBadge(),
                        if (summarizedLocally(sleeve))
                          const LocalSummaryBadge(),
                      ],
                    ),
                  ),
                  StateLamp(
                    key: const ValueKey('status-pill'),
                    state: state,
                    big: true,
                  ),
                  menu,
                ],
              ),
              Padding(
                padding: const EdgeInsets.only(top: 8, right: 12),
                child: Row(
                  children: [
                    Expanded(
                      child: ContextMeter(
                        tokens: tokens,
                        window: window,
                        width: null,
                      ),
                    ),
                    if (sleeve.harness == Harness.HARNESS_CLAUDE) ...[
                      const SizedBox(width: Tokens.gapL),
                      CacheLamp(expires: cacheExpires),
                    ],
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

class _Composer extends StatelessWidget {
  const _Composer({
    required this.controller,
    required this.working,
    required this.onSend,
    required this.onStop,
  });

  final TextEditingController controller;
  final bool working;
  final VoidCallback onSend;
  final VoidCallback onStop;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final none = const OutlineInputBorder(borderSide: BorderSide.none);
    return Container(
      decoration: BoxDecoration(
        color: t.p.frame,
        border: Border(top: BorderSide(color: t.p.bezel)),
      ),
      padding: const EdgeInsets.fromLTRB(Tokens.gap, 10, Tokens.gap, 10),
      child: Center(
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: Tokens.contentMaxWidth),
          child: Container(
            decoration: BoxDecoration(
              color: t.p.glass,
              border: Border.all(color: t.p.bezelHi),
              borderRadius: BorderRadius.circular(Tokens.bezelRadius - 3),
            ),
            padding: const EdgeInsets.fromLTRB(12, 4, 4, 4),
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.end,
              children: [
                Padding(
                  padding: const EdgeInsets.only(bottom: 11),
                  child: Text(
                    '>',
                    style: t.title(working ? t.p.info : t.p.lit),
                  ),
                ),
                const SizedBox(width: 10),
                Expanded(
                  child: CallbackShortcuts(
                    bindings: {
                      const SingleActivator(LogicalKeyboardKey.enter): onSend,
                    },
                    child: TextField(
                      key: const ValueKey('composer'),
                      controller: controller,
                      minLines: 1,
                      maxLines: 6,
                      style: t.body(t.p.emph),
                      cursorWidth: 8,
                      cursorColor: t.p.lit.withValues(alpha: 0.85),
                      textCapitalization: TextCapitalization.sentences,
                      keyboardType: TextInputType.multiline,
                      decoration: InputDecoration(
                        hintText: working ? 'steer the agent' : 'message',
                        hintStyle: t.body(t.p.dim.withValues(alpha: 0.7)),
                        border: none,
                        enabledBorder: none,
                        focusedBorder: none,
                        contentPadding: const EdgeInsets.symmetric(
                          vertical: 12,
                        ),
                      ),
                    ),
                  ),
                ),
                const SizedBox(width: 6),
                if (working) ...[
                  Tooltip(
                    message: 'Stop',
                    child: OutlinedButton(
                      key: const ValueKey('stop'),
                      style: OutlinedButton.styleFrom(
                        foregroundColor: t.p.alarm,
                        side: BorderSide(
                          color: t.p.alarm.withValues(alpha: 0.7),
                        ),
                        padding: const EdgeInsets.symmetric(horizontal: 14),
                      ),
                      onPressed: onStop,
                      child: const Text('STOP'),
                    ),
                  ),
                  const SizedBox(width: 6),
                ],
                Tooltip(
                  message: 'Send',
                  child: FilledButton(
                    key: const ValueKey('send'),
                    style: FilledButton.styleFrom(
                      padding: const EdgeInsets.symmetric(horizontal: 14),
                    ),
                    onPressed: onSend,
                    child: const Text('SEND'),
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
