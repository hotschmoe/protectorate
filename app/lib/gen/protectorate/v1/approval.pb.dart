// This is a generated file - do not edit.
//
// Generated from protectorate/v1/approval.proto.

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

import 'approval.pbenum.dart';

export 'package:protobuf/protobuf.dart' show GeneratedMessageGenericExtensions;

export 'approval.pbenum.dart';

class Approval extends $pb.GeneratedMessage {
  factory Approval({
    $core.String? id,
    $core.String? actor,
    $core.String? procedure,
    $core.String? subject,
    $core.String? reason,
    $core.String? detail,
    $core.String? runId,
    ApprovalState? state,
    $0.Timestamp? createdAt,
    $0.Timestamp? expiresAt,
    $core.String? decidedBy,
  }) {
    final result = Approval._();
    if (id != null) result.id = id;
    if (actor != null) result.actor = actor;
    if (procedure != null) result.procedure = procedure;
    if (subject != null) result.subject = subject;
    if (reason != null) result.reason = reason;
    if (detail != null) result.detail = detail;
    if (runId != null) result.runId = runId;
    if (state != null) result.state = state;
    if (createdAt != null) result.createdAt = createdAt;
    if (expiresAt != null) result.expiresAt = expiresAt;
    if (decidedBy != null) result.decidedBy = decidedBy;
    return result;
  }

  Approval._();

