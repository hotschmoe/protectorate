// This is a generated file - do not edit.
//
// Generated from protectorate/v1/chat.proto.

// @dart = 3.3

// ignore_for_file: annotate_overrides, camel_case_types, comment_references
// ignore_for_file: constant_identifier_names
// ignore_for_file: curly_braces_in_flow_control_structures
// ignore_for_file: deprecated_member_use_from_same_package, library_prefixes
// ignore_for_file: non_constant_identifier_names, prefer_relative_imports

import 'dart:core' as $core;

import 'package:protobuf/protobuf.dart' as $pb;

class SleeveMode extends $pb.ProtobufEnum {
  /// UNSPECIFIED on spawn means CHAT for agent harnesses and TERMINAL for
  /// HARNESS_SHELL.
  static const SleeveMode SLEEVE_MODE_UNSPECIFIED =
      SleeveMode._(0, _omitEnumNames ? '' : 'SLEEVE_MODE_UNSPECIFIED');
  static const SleeveMode SLEEVE_MODE_CHAT =
      SleeveMode._(1, _omitEnumNames ? '' : 'SLEEVE_MODE_CHAT');
  static const SleeveMode SLEEVE_MODE_TERMINAL =
      SleeveMode._(2, _omitEnumNames ? '' : 'SLEEVE_MODE_TERMINAL');

  static const $core.List<SleeveMode> values = <SleeveMode>[
    SLEEVE_MODE_UNSPECIFIED,
    SLEEVE_MODE_CHAT,
    SLEEVE_MODE_TERMINAL,
  ];

  static final $core.List<SleeveMode?> _byValue =
      $pb.ProtobufEnum.$_initByValueList(values, 2);
  static SleeveMode? valueOf($core.int value) =>
      value < 0 || value >= _byValue.length ? null : _byValue[value];

  const SleeveMode._(super.value, super.name);
}

/// ChatState is the single status shown for a chat, highest priority first:
/// a pending approval outranks a running turn, which outranks idle.
class ChatState extends $pb.ProtobufEnum {
  static const ChatState CHAT_STATE_UNSPECIFIED =
      ChatState._(0, _omitEnumNames ? '' : 'CHAT_STATE_UNSPECIFIED');
  static const ChatState CHAT_STATE_AWAITING_APPROVAL =
      ChatState._(1, _omitEnumNames ? '' : 'CHAT_STATE_AWAITING_APPROVAL');
  static const ChatState CHAT_STATE_WORKING =
      ChatState._(2, _omitEnumNames ? '' : 'CHAT_STATE_WORKING');
  static const ChatState CHAT_STATE_COMPACTING =
      ChatState._(3, _omitEnumNames ? '' : 'CHAT_STATE_COMPACTING');

  /// IDLE: the last turn settled and the harness waits for input.
  static const ChatState CHAT_STATE_IDLE =
      ChatState._(4, _omitEnumNames ? '' : 'CHAT_STATE_IDLE');
  static const ChatState CHAT_STATE_FAILED =
      ChatState._(5, _omitEnumNames ? '' : 'CHAT_STATE_FAILED');
  static const ChatState CHAT_STATE_EXITED =
      ChatState._(6, _omitEnumNames ? '' : 'CHAT_STATE_EXITED');

  static const $core.List<ChatState> values = <ChatState>[
    CHAT_STATE_UNSPECIFIED,
    CHAT_STATE_AWAITING_APPROVAL,
    CHAT_STATE_WORKING,
    CHAT_STATE_COMPACTING,
    CHAT_STATE_IDLE,
    CHAT_STATE_FAILED,
    CHAT_STATE_EXITED,
  ];

  static final $core.List<ChatState?> _byValue =
      $pb.ProtobufEnum.$_initByValueList(values, 6);
  static ChatState? valueOf($core.int value) =>
      value < 0 || value >= _byValue.length ? null : _byValue[value];

  const ChatState._(super.value, super.name);
}

class TurnEnd extends $pb.ProtobufEnum {
  static const TurnEnd TURN_END_UNSPECIFIED =
      TurnEnd._(0, _omitEnumNames ? '' : 'TURN_END_UNSPECIFIED');
  static const TurnEnd TURN_END_COMPLETED =
      TurnEnd._(1, _omitEnumNames ? '' : 'TURN_END_COMPLETED');

  /// CANCELED: an interrupt was requested during the turn.
  static const TurnEnd TURN_END_CANCELED =
      TurnEnd._(2, _omitEnumNames ? '' : 'TURN_END_CANCELED');

  /// ERROR: the harness or its provider reported a failure during the turn.
  static const TurnEnd TURN_END_ERROR =
      TurnEnd._(3, _omitEnumNames ? '' : 'TURN_END_ERROR');

