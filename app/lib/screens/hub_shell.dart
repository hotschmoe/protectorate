import 'package:flutter/material.dart';

import '../gen/protectorate/v1/events.pb.dart';
import '../hubs/event_feed.dart';
import '../hubs/hub_client.dart';
import '../hubs/hub_profile.dart';
import 'overview_tab.dart';
import 'quota_tab.dart';
import 'sessions_tab.dart';

const phoneBreakpoint = 600.0;

class _Destination {
  const _Destination(this.label, this.icon);
  final String label;
  final IconData icon;
}

const _destinations = [
  _Destination('Overview', Icons.dashboard),
  _Destination('Quota', Icons.speed),
  _Destination('Sessions', Icons.terminal),
  _Destination('Backlog', Icons.checklist),
];

class HubShell extends StatefulWidget {
  const HubShell({super.key, required this.hub, required this.clientFactory});

  final HubProfile hub;
  final HubClient Function(HubProfile) clientFactory;

  @override
  State<HubShell> createState() => _HubShellState();
}

class _HubShellState extends State<HubShell> {
  late final HubClient _client = widget.clientFactory(widget.hub);
  late final EventFeed _feed = EventFeed(
    () => _client.events.subscribe(SubscribeRequest()),
  )..start();
  int _index = 0;

  @override
  void dispose() {
    _feed.dispose();
    super.dispose();
  }

  Widget _body() => switch (_index) {
    0 => OverviewTab(client: _client, feed: _feed),
    1 => QuotaTab(client: _client, feed: _feed),
    2 => SessionsTab(hub: widget.hub, client: _client),
    _ => _Placeholder(title: _destinations[_index].label),
  };

  @override
  Widget build(BuildContext context) {
    final wide = MediaQuery.sizeOf(context).width >= phoneBreakpoint;
    return Scaffold(
      appBar: AppBar(title: Text(widget.hub.name)),
      body: wide
          ? Row(
              children: [
                NavigationRail(
                  selectedIndex: _index,
                  labelType: NavigationRailLabelType.all,
                  onDestinationSelected: (i) => setState(() => _index = i),
                  destinations: [
                    for (final d in _destinations)
                      NavigationRailDestination(
                        icon: Icon(d.icon),
                        label: Text(d.label),
                      ),
                  ],
                ),
                const VerticalDivider(width: 1),
                Expanded(child: _body()),
              ],
            )
          : _body(),
      bottomNavigationBar: wide
          ? null
          : NavigationBar(
              selectedIndex: _index,
              onDestinationSelected: (i) => setState(() => _index = i),
              destinations: [
                for (final d in _destinations)
                  NavigationDestination(icon: Icon(d.icon), label: d.label),
              ],
            ),
    );
  }
}

class _Placeholder extends StatelessWidget {
  const _Placeholder({required this.title});

  final String title;

  @override
  Widget build(BuildContext context) =>
      Center(child: Text('$title: not wired yet'));
}
