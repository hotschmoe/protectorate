// This is a generated file - do not edit.
//
// Generated from protectorate/v1/approval.proto.

// @dart = 3.3

// ignore_for_file: annotate_overrides, camel_case_types, comment_references
// ignore_for_file: constant_identifier_names
// ignore_for_file: curly_braces_in_flow_control_structures
// ignore_for_file: deprecated_member_use_from_same_package, library_prefixes
// ignore_for_file: non_constant_identifier_names, prefer_relative_imports

import 'dart:core' as $core;

import 'package:protobuf/protobuf.dart' as $pb;

class ApprovalState extends $pb.ProtobufEnum {
  static const ApprovalState APPROVAL_STATE_UNSPECIFIED =
      ApprovalState._(0, _omitEnumNames ? '' : 'APPROVAL_STATE_UNSPECIFIED');
  static const ApprovalState APPROVAL_STATE_PENDING =
      ApprovalState._(1, _omitEnumNames ? '' : 'APPROVAL_STATE_PENDING');

  /// APPROVED is waiting for the caller to repeat the call; USED once it ran.
  static const ApprovalState APPROVAL_STATE_APPROVED =
      ApprovalState._(2, _omitEnumNames ? '' : 'APPROVAL_STATE_APPROVED');
  static const ApprovalState APPROVAL_STATE_DENIED =
      ApprovalState._(3, _omitEnumNames ? '' : 'APPROVAL_STATE_DENIED');
  static const ApprovalState APPROVAL_STATE_EXPIRED =
      ApprovalState._(4, _omitEnumNames ? '' : 'APPROVAL_STATE_EXPIRED');
  static const ApprovalState APPROVAL_STATE_USED =
      ApprovalState._(5, _omitEnumNames ? '' : 'APPROVAL_STATE_USED');

  static const $core.List<ApprovalState> values = <ApprovalState>[
    APPROVAL_STATE_UNSPECIFIED,
    APPROVAL_STATE_PENDING,
    APPROVAL_STATE_APPROVED,
    APPROVAL_STATE_DENIED,
    APPROVAL_STATE_EXPIRED,
    APPROVAL_STATE_USED,
  ];

  static final $core.List<ApprovalState?> _byValue =
      $pb.ProtobufEnum.$_initByValueList(values, 5);
  static ApprovalState? valueOf($core.int value) =>
      value < 0 || value >= _byValue.length ? null : _byValue[value];

  const ApprovalState._(super.value, super.name);
}

const $core.bool _omitEnumNames =
    $core.bool.fromEnvironment('protobuf.omit_enum_names');
