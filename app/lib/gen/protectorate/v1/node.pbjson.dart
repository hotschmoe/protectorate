// This is a generated file - do not edit.
//
// Generated from protectorate/v1/node.proto.

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

import 'package:protobuf/well_known_types/google/protobuf/duration.pbjson.dart'
    as $3;
import 'package:protobuf/well_known_types/google/protobuf/timestamp.pbjson.dart'
    as $1;

import 'chat.pbjson.dart' as $5;
import 'desk.pbjson.dart' as $0;
import 'job.pbjson.dart' as $4;
import 'sleeve.pbjson.dart' as $2;

@$core.Deprecated('Use updatePhaseDescriptor instead')
const UpdatePhase$json = {
  '1': 'UpdatePhase',
  '2': [
    {'1': 'UPDATE_PHASE_UNSPECIFIED', '2': 0},
    {'1': 'UPDATE_PHASE_SKIPPED', '2': 1},
    {'1': 'UPDATE_PHASE_SENDING', '2': 2},
    {'1': 'UPDATE_PHASE_VERIFYING', '2': 3},
    {'1': 'UPDATE_PHASE_SWAPPING', '2': 4},
    {'1': 'UPDATE_PHASE_RESTARTING', '2': 5},
    {'1': 'UPDATE_PHASE_CONFIRMED', '2': 6},
    {'1': 'UPDATE_PHASE_ROLLED_BACK', '2': 7},
  ],
};

/// Descriptor for `UpdatePhase`. Decode as a `google.protobuf.EnumDescriptorProto`.
final $typed_data.Uint8List updatePhaseDescriptor = $convert.base64Decode(
    'CgtVcGRhdGVQaGFzZRIcChhVUERBVEVfUEhBU0VfVU5TUEVDSUZJRUQQABIYChRVUERBVEVfUE'
    'hBU0VfU0tJUFBFRBABEhgKFFVQREFURV9QSEFTRV9TRU5ESU5HEAISGgoWVVBEQVRFX1BIQVNF'
    'X1ZFUklGWUlORxADEhkKFVVQREFURV9QSEFTRV9TV0FQUElORxAEEhsKF1VQREFURV9QSEFTRV'
    '9SRVNUQVJUSU5HEAUSGgoWVVBEQVRFX1BIQVNFX0NPTkZJUk1FRBAGEhwKGFVQREFURV9QSEFT'
    'RV9ST0xMRURfQkFDSxAH');

@$core.Deprecated('Use fileKindDescriptor instead')
const FileKind$json = {
  '1': 'FileKind',
  '2': [
    {'1': 'FILE_KIND_UNSPECIFIED', '2': 0},
    {'1': 'FILE_KIND_FILE', '2': 1},
    {'1': 'FILE_KIND_DIRECTORY', '2': 2},
  ],
};

/// Descriptor for `FileKind`. Decode as a `google.protobuf.EnumDescriptorProto`.
final $typed_data.Uint8List fileKindDescriptor = $convert.base64Decode(
    'CghGaWxlS2luZBIZChVGSUxFX0tJTkRfVU5TUEVDSUZJRUQQABISCg5GSUxFX0tJTkRfRklMRR'
    'ABEhcKE0ZJTEVfS0lORF9ESVJFQ1RPUlkQAg==');

@$core.Deprecated('Use nodeFreshnessDescriptor instead')
const NodeFreshness$json = {
  '1': 'NodeFreshness',
  '2': [
    {'1': 'NODE_FRESHNESS_UNSPECIFIED', '2': 0},
    {'1': 'NODE_FRESHNESS_FRESH', '2': 1},
    {'1': 'NODE_FRESHNESS_STALE', '2': 2},
    {'1': 'NODE_FRESHNESS_UNREACHABLE', '2': 3},
    {'1': 'NODE_FRESHNESS_INVITED', '2': 4},
  ],
};

/// Descriptor for `NodeFreshness`. Decode as a `google.protobuf.EnumDescriptorProto`.
final $typed_data.Uint8List nodeFreshnessDescriptor = $convert.base64Decode(
    'Cg1Ob2RlRnJlc2huZXNzEh4KGk5PREVfRlJFU0hORVNTX1VOU1BFQ0lGSUVEEAASGAoUTk9ERV'
    '9GUkVTSE5FU1NfRlJFU0gQARIYChROT0RFX0ZSRVNITkVTU19TVEFMRRACEh4KGk5PREVfRlJF'
    'U0hORVNTX1VOUkVBQ0hBQkxFEAMSGgoWTk9ERV9GUkVTSE5FU1NfSU5WSVRFRBAE');

@$core.Deprecated('Use alertKindDescriptor instead')
const AlertKind$json = {
  '1': 'AlertKind',
  '2': [
    {'1': 'ALERT_KIND_UNSPECIFIED', '2': 0},
    {'1': 'ALERT_KIND_DISK_PERCENT', '2': 1},
    {'1': 'ALERT_KIND_MEM_PRESSURE', '2': 2},
    {'1': 'ALERT_KIND_UNREACHABLE', '2': 3},
    {'1': 'ALERT_KIND_NAME_MISMATCH', '2': 4},
  ],
};

/// Descriptor for `AlertKind`. Decode as a `google.protobuf.EnumDescriptorProto`.
final $typed_data.Uint8List alertKindDescriptor = $convert.base64Decode(
    'CglBbGVydEtpbmQSGgoWQUxFUlRfS0lORF9VTlNQRUNJRklFRBAAEhsKF0FMRVJUX0tJTkRfRE'
    'lTS19QRVJDRU5UEAESGwoXQUxFUlRfS0lORF9NRU1fUFJFU1NVUkUQAhIaChZBTEVSVF9LSU5E'
    'X1VOUkVBQ0hBQkxFEAMSHAoYQUxFUlRfS0lORF9OQU1FX01JU01BVENIEAQ=');

@$core.Deprecated('Use getDeskStateRequestDescriptor instead')
const GetDeskStateRequest$json = {
  '1': 'GetDeskStateRequest',
  '2': [
    {
      '1': 'policy',
      '3': 1,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.DeskPolicy',
      '10': 'policy'
    },
    {'1': 'lease_ended', '3': 2, '4': 1, '5': 8, '10': 'leaseEnded'},
  ],
};

/// Descriptor for `GetDeskStateRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List getDeskStateRequestDescriptor = $convert.base64Decode(
    'ChNHZXREZXNrU3RhdGVSZXF1ZXN0EjMKBnBvbGljeRgBIAEoCzIbLnByb3RlY3RvcmF0ZS52MS'
    '5EZXNrUG9saWN5UgZwb2xpY3kSHwoLbGVhc2VfZW5kZWQYAiABKAhSCmxlYXNlRW5kZWQ=');

@$core.Deprecated('Use setDeskPolicyRequestDescriptor instead')
const SetDeskPolicyRequest$json = {
  '1': 'SetDeskPolicyRequest',
  '2': [
    {'1': 'name', '3': 1, '4': 1, '5': 9, '10': 'name'},
    {'1': 'allow_family', '3': 2, '4': 1, '5': 8, '10': 'allowFamily'},
    {
      '1': 'policy',
      '3': 3,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.DeskPolicy',
      '10': 'policy'
    },
  ],
};

/// Descriptor for `SetDeskPolicyRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List setDeskPolicyRequestDescriptor = $convert.base64Decode(
    'ChRTZXREZXNrUG9saWN5UmVxdWVzdBISCgRuYW1lGAEgASgJUgRuYW1lEiEKDGFsbG93X2ZhbW'
    'lseRgCIAEoCFILYWxsb3dGYW1pbHkSMwoGcG9saWN5GAMgASgLMhsucHJvdGVjdG9yYXRlLnYx'
    'LkRlc2tQb2xpY3lSBnBvbGljeQ==');

@$core.Deprecated('Use getDeskPolicyRequestDescriptor instead')
const GetDeskPolicyRequest$json = {
  '1': 'GetDeskPolicyRequest',
  '2': [
    {'1': 'name', '3': 1, '4': 1, '5': 9, '10': 'name'},
    {'1': 'allow_family', '3': 2, '4': 1, '5': 8, '10': 'allowFamily'},
  ],
};

/// Descriptor for `GetDeskPolicyRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List getDeskPolicyRequestDescriptor = $convert.base64Decode(
    'ChRHZXREZXNrUG9saWN5UmVxdWVzdBISCgRuYW1lGAEgASgJUgRuYW1lEiEKDGFsbG93X2ZhbW'
    'lseRgCIAEoCFILYWxsb3dGYW1pbHk=');

@$core.Deprecated('Use deskPolicyStatusDescriptor instead')
const DeskPolicyStatus$json = {
  '1': 'DeskPolicyStatus',
  '2': [
    {
      '1': 'policy',
      '3': 1,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.DeskPolicy',
      '10': 'policy'
    },
    {
      '1': 'state',
      '3': 2,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.DeskState',
      '10': 'state'
    },
    {'1': 'holder', '3': 3, '4': 1, '5': 9, '10': 'holder'},
  ],
};

