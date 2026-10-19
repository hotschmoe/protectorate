// This is a generated file - do not edit.
//
// Generated from protectorate/v1/quota.proto.

// @dart = 3.3

// ignore_for_file: annotate_overrides, camel_case_types, comment_references
// ignore_for_file: constant_identifier_names
// ignore_for_file: curly_braces_in_flow_control_structures
// ignore_for_file: deprecated_member_use_from_same_package, library_prefixes
// ignore_for_file: non_constant_identifier_names, prefer_relative_imports

import 'dart:core' as $core;

import 'package:protobuf/protobuf.dart' as $pb;

class Freshness extends $pb.ProtobufEnum {
  static const Freshness FRESHNESS_UNSPECIFIED =
      Freshness._(0, _omitEnumNames ? '' : 'FRESHNESS_UNSPECIFIED');

  /// FRESHNESS_FRESH: the last good read is recent.
  static const Freshness FRESHNESS_FRESH =
      Freshness._(1, _omitEnumNames ? '' : 'FRESHNESS_FRESH');

  /// FRESHNESS_STALE: only an old good read exists; windows are that old data
  /// and must not be treated as current capacity.
  static const Freshness FRESHNESS_STALE =
      Freshness._(2, _omitEnumNames ? '' : 'FRESHNESS_STALE');

  /// FRESHNESS_UNKNOWN: no read has ever succeeded. There is no data; this is
  /// not spare capacity.
  static const Freshness FRESHNESS_UNKNOWN =
      Freshness._(3, _omitEnumNames ? '' : 'FRESHNESS_UNKNOWN');

  static const $core.List<Freshness> values = <Freshness>[
    FRESHNESS_UNSPECIFIED,
    FRESHNESS_FRESH,
    FRESHNESS_STALE,
    FRESHNESS_UNKNOWN,
  ];

  static final $core.List<Freshness?> _byValue =
      $pb.ProtobufEnum.$_initByValueList(values, 3);
  static Freshness? valueOf($core.int value) =>
      value < 0 || value >= _byValue.length ? null : _byValue[value];

  const Freshness._(super.value, super.name);
}

class TaskSize extends $pb.ProtobufEnum {
  static const TaskSize TASK_SIZE_UNSPECIFIED =
      TaskSize._(0, _omitEnumNames ? '' : 'TASK_SIZE_UNSPECIFIED');

  /// TASK_SIZE_SMALL needs at least 5% usable capacity now.
  static const TaskSize TASK_SIZE_SMALL =
      TaskSize._(1, _omitEnumNames ? '' : 'TASK_SIZE_SMALL');

  /// TASK_SIZE_MEDIUM needs at least 15%. It is the default.
  static const TaskSize TASK_SIZE_MEDIUM =
      TaskSize._(2, _omitEnumNames ? '' : 'TASK_SIZE_MEDIUM');

  /// TASK_SIZE_LARGE needs at least 30%.
  static const TaskSize TASK_SIZE_LARGE =
      TaskSize._(3, _omitEnumNames ? '' : 'TASK_SIZE_LARGE');

  static const $core.List<TaskSize> values = <TaskSize>[
    TASK_SIZE_UNSPECIFIED,
    TASK_SIZE_SMALL,
    TASK_SIZE_MEDIUM,
    TASK_SIZE_LARGE,
  ];

  static final $core.List<TaskSize?> _byValue =
      $pb.ProtobufEnum.$_initByValueList(values, 3);
  static TaskSize? valueOf($core.int value) =>
      value < 0 || value >= _byValue.length ? null : _byValue[value];

  const TaskSize._(super.value, super.name);
}

const $core.bool _omitEnumNames =
    $core.bool.fromEnvironment('protobuf.omit_enum_names');
