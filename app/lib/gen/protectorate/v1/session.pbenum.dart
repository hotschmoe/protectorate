// This is a generated file - do not edit.
//
// Generated from protectorate/v1/session.proto.

// @dart = 3.3

// ignore_for_file: annotate_overrides, camel_case_types, comment_references
// ignore_for_file: constant_identifier_names
// ignore_for_file: curly_braces_in_flow_control_structures
// ignore_for_file: deprecated_member_use_from_same_package, library_prefixes
// ignore_for_file: non_constant_identifier_names, prefer_relative_imports

import 'dart:core' as $core;

import 'package:protobuf/protobuf.dart' as $pb;

class SessionState extends $pb.ProtobufEnum {
  static const SessionState SESSION_STATE_UNSPECIFIED =
      SessionState._(0, _omitEnumNames ? '' : 'SESSION_STATE_UNSPECIFIED');
  static const SessionState SESSION_STATE_RUNNING =
      SessionState._(1, _omitEnumNames ? '' : 'SESSION_STATE_RUNNING');

  /// The command ended; the pane is kept with its last output.
  static const SessionState SESSION_STATE_EXITED =
      SessionState._(2, _omitEnumNames ? '' : 'SESSION_STATE_EXITED');

  static const $core.List<SessionState> values = <SessionState>[
    SESSION_STATE_UNSPECIFIED,
    SESSION_STATE_RUNNING,
    SESSION_STATE_EXITED,
  ];

  static final $core.List<SessionState?> _byValue =
      $pb.ProtobufEnum.$_initByValueList(values, 2);
  static SessionState? valueOf($core.int value) =>
      value < 0 || value >= _byValue.length ? null : _byValue[value];

  const SessionState._(super.value, super.name);
}

const $core.bool _omitEnumNames =
    $core.bool.fromEnvironment('protobuf.omit_enum_names');
