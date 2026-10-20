import 'package:connectrpc/test.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:protectorate/chat/chat_model.dart';
import 'package:protectorate/gen/protectorate/v1/chat.connect.spec.dart';
import 'package:protectorate/gen/protectorate/v1/chat.pb.dart';
import 'package:protectorate/gen/protectorate/v1/sleeve.pb.dart';
import 'package:protectorate/hubs/hub_client.dart';
import 'package:protectorate/sleeves/model_selection.dart';
import 'package:fixnum/fixnum.dart';

void main() {
  final catalog = ListModelsResponse(
    supported: true,
    liveUpdate: true,
    models: [
      ModelOption(
        model: 'native-a',
        name: 'Native A',
        efforts: ['low', 'xhigh'],
        defaultEffort: 'low',
        isDefault: true,
      ),
      ModelOption(
        model: 'native-b',
        name: 'Native B',
        efforts: ['medium'],
        defaultEffort: 'medium',
      ),
    ],
  );

  testWidgets(
    'native catalog controls model-specific efforts and sends selected settings',
    (t) async {
      SetSelectionRequest? sent;
      final client = HubClient.withTransport(
        FakeTransportBuilder()
            .unary(ChatService.listModels, (ListModelsRequest r, ctx) {
              expect(r.sleeve, 'envoy');
              return catalog;
            })
            .unary(ChatService.setSelection, (SetSelectionRequest r, ctx) {
              sent = r;
              return SetSelectionResponse(
                chat: ChatStatus(
                  model: r.selection.model,
                  reasoningEffort: r.selection.effort,
                ),
              );
            })
            .build(),
      );
      final sleeve = Sleeve(
        name: 'envoy',
        harness: Harness.HARNESS_CODEX,
        chat: ChatStatus(
          model: 'native-a',
          reasoningEffort: 'low',
          settings: ModelSettings(
            requested: ModelSelection(),
            nativeDefaults: ModelSelection(model: 'native-a', effort: 'low'),
          ),
        ),
      );
      await t.pumpWidget(
        MaterialApp(
          home: Builder(
            builder: (context) => TextButton(
              onPressed: () => editModelSelection(context, client, sleeve),
              child: const Text('settings'),
            ),
          ),
        ),
      );
      await t.tap(find.text('settings'));
      await t.pumpAndSettle();
      await t.tap(find.byKey(const ValueKey('model-')));
      await t.pumpAndSettle();
      await t.tap(find.text('Native A').last);
      await t.pumpAndSettle();
      await t.tap(find.byKey(const ValueKey('effort-native-a-')));
      await t.pumpAndSettle();
      expect(find.text('xhigh'), findsOneWidget);
      expect(find.text('medium'), findsNothing);
      await t.tap(find.text('xhigh'));
      await t.pumpAndSettle();
      await t.tap(find.text('APPLY'));
      await t.pumpAndSettle();
      expect(sent?.sleeve, 'envoy');
      expect(sent?.selection.model, 'native-a');
      expect(sent?.selection.effort, 'xhigh');
    },
  );

  testWidgets(
    'unsupported harness explains native defaults without invented options',
    (t) async {
      await t.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: ModelSelectionFields(
              client: null,
              harness: Harness.HARNESS_CLAUDE,
              selection: ModelSelection(),
              onChanged: (_) {},
            ),
          ),
        ),
      );
      expect(find.textContaining('Codex chat only'), findsOneWidget);
      expect(find.byType(DropdownButtonFormField<String>), findsNothing);
    },
  );

  test('settings replay updates effective effort independently of request', () {
    final transcript = ChatTranscript();
    transcript.apply(
      ChatEvent(
        seq: Int64(1),
        settings: ModelSettings(
          requested: ModelSelection(model: 'asked', effort: 'xhigh'),
          effective: ModelSelection(model: 'routed', effort: 'low'),
        ),
      ),
    );
    expect(transcript.settings?.requested.model, 'asked');
    expect(transcript.settings?.effective.model, 'routed');
    expect(transcript.settings?.effective.effort, 'low');
  });
}
