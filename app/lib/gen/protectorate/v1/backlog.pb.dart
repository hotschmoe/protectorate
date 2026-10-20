// This is a generated file - do not edit.
//
// Generated from protectorate/v1/backlog.proto.

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

export 'package:protobuf/protobuf.dart' show GeneratedMessageGenericExtensions;

/// Note is one note.
class Note extends $pb.GeneratedMessage {
  factory Note({
    $core.String? id,
    $core.String? title,
    $core.String? body,
    $0.Timestamp? createdAt,
    $0.Timestamp? updatedAt,
    $core.bool? pinned,
    $core.String? project,
    $core.String? source,
    $core.String? state,
    NoteCompletion? completion,
  }) {
    final result = Note._();
    if (id != null) result.id = id;
    if (title != null) result.title = title;
    if (body != null) result.body = body;
    if (createdAt != null) result.createdAt = createdAt;
    if (updatedAt != null) result.updatedAt = updatedAt;
    if (pinned != null) result.pinned = pinned;
    if (project != null) result.project = project;
    if (source != null) result.source = source;
    if (state != null) result.state = state;
    if (completion != null) result.completion = completion;
    return result;
  }

  Note._();

  factory Note.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      Note()..mergeFromBuffer(data, registry);
  factory Note.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      Note()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'Note',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: Note.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'id')
    ..aOS(2, _omitFieldNames ? '' : 'title')
    ..aOS(3, _omitFieldNames ? '' : 'body')
    ..aOM<$0.Timestamp>(4, _omitFieldNames ? '' : 'createdAt',
        subBuilder: $0.Timestamp.$_createMessage)
    ..aOM<$0.Timestamp>(5, _omitFieldNames ? '' : 'updatedAt',
        subBuilder: $0.Timestamp.$_createMessage)
    ..aOB(6, _omitFieldNames ? '' : 'pinned')
    ..aOS(7, _omitFieldNames ? '' : 'project')
    ..aOS(8, _omitFieldNames ? '' : 'source')
    ..aOS(9, _omitFieldNames ? '' : 'state')
    ..aOM<NoteCompletion>(10, _omitFieldNames ? '' : 'completion',
        subBuilder: NoteCompletion.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  Note clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  Note copyWith(void Function(Note) updates) =>
      super.copyWith((message) => updates(message as Note)) as Note;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use Note() / Note.new instead')
  static Note create() => Note._();
  static $pb.GeneratedMessage $_createMessage() => Note._();
  @$core.override
  Note createEmptyInstance() => Note._();
  @$core.pragma('dart2js:noInline')
  static Note getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<Note>(Note.$_createMessage);
  static Note? _defaultInstance;

  /// id is the stable identifier assigned on creation.
  @$pb.TagNumber(1)
  $core.String get id => $_getSZ(0);
  @$pb.TagNumber(1)
  set id($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasId() => $_has(0);
  @$pb.TagNumber(1)
  void clearId() => $_clearField(1);

  /// title is a single line; it may be empty when the note has a body.
  @$pb.TagNumber(2)
  $core.String get title => $_getSZ(1);
  @$pb.TagNumber(2)
  set title($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasTitle() => $_has(1);
  @$pb.TagNumber(2)
  void clearTitle() => $_clearField(2);

  /// body is markdown text.
  @$pb.TagNumber(3)
  $core.String get body => $_getSZ(2);
  @$pb.TagNumber(3)
  set body($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasBody() => $_has(2);
  @$pb.TagNumber(3)
  void clearBody() => $_clearField(3);

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
  $0.Timestamp get updatedAt => $_getN(4);
  @$pb.TagNumber(5)
  set updatedAt($0.Timestamp value) => $_setField(5, value);
  @$pb.TagNumber(5)
  $core.bool hasUpdatedAt() => $_has(4);
  @$pb.TagNumber(5)
  void clearUpdatedAt() => $_clearField(5);
  @$pb.TagNumber(5)
  $0.Timestamp ensureUpdatedAt() => $_ensure(4);

  /// pinned notes sort first.
  @$pb.TagNumber(6)
  $core.bool get pinned => $_getBF(5);
  @$pb.TagNumber(6)
  set pinned($core.bool value) => $_setBool(5, value);
  @$pb.TagNumber(6)
  $core.bool hasPinned() => $_has(5);
  @$pb.TagNumber(6)
  void clearPinned() => $_clearField(6);

  /// project is an optional tag such as "hotschmoe/protectorate".
  @$pb.TagNumber(7)
  $core.String get project => $_getSZ(6);
  @$pb.TagNumber(7)
  set project($core.String value) => $_setString(6, value);
  @$pb.TagNumber(7)
  $core.bool hasProject() => $_has(6);
  @$pb.TagNumber(7)
  void clearProject() => $_clearField(7);

  /// source is who created the note: "owner", a sleeve name, "envoy" or "hub".
  @$pb.TagNumber(8)
  $core.String get source => $_getSZ(7);
  @$pb.TagNumber(8)
  set source($core.String value) => $_setString(7, value);
  @$pb.TagNumber(8)
  $core.bool hasSource() => $_has(7);
  @$pb.TagNumber(8)
  void clearSource() => $_clearField(8);

  /// state is "open" or "done".
  @$pb.TagNumber(9)
  $core.String get state => $_getSZ(8);
  @$pb.TagNumber(9)
  set state($core.String value) => $_setString(8, value);
  @$pb.TagNumber(9)
  $core.bool hasState() => $_has(8);
  @$pb.TagNumber(9)
  void clearState() => $_clearField(9);

  /// completion is present only while the note is done.
  @$pb.TagNumber(10)
  NoteCompletion get completion => $_getN(9);
  @$pb.TagNumber(10)
  set completion(NoteCompletion value) => $_setField(10, value);
  @$pb.TagNumber(10)
  $core.bool hasCompletion() => $_has(9);
  @$pb.TagNumber(10)
  void clearCompletion() => $_clearField(10);
  @$pb.TagNumber(10)
  NoteCompletion ensureCompletion() => $_ensure(9);
}

class ListNotesRequest extends $pb.GeneratedMessage {
  factory ListNotesRequest({
    $core.String? project,
    $core.String? query,
    $core.String? state,
  }) {
    final result = ListNotesRequest._();
    if (project != null) result.project = project;
    if (query != null) result.query = query;
    if (state != null) result.state = state;
    return result;
  }

  ListNotesRequest._();

  factory ListNotesRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListNotesRequest()..mergeFromBuffer(data, registry);
  factory ListNotesRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListNotesRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ListNotesRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ListNotesRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'project')
    ..aOS(2, _omitFieldNames ? '' : 'query')
    ..aOS(3, _omitFieldNames ? '' : 'state')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListNotesRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListNotesRequest copyWith(void Function(ListNotesRequest) updates) =>
      super.copyWith((message) => updates(message as ListNotesRequest))
          as ListNotesRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use ListNotesRequest() / ListNotesRequest.new instead')
  static ListNotesRequest create() => ListNotesRequest._();
  static $pb.GeneratedMessage $_createMessage() => ListNotesRequest._();
  @$core.override
  ListNotesRequest createEmptyInstance() => ListNotesRequest._();
  @$core.pragma('dart2js:noInline')
  static ListNotesRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<ListNotesRequest>(
          ListNotesRequest.$_createMessage);
  static ListNotesRequest? _defaultInstance;

  /// project keeps notes with exactly this tag.
  @$pb.TagNumber(1)
  $core.String get project => $_getSZ(0);
  @$pb.TagNumber(1)
  set project($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasProject() => $_has(0);
  @$pb.TagNumber(1)
  void clearProject() => $_clearField(1);

  /// query keeps notes whose title or body contains it, ignoring case.
  @$pb.TagNumber(2)
  $core.String get query => $_getSZ(1);
  @$pb.TagNumber(2)
  set query($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasQuery() => $_has(1);
  @$pb.TagNumber(2)
  void clearQuery() => $_clearField(2);

  /// state is "open" (also the default), "done", or "all".
  @$pb.TagNumber(3)
  $core.String get state => $_getSZ(2);
  @$pb.TagNumber(3)
  set state($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasState() => $_has(2);
  @$pb.TagNumber(3)
  void clearState() => $_clearField(3);
}

class ListNotesResponse extends $pb.GeneratedMessage {
  factory ListNotesResponse({
    $core.Iterable<Note>? notes,
  }) {
    final result = ListNotesResponse._();
    if (notes != null) result.notes.addAll(notes);
    return result;
  }

  ListNotesResponse._();

  factory ListNotesResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListNotesResponse()..mergeFromBuffer(data, registry);
  factory ListNotesResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      ListNotesResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'ListNotesResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: ListNotesResponse.$_createMessage)
    ..pPM<Note>(1, _omitFieldNames ? '' : 'notes',
        subBuilder: Note.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListNotesResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  ListNotesResponse copyWith(void Function(ListNotesResponse) updates) =>
      super.copyWith((message) => updates(message as ListNotesResponse))
          as ListNotesResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use ListNotesResponse() / ListNotesResponse.new instead')
  static ListNotesResponse create() => ListNotesResponse._();
  static $pb.GeneratedMessage $_createMessage() => ListNotesResponse._();
  @$core.override
  ListNotesResponse createEmptyInstance() => ListNotesResponse._();
  @$core.pragma('dart2js:noInline')
  static ListNotesResponse getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<ListNotesResponse>(
          ListNotesResponse.$_createMessage);
  static ListNotesResponse? _defaultInstance;

  @$pb.TagNumber(1)
  $pb.PbList<Note> get notes => $_getList(0);
}

class GetNoteRequest extends $pb.GeneratedMessage {
  factory GetNoteRequest({
    $core.String? id,
  }) {
    final result = GetNoteRequest._();
    if (id != null) result.id = id;
    return result;
  }

  GetNoteRequest._();

  factory GetNoteRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      GetNoteRequest()..mergeFromBuffer(data, registry);
  factory GetNoteRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      GetNoteRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'GetNoteRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: GetNoteRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'id')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  GetNoteRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  GetNoteRequest copyWith(void Function(GetNoteRequest) updates) =>
      super.copyWith((message) => updates(message as GetNoteRequest))
          as GetNoteRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use GetNoteRequest() / GetNoteRequest.new instead')
  static GetNoteRequest create() => GetNoteRequest._();
  static $pb.GeneratedMessage $_createMessage() => GetNoteRequest._();
  @$core.override
  GetNoteRequest createEmptyInstance() => GetNoteRequest._();
  @$core.pragma('dart2js:noInline')
  static GetNoteRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<GetNoteRequest>(
          GetNoteRequest.$_createMessage);
  static GetNoteRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get id => $_getSZ(0);
  @$pb.TagNumber(1)
  set id($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasId() => $_has(0);
  @$pb.TagNumber(1)
  void clearId() => $_clearField(1);
}

class GetNoteResponse extends $pb.GeneratedMessage {
  factory GetNoteResponse({
    Note? note,
  }) {
    final result = GetNoteResponse._();
    if (note != null) result.note = note;
    return result;
  }

  GetNoteResponse._();

  factory GetNoteResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      GetNoteResponse()..mergeFromBuffer(data, registry);
  factory GetNoteResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      GetNoteResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'GetNoteResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: GetNoteResponse.$_createMessage)
    ..aOM<Note>(1, _omitFieldNames ? '' : 'note',
        subBuilder: Note.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  GetNoteResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  GetNoteResponse copyWith(void Function(GetNoteResponse) updates) =>
      super.copyWith((message) => updates(message as GetNoteResponse))
          as GetNoteResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use GetNoteResponse() / GetNoteResponse.new instead')
  static GetNoteResponse create() => GetNoteResponse._();
  static $pb.GeneratedMessage $_createMessage() => GetNoteResponse._();
  @$core.override
  GetNoteResponse createEmptyInstance() => GetNoteResponse._();
  @$core.pragma('dart2js:noInline')
  static GetNoteResponse getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<GetNoteResponse>(
          GetNoteResponse.$_createMessage);
  static GetNoteResponse? _defaultInstance;

  @$pb.TagNumber(1)
  Note get note => $_getN(0);
  @$pb.TagNumber(1)
  set note(Note value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasNote() => $_has(0);
  @$pb.TagNumber(1)
  void clearNote() => $_clearField(1);
  @$pb.TagNumber(1)
  Note ensureNote() => $_ensure(0);
}

/// CreateNoteRequest needs a title or a body. The source is taken from the
/// caller's identity, not from the request.
class CreateNoteRequest extends $pb.GeneratedMessage {
  factory CreateNoteRequest({
    $core.String? title,
    $core.String? body,
    $core.bool? pinned,
    $core.String? project,
  }) {
    final result = CreateNoteRequest._();
    if (title != null) result.title = title;
    if (body != null) result.body = body;
    if (pinned != null) result.pinned = pinned;
    if (project != null) result.project = project;
    return result;
  }

  CreateNoteRequest._();

  factory CreateNoteRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      CreateNoteRequest()..mergeFromBuffer(data, registry);
  factory CreateNoteRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      CreateNoteRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'CreateNoteRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: CreateNoteRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'title')
    ..aOS(2, _omitFieldNames ? '' : 'body')
    ..aOB(3, _omitFieldNames ? '' : 'pinned')
    ..aOS(4, _omitFieldNames ? '' : 'project')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  CreateNoteRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  CreateNoteRequest copyWith(void Function(CreateNoteRequest) updates) =>
      super.copyWith((message) => updates(message as CreateNoteRequest))
          as CreateNoteRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use CreateNoteRequest() / CreateNoteRequest.new instead')
  static CreateNoteRequest create() => CreateNoteRequest._();
  static $pb.GeneratedMessage $_createMessage() => CreateNoteRequest._();
  @$core.override
  CreateNoteRequest createEmptyInstance() => CreateNoteRequest._();
  @$core.pragma('dart2js:noInline')
  static CreateNoteRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<CreateNoteRequest>(
          CreateNoteRequest.$_createMessage);
  static CreateNoteRequest? _defaultInstance;

  /// title is a single line, up to 200 characters.
  @$pb.TagNumber(1)
  $core.String get title => $_getSZ(0);
  @$pb.TagNumber(1)
  set title($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasTitle() => $_has(0);
  @$pb.TagNumber(1)
  void clearTitle() => $_clearField(1);

  /// body is markdown text.
  @$pb.TagNumber(2)
  $core.String get body => $_getSZ(1);
  @$pb.TagNumber(2)
  set body($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasBody() => $_has(1);
  @$pb.TagNumber(2)
  void clearBody() => $_clearField(2);

  @$pb.TagNumber(3)
  $core.bool get pinned => $_getBF(2);
  @$pb.TagNumber(3)
  set pinned($core.bool value) => $_setBool(2, value);
  @$pb.TagNumber(3)
  $core.bool hasPinned() => $_has(2);
  @$pb.TagNumber(3)
  void clearPinned() => $_clearField(3);

  /// project is an optional tag such as "hotschmoe/protectorate".
  @$pb.TagNumber(4)
  $core.String get project => $_getSZ(3);
  @$pb.TagNumber(4)
  set project($core.String value) => $_setString(3, value);
  @$pb.TagNumber(4)
  $core.bool hasProject() => $_has(3);
  @$pb.TagNumber(4)
  void clearProject() => $_clearField(4);
}

class CreateNoteResponse extends $pb.GeneratedMessage {
  factory CreateNoteResponse({
    Note? note,
  }) {
    final result = CreateNoteResponse._();
    if (note != null) result.note = note;
    return result;
  }

  CreateNoteResponse._();

  factory CreateNoteResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      CreateNoteResponse()..mergeFromBuffer(data, registry);
  factory CreateNoteResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      CreateNoteResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'CreateNoteResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: CreateNoteResponse.$_createMessage)
    ..aOM<Note>(1, _omitFieldNames ? '' : 'note',
        subBuilder: Note.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  CreateNoteResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  CreateNoteResponse copyWith(void Function(CreateNoteResponse) updates) =>
      super.copyWith((message) => updates(message as CreateNoteResponse))
          as CreateNoteResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use CreateNoteResponse() / CreateNoteResponse.new instead')
  static CreateNoteResponse create() => CreateNoteResponse._();
  static $pb.GeneratedMessage $_createMessage() => CreateNoteResponse._();
  @$core.override
  CreateNoteResponse createEmptyInstance() => CreateNoteResponse._();
  @$core.pragma('dart2js:noInline')
  static CreateNoteResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<CreateNoteResponse>(
          CreateNoteResponse.$_createMessage);
  static CreateNoteResponse? _defaultInstance;

  @$pb.TagNumber(1)
  Note get note => $_getN(0);
  @$pb.TagNumber(1)
  set note(Note value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasNote() => $_has(0);
  @$pb.TagNumber(1)
  void clearNote() => $_clearField(1);
  @$pb.TagNumber(1)
  Note ensureNote() => $_ensure(0);
}

/// UpdateNoteRequest sets the fields that are present; an empty project clears
/// the tag.
class UpdateNoteRequest extends $pb.GeneratedMessage {
  factory UpdateNoteRequest({
    $core.String? id,
    $core.String? title,
    $core.String? body,
    $core.bool? pinned,
    $core.String? project,
    $core.String? state,
    $core.String? resolution,
  }) {
    final result = UpdateNoteRequest._();
    if (id != null) result.id = id;
    if (title != null) result.title = title;
    if (body != null) result.body = body;
    if (pinned != null) result.pinned = pinned;
    if (project != null) result.project = project;
    if (state != null) result.state = state;
    if (resolution != null) result.resolution = resolution;
    return result;
  }

  UpdateNoteRequest._();

  factory UpdateNoteRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      UpdateNoteRequest()..mergeFromBuffer(data, registry);
  factory UpdateNoteRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      UpdateNoteRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'UpdateNoteRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: UpdateNoteRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'id')
    ..aOS(2, _omitFieldNames ? '' : 'title')
    ..aOS(3, _omitFieldNames ? '' : 'body')
    ..aOB(4, _omitFieldNames ? '' : 'pinned')
    ..aOS(5, _omitFieldNames ? '' : 'project')
    ..aOS(6, _omitFieldNames ? '' : 'state')
    ..aOS(7, _omitFieldNames ? '' : 'resolution')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  UpdateNoteRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  UpdateNoteRequest copyWith(void Function(UpdateNoteRequest) updates) =>
      super.copyWith((message) => updates(message as UpdateNoteRequest))
          as UpdateNoteRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use UpdateNoteRequest() / UpdateNoteRequest.new instead')
  static UpdateNoteRequest create() => UpdateNoteRequest._();
  static $pb.GeneratedMessage $_createMessage() => UpdateNoteRequest._();
  @$core.override
  UpdateNoteRequest createEmptyInstance() => UpdateNoteRequest._();
  @$core.pragma('dart2js:noInline')
  static UpdateNoteRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<UpdateNoteRequest>(
          UpdateNoteRequest.$_createMessage);
  static UpdateNoteRequest? _defaultInstance;

  /// id of the note to change.
  @$pb.TagNumber(1)
  $core.String get id => $_getSZ(0);
  @$pb.TagNumber(1)
  set id($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasId() => $_has(0);
  @$pb.TagNumber(1)
  void clearId() => $_clearField(1);

  /// title replaces the title when set.
  @$pb.TagNumber(2)
  $core.String get title => $_getSZ(1);
  @$pb.TagNumber(2)
  set title($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasTitle() => $_has(1);
  @$pb.TagNumber(2)
  void clearTitle() => $_clearField(2);

  /// body replaces the whole markdown body when set; send the full text.
  @$pb.TagNumber(3)
  $core.String get body => $_getSZ(2);
  @$pb.TagNumber(3)
  set body($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasBody() => $_has(2);
  @$pb.TagNumber(3)
  void clearBody() => $_clearField(3);

  /// pinned pins or unpins the note when set.
  @$pb.TagNumber(4)
  $core.bool get pinned => $_getBF(3);
  @$pb.TagNumber(4)
  set pinned($core.bool value) => $_setBool(3, value);
  @$pb.TagNumber(4)
  $core.bool hasPinned() => $_has(3);
  @$pb.TagNumber(4)
  void clearPinned() => $_clearField(4);

  /// project replaces the project tag when set; "" clears it.
  @$pb.TagNumber(5)
  $core.String get project => $_getSZ(4);
  @$pb.TagNumber(5)
  set project($core.String value) => $_setString(4, value);
  @$pb.TagNumber(5)
  $core.bool hasProject() => $_has(4);
  @$pb.TagNumber(5)
  void clearProject() => $_clearField(5);

  /// state closes ("done") or reopens ("open") the note when set.
  @$pb.TagNumber(6)
  $core.String get state => $_getSZ(5);
  @$pb.TagNumber(6)
  set state($core.String value) => $_setString(5, value);
  @$pb.TagNumber(6)
  $core.bool hasState() => $_has(5);
  @$pb.TagNumber(6)
  void clearState() => $_clearField(6);

  /// resolution is an optional single line, up to 200 characters, for a done note.
  @$pb.TagNumber(7)
  $core.String get resolution => $_getSZ(6);
  @$pb.TagNumber(7)
  set resolution($core.String value) => $_setString(6, value);
  @$pb.TagNumber(7)
  $core.bool hasResolution() => $_has(6);
  @$pb.TagNumber(7)
  void clearResolution() => $_clearField(7);
}

class UpdateNoteResponse extends $pb.GeneratedMessage {
  factory UpdateNoteResponse({
    Note? note,
  }) {
    final result = UpdateNoteResponse._();
    if (note != null) result.note = note;
    return result;
  }

  UpdateNoteResponse._();

  factory UpdateNoteResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      UpdateNoteResponse()..mergeFromBuffer(data, registry);
  factory UpdateNoteResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      UpdateNoteResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'UpdateNoteResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: UpdateNoteResponse.$_createMessage)
    ..aOM<Note>(1, _omitFieldNames ? '' : 'note',
        subBuilder: Note.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  UpdateNoteResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  UpdateNoteResponse copyWith(void Function(UpdateNoteResponse) updates) =>
      super.copyWith((message) => updates(message as UpdateNoteResponse))
          as UpdateNoteResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use UpdateNoteResponse() / UpdateNoteResponse.new instead')
  static UpdateNoteResponse create() => UpdateNoteResponse._();
  static $pb.GeneratedMessage $_createMessage() => UpdateNoteResponse._();
  @$core.override
  UpdateNoteResponse createEmptyInstance() => UpdateNoteResponse._();
  @$core.pragma('dart2js:noInline')
  static UpdateNoteResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<UpdateNoteResponse>(
          UpdateNoteResponse.$_createMessage);
  static UpdateNoteResponse? _defaultInstance;

  @$pb.TagNumber(1)
  Note get note => $_getN(0);
  @$pb.TagNumber(1)
  set note(Note value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasNote() => $_has(0);
  @$pb.TagNumber(1)
  void clearNote() => $_clearField(1);
  @$pb.TagNumber(1)
  Note ensureNote() => $_ensure(0);
}

class DeleteNoteRequest extends $pb.GeneratedMessage {
  factory DeleteNoteRequest({
    $core.String? id,
  }) {
    final result = DeleteNoteRequest._();
    if (id != null) result.id = id;
    return result;
  }

  DeleteNoteRequest._();

  factory DeleteNoteRequest.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      DeleteNoteRequest()..mergeFromBuffer(data, registry);
  factory DeleteNoteRequest.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      DeleteNoteRequest()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'DeleteNoteRequest',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: DeleteNoteRequest.$_createMessage)
    ..aOS(1, _omitFieldNames ? '' : 'id')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  DeleteNoteRequest clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  DeleteNoteRequest copyWith(void Function(DeleteNoteRequest) updates) =>
      super.copyWith((message) => updates(message as DeleteNoteRequest))
          as DeleteNoteRequest;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use DeleteNoteRequest() / DeleteNoteRequest.new instead')
  static DeleteNoteRequest create() => DeleteNoteRequest._();
  static $pb.GeneratedMessage $_createMessage() => DeleteNoteRequest._();
  @$core.override
  DeleteNoteRequest createEmptyInstance() => DeleteNoteRequest._();
  @$core.pragma('dart2js:noInline')
  static DeleteNoteRequest getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<DeleteNoteRequest>(
          DeleteNoteRequest.$_createMessage);
  static DeleteNoteRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get id => $_getSZ(0);
  @$pb.TagNumber(1)
  set id($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasId() => $_has(0);
  @$pb.TagNumber(1)
  void clearId() => $_clearField(1);
}

class DeleteNoteResponse extends $pb.GeneratedMessage {
  factory DeleteNoteResponse() => DeleteNoteResponse._();

  DeleteNoteResponse._();

  factory DeleteNoteResponse.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      DeleteNoteResponse()..mergeFromBuffer(data, registry);
  factory DeleteNoteResponse.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      DeleteNoteResponse()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'DeleteNoteResponse',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: DeleteNoteResponse.$_createMessage)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  DeleteNoteResponse clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  DeleteNoteResponse copyWith(void Function(DeleteNoteResponse) updates) =>
      super.copyWith((message) => updates(message as DeleteNoteResponse))
          as DeleteNoteResponse;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use DeleteNoteResponse() / DeleteNoteResponse.new instead')
  static DeleteNoteResponse create() => DeleteNoteResponse._();
  static $pb.GeneratedMessage $_createMessage() => DeleteNoteResponse._();
  @$core.override
  DeleteNoteResponse createEmptyInstance() => DeleteNoteResponse._();
  @$core.pragma('dart2js:noInline')
  static DeleteNoteResponse getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<DeleteNoteResponse>(
          DeleteNoteResponse.$_createMessage);
  static DeleteNoteResponse? _defaultInstance;
}

/// NoteCompletion records the latest close; reopening clears it.
class NoteCompletion extends $pb.GeneratedMessage {
  factory NoteCompletion({
    $0.Timestamp? doneAt,
    $core.String? doneBy,
    $core.String? resolution,
  }) {
    final result = NoteCompletion._();
    if (doneAt != null) result.doneAt = doneAt;
    if (doneBy != null) result.doneBy = doneBy;
    if (resolution != null) result.resolution = resolution;
    return result;
  }

  NoteCompletion._();

  factory NoteCompletion.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      NoteCompletion()..mergeFromBuffer(data, registry);
  factory NoteCompletion.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      NoteCompletion()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'NoteCompletion',
      package:
          const $pb.PackageName(_omitMessageNames ? '' : 'protectorate.v1'),
      createEmptyInstance: NoteCompletion.$_createMessage)
    ..aOM<$0.Timestamp>(1, _omitFieldNames ? '' : 'doneAt',
        subBuilder: $0.Timestamp.$_createMessage)
    ..aOS(2, _omitFieldNames ? '' : 'doneBy')
    ..aOS(3, _omitFieldNames ? '' : 'resolution')
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  NoteCompletion clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  NoteCompletion copyWith(void Function(NoteCompletion) updates) =>
      super.copyWith((message) => updates(message as NoteCompletion))
          as NoteCompletion;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  @$core.Deprecated('Use NoteCompletion() / NoteCompletion.new instead')
  static NoteCompletion create() => NoteCompletion._();
  static $pb.GeneratedMessage $_createMessage() => NoteCompletion._();
  @$core.override
  NoteCompletion createEmptyInstance() => NoteCompletion._();
  @$core.pragma('dart2js:noInline')
  static NoteCompletion getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<NoteCompletion>(
          NoteCompletion.$_createMessage);
  static NoteCompletion? _defaultInstance;

  @$pb.TagNumber(1)
  $0.Timestamp get doneAt => $_getN(0);
  @$pb.TagNumber(1)
  set doneAt($0.Timestamp value) => $_setField(1, value);
  @$pb.TagNumber(1)
  $core.bool hasDoneAt() => $_has(0);
  @$pb.TagNumber(1)
  void clearDoneAt() => $_clearField(1);
  @$pb.TagNumber(1)
  $0.Timestamp ensureDoneAt() => $_ensure(0);

  /// done_by is the authenticated actor, exactly as named in the audit log.
  @$pb.TagNumber(2)
  $core.String get doneBy => $_getSZ(1);
  @$pb.TagNumber(2)
  set doneBy($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasDoneBy() => $_has(1);
  @$pb.TagNumber(2)
  void clearDoneBy() => $_clearField(2);

  @$pb.TagNumber(3)
  $core.String get resolution => $_getSZ(2);
  @$pb.TagNumber(3)
  set resolution($core.String value) => $_setString(2, value);
  @$pb.TagNumber(3)
  $core.bool hasResolution() => $_has(2);
  @$pb.TagNumber(3)
  void clearResolution() => $_clearField(3);
}

/// BacklogService keeps the owner's notes on the hub (docs/backlog.md). The
/// owner writes them in the app or CLI; the orchestrator reads and edits them
/// through the same API. Mutations publish note.* events on EventService.
class BacklogServiceApi {
  final $pb.RpcClient _client;

  BacklogServiceApi(this._client);

  /// ListNotes returns notes, pinned first and then most recently updated,
  /// open by default, optionally filtered by project tag and a text query.
  /// Set state to "done" for completed notes or "all" for both states.
  $async.Future<ListNotesResponse> listNotes(
          $pb.ClientContext? ctx, ListNotesRequest request) =>
      _client.invoke<ListNotesResponse>(
          ctx, 'BacklogService', 'ListNotes', request, ListNotesResponse());

  /// GetNote returns one note by id.
  $async.Future<GetNoteResponse> getNote(
          $pb.ClientContext? ctx, GetNoteRequest request) =>
      _client.invoke<GetNoteResponse>(
          ctx, 'BacklogService', 'GetNote', request, GetNoteResponse());

  /// CreateNote adds a note and returns it with its id.
  $async.Future<CreateNoteResponse> createNote(
          $pb.ClientContext? ctx, CreateNoteRequest request) =>
      _client.invoke<CreateNoteResponse>(
          ctx, 'BacklogService', 'CreateNote', request, CreateNoteResponse());

  /// UpdateNote changes the fields that are set and leaves the others alone.
  /// Set state to "done" with an optional resolution line to close a note,
  /// or "open" to reopen it and clear completion. Requires backlog_update_note.
  $async.Future<UpdateNoteResponse> updateNote(
          $pb.ClientContext? ctx, UpdateNoteRequest request) =>
      _client.invoke<UpdateNoteResponse>(
          ctx, 'BacklogService', 'UpdateNote', request, UpdateNoteResponse());

  /// DeleteNote removes a note for good.
  $async.Future<DeleteNoteResponse> deleteNote(
          $pb.ClientContext? ctx, DeleteNoteRequest request) =>
      _client.invoke<DeleteNoteResponse>(
          ctx, 'BacklogService', 'DeleteNote', request, DeleteNoteResponse());
}

const $core.bool _omitFieldNames =
    $core.bool.fromEnvironment('protobuf.omit_field_names');
const $core.bool _omitMessageNames =
    $core.bool.fromEnvironment('protobuf.omit_message_names');
