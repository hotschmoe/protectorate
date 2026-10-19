// This is a generated file - do not edit.
//
// Generated from protectorate/v1/lease.proto.

// @dart = 3.3

// ignore_for_file: annotate_overrides, camel_case_types, comment_references
// ignore_for_file: constant_identifier_names
// ignore_for_file: curly_braces_in_flow_control_structures
// ignore_for_file: deprecated_member_use_from_same_package, library_prefixes
// ignore_for_file: non_constant_identifier_names, prefer_relative_imports

import 'dart:core' as $core;

import 'package:protobuf/protobuf.dart' as $pb;

class LeaseState extends $pb.ProtobufEnum {
  /// UNSPECIFIED is a lease that is held.
  static const LeaseState LEASE_STATE_UNSPECIFIED =
      LeaseState._(0, _omitEnumNames ? '' : 'LEASE_STATE_UNSPECIFIED');

  /// QUEUED is a ticket waiting for its resource; id is the ticket id.
  static const LeaseState LEASE_STATE_QUEUED =
      LeaseState._(1, _omitEnumNames ? '' : 'LEASE_STATE_QUEUED');

  static const $core.List<LeaseState> values = <LeaseState>[
    LEASE_STATE_UNSPECIFIED,
    LEASE_STATE_QUEUED,
  ];

  static final $core.List<LeaseState?> _byValue =
      $pb.ProtobufEnum.$_initByValueList(values, 1);
  static LeaseState? valueOf($core.int value) =>
      value < 0 || value >= _byValue.length ? null : _byValue[value];

  const LeaseState._(super.value, super.name);
}

const $core.bool _omitEnumNames =
    $core.bool.fromEnvironment('protobuf.omit_enum_names');
