import 'package:flutter/material.dart';

import '../gen/protectorate/v1/session.pb.dart';
import '../hubs/hub_client.dart';
import '../hubs/hub_profile.dart';
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
    } finally {
      if (mounted) setState(() => _loading = false);
    }
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
        title: const Text('New session'),
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
            child: const Text('Cancel'),
          ),
          FilledButton(
            key: const ValueKey('new-create'),
            onPressed: () => Navigator.of(context).pop(true),
            child: const Text('Create'),
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
    return Scaffold(
      floatingActionButton: FloatingActionButton.extended(
        heroTag: null,
        key: const ValueKey('new-session'),
        onPressed: _create,
        icon: const Icon(Icons.add),
        label: const Text('New session'),
      ),
      body: Column(
        children: [
          if (_loading) const LinearProgressIndicator(),
          if (_error != null) ListTile(title: Text('$_error')),
          Expanded(
            child: RefreshIndicator(
              onRefresh: _refresh,
              child: ListView(
                physics: const AlwaysScrollableScrollPhysics(),
                children: [
                  for (final s in _sessions)
                    ListTile(
                      key: ValueKey('session-${s.name}'),
                      leading: const Icon(Icons.terminal),
                      title: Text(s.name),
                      subtitle: Text(
                        '${s.cwd}  (${s.attachedClients} attached)',
                      ),
                      onTap: () => _open(s.name),
                      trailing: IconButton(
                        tooltip: 'Kill',
                        icon: const Icon(Icons.stop_circle_outlined),
                        onPressed: () => _kill(s.name),
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
}
