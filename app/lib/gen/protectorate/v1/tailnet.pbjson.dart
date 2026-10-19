// This is a generated file - do not edit.
//
// Generated from protectorate/v1/tailnet.proto.

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

@$core.Deprecated('Use tailnetServiceGetStatusRequestDescriptor instead')
const TailnetServiceGetStatusRequest$json = {
  '1': 'TailnetServiceGetStatusRequest',
};

/// Descriptor for `TailnetServiceGetStatusRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List tailnetServiceGetStatusRequestDescriptor =
    $convert.base64Decode('Ch5UYWlsbmV0U2VydmljZUdldFN0YXR1c1JlcXVlc3Q=');

@$core.Deprecated('Use tailnetServiceGetStatusResponseDescriptor instead')
const TailnetServiceGetStatusResponse$json = {
  '1': 'TailnetServiceGetStatusResponse',
  '2': [
    {'1': 'backend_state', '3': 1, '4': 1, '5': 9, '10': 'backendState'},
    {
      '1': 'self',
      '3': 2,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.TailnetPeer',
      '10': 'self'
    },
    {
      '1': 'peers',
      '3': 3,
      '4': 3,
      '5': 11,
      '6': '.protectorate.v1.TailnetPeer',
      '10': 'peers'
    },
    {'1': 'health', '3': 4, '4': 3, '5': 9, '10': 'health'},
  ],
};

/// Descriptor for `TailnetServiceGetStatusResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List tailnetServiceGetStatusResponseDescriptor =
    $convert.base64Decode(
        'Ch9UYWlsbmV0U2VydmljZUdldFN0YXR1c1Jlc3BvbnNlEiMKDWJhY2tlbmRfc3RhdGUYASABKA'
        'lSDGJhY2tlbmRTdGF0ZRIwCgRzZWxmGAIgASgLMhwucHJvdGVjdG9yYXRlLnYxLlRhaWxuZXRQ'
        'ZWVyUgRzZWxmEjIKBXBlZXJzGAMgAygLMhwucHJvdGVjdG9yYXRlLnYxLlRhaWxuZXRQZWVyUg'
        'VwZWVycxIWCgZoZWFsdGgYBCADKAlSBmhlYWx0aA==');

@$core.Deprecated('Use tailnetPeerDescriptor instead')
const TailnetPeer$json = {
  '1': 'TailnetPeer',
  '2': [
    {'1': 'name', '3': 1, '4': 1, '5': 9, '10': 'name'},
    {'1': 'tags', '3': 2, '4': 3, '5': 9, '10': 'tags'},
    {'1': 'ips', '3': 3, '4': 3, '5': 9, '10': 'ips'},
    {'1': 'online', '3': 4, '4': 1, '5': 8, '10': 'online'},
    {
      '1': 'last_handshake',
      '3': 5,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Timestamp',
      '10': 'lastHandshake'
    },
    {
      '1': 'last_seen',
      '3': 6,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Timestamp',
      '10': 'lastSeen'
    },
    {'1': 'direct_addr', '3': 7, '4': 1, '5': 9, '10': 'directAddr'},
    {'1': 'relay', '3': 8, '4': 1, '5': 9, '10': 'relay'},
    {'1': 'rx_bytes', '3': 9, '4': 1, '5': 3, '10': 'rxBytes'},
    {'1': 'tx_bytes', '3': 10, '4': 1, '5': 3, '10': 'txBytes'},
  ],
};

/// Descriptor for `TailnetPeer`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List tailnetPeerDescriptor = $convert.base64Decode(
    'CgtUYWlsbmV0UGVlchISCgRuYW1lGAEgASgJUgRuYW1lEhIKBHRhZ3MYAiADKAlSBHRhZ3MSEA'
    'oDaXBzGAMgAygJUgNpcHMSFgoGb25saW5lGAQgASgIUgZvbmxpbmUSQQoObGFzdF9oYW5kc2hh'
    'a2UYBSABKAsyGi5nb29nbGUucHJvdG9idWYuVGltZXN0YW1wUg1sYXN0SGFuZHNoYWtlEjcKCW'
    'xhc3Rfc2VlbhgGIAEoCzIaLmdvb2dsZS5wcm90b2J1Zi5UaW1lc3RhbXBSCGxhc3RTZWVuEh8K'
    'C2RpcmVjdF9hZGRyGAcgASgJUgpkaXJlY3RBZGRyEhQKBXJlbGF5GAggASgJUgVyZWxheRIZCg'
    'hyeF9ieXRlcxgJIAEoA1IHcnhCeXRlcxIZCgh0eF9ieXRlcxgKIAEoA1IHdHhCeXRlcw==');

const $core.Map<$core.String, $core.dynamic> TailnetServiceBase$json = {
  '1': 'TailnetService',
  '2': [
    {
      '1': 'GetStatus',
      '2': '.protectorate.v1.TailnetServiceGetStatusRequest',
      '3': '.protectorate.v1.TailnetServiceGetStatusResponse',
      '4': {'34': 1},
    },
  ],
};

@$core.Deprecated('Use tailnetServiceDescriptor instead')
const $core.Map<$core.String, $core.Map<$core.String, $core.dynamic>>
    TailnetServiceBase$messageJson = {
  '.protectorate.v1.TailnetServiceGetStatusRequest':
      TailnetServiceGetStatusRequest$json,
  '.protectorate.v1.TailnetServiceGetStatusResponse':
      TailnetServiceGetStatusResponse$json,
  '.protectorate.v1.TailnetPeer': TailnetPeer$json,
  '.google.protobuf.Timestamp': $0.Timestamp$json,
};

/// Descriptor for `TailnetService`. Decode as a `google.protobuf.ServiceDescriptorProto`.
final $typed_data.Uint8List tailnetServiceDescriptor = $convert.base64Decode(
    'Cg5UYWlsbmV0U2VydmljZRJzCglHZXRTdGF0dXMSLy5wcm90ZWN0b3JhdGUudjEuVGFpbG5ldF'
    'NlcnZpY2VHZXRTdGF0dXNSZXF1ZXN0GjAucHJvdGVjdG9yYXRlLnYxLlRhaWxuZXRTZXJ2aWNl'
    'R2V0U3RhdHVzUmVzcG9uc2UiA5ACAQ==');
