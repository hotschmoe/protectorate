// This is a generated file - do not edit.
//
// Generated from protectorate/v1/quota.proto.

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
    as $0;

@$core.Deprecated('Use freshnessDescriptor instead')
const Freshness$json = {
  '1': 'Freshness',
  '2': [
    {'1': 'FRESHNESS_UNSPECIFIED', '2': 0},
    {'1': 'FRESHNESS_FRESH', '2': 1},
    {'1': 'FRESHNESS_STALE', '2': 2},
    {'1': 'FRESHNESS_UNKNOWN', '2': 3},
  ],
};

/// Descriptor for `Freshness`. Decode as a `google.protobuf.EnumDescriptorProto`.
final $typed_data.Uint8List freshnessDescriptor = $convert.base64Decode(
    'CglGcmVzaG5lc3MSGQoVRlJFU0hORVNTX1VOU1BFQ0lGSUVEEAASEwoPRlJFU0hORVNTX0ZSRV'
    'NIEAESEwoPRlJFU0hORVNTX1NUQUxFEAISFQoRRlJFU0hORVNTX1VOS05PV04QAw==');

@$core.Deprecated('Use taskSizeDescriptor instead')
const TaskSize$json = {
  '1': 'TaskSize',
  '2': [
    {'1': 'TASK_SIZE_UNSPECIFIED', '2': 0},
    {'1': 'TASK_SIZE_SMALL', '2': 1},
    {'1': 'TASK_SIZE_MEDIUM', '2': 2},
    {'1': 'TASK_SIZE_LARGE', '2': 3},
  ],
};

/// Descriptor for `TaskSize`. Decode as a `google.protobuf.EnumDescriptorProto`.
final $typed_data.Uint8List taskSizeDescriptor = $convert.base64Decode(
    'CghUYXNrU2l6ZRIZChVUQVNLX1NJWkVfVU5TUEVDSUZJRUQQABITCg9UQVNLX1NJWkVfU01BTE'
    'wQARIUChBUQVNLX1NJWkVfTUVESVVNEAISEwoPVEFTS19TSVpFX0xBUkdFEAM=');

@$core.Deprecated('Use getQuotaRequestDescriptor instead')
const GetQuotaRequest$json = {
  '1': 'GetQuotaRequest',
  '2': [
    {'1': 'refresh', '3': 1, '4': 1, '5': 8, '10': 'refresh'},
  ],
};

/// Descriptor for `GetQuotaRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List getQuotaRequestDescriptor = $convert.base64Decode(
    'Cg9HZXRRdW90YVJlcXVlc3QSGAoHcmVmcmVzaBgBIAEoCFIHcmVmcmVzaA==');

@$core.Deprecated('Use getQuotaResponseDescriptor instead')
const GetQuotaResponse$json = {
  '1': 'GetQuotaResponse',
  '2': [
    {
      '1': 'quotas',
      '3': 1,
      '4': 3,
      '5': 11,
      '6': '.protectorate.v1.ProviderQuota',
      '10': 'quotas'
    },
  ],
};

/// Descriptor for `GetQuotaResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List getQuotaResponseDescriptor = $convert.base64Decode(
    'ChBHZXRRdW90YVJlc3BvbnNlEjYKBnF1b3RhcxgBIAMoCzIeLnByb3RlY3RvcmF0ZS52MS5Qcm'
    '92aWRlclF1b3RhUgZxdW90YXM=');

@$core.Deprecated('Use providerQuotaDescriptor instead')
const ProviderQuota$json = {
  '1': 'ProviderQuota',
  '2': [
    {'1': 'provider', '3': 1, '4': 1, '5': 9, '10': 'provider'},
    {'1': 'plan', '3': 2, '4': 1, '5': 9, '10': 'plan'},
    {'1': 'source', '3': 3, '4': 1, '5': 9, '10': 'source'},
    {
      '1': 'freshness',
      '3': 4,
      '4': 1,
      '5': 14,
      '6': '.protectorate.v1.Freshness',
      '10': 'freshness'
    },
    {
      '1': 'fetched_at',
      '3': 5,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Timestamp',
      '10': 'fetchedAt'
    },
    {
      '1': 'age',
      '3': 6,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Duration',
      '10': 'age'
    },
    {'1': 'error', '3': 7, '4': 1, '5': 9, '10': 'error'},
    {
      '1': 'checked_at',
      '3': 8,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Timestamp',
      '10': 'checkedAt'
    },
    {
      '1': 'windows',
      '3': 9,
      '4': 3,
      '5': 11,
      '6': '.protectorate.v1.QuotaWindow',
      '10': 'windows'
    },
    {
      '1': 'resets',
      '3': 10,
      '4': 3,
      '5': 11,
      '6': '.protectorate.v1.ResetCredit',
      '10': 'resets'
    },
    {'1': 'models', '3': 11, '4': 3, '5': 9, '10': 'models'},
    {
      '1': 'latency',
      '3': 12,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Duration',
      '10': 'latency'
    },
  ],
};

