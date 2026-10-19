// This is a generated file - do not edit.
//
// Generated from protectorate/v1/directory.proto.

// @dart = 3.3

// ignore_for_file: annotate_overrides, camel_case_types, comment_references
// ignore_for_file: constant_identifier_names
// ignore_for_file: curly_braces_in_flow_control_structures
// ignore_for_file: deprecated_member_use_from_same_package, library_prefixes
// ignore_for_file: non_constant_identifier_names, prefer_relative_imports

import 'dart:async' as $async;
import 'dart:core' as $core;

import 'package:protobuf/protobuf.dart' as $pb;

export 'package:protobuf/protobuf.dart' show GeneratedMessageGenericExtensions;

class ListProjectsRequest extends $pb.GeneratedMessage {
  factory ListProjectsRequest({
    $core.String? sleeve,
  }) {
    final result = ListProjectsRequest._();
    if (sleeve != null) result.sleeve = sleeve;
    return result;
  }

  ListProjectsRequest._();

  factory ListProjectsRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListProjectsRequest()..mergeFromBuffer(data, registry);
  factory ListProjectsRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListProjectsRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ListProjectsRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ListProjectsRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'sleeve')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListProjectsRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListProjectsRequest copyWith(void Function(ListProjectsRequest) updates) =>
      super.copyWith((message) => updates(message as ListProjectsRequest))
          as ListProjectsRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core
      .Deprecated('Use ListProjectsRequest() / ListProjectsRequest.new instead')
  static ListProjectsRequest create() => ListProjectsRequest._();
  static $pb.GeneratedMessage $_createMessage() => ListProjectsRequest._();
  @$core.override
  ListProjectsRequest createEmptyInstance() => ListProjectsRequest._();
  @$core.pragma('dart2js:noInline')
  static ListProjectsRequest getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<ListProjectsRequest>(
          ListProjectsRequest.$_createMessage);
  static ListProjectsRequest? _defaultInstance;

  /// sleeve keeps only the projects this sleeve works on.
  @$pb.TagNumber(1)
  $core.String get sleeve => $_getSZ(0);
  @$pb.TagNumber(1)
  set sleeve($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasSleeve() => $_has(0);
  @$pb.TagNumber(1)
  void clearSleeve() => $_clearField(1);
}

class ListProjectsResponse extends $pb.GeneratedMessage {
  factory ListProjectsResponse({
    $core.Iterable<DirectoryProject>? projects,
  }) {
    final result = ListProjectsResponse._();
    if (projects != null) result.projects.addAll(projects);
    return result;
  }

  ListProjectsResponse._();

  factory ListProjectsResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListProjectsResponse()..mergeFromBuffer(data, registry);
  factory ListProjectsResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListProjectsResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ListProjectsResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ListProjectsResponse.$_createMessage)
    ..pPM<DirectoryProject>(1, _omitFieldNames ? '' : 'projects',
        subBuilder: DirectoryProject.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListProjectsResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListProjectsResponse copyWith(void Function(ListProjectsResponse) updates) =>
      super.copyWith((message) => updates(message as ListProjectsResponse))
          as ListProjectsResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated(
      'Use ListProjectsResponse() / ListProjectsResponse.new instead')
  static ListProjectsResponse create() => ListProjectsResponse._();
  static $pb.GeneratedMessage $_createMessage() => ListProjectsResponse._();
  @$core.override
  ListProjectsResponse createEmptyInstance() => ListProjectsResponse._();
  @$core.pragma('dart2js:noInline')
  static ListProjectsResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<ListProjectsResponse>(
          ListProjectsResponse.$_createMessage);
  static ListProjectsResponse? _defaultInstance;

  @$pb.TagNumber(1)
  $pb.PbList<DirectoryProject> get projects => $_getList(0);
}

class GetProjectRequest extends $pb.GeneratedMessage {
  factory GetProjectRequest({
    $core.String? project,
  }) {
    final result = GetProjectRequest._();
    if (project != null) result.project = project;
    return result;
  }

  GetProjectRequest._();

  factory GetProjectRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      GetProjectRequest()..mergeFromBuffer(data, registry);
  factory GetProjectRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      GetProjectRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'GetProjectRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: GetProjectRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'project')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  GetProjectRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  GetProjectRequest copyWith(void Function(GetProjectRequest) updates) =>
      super.copyWith((message) => updates(message as GetProjectRequest))
          as GetProjectRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use GetProjectRequest() / GetProjectRequest.new instead')
  static GetProjectRequest create() => GetProjectRequest._();
  static $pb.GeneratedMessage $_createMessage() => GetProjectRequest._();
  @$core.override
  GetProjectRequest createEmptyInstance() => GetProjectRequest._();
  @$core.pragma('dart2js:noInline')
  static GetProjectRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<GetProjectRequest>(
          GetProjectRequest.$_createMessage);
  static GetProjectRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get project => $_getSZ(0);
  @$pb.TagNumber(1)
  set project($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasProject() => $_has(0);
  @$pb.TagNumber(1)
  void clearProject() => $_clearField(1);
}

class GetProjectResponse extends $pb.GeneratedMessage {
  factory GetProjectResponse({
    DirectoryProject? project,
  }) {
    final result = GetProjectResponse._();
    if (project != null) result.project = project;
    return result;
  }

