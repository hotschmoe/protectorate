// This is a generated file - do not edit.
//
// Generated from protectorate/v1/lease.proto.

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
    as $0;
import 'package:protobuf/well_known_types/google/protobuf/timestamp.pbjson.dart'
    as $1;

@$core.Deprecated('Use leaseStateDescriptor instead')
const LeaseState$json = {
  '1': 'LeaseState',
  '2': [
    {'1': 'LEASE_STATE_UNSPECIFIED', '2': 0},
    {'1': 'LEASE_STATE_QUEUED', '2': 1},
  ],
};

/// Descriptor for `LeaseState`. Decode as a `google.protobuf.EnumDescriptorProto`.
final $typed_data.Uint8List leaseStateDescriptor = $convert.base64Decode(
    'CgpMZWFzZVN0YXRlEhsKF0xFQVNFX1NUQVRFX1VOU1BFQ0lGSUVEEAASFgoSTEVBU0VfU1RBVE'
    'VfUVVFVUVEEAE=');

@$core.Deprecated('Use nodeSelectorDescriptor instead')
const NodeSelector$json = {
  '1': 'NodeSelector',
  '2': [
    {'1': 'os', '3': 1, '4': 1, '5': 9, '10': 'os'},
    {'1': 'arch', '3': 2, '4': 1, '5': 9, '10': 'arch'},
    {
      '1': 'min_free_memory_bytes',
      '3': 3,
      '4': 1,
      '5': 4,
      '10': 'minFreeMemoryBytes'
    },
    {'1': 'gpu_vram_bytes', '3': 4, '4': 1, '5': 4, '10': 'gpuVramBytes'},
    {'1': 'gpu', '3': 5, '4': 1, '5': 9, '10': 'gpu'},
    {'1': 'npu', '3': 6, '4': 1, '5': 9, '10': 'npu'},
  ],
};

/// Descriptor for `NodeSelector`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List nodeSelectorDescriptor = $convert.base64Decode(
    'CgxOb2RlU2VsZWN0b3ISDgoCb3MYASABKAlSAm9zEhIKBGFyY2gYAiABKAlSBGFyY2gSMQoVbW'
    'luX2ZyZWVfbWVtb3J5X2J5dGVzGAMgASgEUhJtaW5GcmVlTWVtb3J5Qnl0ZXMSJAoOZ3B1X3Zy'
    'YW1fYnl0ZXMYBCABKARSDGdwdVZyYW1CeXRlcxIQCgNncHUYBSABKAlSA2dwdRIQCgNucHUYBi'
    'ABKAlSA25wdQ==');

@$core.Deprecated('Use acquireLeaseRequestDescriptor instead')
const AcquireLeaseRequest$json = {
  '1': 'AcquireLeaseRequest',
  '2': [
    {'1': 'resource', '3': 1, '4': 1, '5': 9, '9': 0, '10': 'resource'},
    {
      '1': 'selector',
      '3': 2,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.NodeSelector',
      '9': 0,
      '10': 'selector'
    },
    {'1': 'holder', '3': 3, '4': 1, '5': 9, '10': 'holder'},
    {
      '1': 'ttl',
      '3': 4,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Duration',
      '10': 'ttl'
    },
    {'1': 'allow_family', '3': 5, '4': 1, '5': 8, '10': 'allowFamily'},
    {'1': 'wait', '3': 6, '4': 1, '5': 8, '10': 'wait'},
    {'1': 'priority', '3': 7, '4': 1, '5': 5, '10': 'priority'},
  ],
  '8': [
    {'1': 'target'},
  ],
};

/// Descriptor for `AcquireLeaseRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List acquireLeaseRequestDescriptor = $convert.base64Decode(
    'ChNBY3F1aXJlTGVhc2VSZXF1ZXN0EhwKCHJlc291cmNlGAEgASgJSABSCHJlc291cmNlEjsKCH'
    'NlbGVjdG9yGAIgASgLMh0ucHJvdGVjdG9yYXRlLnYxLk5vZGVTZWxlY3RvckgAUghzZWxlY3Rv'
    'chIWCgZob2xkZXIYAyABKAlSBmhvbGRlchIrCgN0dGwYBCABKAsyGS5nb29nbGUucHJvdG9idW'
    'YuRHVyYXRpb25SA3R0bBIhCgxhbGxvd19mYW1pbHkYBSABKAhSC2FsbG93RmFtaWx5EhIKBHdh'
    'aXQYBiABKAhSBHdhaXQSGgoIcHJpb3JpdHkYByABKAVSCHByaW9yaXR5QggKBnRhcmdldA==');

