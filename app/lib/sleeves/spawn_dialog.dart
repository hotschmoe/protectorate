import 'package:fixnum/fixnum.dart';
import 'package:flutter/material.dart';

import '../gen/protectorate/v1/sleeve.pb.dart';
import '../hubs/hub_client.dart';
import '../nodes/node_view.dart';

final _sleeveName = RegExp(r'^[a-z0-9][a-z0-9-]{0,39}$');

// The sleeve name a repository URL or directory path suggests, or '' when
// nothing usable is left of it.
String suggestSleeveName(String workspace) {
  var s = workspace.trim();
  while (s.endsWith('/')) {
    s = s.substring(0, s.length - 1);
  }
  s = s.split(RegExp(r'[/:]')).last;
  if (s.endsWith('.git')) s = s.substring(0, s.length - 4);
  s = s
      .toLowerCase()
      .replaceAll(RegExp(r'[^a-z0-9]+'), '-')
      .replaceAll(RegExp(r'^-+|-+$'), '');
  if (s.length > 40) s = s.substring(0, 40).replaceAll(RegExp(r'-+$'), '');
  return s;
}

// Asks the hub for its known workspaces, lets the user pick or enter one and
// spawns the sleeve. Returns null when cancelled; a failed spawn throws.
Future<Sleeve?> startSleeve(BuildContext context, HubClient client) async {
  var known = const <Workspace>[];
  try {
    known = (await client.sleeves.listWorkspaces(
      ListWorkspacesRequest(),
    )).workspaces;
  } on Object {
    // An older hub has no list; the dialog still takes a URL or a path.
  }
  if (!context.mounted) return null;
  final req = await showDialog<SpawnSleeveRequest>(
    context: context,
    builder: (_) => SpawnSleeveDialog(workspaces: known),
  );
  if (req == null) return null;
  return (await client.sleeves.spawnSleeve(req)).sleeve;
}

enum _Source { existing, clone, path }

class SpawnSleeveDialog extends StatefulWidget {
  const SpawnSleeveDialog({super.key, this.workspaces = const []});

  final List<Workspace> workspaces;

  @override
  State<SpawnSleeveDialog> createState() => _SpawnSleeveDialogState();
}

class _SpawnSleeveDialogState extends State<SpawnSleeveDialog> {
  final _form = GlobalKey<FormState>();
  final _name = TextEditingController();
  final _text = TextEditingController();
  final _cpu = TextEditingController();
  final _mem = TextEditingController();
  Harness _harness = Harness.HARNESS_CLAUDE;
  late _Source _source;
  Workspace? _picked;
  bool _nameTouched = false;

  @override
  void initState() {
    super.initState();
    _source = widget.workspaces.isEmpty ? _Source.clone : _Source.existing;
  }

