// This is a generated file - do not edit.
//
// Generated from protectorate/v1/job.proto.

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
    as $1;
import 'package:protobuf/well_known_types/google/protobuf/timestamp.pbjson.dart'
    as $2;

import 'lease.pbjson.dart' as $0;

@$core.Deprecated('Use jobStateDescriptor instead')
const JobState$json = {
  '1': 'JobState',
  '2': [
    {'1': 'JOB_STATE_UNSPECIFIED', '2': 0},
    {'1': 'JOB_STATE_PREPARING', '2': 1},
    {'1': 'JOB_STATE_RUNNING', '2': 2},
    {'1': 'JOB_STATE_SUCCEEDED', '2': 3},
    {'1': 'JOB_STATE_FAILED', '2': 4},
    {'1': 'JOB_STATE_CANCELED', '2': 5},
    {'1': 'JOB_STATE_TIMED_OUT', '2': 6},
  ],
};

/// Descriptor for `JobState`. Decode as a `google.protobuf.EnumDescriptorProto`.
final $typed_data.Uint8List jobStateDescriptor = $convert.base64Decode(
    'CghKb2JTdGF0ZRIZChVKT0JfU1RBVEVfVU5TUEVDSUZJRUQQABIXChNKT0JfU1RBVEVfUFJFUE'
    'FSSU5HEAESFQoRSk9CX1NUQVRFX1JVTk5JTkcQAhIXChNKT0JfU1RBVEVfU1VDQ0VFREVEEAMS'
    'FAoQSk9CX1NUQVRFX0ZBSUxFRBAEEhYKEkpPQl9TVEFURV9DQU5DRUxFRBAFEhcKE0pPQl9TVE'
    'FURV9USU1FRF9PVVQQBg==');

@$core.Deprecated('Use jobEndDescriptor instead')
const JobEnd$json = {
  '1': 'JobEnd',
  '2': [
    {'1': 'JOB_END_UNSPECIFIED', '2': 0},
    {'1': 'JOB_END_EXITED', '2': 1},
    {'1': 'JOB_END_TIMED_OUT', '2': 2},
    {'1': 'JOB_END_CANCELED', '2': 3},
    {'1': 'JOB_END_ERROR', '2': 4},
  ],
};

/// Descriptor for `JobEnd`. Decode as a `google.protobuf.EnumDescriptorProto`.
final $typed_data.Uint8List jobEndDescriptor = $convert.base64Decode(
    'CgZKb2JFbmQSFwoTSk9CX0VORF9VTlNQRUNJRklFRBAAEhIKDkpPQl9FTkRfRVhJVEVEEAESFQ'
    'oRSk9CX0VORF9USU1FRF9PVVQQAhIUChBKT0JfRU5EX0NBTkNFTEVEEAMSEQoNSk9CX0VORF9F'
    'UlJPUhAE');

@$core.Deprecated('Use jobTargetDescriptor instead')
const JobTarget$json = {
  '1': 'JobTarget',
  '2': [
    {'1': 'lease_id', '3': 1, '4': 1, '5': 9, '9': 0, '10': 'leaseId'},
    {'1': 'node', '3': 2, '4': 1, '5': 9, '9': 0, '10': 'node'},
    {
      '1': 'selector',
      '3': 3,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.NodeSelector',
      '9': 0,
      '10': 'selector'
    },
    {'1': 'allow_family', '3': 4, '4': 1, '5': 8, '10': 'allowFamily'},
  ],
  '8': [
    {'1': 'target'},
  ],
};

/// Descriptor for `JobTarget`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List jobTargetDescriptor = $convert.base64Decode(
    'CglKb2JUYXJnZXQSGwoIbGVhc2VfaWQYASABKAlIAFIHbGVhc2VJZBIUCgRub2RlGAIgASgJSA'
    'BSBG5vZGUSOwoIc2VsZWN0b3IYAyABKAsyHS5wcm90ZWN0b3JhdGUudjEuTm9kZVNlbGVjdG9y'
    'SABSCHNlbGVjdG9yEiEKDGFsbG93X2ZhbWlseRgEIAEoCFILYWxsb3dGYW1pbHlCCAoGdGFyZ2'
    'V0');

@$core.Deprecated('Use gitSourceDescriptor instead')
const GitSource$json = {
  '1': 'GitSource',
  '2': [
    {'1': 'repo', '3': 1, '4': 1, '5': 9, '10': 'repo'},
    {'1': 'commit', '3': 2, '4': 1, '5': 9, '10': 'commit'},
  ],
};

/// Descriptor for `GitSource`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List gitSourceDescriptor = $convert.base64Decode(
    'CglHaXRTb3VyY2USEgoEcmVwbxgBIAEoCVIEcmVwbxIWCgZjb21taXQYAiABKAlSBmNvbW1pdA'
    '==');

