// This is a generated file - do not edit.
//
// Generated from protectorate/v1/node.proto.

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
    as $3;
import 'package:protobuf/well_known_types/google/protobuf/timestamp.pb.dart'
    as $0;

import 'job.pb.dart' as $2;
import 'node.pbenum.dart';
import 'sleeve.pb.dart' as $1;

export 'package:protobuf/protobuf.dart' show GeneratedMessageGenericExtensions;

export 'node.pbenum.dart';

class SetNodeTierRequest extends $pb.GeneratedMessage {
  factory SetNodeTierRequest({
    $core.String? name,
    $core.String? tier,
  }) {
    final result = SetNodeTierRequest._();
    if (name != null) result.name = name;
    if (tier != null) result.tier = tier;
    return result;
  }

  SetNodeTierRequest._();

  factory SetNodeTierRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      SetNodeTierRequest()..mergeFromBuffer(data, registry);
  factory SetNodeTierRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      SetNodeTierRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'SetNodeTierRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: SetNodeTierRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'name')
    ..aOS(2, _omitFieldNames ? '' : 'tier')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  SetNodeTierRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  SetNodeTierRequest copyWith(void Function(SetNodeTierRequest) updates) =>
      super.copyWith((message) => updates(message as SetNodeTierRequest))
          as SetNodeTierRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use SetNodeTierRequest() / SetNodeTierRequest.new instead')
  static SetNodeTierRequest create() => SetNodeTierRequest._();
  static $pb.GeneratedMessage $_createMessage() => SetNodeTierRequest._();
  @$core.override
  SetNodeTierRequest createEmptyInstance() => SetNodeTierRequest._();
  @$core.pragma('dart2js:noInline')
  static SetNodeTierRequest getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<SetNodeTierRequest>(
          SetNodeTierRequest.$_createMessage);
  static SetNodeTierRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get name => $_getSZ(0);
  @$pb.TagNumber(1)
  set name($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasName() => $_has(0);
  @$pb.TagNumber(1)
  void clearName() => $_clearField(1);

  /// tier is "shared" or "care".
  @$pb.TagNumber(2)
  $core.String get tier => $_getSZ(1);
  @$pb.TagNumber(2)
  set tier($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasTier() => $_has(1);
  @$pb.TagNumber(2)
  void clearTier() => $_clearField(2);
}

class UpdateNodeRequest extends $pb.GeneratedMessage {
  factory UpdateNodeRequest({
    $core.String? name,
    $core.bool? force,
  }) {
    final result = UpdateNodeRequest._();
    if (name != null) result.name = name;
    if (force != null) result.force = force;
    return result;
  }

  UpdateNodeRequest._();

  factory UpdateNodeRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      UpdateNodeRequest()..mergeFromBuffer(data, registry);
  factory UpdateNodeRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      UpdateNodeRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'UpdateNodeRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: UpdateNodeRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'name')
    ..aOB(3, _omitFieldNames ? '' : 'force')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  UpdateNodeRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  UpdateNodeRequest copyWith(void Function(UpdateNodeRequest) updates) =>
      super.copyWith((message) => updates(message as UpdateNodeRequest))
          as UpdateNodeRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use UpdateNodeRequest() / UpdateNodeRequest.new instead')
  static UpdateNodeRequest create() => UpdateNodeRequest._();
  static $pb.GeneratedMessage $_createMessage() => UpdateNodeRequest._();
  @$core.override
  UpdateNodeRequest createEmptyInstance() => UpdateNodeRequest._();
  @$core.pragma('dart2js:noInline')
  static UpdateNodeRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<UpdateNodeRequest>(
          UpdateNodeRequest.$_createMessage);
  static UpdateNodeRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get name => $_getSZ(0);
  @$pb.TagNumber(1)
  set name($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasName() => $_has(0);
  @$pb.TagNumber(1)
  void clearName() => $_clearField(1);

  /// force sends the binary even when the node reports the hub's version.
  @$pb.TagNumber(3)
  $core.bool get force => $_getBF(1);
  @$pb.TagNumber(3)
  set force($core.bool value) => $_setBool(1, value);
  @$pb.TagNumber(3)
  $core.bool hasForce() => $_has(1);
  @$pb.TagNumber(3)
  void clearForce() => $_clearField(3);
}

class UpdateNodeProgress extends $pb.GeneratedMessage {
  factory UpdateNodeProgress({
    UpdatePhase? phase,
    $fixnum.Int64? sentBytes,
    $fixnum.Int64? totalBytes,
    $core.String? version,
    $core.String? previousVersion,
    $core.String? restart,
    $core.String? message,
  }) {
    final result = UpdateNodeProgress._();
    if (phase != null) result.phase = phase;
    if (sentBytes != null) result.sentBytes = sentBytes;
    if (totalBytes != null) result.totalBytes = totalBytes;
    if (version != null) result.version = version;
    if (previousVersion != null) result.previousVersion = previousVersion;
    if (restart != null) result.restart = restart;
    if (message != null) result.message = message;
    return result;
  }

  UpdateNodeProgress._();

  factory UpdateNodeProgress.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      UpdateNodeProgress()..mergeFromBuffer(data, registry);
  factory UpdateNodeProgress.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      UpdateNodeProgress()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'UpdateNodeProgress',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: UpdateNodeProgress.$_createMessage)
    ..aE<UpdatePhase>(1, _omitFieldNames ? '' : 'phase',
        enumValues: UpdatePhase.values)
    ..a<$fixnum.Int64>(
        2, _omitFieldNames ? '' : 'sentBytes', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..a<$fixnum.Int64>(
        3, _omitFieldNames ? '' : 'totalBytes', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..aOS(4, _omitFieldNames ? '' : 'version')
    ..aOS(5, _omitFieldNames ? '' : 'previousVersion')
    ..aOS(6, _omitFieldNames ? '' : 'restart')
    ..aOS(7, _omitFieldNames ? '' : 'message')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  UpdateNodeProgress clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  UpdateNodeProgress copyWith(void Function(UpdateNodeProgress) updates) =>
      super.copyWith((message) => updates(message as UpdateNodeProgress))
          as UpdateNodeProgress;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use UpdateNodeProgress() / UpdateNodeProgress.new instead')
  static UpdateNodeProgress create() => UpdateNodeProgress._();
  static $pb.GeneratedMessage $_createMessage() => UpdateNodeProgress._();
  @$core.override
  UpdateNodeProgress createEmptyInstance() => UpdateNodeProgress._();
  @$core.pragma('dart2js:noInline')
  static UpdateNodeProgress getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<UpdateNodeProgress>(
          UpdateNodeProgress.$_createMessage);
  static UpdateNodeProgress? _defaultInstance;

  @$pb.TagNumber(1)
  UpdatePhase get phase => $_getN(0);
  @$pb.TagNumber(1)
  set phase(UpdatePhase value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasPhase() => $_has(0);
  @$pb.TagNumber(1)
  void clearPhase() => $_clearField(1);

  /// sent_bytes and total_bytes are uncompressed bytes of the binary; set from
  /// SENDING on.
  @$pb.TagNumber(2)
  $fixnum.Int64 get sentBytes => $_getI64(1);
  @$pb.TagNumber(2)
  set sentBytes($fixnum.Int64 value) => $_setInt64(1, value);
  @$pb.TagNumber(2)
  $core.bool hasSentBytes() => $_has(1);
  @$pb.TagNumber(2)
  void clearSentBytes() => $_clearField(2);

  @$pb.TagNumber(3)
  $fixnum.Int64 get totalBytes => $_getI64(2);
  @$pb.TagNumber(3)
  set totalBytes($fixnum.Int64 value) => $_setInt64(2, value);
  @$pb.TagNumber(3)
  $core.bool hasTotalBytes() => $_has(2);
  @$pb.TagNumber(3)
  void clearTotalBytes() => $_clearField(3);

  /// version is what the new binary reports, previous_version what the node
  /// ran before; set from SWAPPING on (SKIPPED carries the node's version in
  /// both).
  @$pb.TagNumber(4)
  $core.String get version => $_getSZ(3);
  @$pb.TagNumber(4)
  set version($core.String value) => $_setString(3, value);
  @$pb.TagNumber(4)
  $core.bool hasVersion() => $_has(3);
  @$pb.TagNumber(4)
  void clearVersion() => $_clearField(4);

  @$pb.TagNumber(5)
  $core.String get previousVersion => $_getSZ(4);
  @$pb.TagNumber(5)
  set previousVersion($core.String value) => $_setString(4, value);
  @$pb.TagNumber(5)
  $core.bool hasPreviousVersion() => $_has(4);
  @$pb.TagNumber(5)
  void clearPreviousVersion() => $_clearField(5);

  /// restart is how the node restarts, as in ApplyUpdateResponse.
  @$pb.TagNumber(6)
  $core.String get restart => $_getSZ(5);
  @$pb.TagNumber(6)
  set restart($core.String value) => $_setString(5, value);
  @$pb.TagNumber(6)
  $core.bool hasRestart() => $_has(5);
  @$pb.TagNumber(6)
  void clearRestart() => $_clearField(6);

  /// message is a human line about the step.
  @$pb.TagNumber(7)
  $core.String get message => $_getSZ(6);
  @$pb.TagNumber(7)
  set message($core.String value) => $_setString(6, value);
  @$pb.TagNumber(7)
  $core.bool hasMessage() => $_has(6);
  @$pb.TagNumber(7)
  void clearMessage() => $_clearField(7);
}

class ApplyUpdateRequest extends $pb.GeneratedMessage {
  factory ApplyUpdateRequest({
    $core.String? name,
    $core.String? path,
    $fixnum.Int64? size,
    $core.String? sha256,
    $core.String? expectedVersion,
  }) {
    final result = ApplyUpdateRequest._();
    if (name != null) result.name = name;
    if (path != null) result.path = path;
    if (size != null) result.size = size;
    if (sha256 != null) result.sha256 = sha256;
    if (expectedVersion != null) result.expectedVersion = expectedVersion;
    return result;
  }

  ApplyUpdateRequest._();

  factory ApplyUpdateRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ApplyUpdateRequest()..mergeFromBuffer(data, registry);
  factory ApplyUpdateRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ApplyUpdateRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ApplyUpdateRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ApplyUpdateRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'name')
    ..aOS(3, _omitFieldNames ? '' : 'path')
    ..a<$fixnum.Int64>(4, _omitFieldNames ? '' : 'size', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..aOS(5, _omitFieldNames ? '' : 'sha256')
    ..aOS(6, _omitFieldNames ? '' : 'expectedVersion')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ApplyUpdateRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ApplyUpdateRequest copyWith(void Function(ApplyUpdateRequest) updates) =>
      super.copyWith((message) => updates(message as ApplyUpdateRequest))
          as ApplyUpdateRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use ApplyUpdateRequest() / ApplyUpdateRequest.new instead')
  static ApplyUpdateRequest create() => ApplyUpdateRequest._();
  static $pb.GeneratedMessage $_createMessage() => ApplyUpdateRequest._();
  @$core.override
  ApplyUpdateRequest createEmptyInstance() => ApplyUpdateRequest._();
  @$core.pragma('dart2js:noInline')
  static ApplyUpdateRequest getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<ApplyUpdateRequest>(
          ApplyUpdateRequest.$_createMessage);
  static ApplyUpdateRequest? _defaultInstance;

  /// name is the node name; the agent ignores it.
  @$pb.TagNumber(1)
  $core.String get name => $_getSZ(0);
  @$pb.TagNumber(1)
  set name($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasName() => $_has(0);
  @$pb.TagNumber(1)
  void clearName() => $_clearField(1);

  /// path is where PutFile put the new binary, as PutFile reported it.
  @$pb.TagNumber(3)
  $core.String get path => $_getSZ(1);
  @$pb.TagNumber(3)
  set path($core.String value) => $_setString(1, value);
  @$pb.TagNumber(3)
  $core.bool hasPath() => $_has(1);
  @$pb.TagNumber(3)
  void clearPath() => $_clearField(3);

  /// size and sha256 (hex) are what the node must find at path.
  @$pb.TagNumber(4)
  $fixnum.Int64 get size => $_getI64(2);
  @$pb.TagNumber(4)
  set size($fixnum.Int64 value) => $_setInt64(2, value);
  @$pb.TagNumber(4)
  $core.bool hasSize() => $_has(2);
  @$pb.TagNumber(4)
  void clearSize() => $_clearField(4);

  @$pb.TagNumber(5)
  $core.String get sha256 => $_getSZ(3);
  @$pb.TagNumber(5)
  set sha256($core.String value) => $_setString(3, value);
  @$pb.TagNumber(5)
  $core.bool hasSha256() => $_has(3);
  @$pb.TagNumber(5)
  void clearSha256() => $_clearField(5);

  /// When set, the candidate must report this version before any binary swap.
  @$pb.TagNumber(6)
  $core.String get expectedVersion => $_getSZ(4);
  @$pb.TagNumber(6)
  set expectedVersion($core.String value) => $_setString(4, value);
  @$pb.TagNumber(6)
  $core.bool hasExpectedVersion() => $_has(4);
  @$pb.TagNumber(6)
  void clearExpectedVersion() => $_clearField(6);
}

class ApplyUpdateResponse extends $pb.GeneratedMessage {
  factory ApplyUpdateResponse({
    $core.String? version,
    $core.String? previousVersion,
    $core.String? restart,
  }) {
    final result = ApplyUpdateResponse._();
    if (version != null) result.version = version;
    if (previousVersion != null) result.previousVersion = previousVersion;
    if (restart != null) result.restart = restart;
    return result;
  }

  ApplyUpdateResponse._();

  factory ApplyUpdateResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ApplyUpdateResponse()..mergeFromBuffer(data, registry);
  factory ApplyUpdateResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ApplyUpdateResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ApplyUpdateResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ApplyUpdateResponse.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'version')
    ..aOS(2, _omitFieldNames ? '' : 'previousVersion')
    ..aOS(3, _omitFieldNames ? '' : 'restart')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ApplyUpdateResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ApplyUpdateResponse copyWith(void Function(ApplyUpdateResponse) updates) =>
      super.copyWith((message) => updates(message as ApplyUpdateResponse))
          as ApplyUpdateResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core
      .Deprecated('Use ApplyUpdateResponse() / ApplyUpdateResponse.new instead')
  static ApplyUpdateResponse create() => ApplyUpdateResponse._();
  static $pb.GeneratedMessage $_createMessage() => ApplyUpdateResponse._();
  @$core.override
  ApplyUpdateResponse createEmptyInstance() => ApplyUpdateResponse._();
  @$core.pragma('dart2js:noInline')
  static ApplyUpdateResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<ApplyUpdateResponse>(
          ApplyUpdateResponse.$_createMessage);
  static ApplyUpdateResponse? _defaultInstance;

  /// version is what the new binary reports; the node's agent_version becomes
  /// this once it has restarted.
  @$pb.TagNumber(1)
  $core.String get version => $_getSZ(0);
  @$pb.TagNumber(1)
  set version($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasVersion() => $_has(0);
  @$pb.TagNumber(1)
  void clearVersion() => $_clearField(1);

  /// previous_version is what the node ran before.
  @$pb.TagNumber(2)
  $core.String get previousVersion => $_getSZ(1);
  @$pb.TagNumber(2)
  set previousVersion($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasPreviousVersion() => $_has(1);
  @$pb.TagNumber(2)
  void clearPreviousVersion() => $_clearField(2);

  /// restart is how the node restarts: "systemd", "task", "termux", "exit"
  /// (a supervisor restarts it) or "re-exec".
  @$pb.TagNumber(3)
  $core.String get restart => $_getSZ(2);
  @$pb.TagNumber(3)
  set restart($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasRestart() => $_has(2);
  @$pb.TagNumber(3)
  void clearRestart() => $_clearField(3);
}

/// FileChunk is a slice of the uncompressed stream, zstd-compressed only when
/// that made it smaller.
class FileChunk extends $pb.GeneratedMessage {
  factory FileChunk({
    $core.List<$core.int>? data,
    $core.bool? compressed,
    $core.int? rawSize,
  }) {
    final result = FileChunk._();
    if (data != null) result.data = data;
    if (compressed != null) result.compressed = compressed;
    if (rawSize != null) result.rawSize = rawSize;
    return result;
  }

  FileChunk._();

  factory FileChunk.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      FileChunk()..mergeFromBuffer(data, registry);
  factory FileChunk.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      FileChunk()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'FileChunk',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: FileChunk.$_createMessage)
    ..a<$core.List<$core.int>>(
        1, _omitFieldNames ? '' : 'data', $pb.PbFieldType.OY)
    ..aOB(2, _omitFieldNames ? '' : 'compressed')
    ..aI(3, _omitFieldNames ? '' : 'rawSize', fieldType: $pb.PbFieldType.OU3)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  FileChunk clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  FileChunk copyWith(void Function(FileChunk) updates) =>
      super.copyWith((message) => updates(message as FileChunk)) as FileChunk;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use FileChunk() / FileChunk.new instead')
  static FileChunk create() => FileChunk._();
  static $pb.GeneratedMessage $_createMessage() => FileChunk._();
  @$core.override
  FileChunk createEmptyInstance() => FileChunk._();
  @$core.pragma('dart2js:noInline')
  static FileChunk getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<FileChunk>(FileChunk.$_createMessage);
  static FileChunk? _defaultInstance;

  @$pb.TagNumber(1)
  $core.List<$core.int> get data => $_getN(0);
  @$pb.TagNumber(1)
  set data($core.List<$core.int> value) => $_setBytes(0, value);
  @$pb.TagNumber(1)
  $core.bool hasData() => $_has(0);
  @$pb.TagNumber(1)
  void clearData() => $_clearField(1);

  @$pb.TagNumber(2)
  $core.bool get compressed => $_getBF(1);
  @$pb.TagNumber(2)
  set compressed($core.bool value) => $_setBool(1, value);
  @$pb.TagNumber(2)
  $core.bool hasCompressed() => $_has(1);
  @$pb.TagNumber(2)
  void clearCompressed() => $_clearField(2);

  /// raw_size is the length after decompression.
  @$pb.TagNumber(3)
  $core.int get rawSize => $_getIZ(2);
  @$pb.TagNumber(3)
  set rawSize($core.int value) => $_setUnsignedInt32(2, value);
  @$pb.TagNumber(3)
  $core.bool hasRawSize() => $_has(2);
  @$pb.TagNumber(3)
  void clearRawSize() => $_clearField(3);
}

class FileTrailer extends $pb.GeneratedMessage {
  factory FileTrailer({
    $fixnum.Int64? size,
    $core.String? sha256,
    $core.int? skipped,
  }) {
    final result = FileTrailer._();
    if (size != null) result.size = size;
    if (sha256 != null) result.sha256 = sha256;
    if (skipped != null) result.skipped = skipped;
    return result;
  }

  FileTrailer._();

  factory FileTrailer.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      FileTrailer()..mergeFromBuffer(data, registry);
  factory FileTrailer.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      FileTrailer()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'FileTrailer',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: FileTrailer.$_createMessage)
    ..a<$fixnum.Int64>(1, _omitFieldNames ? '' : 'size', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..aOS(2, _omitFieldNames ? '' : 'sha256')
    ..aI(3, _omitFieldNames ? '' : 'skipped', fieldType: $pb.PbFieldType.OU3)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  FileTrailer clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  FileTrailer copyWith(void Function(FileTrailer) updates) =>
      super.copyWith((message) => updates(message as FileTrailer))
          as FileTrailer;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use FileTrailer() / FileTrailer.new instead')
  static FileTrailer create() => FileTrailer._();
  static $pb.GeneratedMessage $_createMessage() => FileTrailer._();
  @$core.override
  FileTrailer createEmptyInstance() => FileTrailer._();
  @$core.pragma('dart2js:noInline')
  static FileTrailer getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<FileTrailer>(
          FileTrailer.$_createMessage);
  static FileTrailer? _defaultInstance;

  /// size and sha256 (hex) cover the uncompressed stream.
  @$pb.TagNumber(1)
  $fixnum.Int64 get size => $_getI64(0);
  @$pb.TagNumber(1)
  set size($fixnum.Int64 value) => $_setInt64(0, value);
  @$pb.TagNumber(1)
  $core.bool hasSize() => $_has(0);
  @$pb.TagNumber(1)
  void clearSize() => $_clearField(1);

  @$pb.TagNumber(2)
  $core.String get sha256 => $_getSZ(1);
  @$pb.TagNumber(2)
  set sha256($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasSha256() => $_has(1);
  @$pb.TagNumber(2)
  void clearSha256() => $_clearField(2);

  /// skipped counts directory entries that were not sent (symlinks, devices).
  @$pb.TagNumber(3)
  $core.int get skipped => $_getIZ(2);
  @$pb.TagNumber(3)
  set skipped($core.int value) => $_setUnsignedInt32(2, value);
  @$pb.TagNumber(3)
  $core.bool hasSkipped() => $_has(2);
  @$pb.TagNumber(3)
  void clearSkipped() => $_clearField(3);
}

class PutFileRequest extends $pb.GeneratedMessage {
  factory PutFileRequest({
    $core.String? name,
    $core.String? transferId,
    $core.String? path,
    FileKind? kind,
    $core.int? mode,
    $core.bool? overwrite,
    $fixnum.Int64? offset,
    FileChunk? chunk,
    $core.bool? last,
    FileTrailer? trailer,
  }) {
    final result = PutFileRequest._();
    if (name != null) result.name = name;
    if (transferId != null) result.transferId = transferId;
    if (path != null) result.path = path;
    if (kind != null) result.kind = kind;
    if (mode != null) result.mode = mode;
    if (overwrite != null) result.overwrite = overwrite;
    if (offset != null) result.offset = offset;
    if (chunk != null) result.chunk = chunk;
    if (last != null) result.last = last;
    if (trailer != null) result.trailer = trailer;
    return result;
  }

  PutFileRequest._();

  factory PutFileRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      PutFileRequest()..mergeFromBuffer(data, registry);
  factory PutFileRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      PutFileRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'PutFileRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: PutFileRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'name')
    ..aOS(3, _omitFieldNames ? '' : 'transferId')
    ..aOS(4, _omitFieldNames ? '' : 'path')
    ..aE<FileKind>(5, _omitFieldNames ? '' : 'kind',
        enumValues: FileKind.values)
    ..aI(6, _omitFieldNames ? '' : 'mode', fieldType: $pb.PbFieldType.OU3)
    ..aOB(7, _omitFieldNames ? '' : 'overwrite')
    ..a<$fixnum.Int64>(8, _omitFieldNames ? '' : 'offset', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..aOM<FileChunk>(9, _omitFieldNames ? '' : 'chunk',
        subBuilder: FileChunk.$_createMessage)
    ..aOB(10, _omitFieldNames ? '' : 'last')
    ..aOM<FileTrailer>(11, _omitFieldNames ? '' : 'trailer',
        subBuilder: FileTrailer.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  PutFileRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  PutFileRequest copyWith(void Function(PutFileRequest) updates) =>
      super.copyWith((message) => updates(message as PutFileRequest))
          as PutFileRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use PutFileRequest() / PutFileRequest.new instead')
  static PutFileRequest create() => PutFileRequest._();
  static $pb.GeneratedMessage $_createMessage() => PutFileRequest._();
  @$core.override
  PutFileRequest createEmptyInstance() => PutFileRequest._();
  @$core.pragma('dart2js:noInline')
  static PutFileRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<PutFileRequest>(
          PutFileRequest.$_createMessage);
  static PutFileRequest? _defaultInstance;

  /// name is the node name.
  @$pb.TagNumber(1)
  $core.String get name => $_getSZ(0);
  @$pb.TagNumber(1)
  set name($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasName() => $_has(0);
  @$pb.TagNumber(1)
  void clearName() => $_clearField(1);

  /// transfer_id is chosen by the caller: 8 to 80 characters of [A-Za-z0-9._-].
  @$pb.TagNumber(3)
  $core.String get transferId => $_getSZ(1);
  @$pb.TagNumber(3)
  set transferId($core.String value) => $_setString(1, value);
  @$pb.TagNumber(3)
  $core.bool hasTransferId() => $_has(1);
  @$pb.TagNumber(3)
  void clearTransferId() => $_clearField(3);

  /// path, kind, mode and overwrite are read on the first call (offset 0).
  /// A relative path is under the node's file root; the path names the
  /// destination itself, not a directory to put it in.
  @$pb.TagNumber(4)
  $core.String get path => $_getSZ(2);
  @$pb.TagNumber(4)
  set path($core.String value) => $_setString(2, value);
  @$pb.TagNumber(4)
  $core.bool hasPath() => $_has(2);
  @$pb.TagNumber(4)
  void clearPath() => $_clearField(4);

  @$pb.TagNumber(5)
  FileKind get kind => $_getN(3);
  @$pb.TagNumber(5)
  set kind(FileKind value) => $_setField(5, value);
  @$pb.TagNumber(5)
  $core.bool hasKind() => $_has(3);
  @$pb.TagNumber(5)
  void clearKind() => $_clearField(5);

  /// mode holds the permission bits of a file; ignored for directories.
  @$pb.TagNumber(6)
  $core.int get mode => $_getIZ(4);
  @$pb.TagNumber(6)
  set mode($core.int value) => $_setUnsignedInt32(4, value);
  @$pb.TagNumber(6)
  $core.bool hasMode() => $_has(4);
  @$pb.TagNumber(6)
  void clearMode() => $_clearField(6);

  /// overwrite replaces an existing destination instead of failing with
  /// ALREADY_EXISTS.
  @$pb.TagNumber(7)
  $core.bool get overwrite => $_getBF(5);
  @$pb.TagNumber(7)
  set overwrite($core.bool value) => $_setBool(5, value);
  @$pb.TagNumber(7)
  $core.bool hasOverwrite() => $_has(5);
  @$pb.TagNumber(7)
  void clearOverwrite() => $_clearField(7);

  /// offset is the number of uncompressed bytes sent in earlier calls.
  @$pb.TagNumber(8)
  $fixnum.Int64 get offset => $_getI64(6);
  @$pb.TagNumber(8)
  set offset($fixnum.Int64 value) => $_setInt64(6, value);
  @$pb.TagNumber(8)
  $core.bool hasOffset() => $_has(6);
  @$pb.TagNumber(8)
  void clearOffset() => $_clearField(8);

  @$pb.TagNumber(9)
  FileChunk get chunk => $_getN(7);
  @$pb.TagNumber(9)
  set chunk(FileChunk value) => $_setField(9, value);
  @$pb.TagNumber(9)
  $core.bool hasChunk() => $_has(7);
  @$pb.TagNumber(9)
  void clearChunk() => $_clearField(9);
  @$pb.TagNumber(9)
  FileChunk ensureChunk() => $_ensure(7);

  /// last ends the transfer; trailer is then required.
  @$pb.TagNumber(10)
  $core.bool get last => $_getBF(8);
  @$pb.TagNumber(10)
  set last($core.bool value) => $_setBool(8, value);
  @$pb.TagNumber(10)
  $core.bool hasLast() => $_has(8);
  @$pb.TagNumber(10)
  void clearLast() => $_clearField(10);

  @$pb.TagNumber(11)
  FileTrailer get trailer => $_getN(9);
  @$pb.TagNumber(11)
  set trailer(FileTrailer value) => $_setField(11, value);
  @$pb.TagNumber(11)
  $core.bool hasTrailer() => $_has(9);
  @$pb.TagNumber(11)
  void clearTrailer() => $_clearField(11);
  @$pb.TagNumber(11)
  FileTrailer ensureTrailer() => $_ensure(9);
}

class PutFileResponse extends $pb.GeneratedMessage {
  factory PutFileResponse({
    $core.String? path,
    $fixnum.Int64? received,
    $core.bool? done,
  }) {
    final result = PutFileResponse._();
    if (path != null) result.path = path;
    if (received != null) result.received = received;
    if (done != null) result.done = done;
    return result;
  }

  PutFileResponse._();

  factory PutFileResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      PutFileResponse()..mergeFromBuffer(data, registry);
  factory PutFileResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      PutFileResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'PutFileResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: PutFileResponse.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'path')
    ..a<$fixnum.Int64>(
        2, _omitFieldNames ? '' : 'received', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..aOB(3, _omitFieldNames ? '' : 'done')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  PutFileResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  PutFileResponse copyWith(void Function(PutFileResponse) updates) =>
      super.copyWith((message) => updates(message as PutFileResponse))
          as PutFileResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use PutFileResponse() / PutFileResponse.new instead')
  static PutFileResponse create() => PutFileResponse._();
  static $pb.GeneratedMessage $_createMessage() => PutFileResponse._();
  @$core.override
  PutFileResponse createEmptyInstance() => PutFileResponse._();
  @$core.pragma('dart2js:noInline')
  static PutFileResponse getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<PutFileResponse>(
          PutFileResponse.$_createMessage);
  static PutFileResponse? _defaultInstance;

  /// path is the resolved destination, set once the transfer is complete.
  @$pb.TagNumber(1)
  $core.String get path => $_getSZ(0);
  @$pb.TagNumber(1)
  set path($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasPath() => $_has(0);
  @$pb.TagNumber(1)
  void clearPath() => $_clearField(1);

  @$pb.TagNumber(2)
  $fixnum.Int64 get received => $_getI64(1);
  @$pb.TagNumber(2)
  set received($fixnum.Int64 value) => $_setInt64(1, value);
  @$pb.TagNumber(2)
  $core.bool hasReceived() => $_has(1);
  @$pb.TagNumber(2)
  void clearReceived() => $_clearField(2);

  @$pb.TagNumber(3)
  $core.bool get done => $_getBF(2);
  @$pb.TagNumber(3)
  set done($core.bool value) => $_setBool(2, value);
  @$pb.TagNumber(3)
  $core.bool hasDone() => $_has(2);
  @$pb.TagNumber(3)
  void clearDone() => $_clearField(3);
}

class GetFileRequest extends $pb.GeneratedMessage {
  factory GetFileRequest({
    $core.String? name,
    $core.String? path,
    $core.bool? noCompress,
  }) {
    final result = GetFileRequest._();
    if (name != null) result.name = name;
    if (path != null) result.path = path;
    if (noCompress != null) result.noCompress = noCompress;
    return result;
  }

  GetFileRequest._();

  factory GetFileRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      GetFileRequest()..mergeFromBuffer(data, registry);
  factory GetFileRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      GetFileRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'GetFileRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: GetFileRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'name')
    ..aOS(3, _omitFieldNames ? '' : 'path')
    ..aOB(4, _omitFieldNames ? '' : 'noCompress')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  GetFileRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  GetFileRequest copyWith(void Function(GetFileRequest) updates) =>
      super.copyWith((message) => updates(message as GetFileRequest))
          as GetFileRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use GetFileRequest() / GetFileRequest.new instead')
  static GetFileRequest create() => GetFileRequest._();
  static $pb.GeneratedMessage $_createMessage() => GetFileRequest._();
  @$core.override
  GetFileRequest createEmptyInstance() => GetFileRequest._();
  @$core.pragma('dart2js:noInline')
  static GetFileRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<GetFileRequest>(
          GetFileRequest.$_createMessage);
  static GetFileRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get name => $_getSZ(0);
  @$pb.TagNumber(1)
  set name($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasName() => $_has(0);
  @$pb.TagNumber(1)
  void clearName() => $_clearField(1);

  @$pb.TagNumber(3)
  $core.String get path => $_getSZ(1);
  @$pb.TagNumber(3)
  set path($core.String value) => $_setString(1, value);
  @$pb.TagNumber(3)
  $core.bool hasPath() => $_has(1);
  @$pb.TagNumber(3)
  void clearPath() => $_clearField(3);

  /// no_compress sends every chunk as is.
  @$pb.TagNumber(4)
  $core.bool get noCompress => $_getBF(2);
  @$pb.TagNumber(4)
  set noCompress($core.bool value) => $_setBool(2, value);
  @$pb.TagNumber(4)
  $core.bool hasNoCompress() => $_has(2);
  @$pb.TagNumber(4)
  void clearNoCompress() => $_clearField(4);
}

class FileHeader extends $pb.GeneratedMessage {
  factory FileHeader({
    FileKind? kind,
    $core.int? mode,
    $fixnum.Int64? size,
    $core.String? path,
  }) {
    final result = FileHeader._();
    if (kind != null) result.kind = kind;
    if (mode != null) result.mode = mode;
    if (size != null) result.size = size;
    if (path != null) result.path = path;
    return result;
  }

  FileHeader._();

  factory FileHeader.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      FileHeader()..mergeFromBuffer(data, registry);
  factory FileHeader.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      FileHeader()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'FileHeader',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: FileHeader.$_createMessage)
    ..aE<FileKind>(1, _omitFieldNames ? '' : 'kind',
        enumValues: FileKind.values)
    ..aI(2, _omitFieldNames ? '' : 'mode', fieldType: $pb.PbFieldType.OU3)
    ..a<$fixnum.Int64>(3, _omitFieldNames ? '' : 'size', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..aOS(4, _omitFieldNames ? '' : 'path')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  FileHeader clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  FileHeader copyWith(void Function(FileHeader) updates) =>
      super.copyWith((message) => updates(message as FileHeader)) as FileHeader;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use FileHeader() / FileHeader.new instead')
  static FileHeader create() => FileHeader._();
  static $pb.GeneratedMessage $_createMessage() => FileHeader._();
  @$core.override
  FileHeader createEmptyInstance() => FileHeader._();
  @$core.pragma('dart2js:noInline')
  static FileHeader getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<FileHeader>(FileHeader.$_createMessage);
  static FileHeader? _defaultInstance;

  @$pb.TagNumber(1)
  FileKind get kind => $_getN(0);
  @$pb.TagNumber(1)
  set kind(FileKind value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasKind() => $_has(0);
  @$pb.TagNumber(1)
  void clearKind() => $_clearField(1);

  @$pb.TagNumber(2)
  $core.int get mode => $_getIZ(1);
  @$pb.TagNumber(2)
  set mode($core.int value) => $_setUnsignedInt32(1, value);
  @$pb.TagNumber(2)
  $core.bool hasMode() => $_has(1);
  @$pb.TagNumber(2)
  void clearMode() => $_clearField(2);

  /// size is the file size; 0 for directories, whose size is known at the end.
  @$pb.TagNumber(3)
  $fixnum.Int64 get size => $_getI64(2);
  @$pb.TagNumber(3)
  set size($fixnum.Int64 value) => $_setInt64(2, value);
  @$pb.TagNumber(3)
  $core.bool hasSize() => $_has(2);
  @$pb.TagNumber(3)
  void clearSize() => $_clearField(3);

  @$pb.TagNumber(4)
  $core.String get path => $_getSZ(3);
  @$pb.TagNumber(4)
  set path($core.String value) => $_setString(3, value);
  @$pb.TagNumber(4)
  $core.bool hasPath() => $_has(3);
  @$pb.TagNumber(4)
  void clearPath() => $_clearField(4);
}

enum GetFileResponse_Part { header, chunk, trailer, notSet }

class GetFileResponse extends $pb.GeneratedMessage {
  factory GetFileResponse({
    FileHeader? header,
    FileChunk? chunk,
    FileTrailer? trailer,
  }) {
    final result = GetFileResponse._();
    if (header != null) result.header = header;
    if (chunk != null) result.chunk = chunk;
    if (trailer != null) result.trailer = trailer;
    return result;
  }

  GetFileResponse._();

  factory GetFileResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      GetFileResponse()..mergeFromBuffer(data, registry);
  factory GetFileResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      GetFileResponse()..mergeFromJson(json, registry);

  static const $core.Map<$core.int, GetFileResponse_Part>
      _GetFileResponse_PartByTag = {
    1: GetFileResponse_Part.header,
    2: GetFileResponse_Part.chunk,
    3: GetFileResponse_Part.trailer,
    0: GetFileResponse_Part.notSet
  };
  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'GetFileResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: GetFileResponse.$_createMessage)
    ..oo(0, [1, 2, 3])
    ..aOM<FileHeader>(1, _omitFieldNames ? '' : 'header',
        subBuilder: FileHeader.$_createMessage)
    ..aOM<FileChunk>(2, _omitFieldNames ? '' : 'chunk',
        subBuilder: FileChunk.$_createMessage)
    ..aOM<FileTrailer>(3, _omitFieldNames ? '' : 'trailer',
        subBuilder: FileTrailer.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  GetFileResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  GetFileResponse copyWith(void Function(GetFileResponse) updates) =>
      super.copyWith((message) => updates(message as GetFileResponse))
          as GetFileResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use GetFileResponse() / GetFileResponse.new instead')
  static GetFileResponse create() => GetFileResponse._();
  static $pb.GeneratedMessage $_createMessage() => GetFileResponse._();
  @$core.override
  GetFileResponse createEmptyInstance() => GetFileResponse._();
  @$core.pragma('dart2js:noInline')
  static GetFileResponse getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<GetFileResponse>(
          GetFileResponse.$_createMessage);
  static GetFileResponse? _defaultInstance;

  @$pb.TagNumber(1)
  @$pb.TagNumber(2)
  @$pb.TagNumber(3)
  GetFileResponse_Part whichPart() =>
      _GetFileResponse_PartByTag[$_whichOneof(0)]!;
  @$pb.TagNumber(1)
  @$pb.TagNumber(2)
  @$pb.TagNumber(3)
  void clearPart() => $_clearField($_whichOneof(0));

  @$pb.TagNumber(1)
  FileHeader get header => $_getN(0);
  @$pb.TagNumber(1)
  set header(FileHeader value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasHeader() => $_has(0);
  @$pb.TagNumber(1)
  void clearHeader() => $_clearField(1);
  @$pb.TagNumber(1)
  FileHeader ensureHeader() => $_ensure(0);

  @$pb.TagNumber(2)
  FileChunk get chunk => $_getN(1);
  @$pb.TagNumber(2)
  set chunk(FileChunk value) => $_setField(2, value);
  @$pb.TagNumber(2)
  $core.bool hasChunk() => $_has(1);
  @$pb.TagNumber(2)
  void clearChunk() => $_clearField(2);
  @$pb.TagNumber(2)
  FileChunk ensureChunk() => $_ensure(1);

  @$pb.TagNumber(3)
  FileTrailer get trailer => $_getN(2);
  @$pb.TagNumber(3)
  set trailer(FileTrailer value) => $_setField(3, value);
  @$pb.TagNumber(3)
  $core.bool hasTrailer() => $_has(2);
  @$pb.TagNumber(3)
  void clearTrailer() => $_clearField(3);
  @$pb.TagNumber(3)
  FileTrailer ensureTrailer() => $_ensure(2);
}

class GetMetricsRequest extends $pb.GeneratedMessage {
  factory GetMetricsRequest() => GetMetricsRequest._();

  GetMetricsRequest._();

  factory GetMetricsRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      GetMetricsRequest()..mergeFromBuffer(data, registry);
  factory GetMetricsRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      GetMetricsRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'GetMetricsRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: GetMetricsRequest.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  GetMetricsRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  GetMetricsRequest copyWith(void Function(GetMetricsRequest) updates) =>
      super.copyWith((message) => updates(message as GetMetricsRequest))
          as GetMetricsRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use GetMetricsRequest() / GetMetricsRequest.new instead')
  static GetMetricsRequest create() => GetMetricsRequest._();
  static $pb.GeneratedMessage $_createMessage() => GetMetricsRequest._();
  @$core.override
  GetMetricsRequest createEmptyInstance() => GetMetricsRequest._();
  @$core.pragma('dart2js:noInline')
  static GetMetricsRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<GetMetricsRequest>(
          GetMetricsRequest.$_createMessage);
  static GetMetricsRequest? _defaultInstance;
}

class WatchMetricsRequest extends $pb.GeneratedMessage {
  factory WatchMetricsRequest({
    $core.int? intervalSeconds,
  }) {
    final result = WatchMetricsRequest._();
    if (intervalSeconds != null) result.intervalSeconds = intervalSeconds;
    return result;
  }

  WatchMetricsRequest._();

  factory WatchMetricsRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      WatchMetricsRequest()..mergeFromBuffer(data, registry);
  factory WatchMetricsRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      WatchMetricsRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'WatchMetricsRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: WatchMetricsRequest.$_createMessage)
    ..aI(1, _omitFieldNames ? '' : 'intervalSeconds')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  WatchMetricsRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  WatchMetricsRequest copyWith(void Function(WatchMetricsRequest) updates) =>
      super.copyWith((message) => updates(message as WatchMetricsRequest))
          as WatchMetricsRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core
      .Deprecated('Use WatchMetricsRequest() / WatchMetricsRequest.new instead')
  static WatchMetricsRequest create() => WatchMetricsRequest._();
  static $pb.GeneratedMessage $_createMessage() => WatchMetricsRequest._();
  @$core.override
  WatchMetricsRequest createEmptyInstance() => WatchMetricsRequest._();
  @$core.pragma('dart2js:noInline')
  static WatchMetricsRequest getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<WatchMetricsRequest>(
          WatchMetricsRequest.$_createMessage);
  static WatchMetricsRequest? _defaultInstance;

  /// interval_seconds is clamped to a minimum of 5; 0 selects 10.
  @$pb.TagNumber(1)
  $core.int get intervalSeconds => $_getIZ(0);
  @$pb.TagNumber(1)
  set intervalSeconds($core.int value) => $_setSignedInt32(0, value);
  @$pb.TagNumber(1)
  $core.bool hasIntervalSeconds() => $_has(0);
  @$pb.TagNumber(1)
  void clearIntervalSeconds() => $_clearField(1);
}

class NodeMetrics extends $pb.GeneratedMessage {
  factory NodeMetrics({
    $core.String? hostname,
    $core.String? os,
    $core.String? arch,
    $core.String? platform,
    $core.String? agentVersion,
    $fixnum.Int64? uptimeSeconds,
    $0.Timestamp? collectedAt,
    CpuMetrics? cpu,
    MemoryMetrics? memory,
    $core.Iterable<FilesystemMetrics>? filesystems,
    $core.Iterable<GpuMetrics>? gpus,
    ContainerEngine? engine,
    $core.Iterable<DirSize>? watchDirs,
    $core.Iterable<$core.String>? warnings,
    $core.Iterable<$1.Sleeve>? sleeves,
    $core.Iterable<AcceleratorMetrics>? npus,
    $2.JobIsolation? jobs,
    NodeUpdateState? update,
  }) {
    final result = NodeMetrics._();
    if (hostname != null) result.hostname = hostname;
    if (os != null) result.os = os;
    if (arch != null) result.arch = arch;
    if (platform != null) result.platform = platform;
    if (agentVersion != null) result.agentVersion = agentVersion;
    if (uptimeSeconds != null) result.uptimeSeconds = uptimeSeconds;
    if (collectedAt != null) result.collectedAt = collectedAt;
    if (cpu != null) result.cpu = cpu;
    if (memory != null) result.memory = memory;
    if (filesystems != null) result.filesystems.addAll(filesystems);
    if (gpus != null) result.gpus.addAll(gpus);
    if (engine != null) result.engine = engine;
    if (watchDirs != null) result.watchDirs.addAll(watchDirs);
    if (warnings != null) result.warnings.addAll(warnings);
    if (sleeves != null) result.sleeves.addAll(sleeves);
    if (npus != null) result.npus.addAll(npus);
    if (jobs != null) result.jobs = jobs;
    if (update != null) result.update = update;
    return result;
  }

  NodeMetrics._();

  factory NodeMetrics.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      NodeMetrics()..mergeFromBuffer(data, registry);
  factory NodeMetrics.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      NodeMetrics()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'NodeMetrics',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: NodeMetrics.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'hostname')
    ..aOS(2, _omitFieldNames ? '' : 'os')
    ..aOS(3, _omitFieldNames ? '' : 'arch')
    ..aOS(4, _omitFieldNames ? '' : 'platform')
    ..aOS(5, _omitFieldNames ? '' : 'agentVersion')
    ..aInt64(6, _omitFieldNames ? '' : 'uptimeSeconds')
    ..aOM<$0.Timestamp>(7, _omitFieldNames ? '' : 'collectedAt',
        subBuilder: $0.Timestamp.$_createMessage)
    ..aOM<CpuMetrics>(8, _omitFieldNames ? '' : 'cpu',
        subBuilder: CpuMetrics.$_createMessage)
    ..aOM<MemoryMetrics>(9, _omitFieldNames ? '' : 'memory',
        subBuilder: MemoryMetrics.$_createMessage)
    ..pPM<FilesystemMetrics>(10, _omitFieldNames ? '' : 'filesystems',
        subBuilder: FilesystemMetrics.$_createMessage)
    ..pPM<GpuMetrics>(11, _omitFieldNames ? '' : 'gpus',
        subBuilder: GpuMetrics.$_createMessage)
    ..aOM<ContainerEngine>(12, _omitFieldNames ? '' : 'engine',
        subBuilder: ContainerEngine.$_createMessage)
    ..pPM<DirSize>(13, _omitFieldNames ? '' : 'watchDirs',
        subBuilder: DirSize.$_createMessage)
    ..pPS(14, _omitFieldNames ? '' : 'warnings')
    ..pPM<$1.Sleeve>(15, _omitFieldNames ? '' : 'sleeves',
        subBuilder: $1.Sleeve.$_createMessage)
    ..pPM<AcceleratorMetrics>(16, _omitFieldNames ? '' : 'npus',
        subBuilder: AcceleratorMetrics.$_createMessage)
    ..aOM<$2.JobIsolation>(17, _omitFieldNames ? '' : 'jobs',
        subBuilder: $2.JobIsolation.$_createMessage)
    ..aOM<NodeUpdateState>(18, _omitFieldNames ? '' : 'update',
        subBuilder: NodeUpdateState.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  NodeMetrics clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  NodeMetrics copyWith(void Function(NodeMetrics) updates) =>
      super.copyWith((message) => updates(message as NodeMetrics))
          as NodeMetrics;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use NodeMetrics() / NodeMetrics.new instead')
  static NodeMetrics create() => NodeMetrics._();
  static $pb.GeneratedMessage $_createMessage() => NodeMetrics._();
  @$core.override
  NodeMetrics createEmptyInstance() => NodeMetrics._();
  @$core.pragma('dart2js:noInline')
  static NodeMetrics getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<NodeMetrics>(
          NodeMetrics.$_createMessage);
  static NodeMetrics? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get hostname => $_getSZ(0);
  @$pb.TagNumber(1)
  set hostname($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasHostname() => $_has(0);
  @$pb.TagNumber(1)
  void clearHostname() => $_clearField(1);

  @$pb.TagNumber(2)
  $core.String get os => $_getSZ(1);
  @$pb.TagNumber(2)
  set os($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasOs() => $_has(1);
  @$pb.TagNumber(2)
  void clearOs() => $_clearField(2);

  @$pb.TagNumber(3)
  $core.String get arch => $_getSZ(2);
  @$pb.TagNumber(3)
  set arch($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasArch() => $_has(2);
  @$pb.TagNumber(3)
  void clearArch() => $_clearField(3);

  /// platform is a human description such as "ubuntu 24.04" or "Windows 11".
  @$pb.TagNumber(4)
  $core.String get platform => $_getSZ(3);
  @$pb.TagNumber(4)
  set platform($core.String value) => $_setString(3, value);
  @$pb.TagNumber(4)
  $core.bool hasPlatform() => $_has(3);
  @$pb.TagNumber(4)
  void clearPlatform() => $_clearField(4);

  @$pb.TagNumber(5)
  $core.String get agentVersion => $_getSZ(4);
  @$pb.TagNumber(5)
  set agentVersion($core.String value) => $_setString(4, value);
  @$pb.TagNumber(5)
  $core.bool hasAgentVersion() => $_has(4);
  @$pb.TagNumber(5)
  void clearAgentVersion() => $_clearField(5);

  @$pb.TagNumber(6)
  $fixnum.Int64 get uptimeSeconds => $_getI64(5);
  @$pb.TagNumber(6)
  set uptimeSeconds($fixnum.Int64 value) => $_setInt64(5, value);
  @$pb.TagNumber(6)
  $core.bool hasUptimeSeconds() => $_has(5);
  @$pb.TagNumber(6)
  void clearUptimeSeconds() => $_clearField(6);

  @$pb.TagNumber(7)
  $0.Timestamp get collectedAt => $_getN(6);
  @$pb.TagNumber(7)
  set collectedAt($0.Timestamp value) => $_setField(7, value);
  @$pb.TagNumber(7)
  $core.bool hasCollectedAt() => $_has(6);
  @$pb.TagNumber(7)
  void clearCollectedAt() => $_clearField(7);
  @$pb.TagNumber(7)
  $0.Timestamp ensureCollectedAt() => $_ensure(6);

  @$pb.TagNumber(8)
  CpuMetrics get cpu => $_getN(7);
  @$pb.TagNumber(8)
  set cpu(CpuMetrics value) => $_setField(8, value);
  @$pb.TagNumber(8)
  $core.bool hasCpu() => $_has(7);
  @$pb.TagNumber(8)
  void clearCpu() => $_clearField(8);
  @$pb.TagNumber(8)
  CpuMetrics ensureCpu() => $_ensure(7);

  @$pb.TagNumber(9)
  MemoryMetrics get memory => $_getN(8);
  @$pb.TagNumber(9)
  set memory(MemoryMetrics value) => $_setField(9, value);
  @$pb.TagNumber(9)
  $core.bool hasMemory() => $_has(8);
  @$pb.TagNumber(9)
  void clearMemory() => $_clearField(9);
  @$pb.TagNumber(9)
  MemoryMetrics ensureMemory() => $_ensure(8);

  @$pb.TagNumber(10)
  $pb.PbList<FilesystemMetrics> get filesystems => $_getList(9);

  /// gpus lists discrete and integrated GPUs: discrete first, then integrated.
  /// Virtual display adapters are left out.
  @$pb.TagNumber(11)
  $pb.PbList<GpuMetrics> get gpus => $_getList(10);

  /// engine is unset when no container engine is reachable.
  @$pb.TagNumber(12)
  ContainerEngine get engine => $_getN(11);
  @$pb.TagNumber(12)
  set engine(ContainerEngine value) => $_setField(12, value);
  @$pb.TagNumber(12)
  $core.bool hasEngine() => $_has(11);
  @$pb.TagNumber(12)
  void clearEngine() => $_clearField(12);
  @$pb.TagNumber(12)
  ContainerEngine ensureEngine() => $_ensure(11);

  @$pb.TagNumber(13)
  $pb.PbList<DirSize> get watchDirs => $_getList(12);

  /// warnings lists sections that could not be collected on this node.
  @$pb.TagNumber(14)
  $pb.PbList<$core.String> get warnings => $_getList(13);

  /// sleeves lists the sleeves running on this node with their usage; only the
  /// hub's own node has any.
  @$pb.TagNumber(15)
  $pb.PbList<$1.Sleeve> get sleeves => $_getList(14);

  /// npus lists neural processing units.
  @$pb.TagNumber(16)
  $pb.PbList<AcceleratorMetrics> get npus => $_getList(15);

  /// jobs is how this node isolates and elevates jobs; unset when it runs
  /// none.
  @$pb.TagNumber(17)
  $2.JobIsolation get jobs => $_getN(16);
  @$pb.TagNumber(17)
  set jobs($2.JobIsolation value) => $_setField(17, value);
  @$pb.TagNumber(17)
  $core.bool hasJobs() => $_has(16);
  @$pb.TagNumber(17)
  void clearJobs() => $_clearField(17);
  @$pb.TagNumber(17)
  $2.JobIsolation ensureJobs() => $_ensure(16);

  /// update is the persisted trial outcome for the last binary replacement.
  @$pb.TagNumber(18)
  NodeUpdateState get update => $_getN(17);
  @$pb.TagNumber(18)
  set update(NodeUpdateState value) => $_setField(18, value);
  @$pb.TagNumber(18)
  $core.bool hasUpdate() => $_has(17);
  @$pb.TagNumber(18)
  void clearUpdate() => $_clearField(18);
  @$pb.TagNumber(18)
  NodeUpdateState ensureUpdate() => $_ensure(17);
}

class NodeUpdateState extends $pb.GeneratedMessage {
  factory NodeUpdateState({
    $core.String? phase,
    $core.String? previousVersion,
    $core.String? version,
    $core.int? attempts,
  }) {
    final result = NodeUpdateState._();
    if (phase != null) result.phase = phase;
    if (previousVersion != null) result.previousVersion = previousVersion;
    if (version != null) result.version = version;
    if (attempts != null) result.attempts = attempts;
    return result;
  }

  NodeUpdateState._();

  factory NodeUpdateState.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      NodeUpdateState()..mergeFromBuffer(data, registry);
  factory NodeUpdateState.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      NodeUpdateState()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'NodeUpdateState',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: NodeUpdateState.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'phase')
    ..aOS(2, _omitFieldNames ? '' : 'previousVersion')
    ..aOS(3, _omitFieldNames ? '' : 'version')
    ..aI(4, _omitFieldNames ? '' : 'attempts', fieldType: $pb.PbFieldType.OU3)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  NodeUpdateState clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  NodeUpdateState copyWith(void Function(NodeUpdateState) updates) =>
      super.copyWith((message) => updates(message as NodeUpdateState))
          as NodeUpdateState;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use NodeUpdateState() / NodeUpdateState.new instead')
  static NodeUpdateState create() => NodeUpdateState._();
  static $pb.GeneratedMessage $_createMessage() => NodeUpdateState._();
  @$core.override
  NodeUpdateState createEmptyInstance() => NodeUpdateState._();
  @$core.pragma('dart2js:noInline')
  static NodeUpdateState getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<NodeUpdateState>(
          NodeUpdateState.$_createMessage);
  static NodeUpdateState? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get phase => $_getSZ(0);
  @$pb.TagNumber(1)
  set phase($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasPhase() => $_has(0);
  @$pb.TagNumber(1)
  void clearPhase() => $_clearField(1);

  @$pb.TagNumber(2)
  $core.String get previousVersion => $_getSZ(1);
  @$pb.TagNumber(2)
  set previousVersion($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasPreviousVersion() => $_has(1);
  @$pb.TagNumber(2)
  void clearPreviousVersion() => $_clearField(2);

  @$pb.TagNumber(3)
  $core.String get version => $_getSZ(2);
  @$pb.TagNumber(3)
  set version($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasVersion() => $_has(2);
  @$pb.TagNumber(3)
  void clearVersion() => $_clearField(3);

  @$pb.TagNumber(4)
  $core.int get attempts => $_getIZ(3);
  @$pb.TagNumber(4)
  set attempts($core.int value) => $_setUnsignedInt32(3, value);
  @$pb.TagNumber(4)
  $core.bool hasAttempts() => $_has(3);
  @$pb.TagNumber(4)
  void clearAttempts() => $_clearField(4);
}

class CpuMetrics extends $pb.GeneratedMessage {
  factory CpuMetrics({
    $core.int? logicalCores,
    $core.double? usagePercent,
    $core.double? load1,
    $core.double? load5,
    $core.double? load15,
  }) {
    final result = CpuMetrics._();
    if (logicalCores != null) result.logicalCores = logicalCores;
    if (usagePercent != null) result.usagePercent = usagePercent;
    if (load1 != null) result.load1 = load1;
    if (load5 != null) result.load5 = load5;
    if (load15 != null) result.load15 = load15;
    return result;
  }

  CpuMetrics._();

  factory CpuMetrics.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      CpuMetrics()..mergeFromBuffer(data, registry);
  factory CpuMetrics.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      CpuMetrics()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'CpuMetrics',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: CpuMetrics.$_createMessage)
    ..aI(1, _omitFieldNames ? '' : 'logicalCores')
    ..aD(2, _omitFieldNames ? '' : 'usagePercent')
    ..aD(3, _omitFieldNames ? '' : 'load1')
    ..aD(4, _omitFieldNames ? '' : 'load5')
    ..aD(5, _omitFieldNames ? '' : 'load15')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  CpuMetrics clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  CpuMetrics copyWith(void Function(CpuMetrics) updates) =>
      super.copyWith((message) => updates(message as CpuMetrics)) as CpuMetrics;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use CpuMetrics() / CpuMetrics.new instead')
  static CpuMetrics create() => CpuMetrics._();
  static $pb.GeneratedMessage $_createMessage() => CpuMetrics._();
  @$core.override
  CpuMetrics createEmptyInstance() => CpuMetrics._();
  @$core.pragma('dart2js:noInline')
  static CpuMetrics getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<CpuMetrics>(CpuMetrics.$_createMessage);
  static CpuMetrics? _defaultInstance;

  @$pb.TagNumber(1)
  $core.int get logicalCores => $_getIZ(0);
  @$pb.TagNumber(1)
  set logicalCores($core.int value) => $_setSignedInt32(0, value);
  @$pb.TagNumber(1)
  $core.bool hasLogicalCores() => $_has(0);
  @$pb.TagNumber(1)
  void clearLogicalCores() => $_clearField(1);

  @$pb.TagNumber(2)
  $core.double get usagePercent => $_getN(1);
  @$pb.TagNumber(2)
  set usagePercent($core.double value) => $_setDouble(1, value);
  @$pb.TagNumber(2)
  $core.bool hasUsagePercent() => $_has(1);
  @$pb.TagNumber(2)
  void clearUsagePercent() => $_clearField(2);

  /// load averages are unset where the OS has none (Windows).
  @$pb.TagNumber(3)
  $core.double get load1 => $_getN(2);
  @$pb.TagNumber(3)
  set load1($core.double value) => $_setDouble(2, value);
  @$pb.TagNumber(3)
  $core.bool hasLoad1() => $_has(2);
  @$pb.TagNumber(3)
  void clearLoad1() => $_clearField(3);

  @$pb.TagNumber(4)
  $core.double get load5 => $_getN(3);
  @$pb.TagNumber(4)
  set load5($core.double value) => $_setDouble(3, value);
  @$pb.TagNumber(4)
  $core.bool hasLoad5() => $_has(3);
  @$pb.TagNumber(4)
  void clearLoad5() => $_clearField(4);

  @$pb.TagNumber(5)
  $core.double get load15 => $_getN(4);
  @$pb.TagNumber(5)
  set load15($core.double value) => $_setDouble(4, value);
  @$pb.TagNumber(5)
  $core.bool hasLoad15() => $_has(4);
  @$pb.TagNumber(5)
  void clearLoad15() => $_clearField(5);
}

class MemoryMetrics extends $pb.GeneratedMessage {
  factory MemoryMetrics({
    $fixnum.Int64? totalBytes,
    $fixnum.Int64? usedBytes,
    $fixnum.Int64? availableBytes,
    $core.double? usedPercent,
    $fixnum.Int64? swapTotalBytes,
    $fixnum.Int64? swapUsedBytes,
    $core.double? swapUsedPercent,
  }) {
    final result = MemoryMetrics._();
    if (totalBytes != null) result.totalBytes = totalBytes;
    if (usedBytes != null) result.usedBytes = usedBytes;
    if (availableBytes != null) result.availableBytes = availableBytes;
    if (usedPercent != null) result.usedPercent = usedPercent;
    if (swapTotalBytes != null) result.swapTotalBytes = swapTotalBytes;
    if (swapUsedBytes != null) result.swapUsedBytes = swapUsedBytes;
    if (swapUsedPercent != null) result.swapUsedPercent = swapUsedPercent;
    return result;
  }

  MemoryMetrics._();

  factory MemoryMetrics.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      MemoryMetrics()..mergeFromBuffer(data, registry);
  factory MemoryMetrics.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      MemoryMetrics()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'MemoryMetrics',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: MemoryMetrics.$_createMessage)
    ..a<$fixnum.Int64>(
        1, _omitFieldNames ? '' : 'totalBytes', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..a<$fixnum.Int64>(
        2, _omitFieldNames ? '' : 'usedBytes', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..a<$fixnum.Int64>(
        3, _omitFieldNames ? '' : 'availableBytes', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..aD(4, _omitFieldNames ? '' : 'usedPercent')
    ..a<$fixnum.Int64>(
        5, _omitFieldNames ? '' : 'swapTotalBytes', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..a<$fixnum.Int64>(
        6, _omitFieldNames ? '' : 'swapUsedBytes', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..aD(7, _omitFieldNames ? '' : 'swapUsedPercent')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  MemoryMetrics clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  MemoryMetrics copyWith(void Function(MemoryMetrics) updates) =>
      super.copyWith((message) => updates(message as MemoryMetrics))
          as MemoryMetrics;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use MemoryMetrics() / MemoryMetrics.new instead')
  static MemoryMetrics create() => MemoryMetrics._();
  static $pb.GeneratedMessage $_createMessage() => MemoryMetrics._();
  @$core.override
  MemoryMetrics createEmptyInstance() => MemoryMetrics._();
  @$core.pragma('dart2js:noInline')
  static MemoryMetrics getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<MemoryMetrics>(
          MemoryMetrics.$_createMessage);
  static MemoryMetrics? _defaultInstance;

  @$pb.TagNumber(1)
  $fixnum.Int64 get totalBytes => $_getI64(0);
  @$pb.TagNumber(1)
  set totalBytes($fixnum.Int64 value) => $_setInt64(0, value);
  @$pb.TagNumber(1)
  $core.bool hasTotalBytes() => $_has(0);
  @$pb.TagNumber(1)
  void clearTotalBytes() => $_clearField(1);

  @$pb.TagNumber(2)
  $fixnum.Int64 get usedBytes => $_getI64(1);
  @$pb.TagNumber(2)
  set usedBytes($fixnum.Int64 value) => $_setInt64(1, value);
  @$pb.TagNumber(2)
  $core.bool hasUsedBytes() => $_has(1);
  @$pb.TagNumber(2)
  void clearUsedBytes() => $_clearField(2);

  @$pb.TagNumber(3)
  $fixnum.Int64 get availableBytes => $_getI64(2);
  @$pb.TagNumber(3)
  set availableBytes($fixnum.Int64 value) => $_setInt64(2, value);
  @$pb.TagNumber(3)
  $core.bool hasAvailableBytes() => $_has(2);
  @$pb.TagNumber(3)
  void clearAvailableBytes() => $_clearField(3);

  @$pb.TagNumber(4)
  $core.double get usedPercent => $_getN(3);
  @$pb.TagNumber(4)
  set usedPercent($core.double value) => $_setDouble(3, value);
  @$pb.TagNumber(4)
  $core.bool hasUsedPercent() => $_has(3);
  @$pb.TagNumber(4)
  void clearUsedPercent() => $_clearField(4);

  @$pb.TagNumber(5)
  $fixnum.Int64 get swapTotalBytes => $_getI64(4);
  @$pb.TagNumber(5)
  set swapTotalBytes($fixnum.Int64 value) => $_setInt64(4, value);
  @$pb.TagNumber(5)
  $core.bool hasSwapTotalBytes() => $_has(4);
  @$pb.TagNumber(5)
  void clearSwapTotalBytes() => $_clearField(5);

  @$pb.TagNumber(6)
  $fixnum.Int64 get swapUsedBytes => $_getI64(5);
  @$pb.TagNumber(6)
  set swapUsedBytes($fixnum.Int64 value) => $_setInt64(5, value);
  @$pb.TagNumber(6)
  $core.bool hasSwapUsedBytes() => $_has(5);
  @$pb.TagNumber(6)
  void clearSwapUsedBytes() => $_clearField(6);

  @$pb.TagNumber(7)
  $core.double get swapUsedPercent => $_getN(6);
  @$pb.TagNumber(7)
  set swapUsedPercent($core.double value) => $_setDouble(6, value);
  @$pb.TagNumber(7)
  $core.bool hasSwapUsedPercent() => $_has(6);
  @$pb.TagNumber(7)
  void clearSwapUsedPercent() => $_clearField(7);
}

class FilesystemMetrics extends $pb.GeneratedMessage {
  factory FilesystemMetrics({
    $core.String? mount,
    $core.String? device,
    $core.String? fstype,
    $fixnum.Int64? totalBytes,
    $fixnum.Int64? usedBytes,
    $fixnum.Int64? freeBytes,
    $core.double? usedPercent,
    $core.bool? primary,
  }) {
    final result = FilesystemMetrics._();
    if (mount != null) result.mount = mount;
    if (device != null) result.device = device;
    if (fstype != null) result.fstype = fstype;
    if (totalBytes != null) result.totalBytes = totalBytes;
    if (usedBytes != null) result.usedBytes = usedBytes;
    if (freeBytes != null) result.freeBytes = freeBytes;
    if (usedPercent != null) result.usedPercent = usedPercent;
    if (primary != null) result.primary = primary;
    return result;
  }

  FilesystemMetrics._();

  factory FilesystemMetrics.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      FilesystemMetrics()..mergeFromBuffer(data, registry);
  factory FilesystemMetrics.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      FilesystemMetrics()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'FilesystemMetrics',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: FilesystemMetrics.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'mount')
    ..aOS(2, _omitFieldNames ? '' : 'device')
    ..aOS(3, _omitFieldNames ? '' : 'fstype')
    ..a<$fixnum.Int64>(
        4, _omitFieldNames ? '' : 'totalBytes', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..a<$fixnum.Int64>(
        5, _omitFieldNames ? '' : 'usedBytes', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..a<$fixnum.Int64>(
        6, _omitFieldNames ? '' : 'freeBytes', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..aD(7, _omitFieldNames ? '' : 'usedPercent')
    ..aOB(8, _omitFieldNames ? '' : 'primary')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  FilesystemMetrics clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  FilesystemMetrics copyWith(void Function(FilesystemMetrics) updates) =>
      super.copyWith((message) => updates(message as FilesystemMetrics))
          as FilesystemMetrics;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use FilesystemMetrics() / FilesystemMetrics.new instead')
  static FilesystemMetrics create() => FilesystemMetrics._();
  static $pb.GeneratedMessage $_createMessage() => FilesystemMetrics._();
  @$core.override
  FilesystemMetrics createEmptyInstance() => FilesystemMetrics._();
  @$core.pragma('dart2js:noInline')
  static FilesystemMetrics getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<FilesystemMetrics>(
          FilesystemMetrics.$_createMessage);
  static FilesystemMetrics? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get mount => $_getSZ(0);
  @$pb.TagNumber(1)
  set mount($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasMount() => $_has(0);
  @$pb.TagNumber(1)
  void clearMount() => $_clearField(1);

  @$pb.TagNumber(2)
  $core.String get device => $_getSZ(1);
  @$pb.TagNumber(2)
  set device($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasDevice() => $_has(1);
  @$pb.TagNumber(2)
  void clearDevice() => $_clearField(2);

  @$pb.TagNumber(3)
  $core.String get fstype => $_getSZ(2);
  @$pb.TagNumber(3)
  set fstype($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasFstype() => $_has(2);
  @$pb.TagNumber(3)
  void clearFstype() => $_clearField(3);

  @$pb.TagNumber(4)
  $fixnum.Int64 get totalBytes => $_getI64(3);
  @$pb.TagNumber(4)
  set totalBytes($fixnum.Int64 value) => $_setInt64(3, value);
  @$pb.TagNumber(4)
  $core.bool hasTotalBytes() => $_has(3);
  @$pb.TagNumber(4)
  void clearTotalBytes() => $_clearField(4);

  @$pb.TagNumber(5)
  $fixnum.Int64 get usedBytes => $_getI64(4);
  @$pb.TagNumber(5)
  set usedBytes($fixnum.Int64 value) => $_setInt64(4, value);
  @$pb.TagNumber(5)
  $core.bool hasUsedBytes() => $_has(4);
  @$pb.TagNumber(5)
  void clearUsedBytes() => $_clearField(5);

  @$pb.TagNumber(6)
  $fixnum.Int64 get freeBytes => $_getI64(5);
  @$pb.TagNumber(6)
  set freeBytes($fixnum.Int64 value) => $_setInt64(5, value);
  @$pb.TagNumber(6)
  $core.bool hasFreeBytes() => $_has(5);
  @$pb.TagNumber(6)
  void clearFreeBytes() => $_clearField(6);

  @$pb.TagNumber(7)
  $core.double get usedPercent => $_getN(6);
  @$pb.TagNumber(7)
  set usedPercent($core.double value) => $_setDouble(6, value);
  @$pb.TagNumber(7)
  $core.bool hasUsedPercent() => $_has(6);
  @$pb.TagNumber(7)
  void clearUsedPercent() => $_clearField(7);

  /// The filesystem holding the OS or the agent home; shown as the node DISK.
  @$pb.TagNumber(8)
  $core.bool get primary => $_getBF(7);
  @$pb.TagNumber(8)
  set primary($core.bool value) => $_setBool(7, value);
  @$pb.TagNumber(8)
  $core.bool hasPrimary() => $_has(7);
  @$pb.TagNumber(8)
  void clearPrimary() => $_clearField(8);
}

class GpuMetrics extends $pb.GeneratedMessage {
  factory GpuMetrics({
    $core.int? index,
    $core.String? vendor,
    $core.String? name,
    $core.double? utilizationPercent,
    $fixnum.Int64? memoryUsedBytes,
    $fixnum.Int64? memoryTotalBytes,
    $core.double? temperatureCelsius,
    $core.bool? integrated,
    $core.String? driver,
    $core.String? busId,
  }) {
    final result = GpuMetrics._();
    if (index != null) result.index = index;
    if (vendor != null) result.vendor = vendor;
    if (name != null) result.name = name;
    if (utilizationPercent != null)
      result.utilizationPercent = utilizationPercent;
    if (memoryUsedBytes != null) result.memoryUsedBytes = memoryUsedBytes;
    if (memoryTotalBytes != null) result.memoryTotalBytes = memoryTotalBytes;
    if (temperatureCelsius != null)
      result.temperatureCelsius = temperatureCelsius;
    if (integrated != null) result.integrated = integrated;
    if (driver != null) result.driver = driver;
    if (busId != null) result.busId = busId;
    return result;
  }

  GpuMetrics._();

  factory GpuMetrics.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      GpuMetrics()..mergeFromBuffer(data, registry);
  factory GpuMetrics.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      GpuMetrics()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'GpuMetrics',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: GpuMetrics.$_createMessage)
    ..aI(1, _omitFieldNames ? '' : 'index')
    ..aOS(2, _omitFieldNames ? '' : 'vendor')
    ..aOS(3, _omitFieldNames ? '' : 'name')
    ..aD(4, _omitFieldNames ? '' : 'utilizationPercent')
    ..a<$fixnum.Int64>(
        5, _omitFieldNames ? '' : 'memoryUsedBytes', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..a<$fixnum.Int64>(
        6, _omitFieldNames ? '' : 'memoryTotalBytes', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..aD(7, _omitFieldNames ? '' : 'temperatureCelsius')
    ..aOB(8, _omitFieldNames ? '' : 'integrated')
    ..aOS(9, _omitFieldNames ? '' : 'driver')
    ..aOS(10, _omitFieldNames ? '' : 'busId')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  GpuMetrics clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  GpuMetrics copyWith(void Function(GpuMetrics) updates) =>
      super.copyWith((message) => updates(message as GpuMetrics)) as GpuMetrics;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use GpuMetrics() / GpuMetrics.new instead')
  static GpuMetrics create() => GpuMetrics._();
  static $pb.GeneratedMessage $_createMessage() => GpuMetrics._();
  @$core.override
  GpuMetrics createEmptyInstance() => GpuMetrics._();
  @$core.pragma('dart2js:noInline')
  static GpuMetrics getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<GpuMetrics>(GpuMetrics.$_createMessage);
  static GpuMetrics? _defaultInstance;

  @$pb.TagNumber(1)
  $core.int get index => $_getIZ(0);
  @$pb.TagNumber(1)
  set index($core.int value) => $_setSignedInt32(0, value);
  @$pb.TagNumber(1)
  $core.bool hasIndex() => $_has(0);
  @$pb.TagNumber(1)
  void clearIndex() => $_clearField(1);

  /// vendor is lowercase: nvidia, amd, intel, qualcomm, arm, ...
  @$pb.TagNumber(2)
  $core.String get vendor => $_getSZ(1);
  @$pb.TagNumber(2)
  set vendor($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasVendor() => $_has(1);
  @$pb.TagNumber(2)
  void clearVendor() => $_clearField(2);

  @$pb.TagNumber(3)
  $core.String get name => $_getSZ(2);
  @$pb.TagNumber(3)
  set name($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasName() => $_has(2);
  @$pb.TagNumber(3)
  void clearName() => $_clearField(3);

  /// utilization_percent is -1 when the device does not report it. For Intel
  /// GPUs on Linux without xpu-smi it is the share of time the render tile
  /// was out of its idle power state.
  @$pb.TagNumber(4)
  $core.double get utilizationPercent => $_getN(3);
  @$pb.TagNumber(4)
  set utilizationPercent($core.double value) => $_setDouble(3, value);
  @$pb.TagNumber(4)
  $core.bool hasUtilizationPercent() => $_has(3);
  @$pb.TagNumber(4)
  void clearUtilizationPercent() => $_clearField(4);

  /// memory_* are dedicated VRAM. An integrated GPU shares system memory and
  /// reports neither. memory_used_bytes is unset when the driver does not
  /// report it (Intel xe on Linux without xpu-smi).
  @$pb.TagNumber(5)
  $fixnum.Int64 get memoryUsedBytes => $_getI64(4);
  @$pb.TagNumber(5)
  set memoryUsedBytes($fixnum.Int64 value) => $_setInt64(4, value);
  @$pb.TagNumber(5)
  $core.bool hasMemoryUsedBytes() => $_has(4);
  @$pb.TagNumber(5)
  void clearMemoryUsedBytes() => $_clearField(5);

  @$pb.TagNumber(6)
  $fixnum.Int64 get memoryTotalBytes => $_getI64(5);
  @$pb.TagNumber(6)
  set memoryTotalBytes($fixnum.Int64 value) => $_setInt64(5, value);
  @$pb.TagNumber(6)
  $core.bool hasMemoryTotalBytes() => $_has(5);
  @$pb.TagNumber(6)
  void clearMemoryTotalBytes() => $_clearField(6);

  /// temperature_celsius is -1 when the device does not report it.
  @$pb.TagNumber(7)
  $core.double get temperatureCelsius => $_getN(6);
  @$pb.TagNumber(7)
  set temperatureCelsius($core.double value) => $_setDouble(6, value);
  @$pb.TagNumber(7)
  $core.bool hasTemperatureCelsius() => $_has(6);
  @$pb.TagNumber(7)
  void clearTemperatureCelsius() => $_clearField(7);

  /// integrated is set for a GPU on the SoC or CPU package.
  @$pb.TagNumber(8)
  $core.bool get integrated => $_getBF(7);
  @$pb.TagNumber(8)
  set integrated($core.bool value) => $_setBool(7, value);
  @$pb.TagNumber(8)
  $core.bool hasIntegrated() => $_has(7);
  @$pb.TagNumber(8)
  void clearIntegrated() => $_clearField(8);

  /// driver is the kernel driver (Linux) or driver service (Windows), with its
  /// version when known, e.g. "xe", "mali r53p0-00eac0", "igfxnd 32.0.101.8805".
  @$pb.TagNumber(9)
  $core.String get driver => $_getSZ(8);
  @$pb.TagNumber(9)
  set driver($core.String value) => $_setString(8, value);
  @$pb.TagNumber(9)
  $core.bool hasDriver() => $_has(8);
  @$pb.TagNumber(9)
  void clearDriver() => $_clearField(9);

  /// bus_id identifies the adapter: the PCI address (0000:0b:00.0) or the
  /// platform device (CIXH5000:00, ACPI\VEN_QCOM&DEV_0F36...). Empty for
  /// GPUs reported by nvidia-smi and rocm-smi.
  @$pb.TagNumber(10)
  $core.String get busId => $_getSZ(9);
  @$pb.TagNumber(10)
  set busId($core.String value) => $_setString(9, value);
  @$pb.TagNumber(10)
  $core.bool hasBusId() => $_has(9);
  @$pb.TagNumber(10)
  void clearBusId() => $_clearField(10);
}

/// AcceleratorMetrics describes a compute accelerator that is not a GPU.
class AcceleratorMetrics extends $pb.GeneratedMessage {
  factory AcceleratorMetrics({
    $core.String? kind,
    $core.String? vendor,
    $core.String? name,
    $core.String? driver,
    $core.double? utilizationPercent,
    $fixnum.Int64? frequencyHz,
    $core.String? busId,
  }) {
    final result = AcceleratorMetrics._();
    if (kind != null) result.kind = kind;
    if (vendor != null) result.vendor = vendor;
    if (name != null) result.name = name;
    if (driver != null) result.driver = driver;
    if (utilizationPercent != null)
      result.utilizationPercent = utilizationPercent;
    if (frequencyHz != null) result.frequencyHz = frequencyHz;
    if (busId != null) result.busId = busId;
    return result;
  }

  AcceleratorMetrics._();

  factory AcceleratorMetrics.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      AcceleratorMetrics()..mergeFromBuffer(data, registry);
  factory AcceleratorMetrics.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      AcceleratorMetrics()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'AcceleratorMetrics',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: AcceleratorMetrics.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'kind')
    ..aOS(2, _omitFieldNames ? '' : 'vendor')
    ..aOS(3, _omitFieldNames ? '' : 'name')
    ..aOS(4, _omitFieldNames ? '' : 'driver')
    ..aD(5, _omitFieldNames ? '' : 'utilizationPercent')
    ..a<$fixnum.Int64>(
        6, _omitFieldNames ? '' : 'frequencyHz', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..aOS(7, _omitFieldNames ? '' : 'busId')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  AcceleratorMetrics clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  AcceleratorMetrics copyWith(void Function(AcceleratorMetrics) updates) =>
      super.copyWith((message) => updates(message as AcceleratorMetrics))
          as AcceleratorMetrics;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use AcceleratorMetrics() / AcceleratorMetrics.new instead')
  static AcceleratorMetrics create() => AcceleratorMetrics._();
  static $pb.GeneratedMessage $_createMessage() => AcceleratorMetrics._();
  @$core.override
  AcceleratorMetrics createEmptyInstance() => AcceleratorMetrics._();
  @$core.pragma('dart2js:noInline')
  static AcceleratorMetrics getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<AcceleratorMetrics>(
          AcceleratorMetrics.$_createMessage);
  static AcceleratorMetrics? _defaultInstance;

  /// kind is "npu".
  @$pb.TagNumber(1)
  $core.String get kind => $_getSZ(0);
  @$pb.TagNumber(1)
  set kind($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasKind() => $_has(0);
  @$pb.TagNumber(1)
  void clearKind() => $_clearField(1);

  /// vendor is lowercase: intel, amd, qualcomm, armchina, ...
  @$pb.TagNumber(2)
  $core.String get vendor => $_getSZ(1);
  @$pb.TagNumber(2)
  set vendor($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasVendor() => $_has(1);
  @$pb.TagNumber(2)
  void clearVendor() => $_clearField(2);

  @$pb.TagNumber(3)
  $core.String get name => $_getSZ(2);
  @$pb.TagNumber(3)
  set name($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasName() => $_has(2);
  @$pb.TagNumber(3)
  void clearName() => $_clearField(3);

  /// driver is as in GpuMetrics.driver.
  @$pb.TagNumber(4)
  $core.String get driver => $_getSZ(3);
  @$pb.TagNumber(4)
  set driver($core.String value) => $_setString(3, value);
  @$pb.TagNumber(4)
  $core.bool hasDriver() => $_has(3);
  @$pb.TagNumber(4)
  void clearDriver() => $_clearField(4);

  /// utilization_percent is -1 when the device does not report it.
  @$pb.TagNumber(5)
  $core.double get utilizationPercent => $_getN(4);
  @$pb.TagNumber(5)
  set utilizationPercent($core.double value) => $_setDouble(4, value);
  @$pb.TagNumber(5)
  $core.bool hasUtilizationPercent() => $_has(4);
  @$pb.TagNumber(5)
  void clearUtilizationPercent() => $_clearField(5);

  /// frequency_hz is the current clock, 0 when unknown.
  @$pb.TagNumber(6)
  $fixnum.Int64 get frequencyHz => $_getI64(5);
  @$pb.TagNumber(6)
  set frequencyHz($fixnum.Int64 value) => $_setInt64(5, value);
  @$pb.TagNumber(6)
  $core.bool hasFrequencyHz() => $_has(5);
  @$pb.TagNumber(6)
  void clearFrequencyHz() => $_clearField(6);

  /// bus_id is as in GpuMetrics.bus_id.
  @$pb.TagNumber(7)
  $core.String get busId => $_getSZ(6);
  @$pb.TagNumber(7)
  set busId($core.String value) => $_setString(6, value);
  @$pb.TagNumber(7)
  $core.bool hasBusId() => $_has(6);
  @$pb.TagNumber(7)
  void clearBusId() => $_clearField(7);
}

class ContainerEngine extends $pb.GeneratedMessage {
  factory ContainerEngine({
    $core.String? kind,
    $core.String? version,
    EngineDiskUsage? disk,
    $0.Timestamp? diskMeasuredAt,
    $core.Iterable<ContainerMetrics>? containers,
  }) {
    final result = ContainerEngine._();
    if (kind != null) result.kind = kind;
    if (version != null) result.version = version;
    if (disk != null) result.disk = disk;
    if (diskMeasuredAt != null) result.diskMeasuredAt = diskMeasuredAt;
    if (containers != null) result.containers.addAll(containers);
    return result;
  }

  ContainerEngine._();

  factory ContainerEngine.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ContainerEngine()..mergeFromBuffer(data, registry);
  factory ContainerEngine.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ContainerEngine()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ContainerEngine',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ContainerEngine.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'kind')
    ..aOS(2, _omitFieldNames ? '' : 'version')
    ..aOM<EngineDiskUsage>(3, _omitFieldNames ? '' : 'disk',
        subBuilder: EngineDiskUsage.$_createMessage)
    ..aOM<$0.Timestamp>(4, _omitFieldNames ? '' : 'diskMeasuredAt',
        subBuilder: $0.Timestamp.$_createMessage)
    ..pPM<ContainerMetrics>(5, _omitFieldNames ? '' : 'containers',
        subBuilder: ContainerMetrics.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ContainerEngine clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ContainerEngine copyWith(void Function(ContainerEngine) updates) =>
      super.copyWith((message) => updates(message as ContainerEngine))
          as ContainerEngine;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use ContainerEngine() / ContainerEngine.new instead')
  static ContainerEngine create() => ContainerEngine._();
  static $pb.GeneratedMessage $_createMessage() => ContainerEngine._();
  @$core.override
  ContainerEngine createEmptyInstance() => ContainerEngine._();
  @$core.pragma('dart2js:noInline')
  static ContainerEngine getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<ContainerEngine>(
          ContainerEngine.$_createMessage);
  static ContainerEngine? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get kind => $_getSZ(0);
  @$pb.TagNumber(1)
  set kind($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasKind() => $_has(0);
  @$pb.TagNumber(1)
  void clearKind() => $_clearField(1);

  @$pb.TagNumber(2)
  $core.String get version => $_getSZ(1);
  @$pb.TagNumber(2)
  set version($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasVersion() => $_has(1);
  @$pb.TagNumber(2)
  void clearVersion() => $_clearField(2);

  /// disk is unset until the first slow-interval measurement completes.
  @$pb.TagNumber(3)
  EngineDiskUsage get disk => $_getN(2);
  @$pb.TagNumber(3)
  set disk(EngineDiskUsage value) => $_setField(3, value);
  @$pb.TagNumber(3)
  $core.bool hasDisk() => $_has(2);
  @$pb.TagNumber(3)
  void clearDisk() => $_clearField(3);
  @$pb.TagNumber(3)
  EngineDiskUsage ensureDisk() => $_ensure(2);

  @$pb.TagNumber(4)
  $0.Timestamp get diskMeasuredAt => $_getN(3);
  @$pb.TagNumber(4)
  set diskMeasuredAt($0.Timestamp value) => $_setField(4, value);
  @$pb.TagNumber(4)
  $core.bool hasDiskMeasuredAt() => $_has(3);
  @$pb.TagNumber(4)
  void clearDiskMeasuredAt() => $_clearField(4);
  @$pb.TagNumber(4)
  $0.Timestamp ensureDiskMeasuredAt() => $_ensure(3);

  @$pb.TagNumber(5)
  $pb.PbList<ContainerMetrics> get containers => $_getList(4);
}

class EngineDiskUsage extends $pb.GeneratedMessage {
  factory EngineDiskUsage({
    DiskBucket? images,
    DiskBucket? containers,
    DiskBucket? volumes,
    DiskBucket? buildCache,
  }) {
    final result = EngineDiskUsage._();
    if (images != null) result.images = images;
    if (containers != null) result.containers = containers;
    if (volumes != null) result.volumes = volumes;
    if (buildCache != null) result.buildCache = buildCache;
    return result;
  }

  EngineDiskUsage._();

  factory EngineDiskUsage.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      EngineDiskUsage()..mergeFromBuffer(data, registry);
  factory EngineDiskUsage.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      EngineDiskUsage()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'EngineDiskUsage',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: EngineDiskUsage.$_createMessage)
    ..aOM<DiskBucket>(1, _omitFieldNames ? '' : 'images',
        subBuilder: DiskBucket.$_createMessage)
    ..aOM<DiskBucket>(2, _omitFieldNames ? '' : 'containers',
        subBuilder: DiskBucket.$_createMessage)
    ..aOM<DiskBucket>(3, _omitFieldNames ? '' : 'volumes',
        subBuilder: DiskBucket.$_createMessage)
    ..aOM<DiskBucket>(4, _omitFieldNames ? '' : 'buildCache',
        subBuilder: DiskBucket.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  EngineDiskUsage clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  EngineDiskUsage copyWith(void Function(EngineDiskUsage) updates) =>
      super.copyWith((message) => updates(message as EngineDiskUsage))
          as EngineDiskUsage;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use EngineDiskUsage() / EngineDiskUsage.new instead')
  static EngineDiskUsage create() => EngineDiskUsage._();
  static $pb.GeneratedMessage $_createMessage() => EngineDiskUsage._();
  @$core.override
  EngineDiskUsage createEmptyInstance() => EngineDiskUsage._();
  @$core.pragma('dart2js:noInline')
  static EngineDiskUsage getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<EngineDiskUsage>(
          EngineDiskUsage.$_createMessage);
  static EngineDiskUsage? _defaultInstance;

  @$pb.TagNumber(1)
  DiskBucket get images => $_getN(0);
  @$pb.TagNumber(1)
  set images(DiskBucket value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasImages() => $_has(0);
  @$pb.TagNumber(1)
  void clearImages() => $_clearField(1);
  @$pb.TagNumber(1)
  DiskBucket ensureImages() => $_ensure(0);

  @$pb.TagNumber(2)
  DiskBucket get containers => $_getN(1);
  @$pb.TagNumber(2)
  set containers(DiskBucket value) => $_setField(2, value);
  @$pb.TagNumber(2)
  $core.bool hasContainers() => $_has(1);
  @$pb.TagNumber(2)
  void clearContainers() => $_clearField(2);
  @$pb.TagNumber(2)
  DiskBucket ensureContainers() => $_ensure(1);

  @$pb.TagNumber(3)
  DiskBucket get volumes => $_getN(2);
  @$pb.TagNumber(3)
  set volumes(DiskBucket value) => $_setField(3, value);
  @$pb.TagNumber(3)
  $core.bool hasVolumes() => $_has(2);
  @$pb.TagNumber(3)
  void clearVolumes() => $_clearField(3);
  @$pb.TagNumber(3)
  DiskBucket ensureVolumes() => $_ensure(2);

  @$pb.TagNumber(4)
  DiskBucket get buildCache => $_getN(3);
  @$pb.TagNumber(4)
  set buildCache(DiskBucket value) => $_setField(4, value);
  @$pb.TagNumber(4)
  $core.bool hasBuildCache() => $_has(3);
  @$pb.TagNumber(4)
  void clearBuildCache() => $_clearField(4);
  @$pb.TagNumber(4)
  DiskBucket ensureBuildCache() => $_ensure(3);
}

class DiskBucket extends $pb.GeneratedMessage {
  factory DiskBucket({
    $fixnum.Int64? count,
    $fixnum.Int64? sizeBytes,
    $fixnum.Int64? reclaimableBytes,
  }) {
    final result = DiskBucket._();
    if (count != null) result.count = count;
    if (sizeBytes != null) result.sizeBytes = sizeBytes;
    if (reclaimableBytes != null) result.reclaimableBytes = reclaimableBytes;
    return result;
  }

  DiskBucket._();

  factory DiskBucket.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      DiskBucket()..mergeFromBuffer(data, registry);
  factory DiskBucket.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      DiskBucket()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'DiskBucket',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: DiskBucket.$_createMessage)
    ..aInt64(1, _omitFieldNames ? '' : 'count')
    ..aInt64(2, _omitFieldNames ? '' : 'sizeBytes')
    ..aInt64(3, _omitFieldNames ? '' : 'reclaimableBytes')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  DiskBucket clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  DiskBucket copyWith(void Function(DiskBucket) updates) =>
      super.copyWith((message) => updates(message as DiskBucket)) as DiskBucket;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use DiskBucket() / DiskBucket.new instead')
  static DiskBucket create() => DiskBucket._();
  static $pb.GeneratedMessage $_createMessage() => DiskBucket._();
  @$core.override
  DiskBucket createEmptyInstance() => DiskBucket._();
  @$core.pragma('dart2js:noInline')
  static DiskBucket getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<DiskBucket>(DiskBucket.$_createMessage);
  static DiskBucket? _defaultInstance;

  @$pb.TagNumber(1)
  $fixnum.Int64 get count => $_getI64(0);
  @$pb.TagNumber(1)
  set count($fixnum.Int64 value) => $_setInt64(0, value);
  @$pb.TagNumber(1)
  $core.bool hasCount() => $_has(0);
  @$pb.TagNumber(1)
  void clearCount() => $_clearField(1);

  @$pb.TagNumber(2)
  $fixnum.Int64 get sizeBytes => $_getI64(1);
  @$pb.TagNumber(2)
  set sizeBytes($fixnum.Int64 value) => $_setInt64(1, value);
  @$pb.TagNumber(2)
  $core.bool hasSizeBytes() => $_has(1);
  @$pb.TagNumber(2)
  void clearSizeBytes() => $_clearField(2);

  @$pb.TagNumber(3)
  $fixnum.Int64 get reclaimableBytes => $_getI64(2);
  @$pb.TagNumber(3)
  set reclaimableBytes($fixnum.Int64 value) => $_setInt64(2, value);
  @$pb.TagNumber(3)
  $core.bool hasReclaimableBytes() => $_has(2);
  @$pb.TagNumber(3)
  void clearReclaimableBytes() => $_clearField(3);
}

class ContainerMetrics extends $pb.GeneratedMessage {
  factory ContainerMetrics({
    $core.String? id,
    $core.String? name,
    $core.String? image,
    $core.String? state,
    $core.double? cpuPercent,
    $fixnum.Int64? memoryUsedBytes,
    $fixnum.Int64? memoryLimitBytes,
    $core.double? memoryPercent,
    $fixnum.Int64? writableLayerBytes,
    $fixnum.Int64? rootfsBytes,
    $core.String? statsError,
    $0.Timestamp? createdAt,
  }) {
    final result = ContainerMetrics._();
    if (id != null) result.id = id;
    if (name != null) result.name = name;
    if (image != null) result.image = image;
    if (state != null) result.state = state;
    if (cpuPercent != null) result.cpuPercent = cpuPercent;
    if (memoryUsedBytes != null) result.memoryUsedBytes = memoryUsedBytes;
    if (memoryLimitBytes != null) result.memoryLimitBytes = memoryLimitBytes;
    if (memoryPercent != null) result.memoryPercent = memoryPercent;
    if (writableLayerBytes != null)
      result.writableLayerBytes = writableLayerBytes;
    if (rootfsBytes != null) result.rootfsBytes = rootfsBytes;
    if (statsError != null) result.statsError = statsError;
    if (createdAt != null) result.createdAt = createdAt;
    return result;
  }

  ContainerMetrics._();

  factory ContainerMetrics.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ContainerMetrics()..mergeFromBuffer(data, registry);
  factory ContainerMetrics.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ContainerMetrics()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ContainerMetrics',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ContainerMetrics.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'id')
    ..aOS(2, _omitFieldNames ? '' : 'name')
    ..aOS(3, _omitFieldNames ? '' : 'image')
    ..aOS(4, _omitFieldNames ? '' : 'state')
    ..aD(5, _omitFieldNames ? '' : 'cpuPercent')
    ..a<$fixnum.Int64>(
        6, _omitFieldNames ? '' : 'memoryUsedBytes', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..a<$fixnum.Int64>(
        7, _omitFieldNames ? '' : 'memoryLimitBytes', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..aD(8, _omitFieldNames ? '' : 'memoryPercent')
    ..aInt64(9, _omitFieldNames ? '' : 'writableLayerBytes')
    ..aInt64(10, _omitFieldNames ? '' : 'rootfsBytes')
    ..aOS(11, _omitFieldNames ? '' : 'statsError')
    ..aOM<$0.Timestamp>(12, _omitFieldNames ? '' : 'createdAt',
        subBuilder: $0.Timestamp.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ContainerMetrics clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ContainerMetrics copyWith(void Function(ContainerMetrics) updates) =>
      super.copyWith((message) => updates(message as ContainerMetrics))
          as ContainerMetrics;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use ContainerMetrics() / ContainerMetrics.new instead')
  static ContainerMetrics create() => ContainerMetrics._();
  static $pb.GeneratedMessage $_createMessage() => ContainerMetrics._();
  @$core.override
  ContainerMetrics createEmptyInstance() => ContainerMetrics._();
  @$core.pragma('dart2js:noInline')
  static ContainerMetrics getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<ContainerMetrics>(
          ContainerMetrics.$_createMessage);
  static ContainerMetrics? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get id => $_getSZ(0);
  @$pb.TagNumber(1)
  set id($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasId() => $_has(0);
  @$pb.TagNumber(1)
  void clearId() => $_clearField(1);

  @$pb.TagNumber(2)
  $core.String get name => $_getSZ(1);
  @$pb.TagNumber(2)
  set name($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasName() => $_has(1);
  @$pb.TagNumber(2)
  void clearName() => $_clearField(2);

  @$pb.TagNumber(3)
  $core.String get image => $_getSZ(2);
  @$pb.TagNumber(3)
  set image($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasImage() => $_has(2);
  @$pb.TagNumber(3)
  void clearImage() => $_clearField(3);

  @$pb.TagNumber(4)
  $core.String get state => $_getSZ(3);
  @$pb.TagNumber(4)
  set state($core.String value) => $_setString(3, value);
  @$pb.TagNumber(4)
  $core.bool hasState() => $_has(3);
  @$pb.TagNumber(4)
  void clearState() => $_clearField(4);

  @$pb.TagNumber(5)
  $core.double get cpuPercent => $_getN(4);
  @$pb.TagNumber(5)
  set cpuPercent($core.double value) => $_setDouble(4, value);
  @$pb.TagNumber(5)
  $core.bool hasCpuPercent() => $_has(4);
  @$pb.TagNumber(5)
  void clearCpuPercent() => $_clearField(5);

  @$pb.TagNumber(6)
  $fixnum.Int64 get memoryUsedBytes => $_getI64(5);
  @$pb.TagNumber(6)
  set memoryUsedBytes($fixnum.Int64 value) => $_setInt64(5, value);
  @$pb.TagNumber(6)
  $core.bool hasMemoryUsedBytes() => $_has(5);
  @$pb.TagNumber(6)
  void clearMemoryUsedBytes() => $_clearField(6);

  @$pb.TagNumber(7)
  $fixnum.Int64 get memoryLimitBytes => $_getI64(6);
  @$pb.TagNumber(7)
  set memoryLimitBytes($fixnum.Int64 value) => $_setInt64(6, value);
  @$pb.TagNumber(7)
  $core.bool hasMemoryLimitBytes() => $_has(6);
  @$pb.TagNumber(7)
  void clearMemoryLimitBytes() => $_clearField(7);

  @$pb.TagNumber(8)
  $core.double get memoryPercent => $_getN(7);
  @$pb.TagNumber(8)
  set memoryPercent($core.double value) => $_setDouble(7, value);
  @$pb.TagNumber(8)
  $core.bool hasMemoryPercent() => $_has(7);
  @$pb.TagNumber(8)
  void clearMemoryPercent() => $_clearField(8);

  /// writable_layer_bytes and rootfs_bytes come from the slow-interval df and
  /// are 0 until measured.
  @$pb.TagNumber(9)
  $fixnum.Int64 get writableLayerBytes => $_getI64(8);
  @$pb.TagNumber(9)
  set writableLayerBytes($fixnum.Int64 value) => $_setInt64(8, value);
  @$pb.TagNumber(9)
  $core.bool hasWritableLayerBytes() => $_has(8);
  @$pb.TagNumber(9)
  void clearWritableLayerBytes() => $_clearField(9);

  @$pb.TagNumber(10)
  $fixnum.Int64 get rootfsBytes => $_getI64(9);
  @$pb.TagNumber(10)
  set rootfsBytes($fixnum.Int64 value) => $_setInt64(9, value);
  @$pb.TagNumber(10)
  $core.bool hasRootfsBytes() => $_has(9);
  @$pb.TagNumber(10)
  void clearRootfsBytes() => $_clearField(10);

  /// stats_error is set when stats for this container could not be read.
  @$pb.TagNumber(11)
  $core.String get statsError => $_getSZ(10);
  @$pb.TagNumber(11)
  set statsError($core.String value) => $_setString(10, value);
  @$pb.TagNumber(11)
  $core.bool hasStatsError() => $_has(10);
  @$pb.TagNumber(11)
  void clearStatsError() => $_clearField(11);

  /// created_at is when the container was created; stopped containers are
  /// listed too and carry no cpu or memory figures.
  @$pb.TagNumber(12)
  $0.Timestamp get createdAt => $_getN(11);
  @$pb.TagNumber(12)
  set createdAt($0.Timestamp value) => $_setField(12, value);
  @$pb.TagNumber(12)
  $core.bool hasCreatedAt() => $_has(11);
  @$pb.TagNumber(12)
  void clearCreatedAt() => $_clearField(12);
  @$pb.TagNumber(12)
  $0.Timestamp ensureCreatedAt() => $_ensure(11);
}

class DirSize extends $pb.GeneratedMessage {
  factory DirSize({
    $core.String? path,
    $fixnum.Int64? sizeBytes,
    $0.Timestamp? measuredAt,
    $3.Duration? took,
    $core.bool? complete,
    $core.String? error,
  }) {
    final result = DirSize._();
    if (path != null) result.path = path;
    if (sizeBytes != null) result.sizeBytes = sizeBytes;
    if (measuredAt != null) result.measuredAt = measuredAt;
    if (took != null) result.took = took;
    if (complete != null) result.complete = complete;
    if (error != null) result.error = error;
    return result;
  }

  DirSize._();

  factory DirSize.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      DirSize()..mergeFromBuffer(data, registry);
  factory DirSize.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      DirSize()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'DirSize',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: DirSize.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'path')
    ..aInt64(2, _omitFieldNames ? '' : 'sizeBytes')
    ..aOM<$0.Timestamp>(3, _omitFieldNames ? '' : 'measuredAt',
        subBuilder: $0.Timestamp.$_createMessage)
    ..aOM<$3.Duration>(4, _omitFieldNames ? '' : 'took',
        subBuilder: $3.Duration.$_createMessage)
    ..aOB(5, _omitFieldNames ? '' : 'complete')
    ..aOS(6, _omitFieldNames ? '' : 'error')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  DirSize clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  DirSize copyWith(void Function(DirSize) updates) =>
      super.copyWith((message) => updates(message as DirSize)) as DirSize;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use DirSize() / DirSize.new instead')
  static DirSize create() => DirSize._();
  static $pb.GeneratedMessage $_createMessage() => DirSize._();
  @$core.override
  DirSize createEmptyInstance() => DirSize._();
  @$core.pragma('dart2js:noInline')
  static DirSize getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<DirSize>(DirSize.$_createMessage);
  static DirSize? _defaultInstance;

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

  @$pb.TagNumber(3)
  $0.Timestamp get measuredAt => $_getN(2);
  @$pb.TagNumber(3)
  set measuredAt($0.Timestamp value) => $_setField(3, value);
  @$pb.TagNumber(3)
  $core.bool hasMeasuredAt() => $_has(2);
  @$pb.TagNumber(3)
  void clearMeasuredAt() => $_clearField(3);
  @$pb.TagNumber(3)
  $0.Timestamp ensureMeasuredAt() => $_ensure(2);

  @$pb.TagNumber(4)
  $3.Duration get took => $_getN(3);
  @$pb.TagNumber(4)
  set took($3.Duration value) => $_setField(4, value);
  @$pb.TagNumber(4)
  $core.bool hasTook() => $_has(3);
  @$pb.TagNumber(4)
  void clearTook() => $_clearField(4);
  @$pb.TagNumber(4)
  $3.Duration ensureTook() => $_ensure(3);

  /// complete is false when the time budget ended the walk early; size_bytes
  /// is then a lower bound.
  @$pb.TagNumber(5)
  $core.bool get complete => $_getBF(4);
  @$pb.TagNumber(5)
  set complete($core.bool value) => $_setBool(4, value);
  @$pb.TagNumber(5)
  $core.bool hasComplete() => $_has(4);
  @$pb.TagNumber(5)
  void clearComplete() => $_clearField(5);

  @$pb.TagNumber(6)
  $core.String get error => $_getSZ(5);
  @$pb.TagNumber(6)
  set error($core.String value) => $_setString(5, value);
  @$pb.TagNumber(6)
  $core.bool hasError() => $_has(5);
  @$pb.TagNumber(6)
  void clearError() => $_clearField(6);
}

class NodeAlert extends $pb.GeneratedMessage {
  factory NodeAlert({
    AlertKind? kind,
    $core.String? subject,
    $core.String? message,
    $core.double? value,
    $core.double? threshold,
  }) {
    final result = NodeAlert._();
    if (kind != null) result.kind = kind;
    if (subject != null) result.subject = subject;
    if (message != null) result.message = message;
    if (value != null) result.value = value;
    if (threshold != null) result.threshold = threshold;
    return result;
  }

  NodeAlert._();

  factory NodeAlert.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      NodeAlert()..mergeFromBuffer(data, registry);
  factory NodeAlert.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      NodeAlert()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'NodeAlert',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: NodeAlert.$_createMessage)
    ..aE<AlertKind>(1, _omitFieldNames ? '' : 'kind',
        enumValues: AlertKind.values)
    ..aOS(2, _omitFieldNames ? '' : 'subject')
    ..aOS(3, _omitFieldNames ? '' : 'message')
    ..aD(4, _omitFieldNames ? '' : 'value')
    ..aD(5, _omitFieldNames ? '' : 'threshold')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  NodeAlert clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  NodeAlert copyWith(void Function(NodeAlert) updates) =>
      super.copyWith((message) => updates(message as NodeAlert)) as NodeAlert;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use NodeAlert() / NodeAlert.new instead')
  static NodeAlert create() => NodeAlert._();
  static $pb.GeneratedMessage $_createMessage() => NodeAlert._();
  @$core.override
  NodeAlert createEmptyInstance() => NodeAlert._();
  @$core.pragma('dart2js:noInline')
  static NodeAlert getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<NodeAlert>(NodeAlert.$_createMessage);
  static NodeAlert? _defaultInstance;

  @$pb.TagNumber(1)
  AlertKind get kind => $_getN(0);
  @$pb.TagNumber(1)
  set kind(AlertKind value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasKind() => $_has(0);
  @$pb.TagNumber(1)
  void clearKind() => $_clearField(1);

  /// subject is the filesystem mount for disk alerts, empty otherwise.
  @$pb.TagNumber(2)
  $core.String get subject => $_getSZ(1);
  @$pb.TagNumber(2)
  set subject($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasSubject() => $_has(1);
  @$pb.TagNumber(2)
  void clearSubject() => $_clearField(2);

  @$pb.TagNumber(3)
  $core.String get message => $_getSZ(2);
  @$pb.TagNumber(3)
  set message($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasMessage() => $_has(2);
  @$pb.TagNumber(3)
  void clearMessage() => $_clearField(3);

  @$pb.TagNumber(4)
  $core.double get value => $_getN(3);
  @$pb.TagNumber(4)
  set value($core.double value) => $_setDouble(3, value);
  @$pb.TagNumber(4)
  $core.bool hasValue() => $_has(3);
  @$pb.TagNumber(4)
  void clearValue() => $_clearField(4);

  @$pb.TagNumber(5)
  $core.double get threshold => $_getN(4);
  @$pb.TagNumber(5)
  set threshold($core.double value) => $_setDouble(4, value);
  @$pb.TagNumber(5)
  $core.bool hasThreshold() => $_has(4);
  @$pb.TagNumber(5)
  void clearThreshold() => $_clearField(5);
}

/// NodeInfo is the inventory record of one node.
class NodeInfo extends $pb.GeneratedMessage {
  factory NodeInfo({
    $core.String? name,
    $core.String? address,
    $core.String? os,
    $core.String? arch,
    $core.Iterable<$core.String>? tags,
    $core.String? sshAlias,
    $core.String? tier,
    $core.String? notes,
    $core.bool? local,
    $core.bool? invited,
  }) {
    final result = NodeInfo._();
    if (name != null) result.name = name;
    if (address != null) result.address = address;
    if (os != null) result.os = os;
    if (arch != null) result.arch = arch;
    if (tags != null) result.tags.addAll(tags);
    if (sshAlias != null) result.sshAlias = sshAlias;
    if (tier != null) result.tier = tier;
    if (notes != null) result.notes = notes;
    if (local != null) result.local = local;
    if (invited != null) result.invited = invited;
    return result;
  }

  NodeInfo._();

  factory NodeInfo.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      NodeInfo()..mergeFromBuffer(data, registry);
  factory NodeInfo.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      NodeInfo()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'NodeInfo',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: NodeInfo.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'name')
    ..aOS(2, _omitFieldNames ? '' : 'address')
    ..aOS(3, _omitFieldNames ? '' : 'os')
    ..aOS(4, _omitFieldNames ? '' : 'arch')
    ..pPS(5, _omitFieldNames ? '' : 'tags')
    ..aOS(6, _omitFieldNames ? '' : 'sshAlias')
    ..aOS(7, _omitFieldNames ? '' : 'tier')
    ..aOS(8, _omitFieldNames ? '' : 'notes')
    ..aOB(9, _omitFieldNames ? '' : 'local')
    ..aOB(10, _omitFieldNames ? '' : 'invited')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  NodeInfo clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  NodeInfo copyWith(void Function(NodeInfo) updates) =>
      super.copyWith((message) => updates(message as NodeInfo)) as NodeInfo;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use NodeInfo() / NodeInfo.new instead')
  static NodeInfo create() => NodeInfo._();
  static $pb.GeneratedMessage $_createMessage() => NodeInfo._();
  @$core.override
  NodeInfo createEmptyInstance() => NodeInfo._();
  @$core.pragma('dart2js:noInline')
  static NodeInfo getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<NodeInfo>(NodeInfo.$_createMessage);
  static NodeInfo? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get name => $_getSZ(0);
  @$pb.TagNumber(1)
  set name($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasName() => $_has(0);
  @$pb.TagNumber(1)
  void clearName() => $_clearField(1);

  /// address is where the hub reaches the node agent, host:port (":7475" is
  /// assumed when no port is given). The node reports it when it joins.
  @$pb.TagNumber(2)
  $core.String get address => $_getSZ(1);
  @$pb.TagNumber(2)
  set address($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasAddress() => $_has(1);
  @$pb.TagNumber(2)
  void clearAddress() => $_clearField(2);

  @$pb.TagNumber(3)
  $core.String get os => $_getSZ(2);
  @$pb.TagNumber(3)
  set os($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasOs() => $_has(2);
  @$pb.TagNumber(3)
  void clearOs() => $_clearField(3);

  @$pb.TagNumber(4)
  $core.String get arch => $_getSZ(3);
  @$pb.TagNumber(4)
  set arch($core.String value) => $_setString(3, value);
  @$pb.TagNumber(4)
  $core.bool hasArch() => $_has(3);
  @$pb.TagNumber(4)
  void clearArch() => $_clearField(4);

  @$pb.TagNumber(5)
  $pb.PbList<$core.String> get tags => $_getList(4);

  /// ssh_alias is the break-glass `ssh` host alias.
  @$pb.TagNumber(6)
  $core.String get sshAlias => $_getSZ(5);
  @$pb.TagNumber(6)
  set sshAlias($core.String value) => $_setString(5, value);
  @$pb.TagNumber(6)
  $core.bool hasSshAlias() => $_has(5);
  @$pb.TagNumber(6)
  void clearSshAlias() => $_clearField(6);

  /// tier is "shared" (the compute pool) or "care" (a machine kept for a
  /// person, never scheduled or leased).
  @$pb.TagNumber(7)
  $core.String get tier => $_getSZ(6);
  @$pb.TagNumber(7)
  set tier($core.String value) => $_setString(6, value);
  @$pb.TagNumber(7)
  $core.bool hasTier() => $_has(6);
  @$pb.TagNumber(7)
  void clearTier() => $_clearField(7);

  @$pb.TagNumber(8)
  $core.String get notes => $_getSZ(7);
  @$pb.TagNumber(8)
  set notes($core.String value) => $_setString(7, value);
  @$pb.TagNumber(8)
  $core.bool hasNotes() => $_has(7);
  @$pb.TagNumber(8)
  void clearNotes() => $_clearField(8);

  /// local marks the hub's own node, which is not stored in the inventory.
  @$pb.TagNumber(9)
  $core.bool get local => $_getBF(8);
  @$pb.TagNumber(9)
  set local($core.bool value) => $_setBool(8, value);
  @$pb.TagNumber(9)
  $core.bool hasLocal() => $_has(8);
  @$pb.TagNumber(9)
  void clearLocal() => $_clearField(9);

  /// invited marks a node whose enrollment code was issued but which has not
  /// answered a poll yet; the hub clears it on the first successful poll.
  @$pb.TagNumber(10)
  $core.bool get invited => $_getBF(9);
  @$pb.TagNumber(10)
  set invited($core.bool value) => $_setBool(9, value);
  @$pb.TagNumber(10)
  $core.bool hasInvited() => $_has(9);
  @$pb.TagNumber(10)
  void clearInvited() => $_clearField(10);
}

class NodeStatus extends $pb.GeneratedMessage {
  factory NodeStatus({
    NodeInfo? node,
    NodeMetrics? metrics,
    NodeFreshness? freshness,
    $0.Timestamp? fetchedAt,
    $3.Duration? age,
    $core.String? error,
    $core.Iterable<NodeAlert>? alerts,
    $core.bool? draining,
    $core.Iterable<GpuBusy>? busyGpus,
  }) {
    final result = NodeStatus._();
    if (node != null) result.node = node;
    if (metrics != null) result.metrics = metrics;
    if (freshness != null) result.freshness = freshness;
    if (fetchedAt != null) result.fetchedAt = fetchedAt;
    if (age != null) result.age = age;
    if (error != null) result.error = error;
    if (alerts != null) result.alerts.addAll(alerts);
    if (draining != null) result.draining = draining;
    if (busyGpus != null) result.busyGpus.addAll(busyGpus);
    return result;
  }

  NodeStatus._();

  factory NodeStatus.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      NodeStatus()..mergeFromBuffer(data, registry);
  factory NodeStatus.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      NodeStatus()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'NodeStatus',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: NodeStatus.$_createMessage)
    ..aOM<NodeInfo>(1, _omitFieldNames ? '' : 'node',
        subBuilder: NodeInfo.$_createMessage)
    ..aOM<NodeMetrics>(2, _omitFieldNames ? '' : 'metrics',
        subBuilder: NodeMetrics.$_createMessage)
    ..aE<NodeFreshness>(3, _omitFieldNames ? '' : 'freshness',
        enumValues: NodeFreshness.values)
    ..aOM<$0.Timestamp>(4, _omitFieldNames ? '' : 'fetchedAt',
        subBuilder: $0.Timestamp.$_createMessage)
    ..aOM<$3.Duration>(5, _omitFieldNames ? '' : 'age',
        subBuilder: $3.Duration.$_createMessage)
    ..aOS(6, _omitFieldNames ? '' : 'error')
    ..pPM<NodeAlert>(7, _omitFieldNames ? '' : 'alerts',
        subBuilder: NodeAlert.$_createMessage)
    ..aOB(11, _omitFieldNames ? '' : 'draining')
    ..pPM<GpuBusy>(12, _omitFieldNames ? '' : 'busyGpus',
        subBuilder: GpuBusy.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  NodeStatus clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  NodeStatus copyWith(void Function(NodeStatus) updates) =>
      super.copyWith((message) => updates(message as NodeStatus)) as NodeStatus;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use NodeStatus() / NodeStatus.new instead')
  static NodeStatus create() => NodeStatus._();
  static $pb.GeneratedMessage $_createMessage() => NodeStatus._();
  @$core.override
  NodeStatus createEmptyInstance() => NodeStatus._();
  @$core.pragma('dart2js:noInline')
  static NodeStatus getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<NodeStatus>(NodeStatus.$_createMessage);
  static NodeStatus? _defaultInstance;

  @$pb.TagNumber(1)
  NodeInfo get node => $_getN(0);
  @$pb.TagNumber(1)
  set node(NodeInfo value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasNode() => $_has(0);
  @$pb.TagNumber(1)
  void clearNode() => $_clearField(1);
  @$pb.TagNumber(1)
  NodeInfo ensureNode() => $_ensure(0);

  /// metrics is the last good poll; unset when the node was never reached.
  @$pb.TagNumber(2)
  NodeMetrics get metrics => $_getN(1);
  @$pb.TagNumber(2)
  set metrics(NodeMetrics value) => $_setField(2, value);
  @$pb.TagNumber(2)
  $core.bool hasMetrics() => $_has(1);
  @$pb.TagNumber(2)
  void clearMetrics() => $_clearField(2);
  @$pb.TagNumber(2)
  NodeMetrics ensureMetrics() => $_ensure(1);

  @$pb.TagNumber(3)
  NodeFreshness get freshness => $_getN(2);
  @$pb.TagNumber(3)
  set freshness(NodeFreshness value) => $_setField(3, value);
  @$pb.TagNumber(3)
  $core.bool hasFreshness() => $_has(2);
  @$pb.TagNumber(3)
  void clearFreshness() => $_clearField(3);

  @$pb.TagNumber(4)
  $0.Timestamp get fetchedAt => $_getN(3);
  @$pb.TagNumber(4)
  set fetchedAt($0.Timestamp value) => $_setField(4, value);
  @$pb.TagNumber(4)
  $core.bool hasFetchedAt() => $_has(3);
  @$pb.TagNumber(4)
  void clearFetchedAt() => $_clearField(4);
  @$pb.TagNumber(4)
  $0.Timestamp ensureFetchedAt() => $_ensure(3);

  @$pb.TagNumber(5)
  $3.Duration get age => $_getN(4);
  @$pb.TagNumber(5)
  set age($3.Duration value) => $_setField(5, value);
  @$pb.TagNumber(5)
  $core.bool hasAge() => $_has(4);
  @$pb.TagNumber(5)
  void clearAge() => $_clearField(5);
  @$pb.TagNumber(5)
  $3.Duration ensureAge() => $_ensure(4);

  /// error is the failure of the most recent poll, empty if it succeeded.
  @$pb.TagNumber(6)
  $core.String get error => $_getSZ(5);
  @$pb.TagNumber(6)
  set error($core.String value) => $_setString(5, value);
  @$pb.TagNumber(6)
  $core.bool hasError() => $_has(5);
  @$pb.TagNumber(6)
  void clearError() => $_clearField(6);

  @$pb.TagNumber(7)
  $pb.PbList<NodeAlert> get alerts => $_getList(6);

  /// draining marks a node being removed: no new jobs or leases go to it.
  @$pb.TagNumber(11)
  $core.bool get draining => $_getBF(7);
  @$pb.TagNumber(11)
  set draining($core.bool value) => $_setBool(7, value);
  @$pb.TagNumber(11)
  $core.bool hasDraining() => $_has(7);
  @$pb.TagNumber(11)
  void clearDraining() => $_clearField(11);

  /// busy_gpus are the node's GPUs that something outside protectorate's
  /// leases is using, as the hub judges from the node's recent polls:
  /// automatic placement skips them, explicit requests are refused.
  @$pb.TagNumber(12)
  $pb.PbList<GpuBusy> get busyGpus => $_getList(8);
}

class GpuBusy extends $pb.GeneratedMessage {
  factory GpuBusy({
    $core.int? index,
    $core.String? reason,
  }) {
    final result = GpuBusy._();
    if (index != null) result.index = index;
    if (reason != null) result.reason = reason;
    return result;
  }

  GpuBusy._();

  factory GpuBusy.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      GpuBusy()..mergeFromBuffer(data, registry);
  factory GpuBusy.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      GpuBusy()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'GpuBusy',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: GpuBusy.$_createMessage)
    ..aI(1, _omitFieldNames ? '' : 'index')
    ..aOS(2, _omitFieldNames ? '' : 'reason')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  GpuBusy clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  GpuBusy copyWith(void Function(GpuBusy) updates) =>
      super.copyWith((message) => updates(message as GpuBusy)) as GpuBusy;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use GpuBusy() / GpuBusy.new instead')
  static GpuBusy create() => GpuBusy._();
  static $pb.GeneratedMessage $_createMessage() => GpuBusy._();
  @$core.override
  GpuBusy createEmptyInstance() => GpuBusy._();
  @$core.pragma('dart2js:noInline')
  static GpuBusy getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<GpuBusy>(GpuBusy.$_createMessage);
  static GpuBusy? _defaultInstance;

  /// index is GpuMetrics.index.
  @$pb.TagNumber(1)
  $core.int get index => $_getIZ(0);
  @$pb.TagNumber(1)
  set index($core.int value) => $_setSignedInt32(0, value);
  @$pb.TagNumber(1)
  $core.bool hasIndex() => $_has(0);
  @$pb.TagNumber(1)
  void clearIndex() => $_clearField(1);

  /// reason says what makes it busy, e.g. "92% of its VRAM is in use".
  @$pb.TagNumber(2)
  $core.String get reason => $_getSZ(1);
  @$pb.TagNumber(2)
  set reason($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasReason() => $_has(1);
  @$pb.TagNumber(2)
  void clearReason() => $_clearField(2);
}

class ListNodesRequest extends $pb.GeneratedMessage {
  factory ListNodesRequest() => ListNodesRequest._();

  ListNodesRequest._();

  factory ListNodesRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListNodesRequest()..mergeFromBuffer(data, registry);
  factory ListNodesRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListNodesRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ListNodesRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ListNodesRequest.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListNodesRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListNodesRequest copyWith(void Function(ListNodesRequest) updates) =>
      super.copyWith((message) => updates(message as ListNodesRequest))
          as ListNodesRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use ListNodesRequest() / ListNodesRequest.new instead')
  static ListNodesRequest create() => ListNodesRequest._();
  static $pb.GeneratedMessage $_createMessage() => ListNodesRequest._();
  @$core.override
  ListNodesRequest createEmptyInstance() => ListNodesRequest._();
  @$core.pragma('dart2js:noInline')
  static ListNodesRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<ListNodesRequest>(
          ListNodesRequest.$_createMessage);
  static ListNodesRequest? _defaultInstance;
}

class ListNodesResponse extends $pb.GeneratedMessage {
  factory ListNodesResponse({
    $core.Iterable<NodeStatus>? nodes,
  }) {
    final result = ListNodesResponse._();
    if (nodes != null) result.nodes.addAll(nodes);
    return result;
  }

  ListNodesResponse._();

  factory ListNodesResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListNodesResponse()..mergeFromBuffer(data, registry);
  factory ListNodesResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListNodesResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ListNodesResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ListNodesResponse.$_createMessage)
    ..pPM<NodeStatus>(1, _omitFieldNames ? '' : 'nodes',
        subBuilder: NodeStatus.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListNodesResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListNodesResponse copyWith(void Function(ListNodesResponse) updates) =>
      super.copyWith((message) => updates(message as ListNodesResponse))
          as ListNodesResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use ListNodesResponse() / ListNodesResponse.new instead')
  static ListNodesResponse create() => ListNodesResponse._();
  static $pb.GeneratedMessage $_createMessage() => ListNodesResponse._();
  @$core.override
  ListNodesResponse createEmptyInstance() => ListNodesResponse._();
  @$core.pragma('dart2js:noInline')
  static ListNodesResponse getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<ListNodesResponse>(
          ListNodesResponse.$_createMessage);
  static ListNodesResponse? _defaultInstance;

  @$pb.TagNumber(1)
  $pb.PbList<NodeStatus> get nodes => $_getList(0);
}

class GetNodeRequest extends $pb.GeneratedMessage {
  factory GetNodeRequest({
    $core.String? name,
  }) {
    final result = GetNodeRequest._();
    if (name != null) result.name = name;
    return result;
  }

  GetNodeRequest._();

  factory GetNodeRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      GetNodeRequest()..mergeFromBuffer(data, registry);
  factory GetNodeRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      GetNodeRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'GetNodeRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: GetNodeRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'name')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  GetNodeRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  GetNodeRequest copyWith(void Function(GetNodeRequest) updates) =>
      super.copyWith((message) => updates(message as GetNodeRequest))
          as GetNodeRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use GetNodeRequest() / GetNodeRequest.new instead')
  static GetNodeRequest create() => GetNodeRequest._();
  static $pb.GeneratedMessage $_createMessage() => GetNodeRequest._();
  @$core.override
  GetNodeRequest createEmptyInstance() => GetNodeRequest._();
  @$core.pragma('dart2js:noInline')
  static GetNodeRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<GetNodeRequest>(
          GetNodeRequest.$_createMessage);
  static GetNodeRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get name => $_getSZ(0);
  @$pb.TagNumber(1)
  set name($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasName() => $_has(0);
  @$pb.TagNumber(1)
  void clearName() => $_clearField(1);
}

class AddNodeRequest extends $pb.GeneratedMessage {
  factory AddNodeRequest({
    NodeInfo? node,
    $core.bool? update,
  }) {
    final result = AddNodeRequest._();
    if (node != null) result.node = node;
    if (update != null) result.update = update;
    return result;
  }

  AddNodeRequest._();

  factory AddNodeRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      AddNodeRequest()..mergeFromBuffer(data, registry);
  factory AddNodeRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      AddNodeRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'AddNodeRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: AddNodeRequest.$_createMessage)
    ..aOM<NodeInfo>(1, _omitFieldNames ? '' : 'node',
        subBuilder: NodeInfo.$_createMessage)
    ..aOB(2, _omitFieldNames ? '' : 'update')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  AddNodeRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  AddNodeRequest copyWith(void Function(AddNodeRequest) updates) =>
      super.copyWith((message) => updates(message as AddNodeRequest))
          as AddNodeRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use AddNodeRequest() / AddNodeRequest.new instead')
  static AddNodeRequest create() => AddNodeRequest._();
  static $pb.GeneratedMessage $_createMessage() => AddNodeRequest._();
  @$core.override
  AddNodeRequest createEmptyInstance() => AddNodeRequest._();
  @$core.pragma('dart2js:noInline')
  static AddNodeRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<AddNodeRequest>(
          AddNodeRequest.$_createMessage);
  static AddNodeRequest? _defaultInstance;

  @$pb.TagNumber(1)
  NodeInfo get node => $_getN(0);
  @$pb.TagNumber(1)
  set node(NodeInfo value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasNode() => $_has(0);
  @$pb.TagNumber(1)
  void clearNode() => $_clearField(1);
  @$pb.TagNumber(1)
  NodeInfo ensureNode() => $_ensure(0);

  /// update replaces an existing record instead of failing with
  /// ALREADY_EXISTS; fields left empty keep their stored value.
  @$pb.TagNumber(2)
  $core.bool get update => $_getBF(1);
  @$pb.TagNumber(2)
  set update($core.bool value) => $_setBool(1, value);
  @$pb.TagNumber(2)
  $core.bool hasUpdate() => $_has(1);
  @$pb.TagNumber(2)
  void clearUpdate() => $_clearField(2);
}

class RemoveNodeRequest extends $pb.GeneratedMessage {
  factory RemoveNodeRequest({
    $core.String? name,
    RemoveJobs? jobs,
    $core.bool? uninstall,
    $3.Duration? waitTimeout,
  }) {
    final result = RemoveNodeRequest._();
    if (name != null) result.name = name;
    if (jobs != null) result.jobs = jobs;
    if (uninstall != null) result.uninstall = uninstall;
    if (waitTimeout != null) result.waitTimeout = waitTimeout;
    return result;
  }

  RemoveNodeRequest._();

  factory RemoveNodeRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      RemoveNodeRequest()..mergeFromBuffer(data, registry);
  factory RemoveNodeRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      RemoveNodeRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'RemoveNodeRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: RemoveNodeRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'name')
    ..aE<RemoveJobs>(2, _omitFieldNames ? '' : 'jobs',
        enumValues: RemoveJobs.values)
    ..aOB(3, _omitFieldNames ? '' : 'uninstall')
    ..aOM<$3.Duration>(4, _omitFieldNames ? '' : 'waitTimeout',
        subBuilder: $3.Duration.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  RemoveNodeRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  RemoveNodeRequest copyWith(void Function(RemoveNodeRequest) updates) =>
      super.copyWith((message) => updates(message as RemoveNodeRequest))
          as RemoveNodeRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use RemoveNodeRequest() / RemoveNodeRequest.new instead')
  static RemoveNodeRequest create() => RemoveNodeRequest._();
  static $pb.GeneratedMessage $_createMessage() => RemoveNodeRequest._();
  @$core.override
  RemoveNodeRequest createEmptyInstance() => RemoveNodeRequest._();
  @$core.pragma('dart2js:noInline')
  static RemoveNodeRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<RemoveNodeRequest>(
          RemoveNodeRequest.$_createMessage);
  static RemoveNodeRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get name => $_getSZ(0);
  @$pb.TagNumber(1)
  set name($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasName() => $_has(0);
  @$pb.TagNumber(1)
  void clearName() => $_clearField(1);

  @$pb.TagNumber(2)
  RemoveJobs get jobs => $_getN(1);
  @$pb.TagNumber(2)
  set jobs(RemoveJobs value) => $_setField(2, value);
  @$pb.TagNumber(2)
  $core.bool hasJobs() => $_has(1);
  @$pb.TagNumber(2)
  void clearJobs() => $_clearField(2);

  /// uninstall asks a reachable agent to unregister its startup and stop
  /// (UninstallResponse); an unreachable one is skipped with a message.
  @$pb.TagNumber(3)
  $core.bool get uninstall => $_getBF(2);
  @$pb.TagNumber(3)
  set uninstall($core.bool value) => $_setBool(2, value);
  @$pb.TagNumber(3)
  $core.bool hasUninstall() => $_has(2);
  @$pb.TagNumber(3)
  void clearUninstall() => $_clearField(3);

  /// wait_timeout bounds REMOVE_JOBS_WAIT; unset means 1h.
  @$pb.TagNumber(4)
  $3.Duration get waitTimeout => $_getN(3);
  @$pb.TagNumber(4)
  set waitTimeout($3.Duration value) => $_setField(4, value);
  @$pb.TagNumber(4)
  $core.bool hasWaitTimeout() => $_has(3);
  @$pb.TagNumber(4)
  void clearWaitTimeout() => $_clearField(4);
  @$pb.TagNumber(4)
  $3.Duration ensureWaitTimeout() => $_ensure(3);
}

class RemoveNodeProgress extends $pb.GeneratedMessage {
  factory RemoveNodeProgress({
    RemovePhase? phase,
    $core.int? runningJobs,
    $core.String? message,
  }) {
    final result = RemoveNodeProgress._();
    if (phase != null) result.phase = phase;
    if (runningJobs != null) result.runningJobs = runningJobs;
    if (message != null) result.message = message;
    return result;
  }

  RemoveNodeProgress._();

  factory RemoveNodeProgress.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      RemoveNodeProgress()..mergeFromBuffer(data, registry);
  factory RemoveNodeProgress.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      RemoveNodeProgress()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'RemoveNodeProgress',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: RemoveNodeProgress.$_createMessage)
    ..aE<RemovePhase>(1, _omitFieldNames ? '' : 'phase',
        enumValues: RemovePhase.values)
    ..aI(2, _omitFieldNames ? '' : 'runningJobs',
        fieldType: $pb.PbFieldType.OU3)
    ..aOS(3, _omitFieldNames ? '' : 'message')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  RemoveNodeProgress clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  RemoveNodeProgress copyWith(void Function(RemoveNodeProgress) updates) =>
      super.copyWith((message) => updates(message as RemoveNodeProgress))
          as RemoveNodeProgress;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use RemoveNodeProgress() / RemoveNodeProgress.new instead')
  static RemoveNodeProgress create() => RemoveNodeProgress._();
  static $pb.GeneratedMessage $_createMessage() => RemoveNodeProgress._();
  @$core.override
  RemoveNodeProgress createEmptyInstance() => RemoveNodeProgress._();
  @$core.pragma('dart2js:noInline')
  static RemoveNodeProgress getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<RemoveNodeProgress>(
          RemoveNodeProgress.$_createMessage);
  static RemoveNodeProgress? _defaultInstance;

  @$pb.TagNumber(1)
  RemovePhase get phase => $_getN(0);
  @$pb.TagNumber(1)
  set phase(RemovePhase value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasPhase() => $_has(0);
  @$pb.TagNumber(1)
  void clearPhase() => $_clearField(1);

  @$pb.TagNumber(2)
  $core.int get runningJobs => $_getIZ(1);
  @$pb.TagNumber(2)
  set runningJobs($core.int value) => $_setUnsignedInt32(1, value);
  @$pb.TagNumber(2)
  $core.bool hasRunningJobs() => $_has(1);
  @$pb.TagNumber(2)
  void clearRunningJobs() => $_clearField(2);

  /// message is a human line about the step.
  @$pb.TagNumber(3)
  $core.String get message => $_getSZ(2);
  @$pb.TagNumber(3)
  set message($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasMessage() => $_has(2);
  @$pb.TagNumber(3)
  void clearMessage() => $_clearField(3);
}

class UninstallRequest extends $pb.GeneratedMessage {
  factory UninstallRequest() => UninstallRequest._();

  UninstallRequest._();

  factory UninstallRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      UninstallRequest()..mergeFromBuffer(data, registry);
  factory UninstallRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      UninstallRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'UninstallRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: UninstallRequest.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  UninstallRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  UninstallRequest copyWith(void Function(UninstallRequest) updates) =>
      super.copyWith((message) => updates(message as UninstallRequest))
          as UninstallRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use UninstallRequest() / UninstallRequest.new instead')
  static UninstallRequest create() => UninstallRequest._();
  static $pb.GeneratedMessage $_createMessage() => UninstallRequest._();
  @$core.override
  UninstallRequest createEmptyInstance() => UninstallRequest._();
  @$core.pragma('dart2js:noInline')
  static UninstallRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<UninstallRequest>(
          UninstallRequest.$_createMessage);
  static UninstallRequest? _defaultInstance;
}

class UninstallResponse extends $pb.GeneratedMessage {
  factory UninstallResponse({
    $core.Iterable<$core.String>? actions,
    $core.Iterable<$core.String>? left,
  }) {
    final result = UninstallResponse._();
    if (actions != null) result.actions.addAll(actions);
    if (left != null) result.left.addAll(left);
    return result;
  }

  UninstallResponse._();

  factory UninstallResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      UninstallResponse()..mergeFromBuffer(data, registry);
  factory UninstallResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      UninstallResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'UninstallResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: UninstallResponse.$_createMessage)
    ..pPS(1, _omitFieldNames ? '' : 'actions')
    ..pPS(2, _omitFieldNames ? '' : 'left')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  UninstallResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  UninstallResponse copyWith(void Function(UninstallResponse) updates) =>
      super.copyWith((message) => updates(message as UninstallResponse))
          as UninstallResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use UninstallResponse() / UninstallResponse.new instead')
  static UninstallResponse create() => UninstallResponse._();
  static $pb.GeneratedMessage $_createMessage() => UninstallResponse._();
  @$core.override
  UninstallResponse createEmptyInstance() => UninstallResponse._();
  @$core.pragma('dart2js:noInline')
  static UninstallResponse getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<UninstallResponse>(
          UninstallResponse.$_createMessage);
  static UninstallResponse? _defaultInstance;

  /// actions are the steps the agent takes after answering.
  @$pb.TagNumber(1)
  $pb.PbList<$core.String> get actions => $_getList(0);

  /// left lists files and directories the user may delete.
  @$pb.TagNumber(2)
  $pb.PbList<$core.String> get left => $_getList(1);
}

class InviteNodeRequest extends $pb.GeneratedMessage {
  factory InviteNodeRequest({
    $core.String? name,
    $core.String? tier,
    $core.Iterable<$core.String>? pools,
    $3.Duration? ttl,
    $core.String? installUrl,
  }) {
    final result = InviteNodeRequest._();
    if (name != null) result.name = name;
    if (tier != null) result.tier = tier;
    if (pools != null) result.pools.addAll(pools);
    if (ttl != null) result.ttl = ttl;
    if (installUrl != null) result.installUrl = installUrl;
    return result;
  }

  InviteNodeRequest._();

  factory InviteNodeRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      InviteNodeRequest()..mergeFromBuffer(data, registry);
  factory InviteNodeRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      InviteNodeRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'InviteNodeRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: InviteNodeRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'name')
    ..aOS(2, _omitFieldNames ? '' : 'tier')
    ..pPS(3, _omitFieldNames ? '' : 'pools')
    ..aOM<$3.Duration>(4, _omitFieldNames ? '' : 'ttl',
        subBuilder: $3.Duration.$_createMessage)
    ..aOS(5, _omitFieldNames ? '' : 'installUrl')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  InviteNodeRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  InviteNodeRequest copyWith(void Function(InviteNodeRequest) updates) =>
      super.copyWith((message) => updates(message as InviteNodeRequest))
          as InviteNodeRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use InviteNodeRequest() / InviteNodeRequest.new instead')
  static InviteNodeRequest create() => InviteNodeRequest._();
  static $pb.GeneratedMessage $_createMessage() => InviteNodeRequest._();
  @$core.override
  InviteNodeRequest createEmptyInstance() => InviteNodeRequest._();
  @$core.pragma('dart2js:noInline')
  static InviteNodeRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<InviteNodeRequest>(
          InviteNodeRequest.$_createMessage);
  static InviteNodeRequest? _defaultInstance;

  /// name must be a lowercase DNS label (a-z, 0-9, '-'), at most 63 long.
  @$pb.TagNumber(1)
  $core.String get name => $_getSZ(0);
  @$pb.TagNumber(1)
  set name($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasName() => $_has(0);
  @$pb.TagNumber(1)
  void clearName() => $_clearField(1);

  /// tier is "shared" (default) or "care".
  @$pb.TagNumber(2)
  $core.String get tier => $_getSZ(1);
  @$pb.TagNumber(2)
  set tier($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasTier() => $_has(1);
  @$pb.TagNumber(2)
  void clearTier() => $_clearField(2);

  /// pools become pool-<name> tags on the node.
  @$pb.TagNumber(3)
  $pb.PbList<$core.String> get pools => $_getList(2);

  /// ttl is how long the code can be redeemed; unset means 1h, at most 24h.
  @$pb.TagNumber(4)
  $3.Duration get ttl => $_getN(3);
  @$pb.TagNumber(4)
  set ttl($3.Duration value) => $_setField(4, value);
  @$pb.TagNumber(4)
  $core.bool hasTtl() => $_has(3);
  @$pb.TagNumber(4)
  void clearTtl() => $_clearField(4);
  @$pb.TagNumber(4)
  $3.Duration ensureTtl() => $_ensure(3);

  /// install_url is the hub's HTTPS URL the new machine downloads from and
  /// joins through. Empty means the hub's --public-url, else its first TLS
  /// listener.
  @$pb.TagNumber(5)
  $core.String get installUrl => $_getSZ(4);
  @$pb.TagNumber(5)
  set installUrl($core.String value) => $_setString(4, value);
  @$pb.TagNumber(5)
  $core.bool hasInstallUrl() => $_has(4);
  @$pb.TagNumber(5)
  void clearInstallUrl() => $_clearField(5);
}

class InstallCommand extends $pb.GeneratedMessage {
  factory InstallCommand({
    InstallPlatform? platform,
    $core.String? shell,
    $core.String? command,
    $core.String? note,
  }) {
    final result = InstallCommand._();
    if (platform != null) result.platform = platform;
    if (shell != null) result.shell = shell;
    if (command != null) result.command = command;
    if (note != null) result.note = note;
    return result;
  }

  InstallCommand._();

  factory InstallCommand.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      InstallCommand()..mergeFromBuffer(data, registry);
  factory InstallCommand.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      InstallCommand()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'InstallCommand',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: InstallCommand.$_createMessage)
    ..aE<InstallPlatform>(1, _omitFieldNames ? '' : 'platform',
        enumValues: InstallPlatform.values)
    ..aOS(2, _omitFieldNames ? '' : 'shell')
    ..aOS(3, _omitFieldNames ? '' : 'command')
    ..aOS(4, _omitFieldNames ? '' : 'note')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  InstallCommand clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  InstallCommand copyWith(void Function(InstallCommand) updates) =>
      super.copyWith((message) => updates(message as InstallCommand))
          as InstallCommand;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use InstallCommand() / InstallCommand.new instead')
  static InstallCommand create() => InstallCommand._();
  static $pb.GeneratedMessage $_createMessage() => InstallCommand._();
  @$core.override
  InstallCommand createEmptyInstance() => InstallCommand._();
  @$core.pragma('dart2js:noInline')
  static InstallCommand getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<InstallCommand>(
          InstallCommand.$_createMessage);
  static InstallCommand? _defaultInstance;

  @$pb.TagNumber(1)
  InstallPlatform get platform => $_getN(0);
  @$pb.TagNumber(1)
  set platform(InstallPlatform value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasPlatform() => $_has(0);
  @$pb.TagNumber(1)
  void clearPlatform() => $_clearField(1);

  /// shell is where to paste command, e.g. "PowerShell".
  @$pb.TagNumber(2)
  $core.String get shell => $_getSZ(1);
  @$pb.TagNumber(2)
  set shell($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasShell() => $_has(1);
  @$pb.TagNumber(2)
  void clearShell() => $_clearField(2);

  /// command is one line that fetches the hub CA, checks its SHA-256, then
  /// downloads the hub's installer over TLS pinned to that CA and runs it
  /// with the code. It holds the code: treat it like a password.
  @$pb.TagNumber(3)
  $core.String get command => $_getSZ(2);
  @$pb.TagNumber(3)
  set command($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasCommand() => $_has(2);
  @$pb.TagNumber(3)
  void clearCommand() => $_clearField(3);

  @$pb.TagNumber(4)
  $core.String get note => $_getSZ(3);
  @$pb.TagNumber(4)
  set note($core.String value) => $_setString(3, value);
  @$pb.TagNumber(4)
  $core.bool hasNote() => $_has(3);
  @$pb.TagNumber(4)
  void clearNote() => $_clearField(4);
}

class InviteNodeResponse extends $pb.GeneratedMessage {
  factory InviteNodeResponse({
    NodeInfo? node,
    $core.String? hubUrl,
    $core.String? code,
    $0.Timestamp? expiresAt,
    $core.String? installUrl,
    $core.Iterable<InstallCommand>? commands,
    $core.String? caSha256,
  }) {
    final result = InviteNodeResponse._();
    if (node != null) result.node = node;
    if (hubUrl != null) result.hubUrl = hubUrl;
    if (code != null) result.code = code;
    if (expiresAt != null) result.expiresAt = expiresAt;
    if (installUrl != null) result.installUrl = installUrl;
    if (commands != null) result.commands.addAll(commands);
    if (caSha256 != null) result.caSha256 = caSha256;
    return result;
  }

  InviteNodeResponse._();

  factory InviteNodeResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      InviteNodeResponse()..mergeFromBuffer(data, registry);
  factory InviteNodeResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      InviteNodeResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'InviteNodeResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: InviteNodeResponse.$_createMessage)
    ..aOM<NodeInfo>(1, _omitFieldNames ? '' : 'node',
        subBuilder: NodeInfo.$_createMessage)
    ..aOS(2, _omitFieldNames ? '' : 'hubUrl')
    ..aOS(3, _omitFieldNames ? '' : 'code')
    ..aOM<$0.Timestamp>(4, _omitFieldNames ? '' : 'expiresAt',
        subBuilder: $0.Timestamp.$_createMessage)
    ..aOS(5, _omitFieldNames ? '' : 'installUrl')
    ..pPM<InstallCommand>(6, _omitFieldNames ? '' : 'commands',
        subBuilder: InstallCommand.$_createMessage)
    ..aOS(7, _omitFieldNames ? '' : 'caSha256')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  InviteNodeResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  InviteNodeResponse copyWith(void Function(InviteNodeResponse) updates) =>
      super.copyWith((message) => updates(message as InviteNodeResponse))
          as InviteNodeResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use InviteNodeResponse() / InviteNodeResponse.new instead')
  static InviteNodeResponse create() => InviteNodeResponse._();
  static $pb.GeneratedMessage $_createMessage() => InviteNodeResponse._();
  @$core.override
  InviteNodeResponse createEmptyInstance() => InviteNodeResponse._();
  @$core.pragma('dart2js:noInline')
  static InviteNodeResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<InviteNodeResponse>(
          InviteNodeResponse.$_createMessage);
  static InviteNodeResponse? _defaultInstance;

  @$pb.TagNumber(1)
  NodeInfo get node => $_getN(0);
  @$pb.TagNumber(1)
  set node(NodeInfo value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasNode() => $_has(0);
  @$pb.TagNumber(1)
  void clearNode() => $_clearField(1);
  @$pb.TagNumber(1)
  NodeInfo ensureNode() => $_ensure(0);

  /// hub_url, code and ca_sha256 are for the manual path
  /// (`node install --hub URL --code CODE --ca-cert FILE`).
  @$pb.TagNumber(2)
  $core.String get hubUrl => $_getSZ(1);
  @$pb.TagNumber(2)
  set hubUrl($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasHubUrl() => $_has(1);
  @$pb.TagNumber(2)
  void clearHubUrl() => $_clearField(2);

  @$pb.TagNumber(3)
  $core.String get code => $_getSZ(2);
  @$pb.TagNumber(3)
  set code($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasCode() => $_has(2);
  @$pb.TagNumber(3)
  void clearCode() => $_clearField(3);

  @$pb.TagNumber(4)
  $0.Timestamp get expiresAt => $_getN(3);
  @$pb.TagNumber(4)
  set expiresAt($0.Timestamp value) => $_setField(4, value);
  @$pb.TagNumber(4)
  $core.bool hasExpiresAt() => $_has(3);
  @$pb.TagNumber(4)
  void clearExpiresAt() => $_clearField(4);
  @$pb.TagNumber(4)
  $0.Timestamp ensureExpiresAt() => $_ensure(3);

  @$pb.TagNumber(5)
  $core.String get installUrl => $_getSZ(4);
  @$pb.TagNumber(5)
  set installUrl($core.String value) => $_setString(4, value);
  @$pb.TagNumber(5)
  $core.bool hasInstallUrl() => $_has(4);
  @$pb.TagNumber(5)
  void clearInstallUrl() => $_clearField(5);

  @$pb.TagNumber(6)
  $pb.PbList<InstallCommand> get commands => $_getList(5);

  /// ca_sha256 is the hex SHA-256 of the hub CA certificate PEM served at
  /// /ca.pem.
  @$pb.TagNumber(7)
  $core.String get caSha256 => $_getSZ(6);
  @$pb.TagNumber(7)
  set caSha256($core.String value) => $_setString(6, value);
  @$pb.TagNumber(7)
  $core.bool hasCaSha256() => $_has(6);
  @$pb.TagNumber(7)
  void clearCaSha256() => $_clearField(7);
}

class JoinNodeRequest extends $pb.GeneratedMessage {
  factory JoinNodeRequest({
    $core.String? name,
    $core.String? code,
    $core.List<$core.int>? publicKey,
    $core.List<$core.int>? proof,
    $core.String? address,
  }) {
    final result = JoinNodeRequest._();
    if (name != null) result.name = name;
    if (code != null) result.code = code;
    if (publicKey != null) result.publicKey = publicKey;
    if (proof != null) result.proof = proof;
    if (address != null) result.address = address;
    return result;
  }

  JoinNodeRequest._();

  factory JoinNodeRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      JoinNodeRequest()..mergeFromBuffer(data, registry);
  factory JoinNodeRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      JoinNodeRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'JoinNodeRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: JoinNodeRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'name')
    ..aOS(2, _omitFieldNames ? '' : 'code')
    ..a<$core.List<$core.int>>(
        3, _omitFieldNames ? '' : 'publicKey', $pb.PbFieldType.OY)
    ..a<$core.List<$core.int>>(
        4, _omitFieldNames ? '' : 'proof', $pb.PbFieldType.OY)
    ..aOS(5, _omitFieldNames ? '' : 'address')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  JoinNodeRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  JoinNodeRequest copyWith(void Function(JoinNodeRequest) updates) =>
      super.copyWith((message) => updates(message as JoinNodeRequest))
          as JoinNodeRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use JoinNodeRequest() / JoinNodeRequest.new instead')
  static JoinNodeRequest create() => JoinNodeRequest._();
  static $pb.GeneratedMessage $_createMessage() => JoinNodeRequest._();
  @$core.override
  JoinNodeRequest createEmptyInstance() => JoinNodeRequest._();
  @$core.pragma('dart2js:noInline')
  static JoinNodeRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<JoinNodeRequest>(
          JoinNodeRequest.$_createMessage);
  static JoinNodeRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get name => $_getSZ(0);
  @$pb.TagNumber(1)
  set name($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasName() => $_has(0);
  @$pb.TagNumber(1)
  void clearName() => $_clearField(1);

  @$pb.TagNumber(2)
  $core.String get code => $_getSZ(1);
  @$pb.TagNumber(2)
  set code($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasCode() => $_has(1);
  @$pb.TagNumber(2)
  void clearCode() => $_clearField(2);

  /// public_key is the uncompressed P-256 point the node generated.
  @$pb.TagNumber(3)
  $core.List<$core.int> get publicKey => $_getN(2);
  @$pb.TagNumber(3)
  set publicKey($core.List<$core.int> value) => $_setBytes(2, value);
  @$pb.TagNumber(3)
  $core.bool hasPublicKey() => $_has(2);
  @$pb.TagNumber(3)
  void clearPublicKey() => $_clearField(3);

  /// proof is an ECDSA P-256 SHA-256 (ASN.1) signature over
  /// "protectorate-join:" + name + ":" + code made with that key.
  @$pb.TagNumber(4)
  $core.List<$core.int> get proof => $_getN(3);
  @$pb.TagNumber(4)
  set proof($core.List<$core.int> value) => $_setBytes(3, value);
  @$pb.TagNumber(4)
  $core.bool hasProof() => $_has(3);
  @$pb.TagNumber(4)
  void clearProof() => $_clearField(4);

  /// address is the private host:port the node agent listens on.
  @$pb.TagNumber(5)
  $core.String get address => $_getSZ(4);
  @$pb.TagNumber(5)
  set address($core.String value) => $_setString(4, value);
  @$pb.TagNumber(5)
  $core.bool hasAddress() => $_has(4);
  @$pb.TagNumber(5)
  void clearAddress() => $_clearField(5);
}

class JoinNodeResponse extends $pb.GeneratedMessage {
  factory JoinNodeResponse({
    $core.String? certificate,
    $core.String? ca,
  }) {
    final result = JoinNodeResponse._();
    if (certificate != null) result.certificate = certificate;
    if (ca != null) result.ca = ca;
    return result;
  }

  JoinNodeResponse._();

  factory JoinNodeResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      JoinNodeResponse()..mergeFromBuffer(data, registry);
  factory JoinNodeResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      JoinNodeResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'JoinNodeResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: JoinNodeResponse.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'certificate')
    ..aOS(2, _omitFieldNames ? '' : 'ca')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  JoinNodeResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  JoinNodeResponse copyWith(void Function(JoinNodeResponse) updates) =>
      super.copyWith((message) => updates(message as JoinNodeResponse))
          as JoinNodeResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use JoinNodeResponse() / JoinNodeResponse.new instead')
  static JoinNodeResponse create() => JoinNodeResponse._();
  static $pb.GeneratedMessage $_createMessage() => JoinNodeResponse._();
  @$core.override
  JoinNodeResponse createEmptyInstance() => JoinNodeResponse._();
  @$core.pragma('dart2js:noInline')
  static JoinNodeResponse getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<JoinNodeResponse>(
          JoinNodeResponse.$_createMessage);
  static JoinNodeResponse? _defaultInstance;

  /// certificate is the node's PEM certificate, valid for client and server
  /// use and naming the node.
  @$pb.TagNumber(1)
  $core.String get certificate => $_getSZ(0);
  @$pb.TagNumber(1)
  set certificate($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasCertificate() => $_has(0);
  @$pb.TagNumber(1)
  void clearCertificate() => $_clearField(1);

  /// ca is the hub CA certificate PEM, the only root the node trusts.
  @$pb.TagNumber(2)
  $core.String get ca => $_getSZ(1);
  @$pb.TagNumber(2)
  set ca($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasCa() => $_has(1);
  @$pb.TagNumber(2)
  void clearCa() => $_clearField(2);
}

class RenewCertificateRequest extends $pb.GeneratedMessage {
  factory RenewCertificateRequest({
    $core.String? certificate,
  }) {
    final result = RenewCertificateRequest._();
    if (certificate != null) result.certificate = certificate;
    return result;
  }

  RenewCertificateRequest._();

  factory RenewCertificateRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      RenewCertificateRequest()..mergeFromBuffer(data, registry);
  factory RenewCertificateRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      RenewCertificateRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'RenewCertificateRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: RenewCertificateRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'certificate')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  RenewCertificateRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  RenewCertificateRequest copyWith(
          void Function(RenewCertificateRequest) updates) =>
      super.copyWith((message) => updates(message as RenewCertificateRequest))
          as RenewCertificateRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use RenewCertificateRequest() / RenewCertificateRequest.new instead')
  static RenewCertificateRequest create() => RenewCertificateRequest._();
  static $pb.GeneratedMessage $_createMessage() => RenewCertificateRequest._();
  @$core.override
  RenewCertificateRequest createEmptyInstance() => RenewCertificateRequest._();
  @$core.pragma('dart2js:noInline')
  static RenewCertificateRequest getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<RenewCertificateRequest>(
          RenewCertificateRequest.$_createMessage);
  static RenewCertificateRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get certificate => $_getSZ(0);
  @$pb.TagNumber(1)
  set certificate($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasCertificate() => $_has(0);
  @$pb.TagNumber(1)
  void clearCertificate() => $_clearField(1);
}

class RenewCertificateResponse extends $pb.GeneratedMessage {
  factory RenewCertificateResponse() => RenewCertificateResponse._();

  RenewCertificateResponse._();

  factory RenewCertificateResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      RenewCertificateResponse()..mergeFromBuffer(data, registry);
  factory RenewCertificateResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      RenewCertificateResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'RenewCertificateResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: RenewCertificateResponse.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  RenewCertificateResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  RenewCertificateResponse copyWith(
          void Function(RenewCertificateResponse) updates) =>
      super.copyWith((message) => updates(message as RenewCertificateResponse))
          as RenewCertificateResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use RenewCertificateResponse() / RenewCertificateResponse.new instead')
  static RenewCertificateResponse create() => RenewCertificateResponse._();
  static $pb.GeneratedMessage $_createMessage() => RenewCertificateResponse._();
  @$core.override
  RenewCertificateResponse createEmptyInstance() =>
      RenewCertificateResponse._();
  @$core.pragma('dart2js:noInline')
  static RenewCertificateResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<RenewCertificateResponse>(
          RenewCertificateResponse.$_createMessage);
  static RenewCertificateResponse? _defaultInstance;
}

class WatchNodeSetupRequest extends $pb.GeneratedMessage {
  factory WatchNodeSetupRequest({
    $core.String? name,
  }) {
    final result = WatchNodeSetupRequest._();
    if (name != null) result.name = name;
    return result;
  }

  WatchNodeSetupRequest._();

  factory WatchNodeSetupRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      WatchNodeSetupRequest()..mergeFromBuffer(data, registry);
  factory WatchNodeSetupRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      WatchNodeSetupRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'WatchNodeSetupRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: WatchNodeSetupRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'name')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  WatchNodeSetupRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  WatchNodeSetupRequest copyWith(
          void Function(WatchNodeSetupRequest) updates) =>
      super.copyWith((message) => updates(message as WatchNodeSetupRequest))
          as WatchNodeSetupRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use WatchNodeSetupRequest() / WatchNodeSetupRequest.new instead')
  static WatchNodeSetupRequest create() => WatchNodeSetupRequest._();
  static $pb.GeneratedMessage $_createMessage() => WatchNodeSetupRequest._();
  @$core.override
  WatchNodeSetupRequest createEmptyInstance() => WatchNodeSetupRequest._();
  @$core.pragma('dart2js:noInline')
  static WatchNodeSetupRequest getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<WatchNodeSetupRequest>(
          WatchNodeSetupRequest.$_createMessage);
  static WatchNodeSetupRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get name => $_getSZ(0);
  @$pb.TagNumber(1)
  set name($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasName() => $_has(0);
  @$pb.TagNumber(1)
  void clearName() => $_clearField(1);
}

class NodeSetupProgress extends $pb.GeneratedMessage {
  factory NodeSetupProgress({
    NodeSetupStage? stage,
    $core.Iterable<$core.String>? problems,
    $core.Iterable<$core.String>? notes,
    $core.String? message,
    NodeStatus? status,
  }) {
    final result = NodeSetupProgress._();
    if (stage != null) result.stage = stage;
    if (problems != null) result.problems.addAll(problems);
    if (notes != null) result.notes.addAll(notes);
    if (message != null) result.message = message;
    if (status != null) result.status = status;
    return result;
  }

  NodeSetupProgress._();

  factory NodeSetupProgress.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      NodeSetupProgress()..mergeFromBuffer(data, registry);
  factory NodeSetupProgress.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      NodeSetupProgress()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'NodeSetupProgress',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: NodeSetupProgress.$_createMessage)
    ..aE<NodeSetupStage>(1, _omitFieldNames ? '' : 'stage',
        enumValues: NodeSetupStage.values)
    ..pPS(2, _omitFieldNames ? '' : 'problems')
    ..pPS(3, _omitFieldNames ? '' : 'notes')
    ..aOS(4, _omitFieldNames ? '' : 'message')
    ..aOM<NodeStatus>(5, _omitFieldNames ? '' : 'status',
        subBuilder: NodeStatus.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  NodeSetupProgress clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  NodeSetupProgress copyWith(void Function(NodeSetupProgress) updates) =>
      super.copyWith((message) => updates(message as NodeSetupProgress))
          as NodeSetupProgress;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use NodeSetupProgress() / NodeSetupProgress.new instead')
  static NodeSetupProgress create() => NodeSetupProgress._();
  static $pb.GeneratedMessage $_createMessage() => NodeSetupProgress._();
  @$core.override
  NodeSetupProgress createEmptyInstance() => NodeSetupProgress._();
  @$core.pragma('dart2js:noInline')
  static NodeSetupProgress getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<NodeSetupProgress>(
          NodeSetupProgress.$_createMessage);
  static NodeSetupProgress? _defaultInstance;

  @$pb.TagNumber(1)
  NodeSetupStage get stage => $_getN(0);
  @$pb.TagNumber(1)
  set stage(NodeSetupStage value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasStage() => $_has(0);
  @$pb.TagNumber(1)
  void clearStage() => $_clearField(1);

  /// problems are what keeps the node from READY, one sentence each.
  @$pb.TagNumber(2)
  $pb.PbList<$core.String> get problems => $_getList(1);

  /// notes are worth knowing but do not block READY.
  @$pb.TagNumber(3)
  $pb.PbList<$core.String> get notes => $_getList(2);

  /// message is a human line about the stage.
  @$pb.TagNumber(4)
  $core.String get message => $_getSZ(3);
  @$pb.TagNumber(4)
  set message($core.String value) => $_setString(3, value);
  @$pb.TagNumber(4)
  $core.bool hasMessage() => $_has(3);
  @$pb.TagNumber(4)
  void clearMessage() => $_clearField(4);

  @$pb.TagNumber(5)
  NodeStatus get status => $_getN(4);
  @$pb.TagNumber(5)
  set status(NodeStatus value) => $_setField(5, value);
  @$pb.TagNumber(5)
  $core.bool hasStatus() => $_has(4);
  @$pb.TagNumber(5)
  void clearStatus() => $_clearField(5);
  @$pb.TagNumber(5)
  NodeStatus ensureStatus() => $_ensure(4);
}

/// NodeAgentService runs on every node. The hub polls GetMetrics; WatchMetrics
/// is for live views. The job RPCs are called by the hub only and are unary
/// except RunJob, so they work over plain HTTP/1.1.
class NodeAgentServiceApi {
  final $pb.RpcClient _client;

  NodeAgentServiceApi(this._client);

  /// GetMetrics collects this node's current metrics.
  $async.Future<NodeMetrics> getMetrics(
          $pb.ClientContext? ctx, GetMetricsRequest request) =>
      _client.invoke<NodeMetrics>(
          ctx, 'NodeAgentService', 'GetMetrics', request, NodeMetrics());

  /// WatchMetrics streams this node's metrics at an interval.
  $async.Future<NodeMetrics> watchMetrics(
          $pb.ClientContext? ctx, WatchMetricsRequest request) =>
      _client.invoke<NodeMetrics>(
          ctx, 'NodeAgentService', 'WatchMetrics', request, NodeMetrics());

  /// PrepareJob tells the hub what the node already holds for a git job: whether
  /// it has the commit and which commits it knows, so the hub can send a
  /// smaller bundle.
  $async.Future<$2.PrepareJobResponse> prepareJob(
          $pb.ClientContext? ctx, $2.PrepareJobRequest request) =>
      _client.invoke<$2.PrepareJobResponse>(ctx, 'NodeAgentService',
          'PrepareJob', request, $2.PrepareJobResponse());

  /// UploadJobData appends one chunk of a job's source (a git bundle or an
  /// artifact file). Chunks must arrive in order; the last one carries the
  /// SHA-256 of the whole upload.
  $async.Future<$2.UploadJobDataResponse> uploadJobData(
          $pb.ClientContext? ctx, $2.UploadJobDataRequest request) =>
      _client.invoke<$2.UploadJobDataResponse>(ctx, 'NodeAgentService',
          'UploadJobData', request, $2.UploadJobDataResponse());

  /// RunJob checks the source out, runs the command and streams its output;
  /// matching artifacts follow the exit. The job does not depend on the
  /// stream: if the caller disconnects it keeps running (bounded by its
  /// timeout) and AttachJob picks the stream up again.
  $async.Future<$2.JobEvent> runJob(
          $pb.ClientContext? ctx, $2.RunJobRequest request) =>
      _client.invoke<$2.JobEvent>(
          ctx, 'NodeAgentService', 'RunJob', request, $2.JobEvent());

  /// AttachJob streams a job again from an output offset: started, the output
  /// from from_offset, then artifacts and the result once it ended. A finished
  /// job is kept for an hour. NOT_FOUND when the node never had the job.
  $async.Future<$2.JobEvent> attachJob(
          $pb.ClientContext? ctx, $2.AttachJobRequest request) =>
      _client.invoke<$2.JobEvent>(
          ctx, 'NodeAgentService', 'AttachJob', request, $2.JobEvent());

  /// ListJobs lists the jobs the node is running or still holds the result of.
  $async.Future<$2.NodeAgentServiceListJobsResponse> listJobs(
          $pb.ClientContext? ctx, $2.NodeAgentServiceListJobsRequest request) =>
      _client.invoke<$2.NodeAgentServiceListJobsResponse>(
          ctx,
          'NodeAgentService',
          'ListJobs',
          request,
          $2.NodeAgentServiceListJobsResponse());

  /// CancelJob kills a running job's process tree.
  $async.Future<$2.NodeAgentServiceCancelJobResponse> cancelJob(
          $pb.ClientContext? ctx,
          $2.NodeAgentServiceCancelJobRequest request) =>
      _client.invoke<$2.NodeAgentServiceCancelJobResponse>(
          ctx,
          'NodeAgentService',
          'CancelJob',
          request,
          $2.NodeAgentServiceCancelJobResponse());

  /// PutFile receives one chunk of a file or directory transfer into the
  /// node's file root. The hub relays the caller's chunks unchanged.
  $async.Future<PutFileResponse> putFile(
          $pb.ClientContext? ctx, PutFileRequest request) =>
      _client.invoke<PutFileResponse>(
          ctx, 'NodeAgentService', 'PutFile', request, PutFileResponse());

  /// GetFile streams a file or directory out of the node's file root.
  $async.Future<GetFileResponse> getFile(
          $pb.ClientContext? ctx, GetFileRequest request) =>
      _client.invoke<GetFileResponse>(
          ctx, 'NodeAgentService', 'GetFile', request, GetFileResponse());

  /// ApplyUpdate swaps the agent's binary for a file already transferred into
  /// its file root and restarts the agent. See NodeService.ApplyUpdate.
  $async.Future<ApplyUpdateResponse> applyUpdate(
          $pb.ClientContext? ctx, ApplyUpdateRequest request) =>
      _client.invoke<ApplyUpdateResponse>(ctx, 'NodeAgentService',
          'ApplyUpdate', request, ApplyUpdateResponse());

  /// Uninstall unregisters the agent's startup (the logon task, systemd user
  /// unit or termux service `node install` registered) and stops the agent
  /// once it has answered. It deletes nothing; the response names what is left.
  $async.Future<UninstallResponse> uninstall(
          $pb.ClientContext? ctx, UninstallRequest request) =>
      _client.invoke<UninstallResponse>(
          ctx, 'NodeAgentService', 'Uninstall', request, UninstallResponse());

  /// RenewCertificate swaps in a later certificate the hub CA issued for the
  /// node's own key and identity. The node checks both, stores it and serves
  /// it from the next handshake on.
  $async.Future<RenewCertificateResponse> renewCertificate(
          $pb.ClientContext? ctx, RenewCertificateRequest request) =>
      _client.invoke<RenewCertificateResponse>(ctx, 'NodeAgentService',
          'RenewCertificate', request, RenewCertificateResponse());
}

/// NodeService runs on the hub: the inventory plus the latest metrics of each
/// node. Polls are announced on EventService as "node.metrics" (subject = node
/// name, payload = summary), "node.unreachable", "node.joined" (an invited
/// node answered its first poll) and "node.name_mismatch".
class NodeServiceApi {
  final $pb.RpcClient _client;

  NodeServiceApi(this._client);

  /// ListNodes returns every node with its latest metrics summary,
  /// freshness and alerts.
  $async.Future<ListNodesResponse> listNodes(
          $pb.ClientContext? ctx, ListNodesRequest request) =>
      _client.invoke<ListNodesResponse>(
          ctx, 'NodeService', 'ListNodes', request, ListNodesResponse());

  /// GetNode returns one node with full metrics: filesystems, GPUs,
  /// containers, engine disk usage and watched directories.
  $async.Future<NodeStatus> getNode(
          $pb.ClientContext? ctx, GetNodeRequest request) =>
      _client.invoke<NodeStatus>(
          ctx, 'NodeService', 'GetNode', request, NodeStatus());

  /// AddNode adds or updates a node in the hub inventory, such as a joined
  /// node's address. Remote nodes are added with InviteNode and JoinNode: the
  /// hub reaches a node only over mutual TLS with the certificate it issued.
  /// Only the owner token may call it.
  $async.Future<NodeInfo> addNode(
          $pb.ClientContext? ctx, AddNodeRequest request) =>
      _client.invoke<NodeInfo>(
          ctx, 'NodeService', 'AddNode', request, NodeInfo());

  /// RemoveNode takes a node out of service and forgets it. It drains the
  /// node first (no new jobs or leases; running jobs are waited for or
  /// cancelled as jobs says), optionally asks the agent to uninstall itself,
  /// releases its leases and removes the inventory entry, which revokes the
  /// node's certificate and any unused enrollment code. It streams the steps
  /// and fails, leaving the node in service, if any step fails. A revoked
  /// machine that comes back cannot rejoin without a new invite.
  /// Cancelling a pending invite is the same call. Only the owner token may
  /// call it.
  $async.Future<RemoveNodeProgress> removeNode(
          $pb.ClientContext? ctx, RemoveNodeRequest request) =>
      _client.invoke<RemoveNodeProgress>(
          ctx, 'NodeService', 'RemoveNode', request, RemoveNodeProgress());

  /// InviteNode creates a single-use enrollment code for a new node, records
  /// the node in the inventory as invited and returns the one-line installers
  /// that use it. Inviting a pending node again replaces its previous code.
  /// Only the owner token may call it.
  $async.Future<InviteNodeResponse> inviteNode(
          $pb.ClientContext? ctx, InviteNodeRequest request) =>
      _client.invoke<InviteNodeResponse>(
          ctx, 'NodeService', 'InviteNode', request, InviteNodeResponse());

  /// JoinNode redeems an InviteNode code from the new machine. The node sends
  /// the public half of a key it generated and a signature over the code made
  /// with that key; the hub CA returns a certificate for the key that names
  /// the node. No token is needed: the single-use code is the credential.
  $async.Future<JoinNodeResponse> joinNode(
          $pb.ClientContext? ctx, JoinNodeRequest request) =>
      _client.invoke<JoinNodeResponse>(
          ctx, 'NodeService', 'JoinNode', request, JoinNodeResponse());

  /// WatchNodeSetup follows a node from its invite to ready. It sends the
  /// current stage at once and again whenever the stage or its problems
  /// change, and ends after READY.
  $async.Future<NodeSetupProgress> watchNodeSetup(
          $pb.ClientContext? ctx, WatchNodeSetupRequest request) =>
      _client.invoke<NodeSetupProgress>(
          ctx, 'NodeService', 'WatchNodeSetup', request, NodeSetupProgress());

  /// PutFile copies a file or directory to a node, one chunk per call so it
  /// works over plain HTTP/1.1. A transfer is a sequence of calls sharing a
  /// transfer_id with consecutive offsets; the call with last set carries the
  /// size and SHA-256 of the whole (uncompressed) stream, and only then is the
  /// destination replaced, atomically. The hub relays chunks without storing
  /// them. The remote path is confined to the node's file root unless the
  /// node agent was started unconfined.
  $async.Future<PutFileResponse> putFile(
          $pb.ClientContext? ctx, PutFileRequest request) =>
      _client.invoke<PutFileResponse>(
          ctx, 'NodeService', 'PutFile', request, PutFileResponse());

  /// GetFile streams a file or directory from a node: a header, chunks and a
  /// trailer with the size and SHA-256 the receiver must verify.
  $async.Future<GetFileResponse> getFile(
          $pb.ClientContext? ctx, GetFileRequest request) =>
      _client.invoke<GetFileResponse>(
          ctx, 'NodeService', 'GetFile', request, GetFileResponse());

  /// ApplyUpdate installs a new agent binary that PutFile placed in the node's
  /// file root. The node verifies size and SHA-256, runs the file's
  /// `version --json`, swaps it in keeping exactly one previous binary
  /// (<exe>.old) and restarts itself through its startup registration, or by
  /// re-executing. It answers before restarting. The new binary is on trial:
  /// unless the hub reaches it, the node rolls back to the previous binary.
  /// Only the owner token may call it. The hub's own node cannot be updated
  /// this way.
  $async.Future<ApplyUpdateResponse> applyUpdate(
          $pb.ClientContext? ctx, ApplyUpdateRequest request) =>
      _client.invoke<ApplyUpdateResponse>(
          ctx, 'NodeService', 'ApplyUpdate', request, ApplyUpdateResponse());

  /// UpdateNode brings a node to the hub's version: the hub sends the node
  /// the binary for its OS and architecture (its own executable for the hub's
  /// platform, otherwise protectorate-<os>-<arch> from its binary dir), has
  /// the node verify and swap it in as ApplyUpdate does, then watches until
  /// the node reports the new version or puts the old binary back. It streams
  /// the progress and ends when the update is confirmed, rolled back or
  /// skipped (the node is already at the hub's version and force is unset);
  /// any failure ends the stream with an error. Only the owner token may call
  /// it. The hub's own node cannot be updated this way.
  $async.Future<UpdateNodeProgress> updateNode(
          $pb.ClientContext? ctx, UpdateNodeRequest request) =>
      _client.invoke<UpdateNodeProgress>(
          ctx, 'NodeService', 'UpdateNode', request, UpdateNodeProgress());

  /// SetNodeTier changes a node's tier in the inventory and announces
  /// "node.tier". Only the owner token may call it. The hub's own node is
  /// always "shared".
  $async.Future<NodeInfo> setNodeTier(
          $pb.ClientContext? ctx, SetNodeTierRequest request) =>
      _client.invoke<NodeInfo>(
          ctx, 'NodeService', 'SetNodeTier', request, NodeInfo());
}

const $core.bool _omitFieldNames =
    $core.bool.fromEnvironment('protobuf.omit_field_names');
const $core.bool _omitMessageNames =
    $core.bool.fromEnvironment('protobuf.omit_message_names');