  factory Approval.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      Approval()..mergeFromBuffer(data, registry);
  factory Approval.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      Approval()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'Approval',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: Approval.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'id')
    ..aOS(2, _omitFieldNames ? '' : 'actor')
    ..aOS(3, _omitFieldNames ? '' : 'procedure')
    ..aOS(4, _omitFieldNames ? '' : 'subject')
    ..aOS(5, _omitFieldNames ? '' : 'reason')
    ..aOS(6, _omitFieldNames ? '' : 'detail')
    ..aOS(7, _omitFieldNames ? '' : 'runId')
    ..aE<ApprovalState>(8, _omitFieldNames ? '' : 'state',
        enumValues: ApprovalState.values)
    ..aOM<$0.Timestamp>(9, _omitFieldNames ? '' : 'createdAt',
        subBuilder: $0.Timestamp.$_createMessage)
    ..aOM<$0.Timestamp>(10, _omitFieldNames ? '' : 'expiresAt',
        subBuilder: $0.Timestamp.$_createMessage)
    ..aOS(11, _omitFieldNames ? '' : 'decidedBy')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  Approval clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  Approval copyWith(void Function(Approval) updates) =>
      super.copyWith((message) => updates(message as Approval)) as Approval;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use Approval() / Approval.new instead')
  static Approval create() => Approval._();
  static $pb.GeneratedMessage $_createMessage() => Approval._();
  @$core.override
  Approval createEmptyInstance() => Approval._();
  @$core.pragma('dart2js:noInline')
  static Approval getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<Approval>(Approval.$_createMessage);
  static Approval? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get id => $_getSZ(0);
  @$pb.TagNumber(1)
  set id($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasId() => $_has(0);
  @$pb.TagNumber(1)
  void clearId() => $_clearField(1);

  /// actor is the principal that made the call, as in the audit log.
  @$pb.TagNumber(2)
  $core.String get actor => $_getSZ(1);
  @$pb.TagNumber(2)
  set actor($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasActor() => $_has(1);
  @$pb.TagNumber(2)
  void clearActor() => $_clearField(2);

  @$pb.TagNumber(3)
  $core.String get procedure => $_getSZ(2);
  @$pb.TagNumber(3)
  set procedure($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasProcedure() => $_has(2);
  @$pb.TagNumber(3)
  void clearProcedure() => $_clearField(3);

  @$pb.TagNumber(4)
  $core.String get subject => $_getSZ(3);
  @$pb.TagNumber(4)
  set subject($core.String value) => $_setString(3, value);
  @$pb.TagNumber(4)
  $core.bool hasSubject() => $_has(3);
  @$pb.TagNumber(4)
  void clearSubject() => $_clearField(4);

  /// reason says why the call is gated, such as "destructive".
  @$pb.TagNumber(5)
  $core.String get reason => $_getSZ(4);
  @$pb.TagNumber(5)
  set reason($core.String value) => $_setString(4, value);
  @$pb.TagNumber(5)
  $core.bool hasReason() => $_has(4);
  @$pb.TagNumber(5)
  void clearReason() => $_clearField(5);

  /// detail adds what the owner needs to decide, such as changed hashes.
  @$pb.TagNumber(6)
  $core.String get detail => $_getSZ(5);
  @$pb.TagNumber(6)
  set detail($core.String value) => $_setString(5, value);
  @$pb.TagNumber(6)
  $core.bool hasDetail() => $_has(5);
  @$pb.TagNumber(6)
  void clearDetail() => $_clearField(6);

  @$pb.TagNumber(7)
  $core.String get runId => $_getSZ(6);
  @$pb.TagNumber(7)
  set runId($core.String value) => $_setString(6, value);
  @$pb.TagNumber(7)
  $core.bool hasRunId() => $_has(6);
  @$pb.TagNumber(7)
  void clearRunId() => $_clearField(7);

  @$pb.TagNumber(8)
  ApprovalState get state => $_getN(7);
  @$pb.TagNumber(8)
  set state(ApprovalState value) => $_setField(8, value);
  @$pb.TagNumber(8)
  $core.bool hasState() => $_has(7);
  @$pb.TagNumber(8)
  void clearState() => $_clearField(8);

  @$pb.TagNumber(9)
  $0.Timestamp get createdAt => $_getN(8);
  @$pb.TagNumber(9)
  set createdAt($0.Timestamp value) => $_setField(9, value);
  @$pb.TagNumber(9)
  $core.bool hasCreatedAt() => $_has(8);
  @$pb.TagNumber(9)
  void clearCreatedAt() => $_clearField(9);
  @$pb.TagNumber(9)
  $0.Timestamp ensureCreatedAt() => $_ensure(8);

  @$pb.TagNumber(10)
  $0.Timestamp get expiresAt => $_getN(9);
  @$pb.TagNumber(10)
  set expiresAt($0.Timestamp value) => $_setField(10, value);
  @$pb.TagNumber(10)
  $core.bool hasExpiresAt() => $_has(9);
  @$pb.TagNumber(10)
  void clearExpiresAt() => $_clearField(10);
  @$pb.TagNumber(10)
  $0.Timestamp ensureExpiresAt() => $_ensure(9);

  @$pb.TagNumber(11)
  $core.String get decidedBy => $_getSZ(10);
  @$pb.TagNumber(11)
  set decidedBy($core.String value) => $_setString(10, value);
  @$pb.TagNumber(11)
  $core.bool hasDecidedBy() => $_has(10);
  @$pb.TagNumber(11)
  void clearDecidedBy() => $_clearField(11);
}

class ListApprovalsRequest extends $pb.GeneratedMessage {
  factory ListApprovalsRequest({
    $core.bool? all,
  }) {
    final result = ListApprovalsRequest._();
    if (all != null) result.all = all;
    return result;
  }

  ListApprovalsRequest._();

  factory ListApprovalsRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListApprovalsRequest()..mergeFromBuffer(data, registry);
  factory ListApprovalsRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListApprovalsRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ListApprovalsRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ListApprovalsRequest.$_createMessage)
    ..aOB(1, _omitFieldNames ? '' : 'all')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListApprovalsRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListApprovalsRequest copyWith(void Function(ListApprovalsRequest) updates) =>
      super.copyWith((message) => updates(message as ListApprovalsRequest))
          as ListApprovalsRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use ListApprovalsRequest() / ListApprovalsRequest.new instead')
  static ListApprovalsRequest create() => ListApprovalsRequest._();
  static $pb.GeneratedMessage $_createMessage() => ListApprovalsRequest._();
  @$core.override
  ListApprovalsRequest createEmptyInstance() => ListApprovalsRequest._();
  @$core.pragma('dart2js:noInline')
  static ListApprovalsRequest getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<ListApprovalsRequest>(
          ListApprovalsRequest.$_createMessage);
  static ListApprovalsRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.bool get all => $_getBF(0);
  @$pb.TagNumber(1)
  set all($core.bool value) => $_setBool(0, value);
  @$pb.TagNumber(1)
  $core.bool hasAll() => $_has(0);
  @$pb.TagNumber(1)
  void clearAll() => $_clearField(1);
}

class ListApprovalsResponse extends $pb.GeneratedMessage {
  factory ListApprovalsResponse({
    $core.Iterable<Approval>? approvals,
  }) {
    final result = ListApprovalsResponse._();
    if (approvals != null) result.approvals.addAll(approvals);
    return result;
  }

  ListApprovalsResponse._();

  factory ListApprovalsResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListApprovalsResponse()..mergeFromBuffer(data, registry);
  factory ListApprovalsResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListApprovalsResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ListApprovalsResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ListApprovalsResponse.$_createMessage)
    ..pPM<Approval>(1, _omitFieldNames ? '' : 'approvals',
        subBuilder: Approval.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListApprovalsResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListApprovalsResponse copyWith(
          void Function(ListApprovalsResponse) updates) =>
      super.copyWith((message) => updates(message as ListApprovalsResponse))
          as ListApprovalsResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use ListApprovalsResponse() / ListApprovalsResponse.new instead')
  static ListApprovalsResponse create() => ListApprovalsResponse._();
  static $pb.GeneratedMessage $_createMessage() => ListApprovalsResponse._();
  @$core.override
  ListApprovalsResponse createEmptyInstance() => ListApprovalsResponse._();
  @$core.pragma('dart2js:noInline')
  static ListApprovalsResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<ListApprovalsResponse>(
          ListApprovalsResponse.$_createMessage);
  static ListApprovalsResponse? _defaultInstance;

  @$pb.TagNumber(1)
  $pb.PbList<Approval> get approvals => $_getList(0);
}

class DecideApprovalRequest extends $pb.GeneratedMessage {
  factory DecideApprovalRequest({
    $core.String? id,
    $core.bool? approve,
  }) {
    final result = DecideApprovalRequest._();
    if (id != null) result.id = id;
    if (approve != null) result.approve = approve;
    return result;
  }

  DecideApprovalRequest._();

  factory DecideApprovalRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      DecideApprovalRequest()..mergeFromBuffer(data, registry);
  factory DecideApprovalRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      DecideApprovalRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'DecideApprovalRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: DecideApprovalRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'id')
    ..aOB(2, _omitFieldNames ? '' : 'approve')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  DecideApprovalRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  DecideApprovalRequest copyWith(
          void Function(DecideApprovalRequest) updates) =>
      super.copyWith((message) => updates(message as DecideApprovalRequest))
          as DecideApprovalRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use DecideApprovalRequest() / DecideApprovalRequest.new instead')
  static DecideApprovalRequest create() => DecideApprovalRequest._();
  static $pb.GeneratedMessage $_createMessage() => DecideApprovalRequest._();
  @$core.override
  DecideApprovalRequest createEmptyInstance() => DecideApprovalRequest._();
  @$core.pragma('dart2js:noInline')
  static DecideApprovalRequest getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<DecideApprovalRequest>(
          DecideApprovalRequest.$_createMessage);
  static DecideApprovalRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get id => $_getSZ(0);
  @$pb.TagNumber(1)
  set id($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasId() => $_has(0);
  @$pb.TagNumber(1)
  void clearId() => $_clearField(1);

  @$pb.TagNumber(2)
  $core.bool get approve => $_getBF(1);
  @$pb.TagNumber(2)
  set approve($core.bool value) => $_setBool(1, value);
  @$pb.TagNumber(2)
  $core.bool hasApprove() => $_has(1);
  @$pb.TagNumber(2)
  void clearApprove() => $_clearField(2);
}

class DecideApprovalResponse extends $pb.GeneratedMessage {
  factory DecideApprovalResponse({
    Approval? approval,
  }) {
    final result = DecideApprovalResponse._();
    if (approval != null) result.approval = approval;
    return result;
  }

  DecideApprovalResponse._();

  factory DecideApprovalResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      DecideApprovalResponse()..mergeFromBuffer(data, registry);
  factory DecideApprovalResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      DecideApprovalResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'DecideApprovalResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: DecideApprovalResponse.$_createMessage)
    ..aOM<Approval>(1, _omitFieldNames ? '' : 'approval',
        subBuilder: Approval.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  DecideApprovalResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  DecideApprovalResponse copyWith(
          void Function(DecideApprovalResponse) updates) =>
      super.copyWith((message) => updates(message as DecideApprovalResponse))
          as DecideApprovalResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use DecideApprovalResponse() / DecideApprovalResponse.new instead')
  static DecideApprovalResponse create() => DecideApprovalResponse._();
  static $pb.GeneratedMessage $_createMessage() => DecideApprovalResponse._();
  @$core.override
  DecideApprovalResponse createEmptyInstance() => DecideApprovalResponse._();
  @$core.pragma('dart2js:noInline')
  static DecideApprovalResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<DecideApprovalResponse>(
          DecideApprovalResponse.$_createMessage);
  static DecideApprovalResponse? _defaultInstance;

  @$pb.TagNumber(1)
  Approval get approval => $_getN(0);
  @$pb.TagNumber(1)
  set approval(Approval value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasApproval() => $_has(0);
  @$pb.TagNumber(1)
  void clearApproval() => $_clearField(1);
  @$pb.TagNumber(1)
  Approval ensureApproval() => $_ensure(0);
}

/// ApprovalService lists and decides the requests the hub opens instead of
/// running a high-impact call made by Envoy, a sleeve or a grant Envoy
/// minted: an RPC marked destructive, and launching a sleeve whose harness
/// executable or profile changed since its last launch.
/// Calls by the owner and devices are never gated. The caller gets
/// an aborted error carrying the request id; repeating the same call with the
/// protectorate-approval header set to that id runs it once approved, and
/// fails once denied or expired. The protectorate CLI and MCP bridge repeat
/// it themselves. Requests expire after ten minutes and live only in hub
/// memory. Changes are announced as "approval.requested" and
/// "approval.decided" (subject = request id). Only the owner and devices
/// may call this service.
class ApprovalServiceApi {
  final $pb.RpcClient _client;

  ApprovalServiceApi(this._client);

  /// ListApprovals returns pending requests, oldest first, or with all every
  /// request the hub still remembers.
  $async.Future<ListApprovalsResponse> listApprovals(
          $pb.ClientContext? ctx, ListApprovalsRequest request) =>
      _client.invoke<ListApprovalsResponse>(ctx, 'ApprovalService',
          'ListApprovals', request, ListApprovalsResponse());

  /// DecideApproval approves or denies a pending request. Each decision is
  /// audited.
  $async.Future<DecideApprovalResponse> decideApproval(
          $pb.ClientContext? ctx, DecideApprovalRequest request) =>
      _client.invoke<DecideApprovalResponse>(ctx, 'ApprovalService',
          'DecideApproval', request, DecideApprovalResponse());
}

const $core.bool _omitFieldNames =
    $core.bool.fromEnvironment('protobuf.omit_field_names');
const $core.bool _omitMessageNames =
    $core.bool.fromEnvironment('protobuf.omit_message_names');
