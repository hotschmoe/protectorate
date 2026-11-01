import 'dart:async';
import 'dart:math';

import 'api/api.g.dart';
import 'hub.dart';

typedef Json = Map<String, Object?>;

const _gib = 1024 * 1024 * 1024;

/// A scripted hub for judging the UI without a real one. It holds its state as the JSON the hub
/// would send and hands it out through the generated `fromJson`, so the JSON shapes are
/// exercised too. Transcripts are append-only like the hub's: a tool result is a new entry, and
/// a streaming text is one delta entry that turns whole at the end. A 250 ms tick streams text,
/// moves activity and jitters node samples.
class FakeHub implements HubClient {
  FakeHub() : _t0 = DateTime.now().millisecondsSinceEpoch {
    _seed();
    _state = HubState.fromJson(_snapshot());
    _timer = Timer.periodic(const Duration(milliseconds: 250), (_) => _onTick());
  }

  final int _t0;
  final _rand = Random(7);
  late final Timer _timer;
  int _tick = 0;
  int _seq = 0;
  int _hubSeq = 0;
  int _eventSeq = 0;
  int _requestSeq = 0;
  int _approvalSeq = 32;
  int _jobSeq = 0x4f30;
  bool _dirty = false;
  final Set<String> _dirtyTx = {};

  late Json _hub;
  final List<Json> _workspaces = [];
  final List<Json> _sleeves = [];
  final List<Json> _approvals = [];
  final List<Json> _nodes = [];
  final List<Json> _jobs = [];
  final List<Json> _quotas = [];
  final List<Json> _events = [];
  final Map<String, List<Json>> _tx = {};

  final List<_Step> _steps = [];
  final List<_Stream> _streams = [];
  final Map<String, int> _turnGen = {};
  final Map<String, void Function(bool)> _onDecide = {};

  late HubState _state;
  final _states = StreamController<HubState>.broadcast();
  final Map<String, StreamController<Transcript>> _txCtl = {};

  int get _now => DateTime.now().millisecondsSinceEpoch;
  int _ago(Duration d) => _now - d.inMilliseconds;

  @override
  String get label => 'FAKE HUB (scripted)';

  @override
  HubState get state => _state;

  @override
  Stream<HubState> get states => _states.stream;

  @override
  Transcript transcript(String sleeveId) =>
      Transcript.fromJson({'sleeve_id': sleeveId, 'entries': _tx[sleeveId] ?? <Json>[]});

  @override
  Stream<Transcript> transcripts(String sleeveId) =>
      _txCtl.putIfAbsent(sleeveId, StreamController<Transcript>.broadcast).stream;

  @override
  void close() {
    _timer.cancel();
    unawaited(_states.close());
    for (final c in _txCtl.values) {
      unawaited(c.close());
    }
  }

  // ---- actions -------------------------------------------------------------------------------

  @override
  Future<void> send(String sleeveId, String text) async {
    final s = _sleeve(sleeveId);
    final life = s['lifecycle'];
    if (life != 'running') {
      throw StateError('${s['name']} is ${(life as String).toUpperCase()}; message not sent');
    }
    final rid = _user(sleeveId, 'owner', text);
    if (s['role'] == 'envoy') {
      _envoyTurn(text, rid);
    } else {
      _sleeveTurn(sleeveId, rid);
    }
  }

  @override
  Future<void> interrupt(String sleeveId) async {
    final s = _sleeve(sleeveId);
    final act = _activity(sleeveId);
    if (act == null || !['working', 'blocked', 'delivering', 'compacting'].contains(act)) {
      throw StateError('${s['name']} is not mid-turn');
    }
    _cancelTurn(sleeveId);
    _setActivity(sleeveId, 'interrupting');
    _after(1000, () {
      for (final call in _openCalls(sleeveId)) {
        _finishTool(sleeveId, call, '(interrupted)', status: 'cancelled');
      }
      _notice(sleeveId, 'interrupted by owner', level: 'warn');
      _settle(sleeveId, stop: 'cancelled');
      _setActivity(sleeveId, 'idle');
      _event('sleeve', s['name'] as String, 'interrupted by owner');
    });
  }

  @override
  Future<void> stop(String sleeveId) async {
    final s = _sleeve(sleeveId);
    if (s['lifecycle'] == 'stopped' || s['lifecycle'] == 'stopping') {
      throw StateError('${s['name']} is already ${(s['lifecycle'] as String).toUpperCase()}');
    }
    final targets = [
      sleeveId,
      for (final c in _sleeves)
        if (c['parent_id'] == sleeveId && c['lifecycle'] != 'stopped') c['id'] as String,
    ];
    for (final id in targets) {
      _cancelTurn(id);
      _sleeve(id)['lifecycle'] = 'stopping';
      _setActivity(id, 'interrupting');
    }
    _after(1500, () {
      for (final id in targets) {
        _sleeve(id)['lifecycle'] = 'stopped';
        _setActivity(id, 'ended');
        _notice(id, id == sleeveId ? 'stopped by owner' : 'stopped with its root', level: 'warn');
        _entry(id, {'type': 'exited', 'code': 0, 'requested': true});
      }
      _event('sleeve', s['name'] as String,
          targets.length > 1 ? 'stopped with ${targets.length - 1} children' : 'stopped');
    });
  }

  @override
  Future<void> resleeve(String sleeveId, Harness target) async {
    final s = _sleeve(sleeveId);
    final inc = s['incarnation'] as Json?;
    if (inc == null) throw StateError('${s['name']} has no incarnation');
    if (target == Harness.unknown) throw StateError('resleeve to what?');
    final from = inc['harness'] as String;
    final g = inc['g'] as int;
    _cancelTurn(sleeveId);
    s['lifecycle'] = 'resleeving';
    _setActivity(sleeveId, 'ended');
    _entry(sleeveId, {'type': 'exited', 'code': 0, 'requested': true});
    _after(2200, () {
      s['lifecycle'] = 'running';
      final next = s['incarnation'] = _incarnation(target.wire, g: g + 1, activity: 'booting', turn: 0);
      _notice(sleeveId, 'resleeved g$g -> g${g + 1}: $from -> ${target.wire} (owner); '
          'brief written to .protectorate/BRIEF.md');
      _sessionInfo(sleeveId, next);
      _event('sleeve', s['name'] as String, 'resleeved g$g -> g${g + 1}: $from -> ${target.wire}');
      _after(1200, () => _setActivity(sleeveId, 'idle'));
    });
  }

