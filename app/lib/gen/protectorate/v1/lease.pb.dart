// This is a generated file - do not edit.
//
// Generated from protectorate/v1/lease.proto.

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
    as $0;
import 'package:protobuf/well_known_types/google/protobuf/timestamp.pb.dart'
    as $1;

import 'lease.pbenum.dart';

export 'package:protobuf/protobuf.dart' show GeneratedMessageGenericExtensions;

export 'lease.pbenum.dart';

/// NodeSelector describes the nodes a lease or job may land on. Zero fields
/// match anything.
class NodeSelector extends $pb.GeneratedMessage {
  factory NodeSelector({
    $core.String? os,
    $core.String? arch,
    $fixnum.Int64? minFreeMemoryBytes,
    $fixnum.Int64? gpuVramBytes,
    $core.String? gpu,
    $core.String? npu,
  }) {
    final result = NodeSelector._();
    if (os != null) result.os = os;
    if (arch != null) result.arch = arch;
    if (minFreeMemoryBytes != null)
      result.minFreeMemoryBytes = minFreeMemoryBytes;
    if (gpuVramBytes != null) result.gpuVramBytes = gpuVramBytes;
    if (gpu != null) result.gpu = gpu;
    if (npu != null) result.npu = npu;
    return result;
  }

  NodeSelector._();

