// This is a generated file - do not edit.
//
// Generated from protectorate/v1/job.proto.

// @dart = 3.3

// ignore_for_file: annotate_overrides, camel_case_types, comment_references
// ignore_for_file: constant_identifier_names
// ignore_for_file: curly_braces_in_flow_control_structures
// ignore_for_file: deprecated_member_use_from_same_package, library_prefixes
// ignore_for_file: non_constant_identifier_names, prefer_relative_imports

import 'dart:core' as $core;

import 'package:protobuf/protobuf.dart' as $pb;

/// JobMode is how the node runs a job's command.
class JobMode extends $pb.ProtobufEnum {
  /// JOB_MODE_UNSPECIFIED is treated as JOB_MODE_SANDBOXED.
  static const JobMode JOB_MODE_UNSPECIFIED =
      JobMode._(0, _omitEnumNames ? '' : 'JOB_MODE_UNSPECIFIED');

  /// JOB_MODE_SANDBOXED isolates the command as far as the node can (see
  /// JobIsolation): the job on a shared node.
  static const JobMode JOB_MODE_SANDBOXED =
      JobMode._(1, _omitEnumNames ? '' : 'JOB_MODE_SANDBOXED');

  /// JOB_MODE_PLAIN runs it as the agent's user, unsandboxed: a job on a care
  /// node under a grant, which maintains that machine.
  static const JobMode JOB_MODE_PLAIN =
      JobMode._(2, _omitEnumNames ? '' : 'JOB_MODE_PLAIN');

  /// JOB_MODE_ADMIN runs it elevated and unsandboxed.
  static const JobMode JOB_MODE_ADMIN =
      JobMode._(3, _omitEnumNames ? '' : 'JOB_MODE_ADMIN');

  static const $core.List<JobMode> values = <JobMode>[
    JOB_MODE_UNSPECIFIED,
    JOB_MODE_SANDBOXED,
    JOB_MODE_PLAIN,
    JOB_MODE_ADMIN,
  ];

  static final $core.List<JobMode?> _byValue =
      $pb.ProtobufEnum.$_initByValueList(values, 3);
  static JobMode? valueOf($core.int value) =>
      value < 0 || value >= _byValue.length ? null : _byValue[value];

  const JobMode._(super.value, super.name);
}

class JobState extends $pb.ProtobufEnum {
  static const JobState JOB_STATE_UNSPECIFIED =
      JobState._(0, _omitEnumNames ? '' : 'JOB_STATE_UNSPECIFIED');

  /// JOB_STATE_PREPARING: choosing a node, shipping the source, waiting for a
  /// free slot on the node, checking out.
  static const JobState JOB_STATE_PREPARING =
      JobState._(1, _omitEnumNames ? '' : 'JOB_STATE_PREPARING');
  static const JobState JOB_STATE_RUNNING =
      JobState._(2, _omitEnumNames ? '' : 'JOB_STATE_RUNNING');
  static const JobState JOB_STATE_SUCCEEDED =
      JobState._(3, _omitEnumNames ? '' : 'JOB_STATE_SUCCEEDED');

  /// JOB_STATE_FAILED: non-zero exit, or the job could not be set up.
  static const JobState JOB_STATE_FAILED =
      JobState._(4, _omitEnumNames ? '' : 'JOB_STATE_FAILED');
  static const JobState JOB_STATE_CANCELED =
      JobState._(5, _omitEnumNames ? '' : 'JOB_STATE_CANCELED');
  static const JobState JOB_STATE_TIMED_OUT =
      JobState._(6, _omitEnumNames ? '' : 'JOB_STATE_TIMED_OUT');

  /// JOB_STATE_KILLED: the command was ended by a signal the node did not send,
  /// which is how a resource limit or the kernel's OOM killer stops it. The
  /// error names the signal.
  static const JobState JOB_STATE_KILLED =
      JobState._(7, _omitEnumNames ? '' : 'JOB_STATE_KILLED');

  static const $core.List<JobState> values = <JobState>[
    JOB_STATE_UNSPECIFIED,
    JOB_STATE_PREPARING,
    JOB_STATE_RUNNING,
    JOB_STATE_SUCCEEDED,
    JOB_STATE_FAILED,
    JOB_STATE_CANCELED,
    JOB_STATE_TIMED_OUT,
    JOB_STATE_KILLED,
  ];

  static final $core.List<JobState?> _byValue =
      $pb.ProtobufEnum.$_initByValueList(values, 7);
  static JobState? valueOf($core.int value) =>
      value < 0 || value >= _byValue.length ? null : _byValue[value];

  const JobState._(super.value, super.name);
}

class JobEnd extends $pb.ProtobufEnum {
  static const JobEnd JOB_END_UNSPECIFIED =
      JobEnd._(0, _omitEnumNames ? '' : 'JOB_END_UNSPECIFIED');
  static const JobEnd JOB_END_EXITED =
      JobEnd._(1, _omitEnumNames ? '' : 'JOB_END_EXITED');
  static const JobEnd JOB_END_TIMED_OUT =
      JobEnd._(2, _omitEnumNames ? '' : 'JOB_END_TIMED_OUT');
  static const JobEnd JOB_END_CANCELED =
      JobEnd._(3, _omitEnumNames ? '' : 'JOB_END_CANCELED');

  /// JOB_END_ERROR: the job could not be set up or started.
  static const JobEnd JOB_END_ERROR =
      JobEnd._(4, _omitEnumNames ? '' : 'JOB_END_ERROR');

  /// JOB_END_KILLED: see JOB_STATE_KILLED.
  static const JobEnd JOB_END_KILLED =
      JobEnd._(5, _omitEnumNames ? '' : 'JOB_END_KILLED');

  static const $core.List<JobEnd> values = <JobEnd>[
    JOB_END_UNSPECIFIED,
    JOB_END_EXITED,
    JOB_END_TIMED_OUT,
    JOB_END_CANCELED,
    JOB_END_ERROR,
    JOB_END_KILLED,
  ];

  static final $core.List<JobEnd?> _byValue =
      $pb.ProtobufEnum.$_initByValueList(values, 5);
  static JobEnd? valueOf($core.int value) =>
      value < 0 || value >= _byValue.length ? null : _byValue[value];

  const JobEnd._(super.value, super.name);
}

const $core.bool _omitEnumNames =
    $core.bool.fromEnvironment('protobuf.omit_enum_names');
