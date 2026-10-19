// This is a generated file - do not edit.
//
// Generated from protectorate/v1/tailnet.proto.

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
import 'package:protobuf/well_known_types/google/protobuf/timestamp.pb.dart'
    as $0;

export 'package:protobuf/protobuf.dart' show GeneratedMessageGenericExtensions;

class TailnetServiceGetStatusRequest extends $pb.GeneratedMessage {
  factory TailnetServiceGetStatusRequest() =>
      TailnetServiceGetStatusRequest._();

  TailnetServiceGetStatusRequest._();

  factory TailnetServiceGetStatusRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      TailnetServiceGetStatusRequest()..mergeFromBuffer(data, registry);
  factory TailnetServiceGetStatusRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      TailnetServiceGetStatusRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'TailnetServiceGetStatusRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: TailnetServiceGetStatusRequest.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  TailnetServiceGetStatusRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  TailnetServiceGetStatusRequest copyWith(
          void Function(TailnetServiceGetStatusRequest) updates) =>
      super.copyWith(
              (message) => updates(message as TailnetServiceGetStatusRequest))
          as TailnetServiceGetStatusRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use TailnetServiceGetStatusRequest() / TailnetServiceGetStatusRequest.new instead')
  static TailnetServiceGetStatusRequest create() =>
      TailnetServiceGetStatusRequest._();
  static $pb.GeneratedMessage $_createMessage() =>
      TailnetServiceGetStatusRequest._();
  @$core.override
  TailnetServiceGetStatusRequest createEmptyInstance() =>
      TailnetServiceGetStatusRequest._();
  @$core.pragma('dart2js:noInline')
  static TailnetServiceGetStatusRequest getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<TailnetServiceGetStatusRequest>(
          TailnetServiceGetStatusRequest.$_createMessage);
  static TailnetServiceGetStatusRequest? _defaultInstance;
}

class TailnetServiceGetStatusResponse extends $pb.GeneratedMessage {
  factory TailnetServiceGetStatusResponse({
    $core.String? backendState,
    TailnetPeer? self,
    $core.Iterable<TailnetPeer>? peers,
    $core.Iterable<$core.String>? health,
  }) {
    final result = TailnetServiceGetStatusResponse._();
    if (backendState != null) result.backendState = backendState;
    if (self != null) result.self = self;
    if (peers != null) result.peers.addAll(peers);
    if (health != null) result.health.addAll(health);
    return result;
  }

  TailnetServiceGetStatusResponse._();

  factory TailnetServiceGetStatusResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      TailnetServiceGetStatusResponse()..mergeFromBuffer(data, registry);
  factory TailnetServiceGetStatusResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      TailnetServiceGetStatusResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'TailnetServiceGetStatusResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: TailnetServiceGetStatusResponse.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'backendState')
    ..aOM<TailnetPeer>(2, _omitFieldNames ? '' : 'self',
        subBuilder: TailnetPeer.$_createMessage)
    ..pPM<TailnetPeer>(3, _omitFieldNames ? '' : 'peers',
        subBuilder: TailnetPeer.$_createMessage)
    ..pPS(4, _omitFieldNames ? '' : 'health')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  TailnetServiceGetStatusResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  TailnetServiceGetStatusResponse copyWith(
          void Function(TailnetServiceGetStatusResponse) updates) =>
      super.copyWith(
              (message) => updates(message as TailnetServiceGetStatusResponse))
          as TailnetServiceGetStatusResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use TailnetServiceGetStatusResponse() / TailnetServiceGetStatusResponse.new instead')
  static TailnetServiceGetStatusResponse create() =>
      TailnetServiceGetStatusResponse._();
  static $pb.GeneratedMessage $_createMessage() =>
      TailnetServiceGetStatusResponse._();
  @$core.override
  TailnetServiceGetStatusResponse createEmptyInstance() =>
      TailnetServiceGetStatusResponse._();
  @$core.pragma('dart2js:noInline')
  static TailnetServiceGetStatusResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<TailnetServiceGetStatusResponse>(
          TailnetServiceGetStatusResponse.$_createMessage);
  static TailnetServiceGetStatusResponse? _defaultInstance;

