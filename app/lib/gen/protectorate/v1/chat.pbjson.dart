// This is a generated file - do not edit.
//
// Generated from protectorate/v1/chat.proto.

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

@$core.Deprecated('Use sleeveModeDescriptor instead')
const SleeveMode$json = {
  '1': 'SleeveMode',
  '2': [
    {'1': 'SLEEVE_MODE_UNSPECIFIED', '2': 0},
    {'1': 'SLEEVE_MODE_CHAT', '2': 1},
    {'1': 'SLEEVE_MODE_TERMINAL', '2': 2},
  ],
};

/// Descriptor for `SleeveMode`. Decode as a `google.protobuf.EnumDescriptorProto`.
final $typed_data.Uint8List sleeveModeDescriptor = $convert.base64Decode(
    'CgpTbGVldmVNb2RlEhsKF1NMRUVWRV9NT0RFX1VOU1BFQ0lGSUVEEAASFAoQU0xFRVZFX01PRE'
    'VfQ0hBVBABEhgKFFNMRUVWRV9NT0RFX1RFUk1JTkFMEAI=');

@$core.Deprecated('Use chatStateDescriptor instead')
const ChatState$json = {
  '1': 'ChatState',
  '2': [
    {'1': 'CHAT_STATE_UNSPECIFIED', '2': 0},
    {'1': 'CHAT_STATE_AWAITING_APPROVAL', '2': 1},
    {'1': 'CHAT_STATE_WORKING', '2': 2},
    {'1': 'CHAT_STATE_COMPACTING', '2': 3},
    {'1': 'CHAT_STATE_IDLE', '2': 4},
    {'1': 'CHAT_STATE_FAILED', '2': 5},
    {'1': 'CHAT_STATE_EXITED', '2': 6},
  ],
};

/// Descriptor for `ChatState`. Decode as a `google.protobuf.EnumDescriptorProto`.
final $typed_data.Uint8List chatStateDescriptor = $convert.base64Decode(
    'CglDaGF0U3RhdGUSGgoWQ0hBVF9TVEFURV9VTlNQRUNJRklFRBAAEiAKHENIQVRfU1RBVEVfQV'
    'dBSVRJTkdfQVBQUk9WQUwQARIWChJDSEFUX1NUQVRFX1dPUktJTkcQAhIZChVDSEFUX1NUQVRF'
    'X0NPTVBBQ1RJTkcQAxITCg9DSEFUX1NUQVRFX0lETEUQBBIVChFDSEFUX1NUQVRFX0ZBSUxFRB'
    'AFEhUKEUNIQVRfU1RBVEVfRVhJVEVEEAY=');

@$core.Deprecated('Use reportStatusDescriptor instead')
const ReportStatus$json = {
  '1': 'ReportStatus',
  '2': [
    {'1': 'REPORT_STATUS_UNSPECIFIED', '2': 0},
    {'1': 'REPORT_STATUS_COMPLETE', '2': 1},
    {'1': 'REPORT_STATUS_PARTIAL', '2': 2},
    {'1': 'REPORT_STATUS_BLOCKED', '2': 3},
  ],
};

/// Descriptor for `ReportStatus`. Decode as a `google.protobuf.EnumDescriptorProto`.
final $typed_data.Uint8List reportStatusDescriptor = $convert.base64Decode(
    'CgxSZXBvcnRTdGF0dXMSHQoZUkVQT1JUX1NUQVRVU19VTlNQRUNJRklFRBAAEhoKFlJFUE9SVF'
    '9TVEFUVVNfQ09NUExFVEUQARIZChVSRVBPUlRfU1RBVFVTX1BBUlRJQUwQAhIZChVSRVBPUlRf'
    'U1RBVFVTX0JMT0NLRUQQAw==');

@$core.Deprecated('Use waitUntilDescriptor instead')
const WaitUntil$json = {
  '1': 'WaitUntil',
  '2': [
    {'1': 'WAIT_UNTIL_UNSPECIFIED', '2': 0},
    {'1': 'WAIT_UNTIL_IDLE', '2': 1},
    {'1': 'WAIT_UNTIL_REPORT', '2': 2},
  ],
};

/// Descriptor for `WaitUntil`. Decode as a `google.protobuf.EnumDescriptorProto`.
final $typed_data.Uint8List waitUntilDescriptor = $convert.base64Decode(
    'CglXYWl0VW50aWwSGgoWV0FJVF9VTlRJTF9VTlNQRUNJRklFRBAAEhMKD1dBSVRfVU5USUxfSU'
    'RMRRABEhUKEVdBSVRfVU5USUxfUkVQT1JUEAI=');

@$core.Deprecated('Use modelSelectionDescriptor instead')
const ModelSelection$json = {
  '1': 'ModelSelection',
  '2': [
    {'1': 'model', '3': 1, '4': 1, '5': 9, '10': 'model'},
    {'1': 'effort', '3': 2, '4': 1, '5': 9, '10': 'effort'},
  ],
};

/// Descriptor for `ModelSelection`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List modelSelectionDescriptor = $convert.base64Decode(
    'Cg5Nb2RlbFNlbGVjdGlvbhIUCgVtb2RlbBgBIAEoCVIFbW9kZWwSFgoGZWZmb3J0GAIgASgJUg'
    'ZlZmZvcnQ=');

@$core.Deprecated('Use modelSettingsDescriptor instead')
const ModelSettings$json = {
  '1': 'ModelSettings',
  '2': [
    {
      '1': 'requested',
      '3': 1,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.ModelSelection',
      '10': 'requested'
    },
    {
      '1': 'effective',
      '3': 2,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.ModelSelection',
      '10': 'effective'
    },
    {
      '1': 'native_defaults',
      '3': 3,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.ModelSelection',
      '10': 'nativeDefaults'
    },
  ],
};

/// Descriptor for `ModelSettings`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List modelSettingsDescriptor = $convert.base64Decode(
    'Cg1Nb2RlbFNldHRpbmdzEj0KCXJlcXVlc3RlZBgBIAEoCzIfLnByb3RlY3RvcmF0ZS52MS5Nb2'
    'RlbFNlbGVjdGlvblIJcmVxdWVzdGVkEj0KCWVmZmVjdGl2ZRgCIAEoCzIfLnByb3RlY3RvcmF0'
    'ZS52MS5Nb2RlbFNlbGVjdGlvblIJZWZmZWN0aXZlEkgKD25hdGl2ZV9kZWZhdWx0cxgDIAEoCz'
    'IfLnByb3RlY3RvcmF0ZS52MS5Nb2RlbFNlbGVjdGlvblIObmF0aXZlRGVmYXVsdHM=');

