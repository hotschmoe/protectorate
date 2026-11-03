import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'chat.dart';
import 'client_update.dart';
import 'hub.dart';
import 'api/api.g.dart';
import 'derived.dart';
import 'icons.dart';
import 'scope.dart';
import 'screens/approvals.dart';
import 'screens/events.dart';
import 'screens/nodes.dart';
import 'screens/quota.dart';
import 'screens/settings.dart';
import 'screens/sleeves.dart';
import 'theme.dart';
import 'widgets.dart';

const _nav = <(Screen, String, Glyph, String, LogicalKeyboardKey)>[
  (Screen.envoy, 'Envoy', Glyph.envoy, 'F1', LogicalKeyboardKey.f1),
  (Screen.sleeves, 'Sleeves', Glyph.sleeves, 'F2', LogicalKeyboardKey.f2),
  (Screen.approvals, 'Approvals', Glyph.approvals, 'F3', LogicalKeyboardKey.f3),
  (Screen.nodes, 'Nodes', Glyph.nodes, 'F4', LogicalKeyboardKey.f4),
  (Screen.quota, 'Quota', Glyph.quota, 'F5', LogicalKeyboardKey.f5),
  (Screen.events, 'Mail and events', Glyph.events, 'F6', LogicalKeyboardKey.f6),
  (Screen.settings, 'Settings', Glyph.settings, 'F12', LogicalKeyboardKey.f12),
];

/// The navigation entry a screen belongs to.
Screen _home(Screen s) => switch (s) {
  Screen.sleeve => Screen.sleeves,
  Screen.node => Screen.nodes,
  Screen.hub => Screen.nodes,
  _ => s,
};

/// The Envoy screen shows events, nodes and quota beside the chat from this content width.
const double _sidePanelFrom = 1040;

class Shell extends StatefulWidget {
  const Shell({
    super.key,
    required this.hub,
    required this.dark,
    required this.setDark,
    required this.hubUrl,
    required this.hubToken,
    required this.connect,
    required this.pair,
    this.initial = Screen.envoy,
    this.initialSleeve,
    this.initialNode,
  });

  final HubClient hub;
  final bool dark;
  final void Function(bool) setDark;
  final String hubUrl;
  final String hubToken;
  final void Function(String url, String token) connect;
  final Future<String> Function(String code) pair;
  final Screen initial;
  final String? initialSleeve;
  final String? initialNode;

  @override
  State<Shell> createState() => _ShellState();
}

class _ShellState extends State<Shell> {
  late HubState _state = widget.hub.state;
  late StreamSubscription<HubState> _sub;
  late Screen _screen = widget.initial;
  late String? _sleeveId = widget.initialSleeve;
  late String? _nodeId = widget.initialNode;
  final _scaffold = GlobalKey<ScaffoldState>();
  final _cmd = TextEditingController();
  final _cmdFocus = FocusNode();
  bool _cmdOpen = false;
  String? _msg;
  Level _msgLevel = Level.info;
  Timer? _msgTimer;

  /// The hub version whose update offer was dismissed, until the app restarts.
  static String? _dismissedUpdate;

  @override
  void initState() {
    super.initState();
    _sub = widget.hub.states.listen((s) => setState(() => _state = s));
    HardwareKeyboard.instance.addHandler(_onKey);
    _cmd.addListener(() => setState(() {}));
  }

  @override
  void dispose() {
    HardwareKeyboard.instance.removeHandler(_onKey);
    unawaited(_sub.cancel());
    _msgTimer?.cancel();
    _cmd.dispose();
    _cmdFocus.dispose();
    super.dispose();
  }

  bool _onKey(KeyEvent e) {
    if (e is! KeyDownEvent) return false;
    for (final (screen, _, _, _, key) in _nav) {
      if (e.logicalKey == key) {
        _go(screen);
        return true;
      }
    }
    final k = HardwareKeyboard.instance;
    if (e.logicalKey == LogicalKeyboardKey.escape) {
      _setPalette(!_cmdOpen);
      return true;
    }
    if (e.logicalKey == LogicalKeyboardKey.keyK && (k.isControlPressed || k.isMetaPressed)) {
      _setPalette(true);
      return true;
    }
    return false;
  }

  void _setPalette(bool open) {
    setState(() => _cmdOpen = open);
    if (open) {
      _cmdFocus.requestFocus();
    } else {
      _cmd.clear();
      _cmdFocus.unfocus();
    }
  }

  void _go(Screen s) => setState(() => _screen = s);

  void _openSleeve(String id) {
    final s = _state.sleeve(id);
    setState(() {
      if (s?.role == SleeveRole.envoy) {
        _screen = Screen.envoy;
      } else {
        _screen = Screen.sleeve;
        _sleeveId = id;
      }
    });
  }

