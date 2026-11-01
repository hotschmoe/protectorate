import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'chat.dart';
import 'hub.dart';
import 'api/api.g.dart';
import 'derived.dart';
import 'scope.dart';
import 'screens/approvals.dart';
import 'screens/events.dart';
import 'screens/nodes.dart';
import 'screens/quota.dart';
import 'screens/settings.dart';
import 'screens/sleeves.dart';
import 'theme.dart';
import 'widgets.dart';

const _keys = <(String, Screen, String, LogicalKeyboardKey)>[
  ('F1', Screen.envoy, 'ENVOY', LogicalKeyboardKey.f1),
  ('F2', Screen.sleeves, 'SLEEVES', LogicalKeyboardKey.f2),
  ('F3', Screen.approvals, 'APPROVALS', LogicalKeyboardKey.f3),
  ('F4', Screen.nodes, 'NODES', LogicalKeyboardKey.f4),
  ('F5', Screen.quota, 'QUOTA', LogicalKeyboardKey.f5),
  ('F6', Screen.events, 'EVENTS', LogicalKeyboardKey.f6),
  ('F12', Screen.settings, 'SETTINGS', LogicalKeyboardKey.f12),
];

/// The F-key a screen belongs to, for the legend highlight.
Screen _home(Screen s) => switch (s) {
  Screen.sleeve => Screen.sleeves,
  Screen.node => Screen.nodes,
  _ => s,
};

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
  final _cmd = TextEditingController();
  final _cmdFocus = FocusNode();
  bool _cmdOpen = false;
  String? _msg;
  Level _msgLevel = Level.info;
  Timer? _msgTimer;

  @override
  void initState() {
    super.initState();
    _sub = widget.hub.states.listen((s) => setState(() => _state = s));
    HardwareKeyboard.instance.addHandler(_onKey);
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
    for (final (_, screen, _, key) in _keys) {
      if (e.logicalKey == key) {
        _go(screen);
        return true;
      }
    }
    if (e.logicalKey == LogicalKeyboardKey.escape) {
      setState(() => _cmdOpen = true);
      _cmdFocus.requestFocus();
      return true;
    }
    return false;
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
      hits.isEmpty ? 'NO $what MATCHES "$q"' : '"$q" IS AMBIGUOUS: ${hits.map(name).join(', ')}',
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
        final s = _match(_state.sleeves, (x) => x.name, arg, 'SLEEVE');
        if (s != null) _openSleeve(s.id);
      case 'n' || 'node' || 'nodes':
        if (arg.isEmpty) return _go(Screen.nodes);
        final n = _match(_state.nodes, (x) => x.name, arg, 'NODE');
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
          'light' || 'printer' => false,
          _ => !widget.dark,
        });
      case '/approve' || '/deny':
        final pending = _state.pending;
        Approval? a;
        if (arg.isNotEmpty) {
          a = _match(pending, (x) => x.id, arg, 'PENDING APPROVAL');
        } else {
          final here = pending.where((x) => x.sleeveId == _currentSleeve).toList();
          if (here.length == 1) {
            a = here.first;
          } else if (pending.length == 1) {
            a = pending.first;
          } else {
            _say('${pending.length} PENDING; GIVE AN ID (F3 LISTS THEM)', level: Level.warn);
          }
        }
        if (a != null) {
          final id = a.id;
          final ok = head == '/approve';
          unawaited(app.act(() => widget.hub.decide(id, ok ? Decision.allowOnce : Decision.deny),
              done: '${ok ? 'APPROVED' : 'DENIED'} $id'));
        }
      case '/interrupt' || '/stop' || '/resleeve':
        final id = _currentSleeve;
        if (id == null) return _say('OPEN A SLEEVE FIRST', level: Level.warn);
        if (head == '/interrupt') {
          unawaited(app.act(() => widget.hub.interrupt(id)));
        } else if (head == '/stop') {
          unawaited(app.act(() => widget.hub.stop(id)));
        } else {
          final h = Harness.fromJson(arg);
          if (h == Harness.unknown) {
            return _say('RESLEEVE TO WHAT? claude codex grok pi_local pi_vendor', level: Level.warn);
          }
          unawaited(app.act(() => widget.hub.resleeve(id, h), done: 'RESLEEVING TO ${h.wire}'));
        }
      default:
        _say('UNKNOWN COMMAND: $head   (? FOR HELP)', level: Level.alarm);
    }
  }

  @override
  Widget build(BuildContext context) {
    final p = widget.dark ? Palette.amber : Palette.printer;
    return LayoutBuilder(builder: (context, box) {
      final phone = box.maxWidth < kPhoneWidth;
      return Term(
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
            canPop: _screen == Screen.envoy,
            onPopInvokedWithResult: (didPop, _) {
              if (!didPop) {
                _go(switch (_screen) {
                  Screen.sleeve => Screen.sleeves,
                  Screen.node => Screen.nodes,
                  _ => Screen.envoy,
                });
              }
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
    final tall = box.maxHeight >= 640;
    final body = phone
        ? Padding(padding: const EdgeInsets.fromLTRB(2, 2, 2, 0), child: _main(context, phone, tall))
        : Padding(
            padding: const EdgeInsets.fromLTRB(4, 4, 4, 0),
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                SizedBox(width: 46 * kCell, child: _left(context, tall)),
                const SizedBox(width: 4),
                Expanded(child: _main(context, phone, tall)),
              ],
            ),
          );
    return Material(
      color: p.bg,
      textStyle: baseStyle(p),
      child: SafeArea(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            _TopBar(phone: phone),
            Expanded(child: body),
            if (phone && _msg != null)
              Padding(
                padding: const EdgeInsets.symmetric(horizontal: kCell),
                child: T(_msg!, color: _msgColor(p), maxLines: 2),
              ),
            if (!phone || _cmdOpen) _commandLine(context, app, phone),
            phone ? _phoneKeys(context) : _legend(context),
          ],
        ),
      ),
    );
  }

  Color _msgColor(Palette p) => switch (_msgLevel) {
    Level.info || Level.unknown => p.lit,
    Level.warn => p.warn,
    Level.alarm => p.alarm,
  };

  Widget _left(BuildContext context, bool tall) {
    final p = pal(context);
    final online = _state.nodes.where((n) => n.online).length;
    final sel = _currentSleeve;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Expanded(
          child: Pane(
            legend: 'WORKSPACES',
            status: T(treeStatusWord(_state), color: p.dim),
            child: SleeveTree(selected: sel),
          ),
        ),
        if (tall) ...[
          Pane(
            legend: 'NODES',
            status: T('$online UP ${_state.nodes.length - online} DOWN', color: p.dim),
            child: const NodesMini(),
          ),
          const Pane(legend: 'QUOTA', child: QuotaMini()),
        ],
      ],
    );
  }

  Widget _main(BuildContext context, bool phone, bool tall) {
    final p = pal(context);
    Widget pane(String legend, Widget child, {String? status, Color? statusColor}) => Pane(
      legend: legend,
      focused: true,
      status: status == null ? null : T(status, color: statusColor ?? p.dim),
      child: child,
    );
    switch (_screen) {
      case Screen.envoy:
        final env = _state.envoy;
        if (env == null) return pane('ENVOY', T('(no Envoy sleeve on this hub)', color: p.dim));
        final chat = SleeveView(key: const ValueKey('envoy'), sleeveId: env.id, envoy: true);
        if (phone || !tall) return chat;
        final unread = _state.unreadEvents;
        return Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Expanded(child: chat),
            SizedBox(
              height: kRow * 8,
              child: Pane(
                legend: 'MAIL / EVENTS',
                status: T('$unread UNREAD', color: unread > 0 ? p.lit : p.dim),
                child: const EventsScreen(limit: 20, markRead: false),
              ),
            ),
          ],
        );
      case Screen.sleeve:
        final id = _sleeveId;
        if (id == null) return pane('SLEEVE', T('(none selected)', color: p.dim));
        return SleeveView(key: ValueKey(id), sleeveId: id);
      case Screen.sleeves:
        return pane(
          'SLEEVES',
          phone ? SleeveTree(selected: _sleeveId) : const SleevesTable(),
          status: treeStatusWord(_state),
        );
      case Screen.approvals:
        final n = _state.pending.length;
        return pane('APPROVALS', const ApprovalsScreen(),
            status: '$n PENDING', statusColor: n > 0 ? p.warn : p.dim);
      case Screen.nodes:
        final online = _state.nodes.where((n) => n.online).length;
        return pane('NODES', const NodesScreen(),
            status: '$online ONLINE  ${_state.nodes.length - online} OFFLINE');
      case Screen.node:
        final n = _nodeId == null ? null : _state.node(_nodeId!);
        return pane('NODE ${n?.name ?? ''}', NodeDetail(nodeId: _nodeId ?? ''),
            status: n == null ? null : (n.online ? 'ONLINE' : 'OFFLINE'),
            statusColor: n == null ? null : (n.online ? p.ok : p.alarm));
      case Screen.quota:
        final stale = _state.quotas.where((q) => q.reading != QuotaReading.ok).length;
        return pane('QUOTA', const QuotaScreen(),
            status: stale == 0 ? 'ALL FRESH' : '$stale STALE',
            statusColor: stale == 0 ? p.dim : p.warn);
      case Screen.events:
        return pane('MAIL / EVENTS', const EventsScreen(), status: '${_state.events.length}');
      case Screen.settings:
        return pane('SETTINGS', const SettingsScreen());
    }
  }

  Widget _commandLine(BuildContext context, AppScope app, bool phone) {
    final p = pal(context);
    return Container(
      decoration: BoxDecoration(border: Border(top: BorderSide(color: p.rule))),
      padding: const EdgeInsets.symmetric(horizontal: kCell),
      child: Row(
        children: [
          Expanded(
            child: Field(
              controller: _cmd,
              focusNode: _cmdFocus,
              prompt: '===> ',
              promptColor: p.lit,
              hint: phone ? 'command (? help)' : 'command   s <sleeve>  n <node>  /approve  > text to Envoy  ? help',
              onSubmit: (v) {
                _run(app, v);
                _cmdFocus.requestFocus();
              },
            ),
          ),
          if (!phone && _msg != null)
            Flexible(child: T(_msg!, color: _msgColor(p))),
        ],
      ),
    );
  }

  Widget _legend(BuildContext context) {
    final p = pal(context);
    final cur = _home(_screen);
    final pending = _state.pending.length;
    return Container(
      color: p.bar,
      padding: const EdgeInsets.symmetric(horizontal: kCell),
      child: Row(
        children: [
          for (final (k, s, label, _) in _keys) ...[
            _FKey(
              k: k,
              label: s == Screen.approvals && pending > 0 ? '$label $pending' : label,
              active: s == cur,
              alert: s == Screen.approvals && pending > 0,
              onTap: () => _go(s),
            ),
            const SizedBox(width: kCell * 2),
          ],
          _FKey(k: 'ESC', label: 'CMD', active: false, onTap: () => _cmdFocus.requestFocus()),
          const Spacer(),
          T(widget.hub.label, color: p.warn),
        ],
      ),
    );
  }

  Widget _phoneKeys(BuildContext context) {
    final p = pal(context);
    final cur = _home(_screen);
    final pending = _state.pending.length;
    final cells = <Widget>[
      for (final (k, s, label, _) in _keys)
        _FKey(
          k: k,
          label: switch (s) {
            Screen.approvals => pending > 0 ? 'APPR $pending' : 'APPR',
            Screen.settings => 'SET',
            _ => label,
          },
          active: s == cur,
          alert: s == Screen.approvals && pending > 0,
          onTap: () => _go(s),
          expand: true,
        ),
      _FKey(k: '===>', label: 'CMD', active: _cmdOpen, expand: true, onTap: () {
        setState(() => _cmdOpen = !_cmdOpen);
        if (_cmdOpen) _cmdFocus.requestFocus();
      }),
    ];
    Widget row(List<Widget> xs) => Row(children: [
      for (var i = 0; i < xs.length; i++) ...[
        if (i > 0) Container(width: 1, height: 30, color: p.rule),
        Expanded(child: xs[i]),
      ],
    ]);
    return Container(
      decoration: BoxDecoration(color: p.bar, border: Border(top: BorderSide(color: p.rule))),
      child: Column(children: [
        row(cells.sublist(0, 4)),
        Container(height: 1, color: p.rule),
        row(cells.sublist(4)),
      ]),
    );
  }
}