/// Descriptor for `DeskPolicyStatus`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List deskPolicyStatusDescriptor = $convert.base64Decode(
    'ChBEZXNrUG9saWN5U3RhdHVzEjMKBnBvbGljeRgBIAEoCzIbLnByb3RlY3RvcmF0ZS52MS5EZX'
    'NrUG9saWN5UgZwb2xpY3kSMAoFc3RhdGUYAiABKAsyGi5wcm90ZWN0b3JhdGUudjEuRGVza1N0'
    'YXRlUgVzdGF0ZRIWCgZob2xkZXIYAyABKAlSBmhvbGRlcg==');

@$core.Deprecated('Use updateNodeRequestDescriptor instead')
const UpdateNodeRequest$json = {
  '1': 'UpdateNodeRequest',
  '2': [
    {'1': 'name', '3': 1, '4': 1, '5': 9, '10': 'name'},
    {'1': 'allow_family', '3': 2, '4': 1, '5': 8, '10': 'allowFamily'},
    {'1': 'force', '3': 3, '4': 1, '5': 8, '10': 'force'},
  ],
};

/// Descriptor for `UpdateNodeRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List updateNodeRequestDescriptor = $convert.base64Decode(
    'ChFVcGRhdGVOb2RlUmVxdWVzdBISCgRuYW1lGAEgASgJUgRuYW1lEiEKDGFsbG93X2ZhbWlseR'
    'gCIAEoCFILYWxsb3dGYW1pbHkSFAoFZm9yY2UYAyABKAhSBWZvcmNl');

@$core.Deprecated('Use updateNodeProgressDescriptor instead')
const UpdateNodeProgress$json = {
  '1': 'UpdateNodeProgress',
  '2': [
    {
      '1': 'phase',
      '3': 1,
      '4': 1,
      '5': 14,
      '6': '.protectorate.v1.UpdatePhase',
      '10': 'phase'
    },
    {'1': 'sent_bytes', '3': 2, '4': 1, '5': 4, '10': 'sentBytes'},
    {'1': 'total_bytes', '3': 3, '4': 1, '5': 4, '10': 'totalBytes'},
    {'1': 'version', '3': 4, '4': 1, '5': 9, '10': 'version'},
    {'1': 'previous_version', '3': 5, '4': 1, '5': 9, '10': 'previousVersion'},
    {'1': 'restart', '3': 6, '4': 1, '5': 9, '10': 'restart'},
    {'1': 'message', '3': 7, '4': 1, '5': 9, '10': 'message'},
  ],
};

/// Descriptor for `UpdateNodeProgress`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List updateNodeProgressDescriptor = $convert.base64Decode(
    'ChJVcGRhdGVOb2RlUHJvZ3Jlc3MSMgoFcGhhc2UYASABKA4yHC5wcm90ZWN0b3JhdGUudjEuVX'
    'BkYXRlUGhhc2VSBXBoYXNlEh0KCnNlbnRfYnl0ZXMYAiABKARSCXNlbnRCeXRlcxIfCgt0b3Rh'
    'bF9ieXRlcxgDIAEoBFIKdG90YWxCeXRlcxIYCgd2ZXJzaW9uGAQgASgJUgd2ZXJzaW9uEikKEH'
    'ByZXZpb3VzX3ZlcnNpb24YBSABKAlSD3ByZXZpb3VzVmVyc2lvbhIYCgdyZXN0YXJ0GAYgASgJ'
    'UgdyZXN0YXJ0EhgKB21lc3NhZ2UYByABKAlSB21lc3NhZ2U=');

@$core.Deprecated('Use applyUpdateRequestDescriptor instead')
const ApplyUpdateRequest$json = {
  '1': 'ApplyUpdateRequest',
  '2': [
    {'1': 'name', '3': 1, '4': 1, '5': 9, '10': 'name'},
    {'1': 'allow_family', '3': 2, '4': 1, '5': 8, '10': 'allowFamily'},
    {'1': 'path', '3': 3, '4': 1, '5': 9, '10': 'path'},
    {'1': 'size', '3': 4, '4': 1, '5': 4, '10': 'size'},
    {'1': 'sha256', '3': 5, '4': 1, '5': 9, '10': 'sha256'},
  ],
};

/// Descriptor for `ApplyUpdateRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List applyUpdateRequestDescriptor = $convert.base64Decode(
    'ChJBcHBseVVwZGF0ZVJlcXVlc3QSEgoEbmFtZRgBIAEoCVIEbmFtZRIhCgxhbGxvd19mYW1pbH'
    'kYAiABKAhSC2FsbG93RmFtaWx5EhIKBHBhdGgYAyABKAlSBHBhdGgSEgoEc2l6ZRgEIAEoBFIE'
    'c2l6ZRIWCgZzaGEyNTYYBSABKAlSBnNoYTI1Ng==');

@$core.Deprecated('Use applyUpdateResponseDescriptor instead')
const ApplyUpdateResponse$json = {
  '1': 'ApplyUpdateResponse',
  '2': [
    {'1': 'version', '3': 1, '4': 1, '5': 9, '10': 'version'},
    {'1': 'previous_version', '3': 2, '4': 1, '5': 9, '10': 'previousVersion'},
    {'1': 'restart', '3': 3, '4': 1, '5': 9, '10': 'restart'},
  ],
};

/// Descriptor for `ApplyUpdateResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List applyUpdateResponseDescriptor = $convert.base64Decode(
    'ChNBcHBseVVwZGF0ZVJlc3BvbnNlEhgKB3ZlcnNpb24YASABKAlSB3ZlcnNpb24SKQoQcHJldm'
    'lvdXNfdmVyc2lvbhgCIAEoCVIPcHJldmlvdXNWZXJzaW9uEhgKB3Jlc3RhcnQYAyABKAlSB3Jl'
    'c3RhcnQ=');

@$core.Deprecated('Use fileChunkDescriptor instead')
const FileChunk$json = {
  '1': 'FileChunk',
  '2': [
    {'1': 'data', '3': 1, '4': 1, '5': 12, '10': 'data'},
    {'1': 'compressed', '3': 2, '4': 1, '5': 8, '10': 'compressed'},
    {'1': 'raw_size', '3': 3, '4': 1, '5': 13, '10': 'rawSize'},
  ],
};

/// Descriptor for `FileChunk`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List fileChunkDescriptor = $convert.base64Decode(
    'CglGaWxlQ2h1bmsSEgoEZGF0YRgBIAEoDFIEZGF0YRIeCgpjb21wcmVzc2VkGAIgASgIUgpjb2'
    '1wcmVzc2VkEhkKCHJhd19zaXplGAMgASgNUgdyYXdTaXpl');

@$core.Deprecated('Use fileTrailerDescriptor instead')
const FileTrailer$json = {
  '1': 'FileTrailer',
  '2': [
    {'1': 'size', '3': 1, '4': 1, '5': 4, '10': 'size'},
    {'1': 'sha256', '3': 2, '4': 1, '5': 9, '10': 'sha256'},
    {'1': 'skipped', '3': 3, '4': 1, '5': 13, '10': 'skipped'},
  ],
};

/// Descriptor for `FileTrailer`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List fileTrailerDescriptor = $convert.base64Decode(
    'CgtGaWxlVHJhaWxlchISCgRzaXplGAEgASgEUgRzaXplEhYKBnNoYTI1NhgCIAEoCVIGc2hhMj'
    'U2EhgKB3NraXBwZWQYAyABKA1SB3NraXBwZWQ=');

@$core.Deprecated('Use putFileRequestDescriptor instead')
const PutFileRequest$json = {
  '1': 'PutFileRequest',
  '2': [
    {'1': 'name', '3': 1, '4': 1, '5': 9, '10': 'name'},
    {'1': 'allow_family', '3': 2, '4': 1, '5': 8, '10': 'allowFamily'},
    {'1': 'transfer_id', '3': 3, '4': 1, '5': 9, '10': 'transferId'},
    {'1': 'path', '3': 4, '4': 1, '5': 9, '10': 'path'},
    {
      '1': 'kind',
      '3': 5,
      '4': 1,
      '5': 14,
      '6': '.protectorate.v1.FileKind',
      '10': 'kind'
    },
    {'1': 'mode', '3': 6, '4': 1, '5': 13, '10': 'mode'},
    {'1': 'overwrite', '3': 7, '4': 1, '5': 8, '10': 'overwrite'},
    {'1': 'offset', '3': 8, '4': 1, '5': 4, '10': 'offset'},
    {
      '1': 'chunk',
      '3': 9,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.FileChunk',
      '10': 'chunk'
    },
    {'1': 'last', '3': 10, '4': 1, '5': 8, '10': 'last'},
    {
      '1': 'trailer',
      '3': 11,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.FileTrailer',
      '10': 'trailer'
    },
  ],
};

/// Descriptor for `PutFileRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List putFileRequestDescriptor = $convert.base64Decode(
    'Cg5QdXRGaWxlUmVxdWVzdBISCgRuYW1lGAEgASgJUgRuYW1lEiEKDGFsbG93X2ZhbWlseRgCIA'
    'EoCFILYWxsb3dGYW1pbHkSHwoLdHJhbnNmZXJfaWQYAyABKAlSCnRyYW5zZmVySWQSEgoEcGF0'
    'aBgEIAEoCVIEcGF0aBItCgRraW5kGAUgASgOMhkucHJvdGVjdG9yYXRlLnYxLkZpbGVLaW5kUg'
    'RraW5kEhIKBG1vZGUYBiABKA1SBG1vZGUSHAoJb3ZlcndyaXRlGAcgASgIUglvdmVyd3JpdGUS'
    'FgoGb2Zmc2V0GAggASgEUgZvZmZzZXQSMAoFY2h1bmsYCSABKAsyGi5wcm90ZWN0b3JhdGUudj'
    'EuRmlsZUNodW5rUgVjaHVuaxISCgRsYXN0GAogASgIUgRsYXN0EjYKB3RyYWlsZXIYCyABKAsy'
    'HC5wcm90ZWN0b3JhdGUudjEuRmlsZVRyYWlsZXJSB3RyYWlsZXI=');

