import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';

import '../approvals/action_approval_banner.dart';
import '../devices/device_approval_banner.dart';
import '../gen/protectorate/v1/events.pb.dart';
import '../hubs/event_feed.dart';
import '../hubs/hub_client.dart';
import '../hubs/hub_profile.dart';
import '../theme/tokens.dart';
import '../vfd/lamp.dart';
import '../vfd/panel.dart';
import '../mail/thread_view.dart' show orchestratorSleeve;
import 'audit_screen.dart';
import 'backlog_tab.dart';
import 'devices_screen.dart';
import 'orchestrator_tab.dart';
import 'overview_tab.dart';
import 'phone_devices_tab.dart';
import 'quota_tab.dart';
import 'sleeves_screen.dart';

const phoneBreakpoint = 600.0;

bool get usesPhoneShell =>
    (!kIsWeb && defaultTargetPlatform == TargetPlatform.android) ||
    (kIsWeb && kDebugMode && const bool.fromEnvironment('DEBUG_PHONE_SHELL'));

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
  _Destination('notes', 'NOTES', Icons.sticky_note_2_outlined),
  _Destination('devices', 'DEVICES', Icons.phonelink_lock),
];

const _phoneDestinations = [
  _Destination('orchestrator', 'ENVOY', Icons.support_agent),
  _Destination('quotas', 'QUOTAS', Icons.speed),
  _Destination('devices', 'DEVICES', Icons.phonelink_lock),
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
  });

  final HubProfile hub;
  final HubClient Function(HubProfile) clientFactory;
  final String? initialTab;
  final String? openSession;

  @override
  State<HubShell> createState() => _HubShellState();
}

class _HubShellState extends State<HubShell> {
  final _navigation = GlobalKey<NavigatorState>();
  final _revision = ValueNotifier(0);
  late final HubClient _client = widget.clientFactory(widget.hub);
  late final EventFeed _feed = EventFeed(
    (signal) => _client.events.subscribe(SubscribeRequest(), signal: signal),
  )..start();
  late int _index = usesPhoneShell
      ? _phoneDestinations
            .indexWhere((d) => d.id == widget.initialTab)
            .clamp(0, _phoneDestinations.length - 1)
      : tabIndex(widget.initialTab);
  String _orchestratorSleeve = orchestratorSleeve;
  late bool _orchestratorMail = widget.initialTab == mailTab;
  DateTime _clock = DateTime.now();
  Timer? _tick;

  @override
  void initState() {
    super.initState();
    if (!usesPhoneShell) {
      _tick = Timer.periodic(const Duration(seconds: 15), (_) {
        if (mounted) _update(() => _clock = DateTime.now());
      });
    }
  }

  @override
  void dispose() {
    _tick?.cancel();
    _feed.dispose();
    _revision.dispose();
    super.dispose();
  }

  Widget _body() => switch (_destinations[_index].id) {
    'orchestrator' => OrchestratorTab(
      client: _client,
      feed: _feed,
      selected: _orchestratorSleeve,
      onSelect: (n) => _update(() => _orchestratorSleeve = n),
      mail: _orchestratorMail,
      onMail: (m) => _update(() => _orchestratorMail = m),
    ),
    'overview' => OverviewTab(client: _client, feed: _feed),
    'sleeves' => SleevesScreen(
      hub: widget.hub,
      client: _client,
      feed: _feed,
      openSession: widget.openSession,
    ),
    'devices' => DevicesScreen(client: _client, feed: _feed, embedded: true),
    _ => BacklogTab(client: _client, feed: _feed),
  };

  Widget _phoneBody(String id) => switch (id) {
    'orchestrator' => OrchestratorTab(
      client: _client,
      feed: _feed,
      selected: orchestratorSleeve,
      onSelect: (_) {},
      mail: false,
      onMail: (_) {},
      envoyOnly: true,
    ),
    'quotas' => QuotaTab(client: _client, feed: _feed),
    _ => PhoneDevicesTab(
      client: _client,
      feed: _feed,
      devices: DevicesScreen(client: _client, feed: _feed, embedded: true),
    ),
  };

  void _go(int i) {
    FocusScope.of(context).unfocus();
    _update(() => _index = i);
  }

  void _update(VoidCallback change) {
    change();
    _revision.value++;
  }

  @override
  Widget build(BuildContext context) {
    final available =
        MediaQuery.sizeOf(context).height -
        MediaQuery.viewInsetsOf(context).bottom;
    return SafeArea(
      bottom: false,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        verticalDirection: VerticalDirection.up,
        children: [
          Expanded(
            child: NavigatorPopHandler<void>(
              onPopWithResult: (_) => _navigation.currentState?.maybePop(),
              child: Navigator(
                key: _navigation,
                onGenerateRoute: (settings) => MaterialPageRoute<void>(
                  settings: settings,
                  builder: (_) => ListenableBuilder(
                    listenable: _revision,
                    builder: (context, _) => _buildShell(context),
                  ),
                ),
              ),
            ),
          ),
          ConstrainedBox(
            constraints: BoxConstraints(
              maxHeight: available.clamp(0, double.infinity) * 0.28,
            ),
            child: SingleChildScrollView(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  ActionApprovalBanner(client: _client, feed: _feed),
                  DeviceApprovalBanner(client: _client, feed: _feed),
                ],
              ),
            ),
          ),
        ],
      ),
    );
  }

  void _leaveHub() => Navigator.of(context).maybePop();

  Widget _buildShell(BuildContext context) {
    final t = Tokens.of(context);
    if (usesPhoneShell) return _phoneScaffold(context);
    final wide = MediaQuery.sizeOf(context).width >= phoneBreakpoint;
    final hh = _clock.hour.toString().padLeft(2, '0');
    final mm = _clock.minute.toString().padLeft(2, '0');
    return Scaffold(
      appBar: AppBar(
        leading: BackButton(onPressed: _leaveHub),
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

  Widget _phoneScaffold(BuildContext context) {
    final t = Tokens.of(context);
    final keyboard = MediaQuery.viewInsetsOf(context).bottom > 0;
    return Scaffold(
      appBar: AppBar(
        leading: BackButton(onPressed: _leaveHub),
        titleSpacing: 0,
        title: Row(
          children: [
            Image.asset('assets/protectorate.png', width: 28, height: 28),
            const SizedBox(width: 10),
            Expanded(
              child: Text(
                widget.hub.name,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: t.title(),
              ),
            ),
          ],
        ),
        actions: [
          ListenableBuilder(
            listenable: _feed,
            builder: (context, _) => Tooltip(
              message: 'Hub connection: ${_feed.state.name}',
              child: Semantics(
                label: 'Hub connection ${_feed.state.name}',
                child: Lamp(
                  tone: switch (_feed.state) {
                    FeedState.connecting => Tone.info,
                    FeedState.live => Tone.ok,
                    FeedState.retrying => Tone.alarm,
                  },
                ),
              ),
            ),
          ),
          const SizedBox(width: 20),
        ],
      ),
      body: SafeArea(
        top: false,
        child: IndexedStack(
          index: _index,
          children: [for (final d in _phoneDestinations) _phoneBody(d.id)],
        ),
      ),
      bottomNavigationBar: keyboard
          ? null
          : _BottomBar(
              index: _index,
              onSelect: _go,
              destinations: _phoneDestinations,
            ),
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
  const _BottomBar({
    required this.index,
    required this.onSelect,
    this.destinations = _destinations,
  });

  final int index;
  final void Function(int) onSelect;
  final List<_Destination> destinations;

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
              for (var i = 0; i < destinations.length; i++)
                Expanded(
                  child: _NavKey(
                    d: destinations[i],
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
