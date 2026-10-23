# Protectorate app

Flutter client (android, linux, windows, macos, web) for the hub. Connect-RPC over HTTP/1.1 JSON, so it works on web and mobile.

## Run

```
export PATH="$HOME/.pub-cache/bin:$PATH"
cd app
flutter pub get
flutter run -d chrome        # or -d linux / windows / macos / an Android device
flutter analyze
dart run tool/build_web.dart
```

## Codegen

Dart code in `lib/gen/` is generated from `proto/` by the root `make gen` (buf plugins `protoc-gen-dart` and `protoc-gen-connect-dart`) and is committed. Never hand-edit it. Tools:

```
dart pub global activate protoc_plugin
dart pub global activate connectrpc
```

Do not pass the `grpc` option to `protoc-gen-dart`.

## Offline web release

`dart run tool/build_web.dart` forwards Flutter build flags and prunes
`build/web/canvaskit` to `canvaskit.js` and `canvaskit.wasm`. Use this entry
point for release builds. The release archive, installer and `hub serve --web`
consume that directory without changing its contents.
The Rust hub serves the verified current release's extracted `web` directory on
the API listener. For a local build, start `protectorate-rs serve ... --web
app/build/web`; the Flutter app and Connect RPCs then share an origin. An
explicit `--web` directory must contain `index.html`.

The bootstrap pins the full CanvasKit renderer for Chromium, Firefox and Safari,
with engine and fallback-font URLs resolved against the document base URL.
Chromium-only CanvasKit, skwasm, Wimp, experimental renderers and debug symbols
are excluded. Roboto is bundled (Apache 2.0) alongside the existing UI fonts,
so Flutter does not fetch its default font from Google at startup. Glyphs absent
from the bundled fonts need additional fonts hosted under `web/fonts/` using
Flutter's fallback-font URL layout; they never fall back to an external CDN.

Measured with Flutter 3.44.1 on 2026-10-20 (sum of file sizes, same release
before/after pruning): web output 44,516,126 -> 13,356,836 bytes; engine assets
38,475,613 -> 7,316,326 bytes; gzip release tarball 15,014,711 -> 4,668,712 bytes.
Manual Chromium verification served the pruned output over loopback HTTP at
412x860, blocked all off-origin requests and waited for `flutter-first-frame`:
the hub picker rendered with zero off-origin requests, failed responses or
JavaScript errors. Both engine requests used the local full CanvasKit pair.

## Terminal dependency

The app uses MIT-licensed [xterm2 5.3.0](https://pub.dev/packages/xterm2/versions/5.3.0),
pinned to the reviewed release. It replaces xterm without adding a second terminal
implementation. Its `characters` dependency was already present through Flutter.

Reviewed on 2026-10-20:

- [xterm](https://pub.dev/packages/xterm/versions/4.0.0): last release 2024-02-27,
  MIT; upstream last pushed 2025-06-19, with 109 open issues/PRs. It analyzes on
  Flutter 3.44.1, but unresolved reports include Android keyboard/composition
  problems and [Flutter 3.44 Windows input](https://github.com/TerminalStudio/xterm.dart/issues/228).
- [xterm2](https://github.com/SoFluffyOS/xterm2): 5.3.0 published 2026-10-12,
  last pushed 2026-10-15, zero open issues/PRs at review. The published source
  supplies the Flutter view ID to `TextInputConfiguration` and includes parser,
  resize, paste and IME fixes. Compilation does not replace phone-device testing.
- [xterm3](https://github.com/klc/xterm3): latest 6.3.7 published 2026-10-08;
  its AGPL-3.0 license rules it out.

The remaining direct dependencies all have callers: connectrpc/protobuf/fixnum
serve the generated API, shared_preferences stores hub profiles, web_socket_channel
supports browser/native terminal sockets, app_links handles native pairing links,
and qr renders node enrollment codes. None can be dropped without removing behavior
or reimplementing it.

## Hub auth

- RPC: `Authorization: Bearer <token>` (web needs the hub to answer CORS preflight allowing that header).
- Terminal WebSocket `ws(s)://<hub>/ws/terminal/<name>`: offered `Sec-WebSocket-Protocol` entries are `protectorate.v1` and `token.<value>` (browsers cannot set headers). The server selects `protectorate.terminal.v1`.
- Frames: binary; `0x30` + bytes for data, `0x31` + JSON `{"cols":N,"rows":M}` for resize.

## Pairing

`protectorate pair [--url <LAN base url>] [--app]` on the hub machine prints a QR code and links containing only the hub URL:

- web: `<base>/#/pair?url=<encoded base>` (served by `hub serve --web`)
- app: `protectorate://pair?url=<encoded base>`

Either link opens the connect dialog with the hub URL filled in. You can also enter the URL under ADD HUB. Bare addresses default to HTTPS; specify `http://` for a hub without TLS and include `:7470` when using its default LAN port. Name this device, press CONNECT, then compare the large code with the request on a trusted device before approving. For the first device, run `protectorate devices approve <code>` on the hub. Denied or expired requests have a TRY AGAIN button. No link can bypass approval or carry a token; old token links are rejected.

Approval issues a long-lived Full device token, including device approval and revocation but excluding owner-only operations. The Devices screen shows platform, approver and last use, and supports rename and revoke. Revocation closes live streams immediately. Existing paired device profiles continue to work. Optional tokenless `tab`, `session` and `node` parameters still select a screen when the hub is already connected.

Native handling uses the `app_links` package. Android registers an intent filter for `protectorate://pair` in `AndroidManifest.xml`. Untested: Android (no device or SDK here), Windows/Linux/macOS scheme registration (the OS must launch the app with the URL; no installer registers the scheme yet), iOS (not scaffolded).

## Screenshots

`scripts/e2e-screens.sh` builds the web app, runs a throwaway hub with demo data and screenshots every main screen at 412x860 and 1280x800 into `e2e-out/`, each in light and dark (`E2E_ONLY=<name glob>` limits the pass). A fake unreachable node `ghost` is added to show that styling.

## Known limitations

- `flutter build apk` fails on linux/arm64 hosts: the Android SDK CMake/NDK binaries are x86_64 only. Build Android on an x86_64 host.
- Hub tokens are stored in shared_preferences (plain text on disk).
- The terminal uses the bundled JetBrains Mono (`assets/fonts`, OFL) because web canvas text cannot use system fonts.
