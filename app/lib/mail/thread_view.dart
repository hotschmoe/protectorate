import 'dart:async';
import 'dart:math' as math;

import 'package:connectrpc/connect.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../gen/protectorate/v1/events.pb.dart';
import '../gen/protectorate/v1/message.pb.dart';
import '../hubs/event_feed.dart';
import '../hubs/hub_client.dart';
import '../nodes/node_view.dart';
import '../theme/tokens.dart';
import '../vfd/lamp.dart';
import '../vfd/panel.dart';

const ownerAddress = 'owner';
const hubSender = 'hub';
// The orchestrator's sleeve name; it is addressed as "orchestrator".
const orchestratorSleeve = 'poe';

bool isMailEvent(Event e) => e.kind.startsWith('msg.');

String eventThread(Event e) => e.payload.fields['thread']?.stringValue ?? '';

final _random = math.Random.secure();

String newRequestId() =>
    'app-${List.generate(16, (_) => _random.nextInt(16).toRadixString(16)).join()}';

String firstLine(String s) => s.trim().split('\n').first.trim();

// "github.com/acme/lib" reads as "acme/lib"; a path project as its last
// directory.
String shortProject(String id) {
  if (id.startsWith('path:')) return id.split('/').last;
  final parts = id.split('/');
  return parts.length >= 3 ? parts.sublist(parts.length - 2).join('/') : id;
}

String partyLabel(String p) =>
    p.startsWith('project:') ? 'project:${shortProject(p.substring(8))}' : p;

// The address a participant is written to with Send.
String mailAddress(String participant) =>
    participant == orchestratorSleeve ? 'orchestrator' : participant;

String kindLabel(MessageKind k) => switch (k) {
  MessageKind.MESSAGE_KIND_REQUEST => 'REQUEST',
  MessageKind.MESSAGE_KIND_NOTICE => 'NOTICE',
  _ => 'NOTE',
};

bool canReply(MailMessage m) => m.from != hubSender && m.from != ownerAddress;

MailDelivery? ownerDelivery(MailMessage m) =>
    m.deliveries.where((d) => d.recipient == ownerAddress).firstOrNull;

bool ownerCanAck(MailMessage m) {
  final d = ownerDelivery(m);
  return d != null && !d.hasAckedAt();
}

bool unreadByOwner(MailMessage m) {
  final d = ownerDelivery(m);
  return d != null && !d.hasDeliveredAt() && !d.hasReadAt() && !d.hasAckedAt();
}

String mailFailure(Object e) {
  if (e is ConnectException && e.code == Code.permissionDenied) {
    return 'Sending, replying and acking need the owner token. '
        'This device can read threads only.';
  }
  return '$e';
}

(String, Tone) deliveryState(MailMessage m, MailDelivery d) {
  if (d.hasAckedAt()) return ('ACKED', Tone.ok);
  final seen = d.hasReadAt()
      ? 'READ'
      : d.hasDeliveredAt()
      ? 'DELIVERED'
      : 'QUEUED';
  if (m.ackRequired) return ('$seen, ACK DUE', Tone.warn);
  return (seen, seen == 'QUEUED' ? Tone.dim : Tone.lit);
}

// The messages a reply most likely answers: the newest one still waiting on
// the owner, then the newest anyone may reply to.
List<MailMessage> replyCandidates(List<MailMessage> msgs) {
  final out = <MailMessage>[];
  final open = msgs.lastWhere(
    (m) => canReply(m) && ownerCanAck(m),
    orElse: MailMessage.new,
  );
  if (open.id.isNotEmpty) out.add(open);
  final last = msgs.lastWhere(canReply, orElse: MailMessage.new);
  if (last.id.isNotEmpty && last.id != open.id) out.add(last);
  return out;
}

// Who may be written to from a thread: everyone but the owner and the hub.
List<String> threadTargets(Iterable<MailMessage> msgs) {
  final out = <String>[];
  void add(String p) {
    if (p == ownerAddress || p == hubSender || p.isEmpty) return;
    final a = mailAddress(p);
    if (!out.contains(a)) out.add(a);
  }

  for (final m in msgs) {
    add(m.from);
    if (m.deliveries.isEmpty) add(m.to);
    for (final d in m.deliveries) {
      add(d.recipient);
    }
  }
  return out;
}