/// Descriptor for `ProviderQuota`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List providerQuotaDescriptor = $convert.base64Decode(
    'Cg1Qcm92aWRlclF1b3RhEhoKCHByb3ZpZGVyGAEgASgJUghwcm92aWRlchISCgRwbGFuGAIgAS'
    'gJUgRwbGFuEhYKBnNvdXJjZRgDIAEoCVIGc291cmNlEjgKCWZyZXNobmVzcxgEIAEoDjIaLnBy'
    'b3RlY3RvcmF0ZS52MS5GcmVzaG5lc3NSCWZyZXNobmVzcxI5CgpmZXRjaGVkX2F0GAUgASgLMh'
    'ouZ29vZ2xlLnByb3RvYnVmLlRpbWVzdGFtcFIJZmV0Y2hlZEF0EisKA2FnZRgGIAEoCzIZLmdv'
    'b2dsZS5wcm90b2J1Zi5EdXJhdGlvblIDYWdlEhQKBWVycm9yGAcgASgJUgVlcnJvchI5CgpjaG'
    'Vja2VkX2F0GAggASgLMhouZ29vZ2xlLnByb3RvYnVmLlRpbWVzdGFtcFIJY2hlY2tlZEF0EjYK'
    'B3dpbmRvd3MYCSADKAsyHC5wcm90ZWN0b3JhdGUudjEuUXVvdGFXaW5kb3dSB3dpbmRvd3MSNA'
    'oGcmVzZXRzGAogAygLMhwucHJvdGVjdG9yYXRlLnYxLlJlc2V0Q3JlZGl0UgZyZXNldHMSFgoG'
    'bW9kZWxzGAsgAygJUgZtb2RlbHMSMwoHbGF0ZW5jeRgMIAEoCzIZLmdvb2dsZS5wcm90b2J1Zi'
    '5EdXJhdGlvblIHbGF0ZW5jeQ==');

@$core.Deprecated('Use quotaWindowDescriptor instead')
const QuotaWindow$json = {
  '1': 'QuotaWindow',
  '2': [
    {'1': 'name', '3': 1, '4': 1, '5': 9, '10': 'name'},
    {'1': 'used_percent', '3': 2, '4': 1, '5': 1, '10': 'usedPercent'},
    {
      '1': 'resets_at',
      '3': 3,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Timestamp',
      '10': 'resetsAt'
    },
    {'1': 'window_seconds', '3': 4, '4': 1, '5': 3, '10': 'windowSeconds'},
    {
      '1': 'estimated_unused_percent_at_reset',
      '3': 5,
      '4': 1,
      '5': 1,
      '9': 0,
      '10': 'estimatedUnusedPercentAtReset',
      '17': true
    },
  ],
  '8': [
    {'1': '_estimated_unused_percent_at_reset'},
  ],
};

/// Descriptor for `QuotaWindow`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List quotaWindowDescriptor = $convert.base64Decode(
    'CgtRdW90YVdpbmRvdxISCgRuYW1lGAEgASgJUgRuYW1lEiEKDHVzZWRfcGVyY2VudBgCIAEoAV'
    'ILdXNlZFBlcmNlbnQSNwoJcmVzZXRzX2F0GAMgASgLMhouZ29vZ2xlLnByb3RvYnVmLlRpbWVz'
    'dGFtcFIIcmVzZXRzQXQSJQoOd2luZG93X3NlY29uZHMYBCABKANSDXdpbmRvd1NlY29uZHMSTQ'
    'ohZXN0aW1hdGVkX3VudXNlZF9wZXJjZW50X2F0X3Jlc2V0GAUgASgBSABSHWVzdGltYXRlZFVu'
    'dXNlZFBlcmNlbnRBdFJlc2V0iAEBQiQKIl9lc3RpbWF0ZWRfdW51c2VkX3BlcmNlbnRfYXRfcm'
    'VzZXQ=');

