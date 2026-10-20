import 'package:connectrpc/connect.dart';
import 'package:flutter/material.dart';

import '../gen/protectorate/v1/sleeve.pb.dart';
import '../hubs/hub_client.dart';
import '../mail/thread_view.dart' show orchestratorSleeve;
import '../theme/tokens.dart';

class PhoneDevicesTab extends StatefulWidget {
  const PhoneDevicesTab({
    super.key,
    required this.client,
    required this.devices,
  });

  final HubClient client;
  final Widget devices;

  @override
  State<PhoneDevicesTab> createState() => _PhoneDevicesTabState();
}

class _PhoneDevicesTabState extends State<PhoneDevicesTab> {
  final _abort = CancelableSignal();
  bool _stopping = false;
  String? _result;
  bool _failed = false;

  @override
  void dispose() {
    _abort.cancel();
    super.dispose();
  }

  Future<void> _stop() async {
    final confirmed = await showModalBottomSheet<bool>(
      context: context,
      showDragHandle: true,
      isScrollControlled: true,
      builder: (context) {
        final t = Tokens.of(context);
        return SafeArea(
          top: false,
          child: SingleChildScrollView(
            padding: const EdgeInsets.fromLTRB(20, 0, 20, 20),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Text('STOP ALL SLEEVES?', style: t.title(t.p.alarm)),
                const SizedBox(height: 12),
                Text(
                  'Stops every sleeve except Envoy. Running work is interrupted. '
                  'This works even when Envoy is unavailable.',
                  style: t.body(),
                ),
                const SizedBox(height: 20),
                FilledButton(
                  key: const ValueKey('stop-all-confirm'),
                  style: FilledButton.styleFrom(
                    backgroundColor: t.p.alarm,
                    foregroundColor: t.p.onKey,
                    minimumSize: const Size.fromHeight(48),
                  ),
                  onPressed: () => Navigator.pop(context, true),
                  child: const Text('STOP ALL SLEEVES'),
                ),
                TextButton(
                  onPressed: () => Navigator.pop(context, false),
                  child: const Text('CANCEL'),
                ),
              ],
            ),
          ),
        );
      },
    );
    if (confirmed != true || !mounted) return;
    setState(() {
      _stopping = true;
      _result = null;
      _failed = false;
    });
    var stopped = 0;
    final failures = <String>[];
    try {
      final list = await widget.client.sleeves.listSleeves(
        ListSleevesRequest(),
        signal: TimeoutSignal(const Duration(seconds: 10), parent: _abort),
      );
      final targets = list.sleeves
          .where((sleeve) => sleeve.name != orchestratorSleeve)
          .toList();
      var next = 0;
      Future<void> stopNext() async {
        while (mounted && next < targets.length) {
          final sleeve = targets[next++];
          try {
            await widget.client.sleeves.killSleeve(
              KillSleeveRequest(name: sleeve.name),
              signal: TimeoutSignal(
                const Duration(seconds: 20),
                parent: _abort,
              ),
            );
            stopped++;
          } on Object catch (e) {
            failures.add('${sleeve.name}: $e');
          }
        }
      }

      await Future.wait([
        for (var worker = 0; worker < 4 && worker < targets.length; worker++)
          stopNext(),
      ]);
      if (mounted) {
        setState(() {
          _failed = failures.isNotEmpty;
          _result = [
            stopped == 0 && failures.isEmpty
                ? 'No sleeves to stop. Envoy is kept.'
                : 'Stopped $stopped sleeves. Envoy is kept.',
            if (failures.isNotEmpty) 'Could not stop ${failures.join('; ')}',
          ].join('\n');
        });
      }
    } on Object catch (e) {
      if (mounted) {
        setState(() {
          _failed = true;
          _result = 'Could not list sleeves to stop: $e';
        });
      }
    } finally {
      if (mounted) setState(() => _stopping = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Expanded(child: widget.devices),
        Container(
          decoration: BoxDecoration(
            color: t.p.frame,
            border: Border(top: BorderSide(color: t.p.bezel)),
          ),
          padding: const EdgeInsets.all(Tokens.gap),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              if (_result != null)
                ConstrainedBox(
                  constraints: const BoxConstraints(maxHeight: 100),
                  child: SingleChildScrollView(
                    child: Padding(
                      padding: const EdgeInsets.only(bottom: 8),
                      child: Text(
                        _result!,
                        style: t.small(_failed ? t.p.alarm : t.p.ok),
                      ),
                    ),
                  ),
                ),
              OutlinedButton.icon(
                key: const ValueKey('stop-all-sleeves'),
                style: OutlinedButton.styleFrom(
                  foregroundColor: t.p.alarm,
                  side: BorderSide(color: t.p.alarm),
                  minimumSize: const Size.fromHeight(48),
                ),
                onPressed: _stopping ? null : _stop,
                icon: const Icon(Icons.stop_circle_outlined),
                label: Text(
                  _stopping ? 'STOPPING SLEEVES...' : 'STOP ALL SLEEVES',
                ),
              ),
            ],
          ),
        ),
      ],
    );
  }
}
