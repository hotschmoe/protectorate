import 'package:flutter/material.dart';

import '../scope.dart';
import '../theme.dart';
import '../widgets.dart';

class SettingsScreen extends StatefulWidget {
  const SettingsScreen({super.key});

  @override
  State<SettingsScreen> createState() => _SettingsScreenState();
}

class _SettingsScreenState extends State<SettingsScreen> {
  final _url = TextEditingController(text: 'https://protectorate.hotschmoe.com');
  final _token = TextEditingController();

  @override
  void dispose() {
    _url.dispose();
    _token.dispose();
    super.dispose();
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
    void save() => app.say('saved for this session only; the client still talks to ${app.hub.label}');
    return ListView(children: [
      const HRule(label: 'HUB'),
      kv('CONNECTED', T(app.hub.label, color: p.warn)),
      kv('HUB URL', Field(controller: _url, prompt: '', onSubmit: (_) => save())),
      kv('DEVICE TOKEN', Field(
        controller: _token,
        prompt: '',
        hint: 'pd1.xxxxxxxx (from protectorate device add)',
        obscure: true,
        onSubmit: (_) => save(),
      )),
      kv('', Actions3270([Btn('SAVE', onTap: save)])),
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
