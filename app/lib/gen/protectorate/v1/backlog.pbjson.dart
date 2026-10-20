// This is a generated file - do not edit.
//
// Generated from protectorate/v1/backlog.proto.

// @dart = 3.3

// ignore_for_file: annotate_overrides, camel_case_types, comment_references
// ignore_for_file: constant_identifier_names
// ignore_for_file: curly_braces_in_flow_control_structures
// ignore_for_file: deprecated_member_use_from_same_package, library_prefixes
// ignore_for_file: non_constant_identifier_names, prefer_relative_imports
// ignore_for_file: unused_import

import 'dart:convert' as $convert;
import 'dart:core' as $core;
import 'dart:typed_data' as $typed_data;

import 'package:protobuf/well_known_types/google/protobuf/timestamp.pbjson.dart'
    as $0;

@$core.Deprecated('Use noteDescriptor instead')
const Note$json = {
  '1': 'Note',
  '2': [
    {'1': 'id', '3': 1, '4': 1, '5': 9, '10': 'id'},
    {'1': 'title', '3': 2, '4': 1, '5': 9, '10': 'title'},
    {'1': 'body', '3': 3, '4': 1, '5': 9, '10': 'body'},
    {
      '1': 'created_at',
      '3': 4,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Timestamp',
      '10': 'createdAt'
    },
    {
      '1': 'updated_at',
      '3': 5,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Timestamp',
      '10': 'updatedAt'
    },
    {'1': 'pinned', '3': 6, '4': 1, '5': 8, '10': 'pinned'},
    {'1': 'project', '3': 7, '4': 1, '5': 9, '10': 'project'},
    {'1': 'source', '3': 8, '4': 1, '5': 9, '10': 'source'},
    {'1': 'state', '3': 9, '4': 1, '5': 9, '10': 'state'},
    {
      '1': 'completion',
      '3': 10,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.NoteCompletion',
      '10': 'completion'
    },
  ],
};

/// Descriptor for `Note`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List noteDescriptor = $convert.base64Decode(
    'CgROb3RlEg4KAmlkGAEgASgJUgJpZBIUCgV0aXRsZRgCIAEoCVIFdGl0bGUSEgoEYm9keRgDIA'
    'EoCVIEYm9keRI5CgpjcmVhdGVkX2F0GAQgASgLMhouZ29vZ2xlLnByb3RvYnVmLlRpbWVzdGFt'
    'cFIJY3JlYXRlZEF0EjkKCnVwZGF0ZWRfYXQYBSABKAsyGi5nb29nbGUucHJvdG9idWYuVGltZX'
    'N0YW1wUgl1cGRhdGVkQXQSFgoGcGlubmVkGAYgASgIUgZwaW5uZWQSGAoHcHJvamVjdBgHIAEo'
    'CVIHcHJvamVjdBIWCgZzb3VyY2UYCCABKAlSBnNvdXJjZRIUCgVzdGF0ZRgJIAEoCVIFc3RhdG'
    'USPwoKY29tcGxldGlvbhgKIAEoCzIfLnByb3RlY3RvcmF0ZS52MS5Ob3RlQ29tcGxldGlvblIK'
    'Y29tcGxldGlvbg==');

@$core.Deprecated('Use listNotesRequestDescriptor instead')
const ListNotesRequest$json = {
  '1': 'ListNotesRequest',
  '2': [
    {'1': 'project', '3': 1, '4': 1, '5': 9, '10': 'project'},
    {'1': 'query', '3': 2, '4': 1, '5': 9, '10': 'query'},
    {'1': 'state', '3': 3, '4': 1, '5': 9, '10': 'state'},
  ],
};

/// Descriptor for `ListNotesRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List listNotesRequestDescriptor = $convert.base64Decode(
    'ChBMaXN0Tm90ZXNSZXF1ZXN0EhgKB3Byb2plY3QYASABKAlSB3Byb2plY3QSFAoFcXVlcnkYAi'
    'ABKAlSBXF1ZXJ5EhQKBXN0YXRlGAMgASgJUgVzdGF0ZQ==');

