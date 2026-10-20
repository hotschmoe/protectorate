import 'dart:async';

import 'package:flutter/material.dart';

import '../gen/protectorate/v1/events.pb.dart';
import '../hubs/event_feed.dart';
import '../hubs/hub_client.dart';
import '../hubs/hub_profile.dart';
import '../gen/protectorate/v1/hub.pb.dart';
import '../nodes/node_view.dart';
import '../nodes/version_skew.dart';
import '../version.dart' as ver;
import '../theme/tokens.dart';
import '../vfd/lamp.dart';
import '../vfd/panel.dart';
import '../mail/thread_view.dart' show orchestratorSleeve;
import 'audit_screen.dart';
import 'backlog_tab.dart';
import 'node_detail_screen.dart';
import 'nodes_tab.dart';
import 'orchestrator_tab.dart';
import 'overview_tab.dart';
import 'sleeves_screen.dart';

const phoneBreakpoint = 600.0;

class _Destination {
  const _Destination(this.id, this.label, this.icon);
  final String id;
  final String label;
  final IconData icon;
}

const _destinations = [
  _Destination('orchestrator', 'ENVOY', Icons.support_agent),
  _Destination('overview', 'OVERVIEW', Icons.grid_view),
  _Destination('sleeves', 'SLEEVES', Icons.smart_toy_outlined),
  _Destination('nodes', 'NODES', Icons.memory),
  _Destination('notes', 'NOTES', Icons.sticky_note_2_outlined),
];

// The mail view lives in the orchestrator tab; its link id lands there.
const mailTab = 'mail';

int tabIndex(String? id) {
  final i = _destinations.indexWhere(
    (d) => d.id == (id == mailTab ? 'orchestrator' : id),
  );
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
    this.appVersion = ver.appVersion,
  });

  final String appVersion;

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
  final _history = NodeHistory();
  String _orchestratorSleeve = orchestratorSleeve;
  late bool _orchestratorMail = widget.initialTab == mailTab;
  String _hubVersion = '';
  FeedState _lastFeedState = FeedState.connecting;
  DateTime _clock = DateTime.now();
  Timer? _tick;

  @override
  void initState() {
    super.initState();
    _loadHubVersion();
    _feed.addListener(_feedChanged);
    _tick = Timer.periodic(const Duration(seconds: 15), (_) {
      if (mounted) setState(() => _clock = DateTime.now());
    });
    final node = widget.openNode;
    if (node != null) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (!mounted) return;
        Navigator.of(context).push(
          PageRouteBuilder<void>(
            transitionDuration: Duration.zero,
            reverseTransitionDuration: Duration.zero,
            pageBuilder: (_, _, _) => NodeDetailScreen(
              hub: widget.hub,
              client: _client,
              feed: _feed,
              name: node,
              history: _history,
              versions: _versions,
            ),
          ),
        );
      });
    }
  }

  Versions get _versions => Versions(hub: _hubVersion, app: widget.appVersion);

  Future<void> _loadHubVersion() async {
    try {
      final s = await _client.hub.getStatus(GetStatusRequest());
      if (mounted) setState(() => _hubVersion = s.version);
    } on Object {
      // Skew is hidden until the hub's version is known.
    }
  }

  // A hub that restarted may run another build; read it again when the event
  // stream comes back.
  void _feedChanged() {
    final s = _feed.state;
    if (s == FeedState.live && _lastFeedState != FeedState.live) {
      _loadHubVersion();
    }
    _lastFeedState = s;
  }

  @override
  void dispose() {
    _feed.removeListener(_feedChanged);
    _tick?.cancel();
    _feed.dispose();
    super.dispose();
  }

  Widget _body() => switch (_destinations[_index].id) {
    'orchestrator' => OrchestratorTab(
      client: _client,
      feed: _feed,
      selected: _orchestratorSleeve,
      onSelect: (n) => setState(() => _orchestratorSleeve = n),
      mail: _orchestratorMail,
      onMail: (m) => setState(() => _orchestratorMail = m),
    ),
    'overview' => OverviewTab(
      hub: widget.hub,
      client: _client,
      feed: _feed,
      history: _history,
      versions: _versions,
    ),
    'nodes' => NodesTab(
      hub: widget.hub,
      client: _client,
      feed: _feed,
      history: _history,
      versions: _versions,
    ),
    'sleeves' => SleevesScreen(
      hub: widget.hub,
      client: _client,
      feed: _feed,
      embedded: true,
      openSession: widget.openSession,
    ),
    _ => BacklogTab(client: _client, feed: _feed),
  };

  void _go(int i) => setState(() => _index = i);

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final wide = MediaQuery.sizeOf(context).width >= phoneBreakpoint;
    final hh = _clock.hour.toString().padLeft(2, '0');
    final mm = _clock.minute.toString().padLeft(2, '0');
    return Scaffold(
      appBar: AppBar(
        title: Text(widget.hub.name),
        actions: [
          IconButton(
            key: const ValueKey('open-audit'),
            tooltip: 'Audit log',
            icon: const Icon(Icons.receipt_long_outlined),
            onPressed: () => Navigator.of(context).push(
              MaterialPageRoute<void>(
                builder: (_) => AuditScreen(client: _client),
              ),
            ),
          ),
          const SizedBox(width: 6),
          ListenableBuilder(
            listenable: _feed,
            builder: (context, _) => Tooltip(
              message: 'Event stream: ${_feed.state.name}',
              child: Lamp(
                tone: switch (_feed.state) {
                  FeedState.connecting => Tone.info,
                  FeedState.live => Tone.ok,
                  FeedState.retrying => Tone.alarm,
                },
              ),
            ),
          ),
          const SizedBox(width: 14),
          Semantics(
            label: 'clock $hh:$mm',
            child: SegmentReadout(
              value: '$hh:$mm',
              size: Tokens.readoutM,
              color: t.p.lit,
            ),
          ),
          const SizedBox(width: Tokens.gapL),
        ],
      ),
      body: wide
          ? Row(
              children: [
                _Rail(index: _index, onSelect: _go),
                Expanded(child: _body()),
              ],
            )
          : _body(),
      bottomNavigationBar: wide
          ? null
          : _BottomBar(index: _index, onSelect: _go),
    );
  }
}

