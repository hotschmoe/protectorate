import 'dart:async';
import 'dart:typed_data';

import 'package:flutter/material.dart';

import '../gen/protectorate/v1/desk.pb.dart';
import '../gen/protectorate/v1/events.pb.dart';
import '../gen/protectorate/v1/node.pb.dart';
import '../hubs/event_feed.dart';
import '../hubs/hub_client.dart';
import '../nodes/desk_view.dart';
import '../nodes/node_view.dart';
import '../theme/tokens.dart';
import '../vfd/lamp.dart';
import '../vfd/panel.dart';
import 'nodes_tab.dart';

const deskPaneBreakpoint = 1000.0;

// A Windows node's desktop, to look at: the screen, its windows and what has
// the foreground, whether agents may have it, and the policy that decides.
// There is no input from here; the owner uses the machine itself.
class DeskScreen extends StatefulWidget {
  const DeskScreen({
    super.key,
    required this.client,
    required this.feed,
    required this.name,
    this.interval = const Duration(seconds: 20),
    this.now,
  });

  final HubClient client;
  final EventFeed feed;
  final String name;
  final Duration interval;
  final DateTime Function()? now;

  @override
  State<DeskScreen> createState() => _DeskScreenState();
}

class _DeskScreenState extends State<DeskScreen> with WidgetsBindingObserver {
  NodeStatus? _status;
  Object? _statusError;
  Uint8List? _png;
  int _screenW = 0;
  int _screenH = 0;
  DateTime? _shotAt;
  String? _shotError;
  bool _shooting = false;
  List<DeskWindow> _windows = const [];
  String? _windowsError;
  ForegroundResponse? _fg;
  String? _fgError;
  int _maxWidth = 960;
  bool _resumed = true;
  Timer? _timer;
  StreamSubscription<Event>? _sub;

  DateTime _now() => (widget.now ?? DateTime.now)();

