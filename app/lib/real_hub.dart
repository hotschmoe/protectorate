import 'dart:async';
import 'dart:collection';
import 'dart:math';

import 'api/api.g.dart';
import 'hub.dart';

/// A hub over the generated [HubApi]: the events WebSocket keeps [state] current and reconnects
/// after a drop, starting again from the snapshot the hub sends first; actions are API calls. Transcripts load with `GetTranscript`
/// when first asked for and then follow the stream's `entry` messages.
class RealHub implements HubClient {
  RealHub(this.url, {String? token})
    : _api = HubApi(Uri.parse(url), token: token) {
    unawaited(_run());
  }

  final String url;
  final HubApi _api;

  HubState _state = HubState(
    seq: 0,
    hub: HubInfo(
      name: '-',
      version: '-',
      startedAt: DateTime.fromMillisecondsSinceEpoch(0),
    ),
  );
  int? _seq;
  int? _savedSeq;
  HubState? _pendingState;
  bool _catchingUp = false;
  bool _reloadTranscripts = false;
  final List<(String, ChatEntry)> _pendingEntries = [];
  bool _connected = false;
  bool _closed = false;
  bool _subscriptionsChanged = false;
  int _backoff = 1;
  String? _lastError;
  StreamSubscription<HubStreamMessage>? _sub;
  Completer<void>? _drop;
  Timer? _flush;
  final _states = StreamController<HubState>.broadcast();
  final Map<String, _Tx> _tx = {};

  @override
  String get label => _connected
      ? url
      : '$url offline${_lastError == null ? '' : ' ($_lastError)'}';

  @override
  bool get connected => _connected;

  @override
  HubState get state => _state;

  @override
  Stream<HubState> get states => _states.stream;

  @override
  void close() {
    _closed = true;
    _flush?.cancel();
    unawaited(_sub?.cancel());
    _finish();
    unawaited(_states.close());
    for (final t in _tx.values) {
      t.close();
    }
  }

  // ---- the event stream ----------------------------------------------------------------------

  Future<void> _run() async {
    while (!_closed) {
      _beginCatchup();
      _reloadTranscripts = true;
      _subscriptionsChanged = false;
      final drop = _drop = Completer<void>();
      _sub = _api
          .events(
            transcripts: [
              for (final e in _tx.entries)
                if (e.value.ctl.hasListener) e.key,
            ],
          )
          .listen(
            _onMessage,
            onError: (Object e) {
              _lastError = _short(e);
              _finish();
            },
            onDone: _finish,
            cancelOnError: true,
          );
      await drop.future;
      await _sub?.cancel();
      _sub = null;
      if (_closed) return;
      if (_catchingUp) {
        _seq = _savedSeq;
        _pendingState = null;
        _pendingEntries.clear();
        _reloadTranscripts = false;
        _catchingUp = false;
      }
      if (_subscriptionsChanged) continue;
      if (_connected) {
        _connected = false;
        _changed();
      }
      await Future<void>.delayed(Duration(seconds: _backoff));
      _backoff = min(_backoff * 2, 30);
    }
  }

  void _finish() {
    final d = _drop;
    if (d != null && !d.isCompleted) d.complete();
  }

  void _subscriptionsChangedNow() {
    if (_closed) return;
    _subscriptionsChanged = true;
    _finish();
  }

  static String _short(Object e) {
    final s = '$e';
    return s.length > 80 ? '${s.substring(0, 80)}...' : s;
  }

  void _beginCatchup() {
    if (_catchingUp) return;
    _catchingUp = true;
    _savedSeq = _seq;
    _pendingState = _state;
    _flush?.cancel();
    _flush = null;
    for (final t in _tx.values) {
      t.flush?.cancel();
      t.flush = null;
    }
  }

  void _setState(HubState state) {
    if (_catchingUp) {
      _pendingState = state;
    } else {
      _state = state;
    }
  }

