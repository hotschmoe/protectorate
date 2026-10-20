import 'dart:async';
import 'dart:typed_data';

import 'package:connectrpc/connect.dart';

import '../gen/protectorate/v1/hub.connect.client.dart';
import '../gen/protectorate/v1/hub.pb.dart';
import 'http_client_io.dart'
    if (dart.library.js_interop) 'http_client_web.dart';

String hubConnectionMessage(Object error, String baseUrl) {
  final detail = '$error'.toLowerCase();
  final reason =
      error is TimeoutException ||
          detail.contains('timed out') ||
          detail.contains('timeout') ||
          (error is ConnectException && error.code == Code.deadlineExceeded)
      ? 'Connection timed out'
      : detail.contains('refused')
      ? 'Connection refused'
      : 'Cannot reach hub';
  final uri = Uri.parse(baseUrl);
  return '$reason at ${uri.origin}${uri.path}.';
}

HttpClient hubHttpClient(String baseUrl, {HttpClient? client}) {
  final send = client ?? createPlatformHttpClient();
  Object failure(Object error) {
    if (error is ConnectException && error.code == Code.canceled) return error;
    return ConnectException(
      error is TimeoutException ? Code.deadlineExceeded : Code.unavailable,
      hubConnectionMessage(error, baseUrl),
      cause: error,
    );
  }

  return (request) async {
    try {
      final response = await send(request);
      return HttpResponse(
        response.status,
        response.header,
        response.body.transform(
          StreamTransformer<Uint8List, Uint8List>.fromHandlers(
            handleError: (error, stack, sink) =>
                sink.addError(failure(error), stack),
          ),
        ),
        response.trailer,
      );
    } on Object catch (error, stack) {
      Error.throwWithStackTrace(failure(error), stack);
    }
  };
}

class HubProbeException implements Exception {
  const HubProbeException(this.message);
  final String message;

  @override
  String toString() => message;
}

Future<void> probeHub(
  HubServiceClient client,
  String baseUrl, {
  Duration timeout = const Duration(seconds: 8),
}) async {
  final abort = CancelableSignal();
  try {
    final status = await client
        .getStatus(GetStatusRequest(), signal: abort)
        .timeout(timeout);
    if (status.version.isEmpty || status.hostname.isEmpty) {
      throw const HubProbeException('No Protectorate hub responded');
    }
  } on Object catch (error) {
    final uri = Uri.parse(baseUrl);
    final host = uri.host.contains(':') ? '[${uri.host}]' : uri.host;
    final destination = '$host:${uri.port}';
    final hint = uri.hasPort ? '' : ' The hub default is :7470.';
    final reason =
        error is ConnectException &&
            (error.code == Code.unauthenticated ||
                error.code == Code.permissionDenied)
        ? 'The hub rejected the token'
        : error is HubProbeException
        ? error.message
        : 'Could not connect to the hub';
    throw HubProbeException(
      '$reason at $destination (${uri.origin}${uri.path}).$hint',
    );
  } finally {
    abort.cancel();
  }
}
