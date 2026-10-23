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
    as $0;

import 'chat.pbjson.dart' as $4;
import 'job.pbjson.dart' as $2;
import 'sleeve.pbjson.dart' as $1;

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
  ],
  '4': [
    {'1': 4, '2': 4},
  ],
};

/// Descriptor for `AlertKind`. Decode as a `google.protobuf.EnumDescriptorProto`.
final $typed_data.Uint8List alertKindDescriptor = $convert.base64Decode(
    'CglBbGVydEtpbmQSGgoWQUxFUlRfS0lORF9VTlNQRUNJRklFRBAAEhsKF0FMRVJUX0tJTkRfRE'
    'lTS19QRVJDRU5UEAESGwoXQUxFUlRfS0lORF9NRU1fUFJFU1NVUkUQAhIaChZBTEVSVF9LSU5E'
    'X1VOUkVBQ0hBQkxFEAMiBAgEEAQ=');

@$core.Deprecated('Use removeJobsDescriptor instead')
const RemoveJobs$json = {
  '1': 'RemoveJobs',
  '2': [
    {'1': 'REMOVE_JOBS_UNSPECIFIED', '2': 0},
    {'1': 'REMOVE_JOBS_WAIT', '2': 1},
    {'1': 'REMOVE_JOBS_CANCEL', '2': 2},
  ],
};

/// Descriptor for `RemoveJobs`. Decode as a `google.protobuf.EnumDescriptorProto`.
final $typed_data.Uint8List removeJobsDescriptor = $convert.base64Decode(
    'CgpSZW1vdmVKb2JzEhsKF1JFTU9WRV9KT0JTX1VOU1BFQ0lGSUVEEAASFAoQUkVNT1ZFX0pPQl'
    'NfV0FJVBABEhYKElJFTU9WRV9KT0JTX0NBTkNFTBAC');

@$core.Deprecated('Use removePhaseDescriptor instead')
const RemovePhase$json = {
  '1': 'RemovePhase',
  '2': [
    {'1': 'REMOVE_PHASE_UNSPECIFIED', '2': 0},
    {'1': 'REMOVE_PHASE_DRAINING', '2': 1},
    {'1': 'REMOVE_PHASE_UNINSTALLING', '2': 2},
    {'1': 'REMOVE_PHASE_REVOKING', '2': 3},
    {'1': 'REMOVE_PHASE_REMOVED', '2': 4},
  ],
};

/// Descriptor for `RemovePhase`. Decode as a `google.protobuf.EnumDescriptorProto`.
final $typed_data.Uint8List removePhaseDescriptor = $convert.base64Decode(
    'CgtSZW1vdmVQaGFzZRIcChhSRU1PVkVfUEhBU0VfVU5TUEVDSUZJRUQQABIZChVSRU1PVkVfUE'
    'hBU0VfRFJBSU5JTkcQARIdChlSRU1PVkVfUEhBU0VfVU5JTlNUQUxMSU5HEAISGQoVUkVNT1ZF'
    'X1BIQVNFX1JFVk9LSU5HEAMSGAoUUkVNT1ZFX1BIQVNFX1JFTU9WRUQQBA==');

@$core.Deprecated('Use installPlatformDescriptor instead')
const InstallPlatform$json = {
  '1': 'InstallPlatform',
  '2': [
    {'1': 'INSTALL_PLATFORM_UNSPECIFIED', '2': 0},
    {'1': 'INSTALL_PLATFORM_WINDOWS', '2': 1},
    {'1': 'INSTALL_PLATFORM_LINUX', '2': 2},
    {'1': 'INSTALL_PLATFORM_TERMUX', '2': 3},
  ],
};

/// Descriptor for `InstallPlatform`. Decode as a `google.protobuf.EnumDescriptorProto`.
final $typed_data.Uint8List installPlatformDescriptor = $convert.base64Decode(
    'Cg9JbnN0YWxsUGxhdGZvcm0SIAocSU5TVEFMTF9QTEFURk9STV9VTlNQRUNJRklFRBAAEhwKGE'
    'lOU1RBTExfUExBVEZPUk1fV0lORE9XUxABEhoKFklOU1RBTExfUExBVEZPUk1fTElOVVgQAhIb'
    'ChdJTlNUQUxMX1BMQVRGT1JNX1RFUk1VWBAD');

@$core.Deprecated('Use nodeSetupStageDescriptor instead')
const NodeSetupStage$json = {
  '1': 'NodeSetupStage',
  '2': [
    {'1': 'NODE_SETUP_STAGE_UNSPECIFIED', '2': 0},
    {'1': 'NODE_SETUP_STAGE_INVITED', '2': 1},
    {'1': 'NODE_SETUP_STAGE_JOINED', '2': 2},
    {'1': 'NODE_SETUP_STAGE_READY', '2': 4},
    {'1': 'NODE_SETUP_STAGE_ATTENTION', '2': 5},
  ],
  '4': [
    {'1': 3, '2': 3},
  ],
};

/// Descriptor for `NodeSetupStage`. Decode as a `google.protobuf.EnumDescriptorProto`.
final $typed_data.Uint8List nodeSetupStageDescriptor = $convert.base64Decode(
    'Cg5Ob2RlU2V0dXBTdGFnZRIgChxOT0RFX1NFVFVQX1NUQUdFX1VOU1BFQ0lGSUVEEAASHAoYTk'
    '9ERV9TRVRVUF9TVEFHRV9JTlZJVEVEEAESGwoXTk9ERV9TRVRVUF9TVEFHRV9KT0lORUQQAhIa'
    'ChZOT0RFX1NFVFVQX1NUQUdFX1JFQURZEAQSHgoaTk9ERV9TRVRVUF9TVEFHRV9BVFRFTlRJT0'
    '4QBSIECAMQAw==');

@$core.Deprecated('Use setNodeTierRequestDescriptor instead')
const SetNodeTierRequest$json = {
  '1': 'SetNodeTierRequest',
  '2': [
    {'1': 'name', '3': 1, '4': 1, '5': 9, '10': 'name'},
    {'1': 'tier', '3': 2, '4': 1, '5': 9, '10': 'tier'},
  ],
};