  @override
  void dispose() {
    _name.dispose();
    _text.dispose();
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

  // The workspace string SpawnSleeve takes for the current choice.
  String get _workspace =>
      _source == _Source.existing ? (_picked?.path ?? '') : _text.text.trim();

  void _suggest() {
    final required = _source == _Source.existing ? _picked?.requiredName : '';
    if (required != null && required.isNotEmpty) {
      _name.text = required;
    } else if (!_nameTouched) {
      _name.text = suggestSleeveName(
        _source == _Source.existing ? (_picked?.name ?? '') : _text.text,
      );
    }
  }

  void _pick(_Source s) {
    setState(() {
      _source = s;
      _text.clear();
      _picked = null;
      if (!_nameTouched) _name.clear();
    });
  }

  String? _validateWorkspace(String? v) {
    final t = (v ?? '').trim();
    if (t.isEmpty) return 'required';
    if (_source == _Source.path && !t.startsWith('/')) {
      return 'an absolute path on the hub';
    }
    if (_source == _Source.clone && (t.startsWith('-') || t.startsWith('/'))) {
      return 'a git URL; use Path for a directory';
    }
    return null;
  }

  void _submit() {
    if (!_form.currentState!.validate()) return;
    final cpu = int.tryParse(_cpu.text.trim()) ?? 0;
    final memMb = int.tryParse(_mem.text.trim()) ?? 0;
    Navigator.of(context).pop(
      SpawnSleeveRequest(
        name: _name.text.trim(),
        workspace: _workspace,
        harness: _harness,
        limits: SleeveLimits(cpuPercent: cpu, memoryBytes: Int64(memMb) << 20),
      ),
    );
  }

  String _detail(Workspace w) => [
    w.path,
    if (w.gitBranch.isNotEmpty) 'branch ${w.gitBranch}',
    if (w.gitRemote.isNotEmpty) w.gitRemote,
    if (w.sleeve.isNotEmpty)
      w.busy ? 'in use by ${w.sleeve}' : 'last used by ${w.sleeve}',
  ].join('\n');

  Widget _workspaceField() {
    switch (_source) {
      case _Source.existing:
        return DropdownButtonFormField<Workspace>(
          key: const ValueKey('sleeve-existing'),
          isExpanded: true,
          initialValue: _picked,
          decoration: InputDecoration(
            labelText: 'Workspace',
            helperText: _picked == null ? null : _detail(_picked!),
            helperMaxLines: 4,
          ),
          autovalidateMode: AutovalidateMode.onUserInteraction,
          validator: (w) => w == null ? 'pick a workspace' : null,
          items: [
            for (final w in widget.workspaces)
              DropdownMenuItem(
                value: w,
                enabled: !w.busy,
                child: Text(
                  w.busy ? '${w.name} (in use)' : w.name,
                  overflow: TextOverflow.ellipsis,
                ),
              ),
          ],
          onChanged: (w) {
            setState(() => _picked = w);
            _suggest();
          },
        );
      case _Source.clone:
      case _Source.path:
        final clone = _source == _Source.clone;
        return TextFormField(
          key: const ValueKey('sleeve-workspace'),
          controller: _text,
          decoration: InputDecoration(
            labelText: clone ? 'Git URL' : 'Absolute path on the hub',
            helperText: clone
                ? 'Cloned into a new workspace named after the sleeve. '
                      'Needs a repository the hub can read without a login.'
                : 'An existing directory, bound read-write into the sandbox.',
            helperMaxLines: 3,
          ),
          validator: _validateWorkspace,
          onChanged: (_) => setState(_suggest),
        );
    }
  }

  @override
  Widget build(BuildContext context) {
    final fixed =
        _source == _Source.existing && (_picked?.requiredName ?? '').isNotEmpty;
    return AlertDialog(
      title: const Text('Start a sleeve'),
      content: SingleChildScrollView(
        child: Form(
          key: _form,
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              SegmentedButton<_Source>(
                key: const ValueKey('sleeve-source'),
                showSelectedIcon: false,
                segments: [
                  if (widget.workspaces.isNotEmpty)
                    const ButtonSegment(
                      value: _Source.existing,
                      label: Text('EXISTING'),
                    ),
                  const ButtonSegment(
                    value: _Source.clone,
                    label: Text('CLONE'),
                  ),
                  const ButtonSegment(value: _Source.path, label: Text('PATH')),
                ],
                selected: {_source},
                onSelectionChanged: (s) => _pick(s.first),
              ),
              _workspaceField(),
              TextFormField(
                key: const ValueKey('sleeve-name'),
                controller: _name,
                readOnly: fixed,
                decoration: InputDecoration(
                  labelText: 'Name',
                  helperText: fixed
                      ? 'This workspace belongs to a sleeve of this name'
                      : null,
                ),
                onChanged: (_) => _nameTouched = true,
                validator: (v) => _sleeveName.hasMatch((v ?? '').trim())
                    ? null
                    : 'a-z, 0-9 and -, up to 40',
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
          child: const Text('CANCEL'),
        ),
        FilledButton(
          key: const ValueKey('sleeve-spawn'),
          onPressed: _submit,
          child: const Text('START'),
        ),
      ],
    );
  }
}
