// This is a generated file - do not edit.
//
// Generated from protectorate/v1/sleeve.proto.

// @dart = 3.3

// ignore_for_file: annotate_overrides, camel_case_types, comment_references
// ignore_for_file: constant_identifier_names
// ignore_for_file: curly_braces_in_flow_control_structures
// ignore_for_file: deprecated_member_use_from_same_package, library_prefixes
// ignore_for_file: non_constant_identifier_names, prefer_relative_imports

import 'dart:async' as $async;
import 'dart:core' as $core;

import 'package:fixnum/fixnum.dart' as $fixnum;
import 'package:protobuf/protobuf.dart' as $pb;
import 'package:protobuf/well_known_types/google/protobuf/duration.pb.dart'
    as $2;
import 'package:protobuf/well_known_types/google/protobuf/timestamp.pb.dart'
    as $0;

import 'chat.pb.dart' as $1;
import 'sleeve.pbenum.dart';
import 'token.pb.dart' as $3;

export 'package:protobuf/protobuf.dart' show GeneratedMessageGenericExtensions;

export 'sleeve.pbenum.dart';

class SleeveLimits extends $pb.GeneratedMessage {
  factory SleeveLimits({
    $core.int? cpuPercent,
    $fixnum.Int64? memoryBytes,
  }) {
    final result = SleeveLimits._();
    if (cpuPercent != null) result.cpuPercent = cpuPercent;
    if (memoryBytes != null) result.memoryBytes = memoryBytes;
    return result;
  }

  SleeveLimits._();

  factory SleeveLimits.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      SleeveLimits()..mergeFromBuffer(data, registry);
  factory SleeveLimits.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      SleeveLimits()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'SleeveLimits',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: SleeveLimits.$_createMessage)
    ..aI(1, _omitFieldNames ? '' : 'cpuPercent', fieldType: $pb.PbFieldType.OU3)
    ..a<$fixnum.Int64>(
        2, _omitFieldNames ? '' : 'memoryBytes', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  SleeveLimits clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  SleeveLimits copyWith(void Function(SleeveLimits) updates) =>
      super.copyWith((message) => updates(message as SleeveLimits))
          as SleeveLimits;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use SleeveLimits() / SleeveLimits.new instead')
  static SleeveLimits create() => SleeveLimits._();
  static $pb.GeneratedMessage $_createMessage() => SleeveLimits._();
  @$core.override
  SleeveLimits createEmptyInstance() => SleeveLimits._();
  @$core.pragma('dart2js:noInline')
  static SleeveLimits getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<SleeveLimits>(
          SleeveLimits.$_createMessage);
  static SleeveLimits? _defaultInstance;

  /// cpu_percent caps CPU time; 100 is one core, 200 two cores. 0 means unlimited.
  @$pb.TagNumber(1)
  $core.int get cpuPercent => $_getIZ(0);
  @$pb.TagNumber(1)
  set cpuPercent($core.int value) => $_setUnsignedInt32(0, value);
  @$pb.TagNumber(1)
  $core.bool hasCpuPercent() => $_has(0);
  @$pb.TagNumber(1)
  void clearCpuPercent() => $_clearField(1);

  /// memory_bytes caps memory including page cache and tmpfs; swap is disabled
  /// when set. 0 means unlimited.
  @$pb.TagNumber(2)
  $fixnum.Int64 get memoryBytes => $_getI64(1);
  @$pb.TagNumber(2)
  set memoryBytes($fixnum.Int64 value) => $_setInt64(1, value);
  @$pb.TagNumber(2)
  $core.bool hasMemoryBytes() => $_has(1);
  @$pb.TagNumber(2)
  void clearMemoryBytes() => $_clearField(2);
}

/// SleeveUsage is read from the sleeve's cgroup.
class SleeveUsage extends $pb.GeneratedMessage {
  factory SleeveUsage({
    $core.double? cpuPercent,
    $core.double? cpuSeconds,
    $fixnum.Int64? memoryCurrentBytes,
    $fixnum.Int64? memoryPeakBytes,
    $core.int? pids,
    $fixnum.Int64? oomKills,
    $fixnum.Int64? workspaceDiskBytes,
    $0.Timestamp? sampledAt,
    $0.Timestamp? diskSampledAt,
  }) {
    final result = SleeveUsage._();
    if (cpuPercent != null) result.cpuPercent = cpuPercent;
    if (cpuSeconds != null) result.cpuSeconds = cpuSeconds;
    if (memoryCurrentBytes != null)
      result.memoryCurrentBytes = memoryCurrentBytes;
    if (memoryPeakBytes != null) result.memoryPeakBytes = memoryPeakBytes;
    if (pids != null) result.pids = pids;
    if (oomKills != null) result.oomKills = oomKills;
    if (workspaceDiskBytes != null)
      result.workspaceDiskBytes = workspaceDiskBytes;
    if (sampledAt != null) result.sampledAt = sampledAt;
    if (diskSampledAt != null) result.diskSampledAt = diskSampledAt;
    return result;
  }

  SleeveUsage._();