@$core.Deprecated('Use artifactSourceDescriptor instead')
const ArtifactSource$json = {
  '1': 'ArtifactSource',
  '2': [
    {'1': 'path', '3': 1, '4': 1, '5': 9, '10': 'path'},
    {'1': 'workdir', '3': 2, '4': 1, '5': 9, '10': 'workdir'},
  ],
};

/// Descriptor for `ArtifactSource`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List artifactSourceDescriptor = $convert.base64Decode(
    'Cg5BcnRpZmFjdFNvdXJjZRISCgRwYXRoGAEgASgJUgRwYXRoEhgKB3dvcmtkaXIYAiABKAlSB3'
    'dvcmtkaXI=');

@$core.Deprecated('Use jobSourceDescriptor instead')
const JobSource$json = {
  '1': 'JobSource',
  '2': [
    {
      '1': 'git',
      '3': 1,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.GitSource',
      '9': 0,
      '10': 'git'
    },
    {
      '1': 'artifact',
      '3': 2,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.ArtifactSource',
      '9': 0,
      '10': 'artifact'
    },
  ],
  '8': [
    {'1': 'kind'},
  ],
};

/// Descriptor for `JobSource`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List jobSourceDescriptor = $convert.base64Decode(
    'CglKb2JTb3VyY2USLgoDZ2l0GAEgASgLMhoucHJvdGVjdG9yYXRlLnYxLkdpdFNvdXJjZUgAUg'
    'NnaXQSPQoIYXJ0aWZhY3QYAiABKAsyHy5wcm90ZWN0b3JhdGUudjEuQXJ0aWZhY3RTb3VyY2VI'
    'AFIIYXJ0aWZhY3RCBgoEa2luZA==');

@$core.Deprecated('Use submitJobRequestDescriptor instead')
const SubmitJobRequest$json = {
  '1': 'SubmitJobRequest',
  '2': [
    {
      '1': 'target',
      '3': 1,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.JobTarget',
      '10': 'target'
    },
    {
      '1': 'source',
      '3': 2,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.JobSource',
      '10': 'source'
    },
    {'1': 'command', '3': 3, '4': 3, '5': 9, '10': 'command'},
    {
      '1': 'env',
      '3': 4,
      '4': 3,
      '5': 11,
      '6': '.protectorate.v1.SubmitJobRequest.EnvEntry',
      '10': 'env'
    },
    {
      '1': 'timeout',
      '3': 5,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Duration',
      '10': 'timeout'
    },
    {'1': 'artifact_globs', '3': 6, '4': 3, '5': 9, '10': 'artifactGlobs'},
    {'1': 'request_id', '3': 7, '4': 1, '5': 9, '10': 'requestId'},
  ],
  '3': [SubmitJobRequest_EnvEntry$json],
};

@$core.Deprecated('Use submitJobRequestDescriptor instead')
const SubmitJobRequest_EnvEntry$json = {
  '1': 'EnvEntry',
  '2': [
    {'1': 'key', '3': 1, '4': 1, '5': 9, '10': 'key'},
    {'1': 'value', '3': 2, '4': 1, '5': 9, '10': 'value'},
  ],
  '7': {'7': true},
};

/// Descriptor for `SubmitJobRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List submitJobRequestDescriptor = $convert.base64Decode(
    'ChBTdWJtaXRKb2JSZXF1ZXN0EjIKBnRhcmdldBgBIAEoCzIaLnByb3RlY3RvcmF0ZS52MS5Kb2'
    'JUYXJnZXRSBnRhcmdldBIyCgZzb3VyY2UYAiABKAsyGi5wcm90ZWN0b3JhdGUudjEuSm9iU291'
    'cmNlUgZzb3VyY2USGAoHY29tbWFuZBgDIAMoCVIHY29tbWFuZBI8CgNlbnYYBCADKAsyKi5wcm'
    '90ZWN0b3JhdGUudjEuU3VibWl0Sm9iUmVxdWVzdC5FbnZFbnRyeVIDZW52EjMKB3RpbWVvdXQY'
    'BSABKAsyGS5nb29nbGUucHJvdG9idWYuRHVyYXRpb25SB3RpbWVvdXQSJQoOYXJ0aWZhY3RfZ2'
    'xvYnMYBiADKAlSDWFydGlmYWN0R2xvYnMSHQoKcmVxdWVzdF9pZBgHIAEoCVIJcmVxdWVzdElk'
    'GjYKCEVudkVudHJ5EhAKA2tleRgBIAEoCVIDa2V5EhQKBXZhbHVlGAIgASgJUgV2YWx1ZToCOA'
    'E=');

@$core.Deprecated('Use getJobRequestDescriptor instead')
const GetJobRequest$json = {
  '1': 'GetJobRequest',
  '2': [
    {'1': 'id', '3': 1, '4': 1, '5': 9, '10': 'id'},
  ],
};

/// Descriptor for `GetJobRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List getJobRequestDescriptor =
    $convert.base64Decode('Cg1HZXRKb2JSZXF1ZXN0Eg4KAmlkGAEgASgJUgJpZA==');