  @override
  Future<void> decide(String approvalId, Decision decision) async {
    final a = _approvals.firstWhere(
      (a) => a['id'] == approvalId,
      orElse: () => throw StateError('no approval $approvalId'),
    );
    if (a['state'] != 'pending') throw StateError('$approvalId is already ${a['state']}');
    if (decision == Decision.unknown) throw StateError('decide what?');
    final approve = decision != Decision.deny;
    a['state'] = approve ? 'approved' : 'denied';
    a['decision'] = decision.wire;
    a['decided_by'] = 'owner';
    a['decided_at_ms'] = _now;
    _dirty = true;
    final sleeveId = a['sleeve_id'] as String;
    final sleeveName = _sleeve(sleeveId)['name'] as String;
    _event('approval', 'owner', '${approve ? 'approved' : 'denied'} $approvalId (${a['summary']})',
        target: sleeveName);
    _entry(sleeveId, {
      'type': 'approval_decision',
      'approval_id': approvalId,
      'decision': decision.wire,
      'by': 'owner',
    });
    _onDecide.remove(approvalId)?.call(approve);
  }

  @override
  Future<void> markEventsRead() async {
    for (final e in _events) {
      e['read'] = true;
    }
    _dirty = true;
  }

  // ---- scripted turns ------------------------------------------------------------------------

  void _envoyTurn(String text, String rid) {
    const id = 's-envoy';
    final t = text.toLowerCase();
    final gen = _beginTurn(id);
    String cmd;
    String out;
    String reply;
    void Function()? effect;
    if (t.contains('test')) {
      final job = 'j-${(_jobSeq++).toRadixString(16)}';
      cmd = 'protectorate job run --on arch=arm64 -- make test';
      out = 'job $job queued on build-arm64 (arm64, AWAY 3h)';
      reply = 'Dispatched `make test` to build-arm64 as $job. It is AWAY, so the job runs at normal '
          'priority. I will report the exit code here.';
      effect = () => _runJob(job, 'n-arm64', 'make test', 'envoy', 9000);
    } else if (t.contains('spawn') || t.contains('start') || t.contains('new')) {
      final n = _sleeves.where((s) => s['parent_id'] == 's-proto').length + 1;
      final name = 'fix-$n';
      cmd = 'protectorate sleeve spawn --parent protectorate --harness codex $name';
      out = 'sleeve $name: provisioning worktree protectorate/$name';
      reply = 'Spawned $name as a child of protectorate on codex, in its own worktree and branch. '
          'It has your message as its first turn.';
      effect = () => _spawnChild(name, text);
    } else if (t.contains('status') || t.contains('?') || t.contains('what')) {
      cmd = 'protectorate sleeve list';
      out = _sleeveListText();
      reply = _statusReply();
    } else {
      cmd = 'protectorate mail send --to protectorate --stdin';
      out = 'mail m-${_rand.nextInt(900) + 100} delivered to protectorate';
      reply = 'Passed that to protectorate as mail. I will watch for its report and tell you '
          'what changed.';
    }
    _deliver(gen, id, rid);
    final call = 'c-${_seq + 1}';
    _step(gen, id, 1000, () => _tool(id, call, 'Bash', cmd, cmd));
    _step(gen, id, 2400, () {
      _finishTool(id, call, out);
      effect?.call();
    });
    _step(gen, id, 2800, () => _say(id, reply, then: () {
      _settle(id);
      _setActivity(id, 'idle');
    }));
  }

  void _sleeveTurn(String id, String rid) {
    final gen = _beginTurn(id);
    _deliver(gen, id, rid);
    final call = 'c-${_seq + 1}';
    _step(gen, id, 1200, () => _tool(id, call, 'Bash', 'git status --short', 'git status --short'));
    _step(gen, id, 2200, () => _finishTool(id, call, ' M app/lib/theme.dart\n?? app/lib/model.dart'));
    _step(gen, id, 2600, () => _say(
      id,
      'Read it. Two files are modified in the worktree; I will fold your note into the next '
      'commit and run the checks before reporting back to Envoy.',
      then: () {
        _settle(id);
        _setActivity(id, 'idle', seen: false);
      },
    ));
  }

  /// The harness takes the message: ack, then the turn starts.
  void _deliver(int gen, String id, String rid) {
    _setActivity(id, 'delivering', requestId: rid);
    _step(gen, id, 500, () {
      _entry(id, {'type': 'prompt_ack', 'request_id': rid, 'ack': 'confirmed'});
      _setActivity(id, 'working');
      _bumpTurn(id);
      _entry(id, {'type': 'turn_started', 'request_id': rid});
    });
  }

  void _spawnChild(String name, String firstTurn) {
    final id = 's-$name';
    _sleeves.add({
      'id': id,
      'name': name,
      'workspace_id': 'w-proto',
      'parent_id': 's-proto',
      'role': 'child',
      'lifecycle': 'provisioning',
      'resleeve_to': ['pi_local'],
      'branch': 'protectorate/$name',
      'incarnation': null,
    });
    _tx[id] = [];
    _event('sleeve', 'envoy', 'spawned $name (codex) under protectorate');
    _after(1200, () => _sleeve(id)['lifecycle'] = 'launching');
    _after(2600, () {
      _sleeve(id)['lifecycle'] = 'running';
      final inc = _sleeve(id)['incarnation'] = _incarnation('codex', g: 1, activity: 'booting', turn: 0);
      _sessionInfo(id, inc);
      final rid = _user(id, 'envoy', firstTurn);
      _after(1200, () => _sleeveTurn(id, rid));
    });
  }

  void _runJob(String id, String nodeId, String command, String by, int ms) {
    _jobs.insert(0, {
      'id': id,
      'node_id': nodeId,
      'target': {'type': 'node', 'node_id': nodeId},
      'argv': command.split(' '),
      'state': 'queued',
      'requested_by': by,
      'attempt': 1,
      'queued_at_ms': _now,
    });
    final node = _nodes.firstWhere((n) => n['id'] == nodeId)['name'] as String;
    _event('job', id, 'queued on $node: $command');
    _after(800, () {
      final j = _jobs.firstWhere((j) => j['id'] == id);
      j['state'] = 'running';
      j['started_at_ms'] = _now;
    });
    _after(ms, () {
      final j = _jobs.firstWhere((j) => j['id'] == id);
      j['state'] = 'succeeded';
      j['ended_at_ms'] = _now;
      j['exit_code'] = 0;
      _event('job', id, 'exited 0 ($node) in ${(ms / 1000).round()}s');
      _user('s-envoy', 'hub', 'job $id exited 0 on $node');
      _say('s-envoy', '$id passed on $node (exit 0). Nothing else is waiting on it.',
          then: () => _settle('s-envoy'));
    });
  }

  // ---- background script ---------------------------------------------------------------------