@$core.Deprecated('Use renewLeaseRequestDescriptor instead')
const RenewLeaseRequest$json = {
  '1': 'RenewLeaseRequest',
  '2': [
    {'1': 'id', '3': 1, '4': 1, '5': 9, '10': 'id'},
    {
      '1': 'ttl',
      '3': 2,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Duration',
      '10': 'ttl'
    },
  ],
};

/// Descriptor for `RenewLeaseRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List renewLeaseRequestDescriptor = $convert.base64Decode(
    'ChFSZW5ld0xlYXNlUmVxdWVzdBIOCgJpZBgBIAEoCVICaWQSKwoDdHRsGAIgASgLMhkuZ29vZ2'
    'xlLnByb3RvYnVmLkR1cmF0aW9uUgN0dGw=');

@$core.Deprecated('Use releaseLeaseRequestDescriptor instead')
const ReleaseLeaseRequest$json = {
  '1': 'ReleaseLeaseRequest',
  '2': [
    {'1': 'id', '3': 1, '4': 1, '5': 9, '10': 'id'},
  ],
};

/// Descriptor for `ReleaseLeaseRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List releaseLeaseRequestDescriptor = $convert
    .base64Decode('ChNSZWxlYXNlTGVhc2VSZXF1ZXN0Eg4KAmlkGAEgASgJUgJpZA==');

@$core.Deprecated('Use releaseLeaseResponseDescriptor instead')
const ReleaseLeaseResponse$json = {
  '1': 'ReleaseLeaseResponse',
};

/// Descriptor for `ReleaseLeaseResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List releaseLeaseResponseDescriptor =
    $convert.base64Decode('ChRSZWxlYXNlTGVhc2VSZXNwb25zZQ==');

@$core.Deprecated('Use listLeasesRequestDescriptor instead')
const ListLeasesRequest$json = {
  '1': 'ListLeasesRequest',
};

/// Descriptor for `ListLeasesRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List listLeasesRequestDescriptor =
    $convert.base64Decode('ChFMaXN0TGVhc2VzUmVxdWVzdA==');

@$core.Deprecated('Use listLeasesResponseDescriptor instead')
const ListLeasesResponse$json = {
  '1': 'ListLeasesResponse',
  '2': [
    {
      '1': 'leases',
      '3': 1,
      '4': 3,
      '5': 11,
      '6': '.protectorate.v1.Lease',
      '10': 'leases'
    },
  ],
};

/// Descriptor for `ListLeasesResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List listLeasesResponseDescriptor = $convert.base64Decode(
    'ChJMaXN0TGVhc2VzUmVzcG9uc2USLgoGbGVhc2VzGAEgAygLMhYucHJvdGVjdG9yYXRlLnYxLk'
    'xlYXNlUgZsZWFzZXM=');

@$core.Deprecated('Use leaseDescriptor instead')
const Lease$json = {
  '1': 'Lease',
  '2': [
    {'1': 'id', '3': 1, '4': 1, '5': 9, '10': 'id'},
    {'1': 'resource', '3': 2, '4': 1, '5': 9, '10': 'resource'},
    {'1': 'node', '3': 3, '4': 1, '5': 9, '10': 'node'},
    {
      '1': 'gpu_index',
      '3': 4,
      '4': 1,
      '5': 5,
      '9': 0,
      '10': 'gpuIndex',
      '17': true
    },
    {'1': 'holder', '3': 5, '4': 1, '5': 9, '10': 'holder'},
    {
      '1': 'ttl',
      '3': 6,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Duration',
      '10': 'ttl'
    },
    {
      '1': 'acquired_at',
      '3': 7,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Timestamp',
      '10': 'acquiredAt'
    },
    {
      '1': 'expires_at',
      '3': 8,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Timestamp',
      '10': 'expiresAt'
    },
    {
      '1': 'state',
      '3': 9,
      '4': 1,
      '5': 14,
      '6': '.protectorate.v1.LeaseState',
      '10': 'state'
    },
    {'1': 'queue_position', '3': 10, '4': 1, '5': 5, '10': 'queuePosition'},
  ],
  '8': [
    {'1': '_gpu_index'},
  ],
};