@$core.Deprecated('Use putFileResponseDescriptor instead')
const PutFileResponse$json = {
  '1': 'PutFileResponse',
  '2': [
    {'1': 'path', '3': 1, '4': 1, '5': 9, '10': 'path'},
    {'1': 'received', '3': 2, '4': 1, '5': 4, '10': 'received'},
    {'1': 'done', '3': 3, '4': 1, '5': 8, '10': 'done'},
  ],
};

/// Descriptor for `PutFileResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List putFileResponseDescriptor = $convert.base64Decode(
    'Cg9QdXRGaWxlUmVzcG9uc2USEgoEcGF0aBgBIAEoCVIEcGF0aBIaCghyZWNlaXZlZBgCIAEoBF'
    'IIcmVjZWl2ZWQSEgoEZG9uZRgDIAEoCFIEZG9uZQ==');

@$core.Deprecated('Use getFileRequestDescriptor instead')
const GetFileRequest$json = {
  '1': 'GetFileRequest',
  '2': [
    {'1': 'name', '3': 1, '4': 1, '5': 9, '10': 'name'},
    {'1': 'allow_family', '3': 2, '4': 1, '5': 8, '10': 'allowFamily'},
    {'1': 'path', '3': 3, '4': 1, '5': 9, '10': 'path'},
    {'1': 'no_compress', '3': 4, '4': 1, '5': 8, '10': 'noCompress'},
  ],
};

/// Descriptor for `GetFileRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List getFileRequestDescriptor = $convert.base64Decode(
    'Cg5HZXRGaWxlUmVxdWVzdBISCgRuYW1lGAEgASgJUgRuYW1lEiEKDGFsbG93X2ZhbWlseRgCIA'
    'EoCFILYWxsb3dGYW1pbHkSEgoEcGF0aBgDIAEoCVIEcGF0aBIfCgtub19jb21wcmVzcxgEIAEo'
    'CFIKbm9Db21wcmVzcw==');

@$core.Deprecated('Use fileHeaderDescriptor instead')
const FileHeader$json = {
  '1': 'FileHeader',
  '2': [
    {
      '1': 'kind',
      '3': 1,
      '4': 1,
      '5': 14,
      '6': '.protectorate.v1.FileKind',
      '10': 'kind'
    },
    {'1': 'mode', '3': 2, '4': 1, '5': 13, '10': 'mode'},
    {'1': 'size', '3': 3, '4': 1, '5': 4, '10': 'size'},
    {'1': 'path', '3': 4, '4': 1, '5': 9, '10': 'path'},
  ],
};

/// Descriptor for `FileHeader`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List fileHeaderDescriptor = $convert.base64Decode(
    'CgpGaWxlSGVhZGVyEi0KBGtpbmQYASABKA4yGS5wcm90ZWN0b3JhdGUudjEuRmlsZUtpbmRSBG'
    'tpbmQSEgoEbW9kZRgCIAEoDVIEbW9kZRISCgRzaXplGAMgASgEUgRzaXplEhIKBHBhdGgYBCAB'
    'KAlSBHBhdGg=');

@$core.Deprecated('Use getFileResponseDescriptor instead')
const GetFileResponse$json = {
  '1': 'GetFileResponse',
  '2': [
    {
      '1': 'header',
      '3': 1,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.FileHeader',
      '9': 0,
      '10': 'header'
    },
    {
      '1': 'chunk',
      '3': 2,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.FileChunk',
      '9': 0,
      '10': 'chunk'
    },
    {
      '1': 'trailer',
      '3': 3,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.FileTrailer',
      '9': 0,
      '10': 'trailer'
    },
  ],
  '8': [
    {'1': 'part'},
  ],
};

/// Descriptor for `GetFileResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List getFileResponseDescriptor = $convert.base64Decode(
    'Cg9HZXRGaWxlUmVzcG9uc2USNQoGaGVhZGVyGAEgASgLMhsucHJvdGVjdG9yYXRlLnYxLkZpbG'
    'VIZWFkZXJIAFIGaGVhZGVyEjIKBWNodW5rGAIgASgLMhoucHJvdGVjdG9yYXRlLnYxLkZpbGVD'
    'aHVua0gAUgVjaHVuaxI4Cgd0cmFpbGVyGAMgASgLMhwucHJvdGVjdG9yYXRlLnYxLkZpbGVUcm'
    'FpbGVySABSB3RyYWlsZXJCBgoEcGFydA==');

@$core.Deprecated('Use getMetricsRequestDescriptor instead')
const GetMetricsRequest$json = {
  '1': 'GetMetricsRequest',
};

/// Descriptor for `GetMetricsRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List getMetricsRequestDescriptor =
    $convert.base64Decode('ChFHZXRNZXRyaWNzUmVxdWVzdA==');

@$core.Deprecated('Use watchMetricsRequestDescriptor instead')
const WatchMetricsRequest$json = {
  '1': 'WatchMetricsRequest',
  '2': [
    {'1': 'interval_seconds', '3': 1, '4': 1, '5': 5, '10': 'intervalSeconds'},
  ],
};

/// Descriptor for `WatchMetricsRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List watchMetricsRequestDescriptor = $convert.base64Decode(
    'ChNXYXRjaE1ldHJpY3NSZXF1ZXN0EikKEGludGVydmFsX3NlY29uZHMYASABKAVSD2ludGVydm'
    'FsU2Vjb25kcw==');

@$core.Deprecated('Use nodeMetricsDescriptor instead')
const NodeMetrics$json = {
  '1': 'NodeMetrics',
  '2': [
    {'1': 'hostname', '3': 1, '4': 1, '5': 9, '10': 'hostname'},
    {'1': 'os', '3': 2, '4': 1, '5': 9, '10': 'os'},
    {'1': 'arch', '3': 3, '4': 1, '5': 9, '10': 'arch'},
    {'1': 'platform', '3': 4, '4': 1, '5': 9, '10': 'platform'},
    {'1': 'agent_version', '3': 5, '4': 1, '5': 9, '10': 'agentVersion'},
    {'1': 'uptime_seconds', '3': 6, '4': 1, '5': 3, '10': 'uptimeSeconds'},
    {
      '1': 'collected_at',
      '3': 7,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Timestamp',
      '10': 'collectedAt'
    },
    {
      '1': 'cpu',
      '3': 8,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.CpuMetrics',
      '10': 'cpu'
    },
    {
      '1': 'memory',
      '3': 9,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.MemoryMetrics',
      '10': 'memory'
    },
    {
      '1': 'filesystems',
      '3': 10,
      '4': 3,
      '5': 11,
      '6': '.protectorate.v1.FilesystemMetrics',
      '10': 'filesystems'
    },
    {
      '1': 'gpus',
      '3': 11,
      '4': 3,
      '5': 11,
      '6': '.protectorate.v1.GpuMetrics',
      '10': 'gpus'
    },
    {
      '1': 'engine',
      '3': 12,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.ContainerEngine',
      '10': 'engine'
    },
    {
      '1': 'watch_dirs',
      '3': 13,
      '4': 3,
      '5': 11,
      '6': '.protectorate.v1.DirSize',
      '10': 'watchDirs'
    },
    {'1': 'warnings', '3': 14, '4': 3, '5': 9, '10': 'warnings'},
    {
      '1': 'sleeves',
      '3': 15,
      '4': 3,
      '5': 11,
      '6': '.protectorate.v1.Sleeve',
      '10': 'sleeves'
    },
  ],
};

/// Descriptor for `NodeMetrics`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List nodeMetricsDescriptor = $convert.base64Decode(
    'CgtOb2RlTWV0cmljcxIaCghob3N0bmFtZRgBIAEoCVIIaG9zdG5hbWUSDgoCb3MYAiABKAlSAm'
    '9zEhIKBGFyY2gYAyABKAlSBGFyY2gSGgoIcGxhdGZvcm0YBCABKAlSCHBsYXRmb3JtEiMKDWFn'
    'ZW50X3ZlcnNpb24YBSABKAlSDGFnZW50VmVyc2lvbhIlCg51cHRpbWVfc2Vjb25kcxgGIAEoA1'
    'INdXB0aW1lU2Vjb25kcxI9Cgxjb2xsZWN0ZWRfYXQYByABKAsyGi5nb29nbGUucHJvdG9idWYu'
    'VGltZXN0YW1wUgtjb2xsZWN0ZWRBdBItCgNjcHUYCCABKAsyGy5wcm90ZWN0b3JhdGUudjEuQ3'
    'B1TWV0cmljc1IDY3B1EjYKBm1lbW9yeRgJIAEoCzIeLnByb3RlY3RvcmF0ZS52MS5NZW1vcnlN'
    'ZXRyaWNzUgZtZW1vcnkSRAoLZmlsZXN5c3RlbXMYCiADKAsyIi5wcm90ZWN0b3JhdGUudjEuRm'
    'lsZXN5c3RlbU1ldHJpY3NSC2ZpbGVzeXN0ZW1zEi8KBGdwdXMYCyADKAsyGy5wcm90ZWN0b3Jh'
    'dGUudjEuR3B1TWV0cmljc1IEZ3B1cxI4CgZlbmdpbmUYDCABKAsyIC5wcm90ZWN0b3JhdGUudj'
    'EuQ29udGFpbmVyRW5naW5lUgZlbmdpbmUSNwoKd2F0Y2hfZGlycxgNIAMoCzIYLnByb3RlY3Rv'
    'cmF0ZS52MS5EaXJTaXplUgl3YXRjaERpcnMSGgoId2FybmluZ3MYDiADKAlSCHdhcm5pbmdzEj'
    'EKB3NsZWV2ZXMYDyADKAsyFy5wcm90ZWN0b3JhdGUudjEuU2xlZXZlUgdzbGVldmVz');

