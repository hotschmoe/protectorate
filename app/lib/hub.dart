import 'model.dart';

/// What the app needs from a hub: state as streams, actions as futures. Each action maps onto one
/// hub API method (`POST /api/<Method>`); the streams onto `GET /api/events`.
abstract class HubClient {
  /// Where the client points and whether it is a scripted stand-in.
  String get label;

  HubState get state;
  Stream<HubState> get states;

  Transcript transcript(String sleeveId);
  Stream<Transcript> transcripts(String sleeveId);

  Future<void> send(String sleeveId, String text);
  Future<void> interrupt(String sleeveId);
  Future<void> stop(String sleeveId);
  Future<void> resleeve(String sleeveId, Harness target);
  Future<void> decide(String approvalId, {required bool approve});
  Future<void> markEventsRead();

  void close();
}
