import 'dart:async';

import 'package:connectrpc/connect.dart';
import 'package:connectrpc/test.dart';
import 'package:fixnum/fixnum.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:protectorate/chat/chat_controller.dart';
import 'package:protectorate/chat/chat_model.dart';
import 'package:protectorate/gen/protectorate/v1/chat.connect.client.dart';
import 'package:protectorate/gen/protectorate/v1/chat.connect.spec.dart';
import 'package:protectorate/gen/protectorate/v1/chat.pb.dart';
import 'package:protectorate/gen/protectorate/v1/sleeve.pb.dart';
import 'package:protectorate/hubs/hub_client.dart';
import 'package:protectorate/hubs/hub_profile.dart';
import 'package:protectorate/screens/chat_screen.dart';
import 'package:protectorate/sleeves/sleeve_card.dart';
import 'package:protectorate/sleeves/sleeve_info.dart';

const _hub = HubProfile(id: '1', name: 'home', baseUrl: 'http://hub:7470');

ChatEvent _ev(int seq, void Function(ChatEvent) f) {
  final e = ChatEvent(seq: Int64(seq));
  f(e);
  return e;
}

ChatEvent _text(int seq, String id, String delta, {bool thinking = false}) =>
    _ev(
      seq,
      (e) => e.text = AssistantText(
        messageId: id,
        delta: delta,
        thinking: thinking,
      ),
    );

ChatEvent _state(int seq, ChatState s) =>
    _ev(seq, (e) => e.state = StateChange(state: s));

