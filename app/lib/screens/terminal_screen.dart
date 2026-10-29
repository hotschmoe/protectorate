import 'dart:async';
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:web_socket_channel/web_socket_channel.dart';
import 'package:xterm2/xterm.dart';

import '../hubs/hub_client.dart';
import '../hubs/connection.dart';
import '../hubs/http_client_io.dart'
    if (dart.library.js_interop) '../hubs/http_client_web.dart';
import '../hubs/hub_profile.dart';
import '../terminal/compose_box.dart';
import '../terminal/extra_keys.dart';
import '../terminal/terminal_style.dart';
import '../terminal/terminal_protocol.dart';
import '../theme/tokens.dart';
import '../vfd/lamp.dart';
import '../vfd/panel.dart';
import 'history_screen.dart';

class TerminalScreen extends StatefulWidget {
  const TerminalScreen({
    super.key,
    required this.hub,
    required this.client,
    required this.session,
  });

  final HubProfile hub;
  final HubClient client;
  final String session;

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

  // Devices may type into host sessions, but the hub still refuses a
  // read-write handshake to any caller without terminal:host.
  bool get _hostSession => !widget.session.startsWith('sleeve-');

  void _connect() {
    final channel = connectWebSocket(
      widget.hub.terminalUri(widget.session, readOnly: _readOnly),
      const [terminalSubprotocol],
      widget.hub.token,
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
            if (_hostSession && !_readOnly && '$e'.contains('403')) {
              _status = 'read-write refused';
              _offerReadOnly = true;
            } else {
              _status = hubConnectionMessage(e, widget.hub.baseUrl);
            }
          });
        });
    _sub = channel.stream.listen(
      (frame) {
        final data = decodeData(frame);
        if (data != null) _terminal.write(_decoder.convert(data));
      },
      onError: (Object e) {
        if (mounted) {
          setState(() => _status = hubConnectionMessage(e, widget.hub.baseUrl));
        }
      },
      onDone: () {
        if (mounted && _status == 'connected') {
          setState(() => _status = 'closed');
        }
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
              child: LampLegend(
                key: const ValueKey('terminal-status'),
                label: (_readOnly ? '$_status  read-only' : _status)
                    .toUpperCase(),
                tone: switch (_status) {
                  'connected' => _readOnly ? Tone.info : Tone.ok,
                  'connecting' => Tone.info,
                  'closed' => Tone.dim,
                  _ => Tone.alarm,
                },
              ),
            ),
          ),
        ],
      ),
      body: SafeArea(
        child: Column(
          children: [
            if (_offerReadOnly)
              Padding(
                padding: const EdgeInsets.all(Tokens.gap),
                child: Row(
                  children: [
                    const Expanded(
                      child: NoteStrip(
                        'Typing into a session outside a sleeve needs a device '
                        'token or a capability with terminal:host. You can '
                        'still watch it.',
                      ),
                    ),
                    const SizedBox(width: 8),
                    TextButton(
                      onPressed: _reconnectReadOnly,
                      child: const Text('VIEW READ-ONLY'),
                    ),
                  ],
                ),
              ),
            Expanded(
              child: Container(
                color: Tokens.of(context).p.glass,
                padding: const EdgeInsets.all(6),
                child: TerminalView(
                  _terminal,
                  autofocus: true,
                  textStyle: terminalTextStyle,
                  theme: terminalTheme(context),
                ),
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