@$core.Deprecated('Use listJobsRequestDescriptor instead')
const ListJobsRequest$json = {
  '1': 'ListJobsRequest',
  '2': [
    {
      '1': 'state',
      '3': 1,
      '4': 1,
      '5': 14,
      '6': '.protectorate.v1.JobState',
      '10': 'state'
    },
    {'1': 'limit', '3': 2, '4': 1, '5': 5, '10': 'limit'},
  ],
};

/// Descriptor for `ListJobsRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List listJobsRequestDescriptor = $convert.base64Decode(
    'Cg9MaXN0Sm9ic1JlcXVlc3QSLwoFc3RhdGUYASABKA4yGS5wcm90ZWN0b3JhdGUudjEuSm9iU3'
    'RhdGVSBXN0YXRlEhQKBWxpbWl0GAIgASgFUgVsaW1pdA==');

@$core.Deprecated('Use listJobsResponseDescriptor instead')
const ListJobsResponse$json = {
  '1': 'ListJobsResponse',
  '2': [
    {
      '1': 'jobs',
      '3': 1,
      '4': 3,
      '5': 11,
      '6': '.protectorate.v1.Job',
      '10': 'jobs'
    },
  ],
};

/// Descriptor for `ListJobsResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List listJobsResponseDescriptor = $convert.base64Decode(
    'ChBMaXN0Sm9ic1Jlc3BvbnNlEigKBGpvYnMYASADKAsyFC5wcm90ZWN0b3JhdGUudjEuSm9iUg'
    'Rqb2Jz');

@$core.Deprecated('Use cancelJobRequestDescriptor instead')
const CancelJobRequest$json = {
  '1': 'CancelJobRequest',
  '2': [
    {'1': 'id', '3': 1, '4': 1, '5': 9, '10': 'id'},
    {'1': 'request_id', '3': 2, '4': 1, '5': 9, '10': 'requestId'},
  ],
};

/// Descriptor for `CancelJobRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List cancelJobRequestDescriptor = $convert.base64Decode(
    'ChBDYW5jZWxKb2JSZXF1ZXN0Eg4KAmlkGAEgASgJUgJpZBIdCgpyZXF1ZXN0X2lkGAIgASgJUg'
    'lyZXF1ZXN0SWQ=');

@$core.Deprecated('Use watchJobRequestDescriptor instead')
const WatchJobRequest$json = {
  '1': 'WatchJobRequest',
  '2': [
    {'1': 'id', '3': 1, '4': 1, '5': 9, '10': 'id'},
    {'1': 'from_offset', '3': 2, '4': 1, '5': 4, '10': 'fromOffset'},
    {'1': 'follow', '3': 3, '4': 1, '5': 8, '10': 'follow'},
  ],
};

/// Descriptor for `WatchJobRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List watchJobRequestDescriptor = $convert.base64Decode(
    'Cg9XYXRjaEpvYlJlcXVlc3QSDgoCaWQYASABKAlSAmlkEh8KC2Zyb21fb2Zmc2V0GAIgASgEUg'
    'pmcm9tT2Zmc2V0EhYKBmZvbGxvdxgDIAEoCFIGZm9sbG93');

@$core.Deprecated('Use watchJobResponseDescriptor instead')
const WatchJobResponse$json = {
  '1': 'WatchJobResponse',
  '2': [
    {
      '1': 'job',
      '3': 1,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.Job',
      '9': 0,
      '10': 'job'
    },
    {
      '1': 'log',
      '3': 2,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.JobLog',
      '9': 0,
      '10': 'log'
    },
  ],
  '8': [
    {'1': 'event'},
  ],
};

/// Descriptor for `WatchJobResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List watchJobResponseDescriptor = $convert.base64Decode(
    'ChBXYXRjaEpvYlJlc3BvbnNlEigKA2pvYhgBIAEoCzIULnByb3RlY3RvcmF0ZS52MS5Kb2JIAF'
    'IDam9iEisKA2xvZxgCIAEoCzIXLnByb3RlY3RvcmF0ZS52MS5Kb2JMb2dIAFIDbG9nQgcKBWV2'
    'ZW50');

@$core.Deprecated('Use jobLogDescriptor instead')
const JobLog$json = {
  '1': 'JobLog',
  '2': [
    {'1': 'offset', '3': 1, '4': 1, '5': 4, '10': 'offset'},
    {'1': 'data', '3': 2, '4': 1, '5': 12, '10': 'data'},
  ],
};

/// Descriptor for `JobLog`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List jobLogDescriptor = $convert.base64Decode(
    'CgZKb2JMb2cSFgoGb2Zmc2V0GAEgASgEUgZvZmZzZXQSEgoEZGF0YRgCIAEoDFIEZGF0YQ==');