  DeskTarget get _target => DeskTarget(name: widget.name);

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    _sub = widget.feed.incoming.listen((e) {
      if ((nodeEventKinds.contains(e.kind) && e.subject == widget.name) ||
          e.kind.startsWith('lease.')) {
        _loadStatus();
      }
    });
    _timer = Timer.periodic(widget.interval, (_) {
      if (_visible) _capture();
    });
    _loadStatus();
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) _capture();
    });
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    _timer?.cancel();
    _sub?.cancel();
    super.dispose();
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    _resumed = state == AppLifecycleState.resumed;
  }

  bool get _visible =>
      mounted && _resumed && (ModalRoute.of(context)?.isCurrent ?? true);

  Future<void> _loadStatus() async {
    try {
      final s = await widget.client.node.getNode(
        GetNodeRequest(name: widget.name),
      );
      if (mounted) {
        setState(() {
          _status = s;
          _statusError = null;
        });
      }
    } on Object catch (e) {
      if (mounted) setState(() => _statusError = e);
    }
  }

  // Screen, windows and foreground together, so they describe one moment.
  Future<void> _capture() async {
    if (_shooting) return;
    setState(() => _shooting = true);
    await Future.wait([_shoot(), _readWindows(), _readForeground()]);
    if (mounted) setState(() => _shooting = false);
  }

  Future<void> _shoot() async {
    try {
      final r = await widget.client.desk.screenshot(
        ScreenshotRequest(target: _target, maxWidth: _maxWidth),
      );
      if (!mounted) return;
      setState(() {
        _png = Uint8List.fromList(r.png);
        _screenW = r.width;
        _screenH = r.height;
        _shotAt = _now();
        _shotError = null;
      });
    } on Object catch (e) {
      if (mounted) setState(() => _shotError = deskFailure(e, 'A screenshot'));
    }
  }

  Future<void> _readWindows() async {
    try {
      final r = await widget.client.desk.listWindows(
        ListWindowsRequest(target: _target),
      );
      if (mounted) {
        setState(() {
          _windows = r.windows;
          _windowsError = null;
        });
      }
    } on Object catch (e) {
      if (mounted) {
        setState(() => _windowsError = deskFailure(e, 'The window list'));
      }
    }
  }

  Future<void> _readForeground() async {
    try {
      final r = await widget.client.desk.foreground(
        ForegroundRequest(target: _target),
      );
      if (mounted) {
        setState(() {
          _fg = r;
          _fgError = null;
        });
      }
    } on Object catch (e) {
      if (mounted) setState(() => _fgError = deskFailure(e, 'The foreground'));
    }
  }

  Future<String?> _setPolicy(DeskPolicy? p) async {
    try {
      final r = await widget.client.node.setDeskPolicy(
        SetDeskPolicyRequest(name: widget.name, policy: p),
      );
      if (!mounted) return null;
      final s = _status;
      if (s != null) {
        setState(() {
          _status = s.deepCopy()
            ..desk = r.hasState() ? r.state : DeskState()
            ..deskHolder = r.holder
            ..deskInputs.clear()
            ..deskInputs.addAll(r.inputs);
        });
      }
      unawaited(_loadStatus());
      return null;
    } on Object catch (e) {
      return deskFailure(e, 'Changing the policy');
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final s = _status;
    return Scaffold(
      appBar: AppBar(
        title: Text('${widget.name} desk'),
        actions: [
          IconButton(
            key: const ValueKey('desk-refresh'),
            tooltip: 'Capture now',
            icon: const Icon(Icons.refresh),
            onPressed: _shooting
                ? null
                : () {
                    _capture();
                    _loadStatus();
                  },
          ),
        ],
      ),
      body: LayoutBuilder(
        builder: (context, box) {
          final wide = box.maxWidth >= deskPaneBreakpoint;
          final screenW = wide
              ? (box.maxWidth - 3 * Tokens.gapL) * 0.6
              : box.maxWidth;
          final want = screenshotWidth(
            screenW,
            MediaQuery.devicePixelRatioOf(context),
          );
          if (want != _maxWidth) {
            _maxWidth = want;
            if (_png != null) {
              WidgetsBinding.instance.addPostFrameCallback((_) {
                if (mounted) _shoot();
              });
            }
          }
          final screen = _ScreenPanel(
            png: _png,
            screenW: _screenW,
            screenH: _screenH,
            at: _shotAt,
            now: _now(),
            error: _shotError,
            busy: _shooting,
            interval: widget.interval,
          );
          final head = <Widget>[
            if (_statusError != null && s == null)
              NoteStrip('$_statusError', tone: Tone.alarm),
            if (s != null) _AvailabilityPanel(status: s, now: _now()),
            ForegroundPanel(foreground: _fg, error: _fgError),
          ];
          final tail = <Widget>[
            if (s != null) DeskInputs(inputs: s.deskInputs),
            if (s != null)
              PolicyPanel(
                key: ValueKey(
                  'policy-${s.desk.policy.window}-${s.desk.hasPolicy()}',
                ),
                policy: s.hasDesk() && s.desk.hasPolicy()
                    ? s.desk.policy
                    : null,
                onApply: _setPolicy,
              ),
          ];
          final windows = WindowList(
            windows: _windows,
            foreground: _fg,
            error: _windowsError,
          );
          Widget gapped(List<Widget> ws) => Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              for (final w in ws)
                Padding(
                  padding: const EdgeInsets.only(bottom: Tokens.gap),
                  child: w,
                ),
            ],
          );
          if (!wide) {
            return ListView(
              padding: const EdgeInsets.all(Tokens.gap),
              children: [
                gapped([screen, ...head, windows, ...tail]),
              ],
            );
          }
          return SingleChildScrollView(
            padding: const EdgeInsets.all(Tokens.gapL),
            child: Center(
              child: ConstrainedBox(
                constraints: const BoxConstraints(
                  maxWidth: Tokens.boardMaxWidth,
                ),
                child: Row(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Expanded(flex: 3, child: gapped([screen, windows])),
                    const SizedBox(width: Tokens.gapL),
                    Expanded(flex: 2, child: gapped([...head, ...tail])),
                  ],
                ),
              ),
            ),
          );
        },
      ),
      backgroundColor: t.p.chassis,
    );
  }
}