@$core.Deprecated('Use modelOptionDescriptor instead')
const ModelOption$json = {
  '1': 'ModelOption',
  '2': [
    {'1': 'model', '3': 1, '4': 1, '5': 9, '10': 'model'},
    {'1': 'name', '3': 2, '4': 1, '5': 9, '10': 'name'},
    {'1': 'efforts', '3': 3, '4': 3, '5': 9, '10': 'efforts'},
    {'1': 'default_effort', '3': 4, '4': 1, '5': 9, '10': 'defaultEffort'},
    {'1': 'is_default', '3': 5, '4': 1, '5': 8, '10': 'isDefault'},
  ],
};

/// Descriptor for `ModelOption`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List modelOptionDescriptor = $convert.base64Decode(
    'CgtNb2RlbE9wdGlvbhIUCgVtb2RlbBgBIAEoCVIFbW9kZWwSEgoEbmFtZRgCIAEoCVIEbmFtZR'
    'IYCgdlZmZvcnRzGAMgAygJUgdlZmZvcnRzEiUKDmRlZmF1bHRfZWZmb3J0GAQgASgJUg1kZWZh'
    'dWx0RWZmb3J0Eh0KCmlzX2RlZmF1bHQYBSABKAhSCWlzRGVmYXVsdA==');

@$core.Deprecated('Use listModelsRequestDescriptor instead')
const ListModelsRequest$json = {
  '1': 'ListModelsRequest',
  '2': [
    {'1': 'sleeve', '3': 1, '4': 1, '5': 9, '10': 'sleeve'},
    {'1': 'harness', '3': 2, '4': 1, '5': 9, '10': 'harness'},
  ],
};

/// Descriptor for `ListModelsRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List listModelsRequestDescriptor = $convert.base64Decode(
    'ChFMaXN0TW9kZWxzUmVxdWVzdBIWCgZzbGVldmUYASABKAlSBnNsZWV2ZRIYCgdoYXJuZXNzGA'
    'IgASgJUgdoYXJuZXNz');

@$core.Deprecated('Use listModelsResponseDescriptor instead')
const ListModelsResponse$json = {
  '1': 'ListModelsResponse',
  '2': [
    {'1': 'supported', '3': 1, '4': 1, '5': 8, '10': 'supported'},
    {'1': 'reason', '3': 2, '4': 1, '5': 9, '10': 'reason'},
    {
      '1': 'models',
      '3': 3,
      '4': 3,
      '5': 11,
      '6': '.protectorate.v1.ModelOption',
      '10': 'models'
    },
    {'1': 'live_update', '3': 4, '4': 1, '5': 8, '10': 'liveUpdate'},
  ],
};

/// Descriptor for `ListModelsResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List listModelsResponseDescriptor = $convert.base64Decode(
    'ChJMaXN0TW9kZWxzUmVzcG9uc2USHAoJc3VwcG9ydGVkGAEgASgIUglzdXBwb3J0ZWQSFgoGcm'
    'Vhc29uGAIgASgJUgZyZWFzb24SNAoGbW9kZWxzGAMgAygLMhwucHJvdGVjdG9yYXRlLnYxLk1v'
    'ZGVsT3B0aW9uUgZtb2RlbHMSHwoLbGl2ZV91cGRhdGUYBCABKAhSCmxpdmVVcGRhdGU=');

@$core.Deprecated('Use setSelectionRequestDescriptor instead')
const SetSelectionRequest$json = {
  '1': 'SetSelectionRequest',
  '2': [
    {'1': 'sleeve', '3': 1, '4': 1, '5': 9, '10': 'sleeve'},
    {
      '1': 'selection',
      '3': 2,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.ModelSelection',
      '10': 'selection'
    },
  ],
};

/// Descriptor for `SetSelectionRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List setSelectionRequestDescriptor = $convert.base64Decode(
    'ChNTZXRTZWxlY3Rpb25SZXF1ZXN0EhYKBnNsZWV2ZRgBIAEoCVIGc2xlZXZlEj0KCXNlbGVjdG'
    'lvbhgCIAEoCzIfLnByb3RlY3RvcmF0ZS52MS5Nb2RlbFNlbGVjdGlvblIJc2VsZWN0aW9u');

@$core.Deprecated('Use setSelectionResponseDescriptor instead')
const SetSelectionResponse$json = {
  '1': 'SetSelectionResponse',
  '2': [
    {
      '1': 'chat',
      '3': 1,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.ChatStatus',
      '10': 'chat'
    },
  ],
};

/// Descriptor for `SetSelectionResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List setSelectionResponseDescriptor = $convert.base64Decode(
    'ChRTZXRTZWxlY3Rpb25SZXNwb25zZRIvCgRjaGF0GAEgASgLMhsucHJvdGVjdG9yYXRlLnYxLk'
    'NoYXRTdGF0dXNSBGNoYXQ=');

@$core.Deprecated('Use chatStatusDescriptor instead')
const ChatStatus$json = {
  '1': 'ChatStatus',
  '2': [
    {
      '1': 'state',
      '3': 1,
      '4': 1,
      '5': 14,
      '6': '.protectorate.v1.ChatState',
      '10': 'state'
    },
    {
      '1': 'harness_session_id',
      '3': 2,
      '4': 1,
      '5': 9,
      '10': 'harnessSessionId'
    },
    {'1': 'model', '3': 3, '4': 1, '5': 9, '10': 'model'},
    {'1': 'context_tokens', '3': 4, '4': 1, '5': 4, '10': 'contextTokens'},
    {'1': 'context_window', '3': 5, '4': 1, '5': 4, '10': 'contextWindow'},
    {
      '1': 'cache_expires_at',
      '3': 6,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Timestamp',
      '10': 'cacheExpiresAt'
    },
    {'1': 'last_seq', '3': 7, '4': 1, '5': 4, '10': 'lastSeq'},
    {
      '1': 'last_activity_at',
      '3': 8,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Timestamp',
      '10': 'lastActivityAt'
    },
    {'1': 'provider', '3': 9, '4': 1, '5': 9, '10': 'provider'},
    {'1': 'local_model', '3': 10, '4': 1, '5': 8, '10': 'localModel'},
    {'1': 'reasoning_effort', '3': 11, '4': 1, '5': 9, '10': 'reasoningEffort'},
    {
      '1': 'settings',
      '3': 12,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.ModelSettings',
      '10': 'settings'
    },
  ],
};

