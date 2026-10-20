# Protectorate app

Flutter client (android, linux, windows, macos, web) for the hub. Connect-RPC over HTTP/1.1 JSON, so it works on web and mobile.

## Run

```
export PATH="$HOME/.pub-cache/bin:$PATH"
cd app
flutter pub get
flutter run -d chrome        # or -d linux / windows / macos / an Android device
flutter analyze && flutter test
flutter build web
```

## Codegen

Dart code in `lib/gen/` is generated from `proto/` by the root `make gen` (buf plugins `protoc-gen-dart` and `protoc-gen-connect-dart`) and is committed. Never hand-edit it. Tools:

```
dart pub global activate protoc_plugin
dart pub global activate connectrpc
```

Do not pass the `grpc` option to `protoc-gen-dart`.

## Hub auth

- RPC: `Authorization: Bearer <token>` (web needs the hub to answer CORS preflight allowing that header).
- Terminal WebSocket `ws(s)://<hub>/ws/terminal/<name>`: offered `Sec-WebSocket-Protocol` entries are `protectorate.v1` and `token.<value>` (browsers cannot set headers). The server selects `protectorate.terminal.v1`.
- Frames: binary; `0x30` + bytes for data, `0x31` + JSON `{"cols":N,"rows":M}` for resize.

## Pairing

`protectorate pair [--url <public base url>] [--name <label>] [--app]` on the hub machine prints a QR code and two links:

- web: `<base>/#/pair?name=..&token=..` (served by `hub serve --web`; the token stays in the URL fragment and is never sent to a server)
- app: `protectorate://pair?url=<base>&name=..&token=..`

Either link opens a confirm dialog with the hub name and URL; confirming saves the profile and opens it. Optional `tab=overview|nodes|quota|sleeves|backlog`, `session=<name>` (a raw tmux session to open) and `node=<name>` parameters pick the landing screen. The `confirm=auto` parameter skips the dialog only in builds made with `--dart-define=E2E_AUTOCONFIRM=true` (used by `scripts/e2e-screens.sh`).

The credential in the link is a scoped device token, not the owner token. `pair` mints it through `TokenService` (so it needs the owner token) with scopes `read`, `events`, `terminal`, `session_*`, `sleeve_*`, `chat_*` and `backlog_*`, a 90 day lifetime (`--ttl` changes it) and the label `pair: <name>`. It cannot manage tokens or touch nodes. A lost phone is handled with `protectorate token revoke <id>`; `pair` prints the id. The app should treat `permission_denied` as "this device lacks that scope", and `unauthenticated` as "expired or revoked, pair again".

Native handling uses the `app_links` package. Android registers an intent filter for `protectorate://pair` in `AndroidManifest.xml`. Untested: Android (no device or SDK here), Windows/Linux/macOS scheme registration (the OS must launch the app with the URL; no installer registers the scheme yet), iOS (not scaffolded).

## Screenshots

`scripts/e2e-screens.sh` builds the web app, runs a throwaway hub with demo data and screenshots every main screen at 412x860 and 1280x800 into `e2e-out/`, each in light and dark (`E2E_ONLY=<name glob>` limits the pass). A fake unreachable node `ghost` is added to show that styling.

## Known limitations

- `flutter build apk` fails on linux/arm64 hosts: the Android SDK CMake/NDK binaries are x86_64 only. Build Android on an x86_64 host.
- Hub tokens are stored in shared_preferences (plain text on disk).
- The terminal uses the bundled JetBrains Mono (`assets/fonts`, OFL) because web canvas text cannot use system fonts.
