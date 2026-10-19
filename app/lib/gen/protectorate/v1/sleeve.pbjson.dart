// This is a generated file - do not edit.
//
// Generated from protectorate/v1/sleeve.proto.

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

import 'chat.pbjson.dart' as $1;

@$core.Deprecated('Use harnessDescriptor instead')
const Harness$json = {
  '1': 'Harness',
  '2': [
    {'1': 'HARNESS_UNSPECIFIED', '2': 0},
    {'1': 'HARNESS_CLAUDE', '2': 1},
    {'1': 'HARNESS_CODEX', '2': 2},
    {'1': 'HARNESS_GROK', '2': 3},
    {'1': 'HARNESS_PI', '2': 4},
    {'1': 'HARNESS_SHELL', '2': 5},
  ],
};

/// Descriptor for `Harness`. Decode as a `google.protobuf.EnumDescriptorProto`.
final $typed_data.Uint8List harnessDescriptor = $convert.base64Decode(
    'CgdIYXJuZXNzEhcKE0hBUk5FU1NfVU5TUEVDSUZJRUQQABISCg5IQVJORVNTX0NMQVVERRABEh'
    'EKDUhBUk5FU1NfQ09ERVgQAhIQCgxIQVJORVNTX0dST0sQAxIOCgpIQVJORVNTX1BJEAQSEQoN'
    'SEFSTkVTU19TSEVMTBAF');

@$core.Deprecated('Use sleeveStateDescriptor instead')
const SleeveState$json = {
  '1': 'SleeveState',
  '2': [
    {'1': 'SLEEVE_STATE_UNSPECIFIED', '2': 0},
    {'1': 'SLEEVE_STATE_RUNNING', '2': 1},
    {'1': 'SLEEVE_STATE_EXITED', '2': 2},
  ],
};

/// Descriptor for `SleeveState`. Decode as a `google.protobuf.EnumDescriptorProto`.
final $typed_data.Uint8List sleeveStateDescriptor = $convert.base64Decode(
    'CgtTbGVldmVTdGF0ZRIcChhTTEVFVkVfU1RBVEVfVU5TUEVDSUZJRUQQABIYChRTTEVFVkVfU1'
    'RBVEVfUlVOTklORxABEhcKE1NMRUVWRV9TVEFURV9FWElURUQQAg==');

@$core.Deprecated('Use sleeveLimitsDescriptor instead')
const SleeveLimits$json = {
  '1': 'SleeveLimits',
  '2': [
    {'1': 'cpu_percent', '3': 1, '4': 1, '5': 13, '10': 'cpuPercent'},
    {'1': 'memory_bytes', '3': 2, '4': 1, '5': 4, '10': 'memoryBytes'},
  ],
};

/// Descriptor for `SleeveLimits`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List sleeveLimitsDescriptor = $convert.base64Decode(
    'CgxTbGVldmVMaW1pdHMSHwoLY3B1X3BlcmNlbnQYASABKA1SCmNwdVBlcmNlbnQSIQoMbWVtb3'
    'J5X2J5dGVzGAIgASgEUgttZW1vcnlCeXRlcw==');

@$core.Deprecated('Use sleeveUsageDescriptor instead')
const SleeveUsage$json = {
  '1': 'SleeveUsage',
  '2': [
    {'1': 'cpu_percent', '3': 1, '4': 1, '5': 1, '10': 'cpuPercent'},
    {'1': 'cpu_seconds', '3': 2, '4': 1, '5': 1, '10': 'cpuSeconds'},
    {
      '1': 'memory_current_bytes',
      '3': 3,
      '4': 1,
      '5': 4,
      '10': 'memoryCurrentBytes'
    },
    {'1': 'memory_peak_bytes', '3': 4, '4': 1, '5': 4, '10': 'memoryPeakBytes'},
    {'1': 'pids', '3': 5, '4': 1, '5': 13, '10': 'pids'},
    {'1': 'oom_kills', '3': 6, '4': 1, '5': 4, '10': 'oomKills'},
    {
      '1': 'workspace_disk_bytes',
      '3': 7,
      '4': 1,
      '5': 4,
      '10': 'workspaceDiskBytes'
    },
    {
      '1': 'sampled_at',
      '3': 8,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Timestamp',
      '10': 'sampledAt'
    },
    {
      '1': 'disk_sampled_at',
      '3': 9,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Timestamp',
      '10': 'diskSampledAt'
    },
  ],
};

