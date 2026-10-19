import 'dart:async';

import 'package:flutter/material.dart';
import 'package:fixnum/fixnum.dart';

import '../gen/protectorate/v1/sleeve.pb.dart';
import '../hubs/event_feed.dart';
import '../hubs/hub_client.dart';
import '../hubs/hub_profile.dart';
import '../nodes/node_view.dart';
import 'terminal_screen.dart';

final _sleeveName = RegExp(r'^[a-z0-9][a-z0-9-]{0,39}$');

class SleevesScreen extends StatefulWidget {
  const SleevesScreen({
    super.key,
    required this.hub,
    required this.client,
    required this.feed,
  });

  final HubProfile hub;
  final HubClient client;
  final EventFeed feed;

  @override
  State<SleevesScreen> createState() => _SleevesScreenState();
}

class _SleevesScreenState extends State<SleevesScreen> {
  List<Sleeve> _sleeves = const [];
  Object? _error;
  bool _loading = true;
  StreamSubscription<dynamic>? _sub;
  Timer? _poll;

  @override
  void initState() {
    super.initState();
    _sub = widget.feed.incoming.listen((e) {
      if (e.kind.startsWith('sleeve.')) _refresh();
    });
    _poll = Timer.periodic(const Duration(seconds: 10), (_) => _refresh());
    _refresh();
  }

  @override
  void dispose() {
    _sub?.cancel();
    _poll?.cancel();
    super.dispose();
  }

  Future<void> _refresh() async {
    try {
      final r = await widget.client.sleeves.listSleeves(ListSleevesRequest());
      if (mounted) {
        setState(() {
          _sleeves = r.sleeves;
          _error = null;
        });
      }
    } on Object catch (e) {
      if (mounted) setState(() => _error = e);
    } finally {
      if (mounted) setState(() => _loading = false);
    }
  }

