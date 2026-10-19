// This is a generated file - do not edit.
//
// Generated from protectorate/v1/quota.proto.

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
    as $1;
import 'package:protobuf/well_known_types/google/protobuf/timestamp.pb.dart'
    as $0;

import 'quota.pbenum.dart';

export 'package:protobuf/protobuf.dart' show GeneratedMessageGenericExtensions;

export 'quota.pbenum.dart';

class GetQuotaRequest extends $pb.GeneratedMessage {
  factory GetQuotaRequest({
    $core.bool? refresh,
  }) {
    final result = GetQuotaRequest._();
    if (refresh != null) result.refresh = refresh;
    return result;
  }

  GetQuotaRequest._();

  factory GetQuotaRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      GetQuotaRequest()..mergeFromBuffer(data, registry);
  factory GetQuotaRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      GetQuotaRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'GetQuotaRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: GetQuotaRequest.$_createMessage)
    ..aOB(1, _omitFieldNames ? '' : 'refresh')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  GetQuotaRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  GetQuotaRequest copyWith(void Function(GetQuotaRequest) updates) =>
      super.copyWith((message) => updates(message as GetQuotaRequest))
          as GetQuotaRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use GetQuotaRequest() / GetQuotaRequest.new instead')
  static GetQuotaRequest create() => GetQuotaRequest._();
  static $pb.GeneratedMessage $_createMessage() => GetQuotaRequest._();
  @$core.override
  GetQuotaRequest createEmptyInstance() => GetQuotaRequest._();
  @$core.pragma('dart2js:noInline')
  static GetQuotaRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<GetQuotaRequest>(
          GetQuotaRequest.$_createMessage);
  static GetQuotaRequest? _defaultInstance;

  /// refresh forces an immediate read of each provider, rate-limited per
  /// provider; providers read too recently keep their cached data.
  @$pb.TagNumber(1)
  $core.bool get refresh => $_getBF(0);
  @$pb.TagNumber(1)
  set refresh($core.bool value) => $_setBool(0, value);
  @$pb.TagNumber(1)
  $core.bool hasRefresh() => $_has(0);
  @$pb.TagNumber(1)
  void clearRefresh() => $_clearField(1);
}

class GetQuotaResponse extends $pb.GeneratedMessage {
  factory GetQuotaResponse({
    $core.Iterable<ProviderQuota>? quotas,
  }) {
    final result = GetQuotaResponse._();
    if (quotas != null) result.quotas.addAll(quotas);
    return result;
  }

  GetQuotaResponse._();

  factory GetQuotaResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      GetQuotaResponse()..mergeFromBuffer(data, registry);
  factory GetQuotaResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      GetQuotaResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'GetQuotaResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: GetQuotaResponse.$_createMessage)
    ..pPM<ProviderQuota>(1, _omitFieldNames ? '' : 'quotas',
        subBuilder: ProviderQuota.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  GetQuotaResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  GetQuotaResponse copyWith(void Function(GetQuotaResponse) updates) =>
      super.copyWith((message) => updates(message as GetQuotaResponse))
          as GetQuotaResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use GetQuotaResponse() / GetQuotaResponse.new instead')
  static GetQuotaResponse create() => GetQuotaResponse._();
  static $pb.GeneratedMessage $_createMessage() => GetQuotaResponse._();
  @$core.override
  GetQuotaResponse createEmptyInstance() => GetQuotaResponse._();
  @$core.pragma('dart2js:noInline')
  static GetQuotaResponse getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<GetQuotaResponse>(
          GetQuotaResponse.$_createMessage);
  static GetQuotaResponse? _defaultInstance;

  @$pb.TagNumber(1)
  $pb.PbList<ProviderQuota> get quotas => $_getList(0);
}

class ProviderQuota extends $pb.GeneratedMessage {
  factory ProviderQuota({
    $core.String? provider,
    $core.String? plan,
    $core.String? source,
    Freshness? freshness,
    $0.Timestamp? fetchedAt,
    $1.Duration? age,
    $core.String? error,
    $0.Timestamp? checkedAt,
    $core.Iterable<QuotaWindow>? windows,
    $core.Iterable<ResetCredit>? resets,
    $core.Iterable<$core.String>? models,
    $1.Duration? latency,
  }) {
    final result = ProviderQuota._();
    if (provider != null) result.provider = provider;
    if (plan != null) result.plan = plan;
    if (source != null) result.source = source;
    if (freshness != null) result.freshness = freshness;
    if (fetchedAt != null) result.fetchedAt = fetchedAt;
    if (age != null) result.age = age;
    if (error != null) result.error = error;
    if (checkedAt != null) result.checkedAt = checkedAt;
    if (windows != null) result.windows.addAll(windows);
    if (resets != null) result.resets.addAll(resets);
    if (models != null) result.models.addAll(models);
    if (latency != null) result.latency = latency;
    return result;
  }