  factory SleeveUsage.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      SleeveUsage()..mergeFromBuffer(data, registry);
  factory SleeveUsage.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      SleeveUsage()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'SleeveUsage',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: SleeveUsage.$_createMessage)
    ..aD(1, _omitFieldNames ? '' : 'cpuPercent')
    ..aD(2, _omitFieldNames ? '' : 'cpuSeconds')
    ..a<$fixnum.Int64>(
        3, _omitFieldNames ? '' : 'memoryCurrentBytes', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..a<$fixnum.Int64>(
        4, _omitFieldNames ? '' : 'memoryPeakBytes', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..aI(5, _omitFieldNames ? '' : 'pids', fieldType: $pb.PbFieldType.OU3)
    ..a<$fixnum.Int64>(
        6, _omitFieldNames ? '' : 'oomKills', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..a<$fixnum.Int64>(
        7, _omitFieldNames ? '' : 'workspaceDiskBytes', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..aOM<$0.Timestamp>(8, _omitFieldNames ? '' : 'sampledAt',
        subBuilder: $0.Timestamp.$_createMessage)
    ..aOM<$0.Timestamp>(9, _omitFieldNames ? '' : 'diskSampledAt',
        subBuilder: $0.Timestamp.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  SleeveUsage clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  SleeveUsage copyWith(void Function(SleeveUsage) updates) =>
      super.copyWith((message) => updates(message as SleeveUsage))
          as SleeveUsage;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use SleeveUsage() / SleeveUsage.new instead')
  static SleeveUsage create() => SleeveUsage._();
  static $pb.GeneratedMessage $_createMessage() => SleeveUsage._();
  @$core.override
  SleeveUsage createEmptyInstance() => SleeveUsage._();
  @$core.pragma('dart2js:noInline')
  static SleeveUsage getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<SleeveUsage>(
          SleeveUsage.$_createMessage);
  static SleeveUsage? _defaultInstance;

  /// cpu_percent is CPU used over the last sample interval; 100 is one core.
  @$pb.TagNumber(1)
  $core.double get cpuPercent => $_getN(0);
  @$pb.TagNumber(1)
  set cpuPercent($core.double value) => $_setDouble(0, value);
  @$pb.TagNumber(1)
  $core.bool hasCpuPercent() => $_has(0);
  @$pb.TagNumber(1)
  void clearCpuPercent() => $_clearField(1);

  @$pb.TagNumber(2)
  $core.double get cpuSeconds => $_getN(1);
  @$pb.TagNumber(2)
  set cpuSeconds($core.double value) => $_setDouble(1, value);
  @$pb.TagNumber(2)
  $core.bool hasCpuSeconds() => $_has(1);
  @$pb.TagNumber(2)
  void clearCpuSeconds() => $_clearField(2);

  @$pb.TagNumber(3)
  $fixnum.Int64 get memoryCurrentBytes => $_getI64(2);
  @$pb.TagNumber(3)
  set memoryCurrentBytes($fixnum.Int64 value) => $_setInt64(2, value);
  @$pb.TagNumber(3)
  $core.bool hasMemoryCurrentBytes() => $_has(2);
  @$pb.TagNumber(3)
  void clearMemoryCurrentBytes() => $_clearField(3);

  @$pb.TagNumber(4)
  $fixnum.Int64 get memoryPeakBytes => $_getI64(3);
  @$pb.TagNumber(4)
  set memoryPeakBytes($fixnum.Int64 value) => $_setInt64(3, value);
  @$pb.TagNumber(4)
  $core.bool hasMemoryPeakBytes() => $_has(3);
  @$pb.TagNumber(4)
  void clearMemoryPeakBytes() => $_clearField(4);

  @$pb.TagNumber(5)
  $core.int get pids => $_getIZ(4);
  @$pb.TagNumber(5)
  set pids($core.int value) => $_setUnsignedInt32(4, value);
  @$pb.TagNumber(5)
  $core.bool hasPids() => $_has(4);
  @$pb.TagNumber(5)
  void clearPids() => $_clearField(5);

  /// oom_kills counts processes the kernel killed for exceeding the memory limit.
  @$pb.TagNumber(6)
  $fixnum.Int64 get oomKills => $_getI64(5);
  @$pb.TagNumber(6)
  set oomKills($fixnum.Int64 value) => $_setInt64(5, value);
  @$pb.TagNumber(6)
  $core.bool hasOomKills() => $_has(5);
  @$pb.TagNumber(6)
  void clearOomKills() => $_clearField(6);

  /// workspace_disk_bytes is measured on a slow interval and is 0 until the
  /// first measurement.
  @$pb.TagNumber(7)
  $fixnum.Int64 get workspaceDiskBytes => $_getI64(6);
  @$pb.TagNumber(7)
  set workspaceDiskBytes($fixnum.Int64 value) => $_setInt64(6, value);
  @$pb.TagNumber(7)
  $core.bool hasWorkspaceDiskBytes() => $_has(6);
  @$pb.TagNumber(7)
  void clearWorkspaceDiskBytes() => $_clearField(7);

  @$pb.TagNumber(8)
  $0.Timestamp get sampledAt => $_getN(7);
  @$pb.TagNumber(8)
  set sampledAt($0.Timestamp value) => $_setField(8, value);
  @$pb.TagNumber(8)
  $core.bool hasSampledAt() => $_has(7);
  @$pb.TagNumber(8)
  void clearSampledAt() => $_clearField(8);
  @$pb.TagNumber(8)
  $0.Timestamp ensureSampledAt() => $_ensure(7);

  @$pb.TagNumber(9)
  $0.Timestamp get diskSampledAt => $_getN(8);
  @$pb.TagNumber(9)
  set diskSampledAt($0.Timestamp value) => $_setField(9, value);
  @$pb.TagNumber(9)
  $core.bool hasDiskSampledAt() => $_has(8);
  @$pb.TagNumber(9)
  void clearDiskSampledAt() => $_clearField(9);
  @$pb.TagNumber(9)
  $0.Timestamp ensureDiskSampledAt() => $_ensure(8);
}

class Sleeve extends $pb.GeneratedMessage {
  factory Sleeve({
    $core.String? name,
    Harness? harness,
    $core.String? workspace,
    $core.String? session,
    $0.Timestamp? createdAt,
    SleeveState? state,
    SleeveLimits? limits,
    SleeveUsage? usage,
    $core.Iterable<$core.String>? args,
    $0.Timestamp? exitedAt,
    $core.int? exitCode,
    $core.int? exitSignal,
    $core.bool? oomKilled,
    $1.SleeveMode? mode,
    $1.ChatStatus? chat,
    $core.bool? supervised,
    $1.Handoff? handoff,
    $core.String? parent,
  }) {
    final result = Sleeve._();
    if (name != null) result.name = name;
    if (harness != null) result.harness = harness;
    if (workspace != null) result.workspace = workspace;
    if (session != null) result.session = session;
    if (createdAt != null) result.createdAt = createdAt;
    if (state != null) result.state = state;
    if (limits != null) result.limits = limits;
    if (usage != null) result.usage = usage;
    if (args != null) result.args.addAll(args);
    if (exitedAt != null) result.exitedAt = exitedAt;
    if (exitCode != null) result.exitCode = exitCode;
    if (exitSignal != null) result.exitSignal = exitSignal;
    if (oomKilled != null) result.oomKilled = oomKilled;
    if (mode != null) result.mode = mode;
    if (chat != null) result.chat = chat;
    if (supervised != null) result.supervised = supervised;
    if (handoff != null) result.handoff = handoff;
    if (parent != null) result.parent = parent;
    return result;
  }

  Sleeve._();

  factory Sleeve.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      Sleeve()..mergeFromBuffer(data, registry);
  factory Sleeve.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      Sleeve()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'Sleeve',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: Sleeve.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'name')
    ..aE<Harness>(2, _omitFieldNames ? '' : 'harness',
        enumValues: Harness.values)
    ..aOS(3, _omitFieldNames ? '' : 'workspace')
    ..aOS(4, _omitFieldNames ? '' : 'session')
    ..aOM<$0.Timestamp>(5, _omitFieldNames ? '' : 'createdAt',
        subBuilder: $0.Timestamp.$_createMessage)
    ..aE<SleeveState>(6, _omitFieldNames ? '' : 'state',
        enumValues: SleeveState.values)
    ..aOM<SleeveLimits>(7, _omitFieldNames ? '' : 'limits',
        subBuilder: SleeveLimits.$_createMessage)
    ..aOM<SleeveUsage>(8, _omitFieldNames ? '' : 'usage',
        subBuilder: SleeveUsage.$_createMessage)
    ..pPS(9, _omitFieldNames ? '' : 'args')
    ..aOM<$0.Timestamp>(10, _omitFieldNames ? '' : 'exitedAt',
        subBuilder: $0.Timestamp.$_createMessage)
    ..aI(11, _omitFieldNames ? '' : 'exitCode')
    ..aI(12, _omitFieldNames ? '' : 'exitSignal')
    ..aOB(13, _omitFieldNames ? '' : 'oomKilled')
    ..aE<$1.SleeveMode>(14, _omitFieldNames ? '' : 'mode',
        enumValues: $1.SleeveMode.values)
    ..aOM<$1.ChatStatus>(15, _omitFieldNames ? '' : 'chat',
        subBuilder: $1.ChatStatus.$_createMessage)
    ..aOB(16, _omitFieldNames ? '' : 'supervised')
    ..aOM<$1.Handoff>(17, _omitFieldNames ? '' : 'handoff',
        subBuilder: $1.Handoff.$_createMessage)
    ..aOS(20, _omitFieldNames ? '' : 'parent')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  Sleeve clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  Sleeve copyWith(void Function(Sleeve) updates) =>
      super.copyWith((message) => updates(message as Sleeve)) as Sleeve;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use Sleeve() / Sleeve.new instead')
  static Sleeve create() => Sleeve._();
  static $pb.GeneratedMessage $_createMessage() => Sleeve._();
  @$core.override
  Sleeve createEmptyInstance() => Sleeve._();
  @$core.pragma('dart2js:noInline')
  static Sleeve getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<Sleeve>(Sleeve.$_createMessage);
  static Sleeve? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get name => $_getSZ(0);
  @$pb.TagNumber(1)
  set name($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasName() => $_has(0);
  @$pb.TagNumber(1)
  void clearName() => $_clearField(1);

  @$pb.TagNumber(2)
  Harness get harness => $_getN(1);
  @$pb.TagNumber(2)
  set harness(Harness value) => $_setField(2, value);
  @$pb.TagNumber(2)
  $core.bool hasHarness() => $_has(1);
  @$pb.TagNumber(2)
  void clearHarness() => $_clearField(2);

  /// workspace is the absolute path bound read-write into the sandbox.
  @$pb.TagNumber(3)
  $core.String get workspace => $_getSZ(2);
  @$pb.TagNumber(3)
  set workspace($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasWorkspace() => $_has(2);
  @$pb.TagNumber(3)
  void clearWorkspace() => $_clearField(3);

  /// session is the tmux session name to use with SessionService and the terminal.
  @$pb.TagNumber(4)
  $core.String get session => $_getSZ(3);
  @$pb.TagNumber(4)
  set session($core.String value) => $_setString(3, value);
  @$pb.TagNumber(4)
  $core.bool hasSession() => $_has(3);
  @$pb.TagNumber(4)
  void clearSession() => $_clearField(4);

  @$pb.TagNumber(5)
  $0.Timestamp get createdAt => $_getN(4);
  @$pb.TagNumber(5)
  set createdAt($0.Timestamp value) => $_setField(5, value);
  @$pb.TagNumber(5)
  $core.bool hasCreatedAt() => $_has(4);
  @$pb.TagNumber(5)
  void clearCreatedAt() => $_clearField(5);
  @$pb.TagNumber(5)
  $0.Timestamp ensureCreatedAt() => $_ensure(4);

  @$pb.TagNumber(6)
  SleeveState get state => $_getN(5);
  @$pb.TagNumber(6)
  set state(SleeveState value) => $_setField(6, value);
  @$pb.TagNumber(6)
  $core.bool hasState() => $_has(5);
  @$pb.TagNumber(6)
  void clearState() => $_clearField(6);

  @$pb.TagNumber(7)
  SleeveLimits get limits => $_getN(6);
  @$pb.TagNumber(7)
  set limits(SleeveLimits value) => $_setField(7, value);
  @$pb.TagNumber(7)
  $core.bool hasLimits() => $_has(6);
  @$pb.TagNumber(7)
  void clearLimits() => $_clearField(7);
  @$pb.TagNumber(7)
  SleeveLimits ensureLimits() => $_ensure(6);

  @$pb.TagNumber(8)
  SleeveUsage get usage => $_getN(7);
  @$pb.TagNumber(8)
  set usage(SleeveUsage value) => $_setField(8, value);
  @$pb.TagNumber(8)
  $core.bool hasUsage() => $_has(7);
  @$pb.TagNumber(8)
  void clearUsage() => $_clearField(8);
  @$pb.TagNumber(8)
  SleeveUsage ensureUsage() => $_ensure(7);

  @$pb.TagNumber(9)
  $pb.PbList<$core.String> get args => $_getList(8);

  @$pb.TagNumber(10)
  $0.Timestamp get exitedAt => $_getN(9);
  @$pb.TagNumber(10)
  set exitedAt($0.Timestamp value) => $_setField(10, value);
  @$pb.TagNumber(10)
  $core.bool hasExitedAt() => $_has(9);
  @$pb.TagNumber(10)
  void clearExitedAt() => $_clearField(10);
  @$pb.TagNumber(10)
  $0.Timestamp ensureExitedAt() => $_ensure(9);

  /// exit_code is the harness's exit status, or 128+exit_signal when a signal
  /// ended it. Unset while running and when the exit status is unknown, such
  /// as a session killed outside the hub.
  @$pb.TagNumber(11)
  $core.int get exitCode => $_getIZ(10);
  @$pb.TagNumber(11)
  set exitCode($core.int value) => $_setSignedInt32(10, value);
  @$pb.TagNumber(11)
  $core.bool hasExitCode() => $_has(10);
  @$pb.TagNumber(11)
  void clearExitCode() => $_clearField(11);

  @$pb.TagNumber(12)
  $core.int get exitSignal => $_getIZ(11);
  @$pb.TagNumber(12)
  set exitSignal($core.int value) => $_setSignedInt32(11, value);
  @$pb.TagNumber(12)
  $core.bool hasExitSignal() => $_has(11);
  @$pb.TagNumber(12)
  void clearExitSignal() => $_clearField(12);

  /// oom_killed is true when the harness exited with SIGKILL status (137) and
  /// the kernel's OOM killer had fired in its memory cgroup. The OOM kill is
  /// read from the cgroup before it disappears; a harness that dies within a
  /// few milliseconds of the kill may still be reported as a plain exit 137.
  @$pb.TagNumber(13)
  $core.bool get oomKilled => $_getBF(12);
  @$pb.TagNumber(13)
  set oomKilled($core.bool value) => $_setBool(12, value);
  @$pb.TagNumber(13)
  $core.bool hasOomKilled() => $_has(12);
  @$pb.TagNumber(13)
  void clearOomKilled() => $_clearField(13);

  @$pb.TagNumber(14)
  $1.SleeveMode get mode => $_getN(13);
  @$pb.TagNumber(14)
  set mode($1.SleeveMode value) => $_setField(14, value);
  @$pb.TagNumber(14)
  $core.bool hasMode() => $_has(13);
  @$pb.TagNumber(14)
  void clearMode() => $_clearField(14);

  /// chat is set for sleeves in chat mode.
  @$pb.TagNumber(15)
  $1.ChatStatus get chat => $_getN(14);
  @$pb.TagNumber(15)
  set chat($1.ChatStatus value) => $_setField(15, value);
  @$pb.TagNumber(15)
  $core.bool hasChat() => $_has(14);
  @$pb.TagNumber(15)
  void clearChat() => $_clearField(15);
  @$pb.TagNumber(15)
  $1.ChatStatus ensureChat() => $_ensure(14);

  /// supervised chat sleeves run the harness in its asking permission mode:
  /// approvals arrive as ApprovalRequest events, answered with
  /// ChatService.Respond. Unsupervised sleeves run autonomous (the sandbox is
  /// the boundary).
  @$pb.TagNumber(16)
  $core.bool get supervised => $_getBF(15);
  @$pb.TagNumber(16)
  set supervised($core.bool value) => $_setBool(15, value);
  @$pb.TagNumber(16)
  $core.bool hasSupervised() => $_has(15);
  @$pb.TagNumber(16)
  void clearSupervised() => $_clearField(16);

  /// handoff is set when a resleeve produced this sleeve's harness.
  @$pb.TagNumber(17)
  $1.Handoff get handoff => $_getN(16);
  @$pb.TagNumber(17)
  set handoff($1.Handoff value) => $_setField(17, value);
  @$pb.TagNumber(17)
  $core.bool hasHandoff() => $_has(16);
  @$pb.TagNumber(17)
  void clearHandoff() => $_clearField(17);
  @$pb.TagNumber(17)
  $1.Handoff ensureHandoff() => $_ensure(16);

  /// parent is the authenticated identity that spawned this sleeve.
  @$pb.TagNumber(20)
  $core.String get parent => $_getSZ(17);
  @$pb.TagNumber(20)
  set parent($core.String value) => $_setString(17, value);
  @$pb.TagNumber(20)
  $core.bool hasParent() => $_has(17);
  @$pb.TagNumber(20)
  void clearParent() => $_clearField(20);
}

class SpawnSleeveRequest extends $pb.GeneratedMessage {
  factory SpawnSleeveRequest({
    $core.String? name,
    $core.String? workspace,
    Harness? harness,
    $core.Iterable<$core.String>? args,
    SleeveLimits? limits,
    $core.Iterable<$core.MapEntry<$core.String, $core.String>>? env,
    $1.SleeveMode? mode,
    $core.bool? supervised,
    $1.ModelSelection? selection,
  }) {
    final result = SpawnSleeveRequest._();
    if (name != null) result.name = name;
    if (workspace != null) result.workspace = workspace;
    if (harness != null) result.harness = harness;
    if (args != null) result.args.addAll(args);
    if (limits != null) result.limits = limits;
    if (env != null) result.env.addEntries(env);
    if (mode != null) result.mode = mode;
    if (supervised != null) result.supervised = supervised;
    if (selection != null) result.selection = selection;
    return result;
  }

  SpawnSleeveRequest._();

  factory SpawnSleeveRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      SpawnSleeveRequest()..mergeFromBuffer(data, registry);
  factory SpawnSleeveRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      SpawnSleeveRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'SpawnSleeveRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: SpawnSleeveRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'name')
    ..aOS(2, _omitFieldNames ? '' : 'workspace')
    ..aE<Harness>(3, _omitFieldNames ? '' : 'harness',
        enumValues: Harness.values)
    ..pPS(4, _omitFieldNames ? '' : 'args')
    ..aOM<SleeveLimits>(5, _omitFieldNames ? '' : 'limits',
        subBuilder: SleeveLimits.$_createMessage)
    ..m<$core.String, $core.String>(6, _omitFieldNames ? '' : 'env',
        entryClassName: 'SpawnSleeveRequest.EnvEntry',
        keyFieldType: $pb.PbFieldType.OS,
        valueFieldType: $pb.PbFieldType.OS,
        packageName: const $pb.PackageName('protectorate.v1'))
    ..aE<$1.SleeveMode>(7, _omitFieldNames ? '' : 'mode',
        enumValues: $1.SleeveMode.values)
    ..aOB(8, _omitFieldNames ? '' : 'supervised')
    ..aOM<$1.ModelSelection>(9, _omitFieldNames ? '' : 'selection',
        subBuilder: $1.ModelSelection.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  SpawnSleeveRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  SpawnSleeveRequest copyWith(void Function(SpawnSleeveRequest) updates) =>
      super.copyWith((message) => updates(message as SpawnSleeveRequest))
          as SpawnSleeveRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use SpawnSleeveRequest() / SpawnSleeveRequest.new instead')
  static SpawnSleeveRequest create() => SpawnSleeveRequest._();
  static $pb.GeneratedMessage $_createMessage() => SpawnSleeveRequest._();
  @$core.override
  SpawnSleeveRequest createEmptyInstance() => SpawnSleeveRequest._();
  @$core.pragma('dart2js:noInline')
  static SpawnSleeveRequest getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<SpawnSleeveRequest>(
          SpawnSleeveRequest.$_createMessage);
  static SpawnSleeveRequest? _defaultInstance;

  /// name is 1-40 of a-z 0-9 and '-', starting with a letter or digit.
  @$pb.TagNumber(1)
  $core.String get name => $_getSZ(0);
  @$pb.TagNumber(1)
  set name($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasName() => $_has(0);
  @$pb.TagNumber(1)
  void clearName() => $_clearField(1);

  /// workspace is an absolute directory path on the hub, or a git URL to clone.
  @$pb.TagNumber(2)
  $core.String get workspace => $_getSZ(1);
  @$pb.TagNumber(2)
  set workspace($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasWorkspace() => $_has(1);
  @$pb.TagNumber(2)
  void clearWorkspace() => $_clearField(2);

  @$pb.TagNumber(3)
  Harness get harness => $_getN(2);
  @$pb.TagNumber(3)
  set harness(Harness value) => $_setField(3, value);
  @$pb.TagNumber(3)
  $core.bool hasHarness() => $_has(2);
  @$pb.TagNumber(3)
  void clearHarness() => $_clearField(3);

  /// args are extra arguments for the harness, or the whole command for shell.
  @$pb.TagNumber(4)
  $pb.PbList<$core.String> get args => $_getList(3);

  @$pb.TagNumber(5)
  SleeveLimits get limits => $_getN(4);
  @$pb.TagNumber(5)
  set limits(SleeveLimits value) => $_setField(5, value);
  @$pb.TagNumber(5)
  $core.bool hasLimits() => $_has(4);
  @$pb.TagNumber(5)
  void clearLimits() => $_clearField(5);
  @$pb.TagNumber(5)
  SleeveLimits ensureLimits() => $_ensure(4);

  /// env holds extra environment variables for the harness.
  @$pb.TagNumber(6)
  $pb.PbMap<$core.String, $core.String> get env => $_getMap(5);

  @$pb.TagNumber(7)
  $1.SleeveMode get mode => $_getN(6);
  @$pb.TagNumber(7)
  set mode($1.SleeveMode value) => $_setField(7, value);
  @$pb.TagNumber(7)
  $core.bool hasMode() => $_has(6);
  @$pb.TagNumber(7)
  void clearMode() => $_clearField(7);

  /// supervised runs a chat sleeve's harness in its asking permission mode
  /// instead of autonomous: Claude --permission-mode default, Codex
  /// approvalPolicy untrusted with its workspace-write sandbox, Grok
  /// /always-approve off. pi has no gate and ignores it.
  @$pb.TagNumber(8)
  $core.bool get supervised => $_getBF(7);
  @$pb.TagNumber(8)
  set supervised($core.bool value) => $_setBool(7, value);
  @$pb.TagNumber(8)
  $core.bool hasSupervised() => $_has(7);
  @$pb.TagNumber(8)
  void clearSupervised() => $_clearField(8);

  /// Codex chat selection; omitted preserves native defaults. Other drivers reject it.
  @$pb.TagNumber(9)
  $1.ModelSelection get selection => $_getN(8);
  @$pb.TagNumber(9)
  set selection($1.ModelSelection value) => $_setField(9, value);
  @$pb.TagNumber(9)
  $core.bool hasSelection() => $_has(8);
  @$pb.TagNumber(9)
  void clearSelection() => $_clearField(9);
  @$pb.TagNumber(9)
  $1.ModelSelection ensureSelection() => $_ensure(8);
}

class SpawnSleeveResponse extends $pb.GeneratedMessage {
  factory SpawnSleeveResponse({
    Sleeve? sleeve,
  }) {
    final result = SpawnSleeveResponse._();
    if (sleeve != null) result.sleeve = sleeve;
    return result;
  }

  SpawnSleeveResponse._();

  factory SpawnSleeveResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      SpawnSleeveResponse()..mergeFromBuffer(data, registry);
  factory SpawnSleeveResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      SpawnSleeveResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'SpawnSleeveResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: SpawnSleeveResponse.$_createMessage)
    ..aOM<Sleeve>(1, _omitFieldNames ? '' : 'sleeve',
        subBuilder: Sleeve.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  SpawnSleeveResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  SpawnSleeveResponse copyWith(void Function(SpawnSleeveResponse) updates) =>
      super.copyWith((message) => updates(message as SpawnSleeveResponse))
          as SpawnSleeveResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core
      .Deprecated('Use SpawnSleeveResponse() / SpawnSleeveResponse.new instead')
  static SpawnSleeveResponse create() => SpawnSleeveResponse._();
  static $pb.GeneratedMessage $_createMessage() => SpawnSleeveResponse._();
  @$core.override
  SpawnSleeveResponse createEmptyInstance() => SpawnSleeveResponse._();
  @$core.pragma('dart2js:noInline')
  static SpawnSleeveResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<SpawnSleeveResponse>(
          SpawnSleeveResponse.$_createMessage);
  static SpawnSleeveResponse? _defaultInstance;

  @$pb.TagNumber(1)
  Sleeve get sleeve => $_getN(0);
  @$pb.TagNumber(1)
  set sleeve(Sleeve value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasSleeve() => $_has(0);
  @$pb.TagNumber(1)
  void clearSleeve() => $_clearField(1);
  @$pb.TagNumber(1)
  Sleeve ensureSleeve() => $_ensure(0);
}

class ListSleevesRequest extends $pb.GeneratedMessage {
  factory ListSleevesRequest() => ListSleevesRequest._();

  ListSleevesRequest._();

  factory ListSleevesRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListSleevesRequest()..mergeFromBuffer(data, registry);
  factory ListSleevesRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListSleevesRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ListSleevesRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ListSleevesRequest.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListSleevesRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListSleevesRequest copyWith(void Function(ListSleevesRequest) updates) =>
      super.copyWith((message) => updates(message as ListSleevesRequest))
          as ListSleevesRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use ListSleevesRequest() / ListSleevesRequest.new instead')
  static ListSleevesRequest create() => ListSleevesRequest._();
  static $pb.GeneratedMessage $_createMessage() => ListSleevesRequest._();
  @$core.override
  ListSleevesRequest createEmptyInstance() => ListSleevesRequest._();
  @$core.pragma('dart2js:noInline')
  static ListSleevesRequest getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<ListSleevesRequest>(
          ListSleevesRequest.$_createMessage);
  static ListSleevesRequest? _defaultInstance;
}

class ListSleevesResponse extends $pb.GeneratedMessage {
  factory ListSleevesResponse({
    $core.Iterable<Sleeve>? sleeves,
  }) {
    final result = ListSleevesResponse._();
    if (sleeves != null) result.sleeves.addAll(sleeves);
    return result;
  }

  ListSleevesResponse._();

  factory ListSleevesResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListSleevesResponse()..mergeFromBuffer(data, registry);
  factory ListSleevesResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListSleevesResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ListSleevesResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ListSleevesResponse.$_createMessage)
    ..pPM<Sleeve>(1, _omitFieldNames ? '' : 'sleeves',
        subBuilder: Sleeve.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListSleevesResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListSleevesResponse copyWith(void Function(ListSleevesResponse) updates) =>
      super.copyWith((message) => updates(message as ListSleevesResponse))
          as ListSleevesResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core
      .Deprecated('Use ListSleevesResponse() / ListSleevesResponse.new instead')
  static ListSleevesResponse create() => ListSleevesResponse._();
  static $pb.GeneratedMessage $_createMessage() => ListSleevesResponse._();
  @$core.override
  ListSleevesResponse createEmptyInstance() => ListSleevesResponse._();
  @$core.pragma('dart2js:noInline')
  static ListSleevesResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<ListSleevesResponse>(
          ListSleevesResponse.$_createMessage);
  static ListSleevesResponse? _defaultInstance;

  @$pb.TagNumber(1)
  $pb.PbList<Sleeve> get sleeves => $_getList(0);
}

class ListWorkspacesRequest extends $pb.GeneratedMessage {
  factory ListWorkspacesRequest() => ListWorkspacesRequest._();

  ListWorkspacesRequest._();

  factory ListWorkspacesRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListWorkspacesRequest()..mergeFromBuffer(data, registry);
  factory ListWorkspacesRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListWorkspacesRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ListWorkspacesRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ListWorkspacesRequest.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListWorkspacesRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListWorkspacesRequest copyWith(
          void Function(ListWorkspacesRequest) updates) =>
      super.copyWith((message) => updates(message as ListWorkspacesRequest))
          as ListWorkspacesRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use ListWorkspacesRequest() / ListWorkspacesRequest.new instead')
  static ListWorkspacesRequest create() => ListWorkspacesRequest._();
  static $pb.GeneratedMessage $_createMessage() => ListWorkspacesRequest._();
  @$core.override
  ListWorkspacesRequest createEmptyInstance() => ListWorkspacesRequest._();
  @$core.pragma('dart2js:noInline')
  static ListWorkspacesRequest getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<ListWorkspacesRequest>(
          ListWorkspacesRequest.$_createMessage);
  static ListWorkspacesRequest? _defaultInstance;
}

class Workspace extends $pb.GeneratedMessage {
  factory Workspace({
    $core.String? name,
    $core.String? path,
    $core.String? gitRemote,
    $core.String? gitBranch,
    $core.String? sleeve,
    $core.bool? busy,
    $core.String? requiredName,
  }) {
    final result = Workspace._();
    if (name != null) result.name = name;
    if (path != null) result.path = path;
    if (gitRemote != null) result.gitRemote = gitRemote;
    if (gitBranch != null) result.gitBranch = gitBranch;
    if (sleeve != null) result.sleeve = sleeve;
    if (busy != null) result.busy = busy;
    if (requiredName != null) result.requiredName = requiredName;
    return result;
  }

  Workspace._();

  factory Workspace.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      Workspace()..mergeFromBuffer(data, registry);
  factory Workspace.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      Workspace()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'Workspace',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: Workspace.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'name')
    ..aOS(2, _omitFieldNames ? '' : 'path')
    ..aOS(3, _omitFieldNames ? '' : 'gitRemote')
    ..aOS(4, _omitFieldNames ? '' : 'gitBranch')
    ..aOS(5, _omitFieldNames ? '' : 'sleeve')
    ..aOB(6, _omitFieldNames ? '' : 'busy')
    ..aOS(7, _omitFieldNames ? '' : 'requiredName')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  Workspace clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  Workspace copyWith(void Function(Workspace) updates) =>
      super.copyWith((message) => updates(message as Workspace)) as Workspace;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use Workspace() / Workspace.new instead')
  static Workspace create() => Workspace._();
  static $pb.GeneratedMessage $_createMessage() => Workspace._();
  @$core.override
  Workspace createEmptyInstance() => Workspace._();
  @$core.pragma('dart2js:noInline')
  static Workspace getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<Workspace>(Workspace.$_createMessage);
  static Workspace? _defaultInstance;

  /// name is the directory's base name.
  @$pb.TagNumber(1)
  $core.String get name => $_getSZ(0);
  @$pb.TagNumber(1)
  set name($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasName() => $_has(0);
  @$pb.TagNumber(1)
  void clearName() => $_clearField(1);

  /// path is the absolute, symlink-resolved directory.
  @$pb.TagNumber(2)
  $core.String get path => $_getSZ(1);
  @$pb.TagNumber(2)
  set path($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasPath() => $_has(1);
  @$pb.TagNumber(2)
  void clearPath() => $_clearField(2);

  /// git_remote is the URL of the origin remote of a git checkout.
  @$pb.TagNumber(3)
  $core.String get gitRemote => $_getSZ(2);
  @$pb.TagNumber(3)
  set gitRemote($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasGitRemote() => $_has(2);
  @$pb.TagNumber(3)
  void clearGitRemote() => $_clearField(3);

  /// git_branch is the checked-out branch of a git checkout; empty for a
  /// detached HEAD or a directory that is not a git checkout.
  @$pb.TagNumber(4)
  $core.String get gitBranch => $_getSZ(3);
  @$pb.TagNumber(4)
  set gitBranch($core.String value) => $_setString(3, value);
  @$pb.TagNumber(4)
  $core.bool hasGitBranch() => $_has(3);
  @$pb.TagNumber(4)
  void clearGitBranch() => $_clearField(4);

  /// sleeve is the sleeve that uses this workspace, running or exited.
  @$pb.TagNumber(5)
  $core.String get sleeve => $_getSZ(4);
  @$pb.TagNumber(5)
  set sleeve($core.String value) => $_setString(4, value);
  @$pb.TagNumber(5)
  $core.bool hasSleeve() => $_has(4);
  @$pb.TagNumber(5)
  void clearSleeve() => $_clearField(5);

  /// busy is true while that sleeve is running; SpawnSleeve then refuses it.
  @$pb.TagNumber(6)
  $core.bool get busy => $_getBF(5);
  @$pb.TagNumber(6)
  set busy($core.bool value) => $_setBool(5, value);
  @$pb.TagNumber(6)
  $core.bool hasBusy() => $_has(5);
  @$pb.TagNumber(6)
  void clearBusy() => $_clearField(6);

  /// required_name is set for a workspace in the hub's state directory: only a
  /// sleeve of exactly that name may use it. Other workspaces take any name.
  @$pb.TagNumber(7)
  $core.String get requiredName => $_getSZ(6);
  @$pb.TagNumber(7)
  set requiredName($core.String value) => $_setString(6, value);
  @$pb.TagNumber(7)
  $core.bool hasRequiredName() => $_has(6);
  @$pb.TagNumber(7)
  void clearRequiredName() => $_clearField(7);
}

class ListWorkspacesResponse extends $pb.GeneratedMessage {
  factory ListWorkspacesResponse({
    $core.Iterable<Workspace>? workspaces,
  }) {
    final result = ListWorkspacesResponse._();
    if (workspaces != null) result.workspaces.addAll(workspaces);
    return result;
  }

  ListWorkspacesResponse._();

  factory ListWorkspacesResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListWorkspacesResponse()..mergeFromBuffer(data, registry);
  factory ListWorkspacesResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListWorkspacesResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ListWorkspacesResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ListWorkspacesResponse.$_createMessage)
    ..pPM<Workspace>(1, _omitFieldNames ? '' : 'workspaces',
        subBuilder: Workspace.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListWorkspacesResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListWorkspacesResponse copyWith(
          void Function(ListWorkspacesResponse) updates) =>
      super.copyWith((message) => updates(message as ListWorkspacesResponse))
          as ListWorkspacesResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use ListWorkspacesResponse() / ListWorkspacesResponse.new instead')
  static ListWorkspacesResponse create() => ListWorkspacesResponse._();
  static $pb.GeneratedMessage $_createMessage() => ListWorkspacesResponse._();
  @$core.override
  ListWorkspacesResponse createEmptyInstance() => ListWorkspacesResponse._();
  @$core.pragma('dart2js:noInline')
  static ListWorkspacesResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<ListWorkspacesResponse>(
          ListWorkspacesResponse.$_createMessage);
  static ListWorkspacesResponse? _defaultInstance;

  @$pb.TagNumber(1)
  $pb.PbList<Workspace> get workspaces => $_getList(0);
}

class GetSleeveRequest extends $pb.GeneratedMessage {
  factory GetSleeveRequest({
    $core.String? name,
  }) {
    final result = GetSleeveRequest._();
    if (name != null) result.name = name;
    return result;
  }

  GetSleeveRequest._();

  factory GetSleeveRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      GetSleeveRequest()..mergeFromBuffer(data, registry);
  factory GetSleeveRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      GetSleeveRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'GetSleeveRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: GetSleeveRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'name')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  GetSleeveRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  GetSleeveRequest copyWith(void Function(GetSleeveRequest) updates) =>
      super.copyWith((message) => updates(message as GetSleeveRequest))
          as GetSleeveRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use GetSleeveRequest() / GetSleeveRequest.new instead')
  static GetSleeveRequest create() => GetSleeveRequest._();
  static $pb.GeneratedMessage $_createMessage() => GetSleeveRequest._();
  @$core.override
  GetSleeveRequest createEmptyInstance() => GetSleeveRequest._();
  @$core.pragma('dart2js:noInline')
  static GetSleeveRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<GetSleeveRequest>(
          GetSleeveRequest.$_createMessage);
  static GetSleeveRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get name => $_getSZ(0);
  @$pb.TagNumber(1)
  set name($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasName() => $_has(0);
  @$pb.TagNumber(1)
  void clearName() => $_clearField(1);
}

class GetSleeveResponse extends $pb.GeneratedMessage {
  factory GetSleeveResponse({
    Sleeve? sleeve,
  }) {
    final result = GetSleeveResponse._();
    if (sleeve != null) result.sleeve = sleeve;
    return result;
  }

  GetSleeveResponse._();

  factory GetSleeveResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      GetSleeveResponse()..mergeFromBuffer(data, registry);
  factory GetSleeveResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      GetSleeveResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'GetSleeveResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: GetSleeveResponse.$_createMessage)
    ..aOM<Sleeve>(1, _omitFieldNames ? '' : 'sleeve',
        subBuilder: Sleeve.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  GetSleeveResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  GetSleeveResponse copyWith(void Function(GetSleeveResponse) updates) =>
      super.copyWith((message) => updates(message as GetSleeveResponse))
          as GetSleeveResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use GetSleeveResponse() / GetSleeveResponse.new instead')
  static GetSleeveResponse create() => GetSleeveResponse._();
  static $pb.GeneratedMessage $_createMessage() => GetSleeveResponse._();
  @$core.override
  GetSleeveResponse createEmptyInstance() => GetSleeveResponse._();
  @$core.pragma('dart2js:noInline')
  static GetSleeveResponse getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<GetSleeveResponse>(
          GetSleeveResponse.$_createMessage);
  static GetSleeveResponse? _defaultInstance;

  @$pb.TagNumber(1)
  Sleeve get sleeve => $_getN(0);
  @$pb.TagNumber(1)
  set sleeve(Sleeve value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasSleeve() => $_has(0);
  @$pb.TagNumber(1)
  void clearSleeve() => $_clearField(1);
  @$pb.TagNumber(1)
  Sleeve ensureSleeve() => $_ensure(0);
}

class KillSleeveRequest extends $pb.GeneratedMessage {
  factory KillSleeveRequest({
    $core.String? name,
  }) {
    final result = KillSleeveRequest._();
    if (name != null) result.name = name;
    return result;
  }

  KillSleeveRequest._();

  factory KillSleeveRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      KillSleeveRequest()..mergeFromBuffer(data, registry);
  factory KillSleeveRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      KillSleeveRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'KillSleeveRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: KillSleeveRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'name')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  KillSleeveRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  KillSleeveRequest copyWith(void Function(KillSleeveRequest) updates) =>
      super.copyWith((message) => updates(message as KillSleeveRequest))
          as KillSleeveRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use KillSleeveRequest() / KillSleeveRequest.new instead')
  static KillSleeveRequest create() => KillSleeveRequest._();
  static $pb.GeneratedMessage $_createMessage() => KillSleeveRequest._();
  @$core.override
  KillSleeveRequest createEmptyInstance() => KillSleeveRequest._();
  @$core.pragma('dart2js:noInline')
  static KillSleeveRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<KillSleeveRequest>(
          KillSleeveRequest.$_createMessage);
  static KillSleeveRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get name => $_getSZ(0);
  @$pb.TagNumber(1)
  set name($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasName() => $_has(0);
  @$pb.TagNumber(1)
  void clearName() => $_clearField(1);
}

class KillSleeveResponse extends $pb.GeneratedMessage {
  factory KillSleeveResponse() => KillSleeveResponse._();

  KillSleeveResponse._();

  factory KillSleeveResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      KillSleeveResponse()..mergeFromBuffer(data, registry);
  factory KillSleeveResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      KillSleeveResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'KillSleeveResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: KillSleeveResponse.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  KillSleeveResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  KillSleeveResponse copyWith(void Function(KillSleeveResponse) updates) =>
      super.copyWith((message) => updates(message as KillSleeveResponse))
          as KillSleeveResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use KillSleeveResponse() / KillSleeveResponse.new instead')
  static KillSleeveResponse create() => KillSleeveResponse._();
  static $pb.GeneratedMessage $_createMessage() => KillSleeveResponse._();
  @$core.override
  KillSleeveResponse createEmptyInstance() => KillSleeveResponse._();
  @$core.pragma('dart2js:noInline')
  static KillSleeveResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<KillSleeveResponse>(
          KillSleeveResponse.$_createMessage);
  static KillSleeveResponse? _defaultInstance;
}

class ResleeveSleeveRequest extends $pb.GeneratedMessage {
  factory ResleeveSleeveRequest({
    $core.String? name,
    Harness? to,
    $core.bool? interrupt,
    $2.Duration? wait,
    $1.ModelSelection? selection,
    $core.bool? nativeDefaults,
  }) {
    final result = ResleeveSleeveRequest._();
    if (name != null) result.name = name;
    if (to != null) result.to = to;
    if (interrupt != null) result.interrupt = interrupt;
    if (wait != null) result.wait = wait;
    if (selection != null) result.selection = selection;
    if (nativeDefaults != null) result.nativeDefaults = nativeDefaults;
    return result;
  }

  ResleeveSleeveRequest._();

  factory ResleeveSleeveRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ResleeveSleeveRequest()..mergeFromBuffer(data, registry);
  factory ResleeveSleeveRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ResleeveSleeveRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ResleeveSleeveRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ResleeveSleeveRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'name')
    ..aE<Harness>(2, _omitFieldNames ? '' : 'to', enumValues: Harness.values)
    ..aOB(3, _omitFieldNames ? '' : 'interrupt')
    ..aOM<$2.Duration>(4, _omitFieldNames ? '' : 'wait',
        subBuilder: $2.Duration.$_createMessage)
    ..aOM<$1.ModelSelection>(5, _omitFieldNames ? '' : 'selection',
        subBuilder: $1.ModelSelection.$_createMessage)
    ..aOB(6, _omitFieldNames ? '' : 'nativeDefaults')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ResleeveSleeveRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ResleeveSleeveRequest copyWith(
          void Function(ResleeveSleeveRequest) updates) =>
      super.copyWith((message) => updates(message as ResleeveSleeveRequest))
          as ResleeveSleeveRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use ResleeveSleeveRequest() / ResleeveSleeveRequest.new instead')
  static ResleeveSleeveRequest create() => ResleeveSleeveRequest._();
  static $pb.GeneratedMessage $_createMessage() => ResleeveSleeveRequest._();
  @$core.override
  ResleeveSleeveRequest createEmptyInstance() => ResleeveSleeveRequest._();
  @$core.pragma('dart2js:noInline')
  static ResleeveSleeveRequest getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<ResleeveSleeveRequest>(
          ResleeveSleeveRequest.$_createMessage);
  static ResleeveSleeveRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get name => $_getSZ(0);
  @$pb.TagNumber(1)
  set name($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasName() => $_has(0);
  @$pb.TagNumber(1)
  void clearName() => $_clearField(1);

  /// to is the new harness. UNSPECIFIED means auto: the top recommendation
  /// of QuotaService.Suggest other than the current harness.
  @$pb.TagNumber(2)
  Harness get to => $_getN(1);
  @$pb.TagNumber(2)
  set to(Harness value) => $_setField(2, value);
  @$pb.TagNumber(2)
  $core.bool hasTo() => $_has(1);
  @$pb.TagNumber(2)
  void clearTo() => $_clearField(2);

  /// interrupt cancels a running turn at once instead of waiting for it.
  @$pb.TagNumber(3)
  $core.bool get interrupt => $_getBF(2);
  @$pb.TagNumber(3)
  set interrupt($core.bool value) => $_setBool(2, value);
  @$pb.TagNumber(3)
  $core.bool hasInterrupt() => $_has(2);
  @$pb.TagNumber(3)
  void clearInterrupt() => $_clearField(3);

  /// wait bounds how long a running turn may continue before it is
  /// interrupted. Default 5 minutes.
  @$pb.TagNumber(4)
  $2.Duration get wait => $_getN(3);
  @$pb.TagNumber(4)
  set wait($2.Duration value) => $_setField(4, value);
  @$pb.TagNumber(4)
  $core.bool hasWait() => $_has(3);
  @$pb.TagNumber(4)
  void clearWait() => $_clearField(4);
  @$pb.TagNumber(4)
  $2.Duration ensureWait() => $_ensure(3);

  @$pb.TagNumber(5)
  $1.ModelSelection get selection => $_getN(4);
  @$pb.TagNumber(5)
  set selection($1.ModelSelection value) => $_setField(5, value);
  @$pb.TagNumber(5)
  $core.bool hasSelection() => $_has(4);
  @$pb.TagNumber(5)
  void clearSelection() => $_clearField(5);
  @$pb.TagNumber(5)
  $1.ModelSelection ensureSelection() => $_ensure(4);

  /// Explicitly discard the outgoing selection when switching harnesses.
  @$pb.TagNumber(6)
  $core.bool get nativeDefaults => $_getBF(5);
  @$pb.TagNumber(6)
  set nativeDefaults($core.bool value) => $_setBool(5, value);
  @$pb.TagNumber(6)
  $core.bool hasNativeDefaults() => $_has(5);
  @$pb.TagNumber(6)
  void clearNativeDefaults() => $_clearField(6);
}

class ResleeveSleeveResponse extends $pb.GeneratedMessage {
  factory ResleeveSleeveResponse({
    Sleeve? sleeve,
  }) {
    final result = ResleeveSleeveResponse._();
    if (sleeve != null) result.sleeve = sleeve;
    return result;
  }

  ResleeveSleeveResponse._();

  factory ResleeveSleeveResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ResleeveSleeveResponse()..mergeFromBuffer(data, registry);
  factory ResleeveSleeveResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ResleeveSleeveResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ResleeveSleeveResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ResleeveSleeveResponse.$_createMessage)
    ..aOM<Sleeve>(1, _omitFieldNames ? '' : 'sleeve',
        subBuilder: Sleeve.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ResleeveSleeveResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ResleeveSleeveResponse copyWith(
          void Function(ResleeveSleeveResponse) updates) =>
      super.copyWith((message) => updates(message as ResleeveSleeveResponse))
          as ResleeveSleeveResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use ResleeveSleeveResponse() / ResleeveSleeveResponse.new instead')
  static ResleeveSleeveResponse create() => ResleeveSleeveResponse._();
  static $pb.GeneratedMessage $_createMessage() => ResleeveSleeveResponse._();
  @$core.override
  ResleeveSleeveResponse createEmptyInstance() => ResleeveSleeveResponse._();
  @$core.pragma('dart2js:noInline')
  static ResleeveSleeveResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<ResleeveSleeveResponse>(
          ResleeveSleeveResponse.$_createMessage);
  static ResleeveSleeveResponse? _defaultInstance;

  /// sleeve is the sleeve on its new harness; sleeve.handoff says how the
  /// handoff was made.
  @$pb.TagNumber(1)
  Sleeve get sleeve => $_getN(0);
  @$pb.TagNumber(1)
  set sleeve(Sleeve value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasSleeve() => $_has(0);
  @$pb.TagNumber(1)
  void clearSleeve() => $_clearField(1);
  @$pb.TagNumber(1)
  Sleeve ensureSleeve() => $_ensure(0);
}

class SpawnEnvoyRequest extends $pb.GeneratedMessage {
  factory SpawnEnvoyRequest({
    Harness? harness,
    $core.Iterable<$core.String>? scopes,
    $core.Iterable<$core.String>? addScopes,
    $2.Duration? ttl,
    $1.ModelSelection? selection,
  }) {
    final result = SpawnEnvoyRequest._();
    if (harness != null) result.harness = harness;
    if (scopes != null) result.scopes.addAll(scopes);
    if (addScopes != null) result.addScopes.addAll(addScopes);
    if (ttl != null) result.ttl = ttl;
    if (selection != null) result.selection = selection;
    return result;
  }

  SpawnEnvoyRequest._();

  factory SpawnEnvoyRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      SpawnEnvoyRequest()..mergeFromBuffer(data, registry);
  factory SpawnEnvoyRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      SpawnEnvoyRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'SpawnEnvoyRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: SpawnEnvoyRequest.$_createMessage)
    ..aE<Harness>(1, _omitFieldNames ? '' : 'harness',
        enumValues: Harness.values)
    ..pPS(2, _omitFieldNames ? '' : 'scopes')
    ..pPS(3, _omitFieldNames ? '' : 'addScopes')
    ..aOM<$2.Duration>(4, _omitFieldNames ? '' : 'ttl',
        subBuilder: $2.Duration.$_createMessage)
    ..aOM<$1.ModelSelection>(5, _omitFieldNames ? '' : 'selection',
        subBuilder: $1.ModelSelection.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  SpawnEnvoyRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  SpawnEnvoyRequest copyWith(void Function(SpawnEnvoyRequest) updates) =>
      super.copyWith((message) => updates(message as SpawnEnvoyRequest))
          as SpawnEnvoyRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use SpawnEnvoyRequest() / SpawnEnvoyRequest.new instead')
  static SpawnEnvoyRequest create() => SpawnEnvoyRequest._();
  static $pb.GeneratedMessage $_createMessage() => SpawnEnvoyRequest._();
  @$core.override
  SpawnEnvoyRequest createEmptyInstance() => SpawnEnvoyRequest._();
  @$core.pragma('dart2js:noInline')
  static SpawnEnvoyRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<SpawnEnvoyRequest>(
          SpawnEnvoyRequest.$_createMessage);
  static SpawnEnvoyRequest? _defaultInstance;

  /// harness is the harness to run; UNSPECIFIED picks the top recommendation
  /// of QuotaService.Suggest. HARNESS_SHELL is invalid.
  @$pb.TagNumber(1)
  Harness get harness => $_getN(0);
  @$pb.TagNumber(1)
  set harness(Harness value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasHarness() => $_has(0);
  @$pb.TagNumber(1)
  void clearHarness() => $_clearField(1);

  /// scopes replaces the default scopes (read, events and the non-destructive
  /// backlog, lease, message, sleeve spawn and resleeve tools) when not empty.
  @$pb.TagNumber(2)
  $pb.PbList<$core.String> get scopes => $_getList(1);

  /// add_scopes extends the scopes, defaults or replaced.
  @$pb.TagNumber(3)
  $pb.PbList<$core.String> get addScopes => $_getList(2);

  /// ttl is the token lifetime. Unset means 720h; zero never expires. The hub
  /// renews the token before it expires without restarting the harness.
  @$pb.TagNumber(4)
  $2.Duration get ttl => $_getN(3);
  @$pb.TagNumber(4)
  set ttl($2.Duration value) => $_setField(4, value);
  @$pb.TagNumber(4)
  $core.bool hasTtl() => $_has(3);
  @$pb.TagNumber(4)
  void clearTtl() => $_clearField(4);
  @$pb.TagNumber(4)
  $2.Duration ensureTtl() => $_ensure(3);

  @$pb.TagNumber(5)
  $1.ModelSelection get selection => $_getN(4);
  @$pb.TagNumber(5)
  set selection($1.ModelSelection value) => $_setField(5, value);
  @$pb.TagNumber(5)
  $core.bool hasSelection() => $_has(4);
  @$pb.TagNumber(5)
  void clearSelection() => $_clearField(5);
  @$pb.TagNumber(5)
  $1.ModelSelection ensureSelection() => $_ensure(4);
}

class SpawnEnvoyResponse extends $pb.GeneratedMessage {
  factory SpawnEnvoyResponse({
    Sleeve? sleeve,
    $3.TokenInfo? token,
    $core.String? workspace,
    $core.String? reason,
    $core.bool? replaced,
  }) {
    final result = SpawnEnvoyResponse._();
    if (sleeve != null) result.sleeve = sleeve;
    if (token != null) result.token = token;
    if (workspace != null) result.workspace = workspace;
    if (reason != null) result.reason = reason;
    if (replaced != null) result.replaced = replaced;
    return result;
  }

  SpawnEnvoyResponse._();

  factory SpawnEnvoyResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      SpawnEnvoyResponse()..mergeFromBuffer(data, registry);
  factory SpawnEnvoyResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      SpawnEnvoyResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'SpawnEnvoyResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: SpawnEnvoyResponse.$_createMessage)
    ..aOM<Sleeve>(1, _omitFieldNames ? '' : 'sleeve',
        subBuilder: Sleeve.$_createMessage)
    ..aOM<$3.TokenInfo>(2, _omitFieldNames ? '' : 'token',
        subBuilder: $3.TokenInfo.$_createMessage)
    ..aOS(3, _omitFieldNames ? '' : 'workspace')
    ..aOS(4, _omitFieldNames ? '' : 'reason')
    ..aOB(5, _omitFieldNames ? '' : 'replaced')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  SpawnEnvoyResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  SpawnEnvoyResponse copyWith(void Function(SpawnEnvoyResponse) updates) =>
      super.copyWith((message) => updates(message as SpawnEnvoyResponse))
          as SpawnEnvoyResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use SpawnEnvoyResponse() / SpawnEnvoyResponse.new instead')
  static SpawnEnvoyResponse create() => SpawnEnvoyResponse._();
  static $pb.GeneratedMessage $_createMessage() => SpawnEnvoyResponse._();
  @$core.override
  SpawnEnvoyResponse createEmptyInstance() => SpawnEnvoyResponse._();
  @$core.pragma('dart2js:noInline')
  static SpawnEnvoyResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<SpawnEnvoyResponse>(
          SpawnEnvoyResponse.$_createMessage);
  static SpawnEnvoyResponse? _defaultInstance;

  @$pb.TagNumber(1)
  Sleeve get sleeve => $_getN(0);
  @$pb.TagNumber(1)
  set sleeve(Sleeve value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasSleeve() => $_has(0);
  @$pb.TagNumber(1)
  void clearSleeve() => $_clearField(1);
  @$pb.TagNumber(1)
  Sleeve ensureSleeve() => $_ensure(0);

  /// token describes the minted token; the secret stays in the token file.
  @$pb.TagNumber(2)
  $3.TokenInfo get token => $_getN(1);
  @$pb.TagNumber(2)
  set token($3.TokenInfo value) => $_setField(2, value);
  @$pb.TagNumber(2)
  $core.bool hasToken() => $_has(1);
  @$pb.TagNumber(2)
  void clearToken() => $_clearField(2);
  @$pb.TagNumber(2)
  $3.TokenInfo ensureToken() => $_ensure(1);

  @$pb.TagNumber(3)
  $core.String get workspace => $_getSZ(2);
  @$pb.TagNumber(3)
  set workspace($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasWorkspace() => $_has(2);
  @$pb.TagNumber(3)
  void clearWorkspace() => $_clearField(3);

  /// reason says why the harness was chosen.
  @$pb.TagNumber(4)
  $core.String get reason => $_getSZ(3);
  @$pb.TagNumber(4)
  set reason($core.String value) => $_setString(3, value);
  @$pb.TagNumber(4)
  $core.bool hasReason() => $_has(3);
  @$pb.TagNumber(4)
  void clearReason() => $_clearField(4);

  /// replaced is true when a running Envoy was stopped first.
  @$pb.TagNumber(5)
  $core.bool get replaced => $_getBF(4);
  @$pb.TagNumber(5)
  set replaced($core.bool value) => $_setBool(4, value);
  @$pb.TagNumber(5)
  $core.bool hasReplaced() => $_has(4);
  @$pb.TagNumber(5)
  void clearReplaced() => $_clearField(5);
}

/// SleeveService manages sleeves on the hub machine: one workspace plus one
/// harness, run inside a bubblewrap sandbox in a systemd scope with CPU and
/// memory limits. In chat mode (the default for agent harnesses) the harness
/// runs its official headless protocol and is driven through ChatService. In
/// terminal mode it is a tmux session ("sleeve-<name>"), so CapturePane and the
/// WebSocket terminal work on it. The mode is fixed at spawn. Sleeves are Linux-only.
/// Changes are announced on EventService as "sleeve.spawned", "sleeve.exited"
/// and "sleeve.oom" (subject = sleeve name).
class SleeveServiceApi {
  final $pb.RpcClient _client;

  SleeveServiceApi(this._client);

  /// SpawnSleeve starts a harness in a new isolated sleeve on the hub. The
  /// workspace is an existing directory (bound read-write at the same path) or
  /// a git URL cloned into the hub's workspaces directory. Respawning the name
  /// of an exited sleeve reuses its home directory and workspace and replaces
  /// its kept session.
  $async.Future<SpawnSleeveResponse> spawnSleeve(
          $pb.ClientContext? ctx, SpawnSleeveRequest request) =>
      _client.invoke<SpawnSleeveResponse>(
          ctx, 'SleeveService', 'SpawnSleeve', request, SpawnSleeveResponse());

  /// ListSleeves returns every known sleeve, running or exited, with its latest
  /// resource usage.
  $async.Future<ListSleevesResponse> listSleeves(
          $pb.ClientContext? ctx, ListSleevesRequest request) =>
      _client.invoke<ListSleevesResponse>(
          ctx, 'SleeveService', 'ListSleeves', request, ListSleevesResponse());

  /// ListWorkspaces returns the directories SpawnSleeve would accept as an
  /// existing workspace: git checkouts and other directories under the hub's
  /// workspaces directory (cloned or created by earlier sleeves), and the
  /// workspaces of known sleeves elsewhere on disk. It never walks the rest of
  /// the filesystem; any other absolute path can still be given to
  /// SpawnSleeve directly.
  $async.Future<ListWorkspacesResponse> listWorkspaces(
          $pb.ClientContext? ctx, ListWorkspacesRequest request) =>
      _client.invoke<ListWorkspacesResponse>(ctx, 'SleeveService',
          'ListWorkspaces', request, ListWorkspacesResponse());

  /// GetSleeve returns one sleeve with its latest resource usage.
  $async.Future<GetSleeveResponse> getSleeve(
          $pb.ClientContext? ctx, GetSleeveRequest request) =>
      _client.invoke<GetSleeveResponse>(
          ctx, 'SleeveService', 'GetSleeve', request, GetSleeveResponse());

  /// KillSleeve stops a sleeve's harness and every process in its sandbox and
  /// forgets the sleeve. The workspace and the sleeve's home directory are kept
  /// on disk.
  $async.Future<KillSleeveResponse> killSleeve(
          $pb.ClientContext? ctx, KillSleeveRequest request) =>
      _client.invoke<KillSleeveResponse>(
          ctx, 'SleeveService', 'KillSleeve', request, KillSleeveResponse());

  /// ResleeveSleeve moves a chat sleeve to another harness on the same
  /// workspace, under the same name, without asking the outgoing harness
  /// anything. The hub takes the chat over (no other sender can start a
  /// turn), waits for it to be IDLE (interrupting a turn that runs past wait),
  /// composes facts in code (git status, diff --stat, recent commits, backlog
  /// and mail, the last user messages and the final assistant message from
  /// the outgoing harness's own transcript), has a separate model write a
  /// Goal/State/Unfinished/Next/Gotchas summary (the subscription with the
  /// most spare quota other than the outgoing harness, else pi with the local
  /// LLM), overwrites <workspace>/.protectorate/HANDOFF.md (kept out of git
  /// through .git/info/exclude, archived under the hub's state directory),
  /// replaces the harness and sends it one plain user message naming the
  /// handoff. The summary runs while the outgoing turn winds down. The old
  /// native session stays on disk for a later resume. If the new harness
  /// cannot start, the old one is respawned on its own session. Announced as
  /// "sleeve.resleeved"; a summary by the fallback model, or none, also emits
  /// "resleeve.warning".
  $async.Future<ResleeveSleeveResponse> resleeveSleeve(
          $pb.ClientContext? ctx, ResleeveSleeveRequest request) =>
      _client.invoke<ResleeveSleeveResponse>(ctx, 'SleeveService',
          'ResleeveSleeve', request, ResleeveSleeveResponse());

  /// SpawnEnvoy creates or respawns Envoy, the orchestrator: a chat sleeve
  /// named "envoy" in $PROTECTORATE_HOME/orchestrator with its own scoped
  /// token. It picks the harness (the top of QuotaService.Suggest unless one
  /// is given), stops a running Envoy, revokes every earlier orchestrator
  /// token, mints a new one, writes the token file and the generated context
  /// files (AGENTS.md, CLAUDE.md, hosts.md) and starts the harness. AGENTS.md
  /// is byte-identical across respawns unless the CLI, the tool catalog, the
  /// hub URL, or the scopes change, so a resumed session keeps its
  /// prompt-cache prefix. Only the owner token may call it, and the token it
  /// mints never holds it.
  $async.Future<SpawnEnvoyResponse> spawnEnvoy(
          $pb.ClientContext? ctx, SpawnEnvoyRequest request) =>
      _client.invoke<SpawnEnvoyResponse>(
          ctx, 'SleeveService', 'SpawnEnvoy', request, SpawnEnvoyResponse());
}

const $core.bool _omitFieldNames =
    $core.bool.fromEnvironment('protobuf.omit_field_names');
const $core.bool _omitMessageNames =
    $core.bool.fromEnvironment('protobuf.omit_message_names');