@$core.Deprecated('Use cpuMetricsDescriptor instead')
const CpuMetrics$json = {
  '1': 'CpuMetrics',
  '2': [
    {'1': 'logical_cores', '3': 1, '4': 1, '5': 5, '10': 'logicalCores'},
    {'1': 'usage_percent', '3': 2, '4': 1, '5': 1, '10': 'usagePercent'},
    {'1': 'load1', '3': 3, '4': 1, '5': 1, '9': 0, '10': 'load1', '17': true},
    {'1': 'load5', '3': 4, '4': 1, '5': 1, '9': 1, '10': 'load5', '17': true},
    {'1': 'load15', '3': 5, '4': 1, '5': 1, '9': 2, '10': 'load15', '17': true},
  ],
  '8': [
    {'1': '_load1'},
    {'1': '_load5'},
    {'1': '_load15'},
  ],
};

/// Descriptor for `CpuMetrics`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List cpuMetricsDescriptor = $convert.base64Decode(
    'CgpDcHVNZXRyaWNzEiMKDWxvZ2ljYWxfY29yZXMYASABKAVSDGxvZ2ljYWxDb3JlcxIjCg11c2'
    'FnZV9wZXJjZW50GAIgASgBUgx1c2FnZVBlcmNlbnQSGQoFbG9hZDEYAyABKAFIAFIFbG9hZDGI'
    'AQESGQoFbG9hZDUYBCABKAFIAVIFbG9hZDWIAQESGwoGbG9hZDE1GAUgASgBSAJSBmxvYWQxNY'
    'gBAUIICgZfbG9hZDFCCAoGX2xvYWQ1QgkKB19sb2FkMTU=');

@$core.Deprecated('Use memoryMetricsDescriptor instead')
const MemoryMetrics$json = {
  '1': 'MemoryMetrics',
  '2': [
    {'1': 'total_bytes', '3': 1, '4': 1, '5': 4, '10': 'totalBytes'},
    {'1': 'used_bytes', '3': 2, '4': 1, '5': 4, '10': 'usedBytes'},
    {'1': 'available_bytes', '3': 3, '4': 1, '5': 4, '10': 'availableBytes'},
    {'1': 'used_percent', '3': 4, '4': 1, '5': 1, '10': 'usedPercent'},
    {'1': 'swap_total_bytes', '3': 5, '4': 1, '5': 4, '10': 'swapTotalBytes'},
    {'1': 'swap_used_bytes', '3': 6, '4': 1, '5': 4, '10': 'swapUsedBytes'},
    {'1': 'swap_used_percent', '3': 7, '4': 1, '5': 1, '10': 'swapUsedPercent'},
  ],
};

/// Descriptor for `MemoryMetrics`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List memoryMetricsDescriptor = $convert.base64Decode(
    'Cg1NZW1vcnlNZXRyaWNzEh8KC3RvdGFsX2J5dGVzGAEgASgEUgp0b3RhbEJ5dGVzEh0KCnVzZW'
    'RfYnl0ZXMYAiABKARSCXVzZWRCeXRlcxInCg9hdmFpbGFibGVfYnl0ZXMYAyABKARSDmF2YWls'
    'YWJsZUJ5dGVzEiEKDHVzZWRfcGVyY2VudBgEIAEoAVILdXNlZFBlcmNlbnQSKAoQc3dhcF90b3'
    'RhbF9ieXRlcxgFIAEoBFIOc3dhcFRvdGFsQnl0ZXMSJgoPc3dhcF91c2VkX2J5dGVzGAYgASgE'
    'Ug1zd2FwVXNlZEJ5dGVzEioKEXN3YXBfdXNlZF9wZXJjZW50GAcgASgBUg9zd2FwVXNlZFBlcm'
    'NlbnQ=');

@$core.Deprecated('Use filesystemMetricsDescriptor instead')
const FilesystemMetrics$json = {
  '1': 'FilesystemMetrics',
  '2': [
    {'1': 'mount', '3': 1, '4': 1, '5': 9, '10': 'mount'},
    {'1': 'device', '3': 2, '4': 1, '5': 9, '10': 'device'},
    {'1': 'fstype', '3': 3, '4': 1, '5': 9, '10': 'fstype'},
    {'1': 'total_bytes', '3': 4, '4': 1, '5': 4, '10': 'totalBytes'},
    {'1': 'used_bytes', '3': 5, '4': 1, '5': 4, '10': 'usedBytes'},
    {'1': 'free_bytes', '3': 6, '4': 1, '5': 4, '10': 'freeBytes'},
    {'1': 'used_percent', '3': 7, '4': 1, '5': 1, '10': 'usedPercent'},
  ],
};

/// Descriptor for `FilesystemMetrics`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List filesystemMetricsDescriptor = $convert.base64Decode(
    'ChFGaWxlc3lzdGVtTWV0cmljcxIUCgVtb3VudBgBIAEoCVIFbW91bnQSFgoGZGV2aWNlGAIgAS'
    'gJUgZkZXZpY2USFgoGZnN0eXBlGAMgASgJUgZmc3R5cGUSHwoLdG90YWxfYnl0ZXMYBCABKARS'
    'CnRvdGFsQnl0ZXMSHQoKdXNlZF9ieXRlcxgFIAEoBFIJdXNlZEJ5dGVzEh0KCmZyZWVfYnl0ZX'
    'MYBiABKARSCWZyZWVCeXRlcxIhCgx1c2VkX3BlcmNlbnQYByABKAFSC3VzZWRQZXJjZW50');

@$core.Deprecated('Use gpuMetricsDescriptor instead')
const GpuMetrics$json = {
  '1': 'GpuMetrics',
  '2': [
    {'1': 'index', '3': 1, '4': 1, '5': 5, '10': 'index'},
    {'1': 'vendor', '3': 2, '4': 1, '5': 9, '10': 'vendor'},
    {'1': 'name', '3': 3, '4': 1, '5': 9, '10': 'name'},
    {
      '1': 'utilization_percent',
      '3': 4,
      '4': 1,
      '5': 1,
      '10': 'utilizationPercent'
    },
    {'1': 'memory_used_bytes', '3': 5, '4': 1, '5': 4, '10': 'memoryUsedBytes'},
    {
      '1': 'memory_total_bytes',
      '3': 6,
      '4': 1,
      '5': 4,
      '10': 'memoryTotalBytes'
    },
    {
      '1': 'temperature_celsius',
      '3': 7,
      '4': 1,
      '5': 1,
      '10': 'temperatureCelsius'
    },
  ],
};

/// Descriptor for `GpuMetrics`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List gpuMetricsDescriptor = $convert.base64Decode(
    'CgpHcHVNZXRyaWNzEhQKBWluZGV4GAEgASgFUgVpbmRleBIWCgZ2ZW5kb3IYAiABKAlSBnZlbm'
    'RvchISCgRuYW1lGAMgASgJUgRuYW1lEi8KE3V0aWxpemF0aW9uX3BlcmNlbnQYBCABKAFSEnV0'
    'aWxpemF0aW9uUGVyY2VudBIqChFtZW1vcnlfdXNlZF9ieXRlcxgFIAEoBFIPbWVtb3J5VXNlZE'
    'J5dGVzEiwKEm1lbW9yeV90b3RhbF9ieXRlcxgGIAEoBFIQbWVtb3J5VG90YWxCeXRlcxIvChN0'
    'ZW1wZXJhdHVyZV9jZWxzaXVzGAcgASgBUhJ0ZW1wZXJhdHVyZUNlbHNpdXM=');

@$core.Deprecated('Use containerEngineDescriptor instead')
const ContainerEngine$json = {
  '1': 'ContainerEngine',
  '2': [
    {'1': 'kind', '3': 1, '4': 1, '5': 9, '10': 'kind'},
    {'1': 'version', '3': 2, '4': 1, '5': 9, '10': 'version'},
    {
      '1': 'disk',
      '3': 3,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.EngineDiskUsage',
      '10': 'disk'
    },
    {
      '1': 'disk_measured_at',
      '3': 4,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Timestamp',
      '10': 'diskMeasuredAt'
    },
    {
      '1': 'containers',
      '3': 5,
      '4': 3,
      '5': 11,
      '6': '.protectorate.v1.ContainerMetrics',
      '10': 'containers'
    },
  ],
};