  void _onMessage(HubStreamMessage m) {
    if (m is HubStreamMessageSnapshot && !_catchingUp) _beginCatchup();
    if (m is HubStreamMessageLive) {
      if (m.seq > (_seq ?? -1)) _seq = m.seq;
      _state = _pendingState ?? _state;
      _pendingState = null;
      _catchingUp = false;
      for (final (sleeveId, entry) in _pendingEntries) {
        _tx[sleeveId]?.add(entry);
      }
      _pendingEntries.clear();
      for (final MapEntry(:key, :value) in _tx.entries) {
        if (_reloadTranscripts) {
          value.loaded = false;
          if (value.loading) {
            value.reload = true;
          } else if (value.ctl.hasListener) {
            unawaited(_load(key, value));
          }
        }
        if (value.ctl.hasListener) _emitTx(key, value);
      }
      _reloadTranscripts = false;
      _connected = true;
      _lastError = null;
      _backoff = 1;
      _changed();
      return;
    }
    _backoff = 1;
    if (m.seq > (_seq ?? -1)) _seq = m.seq;
    final s = _pendingState ?? _state;
    switch (m) {
      case HubStreamMessageSnapshot(:final state):
        _setState(state);
        _seq = state.seq;
        _pendingEntries.clear();
        _reloadTranscripts = true;
      case HubStreamMessageLive():
        throw StateError('live handled above');
      case HubStreamMessageWorkspace(:final workspace):
        _setState(
          _copy(
            s,
            workspaces: _upsert(
              s.workspaces,
              workspace,
              (x) => x.id == workspace.id,
            ),
          ),
        );
      case HubStreamMessageSleeve(:final sleeve):
        _setState(
          _copy(
            s,
            sleeves: _upsert(s.sleeves, sleeve, (x) => x.id == sleeve.id),
          ),
        );
      case HubStreamMessageSleeveRemoved(:final id):
        _setState(
          _copy(
            s,
            sleeves: [
              for (final x in s.sleeves)
                if (x.id != id) x,
            ],
          ),
        );
      case HubStreamMessageEntry(:final sleeveId, :final entry):
        final t = _tx[sleeveId];
        if (t != null) {
          if (_catchingUp) {
            _pendingEntries.add((sleeveId, entry));
          } else {
            t.add(entry);
            _emitTx(sleeveId, t);
          }
        }
        return;
      case HubStreamMessageApproval(:final approval):
        _setState(
          _copy(
            s,
            approvals: _upsert(
              s.approvals,
              approval,
              (x) => x.id == approval.id,
            ),
          ),
        );
      case HubStreamMessageHub(:final hub):
        _setState(_copy(s, hub: hub));
      case HubStreamMessageNode(:final node):
        _setState(
          _copy(s, nodes: _upsert(s.nodes, node, (x) => x.id == node.id)),
        );
      case HubStreamMessageNodeRemoved(:final id):
        _setState(
          _copy(
            s,
            nodes: [
              for (final x in s.nodes)
                if (x.id != id) x,
            ],
          ),
        );
      case HubStreamMessageJob(:final job):
        _setState(_copy(s, jobs: _upsert(s.jobs, job, (x) => x.id == job.id)));
      case HubStreamMessageQuota(:final quota):
        _setState(
          _copy(
            s,
            quotas: _upsert(
              s.quotas,
              quota,
              (x) => x.subscription == quota.subscription,
            ),
          ),
        );
      case HubStreamMessageEvent(:final event):
        final events = _upsert(s.events, event, (x) => x.seq == event.seq);
        _setState(
          _copy(
            s,
            events: events.length > 500
                ? events.sublist(events.length - 500)
                : events,
          ),
        );
      case HubStreamMessageUnknown():
        return;
    }
    _changed();
  }

  static List<T> _upsert<T>(List<T> xs, T x, bool Function(T) same) {
    final i = xs.indexWhere(same);
    return i < 0 ? [...xs, x] : ([...xs]..[i] = x);
  }

  static HubState _copy(
    HubState s, {
    HubInfo? hub,
    List<Workspace>? workspaces,
    List<Sleeve>? sleeves,
    List<Approval>? approvals,
    List<Node>? nodes,
    List<Job>? jobs,
    List<Quota>? quotas,
    List<HubEvent>? events,
  }) => HubState(
    seq: s.seq,
    hub: hub ?? s.hub,
    workspaces: workspaces ?? s.workspaces,
    sleeves: sleeves ?? s.sleeves,
    approvals: approvals ?? s.approvals,
    nodes: nodes ?? s.nodes,
    jobs: jobs ?? s.jobs,
    quotas: quotas ?? s.quotas,
    events: events ?? s.events,
  );

  /// Coalesces bursts into one state per 50 ms.
  void _changed() {
    if (_closed || _catchingUp) return;
    _flush ??= Timer(const Duration(milliseconds: 50), () {
      _flush = null;
      if (!_closed) _states.add(_state);
    });
  }

  // ---- transcripts ---------------------------------------------------------------------------

  _Tx _txFor(String sleeveId) =>
      _tx.putIfAbsent(sleeveId, () => _Tx(_subscriptionsChangedNow));

  @override
  Transcript transcript(String sleeveId) {
    final t = _txFor(sleeveId);
    if (!t.loaded && !t.loading) unawaited(_load(sleeveId, t));
    return t.snapshot(sleeveId);
  }

  @override
  Stream<Transcript> transcripts(String sleeveId) =>
      _txFor(sleeveId).ctl.stream;

