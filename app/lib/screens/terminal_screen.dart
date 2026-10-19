import 'dart:async';
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:web_socket_channel/web_socket_channel.dart';
import 'package:xterm/xterm.dart';

import '../gen/protectorate/v1/chat.pb.dart';
import '../hubs/hub_client.dart';
import '../hubs/hub_profile.dart';
import '../terminal/compose_box.dart';
import '../terminal/extra_keys.dart';
import '../terminal/terminal_style.dart';
import '../terminal/terminal_protocol.dart';
import 'history_screen.dart';

class TerminalScreen extends StatefulWidget {
  const TerminalScreen({
    super.key,
    required this.hub,
    required this.client,
    required this.session,
    this.sleeve,
  });

  final HubProfile hub;
  final HubClient client;
  final String session;

  // Set when this is a sleeve taken over from chat; offers the way back.
  final String? sleeve;

  @override
  State<TerminalScreen> createState() => _TerminalScreenState();
}

class _TerminalScreenState extends State<TerminalScreen>
    with WidgetsBindingObserver {
  final _terminal = Terminal(maxLines: 10000);
  final _modifiers = ModifierState();
  final _decoder = const Utf8Decoder(allowMalformed: true);
  WebSocketChannel? _channel;
  StreamSubscription<dynamic>? _sub;
  String _status = 'connecting';
  bool _readOnly = false;
  bool _offerReadOnly = false;
  int _cols = 80;
  int _rows = 24;
  bool _focused = true;

  @override
  void initState() {
    super.initState();
    _terminal.onOutput = (data) => _send(_modifiers.applyToText(data));
    _terminal.onResize = (cols, rows, _, _) {
      _cols = cols;
      _rows = rows;
      if (_focused) _sendFrame(encodeResize(cols, rows));
    };
    WidgetsBinding.instance.addObserver(this);
    _connect();
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    final focused = state == AppLifecycleState.resumed;
    if (focused && !_focused) _sendFrame(encodeResize(_cols, _rows));
    _focused = focused;
  }

  // A device token without terminal:host may only view sessions outside a
  // sleeve sandbox; the hub refuses the read-write handshake.
  bool get _hostSession => !widget.session.startsWith('sleeve-');

  void _connect() {
    final uri = widget.hub.terminalUri(widget.session, readOnly: _readOnly);
    final channel = WebSocketChannel.connect(
      uri,
      protocols: terminalSubprotocols(widget.hub.token),
    );
    _channel = channel;
    channel.ready
        .then((_) {
          if (!mounted) return;
          setState(() => _status = 'connected');
          _sendFrame(encodeResize(_cols, _rows));
        })
        .catchError((Object e) {
          if (!mounted) return;
          setState(() {
            if (_hostSession && !_readOnly) {
              _status = 'read-write refused';
              _offerReadOnly = true;
            } else {
              _status = 'failed: $e';
            }
          });
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

  void _reconnectReadOnly() {
    _sub?.cancel();
    _channel?.sink.close();
    setState(() {
      _readOnly = true;
      _offerReadOnly = false;
      _status = 'connecting';
    });
    _connect();
  }

  void _sendFrame(List<int> frame) {
    if (_status == 'connected') _channel?.sink.add(frame);
  }

  void _send(String text) => _sendFrame(encodeData(text));

  void _sendCompose(String text) {
    _terminal.paste(text);
    _send('\r');
  }

  Future<void> _backToChat() async {
    try {
      await widget.client.chat.setMode(
        SetModeRequest(
          sleeve: widget.sleeve,
          mode: SleeveMode.SLEEVE_MODE_CHAT,
        ),
      );
      if (mounted) Navigator.of(context).pop(true);
    } on Object catch (e) {
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text('$e')));
    }
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
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
          if (widget.sleeve != null)
            TextButton.icon(
              key: const ValueKey('back-to-chat'),
              onPressed: _backToChat,
              icon: const Icon(Icons.chat_bubble_outline),
              label: const Text('Back to chat'),
            ),
          IconButton(
            tooltip: 'History',
            icon: const Icon(Icons.history),
            onPressed: () => Navigator.of(context).push(
              MaterialPageRoute<void>(
                builder: (_) => HistoryScreen(
                  client: widget.client,
                  session: widget.session,
                ),
              ),
            ),
          ),
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 16),
            child: Center(
              child: Text(_readOnly ? '$_status (read-only)' : _status),
            ),
          ),
        ],
      ),
      body: SafeArea(
        child: Column(
          children: [
            if (_offerReadOnly)
              MaterialBanner(
                content: const Text(
                  'Typing into a session outside a sleeve needs a device '
                  'paired with --host-shell. You can still watch it.',
                ),
                actions: [
                  TextButton(
                    onPressed: _reconnectReadOnly,
                    child: const Text('View read-only'),
                  ),
                ],
              ),
            Expanded(
              child: TerminalView(
                _terminal,
                autofocus: true,
                textStyle: terminalTextStyle,
              ),
            ),
            if (!_readOnly) ...[
              ExtraKeysBar(modifiers: _modifiers, onInput: _send),
              ComposeBox(onSend: _sendCompose),
            ],
          ],
        ),
      ),
    );
  }
}
