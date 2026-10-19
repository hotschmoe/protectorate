// This is a generated file - do not edit.
//
// Generated from protectorate/v1/node.proto.

// @dart = 3.3

// ignore_for_file: annotate_overrides, camel_case_types, comment_references
// ignore_for_file: constant_identifier_names
// ignore_for_file: curly_braces_in_flow_control_structures
// ignore_for_file: deprecated_member_use_from_same_package, library_prefixes
// ignore_for_file: non_constant_identifier_names, prefer_relative_imports

import 'dart:core' as $core;

import 'package:protobuf/protobuf.dart' as $pb;

class NodeFreshness extends $pb.ProtobufEnum {
  static const NodeFreshness NODE_FRESHNESS_UNSPECIFIED =
      NodeFreshness._(0, _omitEnumNames ? '' : 'NODE_FRESHNESS_UNSPECIFIED');

  /// NODE_FRESHNESS_FRESH: the last poll succeeded and is recent.
  static const NodeFreshness NODE_FRESHNESS_FRESH =
      NodeFreshness._(1, _omitEnumNames ? '' : 'NODE_FRESHNESS_FRESH');

  /// NODE_FRESHNESS_STALE: the latest poll failed, the cached metrics are
  /// recent enough to show but are not current.
  static const NodeFreshness NODE_FRESHNESS_STALE =
      NodeFreshness._(2, _omitEnumNames ? '' : 'NODE_FRESHNESS_STALE');

  /// NODE_FRESHNESS_UNREACHABLE: no good poll within the stale window (or
  /// ever). Cached metrics, if any, are old.
  static const NodeFreshness NODE_FRESHNESS_UNREACHABLE =
      NodeFreshness._(3, _omitEnumNames ? '' : 'NODE_FRESHNESS_UNREACHABLE');

  static const $core.List<NodeFreshness> values = <NodeFreshness>[
    NODE_FRESHNESS_UNSPECIFIED,
    NODE_FRESHNESS_FRESH,
    NODE_FRESHNESS_STALE,
    NODE_FRESHNESS_UNREACHABLE,
  ];

  static final $core.List<NodeFreshness?> _byValue =
      $pb.ProtobufEnum.$_initByValueList(values, 3);
  static NodeFreshness? valueOf($core.int value) =>
      value < 0 || value >= _byValue.length ? null : _byValue[value];

  const NodeFreshness._(super.value, super.name);
}

class AlertKind extends $pb.ProtobufEnum {
  static const AlertKind ALERT_KIND_UNSPECIFIED =
      AlertKind._(0, _omitEnumNames ? '' : 'ALERT_KIND_UNSPECIFIED');
  static const AlertKind ALERT_KIND_DISK_PERCENT =
      AlertKind._(1, _omitEnumNames ? '' : 'ALERT_KIND_DISK_PERCENT');
  static const AlertKind ALERT_KIND_MEM_PRESSURE =
      AlertKind._(2, _omitEnumNames ? '' : 'ALERT_KIND_MEM_PRESSURE');
  static const AlertKind ALERT_KIND_UNREACHABLE =
      AlertKind._(3, _omitEnumNames ? '' : 'ALERT_KIND_UNREACHABLE');

  static const $core.List<AlertKind> values = <AlertKind>[
    ALERT_KIND_UNSPECIFIED,
    ALERT_KIND_DISK_PERCENT,
    ALERT_KIND_MEM_PRESSURE,
    ALERT_KIND_UNREACHABLE,
  ];

  static final $core.List<AlertKind?> _byValue =
      $pb.ProtobufEnum.$_initByValueList(values, 3);
  static AlertKind? valueOf($core.int value) =>
      value < 0 || value >= _byValue.length ? null : _byValue[value];

  const AlertKind._(super.value, super.name);
}

const $core.bool _omitEnumNames =
    $core.bool.fromEnvironment('protobuf.omit_enum_names');