  Future<void> _load(String sleeveId, _Tx t) async {
    if (t.loading) return;
    t.loading = true;
    try {
      final r = await _api.getTranscript(
        GetTranscriptRequest(sleeveId: sleeveId, limit: _Tx.keep),
      );
      for (final e in r.entries) {
        t.add(e);
      }
      t.trim();
      t.loaded = true;
      _emitTx(sleeveId, t);
    } on Object catch (e) {
      _lastError = _short(e);
      _changed();
    } finally {
      t.loading = false;
      if (t.reload) {
        t.reload = false;
        t.loaded = false;
        if (!_closed && t.ctl.hasListener) unawaited(_load(sleeveId, t));
      }
    }
  }

  void _emitTx(String sleeveId, _Tx t) {
    if (_catchingUp) return;
    t.flush ??= Timer(const Duration(milliseconds: 50), () {
      t.flush = null;
      if (!t.ctl.isClosed) t.ctl.add(t.snapshot(sleeveId));
    });
  }

  // ---- actions -------------------------------------------------------------------------------

  @override
  Future<void> send(String sleeveId, String text) =>
      _api.sendMessage(SendMessageRequest(sleeveId: sleeveId, text: text));

  @override
  Future<Sleeve> spawnSleeve(SpawnSleeveRequest req) async {
    final s = await _api.spawnSleeve(req);
    final cur = _pendingState ?? _state;
    _setState(_copy(cur, sleeves: _upsert(cur.sleeves, s, (x) => x.id == s.id)));
    _changed();
    return s;
  }

  @override
  Future<void> start(String sleeveId, {Harness? target}) =>
      _api.startSleeve(StartSleeveRequest(sleeveId: sleeveId, target: target));

  @override
  Future<void> interrupt(String sleeveId) =>
      _api.interrupt(SleeveRef(sleeveId: sleeveId));

  @override
  Future<void> stop(String sleeveId) =>
      _api.stopSleeve(SleeveRef(sleeveId: sleeveId));

  @override
  Future<ModelCatalog> listModels(Harness harness) =>
      _api.listModels(ModelCatalogRequest(harness: harness));

  @override
  Future<void> resleeve(ResleeveRequest req) => _api.resleeve(req);

  @override
  Future<void> decide(String approvalId, Decision decision) =>
      _api.decide(DecideRequest(approvalId: approvalId, decision: decision));

  @override
  Future<void> markEventsRead() async {
    final events = _state.events;
    if (events.isEmpty) return;
    await _api.markEventsRead(
      MarkEventsReadRequest(upToSeq: events.map((e) => e.seq).reduce(max)),
    );
  }
}

/// One sleeve's transcript by seq. A delta shares the seq of its block's whole entry, so it
/// joins the delta before it and gives way to the whole entry.
class _Tx {
  _Tx(void Function() changed)
    : ctl = StreamController<Transcript>.broadcast(
        onListen: changed,
        onCancel: changed,
      );

  static const keep = 2000;

  final entries = SplayTreeMap<int, ChatEntry>();
  final StreamController<Transcript> ctl;
  bool loading = false;
  bool loaded = false;
  bool reload = false;
  Timer? flush;

  void add(ChatEntry e) {
    final old = entries[e.seq];
    entries[e.seq] = old == null ? e : _merge(old, e);
    trim();
  }

  void trim() {
    while (entries.length > keep) {
      entries.remove(entries.firstKey());
    }
  }

  static ChatEntry _merge(ChatEntry old, ChatEntry e) {
    final (a, b) = (old.entry, e.entry);
    final Entry? joined = switch ((a, b)) {
      (EntryAssistantText(delta: true), EntryAssistantText(delta: true)) =>
        EntryAssistantText(
          id: (b as EntryAssistantText).id,
          text: (a as EntryAssistantText).text + b.text,
          delta: true,
        ),
      (EntryReasoning(delta: true), EntryReasoning(delta: true)) =>
        EntryReasoning(
          id: (b as EntryReasoning).id,
          text: (a as EntryReasoning).text + b.text,
          delta: true,
        ),
      (EntryAssistantText(delta: false), EntryAssistantText(delta: true)) => a,
      (EntryReasoning(delta: false), EntryReasoning(delta: true)) => a,
      _ => null,
    };
    if (joined == null) return e;
    return ChatEntry(
      seq: e.seq,
      g: e.g,
      turn: e.turn,
      at: old.at,
      entry: joined,
    );
  }

  void close() {
    flush?.cancel();
    unawaited(ctl.close());
  }

  Transcript snapshot(String sleeveId) => Transcript(
    sleeveId: sleeveId,
    entries: List.unmodifiable(entries.values),
  );
}
