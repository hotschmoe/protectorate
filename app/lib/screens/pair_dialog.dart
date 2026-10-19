import 'package:flutter/material.dart';

import '../hubs/pairing.dart';

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
          Text(request.name, style: Theme.of(context).textTheme.titleMedium),
          const SizedBox(height: 4),
          SelectableText(request.baseUrl),
          const SizedBox(height: 12),
          Text(
            request.token.isEmpty
                ? 'This link carries no token.'
                : 'This link grants access to the hub. Only continue if you trust where it came from.',
          ),
        ],
      ),
      actions: [
        TextButton(
          key: const ValueKey('pair-cancel'),
          onPressed: () => Navigator.of(context).pop(false),
          child: const Text('Cancel'),
        ),
        FilledButton(
          key: const ValueKey('pair-confirm'),
          onPressed: () => Navigator.of(context).pop(true),
          child: Text(update ? 'Update and open' : 'Add and open'),
        ),
      ],
    ),
  );
  return ok ?? false;
}
