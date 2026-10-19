import 'dart:async';

import 'package:flutter/foundation.dart';

import '../gen/protectorate/v1/events.pb.dart';

enum FeedState { connecting, live, retrying }

typedef EventStreamFactory = Stream<Event> Function();

class EventFeed extends ChangeNotifier {
  EventFeed(
    this._open, {
    this.maxEvents = 200,
    this.initialDelay = const Duration(seconds: 1),
    this.maxDelay = const Duration(seconds: 30),
  });

  final EventStreamFactory _open;
  final int maxEvents;
  final Duration initialDelay;
  final Duration maxDelay;

  final _incoming = StreamController<Event>.broadcast();
  final List<Event> _events = [];
  FeedState _state = FeedState.connecting;
  Object? _lastError;
  int _attempt = 0;
  StreamSubscription<Event>? _sub;
  Timer? _timer;
  bool _stopped = true;

  Stream<Event> get incoming => _incoming.stream;
  List<Event> get events => List.unmodifiable(_events);
  FeedState get state => _state;
  Object? get lastError => _lastError;
  int get attempt => _attempt;

  void start() {
    if (!_stopped) return;
    _stopped = false;
    _connect();
  }

  void _connect() {
    _setState(FeedState.connecting);
    _sub = _open().listen(
      (e) {
        _attempt = 0;
        _lastError = null;
        _events.insert(0, e);
        _incoming.add(e);
        if (_events.length > maxEvents) _events.removeLast();
        _setState(FeedState.live);
      },
      onError: (Object e) => _retry(e),
      onDone: () => _retry(null),
      cancelOnError: true,
    );
  }

  Duration backoff(int attempt) {
    final ms = initialDelay.inMilliseconds << attempt.clamp(0, 16);
    return Duration(milliseconds: ms.clamp(0, maxDelay.inMilliseconds));
  }

  void _retry(Object? error) {
    if (_stopped) return;
    _sub?.cancel();
    _sub = null;
    _lastError = error;
    final delay = backoff(_attempt);
    _attempt++;
    _setState(FeedState.retrying);
    _timer = Timer(delay, () {
      if (!_stopped) _connect();
    });
  }

  void _setState(FeedState s) {
    _state = s;
    notifyListeners();
  }

  void stop() {
    _stopped = true;
    _timer?.cancel();
    _sub?.cancel();
    _sub = null;
  }

  @override
  void dispose() {
    stop();
    _incoming.close();
    super.dispose();
  }
}