class _NavKey extends StatelessWidget {
  const _NavKey({
    required this.d,
    required this.selected,
    required this.onTap,
    required this.height,
  });

  final _Destination d;
  final bool selected;
  final VoidCallback onTap;
  final double height;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final c = selected ? t.p.info : t.p.legend;
    return Semantics(
      selected: selected,
      button: true,
      child: InkWell(
        key: ValueKey('nav-${d.id}'),
        onTap: onTap,
        child: AnimatedContainer(
          duration: MediaQuery.disableAnimationsOf(context)
              ? Duration.zero
              : const Duration(milliseconds: 180),
          height: height,
          decoration: BoxDecoration(
            color: selected ? t.tint(t.p.info) : null,
            borderRadius: BorderRadius.circular(Tokens.innerRadius + 3),
          ),
          child: Column(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              Lamp(tone: Tone.info, on: selected, size: 6),
              const SizedBox(height: 6),
              Icon(d.icon, size: 20, color: c),
              const SizedBox(height: 4),
              Text(
                d.label,
                maxLines: 1,
                overflow: TextOverflow.fade,
                softWrap: false,
                style: t.legend(c).copyWith(fontSize: 10.5, letterSpacing: 0.8),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class _BottomBar extends StatelessWidget {
  const _BottomBar({required this.index, required this.onSelect});

  final int index;
  final void Function(int) onSelect;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    return Container(
      key: const ValueKey('nav-bar'),
      decoration: BoxDecoration(
        color: t.p.frame,
        border: Border(top: BorderSide(color: t.p.bezel)),
      ),
      child: SafeArea(
        top: false,
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 4, vertical: 4),
          child: Row(
            children: [
              for (var i = 0; i < _destinations.length; i++)
                Expanded(
                  child: _NavKey(
                    d: _destinations[i],
                    selected: i == index,
                    onTap: () => onSelect(i),
                    height: 64,
                  ),
                ),
            ],
          ),
        ),
      ),
    );
  }
}

class _Rail extends StatelessWidget {
  const _Rail({required this.index, required this.onSelect});

  final int index;
  final void Function(int) onSelect;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    return Container(
      key: const ValueKey('nav-rail'),
      width: 100,
      decoration: BoxDecoration(
        color: t.p.frame,
        border: Border(right: BorderSide(color: t.p.bezel)),
      ),
      child: CustomPaint(
        painter: t.dark ? null : _TractorFeed(t),
        child: Padding(
          padding: const EdgeInsets.fromLTRB(14, 12, 8, 12),
          child: SingleChildScrollView(
            child: Column(
              children: [
                for (var i = 0; i < _destinations.length; i++)
                  Padding(
                    padding: const EdgeInsets.only(bottom: 6),
                    child: _NavKey(
                      d: _destinations[i],
                      selected: i == index,
                      onTap: () => onSelect(i),
                      height: 76,
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

// The punched sprocket margin of continuous-form paper, on the printout only.
class _TractorFeed extends CustomPainter {
  _TractorFeed(this.t);

  final Tokens t;

  @override
  void paint(Canvas canvas, Size size) {
    final hole = Paint()..color = t.p.chassis;
    final rim = Paint()
      ..color = t.p.bezel
      ..style = PaintingStyle.stroke;
    for (var y = 12.0; y < size.height; y += 24) {
      canvas.drawCircle(Offset(7, y), 3.2, hole);
      canvas.drawCircle(Offset(7, y), 3.2, rim);
    }
    final perf = Paint()..color = t.p.bezel;
    for (var y = 0.0; y < size.height; y += 6) {
      canvas.drawRect(Rect.fromLTWH(14, y, 1, 3), perf);
    }
  }

  @override
  bool shouldRepaint(_TractorFeed old) => old.t != t;
}
