// This is a generated file - do not edit.
//
// Generated from protectorate/v1/desk.proto.

// @dart = 3.3

// ignore_for_file: annotate_overrides, camel_case_types, comment_references
// ignore_for_file: constant_identifier_names
// ignore_for_file: curly_braces_in_flow_control_structures
// ignore_for_file: deprecated_member_use_from_same_package, library_prefixes
// ignore_for_file: non_constant_identifier_names, prefer_relative_imports

import 'dart:async' as $async;
import 'dart:core' as $core;

import 'package:protobuf/protobuf.dart' as $pb;
import 'package:protobuf/well_known_types/google/protobuf/duration.pb.dart'
    as $0;
import 'package:protobuf/well_known_types/google/protobuf/timestamp.pb.dart'
    as $1;

import 'desk.pbenum.dart';

export 'package:protobuf/protobuf.dart' show GeneratedMessageGenericExtensions;

export 'desk.pbenum.dart';

class DeskTarget extends $pb.GeneratedMessage {
  factory DeskTarget({
    $core.String? name,
    $core.String? leaseId,
    $core.bool? allowFamily,
  }) {
    final result = DeskTarget._();
    if (name != null) result.name = name;
    if (leaseId != null) result.leaseId = leaseId;
    if (allowFamily != null) result.allowFamily = allowFamily;
    return result;
  }

  DeskTarget._();

