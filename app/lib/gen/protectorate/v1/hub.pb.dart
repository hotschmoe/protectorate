// This is a generated file - do not edit.
//
// Generated from protectorate/v1/hub.proto.

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
import 'package:protobuf/well_known_types/google/protobuf/timestamp.pb.dart'
    as $0;

import 'audit.pb.dart' as $1;

export 'package:protobuf/protobuf.dart' show GeneratedMessageGenericExtensions;

class GetStatusRequest extends $pb.GeneratedMessage {
  factory GetStatusRequest() => GetStatusRequest._();

  GetStatusRequest._();

  factory GetStatusRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      GetStatusRequest()..mergeFromBuffer(data, registry);
  factory GetStatusRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      GetStatusRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'GetStatusRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: GetStatusRequest.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  GetStatusRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  GetStatusRequest copyWith(void Function(GetStatusRequest) updates) =>
      super.copyWith((message) => updates(message as GetStatusRequest))
          as GetStatusRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use GetStatusRequest() / GetStatusRequest.new instead')
  static GetStatusRequest create() => GetStatusRequest._();
  static $pb.GeneratedMessage $_createMessage() => GetStatusRequest._();
  @$core.override
  GetStatusRequest createEmptyInstance() => GetStatusRequest._();
  @$core.pragma('dart2js:noInline')
  static GetStatusRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<GetStatusRequest>(
          GetStatusRequest.$_createMessage);
  static GetStatusRequest? _defaultInstance;
}

class GetStatusResponse extends $pb.GeneratedMessage {
  factory GetStatusResponse({
    $core.String? version,
    $core.String? hostname,
    $core.String? os,
    $core.String? arch,
    $fixnum.Int64? uptimeSeconds,
    $0.Timestamp? startedAt,
    $core.String? availableVersion,
    $core.String? updateChannel,
    $core.String? releaseNotesUrl,
    $core.bool? releaseConfirmed,
    $1.AuditHead? auditHead,
  }) {
    final result = GetStatusResponse._();
    if (version != null) result.version = version;
    if (hostname != null) result.hostname = hostname;
    if (os != null) result.os = os;
    if (arch != null) result.arch = arch;
    if (uptimeSeconds != null) result.uptimeSeconds = uptimeSeconds;
    if (startedAt != null) result.startedAt = startedAt;
    if (availableVersion != null) result.availableVersion = availableVersion;
    if (updateChannel != null) result.updateChannel = updateChannel;
    if (releaseNotesUrl != null) result.releaseNotesUrl = releaseNotesUrl;
    if (releaseConfirmed != null) result.releaseConfirmed = releaseConfirmed;
    if (auditHead != null) result.auditHead = auditHead;
    return result;
  }

  GetStatusResponse._();