@$core.Deprecated('Use resetCreditDescriptor instead')
const ResetCredit$json = {
  '1': 'ResetCredit',
  '2': [
    {'1': 'kind', '3': 1, '4': 1, '5': 9, '10': 'kind'},
    {'1': 'count', '3': 2, '4': 1, '5': 5, '10': 'count'},
    {
      '1': 'expires_at',
      '3': 3,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Timestamp',
      '10': 'expiresAt'
    },
  ],
};

/// Descriptor for `ResetCredit`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List resetCreditDescriptor = $convert.base64Decode(
    'CgtSZXNldENyZWRpdBISCgRraW5kGAEgASgJUgRraW5kEhQKBWNvdW50GAIgASgFUgVjb3VudB'
    'I5CgpleHBpcmVzX2F0GAMgASgLMhouZ29vZ2xlLnByb3RvYnVmLlRpbWVzdGFtcFIJZXhwaXJl'
    'c0F0');

@$core.Deprecated('Use suggestRequestDescriptor instead')
const SuggestRequest$json = {
  '1': 'SuggestRequest',
  '2': [
    {
      '1': 'task_size',
      '3': 1,
      '4': 1,
      '5': 14,
      '6': '.protectorate.v1.TaskSize',
      '10': 'taskSize'
    },
    {'1': 'refresh', '3': 2, '4': 1, '5': 8, '10': 'refresh'},
  ],
};

/// Descriptor for `SuggestRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List suggestRequestDescriptor = $convert.base64Decode(
    'Cg5TdWdnZXN0UmVxdWVzdBI2Cgl0YXNrX3NpemUYASABKA4yGS5wcm90ZWN0b3JhdGUudjEuVG'
    'Fza1NpemVSCHRhc2tTaXplEhgKB3JlZnJlc2gYAiABKAhSB3JlZnJlc2g=');

@$core.Deprecated('Use suggestResponseDescriptor instead')
const SuggestResponse$json = {
  '1': 'SuggestResponse',
  '2': [
    {
      '1': 'suggestions',
      '3': 1,
      '4': 3,
      '5': 11,
      '6': '.protectorate.v1.HarnessSuggestion',
      '10': 'suggestions'
    },
    {
      '1': 'generated_at',
      '3': 2,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Timestamp',
      '10': 'generatedAt'
    },
  ],
};

/// Descriptor for `SuggestResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List suggestResponseDescriptor = $convert.base64Decode(
    'Cg9TdWdnZXN0UmVzcG9uc2USRAoLc3VnZ2VzdGlvbnMYASADKAsyIi5wcm90ZWN0b3JhdGUudj'
    'EuSGFybmVzc1N1Z2dlc3Rpb25SC3N1Z2dlc3Rpb25zEj0KDGdlbmVyYXRlZF9hdBgCIAEoCzIa'
    'Lmdvb2dsZS5wcm90b2J1Zi5UaW1lc3RhbXBSC2dlbmVyYXRlZEF0');

@$core.Deprecated('Use harnessSuggestionDescriptor instead')
const HarnessSuggestion$json = {
  '1': 'HarnessSuggestion',
  '2': [
    {'1': 'rank', '3': 1, '4': 1, '5': 5, '10': 'rank'},
    {
      '1': 'harness',
      '3': 2,
      '4': 1,
      '5': 14,
      '6': '.protectorate.v1.Harness',
      '10': 'harness'
    },
    {'1': 'provider', '3': 3, '4': 1, '5': 9, '10': 'provider'},
    {'1': 'recommended', '3': 4, '4': 1, '5': 8, '10': 'recommended'},
    {'1': 'score', '3': 5, '4': 1, '5': 1, '10': 'score'},
    {
      '1': 'freshness',
      '3': 6,
      '4': 1,
      '5': 14,
      '6': '.protectorate.v1.Freshness',
      '10': 'freshness'
    },
    {
      '1': 'usable_percent',
      '3': 7,
      '4': 1,
      '5': 1,
      '9': 0,
      '10': 'usablePercent',
      '17': true
    },
    {
      '1': 'expiring_percent',
      '3': 8,
      '4': 1,
      '5': 1,
      '9': 1,
      '10': 'expiringPercent',
      '17': true
    },
    {'1': 'window', '3': 9, '4': 1, '5': 9, '10': 'window'},
    {
      '1': 'resets_in',
      '3': 10,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Duration',
      '10': 'resetsIn'
    },
    {'1': 'reason', '3': 11, '4': 1, '5': 9, '10': 'reason'},
  ],
  '8': [
    {'1': '_usable_percent'},
    {'1': '_expiring_percent'},
  ],
};