// One thread: its messages with every recipient's state, and a composer that
// writes as whoever the token names.
class ThreadView extends StatefulWidget {
  const ThreadView({
    super.key,
    required this.client,
    required this.feed,
    required this.threadId,
    this.onChanged,
    this.now,
  });

  final HubClient client;
  final EventFeed feed;
  final String threadId;
  final VoidCallback? onChanged;
  final DateTime Function()? now;

  @override
  State<ThreadView> createState() => _ThreadViewState();
}

class _ThreadViewState extends State<ThreadView> {
  List<MailMessage> _msgs = const [];
  Object? _error;
  String? _actionError;
  bool _loading = true;
  bool _sending = false;
  String? _target;
  StreamSubscription<Event>? _sub;
  final _input = TextEditingController();
  final _scroll = ScrollController();

  DateTime _now() => (widget.now ?? DateTime.now)();

  @override
  void initState() {
    super.initState();
    _sub = widget.feed.incoming.listen((e) {
      if (isMailEvent(e) && eventThread(e) == widget.threadId) _load();
    });
    _load(markRead: true);
  }

  @override
  void dispose() {
    _sub?.cancel();
    _input.dispose();
    _scroll.dispose();
    super.dispose();
  }

  Future<void> _load({bool markRead = false}) async {
    try {
      final r = await widget.client.messages.thread(
        ThreadRequest(threadId: widget.threadId),
      );
      if (!mounted) return;
      setState(() {
        _msgs = r.messages;
        _error = null;
        _target ??= _defaultTarget(r.messages);
      });
      if (markRead && r.messages.any(unreadByOwner)) await _markRead();
    } on Object catch (e) {
      if (mounted) setState(() => _error = e);
    } finally {
      if (mounted) setState(() => _loading = false);
    }
  }

  // Reading the thread here is reading it: the owner's messages in it are
  // marked read. A token that may not do that leaves them unread.
  Future<void> _markRead() async {
    try {
      await widget.client.messages.inbox(
        InboxRequest(threadId: widget.threadId),
      );
      widget.onChanged?.call();
    } on Object {
      return;
    }
  }

  String? _defaultTarget(List<MailMessage> msgs) {
    final replies = replyCandidates(msgs);
    if (replies.isNotEmpty) return 'reply:${replies.first.id}';
    final targets = threadTargets(msgs);
    return targets.isEmpty ? null : 'to:${targets.first}';
  }

  Future<void> _act(Future<void> Function() f) async {
    setState(() {
      _sending = true;
      _actionError = null;
    });
    try {
      await f();
      await _load();
      widget.onChanged?.call();
    } on Object catch (e) {
      if (mounted) setState(() => _actionError = mailFailure(e));
    } finally {
      if (mounted) setState(() => _sending = false);
    }
  }

  Future<void> _ack(MailMessage m) =>
      _act(() => widget.client.messages.ack(AckRequest(id: m.id)));