@$core.Deprecated('Use jobArtifactDescriptor instead')
const JobArtifact$json = {
  '1': 'JobArtifact',
  '2': [
    {'1': 'path', '3': 1, '4': 1, '5': 9, '10': 'path'},
    {'1': 'size_bytes', '3': 2, '4': 1, '5': 4, '10': 'sizeBytes'},
    {'1': 'mode', '3': 3, '4': 1, '5': 13, '10': 'mode'},
  ],
};

/// Descriptor for `JobArtifact`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List jobArtifactDescriptor = $convert.base64Decode(
    'CgtKb2JBcnRpZmFjdBISCgRwYXRoGAEgASgJUgRwYXRoEh0KCnNpemVfYnl0ZXMYAiABKARSCX'
    'NpemVCeXRlcxISCgRtb2RlGAMgASgNUgRtb2Rl');

@$core.Deprecated('Use jobDescriptor instead')
const Job$json = {
  '1': 'Job',
  '2': [
    {'1': 'id', '3': 1, '4': 1, '5': 9, '10': 'id'},
    {
      '1': 'state',
      '3': 2,
      '4': 1,
      '5': 14,
      '6': '.protectorate.v1.JobState',
      '10': 'state'
    },
    {
      '1': 'target',
      '3': 3,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.JobTarget',
      '10': 'target'
    },
    {'1': 'node', '3': 4, '4': 1, '5': 9, '10': 'node'},
    {
      '1': 'gpu_index',
      '3': 5,
      '4': 1,
      '5': 5,
      '9': 0,
      '10': 'gpuIndex',
      '17': true
    },
    {
      '1': 'source',
      '3': 6,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.JobSource',
      '10': 'source'
    },
    {'1': 'command', '3': 7, '4': 3, '5': 9, '10': 'command'},
    {'1': 'env_keys', '3': 8, '4': 3, '5': 9, '10': 'envKeys'},
    {
      '1': 'timeout',
      '3': 9,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Duration',
      '10': 'timeout'
    },
    {'1': 'artifact_globs', '3': 10, '4': 3, '5': 9, '10': 'artifactGlobs'},
    {'1': 'exit_code', '3': 11, '4': 1, '5': 5, '10': 'exitCode'},
    {'1': 'error', '3': 12, '4': 1, '5': 9, '10': 'error'},
    {
      '1': 'submitted_at',
      '3': 13,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Timestamp',
      '10': 'submittedAt'
    },
    {
      '1': 'started_at',
      '3': 14,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Timestamp',
      '10': 'startedAt'
    },
    {
      '1': 'finished_at',
      '3': 15,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Timestamp',
      '10': 'finishedAt'
    },
    {'1': 'source_bytes', '3': 16, '4': 1, '5': 4, '10': 'sourceBytes'},
    {
      '1': 'transfer_time',
      '3': 17,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Duration',
      '10': 'transferTime'
    },
    {
      '1': 'run_time',
      '3': 18,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Duration',
      '10': 'runTime'
    },
    {'1': 'workspace_reused', '3': 19, '4': 1, '5': 8, '10': 'workspaceReused'},
    {
      '1': 'artifacts',
      '3': 20,
      '4': 3,
      '5': 11,
      '6': '.protectorate.v1.JobArtifact',
      '10': 'artifacts'
    },
    {'1': 'artifact_bytes', '3': 21, '4': 1, '5': 4, '10': 'artifactBytes'},
    {'1': 'log_bytes', '3': 22, '4': 1, '5': 4, '10': 'logBytes'},
  ],
  '8': [
    {'1': '_gpu_index'},
  ],
};

