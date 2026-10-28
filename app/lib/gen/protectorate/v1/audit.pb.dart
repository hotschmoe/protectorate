// This is a generated file - do not edit.
//
// Generated from protectorate/v1/audit.proto.

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

class ListAuditEntriesRequest extends $pb.GeneratedMessage {
  factory ListAuditEntriesRequest({
    $core.int? limit,
    $core.String? pageToken,
    $core.String? actor,
    $core.String? procedure,
    $core.String? subject,
    $core.String? outcome,
    $core.String? runId,
  }) {
    final result = ListAuditEntriesRequest._();
    if (limit != null) result.limit = limit;
    if (pageToken != null) result.pageToken = pageToken;
    if (actor != null) result.actor = actor;
    if (procedure != null) result.procedure = procedure;
    if (subject != null) result.subject = subject;
    if (outcome != null) result.outcome = outcome;
    if (runId != null) result.runId = runId;
    return result;
  }

  ListAuditEntriesRequest._();

  factory ListAuditEntriesRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListAuditEntriesRequest()..mergeFromBuffer(data, registry);
  factory ListAuditEntriesRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListAuditEntriesRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ListAuditEntriesRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ListAuditEntriesRequest.$_createMessage)
    ..aI(1, _omitFieldNames ? '' : 'limit')
    ..aOS(2, _omitFieldNames ? '' : 'pageToken')
    ..aOS(3, _omitFieldNames ? '' : 'actor')
    ..aOS(4, _omitFieldNames ? '' : 'procedure')
    ..aOS(5, _omitFieldNames ? '' : 'subject')
    ..aOS(6, _omitFieldNames ? '' : 'outcome')
    ..aOS(7, _omitFieldNames ? '' : 'runId')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListAuditEntriesRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListAuditEntriesRequest copyWith(
          void Function(ListAuditEntriesRequest) updates) =>
      super.copyWith((message) => updates(message as ListAuditEntriesRequest))
          as ListAuditEntriesRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use ListAuditEntriesRequest() / ListAuditEntriesRequest.new instead')
  static ListAuditEntriesRequest create() => ListAuditEntriesRequest._();
  static $pb.GeneratedMessage $_createMessage() => ListAuditEntriesRequest._();
  @$core.override
  ListAuditEntriesRequest createEmptyInstance() => ListAuditEntriesRequest._();
  @$core.pragma('dart2js:noInline')
  static ListAuditEntriesRequest getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<ListAuditEntriesRequest>(
          ListAuditEntriesRequest.$_createMessage);
  static ListAuditEntriesRequest? _defaultInstance;

  /// limit is the page size: 0 means 50, at most 500.
  @$pb.TagNumber(1)
  $core.int get limit => $_getIZ(0);
  @$pb.TagNumber(1)
  set limit($core.int value) => $_setSignedInt32(0, value);
  @$pb.TagNumber(1)
  $core.bool hasLimit() => $_has(0);
  @$pb.TagNumber(1)
  void clearLimit() => $_clearField(1);

  /// page_token is a previous response's next_page_token; empty starts at the
  /// newest entry.
  @$pb.TagNumber(2)
  $core.String get pageToken => $_getSZ(1);
  @$pb.TagNumber(2)
  set pageToken($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasPageToken() => $_has(1);
  @$pb.TagNumber(2)
  void clearPageToken() => $_clearField(2);

  /// actor, procedure and subject keep entries whose field contains this text,
  /// ignoring case.
  @$pb.TagNumber(3)
  $core.String get actor => $_getSZ(2);
  @$pb.TagNumber(3)
  set actor($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasActor() => $_has(2);
  @$pb.TagNumber(3)
  void clearActor() => $_clearField(3);

  @$pb.TagNumber(4)
  $core.String get procedure => $_getSZ(3);
  @$pb.TagNumber(4)
  set procedure($core.String value) => $_setString(3, value);
  @$pb.TagNumber(4)
  $core.bool hasProcedure() => $_has(3);
  @$pb.TagNumber(4)
  void clearProcedure() => $_clearField(4);

  @$pb.TagNumber(5)
  $core.String get subject => $_getSZ(4);
  @$pb.TagNumber(5)
  set subject($core.String value) => $_setString(4, value);
  @$pb.TagNumber(5)
  $core.bool hasSubject() => $_has(4);
  @$pb.TagNumber(5)
  void clearSubject() => $_clearField(5);

  /// outcome keeps entries with this outcome: "ok", "denied" or "error".
  @$pb.TagNumber(6)
  $core.String get outcome => $_getSZ(5);
  @$pb.TagNumber(6)
  set outcome($core.String value) => $_setString(5, value);
  @$pb.TagNumber(6)
  $core.bool hasOutcome() => $_has(5);
  @$pb.TagNumber(6)
  void clearOutcome() => $_clearField(6);

  /// run_id keeps entries of exactly this run.
  @$pb.TagNumber(7)
  $core.String get runId => $_getSZ(6);
  @$pb.TagNumber(7)
  set runId($core.String value) => $_setString(6, value);
  @$pb.TagNumber(7)
  $core.bool hasRunId() => $_has(6);
  @$pb.TagNumber(7)
  void clearRunId() => $_clearField(7);
}

class ListAuditEntriesResponse extends $pb.GeneratedMessage {
  factory ListAuditEntriesResponse({
    $core.Iterable<AuditEntry>? entries,
    $core.String? nextPageToken,
    AuditHead? head,
  }) {
    final result = ListAuditEntriesResponse._();
    if (entries != null) result.entries.addAll(entries);
    if (nextPageToken != null) result.nextPageToken = nextPageToken;
    if (head != null) result.head = head;
    return result;
  }

  ListAuditEntriesResponse._();

  factory ListAuditEntriesResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListAuditEntriesResponse()..mergeFromBuffer(data, registry);
  factory ListAuditEntriesResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListAuditEntriesResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ListAuditEntriesResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ListAuditEntriesResponse.$_createMessage)
    ..pPM<AuditEntry>(1, _omitFieldNames ? '' : 'entries',
        subBuilder: AuditEntry.$_createMessage)
    ..aOS(2, _omitFieldNames ? '' : 'nextPageToken')
    ..aOM<AuditHead>(3, _omitFieldNames ? '' : 'head',
        subBuilder: AuditHead.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListAuditEntriesResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListAuditEntriesResponse copyWith(
          void Function(ListAuditEntriesResponse) updates) =>
      super.copyWith((message) => updates(message as ListAuditEntriesResponse))
          as ListAuditEntriesResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use ListAuditEntriesResponse() / ListAuditEntriesResponse.new instead')
  static ListAuditEntriesResponse create() => ListAuditEntriesResponse._();
  static $pb.GeneratedMessage $_createMessage() => ListAuditEntriesResponse._();
  @$core.override
  ListAuditEntriesResponse createEmptyInstance() =>
      ListAuditEntriesResponse._();
  @$core.pragma('dart2js:noInline')
  static ListAuditEntriesResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<ListAuditEntriesResponse>(
          ListAuditEntriesResponse.$_createMessage);
  static ListAuditEntriesResponse? _defaultInstance;

  /// entries are newest first.
  @$pb.TagNumber(1)
  $pb.PbList<AuditEntry> get entries => $_getList(0);

  /// next_page_token continues with older entries; empty at the start of the
  /// log.
  @$pb.TagNumber(2)
  $core.String get nextPageToken => $_getSZ(1);
  @$pb.TagNumber(2)
  set nextPageToken($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasNextPageToken() => $_has(1);
  @$pb.TagNumber(2)
  void clearNextPageToken() => $_clearField(2);

  /// head is the newest entry as the hub recorded it outside the log.
  @$pb.TagNumber(3)
  AuditHead get head => $_getN(2);
  @$pb.TagNumber(3)
  set head(AuditHead value) => $_setField(3, value);
  @$pb.TagNumber(3)
  $core.bool hasHead() => $_has(2);
  @$pb.TagNumber(3)
  void clearHead() => $_clearField(3);
  @$pb.TagNumber(3)
  AuditHead ensureHead() => $_ensure(2);
}

class AuditHead extends $pb.GeneratedMessage {
  factory AuditHead({
    $fixnum.Int64? seq,
    $core.String? hash,
  }) {
    final result = AuditHead._();
    if (seq != null) result.seq = seq;
    if (hash != null) result.hash = hash;
    return result;
  }

  AuditHead._();

  factory AuditHead.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      AuditHead()..mergeFromBuffer(data, registry);
  factory AuditHead.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      AuditHead()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'AuditHead',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: AuditHead.$_createMessage)
    ..a<$fixnum.Int64>(1, _omitFieldNames ? '' : 'seq', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..aOS(2, _omitFieldNames ? '' : 'hash')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  AuditHead clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  AuditHead copyWith(void Function(AuditHead) updates) =>
      super.copyWith((message) => updates(message as AuditHead)) as AuditHead;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use AuditHead() / AuditHead.new instead')
  static AuditHead create() => AuditHead._();
  static $pb.GeneratedMessage $_createMessage() => AuditHead._();
  @$core.override
  AuditHead createEmptyInstance() => AuditHead._();
  @$core.pragma('dart2js:noInline')
  static AuditHead getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<AuditHead>(AuditHead.$_createMessage);
  static AuditHead? _defaultInstance;

  @$pb.TagNumber(1)
  $fixnum.Int64 get seq => $_getI64(0);
  @$pb.TagNumber(1)
  set seq($fixnum.Int64 value) => $_setInt64(0, value);
  @$pb.TagNumber(1)
  $core.bool hasSeq() => $_has(0);
  @$pb.TagNumber(1)
  void clearSeq() => $_clearField(1);

  @$pb.TagNumber(2)
  $core.String get hash => $_getSZ(1);
  @$pb.TagNumber(2)
  set hash($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasHash() => $_has(1);
  @$pb.TagNumber(2)
  void clearHash() => $_clearField(2);
}

class VerifyAuditRequest extends $pb.GeneratedMessage {
  factory VerifyAuditRequest() => VerifyAuditRequest._();

  VerifyAuditRequest._();

  factory VerifyAuditRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      VerifyAuditRequest()..mergeFromBuffer(data, registry);
  factory VerifyAuditRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      VerifyAuditRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'VerifyAuditRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: VerifyAuditRequest.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  VerifyAuditRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  VerifyAuditRequest copyWith(void Function(VerifyAuditRequest) updates) =>
      super.copyWith((message) => updates(message as VerifyAuditRequest))
          as VerifyAuditRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use VerifyAuditRequest() / VerifyAuditRequest.new instead')
  static VerifyAuditRequest create() => VerifyAuditRequest._();
  static $pb.GeneratedMessage $_createMessage() => VerifyAuditRequest._();
  @$core.override
  VerifyAuditRequest createEmptyInstance() => VerifyAuditRequest._();
  @$core.pragma('dart2js:noInline')
  static VerifyAuditRequest getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<VerifyAuditRequest>(
          VerifyAuditRequest.$_createMessage);
  static VerifyAuditRequest? _defaultInstance;
}

class VerifyAuditResponse extends $pb.GeneratedMessage {
  factory VerifyAuditResponse({
    $core.bool? ok,
    $core.String? error,
    $fixnum.Int64? entries,
    AuditHead? head,
    $fixnum.Int64? beyondHead,
  }) {
    final result = VerifyAuditResponse._();
    if (ok != null) result.ok = ok;
    if (error != null) result.error = error;
    if (entries != null) result.entries = entries;
    if (head != null) result.head = head;
    if (beyondHead != null) result.beyondHead = beyondHead;
    return result;
  }

  VerifyAuditResponse._();

  factory VerifyAuditResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      VerifyAuditResponse()..mergeFromBuffer(data, registry);
  factory VerifyAuditResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      VerifyAuditResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'VerifyAuditResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: VerifyAuditResponse.$_createMessage)
    ..aOB(1, _omitFieldNames ? '' : 'ok')
    ..aOS(2, _omitFieldNames ? '' : 'error')
    ..a<$fixnum.Int64>(3, _omitFieldNames ? '' : 'entries', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..aOM<AuditHead>(4, _omitFieldNames ? '' : 'head',
        subBuilder: AuditHead.$_createMessage)
    ..a<$fixnum.Int64>(
        5, _omitFieldNames ? '' : 'beyondHead', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  VerifyAuditResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  VerifyAuditResponse copyWith(void Function(VerifyAuditResponse) updates) =>
      super.copyWith((message) => updates(message as VerifyAuditResponse))
          as VerifyAuditResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core
      .Deprecated('Use VerifyAuditResponse() / VerifyAuditResponse.new instead')
  static VerifyAuditResponse create() => VerifyAuditResponse._();
  static $pb.GeneratedMessage $_createMessage() => VerifyAuditResponse._();
  @$core.override
  VerifyAuditResponse createEmptyInstance() => VerifyAuditResponse._();
  @$core.pragma('dart2js:noInline')
  static VerifyAuditResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<VerifyAuditResponse>(
          VerifyAuditResponse.$_createMessage);
  static VerifyAuditResponse? _defaultInstance;

  /// ok is false when the chain is broken; error names the file and line.
  @$pb.TagNumber(1)
  $core.bool get ok => $_getBF(0);
  @$pb.TagNumber(1)
  set ok($core.bool value) => $_setBool(0, value);
  @$pb.TagNumber(1)
  $core.bool hasOk() => $_has(0);
  @$pb.TagNumber(1)
  void clearOk() => $_clearField(1);

  @$pb.TagNumber(2)
  $core.String get error => $_getSZ(1);
  @$pb.TagNumber(2)
  set error($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasError() => $_has(1);
  @$pb.TagNumber(2)
  void clearError() => $_clearField(2);

  @$pb.TagNumber(3)
  $fixnum.Int64 get entries => $_getI64(2);
  @$pb.TagNumber(3)
  set entries($fixnum.Int64 value) => $_setInt64(2, value);
  @$pb.TagNumber(3)
  $core.bool hasEntries() => $_has(2);
  @$pb.TagNumber(3)
  void clearEntries() => $_clearField(3);

  @$pb.TagNumber(4)
  AuditHead get head => $_getN(3);
  @$pb.TagNumber(4)
  set head(AuditHead value) => $_setField(4, value);
  @$pb.TagNumber(4)
  $core.bool hasHead() => $_has(3);
  @$pb.TagNumber(4)
  void clearHead() => $_clearField(4);
  @$pb.TagNumber(4)
  AuditHead ensureHead() => $_ensure(3);

  /// beyond_head counts entries after the recorded head, left by a writer
  /// that died between appending and recording; the next write records them.
  @$pb.TagNumber(5)
  $fixnum.Int64 get beyondHead => $_getI64(4);
  @$pb.TagNumber(5)
  set beyondHead($fixnum.Int64 value) => $_setInt64(4, value);
  @$pb.TagNumber(5)
  $core.bool hasBeyondHead() => $_has(4);
  @$pb.TagNumber(5)
  void clearBeyondHead() => $_clearField(5);
}

/// AuditEntry is one audit line. Secrets and request bodies are never logged.
class AuditEntry extends $pb.GeneratedMessage {
  factory AuditEntry({
    $0.Timestamp? time,
    $core.String? actor,
    $core.String? tokenId,
    $core.String? procedure,
    $core.String? subject,
    $core.String? outcome,
    $core.String? code,
    $core.String? task,
    $core.String? detail,
    $core.String? runId,
    $fixnum.Int64? seq,
    $core.String? prevHash,
    $core.String? hash,
  }) {
    final result = AuditEntry._();
    if (time != null) result.time = time;
    if (actor != null) result.actor = actor;
    if (tokenId != null) result.tokenId = tokenId;
    if (procedure != null) result.procedure = procedure;
    if (subject != null) result.subject = subject;
    if (outcome != null) result.outcome = outcome;
    if (code != null) result.code = code;
    if (task != null) result.task = task;
    if (detail != null) result.detail = detail;
    if (runId != null) result.runId = runId;
    if (seq != null) result.seq = seq;
    if (prevHash != null) result.prevHash = prevHash;
    if (hash != null) result.hash = hash;
    return result;
  }

  AuditEntry._();

  factory AuditEntry.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      AuditEntry()..mergeFromBuffer(data, registry);
  factory AuditEntry.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      AuditEntry()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'AuditEntry',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: AuditEntry.$_createMessage)
    ..aOM<$0.Timestamp>(1, _omitFieldNames ? '' : 'time',
        subBuilder: $0.Timestamp.$_createMessage)
    ..aOS(2, _omitFieldNames ? '' : 'actor')
    ..aOS(3, _omitFieldNames ? '' : 'tokenId')
    ..aOS(5, _omitFieldNames ? '' : 'procedure')
    ..aOS(6, _omitFieldNames ? '' : 'subject')
    ..aOS(7, _omitFieldNames ? '' : 'outcome')
    ..aOS(8, _omitFieldNames ? '' : 'code')
    ..aOS(9, _omitFieldNames ? '' : 'task')
    ..aOS(10, _omitFieldNames ? '' : 'detail')
    ..aOS(11, _omitFieldNames ? '' : 'runId')
    ..a<$fixnum.Int64>(12, _omitFieldNames ? '' : 'seq', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..aOS(13, _omitFieldNames ? '' : 'prevHash')
    ..aOS(14, _omitFieldNames ? '' : 'hash')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  AuditEntry clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  AuditEntry copyWith(void Function(AuditEntry) updates) =>
      super.copyWith((message) => updates(message as AuditEntry)) as AuditEntry;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use AuditEntry() / AuditEntry.new instead')
  static AuditEntry create() => AuditEntry._();
  static $pb.GeneratedMessage $_createMessage() => AuditEntry._();
  @$core.override
  AuditEntry createEmptyInstance() => AuditEntry._();
  @$core.pragma('dart2js:noInline')
  static AuditEntry getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<AuditEntry>(AuditEntry.$_createMessage);
  static AuditEntry? _defaultInstance;

  @$pb.TagNumber(1)
  $0.Timestamp get time => $_getN(0);
  @$pb.TagNumber(1)
  set time($0.Timestamp value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasTime() => $_has(0);
  @$pb.TagNumber(1)
  void clearTime() => $_clearField(1);
  @$pb.TagNumber(1)
  $0.Timestamp ensureTime() => $_ensure(0);

  /// actor is "owner", "token:<name>" or "unauthenticated".
  @$pb.TagNumber(2)
  $core.String get actor => $_getSZ(1);
  @$pb.TagNumber(2)
  set actor($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasActor() => $_has(1);
  @$pb.TagNumber(2)
  void clearActor() => $_clearField(2);

  @$pb.TagNumber(3)
  $core.String get tokenId => $_getSZ(2);
  @$pb.TagNumber(3)
  set tokenId($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasTokenId() => $_has(2);
  @$pb.TagNumber(3)
  void clearTokenId() => $_clearField(3);

  /// procedure is the Connect procedure, or the terminal path.
  @$pb.TagNumber(5)
  $core.String get procedure => $_getSZ(3);
  @$pb.TagNumber(5)
  set procedure($core.String value) => $_setString(3, value);
  @$pb.TagNumber(5)
  $core.bool hasProcedure() => $_has(3);
  @$pb.TagNumber(5)
  void clearProcedure() => $_clearField(5);

  /// subject is the request's name or id, nothing else from the body.
  @$pb.TagNumber(6)
  $core.String get subject => $_getSZ(4);
  @$pb.TagNumber(6)
  set subject($core.String value) => $_setString(4, value);
  @$pb.TagNumber(6)
  $core.bool hasSubject() => $_has(4);
  @$pb.TagNumber(6)
  void clearSubject() => $_clearField(6);

  /// outcome is "ok", "denied" or "error"; code is the Connect code.
  @$pb.TagNumber(7)
  $core.String get outcome => $_getSZ(5);
  @$pb.TagNumber(7)
  set outcome($core.String value) => $_setString(5, value);
  @$pb.TagNumber(7)
  $core.bool hasOutcome() => $_has(5);
  @$pb.TagNumber(7)
  void clearOutcome() => $_clearField(7);

  @$pb.TagNumber(8)
  $core.String get code => $_getSZ(6);
  @$pb.TagNumber(8)
  set code($core.String value) => $_setString(6, value);
  @$pb.TagNumber(8)
  $core.bool hasCode() => $_has(6);
  @$pb.TagNumber(8)
  void clearCode() => $_clearField(8);

  /// task is the task recorded in the capability that made the call.
  @$pb.TagNumber(9)
  $core.String get task => $_getSZ(7);
  @$pb.TagNumber(9)
  set task($core.String value) => $_setString(7, value);
  @$pb.TagNumber(9)
  $core.bool hasTask() => $_has(7);
  @$pb.TagNumber(9)
  void clearTask() => $_clearField(9);

  /// detail is what the handler added, such as the sleeve or session it acted on.
  @$pb.TagNumber(10)
  $core.String get detail => $_getSZ(8);
  @$pb.TagNumber(10)
  set detail($core.String value) => $_setString(8, value);
  @$pb.TagNumber(10)
  $core.bool hasDetail() => $_has(8);
  @$pb.TagNumber(10)
  void clearDetail() => $_clearField(10);

  /// run_id is the run of the sleeve that made the call or that it concerns.
  @$pb.TagNumber(11)
  $core.String get runId => $_getSZ(9);
  @$pb.TagNumber(11)
  set runId($core.String value) => $_setString(9, value);
  @$pb.TagNumber(11)
  $core.bool hasRunId() => $_has(9);
  @$pb.TagNumber(11)
  void clearRunId() => $_clearField(11);

  /// seq numbers entries from 1 across every file; prev_hash is the hash of
  /// entry seq-1 and hash is SHA-256 over this line with an empty hash.
  @$pb.TagNumber(12)
  $fixnum.Int64 get seq => $_getI64(10);
  @$pb.TagNumber(12)
  set seq($fixnum.Int64 value) => $_setInt64(10, value);
  @$pb.TagNumber(12)
  $core.bool hasSeq() => $_has(10);
  @$pb.TagNumber(12)
  void clearSeq() => $_clearField(12);

  @$pb.TagNumber(13)
  $core.String get prevHash => $_getSZ(11);
  @$pb.TagNumber(13)
  set prevHash($core.String value) => $_setString(11, value);
  @$pb.TagNumber(13)
  $core.bool hasPrevHash() => $_has(11);
  @$pb.TagNumber(13)
  void clearPrevHash() => $_clearField(13);

  @$pb.TagNumber(14)
  $core.String get hash => $_getSZ(12);
  @$pb.TagNumber(14)
  set hash($core.String value) => $_setString(12, value);
  @$pb.TagNumber(14)
  $core.bool hasHash() => $_has(12);
  @$pb.TagNumber(14)
  void clearHash() => $_clearField(14);
}

/// AuditService reads the hub's audit log (docs/auth.md): every mutating call
/// and terminal attach, with who made it and how it ended. Each entry carries
/// the hash of the one before it, and the hub records the newest hash outside
/// the log, so an edited, removed or truncated entry breaks the chain. Only
/// the owner token may call it.
class AuditServiceApi {
  final $pb.RpcClient _client;

  AuditServiceApi(this._client);

  /// ListAuditEntries returns entries newest first, one page at a time. A page
  /// reads a bounded part of the log, so a filter that matches rarely can come
  /// back short, or empty, with a next_page_token to continue.
  $async.Future<ListAuditEntriesResponse> listAuditEntries(
          $pb.ClientContext? ctx, ListAuditEntriesRequest request) =>
      _client.invoke<ListAuditEntriesResponse>(ctx, 'AuditService',
          'ListAuditEntries', request, ListAuditEntriesResponse());

  /// VerifyAudit walks the whole chain from the first entry and checks it
  /// against the recorded head. A break is reported, not raised.
  $async.Future<VerifyAuditResponse> verifyAudit(
          $pb.ClientContext? ctx, VerifyAuditRequest request) =>
      _client.invoke<VerifyAuditResponse>(
          ctx, 'AuditService', 'VerifyAudit', request, VerifyAuditResponse());
}

const $core.bool _omitFieldNames =
    $core.bool.fromEnvironment('protobuf.omit_field_names');
const $core.bool _omitMessageNames =
    $core.bool.fromEnvironment('protobuf.omit_message_names');