  GetProjectResponse._();

  factory GetProjectResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      GetProjectResponse()..mergeFromBuffer(data, registry);
  factory GetProjectResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      GetProjectResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'GetProjectResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: GetProjectResponse.$_createMessage)
    ..aOM<DirectoryProject>(1, _omitFieldNames ? '' : 'project',
        subBuilder: DirectoryProject.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  GetProjectResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  GetProjectResponse copyWith(void Function(GetProjectResponse) updates) =>
      super.copyWith((message) => updates(message as GetProjectResponse))
          as GetProjectResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use GetProjectResponse() / GetProjectResponse.new instead')
  static GetProjectResponse create() => GetProjectResponse._();
  static $pb.GeneratedMessage $_createMessage() => GetProjectResponse._();
  @$core.override
  GetProjectResponse createEmptyInstance() => GetProjectResponse._();
  @$core.pragma('dart2js:noInline')
  static GetProjectResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<GetProjectResponse>(
          GetProjectResponse.$_createMessage);
  static GetProjectResponse? _defaultInstance;

  @$pb.TagNumber(1)
  DirectoryProject get project => $_getN(0);
  @$pb.TagNumber(1)
  set project(DirectoryProject value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasProject() => $_has(0);
  @$pb.TagNumber(1)
  void clearProject() => $_clearField(1);
  @$pb.TagNumber(1)
  DirectoryProject ensureProject() => $_ensure(0);
}

/// DirectoryProject is one project with its resolved edges.
class DirectoryProject extends $pb.GeneratedMessage {
  factory DirectoryProject({
    $core.String? id,
    $core.String? name,
    $core.String? display,
    $core.String? remote,
    $core.Iterable<ProjectOwner>? owners,
    $core.Iterable<$core.String>? provides,
    $core.int? dependencyCount,
    $core.Iterable<ProjectEdge>? dependsOn,
    $core.Iterable<ProjectEdge>? dependents,
    $core.String? defaultBranch,
    $core.String? latestTag,
  }) {
    final result = DirectoryProject._();
    if (id != null) result.id = id;
    if (name != null) result.name = name;
    if (display != null) result.display = display;
    if (remote != null) result.remote = remote;
    if (owners != null) result.owners.addAll(owners);
    if (provides != null) result.provides.addAll(provides);
    if (dependencyCount != null) result.dependencyCount = dependencyCount;
    if (dependsOn != null) result.dependsOn.addAll(dependsOn);
    if (dependents != null) result.dependents.addAll(dependents);
    if (defaultBranch != null) result.defaultBranch = defaultBranch;
    if (latestTag != null) result.latestTag = latestTag;
    return result;
  }

  DirectoryProject._();

  factory DirectoryProject.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      DirectoryProject()..mergeFromBuffer(data, registry);
  factory DirectoryProject.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      DirectoryProject()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'DirectoryProject',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: DirectoryProject.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'id')
    ..aOS(2, _omitFieldNames ? '' : 'name')
    ..aOS(3, _omitFieldNames ? '' : 'display')
    ..aOS(4, _omitFieldNames ? '' : 'remote')
    ..pPM<ProjectOwner>(5, _omitFieldNames ? '' : 'owners',
        subBuilder: ProjectOwner.$_createMessage)
    ..pPS(6, _omitFieldNames ? '' : 'provides')
    ..aI(7, _omitFieldNames ? '' : 'dependencyCount',
        fieldType: $pb.PbFieldType.OU3)
    ..pPM<ProjectEdge>(8, _omitFieldNames ? '' : 'dependsOn',
        subBuilder: ProjectEdge.$_createMessage)
    ..pPM<ProjectEdge>(9, _omitFieldNames ? '' : 'dependents',
        subBuilder: ProjectEdge.$_createMessage)
    ..aOS(10, _omitFieldNames ? '' : 'defaultBranch')
    ..aOS(11, _omitFieldNames ? '' : 'latestTag')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  DirectoryProject clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  DirectoryProject copyWith(void Function(DirectoryProject) updates) =>
      super.copyWith((message) => updates(message as DirectoryProject))
          as DirectoryProject;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use DirectoryProject() / DirectoryProject.new instead')
  static DirectoryProject create() => DirectoryProject._();
  static $pb.GeneratedMessage $_createMessage() => DirectoryProject._();
  @$core.override
  DirectoryProject createEmptyInstance() => DirectoryProject._();
  @$core.pragma('dart2js:noInline')
  static DirectoryProject getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<DirectoryProject>(
          DirectoryProject.$_createMessage);
  static DirectoryProject? _defaultInstance;

  /// id is the normalized remote ("github.com/owner/repo") or "path:<dir>".
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

  /// display is "owner/repo" when the remote has one, else the name.
  @$pb.TagNumber(3)
  $core.String get display => $_getSZ(2);
  @$pb.TagNumber(3)
  set display($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasDisplay() => $_has(2);
  @$pb.TagNumber(3)
  void clearDisplay() => $_clearField(3);

  @$pb.TagNumber(4)
  $core.String get remote => $_getSZ(3);
  @$pb.TagNumber(4)
  set remote($core.String value) => $_setString(3, value);
  @$pb.TagNumber(4)
  $core.bool hasRemote() => $_has(3);
  @$pb.TagNumber(4)
  void clearRemote() => $_clearField(4);

  /// owners are the live sleeves working on the project.
  @$pb.TagNumber(5)
  $pb.PbList<ProjectOwner> get owners => $_getList(4);

  /// provides lists the packages the project publishes, as "<ecosystem>:<name>"
  /// with ecosystem go, npm, cargo or pypi.
  @$pb.TagNumber(6)
  $pb.PbList<$core.String> get provides => $_getList(5);

  /// dependency_count is how many external packages the project depends on.
  @$pb.TagNumber(7)
  $core.int get dependencyCount => $_getIZ(6);
  @$pb.TagNumber(7)
  set dependencyCount($core.int value) => $_setUnsignedInt32(6, value);
  @$pb.TagNumber(7)
  $core.bool hasDependencyCount() => $_has(6);
  @$pb.TagNumber(7)
  void clearDependencyCount() => $_clearField(7);

  /// depends_on are the known projects this one depends on.
  @$pb.TagNumber(8)
  $pb.PbList<ProjectEdge> get dependsOn => $_getList(7);

  /// dependents are the known projects that depend on this one.
  @$pb.TagNumber(9)
  $pb.PbList<ProjectEdge> get dependents => $_getList(8);

  /// default_branch and latest_tag are the last release state the hub saw.
  @$pb.TagNumber(10)
  $core.String get defaultBranch => $_getSZ(9);
  @$pb.TagNumber(10)
  set defaultBranch($core.String value) => $_setString(9, value);
  @$pb.TagNumber(10)
  $core.bool hasDefaultBranch() => $_has(9);
  @$pb.TagNumber(10)
  void clearDefaultBranch() => $_clearField(10);

  @$pb.TagNumber(11)
  $core.String get latestTag => $_getSZ(10);
  @$pb.TagNumber(11)
  set latestTag($core.String value) => $_setString(10, value);
  @$pb.TagNumber(11)
  $core.bool hasLatestTag() => $_has(10);
  @$pb.TagNumber(11)
  void clearLatestTag() => $_clearField(11);
}

class ProjectOwner extends $pb.GeneratedMessage {
  factory ProjectOwner({
    $core.String? sleeve,
    $core.String? workspace,
    $core.String? branch,
    $core.String? head,
  }) {
    final result = ProjectOwner._();
    if (sleeve != null) result.sleeve = sleeve;
    if (workspace != null) result.workspace = workspace;
    if (branch != null) result.branch = branch;
    if (head != null) result.head = head;
    return result;
  }

  ProjectOwner._();

  factory ProjectOwner.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ProjectOwner()..mergeFromBuffer(data, registry);
  factory ProjectOwner.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ProjectOwner()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ProjectOwner',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ProjectOwner.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'sleeve')
    ..aOS(2, _omitFieldNames ? '' : 'workspace')
    ..aOS(3, _omitFieldNames ? '' : 'branch')
    ..aOS(4, _omitFieldNames ? '' : 'head')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ProjectOwner clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ProjectOwner copyWith(void Function(ProjectOwner) updates) =>
      super.copyWith((message) => updates(message as ProjectOwner))
          as ProjectOwner;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use ProjectOwner() / ProjectOwner.new instead')
  static ProjectOwner create() => ProjectOwner._();
  static $pb.GeneratedMessage $_createMessage() => ProjectOwner._();
  @$core.override
  ProjectOwner createEmptyInstance() => ProjectOwner._();
  @$core.pragma('dart2js:noInline')
  static ProjectOwner getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<ProjectOwner>(
          ProjectOwner.$_createMessage);
  static ProjectOwner? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get sleeve => $_getSZ(0);
  @$pb.TagNumber(1)
  set sleeve($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasSleeve() => $_has(0);
  @$pb.TagNumber(1)
  void clearSleeve() => $_clearField(1);

  @$pb.TagNumber(2)
  $core.String get workspace => $_getSZ(1);
  @$pb.TagNumber(2)
  set workspace($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasWorkspace() => $_has(1);
  @$pb.TagNumber(2)
  void clearWorkspace() => $_clearField(2);

  /// branch is empty on a detached HEAD.
  @$pb.TagNumber(3)
  $core.String get branch => $_getSZ(2);
  @$pb.TagNumber(3)
  set branch($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasBranch() => $_has(2);
  @$pb.TagNumber(3)
  void clearBranch() => $_clearField(3);

  @$pb.TagNumber(4)
  $core.String get head => $_getSZ(3);
  @$pb.TagNumber(4)
  set head($core.String value) => $_setString(3, value);
  @$pb.TagNumber(4)
  $core.bool hasHead() => $_has(3);
  @$pb.TagNumber(4)
  void clearHead() => $_clearField(4);
}

/// ProjectEdge links to another project through the packages named in via.
class ProjectEdge extends $pb.GeneratedMessage {
  factory ProjectEdge({
    $core.String? project,
    $core.String? display,
    $core.Iterable<$core.String>? via,
  }) {
    final result = ProjectEdge._();
    if (project != null) result.project = project;
    if (display != null) result.display = display;
    if (via != null) result.via.addAll(via);
    return result;
  }

  ProjectEdge._();

  factory ProjectEdge.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ProjectEdge()..mergeFromBuffer(data, registry);
  factory ProjectEdge.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ProjectEdge()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ProjectEdge',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ProjectEdge.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'project')
    ..aOS(2, _omitFieldNames ? '' : 'display')
    ..pPS(3, _omitFieldNames ? '' : 'via')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ProjectEdge clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ProjectEdge copyWith(void Function(ProjectEdge) updates) =>
      super.copyWith((message) => updates(message as ProjectEdge))
          as ProjectEdge;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use ProjectEdge() / ProjectEdge.new instead')
  static ProjectEdge create() => ProjectEdge._();
  static $pb.GeneratedMessage $_createMessage() => ProjectEdge._();
  @$core.override
  ProjectEdge createEmptyInstance() => ProjectEdge._();
  @$core.pragma('dart2js:noInline')
  static ProjectEdge getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<ProjectEdge>(
          ProjectEdge.$_createMessage);
  static ProjectEdge? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get project => $_getSZ(0);
  @$pb.TagNumber(1)
  set project($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasProject() => $_has(0);
  @$pb.TagNumber(1)
  void clearProject() => $_clearField(1);

  @$pb.TagNumber(2)
  $core.String get display => $_getSZ(1);
  @$pb.TagNumber(2)
  set display($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasDisplay() => $_has(1);
  @$pb.TagNumber(2)
  void clearDisplay() => $_clearField(2);

  @$pb.TagNumber(3)
  $pb.PbList<$core.String> get via => $_getList(2);
}

/// DirectoryService answers which projects the sleeves work on and how they
/// depend on each other. The hub builds it deterministically from each sleeve
/// workspace's git remote and package manifests (go.mod, package.json,
/// Cargo.toml, pyproject.toml), on spawn and on a slow interval. A project is
/// identified by its normalized git remote, or by "path:<workspace>" without
/// one. Project A depends on project B when one of A's dependencies is a
/// package B provides. A project stays listed, with no owners, after its last
/// sleeve is gone.
class DirectoryServiceApi {
  final $pb.RpcClient _client;

  DirectoryServiceApi(this._client);

  /// ListProjects returns every known project, optionally only those a sleeve
  /// works on.
  $async.Future<ListProjectsResponse> listProjects(
          $pb.ClientContext? ctx, ListProjectsRequest request) =>
      _client.invoke<ListProjectsResponse>(ctx, 'DirectoryService',
          'ListProjects', request, ListProjectsResponse());

  /// GetProject returns one project by id, "owner/repo" or bare name. A name
  /// that several projects share is an error that lists them.
  $async.Future<GetProjectResponse> getProject(
          $pb.ClientContext? ctx, GetProjectRequest request) =>
      _client.invoke<GetProjectResponse>(
          ctx, 'DirectoryService', 'GetProject', request, GetProjectResponse());
}

const $core.bool _omitFieldNames =
    $core.bool.fromEnvironment('protobuf.omit_field_names');
const $core.bool _omitMessageNames =
    $core.bool.fromEnvironment('protobuf.omit_message_names');
