import 'package:flutter/material.dart';

import '../chat/badges.dart';
import '../gen/protectorate/v1/chat.pb.dart';
import '../gen/protectorate/v1/sleeve.pb.dart';
import '../nodes/node_view.dart';
import '../theme/tokens.dart';
import 'sleeve_info.dart';

class SleeveCard extends StatelessWidget {
  const SleeveCard({
    super.key,
    required this.sleeve,
    required this.now,
    required this.onOpen,
    required this.onKill,
  });

  final Sleeve sleeve;
  final DateTime now;
  final VoidCallback onOpen;
  final VoidCallback onKill;

  ChatState get _state {
    if (sleeve.state == SleeveState.SLEEVE_STATE_EXITED) {
      return ChatState.CHAT_STATE_EXITED;
    }
    return sleeve.hasChat()
        ? sleeve.chat.state
        : ChatState.CHAT_STATE_UNSPECIFIED;
  }

  @override
  Widget build(BuildContext context) {
    final cs = Theme.of(context).colorScheme;
    final text = Theme.of(context).textTheme;
    final accent = Tokens.of(context).model(
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
    return Card(
      key: ValueKey('sleeve-${sleeve.name}'),
      clipBehavior: Clip.antiAlias,
      margin: EdgeInsets.zero,
      child: InkWell(
        onTap: running ? onOpen : null,
        child: IntrinsicHeight(
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Container(
                key: const ValueKey('harness-accent'),
                width: 6,
                color: accent.fg,
              ),
              Expanded(
                child: Padding(
                  padding: const EdgeInsets.all(Tokens.gap),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Row(
                        children: [
                          Expanded(
                            child: Text(
                              sleeve.name,
                              overflow: TextOverflow.ellipsis,
                              style: text.titleMedium?.copyWith(
                                fontWeight: FontWeight.w700,
                              ),
                            ),
                          ),
                          if (_state != ChatState.CHAT_STATE_UNSPECIFIED)
                            StatusPill(state: _state)
                          else if (running)
                            const StatusPill(
                              state: ChatState.CHAT_STATE_IDLE,
                              label: 'Running',
                            ),
                        ],
                      ),
                      const SizedBox(height: 6),
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
                            Text(model, style: text.labelMedium),
                          if (isLocalModel(sleeve)) const LocalModelBadge(),
                          Text(
                            chatMode ? 'chat' : 'terminal',
                            style: text.labelSmall?.copyWith(
                              color: cs.onSurfaceVariant,
                            ),
                          ),
                        ],
                      ),
                      const SizedBox(height: 6),
                      Text(
                        '$workspaceName${uptime == null ? '' : '  up $uptime'}',
                        style: text.bodySmall,
                      ),
                      Text(
                        sleeve.workspace,
                        overflow: TextOverflow.ellipsis,
                        style: text.bodySmall?.copyWith(
                          color: cs.onSurfaceVariant,
                        ),
                      ),
                      if (sleeve.hasChat() &&
                          sleeve.chat.contextWindow > 0) ...[
                        const SizedBox(height: 8),
                        ContextMeter(
                          tokens: sleeve.chat.contextTokens.toInt(),
                          window: sleeve.chat.contextWindow.toInt(),
                        ),
                      ],
                      if (running) ...[
                        const SizedBox(height: 6),
                        PercentBar(
                          label: 'CPU',
                          percent: cpuLimit > 0
                              ? u.cpuPercent * 100 / cpuLimit
                              : null,
                          trailing:
                              '${u.cpuPercent.toStringAsFixed(0)}%${cpuLimit > 0 ? ' / $cpuLimit%' : ''}',
                        ),
                        PercentBar(
                          label: 'MEM',
                          percent: memLimit > 0
                              ? u.memoryCurrentBytes.toInt() * 100 / memLimit
                              : null,
                          trailing:
                              '${formatBytes(u.memoryCurrentBytes.toInt())}${memLimit > 0 ? ' / ${formatBytes(memLimit)}' : ''}',
                        ),
                        if (u.oomKills > 0)
                          Text(
                            '${u.oomKills} OOM kills',
                            style: text.bodySmall?.copyWith(color: cs.error),
                          ),
                      ],
                      OverflowBar(
                        alignment: MainAxisAlignment.end,
                        children: [
                          FilledButton.tonal(
                            key: ValueKey('open-${sleeve.name}'),
                            onPressed: running ? onOpen : null,
                            child: Text(
                              chatMode ? 'Open chat' : 'Open terminal',
                            ),
                          ),
                          TextButton(
                            key: ValueKey('kill-${sleeve.name}'),
                            style: TextButton.styleFrom(
                              foregroundColor: cs.error,
                            ),
                            onPressed: onKill,
                            child: const Text('Kill'),
                          ),
                        ],
                      ),
                    ],
                  ),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}