/// Descriptor for `SetNodeTierRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List setNodeTierRequestDescriptor = $convert.base64Decode(
    'ChJTZXROb2RlVGllclJlcXVlc3QSEgoEbmFtZRgBIAEoCVIEbmFtZRISCgR0aWVyGAIgASgJUg'
    'R0aWVy');

@$core.Deprecated('Use updateNodeRequestDescriptor instead')
const UpdateNodeRequest$json = {
  '1': 'UpdateNodeRequest',
  '2': [
    {'1': 'name', '3': 1, '4': 1, '5': 9, '10': 'name'},
    {'1': 'force', '3': 3, '4': 1, '5': 8, '10': 'force'},
  ],
  '9': [
    {'1': 2, '2': 3},
  ],
};

/// Descriptor for `UpdateNodeRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List updateNodeRequestDescriptor = $convert.base64Decode(
    'ChFVcGRhdGVOb2RlUmVxdWVzdBISCgRuYW1lGAEgASgJUgRuYW1lEhQKBWZvcmNlGAMgASgIUg'
    'Vmb3JjZUoECAIQAw==');

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
    {'1': 'path', '3': 3, '4': 1, '5': 9, '10': 'path'},
    {'1': 'size', '3': 4, '4': 1, '5': 4, '10': 'size'},
    {'1': 'sha256', '3': 5, '4': 1, '5': 9, '10': 'sha256'},
    {'1': 'expected_version', '3': 6, '4': 1, '5': 9, '10': 'expectedVersion'},
  ],
  '9': [
    {'1': 2, '2': 3},
  ],
};

/// Descriptor for `ApplyUpdateRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List applyUpdateRequestDescriptor = $convert.base64Decode(
    'ChJBcHBseVVwZGF0ZVJlcXVlc3QSEgoEbmFtZRgBIAEoCVIEbmFtZRISCgRwYXRoGAMgASgJUg'
    'RwYXRoEhIKBHNpemUYBCABKARSBHNpemUSFgoGc2hhMjU2GAUgASgJUgZzaGEyNTYSKQoQZXhw'
    'ZWN0ZWRfdmVyc2lvbhgGIAEoCVIPZXhwZWN0ZWRWZXJzaW9uSgQIAhAD');

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
  '9': [
    {'1': 2, '2': 3},
  ],
};

/// Descriptor for `PutFileRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List putFileRequestDescriptor = $convert.base64Decode(
    'Cg5QdXRGaWxlUmVxdWVzdBISCgRuYW1lGAEgASgJUgRuYW1lEh8KC3RyYW5zZmVyX2lkGAMgAS'
    'gJUgp0cmFuc2ZlcklkEhIKBHBhdGgYBCABKAlSBHBhdGgSLQoEa2luZBgFIAEoDjIZLnByb3Rl'
    'Y3RvcmF0ZS52MS5GaWxlS2luZFIEa2luZBISCgRtb2RlGAYgASgNUgRtb2RlEhwKCW92ZXJ3cm'
    'l0ZRgHIAEoCFIJb3ZlcndyaXRlEhYKBm9mZnNldBgIIAEoBFIGb2Zmc2V0EjAKBWNodW5rGAkg'
    'ASgLMhoucHJvdGVjdG9yYXRlLnYxLkZpbGVDaHVua1IFY2h1bmsSEgoEbGFzdBgKIAEoCFIEbG'
    'FzdBI2Cgd0cmFpbGVyGAsgASgLMhwucHJvdGVjdG9yYXRlLnYxLkZpbGVUcmFpbGVyUgd0cmFp'
    'bGVySgQIAhAD');

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
    {'1': 'path', '3': 3, '4': 1, '5': 9, '10': 'path'},
    {'1': 'no_compress', '3': 4, '4': 1, '5': 8, '10': 'noCompress'},
  ],
  '9': [
    {'1': 2, '2': 3},
  ],
};

/// Descriptor for `GetFileRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List getFileRequestDescriptor = $convert.base64Decode(
    'Cg5HZXRGaWxlUmVxdWVzdBISCgRuYW1lGAEgASgJUgRuYW1lEhIKBHBhdGgYAyABKAlSBHBhdG'
    'gSHwoLbm9fY29tcHJlc3MYBCABKAhSCm5vQ29tcHJlc3NKBAgCEAM=');

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
    {
      '1': 'npus',
      '3': 16,
      '4': 3,
      '5': 11,
      '6': '.protectorate.v1.AcceleratorMetrics',
      '10': 'npus'
    },
    {
      '1': 'jobs',
      '3': 17,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.JobIsolation',
      '10': 'jobs'
    },
    {
      '1': 'update',
      '3': 18,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.NodeUpdateState',
      '10': 'update'
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
    'EKB3NsZWV2ZXMYDyADKAsyFy5wcm90ZWN0b3JhdGUudjEuU2xlZXZlUgdzbGVldmVzEjcKBG5w'
    'dXMYECADKAsyIy5wcm90ZWN0b3JhdGUudjEuQWNjZWxlcmF0b3JNZXRyaWNzUgRucHVzEjEKBG'
    'pvYnMYESABKAsyHS5wcm90ZWN0b3JhdGUudjEuSm9iSXNvbGF0aW9uUgRqb2JzEjgKBnVwZGF0'
    'ZRgSIAEoCzIgLnByb3RlY3RvcmF0ZS52MS5Ob2RlVXBkYXRlU3RhdGVSBnVwZGF0ZQ==');

@$core.Deprecated('Use nodeUpdateStateDescriptor instead')
const NodeUpdateState$json = {
  '1': 'NodeUpdateState',
  '2': [
    {'1': 'phase', '3': 1, '4': 1, '5': 9, '10': 'phase'},
    {'1': 'previous_version', '3': 2, '4': 1, '5': 9, '10': 'previousVersion'},
    {'1': 'version', '3': 3, '4': 1, '5': 9, '10': 'version'},
    {'1': 'attempts', '3': 4, '4': 1, '5': 13, '10': 'attempts'},
  ],
};

