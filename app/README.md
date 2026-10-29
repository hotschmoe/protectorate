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
and crypto derives the device id of the app's own token. None can be dropped without removing behavior
or reimplementing it.

## Hub auth

- Each hub profile holds one bearer token, normally a device token (`pd1.`) minted by `DeviceService.AddDevice`. It is stored with the profile in shared_preferences (localStorage for the hub's origin on the web).
- RPC: `Authorization: Bearer <token>` on every call. HTTPS uses the platform's public roots; the LAN hub sits behind a proxy with a public certificate.
- Terminal WebSocket `ws(s)://<hub>/ws/terminal/<name>`: native builds send the Authorization header; browsers cannot set headers, so the web build offers `bearer.<token>` next to `protectorate.v1`. The server selects `protectorate.v1`.
- Frames: binary; `0x30` + bytes for data, `0x31` + JSON `{"cols":N,"rows":M}` for resize.

## Pairing

`protectorate devices add NAME --url <hub base url> [--app]` on the hub machine mints a device token and prints it with a QR code and these links:

- web: `<base>/#/pair?url=<encoded base>&token=<token>` (served by `hub serve --web`)
- app: `protectorate://pair?url=<encoded base>&token=<token>`

DEVICES > ADD DEVICE in a connected app does the same: it asks for a name and shows the token and both links, each with a copy button. The token is shown only once.

Opening either link checks the token with the hub, saves the profile and opens the hub; the web build first removes the token from the address bar. Under ADD HUB you can paste a link, or a hub URL and a token, and give the hub a label. Bare addresses default to HTTPS; specify `http://` for a hub without TLS. A rejected token leaves the dialog open with a TRY AGAIN button.

A device token has Full access, including adding and revoking devices but excluding owner-only operations. The Devices screen marks this device, shows when each device was added, by whom and when it was last used, and supports rename and revoke. Revocation closes live streams immediately. Optional `tab` and `session` parameters still select a screen when the hub is already connected.

Native handling uses the `app_links` package. Android registers an intent filter for `protectorate://pair` in `AndroidManifest.xml`. Untested: Android (no device or SDK here), Windows/Linux/macOS scheme registration (the OS must launch the app with the URL; no installer registers the scheme yet), iOS (not scaffolded).

## Known limitations

- `flutter build apk` fails on linux/arm64 hosts: the Android SDK CMake/NDK binaries are x86_64 only. Build Android on an x86_64 host.
- Hub tokens are stored in shared_preferences (plain text on disk).
- The terminal uses the bundled JetBrains Mono (`assets/fonts`, OFL) because web canvas text cannot use system fonts.