  void _onTick() {
    _tick++;
    final now = _now;
    for (final s in List.of(_steps)) {
      if (s.due <= now) {
        _steps.remove(s);
        if (s.sleeve == null || _turnGen[s.sleeve] == s.gen) s.run();
      }
    }
    for (final st in List.of(_streams)) {
      final e = st.entry['entry'] as Json;
      final take = min(3, st.words.length - st.i);
      e['text'] = '${e['text']}${st.words.sublist(st.i, st.i + take).join()}';
      st.i += take;
      _touchTx(st.sleeve);
      if (st.i >= st.words.length) {
        e['delta'] = false;
        _streams.remove(st);
        st.then?.call();
      }
    }
    _ambient();
    if (_tick % 8 == 0) _jitterNodes();
    if (_dirty) {
      _dirty = false;
      _state = HubState.fromJson(_snapshot());
      _states.add(_state);
    }
    for (final id in _dirtyTx) {
      _txCtl[id]?.add(transcript(id));
    }
    _dirtyTx.clear();
  }

  /// The sleeves' own work, which goes on whether or not the owner does anything.
  void _ambient() {
    final cycle = _tick % 240;
    if (cycle == 8 && _alive('s-fuzz')) {
      _setActivity('s-fuzz', 'working');
      _tool('s-fuzz', 'fz-$_tick', 'Bash',
          'cargo +nightly fuzz run frame_codec -- -max_total_time=12',
          'cargo +nightly fuzz run frame_codec -- -max_total_time=12');
    }
    if (cycle == 56 && _alive('s-fuzz')) {
      final execs = 180000 + _rand.nextInt(60000);
      _finishTool('s-fuzz', 'fz-${_tick - 48}',
          '#$execs\tDONE   cov: 2291 ft: 9120 corp: 611/48Kb exec/s: ${execs ~/ 12}\n'
          'Done $execs runs in 12 second(s)');
      _say('s-fuzz', 'Round clean: $execs execs, no crashes, corpus at 611 inputs.', then: () {
        _settle('s-fuzz');
        _setActivity('s-fuzz', 'idle');
      });
      _event('mail', 'fuzz-wire', 'frame_codec round clean ($execs execs)', target: 'protectorate');
    }
    if (cycle == 20 && _alive('s-proto') && _activity('s-proto') == 'working') {
      _tool('s-proto', 'pt-$_tick', 'Bash', 'cargo test --workspace',
          'CARGO_TARGET_DIR=target/next-core cargo test --workspace');
    }
    if (cycle == 100 && _alive('s-proto')) {
      _finishTool('s-proto', 'pt-${_tick - 80}',
          'test result: ok. 38 passed; 0 failed; 0 ignored\n'
          'journal_roundtrip ... ok\ntorn_tail_recovery ... ok');
      _say('s-proto', 'All 38 tests pass, including torn-tail recovery. Next: group-commit fsync '
          'with a 2 ms window.');
    }
    if (_tick == 120 && _alive('s-proto')) {
      _ask('s-proto', 'tool', 'WebFetch', 'https://docs.rs/crc32c/latest',
          'Fetch the crc32c crate docs to compare the polynomial table against ours.');
    }
    if (_anyClaudeWorking()) {
      final q = _quotas.firstWhere((q) => q['subscription'] == 'claude');
      final w = (q['windows'] as List<Object?>).first as Json;
      w['used_pct'] = min(100.0, (w['used_pct'] as double) + 0.01);
      q['fetched_at_ms'] = _now;
      _dirty = true;
    }
    for (final s in _sleeves) {
      final inc = s['incarnation'] as Json?;
      if (inc == null) continue;
      final act = (inc['activity'] as Json)['state'];
      if (act == 'working') {
        inc['context_tokens'] = min(inc['context_window'] as int,
            (inc['context_tokens'] as int) + 40 + _rand.nextInt(120));
        _dirty = true;
      }
    }
  }

  void _jitterNodes() {
    for (final n in _nodes) {
      if (n['online'] != true) continue;
      final s = n['sample'] as Json;
      final cpu = (s['cpu_pct'] as double) + (_rand.nextDouble() - 0.5) * 10;
      s['cpu_pct'] = cpu.clamp(2.0, 99.0);
      s['load1'] = ((s['cpu_pct'] as double) / 100 * ((n['inventory'] as Json)['cpu_count'] as int))
          .clamp(0.0, 64.0);
      s['sampled_at_ms'] = _now;
      n['last_seen_ms'] = _now;
      final p = n['presence'] as Json;
      if (p['state'] == 'away') p['idle_secs'] = (p['idle_secs'] as int) + 2;
      if (p['state'] == 'present') {
        final idle = (p['idle_secs'] as int) + 2;
        p['idle_secs'] = _rand.nextInt(6) == 0 ? 0 : idle;
      }
      p['sampled_at_ms'] = _now;
    }
    _dirty = true;
  }

  // ---- helpers -------------------------------------------------------------------------------

  Json _sleeve(String id) => _sleeves.firstWhere(
    (s) => s['id'] == id,
    orElse: () => throw StateError('no sleeve $id'),
  );

  bool _alive(String id) => _sleeve(id)['lifecycle'] == 'running';

  String? _activity(String id) {
    final inc = _sleeve(id)['incarnation'] as Json?;
    return inc == null ? null : (inc['activity'] as Json)['state'] as String;
  }

  bool _anyClaudeWorking() => _sleeves.any((s) {
    final inc = s['incarnation'] as Json?;
    return inc != null &&
        inc['harness'] == 'claude' &&
        (inc['activity'] as Json)['state'] == 'working';
  });

  void _setActivity(String id, String state, {bool seen = true, String? requestId}) {
    final inc = _sleeve(id)['incarnation'] as Json?;
    if (inc == null) return;
    inc['activity'] = {'state': state, 'since_ms': _now, 'seen': seen, 'request_id': ?requestId};
    _dirty = true;
  }

  void _bumpTurn(String id) {
    final inc = _sleeve(id)['incarnation'] as Json?;
    if (inc != null) inc['turn'] = (inc['turn'] as int) + 1;
  }

  int _beginTurn(String id) => _turnGen[id] = (_turnGen[id] ?? 0) + 1;

  void _cancelTurn(String id) {
    _beginTurn(id);
    _streams.removeWhere((s) => s.sleeve == id);
  }

  void _step(int gen, String sleeve, int ms, void Function() f) =>
      _steps.add(_Step(_now + ms, f, sleeve: sleeve, gen: gen));

  void _after(int ms, void Function() f) {
    _steps.add(_Step(_now + ms, () {
      f();
      _dirty = true;
    }));
  }

  void _touchTx(String id) {
    _dirtyTx.add(id);
    _dirty = true;
  }

  /// Appends one transcript entry, wrapped as a `ChatEntry`.
  Json _entry(String id, Json body) {
    final inc = _sleeve(id)['incarnation'] as Json?;
    final e = <String, Object?>{
      'seq': ++_seq,
      'g': inc?['g'] ?? 0,
      'turn': inc?['turn'] ?? 0,
      'at_ms': _now,
      'entry': body,
    };
    final list = _tx.putIfAbsent(id, () => []);
    list.add(e);
    if (list.length > 300) list.removeAt(0);
    _touchTx(id);
    return e;
  }

