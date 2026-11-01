import 'dart:async';

import 'package:flutter/material.dart';

import '../api/api.g.dart';
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
    unawaited(_api(app).listDevices().then((l) {
      if (mounted) setState(() => _devices = l.devices);
    }, onError: (Object _) {
      if (mounted) setState(() => _devices = null);
    }));
    unawaited(_api(app).getDoctor(const GetDoctorRequest()).then((d) {
      if (mounted) setState(() => _doctor = d);
    }, onError: (Object _) {
      if (mounted) setState(() => _doctor = null);
    }));
  }

  /// The hub machine's doctor report, read-only: one line per harness, then the problems.
  List<Widget> _doctorRows(DoctorReport d, Palette p, Widget Function(String, Widget) kv) {
    final checked = d.checkedAt.millisecondsSinceEpoch == 0
        ? 'not yet'
        : '${d.checkedAt.toLocal().toIso8601String().substring(0, 10)} ${hhmm(d.checkedAt.toLocal())}';
    return [
      const HRule(label: 'DOCTOR'),
      kv('CHECKED', T('$checked  ${d.os}/${d.arch}  sandbox ${d.sandboxOk ? 'ok' : 'NOT OK'}'
          '  gh ${d.ghLoggedIn ? d.ghAccount : 'not logged in'}',
          color: d.sandboxOk ? p.dim : p.alarm)),
      for (final h in d.harnesses)
        kv(h.name.toUpperCase(), T(
          [
            h.eligible ? 'OK' : 'NO',
            h.present ? '${h.version} (${h.install})' : 'missing',
            if (!h.eligible) h.reasons.join('; '),
            if (h.updateNote.isNotEmpty) 'last update: ${h.updateNote}',
          ].join('  '),
          color: h.eligible ? null : p.warn,
          maxLines: 3,
        )),
      for (final x in d.problems) kv('PROBLEM', T(x, color: p.warn, maxLines: 3)),
      kv('', T('protectorate doctor --refresh on the hub runs it again', color: p.dim)),
    ];
  }

  @override
  Widget build(BuildContext context) {
    final app = AppScope.of(context);
    final p = pal(context);
    Widget kv(String k, Widget v) => Padding(
      padding: const EdgeInsets.only(bottom: 4),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [Cells(14, child: T(k, color: p.dim)), Expanded(child: v)],
      ),
    );
    _loadDevices(app);
    final kept = pageOrigin() != null ? 'kept in this browser' : 'kept for this session only';
    void save() {
      app.connect(_url.text, _token.text);
      app.say(_url.text.trim().isEmpty
          ? 'no hub URL: using the scripted FAKE HUB'
          : 'connecting to ${_url.text.trim()} ($kept)');
    }
    Future<void> pair() async {
      if (_url.text.trim() != app.hubUrl) app.connect(_url.text, '');
      await app.act(() async {
        final name = await app.pair(_code.text);
        _code.clear();
        app.say('paired as $name ($kept)');
      });
    }
    Future<void> revoke(Device d) => app.act(() async {
      await _api(app).revokeDevice(DeviceRef(deviceId: d.id));
      _devicesFor = null;
      if (d.id == loadSetting('device_id')) app.connect(app.hubUrl, '');
      if (mounted) setState(() {});
    }, done: 'revoked ${d.name}');
    final me = loadSetting('device_id');
    return ListView(children: [
      const HRule(label: 'HUB'),
      kv('CONNECTED', T(app.hub.label, color: p.warn)),
      kv('HUB URL', Field(
        controller: _url,
        prompt: '',
        hint: 'https://protectorate.hotschmoe.com (empty: scripted fake hub)',
        onSubmit: (_) => save(),
      )),
      kv('PAIRING CODE', Field(
        controller: _code,
        prompt: '',
        hint: 'XXXX-XXXX-XXXX (on the hub: protectorate device add <name>)',
        onSubmit: (_) => unawaited(pair()),
      )),
      kv('', Actions3270([Btn('PAIR', onTap: () => unawaited(pair()))])),
      kv('DEVICE TOKEN', Field(
        controller: _token,
        prompt: '',
        hint: 'pd1.xxxxxxxx (set by PAIR)',
        obscure: true,
        onSubmit: (_) => save(),
      )),
      kv('', Actions3270([Btn('SAVE', onTap: save)])),
      if (_devices != null) ...[
        const HRule(label: 'DEVICES'),
        for (final d in _devices!)
          kv(d.id, Row(children: [
            Expanded(child: T(d.id == me ? '${d.name} (this device)' : d.name)),
            Btn('REVOKE', onTap: () => unawaited(revoke(d))),
          ])),
      ],
      if (_doctor case final d?) ..._doctorRows(d, p, kv),
      const HRule(label: 'DISPLAY'),
      kv('THEME', Actions3270([
        Btn('AMBER', active: app.dark, onTap: () => app.setDark(true)),
        Btn('LINE PRINTER', active: !app.dark, onTap: () => app.setDark(false)),
      ])),
      kv('FONT', T('JetBrains Mono 13 px (SIL OFL 1.1, bundled)', color: p.dim)),
      const HRule(label: 'KEYS'),
      for (final (k, v) in const [
        ('F1', 'Envoy chat'),
        ('F2', 'sleeves'),
        ('F3', 'approvals'),
        ('F4', 'nodes'),
        ('F5', 'quota'),
        ('F6', 'mail and events'),
        ('F12', 'settings'),
        ('ESC', 'command line'),
        ('ENTER', 'send; SHIFT+ENTER breaks the line'),
      ])
        kv(k, T(v)),
      const HRule(label: 'COMMANDS'),
      for (final (k, v) in commandHelp) kv(k, T(v)),
    ]);
  }
}

const commandHelp = [
  ('e', 'Envoy chat'),
  ('s [name]', 'sleeves, or open one sleeve'),
  ('n [name]', 'nodes, or open one node'),
  ('a', 'approvals'),
  ('q', 'quota'),
  ('ev', 'mail and events'),
  ('> text', 'send text to Envoy'),
  ('/approve [id]', 'approve; without an id, the only pending one'),
  ('/deny [id]', 'deny; same rule'),
  ('/interrupt', 'interrupt the open sleeve'),
  ('/stop', 'stop the open sleeve and its children'),
  ('/resleeve h', 'resleeve the open sleeve to harness h'),
  ('theme', 'switch amber / line printer'),
  ('set', 'settings'),
];