  void _openNode(String id) => setState(() {
    _screen = Screen.node;
    _nodeId = id;
  });

  void _say(String text, {Level level = Level.info}) {
    _msgTimer?.cancel();
    setState(() {
      _msg = text;
      _msgLevel = level;
    });
    _msgTimer = Timer(const Duration(seconds: 8), () {
      if (mounted) setState(() => _msg = null);
    });
  }

  /// The offer when the hub runs another release: its build of this app, or a reload on web.
  Widget? _update() {
    final h = _state.hub;
    if (!widget.hub.connected || h.version == kVersion || h.version == _dismissedUpdate) return null;
    final asset = clientAsset();
    if (!reloadsFromHub && (asset == null || !h.clients.contains(asset))) return null;
    return _UpdateBanner(
      key: ValueKey(h.version),
      version: h.version,
      asset: asset,
      onDismiss: () => setState(() => _dismissedUpdate = h.version),
    );
  }

  String? get _currentSleeve => switch (_screen) {
    Screen.sleeve => _sleeveId,
    Screen.envoy => _state.envoy?.id,
    _ => null,
  };

  X? _match<X>(Iterable<X> items, String Function(X) name, String q, String what) {
    final l = q.toLowerCase();
    final exact = items.where((x) => name(x).toLowerCase() == l).toList();
    if (exact.length == 1) return exact.first;
    var hits = items.where((x) => name(x).toLowerCase().startsWith(l)).toList();
    if (hits.isEmpty) hits = items.where((x) => name(x).toLowerCase().contains(l)).toList();
    if (hits.length == 1) return hits.first;
    _say(
      hits.isEmpty ? 'No $what matches "$q"' : '"$q" is ambiguous: ${hits.map(name).join(', ')}',
      level: Level.alarm,
    );
    return null;
  }

  void _run(AppScope app, String raw) {
    final t = raw.trim();
    _cmd.clear();
    if (t.isEmpty) return;
    if (t.startsWith('>')) {
      final env = _state.envoy;
      final text = t.substring(1).trim();
      if (env == null || text.isEmpty) return;
      _go(Screen.envoy);
      unawaited(app.act(() => widget.hub.send(env.id, text)));
      return;
    }
    final parts = t.split(RegExp(r'\s+'));
    final head = parts.first.toLowerCase();
    final arg = parts.skip(1).join(' ');
    switch (head) {
      case '?' || 'help':
        _say(commandHelp.map((c) => c.$1).join('   '));
      case 'e' || 'envoy':
        _go(Screen.envoy);
      case 's' || 'sleeve' || 'sleeves':
        if (arg.isEmpty) return _go(Screen.sleeves);
        final s = _match(_state.sleeves, (x) => x.name, arg, 'sleeve');
        if (s != null) _openSleeve(s.id);
      case 'n' || 'node' || 'nodes':
        if (arg.isEmpty) return _go(Screen.nodes);
        if (arg.toLowerCase() == 'hub') return _go(Screen.hub);
        final n = _match(_state.nodes, (x) => x.name, arg, 'node');
        if (n != null) _openNode(n.id);
      case 'a' || 'appr' || 'approvals':
        _go(Screen.approvals);
      case 'q' || 'quota':
        _go(Screen.quota);
      case 'ev' || 'events' || 'mail':
        _go(Screen.events);
      case 'set' || 'settings':
        _go(Screen.settings);
      case 'theme':
        widget.setDark(switch (arg) {
          'dark' || 'amber' => true,
          'light' || 'paper' => false,
          _ => !widget.dark,
        });
      case '/approve' || '/deny':
        final pending = _state.pending;
        Approval? a;
        if (arg.isNotEmpty) {
          a = _match(pending, (x) => x.id, arg, 'pending approval');
        } else {
          final here = pending.where((x) => x.sleeveId == _currentSleeve).toList();
          if (here.length == 1) {
            a = here.first;
          } else if (pending.length == 1) {
            a = pending.first;
          } else {
            _say('${pending.length} pending; give an id (F3 lists them)', level: Level.warn);
          }
        }
        if (a != null) {
          final id = a.id;
          final ok = head == '/approve';
          unawaited(app.act(() => widget.hub.decide(id, ok ? Decision.allowOnce : Decision.deny),
              done: '${ok ? 'Approved' : 'Denied'} $id'));
        }
      case '/interrupt' || '/stop' || '/resleeve':
        final id = _currentSleeve;
        if (id == null) return _say('Open a sleeve first', level: Level.warn);
        if (head == '/interrupt') {
          unawaited(app.act(() => widget.hub.interrupt(id)));
        } else if (head == '/stop') {
          unawaited(app.act(() => widget.hub.stop(id)));
        } else {
          final h = Harness.fromJson(arg);
          if (h == Harness.unknown) {
            return _say('Resleeve to what? claude codex grok pi_local pi_vendor', level: Level.warn);
          }
          unawaited(app.act(() => widget.hub.resleeve(id, h), done: 'Resleeving to ${h.wire}'));
        }
      default:
        _say('Unknown command: $head  (? for help)', level: Level.alarm);
    }
  }

