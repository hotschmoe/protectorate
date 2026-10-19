// Renders chat and sleeve screens to PNG. Skipped unless SCREENSHOT_DIR is set.
import 'dart:async';
import 'dart:io';
import 'dart:ui' as ui;

import 'package:connectrpc/test.dart';
import 'package:fixnum/fixnum.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:protobuf/well_known_types/google/protobuf/timestamp.pb.dart';
import 'package:protectorate/gen/protectorate/v1/chat.connect.spec.dart';
import 'package:protectorate/gen/protectorate/v1/chat.pb.dart';
import 'package:protectorate/gen/protectorate/v1/sleeve.connect.spec.dart';
import 'package:protectorate/gen/protectorate/v1/sleeve.pb.dart';
import 'package:protectorate/hubs/event_feed.dart';
import 'package:protectorate/hubs/hub_client.dart';
import 'package:protectorate/hubs/hub_profile.dart';
import 'package:protectorate/gen/protectorate/v1/events.pb.dart';
import 'package:protectorate/screens/chat_screen.dart';
import 'package:protectorate/screens/sleeves_screen.dart';
import 'package:protectorate/theme/tokens.dart';

const _hub = HubProfile(id: '1', name: 'home', baseUrl: 'http://hub:7470');
final _dir = Platform.environment['SCREENSHOT_DIR'];
const _fonts =
    '/home/hotschmoe/development/flutter/bin/cache/artifacts/material_fonts';

ChatEvent _ev(int seq, void Function(ChatEvent) f) {
  final e = ChatEvent(seq: Int64(seq));
  f(e);
  return e;
}

ChatEvent _text(int seq, String id, String text, {bool thinking = false}) =>
    _ev(
      seq,
      (e) => e.text = AssistantText(
        messageId: id,
        delta: text,
        thinking: thinking,
      ),
    );

const _diff = '''--- internal/auth/validate.go
+++ internal/auth/validate.go
@@ -12,6 +12,9 @@ func Validate(u, p string) error {
 	if u == "" {
 		return ErrNoUser
 	}
+	if len(p) < 8 {
+		return ErrWeakPassword
+	}
 	return nil
 }''';

Stream<ChatEvent> _transcript() async* {
  yield _ev(
    1,
    (e) => e.state = StateChange(state: ChatState.CHAT_STATE_WORKING),
  );
  yield _ev(
    2,
    (e) => e.user = UserMessage(
      text:
          'The login form lets an empty password through. '
          'Can you fix it and run the tests?',
      from: 'owner',
    ),
  );
  yield _text(
    3,
    'm0',
    'The validator probably only checks the username.',
    thinking: true,
  );
  yield _text(4, 'm1', 'I will start with the validator.');
  yield _ev(
    5,
    (e) => e.toolCall = ToolCall(
      id: 'c1',
      name: 'Read',
      inputJson: '{"file_path":"internal/auth/validate.go"}',
    ),
  );
  yield _ev(
    6,
    (e) => e.toolResult = ToolResult(
      toolCallId: 'c1',
      output:
          'func Validate(u, p string) error {\n\tif u == "" {\n\t\treturn ErrNoUser\n\t}\n\treturn nil\n}',
    ),
  );
  yield _text(
    7,
    'm2',
    'Found it: `Validate` never looks at the password. I will require at '
        'least 8 characters:\n```go\nif len(p) < 8 {\n\treturn ErrWeakPassword\n}\n```',
  );
  yield _ev(
    8,
    (e) => e.toolCall = ToolCall(
      id: 'c2',
      name: 'Edit',
      inputJson:
          '{"file_path":"internal/auth/validate.go","old_string":"return nil","new_string":"if len(p) < 8 {...}"}',
    ),
  );
  yield _ev(
    9,
    (e) => e.toolResult = ToolResult(toolCallId: 'c2', output: _diff),
  );
  yield _ev(
    10,
    (e) => e.compaction = Compaction(
      tokensBefore: Int64(148000),
      tokensAfter: Int64(21000),
      native: true,
    ),
  );
  yield _ev(
    11,
    (e) => e.usage = UsageUpdate(
      contextTokens: Int64(24000),
      contextWindow: Int64(200000),
    ),
  );
  yield _ev(
    12,
    (e) => e.approval = ApprovalRequest(
      id: 'ap1',
      kind: 'exec',
      summary: 'go test ./internal/auth/...',
    ),
  );
  await Completer<void>().future;
}

Future<void> _loadFonts() async {
  Future<ByteData> read(String p) async =>
      ByteData.sublistView(await File(p).readAsBytes());
  final icons = FontLoader('MaterialIcons')
    ..addFont(read('$_fonts/MaterialIcons-Regular.otf'));
  await icons.load();
  final mono = FontLoader(monoFamily)
    ..addFont(read('assets/fonts/JetBrainsMonoNL-Regular.ttf'))
    ..addFont(read('assets/fonts/JetBrainsMonoNL-Medium.ttf'))
    ..addFont(read('assets/fonts/JetBrainsMonoNL-Bold.ttf'))
    ..addFont(read('assets/fonts/JetBrainsMonoNL-Italic.ttf'));
  await mono.load();
}

