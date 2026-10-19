//
//  Generated code. Do not modify.
//  source: protectorate/v1/job.proto
//

import "package:connectrpc/connect.dart" as connect;
import "job.pb.dart" as protectoratev1job;

/// JobService runs commands on nodes. The hub picks the node, ships the source
/// and streams the logs; the node executes. Jobs are kept under
/// $PROTECTORATE_HOME/jobs/<id>/ (job.json, job.log, artifacts/) and announced
/// on EventService as "job.updated" (subject = job id). A running job is lost
/// when the hub restarts and is then marked failed.
/// A job executes arbitrary commands as the node agent's user, so the scopes
/// that admit SubmitJob are as powerful as shell access to every node.
abstract final class JobService {
  /// Fully-qualified name of the JobService service.
  static const name = 'protectorate.v1.JobService';

  /// SubmitJob queues a job and returns at once with its id. Follow it with
  /// WatchJob.
  static const submitJob = connect.Spec(
    '/$name/SubmitJob',
    connect.StreamType.unary,
    protectoratev1job.SubmitJobRequest.new,
    protectoratev1job.Job.new,
  );

  /// GetJob returns one job record.
  static const getJob = connect.Spec(
    '/$name/GetJob',
    connect.StreamType.unary,
    protectoratev1job.GetJobRequest.new,
    protectoratev1job.Job.new,
    idempotency: connect.Idempotency.noSideEffects,
  );

  /// ListJobs returns job records, newest first.
  static const listJobs = connect.Spec(
    '/$name/ListJobs',
    connect.StreamType.unary,
    protectoratev1job.ListJobsRequest.new,
    protectoratev1job.ListJobsResponse.new,
    idempotency: connect.Idempotency.noSideEffects,
  );

  /// CancelJob stops a job and its whole process tree. Cancelling a finished
  /// job is a no-op that returns its record.
  static const cancelJob = connect.Spec(
    '/$name/CancelJob',
    connect.StreamType.unary,
    protectoratev1job.CancelJobRequest.new,
    protectoratev1job.Job.new,
  );

  /// WatchJob replays a job's log from an offset and, when follow is set,
  /// streams new output and state changes until the job ends.
  static const watchJob = connect.Spec(
    '/$name/WatchJob',
    connect.StreamType.server,
    protectoratev1job.WatchJobRequest.new,
    protectoratev1job.WatchJobResponse.new,
    idempotency: connect.Idempotency.noSideEffects,
  );
}
