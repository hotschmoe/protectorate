import 'package:flutter/material.dart';

import '../gen/protectorate/v1/events.pb.dart';
import '../hubs/event_feed.dart';
import '../hubs/hub_client.dart';
import '../hubs/hub_profile.dart';
import 'backlog_tab.dart';
import 'node_detail_screen.dart';
import 'nodes_tab.dart';
import 'overview_tab.dart';
import 'quota_tab.dart';
import 'sessions_tab.dart';

const phoneBreakpoint = 600.0;

class _Destination {
  const _Destination(this.id, this.label, this.icon);
  final String id;
  final String label;
  final IconData icon;
}

const _destinations = [
  _Destination('overview', 'Overview', Icons.dashboard),
  _Destination('nodes', 'Nodes', Icons.memory),
  _Destination('quota', 'Quota', Icons.speed),
  _Destination('sessions', 'Sessions', Icons.terminal),
  _Destination('backlog', 'Backlog', Icons.checklist),
];

int tabIndex(String? id) {
  final i = _destinations.indexWhere((d) => d.id == id);
  return i < 0 ? 0 : i;
}

class HubShell extends StatefulWidget {
  const HubShell({
    super.key,
    required this.hub,
    required this.clientFactory,
    this.initialTab,
    this.openSession,
    this.openNode,
  });

  final HubProfile hub;
  final HubClient Function(HubProfile) clientFactory;
  final String? initialTab;
  final String? openSession;
  final String? openNode;

  @override
  State<HubShell> createState() => _HubShellState();
}

class _HubShellState extends State<HubShell> {
  late final HubClient _client = widget.clientFactory(widget.hub);
  late final EventFeed _feed = EventFeed(
    () => _client.events.subscribe(SubscribeRequest()),
  )..start();
  late int _index = tabIndex(widget.initialTab);

  @override
  void initState() {
    super.initState();
    final node = widget.openNode;
    if (node != null) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (!mounted) return;
        Navigator.of(context).push(
          MaterialPageRoute<void>(
            builder: (_) =>
                NodeDetailScreen(client: _client, feed: _feed, name: node),
          ),
        );
      });
    }
  }

  @override
  void dispose() {
    _feed.dispose();
    super.dispose();
  }

  Widget _body() => switch (_index) {
    0 => OverviewTab(client: _client, feed: _feed),
    1 => NodesTab(client: _client, feed: _feed),
    2 => QuotaTab(client: _client, feed: _feed),
    3 => SessionsTab(
      hub: widget.hub,
      client: _client,
      openSession: widget.openSession,
    ),
    _ => BacklogTab(client: _client, feed: _feed),
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