  String _user(String id, String from, String text) {
    final rid = 'r-${++_requestSeq}';
    _entry(id, {'type': 'user_message', 'request_id': rid, 'from': from, 'text': text});
    return rid;
  }

  void _notice(String id, String text, {String level = 'info'}) =>
      _entry(id, {'type': 'notice', 'level': level, 'text': text});

  void _sessionInfo(String id, Json inc) => _entry(id, {
    'type': 'session_info',
    'harness': inc['harness'],
    'mode': inc['mode'],
    'native_id': 'sess-${_rand.nextInt(1 << 30).toRadixString(16)}',
    'model': inc['model'],
    'provider': inc['provider'],
  });

  void _settle(String id, {String stop = 'end_turn'}) {
    final inc = _sleeve(id)['incarnation'] as Json?;
    _entry(id, {
      'type': 'turn_settled',
      'stop': stop,
      'usage': {
        'input': 2000 + _rand.nextInt(9000),
        'output': 200 + _rand.nextInt(900),
        'cache_read': 20000 + _rand.nextInt(20000),
        'context_tokens': inc?['context_tokens'],
        'context_window': inc?['context_window'],
      },
      'duration_ms': 2000 + _rand.nextInt(4000),
    });
  }

  static String _toolKind(String name) => switch (name) {
    'Bash' => 'command',
    'Read' => 'read',
    'Edit' || 'apply_patch' => 'edit',
    'WebFetch' => 'web',
    _ => 'other',
  };

  void _tool(String id, String call, String tool, String summary, String input) => _entry(id, {
    'type': 'tool_call',
    'id': call,
    'tool': _toolKind(tool),
    'name': tool,
    'summary': summary,
    'input': input,
  });

  /// Tool calls of this sleeve that have no result yet.
  List<String> _openCalls(String id) {
    final calls = <String>[];
    for (final e in _tx[id] ?? <Json>[]) {
      final b = e['entry'] as Json;
      if (b['type'] == 'tool_call') calls.add(b['id'] as String);
      if (b['type'] == 'tool_result') calls.remove(b['id']);
    }
    return calls;
  }

  void _finishTool(String id, String call, String output, {String status = 'completed'}) {
    if (!_openCalls(id).contains(call)) return;
    final at = (_tx[id] ?? <Json>[]).lastWhere((e) => (e['entry'] as Json)['id'] == call)['at_ms'] as int;
    _entry(id, {
      'type': 'tool_result',
      'id': call,
      'status': status,
      'output': output,
      'duration_ms': _now - at,
    });
  }

  /// A streaming text: one delta entry that grows each tick and turns whole at the end, as a
  /// hub client folds deltas that share the seq of their block.
  void _say(String id, String text, {void Function()? then}) {
    final e = _entry(id, {'type': 'assistant_text', 'id': 't-${_seq + 1}', 'text': '', 'delta': true});
    final words = RegExp(r'\S+\s*').allMatches(text).map((m) => m[0]!).toList();
    _streams.add(_Stream(id, e, words, then));
  }

  void _ask(String id, String kind, String title, String summary, String detail,
      {void Function(bool)? onDecide}) {
    final aid = 'a-${++_approvalSeq}';
    _approvals.add({
      'id': aid,
      'sleeve_id': id,
      'kind': kind,
      'title': title,
      'summary': summary,
      'detail': detail,
      'options': kind == 'tool' ? ['allow_once', 'allow_session', 'deny'] : ['allow_once', 'deny'],
      'requested_at_ms': _now,
      'timeout_at_ms': _now + 30 * 60 * 1000,
      'state': 'pending',
    });
    _entry(id, {'type': 'approval_request', 'approval_id': aid, 'title': title, 'summary': summary});
    _setActivity(id, 'blocked');
    _event('approval', _sleeve(id)['name'] as String, 'asks: $title $summary ($aid)',
        level: 'warn');
    _onDecide[aid] = onDecide ??
        (ok) {
          _setActivity(id, 'working');
          _say(id, ok ? 'Approved; continuing.' : 'Denied; skipping that step.', then: () {
            _settle(id);
            _setActivity(id, 'idle', seen: false);
          });
        };
  }

  void _event(String kind, String source, String text,
      {String? target, String level = 'info'}) {
    _events.add({
      'seq': ++_eventSeq,
      'at_ms': _now,
      'kind': kind,
      'source': source,
      'target': ?target,
      'text': text,
      'read': false,
      'level': level,
    });
    if (_events.length > 500) _events.removeAt(0);
    _dirty = true;
  }

  String _sleeveListText() {
    final b = StringBuffer('NAME           HARNESS    LIFECYCLE   ACTIVITY\n');
    for (final s in _sleeves) {
      final inc = s['incarnation'] as Json?;
      b.writeln('${(s['name'] as String).padRight(15)}'
          '${((inc?['harness'] ?? '-') as String).padRight(11)}'
          '${(s['lifecycle'] as String).toUpperCase().padRight(12)}'
          '${(((inc?['activity'] as Json?)?['state'] ?? '-') as String).toUpperCase()}');
    }
    return b.toString().trimRight();
  }

  String _statusReply() {
    final working = <String>[];
    final blocked = <String>[];
    final other = <String>[];
    for (final s in _sleeves) {
      if (s['role'] == 'envoy') continue;
      final act = _activity(s['id'] as String);
      final n = s['name'] as String;
      if (s['lifecycle'] != 'running') {
        other.add('$n is ${(s['lifecycle'] as String).toUpperCase()}');
      } else if (act == 'blocked') {
        blocked.add(n);
      } else if (act == 'working') {
        working.add(n);
      }
    }
    final pending = _approvals.where((a) => a['state'] == 'pending').length;
    return [
      if (working.isNotEmpty) 'Working: ${working.join(', ')}.',
      if (blocked.isNotEmpty) 'Blocked on you: ${blocked.join(', ')}.',
      if (other.isNotEmpty) '${other.join('; ')}.',
      '$pending approval${pending == 1 ? '' : 's'} pending. build-arm64 is AWAY and free for jobs; '
          'desk-amd64 has someone at it; build-win is offline.',
    ].join(' ');
  }

