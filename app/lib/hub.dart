import 'api/api.g.dart';

/// This build's version: the git tag, passed by the Makefile as `--dart-define`.
const kVersion = String.fromEnvironment('PROTECTORATE_VERSION', defaultValue: 'dev');

/// What the app needs from a hub: state as streams, actions as futures. Each action maps onto one
/// hub API method (`POST /api/<Method>`); the streams onto `GET /api/events`.
abstract class HubClient {
  /// Where the client points and whether it is connected or a scripted stand-in.
  String get label;

  /// False while a real hub is unreachable.
  bool get connected;

  HubState get state;
  Stream<HubState> get states;

  /// The transcript as known now; [transcripts] brings each change.
  Transcript transcript(String sleeveId);
  Stream<Transcript> transcripts(String sleeveId);

  Future<void> send(String sleeveId, String text);
  Future<void> interrupt(String sleeveId);
  Future<void> stop(String sleeveId);
  Future<void> resleeve(String sleeveId, Harness target);
  Future<void> decide(String approvalId, Decision decision);
  Future<void> markEventsRead();

  void close();
}