  void _fail(Object e) {
    if (!mounted) return;
    ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text('$e')));
  }

  Future<void> _spawn() async {
    final req = await showDialog<SpawnSleeveRequest>(
      context: context,
      builder: (_) => const SpawnSleeveDialog(),
    );
    if (req == null) return;
    try {
      await widget.client.sleeves.spawnSleeve(req);
      await _refresh();
    } on Object catch (e) {
      _fail(e);
    }
  }

  Future<void> _kill(Sleeve s) async {
    final ok = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: Text('Kill sleeve ${s.name}?'),
        content: const Text(
          'This stops the harness and every process in its sandbox. '
          'The workspace and the sleeve home directory are kept on disk.',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(ctx).pop(false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            key: const ValueKey('confirm-kill'),
            onPressed: () => Navigator.of(ctx).pop(true),
            child: const Text('Kill'),
          ),
        ],
      ),
    );
    if (ok != true) return;
    try {
      await widget.client.sleeves.killSleeve(KillSleeveRequest(name: s.name));
      await _refresh();
    } on Object catch (e) {
      _fail(e);
    }
  }

  void _attach(Sleeve s) {
    Navigator.of(context).push(
      MaterialPageRoute<void>(
        builder: (_) => TerminalScreen(
          hub: widget.hub,
          client: widget.client,
          session: s.session,
        ),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final cs = Theme.of(context).colorScheme;
    return Scaffold(
      appBar: AppBar(title: const Text('Sleeves')),
      floatingActionButton: FloatingActionButton.extended(
        key: const ValueKey('spawn-sleeve'),
        heroTag: null,
        onPressed: _spawn,
        icon: const Icon(Icons.add),
        label: const Text('Spawn'),
      ),
      body: _loading
          ? const LinearProgressIndicator()
          : Column(
              children: [
                if (_error != null)
                  ListTile(
                    leading: Icon(Icons.error_outline, color: cs.error),
                    title: Text('$_error'),
                  ),
                Expanded(
                  child: _sleeves.isEmpty
                      ? const Center(child: Text('No sleeves.'))
                      : ListView(
                          padding: const EdgeInsets.fromLTRB(12, 8, 12, 88),
                          children: [
                            for (final s in _sleeves)
                              Card(
                                margin: const EdgeInsets.only(bottom: 8),
                                child: Padding(
                                  padding: const EdgeInsets.all(12),
                                  child: Column(
                                    crossAxisAlignment:
                                        CrossAxisAlignment.start,
                                    children: [
                                      SleeveRow(sleeve: s),
                                      Text(
                                        s.workspace,
                                        style: Theme.of(context)
                                            .textTheme
                                            .bodySmall
                                            ?.copyWith(
                                              color: cs.onSurfaceVariant,
                                            ),
                                      ),
                                      OverflowBar(
                                        alignment: MainAxisAlignment.end,
                                        children: [
                                          TextButton(
                                            key: ValueKey('attach-${s.name}'),
                                            onPressed:
                                                s.state ==
                                                    SleeveState
                                                        .SLEEVE_STATE_RUNNING
                                                ? () => _attach(s)
                                                : null,
                                            child: const Text('Attach'),
                                          ),
                                          TextButton(
                                            key: ValueKey('kill-${s.name}'),
                                            style: TextButton.styleFrom(
                                              foregroundColor: cs.error,
                                            ),
                                            onPressed: () => _kill(s),
                                            child: const Text('Kill'),
                                          ),
                                        ],
                                      ),
                                    ],
                                  ),
                                ),
                              ),
                          ],
                        ),
                ),
              ],
            ),
    );
  }
}

class SpawnSleeveDialog extends StatefulWidget {
  const SpawnSleeveDialog({super.key});

  @override
  State<SpawnSleeveDialog> createState() => _SpawnSleeveDialogState();
}

class _SpawnSleeveDialogState extends State<SpawnSleeveDialog> {
  final _form = GlobalKey<FormState>();
  final _name = TextEditingController();
  final _workspace = TextEditingController();
  final _cpu = TextEditingController();
  final _mem = TextEditingController();
  Harness _harness = Harness.HARNESS_CLAUDE;

  @override
  void dispose() {
    _name.dispose();
    _workspace.dispose();
    _cpu.dispose();
    _mem.dispose();
    super.dispose();
  }

  String? _optionalInt(String? v) {
    final t = (v ?? '').trim();
    if (t.isEmpty) return null;
    final n = int.tryParse(t);
    return n == null || n < 0 ? 'whole number' : null;
  }

  void _submit() {
    if (!_form.currentState!.validate()) return;
    final cpu = int.tryParse(_cpu.text.trim()) ?? 0;
    final memMb = int.tryParse(_mem.text.trim()) ?? 0;
    Navigator.of(context).pop(
      SpawnSleeveRequest(
        name: _name.text.trim(),
        workspace: _workspace.text.trim(),
        harness: _harness,
        limits: SleeveLimits(cpuPercent: cpu, memoryBytes: Int64(memMb) << 20),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      title: const Text('Spawn sleeve'),
      content: SingleChildScrollView(
        child: Form(
          key: _form,
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              TextFormField(
                key: const ValueKey('sleeve-name'),
                controller: _name,
                decoration: const InputDecoration(labelText: 'Name'),
                validator: (v) => _sleeveName.hasMatch((v ?? '').trim())
                    ? null
                    : 'a-z, 0-9 and -, up to 40',
              ),
              TextFormField(
                key: const ValueKey('sleeve-workspace'),
                controller: _workspace,
                decoration: const InputDecoration(
                  labelText: 'Workspace path or git URL',
                ),
                validator: (v) => (v ?? '').trim().isEmpty ? 'required' : null,
              ),
              DropdownButtonFormField<Harness>(
                initialValue: _harness,
                decoration: const InputDecoration(labelText: 'Harness'),
                items: [
                  for (final h in Harness.values.where(
                    (h) => h != Harness.HARNESS_UNSPECIFIED,
                  ))
                    DropdownMenuItem(value: h, child: Text(harnessLabel(h))),
                ],
                onChanged: (h) => setState(() => _harness = h ?? _harness),
              ),
              TextFormField(
                key: const ValueKey('sleeve-cpu'),
                controller: _cpu,
                keyboardType: TextInputType.number,
                decoration: const InputDecoration(
                  labelText: 'CPU limit %',
                  helperText: '100 = one core, empty = unlimited',
                ),
                validator: _optionalInt,
              ),
              TextFormField(
                key: const ValueKey('sleeve-mem'),
                controller: _mem,
                keyboardType: TextInputType.number,
                decoration: const InputDecoration(
                  labelText: 'Memory limit MB',
                  helperText: 'empty = unlimited',
                ),
                validator: _optionalInt,
              ),
            ],
          ),
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(),
          child: const Text('Cancel'),
        ),
        FilledButton(
          key: const ValueKey('sleeve-spawn'),
          onPressed: _submit,
          child: const Text('Spawn'),
        ),
      ],
    );
  }
}
