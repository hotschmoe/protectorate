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

import 'package:protobuf/protobuf.dart' as $pb;

export 'package:protobuf/protobuf.dart' show GeneratedMessageGenericExtensions;

class Sleeve extends $pb.GeneratedMessage {
  factory Sleeve({
    $core.String? id,
    $core.String? name,
    $core.String? harness,
    $core.String? state,
  }) {
    final result = Sleeve._();
    if (id != null) result.id = id;
    if (name != null) result.name = name;
    if (harness != null) result.harness = harness;
    if (state != null) result.state = state;
    return result;
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
    ..aOS(1, _omitFieldNames ? '' : 'id')
    ..aOS(2, _omitFieldNames ? '' : 'name')
    ..aOS(3, _omitFieldNames ? '' : 'harness')
    ..aOS(4, _omitFieldNames ? '' : 'state')
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
  $core.String get harness => $_getSZ(2);
  @$pb.TagNumber(3)
  set harness($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasHarness() => $_has(2);
  @$pb.TagNumber(3)
  void clearHarness() => $_clearField(3);

  @$pb.TagNumber(4)
  $core.String get state => $_getSZ(3);
  @$pb.TagNumber(4)
  set state($core.String value) => $_setString(3, value);
  @$pb.TagNumber(4)
  $core.bool hasState() => $_has(3);
  @$pb.TagNumber(4)
  void clearState() => $_clearField(4);
}

class ListRequest extends $pb.GeneratedMessage {
  factory ListRequest() => ListRequest._();

  ListRequest._();

  factory ListRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListRequest()..mergeFromBuffer(data, registry);
  factory ListRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ListRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ListRequest.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListRequest copyWith(void Function(ListRequest) updates) =>
      super.copyWith((message) => updates(message as ListRequest))
          as ListRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use ListRequest() / ListRequest.new instead')
  static ListRequest create() => ListRequest._();
  static $pb.GeneratedMessage $_createMessage() => ListRequest._();
  @$core.override
  ListRequest createEmptyInstance() => ListRequest._();
  @$core.pragma('dart2js:noInline')
  static ListRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<ListRequest>(
          ListRequest.$_createMessage);
  static ListRequest? _defaultInstance;
}

class ListResponse extends $pb.GeneratedMessage {
  factory ListResponse({
    $core.Iterable<Sleeve>? sleeves,
  }) {
    final result = ListResponse._();
    if (sleeves != null) result.sleeves.addAll(sleeves);
    return result;
  }

  ListResponse._();

  factory ListResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListResponse()..mergeFromBuffer(data, registry);
  factory ListResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ListResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ListResponse.$_createMessage)
    ..pPM<Sleeve>(1, _omitFieldNames ? '' : 'sleeves',
        subBuilder: Sleeve.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListResponse copyWith(void Function(ListResponse) updates) =>
      super.copyWith((message) => updates(message as ListResponse))
          as ListResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use ListResponse() / ListResponse.new instead')
  static ListResponse create() => ListResponse._();
  static $pb.GeneratedMessage $_createMessage() => ListResponse._();
  @$core.override
  ListResponse createEmptyInstance() => ListResponse._();
  @$core.pragma('dart2js:noInline')
  static ListResponse getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<ListResponse>(
          ListResponse.$_createMessage);
  static ListResponse? _defaultInstance;

  @$pb.TagNumber(1)
  $pb.PbList<Sleeve> get sleeves => $_getList(0);
}

class SpawnRequest extends $pb.GeneratedMessage {
  factory SpawnRequest({
    $core.String? name,
    $core.String? harness,
  }) {
    final result = SpawnRequest._();
    if (name != null) result.name = name;
    if (harness != null) result.harness = harness;
    return result;
  }

  SpawnRequest._();

  factory SpawnRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      SpawnRequest()..mergeFromBuffer(data, registry);
  factory SpawnRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      SpawnRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'SpawnRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: SpawnRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'name')
    ..aOS(2, _omitFieldNames ? '' : 'harness')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  SpawnRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  SpawnRequest copyWith(void Function(SpawnRequest) updates) =>
      super.copyWith((message) => updates(message as SpawnRequest))
          as SpawnRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use SpawnRequest() / SpawnRequest.new instead')
  static SpawnRequest create() => SpawnRequest._();
  static $pb.GeneratedMessage $_createMessage() => SpawnRequest._();
  @$core.override
  SpawnRequest createEmptyInstance() => SpawnRequest._();
  @$core.pragma('dart2js:noInline')
  static SpawnRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<SpawnRequest>(
          SpawnRequest.$_createMessage);
  static SpawnRequest? _defaultInstance;

  /// Human-readable sleeve name, unique per hub.
  @$pb.TagNumber(1)
  $core.String get name => $_getSZ(0);
  @$pb.TagNumber(1)
  set name($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasName() => $_has(0);
  @$pb.TagNumber(1)
  void clearName() => $_clearField(1);

  /// Agent harness to run: claude, gemini, opencode.
  @$pb.TagNumber(2)
  $core.String get harness => $_getSZ(1);
  @$pb.TagNumber(2)
  set harness($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasHarness() => $_has(1);
  @$pb.TagNumber(2)
  void clearHarness() => $_clearField(2);
}

class SpawnResponse extends $pb.GeneratedMessage {
  factory SpawnResponse({
    Sleeve? sleeve,
  }) {
    final result = SpawnResponse._();
    if (sleeve != null) result.sleeve = sleeve;
    return result;
  }

  SpawnResponse._();

  factory SpawnResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      SpawnResponse()..mergeFromBuffer(data, registry);
  factory SpawnResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      SpawnResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'SpawnResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: SpawnResponse.$_createMessage)
    ..aOM<Sleeve>(1, _omitFieldNames ? '' : 'sleeve',
        subBuilder: Sleeve.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  SpawnResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  SpawnResponse copyWith(void Function(SpawnResponse) updates) =>
      super.copyWith((message) => updates(message as SpawnResponse))
          as SpawnResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use SpawnResponse() / SpawnResponse.new instead')
  static SpawnResponse create() => SpawnResponse._();
  static $pb.GeneratedMessage $_createMessage() => SpawnResponse._();
  @$core.override
  SpawnResponse createEmptyInstance() => SpawnResponse._();
  @$core.pragma('dart2js:noInline')
  static SpawnResponse getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<SpawnResponse>(
          SpawnResponse.$_createMessage);
  static SpawnResponse? _defaultInstance;

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

/// Manages sleeves.
class SleeveServiceApi {
  final $pb.RpcClient _client;

  SleeveServiceApi(this._client);

  /// List returns all sleeves.
  $async.Future<ListResponse> list(
          $pb.ClientContext? ctx, ListRequest request) =>
      _client.invoke<ListResponse>(
          ctx, 'SleeveService', 'List', request, ListResponse());

  /// Spawn creates a sleeve running the given harness.
  $async.Future<SpawnResponse> spawn(
          $pb.ClientContext? ctx, SpawnRequest request) =>
      _client.invoke<SpawnResponse>(
          ctx, 'SleeveService', 'Spawn', request, SpawnResponse());
}

const $core.bool _omitFieldNames =
    $core.bool.fromEnvironment('protobuf.omit_field_names');
const $core.bool _omitMessageNames =
    $core.bool.fromEnvironment('protobuf.omit_message_names');