class _FKey extends StatefulWidget {
  const _FKey({
    required this.k,
    required this.label,
    required this.active,
    required this.onTap,
    this.alert = false,
    this.expand = false,
  });

  final String k;
  final String label;
  final bool active;
  final bool alert;
  final bool expand;
  final VoidCallback onTap;

  @override
  State<_FKey> createState() => _FKeyState();
}

class _FKeyState extends State<_FKey> {
  bool _hot = false;

  @override
  Widget build(BuildContext context) {
    final p = pal(context);
    final rev = widget.active || _hot;
    final fill = widget.alert ? p.warn : p.lit;
    final text = Line([
      sp(context, widget.k, color: rev ? p.bg : p.dim, bg: rev ? fill : null),
      sp(context, ' ', bg: rev ? fill : null),
      sp(context, widget.label,
          color: rev ? p.bg : (widget.alert ? p.bg : p.lit),
          bg: rev ? fill : (widget.alert ? p.warn : null),
          bold: rev || widget.alert),
    ]);
    return MouseRegion(
      cursor: SystemMouseCursors.click,
      onEnter: (_) => setState(() => _hot = true),
      onExit: (_) => setState(() => _hot = false),
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTap: widget.onTap,
        child: widget.expand
            ? Container(
                height: 30,
                alignment: Alignment.center,
                color: rev ? fill : null,
                child: text,
              )
            : text,
      ),
    );
  }
}