Future<void> _shoot(WidgetTester tester, GlobalKey key, String name) async {
  await tester.runAsync(() async {
    final boundary =
        key.currentContext!.findRenderObject()! as RenderRepaintBoundary;
    final image = await boundary.toImage(pixelRatio: 2);
    final data = await image.toByteData(format: ui.ImageByteFormat.png);
    await File('$_dir/$name.png').writeAsBytes(data!.buffer.asUint8List());
  });
}

Sleeve _sleeve(
  String name,
  Harness h,
  String model,
  ChatState st, {
  int ctx = 0,
  int win = 0,
  SleeveMode mode = SleeveMode.SLEEVE_MODE_CHAT,
  double cpu = 30,
  int memMb = 900,
}) => Sleeve(
  name: name,
  harness: h,
  workspace: '/home/me/src/$name',
  session: 'sleeve-$name',
  state: SleeveState.SLEEVE_STATE_RUNNING,
  mode: mode,
  createdAt: Timestamp.fromDateTime(
    DateTime.now().toUtc().subtract(const Duration(hours: 3, minutes: 12)),
  ),
  limits: SleeveLimits(cpuPercent: 200, memoryBytes: Int64(4 << 30)),
  usage: SleeveUsage(cpuPercent: cpu, memoryCurrentBytes: Int64(memMb << 20)),
  chat: st == ChatState.CHAT_STATE_UNSPECIFIED
      ? null
      : ChatStatus(
          state: st,
          model: model,
          contextTokens: Int64(ctx),
          contextWindow: Int64(win),
        ),
);

void main() {
  if (_dir == null) {
    test('screenshots skipped without SCREENSHOT_DIR', () {}, skip: true);
    return;
  }
  setUpAll(_loadFonts);

  for (final dark in [false, true]) {
    for (final w in [412, 1280]) {
      final tag = '$w${dark ? 'dark' : ''}';
      final b = dark ? Brightness.dark : Brightness.light;
      testWidgets('chat $tag', (tester) async {
        tester.view.physicalSize = Size(w.toDouble(), w == 412 ? 1280 : 900);
        tester.view.devicePixelRatio = 1;
        addTearDown(tester.view.reset);
        final key = GlobalKey();
        final transport = FakeTransportBuilder()
            .server(ChatService.watch, (WatchRequest r, ctx) => _transcript())
            .build();
        await tester.pumpWidget(
          RepaintBoundary(
            key: key,
            child: MaterialApp(
              debugShowCheckedModeBanner: false,
              theme: appTheme(b),
              home: ChatScreen(
                hub: _hub,
                client: HubClient.withTransport(transport),
                sleeve: _sleeve(
                  'auth-fix',
                  Harness.HARNESS_CLAUDE,
                  'claude-opus-5',
                  ChatState.CHAT_STATE_WORKING,
                ),
              ),
            ),
          ),
        );
        await tester.pump();
        await tester.pump();
        await tester.tap(find.byKey(const ValueKey('tool-c2')));
        await tester.pump(const Duration(milliseconds: 100));
        await _shoot(tester, key, 'chat-$tag');
      });

      testWidgets('sleeves $tag', (tester) async {
        tester.view.physicalSize = Size(w.toDouble(), w == 412 ? 1100 : 800);
        tester.view.devicePixelRatio = 1;
        addTearDown(tester.view.reset);
        final key = GlobalKey();
        final transport = FakeTransportBuilder()
            .unary(
              SleeveService.listSleeves,
              (ListSleevesRequest r, ctx) => ListSleevesResponse(
                sleeves: [
                  _sleeve(
                    'auth-fix',
                    Harness.HARNESS_CLAUDE,
                    'claude-opus-5',
                    ChatState.CHAT_STATE_AWAITING_APPROVAL,
                    ctx: 24000,
                    win: 200000,
                    cpu: 62,
                    memMb: 1700,
                  ),
                  _sleeve(
                    'refactor',
                    Harness.HARNESS_CODEX,
                    'gpt-5-codex',
                    ChatState.CHAT_STATE_WORKING,
                    ctx: 91000,
                    win: 272000,
                    cpu: 120,
                    memMb: 2400,
                  ),
                  _sleeve(
                    'scratch',
                    Harness.HARNESS_PI,
                    'qwen3:8b',
                    ChatState.CHAT_STATE_IDLE,
                    ctx: 7000,
                    win: 32000,
                    cpu: 8,
                    memMb: 600,
                  ),
                  _sleeve(
                    'ops',
                    Harness.HARNESS_SHELL,
                    '',
                    ChatState.CHAT_STATE_UNSPECIFIED,
                    mode: SleeveMode.SLEEVE_MODE_TERMINAL,
                    cpu: 1,
                    memMb: 90,
                  ),
                ],
              ),
            )
            .build();
        final feed = EventFeed(() => const Stream<Event>.empty());
        addTearDown(feed.dispose);
        await tester.pumpWidget(
          RepaintBoundary(
            key: key,
            child: MaterialApp(
              debugShowCheckedModeBanner: false,
              theme: appTheme(b),
              home: SleevesScreen(
                hub: _hub,
                client: HubClient.withTransport(transport),
                feed: feed,
              ),
            ),
          ),
        );
        await tester.pump();
        await tester.pump(const Duration(milliseconds: 100));
        await _shoot(tester, key, 'sleeves-$tag');
        await tester.pumpWidget(const SizedBox());
      });
    }
  }
}
