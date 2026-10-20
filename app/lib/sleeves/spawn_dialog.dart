import 'package:fixnum/fixnum.dart';
import 'package:flutter/material.dart';

import '../gen/protectorate/v1/sleeve.pb.dart';
import '../gen/protectorate/v1/chat.pb.dart';
import 'model_selection.dart';
import '../hubs/hub_client.dart';
import '../nodes/node_view.dart';
import '../theme/tokens.dart';

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

// Asks the hub for its workspaces, lets the user browse or enter one and
// spawns the sleeve. Returns null when cancelled; a failed spawn throws.
Future<Sleeve?> startSleeve(BuildContext context, HubClient client) async {
  var known = const <Workspace>[];
  try {
    known = (await client.sleeves.listWorkspaces(
      ListWorkspacesRequest(),
    )).workspaces;
  } on Object {
    // The browser and typed inputs remain usable when discovery fails.
  }
  if (!context.mounted) return null;
  final req = await showDialog<SpawnSleeveRequest>(
    context: context,
    builder: (_) => SpawnSleeveDialog(workspaces: known, client: client),
  );
  if (req == null) return null;
  return (await client.sleeves.spawnSleeve(req)).sleeve;
}

enum _Source { browse, existing, clone, path }

class SpawnSleeveDialog extends StatefulWidget {
  const SpawnSleeveDialog({super.key, this.workspaces = const [], this.client});
  final HubClient? client;

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
  final _branch = TextEditingController();
  final _newBranch = TextEditingController();
  final _brief = TextEditingController();
  Harness _harness = Harness.HARNESS_CLAUDE;
  ModelSelection _selection = ModelSelection();
  late _Source _source;
  Workspace? _picked;
  bool _nameTouched = false;
  BrowseWorkspacesResponse? _listing;
  WorkspaceDirectory? _directory;
  bool _loading = false;
  String? _browseError;

  @override
  void initState() {
    super.initState();
    _source = _Source.browse;
    _browse('');
  }

  @override
  void dispose() {
    _name.dispose();
    _text.dispose();
    _cpu.dispose();
    _mem.dispose();
    _branch.dispose();
    _newBranch.dispose();
    _brief.dispose();
    super.dispose();
  }

  String? _optionalInt(String? v) {
    final t = (v ?? '').trim();
    if (t.isEmpty) return null;
    final n = int.tryParse(t);
    return n == null || n < 0 ? 'whole number' : null;
  }

  // The workspace string SpawnSleeve takes for the current choice.
  String get _workspace => _source == _Source.browse
      ? (_directory?.path ?? '')
      : _source == _Source.existing
      ? (_picked?.path ?? '')
      : _text.text.trim();

  void _suggest() {
    final required = _source == _Source.browse
        ? _directory?.requiredName
        : _source == _Source.existing
        ? _picked?.requiredName
        : '';
    if (required != null && required.isNotEmpty) {
      _name.text = required;
    } else if (!_nameTouched) {
      _name.text = suggestSleeveName(_workspace);
    }
  }