/// Descriptor for `ChatStatus`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List chatStatusDescriptor = $convert.base64Decode(
    'CgpDaGF0U3RhdHVzEjAKBXN0YXRlGAEgASgOMhoucHJvdGVjdG9yYXRlLnYxLkNoYXRTdGF0ZV'
    'IFc3RhdGUSLAoSaGFybmVzc19zZXNzaW9uX2lkGAIgASgJUhBoYXJuZXNzU2Vzc2lvbklkEhQK'
    'BW1vZGVsGAMgASgJUgVtb2RlbBIlCg5jb250ZXh0X3Rva2VucxgEIAEoBFINY29udGV4dFRva2'
    'VucxIlCg5jb250ZXh0X3dpbmRvdxgFIAEoBFINY29udGV4dFdpbmRvdxJEChBjYWNoZV9leHBp'
    'cmVzX2F0GAYgASgLMhouZ29vZ2xlLnByb3RvYnVmLlRpbWVzdGFtcFIOY2FjaGVFeHBpcmVzQX'
    'QSGQoIbGFzdF9zZXEYByABKARSB2xhc3RTZXESRAoQbGFzdF9hY3Rpdml0eV9hdBgIIAEoCzIa'
    'Lmdvb2dsZS5wcm90b2J1Zi5UaW1lc3RhbXBSDmxhc3RBY3Rpdml0eUF0EhoKCHByb3ZpZGVyGA'
    'kgASgJUghwcm92aWRlchIfCgtsb2NhbF9tb2RlbBgKIAEoCFIKbG9jYWxNb2RlbBIpChByZWFz'
    'b25pbmdfZWZmb3J0GAsgASgJUg9yZWFzb25pbmdFZmZvcnQSOgoIc2V0dGluZ3MYDCABKAsyHi'
    '5wcm90ZWN0b3JhdGUudjEuTW9kZWxTZXR0aW5nc1IIc2V0dGluZ3M=');

@$core.Deprecated('Use chatEventDescriptor instead')
const ChatEvent$json = {
  '1': 'ChatEvent',
  '2': [
    {'1': 'seq', '3': 1, '4': 1, '5': 4, '10': 'seq'},
    {
      '1': 'time',
      '3': 2,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Timestamp',
      '10': 'time'
    },
    {
      '1': 'user',
      '3': 10,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.UserMessage',
      '9': 0,
      '10': 'user'
    },
    {
      '1': 'text',
      '3': 11,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.AssistantText',
      '9': 0,
      '10': 'text'
    },
    {
      '1': 'tool_call',
      '3': 12,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.ToolCall',
      '9': 0,
      '10': 'toolCall'
    },
    {
      '1': 'tool_result',
      '3': 13,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.ToolResult',
      '9': 0,
      '10': 'toolResult'
    },
    {
      '1': 'approval',
      '3': 14,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.ApprovalRequest',
      '9': 0,
      '10': 'approval'
    },
    {
      '1': 'approval_resolved',
      '3': 15,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.ApprovalResolved',
      '9': 0,
      '10': 'approvalResolved'
    },
    {
      '1': 'usage',
      '3': 16,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.UsageUpdate',
      '9': 0,
      '10': 'usage'
    },
    {
      '1': 'settled',
      '3': 17,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.TurnSettled',
      '9': 0,
      '10': 'settled'
    },
    {
      '1': 'compaction',
      '3': 18,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.Compaction',
      '9': 0,
      '10': 'compaction'
    },
    {
      '1': 'error',
      '3': 19,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.ChatError',
      '9': 0,
      '10': 'error'
    },
    {
      '1': 'state',
      '3': 20,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.StateChange',
      '9': 0,
      '10': 'state'
    },
    {
      '1': 'handoff',
      '3': 21,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.Handoff',
      '9': 0,
      '10': 'handoff'
    },
    {
      '1': 'settings',
      '3': 22,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.ModelSettings',
      '9': 0,
      '10': 'settings'
    },
    {
      '1': 'report',
      '3': 23,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.SleeveResult',
      '9': 0,
      '10': 'report'
    },
  ],
  '8': [
    {'1': 'body'},
  ],
};

/// Descriptor for `ChatEvent`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List chatEventDescriptor = $convert.base64Decode(
    'CglDaGF0RXZlbnQSEAoDc2VxGAEgASgEUgNzZXESLgoEdGltZRgCIAEoCzIaLmdvb2dsZS5wcm'
    '90b2J1Zi5UaW1lc3RhbXBSBHRpbWUSMgoEdXNlchgKIAEoCzIcLnByb3RlY3RvcmF0ZS52MS5V'
    'c2VyTWVzc2FnZUgAUgR1c2VyEjQKBHRleHQYCyABKAsyHi5wcm90ZWN0b3JhdGUudjEuQXNzaX'
    'N0YW50VGV4dEgAUgR0ZXh0EjgKCXRvb2xfY2FsbBgMIAEoCzIZLnByb3RlY3RvcmF0ZS52MS5U'
    'b29sQ2FsbEgAUgh0b29sQ2FsbBI+Cgt0b29sX3Jlc3VsdBgNIAEoCzIbLnByb3RlY3RvcmF0ZS'
    '52MS5Ub29sUmVzdWx0SABSCnRvb2xSZXN1bHQSPgoIYXBwcm92YWwYDiABKAsyIC5wcm90ZWN0'
    'b3JhdGUudjEuQXBwcm92YWxSZXF1ZXN0SABSCGFwcHJvdmFsElAKEWFwcHJvdmFsX3Jlc29sdm'
    'VkGA8gASgLMiEucHJvdGVjdG9yYXRlLnYxLkFwcHJvdmFsUmVzb2x2ZWRIAFIQYXBwcm92YWxS'
    'ZXNvbHZlZBI0CgV1c2FnZRgQIAEoCzIcLnByb3RlY3RvcmF0ZS52MS5Vc2FnZVVwZGF0ZUgAUg'
    'V1c2FnZRI4CgdzZXR0bGVkGBEgASgLMhwucHJvdGVjdG9yYXRlLnYxLlR1cm5TZXR0bGVkSABS'
    'B3NldHRsZWQSPQoKY29tcGFjdGlvbhgSIAEoCzIbLnByb3RlY3RvcmF0ZS52MS5Db21wYWN0aW'
    '9uSABSCmNvbXBhY3Rpb24SMgoFZXJyb3IYEyABKAsyGi5wcm90ZWN0b3JhdGUudjEuQ2hhdEVy'
    'cm9ySABSBWVycm9yEjQKBXN0YXRlGBQgASgLMhwucHJvdGVjdG9yYXRlLnYxLlN0YXRlQ2hhbm'
    'dlSABSBXN0YXRlEjQKB2hhbmRvZmYYFSABKAsyGC5wcm90ZWN0b3JhdGUudjEuSGFuZG9mZkgA'
    'UgdoYW5kb2ZmEjwKCHNldHRpbmdzGBYgASgLMh4ucHJvdGVjdG9yYXRlLnYxLk1vZGVsU2V0dG'
    'luZ3NIAFIIc2V0dGluZ3MSNwoGcmVwb3J0GBcgASgLMh0ucHJvdGVjdG9yYXRlLnYxLlNsZWV2'
    'ZVJlc3VsdEgAUgZyZXBvcnRCBgoEYm9keQ==');

