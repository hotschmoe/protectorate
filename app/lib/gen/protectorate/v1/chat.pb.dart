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
import 'package:protobuf/well_known_types/google/protobuf/timestamp.pb.dart'
    as $0;

import 'chat.pbenum.dart';

export 'package:protobuf/protobuf.dart' show GeneratedMessageGenericExtensions;

export 'chat.pbenum.dart';

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
    0: ChatEvent_Body.notSet
  };
  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ChatEvent',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ChatEvent.$_createMessage)
    ..oo(0, [10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20])
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
  }) {
    final result = TurnSettled._();
    if (stopReason != null) result.stopReason = stopReason;
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

/// ChatService drives a sleeve in chat mode: the harness runs its own official
/// headless protocol inside the sleeve and the hub normalizes what it emits
/// into ChatEvents. The hub keeps each sleeve's events in an append-only log,
/// so any number of clients can watch and a reconnecting client resumes from
/// the last seq it saw. The harness's own session store stays the full record;
/// the log is the view. A sleeve is in chat mode for life, or in terminal mode
/// and then has no chat.
class ChatServiceApi {
  final $pb.RpcClient _client;

  ChatServiceApi(this._client);

  /// Watch replays the sleeve's events after after_seq, then streams new ones
  /// until the client disconnects or the sleeve is killed.
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
}

const $core.bool _omitFieldNames =
    $core.bool.fromEnvironment('protobuf.omit_field_names');
const $core.bool _omitMessageNames =
    $core.bool.fromEnvironment('protobuf.omit_message_names');