  @override
  Widget build(BuildContext context) {
    final p = widget.dark ? Palette.amber : Palette.paper;
    return LayoutBuilder(builder: (context, box) {
      final phone = box.maxWidth < kPhoneWidth;
      return Look(
        p: p,
        phone: phone,
        child: Builder(builder: (context) {
          final app = AppScope(
            hub: widget.hub,
            state: _state,
            go: _go,
            openSleeve: _openSleeve,
            openNode: _openNode,
            say: _say,
            dark: widget.dark,
            setDark: widget.setDark,
            hubUrl: widget.hubUrl,
            hubToken: widget.hubToken,
            connect: widget.connect,
            pair: widget.pair,
            child: Builder(builder: (context) => _frame(context, phone, box)),
          );
          return PopScope(
            canPop: _screen == Screen.envoy && !_cmdOpen,
            onPopInvokedWithResult: (didPop, _) {
              if (didPop) return;
              if (_cmdOpen) return _setPalette(false);
              _go(switch (_screen) {
                Screen.sleeve => Screen.sleeves,
                Screen.node => Screen.nodes,
                Screen.hub => Screen.nodes,
                _ => Screen.envoy,
              });
            },
            child: app,
          );
        }),
      );
    });
  }

  Widget _frame(BuildContext context, bool phone, BoxConstraints box) {
    final p = pal(context);
    final app = AppScope.of(context);
    void nav(Screen s) {
      if (phone) _scaffold.currentState?.closeDrawer();
      _go(s);
    }
    void openSleeve(String id) {
      if (phone) _scaffold.currentState?.closeDrawer();
      _openSleeve(id);
    }
    final side = _Sidebar(
      current: _home(_screen),
      selectedSleeve: _currentSleeve,
      onNav: nav,
      onSleeve: openSleeve,
      onCommand: () {
        if (phone) _scaffold.currentState?.closeDrawer();
        _setPalette(true);
      },
      hubLabel: widget.hub.label,
      connected: widget.hub.connected,
    );
    final update = _update();
    final body = phone
        ? Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              _PhoneBar(
                title: _title(),
                back: _screen == Screen.sleeve || _screen == Screen.node || _screen == Screen.hub,
                onMenu: () => _scaffold.currentState?.openDrawer(),
                onBack: () => _go(_screen == Screen.sleeve ? Screen.sleeves : Screen.nodes),
                onCommand: () => _setPalette(true),
              ),
              ?update,
              Expanded(child: _main(context, phone, box.maxWidth)),
            ],
          )
        : Row(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              SizedBox(width: kSidebar, child: side),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    ?update,
                    Expanded(child: _main(context, phone, box.maxWidth - kSidebar)),
                  ],
                ),
              ),
            ],
          );
    return Scaffold(
      key: _scaffold,
      backgroundColor: p.bg,
      drawer: phone ? Drawer(child: SafeArea(child: side)) : null,
      body: DefaultTextStyle(
        style: sansStyle(p),
        child: SafeArea(
          child: Stack(
            children: [
              Positioned.fill(child: body),
              if (_cmdOpen)
                Positioned.fill(
                  child: _Palette(
                    controller: _cmd,
                    focus: _cmdFocus,
                    phone: phone,
                    onClose: () => _setPalette(false),
                    onRun: (v) {
                      _run(app, v);
                      _setPalette(false);
                    },
                  ),
                ),
              if (_msg != null)
                Positioned(
                  right: phone ? 12 : 20,
                  left: phone ? 12 : null,
                  bottom: phone ? 12 : 20,
                  child: _Toast(
                    text: _msg!,
                    level: _msgLevel,
                    onClose: () => setState(() => _msg = null),
                  ),
                ),
            ],
          ),
        ),
      ),
    );
  }

  String _title() => switch (_screen) {
    Screen.envoy => 'Envoy',
    Screen.sleeve => _sleeveId == null ? 'Sleeve' : (_state.sleeve(_sleeveId!)?.name ?? 'Sleeve'),
    Screen.sleeves => 'Sleeves',
    Screen.approvals => 'Approvals',
    Screen.nodes => 'Nodes',
    Screen.node => _nodeId == null ? 'Node' : (_state.node(_nodeId!)?.name ?? 'Node'),
    Screen.hub => 'hub',
    Screen.quota => 'Quota',
    Screen.events => 'Mail and events',
    Screen.settings => 'Settings',
  };

  Widget _main(BuildContext context, bool phone, double width) {
    final p = pal(context);
    switch (_screen) {
      case Screen.envoy:
        final env = _state.envoy;
        if (env == null) {
          return const Pane(title: 'Envoy',
              child: EmptyState('No Envoy sleeve on this hub', glyph: Glyph.envoy));
        }
        final chat = SleeveView(key: const ValueKey('envoy'), sleeveId: env.id, envoy: true);
        if (phone || width < _sidePanelFrom) return chat;
        return Row(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Expanded(child: chat),
            Container(width: 1, color: p.line),
            const SizedBox(width: 340, child: _EnvoySide()),
          ],
        );
      case Screen.sleeve:
        final id = _sleeveId;
        if (id == null) return const Pane(title: 'Sleeve', child: EmptyState('No sleeve selected'));
        return SleeveView(key: ValueKey(id), sleeveId: id);
      case Screen.sleeves:
        return Pane(
          title: 'Sleeves',
          status: T(treeStatusWord(_state), color: p.dim),
          padded: phone,
          child: phone ? SleeveTree(selected: _sleeveId) : const SleevesTable(),
        );
      case Screen.approvals:
        final n = _state.pending.length;
        return Pane(
          title: 'Approvals',
          status: n > 0 ? Pill('$n pending', color: p.warn, filled: true) : T('Nothing pending', color: p.dim),
          child: const ApprovalsScreen(),
        );
      case Screen.nodes:
        final online = _state.nodes.where((n) => n.online).length;
        return Pane(
          title: 'Nodes',
          status: T('$online online, ${_state.nodes.length - online} offline', color: p.dim),
          padded: phone,
          child: const NodesScreen(),
        );
      case Screen.node:
        final n = _nodeId == null ? null : _state.node(_nodeId!);
        return Pane(
          title: n?.name ?? 'Node',
          back: () => _go(Screen.nodes),
          status: n == null
              ? null
              : Pill(n.online ? 'Online' : 'Offline', color: n.online ? p.ok : p.alarm, dot: true),
          child: NodeDetail(nodeId: _nodeId ?? ''),
        );
      case Screen.hub:
        return Pane(
          title: 'hub',
          back: () => _go(Screen.nodes),
          status: Pill(widget.hub.connected ? 'Online' : 'Offline',
              color: widget.hub.connected ? p.ok : p.alarm, dot: true),
          child: const NodeDetail(),
        );
      case Screen.quota:
        final stale = _state.quotas.where((q) => q.reading != QuotaReading.ok).length;
        return Pane(
          title: 'Quota',
          status: stale == 0
              ? T('All readings fresh', color: p.dim)
              : Pill('$stale stale', color: p.warn, dot: true),
          child: const QuotaScreen(),
        );
      case Screen.events:
        final unread = _state.unreadEvents;
        return Pane(
          title: 'Mail and events',
          status: T('${_state.events.length} total${unread > 0 ? ', $unread unread' : ''}', color: p.dim),
          child: const EventsScreen(),
        );
      case Screen.settings:
        return const Pane(title: 'Settings', child: SettingsScreen());
    }
  }
}