/// Descriptor for `NodeUpdateState`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List nodeUpdateStateDescriptor = $convert.base64Decode(
    'Cg9Ob2RlVXBkYXRlU3RhdGUSFAoFcGhhc2UYASABKAlSBXBoYXNlEikKEHByZXZpb3VzX3Zlcn'
    'Npb24YAiABKAlSD3ByZXZpb3VzVmVyc2lvbhIYCgd2ZXJzaW9uGAMgASgJUgd2ZXJzaW9uEhoK'
    'CGF0dGVtcHRzGAQgASgNUghhdHRlbXB0cw==');

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
    {'1': 'primary', '3': 8, '4': 1, '5': 8, '10': 'primary'},
  ],
};

/// Descriptor for `FilesystemMetrics`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List filesystemMetricsDescriptor = $convert.base64Decode(
    'ChFGaWxlc3lzdGVtTWV0cmljcxIUCgVtb3VudBgBIAEoCVIFbW91bnQSFgoGZGV2aWNlGAIgAS'
    'gJUgZkZXZpY2USFgoGZnN0eXBlGAMgASgJUgZmc3R5cGUSHwoLdG90YWxfYnl0ZXMYBCABKARS'
    'CnRvdGFsQnl0ZXMSHQoKdXNlZF9ieXRlcxgFIAEoBFIJdXNlZEJ5dGVzEh0KCmZyZWVfYnl0ZX'
    'MYBiABKARSCWZyZWVCeXRlcxIhCgx1c2VkX3BlcmNlbnQYByABKAFSC3VzZWRQZXJjZW50EhgK'
    'B3ByaW1hcnkYCCABKAhSB3ByaW1hcnk=');

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
    {
      '1': 'memory_used_bytes',
      '3': 5,
      '4': 1,
      '5': 4,
      '9': 0,
      '10': 'memoryUsedBytes',
      '17': true
    },
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
    {'1': 'integrated', '3': 8, '4': 1, '5': 8, '10': 'integrated'},
    {'1': 'driver', '3': 9, '4': 1, '5': 9, '10': 'driver'},
    {'1': 'bus_id', '3': 10, '4': 1, '5': 9, '10': 'busId'},
  ],
  '8': [
    {'1': '_memory_used_bytes'},
  ],
};

/// Descriptor for `GpuMetrics`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List gpuMetricsDescriptor = $convert.base64Decode(
    'CgpHcHVNZXRyaWNzEhQKBWluZGV4GAEgASgFUgVpbmRleBIWCgZ2ZW5kb3IYAiABKAlSBnZlbm'
    'RvchISCgRuYW1lGAMgASgJUgRuYW1lEi8KE3V0aWxpemF0aW9uX3BlcmNlbnQYBCABKAFSEnV0'
    'aWxpemF0aW9uUGVyY2VudBIvChFtZW1vcnlfdXNlZF9ieXRlcxgFIAEoBEgAUg9tZW1vcnlVc2'
    'VkQnl0ZXOIAQESLAoSbWVtb3J5X3RvdGFsX2J5dGVzGAYgASgEUhBtZW1vcnlUb3RhbEJ5dGVz'
    'Ei8KE3RlbXBlcmF0dXJlX2NlbHNpdXMYByABKAFSEnRlbXBlcmF0dXJlQ2Vsc2l1cxIeCgppbn'
    'RlZ3JhdGVkGAggASgIUgppbnRlZ3JhdGVkEhYKBmRyaXZlchgJIAEoCVIGZHJpdmVyEhUKBmJ1'
    'c19pZBgKIAEoCVIFYnVzSWRCFAoSX21lbW9yeV91c2VkX2J5dGVz');

@$core.Deprecated('Use acceleratorMetricsDescriptor instead')
const AcceleratorMetrics$json = {
  '1': 'AcceleratorMetrics',
  '2': [
    {'1': 'kind', '3': 1, '4': 1, '5': 9, '10': 'kind'},
    {'1': 'vendor', '3': 2, '4': 1, '5': 9, '10': 'vendor'},
    {'1': 'name', '3': 3, '4': 1, '5': 9, '10': 'name'},
    {'1': 'driver', '3': 4, '4': 1, '5': 9, '10': 'driver'},
    {
      '1': 'utilization_percent',
      '3': 5,
      '4': 1,
      '5': 1,
      '10': 'utilizationPercent'
    },
    {'1': 'frequency_hz', '3': 6, '4': 1, '5': 4, '10': 'frequencyHz'},
    {'1': 'bus_id', '3': 7, '4': 1, '5': 9, '10': 'busId'},
  ],
};

/// Descriptor for `AcceleratorMetrics`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List acceleratorMetricsDescriptor = $convert.base64Decode(
    'ChJBY2NlbGVyYXRvck1ldHJpY3MSEgoEa2luZBgBIAEoCVIEa2luZBIWCgZ2ZW5kb3IYAiABKA'
    'lSBnZlbmRvchISCgRuYW1lGAMgASgJUgRuYW1lEhYKBmRyaXZlchgEIAEoCVIGZHJpdmVyEi8K'
    'E3V0aWxpemF0aW9uX3BlcmNlbnQYBSABKAFSEnV0aWxpemF0aW9uUGVyY2VudBIhCgxmcmVxdW'
    'VuY3lfaHoYBiABKARSC2ZyZXF1ZW5jeUh6EhUKBmJ1c19pZBgHIAEoCVIFYnVzSWQ=');

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
  ],
  '9': [
    {'1': 11, '2': 12},
  ],
};

