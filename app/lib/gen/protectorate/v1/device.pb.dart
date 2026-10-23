// This is a generated file - do not edit.
//
// Generated from protectorate/v1/device.proto.

// @dart = 3.3

// ignore_for_file: annotate_overrides, camel_case_types, comment_references
// ignore_for_file: constant_identifier_names
// ignore_for_file: curly_braces_in_flow_control_structures
// ignore_for_file: deprecated_member_use_from_same_package, library_prefixes
// ignore_for_file: non_constant_identifier_names, prefer_relative_imports

import 'dart:async' as $async;
import 'dart:core' as $core;

import 'package:protobuf/protobuf.dart' as $pb;
import 'package:protobuf/well_known_types/google/protobuf/timestamp.pb.dart'
    as $0;

export 'package:protobuf/protobuf.dart' show GeneratedMessageGenericExtensions;

class StartDeviceAuthRequest extends $pb.GeneratedMessage {
  factory StartDeviceAuthRequest({
    $core.String? name,
    $core.String? platform,
    $core.List<$core.int>? publicKey,
  }) {
    final result = StartDeviceAuthRequest._();
    if (name != null) result.name = name;
    if (platform != null) result.platform = platform;
    if (publicKey != null) result.publicKey = publicKey;
    return result;
  }

  StartDeviceAuthRequest._();