/// A screen with a header row: title, status, optional back button and actions.
class Pane extends StatelessWidget {
  const Pane({
    super.key,
    required this.title,
    required this.child,
    this.status,
    this.back,
    this.actions = const [],
    this.padded = true,
  });

  final String title;
  final Widget child;
  final Widget? status;
  final VoidCallback? back;
  final List<Widget> actions;

  /// Content inset from the edges; tables run edge to edge.
  final bool padded;

  @override
  Widget build(BuildContext context) {
    final p = pal(context);
    final phone = Look.of(context).phone;
    final head = phone
        ? (status == null && actions.isEmpty
              ? null
              : Padding(
                  padding: const EdgeInsets.fromLTRB(16, 10, 16, 6),
                  child: Row(children: [?status, const Spacer(), ...actions]),
                ))
        : Container(
            height: 56,
            padding: const EdgeInsets.symmetric(horizontal: 24),
            decoration: BoxDecoration(border: Border(bottom: BorderSide(color: p.line))),
            child: Row(children: [
              if (back != null) ...[
                IconBtn(Glyph.back, tooltip: 'Back', onTap: back),
                const SizedBox(width: 8),
              ],
              Expanded(
                child: Row(children: [
                  Flexible(child: T(title, size: kTitle, weight: FontWeight.w600)),
                  if (status != null) ...[const SizedBox(width: 14), Flexible(child: status!)],
                ]),
              ),
              ...actions,
            ]),
          );
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        ?head,
        Expanded(
          child: padded
              ? Padding(
                  padding: phone
                      ? const EdgeInsets.fromLTRB(16, 4, 16, 0)
                      : const EdgeInsets.fromLTRB(24, 0, 24, 0),
                  child: child,
                )
              : child,
        ),
      ],
    );
  }
}