  factory GetStatusResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      GetStatusResponse()..mergeFromBuffer(data, registry);
  factory GetStatusResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      GetStatusResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'GetStatusResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: GetStatusResponse.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'version')
    ..aOS(2, _omitFieldNames ? '' : 'hostname')
    ..aOS(3, _omitFieldNames ? '' : 'os')
    ..aOS(4, _omitFieldNames ? '' : 'arch')
    ..aInt64(5, _omitFieldNames ? '' : 'uptimeSeconds')
    ..aOM<$0.Timestamp>(6, _omitFieldNames ? '' : 'startedAt',
        subBuilder: $0.Timestamp.$_createMessage)
    ..aOS(7, _omitFieldNames ? '' : 'availableVersion')
    ..aOS(8, _omitFieldNames ? '' : 'updateChannel')
    ..aOS(9, _omitFieldNames ? '' : 'releaseNotesUrl')
    ..aOB(10, _omitFieldNames ? '' : 'releaseConfirmed')
    ..aOM<$1.AuditHead>(11, _omitFieldNames ? '' : 'auditHead',
        subBuilder: $1.AuditHead.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  GetStatusResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  GetStatusResponse copyWith(void Function(GetStatusResponse) updates) =>
      super.copyWith((message) => updates(message as GetStatusResponse))
          as GetStatusResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use GetStatusResponse() / GetStatusResponse.new instead')
  static GetStatusResponse create() => GetStatusResponse._();
  static $pb.GeneratedMessage $_createMessage() => GetStatusResponse._();
  @$core.override
  GetStatusResponse createEmptyInstance() => GetStatusResponse._();
  @$core.pragma('dart2js:noInline')
  static GetStatusResponse getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<GetStatusResponse>(
          GetStatusResponse.$_createMessage);
  static GetStatusResponse? _defaultInstance;

  /// version is the hub build version.
  @$pb.TagNumber(1)
  $core.String get version => $_getSZ(0);
  @$pb.TagNumber(1)
  set version($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasVersion() => $_has(0);
  @$pb.TagNumber(1)
  void clearVersion() => $_clearField(1);

  @$pb.TagNumber(2)
  $core.String get hostname => $_getSZ(1);
  @$pb.TagNumber(2)
  set hostname($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasHostname() => $_has(1);
  @$pb.TagNumber(2)
  void clearHostname() => $_clearField(2);

  /// os and arch are Go GOOS/GOARCH values of the hub host.
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
  $fixnum.Int64 get uptimeSeconds => $_getI64(4);
  @$pb.TagNumber(5)
  set uptimeSeconds($fixnum.Int64 value) => $_setInt64(4, value);
  @$pb.TagNumber(5)
  $core.bool hasUptimeSeconds() => $_has(4);
  @$pb.TagNumber(5)
  void clearUptimeSeconds() => $_clearField(5);

  @$pb.TagNumber(6)
  $0.Timestamp get startedAt => $_getN(5);
  @$pb.TagNumber(6)
  set startedAt($0.Timestamp value) => $_setField(6, value);
  @$pb.TagNumber(6)
  $core.bool hasStartedAt() => $_has(5);
  @$pb.TagNumber(6)
  void clearStartedAt() => $_clearField(6);
  @$pb.TagNumber(6)
  $0.Timestamp ensureStartedAt() => $_ensure(5);

  @$pb.TagNumber(7)
  $core.String get availableVersion => $_getSZ(6);
  @$pb.TagNumber(7)
  set availableVersion($core.String value) => $_setString(6, value);
  @$pb.TagNumber(7)
  $core.bool hasAvailableVersion() => $_has(6);
  @$pb.TagNumber(7)
  void clearAvailableVersion() => $_clearField(7);

  @$pb.TagNumber(8)
  $core.String get updateChannel => $_getSZ(7);
  @$pb.TagNumber(8)
  set updateChannel($core.String value) => $_setString(7, value);
  @$pb.TagNumber(8)
  $core.bool hasUpdateChannel() => $_has(7);
  @$pb.TagNumber(8)
  void clearUpdateChannel() => $_clearField(8);

  @$pb.TagNumber(9)
  $core.String get releaseNotesUrl => $_getSZ(8);
  @$pb.TagNumber(9)
  set releaseNotesUrl($core.String value) => $_setString(8, value);
  @$pb.TagNumber(9)
  $core.bool hasReleaseNotesUrl() => $_has(8);
  @$pb.TagNumber(9)
  void clearReleaseNotesUrl() => $_clearField(9);

  /// True when the running release is the guardian-confirmed active release.
  @$pb.TagNumber(10)
  $core.bool get releaseConfirmed => $_getBF(9);
  @$pb.TagNumber(10)
  set releaseConfirmed($core.bool value) => $_setBool(9, value);
  @$pb.TagNumber(10)
  $core.bool hasReleaseConfirmed() => $_has(9);
  @$pb.TagNumber(10)
  void clearReleaseConfirmed() => $_clearField(10);

  /// audit_head is the newest audit entry; see AuditService.VerifyAudit.
  @$pb.TagNumber(11)
  $1.AuditHead get auditHead => $_getN(10);
  @$pb.TagNumber(11)
  set auditHead($1.AuditHead value) => $_setField(11, value);
  @$pb.TagNumber(11)
  $core.bool hasAuditHead() => $_has(10);
  @$pb.TagNumber(11)
  void clearAuditHead() => $_clearField(11);
  @$pb.TagNumber(11)
  $1.AuditHead ensureAuditHead() => $_ensure(10);
}

class GetSleeveRuntimeRequest extends $pb.GeneratedMessage {
  factory GetSleeveRuntimeRequest() => GetSleeveRuntimeRequest._();

  GetSleeveRuntimeRequest._();

  factory GetSleeveRuntimeRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      GetSleeveRuntimeRequest()..mergeFromBuffer(data, registry);
  factory GetSleeveRuntimeRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      GetSleeveRuntimeRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'GetSleeveRuntimeRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: GetSleeveRuntimeRequest.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  GetSleeveRuntimeRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  GetSleeveRuntimeRequest copyWith(
          void Function(GetSleeveRuntimeRequest) updates) =>
      super.copyWith((message) => updates(message as GetSleeveRuntimeRequest))
          as GetSleeveRuntimeRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use GetSleeveRuntimeRequest() / GetSleeveRuntimeRequest.new instead')
  static GetSleeveRuntimeRequest create() => GetSleeveRuntimeRequest._();
  static $pb.GeneratedMessage $_createMessage() => GetSleeveRuntimeRequest._();
  @$core.override
  GetSleeveRuntimeRequest createEmptyInstance() => GetSleeveRuntimeRequest._();
  @$core.pragma('dart2js:noInline')
  static GetSleeveRuntimeRequest getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<GetSleeveRuntimeRequest>(
          GetSleeveRuntimeRequest.$_createMessage);
  static GetSleeveRuntimeRequest? _defaultInstance;
}

class GetSleeveRuntimeResponse extends $pb.GeneratedMessage {
  factory GetSleeveRuntimeResponse({
    $core.String? bwrapPath,
    $core.String? bwrapVersion,
    $core.bool? overlays,
    $core.String? goBuildCache,
    $core.String? goBuildLower,
    $fixnum.Int64? snapshotBytes,
    $fixnum.Int64? snapshotEntries,
    $0.Timestamp? snapshotPreparedAt,
    $core.bool? snapshotPreparing,
    $core.String? snapshotError,
    $fixnum.Int64? snapshotLimitBytes,
    $core.String? runtime,
    $core.String? helperPath,
  }) {
    final result = GetSleeveRuntimeResponse._();
    if (bwrapPath != null) result.bwrapPath = bwrapPath;
    if (bwrapVersion != null) result.bwrapVersion = bwrapVersion;
    if (overlays != null) result.overlays = overlays;
    if (goBuildCache != null) result.goBuildCache = goBuildCache;
    if (goBuildLower != null) result.goBuildLower = goBuildLower;
    if (snapshotBytes != null) result.snapshotBytes = snapshotBytes;
    if (snapshotEntries != null) result.snapshotEntries = snapshotEntries;
    if (snapshotPreparedAt != null)
      result.snapshotPreparedAt = snapshotPreparedAt;
    if (snapshotPreparing != null) result.snapshotPreparing = snapshotPreparing;
    if (snapshotError != null) result.snapshotError = snapshotError;
    if (snapshotLimitBytes != null)
      result.snapshotLimitBytes = snapshotLimitBytes;
    if (runtime != null) result.runtime = runtime;
    if (helperPath != null) result.helperPath = helperPath;
    return result;
  }

  GetSleeveRuntimeResponse._();

  factory GetSleeveRuntimeResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      GetSleeveRuntimeResponse()..mergeFromBuffer(data, registry);
  factory GetSleeveRuntimeResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      GetSleeveRuntimeResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'GetSleeveRuntimeResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: GetSleeveRuntimeResponse.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'bwrapPath')
    ..aOS(2, _omitFieldNames ? '' : 'bwrapVersion')
    ..aOB(3, _omitFieldNames ? '' : 'overlays')
    ..aOS(4, _omitFieldNames ? '' : 'goBuildCache')
    ..aOS(5, _omitFieldNames ? '' : 'goBuildLower')
    ..a<$fixnum.Int64>(
        6, _omitFieldNames ? '' : 'snapshotBytes', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..a<$fixnum.Int64>(
        7, _omitFieldNames ? '' : 'snapshotEntries', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..aOM<$0.Timestamp>(8, _omitFieldNames ? '' : 'snapshotPreparedAt',
        subBuilder: $0.Timestamp.$_createMessage)
    ..aOB(9, _omitFieldNames ? '' : 'snapshotPreparing')
    ..aOS(10, _omitFieldNames ? '' : 'snapshotError')
    ..a<$fixnum.Int64>(
        11, _omitFieldNames ? '' : 'snapshotLimitBytes', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..aOS(12, _omitFieldNames ? '' : 'runtime')
    ..aOS(13, _omitFieldNames ? '' : 'helperPath')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  GetSleeveRuntimeResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  GetSleeveRuntimeResponse copyWith(
          void Function(GetSleeveRuntimeResponse) updates) =>
      super.copyWith((message) => updates(message as GetSleeveRuntimeResponse))
          as GetSleeveRuntimeResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use GetSleeveRuntimeResponse() / GetSleeveRuntimeResponse.new instead')
  static GetSleeveRuntimeResponse create() => GetSleeveRuntimeResponse._();
  static $pb.GeneratedMessage $_createMessage() => GetSleeveRuntimeResponse._();
  @$core.override
  GetSleeveRuntimeResponse createEmptyInstance() =>
      GetSleeveRuntimeResponse._();
  @$core.pragma('dart2js:noInline')
  static GetSleeveRuntimeResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<GetSleeveRuntimeResponse>(
          GetSleeveRuntimeResponse.$_createMessage);
  static GetSleeveRuntimeResponse? _defaultInstance;

  /// Legacy fields retained while the Flutter client moves to runtime/helper_path.
  @$pb.TagNumber(1)
  $core.String get bwrapPath => $_getSZ(0);
  @$pb.TagNumber(1)
  set bwrapPath($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasBwrapPath() => $_has(0);
  @$pb.TagNumber(1)
  void clearBwrapPath() => $_clearField(1);

  @$pb.TagNumber(2)
  $core.String get bwrapVersion => $_getSZ(1);
  @$pb.TagNumber(2)
  set bwrapVersion($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasBwrapVersion() => $_has(1);
  @$pb.TagNumber(2)
  void clearBwrapVersion() => $_clearField(2);

  @$pb.TagNumber(3)
  $core.bool get overlays => $_getBF(2);
  @$pb.TagNumber(3)
  set overlays($core.bool value) => $_setBool(2, value);
  @$pb.TagNumber(3)
  $core.bool hasOverlays() => $_has(2);
  @$pb.TagNumber(3)
  void clearOverlays() => $_clearField(3);

  @$pb.TagNumber(4)
  $core.String get goBuildCache => $_getSZ(3);
  @$pb.TagNumber(4)
  set goBuildCache($core.String value) => $_setString(3, value);
  @$pb.TagNumber(4)
  $core.bool hasGoBuildCache() => $_has(3);
  @$pb.TagNumber(4)
  void clearGoBuildCache() => $_clearField(4);

  @$pb.TagNumber(5)
  $core.String get goBuildLower => $_getSZ(4);
  @$pb.TagNumber(5)
  set goBuildLower($core.String value) => $_setString(4, value);
  @$pb.TagNumber(5)
  $core.bool hasGoBuildLower() => $_has(4);
  @$pb.TagNumber(5)
  void clearGoBuildLower() => $_clearField(5);

  @$pb.TagNumber(6)
  $fixnum.Int64 get snapshotBytes => $_getI64(5);
  @$pb.TagNumber(6)
  set snapshotBytes($fixnum.Int64 value) => $_setInt64(5, value);
  @$pb.TagNumber(6)
  $core.bool hasSnapshotBytes() => $_has(5);
  @$pb.TagNumber(6)
  void clearSnapshotBytes() => $_clearField(6);

  @$pb.TagNumber(7)
  $fixnum.Int64 get snapshotEntries => $_getI64(6);
  @$pb.TagNumber(7)
  set snapshotEntries($fixnum.Int64 value) => $_setInt64(6, value);
  @$pb.TagNumber(7)
  $core.bool hasSnapshotEntries() => $_has(6);
  @$pb.TagNumber(7)
  void clearSnapshotEntries() => $_clearField(7);

  @$pb.TagNumber(8)
  $0.Timestamp get snapshotPreparedAt => $_getN(7);
  @$pb.TagNumber(8)
  set snapshotPreparedAt($0.Timestamp value) => $_setField(8, value);
  @$pb.TagNumber(8)
  $core.bool hasSnapshotPreparedAt() => $_has(7);
  @$pb.TagNumber(8)
  void clearSnapshotPreparedAt() => $_clearField(8);
  @$pb.TagNumber(8)
  $0.Timestamp ensureSnapshotPreparedAt() => $_ensure(7);

  @$pb.TagNumber(9)
  $core.bool get snapshotPreparing => $_getBF(8);
  @$pb.TagNumber(9)
  set snapshotPreparing($core.bool value) => $_setBool(8, value);
  @$pb.TagNumber(9)
  $core.bool hasSnapshotPreparing() => $_has(8);
  @$pb.TagNumber(9)
  void clearSnapshotPreparing() => $_clearField(9);

  @$pb.TagNumber(10)
  $core.String get snapshotError => $_getSZ(9);
  @$pb.TagNumber(10)
  set snapshotError($core.String value) => $_setString(9, value);
  @$pb.TagNumber(10)
  $core.bool hasSnapshotError() => $_has(9);
  @$pb.TagNumber(10)
  void clearSnapshotError() => $_clearField(10);

  @$pb.TagNumber(11)
  $fixnum.Int64 get snapshotLimitBytes => $_getI64(10);
  @$pb.TagNumber(11)
  set snapshotLimitBytes($fixnum.Int64 value) => $_setInt64(10, value);
  @$pb.TagNumber(11)
  $core.bool hasSnapshotLimitBytes() => $_has(10);
  @$pb.TagNumber(11)
  void clearSnapshotLimitBytes() => $_clearField(11);

  /// runtime identifies the active sleeve launcher, for example
  /// "rust-namespaces". Empty means no launcher is configured.
  @$pb.TagNumber(12)
  $core.String get runtime => $_getSZ(11);
  @$pb.TagNumber(12)
  set runtime($core.String value) => $_setString(11, value);
  @$pb.TagNumber(12)
  $core.bool hasRuntime() => $_has(11);
  @$pb.TagNumber(12)
  void clearRuntime() => $_clearField(12);

  /// helper_path is the Linux namespace helper executable used by the runtime.
  @$pb.TagNumber(13)
  $core.String get helperPath => $_getSZ(12);
  @$pb.TagNumber(13)
  set helperPath($core.String value) => $_setString(12, value);
  @$pb.TagNumber(13)
  $core.bool hasHelperPath() => $_has(12);
  @$pb.TagNumber(13)
  void clearHelperPath() => $_clearField(13);
}

/// HubService reports on the hub process itself.
class HubServiceApi {
  final $pb.RpcClient _client;

  HubServiceApi(this._client);

  /// GetStatus returns the hub version, host platform and uptime.
  $async.Future<GetStatusResponse> getStatus(
          $pb.ClientContext? ctx, GetStatusRequest request) =>
      _client.invoke<GetStatusResponse>(
          ctx, 'HubService', 'GetStatus', request, GetStatusResponse());

  /// GetSleeveRuntime reports the detected sandbox runtime and shared cache lower.
  $async.Future<GetSleeveRuntimeResponse> getSleeveRuntime(
          $pb.ClientContext? ctx, GetSleeveRuntimeRequest request) =>
      _client.invoke<GetSleeveRuntimeResponse>(ctx, 'HubService',
          'GetSleeveRuntime', request, GetSleeveRuntimeResponse());
}

const $core.bool _omitFieldNames =
    $core.bool.fromEnvironment('protobuf.omit_field_names');
const $core.bool _omitMessageNames =
    $core.bool.fromEnvironment('protobuf.omit_message_names');
