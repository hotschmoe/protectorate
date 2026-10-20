import 'package:flutter/material.dart';

import '../hubs/hub_client.dart';
import '../hubs/hub_profile.dart';
import '../version.dart';
import '../theme/tokens.dart';
import '../vfd/panel.dart';
import 'hub_shell.dart';
import 'pair_dialog.dart';

class HubsScreen extends StatelessWidget {
  const HubsScreen({
    super.key,
    required this.store,
    required this.clientFactory,
  });

  final HubStore store;
  final HubClient Function(HubProfile) clientFactory;

  Future<void> _edit(BuildContext context, [HubProfile? existing]) async {
    final connected = await connectHub(
      context,
      existing: existing,
      clientFactory: clientFactory,
      save: store.upsert,
    );
    if (connected != null && context.mounted) _open(context, connected);
  }

  void _open(BuildContext context, HubProfile hub) =>
      Navigator.of(context).push(
        MaterialPageRoute<void>(
          builder: (_) => HubShell(hub: hub, clientFactory: clientFactory),
        ),
      );

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    return Scaffold(
      appBar: AppBar(title: const Text('Protectorate')),
      floatingActionButton: FloatingActionButton.extended(
        heroTag: null,
        key: const ValueKey('add-hub'),
        onPressed: () => _edit(context),
        icon: const Icon(Icons.add),
        label: const Text('ADD HUB'),
      ),
      body: ListenableBuilder(
        listenable: store,
        builder: (context, _) {
          final hubs = store.hubs;
          return ListView(
            padding: const EdgeInsets.fromLTRB(
              Tokens.gap,
              Tokens.gapL,
              Tokens.gap,
              96,
            ),
            children: [
              Center(
                child: ConstrainedBox(
                  constraints: const BoxConstraints(
                    maxWidth: Tokens.contentMaxWidth,
                  ),
                  child: Panel(
                    legend: 'HUBS',
                    trailing: Text('${hubs.length} PAIRED', style: t.legend()),
                    padding: const EdgeInsets.all(4),
                    child: hubs.isEmpty
                        ? Padding(
                            padding: const EdgeInsets.all(20),
                            child: Column(
                              children: [
                                Text('NO HUBS', style: t.legend()),
                                const SizedBox(height: 8),
                                Text(
                                  'Enter your hub URL, or scan the QR from protectorate pair. A trusted device approves your connection.',
                                  textAlign: TextAlign.center,
                                  style: t.small(),
                                ),
                              ],
                            ),
                          )
                        : Column(
                            children: [
                              for (var i = 0; i < hubs.length; i++)
                                _row(context, t, i, hubs[i]),
                            ],
                          ),
                  ),
                ),
              ),
              const SizedBox(height: Tokens.gap),
              Center(
                child: ConstrainedBox(
                  constraints: const BoxConstraints(
                    maxWidth: Tokens.contentMaxWidth,
                  ),
                  child: Panel(
                    legend: 'CONNECT',
                    trailing: Text('APP $appVersion', style: t.legend()),
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          'On the hub, run the command below and scan its QR '
                          'code, or open its link on this device. Compare the '
                          'code shown here with the approval request on a trusted device.',
                          style: t.small(),
                        ),
                        const SizedBox(height: 10),
                        Container(
                          padding: const EdgeInsets.symmetric(
                            horizontal: 12,
                            vertical: 10,
                          ),
                          decoration: BoxDecoration(
                            color: t.p.bar,
                            border: Border.all(color: t.p.bezel),
                            borderRadius: BorderRadius.circular(
                              Tokens.innerRadius,
                            ),
                          ),
                          child: Text.rich(
                            TextSpan(
                              children: [
                                TextSpan(text: '> ', style: t.data(t.p.info)),
                                TextSpan(
                                  text: 'protectorate pair',
                                  style: t.data(t.p.emph),
                                ),
                              ],
                            ),
                          ),
                        ),
                      ],
                    ),
                  ),
                ),
              ),
            ],
          );
        },
      ),
    );
  }

  Widget _row(BuildContext context, Tokens t, int i, HubProfile hub) => InkWell(
    key: ValueKey('hub-${hub.id}'),
    borderRadius: BorderRadius.circular(Tokens.innerRadius),
    onTap: () => _open(context, hub),
    child: BarRow(
      index: i,
      padding: const EdgeInsets.fromLTRB(14, 8, 0, 8),
      child: Row(
        children: [
          Icon(Icons.hub_outlined, color: t.p.lit, size: 22),
          const SizedBox(width: 14),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(hub.name, style: t.title()),
                const SizedBox(height: 2),
                Text(
                  hub.baseUrl,
                  style: t.small(),
                  overflow: TextOverflow.ellipsis,
                ),
                if (hub.token.isEmpty)
                  Text('NO TOKEN', style: t.legend(t.p.warn)),
              ],
            ),
          ),
          IconButton(
            tooltip: 'Reconnect',
            icon: const Icon(Icons.link),
            onPressed: () => _edit(context, hub),
          ),
          IconButton(
            tooltip: 'Remove',
            icon: const Icon(Icons.delete_outline),
            onPressed: () => store.remove(hub.id),
          ),
        ],
      ),
    ),
  );
}