  Future<void> _send() async {
    final body = _input.text.trim();
    final target = _target;
    if (body.isEmpty || target == null || _sending) return;
    await _act(() async {
      if (target.startsWith('reply:')) {
        await widget.client.messages.reply(
          ReplyRequest(
            id: target.substring(6),
            body: body,
            requestId: newRequestId(),
          ),
        );
      } else {
        await widget.client.messages.send(
          MessageServiceSendRequest(
            to: target.substring(3),
            body: body,
            threadId: widget.threadId,
            requestId: newRequestId(),
          ),
        );
      }
      _input.clear();
    });
  }

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    if (_loading && _msgs.isEmpty) return const BusyBar();
    if (_msgs.isEmpty && _error != null) {
      return Padding(
        padding: const EdgeInsets.all(Tokens.gap),
        child: NoteStrip('$_error', tone: Tone.alarm),
      );
    }
    final targets = threadTargets(_msgs);
    final target = _target;
    final picked = target != null && target.startsWith('reply:')
        ? _msgs.where((m) => m.id == target.substring(6)).firstOrNull
        : null;
    final options = <(String, String)>[
      for (final m in {...replyCandidates(_msgs), ?picked})
        ('reply:${m.id}', 'REPLY ${m.id} ${m.from}'),
      for (final a in targets) ('to:$a', 'TO $a'),
    ];
    final open = _msgs.where((m) => m.ackRequired && !_acked(m)).length;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Expanded(
          child: ListView(
            controller: _scroll,
            padding: const EdgeInsets.all(Tokens.gap),
            children: [
              Center(
                child: ConstrainedBox(
                  constraints: const BoxConstraints(
                    maxWidth: Tokens.contentMaxWidth,
                  ),
                  child: Panel(
                    legend:
                        'THREAD ${widget.threadId}  ${_msgs.length} MESSAGES',
                    trailing: open > 0
                        ? LampLegend(label: '$open ACK DUE', tone: Tone.warn)
                        : const LampLegend(label: 'SETTLED', tone: Tone.ok),
                    padding: const EdgeInsets.all(4),
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.stretch,
                      children: [
                        if (_error != null)
                          Padding(
                            padding: const EdgeInsets.all(8),
                            child: NoteStrip(
                              'Refresh failed: $_error',
                              tone: Tone.alarm,
                            ),
                          ),
                        for (var i = 0; i < _msgs.length; i++) ...[
                          if (i > 0) Divider(color: t.p.rule, height: 1),
                          MailBlock(
                            key: ValueKey('mail-${_msgs[i].id}'),
                            message: _msgs[i],
                            now: _now(),
                            onAck: ownerCanAck(_msgs[i]) && !_sending
                                ? () => _ack(_msgs[i])
                                : null,
                            onReply: canReply(_msgs[i]) && !_sending
                                ? () => setState(
                                    () => _target = 'reply:${_msgs[i].id}',
                                  )
                                : null,
                          ),
                        ],
                      ],
                    ),
                  ),
                ),
              ),
            ],
          ),
        ),
        if (_actionError != null)
          Padding(
            padding: const EdgeInsets.fromLTRB(
              Tokens.gap,
              0,
              Tokens.gap,
              Tokens.gapS,
            ),
            child: NoteStrip(_actionError!, tone: Tone.alarm),
          ),
        if (options.isNotEmpty)
          MailComposer(
            controller: _input,
            options: options,
            selected: _target ?? options.first.$1,
            onSelect: (v) => setState(() => _target = v),
            sending: _sending,
            onSend: _send,
          ),
      ],
    );
  }
}

bool _acked(MailMessage m) => m.deliveries.any((d) => d.hasAckedAt());

// One message: sender and address, the body, then each recipient's state.
class MailBlock extends StatelessWidget {
  const MailBlock({
    super.key,
    required this.message,
    required this.now,
    this.onAck,
    this.onReply,
  });

