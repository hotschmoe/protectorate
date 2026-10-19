import 'package:flutter/material.dart';

import '../hubs/hub_profile.dart';
import 'terminal_screen.dart';

class SessionsTab extends StatefulWidget {
  const SessionsTab({super.key, required this.hub});

  final HubProfile hub;

  @override
  State<SessionsTab> createState() => _SessionsTabState();
}

class _SessionsTabState extends State<SessionsTab> {
  final _name = TextEditingController();

  @override
  void dispose() {
    _name.dispose();
    super.dispose();
  }

  void _open() {
    final name = _name.text.trim();
    if (name.isEmpty) return;
    Navigator.of(context).push(MaterialPageRoute<void>(
      builder: (_) => TerminalScreen(hub: widget.hub, session: name),
    ));
  }

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.all(16),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const Text('Session listing is not wired yet. Open a terminal by name.'),
          const SizedBox(height: 16),
          Row(
            children: [
              Expanded(
                child: TextField(
                  key: const ValueKey('session-name'),
                  controller: _name,
                  decoration: const InputDecoration(labelText: 'Session name'),
                  onSubmitted: (_) => _open(),
                ),
              ),
              const SizedBox(width: 8),
              FilledButton(onPressed: _open, child: const Text('Open')),
            ],
          ),
        ],
      ),
    );
  }
}
