// This is a generated file - do not edit.
//
// Generated from protectorate/v1/job.proto.

// @dart = 3.3

// ignore_for_file: annotate_overrides, camel_case_types, comment_references
// ignore_for_file: constant_identifier_names
// ignore_for_file: curly_braces_in_flow_control_structures
// ignore_for_file: deprecated_member_use_from_same_package, library_prefixes
// ignore_for_file: non_constant_identifier_names, prefer_relative_imports

import 'dart:async' as $async;
import 'dart:core' as $core;

import 'package:fixnum/fixnum.dart' as $fixnum;
import 'package:protobuf/protobuf.dart' as $pb;
import 'package:protobuf/well_known_types/google/protobuf/duration.pb.dart'
    as $1;
import 'package:protobuf/well_known_types/google/protobuf/timestamp.pb.dart'
    as $2;

import 'job.pbenum.dart';
import 'lease.pb.dart' as $0;

export 'package:protobuf/protobuf.dart' show GeneratedMessageGenericExtensions;

export 'job.pbenum.dart';

/// JobIsolation is what a node can do for jobs, reported in its metrics.
class JobIsolation extends $pb.GeneratedMessage {
  factory JobIsolation({
    $core.String? sandbox,
    $core.String? note,
    $core.String? admin,
  }) {
    final result = JobIsolation._();
    if (sandbox != null) result.sandbox = sandbox;
    if (note != null) result.note = note;
    if (admin != null) result.admin = admin;
    return result;
  }

  JobIsolation._();