  final MailMessage message;
  final DateTime now;
  final VoidCallback? onAck;
  final VoidCallback? onReply;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final m = message;
    final edge = switch (m.from) {
      ownerAddress => t.p.info,
      hubSender => t.p.dim,
      _ => t.p.lit,
    };
    final age = m.hasCreatedAt()
        ? formatAge(now.difference(m.createdAt.toDateTime()))
        : '';
    return Container(
      padding: const EdgeInsets.fromLTRB(12, 10, 8, 8),
      decoration: BoxDecoration(
        border: Border(left: BorderSide(color: edge, width: 2)),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Expanded(
                child: Text.rich(
                  TextSpan(
                    children: [
                      TextSpan(text: m.from, style: t.data(t.p.emph)),
                      TextSpan(text: '  >  ', style: t.legend()),
                      TextSpan(text: partyLabel(m.to), style: t.data()),
                    ],
                  ),
                  overflow: TextOverflow.ellipsis,
                ),
              ),
              const SizedBox(width: 8),
              Text('${m.id}  $age', style: t.small()),
            ],
          ),
          if (m.kind != MessageKind.MESSAGE_KIND_NOTE &&
                  m.kind != MessageKind.MESSAGE_KIND_UNSPECIFIED ||
              m.hasEscalatedAt() ||
              m.noteId.isNotEmpty)
            Padding(
              padding: const EdgeInsets.only(top: 6),
              child: Wrap(
                spacing: 6,
                runSpacing: 6,
                children: [
                  if (m.kind == MessageKind.MESSAGE_KIND_REQUEST ||
                      m.kind == MessageKind.MESSAGE_KIND_NOTICE)
                    Annunciator(
                      label: kindLabel(m.kind),
                      tone: m.kind == MessageKind.MESSAGE_KIND_REQUEST
                          ? Tone.lit
                          : Tone.dim,
                    ),
                  if (m.hasEscalatedAt())
                    Annunciator(
                      key: ValueKey('escalated-${m.id}'),
                      label: 'ESCALATED',
                      tone: _acked(m) ? Tone.dim : Tone.alarm,
                    ),
                  if (m.noteId.isNotEmpty)
                    Annunciator(label: 'NOTE ${m.noteId}', tone: Tone.info),
                ],
              ),
            ),
          Padding(
            padding: const EdgeInsets.symmetric(vertical: 8),
            child: SelectableText(m.body.trimRight(), style: t.body(t.p.emph)),
          ),
          Row(
            crossAxisAlignment: CrossAxisAlignment.center,
            children: [
              Expanded(
                child: Wrap(
                  spacing: 14,
                  runSpacing: 6,
                  children: [
                    if (m.deliveries.isEmpty)
                      const LampLegend(
                        label: 'WAITING FOR A RECIPIENT',
                        tone: Tone.warn,
                      ),
                    for (final d in m.deliveries)
                      Builder(
                        builder: (context) {
                          final (label, tone) = deliveryState(m, d);
                          final reminded = d.reminders > 0
                              ? ', ${d.reminders} REMINDED'
                              : '';
                          return LampLegend(
                            key: ValueKey('delivery-${m.id}-${d.recipient}'),
                            label: '${d.recipient} $label$reminded',
                            tone: tone,
                            on: tone != Tone.dim,
                          );
                        },
                      ),
                  ],
                ),
              ),
              if (onReply != null)
                TextButton(
                  key: ValueKey('reply-${m.id}'),
                  onPressed: onReply,
                  child: const Text('REPLY'),
                ),
              if (onAck != null) ...[
                const SizedBox(width: 6),
                OutlinedButton(
                  key: ValueKey('ack-${m.id}'),
                  style: m.ackRequired
                      ? OutlinedButton.styleFrom(
                          foregroundColor: t.p.warn,
                          side: BorderSide(color: t.p.warn),
                        )
                      : null,
                  onPressed: onAck,
                  child: const Text('ACK'),
                ),
              ],
            ],
          ),
        ],
      ),
    );
  }
}

// The bottom strip of a thread: who the text goes to, the text, SEND.
class MailComposer extends StatelessWidget {
  const MailComposer({
    super.key,
    required this.controller,
    required this.options,
    required this.selected,
    required this.onSelect,
    required this.onSend,
    this.sending = false,
  });

  final TextEditingController controller;
  final List<(String, String)> options;
  final String selected;
  final ValueChanged<String> onSelect;
  final VoidCallback onSend;
  final bool sending;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    const none = OutlineInputBorder(borderSide: BorderSide.none);
    return Container(
      decoration: BoxDecoration(
        color: t.p.frame,
        border: Border(top: BorderSide(color: t.p.bezel)),
      ),
      padding: const EdgeInsets.fromLTRB(Tokens.gap, 6, Tokens.gap, 10),
      child: Center(
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: Tokens.contentMaxWidth),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            mainAxisSize: MainAxisSize.min,
            children: [
              ChoiceKeys<String>(
                options: options,
                selected: selected,
                onSelected: sending ? null : onSelect,
              ),
              const SizedBox(height: 4),
              Container(
                decoration: BoxDecoration(
                  color: t.p.glass,
                  border: Border.all(color: t.p.bezelHi),
                  borderRadius: BorderRadius.circular(Tokens.bezelRadius - 3),
                ),
                padding: const EdgeInsets.fromLTRB(12, 4, 4, 4),
                child: Row(
                  crossAxisAlignment: CrossAxisAlignment.end,
                  children: [
                    Expanded(
                      child: CallbackShortcuts(
                        bindings: {
                          const SingleActivator(
                            LogicalKeyboardKey.enter,
                            control: true,
                          ): onSend,
                        },
                        child: TextField(
                          key: const ValueKey('mail-compose'),
                          controller: controller,
                          minLines: 1,
                          maxLines: 6,
                          style: t.body(t.p.emph),
                          cursorWidth: 8,
                          textCapitalization: TextCapitalization.sentences,
                          keyboardType: TextInputType.multiline,
                          decoration: InputDecoration(
                            hintText: 'message',
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
                    FilledButton(
                      key: const ValueKey('mail-send'),
                      style: FilledButton.styleFrom(
                        padding: const EdgeInsets.symmetric(horizontal: 14),
                      ),
                      onPressed: sending ? null : onSend,
                      child: const Text('SEND'),
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