/// Descriptor for `ContainerEngine`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List containerEngineDescriptor = $convert.base64Decode(
    'Cg9Db250YWluZXJFbmdpbmUSEgoEa2luZBgBIAEoCVIEa2luZBIYCgd2ZXJzaW9uGAIgASgJUg'
    'd2ZXJzaW9uEjQKBGRpc2sYAyABKAsyIC5wcm90ZWN0b3JhdGUudjEuRW5naW5lRGlza1VzYWdl'
    'UgRkaXNrEkQKEGRpc2tfbWVhc3VyZWRfYXQYBCABKAsyGi5nb29nbGUucHJvdG9idWYuVGltZX'
    'N0YW1wUg5kaXNrTWVhc3VyZWRBdBJBCgpjb250YWluZXJzGAUgAygLMiEucHJvdGVjdG9yYXRl'
    'LnYxLkNvbnRhaW5lck1ldHJpY3NSCmNvbnRhaW5lcnM=');

@$core.Deprecated('Use engineDiskUsageDescriptor instead')
const EngineDiskUsage$json = {
  '1': 'EngineDiskUsage',
  '2': [
    {
      '1': 'images',
      '3': 1,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.DiskBucket',
      '10': 'images'
    },
    {
      '1': 'containers',
      '3': 2,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.DiskBucket',
      '10': 'containers'
    },
    {
      '1': 'volumes',
      '3': 3,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.DiskBucket',
      '10': 'volumes'
    },
    {
      '1': 'build_cache',
      '3': 4,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.DiskBucket',
      '10': 'buildCache'
    },
  ],
};

/// Descriptor for `EngineDiskUsage`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List engineDiskUsageDescriptor = $convert.base64Decode(
    'Cg9FbmdpbmVEaXNrVXNhZ2USMwoGaW1hZ2VzGAEgASgLMhsucHJvdGVjdG9yYXRlLnYxLkRpc2'
    'tCdWNrZXRSBmltYWdlcxI7Cgpjb250YWluZXJzGAIgASgLMhsucHJvdGVjdG9yYXRlLnYxLkRp'
    'c2tCdWNrZXRSCmNvbnRhaW5lcnMSNQoHdm9sdW1lcxgDIAEoCzIbLnByb3RlY3RvcmF0ZS52MS'
    '5EaXNrQnVja2V0Ugd2b2x1bWVzEjwKC2J1aWxkX2NhY2hlGAQgASgLMhsucHJvdGVjdG9yYXRl'
    'LnYxLkRpc2tCdWNrZXRSCmJ1aWxkQ2FjaGU=');

@$core.Deprecated('Use diskBucketDescriptor instead')
const DiskBucket$json = {
  '1': 'DiskBucket',
  '2': [
    {'1': 'count', '3': 1, '4': 1, '5': 3, '10': 'count'},
    {'1': 'size_bytes', '3': 2, '4': 1, '5': 3, '10': 'sizeBytes'},
    {
      '1': 'reclaimable_bytes',
      '3': 3,
      '4': 1,
      '5': 3,
      '10': 'reclaimableBytes'
    },
  ],
};

/// Descriptor for `DiskBucket`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List diskBucketDescriptor = $convert.base64Decode(
    'CgpEaXNrQnVja2V0EhQKBWNvdW50GAEgASgDUgVjb3VudBIdCgpzaXplX2J5dGVzGAIgASgDUg'
    'lzaXplQnl0ZXMSKwoRcmVjbGFpbWFibGVfYnl0ZXMYAyABKANSEHJlY2xhaW1hYmxlQnl0ZXM=');

@$core.Deprecated('Use containerMetricsDescriptor instead')
const ContainerMetrics$json = {
  '1': 'ContainerMetrics',
  '2': [
    {'1': 'id', '3': 1, '4': 1, '5': 9, '10': 'id'},
    {'1': 'name', '3': 2, '4': 1, '5': 9, '10': 'name'},
    {'1': 'image', '3': 3, '4': 1, '5': 9, '10': 'image'},
    {'1': 'state', '3': 4, '4': 1, '5': 9, '10': 'state'},
    {'1': 'cpu_percent', '3': 5, '4': 1, '5': 1, '10': 'cpuPercent'},
    {'1': 'memory_used_bytes', '3': 6, '4': 1, '5': 4, '10': 'memoryUsedBytes'},
    {
      '1': 'memory_limit_bytes',
      '3': 7,
      '4': 1,
      '5': 4,
      '10': 'memoryLimitBytes'
    },
    {'1': 'memory_percent', '3': 8, '4': 1, '5': 1, '10': 'memoryPercent'},
    {
      '1': 'writable_layer_bytes',
      '3': 9,
      '4': 1,
      '5': 3,
      '10': 'writableLayerBytes'
    },
    {'1': 'rootfs_bytes', '3': 10, '4': 1, '5': 3, '10': 'rootfsBytes'},
    {'1': 'stats_error', '3': 11, '4': 1, '5': 9, '10': 'statsError'},
    {
      '1': 'created_at',
      '3': 12,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Timestamp',
      '10': 'createdAt'
    },
  ],
};

/// Descriptor for `ContainerMetrics`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List containerMetricsDescriptor = $convert.base64Decode(
    'ChBDb250YWluZXJNZXRyaWNzEg4KAmlkGAEgASgJUgJpZBISCgRuYW1lGAIgASgJUgRuYW1lEh'
    'QKBWltYWdlGAMgASgJUgVpbWFnZRIUCgVzdGF0ZRgEIAEoCVIFc3RhdGUSHwoLY3B1X3BlcmNl'
    'bnQYBSABKAFSCmNwdVBlcmNlbnQSKgoRbWVtb3J5X3VzZWRfYnl0ZXMYBiABKARSD21lbW9yeV'
    'VzZWRCeXRlcxIsChJtZW1vcnlfbGltaXRfYnl0ZXMYByABKARSEG1lbW9yeUxpbWl0Qnl0ZXMS'
    'JQoObWVtb3J5X3BlcmNlbnQYCCABKAFSDW1lbW9yeVBlcmNlbnQSMAoUd3JpdGFibGVfbGF5ZX'
    'JfYnl0ZXMYCSABKANSEndyaXRhYmxlTGF5ZXJCeXRlcxIhCgxyb290ZnNfYnl0ZXMYCiABKANS'
    'C3Jvb3Rmc0J5dGVzEh8KC3N0YXRzX2Vycm9yGAsgASgJUgpzdGF0c0Vycm9yEjkKCmNyZWF0ZW'
    'RfYXQYDCABKAsyGi5nb29nbGUucHJvdG9idWYuVGltZXN0YW1wUgljcmVhdGVkQXQ=');

@$core.Deprecated('Use dirSizeDescriptor instead')
const DirSize$json = {
  '1': 'DirSize',
  '2': [
    {'1': 'path', '3': 1, '4': 1, '5': 9, '10': 'path'},
    {'1': 'size_bytes', '3': 2, '4': 1, '5': 3, '10': 'sizeBytes'},
    {
      '1': 'measured_at',
      '3': 3,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Timestamp',
      '10': 'measuredAt'
    },
    {
      '1': 'took',
      '3': 4,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Duration',
      '10': 'took'
    },
    {'1': 'complete', '3': 5, '4': 1, '5': 8, '10': 'complete'},
    {'1': 'error', '3': 6, '4': 1, '5': 9, '10': 'error'},
  ],
};

/// Descriptor for `DirSize`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List dirSizeDescriptor = $convert.base64Decode(
    'CgdEaXJTaXplEhIKBHBhdGgYASABKAlSBHBhdGgSHQoKc2l6ZV9ieXRlcxgCIAEoA1IJc2l6ZU'
    'J5dGVzEjsKC21lYXN1cmVkX2F0GAMgASgLMhouZ29vZ2xlLnByb3RvYnVmLlRpbWVzdGFtcFIK'
    'bWVhc3VyZWRBdBItCgR0b29rGAQgASgLMhkuZ29vZ2xlLnByb3RvYnVmLkR1cmF0aW9uUgR0b2'
    '9rEhoKCGNvbXBsZXRlGAUgASgIUghjb21wbGV0ZRIUCgVlcnJvchgGIAEoCVIFZXJyb3I=');

@$core.Deprecated('Use nodeAlertDescriptor instead')
const NodeAlert$json = {
  '1': 'NodeAlert',
  '2': [
    {
      '1': 'kind',
      '3': 1,
      '4': 1,
      '5': 14,
      '6': '.protectorate.v1.AlertKind',
      '10': 'kind'
    },
    {'1': 'subject', '3': 2, '4': 1, '5': 9, '10': 'subject'},
    {'1': 'message', '3': 3, '4': 1, '5': 9, '10': 'message'},
    {'1': 'value', '3': 4, '4': 1, '5': 1, '10': 'value'},
    {'1': 'threshold', '3': 5, '4': 1, '5': 1, '10': 'threshold'},
  ],
};

