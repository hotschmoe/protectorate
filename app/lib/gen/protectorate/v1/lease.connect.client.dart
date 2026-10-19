//
//  Generated code. Do not modify.
//  source: protectorate/v1/lease.proto
//

import "package:connectrpc/connect.dart" as connect;
import "lease.pb.dart" as protectoratev1lease;
import "lease.connect.spec.dart" as specs;

/// LeaseService reserves nodes and GPUs for a holder. A lease that is not
/// renewed expires on its own, so a dead agent cannot hold hardware forever.
/// Leases are kept across hub restarts and announced on EventService as
/// "lease.acquired", "lease.renewed", "lease.released" and "lease.expired"
/// (subject = lease id).
extension type LeaseServiceClient (connect.Transport _transport) {
  /// AcquireLease reserves one resource exclusively: a node name ("win1"), one
  /// GPU ("gpu-box:gpu1") or whatever a selector picks. A selector chooses the
  /// least-loaded fresh matching node that is not already leased.
  Future<protectoratev1lease.Lease> acquireLease(
    protectoratev1lease.AcquireLeaseRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.LeaseService.acquireLease,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// RenewLease pushes the expiry of a live lease to now plus a TTL.
  Future<protectoratev1lease.Lease> renewLease(
    protectoratev1lease.RenewLeaseRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.LeaseService.renewLease,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// ReleaseLease frees a lease at once. Releasing an unknown or expired lease
  /// is an error.
  Future<protectoratev1lease.ReleaseLeaseResponse> releaseLease(
    protectoratev1lease.ReleaseLeaseRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.LeaseService.releaseLease,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// ListLeases returns every live lease.
  Future<protectoratev1lease.ListLeasesResponse> listLeases(
    protectoratev1lease.ListLeasesRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.LeaseService.listLeases,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }
}
