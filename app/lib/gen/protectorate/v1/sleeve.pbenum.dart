// This is a generated file - do not edit.
//
// Generated from protectorate/v1/sleeve.proto.

// @dart = 3.3

// ignore_for_file: annotate_overrides, camel_case_types, comment_references
// ignore_for_file: constant_identifier_names
// ignore_for_file: curly_braces_in_flow_control_structures
// ignore_for_file: deprecated_member_use_from_same_package, library_prefixes
// ignore_for_file: non_constant_identifier_names, prefer_relative_imports

import 'dart:core' as $core;

import 'package:protobuf/protobuf.dart' as $pb;

class Harness extends $pb.ProtobufEnum {
  static const Harness HARNESS_UNSPECIFIED =
      Harness._(0, _omitEnumNames ? '' : 'HARNESS_UNSPECIFIED');
  static const Harness HARNESS_CLAUDE =
      Harness._(1, _omitEnumNames ? '' : 'HARNESS_CLAUDE');
  static const Harness HARNESS_CODEX =
      Harness._(2, _omitEnumNames ? '' : 'HARNESS_CODEX');
  static const Harness HARNESS_GROK =
      Harness._(3, _omitEnumNames ? '' : 'HARNESS_GROK');
  static const Harness HARNESS_PI =
      Harness._(4, _omitEnumNames ? '' : 'HARNESS_PI');

  /// HARNESS_SHELL runs bash, or the argv in SpawnSleeveRequest.args when given.
  static const Harness HARNESS_SHELL =
      Harness._(5, _omitEnumNames ? '' : 'HARNESS_SHELL');

  static const $core.List<Harness> values = <Harness>[
    HARNESS_UNSPECIFIED,
    HARNESS_CLAUDE,
    HARNESS_CODEX,
    HARNESS_GROK,
    HARNESS_PI,
    HARNESS_SHELL,
  ];

  static final $core.List<Harness?> _byValue =
      $pb.ProtobufEnum.$_initByValueList(values, 5);
  static Harness? valueOf($core.int value) =>
      value < 0 || value >= _byValue.length ? null : _byValue[value];

  const Harness._(super.value, super.name);
}

class SleeveState extends $pb.ProtobufEnum {
  static const SleeveState SLEEVE_STATE_UNSPECIFIED =
      SleeveState._(0, _omitEnumNames ? '' : 'SLEEVE_STATE_UNSPECIFIED');
  static const SleeveState SLEEVE_STATE_RUNNING =
      SleeveState._(1, _omitEnumNames ? '' : 'SLEEVE_STATE_RUNNING');

  /// The harness ended or the session was killed outside KillSleeve. When the
  /// harness ended on its own, its tmux session is kept with the final screen
  /// (read it with CapturePane) until KillSleeve, a respawn, or the hub's
  /// dead-session retention sweep.
  static const SleeveState SLEEVE_STATE_EXITED =
      SleeveState._(2, _omitEnumNames ? '' : 'SLEEVE_STATE_EXITED');

  static const $core.List<SleeveState> values = <SleeveState>[
    SLEEVE_STATE_UNSPECIFIED,
    SLEEVE_STATE_RUNNING,
    SLEEVE_STATE_EXITED,
  ];

  static final $core.List<SleeveState?> _byValue =
      $pb.ProtobufEnum.$_initByValueList(values, 2);
  static SleeveState? valueOf($core.int value) =>
      value < 0 || value >= _byValue.length ? null : _byValue[value];

  const SleeveState._(super.value, super.name);
}

const $core.bool _omitEnumNames =
    $core.bool.fromEnvironment('protobuf.omit_enum_names');
