// This is a generated file - do not edit.
//
// Generated from protectorate/v1/release.proto.

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

import 'release.pbenum.dart';

export 'package:protobuf/protobuf.dart' show GeneratedMessageGenericExtensions;

export 'release.pbenum.dart';

class CheckReleaseRequest extends $pb.GeneratedMessage {
  factory CheckReleaseRequest({
    $core.String? channel,
    $core.String? version,
  }) {
    final result = CheckReleaseRequest._();
    if (channel != null) result.channel = channel;
    if (version != null) result.version = version;
    return result;
  }

  CheckReleaseRequest._();

  factory CheckReleaseRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      CheckReleaseRequest()..mergeFromBuffer(data, registry);
  factory CheckReleaseRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      CheckReleaseRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'CheckReleaseRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: CheckReleaseRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'channel')
    ..aOS(2, _omitFieldNames ? '' : 'version')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  CheckReleaseRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  CheckReleaseRequest copyWith(void Function(CheckReleaseRequest) updates) =>
      super.copyWith((message) => updates(message as CheckReleaseRequest))
          as CheckReleaseRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core
      .Deprecated('Use CheckReleaseRequest() / CheckReleaseRequest.new instead')
  static CheckReleaseRequest create() => CheckReleaseRequest._();
  static $pb.GeneratedMessage $_createMessage() => CheckReleaseRequest._();
  @$core.override
  CheckReleaseRequest createEmptyInstance() => CheckReleaseRequest._();
  @$core.pragma('dart2js:noInline')
  static CheckReleaseRequest getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<CheckReleaseRequest>(
          CheckReleaseRequest.$_createMessage);
  static CheckReleaseRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get channel => $_getSZ(0);
  @$pb.TagNumber(1)
  set channel($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasChannel() => $_has(0);
  @$pb.TagNumber(1)
  void clearChannel() => $_clearField(1);

  @$pb.TagNumber(2)
  $core.String get version => $_getSZ(1);
  @$pb.TagNumber(2)
  set version($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasVersion() => $_has(1);
  @$pb.TagNumber(2)
  void clearVersion() => $_clearField(2);
}

class ReleaseInfo extends $pb.GeneratedMessage {
  factory ReleaseInfo({
    $core.String? version,
    $core.String? channel,
    $core.String? notesUrl,
    $core.bool? canUpdate,
  }) {
    final result = ReleaseInfo._();
    if (version != null) result.version = version;
    if (channel != null) result.channel = channel;
    if (notesUrl != null) result.notesUrl = notesUrl;
    if (canUpdate != null) result.canUpdate = canUpdate;
    return result;
  }

  ReleaseInfo._();

  factory ReleaseInfo.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ReleaseInfo()..mergeFromBuffer(data, registry);
  factory ReleaseInfo.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ReleaseInfo()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ReleaseInfo',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ReleaseInfo.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'version')
    ..aOS(2, _omitFieldNames ? '' : 'channel')
    ..aOS(3, _omitFieldNames ? '' : 'notesUrl')
    ..aOB(4, _omitFieldNames ? '' : 'canUpdate')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ReleaseInfo clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ReleaseInfo copyWith(void Function(ReleaseInfo) updates) =>
      super.copyWith((message) => updates(message as ReleaseInfo))
          as ReleaseInfo;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use ReleaseInfo() / ReleaseInfo.new instead')
  static ReleaseInfo create() => ReleaseInfo._();
  static $pb.GeneratedMessage $_createMessage() => ReleaseInfo._();
  @$core.override
  ReleaseInfo createEmptyInstance() => ReleaseInfo._();
  @$core.pragma('dart2js:noInline')
  static ReleaseInfo getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<ReleaseInfo>(
          ReleaseInfo.$_createMessage);
  static ReleaseInfo? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get version => $_getSZ(0);
  @$pb.TagNumber(1)
  set version($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasVersion() => $_has(0);
  @$pb.TagNumber(1)
  void clearVersion() => $_clearField(1);

  @$pb.TagNumber(2)
  $core.String get channel => $_getSZ(1);
  @$pb.TagNumber(2)
  set channel($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasChannel() => $_has(1);
  @$pb.TagNumber(2)
  void clearChannel() => $_clearField(2);

  @$pb.TagNumber(3)
  $core.String get notesUrl => $_getSZ(2);
  @$pb.TagNumber(3)
  set notesUrl($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasNotesUrl() => $_has(2);
  @$pb.TagNumber(3)
  void clearNotesUrl() => $_clearField(3);

  @$pb.TagNumber(4)
  $core.bool get canUpdate => $_getBF(3);
  @$pb.TagNumber(4)
  set canUpdate($core.bool value) => $_setBool(3, value);
  @$pb.TagNumber(4)
  $core.bool hasCanUpdate() => $_has(3);
  @$pb.TagNumber(4)
  void clearCanUpdate() => $_clearField(4);
}

class UpdateHubRequest extends $pb.GeneratedMessage {
  factory UpdateHubRequest({
    $core.String? channel,
    $core.String? version,
  }) {
    final result = UpdateHubRequest._();
    if (channel != null) result.channel = channel;
    if (version != null) result.version = version;
    return result;
  }

  UpdateHubRequest._();

  factory UpdateHubRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      UpdateHubRequest()..mergeFromBuffer(data, registry);
  factory UpdateHubRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      UpdateHubRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'UpdateHubRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: UpdateHubRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'channel')
    ..aOS(2, _omitFieldNames ? '' : 'version')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  UpdateHubRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  UpdateHubRequest copyWith(void Function(UpdateHubRequest) updates) =>
      super.copyWith((message) => updates(message as UpdateHubRequest))
          as UpdateHubRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use UpdateHubRequest() / UpdateHubRequest.new instead')
  static UpdateHubRequest create() => UpdateHubRequest._();
  static $pb.GeneratedMessage $_createMessage() => UpdateHubRequest._();
  @$core.override
  UpdateHubRequest createEmptyInstance() => UpdateHubRequest._();
  @$core.pragma('dart2js:noInline')
  static UpdateHubRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<UpdateHubRequest>(
          UpdateHubRequest.$_createMessage);
  static UpdateHubRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get channel => $_getSZ(0);
  @$pb.TagNumber(1)
  set channel($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasChannel() => $_has(0);
  @$pb.TagNumber(1)
  void clearChannel() => $_clearField(1);

  @$pb.TagNumber(2)
  $core.String get version => $_getSZ(1);
  @$pb.TagNumber(2)
  set version($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasVersion() => $_has(1);
  @$pb.TagNumber(2)
  void clearVersion() => $_clearField(2);
}

class UpdateHubProgress extends $pb.GeneratedMessage {
  factory UpdateHubProgress({
    UpdatePhase? phase,
    $core.String? message,
    $core.String? version,
    $core.String? previousVersion,
    $fixnum.Int64? sentBytes,
    $fixnum.Int64? totalBytes,
  }) {
    final result = UpdateHubProgress._();
    if (phase != null) result.phase = phase;
    if (message != null) result.message = message;
    if (version != null) result.version = version;
    if (previousVersion != null) result.previousVersion = previousVersion;
    if (sentBytes != null) result.sentBytes = sentBytes;
    if (totalBytes != null) result.totalBytes = totalBytes;
    return result;
  }

  UpdateHubProgress._();

  factory UpdateHubProgress.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      UpdateHubProgress()..mergeFromBuffer(data, registry);
  factory UpdateHubProgress.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      UpdateHubProgress()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'UpdateHubProgress',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: UpdateHubProgress.$_createMessage)
    ..aE<UpdatePhase>(1, _omitFieldNames ? '' : 'phase',
        enumValues: UpdatePhase.values)
    ..aOS(2, _omitFieldNames ? '' : 'message')
    ..aOS(3, _omitFieldNames ? '' : 'version')
    ..aOS(4, _omitFieldNames ? '' : 'previousVersion')
    ..a<$fixnum.Int64>(
        5, _omitFieldNames ? '' : 'sentBytes', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..a<$fixnum.Int64>(
        6, _omitFieldNames ? '' : 'totalBytes', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  UpdateHubProgress clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  UpdateHubProgress copyWith(void Function(UpdateHubProgress) updates) =>
      super.copyWith((message) => updates(message as UpdateHubProgress))
          as UpdateHubProgress;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use UpdateHubProgress() / UpdateHubProgress.new instead')
  static UpdateHubProgress create() => UpdateHubProgress._();
  static $pb.GeneratedMessage $_createMessage() => UpdateHubProgress._();
  @$core.override
  UpdateHubProgress createEmptyInstance() => UpdateHubProgress._();
  @$core.pragma('dart2js:noInline')
  static UpdateHubProgress getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<UpdateHubProgress>(
          UpdateHubProgress.$_createMessage);
  static UpdateHubProgress? _defaultInstance;

  @$pb.TagNumber(1)
  UpdatePhase get phase => $_getN(0);
  @$pb.TagNumber(1)
  set phase(UpdatePhase value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasPhase() => $_has(0);
  @$pb.TagNumber(1)
  void clearPhase() => $_clearField(1);

  @$pb.TagNumber(2)
  $core.String get message => $_getSZ(1);
  @$pb.TagNumber(2)
  set message($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasMessage() => $_has(1);
  @$pb.TagNumber(2)
  void clearMessage() => $_clearField(2);

  @$pb.TagNumber(3)
  $core.String get version => $_getSZ(2);
  @$pb.TagNumber(3)
  set version($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasVersion() => $_has(2);
  @$pb.TagNumber(3)
  void clearVersion() => $_clearField(3);

  @$pb.TagNumber(4)
  $core.String get previousVersion => $_getSZ(3);
  @$pb.TagNumber(4)
  set previousVersion($core.String value) => $_setString(3, value);
  @$pb.TagNumber(4)
  $core.bool hasPreviousVersion() => $_has(3);
  @$pb.TagNumber(4)
  void clearPreviousVersion() => $_clearField(4);

  @$pb.TagNumber(5)
  $fixnum.Int64 get sentBytes => $_getI64(4);
  @$pb.TagNumber(5)
  set sentBytes($fixnum.Int64 value) => $_setInt64(4, value);
  @$pb.TagNumber(5)
  $core.bool hasSentBytes() => $_has(4);
  @$pb.TagNumber(5)
  void clearSentBytes() => $_clearField(5);

  @$pb.TagNumber(6)
  $fixnum.Int64 get totalBytes => $_getI64(5);
  @$pb.TagNumber(6)
  set totalBytes($fixnum.Int64 value) => $_setInt64(5, value);
  @$pb.TagNumber(6)
  $core.bool hasTotalBytes() => $_has(5);
  @$pb.TagNumber(6)
  void clearTotalBytes() => $_clearField(6);
}

class ReleaseServiceApi {
  final $pb.RpcClient _client;

  ReleaseServiceApi(this._client);

  $async.Future<ReleaseInfo> checkRelease(
          $pb.ClientContext? ctx, CheckReleaseRequest request) =>
      _client.invoke<ReleaseInfo>(
          ctx, 'ReleaseService', 'CheckRelease', request, ReleaseInfo());
  $async.Future<UpdateHubProgress> updateHub(
          $pb.ClientContext? ctx, UpdateHubRequest request) =>
      _client.invoke<UpdateHubProgress>(
          ctx, 'ReleaseService', 'UpdateHub', request, UpdateHubProgress());
}

const $core.bool _omitFieldNames =
    $core.bool.fromEnvironment('protobuf.omit_field_names');
const $core.bool _omitMessageNames =
    $core.bool.fromEnvironment('protobuf.omit_message_names');
