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

/// DeviceInfo describes a device. Its id is derived from its token and names
/// it in audit lines.
class DeviceInfo extends $pb.GeneratedMessage {
  factory DeviceInfo({
    $core.String? id,
    $core.String? name,
    $0.Timestamp? addedAt,
    $core.String? addedBy,
    $0.Timestamp? lastUsedAt,
  }) {
    final result = DeviceInfo._();
    if (id != null) result.id = id;
    if (name != null) result.name = name;
    if (addedAt != null) result.addedAt = addedAt;
    if (addedBy != null) result.addedBy = addedBy;
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
    ..aOM<$0.Timestamp>(3, _omitFieldNames ? '' : 'addedAt',
        subBuilder: $0.Timestamp.$_createMessage)
    ..aOS(4, _omitFieldNames ? '' : 'addedBy')
    ..aOM<$0.Timestamp>(5, _omitFieldNames ? '' : 'lastUsedAt',
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
  $0.Timestamp get addedAt => $_getN(2);
  @$pb.TagNumber(3)
  set addedAt($0.Timestamp value) => $_setField(3, value);
  @$pb.TagNumber(3)
  $core.bool hasAddedAt() => $_has(2);
  @$pb.TagNumber(3)
  void clearAddedAt() => $_clearField(3);
  @$pb.TagNumber(3)
  $0.Timestamp ensureAddedAt() => $_ensure(2);

  @$pb.TagNumber(4)
  $core.String get addedBy => $_getSZ(3);
  @$pb.TagNumber(4)
  set addedBy($core.String value) => $_setString(3, value);
  @$pb.TagNumber(4)
  $core.bool hasAddedBy() => $_has(3);
  @$pb.TagNumber(4)
  void clearAddedBy() => $_clearField(4);

  /// last_used_at is persisted at most about once a minute.
  @$pb.TagNumber(5)
  $0.Timestamp get lastUsedAt => $_getN(4);
  @$pb.TagNumber(5)
  set lastUsedAt($0.Timestamp value) => $_setField(5, value);
  @$pb.TagNumber(5)
  $core.bool hasLastUsedAt() => $_has(4);
  @$pb.TagNumber(5)
  void clearLastUsedAt() => $_clearField(5);
  @$pb.TagNumber(5)
  $0.Timestamp ensureLastUsedAt() => $_ensure(4);
}

class AddDeviceRequest extends $pb.GeneratedMessage {
  factory AddDeviceRequest({
    $core.String? name,
  }) {
    final result = AddDeviceRequest._();
    if (name != null) result.name = name;
    return result;
  }

  AddDeviceRequest._();

  factory AddDeviceRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      AddDeviceRequest()..mergeFromBuffer(data, registry);
  factory AddDeviceRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      AddDeviceRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'AddDeviceRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: AddDeviceRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'name')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  AddDeviceRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  AddDeviceRequest copyWith(void Function(AddDeviceRequest) updates) =>
      super.copyWith((message) => updates(message as AddDeviceRequest))
          as AddDeviceRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use AddDeviceRequest() / AddDeviceRequest.new instead')
  static AddDeviceRequest create() => AddDeviceRequest._();
  static $pb.GeneratedMessage $_createMessage() => AddDeviceRequest._();
  @$core.override
  AddDeviceRequest createEmptyInstance() => AddDeviceRequest._();
  @$core.pragma('dart2js:noInline')
  static AddDeviceRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<AddDeviceRequest>(
          AddDeviceRequest.$_createMessage);
  static AddDeviceRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get name => $_getSZ(0);
  @$pb.TagNumber(1)
  set name($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasName() => $_has(0);
  @$pb.TagNumber(1)
  void clearName() => $_clearField(1);
}

class AddDeviceResponse extends $pb.GeneratedMessage {
  factory AddDeviceResponse({
    DeviceInfo? device,
    $core.String? token,
  }) {
    final result = AddDeviceResponse._();
    if (device != null) result.device = device;
    if (token != null) result.token = token;
    return result;
  }

  AddDeviceResponse._();

  factory AddDeviceResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      AddDeviceResponse()..mergeFromBuffer(data, registry);
  factory AddDeviceResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      AddDeviceResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'AddDeviceResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: AddDeviceResponse.$_createMessage)
    ..aOM<DeviceInfo>(1, _omitFieldNames ? '' : 'device',
        subBuilder: DeviceInfo.$_createMessage)
    ..aOS(2, _omitFieldNames ? '' : 'token')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  AddDeviceResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  AddDeviceResponse copyWith(void Function(AddDeviceResponse) updates) =>
      super.copyWith((message) => updates(message as AddDeviceResponse))
          as AddDeviceResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use AddDeviceResponse() / AddDeviceResponse.new instead')
  static AddDeviceResponse create() => AddDeviceResponse._();
  static $pb.GeneratedMessage $_createMessage() => AddDeviceResponse._();
  @$core.override
  AddDeviceResponse createEmptyInstance() => AddDeviceResponse._();
  @$core.pragma('dart2js:noInline')
  static AddDeviceResponse getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<AddDeviceResponse>(
          AddDeviceResponse.$_createMessage);
  static AddDeviceResponse? _defaultInstance;

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

  /// token is shown only here.
  @$pb.TagNumber(2)
  $core.String get token => $_getSZ(1);
  @$pb.TagNumber(2)
  set token($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasToken() => $_has(1);
  @$pb.TagNumber(2)
  void clearToken() => $_clearField(2);
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

/// DeviceService manages the clients that may use the hub. Adding a device
/// returns its bearer token once; the hub keeps only a hash of it.
class DeviceServiceApi {
  final $pb.RpcClient _client;

  DeviceServiceApi(this._client);

  $async.Future<AddDeviceResponse> addDevice(
          $pb.ClientContext? ctx, AddDeviceRequest request) =>
      _client.invoke<AddDeviceResponse>(
          ctx, 'DeviceService', 'AddDevice', request, AddDeviceResponse());
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