class _TopBar extends StatelessWidget {
  const _TopBar({required this.phone});

  final bool phone;

  @override
  Widget build(BuildContext context) {
    final app = AppScope.of(context);
    final p = pal(context);
    final s = app.state;
    final pending = s.pending.length;
    return Container(
      color: p.bar,
      padding: const EdgeInsets.symmetric(horizontal: kCell),
      child: Tick(builder: (c, now) {
        final quota = <InlineSpan>[
          sp(c, 'quota', color: p.dim),
          for (final q in s.quotas) ...[
            sp(c, ' ${subscriptionLetter(q.subscription)} ', color: p.dim),
            () {
              final w = q.tightest;
              final v = w?.usedPct;
              return sp(c, v == null ? '-' : '${v.round()}%',
                  color: v == null ? p.dim : v >= 95 ? p.alarm : v >= 80 ? p.warn : p.fg);
            }(),
          ],
        ];
        return Row(children: [
          Line([
            sp(c, 'PROTECTORATE', color: p.lit, bold: true),
            sp(c, ' $kVersion', color: p.dim),
            if (!phone) ...[
              sp(c, '   hub ', color: p.dim),
              sp(c, s.hub.name, color: p.fg),
              sp(c, ' ${s.hub.version}', color: p.dim),
              sp(c, '   up ', color: p.dim),
              sp(c, span(now.difference(s.hub.startedAt)), color: p.fg),
            ],
          ]),
          const Spacer(),
          if (!phone) ...[Line(quota), const SizedBox(width: kCell * 3)],
          if (pending > 0)
            GestureDetector(
              onTap: () => app.go(Screen.approvals),
              child: T(' $pending NEED YOU ', color: p.bg, bg: p.warn, bold: true),
            ),
          T('  ${hhmmss(now)}', color: p.lit),
        ]);
      }),
    );
  }
}
