// This is a generated file - do not edit.
//
// Generated from protectorate/v1/session.proto.

// @dart = 3.3

// ignore_for_file: annotate_overrides, camel_case_types, comment_references
// ignore_for_file: constant_identifier_names
// ignore_for_file: curly_braces_in_flow_control_structures
// ignore_for_file: deprecated_member_use_from_same_package, library_prefixes
// ignore_for_file: non_constant_identifier_names, prefer_relative_imports

import 'dart:async' as $async;
import 'dart:core' as $core;

import 'package:protobuf/protobuf.dart' as $pb;
import 'package:protobuf/well_known_types/google/protobuf/timestamp.pb.dart'
    as $0;

import 'session.pbenum.dart';

export 'package:protobuf/protobuf.dart' show GeneratedMessageGenericExtensions;

export 'session.pbenum.dart';

/// Session is one tmux-backed harness session.
class Session extends $pb.GeneratedMessage {
  factory Session({
    $core.String? name,
    $core.String? cwd,
    $core.Iterable<$core.String>? command,
    $0.Timestamp? createdAt,
    $core.int? attachedClients,
    $0.Timestamp? lastActivity,
    $core.int? paneCols,
    $core.int? paneRows,
    SessionState? state,
    $core.int? exitCode,
    $core.int? exitSignal,
    $0.Timestamp? exitedAt,
  }) {
    final result = Session._();
    if (name != null) result.name = name;
    if (cwd != null) result.cwd = cwd;
    if (command != null) result.command.addAll(command);
    if (createdAt != null) result.createdAt = createdAt;
    if (attachedClients != null) result.attachedClients = attachedClients;
    if (lastActivity != null) result.lastActivity = lastActivity;
    if (paneCols != null) result.paneCols = paneCols;
    if (paneRows != null) result.paneRows = paneRows;
    if (state != null) result.state = state;
    if (exitCode != null) result.exitCode = exitCode;
    if (exitSignal != null) result.exitSignal = exitSignal;
    if (exitedAt != null) result.exitedAt = exitedAt;
    return result;
  }

  Session._();

