import 'dart:async';

import 'package:flutter/material.dart';

import '../api/api.g.dart';
import '../hub.dart';
import '../local.dart';
import '../scope.dart';
import '../theme.dart';
import '../widgets.dart';

class SettingsScreen extends StatefulWidget {
  const SettingsScreen({super.key});

  @override
  State<SettingsScreen> createState() => _SettingsScreenState();
}

class _SettingsScreenState extends State<SettingsScreen> {
  final _url = TextEditingController();
  final _token = TextEditingController();
  final _code = TextEditingController();
  List<Device>? _devices;
  String? _devicesFor;
  String? _devicesError;
  DoctorReport? _doctor;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    final app = AppScope.of(context);
    if (_url.text.isEmpty) _url.text = app.hubUrl;
    if (_token.text.isEmpty) _token.text = app.hubToken;
  }

  @override
  void dispose() {
    _url.dispose();
    _token.dispose();
    _code.dispose();
    super.dispose();
  }

  HubApi _api(AppScope app) => HubApi(Uri.parse(app.hubUrl), token: app.hubToken);

  /// The paired devices, loaded once per hub and token.
  void _loadDevices(AppScope app) {
    final key = '${app.hubUrl} ${app.hubToken}';
    if (app.hubUrl.isEmpty || app.hubToken.isEmpty || _devicesFor == key) return;
    _devicesFor = key;
    _devicesError = null;
    unawaited(_api(app).listDevices().then((l) {
      if (mounted) setState(() => _devices = l.devices);
    }, onError: (Object e) {
      if (mounted) {
        setState(() {
          _devices = null;
          _devicesError = '$e';
        });
      }
    }));
    unawaited(_api(app).getDoctor(const GetDoctorRequest()).then((d) {
      if (mounted) setState(() => _doctor = d);
    }, onError: (Object _) {
      if (mounted) setState(() => _doctor = null);
    }));
  }

  /// The hub machine's doctor report, read-only: one line per harness, then the problems.
  Widget _doctorPanel(DoctorReport d, Palette p) {
    final checked = d.checkedAt.millisecondsSinceEpoch == 0
        ? 'not yet'
        : '${d.checkedAt.toLocal().toIso8601String().substring(0, 10)} ${hhmm(d.checkedAt.toLocal())}';
    return Panel(
      title: 'Doctor',
      trailing: T('checked $checked', color: p.faint, size: kSmall),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          KV('System', Wrap(spacing: 8, runSpacing: 6, children: [
            Pill('${d.os}/${d.arch}', color: p.dim, mono: true),
            Pill(d.sandboxOk ? 'sandbox ok' : 'sandbox not ok', color: d.sandboxOk ? p.ok : p.alarm, dot: true),
            Pill(d.ghLoggedIn ? 'gh ${d.ghAccount}' : 'gh not logged in',
                color: d.ghLoggedIn ? p.ok : p.warn, dot: true),
          ])),
          for (final h in d.harnesses)
            KV(h.name, Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
              Row(children: [
                Pill(h.eligible ? 'ready' : 'not ready', color: h.eligible ? p.ok : p.warn, dot: true),
                const SizedBox(width: 8),
                Flexible(child: T(h.present ? '${h.version} (${h.install})' : 'missing',
                    mono: true, size: 12, color: p.dim)),
              ]),
              if (!h.eligible && h.reasons.isNotEmpty)
                Padding(padding: const EdgeInsets.only(top: 3),
                    child: T(h.reasons.join('; '), color: p.warn, size: kSmall, maxLines: 3)),
              if (h.updateNote.isNotEmpty)
                Padding(padding: const EdgeInsets.only(top: 3),
                    child: T('Last update: ${h.updateNote}', color: p.faint, size: kSmall, maxLines: 2)),
            ])),
          for (final x in d.problems) KV('Problem', T(x, color: p.warn, maxLines: 3)),
          const SizedBox(height: 4),
          Line([
            sp(context, 'Run ', color: p.faint, size: kSmall),
            sp(context, 'protectorate doctor --refresh', mono: true, color: p.dim, size: 11.5),
            sp(context, ' on the hub to check again.', color: p.faint, size: kSmall),
          ], maxLines: 2),
        ],
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final app = AppScope.of(context);
    final p = pal(context);
    _loadDevices(app);
    final kept = pageOrigin() != null ? 'kept in this browser' : 'kept for this session only';
    void save() {
      app.connect(_url.text, _token.text);
      app.say(_url.text.trim().isEmpty
          ? 'No hub URL: using the scripted fake hub'
          : 'Connecting to ${_url.text.trim()} ($kept)');
    }
    Future<void> pair() async {
      if (_url.text.trim() != app.hubUrl) app.connect(_url.text, '');
      await app.act(() async {
        final name = await app.pair(_code.text);
        _code.clear();
        app.say('Paired as $name ($kept)');
      });
    }
    Future<void> revoke(Device d) => app.act(() async {
      await _api(app).revokeDevice(DeviceRef(deviceId: d.id));
      _devicesFor = null;
      if (d.id == loadSetting('device_id')) app.connect(app.hubUrl, '');
      if (mounted) setState(() {});
    }, done: 'Revoked ${d.name}');
    final me = loadSetting('device_id');
    final connected = app.hub.connected;
    const gap = SizedBox(height: 16);
    return Align(
      alignment: Alignment.topLeft,
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 760),
        child: ListView(
          padding: const EdgeInsets.symmetric(vertical: 20),
          children: [
            Panel(
              title: 'Hub',
              trailing: Pill(connected ? 'Connected' : 'Offline', color: connected ? p.ok : p.alarm, dot: true),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  KV('Connected to', T(app.hub.label, mono: true, size: 12.5, maxLines: 2)),
                  KV('Hub', T('${app.state.hub.name}   ${app.state.hub.version}', mono: true, size: 12.5,
                      color: p.dim, maxLines: 2)),
                  KV('Hub URL', Field(
                    controller: _url,
                    mono: true,
                    hint: 'https://hub.example.lan (empty: scripted fake hub)',
                    onSubmit: (_) => save(),
                  )),
                  KV('Device token', Field(
                    controller: _token,
                    mono: true,
                    hint: 'pd1.xxxxxxxx (set by pairing)',
                    obscure: true,
                    onSubmit: (_) => save(),
                  )),
                  KV('', Align(alignment: Alignment.centerLeft,
                      child: Btn('Save and connect', kind: BtnKind.primary, onTap: save))),
                ],
              ),
            ),
            gap,
            Panel(
              title: 'Pair this device',
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  KV('Pairing code', Field(
                    controller: _code,
                    mono: true,
                    hint: 'XXXX-XXXX-XXXX',
                    onSubmit: (_) => unawaited(pair()),
                  )),
                  KV('', Wrap(spacing: 12, runSpacing: 8, crossAxisAlignment: WrapCrossAlignment.center, children: [
                    Btn('Pair', onTap: () => unawaited(pair())),
                    Line([
                      sp(context, 'On the hub: ', color: p.faint, size: kSmall),
                      sp(context, 'protectorate device add <name>', mono: true, color: p.dim, size: 11.5),
                    ], maxLines: 2),
                  ])),
                ],
              ),
            ),
            if (_devices != null || _devicesError != null) ...[
              gap,
              Panel(
                title: 'Devices',
                trailing: _devices == null ? null : T('${_devices!.length}', color: p.faint, size: kSmall),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    if (_devicesError != null) T('Could not load devices: $_devicesError', color: p.alarm, maxLines: 3),
                    for (final d in _devices ?? const <Device>[])
                      Container(
                        height: 40,
                        decoration: BoxDecoration(border: Border(top: BorderSide(color: p.line))),
                        child: Row(children: [
                          Expanded(child: Line([
                            sp(context, d.name, weight: FontWeight.w500),
                            sp(context, '   ${d.id}', mono: true, color: p.faint, size: 11.5),
                          ])),
                          if (d.id == me) ...[Pill('this device', color: p.accent), const SizedBox(width: 10)],
                          Btn('Revoke', kind: BtnKind.danger, dense: true, onTap: () => unawaited(revoke(d))),
                        ]),
                      ),
                  ],
                ),
              ),
            ] else if (app.hubUrl.isNotEmpty && app.hubToken.isNotEmpty) ...[
              gap,
              Panel(title: 'Devices', child: T('Loading', color: p.faint)),
            ],
            if (_doctor case final d?) ...[gap, _doctorPanel(d, p)],
            gap,
            Panel(
              title: 'Appearance',
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  KV('Theme', Align(alignment: Alignment.centerLeft, child: Buttons([
                    Btn('Dark', active: app.dark, onTap: () => app.setDark(true)),
                    Btn('Light', active: !app.dark, onTap: () => app.setDark(false)),
                  ]))),
                  KV('Fonts', T('Inter for the interface, JetBrains Mono for code (both SIL OFL 1.1, bundled)',
                      color: p.dim, maxLines: 2)),
                  KV('App version', T(kVersion, mono: true, size: 12.5, color: p.dim)),
                ],
              ),
            ),
            gap,
            Panel(
              title: 'Keyboard',
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  for (final (k, v) in const [
                    ('F1', 'Envoy'),
                    ('F2', 'Sleeves'),
                    ('F3', 'Approvals'),
                    ('F4', 'Nodes'),
                    ('F5', 'Quota'),
                    ('F6', 'Mail and events'),
                    ('F12', 'Settings'),
                    ('Esc, Ctrl K', 'Command palette'),
                    ('Enter', 'Send; Shift+Enter breaks the line'),
                  ])
                    Padding(
                      padding: const EdgeInsets.only(bottom: 8),
                      child: Row(children: [
                        SizedBox(width: 120, child: Align(alignment: Alignment.centerLeft,
                            child: Wrap(spacing: 4, children: [for (final x in k.split(', ')) Kbd(x)]))),
                        Expanded(child: T(v, color: p.dim)),
                      ]),
                    ),
                ],
              ),
            ),
            gap,
            Panel(
              title: 'Commands',
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  for (final (k, v) in commandHelp)
                    Padding(
                      padding: const EdgeInsets.only(bottom: 8),
                      child: Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
                        SizedBox(width: 140, child: T(k, mono: true, size: 12, color: p.accent)),
                        Expanded(child: T(v, color: p.dim, maxLines: 2)),
                      ]),
                    ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}

const commandHelp = [
  ('e', 'Envoy chat'),
  ('s [name]', 'Sleeves, or open one sleeve'),
  ('n [name]', 'Nodes, or open one node'),
  ('a', 'Approvals'),
  ('q', 'Quota'),
  ('ev', 'Mail and events'),
  ('> text', 'Send text to Envoy'),
  ('/approve [id]', 'Approve; without an id, the only pending one'),
  ('/deny [id]', 'Deny; same rule'),
  ('/interrupt', 'Interrupt the open sleeve'),
  ('/stop', 'Stop the open sleeve and its children'),
  ('/resleeve h', 'Resleeve the open sleeve to harness h'),
  ('theme', 'Switch between dark and light'),
  ('set', 'Settings'),
];