@$core.Deprecated('Use listNotesResponseDescriptor instead')
const ListNotesResponse$json = {
  '1': 'ListNotesResponse',
  '2': [
    {
      '1': 'notes',
      '3': 1,
      '4': 3,
      '5': 11,
      '6': '.protectorate.v1.Note',
      '10': 'notes'
    },
  ],
};

/// Descriptor for `ListNotesResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List listNotesResponseDescriptor = $convert.base64Decode(
    'ChFMaXN0Tm90ZXNSZXNwb25zZRIrCgVub3RlcxgBIAMoCzIVLnByb3RlY3RvcmF0ZS52MS5Ob3'
    'RlUgVub3Rlcw==');

@$core.Deprecated('Use getNoteRequestDescriptor instead')
const GetNoteRequest$json = {
  '1': 'GetNoteRequest',
  '2': [
    {'1': 'id', '3': 1, '4': 1, '5': 9, '10': 'id'},
  ],
};

/// Descriptor for `GetNoteRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List getNoteRequestDescriptor =
    $convert.base64Decode('Cg5HZXROb3RlUmVxdWVzdBIOCgJpZBgBIAEoCVICaWQ=');

@$core.Deprecated('Use getNoteResponseDescriptor instead')
const GetNoteResponse$json = {
  '1': 'GetNoteResponse',
  '2': [
    {
      '1': 'note',
      '3': 1,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.Note',
      '10': 'note'
    },
  ],
};

/// Descriptor for `GetNoteResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List getNoteResponseDescriptor = $convert.base64Decode(
    'Cg9HZXROb3RlUmVzcG9uc2USKQoEbm90ZRgBIAEoCzIVLnByb3RlY3RvcmF0ZS52MS5Ob3RlUg'
    'Rub3Rl');

@$core.Deprecated('Use createNoteRequestDescriptor instead')
const CreateNoteRequest$json = {
  '1': 'CreateNoteRequest',
  '2': [
    {'1': 'title', '3': 1, '4': 1, '5': 9, '10': 'title'},
    {'1': 'body', '3': 2, '4': 1, '5': 9, '10': 'body'},
    {'1': 'pinned', '3': 3, '4': 1, '5': 8, '10': 'pinned'},
    {'1': 'project', '3': 4, '4': 1, '5': 9, '10': 'project'},
  ],
};

/// Descriptor for `CreateNoteRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List createNoteRequestDescriptor = $convert.base64Decode(
    'ChFDcmVhdGVOb3RlUmVxdWVzdBIUCgV0aXRsZRgBIAEoCVIFdGl0bGUSEgoEYm9keRgCIAEoCV'
    'IEYm9keRIWCgZwaW5uZWQYAyABKAhSBnBpbm5lZBIYCgdwcm9qZWN0GAQgASgJUgdwcm9qZWN0');

@$core.Deprecated('Use createNoteResponseDescriptor instead')
const CreateNoteResponse$json = {
  '1': 'CreateNoteResponse',
  '2': [
    {
      '1': 'note',
      '3': 1,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.Note',
      '10': 'note'
    },
  ],
};

/// Descriptor for `CreateNoteResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List createNoteResponseDescriptor = $convert.base64Decode(
    'ChJDcmVhdGVOb3RlUmVzcG9uc2USKQoEbm90ZRgBIAEoCzIVLnByb3RlY3RvcmF0ZS52MS5Ob3'
    'RlUgRub3Rl');

@$core.Deprecated('Use updateNoteRequestDescriptor instead')
const UpdateNoteRequest$json = {
  '1': 'UpdateNoteRequest',
  '2': [
    {'1': 'id', '3': 1, '4': 1, '5': 9, '10': 'id'},
    {'1': 'title', '3': 2, '4': 1, '5': 9, '9': 0, '10': 'title', '17': true},
    {'1': 'body', '3': 3, '4': 1, '5': 9, '9': 1, '10': 'body', '17': true},
    {'1': 'pinned', '3': 4, '4': 1, '5': 8, '9': 2, '10': 'pinned', '17': true},
    {
      '1': 'project',
      '3': 5,
      '4': 1,
      '5': 9,
      '9': 3,
      '10': 'project',
      '17': true
    },
    {'1': 'state', '3': 6, '4': 1, '5': 9, '9': 4, '10': 'state', '17': true},
    {
      '1': 'resolution',
      '3': 7,
      '4': 1,
      '5': 9,
      '9': 5,
      '10': 'resolution',
      '17': true
    },
  ],
  '8': [
    {'1': '_title'},
    {'1': '_body'},
    {'1': '_pinned'},
    {'1': '_project'},
    {'1': '_state'},
    {'1': '_resolution'},
  ],
};