@$core.Deprecated('Use userMessageDescriptor instead')
const UserMessage$json = {
  '1': 'UserMessage',
  '2': [
    {'1': 'text', '3': 1, '4': 1, '5': 9, '10': 'text'},
    {'1': 'from', '3': 2, '4': 1, '5': 9, '10': 'from'},
  ],
};

/// Descriptor for `UserMessage`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List userMessageDescriptor = $convert.base64Decode(
    'CgtVc2VyTWVzc2FnZRISCgR0ZXh0GAEgASgJUgR0ZXh0EhIKBGZyb20YAiABKAlSBGZyb20=');

@$core.Deprecated('Use assistantTextDescriptor instead')
const AssistantText$json = {
  '1': 'AssistantText',
  '2': [
    {'1': 'message_id', '3': 1, '4': 1, '5': 9, '10': 'messageId'},
    {'1': 'delta', '3': 2, '4': 1, '5': 9, '10': 'delta'},
    {'1': 'thinking', '3': 3, '4': 1, '5': 8, '10': 'thinking'},
  ],
};

/// Descriptor for `AssistantText`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List assistantTextDescriptor = $convert.base64Decode(
    'Cg1Bc3Npc3RhbnRUZXh0Eh0KCm1lc3NhZ2VfaWQYASABKAlSCW1lc3NhZ2VJZBIUCgVkZWx0YR'
    'gCIAEoCVIFZGVsdGESGgoIdGhpbmtpbmcYAyABKAhSCHRoaW5raW5n');

@$core.Deprecated('Use toolCallDescriptor instead')
const ToolCall$json = {
  '1': 'ToolCall',
  '2': [
    {'1': 'id', '3': 1, '4': 1, '5': 9, '10': 'id'},
    {'1': 'name', '3': 2, '4': 1, '5': 9, '10': 'name'},
    {'1': 'input_json', '3': 3, '4': 1, '5': 9, '10': 'inputJson'},
  ],
};

/// Descriptor for `ToolCall`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List toolCallDescriptor = $convert.base64Decode(
    'CghUb29sQ2FsbBIOCgJpZBgBIAEoCVICaWQSEgoEbmFtZRgCIAEoCVIEbmFtZRIdCgppbnB1dF'
    '9qc29uGAMgASgJUglpbnB1dEpzb24=');

@$core.Deprecated('Use toolResultDescriptor instead')
const ToolResult$json = {
  '1': 'ToolResult',
  '2': [
    {'1': 'tool_call_id', '3': 1, '4': 1, '5': 9, '10': 'toolCallId'},
    {'1': 'output', '3': 2, '4': 1, '5': 9, '10': 'output'},
    {'1': 'is_error', '3': 3, '4': 1, '5': 8, '10': 'isError'},
    {'1': 'truncated', '3': 4, '4': 1, '5': 8, '10': 'truncated'},
  ],
};

/// Descriptor for `ToolResult`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List toolResultDescriptor = $convert.base64Decode(
    'CgpUb29sUmVzdWx0EiAKDHRvb2xfY2FsbF9pZBgBIAEoCVIKdG9vbENhbGxJZBIWCgZvdXRwdX'
    'QYAiABKAlSBm91dHB1dBIZCghpc19lcnJvchgDIAEoCFIHaXNFcnJvchIcCgl0cnVuY2F0ZWQY'
    'BCABKAhSCXRydW5jYXRlZA==');

@$core.Deprecated('Use approvalRequestDescriptor instead')
const ApprovalRequest$json = {
  '1': 'ApprovalRequest',
  '2': [
    {'1': 'id', '3': 1, '4': 1, '5': 9, '10': 'id'},
    {'1': 'kind', '3': 2, '4': 1, '5': 9, '10': 'kind'},
    {'1': 'summary', '3': 3, '4': 1, '5': 9, '10': 'summary'},
  ],
};

/// Descriptor for `ApprovalRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List approvalRequestDescriptor = $convert.base64Decode(
    'Cg9BcHByb3ZhbFJlcXVlc3QSDgoCaWQYASABKAlSAmlkEhIKBGtpbmQYAiABKAlSBGtpbmQSGA'
    'oHc3VtbWFyeRgDIAEoCVIHc3VtbWFyeQ==');

@$core.Deprecated('Use approvalResolvedDescriptor instead')
const ApprovalResolved$json = {
  '1': 'ApprovalResolved',
  '2': [
    {'1': 'id', '3': 1, '4': 1, '5': 9, '10': 'id'},
    {'1': 'allow', '3': 2, '4': 1, '5': 8, '10': 'allow'},
    {'1': 'by', '3': 3, '4': 1, '5': 9, '10': 'by'},
  ],
};

/// Descriptor for `ApprovalResolved`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List approvalResolvedDescriptor = $convert.base64Decode(
    'ChBBcHByb3ZhbFJlc29sdmVkEg4KAmlkGAEgASgJUgJpZBIUCgVhbGxvdxgCIAEoCFIFYWxsb3'
    'cSDgoCYnkYAyABKAlSAmJ5');