class _Sidebar extends StatelessWidget {
  const _Sidebar({
    required this.current,
    required this.selectedSleeve,
    required this.onNav,
    required this.onSleeve,
    required this.onCommand,
    required this.hubLabel,
    required this.connected,
  });

  final Screen current;
  final String? selectedSleeve;
  final void Function(Screen) onNav;
  final void Function(String) onSleeve;
  final VoidCallback onCommand;
  final String hubLabel;
  final bool connected;

  @override
  Widget build(BuildContext context) {
    final app = AppScope.of(context);
    final p = pal(context);
    final s = app.state;
    final pending = s.pending.length;
    final unread = s.unreadEvents;
    final offline = s.nodes.where((n) => !n.online).length;
    return Container(
      decoration: BoxDecoration(
        color: p.side,
        border: Border(right: BorderSide(color: p.line)),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 14, 12, 10),
            child: Row(children: [
              ClipRRect(
                borderRadius: BorderRadius.circular(5),
                child: Image.asset('assets/logo.png', width: 22, height: 22,
                    filterQuality: FilterQuality.medium),
              ),
              const SizedBox(width: 10),
              const T('Protectorate', weight: FontWeight.w600, size: kBody),
            ]),
          ),
          Padding(
            padding: const EdgeInsets.fromLTRB(10, 0, 10, 10),
            child: Hover(
              onTap: onCommand,
              tooltip: 'Commands (Esc or Ctrl+K)',
              builder: (c, hot, down, focus) => Container(
                height: 30,
                padding: const EdgeInsets.symmetric(horizontal: 9),
                decoration: BoxDecoration(
                  color: hot ? p.raised : p.surface,
                  borderRadius: BorderRadius.circular(kRadius),
                  border: Border.all(color: focus ? p.accent : p.line),
                ),
                child: Row(children: [
                  Icon16(Glyph.search, color: p.faint, size: 14),
                  const SizedBox(width: 8),
                  Expanded(child: T('Run a command', color: p.faint)),
                  if (!Look.of(context).phone) const Kbd('Ctrl K'),
                ]),
              ),
            ),
          ),
          for (final (screen, label, glyph, key, _) in _nav)
            _NavItem(
              label: label,
              glyph: glyph,
              keyHint: key,
              active: screen == current,
              onTap: () => onNav(screen),
              badge: switch (screen) {
                Screen.approvals when pending > 0 => Pill('$pending', color: p.warn, filled: true),
                Screen.events when unread > 0 => Pill('$unread', color: p.dim),
                Screen.nodes when offline > 0 => Pill('$offline down', color: p.alarm),
                _ => null,
              },
            ),
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 18, 12, 6),
            child: Row(children: [
              Expanded(child: T('Workspaces', size: kSmall, weight: FontWeight.w600, color: p.faint)),
              T(treeStatusWord(s), size: 11, color: p.faint),
            ]),
          ),
          Expanded(
            child: SleeveTree(selected: selectedSleeve, onOpen: onSleeve, compact: true),
          ),
          _HubFooter(label: hubLabel, connected: connected, onTap: () => onNav(Screen.settings)),
        ],
      ),
    );
  }
}

class _NavItem extends StatelessWidget {
  const _NavItem({
    required this.label,
    required this.glyph,
    required this.keyHint,
    required this.active,
    required this.onTap,
    this.badge,
  });

  final String label;
  final Glyph glyph;
  final String keyHint;
  final bool active;
  final VoidCallback onTap;
  final Widget? badge;

  @override
  Widget build(BuildContext context) {
    final p = pal(context);
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 1),
      child: Hover(
        onTap: onTap,
        tooltip: '$label ($keyHint)',
        builder: (c, hot, down, focus) => Container(
          height: 30,
          padding: const EdgeInsets.symmetric(horizontal: 8),
          decoration: BoxDecoration(
            color: active ? p.tint(p.accent, 0.12) : (down ? p.press : hot ? p.hover : null),
            borderRadius: BorderRadius.circular(kRadius),
            border: focus ? Border.all(color: p.accent) : null,
          ),
          child: Row(children: [
            Icon16(glyph, color: active ? p.accent : (hot ? p.fg : p.dim), size: 15),
            const SizedBox(width: 10),
            Expanded(
              child: T(label,
                  color: active ? p.fg : (hot ? p.fg : p.dim),
                  weight: active ? FontWeight.w500 : FontWeight.w400),
            ),
            if (badge != null)
              badge!
            else if (hot)
              T(keyHint, mono: true, size: 10.5, color: p.faint),
          ]),
        ),
      ),
    );
  }
}

class _HubFooter extends StatelessWidget {
  const _HubFooter({required this.label, required this.connected, required this.onTap});