  /// backend_state is the tsnet state, e.g. Running or NeedsLogin.
  @$pb.TagNumber(1)
  $core.String get backendState => $_getSZ(0);
  @$pb.TagNumber(1)
  set backendState($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasBackendState() => $_has(0);
  @$pb.TagNumber(1)
  void clearBackendState() => $_clearField(1);

  /// self is the hub's own tailnet node; its path fields are empty.
  @$pb.TagNumber(2)
  TailnetPeer get self => $_getN(1);
  @$pb.TagNumber(2)
  set self(TailnetPeer value) => $_setField(2, value);
  @$pb.TagNumber(2)
  $core.bool hasSelf() => $_has(1);
  @$pb.TagNumber(2)
  void clearSelf() => $_clearField(2);
  @$pb.TagNumber(2)
  TailnetPeer ensureSelf() => $_ensure(1);

  @$pb.TagNumber(3)
  $pb.PbList<TailnetPeer> get peers => $_getList(2);

  /// health lists the problems tsnet currently reports.
  @$pb.TagNumber(4)
  $pb.PbList<$core.String> get health => $_getList(3);
}

class TailnetPeer extends $pb.GeneratedMessage {
  factory TailnetPeer({
    $core.String? name,
    $core.Iterable<$core.String>? tags,
    $core.Iterable<$core.String>? ips,
    $core.bool? online,
    $0.Timestamp? lastHandshake,
    $0.Timestamp? lastSeen,
    $core.String? directAddr,
    $core.String? relay,
    $fixnum.Int64? rxBytes,
    $fixnum.Int64? txBytes,
  }) {
    final result = TailnetPeer._();
    if (name != null) result.name = name;
    if (tags != null) result.tags.addAll(tags);
    if (ips != null) result.ips.addAll(ips);
    if (online != null) result.online = online;
    if (lastHandshake != null) result.lastHandshake = lastHandshake;
    if (lastSeen != null) result.lastSeen = lastSeen;
    if (directAddr != null) result.directAddr = directAddr;
    if (relay != null) result.relay = relay;
    if (rxBytes != null) result.rxBytes = rxBytes;
    if (txBytes != null) result.txBytes = txBytes;
    return result;
  }

  TailnetPeer._();

  factory TailnetPeer.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      TailnetPeer()..mergeFromBuffer(data, registry);
  factory TailnetPeer.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      TailnetPeer()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'TailnetPeer',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: TailnetPeer.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'name')
    ..pPS(2, _omitFieldNames ? '' : 'tags')
    ..pPS(3, _omitFieldNames ? '' : 'ips')
    ..aOB(4, _omitFieldNames ? '' : 'online')
    ..aOM<$0.Timestamp>(5, _omitFieldNames ? '' : 'lastHandshake',
        subBuilder: $0.Timestamp.$_createMessage)
    ..aOM<$0.Timestamp>(6, _omitFieldNames ? '' : 'lastSeen',
        subBuilder: $0.Timestamp.$_createMessage)
    ..aOS(7, _omitFieldNames ? '' : 'directAddr')
    ..aOS(8, _omitFieldNames ? '' : 'relay')
    ..aInt64(9, _omitFieldNames ? '' : 'rxBytes')
    ..aInt64(10, _omitFieldNames ? '' : 'txBytes')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  TailnetPeer clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  TailnetPeer copyWith(void Function(TailnetPeer) updates) =>
      super.copyWith((message) => updates(message as TailnetPeer))
          as TailnetPeer;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use TailnetPeer() / TailnetPeer.new instead')
  static TailnetPeer create() => TailnetPeer._();
  static $pb.GeneratedMessage $_createMessage() => TailnetPeer._();
  @$core.override
  TailnetPeer createEmptyInstance() => TailnetPeer._();
  @$core.pragma('dart2js:noInline')
  static TailnetPeer getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<TailnetPeer>(
          TailnetPeer.$_createMessage);
  static TailnetPeer? _defaultInstance;