@$core.Deprecated('Use usageUpdateDescriptor instead')
const UsageUpdate$json = {
  '1': 'UsageUpdate',
  '2': [
    {'1': 'context_tokens', '3': 1, '4': 1, '5': 4, '10': 'contextTokens'},
    {'1': 'context_window', '3': 2, '4': 1, '5': 4, '10': 'contextWindow'},
    {'1': 'input_tokens', '3': 3, '4': 1, '5': 4, '10': 'inputTokens'},
    {'1': 'output_tokens', '3': 4, '4': 1, '5': 4, '10': 'outputTokens'},
    {'1': 'cache_read_tokens', '3': 5, '4': 1, '5': 4, '10': 'cacheReadTokens'},
    {
      '1': 'cache_write_tokens',
      '3': 6,
      '4': 1,
      '5': 4,
      '10': 'cacheWriteTokens'
    },
    {
      '1': 'cache_expires_at',
      '3': 7,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Timestamp',
      '10': 'cacheExpiresAt'
    },
  ],
};

/// Descriptor for `UsageUpdate`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List usageUpdateDescriptor = $convert.base64Decode(
    'CgtVc2FnZVVwZGF0ZRIlCg5jb250ZXh0X3Rva2VucxgBIAEoBFINY29udGV4dFRva2VucxIlCg'
    '5jb250ZXh0X3dpbmRvdxgCIAEoBFINY29udGV4dFdpbmRvdxIhCgxpbnB1dF90b2tlbnMYAyAB'
    'KARSC2lucHV0VG9rZW5zEiMKDW91dHB1dF90b2tlbnMYBCABKARSDG91dHB1dFRva2VucxIqCh'
    'FjYWNoZV9yZWFkX3Rva2VucxgFIAEoBFIPY2FjaGVSZWFkVG9rZW5zEiwKEmNhY2hlX3dyaXRl'
    'X3Rva2VucxgGIAEoBFIQY2FjaGVXcml0ZVRva2VucxJEChBjYWNoZV9leHBpcmVzX2F0GAcgAS'
    'gLMhouZ29vZ2xlLnByb3RvYnVmLlRpbWVzdGFtcFIOY2FjaGVFeHBpcmVzQXQ=');

@$core.Deprecated('Use turnSettledDescriptor instead')
const TurnSettled$json = {
  '1': 'TurnSettled',
  '2': [
    {'1': 'stop_reason', '3': 1, '4': 1, '5': 9, '10': 'stopReason'},
  ],
};

/// Descriptor for `TurnSettled`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List turnSettledDescriptor = $convert.base64Decode(
    'CgtUdXJuU2V0dGxlZBIfCgtzdG9wX3JlYXNvbhgBIAEoCVIKc3RvcFJlYXNvbg==');

@$core.Deprecated('Use compactionDescriptor instead')
const Compaction$json = {
  '1': 'Compaction',
  '2': [
    {'1': 'tokens_before', '3': 1, '4': 1, '5': 4, '10': 'tokensBefore'},
    {'1': 'tokens_after', '3': 2, '4': 1, '5': 4, '10': 'tokensAfter'},
    {'1': 'native', '3': 3, '4': 1, '5': 8, '10': 'native'},
  ],
};

/// Descriptor for `Compaction`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List compactionDescriptor = $convert.base64Decode(
    'CgpDb21wYWN0aW9uEiMKDXRva2Vuc19iZWZvcmUYASABKARSDHRva2Vuc0JlZm9yZRIhCgx0b2'
    'tlbnNfYWZ0ZXIYAiABKARSC3Rva2Vuc0FmdGVyEhYKBm5hdGl2ZRgDIAEoCFIGbmF0aXZl');

@$core.Deprecated('Use chatErrorDescriptor instead')
const ChatError$json = {
  '1': 'ChatError',
  '2': [
    {'1': 'text', '3': 1, '4': 1, '5': 9, '10': 'text'},
  ],
};

/// Descriptor for `ChatError`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List chatErrorDescriptor =
    $convert.base64Decode('CglDaGF0RXJyb3ISEgoEdGV4dBgBIAEoCVIEdGV4dA==');

@$core.Deprecated('Use stateChangeDescriptor instead')
const StateChange$json = {
  '1': 'StateChange',
  '2': [
    {
      '1': 'state',
      '3': 1,
      '4': 1,
      '5': 14,
      '6': '.protectorate.v1.ChatState',
      '10': 'state'
    },
  ],
};

/// Descriptor for `StateChange`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List stateChangeDescriptor = $convert.base64Decode(
    'CgtTdGF0ZUNoYW5nZRIwCgVzdGF0ZRgBIAEoDjIaLnByb3RlY3RvcmF0ZS52MS5DaGF0U3RhdG'
    'VSBXN0YXRl');

@$core.Deprecated('Use handoffDescriptor instead')
const Handoff$json = {
  '1': 'Handoff',
  '2': [
    {'1': 'from_harness', '3': 1, '4': 1, '5': 9, '10': 'fromHarness'},
    {'1': 'from_session_id', '3': 2, '4': 1, '5': 9, '10': 'fromSessionId'},
    {'1': 'to_harness', '3': 3, '4': 1, '5': 9, '10': 'toHarness'},
    {
      '1': 'summarizer_harness',
      '3': 4,
      '4': 1,
      '5': 9,
      '10': 'summarizerHarness'
    },
    {'1': 'summarizer_model', '3': 5, '4': 1, '5': 9, '10': 'summarizerModel'},
    {
      '1': 'summarizer_provider',
      '3': 6,
      '4': 1,
      '5': 9,
      '10': 'summarizerProvider'
    },
    {'1': 'summarizer_local', '3': 7, '4': 1, '5': 8, '10': 'summarizerLocal'},
    {'1': 'summary_error', '3': 8, '4': 1, '5': 9, '10': 'summaryError'},
    {'1': 'path', '3': 9, '4': 1, '5': 9, '10': 'path'},
    {'1': 'archive_path', '3': 10, '4': 1, '5': 9, '10': 'archivePath'},
    {
      '1': 'at',
      '3': 11,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Timestamp',
      '10': 'at'
    },
  ],
};

