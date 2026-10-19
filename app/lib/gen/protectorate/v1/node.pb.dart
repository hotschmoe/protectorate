// This is a generated file - do not edit.
//
// Generated from protectorate/v1/node.proto.

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

import 'job.pb.dart' as $3;
import 'node.pbenum.dart';
import 'sleeve.pb.dart' as $1;

export 'package:protobuf/protobuf.dart' show GeneratedMessageGenericExtensions;

export 'node.pbenum.dart';

class GetMetricsRequest extends $pb.GeneratedMessage {
  factory GetMetricsRequest() => GetMetricsRequest._();

  GetMetricsRequest._();

  factory GetMetricsRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      GetMetricsRequest()..mergeFromBuffer(data, registry);
  factory GetMetricsRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      GetMetricsRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'GetMetricsRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: GetMetricsRequest.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  GetMetricsRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  GetMetricsRequest copyWith(void Function(GetMetricsRequest) updates) =>
      super.copyWith((message) => updates(message as GetMetricsRequest))
          as GetMetricsRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use GetMetricsRequest() / GetMetricsRequest.new instead')
  static GetMetricsRequest create() => GetMetricsRequest._();
  static $pb.GeneratedMessage $_createMessage() => GetMetricsRequest._();
  @$core.override
  GetMetricsRequest createEmptyInstance() => GetMetricsRequest._();
  @$core.pragma('dart2js:noInline')
  static GetMetricsRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<GetMetricsRequest>(
          GetMetricsRequest.$_createMessage);
  static GetMetricsRequest? _defaultInstance;
}

class WatchMetricsRequest extends $pb.GeneratedMessage {
  factory WatchMetricsRequest({
    $core.int? intervalSeconds,
  }) {
    final result = WatchMetricsRequest._();
    if (intervalSeconds != null) result.intervalSeconds = intervalSeconds;
    return result;
  }

  WatchMetricsRequest._();

  factory WatchMetricsRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      WatchMetricsRequest()..mergeFromBuffer(data, registry);
  factory WatchMetricsRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      WatchMetricsRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'WatchMetricsRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: WatchMetricsRequest.$_createMessage)
    ..aI(1, _omitFieldNames ? '' : 'intervalSeconds')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  WatchMetricsRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  WatchMetricsRequest copyWith(void Function(WatchMetricsRequest) updates) =>
      super.copyWith((message) => updates(message as WatchMetricsRequest))
          as WatchMetricsRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core
      .Deprecated('Use WatchMetricsRequest() / WatchMetricsRequest.new instead')
  static WatchMetricsRequest create() => WatchMetricsRequest._();
  static $pb.GeneratedMessage $_createMessage() => WatchMetricsRequest._();
  @$core.override
  WatchMetricsRequest createEmptyInstance() => WatchMetricsRequest._();
  @$core.pragma('dart2js:noInline')
  static WatchMetricsRequest getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<WatchMetricsRequest>(
          WatchMetricsRequest.$_createMessage);
  static WatchMetricsRequest? _defaultInstance;

  /// interval_seconds is clamped to a minimum of 5; 0 selects 10.
  @$pb.TagNumber(1)
  $core.int get intervalSeconds => $_getIZ(0);
  @$pb.TagNumber(1)
  set intervalSeconds($core.int value) => $_setSignedInt32(0, value);
  @$pb.TagNumber(1)
  $core.bool hasIntervalSeconds() => $_has(0);
  @$pb.TagNumber(1)
  void clearIntervalSeconds() => $_clearField(1);
}

class NodeMetrics extends $pb.GeneratedMessage {
  factory NodeMetrics({
    $core.String? hostname,
    $core.String? os,
    $core.String? arch,
    $core.String? platform,
    $core.String? agentVersion,
    $fixnum.Int64? uptimeSeconds,
    $0.Timestamp? collectedAt,
    CpuMetrics? cpu,
    MemoryMetrics? memory,
    $core.Iterable<FilesystemMetrics>? filesystems,
    $core.Iterable<GpuMetrics>? gpus,
    ContainerEngine? engine,
    $core.Iterable<DirSize>? watchDirs,
    $core.Iterable<$core.String>? warnings,
    $core.Iterable<$1.Sleeve>? sleeves,
  }) {
    final result = NodeMetrics._();
    if (hostname != null) result.hostname = hostname;
    if (os != null) result.os = os;
    if (arch != null) result.arch = arch;
    if (platform != null) result.platform = platform;
    if (agentVersion != null) result.agentVersion = agentVersion;
    if (uptimeSeconds != null) result.uptimeSeconds = uptimeSeconds;
    if (collectedAt != null) result.collectedAt = collectedAt;
    if (cpu != null) result.cpu = cpu;
    if (memory != null) result.memory = memory;
    if (filesystems != null) result.filesystems.addAll(filesystems);
    if (gpus != null) result.gpus.addAll(gpus);
    if (engine != null) result.engine = engine;
    if (watchDirs != null) result.watchDirs.addAll(watchDirs);
    if (warnings != null) result.warnings.addAll(warnings);
    if (sleeves != null) result.sleeves.addAll(sleeves);
    return result;
  }

  NodeMetrics._();

