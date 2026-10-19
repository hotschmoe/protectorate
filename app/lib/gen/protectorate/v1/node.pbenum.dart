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

class FileKind extends $pb.ProtobufEnum {
  static const FileKind FILE_KIND_UNSPECIFIED =
      FileKind._(0, _omitEnumNames ? '' : 'FILE_KIND_UNSPECIFIED');
  static const FileKind FILE_KIND_FILE =
      FileKind._(1, _omitEnumNames ? '' : 'FILE_KIND_FILE');

  /// FILE_KIND_DIRECTORY travels as a tar stream of regular files and
  /// directories; symlinks and special files are skipped.
  static const FileKind FILE_KIND_DIRECTORY =
      FileKind._(2, _omitEnumNames ? '' : 'FILE_KIND_DIRECTORY');

  static const $core.List<FileKind> values = <FileKind>[
    FILE_KIND_UNSPECIFIED,
    FILE_KIND_FILE,
    FILE_KIND_DIRECTORY,
  ];

  static final $core.List<FileKind?> _byValue =
      $pb.ProtobufEnum.$_initByValueList(values, 2);
  static FileKind? valueOf($core.int value) =>
      value < 0 || value >= _byValue.length ? null : _byValue[value];

  const FileKind._(super.value, super.name);
}

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

  /// NODE_FRESHNESS_INVITED: the node was invited and has never answered a
  /// poll. Not an outage: no unreachable alert or event is raised.
  static const NodeFreshness NODE_FRESHNESS_INVITED =
      NodeFreshness._(4, _omitEnumNames ? '' : 'NODE_FRESHNESS_INVITED');

  static const $core.List<NodeFreshness> values = <NodeFreshness>[
    NODE_FRESHNESS_UNSPECIFIED,
    NODE_FRESHNESS_FRESH,
    NODE_FRESHNESS_STALE,
    NODE_FRESHNESS_UNREACHABLE,
    NODE_FRESHNESS_INVITED,
  ];

  static final $core.List<NodeFreshness?> _byValue =
      $pb.ProtobufEnum.$_initByValueList(values, 4);
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

  /// ALERT_KIND_NAME_MISMATCH: an invited node joined the tailnet under a
  /// different name than it was invited as, so the hub cannot reach it.
  static const AlertKind ALERT_KIND_NAME_MISMATCH =
      AlertKind._(4, _omitEnumNames ? '' : 'ALERT_KIND_NAME_MISMATCH');

  static const $core.List<AlertKind> values = <AlertKind>[
    ALERT_KIND_UNSPECIFIED,
    ALERT_KIND_DISK_PERCENT,
    ALERT_KIND_MEM_PRESSURE,
    ALERT_KIND_UNREACHABLE,
    ALERT_KIND_NAME_MISMATCH,
  ];

  static final $core.List<AlertKind?> _byValue =
      $pb.ProtobufEnum.$_initByValueList(values, 4);
  static AlertKind? valueOf($core.int value) =>
      value < 0 || value >= _byValue.length ? null : _byValue[value];

  const AlertKind._(super.value, super.name);
}

const $core.bool _omitEnumNames =
    $core.bool.fromEnvironment('protobuf.omit_enum_names');