/// Descriptor for `Handoff`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List handoffDescriptor = $convert.base64Decode(
    'CgdIYW5kb2ZmEiEKDGZyb21faGFybmVzcxgBIAEoCVILZnJvbUhhcm5lc3MSJgoPZnJvbV9zZX'
    'NzaW9uX2lkGAIgASgJUg1mcm9tU2Vzc2lvbklkEh0KCnRvX2hhcm5lc3MYAyABKAlSCXRvSGFy'
    'bmVzcxItChJzdW1tYXJpemVyX2hhcm5lc3MYBCABKAlSEXN1bW1hcml6ZXJIYXJuZXNzEikKEH'
    'N1bW1hcml6ZXJfbW9kZWwYBSABKAlSD3N1bW1hcml6ZXJNb2RlbBIvChNzdW1tYXJpemVyX3By'
    'b3ZpZGVyGAYgASgJUhJzdW1tYXJpemVyUHJvdmlkZXISKQoQc3VtbWFyaXplcl9sb2NhbBgHIA'
    'EoCFIPc3VtbWFyaXplckxvY2FsEiMKDXN1bW1hcnlfZXJyb3IYCCABKAlSDHN1bW1hcnlFcnJv'
    'chISCgRwYXRoGAkgASgJUgRwYXRoEiEKDGFyY2hpdmVfcGF0aBgKIAEoCVILYXJjaGl2ZVBhdG'
    'gSKgoCYXQYCyABKAsyGi5nb29nbGUucHJvdG9idWYuVGltZXN0YW1wUgJhdA==');

@$core.Deprecated('Use watchRequestDescriptor instead')
const WatchRequest$json = {
  '1': 'WatchRequest',
  '2': [
    {'1': 'sleeve', '3': 1, '4': 1, '5': 9, '10': 'sleeve'},
    {'1': 'after_seq', '3': 2, '4': 1, '5': 4, '10': 'afterSeq'},
  ],
};

/// Descriptor for `WatchRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List watchRequestDescriptor = $convert.base64Decode(
    'CgxXYXRjaFJlcXVlc3QSFgoGc2xlZXZlGAEgASgJUgZzbGVldmUSGwoJYWZ0ZXJfc2VxGAIgAS'
    'gEUghhZnRlclNlcQ==');

@$core.Deprecated('Use sendRequestDescriptor instead')
const SendRequest$json = {
  '1': 'SendRequest',
  '2': [
    {'1': 'sleeve', '3': 1, '4': 1, '5': 9, '10': 'sleeve'},
    {'1': 'text', '3': 2, '4': 1, '5': 9, '10': 'text'},
    {'1': 'request_id', '3': 3, '4': 1, '5': 9, '10': 'requestId'},
  ],
};

/// Descriptor for `SendRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List sendRequestDescriptor = $convert.base64Decode(
    'CgtTZW5kUmVxdWVzdBIWCgZzbGVldmUYASABKAlSBnNsZWV2ZRISCgR0ZXh0GAIgASgJUgR0ZX'
    'h0Eh0KCnJlcXVlc3RfaWQYAyABKAlSCXJlcXVlc3RJZA==');

@$core.Deprecated('Use sendResponseDescriptor instead')
const SendResponse$json = {
  '1': 'SendResponse',
  '2': [
    {'1': 'seq', '3': 1, '4': 1, '5': 4, '10': 'seq'},
  ],
};

/// Descriptor for `SendResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List sendResponseDescriptor =
    $convert.base64Decode('CgxTZW5kUmVzcG9uc2USEAoDc2VxGAEgASgEUgNzZXE=');

@$core.Deprecated('Use interruptRequestDescriptor instead')
const InterruptRequest$json = {
  '1': 'InterruptRequest',
  '2': [
    {'1': 'sleeve', '3': 1, '4': 1, '5': 9, '10': 'sleeve'},
  ],
};

/// Descriptor for `InterruptRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List interruptRequestDescriptor = $convert
    .base64Decode('ChBJbnRlcnJ1cHRSZXF1ZXN0EhYKBnNsZWV2ZRgBIAEoCVIGc2xlZXZl');

@$core.Deprecated('Use interruptResponseDescriptor instead')
const InterruptResponse$json = {
  '1': 'InterruptResponse',
};

/// Descriptor for `InterruptResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List interruptResponseDescriptor =
    $convert.base64Decode('ChFJbnRlcnJ1cHRSZXNwb25zZQ==');

@$core.Deprecated('Use respondRequestDescriptor instead')
const RespondRequest$json = {
  '1': 'RespondRequest',
  '2': [
    {'1': 'sleeve', '3': 1, '4': 1, '5': 9, '10': 'sleeve'},
    {'1': 'approval_id', '3': 2, '4': 1, '5': 9, '10': 'approvalId'},
    {'1': 'allow', '3': 3, '4': 1, '5': 8, '10': 'allow'},
    {'1': 'note', '3': 4, '4': 1, '5': 9, '10': 'note'},
  ],
};

/// Descriptor for `RespondRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List respondRequestDescriptor = $convert.base64Decode(
    'Cg5SZXNwb25kUmVxdWVzdBIWCgZzbGVldmUYASABKAlSBnNsZWV2ZRIfCgthcHByb3ZhbF9pZB'
    'gCIAEoCVIKYXBwcm92YWxJZBIUCgVhbGxvdxgDIAEoCFIFYWxsb3cSEgoEbm90ZRgEIAEoCVIE'
    'bm90ZQ==');

@$core.Deprecated('Use respondResponseDescriptor instead')
const RespondResponse$json = {
  '1': 'RespondResponse',
};

/// Descriptor for `RespondResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List respondResponseDescriptor =
    $convert.base64Decode('Cg9SZXNwb25kUmVzcG9uc2U=');

@$core.Deprecated('Use compactRequestDescriptor instead')
const CompactRequest$json = {
  '1': 'CompactRequest',
  '2': [
    {'1': 'sleeve', '3': 1, '4': 1, '5': 9, '10': 'sleeve'},
  ],
};

/// Descriptor for `CompactRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List compactRequestDescriptor = $convert
    .base64Decode('Cg5Db21wYWN0UmVxdWVzdBIWCgZzbGVldmUYASABKAlSBnNsZWV2ZQ==');

@$core.Deprecated('Use compactResponseDescriptor instead')
const CompactResponse$json = {
  '1': 'CompactResponse',
  '2': [
    {
      '1': 'compaction',
      '3': 1,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.Compaction',
      '10': 'compaction'
    },
  ],
};