  factory NodeMetrics.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      NodeMetrics()..mergeFromBuffer(data, registry);
  factory NodeMetrics.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      NodeMetrics()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'NodeMetrics',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: NodeMetrics.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'hostname')
    ..aOS(2, _omitFieldNames ? '' : 'os')
    ..aOS(3, _omitFieldNames ? '' : 'arch')
    ..aOS(4, _omitFieldNames ? '' : 'platform')
    ..aOS(5, _omitFieldNames ? '' : 'agentVersion')
    ..aInt64(6, _omitFieldNames ? '' : 'uptimeSeconds')
    ..aOM<$0.Timestamp>(7, _omitFieldNames ? '' : 'collectedAt',
        subBuilder: $0.Timestamp.$_createMessage)
    ..aOM<CpuMetrics>(8, _omitFieldNames ? '' : 'cpu',
        subBuilder: CpuMetrics.$_createMessage)
    ..aOM<MemoryMetrics>(9, _omitFieldNames ? '' : 'memory',
        subBuilder: MemoryMetrics.$_createMessage)
    ..pPM<FilesystemMetrics>(10, _omitFieldNames ? '' : 'filesystems',
        subBuilder: FilesystemMetrics.$_createMessage)
    ..pPM<GpuMetrics>(11, _omitFieldNames ? '' : 'gpus',
        subBuilder: GpuMetrics.$_createMessage)
    ..aOM<ContainerEngine>(12, _omitFieldNames ? '' : 'engine',
        subBuilder: ContainerEngine.$_createMessage)
    ..pPM<DirSize>(13, _omitFieldNames ? '' : 'watchDirs',
        subBuilder: DirSize.$_createMessage)
    ..pPS(14, _omitFieldNames ? '' : 'warnings')
    ..pPM<$1.Sleeve>(15, _omitFieldNames ? '' : 'sleeves',
        subBuilder: $1.Sleeve.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  NodeMetrics clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  NodeMetrics copyWith(void Function(NodeMetrics) updates) =>
      super.copyWith((message) => updates(message as NodeMetrics))
          as NodeMetrics;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use NodeMetrics() / NodeMetrics.new instead')
  static NodeMetrics create() => NodeMetrics._();
  static $pb.GeneratedMessage $_createMessage() => NodeMetrics._();
  @$core.override
  NodeMetrics createEmptyInstance() => NodeMetrics._();
  @$core.pragma('dart2js:noInline')
  static NodeMetrics getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<NodeMetrics>(
          NodeMetrics.$_createMessage);
  static NodeMetrics? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get hostname => $_getSZ(0);
  @$pb.TagNumber(1)
  set hostname($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasHostname() => $_has(0);
  @$pb.TagNumber(1)
  void clearHostname() => $_clearField(1);

  @$pb.TagNumber(2)
  $core.String get os => $_getSZ(1);
  @$pb.TagNumber(2)
  set os($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasOs() => $_has(1);
  @$pb.TagNumber(2)
  void clearOs() => $_clearField(2);

  @$pb.TagNumber(3)
  $core.String get arch => $_getSZ(2);
  @$pb.TagNumber(3)
  set arch($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasArch() => $_has(2);
  @$pb.TagNumber(3)
  void clearArch() => $_clearField(3);

  /// platform is a human description such as "ubuntu 24.04" or "Windows 11".
  @$pb.TagNumber(4)
  $core.String get platform => $_getSZ(3);
  @$pb.TagNumber(4)
  set platform($core.String value) => $_setString(3, value);
  @$pb.TagNumber(4)
  $core.bool hasPlatform() => $_has(3);
  @$pb.TagNumber(4)
  void clearPlatform() => $_clearField(4);

  @$pb.TagNumber(5)
  $core.String get agentVersion => $_getSZ(4);
  @$pb.TagNumber(5)
  set agentVersion($core.String value) => $_setString(4, value);
  @$pb.TagNumber(5)
  $core.bool hasAgentVersion() => $_has(4);
  @$pb.TagNumber(5)
  void clearAgentVersion() => $_clearField(5);

  @$pb.TagNumber(6)
  $fixnum.Int64 get uptimeSeconds => $_getI64(5);
  @$pb.TagNumber(6)
  set uptimeSeconds($fixnum.Int64 value) => $_setInt64(5, value);
  @$pb.TagNumber(6)
  $core.bool hasUptimeSeconds() => $_has(5);
  @$pb.TagNumber(6)
  void clearUptimeSeconds() => $_clearField(6);

  @$pb.TagNumber(7)
  $0.Timestamp get collectedAt => $_getN(6);
  @$pb.TagNumber(7)
  set collectedAt($0.Timestamp value) => $_setField(7, value);
  @$pb.TagNumber(7)
  $core.bool hasCollectedAt() => $_has(6);
  @$pb.TagNumber(7)
  void clearCollectedAt() => $_clearField(7);
  @$pb.TagNumber(7)
  $0.Timestamp ensureCollectedAt() => $_ensure(6);

  @$pb.TagNumber(8)
  CpuMetrics get cpu => $_getN(7);
  @$pb.TagNumber(8)
  set cpu(CpuMetrics value) => $_setField(8, value);
  @$pb.TagNumber(8)
  $core.bool hasCpu() => $_has(7);
  @$pb.TagNumber(8)
  void clearCpu() => $_clearField(8);
  @$pb.TagNumber(8)
  CpuMetrics ensureCpu() => $_ensure(7);

  @$pb.TagNumber(9)
  MemoryMetrics get memory => $_getN(8);
  @$pb.TagNumber(9)
  set memory(MemoryMetrics value) => $_setField(9, value);
  @$pb.TagNumber(9)
  $core.bool hasMemory() => $_has(8);
  @$pb.TagNumber(9)
  void clearMemory() => $_clearField(9);
  @$pb.TagNumber(9)
  MemoryMetrics ensureMemory() => $_ensure(8);

  @$pb.TagNumber(10)
  $pb.PbList<FilesystemMetrics> get filesystems => $_getList(9);

  /// gpus is empty when the node has none or no vendor tool is present.
  @$pb.TagNumber(11)
  $pb.PbList<GpuMetrics> get gpus => $_getList(10);

  /// engine is unset when no container engine is reachable.
  @$pb.TagNumber(12)
  ContainerEngine get engine => $_getN(11);
  @$pb.TagNumber(12)
  set engine(ContainerEngine value) => $_setField(12, value);
  @$pb.TagNumber(12)
  $core.bool hasEngine() => $_has(11);
  @$pb.TagNumber(12)
  void clearEngine() => $_clearField(12);
  @$pb.TagNumber(12)
  ContainerEngine ensureEngine() => $_ensure(11);

  @$pb.TagNumber(13)
  $pb.PbList<DirSize> get watchDirs => $_getList(12);

  /// warnings lists sections that could not be collected on this node.
  @$pb.TagNumber(14)
  $pb.PbList<$core.String> get warnings => $_getList(13);

  /// sleeves lists the sleeves running on this node with their usage; only the
  /// hub's own node has any.
  @$pb.TagNumber(15)
  $pb.PbList<$1.Sleeve> get sleeves => $_getList(14);
}

class CpuMetrics extends $pb.GeneratedMessage {
  factory CpuMetrics({
    $core.int? logicalCores,
    $core.double? usagePercent,
    $core.double? load1,
    $core.double? load5,
    $core.double? load15,
  }) {
    final result = CpuMetrics._();
    if (logicalCores != null) result.logicalCores = logicalCores;
    if (usagePercent != null) result.usagePercent = usagePercent;
    if (load1 != null) result.load1 = load1;
    if (load5 != null) result.load5 = load5;
    if (load15 != null) result.load15 = load15;
    return result;
  }

  CpuMetrics._();

  factory CpuMetrics.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      CpuMetrics()..mergeFromBuffer(data, registry);
  factory CpuMetrics.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      CpuMetrics()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'CpuMetrics',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: CpuMetrics.$_createMessage)
    ..aI(1, _omitFieldNames ? '' : 'logicalCores')
    ..aD(2, _omitFieldNames ? '' : 'usagePercent')
    ..aD(3, _omitFieldNames ? '' : 'load1')
    ..aD(4, _omitFieldNames ? '' : 'load5')
    ..aD(5, _omitFieldNames ? '' : 'load15')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  CpuMetrics clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  CpuMetrics copyWith(void Function(CpuMetrics) updates) =>
      super.copyWith((message) => updates(message as CpuMetrics)) as CpuMetrics;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use CpuMetrics() / CpuMetrics.new instead')
  static CpuMetrics create() => CpuMetrics._();
  static $pb.GeneratedMessage $_createMessage() => CpuMetrics._();
  @$core.override
  CpuMetrics createEmptyInstance() => CpuMetrics._();
  @$core.pragma('dart2js:noInline')
  static CpuMetrics getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<CpuMetrics>(CpuMetrics.$_createMessage);
  static CpuMetrics? _defaultInstance;

  @$pb.TagNumber(1)
  $core.int get logicalCores => $_getIZ(0);
  @$pb.TagNumber(1)
  set logicalCores($core.int value) => $_setSignedInt32(0, value);
  @$pb.TagNumber(1)
  $core.bool hasLogicalCores() => $_has(0);
  @$pb.TagNumber(1)
  void clearLogicalCores() => $_clearField(1);

  @$pb.TagNumber(2)
  $core.double get usagePercent => $_getN(1);
  @$pb.TagNumber(2)
  set usagePercent($core.double value) => $_setDouble(1, value);
  @$pb.TagNumber(2)
  $core.bool hasUsagePercent() => $_has(1);
  @$pb.TagNumber(2)
  void clearUsagePercent() => $_clearField(2);

  /// load averages are unset where the OS has none (Windows).
  @$pb.TagNumber(3)
  $core.double get load1 => $_getN(2);
  @$pb.TagNumber(3)
  set load1($core.double value) => $_setDouble(2, value);
  @$pb.TagNumber(3)
  $core.bool hasLoad1() => $_has(2);
  @$pb.TagNumber(3)
  void clearLoad1() => $_clearField(3);

  @$pb.TagNumber(4)
  $core.double get load5 => $_getN(3);
  @$pb.TagNumber(4)
  set load5($core.double value) => $_setDouble(3, value);
  @$pb.TagNumber(4)
  $core.bool hasLoad5() => $_has(3);
  @$pb.TagNumber(4)
  void clearLoad5() => $_clearField(4);

  @$pb.TagNumber(5)
  $core.double get load15 => $_getN(4);
  @$pb.TagNumber(5)
  set load15($core.double value) => $_setDouble(4, value);
  @$pb.TagNumber(5)
  $core.bool hasLoad15() => $_has(4);
  @$pb.TagNumber(5)
  void clearLoad15() => $_clearField(5);
}

class MemoryMetrics extends $pb.GeneratedMessage {
  factory MemoryMetrics({
    $fixnum.Int64? totalBytes,
    $fixnum.Int64? usedBytes,
    $fixnum.Int64? availableBytes,
    $core.double? usedPercent,
    $fixnum.Int64? swapTotalBytes,
    $fixnum.Int64? swapUsedBytes,
    $core.double? swapUsedPercent,
  }) {
    final result = MemoryMetrics._();
    if (totalBytes != null) result.totalBytes = totalBytes;
    if (usedBytes != null) result.usedBytes = usedBytes;
    if (availableBytes != null) result.availableBytes = availableBytes;
    if (usedPercent != null) result.usedPercent = usedPercent;
    if (swapTotalBytes != null) result.swapTotalBytes = swapTotalBytes;
    if (swapUsedBytes != null) result.swapUsedBytes = swapUsedBytes;
    if (swapUsedPercent != null) result.swapUsedPercent = swapUsedPercent;
    return result;
  }

  MemoryMetrics._();

  factory MemoryMetrics.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      MemoryMetrics()..mergeFromBuffer(data, registry);
  factory MemoryMetrics.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      MemoryMetrics()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'MemoryMetrics',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: MemoryMetrics.$_createMessage)
    ..a<$fixnum.Int64>(
        1, _omitFieldNames ? '' : 'totalBytes', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..a<$fixnum.Int64>(
        2, _omitFieldNames ? '' : 'usedBytes', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..a<$fixnum.Int64>(
        3, _omitFieldNames ? '' : 'availableBytes', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..aD(4, _omitFieldNames ? '' : 'usedPercent')
    ..a<$fixnum.Int64>(
        5, _omitFieldNames ? '' : 'swapTotalBytes', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..a<$fixnum.Int64>(
        6, _omitFieldNames ? '' : 'swapUsedBytes', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..aD(7, _omitFieldNames ? '' : 'swapUsedPercent')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  MemoryMetrics clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  MemoryMetrics copyWith(void Function(MemoryMetrics) updates) =>
      super.copyWith((message) => updates(message as MemoryMetrics))
          as MemoryMetrics;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use MemoryMetrics() / MemoryMetrics.new instead')
  static MemoryMetrics create() => MemoryMetrics._();
  static $pb.GeneratedMessage $_createMessage() => MemoryMetrics._();
  @$core.override
  MemoryMetrics createEmptyInstance() => MemoryMetrics._();
  @$core.pragma('dart2js:noInline')
  static MemoryMetrics getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<MemoryMetrics>(
          MemoryMetrics.$_createMessage);
  static MemoryMetrics? _defaultInstance;

  @$pb.TagNumber(1)
  $fixnum.Int64 get totalBytes => $_getI64(0);
  @$pb.TagNumber(1)
  set totalBytes($fixnum.Int64 value) => $_setInt64(0, value);
  @$pb.TagNumber(1)
  $core.bool hasTotalBytes() => $_has(0);
  @$pb.TagNumber(1)
  void clearTotalBytes() => $_clearField(1);

  @$pb.TagNumber(2)
  $fixnum.Int64 get usedBytes => $_getI64(1);
  @$pb.TagNumber(2)
  set usedBytes($fixnum.Int64 value) => $_setInt64(1, value);
  @$pb.TagNumber(2)
  $core.bool hasUsedBytes() => $_has(1);
  @$pb.TagNumber(2)
  void clearUsedBytes() => $_clearField(2);

  @$pb.TagNumber(3)
  $fixnum.Int64 get availableBytes => $_getI64(2);
  @$pb.TagNumber(3)
  set availableBytes($fixnum.Int64 value) => $_setInt64(2, value);
  @$pb.TagNumber(3)
  $core.bool hasAvailableBytes() => $_has(2);
  @$pb.TagNumber(3)
  void clearAvailableBytes() => $_clearField(3);

  @$pb.TagNumber(4)
  $core.double get usedPercent => $_getN(3);
  @$pb.TagNumber(4)
  set usedPercent($core.double value) => $_setDouble(3, value);
  @$pb.TagNumber(4)
  $core.bool hasUsedPercent() => $_has(3);
  @$pb.TagNumber(4)
  void clearUsedPercent() => $_clearField(4);

  @$pb.TagNumber(5)
  $fixnum.Int64 get swapTotalBytes => $_getI64(4);
  @$pb.TagNumber(5)
  set swapTotalBytes($fixnum.Int64 value) => $_setInt64(4, value);
  @$pb.TagNumber(5)
  $core.bool hasSwapTotalBytes() => $_has(4);
  @$pb.TagNumber(5)
  void clearSwapTotalBytes() => $_clearField(5);

  @$pb.TagNumber(6)
  $fixnum.Int64 get swapUsedBytes => $_getI64(5);
  @$pb.TagNumber(6)
  set swapUsedBytes($fixnum.Int64 value) => $_setInt64(5, value);
  @$pb.TagNumber(6)
  $core.bool hasSwapUsedBytes() => $_has(5);
  @$pb.TagNumber(6)
  void clearSwapUsedBytes() => $_clearField(6);

  @$pb.TagNumber(7)
  $core.double get swapUsedPercent => $_getN(6);
  @$pb.TagNumber(7)
  set swapUsedPercent($core.double value) => $_setDouble(6, value);
  @$pb.TagNumber(7)
  $core.bool hasSwapUsedPercent() => $_has(6);
  @$pb.TagNumber(7)
  void clearSwapUsedPercent() => $_clearField(7);
}

class FilesystemMetrics extends $pb.GeneratedMessage {
  factory FilesystemMetrics({
    $core.String? mount,
    $core.String? device,
    $core.String? fstype,
    $fixnum.Int64? totalBytes,
    $fixnum.Int64? usedBytes,
    $fixnum.Int64? freeBytes,
    $core.double? usedPercent,
  }) {
    final result = FilesystemMetrics._();
    if (mount != null) result.mount = mount;
    if (device != null) result.device = device;
    if (fstype != null) result.fstype = fstype;
    if (totalBytes != null) result.totalBytes = totalBytes;
    if (usedBytes != null) result.usedBytes = usedBytes;
    if (freeBytes != null) result.freeBytes = freeBytes;
    if (usedPercent != null) result.usedPercent = usedPercent;
    return result;
  }

  FilesystemMetrics._();

  factory FilesystemMetrics.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      FilesystemMetrics()..mergeFromBuffer(data, registry);
  factory FilesystemMetrics.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      FilesystemMetrics()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'FilesystemMetrics',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: FilesystemMetrics.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'mount')
    ..aOS(2, _omitFieldNames ? '' : 'device')
    ..aOS(3, _omitFieldNames ? '' : 'fstype')
    ..a<$fixnum.Int64>(
        4, _omitFieldNames ? '' : 'totalBytes', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..a<$fixnum.Int64>(
        5, _omitFieldNames ? '' : 'usedBytes', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..a<$fixnum.Int64>(
        6, _omitFieldNames ? '' : 'freeBytes', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..aD(7, _omitFieldNames ? '' : 'usedPercent')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  FilesystemMetrics clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  FilesystemMetrics copyWith(void Function(FilesystemMetrics) updates) =>
      super.copyWith((message) => updates(message as FilesystemMetrics))
          as FilesystemMetrics;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use FilesystemMetrics() / FilesystemMetrics.new instead')
  static FilesystemMetrics create() => FilesystemMetrics._();
  static $pb.GeneratedMessage $_createMessage() => FilesystemMetrics._();
  @$core.override
  FilesystemMetrics createEmptyInstance() => FilesystemMetrics._();
  @$core.pragma('dart2js:noInline')
  static FilesystemMetrics getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<FilesystemMetrics>(
          FilesystemMetrics.$_createMessage);
  static FilesystemMetrics? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get mount => $_getSZ(0);
  @$pb.TagNumber(1)
  set mount($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasMount() => $_has(0);
  @$pb.TagNumber(1)
  void clearMount() => $_clearField(1);

  @$pb.TagNumber(2)
  $core.String get device => $_getSZ(1);
  @$pb.TagNumber(2)
  set device($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasDevice() => $_has(1);
  @$pb.TagNumber(2)
  void clearDevice() => $_clearField(2);

  @$pb.TagNumber(3)
  $core.String get fstype => $_getSZ(2);
  @$pb.TagNumber(3)
  set fstype($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasFstype() => $_has(2);
  @$pb.TagNumber(3)
  void clearFstype() => $_clearField(3);

  @$pb.TagNumber(4)
  $fixnum.Int64 get totalBytes => $_getI64(3);
  @$pb.TagNumber(4)
  set totalBytes($fixnum.Int64 value) => $_setInt64(3, value);
  @$pb.TagNumber(4)
  $core.bool hasTotalBytes() => $_has(3);
  @$pb.TagNumber(4)
  void clearTotalBytes() => $_clearField(4);

  @$pb.TagNumber(5)
  $fixnum.Int64 get usedBytes => $_getI64(4);
  @$pb.TagNumber(5)
  set usedBytes($fixnum.Int64 value) => $_setInt64(4, value);
  @$pb.TagNumber(5)
  $core.bool hasUsedBytes() => $_has(4);
  @$pb.TagNumber(5)
  void clearUsedBytes() => $_clearField(5);

  @$pb.TagNumber(6)
  $fixnum.Int64 get freeBytes => $_getI64(5);
  @$pb.TagNumber(6)
  set freeBytes($fixnum.Int64 value) => $_setInt64(5, value);
  @$pb.TagNumber(6)
  $core.bool hasFreeBytes() => $_has(5);
  @$pb.TagNumber(6)
  void clearFreeBytes() => $_clearField(6);

  @$pb.TagNumber(7)
  $core.double get usedPercent => $_getN(6);
  @$pb.TagNumber(7)
  set usedPercent($core.double value) => $_setDouble(6, value);
  @$pb.TagNumber(7)
  $core.bool hasUsedPercent() => $_has(6);
  @$pb.TagNumber(7)
  void clearUsedPercent() => $_clearField(7);
}

class GpuMetrics extends $pb.GeneratedMessage {
  factory GpuMetrics({
    $core.int? index,
    $core.String? vendor,
    $core.String? name,
    $core.double? utilizationPercent,
    $fixnum.Int64? memoryUsedBytes,
    $fixnum.Int64? memoryTotalBytes,
    $core.double? temperatureCelsius,
  }) {
    final result = GpuMetrics._();
    if (index != null) result.index = index;
    if (vendor != null) result.vendor = vendor;
    if (name != null) result.name = name;
    if (utilizationPercent != null)
      result.utilizationPercent = utilizationPercent;
    if (memoryUsedBytes != null) result.memoryUsedBytes = memoryUsedBytes;
    if (memoryTotalBytes != null) result.memoryTotalBytes = memoryTotalBytes;
    if (temperatureCelsius != null)
      result.temperatureCelsius = temperatureCelsius;
    return result;
  }

  GpuMetrics._();

  factory GpuMetrics.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      GpuMetrics()..mergeFromBuffer(data, registry);
  factory GpuMetrics.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      GpuMetrics()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'GpuMetrics',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: GpuMetrics.$_createMessage)
    ..aI(1, _omitFieldNames ? '' : 'index')
    ..aOS(2, _omitFieldNames ? '' : 'vendor')
    ..aOS(3, _omitFieldNames ? '' : 'name')
    ..aD(4, _omitFieldNames ? '' : 'utilizationPercent')
    ..a<$fixnum.Int64>(
        5, _omitFieldNames ? '' : 'memoryUsedBytes', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..a<$fixnum.Int64>(
        6, _omitFieldNames ? '' : 'memoryTotalBytes', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..aD(7, _omitFieldNames ? '' : 'temperatureCelsius')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  GpuMetrics clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  GpuMetrics copyWith(void Function(GpuMetrics) updates) =>
      super.copyWith((message) => updates(message as GpuMetrics)) as GpuMetrics;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use GpuMetrics() / GpuMetrics.new instead')
  static GpuMetrics create() => GpuMetrics._();
  static $pb.GeneratedMessage $_createMessage() => GpuMetrics._();
  @$core.override
  GpuMetrics createEmptyInstance() => GpuMetrics._();
  @$core.pragma('dart2js:noInline')
  static GpuMetrics getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<GpuMetrics>(GpuMetrics.$_createMessage);
  static GpuMetrics? _defaultInstance;

  @$pb.TagNumber(1)
  $core.int get index => $_getIZ(0);
  @$pb.TagNumber(1)
  set index($core.int value) => $_setSignedInt32(0, value);
  @$pb.TagNumber(1)
  $core.bool hasIndex() => $_has(0);
  @$pb.TagNumber(1)
  void clearIndex() => $_clearField(1);

  @$pb.TagNumber(2)
  $core.String get vendor => $_getSZ(1);
  @$pb.TagNumber(2)
  set vendor($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasVendor() => $_has(1);
  @$pb.TagNumber(2)
  void clearVendor() => $_clearField(2);

  @$pb.TagNumber(3)
  $core.String get name => $_getSZ(2);
  @$pb.TagNumber(3)
  set name($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasName() => $_has(2);
  @$pb.TagNumber(3)
  void clearName() => $_clearField(3);

  /// utilization_percent is -1 when the device does not report it.
  @$pb.TagNumber(4)
  $core.double get utilizationPercent => $_getN(3);
  @$pb.TagNumber(4)
  set utilizationPercent($core.double value) => $_setDouble(3, value);
  @$pb.TagNumber(4)
  $core.bool hasUtilizationPercent() => $_has(3);
  @$pb.TagNumber(4)
  void clearUtilizationPercent() => $_clearField(4);

  @$pb.TagNumber(5)
  $fixnum.Int64 get memoryUsedBytes => $_getI64(4);
  @$pb.TagNumber(5)
  set memoryUsedBytes($fixnum.Int64 value) => $_setInt64(4, value);
  @$pb.TagNumber(5)
  $core.bool hasMemoryUsedBytes() => $_has(4);
  @$pb.TagNumber(5)
  void clearMemoryUsedBytes() => $_clearField(5);

  @$pb.TagNumber(6)
  $fixnum.Int64 get memoryTotalBytes => $_getI64(5);
  @$pb.TagNumber(6)
  set memoryTotalBytes($fixnum.Int64 value) => $_setInt64(5, value);
  @$pb.TagNumber(6)
  $core.bool hasMemoryTotalBytes() => $_has(5);
  @$pb.TagNumber(6)
  void clearMemoryTotalBytes() => $_clearField(6);

  /// temperature_celsius is -1 when the device does not report it.
  @$pb.TagNumber(7)
  $core.double get temperatureCelsius => $_getN(6);
  @$pb.TagNumber(7)
  set temperatureCelsius($core.double value) => $_setDouble(6, value);
  @$pb.TagNumber(7)
  $core.bool hasTemperatureCelsius() => $_has(6);
  @$pb.TagNumber(7)
  void clearTemperatureCelsius() => $_clearField(7);
}

class ContainerEngine extends $pb.GeneratedMessage {
  factory ContainerEngine({
    $core.String? kind,
    $core.String? version,
    EngineDiskUsage? disk,
    $0.Timestamp? diskMeasuredAt,
    $core.Iterable<ContainerMetrics>? containers,
  }) {
    final result = ContainerEngine._();
    if (kind != null) result.kind = kind;
    if (version != null) result.version = version;
    if (disk != null) result.disk = disk;
    if (diskMeasuredAt != null) result.diskMeasuredAt = diskMeasuredAt;
    if (containers != null) result.containers.addAll(containers);
    return result;
  }

  ContainerEngine._();

  factory ContainerEngine.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ContainerEngine()..mergeFromBuffer(data, registry);
  factory ContainerEngine.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ContainerEngine()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ContainerEngine',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ContainerEngine.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'kind')
    ..aOS(2, _omitFieldNames ? '' : 'version')
    ..aOM<EngineDiskUsage>(3, _omitFieldNames ? '' : 'disk',
        subBuilder: EngineDiskUsage.$_createMessage)
    ..aOM<$0.Timestamp>(4, _omitFieldNames ? '' : 'diskMeasuredAt',
        subBuilder: $0.Timestamp.$_createMessage)
    ..pPM<ContainerMetrics>(5, _omitFieldNames ? '' : 'containers',
        subBuilder: ContainerMetrics.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ContainerEngine clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ContainerEngine copyWith(void Function(ContainerEngine) updates) =>
      super.copyWith((message) => updates(message as ContainerEngine))
          as ContainerEngine;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use ContainerEngine() / ContainerEngine.new instead')
  static ContainerEngine create() => ContainerEngine._();
  static $pb.GeneratedMessage $_createMessage() => ContainerEngine._();
  @$core.override
  ContainerEngine createEmptyInstance() => ContainerEngine._();
  @$core.pragma('dart2js:noInline')
  static ContainerEngine getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<ContainerEngine>(
          ContainerEngine.$_createMessage);
  static ContainerEngine? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get kind => $_getSZ(0);
  @$pb.TagNumber(1)
  set kind($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasKind() => $_has(0);
  @$pb.TagNumber(1)
  void clearKind() => $_clearField(1);

  @$pb.TagNumber(2)
  $core.String get version => $_getSZ(1);
  @$pb.TagNumber(2)
  set version($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasVersion() => $_has(1);
  @$pb.TagNumber(2)
  void clearVersion() => $_clearField(2);

  /// disk is unset until the first slow-interval measurement completes.
  @$pb.TagNumber(3)
  EngineDiskUsage get disk => $_getN(2);
  @$pb.TagNumber(3)
  set disk(EngineDiskUsage value) => $_setField(3, value);
  @$pb.TagNumber(3)
  $core.bool hasDisk() => $_has(2);
  @$pb.TagNumber(3)
  void clearDisk() => $_clearField(3);
  @$pb.TagNumber(3)
  EngineDiskUsage ensureDisk() => $_ensure(2);

  @$pb.TagNumber(4)
  $0.Timestamp get diskMeasuredAt => $_getN(3);
  @$pb.TagNumber(4)
  set diskMeasuredAt($0.Timestamp value) => $_setField(4, value);
  @$pb.TagNumber(4)
  $core.bool hasDiskMeasuredAt() => $_has(3);
  @$pb.TagNumber(4)
  void clearDiskMeasuredAt() => $_clearField(4);
  @$pb.TagNumber(4)
  $0.Timestamp ensureDiskMeasuredAt() => $_ensure(3);

  @$pb.TagNumber(5)
  $pb.PbList<ContainerMetrics> get containers => $_getList(4);
}

class EngineDiskUsage extends $pb.GeneratedMessage {
  factory EngineDiskUsage({
    DiskBucket? images,
    DiskBucket? containers,
    DiskBucket? volumes,
    DiskBucket? buildCache,
  }) {
    final result = EngineDiskUsage._();
    if (images != null) result.images = images;
    if (containers != null) result.containers = containers;
    if (volumes != null) result.volumes = volumes;
    if (buildCache != null) result.buildCache = buildCache;
    return result;
  }

  EngineDiskUsage._();

  factory EngineDiskUsage.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      EngineDiskUsage()..mergeFromBuffer(data, registry);
  factory EngineDiskUsage.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      EngineDiskUsage()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'EngineDiskUsage',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: EngineDiskUsage.$_createMessage)
    ..aOM<DiskBucket>(1, _omitFieldNames ? '' : 'images',
        subBuilder: DiskBucket.$_createMessage)
    ..aOM<DiskBucket>(2, _omitFieldNames ? '' : 'containers',
        subBuilder: DiskBucket.$_createMessage)
    ..aOM<DiskBucket>(3, _omitFieldNames ? '' : 'volumes',
        subBuilder: DiskBucket.$_createMessage)
    ..aOM<DiskBucket>(4, _omitFieldNames ? '' : 'buildCache',
        subBuilder: DiskBucket.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  EngineDiskUsage clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  EngineDiskUsage copyWith(void Function(EngineDiskUsage) updates) =>
      super.copyWith((message) => updates(message as EngineDiskUsage))
          as EngineDiskUsage;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use EngineDiskUsage() / EngineDiskUsage.new instead')
  static EngineDiskUsage create() => EngineDiskUsage._();
  static $pb.GeneratedMessage $_createMessage() => EngineDiskUsage._();
  @$core.override
  EngineDiskUsage createEmptyInstance() => EngineDiskUsage._();
  @$core.pragma('dart2js:noInline')
  static EngineDiskUsage getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<EngineDiskUsage>(
          EngineDiskUsage.$_createMessage);
  static EngineDiskUsage? _defaultInstance;

  @$pb.TagNumber(1)
  DiskBucket get images => $_getN(0);
  @$pb.TagNumber(1)
  set images(DiskBucket value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasImages() => $_has(0);
  @$pb.TagNumber(1)
  void clearImages() => $_clearField(1);
  @$pb.TagNumber(1)
  DiskBucket ensureImages() => $_ensure(0);

  @$pb.TagNumber(2)
  DiskBucket get containers => $_getN(1);
  @$pb.TagNumber(2)
  set containers(DiskBucket value) => $_setField(2, value);
  @$pb.TagNumber(2)
  $core.bool hasContainers() => $_has(1);
  @$pb.TagNumber(2)
  void clearContainers() => $_clearField(2);
  @$pb.TagNumber(2)
  DiskBucket ensureContainers() => $_ensure(1);

  @$pb.TagNumber(3)
  DiskBucket get volumes => $_getN(2);
  @$pb.TagNumber(3)
  set volumes(DiskBucket value) => $_setField(3, value);
  @$pb.TagNumber(3)
  $core.bool hasVolumes() => $_has(2);
  @$pb.TagNumber(3)
  void clearVolumes() => $_clearField(3);
  @$pb.TagNumber(3)
  DiskBucket ensureVolumes() => $_ensure(2);

  @$pb.TagNumber(4)
  DiskBucket get buildCache => $_getN(3);
  @$pb.TagNumber(4)
  set buildCache(DiskBucket value) => $_setField(4, value);
  @$pb.TagNumber(4)
  $core.bool hasBuildCache() => $_has(3);
  @$pb.TagNumber(4)
  void clearBuildCache() => $_clearField(4);
  @$pb.TagNumber(4)
  DiskBucket ensureBuildCache() => $_ensure(3);
}

class DiskBucket extends $pb.GeneratedMessage {
  factory DiskBucket({
    $fixnum.Int64? count,
    $fixnum.Int64? sizeBytes,
    $fixnum.Int64? reclaimableBytes,
  }) {
    final result = DiskBucket._();
    if (count != null) result.count = count;
    if (sizeBytes != null) result.sizeBytes = sizeBytes;
    if (reclaimableBytes != null) result.reclaimableBytes = reclaimableBytes;
    return result;
  }

  DiskBucket._();

  factory DiskBucket.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      DiskBucket()..mergeFromBuffer(data, registry);
  factory DiskBucket.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      DiskBucket()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'DiskBucket',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: DiskBucket.$_createMessage)
    ..aInt64(1, _omitFieldNames ? '' : 'count')
    ..aInt64(2, _omitFieldNames ? '' : 'sizeBytes')
    ..aInt64(3, _omitFieldNames ? '' : 'reclaimableBytes')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  DiskBucket clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  DiskBucket copyWith(void Function(DiskBucket) updates) =>
      super.copyWith((message) => updates(message as DiskBucket)) as DiskBucket;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use DiskBucket() / DiskBucket.new instead')
  static DiskBucket create() => DiskBucket._();
  static $pb.GeneratedMessage $_createMessage() => DiskBucket._();
  @$core.override
  DiskBucket createEmptyInstance() => DiskBucket._();
  @$core.pragma('dart2js:noInline')
  static DiskBucket getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<DiskBucket>(DiskBucket.$_createMessage);
  static DiskBucket? _defaultInstance;

  @$pb.TagNumber(1)
  $fixnum.Int64 get count => $_getI64(0);
  @$pb.TagNumber(1)
  set count($fixnum.Int64 value) => $_setInt64(0, value);
  @$pb.TagNumber(1)
  $core.bool hasCount() => $_has(0);
  @$pb.TagNumber(1)
  void clearCount() => $_clearField(1);

  @$pb.TagNumber(2)
  $fixnum.Int64 get sizeBytes => $_getI64(1);
  @$pb.TagNumber(2)
  set sizeBytes($fixnum.Int64 value) => $_setInt64(1, value);
  @$pb.TagNumber(2)
  $core.bool hasSizeBytes() => $_has(1);
  @$pb.TagNumber(2)
  void clearSizeBytes() => $_clearField(2);

  @$pb.TagNumber(3)
  $fixnum.Int64 get reclaimableBytes => $_getI64(2);
  @$pb.TagNumber(3)
  set reclaimableBytes($fixnum.Int64 value) => $_setInt64(2, value);
  @$pb.TagNumber(3)
  $core.bool hasReclaimableBytes() => $_has(2);
  @$pb.TagNumber(3)
  void clearReclaimableBytes() => $_clearField(3);
}

class ContainerMetrics extends $pb.GeneratedMessage {
  factory ContainerMetrics({
    $core.String? id,
    $core.String? name,
    $core.String? image,
    $core.String? state,
    $core.double? cpuPercent,
    $fixnum.Int64? memoryUsedBytes,
    $fixnum.Int64? memoryLimitBytes,
    $core.double? memoryPercent,
    $fixnum.Int64? writableLayerBytes,
    $fixnum.Int64? rootfsBytes,
    $core.String? statsError,
  }) {
    final result = ContainerMetrics._();
    if (id != null) result.id = id;
    if (name != null) result.name = name;
    if (image != null) result.image = image;
    if (state != null) result.state = state;
    if (cpuPercent != null) result.cpuPercent = cpuPercent;
    if (memoryUsedBytes != null) result.memoryUsedBytes = memoryUsedBytes;
    if (memoryLimitBytes != null) result.memoryLimitBytes = memoryLimitBytes;
    if (memoryPercent != null) result.memoryPercent = memoryPercent;
    if (writableLayerBytes != null)
      result.writableLayerBytes = writableLayerBytes;
    if (rootfsBytes != null) result.rootfsBytes = rootfsBytes;
    if (statsError != null) result.statsError = statsError;
    return result;
  }

  ContainerMetrics._();

  factory ContainerMetrics.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ContainerMetrics()..mergeFromBuffer(data, registry);
  factory ContainerMetrics.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ContainerMetrics()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ContainerMetrics',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ContainerMetrics.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'id')
    ..aOS(2, _omitFieldNames ? '' : 'name')
    ..aOS(3, _omitFieldNames ? '' : 'image')
    ..aOS(4, _omitFieldNames ? '' : 'state')
    ..aD(5, _omitFieldNames ? '' : 'cpuPercent')
    ..a<$fixnum.Int64>(
        6, _omitFieldNames ? '' : 'memoryUsedBytes', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..a<$fixnum.Int64>(
        7, _omitFieldNames ? '' : 'memoryLimitBytes', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..aD(8, _omitFieldNames ? '' : 'memoryPercent')
    ..aInt64(9, _omitFieldNames ? '' : 'writableLayerBytes')
    ..aInt64(10, _omitFieldNames ? '' : 'rootfsBytes')
    ..aOS(11, _omitFieldNames ? '' : 'statsError')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ContainerMetrics clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ContainerMetrics copyWith(void Function(ContainerMetrics) updates) =>
      super.copyWith((message) => updates(message as ContainerMetrics))
          as ContainerMetrics;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use ContainerMetrics() / ContainerMetrics.new instead')
  static ContainerMetrics create() => ContainerMetrics._();
  static $pb.GeneratedMessage $_createMessage() => ContainerMetrics._();
  @$core.override
  ContainerMetrics createEmptyInstance() => ContainerMetrics._();
  @$core.pragma('dart2js:noInline')
  static ContainerMetrics getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<ContainerMetrics>(
          ContainerMetrics.$_createMessage);
  static ContainerMetrics? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get id => $_getSZ(0);
  @$pb.TagNumber(1)
  set id($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasId() => $_has(0);
  @$pb.TagNumber(1)
  void clearId() => $_clearField(1);

  @$pb.TagNumber(2)
  $core.String get name => $_getSZ(1);
  @$pb.TagNumber(2)
  set name($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasName() => $_has(1);
  @$pb.TagNumber(2)
  void clearName() => $_clearField(2);

  @$pb.TagNumber(3)
  $core.String get image => $_getSZ(2);
  @$pb.TagNumber(3)
  set image($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasImage() => $_has(2);
  @$pb.TagNumber(3)
  void clearImage() => $_clearField(3);

  @$pb.TagNumber(4)
  $core.String get state => $_getSZ(3);
  @$pb.TagNumber(4)
  set state($core.String value) => $_setString(3, value);
  @$pb.TagNumber(4)
  $core.bool hasState() => $_has(3);
  @$pb.TagNumber(4)
  void clearState() => $_clearField(4);

  @$pb.TagNumber(5)
  $core.double get cpuPercent => $_getN(4);
  @$pb.TagNumber(5)
  set cpuPercent($core.double value) => $_setDouble(4, value);
  @$pb.TagNumber(5)
  $core.bool hasCpuPercent() => $_has(4);
  @$pb.TagNumber(5)
  void clearCpuPercent() => $_clearField(5);

  @$pb.TagNumber(6)
  $fixnum.Int64 get memoryUsedBytes => $_getI64(5);
  @$pb.TagNumber(6)
  set memoryUsedBytes($fixnum.Int64 value) => $_setInt64(5, value);
  @$pb.TagNumber(6)
  $core.bool hasMemoryUsedBytes() => $_has(5);
  @$pb.TagNumber(6)
  void clearMemoryUsedBytes() => $_clearField(6);

  @$pb.TagNumber(7)
  $fixnum.Int64 get memoryLimitBytes => $_getI64(6);
  @$pb.TagNumber(7)
  set memoryLimitBytes($fixnum.Int64 value) => $_setInt64(6, value);
  @$pb.TagNumber(7)
  $core.bool hasMemoryLimitBytes() => $_has(6);
  @$pb.TagNumber(7)
  void clearMemoryLimitBytes() => $_clearField(7);

  @$pb.TagNumber(8)
  $core.double get memoryPercent => $_getN(7);
  @$pb.TagNumber(8)
  set memoryPercent($core.double value) => $_setDouble(7, value);
  @$pb.TagNumber(8)
  $core.bool hasMemoryPercent() => $_has(7);
  @$pb.TagNumber(8)
  void clearMemoryPercent() => $_clearField(8);

  /// writable_layer_bytes and rootfs_bytes come from the slow-interval df and
  /// are 0 until measured.
  @$pb.TagNumber(9)
  $fixnum.Int64 get writableLayerBytes => $_getI64(8);
  @$pb.TagNumber(9)
  set writableLayerBytes($fixnum.Int64 value) => $_setInt64(8, value);
  @$pb.TagNumber(9)
  $core.bool hasWritableLayerBytes() => $_has(8);
  @$pb.TagNumber(9)
  void clearWritableLayerBytes() => $_clearField(9);

  @$pb.TagNumber(10)
  $fixnum.Int64 get rootfsBytes => $_getI64(9);
  @$pb.TagNumber(10)
  set rootfsBytes($fixnum.Int64 value) => $_setInt64(9, value);
  @$pb.TagNumber(10)
  $core.bool hasRootfsBytes() => $_has(9);
  @$pb.TagNumber(10)
  void clearRootfsBytes() => $_clearField(10);

  /// stats_error is set when stats for this container could not be read.
  @$pb.TagNumber(11)
  $core.String get statsError => $_getSZ(10);
  @$pb.TagNumber(11)
  set statsError($core.String value) => $_setString(10, value);
  @$pb.TagNumber(11)
  $core.bool hasStatsError() => $_has(10);
  @$pb.TagNumber(11)
  void clearStatsError() => $_clearField(11);
}

class DirSize extends $pb.GeneratedMessage {
  factory DirSize({
    $core.String? path,
    $fixnum.Int64? sizeBytes,
    $0.Timestamp? measuredAt,
    $2.Duration? took,
    $core.bool? complete,
    $core.String? error,
  }) {
    final result = DirSize._();
    if (path != null) result.path = path;
    if (sizeBytes != null) result.sizeBytes = sizeBytes;
    if (measuredAt != null) result.measuredAt = measuredAt;
    if (took != null) result.took = took;
    if (complete != null) result.complete = complete;
    if (error != null) result.error = error;
    return result;
  }

  DirSize._();

  factory DirSize.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      DirSize()..mergeFromBuffer(data, registry);
  factory DirSize.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      DirSize()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'DirSize',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: DirSize.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'path')
    ..aInt64(2, _omitFieldNames ? '' : 'sizeBytes')
    ..aOM<$0.Timestamp>(3, _omitFieldNames ? '' : 'measuredAt',
        subBuilder: $0.Timestamp.$_createMessage)
    ..aOM<$2.Duration>(4, _omitFieldNames ? '' : 'took',
        subBuilder: $2.Duration.$_createMessage)
    ..aOB(5, _omitFieldNames ? '' : 'complete')
    ..aOS(6, _omitFieldNames ? '' : 'error')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  DirSize clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  DirSize copyWith(void Function(DirSize) updates) =>
      super.copyWith((message) => updates(message as DirSize)) as DirSize;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use DirSize() / DirSize.new instead')
  static DirSize create() => DirSize._();
  static $pb.GeneratedMessage $_createMessage() => DirSize._();
  @$core.override
  DirSize createEmptyInstance() => DirSize._();
  @$core.pragma('dart2js:noInline')
  static DirSize getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<DirSize>(DirSize.$_createMessage);
  static DirSize? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get path => $_getSZ(0);
  @$pb.TagNumber(1)
  set path($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasPath() => $_has(0);
  @$pb.TagNumber(1)
  void clearPath() => $_clearField(1);

  @$pb.TagNumber(2)
  $fixnum.Int64 get sizeBytes => $_getI64(1);
  @$pb.TagNumber(2)
  set sizeBytes($fixnum.Int64 value) => $_setInt64(1, value);
  @$pb.TagNumber(2)
  $core.bool hasSizeBytes() => $_has(1);
  @$pb.TagNumber(2)
  void clearSizeBytes() => $_clearField(2);

  @$pb.TagNumber(3)
  $0.Timestamp get measuredAt => $_getN(2);
  @$pb.TagNumber(3)
  set measuredAt($0.Timestamp value) => $_setField(3, value);
  @$pb.TagNumber(3)
  $core.bool hasMeasuredAt() => $_has(2);
  @$pb.TagNumber(3)
  void clearMeasuredAt() => $_clearField(3);
  @$pb.TagNumber(3)
  $0.Timestamp ensureMeasuredAt() => $_ensure(2);

  @$pb.TagNumber(4)
  $2.Duration get took => $_getN(3);
  @$pb.TagNumber(4)
  set took($2.Duration value) => $_setField(4, value);
  @$pb.TagNumber(4)
  $core.bool hasTook() => $_has(3);
  @$pb.TagNumber(4)
  void clearTook() => $_clearField(4);
  @$pb.TagNumber(4)
  $2.Duration ensureTook() => $_ensure(3);

  /// complete is false when the time budget ended the walk early; size_bytes
  /// is then a lower bound.
  @$pb.TagNumber(5)
  $core.bool get complete => $_getBF(4);
  @$pb.TagNumber(5)
  set complete($core.bool value) => $_setBool(4, value);
  @$pb.TagNumber(5)
  $core.bool hasComplete() => $_has(4);
  @$pb.TagNumber(5)
  void clearComplete() => $_clearField(5);

  @$pb.TagNumber(6)
  $core.String get error => $_getSZ(5);
  @$pb.TagNumber(6)
  set error($core.String value) => $_setString(5, value);
  @$pb.TagNumber(6)
  $core.bool hasError() => $_has(5);
  @$pb.TagNumber(6)
  void clearError() => $_clearField(6);
}

class NodeAlert extends $pb.GeneratedMessage {
  factory NodeAlert({
    AlertKind? kind,
    $core.String? subject,
    $core.String? message,
    $core.double? value,
    $core.double? threshold,
  }) {
    final result = NodeAlert._();
    if (kind != null) result.kind = kind;
    if (subject != null) result.subject = subject;
    if (message != null) result.message = message;
    if (value != null) result.value = value;
    if (threshold != null) result.threshold = threshold;
    return result;
  }

  NodeAlert._();

  factory NodeAlert.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      NodeAlert()..mergeFromBuffer(data, registry);
  factory NodeAlert.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      NodeAlert()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'NodeAlert',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: NodeAlert.$_createMessage)
    ..aE<AlertKind>(1, _omitFieldNames ? '' : 'kind',
        enumValues: AlertKind.values)
    ..aOS(2, _omitFieldNames ? '' : 'subject')
    ..aOS(3, _omitFieldNames ? '' : 'message')
    ..aD(4, _omitFieldNames ? '' : 'value')
    ..aD(5, _omitFieldNames ? '' : 'threshold')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  NodeAlert clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  NodeAlert copyWith(void Function(NodeAlert) updates) =>
      super.copyWith((message) => updates(message as NodeAlert)) as NodeAlert;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use NodeAlert() / NodeAlert.new instead')
  static NodeAlert create() => NodeAlert._();
  static $pb.GeneratedMessage $_createMessage() => NodeAlert._();
  @$core.override
  NodeAlert createEmptyInstance() => NodeAlert._();
  @$core.pragma('dart2js:noInline')
  static NodeAlert getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<NodeAlert>(NodeAlert.$_createMessage);
  static NodeAlert? _defaultInstance;

  @$pb.TagNumber(1)
  AlertKind get kind => $_getN(0);
  @$pb.TagNumber(1)
  set kind(AlertKind value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasKind() => $_has(0);
  @$pb.TagNumber(1)
  void clearKind() => $_clearField(1);

  /// subject is the filesystem mount for disk alerts, empty otherwise.
  @$pb.TagNumber(2)
  $core.String get subject => $_getSZ(1);
  @$pb.TagNumber(2)
  set subject($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasSubject() => $_has(1);
  @$pb.TagNumber(2)
  void clearSubject() => $_clearField(2);

  @$pb.TagNumber(3)
  $core.String get message => $_getSZ(2);
  @$pb.TagNumber(3)
  set message($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasMessage() => $_has(2);
  @$pb.TagNumber(3)
  void clearMessage() => $_clearField(3);

  @$pb.TagNumber(4)
  $core.double get value => $_getN(3);
  @$pb.TagNumber(4)
  set value($core.double value) => $_setDouble(3, value);
  @$pb.TagNumber(4)
  $core.bool hasValue() => $_has(3);
  @$pb.TagNumber(4)
  void clearValue() => $_clearField(4);

  @$pb.TagNumber(5)
  $core.double get threshold => $_getN(4);
  @$pb.TagNumber(5)
  set threshold($core.double value) => $_setDouble(4, value);
  @$pb.TagNumber(5)
  $core.bool hasThreshold() => $_has(4);
  @$pb.TagNumber(5)
  void clearThreshold() => $_clearField(5);
}

/// NodeInfo is the inventory record of one node.
class NodeInfo extends $pb.GeneratedMessage {
  factory NodeInfo({
    $core.String? name,
    $core.String? address,
    $core.String? os,
    $core.String? arch,
    $core.Iterable<$core.String>? tags,
    $core.String? sshAlias,
    $core.String? tier,
    $core.String? notes,
    $core.bool? local,
  }) {
    final result = NodeInfo._();
    if (name != null) result.name = name;
    if (address != null) result.address = address;
    if (os != null) result.os = os;
    if (arch != null) result.arch = arch;
    if (tags != null) result.tags.addAll(tags);
    if (sshAlias != null) result.sshAlias = sshAlias;
    if (tier != null) result.tier = tier;
    if (notes != null) result.notes = notes;
    if (local != null) result.local = local;
    return result;
  }

  NodeInfo._();

  factory NodeInfo.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      NodeInfo()..mergeFromBuffer(data, registry);
  factory NodeInfo.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      NodeInfo()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'NodeInfo',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: NodeInfo.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'name')
    ..aOS(2, _omitFieldNames ? '' : 'address')
    ..aOS(3, _omitFieldNames ? '' : 'os')
    ..aOS(4, _omitFieldNames ? '' : 'arch')
    ..pPS(5, _omitFieldNames ? '' : 'tags')
    ..aOS(6, _omitFieldNames ? '' : 'sshAlias')
    ..aOS(7, _omitFieldNames ? '' : 'tier')
    ..aOS(8, _omitFieldNames ? '' : 'notes')
    ..aOB(9, _omitFieldNames ? '' : 'local')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  NodeInfo clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  NodeInfo copyWith(void Function(NodeInfo) updates) =>
      super.copyWith((message) => updates(message as NodeInfo)) as NodeInfo;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use NodeInfo() / NodeInfo.new instead')
  static NodeInfo create() => NodeInfo._();
  static $pb.GeneratedMessage $_createMessage() => NodeInfo._();
  @$core.override
  NodeInfo createEmptyInstance() => NodeInfo._();
  @$core.pragma('dart2js:noInline')
  static NodeInfo getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<NodeInfo>(NodeInfo.$_createMessage);
  static NodeInfo? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get name => $_getSZ(0);
  @$pb.TagNumber(1)
  set name($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasName() => $_has(0);
  @$pb.TagNumber(1)
  void clearName() => $_clearField(1);

  /// address is the tailnet name or an address; ":7475" is assumed when no
  /// port is given.
  @$pb.TagNumber(2)
  $core.String get address => $_getSZ(1);
  @$pb.TagNumber(2)
  set address($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasAddress() => $_has(1);
  @$pb.TagNumber(2)
  void clearAddress() => $_clearField(2);

  @$pb.TagNumber(3)
  $core.String get os => $_getSZ(2);
  @$pb.TagNumber(3)
  set os($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasOs() => $_has(2);
  @$pb.TagNumber(3)
  void clearOs() => $_clearField(3);

  @$pb.TagNumber(4)
  $core.String get arch => $_getSZ(3);
  @$pb.TagNumber(4)
  set arch($core.String value) => $_setString(3, value);
  @$pb.TagNumber(4)
  $core.bool hasArch() => $_has(3);
  @$pb.TagNumber(4)
  void clearArch() => $_clearField(4);

  @$pb.TagNumber(5)
  $pb.PbList<$core.String> get tags => $_getList(4);

  /// ssh_alias is the break-glass `ssh` host alias.
  @$pb.TagNumber(6)
  $core.String get sshAlias => $_getSZ(5);
  @$pb.TagNumber(6)
  set sshAlias($core.String value) => $_setString(5, value);
  @$pb.TagNumber(6)
  $core.bool hasSshAlias() => $_has(5);
  @$pb.TagNumber(6)
  void clearSshAlias() => $_clearField(6);

  /// tier is "server" or "family" and drives future command gating.
  @$pb.TagNumber(7)
  $core.String get tier => $_getSZ(6);
  @$pb.TagNumber(7)
  set tier($core.String value) => $_setString(6, value);
  @$pb.TagNumber(7)
  $core.bool hasTier() => $_has(6);
  @$pb.TagNumber(7)
  void clearTier() => $_clearField(7);

  @$pb.TagNumber(8)
  $core.String get notes => $_getSZ(7);
  @$pb.TagNumber(8)
  set notes($core.String value) => $_setString(7, value);
  @$pb.TagNumber(8)
  $core.bool hasNotes() => $_has(7);
  @$pb.TagNumber(8)
  void clearNotes() => $_clearField(8);

  /// local marks the hub's own node, which is not stored in the inventory.
  @$pb.TagNumber(9)
  $core.bool get local => $_getBF(8);
  @$pb.TagNumber(9)
  set local($core.bool value) => $_setBool(8, value);
  @$pb.TagNumber(9)
  $core.bool hasLocal() => $_has(8);
  @$pb.TagNumber(9)
  void clearLocal() => $_clearField(9);
}

class NodeStatus extends $pb.GeneratedMessage {
  factory NodeStatus({
    NodeInfo? node,
    NodeMetrics? metrics,
    NodeFreshness? freshness,
    $0.Timestamp? fetchedAt,
    $2.Duration? age,
    $core.String? error,
    $core.Iterable<NodeAlert>? alerts,
  }) {
    final result = NodeStatus._();
    if (node != null) result.node = node;
    if (metrics != null) result.metrics = metrics;
    if (freshness != null) result.freshness = freshness;
    if (fetchedAt != null) result.fetchedAt = fetchedAt;
    if (age != null) result.age = age;
    if (error != null) result.error = error;
    if (alerts != null) result.alerts.addAll(alerts);
    return result;
  }

  NodeStatus._();

  factory NodeStatus.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      NodeStatus()..mergeFromBuffer(data, registry);
  factory NodeStatus.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      NodeStatus()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'NodeStatus',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: NodeStatus.$_createMessage)
    ..aOM<NodeInfo>(1, _omitFieldNames ? '' : 'node',
        subBuilder: NodeInfo.$_createMessage)
    ..aOM<NodeMetrics>(2, _omitFieldNames ? '' : 'metrics',
        subBuilder: NodeMetrics.$_createMessage)
    ..aE<NodeFreshness>(3, _omitFieldNames ? '' : 'freshness',
        enumValues: NodeFreshness.values)
    ..aOM<$0.Timestamp>(4, _omitFieldNames ? '' : 'fetchedAt',
        subBuilder: $0.Timestamp.$_createMessage)
    ..aOM<$2.Duration>(5, _omitFieldNames ? '' : 'age',
        subBuilder: $2.Duration.$_createMessage)
    ..aOS(6, _omitFieldNames ? '' : 'error')
    ..pPM<NodeAlert>(7, _omitFieldNames ? '' : 'alerts',
        subBuilder: NodeAlert.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  NodeStatus clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  NodeStatus copyWith(void Function(NodeStatus) updates) =>
      super.copyWith((message) => updates(message as NodeStatus)) as NodeStatus;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use NodeStatus() / NodeStatus.new instead')
  static NodeStatus create() => NodeStatus._();
  static $pb.GeneratedMessage $_createMessage() => NodeStatus._();
  @$core.override
  NodeStatus createEmptyInstance() => NodeStatus._();
  @$core.pragma('dart2js:noInline')
  static NodeStatus getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<NodeStatus>(NodeStatus.$_createMessage);
  static NodeStatus? _defaultInstance;

  @$pb.TagNumber(1)
  NodeInfo get node => $_getN(0);
  @$pb.TagNumber(1)
  set node(NodeInfo value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasNode() => $_has(0);
  @$pb.TagNumber(1)
  void clearNode() => $_clearField(1);
  @$pb.TagNumber(1)
  NodeInfo ensureNode() => $_ensure(0);

  /// metrics is the last good poll; unset when the node was never reached.
  @$pb.TagNumber(2)
  NodeMetrics get metrics => $_getN(1);
  @$pb.TagNumber(2)
  set metrics(NodeMetrics value) => $_setField(2, value);
  @$pb.TagNumber(2)
  $core.bool hasMetrics() => $_has(1);
  @$pb.TagNumber(2)
  void clearMetrics() => $_clearField(2);
  @$pb.TagNumber(2)
  NodeMetrics ensureMetrics() => $_ensure(1);

  @$pb.TagNumber(3)
  NodeFreshness get freshness => $_getN(2);
  @$pb.TagNumber(3)
  set freshness(NodeFreshness value) => $_setField(3, value);
  @$pb.TagNumber(3)
  $core.bool hasFreshness() => $_has(2);
  @$pb.TagNumber(3)
  void clearFreshness() => $_clearField(3);

  @$pb.TagNumber(4)
  $0.Timestamp get fetchedAt => $_getN(3);
  @$pb.TagNumber(4)
  set fetchedAt($0.Timestamp value) => $_setField(4, value);
  @$pb.TagNumber(4)
  $core.bool hasFetchedAt() => $_has(3);
  @$pb.TagNumber(4)
  void clearFetchedAt() => $_clearField(4);
  @$pb.TagNumber(4)
  $0.Timestamp ensureFetchedAt() => $_ensure(3);

  @$pb.TagNumber(5)
  $2.Duration get age => $_getN(4);
  @$pb.TagNumber(5)
  set age($2.Duration value) => $_setField(5, value);
  @$pb.TagNumber(5)
  $core.bool hasAge() => $_has(4);
  @$pb.TagNumber(5)
  void clearAge() => $_clearField(5);
  @$pb.TagNumber(5)
  $2.Duration ensureAge() => $_ensure(4);

  /// error is the failure of the most recent poll, empty if it succeeded.
  @$pb.TagNumber(6)
  $core.String get error => $_getSZ(5);
  @$pb.TagNumber(6)
  set error($core.String value) => $_setString(5, value);
  @$pb.TagNumber(6)
  $core.bool hasError() => $_has(5);
  @$pb.TagNumber(6)
  void clearError() => $_clearField(6);

  @$pb.TagNumber(7)
  $pb.PbList<NodeAlert> get alerts => $_getList(6);
}

class ListNodesRequest extends $pb.GeneratedMessage {
  factory ListNodesRequest() => ListNodesRequest._();

  ListNodesRequest._();

  factory ListNodesRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListNodesRequest()..mergeFromBuffer(data, registry);
  factory ListNodesRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListNodesRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ListNodesRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ListNodesRequest.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListNodesRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListNodesRequest copyWith(void Function(ListNodesRequest) updates) =>
      super.copyWith((message) => updates(message as ListNodesRequest))
          as ListNodesRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use ListNodesRequest() / ListNodesRequest.new instead')
  static ListNodesRequest create() => ListNodesRequest._();
  static $pb.GeneratedMessage $_createMessage() => ListNodesRequest._();
  @$core.override
  ListNodesRequest createEmptyInstance() => ListNodesRequest._();
  @$core.pragma('dart2js:noInline')
  static ListNodesRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<ListNodesRequest>(
          ListNodesRequest.$_createMessage);
  static ListNodesRequest? _defaultInstance;
}

class ListNodesResponse extends $pb.GeneratedMessage {
  factory ListNodesResponse({
    $core.Iterable<NodeStatus>? nodes,
  }) {
    final result = ListNodesResponse._();
    if (nodes != null) result.nodes.addAll(nodes);
    return result;
  }

  ListNodesResponse._();

  factory ListNodesResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListNodesResponse()..mergeFromBuffer(data, registry);
  factory ListNodesResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListNodesResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ListNodesResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ListNodesResponse.$_createMessage)
    ..pPM<NodeStatus>(1, _omitFieldNames ? '' : 'nodes',
        subBuilder: NodeStatus.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListNodesResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListNodesResponse copyWith(void Function(ListNodesResponse) updates) =>
      super.copyWith((message) => updates(message as ListNodesResponse))
          as ListNodesResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use ListNodesResponse() / ListNodesResponse.new instead')
  static ListNodesResponse create() => ListNodesResponse._();
  static $pb.GeneratedMessage $_createMessage() => ListNodesResponse._();
  @$core.override
  ListNodesResponse createEmptyInstance() => ListNodesResponse._();
  @$core.pragma('dart2js:noInline')
  static ListNodesResponse getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<ListNodesResponse>(
          ListNodesResponse.$_createMessage);
  static ListNodesResponse? _defaultInstance;

  @$pb.TagNumber(1)
  $pb.PbList<NodeStatus> get nodes => $_getList(0);
}

class GetNodeRequest extends $pb.GeneratedMessage {
  factory GetNodeRequest({
    $core.String? name,
  }) {
    final result = GetNodeRequest._();
    if (name != null) result.name = name;
    return result;
  }

  GetNodeRequest._();

  factory GetNodeRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      GetNodeRequest()..mergeFromBuffer(data, registry);
  factory GetNodeRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      GetNodeRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'GetNodeRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: GetNodeRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'name')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  GetNodeRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  GetNodeRequest copyWith(void Function(GetNodeRequest) updates) =>
      super.copyWith((message) => updates(message as GetNodeRequest))
          as GetNodeRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use GetNodeRequest() / GetNodeRequest.new instead')
  static GetNodeRequest create() => GetNodeRequest._();
  static $pb.GeneratedMessage $_createMessage() => GetNodeRequest._();
  @$core.override
  GetNodeRequest createEmptyInstance() => GetNodeRequest._();
  @$core.pragma('dart2js:noInline')
  static GetNodeRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<GetNodeRequest>(
          GetNodeRequest.$_createMessage);
  static GetNodeRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get name => $_getSZ(0);
  @$pb.TagNumber(1)
  set name($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasName() => $_has(0);
  @$pb.TagNumber(1)
  void clearName() => $_clearField(1);
}

class AddNodeRequest extends $pb.GeneratedMessage {
  factory AddNodeRequest({
    NodeInfo? node,
  }) {
    final result = AddNodeRequest._();
    if (node != null) result.node = node;
    return result;
  }

  AddNodeRequest._();

  factory AddNodeRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      AddNodeRequest()..mergeFromBuffer(data, registry);
  factory AddNodeRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      AddNodeRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'AddNodeRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: AddNodeRequest.$_createMessage)
    ..aOM<NodeInfo>(1, _omitFieldNames ? '' : 'node',
        subBuilder: NodeInfo.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  AddNodeRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  AddNodeRequest copyWith(void Function(AddNodeRequest) updates) =>
      super.copyWith((message) => updates(message as AddNodeRequest))
          as AddNodeRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use AddNodeRequest() / AddNodeRequest.new instead')
  static AddNodeRequest create() => AddNodeRequest._();
  static $pb.GeneratedMessage $_createMessage() => AddNodeRequest._();
  @$core.override
  AddNodeRequest createEmptyInstance() => AddNodeRequest._();
  @$core.pragma('dart2js:noInline')
  static AddNodeRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<AddNodeRequest>(
          AddNodeRequest.$_createMessage);
  static AddNodeRequest? _defaultInstance;

  @$pb.TagNumber(1)
  NodeInfo get node => $_getN(0);
  @$pb.TagNumber(1)
  set node(NodeInfo value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasNode() => $_has(0);
  @$pb.TagNumber(1)
  void clearNode() => $_clearField(1);
  @$pb.TagNumber(1)
  NodeInfo ensureNode() => $_ensure(0);
}

class RemoveNodeRequest extends $pb.GeneratedMessage {
  factory RemoveNodeRequest({
    $core.String? name,
  }) {
    final result = RemoveNodeRequest._();
    if (name != null) result.name = name;
    return result;
  }

  RemoveNodeRequest._();

  factory RemoveNodeRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      RemoveNodeRequest()..mergeFromBuffer(data, registry);
  factory RemoveNodeRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      RemoveNodeRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'RemoveNodeRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: RemoveNodeRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'name')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  RemoveNodeRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  RemoveNodeRequest copyWith(void Function(RemoveNodeRequest) updates) =>
      super.copyWith((message) => updates(message as RemoveNodeRequest))
          as RemoveNodeRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use RemoveNodeRequest() / RemoveNodeRequest.new instead')
  static RemoveNodeRequest create() => RemoveNodeRequest._();
  static $pb.GeneratedMessage $_createMessage() => RemoveNodeRequest._();
  @$core.override
  RemoveNodeRequest createEmptyInstance() => RemoveNodeRequest._();
  @$core.pragma('dart2js:noInline')
  static RemoveNodeRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<RemoveNodeRequest>(
          RemoveNodeRequest.$_createMessage);
  static RemoveNodeRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get name => $_getSZ(0);
  @$pb.TagNumber(1)
  set name($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasName() => $_has(0);
  @$pb.TagNumber(1)
  void clearName() => $_clearField(1);
}

class RemoveNodeResponse extends $pb.GeneratedMessage {
  factory RemoveNodeResponse() => RemoveNodeResponse._();

  RemoveNodeResponse._();

  factory RemoveNodeResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      RemoveNodeResponse()..mergeFromBuffer(data, registry);
  factory RemoveNodeResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      RemoveNodeResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'RemoveNodeResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: RemoveNodeResponse.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  RemoveNodeResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  RemoveNodeResponse copyWith(void Function(RemoveNodeResponse) updates) =>
      super.copyWith((message) => updates(message as RemoveNodeResponse))
          as RemoveNodeResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use RemoveNodeResponse() / RemoveNodeResponse.new instead')
  static RemoveNodeResponse create() => RemoveNodeResponse._();
  static $pb.GeneratedMessage $_createMessage() => RemoveNodeResponse._();
  @$core.override
  RemoveNodeResponse createEmptyInstance() => RemoveNodeResponse._();
  @$core.pragma('dart2js:noInline')
  static RemoveNodeResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<RemoveNodeResponse>(
          RemoveNodeResponse.$_createMessage);
  static RemoveNodeResponse? _defaultInstance;
}

/// NodeAgentService runs on every node. The hub polls GetMetrics; WatchMetrics
/// is for live views. The job RPCs are called by the hub only and are unary
/// except RunJob, so they work over plain HTTP/1.1.
class NodeAgentServiceApi {
  final $pb.RpcClient _client;

  NodeAgentServiceApi(this._client);

  /// GetMetrics collects this node's current metrics.
  $async.Future<NodeMetrics> getMetrics(
          $pb.ClientContext? ctx, GetMetricsRequest request) =>
      _client.invoke<NodeMetrics>(
          ctx, 'NodeAgentService', 'GetMetrics', request, NodeMetrics());

  /// WatchMetrics streams this node's metrics at an interval.
  $async.Future<NodeMetrics> watchMetrics(
          $pb.ClientContext? ctx, WatchMetricsRequest request) =>
      _client.invoke<NodeMetrics>(
          ctx, 'NodeAgentService', 'WatchMetrics', request, NodeMetrics());

  /// PrepareJob tells the hub what the node already holds for a git job: whether
  /// it has the commit and which commits it knows, so the hub can send a
  /// smaller bundle.
  $async.Future<$3.PrepareJobResponse> prepareJob(
          $pb.ClientContext? ctx, $3.PrepareJobRequest request) =>
      _client.invoke<$3.PrepareJobResponse>(ctx, 'NodeAgentService',
          'PrepareJob', request, $3.PrepareJobResponse());

  /// UploadJobData appends one chunk of a job's source (a git bundle or an
  /// artifact file). Chunks must arrive in order; the last one carries the
  /// SHA-256 of the whole upload.
  $async.Future<$3.UploadJobDataResponse> uploadJobData(
          $pb.ClientContext? ctx, $3.UploadJobDataRequest request) =>
      _client.invoke<$3.UploadJobDataResponse>(ctx, 'NodeAgentService',
          'UploadJobData', request, $3.UploadJobDataResponse());

  /// RunJob checks the source out, runs the command and streams its output;
  /// matching artifacts follow the exit. The job dies with the stream: if the
  /// caller disconnects, the process tree is killed.
  $async.Future<$3.JobEvent> runJob(
          $pb.ClientContext? ctx, $3.RunJobRequest request) =>
      _client.invoke<$3.JobEvent>(
          ctx, 'NodeAgentService', 'RunJob', request, $3.JobEvent());

  /// CancelJob kills a running job's process tree.
  $async.Future<$3.NodeAgentServiceCancelJobResponse> cancelJob(
          $pb.ClientContext? ctx,
          $3.NodeAgentServiceCancelJobRequest request) =>
      _client.invoke<$3.NodeAgentServiceCancelJobResponse>(
          ctx,
          'NodeAgentService',
          'CancelJob',
          request,
          $3.NodeAgentServiceCancelJobResponse());
}

/// NodeService runs on the hub: the inventory plus the latest metrics of each
/// node. Polls are announced on EventService as "node.metrics" (subject = node
/// name, payload = summary) and "node.unreachable".
class NodeServiceApi {
  final $pb.RpcClient _client;

  NodeServiceApi(this._client);

  /// ListNodes returns every node with its latest metrics summary,
  /// freshness and alerts.
  $async.Future<ListNodesResponse> listNodes(
          $pb.ClientContext? ctx, ListNodesRequest request) =>
      _client.invoke<ListNodesResponse>(
          ctx, 'NodeService', 'ListNodes', request, ListNodesResponse());

  /// GetNode returns one node with full metrics: filesystems, GPUs,
  /// containers, engine disk usage and watched directories.
  $async.Future<NodeStatus> getNode(
          $pb.ClientContext? ctx, GetNodeRequest request) =>
      _client.invoke<NodeStatus>(
          ctx, 'NodeService', 'GetNode', request, NodeStatus());

  /// AddNode adds or updates a node in the hub inventory.
  $async.Future<NodeInfo> addNode(
          $pb.ClientContext? ctx, AddNodeRequest request) =>
      _client.invoke<NodeInfo>(
          ctx, 'NodeService', 'AddNode', request, NodeInfo());

  /// RemoveNode removes a node from the hub inventory. It does not touch the
  /// machine itself.
  $async.Future<RemoveNodeResponse> removeNode(
          $pb.ClientContext? ctx, RemoveNodeRequest request) =>
      _client.invoke<RemoveNodeResponse>(
          ctx, 'NodeService', 'RemoveNode', request, RemoveNodeResponse());
}

const $core.bool _omitFieldNames =
    $core.bool.fromEnvironment('protobuf.omit_field_names');
const $core.bool _omitMessageNames =
    $core.bool.fromEnvironment('protobuf.omit_message_names');