/// Descriptor for `UpdateNoteRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List updateNoteRequestDescriptor = $convert.base64Decode(
    'ChFVcGRhdGVOb3RlUmVxdWVzdBIOCgJpZBgBIAEoCVICaWQSGQoFdGl0bGUYAiABKAlIAFIFdG'
    'l0bGWIAQESFwoEYm9keRgDIAEoCUgBUgRib2R5iAEBEhsKBnBpbm5lZBgEIAEoCEgCUgZwaW5u'
    'ZWSIAQESHQoHcHJvamVjdBgFIAEoCUgDUgdwcm9qZWN0iAEBEhkKBXN0YXRlGAYgASgJSARSBX'
    'N0YXRliAEBEiMKCnJlc29sdXRpb24YByABKAlIBVIKcmVzb2x1dGlvbogBAUIICgZfdGl0bGVC'
    'BwoFX2JvZHlCCQoHX3Bpbm5lZEIKCghfcHJvamVjdEIICgZfc3RhdGVCDQoLX3Jlc29sdXRpb2'
    '4=');

@$core.Deprecated('Use updateNoteResponseDescriptor instead')
const UpdateNoteResponse$json = {
  '1': 'UpdateNoteResponse',
  '2': [
    {
      '1': 'note',
      '3': 1,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.Note',
      '10': 'note'
    },
  ],
};

/// Descriptor for `UpdateNoteResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List updateNoteResponseDescriptor = $convert.base64Decode(
    'ChJVcGRhdGVOb3RlUmVzcG9uc2USKQoEbm90ZRgBIAEoCzIVLnByb3RlY3RvcmF0ZS52MS5Ob3'
    'RlUgRub3Rl');

@$core.Deprecated('Use deleteNoteRequestDescriptor instead')
const DeleteNoteRequest$json = {
  '1': 'DeleteNoteRequest',
  '2': [
    {'1': 'id', '3': 1, '4': 1, '5': 9, '10': 'id'},
  ],
};

/// Descriptor for `DeleteNoteRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List deleteNoteRequestDescriptor =
    $convert.base64Decode('ChFEZWxldGVOb3RlUmVxdWVzdBIOCgJpZBgBIAEoCVICaWQ=');

@$core.Deprecated('Use deleteNoteResponseDescriptor instead')
const DeleteNoteResponse$json = {
  '1': 'DeleteNoteResponse',
};

/// Descriptor for `DeleteNoteResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List deleteNoteResponseDescriptor =
    $convert.base64Decode('ChJEZWxldGVOb3RlUmVzcG9uc2U=');

@$core.Deprecated('Use noteCompletionDescriptor instead')
const NoteCompletion$json = {
  '1': 'NoteCompletion',
  '2': [
    {
      '1': 'done_at',
      '3': 1,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Timestamp',
      '10': 'doneAt'
    },
    {'1': 'done_by', '3': 2, '4': 1, '5': 9, '10': 'doneBy'},
    {'1': 'resolution', '3': 3, '4': 1, '5': 9, '10': 'resolution'},
  ],
};

/// Descriptor for `NoteCompletion`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List noteCompletionDescriptor = $convert.base64Decode(
    'Cg5Ob3RlQ29tcGxldGlvbhIzCgdkb25lX2F0GAEgASgLMhouZ29vZ2xlLnByb3RvYnVmLlRpbW'
    'VzdGFtcFIGZG9uZUF0EhcKB2RvbmVfYnkYAiABKAlSBmRvbmVCeRIeCgpyZXNvbHV0aW9uGAMg'
    'ASgJUgpyZXNvbHV0aW9u');

