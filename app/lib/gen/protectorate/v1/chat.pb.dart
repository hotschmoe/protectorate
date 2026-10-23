// This is a generated file - do not edit.
//
// Generated from protectorate/v1/chat.proto.

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
    as $0;

import 'chat.pbenum.dart';

export 'package:protobuf/protobuf.dart' show GeneratedMessageGenericExtensions;

export 'chat.pbenum.dart';

class ModelSelection extends $pb.GeneratedMessage {
  factory ModelSelection({
    $core.String? model,
    $core.String? effort,
  }) {
    final result = ModelSelection._();
    if (model != null) result.model = model;
    if (effort != null) result.effort = effort;
    return result;
  }

  ModelSelection._();

  factory ModelSelection.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ModelSelection()..mergeFromBuffer(data, registry);
  factory ModelSelection.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ModelSelection()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ModelSelection',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ModelSelection.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'model')
    ..aOS(2, _omitFieldNames ? '' : 'effort')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ModelSelection clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ModelSelection copyWith(void Function(ModelSelection) updates) =>
      super.copyWith((message) => updates(message as ModelSelection))
          as ModelSelection;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use ModelSelection() / ModelSelection.new instead')
  static ModelSelection create() => ModelSelection._();
  static $pb.GeneratedMessage $_createMessage() => ModelSelection._();
  @$core.override
  ModelSelection createEmptyInstance() => ModelSelection._();
  @$core.pragma('dart2js:noInline')
  static ModelSelection getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<ModelSelection>(
          ModelSelection.$_createMessage);
  static ModelSelection? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get model => $_getSZ(0);
  @$pb.TagNumber(1)
  set model($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasModel() => $_has(0);
  @$pb.TagNumber(1)
  void clearModel() => $_clearField(1);

  /// Native effort value, validated against the selected model's catalog.
  @$pb.TagNumber(2)
  $core.String get effort => $_getSZ(1);
  @$pb.TagNumber(2)
  set effort($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasEffort() => $_has(1);
  @$pb.TagNumber(2)
  void clearEffort() => $_clearField(2);
}

class ModelSettings extends $pb.GeneratedMessage {
  factory ModelSettings({
    ModelSelection? requested,
    ModelSelection? effective,
    ModelSelection? nativeDefaults,
  }) {
    final result = ModelSettings._();
    if (requested != null) result.requested = requested;
    if (effective != null) result.effective = effective;
    if (nativeDefaults != null) result.nativeDefaults = nativeDefaults;
    return result;
  }

  ModelSettings._();

  factory ModelSettings.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ModelSettings()..mergeFromBuffer(data, registry);
  factory ModelSettings.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ModelSettings()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ModelSettings',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ModelSettings.$_createMessage)
    ..aOM<ModelSelection>(1, _omitFieldNames ? '' : 'requested',
        subBuilder: ModelSelection.$_createMessage)
    ..aOM<ModelSelection>(2, _omitFieldNames ? '' : 'effective',
        subBuilder: ModelSelection.$_createMessage)
    ..aOM<ModelSelection>(3, _omitFieldNames ? '' : 'nativeDefaults',
        subBuilder: ModelSelection.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ModelSettings clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ModelSettings copyWith(void Function(ModelSettings) updates) =>
      super.copyWith((message) => updates(message as ModelSettings))
          as ModelSettings;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use ModelSettings() / ModelSettings.new instead')
  static ModelSettings create() => ModelSettings._();
  static $pb.GeneratedMessage $_createMessage() => ModelSettings._();
  @$core.override
  ModelSettings createEmptyInstance() => ModelSettings._();
  @$core.pragma('dart2js:noInline')
  static ModelSettings getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<ModelSettings>(
          ModelSettings.$_createMessage);
  static ModelSettings? _defaultInstance;

  @$pb.TagNumber(1)
  ModelSelection get requested => $_getN(0);
  @$pb.TagNumber(1)
  set requested(ModelSelection value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasRequested() => $_has(0);
  @$pb.TagNumber(1)
  void clearRequested() => $_clearField(1);
  @$pb.TagNumber(1)
  ModelSelection ensureRequested() => $_ensure(0);

  @$pb.TagNumber(2)
  ModelSelection get effective => $_getN(1);
  @$pb.TagNumber(2)
  set effective(ModelSelection value) => $_setField(2, value);
  @$pb.TagNumber(2)
  $core.bool hasEffective() => $_has(1);
  @$pb.TagNumber(2)
  void clearEffective() => $_clearField(2);
  @$pb.TagNumber(2)
  ModelSelection ensureEffective() => $_ensure(1);

  /// Native session defaults captured before any explicit selection.
  @$pb.TagNumber(3)
  ModelSelection get nativeDefaults => $_getN(2);
  @$pb.TagNumber(3)
  set nativeDefaults(ModelSelection value) => $_setField(3, value);
  @$pb.TagNumber(3)
  $core.bool hasNativeDefaults() => $_has(2);
  @$pb.TagNumber(3)
  void clearNativeDefaults() => $_clearField(3);
  @$pb.TagNumber(3)
  ModelSelection ensureNativeDefaults() => $_ensure(2);
}

class ModelOption extends $pb.GeneratedMessage {
  factory ModelOption({
    $core.String? model,
    $core.String? name,
    $core.Iterable<$core.String>? efforts,
    $core.String? defaultEffort,
    $core.bool? isDefault,
  }) {
    final result = ModelOption._();
    if (model != null) result.model = model;
    if (name != null) result.name = name;
    if (efforts != null) result.efforts.addAll(efforts);
    if (defaultEffort != null) result.defaultEffort = defaultEffort;
    if (isDefault != null) result.isDefault = isDefault;
    return result;
  }

  ModelOption._();

  factory ModelOption.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ModelOption()..mergeFromBuffer(data, registry);
  factory ModelOption.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ModelOption()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ModelOption',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ModelOption.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'model')
    ..aOS(2, _omitFieldNames ? '' : 'name')
    ..pPS(3, _omitFieldNames ? '' : 'efforts')
    ..aOS(4, _omitFieldNames ? '' : 'defaultEffort')
    ..aOB(5, _omitFieldNames ? '' : 'isDefault')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ModelOption clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ModelOption copyWith(void Function(ModelOption) updates) =>
      super.copyWith((message) => updates(message as ModelOption))
          as ModelOption;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use ModelOption() / ModelOption.new instead')
  static ModelOption create() => ModelOption._();
  static $pb.GeneratedMessage $_createMessage() => ModelOption._();
  @$core.override
  ModelOption createEmptyInstance() => ModelOption._();
  @$core.pragma('dart2js:noInline')
  static ModelOption getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<ModelOption>(
          ModelOption.$_createMessage);
  static ModelOption? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get model => $_getSZ(0);
  @$pb.TagNumber(1)
  set model($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasModel() => $_has(0);
  @$pb.TagNumber(1)
  void clearModel() => $_clearField(1);

  @$pb.TagNumber(2)
  $core.String get name => $_getSZ(1);
  @$pb.TagNumber(2)
  set name($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasName() => $_has(1);
  @$pb.TagNumber(2)
  void clearName() => $_clearField(2);

  @$pb.TagNumber(3)
  $pb.PbList<$core.String> get efforts => $_getList(2);

  @$pb.TagNumber(4)
  $core.String get defaultEffort => $_getSZ(3);
  @$pb.TagNumber(4)
  set defaultEffort($core.String value) => $_setString(3, value);
  @$pb.TagNumber(4)
  $core.bool hasDefaultEffort() => $_has(3);
  @$pb.TagNumber(4)
  void clearDefaultEffort() => $_clearField(4);

  @$pb.TagNumber(5)
  $core.bool get isDefault => $_getBF(4);
  @$pb.TagNumber(5)
  set isDefault($core.bool value) => $_setBool(4, value);
  @$pb.TagNumber(5)
  $core.bool hasIsDefault() => $_has(4);
  @$pb.TagNumber(5)
  void clearIsDefault() => $_clearField(5);
}

class ListModelsRequest extends $pb.GeneratedMessage {
  factory ListModelsRequest({
    $core.String? sleeve,
    $core.String? harness,
  }) {
    final result = ListModelsRequest._();
    if (sleeve != null) result.sleeve = sleeve;
    if (harness != null) result.harness = harness;
    return result;
  }

  ListModelsRequest._();

  factory ListModelsRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListModelsRequest()..mergeFromBuffer(data, registry);
  factory ListModelsRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListModelsRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ListModelsRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ListModelsRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'sleeve')
    ..aOS(2, _omitFieldNames ? '' : 'harness')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListModelsRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListModelsRequest copyWith(void Function(ListModelsRequest) updates) =>
      super.copyWith((message) => updates(message as ListModelsRequest))
          as ListModelsRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use ListModelsRequest() / ListModelsRequest.new instead')
  static ListModelsRequest create() => ListModelsRequest._();
  static $pb.GeneratedMessage $_createMessage() => ListModelsRequest._();
  @$core.override
  ListModelsRequest createEmptyInstance() => ListModelsRequest._();
  @$core.pragma('dart2js:noInline')
  static ListModelsRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<ListModelsRequest>(
          ListModelsRequest.$_createMessage);
  static ListModelsRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get sleeve => $_getSZ(0);
  @$pb.TagNumber(1)
  set sleeve($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasSleeve() => $_has(0);
  @$pb.TagNumber(1)
  void clearSleeve() => $_clearField(1);

  /// claude, codex, grok or pi; used when sleeve is empty, e.g. at creation.
  @$pb.TagNumber(2)
  $core.String get harness => $_getSZ(1);
  @$pb.TagNumber(2)
  set harness($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasHarness() => $_has(1);
  @$pb.TagNumber(2)
  void clearHarness() => $_clearField(2);
}

class ListModelsResponse extends $pb.GeneratedMessage {
  factory ListModelsResponse({
    $core.bool? supported,
    $core.String? reason,
    $core.Iterable<ModelOption>? models,
    $core.bool? liveUpdate,
  }) {
    final result = ListModelsResponse._();
    if (supported != null) result.supported = supported;
    if (reason != null) result.reason = reason;
    if (models != null) result.models.addAll(models);
    if (liveUpdate != null) result.liveUpdate = liveUpdate;
    return result;
  }

  ListModelsResponse._();

  factory ListModelsResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListModelsResponse()..mergeFromBuffer(data, registry);
  factory ListModelsResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListModelsResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ListModelsResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ListModelsResponse.$_createMessage)
    ..aOB(1, _omitFieldNames ? '' : 'supported')
    ..aOS(2, _omitFieldNames ? '' : 'reason')
    ..pPM<ModelOption>(3, _omitFieldNames ? '' : 'models',
        subBuilder: ModelOption.$_createMessage)
    ..aOB(4, _omitFieldNames ? '' : 'liveUpdate')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListModelsResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListModelsResponse copyWith(void Function(ListModelsResponse) updates) =>
      super.copyWith((message) => updates(message as ListModelsResponse))
          as ListModelsResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use ListModelsResponse() / ListModelsResponse.new instead')
  static ListModelsResponse create() => ListModelsResponse._();
  static $pb.GeneratedMessage $_createMessage() => ListModelsResponse._();
  @$core.override
  ListModelsResponse createEmptyInstance() => ListModelsResponse._();
  @$core.pragma('dart2js:noInline')
  static ListModelsResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<ListModelsResponse>(
          ListModelsResponse.$_createMessage);
  static ListModelsResponse? _defaultInstance;

  @$pb.TagNumber(1)
  $core.bool get supported => $_getBF(0);
  @$pb.TagNumber(1)
  set supported($core.bool value) => $_setBool(0, value);
  @$pb.TagNumber(1)
  $core.bool hasSupported() => $_has(0);
  @$pb.TagNumber(1)
  void clearSupported() => $_clearField(1);

  @$pb.TagNumber(2)
  $core.String get reason => $_getSZ(1);
  @$pb.TagNumber(2)
  set reason($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasReason() => $_has(1);
  @$pb.TagNumber(2)
  void clearReason() => $_clearField(2);

  @$pb.TagNumber(3)
  $pb.PbList<ModelOption> get models => $_getList(2);

  @$pb.TagNumber(4)
  $core.bool get liveUpdate => $_getBF(3);
  @$pb.TagNumber(4)
  set liveUpdate($core.bool value) => $_setBool(3, value);
  @$pb.TagNumber(4)
  $core.bool hasLiveUpdate() => $_has(3);
  @$pb.TagNumber(4)
  void clearLiveUpdate() => $_clearField(4);
}

class SetSelectionRequest extends $pb.GeneratedMessage {
  factory SetSelectionRequest({
    $core.String? sleeve,
    ModelSelection? selection,
  }) {
    final result = SetSelectionRequest._();
    if (sleeve != null) result.sleeve = sleeve;
    if (selection != null) result.selection = selection;
    return result;
  }

  SetSelectionRequest._();

  factory SetSelectionRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      SetSelectionRequest()..mergeFromBuffer(data, registry);
  factory SetSelectionRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      SetSelectionRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'SetSelectionRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: SetSelectionRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'sleeve')
    ..aOM<ModelSelection>(2, _omitFieldNames ? '' : 'selection',
        subBuilder: ModelSelection.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  SetSelectionRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  SetSelectionRequest copyWith(void Function(SetSelectionRequest) updates) =>
      super.copyWith((message) => updates(message as SetSelectionRequest))
          as SetSelectionRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core
      .Deprecated('Use SetSelectionRequest() / SetSelectionRequest.new instead')
  static SetSelectionRequest create() => SetSelectionRequest._();
  static $pb.GeneratedMessage $_createMessage() => SetSelectionRequest._();
  @$core.override
  SetSelectionRequest createEmptyInstance() => SetSelectionRequest._();
  @$core.pragma('dart2js:noInline')
  static SetSelectionRequest getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<SetSelectionRequest>(
          SetSelectionRequest.$_createMessage);
  static SetSelectionRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get sleeve => $_getSZ(0);
  @$pb.TagNumber(1)
  set sleeve($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasSleeve() => $_has(0);
  @$pb.TagNumber(1)
  void clearSleeve() => $_clearField(1);

  @$pb.TagNumber(2)
  ModelSelection get selection => $_getN(1);
  @$pb.TagNumber(2)
  set selection(ModelSelection value) => $_setField(2, value);
  @$pb.TagNumber(2)
  $core.bool hasSelection() => $_has(1);
  @$pb.TagNumber(2)
  void clearSelection() => $_clearField(2);
  @$pb.TagNumber(2)
  ModelSelection ensureSelection() => $_ensure(1);
}

class SetSelectionResponse extends $pb.GeneratedMessage {
  factory SetSelectionResponse({
    ChatStatus? chat,
  }) {
    final result = SetSelectionResponse._();
    if (chat != null) result.chat = chat;
    return result;
  }

  SetSelectionResponse._();

  factory SetSelectionResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      SetSelectionResponse()..mergeFromBuffer(data, registry);
  factory SetSelectionResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      SetSelectionResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'SetSelectionResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: SetSelectionResponse.$_createMessage)
    ..aOM<ChatStatus>(1, _omitFieldNames ? '' : 'chat',
        subBuilder: ChatStatus.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  SetSelectionResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  SetSelectionResponse copyWith(void Function(SetSelectionResponse) updates) =>
      super.copyWith((message) => updates(message as SetSelectionResponse))
          as SetSelectionResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use SetSelectionResponse() / SetSelectionResponse.new instead')
  static SetSelectionResponse create() => SetSelectionResponse._();
  static $pb.GeneratedMessage $_createMessage() => SetSelectionResponse._();
  @$core.override
  SetSelectionResponse createEmptyInstance() => SetSelectionResponse._();
  @$core.pragma('dart2js:noInline')
  static SetSelectionResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<SetSelectionResponse>(
          SetSelectionResponse.$_createMessage);
  static SetSelectionResponse? _defaultInstance;

  @$pb.TagNumber(1)
  ChatStatus get chat => $_getN(0);
  @$pb.TagNumber(1)
  set chat(ChatStatus value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasChat() => $_has(0);
  @$pb.TagNumber(1)
  void clearChat() => $_clearField(1);
  @$pb.TagNumber(1)
  ChatStatus ensureChat() => $_ensure(0);
}

/// ChatStatus is the live summary kept on the Sleeve.
class ChatStatus extends $pb.GeneratedMessage {
  factory ChatStatus({
    ChatState? state,
    $core.String? harnessSessionId,
    $core.String? model,
    $fixnum.Int64? contextTokens,
    $fixnum.Int64? contextWindow,
    $0.Timestamp? cacheExpiresAt,
    $fixnum.Int64? lastSeq,
    $0.Timestamp? lastActivityAt,
    $core.String? provider,
    $core.bool? localModel,
    $core.String? reasoningEffort,
    ModelSettings? settings,
    CacheTelemetry? cache,
  }) {
    final result = ChatStatus._();
    if (state != null) result.state = state;
    if (harnessSessionId != null) result.harnessSessionId = harnessSessionId;
    if (model != null) result.model = model;
    if (contextTokens != null) result.contextTokens = contextTokens;
    if (contextWindow != null) result.contextWindow = contextWindow;
    if (cacheExpiresAt != null) result.cacheExpiresAt = cacheExpiresAt;
    if (lastSeq != null) result.lastSeq = lastSeq;
    if (lastActivityAt != null) result.lastActivityAt = lastActivityAt;
    if (provider != null) result.provider = provider;
    if (localModel != null) result.localModel = localModel;
    if (reasoningEffort != null) result.reasoningEffort = reasoningEffort;
    if (settings != null) result.settings = settings;
    if (cache != null) result.cache = cache;
    return result;
  }

  ChatStatus._();

  factory ChatStatus.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ChatStatus()..mergeFromBuffer(data, registry);
  factory ChatStatus.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ChatStatus()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ChatStatus',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ChatStatus.$_createMessage)
    ..aE<ChatState>(1, _omitFieldNames ? '' : 'state',
        enumValues: ChatState.values)
    ..aOS(2, _omitFieldNames ? '' : 'harnessSessionId')
    ..aOS(3, _omitFieldNames ? '' : 'model')
    ..a<$fixnum.Int64>(
        4, _omitFieldNames ? '' : 'contextTokens', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..a<$fixnum.Int64>(
        5, _omitFieldNames ? '' : 'contextWindow', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..aOM<$0.Timestamp>(6, _omitFieldNames ? '' : 'cacheExpiresAt',
        subBuilder: $0.Timestamp.$_createMessage)
    ..a<$fixnum.Int64>(7, _omitFieldNames ? '' : 'lastSeq', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..aOM<$0.Timestamp>(8, _omitFieldNames ? '' : 'lastActivityAt',
        subBuilder: $0.Timestamp.$_createMessage)
    ..aOS(9, _omitFieldNames ? '' : 'provider')
    ..aOB(10, _omitFieldNames ? '' : 'localModel')
    ..aOS(11, _omitFieldNames ? '' : 'reasoningEffort')
    ..aOM<ModelSettings>(12, _omitFieldNames ? '' : 'settings',
        subBuilder: ModelSettings.$_createMessage)
    ..aOM<CacheTelemetry>(13, _omitFieldNames ? '' : 'cache',
        subBuilder: CacheTelemetry.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ChatStatus clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ChatStatus copyWith(void Function(ChatStatus) updates) =>
      super.copyWith((message) => updates(message as ChatStatus)) as ChatStatus;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use ChatStatus() / ChatStatus.new instead')
  static ChatStatus create() => ChatStatus._();
  static $pb.GeneratedMessage $_createMessage() => ChatStatus._();
  @$core.override
  ChatStatus createEmptyInstance() => ChatStatus._();
  @$core.pragma('dart2js:noInline')
  static ChatStatus getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<ChatStatus>(ChatStatus.$_createMessage);
  static ChatStatus? _defaultInstance;

  @$pb.TagNumber(1)
  ChatState get state => $_getN(0);
  @$pb.TagNumber(1)
  set state(ChatState value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasState() => $_has(0);
  @$pb.TagNumber(1)
  void clearState() => $_clearField(1);

  /// harness_session_id is the harness-native id used for resume.
  @$pb.TagNumber(2)
  $core.String get harnessSessionId => $_getSZ(1);
  @$pb.TagNumber(2)
  set harnessSessionId($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasHarnessSessionId() => $_has(1);
  @$pb.TagNumber(2)
  void clearHarnessSessionId() => $_clearField(2);

  @$pb.TagNumber(3)
  $core.String get model => $_getSZ(2);
  @$pb.TagNumber(3)
  set model($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasModel() => $_has(2);
  @$pb.TagNumber(3)
  void clearModel() => $_clearField(3);

  /// context_tokens is the prompt size of the last model call (what fills the
  /// window), not a sum over the turn.
  @$pb.TagNumber(4)
  $fixnum.Int64 get contextTokens => $_getI64(3);
  @$pb.TagNumber(4)
  set contextTokens($fixnum.Int64 value) => $_setInt64(3, value);
  @$pb.TagNumber(4)
  $core.bool hasContextTokens() => $_has(3);
  @$pb.TagNumber(4)
  void clearContextTokens() => $_clearField(4);

  @$pb.TagNumber(5)
  $fixnum.Int64 get contextWindow => $_getI64(4);
  @$pb.TagNumber(5)
  set contextWindow($fixnum.Int64 value) => $_setInt64(4, value);
  @$pb.TagNumber(5)
  $core.bool hasContextWindow() => $_has(4);
  @$pb.TagNumber(5)
  void clearContextWindow() => $_clearField(5);

  /// cache_expires_at is when the harness's prompt cache goes cold if no new
  /// turn starts, when the harness reports it.
  @$pb.TagNumber(6)
  $0.Timestamp get cacheExpiresAt => $_getN(5);
  @$pb.TagNumber(6)
  set cacheExpiresAt($0.Timestamp value) => $_setField(6, value);
  @$pb.TagNumber(6)
  $core.bool hasCacheExpiresAt() => $_has(5);
  @$pb.TagNumber(6)
  void clearCacheExpiresAt() => $_clearField(6);
  @$pb.TagNumber(6)
  $0.Timestamp ensureCacheExpiresAt() => $_ensure(5);

  @$pb.TagNumber(7)
  $fixnum.Int64 get lastSeq => $_getI64(6);
  @$pb.TagNumber(7)
  set lastSeq($fixnum.Int64 value) => $_setInt64(6, value);
  @$pb.TagNumber(7)
  $core.bool hasLastSeq() => $_has(6);
  @$pb.TagNumber(7)
  void clearLastSeq() => $_clearField(7);

  @$pb.TagNumber(8)
  $0.Timestamp get lastActivityAt => $_getN(7);
  @$pb.TagNumber(8)
  set lastActivityAt($0.Timestamp value) => $_setField(8, value);
  @$pb.TagNumber(8)
  $core.bool hasLastActivityAt() => $_has(7);
  @$pb.TagNumber(8)
  void clearLastActivityAt() => $_clearField(8);
  @$pb.TagNumber(8)
  $0.Timestamp ensureLastActivityAt() => $_ensure(7);

  /// provider is who serves the model: anthropic, openai, xai, or the provider
  /// id pi reports (deepseek, zai, openrouter, a custom name, ...). Empty until
  /// the first model call.
  @$pb.TagNumber(9)
  $core.String get provider => $_getSZ(8);
  @$pb.TagNumber(9)
  set provider($core.String value) => $_setString(8, value);
  @$pb.TagNumber(9)
  $core.bool hasProvider() => $_has(8);
  @$pb.TagNumber(9)
  void clearProvider() => $_clearField(9);

  /// local_model is true when the model is served from this machine or the
  /// private network (a user-run LLM server) rather than a hosted API. The hub
  /// decides it from the provider's base URL.
  @$pb.TagNumber(10)
  $core.bool get localModel => $_getBF(9);
  @$pb.TagNumber(10)
  set localModel($core.bool value) => $_setBool(9, value);
  @$pb.TagNumber(10)
  $core.bool hasLocalModel() => $_has(9);
  @$pb.TagNumber(10)
  void clearLocalModel() => $_clearField(10);

  @$pb.TagNumber(11)
  $core.String get reasoningEffort => $_getSZ(10);
  @$pb.TagNumber(11)
  set reasoningEffort($core.String value) => $_setString(10, value);
  @$pb.TagNumber(11)
  $core.bool hasReasoningEffort() => $_has(10);
  @$pb.TagNumber(11)
  void clearReasoningEffort() => $_clearField(11);

  @$pb.TagNumber(12)
  ModelSettings get settings => $_getN(11);
  @$pb.TagNumber(12)
  set settings(ModelSettings value) => $_setField(12, value);
  @$pb.TagNumber(12)
  $core.bool hasSettings() => $_has(11);
  @$pb.TagNumber(12)
  void clearSettings() => $_clearField(12);
  @$pb.TagNumber(12)
  ModelSettings ensureSettings() => $_ensure(11);

  /// cache is the prompt-cache telemetry over the chat log.
  @$pb.TagNumber(13)
  CacheTelemetry get cache => $_getN(12);
  @$pb.TagNumber(13)
  set cache(CacheTelemetry value) => $_setField(13, value);
  @$pb.TagNumber(13)
  $core.bool hasCache() => $_has(12);
  @$pb.TagNumber(13)
  void clearCache() => $_clearField(13);
  @$pb.TagNumber(13)
  CacheTelemetry ensureCache() => $_ensure(12);
}

/// TokenCounts sums the model calls of a span. input_tokens excludes the
/// cached parts: a prompt is input + cache_read + cache_write tokens, and
/// read_ratio is cache_read over that prompt (0 with no prompt).
class TokenCounts extends $pb.GeneratedMessage {
  factory TokenCounts({
    $fixnum.Int64? calls,
    $fixnum.Int64? inputTokens,
    $fixnum.Int64? outputTokens,
    $fixnum.Int64? cacheReadTokens,
    $fixnum.Int64? cacheWriteTokens,
    $core.double? readRatio,
  }) {
    final result = TokenCounts._();
    if (calls != null) result.calls = calls;
    if (inputTokens != null) result.inputTokens = inputTokens;
    if (outputTokens != null) result.outputTokens = outputTokens;
    if (cacheReadTokens != null) result.cacheReadTokens = cacheReadTokens;
    if (cacheWriteTokens != null) result.cacheWriteTokens = cacheWriteTokens;
    if (readRatio != null) result.readRatio = readRatio;
    return result;
  }

  TokenCounts._();

  factory TokenCounts.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      TokenCounts()..mergeFromBuffer(data, registry);
  factory TokenCounts.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      TokenCounts()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'TokenCounts',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: TokenCounts.$_createMessage)
    ..a<$fixnum.Int64>(1, _omitFieldNames ? '' : 'calls', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..a<$fixnum.Int64>(
        2, _omitFieldNames ? '' : 'inputTokens', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..a<$fixnum.Int64>(
        3, _omitFieldNames ? '' : 'outputTokens', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..a<$fixnum.Int64>(
        4, _omitFieldNames ? '' : 'cacheReadTokens', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..a<$fixnum.Int64>(
        5, _omitFieldNames ? '' : 'cacheWriteTokens', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..aD(6, _omitFieldNames ? '' : 'readRatio')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  TokenCounts clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  TokenCounts copyWith(void Function(TokenCounts) updates) =>
      super.copyWith((message) => updates(message as TokenCounts))
          as TokenCounts;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use TokenCounts() / TokenCounts.new instead')
  static TokenCounts create() => TokenCounts._();
  static $pb.GeneratedMessage $_createMessage() => TokenCounts._();
  @$core.override
  TokenCounts createEmptyInstance() => TokenCounts._();
  @$core.pragma('dart2js:noInline')
  static TokenCounts getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<TokenCounts>(
          TokenCounts.$_createMessage);
  static TokenCounts? _defaultInstance;

  @$pb.TagNumber(1)
  $fixnum.Int64 get calls => $_getI64(0);
  @$pb.TagNumber(1)
  set calls($fixnum.Int64 value) => $_setInt64(0, value);
  @$pb.TagNumber(1)
  $core.bool hasCalls() => $_has(0);
  @$pb.TagNumber(1)
  void clearCalls() => $_clearField(1);

  @$pb.TagNumber(2)
  $fixnum.Int64 get inputTokens => $_getI64(1);
  @$pb.TagNumber(2)
  set inputTokens($fixnum.Int64 value) => $_setInt64(1, value);
  @$pb.TagNumber(2)
  $core.bool hasInputTokens() => $_has(1);
  @$pb.TagNumber(2)
  void clearInputTokens() => $_clearField(2);

  @$pb.TagNumber(3)
  $fixnum.Int64 get outputTokens => $_getI64(2);
  @$pb.TagNumber(3)
  set outputTokens($fixnum.Int64 value) => $_setInt64(2, value);
  @$pb.TagNumber(3)
  $core.bool hasOutputTokens() => $_has(2);
  @$pb.TagNumber(3)
  void clearOutputTokens() => $_clearField(3);

  @$pb.TagNumber(4)
  $fixnum.Int64 get cacheReadTokens => $_getI64(3);
  @$pb.TagNumber(4)
  set cacheReadTokens($fixnum.Int64 value) => $_setInt64(3, value);
  @$pb.TagNumber(4)
  $core.bool hasCacheReadTokens() => $_has(3);
  @$pb.TagNumber(4)
  void clearCacheReadTokens() => $_clearField(4);

  @$pb.TagNumber(5)
  $fixnum.Int64 get cacheWriteTokens => $_getI64(4);
  @$pb.TagNumber(5)
  set cacheWriteTokens($fixnum.Int64 value) => $_setInt64(4, value);
  @$pb.TagNumber(5)
  $core.bool hasCacheWriteTokens() => $_has(4);
  @$pb.TagNumber(5)
  void clearCacheWriteTokens() => $_clearField(5);

  @$pb.TagNumber(6)
  $core.double get readRatio => $_getN(5);
  @$pb.TagNumber(6)
  set readRatio($core.double value) => $_setDouble(5, value);
  @$pb.TagNumber(6)
  $core.bool hasReadRatio() => $_has(5);
  @$pb.TagNumber(6)
  void clearReadRatio() => $_clearField(6);
}

/// CacheTelemetry tells whether the harness's native prompt cache is being
/// used. first_call is the first model call since the harness (re)started and
/// is absent before it. resumed is true when the run started on a log that
/// already held model calls; cold_resume then flags a first call that read
/// nothing from the cache although it was expected to.
class CacheTelemetry extends $pb.GeneratedMessage {
  factory CacheTelemetry({
    TokenCounts? total,
    TokenCounts? firstCall,
    $core.bool? resumed,
    $core.bool? coldResume,
  }) {
    final result = CacheTelemetry._();
    if (total != null) result.total = total;
    if (firstCall != null) result.firstCall = firstCall;
    if (resumed != null) result.resumed = resumed;
    if (coldResume != null) result.coldResume = coldResume;
    return result;
  }

  CacheTelemetry._();

  factory CacheTelemetry.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      CacheTelemetry()..mergeFromBuffer(data, registry);
  factory CacheTelemetry.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      CacheTelemetry()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'CacheTelemetry',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: CacheTelemetry.$_createMessage)
    ..aOM<TokenCounts>(1, _omitFieldNames ? '' : 'total',
        subBuilder: TokenCounts.$_createMessage)
    ..aOM<TokenCounts>(2, _omitFieldNames ? '' : 'firstCall',
        subBuilder: TokenCounts.$_createMessage)
    ..aOB(3, _omitFieldNames ? '' : 'resumed')
    ..aOB(4, _omitFieldNames ? '' : 'coldResume')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  CacheTelemetry clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  CacheTelemetry copyWith(void Function(CacheTelemetry) updates) =>
      super.copyWith((message) => updates(message as CacheTelemetry))
          as CacheTelemetry;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use CacheTelemetry() / CacheTelemetry.new instead')
  static CacheTelemetry create() => CacheTelemetry._();
  static $pb.GeneratedMessage $_createMessage() => CacheTelemetry._();
  @$core.override
  CacheTelemetry createEmptyInstance() => CacheTelemetry._();
  @$core.pragma('dart2js:noInline')
  static CacheTelemetry getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<CacheTelemetry>(
          CacheTelemetry.$_createMessage);
  static CacheTelemetry? _defaultInstance;

  @$pb.TagNumber(1)
  TokenCounts get total => $_getN(0);
  @$pb.TagNumber(1)
  set total(TokenCounts value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasTotal() => $_has(0);
  @$pb.TagNumber(1)
  void clearTotal() => $_clearField(1);
  @$pb.TagNumber(1)
  TokenCounts ensureTotal() => $_ensure(0);

  @$pb.TagNumber(2)
  TokenCounts get firstCall => $_getN(1);
  @$pb.TagNumber(2)
  set firstCall(TokenCounts value) => $_setField(2, value);
  @$pb.TagNumber(2)
  $core.bool hasFirstCall() => $_has(1);
  @$pb.TagNumber(2)
  void clearFirstCall() => $_clearField(2);
  @$pb.TagNumber(2)
  TokenCounts ensureFirstCall() => $_ensure(1);

  @$pb.TagNumber(3)
  $core.bool get resumed => $_getBF(2);
  @$pb.TagNumber(3)
  set resumed($core.bool value) => $_setBool(2, value);
  @$pb.TagNumber(3)
  $core.bool hasResumed() => $_has(2);
  @$pb.TagNumber(3)
  void clearResumed() => $_clearField(3);

  @$pb.TagNumber(4)
  $core.bool get coldResume => $_getBF(3);
  @$pb.TagNumber(4)
  set coldResume($core.bool value) => $_setBool(3, value);
  @$pb.TagNumber(4)
  $core.bool hasColdResume() => $_has(3);
  @$pb.TagNumber(4)
  void clearColdResume() => $_clearField(4);
}

enum ChatEvent_Body {
  user,
  text,
  toolCall,
  toolResult,
  approval,
  approvalResolved,
  usage,
  settled,
  compaction,
  error,
  state,
  handoff,
  settings,
  report,
  notSet
}

class ChatEvent extends $pb.GeneratedMessage {
  factory ChatEvent({
    $fixnum.Int64? seq,
    $0.Timestamp? time,
    UserMessage? user,
    AssistantText? text,
    ToolCall? toolCall,
    ToolResult? toolResult,
    ApprovalRequest? approval,
    ApprovalResolved? approvalResolved,
    UsageUpdate? usage,
    TurnSettled? settled,
    Compaction? compaction,
    ChatError? error,
    StateChange? state,
    Handoff? handoff,
    ModelSettings? settings,
    SleeveResult? report,
  }) {
    final result = ChatEvent._();
    if (seq != null) result.seq = seq;
    if (time != null) result.time = time;
    if (user != null) result.user = user;
    if (text != null) result.text = text;
    if (toolCall != null) result.toolCall = toolCall;
    if (toolResult != null) result.toolResult = toolResult;
    if (approval != null) result.approval = approval;
    if (approvalResolved != null) result.approvalResolved = approvalResolved;
    if (usage != null) result.usage = usage;
    if (settled != null) result.settled = settled;
    if (compaction != null) result.compaction = compaction;
    if (error != null) result.error = error;
    if (state != null) result.state = state;
    if (handoff != null) result.handoff = handoff;
    if (settings != null) result.settings = settings;
    if (report != null) result.report = report;
    return result;
  }

  ChatEvent._();

  factory ChatEvent.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ChatEvent()..mergeFromBuffer(data, registry);
  factory ChatEvent.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ChatEvent()..mergeFromJson(json, registry);

  static const $core.Map<$core.int, ChatEvent_Body> _ChatEvent_BodyByTag = {
    10: ChatEvent_Body.user,
    11: ChatEvent_Body.text,
    12: ChatEvent_Body.toolCall,
    13: ChatEvent_Body.toolResult,
    14: ChatEvent_Body.approval,
    15: ChatEvent_Body.approvalResolved,
    16: ChatEvent_Body.usage,
    17: ChatEvent_Body.settled,
    18: ChatEvent_Body.compaction,
    19: ChatEvent_Body.error,
    20: ChatEvent_Body.state,
    21: ChatEvent_Body.handoff,
    22: ChatEvent_Body.settings,
    23: ChatEvent_Body.report,
    0: ChatEvent_Body.notSet
  };
  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ChatEvent',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ChatEvent.$_createMessage)
    ..oo(0, [10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23])
    ..a<$fixnum.Int64>(1, _omitFieldNames ? '' : 'seq', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..aOM<$0.Timestamp>(2, _omitFieldNames ? '' : 'time',
        subBuilder: $0.Timestamp.$_createMessage)
    ..aOM<UserMessage>(10, _omitFieldNames ? '' : 'user',
        subBuilder: UserMessage.$_createMessage)
    ..aOM<AssistantText>(11, _omitFieldNames ? '' : 'text',
        subBuilder: AssistantText.$_createMessage)
    ..aOM<ToolCall>(12, _omitFieldNames ? '' : 'toolCall',
        subBuilder: ToolCall.$_createMessage)
    ..aOM<ToolResult>(13, _omitFieldNames ? '' : 'toolResult',
        subBuilder: ToolResult.$_createMessage)
    ..aOM<ApprovalRequest>(14, _omitFieldNames ? '' : 'approval',
        subBuilder: ApprovalRequest.$_createMessage)
    ..aOM<ApprovalResolved>(15, _omitFieldNames ? '' : 'approvalResolved',
        subBuilder: ApprovalResolved.$_createMessage)
    ..aOM<UsageUpdate>(16, _omitFieldNames ? '' : 'usage',
        subBuilder: UsageUpdate.$_createMessage)
    ..aOM<TurnSettled>(17, _omitFieldNames ? '' : 'settled',
        subBuilder: TurnSettled.$_createMessage)
    ..aOM<Compaction>(18, _omitFieldNames ? '' : 'compaction',
        subBuilder: Compaction.$_createMessage)
    ..aOM<ChatError>(19, _omitFieldNames ? '' : 'error',
        subBuilder: ChatError.$_createMessage)
    ..aOM<StateChange>(20, _omitFieldNames ? '' : 'state',
        subBuilder: StateChange.$_createMessage)
    ..aOM<Handoff>(21, _omitFieldNames ? '' : 'handoff',
        subBuilder: Handoff.$_createMessage)
    ..aOM<ModelSettings>(22, _omitFieldNames ? '' : 'settings',
        subBuilder: ModelSettings.$_createMessage)
    ..aOM<SleeveResult>(23, _omitFieldNames ? '' : 'report',
        subBuilder: SleeveResult.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ChatEvent clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ChatEvent copyWith(void Function(ChatEvent) updates) =>
      super.copyWith((message) => updates(message as ChatEvent)) as ChatEvent;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use ChatEvent() / ChatEvent.new instead')
  static ChatEvent create() => ChatEvent._();
  static $pb.GeneratedMessage $_createMessage() => ChatEvent._();
  @$core.override
  ChatEvent createEmptyInstance() => ChatEvent._();
  @$core.pragma('dart2js:noInline')
  static ChatEvent getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<ChatEvent>(ChatEvent.$_createMessage);
  static ChatEvent? _defaultInstance;

  @$pb.TagNumber(10)
  @$pb.TagNumber(11)
  @$pb.TagNumber(12)
  @$pb.TagNumber(13)
  @$pb.TagNumber(14)
  @$pb.TagNumber(15)
  @$pb.TagNumber(16)
  @$pb.TagNumber(17)
  @$pb.TagNumber(18)
  @$pb.TagNumber(19)
  @$pb.TagNumber(20)
  @$pb.TagNumber(21)
  @$pb.TagNumber(22)
  @$pb.TagNumber(23)
  ChatEvent_Body whichBody() => _ChatEvent_BodyByTag[$_whichOneof(0)]!;
  @$pb.TagNumber(10)
  @$pb.TagNumber(11)
  @$pb.TagNumber(12)
  @$pb.TagNumber(13)
  @$pb.TagNumber(14)
  @$pb.TagNumber(15)
  @$pb.TagNumber(16)
  @$pb.TagNumber(17)
  @$pb.TagNumber(18)
  @$pb.TagNumber(19)
  @$pb.TagNumber(20)
  @$pb.TagNumber(21)
  @$pb.TagNumber(22)
  @$pb.TagNumber(23)
  void clearBody() => $_clearField($_whichOneof(0));

  /// seq increases by one per event within a sleeve, starting at 1. All text
  /// deltas of one assistant message share that message's seq.
  @$pb.TagNumber(1)
  $fixnum.Int64 get seq => $_getI64(0);
  @$pb.TagNumber(1)
  set seq($fixnum.Int64 value) => $_setInt64(0, value);
  @$pb.TagNumber(1)
  $core.bool hasSeq() => $_has(0);
  @$pb.TagNumber(1)
  void clearSeq() => $_clearField(1);

  @$pb.TagNumber(2)
  $0.Timestamp get time => $_getN(1);
  @$pb.TagNumber(2)
  set time($0.Timestamp value) => $_setField(2, value);
  @$pb.TagNumber(2)
  $core.bool hasTime() => $_has(1);
  @$pb.TagNumber(2)
  void clearTime() => $_clearField(2);
  @$pb.TagNumber(2)
  $0.Timestamp ensureTime() => $_ensure(1);

  @$pb.TagNumber(10)
  UserMessage get user => $_getN(2);
  @$pb.TagNumber(10)
  set user(UserMessage value) => $_setField(10, value);
  @$pb.TagNumber(10)
  $core.bool hasUser() => $_has(2);
  @$pb.TagNumber(10)
  void clearUser() => $_clearField(10);
  @$pb.TagNumber(10)
  UserMessage ensureUser() => $_ensure(2);

  @$pb.TagNumber(11)
  AssistantText get text => $_getN(3);
  @$pb.TagNumber(11)
  set text(AssistantText value) => $_setField(11, value);
  @$pb.TagNumber(11)
  $core.bool hasText() => $_has(3);
  @$pb.TagNumber(11)
  void clearText() => $_clearField(11);
  @$pb.TagNumber(11)
  AssistantText ensureText() => $_ensure(3);

  @$pb.TagNumber(12)
  ToolCall get toolCall => $_getN(4);
  @$pb.TagNumber(12)
  set toolCall(ToolCall value) => $_setField(12, value);
  @$pb.TagNumber(12)
  $core.bool hasToolCall() => $_has(4);
  @$pb.TagNumber(12)
  void clearToolCall() => $_clearField(12);
  @$pb.TagNumber(12)
  ToolCall ensureToolCall() => $_ensure(4);

  @$pb.TagNumber(13)
  ToolResult get toolResult => $_getN(5);
  @$pb.TagNumber(13)
  set toolResult(ToolResult value) => $_setField(13, value);
  @$pb.TagNumber(13)
  $core.bool hasToolResult() => $_has(5);
  @$pb.TagNumber(13)
  void clearToolResult() => $_clearField(13);
  @$pb.TagNumber(13)
  ToolResult ensureToolResult() => $_ensure(5);

  @$pb.TagNumber(14)
  ApprovalRequest get approval => $_getN(6);
  @$pb.TagNumber(14)
  set approval(ApprovalRequest value) => $_setField(14, value);
  @$pb.TagNumber(14)
  $core.bool hasApproval() => $_has(6);
  @$pb.TagNumber(14)
  void clearApproval() => $_clearField(14);
  @$pb.TagNumber(14)
  ApprovalRequest ensureApproval() => $_ensure(6);

  @$pb.TagNumber(15)
  ApprovalResolved get approvalResolved => $_getN(7);
  @$pb.TagNumber(15)
  set approvalResolved(ApprovalResolved value) => $_setField(15, value);
  @$pb.TagNumber(15)
  $core.bool hasApprovalResolved() => $_has(7);
  @$pb.TagNumber(15)
  void clearApprovalResolved() => $_clearField(15);
  @$pb.TagNumber(15)
  ApprovalResolved ensureApprovalResolved() => $_ensure(7);

  @$pb.TagNumber(16)
  UsageUpdate get usage => $_getN(8);
  @$pb.TagNumber(16)
  set usage(UsageUpdate value) => $_setField(16, value);
  @$pb.TagNumber(16)
  $core.bool hasUsage() => $_has(8);
  @$pb.TagNumber(16)
  void clearUsage() => $_clearField(16);
  @$pb.TagNumber(16)
  UsageUpdate ensureUsage() => $_ensure(8);

  @$pb.TagNumber(17)
  TurnSettled get settled => $_getN(9);
  @$pb.TagNumber(17)
  set settled(TurnSettled value) => $_setField(17, value);
  @$pb.TagNumber(17)
  $core.bool hasSettled() => $_has(9);
  @$pb.TagNumber(17)
  void clearSettled() => $_clearField(17);
  @$pb.TagNumber(17)
  TurnSettled ensureSettled() => $_ensure(9);

  @$pb.TagNumber(18)
  Compaction get compaction => $_getN(10);
  @$pb.TagNumber(18)
  set compaction(Compaction value) => $_setField(18, value);
  @$pb.TagNumber(18)
  $core.bool hasCompaction() => $_has(10);
  @$pb.TagNumber(18)
  void clearCompaction() => $_clearField(18);
  @$pb.TagNumber(18)
  Compaction ensureCompaction() => $_ensure(10);

  @$pb.TagNumber(19)
  ChatError get error => $_getN(11);
  @$pb.TagNumber(19)
  set error(ChatError value) => $_setField(19, value);
  @$pb.TagNumber(19)
  $core.bool hasError() => $_has(11);
  @$pb.TagNumber(19)
  void clearError() => $_clearField(19);
  @$pb.TagNumber(19)
  ChatError ensureError() => $_ensure(11);

  @$pb.TagNumber(20)
  StateChange get state => $_getN(12);
  @$pb.TagNumber(20)
  set state(StateChange value) => $_setField(20, value);
  @$pb.TagNumber(20)
  $core.bool hasState() => $_has(12);
  @$pb.TagNumber(20)
  void clearState() => $_clearField(20);
  @$pb.TagNumber(20)
  StateChange ensureState() => $_ensure(12);

  /// handoff marks where a resleeve replaced the harness; the events after
  /// it come from the new one.
  @$pb.TagNumber(21)
  Handoff get handoff => $_getN(13);
  @$pb.TagNumber(21)
  set handoff(Handoff value) => $_setField(21, value);
  @$pb.TagNumber(21)
  $core.bool hasHandoff() => $_has(13);
  @$pb.TagNumber(21)
  void clearHandoff() => $_clearField(21);
  @$pb.TagNumber(21)
  Handoff ensureHandoff() => $_ensure(13);

  @$pb.TagNumber(22)
  ModelSettings get settings => $_getN(14);
  @$pb.TagNumber(22)
  set settings(ModelSettings value) => $_setField(22, value);
  @$pb.TagNumber(22)
  $core.bool hasSettings() => $_has(14);
  @$pb.TagNumber(22)
  void clearSettings() => $_clearField(22);
  @$pb.TagNumber(22)
  ModelSettings ensureSettings() => $_ensure(14);

  @$pb.TagNumber(23)
  SleeveResult get report => $_getN(15);
  @$pb.TagNumber(23)
  set report(SleeveResult value) => $_setField(23, value);
  @$pb.TagNumber(23)
  $core.bool hasReport() => $_has(15);
  @$pb.TagNumber(23)
  void clearReport() => $_clearField(23);
  @$pb.TagNumber(23)
  SleeveResult ensureReport() => $_ensure(15);
}

class UserMessage extends $pb.GeneratedMessage {
  factory UserMessage({
    $core.String? text,
    $core.String? from,
  }) {
    final result = UserMessage._();
    if (text != null) result.text = text;
    if (from != null) result.from = from;
    return result;
  }

  UserMessage._();

  factory UserMessage.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      UserMessage()..mergeFromBuffer(data, registry);
  factory UserMessage.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      UserMessage()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'UserMessage',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: UserMessage.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'text')
    ..aOS(2, _omitFieldNames ? '' : 'from')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  UserMessage clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  UserMessage copyWith(void Function(UserMessage) updates) =>
      super.copyWith((message) => updates(message as UserMessage))
          as UserMessage;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use UserMessage() / UserMessage.new instead')
  static UserMessage create() => UserMessage._();
  static $pb.GeneratedMessage $_createMessage() => UserMessage._();
  @$core.override
  UserMessage createEmptyInstance() => UserMessage._();
  @$core.pragma('dart2js:noInline')
  static UserMessage getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<UserMessage>(
          UserMessage.$_createMessage);
  static UserMessage? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get text => $_getSZ(0);
  @$pb.TagNumber(1)
  set text($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasText() => $_has(0);
  @$pb.TagNumber(1)
  void clearText() => $_clearField(1);

  /// from is who sent it: "owner", "token:<name>" or "hub" for messages the
  /// hub composes (a resleeve handoff prompt).
  @$pb.TagNumber(2)
  $core.String get from => $_getSZ(1);
  @$pb.TagNumber(2)
  set from($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasFrom() => $_has(1);
  @$pb.TagNumber(2)
  void clearFrom() => $_clearField(2);
}

/// AssistantText is streamed as deltas that share one seq and message_id. The
/// stored log keeps the whole message under that seq, and a watch that starts
/// at it first sends the text so far, then the remaining deltas. To resume
/// after a text event, watch from its seq minus one and replace that message.
class AssistantText extends $pb.GeneratedMessage {
  factory AssistantText({
    $core.String? messageId,
    $core.String? delta,
    $core.bool? thinking,
  }) {
    final result = AssistantText._();
    if (messageId != null) result.messageId = messageId;
    if (delta != null) result.delta = delta;
    if (thinking != null) result.thinking = thinking;
    return result;
  }

  AssistantText._();

  factory AssistantText.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      AssistantText()..mergeFromBuffer(data, registry);
  factory AssistantText.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      AssistantText()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'AssistantText',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: AssistantText.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'messageId')
    ..aOS(2, _omitFieldNames ? '' : 'delta')
    ..aOB(3, _omitFieldNames ? '' : 'thinking')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  AssistantText clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  AssistantText copyWith(void Function(AssistantText) updates) =>
      super.copyWith((message) => updates(message as AssistantText))
          as AssistantText;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use AssistantText() / AssistantText.new instead')
  static AssistantText create() => AssistantText._();
  static $pb.GeneratedMessage $_createMessage() => AssistantText._();
  @$core.override
  AssistantText createEmptyInstance() => AssistantText._();
  @$core.pragma('dart2js:noInline')
  static AssistantText getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<AssistantText>(
          AssistantText.$_createMessage);
  static AssistantText? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get messageId => $_getSZ(0);
  @$pb.TagNumber(1)
  set messageId($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasMessageId() => $_has(0);
  @$pb.TagNumber(1)
  void clearMessageId() => $_clearField(1);

  @$pb.TagNumber(2)
  $core.String get delta => $_getSZ(1);
  @$pb.TagNumber(2)
  set delta($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasDelta() => $_has(1);
  @$pb.TagNumber(2)
  void clearDelta() => $_clearField(2);

  /// thinking marks reasoning text, shown collapsed.
  @$pb.TagNumber(3)
  $core.bool get thinking => $_getBF(2);
  @$pb.TagNumber(3)
  set thinking($core.bool value) => $_setBool(2, value);
  @$pb.TagNumber(3)
  $core.bool hasThinking() => $_has(2);
  @$pb.TagNumber(3)
  void clearThinking() => $_clearField(3);
}

class ToolCall extends $pb.GeneratedMessage {
  factory ToolCall({
    $core.String? id,
    $core.String? name,
    $core.String? inputJson,
  }) {
    final result = ToolCall._();
    if (id != null) result.id = id;
    if (name != null) result.name = name;
    if (inputJson != null) result.inputJson = inputJson;
    return result;
  }

  ToolCall._();

  factory ToolCall.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ToolCall()..mergeFromBuffer(data, registry);
  factory ToolCall.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ToolCall()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ToolCall',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ToolCall.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'id')
    ..aOS(2, _omitFieldNames ? '' : 'name')
    ..aOS(3, _omitFieldNames ? '' : 'inputJson')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ToolCall clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ToolCall copyWith(void Function(ToolCall) updates) =>
      super.copyWith((message) => updates(message as ToolCall)) as ToolCall;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use ToolCall() / ToolCall.new instead')
  static ToolCall create() => ToolCall._();
  static $pb.GeneratedMessage $_createMessage() => ToolCall._();
  @$core.override
  ToolCall createEmptyInstance() => ToolCall._();
  @$core.pragma('dart2js:noInline')
  static ToolCall getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<ToolCall>(ToolCall.$_createMessage);
  static ToolCall? _defaultInstance;

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

  /// input_json is the tool input as the harness reported it.
  @$pb.TagNumber(3)
  $core.String get inputJson => $_getSZ(2);
  @$pb.TagNumber(3)
  set inputJson($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasInputJson() => $_has(2);
  @$pb.TagNumber(3)
  void clearInputJson() => $_clearField(3);
}

class ToolResult extends $pb.GeneratedMessage {
  factory ToolResult({
    $core.String? toolCallId,
    $core.String? output,
    $core.bool? isError,
    $core.bool? truncated,
  }) {
    final result = ToolResult._();
    if (toolCallId != null) result.toolCallId = toolCallId;
    if (output != null) result.output = output;
    if (isError != null) result.isError = isError;
    if (truncated != null) result.truncated = truncated;
    return result;
  }

  ToolResult._();

  factory ToolResult.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ToolResult()..mergeFromBuffer(data, registry);
  factory ToolResult.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ToolResult()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ToolResult',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ToolResult.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'toolCallId')
    ..aOS(2, _omitFieldNames ? '' : 'output')
    ..aOB(3, _omitFieldNames ? '' : 'isError')
    ..aOB(4, _omitFieldNames ? '' : 'truncated')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ToolResult clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ToolResult copyWith(void Function(ToolResult) updates) =>
      super.copyWith((message) => updates(message as ToolResult)) as ToolResult;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use ToolResult() / ToolResult.new instead')
  static ToolResult create() => ToolResult._();
  static $pb.GeneratedMessage $_createMessage() => ToolResult._();
  @$core.override
  ToolResult createEmptyInstance() => ToolResult._();
  @$core.pragma('dart2js:noInline')
  static ToolResult getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<ToolResult>(ToolResult.$_createMessage);
  static ToolResult? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get toolCallId => $_getSZ(0);
  @$pb.TagNumber(1)
  set toolCallId($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasToolCallId() => $_has(0);
  @$pb.TagNumber(1)
  void clearToolCallId() => $_clearField(1);

  /// output is truncated by the hub to a bounded size; truncated says so.
  @$pb.TagNumber(2)
  $core.String get output => $_getSZ(1);
  @$pb.TagNumber(2)
  set output($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasOutput() => $_has(1);
  @$pb.TagNumber(2)
  void clearOutput() => $_clearField(2);

  @$pb.TagNumber(3)
  $core.bool get isError => $_getBF(2);
  @$pb.TagNumber(3)
  set isError($core.bool value) => $_setBool(2, value);
  @$pb.TagNumber(3)
  $core.bool hasIsError() => $_has(2);
  @$pb.TagNumber(3)
  void clearIsError() => $_clearField(3);

  @$pb.TagNumber(4)
  $core.bool get truncated => $_getBF(3);
  @$pb.TagNumber(4)
  set truncated($core.bool value) => $_setBool(3, value);
  @$pb.TagNumber(4)
  $core.bool hasTruncated() => $_has(3);
  @$pb.TagNumber(4)
  void clearTruncated() => $_clearField(4);
}

class ApprovalRequest extends $pb.GeneratedMessage {
  factory ApprovalRequest({
    $core.String? id,
    $core.String? kind,
    $core.String? summary,
  }) {
    final result = ApprovalRequest._();
    if (id != null) result.id = id;
    if (kind != null) result.kind = kind;
    if (summary != null) result.summary = summary;
    return result;
  }

  ApprovalRequest._();

  factory ApprovalRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ApprovalRequest()..mergeFromBuffer(data, registry);
  factory ApprovalRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ApprovalRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ApprovalRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ApprovalRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'id')
    ..aOS(2, _omitFieldNames ? '' : 'kind')
    ..aOS(3, _omitFieldNames ? '' : 'summary')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ApprovalRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ApprovalRequest copyWith(void Function(ApprovalRequest) updates) =>
      super.copyWith((message) => updates(message as ApprovalRequest))
          as ApprovalRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use ApprovalRequest() / ApprovalRequest.new instead')
  static ApprovalRequest create() => ApprovalRequest._();
  static $pb.GeneratedMessage $_createMessage() => ApprovalRequest._();
  @$core.override
  ApprovalRequest createEmptyInstance() => ApprovalRequest._();
  @$core.pragma('dart2js:noInline')
  static ApprovalRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<ApprovalRequest>(
          ApprovalRequest.$_createMessage);
  static ApprovalRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get id => $_getSZ(0);
  @$pb.TagNumber(1)
  set id($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasId() => $_has(0);
  @$pb.TagNumber(1)
  void clearId() => $_clearField(1);

  @$pb.TagNumber(2)
  $core.String get kind => $_getSZ(1);
  @$pb.TagNumber(2)
  set kind($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasKind() => $_has(1);
  @$pb.TagNumber(2)
  void clearKind() => $_clearField(2);

  @$pb.TagNumber(3)
  $core.String get summary => $_getSZ(2);
  @$pb.TagNumber(3)
  set summary($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasSummary() => $_has(2);
  @$pb.TagNumber(3)
  void clearSummary() => $_clearField(3);
}

class ApprovalResolved extends $pb.GeneratedMessage {
  factory ApprovalResolved({
    $core.String? id,
    $core.bool? allow,
    $core.String? by,
  }) {
    final result = ApprovalResolved._();
    if (id != null) result.id = id;
    if (allow != null) result.allow = allow;
    if (by != null) result.by = by;
    return result;
  }

  ApprovalResolved._();

  factory ApprovalResolved.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ApprovalResolved()..mergeFromBuffer(data, registry);
  factory ApprovalResolved.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ApprovalResolved()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ApprovalResolved',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ApprovalResolved.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'id')
    ..aOB(2, _omitFieldNames ? '' : 'allow')
    ..aOS(3, _omitFieldNames ? '' : 'by')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ApprovalResolved clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ApprovalResolved copyWith(void Function(ApprovalResolved) updates) =>
      super.copyWith((message) => updates(message as ApprovalResolved))
          as ApprovalResolved;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use ApprovalResolved() / ApprovalResolved.new instead')
  static ApprovalResolved create() => ApprovalResolved._();
  static $pb.GeneratedMessage $_createMessage() => ApprovalResolved._();
  @$core.override
  ApprovalResolved createEmptyInstance() => ApprovalResolved._();
  @$core.pragma('dart2js:noInline')
  static ApprovalResolved getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<ApprovalResolved>(
          ApprovalResolved.$_createMessage);
  static ApprovalResolved? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get id => $_getSZ(0);
  @$pb.TagNumber(1)
  set id($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasId() => $_has(0);
  @$pb.TagNumber(1)
  void clearId() => $_clearField(1);

  @$pb.TagNumber(2)
  $core.bool get allow => $_getBF(1);
  @$pb.TagNumber(2)
  set allow($core.bool value) => $_setBool(1, value);
  @$pb.TagNumber(2)
  $core.bool hasAllow() => $_has(1);
  @$pb.TagNumber(2)
  void clearAllow() => $_clearField(2);

  @$pb.TagNumber(3)
  $core.String get by => $_getSZ(2);
  @$pb.TagNumber(3)
  set by($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasBy() => $_has(2);
  @$pb.TagNumber(3)
  void clearBy() => $_clearField(3);
}

class UsageUpdate extends $pb.GeneratedMessage {
  factory UsageUpdate({
    $fixnum.Int64? contextTokens,
    $fixnum.Int64? contextWindow,
    $fixnum.Int64? inputTokens,
    $fixnum.Int64? outputTokens,
    $fixnum.Int64? cacheReadTokens,
    $fixnum.Int64? cacheWriteTokens,
    $0.Timestamp? cacheExpiresAt,
  }) {
    final result = UsageUpdate._();
    if (contextTokens != null) result.contextTokens = contextTokens;
    if (contextWindow != null) result.contextWindow = contextWindow;
    if (inputTokens != null) result.inputTokens = inputTokens;
    if (outputTokens != null) result.outputTokens = outputTokens;
    if (cacheReadTokens != null) result.cacheReadTokens = cacheReadTokens;
    if (cacheWriteTokens != null) result.cacheWriteTokens = cacheWriteTokens;
    if (cacheExpiresAt != null) result.cacheExpiresAt = cacheExpiresAt;
    return result;
  }

  UsageUpdate._();

  factory UsageUpdate.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      UsageUpdate()..mergeFromBuffer(data, registry);
  factory UsageUpdate.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      UsageUpdate()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'UsageUpdate',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: UsageUpdate.$_createMessage)
    ..a<$fixnum.Int64>(
        1, _omitFieldNames ? '' : 'contextTokens', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..a<$fixnum.Int64>(
        2, _omitFieldNames ? '' : 'contextWindow', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..a<$fixnum.Int64>(
        3, _omitFieldNames ? '' : 'inputTokens', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..a<$fixnum.Int64>(
        4, _omitFieldNames ? '' : 'outputTokens', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..a<$fixnum.Int64>(
        5, _omitFieldNames ? '' : 'cacheReadTokens', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..a<$fixnum.Int64>(
        6, _omitFieldNames ? '' : 'cacheWriteTokens', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..aOM<$0.Timestamp>(7, _omitFieldNames ? '' : 'cacheExpiresAt',
        subBuilder: $0.Timestamp.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  UsageUpdate clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  UsageUpdate copyWith(void Function(UsageUpdate) updates) =>
      super.copyWith((message) => updates(message as UsageUpdate))
          as UsageUpdate;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use UsageUpdate() / UsageUpdate.new instead')
  static UsageUpdate create() => UsageUpdate._();
  static $pb.GeneratedMessage $_createMessage() => UsageUpdate._();
  @$core.override
  UsageUpdate createEmptyInstance() => UsageUpdate._();
  @$core.pragma('dart2js:noInline')
  static UsageUpdate getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<UsageUpdate>(
          UsageUpdate.$_createMessage);
  static UsageUpdate? _defaultInstance;

  @$pb.TagNumber(1)
  $fixnum.Int64 get contextTokens => $_getI64(0);
  @$pb.TagNumber(1)
  set contextTokens($fixnum.Int64 value) => $_setInt64(0, value);
  @$pb.TagNumber(1)
  $core.bool hasContextTokens() => $_has(0);
  @$pb.TagNumber(1)
  void clearContextTokens() => $_clearField(1);

  @$pb.TagNumber(2)
  $fixnum.Int64 get contextWindow => $_getI64(1);
  @$pb.TagNumber(2)
  set contextWindow($fixnum.Int64 value) => $_setInt64(1, value);
  @$pb.TagNumber(2)
  $core.bool hasContextWindow() => $_has(1);
  @$pb.TagNumber(2)
  void clearContextWindow() => $_clearField(2);

  @$pb.TagNumber(3)
  $fixnum.Int64 get inputTokens => $_getI64(2);
  @$pb.TagNumber(3)
  set inputTokens($fixnum.Int64 value) => $_setInt64(2, value);
  @$pb.TagNumber(3)
  $core.bool hasInputTokens() => $_has(2);
  @$pb.TagNumber(3)
  void clearInputTokens() => $_clearField(3);

  @$pb.TagNumber(4)
  $fixnum.Int64 get outputTokens => $_getI64(3);
  @$pb.TagNumber(4)
  set outputTokens($fixnum.Int64 value) => $_setInt64(3, value);
  @$pb.TagNumber(4)
  $core.bool hasOutputTokens() => $_has(3);
  @$pb.TagNumber(4)
  void clearOutputTokens() => $_clearField(4);

  @$pb.TagNumber(5)
  $fixnum.Int64 get cacheReadTokens => $_getI64(4);
  @$pb.TagNumber(5)
  set cacheReadTokens($fixnum.Int64 value) => $_setInt64(4, value);
  @$pb.TagNumber(5)
  $core.bool hasCacheReadTokens() => $_has(4);
  @$pb.TagNumber(5)
  void clearCacheReadTokens() => $_clearField(5);

  @$pb.TagNumber(6)
  $fixnum.Int64 get cacheWriteTokens => $_getI64(5);
  @$pb.TagNumber(6)
  set cacheWriteTokens($fixnum.Int64 value) => $_setInt64(5, value);
  @$pb.TagNumber(6)
  $core.bool hasCacheWriteTokens() => $_has(5);
  @$pb.TagNumber(6)
  void clearCacheWriteTokens() => $_clearField(6);

  @$pb.TagNumber(7)
  $0.Timestamp get cacheExpiresAt => $_getN(6);
  @$pb.TagNumber(7)
  set cacheExpiresAt($0.Timestamp value) => $_setField(7, value);
  @$pb.TagNumber(7)
  $core.bool hasCacheExpiresAt() => $_has(6);
  @$pb.TagNumber(7)
  void clearCacheExpiresAt() => $_clearField(7);
  @$pb.TagNumber(7)
  $0.Timestamp ensureCacheExpiresAt() => $_ensure(6);
}

class TurnSettled extends $pb.GeneratedMessage {
  factory TurnSettled({
    $core.String? stopReason,
    TokenCounts? usage,
  }) {
    final result = TurnSettled._();
    if (stopReason != null) result.stopReason = stopReason;
    if (usage != null) result.usage = usage;
    return result;
  }

  TurnSettled._();

  factory TurnSettled.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      TurnSettled()..mergeFromBuffer(data, registry);
  factory TurnSettled.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      TurnSettled()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'TurnSettled',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: TurnSettled.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'stopReason')
    ..aOM<TokenCounts>(2, _omitFieldNames ? '' : 'usage',
        subBuilder: TokenCounts.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  TurnSettled clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  TurnSettled copyWith(void Function(TurnSettled) updates) =>
      super.copyWith((message) => updates(message as TurnSettled))
          as TurnSettled;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use TurnSettled() / TurnSettled.new instead')
  static TurnSettled create() => TurnSettled._();
  static $pb.GeneratedMessage $_createMessage() => TurnSettled._();
  @$core.override
  TurnSettled createEmptyInstance() => TurnSettled._();
  @$core.pragma('dart2js:noInline')
  static TurnSettled getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<TurnSettled>(
          TurnSettled.$_createMessage);
  static TurnSettled? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get stopReason => $_getSZ(0);
  @$pb.TagNumber(1)
  set stopReason($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasStopReason() => $_has(0);
  @$pb.TagNumber(1)
  void clearStopReason() => $_clearField(1);

  /// usage sums the turn's model calls; absent when the harness reported none.
  @$pb.TagNumber(2)
  TokenCounts get usage => $_getN(1);
  @$pb.TagNumber(2)
  set usage(TokenCounts value) => $_setField(2, value);
  @$pb.TagNumber(2)
  $core.bool hasUsage() => $_has(1);
  @$pb.TagNumber(2)
  void clearUsage() => $_clearField(2);
  @$pb.TagNumber(2)
  TokenCounts ensureUsage() => $_ensure(1);
}

class Compaction extends $pb.GeneratedMessage {
  factory Compaction({
    $fixnum.Int64? tokensBefore,
    $fixnum.Int64? tokensAfter,
    $core.bool? native,
  }) {
    final result = Compaction._();
    if (tokensBefore != null) result.tokensBefore = tokensBefore;
    if (tokensAfter != null) result.tokensAfter = tokensAfter;
    if (native != null) result.native = native;
    return result;
  }

  Compaction._();

  factory Compaction.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      Compaction()..mergeFromBuffer(data, registry);
  factory Compaction.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      Compaction()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'Compaction',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: Compaction.$_createMessage)
    ..a<$fixnum.Int64>(
        1, _omitFieldNames ? '' : 'tokensBefore', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..a<$fixnum.Int64>(
        2, _omitFieldNames ? '' : 'tokensAfter', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..aOB(3, _omitFieldNames ? '' : 'native')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  Compaction clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  Compaction copyWith(void Function(Compaction) updates) =>
      super.copyWith((message) => updates(message as Compaction)) as Compaction;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use Compaction() / Compaction.new instead')
  static Compaction create() => Compaction._();
  static $pb.GeneratedMessage $_createMessage() => Compaction._();
  @$core.override
  Compaction createEmptyInstance() => Compaction._();
  @$core.pragma('dart2js:noInline')
  static Compaction getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<Compaction>(Compaction.$_createMessage);
  static Compaction? _defaultInstance;

  @$pb.TagNumber(1)
  $fixnum.Int64 get tokensBefore => $_getI64(0);
  @$pb.TagNumber(1)
  set tokensBefore($fixnum.Int64 value) => $_setInt64(0, value);
  @$pb.TagNumber(1)
  $core.bool hasTokensBefore() => $_has(0);
  @$pb.TagNumber(1)
  void clearTokensBefore() => $_clearField(1);

  @$pb.TagNumber(2)
  $fixnum.Int64 get tokensAfter => $_getI64(1);
  @$pb.TagNumber(2)
  set tokensAfter($fixnum.Int64 value) => $_setInt64(1, value);
  @$pb.TagNumber(2)
  $core.bool hasTokensAfter() => $_has(1);
  @$pb.TagNumber(2)
  void clearTokensAfter() => $_clearField(2);

  /// native is true for compaction the harness started on its own.
  @$pb.TagNumber(3)
  $core.bool get native => $_getBF(2);
  @$pb.TagNumber(3)
  set native($core.bool value) => $_setBool(2, value);
  @$pb.TagNumber(3)
  $core.bool hasNative() => $_has(2);
  @$pb.TagNumber(3)
  void clearNative() => $_clearField(3);
}

class ChatError extends $pb.GeneratedMessage {
  factory ChatError({
    $core.String? text,
  }) {
    final result = ChatError._();
    if (text != null) result.text = text;
    return result;
  }

  ChatError._();

  factory ChatError.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ChatError()..mergeFromBuffer(data, registry);
  factory ChatError.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ChatError()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ChatError',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ChatError.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'text')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ChatError clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ChatError copyWith(void Function(ChatError) updates) =>
      super.copyWith((message) => updates(message as ChatError)) as ChatError;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use ChatError() / ChatError.new instead')
  static ChatError create() => ChatError._();
  static $pb.GeneratedMessage $_createMessage() => ChatError._();
  @$core.override
  ChatError createEmptyInstance() => ChatError._();
  @$core.pragma('dart2js:noInline')
  static ChatError getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<ChatError>(ChatError.$_createMessage);
  static ChatError? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get text => $_getSZ(0);
  @$pb.TagNumber(1)
  set text($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasText() => $_has(0);
  @$pb.TagNumber(1)
  void clearText() => $_clearField(1);
}

class StateChange extends $pb.GeneratedMessage {
  factory StateChange({
    ChatState? state,
  }) {
    final result = StateChange._();
    if (state != null) result.state = state;
    return result;
  }

  StateChange._();

  factory StateChange.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      StateChange()..mergeFromBuffer(data, registry);
  factory StateChange.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      StateChange()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'StateChange',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: StateChange.$_createMessage)
    ..aE<ChatState>(1, _omitFieldNames ? '' : 'state',
        enumValues: ChatState.values)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  StateChange clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  StateChange copyWith(void Function(StateChange) updates) =>
      super.copyWith((message) => updates(message as StateChange))
          as StateChange;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use StateChange() / StateChange.new instead')
  static StateChange create() => StateChange._();
  static $pb.GeneratedMessage $_createMessage() => StateChange._();
  @$core.override
  StateChange createEmptyInstance() => StateChange._();
  @$core.pragma('dart2js:noInline')
  static StateChange getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<StateChange>(
          StateChange.$_createMessage);
  static StateChange? _defaultInstance;

  @$pb.TagNumber(1)
  ChatState get state => $_getN(0);
  @$pb.TagNumber(1)
  set state(ChatState value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasState() => $_has(0);
  @$pb.TagNumber(1)
  void clearState() => $_clearField(1);
}

/// Handoff describes one resleeve: which harness gave way to which, and who
/// wrote the summary in the handoff file. Harness names are claude, codex,
/// grok and pi.
class Handoff extends $pb.GeneratedMessage {
  factory Handoff({
    $core.String? fromHarness,
    $core.String? fromSessionId,
    $core.String? toHarness,
    $core.String? summarizerHarness,
    $core.String? summarizerModel,
    $core.String? summarizerProvider,
    $core.bool? summarizerLocal,
    $core.String? summaryError,
    $core.String? path,
    $core.String? archivePath,
    $0.Timestamp? at,
  }) {
    final result = Handoff._();
    if (fromHarness != null) result.fromHarness = fromHarness;
    if (fromSessionId != null) result.fromSessionId = fromSessionId;
    if (toHarness != null) result.toHarness = toHarness;
    if (summarizerHarness != null) result.summarizerHarness = summarizerHarness;
    if (summarizerModel != null) result.summarizerModel = summarizerModel;
    if (summarizerProvider != null)
      result.summarizerProvider = summarizerProvider;
    if (summarizerLocal != null) result.summarizerLocal = summarizerLocal;
    if (summaryError != null) result.summaryError = summaryError;
    if (path != null) result.path = path;
    if (archivePath != null) result.archivePath = archivePath;
    if (at != null) result.at = at;
    return result;
  }

  Handoff._();

  factory Handoff.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      Handoff()..mergeFromBuffer(data, registry);
  factory Handoff.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      Handoff()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'Handoff',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: Handoff.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'fromHarness')
    ..aOS(2, _omitFieldNames ? '' : 'fromSessionId')
    ..aOS(3, _omitFieldNames ? '' : 'toHarness')
    ..aOS(4, _omitFieldNames ? '' : 'summarizerHarness')
    ..aOS(5, _omitFieldNames ? '' : 'summarizerModel')
    ..aOS(6, _omitFieldNames ? '' : 'summarizerProvider')
    ..aOB(7, _omitFieldNames ? '' : 'summarizerLocal')
    ..aOS(8, _omitFieldNames ? '' : 'summaryError')
    ..aOS(9, _omitFieldNames ? '' : 'path')
    ..aOS(10, _omitFieldNames ? '' : 'archivePath')
    ..aOM<$0.Timestamp>(11, _omitFieldNames ? '' : 'at',
        subBuilder: $0.Timestamp.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  Handoff clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  Handoff copyWith(void Function(Handoff) updates) =>
      super.copyWith((message) => updates(message as Handoff)) as Handoff;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use Handoff() / Handoff.new instead')
  static Handoff create() => Handoff._();
  static $pb.GeneratedMessage $_createMessage() => Handoff._();
  @$core.override
  Handoff createEmptyInstance() => Handoff._();
  @$core.pragma('dart2js:noInline')
  static Handoff getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<Handoff>(Handoff.$_createMessage);
  static Handoff? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get fromHarness => $_getSZ(0);
  @$pb.TagNumber(1)
  set fromHarness($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasFromHarness() => $_has(0);
  @$pb.TagNumber(1)
  void clearFromHarness() => $_clearField(1);

  /// from_session_id is the outgoing harness's native session, kept on disk
  /// for a later resume.
  @$pb.TagNumber(2)
  $core.String get fromSessionId => $_getSZ(1);
  @$pb.TagNumber(2)
  set fromSessionId($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasFromSessionId() => $_has(1);
  @$pb.TagNumber(2)
  void clearFromSessionId() => $_clearField(2);

  @$pb.TagNumber(3)
  $core.String get toHarness => $_getSZ(2);
  @$pb.TagNumber(3)
  set toHarness($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasToHarness() => $_has(2);
  @$pb.TagNumber(3)
  void clearToHarness() => $_clearField(3);

  /// summarizer_harness is empty when no summary was written; summary_error
  /// then says why, and the handoff holds the facts only.
  @$pb.TagNumber(4)
  $core.String get summarizerHarness => $_getSZ(3);
  @$pb.TagNumber(4)
  set summarizerHarness($core.String value) => $_setString(3, value);
  @$pb.TagNumber(4)
  $core.bool hasSummarizerHarness() => $_has(3);
  @$pb.TagNumber(4)
  void clearSummarizerHarness() => $_clearField(4);

  @$pb.TagNumber(5)
  $core.String get summarizerModel => $_getSZ(4);
  @$pb.TagNumber(5)
  set summarizerModel($core.String value) => $_setString(4, value);
  @$pb.TagNumber(5)
  $core.bool hasSummarizerModel() => $_has(4);
  @$pb.TagNumber(5)
  void clearSummarizerModel() => $_clearField(5);

  @$pb.TagNumber(6)
  $core.String get summarizerProvider => $_getSZ(5);
  @$pb.TagNumber(6)
  set summarizerProvider($core.String value) => $_setString(5, value);
  @$pb.TagNumber(6)
  $core.bool hasSummarizerProvider() => $_has(5);
  @$pb.TagNumber(6)
  void clearSummarizerProvider() => $_clearField(6);

  /// summarizer_local is true when the summary came from a model served by
  /// the user's own machine or network (the fallback when no subscription
  /// has spare quota): show "summarized by local model".
  @$pb.TagNumber(7)
  $core.bool get summarizerLocal => $_getBF(6);
  @$pb.TagNumber(7)
  set summarizerLocal($core.bool value) => $_setBool(6, value);
  @$pb.TagNumber(7)
  $core.bool hasSummarizerLocal() => $_has(6);
  @$pb.TagNumber(7)
  void clearSummarizerLocal() => $_clearField(7);

  @$pb.TagNumber(8)
  $core.String get summaryError => $_getSZ(7);
  @$pb.TagNumber(8)
  set summaryError($core.String value) => $_setString(7, value);
  @$pb.TagNumber(8)
  $core.bool hasSummaryError() => $_has(7);
  @$pb.TagNumber(8)
  void clearSummaryError() => $_clearField(8);

  /// path is the handoff file in the workspace; archive_path the hub's copy.
  @$pb.TagNumber(9)
  $core.String get path => $_getSZ(8);
  @$pb.TagNumber(9)
  set path($core.String value) => $_setString(8, value);
  @$pb.TagNumber(9)
  $core.bool hasPath() => $_has(8);
  @$pb.TagNumber(9)
  void clearPath() => $_clearField(9);

  @$pb.TagNumber(10)
  $core.String get archivePath => $_getSZ(9);
  @$pb.TagNumber(10)
  set archivePath($core.String value) => $_setString(9, value);
  @$pb.TagNumber(10)
  $core.bool hasArchivePath() => $_has(9);
  @$pb.TagNumber(10)
  void clearArchivePath() => $_clearField(10);

  @$pb.TagNumber(11)
  $0.Timestamp get at => $_getN(10);
  @$pb.TagNumber(11)
  set at($0.Timestamp value) => $_setField(11, value);
  @$pb.TagNumber(11)
  $core.bool hasAt() => $_has(10);
  @$pb.TagNumber(11)
  void clearAt() => $_clearField(11);
  @$pb.TagNumber(11)
  $0.Timestamp ensureAt() => $_ensure(10);
}

class WatchRequest extends $pb.GeneratedMessage {
  factory WatchRequest({
    $core.String? sleeve,
    $fixnum.Int64? afterSeq,
  }) {
    final result = WatchRequest._();
    if (sleeve != null) result.sleeve = sleeve;
    if (afterSeq != null) result.afterSeq = afterSeq;
    return result;
  }

  WatchRequest._();

  factory WatchRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      WatchRequest()..mergeFromBuffer(data, registry);
  factory WatchRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      WatchRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'WatchRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: WatchRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'sleeve')
    ..a<$fixnum.Int64>(
        2, _omitFieldNames ? '' : 'afterSeq', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  WatchRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  WatchRequest copyWith(void Function(WatchRequest) updates) =>
      super.copyWith((message) => updates(message as WatchRequest))
          as WatchRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use WatchRequest() / WatchRequest.new instead')
  static WatchRequest create() => WatchRequest._();
  static $pb.GeneratedMessage $_createMessage() => WatchRequest._();
  @$core.override
  WatchRequest createEmptyInstance() => WatchRequest._();
  @$core.pragma('dart2js:noInline')
  static WatchRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<WatchRequest>(
          WatchRequest.$_createMessage);
  static WatchRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get sleeve => $_getSZ(0);
  @$pb.TagNumber(1)
  set sleeve($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasSleeve() => $_has(0);
  @$pb.TagNumber(1)
  void clearSleeve() => $_clearField(1);

  @$pb.TagNumber(2)
  $fixnum.Int64 get afterSeq => $_getI64(1);
  @$pb.TagNumber(2)
  set afterSeq($fixnum.Int64 value) => $_setInt64(1, value);
  @$pb.TagNumber(2)
  $core.bool hasAfterSeq() => $_has(1);
  @$pb.TagNumber(2)
  void clearAfterSeq() => $_clearField(2);
}

class SendRequest extends $pb.GeneratedMessage {
  factory SendRequest({
    $core.String? sleeve,
    $core.String? text,
    $core.String? requestId,
  }) {
    final result = SendRequest._();
    if (sleeve != null) result.sleeve = sleeve;
    if (text != null) result.text = text;
    if (requestId != null) result.requestId = requestId;
    return result;
  }

  SendRequest._();

  factory SendRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      SendRequest()..mergeFromBuffer(data, registry);
  factory SendRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      SendRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'SendRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: SendRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'sleeve')
    ..aOS(2, _omitFieldNames ? '' : 'text')
    ..aOS(3, _omitFieldNames ? '' : 'requestId')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  SendRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  SendRequest copyWith(void Function(SendRequest) updates) =>
      super.copyWith((message) => updates(message as SendRequest))
          as SendRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use SendRequest() / SendRequest.new instead')
  static SendRequest create() => SendRequest._();
  static $pb.GeneratedMessage $_createMessage() => SendRequest._();
  @$core.override
  SendRequest createEmptyInstance() => SendRequest._();
  @$core.pragma('dart2js:noInline')
  static SendRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<SendRequest>(
          SendRequest.$_createMessage);
  static SendRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get sleeve => $_getSZ(0);
  @$pb.TagNumber(1)
  set sleeve($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasSleeve() => $_has(0);
  @$pb.TagNumber(1)
  void clearSleeve() => $_clearField(1);

  @$pb.TagNumber(2)
  $core.String get text => $_getSZ(1);
  @$pb.TagNumber(2)
  set text($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasText() => $_has(1);
  @$pb.TagNumber(2)
  void clearText() => $_clearField(2);

  @$pb.TagNumber(3)
  $core.String get requestId => $_getSZ(2);
  @$pb.TagNumber(3)
  set requestId($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasRequestId() => $_has(2);
  @$pb.TagNumber(3)
  void clearRequestId() => $_clearField(3);
}

class SendResponse extends $pb.GeneratedMessage {
  factory SendResponse({
    $fixnum.Int64? seq,
  }) {
    final result = SendResponse._();
    if (seq != null) result.seq = seq;
    return result;
  }

  SendResponse._();

  factory SendResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      SendResponse()..mergeFromBuffer(data, registry);
  factory SendResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      SendResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'SendResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: SendResponse.$_createMessage)
    ..a<$fixnum.Int64>(1, _omitFieldNames ? '' : 'seq', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  SendResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  SendResponse copyWith(void Function(SendResponse) updates) =>
      super.copyWith((message) => updates(message as SendResponse))
          as SendResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use SendResponse() / SendResponse.new instead')
  static SendResponse create() => SendResponse._();
  static $pb.GeneratedMessage $_createMessage() => SendResponse._();
  @$core.override
  SendResponse createEmptyInstance() => SendResponse._();
  @$core.pragma('dart2js:noInline')
  static SendResponse getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<SendResponse>(
          SendResponse.$_createMessage);
  static SendResponse? _defaultInstance;

  /// seq is the UserMessage event recording this send.
  @$pb.TagNumber(1)
  $fixnum.Int64 get seq => $_getI64(0);
  @$pb.TagNumber(1)
  set seq($fixnum.Int64 value) => $_setInt64(0, value);
  @$pb.TagNumber(1)
  $core.bool hasSeq() => $_has(0);
  @$pb.TagNumber(1)
  void clearSeq() => $_clearField(1);
}

class InterruptRequest extends $pb.GeneratedMessage {
  factory InterruptRequest({
    $core.String? sleeve,
  }) {
    final result = InterruptRequest._();
    if (sleeve != null) result.sleeve = sleeve;
    return result;
  }

  InterruptRequest._();

  factory InterruptRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      InterruptRequest()..mergeFromBuffer(data, registry);
  factory InterruptRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      InterruptRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'InterruptRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: InterruptRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'sleeve')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  InterruptRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  InterruptRequest copyWith(void Function(InterruptRequest) updates) =>
      super.copyWith((message) => updates(message as InterruptRequest))
          as InterruptRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use InterruptRequest() / InterruptRequest.new instead')
  static InterruptRequest create() => InterruptRequest._();
  static $pb.GeneratedMessage $_createMessage() => InterruptRequest._();
  @$core.override
  InterruptRequest createEmptyInstance() => InterruptRequest._();
  @$core.pragma('dart2js:noInline')
  static InterruptRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<InterruptRequest>(
          InterruptRequest.$_createMessage);
  static InterruptRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get sleeve => $_getSZ(0);
  @$pb.TagNumber(1)
  set sleeve($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasSleeve() => $_has(0);
  @$pb.TagNumber(1)
  void clearSleeve() => $_clearField(1);
}

class InterruptResponse extends $pb.GeneratedMessage {
  factory InterruptResponse() => InterruptResponse._();

  InterruptResponse._();

  factory InterruptResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      InterruptResponse()..mergeFromBuffer(data, registry);
  factory InterruptResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      InterruptResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'InterruptResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: InterruptResponse.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  InterruptResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  InterruptResponse copyWith(void Function(InterruptResponse) updates) =>
      super.copyWith((message) => updates(message as InterruptResponse))
          as InterruptResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use InterruptResponse() / InterruptResponse.new instead')
  static InterruptResponse create() => InterruptResponse._();
  static $pb.GeneratedMessage $_createMessage() => InterruptResponse._();
  @$core.override
  InterruptResponse createEmptyInstance() => InterruptResponse._();
  @$core.pragma('dart2js:noInline')
  static InterruptResponse getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<InterruptResponse>(
          InterruptResponse.$_createMessage);
  static InterruptResponse? _defaultInstance;
}

class RespondRequest extends $pb.GeneratedMessage {
  factory RespondRequest({
    $core.String? sleeve,
    $core.String? approvalId,
    $core.bool? allow,
    $core.String? note,
  }) {
    final result = RespondRequest._();
    if (sleeve != null) result.sleeve = sleeve;
    if (approvalId != null) result.approvalId = approvalId;
    if (allow != null) result.allow = allow;
    if (note != null) result.note = note;
    return result;
  }

  RespondRequest._();

  factory RespondRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      RespondRequest()..mergeFromBuffer(data, registry);
  factory RespondRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      RespondRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'RespondRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: RespondRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'sleeve')
    ..aOS(2, _omitFieldNames ? '' : 'approvalId')
    ..aOB(3, _omitFieldNames ? '' : 'allow')
    ..aOS(4, _omitFieldNames ? '' : 'note')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  RespondRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  RespondRequest copyWith(void Function(RespondRequest) updates) =>
      super.copyWith((message) => updates(message as RespondRequest))
          as RespondRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use RespondRequest() / RespondRequest.new instead')
  static RespondRequest create() => RespondRequest._();
  static $pb.GeneratedMessage $_createMessage() => RespondRequest._();
  @$core.override
  RespondRequest createEmptyInstance() => RespondRequest._();
  @$core.pragma('dart2js:noInline')
  static RespondRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<RespondRequest>(
          RespondRequest.$_createMessage);
  static RespondRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get sleeve => $_getSZ(0);
  @$pb.TagNumber(1)
  set sleeve($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasSleeve() => $_has(0);
  @$pb.TagNumber(1)
  void clearSleeve() => $_clearField(1);

  @$pb.TagNumber(2)
  $core.String get approvalId => $_getSZ(1);
  @$pb.TagNumber(2)
  set approvalId($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasApprovalId() => $_has(1);
  @$pb.TagNumber(2)
  void clearApprovalId() => $_clearField(2);

  @$pb.TagNumber(3)
  $core.bool get allow => $_getBF(2);
  @$pb.TagNumber(3)
  set allow($core.bool value) => $_setBool(2, value);
  @$pb.TagNumber(3)
  $core.bool hasAllow() => $_has(2);
  @$pb.TagNumber(3)
  void clearAllow() => $_clearField(3);

  @$pb.TagNumber(4)
  $core.String get note => $_getSZ(3);
  @$pb.TagNumber(4)
  set note($core.String value) => $_setString(3, value);
  @$pb.TagNumber(4)
  $core.bool hasNote() => $_has(3);
  @$pb.TagNumber(4)
  void clearNote() => $_clearField(4);
}

class RespondResponse extends $pb.GeneratedMessage {
  factory RespondResponse() => RespondResponse._();

  RespondResponse._();

  factory RespondResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      RespondResponse()..mergeFromBuffer(data, registry);
  factory RespondResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      RespondResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'RespondResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: RespondResponse.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  RespondResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  RespondResponse copyWith(void Function(RespondResponse) updates) =>
      super.copyWith((message) => updates(message as RespondResponse))
          as RespondResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use RespondResponse() / RespondResponse.new instead')
  static RespondResponse create() => RespondResponse._();
  static $pb.GeneratedMessage $_createMessage() => RespondResponse._();
  @$core.override
  RespondResponse createEmptyInstance() => RespondResponse._();
  @$core.pragma('dart2js:noInline')
  static RespondResponse getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<RespondResponse>(
          RespondResponse.$_createMessage);
  static RespondResponse? _defaultInstance;
}

class CompactRequest extends $pb.GeneratedMessage {
  factory CompactRequest({
    $core.String? sleeve,
  }) {
    final result = CompactRequest._();
    if (sleeve != null) result.sleeve = sleeve;
    return result;
  }

  CompactRequest._();

  factory CompactRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      CompactRequest()..mergeFromBuffer(data, registry);
  factory CompactRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      CompactRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'CompactRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: CompactRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'sleeve')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  CompactRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  CompactRequest copyWith(void Function(CompactRequest) updates) =>
      super.copyWith((message) => updates(message as CompactRequest))
          as CompactRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use CompactRequest() / CompactRequest.new instead')
  static CompactRequest create() => CompactRequest._();
  static $pb.GeneratedMessage $_createMessage() => CompactRequest._();
  @$core.override
  CompactRequest createEmptyInstance() => CompactRequest._();
  @$core.pragma('dart2js:noInline')
  static CompactRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<CompactRequest>(
          CompactRequest.$_createMessage);
  static CompactRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get sleeve => $_getSZ(0);
  @$pb.TagNumber(1)
  set sleeve($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasSleeve() => $_has(0);
  @$pb.TagNumber(1)
  void clearSleeve() => $_clearField(1);
}

class CompactResponse extends $pb.GeneratedMessage {
  factory CompactResponse({
    Compaction? compaction,
  }) {
    final result = CompactResponse._();
    if (compaction != null) result.compaction = compaction;
    return result;
  }

  CompactResponse._();

  factory CompactResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      CompactResponse()..mergeFromBuffer(data, registry);
  factory CompactResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      CompactResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'CompactResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: CompactResponse.$_createMessage)
    ..aOM<Compaction>(1, _omitFieldNames ? '' : 'compaction',
        subBuilder: Compaction.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  CompactResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  CompactResponse copyWith(void Function(CompactResponse) updates) =>
      super.copyWith((message) => updates(message as CompactResponse))
          as CompactResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use CompactResponse() / CompactResponse.new instead')
  static CompactResponse create() => CompactResponse._();
  static $pb.GeneratedMessage $_createMessage() => CompactResponse._();
  @$core.override
  CompactResponse createEmptyInstance() => CompactResponse._();
  @$core.pragma('dart2js:noInline')
  static CompactResponse getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<CompactResponse>(
          CompactResponse.$_createMessage);
  static CompactResponse? _defaultInstance;

  @$pb.TagNumber(1)
  Compaction get compaction => $_getN(0);
  @$pb.TagNumber(1)
  set compaction(Compaction value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasCompaction() => $_has(0);
  @$pb.TagNumber(1)
  void clearCompaction() => $_clearField(1);
  @$pb.TagNumber(1)
  Compaction ensureCompaction() => $_ensure(0);
}

class SleeveResult extends $pb.GeneratedMessage {
  factory SleeveResult({
    ReportStatus? status,
    $core.String? summary,
    $0.Timestamp? time,
    $fixnum.Int64? chatSeq,
    $fixnum.Int64? briefSeq,
    $fixnum.Int64? sourceSeq,
  }) {
    final result = SleeveResult._();
    if (status != null) result.status = status;
    if (summary != null) result.summary = summary;
    if (time != null) result.time = time;
    if (chatSeq != null) result.chatSeq = chatSeq;
    if (briefSeq != null) result.briefSeq = briefSeq;
    if (sourceSeq != null) result.sourceSeq = sourceSeq;
    return result;
  }

  SleeveResult._();

  factory SleeveResult.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      SleeveResult()..mergeFromBuffer(data, registry);
  factory SleeveResult.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      SleeveResult()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'SleeveResult',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: SleeveResult.$_createMessage)
    ..aE<ReportStatus>(1, _omitFieldNames ? '' : 'status',
        enumValues: ReportStatus.values)
    ..aOS(2, _omitFieldNames ? '' : 'summary')
    ..aOM<$0.Timestamp>(3, _omitFieldNames ? '' : 'time',
        subBuilder: $0.Timestamp.$_createMessage)
    ..a<$fixnum.Int64>(4, _omitFieldNames ? '' : 'chatSeq', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..a<$fixnum.Int64>(
        5, _omitFieldNames ? '' : 'briefSeq', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..a<$fixnum.Int64>(
        6, _omitFieldNames ? '' : 'sourceSeq', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  SleeveResult clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  SleeveResult copyWith(void Function(SleeveResult) updates) =>
      super.copyWith((message) => updates(message as SleeveResult))
          as SleeveResult;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use SleeveResult() / SleeveResult.new instead')
  static SleeveResult create() => SleeveResult._();
  static $pb.GeneratedMessage $_createMessage() => SleeveResult._();
  @$core.override
  SleeveResult createEmptyInstance() => SleeveResult._();
  @$core.pragma('dart2js:noInline')
  static SleeveResult getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<SleeveResult>(
          SleeveResult.$_createMessage);
  static SleeveResult? _defaultInstance;

  @$pb.TagNumber(1)
  ReportStatus get status => $_getN(0);
  @$pb.TagNumber(1)
  set status(ReportStatus value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasStatus() => $_has(0);
  @$pb.TagNumber(1)
  void clearStatus() => $_clearField(1);

  /// At most 32 KiB, the same limit as mail bodies.
  @$pb.TagNumber(2)
  $core.String get summary => $_getSZ(1);
  @$pb.TagNumber(2)
  set summary($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasSummary() => $_has(1);
  @$pb.TagNumber(2)
  void clearSummary() => $_clearField(2);

  @$pb.TagNumber(3)
  $0.Timestamp get time => $_getN(2);
  @$pb.TagNumber(3)
  set time($0.Timestamp value) => $_setField(3, value);
  @$pb.TagNumber(3)
  $core.bool hasTime() => $_has(2);
  @$pb.TagNumber(3)
  void clearTime() => $_clearField(3);
  @$pb.TagNumber(3)
  $0.Timestamp ensureTime() => $_ensure(2);

  @$pb.TagNumber(4)
  $fixnum.Int64 get chatSeq => $_getI64(3);
  @$pb.TagNumber(4)
  set chatSeq($fixnum.Int64 value) => $_setInt64(3, value);
  @$pb.TagNumber(4)
  $core.bool hasChatSeq() => $_has(3);
  @$pb.TagNumber(4)
  void clearChatSeq() => $_clearField(4);

  /// The latest non-mail user message; zero for the initial spawn brief.
  @$pb.TagNumber(5)
  $fixnum.Int64 get briefSeq => $_getI64(4);
  @$pb.TagNumber(5)
  set briefSeq($fixnum.Int64 value) => $_setInt64(4, value);
  @$pb.TagNumber(5)
  $core.bool hasBriefSeq() => $_has(4);
  @$pb.TagNumber(5)
  void clearBriefSeq() => $_clearField(5);

  /// source_seq is the sequence the sleeve gave the report; zero if unsequenced.
  @$pb.TagNumber(6)
  $fixnum.Int64 get sourceSeq => $_getI64(5);
  @$pb.TagNumber(6)
  set sourceSeq($fixnum.Int64 value) => $_setInt64(5, value);
  @$pb.TagNumber(6)
  $core.bool hasSourceSeq() => $_has(5);
  @$pb.TagNumber(6)
  void clearSourceSeq() => $_clearField(6);
}

class ReportRequest extends $pb.GeneratedMessage {
  factory ReportRequest({
    $core.String? sleeve,
    ReportStatus? status,
    $core.String? summary,
    $fixnum.Int64? sourceSeq,
  }) {
    final result = ReportRequest._();
    if (sleeve != null) result.sleeve = sleeve;
    if (status != null) result.status = status;
    if (summary != null) result.summary = summary;
    if (sourceSeq != null) result.sourceSeq = sourceSeq;
    return result;
  }

  ReportRequest._();

  factory ReportRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ReportRequest()..mergeFromBuffer(data, registry);
  factory ReportRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ReportRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ReportRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ReportRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'sleeve')
    ..aE<ReportStatus>(2, _omitFieldNames ? '' : 'status',
        enumValues: ReportStatus.values)
    ..aOS(3, _omitFieldNames ? '' : 'summary')
    ..a<$fixnum.Int64>(
        4, _omitFieldNames ? '' : 'sourceSeq', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ReportRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ReportRequest copyWith(void Function(ReportRequest) updates) =>
      super.copyWith((message) => updates(message as ReportRequest))
          as ReportRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use ReportRequest() / ReportRequest.new instead')
  static ReportRequest create() => ReportRequest._();
  static $pb.GeneratedMessage $_createMessage() => ReportRequest._();
  @$core.override
  ReportRequest createEmptyInstance() => ReportRequest._();
  @$core.pragma('dart2js:noInline')
  static ReportRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<ReportRequest>(
          ReportRequest.$_createMessage);
  static ReportRequest? _defaultInstance;

  /// Empty selects the authenticated sleeve. An explicit name must be its own.
  @$pb.TagNumber(1)
  $core.String get sleeve => $_getSZ(0);
  @$pb.TagNumber(1)
  set sleeve($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasSleeve() => $_has(0);
  @$pb.TagNumber(1)
  void clearSleeve() => $_clearField(1);

  @$pb.TagNumber(2)
  ReportStatus get status => $_getN(1);
  @$pb.TagNumber(2)
  set status(ReportStatus value) => $_setField(2, value);
  @$pb.TagNumber(2)
  $core.bool hasStatus() => $_has(1);
  @$pb.TagNumber(2)
  void clearStatus() => $_clearField(2);

  @$pb.TagNumber(3)
  $core.String get summary => $_getSZ(2);
  @$pb.TagNumber(3)
  set summary($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasSummary() => $_has(2);
  @$pb.TagNumber(3)
  void clearSummary() => $_clearField(3);

  /// source_seq orders the sleeve's reports: it must grow with each report the
  /// sleeve sends, and a report whose source_seq is not above the last
  /// sequenced one the hub recorded for that sleeve is dropped, so a late or
  /// repeated report cannot replace a newer one. Zero is unsequenced and always
  /// recorded, in arrival order.
  @$pb.TagNumber(4)
  $fixnum.Int64 get sourceSeq => $_getI64(3);
  @$pb.TagNumber(4)
  set sourceSeq($fixnum.Int64 value) => $_setInt64(3, value);
  @$pb.TagNumber(4)
  $core.bool hasSourceSeq() => $_has(3);
  @$pb.TagNumber(4)
  void clearSourceSeq() => $_clearField(4);
}

class ReportResponse extends $pb.GeneratedMessage {
  factory ReportResponse({
    SleeveResult? result,
    $core.bool? dropped,
  }) {
    final result$ = ReportResponse._();
    if (result != null) result$.result = result;
    if (dropped != null) result$.dropped = dropped;
    return result$;
  }

  ReportResponse._();

  factory ReportResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ReportResponse()..mergeFromBuffer(data, registry);
  factory ReportResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ReportResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ReportResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ReportResponse.$_createMessage)
    ..aOM<SleeveResult>(1, _omitFieldNames ? '' : 'result',
        subBuilder: SleeveResult.$_createMessage)
    ..aOB(2, _omitFieldNames ? '' : 'dropped')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ReportResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ReportResponse copyWith(void Function(ReportResponse) updates) =>
      super.copyWith((message) => updates(message as ReportResponse))
          as ReportResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use ReportResponse() / ReportResponse.new instead')
  static ReportResponse create() => ReportResponse._();
  static $pb.GeneratedMessage $_createMessage() => ReportResponse._();
  @$core.override
  ReportResponse createEmptyInstance() => ReportResponse._();
  @$core.pragma('dart2js:noInline')
  static ReportResponse getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<ReportResponse>(
          ReportResponse.$_createMessage);
  static ReportResponse? _defaultInstance;

  /// result is the recorded result: the new report, or for a dropped one the
  /// result the sleeve already has.
  @$pb.TagNumber(1)
  SleeveResult get result => $_getN(0);
  @$pb.TagNumber(1)
  set result(SleeveResult value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasResult() => $_has(0);
  @$pb.TagNumber(1)
  void clearResult() => $_clearField(1);
  @$pb.TagNumber(1)
  SleeveResult ensureResult() => $_ensure(0);

  @$pb.TagNumber(2)
  $core.bool get dropped => $_getBF(1);
  @$pb.TagNumber(2)
  set dropped($core.bool value) => $_setBool(1, value);
  @$pb.TagNumber(2)
  $core.bool hasDropped() => $_has(1);
  @$pb.TagNumber(2)
  void clearDropped() => $_clearField(2);
}

class WaitRequest extends $pb.GeneratedMessage {
  factory WaitRequest({
    $core.String? sleeve,
    WaitUntil? until,
    $1.Duration? timeout,
  }) {
    final result = WaitRequest._();
    if (sleeve != null) result.sleeve = sleeve;
    if (until != null) result.until = until;
    if (timeout != null) result.timeout = timeout;
    return result;
  }

  WaitRequest._();

  factory WaitRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      WaitRequest()..mergeFromBuffer(data, registry);
  factory WaitRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      WaitRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'WaitRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: WaitRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'sleeve')
    ..aE<WaitUntil>(2, _omitFieldNames ? '' : 'until',
        enumValues: WaitUntil.values)
    ..aOM<$1.Duration>(3, _omitFieldNames ? '' : 'timeout',
        subBuilder: $1.Duration.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  WaitRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  WaitRequest copyWith(void Function(WaitRequest) updates) =>
      super.copyWith((message) => updates(message as WaitRequest))
          as WaitRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use WaitRequest() / WaitRequest.new instead')
  static WaitRequest create() => WaitRequest._();
  static $pb.GeneratedMessage $_createMessage() => WaitRequest._();
  @$core.override
  WaitRequest createEmptyInstance() => WaitRequest._();
  @$core.pragma('dart2js:noInline')
  static WaitRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<WaitRequest>(
          WaitRequest.$_createMessage);
  static WaitRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get sleeve => $_getSZ(0);
  @$pb.TagNumber(1)
  set sleeve($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasSleeve() => $_has(0);
  @$pb.TagNumber(1)
  void clearSleeve() => $_clearField(1);

  @$pb.TagNumber(2)
  WaitUntil get until => $_getN(1);
  @$pb.TagNumber(2)
  set until(WaitUntil value) => $_setField(2, value);
  @$pb.TagNumber(2)
  $core.bool hasUntil() => $_has(1);
  @$pb.TagNumber(2)
  void clearUntil() => $_clearField(2);

  /// Unset defaults to ten minutes. Zero waits until cancellation.
  @$pb.TagNumber(3)
  $1.Duration get timeout => $_getN(2);
  @$pb.TagNumber(3)
  set timeout($1.Duration value) => $_setField(3, value);
  @$pb.TagNumber(3)
  $core.bool hasTimeout() => $_has(2);
  @$pb.TagNumber(3)
  void clearTimeout() => $_clearField(3);
  @$pb.TagNumber(3)
  $1.Duration ensureTimeout() => $_ensure(2);
}

class WaitResponse extends $pb.GeneratedMessage {
  factory WaitResponse({
    $core.String? sleeve,
    SleeveResult? result,
  }) {
    final result$ = WaitResponse._();
    if (sleeve != null) result$.sleeve = sleeve;
    if (result != null) result$.result = result;
    return result$;
  }

  WaitResponse._();

  factory WaitResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      WaitResponse()..mergeFromBuffer(data, registry);
  factory WaitResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      WaitResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'WaitResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: WaitResponse.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'sleeve')
    ..aOM<SleeveResult>(2, _omitFieldNames ? '' : 'result',
        subBuilder: SleeveResult.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  WaitResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  WaitResponse copyWith(void Function(WaitResponse) updates) =>
      super.copyWith((message) => updates(message as WaitResponse))
          as WaitResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use WaitResponse() / WaitResponse.new instead')
  static WaitResponse create() => WaitResponse._();
  static $pb.GeneratedMessage $_createMessage() => WaitResponse._();
  @$core.override
  WaitResponse createEmptyInstance() => WaitResponse._();
  @$core.pragma('dart2js:noInline')
  static WaitResponse getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<WaitResponse>(
          WaitResponse.$_createMessage);
  static WaitResponse? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get sleeve => $_getSZ(0);
  @$pb.TagNumber(1)
  set sleeve($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasSleeve() => $_has(0);
  @$pb.TagNumber(1)
  void clearSleeve() => $_clearField(1);

  @$pb.TagNumber(2)
  SleeveResult get result => $_getN(1);
  @$pb.TagNumber(2)
  set result(SleeveResult value) => $_setField(2, value);
  @$pb.TagNumber(2)
  $core.bool hasResult() => $_has(1);
  @$pb.TagNumber(2)
  void clearResult() => $_clearField(2);
  @$pb.TagNumber(2)
  SleeveResult ensureResult() => $_ensure(1);
}

/// ChatService drives a sleeve in chat mode: the harness runs its own official
/// headless protocol inside the sleeve and the hub normalizes what it emits
/// into ChatEvents. The hub keeps each sleeve's events in an append-only log,
/// so any number of clients can watch and a reconnecting client resumes from
/// the last seq it saw. The harness's own session store stays the full record;
/// the log is the view. A sleeve is in chat mode for life, or in terminal mode
/// and then has only self-reported result events in this log.
class ChatServiceApi {
  final $pb.RpcClient _client;

  ChatServiceApi(this._client);

  /// Watch replays the sleeve's events after after_seq, then streams new ones
  /// until the client disconnects or the sleeve is killed. Terminal sleeves
  /// expose their result history without starting a headless harness.
  $async.Future<ChatEvent> watch(
          $pb.ClientContext? ctx, WatchRequest request) =>
      _client.invoke<ChatEvent>(
          ctx, 'ChatService', 'Watch', request, ChatEvent());

  /// Send delivers a user message. It starts a turn when the harness is idle;
  /// during a turn it steers that turn on harnesses that support it and fails
  /// with failed_precondition on the rest. Retrying with the same request_id
  /// returns the original result instead of sending twice.
  $async.Future<SendResponse> send(
          $pb.ClientContext? ctx, SendRequest request) =>
      _client.invoke<SendResponse>(
          ctx, 'ChatService', 'Send', request, SendResponse());

  /// Interrupt cancels the running turn. It is a no-op when the harness is idle.
  $async.Future<InterruptResponse> interrupt(
          $pb.ClientContext? ctx, InterruptRequest request) =>
      _client.invoke<InterruptResponse>(
          ctx, 'ChatService', 'Interrupt', request, InterruptResponse());

  /// Respond answers a pending approval.
  $async.Future<RespondResponse> respond(
          $pb.ClientContext? ctx, RespondRequest request) =>
      _client.invoke<RespondResponse>(
          ctx, 'ChatService', 'Respond', request, RespondResponse());

  /// Compact runs the harness's own native compaction between turns. It fails
  /// with failed_precondition while a turn is running or an approval is
  /// pending, and returns after the harness has confirmed the compaction.
  $async.Future<CompactResponse> compact(
          $pb.ClientContext? ctx, CompactRequest request) =>
      _client.invoke<CompactResponse>(
          ctx, 'ChatService', 'Compact', request, CompactResponse());

  /// Native catalog; no inference turn is started. Select by sleeve or harness.
  $async.Future<ListModelsResponse> listModels(
          $pb.ClientContext? ctx, ListModelsRequest request) =>
      _client.invoke<ListModelsResponse>(
          ctx, 'ChatService', 'ListModels', request, ListModelsResponse());

  /// Replace the requested selection between turns, without restarting.
  /// Empty fields restore the captured native defaults. Unsupported drivers fail.
  $async.Future<SetSelectionResponse> setSelection(
          $pb.ClientContext? ctx, SetSelectionRequest request) =>
      _client.invoke<SetSelectionResponse>(
          ctx, 'ChatService', 'SetSelection', request, SetSelectionResponse());

  /// Report replaces this sleeve's result for its current brief. Only the
  /// sleeve's own identity token may report, never the owner or another sleeve.
  $async.Future<ReportResponse> report(
          $pb.ClientContext? ctx, ReportRequest request) =>
      _client.invoke<ReportResponse>(
          ctx, 'ChatService', 'Report', request, ReportResponse());

  /// Wait blocks on hub events until a sleeve is idle without pending mail,
  /// or has reported for its current brief. Exits and timeouts fail the call.
  /// Available as the read-only MCP tool chat_wait; no polling is needed.
  $async.Future<WaitResponse> wait(
          $pb.ClientContext? ctx, WaitRequest request) =>
      _client.invoke<WaitResponse>(
          ctx, 'ChatService', 'Wait', request, WaitResponse());
}

const $core.bool _omitFieldNames =
    $core.bool.fromEnvironment('protobuf.omit_field_names');
const $core.bool _omitMessageNames =
    $core.bool.fromEnvironment('protobuf.omit_message_names');