class _ScreenPanel extends StatelessWidget {
  const _ScreenPanel({
    required this.png,
    required this.screenW,
    required this.screenH,
    required this.at,
    required this.now,
    required this.error,
    required this.busy,
    required this.interval,
  });

  final Uint8List? png;
  final int screenW;
  final int screenH;
  final DateTime? at;
  final DateTime now;
  final String? error;
  final bool busy;
  final Duration interval;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final shot = png;
    final ratio = screenW > 0 && screenH > 0 ? screenW / screenH : 16 / 9;
    return Panel(
      legend: screenW > 0 ? 'SCREEN  ${screenW}x$screenH' : 'SCREEN',
      trailing: const Annunciator(
        key: ValueKey('view-only'),
        label: 'VIEW ONLY',
        tone: Tone.dim,
        lit: false,
      ),
      padding: const EdgeInsets.all(6),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          AspectRatio(
            aspectRatio: ratio,
            child: DecoratedBox(
              decoration: BoxDecoration(
                color: t.p.ghost,
                border: Border.all(color: t.p.bezel),
                borderRadius: BorderRadius.circular(Tokens.innerRadius),
              ),
              child: shot == null
                  ? Center(
                      child: Text(
                        busy ? 'CAPTURING' : 'NO IMAGE',
                        style: t.legend(),
                      ),
                    )
                  : ClipRRect(
                      borderRadius: BorderRadius.circular(Tokens.innerRadius),
                      child: Image.memory(
                        shot,
                        key: const ValueKey('desk-shot'),
                        fit: BoxFit.contain,
                        gaplessPlayback: true,
                        filterQuality: FilterQuality.medium,
                      ),
                    ),
            ),
          ),
          SizedBox(
            height: 3,
            child: busy ? const BusyBar(tone: Tone.info) : null,
          ),
          if (error != null)
            Padding(
              padding: const EdgeInsets.only(top: 6),
              child: NoteStrip(error!, tone: Tone.alarm),
            ),
          Padding(
            padding: const EdgeInsets.fromLTRB(4, 6, 4, 2),
            child: Text(
              [
                at == null
                    ? 'not captured yet'
                    : 'captured ${formatAge(now.difference(at!))}',
                'refreshes every ${interval.inSeconds}s while open',
                'agents drive it with desk commands under a lease; '
                    'use the machine itself',
              ].join('. '),
              style: t.small(),
            ),
          ),
        ],
      ),
    );
  }
}

class _AvailabilityPanel extends StatelessWidget {
  const _AvailabilityPanel({required this.status, required this.now});

  final NodeStatus status;
  final DateTime now;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final s = status.hasDesk() ? status.desk : null;
    final (label, tone) = deskStatus(s, status.deskHolder);
    return Panel(
      legend: 'AVAILABILITY',
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          LampLegend(
            key: const ValueKey('desk-availability'),
            label: label,
            tone: tone,
            big: true,
          ),
          const SizedBox(height: 8),
          if (s == null)
            Text(
              'The node has not reported its desktop. An older agent shows '
              'nothing here; update it.',
              style: t.small(),
            )
          else ...[
            if (s.detail.isNotEmpty) Text(s.detail, style: t.data()),
            Text(lastInputLabel(s, now), style: t.small()),
            Text(
              s.hasPolicy()
                  ? 'Policy: ${policyLine(s.policy)}'
                  : 'No policy: agents may use the desktop at any time.',
              style: t.small(),
            ),
          ],
        ],
      ),
    );
  }
}