/// Descriptor for `NodeInfo`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List nodeInfoDescriptor = $convert.base64Decode(
    'CghOb2RlSW5mbxISCgRuYW1lGAEgASgJUgRuYW1lEhgKB2FkZHJlc3MYAiABKAlSB2FkZHJlc3'
    'MSDgoCb3MYAyABKAlSAm9zEhIKBGFyY2gYBCABKAlSBGFyY2gSEgoEdGFncxgFIAMoCVIEdGFn'
    'cxIbCglzc2hfYWxpYXMYBiABKAlSCHNzaEFsaWFzEhIKBHRpZXIYByABKAlSBHRpZXISFAoFbm'
    '90ZXMYCCABKAlSBW5vdGVzEhQKBWxvY2FsGAkgASgIUgVsb2NhbBIYCgdpbnZpdGVkGAogASgI'
    'UgdpbnZpdGVkSgQICxAM');

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
    {'1': 'draining', '3': 11, '4': 1, '5': 8, '10': 'draining'},
    {
      '1': 'busy_gpus',
      '3': 12,
      '4': 3,
      '5': 11,
      '6': '.protectorate.v1.GpuBusy',
      '10': 'busyGpus'
    },
  ],
  '9': [
    {'1': 8, '2': 9},
    {'1': 9, '2': 10},
    {'1': 10, '2': 11},
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
    'ZS52MS5Ob2RlQWxlcnRSBmFsZXJ0cxIaCghkcmFpbmluZxgLIAEoCFIIZHJhaW5pbmcSNQoJYn'
    'VzeV9ncHVzGAwgAygLMhgucHJvdGVjdG9yYXRlLnYxLkdwdUJ1c3lSCGJ1c3lHcHVzSgQICBAJ'
    'SgQICRAKSgQIChAL');

@$core.Deprecated('Use gpuBusyDescriptor instead')
const GpuBusy$json = {
  '1': 'GpuBusy',
  '2': [
    {'1': 'index', '3': 1, '4': 1, '5': 5, '10': 'index'},
    {'1': 'reason', '3': 2, '4': 1, '5': 9, '10': 'reason'},
  ],
};

/// Descriptor for `GpuBusy`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List gpuBusyDescriptor = $convert.base64Decode(
    'CgdHcHVCdXN5EhQKBWluZGV4GAEgASgFUgVpbmRleBIWCgZyZWFzb24YAiABKAlSBnJlYXNvbg'
    '==');

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
    {
      '1': 'jobs',
      '3': 2,
      '4': 1,
      '5': 14,
      '6': '.protectorate.v1.RemoveJobs',
      '10': 'jobs'
    },
    {'1': 'uninstall', '3': 3, '4': 1, '5': 8, '10': 'uninstall'},
    {
      '1': 'wait_timeout',
      '3': 4,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Duration',
      '10': 'waitTimeout'
    },
  ],
};

/// Descriptor for `RemoveNodeRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List removeNodeRequestDescriptor = $convert.base64Decode(
    'ChFSZW1vdmVOb2RlUmVxdWVzdBISCgRuYW1lGAEgASgJUgRuYW1lEi8KBGpvYnMYAiABKA4yGy'
    '5wcm90ZWN0b3JhdGUudjEuUmVtb3ZlSm9ic1IEam9icxIcCgl1bmluc3RhbGwYAyABKAhSCXVu'
    'aW5zdGFsbBI8Cgx3YWl0X3RpbWVvdXQYBCABKAsyGS5nb29nbGUucHJvdG9idWYuRHVyYXRpb2'
    '5SC3dhaXRUaW1lb3V0');

@$core.Deprecated('Use removeNodeProgressDescriptor instead')
const RemoveNodeProgress$json = {
  '1': 'RemoveNodeProgress',
  '2': [
    {
      '1': 'phase',
      '3': 1,
      '4': 1,
      '5': 14,
      '6': '.protectorate.v1.RemovePhase',
      '10': 'phase'
    },
    {'1': 'running_jobs', '3': 2, '4': 1, '5': 13, '10': 'runningJobs'},
    {'1': 'message', '3': 3, '4': 1, '5': 9, '10': 'message'},
  ],
};

/// Descriptor for `RemoveNodeProgress`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List removeNodeProgressDescriptor = $convert.base64Decode(
    'ChJSZW1vdmVOb2RlUHJvZ3Jlc3MSMgoFcGhhc2UYASABKA4yHC5wcm90ZWN0b3JhdGUudjEuUm'
    'Vtb3ZlUGhhc2VSBXBoYXNlEiEKDHJ1bm5pbmdfam9icxgCIAEoDVILcnVubmluZ0pvYnMSGAoH'
    'bWVzc2FnZRgDIAEoCVIHbWVzc2FnZQ==');

@$core.Deprecated('Use uninstallRequestDescriptor instead')
const UninstallRequest$json = {
  '1': 'UninstallRequest',
};

/// Descriptor for `UninstallRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List uninstallRequestDescriptor =
    $convert.base64Decode('ChBVbmluc3RhbGxSZXF1ZXN0');

@$core.Deprecated('Use uninstallResponseDescriptor instead')
const UninstallResponse$json = {
  '1': 'UninstallResponse',
  '2': [
    {'1': 'actions', '3': 1, '4': 3, '5': 9, '10': 'actions'},
    {'1': 'left', '3': 2, '4': 3, '5': 9, '10': 'left'},
  ],
};

/// Descriptor for `UninstallResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List uninstallResponseDescriptor = $convert.base64Decode(
    'ChFVbmluc3RhbGxSZXNwb25zZRIYCgdhY3Rpb25zGAEgAygJUgdhY3Rpb25zEhIKBGxlZnQYAi'
    'ADKAlSBGxlZnQ=');

@$core.Deprecated('Use inviteNodeRequestDescriptor instead')
const InviteNodeRequest$json = {
  '1': 'InviteNodeRequest',
  '2': [
    {'1': 'name', '3': 1, '4': 1, '5': 9, '10': 'name'},
    {'1': 'tier', '3': 2, '4': 1, '5': 9, '10': 'tier'},
    {'1': 'pools', '3': 3, '4': 3, '5': 9, '10': 'pools'},
    {
      '1': 'ttl',
      '3': 4,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Duration',
      '10': 'ttl'
    },
    {'1': 'install_url', '3': 5, '4': 1, '5': 9, '10': 'installUrl'},
  ],
};

