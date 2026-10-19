import 'dart:async';
import 'dart:math' as math;

import 'package:connectrpc/connect.dart';
import 'package:flutter/foundation.dart';

import '../gen/protectorate/v1/backlog.pb.dart';
import '../hubs/hub_client.dart';

enum SaveState { saved, edited, saving, offline, rejected }

const saveDebounce = Duration(milliseconds: 800);
const saveRetry = Duration(seconds: 2);
const _maxRetry = Duration(seconds: 30);

// One note being edited. It owns the autosave: edits wait out a short pause,
// then go to the hub; a failed save stays dirty and retries with backoff until
// it lands, so a dropped connection loses nothing as long as the app lives.
// It outlives the editor widget so that closing the editor never drops a save.
class NoteDraft extends ChangeNotifier {
  NoteDraft(
    this.client, {
    Note? note,
    this.onSaved,
    this.debounce = saveDebounce,
    this.retry = saveRetry,
  }) {
    if (note != null) _load(note);
  }

  final HubClient client;
  final void Function(Note)? onSaved;
  final Duration debounce;
  final Duration retry;

  String? id;
  String title = '';
  String body = '';
  String project = '';
  bool pinned = false;
  String source = '';
  DateTime? created;
  DateTime? updated;
  SaveState state = SaveState.saved;
  String? error;

  final _dirty = <String>{};
  Timer? _timer;
  bool _saving = false;
  bool _disposed = false;
  int _rev = 0;
  int _failures = 0;

  bool get hasUnsaved => _dirty.isNotEmpty || _saving;

  bool get blank => title.trim().isEmpty && body.trim().isEmpty;

  void _load(Note n) {
    id = n.id;
    title = n.title;
    body = n.body;
    project = n.project;
    pinned = n.pinned;
    source = n.source;
    created = n.hasCreatedAt() ? n.createdAt.toDateTime() : null;
    updated = n.hasUpdatedAt() ? n.updatedAt.toDateTime() : null;
  }

  Note snapshot() => Note(
    id: id,
    title: title,
    body: body,
    project: project,
    pinned: pinned,
    source: source,
  );

  void edit({String? title, String? body, String? project, bool? pinned}) {
    if (title != null && title != this.title) {
      this.title = title;
      _dirty.add('title');
    }
    if (body != null && body != this.body) {
      this.body = body;
      _dirty.add('body');
    }
    if (project != null && project != this.project) {
      this.project = project;
      _dirty.add('project');
    }
    if (pinned != null && pinned != this.pinned) {
      this.pinned = pinned;
      _dirty.add('pinned');
    }
    if (_dirty.isEmpty) return;
    _rev++;
    _failures = 0;
    state = _saving ? SaveState.saving : SaveState.edited;
    error = null;
    _notify();
    _schedule(pinned != null ? Duration.zero : debounce);
  }

  // Takes the hub's copy unless there are local edits, which win.
  void adopt(Note n) {
    if (hasUnsaved) return;
    final at = n.hasUpdatedAt() ? n.updatedAt.toDateTime() : null;
    if (at == updated && n.id == id) return;
    _load(n);
    state = SaveState.saved;
    _notify();
  }

  void _schedule(Duration d) {
    _timer?.cancel();
    if (_disposed) return;
    _timer = Timer(d, flush);
  }

  Future<void> flush() async {
    _timer?.cancel();
    if (_saving || _dirty.isEmpty) return;
    if (id == null && blank) {
      _dirty.clear();
      state = SaveState.saved;
      _notify();
      return;
    }
    _saving = true;
    state = SaveState.saving;
    _notify();
    final sent = Set.of(_dirty);
    final rev = _rev;
    try {
      final Note n;
      if (id == null) {
        n = (await client.backlog.createNote(
          CreateNoteRequest(
            title: title,
            body: body,
            project: project,
            pinned: pinned,
          ),
        )).note;
      } else {
        n = (await client.backlog.updateNote(
          UpdateNoteRequest(
            id: id,
            title: sent.contains('title') ? title : null,
            body: sent.contains('body') ? body : null,
            project: sent.contains('project') ? project : null,
            pinned: sent.contains('pinned') ? pinned : null,
          ),
        )).note;
      }
      id = n.id;
      source = n.source;
      created = n.createdAt.toDateTime();
      updated = n.updatedAt.toDateTime();
      _failures = 0;
      if (rev == _rev) _dirty.clear();
      state = _dirty.isEmpty ? SaveState.saved : SaveState.edited;
      onSaved?.call(n);
      if (_dirty.isNotEmpty) _schedule(Duration.zero);
    } on ConnectException catch (e) {
      if (e.code == Code.invalidArgument || e.code == Code.notFound) {
        error = e.message;
        state = SaveState.rejected;
      } else {
        _failed(e.message);
      }
    } on Object catch (e) {
      _failed('$e');
    } finally {
      _saving = false;
      _notify();
    }
  }

  void _failed(String why) {
    _failures++;
    error = why;
    state = SaveState.offline;
    final wait = retry * math.pow(2, _failures - 1);
    _schedule(wait > _maxRetry ? _maxRetry : wait);
  }

  void _notify() {
    if (!_disposed) notifyListeners();
  }

  @override
  void dispose() {
    _disposed = true;
    _timer?.cancel();
    super.dispose();
  }
}