// The owner's controls for when agents may have the desktop.
class PolicyPanel extends StatefulWidget {
  const PolicyPanel({super.key, required this.policy, required this.onApply});

  final DeskPolicy? policy;
  final Future<String?> Function(DeskPolicy? policy) onApply;

  @override
  State<PolicyPanel> createState() => _PolicyPanelState();
}

class _PolicyPanelState extends State<PolicyPanel> {
  late final _window = TextEditingController(
    text: widget.policy?.window ?? '19:00-07:00',
  );
  late final _idle = TextEditingController(
    text: widget.policy == null
        ? '15'
        : '${durationOf(widget.policy!.idle).inMinutes}',
  );
  late final _tz = TextEditingController(text: widget.policy?.timezone ?? '');
  late bool _lock = widget.policy?.lockAfter ?? false;
  String? _error;
  bool _busy = false;

  @override
  void dispose() {
    _window.dispose();
    _idle.dispose();
    _tz.dispose();
    super.dispose();
  }

  Future<void> _apply(DeskPolicy? p) async {
    setState(() {
      _busy = true;
      _error = null;
    });
    final err = await widget.onApply(p);
    if (mounted) {
      setState(() {
        _busy = false;
        _error = err;
      });
    }
  }

  void _set() {
    final (p, err) = formPolicy(
      window: _window.text,
      idleMinutes: _idle.text,
      timezone: _tz.text,
      lockAfter: _lock,
    );
    if (p == null) {
      setState(() => _error = err);
      return;
    }
    _apply(p);
  }

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    return Panel(
      legend: 'POLICY',
      trailing: widget.policy == null
          ? const LampLegend(label: 'NONE', tone: Tone.dim, on: false)
          : const LampLegend(label: 'SET', tone: Tone.info),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text(
            'Agents get the desktop only inside the window and after the user '
            'has been idle that long. Owner token only.',
            style: t.small(),
          ),
          const SizedBox(height: 10),
          Row(
            children: [
              Expanded(
                flex: 3,
                child: TextField(
                  key: const ValueKey('policy-window'),
                  controller: _window,
                  style: t.data(t.p.emph),
                  decoration: const InputDecoration(labelText: 'Window'),
                ),
              ),
              const SizedBox(width: 8),
              Expanded(
                flex: 2,
                child: TextField(
                  key: const ValueKey('policy-idle'),
                  controller: _idle,
                  keyboardType: TextInputType.number,
                  style: t.data(t.p.emph),
                  decoration: const InputDecoration(labelText: 'Idle min'),
                ),
              ),
            ],
          ),
          const SizedBox(height: 8),
          TextField(
            key: const ValueKey('policy-tz'),
            controller: _tz,
            style: t.data(t.p.emph),
            decoration: const InputDecoration(
              labelText: 'Time zone (blank: the node\'s local time)',
            ),
          ),
          const SizedBox(height: 8),
          Row(
            children: [
              Text('LOCK AFTER', style: t.legend()),
              const SizedBox(width: 10),
              ChoiceKeys<bool>(
                options: const [(false, 'OFF'), (true, 'ON')],
                selected: _lock,
                onSelected: _busy ? null : (v) => setState(() => _lock = v),
              ),
            ],
          ),
          if (_error != null) ...[
            const SizedBox(height: 8),
            NoteStrip(_error!, tone: Tone.alarm),
          ],
          const SizedBox(height: 8),
          Wrap(
            alignment: WrapAlignment.end,
            spacing: 8,
            runSpacing: 8,
            children: [
              if (widget.policy != null)
                OutlinedButton(
                  key: const ValueKey('policy-off'),
                  onPressed: _busy ? null : () => _apply(null),
                  child: const Text('POLICY OFF'),
                ),
              FilledButton(
                key: const ValueKey('policy-apply'),
                onPressed: _busy ? null : _set,
                child: Text(widget.policy == null ? 'SET POLICY' : 'UPDATE'),
              ),
            ],
          ),
        ],
      ),
    );
  }
}
