import 'package:flutter/material.dart';

import '../chat/badges.dart';
import '../gen/protectorate/v1/chat.pb.dart';
import '../gen/protectorate/v1/sleeve.pb.dart';
import '../mail/thread_view.dart' show orchestratorSleeve;
import '../nodes/node_view.dart';
import '../theme/tokens.dart';
import '../vfd/meter.dart';
import '../vfd/panel.dart';
import 'sleeve_info.dart';

ChatState sleeveState(Sleeve s) {
  if (s.state == SleeveState.SLEEVE_STATE_EXITED) {
    return ChatState.CHAT_STATE_EXITED;
  }
  return s.hasChat() ? s.chat.state : ChatState.CHAT_STATE_UNSPECIFIED;
}

class SleeveStateLamp extends StatelessWidget {
  const SleeveStateLamp({super.key, required this.sleeve, this.big = false});

  final Sleeve sleeve;
  final bool big;

  @override
  Widget build(BuildContext context) {
    final st = sleeveState(sleeve);
    if (st != ChatState.CHAT_STATE_UNSPECIFIED) {
      return StateLamp(state: st, big: big);
    }
    if (sleeve.state == SleeveState.SLEEVE_STATE_RUNNING) {
      return StateLamp(
        state: ChatState.CHAT_STATE_IDLE,
        label: 'Running',
        big: big,
      );
    }
    return const SizedBox.shrink();
  }
}

class SleeveCard extends StatelessWidget {
  const SleeveCard({
    super.key,
    required this.sleeve,
    required this.now,
    required this.onOpen,
    required this.onKill,
    this.selected = false,
    this.compact = false,
  });