/// Descriptor for `CompactResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List compactResponseDescriptor = $convert.base64Decode(
    'Cg9Db21wYWN0UmVzcG9uc2USOwoKY29tcGFjdGlvbhgBIAEoCzIbLnByb3RlY3RvcmF0ZS52MS'
    '5Db21wYWN0aW9uUgpjb21wYWN0aW9u');

@$core.Deprecated('Use sleeveResultDescriptor instead')
const SleeveResult$json = {
  '1': 'SleeveResult',
  '2': [
    {
      '1': 'status',
      '3': 1,
      '4': 1,
      '5': 14,
      '6': '.protectorate.v1.ReportStatus',
      '10': 'status'
    },
    {'1': 'summary', '3': 2, '4': 1, '5': 9, '10': 'summary'},
    {
      '1': 'time',
      '3': 3,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Timestamp',
      '10': 'time'
    },
    {'1': 'chat_seq', '3': 4, '4': 1, '5': 4, '10': 'chatSeq'},
    {'1': 'brief_seq', '3': 5, '4': 1, '5': 4, '10': 'briefSeq'},
  ],
};

/// Descriptor for `SleeveResult`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List sleeveResultDescriptor = $convert.base64Decode(
    'CgxTbGVldmVSZXN1bHQSNQoGc3RhdHVzGAEgASgOMh0ucHJvdGVjdG9yYXRlLnYxLlJlcG9ydF'
    'N0YXR1c1IGc3RhdHVzEhgKB3N1bW1hcnkYAiABKAlSB3N1bW1hcnkSLgoEdGltZRgDIAEoCzIa'
    'Lmdvb2dsZS5wcm90b2J1Zi5UaW1lc3RhbXBSBHRpbWUSGQoIY2hhdF9zZXEYBCABKARSB2NoYX'
    'RTZXESGwoJYnJpZWZfc2VxGAUgASgEUghicmllZlNlcQ==');

@$core.Deprecated('Use reportRequestDescriptor instead')
const ReportRequest$json = {
  '1': 'ReportRequest',
  '2': [
    {'1': 'sleeve', '3': 1, '4': 1, '5': 9, '10': 'sleeve'},
    {
      '1': 'status',
      '3': 2,
      '4': 1,
      '5': 14,
      '6': '.protectorate.v1.ReportStatus',
      '10': 'status'
    },
    {'1': 'summary', '3': 3, '4': 1, '5': 9, '10': 'summary'},
  ],
};

/// Descriptor for `ReportRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List reportRequestDescriptor = $convert.base64Decode(
    'Cg1SZXBvcnRSZXF1ZXN0EhYKBnNsZWV2ZRgBIAEoCVIGc2xlZXZlEjUKBnN0YXR1cxgCIAEoDj'
    'IdLnByb3RlY3RvcmF0ZS52MS5SZXBvcnRTdGF0dXNSBnN0YXR1cxIYCgdzdW1tYXJ5GAMgASgJ'
    'UgdzdW1tYXJ5');

@$core.Deprecated('Use reportResponseDescriptor instead')
const ReportResponse$json = {
  '1': 'ReportResponse',
  '2': [
    {
      '1': 'result',
      '3': 1,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.SleeveResult',
      '10': 'result'
    },
  ],
};

/// Descriptor for `ReportResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List reportResponseDescriptor = $convert.base64Decode(
    'Cg5SZXBvcnRSZXNwb25zZRI1CgZyZXN1bHQYASABKAsyHS5wcm90ZWN0b3JhdGUudjEuU2xlZX'
    'ZlUmVzdWx0UgZyZXN1bHQ=');

@$core.Deprecated('Use waitRequestDescriptor instead')
const WaitRequest$json = {
  '1': 'WaitRequest',
  '2': [
    {'1': 'sleeve', '3': 1, '4': 1, '5': 9, '10': 'sleeve'},
    {
      '1': 'until',
      '3': 2,
      '4': 1,
      '5': 14,
      '6': '.protectorate.v1.WaitUntil',
      '10': 'until'
    },
    {
      '1': 'timeout',
      '3': 3,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Duration',
      '10': 'timeout'
    },
  ],
};

/// Descriptor for `WaitRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List waitRequestDescriptor = $convert.base64Decode(
    'CgtXYWl0UmVxdWVzdBIWCgZzbGVldmUYASABKAlSBnNsZWV2ZRIwCgV1bnRpbBgCIAEoDjIaLn'
    'Byb3RlY3RvcmF0ZS52MS5XYWl0VW50aWxSBXVudGlsEjMKB3RpbWVvdXQYAyABKAsyGS5nb29n'
    'bGUucHJvdG9idWYuRHVyYXRpb25SB3RpbWVvdXQ=');

@$core.Deprecated('Use waitResponseDescriptor instead')
const WaitResponse$json = {
  '1': 'WaitResponse',
  '2': [
    {'1': 'sleeve', '3': 1, '4': 1, '5': 9, '10': 'sleeve'},
    {
      '1': 'result',
      '3': 2,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.SleeveResult',
      '10': 'result'
    },
  ],
};

/// Descriptor for `WaitResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List waitResponseDescriptor = $convert.base64Decode(
    'CgxXYWl0UmVzcG9uc2USFgoGc2xlZXZlGAEgASgJUgZzbGVldmUSNQoGcmVzdWx0GAIgASgLMh'
    '0ucHJvdGVjdG9yYXRlLnYxLlNsZWV2ZVJlc3VsdFIGcmVzdWx0');

