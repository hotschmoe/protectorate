//
//  Generated code. Do not modify.
//  source: protectorate/v1/job.proto
//

import "package:connectrpc/connect.dart" as connect;
import "job.pb.dart" as protectoratev1job;
import "job.connect.spec.dart" as specs;

/// JobService runs commands on nodes. The hub picks the node, ships the source
/// and streams the logs; the node executes. Jobs are kept under
/// $PROTECTORATE_HOME/jobs/<id>/ (job.json, job.log, artifacts/) and announced
/// on EventService as "job.updated" (subject = job id). A running job survives a
/// hub restart: the node keeps it, and the restarted hub reconciles with the
/// node and reattaches the log stream. It is marked failed only when the node
/// reports it never had the job.
/// A job executes arbitrary commands as the node agent's user, so the scopes
/// that admit SubmitJob are as powerful as shell access to every node.
extension type JobServiceClient (connect.Transport _transport) {
  /// SubmitJob queues a job and returns at once with its id. Follow it with
  /// WatchJob.
  Future<protectoratev1job.Job> submitJob(
    protectoratev1job.SubmitJobRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.JobService.submitJob,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// GetJob returns one job record.
  Future<protectoratev1job.Job> getJob(
    protectoratev1job.GetJobRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.JobService.getJob,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// ListJobs returns job records, newest first.
  Future<protectoratev1job.ListJobsResponse> listJobs(
    protectoratev1job.ListJobsRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.JobService.listJobs,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// CancelJob stops a job and its whole process tree, named by id or by the
  /// SubmitJob request_id. Cancelling a finished job is a no-op that returns
  /// its record.
  Future<protectoratev1job.Job> cancelJob(
    protectoratev1job.CancelJobRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.JobService.cancelJob,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// WatchJob replays a job's log from an offset and, when follow is set,
  /// streams new output and state changes until the job ends.
  Stream<protectoratev1job.WatchJobResponse> watchJob(
    protectoratev1job.WatchJobRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).server(
      specs.JobService.watchJob,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }
}
