// This is a generated file - do not edit.
//
// Generated from protectorate/v1/backlog.proto.

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

import 'backlog.pbenum.dart';

export 'package:protobuf/protobuf.dart' show GeneratedMessageGenericExtensions;

export 'backlog.pbenum.dart';

/// Claim records who took an item and when.
class Claim extends $pb.GeneratedMessage {
  factory Claim({
    $core.String? by,
    $0.Timestamp? at,
  }) {
    final result = Claim._();
    if (by != null) result.by = by;
    if (at != null) result.at = at;
    return result;
  }

  Claim._();

  factory Claim.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      Claim()..mergeFromBuffer(data, registry);
  factory Claim.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      Claim()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'Claim',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: Claim.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'by')
    ..aOM<$0.Timestamp>(2, _omitFieldNames ? '' : 'at',
        subBuilder: $0.Timestamp.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  Claim clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  Claim copyWith(void Function(Claim) updates) =>
      super.copyWith((message) => updates(message as Claim)) as Claim;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use Claim() / Claim.new instead')
  static Claim create() => Claim._();
  static $pb.GeneratedMessage $_createMessage() => Claim._();
  @$core.override
  Claim createEmptyInstance() => Claim._();
  @$core.pragma('dart2js:noInline')
  static Claim getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<Claim>(Claim.$_createMessage);
  static Claim? _defaultInstance;

  /// by is "<sleeve>" or "<sleeve>/<harness>"; no whitespace or '@'.
  @$pb.TagNumber(1)
  $core.String get by => $_getSZ(0);
  @$pb.TagNumber(1)
  set by($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasBy() => $_has(0);
  @$pb.TagNumber(1)
  void clearBy() => $_clearField(1);

  @$pb.TagNumber(2)
  $0.Timestamp get at => $_getN(1);
  @$pb.TagNumber(2)
  set at($0.Timestamp value) => $_setField(2, value);
  @$pb.TagNumber(2)
  $core.bool hasAt() => $_has(1);
  @$pb.TagNumber(2)
  void clearAt() => $_clearField(2);
  @$pb.TagNumber(2)
  $0.Timestamp ensureAt() => $_ensure(1);
}

/// Item is one backlog entry.
class Item extends $pb.GeneratedMessage {
  factory Item({
    $core.String? id,
    $core.String? project,
    $core.String? title,
    $core.String? issueUrl,
    $core.int? priority,
    $core.String? harness,
    ItemState? state,
    Claim? claim,
    $core.String? notes,
    $core.bool? donate,
  }) {
    final result = Item._();
    if (id != null) result.id = id;
    if (project != null) result.project = project;
    if (title != null) result.title = title;
    if (issueUrl != null) result.issueUrl = issueUrl;
    if (priority != null) result.priority = priority;
    if (harness != null) result.harness = harness;
    if (state != null) result.state = state;
    if (claim != null) result.claim = claim;
    if (notes != null) result.notes = notes;
    if (donate != null) result.donate = donate;
    return result;
  }

  Item._();

  factory Item.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      Item()..mergeFromBuffer(data, registry);
  factory Item.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      Item()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'Item',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: Item.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'id')
    ..aOS(2, _omitFieldNames ? '' : 'project')
    ..aOS(3, _omitFieldNames ? '' : 'title')
    ..aOS(4, _omitFieldNames ? '' : 'issueUrl')
    ..aI(5, _omitFieldNames ? '' : 'priority')
    ..aOS(6, _omitFieldNames ? '' : 'harness')
    ..aE<ItemState>(7, _omitFieldNames ? '' : 'state',
        enumValues: ItemState.values)
    ..aOM<Claim>(8, _omitFieldNames ? '' : 'claim',
        subBuilder: Claim.$_createMessage)
    ..aOS(9, _omitFieldNames ? '' : 'notes')
    ..aOB(10, _omitFieldNames ? '' : 'donate')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  Item clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  Item copyWith(void Function(Item) updates) =>
      super.copyWith((message) => updates(message as Item)) as Item;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use Item() / Item.new instead')
  static Item create() => Item._();
  static $pb.GeneratedMessage $_createMessage() => Item._();
  @$core.override
  Item createEmptyInstance() => Item._();
  @$core.pragma('dart2js:noInline')
  static Item getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<Item>(Item.$_createMessage);
  static Item? _defaultInstance;

  /// id is the stable item identifier assigned on creation.
  @$pb.TagNumber(1)
  $core.String get id => $_getSZ(0);
  @$pb.TagNumber(1)
  set id($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasId() => $_has(0);
  @$pb.TagNumber(1)
  void clearId() => $_clearField(1);

  /// project is the heading text, e.g. "hotschmoe/protectorate" or "donate: owner/repo".
  @$pb.TagNumber(2)
  $core.String get project => $_getSZ(1);
  @$pb.TagNumber(2)
  set project($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasProject() => $_has(1);
  @$pb.TagNumber(2)
  void clearProject() => $_clearField(2);

  /// title is the one-line description of the work.
  @$pb.TagNumber(3)
  $core.String get title => $_getSZ(2);
  @$pb.TagNumber(3)
  set title($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasTitle() => $_has(2);
  @$pb.TagNumber(3)
  void clearTitle() => $_clearField(3);

  /// issue_url optionally links the GitHub issue.
  @$pb.TagNumber(4)
  $core.String get issueUrl => $_getSZ(3);
  @$pb.TagNumber(4)
  set issueUrl($core.String value) => $_setString(3, value);
  @$pb.TagNumber(4)
  $core.bool hasIssueUrl() => $_has(3);
  @$pb.TagNumber(4)
  void clearIssueUrl() => $_clearField(4);

  /// priority is 1 (highest) to 9; 0 means unset.
  @$pb.TagNumber(5)
  $core.int get priority => $_getIZ(4);
  @$pb.TagNumber(5)
  set priority($core.int value) => $_setSignedInt32(4, value);
  @$pb.TagNumber(5)
  $core.bool hasPriority() => $_has(4);
  @$pb.TagNumber(5)
  void clearPriority() => $_clearField(5);

  /// harness is a preference such as "codex"; empty or "any" means no preference.
  @$pb.TagNumber(6)
  $core.String get harness => $_getSZ(5);
  @$pb.TagNumber(6)
  set harness($core.String value) => $_setString(5, value);
  @$pb.TagNumber(6)
  $core.bool hasHarness() => $_has(5);
  @$pb.TagNumber(6)
  void clearHarness() => $_clearField(6);

  @$pb.TagNumber(7)
  ItemState get state => $_getN(6);
  @$pb.TagNumber(7)
  set state(ItemState value) => $_setField(7, value);
  @$pb.TagNumber(7)
  $core.bool hasState() => $_has(6);
  @$pb.TagNumber(7)
  void clearState() => $_clearField(7);

  @$pb.TagNumber(8)
  Claim get claim => $_getN(7);
  @$pb.TagNumber(8)
  set claim(Claim value) => $_setField(8, value);
  @$pb.TagNumber(8)
  $core.bool hasClaim() => $_has(7);
  @$pb.TagNumber(8)
  void clearClaim() => $_clearField(8);
  @$pb.TagNumber(8)
  Claim ensureClaim() => $_ensure(7);

  /// notes is free-form text attached to the item.
  @$pb.TagNumber(9)
  $core.String get notes => $_getSZ(8);
  @$pb.TagNumber(9)
  set notes($core.String value) => $_setString(8, value);
  @$pb.TagNumber(9)
  $core.bool hasNotes() => $_has(8);
  @$pb.TagNumber(9)
  void clearNotes() => $_clearField(9);

  /// donate marks public repos we do not own; results must go to a review inbox.
  @$pb.TagNumber(10)
  $core.bool get donate => $_getBF(9);
  @$pb.TagNumber(10)
  set donate($core.bool value) => $_setBool(9, value);
  @$pb.TagNumber(10)
  $core.bool hasDonate() => $_has(9);
  @$pb.TagNumber(10)
  void clearDonate() => $_clearField(10);
}

class ListItemsRequest extends $pb.GeneratedMessage {
  factory ListItemsRequest({
    $core.String? project,
    ItemState? state,
    $core.String? harness,
  }) {
    final result = ListItemsRequest._();
    if (project != null) result.project = project;
    if (state != null) result.state = state;
    if (harness != null) result.harness = harness;
    return result;
  }

  ListItemsRequest._();

  factory ListItemsRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListItemsRequest()..mergeFromBuffer(data, registry);
  factory ListItemsRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListItemsRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ListItemsRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ListItemsRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'project')
    ..aE<ItemState>(2, _omitFieldNames ? '' : 'state',
        enumValues: ItemState.values)
    ..aOS(3, _omitFieldNames ? '' : 'harness')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListItemsRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListItemsRequest copyWith(void Function(ListItemsRequest) updates) =>
      super.copyWith((message) => updates(message as ListItemsRequest))
          as ListItemsRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use ListItemsRequest() / ListItemsRequest.new instead')
  static ListItemsRequest create() => ListItemsRequest._();
  static $pb.GeneratedMessage $_createMessage() => ListItemsRequest._();
  @$core.override
  ListItemsRequest createEmptyInstance() => ListItemsRequest._();
  @$core.pragma('dart2js:noInline')
  static ListItemsRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<ListItemsRequest>(
          ListItemsRequest.$_createMessage);
  static ListItemsRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get project => $_getSZ(0);
  @$pb.TagNumber(1)
  set project($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasProject() => $_has(0);
  @$pb.TagNumber(1)
  void clearProject() => $_clearField(1);

  /// state of UNSPECIFIED matches every state.
  @$pb.TagNumber(2)
  ItemState get state => $_getN(1);
  @$pb.TagNumber(2)
  set state(ItemState value) => $_setField(2, value);
  @$pb.TagNumber(2)
  $core.bool hasState() => $_has(1);
  @$pb.TagNumber(2)
  void clearState() => $_clearField(2);

  /// harness keeps items this harness may run: its own plus "any"/unset.
  @$pb.TagNumber(3)
  $core.String get harness => $_getSZ(2);
  @$pb.TagNumber(3)
  set harness($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasHarness() => $_has(2);
  @$pb.TagNumber(3)
  void clearHarness() => $_clearField(3);
}

class ListItemsResponse extends $pb.GeneratedMessage {
  factory ListItemsResponse({
    $core.Iterable<Item>? items,
  }) {
    final result = ListItemsResponse._();
    if (items != null) result.items.addAll(items);
    return result;
  }

  ListItemsResponse._();

  factory ListItemsResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListItemsResponse()..mergeFromBuffer(data, registry);
  factory ListItemsResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListItemsResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ListItemsResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ListItemsResponse.$_createMessage)
    ..pPM<Item>(1, _omitFieldNames ? '' : 'items',
        subBuilder: Item.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListItemsResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListItemsResponse copyWith(void Function(ListItemsResponse) updates) =>
      super.copyWith((message) => updates(message as ListItemsResponse))
          as ListItemsResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use ListItemsResponse() / ListItemsResponse.new instead')
  static ListItemsResponse create() => ListItemsResponse._();
  static $pb.GeneratedMessage $_createMessage() => ListItemsResponse._();
  @$core.override
  ListItemsResponse createEmptyInstance() => ListItemsResponse._();
  @$core.pragma('dart2js:noInline')
  static ListItemsResponse getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<ListItemsResponse>(
          ListItemsResponse.$_createMessage);
  static ListItemsResponse? _defaultInstance;

  @$pb.TagNumber(1)
  $pb.PbList<Item> get items => $_getList(0);
}

class AddItemRequest extends $pb.GeneratedMessage {
  factory AddItemRequest({
    $core.String? project,
    $core.String? title,
    $core.String? issueUrl,
    $core.int? priority,
    $core.String? harness,
  }) {
    final result = AddItemRequest._();
    if (project != null) result.project = project;
    if (title != null) result.title = title;
    if (issueUrl != null) result.issueUrl = issueUrl;
    if (priority != null) result.priority = priority;
    if (harness != null) result.harness = harness;
    return result;
  }

  AddItemRequest._();

  factory AddItemRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      AddItemRequest()..mergeFromBuffer(data, registry);
  factory AddItemRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      AddItemRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'AddItemRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: AddItemRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'project')
    ..aOS(2, _omitFieldNames ? '' : 'title')
    ..aOS(3, _omitFieldNames ? '' : 'issueUrl')
    ..aI(4, _omitFieldNames ? '' : 'priority')
    ..aOS(5, _omitFieldNames ? '' : 'harness')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  AddItemRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  AddItemRequest copyWith(void Function(AddItemRequest) updates) =>
      super.copyWith((message) => updates(message as AddItemRequest))
          as AddItemRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use AddItemRequest() / AddItemRequest.new instead')
  static AddItemRequest create() => AddItemRequest._();
  static $pb.GeneratedMessage $_createMessage() => AddItemRequest._();
  @$core.override
  AddItemRequest createEmptyInstance() => AddItemRequest._();
  @$core.pragma('dart2js:noInline')
  static AddItemRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<AddItemRequest>(
          AddItemRequest.$_createMessage);
  static AddItemRequest? _defaultInstance;

  /// project is the heading to file the item under.
  @$pb.TagNumber(1)
  $core.String get project => $_getSZ(0);
  @$pb.TagNumber(1)
  set project($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasProject() => $_has(0);
  @$pb.TagNumber(1)
  void clearProject() => $_clearField(1);

  @$pb.TagNumber(2)
  $core.String get title => $_getSZ(1);
  @$pb.TagNumber(2)
  set title($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasTitle() => $_has(1);
  @$pb.TagNumber(2)
  void clearTitle() => $_clearField(2);

  @$pb.TagNumber(3)
  $core.String get issueUrl => $_getSZ(2);
  @$pb.TagNumber(3)
  set issueUrl($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasIssueUrl() => $_has(2);
  @$pb.TagNumber(3)
  void clearIssueUrl() => $_clearField(3);

  /// priority is 1 (highest) to 9; 0 means unset.
  @$pb.TagNumber(4)
  $core.int get priority => $_getIZ(3);
  @$pb.TagNumber(4)
  set priority($core.int value) => $_setSignedInt32(3, value);
  @$pb.TagNumber(4)
  $core.bool hasPriority() => $_has(3);
  @$pb.TagNumber(4)
  void clearPriority() => $_clearField(4);

  /// harness is a preference such as "codex"; empty or "any" means none.
  @$pb.TagNumber(5)
  $core.String get harness => $_getSZ(4);
  @$pb.TagNumber(5)
  set harness($core.String value) => $_setString(4, value);
  @$pb.TagNumber(5)
  $core.bool hasHarness() => $_has(4);
  @$pb.TagNumber(5)
  void clearHarness() => $_clearField(5);
}

class AddItemResponse extends $pb.GeneratedMessage {
  factory AddItemResponse({
    Item? item,
  }) {
    final result = AddItemResponse._();
    if (item != null) result.item = item;
    return result;
  }

  AddItemResponse._();

  factory AddItemResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      AddItemResponse()..mergeFromBuffer(data, registry);
  factory AddItemResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      AddItemResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'AddItemResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: AddItemResponse.$_createMessage)
    ..aOM<Item>(1, _omitFieldNames ? '' : 'item',
        subBuilder: Item.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  AddItemResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  AddItemResponse copyWith(void Function(AddItemResponse) updates) =>
      super.copyWith((message) => updates(message as AddItemResponse))
          as AddItemResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use AddItemResponse() / AddItemResponse.new instead')
  static AddItemResponse create() => AddItemResponse._();
  static $pb.GeneratedMessage $_createMessage() => AddItemResponse._();
  @$core.override
  AddItemResponse createEmptyInstance() => AddItemResponse._();
  @$core.pragma('dart2js:noInline')
  static AddItemResponse getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<AddItemResponse>(
          AddItemResponse.$_createMessage);
  static AddItemResponse? _defaultInstance;

  @$pb.TagNumber(1)
  Item get item => $_getN(0);
  @$pb.TagNumber(1)
  set item(Item value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasItem() => $_has(0);
  @$pb.TagNumber(1)
  void clearItem() => $_clearField(1);
  @$pb.TagNumber(1)
  Item ensureItem() => $_ensure(0);
}

/// UpdateItemRequest moves an item to state. Claiming requires claim.by and
/// fails with FAILED_PRECONDITION if another claimant holds the item.
class UpdateItemRequest extends $pb.GeneratedMessage {
  factory UpdateItemRequest({
    $core.String? id,
    ItemState? state,
    Claim? claim,
  }) {
    final result = UpdateItemRequest._();
    if (id != null) result.id = id;
    if (state != null) result.state = state;
    if (claim != null) result.claim = claim;
    return result;
  }

  UpdateItemRequest._();

  factory UpdateItemRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      UpdateItemRequest()..mergeFromBuffer(data, registry);
  factory UpdateItemRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      UpdateItemRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'UpdateItemRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: UpdateItemRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'id')
    ..aE<ItemState>(2, _omitFieldNames ? '' : 'state',
        enumValues: ItemState.values)
    ..aOM<Claim>(3, _omitFieldNames ? '' : 'claim',
        subBuilder: Claim.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  UpdateItemRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  UpdateItemRequest copyWith(void Function(UpdateItemRequest) updates) =>
      super.copyWith((message) => updates(message as UpdateItemRequest))
          as UpdateItemRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use UpdateItemRequest() / UpdateItemRequest.new instead')
  static UpdateItemRequest create() => UpdateItemRequest._();
  static $pb.GeneratedMessage $_createMessage() => UpdateItemRequest._();
  @$core.override
  UpdateItemRequest createEmptyInstance() => UpdateItemRequest._();
  @$core.pragma('dart2js:noInline')
  static UpdateItemRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<UpdateItemRequest>(
          UpdateItemRequest.$_createMessage);
  static UpdateItemRequest? _defaultInstance;

  /// id of the item to change.
  @$pb.TagNumber(1)
  $core.String get id => $_getSZ(0);
  @$pb.TagNumber(1)
  set id($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasId() => $_has(0);
  @$pb.TagNumber(1)
  void clearId() => $_clearField(1);

  /// state is the state to move the item to.
  @$pb.TagNumber(2)
  ItemState get state => $_getN(1);
  @$pb.TagNumber(2)
  set state(ItemState value) => $_setField(2, value);
  @$pb.TagNumber(2)
  $core.bool hasState() => $_has(1);
  @$pb.TagNumber(2)
  void clearState() => $_clearField(2);

  /// claim is required when state is CLAIMED.
  @$pb.TagNumber(3)
  Claim get claim => $_getN(2);
  @$pb.TagNumber(3)
  set claim(Claim value) => $_setField(3, value);
  @$pb.TagNumber(3)
  $core.bool hasClaim() => $_has(2);
  @$pb.TagNumber(3)
  void clearClaim() => $_clearField(3);
  @$pb.TagNumber(3)
  Claim ensureClaim() => $_ensure(2);
}

class UpdateItemResponse extends $pb.GeneratedMessage {
  factory UpdateItemResponse({
    Item? item,
  }) {
    final result = UpdateItemResponse._();
    if (item != null) result.item = item;
    return result;
  }

  UpdateItemResponse._();

  factory UpdateItemResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      UpdateItemResponse()..mergeFromBuffer(data, registry);
  factory UpdateItemResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      UpdateItemResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'UpdateItemResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: UpdateItemResponse.$_createMessage)
    ..aOM<Item>(1, _omitFieldNames ? '' : 'item',
        subBuilder: Item.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  UpdateItemResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  UpdateItemResponse copyWith(void Function(UpdateItemResponse) updates) =>
      super.copyWith((message) => updates(message as UpdateItemResponse))
          as UpdateItemResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use UpdateItemResponse() / UpdateItemResponse.new instead')
  static UpdateItemResponse create() => UpdateItemResponse._();
  static $pb.GeneratedMessage $_createMessage() => UpdateItemResponse._();
  @$core.override
  UpdateItemResponse createEmptyInstance() => UpdateItemResponse._();
  @$core.pragma('dart2js:noInline')
  static UpdateItemResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<UpdateItemResponse>(
          UpdateItemResponse.$_createMessage);
  static UpdateItemResponse? _defaultInstance;

  @$pb.TagNumber(1)
  Item get item => $_getN(0);
  @$pb.TagNumber(1)
  set item(Item value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasItem() => $_has(0);
  @$pb.TagNumber(1)
  void clearItem() => $_clearField(1);
  @$pb.TagNumber(1)
  Item ensureItem() => $_ensure(0);
}

/// BacklogService fronts the markdown file at $PROTECTORATE_HOME/backlog.md.
/// Mutations publish backlog.* events on EventService.
class BacklogServiceApi {
  final $pb.RpcClient _client;

  BacklogServiceApi(this._client);

  /// ListItems returns backlog items, optionally filtered by project, state
  /// and harness.
  $async.Future<ListItemsResponse> listItems(
          $pb.ClientContext? ctx, ListItemsRequest request) =>
      _client.invoke<ListItemsResponse>(
          ctx, 'BacklogService', 'ListItems', request, ListItemsResponse());

  /// AddItem appends a new TODO item to a project and returns it with its id.
  $async.Future<AddItemResponse> addItem(
          $pb.ClientContext? ctx, AddItemRequest request) =>
      _client.invoke<AddItemResponse>(
          ctx, 'BacklogService', 'AddItem', request, AddItemResponse());

  /// UpdateItem sets an item's state, claiming or releasing it.
  $async.Future<UpdateItemResponse> updateItem(
          $pb.ClientContext? ctx, UpdateItemRequest request) =>
      _client.invoke<UpdateItemResponse>(
          ctx, 'BacklogService', 'UpdateItem', request, UpdateItemResponse());
}

const $core.bool _omitFieldNames =
    $core.bool.fromEnvironment('protobuf.omit_field_names');
const $core.bool _omitMessageNames =
    $core.bool.fromEnvironment('protobuf.omit_message_names');
