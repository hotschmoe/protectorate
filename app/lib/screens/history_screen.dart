import 'package:flutter/material.dart';
import 'package:xterm/xterm.dart';

import '../gen/protectorate/v1/session.pb.dart';
import '../hubs/hub_client.dart';
import '../terminal/terminal_style.dart';
import '../theme/tokens.dart';
import '../vfd/lamp.dart';
import '../vfd/panel.dart';

class HistoryScreen extends StatefulWidget {
  const HistoryScreen({super.key, required this.client, required this.session});

  final HubClient client;
  final String session;

  @override
  State<HistoryScreen> createState() => _HistoryScreenState();
}

class _HistoryScreenState extends State<HistoryScreen> {
  static const _step = 500;

  int _lines = _step;
  String? _error;
  bool _loading = false;
  Terminal _terminal = Terminal(maxLines: 1);

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    setState(() => _loading = true);
    try {
      final r = await widget.client.sessions.capturePane(
        CapturePaneRequest(name: widget.session, lines: _lines),
      );
      final text = r.text.replaceAll('\r\n', '\n').replaceAll('\n', '\r\n');
      final lineCount = '\n'.allMatches(r.text).length + 1;
      final t = Terminal(maxLines: lineCount + 100)
        ..resize(200, 50)
        ..write(text);
      if (mounted) {
        setState(() {
          _terminal = t;
          _error = null;
        });
      }
    } on Object catch (e) {
      if (mounted) setState(() => _error = '$e');
    } finally {
      if (mounted) setState(() => _loading = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: Text('${widget.session} history'),
        actions: [
          TextButton(
            key: const ValueKey('load-more'),
            onPressed: _loading
                ? null
                : () {
                    _lines += _step;
                    _load();
                  },
            child: const Text('LOAD MORE'),
          ),
        ],
      ),
      body: Column(
        children: [
          if (_loading) const BusyBar(),
          if (_error != null)
            Padding(
              padding: const EdgeInsets.all(Tokens.gap),
              child: NoteStrip(_error!, tone: Tone.alarm),
            ),
          Expanded(
            child: Container(
              color: Tokens.of(context).p.glass,
              padding: const EdgeInsets.all(6),
              child: TerminalView(
                _terminal,
                readOnly: true,
                textStyle: terminalTextStyle,
                theme: terminalTheme(context),
              ),
            ),
          ),
        ],
      ),
    );
  }
}
