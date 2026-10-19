// This is a generated file - do not edit.
//
// Generated from protectorate/v1/message.proto.

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

import 'message.pbenum.dart';

export 'package:protobuf/protobuf.dart' show GeneratedMessageGenericExtensions;

export 'message.pbenum.dart';

class MessageServiceSendRequest extends $pb.GeneratedMessage {
  factory MessageServiceSendRequest({
    $core.String? to,
    $core.String? body,
    $core.String? threadId,
    MessageKind? kind,
    $core.bool? ackRequired,
    $core.String? requestId,
  }) {
    final result = MessageServiceSendRequest._();
    if (to != null) result.to = to;
    if (body != null) result.body = body;
    if (threadId != null) result.threadId = threadId;
    if (kind != null) result.kind = kind;
    if (ackRequired != null) result.ackRequired = ackRequired;
    if (requestId != null) result.requestId = requestId;
    return result;
  }

  MessageServiceSendRequest._();

  factory MessageServiceSendRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      MessageServiceSendRequest()..mergeFromBuffer(data, registry);
  factory MessageServiceSendRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      MessageServiceSendRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'MessageServiceSendRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: MessageServiceSendRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'to')
    ..aOS(2, _omitFieldNames ? '' : 'body')
    ..aOS(3, _omitFieldNames ? '' : 'threadId')
    ..aE<MessageKind>(4, _omitFieldNames ? '' : 'kind',
        enumValues: MessageKind.values)
    ..aOB(5, _omitFieldNames ? '' : 'ackRequired')
    ..aOS(6, _omitFieldNames ? '' : 'requestId')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  MessageServiceSendRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  MessageServiceSendRequest copyWith(
          void Function(MessageServiceSendRequest) updates) =>
      super.copyWith((message) => updates(message as MessageServiceSendRequest))
          as MessageServiceSendRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use MessageServiceSendRequest() / MessageServiceSendRequest.new instead')
  static MessageServiceSendRequest create() => MessageServiceSendRequest._();
  static $pb.GeneratedMessage $_createMessage() =>
      MessageServiceSendRequest._();
  @$core.override
  MessageServiceSendRequest createEmptyInstance() =>
      MessageServiceSendRequest._();
  @$core.pragma('dart2js:noInline')
  static MessageServiceSendRequest getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<MessageServiceSendRequest>(
          MessageServiceSendRequest.$_createMessage);
  static MessageServiceSendRequest? _defaultInstance;