/// Descriptor for `Job`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List jobDescriptor = $convert.base64Decode(
    'CgNKb2ISDgoCaWQYASABKAlSAmlkEi8KBXN0YXRlGAIgASgOMhkucHJvdGVjdG9yYXRlLnYxLk'
    'pvYlN0YXRlUgVzdGF0ZRIyCgZ0YXJnZXQYAyABKAsyGi5wcm90ZWN0b3JhdGUudjEuSm9iVGFy'
    'Z2V0UgZ0YXJnZXQSEgoEbm9kZRgEIAEoCVIEbm9kZRIgCglncHVfaW5kZXgYBSABKAVIAFIIZ3'
    'B1SW5kZXiIAQESMgoGc291cmNlGAYgASgLMhoucHJvdGVjdG9yYXRlLnYxLkpvYlNvdXJjZVIG'
    'c291cmNlEhgKB2NvbW1hbmQYByADKAlSB2NvbW1hbmQSGQoIZW52X2tleXMYCCADKAlSB2Vudk'
    'tleXMSMwoHdGltZW91dBgJIAEoCzIZLmdvb2dsZS5wcm90b2J1Zi5EdXJhdGlvblIHdGltZW91'
    'dBIlCg5hcnRpZmFjdF9nbG9icxgKIAMoCVINYXJ0aWZhY3RHbG9icxIbCglleGl0X2NvZGUYCy'
    'ABKAVSCGV4aXRDb2RlEhQKBWVycm9yGAwgASgJUgVlcnJvchI9CgxzdWJtaXR0ZWRfYXQYDSAB'
    'KAsyGi5nb29nbGUucHJvdG9idWYuVGltZXN0YW1wUgtzdWJtaXR0ZWRBdBI5CgpzdGFydGVkX2'
    'F0GA4gASgLMhouZ29vZ2xlLnByb3RvYnVmLlRpbWVzdGFtcFIJc3RhcnRlZEF0EjsKC2Zpbmlz'
    'aGVkX2F0GA8gASgLMhouZ29vZ2xlLnByb3RvYnVmLlRpbWVzdGFtcFIKZmluaXNoZWRBdBIhCg'
    'xzb3VyY2VfYnl0ZXMYECABKARSC3NvdXJjZUJ5dGVzEj4KDXRyYW5zZmVyX3RpbWUYESABKAsy'
    'GS5nb29nbGUucHJvdG9idWYuRHVyYXRpb25SDHRyYW5zZmVyVGltZRI0CghydW5fdGltZRgSIA'
    'EoCzIZLmdvb2dsZS5wcm90b2J1Zi5EdXJhdGlvblIHcnVuVGltZRIpChB3b3Jrc3BhY2VfcmV1'
    'c2VkGBMgASgIUg93b3Jrc3BhY2VSZXVzZWQSOgoJYXJ0aWZhY3RzGBQgAygLMhwucHJvdGVjdG'
    '9yYXRlLnYxLkpvYkFydGlmYWN0UglhcnRpZmFjdHMSJQoOYXJ0aWZhY3RfYnl0ZXMYFSABKARS'
    'DWFydGlmYWN0Qnl0ZXMSGwoJbG9nX2J5dGVzGBYgASgEUghsb2dCeXRlc0IMCgpfZ3B1X2luZG'
    'V4');

@$core.Deprecated('Use prepareJobRequestDescriptor instead')
const PrepareJobRequest$json = {
  '1': 'PrepareJobRequest',
  '2': [
    {'1': 'job_id', '3': 1, '4': 1, '5': 9, '10': 'jobId'},
    {'1': 'repo_key', '3': 2, '4': 1, '5': 9, '10': 'repoKey'},
    {'1': 'commit', '3': 3, '4': 1, '5': 9, '10': 'commit'},
  ],
};

/// Descriptor for `PrepareJobRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List prepareJobRequestDescriptor = $convert.base64Decode(
    'ChFQcmVwYXJlSm9iUmVxdWVzdBIVCgZqb2JfaWQYASABKAlSBWpvYklkEhkKCHJlcG9fa2V5GA'
    'IgASgJUgdyZXBvS2V5EhYKBmNvbW1pdBgDIAEoCVIGY29tbWl0');

@$core.Deprecated('Use prepareJobResponseDescriptor instead')
const PrepareJobResponse$json = {
  '1': 'PrepareJobResponse',
  '2': [
    {'1': 'has_commit', '3': 1, '4': 1, '5': 8, '10': 'hasCommit'},
    {'1': 'known_commits', '3': 2, '4': 3, '5': 9, '10': 'knownCommits'},
  ],
};

/// Descriptor for `PrepareJobResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List prepareJobResponseDescriptor = $convert.base64Decode(
    'ChJQcmVwYXJlSm9iUmVzcG9uc2USHQoKaGFzX2NvbW1pdBgBIAEoCFIJaGFzQ29tbWl0EiMKDW'
    'tub3duX2NvbW1pdHMYAiADKAlSDGtub3duQ29tbWl0cw==');

@$core.Deprecated('Use uploadJobDataRequestDescriptor instead')
const UploadJobDataRequest$json = {
  '1': 'UploadJobDataRequest',
  '2': [
    {'1': 'job_id', '3': 1, '4': 1, '5': 9, '10': 'jobId'},
    {'1': 'offset', '3': 2, '4': 1, '5': 4, '10': 'offset'},
    {'1': 'data', '3': 3, '4': 1, '5': 12, '10': 'data'},
    {'1': 'last', '3': 4, '4': 1, '5': 8, '10': 'last'},
    {'1': 'sha256', '3': 5, '4': 1, '5': 9, '10': 'sha256'},
  ],
};

/// Descriptor for `UploadJobDataRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List uploadJobDataRequestDescriptor = $convert.base64Decode(
    'ChRVcGxvYWRKb2JEYXRhUmVxdWVzdBIVCgZqb2JfaWQYASABKAlSBWpvYklkEhYKBm9mZnNldB'
    'gCIAEoBFIGb2Zmc2V0EhIKBGRhdGEYAyABKAxSBGRhdGESEgoEbGFzdBgEIAEoCFIEbGFzdBIW'
    'CgZzaGEyNTYYBSABKAlSBnNoYTI1Ng==');