  void _pick(_Source s) {
    setState(() {
      _source = s;
      _text.clear();
      _picked = null;
      _directory = null;
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
        branch: _branch.text.trim(),
        newBranch: _newBranch.text.trim(),
        brief: _harness == Harness.HARNESS_SHELL ? '' : _brief.text,
        harness: _harness,
        selection: _selection,
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

  Future<void> _browse(String path) async {
    final client = widget.client;
    if (client == null) return;
    setState(() {
      _loading = true;
      _browseError = null;
    });
    try {
      final listing = await client.sleeves.browseWorkspaces(
        BrowseWorkspacesRequest(path: path),
      );
      if (!mounted) return;
      setState(() {
        _listing = listing;
        _loading = false;
      });
    } on Object catch (e) {
      if (!mounted) return;
      setState(() {
        _browseError = '$e';
        _loading = false;
      });
    }
  }

  Widget _browser() {
    final t = Tokens.of(context);
    final listing = _listing;
    return FormField<String>(
      validator: (_) => _directory == null ? 'Select a workspace' : null,
      builder: (field) => Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              TextButton(
                onPressed: _loading || listing == null || listing.path.isEmpty
                    ? null
                    : () => _browse(listing.parent),
                child: const Text('BACK'),
              ),
              Expanded(
                child: Text(
                  listing?.path.isNotEmpty == true
                      ? listing!.path
                      : 'HUB ROOTS',
                  style: t.small(t.p.lit),
                  maxLines: 2,
                  overflow: TextOverflow.ellipsis,
                ),
              ),
            ],
          ),
          if (_loading) const LinearProgressIndicator(),
          if (_browseError != null)
            Text(_browseError!, style: t.small(t.p.lit)),
          if (!_loading && listing != null && listing.directories.isEmpty)
            Text('No directories. Use PATH or CLONE.', style: t.small(t.p.dim)),
          SizedBox(
            height: 210,
            width: double.maxFinite,
            child: ListView(
              children: [
                for (final d in listing?.directories ?? <WorkspaceDirectory>[])
                  Padding(
                    padding: const EdgeInsets.symmetric(vertical: 4),
                    child: Row(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Expanded(
                          child: TextButton(
                            onPressed: _loading || !d.acceptable
                                ? null
                                : () {
                                    setState(() {
                                      _directory = d;
                                      _suggest();
                                    });
                                    field.didChange(d.path);
                                  },
                            child: Align(
                              alignment: Alignment.centerLeft,
                              child: Column(
                                crossAxisAlignment: CrossAxisAlignment.start,
                                children: [
                                  Text(
                                    d.name,
                                    style: t.data(
                                      d.acceptable ? t.p.lit : t.p.dim,
                                    ),
                                  ),
                                  if (d.isGit)
                                    Text(
                                      d.gitBranch.isEmpty
                                          ? 'git'
                                          : 'git: ${d.gitBranch}',
                                      style: t.small(t.p.dim),
                                    ),
                                  if (!d.acceptable)
                                    Text(d.reason, style: t.small(t.p.dim)),
                                ],
                              ),
                            ),
                          ),
                        ),
                        TextButton(
                          onPressed: _loading ? null : () => _browse(d.path),
                          child: const Text('OPEN'),
                        ),
                      ],
                    ),
                  ),
              ],
            ),
          ),
          if (listing?.truncated == true)
            Text(
              'Listing truncated. Use PATH for other directories.',
              style: t.small(t.p.dim),
            ),
          if (_directory != null)
            Text('Selected: ${_directory!.path}', style: t.small(t.p.lit)),
          if (field.hasError) Text(field.errorText!, style: t.small(t.p.lit)),
        ],
      ),
    );
  }

  Widget _workspaceField() {
    switch (_source) {
      case _Source.browse:
        return _browser();
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
        (_source == _Source.existing &&
            (_picked?.requiredName ?? '').isNotEmpty) ||
        (_source == _Source.browse &&
            (_directory?.requiredName ?? '').isNotEmpty);
    return AlertDialog(
      title: const Text('Start a sleeve'),
      content: SingleChildScrollView(
        child: Form(
          key: _form,
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              DropdownButtonFormField<_Source>(
                key: const ValueKey('sleeve-source'),
                initialValue: _source,
                isExpanded: true,
                decoration: const InputDecoration(
                  labelText: 'Workspace source',
                ),
                items: [
                  const DropdownMenuItem(
                    value: _Source.browse,
                    child: Text('BROWSE HUB'),
                  ),
                  if (widget.workspaces.isNotEmpty)
                    const DropdownMenuItem(
                      value: _Source.existing,
                      child: Text('KNOWN WORKSPACES'),
                    ),
                  const DropdownMenuItem(
                    value: _Source.clone,
                    child: Text('CLONE'),
                  ),
                  const DropdownMenuItem(
                    value: _Source.path,
                    child: Text('PATH'),
                  ),
                ],
                onChanged: (s) {
                  if (s != null) _pick(s);
                },
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
                onChanged: (h) => setState(() {
                  _harness = h ?? _harness;
                  _selection = ModelSelection();
                }),
              ),
              ModelSelectionFields(
                client: widget.client,
                harness: _harness,
                selection: _selection,
                onChanged: (value) => setState(() => _selection = value),
              ),
              TextFormField(
                key: const ValueKey('sleeve-branch'),
                controller: _branch,
                decoration: const InputDecoration(
                  labelText: 'Base branch (optional)',
                  helperText: 'Branch fields create a separate clone',
                ),
              ),
              TextFormField(
                key: const ValueKey('sleeve-new-branch'),
                controller: _newBranch,
                decoration: const InputDecoration(
                  labelText: 'New branch (optional)',
                ),
              ),
              TextFormField(
                key: const ValueKey('sleeve-brief'),
                controller: _brief,
                minLines: 3,
                maxLines: 8,
                enabled: _harness != Harness.HARNESS_SHELL,
                decoration: const InputDecoration(
                  labelText: 'Brief (optional, chat only)',
                ),
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
