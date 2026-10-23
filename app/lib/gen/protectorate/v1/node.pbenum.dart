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

class UpdatePhase extends $pb.ProtobufEnum {
  static const UpdatePhase UPDATE_PHASE_UNSPECIFIED =
      UpdatePhase._(0, _omitEnumNames ? '' : 'UPDATE_PHASE_UNSPECIFIED');

  /// UPDATE_PHASE_SKIPPED: the node already runs the hub's version.
  static const UpdatePhase UPDATE_PHASE_SKIPPED =
      UpdatePhase._(1, _omitEnumNames ? '' : 'UPDATE_PHASE_SKIPPED');

  /// UPDATE_PHASE_SENDING: the binary is on its way; sent_bytes of total_bytes.
  static const UpdatePhase UPDATE_PHASE_SENDING =
      UpdatePhase._(2, _omitEnumNames ? '' : 'UPDATE_PHASE_SENDING');

  /// UPDATE_PHASE_VERIFYING: the node checks size, SHA-256 and that it runs.
  static const UpdatePhase UPDATE_PHASE_VERIFYING =
      UpdatePhase._(3, _omitEnumNames ? '' : 'UPDATE_PHASE_VERIFYING');

  /// UPDATE_PHASE_SWAPPING: the node swapped the binary in (version and
  /// previous_version are set) and is about to restart.
  static const UpdatePhase UPDATE_PHASE_SWAPPING =
      UpdatePhase._(4, _omitEnumNames ? '' : 'UPDATE_PHASE_SWAPPING');

  /// UPDATE_PHASE_RESTARTING: waiting for the node to come back.
  static const UpdatePhase UPDATE_PHASE_RESTARTING =
      UpdatePhase._(5, _omitEnumNames ? '' : 'UPDATE_PHASE_RESTARTING');

  /// UPDATE_PHASE_CONFIRMED: the node reports the new version. Final.
  static const UpdatePhase UPDATE_PHASE_CONFIRMED =
      UpdatePhase._(6, _omitEnumNames ? '' : 'UPDATE_PHASE_CONFIRMED');

  /// UPDATE_PHASE_ROLLED_BACK: the node came back on its previous binary.
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

class FileKind extends $pb.ProtobufEnum {
  static const FileKind FILE_KIND_UNSPECIFIED =
      FileKind._(0, _omitEnumNames ? '' : 'FILE_KIND_UNSPECIFIED');
  static const FileKind FILE_KIND_FILE =
      FileKind._(1, _omitEnumNames ? '' : 'FILE_KIND_FILE');

  /// FILE_KIND_DIRECTORY travels as a verified archive stream of regular
  /// files and directories; symlinks and special files are skipped. The
  /// archive encoding is an implementation detail of the transfer peers.
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

class RemoveJobs extends $pb.ProtobufEnum {
  /// REMOVE_JOBS_UNSPECIFIED refuses to remove a node that runs jobs.
  static const RemoveJobs REMOVE_JOBS_UNSPECIFIED =
      RemoveJobs._(0, _omitEnumNames ? '' : 'REMOVE_JOBS_UNSPECIFIED');

  /// REMOVE_JOBS_WAIT lets running jobs finish, up to wait_timeout.
  static const RemoveJobs REMOVE_JOBS_WAIT =
      RemoveJobs._(1, _omitEnumNames ? '' : 'REMOVE_JOBS_WAIT');

  /// REMOVE_JOBS_CANCEL cancels running jobs.
  static const RemoveJobs REMOVE_JOBS_CANCEL =
      RemoveJobs._(2, _omitEnumNames ? '' : 'REMOVE_JOBS_CANCEL');

  static const $core.List<RemoveJobs> values = <RemoveJobs>[
    REMOVE_JOBS_UNSPECIFIED,
    REMOVE_JOBS_WAIT,
    REMOVE_JOBS_CANCEL,
  ];

  static final $core.List<RemoveJobs?> _byValue =
      $pb.ProtobufEnum.$_initByValueList(values, 2);
  static RemoveJobs? valueOf($core.int value) =>
      value < 0 || value >= _byValue.length ? null : _byValue[value];

  const RemoveJobs._(super.value, super.name);
}

class RemovePhase extends $pb.ProtobufEnum {
  static const RemovePhase REMOVE_PHASE_UNSPECIFIED =
      RemovePhase._(0, _omitEnumNames ? '' : 'REMOVE_PHASE_UNSPECIFIED');

  /// REMOVE_PHASE_DRAINING: no new jobs or leases; running_jobs still run.
  static const RemovePhase REMOVE_PHASE_DRAINING =
      RemovePhase._(1, _omitEnumNames ? '' : 'REMOVE_PHASE_DRAINING');

  /// REMOVE_PHASE_UNINSTALLING: the agent was asked to uninstall (or skipped).
  static const RemovePhase REMOVE_PHASE_UNINSTALLING =
      RemovePhase._(2, _omitEnumNames ? '' : 'REMOVE_PHASE_UNINSTALLING');

