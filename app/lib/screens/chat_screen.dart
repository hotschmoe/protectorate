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

enum _Menu { compact }

class _ChatScreenState extends State<ChatScreen> {
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
    _chat.start();
  }

  @override
  void dispose() {
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
    return ListenableBuilder(
      listenable: _chat,
      builder: (context, _) {
        final tr = _chat.transcript;
        final model = widget.sleeve.hasChat() ? widget.sleeve.chat.model : '';
        final items = tr.items.reversed.toList();
        return Scaffold(
          appBar: AppBar(
            title: Text(widget.sleeve.name),
            actions: [
              PopupMenuButton<_Menu>(
                key: const ValueKey('chat-menu'),
                onSelected: (m) => switch (m) {
                  _Menu.compact => _run(_chat.compact),
                },
                itemBuilder: (_) => [
                  PopupMenuItem(
                    key: const ValueKey('menu-compact'),
                    value: _Menu.compact,
                    enabled: _chat.canCompact,
                    child: const Text('Compact'),
                  ),
                ],
              ),
            ],
          ),
          body: SafeArea(
            child: Column(
              children: [
                _Header(
                  sleeve: widget.sleeve,
                  model: model,
                  state: _chat.state,
                  tokens: tr.contextTokens,
                  window: tr.contextWindow,
                ),
                if (_chat.feedState == FeedState.retrying)
                  const LinearProgressIndicator(
                    key: ValueKey('reconnecting'),
                    minHeight: 2,
                  ),
                Expanded(
                  child: Center(
                    child: ConstrainedBox(
                      constraints: const BoxConstraints(
                        maxWidth: Tokens.contentMaxWidth,
                      ),
                      child: items.isEmpty
                          ? const Center(child: Text('No messages yet.'))
                          : ListView.separated(
                              key: const ValueKey('transcript'),
                              reverse: true,
                              padding: const EdgeInsets.all(Tokens.gap),
                              itemCount: items.length,
                              separatorBuilder: (_, _) =>
                                  const SizedBox(height: Tokens.gap),
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
            ),
          ),
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
  });

  final Sleeve sleeve;
  final String model;
  final ChatState state;
  final int tokens;
  final int window;

  @override
  Widget build(BuildContext context) {
    final cs = Theme.of(context).colorScheme;
    return Container(
      width: double.infinity,
      padding: const EdgeInsets.symmetric(horizontal: Tokens.gap, vertical: 8),
      decoration: BoxDecoration(
        border: Border(bottom: BorderSide(color: cs.outlineVariant)),
      ),
      child: Wrap(
        spacing: 8,
        runSpacing: 6,
        crossAxisAlignment: WrapCrossAlignment.center,
        children: [
          HarnessBadge(
            harness: sleeve.harness,
            provider: sleeveProvider(sleeve),
            local: isLocalModel(sleeve),
          ),
          if (model.isNotEmpty)
            Text(model, style: Theme.of(context).textTheme.labelMedium),
          if (isLocalModel(sleeve)) const LocalModelBadge(),
          StatusPill(key: const ValueKey('status-pill'), state: state),
          ContextMeter(tokens: tokens, window: window),
        ],
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
    final cs = Theme.of(context).colorScheme;
    return Container(
      decoration: BoxDecoration(
        color: cs.surface,
        border: Border(top: BorderSide(color: cs.outlineVariant)),
      ),
      padding: const EdgeInsets.fromLTRB(Tokens.gap, 8, Tokens.gap, 8),
      child: Center(
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: Tokens.contentMaxWidth),
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.end,
            children: [
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
                    textCapitalization: TextCapitalization.sentences,
                    keyboardType: TextInputType.multiline,
                    decoration: InputDecoration(
                      hintText: working ? 'Steer the agent' : 'Message',
                      filled: true,
                      fillColor: cs.surfaceContainerHighest,
                      contentPadding: const EdgeInsets.symmetric(
                        horizontal: 16,
                        vertical: 10,
                      ),
                      border: OutlineInputBorder(
                        borderRadius: BorderRadius.circular(22),
                        borderSide: BorderSide.none,
                      ),
                    ),
                  ),
                ),
              ),
              const SizedBox(width: 6),
              if (working)
                IconButton.filledTonal(
                  key: const ValueKey('stop'),
                  tooltip: 'Stop',
                  onPressed: onStop,
                  icon: const Icon(Icons.stop_rounded),
                ),
              IconButton.filled(
                key: const ValueKey('send'),
                tooltip: 'Send',
                onPressed: onSend,
                icon: const Icon(Icons.arrow_upward_rounded),
              ),
            ],
          ),
        ),
      ),
    );
  }
}