/// Descriptor for `InviteNodeRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List inviteNodeRequestDescriptor = $convert.base64Decode(
    'ChFJbnZpdGVOb2RlUmVxdWVzdBISCgRuYW1lGAEgASgJUgRuYW1lEhIKBHRpZXIYAiABKAlSBH'
    'RpZXISFAoFcG9vbHMYAyADKAlSBXBvb2xzEisKA3R0bBgEIAEoCzIZLmdvb2dsZS5wcm90b2J1'
    'Zi5EdXJhdGlvblIDdHRsEh8KC2luc3RhbGxfdXJsGAUgASgJUgppbnN0YWxsVXJs');

@$core.Deprecated('Use installCommandDescriptor instead')
const InstallCommand$json = {
  '1': 'InstallCommand',
  '2': [
    {
      '1': 'platform',
      '3': 1,
      '4': 1,
      '5': 14,
      '6': '.protectorate.v1.InstallPlatform',
      '10': 'platform'
    },
    {'1': 'shell', '3': 2, '4': 1, '5': 9, '10': 'shell'},
    {'1': 'command', '3': 3, '4': 1, '5': 9, '10': 'command'},
    {'1': 'note', '3': 4, '4': 1, '5': 9, '10': 'note'},
  ],
};

/// Descriptor for `InstallCommand`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List installCommandDescriptor = $convert.base64Decode(
    'Cg5JbnN0YWxsQ29tbWFuZBI8CghwbGF0Zm9ybRgBIAEoDjIgLnByb3RlY3RvcmF0ZS52MS5Jbn'
    'N0YWxsUGxhdGZvcm1SCHBsYXRmb3JtEhQKBXNoZWxsGAIgASgJUgVzaGVsbBIYCgdjb21tYW5k'
    'GAMgASgJUgdjb21tYW5kEhIKBG5vdGUYBCABKAlSBG5vdGU=');

@$core.Deprecated('Use inviteNodeResponseDescriptor instead')
const InviteNodeResponse$json = {
  '1': 'InviteNodeResponse',
  '2': [
    {
      '1': 'node',
      '3': 1,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.NodeInfo',
      '10': 'node'
    },
    {'1': 'hub_url', '3': 2, '4': 1, '5': 9, '10': 'hubUrl'},
    {'1': 'code', '3': 3, '4': 1, '5': 9, '10': 'code'},
    {
      '1': 'expires_at',
      '3': 4,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Timestamp',
      '10': 'expiresAt'
    },
    {'1': 'install_url', '3': 5, '4': 1, '5': 9, '10': 'installUrl'},
    {
      '1': 'commands',
      '3': 6,
      '4': 3,
      '5': 11,
      '6': '.protectorate.v1.InstallCommand',
      '10': 'commands'
    },
    {'1': 'ca_sha256', '3': 7, '4': 1, '5': 9, '10': 'caSha256'},
  ],
};

/// Descriptor for `InviteNodeResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List inviteNodeResponseDescriptor = $convert.base64Decode(
    'ChJJbnZpdGVOb2RlUmVzcG9uc2USLQoEbm9kZRgBIAEoCzIZLnByb3RlY3RvcmF0ZS52MS5Ob2'
    'RlSW5mb1IEbm9kZRIXCgdodWJfdXJsGAIgASgJUgZodWJVcmwSEgoEY29kZRgDIAEoCVIEY29k'
    'ZRI5CgpleHBpcmVzX2F0GAQgASgLMhouZ29vZ2xlLnByb3RvYnVmLlRpbWVzdGFtcFIJZXhwaX'
    'Jlc0F0Eh8KC2luc3RhbGxfdXJsGAUgASgJUgppbnN0YWxsVXJsEjsKCGNvbW1hbmRzGAYgAygL'
    'Mh8ucHJvdGVjdG9yYXRlLnYxLkluc3RhbGxDb21tYW5kUghjb21tYW5kcxIbCgljYV9zaGEyNT'
    'YYByABKAlSCGNhU2hhMjU2');

@$core.Deprecated('Use joinNodeRequestDescriptor instead')
const JoinNodeRequest$json = {
  '1': 'JoinNodeRequest',
  '2': [
    {'1': 'name', '3': 1, '4': 1, '5': 9, '10': 'name'},
    {'1': 'code', '3': 2, '4': 1, '5': 9, '10': 'code'},
    {'1': 'public_key', '3': 3, '4': 1, '5': 12, '10': 'publicKey'},
    {'1': 'proof', '3': 4, '4': 1, '5': 12, '10': 'proof'},
    {'1': 'address', '3': 5, '4': 1, '5': 9, '10': 'address'},
  ],
};

/// Descriptor for `JoinNodeRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List joinNodeRequestDescriptor = $convert.base64Decode(
    'Cg9Kb2luTm9kZVJlcXVlc3QSEgoEbmFtZRgBIAEoCVIEbmFtZRISCgRjb2RlGAIgASgJUgRjb2'
    'RlEh0KCnB1YmxpY19rZXkYAyABKAxSCXB1YmxpY0tleRIUCgVwcm9vZhgEIAEoDFIFcHJvb2YS'
    'GAoHYWRkcmVzcxgFIAEoCVIHYWRkcmVzcw==');

@$core.Deprecated('Use joinNodeResponseDescriptor instead')
const JoinNodeResponse$json = {
  '1': 'JoinNodeResponse',
  '2': [
    {'1': 'certificate', '3': 1, '4': 1, '5': 9, '10': 'certificate'},
    {'1': 'ca', '3': 2, '4': 1, '5': 9, '10': 'ca'},
  ],
};

/// Descriptor for `JoinNodeResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List joinNodeResponseDescriptor = $convert.base64Decode(
    'ChBKb2luTm9kZVJlc3BvbnNlEiAKC2NlcnRpZmljYXRlGAEgASgJUgtjZXJ0aWZpY2F0ZRIOCg'
    'JjYRgCIAEoCVICY2E=');

@$core.Deprecated('Use renewCertificateRequestDescriptor instead')
const RenewCertificateRequest$json = {
  '1': 'RenewCertificateRequest',
  '2': [
    {'1': 'certificate', '3': 1, '4': 1, '5': 9, '10': 'certificate'},
  ],
};

