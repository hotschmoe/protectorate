// This is a generated file - do not edit.
//
// Generated from protectorate/v1/token.proto.

// @dart = 3.3

// ignore_for_file: annotate_overrides, camel_case_types, comment_references
// ignore_for_file: constant_identifier_names
// ignore_for_file: curly_braces_in_flow_control_structures
// ignore_for_file: deprecated_member_use_from_same_package, library_prefixes
// ignore_for_file: non_constant_identifier_names, prefer_relative_imports

import 'dart:async' as $async;
import 'dart:core' as $core;

import 'package:protobuf/protobuf.dart' as $pb;
import 'package:protobuf/well_known_types/google/protobuf/duration.pb.dart'
    as $0;
import 'package:protobuf/well_known_types/google/protobuf/timestamp.pb.dart'
    as $1;

export 'package:protobuf/protobuf.dart' show GeneratedMessageGenericExtensions;

class GrantServiceAdminRequest extends $pb.GeneratedMessage {
  factory GrantServiceAdminRequest({
    $core.String? node,
    $0.Duration? ttl,
    $core.String? task,
  }) {
    final result = GrantServiceAdminRequest._();
    if (node != null) result.node = node;
    if (ttl != null) result.ttl = ttl;
    if (task != null) result.task = task;
    return result;
  }

  GrantServiceAdminRequest._();

  factory GrantServiceAdminRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      GrantServiceAdminRequest()..mergeFromBuffer(data, registry);
  factory GrantServiceAdminRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      GrantServiceAdminRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'GrantServiceAdminRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: GrantServiceAdminRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'node')
    ..aOM<$0.Duration>(2, _omitFieldNames ? '' : 'ttl',
        subBuilder: $0.Duration.$_createMessage)
    ..aOS(3, _omitFieldNames ? '' : 'task')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  GrantServiceAdminRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  GrantServiceAdminRequest copyWith(
          void Function(GrantServiceAdminRequest) updates) =>
      super.copyWith((message) => updates(message as GrantServiceAdminRequest))
          as GrantServiceAdminRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use GrantServiceAdminRequest() / GrantServiceAdminRequest.new instead')
  static GrantServiceAdminRequest create() => GrantServiceAdminRequest._();
  static $pb.GeneratedMessage $_createMessage() => GrantServiceAdminRequest._();
  @$core.override
  GrantServiceAdminRequest createEmptyInstance() =>
      GrantServiceAdminRequest._();
  @$core.pragma('dart2js:noInline')
  static GrantServiceAdminRequest getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<GrantServiceAdminRequest>(
          GrantServiceAdminRequest.$_createMessage);
  static GrantServiceAdminRequest? _defaultInstance;

