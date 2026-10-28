// This is a generated file - do not edit.
//
// Generated from protectorate/v1/release.proto.

// @dart = 3.3

// ignore_for_file: annotate_overrides, camel_case_types, comment_references
// ignore_for_file: constant_identifier_names
// ignore_for_file: curly_braces_in_flow_control_structures
// ignore_for_file: deprecated_member_use_from_same_package, library_prefixes
// ignore_for_file: non_constant_identifier_names, prefer_relative_imports

import 'dart:core' as $core;

import 'package:protobuf/protobuf.dart' as $pb;

class UpdatePhase extends $pb.ProtobufEnum {
  static const UpdatePhase UPDATE_PHASE_UNSPECIFIED =
      UpdatePhase._(0, _omitEnumNames ? '' : 'UPDATE_PHASE_UNSPECIFIED');

  /// UPDATE_PHASE_SKIPPED: the hub already runs the requested version.
  static const UpdatePhase UPDATE_PHASE_SKIPPED =
      UpdatePhase._(1, _omitEnumNames ? '' : 'UPDATE_PHASE_SKIPPED');

  /// UPDATE_PHASE_SENDING: the release is downloading; sent_bytes of total_bytes.
  static const UpdatePhase UPDATE_PHASE_SENDING =
      UpdatePhase._(2, _omitEnumNames ? '' : 'UPDATE_PHASE_SENDING');

  /// UPDATE_PHASE_VERIFYING: the downloaded release passed its SHA-256 checks.
  static const UpdatePhase UPDATE_PHASE_VERIFYING =
      UpdatePhase._(3, _omitEnumNames ? '' : 'UPDATE_PHASE_VERIFYING');

  /// UPDATE_PHASE_SWAPPING: the new release is in place (version and
  /// previous_version are set) and the hub is about to restart.
  static const UpdatePhase UPDATE_PHASE_SWAPPING =
      UpdatePhase._(4, _omitEnumNames ? '' : 'UPDATE_PHASE_SWAPPING');

  /// UPDATE_PHASE_RESTARTING: waiting for the hub to come back.
  static const UpdatePhase UPDATE_PHASE_RESTARTING =
      UpdatePhase._(5, _omitEnumNames ? '' : 'UPDATE_PHASE_RESTARTING');

  /// UPDATE_PHASE_CONFIRMED: the hub runs the new version. Final.
  static const UpdatePhase UPDATE_PHASE_CONFIRMED =
      UpdatePhase._(6, _omitEnumNames ? '' : 'UPDATE_PHASE_CONFIRMED');

  /// UPDATE_PHASE_ROLLED_BACK: the hub came back on its previous release.
  /// Final.
  static const UpdatePhase UPDATE_PHASE_ROLLED_BACK =
      UpdatePhase._(7, _omitEnumNames ? '' : 'UPDATE_PHASE_ROLLED_BACK');

  static const $core.List<UpdatePhase> values = <UpdatePhase>[
    UPDATE_PHASE_UNSPECIFIED,
    UPDATE_PHASE_SKIPPED,
    UPDATE_PHASE_SENDING,
    UPDATE_PHASE_VERIFYING,
    UPDATE_PHASE_SWAPPING,
    UPDATE_PHASE_RESTARTING,
    UPDATE_PHASE_CONFIRMED,
    UPDATE_PHASE_ROLLED_BACK,
  ];

  static final $core.List<UpdatePhase?> _byValue =
      $pb.ProtobufEnum.$_initByValueList(values, 7);
  static UpdatePhase? valueOf($core.int value) =>
      value < 0 || value >= _byValue.length ? null : _byValue[value];

  const UpdatePhase._(super.value, super.name);
}

const $core.bool _omitEnumNames =
    $core.bool.fromEnvironment('protobuf.omit_enum_names');