/// Descriptor for `RenewCertificateRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List renewCertificateRequestDescriptor =
    $convert.base64Decode(
        'ChdSZW5ld0NlcnRpZmljYXRlUmVxdWVzdBIgCgtjZXJ0aWZpY2F0ZRgBIAEoCVILY2VydGlmaW'
        'NhdGU=');

@$core.Deprecated('Use renewCertificateResponseDescriptor instead')
const RenewCertificateResponse$json = {
  '1': 'RenewCertificateResponse',
};

/// Descriptor for `RenewCertificateResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List renewCertificateResponseDescriptor =
    $convert.base64Decode('ChhSZW5ld0NlcnRpZmljYXRlUmVzcG9uc2U=');

@$core.Deprecated('Use watchNodeSetupRequestDescriptor instead')
const WatchNodeSetupRequest$json = {
  '1': 'WatchNodeSetupRequest',
  '2': [
    {'1': 'name', '3': 1, '4': 1, '5': 9, '10': 'name'},
  ],
};

/// Descriptor for `WatchNodeSetupRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List watchNodeSetupRequestDescriptor =
    $convert.base64Decode(
        'ChVXYXRjaE5vZGVTZXR1cFJlcXVlc3QSEgoEbmFtZRgBIAEoCVIEbmFtZQ==');

@$core.Deprecated('Use nodeSetupProgressDescriptor instead')
const NodeSetupProgress$json = {
  '1': 'NodeSetupProgress',
  '2': [
    {
      '1': 'stage',
      '3': 1,
      '4': 1,
      '5': 14,
      '6': '.protectorate.v1.NodeSetupStage',
      '10': 'stage'
    },
    {'1': 'problems', '3': 2, '4': 3, '5': 9, '10': 'problems'},
    {'1': 'notes', '3': 3, '4': 3, '5': 9, '10': 'notes'},
    {'1': 'message', '3': 4, '4': 1, '5': 9, '10': 'message'},
    {
      '1': 'status',
      '3': 5,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.NodeStatus',
      '10': 'status'
    },
  ],
};

/// Descriptor for `NodeSetupProgress`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List nodeSetupProgressDescriptor = $convert.base64Decode(
    'ChFOb2RlU2V0dXBQcm9ncmVzcxI1CgVzdGFnZRgBIAEoDjIfLnByb3RlY3RvcmF0ZS52MS5Ob2'
    'RlU2V0dXBTdGFnZVIFc3RhZ2USGgoIcHJvYmxlbXMYAiADKAlSCHByb2JsZW1zEhQKBW5vdGVz'
    'GAMgAygJUgVub3RlcxIYCgdtZXNzYWdlGAQgASgJUgdtZXNzYWdlEjMKBnN0YXR1cxgFIAEoCz'
    'IbLnByb3RlY3RvcmF0ZS52MS5Ob2RlU3RhdHVzUgZzdGF0dXM=');

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
      '1': 'AttachJob',
      '2': '.protectorate.v1.AttachJobRequest',
      '3': '.protectorate.v1.JobEvent',
      '6': true
    },
    {
      '1': 'ListJobs',
      '2': '.protectorate.v1.NodeAgentServiceListJobsRequest',
      '3': '.protectorate.v1.NodeAgentServiceListJobsResponse',
      '4': {'34': 1},
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
      '1': 'ApplyUpdate',
      '2': '.protectorate.v1.ApplyUpdateRequest',
      '3': '.protectorate.v1.ApplyUpdateResponse'
    },
    {
      '1': 'Uninstall',
      '2': '.protectorate.v1.UninstallRequest',
      '3': '.protectorate.v1.UninstallResponse'
    },
    {
      '1': 'RenewCertificate',
      '2': '.protectorate.v1.RenewCertificateRequest',
      '3': '.protectorate.v1.RenewCertificateResponse'
    },
  ],
  '3': {},
};

