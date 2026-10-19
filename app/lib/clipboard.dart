import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'clipboard_io.dart' if (dart.library.js_interop) 'clipboard_web.dart';

typedef SetData = Future<void> Function(String text);
typedef LegacyCopy = bool Function(String text);

// Browsers expose navigator.clipboard only in secure contexts, so on plain
// http the async API is skipped and execCommand is tried instead.
Future<bool> copyWithFallback(
  String text, {
  required bool secureContext,
  required SetData setData,
  required LegacyCopy legacyCopy,
}) async {
  if (secureContext) {
    try {
      await setData(text);
      return true;
    } catch (_) {}
  }
  try {
    return legacyCopy(text);
  } catch (_) {
    return false;
  }
}

Future<bool> copyText(BuildContext context, String text) async {
  final messenger = ScaffoldMessenger.of(context);
  final ok = await copyWithFallback(
    text,
    secureContext: isSecureContext,
    setData: (s) => Clipboard.setData(ClipboardData(text: s)),
    legacyCopy: legacyCopy,
  );
  if (ok) {
    messenger.showSnackBar(
      const SnackBar(
        content: Text('Copied to clipboard'),
        duration: Duration(seconds: 2),
      ),
    );
    return true;
  }
  if (!context.mounted) return false;
  await showDialog<void>(
    context: context,
    builder: (_) => AlertDialog(
      key: const ValueKey('copy-manual'),
      title: const Text('Select and copy (this page is not https)'),
      content: TextField(
        controller: TextEditingController(text: text),
        readOnly: true,
        autofocus: true,
        maxLines: null,
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(),
          child: const Text('CLOSE'),
        ),
      ],
    ),
  );
  return false;
}