  ProviderQuota._();

  factory ProviderQuota.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ProviderQuota()..mergeFromBuffer(data, registry);
  factory ProviderQuota.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ProviderQuota()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ProviderQuota',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ProviderQuota.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'provider')
    ..aOS(2, _omitFieldNames ? '' : 'plan')
    ..aOS(3, _omitFieldNames ? '' : 'source')
    ..aE<Freshness>(4, _omitFieldNames ? '' : 'freshness',
        enumValues: Freshness.values)
    ..aOM<$0.Timestamp>(5, _omitFieldNames ? '' : 'fetchedAt',
        subBuilder: $0.Timestamp.$_createMessage)
    ..aOM<$1.Duration>(6, _omitFieldNames ? '' : 'age',
        subBuilder: $1.Duration.$_createMessage)
    ..aOS(7, _omitFieldNames ? '' : 'error')
    ..aOM<$0.Timestamp>(8, _omitFieldNames ? '' : 'checkedAt',
        subBuilder: $0.Timestamp.$_createMessage)
    ..pPM<QuotaWindow>(9, _omitFieldNames ? '' : 'windows',
        subBuilder: QuotaWindow.$_createMessage)
    ..pPM<ResetCredit>(10, _omitFieldNames ? '' : 'resets',
        subBuilder: ResetCredit.$_createMessage)
    ..pPS(11, _omitFieldNames ? '' : 'models')
    ..aOM<$1.Duration>(12, _omitFieldNames ? '' : 'latency',
        subBuilder: $1.Duration.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ProviderQuota clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ProviderQuota copyWith(void Function(ProviderQuota) updates) =>
      super.copyWith((message) => updates(message as ProviderQuota))
          as ProviderQuota;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use ProviderQuota() / ProviderQuota.new instead')
  static ProviderQuota create() => ProviderQuota._();
  static $pb.GeneratedMessage $_createMessage() => ProviderQuota._();
  @$core.override
  ProviderQuota createEmptyInstance() => ProviderQuota._();
  @$core.pragma('dart2js:noInline')
  static ProviderQuota getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<ProviderQuota>(
          ProviderQuota.$_createMessage);
  static ProviderQuota? _defaultInstance;

  /// provider is the subscription name, e.g. "claude" or "codex".
  @$pb.TagNumber(1)
  $core.String get provider => $_getSZ(0);
  @$pb.TagNumber(1)
  set provider($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasProvider() => $_has(0);
  @$pb.TagNumber(1)
  void clearProvider() => $_clearField(1);

  @$pb.TagNumber(2)
  $core.String get plan => $_getSZ(1);
  @$pb.TagNumber(2)
  set plan($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasPlan() => $_has(1);
  @$pb.TagNumber(2)
  void clearPlan() => $_clearField(2);

  /// source names where the numbers were read from.
  @$pb.TagNumber(3)
  $core.String get source => $_getSZ(2);
  @$pb.TagNumber(3)
  set source($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasSource() => $_has(2);
  @$pb.TagNumber(3)
  void clearSource() => $_clearField(3);

  @$pb.TagNumber(4)
  Freshness get freshness => $_getN(3);
  @$pb.TagNumber(4)
  set freshness(Freshness value) => $_setField(4, value);
  @$pb.TagNumber(4)
  $core.bool hasFreshness() => $_has(3);
  @$pb.TagNumber(4)
  void clearFreshness() => $_clearField(4);

  /// fetched_at is when the last good read completed; unset when UNKNOWN.
  @$pb.TagNumber(5)
  $0.Timestamp get fetchedAt => $_getN(4);
  @$pb.TagNumber(5)
  set fetchedAt($0.Timestamp value) => $_setField(5, value);
  @$pb.TagNumber(5)
  $core.bool hasFetchedAt() => $_has(4);
  @$pb.TagNumber(5)
  void clearFetchedAt() => $_clearField(5);
  @$pb.TagNumber(5)
  $0.Timestamp ensureFetchedAt() => $_ensure(4);

  /// age is the time since fetched_at at response time; unset when UNKNOWN.
  @$pb.TagNumber(6)
  $1.Duration get age => $_getN(5);
  @$pb.TagNumber(6)
  set age($1.Duration value) => $_setField(6, value);
  @$pb.TagNumber(6)
  $core.bool hasAge() => $_has(5);
  @$pb.TagNumber(6)
  void clearAge() => $_clearField(6);
  @$pb.TagNumber(6)
  $1.Duration ensureAge() => $_ensure(5);

  /// error is the failure of the most recent read, empty if it succeeded.
  @$pb.TagNumber(7)
  $core.String get error => $_getSZ(6);
  @$pb.TagNumber(7)
  set error($core.String value) => $_setString(6, value);
  @$pb.TagNumber(7)
  $core.bool hasError() => $_has(6);
  @$pb.TagNumber(7)
  void clearError() => $_clearField(7);

  /// checked_at is when the most recent read was attempted.
  @$pb.TagNumber(8)
  $0.Timestamp get checkedAt => $_getN(7);
  @$pb.TagNumber(8)
  set checkedAt($0.Timestamp value) => $_setField(8, value);
  @$pb.TagNumber(8)
  $core.bool hasCheckedAt() => $_has(7);
  @$pb.TagNumber(8)
  void clearCheckedAt() => $_clearField(8);
  @$pb.TagNumber(8)
  $0.Timestamp ensureCheckedAt() => $_ensure(7);

  @$pb.TagNumber(9)
  $pb.PbList<QuotaWindow> get windows => $_getList(8);

  @$pb.TagNumber(10)
  $pb.PbList<ResetCredit> get resets => $_getList(9);

  @$pb.TagNumber(11)
  $pb.PbList<$core.String> get models => $_getList(10);

  @$pb.TagNumber(12)
  $1.Duration get latency => $_getN(11);
  @$pb.TagNumber(12)
  set latency($1.Duration value) => $_setField(12, value);
  @$pb.TagNumber(12)
  $core.bool hasLatency() => $_has(11);
  @$pb.TagNumber(12)
  void clearLatency() => $_clearField(12);
  @$pb.TagNumber(12)
  $1.Duration ensureLatency() => $_ensure(11);
}

class QuotaWindow extends $pb.GeneratedMessage {
  factory QuotaWindow({
    $core.String? name,
    $core.double? usedPercent,
    $0.Timestamp? resetsAt,
    $fixnum.Int64? windowSeconds,
    $core.double? estimatedUnusedPercentAtReset,
  }) {
    final result = QuotaWindow._();
    if (name != null) result.name = name;
    if (usedPercent != null) result.usedPercent = usedPercent;
    if (resetsAt != null) result.resetsAt = resetsAt;
    if (windowSeconds != null) result.windowSeconds = windowSeconds;
    if (estimatedUnusedPercentAtReset != null)
      result.estimatedUnusedPercentAtReset = estimatedUnusedPercentAtReset;
    return result;
  }

  QuotaWindow._();

  factory QuotaWindow.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      QuotaWindow()..mergeFromBuffer(data, registry);
  factory QuotaWindow.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      QuotaWindow()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'QuotaWindow',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: QuotaWindow.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'name')
    ..aD(2, _omitFieldNames ? '' : 'usedPercent')
    ..aOM<$0.Timestamp>(3, _omitFieldNames ? '' : 'resetsAt',
        subBuilder: $0.Timestamp.$_createMessage)
    ..aInt64(4, _omitFieldNames ? '' : 'windowSeconds')
    ..aD(5, _omitFieldNames ? '' : 'estimatedUnusedPercentAtReset')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  QuotaWindow clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  QuotaWindow copyWith(void Function(QuotaWindow) updates) =>
      super.copyWith((message) => updates(message as QuotaWindow))
          as QuotaWindow;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use QuotaWindow() / QuotaWindow.new instead')
  static QuotaWindow create() => QuotaWindow._();
  static $pb.GeneratedMessage $_createMessage() => QuotaWindow._();
  @$core.override
  QuotaWindow createEmptyInstance() => QuotaWindow._();
  @$core.pragma('dart2js:noInline')
  static QuotaWindow getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<QuotaWindow>(
          QuotaWindow.$_createMessage);
  static QuotaWindow? _defaultInstance;

  /// name identifies the window, e.g. "5h".
  @$pb.TagNumber(1)
  $core.String get name => $_getSZ(0);
  @$pb.TagNumber(1)
  set name($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasName() => $_has(0);
  @$pb.TagNumber(1)
  void clearName() => $_clearField(1);

  @$pb.TagNumber(2)
  $core.double get usedPercent => $_getN(1);
  @$pb.TagNumber(2)
  set usedPercent($core.double value) => $_setDouble(1, value);
  @$pb.TagNumber(2)
  $core.bool hasUsedPercent() => $_has(1);
  @$pb.TagNumber(2)
  void clearUsedPercent() => $_clearField(2);

  @$pb.TagNumber(3)
  $0.Timestamp get resetsAt => $_getN(2);
  @$pb.TagNumber(3)
  set resetsAt($0.Timestamp value) => $_setField(3, value);
  @$pb.TagNumber(3)
  $core.bool hasResetsAt() => $_has(2);
  @$pb.TagNumber(3)
  void clearResetsAt() => $_clearField(3);
  @$pb.TagNumber(3)
  $0.Timestamp ensureResetsAt() => $_ensure(2);

  /// window_seconds is the window length when known, else 0.
  @$pb.TagNumber(4)
  $fixnum.Int64 get windowSeconds => $_getI64(3);
  @$pb.TagNumber(4)
  set windowSeconds($fixnum.Int64 value) => $_setInt64(3, value);
  @$pb.TagNumber(4)
  $core.bool hasWindowSeconds() => $_has(3);
  @$pb.TagNumber(4)
  void clearWindowSeconds() => $_clearField(4);

  /// estimated_unused_percent_at_reset is an ESTIMATE of the percent still
  /// unused when the window resets, assuming usage keeps the average pace it
  /// had from window start to fetched_at. Unset when the window length or
  /// reset time is unknown, the window is too young for a pace, or it has
  /// already reset.
  @$pb.TagNumber(5)
  $core.double get estimatedUnusedPercentAtReset => $_getN(4);
  @$pb.TagNumber(5)
  set estimatedUnusedPercentAtReset($core.double value) =>
      $_setDouble(4, value);
  @$pb.TagNumber(5)
  $core.bool hasEstimatedUnusedPercentAtReset() => $_has(4);
  @$pb.TagNumber(5)
  void clearEstimatedUnusedPercentAtReset() => $_clearField(5);
}

class ResetCredit extends $pb.GeneratedMessage {
  factory ResetCredit({
    $core.String? kind,
    $core.int? count,
    $0.Timestamp? expiresAt,
  }) {
    final result = ResetCredit._();
    if (kind != null) result.kind = kind;
    if (count != null) result.count = count;
    if (expiresAt != null) result.expiresAt = expiresAt;
    return result;
  }

  ResetCredit._();

  factory ResetCredit.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ResetCredit()..mergeFromBuffer(data, registry);
  factory ResetCredit.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ResetCredit()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ResetCredit',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ResetCredit.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'kind')
    ..aI(2, _omitFieldNames ? '' : 'count')
    ..aOM<$0.Timestamp>(3, _omitFieldNames ? '' : 'expiresAt',
        subBuilder: $0.Timestamp.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ResetCredit clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ResetCredit copyWith(void Function(ResetCredit) updates) =>
      super.copyWith((message) => updates(message as ResetCredit))
          as ResetCredit;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use ResetCredit() / ResetCredit.new instead')
  static ResetCredit create() => ResetCredit._();
  static $pb.GeneratedMessage $_createMessage() => ResetCredit._();
  @$core.override
  ResetCredit createEmptyInstance() => ResetCredit._();
  @$core.pragma('dart2js:noInline')
  static ResetCredit getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<ResetCredit>(
          ResetCredit.$_createMessage);
  static ResetCredit? _defaultInstance;

  /// kind names the type of reset credit; count of them are available.
  @$pb.TagNumber(1)
  $core.String get kind => $_getSZ(0);
  @$pb.TagNumber(1)
  set kind($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasKind() => $_has(0);
  @$pb.TagNumber(1)
  void clearKind() => $_clearField(1);

  @$pb.TagNumber(2)
  $core.int get count => $_getIZ(1);
  @$pb.TagNumber(2)
  set count($core.int value) => $_setSignedInt32(1, value);
  @$pb.TagNumber(2)
  $core.bool hasCount() => $_has(1);
  @$pb.TagNumber(2)
  void clearCount() => $_clearField(2);

  @$pb.TagNumber(3)
  $0.Timestamp get expiresAt => $_getN(2);
  @$pb.TagNumber(3)
  set expiresAt($0.Timestamp value) => $_setField(3, value);
  @$pb.TagNumber(3)
  $core.bool hasExpiresAt() => $_has(2);
  @$pb.TagNumber(3)
  void clearExpiresAt() => $_clearField(3);
  @$pb.TagNumber(3)
  $0.Timestamp ensureExpiresAt() => $_ensure(2);
}

/// QuotaService reports subscription usage. Updates are also announced on
/// EventService as events with kind "quota.updated" (subject = provider,
/// payload = the ProviderQuota in proto-JSON form).
class QuotaServiceApi {
  final $pb.RpcClient _client;

  QuotaServiceApi(this._client);

  /// GetQuota returns the usage windows, resets and freshness of every
  /// provider subscription (Claude, Codex, Grok, Antigravity, local LLMs).
  $async.Future<GetQuotaResponse> getQuota(
          $pb.ClientContext? ctx, GetQuotaRequest request) =>
      _client.invoke<GetQuotaResponse>(
          ctx, 'QuotaService', 'GetQuota', request, GetQuotaResponse());
}

const $core.bool _omitFieldNames =
    $core.bool.fromEnvironment('protobuf.omit_field_names');
const $core.bool _omitMessageNames =
    $core.bool.fromEnvironment('protobuf.omit_message_names');
