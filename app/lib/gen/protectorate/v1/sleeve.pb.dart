// This is a generated file - do not edit.
//
// Generated from protectorate/v1/sleeve.proto.

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
    as $2;
import 'package:protobuf/well_known_types/google/protobuf/timestamp.pb.dart'
    as $0;

import 'chat.pb.dart' as $1;
import 'sleeve.pbenum.dart';
import 'token.pb.dart' as $3;

export 'package:protobuf/protobuf.dart' show GeneratedMessageGenericExtensions;

export 'sleeve.pbenum.dart';

class Checkpoint extends $pb.GeneratedMessage {
  factory Checkpoint({
    $fixnum.Int64? id,
    $core.String? commit,
    $0.Timestamp? createdAt,
    $core.String? reason,
    $core.String? runId,
  }) {
    final result = Checkpoint._();
    if (id != null) result.id = id;
    if (commit != null) result.commit = commit;
    if (createdAt != null) result.createdAt = createdAt;
    if (reason != null) result.reason = reason;
    if (runId != null) result.runId = runId;
    return result;
  }

  Checkpoint._();

  factory Checkpoint.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      Checkpoint()..mergeFromBuffer(data, registry);
  factory Checkpoint.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      Checkpoint()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'Checkpoint',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: Checkpoint.$_createMessage)
    ..a<$fixnum.Int64>(1, _omitFieldNames ? '' : 'id', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..aOS(2, _omitFieldNames ? '' : 'commit')
    ..aOM<$0.Timestamp>(3, _omitFieldNames ? '' : 'createdAt',
        subBuilder: $0.Timestamp.$_createMessage)
    ..aOS(4, _omitFieldNames ? '' : 'reason')
    ..aOS(5, _omitFieldNames ? '' : 'runId')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  Checkpoint clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  Checkpoint copyWith(void Function(Checkpoint) updates) =>
      super.copyWith((message) => updates(message as Checkpoint)) as Checkpoint;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use Checkpoint() / Checkpoint.new instead')
  static Checkpoint create() => Checkpoint._();
  static $pb.GeneratedMessage $_createMessage() => Checkpoint._();
  @$core.override
  Checkpoint createEmptyInstance() => Checkpoint._();
  @$core.pragma('dart2js:noInline')
  static Checkpoint getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<Checkpoint>(Checkpoint.$_createMessage);
  static Checkpoint? _defaultInstance;

  @$pb.TagNumber(1)
  $fixnum.Int64 get id => $_getI64(0);
  @$pb.TagNumber(1)
  set id($fixnum.Int64 value) => $_setInt64(0, value);
  @$pb.TagNumber(1)
  $core.bool hasId() => $_has(0);
  @$pb.TagNumber(1)
  void clearId() => $_clearField(1);

  /// commit is the checkpoint's commit in the private store.
  @$pb.TagNumber(2)
  $core.String get commit => $_getSZ(1);
  @$pb.TagNumber(2)
  set commit($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasCommit() => $_has(1);
  @$pb.TagNumber(2)
  void clearCommit() => $_clearField(2);

  @$pb.TagNumber(3)
  $0.Timestamp get createdAt => $_getN(2);
  @$pb.TagNumber(3)
  set createdAt($0.Timestamp value) => $_setField(3, value);
  @$pb.TagNumber(3)
  $core.bool hasCreatedAt() => $_has(2);
  @$pb.TagNumber(3)
  void clearCreatedAt() => $_clearField(3);
  @$pb.TagNumber(3)
  $0.Timestamp ensureCreatedAt() => $_ensure(2);

  /// reason is "turn", "resleeve", "manual" or "before restore of N".
  @$pb.TagNumber(4)
  $core.String get reason => $_getSZ(3);
  @$pb.TagNumber(4)
  set reason($core.String value) => $_setString(3, value);
  @$pb.TagNumber(4)
  $core.bool hasReason() => $_has(3);
  @$pb.TagNumber(4)
  void clearReason() => $_clearField(4);

  /// run_id is the sleeve's run when the checkpoint was taken.
  @$pb.TagNumber(5)
  $core.String get runId => $_getSZ(4);
  @$pb.TagNumber(5)
  set runId($core.String value) => $_setString(4, value);
  @$pb.TagNumber(5)
  $core.bool hasRunId() => $_has(4);
  @$pb.TagNumber(5)
  void clearRunId() => $_clearField(5);
}

class CreateCheckpointRequest extends $pb.GeneratedMessage {
  factory CreateCheckpointRequest({
    $core.String? name,
  }) {
    final result = CreateCheckpointRequest._();
    if (name != null) result.name = name;
    return result;
  }

  CreateCheckpointRequest._();

  factory CreateCheckpointRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      CreateCheckpointRequest()..mergeFromBuffer(data, registry);
  factory CreateCheckpointRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      CreateCheckpointRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'CreateCheckpointRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: CreateCheckpointRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'name')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  CreateCheckpointRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  CreateCheckpointRequest copyWith(
          void Function(CreateCheckpointRequest) updates) =>
      super.copyWith((message) => updates(message as CreateCheckpointRequest))
          as CreateCheckpointRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use CreateCheckpointRequest() / CreateCheckpointRequest.new instead')
  static CreateCheckpointRequest create() => CreateCheckpointRequest._();
  static $pb.GeneratedMessage $_createMessage() => CreateCheckpointRequest._();
  @$core.override
  CreateCheckpointRequest createEmptyInstance() => CreateCheckpointRequest._();
  @$core.pragma('dart2js:noInline')
  static CreateCheckpointRequest getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<CreateCheckpointRequest>(
          CreateCheckpointRequest.$_createMessage);
  static CreateCheckpointRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get name => $_getSZ(0);
  @$pb.TagNumber(1)
  set name($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasName() => $_has(0);
  @$pb.TagNumber(1)
  void clearName() => $_clearField(1);
}

class CreateCheckpointResponse extends $pb.GeneratedMessage {
  factory CreateCheckpointResponse({
    Checkpoint? checkpoint,
    $core.bool? unchanged,
  }) {
    final result = CreateCheckpointResponse._();
    if (checkpoint != null) result.checkpoint = checkpoint;
    if (unchanged != null) result.unchanged = unchanged;
    return result;
  }

  CreateCheckpointResponse._();

  factory CreateCheckpointResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      CreateCheckpointResponse()..mergeFromBuffer(data, registry);
  factory CreateCheckpointResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      CreateCheckpointResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'CreateCheckpointResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: CreateCheckpointResponse.$_createMessage)
    ..aOM<Checkpoint>(1, _omitFieldNames ? '' : 'checkpoint',
        subBuilder: Checkpoint.$_createMessage)
    ..aOB(2, _omitFieldNames ? '' : 'unchanged')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  CreateCheckpointResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  CreateCheckpointResponse copyWith(
          void Function(CreateCheckpointResponse) updates) =>
      super.copyWith((message) => updates(message as CreateCheckpointResponse))
          as CreateCheckpointResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use CreateCheckpointResponse() / CreateCheckpointResponse.new instead')
  static CreateCheckpointResponse create() => CreateCheckpointResponse._();
  static $pb.GeneratedMessage $_createMessage() => CreateCheckpointResponse._();
  @$core.override
  CreateCheckpointResponse createEmptyInstance() =>
      CreateCheckpointResponse._();
  @$core.pragma('dart2js:noInline')
  static CreateCheckpointResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<CreateCheckpointResponse>(
          CreateCheckpointResponse.$_createMessage);
  static CreateCheckpointResponse? _defaultInstance;

  @$pb.TagNumber(1)
  Checkpoint get checkpoint => $_getN(0);
  @$pb.TagNumber(1)
  set checkpoint(Checkpoint value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasCheckpoint() => $_has(0);
  @$pb.TagNumber(1)
  void clearCheckpoint() => $_clearField(1);
  @$pb.TagNumber(1)
  Checkpoint ensureCheckpoint() => $_ensure(0);

  @$pb.TagNumber(2)
  $core.bool get unchanged => $_getBF(1);
  @$pb.TagNumber(2)
  set unchanged($core.bool value) => $_setBool(1, value);
  @$pb.TagNumber(2)
  $core.bool hasUnchanged() => $_has(1);
  @$pb.TagNumber(2)
  void clearUnchanged() => $_clearField(2);
}

class ListCheckpointsRequest extends $pb.GeneratedMessage {
  factory ListCheckpointsRequest({
    $core.String? name,
  }) {
    final result = ListCheckpointsRequest._();
    if (name != null) result.name = name;
    return result;
  }

  ListCheckpointsRequest._();

  factory ListCheckpointsRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListCheckpointsRequest()..mergeFromBuffer(data, registry);
  factory ListCheckpointsRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListCheckpointsRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ListCheckpointsRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ListCheckpointsRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'name')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListCheckpointsRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListCheckpointsRequest copyWith(
          void Function(ListCheckpointsRequest) updates) =>
      super.copyWith((message) => updates(message as ListCheckpointsRequest))
          as ListCheckpointsRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use ListCheckpointsRequest() / ListCheckpointsRequest.new instead')
  static ListCheckpointsRequest create() => ListCheckpointsRequest._();
  static $pb.GeneratedMessage $_createMessage() => ListCheckpointsRequest._();
  @$core.override
  ListCheckpointsRequest createEmptyInstance() => ListCheckpointsRequest._();
  @$core.pragma('dart2js:noInline')
  static ListCheckpointsRequest getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<ListCheckpointsRequest>(
          ListCheckpointsRequest.$_createMessage);
  static ListCheckpointsRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get name => $_getSZ(0);
  @$pb.TagNumber(1)
  set name($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasName() => $_has(0);
  @$pb.TagNumber(1)
  void clearName() => $_clearField(1);
}

class ListCheckpointsResponse extends $pb.GeneratedMessage {
  factory ListCheckpointsResponse({
    $core.Iterable<Checkpoint>? checkpoints,
  }) {
    final result = ListCheckpointsResponse._();
    if (checkpoints != null) result.checkpoints.addAll(checkpoints);
    return result;
  }

  ListCheckpointsResponse._();

  factory ListCheckpointsResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListCheckpointsResponse()..mergeFromBuffer(data, registry);
  factory ListCheckpointsResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListCheckpointsResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ListCheckpointsResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ListCheckpointsResponse.$_createMessage)
    ..pPM<Checkpoint>(1, _omitFieldNames ? '' : 'checkpoints',
        subBuilder: Checkpoint.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListCheckpointsResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListCheckpointsResponse copyWith(
          void Function(ListCheckpointsResponse) updates) =>
      super.copyWith((message) => updates(message as ListCheckpointsResponse))
          as ListCheckpointsResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use ListCheckpointsResponse() / ListCheckpointsResponse.new instead')
  static ListCheckpointsResponse create() => ListCheckpointsResponse._();
  static $pb.GeneratedMessage $_createMessage() => ListCheckpointsResponse._();
  @$core.override
  ListCheckpointsResponse createEmptyInstance() => ListCheckpointsResponse._();
  @$core.pragma('dart2js:noInline')
  static ListCheckpointsResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<ListCheckpointsResponse>(
          ListCheckpointsResponse.$_createMessage);
  static ListCheckpointsResponse? _defaultInstance;

  @$pb.TagNumber(1)
  $pb.PbList<Checkpoint> get checkpoints => $_getList(0);
}

class DiffCheckpointRequest extends $pb.GeneratedMessage {
  factory DiffCheckpointRequest({
    $core.String? name,
    $fixnum.Int64? fromId,
    $fixnum.Int64? toId,
  }) {
    final result = DiffCheckpointRequest._();
    if (name != null) result.name = name;
    if (fromId != null) result.fromId = fromId;
    if (toId != null) result.toId = toId;
    return result;
  }

  DiffCheckpointRequest._();

  factory DiffCheckpointRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      DiffCheckpointRequest()..mergeFromBuffer(data, registry);
  factory DiffCheckpointRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      DiffCheckpointRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'DiffCheckpointRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: DiffCheckpointRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'name')
    ..a<$fixnum.Int64>(2, _omitFieldNames ? '' : 'fromId', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..a<$fixnum.Int64>(3, _omitFieldNames ? '' : 'toId', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  DiffCheckpointRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  DiffCheckpointRequest copyWith(
          void Function(DiffCheckpointRequest) updates) =>
      super.copyWith((message) => updates(message as DiffCheckpointRequest))
          as DiffCheckpointRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use DiffCheckpointRequest() / DiffCheckpointRequest.new instead')
  static DiffCheckpointRequest create() => DiffCheckpointRequest._();
  static $pb.GeneratedMessage $_createMessage() => DiffCheckpointRequest._();
  @$core.override
  DiffCheckpointRequest createEmptyInstance() => DiffCheckpointRequest._();
  @$core.pragma('dart2js:noInline')
  static DiffCheckpointRequest getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<DiffCheckpointRequest>(
          DiffCheckpointRequest.$_createMessage);
  static DiffCheckpointRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get name => $_getSZ(0);
  @$pb.TagNumber(1)
  set name($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasName() => $_has(0);
  @$pb.TagNumber(1)
  void clearName() => $_clearField(1);

  @$pb.TagNumber(2)
  $fixnum.Int64 get fromId => $_getI64(1);
  @$pb.TagNumber(2)
  set fromId($fixnum.Int64 value) => $_setInt64(1, value);
  @$pb.TagNumber(2)
  $core.bool hasFromId() => $_has(1);
  @$pb.TagNumber(2)
  void clearFromId() => $_clearField(2);

  @$pb.TagNumber(3)
  $fixnum.Int64 get toId => $_getI64(2);
  @$pb.TagNumber(3)
  set toId($fixnum.Int64 value) => $_setInt64(2, value);
  @$pb.TagNumber(3)
  $core.bool hasToId() => $_has(2);
  @$pb.TagNumber(3)
  void clearToId() => $_clearField(3);
}

class DiffCheckpointResponse extends $pb.GeneratedMessage {
  factory DiffCheckpointResponse({
    $core.String? diff,
    $core.bool? truncated,
  }) {
    final result = DiffCheckpointResponse._();
    if (diff != null) result.diff = diff;
    if (truncated != null) result.truncated = truncated;
    return result;
  }

  DiffCheckpointResponse._();

  factory DiffCheckpointResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      DiffCheckpointResponse()..mergeFromBuffer(data, registry);
  factory DiffCheckpointResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      DiffCheckpointResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'DiffCheckpointResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: DiffCheckpointResponse.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'diff')
    ..aOB(2, _omitFieldNames ? '' : 'truncated')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  DiffCheckpointResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  DiffCheckpointResponse copyWith(
          void Function(DiffCheckpointResponse) updates) =>
      super.copyWith((message) => updates(message as DiffCheckpointResponse))
          as DiffCheckpointResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use DiffCheckpointResponse() / DiffCheckpointResponse.new instead')
  static DiffCheckpointResponse create() => DiffCheckpointResponse._();
  static $pb.GeneratedMessage $_createMessage() => DiffCheckpointResponse._();
  @$core.override
  DiffCheckpointResponse createEmptyInstance() => DiffCheckpointResponse._();
  @$core.pragma('dart2js:noInline')
  static DiffCheckpointResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<DiffCheckpointResponse>(
          DiffCheckpointResponse.$_createMessage);
  static DiffCheckpointResponse? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get diff => $_getSZ(0);
  @$pb.TagNumber(1)
  set diff($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasDiff() => $_has(0);
  @$pb.TagNumber(1)
  void clearDiff() => $_clearField(1);

  @$pb.TagNumber(2)
  $core.bool get truncated => $_getBF(1);
  @$pb.TagNumber(2)
  set truncated($core.bool value) => $_setBool(1, value);
  @$pb.TagNumber(2)
  $core.bool hasTruncated() => $_has(1);
  @$pb.TagNumber(2)
  void clearTruncated() => $_clearField(2);
}

class RestoreCheckpointRequest extends $pb.GeneratedMessage {
  factory RestoreCheckpointRequest({
    $core.String? name,
    $fixnum.Int64? id,
  }) {
    final result = RestoreCheckpointRequest._();
    if (name != null) result.name = name;
    if (id != null) result.id = id;
    return result;
  }

  RestoreCheckpointRequest._();

  factory RestoreCheckpointRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      RestoreCheckpointRequest()..mergeFromBuffer(data, registry);
  factory RestoreCheckpointRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      RestoreCheckpointRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'RestoreCheckpointRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: RestoreCheckpointRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'name')
    ..a<$fixnum.Int64>(2, _omitFieldNames ? '' : 'id', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  RestoreCheckpointRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  RestoreCheckpointRequest copyWith(
          void Function(RestoreCheckpointRequest) updates) =>
      super.copyWith((message) => updates(message as RestoreCheckpointRequest))
          as RestoreCheckpointRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use RestoreCheckpointRequest() / RestoreCheckpointRequest.new instead')
  static RestoreCheckpointRequest create() => RestoreCheckpointRequest._();
  static $pb.GeneratedMessage $_createMessage() => RestoreCheckpointRequest._();
  @$core.override
  RestoreCheckpointRequest createEmptyInstance() =>
      RestoreCheckpointRequest._();
  @$core.pragma('dart2js:noInline')
  static RestoreCheckpointRequest getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<RestoreCheckpointRequest>(
          RestoreCheckpointRequest.$_createMessage);
  static RestoreCheckpointRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get name => $_getSZ(0);
  @$pb.TagNumber(1)
  set name($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasName() => $_has(0);
  @$pb.TagNumber(1)
  void clearName() => $_clearField(1);

  @$pb.TagNumber(2)
  $fixnum.Int64 get id => $_getI64(1);
  @$pb.TagNumber(2)
  set id($fixnum.Int64 value) => $_setInt64(1, value);
  @$pb.TagNumber(2)
  $core.bool hasId() => $_has(1);
  @$pb.TagNumber(2)
  void clearId() => $_clearField(2);
}

class RestoreCheckpointResponse extends $pb.GeneratedMessage {
  factory RestoreCheckpointResponse({
    Checkpoint? before,
  }) {
    final result = RestoreCheckpointResponse._();
    if (before != null) result.before = before;
    return result;
  }

  RestoreCheckpointResponse._();

  factory RestoreCheckpointResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      RestoreCheckpointResponse()..mergeFromBuffer(data, registry);
  factory RestoreCheckpointResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      RestoreCheckpointResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'RestoreCheckpointResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: RestoreCheckpointResponse.$_createMessage)
    ..aOM<Checkpoint>(1, _omitFieldNames ? '' : 'before',
        subBuilder: Checkpoint.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  RestoreCheckpointResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  RestoreCheckpointResponse copyWith(
          void Function(RestoreCheckpointResponse) updates) =>
      super.copyWith((message) => updates(message as RestoreCheckpointResponse))
          as RestoreCheckpointResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use RestoreCheckpointResponse() / RestoreCheckpointResponse.new instead')
  static RestoreCheckpointResponse create() => RestoreCheckpointResponse._();
  static $pb.GeneratedMessage $_createMessage() =>
      RestoreCheckpointResponse._();
  @$core.override
  RestoreCheckpointResponse createEmptyInstance() =>
      RestoreCheckpointResponse._();
  @$core.pragma('dart2js:noInline')
  static RestoreCheckpointResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<RestoreCheckpointResponse>(
          RestoreCheckpointResponse.$_createMessage);
  static RestoreCheckpointResponse? _defaultInstance;

  @$pb.TagNumber(1)
  Checkpoint get before => $_getN(0);
  @$pb.TagNumber(1)
  set before(Checkpoint value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasBefore() => $_has(0);
  @$pb.TagNumber(1)
  void clearBefore() => $_clearField(1);
  @$pb.TagNumber(1)
  Checkpoint ensureBefore() => $_ensure(0);
}

class SleeveLimits extends $pb.GeneratedMessage {
  factory SleeveLimits({
    $core.int? cpuPercent,
    $fixnum.Int64? memoryBytes,
    $fixnum.Int64? diskBytes,
  }) {
    final result = SleeveLimits._();
    if (cpuPercent != null) result.cpuPercent = cpuPercent;
    if (memoryBytes != null) result.memoryBytes = memoryBytes;
    if (diskBytes != null) result.diskBytes = diskBytes;
    return result;
  }

  SleeveLimits._();

  factory SleeveLimits.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      SleeveLimits()..mergeFromBuffer(data, registry);
  factory SleeveLimits.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      SleeveLimits()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'SleeveLimits',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: SleeveLimits.$_createMessage)
    ..aI(1, _omitFieldNames ? '' : 'cpuPercent', fieldType: $pb.PbFieldType.OU3)
    ..a<$fixnum.Int64>(
        2, _omitFieldNames ? '' : 'memoryBytes', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..a<$fixnum.Int64>(
        3, _omitFieldNames ? '' : 'diskBytes', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  SleeveLimits clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  SleeveLimits copyWith(void Function(SleeveLimits) updates) =>
      super.copyWith((message) => updates(message as SleeveLimits))
          as SleeveLimits;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use SleeveLimits() / SleeveLimits.new instead')
  static SleeveLimits create() => SleeveLimits._();
  static $pb.GeneratedMessage $_createMessage() => SleeveLimits._();
  @$core.override
  SleeveLimits createEmptyInstance() => SleeveLimits._();
  @$core.pragma('dart2js:noInline')
  static SleeveLimits getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<SleeveLimits>(
          SleeveLimits.$_createMessage);
  static SleeveLimits? _defaultInstance;

  /// cpu_percent caps CPU time; 100 is one core, 200 two cores. 0 means unlimited.
  @$pb.TagNumber(1)
  $core.int get cpuPercent => $_getIZ(0);
  @$pb.TagNumber(1)
  set cpuPercent($core.int value) => $_setUnsignedInt32(0, value);
  @$pb.TagNumber(1)
  $core.bool hasCpuPercent() => $_has(0);
  @$pb.TagNumber(1)
  void clearCpuPercent() => $_clearField(1);

  /// memory_bytes caps memory including page cache and tmpfs; swap is disabled
  /// when set. 0 means unlimited.
  @$pb.TagNumber(2)
  $fixnum.Int64 get memoryBytes => $_getI64(1);
  @$pb.TagNumber(2)
  set memoryBytes($fixnum.Int64 value) => $_setInt64(1, value);
  @$pb.TagNumber(2)
  $core.bool hasMemoryBytes() => $_has(1);
  @$pb.TagNumber(2)
  void clearMemoryBytes() => $_clearField(2);

  /// disk_bytes bounds the sleeve's workspace, home, cache overlays and
  /// checkpoint store, as the hub samples them every 10 seconds while it runs.
  /// At 90% the hub emits "sleeve.disk_warning"; at 100% it stops the harness,
  /// emits "sleeve.disk_limit", notes it in the chat and audits it. Writes
  /// between samples are not blocked. 0 means unlimited.
  @$pb.TagNumber(3)
  $fixnum.Int64 get diskBytes => $_getI64(2);
  @$pb.TagNumber(3)
  set diskBytes($fixnum.Int64 value) => $_setInt64(2, value);
  @$pb.TagNumber(3)
  $core.bool hasDiskBytes() => $_has(2);
  @$pb.TagNumber(3)
  void clearDiskBytes() => $_clearField(3);
}

/// SleeveUsage is read from the sleeve's cgroup.
class SleeveUsage extends $pb.GeneratedMessage {
  factory SleeveUsage({
    $core.double? cpuPercent,
    $core.double? cpuSeconds,
    $fixnum.Int64? memoryCurrentBytes,
    $fixnum.Int64? memoryPeakBytes,
    $core.int? pids,
    $fixnum.Int64? oomKills,
    $fixnum.Int64? workspaceDiskBytes,
    $0.Timestamp? sampledAt,
    $0.Timestamp? diskSampledAt,
    $fixnum.Int64? homeDiskBytes,
    $fixnum.Int64? cacheDiskBytes,
    $fixnum.Int64? goBuildCacheDiskBytes,
  }) {
    final result = SleeveUsage._();
    if (cpuPercent != null) result.cpuPercent = cpuPercent;
    if (cpuSeconds != null) result.cpuSeconds = cpuSeconds;
    if (memoryCurrentBytes != null)
      result.memoryCurrentBytes = memoryCurrentBytes;
    if (memoryPeakBytes != null) result.memoryPeakBytes = memoryPeakBytes;
    if (pids != null) result.pids = pids;
    if (oomKills != null) result.oomKills = oomKills;
    if (workspaceDiskBytes != null)
      result.workspaceDiskBytes = workspaceDiskBytes;
    if (sampledAt != null) result.sampledAt = sampledAt;
    if (diskSampledAt != null) result.diskSampledAt = diskSampledAt;
    if (homeDiskBytes != null) result.homeDiskBytes = homeDiskBytes;
    if (cacheDiskBytes != null) result.cacheDiskBytes = cacheDiskBytes;
    if (goBuildCacheDiskBytes != null)
      result.goBuildCacheDiskBytes = goBuildCacheDiskBytes;
    return result;
  }

  SleeveUsage._();

  factory SleeveUsage.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      SleeveUsage()..mergeFromBuffer(data, registry);
  factory SleeveUsage.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      SleeveUsage()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'SleeveUsage',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: SleeveUsage.$_createMessage)
    ..aD(1, _omitFieldNames ? '' : 'cpuPercent')
    ..aD(2, _omitFieldNames ? '' : 'cpuSeconds')
    ..a<$fixnum.Int64>(
        3, _omitFieldNames ? '' : 'memoryCurrentBytes', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..a<$fixnum.Int64>(
        4, _omitFieldNames ? '' : 'memoryPeakBytes', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..aI(5, _omitFieldNames ? '' : 'pids', fieldType: $pb.PbFieldType.OU3)
    ..a<$fixnum.Int64>(
        6, _omitFieldNames ? '' : 'oomKills', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..a<$fixnum.Int64>(
        7, _omitFieldNames ? '' : 'workspaceDiskBytes', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..aOM<$0.Timestamp>(8, _omitFieldNames ? '' : 'sampledAt',
        subBuilder: $0.Timestamp.$_createMessage)
    ..aOM<$0.Timestamp>(9, _omitFieldNames ? '' : 'diskSampledAt',
        subBuilder: $0.Timestamp.$_createMessage)
    ..a<$fixnum.Int64>(
        10, _omitFieldNames ? '' : 'homeDiskBytes', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..a<$fixnum.Int64>(
        11, _omitFieldNames ? '' : 'cacheDiskBytes', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..a<$fixnum.Int64>(
        12, _omitFieldNames ? '' : 'goBuildCacheDiskBytes', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  SleeveUsage clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  SleeveUsage copyWith(void Function(SleeveUsage) updates) =>
      super.copyWith((message) => updates(message as SleeveUsage))
          as SleeveUsage;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use SleeveUsage() / SleeveUsage.new instead')
  static SleeveUsage create() => SleeveUsage._();
  static $pb.GeneratedMessage $_createMessage() => SleeveUsage._();
  @$core.override
  SleeveUsage createEmptyInstance() => SleeveUsage._();
  @$core.pragma('dart2js:noInline')
  static SleeveUsage getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<SleeveUsage>(
          SleeveUsage.$_createMessage);
  static SleeveUsage? _defaultInstance;

  /// cpu_percent is CPU used over the last sample interval; 100 is one core.
  @$pb.TagNumber(1)
  $core.double get cpuPercent => $_getN(0);
  @$pb.TagNumber(1)
  set cpuPercent($core.double value) => $_setDouble(0, value);
  @$pb.TagNumber(1)
  $core.bool hasCpuPercent() => $_has(0);
  @$pb.TagNumber(1)
  void clearCpuPercent() => $_clearField(1);

  @$pb.TagNumber(2)
  $core.double get cpuSeconds => $_getN(1);
  @$pb.TagNumber(2)
  set cpuSeconds($core.double value) => $_setDouble(1, value);
  @$pb.TagNumber(2)
  $core.bool hasCpuSeconds() => $_has(1);
  @$pb.TagNumber(2)
  void clearCpuSeconds() => $_clearField(2);

  @$pb.TagNumber(3)
  $fixnum.Int64 get memoryCurrentBytes => $_getI64(2);
  @$pb.TagNumber(3)
  set memoryCurrentBytes($fixnum.Int64 value) => $_setInt64(2, value);
  @$pb.TagNumber(3)
  $core.bool hasMemoryCurrentBytes() => $_has(2);
  @$pb.TagNumber(3)
  void clearMemoryCurrentBytes() => $_clearField(3);

  @$pb.TagNumber(4)
  $fixnum.Int64 get memoryPeakBytes => $_getI64(3);
  @$pb.TagNumber(4)
  set memoryPeakBytes($fixnum.Int64 value) => $_setInt64(3, value);
  @$pb.TagNumber(4)
  $core.bool hasMemoryPeakBytes() => $_has(3);
  @$pb.TagNumber(4)
  void clearMemoryPeakBytes() => $_clearField(4);

  @$pb.TagNumber(5)
  $core.int get pids => $_getIZ(4);
  @$pb.TagNumber(5)
  set pids($core.int value) => $_setUnsignedInt32(4, value);
  @$pb.TagNumber(5)
  $core.bool hasPids() => $_has(4);
  @$pb.TagNumber(5)
  void clearPids() => $_clearField(5);

  /// oom_kills counts processes the kernel killed for exceeding the memory limit.
  @$pb.TagNumber(6)
  $fixnum.Int64 get oomKills => $_getI64(5);
  @$pb.TagNumber(6)
  set oomKills($fixnum.Int64 value) => $_setInt64(5, value);
  @$pb.TagNumber(6)
  $core.bool hasOomKills() => $_has(5);
  @$pb.TagNumber(6)
  void clearOomKills() => $_clearField(6);

  /// workspace_disk_bytes is measured on a slow interval and is 0 until the
  /// first measurement.
  @$pb.TagNumber(7)
  $fixnum.Int64 get workspaceDiskBytes => $_getI64(6);
  @$pb.TagNumber(7)
  set workspaceDiskBytes($fixnum.Int64 value) => $_setInt64(6, value);
  @$pb.TagNumber(7)
  $core.bool hasWorkspaceDiskBytes() => $_has(6);
  @$pb.TagNumber(7)
  void clearWorkspaceDiskBytes() => $_clearField(7);

  @$pb.TagNumber(8)
  $0.Timestamp get sampledAt => $_getN(7);
  @$pb.TagNumber(8)
  set sampledAt($0.Timestamp value) => $_setField(8, value);
  @$pb.TagNumber(8)
  $core.bool hasSampledAt() => $_has(7);
  @$pb.TagNumber(8)
  void clearSampledAt() => $_clearField(8);
  @$pb.TagNumber(8)
  $0.Timestamp ensureSampledAt() => $_ensure(7);

  @$pb.TagNumber(9)
  $0.Timestamp get diskSampledAt => $_getN(8);
  @$pb.TagNumber(9)
  set diskSampledAt($0.Timestamp value) => $_setField(9, value);
  @$pb.TagNumber(9)
  $core.bool hasDiskSampledAt() => $_has(8);
  @$pb.TagNumber(9)
  void clearDiskSampledAt() => $_clearField(9);
  @$pb.TagNumber(9)
  $0.Timestamp ensureDiskSampledAt() => $_ensure(8);

  @$pb.TagNumber(10)
  $fixnum.Int64 get homeDiskBytes => $_getI64(9);
  @$pb.TagNumber(10)
  set homeDiskBytes($fixnum.Int64 value) => $_setInt64(9, value);
  @$pb.TagNumber(10)
  $core.bool hasHomeDiskBytes() => $_has(9);
  @$pb.TagNumber(10)
  void clearHomeDiskBytes() => $_clearField(10);

  @$pb.TagNumber(11)
  $fixnum.Int64 get cacheDiskBytes => $_getI64(10);
  @$pb.TagNumber(11)
  set cacheDiskBytes($fixnum.Int64 value) => $_setInt64(10, value);
  @$pb.TagNumber(11)
  $core.bool hasCacheDiskBytes() => $_has(10);
  @$pb.TagNumber(11)
  void clearCacheDiskBytes() => $_clearField(11);

  @$pb.TagNumber(12)
  $fixnum.Int64 get goBuildCacheDiskBytes => $_getI64(11);
  @$pb.TagNumber(12)
  set goBuildCacheDiskBytes($fixnum.Int64 value) => $_setInt64(11, value);
  @$pb.TagNumber(12)
  $core.bool hasGoBuildCacheDiskBytes() => $_has(11);
  @$pb.TagNumber(12)
  void clearGoBuildCacheDiskBytes() => $_clearField(12);
}

class Sleeve extends $pb.GeneratedMessage {
  factory Sleeve({
    $core.String? name,
    Harness? harness,
    $core.String? workspace,
    $core.String? session,
    $0.Timestamp? createdAt,
    SleeveState? state,
    SleeveLimits? limits,
    SleeveUsage? usage,
    $core.Iterable<$core.String>? args,
    $0.Timestamp? exitedAt,
    $core.int? exitCode,
    $core.int? exitSignal,
    $core.bool? oomKilled,
    $1.SleeveMode? mode,
    $1.ChatStatus? chat,
    $core.bool? supervised,
    $1.Handoff? handoff,
    $core.Iterable<ReadOnlyMount>? readOnly,
    $2.Duration? exitAfterIdle,
    $core.String? parent,
    $core.String? gitBranch,
    $core.String? baseRepo,
    $core.String? baseBranch,
    $core.String? baseTip,
    $1.SleeveResult? result,
    $core.bool? killed,
    $core.bool? nested,
    $core.String? runId,
    $core.String? harnessSha256,
    $core.String? profileSha256,
  }) {
    final result$ = Sleeve._();
    if (name != null) result$.name = name;
    if (harness != null) result$.harness = harness;
    if (workspace != null) result$.workspace = workspace;
    if (session != null) result$.session = session;
    if (createdAt != null) result$.createdAt = createdAt;
    if (state != null) result$.state = state;
    if (limits != null) result$.limits = limits;
    if (usage != null) result$.usage = usage;
    if (args != null) result$.args.addAll(args);
    if (exitedAt != null) result$.exitedAt = exitedAt;
    if (exitCode != null) result$.exitCode = exitCode;
    if (exitSignal != null) result$.exitSignal = exitSignal;
    if (oomKilled != null) result$.oomKilled = oomKilled;
    if (mode != null) result$.mode = mode;
    if (chat != null) result$.chat = chat;
    if (supervised != null) result$.supervised = supervised;
    if (handoff != null) result$.handoff = handoff;
    if (readOnly != null) result$.readOnly.addAll(readOnly);
    if (exitAfterIdle != null) result$.exitAfterIdle = exitAfterIdle;
    if (parent != null) result$.parent = parent;
    if (gitBranch != null) result$.gitBranch = gitBranch;
    if (baseRepo != null) result$.baseRepo = baseRepo;
    if (baseBranch != null) result$.baseBranch = baseBranch;
    if (baseTip != null) result$.baseTip = baseTip;
    if (result != null) result$.result = result;
    if (killed != null) result$.killed = killed;
    if (nested != null) result$.nested = nested;
    if (runId != null) result$.runId = runId;
    if (harnessSha256 != null) result$.harnessSha256 = harnessSha256;
    if (profileSha256 != null) result$.profileSha256 = profileSha256;
    return result$;
  }

  Sleeve._();

  factory Sleeve.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      Sleeve()..mergeFromBuffer(data, registry);
  factory Sleeve.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      Sleeve()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'Sleeve',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: Sleeve.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'name')
    ..aE<Harness>(2, _omitFieldNames ? '' : 'harness',
        enumValues: Harness.values)
    ..aOS(3, _omitFieldNames ? '' : 'workspace')
    ..aOS(4, _omitFieldNames ? '' : 'session')
    ..aOM<$0.Timestamp>(5, _omitFieldNames ? '' : 'createdAt',
        subBuilder: $0.Timestamp.$_createMessage)
    ..aE<SleeveState>(6, _omitFieldNames ? '' : 'state',
        enumValues: SleeveState.values)
    ..aOM<SleeveLimits>(7, _omitFieldNames ? '' : 'limits',
        subBuilder: SleeveLimits.$_createMessage)
    ..aOM<SleeveUsage>(8, _omitFieldNames ? '' : 'usage',
        subBuilder: SleeveUsage.$_createMessage)
    ..pPS(9, _omitFieldNames ? '' : 'args')
    ..aOM<$0.Timestamp>(10, _omitFieldNames ? '' : 'exitedAt',
        subBuilder: $0.Timestamp.$_createMessage)
    ..aI(11, _omitFieldNames ? '' : 'exitCode')
    ..aI(12, _omitFieldNames ? '' : 'exitSignal')
    ..aOB(13, _omitFieldNames ? '' : 'oomKilled')
    ..aE<$1.SleeveMode>(14, _omitFieldNames ? '' : 'mode',
        enumValues: $1.SleeveMode.values)
    ..aOM<$1.ChatStatus>(15, _omitFieldNames ? '' : 'chat',
        subBuilder: $1.ChatStatus.$_createMessage)
    ..aOB(16, _omitFieldNames ? '' : 'supervised')
    ..aOM<$1.Handoff>(17, _omitFieldNames ? '' : 'handoff',
        subBuilder: $1.Handoff.$_createMessage)
    ..pPM<ReadOnlyMount>(18, _omitFieldNames ? '' : 'readOnly',
        subBuilder: ReadOnlyMount.$_createMessage)
    ..aOM<$2.Duration>(19, _omitFieldNames ? '' : 'exitAfterIdle',
        subBuilder: $2.Duration.$_createMessage)
    ..aOS(20, _omitFieldNames ? '' : 'parent')
    ..aOS(21, _omitFieldNames ? '' : 'gitBranch')
    ..aOS(22, _omitFieldNames ? '' : 'baseRepo')
    ..aOS(23, _omitFieldNames ? '' : 'baseBranch')
    ..aOS(24, _omitFieldNames ? '' : 'baseTip')
    ..aOM<$1.SleeveResult>(25, _omitFieldNames ? '' : 'result',
        subBuilder: $1.SleeveResult.$_createMessage)
    ..aOB(28, _omitFieldNames ? '' : 'killed')
    ..aOB(30, _omitFieldNames ? '' : 'nested')
    ..aOS(31, _omitFieldNames ? '' : 'runId')
    ..aOS(32, _omitFieldNames ? '' : 'harnessSha256')
    ..aOS(33, _omitFieldNames ? '' : 'profileSha256')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  Sleeve clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  Sleeve copyWith(void Function(Sleeve) updates) =>
      super.copyWith((message) => updates(message as Sleeve)) as Sleeve;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use Sleeve() / Sleeve.new instead')
  static Sleeve create() => Sleeve._();
  static $pb.GeneratedMessage $_createMessage() => Sleeve._();
  @$core.override
  Sleeve createEmptyInstance() => Sleeve._();
  @$core.pragma('dart2js:noInline')
  static Sleeve getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<Sleeve>(Sleeve.$_createMessage);
  static Sleeve? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get name => $_getSZ(0);
  @$pb.TagNumber(1)
  set name($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasName() => $_has(0);
  @$pb.TagNumber(1)
  void clearName() => $_clearField(1);

  @$pb.TagNumber(2)
  Harness get harness => $_getN(1);
  @$pb.TagNumber(2)
  set harness(Harness value) => $_setField(2, value);
  @$pb.TagNumber(2)
  $core.bool hasHarness() => $_has(1);
  @$pb.TagNumber(2)
  void clearHarness() => $_clearField(2);

  /// workspace is the absolute path bound read-write into the sandbox.
  @$pb.TagNumber(3)
  $core.String get workspace => $_getSZ(2);
  @$pb.TagNumber(3)
  set workspace($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasWorkspace() => $_has(2);
  @$pb.TagNumber(3)
  void clearWorkspace() => $_clearField(3);

  /// session is the tmux session name to use with SessionService and the terminal.
  @$pb.TagNumber(4)
  $core.String get session => $_getSZ(3);
  @$pb.TagNumber(4)
  set session($core.String value) => $_setString(3, value);
  @$pb.TagNumber(4)
  $core.bool hasSession() => $_has(3);
  @$pb.TagNumber(4)
  void clearSession() => $_clearField(4);

  @$pb.TagNumber(5)
  $0.Timestamp get createdAt => $_getN(4);
  @$pb.TagNumber(5)
  set createdAt($0.Timestamp value) => $_setField(5, value);
  @$pb.TagNumber(5)
  $core.bool hasCreatedAt() => $_has(4);
  @$pb.TagNumber(5)
  void clearCreatedAt() => $_clearField(5);
  @$pb.TagNumber(5)
  $0.Timestamp ensureCreatedAt() => $_ensure(4);

  @$pb.TagNumber(6)
  SleeveState get state => $_getN(5);
  @$pb.TagNumber(6)
  set state(SleeveState value) => $_setField(6, value);
  @$pb.TagNumber(6)
  $core.bool hasState() => $_has(5);
  @$pb.TagNumber(6)
  void clearState() => $_clearField(6);

  @$pb.TagNumber(7)
  SleeveLimits get limits => $_getN(6);
  @$pb.TagNumber(7)
  set limits(SleeveLimits value) => $_setField(7, value);
  @$pb.TagNumber(7)
  $core.bool hasLimits() => $_has(6);
  @$pb.TagNumber(7)
  void clearLimits() => $_clearField(7);
  @$pb.TagNumber(7)
  SleeveLimits ensureLimits() => $_ensure(6);

  @$pb.TagNumber(8)
  SleeveUsage get usage => $_getN(7);
  @$pb.TagNumber(8)
  set usage(SleeveUsage value) => $_setField(8, value);
  @$pb.TagNumber(8)
  $core.bool hasUsage() => $_has(7);
  @$pb.TagNumber(8)
  void clearUsage() => $_clearField(8);
  @$pb.TagNumber(8)
  SleeveUsage ensureUsage() => $_ensure(7);

  @$pb.TagNumber(9)
  $pb.PbList<$core.String> get args => $_getList(8);

  @$pb.TagNumber(10)
  $0.Timestamp get exitedAt => $_getN(9);
  @$pb.TagNumber(10)
  set exitedAt($0.Timestamp value) => $_setField(10, value);
  @$pb.TagNumber(10)
  $core.bool hasExitedAt() => $_has(9);
  @$pb.TagNumber(10)
  void clearExitedAt() => $_clearField(10);
  @$pb.TagNumber(10)
  $0.Timestamp ensureExitedAt() => $_ensure(9);

  /// exit_code is the harness's exit status, or 128+exit_signal when a signal
  /// ended it. Unset while running and when the exit status is unknown, such
  /// as a session killed outside the hub.
  @$pb.TagNumber(11)
  $core.int get exitCode => $_getIZ(10);
  @$pb.TagNumber(11)
  set exitCode($core.int value) => $_setSignedInt32(10, value);
  @$pb.TagNumber(11)
  $core.bool hasExitCode() => $_has(10);
  @$pb.TagNumber(11)
  void clearExitCode() => $_clearField(11);

  @$pb.TagNumber(12)
  $core.int get exitSignal => $_getIZ(11);
  @$pb.TagNumber(12)
  set exitSignal($core.int value) => $_setSignedInt32(11, value);
  @$pb.TagNumber(12)
  $core.bool hasExitSignal() => $_has(11);
  @$pb.TagNumber(12)
  void clearExitSignal() => $_clearField(12);

  /// oom_killed is true when the harness exited with SIGKILL status (137) and
  /// the kernel's OOM killer had fired in its memory cgroup. The OOM kill is
  /// read from the cgroup before it disappears; a harness that dies within a
  /// few milliseconds of the kill may still be reported as a plain exit 137.
  @$pb.TagNumber(13)
  $core.bool get oomKilled => $_getBF(12);
  @$pb.TagNumber(13)
  set oomKilled($core.bool value) => $_setBool(12, value);
  @$pb.TagNumber(13)
  $core.bool hasOomKilled() => $_has(12);
  @$pb.TagNumber(13)
  void clearOomKilled() => $_clearField(13);

  @$pb.TagNumber(14)
  $1.SleeveMode get mode => $_getN(13);
  @$pb.TagNumber(14)
  set mode($1.SleeveMode value) => $_setField(14, value);
  @$pb.TagNumber(14)
  $core.bool hasMode() => $_has(13);
  @$pb.TagNumber(14)
  void clearMode() => $_clearField(14);

  /// chat is set for sleeves in chat mode.
  @$pb.TagNumber(15)
  $1.ChatStatus get chat => $_getN(14);
  @$pb.TagNumber(15)
  set chat($1.ChatStatus value) => $_setField(15, value);
  @$pb.TagNumber(15)
  $core.bool hasChat() => $_has(14);
  @$pb.TagNumber(15)
  void clearChat() => $_clearField(15);
  @$pb.TagNumber(15)
  $1.ChatStatus ensureChat() => $_ensure(14);

  /// supervised chat sleeves run the harness in its asking permission mode:
  /// approvals arrive as ApprovalRequest events, answered with
  /// ChatService.Respond. Unsupervised sleeves run autonomous (the sandbox is
  /// the boundary).
  @$pb.TagNumber(16)
  $core.bool get supervised => $_getBF(15);
  @$pb.TagNumber(16)
  set supervised($core.bool value) => $_setBool(15, value);
  @$pb.TagNumber(16)
  $core.bool hasSupervised() => $_has(15);
  @$pb.TagNumber(16)
  void clearSupervised() => $_clearField(16);

  /// handoff is set when a resleeve produced this sleeve's harness.
  @$pb.TagNumber(17)
  $1.Handoff get handoff => $_getN(16);
  @$pb.TagNumber(17)
  set handoff($1.Handoff value) => $_setField(17, value);
  @$pb.TagNumber(17)
  $core.bool hasHandoff() => $_has(16);
  @$pb.TagNumber(17)
  void clearHandoff() => $_clearField(17);
  @$pb.TagNumber(17)
  $1.Handoff ensureHandoff() => $_ensure(16);

  @$pb.TagNumber(18)
  $pb.PbList<ReadOnlyMount> get readOnly => $_getList(17);

  @$pb.TagNumber(19)
  $2.Duration get exitAfterIdle => $_getN(18);
  @$pb.TagNumber(19)
  set exitAfterIdle($2.Duration value) => $_setField(19, value);
  @$pb.TagNumber(19)
  $core.bool hasExitAfterIdle() => $_has(18);
  @$pb.TagNumber(19)
  void clearExitAfterIdle() => $_clearField(19);
  @$pb.TagNumber(19)
  $2.Duration ensureExitAfterIdle() => $_ensure(18);

  /// parent is the authenticated identity that spawned this sleeve.
  @$pb.TagNumber(20)
  $core.String get parent => $_getSZ(19);
  @$pb.TagNumber(20)
  set parent($core.String value) => $_setString(19, value);
  @$pb.TagNumber(20)
  $core.bool hasParent() => $_has(19);
  @$pb.TagNumber(20)
  void clearParent() => $_clearField(20);

  /// Current branch read from .git files; empty for non-git and worktrees.
  @$pb.TagNumber(21)
  $core.String get gitBranch => $_getSZ(20);
  @$pb.TagNumber(21)
  set gitBranch($core.String value) => $_setString(20, value);
  @$pb.TagNumber(21)
  $core.bool hasGitBranch() => $_has(20);
  @$pb.TagNumber(21)
  void clearGitBranch() => $_clearField(21);

  @$pb.TagNumber(22)
  $core.String get baseRepo => $_getSZ(21);
  @$pb.TagNumber(22)
  set baseRepo($core.String value) => $_setString(21, value);
  @$pb.TagNumber(22)
  $core.bool hasBaseRepo() => $_has(21);
  @$pb.TagNumber(22)
  void clearBaseRepo() => $_clearField(22);

  @$pb.TagNumber(23)
  $core.String get baseBranch => $_getSZ(22);
  @$pb.TagNumber(23)
  set baseBranch($core.String value) => $_setString(22, value);
  @$pb.TagNumber(23)
  $core.bool hasBaseBranch() => $_has(22);
  @$pb.TagNumber(23)
  void clearBaseBranch() => $_clearField(23);

  @$pb.TagNumber(24)
  $core.String get baseTip => $_getSZ(23);
  @$pb.TagNumber(24)
  set baseTip($core.String value) => $_setString(23, value);
  @$pb.TagNumber(24)
  $core.bool hasBaseTip() => $_has(23);
  @$pb.TagNumber(24)
  void clearBaseTip() => $_clearField(24);

  /// Latest self-reported result for the current brief, independent of mail turns.
  @$pb.TagNumber(25)
  $1.SleeveResult get result => $_getN(24);
  @$pb.TagNumber(25)
  set result($1.SleeveResult value) => $_setField(25, value);
  @$pb.TagNumber(25)
  $core.bool hasResult() => $_has(24);
  @$pb.TagNumber(25)
  void clearResult() => $_clearField(25);
  @$pb.TagNumber(25)
  $1.SleeveResult ensureResult() => $_ensure(24);

  /// Killed records remain inspectable until their name is respawned.
  @$pb.TagNumber(28)
  $core.bool get killed => $_getBF(25);
  @$pb.TagNumber(28)
  set killed($core.bool value) => $_setBool(25, value);
  @$pb.TagNumber(28)
  $core.bool hasKilled() => $_has(25);
  @$pb.TagNumber(28)
  void clearKilled() => $_clearField(28);

  /// Allows sandboxed child sleeves within this sleeve's resource limits.
  @$pb.TagNumber(30)
  $core.bool get nested => $_getBF(26);
  @$pb.TagNumber(30)
  set nested($core.bool value) => $_setBool(26, value);
  @$pb.TagNumber(30)
  $core.bool hasNested() => $_has(26);
  @$pb.TagNumber(30)
  void clearNested() => $_clearField(30);

  /// run_id is minted at spawn and kept across resleeves and hub restarts. It
  /// marks the sleeve's audit entries, jobs, mail, leases, checkpoints and
  /// egress decisions; filter with ListAuditEntries.run_id.
  @$pb.TagNumber(31)
  $core.String get runId => $_getSZ(27);
  @$pb.TagNumber(31)
  set runId($core.String value) => $_setString(27, value);
  @$pb.TagNumber(31)
  $core.bool hasRunId() => $_has(27);
  @$pb.TagNumber(31)
  void clearRunId() => $_clearField(31);

  /// harness_sha256 and profile_sha256 pin this harness's last approved
  /// launch: the executable's bytes, and the sandbox inputs (mounts, tool
  /// roots, limits, environment, egress allowlist) and token scopes. A later
  /// launch that differs is recorded when the owner asked for it and needs
  /// owner approval (ApprovalService) when Envoy, a sleeve or the hub itself
  /// did.
  @$pb.TagNumber(32)
  $core.String get harnessSha256 => $_getSZ(28);
  @$pb.TagNumber(32)
  set harnessSha256($core.String value) => $_setString(28, value);
  @$pb.TagNumber(32)
  $core.bool hasHarnessSha256() => $_has(28);
  @$pb.TagNumber(32)
  void clearHarnessSha256() => $_clearField(32);

  @$pb.TagNumber(33)
  $core.String get profileSha256 => $_getSZ(29);
  @$pb.TagNumber(33)
  set profileSha256($core.String value) => $_setString(29, value);
  @$pb.TagNumber(33)
  $core.bool hasProfileSha256() => $_has(29);
  @$pb.TagNumber(33)
  void clearProfileSha256() => $_clearField(33);
}

class BrowseWorkspacesRequest extends $pb.GeneratedMessage {
  factory BrowseWorkspacesRequest({
    $core.String? path,
  }) {
    final result = BrowseWorkspacesRequest._();
    if (path != null) result.path = path;
    return result;
  }

  BrowseWorkspacesRequest._();

  factory BrowseWorkspacesRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      BrowseWorkspacesRequest()..mergeFromBuffer(data, registry);
  factory BrowseWorkspacesRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      BrowseWorkspacesRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'BrowseWorkspacesRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: BrowseWorkspacesRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'path')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  BrowseWorkspacesRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  BrowseWorkspacesRequest copyWith(
          void Function(BrowseWorkspacesRequest) updates) =>
      super.copyWith((message) => updates(message as BrowseWorkspacesRequest))
          as BrowseWorkspacesRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use BrowseWorkspacesRequest() / BrowseWorkspacesRequest.new instead')
  static BrowseWorkspacesRequest create() => BrowseWorkspacesRequest._();
  static $pb.GeneratedMessage $_createMessage() => BrowseWorkspacesRequest._();
  @$core.override
  BrowseWorkspacesRequest createEmptyInstance() => BrowseWorkspacesRequest._();
  @$core.pragma('dart2js:noInline')
  static BrowseWorkspacesRequest getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<BrowseWorkspacesRequest>(
          BrowseWorkspacesRequest.$_createMessage);
  static BrowseWorkspacesRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get path => $_getSZ(0);
  @$pb.TagNumber(1)
  set path($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasPath() => $_has(0);
  @$pb.TagNumber(1)
  void clearPath() => $_clearField(1);
}

class WorkspaceDirectory extends $pb.GeneratedMessage {
  factory WorkspaceDirectory({
    $core.String? name,
    $core.String? path,
    $core.bool? isGit,
    $core.String? gitBranch,
    $core.String? gitRemote,
    $core.bool? acceptable,
    $core.String? reason,
    $core.String? requiredName,
  }) {
    final result = WorkspaceDirectory._();
    if (name != null) result.name = name;
    if (path != null) result.path = path;
    if (isGit != null) result.isGit = isGit;
    if (gitBranch != null) result.gitBranch = gitBranch;
    if (gitRemote != null) result.gitRemote = gitRemote;
    if (acceptable != null) result.acceptable = acceptable;
    if (reason != null) result.reason = reason;
    if (requiredName != null) result.requiredName = requiredName;
    return result;
  }

  WorkspaceDirectory._();

  factory WorkspaceDirectory.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      WorkspaceDirectory()..mergeFromBuffer(data, registry);
  factory WorkspaceDirectory.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      WorkspaceDirectory()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'WorkspaceDirectory',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: WorkspaceDirectory.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'name')
    ..aOS(2, _omitFieldNames ? '' : 'path')
    ..aOB(3, _omitFieldNames ? '' : 'isGit')
    ..aOS(4, _omitFieldNames ? '' : 'gitBranch')
    ..aOS(5, _omitFieldNames ? '' : 'gitRemote')
    ..aOB(6, _omitFieldNames ? '' : 'acceptable')
    ..aOS(7, _omitFieldNames ? '' : 'reason')
    ..aOS(8, _omitFieldNames ? '' : 'requiredName')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  WorkspaceDirectory clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  WorkspaceDirectory copyWith(void Function(WorkspaceDirectory) updates) =>
      super.copyWith((message) => updates(message as WorkspaceDirectory))
          as WorkspaceDirectory;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use WorkspaceDirectory() / WorkspaceDirectory.new instead')
  static WorkspaceDirectory create() => WorkspaceDirectory._();
  static $pb.GeneratedMessage $_createMessage() => WorkspaceDirectory._();
  @$core.override
  WorkspaceDirectory createEmptyInstance() => WorkspaceDirectory._();
  @$core.pragma('dart2js:noInline')
  static WorkspaceDirectory getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<WorkspaceDirectory>(
          WorkspaceDirectory.$_createMessage);
  static WorkspaceDirectory? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get name => $_getSZ(0);
  @$pb.TagNumber(1)
  set name($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasName() => $_has(0);
  @$pb.TagNumber(1)
  void clearName() => $_clearField(1);

  @$pb.TagNumber(2)
  $core.String get path => $_getSZ(1);
  @$pb.TagNumber(2)
  set path($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasPath() => $_has(1);
  @$pb.TagNumber(2)
  void clearPath() => $_clearField(2);

  @$pb.TagNumber(3)
  $core.bool get isGit => $_getBF(2);
  @$pb.TagNumber(3)
  set isGit($core.bool value) => $_setBool(2, value);
  @$pb.TagNumber(3)
  $core.bool hasIsGit() => $_has(2);
  @$pb.TagNumber(3)
  void clearIsGit() => $_clearField(3);

  @$pb.TagNumber(4)
  $core.String get gitBranch => $_getSZ(3);
  @$pb.TagNumber(4)
  set gitBranch($core.String value) => $_setString(3, value);
  @$pb.TagNumber(4)
  $core.bool hasGitBranch() => $_has(3);
  @$pb.TagNumber(4)
  void clearGitBranch() => $_clearField(4);

  @$pb.TagNumber(5)
  $core.String get gitRemote => $_getSZ(4);
  @$pb.TagNumber(5)
  set gitRemote($core.String value) => $_setString(4, value);
  @$pb.TagNumber(5)
  $core.bool hasGitRemote() => $_has(4);
  @$pb.TagNumber(5)
  void clearGitRemote() => $_clearField(5);

  @$pb.TagNumber(6)
  $core.bool get acceptable => $_getBF(5);
  @$pb.TagNumber(6)
  set acceptable($core.bool value) => $_setBool(5, value);
  @$pb.TagNumber(6)
  $core.bool hasAcceptable() => $_has(5);
  @$pb.TagNumber(6)
  void clearAcceptable() => $_clearField(6);

  @$pb.TagNumber(7)
  $core.String get reason => $_getSZ(6);
  @$pb.TagNumber(7)
  set reason($core.String value) => $_setString(6, value);
  @$pb.TagNumber(7)
  $core.bool hasReason() => $_has(6);
  @$pb.TagNumber(7)
  void clearReason() => $_clearField(7);

  @$pb.TagNumber(8)
  $core.String get requiredName => $_getSZ(7);
  @$pb.TagNumber(8)
  set requiredName($core.String value) => $_setString(7, value);
  @$pb.TagNumber(8)
  $core.bool hasRequiredName() => $_has(7);
  @$pb.TagNumber(8)
  void clearRequiredName() => $_clearField(8);
}

class BrowseWorkspacesResponse extends $pb.GeneratedMessage {
  factory BrowseWorkspacesResponse({
    $core.String? path,
    $core.String? parent,
    $core.Iterable<WorkspaceDirectory>? directories,
    $core.bool? truncated,
  }) {
    final result = BrowseWorkspacesResponse._();
    if (path != null) result.path = path;
    if (parent != null) result.parent = parent;
    if (directories != null) result.directories.addAll(directories);
    if (truncated != null) result.truncated = truncated;
    return result;
  }

  BrowseWorkspacesResponse._();

  factory BrowseWorkspacesResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      BrowseWorkspacesResponse()..mergeFromBuffer(data, registry);
  factory BrowseWorkspacesResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      BrowseWorkspacesResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'BrowseWorkspacesResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: BrowseWorkspacesResponse.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'path')
    ..aOS(2, _omitFieldNames ? '' : 'parent')
    ..pPM<WorkspaceDirectory>(3, _omitFieldNames ? '' : 'directories',
        subBuilder: WorkspaceDirectory.$_createMessage)
    ..aOB(4, _omitFieldNames ? '' : 'truncated')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  BrowseWorkspacesResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  BrowseWorkspacesResponse copyWith(
          void Function(BrowseWorkspacesResponse) updates) =>
      super.copyWith((message) => updates(message as BrowseWorkspacesResponse))
          as BrowseWorkspacesResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use BrowseWorkspacesResponse() / BrowseWorkspacesResponse.new instead')
  static BrowseWorkspacesResponse create() => BrowseWorkspacesResponse._();
  static $pb.GeneratedMessage $_createMessage() => BrowseWorkspacesResponse._();
  @$core.override
  BrowseWorkspacesResponse createEmptyInstance() =>
      BrowseWorkspacesResponse._();
  @$core.pragma('dart2js:noInline')
  static BrowseWorkspacesResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<BrowseWorkspacesResponse>(
          BrowseWorkspacesResponse.$_createMessage);
  static BrowseWorkspacesResponse? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get path => $_getSZ(0);
  @$pb.TagNumber(1)
  set path($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasPath() => $_has(0);
  @$pb.TagNumber(1)
  void clearPath() => $_clearField(1);

  /// Empty at a root; clients return to the root list from there.
  @$pb.TagNumber(2)
  $core.String get parent => $_getSZ(1);
  @$pb.TagNumber(2)
  set parent($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasParent() => $_has(1);
  @$pb.TagNumber(2)
  void clearParent() => $_clearField(2);

  @$pb.TagNumber(3)
  $pb.PbList<WorkspaceDirectory> get directories => $_getList(2);

  @$pb.TagNumber(4)
  $core.bool get truncated => $_getBF(3);
  @$pb.TagNumber(4)
  set truncated($core.bool value) => $_setBool(3, value);
  @$pb.TagNumber(4)
  $core.bool hasTruncated() => $_has(3);
  @$pb.TagNumber(4)
  void clearTruncated() => $_clearField(4);
}

class ReadOnlyMount extends $pb.GeneratedMessage {
  factory ReadOnlyMount({
    $core.String? source,
    $core.String? target,
  }) {
    final result = ReadOnlyMount._();
    if (source != null) result.source = source;
    if (target != null) result.target = target;
    return result;
  }

  ReadOnlyMount._();

  factory ReadOnlyMount.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ReadOnlyMount()..mergeFromBuffer(data, registry);
  factory ReadOnlyMount.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ReadOnlyMount()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ReadOnlyMount',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ReadOnlyMount.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'source')
    ..aOS(2, _omitFieldNames ? '' : 'target')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ReadOnlyMount clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ReadOnlyMount copyWith(void Function(ReadOnlyMount) updates) =>
      super.copyWith((message) => updates(message as ReadOnlyMount))
          as ReadOnlyMount;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use ReadOnlyMount() / ReadOnlyMount.new instead')
  static ReadOnlyMount create() => ReadOnlyMount._();
  static $pb.GeneratedMessage $_createMessage() => ReadOnlyMount._();
  @$core.override
  ReadOnlyMount createEmptyInstance() => ReadOnlyMount._();
  @$core.pragma('dart2js:noInline')
  static ReadOnlyMount getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<ReadOnlyMount>(
          ReadOnlyMount.$_createMessage);
  static ReadOnlyMount? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get source => $_getSZ(0);
  @$pb.TagNumber(1)
  set source($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasSource() => $_has(0);
  @$pb.TagNumber(1)
  void clearSource() => $_clearField(1);

  /// Defaults to source when omitted.
  @$pb.TagNumber(2)
  $core.String get target => $_getSZ(1);
  @$pb.TagNumber(2)
  set target($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasTarget() => $_has(1);
  @$pb.TagNumber(2)
  void clearTarget() => $_clearField(2);
}

class SpawnSleeveRequest extends $pb.GeneratedMessage {
  factory SpawnSleeveRequest({
    $core.String? name,
    $core.String? workspace,
    Harness? harness,
    $core.Iterable<$core.String>? args,
    SleeveLimits? limits,
    $core.Iterable<$core.MapEntry<$core.String, $core.String>>? env,
    $1.SleeveMode? mode,
    $core.bool? supervised,
    $1.ModelSelection? selection,
    $core.Iterable<ReadOnlyMount>? readOnly,
    $2.Duration? exitAfterIdle,
    $core.String? branch,
    $core.String? newBranch,
    $core.String? brief,
    $core.bool? nested,
  }) {
    final result = SpawnSleeveRequest._();
    if (name != null) result.name = name;
    if (workspace != null) result.workspace = workspace;
    if (harness != null) result.harness = harness;
    if (args != null) result.args.addAll(args);
    if (limits != null) result.limits = limits;
    if (env != null) result.env.addEntries(env);
    if (mode != null) result.mode = mode;
    if (supervised != null) result.supervised = supervised;
    if (selection != null) result.selection = selection;
    if (readOnly != null) result.readOnly.addAll(readOnly);
    if (exitAfterIdle != null) result.exitAfterIdle = exitAfterIdle;
    if (branch != null) result.branch = branch;
    if (newBranch != null) result.newBranch = newBranch;
    if (brief != null) result.brief = brief;
    if (nested != null) result.nested = nested;
    return result;
  }

  SpawnSleeveRequest._();

  factory SpawnSleeveRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      SpawnSleeveRequest()..mergeFromBuffer(data, registry);
  factory SpawnSleeveRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      SpawnSleeveRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'SpawnSleeveRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: SpawnSleeveRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'name')
    ..aOS(2, _omitFieldNames ? '' : 'workspace')
    ..aE<Harness>(3, _omitFieldNames ? '' : 'harness',
        enumValues: Harness.values)
    ..pPS(4, _omitFieldNames ? '' : 'args')
    ..aOM<SleeveLimits>(5, _omitFieldNames ? '' : 'limits',
        subBuilder: SleeveLimits.$_createMessage)
    ..m<$core.String, $core.String>(6, _omitFieldNames ? '' : 'env',
        entryClassName: 'SpawnSleeveRequest.EnvEntry',
        keyFieldType: $pb.PbFieldType.OS,
        valueFieldType: $pb.PbFieldType.OS,
        packageName: const $pb.PackageName('protectorate.v1'))
    ..aE<$1.SleeveMode>(7, _omitFieldNames ? '' : 'mode',
        enumValues: $1.SleeveMode.values)
    ..aOB(8, _omitFieldNames ? '' : 'supervised')
    ..aOM<$1.ModelSelection>(9, _omitFieldNames ? '' : 'selection',
        subBuilder: $1.ModelSelection.$_createMessage)
    ..pPM<ReadOnlyMount>(10, _omitFieldNames ? '' : 'readOnly',
        subBuilder: ReadOnlyMount.$_createMessage)
    ..aOM<$2.Duration>(11, _omitFieldNames ? '' : 'exitAfterIdle',
        subBuilder: $2.Duration.$_createMessage)
    ..aOS(12, _omitFieldNames ? '' : 'branch')
    ..aOS(13, _omitFieldNames ? '' : 'newBranch')
    ..aOS(14, _omitFieldNames ? '' : 'brief')
    ..aOB(17, _omitFieldNames ? '' : 'nested')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  SpawnSleeveRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  SpawnSleeveRequest copyWith(void Function(SpawnSleeveRequest) updates) =>
      super.copyWith((message) => updates(message as SpawnSleeveRequest))
          as SpawnSleeveRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use SpawnSleeveRequest() / SpawnSleeveRequest.new instead')
  static SpawnSleeveRequest create() => SpawnSleeveRequest._();
  static $pb.GeneratedMessage $_createMessage() => SpawnSleeveRequest._();
  @$core.override
  SpawnSleeveRequest createEmptyInstance() => SpawnSleeveRequest._();
  @$core.pragma('dart2js:noInline')
  static SpawnSleeveRequest getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<SpawnSleeveRequest>(
          SpawnSleeveRequest.$_createMessage);
  static SpawnSleeveRequest? _defaultInstance;

  /// name is 1-40 of a-z 0-9 and '-', starting with a letter or digit.
  @$pb.TagNumber(1)
  $core.String get name => $_getSZ(0);
  @$pb.TagNumber(1)
  set name($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasName() => $_has(0);
  @$pb.TagNumber(1)
  void clearName() => $_clearField(1);

  /// workspace is an absolute directory path on the hub, or a git URL to clone.
  @$pb.TagNumber(2)
  $core.String get workspace => $_getSZ(1);
  @$pb.TagNumber(2)
  set workspace($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasWorkspace() => $_has(1);
  @$pb.TagNumber(2)
  void clearWorkspace() => $_clearField(2);

  @$pb.TagNumber(3)
  Harness get harness => $_getN(2);
  @$pb.TagNumber(3)
  set harness(Harness value) => $_setField(3, value);
  @$pb.TagNumber(3)
  $core.bool hasHarness() => $_has(2);
  @$pb.TagNumber(3)
  void clearHarness() => $_clearField(3);

  /// args are extra arguments for the harness, or the whole command for shell.
  @$pb.TagNumber(4)
  $pb.PbList<$core.String> get args => $_getList(3);

  @$pb.TagNumber(5)
  SleeveLimits get limits => $_getN(4);
  @$pb.TagNumber(5)
  set limits(SleeveLimits value) => $_setField(5, value);
  @$pb.TagNumber(5)
  $core.bool hasLimits() => $_has(4);
  @$pb.TagNumber(5)
  void clearLimits() => $_clearField(5);
  @$pb.TagNumber(5)
  SleeveLimits ensureLimits() => $_ensure(4);

  /// env holds extra environment variables for the harness.
  @$pb.TagNumber(6)
  $pb.PbMap<$core.String, $core.String> get env => $_getMap(5);

  @$pb.TagNumber(7)
  $1.SleeveMode get mode => $_getN(6);
  @$pb.TagNumber(7)
  set mode($1.SleeveMode value) => $_setField(7, value);
  @$pb.TagNumber(7)
  $core.bool hasMode() => $_has(6);
  @$pb.TagNumber(7)
  void clearMode() => $_clearField(7);

  /// supervised runs a chat sleeve's harness in its asking permission mode
  /// instead of autonomous: Claude --permission-mode default, Codex
  /// approvalPolicy untrusted with its workspace-write sandbox, Grok
  /// /always-approve off. pi has no gate and ignores it.
  @$pb.TagNumber(8)
  $core.bool get supervised => $_getBF(7);
  @$pb.TagNumber(8)
  set supervised($core.bool value) => $_setBool(7, value);
  @$pb.TagNumber(8)
  $core.bool hasSupervised() => $_has(7);
  @$pb.TagNumber(8)
  void clearSupervised() => $_clearField(8);

  /// Codex chat selection; omitted effort uses the hub's sleeve effort default.
  /// Other drivers reject explicit selection.
  @$pb.TagNumber(9)
  $1.ModelSelection get selection => $_getN(8);
  @$pb.TagNumber(9)
  set selection($1.ModelSelection value) => $_setField(9, value);
  @$pb.TagNumber(9)
  $core.bool hasSelection() => $_has(8);
  @$pb.TagNumber(9)
  void clearSelection() => $_clearField(9);
  @$pb.TagNumber(9)
  $1.ModelSelection ensureSelection() => $_ensure(8);

  @$pb.TagNumber(10)
  $pb.PbList<ReadOnlyMount> get readOnly => $_getList(9);

  /// Zero disables automatic removal. Applies only to chat sleeves.
  @$pb.TagNumber(11)
  $2.Duration get exitAfterIdle => $_getN(10);
  @$pb.TagNumber(11)
  set exitAfterIdle($2.Duration value) => $_setField(11, value);
  @$pb.TagNumber(11)
  $core.bool hasExitAfterIdle() => $_has(10);
  @$pb.TagNumber(11)
  void clearExitAfterIdle() => $_clearField(11);
  @$pb.TagNumber(11)
  $2.Duration ensureExitAfterIdle() => $_ensure(10);

  /// Branch to clone; local sources default to their current branch.
  @$pb.TagNumber(12)
  $core.String get branch => $_getSZ(11);
  @$pb.TagNumber(12)
  set branch($core.String value) => $_setString(11, value);
  @$pb.TagNumber(12)
  $core.bool hasBranch() => $_has(11);
  @$pb.TagNumber(12)
  void clearBranch() => $_clearField(12);

  /// Create and check out this branch from branch in a fresh clone.
  @$pb.TagNumber(13)
  $core.String get newBranch => $_getSZ(12);
  @$pb.TagNumber(13)
  set newBranch($core.String value) => $_setString(12, value);
  @$pb.TagNumber(13)
  $core.bool hasNewBranch() => $_has(12);
  @$pb.TagNumber(13)
  void clearNewBranch() => $_clearField(13);

  /// First chat message, delivered before spawn succeeds. Chat mode only.
  @$pb.TagNumber(14)
  $core.String get brief => $_getSZ(13);
  @$pb.TagNumber(14)
  set brief($core.String value) => $_setString(13, value);
  @$pb.TagNumber(14)
  $core.bool hasBrief() => $_has(13);
  @$pb.TagNumber(14)
  void clearBrief() => $_clearField(14);

  /// Delegates a private cgroup subtree; requires CPU and memory limits.
  @$pb.TagNumber(17)
  $core.bool get nested => $_getBF(14);
  @$pb.TagNumber(17)
  set nested($core.bool value) => $_setBool(14, value);
  @$pb.TagNumber(17)
  $core.bool hasNested() => $_has(14);
  @$pb.TagNumber(17)
  void clearNested() => $_clearField(17);
}

class SpawnSleeveResponse extends $pb.GeneratedMessage {
  factory SpawnSleeveResponse({
    Sleeve? sleeve,
  }) {
    final result = SpawnSleeveResponse._();
    if (sleeve != null) result.sleeve = sleeve;
    return result;
  }

  SpawnSleeveResponse._();

  factory SpawnSleeveResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      SpawnSleeveResponse()..mergeFromBuffer(data, registry);
  factory SpawnSleeveResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      SpawnSleeveResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'SpawnSleeveResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: SpawnSleeveResponse.$_createMessage)
    ..aOM<Sleeve>(1, _omitFieldNames ? '' : 'sleeve',
        subBuilder: Sleeve.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  SpawnSleeveResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  SpawnSleeveResponse copyWith(void Function(SpawnSleeveResponse) updates) =>
      super.copyWith((message) => updates(message as SpawnSleeveResponse))
          as SpawnSleeveResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core
      .Deprecated('Use SpawnSleeveResponse() / SpawnSleeveResponse.new instead')
  static SpawnSleeveResponse create() => SpawnSleeveResponse._();
  static $pb.GeneratedMessage $_createMessage() => SpawnSleeveResponse._();
  @$core.override
  SpawnSleeveResponse createEmptyInstance() => SpawnSleeveResponse._();
  @$core.pragma('dart2js:noInline')
  static SpawnSleeveResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<SpawnSleeveResponse>(
          SpawnSleeveResponse.$_createMessage);
  static SpawnSleeveResponse? _defaultInstance;

  @$pb.TagNumber(1)
  Sleeve get sleeve => $_getN(0);
  @$pb.TagNumber(1)
  set sleeve(Sleeve value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasSleeve() => $_has(0);
  @$pb.TagNumber(1)
  void clearSleeve() => $_clearField(1);
  @$pb.TagNumber(1)
  Sleeve ensureSleeve() => $_ensure(0);
}

class ListSleevesRequest extends $pb.GeneratedMessage {
  factory ListSleevesRequest() => ListSleevesRequest._();

  ListSleevesRequest._();

  factory ListSleevesRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListSleevesRequest()..mergeFromBuffer(data, registry);
  factory ListSleevesRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListSleevesRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ListSleevesRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ListSleevesRequest.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListSleevesRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListSleevesRequest copyWith(void Function(ListSleevesRequest) updates) =>
      super.copyWith((message) => updates(message as ListSleevesRequest))
          as ListSleevesRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use ListSleevesRequest() / ListSleevesRequest.new instead')
  static ListSleevesRequest create() => ListSleevesRequest._();
  static $pb.GeneratedMessage $_createMessage() => ListSleevesRequest._();
  @$core.override
  ListSleevesRequest createEmptyInstance() => ListSleevesRequest._();
  @$core.pragma('dart2js:noInline')
  static ListSleevesRequest getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<ListSleevesRequest>(
          ListSleevesRequest.$_createMessage);
  static ListSleevesRequest? _defaultInstance;
}

class ListSleevesResponse extends $pb.GeneratedMessage {
  factory ListSleevesResponse({
    $core.Iterable<Sleeve>? sleeves,
  }) {
    final result = ListSleevesResponse._();
    if (sleeves != null) result.sleeves.addAll(sleeves);
    return result;
  }

  ListSleevesResponse._();

  factory ListSleevesResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListSleevesResponse()..mergeFromBuffer(data, registry);
  factory ListSleevesResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListSleevesResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ListSleevesResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ListSleevesResponse.$_createMessage)
    ..pPM<Sleeve>(1, _omitFieldNames ? '' : 'sleeves',
        subBuilder: Sleeve.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListSleevesResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListSleevesResponse copyWith(void Function(ListSleevesResponse) updates) =>
      super.copyWith((message) => updates(message as ListSleevesResponse))
          as ListSleevesResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core
      .Deprecated('Use ListSleevesResponse() / ListSleevesResponse.new instead')
  static ListSleevesResponse create() => ListSleevesResponse._();
  static $pb.GeneratedMessage $_createMessage() => ListSleevesResponse._();
  @$core.override
  ListSleevesResponse createEmptyInstance() => ListSleevesResponse._();
  @$core.pragma('dart2js:noInline')
  static ListSleevesResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<ListSleevesResponse>(
          ListSleevesResponse.$_createMessage);
  static ListSleevesResponse? _defaultInstance;

  @$pb.TagNumber(1)
  $pb.PbList<Sleeve> get sleeves => $_getList(0);
}

class ListWorkspacesRequest extends $pb.GeneratedMessage {
  factory ListWorkspacesRequest() => ListWorkspacesRequest._();

  ListWorkspacesRequest._();

  factory ListWorkspacesRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListWorkspacesRequest()..mergeFromBuffer(data, registry);
  factory ListWorkspacesRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListWorkspacesRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ListWorkspacesRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ListWorkspacesRequest.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListWorkspacesRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListWorkspacesRequest copyWith(
          void Function(ListWorkspacesRequest) updates) =>
      super.copyWith((message) => updates(message as ListWorkspacesRequest))
          as ListWorkspacesRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use ListWorkspacesRequest() / ListWorkspacesRequest.new instead')
  static ListWorkspacesRequest create() => ListWorkspacesRequest._();
  static $pb.GeneratedMessage $_createMessage() => ListWorkspacesRequest._();
  @$core.override
  ListWorkspacesRequest createEmptyInstance() => ListWorkspacesRequest._();
  @$core.pragma('dart2js:noInline')
  static ListWorkspacesRequest getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<ListWorkspacesRequest>(
          ListWorkspacesRequest.$_createMessage);
  static ListWorkspacesRequest? _defaultInstance;
}

class Workspace extends $pb.GeneratedMessage {
  factory Workspace({
    $core.String? name,
    $core.String? path,
    $core.String? gitRemote,
    $core.String? gitBranch,
    $core.String? sleeve,
    $core.bool? busy,
    $core.String? requiredName,
    $core.bool? readOnly,
  }) {
    final result = Workspace._();
    if (name != null) result.name = name;
    if (path != null) result.path = path;
    if (gitRemote != null) result.gitRemote = gitRemote;
    if (gitBranch != null) result.gitBranch = gitBranch;
    if (sleeve != null) result.sleeve = sleeve;
    if (busy != null) result.busy = busy;
    if (requiredName != null) result.requiredName = requiredName;
    if (readOnly != null) result.readOnly = readOnly;
    return result;
  }

  Workspace._();

  factory Workspace.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      Workspace()..mergeFromBuffer(data, registry);
  factory Workspace.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      Workspace()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'Workspace',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: Workspace.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'name')
    ..aOS(2, _omitFieldNames ? '' : 'path')
    ..aOS(3, _omitFieldNames ? '' : 'gitRemote')
    ..aOS(4, _omitFieldNames ? '' : 'gitBranch')
    ..aOS(5, _omitFieldNames ? '' : 'sleeve')
    ..aOB(6, _omitFieldNames ? '' : 'busy')
    ..aOS(7, _omitFieldNames ? '' : 'requiredName')
    ..aOB(8, _omitFieldNames ? '' : 'readOnly')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  Workspace clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  Workspace copyWith(void Function(Workspace) updates) =>
      super.copyWith((message) => updates(message as Workspace)) as Workspace;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use Workspace() / Workspace.new instead')
  static Workspace create() => Workspace._();
  static $pb.GeneratedMessage $_createMessage() => Workspace._();
  @$core.override
  Workspace createEmptyInstance() => Workspace._();
  @$core.pragma('dart2js:noInline')
  static Workspace getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<Workspace>(Workspace.$_createMessage);
  static Workspace? _defaultInstance;

  /// name is the directory's base name.
  @$pb.TagNumber(1)
  $core.String get name => $_getSZ(0);
  @$pb.TagNumber(1)
  set name($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasName() => $_has(0);
  @$pb.TagNumber(1)
  void clearName() => $_clearField(1);

  /// path is the absolute, symlink-resolved directory.
  @$pb.TagNumber(2)
  $core.String get path => $_getSZ(1);
  @$pb.TagNumber(2)
  set path($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasPath() => $_has(1);
  @$pb.TagNumber(2)
  void clearPath() => $_clearField(2);

  /// git_remote is the URL of the origin remote of a git checkout.
  @$pb.TagNumber(3)
  $core.String get gitRemote => $_getSZ(2);
  @$pb.TagNumber(3)
  set gitRemote($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasGitRemote() => $_has(2);
  @$pb.TagNumber(3)
  void clearGitRemote() => $_clearField(3);

  /// git_branch is the checked-out branch of a git checkout; empty for a
  /// detached HEAD or a directory that is not a git checkout.
  @$pb.TagNumber(4)
  $core.String get gitBranch => $_getSZ(3);
  @$pb.TagNumber(4)
  set gitBranch($core.String value) => $_setString(3, value);
  @$pb.TagNumber(4)
  $core.bool hasGitBranch() => $_has(3);
  @$pb.TagNumber(4)
  void clearGitBranch() => $_clearField(4);

  /// sleeve is the sleeve that uses this workspace, running or exited.
  @$pb.TagNumber(5)
  $core.String get sleeve => $_getSZ(4);
  @$pb.TagNumber(5)
  set sleeve($core.String value) => $_setString(4, value);
  @$pb.TagNumber(5)
  $core.bool hasSleeve() => $_has(4);
  @$pb.TagNumber(5)
  void clearSleeve() => $_clearField(5);

  /// busy is true while that sleeve is running; SpawnSleeve then refuses it.
  @$pb.TagNumber(6)
  $core.bool get busy => $_getBF(5);
  @$pb.TagNumber(6)
  set busy($core.bool value) => $_setBool(5, value);
  @$pb.TagNumber(6)
  $core.bool hasBusy() => $_has(5);
  @$pb.TagNumber(6)
  void clearBusy() => $_clearField(6);

  /// required_name is set for a workspace in the hub's state directory: only a
  /// sleeve of exactly that name may use it. Other workspaces take any name.
  @$pb.TagNumber(7)
  $core.String get requiredName => $_getSZ(6);
  @$pb.TagNumber(7)
  set requiredName($core.String value) => $_setString(6, value);
  @$pb.TagNumber(7)
  $core.bool hasRequiredName() => $_has(6);
  @$pb.TagNumber(7)
  void clearRequiredName() => $_clearField(7);

  @$pb.TagNumber(8)
  $core.bool get readOnly => $_getBF(7);
  @$pb.TagNumber(8)
  set readOnly($core.bool value) => $_setBool(7, value);
  @$pb.TagNumber(8)
  $core.bool hasReadOnly() => $_has(7);
  @$pb.TagNumber(8)
  void clearReadOnly() => $_clearField(8);
}

class ListWorkspacesResponse extends $pb.GeneratedMessage {
  factory ListWorkspacesResponse({
    $core.Iterable<Workspace>? workspaces,
  }) {
    final result = ListWorkspacesResponse._();
    if (workspaces != null) result.workspaces.addAll(workspaces);
    return result;
  }

  ListWorkspacesResponse._();

  factory ListWorkspacesResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListWorkspacesResponse()..mergeFromBuffer(data, registry);
  factory ListWorkspacesResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListWorkspacesResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ListWorkspacesResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ListWorkspacesResponse.$_createMessage)
    ..pPM<Workspace>(1, _omitFieldNames ? '' : 'workspaces',
        subBuilder: Workspace.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListWorkspacesResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListWorkspacesResponse copyWith(
          void Function(ListWorkspacesResponse) updates) =>
      super.copyWith((message) => updates(message as ListWorkspacesResponse))
          as ListWorkspacesResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use ListWorkspacesResponse() / ListWorkspacesResponse.new instead')
  static ListWorkspacesResponse create() => ListWorkspacesResponse._();
  static $pb.GeneratedMessage $_createMessage() => ListWorkspacesResponse._();
  @$core.override
  ListWorkspacesResponse createEmptyInstance() => ListWorkspacesResponse._();
  @$core.pragma('dart2js:noInline')
  static ListWorkspacesResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<ListWorkspacesResponse>(
          ListWorkspacesResponse.$_createMessage);
  static ListWorkspacesResponse? _defaultInstance;

  @$pb.TagNumber(1)
  $pb.PbList<Workspace> get workspaces => $_getList(0);
}

class GetSleeveRequest extends $pb.GeneratedMessage {
  factory GetSleeveRequest({
    $core.String? name,
  }) {
    final result = GetSleeveRequest._();
    if (name != null) result.name = name;
    return result;
  }

  GetSleeveRequest._();

  factory GetSleeveRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      GetSleeveRequest()..mergeFromBuffer(data, registry);
  factory GetSleeveRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      GetSleeveRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'GetSleeveRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: GetSleeveRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'name')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  GetSleeveRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  GetSleeveRequest copyWith(void Function(GetSleeveRequest) updates) =>
      super.copyWith((message) => updates(message as GetSleeveRequest))
          as GetSleeveRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use GetSleeveRequest() / GetSleeveRequest.new instead')
  static GetSleeveRequest create() => GetSleeveRequest._();
  static $pb.GeneratedMessage $_createMessage() => GetSleeveRequest._();
  @$core.override
  GetSleeveRequest createEmptyInstance() => GetSleeveRequest._();
  @$core.pragma('dart2js:noInline')
  static GetSleeveRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<GetSleeveRequest>(
          GetSleeveRequest.$_createMessage);
  static GetSleeveRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get name => $_getSZ(0);
  @$pb.TagNumber(1)
  set name($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasName() => $_has(0);
  @$pb.TagNumber(1)
  void clearName() => $_clearField(1);
}

class GetSleeveResponse extends $pb.GeneratedMessage {
  factory GetSleeveResponse({
    Sleeve? sleeve,
  }) {
    final result = GetSleeveResponse._();
    if (sleeve != null) result.sleeve = sleeve;
    return result;
  }

  GetSleeveResponse._();

  factory GetSleeveResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      GetSleeveResponse()..mergeFromBuffer(data, registry);
  factory GetSleeveResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      GetSleeveResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'GetSleeveResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: GetSleeveResponse.$_createMessage)
    ..aOM<Sleeve>(1, _omitFieldNames ? '' : 'sleeve',
        subBuilder: Sleeve.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  GetSleeveResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  GetSleeveResponse copyWith(void Function(GetSleeveResponse) updates) =>
      super.copyWith((message) => updates(message as GetSleeveResponse))
          as GetSleeveResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use GetSleeveResponse() / GetSleeveResponse.new instead')
  static GetSleeveResponse create() => GetSleeveResponse._();
  static $pb.GeneratedMessage $_createMessage() => GetSleeveResponse._();
  @$core.override
  GetSleeveResponse createEmptyInstance() => GetSleeveResponse._();
  @$core.pragma('dart2js:noInline')
  static GetSleeveResponse getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<GetSleeveResponse>(
          GetSleeveResponse.$_createMessage);
  static GetSleeveResponse? _defaultInstance;

  @$pb.TagNumber(1)
  Sleeve get sleeve => $_getN(0);
  @$pb.TagNumber(1)
  set sleeve(Sleeve value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasSleeve() => $_has(0);
  @$pb.TagNumber(1)
  void clearSleeve() => $_clearField(1);
  @$pb.TagNumber(1)
  Sleeve ensureSleeve() => $_ensure(0);
}

class KillSleeveRequest extends $pb.GeneratedMessage {
  factory KillSleeveRequest({
    $core.String? name,
    $core.bool? dropCaches,
  }) {
    final result = KillSleeveRequest._();
    if (name != null) result.name = name;
    if (dropCaches != null) result.dropCaches = dropCaches;
    return result;
  }

  KillSleeveRequest._();

  factory KillSleeveRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      KillSleeveRequest()..mergeFromBuffer(data, registry);
  factory KillSleeveRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      KillSleeveRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'KillSleeveRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: KillSleeveRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'name')
    ..aOB(2, _omitFieldNames ? '' : 'dropCaches')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  KillSleeveRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  KillSleeveRequest copyWith(void Function(KillSleeveRequest) updates) =>
      super.copyWith((message) => updates(message as KillSleeveRequest))
          as KillSleeveRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use KillSleeveRequest() / KillSleeveRequest.new instead')
  static KillSleeveRequest create() => KillSleeveRequest._();
  static $pb.GeneratedMessage $_createMessage() => KillSleeveRequest._();
  @$core.override
  KillSleeveRequest createEmptyInstance() => KillSleeveRequest._();
  @$core.pragma('dart2js:noInline')
  static KillSleeveRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<KillSleeveRequest>(
          KillSleeveRequest.$_createMessage);
  static KillSleeveRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get name => $_getSZ(0);
  @$pb.TagNumber(1)
  set name($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasName() => $_has(0);
  @$pb.TagNumber(1)
  void clearName() => $_clearField(1);

  /// Owner only: remove regenerable home caches after stopping the sleeve.
  @$pb.TagNumber(2)
  $core.bool get dropCaches => $_getBF(1);
  @$pb.TagNumber(2)
  set dropCaches($core.bool value) => $_setBool(1, value);
  @$pb.TagNumber(2)
  $core.bool hasDropCaches() => $_has(1);
  @$pb.TagNumber(2)
  void clearDropCaches() => $_clearField(2);
}

class KillSleeveResponse extends $pb.GeneratedMessage {
  factory KillSleeveResponse() => KillSleeveResponse._();

  KillSleeveResponse._();

  factory KillSleeveResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      KillSleeveResponse()..mergeFromBuffer(data, registry);
  factory KillSleeveResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      KillSleeveResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'KillSleeveResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: KillSleeveResponse.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  KillSleeveResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  KillSleeveResponse copyWith(void Function(KillSleeveResponse) updates) =>
      super.copyWith((message) => updates(message as KillSleeveResponse))
          as KillSleeveResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use KillSleeveResponse() / KillSleeveResponse.new instead')
  static KillSleeveResponse create() => KillSleeveResponse._();
  static $pb.GeneratedMessage $_createMessage() => KillSleeveResponse._();
  @$core.override
  KillSleeveResponse createEmptyInstance() => KillSleeveResponse._();
  @$core.pragma('dart2js:noInline')
  static KillSleeveResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<KillSleeveResponse>(
          KillSleeveResponse.$_createMessage);
  static KillSleeveResponse? _defaultInstance;
}

class ResleeveSleeveRequest extends $pb.GeneratedMessage {
  factory ResleeveSleeveRequest({
    $core.String? name,
    Harness? to,
    $core.bool? interrupt,
    $2.Duration? wait,
    $1.ModelSelection? selection,
    $core.bool? nativeDefaults,
  }) {
    final result = ResleeveSleeveRequest._();
    if (name != null) result.name = name;
    if (to != null) result.to = to;
    if (interrupt != null) result.interrupt = interrupt;
    if (wait != null) result.wait = wait;
    if (selection != null) result.selection = selection;
    if (nativeDefaults != null) result.nativeDefaults = nativeDefaults;
    return result;
  }

  ResleeveSleeveRequest._();

  factory ResleeveSleeveRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ResleeveSleeveRequest()..mergeFromBuffer(data, registry);
  factory ResleeveSleeveRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ResleeveSleeveRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ResleeveSleeveRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ResleeveSleeveRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'name')
    ..aE<Harness>(2, _omitFieldNames ? '' : 'to', enumValues: Harness.values)
    ..aOB(3, _omitFieldNames ? '' : 'interrupt')
    ..aOM<$2.Duration>(4, _omitFieldNames ? '' : 'wait',
        subBuilder: $2.Duration.$_createMessage)
    ..aOM<$1.ModelSelection>(5, _omitFieldNames ? '' : 'selection',
        subBuilder: $1.ModelSelection.$_createMessage)
    ..aOB(6, _omitFieldNames ? '' : 'nativeDefaults')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ResleeveSleeveRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ResleeveSleeveRequest copyWith(
          void Function(ResleeveSleeveRequest) updates) =>
      super.copyWith((message) => updates(message as ResleeveSleeveRequest))
          as ResleeveSleeveRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use ResleeveSleeveRequest() / ResleeveSleeveRequest.new instead')
  static ResleeveSleeveRequest create() => ResleeveSleeveRequest._();
  static $pb.GeneratedMessage $_createMessage() => ResleeveSleeveRequest._();
  @$core.override
  ResleeveSleeveRequest createEmptyInstance() => ResleeveSleeveRequest._();
  @$core.pragma('dart2js:noInline')
  static ResleeveSleeveRequest getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<ResleeveSleeveRequest>(
          ResleeveSleeveRequest.$_createMessage);
  static ResleeveSleeveRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get name => $_getSZ(0);
  @$pb.TagNumber(1)
  set name($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasName() => $_has(0);
  @$pb.TagNumber(1)
  void clearName() => $_clearField(1);

  /// to is the new harness. UNSPECIFIED means auto: the top recommendation
  /// of QuotaService.Suggest other than the current harness.
  @$pb.TagNumber(2)
  Harness get to => $_getN(1);
  @$pb.TagNumber(2)
  set to(Harness value) => $_setField(2, value);
  @$pb.TagNumber(2)
  $core.bool hasTo() => $_has(1);
  @$pb.TagNumber(2)
  void clearTo() => $_clearField(2);

  /// interrupt cancels a running turn at once instead of waiting for it.
  @$pb.TagNumber(3)
  $core.bool get interrupt => $_getBF(2);
  @$pb.TagNumber(3)
  set interrupt($core.bool value) => $_setBool(2, value);
  @$pb.TagNumber(3)
  $core.bool hasInterrupt() => $_has(2);
  @$pb.TagNumber(3)
  void clearInterrupt() => $_clearField(3);

  /// wait bounds how long a running turn may continue before it is
  /// interrupted. Default 5 minutes.
  @$pb.TagNumber(4)
  $2.Duration get wait => $_getN(3);
  @$pb.TagNumber(4)
  set wait($2.Duration value) => $_setField(4, value);
  @$pb.TagNumber(4)
  $core.bool hasWait() => $_has(3);
  @$pb.TagNumber(4)
  void clearWait() => $_clearField(4);
  @$pb.TagNumber(4)
  $2.Duration ensureWait() => $_ensure(3);

  @$pb.TagNumber(5)
  $1.ModelSelection get selection => $_getN(4);
  @$pb.TagNumber(5)
  set selection($1.ModelSelection value) => $_setField(5, value);
  @$pb.TagNumber(5)
  $core.bool hasSelection() => $_has(4);
  @$pb.TagNumber(5)
  void clearSelection() => $_clearField(5);
  @$pb.TagNumber(5)
  $1.ModelSelection ensureSelection() => $_ensure(4);

  /// Explicitly discard the outgoing selection when switching harnesses.
  @$pb.TagNumber(6)
  $core.bool get nativeDefaults => $_getBF(5);
  @$pb.TagNumber(6)
  set nativeDefaults($core.bool value) => $_setBool(5, value);
  @$pb.TagNumber(6)
  $core.bool hasNativeDefaults() => $_has(5);
  @$pb.TagNumber(6)
  void clearNativeDefaults() => $_clearField(6);
}

class ResleeveSleeveResponse extends $pb.GeneratedMessage {
  factory ResleeveSleeveResponse({
    Sleeve? sleeve,
  }) {
    final result = ResleeveSleeveResponse._();
    if (sleeve != null) result.sleeve = sleeve;
    return result;
  }

  ResleeveSleeveResponse._();

  factory ResleeveSleeveResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ResleeveSleeveResponse()..mergeFromBuffer(data, registry);
  factory ResleeveSleeveResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ResleeveSleeveResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ResleeveSleeveResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ResleeveSleeveResponse.$_createMessage)
    ..aOM<Sleeve>(1, _omitFieldNames ? '' : 'sleeve',
        subBuilder: Sleeve.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ResleeveSleeveResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ResleeveSleeveResponse copyWith(
          void Function(ResleeveSleeveResponse) updates) =>
      super.copyWith((message) => updates(message as ResleeveSleeveResponse))
          as ResleeveSleeveResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use ResleeveSleeveResponse() / ResleeveSleeveResponse.new instead')
  static ResleeveSleeveResponse create() => ResleeveSleeveResponse._();
  static $pb.GeneratedMessage $_createMessage() => ResleeveSleeveResponse._();
  @$core.override
  ResleeveSleeveResponse createEmptyInstance() => ResleeveSleeveResponse._();
  @$core.pragma('dart2js:noInline')
  static ResleeveSleeveResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<ResleeveSleeveResponse>(
          ResleeveSleeveResponse.$_createMessage);
  static ResleeveSleeveResponse? _defaultInstance;

  /// sleeve is the sleeve on its new harness; sleeve.handoff says how the
  /// handoff was made.
  @$pb.TagNumber(1)
  Sleeve get sleeve => $_getN(0);
  @$pb.TagNumber(1)
  set sleeve(Sleeve value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasSleeve() => $_has(0);
  @$pb.TagNumber(1)
  void clearSleeve() => $_clearField(1);
  @$pb.TagNumber(1)
  Sleeve ensureSleeve() => $_ensure(0);
}

class SpawnEnvoyRequest extends $pb.GeneratedMessage {
  factory SpawnEnvoyRequest({
    Harness? harness,
    $core.Iterable<$core.String>? scopes,
    $core.Iterable<$core.String>? addScopes,
    $2.Duration? ttl,
    $1.ModelSelection? selection,
  }) {
    final result = SpawnEnvoyRequest._();
    if (harness != null) result.harness = harness;
    if (scopes != null) result.scopes.addAll(scopes);
    if (addScopes != null) result.addScopes.addAll(addScopes);
    if (ttl != null) result.ttl = ttl;
    if (selection != null) result.selection = selection;
    return result;
  }

  SpawnEnvoyRequest._();

  factory SpawnEnvoyRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      SpawnEnvoyRequest()..mergeFromBuffer(data, registry);
  factory SpawnEnvoyRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      SpawnEnvoyRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'SpawnEnvoyRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: SpawnEnvoyRequest.$_createMessage)
    ..aE<Harness>(1, _omitFieldNames ? '' : 'harness',
        enumValues: Harness.values)
    ..pPS(2, _omitFieldNames ? '' : 'scopes')
    ..pPS(3, _omitFieldNames ? '' : 'addScopes')
    ..aOM<$2.Duration>(4, _omitFieldNames ? '' : 'ttl',
        subBuilder: $2.Duration.$_createMessage)
    ..aOM<$1.ModelSelection>(5, _omitFieldNames ? '' : 'selection',
        subBuilder: $1.ModelSelection.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  SpawnEnvoyRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  SpawnEnvoyRequest copyWith(void Function(SpawnEnvoyRequest) updates) =>
      super.copyWith((message) => updates(message as SpawnEnvoyRequest))
          as SpawnEnvoyRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use SpawnEnvoyRequest() / SpawnEnvoyRequest.new instead')
  static SpawnEnvoyRequest create() => SpawnEnvoyRequest._();
  static $pb.GeneratedMessage $_createMessage() => SpawnEnvoyRequest._();
  @$core.override
  SpawnEnvoyRequest createEmptyInstance() => SpawnEnvoyRequest._();
  @$core.pragma('dart2js:noInline')
  static SpawnEnvoyRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<SpawnEnvoyRequest>(
          SpawnEnvoyRequest.$_createMessage);
  static SpawnEnvoyRequest? _defaultInstance;

  /// harness is the harness to run; UNSPECIFIED picks the top recommendation
  /// of QuotaService.Suggest. HARNESS_SHELL is invalid.
  @$pb.TagNumber(1)
  Harness get harness => $_getN(0);
  @$pb.TagNumber(1)
  set harness(Harness value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasHarness() => $_has(0);
  @$pb.TagNumber(1)
  void clearHarness() => $_clearField(1);

  /// scopes replaces the default scopes (read, events and the non-destructive
  /// backlog, lease, message, sleeve spawn and resleeve tools) when not empty.
  @$pb.TagNumber(2)
  $pb.PbList<$core.String> get scopes => $_getList(1);

  /// add_scopes extends the scopes, defaults or replaced.
  @$pb.TagNumber(3)
  $pb.PbList<$core.String> get addScopes => $_getList(2);

  /// ttl is the token lifetime. Unset means 720h; zero never expires. The hub
  /// renews the token before it expires without restarting the harness.
  @$pb.TagNumber(4)
  $2.Duration get ttl => $_getN(3);
  @$pb.TagNumber(4)
  set ttl($2.Duration value) => $_setField(4, value);
  @$pb.TagNumber(4)
  $core.bool hasTtl() => $_has(3);
  @$pb.TagNumber(4)
  void clearTtl() => $_clearField(4);
  @$pb.TagNumber(4)
  $2.Duration ensureTtl() => $_ensure(3);

  @$pb.TagNumber(5)
  $1.ModelSelection get selection => $_getN(4);
  @$pb.TagNumber(5)
  set selection($1.ModelSelection value) => $_setField(5, value);
  @$pb.TagNumber(5)
  $core.bool hasSelection() => $_has(4);
  @$pb.TagNumber(5)
  void clearSelection() => $_clearField(5);
  @$pb.TagNumber(5)
  $1.ModelSelection ensureSelection() => $_ensure(4);
}

class SpawnEnvoyResponse extends $pb.GeneratedMessage {
  factory SpawnEnvoyResponse({
    Sleeve? sleeve,
    $3.TokenInfo? token,
    $core.String? workspace,
    $core.String? reason,
    $core.bool? replaced,
  }) {
    final result = SpawnEnvoyResponse._();
    if (sleeve != null) result.sleeve = sleeve;
    if (token != null) result.token = token;
    if (workspace != null) result.workspace = workspace;
    if (reason != null) result.reason = reason;
    if (replaced != null) result.replaced = replaced;
    return result;
  }

  SpawnEnvoyResponse._();

  factory SpawnEnvoyResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      SpawnEnvoyResponse()..mergeFromBuffer(data, registry);
  factory SpawnEnvoyResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      SpawnEnvoyResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'SpawnEnvoyResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: SpawnEnvoyResponse.$_createMessage)
    ..aOM<Sleeve>(1, _omitFieldNames ? '' : 'sleeve',
        subBuilder: Sleeve.$_createMessage)
    ..aOM<$3.TokenInfo>(2, _omitFieldNames ? '' : 'token',
        subBuilder: $3.TokenInfo.$_createMessage)
    ..aOS(3, _omitFieldNames ? '' : 'workspace')
    ..aOS(4, _omitFieldNames ? '' : 'reason')
    ..aOB(5, _omitFieldNames ? '' : 'replaced')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  SpawnEnvoyResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  SpawnEnvoyResponse copyWith(void Function(SpawnEnvoyResponse) updates) =>
      super.copyWith((message) => updates(message as SpawnEnvoyResponse))
          as SpawnEnvoyResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use SpawnEnvoyResponse() / SpawnEnvoyResponse.new instead')
  static SpawnEnvoyResponse create() => SpawnEnvoyResponse._();
  static $pb.GeneratedMessage $_createMessage() => SpawnEnvoyResponse._();
  @$core.override
  SpawnEnvoyResponse createEmptyInstance() => SpawnEnvoyResponse._();
  @$core.pragma('dart2js:noInline')
  static SpawnEnvoyResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<SpawnEnvoyResponse>(
          SpawnEnvoyResponse.$_createMessage);
  static SpawnEnvoyResponse? _defaultInstance;

  @$pb.TagNumber(1)
  Sleeve get sleeve => $_getN(0);
  @$pb.TagNumber(1)
  set sleeve(Sleeve value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasSleeve() => $_has(0);
  @$pb.TagNumber(1)
  void clearSleeve() => $_clearField(1);
  @$pb.TagNumber(1)
  Sleeve ensureSleeve() => $_ensure(0);

  /// token describes the minted token; the secret stays in the token file.
  @$pb.TagNumber(2)
  $3.TokenInfo get token => $_getN(1);
  @$pb.TagNumber(2)
  set token($3.TokenInfo value) => $_setField(2, value);
  @$pb.TagNumber(2)
  $core.bool hasToken() => $_has(1);
  @$pb.TagNumber(2)
  void clearToken() => $_clearField(2);
  @$pb.TagNumber(2)
  $3.TokenInfo ensureToken() => $_ensure(1);

  @$pb.TagNumber(3)
  $core.String get workspace => $_getSZ(2);
  @$pb.TagNumber(3)
  set workspace($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasWorkspace() => $_has(2);
  @$pb.TagNumber(3)
  void clearWorkspace() => $_clearField(3);

  /// reason says why the harness was chosen.
  @$pb.TagNumber(4)
  $core.String get reason => $_getSZ(3);
  @$pb.TagNumber(4)
  set reason($core.String value) => $_setString(3, value);
  @$pb.TagNumber(4)
  $core.bool hasReason() => $_has(3);
  @$pb.TagNumber(4)
  void clearReason() => $_clearField(4);

  /// replaced is true when a running Envoy was stopped first.
  @$pb.TagNumber(5)
  $core.bool get replaced => $_getBF(4);
  @$pb.TagNumber(5)
  set replaced($core.bool value) => $_setBool(4, value);
  @$pb.TagNumber(5)
  $core.bool hasReplaced() => $_has(4);
  @$pb.TagNumber(5)
  void clearReplaced() => $_clearField(5);
}

class PruneSleevesRequest extends $pb.GeneratedMessage {
  factory PruneSleevesRequest({
    $core.bool? yes,
    $core.bool? caches,
    $core.bool? homes,
    $core.bool? workspaces,
    $core.bool? running,
    $core.bool? ignored,
  }) {
    final result = PruneSleevesRequest._();
    if (yes != null) result.yes = yes;
    if (caches != null) result.caches = caches;
    if (homes != null) result.homes = homes;
    if (workspaces != null) result.workspaces = workspaces;
    if (running != null) result.running = running;
    if (ignored != null) result.ignored = ignored;
    return result;
  }

  PruneSleevesRequest._();

  factory PruneSleevesRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      PruneSleevesRequest()..mergeFromBuffer(data, registry);
  factory PruneSleevesRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      PruneSleevesRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'PruneSleevesRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: PruneSleevesRequest.$_createMessage)
    ..aOB(1, _omitFieldNames ? '' : 'yes')
    ..aOB(2, _omitFieldNames ? '' : 'caches')
    ..aOB(3, _omitFieldNames ? '' : 'homes')
    ..aOB(4, _omitFieldNames ? '' : 'workspaces')
    ..aOB(5, _omitFieldNames ? '' : 'running')
    ..aOB(6, _omitFieldNames ? '' : 'ignored')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  PruneSleevesRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  PruneSleevesRequest copyWith(void Function(PruneSleevesRequest) updates) =>
      super.copyWith((message) => updates(message as PruneSleevesRequest))
          as PruneSleevesRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core
      .Deprecated('Use PruneSleevesRequest() / PruneSleevesRequest.new instead')
  static PruneSleevesRequest create() => PruneSleevesRequest._();
  static $pb.GeneratedMessage $_createMessage() => PruneSleevesRequest._();
  @$core.override
  PruneSleevesRequest createEmptyInstance() => PruneSleevesRequest._();
  @$core.pragma('dart2js:noInline')
  static PruneSleevesRequest getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<PruneSleevesRequest>(
          PruneSleevesRequest.$_createMessage);
  static PruneSleevesRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.bool get yes => $_getBF(0);
  @$pb.TagNumber(1)
  set yes($core.bool value) => $_setBool(0, value);
  @$pb.TagNumber(1)
  $core.bool hasYes() => $_has(0);
  @$pb.TagNumber(1)
  void clearYes() => $_clearField(1);

  /// With no category selected, caches is the default.
  @$pb.TagNumber(2)
  $core.bool get caches => $_getBF(1);
  @$pb.TagNumber(2)
  set caches($core.bool value) => $_setBool(1, value);
  @$pb.TagNumber(2)
  $core.bool hasCaches() => $_has(1);
  @$pb.TagNumber(2)
  void clearCaches() => $_clearField(2);

  /// homes also selects each sleeve's checkpoint store.
  @$pb.TagNumber(3)
  $core.bool get homes => $_getBF(2);
  @$pb.TagNumber(3)
  set homes($core.bool value) => $_setBool(2, value);
  @$pb.TagNumber(3)
  $core.bool hasHomes() => $_has(2);
  @$pb.TagNumber(3)
  void clearHomes() => $_clearField(3);

  @$pb.TagNumber(4)
  $core.bool get workspaces => $_getBF(3);
  @$pb.TagNumber(4)
  set workspaces($core.bool value) => $_setBool(3, value);
  @$pb.TagNumber(4)
  $core.bool hasWorkspaces() => $_has(3);
  @$pb.TagNumber(4)
  void clearWorkspaces() => $_clearField(4);

  /// Running sleeves are inventoried but always kept.
  @$pb.TagNumber(5)
  $core.bool get running => $_getBF(4);
  @$pb.TagNumber(5)
  set running($core.bool value) => $_setBool(4, value);
  @$pb.TagNumber(5)
  $core.bool hasRunning() => $_has(4);
  @$pb.TagNumber(5)
  void clearRunning() => $_clearField(5);

  /// With workspaces, files ignored by the clone's tracked .gitignore rules
  /// (build output) do not keep a clone; other untracked files still do.
  @$pb.TagNumber(6)
  $core.bool get ignored => $_getBF(5);
  @$pb.TagNumber(6)
  set ignored($core.bool value) => $_setBool(5, value);
  @$pb.TagNumber(6)
  $core.bool hasIgnored() => $_has(5);
  @$pb.TagNumber(6)
  void clearIgnored() => $_clearField(6);
}

class PruneSleevesResponse extends $pb.GeneratedMessage {
  factory PruneSleevesResponse({
    $core.Iterable<SleeveDiskEntry>? entries,
  }) {
    final result = PruneSleevesResponse._();
    if (entries != null) result.entries.addAll(entries);
    return result;
  }

  PruneSleevesResponse._();

  factory PruneSleevesResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      PruneSleevesResponse()..mergeFromBuffer(data, registry);
  factory PruneSleevesResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      PruneSleevesResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'PruneSleevesResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: PruneSleevesResponse.$_createMessage)
    ..pPM<SleeveDiskEntry>(1, _omitFieldNames ? '' : 'entries',
        subBuilder: SleeveDiskEntry.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  PruneSleevesResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  PruneSleevesResponse copyWith(void Function(PruneSleevesResponse) updates) =>
      super.copyWith((message) => updates(message as PruneSleevesResponse))
          as PruneSleevesResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use PruneSleevesResponse() / PruneSleevesResponse.new instead')
  static PruneSleevesResponse create() => PruneSleevesResponse._();
  static $pb.GeneratedMessage $_createMessage() => PruneSleevesResponse._();
  @$core.override
  PruneSleevesResponse createEmptyInstance() => PruneSleevesResponse._();
  @$core.pragma('dart2js:noInline')
  static PruneSleevesResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<PruneSleevesResponse>(
          PruneSleevesResponse.$_createMessage);
  static PruneSleevesResponse? _defaultInstance;

  @$pb.TagNumber(1)
  $pb.PbList<SleeveDiskEntry> get entries => $_getList(0);
}

class SleeveDiskEntry extends $pb.GeneratedMessage {
  factory SleeveDiskEntry({
    $core.String? sleeve,
    $core.String? path,
    $core.String? kind,
    $fixnum.Int64? bytes,
    $core.bool? eligible,
    $core.bool? removed,
    $core.String? reason,
  }) {
    final result = SleeveDiskEntry._();
    if (sleeve != null) result.sleeve = sleeve;
    if (path != null) result.path = path;
    if (kind != null) result.kind = kind;
    if (bytes != null) result.bytes = bytes;
    if (eligible != null) result.eligible = eligible;
    if (removed != null) result.removed = removed;
    if (reason != null) result.reason = reason;
    return result;
  }

  SleeveDiskEntry._();

  factory SleeveDiskEntry.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      SleeveDiskEntry()..mergeFromBuffer(data, registry);
  factory SleeveDiskEntry.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      SleeveDiskEntry()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'SleeveDiskEntry',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: SleeveDiskEntry.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'sleeve')
    ..aOS(2, _omitFieldNames ? '' : 'path')
    ..aOS(3, _omitFieldNames ? '' : 'kind')
    ..a<$fixnum.Int64>(4, _omitFieldNames ? '' : 'bytes', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..aOB(5, _omitFieldNames ? '' : 'eligible')
    ..aOB(6, _omitFieldNames ? '' : 'removed')
    ..aOS(7, _omitFieldNames ? '' : 'reason')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  SleeveDiskEntry clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  SleeveDiskEntry copyWith(void Function(SleeveDiskEntry) updates) =>
      super.copyWith((message) => updates(message as SleeveDiskEntry))
          as SleeveDiskEntry;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use SleeveDiskEntry() / SleeveDiskEntry.new instead')
  static SleeveDiskEntry create() => SleeveDiskEntry._();
  static $pb.GeneratedMessage $_createMessage() => SleeveDiskEntry._();
  @$core.override
  SleeveDiskEntry createEmptyInstance() => SleeveDiskEntry._();
  @$core.pragma('dart2js:noInline')
  static SleeveDiskEntry getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<SleeveDiskEntry>(
          SleeveDiskEntry.$_createMessage);
  static SleeveDiskEntry? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get sleeve => $_getSZ(0);
  @$pb.TagNumber(1)
  set sleeve($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasSleeve() => $_has(0);
  @$pb.TagNumber(1)
  void clearSleeve() => $_clearField(1);

  @$pb.TagNumber(2)
  $core.String get path => $_getSZ(1);
  @$pb.TagNumber(2)
  set path($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasPath() => $_has(1);
  @$pb.TagNumber(2)
  void clearPath() => $_clearField(2);

  @$pb.TagNumber(3)
  $core.String get kind => $_getSZ(2);
  @$pb.TagNumber(3)
  set kind($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasKind() => $_has(2);
  @$pb.TagNumber(3)
  void clearKind() => $_clearField(3);

  @$pb.TagNumber(4)
  $fixnum.Int64 get bytes => $_getI64(3);
  @$pb.TagNumber(4)
  set bytes($fixnum.Int64 value) => $_setInt64(3, value);
  @$pb.TagNumber(4)
  $core.bool hasBytes() => $_has(3);
  @$pb.TagNumber(4)
  void clearBytes() => $_clearField(4);

  @$pb.TagNumber(5)
  $core.bool get eligible => $_getBF(4);
  @$pb.TagNumber(5)
  set eligible($core.bool value) => $_setBool(4, value);
  @$pb.TagNumber(5)
  $core.bool hasEligible() => $_has(4);
  @$pb.TagNumber(5)
  void clearEligible() => $_clearField(5);

  @$pb.TagNumber(6)
  $core.bool get removed => $_getBF(5);
  @$pb.TagNumber(6)
  set removed($core.bool value) => $_setBool(5, value);
  @$pb.TagNumber(6)
  $core.bool hasRemoved() => $_has(5);
  @$pb.TagNumber(6)
  void clearRemoved() => $_clearField(6);

  @$pb.TagNumber(7)
  $core.String get reason => $_getSZ(6);
  @$pb.TagNumber(7)
  set reason($core.String value) => $_setString(6, value);
  @$pb.TagNumber(7)
  $core.bool hasReason() => $_has(6);
  @$pb.TagNumber(7)
  void clearReason() => $_clearField(7);
}

/// SleeveService manages sleeves on the hub machine: one workspace plus one
/// harness, run inside a Linux namespace sandbox in a systemd scope with CPU and
/// memory limits. In chat mode (the default for agent harnesses) the harness
/// runs its official headless protocol and is driven through ChatService. In
/// terminal mode it is a tmux session ("sleeve-<name>"), so CapturePane and the
/// WebSocket terminal work on it. The mode is fixed at spawn. Sleeves are Linux-only.
/// Changes are announced on EventService as "sleeve.spawned", "sleeve.exited",
/// "sleeve.oom", "sleeve.disk_warning", "sleeve.disk_limit" and
/// "sleeve.restored" (subject = sleeve name).
class SleeveServiceApi {
  final $pb.RpcClient _client;

  SleeveServiceApi(this._client);

  /// SpawnSleeve starts a harness in a new isolated sleeve on the hub. The
  /// workspace is an existing directory (bound read-write at the same path) or
  /// a git URL cloned into the hub's workspaces directory. Branch options clone
  /// local checkouts too; brief is delivered as the first chat message.
  /// Respawning the name
  /// of an exited sleeve reuses its home directory and workspace and replaces
  /// its kept session.
  $async.Future<SpawnSleeveResponse> spawnSleeve(
          $pb.ClientContext? ctx, SpawnSleeveRequest request) =>
      _client.invoke<SpawnSleeveResponse>(
          ctx, 'SleeveService', 'SpawnSleeve', request, SpawnSleeveResponse());

  /// ListSleeves returns every known sleeve, running or exited, with its latest
  /// resource usage.
  $async.Future<ListSleevesResponse> listSleeves(
          $pb.ClientContext? ctx, ListSleevesRequest request) =>
      _client.invoke<ListSleevesResponse>(
          ctx, 'SleeveService', 'ListSleeves', request, ListSleevesResponse());

  /// ListWorkspaces returns the directories SpawnSleeve would accept as an
  /// existing workspace: git checkouts and other directories under the hub's
  /// workspaces directory (cloned or created by earlier sleeves), and the
  /// workspaces of known sleeves elsewhere on disk. Read-only reference entries
  /// are also returned with read_only set. It never walks the rest of
  /// the filesystem; any other absolute path can still be given to
  /// SpawnSleeve directly.
  $async.Future<ListWorkspacesResponse> listWorkspaces(
          $pb.ClientContext? ctx, ListWorkspacesRequest request) =>
      _client.invoke<ListWorkspacesResponse>(ctx, 'SleeveService',
          'ListWorkspaces', request, ListWorkspacesResponse());

  /// BrowseWorkspaces lists configured roots when path is empty, otherwise
  /// subdirectories of path. Resolved paths must stay inside a configured root.
  /// Sleeve identity tokens cannot call this RPC, even with read scope.
  $async.Future<BrowseWorkspacesResponse> browseWorkspaces(
          $pb.ClientContext? ctx, BrowseWorkspacesRequest request) =>
      _client.invoke<BrowseWorkspacesResponse>(ctx, 'SleeveService',
          'BrowseWorkspaces', request, BrowseWorkspacesResponse());

  /// GetSleeve returns one sleeve with its latest resource usage.
  $async.Future<GetSleeveResponse> getSleeve(
          $pb.ClientContext? ctx, GetSleeveRequest request) =>
      _client.invoke<GetSleeveResponse>(
          ctx, 'SleeveService', 'GetSleeve', request, GetSleeveResponse());

  /// KillSleeve stops a sleeve's harness and every process in its sandbox and
  /// retains its exited record. The workspace and the sleeve's home directory
  /// are kept on disk unless drop_caches removes regenerable caches.
  $async.Future<KillSleeveResponse> killSleeve(
          $pb.ClientContext? ctx, KillSleeveRequest request) =>
      _client.invoke<KillSleeveResponse>(
          ctx, 'SleeveService', 'KillSleeve', request, KillSleeveResponse());

  /// ResleeveSleeve moves a chat sleeve to another harness on the same
  /// workspace, under the same name, without asking the outgoing harness
  /// anything. The hub takes the chat over (no other sender can start a
  /// turn), waits for it to be IDLE (interrupting a turn that runs past wait),
  /// composes facts in code (git status, diff --stat, recent commits, backlog
  /// and mail, the last user messages and the final assistant message from
  /// the outgoing harness's own transcript), has a separate model write a
  /// Goal/State/Unfinished/Next/Gotchas summary (the subscription with the
  /// most spare quota other than the outgoing harness, else pi with the local
  /// LLM), overwrites <workspace>/.protectorate/HANDOFF.md (kept out of git
  /// through .git/info/exclude, archived under the hub's state directory),
  /// replaces the harness and sends it one plain user message naming the
  /// handoff. The summary runs while the outgoing turn winds down. The old
  /// native session stays on disk for a later resume. If the new harness
  /// cannot start, the old one is respawned on its own session. Announced as
  /// "sleeve.resleeved"; a summary by the fallback model, or none, also emits
  /// "resleeve.warning".
  $async.Future<ResleeveSleeveResponse> resleeveSleeve(
          $pb.ClientContext? ctx, ResleeveSleeveRequest request) =>
      _client.invoke<ResleeveSleeveResponse>(ctx, 'SleeveService',
          'ResleeveSleeve', request, ResleeveSleeveResponse());

  /// SpawnEnvoy creates or respawns Envoy, the orchestrator: a chat sleeve
  /// named "envoy" in $PROTECTORATE_HOME/orchestrator with its own scoped
  /// token. It picks the harness (the top of QuotaService.Suggest unless one
  /// is given), stops a running Envoy, revokes every earlier orchestrator
  /// token, mints a new one, writes the token file and the generated context
  /// files (AGENTS.md, CLAUDE.md, hosts.md) and starts the harness. AGENTS.md
  /// is byte-identical across respawns unless the CLI, the tool catalog, the
  /// hub URL, or the scopes change, so a resumed session keeps its
  /// prompt-cache prefix. Only the owner token may call it, and the token it
  /// mints never holds it.
  $async.Future<SpawnEnvoyResponse> spawnEnvoy(
          $pb.ClientContext? ctx, SpawnEnvoyRequest request) =>
      _client.invoke<SpawnEnvoyResponse>(
          ctx, 'SleeveService', 'SpawnEnvoy', request, SpawnEnvoyResponse());

  /// PruneSleeves inventories disk and removes eligible data only with yes.
  $async.Future<PruneSleevesResponse> pruneSleeves(
          $pb.ClientContext? ctx, PruneSleevesRequest request) =>
      _client.invoke<PruneSleevesResponse>(ctx, 'SleeveService', 'PruneSleeves',
          request, PruneSleevesResponse());

  /// CreateCheckpoint snapshots the sleeve's git workspace: tracked files and
  /// untracked files that are not ignored. The hub also checkpoints a chat
  /// sleeve when a turn settles and before a resleeve. Checkpoints live in a
  /// private store under the hub's state, never in the workspace repository,
  /// so no push can carry them; the newest 20 are kept. If nothing changed
  /// since the latest checkpoint, that checkpoint is returned with unchanged.
  $async.Future<CreateCheckpointResponse> createCheckpoint(
          $pb.ClientContext? ctx, CreateCheckpointRequest request) =>
      _client.invoke<CreateCheckpointResponse>(ctx, 'SleeveService',
          'CreateCheckpoint', request, CreateCheckpointResponse());

  /// ListCheckpoints returns the sleeve's checkpoints, oldest first.
  $async.Future<ListCheckpointsResponse> listCheckpoints(
          $pb.ClientContext? ctx, ListCheckpointsRequest request) =>
      _client.invoke<ListCheckpointsResponse>(ctx, 'SleeveService',
          'ListCheckpoints', request, ListCheckpointsResponse());

  /// DiffCheckpoint returns a unified diff from checkpoint from_id to
  /// checkpoint to_id, or to the current workspace when to_id is 0. Output
  /// beyond 1 MiB is cut off and marked truncated.
  $async.Future<DiffCheckpointResponse> diffCheckpoint(
          $pb.ClientContext? ctx, DiffCheckpointRequest request) =>
      _client.invoke<DiffCheckpointResponse>(ctx, 'SleeveService',
          'DiffCheckpoint', request, DiffCheckpointResponse());

  /// RestoreCheckpoint makes the workspace's non-ignored files match a
  /// checkpoint, removing files the checkpoint lacks. Ignored files, the
  /// workspace's .git directory and its index are untouched. It first
  /// checkpoints the current state, returned as before, so a restore can be
  /// undone. A chat sleeve must be idle.
  $async.Future<RestoreCheckpointResponse> restoreCheckpoint(
          $pb.ClientContext? ctx, RestoreCheckpointRequest request) =>
      _client.invoke<RestoreCheckpointResponse>(ctx, 'SleeveService',
          'RestoreCheckpoint', request, RestoreCheckpointResponse());
}

const $core.bool _omitFieldNames =
    $core.bool.fromEnvironment('protobuf.omit_field_names');
const $core.bool _omitMessageNames =
    $core.bool.fromEnvironment('protobuf.omit_message_names');