/// Descriptor for `NodeAlert`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List nodeAlertDescriptor = $convert.base64Decode(
    'CglOb2RlQWxlcnQSLgoEa2luZBgBIAEoDjIaLnByb3RlY3RvcmF0ZS52MS5BbGVydEtpbmRSBG'
    'tpbmQSGAoHc3ViamVjdBgCIAEoCVIHc3ViamVjdBIYCgdtZXNzYWdlGAMgASgJUgdtZXNzYWdl'
    'EhQKBXZhbHVlGAQgASgBUgV2YWx1ZRIcCgl0aHJlc2hvbGQYBSABKAFSCXRocmVzaG9sZA==');

@$core.Deprecated('Use nodeInfoDescriptor instead')
const NodeInfo$json = {
  '1': 'NodeInfo',
  '2': [
    {'1': 'name', '3': 1, '4': 1, '5': 9, '10': 'name'},
    {'1': 'address', '3': 2, '4': 1, '5': 9, '10': 'address'},
    {'1': 'os', '3': 3, '4': 1, '5': 9, '10': 'os'},
    {'1': 'arch', '3': 4, '4': 1, '5': 9, '10': 'arch'},
    {'1': 'tags', '3': 5, '4': 3, '5': 9, '10': 'tags'},
    {'1': 'ssh_alias', '3': 6, '4': 1, '5': 9, '10': 'sshAlias'},
    {'1': 'tier', '3': 7, '4': 1, '5': 9, '10': 'tier'},
    {'1': 'notes', '3': 8, '4': 1, '5': 9, '10': 'notes'},
    {'1': 'local', '3': 9, '4': 1, '5': 8, '10': 'local'},
    {'1': 'invited', '3': 10, '4': 1, '5': 8, '10': 'invited'},
    {'1': 'invite_key_id', '3': 11, '4': 1, '5': 4, '10': 'inviteKeyId'},
  ],
};

/// Descriptor for `NodeInfo`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List nodeInfoDescriptor = $convert.base64Decode(
    'CghOb2RlSW5mbxISCgRuYW1lGAEgASgJUgRuYW1lEhgKB2FkZHJlc3MYAiABKAlSB2FkZHJlc3'
    'MSDgoCb3MYAyABKAlSAm9zEhIKBGFyY2gYBCABKAlSBGFyY2gSEgoEdGFncxgFIAMoCVIEdGFn'
    'cxIbCglzc2hfYWxpYXMYBiABKAlSCHNzaEFsaWFzEhIKBHRpZXIYByABKAlSBHRpZXISFAoFbm'
    '90ZXMYCCABKAlSBW5vdGVzEhQKBWxvY2FsGAkgASgIUgVsb2NhbBIYCgdpbnZpdGVkGAogASgI'
    'UgdpbnZpdGVkEiIKDWludml0ZV9rZXlfaWQYCyABKARSC2ludml0ZUtleUlk');

@$core.Deprecated('Use nodeStatusDescriptor instead')
const NodeStatus$json = {
  '1': 'NodeStatus',
  '2': [
    {
      '1': 'node',
      '3': 1,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.NodeInfo',
      '10': 'node'
    },
    {
      '1': 'metrics',
      '3': 2,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.NodeMetrics',
      '10': 'metrics'
    },
    {
      '1': 'freshness',
      '3': 3,
      '4': 1,
      '5': 14,
      '6': '.protectorate.v1.NodeFreshness',
      '10': 'freshness'
    },
    {
      '1': 'fetched_at',
      '3': 4,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Timestamp',
      '10': 'fetchedAt'
    },
    {
      '1': 'age',
      '3': 5,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Duration',
      '10': 'age'
    },
    {'1': 'error', '3': 6, '4': 1, '5': 9, '10': 'error'},
    {
      '1': 'alerts',
      '3': 7,
      '4': 3,
      '5': 11,
      '6': '.protectorate.v1.NodeAlert',
      '10': 'alerts'
    },
    {
      '1': 'desk',
      '3': 8,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.DeskState',
      '10': 'desk'
    },
    {'1': 'desk_holder', '3': 9, '4': 1, '5': 9, '10': 'deskHolder'},
  ],
};

/// Descriptor for `NodeStatus`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List nodeStatusDescriptor = $convert.base64Decode(
    'CgpOb2RlU3RhdHVzEi0KBG5vZGUYASABKAsyGS5wcm90ZWN0b3JhdGUudjEuTm9kZUluZm9SBG'
    '5vZGUSNgoHbWV0cmljcxgCIAEoCzIcLnByb3RlY3RvcmF0ZS52MS5Ob2RlTWV0cmljc1IHbWV0'
    'cmljcxI8CglmcmVzaG5lc3MYAyABKA4yHi5wcm90ZWN0b3JhdGUudjEuTm9kZUZyZXNobmVzc1'
    'IJZnJlc2huZXNzEjkKCmZldGNoZWRfYXQYBCABKAsyGi5nb29nbGUucHJvdG9idWYuVGltZXN0'
    'YW1wUglmZXRjaGVkQXQSKwoDYWdlGAUgASgLMhkuZ29vZ2xlLnByb3RvYnVmLkR1cmF0aW9uUg'
    'NhZ2USFAoFZXJyb3IYBiABKAlSBWVycm9yEjIKBmFsZXJ0cxgHIAMoCzIaLnByb3RlY3RvcmF0'
    'ZS52MS5Ob2RlQWxlcnRSBmFsZXJ0cxIuCgRkZXNrGAggASgLMhoucHJvdGVjdG9yYXRlLnYxLk'
    'Rlc2tTdGF0ZVIEZGVzaxIfCgtkZXNrX2hvbGRlchgJIAEoCVIKZGVza0hvbGRlcg==');

@$core.Deprecated('Use listNodesRequestDescriptor instead')
const ListNodesRequest$json = {
  '1': 'ListNodesRequest',
};

/// Descriptor for `ListNodesRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List listNodesRequestDescriptor =
    $convert.base64Decode('ChBMaXN0Tm9kZXNSZXF1ZXN0');

@$core.Deprecated('Use listNodesResponseDescriptor instead')
const ListNodesResponse$json = {
  '1': 'ListNodesResponse',
  '2': [
    {
      '1': 'nodes',
      '3': 1,
      '4': 3,
      '5': 11,
      '6': '.protectorate.v1.NodeStatus',
      '10': 'nodes'
    },
  ],
};

/// Descriptor for `ListNodesResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List listNodesResponseDescriptor = $convert.base64Decode(
    'ChFMaXN0Tm9kZXNSZXNwb25zZRIxCgVub2RlcxgBIAMoCzIbLnByb3RlY3RvcmF0ZS52MS5Ob2'
    'RlU3RhdHVzUgVub2Rlcw==');

@$core.Deprecated('Use getNodeRequestDescriptor instead')
const GetNodeRequest$json = {
  '1': 'GetNodeRequest',
  '2': [
    {'1': 'name', '3': 1, '4': 1, '5': 9, '10': 'name'},
  ],
};

/// Descriptor for `GetNodeRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List getNodeRequestDescriptor =
    $convert.base64Decode('Cg5HZXROb2RlUmVxdWVzdBISCgRuYW1lGAEgASgJUgRuYW1l');

@$core.Deprecated('Use addNodeRequestDescriptor instead')
const AddNodeRequest$json = {
  '1': 'AddNodeRequest',
  '2': [
    {
      '1': 'node',
      '3': 1,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.NodeInfo',
      '10': 'node'
    },
    {'1': 'update', '3': 2, '4': 1, '5': 8, '10': 'update'},
  ],
};

/// Descriptor for `AddNodeRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List addNodeRequestDescriptor = $convert.base64Decode(
    'Cg5BZGROb2RlUmVxdWVzdBItCgRub2RlGAEgASgLMhkucHJvdGVjdG9yYXRlLnYxLk5vZGVJbm'
    'ZvUgRub2RlEhYKBnVwZGF0ZRgCIAEoCFIGdXBkYXRl');

@$core.Deprecated('Use removeNodeRequestDescriptor instead')
const RemoveNodeRequest$json = {
  '1': 'RemoveNodeRequest',
  '2': [
    {'1': 'name', '3': 1, '4': 1, '5': 9, '10': 'name'},
  ],
};

/// Descriptor for `RemoveNodeRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List removeNodeRequestDescriptor = $convert
    .base64Decode('ChFSZW1vdmVOb2RlUmVxdWVzdBISCgRuYW1lGAEgASgJUgRuYW1l');

@$core.Deprecated('Use removeNodeResponseDescriptor instead')
const RemoveNodeResponse$json = {
  '1': 'RemoveNodeResponse',
};

/// Descriptor for `RemoveNodeResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List removeNodeResponseDescriptor =
    $convert.base64Decode('ChJSZW1vdmVOb2RlUmVzcG9uc2U=');