  factory JobIsolation.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      JobIsolation()..mergeFromBuffer(data, registry);
  factory JobIsolation.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      JobIsolation()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'JobIsolation',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: JobIsolation.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'sandbox')
    ..aOS(2, _omitFieldNames ? '' : 'note')
    ..aOS(3, _omitFieldNames ? '' : 'admin')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  JobIsolation clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  JobIsolation copyWith(void Function(JobIsolation) updates) =>
      super.copyWith((message) => updates(message as JobIsolation))
          as JobIsolation;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use JobIsolation() / JobIsolation.new instead')
  static JobIsolation create() => JobIsolation._();
  static $pb.GeneratedMessage $_createMessage() => JobIsolation._();
  @$core.override
  JobIsolation createEmptyInstance() => JobIsolation._();
  @$core.pragma('dart2js:noInline')
  static JobIsolation getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<JobIsolation>(
          JobIsolation.$_createMessage);
  static JobIsolation? _defaultInstance;

  /// sandbox is how JOB_MODE_SANDBOXED jobs are isolated: "rust-namespaces"
  /// (Linux: a direct namespace launcher in a systemd user scope), "runner"
  /// (Windows: the protectorate-runner account in a job object) or "none".
  @$pb.TagNumber(1)
  $core.String get sandbox => $_getSZ(0);
  @$pb.TagNumber(1)
  set sandbox($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasSandbox() => $_has(0);
  @$pb.TagNumber(1)
  void clearSandbox() => $_clearField(1);

  /// note says why isolation is missing or partial.
  @$pb.TagNumber(2)
  $core.String get note => $_getSZ(1);
  @$pb.TagNumber(2)
  set note($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasNote() => $_has(1);
  @$pb.TagNumber(2)
  void clearNote() => $_clearField(2);

  /// admin is how admin jobs elevate on a Linux node: "sudo" or "root"; empty
  /// when this node cannot run them, always on Windows and Termux.
  @$pb.TagNumber(3)
  $core.String get admin => $_getSZ(2);
  @$pb.TagNumber(3)
  set admin($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasAdmin() => $_has(2);
  @$pb.TagNumber(3)
  void clearAdmin() => $_clearField(3);
}

enum JobTarget_Target { leaseId, node, selector, notSet }

/// JobTarget says where a job runs. Nodes of tier "care" are never picked
/// by a selector or leased; naming one needs the owner token or a token with
/// the scope node_admin:<node>.
class JobTarget extends $pb.GeneratedMessage {
  factory JobTarget({
    $core.String? leaseId,
    $core.String? node,
    $0.NodeSelector? selector,
  }) {
    final result = JobTarget._();
    if (leaseId != null) result.leaseId = leaseId;
    if (node != null) result.node = node;
    if (selector != null) result.selector = selector;
    return result;
  }

  JobTarget._();

  factory JobTarget.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      JobTarget()..mergeFromBuffer(data, registry);
  factory JobTarget.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      JobTarget()..mergeFromJson(json, registry);

  static const $core.Map<$core.int, JobTarget_Target> _JobTarget_TargetByTag = {
    1: JobTarget_Target.leaseId,
    2: JobTarget_Target.node,
    3: JobTarget_Target.selector,
    0: JobTarget_Target.notSet
  };
  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'JobTarget',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: JobTarget.$_createMessage)
    ..oo(0, [1, 2, 3])
    ..aOS(1, _omitFieldNames ? '' : 'leaseId')
    ..aOS(2, _omitFieldNames ? '' : 'node')
    ..aOM<$0.NodeSelector>(3, _omitFieldNames ? '' : 'selector',
        subBuilder: $0.NodeSelector.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  JobTarget clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  JobTarget copyWith(void Function(JobTarget) updates) =>
      super.copyWith((message) => updates(message as JobTarget)) as JobTarget;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use JobTarget() / JobTarget.new instead')
  static JobTarget create() => JobTarget._();
  static $pb.GeneratedMessage $_createMessage() => JobTarget._();
  @$core.override
  JobTarget createEmptyInstance() => JobTarget._();
  @$core.pragma('dart2js:noInline')
  static JobTarget getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<JobTarget>(JobTarget.$_createMessage);
  static JobTarget? _defaultInstance;

  @$pb.TagNumber(1)
  @$pb.TagNumber(2)
  @$pb.TagNumber(3)
  JobTarget_Target whichTarget() => _JobTarget_TargetByTag[$_whichOneof(0)]!;
  @$pb.TagNumber(1)
  @$pb.TagNumber(2)
  @$pb.TagNumber(3)
  void clearTarget() => $_clearField($_whichOneof(0));

  /// lease_id runs on the node of a live lease (and on its GPU, if any).
  @$pb.TagNumber(1)
  $core.String get leaseId => $_getSZ(0);
  @$pb.TagNumber(1)
  set leaseId($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasLeaseId() => $_has(0);
  @$pb.TagNumber(1)
  void clearLeaseId() => $_clearField(1);

  /// node is a node name.
  @$pb.TagNumber(2)
  $core.String get node => $_getSZ(1);
  @$pb.TagNumber(2)
  set node($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasNode() => $_has(1);
  @$pb.TagNumber(2)
  void clearNode() => $_clearField(2);

  /// selector picks the least-loaded fresh matching node of tier "shared"
  /// that is not leased.
  @$pb.TagNumber(3)
  $0.NodeSelector get selector => $_getN(2);
  @$pb.TagNumber(3)
  set selector($0.NodeSelector value) => $_setField(3, value);
  @$pb.TagNumber(3)
  $core.bool hasSelector() => $_has(2);
  @$pb.TagNumber(3)
  void clearSelector() => $_clearField(3);
  @$pb.TagNumber(3)
  $0.NodeSelector ensureSelector() => $_ensure(2);
}

/// GitSource ships one commit of a repository on the hub.
class GitSource extends $pb.GeneratedMessage {
  factory GitSource({
    $core.String? repo,
    $core.String? commit,
  }) {
    final result = GitSource._();
    if (repo != null) result.repo = repo;
    if (commit != null) result.commit = commit;
    return result;
  }

  GitSource._();

  factory GitSource.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      GitSource()..mergeFromBuffer(data, registry);
  factory GitSource.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      GitSource()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'GitSource',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: GitSource.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'repo')
    ..aOS(2, _omitFieldNames ? '' : 'commit')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  GitSource clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  GitSource copyWith(void Function(GitSource) updates) =>
      super.copyWith((message) => updates(message as GitSource)) as GitSource;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use GitSource() / GitSource.new instead')
  static GitSource create() => GitSource._();
  static $pb.GeneratedMessage $_createMessage() => GitSource._();
  @$core.override
  GitSource createEmptyInstance() => GitSource._();
  @$core.pragma('dart2js:noInline')
  static GitSource getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<GitSource>(GitSource.$_createMessage);
  static GitSource? _defaultInstance;

  /// repo is a path to a git repository on the hub.
  @$pb.TagNumber(1)
  $core.String get repo => $_getSZ(0);
  @$pb.TagNumber(1)
  set repo($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasRepo() => $_has(0);
  @$pb.TagNumber(1)
  void clearRepo() => $_clearField(1);

  /// commit is any revision the repository resolves; empty means HEAD. The job
  /// record holds the full id.
  @$pb.TagNumber(2)
  $core.String get commit => $_getSZ(1);
  @$pb.TagNumber(2)
  set commit($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasCommit() => $_has(1);
  @$pb.TagNumber(2)
  void clearCommit() => $_clearField(2);
}

/// ArtifactSource copies a file or a directory from the hub into a fresh job
/// directory. A file is marked executable. A directory is shipped as a zip and
/// extracted with its relative layout and permission bits.
class ArtifactSource extends $pb.GeneratedMessage {
  factory ArtifactSource({
    $core.String? path,
    $core.String? workdir,
  }) {
    final result = ArtifactSource._();
    if (path != null) result.path = path;
    if (workdir != null) result.workdir = workdir;
    return result;
  }

  ArtifactSource._();

  factory ArtifactSource.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ArtifactSource()..mergeFromBuffer(data, registry);
  factory ArtifactSource.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ArtifactSource()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ArtifactSource',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ArtifactSource.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'path')
    ..aOS(2, _omitFieldNames ? '' : 'workdir')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ArtifactSource clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ArtifactSource copyWith(void Function(ArtifactSource) updates) =>
      super.copyWith((message) => updates(message as ArtifactSource))
          as ArtifactSource;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use ArtifactSource() / ArtifactSource.new instead')
  static ArtifactSource create() => ArtifactSource._();
  static $pb.GeneratedMessage $_createMessage() => ArtifactSource._();
  @$core.override
  ArtifactSource createEmptyInstance() => ArtifactSource._();
  @$core.pragma('dart2js:noInline')
  static ArtifactSource getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<ArtifactSource>(
          ArtifactSource.$_createMessage);
  static ArtifactSource? _defaultInstance;

  /// path is a file or directory on the hub.
  @$pb.TagNumber(1)
  $core.String get path => $_getSZ(0);
  @$pb.TagNumber(1)
  set path($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasPath() => $_has(0);
  @$pb.TagNumber(1)
  void clearPath() => $_clearField(1);

  /// workdir is the directory, relative to the job directory, the command runs
  /// in; empty means the job directory itself. Directories only.
  @$pb.TagNumber(2)
  $core.String get workdir => $_getSZ(1);
  @$pb.TagNumber(2)
  set workdir($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasWorkdir() => $_has(1);
  @$pb.TagNumber(2)
  void clearWorkdir() => $_clearField(2);
}

enum JobSource_Kind { git, artifact, notSet }

/// JobSource is what the command runs against. Without a source the command
/// runs in an empty job directory.
class JobSource extends $pb.GeneratedMessage {
  factory JobSource({
    GitSource? git,
    ArtifactSource? artifact,
  }) {
    final result = JobSource._();
    if (git != null) result.git = git;
    if (artifact != null) result.artifact = artifact;
    return result;
  }

  JobSource._();

  factory JobSource.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      JobSource()..mergeFromBuffer(data, registry);
  factory JobSource.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      JobSource()..mergeFromJson(json, registry);

  static const $core.Map<$core.int, JobSource_Kind> _JobSource_KindByTag = {
    1: JobSource_Kind.git,
    2: JobSource_Kind.artifact,
    0: JobSource_Kind.notSet
  };
  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'JobSource',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: JobSource.$_createMessage)
    ..oo(0, [1, 2])
    ..aOM<GitSource>(1, _omitFieldNames ? '' : 'git',
        subBuilder: GitSource.$_createMessage)
    ..aOM<ArtifactSource>(2, _omitFieldNames ? '' : 'artifact',
        subBuilder: ArtifactSource.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  JobSource clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  JobSource copyWith(void Function(JobSource) updates) =>
      super.copyWith((message) => updates(message as JobSource)) as JobSource;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use JobSource() / JobSource.new instead')
  static JobSource create() => JobSource._();
  static $pb.GeneratedMessage $_createMessage() => JobSource._();
  @$core.override
  JobSource createEmptyInstance() => JobSource._();
  @$core.pragma('dart2js:noInline')
  static JobSource getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<JobSource>(JobSource.$_createMessage);
  static JobSource? _defaultInstance;

  @$pb.TagNumber(1)
  @$pb.TagNumber(2)
  JobSource_Kind whichKind() => _JobSource_KindByTag[$_whichOneof(0)]!;
  @$pb.TagNumber(1)
  @$pb.TagNumber(2)
  void clearKind() => $_clearField($_whichOneof(0));

  /// git checks the commit out in a per-repo worktree on the node that is
  /// reused by later jobs, so build caches stay warm.
  @$pb.TagNumber(1)
  GitSource get git => $_getN(0);
  @$pb.TagNumber(1)
  set git(GitSource value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasGit() => $_has(0);
  @$pb.TagNumber(1)
  void clearGit() => $_clearField(1);
  @$pb.TagNumber(1)
  GitSource ensureGit() => $_ensure(0);

  /// artifact runs in a fresh job directory holding the file.
  @$pb.TagNumber(2)
  ArtifactSource get artifact => $_getN(1);
  @$pb.TagNumber(2)
  set artifact(ArtifactSource value) => $_setField(2, value);
  @$pb.TagNumber(2)
  $core.bool hasArtifact() => $_has(1);
  @$pb.TagNumber(2)
  void clearArtifact() => $_clearField(2);
  @$pb.TagNumber(2)
  ArtifactSource ensureArtifact() => $_ensure(1);
}

class SubmitJobRequest extends $pb.GeneratedMessage {
  factory SubmitJobRequest({
    JobTarget? target,
    JobSource? source,
    $core.Iterable<$core.String>? command,
    $core.Iterable<$core.MapEntry<$core.String, $core.String>>? env,
    $1.Duration? timeout,
    $core.Iterable<$core.String>? artifactGlobs,
    $core.String? requestId,
    $core.bool? admin,
    JobLimits? limits,
  }) {
    final result = SubmitJobRequest._();
    if (target != null) result.target = target;
    if (source != null) result.source = source;
    if (command != null) result.command.addAll(command);
    if (env != null) result.env.addEntries(env);
    if (timeout != null) result.timeout = timeout;
    if (artifactGlobs != null) result.artifactGlobs.addAll(artifactGlobs);
    if (requestId != null) result.requestId = requestId;
    if (admin != null) result.admin = admin;
    if (limits != null) result.limits = limits;
    return result;
  }

  SubmitJobRequest._();

  factory SubmitJobRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      SubmitJobRequest()..mergeFromBuffer(data, registry);
  factory SubmitJobRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      SubmitJobRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'SubmitJobRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: SubmitJobRequest.$_createMessage)
    ..aOM<JobTarget>(1, _omitFieldNames ? '' : 'target',
        subBuilder: JobTarget.$_createMessage)
    ..aOM<JobSource>(2, _omitFieldNames ? '' : 'source',
        subBuilder: JobSource.$_createMessage)
    ..pPS(3, _omitFieldNames ? '' : 'command')
    ..m<$core.String, $core.String>(4, _omitFieldNames ? '' : 'env',
        entryClassName: 'SubmitJobRequest.EnvEntry',
        keyFieldType: $pb.PbFieldType.OS,
        valueFieldType: $pb.PbFieldType.OS,
        packageName: const $pb.PackageName('protectorate.v1'))
    ..aOM<$1.Duration>(5, _omitFieldNames ? '' : 'timeout',
        subBuilder: $1.Duration.$_createMessage)
    ..pPS(6, _omitFieldNames ? '' : 'artifactGlobs')
    ..aOS(7, _omitFieldNames ? '' : 'requestId')
    ..aOB(8, _omitFieldNames ? '' : 'admin')
    ..aOM<JobLimits>(9, _omitFieldNames ? '' : 'limits',
        subBuilder: JobLimits.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  SubmitJobRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  SubmitJobRequest copyWith(void Function(SubmitJobRequest) updates) =>
      super.copyWith((message) => updates(message as SubmitJobRequest))
          as SubmitJobRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use SubmitJobRequest() / SubmitJobRequest.new instead')
  static SubmitJobRequest create() => SubmitJobRequest._();
  static $pb.GeneratedMessage $_createMessage() => SubmitJobRequest._();
  @$core.override
  SubmitJobRequest createEmptyInstance() => SubmitJobRequest._();
  @$core.pragma('dart2js:noInline')
  static SubmitJobRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<SubmitJobRequest>(
          SubmitJobRequest.$_createMessage);
  static SubmitJobRequest? _defaultInstance;

  @$pb.TagNumber(1)
  JobTarget get target => $_getN(0);
  @$pb.TagNumber(1)
  set target(JobTarget value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasTarget() => $_has(0);
  @$pb.TagNumber(1)
  void clearTarget() => $_clearField(1);
  @$pb.TagNumber(1)
  JobTarget ensureTarget() => $_ensure(0);

  @$pb.TagNumber(2)
  JobSource get source => $_getN(1);
  @$pb.TagNumber(2)
  set source(JobSource value) => $_setField(2, value);
  @$pb.TagNumber(2)
  $core.bool hasSource() => $_has(1);
  @$pb.TagNumber(2)
  void clearSource() => $_clearField(2);
  @$pb.TagNumber(2)
  JobSource ensureSource() => $_ensure(1);

  /// command is the argv to execute; argv[0] is resolved on the node, relative
  /// to the working directory when it contains a path separator.
  @$pb.TagNumber(3)
  $pb.PbList<$core.String> get command => $_getList(2);

  @$pb.TagNumber(4)
  $pb.PbMap<$core.String, $core.String> get env => $_getMap(3);

  /// timeout bounds the command's run time; zero means 1h, at most 24h.
  @$pb.TagNumber(5)
  $1.Duration get timeout => $_getN(4);
  @$pb.TagNumber(5)
  set timeout($1.Duration value) => $_setField(5, value);
  @$pb.TagNumber(5)
  $core.bool hasTimeout() => $_has(4);
  @$pb.TagNumber(5)
  void clearTimeout() => $_clearField(5);
  @$pb.TagNumber(5)
  $1.Duration ensureTimeout() => $_ensure(4);

  /// artifact_globs select files, relative to the working directory, that are
  /// sent back to $PROTECTORATE_HOME/jobs/<id>/artifacts/. A glob that matches
  /// a directory takes the files below it.
  @$pb.TagNumber(6)
  $pb.PbList<$core.String> get artifactGlobs => $_getList(5);

  /// request_id is a client-chosen idempotency key, 8 to 128 characters of
  /// [A-Za-z0-9._-]. The hub remembers it for 1h after the first submit with
  /// it: a repeat returns the same job instead of creating another, and waits
  /// if the first call is still in flight. Pick it with enough entropy to be
  /// unique among callers (a UUID or 16 random bytes in hex). With it, a caller
  /// that lost the response can still find or cancel the job (CancelJob
  /// request_id). Empty means no deduplication.
  @$pb.TagNumber(7)
  $core.String get requestId => $_getSZ(6);
  @$pb.TagNumber(7)
  set requestId($core.String value) => $_setString(6, value);
  @$pb.TagNumber(7)
  $core.bool hasRequestId() => $_has(6);
  @$pb.TagNumber(7)
  void clearRequestId() => $_clearField(7);

  /// admin runs the command elevated with sudo -n, or directly when the agent
  /// is root; only Linux nodes run admin jobs. It needs a named node or a
  /// lease (not a selector), a node that reports it can elevate, and the owner
  /// token or a node_admin grant for the node. Admin jobs are never sandboxed.
  @$pb.TagNumber(8)
  $core.bool get admin => $_getBF(7);
  @$pb.TagNumber(8)
  set admin($core.bool value) => $_setBool(7, value);
  @$pb.TagNumber(8)
  $core.bool hasAdmin() => $_has(7);
  @$pb.TagNumber(8)
  void clearAdmin() => $_clearField(8);

  /// limits cap a sandboxed job (JOB_MODE_SANDBOXED on a node whose sandbox
  /// is "bwrap" or a Windows job object); unset means no cap.
  @$pb.TagNumber(9)
  JobLimits get limits => $_getN(8);
  @$pb.TagNumber(9)
  set limits(JobLimits value) => $_setField(9, value);
  @$pb.TagNumber(9)
  $core.bool hasLimits() => $_has(8);
  @$pb.TagNumber(9)
  void clearLimits() => $_clearField(9);
  @$pb.TagNumber(9)
  JobLimits ensureLimits() => $_ensure(8);
}

/// JobLimits are cgroup (Linux) or job object (Windows) caps.
class JobLimits extends $pb.GeneratedMessage {
  factory JobLimits({
    $core.int? cpuPercent,
    $fixnum.Int64? memoryBytes,
  }) {
    final result = JobLimits._();
    if (cpuPercent != null) result.cpuPercent = cpuPercent;
    if (memoryBytes != null) result.memoryBytes = memoryBytes;
    return result;
  }

  JobLimits._();

  factory JobLimits.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      JobLimits()..mergeFromBuffer(data, registry);
  factory JobLimits.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      JobLimits()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'JobLimits',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: JobLimits.$_createMessage)
    ..aI(1, _omitFieldNames ? '' : 'cpuPercent', fieldType: $pb.PbFieldType.OU3)
    ..a<$fixnum.Int64>(
        2, _omitFieldNames ? '' : 'memoryBytes', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  JobLimits clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  JobLimits copyWith(void Function(JobLimits) updates) =>
      super.copyWith((message) => updates(message as JobLimits)) as JobLimits;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use JobLimits() / JobLimits.new instead')
  static JobLimits create() => JobLimits._();
  static $pb.GeneratedMessage $_createMessage() => JobLimits._();
  @$core.override
  JobLimits createEmptyInstance() => JobLimits._();
  @$core.pragma('dart2js:noInline')
  static JobLimits getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<JobLimits>(JobLimits.$_createMessage);
  static JobLimits? _defaultInstance;

  /// cpu_percent is CPU time as a share of one core: 200 is two cores.
  @$pb.TagNumber(1)
  $core.int get cpuPercent => $_getIZ(0);
  @$pb.TagNumber(1)
  set cpuPercent($core.int value) => $_setUnsignedInt32(0, value);
  @$pb.TagNumber(1)
  $core.bool hasCpuPercent() => $_has(0);
  @$pb.TagNumber(1)
  void clearCpuPercent() => $_clearField(1);

  /// memory_bytes caps resident memory, at least 64 MiB; no swap on Linux.
  @$pb.TagNumber(2)
  $fixnum.Int64 get memoryBytes => $_getI64(1);
  @$pb.TagNumber(2)
  set memoryBytes($fixnum.Int64 value) => $_setInt64(1, value);
  @$pb.TagNumber(2)
  $core.bool hasMemoryBytes() => $_has(1);
  @$pb.TagNumber(2)
  void clearMemoryBytes() => $_clearField(2);
}

class GetJobRequest extends $pb.GeneratedMessage {
  factory GetJobRequest({
    $core.String? id,
  }) {
    final result = GetJobRequest._();
    if (id != null) result.id = id;
    return result;
  }

  GetJobRequest._();

  factory GetJobRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      GetJobRequest()..mergeFromBuffer(data, registry);
  factory GetJobRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      GetJobRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'GetJobRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: GetJobRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'id')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  GetJobRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  GetJobRequest copyWith(void Function(GetJobRequest) updates) =>
      super.copyWith((message) => updates(message as GetJobRequest))
          as GetJobRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use GetJobRequest() / GetJobRequest.new instead')
  static GetJobRequest create() => GetJobRequest._();
  static $pb.GeneratedMessage $_createMessage() => GetJobRequest._();
  @$core.override
  GetJobRequest createEmptyInstance() => GetJobRequest._();
  @$core.pragma('dart2js:noInline')
  static GetJobRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<GetJobRequest>(
          GetJobRequest.$_createMessage);
  static GetJobRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get id => $_getSZ(0);
  @$pb.TagNumber(1)
  set id($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasId() => $_has(0);
  @$pb.TagNumber(1)
  void clearId() => $_clearField(1);
}

class ListJobsRequest extends $pb.GeneratedMessage {
  factory ListJobsRequest({
    JobState? state,
    $core.int? limit,
  }) {
    final result = ListJobsRequest._();
    if (state != null) result.state = state;
    if (limit != null) result.limit = limit;
    return result;
  }

  ListJobsRequest._();

  factory ListJobsRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListJobsRequest()..mergeFromBuffer(data, registry);
  factory ListJobsRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListJobsRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ListJobsRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ListJobsRequest.$_createMessage)
    ..aE<JobState>(1, _omitFieldNames ? '' : 'state',
        enumValues: JobState.values)
    ..aI(2, _omitFieldNames ? '' : 'limit')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListJobsRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListJobsRequest copyWith(void Function(ListJobsRequest) updates) =>
      super.copyWith((message) => updates(message as ListJobsRequest))
          as ListJobsRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use ListJobsRequest() / ListJobsRequest.new instead')
  static ListJobsRequest create() => ListJobsRequest._();
  static $pb.GeneratedMessage $_createMessage() => ListJobsRequest._();
  @$core.override
  ListJobsRequest createEmptyInstance() => ListJobsRequest._();
  @$core.pragma('dart2js:noInline')
  static ListJobsRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<ListJobsRequest>(
          ListJobsRequest.$_createMessage);
  static ListJobsRequest? _defaultInstance;

  /// state, when set, keeps only jobs in that state.
  @$pb.TagNumber(1)
  JobState get state => $_getN(0);
  @$pb.TagNumber(1)
  set state(JobState value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasState() => $_has(0);
  @$pb.TagNumber(1)
  void clearState() => $_clearField(1);

  /// limit caps the result; zero means 50.
  @$pb.TagNumber(2)
  $core.int get limit => $_getIZ(1);
  @$pb.TagNumber(2)
  set limit($core.int value) => $_setSignedInt32(1, value);
  @$pb.TagNumber(2)
  $core.bool hasLimit() => $_has(1);
  @$pb.TagNumber(2)
  void clearLimit() => $_clearField(2);
}

class ListJobsResponse extends $pb.GeneratedMessage {
  factory ListJobsResponse({
    $core.Iterable<Job>? jobs,
  }) {
    final result = ListJobsResponse._();
    if (jobs != null) result.jobs.addAll(jobs);
    return result;
  }

  ListJobsResponse._();

  factory ListJobsResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListJobsResponse()..mergeFromBuffer(data, registry);
  factory ListJobsResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListJobsResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ListJobsResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ListJobsResponse.$_createMessage)
    ..pPM<Job>(1, _omitFieldNames ? '' : 'jobs',
        subBuilder: Job.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListJobsResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListJobsResponse copyWith(void Function(ListJobsResponse) updates) =>
      super.copyWith((message) => updates(message as ListJobsResponse))
          as ListJobsResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use ListJobsResponse() / ListJobsResponse.new instead')
  static ListJobsResponse create() => ListJobsResponse._();
  static $pb.GeneratedMessage $_createMessage() => ListJobsResponse._();
  @$core.override
  ListJobsResponse createEmptyInstance() => ListJobsResponse._();
  @$core.pragma('dart2js:noInline')
  static ListJobsResponse getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<ListJobsResponse>(
          ListJobsResponse.$_createMessage);
  static ListJobsResponse? _defaultInstance;

  @$pb.TagNumber(1)
  $pb.PbList<Job> get jobs => $_getList(0);
}

class CancelJobRequest extends $pb.GeneratedMessage {
  factory CancelJobRequest({
    $core.String? id,
    $core.String? requestId,
  }) {
    final result = CancelJobRequest._();
    if (id != null) result.id = id;
    if (requestId != null) result.requestId = requestId;
    return result;
  }

  CancelJobRequest._();

  factory CancelJobRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      CancelJobRequest()..mergeFromBuffer(data, registry);
  factory CancelJobRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      CancelJobRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'CancelJobRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: CancelJobRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'id')
    ..aOS(2, _omitFieldNames ? '' : 'requestId')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  CancelJobRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  CancelJobRequest copyWith(void Function(CancelJobRequest) updates) =>
      super.copyWith((message) => updates(message as CancelJobRequest))
          as CancelJobRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use CancelJobRequest() / CancelJobRequest.new instead')
  static CancelJobRequest create() => CancelJobRequest._();
  static $pb.GeneratedMessage $_createMessage() => CancelJobRequest._();
  @$core.override
  CancelJobRequest createEmptyInstance() => CancelJobRequest._();
  @$core.pragma('dart2js:noInline')
  static CancelJobRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<CancelJobRequest>(
          CancelJobRequest.$_createMessage);
  static CancelJobRequest? _defaultInstance;

  /// id names the job. Set exactly one of id and request_id.
  @$pb.TagNumber(1)
  $core.String get id => $_getSZ(0);
  @$pb.TagNumber(1)
  set id($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasId() => $_has(0);
  @$pb.TagNumber(1)
  void clearId() => $_clearField(1);

  /// request_id names the job by the SubmitJob key, for a caller that never
  /// saw the response. A submit still in flight is awaited and then canceled.
  /// When no submit with the key has reached the hub, the key is remembered as
  /// canceled for 1h so a late submit is refused (aborted), and the call fails
  /// with not_found.
  @$pb.TagNumber(2)
  $core.String get requestId => $_getSZ(1);
  @$pb.TagNumber(2)
  set requestId($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasRequestId() => $_has(1);
  @$pb.TagNumber(2)
  void clearRequestId() => $_clearField(2);
}

class WatchJobRequest extends $pb.GeneratedMessage {
  factory WatchJobRequest({
    $core.String? id,
    $fixnum.Int64? fromOffset,
    $core.bool? follow,
  }) {
    final result = WatchJobRequest._();
    if (id != null) result.id = id;
    if (fromOffset != null) result.fromOffset = fromOffset;
    if (follow != null) result.follow = follow;
    return result;
  }

  WatchJobRequest._();

  factory WatchJobRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      WatchJobRequest()..mergeFromBuffer(data, registry);
  factory WatchJobRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      WatchJobRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'WatchJobRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: WatchJobRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'id')
    ..a<$fixnum.Int64>(
        2, _omitFieldNames ? '' : 'fromOffset', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..aOB(3, _omitFieldNames ? '' : 'follow')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  WatchJobRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  WatchJobRequest copyWith(void Function(WatchJobRequest) updates) =>
      super.copyWith((message) => updates(message as WatchJobRequest))
          as WatchJobRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use WatchJobRequest() / WatchJobRequest.new instead')
  static WatchJobRequest create() => WatchJobRequest._();
  static $pb.GeneratedMessage $_createMessage() => WatchJobRequest._();
  @$core.override
  WatchJobRequest createEmptyInstance() => WatchJobRequest._();
  @$core.pragma('dart2js:noInline')
  static WatchJobRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<WatchJobRequest>(
          WatchJobRequest.$_createMessage);
  static WatchJobRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get id => $_getSZ(0);
  @$pb.TagNumber(1)
  set id($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasId() => $_has(0);
  @$pb.TagNumber(1)
  void clearId() => $_clearField(1);

  /// from_offset is the byte offset in the log to start from.
  @$pb.TagNumber(2)
  $fixnum.Int64 get fromOffset => $_getI64(1);
  @$pb.TagNumber(2)
  set fromOffset($fixnum.Int64 value) => $_setInt64(1, value);
  @$pb.TagNumber(2)
  $core.bool hasFromOffset() => $_has(1);
  @$pb.TagNumber(2)
  void clearFromOffset() => $_clearField(2);

  /// follow keeps the stream open until the job ends; otherwise it ends after
  /// the log written so far.
  @$pb.TagNumber(3)
  $core.bool get follow => $_getBF(2);
  @$pb.TagNumber(3)
  set follow($core.bool value) => $_setBool(2, value);
  @$pb.TagNumber(3)
  $core.bool hasFollow() => $_has(2);
  @$pb.TagNumber(3)
  void clearFollow() => $_clearField(3);
}

enum WatchJobResponse_Event { job, log, notSet }

class WatchJobResponse extends $pb.GeneratedMessage {
  factory WatchJobResponse({
    Job? job,
    JobLog? log,
  }) {
    final result = WatchJobResponse._();
    if (job != null) result.job = job;
    if (log != null) result.log = log;
    return result;
  }

  WatchJobResponse._();

  factory WatchJobResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      WatchJobResponse()..mergeFromBuffer(data, registry);
  factory WatchJobResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      WatchJobResponse()..mergeFromJson(json, registry);

  static const $core.Map<$core.int, WatchJobResponse_Event>
      _WatchJobResponse_EventByTag = {
    1: WatchJobResponse_Event.job,
    2: WatchJobResponse_Event.log,
    0: WatchJobResponse_Event.notSet
  };
  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'WatchJobResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: WatchJobResponse.$_createMessage)
    ..oo(0, [1, 2])
    ..aOM<Job>(1, _omitFieldNames ? '' : 'job', subBuilder: Job.$_createMessage)
    ..aOM<JobLog>(2, _omitFieldNames ? '' : 'log',
        subBuilder: JobLog.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  WatchJobResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  WatchJobResponse copyWith(void Function(WatchJobResponse) updates) =>
      super.copyWith((message) => updates(message as WatchJobResponse))
          as WatchJobResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use WatchJobResponse() / WatchJobResponse.new instead')
  static WatchJobResponse create() => WatchJobResponse._();
  static $pb.GeneratedMessage $_createMessage() => WatchJobResponse._();
  @$core.override
  WatchJobResponse createEmptyInstance() => WatchJobResponse._();
  @$core.pragma('dart2js:noInline')
  static WatchJobResponse getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<WatchJobResponse>(
          WatchJobResponse.$_createMessage);
  static WatchJobResponse? _defaultInstance;

  @$pb.TagNumber(1)
  @$pb.TagNumber(2)
  WatchJobResponse_Event whichEvent() =>
      _WatchJobResponse_EventByTag[$_whichOneof(0)]!;
  @$pb.TagNumber(1)
  @$pb.TagNumber(2)
  void clearEvent() => $_clearField($_whichOneof(0));

  /// job is sent first and again on every state change.
  @$pb.TagNumber(1)
  Job get job => $_getN(0);
  @$pb.TagNumber(1)
  set job(Job value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasJob() => $_has(0);
  @$pb.TagNumber(1)
  void clearJob() => $_clearField(1);
  @$pb.TagNumber(1)
  Job ensureJob() => $_ensure(0);

  @$pb.TagNumber(2)
  JobLog get log => $_getN(1);
  @$pb.TagNumber(2)
  set log(JobLog value) => $_setField(2, value);
  @$pb.TagNumber(2)
  $core.bool hasLog() => $_has(1);
  @$pb.TagNumber(2)
  void clearLog() => $_clearField(2);
  @$pb.TagNumber(2)
  JobLog ensureLog() => $_ensure(1);
}

/// JobLog is a slice of the merged stdout and stderr of a job.
class JobLog extends $pb.GeneratedMessage {
  factory JobLog({
    $fixnum.Int64? offset,
    $core.List<$core.int>? data,
  }) {
    final result = JobLog._();
    if (offset != null) result.offset = offset;
    if (data != null) result.data = data;
    return result;
  }

  JobLog._();

  factory JobLog.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      JobLog()..mergeFromBuffer(data, registry);
  factory JobLog.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      JobLog()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'JobLog',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: JobLog.$_createMessage)
    ..a<$fixnum.Int64>(1, _omitFieldNames ? '' : 'offset', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..a<$core.List<$core.int>>(
        2, _omitFieldNames ? '' : 'data', $pb.PbFieldType.OY)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  JobLog clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  JobLog copyWith(void Function(JobLog) updates) =>
      super.copyWith((message) => updates(message as JobLog)) as JobLog;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use JobLog() / JobLog.new instead')
  static JobLog create() => JobLog._();
  static $pb.GeneratedMessage $_createMessage() => JobLog._();
  @$core.override
  JobLog createEmptyInstance() => JobLog._();
  @$core.pragma('dart2js:noInline')
  static JobLog getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<JobLog>(JobLog.$_createMessage);
  static JobLog? _defaultInstance;

  @$pb.TagNumber(1)
  $fixnum.Int64 get offset => $_getI64(0);
  @$pb.TagNumber(1)
  set offset($fixnum.Int64 value) => $_setInt64(0, value);
  @$pb.TagNumber(1)
  $core.bool hasOffset() => $_has(0);
  @$pb.TagNumber(1)
  void clearOffset() => $_clearField(1);

  @$pb.TagNumber(2)
  $core.List<$core.int> get data => $_getN(1);
  @$pb.TagNumber(2)
  set data($core.List<$core.int> value) => $_setBytes(1, value);
  @$pb.TagNumber(2)
  $core.bool hasData() => $_has(1);
  @$pb.TagNumber(2)
  void clearData() => $_clearField(2);
}

/// JobArtifact is a file that came back from the node.
class JobArtifact extends $pb.GeneratedMessage {
  factory JobArtifact({
    $core.String? path,
    $fixnum.Int64? sizeBytes,
    $core.int? mode,
  }) {
    final result = JobArtifact._();
    if (path != null) result.path = path;
    if (sizeBytes != null) result.sizeBytes = sizeBytes;
    if (mode != null) result.mode = mode;
    return result;
  }

  JobArtifact._();

  factory JobArtifact.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      JobArtifact()..mergeFromBuffer(data, registry);
  factory JobArtifact.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      JobArtifact()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'JobArtifact',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: JobArtifact.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'path')
    ..a<$fixnum.Int64>(
        2, _omitFieldNames ? '' : 'sizeBytes', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..aI(3, _omitFieldNames ? '' : 'mode', fieldType: $pb.PbFieldType.OU3)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  JobArtifact clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  JobArtifact copyWith(void Function(JobArtifact) updates) =>
      super.copyWith((message) => updates(message as JobArtifact))
          as JobArtifact;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use JobArtifact() / JobArtifact.new instead')
  static JobArtifact create() => JobArtifact._();
  static $pb.GeneratedMessage $_createMessage() => JobArtifact._();
  @$core.override
  JobArtifact createEmptyInstance() => JobArtifact._();
  @$core.pragma('dart2js:noInline')
  static JobArtifact getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<JobArtifact>(
          JobArtifact.$_createMessage);
  static JobArtifact? _defaultInstance;

  /// path is relative to the job's artifacts directory.
  @$pb.TagNumber(1)
  $core.String get path => $_getSZ(0);
  @$pb.TagNumber(1)
  set path($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasPath() => $_has(0);
  @$pb.TagNumber(1)
  void clearPath() => $_clearField(1);

  @$pb.TagNumber(2)
  $fixnum.Int64 get sizeBytes => $_getI64(1);
  @$pb.TagNumber(2)
  set sizeBytes($fixnum.Int64 value) => $_setInt64(1, value);
  @$pb.TagNumber(2)
  $core.bool hasSizeBytes() => $_has(1);
  @$pb.TagNumber(2)
  void clearSizeBytes() => $_clearField(2);

  /// mode holds the file's permission bits as the node reported them.
  @$pb.TagNumber(3)
  $core.int get mode => $_getIZ(2);
  @$pb.TagNumber(3)
  set mode($core.int value) => $_setUnsignedInt32(2, value);
  @$pb.TagNumber(3)
  $core.bool hasMode() => $_has(2);
  @$pb.TagNumber(3)
  void clearMode() => $_clearField(3);
}

/// Job is the persisted record of one job.
class Job extends $pb.GeneratedMessage {
  factory Job({
    $core.String? id,
    JobState? state,
    JobTarget? target,
    $core.String? node,
    $core.int? gpuIndex,
    JobSource? source,
    $core.Iterable<$core.String>? command,
    $core.Iterable<$core.String>? envKeys,
    $1.Duration? timeout,
    $core.Iterable<$core.String>? artifactGlobs,
    $core.int? exitCode,
    $core.String? error,
    $2.Timestamp? submittedAt,
    $2.Timestamp? startedAt,
    $2.Timestamp? finishedAt,
    $fixnum.Int64? sourceBytes,
    $1.Duration? transferTime,
    $1.Duration? runTime,
    $core.bool? workspaceReused,
    $core.Iterable<JobArtifact>? artifacts,
    $fixnum.Int64? artifactBytes,
    $fixnum.Int64? logBytes,
    JobMode? mode,
    $core.String? isolation,
    $core.String? task,
    JobLimits? limits,
    $core.String? runId,
  }) {
    final result = Job._();
    if (id != null) result.id = id;
    if (state != null) result.state = state;
    if (target != null) result.target = target;
    if (node != null) result.node = node;
    if (gpuIndex != null) result.gpuIndex = gpuIndex;
    if (source != null) result.source = source;
    if (command != null) result.command.addAll(command);
    if (envKeys != null) result.envKeys.addAll(envKeys);
    if (timeout != null) result.timeout = timeout;
    if (artifactGlobs != null) result.artifactGlobs.addAll(artifactGlobs);
    if (exitCode != null) result.exitCode = exitCode;
    if (error != null) result.error = error;
    if (submittedAt != null) result.submittedAt = submittedAt;
    if (startedAt != null) result.startedAt = startedAt;
    if (finishedAt != null) result.finishedAt = finishedAt;
    if (sourceBytes != null) result.sourceBytes = sourceBytes;
    if (transferTime != null) result.transferTime = transferTime;
    if (runTime != null) result.runTime = runTime;
    if (workspaceReused != null) result.workspaceReused = workspaceReused;
    if (artifacts != null) result.artifacts.addAll(artifacts);
    if (artifactBytes != null) result.artifactBytes = artifactBytes;
    if (logBytes != null) result.logBytes = logBytes;
    if (mode != null) result.mode = mode;
    if (isolation != null) result.isolation = isolation;
    if (task != null) result.task = task;
    if (limits != null) result.limits = limits;
    if (runId != null) result.runId = runId;
    return result;
  }

  Job._();

  factory Job.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      Job()..mergeFromBuffer(data, registry);
  factory Job.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      Job()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'Job',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: Job.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'id')
    ..aE<JobState>(2, _omitFieldNames ? '' : 'state',
        enumValues: JobState.values)
    ..aOM<JobTarget>(3, _omitFieldNames ? '' : 'target',
        subBuilder: JobTarget.$_createMessage)
    ..aOS(4, _omitFieldNames ? '' : 'node')
    ..aI(5, _omitFieldNames ? '' : 'gpuIndex')
    ..aOM<JobSource>(6, _omitFieldNames ? '' : 'source',
        subBuilder: JobSource.$_createMessage)
    ..pPS(7, _omitFieldNames ? '' : 'command')
    ..pPS(8, _omitFieldNames ? '' : 'envKeys')
    ..aOM<$1.Duration>(9, _omitFieldNames ? '' : 'timeout',
        subBuilder: $1.Duration.$_createMessage)
    ..pPS(10, _omitFieldNames ? '' : 'artifactGlobs')
    ..aI(11, _omitFieldNames ? '' : 'exitCode')
    ..aOS(12, _omitFieldNames ? '' : 'error')
    ..aOM<$2.Timestamp>(13, _omitFieldNames ? '' : 'submittedAt',
        subBuilder: $2.Timestamp.$_createMessage)
    ..aOM<$2.Timestamp>(14, _omitFieldNames ? '' : 'startedAt',
        subBuilder: $2.Timestamp.$_createMessage)
    ..aOM<$2.Timestamp>(15, _omitFieldNames ? '' : 'finishedAt',
        subBuilder: $2.Timestamp.$_createMessage)
    ..a<$fixnum.Int64>(
        16, _omitFieldNames ? '' : 'sourceBytes', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..aOM<$1.Duration>(17, _omitFieldNames ? '' : 'transferTime',
        subBuilder: $1.Duration.$_createMessage)
    ..aOM<$1.Duration>(18, _omitFieldNames ? '' : 'runTime',
        subBuilder: $1.Duration.$_createMessage)
    ..aOB(19, _omitFieldNames ? '' : 'workspaceReused')
    ..pPM<JobArtifact>(20, _omitFieldNames ? '' : 'artifacts',
        subBuilder: JobArtifact.$_createMessage)
    ..a<$fixnum.Int64>(
        21, _omitFieldNames ? '' : 'artifactBytes', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..a<$fixnum.Int64>(
        22, _omitFieldNames ? '' : 'logBytes', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..aE<JobMode>(23, _omitFieldNames ? '' : 'mode', enumValues: JobMode.values)
    ..aOS(24, _omitFieldNames ? '' : 'isolation')
    ..aOS(25, _omitFieldNames ? '' : 'task')
    ..aOM<JobLimits>(26, _omitFieldNames ? '' : 'limits',
        subBuilder: JobLimits.$_createMessage)
    ..aOS(27, _omitFieldNames ? '' : 'runId')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  Job clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  Job copyWith(void Function(Job) updates) =>
      super.copyWith((message) => updates(message as Job)) as Job;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use Job() / Job.new instead')
  static Job create() => Job._();
  static $pb.GeneratedMessage $_createMessage() => Job._();
  @$core.override
  Job createEmptyInstance() => Job._();
  @$core.pragma('dart2js:noInline')
  static Job getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<Job>(Job.$_createMessage);
  static Job? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get id => $_getSZ(0);
  @$pb.TagNumber(1)
  set id($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasId() => $_has(0);
  @$pb.TagNumber(1)
  void clearId() => $_clearField(1);

  @$pb.TagNumber(2)
  JobState get state => $_getN(1);
  @$pb.TagNumber(2)
  set state(JobState value) => $_setField(2, value);
  @$pb.TagNumber(2)
  $core.bool hasState() => $_has(1);
  @$pb.TagNumber(2)
  void clearState() => $_clearField(2);

  @$pb.TagNumber(3)
  JobTarget get target => $_getN(2);
  @$pb.TagNumber(3)
  set target(JobTarget value) => $_setField(3, value);
  @$pb.TagNumber(3)
  $core.bool hasTarget() => $_has(2);
  @$pb.TagNumber(3)
  void clearTarget() => $_clearField(3);
  @$pb.TagNumber(3)
  JobTarget ensureTarget() => $_ensure(2);

  /// node is the node chosen to run the job.
  @$pb.TagNumber(4)
  $core.String get node => $_getSZ(3);
  @$pb.TagNumber(4)
  set node($core.String value) => $_setString(3, value);
  @$pb.TagNumber(4)
  $core.bool hasNode() => $_has(3);
  @$pb.TagNumber(4)
  void clearNode() => $_clearField(4);

  /// gpu_index is set when the target was a GPU lease.
  @$pb.TagNumber(5)
  $core.int get gpuIndex => $_getIZ(4);
  @$pb.TagNumber(5)
  set gpuIndex($core.int value) => $_setSignedInt32(4, value);
  @$pb.TagNumber(5)
  $core.bool hasGpuIndex() => $_has(4);
  @$pb.TagNumber(5)
  void clearGpuIndex() => $_clearField(5);

  @$pb.TagNumber(6)
  JobSource get source => $_getN(5);
  @$pb.TagNumber(6)
  set source(JobSource value) => $_setField(6, value);
  @$pb.TagNumber(6)
  $core.bool hasSource() => $_has(5);
  @$pb.TagNumber(6)
  void clearSource() => $_clearField(6);
  @$pb.TagNumber(6)
  JobSource ensureSource() => $_ensure(5);

  @$pb.TagNumber(7)
  $pb.PbList<$core.String> get command => $_getList(6);

  /// env_keys lists the names of the environment variables set; values are
  /// never stored.
  @$pb.TagNumber(8)
  $pb.PbList<$core.String> get envKeys => $_getList(7);

  @$pb.TagNumber(9)
  $1.Duration get timeout => $_getN(8);
  @$pb.TagNumber(9)
  set timeout($1.Duration value) => $_setField(9, value);
  @$pb.TagNumber(9)
  $core.bool hasTimeout() => $_has(8);
  @$pb.TagNumber(9)
  void clearTimeout() => $_clearField(9);
  @$pb.TagNumber(9)
  $1.Duration ensureTimeout() => $_ensure(8);

  @$pb.TagNumber(10)
  $pb.PbList<$core.String> get artifactGlobs => $_getList(9);

  /// exit_code is meaningful once the command ran; -1 when it was killed.
  @$pb.TagNumber(11)
  $core.int get exitCode => $_getIZ(10);
  @$pb.TagNumber(11)
  set exitCode($core.int value) => $_setSignedInt32(10, value);
  @$pb.TagNumber(11)
  $core.bool hasExitCode() => $_has(10);
  @$pb.TagNumber(11)
  void clearExitCode() => $_clearField(11);

  /// error describes why a job failed, timed out or was cancelled early.
  @$pb.TagNumber(12)
  $core.String get error => $_getSZ(11);
  @$pb.TagNumber(12)
  set error($core.String value) => $_setString(11, value);
  @$pb.TagNumber(12)
  $core.bool hasError() => $_has(11);
  @$pb.TagNumber(12)
  void clearError() => $_clearField(12);

  @$pb.TagNumber(13)
  $2.Timestamp get submittedAt => $_getN(12);
  @$pb.TagNumber(13)
  set submittedAt($2.Timestamp value) => $_setField(13, value);
  @$pb.TagNumber(13)
  $core.bool hasSubmittedAt() => $_has(12);
  @$pb.TagNumber(13)
  void clearSubmittedAt() => $_clearField(13);
  @$pb.TagNumber(13)
  $2.Timestamp ensureSubmittedAt() => $_ensure(12);

  @$pb.TagNumber(14)
  $2.Timestamp get startedAt => $_getN(13);
  @$pb.TagNumber(14)
  set startedAt($2.Timestamp value) => $_setField(14, value);
  @$pb.TagNumber(14)
  $core.bool hasStartedAt() => $_has(13);
  @$pb.TagNumber(14)
  void clearStartedAt() => $_clearField(14);
  @$pb.TagNumber(14)
  $2.Timestamp ensureStartedAt() => $_ensure(13);

  @$pb.TagNumber(15)
  $2.Timestamp get finishedAt => $_getN(14);
  @$pb.TagNumber(15)
  set finishedAt($2.Timestamp value) => $_setField(15, value);
  @$pb.TagNumber(15)
  $core.bool hasFinishedAt() => $_has(14);
  @$pb.TagNumber(15)
  void clearFinishedAt() => $_clearField(15);
  @$pb.TagNumber(15)
  $2.Timestamp ensureFinishedAt() => $_ensure(14);

  /// source_bytes is what the hub sent to the node; zero when the node already
  /// had the commit.
  @$pb.TagNumber(16)
  $fixnum.Int64 get sourceBytes => $_getI64(15);
  @$pb.TagNumber(16)
  set sourceBytes($fixnum.Int64 value) => $_setInt64(15, value);
  @$pb.TagNumber(16)
  $core.bool hasSourceBytes() => $_has(15);
  @$pb.TagNumber(16)
  void clearSourceBytes() => $_clearField(16);

  /// transfer_time is the time spent preparing and shipping the source.
  @$pb.TagNumber(17)
  $1.Duration get transferTime => $_getN(16);
  @$pb.TagNumber(17)
  set transferTime($1.Duration value) => $_setField(17, value);
  @$pb.TagNumber(17)
  $core.bool hasTransferTime() => $_has(16);
  @$pb.TagNumber(17)
  void clearTransferTime() => $_clearField(17);
  @$pb.TagNumber(17)
  $1.Duration ensureTransferTime() => $_ensure(16);

  /// run_time is the command's wall time on the node.
  @$pb.TagNumber(18)
  $1.Duration get runTime => $_getN(17);
  @$pb.TagNumber(18)
  set runTime($1.Duration value) => $_setField(18, value);
  @$pb.TagNumber(18)
  $core.bool hasRunTime() => $_has(17);
  @$pb.TagNumber(18)
  void clearRunTime() => $_clearField(18);
  @$pb.TagNumber(18)
  $1.Duration ensureRunTime() => $_ensure(17);

  /// workspace_reused is true when a git job found the node's worktree from an
  /// earlier job.
  @$pb.TagNumber(19)
  $core.bool get workspaceReused => $_getBF(18);
  @$pb.TagNumber(19)
  set workspaceReused($core.bool value) => $_setBool(18, value);
  @$pb.TagNumber(19)
  $core.bool hasWorkspaceReused() => $_has(18);
  @$pb.TagNumber(19)
  void clearWorkspaceReused() => $_clearField(19);

  @$pb.TagNumber(20)
  $pb.PbList<JobArtifact> get artifacts => $_getList(19);

  @$pb.TagNumber(21)
  $fixnum.Int64 get artifactBytes => $_getI64(20);
  @$pb.TagNumber(21)
  set artifactBytes($fixnum.Int64 value) => $_setInt64(20, value);
  @$pb.TagNumber(21)
  $core.bool hasArtifactBytes() => $_has(20);
  @$pb.TagNumber(21)
  void clearArtifactBytes() => $_clearField(21);

  @$pb.TagNumber(22)
  $fixnum.Int64 get logBytes => $_getI64(21);
  @$pb.TagNumber(22)
  set logBytes($fixnum.Int64 value) => $_setInt64(21, value);
  @$pb.TagNumber(22)
  $core.bool hasLogBytes() => $_has(21);
  @$pb.TagNumber(22)
  void clearLogBytes() => $_clearField(22);

  @$pb.TagNumber(23)
  JobMode get mode => $_getN(22);
  @$pb.TagNumber(23)
  set mode(JobMode value) => $_setField(23, value);
  @$pb.TagNumber(23)
  $core.bool hasMode() => $_has(22);
  @$pb.TagNumber(23)
  void clearMode() => $_clearField(23);

  /// isolation is how the node ran the command, as it reported at start, e.g.
  /// "bwrap", "none: bwrap is not installed", "sudo".
  @$pb.TagNumber(24)
  $core.String get isolation => $_getSZ(23);
  @$pb.TagNumber(24)
  set isolation($core.String value) => $_setString(23, value);
  @$pb.TagNumber(24)
  $core.bool hasIsolation() => $_has(23);
  @$pb.TagNumber(24)
  void clearIsolation() => $_clearField(24);

  /// task is the task of the admin grant the job was submitted with.
  @$pb.TagNumber(25)
  $core.String get task => $_getSZ(24);
  @$pb.TagNumber(25)
  set task($core.String value) => $_setString(24, value);
  @$pb.TagNumber(25)
  $core.bool hasTask() => $_has(24);
  @$pb.TagNumber(25)
  void clearTask() => $_clearField(25);

  @$pb.TagNumber(26)
  JobLimits get limits => $_getN(25);
  @$pb.TagNumber(26)
  set limits(JobLimits value) => $_setField(26, value);
  @$pb.TagNumber(26)
  $core.bool hasLimits() => $_has(25);
  @$pb.TagNumber(26)
  void clearLimits() => $_clearField(26);
  @$pb.TagNumber(26)
  JobLimits ensureLimits() => $_ensure(25);

  /// run_id is the submitting sleeve's run.
  @$pb.TagNumber(27)
  $core.String get runId => $_getSZ(26);
  @$pb.TagNumber(27)
  set runId($core.String value) => $_setString(26, value);
  @$pb.TagNumber(27)
  $core.bool hasRunId() => $_has(26);
  @$pb.TagNumber(27)
  void clearRunId() => $_clearField(27);
}

class PrepareJobRequest extends $pb.GeneratedMessage {
  factory PrepareJobRequest({
    $core.String? jobId,
    $core.String? repoKey,
    $core.String? commit,
  }) {
    final result = PrepareJobRequest._();
    if (jobId != null) result.jobId = jobId;
    if (repoKey != null) result.repoKey = repoKey;
    if (commit != null) result.commit = commit;
    return result;
  }

  PrepareJobRequest._();

  factory PrepareJobRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      PrepareJobRequest()..mergeFromBuffer(data, registry);
  factory PrepareJobRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      PrepareJobRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'PrepareJobRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: PrepareJobRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'jobId')
    ..aOS(2, _omitFieldNames ? '' : 'repoKey')
    ..aOS(3, _omitFieldNames ? '' : 'commit')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  PrepareJobRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  PrepareJobRequest copyWith(void Function(PrepareJobRequest) updates) =>
      super.copyWith((message) => updates(message as PrepareJobRequest))
          as PrepareJobRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use PrepareJobRequest() / PrepareJobRequest.new instead')
  static PrepareJobRequest create() => PrepareJobRequest._();
  static $pb.GeneratedMessage $_createMessage() => PrepareJobRequest._();
  @$core.override
  PrepareJobRequest createEmptyInstance() => PrepareJobRequest._();
  @$core.pragma('dart2js:noInline')
  static PrepareJobRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<PrepareJobRequest>(
          PrepareJobRequest.$_createMessage);
  static PrepareJobRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get jobId => $_getSZ(0);
  @$pb.TagNumber(1)
  set jobId($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasJobId() => $_has(0);
  @$pb.TagNumber(1)
  void clearJobId() => $_clearField(1);

  /// repo_key names the node's cache directory for one hub repository.
  @$pb.TagNumber(2)
  $core.String get repoKey => $_getSZ(1);
  @$pb.TagNumber(2)
  set repoKey($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasRepoKey() => $_has(1);
  @$pb.TagNumber(2)
  void clearRepoKey() => $_clearField(2);

  @$pb.TagNumber(3)
  $core.String get commit => $_getSZ(2);
  @$pb.TagNumber(3)
  set commit($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasCommit() => $_has(2);
  @$pb.TagNumber(3)
  void clearCommit() => $_clearField(3);
}

class PrepareJobResponse extends $pb.GeneratedMessage {
  factory PrepareJobResponse({
    $core.bool? hasCommit,
    $core.Iterable<$core.String>? knownCommits,
  }) {
    final result = PrepareJobResponse._();
    if (hasCommit != null) result.hasCommit = hasCommit;
    if (knownCommits != null) result.knownCommits.addAll(knownCommits);
    return result;
  }

  PrepareJobResponse._();

  factory PrepareJobResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      PrepareJobResponse()..mergeFromBuffer(data, registry);
  factory PrepareJobResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      PrepareJobResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'PrepareJobResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: PrepareJobResponse.$_createMessage)
    ..aOB(1, _omitFieldNames ? '' : 'hasCommit')
    ..pPS(2, _omitFieldNames ? '' : 'knownCommits')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  PrepareJobResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  PrepareJobResponse copyWith(void Function(PrepareJobResponse) updates) =>
      super.copyWith((message) => updates(message as PrepareJobResponse))
          as PrepareJobResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use PrepareJobResponse() / PrepareJobResponse.new instead')
  static PrepareJobResponse create() => PrepareJobResponse._();
  static $pb.GeneratedMessage $_createMessage() => PrepareJobResponse._();
  @$core.override
  PrepareJobResponse createEmptyInstance() => PrepareJobResponse._();
  @$core.pragma('dart2js:noInline')
  static PrepareJobResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<PrepareJobResponse>(
          PrepareJobResponse.$_createMessage);
  static PrepareJobResponse? _defaultInstance;

  /// has_commit is true when the node's cache already holds the commit.
  @$pb.TagNumber(1)
  $core.bool get hasCommit => $_getBF(0);
  @$pb.TagNumber(1)
  set hasCommit($core.bool value) => $_setBool(0, value);
  @$pb.TagNumber(1)
  $core.bool hasHasCommit() => $_has(0);
  @$pb.TagNumber(1)
  void clearHasCommit() => $_clearField(1);

  /// known_commits are recent commits in the node's cache, usable as bundle
  /// prerequisites.
  @$pb.TagNumber(2)
  $pb.PbList<$core.String> get knownCommits => $_getList(1);
}

class UploadJobDataRequest extends $pb.GeneratedMessage {
  factory UploadJobDataRequest({
    $core.String? jobId,
    $fixnum.Int64? offset,
    $core.List<$core.int>? data,
    $core.bool? last,
    $core.String? sha256,
  }) {
    final result = UploadJobDataRequest._();
    if (jobId != null) result.jobId = jobId;
    if (offset != null) result.offset = offset;
    if (data != null) result.data = data;
    if (last != null) result.last = last;
    if (sha256 != null) result.sha256 = sha256;
    return result;
  }

  UploadJobDataRequest._();

  factory UploadJobDataRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      UploadJobDataRequest()..mergeFromBuffer(data, registry);
  factory UploadJobDataRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      UploadJobDataRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'UploadJobDataRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: UploadJobDataRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'jobId')
    ..a<$fixnum.Int64>(2, _omitFieldNames ? '' : 'offset', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..a<$core.List<$core.int>>(
        3, _omitFieldNames ? '' : 'data', $pb.PbFieldType.OY)
    ..aOB(4, _omitFieldNames ? '' : 'last')
    ..aOS(5, _omitFieldNames ? '' : 'sha256')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  UploadJobDataRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  UploadJobDataRequest copyWith(void Function(UploadJobDataRequest) updates) =>
      super.copyWith((message) => updates(message as UploadJobDataRequest))
          as UploadJobDataRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use UploadJobDataRequest() / UploadJobDataRequest.new instead')
  static UploadJobDataRequest create() => UploadJobDataRequest._();
  static $pb.GeneratedMessage $_createMessage() => UploadJobDataRequest._();
  @$core.override
  UploadJobDataRequest createEmptyInstance() => UploadJobDataRequest._();
  @$core.pragma('dart2js:noInline')
  static UploadJobDataRequest getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<UploadJobDataRequest>(
          UploadJobDataRequest.$_createMessage);
  static UploadJobDataRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get jobId => $_getSZ(0);
  @$pb.TagNumber(1)
  set jobId($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasJobId() => $_has(0);
  @$pb.TagNumber(1)
  void clearJobId() => $_clearField(1);

  /// offset must equal the bytes received so far.
  @$pb.TagNumber(2)
  $fixnum.Int64 get offset => $_getI64(1);
  @$pb.TagNumber(2)
  set offset($fixnum.Int64 value) => $_setInt64(1, value);
  @$pb.TagNumber(2)
  $core.bool hasOffset() => $_has(1);
  @$pb.TagNumber(2)
  void clearOffset() => $_clearField(2);

  @$pb.TagNumber(3)
  $core.List<$core.int> get data => $_getN(2);
  @$pb.TagNumber(3)
  set data($core.List<$core.int> value) => $_setBytes(2, value);
  @$pb.TagNumber(3)
  $core.bool hasData() => $_has(2);
  @$pb.TagNumber(3)
  void clearData() => $_clearField(3);

  @$pb.TagNumber(4)
  $core.bool get last => $_getBF(3);
  @$pb.TagNumber(4)
  set last($core.bool value) => $_setBool(3, value);
  @$pb.TagNumber(4)
  $core.bool hasLast() => $_has(3);
  @$pb.TagNumber(4)
  void clearLast() => $_clearField(4);

  /// sha256 is the hex digest of the whole upload; required on the last chunk.
  @$pb.TagNumber(5)
  $core.String get sha256 => $_getSZ(4);
  @$pb.TagNumber(5)
  set sha256($core.String value) => $_setString(4, value);
  @$pb.TagNumber(5)
  $core.bool hasSha256() => $_has(4);
  @$pb.TagNumber(5)
  void clearSha256() => $_clearField(5);
}

class UploadJobDataResponse extends $pb.GeneratedMessage {
  factory UploadJobDataResponse({
    $fixnum.Int64? received,
  }) {
    final result = UploadJobDataResponse._();
    if (received != null) result.received = received;
    return result;
  }

  UploadJobDataResponse._();

  factory UploadJobDataResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      UploadJobDataResponse()..mergeFromBuffer(data, registry);
  factory UploadJobDataResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      UploadJobDataResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'UploadJobDataResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: UploadJobDataResponse.$_createMessage)
    ..a<$fixnum.Int64>(
        1, _omitFieldNames ? '' : 'received', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  UploadJobDataResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  UploadJobDataResponse copyWith(
          void Function(UploadJobDataResponse) updates) =>
      super.copyWith((message) => updates(message as UploadJobDataResponse))
          as UploadJobDataResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use UploadJobDataResponse() / UploadJobDataResponse.new instead')
  static UploadJobDataResponse create() => UploadJobDataResponse._();
  static $pb.GeneratedMessage $_createMessage() => UploadJobDataResponse._();
  @$core.override
  UploadJobDataResponse createEmptyInstance() => UploadJobDataResponse._();
  @$core.pragma('dart2js:noInline')
  static UploadJobDataResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<UploadJobDataResponse>(
          UploadJobDataResponse.$_createMessage);
  static UploadJobDataResponse? _defaultInstance;

  @$pb.TagNumber(1)
  $fixnum.Int64 get received => $_getI64(0);
  @$pb.TagNumber(1)
  set received($fixnum.Int64 value) => $_setInt64(0, value);
  @$pb.TagNumber(1)
  $core.bool hasReceived() => $_has(0);
  @$pb.TagNumber(1)
  void clearReceived() => $_clearField(1);
}

class RunJobRequest extends $pb.GeneratedMessage {
  factory RunJobRequest({
    $core.String? jobId,
    $core.String? repoKey,
    $core.String? commit,
    $core.bool? bundleUploaded,
    $core.String? artifactName,
    $core.Iterable<$core.String>? command,
    $core.Iterable<$core.MapEntry<$core.String, $core.String>>? env,
    $1.Duration? timeout,
    $core.Iterable<$core.String>? artifactGlobs,
    $core.bool? artifactArchive,
    $core.String? workdir,
    JobMode? mode,
    JobLimits? limits,
  }) {
    final result = RunJobRequest._();
    if (jobId != null) result.jobId = jobId;
    if (repoKey != null) result.repoKey = repoKey;
    if (commit != null) result.commit = commit;
    if (bundleUploaded != null) result.bundleUploaded = bundleUploaded;
    if (artifactName != null) result.artifactName = artifactName;
    if (command != null) result.command.addAll(command);
    if (env != null) result.env.addEntries(env);
    if (timeout != null) result.timeout = timeout;
    if (artifactGlobs != null) result.artifactGlobs.addAll(artifactGlobs);
    if (artifactArchive != null) result.artifactArchive = artifactArchive;
    if (workdir != null) result.workdir = workdir;
    if (mode != null) result.mode = mode;
    if (limits != null) result.limits = limits;
    return result;
  }

  RunJobRequest._();

  factory RunJobRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      RunJobRequest()..mergeFromBuffer(data, registry);
  factory RunJobRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      RunJobRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'RunJobRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: RunJobRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'jobId')
    ..aOS(2, _omitFieldNames ? '' : 'repoKey')
    ..aOS(3, _omitFieldNames ? '' : 'commit')
    ..aOB(4, _omitFieldNames ? '' : 'bundleUploaded')
    ..aOS(5, _omitFieldNames ? '' : 'artifactName')
    ..pPS(6, _omitFieldNames ? '' : 'command')
    ..m<$core.String, $core.String>(7, _omitFieldNames ? '' : 'env',
        entryClassName: 'RunJobRequest.EnvEntry',
        keyFieldType: $pb.PbFieldType.OS,
        valueFieldType: $pb.PbFieldType.OS,
        packageName: const $pb.PackageName('protectorate.v1'))
    ..aOM<$1.Duration>(8, _omitFieldNames ? '' : 'timeout',
        subBuilder: $1.Duration.$_createMessage)
    ..pPS(9, _omitFieldNames ? '' : 'artifactGlobs')
    ..aOB(10, _omitFieldNames ? '' : 'artifactArchive')
    ..aOS(11, _omitFieldNames ? '' : 'workdir')
    ..aE<JobMode>(12, _omitFieldNames ? '' : 'mode', enumValues: JobMode.values)
    ..aOM<JobLimits>(13, _omitFieldNames ? '' : 'limits',
        subBuilder: JobLimits.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  RunJobRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  RunJobRequest copyWith(void Function(RunJobRequest) updates) =>
      super.copyWith((message) => updates(message as RunJobRequest))
          as RunJobRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use RunJobRequest() / RunJobRequest.new instead')
  static RunJobRequest create() => RunJobRequest._();
  static $pb.GeneratedMessage $_createMessage() => RunJobRequest._();
  @$core.override
  RunJobRequest createEmptyInstance() => RunJobRequest._();
  @$core.pragma('dart2js:noInline')
  static RunJobRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<RunJobRequest>(
          RunJobRequest.$_createMessage);
  static RunJobRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get jobId => $_getSZ(0);
  @$pb.TagNumber(1)
  set jobId($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasJobId() => $_has(0);
  @$pb.TagNumber(1)
  void clearJobId() => $_clearField(1);

  /// repo_key and commit select a git job; bundle_uploaded says a bundle for
  /// the commit was uploaded and must be fetched first.
  @$pb.TagNumber(2)
  $core.String get repoKey => $_getSZ(1);
  @$pb.TagNumber(2)
  set repoKey($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasRepoKey() => $_has(1);
  @$pb.TagNumber(2)
  void clearRepoKey() => $_clearField(2);

  @$pb.TagNumber(3)
  $core.String get commit => $_getSZ(2);
  @$pb.TagNumber(3)
  set commit($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasCommit() => $_has(2);
  @$pb.TagNumber(3)
  void clearCommit() => $_clearField(3);

  @$pb.TagNumber(4)
  $core.bool get bundleUploaded => $_getBF(3);
  @$pb.TagNumber(4)
  set bundleUploaded($core.bool value) => $_setBool(3, value);
  @$pb.TagNumber(4)
  $core.bool hasBundleUploaded() => $_has(3);
  @$pb.TagNumber(4)
  void clearBundleUploaded() => $_clearField(4);

  /// artifact_name is the file name an uploaded artifact gets in the job
  /// directory.
  @$pb.TagNumber(5)
  $core.String get artifactName => $_getSZ(4);
  @$pb.TagNumber(5)
  set artifactName($core.String value) => $_setString(4, value);
  @$pb.TagNumber(5)
  $core.bool hasArtifactName() => $_has(4);
  @$pb.TagNumber(5)
  void clearArtifactName() => $_clearField(5);

  @$pb.TagNumber(6)
  $pb.PbList<$core.String> get command => $_getList(5);

  @$pb.TagNumber(7)
  $pb.PbMap<$core.String, $core.String> get env => $_getMap(6);

  @$pb.TagNumber(8)
  $1.Duration get timeout => $_getN(7);
  @$pb.TagNumber(8)
  set timeout($1.Duration value) => $_setField(8, value);
  @$pb.TagNumber(8)
  $core.bool hasTimeout() => $_has(7);
  @$pb.TagNumber(8)
  void clearTimeout() => $_clearField(8);
  @$pb.TagNumber(8)
  $1.Duration ensureTimeout() => $_ensure(7);

  @$pb.TagNumber(9)
  $pb.PbList<$core.String> get artifactGlobs => $_getList(8);

  /// artifact_archive says the upload is a zip to extract into the job
  /// directory instead of a single file named artifact_name.
  @$pb.TagNumber(10)
  $core.bool get artifactArchive => $_getBF(9);
  @$pb.TagNumber(10)
  set artifactArchive($core.bool value) => $_setBool(9, value);
  @$pb.TagNumber(10)
  $core.bool hasArtifactArchive() => $_has(9);
  @$pb.TagNumber(10)
  void clearArtifactArchive() => $_clearField(10);

  /// workdir is relative to the job directory; only valid with an archive.
  @$pb.TagNumber(11)
  $core.String get workdir => $_getSZ(10);
  @$pb.TagNumber(11)
  set workdir($core.String value) => $_setString(10, value);
  @$pb.TagNumber(11)
  $core.bool hasWorkdir() => $_has(10);
  @$pb.TagNumber(11)
  void clearWorkdir() => $_clearField(11);

  @$pb.TagNumber(12)
  JobMode get mode => $_getN(11);
  @$pb.TagNumber(12)
  set mode(JobMode value) => $_setField(12, value);
  @$pb.TagNumber(12)
  $core.bool hasMode() => $_has(11);
  @$pb.TagNumber(12)
  void clearMode() => $_clearField(12);

  @$pb.TagNumber(13)
  JobLimits get limits => $_getN(12);
  @$pb.TagNumber(13)
  set limits(JobLimits value) => $_setField(13, value);
  @$pb.TagNumber(13)
  $core.bool hasLimits() => $_has(12);
  @$pb.TagNumber(13)
  void clearLimits() => $_clearField(13);
  @$pb.TagNumber(13)
  JobLimits ensureLimits() => $_ensure(12);
}

class JobStarted extends $pb.GeneratedMessage {
  factory JobStarted({
    $core.bool? workspaceReused,
    $core.String? isolation,
  }) {
    final result = JobStarted._();
    if (workspaceReused != null) result.workspaceReused = workspaceReused;
    if (isolation != null) result.isolation = isolation;
    return result;
  }

  JobStarted._();

  factory JobStarted.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      JobStarted()..mergeFromBuffer(data, registry);
  factory JobStarted.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      JobStarted()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'JobStarted',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: JobStarted.$_createMessage)
    ..aOB(1, _omitFieldNames ? '' : 'workspaceReused')
    ..aOS(2, _omitFieldNames ? '' : 'isolation')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  JobStarted clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  JobStarted copyWith(void Function(JobStarted) updates) =>
      super.copyWith((message) => updates(message as JobStarted)) as JobStarted;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use JobStarted() / JobStarted.new instead')
  static JobStarted create() => JobStarted._();
  static $pb.GeneratedMessage $_createMessage() => JobStarted._();
  @$core.override
  JobStarted createEmptyInstance() => JobStarted._();
  @$core.pragma('dart2js:noInline')
  static JobStarted getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<JobStarted>(JobStarted.$_createMessage);
  static JobStarted? _defaultInstance;

  @$pb.TagNumber(1)
  $core.bool get workspaceReused => $_getBF(0);
  @$pb.TagNumber(1)
  set workspaceReused($core.bool value) => $_setBool(0, value);
  @$pb.TagNumber(1)
  $core.bool hasWorkspaceReused() => $_has(0);
  @$pb.TagNumber(1)
  void clearWorkspaceReused() => $_clearField(1);

  /// isolation is how the command runs, as in Job.isolation.
  @$pb.TagNumber(2)
  $core.String get isolation => $_getSZ(1);
  @$pb.TagNumber(2)
  set isolation($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasIsolation() => $_has(1);
  @$pb.TagNumber(2)
  void clearIsolation() => $_clearField(2);
}

class JobOutput extends $pb.GeneratedMessage {
  factory JobOutput({
    $core.List<$core.int>? data,
  }) {
    final result = JobOutput._();
    if (data != null) result.data = data;
    return result;
  }

  JobOutput._();

  factory JobOutput.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      JobOutput()..mergeFromBuffer(data, registry);
  factory JobOutput.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      JobOutput()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'JobOutput',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: JobOutput.$_createMessage)
    ..a<$core.List<$core.int>>(
        1, _omitFieldNames ? '' : 'data', $pb.PbFieldType.OY)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  JobOutput clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  JobOutput copyWith(void Function(JobOutput) updates) =>
      super.copyWith((message) => updates(message as JobOutput)) as JobOutput;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use JobOutput() / JobOutput.new instead')
  static JobOutput create() => JobOutput._();
  static $pb.GeneratedMessage $_createMessage() => JobOutput._();
  @$core.override
  JobOutput createEmptyInstance() => JobOutput._();
  @$core.pragma('dart2js:noInline')
  static JobOutput getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<JobOutput>(JobOutput.$_createMessage);
  static JobOutput? _defaultInstance;

  @$pb.TagNumber(1)
  $core.List<$core.int> get data => $_getN(0);
  @$pb.TagNumber(1)
  set data($core.List<$core.int> value) => $_setBytes(0, value);
  @$pb.TagNumber(1)
  $core.bool hasData() => $_has(0);
  @$pb.TagNumber(1)
  void clearData() => $_clearField(1);
}

/// JobArtifactChunk is one frame of an artifact file; the last frame of a file
/// has last set.
class JobArtifactChunk extends $pb.GeneratedMessage {
  factory JobArtifactChunk({
    $core.String? path,
    $core.List<$core.int>? data,
    $core.bool? last,
    $core.int? mode,
  }) {
    final result = JobArtifactChunk._();
    if (path != null) result.path = path;
    if (data != null) result.data = data;
    if (last != null) result.last = last;
    if (mode != null) result.mode = mode;
    return result;
  }

  JobArtifactChunk._();

  factory JobArtifactChunk.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      JobArtifactChunk()..mergeFromBuffer(data, registry);
  factory JobArtifactChunk.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      JobArtifactChunk()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'JobArtifactChunk',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: JobArtifactChunk.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'path')
    ..a<$core.List<$core.int>>(
        2, _omitFieldNames ? '' : 'data', $pb.PbFieldType.OY)
    ..aOB(3, _omitFieldNames ? '' : 'last')
    ..aI(4, _omitFieldNames ? '' : 'mode', fieldType: $pb.PbFieldType.OU3)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  JobArtifactChunk clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  JobArtifactChunk copyWith(void Function(JobArtifactChunk) updates) =>
      super.copyWith((message) => updates(message as JobArtifactChunk))
          as JobArtifactChunk;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use JobArtifactChunk() / JobArtifactChunk.new instead')
  static JobArtifactChunk create() => JobArtifactChunk._();
  static $pb.GeneratedMessage $_createMessage() => JobArtifactChunk._();
  @$core.override
  JobArtifactChunk createEmptyInstance() => JobArtifactChunk._();
  @$core.pragma('dart2js:noInline')
  static JobArtifactChunk getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<JobArtifactChunk>(
          JobArtifactChunk.$_createMessage);
  static JobArtifactChunk? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get path => $_getSZ(0);
  @$pb.TagNumber(1)
  set path($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasPath() => $_has(0);
  @$pb.TagNumber(1)
  void clearPath() => $_clearField(1);

  @$pb.TagNumber(2)
  $core.List<$core.int> get data => $_getN(1);
  @$pb.TagNumber(2)
  set data($core.List<$core.int> value) => $_setBytes(1, value);
  @$pb.TagNumber(2)
  $core.bool hasData() => $_has(1);
  @$pb.TagNumber(2)
  void clearData() => $_clearField(2);

  @$pb.TagNumber(3)
  $core.bool get last => $_getBF(2);
  @$pb.TagNumber(3)
  set last($core.bool value) => $_setBool(2, value);
  @$pb.TagNumber(3)
  $core.bool hasLast() => $_has(2);
  @$pb.TagNumber(3)
  void clearLast() => $_clearField(3);

  /// mode is the file's permission bits; set on the first frame of a file.
  @$pb.TagNumber(4)
  $core.int get mode => $_getIZ(3);
  @$pb.TagNumber(4)
  set mode($core.int value) => $_setUnsignedInt32(3, value);
  @$pb.TagNumber(4)
  $core.bool hasMode() => $_has(3);
  @$pb.TagNumber(4)
  void clearMode() => $_clearField(4);
}

class JobFinished extends $pb.GeneratedMessage {
  factory JobFinished({
    JobEnd? end,
    $core.int? exitCode,
    $core.String? error,
    $1.Duration? runTime,
  }) {
    final result = JobFinished._();
    if (end != null) result.end = end;
    if (exitCode != null) result.exitCode = exitCode;
    if (error != null) result.error = error;
    if (runTime != null) result.runTime = runTime;
    return result;
  }

  JobFinished._();

  factory JobFinished.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      JobFinished()..mergeFromBuffer(data, registry);
  factory JobFinished.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      JobFinished()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'JobFinished',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: JobFinished.$_createMessage)
    ..aE<JobEnd>(1, _omitFieldNames ? '' : 'end', enumValues: JobEnd.values)
    ..aI(2, _omitFieldNames ? '' : 'exitCode')
    ..aOS(3, _omitFieldNames ? '' : 'error')
    ..aOM<$1.Duration>(4, _omitFieldNames ? '' : 'runTime',
        subBuilder: $1.Duration.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  JobFinished clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  JobFinished copyWith(void Function(JobFinished) updates) =>
      super.copyWith((message) => updates(message as JobFinished))
          as JobFinished;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use JobFinished() / JobFinished.new instead')
  static JobFinished create() => JobFinished._();
  static $pb.GeneratedMessage $_createMessage() => JobFinished._();
  @$core.override
  JobFinished createEmptyInstance() => JobFinished._();
  @$core.pragma('dart2js:noInline')
  static JobFinished getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<JobFinished>(
          JobFinished.$_createMessage);
  static JobFinished? _defaultInstance;

  @$pb.TagNumber(1)
  JobEnd get end => $_getN(0);
  @$pb.TagNumber(1)
  set end(JobEnd value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasEnd() => $_has(0);
  @$pb.TagNumber(1)
  void clearEnd() => $_clearField(1);

  @$pb.TagNumber(2)
  $core.int get exitCode => $_getIZ(1);
  @$pb.TagNumber(2)
  set exitCode($core.int value) => $_setSignedInt32(1, value);
  @$pb.TagNumber(2)
  $core.bool hasExitCode() => $_has(1);
  @$pb.TagNumber(2)
  void clearExitCode() => $_clearField(2);

  @$pb.TagNumber(3)
  $core.String get error => $_getSZ(2);
  @$pb.TagNumber(3)
  set error($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasError() => $_has(2);
  @$pb.TagNumber(3)
  void clearError() => $_clearField(3);

  @$pb.TagNumber(4)
  $1.Duration get runTime => $_getN(3);
  @$pb.TagNumber(4)
  set runTime($1.Duration value) => $_setField(4, value);
  @$pb.TagNumber(4)
  $core.bool hasRunTime() => $_has(3);
  @$pb.TagNumber(4)
  void clearRunTime() => $_clearField(4);
  @$pb.TagNumber(4)
  $1.Duration ensureRunTime() => $_ensure(3);
}

enum JobEvent_Event { started, output, artifact, finished, notSet }

class JobEvent extends $pb.GeneratedMessage {
  factory JobEvent({
    JobStarted? started,
    JobOutput? output,
    JobArtifactChunk? artifact,
    JobFinished? finished,
  }) {
    final result = JobEvent._();
    if (started != null) result.started = started;
    if (output != null) result.output = output;
    if (artifact != null) result.artifact = artifact;
    if (finished != null) result.finished = finished;
    return result;
  }

  JobEvent._();

  factory JobEvent.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      JobEvent()..mergeFromBuffer(data, registry);
  factory JobEvent.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      JobEvent()..mergeFromJson(json, registry);

  static const $core.Map<$core.int, JobEvent_Event> _JobEvent_EventByTag = {
    1: JobEvent_Event.started,
    2: JobEvent_Event.output,
    3: JobEvent_Event.artifact,
    4: JobEvent_Event.finished,
    0: JobEvent_Event.notSet
  };
  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'JobEvent',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: JobEvent.$_createMessage)
    ..oo(0, [1, 2, 3, 4])
    ..aOM<JobStarted>(1, _omitFieldNames ? '' : 'started',
        subBuilder: JobStarted.$_createMessage)
    ..aOM<JobOutput>(2, _omitFieldNames ? '' : 'output',
        subBuilder: JobOutput.$_createMessage)
    ..aOM<JobArtifactChunk>(3, _omitFieldNames ? '' : 'artifact',
        subBuilder: JobArtifactChunk.$_createMessage)
    ..aOM<JobFinished>(4, _omitFieldNames ? '' : 'finished',
        subBuilder: JobFinished.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  JobEvent clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  JobEvent copyWith(void Function(JobEvent) updates) =>
      super.copyWith((message) => updates(message as JobEvent)) as JobEvent;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use JobEvent() / JobEvent.new instead')
  static JobEvent create() => JobEvent._();
  static $pb.GeneratedMessage $_createMessage() => JobEvent._();
  @$core.override
  JobEvent createEmptyInstance() => JobEvent._();
  @$core.pragma('dart2js:noInline')
  static JobEvent getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<JobEvent>(JobEvent.$_createMessage);
  static JobEvent? _defaultInstance;

  @$pb.TagNumber(1)
  @$pb.TagNumber(2)
  @$pb.TagNumber(3)
  @$pb.TagNumber(4)
  JobEvent_Event whichEvent() => _JobEvent_EventByTag[$_whichOneof(0)]!;
  @$pb.TagNumber(1)
  @$pb.TagNumber(2)
  @$pb.TagNumber(3)
  @$pb.TagNumber(4)
  void clearEvent() => $_clearField($_whichOneof(0));

  @$pb.TagNumber(1)
  JobStarted get started => $_getN(0);
  @$pb.TagNumber(1)
  set started(JobStarted value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasStarted() => $_has(0);
  @$pb.TagNumber(1)
  void clearStarted() => $_clearField(1);
  @$pb.TagNumber(1)
  JobStarted ensureStarted() => $_ensure(0);

  @$pb.TagNumber(2)
  JobOutput get output => $_getN(1);
  @$pb.TagNumber(2)
  set output(JobOutput value) => $_setField(2, value);
  @$pb.TagNumber(2)
  $core.bool hasOutput() => $_has(1);
  @$pb.TagNumber(2)
  void clearOutput() => $_clearField(2);
  @$pb.TagNumber(2)
  JobOutput ensureOutput() => $_ensure(1);

  @$pb.TagNumber(3)
  JobArtifactChunk get artifact => $_getN(2);
  @$pb.TagNumber(3)
  set artifact(JobArtifactChunk value) => $_setField(3, value);
  @$pb.TagNumber(3)
  $core.bool hasArtifact() => $_has(2);
  @$pb.TagNumber(3)
  void clearArtifact() => $_clearField(3);
  @$pb.TagNumber(3)
  JobArtifactChunk ensureArtifact() => $_ensure(2);

  @$pb.TagNumber(4)
  JobFinished get finished => $_getN(3);
  @$pb.TagNumber(4)
  set finished(JobFinished value) => $_setField(4, value);
  @$pb.TagNumber(4)
  $core.bool hasFinished() => $_has(3);
  @$pb.TagNumber(4)
  void clearFinished() => $_clearField(4);
  @$pb.TagNumber(4)
  JobFinished ensureFinished() => $_ensure(3);
}

class NodeAgentServiceCancelJobRequest extends $pb.GeneratedMessage {
  factory NodeAgentServiceCancelJobRequest({
    $core.String? jobId,
  }) {
    final result = NodeAgentServiceCancelJobRequest._();
    if (jobId != null) result.jobId = jobId;
    return result;
  }

  NodeAgentServiceCancelJobRequest._();

  factory NodeAgentServiceCancelJobRequest.fromBuffer(
          $core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      NodeAgentServiceCancelJobRequest()..mergeFromBuffer(data, registry);
  factory NodeAgentServiceCancelJobRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      NodeAgentServiceCancelJobRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'NodeAgentServiceCancelJobRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: NodeAgentServiceCancelJobRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'jobId')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  NodeAgentServiceCancelJobRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  NodeAgentServiceCancelJobRequest copyWith(
          void Function(NodeAgentServiceCancelJobRequest) updates) =>
      super.copyWith(
              (message) => updates(message as NodeAgentServiceCancelJobRequest))
          as NodeAgentServiceCancelJobRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use NodeAgentServiceCancelJobRequest() / NodeAgentServiceCancelJobRequest.new instead')
  static NodeAgentServiceCancelJobRequest create() =>
      NodeAgentServiceCancelJobRequest._();
  static $pb.GeneratedMessage $_createMessage() =>
      NodeAgentServiceCancelJobRequest._();
  @$core.override
  NodeAgentServiceCancelJobRequest createEmptyInstance() =>
      NodeAgentServiceCancelJobRequest._();
  @$core.pragma('dart2js:noInline')
  static NodeAgentServiceCancelJobRequest getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<NodeAgentServiceCancelJobRequest>(
          NodeAgentServiceCancelJobRequest.$_createMessage);
  static NodeAgentServiceCancelJobRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get jobId => $_getSZ(0);
  @$pb.TagNumber(1)
  set jobId($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasJobId() => $_has(0);
  @$pb.TagNumber(1)
  void clearJobId() => $_clearField(1);
}

class NodeAgentServiceCancelJobResponse extends $pb.GeneratedMessage {
  factory NodeAgentServiceCancelJobResponse({
    $core.bool? found,
  }) {
    final result = NodeAgentServiceCancelJobResponse._();
    if (found != null) result.found = found;
    return result;
  }

  NodeAgentServiceCancelJobResponse._();

  factory NodeAgentServiceCancelJobResponse.fromBuffer(
          $core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      NodeAgentServiceCancelJobResponse()..mergeFromBuffer(data, registry);
  factory NodeAgentServiceCancelJobResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      NodeAgentServiceCancelJobResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'NodeAgentServiceCancelJobResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: NodeAgentServiceCancelJobResponse.$_createMessage)
    ..aOB(1, _omitFieldNames ? '' : 'found')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  NodeAgentServiceCancelJobResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  NodeAgentServiceCancelJobResponse copyWith(
          void Function(NodeAgentServiceCancelJobResponse) updates) =>
      super.copyWith((message) =>
              updates(message as NodeAgentServiceCancelJobResponse))
          as NodeAgentServiceCancelJobResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use NodeAgentServiceCancelJobResponse() / NodeAgentServiceCancelJobResponse.new instead')
  static NodeAgentServiceCancelJobResponse create() =>
      NodeAgentServiceCancelJobResponse._();
  static $pb.GeneratedMessage $_createMessage() =>
      NodeAgentServiceCancelJobResponse._();
  @$core.override
  NodeAgentServiceCancelJobResponse createEmptyInstance() =>
      NodeAgentServiceCancelJobResponse._();
  @$core.pragma('dart2js:noInline')
  static NodeAgentServiceCancelJobResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<NodeAgentServiceCancelJobResponse>(
          NodeAgentServiceCancelJobResponse.$_createMessage);
  static NodeAgentServiceCancelJobResponse? _defaultInstance;

  /// found is false when no such job is running.
  @$pb.TagNumber(1)
  $core.bool get found => $_getBF(0);
  @$pb.TagNumber(1)
  set found($core.bool value) => $_setBool(0, value);
  @$pb.TagNumber(1)
  $core.bool hasFound() => $_has(0);
  @$pb.TagNumber(1)
  void clearFound() => $_clearField(1);
}

class NodeAgentServiceListJobsRequest extends $pb.GeneratedMessage {
  factory NodeAgentServiceListJobsRequest() =>
      NodeAgentServiceListJobsRequest._();

  NodeAgentServiceListJobsRequest._();

  factory NodeAgentServiceListJobsRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      NodeAgentServiceListJobsRequest()..mergeFromBuffer(data, registry);
  factory NodeAgentServiceListJobsRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      NodeAgentServiceListJobsRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'NodeAgentServiceListJobsRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: NodeAgentServiceListJobsRequest.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  NodeAgentServiceListJobsRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  NodeAgentServiceListJobsRequest copyWith(
          void Function(NodeAgentServiceListJobsRequest) updates) =>
      super.copyWith(
              (message) => updates(message as NodeAgentServiceListJobsRequest))
          as NodeAgentServiceListJobsRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use NodeAgentServiceListJobsRequest() / NodeAgentServiceListJobsRequest.new instead')
  static NodeAgentServiceListJobsRequest create() =>
      NodeAgentServiceListJobsRequest._();
  static $pb.GeneratedMessage $_createMessage() =>
      NodeAgentServiceListJobsRequest._();
  @$core.override
  NodeAgentServiceListJobsRequest createEmptyInstance() =>
      NodeAgentServiceListJobsRequest._();
  @$core.pragma('dart2js:noInline')
  static NodeAgentServiceListJobsRequest getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<NodeAgentServiceListJobsRequest>(
          NodeAgentServiceListJobsRequest.$_createMessage);
  static NodeAgentServiceListJobsRequest? _defaultInstance;
}

class NodeAgentServiceListJobsResponse extends $pb.GeneratedMessage {
  factory NodeAgentServiceListJobsResponse({
    $core.Iterable<NodeJob>? jobs,
  }) {
    final result = NodeAgentServiceListJobsResponse._();
    if (jobs != null) result.jobs.addAll(jobs);
    return result;
  }

  NodeAgentServiceListJobsResponse._();

  factory NodeAgentServiceListJobsResponse.fromBuffer(
          $core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      NodeAgentServiceListJobsResponse()..mergeFromBuffer(data, registry);
  factory NodeAgentServiceListJobsResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      NodeAgentServiceListJobsResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'NodeAgentServiceListJobsResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: NodeAgentServiceListJobsResponse.$_createMessage)
    ..pPM<NodeJob>(1, _omitFieldNames ? '' : 'jobs',
        subBuilder: NodeJob.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  NodeAgentServiceListJobsResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  NodeAgentServiceListJobsResponse copyWith(
          void Function(NodeAgentServiceListJobsResponse) updates) =>
      super.copyWith(
              (message) => updates(message as NodeAgentServiceListJobsResponse))
          as NodeAgentServiceListJobsResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use NodeAgentServiceListJobsResponse() / NodeAgentServiceListJobsResponse.new instead')
  static NodeAgentServiceListJobsResponse create() =>
      NodeAgentServiceListJobsResponse._();
  static $pb.GeneratedMessage $_createMessage() =>
      NodeAgentServiceListJobsResponse._();
  @$core.override
  NodeAgentServiceListJobsResponse createEmptyInstance() =>
      NodeAgentServiceListJobsResponse._();
  @$core.pragma('dart2js:noInline')
  static NodeAgentServiceListJobsResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<NodeAgentServiceListJobsResponse>(
          NodeAgentServiceListJobsResponse.$_createMessage);
  static NodeAgentServiceListJobsResponse? _defaultInstance;

  @$pb.TagNumber(1)
  $pb.PbList<NodeJob> get jobs => $_getList(0);
}

/// NodeJob is a job the node holds: running, or finished and kept until the
/// hub has had time to collect the result.
class NodeJob extends $pb.GeneratedMessage {
  factory NodeJob({
    $core.String? jobId,
    $core.bool? started,
    JobFinished? finished,
    $fixnum.Int64? logBytes,
    $2.Timestamp? finishedAt,
  }) {
    final result = NodeJob._();
    if (jobId != null) result.jobId = jobId;
    if (started != null) result.started = started;
    if (finished != null) result.finished = finished;
    if (logBytes != null) result.logBytes = logBytes;
    if (finishedAt != null) result.finishedAt = finishedAt;
    return result;
  }

  NodeJob._();

  factory NodeJob.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      NodeJob()..mergeFromBuffer(data, registry);
  factory NodeJob.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      NodeJob()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'NodeJob',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: NodeJob.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'jobId')
    ..aOB(2, _omitFieldNames ? '' : 'started')
    ..aOM<JobFinished>(3, _omitFieldNames ? '' : 'finished',
        subBuilder: JobFinished.$_createMessage)
    ..a<$fixnum.Int64>(
        4, _omitFieldNames ? '' : 'logBytes', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..aOM<$2.Timestamp>(5, _omitFieldNames ? '' : 'finishedAt',
        subBuilder: $2.Timestamp.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  NodeJob clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  NodeJob copyWith(void Function(NodeJob) updates) =>
      super.copyWith((message) => updates(message as NodeJob)) as NodeJob;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use NodeJob() / NodeJob.new instead')
  static NodeJob create() => NodeJob._();
  static $pb.GeneratedMessage $_createMessage() => NodeJob._();
  @$core.override
  NodeJob createEmptyInstance() => NodeJob._();
  @$core.pragma('dart2js:noInline')
  static NodeJob getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<NodeJob>(NodeJob.$_createMessage);
  static NodeJob? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get jobId => $_getSZ(0);
  @$pb.TagNumber(1)
  set jobId($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasJobId() => $_has(0);
  @$pb.TagNumber(1)
  void clearJobId() => $_clearField(1);

  /// started is true once the command was launched.
  @$pb.TagNumber(2)
  $core.bool get started => $_getBF(1);
  @$pb.TagNumber(2)
  set started($core.bool value) => $_setBool(1, value);
  @$pb.TagNumber(2)
  $core.bool hasStarted() => $_has(1);
  @$pb.TagNumber(2)
  void clearStarted() => $_clearField(2);

  /// finished is set once the job ended; its artifacts are still held.
  @$pb.TagNumber(3)
  JobFinished get finished => $_getN(2);
  @$pb.TagNumber(3)
  set finished(JobFinished value) => $_setField(3, value);
  @$pb.TagNumber(3)
  $core.bool hasFinished() => $_has(2);
  @$pb.TagNumber(3)
  void clearFinished() => $_clearField(3);
  @$pb.TagNumber(3)
  JobFinished ensureFinished() => $_ensure(2);

  /// log_bytes is the size of the output so far; a reattach can resume at any
  /// offset up to it.
  @$pb.TagNumber(4)
  $fixnum.Int64 get logBytes => $_getI64(3);
  @$pb.TagNumber(4)
  set logBytes($fixnum.Int64 value) => $_setInt64(3, value);
  @$pb.TagNumber(4)
  $core.bool hasLogBytes() => $_has(3);
  @$pb.TagNumber(4)
  void clearLogBytes() => $_clearField(4);

  @$pb.TagNumber(5)
  $2.Timestamp get finishedAt => $_getN(4);
  @$pb.TagNumber(5)
  set finishedAt($2.Timestamp value) => $_setField(5, value);
  @$pb.TagNumber(5)
  $core.bool hasFinishedAt() => $_has(4);
  @$pb.TagNumber(5)
  void clearFinishedAt() => $_clearField(5);
  @$pb.TagNumber(5)
  $2.Timestamp ensureFinishedAt() => $_ensure(4);
}

class AttachJobRequest extends $pb.GeneratedMessage {
  factory AttachJobRequest({
    $core.String? jobId,
    $fixnum.Int64? fromOffset,
  }) {
    final result = AttachJobRequest._();
    if (jobId != null) result.jobId = jobId;
    if (fromOffset != null) result.fromOffset = fromOffset;
    return result;
  }

  AttachJobRequest._();

  factory AttachJobRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      AttachJobRequest()..mergeFromBuffer(data, registry);
  factory AttachJobRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      AttachJobRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'AttachJobRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: AttachJobRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'jobId')
    ..a<$fixnum.Int64>(
        2, _omitFieldNames ? '' : 'fromOffset', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  AttachJobRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  AttachJobRequest copyWith(void Function(AttachJobRequest) updates) =>
      super.copyWith((message) => updates(message as AttachJobRequest))
          as AttachJobRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use AttachJobRequest() / AttachJobRequest.new instead')
  static AttachJobRequest create() => AttachJobRequest._();
  static $pb.GeneratedMessage $_createMessage() => AttachJobRequest._();
  @$core.override
  AttachJobRequest createEmptyInstance() => AttachJobRequest._();
  @$core.pragma('dart2js:noInline')
  static AttachJobRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<AttachJobRequest>(
          AttachJobRequest.$_createMessage);
  static AttachJobRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get jobId => $_getSZ(0);
  @$pb.TagNumber(1)
  set jobId($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasJobId() => $_has(0);
  @$pb.TagNumber(1)
  void clearJobId() => $_clearField(1);

  /// from_offset is the first output byte to send.
  @$pb.TagNumber(2)
  $fixnum.Int64 get fromOffset => $_getI64(1);
  @$pb.TagNumber(2)
  set fromOffset($fixnum.Int64 value) => $_setInt64(1, value);
  @$pb.TagNumber(2)
  $core.bool hasFromOffset() => $_has(1);
  @$pb.TagNumber(2)
  void clearFromOffset() => $_clearField(2);
}

/// JobService runs commands on nodes. The hub picks the node, ships the source
/// and streams the logs; the node executes. Jobs are kept under
/// $PROTECTORATE_HOME/jobs/<id>/ (job.json, job.log, artifacts/) and announced
/// on EventService as "job.updated" (subject = job id). A running job survives a
/// hub restart: the node keeps it, and the restarted hub reconciles with the
/// node and reattaches the log stream. It is marked failed only when the node
/// reports it never had the job.
///
/// A job executes arbitrary commands as the node agent's user, so the scopes
/// that admit SubmitJob are as powerful as shell access to every node.
class JobServiceApi {
  final $pb.RpcClient _client;

  JobServiceApi(this._client);

  /// SubmitJob queues a job and returns at once with its id. Follow it with
  /// WatchJob.
  $async.Future<Job> submitJob(
          $pb.ClientContext? ctx, SubmitJobRequest request) =>
      _client.invoke<Job>(ctx, 'JobService', 'SubmitJob', request, Job());

  /// GetJob returns one job record.
  $async.Future<Job> getJob($pb.ClientContext? ctx, GetJobRequest request) =>
      _client.invoke<Job>(ctx, 'JobService', 'GetJob', request, Job());

  /// ListJobs returns job records, newest first.
  $async.Future<ListJobsResponse> listJobs(
          $pb.ClientContext? ctx, ListJobsRequest request) =>
      _client.invoke<ListJobsResponse>(
          ctx, 'JobService', 'ListJobs', request, ListJobsResponse());

  /// CancelJob stops a job and its whole process tree, named by id or by the
  /// SubmitJob request_id. Cancelling a finished job is a no-op that returns
  /// its record. Once the call returns, the cancel is durable: it survives a
  /// hub or node crash and restart, and the job ends CANCELED even when the
  /// node lost the job or never received it. The only other outcome is the
  /// result the job had already reached on the node before the cancel took
  /// effect (for example SUCCEEDED).
  $async.Future<Job> cancelJob(
          $pb.ClientContext? ctx, CancelJobRequest request) =>
      _client.invoke<Job>(ctx, 'JobService', 'CancelJob', request, Job());

  /// WatchJob replays a job's log from an offset and, when follow is set,
  /// streams new output and state changes until the job ends.
  $async.Future<WatchJobResponse> watchJob(
          $pb.ClientContext? ctx, WatchJobRequest request) =>
      _client.invoke<WatchJobResponse>(
          ctx, 'JobService', 'WatchJob', request, WatchJobResponse());
}

const $core.bool _omitFieldNames =
    $core.bool.fromEnvironment('protobuf.omit_field_names');
const $core.bool _omitMessageNames =
    $core.bool.fromEnvironment('protobuf.omit_message_names');
