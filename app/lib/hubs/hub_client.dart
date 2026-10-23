import 'package:connectrpc/connect.dart';
import 'package:connectrpc/protobuf.dart';
import 'package:connectrpc/protocol/connect.dart' as protocol;

import '../gen/protectorate/v1/audit.connect.client.dart';
import '../gen/protectorate/v1/backlog.connect.client.dart';
import '../gen/protectorate/v1/chat.connect.client.dart';
import '../gen/protectorate/v1/desk.connect.client.dart';
import '../gen/protectorate/v1/directory.connect.client.dart';
import '../gen/protectorate/v1/device.connect.client.dart';
import '../gen/protectorate/v1/events.connect.client.dart';
import '../gen/protectorate/v1/hub.connect.client.dart';
import '../gen/protectorate/v1/lease.connect.client.dart';
import '../gen/protectorate/v1/message.connect.client.dart';
import '../gen/protectorate/v1/node.connect.client.dart';
import '../gen/protectorate/v1/quota.connect.client.dart';
import '../gen/protectorate/v1/release.connect.client.dart';
import '../gen/protectorate/v1/session.connect.client.dart';
import '../gen/protectorate/v1/sleeve.connect.client.dart';
import 'hub_profile.dart';
import 'connection.dart';
import 'http_client_io.dart'
    if (dart.library.js_interop) 'http_client_web.dart';

Interceptor bearerInterceptor(String token) {
  return <I extends Object, O extends Object>(AnyFn<I, O> next) =>
      (Request<I, O> req) {
        if (token.isNotEmpty) {
          req.headers.set('authorization', ['Bearer $token']);
        }
        return next(req);
      };
}

class HubClient {
  HubClient(HubProfile profile, {HttpClient? httpClient})
    : this.withTransport(
        protocol.Transport(
          baseUrl: profile.baseUrl,
          codec: const JsonCodec(),
          httpClient: hubHttpClient(
            profile.baseUrl,
            client:
                httpClient ?? createPlatformHttpClient(caPem: profile.caPem),
          ),
          interceptors: [bearerInterceptor(profile.token)],
        ),
      );

  HubClient.withTransport(Transport transport)
    : hub = HubServiceClient(transport),
      events = EventServiceClient(transport),
      node = NodeServiceClient(transport),
      quota = QuotaServiceClient(transport),
      releases = ReleaseServiceClient(transport),
      sessions = SessionServiceClient(transport),
      sleeves = SleeveServiceClient(transport),
      backlog = BacklogServiceClient(transport),
      chat = ChatServiceClient(transport),
      messages = MessageServiceClient(transport),
      directory = DirectoryServiceClient(transport),
      devices = DeviceServiceClient(transport),
      desk = DeskServiceClient(transport),
      audit = AuditServiceClient(transport),
      leases = LeaseServiceClient(transport);

  final HubServiceClient hub;
  final EventServiceClient events;
  final NodeServiceClient node;
  final QuotaServiceClient quota;
  final ReleaseServiceClient releases;
  final SessionServiceClient sessions;
  final SleeveServiceClient sleeves;
  final BacklogServiceClient backlog;
  final ChatServiceClient chat;
  final MessageServiceClient messages;
  final DirectoryServiceClient directory;
  final DeviceServiceClient devices;
  final DeskServiceClient desk;
  final AuditServiceClient audit;
  final LeaseServiceClient leases;
}
