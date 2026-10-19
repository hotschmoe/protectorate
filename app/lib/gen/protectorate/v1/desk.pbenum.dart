// This is a generated file - do not edit.
//
// Generated from protectorate/v1/desk.proto.

// @dart = 3.3

// ignore_for_file: annotate_overrides, camel_case_types, comment_references
// ignore_for_file: constant_identifier_names
// ignore_for_file: curly_braces_in_flow_control_structures
// ignore_for_file: deprecated_member_use_from_same_package, library_prefixes
// ignore_for_file: non_constant_identifier_names, prefer_relative_imports

import 'dart:core' as $core;

import 'package:protobuf/protobuf.dart' as $pb;

class DeskReason extends $pb.ProtobufEnum {
  static const DeskReason DESK_REASON_UNSPECIFIED =
      DeskReason._(0, _omitEnumNames ? '' : 'DESK_REASON_UNSPECIFIED');

  /// AVAILABLE: no policy applies, or the policy allows agents now.
  static const DeskReason DESK_REASON_AVAILABLE =
      DeskReason._(1, _omitEnumNames ? '' : 'DESK_REASON_AVAILABLE');
  static const DeskReason DESK_REASON_OUTSIDE_WINDOW =
      DeskReason._(2, _omitEnumNames ? '' : 'DESK_REASON_OUTSIDE_WINDOW');
  static const DeskReason DESK_REASON_USER_ACTIVE =
      DeskReason._(3, _omitEnumNames ? '' : 'DESK_REASON_USER_ACTIVE');
  static const DeskReason DESK_REASON_NOT_INTERACTIVE =
      DeskReason._(4, _omitEnumNames ? '' : 'DESK_REASON_NOT_INTERACTIVE');
  static const DeskReason DESK_REASON_LOCKED =
      DeskReason._(5, _omitEnumNames ? '' : 'DESK_REASON_LOCKED');
  static const DeskReason DESK_REASON_UNSUPPORTED =
      DeskReason._(6, _omitEnumNames ? '' : 'DESK_REASON_UNSUPPORTED');

  /// DESK_REASON_UNKNOWN: the node has not reported (an older agent, or the
  /// poll failed).
  static const DeskReason DESK_REASON_UNKNOWN =
      DeskReason._(7, _omitEnumNames ? '' : 'DESK_REASON_UNKNOWN');

  static const $core.List<DeskReason> values = <DeskReason>[
    DESK_REASON_UNSPECIFIED,
    DESK_REASON_AVAILABLE,
    DESK_REASON_OUTSIDE_WINDOW,
    DESK_REASON_USER_ACTIVE,
    DESK_REASON_NOT_INTERACTIVE,
    DESK_REASON_LOCKED,
    DESK_REASON_UNSUPPORTED,
    DESK_REASON_UNKNOWN,
  ];

  static final $core.List<DeskReason?> _byValue =
      $pb.ProtobufEnum.$_initByValueList(values, 7);
  static DeskReason? valueOf($core.int value) =>
      value < 0 || value >= _byValue.length ? null : _byValue[value];

  const DeskReason._(super.value, super.name);
}

const $core.bool _omitEnumNames =
    $core.bool.fromEnvironment('protobuf.omit_enum_names');