const $core.Map<$core.String, $core.dynamic> NodeAgentServiceBase$json = {
  '1': 'NodeAgentService',
  '2': [
    {
      '1': 'GetMetrics',
      '2': '.protectorate.v1.GetMetricsRequest',
      '3': '.protectorate.v1.NodeMetrics',
      '4': {'34': 1},
    },
    {
      '1': 'WatchMetrics',
      '2': '.protectorate.v1.WatchMetricsRequest',
      '3': '.protectorate.v1.NodeMetrics',
      '4': {'34': 1},
      '6': true,
    },
    {
      '1': 'PrepareJob',
      '2': '.protectorate.v1.PrepareJobRequest',
      '3': '.protectorate.v1.PrepareJobResponse'
    },
    {
      '1': 'UploadJobData',
      '2': '.protectorate.v1.UploadJobDataRequest',
      '3': '.protectorate.v1.UploadJobDataResponse'
    },
    {
      '1': 'RunJob',
      '2': '.protectorate.v1.RunJobRequest',
      '3': '.protectorate.v1.JobEvent',
      '6': true
    },
    {
      '1': 'CancelJob',
      '2': '.protectorate.v1.NodeAgentServiceCancelJobRequest',
      '3': '.protectorate.v1.NodeAgentServiceCancelJobResponse'
    },
    {
      '1': 'PutFile',
      '2': '.protectorate.v1.PutFileRequest',
      '3': '.protectorate.v1.PutFileResponse'
    },
    {
      '1': 'GetFile',
      '2': '.protectorate.v1.GetFileRequest',
      '3': '.protectorate.v1.GetFileResponse',
      '6': true
    },
    {
      '1': 'DeskAction',
      '2': '.protectorate.v1.DeskActionRequest',
      '3': '.protectorate.v1.DeskActionResponse'
    },
    {
      '1': 'ApplyUpdate',
      '2': '.protectorate.v1.ApplyUpdateRequest',
      '3': '.protectorate.v1.ApplyUpdateResponse'
    },
    {
      '1': 'GetDeskState',
      '2': '.protectorate.v1.GetDeskStateRequest',
      '3': '.protectorate.v1.DeskState'
    },
  ],
  '3': {},
};

@$core.Deprecated('Use nodeAgentServiceDescriptor instead')
const $core.Map<$core.String, $core.Map<$core.String, $core.dynamic>>
    NodeAgentServiceBase$messageJson = {
  '.protectorate.v1.GetMetricsRequest': GetMetricsRequest$json,
  '.protectorate.v1.NodeMetrics': NodeMetrics$json,
  '.google.protobuf.Timestamp': $1.Timestamp$json,
  '.protectorate.v1.CpuMetrics': CpuMetrics$json,
  '.protectorate.v1.MemoryMetrics': MemoryMetrics$json,
  '.protectorate.v1.FilesystemMetrics': FilesystemMetrics$json,
  '.protectorate.v1.GpuMetrics': GpuMetrics$json,
  '.protectorate.v1.ContainerEngine': ContainerEngine$json,
  '.protectorate.v1.EngineDiskUsage': EngineDiskUsage$json,
  '.protectorate.v1.DiskBucket': DiskBucket$json,
  '.protectorate.v1.ContainerMetrics': ContainerMetrics$json,
  '.protectorate.v1.DirSize': DirSize$json,
  '.google.protobuf.Duration': $3.Duration$json,
  '.protectorate.v1.Sleeve': $2.Sleeve$json,
  '.protectorate.v1.SleeveLimits': $2.SleeveLimits$json,
  '.protectorate.v1.SleeveUsage': $2.SleeveUsage$json,
  '.protectorate.v1.ChatStatus': $5.ChatStatus$json,
  '.protectorate.v1.WatchMetricsRequest': WatchMetricsRequest$json,
  '.protectorate.v1.PrepareJobRequest': $4.PrepareJobRequest$json,
  '.protectorate.v1.PrepareJobResponse': $4.PrepareJobResponse$json,
  '.protectorate.v1.UploadJobDataRequest': $4.UploadJobDataRequest$json,
  '.protectorate.v1.UploadJobDataResponse': $4.UploadJobDataResponse$json,
  '.protectorate.v1.RunJobRequest': $4.RunJobRequest$json,
  '.protectorate.v1.RunJobRequest.EnvEntry': $4.RunJobRequest_EnvEntry$json,
  '.protectorate.v1.JobEvent': $4.JobEvent$json,
  '.protectorate.v1.JobStarted': $4.JobStarted$json,
  '.protectorate.v1.JobOutput': $4.JobOutput$json,
  '.protectorate.v1.JobArtifactChunk': $4.JobArtifactChunk$json,
  '.protectorate.v1.JobFinished': $4.JobFinished$json,
  '.protectorate.v1.NodeAgentServiceCancelJobRequest':
      $4.NodeAgentServiceCancelJobRequest$json,
  '.protectorate.v1.NodeAgentServiceCancelJobResponse':
      $4.NodeAgentServiceCancelJobResponse$json,
  '.protectorate.v1.PutFileRequest': PutFileRequest$json,
  '.protectorate.v1.FileChunk': FileChunk$json,
  '.protectorate.v1.FileTrailer': FileTrailer$json,
  '.protectorate.v1.PutFileResponse': PutFileResponse$json,
  '.protectorate.v1.GetFileRequest': GetFileRequest$json,
  '.protectorate.v1.GetFileResponse': GetFileResponse$json,
  '.protectorate.v1.FileHeader': FileHeader$json,
  '.protectorate.v1.DeskActionRequest': $0.DeskActionRequest$json,
  '.protectorate.v1.ScreenshotRequest': $0.ScreenshotRequest$json,
  '.protectorate.v1.DeskTarget': $0.DeskTarget$json,
  '.protectorate.v1.ListWindowsRequest': $0.ListWindowsRequest$json,
  '.protectorate.v1.ClickRequest': $0.ClickRequest$json,
  '.protectorate.v1.MoveRequest': $0.MoveRequest$json,
  '.protectorate.v1.TypeRequest': $0.TypeRequest$json,
  '.protectorate.v1.KeyRequest': $0.KeyRequest$json,
  '.protectorate.v1.RunRequest': $0.RunRequest$json,
  '.protectorate.v1.DeskActionResponse': $0.DeskActionResponse$json,
  '.protectorate.v1.ScreenshotResponse': $0.ScreenshotResponse$json,
  '.protectorate.v1.ListWindowsResponse': $0.ListWindowsResponse$json,
  '.protectorate.v1.DeskWindow': $0.DeskWindow$json,
  '.protectorate.v1.InputResponse': $0.InputResponse$json,
  '.protectorate.v1.RunResponse': $0.RunResponse$json,
  '.protectorate.v1.ApplyUpdateRequest': ApplyUpdateRequest$json,
  '.protectorate.v1.ApplyUpdateResponse': ApplyUpdateResponse$json,
  '.protectorate.v1.GetDeskStateRequest': GetDeskStateRequest$json,
  '.protectorate.v1.DeskPolicy': $0.DeskPolicy$json,
  '.protectorate.v1.DeskState': $0.DeskState$json,
};

/// Descriptor for `NodeAgentService`. Decode as a `google.protobuf.ServiceDescriptorProto`.
final $typed_data.Uint8List nodeAgentServiceDescriptor = $convert.base64Decode(
    'ChBOb2RlQWdlbnRTZXJ2aWNlElMKCkdldE1ldHJpY3MSIi5wcm90ZWN0b3JhdGUudjEuR2V0TW'
    'V0cmljc1JlcXVlc3QaHC5wcm90ZWN0b3JhdGUudjEuTm9kZU1ldHJpY3MiA5ACARJZCgxXYXRj'
    'aE1ldHJpY3MSJC5wcm90ZWN0b3JhdGUudjEuV2F0Y2hNZXRyaWNzUmVxdWVzdBocLnByb3RlY3'
    'RvcmF0ZS52MS5Ob2RlTWV0cmljcyIDkAIBMAESVQoKUHJlcGFyZUpvYhIiLnByb3RlY3RvcmF0'
    'ZS52MS5QcmVwYXJlSm9iUmVxdWVzdBojLnByb3RlY3RvcmF0ZS52MS5QcmVwYXJlSm9iUmVzcG'
    '9uc2USXgoNVXBsb2FkSm9iRGF0YRIlLnByb3RlY3RvcmF0ZS52MS5VcGxvYWRKb2JEYXRhUmVx'
    'dWVzdBomLnByb3RlY3RvcmF0ZS52MS5VcGxvYWRKb2JEYXRhUmVzcG9uc2USRQoGUnVuSm9iEh'
    '4ucHJvdGVjdG9yYXRlLnYxLlJ1bkpvYlJlcXVlc3QaGS5wcm90ZWN0b3JhdGUudjEuSm9iRXZl'
    'bnQwARJyCglDYW5jZWxKb2ISMS5wcm90ZWN0b3JhdGUudjEuTm9kZUFnZW50U2VydmljZUNhbm'
    'NlbEpvYlJlcXVlc3QaMi5wcm90ZWN0b3JhdGUudjEuTm9kZUFnZW50U2VydmljZUNhbmNlbEpv'
    'YlJlc3BvbnNlEkwKB1B1dEZpbGUSHy5wcm90ZWN0b3JhdGUudjEuUHV0RmlsZVJlcXVlc3QaIC'
    '5wcm90ZWN0b3JhdGUudjEuUHV0RmlsZVJlc3BvbnNlEk4KB0dldEZpbGUSHy5wcm90ZWN0b3Jh'
    'dGUudjEuR2V0RmlsZVJlcXVlc3QaIC5wcm90ZWN0b3JhdGUudjEuR2V0RmlsZVJlc3BvbnNlMA'
    'ESVQoKRGVza0FjdGlvbhIiLnByb3RlY3RvcmF0ZS52MS5EZXNrQWN0aW9uUmVxdWVzdBojLnBy'
    'b3RlY3RvcmF0ZS52MS5EZXNrQWN0aW9uUmVzcG9uc2USWAoLQXBwbHlVcGRhdGUSIy5wcm90ZW'
    'N0b3JhdGUudjEuQXBwbHlVcGRhdGVSZXF1ZXN0GiQucHJvdGVjdG9yYXRlLnYxLkFwcGx5VXBk'
    'YXRlUmVzcG9uc2USUAoMR2V0RGVza1N0YXRlEiQucHJvdGVjdG9yYXRlLnYxLkdldERlc2tTdG'
    'F0ZVJlcXVlc3QaGi5wcm90ZWN0b3JhdGUudjEuRGVza1N0YXRlGgSQtRgB');