/// Descriptor for `SleeveUsage`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List sleeveUsageDescriptor = $convert.base64Decode(
    'CgtTbGVldmVVc2FnZRIfCgtjcHVfcGVyY2VudBgBIAEoAVIKY3B1UGVyY2VudBIfCgtjcHVfc2'
    'Vjb25kcxgCIAEoAVIKY3B1U2Vjb25kcxIwChRtZW1vcnlfY3VycmVudF9ieXRlcxgDIAEoBFIS'
    'bWVtb3J5Q3VycmVudEJ5dGVzEioKEW1lbW9yeV9wZWFrX2J5dGVzGAQgASgEUg9tZW1vcnlQZW'
    'FrQnl0ZXMSEgoEcGlkcxgFIAEoDVIEcGlkcxIbCglvb21fa2lsbHMYBiABKARSCG9vbUtpbGxz'
    'EjAKFHdvcmtzcGFjZV9kaXNrX2J5dGVzGAcgASgEUhJ3b3Jrc3BhY2VEaXNrQnl0ZXMSOQoKc2'
    'FtcGxlZF9hdBgIIAEoCzIaLmdvb2dsZS5wcm90b2J1Zi5UaW1lc3RhbXBSCXNhbXBsZWRBdBJC'
    'Cg9kaXNrX3NhbXBsZWRfYXQYCSABKAsyGi5nb29nbGUucHJvdG9idWYuVGltZXN0YW1wUg1kaX'
    'NrU2FtcGxlZEF0');

@$core.Deprecated('Use sleeveDescriptor instead')
const Sleeve$json = {
  '1': 'Sleeve',
  '2': [
    {'1': 'name', '3': 1, '4': 1, '5': 9, '10': 'name'},
    {
      '1': 'harness',
      '3': 2,
      '4': 1,
      '5': 14,
      '6': '.protectorate.v1.Harness',
      '10': 'harness'
    },
    {'1': 'workspace', '3': 3, '4': 1, '5': 9, '10': 'workspace'},
    {'1': 'session', '3': 4, '4': 1, '5': 9, '10': 'session'},
    {
      '1': 'created_at',
      '3': 5,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Timestamp',
      '10': 'createdAt'
    },
    {
      '1': 'state',
      '3': 6,
      '4': 1,
      '5': 14,
      '6': '.protectorate.v1.SleeveState',
      '10': 'state'
    },
    {
      '1': 'limits',
      '3': 7,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.SleeveLimits',
      '10': 'limits'
    },
    {
      '1': 'usage',
      '3': 8,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.SleeveUsage',
      '10': 'usage'
    },
    {'1': 'args', '3': 9, '4': 3, '5': 9, '10': 'args'},
    {
      '1': 'exited_at',
      '3': 10,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Timestamp',
      '10': 'exitedAt'
    },
    {
      '1': 'exit_code',
      '3': 11,
      '4': 1,
      '5': 5,
      '9': 0,
      '10': 'exitCode',
      '17': true
    },
    {'1': 'exit_signal', '3': 12, '4': 1, '5': 5, '10': 'exitSignal'},
    {'1': 'oom_killed', '3': 13, '4': 1, '5': 8, '10': 'oomKilled'},
    {
      '1': 'mode',
      '3': 14,
      '4': 1,
      '5': 14,
      '6': '.protectorate.v1.SleeveMode',
      '10': 'mode'
    },
    {
      '1': 'chat',
      '3': 15,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.ChatStatus',
      '10': 'chat'
    },
    {'1': 'supervised', '3': 16, '4': 1, '5': 8, '10': 'supervised'},
  ],
  '8': [
    {'1': '_exit_code'},
  ],
};