  Json _incarnation(String harness, {required int g, required String activity, int turn = 3,
      int ctx = 24000, int ageMin = 40}) {
    final (sub, provider, model, window) = switch (harness) {
      'claude' => ('claude', 'anthropic', 'claude-opus-4-5', 200000),
      'codex' => ('codex', 'openai', 'gpt-5.1-codex', 272000),
      'grok' => ('grok', 'xai', 'grok-code-fast-1', 256000),
      'pi_vendor' => ('grok', 'xai', 'grok-4', 256000),
      'pi_local' => ('local', 'llama.cpp', 'qwen3-coder-30b-a3b', 131072),
      _ => ('unknown', '', '', 0),
    };
    return {
      'g': g,
      'mode': 'headless',
      'harness': harness,
      'subscription': sub,
      'provider': provider,
      'model': model,
      'activity': {'state': activity, 'since_ms': _ago(const Duration(minutes: 2)), 'seen': true},
      'started_at_ms': _ago(Duration(minutes: ageMin)),
      'turn': turn,
      'context_tokens': ctx,
      'context_window': window,
      'cache_ratio': harness == 'pi_local' ? null : 0.9 + _rand.nextDouble() * 0.08,
    };
  }

  Json _snapshot() => {
    'seq': ++_hubSeq,
    'hub': _hub,
    'workspaces': _workspaces,
    'sleeves': _sleeves,
    'approvals': _approvals,
    'nodes': _nodes,
    'jobs': _jobs,
    'quotas': _quotas,
    'events': _events,
  };

  // ---- seed data -----------------------------------------------------------------------------