  /// name is the MagicDNS name without the trailing dot, or the host name.
  @$pb.TagNumber(1)
  $core.String get name => $_getSZ(0);
  @$pb.TagNumber(1)
  set name($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasName() => $_has(0);
  @$pb.TagNumber(1)
  void clearName() => $_clearField(1);

  @$pb.TagNumber(2)
  $pb.PbList<$core.String> get tags => $_getList(1);

  @$pb.TagNumber(3)
  $pb.PbList<$core.String> get ips => $_getList(2);

  /// online is whether the control server sees the node connected.
  @$pb.TagNumber(4)
  $core.bool get online => $_getBF(3);
  @$pb.TagNumber(4)
  set online($core.bool value) => $_setBool(3, value);
  @$pb.TagNumber(4)
  $core.bool hasOnline() => $_has(3);
  @$pb.TagNumber(4)
  void clearOnline() => $_clearField(4);

  /// last_handshake is the last WireGuard handshake; unset if none yet.
  @$pb.TagNumber(5)
  $0.Timestamp get lastHandshake => $_getN(4);
  @$pb.TagNumber(5)
  set lastHandshake($0.Timestamp value) => $_setField(5, value);
  @$pb.TagNumber(5)
  $core.bool hasLastHandshake() => $_has(4);
  @$pb.TagNumber(5)
  void clearLastHandshake() => $_clearField(5);
  @$pb.TagNumber(5)
  $0.Timestamp ensureLastHandshake() => $_ensure(4);

  /// last_seen is when the control server last saw the node; unset while online.
  @$pb.TagNumber(6)
  $0.Timestamp get lastSeen => $_getN(5);
  @$pb.TagNumber(6)
  set lastSeen($0.Timestamp value) => $_setField(6, value);
  @$pb.TagNumber(6)
  $core.bool hasLastSeen() => $_has(5);
  @$pb.TagNumber(6)
  void clearLastSeen() => $_clearField(6);
  @$pb.TagNumber(6)
  $0.Timestamp ensureLastSeen() => $_ensure(5);

  /// direct_addr is the UDP address in use for a direct path; empty if the
  /// path is relayed or not established.
  @$pb.TagNumber(7)
  $core.String get directAddr => $_getSZ(6);
  @$pb.TagNumber(7)
  set directAddr($core.String value) => $_setString(6, value);
  @$pb.TagNumber(7)
  $core.bool hasDirectAddr() => $_has(6);
  @$pb.TagNumber(7)
  void clearDirectAddr() => $_clearField(7);

  /// relay is the DERP region the peer is reached through, e.g. "protectorate".
  @$pb.TagNumber(8)
  $core.String get relay => $_getSZ(7);
  @$pb.TagNumber(8)
  set relay($core.String value) => $_setString(7, value);
  @$pb.TagNumber(8)
  $core.bool hasRelay() => $_has(7);
  @$pb.TagNumber(8)
  void clearRelay() => $_clearField(8);

  @$pb.TagNumber(9)
  $fixnum.Int64 get rxBytes => $_getI64(8);
  @$pb.TagNumber(9)
  set rxBytes($fixnum.Int64 value) => $_setInt64(8, value);
  @$pb.TagNumber(9)
  $core.bool hasRxBytes() => $_has(8);
  @$pb.TagNumber(9)
  void clearRxBytes() => $_clearField(9);

  @$pb.TagNumber(10)
  $fixnum.Int64 get txBytes => $_getI64(9);
  @$pb.TagNumber(10)
  set txBytes($fixnum.Int64 value) => $_setInt64(9, value);
  @$pb.TagNumber(10)
  $core.bool hasTxBytes() => $_has(9);
  @$pb.TagNumber(10)
  void clearTxBytes() => $_clearField(10);
}

/// TailnetService exposes the hub's own view of the tailnet for debugging.
class TailnetServiceApi {
  final $pb.RpcClient _client;

  TailnetServiceApi(this._client);

  /// GetStatus returns the hub's tailnet identity and every peer it knows.
  /// FAILED_PRECONDITION when the tailnet is not enabled on this hub.
  $async.Future<TailnetServiceGetStatusResponse> getStatus(
          $pb.ClientContext? ctx, TailnetServiceGetStatusRequest request) =>
      _client.invoke<TailnetServiceGetStatusResponse>(ctx, 'TailnetService',
          'GetStatus', request, TailnetServiceGetStatusResponse());
}

const $core.bool _omitFieldNames =
    $core.bool.fromEnvironment('protobuf.omit_field_names');
const $core.bool _omitMessageNames =
    $core.bool.fromEnvironment('protobuf.omit_message_names');