/// Descriptor for `Lease`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List leaseDescriptor = $convert.base64Decode(
    'CgVMZWFzZRIOCgJpZBgBIAEoCVICaWQSGgoIcmVzb3VyY2UYAiABKAlSCHJlc291cmNlEhIKBG'
    '5vZGUYAyABKAlSBG5vZGUSIAoJZ3B1X2luZGV4GAQgASgFSABSCGdwdUluZGV4iAEBEhYKBmhv'
    'bGRlchgFIAEoCVIGaG9sZGVyEisKA3R0bBgGIAEoCzIZLmdvb2dsZS5wcm90b2J1Zi5EdXJhdG'
    'lvblIDdHRsEjsKC2FjcXVpcmVkX2F0GAcgASgLMhouZ29vZ2xlLnByb3RvYnVmLlRpbWVzdGFt'
    'cFIKYWNxdWlyZWRBdBI5CgpleHBpcmVzX2F0GAggASgLMhouZ29vZ2xlLnByb3RvYnVmLlRpbW'
    'VzdGFtcFIJZXhwaXJlc0F0EjEKBXN0YXRlGAkgASgOMhsucHJvdGVjdG9yYXRlLnYxLkxlYXNl'
    'U3RhdGVSBXN0YXRlEiUKDnF1ZXVlX3Bvc2l0aW9uGAogASgFUg1xdWV1ZVBvc2l0aW9uQgwKCl'
    '9ncHVfaW5kZXg=');

const $core.Map<$core.String, $core.dynamic> LeaseServiceBase$json = {
  '1': 'LeaseService',
  '2': [
    {
      '1': 'AcquireLease',
      '2': '.protectorate.v1.AcquireLeaseRequest',
      '3': '.protectorate.v1.Lease'
    },
    {
      '1': 'RenewLease',
      '2': '.protectorate.v1.RenewLeaseRequest',
      '3': '.protectorate.v1.Lease'
    },
    {
      '1': 'ReleaseLease',
      '2': '.protectorate.v1.ReleaseLeaseRequest',
      '3': '.protectorate.v1.ReleaseLeaseResponse'
    },
    {
      '1': 'ListLeases',
      '2': '.protectorate.v1.ListLeasesRequest',
      '3': '.protectorate.v1.ListLeasesResponse',
      '4': {'34': 1},
    },
  ],
};

@$core.Deprecated('Use leaseServiceDescriptor instead')
const $core.Map<$core.String, $core.Map<$core.String, $core.dynamic>>
    LeaseServiceBase$messageJson = {
  '.protectorate.v1.AcquireLeaseRequest': AcquireLeaseRequest$json,
  '.protectorate.v1.NodeSelector': NodeSelector$json,
  '.google.protobuf.Duration': $0.Duration$json,
  '.protectorate.v1.Lease': Lease$json,
  '.google.protobuf.Timestamp': $1.Timestamp$json,
  '.protectorate.v1.RenewLeaseRequest': RenewLeaseRequest$json,
  '.protectorate.v1.ReleaseLeaseRequest': ReleaseLeaseRequest$json,
  '.protectorate.v1.ReleaseLeaseResponse': ReleaseLeaseResponse$json,
  '.protectorate.v1.ListLeasesRequest': ListLeasesRequest$json,
  '.protectorate.v1.ListLeasesResponse': ListLeasesResponse$json,
};

/// Descriptor for `LeaseService`. Decode as a `google.protobuf.ServiceDescriptorProto`.
final $typed_data.Uint8List leaseServiceDescriptor = $convert.base64Decode(
    'CgxMZWFzZVNlcnZpY2USTAoMQWNxdWlyZUxlYXNlEiQucHJvdGVjdG9yYXRlLnYxLkFjcXVpcm'
    'VMZWFzZVJlcXVlc3QaFi5wcm90ZWN0b3JhdGUudjEuTGVhc2USSAoKUmVuZXdMZWFzZRIiLnBy'
    'b3RlY3RvcmF0ZS52MS5SZW5ld0xlYXNlUmVxdWVzdBoWLnByb3RlY3RvcmF0ZS52MS5MZWFzZR'
    'JbCgxSZWxlYXNlTGVhc2USJC5wcm90ZWN0b3JhdGUudjEuUmVsZWFzZUxlYXNlUmVxdWVzdBol'
    'LnByb3RlY3RvcmF0ZS52MS5SZWxlYXNlTGVhc2VSZXNwb25zZRJaCgpMaXN0TGVhc2VzEiIucH'
    'JvdGVjdG9yYXRlLnYxLkxpc3RMZWFzZXNSZXF1ZXN0GiMucHJvdGVjdG9yYXRlLnYxLkxpc3RM'
    'ZWFzZXNSZXNwb25zZSIDkAIB');
