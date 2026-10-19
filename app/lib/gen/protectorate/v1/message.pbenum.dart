// This is a generated file - do not edit.
//
// Generated from protectorate/v1/message.proto.

// @dart = 3.3

// ignore_for_file: annotate_overrides, camel_case_types, comment_references
// ignore_for_file: constant_identifier_names
// ignore_for_file: curly_braces_in_flow_control_structures
// ignore_for_file: deprecated_member_use_from_same_package, library_prefixes
// ignore_for_file: non_constant_identifier_names, prefer_relative_imports

import 'dart:core' as $core;

import 'package:protobuf/protobuf.dart' as $pb;

class MessageKind extends $pb.ProtobufEnum {
  /// UNSPECIFIED sends a note.
  static const MessageKind MESSAGE_KIND_UNSPECIFIED =
      MessageKind._(0, _omitEnumNames ? '' : 'MESSAGE_KIND_UNSPECIFIED');

  /// NOTE is information; no one is asked to act.
  static const MessageKind MESSAGE_KIND_NOTE =
      MessageKind._(1, _omitEnumNames ? '' : 'MESSAGE_KIND_NOTE');

  /// REQUEST asks the recipient to do something. To a project with no live
  /// sleeve it also becomes a backlog note.
  static const MessageKind MESSAGE_KIND_REQUEST =
      MessageKind._(2, _omitEnumNames ? '' : 'MESSAGE_KIND_REQUEST');

  /// NOTICE states facts, typically sent by the hub, such as a dependency
  /// release or a granted lease.
  static const MessageKind MESSAGE_KIND_NOTICE =
      MessageKind._(3, _omitEnumNames ? '' : 'MESSAGE_KIND_NOTICE');

  static const $core.List<MessageKind> values = <MessageKind>[
    MESSAGE_KIND_UNSPECIFIED,
    MESSAGE_KIND_NOTE,
    MESSAGE_KIND_REQUEST,
    MESSAGE_KIND_NOTICE,
  ];

  static final $core.List<MessageKind?> _byValue =
      $pb.ProtobufEnum.$_initByValueList(values, 3);
  static MessageKind? valueOf($core.int value) =>
      value < 0 || value >= _byValue.length ? null : _byValue[value];

  const MessageKind._(super.value, super.name);
}

const $core.bool _omitEnumNames =
    $core.bool.fromEnvironment('protobuf.omit_enum_names');