  /// REMOVE_PHASE_REVOKING: the node's certificate and enrollment code are
  /// revoked.
  static const RemovePhase REMOVE_PHASE_REVOKING =
      RemovePhase._(3, _omitEnumNames ? '' : 'REMOVE_PHASE_REVOKING');

  /// REMOVE_PHASE_REMOVED: the inventory entry is gone. Final.
  static const RemovePhase REMOVE_PHASE_REMOVED =
      RemovePhase._(4, _omitEnumNames ? '' : 'REMOVE_PHASE_REMOVED');

  static const $core.List<RemovePhase> values = <RemovePhase>[
    REMOVE_PHASE_UNSPECIFIED,
    REMOVE_PHASE_DRAINING,
    REMOVE_PHASE_UNINSTALLING,
    REMOVE_PHASE_REVOKING,
    REMOVE_PHASE_REMOVED,
  ];

  static final $core.List<RemovePhase?> _byValue =
      $pb.ProtobufEnum.$_initByValueList(values, 4);
  static RemovePhase? valueOf($core.int value) =>
      value < 0 || value >= _byValue.length ? null : _byValue[value];

  const RemovePhase._(super.value, super.name);
}

class InstallPlatform extends $pb.ProtobufEnum {
  static const InstallPlatform INSTALL_PLATFORM_UNSPECIFIED = InstallPlatform._(
      0, _omitEnumNames ? '' : 'INSTALL_PLATFORM_UNSPECIFIED');

  /// INSTALL_PLATFORM_WINDOWS: Windows PowerShell 5.1, x64 or ARM64.
  static const InstallPlatform INSTALL_PLATFORM_WINDOWS =
      InstallPlatform._(1, _omitEnumNames ? '' : 'INSTALL_PLATFORM_WINDOWS');

  /// INSTALL_PLATFORM_LINUX: sh with curl, x86_64 or aarch64.
  static const InstallPlatform INSTALL_PLATFORM_LINUX =
      InstallPlatform._(2, _omitEnumNames ? '' : 'INSTALL_PLATFORM_LINUX');

  /// INSTALL_PLATFORM_TERMUX: Android under Termux, aarch64.
  static const InstallPlatform INSTALL_PLATFORM_TERMUX =
      InstallPlatform._(3, _omitEnumNames ? '' : 'INSTALL_PLATFORM_TERMUX');

  static const $core.List<InstallPlatform> values = <InstallPlatform>[
    INSTALL_PLATFORM_UNSPECIFIED,
    INSTALL_PLATFORM_WINDOWS,
    INSTALL_PLATFORM_LINUX,
    INSTALL_PLATFORM_TERMUX,
  ];

  static final $core.List<InstallPlatform?> _byValue =
      $pb.ProtobufEnum.$_initByValueList(values, 3);
  static InstallPlatform? valueOf($core.int value) =>
      value < 0 || value >= _byValue.length ? null : _byValue[value];

  const InstallPlatform._(super.value, super.name);
}

class NodeSetupStage extends $pb.ProtobufEnum {
  static const NodeSetupStage NODE_SETUP_STAGE_UNSPECIFIED =
      NodeSetupStage._(0, _omitEnumNames ? '' : 'NODE_SETUP_STAGE_UNSPECIFIED');

  /// NODE_SETUP_STAGE_INVITED: the code is issued; the machine has not
  /// joined yet.
  static const NodeSetupStage NODE_SETUP_STAGE_INVITED =
      NodeSetupStage._(1, _omitEnumNames ? '' : 'NODE_SETUP_STAGE_INVITED');

  /// NODE_SETUP_STAGE_JOINED: the machine redeemed its code and holds a
  /// certificate; the hub has not reached its agent yet.
  static const NodeSetupStage NODE_SETUP_STAGE_JOINED =
      NodeSetupStage._(2, _omitEnumNames ? '' : 'NODE_SETUP_STAGE_JOINED');

  /// NODE_SETUP_STAGE_READY: the node is in service. Final.
  static const NodeSetupStage NODE_SETUP_STAGE_READY =
      NodeSetupStage._(4, _omitEnumNames ? '' : 'NODE_SETUP_STAGE_READY');

  /// NODE_SETUP_STAGE_ATTENTION: something needs fixing; problems says what.
  /// The watch goes on.
  static const NodeSetupStage NODE_SETUP_STAGE_ATTENTION =
      NodeSetupStage._(5, _omitEnumNames ? '' : 'NODE_SETUP_STAGE_ATTENTION');

  static const $core.List<NodeSetupStage> values = <NodeSetupStage>[
    NODE_SETUP_STAGE_UNSPECIFIED,
    NODE_SETUP_STAGE_INVITED,
    NODE_SETUP_STAGE_JOINED,
    NODE_SETUP_STAGE_READY,
    NODE_SETUP_STAGE_ATTENTION,
  ];

  static final $core.List<NodeSetupStage?> _byValue =
      $pb.ProtobufEnum.$_initByValueList(values, 5);
  static NodeSetupStage? valueOf($core.int value) =>
      value < 0 || value >= _byValue.length ? null : _byValue[value];

  const NodeSetupStage._(super.value, super.name);
}

const $core.bool _omitEnumNames =
    $core.bool.fromEnvironment('protobuf.omit_enum_names');
