import 'package:web/web.dart' as web;

bool get isSecureContext => web.window.isSecureContext;

bool legacyCopy(String text) {
  final area = web.document.createElement('textarea') as web.HTMLTextAreaElement
    ..value = text
    ..setAttribute('readonly', '')
    ..style.position = 'fixed'
    ..style.opacity = '0'
    ..style.left = '-9999px';
  web.document.body!.append(area);
  try {
    area.select();
    return web.document.execCommand('copy');
  } finally {
    area.remove();
  }
}
