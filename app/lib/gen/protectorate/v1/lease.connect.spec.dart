//
//  Generated code. Do not modify.
//  source: protectorate/v1/lease.proto
//

import "package:connectrpc/connect.dart" as connect;
import "lease.pb.dart" as protectoratev1lease;

/// LeaseService reserves nodes and GPUs for a holder. A lease that is not
/// renewed expires on its own, so a dead agent cannot hold hardware forever.
/// Leases are kept across hub restarts and announced on EventService as
/// "lease.acquired", "lease.renewed", "lease.released" and "lease.expired"
/// (subject = lease id).
abstract final class LeaseService {
  /// Fully-qualified name of the LeaseService service.
  static const name = 'protectorate.v1.LeaseService';

  /// AcquireLease reserves one resource exclusively: a node name ("win1"), one
  /// GPU ("gpu-box:gpu1") or whatever a selector picks. A selector chooses the
  /// least-loaded fresh matching node that is not already leased.
  static const acquireLease = connect.Spec(
    '/$name/AcquireLease',
    connect.StreamType.unary,
    protectoratev1lease.AcquireLeaseRequest.new,
    protectoratev1lease.Lease.new,
  );

  /// RenewLease pushes the expiry of a live lease to now plus a TTL.
  static const renewLease = connect.Spec(
    '/$name/RenewLease',
    connect.StreamType.unary,
    protectoratev1lease.RenewLeaseRequest.new,
    protectoratev1lease.Lease.new,
  );

  /// ReleaseLease frees a lease at once. Releasing an unknown or expired lease
  /// is an error.
  static const releaseLease = connect.Spec(
    '/$name/ReleaseLease',
    connect.StreamType.unary,
    protectoratev1lease.ReleaseLeaseRequest.new,
    protectoratev1lease.ReleaseLeaseResponse.new,
  );

  /// ListLeases returns every live lease.
  static const listLeases = connect.Spec(
    '/$name/ListLeases',
    connect.StreamType.unary,
    protectoratev1lease.ListLeasesRequest.new,
    protectoratev1lease.ListLeasesResponse.new,
    idempotency: connect.Idempotency.noSideEffects,
  );
}
