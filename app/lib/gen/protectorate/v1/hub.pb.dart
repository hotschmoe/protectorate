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
  }) {
    final result = GetStatusResponse._();
    if (version != null) result.version = version;
    if (hostname != null) result.hostname = hostname;
    if (os != null) result.os = os;
    if (arch != null) result.arch = arch;
    if (uptimeSeconds != null) result.uptimeSeconds = uptimeSeconds;
    if (startedAt != null) result.startedAt = startedAt;
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
  }) {
    final result = GetSleeveRuntimeResponse._();
    if (bwrapPath != null) result.bwrapPath = bwrapPath;
    if (bwrapVersion != null) result.bwrapVersion = bwrapVersion;
    if (overlays != null) result.overlays = overlays;
    if (goBuildCache != null) result.goBuildCache = goBuildCache;
    if (goBuildLower != null) result.goBuildLower = goBuildLower;
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