@$core.Deprecated('Use uploadJobDataResponseDescriptor instead')
const UploadJobDataResponse$json = {
  '1': 'UploadJobDataResponse',
  '2': [
    {'1': 'received', '3': 1, '4': 1, '5': 4, '10': 'received'},
  ],
};

/// Descriptor for `UploadJobDataResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List uploadJobDataResponseDescriptor =
    $convert.base64Decode(
        'ChVVcGxvYWRKb2JEYXRhUmVzcG9uc2USGgoIcmVjZWl2ZWQYASABKARSCHJlY2VpdmVk');

@$core.Deprecated('Use runJobRequestDescriptor instead')
const RunJobRequest$json = {
  '1': 'RunJobRequest',
  '2': [
    {'1': 'job_id', '3': 1, '4': 1, '5': 9, '10': 'jobId'},
    {'1': 'repo_key', '3': 2, '4': 1, '5': 9, '10': 'repoKey'},
    {'1': 'commit', '3': 3, '4': 1, '5': 9, '10': 'commit'},
    {'1': 'bundle_uploaded', '3': 4, '4': 1, '5': 8, '10': 'bundleUploaded'},
    {'1': 'artifact_name', '3': 5, '4': 1, '5': 9, '10': 'artifactName'},
    {'1': 'command', '3': 6, '4': 3, '5': 9, '10': 'command'},
    {
      '1': 'env',
      '3': 7,
      '4': 3,
      '5': 11,
      '6': '.protectorate.v1.RunJobRequest.EnvEntry',
      '10': 'env'
    },
    {
      '1': 'timeout',
      '3': 8,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Duration',
      '10': 'timeout'
    },
    {'1': 'artifact_globs', '3': 9, '4': 3, '5': 9, '10': 'artifactGlobs'},
    {'1': 'artifact_archive', '3': 10, '4': 1, '5': 8, '10': 'artifactArchive'},
    {'1': 'workdir', '3': 11, '4': 1, '5': 9, '10': 'workdir'},
  ],
  '3': [RunJobRequest_EnvEntry$json],
};

@$core.Deprecated('Use runJobRequestDescriptor instead')
const RunJobRequest_EnvEntry$json = {
  '1': 'EnvEntry',
  '2': [
    {'1': 'key', '3': 1, '4': 1, '5': 9, '10': 'key'},
    {'1': 'value', '3': 2, '4': 1, '5': 9, '10': 'value'},
  ],
  '7': {'7': true},
};

/// Descriptor for `RunJobRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List runJobRequestDescriptor = $convert.base64Decode(
    'Cg1SdW5Kb2JSZXF1ZXN0EhUKBmpvYl9pZBgBIAEoCVIFam9iSWQSGQoIcmVwb19rZXkYAiABKA'
    'lSB3JlcG9LZXkSFgoGY29tbWl0GAMgASgJUgZjb21taXQSJwoPYnVuZGxlX3VwbG9hZGVkGAQg'
    'ASgIUg5idW5kbGVVcGxvYWRlZBIjCg1hcnRpZmFjdF9uYW1lGAUgASgJUgxhcnRpZmFjdE5hbW'
    'USGAoHY29tbWFuZBgGIAMoCVIHY29tbWFuZBI5CgNlbnYYByADKAsyJy5wcm90ZWN0b3JhdGUu'
    'djEuUnVuSm9iUmVxdWVzdC5FbnZFbnRyeVIDZW52EjMKB3RpbWVvdXQYCCABKAsyGS5nb29nbG'
    'UucHJvdG9idWYuRHVyYXRpb25SB3RpbWVvdXQSJQoOYXJ0aWZhY3RfZ2xvYnMYCSADKAlSDWFy'
    'dGlmYWN0R2xvYnMSKQoQYXJ0aWZhY3RfYXJjaGl2ZRgKIAEoCFIPYXJ0aWZhY3RBcmNoaXZlEh'
    'gKB3dvcmtkaXIYCyABKAlSB3dvcmtkaXIaNgoIRW52RW50cnkSEAoDa2V5GAEgASgJUgNrZXkS'
    'FAoFdmFsdWUYAiABKAlSBXZhbHVlOgI4AQ==');

@$core.Deprecated('Use jobStartedDescriptor instead')
const JobStarted$json = {
  '1': 'JobStarted',
  '2': [
    {'1': 'workspace_reused', '3': 1, '4': 1, '5': 8, '10': 'workspaceReused'},
  ],
};

/// Descriptor for `JobStarted`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List jobStartedDescriptor = $convert.base64Decode(
    'CgpKb2JTdGFydGVkEikKEHdvcmtzcGFjZV9yZXVzZWQYASABKAhSD3dvcmtzcGFjZVJldXNlZA'
    '==');

@$core.Deprecated('Use jobOutputDescriptor instead')
const JobOutput$json = {
  '1': 'JobOutput',
  '2': [
    {'1': 'data', '3': 1, '4': 1, '5': 12, '10': 'data'},
  ],
};