  /// to is a sleeve name, "project:<id-or-name>", "orchestrator" or "owner".
  @$pb.TagNumber(1)
  $core.String get to => $_getSZ(0);
  @$pb.TagNumber(1)
  set to($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasTo() => $_has(0);
  @$pb.TagNumber(1)
  void clearTo() => $_clearField(1);

  @$pb.TagNumber(2)
  $core.String get body => $_getSZ(1);
  @$pb.TagNumber(2)
  set body($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasBody() => $_has(1);
  @$pb.TagNumber(2)
  void clearBody() => $_clearField(2);

  /// thread_id continues an existing thread; empty starts a new one.
  @$pb.TagNumber(3)
  $core.String get threadId => $_getSZ(2);
  @$pb.TagNumber(3)
  set threadId($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasThreadId() => $_has(2);
  @$pb.TagNumber(3)
  void clearThreadId() => $_clearField(3);

  @$pb.TagNumber(4)
  MessageKind get kind => $_getN(3);
  @$pb.TagNumber(4)
  set kind(MessageKind value) => $_setField(4, value);
  @$pb.TagNumber(4)
  $core.bool hasKind() => $_has(3);
  @$pb.TagNumber(4)
  void clearKind() => $_clearField(4);

  /// ack_required starts the ack timer.
  @$pb.TagNumber(5)
  $core.bool get ackRequired => $_getBF(4);
  @$pb.TagNumber(5)
  set ackRequired($core.bool value) => $_setBool(4, value);
  @$pb.TagNumber(5)
  $core.bool hasAckRequired() => $_has(4);
  @$pb.TagNumber(5)
  void clearAckRequired() => $_clearField(5);

  /// request_id (8-128 chars) makes a retry return the original message.
  @$pb.TagNumber(6)
  $core.String get requestId => $_getSZ(5);
  @$pb.TagNumber(6)
  set requestId($core.String value) => $_setString(5, value);
  @$pb.TagNumber(6)
  $core.bool hasRequestId() => $_has(5);
  @$pb.TagNumber(6)
  void clearRequestId() => $_clearField(6);
}

class MessageServiceSendResponse extends $pb.GeneratedMessage {
  factory MessageServiceSendResponse({
    MailMessage? message,
    $core.bool? duplicate,
    $core.bool? waiting,
    $core.String? backlogItem,
  }) {
    final result = MessageServiceSendResponse._();
    if (message != null) result.message = message;
    if (duplicate != null) result.duplicate = duplicate;
    if (waiting != null) result.waiting = waiting;
    if (backlogItem != null) result.backlogItem = backlogItem;
    return result;
  }

  MessageServiceSendResponse._();

  factory MessageServiceSendResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      MessageServiceSendResponse()..mergeFromBuffer(data, registry);
  factory MessageServiceSendResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      MessageServiceSendResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'MessageServiceSendResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: MessageServiceSendResponse.$_createMessage)
    ..aOM<MailMessage>(1, _omitFieldNames ? '' : 'message',
        subBuilder: MailMessage.$_createMessage)
    ..aOB(2, _omitFieldNames ? '' : 'duplicate')
    ..aOB(3, _omitFieldNames ? '' : 'waiting')
    ..aOS(4, _omitFieldNames ? '' : 'backlogItem')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  MessageServiceSendResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  MessageServiceSendResponse copyWith(
          void Function(MessageServiceSendResponse) updates) =>
      super.copyWith(
              (message) => updates(message as MessageServiceSendResponse))
          as MessageServiceSendResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use MessageServiceSendResponse() / MessageServiceSendResponse.new instead')
  static MessageServiceSendResponse create() => MessageServiceSendResponse._();
  static $pb.GeneratedMessage $_createMessage() =>
      MessageServiceSendResponse._();
  @$core.override
  MessageServiceSendResponse createEmptyInstance() =>
      MessageServiceSendResponse._();
  @$core.pragma('dart2js:noInline')
  static MessageServiceSendResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<MessageServiceSendResponse>(
          MessageServiceSendResponse.$_createMessage);
  static MessageServiceSendResponse? _defaultInstance;

  @$pb.TagNumber(1)
  MailMessage get message => $_getN(0);
  @$pb.TagNumber(1)
  set message(MailMessage value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasMessage() => $_has(0);
  @$pb.TagNumber(1)
  void clearMessage() => $_clearField(1);
  @$pb.TagNumber(1)
  MailMessage ensureMessage() => $_ensure(0);

  /// duplicate is true when request_id matched an earlier send.
  @$pb.TagNumber(2)
  $core.bool get duplicate => $_getBF(1);
  @$pb.TagNumber(2)
  set duplicate($core.bool value) => $_setBool(1, value);
  @$pb.TagNumber(2)
  $core.bool hasDuplicate() => $_has(1);
  @$pb.TagNumber(2)
  void clearDuplicate() => $_clearField(2);

  /// waiting is true while no recipient is resolved, such as a project with no
  /// live sleeve.
  @$pb.TagNumber(3)
  $core.bool get waiting => $_getBF(2);
  @$pb.TagNumber(3)
  set waiting($core.bool value) => $_setBool(2, value);
  @$pb.TagNumber(3)
  $core.bool hasWaiting() => $_has(2);
  @$pb.TagNumber(3)
  void clearWaiting() => $_clearField(3);

  /// backlog_item is the id of the backlog item a request was filed as.
  @$pb.TagNumber(4)
  $core.String get backlogItem => $_getSZ(3);
  @$pb.TagNumber(4)
  set backlogItem($core.String value) => $_setString(3, value);
  @$pb.TagNumber(4)
  $core.bool hasBacklogItem() => $_has(3);
  @$pb.TagNumber(4)
  void clearBacklogItem() => $_clearField(4);
}

class InboxRequest extends $pb.GeneratedMessage {
  factory InboxRequest({
    $core.bool? unreadOnly,
    $core.bool? peek,
    $core.String? as,
  }) {
    final result = InboxRequest._();
    if (unreadOnly != null) result.unreadOnly = unreadOnly;
    if (peek != null) result.peek = peek;
    if (as != null) result.as = as;
    return result;
  }

  InboxRequest._();

  factory InboxRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      InboxRequest()..mergeFromBuffer(data, registry);
  factory InboxRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      InboxRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'InboxRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: InboxRequest.$_createMessage)
    ..aOB(1, _omitFieldNames ? '' : 'unreadOnly')
    ..aOB(2, _omitFieldNames ? '' : 'peek')
    ..aOS(3, _omitFieldNames ? '' : 'as')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  InboxRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  InboxRequest copyWith(void Function(InboxRequest) updates) =>
      super.copyWith((message) => updates(message as InboxRequest))
          as InboxRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use InboxRequest() / InboxRequest.new instead')
  static InboxRequest create() => InboxRequest._();
  static $pb.GeneratedMessage $_createMessage() => InboxRequest._();
  @$core.override
  InboxRequest createEmptyInstance() => InboxRequest._();
  @$core.pragma('dart2js:noInline')
  static InboxRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<InboxRequest>(
          InboxRequest.$_createMessage);
  static InboxRequest? _defaultInstance;

  /// unread_only leaves out messages already injected, shown or acked.
  @$pb.TagNumber(1)
  $core.bool get unreadOnly => $_getBF(0);
  @$pb.TagNumber(1)
  set unreadOnly($core.bool value) => $_setBool(0, value);
  @$pb.TagNumber(1)
  $core.bool hasUnreadOnly() => $_has(0);
  @$pb.TagNumber(1)
  void clearUnreadOnly() => $_clearField(1);

  /// peek lists without marking read.
  @$pb.TagNumber(2)
  $core.bool get peek => $_getBF(1);
  @$pb.TagNumber(2)
  set peek($core.bool value) => $_setBool(1, value);
  @$pb.TagNumber(2)
  $core.bool hasPeek() => $_has(1);
  @$pb.TagNumber(2)
  void clearPeek() => $_clearField(2);

  /// as reads another mailbox; only the owner may.
  @$pb.TagNumber(3)
  $core.String get as => $_getSZ(2);
  @$pb.TagNumber(3)
  set as($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasAs() => $_has(2);
  @$pb.TagNumber(3)
  void clearAs() => $_clearField(3);
}

class InboxResponse extends $pb.GeneratedMessage {
  factory InboxResponse({
    $core.Iterable<MailMessage>? messages,
  }) {
    final result = InboxResponse._();
    if (messages != null) result.messages.addAll(messages);
    return result;
  }

  InboxResponse._();

  factory InboxResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      InboxResponse()..mergeFromBuffer(data, registry);
  factory InboxResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      InboxResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'InboxResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: InboxResponse.$_createMessage)
    ..pPM<MailMessage>(1, _omitFieldNames ? '' : 'messages',
        subBuilder: MailMessage.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  InboxResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  InboxResponse copyWith(void Function(InboxResponse) updates) =>
      super.copyWith((message) => updates(message as InboxResponse))
          as InboxResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use InboxResponse() / InboxResponse.new instead')
  static InboxResponse create() => InboxResponse._();
  static $pb.GeneratedMessage $_createMessage() => InboxResponse._();
  @$core.override
  InboxResponse createEmptyInstance() => InboxResponse._();
  @$core.pragma('dart2js:noInline')
  static InboxResponse getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<InboxResponse>(
          InboxResponse.$_createMessage);
  static InboxResponse? _defaultInstance;

  @$pb.TagNumber(1)
  $pb.PbList<MailMessage> get messages => $_getList(0);
}

class AckRequest extends $pb.GeneratedMessage {
  factory AckRequest({
    $core.String? id,
    $core.String? as,
  }) {
    final result = AckRequest._();
    if (id != null) result.id = id;
    if (as != null) result.as = as;
    return result;
  }

  AckRequest._();

  factory AckRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      AckRequest()..mergeFromBuffer(data, registry);
  factory AckRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      AckRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'AckRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: AckRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'id')
    ..aOS(2, _omitFieldNames ? '' : 'as')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  AckRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  AckRequest copyWith(void Function(AckRequest) updates) =>
      super.copyWith((message) => updates(message as AckRequest)) as AckRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use AckRequest() / AckRequest.new instead')
  static AckRequest create() => AckRequest._();
  static $pb.GeneratedMessage $_createMessage() => AckRequest._();
  @$core.override
  AckRequest createEmptyInstance() => AckRequest._();
  @$core.pragma('dart2js:noInline')
  static AckRequest getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<AckRequest>(AckRequest.$_createMessage);
  static AckRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get id => $_getSZ(0);
  @$pb.TagNumber(1)
  set id($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasId() => $_has(0);
  @$pb.TagNumber(1)
  void clearId() => $_clearField(1);

  /// as acks on behalf of a recipient; only the owner may.
  @$pb.TagNumber(2)
  $core.String get as => $_getSZ(1);
  @$pb.TagNumber(2)
  set as($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasAs() => $_has(1);
  @$pb.TagNumber(2)
  void clearAs() => $_clearField(2);
}

class AckResponse extends $pb.GeneratedMessage {
  factory AckResponse({
    MailMessage? message,
  }) {
    final result = AckResponse._();
    if (message != null) result.message = message;
    return result;
  }

  AckResponse._();

  factory AckResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      AckResponse()..mergeFromBuffer(data, registry);
  factory AckResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      AckResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'AckResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: AckResponse.$_createMessage)
    ..aOM<MailMessage>(1, _omitFieldNames ? '' : 'message',
        subBuilder: MailMessage.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  AckResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  AckResponse copyWith(void Function(AckResponse) updates) =>
      super.copyWith((message) => updates(message as AckResponse))
          as AckResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use AckResponse() / AckResponse.new instead')
  static AckResponse create() => AckResponse._();
  static $pb.GeneratedMessage $_createMessage() => AckResponse._();
  @$core.override
  AckResponse createEmptyInstance() => AckResponse._();
  @$core.pragma('dart2js:noInline')
  static AckResponse getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<AckResponse>(
          AckResponse.$_createMessage);
  static AckResponse? _defaultInstance;

  @$pb.TagNumber(1)
  MailMessage get message => $_getN(0);
  @$pb.TagNumber(1)
  set message(MailMessage value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasMessage() => $_has(0);
  @$pb.TagNumber(1)
  void clearMessage() => $_clearField(1);
  @$pb.TagNumber(1)
  MailMessage ensureMessage() => $_ensure(0);
}

class ReplyRequest extends $pb.GeneratedMessage {
  factory ReplyRequest({
    $core.String? id,
    $core.String? body,
    $core.String? requestId,
  }) {
    final result = ReplyRequest._();
    if (id != null) result.id = id;
    if (body != null) result.body = body;
    if (requestId != null) result.requestId = requestId;
    return result;
  }

  ReplyRequest._();

  factory ReplyRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ReplyRequest()..mergeFromBuffer(data, registry);
  factory ReplyRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ReplyRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ReplyRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ReplyRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'id')
    ..aOS(2, _omitFieldNames ? '' : 'body')
    ..aOS(3, _omitFieldNames ? '' : 'requestId')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ReplyRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ReplyRequest copyWith(void Function(ReplyRequest) updates) =>
      super.copyWith((message) => updates(message as ReplyRequest))
          as ReplyRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use ReplyRequest() / ReplyRequest.new instead')
  static ReplyRequest create() => ReplyRequest._();
  static $pb.GeneratedMessage $_createMessage() => ReplyRequest._();
  @$core.override
  ReplyRequest createEmptyInstance() => ReplyRequest._();
  @$core.pragma('dart2js:noInline')
  static ReplyRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<ReplyRequest>(
          ReplyRequest.$_createMessage);
  static ReplyRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get id => $_getSZ(0);
  @$pb.TagNumber(1)
  set id($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasId() => $_has(0);
  @$pb.TagNumber(1)
  void clearId() => $_clearField(1);

  @$pb.TagNumber(2)
  $core.String get body => $_getSZ(1);
  @$pb.TagNumber(2)
  set body($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasBody() => $_has(1);
  @$pb.TagNumber(2)
  void clearBody() => $_clearField(2);

  @$pb.TagNumber(3)
  $core.String get requestId => $_getSZ(2);
  @$pb.TagNumber(3)
  set requestId($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasRequestId() => $_has(2);
  @$pb.TagNumber(3)
  void clearRequestId() => $_clearField(3);
}

class ReplyResponse extends $pb.GeneratedMessage {
  factory ReplyResponse({
    MailMessage? message,
  }) {
    final result = ReplyResponse._();
    if (message != null) result.message = message;
    return result;
  }

  ReplyResponse._();

  factory ReplyResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ReplyResponse()..mergeFromBuffer(data, registry);
  factory ReplyResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ReplyResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ReplyResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ReplyResponse.$_createMessage)
    ..aOM<MailMessage>(1, _omitFieldNames ? '' : 'message',
        subBuilder: MailMessage.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ReplyResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ReplyResponse copyWith(void Function(ReplyResponse) updates) =>
      super.copyWith((message) => updates(message as ReplyResponse))
          as ReplyResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use ReplyResponse() / ReplyResponse.new instead')
  static ReplyResponse create() => ReplyResponse._();
  static $pb.GeneratedMessage $_createMessage() => ReplyResponse._();
  @$core.override
  ReplyResponse createEmptyInstance() => ReplyResponse._();
  @$core.pragma('dart2js:noInline')
  static ReplyResponse getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<ReplyResponse>(
          ReplyResponse.$_createMessage);
  static ReplyResponse? _defaultInstance;

  @$pb.TagNumber(1)
  MailMessage get message => $_getN(0);
  @$pb.TagNumber(1)
  set message(MailMessage value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasMessage() => $_has(0);
  @$pb.TagNumber(1)
  void clearMessage() => $_clearField(1);
  @$pb.TagNumber(1)
  MailMessage ensureMessage() => $_ensure(0);
}

class ThreadRequest extends $pb.GeneratedMessage {
  factory ThreadRequest({
    $core.String? threadId,
  }) {
    final result = ThreadRequest._();
    if (threadId != null) result.threadId = threadId;
    return result;
  }

  ThreadRequest._();

  factory ThreadRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ThreadRequest()..mergeFromBuffer(data, registry);
  factory ThreadRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ThreadRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ThreadRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ThreadRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'threadId')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ThreadRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ThreadRequest copyWith(void Function(ThreadRequest) updates) =>
      super.copyWith((message) => updates(message as ThreadRequest))
          as ThreadRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use ThreadRequest() / ThreadRequest.new instead')
  static ThreadRequest create() => ThreadRequest._();
  static $pb.GeneratedMessage $_createMessage() => ThreadRequest._();
  @$core.override
  ThreadRequest createEmptyInstance() => ThreadRequest._();
  @$core.pragma('dart2js:noInline')
  static ThreadRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<ThreadRequest>(
          ThreadRequest.$_createMessage);
  static ThreadRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get threadId => $_getSZ(0);
  @$pb.TagNumber(1)
  set threadId($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasThreadId() => $_has(0);
  @$pb.TagNumber(1)
  void clearThreadId() => $_clearField(1);
}

class ThreadResponse extends $pb.GeneratedMessage {
  factory ThreadResponse({
    $core.Iterable<MailMessage>? messages,
  }) {
    final result = ThreadResponse._();
    if (messages != null) result.messages.addAll(messages);
    return result;
  }

  ThreadResponse._();

  factory ThreadResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ThreadResponse()..mergeFromBuffer(data, registry);
  factory ThreadResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ThreadResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ThreadResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ThreadResponse.$_createMessage)
    ..pPM<MailMessage>(1, _omitFieldNames ? '' : 'messages',
        subBuilder: MailMessage.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ThreadResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ThreadResponse copyWith(void Function(ThreadResponse) updates) =>
      super.copyWith((message) => updates(message as ThreadResponse))
          as ThreadResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use ThreadResponse() / ThreadResponse.new instead')
  static ThreadResponse create() => ThreadResponse._();
  static $pb.GeneratedMessage $_createMessage() => ThreadResponse._();
  @$core.override
  ThreadResponse createEmptyInstance() => ThreadResponse._();
  @$core.pragma('dart2js:noInline')
  static ThreadResponse getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<ThreadResponse>(
          ThreadResponse.$_createMessage);
  static ThreadResponse? _defaultInstance;

  @$pb.TagNumber(1)
  $pb.PbList<MailMessage> get messages => $_getList(0);
}

/// MailMessage is one message with each recipient's delivery state.
class MailMessage extends $pb.GeneratedMessage {
  factory MailMessage({
    $core.String? id,
    $core.String? threadId,
    $core.String? from,
    $core.String? to,
    MessageKind? kind,
    $core.bool? ackRequired,
    $core.String? body,
    $core.String? replyTo,
    $0.Timestamp? createdAt,
    $core.Iterable<MailDelivery>? deliveries,
    $0.Timestamp? escalatedAt,
    $core.String? backlogItem,
  }) {
    final result = MailMessage._();
    if (id != null) result.id = id;
    if (threadId != null) result.threadId = threadId;
    if (from != null) result.from = from;
    if (to != null) result.to = to;
    if (kind != null) result.kind = kind;
    if (ackRequired != null) result.ackRequired = ackRequired;
    if (body != null) result.body = body;
    if (replyTo != null) result.replyTo = replyTo;
    if (createdAt != null) result.createdAt = createdAt;
    if (deliveries != null) result.deliveries.addAll(deliveries);
    if (escalatedAt != null) result.escalatedAt = escalatedAt;
    if (backlogItem != null) result.backlogItem = backlogItem;
    return result;
  }

  MailMessage._();

  factory MailMessage.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      MailMessage()..mergeFromBuffer(data, registry);
  factory MailMessage.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      MailMessage()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'MailMessage',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: MailMessage.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'id')
    ..aOS(2, _omitFieldNames ? '' : 'threadId')
    ..aOS(3, _omitFieldNames ? '' : 'from')
    ..aOS(4, _omitFieldNames ? '' : 'to')
    ..aE<MessageKind>(5, _omitFieldNames ? '' : 'kind',
        enumValues: MessageKind.values)
    ..aOB(6, _omitFieldNames ? '' : 'ackRequired')
    ..aOS(7, _omitFieldNames ? '' : 'body')
    ..aOS(8, _omitFieldNames ? '' : 'replyTo')
    ..aOM<$0.Timestamp>(9, _omitFieldNames ? '' : 'createdAt',
        subBuilder: $0.Timestamp.$_createMessage)
    ..pPM<MailDelivery>(10, _omitFieldNames ? '' : 'deliveries',
        subBuilder: MailDelivery.$_createMessage)
    ..aOM<$0.Timestamp>(11, _omitFieldNames ? '' : 'escalatedAt',
        subBuilder: $0.Timestamp.$_createMessage)
    ..aOS(12, _omitFieldNames ? '' : 'backlogItem')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  MailMessage clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  MailMessage copyWith(void Function(MailMessage) updates) =>
      super.copyWith((message) => updates(message as MailMessage))
          as MailMessage;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use MailMessage() / MailMessage.new instead')
  static MailMessage create() => MailMessage._();
  static $pb.GeneratedMessage $_createMessage() => MailMessage._();
  @$core.override
  MailMessage createEmptyInstance() => MailMessage._();
  @$core.pragma('dart2js:noInline')
  static MailMessage getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<MailMessage>(
          MailMessage.$_createMessage);
  static MailMessage? _defaultInstance;

  /// id is "m<n>"; thread_id is "t<n>".
  @$pb.TagNumber(1)
  $core.String get id => $_getSZ(0);
  @$pb.TagNumber(1)
  set id($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasId() => $_has(0);
  @$pb.TagNumber(1)
  void clearId() => $_clearField(1);

  @$pb.TagNumber(2)
  $core.String get threadId => $_getSZ(1);
  @$pb.TagNumber(2)
  set threadId($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasThreadId() => $_has(1);
  @$pb.TagNumber(2)
  void clearThreadId() => $_clearField(2);

  /// from is the sender: a sleeve name, "owner" or "hub".
  @$pb.TagNumber(3)
  $core.String get from => $_getSZ(2);
  @$pb.TagNumber(3)
  set from($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasFrom() => $_has(2);
  @$pb.TagNumber(3)
  void clearFrom() => $_clearField(3);

  /// to is the address as sent.
  @$pb.TagNumber(4)
  $core.String get to => $_getSZ(3);
  @$pb.TagNumber(4)
  set to($core.String value) => $_setString(3, value);
  @$pb.TagNumber(4)
  $core.bool hasTo() => $_has(3);
  @$pb.TagNumber(4)
  void clearTo() => $_clearField(4);

  @$pb.TagNumber(5)
  MessageKind get kind => $_getN(4);
  @$pb.TagNumber(5)
  set kind(MessageKind value) => $_setField(5, value);
  @$pb.TagNumber(5)
  $core.bool hasKind() => $_has(4);
  @$pb.TagNumber(5)
  void clearKind() => $_clearField(5);

  @$pb.TagNumber(6)
  $core.bool get ackRequired => $_getBF(5);
  @$pb.TagNumber(6)
  set ackRequired($core.bool value) => $_setBool(5, value);
  @$pb.TagNumber(6)
  $core.bool hasAckRequired() => $_has(5);
  @$pb.TagNumber(6)
  void clearAckRequired() => $_clearField(6);

  @$pb.TagNumber(7)
  $core.String get body => $_getSZ(6);
  @$pb.TagNumber(7)
  set body($core.String value) => $_setString(6, value);
  @$pb.TagNumber(7)
  $core.bool hasBody() => $_has(6);
  @$pb.TagNumber(7)
  void clearBody() => $_clearField(7);

  @$pb.TagNumber(8)
  $core.String get replyTo => $_getSZ(7);
  @$pb.TagNumber(8)
  set replyTo($core.String value) => $_setString(7, value);
  @$pb.TagNumber(8)
  $core.bool hasReplyTo() => $_has(7);
  @$pb.TagNumber(8)
  void clearReplyTo() => $_clearField(8);

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
  $pb.PbList<MailDelivery> get deliveries => $_getList(9);

  @$pb.TagNumber(11)
  $0.Timestamp get escalatedAt => $_getN(10);
  @$pb.TagNumber(11)
  set escalatedAt($0.Timestamp value) => $_setField(11, value);
  @$pb.TagNumber(11)
  $core.bool hasEscalatedAt() => $_has(10);
  @$pb.TagNumber(11)
  void clearEscalatedAt() => $_clearField(11);
  @$pb.TagNumber(11)
  $0.Timestamp ensureEscalatedAt() => $_ensure(10);

  @$pb.TagNumber(12)
  $core.String get backlogItem => $_getSZ(11);
  @$pb.TagNumber(12)
  set backlogItem($core.String value) => $_setString(11, value);
  @$pb.TagNumber(12)
  $core.bool hasBacklogItem() => $_has(11);
  @$pb.TagNumber(12)
  void clearBacklogItem() => $_clearField(12);
}

/// MailDelivery is one recipient's state for a message.
class MailDelivery extends $pb.GeneratedMessage {
  factory MailDelivery({
    $core.String? recipient,
    $0.Timestamp? deliveredAt,
    $0.Timestamp? readAt,
    $0.Timestamp? ackedAt,
    $core.int? reminders,
  }) {
    final result = MailDelivery._();
    if (recipient != null) result.recipient = recipient;
    if (deliveredAt != null) result.deliveredAt = deliveredAt;
    if (readAt != null) result.readAt = readAt;
    if (ackedAt != null) result.ackedAt = ackedAt;
    if (reminders != null) result.reminders = reminders;
    return result;
  }

  MailDelivery._();

  factory MailDelivery.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      MailDelivery()..mergeFromBuffer(data, registry);
  factory MailDelivery.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      MailDelivery()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'MailDelivery',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: MailDelivery.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'recipient')
    ..aOM<$0.Timestamp>(2, _omitFieldNames ? '' : 'deliveredAt',
        subBuilder: $0.Timestamp.$_createMessage)
    ..aOM<$0.Timestamp>(3, _omitFieldNames ? '' : 'readAt',
        subBuilder: $0.Timestamp.$_createMessage)
    ..aOM<$0.Timestamp>(4, _omitFieldNames ? '' : 'ackedAt',
        subBuilder: $0.Timestamp.$_createMessage)
    ..aI(5, _omitFieldNames ? '' : 'reminders', fieldType: $pb.PbFieldType.OU3)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  MailDelivery clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  MailDelivery copyWith(void Function(MailDelivery) updates) =>
      super.copyWith((message) => updates(message as MailDelivery))
          as MailDelivery;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use MailDelivery() / MailDelivery.new instead')
  static MailDelivery create() => MailDelivery._();
  static $pb.GeneratedMessage $_createMessage() => MailDelivery._();
  @$core.override
  MailDelivery createEmptyInstance() => MailDelivery._();
  @$core.pragma('dart2js:noInline')
  static MailDelivery getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<MailDelivery>(
          MailDelivery.$_createMessage);
  static MailDelivery? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get recipient => $_getSZ(0);
  @$pb.TagNumber(1)
  set recipient($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasRecipient() => $_has(0);
  @$pb.TagNumber(1)
  void clearRecipient() => $_clearField(1);

  /// delivered_at is when the message was injected into the recipient's chat.
  @$pb.TagNumber(2)
  $0.Timestamp get deliveredAt => $_getN(1);
  @$pb.TagNumber(2)
  set deliveredAt($0.Timestamp value) => $_setField(2, value);
  @$pb.TagNumber(2)
  $core.bool hasDeliveredAt() => $_has(1);
  @$pb.TagNumber(2)
  void clearDeliveredAt() => $_clearField(2);
  @$pb.TagNumber(2)
  $0.Timestamp ensureDeliveredAt() => $_ensure(1);

  @$pb.TagNumber(3)
  $0.Timestamp get readAt => $_getN(2);
  @$pb.TagNumber(3)
  set readAt($0.Timestamp value) => $_setField(3, value);
  @$pb.TagNumber(3)
  $core.bool hasReadAt() => $_has(2);
  @$pb.TagNumber(3)
  void clearReadAt() => $_clearField(3);
  @$pb.TagNumber(3)
  $0.Timestamp ensureReadAt() => $_ensure(2);

  @$pb.TagNumber(4)
  $0.Timestamp get ackedAt => $_getN(3);
  @$pb.TagNumber(4)
  set ackedAt($0.Timestamp value) => $_setField(4, value);
  @$pb.TagNumber(4)
  $core.bool hasAckedAt() => $_has(3);
  @$pb.TagNumber(4)
  void clearAckedAt() => $_clearField(4);
  @$pb.TagNumber(4)
  $0.Timestamp ensureAckedAt() => $_ensure(3);

  @$pb.TagNumber(5)
  $core.int get reminders => $_getIZ(4);
  @$pb.TagNumber(5)
  set reminders($core.int value) => $_setUnsignedInt32(4, value);
  @$pb.TagNumber(5)
  $core.bool hasReminders() => $_has(4);
  @$pb.TagNumber(5)
  void clearReminders() => $_clearField(5);
}

/// MessageService is the hub's mailbox between sleeves, the orchestrator and
/// the owner. The sender of a message is the authenticated caller (a sleeve's
/// own token names the sleeve), never a claimed name. Messages are kept on the
/// hub and survive restarts.
///
/// Delivery is deterministic hub code. A message to a sleeve in chat mode is
/// sent into its chat as an ordinary user message ("from" = "sleeve:<name>")
/// at its next IDLE, never during a running turn; the text says how to reply
/// and ack. A sleeve in terminal mode gets no injection and reads its Inbox.
/// A project address resolves to the sleeves currently working on that project;
/// with none live the message waits, and a request also becomes a backlog item.
/// An ack_required message not acked within the timeout (default 30 minutes)
/// is delivered once more, then escalated to the orchestrator.
///
/// Changes are announced on EventService as "msg.sent", "msg.delivered",
/// "msg.acked", "msg.reminded" and "msg.escalated" (subject = message id).
class MessageServiceApi {
  final $pb.RpcClient _client;

  MessageServiceApi(this._client);

  /// Send addresses a message to a sleeve name, "project:<id-or-name>",
  /// "orchestrator" or "owner". Retrying with the same request_id returns the
  /// original message instead of sending twice.
  $async.Future<MessageServiceSendResponse> send(
          $pb.ClientContext? ctx, MessageServiceSendRequest request) =>
      _client.invoke<MessageServiceSendResponse>(
          ctx, 'MessageService', 'Send', request, MessageServiceSendResponse());

  /// Inbox lists the messages addressed to the caller, oldest first, and marks
  /// them read unless peek is set. The owner may read another mailbox with as.
  $async.Future<InboxResponse> inbox(
          $pb.ClientContext? ctx, InboxRequest request) =>
      _client.invoke<InboxResponse>(
          ctx, 'MessageService', 'Inbox', request, InboxResponse());

  /// Ack confirms a message. It stops the ack timer.
  $async.Future<AckResponse> ack($pb.ClientContext? ctx, AckRequest request) =>
      _client.invoke<AckResponse>(
          ctx, 'MessageService', 'Ack', request, AckResponse());

  /// Reply answers a message in the same thread, addressed to its sender, and
  /// acks the original.
  $async.Future<ReplyResponse> reply(
          $pb.ClientContext? ctx, ReplyRequest request) =>
      _client.invoke<ReplyResponse>(
          ctx, 'MessageService', 'Reply', request, ReplyResponse());

  /// Thread returns every message of a thread in the order sent.
  $async.Future<ThreadResponse> thread(
          $pb.ClientContext? ctx, ThreadRequest request) =>
      _client.invoke<ThreadResponse>(
          ctx, 'MessageService', 'Thread', request, ThreadResponse());
}

const $core.bool _omitFieldNames =
    $core.bool.fromEnvironment('protobuf.omit_field_names');
const $core.bool _omitMessageNames =
    $core.bool.fromEnvironment('protobuf.omit_message_names');