  factory StartDeviceAuthRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      StartDeviceAuthRequest()..mergeFromBuffer(data, registry);
  factory StartDeviceAuthRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      StartDeviceAuthRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'StartDeviceAuthRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: StartDeviceAuthRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'name')
    ..aOS(2, _omitFieldNames ? '' : 'platform')
    ..a<$core.List<$core.int>>(
        3, _omitFieldNames ? '' : 'publicKey', $pb.PbFieldType.OY)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  StartDeviceAuthRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  StartDeviceAuthRequest copyWith(
          void Function(StartDeviceAuthRequest) updates) =>
      super.copyWith((message) => updates(message as StartDeviceAuthRequest))
          as StartDeviceAuthRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use StartDeviceAuthRequest() / StartDeviceAuthRequest.new instead')
  static StartDeviceAuthRequest create() => StartDeviceAuthRequest._();
  static $pb.GeneratedMessage $_createMessage() => StartDeviceAuthRequest._();
  @$core.override
  StartDeviceAuthRequest createEmptyInstance() => StartDeviceAuthRequest._();
  @$core.pragma('dart2js:noInline')
  static StartDeviceAuthRequest getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<StartDeviceAuthRequest>(
          StartDeviceAuthRequest.$_createMessage);
  static StartDeviceAuthRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get name => $_getSZ(0);
  @$pb.TagNumber(1)
  set name($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasName() => $_has(0);
  @$pb.TagNumber(1)
  void clearName() => $_clearField(1);

  @$pb.TagNumber(2)
  $core.String get platform => $_getSZ(1);
  @$pb.TagNumber(2)
  set platform($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasPlatform() => $_has(1);
  @$pb.TagNumber(2)
  void clearPlatform() => $_clearField(2);

  /// public_key is the device's uncompressed P-256 public key (65 bytes).
  @$pb.TagNumber(3)
  $core.List<$core.int> get publicKey => $_getN(2);
  @$pb.TagNumber(3)
  set publicKey($core.List<$core.int> value) => $_setBytes(2, value);
  @$pb.TagNumber(3)
  $core.bool hasPublicKey() => $_has(2);
  @$pb.TagNumber(3)
  void clearPublicKey() => $_clearField(3);
}

class StartDeviceAuthResponse extends $pb.GeneratedMessage {
  factory StartDeviceAuthResponse({
    $core.String? deviceCode,
    $core.String? userCode,
    $0.Timestamp? expiresAt,
    $core.int? interval,
  }) {
    final result = StartDeviceAuthResponse._();
    if (deviceCode != null) result.deviceCode = deviceCode;
    if (userCode != null) result.userCode = userCode;
    if (expiresAt != null) result.expiresAt = expiresAt;
    if (interval != null) result.interval = interval;
    return result;
  }

  StartDeviceAuthResponse._();

  factory StartDeviceAuthResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      StartDeviceAuthResponse()..mergeFromBuffer(data, registry);
  factory StartDeviceAuthResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      StartDeviceAuthResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'StartDeviceAuthResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: StartDeviceAuthResponse.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'deviceCode')
    ..aOS(2, _omitFieldNames ? '' : 'userCode')
    ..aOM<$0.Timestamp>(3, _omitFieldNames ? '' : 'expiresAt',
        subBuilder: $0.Timestamp.$_createMessage)
    ..aI(4, _omitFieldNames ? '' : 'interval')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  StartDeviceAuthResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  StartDeviceAuthResponse copyWith(
          void Function(StartDeviceAuthResponse) updates) =>
      super.copyWith((message) => updates(message as StartDeviceAuthResponse))
          as StartDeviceAuthResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use StartDeviceAuthResponse() / StartDeviceAuthResponse.new instead')
  static StartDeviceAuthResponse create() => StartDeviceAuthResponse._();
  static $pb.GeneratedMessage $_createMessage() => StartDeviceAuthResponse._();
  @$core.override
  StartDeviceAuthResponse createEmptyInstance() => StartDeviceAuthResponse._();
  @$core.pragma('dart2js:noInline')
  static StartDeviceAuthResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<StartDeviceAuthResponse>(
          StartDeviceAuthResponse.$_createMessage);
  static StartDeviceAuthResponse? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get deviceCode => $_getSZ(0);
  @$pb.TagNumber(1)
  set deviceCode($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasDeviceCode() => $_has(0);
  @$pb.TagNumber(1)
  void clearDeviceCode() => $_clearField(1);

  @$pb.TagNumber(2)
  $core.String get userCode => $_getSZ(1);
  @$pb.TagNumber(2)
  set userCode($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasUserCode() => $_has(1);
  @$pb.TagNumber(2)
  void clearUserCode() => $_clearField(2);

  @$pb.TagNumber(3)
  $0.Timestamp get expiresAt => $_getN(2);
  @$pb.TagNumber(3)
  set expiresAt($0.Timestamp value) => $_setField(3, value);
  @$pb.TagNumber(3)
  $core.bool hasExpiresAt() => $_has(2);
  @$pb.TagNumber(3)
  void clearExpiresAt() => $_clearField(3);
  @$pb.TagNumber(3)
  $0.Timestamp ensureExpiresAt() => $_ensure(2);

  @$pb.TagNumber(4)
  $core.int get interval => $_getIZ(3);
  @$pb.TagNumber(4)
  set interval($core.int value) => $_setSignedInt32(3, value);
  @$pb.TagNumber(4)
  $core.bool hasInterval() => $_has(3);
  @$pb.TagNumber(4)
  void clearInterval() => $_clearField(4);
}

class PollDeviceAuthRequest extends $pb.GeneratedMessage {
  factory PollDeviceAuthRequest({
    $core.String? deviceCode,
  }) {
    final result = PollDeviceAuthRequest._();
    if (deviceCode != null) result.deviceCode = deviceCode;
    return result;
  }

  PollDeviceAuthRequest._();

  factory PollDeviceAuthRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      PollDeviceAuthRequest()..mergeFromBuffer(data, registry);
  factory PollDeviceAuthRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      PollDeviceAuthRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'PollDeviceAuthRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: PollDeviceAuthRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'deviceCode')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  PollDeviceAuthRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  PollDeviceAuthRequest copyWith(
          void Function(PollDeviceAuthRequest) updates) =>
      super.copyWith((message) => updates(message as PollDeviceAuthRequest))
          as PollDeviceAuthRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use PollDeviceAuthRequest() / PollDeviceAuthRequest.new instead')
  static PollDeviceAuthRequest create() => PollDeviceAuthRequest._();
  static $pb.GeneratedMessage $_createMessage() => PollDeviceAuthRequest._();
  @$core.override
  PollDeviceAuthRequest createEmptyInstance() => PollDeviceAuthRequest._();
  @$core.pragma('dart2js:noInline')
  static PollDeviceAuthRequest getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<PollDeviceAuthRequest>(
          PollDeviceAuthRequest.$_createMessage);
  static PollDeviceAuthRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get deviceCode => $_getSZ(0);
  @$pb.TagNumber(1)
  set deviceCode($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasDeviceCode() => $_has(0);
  @$pb.TagNumber(1)
  void clearDeviceCode() => $_clearField(1);
}

class PollDeviceAuthResponse extends $pb.GeneratedMessage {
  factory PollDeviceAuthResponse({
    $core.String? status,
    $core.int? interval,
    DeviceInfo? device,
  }) {
    final result = PollDeviceAuthResponse._();
    if (status != null) result.status = status;
    if (interval != null) result.interval = interval;
    if (device != null) result.device = device;
    return result;
  }

  PollDeviceAuthResponse._();

  factory PollDeviceAuthResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      PollDeviceAuthResponse()..mergeFromBuffer(data, registry);
  factory PollDeviceAuthResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      PollDeviceAuthResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'PollDeviceAuthResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: PollDeviceAuthResponse.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'status')
    ..aI(2, _omitFieldNames ? '' : 'interval')
    ..aOM<DeviceInfo>(3, _omitFieldNames ? '' : 'device',
        subBuilder: DeviceInfo.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  PollDeviceAuthResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  PollDeviceAuthResponse copyWith(
          void Function(PollDeviceAuthResponse) updates) =>
      super.copyWith((message) => updates(message as PollDeviceAuthResponse))
          as PollDeviceAuthResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use PollDeviceAuthResponse() / PollDeviceAuthResponse.new instead')
  static PollDeviceAuthResponse create() => PollDeviceAuthResponse._();
  static $pb.GeneratedMessage $_createMessage() => PollDeviceAuthResponse._();
  @$core.override
  PollDeviceAuthResponse createEmptyInstance() => PollDeviceAuthResponse._();
  @$core.pragma('dart2js:noInline')
  static PollDeviceAuthResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<PollDeviceAuthResponse>(
          PollDeviceAuthResponse.$_createMessage);
  static PollDeviceAuthResponse? _defaultInstance;

  /// status is pending, slow_down, approved, denied, expired or consumed.
  @$pb.TagNumber(1)
  $core.String get status => $_getSZ(0);
  @$pb.TagNumber(1)
  set status($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasStatus() => $_has(0);
  @$pb.TagNumber(1)
  void clearStatus() => $_clearField(1);

  @$pb.TagNumber(2)
  $core.int get interval => $_getIZ(1);
  @$pb.TagNumber(2)
  set interval($core.int value) => $_setSignedInt32(1, value);
  @$pb.TagNumber(2)
  $core.bool hasInterval() => $_has(1);
  @$pb.TagNumber(2)
  void clearInterval() => $_clearField(2);

  /// device is set once, on the poll that reports approved.
  @$pb.TagNumber(3)
  DeviceInfo get device => $_getN(2);
  @$pb.TagNumber(3)
  set device(DeviceInfo value) => $_setField(3, value);
  @$pb.TagNumber(3)
  $core.bool hasDevice() => $_has(2);
  @$pb.TagNumber(3)
  void clearDevice() => $_clearField(3);
  @$pb.TagNumber(3)
  DeviceInfo ensureDevice() => $_ensure(2);
}

/// DeviceInfo describes an approved device. Its id is derived from its public
/// key and names it in audit lines.
class DeviceInfo extends $pb.GeneratedMessage {
  factory DeviceInfo({
    $core.String? id,
    $core.String? name,
    $core.String? platform,
    $0.Timestamp? approvedAt,
    $core.String? approvedBy,
    $0.Timestamp? lastUsedAt,
  }) {
    final result = DeviceInfo._();
    if (id != null) result.id = id;
    if (name != null) result.name = name;
    if (platform != null) result.platform = platform;
    if (approvedAt != null) result.approvedAt = approvedAt;
    if (approvedBy != null) result.approvedBy = approvedBy;
    if (lastUsedAt != null) result.lastUsedAt = lastUsedAt;
    return result;
  }

  DeviceInfo._();

  factory DeviceInfo.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      DeviceInfo()..mergeFromBuffer(data, registry);
  factory DeviceInfo.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      DeviceInfo()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'DeviceInfo',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: DeviceInfo.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'id')
    ..aOS(2, _omitFieldNames ? '' : 'name')
    ..aOS(3, _omitFieldNames ? '' : 'platform')
    ..aOM<$0.Timestamp>(4, _omitFieldNames ? '' : 'approvedAt',
        subBuilder: $0.Timestamp.$_createMessage)
    ..aOS(5, _omitFieldNames ? '' : 'approvedBy')
    ..aOM<$0.Timestamp>(6, _omitFieldNames ? '' : 'lastUsedAt',
        subBuilder: $0.Timestamp.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  DeviceInfo clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  DeviceInfo copyWith(void Function(DeviceInfo) updates) =>
      super.copyWith((message) => updates(message as DeviceInfo)) as DeviceInfo;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use DeviceInfo() / DeviceInfo.new instead')
  static DeviceInfo create() => DeviceInfo._();
  static $pb.GeneratedMessage $_createMessage() => DeviceInfo._();
  @$core.override
  DeviceInfo createEmptyInstance() => DeviceInfo._();
  @$core.pragma('dart2js:noInline')
  static DeviceInfo getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<DeviceInfo>(DeviceInfo.$_createMessage);
  static DeviceInfo? _defaultInstance;

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
  $core.String get platform => $_getSZ(2);
  @$pb.TagNumber(3)
  set platform($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasPlatform() => $_has(2);
  @$pb.TagNumber(3)
  void clearPlatform() => $_clearField(3);

  @$pb.TagNumber(4)
  $0.Timestamp get approvedAt => $_getN(3);
  @$pb.TagNumber(4)
  set approvedAt($0.Timestamp value) => $_setField(4, value);
  @$pb.TagNumber(4)
  $core.bool hasApprovedAt() => $_has(3);
  @$pb.TagNumber(4)
  void clearApprovedAt() => $_clearField(4);
  @$pb.TagNumber(4)
  $0.Timestamp ensureApprovedAt() => $_ensure(3);

  @$pb.TagNumber(5)
  $core.String get approvedBy => $_getSZ(4);
  @$pb.TagNumber(5)
  set approvedBy($core.String value) => $_setString(4, value);
  @$pb.TagNumber(5)
  $core.bool hasApprovedBy() => $_has(4);
  @$pb.TagNumber(5)
  void clearApprovedBy() => $_clearField(5);

  /// last_used_at is persisted at most about once a minute.
  @$pb.TagNumber(6)
  $0.Timestamp get lastUsedAt => $_getN(5);
  @$pb.TagNumber(6)
  set lastUsedAt($0.Timestamp value) => $_setField(6, value);
  @$pb.TagNumber(6)
  $core.bool hasLastUsedAt() => $_has(5);
  @$pb.TagNumber(6)
  void clearLastUsedAt() => $_clearField(6);
  @$pb.TagNumber(6)
  $0.Timestamp ensureLastUsedAt() => $_ensure(5);
}

class DeviceAuthInfo extends $pb.GeneratedMessage {
  factory DeviceAuthInfo({
    $core.String? userCode,
    $core.String? name,
    $core.String? platform,
    $core.String? requestingIp,
    $0.Timestamp? expiresAt,
  }) {
    final result = DeviceAuthInfo._();
    if (userCode != null) result.userCode = userCode;
    if (name != null) result.name = name;
    if (platform != null) result.platform = platform;
    if (requestingIp != null) result.requestingIp = requestingIp;
    if (expiresAt != null) result.expiresAt = expiresAt;
    return result;
  }

  DeviceAuthInfo._();

  factory DeviceAuthInfo.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      DeviceAuthInfo()..mergeFromBuffer(data, registry);
  factory DeviceAuthInfo.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      DeviceAuthInfo()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'DeviceAuthInfo',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: DeviceAuthInfo.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'userCode')
    ..aOS(2, _omitFieldNames ? '' : 'name')
    ..aOS(3, _omitFieldNames ? '' : 'platform')
    ..aOS(4, _omitFieldNames ? '' : 'requestingIp')
    ..aOM<$0.Timestamp>(5, _omitFieldNames ? '' : 'expiresAt',
        subBuilder: $0.Timestamp.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  DeviceAuthInfo clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  DeviceAuthInfo copyWith(void Function(DeviceAuthInfo) updates) =>
      super.copyWith((message) => updates(message as DeviceAuthInfo))
          as DeviceAuthInfo;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use DeviceAuthInfo() / DeviceAuthInfo.new instead')
  static DeviceAuthInfo create() => DeviceAuthInfo._();
  static $pb.GeneratedMessage $_createMessage() => DeviceAuthInfo._();
  @$core.override
  DeviceAuthInfo createEmptyInstance() => DeviceAuthInfo._();
  @$core.pragma('dart2js:noInline')
  static DeviceAuthInfo getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<DeviceAuthInfo>(
          DeviceAuthInfo.$_createMessage);
  static DeviceAuthInfo? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get userCode => $_getSZ(0);
  @$pb.TagNumber(1)
  set userCode($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasUserCode() => $_has(0);
  @$pb.TagNumber(1)
  void clearUserCode() => $_clearField(1);

  @$pb.TagNumber(2)
  $core.String get name => $_getSZ(1);
  @$pb.TagNumber(2)
  set name($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasName() => $_has(1);
  @$pb.TagNumber(2)
  void clearName() => $_clearField(2);

  @$pb.TagNumber(3)
  $core.String get platform => $_getSZ(2);
  @$pb.TagNumber(3)
  set platform($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasPlatform() => $_has(2);
  @$pb.TagNumber(3)
  void clearPlatform() => $_clearField(3);

  @$pb.TagNumber(4)
  $core.String get requestingIp => $_getSZ(3);
  @$pb.TagNumber(4)
  set requestingIp($core.String value) => $_setString(3, value);
  @$pb.TagNumber(4)
  $core.bool hasRequestingIp() => $_has(3);
  @$pb.TagNumber(4)
  void clearRequestingIp() => $_clearField(4);

  @$pb.TagNumber(5)
  $0.Timestamp get expiresAt => $_getN(4);
  @$pb.TagNumber(5)
  set expiresAt($0.Timestamp value) => $_setField(5, value);
  @$pb.TagNumber(5)
  $core.bool hasExpiresAt() => $_has(4);
  @$pb.TagNumber(5)
  void clearExpiresAt() => $_clearField(5);
  @$pb.TagNumber(5)
  $0.Timestamp ensureExpiresAt() => $_ensure(4);
}

class ListDeviceAuthRequest extends $pb.GeneratedMessage {
  factory ListDeviceAuthRequest() => ListDeviceAuthRequest._();

  ListDeviceAuthRequest._();

  factory ListDeviceAuthRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListDeviceAuthRequest()..mergeFromBuffer(data, registry);
  factory ListDeviceAuthRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListDeviceAuthRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ListDeviceAuthRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ListDeviceAuthRequest.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListDeviceAuthRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListDeviceAuthRequest copyWith(
          void Function(ListDeviceAuthRequest) updates) =>
      super.copyWith((message) => updates(message as ListDeviceAuthRequest))
          as ListDeviceAuthRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use ListDeviceAuthRequest() / ListDeviceAuthRequest.new instead')
  static ListDeviceAuthRequest create() => ListDeviceAuthRequest._();
  static $pb.GeneratedMessage $_createMessage() => ListDeviceAuthRequest._();
  @$core.override
  ListDeviceAuthRequest createEmptyInstance() => ListDeviceAuthRequest._();
  @$core.pragma('dart2js:noInline')
  static ListDeviceAuthRequest getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<ListDeviceAuthRequest>(
          ListDeviceAuthRequest.$_createMessage);
  static ListDeviceAuthRequest? _defaultInstance;
}

class ListDeviceAuthResponse extends $pb.GeneratedMessage {
  factory ListDeviceAuthResponse({
    $core.Iterable<DeviceAuthInfo>? requests,
  }) {
    final result = ListDeviceAuthResponse._();
    if (requests != null) result.requests.addAll(requests);
    return result;
  }

  ListDeviceAuthResponse._();

  factory ListDeviceAuthResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListDeviceAuthResponse()..mergeFromBuffer(data, registry);
  factory ListDeviceAuthResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListDeviceAuthResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ListDeviceAuthResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ListDeviceAuthResponse.$_createMessage)
    ..pPM<DeviceAuthInfo>(1, _omitFieldNames ? '' : 'requests',
        subBuilder: DeviceAuthInfo.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListDeviceAuthResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListDeviceAuthResponse copyWith(
          void Function(ListDeviceAuthResponse) updates) =>
      super.copyWith((message) => updates(message as ListDeviceAuthResponse))
          as ListDeviceAuthResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use ListDeviceAuthResponse() / ListDeviceAuthResponse.new instead')
  static ListDeviceAuthResponse create() => ListDeviceAuthResponse._();
  static $pb.GeneratedMessage $_createMessage() => ListDeviceAuthResponse._();
  @$core.override
  ListDeviceAuthResponse createEmptyInstance() => ListDeviceAuthResponse._();
  @$core.pragma('dart2js:noInline')
  static ListDeviceAuthResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<ListDeviceAuthResponse>(
          ListDeviceAuthResponse.$_createMessage);
  static ListDeviceAuthResponse? _defaultInstance;

  @$pb.TagNumber(1)
  $pb.PbList<DeviceAuthInfo> get requests => $_getList(0);
}

class ApproveDeviceAuthRequest extends $pb.GeneratedMessage {
  factory ApproveDeviceAuthRequest({
    $core.String? userCode,
  }) {
    final result = ApproveDeviceAuthRequest._();
    if (userCode != null) result.userCode = userCode;
    return result;
  }

  ApproveDeviceAuthRequest._();

  factory ApproveDeviceAuthRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ApproveDeviceAuthRequest()..mergeFromBuffer(data, registry);
  factory ApproveDeviceAuthRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ApproveDeviceAuthRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ApproveDeviceAuthRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ApproveDeviceAuthRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'userCode')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ApproveDeviceAuthRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ApproveDeviceAuthRequest copyWith(
          void Function(ApproveDeviceAuthRequest) updates) =>
      super.copyWith((message) => updates(message as ApproveDeviceAuthRequest))
          as ApproveDeviceAuthRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use ApproveDeviceAuthRequest() / ApproveDeviceAuthRequest.new instead')
  static ApproveDeviceAuthRequest create() => ApproveDeviceAuthRequest._();
  static $pb.GeneratedMessage $_createMessage() => ApproveDeviceAuthRequest._();
  @$core.override
  ApproveDeviceAuthRequest createEmptyInstance() =>
      ApproveDeviceAuthRequest._();
  @$core.pragma('dart2js:noInline')
  static ApproveDeviceAuthRequest getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<ApproveDeviceAuthRequest>(
          ApproveDeviceAuthRequest.$_createMessage);
  static ApproveDeviceAuthRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get userCode => $_getSZ(0);
  @$pb.TagNumber(1)
  set userCode($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasUserCode() => $_has(0);
  @$pb.TagNumber(1)
  void clearUserCode() => $_clearField(1);
}

class ApproveDeviceAuthResponse extends $pb.GeneratedMessage {
  factory ApproveDeviceAuthResponse() => ApproveDeviceAuthResponse._();

  ApproveDeviceAuthResponse._();

  factory ApproveDeviceAuthResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ApproveDeviceAuthResponse()..mergeFromBuffer(data, registry);
  factory ApproveDeviceAuthResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ApproveDeviceAuthResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ApproveDeviceAuthResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ApproveDeviceAuthResponse.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ApproveDeviceAuthResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ApproveDeviceAuthResponse copyWith(
          void Function(ApproveDeviceAuthResponse) updates) =>
      super.copyWith((message) => updates(message as ApproveDeviceAuthResponse))
          as ApproveDeviceAuthResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use ApproveDeviceAuthResponse() / ApproveDeviceAuthResponse.new instead')
  static ApproveDeviceAuthResponse create() => ApproveDeviceAuthResponse._();
  static $pb.GeneratedMessage $_createMessage() =>
      ApproveDeviceAuthResponse._();
  @$core.override
  ApproveDeviceAuthResponse createEmptyInstance() =>
      ApproveDeviceAuthResponse._();
  @$core.pragma('dart2js:noInline')
  static ApproveDeviceAuthResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<ApproveDeviceAuthResponse>(
          ApproveDeviceAuthResponse.$_createMessage);
  static ApproveDeviceAuthResponse? _defaultInstance;
}

class DenyDeviceAuthRequest extends $pb.GeneratedMessage {
  factory DenyDeviceAuthRequest({
    $core.String? userCode,
  }) {
    final result = DenyDeviceAuthRequest._();
    if (userCode != null) result.userCode = userCode;
    return result;
  }

  DenyDeviceAuthRequest._();

  factory DenyDeviceAuthRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      DenyDeviceAuthRequest()..mergeFromBuffer(data, registry);
  factory DenyDeviceAuthRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      DenyDeviceAuthRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'DenyDeviceAuthRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: DenyDeviceAuthRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'userCode')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  DenyDeviceAuthRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  DenyDeviceAuthRequest copyWith(
          void Function(DenyDeviceAuthRequest) updates) =>
      super.copyWith((message) => updates(message as DenyDeviceAuthRequest))
          as DenyDeviceAuthRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use DenyDeviceAuthRequest() / DenyDeviceAuthRequest.new instead')
  static DenyDeviceAuthRequest create() => DenyDeviceAuthRequest._();
  static $pb.GeneratedMessage $_createMessage() => DenyDeviceAuthRequest._();
  @$core.override
  DenyDeviceAuthRequest createEmptyInstance() => DenyDeviceAuthRequest._();
  @$core.pragma('dart2js:noInline')
  static DenyDeviceAuthRequest getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<DenyDeviceAuthRequest>(
          DenyDeviceAuthRequest.$_createMessage);
  static DenyDeviceAuthRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get userCode => $_getSZ(0);
  @$pb.TagNumber(1)
  set userCode($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasUserCode() => $_has(0);
  @$pb.TagNumber(1)
  void clearUserCode() => $_clearField(1);
}

class DenyDeviceAuthResponse extends $pb.GeneratedMessage {
  factory DenyDeviceAuthResponse() => DenyDeviceAuthResponse._();

  DenyDeviceAuthResponse._();

  factory DenyDeviceAuthResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      DenyDeviceAuthResponse()..mergeFromBuffer(data, registry);
  factory DenyDeviceAuthResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      DenyDeviceAuthResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'DenyDeviceAuthResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: DenyDeviceAuthResponse.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  DenyDeviceAuthResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  DenyDeviceAuthResponse copyWith(
          void Function(DenyDeviceAuthResponse) updates) =>
      super.copyWith((message) => updates(message as DenyDeviceAuthResponse))
          as DenyDeviceAuthResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use DenyDeviceAuthResponse() / DenyDeviceAuthResponse.new instead')
  static DenyDeviceAuthResponse create() => DenyDeviceAuthResponse._();
  static $pb.GeneratedMessage $_createMessage() => DenyDeviceAuthResponse._();
  @$core.override
  DenyDeviceAuthResponse createEmptyInstance() => DenyDeviceAuthResponse._();
  @$core.pragma('dart2js:noInline')
  static DenyDeviceAuthResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<DenyDeviceAuthResponse>(
          DenyDeviceAuthResponse.$_createMessage);
  static DenyDeviceAuthResponse? _defaultInstance;
}

class ListDevicesRequest extends $pb.GeneratedMessage {
  factory ListDevicesRequest() => ListDevicesRequest._();

  ListDevicesRequest._();

  factory ListDevicesRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListDevicesRequest()..mergeFromBuffer(data, registry);
  factory ListDevicesRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListDevicesRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ListDevicesRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ListDevicesRequest.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListDevicesRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListDevicesRequest copyWith(void Function(ListDevicesRequest) updates) =>
      super.copyWith((message) => updates(message as ListDevicesRequest))
          as ListDevicesRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use ListDevicesRequest() / ListDevicesRequest.new instead')
  static ListDevicesRequest create() => ListDevicesRequest._();
  static $pb.GeneratedMessage $_createMessage() => ListDevicesRequest._();
  @$core.override
  ListDevicesRequest createEmptyInstance() => ListDevicesRequest._();
  @$core.pragma('dart2js:noInline')
  static ListDevicesRequest getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<ListDevicesRequest>(
          ListDevicesRequest.$_createMessage);
  static ListDevicesRequest? _defaultInstance;
}

class ListDevicesResponse extends $pb.GeneratedMessage {
  factory ListDevicesResponse({
    $core.Iterable<DeviceInfo>? devices,
  }) {
    final result = ListDevicesResponse._();
    if (devices != null) result.devices.addAll(devices);
    return result;
  }

  ListDevicesResponse._();

  factory ListDevicesResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListDevicesResponse()..mergeFromBuffer(data, registry);
  factory ListDevicesResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListDevicesResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ListDevicesResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ListDevicesResponse.$_createMessage)
    ..pPM<DeviceInfo>(1, _omitFieldNames ? '' : 'devices',
        subBuilder: DeviceInfo.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListDevicesResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListDevicesResponse copyWith(void Function(ListDevicesResponse) updates) =>
      super.copyWith((message) => updates(message as ListDevicesResponse))
          as ListDevicesResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core
      .Deprecated('Use ListDevicesResponse() / ListDevicesResponse.new instead')
  static ListDevicesResponse create() => ListDevicesResponse._();
  static $pb.GeneratedMessage $_createMessage() => ListDevicesResponse._();
  @$core.override
  ListDevicesResponse createEmptyInstance() => ListDevicesResponse._();
  @$core.pragma('dart2js:noInline')
  static ListDevicesResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<ListDevicesResponse>(
          ListDevicesResponse.$_createMessage);
  static ListDevicesResponse? _defaultInstance;

  @$pb.TagNumber(1)
  $pb.PbList<DeviceInfo> get devices => $_getList(0);
}

class RevokeDeviceRequest extends $pb.GeneratedMessage {
  factory RevokeDeviceRequest({
    $core.String? id,
  }) {
    final result = RevokeDeviceRequest._();
    if (id != null) result.id = id;
    return result;
  }

  RevokeDeviceRequest._();

  factory RevokeDeviceRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      RevokeDeviceRequest()..mergeFromBuffer(data, registry);
  factory RevokeDeviceRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      RevokeDeviceRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'RevokeDeviceRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: RevokeDeviceRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'id')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  RevokeDeviceRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  RevokeDeviceRequest copyWith(void Function(RevokeDeviceRequest) updates) =>
      super.copyWith((message) => updates(message as RevokeDeviceRequest))
          as RevokeDeviceRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core
      .Deprecated('Use RevokeDeviceRequest() / RevokeDeviceRequest.new instead')
  static RevokeDeviceRequest create() => RevokeDeviceRequest._();
  static $pb.GeneratedMessage $_createMessage() => RevokeDeviceRequest._();
  @$core.override
  RevokeDeviceRequest createEmptyInstance() => RevokeDeviceRequest._();
  @$core.pragma('dart2js:noInline')
  static RevokeDeviceRequest getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<RevokeDeviceRequest>(
          RevokeDeviceRequest.$_createMessage);
  static RevokeDeviceRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get id => $_getSZ(0);
  @$pb.TagNumber(1)
  set id($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasId() => $_has(0);
  @$pb.TagNumber(1)
  void clearId() => $_clearField(1);
}

class RevokeDeviceResponse extends $pb.GeneratedMessage {
  factory RevokeDeviceResponse({
    DeviceInfo? device,
  }) {
    final result = RevokeDeviceResponse._();
    if (device != null) result.device = device;
    return result;
  }

  RevokeDeviceResponse._();

  factory RevokeDeviceResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      RevokeDeviceResponse()..mergeFromBuffer(data, registry);
  factory RevokeDeviceResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      RevokeDeviceResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'RevokeDeviceResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: RevokeDeviceResponse.$_createMessage)
    ..aOM<DeviceInfo>(1, _omitFieldNames ? '' : 'device',
        subBuilder: DeviceInfo.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  RevokeDeviceResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  RevokeDeviceResponse copyWith(void Function(RevokeDeviceResponse) updates) =>
      super.copyWith((message) => updates(message as RevokeDeviceResponse))
          as RevokeDeviceResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use RevokeDeviceResponse() / RevokeDeviceResponse.new instead')
  static RevokeDeviceResponse create() => RevokeDeviceResponse._();
  static $pb.GeneratedMessage $_createMessage() => RevokeDeviceResponse._();
  @$core.override
  RevokeDeviceResponse createEmptyInstance() => RevokeDeviceResponse._();
  @$core.pragma('dart2js:noInline')
  static RevokeDeviceResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<RevokeDeviceResponse>(
          RevokeDeviceResponse.$_createMessage);
  static RevokeDeviceResponse? _defaultInstance;

  @$pb.TagNumber(1)
  DeviceInfo get device => $_getN(0);
  @$pb.TagNumber(1)
  set device(DeviceInfo value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasDevice() => $_has(0);
  @$pb.TagNumber(1)
  void clearDevice() => $_clearField(1);
  @$pb.TagNumber(1)
  DeviceInfo ensureDevice() => $_ensure(0);
}

class RenameDeviceRequest extends $pb.GeneratedMessage {
  factory RenameDeviceRequest({
    $core.String? id,
    $core.String? name,
  }) {
    final result = RenameDeviceRequest._();
    if (id != null) result.id = id;
    if (name != null) result.name = name;
    return result;
  }

  RenameDeviceRequest._();

  factory RenameDeviceRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      RenameDeviceRequest()..mergeFromBuffer(data, registry);
  factory RenameDeviceRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      RenameDeviceRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'RenameDeviceRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: RenameDeviceRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'id')
    ..aOS(2, _omitFieldNames ? '' : 'name')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  RenameDeviceRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  RenameDeviceRequest copyWith(void Function(RenameDeviceRequest) updates) =>
      super.copyWith((message) => updates(message as RenameDeviceRequest))
          as RenameDeviceRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core
      .Deprecated('Use RenameDeviceRequest() / RenameDeviceRequest.new instead')
  static RenameDeviceRequest create() => RenameDeviceRequest._();
  static $pb.GeneratedMessage $_createMessage() => RenameDeviceRequest._();
  @$core.override
  RenameDeviceRequest createEmptyInstance() => RenameDeviceRequest._();
  @$core.pragma('dart2js:noInline')
  static RenameDeviceRequest getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<RenameDeviceRequest>(
          RenameDeviceRequest.$_createMessage);
  static RenameDeviceRequest? _defaultInstance;

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
}

class RenameDeviceResponse extends $pb.GeneratedMessage {
  factory RenameDeviceResponse({
    DeviceInfo? device,
  }) {
    final result = RenameDeviceResponse._();
    if (device != null) result.device = device;
    return result;
  }

  RenameDeviceResponse._();

  factory RenameDeviceResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      RenameDeviceResponse()..mergeFromBuffer(data, registry);
  factory RenameDeviceResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      RenameDeviceResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'RenameDeviceResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: RenameDeviceResponse.$_createMessage)
    ..aOM<DeviceInfo>(1, _omitFieldNames ? '' : 'device',
        subBuilder: DeviceInfo.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  RenameDeviceResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  RenameDeviceResponse copyWith(void Function(RenameDeviceResponse) updates) =>
      super.copyWith((message) => updates(message as RenameDeviceResponse))
          as RenameDeviceResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use RenameDeviceResponse() / RenameDeviceResponse.new instead')
  static RenameDeviceResponse create() => RenameDeviceResponse._();
  static $pb.GeneratedMessage $_createMessage() => RenameDeviceResponse._();
  @$core.override
  RenameDeviceResponse createEmptyInstance() => RenameDeviceResponse._();
  @$core.pragma('dart2js:noInline')
  static RenameDeviceResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<RenameDeviceResponse>(
          RenameDeviceResponse.$_createMessage);
  static RenameDeviceResponse? _defaultInstance;

  @$pb.TagNumber(1)
  DeviceInfo get device => $_getN(0);
  @$pb.TagNumber(1)
  set device(DeviceInfo value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasDevice() => $_has(0);
  @$pb.TagNumber(1)
  void clearDevice() => $_clearField(1);
  @$pb.TagNumber(1)
  DeviceInfo ensureDevice() => $_ensure(0);
}

/// DeviceService connects new clients through approval by a trusted device.
/// A device registers a P-256 public key when it starts; once approved it
/// signs every request with the private half, which never leaves it.
class DeviceServiceApi {
  final $pb.RpcClient _client;

  DeviceServiceApi(this._client);

  /// StartDeviceAuth is unauthenticated and rate limited on the LAN.
  $async.Future<StartDeviceAuthResponse> startDeviceAuth(
          $pb.ClientContext? ctx, StartDeviceAuthRequest request) =>
      _client.invoke<StartDeviceAuthResponse>(ctx, 'DeviceService',
          'StartDeviceAuth', request, StartDeviceAuthResponse());

  /// PollDeviceAuth reports the outcome, and the approved device once.
  $async.Future<PollDeviceAuthResponse> pollDeviceAuth(
          $pb.ClientContext? ctx, PollDeviceAuthRequest request) =>
      _client.invoke<PollDeviceAuthResponse>(ctx, 'DeviceService',
          'PollDeviceAuth', request, PollDeviceAuthResponse());
  $async.Future<ListDeviceAuthResponse> listDeviceAuth(
          $pb.ClientContext? ctx, ListDeviceAuthRequest request) =>
      _client.invoke<ListDeviceAuthResponse>(ctx, 'DeviceService',
          'ListDeviceAuth', request, ListDeviceAuthResponse());
  $async.Future<ApproveDeviceAuthResponse> approveDeviceAuth(
          $pb.ClientContext? ctx, ApproveDeviceAuthRequest request) =>
      _client.invoke<ApproveDeviceAuthResponse>(ctx, 'DeviceService',
          'ApproveDeviceAuth', request, ApproveDeviceAuthResponse());
  $async.Future<DenyDeviceAuthResponse> denyDeviceAuth(
          $pb.ClientContext? ctx, DenyDeviceAuthRequest request) =>
      _client.invoke<DenyDeviceAuthResponse>(ctx, 'DeviceService',
          'DenyDeviceAuth', request, DenyDeviceAuthResponse());
  $async.Future<ListDevicesResponse> listDevices(
          $pb.ClientContext? ctx, ListDevicesRequest request) =>
      _client.invoke<ListDevicesResponse>(
          ctx, 'DeviceService', 'ListDevices', request, ListDevicesResponse());
  $async.Future<RevokeDeviceResponse> revokeDevice(
          $pb.ClientContext? ctx, RevokeDeviceRequest request) =>
      _client.invoke<RevokeDeviceResponse>(ctx, 'DeviceService', 'RevokeDevice',
          request, RevokeDeviceResponse());
  $async.Future<RenameDeviceResponse> renameDevice(
          $pb.ClientContext? ctx, RenameDeviceRequest request) =>
      _client.invoke<RenameDeviceResponse>(ctx, 'DeviceService', 'RenameDevice',
          request, RenameDeviceResponse());
}

const $core.bool _omitFieldNames =
    $core.bool.fromEnvironment('protobuf.omit_field_names');
const $core.bool _omitMessageNames =
    $core.bool.fromEnvironment('protobuf.omit_message_names');
