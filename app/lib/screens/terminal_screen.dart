import 'dart:async';
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:web_socket_channel/web_socket_channel.dart';
import 'package:xterm/xterm.dart';

import '../hubs/hub_profile.dart';
import '../terminal/compose_box.dart';
import '../terminal/extra_keys.dart';
import '../terminal/terminal_protocol.dart';

class TerminalScreen extends StatefulWidget {
  const TerminalScreen({super.key, required this.hub, required this.session});

  final HubProfile hub;
  final String session;

  @override
  State<TerminalScreen> createState() => _TerminalScreenState();
}

class _TerminalScreenState extends State<TerminalScreen> {
  final _terminal = Terminal(maxLines: 10000);
  final _modifiers = ModifierState();
  final _decoder = const Utf8Decoder(allowMalformed: true);
  WebSocketChannel? _channel;
  StreamSubscription<dynamic>? _sub;
  String _status = 'connecting';
  int _cols = 80;
  int _rows = 24;

  @override
  void initState() {
    super.initState();
    _terminal.onOutput = (data) => _send(_modifiers.applyToText(data));
    _terminal.onResize = (cols, rows, _, _) {
      _cols = cols;
      _rows = rows;
      _sendFrame(encodeResize(cols, rows));
    };
    _connect();
  }

  void _connect() {
    final uri = widget.hub.terminalUri(widget.session);
    final channel = WebSocketChannel.connect(
      uri,
      protocols: terminalSubprotocols(widget.hub.token),
    );
    _channel = channel;
    channel.ready.then((_) {
      if (!mounted) return;
      setState(() => _status = 'connected');
      _sendFrame(encodeResize(_cols, _rows));
    }).catchError((Object e) {
      if (mounted) setState(() => _status = 'failed: $e');
    });
    _sub = channel.stream.listen(
      (frame) {
        final data = decodeData(frame);
        if (data != null) _terminal.write(_decoder.convert(data));
      },
      onError: (Object e) {
        if (mounted) setState(() => _status = 'error: $e');
      },
      onDone: () {
        if (mounted) setState(() => _status = 'closed');
      },
    );
  }

  void _sendFrame(List<int> frame) {
    if (_status == 'connected') _channel?.sink.add(frame);
  }

  void _send(String text) => _sendFrame(encodeData(text));

  void _sendCompose(String text) {
    _terminal.paste(text);
    _send('\r');
  }

  @override
  void dispose() {
    _sub?.cancel();
    _channel?.sink.close();
    _modifiers.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: Text(widget.session),
        actions: [
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 16),
            child: Center(child: Text(_status)),
          ),
        ],
      ),
      body: SafeArea(
        child: Column(
          children: [
            Expanded(child: TerminalView(_terminal, autofocus: true)),
            ExtraKeysBar(
              modifiers: _modifiers,
              onInput: _send,
            ),
            ComposeBox(onSend: _sendCompose),
          ],
        ),
      ),
    );
  }
}