  void _seed() {
    _hub = {
      'name': 'protectorate',
      'version': kVersion,
      'started_at_ms': _t0 - const Duration(days: 11, hours: 4, minutes: 12).inMilliseconds,
    };
    _workspaces.addAll([
      {'id': 'w-envoy', 'name': 'envoy', 'path': '~/.local/share/protectorate/envoy'},
      {'id': 'w-proto', 'name': 'protectorate', 'path': '~/github/protectorate'},
      {'id': 'w-herdr', 'name': 'herdr-port', 'path': '~/github/herdr-port'},
      {'id': 'w-notes', 'name': 'notes-site', 'path': '~/github/notes-site'},
    ]);
    _sleeves.addAll([
      {
        'id': 's-envoy', 'name': 'envoy', 'workspace_id': 'w-envoy', 'role': 'envoy',
        'lifecycle': 'running', 'resleeve_to': ['codex', 'pi_local'],
        'incarnation': _incarnation('claude', g: 4, activity: 'idle', turn: 31, ctx: 61200,
            ageMin: 300),
      },
      {
        'id': 's-proto', 'name': 'protectorate', 'workspace_id': 'w-proto', 'role': 'root',
        'lifecycle': 'running', 'resleeve_to': ['pi_local'], 'branch': 'next-core',
        'incarnation': _incarnation('claude', g: 2, activity: 'working', turn: 9, ctx: 41200),
        'budget_used': 1240000, 'budget_limit': 3000000,
      },
      {
        'id': 's-laapp', 'name': 'la/app', 'workspace_id': 'w-proto', 'parent_id': 's-proto',
        'role': 'child', 'lifecycle': 'running', 'resleeve_to': ['pi_local'],
        'branch': 'la/app',
        'incarnation': _incarnation('codex', g: 3, activity: 'blocked', turn: 14, ctx: 118000),
      },
      {
        'id': 's-fuzz', 'name': 'fuzz-wire', 'workspace_id': 'w-proto', 'parent_id': 's-proto',
        'role': 'child', 'lifecycle': 'running', 'resleeve_to': ['pi_local'],
        'branch': 'fuzz-wire',
        'incarnation': _incarnation('pi_local', g: 2, activity: 'idle', turn: 22, ctx: 30500),
      },
      {
        'id': 's-herdr', 'name': 'herdr-port', 'workspace_id': 'w-herdr', 'role': 'root',
        'lifecycle': 'running', 'resleeve_to': ['pi_local'], 'branch': 'main',
        'incarnation': {
          ..._incarnation('claude', g: 1, activity: 'idle', turn: 40, ctx: 152000, ageMin: 600),
          'activity': {'state': 'idle', 'since_ms': _ago(const Duration(hours: 5)), 'seen': false},
        },
      },
      {
        'id': 's-notes', 'name': 'notes-site', 'workspace_id': 'w-notes', 'role': 'root',
        'lifecycle': 'parked', 'resleeve_to': ['pi_local'], 'branch': 'main',
        'incarnation': {
          ..._incarnation('pi_local', g: 4, activity: 'ended', turn: 2, ctx: 9000),
        },
      },
    ]);

    final h = _ago(const Duration(minutes: 49));
    int at(int min) => h + min * 60000;
    Json e(int min, Json body, {int g = 4, int turn = 0}) =>
        {'seq': ++_seq, 'g': g, 'turn': turn, 'at_ms': at(min), 'entry': body};
    Json user(String from, String text) =>
        {'type': 'user_message', 'request_id': 'r-${++_requestSeq}', 'from': from, 'text': text};
    Json say(String text) => {'type': 'assistant_text', 'id': 't-${_seq + 1}', 'text': text};
    Json note(String text, [String level = 'info']) => {'type': 'notice', 'level': level, 'text': text};
    List<Json> tool(int min, int endMin, String call, String name, String summary, Object input,
        {String? status = 'completed', String output = '', int g = 4, int turn = 0}) => [
      e(min, {
        'type': 'tool_call', 'id': call, 'tool': _toolKind(name), 'name': name,
        'summary': summary, 'input': input,
      }, g: g, turn: turn),
      if (status != null)
        e(endMin, {
          'type': 'tool_result', 'id': call, 'status': status, 'output': output,
          'duration_ms': (endMin - min) * 60000 + 1400,
        }, g: g, turn: turn),
    ];
    Json settled(int ctx, int window, {String stop = 'end_turn'}) => {
      'type': 'turn_settled', 'stop': stop, 'duration_ms': 4200,
      'usage': {'input': 3200, 'output': 640, 'cache_read': ctx - 3200, 'context_tokens': ctx,
          'context_window': window},
    };

    _approvals.addAll([
      {
        'id': 'a-29', 'sleeve_id': 's-proto', 'kind': 'tool', 'title': 'Bash',
        'summary': 'cargo +nightly fuzz build', 'detail': 'cargo +nightly fuzz build --fuzz-dir crates/pt-journal/fuzz',
        'requested_at_ms': at(2), 'state': 'approved', 'decision': 'allow_once',
        'decided_by': 'owner',
        'decided_at_ms': at(3),
      },
      {
        'id': 'a-30', 'sleeve_id': 's-herdr', 'kind': 'action', 'title': 'protected_push',
        'summary': 'git push origin main (herdr-port)',
        'detail': 'Push 14 commits to the protected branch main of herdr-port.',
        'requested_at_ms': at(5), 'state': 'denied', 'decision': 'deny', 'decided_by': 'owner',
        'decided_at_ms': at(9),
      },
      {
        'id': 'a-31', 'sleeve_id': 's-laapp', 'kind': 'tool', 'title': 'Bash',
        'summary': 'git push origin la/app',
        'detail': 'git push origin la/app\n\nla/app is 6 commits ahead of next. '
            'Codex asks before network writes.',
        'options': ['allow_once', 'allow_session', 'deny'],
        'requested_at_ms': at(44), 'timeout_at_ms': at(44) + 30 * 60000, 'state': 'pending',
      },
      {
        'id': 'a-32', 'sleeve_id': 's-envoy', 'kind': 'action', 'title': 'node_job',
        'summary': 'pacman -Syu on build-arm64',
        'detail': 'Maintenance job on build-arm64 (AWAY 3h): sudo pacman -Syu --noconfirm, '
            'then reboot if the kernel changed. Node maintenance jobs need the owner.',
        'requested_at_ms': at(47), 'timeout_at_ms': at(47) + 30 * 60000, 'state': 'pending',
      },
    ]);

    _tx['s-envoy'] = [
      e(0, {'type': 'session_info', 'harness': 'claude', 'mode': 'headless',
          'native_id': '4f1c9a2e-envoy', 'model': 'claude-opus-4-5', 'provider': 'anthropic'}),
      e(0, user('owner', "morning. what's running?"), turn: 29),
      ...tool(0, 1, 'c-e1', 'Bash', 'protectorate sleeve list', 'protectorate sleeve list',
          turn: 29,
          output: 'NAME           HARNESS    LIFECYCLE   ACTIVITY\n'
              'envoy          claude     RUNNING     WORKING\n'
              'protectorate   claude     RUNNING     WORKING\n'
              'la/app         codex      RUNNING     WORKING\n'
              'fuzz-wire      pi_local   RUNNING     IDLE\n'
              'herdr-port     claude     RUNNING     IDLE\n'
              'notes-site     pi_local   PARKED      ENDED'),
      e(1, say('Five sleeves besides me. protectorate (claude) is on the journal codec; '
          'la/app (codex) is building the app shell; fuzz-wire (pi_local) fuzzes the frame '
          'codec between rounds; herdr-port finished its port overnight and you have not '
          'looked yet. notes-site is PARKED after three crashes in an hour; it needs a look '
          'from you before it runs again.\n\nNodes: build-arm64 is AWAY and free, desk-amd64 has '
          'someone at it, build-win has been offline since 06:40.'), turn: 29),
      e(1, settled(58400, 200000), turn: 29),
      e(30, user('owner', 'run make test on arm64 for la/app and tell me'), turn: 30),
      ...tool(30, 34, 'c-e2', 'Bash',
          'protectorate job run --on arch=arm64 --sleeve la/app -- make test',
          'protectorate job run --on arch=arm64 --sleeve la/app -- make test',
          turn: 30, output: 'job j-4f2a on build-arm64\n... 412 tests, 0 failed\nexit 0 in 3m12s'),
      e(34, say('Tests pass on arm64: j-4f2a on build-arm64, 412 tests, exit 0 in 3m12s.'), turn: 30),
      e(34, settled(59900, 200000), turn: 30),
      e(44, user('mail:la/app', 'Tests are green on amd64 and arm64. '
          'Requesting approval to push la/app (a-31).'), turn: 31),
      e(45, say('la/app wants to push its branch; that approval is yours (a-31). While build-arm64 '
          'is idle I would also like to run system updates on it.'), turn: 31),
      e(47, {'type': 'approval_request', 'approval_id': 'a-32', 'title': 'node_job',
          'summary': 'pacman -Syu on build-arm64'}, turn: 31),
    ];

    _tx['s-proto'] = [
      e(0, user('envoy', 'Implement the pt-journal frame codec: length, CRC-32C, payload, and '
          'torn-tail recovery. Keep it Verus-ready.'), g: 2, turn: 8),
      e(1, {'type': 'reasoning', 'id': 'k-p1', 'text': 'The spec puts the journal in Verus phase '
          '1, so the codec should be a pure function over byte slices. Read section 10 first.'},
          g: 2, turn: 8),
      e(1, say('Reading the spec section on the journal and the old frame code first.'), g: 2,
          turn: 8),
      ...tool(2, 2, 'c-p1', 'Read', 'SPEC.md', {'file_path': 'SPEC.md', 'offset': 432, 'limit': 40},
          g: 2, turn: 8, output: '## 10. Verus\n...(40 lines)'),
      e(3, {'type': 'approval_request', 'approval_id': 'a-29', 'title': 'Bash',
          'summary': 'cargo +nightly fuzz build'}, g: 2, turn: 8),
      e(3, {'type': 'approval_decision', 'approval_id': 'a-29', 'decision': 'allow_once',
          'by': 'owner'}, g: 2, turn: 8),
      ...tool(12, 12, 'c-p2', 'Edit', 'crates/pt-journal/src/frame.rs', {
        'file_path': 'crates/pt-journal/src/frame.rs', 'old_string': '',
        'new_string': 'pub fn encode(...',
      }, g: 2, turn: 8, output: '+148 -0'),
      e(12, {'type': 'file_change', 'tool_id': 'c-p2',
          'files': [{'path': 'crates/pt-journal/src/frame.rs', 'op': 'add', 'added': 148,
              'removed': 0}]}, g: 2, turn: 8),
      e(20, {'type': 'compaction', 'trigger': 'auto', 'tokens_before': 168400,
          'tokens_after': 41200}, g: 2, turn: 8),
      e(21, say('Encoder and decoder are in. Writing the torn-tail path: a short read at the tail '
          'truncates to the last whole frame.'), g: 2, turn: 8),
      ...tool(26, 27, 'c-p3', 'Bash', 'cargo clippy --workspace',
          'cargo clippy --workspace --all-targets -- -D warnings', g: 2, turn: 8, status: 'failed',
          output: 'error: this `if` has identical blocks\n  --> crates/pt-journal/src/frame.rs:88:9'),
      e(28, say('Fixed the duplicated branch; clippy is clean.'), g: 2, turn: 8),
      e(28, settled(41200, 200000), g: 2, turn: 8),
    ];

    _tx['s-laapp'] = [
      e(10, user('envoy', 'Build the Flutter shell in the new look, Envoy chat first. Push la/app '
          'when the tests pass.'), g: 3, turn: 13),
      e(11, say('Starting from the palette tokens on the spec page.'), g: 3, turn: 13),
      ...tool(12, 12, 'c-l1', 'apply_patch', 'app/lib/theme.dart',
          '*** Begin Patch\n*** Add File: app/lib/theme.dart\n+class Palette {', g: 3, turn: 13,
          output: 'A app/lib/theme.dart (+182)'),
      ...tool(20, 21, 'c-l2', 'Bash', 'flutter analyze', 'flutter analyze', g: 3, turn: 13,
          output: 'Analyzing app...\nNo issues found! (ran in 4.1s)'),
      e(21, settled(96000, 272000), g: 3, turn: 13),
      e(40, user('envoy', 'arm64 tests passed (j-4f2a). Push when ready.'), g: 3, turn: 14),
      e(43, say('Pushing la/app.'), g: 3, turn: 14),
      ...tool(44, 44, 'c-l3', 'Bash', 'git push origin la/app', 'git push origin la/app', g: 3,
          turn: 14, status: null),
      e(44, {'type': 'approval_request', 'approval_id': 'a-31', 'title': 'Bash',
          'summary': 'git push origin la/app'}, g: 3, turn: 14),
    ];
    _onDecide['a-31'] = (ok) {
      const id = 's-laapp';
      _setActivity(id, 'working');
      if (ok) {
        _finishTool(id, 'c-l3', 'To github.com:hotschmoe/protectorate.git\n'
            ' * [new branch]      la/app -> la/app');
        _say(id, 'Pushed la/app (6 commits). Reporting to Envoy.',
            then: () => _setActivity(id, 'idle', seen: false));
        _event('mail', 'la/app', 'pushed la/app, 6 commits', target: 'envoy');
      } else {
        _finishTool(id, 'c-l3', 'denied by owner', status: 'declined');
        _say(id, 'Not pushing. The branch stays local until you say otherwise.',
            then: () => _setActivity(id, 'idle', seen: false));
      }
    };
    _onDecide['a-32'] = (ok) {
      if (!ok) {
        _say('s-envoy', 'Leaving build-arm64 as it is.');
        return;
      }
      final job = 'j-${(_jobSeq++).toRadixString(16)}';
      _say('s-envoy', 'Running the updates on build-arm64 as $job.');
      _runJob(job, 'n-arm64', 'sudo pacman -Syu --noconfirm', 'envoy', 7000);
    };

    _tx['s-fuzz'] = [
      e(0, user('envoy', 'Fuzz pt-frame in rounds and mail protectorate after each one.'), g: 1),
      e(6, {'type': 'error', 'kind': 'quota', 'text': 'usage limit reached for the 5h window'},
          g: 1),
      e(6, note('G1 quota_exhausted: claude 5h window at 100%', 'warn'), g: 1),
      e(6, {'type': 'exited', 'code': 0, 'requested': true}, g: 1),
      e(6, note('resleeved g1 -> g2: claude -> pi_local (guardrail); brief written to '
          '.protectorate/BRIEF.md'), g: 2),
      e(6, {'type': 'session_info', 'harness': 'pi_local', 'mode': 'headless',
          'native_id': '9b2e-fuzz', 'model': 'qwen3-coder-30b-a3b', 'provider': 'llama.cpp'}, g: 2),
      e(7, say('Picked up from the brief: round 4 of frame_codec, corpus at 597 inputs.'), g: 2),
    ];

    _tx['s-herdr'] = [
      e(0, user('owner', 'Port the pane layout code and its tests, keep commits small.'), g: 1),
      e(1, say('Port done: 14 commits on main, all tests pass. The push to main needs your '
          'approval since main is protected; it was denied (a-30), so the commits are local.'),
          g: 1),
      e(1, settled(152000, 200000), g: 1),
    ];

    _tx['s-notes'] = [
      e(0, {'type': 'exited', 'signal': 11, 'requested': false,
          'stderr_tail': 'Segmentation fault (core dumped)'}, g: 2),
      e(0, note('G3 crash: exit 139 (SIGSEGV)', 'alarm'), g: 2),
      e(1, note('resleeved g2 -> g3: codex -> pi_local'), g: 3),
      e(2, {'type': 'exited', 'signal': 11, 'requested': false}, g: 3),
      e(2, note('G3 crash: exit 139 (SIGSEGV)', 'alarm'), g: 3),
      e(3, note('parked: third resleeve within an hour. Resleeve by hand to run it again.',
          'alarm'), g: 4),
    ];

    Json node(String id, String name, Json inv, Json? sample, Json presence,
        {bool online = true, int lastSeenAgoMin = 0}) => {
      'id': id,
      'name': name,
      'online': online,
      'last_seen_ms': _ago(Duration(minutes: lastSeenAgoMin)),
      'node_version': kVersion,
      'inventory': {...inv, 'collected_at_ms': _ago(const Duration(minutes: 6))},
      'sample': sample == null ? null : {...sample, 'sampled_at_ms': _now},
      'presence': {...presence, 'sampled_at_ms': _ago(Duration(minutes: lastSeenAgoMin))},
    };
    _nodes.addAll([
      node('n-arm64', 'build-arm64', {
        'hostname': 'build-arm64', 'os': 'linux', 'os_version': 'Arch Linux ARM (6.6.10)',
        'arch': 'arm64', 'cpu_model': 'CIX P1 CD8180', 'cpu_count': 12, 'ram_bytes': 64 * _gib,
        'gpus': [{'name': 'Mali-G720-Immortalis', 'driver': 'panthor', 'util_pct': 3.0}],
        'disks': [
          {'mount': '/', 'size_bytes': 931 * _gib, 'free_bytes': 512 * _gib},
          {'mount': '/home', 'size_bytes': 1863 * _gib, 'free_bytes': 1210 * _gib},
        ],
        'toolchains': [
          {'name': 'rustc', 'version': '1.91.0'}, {'name': 'cargo', 'version': '1.91.0'},
          {'name': 'go', 'version': '1.25.3'}, {'name': 'flutter', 'version': '3.44.1'},
          {'name': 'git', 'version': '2.51.1'},
        ],
      }, {'cpu_pct': 41.0, 'load1': 4.9, 'mem_avail_bytes': 38 * _gib, 'swap_used_bytes': 0},
          {'state': 'away', 'idle_secs': 11040, 'locked': true, 'source': 'logind'}),
      node('n-desk', 'desk-amd64', {
        'hostname': 'desk-amd64', 'os': 'linux', 'os_version': 'Fedora 43 (6.17.4)',
        'arch': 'amd64', 'cpu_model': 'AMD Ryzen 9 7950X', 'cpu_count': 32, 'ram_bytes': 96 * _gib,
        'gpus': [
          {'name': 'NVIDIA GeForce RTX 4070', 'driver': 'nvidia 580.95', 'vram_bytes': 12 * _gib,
            'util_pct': 18.0},
        ],
        'disks': [
          {'mount': '/', 'size_bytes': 1863 * _gib, 'free_bytes': 403 * _gib},
        ],
        'toolchains': [
          {'name': 'rustc', 'version': '1.91.0'}, {'name': 'go', 'version': '1.25.3'},
          {'name': 'node', 'version': '22.20.0'}, {'name': 'python', 'version': '3.14.0'},
          {'name': 'git', 'version': '2.51.0'},
        ],
      }, {'cpu_pct': 8.0, 'load1': 2.6, 'mem_avail_bytes': 71 * _gib, 'swap_used_bytes': 0},
          {'state': 'present', 'idle_secs': 40, 'locked': false, 'source': 'evdev'}),
      node('n-win', 'build-win', {
        'hostname': 'BUILD-WIN', 'os': 'windows', 'os_version': 'Windows 11 Pro 25H2',
        'arch': 'amd64', 'cpu_model': 'Intel Core i7-12700', 'cpu_count': 20, 'ram_bytes': 32 * _gib,
        'gpus': [{'name': 'Intel UHD Graphics 770', 'driver': '32.0.101.6314'}],
        'disks': [
          {'mount': 'C:', 'size_bytes': 953 * _gib, 'free_bytes': 88 * _gib},
          {'mount': 'D:', 'size_bytes': 3726 * _gib, 'free_bytes': 2900 * _gib},
        ],
        'toolchains': [
          {'name': 'rustc', 'version': '1.90.0'}, {'name': 'msvc', 'version': '17.14'},
          {'name': 'git', 'version': '2.51.0.windows.1'},
        ],
      }, null, {'state': 'unknown', 'source': 'wts'}, online: false, lastSeenAgoMin: 181),
    ]);

    _jobs.addAll([
      {
        'id': 'j-4f2a', 'node_id': 'n-arm64', 'target': {'type': 'arch', 'arch': 'arm64'},
        'argv': ['make', 'test'], 'state': 'succeeded',
        'requested_by': 'envoy', 'attempt': 1, 'queued_at_ms': at(30), 'started_at_ms': at(30),
        'ended_at_ms': at(34) - 48000, 'exit_code': 0,
      },
      {
        'id': 'j-4e91', 'node_id': 'n-desk', 'state': 'succeeded',
        'target': {'type': 'arch', 'arch': 'amd64'},
        'argv': ['cargo', 'build', '--release', '--workspace'], 'requested_by': 'protectorate',
        'attempt': 1, 'queued_at_ms': at(8), 'started_at_ms': at(8), 'ended_at_ms': at(12),
        'exit_code': 0,
      },
      {
        'id': 'j-4d10', 'node_id': 'n-win', 'state': 'lost',
        'target': {'type': 'node', 'node_id': 'n-win'}, 'maintenance': true,
        'argv': ['winget', 'upgrade', '--all', '--silent'], 'requested_by': 'envoy', 'attempt': 1,
        'queued_at_ms': _ago(const Duration(hours: 3, minutes: 5)),
        'started_at_ms': _ago(const Duration(hours: 3, minutes: 4)),
      },
      {
        'id': 'j-4c77', 'node_id': 'n-arm64', 'state': 'failed',
        'target': {'type': 'arch', 'arch': 'arm64'}, 'argv': ['make', 'test'],
        'requested_by': 'la/app', 'attempt': 1,
        'queued_at_ms': _ago(const Duration(hours: 6)),
        'started_at_ms': _ago(const Duration(hours: 6)),
        'ended_at_ms': _ago(const Duration(hours: 5, minutes: 57)), 'exit_code': 2,
      },
    ]);

    _quotas.addAll([
      {
        'subscription': 'claude', 'plan': 'max-20x', 'reading': 'ok', 'fetched_at_ms': _now,
        'windows': [
          {'label': '5h', 'used_pct': 62.0, 'resets_at_ms': _now + const Duration(hours: 2, minutes: 11).inMilliseconds},
          {'label': 'wk', 'used_pct': 41.0, 'resets_at_ms': _now + const Duration(days: 4, hours: 3).inMilliseconds},
        ],
      },
      {
        'subscription': 'codex', 'plan': 'pro', 'reading': 'ok',
        'fetched_at_ms': _ago(const Duration(minutes: 1)),
        'windows': [
          {'label': '5h', 'used_pct': 18.0, 'resets_at_ms': _now + const Duration(hours: 4, minutes: 2).inMilliseconds},
          {'label': 'wk', 'used_pct': 88.0, 'resets_at_ms': _now + const Duration(days: 2, hours: 7).inMilliseconds},
        ],
      },
      {
        'subscription': 'grok', 'plan': 'supergrok-heavy', 'reading': 'stale',
        'fetched_at_ms': _ago(const Duration(minutes: 38)),
        'note': 'reader timed out; showing the last good value',
        'windows': [
          {'label': 'wk', 'used_pct': 33.0, 'resets_at_ms': _now + const Duration(days: 5, hours: 1).inMilliseconds},
        ],
      },
      {
        'subscription': 'local', 'plan': 'llama.cpp (local)', 'reading': 'ok',
        'fetched_at_ms': _now, 'note': 'unmetered; 1 slot of 2 busy',
        'windows': [{'label': 'slots'}],
      },
    ]);

    void ev(int min, String kind, String src, String text,
        {String? target, String level = 'info', bool read = true}) {
      _events.add({
        'seq': ++_eventSeq, 'at_ms': at(min), 'kind': kind, 'source': src, 'target': ?target,
        'text': text, 'read': read, 'level': level,
      });
    }

    ev(0, 'hub', 'protectorate', 'hub update check: $kVersion is current');
    ev(2, 'approval', 'protectorate', 'asks: Bash cargo +nightly fuzz build (a-29)', level: 'warn');
    ev(3, 'approval', 'owner', 'approved a-29', target: 'protectorate');
    ev(6, 'guardrail', 'fuzz-wire', 'G1 quota_exhausted (claude 5h) -> resleeve to pi_local',
        level: 'warn');
    ev(9, 'approval', 'owner', 'denied a-30 (git push origin main)', target: 'herdr-port');
    ev(12, 'job', 'j-4e91', 'exited 0 (desk-amd64) in 4m02s');
    ev(18, 'guardrail', 'notes-site', 'parked: third resleeve within an hour', level: 'alarm');
    ev(30, 'job', 'j-4f2a', 'queued on build-arm64: make test');
    ev(34, 'job', 'j-4f2a', 'exited 0 (build-arm64) in 3m12s');
    ev(36, 'node', 'build-win', 'unreachable for 3h; presence UNKNOWN', level: 'alarm', read: false);
    ev(44, 'mail', 'la/app', 'request: approve push of la/app', target: 'envoy', read: false);
    ev(44, 'approval', 'la/app', 'asks: Bash git push origin la/app (a-31)', level: 'warn',
        read: false);
    ev(47, 'approval', 'envoy', 'asks: node_job pacman -Syu on build-arm64 (a-32)', level: 'warn',
        read: false);
  }
}

class _Step {
  _Step(this.due, this.run, {this.sleeve, this.gen = 0});

  final int due;
  final void Function() run;
  final String? sleeve;
  final int gen;
}

class _Stream {
  _Stream(this.sleeve, this.entry, this.words, this.then);

  final String sleeve;
  final Json entry;
  final List<String> words;
  final void Function()? then;
  int i = 0;
}