const $core.Map<$core.String, $core.dynamic> BacklogServiceBase$json = {
  '1': 'BacklogService',
  '2': [
    {
      '1': 'ListNotes',
      '2': '.protectorate.v1.ListNotesRequest',
      '3': '.protectorate.v1.ListNotesResponse',
      '4': {'34': 1},
    },
    {
      '1': 'GetNote',
      '2': '.protectorate.v1.GetNoteRequest',
      '3': '.protectorate.v1.GetNoteResponse',
      '4': {'34': 1},
    },
    {
      '1': 'CreateNote',
      '2': '.protectorate.v1.CreateNoteRequest',
      '3': '.protectorate.v1.CreateNoteResponse'
    },
    {
      '1': 'UpdateNote',
      '2': '.protectorate.v1.UpdateNoteRequest',
      '3': '.protectorate.v1.UpdateNoteResponse'
    },
    {
      '1': 'DeleteNote',
      '2': '.protectorate.v1.DeleteNoteRequest',
      '3': '.protectorate.v1.DeleteNoteResponse',
      '4': {}
    },
  ],
};

@$core.Deprecated('Use backlogServiceDescriptor instead')
const $core.Map<$core.String, $core.Map<$core.String, $core.dynamic>>
    BacklogServiceBase$messageJson = {
  '.protectorate.v1.ListNotesRequest': ListNotesRequest$json,
  '.protectorate.v1.ListNotesResponse': ListNotesResponse$json,
  '.protectorate.v1.Note': Note$json,
  '.google.protobuf.Timestamp': $0.Timestamp$json,
  '.protectorate.v1.NoteCompletion': NoteCompletion$json,
  '.protectorate.v1.GetNoteRequest': GetNoteRequest$json,
  '.protectorate.v1.GetNoteResponse': GetNoteResponse$json,
  '.protectorate.v1.CreateNoteRequest': CreateNoteRequest$json,
  '.protectorate.v1.CreateNoteResponse': CreateNoteResponse$json,
  '.protectorate.v1.UpdateNoteRequest': UpdateNoteRequest$json,
  '.protectorate.v1.UpdateNoteResponse': UpdateNoteResponse$json,
  '.protectorate.v1.DeleteNoteRequest': DeleteNoteRequest$json,
  '.protectorate.v1.DeleteNoteResponse': DeleteNoteResponse$json,
};

/// Descriptor for `BacklogService`. Decode as a `google.protobuf.ServiceDescriptorProto`.
final $typed_data.Uint8List backlogServiceDescriptor = $convert.base64Decode(
    'Cg5CYWNrbG9nU2VydmljZRJXCglMaXN0Tm90ZXMSIS5wcm90ZWN0b3JhdGUudjEuTGlzdE5vdG'
    'VzUmVxdWVzdBoiLnByb3RlY3RvcmF0ZS52MS5MaXN0Tm90ZXNSZXNwb25zZSIDkAIBElEKB0dl'
    'dE5vdGUSHy5wcm90ZWN0b3JhdGUudjEuR2V0Tm90ZVJlcXVlc3QaIC5wcm90ZWN0b3JhdGUudj'
    'EuR2V0Tm90ZVJlc3BvbnNlIgOQAgESVQoKQ3JlYXRlTm90ZRIiLnByb3RlY3RvcmF0ZS52MS5D'
    'cmVhdGVOb3RlUmVxdWVzdBojLnByb3RlY3RvcmF0ZS52MS5DcmVhdGVOb3RlUmVzcG9uc2USVQ'
    'oKVXBkYXRlTm90ZRIiLnByb3RlY3RvcmF0ZS52MS5VcGRhdGVOb3RlUmVxdWVzdBojLnByb3Rl'
    'Y3RvcmF0ZS52MS5VcGRhdGVOb3RlUmVzcG9uc2USWwoKRGVsZXRlTm90ZRIiLnByb3RlY3Rvcm'
    'F0ZS52MS5EZWxldGVOb3RlUmVxdWVzdBojLnByb3RlY3RvcmF0ZS52MS5EZWxldGVOb3RlUmVz'
    'cG9uc2UiBIi1GAE=');