@$core.Deprecated('Use nodeAgentServiceDescriptor instead')
const $core.Map<$core.String, $core.Map<$core.String, $core.dynamic>>
    NodeAgentServiceBase$messageJson = {
  '.protectorate.v1.GetMetricsRequest': GetMetricsRequest$json,
  '.protectorate.v1.NodeMetrics': NodeMetrics$json,
  '.google.protobuf.Timestamp': $0.Timestamp$json,
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
  '.protectorate.v1.Sleeve': $1.Sleeve$json,
  '.protectorate.v1.SleeveLimits': $1.SleeveLimits$json,
  '.protectorate.v1.SleeveUsage': $1.SleeveUsage$json,
  '.protectorate.v1.ChatStatus': $4.ChatStatus$json,
  '.protectorate.v1.ModelSettings': $4.ModelSettings$json,
  '.protectorate.v1.ModelSelection': $4.ModelSelection$json,
  '.protectorate.v1.CacheTelemetry': $4.CacheTelemetry$json,
  '.protectorate.v1.TokenCounts': $4.TokenCounts$json,
  '.protectorate.v1.Handoff': $4.Handoff$json,
  '.protectorate.v1.ReadOnlyMount': $1.ReadOnlyMount$json,
  '.protectorate.v1.SleeveResult': $4.SleeveResult$json,
  '.protectorate.v1.AcceleratorMetrics': AcceleratorMetrics$json,
  '.protectorate.v1.JobIsolation': $2.JobIsolation$json,
  '.protectorate.v1.NodeUpdateState': NodeUpdateState$json,
  '.protectorate.v1.WatchMetricsRequest': WatchMetricsRequest$json,
  '.protectorate.v1.PrepareJobRequest': $2.PrepareJobRequest$json,
  '.protectorate.v1.PrepareJobResponse': $2.PrepareJobResponse$json,
  '.protectorate.v1.UploadJobDataRequest': $2.UploadJobDataRequest$json,
  '.protectorate.v1.UploadJobDataResponse': $2.UploadJobDataResponse$json,
  '.protectorate.v1.RunJobRequest': $2.RunJobRequest$json,
  '.protectorate.v1.RunJobRequest.EnvEntry': $2.RunJobRequest_EnvEntry$json,
  '.protectorate.v1.JobLimits': $2.JobLimits$json,
  '.protectorate.v1.JobEvent': $2.JobEvent$json,
  '.protectorate.v1.JobStarted': $2.JobStarted$json,
  '.protectorate.v1.JobOutput': $2.JobOutput$json,
  '.protectorate.v1.JobArtifactChunk': $2.JobArtifactChunk$json,
  '.protectorate.v1.JobFinished': $2.JobFinished$json,
  '.protectorate.v1.AttachJobRequest': $2.AttachJobRequest$json,
  '.protectorate.v1.NodeAgentServiceListJobsRequest':
      $2.NodeAgentServiceListJobsRequest$json,
  '.protectorate.v1.NodeAgentServiceListJobsResponse':
      $2.NodeAgentServiceListJobsResponse$json,
  '.protectorate.v1.NodeJob': $2.NodeJob$json,
  '.protectorate.v1.NodeAgentServiceCancelJobRequest':
      $2.NodeAgentServiceCancelJobRequest$json,
  '.protectorate.v1.NodeAgentServiceCancelJobResponse':
      $2.NodeAgentServiceCancelJobResponse$json,
  '.protectorate.v1.PutFileRequest': PutFileRequest$json,
  '.protectorate.v1.FileChunk': FileChunk$json,
  '.protectorate.v1.FileTrailer': FileTrailer$json,
  '.protectorate.v1.PutFileResponse': PutFileResponse$json,
  '.protectorate.v1.GetFileRequest': GetFileRequest$json,
  '.protectorate.v1.GetFileResponse': GetFileResponse$json,
  '.protectorate.v1.FileHeader': FileHeader$json,
  '.protectorate.v1.ApplyUpdateRequest': ApplyUpdateRequest$json,
  '.protectorate.v1.ApplyUpdateResponse': ApplyUpdateResponse$json,
  '.protectorate.v1.UninstallRequest': UninstallRequest$json,
  '.protectorate.v1.UninstallResponse': UninstallResponse$json,
  '.protectorate.v1.RenewCertificateRequest': RenewCertificateRequest$json,
  '.protectorate.v1.RenewCertificateResponse': RenewCertificateResponse$json,
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
    'bnQwARJLCglBdHRhY2hKb2ISIS5wcm90ZWN0b3JhdGUudjEuQXR0YWNoSm9iUmVxdWVzdBoZLn'
    'Byb3RlY3RvcmF0ZS52MS5Kb2JFdmVudDABEnQKCExpc3RKb2JzEjAucHJvdGVjdG9yYXRlLnYx'
    'Lk5vZGVBZ2VudFNlcnZpY2VMaXN0Sm9ic1JlcXVlc3QaMS5wcm90ZWN0b3JhdGUudjEuTm9kZU'
    'FnZW50U2VydmljZUxpc3RKb2JzUmVzcG9uc2UiA5ACARJyCglDYW5jZWxKb2ISMS5wcm90ZWN0'
    'b3JhdGUudjEuTm9kZUFnZW50U2VydmljZUNhbmNlbEpvYlJlcXVlc3QaMi5wcm90ZWN0b3JhdG'
    'UudjEuTm9kZUFnZW50U2VydmljZUNhbmNlbEpvYlJlc3BvbnNlEkwKB1B1dEZpbGUSHy5wcm90'
    'ZWN0b3JhdGUudjEuUHV0RmlsZVJlcXVlc3QaIC5wcm90ZWN0b3JhdGUudjEuUHV0RmlsZVJlc3'
    'BvbnNlEk4KB0dldEZpbGUSHy5wcm90ZWN0b3JhdGUudjEuR2V0RmlsZVJlcXVlc3QaIC5wcm90'
    'ZWN0b3JhdGUudjEuR2V0RmlsZVJlc3BvbnNlMAESWAoLQXBwbHlVcGRhdGUSIy5wcm90ZWN0b3'
    'JhdGUudjEuQXBwbHlVcGRhdGVSZXF1ZXN0GiQucHJvdGVjdG9yYXRlLnYxLkFwcGx5VXBkYXRl'
    'UmVzcG9uc2USUgoJVW5pbnN0YWxsEiEucHJvdGVjdG9yYXRlLnYxLlVuaW5zdGFsbFJlcXVlc3'
    'QaIi5wcm90ZWN0b3JhdGUudjEuVW5pbnN0YWxsUmVzcG9uc2USZwoQUmVuZXdDZXJ0aWZpY2F0'
    'ZRIoLnByb3RlY3RvcmF0ZS52MS5SZW5ld0NlcnRpZmljYXRlUmVxdWVzdBopLnByb3RlY3Rvcm'
    'F0ZS52MS5SZW5ld0NlcnRpZmljYXRlUmVzcG9uc2UaBJC1GAE=');

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
      '3': '.protectorate.v1.RemoveNodeProgress',
      '4': {},
      '6': true
    },
    {
      '1': 'InviteNode',
      '2': '.protectorate.v1.InviteNodeRequest',
      '3': '.protectorate.v1.InviteNodeResponse',
      '4': {}
    },
    {
      '1': 'JoinNode',
      '2': '.protectorate.v1.JoinNodeRequest',
      '3': '.protectorate.v1.JoinNodeResponse'
    },
    {
      '1': 'WatchNodeSetup',
      '2': '.protectorate.v1.WatchNodeSetupRequest',
      '3': '.protectorate.v1.NodeSetupProgress',
      '4': {'34': 1},
      '6': true,
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
      '1': 'SetNodeTier',
      '2': '.protectorate.v1.SetNodeTierRequest',
      '3': '.protectorate.v1.NodeInfo',
      '4': {}
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
  '.google.protobuf.Timestamp': $0.Timestamp$json,
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
  '.protectorate.v1.Sleeve': $1.Sleeve$json,
  '.protectorate.v1.SleeveLimits': $1.SleeveLimits$json,
  '.protectorate.v1.SleeveUsage': $1.SleeveUsage$json,
  '.protectorate.v1.ChatStatus': $4.ChatStatus$json,
  '.protectorate.v1.ModelSettings': $4.ModelSettings$json,
  '.protectorate.v1.ModelSelection': $4.ModelSelection$json,
  '.protectorate.v1.CacheTelemetry': $4.CacheTelemetry$json,
  '.protectorate.v1.TokenCounts': $4.TokenCounts$json,
  '.protectorate.v1.Handoff': $4.Handoff$json,
  '.protectorate.v1.ReadOnlyMount': $1.ReadOnlyMount$json,
  '.protectorate.v1.SleeveResult': $4.SleeveResult$json,
  '.protectorate.v1.AcceleratorMetrics': AcceleratorMetrics$json,
  '.protectorate.v1.JobIsolation': $2.JobIsolation$json,
  '.protectorate.v1.NodeUpdateState': NodeUpdateState$json,
  '.protectorate.v1.NodeAlert': NodeAlert$json,
  '.protectorate.v1.GpuBusy': GpuBusy$json,
  '.protectorate.v1.GetNodeRequest': GetNodeRequest$json,
  '.protectorate.v1.AddNodeRequest': AddNodeRequest$json,
  '.protectorate.v1.RemoveNodeRequest': RemoveNodeRequest$json,
  '.protectorate.v1.RemoveNodeProgress': RemoveNodeProgress$json,
  '.protectorate.v1.InviteNodeRequest': InviteNodeRequest$json,
  '.protectorate.v1.InviteNodeResponse': InviteNodeResponse$json,
  '.protectorate.v1.InstallCommand': InstallCommand$json,
  '.protectorate.v1.JoinNodeRequest': JoinNodeRequest$json,
  '.protectorate.v1.JoinNodeResponse': JoinNodeResponse$json,
  '.protectorate.v1.WatchNodeSetupRequest': WatchNodeSetupRequest$json,
  '.protectorate.v1.NodeSetupProgress': NodeSetupProgress$json,
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
  '.protectorate.v1.SetNodeTierRequest': SetNodeTierRequest$json,
};

