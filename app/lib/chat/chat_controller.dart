import 'dart:async';

import 'package:connectrpc/connect.dart';
import 'package:fixnum/fixnum.dart';
import 'package:flutter/foundation.dart';

import '../gen/protectorate/v1/chat.connect.client.dart';
import '../gen/protectorate/v1/chat.pb.dart';
import '../hubs/event_feed.dart';
import 'chat_model.dart';

bool isTransient(Object e) {
  if (e is ConnectException) {
    return e.code == Code.unavailable ||
        e.code == Code.unknown ||
        e.code == Code.deadlineExceeded;
  }
  return true;
}

class ChatController extends ChangeNotifier {
  ChatController({
    required this.client,
    required this.sleeve,
    this.initialDelay = const Duration(seconds: 1),
    this.maxDelay = const Duration(seconds: 30),
    this.sendRetries = 4,
    this.sendRetryDelay = const Duration(milliseconds: 500),
    String Function()? newRequestId,
  }) : _newRequestId = newRequestId ?? _defaultRequestId;

  final ChatServiceClient client;
  final String sleeve;
  final Duration initialDelay;
  final Duration maxDelay;
  final int sendRetries;
  final Duration sendRetryDelay;
  final String Function() _newRequestId;

  final transcript = ChatTranscript();
  FeedState feedState = FeedState.connecting;
  bool sending = false;

  int _attempt = 0;
  StreamSubscription<ChatEvent>? _sub;
  CancelableSignal? _abort;
  Timer? _timer;
  bool _stopped = true;
  bool _disposed = false;

  static int _counter = 0;
  static String _defaultRequestId() =>
      '${DateTime.now().microsecondsSinceEpoch}-${_counter++}';

  void start() {
    if (!_stopped) return;
    _stopped = false;
    _connect();
  }

  void _connect() {
    feedState = FeedState.connecting;
    notifyListeners();
    final abort = _abort = CancelableSignal();
    _sub = client
        .watch(
          WatchRequest(
            sleeve: sleeve,
            afterSeq: Int64(transcript.resumeAfter()),
          ),
          signal: abort,
        )
        .listen(
          (e) {
            _attempt = 0;
            feedState = FeedState.live;
            transcript.apply(e);
            notifyListeners();
          },
          onError: (Object _) => _retry(),
          onDone: _retry,
          cancelOnError: true,
        );
  }

  // Cancelling the subscription alone leaves a web fetch open, and the
  // browser allows only six connections per host; the signal aborts it.
  void _close() {
    _sub?.cancel();
    _sub = null;
    _abort?.cancel();
    _abort = null;
  }

  void _retry() {
    if (_stopped) return;
    _close();
    final delay = reconnectBackoff(_attempt, initialDelay, maxDelay);
    _attempt++;
    feedState = FeedState.retrying;
    notifyListeners();
    _timer = Timer(delay, () {
      if (!_stopped) _connect();
    });
  }

  ChatState get state => transcript.state;
  bool get working =>
      state == ChatState.CHAT_STATE_WORKING ||
      state == ChatState.CHAT_STATE_AWAITING_APPROVAL;
  bool get canCompact => state == ChatState.CHAT_STATE_IDLE;

  // The request id is fixed before the first attempt so a retry after a lost
  // response cannot deliver the message twice.
  Future<void> send(String text) async {
    final req = SendRequest(
      sleeve: sleeve,
      text: text,
      requestId: _newRequestId(),
    );
    sending = true;
    notifyListeners();
    try {
      for (var attempt = 0; ; attempt++) {
        try {
          await client.send(req);
          return;
        } on Object catch (e) {
          if (attempt >= sendRetries || !isTransient(e) || _disposed) rethrow;
          await Future<void>.delayed(
            reconnectBackoff(attempt, sendRetryDelay, maxDelay),
          );
        }
      }
    } finally {
      sending = false;
      if (!_disposed) notifyListeners();
    }
  }

  Future<void> interrupt() =>
      client.interrupt(InterruptRequest(sleeve: sleeve));

  Future<void> respond(String approvalId, bool allow) => client.respond(
    RespondRequest(sleeve: sleeve, approvalId: approvalId, allow: allow),
  );

  Future<void> compact() => client.compact(CompactRequest(sleeve: sleeve));

  @override
  void dispose() {
    _disposed = true;
    _stopped = true;
    _timer?.cancel();
    _close();
    super.dispose();
  }
}