/// Descriptor for `Sleeve`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List sleeveDescriptor = $convert.base64Decode(
    'CgZTbGVldmUSEgoEbmFtZRgBIAEoCVIEbmFtZRIyCgdoYXJuZXNzGAIgASgOMhgucHJvdGVjdG'
    '9yYXRlLnYxLkhhcm5lc3NSB2hhcm5lc3MSHAoJd29ya3NwYWNlGAMgASgJUgl3b3Jrc3BhY2US'
    'GAoHc2Vzc2lvbhgEIAEoCVIHc2Vzc2lvbhI5CgpjcmVhdGVkX2F0GAUgASgLMhouZ29vZ2xlLn'
    'Byb3RvYnVmLlRpbWVzdGFtcFIJY3JlYXRlZEF0EjIKBXN0YXRlGAYgASgOMhwucHJvdGVjdG9y'
    'YXRlLnYxLlNsZWV2ZVN0YXRlUgVzdGF0ZRI1CgZsaW1pdHMYByABKAsyHS5wcm90ZWN0b3JhdG'
    'UudjEuU2xlZXZlTGltaXRzUgZsaW1pdHMSMgoFdXNhZ2UYCCABKAsyHC5wcm90ZWN0b3JhdGUu'
    'djEuU2xlZXZlVXNhZ2VSBXVzYWdlEhIKBGFyZ3MYCSADKAlSBGFyZ3MSNwoJZXhpdGVkX2F0GA'
    'ogASgLMhouZ29vZ2xlLnByb3RvYnVmLlRpbWVzdGFtcFIIZXhpdGVkQXQSIAoJZXhpdF9jb2Rl'
    'GAsgASgFSABSCGV4aXRDb2RliAEBEh8KC2V4aXRfc2lnbmFsGAwgASgFUgpleGl0U2lnbmFsEh'
    '0KCm9vbV9raWxsZWQYDSABKAhSCW9vbUtpbGxlZBIvCgRtb2RlGA4gASgOMhsucHJvdGVjdG9y'
    'YXRlLnYxLlNsZWV2ZU1vZGVSBG1vZGUSLwoEY2hhdBgPIAEoCzIbLnByb3RlY3RvcmF0ZS52MS'
    '5DaGF0U3RhdHVzUgRjaGF0Eh4KCnN1cGVydmlzZWQYECABKAhSCnN1cGVydmlzZWRCDAoKX2V4'
    'aXRfY29kZQ==');

@$core.Deprecated('Use spawnSleeveRequestDescriptor instead')
const SpawnSleeveRequest$json = {
  '1': 'SpawnSleeveRequest',
  '2': [
    {'1': 'name', '3': 1, '4': 1, '5': 9, '10': 'name'},
    {'1': 'workspace', '3': 2, '4': 1, '5': 9, '10': 'workspace'},
    {
      '1': 'harness',
      '3': 3,
      '4': 1,
      '5': 14,
      '6': '.protectorate.v1.Harness',
      '10': 'harness'
    },
    {'1': 'args', '3': 4, '4': 3, '5': 9, '10': 'args'},
    {
      '1': 'limits',
      '3': 5,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.SleeveLimits',
      '10': 'limits'
    },
    {
      '1': 'env',
      '3': 6,
      '4': 3,
      '5': 11,
      '6': '.protectorate.v1.SpawnSleeveRequest.EnvEntry',
      '10': 'env'
    },
    {
      '1': 'mode',
      '3': 7,
      '4': 1,
      '5': 14,
      '6': '.protectorate.v1.SleeveMode',
      '10': 'mode'
    },
    {'1': 'supervised', '3': 8, '4': 1, '5': 8, '10': 'supervised'},
  ],
  '3': [SpawnSleeveRequest_EnvEntry$json],
};

@$core.Deprecated('Use spawnSleeveRequestDescriptor instead')
const SpawnSleeveRequest_EnvEntry$json = {
  '1': 'EnvEntry',
  '2': [
    {'1': 'key', '3': 1, '4': 1, '5': 9, '10': 'key'},
    {'1': 'value', '3': 2, '4': 1, '5': 9, '10': 'value'},
  ],
  '7': {'7': true},
};