  factory NodeSelector.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      NodeSelector()..mergeFromBuffer(data, registry);
  factory NodeSelector.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      NodeSelector()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'NodeSelector',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: NodeSelector.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'os')
    ..aOS(2, _omitFieldNames ? '' : 'arch')
    ..a<$fixnum.Int64>(
        3, _omitFieldNames ? '' : 'minFreeMemoryBytes', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..a<$fixnum.Int64>(
        4, _omitFieldNames ? '' : 'gpuVramBytes', $pb.PbFieldType.OU6,
        defaultOrMaker: $fixnum.Int64.ZERO)
    ..aOS(5, _omitFieldNames ? '' : 'gpu')
    ..aOS(6, _omitFieldNames ? '' : 'npu')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  NodeSelector clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  NodeSelector copyWith(void Function(NodeSelector) updates) =>
      super.copyWith((message) => updates(message as NodeSelector))
          as NodeSelector;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use NodeSelector() / NodeSelector.new instead')
  static NodeSelector create() => NodeSelector._();
  static $pb.GeneratedMessage $_createMessage() => NodeSelector._();
  @$core.override
  NodeSelector createEmptyInstance() => NodeSelector._();
  @$core.pragma('dart2js:noInline')
  static NodeSelector getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<NodeSelector>(
          NodeSelector.$_createMessage);
  static NodeSelector? _defaultInstance;

  /// os and arch are Go GOOS/GOARCH values such as "windows" and "arm64".
  @$pb.TagNumber(1)
  $core.String get os => $_getSZ(0);
  @$pb.TagNumber(1)
  set os($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasOs() => $_has(0);
  @$pb.TagNumber(1)
  void clearOs() => $_clearField(1);

  @$pb.TagNumber(2)
  $core.String get arch => $_getSZ(1);
  @$pb.TagNumber(2)
  set arch($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasArch() => $_has(1);
  @$pb.TagNumber(2)
  void clearArch() => $_clearField(2);

  /// min_free_memory_bytes is the least available memory the node must report.
  @$pb.TagNumber(3)
  $fixnum.Int64 get minFreeMemoryBytes => $_getI64(2);
  @$pb.TagNumber(3)
  set minFreeMemoryBytes($fixnum.Int64 value) => $_setInt64(2, value);
  @$pb.TagNumber(3)
  $core.bool hasMinFreeMemoryBytes() => $_has(2);
  @$pb.TagNumber(3)
  void clearMinFreeMemoryBytes() => $_clearField(3);

  /// gpu_vram_bytes asks for one GPU with at least this much free VRAM; a lease
  /// then reserves that GPU instead of the whole node.
  @$pb.TagNumber(4)
  $fixnum.Int64 get gpuVramBytes => $_getI64(3);
  @$pb.TagNumber(4)
  set gpuVramBytes($fixnum.Int64 value) => $_setInt64(3, value);
  @$pb.TagNumber(4)
  $core.bool hasGpuVramBytes() => $_has(3);
  @$pb.TagNumber(4)
  void clearGpuVramBytes() => $_clearField(4);

  /// gpu requires a GPU whose vendor or name contains this text, ignoring
  /// case (e.g. "adreno", "intel"); with gpu_vram_bytes the reserved GPU must
  /// match it too.
  @$pb.TagNumber(5)
  $core.String get gpu => $_getSZ(4);
  @$pb.TagNumber(5)
  set gpu($core.String value) => $_setString(4, value);
  @$pb.TagNumber(5)
  $core.bool hasGpu() => $_has(4);
  @$pb.TagNumber(5)
  void clearGpu() => $_clearField(5);

  /// npu requires an NPU whose vendor or name contains this text, ignoring
  /// case (e.g. "hexagon").
  @$pb.TagNumber(6)
  $core.String get npu => $_getSZ(5);
  @$pb.TagNumber(6)
  set npu($core.String value) => $_setString(5, value);
  @$pb.TagNumber(6)
  $core.bool hasNpu() => $_has(5);
  @$pb.TagNumber(6)
  void clearNpu() => $_clearField(6);
}

enum AcquireLeaseRequest_Target { resource, selector, notSet }

class AcquireLeaseRequest extends $pb.GeneratedMessage {
  factory AcquireLeaseRequest({
    $core.String? resource,
    NodeSelector? selector,
    $core.String? holder,
    $0.Duration? ttl,
    $core.bool? allowFamily,
    $core.bool? wait,
    $core.int? priority,
  }) {
    final result = AcquireLeaseRequest._();
    if (resource != null) result.resource = resource;
    if (selector != null) result.selector = selector;
    if (holder != null) result.holder = holder;
    if (ttl != null) result.ttl = ttl;
    if (allowFamily != null) result.allowFamily = allowFamily;
    if (wait != null) result.wait = wait;
    if (priority != null) result.priority = priority;
    return result;
  }

  AcquireLeaseRequest._();

  factory AcquireLeaseRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      AcquireLeaseRequest()..mergeFromBuffer(data, registry);
  factory AcquireLeaseRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      AcquireLeaseRequest()..mergeFromJson(json, registry);

  static const $core.Map<$core.int, AcquireLeaseRequest_Target>
      _AcquireLeaseRequest_TargetByTag = {
    1: AcquireLeaseRequest_Target.resource,
    2: AcquireLeaseRequest_Target.selector,
    0: AcquireLeaseRequest_Target.notSet
  };
  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'AcquireLeaseRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: AcquireLeaseRequest.$_createMessage)
    ..oo(0, [1, 2])
    ..aOS(1, _omitFieldNames ? '' : 'resource')
    ..aOM<NodeSelector>(2, _omitFieldNames ? '' : 'selector',
        subBuilder: NodeSelector.$_createMessage)
    ..aOS(3, _omitFieldNames ? '' : 'holder')
    ..aOM<$0.Duration>(4, _omitFieldNames ? '' : 'ttl',
        subBuilder: $0.Duration.$_createMessage)
    ..aOB(5, _omitFieldNames ? '' : 'allowFamily')
    ..aOB(6, _omitFieldNames ? '' : 'wait')
    ..aI(7, _omitFieldNames ? '' : 'priority')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  AcquireLeaseRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  AcquireLeaseRequest copyWith(void Function(AcquireLeaseRequest) updates) =>
      super.copyWith((message) => updates(message as AcquireLeaseRequest))
          as AcquireLeaseRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core
      .Deprecated('Use AcquireLeaseRequest() / AcquireLeaseRequest.new instead')
  static AcquireLeaseRequest create() => AcquireLeaseRequest._();
  static $pb.GeneratedMessage $_createMessage() => AcquireLeaseRequest._();
  @$core.override
  AcquireLeaseRequest createEmptyInstance() => AcquireLeaseRequest._();
  @$core.pragma('dart2js:noInline')
  static AcquireLeaseRequest getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<AcquireLeaseRequest>(
          AcquireLeaseRequest.$_createMessage);
  static AcquireLeaseRequest? _defaultInstance;

  @$pb.TagNumber(1)
  @$pb.TagNumber(2)
  AcquireLeaseRequest_Target whichTarget() =>
      _AcquireLeaseRequest_TargetByTag[$_whichOneof(0)]!;
  @$pb.TagNumber(1)
  @$pb.TagNumber(2)
  void clearTarget() => $_clearField($_whichOneof(0));

  /// resource is a node name or "<node>:gpu<index>".
  @$pb.TagNumber(1)
  $core.String get resource => $_getSZ(0);
  @$pb.TagNumber(1)
  set resource($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasResource() => $_has(0);
  @$pb.TagNumber(1)
  void clearResource() => $_clearField(1);

  @$pb.TagNumber(2)
  NodeSelector get selector => $_getN(1);
  @$pb.TagNumber(2)
  set selector(NodeSelector value) => $_setField(2, value);
  @$pb.TagNumber(2)
  $core.bool hasSelector() => $_has(1);
  @$pb.TagNumber(2)
  void clearSelector() => $_clearField(2);
  @$pb.TagNumber(2)
  NodeSelector ensureSelector() => $_ensure(1);

  /// holder labels who holds the lease, e.g. a sleeve or agent name.
  @$pb.TagNumber(3)
  $core.String get holder => $_getSZ(2);
  @$pb.TagNumber(3)
  set holder($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasHolder() => $_has(2);
  @$pb.TagNumber(3)
  void clearHolder() => $_clearField(3);

  /// ttl is how long the lease lives without a renewal; at most 24h.
  @$pb.TagNumber(4)
  $0.Duration get ttl => $_getN(3);
  @$pb.TagNumber(4)
  set ttl($0.Duration value) => $_setField(4, value);
  @$pb.TagNumber(4)
  $core.bool hasTtl() => $_has(3);
  @$pb.TagNumber(4)
  void clearTtl() => $_clearField(4);
  @$pb.TagNumber(4)
  $0.Duration ensureTtl() => $_ensure(3);

  /// allow_family admits nodes of tier "family", which are skipped otherwise.
  @$pb.TagNumber(5)
  $core.bool get allowFamily => $_getBF(4);
  @$pb.TagNumber(5)
  set allowFamily($core.bool value) => $_setBool(4, value);
  @$pb.TagNumber(5)
  $core.bool hasAllowFamily() => $_has(4);
  @$pb.TagNumber(5)
  void clearAllowFamily() => $_clearField(5);

  /// wait queues the caller when the resource is held instead of failing.
  @$pb.TagNumber(6)
  $core.bool get wait => $_getBF(5);
  @$pb.TagNumber(6)
  set wait($core.bool value) => $_setBool(5, value);
  @$pb.TagNumber(6)
  $core.bool hasWait() => $_has(5);
  @$pb.TagNumber(6)
  void clearWait() => $_clearField(6);

  /// priority orders the queue: larger first, then first come first served.
  @$pb.TagNumber(7)
  $core.int get priority => $_getIZ(6);
  @$pb.TagNumber(7)
  set priority($core.int value) => $_setSignedInt32(6, value);
  @$pb.TagNumber(7)
  $core.bool hasPriority() => $_has(6);
  @$pb.TagNumber(7)
  void clearPriority() => $_clearField(7);
}

class RenewLeaseRequest extends $pb.GeneratedMessage {
  factory RenewLeaseRequest({
    $core.String? id,
    $0.Duration? ttl,
  }) {
    final result = RenewLeaseRequest._();
    if (id != null) result.id = id;
    if (ttl != null) result.ttl = ttl;
    return result;
  }

  RenewLeaseRequest._();

  factory RenewLeaseRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      RenewLeaseRequest()..mergeFromBuffer(data, registry);
  factory RenewLeaseRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      RenewLeaseRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'RenewLeaseRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: RenewLeaseRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'id')
    ..aOM<$0.Duration>(2, _omitFieldNames ? '' : 'ttl',
        subBuilder: $0.Duration.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  RenewLeaseRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  RenewLeaseRequest copyWith(void Function(RenewLeaseRequest) updates) =>
      super.copyWith((message) => updates(message as RenewLeaseRequest))
          as RenewLeaseRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use RenewLeaseRequest() / RenewLeaseRequest.new instead')
  static RenewLeaseRequest create() => RenewLeaseRequest._();
  static $pb.GeneratedMessage $_createMessage() => RenewLeaseRequest._();
  @$core.override
  RenewLeaseRequest createEmptyInstance() => RenewLeaseRequest._();
  @$core.pragma('dart2js:noInline')
  static RenewLeaseRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<RenewLeaseRequest>(
          RenewLeaseRequest.$_createMessage);
  static RenewLeaseRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get id => $_getSZ(0);
  @$pb.TagNumber(1)
  set id($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasId() => $_has(0);
  @$pb.TagNumber(1)
  void clearId() => $_clearField(1);

  /// ttl replaces the lease's TTL; zero keeps the current one.
  @$pb.TagNumber(2)
  $0.Duration get ttl => $_getN(1);
  @$pb.TagNumber(2)
  set ttl($0.Duration value) => $_setField(2, value);
  @$pb.TagNumber(2)
  $core.bool hasTtl() => $_has(1);
  @$pb.TagNumber(2)
  void clearTtl() => $_clearField(2);
  @$pb.TagNumber(2)
  $0.Duration ensureTtl() => $_ensure(1);
}

class ReleaseLeaseRequest extends $pb.GeneratedMessage {
  factory ReleaseLeaseRequest({
    $core.String? id,
  }) {
    final result = ReleaseLeaseRequest._();
    if (id != null) result.id = id;
    return result;
  }

  ReleaseLeaseRequest._();

  factory ReleaseLeaseRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ReleaseLeaseRequest()..mergeFromBuffer(data, registry);
  factory ReleaseLeaseRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ReleaseLeaseRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ReleaseLeaseRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ReleaseLeaseRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'id')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ReleaseLeaseRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ReleaseLeaseRequest copyWith(void Function(ReleaseLeaseRequest) updates) =>
      super.copyWith((message) => updates(message as ReleaseLeaseRequest))
          as ReleaseLeaseRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core
      .Deprecated('Use ReleaseLeaseRequest() / ReleaseLeaseRequest.new instead')
  static ReleaseLeaseRequest create() => ReleaseLeaseRequest._();
  static $pb.GeneratedMessage $_createMessage() => ReleaseLeaseRequest._();
  @$core.override
  ReleaseLeaseRequest createEmptyInstance() => ReleaseLeaseRequest._();
  @$core.pragma('dart2js:noInline')
  static ReleaseLeaseRequest getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<ReleaseLeaseRequest>(
          ReleaseLeaseRequest.$_createMessage);
  static ReleaseLeaseRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get id => $_getSZ(0);
  @$pb.TagNumber(1)
  set id($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasId() => $_has(0);
  @$pb.TagNumber(1)
  void clearId() => $_clearField(1);
}

class ReleaseLeaseResponse extends $pb.GeneratedMessage {
  factory ReleaseLeaseResponse() => ReleaseLeaseResponse._();

  ReleaseLeaseResponse._();

  factory ReleaseLeaseResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ReleaseLeaseResponse()..mergeFromBuffer(data, registry);
  factory ReleaseLeaseResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ReleaseLeaseResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ReleaseLeaseResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ReleaseLeaseResponse.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ReleaseLeaseResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ReleaseLeaseResponse copyWith(void Function(ReleaseLeaseResponse) updates) =>
      super.copyWith((message) => updates(message as ReleaseLeaseResponse))
          as ReleaseLeaseResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use ReleaseLeaseResponse() / ReleaseLeaseResponse.new instead')
  static ReleaseLeaseResponse create() => ReleaseLeaseResponse._();
  static $pb.GeneratedMessage $_createMessage() => ReleaseLeaseResponse._();
  @$core.override
  ReleaseLeaseResponse createEmptyInstance() => ReleaseLeaseResponse._();
  @$core.pragma('dart2js:noInline')
  static ReleaseLeaseResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<ReleaseLeaseResponse>(
          ReleaseLeaseResponse.$_createMessage);
  static ReleaseLeaseResponse? _defaultInstance;
}

class ListLeasesRequest extends $pb.GeneratedMessage {
  factory ListLeasesRequest() => ListLeasesRequest._();

  ListLeasesRequest._();

  factory ListLeasesRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListLeasesRequest()..mergeFromBuffer(data, registry);
  factory ListLeasesRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListLeasesRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ListLeasesRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ListLeasesRequest.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListLeasesRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListLeasesRequest copyWith(void Function(ListLeasesRequest) updates) =>
      super.copyWith((message) => updates(message as ListLeasesRequest))
          as ListLeasesRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use ListLeasesRequest() / ListLeasesRequest.new instead')
  static ListLeasesRequest create() => ListLeasesRequest._();
  static $pb.GeneratedMessage $_createMessage() => ListLeasesRequest._();
  @$core.override
  ListLeasesRequest createEmptyInstance() => ListLeasesRequest._();
  @$core.pragma('dart2js:noInline')
  static ListLeasesRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<ListLeasesRequest>(
          ListLeasesRequest.$_createMessage);
  static ListLeasesRequest? _defaultInstance;
}

class ListLeasesResponse extends $pb.GeneratedMessage {
  factory ListLeasesResponse({
    $core.Iterable<Lease>? leases,
  }) {
    final result = ListLeasesResponse._();
    if (leases != null) result.leases.addAll(leases);
    return result;
  }

  ListLeasesResponse._();

  factory ListLeasesResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListLeasesResponse()..mergeFromBuffer(data, registry);
  factory ListLeasesResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListLeasesResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ListLeasesResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ListLeasesResponse.$_createMessage)
    ..pPM<Lease>(1, _omitFieldNames ? '' : 'leases',
        subBuilder: Lease.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListLeasesResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListLeasesResponse copyWith(void Function(ListLeasesResponse) updates) =>
      super.copyWith((message) => updates(message as ListLeasesResponse))
          as ListLeasesResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use ListLeasesResponse() / ListLeasesResponse.new instead')
  static ListLeasesResponse create() => ListLeasesResponse._();
  static $pb.GeneratedMessage $_createMessage() => ListLeasesResponse._();
  @$core.override
  ListLeasesResponse createEmptyInstance() => ListLeasesResponse._();
  @$core.pragma('dart2js:noInline')
  static ListLeasesResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<ListLeasesResponse>(
          ListLeasesResponse.$_createMessage);
  static ListLeasesResponse? _defaultInstance;

  @$pb.TagNumber(1)
  $pb.PbList<Lease> get leases => $_getList(0);
}

/// Lease is one reservation, or a ticket waiting for one.
class Lease extends $pb.GeneratedMessage {
  factory Lease({
    $core.String? id,
    $core.String? resource,
    $core.String? node,
    $core.int? gpuIndex,
    $core.String? holder,
    $0.Duration? ttl,
    $1.Timestamp? acquiredAt,
    $1.Timestamp? expiresAt,
    LeaseState? state,
    $core.int? queuePosition,
  }) {
    final result = Lease._();
    if (id != null) result.id = id;
    if (resource != null) result.resource = resource;
    if (node != null) result.node = node;
    if (gpuIndex != null) result.gpuIndex = gpuIndex;
    if (holder != null) result.holder = holder;
    if (ttl != null) result.ttl = ttl;
    if (acquiredAt != null) result.acquiredAt = acquiredAt;
    if (expiresAt != null) result.expiresAt = expiresAt;
    if (state != null) result.state = state;
    if (queuePosition != null) result.queuePosition = queuePosition;
    return result;
  }

  Lease._();

  factory Lease.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      Lease()..mergeFromBuffer(data, registry);
  factory Lease.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      Lease()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'Lease',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: Lease.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'id')
    ..aOS(2, _omitFieldNames ? '' : 'resource')
    ..aOS(3, _omitFieldNames ? '' : 'node')
    ..aI(4, _omitFieldNames ? '' : 'gpuIndex')
    ..aOS(5, _omitFieldNames ? '' : 'holder')
    ..aOM<$0.Duration>(6, _omitFieldNames ? '' : 'ttl',
        subBuilder: $0.Duration.$_createMessage)
    ..aOM<$1.Timestamp>(7, _omitFieldNames ? '' : 'acquiredAt',
        subBuilder: $1.Timestamp.$_createMessage)
    ..aOM<$1.Timestamp>(8, _omitFieldNames ? '' : 'expiresAt',
        subBuilder: $1.Timestamp.$_createMessage)
    ..aE<LeaseState>(9, _omitFieldNames ? '' : 'state',
        enumValues: LeaseState.values)
    ..aI(10, _omitFieldNames ? '' : 'queuePosition')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  Lease clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  Lease copyWith(void Function(Lease) updates) =>
      super.copyWith((message) => updates(message as Lease)) as Lease;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use Lease() / Lease.new instead')
  static Lease create() => Lease._();
  static $pb.GeneratedMessage $_createMessage() => Lease._();
  @$core.override
  Lease createEmptyInstance() => Lease._();
  @$core.pragma('dart2js:noInline')
  static Lease getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<Lease>(Lease.$_createMessage);
  static Lease? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get id => $_getSZ(0);
  @$pb.TagNumber(1)
  set id($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasId() => $_has(0);
  @$pb.TagNumber(1)
  void clearId() => $_clearField(1);

  /// resource is the chosen resource: a node name or "<node>:gpu<index>".
  @$pb.TagNumber(2)
  $core.String get resource => $_getSZ(1);
  @$pb.TagNumber(2)
  set resource($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasResource() => $_has(1);
  @$pb.TagNumber(2)
  void clearResource() => $_clearField(2);

  @$pb.TagNumber(3)
  $core.String get node => $_getSZ(2);
  @$pb.TagNumber(3)
  set node($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasNode() => $_has(2);
  @$pb.TagNumber(3)
  void clearNode() => $_clearField(3);

  /// gpu_index is set when the lease covers a single GPU.
  @$pb.TagNumber(4)
  $core.int get gpuIndex => $_getIZ(3);
  @$pb.TagNumber(4)
  set gpuIndex($core.int value) => $_setSignedInt32(3, value);
  @$pb.TagNumber(4)
  $core.bool hasGpuIndex() => $_has(3);
  @$pb.TagNumber(4)
  void clearGpuIndex() => $_clearField(4);

  @$pb.TagNumber(5)
  $core.String get holder => $_getSZ(4);
  @$pb.TagNumber(5)
  set holder($core.String value) => $_setString(4, value);
  @$pb.TagNumber(5)
  $core.bool hasHolder() => $_has(4);
  @$pb.TagNumber(5)
  void clearHolder() => $_clearField(5);

  @$pb.TagNumber(6)
  $0.Duration get ttl => $_getN(5);
  @$pb.TagNumber(6)
  set ttl($0.Duration value) => $_setField(6, value);
  @$pb.TagNumber(6)
  $core.bool hasTtl() => $_has(5);
  @$pb.TagNumber(6)
  void clearTtl() => $_clearField(6);
  @$pb.TagNumber(6)
  $0.Duration ensureTtl() => $_ensure(5);

  @$pb.TagNumber(7)
  $1.Timestamp get acquiredAt => $_getN(6);
  @$pb.TagNumber(7)
  set acquiredAt($1.Timestamp value) => $_setField(7, value);
  @$pb.TagNumber(7)
  $core.bool hasAcquiredAt() => $_has(6);
  @$pb.TagNumber(7)
  void clearAcquiredAt() => $_clearField(7);
  @$pb.TagNumber(7)
  $1.Timestamp ensureAcquiredAt() => $_ensure(6);

  @$pb.TagNumber(8)
  $1.Timestamp get expiresAt => $_getN(7);
  @$pb.TagNumber(8)
  set expiresAt($1.Timestamp value) => $_setField(8, value);
  @$pb.TagNumber(8)
  $core.bool hasExpiresAt() => $_has(7);
  @$pb.TagNumber(8)
  void clearExpiresAt() => $_clearField(8);
  @$pb.TagNumber(8)
  $1.Timestamp ensureExpiresAt() => $_ensure(7);

  @$pb.TagNumber(9)
  LeaseState get state => $_getN(8);
  @$pb.TagNumber(9)
  set state(LeaseState value) => $_setField(9, value);
  @$pb.TagNumber(9)
  $core.bool hasState() => $_has(8);
  @$pb.TagNumber(9)
  void clearState() => $_clearField(9);

  /// queue_position is 1 for the next ticket to be granted; set when QUEUED.
  @$pb.TagNumber(10)
  $core.int get queuePosition => $_getIZ(9);
  @$pb.TagNumber(10)
  set queuePosition($core.int value) => $_setSignedInt32(9, value);
  @$pb.TagNumber(10)
  $core.bool hasQueuePosition() => $_has(9);
  @$pb.TagNumber(10)
  void clearQueuePosition() => $_clearField(10);
}

/// LeaseService reserves nodes and GPUs for a holder. A lease that is not
/// renewed expires on its own, so a dead agent cannot hold hardware forever.
/// Leases are kept across hub restarts and announced on EventService as
/// "lease.acquired", "lease.renewed", "lease.released" and "lease.expired"
/// (subject = lease id), and for the queue as "lease.queued", "lease.granted"
/// and "lease.dropped" (subject = ticket id).
class LeaseServiceApi {
  final $pb.RpcClient _client;

  LeaseServiceApi(this._client);

  /// AcquireLease reserves one resource exclusively: a node name ("win1"), one
  /// GPU ("gpu-box:gpu1") or whatever a selector picks. A selector chooses the
  /// least-loaded fresh matching node that is not already leased. When the
  /// resource is held, the call fails with failed_precondition, unless the
  /// request sets wait: then the caller is queued first in first out (highest
  /// priority first) and gets a ticket, a Lease in state QUEUED. When the hub
  /// grants a ticket it holds the lease for the waiter for a short claim
  /// window and sends it a notice message; RenewLease with no ttl claims the
  /// lease for the requested ttl, otherwise it passes to the next waiter.
  /// ReleaseLease on a ticket leaves the queue.
  $async.Future<Lease> acquireLease(
          $pb.ClientContext? ctx, AcquireLeaseRequest request) =>
      _client.invoke<Lease>(
          ctx, 'LeaseService', 'AcquireLease', request, Lease());

  /// RenewLease pushes the expiry of a live lease to now plus a TTL.
  $async.Future<Lease> renewLease(
          $pb.ClientContext? ctx, RenewLeaseRequest request) =>
      _client.invoke<Lease>(
          ctx, 'LeaseService', 'RenewLease', request, Lease());

  /// ReleaseLease frees a lease at once. Releasing an unknown or expired lease
  /// is an error.
  $async.Future<ReleaseLeaseResponse> releaseLease(
          $pb.ClientContext? ctx, ReleaseLeaseRequest request) =>
      _client.invoke<ReleaseLeaseResponse>(
          ctx, 'LeaseService', 'ReleaseLease', request, ReleaseLeaseResponse());

  /// ListLeases returns every live lease, then every queued ticket.
  $async.Future<ListLeasesResponse> listLeases(
          $pb.ClientContext? ctx, ListLeasesRequest request) =>
      _client.invoke<ListLeasesResponse>(
          ctx, 'LeaseService', 'ListLeases', request, ListLeasesResponse());
}

const $core.bool _omitFieldNames =
    $core.bool.fromEnvironment('protobuf.omit_field_names');
const $core.bool _omitMessageNames =
    $core.bool.fromEnvironment('protobuf.omit_message_names');