void main() {
  group('transcript folding', () {
    test('deltas share their message seq and append; a new seq starts one', () {
      final t = ChatTranscript();
      t.apply(_text(1, 'm1', 'Hel'));
      t.apply(_text(1, 'm1', 'lo'));
      t.apply(_text(2, 'm2', 'next'));
      expect(t.items.length, 2);
      expect((t.items[0] as AssistantItem).text, 'Hello');
      expect((t.items[1] as AssistantItem).text, 'next');
    });

    test('thinking and answer with the same id stay separate', () {
      final t = ChatTranscript();
      t.apply(_text(1, 'm1', 'hmm', thinking: true));
      t.apply(_text(2, 'm1', 'answer'));
      expect(t.items.map((i) => (i as AssistantItem).thinking), [true, false]);
    });

    test('a coalesced replay folds to the same transcript as the stream', () {
      final streamed = ChatTranscript()
        ..apply(_text(1, 'm', 'a'))
        ..apply(_text(1, 'm', 'b'));
      final replayed = ChatTranscript()..apply(_text(1, 'm', 'ab'));
      expect(
        (replayed.items.single as AssistantItem).text,
        (streamed.items.single as AssistantItem).text,
      );
    });

    test('tool results pair with their call by id', () {
      final t = ChatTranscript();
      t.apply(
        _ev(
          1,
          (e) => e.toolCall = ToolCall(
            id: 'c1',
            name: 'Bash',
            inputJson: '{"command":"ls"}',
          ),
        ),
      );
      t.apply(
        _ev(
          2,
          (e) => e.toolCall = ToolCall(id: 'c2', name: 'Read', inputJson: '{}'),
        ),
      );
      t.apply(
        _ev(
          3,
          (e) => e.toolResult = ToolResult(
            toolCallId: 'c1',
            output: 'a.txt',
            isError: true,
            truncated: true,
          ),
        ),
      );
      final c1 = t.items[0] as ToolItem;
      final c2 = t.items[1] as ToolItem;
      expect(t.items.length, 2);
      expect(
        [c1.done, c1.output, c1.isError, c1.truncated],
        [true, 'a.txt', true, true],
      );
      expect(c2.done, isFalse);
    });

    test('approvals drive state until resolved', () {
      final t = ChatTranscript();
      t.apply(_state(1, ChatState.CHAT_STATE_WORKING));
      t.apply(
        _ev(
          2,
          (e) => e.approval = ApprovalRequest(id: 'a', summary: 'rm -rf x'),
        ),
      );
      expect(t.state, ChatState.CHAT_STATE_AWAITING_APPROVAL);
      t.apply(
        _ev(
          3,
          (e) => e.approvalResolved = ApprovalResolved(
            id: 'a',
            allow: true,
            by: 'owner',
          ),
        ),
      );
      final a = t.items.single as ApprovalItem;
      expect([a.resolved, a.allowed, a.by], [true, true, 'owner']);
      expect(t.state, ChatState.CHAT_STATE_WORKING);
    });

    test('usage, compaction and duplicate seq', () {
      final t = ChatTranscript();
      t.apply(
        _ev(
          1,
          (e) => e.usage = UsageUpdate(
            contextTokens: Int64(900),
            contextWindow: Int64(1000),
          ),
        ),
      );
      expect(t.contextFraction, 0.9);
      expect(
        t.apply(
          _ev(
            2,
            (e) => e.compaction = Compaction(
              tokensBefore: Int64(900),
              tokensAfter: Int64(100),
              native: true,
            ),
          ),
        ),
        isTrue,
      );
      expect(t.contextTokens, 100);
      expect(t.apply(_text(2, 'x', 'dup')), isFalse);
      expect(t.items.length, 1);
    });

    test('a handoff marks where the harness changed', () {
      final t = ChatTranscript();
      t.apply(_text(1, 'm1', 'old harness'));
      t.apply(
        _ev(
          2,
          (e) => e.handoff = Handoff(
            fromHarness: 'claude',
            toHarness: 'codex',
            summarizerHarness: 'pi',
            summarizerLocal: true,
          ),
        ),
      );
      t.apply(_text(3, 'm2', 'new harness'));
      expect(t.items.map((i) => i.runtimeType), [
        AssistantItem,
        HandoffItem,
        AssistantItem,
      ]);
      expect((t.items[1] as HandoffItem).handoff.toHarness, 'codex');
    });
  });

  test('status priority', () {
    expect(
      resolveState([
        ChatState.CHAT_STATE_IDLE,
        ChatState.CHAT_STATE_WORKING,
        ChatState.CHAT_STATE_AWAITING_APPROVAL,
      ]),
      ChatState.CHAT_STATE_AWAITING_APPROVAL,
    );
    expect(
      resolveState([
        ChatState.CHAT_STATE_EXITED,
        ChatState.CHAT_STATE_FAILED,
        ChatState.CHAT_STATE_COMPACTING,
        ChatState.CHAT_STATE_WORKING,
      ]),
      ChatState.CHAT_STATE_WORKING,
    );
    expect(
      resolveState([
        ChatState.CHAT_STATE_EXITED,
        ChatState.CHAT_STATE_FAILED,
        ChatState.CHAT_STATE_COMPACTING,
      ]),
      ChatState.CHAT_STATE_COMPACTING,
    );
    expect(
      resolveState([
        ChatState.CHAT_STATE_EXITED,
        ChatState.CHAT_STATE_UNSPECIFIED,
        ChatState.CHAT_STATE_FAILED,
        ChatState.CHAT_STATE_IDLE,
      ]),
      ChatState.CHAT_STATE_IDLE,
    );
    expect(resolveState(const []), ChatState.CHAT_STATE_UNSPECIFIED);
  });

  test('local model rule', () {
    final cases = <(Sleeve, bool)>[
      (
        Sleeve(
          harness: Harness.HARNESS_PI,
          chat: ChatStatus(
            model: 'qwen3:8b',
            provider: 'vllm',
            localModel: true,
          ),
        ),
        true,
      ),
      (
        Sleeve(
          harness: Harness.HARNESS_PI,
          chat: ChatStatus(model: 'deepseek-v4', provider: 'deepseek'),
        ),
        false,
      ),
      (
        Sleeve(
          harness: Harness.HARNESS_PI,
          chat: ChatStatus(model: 'qwen3:8b'),
        ),
        false,
      ),
      (Sleeve(harness: Harness.HARNESS_SHELL), false),
    ];
    for (final (s, want) in cases) {
      expect(isLocalModel(s), want, reason: '$s');
    }
  });

  test('fence splitting and tool summaries', () {
    final segs = splitFences('before\n```dart\nvar a;\n```\nafter');
    expect(segs.map((s) => s.code), [false, true, false]);
    expect(segs[1].text, 'var a;');
    expect(splitFences('x\n```\nopen').last.code, isTrue);
    expect(toolSummary('Bash', '{"command":"ls -la"}'), 'ls -la');
    expect(toolSummary('X', 'not json'), 'not json');
  });

  group('controller', () {
    test(
      'reconnect mid-message resumes one earlier and replaces the text',
      () async {
        final after = <int>[];
        var calls = 0;
        final transport = FakeTransportBuilder().server(ChatService.watch, (
          WatchRequest r,
          ctx,
        ) async* {
          after.add(r.afterSeq.toInt());
          calls++;
          if (calls == 1) {
            yield _state(1, ChatState.CHAT_STATE_WORKING);
            yield _text(2, 'm', 'a');
            yield _text(2, 'm', 'b');
            throw ConnectException(Code.unavailable, 'drop');
          }
          yield _text(2, 'm', 'abc');
          yield _text(2, 'm', 'd');
          yield _state(3, ChatState.CHAT_STATE_IDLE);
          await Completer<void>().future;
        }).build();
        final c = ChatController(
          client: ChatServiceClient(transport),
          sleeve: 's',
          initialDelay: const Duration(milliseconds: 5),
        )..start();
        await Future<void>.delayed(const Duration(milliseconds: 200));
        c.dispose();
        expect(after, [0, 1]);
        expect(c.transcript.lastSeq, 3);
        expect((c.transcript.items.single as AssistantItem).text, 'abcd');
      },
    );

    test('dispose aborts the watch call', () async {
      AbortSignal? signal;
      final transport = FakeTransportBuilder().server(ChatService.watch, (
        WatchRequest r,
        ctx,
      ) async* {
        signal = ctx.signal;
        yield _state(1, ChatState.CHAT_STATE_IDLE);
        await Completer<void>().future;
      }).build();
      final c = ChatController(
        client: ChatServiceClient(transport),
        sleeve: 's',
      )..start();
      await Future<void>.delayed(const Duration(milliseconds: 50));
      var aborted = false;
      signal!.future.then((_) => aborted = true);
      c.dispose();
      await Future<void>.delayed(Duration.zero);
      expect(aborted, isTrue);
    });

    test('send retries keep one request_id', () async {
      final ids = <String>[];
      final transport = FakeTransportBuilder().unary(ChatService.send, (
        SendRequest r,
        ctx,
      ) {
        ids.add(r.requestId);
        if (ids.length < 3) throw ConnectException(Code.unavailable, 'net');
        return SendResponse(seq: Int64(7));
      }).build();
      final c = ChatController(
        client: ChatServiceClient(transport),
        sleeve: 's',
        sendRetryDelay: const Duration(milliseconds: 1),
      );
      await c.send('hi');
      await c.send('again');
      c.dispose();
      expect(ids.sublist(0, 3).toSet().length, 1);
      expect(ids[3], isNot(ids[0]));
    });

    test('a permanent send error is not retried', () async {
      var n = 0;
      final transport = FakeTransportBuilder().unary(ChatService.send, (
        SendRequest r,
        ctx,
      ) {
        n++;
        throw ConnectException(Code.failedPrecondition, 'busy');
      }).build();
      final c = ChatController(
        client: ChatServiceClient(transport),
        sleeve: 's',
        sendRetryDelay: const Duration(milliseconds: 1),
      );
      await expectLater(c.send('x'), throwsA(isA<ConnectException>()));
      c.dispose();
      expect(n, 1);
    });
  });

  group('chat screen', () {
    Sleeve sleeve({
      Harness h = Harness.HARNESS_CLAUDE,
      String model = 'opus',
      bool local = false,
    }) => Sleeve(
      name: 'alice',
      harness: h,
      workspace: '/w/app',
      session: 'sleeve-alice',
      state: SleeveState.SLEEVE_STATE_RUNNING,
      mode: SleeveMode.SLEEVE_MODE_CHAT,
      chat: ChatStatus(model: model, localModel: local),
    );

    testWidgets('renders the transcript and sends, stops, responds', (
      tester,
    ) async {
      tester.view.physicalSize = const Size(412, 800);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      final sent = <String>[];
      var interrupted = 0;
      final responses = <(String, bool)>[];
      final transport = FakeTransportBuilder()
          .server(ChatService.watch, (WatchRequest r, ctx) async* {
            yield _state(1, ChatState.CHAT_STATE_WORKING);
            yield _ev(2, (e) => e.user = UserMessage(text: 'fix the bug'));
            yield _text(3, 'm1', 'Looking at it.\n```\nx := 1\n```');
            yield _ev(
              4,
              (e) => e.toolCall = ToolCall(
                id: 'c1',
                name: 'Bash',
                inputJson: '{"command":"go test"}',
              ),
            );
            yield _ev(
              5,
              (e) => e.approval = ApprovalRequest(
                id: 'ap1',
                kind: 'exec',
                summary: 'go test ./...',
              ),
            );
            yield _ev(
              6,
              (e) => e.usage = UsageUpdate(
                contextTokens: Int64(50000),
                contextWindow: Int64(200000),
              ),
            );
            await Completer<void>().future;
          })
          .unary(ChatService.send, (SendRequest r, ctx) {
            sent.add('${r.text}|${r.requestId.isNotEmpty}');
            return SendResponse(seq: Int64(9));
          })
          .unary(ChatService.interrupt, (InterruptRequest r, ctx) {
            interrupted++;
            return InterruptResponse();
          })
          .unary(ChatService.respond, (RespondRequest r, ctx) {
            responses.add((r.approvalId, r.allow));
            return RespondResponse();
          })
          .build();
      await tester.pumpWidget(
        MaterialApp(
          home: ChatScreen(
            hub: _hub,
            client: HubClient.withTransport(transport),
            sleeve: sleeve(),
          ),
        ),
      );
      await tester.pump();
      await tester.pump();

      expect(find.text('fix the bug'), findsOneWidget);
      expect(find.textContaining('Looking at it.'), findsOneWidget);
      expect(find.text('x := 1'), findsOneWidget);
      expect(find.text('NEEDS APPROVAL'), findsOneWidget);
      expect(find.text('50.0k / 200k'), findsOneWidget);
      expect(find.byKey(const ValueKey('stop')), findsOneWidget);

      await tester.tap(find.byKey(const ValueKey('allow-ap1')));
      await tester.pump();
      expect(responses, [('ap1', true)]);

      await tester.tap(find.byKey(const ValueKey('stop')));
      await tester.pump();
      expect(interrupted, 1);

      await tester.enterText(find.byKey(const ValueKey('composer')), 'go on');
      await tester.tap(find.byKey(const ValueKey('send')));
      await tester.pump();
      expect(sent, ['go on|true']);
      expect(
        tester
            .widget<TextField>(find.byKey(const ValueKey('composer')))
            .controller!
            .text,
        '',
      );

      await tester.tap(find.byKey(const ValueKey('chat-menu')));
      await tester.pump(const Duration(milliseconds: 400));
      final compact = tester.widget<PopupMenuItem<Object>>(
        find.byKey(const ValueKey('menu-compact')),
      );
      expect(compact.enabled, isFalse);
    });

    testWidgets('compact enabled when idle and shows local model badge', (
      tester,
    ) async {
      final transport = FakeTransportBuilder().server(ChatService.watch, (
        WatchRequest r,
        ctx,
      ) async* {
        yield _state(1, ChatState.CHAT_STATE_IDLE);
        yield _ev(
          2,
          (e) => e.compaction = Compaction(
            tokensBefore: Int64(120000),
            tokensAfter: Int64(18000),
          ),
        );
        await Completer<void>().future;
      }).build();
      await tester.pumpWidget(
        MaterialApp(
          home: ChatScreen(
            hub: _hub,
            client: HubClient.withTransport(transport),
            sleeve: sleeve(
              h: Harness.HARNESS_PI,
              model: 'qwen3:8b',
              local: true,
            ),
          ),
        ),
      );
      await tester.pump();
      await tester.pump();
      expect(find.byKey(const ValueKey('local-model-badge')), findsOneWidget);
      expect(find.textContaining('COMPACTED 120k TO 18.0k'), findsOneWidget);
      expect(find.text('IDLE'), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('chat-menu')));
      await tester.pump(const Duration(milliseconds: 400));
      final compact = tester.widget<PopupMenuItem<Object>>(
        find.byKey(const ValueKey('menu-compact')),
      );
      expect(compact.enabled, isTrue);
    });
  });

  group('sleeve card', () {
    Future<void> pump(WidgetTester tester, Sleeve s, List<String> log) =>
        tester.pumpWidget(
          MaterialApp(
            home: Scaffold(
              body: SleeveCard(
                sleeve: s,
                now: DateTime.utc(2026, 1, 1, 2),
                onOpen: () => log.add('open'),
                onKill: () => log.add('kill'),
              ),
            ),
          ),
        );

    testWidgets('shows model, status, context, usage and local warning', (
      tester,
    ) async {
      final log = <String>[];
      await pump(
        tester,
        Sleeve(
          name: 'pi1',
          harness: Harness.HARNESS_PI,
          workspace: '/w/app',
          state: SleeveState.SLEEVE_STATE_RUNNING,
          mode: SleeveMode.SLEEVE_MODE_CHAT,
          limits: SleeveLimits(memoryBytes: Int64(4 << 30)),
          usage: SleeveUsage(
            cpuPercent: 40,
            memoryCurrentBytes: Int64(1 << 30),
          ),
          chat: ChatStatus(
            state: ChatState.CHAT_STATE_WORKING,
            model: 'qwen3:8b',
            provider: 'vllm',
            localModel: true,
            contextTokens: Int64(8000),
            contextWindow: Int64(32000),
          ),
        ),
        log,
      );
      expect(find.text('qwen3:8b'), findsOneWidget);
      expect(find.byKey(const ValueKey('local-model-badge')), findsOneWidget);
      expect(find.text('WORKING'), findsOneWidget);
      expect(find.text('8.0k / 32.0k'), findsOneWidget);
      expect(find.text('1.00 GB / 4.00 GB'), findsOneWidget);
      expect(find.text('OPEN CHAT'), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('open-pi1')));
      expect(log, ['open']);
    });

    testWidgets('a local summary is flagged on the card', (tester) async {
      Sleeve sleeve(bool local) => Sleeve(
        name: 'cx',
        harness: Harness.HARNESS_CODEX,
        workspace: '/w/app',
        state: SleeveState.SLEEVE_STATE_RUNNING,
        mode: SleeveMode.SLEEVE_MODE_CHAT,
        chat: ChatStatus(model: 'gpt', provider: 'openai'),
        handoff: Handoff(
          fromHarness: 'claude',
          toHarness: 'codex',
          summarizerHarness: local ? 'pi' : 'grok',
          summarizerLocal: local,
        ),
      );
      for (final local in [true, false]) {
        await pump(tester, sleeve(local), []);
        expect(
          find.byKey(const ValueKey('local-summary-badge')),
          local ? findsOneWidget : findsNothing,
        );
        expect(find.byKey(const ValueKey('local-model-badge')), findsNothing);
      }
    });

    testWidgets('pi on a hosted open model has no warning', (tester) async {
      await pump(
        tester,
        Sleeve(
          name: 'pi2',
          harness: Harness.HARNESS_PI,
          workspace: '/w/app',
          state: SleeveState.SLEEVE_STATE_RUNNING,
          mode: SleeveMode.SLEEVE_MODE_CHAT,
          chat: ChatStatus(model: 'deepseek-v4-pro', provider: 'deepseek'),
        ),
        [],
      );
      expect(find.text('deepseek-v4-pro'), findsOneWidget);
      expect(find.byKey(const ValueKey('local-model-badge')), findsNothing);
    });

    testWidgets('terminal-mode shell has no warning and opens terminal', (
      tester,
    ) async {
      await pump(
        tester,
        Sleeve(
          name: 'sh',
          harness: Harness.HARNESS_SHELL,
          workspace: '/w/x',
          state: SleeveState.SLEEVE_STATE_RUNNING,
        ),
        [],
      );
      expect(find.byKey(const ValueKey('local-model-badge')), findsNothing);
      expect(find.text('OPEN TERMINAL'), findsOneWidget);
      expect(find.text('RUNNING'), findsOneWidget);
    });
  });
}