  /// node is a node name, or (owner only) a path.Match glob over node names.
  @$pb.TagNumber(1)
  $core.String get node => $_getSZ(0);
  @$pb.TagNumber(1)
  set node($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasNode() => $_has(0);
  @$pb.TagNumber(1)
  void clearNode() => $_clearField(1);

  /// ttl is the grant's lifetime; unset means 4h, at most 24h.
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

  /// task says what the grant is for, e.g. "update the kids' machine". It is
  /// stored with the token and written to every audit line the grant causes.
  @$pb.TagNumber(3)
  $core.String get task => $_getSZ(2);
  @$pb.TagNumber(3)
  set task($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasTask() => $_has(2);
  @$pb.TagNumber(3)
  void clearTask() => $_clearField(3);
}

class GrantServiceAdminResponse extends $pb.GeneratedMessage {
  factory GrantServiceAdminResponse({
    TokenInfo? token,
    $core.String? secret,
  }) {
    final result = GrantServiceAdminResponse._();
    if (token != null) result.token = token;
    if (secret != null) result.secret = secret;
    return result;
  }

  GrantServiceAdminResponse._();

  factory GrantServiceAdminResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      GrantServiceAdminResponse()..mergeFromBuffer(data, registry);
  factory GrantServiceAdminResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      GrantServiceAdminResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'GrantServiceAdminResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: GrantServiceAdminResponse.$_createMessage)
    ..aOM<TokenInfo>(1, _omitFieldNames ? '' : 'token',
        subBuilder: TokenInfo.$_createMessage)
    ..aOS(2, _omitFieldNames ? '' : 'secret')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  GrantServiceAdminResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  GrantServiceAdminResponse copyWith(
          void Function(GrantServiceAdminResponse) updates) =>
      super.copyWith((message) => updates(message as GrantServiceAdminResponse))
          as GrantServiceAdminResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use GrantServiceAdminResponse() / GrantServiceAdminResponse.new instead')
  static GrantServiceAdminResponse create() => GrantServiceAdminResponse._();
  static $pb.GeneratedMessage $_createMessage() =>
      GrantServiceAdminResponse._();
  @$core.override
  GrantServiceAdminResponse createEmptyInstance() =>
      GrantServiceAdminResponse._();
  @$core.pragma('dart2js:noInline')
  static GrantServiceAdminResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<GrantServiceAdminResponse>(
          GrantServiceAdminResponse.$_createMessage);
  static GrantServiceAdminResponse? _defaultInstance;

  @$pb.TagNumber(1)
  TokenInfo get token => $_getN(0);
  @$pb.TagNumber(1)
  set token(TokenInfo value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasToken() => $_has(0);
  @$pb.TagNumber(1)
  void clearToken() => $_clearField(1);
  @$pb.TagNumber(1)
  TokenInfo ensureToken() => $_ensure(0);

  /// secret is the bearer token. It is shown only in this response.
  @$pb.TagNumber(2)
  $core.String get secret => $_getSZ(1);
  @$pb.TagNumber(2)
  set secret($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasSecret() => $_has(1);
  @$pb.TagNumber(2)
  void clearSecret() => $_clearField(2);
}

/// TokenInfo describes an issued token. It never carries the secret.
class TokenInfo extends $pb.GeneratedMessage {
  factory TokenInfo({
    $core.String? id,
    $core.String? name,
    $core.Iterable<$core.String>? scopes,
    $1.Timestamp? createdAt,
    $1.Timestamp? expiresAt,
    $1.Timestamp? lastUsedAt,
    $1.Timestamp? revokedAt,
    $core.String? task,
    $core.String? mintedBy,
  }) {
    final result = TokenInfo._();
    if (id != null) result.id = id;
    if (name != null) result.name = name;
    if (scopes != null) result.scopes.addAll(scopes);
    if (createdAt != null) result.createdAt = createdAt;
    if (expiresAt != null) result.expiresAt = expiresAt;
    if (lastUsedAt != null) result.lastUsedAt = lastUsedAt;
    if (revokedAt != null) result.revokedAt = revokedAt;
    if (task != null) result.task = task;
    if (mintedBy != null) result.mintedBy = mintedBy;
    return result;
  }

  TokenInfo._();

  factory TokenInfo.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      TokenInfo()..mergeFromBuffer(data, registry);
  factory TokenInfo.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      TokenInfo()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'TokenInfo',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: TokenInfo.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'id')
    ..aOS(2, _omitFieldNames ? '' : 'name')
    ..pPS(3, _omitFieldNames ? '' : 'scopes')
    ..aOM<$1.Timestamp>(4, _omitFieldNames ? '' : 'createdAt',
        subBuilder: $1.Timestamp.$_createMessage)
    ..aOM<$1.Timestamp>(5, _omitFieldNames ? '' : 'expiresAt',
        subBuilder: $1.Timestamp.$_createMessage)
    ..aOM<$1.Timestamp>(6, _omitFieldNames ? '' : 'lastUsedAt',
        subBuilder: $1.Timestamp.$_createMessage)
    ..aOM<$1.Timestamp>(7, _omitFieldNames ? '' : 'revokedAt',
        subBuilder: $1.Timestamp.$_createMessage)
    ..aOS(8, _omitFieldNames ? '' : 'task')
    ..aOS(9, _omitFieldNames ? '' : 'mintedBy')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  TokenInfo clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  TokenInfo copyWith(void Function(TokenInfo) updates) =>
      super.copyWith((message) => updates(message as TokenInfo)) as TokenInfo;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use TokenInfo() / TokenInfo.new instead')
  static TokenInfo create() => TokenInfo._();
  static $pb.GeneratedMessage $_createMessage() => TokenInfo._();
  @$core.override
  TokenInfo createEmptyInstance() => TokenInfo._();
  @$core.pragma('dart2js:noInline')
  static TokenInfo getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<TokenInfo>(TokenInfo.$_createMessage);
  static TokenInfo? _defaultInstance;

  /// id is the stable identifier used to revoke the token and in audit lines.
  @$pb.TagNumber(1)
  $core.String get id => $_getSZ(0);
  @$pb.TagNumber(1)
  set id($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasId() => $_has(0);
  @$pb.TagNumber(1)
  void clearId() => $_clearField(1);

  /// name is the label given at creation.
  @$pb.TagNumber(2)
  $core.String get name => $_getSZ(1);
  @$pb.TagNumber(2)
  set name($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasName() => $_has(1);
  @$pb.TagNumber(2)
  void clearName() => $_clearField(2);

  /// scopes are the grants: tool-name globs such as "session_*", the special
  /// scopes read, events, terminal and terminal:readonly, and on an admin
  /// grant node_admin:<node>.
  @$pb.TagNumber(3)
  $pb.PbList<$core.String> get scopes => $_getList(2);

  @$pb.TagNumber(4)
  $1.Timestamp get createdAt => $_getN(3);
  @$pb.TagNumber(4)
  set createdAt($1.Timestamp value) => $_setField(4, value);
  @$pb.TagNumber(4)
  $core.bool hasCreatedAt() => $_has(3);
  @$pb.TagNumber(4)
  void clearCreatedAt() => $_clearField(4);
  @$pb.TagNumber(4)
  $1.Timestamp ensureCreatedAt() => $_ensure(3);

  /// expires_at is unset for a token that never expires.
  @$pb.TagNumber(5)
  $1.Timestamp get expiresAt => $_getN(4);
  @$pb.TagNumber(5)
  set expiresAt($1.Timestamp value) => $_setField(5, value);
  @$pb.TagNumber(5)
  $core.bool hasExpiresAt() => $_has(4);
  @$pb.TagNumber(5)
  void clearExpiresAt() => $_clearField(5);
  @$pb.TagNumber(5)
  $1.Timestamp ensureExpiresAt() => $_ensure(4);

  /// last_used_at is unset until the token authenticates once. It is
  /// persisted at most about once a minute.
  @$pb.TagNumber(6)
  $1.Timestamp get lastUsedAt => $_getN(5);
  @$pb.TagNumber(6)
  set lastUsedAt($1.Timestamp value) => $_setField(6, value);
  @$pb.TagNumber(6)
  $core.bool hasLastUsedAt() => $_has(5);
  @$pb.TagNumber(6)
  void clearLastUsedAt() => $_clearField(6);
  @$pb.TagNumber(6)
  $1.Timestamp ensureLastUsedAt() => $_ensure(5);

  /// revoked_at is unset while the token is not revoked.
  @$pb.TagNumber(7)
  $1.Timestamp get revokedAt => $_getN(6);
  @$pb.TagNumber(7)
  set revokedAt($1.Timestamp value) => $_setField(7, value);
  @$pb.TagNumber(7)
  $core.bool hasRevokedAt() => $_has(6);
  @$pb.TagNumber(7)
  void clearRevokedAt() => $_clearField(7);
  @$pb.TagNumber(7)
  $1.Timestamp ensureRevokedAt() => $_ensure(6);

  /// task is set on an admin grant: what it was minted for.
  @$pb.TagNumber(8)
  $core.String get task => $_getSZ(7);
  @$pb.TagNumber(8)
  set task($core.String value) => $_setString(7, value);
  @$pb.TagNumber(8)
  $core.bool hasTask() => $_has(7);
  @$pb.TagNumber(8)
  void clearTask() => $_clearField(8);

  /// minted_by names who minted an admin grant: "owner" or a token name.
  @$pb.TagNumber(9)
  $core.String get mintedBy => $_getSZ(8);
  @$pb.TagNumber(9)
  set mintedBy($core.String value) => $_setString(8, value);
  @$pb.TagNumber(9)
  $core.bool hasMintedBy() => $_has(8);
  @$pb.TagNumber(9)
  void clearMintedBy() => $_clearField(9);
}

class CreateTokenRequest extends $pb.GeneratedMessage {
  factory CreateTokenRequest({
    $core.String? name,
    $core.Iterable<$core.String>? scopes,
    $0.Duration? ttl,
  }) {
    final result = CreateTokenRequest._();
    if (name != null) result.name = name;
    if (scopes != null) result.scopes.addAll(scopes);
    if (ttl != null) result.ttl = ttl;
    return result;
  }

  CreateTokenRequest._();

  factory CreateTokenRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      CreateTokenRequest()..mergeFromBuffer(data, registry);
  factory CreateTokenRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      CreateTokenRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'CreateTokenRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: CreateTokenRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'name')
    ..pPS(2, _omitFieldNames ? '' : 'scopes')
    ..aOM<$0.Duration>(3, _omitFieldNames ? '' : 'ttl',
        subBuilder: $0.Duration.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  CreateTokenRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  CreateTokenRequest copyWith(void Function(CreateTokenRequest) updates) =>
      super.copyWith((message) => updates(message as CreateTokenRequest))
          as CreateTokenRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use CreateTokenRequest() / CreateTokenRequest.new instead')
  static CreateTokenRequest create() => CreateTokenRequest._();
  static $pb.GeneratedMessage $_createMessage() => CreateTokenRequest._();
  @$core.override
  CreateTokenRequest createEmptyInstance() => CreateTokenRequest._();
  @$core.pragma('dart2js:noInline')
  static CreateTokenRequest getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<CreateTokenRequest>(
          CreateTokenRequest.$_createMessage);
  static CreateTokenRequest? _defaultInstance;

  /// name labels the token, for example "pixel phone" or "mcp bridge".
  @$pb.TagNumber(1)
  $core.String get name => $_getSZ(0);
  @$pb.TagNumber(1)
  set name($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasName() => $_has(0);
  @$pb.TagNumber(1)
  void clearName() => $_clearField(1);

  /// scopes grant access; at least one is required and nothing is implied.
  @$pb.TagNumber(2)
  $pb.PbList<$core.String> get scopes => $_getList(1);

  /// ttl is the lifetime from now; zero means the token never expires.
  @$pb.TagNumber(3)
  $0.Duration get ttl => $_getN(2);
  @$pb.TagNumber(3)
  set ttl($0.Duration value) => $_setField(3, value);
  @$pb.TagNumber(3)
  $core.bool hasTtl() => $_has(2);
  @$pb.TagNumber(3)
  void clearTtl() => $_clearField(3);
  @$pb.TagNumber(3)
  $0.Duration ensureTtl() => $_ensure(2);
}

class CreateTokenResponse extends $pb.GeneratedMessage {
  factory CreateTokenResponse({
    TokenInfo? token,
    $core.String? secret,
  }) {
    final result = CreateTokenResponse._();
    if (token != null) result.token = token;
    if (secret != null) result.secret = secret;
    return result;
  }

  CreateTokenResponse._();

  factory CreateTokenResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      CreateTokenResponse()..mergeFromBuffer(data, registry);
  factory CreateTokenResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      CreateTokenResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'CreateTokenResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: CreateTokenResponse.$_createMessage)
    ..aOM<TokenInfo>(1, _omitFieldNames ? '' : 'token',
        subBuilder: TokenInfo.$_createMessage)
    ..aOS(2, _omitFieldNames ? '' : 'secret')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  CreateTokenResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  CreateTokenResponse copyWith(void Function(CreateTokenResponse) updates) =>
      super.copyWith((message) => updates(message as CreateTokenResponse))
          as CreateTokenResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core
      .Deprecated('Use CreateTokenResponse() / CreateTokenResponse.new instead')
  static CreateTokenResponse create() => CreateTokenResponse._();
  static $pb.GeneratedMessage $_createMessage() => CreateTokenResponse._();
  @$core.override
  CreateTokenResponse createEmptyInstance() => CreateTokenResponse._();
  @$core.pragma('dart2js:noInline')
  static CreateTokenResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<CreateTokenResponse>(
          CreateTokenResponse.$_createMessage);
  static CreateTokenResponse? _defaultInstance;

  @$pb.TagNumber(1)
  TokenInfo get token => $_getN(0);
  @$pb.TagNumber(1)
  set token(TokenInfo value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasToken() => $_has(0);
  @$pb.TagNumber(1)
  void clearToken() => $_clearField(1);
  @$pb.TagNumber(1)
  TokenInfo ensureToken() => $_ensure(0);

  /// secret is the bearer token. It is shown only in this response.
  @$pb.TagNumber(2)
  $core.String get secret => $_getSZ(1);
  @$pb.TagNumber(2)
  set secret($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasSecret() => $_has(1);
  @$pb.TagNumber(2)
  void clearSecret() => $_clearField(2);
}

class ListTokensRequest extends $pb.GeneratedMessage {
  factory ListTokensRequest() => ListTokensRequest._();

  ListTokensRequest._();

  factory ListTokensRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListTokensRequest()..mergeFromBuffer(data, registry);
  factory ListTokensRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListTokensRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ListTokensRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ListTokensRequest.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListTokensRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListTokensRequest copyWith(void Function(ListTokensRequest) updates) =>
      super.copyWith((message) => updates(message as ListTokensRequest))
          as ListTokensRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use ListTokensRequest() / ListTokensRequest.new instead')
  static ListTokensRequest create() => ListTokensRequest._();
  static $pb.GeneratedMessage $_createMessage() => ListTokensRequest._();
  @$core.override
  ListTokensRequest createEmptyInstance() => ListTokensRequest._();
  @$core.pragma('dart2js:noInline')
  static ListTokensRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<ListTokensRequest>(
          ListTokensRequest.$_createMessage);
  static ListTokensRequest? _defaultInstance;
}

class ListTokensResponse extends $pb.GeneratedMessage {
  factory ListTokensResponse({
    $core.Iterable<TokenInfo>? tokens,
  }) {
    final result = ListTokensResponse._();
    if (tokens != null) result.tokens.addAll(tokens);
    return result;
  }

  ListTokensResponse._();

  factory ListTokensResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListTokensResponse()..mergeFromBuffer(data, registry);
  factory ListTokensResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListTokensResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ListTokensResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ListTokensResponse.$_createMessage)
    ..pPM<TokenInfo>(1, _omitFieldNames ? '' : 'tokens',
        subBuilder: TokenInfo.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListTokensResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListTokensResponse copyWith(void Function(ListTokensResponse) updates) =>
      super.copyWith((message) => updates(message as ListTokensResponse))
          as ListTokensResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use ListTokensResponse() / ListTokensResponse.new instead')
  static ListTokensResponse create() => ListTokensResponse._();
  static $pb.GeneratedMessage $_createMessage() => ListTokensResponse._();
  @$core.override
  ListTokensResponse createEmptyInstance() => ListTokensResponse._();
  @$core.pragma('dart2js:noInline')
  static ListTokensResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<ListTokensResponse>(
          ListTokensResponse.$_createMessage);
  static ListTokensResponse? _defaultInstance;

  @$pb.TagNumber(1)
  $pb.PbList<TokenInfo> get tokens => $_getList(0);
}

class RevokeTokenRequest extends $pb.GeneratedMessage {
  factory RevokeTokenRequest({
    $core.String? id,
  }) {
    final result = RevokeTokenRequest._();
    if (id != null) result.id = id;
    return result;
  }

  RevokeTokenRequest._();

  factory RevokeTokenRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      RevokeTokenRequest()..mergeFromBuffer(data, registry);
  factory RevokeTokenRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      RevokeTokenRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'RevokeTokenRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: RevokeTokenRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'id')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  RevokeTokenRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  RevokeTokenRequest copyWith(void Function(RevokeTokenRequest) updates) =>
      super.copyWith((message) => updates(message as RevokeTokenRequest))
          as RevokeTokenRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use RevokeTokenRequest() / RevokeTokenRequest.new instead')
  static RevokeTokenRequest create() => RevokeTokenRequest._();
  static $pb.GeneratedMessage $_createMessage() => RevokeTokenRequest._();
  @$core.override
  RevokeTokenRequest createEmptyInstance() => RevokeTokenRequest._();
  @$core.pragma('dart2js:noInline')
  static RevokeTokenRequest getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<RevokeTokenRequest>(
          RevokeTokenRequest.$_createMessage);
  static RevokeTokenRequest? _defaultInstance;

  /// id of the token to revoke.
  @$pb.TagNumber(1)
  $core.String get id => $_getSZ(0);
  @$pb.TagNumber(1)
  set id($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasId() => $_has(0);
  @$pb.TagNumber(1)
  void clearId() => $_clearField(1);
}

class RevokeTokenResponse extends $pb.GeneratedMessage {
  factory RevokeTokenResponse({
    TokenInfo? token,
  }) {
    final result = RevokeTokenResponse._();
    if (token != null) result.token = token;
    return result;
  }

  RevokeTokenResponse._();

  factory RevokeTokenResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      RevokeTokenResponse()..mergeFromBuffer(data, registry);
  factory RevokeTokenResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      RevokeTokenResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'RevokeTokenResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: RevokeTokenResponse.$_createMessage)
    ..aOM<TokenInfo>(1, _omitFieldNames ? '' : 'token',
        subBuilder: TokenInfo.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  RevokeTokenResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  RevokeTokenResponse copyWith(void Function(RevokeTokenResponse) updates) =>
      super.copyWith((message) => updates(message as RevokeTokenResponse))
          as RevokeTokenResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core
      .Deprecated('Use RevokeTokenResponse() / RevokeTokenResponse.new instead')
  static RevokeTokenResponse create() => RevokeTokenResponse._();
  static $pb.GeneratedMessage $_createMessage() => RevokeTokenResponse._();
  @$core.override
  RevokeTokenResponse createEmptyInstance() => RevokeTokenResponse._();
  @$core.pragma('dart2js:noInline')
  static RevokeTokenResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<RevokeTokenResponse>(
          RevokeTokenResponse.$_createMessage);
  static RevokeTokenResponse? _defaultInstance;

  @$pb.TagNumber(1)
  TokenInfo get token => $_getN(0);
  @$pb.TagNumber(1)
  set token(TokenInfo value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasToken() => $_has(0);
  @$pb.TagNumber(1)
  void clearToken() => $_clearField(1);
  @$pb.TagNumber(1)
  TokenInfo ensureToken() => $_ensure(0);
}

/// TokenService issues and revokes scoped API tokens. Only the owner token (the
/// one in $PROTECTORATE_HOME/token) may call it; no scope grants access.
class TokenServiceApi {
  final $pb.RpcClient _client;

  TokenServiceApi(this._client);

  /// CreateToken issues a scoped token. The secret is returned once and only
  /// its SHA-256 hash is stored, so it cannot be recovered later.
  $async.Future<CreateTokenResponse> createToken(
          $pb.ClientContext? ctx, CreateTokenRequest request) =>
      _client.invoke<CreateTokenResponse>(
          ctx, 'TokenService', 'CreateToken', request, CreateTokenResponse());

  /// ListTokens returns every issued token, including revoked and expired
  /// ones, without secrets.
  $async.Future<ListTokensResponse> listTokens(
          $pb.ClientContext? ctx, ListTokensRequest request) =>
      _client.invoke<ListTokensResponse>(
          ctx, 'TokenService', 'ListTokens', request, ListTokensResponse());

  /// RevokeToken permanently disables a token. Revoking an already revoked
  /// token succeeds and keeps the original revocation time.
  $async.Future<RevokeTokenResponse> revokeToken(
          $pb.ClientContext? ctx, RevokeTokenRequest request) =>
      _client.invoke<RevokeTokenResponse>(
          ctx, 'TokenService', 'RevokeToken', request, RevokeTokenResponse());
}

/// GrantService mints task-scoped admin grants: short-lived tokens whose
/// scopes are job_* and node_admin:<node>. The owner may grant any node (or a
/// glob over node names); the orchestrator's token may grant one shared node
/// by name, and only when the hub runs with --orchestrator-admin-grants. Every
/// call made with a grant is audited with its task.
class GrantServiceApi {
  final $pb.RpcClient _client;

  GrantServiceApi(this._client);

  /// Admin mints a grant. The secret is returned once.
  $async.Future<GrantServiceAdminResponse> admin(
          $pb.ClientContext? ctx, GrantServiceAdminRequest request) =>
      _client.invoke<GrantServiceAdminResponse>(
          ctx, 'GrantService', 'Admin', request, GrantServiceAdminResponse());
}

const $core.bool _omitFieldNames =
    $core.bool.fromEnvironment('protobuf.omit_field_names');
const $core.bool _omitMessageNames =
    $core.bool.fromEnvironment('protobuf.omit_message_names');