  final String label;
  final bool connected;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final app = AppScope.of(context);
    final p = pal(context);
    final s = app.state;
    return Container(
      decoration: BoxDecoration(border: Border(top: BorderSide(color: p.line))),
      padding: const EdgeInsets.fromLTRB(8, 8, 8, 10),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Hover(
            onTap: onTap,
            tooltip: '$label\nhub ${s.hub.version}, app $kVersion',
            builder: (c, hot, down, focus) => Container(
              padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 6),
              decoration: BoxDecoration(
                color: hot ? p.hover : null,
                borderRadius: BorderRadius.circular(kRadius),
              ),
              child: Tick(builder: (c, now) => Row(children: [
                Dot(connected ? p.ok : p.alarm),
                const SizedBox(width: 8),
                Expanded(child: T(s.hub.name, weight: FontWeight.w500)),
                T('up ${span(now.difference(s.hub.startedAt))}', color: p.faint, size: kSmall),
              ])),
            ),
          ),
          if (s.quotas.isNotEmpty)
            Padding(
              padding: const EdgeInsets.fromLTRB(8, 4, 8, 0),
              child: Row(children: [
                for (final q in s.quotas) ...[
                  _QuotaChip(q: q),
                  const SizedBox(width: 12),
                ],
              ]),
            ),
        ],
      ),
    );
  }
}

class _QuotaChip extends StatelessWidget {
  const _QuotaChip({required this.q});

  final Quota q;

  @override
  Widget build(BuildContext context) {
    final p = pal(context);
    final w = q.tightest;
    final v = w?.usedPct;
    final color = v == null ? p.faint : v >= 95 ? p.alarm : v >= 80 ? p.warn : p.dim;
    return Tooltip(
      message: '${q.subscription.wire}${w == null ? ': no reading' : ' ${w.label}: ${v!.round()}% used'}',
      child: Line([
        sp(context, subscriptionLetter(q.subscription), color: p.faint, size: kSmall, weight: FontWeight.w600),
        sp(context, ' ${v == null ? '-' : '${v.round()}%'}', color: color, size: kSmall),
      ]),
    );
  }
}

class _PhoneBar extends StatelessWidget {
  const _PhoneBar({
    required this.title,
    required this.back,
    required this.onMenu,
    required this.onBack,
    required this.onCommand,
  });

  final String title;
  final bool back;
  final VoidCallback onMenu;
  final VoidCallback onBack;
  final VoidCallback onCommand;

  @override
  Widget build(BuildContext context) {
    final app = AppScope.of(context);
    final p = pal(context);
    final pending = app.state.pending.length;
    return Container(
      height: 52,
      padding: const EdgeInsets.symmetric(horizontal: 8),
      decoration: BoxDecoration(
        color: p.side,
        border: Border(bottom: BorderSide(color: p.line)),
      ),
      child: Row(children: [
        back
            ? IconBtn(Glyph.back, tooltip: 'Back', onTap: onBack, size: 36)
            : IconBtn(Glyph.menu, tooltip: 'Menu', onTap: onMenu, size: 36),
        const SizedBox(width: 6),
        if (!back) ...[
          ClipRRect(
            borderRadius: BorderRadius.circular(4),
            child: Image.asset('assets/logo.png', width: 20, height: 20,
                filterQuality: FilterQuality.medium),
          ),
          const SizedBox(width: 10),
        ],
        Expanded(child: T(title, size: kTitle, weight: FontWeight.w600)),
        if (pending > 0)
          Hover(
            onTap: () => app.go(Screen.approvals),
            builder: (c, hot, down, focus) =>
                Pill('$pending need you', color: p.warn, filled: true),
          ),
        const SizedBox(width: 4),
        IconBtn(Glyph.search, tooltip: 'Commands', onTap: onCommand, size: 36),
      ]),
    );
  }
}

/// The command palette: the same commands as the CLI, opened with Esc or Ctrl+K.
class _Palette extends StatelessWidget {
  const _Palette({
    required this.controller,
    required this.focus,
    required this.phone,
    required this.onClose,
    required this.onRun,
  });

  final TextEditingController controller;
  final FocusNode focus;
  final bool phone;
  final VoidCallback onClose;
  final void Function(String) onRun;