/// Descriptor for `NodeService`. Decode as a `google.protobuf.ServiceDescriptorProto`.
final $typed_data.Uint8List nodeServiceDescriptor = $convert.base64Decode(
    'CgtOb2RlU2VydmljZRJXCglMaXN0Tm9kZXMSIS5wcm90ZWN0b3JhdGUudjEuTGlzdE5vZGVzUm'
    'VxdWVzdBoiLnByb3RlY3RvcmF0ZS52MS5MaXN0Tm9kZXNSZXNwb25zZSIDkAIBEkwKB0dldE5v'
    'ZGUSHy5wcm90ZWN0b3JhdGUudjEuR2V0Tm9kZVJlcXVlc3QaGy5wcm90ZWN0b3JhdGUudjEuTm'
    '9kZVN0YXR1cyIDkAIBEksKB0FkZE5vZGUSHy5wcm90ZWN0b3JhdGUudjEuQWRkTm9kZVJlcXVl'
    'c3QaGS5wcm90ZWN0b3JhdGUudjEuTm9kZUluZm8iBKC1GAESYQoKUmVtb3ZlTm9kZRIiLnByb3'
    'RlY3RvcmF0ZS52MS5SZW1vdmVOb2RlUmVxdWVzdBojLnByb3RlY3RvcmF0ZS52MS5SZW1vdmVO'
    'b2RlUHJvZ3Jlc3MiCIi1GAGgtRgBMAESWwoKSW52aXRlTm9kZRIiLnByb3RlY3RvcmF0ZS52MS'
    '5JbnZpdGVOb2RlUmVxdWVzdBojLnByb3RlY3RvcmF0ZS52MS5JbnZpdGVOb2RlUmVzcG9uc2Ui'
    'BKC1GAESTwoISm9pbk5vZGUSIC5wcm90ZWN0b3JhdGUudjEuSm9pbk5vZGVSZXF1ZXN0GiEucH'
    'JvdGVjdG9yYXRlLnYxLkpvaW5Ob2RlUmVzcG9uc2USYwoOV2F0Y2hOb2RlU2V0dXASJi5wcm90'
    'ZWN0b3JhdGUudjEuV2F0Y2hOb2RlU2V0dXBSZXF1ZXN0GiIucHJvdGVjdG9yYXRlLnYxLk5vZG'
    'VTZXR1cFByb2dyZXNzIgOQAgEwARJMCgdQdXRGaWxlEh8ucHJvdGVjdG9yYXRlLnYxLlB1dEZp'
    'bGVSZXF1ZXN0GiAucHJvdGVjdG9yYXRlLnYxLlB1dEZpbGVSZXNwb25zZRJOCgdHZXRGaWxlEh'
    '8ucHJvdGVjdG9yYXRlLnYxLkdldEZpbGVSZXF1ZXN0GiAucHJvdGVjdG9yYXRlLnYxLkdldEZp'
    'bGVSZXNwb25zZTABEl4KC0FwcGx5VXBkYXRlEiMucHJvdGVjdG9yYXRlLnYxLkFwcGx5VXBkYX'
    'RlUmVxdWVzdBokLnByb3RlY3RvcmF0ZS52MS5BcHBseVVwZGF0ZVJlc3BvbnNlIgSgtRgBEmEK'
    'ClVwZGF0ZU5vZGUSIi5wcm90ZWN0b3JhdGUudjEuVXBkYXRlTm9kZVJlcXVlc3QaIy5wcm90ZW'
    'N0b3JhdGUudjEuVXBkYXRlTm9kZVByb2dyZXNzIgigtRgBqLUYATABElMKC1NldE5vZGVUaWVy'
    'EiMucHJvdGVjdG9yYXRlLnYxLlNldE5vZGVUaWVyUmVxdWVzdBoZLnByb3RlY3RvcmF0ZS52MS'
    '5Ob2RlSW5mbyIEoLUYAQ==');