const $core.Map<$core.String, $core.dynamic> ChatServiceBase$json = {
  '1': 'ChatService',
  '2': [
    {
      '1': 'Watch',
      '2': '.protectorate.v1.WatchRequest',
      '3': '.protectorate.v1.ChatEvent',
      '4': {'34': 1},
      '6': true,
    },
    {
      '1': 'Send',
      '2': '.protectorate.v1.SendRequest',
      '3': '.protectorate.v1.SendResponse'
    },
    {
      '1': 'Interrupt',
      '2': '.protectorate.v1.InterruptRequest',
      '3': '.protectorate.v1.InterruptResponse'
    },
    {
      '1': 'Respond',
      '2': '.protectorate.v1.RespondRequest',
      '3': '.protectorate.v1.RespondResponse'
    },
    {
      '1': 'Compact',
      '2': '.protectorate.v1.CompactRequest',
      '3': '.protectorate.v1.CompactResponse'
    },
    {
      '1': 'ListModels',
      '2': '.protectorate.v1.ListModelsRequest',
      '3': '.protectorate.v1.ListModelsResponse',
      '4': {'34': 1},
    },
    {
      '1': 'SetSelection',
      '2': '.protectorate.v1.SetSelectionRequest',
      '3': '.protectorate.v1.SetSelectionResponse'
    },
    {
      '1': 'Report',
      '2': '.protectorate.v1.ReportRequest',
      '3': '.protectorate.v1.ReportResponse'
    },
    {
      '1': 'Wait',
      '2': '.protectorate.v1.WaitRequest',
      '3': '.protectorate.v1.WaitResponse',
      '4': {'34': 1},
    },
  ],
};

@$core.Deprecated('Use chatServiceDescriptor instead')
const $core.Map<$core.String, $core.Map<$core.String, $core.dynamic>>
    ChatServiceBase$messageJson = {
  '.protectorate.v1.WatchRequest': WatchRequest$json,
  '.protectorate.v1.ChatEvent': ChatEvent$json,
  '.google.protobuf.Timestamp': $0.Timestamp$json,
  '.protectorate.v1.UserMessage': UserMessage$json,
  '.protectorate.v1.AssistantText': AssistantText$json,
  '.protectorate.v1.ToolCall': ToolCall$json,
  '.protectorate.v1.ToolResult': ToolResult$json,
  '.protectorate.v1.ApprovalRequest': ApprovalRequest$json,
  '.protectorate.v1.ApprovalResolved': ApprovalResolved$json,
  '.protectorate.v1.UsageUpdate': UsageUpdate$json,
  '.protectorate.v1.TurnSettled': TurnSettled$json,
  '.protectorate.v1.Compaction': Compaction$json,
  '.protectorate.v1.ChatError': ChatError$json,
  '.protectorate.v1.StateChange': StateChange$json,
  '.protectorate.v1.Handoff': Handoff$json,
  '.protectorate.v1.ModelSettings': ModelSettings$json,
  '.protectorate.v1.ModelSelection': ModelSelection$json,
  '.protectorate.v1.SleeveResult': SleeveResult$json,
  '.protectorate.v1.SendRequest': SendRequest$json,
  '.protectorate.v1.SendResponse': SendResponse$json,
  '.protectorate.v1.InterruptRequest': InterruptRequest$json,
  '.protectorate.v1.InterruptResponse': InterruptResponse$json,
  '.protectorate.v1.RespondRequest': RespondRequest$json,
  '.protectorate.v1.RespondResponse': RespondResponse$json,
  '.protectorate.v1.CompactRequest': CompactRequest$json,
  '.protectorate.v1.CompactResponse': CompactResponse$json,
  '.protectorate.v1.ListModelsRequest': ListModelsRequest$json,
  '.protectorate.v1.ListModelsResponse': ListModelsResponse$json,
  '.protectorate.v1.ModelOption': ModelOption$json,
  '.protectorate.v1.SetSelectionRequest': SetSelectionRequest$json,
  '.protectorate.v1.SetSelectionResponse': SetSelectionResponse$json,
  '.protectorate.v1.ChatStatus': ChatStatus$json,
  '.protectorate.v1.ReportRequest': ReportRequest$json,
  '.protectorate.v1.ReportResponse': ReportResponse$json,
  '.protectorate.v1.WaitRequest': WaitRequest$json,
  '.google.protobuf.Duration': $1.Duration$json,
  '.protectorate.v1.WaitResponse': WaitResponse$json,
};

/// Descriptor for `ChatService`. Decode as a `google.protobuf.ServiceDescriptorProto`.
final $typed_data.Uint8List chatServiceDescriptor = $convert.base64Decode(
    'CgtDaGF0U2VydmljZRJJCgVXYXRjaBIdLnByb3RlY3RvcmF0ZS52MS5XYXRjaFJlcXVlc3QaGi'
    '5wcm90ZWN0b3JhdGUudjEuQ2hhdEV2ZW50IgOQAgEwARJDCgRTZW5kEhwucHJvdGVjdG9yYXRl'
    'LnYxLlNlbmRSZXF1ZXN0Gh0ucHJvdGVjdG9yYXRlLnYxLlNlbmRSZXNwb25zZRJSCglJbnRlcn'
    'J1cHQSIS5wcm90ZWN0b3JhdGUudjEuSW50ZXJydXB0UmVxdWVzdBoiLnByb3RlY3RvcmF0ZS52'
    'MS5JbnRlcnJ1cHRSZXNwb25zZRJMCgdSZXNwb25kEh8ucHJvdGVjdG9yYXRlLnYxLlJlc3Bvbm'
    'RSZXF1ZXN0GiAucHJvdGVjdG9yYXRlLnYxLlJlc3BvbmRSZXNwb25zZRJMCgdDb21wYWN0Eh8u'
    'cHJvdGVjdG9yYXRlLnYxLkNvbXBhY3RSZXF1ZXN0GiAucHJvdGVjdG9yYXRlLnYxLkNvbXBhY3'
    'RSZXNwb25zZRJaCgpMaXN0TW9kZWxzEiIucHJvdGVjdG9yYXRlLnYxLkxpc3RNb2RlbHNSZXF1'
    'ZXN0GiMucHJvdGVjdG9yYXRlLnYxLkxpc3RNb2RlbHNSZXNwb25zZSIDkAIBElsKDFNldFNlbG'
    'VjdGlvbhIkLnByb3RlY3RvcmF0ZS52MS5TZXRTZWxlY3Rpb25SZXF1ZXN0GiUucHJvdGVjdG9y'
    'YXRlLnYxLlNldFNlbGVjdGlvblJlc3BvbnNlEkkKBlJlcG9ydBIeLnByb3RlY3RvcmF0ZS52MS'
    '5SZXBvcnRSZXF1ZXN0Gh8ucHJvdGVjdG9yYXRlLnYxLlJlcG9ydFJlc3BvbnNlEkgKBFdhaXQS'
    'HC5wcm90ZWN0b3JhdGUudjEuV2FpdFJlcXVlc3QaHS5wcm90ZWN0b3JhdGUudjEuV2FpdFJlc3'
    'BvbnNlIgOQAgE=');
