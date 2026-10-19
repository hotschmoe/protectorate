import 'package:flutter/material.dart';

import '../gen/protectorate/v1/hub.pb.dart';
import '../hubs/event_feed.dart';
import '../hubs/hub_client.dart';

class OverviewTab extends StatefulWidget {
  const OverviewTab({super.key, required this.client, required this.feed});

  final HubClient client;
  final EventFeed feed;

  @override
  State<OverviewTab> createState() => _OverviewTabState();
}

class _OverviewTabState extends State<OverviewTab> {
  late Future<GetStatusResponse> _status = _loadStatus();
  EventFeed get _feed => widget.feed;

  Future<GetStatusResponse> _loadStatus() =>
      widget.client.hub.getStatus(GetStatusRequest());

  @override
  Widget build(BuildContext context) {
    return Column(
      children: [
        FutureBuilder<GetStatusResponse>(
          future: _status,
          builder: (context, snap) {
            if (snap.connectionState != ConnectionState.done) {
              return const LinearProgressIndicator();
            }
            if (snap.hasError) {
              return ListTile(
                leading: const Icon(Icons.error_outline),
                title: Text('${snap.error}'),
                trailing: IconButton(
                  tooltip: 'Retry',
                  icon: const Icon(Icons.refresh),
                  onPressed: () => setState(() => _status = _loadStatus()),
                ),
              );
            }
            final s = snap.requireData;
            return ListTile(
              leading: const Icon(Icons.dns),
              title: Text('${s.hostname} (${s.os}/${s.arch})'),
              subtitle: Text('version ${s.version}, up ${s.uptimeSeconds}s'),
            );
          },
        ),
        const Divider(height: 1),
        ListenableBuilder(
          listenable: _feed,
          builder: (context, _) => ListTile(
            dense: true,
            title: Text(switch (_feed.state) {
              FeedState.connecting => 'Events: connecting',
              FeedState.live => 'Events: live',
              FeedState.retrying =>
                'Events: retrying (attempt ${_feed.attempt}) ${_feed.lastError ?? ''}',
            }),
          ),
        ),
        Expanded(
          child: ListenableBuilder(
            listenable: _feed,
            builder: (context, _) {
              final events = _feed.events;
              return ListView.builder(
                itemCount: events.length,
                itemBuilder: (context, i) {
                  final e = events[i];
                  return ListTile(
                    dense: true,
                    title: Text(e.kind),
                    subtitle: Text(e.subject),
                    trailing: Text(e.hasTime()
                        ? e.time.toDateTime().toLocal().toString().substring(11, 19)
                        : ''),
                  );
                },
              );
            },
          ),
        ),
      ],
    );
  }
}
