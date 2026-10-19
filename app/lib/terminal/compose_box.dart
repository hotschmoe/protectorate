import 'package:flutter/material.dart';

import '../theme/tokens.dart';

class ComposeBox extends StatefulWidget {
  const ComposeBox({super.key, required this.onSend});

  final ValueChanged<String> onSend;

  @override
  State<ComposeBox> createState() => _ComposeBoxState();
}

class _ComposeBoxState extends State<ComposeBox> {
  final _controller = TextEditingController();

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  void _send() {
    final text = _controller.text;
    if (text.isEmpty) return;
    widget.onSend(text);
    _controller.clear();
  }

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    return Container(
      color: t.p.frame,
      padding: const EdgeInsets.all(8),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.end,
        children: [
          Expanded(
            child: TextField(
              key: const ValueKey('compose-field'),
              controller: _controller,
              minLines: 2,
              maxLines: 8,
              keyboardType: TextInputType.multiline,
              style: t.body(t.p.emph),
              cursorWidth: 8,
              decoration: const InputDecoration(hintText: 'compose prompt'),
            ),
          ),
          const SizedBox(width: 8),
          Tooltip(
            message: 'Send',
            child: FilledButton(
              key: const ValueKey('compose-send'),
              onPressed: _send,
              child: const Text('SEND'),
            ),
          ),
        ],
      ),
    );
  }
}