/// Descriptor for `SpawnSleeveRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List spawnSleeveRequestDescriptor = $convert.base64Decode(
    'ChJTcGF3blNsZWV2ZVJlcXVlc3QSEgoEbmFtZRgBIAEoCVIEbmFtZRIcCgl3b3Jrc3BhY2UYAi'
    'ABKAlSCXdvcmtzcGFjZRIyCgdoYXJuZXNzGAMgASgOMhgucHJvdGVjdG9yYXRlLnYxLkhhcm5l'
    'c3NSB2hhcm5lc3MSEgoEYXJncxgEIAMoCVIEYXJncxI1CgZsaW1pdHMYBSABKAsyHS5wcm90ZW'
    'N0b3JhdGUudjEuU2xlZXZlTGltaXRzUgZsaW1pdHMSPgoDZW52GAYgAygLMiwucHJvdGVjdG9y'
    'YXRlLnYxLlNwYXduU2xlZXZlUmVxdWVzdC5FbnZFbnRyeVIDZW52Ei8KBG1vZGUYByABKA4yGy'
    '5wcm90ZWN0b3JhdGUudjEuU2xlZXZlTW9kZVIEbW9kZRIeCgpzdXBlcnZpc2VkGAggASgIUgpz'
    'dXBlcnZpc2VkGjYKCEVudkVudHJ5EhAKA2tleRgBIAEoCVIDa2V5EhQKBXZhbHVlGAIgASgJUg'
    'V2YWx1ZToCOAE=');

@$core.Deprecated('Use spawnSleeveResponseDescriptor instead')
const SpawnSleeveResponse$json = {
  '1': 'SpawnSleeveResponse',
  '2': [
    {
      '1': 'sleeve',
      '3': 1,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.Sleeve',
      '10': 'sleeve'
    },
  ],
};

/// Descriptor for `SpawnSleeveResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List spawnSleeveResponseDescriptor = $convert.base64Decode(
    'ChNTcGF3blNsZWV2ZVJlc3BvbnNlEi8KBnNsZWV2ZRgBIAEoCzIXLnByb3RlY3RvcmF0ZS52MS'
    '5TbGVldmVSBnNsZWV2ZQ==');

@$core.Deprecated('Use listSleevesRequestDescriptor instead')
const ListSleevesRequest$json = {
  '1': 'ListSleevesRequest',
};

/// Descriptor for `ListSleevesRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List listSleevesRequestDescriptor =
    $convert.base64Decode('ChJMaXN0U2xlZXZlc1JlcXVlc3Q=');

@$core.Deprecated('Use listSleevesResponseDescriptor instead')
const ListSleevesResponse$json = {
  '1': 'ListSleevesResponse',
  '2': [
    {
      '1': 'sleeves',
      '3': 1,
      '4': 3,
      '5': 11,
      '6': '.protectorate.v1.Sleeve',
      '10': 'sleeves'
    },
  ],
};

/// Descriptor for `ListSleevesResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List listSleevesResponseDescriptor = $convert.base64Decode(
    'ChNMaXN0U2xlZXZlc1Jlc3BvbnNlEjEKB3NsZWV2ZXMYASADKAsyFy5wcm90ZWN0b3JhdGUudj'
    'EuU2xlZXZlUgdzbGVldmVz');

@$core.Deprecated('Use getSleeveRequestDescriptor instead')
const GetSleeveRequest$json = {
  '1': 'GetSleeveRequest',
  '2': [
    {'1': 'name', '3': 1, '4': 1, '5': 9, '10': 'name'},
  ],
};

/// Descriptor for `GetSleeveRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List getSleeveRequestDescriptor = $convert
    .base64Decode('ChBHZXRTbGVldmVSZXF1ZXN0EhIKBG5hbWUYASABKAlSBG5hbWU=');

@$core.Deprecated('Use getSleeveResponseDescriptor instead')
const GetSleeveResponse$json = {
  '1': 'GetSleeveResponse',
  '2': [
    {
      '1': 'sleeve',
      '3': 1,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.Sleeve',
      '10': 'sleeve'
    },
  ],
};

/// Descriptor for `GetSleeveResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List getSleeveResponseDescriptor = $convert.base64Decode(
    'ChFHZXRTbGVldmVSZXNwb25zZRIvCgZzbGVldmUYASABKAsyFy5wcm90ZWN0b3JhdGUudjEuU2'
    'xlZXZlUgZzbGVldmU=');

@$core.Deprecated('Use killSleeveRequestDescriptor instead')
const KillSleeveRequest$json = {
  '1': 'KillSleeveRequest',
  '2': [
    {'1': 'name', '3': 1, '4': 1, '5': 9, '10': 'name'},
  ],
};

/// Descriptor for `KillSleeveRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List killSleeveRequestDescriptor = $convert
    .base64Decode('ChFLaWxsU2xlZXZlUmVxdWVzdBISCgRuYW1lGAEgASgJUgRuYW1l');