/// Descriptor for `JobOutput`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List jobOutputDescriptor =
    $convert.base64Decode('CglKb2JPdXRwdXQSEgoEZGF0YRgBIAEoDFIEZGF0YQ==');

@$core.Deprecated('Use jobArtifactChunkDescriptor instead')
const JobArtifactChunk$json = {
  '1': 'JobArtifactChunk',
  '2': [
    {'1': 'path', '3': 1, '4': 1, '5': 9, '10': 'path'},
    {'1': 'data', '3': 2, '4': 1, '5': 12, '10': 'data'},
    {'1': 'last', '3': 3, '4': 1, '5': 8, '10': 'last'},
    {'1': 'mode', '3': 4, '4': 1, '5': 13, '10': 'mode'},
  ],
};

/// Descriptor for `JobArtifactChunk`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List jobArtifactChunkDescriptor = $convert.base64Decode(
    'ChBKb2JBcnRpZmFjdENodW5rEhIKBHBhdGgYASABKAlSBHBhdGgSEgoEZGF0YRgCIAEoDFIEZG'
    'F0YRISCgRsYXN0GAMgASgIUgRsYXN0EhIKBG1vZGUYBCABKA1SBG1vZGU=');

@$core.Deprecated('Use jobFinishedDescriptor instead')
const JobFinished$json = {
  '1': 'JobFinished',
  '2': [
    {
      '1': 'end',
      '3': 1,
      '4': 1,
      '5': 14,
      '6': '.protectorate.v1.JobEnd',
      '10': 'end'
    },
    {'1': 'exit_code', '3': 2, '4': 1, '5': 5, '10': 'exitCode'},
    {'1': 'error', '3': 3, '4': 1, '5': 9, '10': 'error'},
    {
      '1': 'run_time',
      '3': 4,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Duration',
      '10': 'runTime'
    },
  ],
};

/// Descriptor for `JobFinished`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List jobFinishedDescriptor = $convert.base64Decode(
    'CgtKb2JGaW5pc2hlZBIpCgNlbmQYASABKA4yFy5wcm90ZWN0b3JhdGUudjEuSm9iRW5kUgNlbm'
    'QSGwoJZXhpdF9jb2RlGAIgASgFUghleGl0Q29kZRIUCgVlcnJvchgDIAEoCVIFZXJyb3ISNAoI'
    'cnVuX3RpbWUYBCABKAsyGS5nb29nbGUucHJvdG9idWYuRHVyYXRpb25SB3J1blRpbWU=');

@$core.Deprecated('Use jobEventDescriptor instead')
const JobEvent$json = {
  '1': 'JobEvent',
  '2': [
    {
      '1': 'started',
      '3': 1,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.JobStarted',
      '9': 0,
      '10': 'started'
    },
    {
      '1': 'output',
      '3': 2,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.JobOutput',
      '9': 0,
      '10': 'output'
    },
    {
      '1': 'artifact',
      '3': 3,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.JobArtifactChunk',
      '9': 0,
      '10': 'artifact'
    },
    {
      '1': 'finished',
      '3': 4,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.JobFinished',
      '9': 0,
      '10': 'finished'
    },
  ],
  '8': [
    {'1': 'event'},
  ],
};

/// Descriptor for `JobEvent`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List jobEventDescriptor = $convert.base64Decode(
    'CghKb2JFdmVudBI3CgdzdGFydGVkGAEgASgLMhsucHJvdGVjdG9yYXRlLnYxLkpvYlN0YXJ0ZW'
    'RIAFIHc3RhcnRlZBI0CgZvdXRwdXQYAiABKAsyGi5wcm90ZWN0b3JhdGUudjEuSm9iT3V0cHV0'
    'SABSBm91dHB1dBI/CghhcnRpZmFjdBgDIAEoCzIhLnByb3RlY3RvcmF0ZS52MS5Kb2JBcnRpZm'
    'FjdENodW5rSABSCGFydGlmYWN0EjoKCGZpbmlzaGVkGAQgASgLMhwucHJvdGVjdG9yYXRlLnYx'
    'LkpvYkZpbmlzaGVkSABSCGZpbmlzaGVkQgcKBWV2ZW50');

@$core.Deprecated('Use nodeAgentServiceCancelJobRequestDescriptor instead')
const NodeAgentServiceCancelJobRequest$json = {
  '1': 'NodeAgentServiceCancelJobRequest',
  '2': [
    {'1': 'job_id', '3': 1, '4': 1, '5': 9, '10': 'jobId'},
  ],
};

/// Descriptor for `NodeAgentServiceCancelJobRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List nodeAgentServiceCancelJobRequestDescriptor =
    $convert.base64Decode(
        'CiBOb2RlQWdlbnRTZXJ2aWNlQ2FuY2VsSm9iUmVxdWVzdBIVCgZqb2JfaWQYASABKAlSBWpvYk'
        'lk');