const $core.Map<$core.String, $core.dynamic> NodeServiceBase$json = {
  '1': 'NodeService',
  '2': [
    {
      '1': 'ListNodes',
      '2': '.protectorate.v1.ListNodesRequest',
      '3': '.protectorate.v1.ListNodesResponse',
      '4': {'34': 1},
    },
    {
      '1': 'GetNode',
      '2': '.protectorate.v1.GetNodeRequest',
      '3': '.protectorate.v1.NodeStatus',
      '4': {'34': 1},
    },
    {
      '1': 'AddNode',
      '2': '.protectorate.v1.AddNodeRequest',
      '3': '.protectorate.v1.NodeInfo',
      '4': {}
    },
    {
      '1': 'RemoveNode',
      '2': '.protectorate.v1.RemoveNodeRequest',
      '3': '.protectorate.v1.RemoveNodeResponse'
    },
    {
      '1': 'PutFile',
      '2': '.protectorate.v1.PutFileRequest',
      '3': '.protectorate.v1.PutFileResponse'
    },
    {
      '1': 'GetFile',
      '2': '.protectorate.v1.GetFileRequest',
      '3': '.protectorate.v1.GetFileResponse',
      '6': true
    },
    {
      '1': 'ApplyUpdate',
      '2': '.protectorate.v1.ApplyUpdateRequest',
      '3': '.protectorate.v1.ApplyUpdateResponse',
      '4': {}
    },
    {
      '1': 'UpdateNode',
      '2': '.protectorate.v1.UpdateNodeRequest',
      '3': '.protectorate.v1.UpdateNodeProgress',
      '4': {},
      '6': true
    },
    {
      '1': 'SetDeskPolicy',
      '2': '.protectorate.v1.SetDeskPolicyRequest',
      '3': '.protectorate.v1.DeskPolicyStatus',
      '4': {}
    },
    {
      '1': 'GetDeskPolicy',
      '2': '.protectorate.v1.GetDeskPolicyRequest',
      '3': '.protectorate.v1.DeskPolicyStatus'
    },
  ],
};

@$core.Deprecated('Use nodeServiceDescriptor instead')
const $core.Map<$core.String, $core.Map<$core.String, $core.dynamic>>
    NodeServiceBase$messageJson = {
  '.protectorate.v1.ListNodesRequest': ListNodesRequest$json,
  '.protectorate.v1.ListNodesResponse': ListNodesResponse$json,
  '.protectorate.v1.NodeStatus': NodeStatus$json,
  '.protectorate.v1.NodeInfo': NodeInfo$json,
  '.protectorate.v1.NodeMetrics': NodeMetrics$json,
  '.google.protobuf.Timestamp': $1.Timestamp$json,
  '.protectorate.v1.CpuMetrics': CpuMetrics$json,
  '.protectorate.v1.MemoryMetrics': MemoryMetrics$json,
  '.protectorate.v1.FilesystemMetrics': FilesystemMetrics$json,
  '.protectorate.v1.GpuMetrics': GpuMetrics$json,
  '.protectorate.v1.ContainerEngine': ContainerEngine$json,
  '.protectorate.v1.EngineDiskUsage': EngineDiskUsage$json,
  '.protectorate.v1.DiskBucket': DiskBucket$json,
  '.protectorate.v1.ContainerMetrics': ContainerMetrics$json,
  '.protectorate.v1.DirSize': DirSize$json,
  '.google.protobuf.Duration': $3.Duration$json,
  '.protectorate.v1.Sleeve': $2.Sleeve$json,
  '.protectorate.v1.SleeveLimits': $2.SleeveLimits$json,
  '.protectorate.v1.SleeveUsage': $2.SleeveUsage$json,
  '.protectorate.v1.ChatStatus': $5.ChatStatus$json,
  '.protectorate.v1.NodeAlert': NodeAlert$json,
  '.protectorate.v1.DeskState': $0.DeskState$json,
  '.protectorate.v1.DeskPolicy': $0.DeskPolicy$json,
  '.protectorate.v1.GetNodeRequest': GetNodeRequest$json,
  '.protectorate.v1.AddNodeRequest': AddNodeRequest$json,
  '.protectorate.v1.RemoveNodeRequest': RemoveNodeRequest$json,
  '.protectorate.v1.RemoveNodeResponse': RemoveNodeResponse$json,
  '.protectorate.v1.PutFileRequest': PutFileRequest$json,
  '.protectorate.v1.FileChunk': FileChunk$json,
  '.protectorate.v1.FileTrailer': FileTrailer$json,
  '.protectorate.v1.PutFileResponse': PutFileResponse$json,
  '.protectorate.v1.GetFileRequest': GetFileRequest$json,
  '.protectorate.v1.GetFileResponse': GetFileResponse$json,
  '.protectorate.v1.FileHeader': FileHeader$json,
  '.protectorate.v1.ApplyUpdateRequest': ApplyUpdateRequest$json,
  '.protectorate.v1.ApplyUpdateResponse': ApplyUpdateResponse$json,
  '.protectorate.v1.UpdateNodeRequest': UpdateNodeRequest$json,
  '.protectorate.v1.UpdateNodeProgress': UpdateNodeProgress$json,
  '.protectorate.v1.SetDeskPolicyRequest': SetDeskPolicyRequest$json,
  '.protectorate.v1.DeskPolicyStatus': DeskPolicyStatus$json,
  '.protectorate.v1.GetDeskPolicyRequest': GetDeskPolicyRequest$json,
};

/// Descriptor for `NodeService`. Decode as a `google.protobuf.ServiceDescriptorProto`.
final $typed_data.Uint8List nodeServiceDescriptor = $convert.base64Decode(
    'CgtOb2RlU2VydmljZRJXCglMaXN0Tm9kZXMSIS5wcm90ZWN0b3JhdGUudjEuTGlzdE5vZGVzUm'
    'VxdWVzdBoiLnByb3RlY3RvcmF0ZS52MS5MaXN0Tm9kZXNSZXNwb25zZSIDkAIBEkwKB0dldE5v'
    'ZGUSHy5wcm90ZWN0b3JhdGUudjEuR2V0Tm9kZVJlcXVlc3QaGy5wcm90ZWN0b3JhdGUudjEuTm'
    '9kZVN0YXR1cyIDkAIBEksKB0FkZE5vZGUSHy5wcm90ZWN0b3JhdGUudjEuQWRkTm9kZVJlcXVl'
    'c3QaGS5wcm90ZWN0b3JhdGUudjEuTm9kZUluZm8iBKC1GAESVQoKUmVtb3ZlTm9kZRIiLnByb3'
    'RlY3RvcmF0ZS52MS5SZW1vdmVOb2RlUmVxdWVzdBojLnByb3RlY3RvcmF0ZS52MS5SZW1vdmVO'
    'b2RlUmVzcG9uc2USTAoHUHV0RmlsZRIfLnByb3RlY3RvcmF0ZS52MS5QdXRGaWxlUmVxdWVzdB'
    'ogLnByb3RlY3RvcmF0ZS52MS5QdXRGaWxlUmVzcG9uc2USTgoHR2V0RmlsZRIfLnByb3RlY3Rv'
    'cmF0ZS52MS5HZXRGaWxlUmVxdWVzdBogLnByb3RlY3RvcmF0ZS52MS5HZXRGaWxlUmVzcG9uc2'
    'UwARJeCgtBcHBseVVwZGF0ZRIjLnByb3RlY3RvcmF0ZS52MS5BcHBseVVwZGF0ZVJlcXVlc3Qa'
    'JC5wcm90ZWN0b3JhdGUudjEuQXBwbHlVcGRhdGVSZXNwb25zZSIEoLUYARJdCgpVcGRhdGVOb2'
    'RlEiIucHJvdGVjdG9yYXRlLnYxLlVwZGF0ZU5vZGVSZXF1ZXN0GiMucHJvdGVjdG9yYXRlLnYx'
    'LlVwZGF0ZU5vZGVQcm9ncmVzcyIEoLUYATABEl8KDVNldERlc2tQb2xpY3kSJS5wcm90ZWN0b3'
    'JhdGUudjEuU2V0RGVza1BvbGljeVJlcXVlc3QaIS5wcm90ZWN0b3JhdGUudjEuRGVza1BvbGlj'
    'eVN0YXR1cyIEoLUYARJZCg1HZXREZXNrUG9saWN5EiUucHJvdGVjdG9yYXRlLnYxLkdldERlc2'
    'tQb2xpY3lSZXF1ZXN0GiEucHJvdGVjdG9yYXRlLnYxLkRlc2tQb2xpY3lTdGF0dXM=');
