import 'package:connectrpc/connect.dart';
import 'package:flutter/material.dart';

import '../gen/protectorate/v1/chat.pb.dart';
import '../gen/protectorate/v1/session.pb.dart';
import '../gen/protectorate/v1/sleeve.pb.dart';
import '../hubs/hub_client.dart';
import '../hubs/hub_profile.dart';
import '../nodes/node_view.dart';
import '../sleeves/sleeve_info.dart';
import '../sleeves/spawn_dialog.dart';
import '../theme/tokens.dart';
import '../vfd/lamp.dart';
import '../vfd/panel.dart';
import 'chat_screen.dart';
import 'terminal_screen.dart';

class SessionsTab extends StatefulWidget {
  const SessionsTab({
    super.key,
    required this.hub,
    required this.client,
    this.openSession,
  });

  final HubProfile hub;
  final HubClient client;
  final String? openSession;

  @override
  State<SessionsTab> createState() => _SessionsTabState();
}

class _SessionsTabState extends State<SessionsTab> {
  List<Session> _sessions = const [];
  List<Sleeve> _sleeves = const [];
  Object? _error;
  bool _loading = true;

  @override
  void initState() {
    super.initState();
    _refresh();
    final target = widget.openSession;
    if (target != null) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted) _open(target);
      });
    }
  }

  Future<void> _refresh() async {
    setState(() => _loading = true);
    try {
      final r = await widget.client.sessions.listSessions(
        ListSessionsRequest(),
      );
      if (mounted) {
        setState(() {
          _sessions = r.sessions;
          _error = null;
        });
      }
    } on Object catch (e) {
      if (mounted) setState(() => _error = e);
    }
    try {
      final r = await widget.client.sleeves.listSleeves(ListSleevesRequest());
      if (mounted) setState(() => _sleeves = r.sleeves);
    } on Object {
      // Sleeves are Linux hub only; the raw sessions still list.
    } finally {
      if (mounted) setState(() => _loading = false);
    }
  }

  Future<void> _start() async {
    try {
      final s = await startSleeve(context, widget.client);
      if (s == null) return;
      await _refresh();
      if (mounted) _openSleeve(s);
    } on Object catch (e) {
      if (mounted) setState(() => _error = e);
    }
  }

  void _openSleeve(Sleeve s) {
    Navigator.of(context).push(
      MaterialPageRoute<void>(
        builder: (_) => effectiveMode(s) == SleeveMode.SLEEVE_MODE_CHAT
            ? ChatScreen(hub: widget.hub, client: widget.client, sleeve: s)
            : TerminalScreen(
                hub: widget.hub,
                client: widget.client,
                session: s.session,
              ),
      ),
    );
  }

  void _open(String name) {
    Navigator.of(context).push(
      MaterialPageRoute<void>(
        builder: (_) => TerminalScreen(
          hub: widget.hub,
          client: widget.client,
          session: name,
        ),
      ),
    );
  }

  Future<void> _create() async {
    final name = TextEditingController();
    final cwd = TextEditingController();
    final command = TextEditingController();
    final ok = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('New raw session'),
        scrollable: true,
        content: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            TextField(
              key: const ValueKey('new-name'),
              controller: name,
              decoration: const InputDecoration(labelText: 'Name'),
            ),
            TextField(
              controller: cwd,
              decoration: const InputDecoration(labelText: 'Working directory'),
            ),
            TextField(
              controller: command,
              decoration: const InputDecoration(
                labelText: 'Command (blank = shell)',
              ),
            ),
          ],
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(context).pop(false),
            child: const Text('CANCEL'),
          ),
          FilledButton(
            key: const ValueKey('new-create'),
            onPressed: () => Navigator.of(context).pop(true),
            child: const Text('CREATE'),
          ),
        ],
      ),
    );
    final req = CreateSessionRequest(
      name: name.text.trim(),
      cwd: cwd.text.trim(),
      command: command.text.trim().isEmpty
          ? const []
          : command.text.trim().split(RegExp(r'\s+')),
    );
    name.dispose();
    cwd.dispose();
    command.dispose();
    if (ok != true || req.name.isEmpty) return;
    try {
      final r = await widget.client.sessions.createSession(req);
      await _refresh();
      if (mounted) _open(r.session.name);
    } on ConnectException catch (e) {
      if (!mounted) return;
      setState(
        () => _error = e.code == Code.permissionDenied
            ? 'Raw sessions run unsandboxed and need the owner token. '
                  'Use START WORK to run a harness in a sandboxed sleeve instead.'
            : e,
      );
    } on Object catch (e) {
      if (mounted) setState(() => _error = e);
    }
  }

  Future<void> _kill(String name) async {
    try {
      await widget.client.sessions.killSession(KillSessionRequest(name: name));
    } on Object catch (e) {
      if (mounted) setState(() => _error = e);
    }
    await _refresh();
  }

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    return Scaffold(
      floatingActionButton: FloatingActionButton.extended(
        heroTag: null,
        key: const ValueKey('start-work'),
        onPressed: _start,
        icon: const Icon(Icons.add),
        label: const Text('START WORK'),
      ),
      body: Column(
        children: [
          if (_loading) const BusyBar(),
          if (_error != null)
            Padding(
              padding: const EdgeInsets.fromLTRB(12, 12, 12, 0),
              child: NoteStrip('$_error', tone: Tone.alarm),
            ),
          Expanded(
            child: RefreshIndicator(
              onRefresh: _refresh,
              child: ListView(
                physics: const AlwaysScrollableScrollPhysics(),
                padding: const EdgeInsets.fromLTRB(
                  Tokens.gap,
                  Tokens.gap,
                  Tokens.gap,
                  96,
                ),
                children: [
                  Center(
                    child: ConstrainedBox(
                      constraints: const BoxConstraints(
                        maxWidth: Tokens.contentMaxWidth,
                      ),
                      child: Column(
                        children: [
                          Panel(
                            legend: 'SLEEVES',
                            trailing: Text(
                              '${_sleeves.length}',
                              style: t.legend(),
                            ),
                            padding: const EdgeInsets.all(4),
                            child: _sleeves.isEmpty
                                ? Padding(
                                    padding: const EdgeInsets.all(12),
                                    child: Text(
                                      _loading
                                          ? ''
                                          : 'NO SLEEVES. START WORK PICKS A '
                                                'REPOSITORY AND A HARNESS.',
                                      style: t.legend(),
                                    ),
                                  )
                                : Column(
                                    children: [
                                      for (var i = 0; i < _sleeves.length; i++)
                                        _sleeveRow(t, i, _sleeves[i]),
                                    ],
                                  ),
                          ),
                          const SizedBox(height: Tokens.gap),
                          Panel(
                            legend: 'RAW TERMINAL SESSIONS',
                            trailing: Row(
                              mainAxisSize: MainAxisSize.min,
                              children: [
                                Text('${_sessions.length}', style: t.legend()),
                                TextButton(
                                  key: const ValueKey('new-session'),
                                  onPressed: _create,
                                  child: const Text('NEW RAW SESSION'),
                                ),
                              ],
                            ),
                            padding: const EdgeInsets.all(4),
                            child: _sessions.isEmpty
                                ? Padding(
                                    padding: const EdgeInsets.all(12),
                                    child: Text(
                                      _loading ? '' : 'NO SESSIONS',
                                      style: t.legend(),
                                    ),
                                  )
                                : Column(
                                    children: [
                                      for (var i = 0; i < _sessions.length; i++)
                                        _row(t, i, _sessions[i]),
                                    ],
                                  ),
                          ),
                        ],
                      ),
                    ),
                  ),
                ],
              ),
            ),
          ),
        ],
      ),
    );
  }

  Widget _sleeveRow(Tokens t, int i, Sleeve s) {
    return InkWell(
      key: ValueKey('sleeve-${s.name}'),
      borderRadius: BorderRadius.circular(Tokens.innerRadius),
      onTap: () => _openSleeve(s),
      child: BarRow(
        index: i,
        padding: const EdgeInsets.fromLTRB(12, 6, 12, 6),
        child: Row(
          children: [
            Lamp(
              tone: Tone.ok,
              on: s.state == SleeveState.SLEEVE_STATE_RUNNING,
            ),
            const SizedBox(width: 12),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(s.name, style: t.data(t.p.emph)),
                  Text(
                    s.workspace,
                    style: t.small(),
                    overflow: TextOverflow.ellipsis,
                  ),
                ],
              ),
            ),
            Text(harnessLabel(s.harness).toUpperCase(), style: t.legend()),
          ],
        ),
      ),
    );
  }

  Widget _row(Tokens t, int i, Session s) {
    final attached = s.attachedClients;
    return InkWell(
      key: ValueKey('session-${s.name}'),
      borderRadius: BorderRadius.circular(Tokens.innerRadius),
      onTap: () => _open(s.name),
      child: BarRow(
        index: i,
        padding: const EdgeInsets.fromLTRB(12, 6, 0, 6),
        child: Row(
          children: [
            Lamp(tone: Tone.ok, on: attached > 0),
            const SizedBox(width: 12),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(s.name, style: t.data(t.p.emph)),
                  Text(
                    s.cwd,
                    style: t.small(),
                    overflow: TextOverflow.ellipsis,
                  ),
                ],
              ),
            ),
            Text('$attached ATTACHED', style: t.legend()),
            IconButton(
              tooltip: 'Kill',
              color: t.p.alarm,
              icon: const Icon(Icons.stop_circle_outlined),
              onPressed: () => _kill(s.name),
            ),
          ],
        ),
      ),
    );
  }
}