  static const $core.List<TurnEnd> values = <TurnEnd>[
    TURN_END_UNSPECIFIED,
    TURN_END_COMPLETED,
    TURN_END_CANCELED,
    TURN_END_ERROR,
  ];

  static final $core.List<TurnEnd?> _byValue =
      $pb.ProtobufEnum.$_initByValueList(values, 3);
  static TurnEnd? valueOf($core.int value) =>
      value < 0 || value >= _byValue.length ? null : _byValue[value];

  const TurnEnd._(super.value, super.name);
}

class ChatErrorKind extends $pb.ProtobufEnum {
  /// UNSPECIFIED: the harness or its provider reported the error.
  static const ChatErrorKind CHAT_ERROR_KIND_UNSPECIFIED =
      ChatErrorKind._(0, _omitEnumNames ? '' : 'CHAT_ERROR_KIND_UNSPECIFIED');

  /// EXITED: the harness process ended without a signal, or because the hub
  /// stopped it.
  static const ChatErrorKind CHAT_ERROR_KIND_EXITED =
      ChatErrorKind._(1, _omitEnumNames ? '' : 'CHAT_ERROR_KIND_EXITED');

  /// KILLED: a signal nobody on the hub asked for ended the harness process,
  /// which is how a resource limit or the kernel's OOM killer stops it.
  static const ChatErrorKind CHAT_ERROR_KIND_KILLED =
      ChatErrorKind._(2, _omitEnumNames ? '' : 'CHAT_ERROR_KIND_KILLED');

  static const $core.List<ChatErrorKind> values = <ChatErrorKind>[
    CHAT_ERROR_KIND_UNSPECIFIED,
    CHAT_ERROR_KIND_EXITED,
    CHAT_ERROR_KIND_KILLED,
  ];

  static final $core.List<ChatErrorKind?> _byValue =
      $pb.ProtobufEnum.$_initByValueList(values, 2);
  static ChatErrorKind? valueOf($core.int value) =>
      value < 0 || value >= _byValue.length ? null : _byValue[value];

  const ChatErrorKind._(super.value, super.name);
}

class ReportStatus extends $pb.ProtobufEnum {
  static const ReportStatus REPORT_STATUS_UNSPECIFIED =
      ReportStatus._(0, _omitEnumNames ? '' : 'REPORT_STATUS_UNSPECIFIED');
  static const ReportStatus REPORT_STATUS_COMPLETE =
      ReportStatus._(1, _omitEnumNames ? '' : 'REPORT_STATUS_COMPLETE');
  static const ReportStatus REPORT_STATUS_PARTIAL =
      ReportStatus._(2, _omitEnumNames ? '' : 'REPORT_STATUS_PARTIAL');
  static const ReportStatus REPORT_STATUS_BLOCKED =
      ReportStatus._(3, _omitEnumNames ? '' : 'REPORT_STATUS_BLOCKED');

  static const $core.List<ReportStatus> values = <ReportStatus>[
    REPORT_STATUS_UNSPECIFIED,
    REPORT_STATUS_COMPLETE,
    REPORT_STATUS_PARTIAL,
    REPORT_STATUS_BLOCKED,
  ];

  static final $core.List<ReportStatus?> _byValue =
      $pb.ProtobufEnum.$_initByValueList(values, 3);
  static ReportStatus? valueOf($core.int value) =>
      value < 0 || value >= _byValue.length ? null : _byValue[value];

  const ReportStatus._(super.value, super.name);
}

class WaitUntil extends $pb.ProtobufEnum {
  static const WaitUntil WAIT_UNTIL_UNSPECIFIED =
      WaitUntil._(0, _omitEnumNames ? '' : 'WAIT_UNTIL_UNSPECIFIED');
  static const WaitUntil WAIT_UNTIL_IDLE =
      WaitUntil._(1, _omitEnumNames ? '' : 'WAIT_UNTIL_IDLE');
  static const WaitUntil WAIT_UNTIL_REPORT =
      WaitUntil._(2, _omitEnumNames ? '' : 'WAIT_UNTIL_REPORT');

  static const $core.List<WaitUntil> values = <WaitUntil>[
    WAIT_UNTIL_UNSPECIFIED,
    WAIT_UNTIL_IDLE,
    WAIT_UNTIL_REPORT,
  ];

  static final $core.List<WaitUntil?> _byValue =
      $pb.ProtobufEnum.$_initByValueList(values, 2);
  static WaitUntil? valueOf($core.int value) =>
      value < 0 || value >= _byValue.length ? null : _byValue[value];

  const WaitUntil._(super.value, super.name);
}

const $core.bool _omitEnumNames =
    $core.bool.fromEnvironment('protobuf.omit_enum_names');