  final Sleeve sleeve;
  final DateTime now;
  final VoidCallback onOpen;
  final VoidCallback onKill;
  final bool selected;
  final bool compact;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final accent = t.model(
      sleeve.harness,
      sleeveProvider(sleeve),
      local: isLocalModel(sleeve),
    );
    final running = sleeve.state == SleeveState.SLEEVE_STATE_RUNNING;
    final chatMode = effectiveMode(sleeve) == SleeveMode.SLEEVE_MODE_CHAT;
    final model = sleeve.hasChat() ? sleeve.chat.model : '';
    final u = sleeve.usage;
    final memLimit = sleeve.limits.memoryBytes.toInt();
    final cpuLimit = sleeve.limits.cpuPercent;
    final workspaceName =
        sleeve.workspace.split('/').where((p) => p.isNotEmpty).lastOrNull ??
        sleeve.workspace;
    final uptime = sleeve.hasCreatedAt() && running
        ? formatUptime(
            now
                .difference(sleeve.createdAt.toDateTime())
                .inSeconds
                .clamp(0, 1 << 31),
          )
        : null;
    final attention =
        sleeveState(sleeve) == ChatState.CHAT_STATE_AWAITING_APPROVAL;
    return Panel(
      key: ValueKey('sleeve-${sleeve.name}'),
      tone: selected ? Tone.info : (attention ? Tone.warn : null),
      onTap: running ? onOpen : null,
      padding: EdgeInsets.zero,
      child: IntrinsicHeight(
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Container(
              key: const ValueKey('harness-accent'),
              width: 4,
              decoration: BoxDecoration(
                color: accent.fg,
                boxShadow: t.boxGlow(accent.fg, radius: 8),
              ),
            ),
            Expanded(
              child: Padding(
                padding: const EdgeInsets.fromLTRB(14, 12, 12, 10),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Row(
                      children: [
                        Expanded(
                          child: Text(
                            sleeve.name,
                            overflow: TextOverflow.ellipsis,
                            style: t.title(),
                          ),
                        ),
                        const SizedBox(width: 8),
                        SleeveStateLamp(sleeve: sleeve),
                      ],
                    ),
                    const SizedBox(height: 8),
                    Wrap(
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
                          Text(model, style: t.data(t.p.emph)),
                        if (isLocalModel(sleeve)) const LocalModelBadge(),
                        if (summarizedLocally(sleeve))
                          const LocalSummaryBadge(),
                        if (sleeve.name == orchestratorSleeve)
                          Text(
                            'ENVOY',
                            key: const ValueKey('envoy-badge'),
                            style: t.legend(t.p.emph),
                          ),
                        Text(chatMode ? 'CHAT' : 'TERMINAL', style: t.legend()),
                      ],
                    ),
                    const SizedBox(height: 8),
                    Text.rich(
                      TextSpan(
                        children: [
                          TextSpan(text: workspaceName, style: t.data(t.p.lit)),
                          if (uptime != null)
                            TextSpan(text: '  up $uptime', style: t.small()),
                        ],
                      ),
                      overflow: TextOverflow.ellipsis,
                    ),
                    if (sleeve.gitBranch.isNotEmpty)
                      Text(
                        'branch ${sleeve.gitBranch}',
                        overflow: TextOverflow.ellipsis,
                        style: t.small(t.p.lit),
                      ),
                    if (!compact)
                      Text(
                        sleeve.workspace,
                        overflow: TextOverflow.ellipsis,
                        style: t.small(),
                      ),
                    if (sleeve.hasChat() && sleeve.chat.contextWindow > 0) ...[
                      const SizedBox(height: 10),
                      ContextMeter(
                        tokens: sleeve.chat.contextTokens.toInt(),
                        window: sleeve.chat.contextWindow.toInt(),
                        width: null,
                      ),
                    ],
                    Text(
                      'home ${formatBytes(u.homeDiskBytes.toInt())}  cache ${formatBytes(u.cacheDiskBytes.toInt())}  rest ${formatBytes((u.homeDiskBytes - u.cacheDiskBytes).toInt())}  Go ${formatBytes(u.goBuildCacheDiskBytes.toInt())}',
                      style: t.small(),
                    ),
                    if (running) ...[
                      const SizedBox(height: 4),
                      MeterRow(
                        label: 'CPU',
                        labelChars: 3,
                        percent: cpuLimit > 0
                            ? u.cpuPercent * 100 / cpuLimit
                            : null,
                        value:
                            '${u.cpuPercent.toStringAsFixed(0)}%${cpuLimit > 0 ? ' / $cpuLimit%' : ''}',
                        valueChars: 17,
                      ),
                      MeterRow(
                        label: 'MEM',
                        labelChars: 3,
                        percent: memLimit > 0
                            ? u.memoryCurrentBytes.toInt() * 100 / memLimit
                            : null,
                        value:
                            '${formatBytes(u.memoryCurrentBytes.toInt())}${memLimit > 0 ? ' / ${formatBytes(memLimit)}' : ''}',
                        valueChars: 17,
                      ),
                      if (u.oomKills > 0)
                        Text(
                          '${u.oomKills} OOM kills',
                          style: t.small(t.p.alarm),
                        ),
                    ],
                    if (!compact) ...[
                      const SizedBox(height: 10),
                      Row(
                        mainAxisAlignment: MainAxisAlignment.end,
                        children: [
                          OutlinedButton(
                            key: ValueKey('kill-${sleeve.name}'),
                            style: OutlinedButton.styleFrom(
                              foregroundColor: t.p.alarm,
                              side: BorderSide(
                                color: t.p.alarm.withValues(alpha: 0.6),
                              ),
                            ),
                            onPressed: onKill,
                            child: const Text('KILL'),
                          ),
                          const SizedBox(width: 8),
                          FilledButton(
                            key: ValueKey('open-${sleeve.name}'),
                            onPressed: running ? onOpen : null,
                            child: Text(
                              chatMode ? 'OPEN CHAT' : 'OPEN TERMINAL',
                            ),
                          ),
                        ],
                      ),
                    ],
                  ],
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}