/// Descriptor for `HarnessSuggestion`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List harnessSuggestionDescriptor = $convert.base64Decode(
    'ChFIYXJuZXNzU3VnZ2VzdGlvbhISCgRyYW5rGAEgASgFUgRyYW5rEjIKB2hhcm5lc3MYAiABKA'
    '4yGC5wcm90ZWN0b3JhdGUudjEuSGFybmVzc1IHaGFybmVzcxIaCghwcm92aWRlchgDIAEoCVII'
    'cHJvdmlkZXISIAoLcmVjb21tZW5kZWQYBCABKAhSC3JlY29tbWVuZGVkEhQKBXNjb3JlGAUgAS'
    'gBUgVzY29yZRI4CglmcmVzaG5lc3MYBiABKA4yGi5wcm90ZWN0b3JhdGUudjEuRnJlc2huZXNz'
    'UglmcmVzaG5lc3MSKgoOdXNhYmxlX3BlcmNlbnQYByABKAFIAFINdXNhYmxlUGVyY2VudIgBAR'
    'IuChBleHBpcmluZ19wZXJjZW50GAggASgBSAFSD2V4cGlyaW5nUGVyY2VudIgBARIWCgZ3aW5k'
    'b3cYCSABKAlSBndpbmRvdxI2CglyZXNldHNfaW4YCiABKAsyGS5nb29nbGUucHJvdG9idWYuRH'
    'VyYXRpb25SCHJlc2V0c0luEhYKBnJlYXNvbhgLIAEoCVIGcmVhc29uQhEKD191c2FibGVfcGVy'
    'Y2VudEITChFfZXhwaXJpbmdfcGVyY2VudA==');

const $core.Map<$core.String, $core.dynamic> QuotaServiceBase$json = {
  '1': 'QuotaService',
  '2': [
    {
      '1': 'GetQuota',
      '2': '.protectorate.v1.GetQuotaRequest',
      '3': '.protectorate.v1.GetQuotaResponse',
      '4': {'34': 1},
    },
    {
      '1': 'Suggest',
      '2': '.protectorate.v1.SuggestRequest',
      '3': '.protectorate.v1.SuggestResponse',
      '4': {'34': 1},
    },
  ],
};

@$core.Deprecated('Use quotaServiceDescriptor instead')
const $core.Map<$core.String, $core.Map<$core.String, $core.dynamic>>
    QuotaServiceBase$messageJson = {
  '.protectorate.v1.GetQuotaRequest': GetQuotaRequest$json,
  '.protectorate.v1.GetQuotaResponse': GetQuotaResponse$json,
  '.protectorate.v1.ProviderQuota': ProviderQuota$json,
  '.google.protobuf.Timestamp': $0.Timestamp$json,
  '.google.protobuf.Duration': $1.Duration$json,
  '.protectorate.v1.QuotaWindow': QuotaWindow$json,
  '.protectorate.v1.ResetCredit': ResetCredit$json,
  '.protectorate.v1.SuggestRequest': SuggestRequest$json,
  '.protectorate.v1.SuggestResponse': SuggestResponse$json,
  '.protectorate.v1.HarnessSuggestion': HarnessSuggestion$json,
};

/// Descriptor for `QuotaService`. Decode as a `google.protobuf.ServiceDescriptorProto`.
final $typed_data.Uint8List quotaServiceDescriptor = $convert.base64Decode(
    'CgxRdW90YVNlcnZpY2USVAoIR2V0UXVvdGESIC5wcm90ZWN0b3JhdGUudjEuR2V0UXVvdGFSZX'
    'F1ZXN0GiEucHJvdGVjdG9yYXRlLnYxLkdldFF1b3RhUmVzcG9uc2UiA5ACARJRCgdTdWdnZXN0'
    'Eh8ucHJvdGVjdG9yYXRlLnYxLlN1Z2dlc3RSZXF1ZXN0GiAucHJvdGVjdG9yYXRlLnYxLlN1Z2'
    'dlc3RSZXNwb25zZSIDkAIB');
