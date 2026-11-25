import 'package:flutter/material.dart';

import 'api/api.g.dart';
import 'hub.dart';
import 'icons.dart';
import 'theme.dart';
import 'widgets.dart';

typedef ModelSelection = ({String? model, String? provider, String? effort});
const defaultModelSelection = (model: null, provider: null, effort: null);

class ModelSelectionFields extends StatefulWidget {
  const ModelSelectionFields({
    super.key,
    required this.hub,
    required this.harness,
    required this.onChanged,
    this.enabled = true,
  });

  final HubClient hub;
  final Harness harness;
  final ValueChanged<ModelSelection> onChanged;
  final bool enabled;

  @override
  State<ModelSelectionFields> createState() => _ModelSelectionFieldsState();
}

class _ModelSelectionFieldsState extends State<ModelSelectionFields> {
  final _modelText = TextEditingController();
  final _providerText = TextEditingController();
  final _effortText = TextEditingController();
  late final _catalog = widget.hub.listModels(widget.harness);
  static const _default = -1;
  static const _custom = -2;
  int _choice = _default;
  String? _effort;

  @override
  void dispose() {
    _modelText.dispose();
    _providerText.dispose();
    _effortText.dispose();
    super.dispose();
  }

  String? _value(TextEditingController controller) {
    final text = controller.text.trim();
    return text.isEmpty ? null : text;
  }

  void _publish(List<ModelChoice> models) {
    final option = _choice >= 0 ? models[_choice] : null;
    final freeText = models.isEmpty || _choice == _custom;
    final model = freeText ? _value(_modelText) : option?.id;
    widget.onChanged((
      model: model,
      provider: freeText && model != null
          ? _value(_providerText)
          : option?.provider,
      effort: _effort,
    ));
  }

  @override
  Widget build(BuildContext context) {
    final p = pal(context);
    final labelStyle = ts(context);
    Text label(String text) => Text(
      text,
      style: labelStyle,
      maxLines: 1,
      overflow: TextOverflow.ellipsis,
    );
    return FutureBuilder<ModelCatalog>(
      future: _catalog,
      builder: (context, snapshot) {
        if (snapshot.connectionState != ConnectionState.done) {
          return T('Loading models…', color: p.dim, size: kSmall);
        }
        final row = snapshot.data?.harnesses.firstOrNull;
        final models = row?.models ?? const <ModelChoice>[];
        final defaultModel = models.where((m) => m.isDefault).firstOrNull;
        final option = _choice >= 0 ? models[_choice] : null;
        final freeText = models.isEmpty || _choice == _custom;
        final selected = _choice == _default ? defaultModel : option;
        final efforts = selected?.efforts ?? row?.efforts ?? const <String>[];
        final defaultEffort = _choice == _default
            ? row?.defaultEffort
            : selected?.defaultEffort;
        final showEffortText =
            efforts.isEmpty && selected?.efforts == null && freeText;
        return Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            const Label('Model'),
            const SizedBox(height: 4),
            if (models.isNotEmpty)
              DecoratedBox(
                decoration: BoxDecoration(
                  border: Border.all(color: p.line),
                  borderRadius: BorderRadius.circular(6),
                ),
                child: Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 10),
                  child: DropdownButtonHideUnderline(
                    child: DropdownButton<int>(
                      isDense: true,
                      isExpanded: true,
                      icon: Icon16(Glyph.chevronDown, color: p.dim),
                      padding: const EdgeInsets.symmetric(vertical: 8),
                      value: _choice,
                      items: [
                        DropdownMenuItem(
                          value: _default,
                          child: label(
                            defaultModel == null
                                ? 'Default'
                                : 'Default (${defaultModel.label})',
                          ),
                        ),
                        for (final (i, model) in models.indexed)
                          DropdownMenuItem(value: i, child: label(model.label)),
                        DropdownMenuItem(
                          value: _custom,
                          child: label('Enter model ID…'),
                        ),
                      ],
                      onChanged: widget.enabled
                          ? (value) {
                              if (value == null) return;
                              setState(() {
                                _choice = value;
                                _effort = null;
                                _effortText.clear();
                              });
                              _publish(models);
                            }
                          : null,
                    ),
                  ),
                ),
              ),
            if (freeText) ...[
              if (models.isNotEmpty) const SizedBox(height: 6),
              Field(
                controller: _modelText,
                hint: models.isEmpty
                    ? 'Model ID (blank for default)'
                    : 'Model ID',
                mono: true,
                enabled: widget.enabled,
                onChanged: (_) => _publish(models),
              ),
              if (widget.harness == Harness.piLocal ||
                  widget.harness == Harness.piVendor) ...[
                const SizedBox(height: 6),
                Field(
                  controller: _providerText,
                  hint: 'Provider (optional)',
                  mono: true,
                  enabled: widget.enabled,
                  onChanged: (_) => _publish(models),
                ),
              ],
            ],
            if (efforts.isNotEmpty || showEffortText) ...[
              const SizedBox(height: 8),
              const Label('Effort'),
              const SizedBox(height: 4),
              if (showEffortText)
                Field(
                  controller: _effortText,
                  hint: 'Effort (blank for default)',
                  mono: true,
                  enabled: widget.enabled,
                  onChanged: (_) {
                    _effort = _value(_effortText);
                    _publish(models);
                  },
                )
              else
                Buttons([
                  for (final value in <String?>[null, ...efforts])
                    Btn(
                      value ??
                          (defaultEffort == null
                              ? 'Default'
                              : 'Default ($defaultEffort)'),
                      dense: true,
                      active: value == _effort,
                      onTap: widget.enabled
                          ? () {
                              setState(() => _effort = value);
                              _publish(models);
                            }
                          : null,
                    ),
                ]),
            ],
          ],
        );
      },
    );
  }
}