  factory DeskTarget.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      DeskTarget()..mergeFromBuffer(data, registry);
  factory DeskTarget.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      DeskTarget()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'DeskTarget',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: DeskTarget.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'name')
    ..aOS(2, _omitFieldNames ? '' : 'leaseId')
    ..aOB(3, _omitFieldNames ? '' : 'allowFamily')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  DeskTarget clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  DeskTarget copyWith(void Function(DeskTarget) updates) =>
      super.copyWith((message) => updates(message as DeskTarget)) as DeskTarget;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use DeskTarget() / DeskTarget.new instead')
  static DeskTarget create() => DeskTarget._();
  static $pb.GeneratedMessage $_createMessage() => DeskTarget._();
  @$core.override
  DeskTarget createEmptyInstance() => DeskTarget._();
  @$core.pragma('dart2js:noInline')
  static DeskTarget getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<DeskTarget>(DeskTarget.$_createMessage);
  static DeskTarget? _defaultInstance;

  /// name is the node name.
  @$pb.TagNumber(1)
  $core.String get name => $_getSZ(0);
  @$pb.TagNumber(1)
  set name($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasName() => $_has(0);
  @$pb.TagNumber(1)
  void clearName() => $_clearField(1);

  /// lease_id names a live lease on "desk:<name>". Optional for mutating
  /// commands: when empty the hub leases the desktop for the caller.
  @$pb.TagNumber(2)
  $core.String get leaseId => $_getSZ(1);
  @$pb.TagNumber(2)
  set leaseId($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasLeaseId() => $_has(1);
  @$pb.TagNumber(2)
  void clearLeaseId() => $_clearField(2);

  @$pb.TagNumber(3)
  $core.bool get allowFamily => $_getBF(2);
  @$pb.TagNumber(3)
  set allowFamily($core.bool value) => $_setBool(2, value);
  @$pb.TagNumber(3)
  $core.bool hasAllowFamily() => $_has(2);
  @$pb.TagNumber(3)
  void clearAllowFamily() => $_clearField(3);
}

class ScreenshotRequest extends $pb.GeneratedMessage {
  factory ScreenshotRequest({
    DeskTarget? target,
  }) {
    final result = ScreenshotRequest._();
    if (target != null) result.target = target;
    return result;
  }

  ScreenshotRequest._();

  factory ScreenshotRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ScreenshotRequest()..mergeFromBuffer(data, registry);
  factory ScreenshotRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ScreenshotRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ScreenshotRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ScreenshotRequest.$_createMessage)
    ..aOM<DeskTarget>(1, _omitFieldNames ? '' : 'target',
        subBuilder: DeskTarget.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ScreenshotRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ScreenshotRequest copyWith(void Function(ScreenshotRequest) updates) =>
      super.copyWith((message) => updates(message as ScreenshotRequest))
          as ScreenshotRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use ScreenshotRequest() / ScreenshotRequest.new instead')
  static ScreenshotRequest create() => ScreenshotRequest._();
  static $pb.GeneratedMessage $_createMessage() => ScreenshotRequest._();
  @$core.override
  ScreenshotRequest createEmptyInstance() => ScreenshotRequest._();
  @$core.pragma('dart2js:noInline')
  static ScreenshotRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<ScreenshotRequest>(
          ScreenshotRequest.$_createMessage);
  static ScreenshotRequest? _defaultInstance;

  @$pb.TagNumber(1)
  DeskTarget get target => $_getN(0);
  @$pb.TagNumber(1)
  set target(DeskTarget value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasTarget() => $_has(0);
  @$pb.TagNumber(1)
  void clearTarget() => $_clearField(1);
  @$pb.TagNumber(1)
  DeskTarget ensureTarget() => $_ensure(0);
}

class ScreenshotResponse extends $pb.GeneratedMessage {
  factory ScreenshotResponse({
    $core.List<$core.int>? png,
    $core.int? width,
    $core.int? height,
  }) {
    final result = ScreenshotResponse._();
    if (png != null) result.png = png;
    if (width != null) result.width = width;
    if (height != null) result.height = height;
    return result;
  }

  ScreenshotResponse._();

  factory ScreenshotResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ScreenshotResponse()..mergeFromBuffer(data, registry);
  factory ScreenshotResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ScreenshotResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ScreenshotResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ScreenshotResponse.$_createMessage)
    ..a<$core.List<$core.int>>(
        1, _omitFieldNames ? '' : 'png', $pb.PbFieldType.OY)
    ..aI(2, _omitFieldNames ? '' : 'width')
    ..aI(3, _omitFieldNames ? '' : 'height')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ScreenshotResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ScreenshotResponse copyWith(void Function(ScreenshotResponse) updates) =>
      super.copyWith((message) => updates(message as ScreenshotResponse))
          as ScreenshotResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use ScreenshotResponse() / ScreenshotResponse.new instead')
  static ScreenshotResponse create() => ScreenshotResponse._();
  static $pb.GeneratedMessage $_createMessage() => ScreenshotResponse._();
  @$core.override
  ScreenshotResponse createEmptyInstance() => ScreenshotResponse._();
  @$core.pragma('dart2js:noInline')
  static ScreenshotResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<ScreenshotResponse>(
          ScreenshotResponse.$_createMessage);
  static ScreenshotResponse? _defaultInstance;

  @$pb.TagNumber(1)
  $core.List<$core.int> get png => $_getN(0);
  @$pb.TagNumber(1)
  set png($core.List<$core.int> value) => $_setBytes(0, value);
  @$pb.TagNumber(1)
  $core.bool hasPng() => $_has(0);
  @$pb.TagNumber(1)
  void clearPng() => $_clearField(1);

  @$pb.TagNumber(2)
  $core.int get width => $_getIZ(1);
  @$pb.TagNumber(2)
  set width($core.int value) => $_setSignedInt32(1, value);
  @$pb.TagNumber(2)
  $core.bool hasWidth() => $_has(1);
  @$pb.TagNumber(2)
  void clearWidth() => $_clearField(2);

  @$pb.TagNumber(3)
  $core.int get height => $_getIZ(2);
  @$pb.TagNumber(3)
  set height($core.int value) => $_setSignedInt32(2, value);
  @$pb.TagNumber(3)
  $core.bool hasHeight() => $_has(2);
  @$pb.TagNumber(3)
  void clearHeight() => $_clearField(3);
}

class ListWindowsRequest extends $pb.GeneratedMessage {
  factory ListWindowsRequest({
    DeskTarget? target,
  }) {
    final result = ListWindowsRequest._();
    if (target != null) result.target = target;
    return result;
  }

  ListWindowsRequest._();

  factory ListWindowsRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListWindowsRequest()..mergeFromBuffer(data, registry);
  factory ListWindowsRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListWindowsRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ListWindowsRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ListWindowsRequest.$_createMessage)
    ..aOM<DeskTarget>(1, _omitFieldNames ? '' : 'target',
        subBuilder: DeskTarget.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListWindowsRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListWindowsRequest copyWith(void Function(ListWindowsRequest) updates) =>
      super.copyWith((message) => updates(message as ListWindowsRequest))
          as ListWindowsRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use ListWindowsRequest() / ListWindowsRequest.new instead')
  static ListWindowsRequest create() => ListWindowsRequest._();
  static $pb.GeneratedMessage $_createMessage() => ListWindowsRequest._();
  @$core.override
  ListWindowsRequest createEmptyInstance() => ListWindowsRequest._();
  @$core.pragma('dart2js:noInline')
  static ListWindowsRequest getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<ListWindowsRequest>(
          ListWindowsRequest.$_createMessage);
  static ListWindowsRequest? _defaultInstance;

  @$pb.TagNumber(1)
  DeskTarget get target => $_getN(0);
  @$pb.TagNumber(1)
  set target(DeskTarget value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasTarget() => $_has(0);
  @$pb.TagNumber(1)
  void clearTarget() => $_clearField(1);
  @$pb.TagNumber(1)
  DeskTarget ensureTarget() => $_ensure(0);
}

class DeskWindow extends $pb.GeneratedMessage {
  factory DeskWindow({
    $core.String? title,
    $core.int? x,
    $core.int? y,
    $core.int? width,
    $core.int? height,
    $core.int? pid,
    $core.bool? minimized,
  }) {
    final result = DeskWindow._();
    if (title != null) result.title = title;
    if (x != null) result.x = x;
    if (y != null) result.y = y;
    if (width != null) result.width = width;
    if (height != null) result.height = height;
    if (pid != null) result.pid = pid;
    if (minimized != null) result.minimized = minimized;
    return result;
  }

  DeskWindow._();

  factory DeskWindow.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      DeskWindow()..mergeFromBuffer(data, registry);
  factory DeskWindow.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      DeskWindow()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'DeskWindow',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: DeskWindow.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'title')
    ..aI(2, _omitFieldNames ? '' : 'x')
    ..aI(3, _omitFieldNames ? '' : 'y')
    ..aI(4, _omitFieldNames ? '' : 'width')
    ..aI(5, _omitFieldNames ? '' : 'height')
    ..aI(6, _omitFieldNames ? '' : 'pid', fieldType: $pb.PbFieldType.OU3)
    ..aOB(7, _omitFieldNames ? '' : 'minimized')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  DeskWindow clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  DeskWindow copyWith(void Function(DeskWindow) updates) =>
      super.copyWith((message) => updates(message as DeskWindow)) as DeskWindow;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use DeskWindow() / DeskWindow.new instead')
  static DeskWindow create() => DeskWindow._();
  static $pb.GeneratedMessage $_createMessage() => DeskWindow._();
  @$core.override
  DeskWindow createEmptyInstance() => DeskWindow._();
  @$core.pragma('dart2js:noInline')
  static DeskWindow getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<DeskWindow>(DeskWindow.$_createMessage);
  static DeskWindow? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get title => $_getSZ(0);
  @$pb.TagNumber(1)
  set title($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasTitle() => $_has(0);
  @$pb.TagNumber(1)
  void clearTitle() => $_clearField(1);

  @$pb.TagNumber(2)
  $core.int get x => $_getIZ(1);
  @$pb.TagNumber(2)
  set x($core.int value) => $_setSignedInt32(1, value);
  @$pb.TagNumber(2)
  $core.bool hasX() => $_has(1);
  @$pb.TagNumber(2)
  void clearX() => $_clearField(2);

  @$pb.TagNumber(3)
  $core.int get y => $_getIZ(2);
  @$pb.TagNumber(3)
  set y($core.int value) => $_setSignedInt32(2, value);
  @$pb.TagNumber(3)
  $core.bool hasY() => $_has(2);
  @$pb.TagNumber(3)
  void clearY() => $_clearField(3);

  @$pb.TagNumber(4)
  $core.int get width => $_getIZ(3);
  @$pb.TagNumber(4)
  set width($core.int value) => $_setSignedInt32(3, value);
  @$pb.TagNumber(4)
  $core.bool hasWidth() => $_has(3);
  @$pb.TagNumber(4)
  void clearWidth() => $_clearField(4);

  @$pb.TagNumber(5)
  $core.int get height => $_getIZ(4);
  @$pb.TagNumber(5)
  set height($core.int value) => $_setSignedInt32(4, value);
  @$pb.TagNumber(5)
  $core.bool hasHeight() => $_has(4);
  @$pb.TagNumber(5)
  void clearHeight() => $_clearField(5);

  @$pb.TagNumber(6)
  $core.int get pid => $_getIZ(5);
  @$pb.TagNumber(6)
  set pid($core.int value) => $_setUnsignedInt32(5, value);
  @$pb.TagNumber(6)
  $core.bool hasPid() => $_has(5);
  @$pb.TagNumber(6)
  void clearPid() => $_clearField(6);

  @$pb.TagNumber(7)
  $core.bool get minimized => $_getBF(6);
  @$pb.TagNumber(7)
  set minimized($core.bool value) => $_setBool(6, value);
  @$pb.TagNumber(7)
  $core.bool hasMinimized() => $_has(6);
  @$pb.TagNumber(7)
  void clearMinimized() => $_clearField(7);
}

class ListWindowsResponse extends $pb.GeneratedMessage {
  factory ListWindowsResponse({
    $core.Iterable<DeskWindow>? windows,
  }) {
    final result = ListWindowsResponse._();
    if (windows != null) result.windows.addAll(windows);
    return result;
  }

  ListWindowsResponse._();

  factory ListWindowsResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListWindowsResponse()..mergeFromBuffer(data, registry);
  factory ListWindowsResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListWindowsResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ListWindowsResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ListWindowsResponse.$_createMessage)
    ..pPM<DeskWindow>(1, _omitFieldNames ? '' : 'windows',
        subBuilder: DeskWindow.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListWindowsResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListWindowsResponse copyWith(void Function(ListWindowsResponse) updates) =>
      super.copyWith((message) => updates(message as ListWindowsResponse))
          as ListWindowsResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core
      .Deprecated('Use ListWindowsResponse() / ListWindowsResponse.new instead')
  static ListWindowsResponse create() => ListWindowsResponse._();
  static $pb.GeneratedMessage $_createMessage() => ListWindowsResponse._();
  @$core.override
  ListWindowsResponse createEmptyInstance() => ListWindowsResponse._();
  @$core.pragma('dart2js:noInline')
  static ListWindowsResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<ListWindowsResponse>(
          ListWindowsResponse.$_createMessage);
  static ListWindowsResponse? _defaultInstance;

  @$pb.TagNumber(1)
  $pb.PbList<DeskWindow> get windows => $_getList(0);
}

class ForegroundRequest extends $pb.GeneratedMessage {
  factory ForegroundRequest({
    DeskTarget? target,
  }) {
    final result = ForegroundRequest._();
    if (target != null) result.target = target;
    return result;
  }

  ForegroundRequest._();

  factory ForegroundRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ForegroundRequest()..mergeFromBuffer(data, registry);
  factory ForegroundRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ForegroundRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ForegroundRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ForegroundRequest.$_createMessage)
    ..aOM<DeskTarget>(1, _omitFieldNames ? '' : 'target',
        subBuilder: DeskTarget.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ForegroundRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ForegroundRequest copyWith(void Function(ForegroundRequest) updates) =>
      super.copyWith((message) => updates(message as ForegroundRequest))
          as ForegroundRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use ForegroundRequest() / ForegroundRequest.new instead')
  static ForegroundRequest create() => ForegroundRequest._();
  static $pb.GeneratedMessage $_createMessage() => ForegroundRequest._();
  @$core.override
  ForegroundRequest createEmptyInstance() => ForegroundRequest._();
  @$core.pragma('dart2js:noInline')
  static ForegroundRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<ForegroundRequest>(
          ForegroundRequest.$_createMessage);
  static ForegroundRequest? _defaultInstance;

  @$pb.TagNumber(1)
  DeskTarget get target => $_getN(0);
  @$pb.TagNumber(1)
  set target(DeskTarget value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasTarget() => $_has(0);
  @$pb.TagNumber(1)
  void clearTarget() => $_clearField(1);
  @$pb.TagNumber(1)
  DeskTarget ensureTarget() => $_ensure(0);
}

class ForegroundResponse extends $pb.GeneratedMessage {
  factory ForegroundResponse({
    $core.String? title,
    $core.int? pid,
    $core.String? process,
    $core.String? integrity,
    $core.String? agentIntegrity,
    $core.bool? blocks,
  }) {
    final result = ForegroundResponse._();
    if (title != null) result.title = title;
    if (pid != null) result.pid = pid;
    if (process != null) result.process = process;
    if (integrity != null) result.integrity = integrity;
    if (agentIntegrity != null) result.agentIntegrity = agentIntegrity;
    if (blocks != null) result.blocks = blocks;
    return result;
  }

  ForegroundResponse._();

  factory ForegroundResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ForegroundResponse()..mergeFromBuffer(data, registry);
  factory ForegroundResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ForegroundResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ForegroundResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ForegroundResponse.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'title')
    ..aI(2, _omitFieldNames ? '' : 'pid', fieldType: $pb.PbFieldType.OU3)
    ..aOS(3, _omitFieldNames ? '' : 'process')
    ..aOS(4, _omitFieldNames ? '' : 'integrity')
    ..aOS(5, _omitFieldNames ? '' : 'agentIntegrity')
    ..aOB(6, _omitFieldNames ? '' : 'blocks')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ForegroundResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ForegroundResponse copyWith(void Function(ForegroundResponse) updates) =>
      super.copyWith((message) => updates(message as ForegroundResponse))
          as ForegroundResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use ForegroundResponse() / ForegroundResponse.new instead')
  static ForegroundResponse create() => ForegroundResponse._();
  static $pb.GeneratedMessage $_createMessage() => ForegroundResponse._();
  @$core.override
  ForegroundResponse createEmptyInstance() => ForegroundResponse._();
  @$core.pragma('dart2js:noInline')
  static ForegroundResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<ForegroundResponse>(
          ForegroundResponse.$_createMessage);
  static ForegroundResponse? _defaultInstance;

  /// title is empty when no window has the foreground.
  @$pb.TagNumber(1)
  $core.String get title => $_getSZ(0);
  @$pb.TagNumber(1)
  set title($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasTitle() => $_has(0);
  @$pb.TagNumber(1)
  void clearTitle() => $_clearField(1);

  @$pb.TagNumber(2)
  $core.int get pid => $_getIZ(1);
  @$pb.TagNumber(2)
  set pid($core.int value) => $_setUnsignedInt32(1, value);
  @$pb.TagNumber(2)
  $core.bool hasPid() => $_has(1);
  @$pb.TagNumber(2)
  void clearPid() => $_clearField(2);

  /// process is the executable's base name, empty when it cannot be read.
  @$pb.TagNumber(3)
  $core.String get process => $_getSZ(2);
  @$pb.TagNumber(3)
  set process($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasProcess() => $_has(2);
  @$pb.TagNumber(3)
  void clearProcess() => $_clearField(3);

  /// integrity is "untrusted", "low", "medium", "high", "system" or
  /// "protected", or "unreadable" when the process token cannot be opened
  /// (which only a higher-integrity process does to this agent).
  @$pb.TagNumber(4)
  $core.String get integrity => $_getSZ(3);
  @$pb.TagNumber(4)
  set integrity($core.String value) => $_setString(3, value);
  @$pb.TagNumber(4)
  $core.bool hasIntegrity() => $_has(3);
  @$pb.TagNumber(4)
  void clearIntegrity() => $_clearField(4);

  /// agent_integrity is the integrity level of the node agent itself.
  @$pb.TagNumber(5)
  $core.String get agentIntegrity => $_getSZ(4);
  @$pb.TagNumber(5)
  set agentIntegrity($core.String value) => $_setString(4, value);
  @$pb.TagNumber(5)
  $core.bool hasAgentIntegrity() => $_has(4);
  @$pb.TagNumber(5)
  void clearAgentIntegrity() => $_clearField(5);

  /// blocks is true when injected input would not reach this window.
  @$pb.TagNumber(6)
  $core.bool get blocks => $_getBF(5);
  @$pb.TagNumber(6)
  set blocks($core.bool value) => $_setBool(5, value);
  @$pb.TagNumber(6)
  $core.bool hasBlocks() => $_has(5);
  @$pb.TagNumber(6)
  void clearBlocks() => $_clearField(6);
}

class ClickRequest extends $pb.GeneratedMessage {
  factory ClickRequest({
    DeskTarget? target,
    $core.int? x,
    $core.int? y,
    $core.String? button,
    $core.bool? double_5,
  }) {
    final result = ClickRequest._();
    if (target != null) result.target = target;
    if (x != null) result.x = x;
    if (y != null) result.y = y;
    if (button != null) result.button = button;
    if (double_5 != null) result.double_5 = double_5;
    return result;
  }

  ClickRequest._();

  factory ClickRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ClickRequest()..mergeFromBuffer(data, registry);
  factory ClickRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ClickRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ClickRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ClickRequest.$_createMessage)
    ..aOM<DeskTarget>(1, _omitFieldNames ? '' : 'target',
        subBuilder: DeskTarget.$_createMessage)
    ..aI(2, _omitFieldNames ? '' : 'x')
    ..aI(3, _omitFieldNames ? '' : 'y')
    ..aOS(4, _omitFieldNames ? '' : 'button')
    ..aOB(5, _omitFieldNames ? '' : 'double')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ClickRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ClickRequest copyWith(void Function(ClickRequest) updates) =>
      super.copyWith((message) => updates(message as ClickRequest))
          as ClickRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use ClickRequest() / ClickRequest.new instead')
  static ClickRequest create() => ClickRequest._();
  static $pb.GeneratedMessage $_createMessage() => ClickRequest._();
  @$core.override
  ClickRequest createEmptyInstance() => ClickRequest._();
  @$core.pragma('dart2js:noInline')
  static ClickRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<ClickRequest>(
          ClickRequest.$_createMessage);
  static ClickRequest? _defaultInstance;

  @$pb.TagNumber(1)
  DeskTarget get target => $_getN(0);
  @$pb.TagNumber(1)
  set target(DeskTarget value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasTarget() => $_has(0);
  @$pb.TagNumber(1)
  void clearTarget() => $_clearField(1);
  @$pb.TagNumber(1)
  DeskTarget ensureTarget() => $_ensure(0);

  @$pb.TagNumber(2)
  $core.int get x => $_getIZ(1);
  @$pb.TagNumber(2)
  set x($core.int value) => $_setSignedInt32(1, value);
  @$pb.TagNumber(2)
  $core.bool hasX() => $_has(1);
  @$pb.TagNumber(2)
  void clearX() => $_clearField(2);

  @$pb.TagNumber(3)
  $core.int get y => $_getIZ(2);
  @$pb.TagNumber(3)
  set y($core.int value) => $_setSignedInt32(2, value);
  @$pb.TagNumber(3)
  $core.bool hasY() => $_has(2);
  @$pb.TagNumber(3)
  void clearY() => $_clearField(3);

  /// button is "left" (default), "right" or "middle".
  @$pb.TagNumber(4)
  $core.String get button => $_getSZ(3);
  @$pb.TagNumber(4)
  set button($core.String value) => $_setString(3, value);
  @$pb.TagNumber(4)
  $core.bool hasButton() => $_has(3);
  @$pb.TagNumber(4)
  void clearButton() => $_clearField(4);

  @$pb.TagNumber(5)
  $core.bool get double_5 => $_getBF(4);
  @$pb.TagNumber(5)
  set double_5($core.bool value) => $_setBool(4, value);
  @$pb.TagNumber(5)
  $core.bool hasDouble_5() => $_has(4);
  @$pb.TagNumber(5)
  void clearDouble_5() => $_clearField(5);
}

class MoveRequest extends $pb.GeneratedMessage {
  factory MoveRequest({
    DeskTarget? target,
    $core.int? x,
    $core.int? y,
  }) {
    final result = MoveRequest._();
    if (target != null) result.target = target;
    if (x != null) result.x = x;
    if (y != null) result.y = y;
    return result;
  }

  MoveRequest._();

  factory MoveRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      MoveRequest()..mergeFromBuffer(data, registry);
  factory MoveRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      MoveRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'MoveRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: MoveRequest.$_createMessage)
    ..aOM<DeskTarget>(1, _omitFieldNames ? '' : 'target',
        subBuilder: DeskTarget.$_createMessage)
    ..aI(2, _omitFieldNames ? '' : 'x')
    ..aI(3, _omitFieldNames ? '' : 'y')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  MoveRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  MoveRequest copyWith(void Function(MoveRequest) updates) =>
      super.copyWith((message) => updates(message as MoveRequest))
          as MoveRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use MoveRequest() / MoveRequest.new instead')
  static MoveRequest create() => MoveRequest._();
  static $pb.GeneratedMessage $_createMessage() => MoveRequest._();
  @$core.override
  MoveRequest createEmptyInstance() => MoveRequest._();
  @$core.pragma('dart2js:noInline')
  static MoveRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<MoveRequest>(
          MoveRequest.$_createMessage);
  static MoveRequest? _defaultInstance;

  @$pb.TagNumber(1)
  DeskTarget get target => $_getN(0);
  @$pb.TagNumber(1)
  set target(DeskTarget value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasTarget() => $_has(0);
  @$pb.TagNumber(1)
  void clearTarget() => $_clearField(1);
  @$pb.TagNumber(1)
  DeskTarget ensureTarget() => $_ensure(0);

  @$pb.TagNumber(2)
  $core.int get x => $_getIZ(1);
  @$pb.TagNumber(2)
  set x($core.int value) => $_setSignedInt32(1, value);
  @$pb.TagNumber(2)
  $core.bool hasX() => $_has(1);
  @$pb.TagNumber(2)
  void clearX() => $_clearField(2);

  @$pb.TagNumber(3)
  $core.int get y => $_getIZ(2);
  @$pb.TagNumber(3)
  set y($core.int value) => $_setSignedInt32(2, value);
  @$pb.TagNumber(3)
  $core.bool hasY() => $_has(2);
  @$pb.TagNumber(3)
  void clearY() => $_clearField(3);
}

class TypeRequest extends $pb.GeneratedMessage {
  factory TypeRequest({
    DeskTarget? target,
    $core.String? text,
  }) {
    final result = TypeRequest._();
    if (target != null) result.target = target;
    if (text != null) result.text = text;
    return result;
  }

  TypeRequest._();

  factory TypeRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      TypeRequest()..mergeFromBuffer(data, registry);
  factory TypeRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      TypeRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'TypeRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: TypeRequest.$_createMessage)
    ..aOM<DeskTarget>(1, _omitFieldNames ? '' : 'target',
        subBuilder: DeskTarget.$_createMessage)
    ..aOS(2, _omitFieldNames ? '' : 'text')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  TypeRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  TypeRequest copyWith(void Function(TypeRequest) updates) =>
      super.copyWith((message) => updates(message as TypeRequest))
          as TypeRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use TypeRequest() / TypeRequest.new instead')
  static TypeRequest create() => TypeRequest._();
  static $pb.GeneratedMessage $_createMessage() => TypeRequest._();
  @$core.override
  TypeRequest createEmptyInstance() => TypeRequest._();
  @$core.pragma('dart2js:noInline')
  static TypeRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<TypeRequest>(
          TypeRequest.$_createMessage);
  static TypeRequest? _defaultInstance;

  @$pb.TagNumber(1)
  DeskTarget get target => $_getN(0);
  @$pb.TagNumber(1)
  set target(DeskTarget value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasTarget() => $_has(0);
  @$pb.TagNumber(1)
  void clearTarget() => $_clearField(1);
  @$pb.TagNumber(1)
  DeskTarget ensureTarget() => $_ensure(0);

  @$pb.TagNumber(2)
  $core.String get text => $_getSZ(1);
  @$pb.TagNumber(2)
  set text($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasText() => $_has(1);
  @$pb.TagNumber(2)
  void clearText() => $_clearField(2);
}

class KeyRequest extends $pb.GeneratedMessage {
  factory KeyRequest({
    DeskTarget? target,
    $core.String? chord,
  }) {
    final result = KeyRequest._();
    if (target != null) result.target = target;
    if (chord != null) result.chord = chord;
    return result;
  }

  KeyRequest._();

  factory KeyRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      KeyRequest()..mergeFromBuffer(data, registry);
  factory KeyRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      KeyRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'KeyRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: KeyRequest.$_createMessage)
    ..aOM<DeskTarget>(1, _omitFieldNames ? '' : 'target',
        subBuilder: DeskTarget.$_createMessage)
    ..aOS(2, _omitFieldNames ? '' : 'chord')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  KeyRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  KeyRequest copyWith(void Function(KeyRequest) updates) =>
      super.copyWith((message) => updates(message as KeyRequest)) as KeyRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use KeyRequest() / KeyRequest.new instead')
  static KeyRequest create() => KeyRequest._();
  static $pb.GeneratedMessage $_createMessage() => KeyRequest._();
  @$core.override
  KeyRequest createEmptyInstance() => KeyRequest._();
  @$core.pragma('dart2js:noInline')
  static KeyRequest getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<KeyRequest>(KeyRequest.$_createMessage);
  static KeyRequest? _defaultInstance;

  @$pb.TagNumber(1)
  DeskTarget get target => $_getN(0);
  @$pb.TagNumber(1)
  set target(DeskTarget value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasTarget() => $_has(0);
  @$pb.TagNumber(1)
  void clearTarget() => $_clearField(1);
  @$pb.TagNumber(1)
  DeskTarget ensureTarget() => $_ensure(0);

  @$pb.TagNumber(2)
  $core.String get chord => $_getSZ(1);
  @$pb.TagNumber(2)
  set chord($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasChord() => $_has(1);
  @$pb.TagNumber(2)
  void clearChord() => $_clearField(2);
}

class RunRequest extends $pb.GeneratedMessage {
  factory RunRequest({
    DeskTarget? target,
    $core.String? program,
    $core.Iterable<$core.String>? args,
  }) {
    final result = RunRequest._();
    if (target != null) result.target = target;
    if (program != null) result.program = program;
    if (args != null) result.args.addAll(args);
    return result;
  }

  RunRequest._();

  factory RunRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      RunRequest()..mergeFromBuffer(data, registry);
  factory RunRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      RunRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'RunRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: RunRequest.$_createMessage)
    ..aOM<DeskTarget>(1, _omitFieldNames ? '' : 'target',
        subBuilder: DeskTarget.$_createMessage)
    ..aOS(2, _omitFieldNames ? '' : 'program')
    ..pPS(3, _omitFieldNames ? '' : 'args')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  RunRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  RunRequest copyWith(void Function(RunRequest) updates) =>
      super.copyWith((message) => updates(message as RunRequest)) as RunRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use RunRequest() / RunRequest.new instead')
  static RunRequest create() => RunRequest._();
  static $pb.GeneratedMessage $_createMessage() => RunRequest._();
  @$core.override
  RunRequest createEmptyInstance() => RunRequest._();
  @$core.pragma('dart2js:noInline')
  static RunRequest getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<RunRequest>(RunRequest.$_createMessage);
  static RunRequest? _defaultInstance;

  @$pb.TagNumber(1)
  DeskTarget get target => $_getN(0);
  @$pb.TagNumber(1)
  set target(DeskTarget value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasTarget() => $_has(0);
  @$pb.TagNumber(1)
  void clearTarget() => $_clearField(1);
  @$pb.TagNumber(1)
  DeskTarget ensureTarget() => $_ensure(0);

  @$pb.TagNumber(2)
  $core.String get program => $_getSZ(1);
  @$pb.TagNumber(2)
  set program($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasProgram() => $_has(1);
  @$pb.TagNumber(2)
  void clearProgram() => $_clearField(2);

  @$pb.TagNumber(3)
  $pb.PbList<$core.String> get args => $_getList(2);
}

class InputResponse extends $pb.GeneratedMessage {
  factory InputResponse() => InputResponse._();

  InputResponse._();

  factory InputResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      InputResponse()..mergeFromBuffer(data, registry);
  factory InputResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      InputResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'InputResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: InputResponse.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  InputResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  InputResponse copyWith(void Function(InputResponse) updates) =>
      super.copyWith((message) => updates(message as InputResponse))
          as InputResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use InputResponse() / InputResponse.new instead')
  static InputResponse create() => InputResponse._();
  static $pb.GeneratedMessage $_createMessage() => InputResponse._();
  @$core.override
  InputResponse createEmptyInstance() => InputResponse._();
  @$core.pragma('dart2js:noInline')
  static InputResponse getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<InputResponse>(
          InputResponse.$_createMessage);
  static InputResponse? _defaultInstance;
}

class RunResponse extends $pb.GeneratedMessage {
  factory RunResponse({
    $core.int? pid,
  }) {
    final result = RunResponse._();
    if (pid != null) result.pid = pid;
    return result;
  }

  RunResponse._();

  factory RunResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      RunResponse()..mergeFromBuffer(data, registry);
  factory RunResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      RunResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'RunResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: RunResponse.$_createMessage)
    ..aI(1, _omitFieldNames ? '' : 'pid', fieldType: $pb.PbFieldType.OU3)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  RunResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  RunResponse copyWith(void Function(RunResponse) updates) =>
      super.copyWith((message) => updates(message as RunResponse))
          as RunResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use RunResponse() / RunResponse.new instead')
  static RunResponse create() => RunResponse._();
  static $pb.GeneratedMessage $_createMessage() => RunResponse._();
  @$core.override
  RunResponse createEmptyInstance() => RunResponse._();
  @$core.pragma('dart2js:noInline')
  static RunResponse getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<RunResponse>(
          RunResponse.$_createMessage);
  static RunResponse? _defaultInstance;

  @$pb.TagNumber(1)
  $core.int get pid => $_getIZ(0);
  @$pb.TagNumber(1)
  set pid($core.int value) => $_setUnsignedInt32(0, value);
  @$pb.TagNumber(1)
  $core.bool hasPid() => $_has(0);
  @$pb.TagNumber(1)
  void clearPid() => $_clearField(1);
}

enum DeskActionRequest_Action {
  screenshot,
  listWindows,
  click,
  move,
  type,
  key,
  run,
  foreground,
  notSet
}

/// DeskActionRequest is the one call the hub makes to a node agent; the hub
/// has already checked the lease.
class DeskActionRequest extends $pb.GeneratedMessage {
  factory DeskActionRequest({
    ScreenshotRequest? screenshot,
    ListWindowsRequest? listWindows,
    ClickRequest? click,
    MoveRequest? move,
    TypeRequest? type,
    KeyRequest? key,
    RunRequest? run,
    ForegroundRequest? foreground,
  }) {
    final result = DeskActionRequest._();
    if (screenshot != null) result.screenshot = screenshot;
    if (listWindows != null) result.listWindows = listWindows;
    if (click != null) result.click = click;
    if (move != null) result.move = move;
    if (type != null) result.type = type;
    if (key != null) result.key = key;
    if (run != null) result.run = run;
    if (foreground != null) result.foreground = foreground;
    return result;
  }

  DeskActionRequest._();

  factory DeskActionRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      DeskActionRequest()..mergeFromBuffer(data, registry);
  factory DeskActionRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      DeskActionRequest()..mergeFromJson(json, registry);

  static const $core.Map<$core.int, DeskActionRequest_Action>
      _DeskActionRequest_ActionByTag = {
    1: DeskActionRequest_Action.screenshot,
    2: DeskActionRequest_Action.listWindows,
    3: DeskActionRequest_Action.click,
    4: DeskActionRequest_Action.move,
    5: DeskActionRequest_Action.type,
    6: DeskActionRequest_Action.key,
    7: DeskActionRequest_Action.run,
    8: DeskActionRequest_Action.foreground,
    0: DeskActionRequest_Action.notSet
  };
  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'DeskActionRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: DeskActionRequest.$_createMessage)
    ..oo(0, [1, 2, 3, 4, 5, 6, 7, 8])
    ..aOM<ScreenshotRequest>(1, _omitFieldNames ? '' : 'screenshot',
        subBuilder: ScreenshotRequest.$_createMessage)
    ..aOM<ListWindowsRequest>(2, _omitFieldNames ? '' : 'listWindows',
        subBuilder: ListWindowsRequest.$_createMessage)
    ..aOM<ClickRequest>(3, _omitFieldNames ? '' : 'click',
        subBuilder: ClickRequest.$_createMessage)
    ..aOM<MoveRequest>(4, _omitFieldNames ? '' : 'move',
        subBuilder: MoveRequest.$_createMessage)
    ..aOM<TypeRequest>(5, _omitFieldNames ? '' : 'type',
        subBuilder: TypeRequest.$_createMessage)
    ..aOM<KeyRequest>(6, _omitFieldNames ? '' : 'key',
        subBuilder: KeyRequest.$_createMessage)
    ..aOM<RunRequest>(7, _omitFieldNames ? '' : 'run',
        subBuilder: RunRequest.$_createMessage)
    ..aOM<ForegroundRequest>(8, _omitFieldNames ? '' : 'foreground',
        subBuilder: ForegroundRequest.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  DeskActionRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  DeskActionRequest copyWith(void Function(DeskActionRequest) updates) =>
      super.copyWith((message) => updates(message as DeskActionRequest))
          as DeskActionRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use DeskActionRequest() / DeskActionRequest.new instead')
  static DeskActionRequest create() => DeskActionRequest._();
  static $pb.GeneratedMessage $_createMessage() => DeskActionRequest._();
  @$core.override
  DeskActionRequest createEmptyInstance() => DeskActionRequest._();
  @$core.pragma('dart2js:noInline')
  static DeskActionRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<DeskActionRequest>(
          DeskActionRequest.$_createMessage);
  static DeskActionRequest? _defaultInstance;

  @$pb.TagNumber(1)
  @$pb.TagNumber(2)
  @$pb.TagNumber(3)
  @$pb.TagNumber(4)
  @$pb.TagNumber(5)
  @$pb.TagNumber(6)
  @$pb.TagNumber(7)
  @$pb.TagNumber(8)
  DeskActionRequest_Action whichAction() =>
      _DeskActionRequest_ActionByTag[$_whichOneof(0)]!;
  @$pb.TagNumber(1)
  @$pb.TagNumber(2)
  @$pb.TagNumber(3)
  @$pb.TagNumber(4)
  @$pb.TagNumber(5)
  @$pb.TagNumber(6)
  @$pb.TagNumber(7)
  @$pb.TagNumber(8)
  void clearAction() => $_clearField($_whichOneof(0));

  @$pb.TagNumber(1)
  ScreenshotRequest get screenshot => $_getN(0);
  @$pb.TagNumber(1)
  set screenshot(ScreenshotRequest value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasScreenshot() => $_has(0);
  @$pb.TagNumber(1)
  void clearScreenshot() => $_clearField(1);
  @$pb.TagNumber(1)
  ScreenshotRequest ensureScreenshot() => $_ensure(0);

  @$pb.TagNumber(2)
  ListWindowsRequest get listWindows => $_getN(1);
  @$pb.TagNumber(2)
  set listWindows(ListWindowsRequest value) => $_setField(2, value);
  @$pb.TagNumber(2)
  $core.bool hasListWindows() => $_has(1);
  @$pb.TagNumber(2)
  void clearListWindows() => $_clearField(2);
  @$pb.TagNumber(2)
  ListWindowsRequest ensureListWindows() => $_ensure(1);

  @$pb.TagNumber(3)
  ClickRequest get click => $_getN(2);
  @$pb.TagNumber(3)
  set click(ClickRequest value) => $_setField(3, value);
  @$pb.TagNumber(3)
  $core.bool hasClick() => $_has(2);
  @$pb.TagNumber(3)
  void clearClick() => $_clearField(3);
  @$pb.TagNumber(3)
  ClickRequest ensureClick() => $_ensure(2);

  @$pb.TagNumber(4)
  MoveRequest get move => $_getN(3);
  @$pb.TagNumber(4)
  set move(MoveRequest value) => $_setField(4, value);
  @$pb.TagNumber(4)
  $core.bool hasMove() => $_has(3);
  @$pb.TagNumber(4)
  void clearMove() => $_clearField(4);
  @$pb.TagNumber(4)
  MoveRequest ensureMove() => $_ensure(3);

  @$pb.TagNumber(5)
  TypeRequest get type => $_getN(4);
  @$pb.TagNumber(5)
  set type(TypeRequest value) => $_setField(5, value);
  @$pb.TagNumber(5)
  $core.bool hasType() => $_has(4);
  @$pb.TagNumber(5)
  void clearType() => $_clearField(5);
  @$pb.TagNumber(5)
  TypeRequest ensureType() => $_ensure(4);

  @$pb.TagNumber(6)
  KeyRequest get key => $_getN(5);
  @$pb.TagNumber(6)
  set key(KeyRequest value) => $_setField(6, value);
  @$pb.TagNumber(6)
  $core.bool hasKey() => $_has(5);
  @$pb.TagNumber(6)
  void clearKey() => $_clearField(6);
  @$pb.TagNumber(6)
  KeyRequest ensureKey() => $_ensure(5);

  @$pb.TagNumber(7)
  RunRequest get run => $_getN(6);
  @$pb.TagNumber(7)
  set run(RunRequest value) => $_setField(7, value);
  @$pb.TagNumber(7)
  $core.bool hasRun() => $_has(6);
  @$pb.TagNumber(7)
  void clearRun() => $_clearField(7);
  @$pb.TagNumber(7)
  RunRequest ensureRun() => $_ensure(6);

  @$pb.TagNumber(8)
  ForegroundRequest get foreground => $_getN(7);
  @$pb.TagNumber(8)
  set foreground(ForegroundRequest value) => $_setField(8, value);
  @$pb.TagNumber(8)
  $core.bool hasForeground() => $_has(7);
  @$pb.TagNumber(8)
  void clearForeground() => $_clearField(8);
  @$pb.TagNumber(8)
  ForegroundRequest ensureForeground() => $_ensure(7);
}

enum DeskActionResponse_Result {
  screenshot,
  listWindows,
  input,
  run,
  foreground,
  notSet
}

class DeskActionResponse extends $pb.GeneratedMessage {
  factory DeskActionResponse({
    ScreenshotResponse? screenshot,
    ListWindowsResponse? listWindows,
    InputResponse? input,
    RunResponse? run,
    ForegroundResponse? foreground,
  }) {
    final result = DeskActionResponse._();
    if (screenshot != null) result.screenshot = screenshot;
    if (listWindows != null) result.listWindows = listWindows;
    if (input != null) result.input = input;
    if (run != null) result.run = run;
    if (foreground != null) result.foreground = foreground;
    return result;
  }

  DeskActionResponse._();

  factory DeskActionResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      DeskActionResponse()..mergeFromBuffer(data, registry);
  factory DeskActionResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      DeskActionResponse()..mergeFromJson(json, registry);

  static const $core.Map<$core.int, DeskActionResponse_Result>
      _DeskActionResponse_ResultByTag = {
    1: DeskActionResponse_Result.screenshot,
    2: DeskActionResponse_Result.listWindows,
    3: DeskActionResponse_Result.input,
    4: DeskActionResponse_Result.run,
    5: DeskActionResponse_Result.foreground,
    0: DeskActionResponse_Result.notSet
  };
  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'DeskActionResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: DeskActionResponse.$_createMessage)
    ..oo(0, [1, 2, 3, 4, 5])
    ..aOM<ScreenshotResponse>(1, _omitFieldNames ? '' : 'screenshot',
        subBuilder: ScreenshotResponse.$_createMessage)
    ..aOM<ListWindowsResponse>(2, _omitFieldNames ? '' : 'listWindows',
        subBuilder: ListWindowsResponse.$_createMessage)
    ..aOM<InputResponse>(3, _omitFieldNames ? '' : 'input',
        subBuilder: InputResponse.$_createMessage)
    ..aOM<RunResponse>(4, _omitFieldNames ? '' : 'run',
        subBuilder: RunResponse.$_createMessage)
    ..aOM<ForegroundResponse>(5, _omitFieldNames ? '' : 'foreground',
        subBuilder: ForegroundResponse.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  DeskActionResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  DeskActionResponse copyWith(void Function(DeskActionResponse) updates) =>
      super.copyWith((message) => updates(message as DeskActionResponse))
          as DeskActionResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use DeskActionResponse() / DeskActionResponse.new instead')
  static DeskActionResponse create() => DeskActionResponse._();
  static $pb.GeneratedMessage $_createMessage() => DeskActionResponse._();
  @$core.override
  DeskActionResponse createEmptyInstance() => DeskActionResponse._();
  @$core.pragma('dart2js:noInline')
  static DeskActionResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<DeskActionResponse>(
          DeskActionResponse.$_createMessage);
  static DeskActionResponse? _defaultInstance;

  @$pb.TagNumber(1)
  @$pb.TagNumber(2)
  @$pb.TagNumber(3)
  @$pb.TagNumber(4)
  @$pb.TagNumber(5)
  DeskActionResponse_Result whichResult() =>
      _DeskActionResponse_ResultByTag[$_whichOneof(0)]!;
  @$pb.TagNumber(1)
  @$pb.TagNumber(2)
  @$pb.TagNumber(3)
  @$pb.TagNumber(4)
  @$pb.TagNumber(5)
  void clearResult() => $_clearField($_whichOneof(0));

  @$pb.TagNumber(1)
  ScreenshotResponse get screenshot => $_getN(0);
  @$pb.TagNumber(1)
  set screenshot(ScreenshotResponse value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasScreenshot() => $_has(0);
  @$pb.TagNumber(1)
  void clearScreenshot() => $_clearField(1);
  @$pb.TagNumber(1)
  ScreenshotResponse ensureScreenshot() => $_ensure(0);

  @$pb.TagNumber(2)
  ListWindowsResponse get listWindows => $_getN(1);
  @$pb.TagNumber(2)
  set listWindows(ListWindowsResponse value) => $_setField(2, value);
  @$pb.TagNumber(2)
  $core.bool hasListWindows() => $_has(1);
  @$pb.TagNumber(2)
  void clearListWindows() => $_clearField(2);
  @$pb.TagNumber(2)
  ListWindowsResponse ensureListWindows() => $_ensure(1);

  @$pb.TagNumber(3)
  InputResponse get input => $_getN(2);
  @$pb.TagNumber(3)
  set input(InputResponse value) => $_setField(3, value);
  @$pb.TagNumber(3)
  $core.bool hasInput() => $_has(2);
  @$pb.TagNumber(3)
  void clearInput() => $_clearField(3);
  @$pb.TagNumber(3)
  InputResponse ensureInput() => $_ensure(2);

  @$pb.TagNumber(4)
  RunResponse get run => $_getN(3);
  @$pb.TagNumber(4)
  set run(RunResponse value) => $_setField(4, value);
  @$pb.TagNumber(4)
  $core.bool hasRun() => $_has(3);
  @$pb.TagNumber(4)
  void clearRun() => $_clearField(4);
  @$pb.TagNumber(4)
  RunResponse ensureRun() => $_ensure(3);

  @$pb.TagNumber(5)
  ForegroundResponse get foreground => $_getN(4);
  @$pb.TagNumber(5)
  set foreground(ForegroundResponse value) => $_setField(5, value);
  @$pb.TagNumber(5)
  $core.bool hasForeground() => $_has(4);
  @$pb.TagNumber(5)
  void clearForeground() => $_clearField(5);
  @$pb.TagNumber(5)
  ForegroundResponse ensureForeground() => $_ensure(4);
}

/// DeskUse counts the desktop input commands (click, move, type, key, run) one
/// caller issued on a node today, in the hub's local time. The hub keeps it in
/// memory, so it also resets when the hub restarts. It only makes use visible;
/// nothing is limited.
class DeskUse extends $pb.GeneratedMessage {
  factory DeskUse({
    $core.String? caller,
    $core.int? count,
  }) {
    final result = DeskUse._();
    if (caller != null) result.caller = caller;
    if (count != null) result.count = count;
    return result;
  }

  DeskUse._();

  factory DeskUse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      DeskUse()..mergeFromBuffer(data, registry);
  factory DeskUse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      DeskUse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'DeskUse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: DeskUse.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'caller')
    ..aI(2, _omitFieldNames ? '' : 'count')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  DeskUse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  DeskUse copyWith(void Function(DeskUse) updates) =>
      super.copyWith((message) => updates(message as DeskUse)) as DeskUse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use DeskUse() / DeskUse.new instead')
  static DeskUse create() => DeskUse._();
  static $pb.GeneratedMessage $_createMessage() => DeskUse._();
  @$core.override
  DeskUse createEmptyInstance() => DeskUse._();
  @$core.pragma('dart2js:noInline')
  static DeskUse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<DeskUse>(DeskUse.$_createMessage);
  static DeskUse? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get caller => $_getSZ(0);
  @$pb.TagNumber(1)
  set caller($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasCaller() => $_has(0);
  @$pb.TagNumber(1)
  void clearCaller() => $_clearField(1);

  @$pb.TagNumber(2)
  $core.int get count => $_getIZ(1);
  @$pb.TagNumber(2)
  set count($core.int value) => $_setSignedInt32(1, value);
  @$pb.TagNumber(2)
  $core.bool hasCount() => $_has(1);
  @$pb.TagNumber(2)
  void clearCount() => $_clearField(2);
}

/// DeskPolicy says when a node's desktop may be driven by agents. Without a
/// policy the desktop is always available. With one, it is available only
/// inside the window and after the user has had no physical input (injected
/// input, such as the node's own desk commands, does not count) for idle.
class DeskPolicy extends $pb.GeneratedMessage {
  factory DeskPolicy({
    $core.String? window,
    $0.Duration? idle,
    $core.String? timezone,
    $core.bool? lockAfter,
  }) {
    final result = DeskPolicy._();
    if (window != null) result.window = window;
    if (idle != null) result.idle = idle;
    if (timezone != null) result.timezone = timezone;
    if (lockAfter != null) result.lockAfter = lockAfter;
    return result;
  }

  DeskPolicy._();

  factory DeskPolicy.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      DeskPolicy()..mergeFromBuffer(data, registry);
  factory DeskPolicy.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      DeskPolicy()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'DeskPolicy',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: DeskPolicy.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'window')
    ..aOM<$0.Duration>(2, _omitFieldNames ? '' : 'idle',
        subBuilder: $0.Duration.$_createMessage)
    ..aOS(3, _omitFieldNames ? '' : 'timezone')
    ..aOB(4, _omitFieldNames ? '' : 'lockAfter')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  DeskPolicy clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  DeskPolicy copyWith(void Function(DeskPolicy) updates) =>
      super.copyWith((message) => updates(message as DeskPolicy)) as DeskPolicy;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use DeskPolicy() / DeskPolicy.new instead')
  static DeskPolicy create() => DeskPolicy._();
  static $pb.GeneratedMessage $_createMessage() => DeskPolicy._();
  @$core.override
  DeskPolicy createEmptyInstance() => DeskPolicy._();
  @$core.pragma('dart2js:noInline')
  static DeskPolicy getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<DeskPolicy>(DeskPolicy.$_createMessage);
  static DeskPolicy? _defaultInstance;

  /// window is "HH:MM-HH:MM" in the policy's time zone; start after end spans
  /// midnight ("19:00-07:00").
  @$pb.TagNumber(1)
  $core.String get window => $_getSZ(0);
  @$pb.TagNumber(1)
  set window($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasWindow() => $_has(0);
  @$pb.TagNumber(1)
  void clearWindow() => $_clearField(1);

  @$pb.TagNumber(2)
  $0.Duration get idle => $_getN(1);
  @$pb.TagNumber(2)
  set idle($0.Duration value) => $_setField(2, value);
  @$pb.TagNumber(2)
  $core.bool hasIdle() => $_has(1);
  @$pb.TagNumber(2)
  void clearIdle() => $_clearField(2);
  @$pb.TagNumber(2)
  $0.Duration ensureIdle() => $_ensure(1);

  /// timezone is an IANA name; empty means the node's local time.
  @$pb.TagNumber(3)
  $core.String get timezone => $_getSZ(2);
  @$pb.TagNumber(3)
  set timezone($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasTimezone() => $_has(2);
  @$pb.TagNumber(3)
  void clearTimezone() => $_clearField(3);

  /// lock_after locks the workstation when the window ends and when an agent
  /// lease ends inside the window with no other lease following within a
  /// minute. It never locks while the user is active.
  @$pb.TagNumber(4)
  $core.bool get lockAfter => $_getBF(3);
  @$pb.TagNumber(4)
  set lockAfter($core.bool value) => $_setBool(3, value);
  @$pb.TagNumber(4)
  $core.bool hasLockAfter() => $_has(3);
  @$pb.TagNumber(4)
  void clearLockAfter() => $_clearField(4);
}

/// DeskState is a node's evaluation of its policy at one moment.
class DeskState extends $pb.GeneratedMessage {
  factory DeskState({
    $core.bool? available,
    DeskReason? reason,
    $core.String? detail,
    $1.Timestamp? lastInput,
    $1.Timestamp? evaluatedAt,
    DeskPolicy? policy,
  }) {
    final result = DeskState._();
    if (available != null) result.available = available;
    if (reason != null) result.reason = reason;
    if (detail != null) result.detail = detail;
    if (lastInput != null) result.lastInput = lastInput;
    if (evaluatedAt != null) result.evaluatedAt = evaluatedAt;
    if (policy != null) result.policy = policy;
    return result;
  }

  DeskState._();

  factory DeskState.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      DeskState()..mergeFromBuffer(data, registry);
  factory DeskState.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      DeskState()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'DeskState',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: DeskState.$_createMessage)
    ..aOB(1, _omitFieldNames ? '' : 'available')
    ..aE<DeskReason>(2, _omitFieldNames ? '' : 'reason',
        enumValues: DeskReason.values)
    ..aOS(3, _omitFieldNames ? '' : 'detail')
    ..aOM<$1.Timestamp>(4, _omitFieldNames ? '' : 'lastInput',
        subBuilder: $1.Timestamp.$_createMessage)
    ..aOM<$1.Timestamp>(5, _omitFieldNames ? '' : 'evaluatedAt',
        subBuilder: $1.Timestamp.$_createMessage)
    ..aOM<DeskPolicy>(6, _omitFieldNames ? '' : 'policy',
        subBuilder: DeskPolicy.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  DeskState clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  DeskState copyWith(void Function(DeskState) updates) =>
      super.copyWith((message) => updates(message as DeskState)) as DeskState;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use DeskState() / DeskState.new instead')
  static DeskState create() => DeskState._();
  static $pb.GeneratedMessage $_createMessage() => DeskState._();
  @$core.override
  DeskState createEmptyInstance() => DeskState._();
  @$core.pragma('dart2js:noInline')
  static DeskState getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<DeskState>(DeskState.$_createMessage);
  static DeskState? _defaultInstance;

  @$pb.TagNumber(1)
  $core.bool get available => $_getBF(0);
  @$pb.TagNumber(1)
  set available($core.bool value) => $_setBool(0, value);
  @$pb.TagNumber(1)
  $core.bool hasAvailable() => $_has(0);
  @$pb.TagNumber(1)
  void clearAvailable() => $_clearField(1);

  @$pb.TagNumber(2)
  DeskReason get reason => $_getN(1);
  @$pb.TagNumber(2)
  set reason(DeskReason value) => $_setField(2, value);
  @$pb.TagNumber(2)
  $core.bool hasReason() => $_has(1);
  @$pb.TagNumber(2)
  void clearReason() => $_clearField(2);

  /// detail is a sentence for people, e.g. "physical input 3m ago, idle 15m
  /// required".
  @$pb.TagNumber(3)
  $core.String get detail => $_getSZ(2);
  @$pb.TagNumber(3)
  set detail($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasDetail() => $_has(2);
  @$pb.TagNumber(3)
  void clearDetail() => $_clearField(3);

  /// last_input is the last physical keyboard or mouse input the node saw; unset
  /// when it has seen none since the agent started.
  @$pb.TagNumber(4)
  $1.Timestamp get lastInput => $_getN(3);
  @$pb.TagNumber(4)
  set lastInput($1.Timestamp value) => $_setField(4, value);
  @$pb.TagNumber(4)
  $core.bool hasLastInput() => $_has(3);
  @$pb.TagNumber(4)
  void clearLastInput() => $_clearField(4);
  @$pb.TagNumber(4)
  $1.Timestamp ensureLastInput() => $_ensure(3);

  @$pb.TagNumber(5)
  $1.Timestamp get evaluatedAt => $_getN(4);
  @$pb.TagNumber(5)
  set evaluatedAt($1.Timestamp value) => $_setField(5, value);
  @$pb.TagNumber(5)
  $core.bool hasEvaluatedAt() => $_has(4);
  @$pb.TagNumber(5)
  void clearEvaluatedAt() => $_clearField(5);
  @$pb.TagNumber(5)
  $1.Timestamp ensureEvaluatedAt() => $_ensure(4);

  /// policy is the policy the node evaluated; unset when none applies.
  @$pb.TagNumber(6)
  DeskPolicy get policy => $_getN(5);
  @$pb.TagNumber(6)
  set policy(DeskPolicy value) => $_setField(6, value);
  @$pb.TagNumber(6)
  $core.bool hasPolicy() => $_has(5);
  @$pb.TagNumber(6)
  void clearPolicy() => $_clearField(6);
  @$pb.TagNumber(6)
  DeskPolicy ensurePolicy() => $_ensure(5);
}

/// DeskService drives the interactive desktop of a node, for programs that
/// only have a GUI. It works only when the node agent runs in the logged-in
/// user's session (`protectorate node install`); otherwise calls fail
/// with FAILED_PRECONDITION. Only Windows nodes implement it.
///
/// The desktop is a lease resource named "desk:<node>". Every command that
/// changes the desktop (click, move, type, key, run) runs under a live lease on
/// it; screenshot and windows need none. A command that names no lease makes
/// the hub take a 2 minute lease for the caller when the desktop is free (the
/// id and expiry come back in the Protectorate-Desk-Lease and
/// Protectorate-Desk-Lease-Expires response headers), renew the caller's own,
/// or refuse when another holder has it. Coordinates are pixels of the
/// screenshot, whose origin is the top-left of the virtual screen.
///
/// The desktop is the last resort. Prefer, in this order: the program's own API
/// or batch/CLI mode; `protectorate job run` or ssh on the node; `protectorate
/// node put/get` for files; only then screen and input, which are slow, flaky
/// and can land in the wrong window.
class DeskServiceApi {
  final $pb.RpcClient _client;

  DeskServiceApi(this._client);

  /// Screenshot captures the whole virtual screen as a PNG.
  $async.Future<ScreenshotResponse> screenshot(
          $pb.ClientContext? ctx, ScreenshotRequest request) =>
      _client.invoke<ScreenshotResponse>(
          ctx, 'DeskService', 'Screenshot', request, ScreenshotResponse());

  /// ListWindows lists visible top-level windows, topmost first.
  $async.Future<ListWindowsResponse> listWindows(
          $pb.ClientContext? ctx, ListWindowsRequest request) =>
      _client.invoke<ListWindowsResponse>(
          ctx, 'DeskService', 'ListWindows', request, ListWindowsResponse());

  /// Foreground reports the window that receives keystrokes now, with its
  /// process and integrity level. Read-only. Windows drops input injected into
  /// a window of a higher integrity level than the agent's without any error,
  /// so Click, Move, Type and Key refuse with FAILED_PRECONDITION when the
  /// foreground window (or, for the mouse, the window under the pointer) is
  /// higher; this shows what is in the way.
  $async.Future<ForegroundResponse> foreground(
          $pb.ClientContext? ctx, ForegroundRequest request) =>
      _client.invoke<ForegroundResponse>(
          ctx, 'DeskService', 'Foreground', request, ForegroundResponse());

  /// Click moves the pointer and clicks.
  /// Last resort: prefer the shell (job run, ssh), node put/get, or the
  /// program's API. Needs a desk lease.
  $async.Future<InputResponse> click(
          $pb.ClientContext? ctx, ClickRequest request) =>
      _client.invoke<InputResponse>(
          ctx, 'DeskService', 'Click', request, InputResponse());

  /// Move puts the pointer at a position.
  /// Last resort: prefer the shell (job run, ssh), node put/get, or the
  /// program's API. Needs a desk lease.
  $async.Future<InputResponse> move(
          $pb.ClientContext? ctx, MoveRequest request) =>
      _client.invoke<InputResponse>(
          ctx, 'DeskService', 'Move', request, InputResponse());

  /// Type enters text as key presses into the focused window. Last resort:
  /// prefer the shell (job run, ssh), node put/get, or the program's API. Needs
  /// a desk lease.
  $async.Future<InputResponse> type(
          $pb.ClientContext? ctx, TypeRequest request) =>
      _client.invoke<InputResponse>(
          ctx, 'DeskService', 'Type', request, InputResponse());

  /// Key presses a chord such as "ctrl+s", "alt+f4" or "enter". Last resort:
  /// prefer the shell (job run, ssh), node put/get, or the program's API. Needs
  /// a desk lease.
  $async.Future<InputResponse> key(
          $pb.ClientContext? ctx, KeyRequest request) =>
      _client.invoke<InputResponse>(
          ctx, 'DeskService', 'Key', request, InputResponse());

  /// Run starts a program in the interactive session and returns at once. Last
  /// resort: prefer the shell (job run, ssh), node put/get, or the program's
  /// API. Needs a desk lease.
  $async.Future<RunResponse> run($pb.ClientContext? ctx, RunRequest request) =>
      _client.invoke<RunResponse>(
          ctx, 'DeskService', 'Run', request, RunResponse());
}

const $core.bool _omitFieldNames =
    $core.bool.fromEnvironment('protobuf.omit_field_names');
const $core.bool _omitMessageNames =
    $core.bool.fromEnvironment('protobuf.omit_message_names');