@$core.Deprecated('Use nodeAgentServiceCancelJobResponseDescriptor instead')
const NodeAgentServiceCancelJobResponse$json = {
  '1': 'NodeAgentServiceCancelJobResponse',
  '2': [
    {'1': 'found', '3': 1, '4': 1, '5': 8, '10': 'found'},
  ],
};

/// Descriptor for `NodeAgentServiceCancelJobResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List nodeAgentServiceCancelJobResponseDescriptor =
    $convert.base64Decode(
        'CiFOb2RlQWdlbnRTZXJ2aWNlQ2FuY2VsSm9iUmVzcG9uc2USFAoFZm91bmQYASABKAhSBWZvdW'
        '5k');

const $core.Map<$core.String, $core.dynamic> JobServiceBase$json = {
  '1': 'JobService',
  '2': [
    {
      '1': 'SubmitJob',
      '2': '.protectorate.v1.SubmitJobRequest',
      '3': '.protectorate.v1.Job'
    },
    {
      '1': 'GetJob',
      '2': '.protectorate.v1.GetJobRequest',
      '3': '.protectorate.v1.Job',
      '4': {'34': 1},
    },
    {
      '1': 'ListJobs',
      '2': '.protectorate.v1.ListJobsRequest',
      '3': '.protectorate.v1.ListJobsResponse',
      '4': {'34': 1},
    },
    {
      '1': 'CancelJob',
      '2': '.protectorate.v1.CancelJobRequest',
      '3': '.protectorate.v1.Job'
    },
    {
      '1': 'WatchJob',
      '2': '.protectorate.v1.WatchJobRequest',
      '3': '.protectorate.v1.WatchJobResponse',
      '4': {'34': 1},
      '6': true,
    },
  ],
};

@$core.Deprecated('Use jobServiceDescriptor instead')
const $core.Map<$core.String, $core.Map<$core.String, $core.dynamic>>
    JobServiceBase$messageJson = {
  '.protectorate.v1.SubmitJobRequest': SubmitJobRequest$json,
  '.protectorate.v1.JobTarget': JobTarget$json,
  '.protectorate.v1.NodeSelector': $0.NodeSelector$json,
  '.protectorate.v1.JobSource': JobSource$json,
  '.protectorate.v1.GitSource': GitSource$json,
  '.protectorate.v1.ArtifactSource': ArtifactSource$json,
  '.protectorate.v1.SubmitJobRequest.EnvEntry': SubmitJobRequest_EnvEntry$json,
  '.google.protobuf.Duration': $1.Duration$json,
  '.protectorate.v1.Job': Job$json,
  '.google.protobuf.Timestamp': $2.Timestamp$json,
  '.protectorate.v1.JobArtifact': JobArtifact$json,
  '.protectorate.v1.GetJobRequest': GetJobRequest$json,
  '.protectorate.v1.ListJobsRequest': ListJobsRequest$json,
  '.protectorate.v1.ListJobsResponse': ListJobsResponse$json,
  '.protectorate.v1.CancelJobRequest': CancelJobRequest$json,
  '.protectorate.v1.WatchJobRequest': WatchJobRequest$json,
  '.protectorate.v1.WatchJobResponse': WatchJobResponse$json,
  '.protectorate.v1.JobLog': JobLog$json,
};

/// Descriptor for `JobService`. Decode as a `google.protobuf.ServiceDescriptorProto`.
final $typed_data.Uint8List jobServiceDescriptor = $convert.base64Decode(
    'CgpKb2JTZXJ2aWNlEkQKCVN1Ym1pdEpvYhIhLnByb3RlY3RvcmF0ZS52MS5TdWJtaXRKb2JSZX'
    'F1ZXN0GhQucHJvdGVjdG9yYXRlLnYxLkpvYhJDCgZHZXRKb2ISHi5wcm90ZWN0b3JhdGUudjEu'
    'R2V0Sm9iUmVxdWVzdBoULnByb3RlY3RvcmF0ZS52MS5Kb2IiA5ACARJUCghMaXN0Sm9icxIgLn'
    'Byb3RlY3RvcmF0ZS52MS5MaXN0Sm9ic1JlcXVlc3QaIS5wcm90ZWN0b3JhdGUudjEuTGlzdEpv'
    'YnNSZXNwb25zZSIDkAIBEkQKCUNhbmNlbEpvYhIhLnByb3RlY3RvcmF0ZS52MS5DYW5jZWxKb2'
    'JSZXF1ZXN0GhQucHJvdGVjdG9yYXRlLnYxLkpvYhJWCghXYXRjaEpvYhIgLnByb3RlY3RvcmF0'
    'ZS52MS5XYXRjaEpvYlJlcXVlc3QaIS5wcm90ZWN0b3JhdGUudjEuV2F0Y2hKb2JSZXNwb25zZS'
    'IDkAIBMAE=');
