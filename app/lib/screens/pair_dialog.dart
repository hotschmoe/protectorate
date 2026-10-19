import 'package:flutter/material.dart';

import '../hubs/pairing.dart';
import '../theme/tokens.dart';
import '../vfd/panel.dart';

Future<bool> confirmPairing(
  BuildContext context,
  PairingRequest request, {
  required bool update,
}) async {
  final ok = await showDialog<bool>(
    context: context,
    builder: (context) => AlertDialog(
      title: Text(update ? 'Update hub?' : 'Add hub?'),
      content: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text('HUB', style: Tokens.of(context).legend()),
          const SizedBox(height: 4),
          Text(request.name, style: Tokens.of(context).title()),
          const SizedBox(height: 4),
          SelectableText(
            request.baseUrl,
            style: Tokens.of(context).data(Tokens.of(context).p.info),
          ),
          const SizedBox(height: 14),
          request.token.isEmpty
              ? NoteStrip('This link carries no token.', tone: Tone.info)
              : const NoteStrip(
                  'This link grants access to the hub. Only continue if you trust where it came from.',
                ),
        ],
      ),
      actions: [
        TextButton(
          key: const ValueKey('pair-cancel'),
          onPressed: () => Navigator.of(context).pop(false),
          child: const Text('CANCEL'),
        ),
        FilledButton(
          key: const ValueKey('pair-confirm'),
          onPressed: () => Navigator.of(context).pop(true),
          child: Text(update ? 'UPDATE AND OPEN' : 'ADD AND OPEN'),
        ),
      ],
    ),
  );
  return ok ?? false;
}
