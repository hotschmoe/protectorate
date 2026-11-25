import 'dart:js_interop';

@JS('location')
external _Location get _location;

extension type _Location._(JSObject _) implements JSObject {
  external void reload();
}

const bool reloadsFromHub = true;

String? clientAsset() => null;

void reloadPage() => _location.reload();

Future<String> downloadClient(Uri url, String? token, void Function(int got, int total) progress) =>
    throw UnsupportedError('the web app reloads from the hub');

String? installHint() => null;

Future<({String text, bool ok})?> lastInstall(String running) async => null;

Future<String> installClient(String path, String version) => throw UnsupportedError('the web app reloads from the hub');