  factory Session.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      Session()..mergeFromBuffer(data, registry);
  factory Session.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      Session()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'Session',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: Session.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'name')
    ..aOS(2, _omitFieldNames ? '' : 'cwd')
    ..pPS(3, _omitFieldNames ? '' : 'command')
    ..aOM<$0.Timestamp>(4, _omitFieldNames ? '' : 'createdAt',
        subBuilder: $0.Timestamp.$_createMessage)
    ..aI(5, _omitFieldNames ? '' : 'attachedClients')
    ..aOM<$0.Timestamp>(6, _omitFieldNames ? '' : 'lastActivity',
        subBuilder: $0.Timestamp.$_createMessage)
    ..aI(7, _omitFieldNames ? '' : 'paneCols')
    ..aI(8, _omitFieldNames ? '' : 'paneRows')
    ..aE<SessionState>(9, _omitFieldNames ? '' : 'state',
        enumValues: SessionState.values)
    ..aI(10, _omitFieldNames ? '' : 'exitCode')
    ..aI(11, _omitFieldNames ? '' : 'exitSignal')
    ..aOM<$0.Timestamp>(12, _omitFieldNames ? '' : 'exitedAt',
        subBuilder: $0.Timestamp.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  Session clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  Session copyWith(void Function(Session) updates) =>
      super.copyWith((message) => updates(message as Session)) as Session;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use Session() / Session.new instead')
  static Session create() => Session._();
  static $pb.GeneratedMessage $_createMessage() => Session._();
  @$core.override
  Session createEmptyInstance() => Session._();
  @$core.pragma('dart2js:noInline')
  static Session getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<Session>(Session.$_createMessage);
  static Session? _defaultInstance;

  /// name is the unique session name, also used in the terminal URL.
  @$pb.TagNumber(1)
  $core.String get name => $_getSZ(0);
  @$pb.TagNumber(1)
  set name($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasName() => $_has(0);
  @$pb.TagNumber(1)
  void clearName() => $_clearField(1);

  /// Current working directory of the session's active pane.
  @$pb.TagNumber(2)
  $core.String get cwd => $_getSZ(1);
  @$pb.TagNumber(2)
  set cwd($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasCwd() => $_has(1);
  @$pb.TagNumber(2)
  void clearCwd() => $_clearField(2);

  /// argv the session was started with; empty for the default shell.
  @$pb.TagNumber(3)
  $pb.PbList<$core.String> get command => $_getList(2);

  @$pb.TagNumber(4)
  $0.Timestamp get createdAt => $_getN(3);
  @$pb.TagNumber(4)
  set createdAt($0.Timestamp value) => $_setField(4, value);
  @$pb.TagNumber(4)
  $core.bool hasCreatedAt() => $_has(3);
  @$pb.TagNumber(4)
  void clearCreatedAt() => $_clearField(4);
  @$pb.TagNumber(4)
  $0.Timestamp ensureCreatedAt() => $_ensure(3);

  @$pb.TagNumber(5)
  $core.int get attachedClients => $_getIZ(4);
  @$pb.TagNumber(5)
  set attachedClients($core.int value) => $_setSignedInt32(4, value);
  @$pb.TagNumber(5)
  $core.bool hasAttachedClients() => $_has(4);
  @$pb.TagNumber(5)
  void clearAttachedClients() => $_clearField(5);

  @$pb.TagNumber(6)
  $0.Timestamp get lastActivity => $_getN(5);
  @$pb.TagNumber(6)
  set lastActivity($0.Timestamp value) => $_setField(6, value);
  @$pb.TagNumber(6)
  $core.bool hasLastActivity() => $_has(5);
  @$pb.TagNumber(6)
  void clearLastActivity() => $_clearField(6);
  @$pb.TagNumber(6)
  $0.Timestamp ensureLastActivity() => $_ensure(5);

  @$pb.TagNumber(7)
  $core.int get paneCols => $_getIZ(6);
  @$pb.TagNumber(7)
  set paneCols($core.int value) => $_setSignedInt32(6, value);
  @$pb.TagNumber(7)
  $core.bool hasPaneCols() => $_has(6);
  @$pb.TagNumber(7)
  void clearPaneCols() => $_clearField(7);

  @$pb.TagNumber(8)
  $core.int get paneRows => $_getIZ(7);
  @$pb.TagNumber(8)
  set paneRows($core.int value) => $_setSignedInt32(7, value);
  @$pb.TagNumber(8)
  $core.bool hasPaneRows() => $_has(7);
  @$pb.TagNumber(8)
  void clearPaneRows() => $_clearField(8);

  @$pb.TagNumber(9)
  SessionState get state => $_getN(8);
  @$pb.TagNumber(9)
  set state(SessionState value) => $_setField(9, value);
  @$pb.TagNumber(9)
  $core.bool hasState() => $_has(8);
  @$pb.TagNumber(9)
  void clearState() => $_clearField(9);

  /// exit_code is the command's exit status, or 128+exit_signal when a signal
  /// ended it. Only meaningful when state is EXITED.
  @$pb.TagNumber(10)
  $core.int get exitCode => $_getIZ(9);
  @$pb.TagNumber(10)
  set exitCode($core.int value) => $_setSignedInt32(9, value);
  @$pb.TagNumber(10)
  $core.bool hasExitCode() => $_has(9);
  @$pb.TagNumber(10)
  void clearExitCode() => $_clearField(10);

  /// exit_signal is the terminating signal number, 0 when the command exited
  /// on its own.
  @$pb.TagNumber(11)
  $core.int get exitSignal => $_getIZ(10);
  @$pb.TagNumber(11)
  set exitSignal($core.int value) => $_setSignedInt32(10, value);
  @$pb.TagNumber(11)
  $core.bool hasExitSignal() => $_has(10);
  @$pb.TagNumber(11)
  void clearExitSignal() => $_clearField(11);

  @$pb.TagNumber(12)
  $0.Timestamp get exitedAt => $_getN(11);
  @$pb.TagNumber(12)
  set exitedAt($0.Timestamp value) => $_setField(12, value);
  @$pb.TagNumber(12)
  $core.bool hasExitedAt() => $_has(11);
  @$pb.TagNumber(12)
  void clearExitedAt() => $_clearField(12);
  @$pb.TagNumber(12)
  $0.Timestamp ensureExitedAt() => $_ensure(11);
}

class ListSessionsRequest extends $pb.GeneratedMessage {
  factory ListSessionsRequest() => ListSessionsRequest._();

  ListSessionsRequest._();

  factory ListSessionsRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListSessionsRequest()..mergeFromBuffer(data, registry);
  factory ListSessionsRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListSessionsRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ListSessionsRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ListSessionsRequest.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListSessionsRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListSessionsRequest copyWith(void Function(ListSessionsRequest) updates) =>
      super.copyWith((message) => updates(message as ListSessionsRequest))
          as ListSessionsRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core
      .Deprecated('Use ListSessionsRequest() / ListSessionsRequest.new instead')
  static ListSessionsRequest create() => ListSessionsRequest._();
  static $pb.GeneratedMessage $_createMessage() => ListSessionsRequest._();
  @$core.override
  ListSessionsRequest createEmptyInstance() => ListSessionsRequest._();
  @$core.pragma('dart2js:noInline')
  static ListSessionsRequest getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<ListSessionsRequest>(
          ListSessionsRequest.$_createMessage);
  static ListSessionsRequest? _defaultInstance;
}

class ListSessionsResponse extends $pb.GeneratedMessage {
  factory ListSessionsResponse({
    $core.Iterable<Session>? sessions,
  }) {
    final result = ListSessionsResponse._();
    if (sessions != null) result.sessions.addAll(sessions);
    return result;
  }

  ListSessionsResponse._();

  factory ListSessionsResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListSessionsResponse()..mergeFromBuffer(data, registry);
  factory ListSessionsResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListSessionsResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ListSessionsResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ListSessionsResponse.$_createMessage)
    ..pPM<Session>(1, _omitFieldNames ? '' : 'sessions',
        subBuilder: Session.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListSessionsResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListSessionsResponse copyWith(void Function(ListSessionsResponse) updates) =>
      super.copyWith((message) => updates(message as ListSessionsResponse))
          as ListSessionsResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use ListSessionsResponse() / ListSessionsResponse.new instead')
  static ListSessionsResponse create() => ListSessionsResponse._();
  static $pb.GeneratedMessage $_createMessage() => ListSessionsResponse._();
  @$core.override
  ListSessionsResponse createEmptyInstance() => ListSessionsResponse._();
  @$core.pragma('dart2js:noInline')
  static ListSessionsResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<ListSessionsResponse>(
          ListSessionsResponse.$_createMessage);
  static ListSessionsResponse? _defaultInstance;

  @$pb.TagNumber(1)
  $pb.PbList<Session> get sessions => $_getList(0);
}

class CreateSessionRequest extends $pb.GeneratedMessage {
  factory CreateSessionRequest({
    $core.String? name,
    $core.String? cwd,
    $core.Iterable<$core.String>? command,
    $core.Iterable<$core.MapEntry<$core.String, $core.String>>? env,
  }) {
    final result = CreateSessionRequest._();
    if (name != null) result.name = name;
    if (cwd != null) result.cwd = cwd;
    if (command != null) result.command.addAll(command);
    if (env != null) result.env.addEntries(env);
    return result;
  }

  CreateSessionRequest._();

  factory CreateSessionRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      CreateSessionRequest()..mergeFromBuffer(data, registry);
  factory CreateSessionRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      CreateSessionRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'CreateSessionRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: CreateSessionRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'name')
    ..aOS(2, _omitFieldNames ? '' : 'cwd')
    ..pPS(3, _omitFieldNames ? '' : 'command')
    ..m<$core.String, $core.String>(4, _omitFieldNames ? '' : 'env',
        entryClassName: 'CreateSessionRequest.EnvEntry',
        keyFieldType: $pb.PbFieldType.OS,
        valueFieldType: $pb.PbFieldType.OS,
        packageName: const $pb.PackageName('protectorate.v1'))
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  CreateSessionRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  CreateSessionRequest copyWith(void Function(CreateSessionRequest) updates) =>
      super.copyWith((message) => updates(message as CreateSessionRequest))
          as CreateSessionRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use CreateSessionRequest() / CreateSessionRequest.new instead')
  static CreateSessionRequest create() => CreateSessionRequest._();
  static $pb.GeneratedMessage $_createMessage() => CreateSessionRequest._();
  @$core.override
  CreateSessionRequest createEmptyInstance() => CreateSessionRequest._();
  @$core.pragma('dart2js:noInline')
  static CreateSessionRequest getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<CreateSessionRequest>(
          CreateSessionRequest.$_createMessage);
  static CreateSessionRequest? _defaultInstance;

  /// name identifies the session; it must not already exist.
  @$pb.TagNumber(1)
  $core.String get name => $_getSZ(0);
  @$pb.TagNumber(1)
  set name($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasName() => $_has(0);
  @$pb.TagNumber(1)
  void clearName() => $_clearField(1);

  /// cwd is the starting directory; empty uses tmux's default.
  @$pb.TagNumber(2)
  $core.String get cwd => $_getSZ(1);
  @$pb.TagNumber(2)
  set cwd($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasCwd() => $_has(1);
  @$pb.TagNumber(2)
  void clearCwd() => $_clearField(2);

  /// command is the argv to run; empty starts the default shell.
  @$pb.TagNumber(3)
  $pb.PbList<$core.String> get command => $_getList(2);

  /// env holds extra environment variables for the session.
  @$pb.TagNumber(4)
  $pb.PbMap<$core.String, $core.String> get env => $_getMap(3);
}

class CreateSessionResponse extends $pb.GeneratedMessage {
  factory CreateSessionResponse({
    Session? session,
  }) {
    final result = CreateSessionResponse._();
    if (session != null) result.session = session;
    return result;
  }

  CreateSessionResponse._();

  factory CreateSessionResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      CreateSessionResponse()..mergeFromBuffer(data, registry);
  factory CreateSessionResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      CreateSessionResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'CreateSessionResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: CreateSessionResponse.$_createMessage)
    ..aOM<Session>(1, _omitFieldNames ? '' : 'session',
        subBuilder: Session.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  CreateSessionResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  CreateSessionResponse copyWith(
          void Function(CreateSessionResponse) updates) =>
      super.copyWith((message) => updates(message as CreateSessionResponse))
          as CreateSessionResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use CreateSessionResponse() / CreateSessionResponse.new instead')
  static CreateSessionResponse create() => CreateSessionResponse._();
  static $pb.GeneratedMessage $_createMessage() => CreateSessionResponse._();
  @$core.override
  CreateSessionResponse createEmptyInstance() => CreateSessionResponse._();
  @$core.pragma('dart2js:noInline')
  static CreateSessionResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<CreateSessionResponse>(
          CreateSessionResponse.$_createMessage);
  static CreateSessionResponse? _defaultInstance;

  @$pb.TagNumber(1)
  Session get session => $_getN(0);
  @$pb.TagNumber(1)
  set session(Session value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasSession() => $_has(0);
  @$pb.TagNumber(1)
  void clearSession() => $_clearField(1);
  @$pb.TagNumber(1)
  Session ensureSession() => $_ensure(0);
}

class KillSessionRequest extends $pb.GeneratedMessage {
  factory KillSessionRequest({
    $core.String? name,
  }) {
    final result = KillSessionRequest._();
    if (name != null) result.name = name;
    return result;
  }

  KillSessionRequest._();

  factory KillSessionRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      KillSessionRequest()..mergeFromBuffer(data, registry);
  factory KillSessionRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      KillSessionRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'KillSessionRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: KillSessionRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'name')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  KillSessionRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  KillSessionRequest copyWith(void Function(KillSessionRequest) updates) =>
      super.copyWith((message) => updates(message as KillSessionRequest))
          as KillSessionRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use KillSessionRequest() / KillSessionRequest.new instead')
  static KillSessionRequest create() => KillSessionRequest._();
  static $pb.GeneratedMessage $_createMessage() => KillSessionRequest._();
  @$core.override
  KillSessionRequest createEmptyInstance() => KillSessionRequest._();
  @$core.pragma('dart2js:noInline')
  static KillSessionRequest getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<KillSessionRequest>(
          KillSessionRequest.$_createMessage);
  static KillSessionRequest? _defaultInstance;

  /// name of the session to kill.
  @$pb.TagNumber(1)
  $core.String get name => $_getSZ(0);
  @$pb.TagNumber(1)
  set name($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasName() => $_has(0);
  @$pb.TagNumber(1)
  void clearName() => $_clearField(1);
}

class KillSessionResponse extends $pb.GeneratedMessage {
  factory KillSessionResponse() => KillSessionResponse._();

  KillSessionResponse._();

  factory KillSessionResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      KillSessionResponse()..mergeFromBuffer(data, registry);
  factory KillSessionResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      KillSessionResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'KillSessionResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: KillSessionResponse.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  KillSessionResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  KillSessionResponse copyWith(void Function(KillSessionResponse) updates) =>
      super.copyWith((message) => updates(message as KillSessionResponse))
          as KillSessionResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core
      .Deprecated('Use KillSessionResponse() / KillSessionResponse.new instead')
  static KillSessionResponse create() => KillSessionResponse._();
  static $pb.GeneratedMessage $_createMessage() => KillSessionResponse._();
  @$core.override
  KillSessionResponse createEmptyInstance() => KillSessionResponse._();
  @$core.pragma('dart2js:noInline')
  static KillSessionResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<KillSessionResponse>(
          KillSessionResponse.$_createMessage);
  static KillSessionResponse? _defaultInstance;
}

class RespawnSessionRequest extends $pb.GeneratedMessage {
  factory RespawnSessionRequest({
    $core.String? name,
  }) {
    final result = RespawnSessionRequest._();
    if (name != null) result.name = name;
    return result;
  }

  RespawnSessionRequest._();

  factory RespawnSessionRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      RespawnSessionRequest()..mergeFromBuffer(data, registry);
  factory RespawnSessionRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      RespawnSessionRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'RespawnSessionRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: RespawnSessionRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'name')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  RespawnSessionRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  RespawnSessionRequest copyWith(
          void Function(RespawnSessionRequest) updates) =>
      super.copyWith((message) => updates(message as RespawnSessionRequest))
          as RespawnSessionRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use RespawnSessionRequest() / RespawnSessionRequest.new instead')
  static RespawnSessionRequest create() => RespawnSessionRequest._();
  static $pb.GeneratedMessage $_createMessage() => RespawnSessionRequest._();
  @$core.override
  RespawnSessionRequest createEmptyInstance() => RespawnSessionRequest._();
  @$core.pragma('dart2js:noInline')
  static RespawnSessionRequest getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<RespawnSessionRequest>(
          RespawnSessionRequest.$_createMessage);
  static RespawnSessionRequest? _defaultInstance;

  /// name of the exited session to restart.
  @$pb.TagNumber(1)
  $core.String get name => $_getSZ(0);
  @$pb.TagNumber(1)
  set name($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasName() => $_has(0);
  @$pb.TagNumber(1)
  void clearName() => $_clearField(1);
}

class RespawnSessionResponse extends $pb.GeneratedMessage {
  factory RespawnSessionResponse({
    Session? session,
  }) {
    final result = RespawnSessionResponse._();
    if (session != null) result.session = session;
    return result;
  }

  RespawnSessionResponse._();

  factory RespawnSessionResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      RespawnSessionResponse()..mergeFromBuffer(data, registry);
  factory RespawnSessionResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      RespawnSessionResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'RespawnSessionResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: RespawnSessionResponse.$_createMessage)
    ..aOM<Session>(1, _omitFieldNames ? '' : 'session',
        subBuilder: Session.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  RespawnSessionResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  RespawnSessionResponse copyWith(
          void Function(RespawnSessionResponse) updates) =>
      super.copyWith((message) => updates(message as RespawnSessionResponse))
          as RespawnSessionResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use RespawnSessionResponse() / RespawnSessionResponse.new instead')
  static RespawnSessionResponse create() => RespawnSessionResponse._();
  static $pb.GeneratedMessage $_createMessage() => RespawnSessionResponse._();
  @$core.override
  RespawnSessionResponse createEmptyInstance() => RespawnSessionResponse._();
  @$core.pragma('dart2js:noInline')
  static RespawnSessionResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<RespawnSessionResponse>(
          RespawnSessionResponse.$_createMessage);
  static RespawnSessionResponse? _defaultInstance;

  @$pb.TagNumber(1)
  Session get session => $_getN(0);
  @$pb.TagNumber(1)
  set session(Session value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasSession() => $_has(0);
  @$pb.TagNumber(1)
  void clearSession() => $_clearField(1);
  @$pb.TagNumber(1)
  Session ensureSession() => $_ensure(0);
}

class CapturePaneRequest extends $pb.GeneratedMessage {
  factory CapturePaneRequest({
    $core.String? name,
    $core.int? lines,
  }) {
    final result = CapturePaneRequest._();
    if (name != null) result.name = name;
    if (lines != null) result.lines = lines;
    return result;
  }

  CapturePaneRequest._();

  factory CapturePaneRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      CapturePaneRequest()..mergeFromBuffer(data, registry);
  factory CapturePaneRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      CapturePaneRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'CapturePaneRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: CapturePaneRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'name')
    ..aI(2, _omitFieldNames ? '' : 'lines')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  CapturePaneRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  CapturePaneRequest copyWith(void Function(CapturePaneRequest) updates) =>
      super.copyWith((message) => updates(message as CapturePaneRequest))
          as CapturePaneRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use CapturePaneRequest() / CapturePaneRequest.new instead')
  static CapturePaneRequest create() => CapturePaneRequest._();
  static $pb.GeneratedMessage $_createMessage() => CapturePaneRequest._();
  @$core.override
  CapturePaneRequest createEmptyInstance() => CapturePaneRequest._();
  @$core.pragma('dart2js:noInline')
  static CapturePaneRequest getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<CapturePaneRequest>(
          CapturePaneRequest.$_createMessage);
  static CapturePaneRequest? _defaultInstance;

  /// name of the session to read.
  @$pb.TagNumber(1)
  $core.String get name => $_getSZ(0);
  @$pb.TagNumber(1)
  set name($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasName() => $_has(0);
  @$pb.TagNumber(1)
  void clearName() => $_clearField(1);

  /// Number of history lines above the visible screen to include. Values
  /// above 10000 are clamped to 10000.
  @$pb.TagNumber(2)
  $core.int get lines => $_getIZ(1);
  @$pb.TagNumber(2)
  set lines($core.int value) => $_setSignedInt32(1, value);
  @$pb.TagNumber(2)
  $core.bool hasLines() => $_has(1);
  @$pb.TagNumber(2)
  void clearLines() => $_clearField(2);
}

class CapturePaneResponse extends $pb.GeneratedMessage {
  factory CapturePaneResponse({
    $core.String? text,
  }) {
    final result = CapturePaneResponse._();
    if (text != null) result.text = text;
    return result;
  }

  CapturePaneResponse._();

  factory CapturePaneResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      CapturePaneResponse()..mergeFromBuffer(data, registry);
  factory CapturePaneResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      CapturePaneResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'CapturePaneResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: CapturePaneResponse.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'text')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  CapturePaneResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  CapturePaneResponse copyWith(void Function(CapturePaneResponse) updates) =>
      super.copyWith((message) => updates(message as CapturePaneResponse))
          as CapturePaneResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core
      .Deprecated('Use CapturePaneResponse() / CapturePaneResponse.new instead')
  static CapturePaneResponse create() => CapturePaneResponse._();
  static $pb.GeneratedMessage $_createMessage() => CapturePaneResponse._();
  @$core.override
  CapturePaneResponse createEmptyInstance() => CapturePaneResponse._();
  @$core.pragma('dart2js:noInline')
  static CapturePaneResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<CapturePaneResponse>(
          CapturePaneResponse.$_createMessage);
  static CapturePaneResponse? _defaultInstance;

  /// Pane text with ANSI SGR escapes preserved and wrapped lines joined.
  @$pb.TagNumber(1)
  $core.String get text => $_getSZ(0);
  @$pb.TagNumber(1)
  set text($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasText() => $_has(0);
  @$pb.TagNumber(1)
  void clearText() => $_clearField(1);
}

/// SessionService manages tmux-backed harness sessions. Interactive access is
/// the WebSocket terminal at GET /ws/terminal/{name}, not an RPC.
///
/// When a session's command ends, the session stays in state EXITED with its
/// last screen, exit code and exit signal until it is killed, respawned, or
/// removed by the hub's dead-session retention sweep (--dead-retention).
/// Changes are announced on EventService: "session.created", "session.exited"
/// (command ended; payload has exit_code, exit_signal and exited_at) and
/// "session.removed" (session gone). The WebSocket terminal attaches to an
/// exited session read-only and sends a notice frame saying so.
class SessionServiceApi {
  final $pb.RpcClient _client;

  SessionServiceApi(this._client);

  /// ListSessions returns all sessions, running and exited.
  $async.Future<ListSessionsResponse> listSessions(
          $pb.ClientContext? ctx, ListSessionsRequest request) =>
      _client.invoke<ListSessionsResponse>(ctx, 'SessionService',
          'ListSessions', request, ListSessionsResponse());

  /// CreateSession starts a detached tmux session running command, or the
  /// default shell when command is empty. Names starting with "sleeve-" are
  /// reserved for sleeves and fail with FAILED_PRECONDITION.
  $async.Future<CreateSessionResponse> createSession(
          $pb.ClientContext? ctx, CreateSessionRequest request) =>
      _client.invoke<CreateSessionResponse>(ctx, 'SessionService',
          'CreateSession', request, CreateSessionResponse());

  /// KillSession terminates a session and every process in it. It also removes
  /// an exited session, discarding its final screen.
  $async.Future<KillSessionResponse> killSession(
          $pb.ClientContext? ctx, KillSessionRequest request) =>
      _client.invoke<KillSessionResponse>(
          ctx, 'SessionService', 'KillSession', request, KillSessionResponse());

  /// RespawnSession restarts an exited session with its original command and
  /// working directory. The previous output stays in scrollback. It fails with
  /// FAILED_PRECONDITION while the session is still running, and for sessions
  /// named "sleeve-*", which only SleeveService.SpawnSleeve may restart.
  $async.Future<RespawnSessionResponse> respawnSession(
          $pb.ClientContext? ctx, RespawnSessionRequest request) =>
      _client.invoke<RespawnSessionResponse>(ctx, 'SessionService',
          'RespawnSession', request, RespawnSessionResponse());

  /// CapturePane returns the current text of a session's active pane. It also
  /// works on exited sessions, which is how a crash is diagnosed. Include at
  /// least one history line to see the whole final screen of an exited session.
  $async.Future<CapturePaneResponse> capturePane(
          $pb.ClientContext? ctx, CapturePaneRequest request) =>
      _client.invoke<CapturePaneResponse>(
          ctx, 'SessionService', 'CapturePane', request, CapturePaneResponse());
}

const $core.bool _omitFieldNames =
    $core.bool.fromEnvironment('protobuf.omit_field_names');
const $core.bool _omitMessageNames =
    $core.bool.fromEnvironment('protobuf.omit_message_names');
