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

const $core.bool _omitEnumNames =
    $core.bool.fromEnvironment('protobuf.omit_enum_names');