  @override
  Widget build(BuildContext context) {
    final p = pal(context);
    final q = controller.text.trim().toLowerCase();
    final head = q.split(' ').first;
    final hits = [
      for (final c in commandHelp)
        if (q.isEmpty || c.$1.toLowerCase().startsWith(head) || (q.length > 1 && c.$2.toLowerCase().contains(q))) c,
    ];
    return GestureDetector(
      behavior: HitTestBehavior.opaque,
      onTap: onClose,
      child: ColoredBox(
        color: Colors.black.withValues(alpha: p.dark ? 0.45 : 0.18),
        child: Align(
          alignment: phone ? Alignment.topCenter : const Alignment(0, -0.55),
          child: GestureDetector(
            onTap: () {},
            child: Container(
              width: 580,
              margin: EdgeInsets.all(phone ? 10 : 24),
              constraints: const BoxConstraints(maxHeight: 460),
              decoration: BoxDecoration(
                color: p.surface,
                borderRadius: BorderRadius.circular(10),
                border: Border.all(color: p.edge),
                boxShadow: const [
                  BoxShadow(color: Color(0x66000000), blurRadius: 32, offset: Offset(0, 12)),
                ],
              ),
              child: Column(
                mainAxisSize: MainAxisSize.min,
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  Padding(
                    padding: const EdgeInsets.fromLTRB(14, 12, 14, 12),
                    child: Field(
                      controller: controller,
                      focusNode: focus,
                      autofocus: true,
                      bare: true,
                      hint: 'Type a command, or > to message Envoy',
                      leading: Icon16(Glyph.search, color: p.faint),
                      trailing: const Kbd('Esc'),
                      onSubmit: onRun,
                    ),
                  ),
                  Container(height: 1, color: p.line),
                  Flexible(
                    child: ListView(
                      shrinkWrap: true,
                      padding: const EdgeInsets.all(6),
                      children: [
                        if (q.startsWith('>'))
                          _PaletteRow(k: '>', v: 'Send "${controller.text.trim().substring(1).trim()}" to Envoy',
                              onTap: () => onRun(controller.text))
                        else if (hits.isEmpty)
                          Padding(
                            padding: const EdgeInsets.all(10),
                            child: T('Press Enter to run "${controller.text.trim()}"', color: p.faint),
                          ),
                        if (!q.startsWith('>'))
                          for (final (k, v) in hits)
                            _PaletteRow(k: k, v: v, onTap: () {
                              final word = k.split(' ').first;
                              final takesArg = k.contains(' ');
                              if (takesArg) {
                                controller.text = '$word ';
                                controller.selection = TextSelection.collapsed(offset: controller.text.length);
                                focus.requestFocus();
                              } else {
                                onRun(word);
                              }
                            }),
                      ],
                    ),
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}

class _PaletteRow extends StatelessWidget {
  const _PaletteRow({required this.k, required this.v, required this.onTap});

  final String k;
  final String v;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final p = pal(context);
    return Hover(
      onTap: onTap,
      focusable: false,
      builder: (c, hot, down, focus) => Container(
        height: 34,
        padding: const EdgeInsets.symmetric(horizontal: 10),
        decoration: BoxDecoration(
          color: hot ? p.hover : null,
          borderRadius: BorderRadius.circular(kRadius),
        ),
        child: Row(children: [
          SizedBox(width: 130, child: T(k, mono: true, color: p.accent, size: 12)),
          Expanded(child: T(v, color: hot ? p.fg : p.dim)),
        ]),
      ),
    );
  }
}

/// A bar above the screen while the hub runs another release than this app. One click downloads
/// this platform's build from the hub and starts its installer; on web it reloads the page,
/// which the hub serves from its own release.
class _UpdateBanner extends StatefulWidget {
  const _UpdateBanner({super.key, required this.version, required this.asset, required this.onDismiss});

  final String version;
  final String? asset;
  final VoidCallback onDismiss;

  @override
  State<_UpdateBanner> createState() => _UpdateBannerState();
}

class _UpdateBannerState extends State<_UpdateBanner> {
  bool _busy = false;
  int _got = 0;
  int _total = 0;
  String? _note;
  String? _error;

  Future<void> _install(AppScope app) async {
    setState(() {
      _busy = true;
      _got = _total = 0;
      _note = _error = null;
    });
    try {
      if (app.hubUrl.isEmpty) throw StateError('no hub URL to download from');
      final url = HubApi(Uri.parse(app.hubUrl)).clientUrl(widget.asset!);
      final path = await downloadClient(url, app.hubToken.isEmpty ? null : app.hubToken, (got, total) {
        final step = total > 0 && (got * 100 ~/ total != _got * 100 ~/ total || total != _total);
        if (mounted && step) {
          setState(() {
            _got = got;
            _total = total;
          });
        }
      });
      if (!mounted) return;
      setState(() => _note = 'Installing ${widget.version}');
      final next = await installClient(path);
      if (mounted) {
        setState(() {
          _busy = false;
          _note = next;
        });
      }
    } catch (e) {
      if (mounted) {
        setState(() {
          _busy = false;
          _note = null;
          _error = switch (e) {
            StateError(:final message) => message,
            PlatformException(:final message, :final code) => message ?? code,
            _ => '$e',
          };
        });
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final app = AppScope.of(context);
    final p = pal(context);
    final phone = Look.of(context).phone;
    final error = _error;
    final color = error == null ? p.accent : p.alarm;
    final String text;
    final Widget action;
    if (reloadsFromHub) {
      text = 'The hub was updated to ${widget.version}; this page runs $kVersion';
      action = const Btn('Reload', kind: BtnKind.primary, dense: true, onTap: reloadPage);
    } else if (_busy && _note == null) {
      text = _total == 0
          ? 'Downloading ${widget.version}'
          : 'Downloading ${widget.version}, ${bytes(_got)} of ${bytes(_total)}';
      action = Meter(_total == 0 ? null : _got / _total, width: phone ? 64 : 140);
    } else {
      text = error != null
          ? 'Update to ${widget.version} failed: $error'
          : _note ?? 'The hub runs ${widget.version}; this app is $kVersion';
      action = Btn(
        error != null ? 'Retry' : 'Install update',
        kind: BtnKind.primary,
        dense: true,
        glyph: Glyph.download,
        onTap: _busy ? null : () => unawaited(_install(app)),
      );
    }
    return Container(
      padding: EdgeInsets.fromLTRB(phone ? 16 : 24, 7, phone ? 8 : 16, 7),
      decoration: BoxDecoration(
        color: Color.alphaBlend(p.tint(color, 0.08), p.bg),
        border: Border(bottom: BorderSide(color: color.withValues(alpha: 0.35))),
      ),
      child: Row(children: [
        Icon16(error == null ? Glyph.download : Glyph.alert, color: color, size: 14),
        const SizedBox(width: 10),
        Expanded(child: T(text, maxLines: 2)),
        const SizedBox(width: 12),
        action,
        const SizedBox(width: 6),
        IconBtn(Glyph.close, tooltip: 'Dismiss until the hub changes version', size: 26,
            onTap: _busy ? null : widget.onDismiss),
      ]),
    );
  }
}

class _Toast extends StatelessWidget {
  const _Toast({required this.text, required this.level, required this.onClose});

  final String text;
  final Level level;
  final VoidCallback onClose;

  @override
  Widget build(BuildContext context) {
    final p = pal(context);
    final (color, glyph) = switch (level) {
      Level.info || Level.unknown => (p.accent, Glyph.check),
      Level.warn => (p.warn, Glyph.alert),
      Level.alarm => (p.alarm, Glyph.alert),
    };
    return Container(
      constraints: const BoxConstraints(maxWidth: 520),
      padding: const EdgeInsets.fromLTRB(12, 10, 6, 10),
      decoration: BoxDecoration(
        color: p.raised,
        borderRadius: BorderRadius.circular(kCardRadius),
        border: Border.all(color: color.withValues(alpha: 0.45)),
        boxShadow: const [BoxShadow(color: Color(0x55000000), blurRadius: 20, offset: Offset(0, 6))],
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Padding(padding: const EdgeInsets.only(top: 1), child: Icon16(glyph, color: color)),
          const SizedBox(width: 10),
          Flexible(child: T(text, maxLines: 4)),
          const SizedBox(width: 6),
          IconBtn(Glyph.close, tooltip: 'Dismiss', onTap: onClose, size: 22),
        ],
      ),
    );
  }
}

/// Beside Envoy's chat on a wide window: recent events, nodes and quota.
class _EnvoySide extends StatelessWidget {
  const _EnvoySide();

  @override
  Widget build(BuildContext context) {
    final app = AppScope.of(context);
    final p = pal(context);
    final unread = app.state.unreadEvents;
    final online = app.state.nodes.where((n) => n.online).length;
    Widget head(String t, String status, VoidCallback go, {Color? color}) => Padding(
      padding: const EdgeInsets.fromLTRB(16, 14, 10, 6),
      child: Row(children: [
        Expanded(child: T(t, weight: FontWeight.w600, size: kSmall, color: p.dim)),
        T(status, size: kSmall, color: color ?? p.faint),
        const SizedBox(width: 4),
        IconBtn(Glyph.chevronRight, tooltip: 'Open $t', onTap: go, size: 22),
      ]),
    );
    return ColoredBox(
      color: p.bg,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          head('Mail and events', unread > 0 ? '$unread unread' : 'all read', () => app.go(Screen.events),
              color: unread > 0 ? p.accent : null),
          const Expanded(child: EventsScreen(limit: 20, markRead: false)),
          Container(height: 1, color: p.line),
          head('Nodes', '$online of ${app.state.nodes.length} online', () => app.go(Screen.nodes)),
          const Padding(padding: EdgeInsets.symmetric(horizontal: 8), child: NodesMini()),
          const SizedBox(height: 6),
          Container(height: 1, color: p.line),
          head('Quota', '', () => app.go(Screen.quota)),
          const Padding(padding: EdgeInsets.fromLTRB(16, 0, 16, 14), child: QuotaMini()),
        ],
      ),
    );
  }
}
