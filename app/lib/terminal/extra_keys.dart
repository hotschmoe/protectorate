import 'package:flutter/material.dart';

import '../theme/tokens.dart';

enum ExtraKey {
  esc('Esc', '\x1b'),
  tab('Tab', '\t'),
  up('↑', '\x1b[A'),
  down('↓', '\x1b[B'),
  right('→', '\x1b[C'),
  left('←', '\x1b[D'),
  pipe('|', '|'),
  tilde('~', '~'),
  slash('/', '/');

  const ExtraKey(this.label, this.sequence);
  final String label;
  final String sequence;
}

class ModifierState extends ChangeNotifier {
  bool _ctrl = false;

  bool get ctrl => _ctrl;

  void toggleCtrl() {
    _ctrl = !_ctrl;
    notifyListeners();
  }

  String applyToKey(ExtraKey key) {
    final out = key.sequence;
    _consume();
    return out;
  }

  /// Applies the sticky Ctrl to typed text; only a single ASCII char maps.
  String applyToText(String text) {
    if (!_ctrl) return text;
    _consume();
    if (text.length != 1) return text;
    final c = text.codeUnitAt(0);
    if (c >= 0x40 && c <= 0x7e) return String.fromCharCode(c & 0x1f);
    if (c == 0x20) return '\x00';
    return text;
  }

  void _consume() {
    if (_ctrl) {
      _ctrl = false;
      notifyListeners();
    }
  }
}

class ExtraKeysBar extends StatelessWidget {
  const ExtraKeysBar({
    super.key,
    required this.modifiers,
    required this.onInput,
  });

  final ModifierState modifiers;
  final ValueChanged<String> onInput;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    return Material(
      color: t.p.frame,
      shape: Border(top: BorderSide(color: t.p.bezel)),
      child: SingleChildScrollView(
        scrollDirection: Axis.horizontal,
        child: ListenableBuilder(
          listenable: modifiers,
          builder: (context, _) => Row(
            children: [
              _KeyButton(
                label: 'Ctrl',
                selected: modifiers.ctrl,
                onPressed: modifiers.toggleCtrl,
              ),
              for (final key in ExtraKey.values)
                _KeyButton(
                  label: key.label,
                  onPressed: () => onInput(modifiers.applyToKey(key)),
                ),
            ],
          ),
        ),
      ),
    );
  }
}

class _KeyButton extends StatelessWidget {
  const _KeyButton({
    required this.label,
    required this.onPressed,
    this.selected = false,
  });

  final String label;
  final VoidCallback onPressed;
  final bool selected;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 3, vertical: 6),
      child: OutlinedButton(
        key: ValueKey('key-$label'),
        style: OutlinedButton.styleFrom(
          minimumSize: const Size(48, 40),
          padding: const EdgeInsets.symmetric(horizontal: 10),
          backgroundColor: selected ? t.tint(t.p.info) : t.p.glass,
          foregroundColor: selected ? t.p.info : t.p.lit,
          side: BorderSide(color: selected ? t.p.info : t.p.bezelHi),
        ),
        onPressed: onPressed,
        child: Text(label.toUpperCase()),
      ),
    );
  }
}