@$core.Deprecated('Use killSleeveResponseDescriptor instead')
const KillSleeveResponse$json = {
  '1': 'KillSleeveResponse',
};

/// Descriptor for `KillSleeveResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List killSleeveResponseDescriptor =
    $convert.base64Decode('ChJLaWxsU2xlZXZlUmVzcG9uc2U=');

const $core.Map<$core.String, $core.dynamic> SleeveServiceBase$json = {
  '1': 'SleeveService',
  '2': [
    {
      '1': 'SpawnSleeve',
      '2': '.protectorate.v1.SpawnSleeveRequest',
      '3': '.protectorate.v1.SpawnSleeveResponse'
    },
    {
      '1': 'ListSleeves',
      '2': '.protectorate.v1.ListSleevesRequest',
      '3': '.protectorate.v1.ListSleevesResponse',
      '4': {'34': 1},
    },
    {
      '1': 'GetSleeve',
      '2': '.protectorate.v1.GetSleeveRequest',
      '3': '.protectorate.v1.GetSleeveResponse',
      '4': {'34': 1},
    },
    {
      '1': 'KillSleeve',
      '2': '.protectorate.v1.KillSleeveRequest',
      '3': '.protectorate.v1.KillSleeveResponse',
      '4': {}
    },
  ],
};

@$core.Deprecated('Use sleeveServiceDescriptor instead')
const $core.Map<$core.String, $core.Map<$core.String, $core.dynamic>>
    SleeveServiceBase$messageJson = {
  '.protectorate.v1.SpawnSleeveRequest': SpawnSleeveRequest$json,
  '.protectorate.v1.SleeveLimits': SleeveLimits$json,
  '.protectorate.v1.SpawnSleeveRequest.EnvEntry':
      SpawnSleeveRequest_EnvEntry$json,
  '.protectorate.v1.SpawnSleeveResponse': SpawnSleeveResponse$json,
  '.protectorate.v1.Sleeve': Sleeve$json,
  '.google.protobuf.Timestamp': $0.Timestamp$json,
  '.protectorate.v1.SleeveUsage': SleeveUsage$json,
  '.protectorate.v1.ChatStatus': $1.ChatStatus$json,
  '.protectorate.v1.ListSleevesRequest': ListSleevesRequest$json,
  '.protectorate.v1.ListSleevesResponse': ListSleevesResponse$json,
  '.protectorate.v1.GetSleeveRequest': GetSleeveRequest$json,
  '.protectorate.v1.GetSleeveResponse': GetSleeveResponse$json,
  '.protectorate.v1.KillSleeveRequest': KillSleeveRequest$json,
  '.protectorate.v1.KillSleeveResponse': KillSleeveResponse$json,
};

/// Descriptor for `SleeveService`. Decode as a `google.protobuf.ServiceDescriptorProto`.
final $typed_data.Uint8List sleeveServiceDescriptor = $convert.base64Decode(
    'Cg1TbGVldmVTZXJ2aWNlElgKC1NwYXduU2xlZXZlEiMucHJvdGVjdG9yYXRlLnYxLlNwYXduU2'
    'xlZXZlUmVxdWVzdBokLnByb3RlY3RvcmF0ZS52MS5TcGF3blNsZWV2ZVJlc3BvbnNlEl0KC0xp'
    'c3RTbGVldmVzEiMucHJvdGVjdG9yYXRlLnYxLkxpc3RTbGVldmVzUmVxdWVzdBokLnByb3RlY3'
    'RvcmF0ZS52MS5MaXN0U2xlZXZlc1Jlc3BvbnNlIgOQAgESVwoJR2V0U2xlZXZlEiEucHJvdGVj'
    'dG9yYXRlLnYxLkdldFNsZWV2ZVJlcXVlc3QaIi5wcm90ZWN0b3JhdGUudjEuR2V0U2xlZXZlUm'
    'VzcG9uc2UiA5ACARJbCgpLaWxsU2xlZXZlEiIucHJvdGVjdG9yYXRlLnYxLktpbGxTbGVldmVS'
    'ZXF1ZXN0GiMucHJvdGVjdG9yYXRlLnYxLktpbGxTbGVldmVSZXNwb25zZSIEiLUYAQ==');
