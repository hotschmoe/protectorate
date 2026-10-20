import 'package:flutter/material.dart';

import '../gen/protectorate/v1/chat.pb.dart';
import '../gen/protectorate/v1/sleeve.pb.dart';
import '../hubs/hub_client.dart';

// Shared native catalog picker for creation and an existing chat.
class ModelSelectionFields extends StatefulWidget {
  const ModelSelectionFields({
    super.key,
    required this.client,
    required this.harness,
    required this.selection,
    required this.onChanged,
    this.sleeve = '',
    this.defaults,
  });
  final HubClient? client;
  final Harness harness;
  final String sleeve;
  final ModelSelection selection;
  final ModelSelection? defaults;
  final ValueChanged<ModelSelection> onChanged;

  @override
  State<ModelSelectionFields> createState() => _ModelSelectionFieldsState();
}

class _ModelSelectionFieldsState extends State<ModelSelectionFields> {
  ListModelsResponse? _catalog;
  Object? _error;
  int _generation = 0;

  @override
  void initState() {
    super.initState();
    _load();
  }

  @override
  void didUpdateWidget(ModelSelectionFields old) {
    super.didUpdateWidget(old);
    if (old.harness != widget.harness ||
        old.sleeve != widget.sleeve ||
        old.client != widget.client) {
      _catalog = null;
      _error = null;
      _load();
    }
  }

  Future<void> _load() async {
    final generation = ++_generation;
    if (widget.harness != Harness.HARNESS_CODEX || widget.client == null) {
      return;
    }
    try {
      final result = await widget.client!.chat.listModels(
        ListModelsRequest(sleeve: widget.sleeve, harness: 'codex'),
      );
      if (mounted && generation == _generation) {
        setState(() => _catalog = result);
      }
    } on Object catch (e) {
      if (mounted && generation == _generation) setState(() => _error = e);
    }
  }

  @override
  Widget build(BuildContext context) {
    if (widget.harness != Harness.HARNESS_CODEX) {
      return const Text(
        'Native defaults; model/effort controls are currently available for Codex chat only.',
      );
    }
    if (_error != null) {
      return Column(
        children: [
          Text('Cannot load native models: $_error'),
          TextButton(
            onPressed: () {
              setState(() => _error = null);
              _load();
            },
            child: const Text('RETRY'),
          ),
        ],
      );
    }
    final catalog = _catalog;
    if (catalog == null) {
      return const Text(
        'Loading native model options; native defaults remain available.',
      );
    }
    if (!catalog.supported) return Text(catalog.reason);
    final selection = widget.selection;
    final models = catalog.models;
    final effectiveModel = selection.model.isNotEmpty
        ? selection.model
        : widget.defaults?.model;
    ModelOption? model;
    for (final m in models) {
      if (m.model == effectiveModel) model = m;
    }
    model ??= models.where((m) => m.isDefault).firstOrNull;
    final efforts = model?.efforts ?? <String>[];
    return Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        DropdownButtonFormField<String>(
          key: ValueKey('model-${selection.model}'),
          initialValue: selection.model,
          isExpanded: true,
          decoration: const InputDecoration(labelText: 'Model'),
          items: [
            const DropdownMenuItem(value: '', child: Text('Native default')),
            for (final m in models)
              DropdownMenuItem(
                value: m.model,
                child: Text(m.name.isEmpty ? m.model : m.name),
              ),
            if (selection.model.isNotEmpty &&
                !models.any((m) => m.model == selection.model))
              DropdownMenuItem(
                value: selection.model,
                child: Text('${selection.model} (not in catalog)'),
              ),
          ],
          onChanged: (v) => widget.onChanged(ModelSelection(model: v ?? '')),
        ),
        DropdownButtonFormField<String>(
          key: ValueKey('effort-${selection.model}-${selection.effort}'),
          initialValue: selection.effort,
          decoration: InputDecoration(
            labelText: 'Reasoning effort',
            helperText: model == null
                ? null
                : 'Model default: ${model.defaultEffort}',
          ),
          items: [
            const DropdownMenuItem(value: '', child: Text('Native default')),
            for (final e in efforts) DropdownMenuItem(value: e, child: Text(e)),
            if (selection.effort.isNotEmpty &&
                !efforts.contains(selection.effort))
              DropdownMenuItem(
                value: selection.effort,
                child: Text('${selection.effort} (not in catalog)'),
              ),
          ],
          onChanged: (v) => widget.onChanged(
            ModelSelection(model: selection.model, effort: v ?? ''),
          ),
        ),
      ],
    );
  }
}

Future<ChatStatus?> editModelSelection(
  BuildContext context,
  HubClient client,
  Sleeve sleeve,
) async {
  var selection = sleeve.chat.settings.requested.deepCopy();
  Object? error;
  var saving = false;
  return showDialog<ChatStatus>(
    context: context,
    builder: (context) => StatefulBuilder(
      builder: (context, update) => AlertDialog(
        title: const Text('MODEL AND EFFORT'),
        content: SizedBox(
          width: 420,
          child: SingleChildScrollView(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                Text(
                  'Effective: ${sleeve.chat.model} / ${sleeve.chat.reasoningEffort.isEmpty ? "unknown" : sleeve.chat.reasoningEffort}',
                ),
                const Text(
                  'Applies between turns in the same native session. No restart.',
                ),
                ModelSelectionFields(
                  client: client,
                  harness: sleeve.harness,
                  sleeve: sleeve.name,
                  defaults: sleeve.chat.settings.nativeDefaults,
                  selection: selection,
                  onChanged: (value) => update(() => selection = value),
                ),
                if (error != null) Text('$error'),
              ],
            ),
          ),
        ),
        actions: [
          TextButton(
            onPressed: saving ? null : () => Navigator.pop(context),
            child: const Text('CANCEL'),
          ),
          FilledButton(
            onPressed: saving || sleeve.harness != Harness.HARNESS_CODEX
                ? null
                : () async {
                    update(() {
                      saving = true;
                      error = null;
                    });
                    try {
                      final result = await client.chat.setSelection(
                        SetSelectionRequest(
                          sleeve: sleeve.name,
                          selection: selection,
                        ),
                      );
                      if (context.mounted) Navigator.pop(context, result.chat);
                    } on Object catch (e) {
                      if (context.mounted) {
                        update(() {
                          error = e;
                          saving = false;
                        });
                      }
                    }
                  },
            child: const Text('APPLY'),
          ),
        ],
      ),
    ),
  );
}
