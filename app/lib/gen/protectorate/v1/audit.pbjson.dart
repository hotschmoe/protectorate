// This is a generated file - do not edit.
//
// Generated from protectorate/v1/audit.proto.

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

@$core.Deprecated('Use listAuditEntriesRequestDescriptor instead')
const ListAuditEntriesRequest$json = {
  '1': 'ListAuditEntriesRequest',
  '2': [
    {'1': 'limit', '3': 1, '4': 1, '5': 5, '10': 'limit'},
    {'1': 'page_token', '3': 2, '4': 1, '5': 9, '10': 'pageToken'},
    {'1': 'actor', '3': 3, '4': 1, '5': 9, '10': 'actor'},
    {'1': 'procedure', '3': 4, '4': 1, '5': 9, '10': 'procedure'},
    {'1': 'subject', '3': 5, '4': 1, '5': 9, '10': 'subject'},
    {'1': 'outcome', '3': 6, '4': 1, '5': 9, '10': 'outcome'},
    {'1': 'run_id', '3': 7, '4': 1, '5': 9, '10': 'runId'},
  ],
};

/// Descriptor for `ListAuditEntriesRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List listAuditEntriesRequestDescriptor = $convert.base64Decode(
    'ChdMaXN0QXVkaXRFbnRyaWVzUmVxdWVzdBIUCgVsaW1pdBgBIAEoBVIFbGltaXQSHQoKcGFnZV'
    '90b2tlbhgCIAEoCVIJcGFnZVRva2VuEhQKBWFjdG9yGAMgASgJUgVhY3RvchIcCglwcm9jZWR1'
    'cmUYBCABKAlSCXByb2NlZHVyZRIYCgdzdWJqZWN0GAUgASgJUgdzdWJqZWN0EhgKB291dGNvbW'
    'UYBiABKAlSB291dGNvbWUSFQoGcnVuX2lkGAcgASgJUgVydW5JZA==');

@$core.Deprecated('Use listAuditEntriesResponseDescriptor instead')
const ListAuditEntriesResponse$json = {
  '1': 'ListAuditEntriesResponse',
  '2': [
    {
      '1': 'entries',
      '3': 1,
      '4': 3,
      '5': 11,
      '6': '.protectorate.v1.AuditEntry',
      '10': 'entries'
    },
    {'1': 'next_page_token', '3': 2, '4': 1, '5': 9, '10': 'nextPageToken'},
    {
      '1': 'head',
      '3': 3,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.AuditHead',
      '10': 'head'
    },
  ],
};

/// Descriptor for `ListAuditEntriesResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List listAuditEntriesResponseDescriptor = $convert.base64Decode(
    'ChhMaXN0QXVkaXRFbnRyaWVzUmVzcG9uc2USNQoHZW50cmllcxgBIAMoCzIbLnByb3RlY3Rvcm'
    'F0ZS52MS5BdWRpdEVudHJ5UgdlbnRyaWVzEiYKD25leHRfcGFnZV90b2tlbhgCIAEoCVINbmV4'
    'dFBhZ2VUb2tlbhIuCgRoZWFkGAMgASgLMhoucHJvdGVjdG9yYXRlLnYxLkF1ZGl0SGVhZFIEaG'
    'VhZA==');

@$core.Deprecated('Use auditHeadDescriptor instead')
const AuditHead$json = {
  '1': 'AuditHead',
  '2': [
    {'1': 'seq', '3': 1, '4': 1, '5': 4, '10': 'seq'},
    {'1': 'hash', '3': 2, '4': 1, '5': 9, '10': 'hash'},
  ],
};

/// Descriptor for `AuditHead`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List auditHeadDescriptor = $convert.base64Decode(
    'CglBdWRpdEhlYWQSEAoDc2VxGAEgASgEUgNzZXESEgoEaGFzaBgCIAEoCVIEaGFzaA==');

@$core.Deprecated('Use verifyAuditRequestDescriptor instead')
const VerifyAuditRequest$json = {
  '1': 'VerifyAuditRequest',
};

/// Descriptor for `VerifyAuditRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List verifyAuditRequestDescriptor =
    $convert.base64Decode('ChJWZXJpZnlBdWRpdFJlcXVlc3Q=');

@$core.Deprecated('Use verifyAuditResponseDescriptor instead')
const VerifyAuditResponse$json = {
  '1': 'VerifyAuditResponse',
  '2': [
    {'1': 'ok', '3': 1, '4': 1, '5': 8, '10': 'ok'},
    {'1': 'error', '3': 2, '4': 1, '5': 9, '10': 'error'},
    {'1': 'entries', '3': 3, '4': 1, '5': 4, '10': 'entries'},
    {
      '1': 'head',
      '3': 4,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.AuditHead',
      '10': 'head'
    },
    {'1': 'beyond_head', '3': 5, '4': 1, '5': 4, '10': 'beyondHead'},
  ],
};

/// Descriptor for `VerifyAuditResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List verifyAuditResponseDescriptor = $convert.base64Decode(
    'ChNWZXJpZnlBdWRpdFJlc3BvbnNlEg4KAm9rGAEgASgIUgJvaxIUCgVlcnJvchgCIAEoCVIFZX'
    'Jyb3ISGAoHZW50cmllcxgDIAEoBFIHZW50cmllcxIuCgRoZWFkGAQgASgLMhoucHJvdGVjdG9y'
    'YXRlLnYxLkF1ZGl0SGVhZFIEaGVhZBIfCgtiZXlvbmRfaGVhZBgFIAEoBFIKYmV5b25kSGVhZA'
    '==');

@$core.Deprecated('Use auditEntryDescriptor instead')
const AuditEntry$json = {
  '1': 'AuditEntry',
  '2': [
    {
      '1': 'time',
      '3': 1,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Timestamp',
      '10': 'time'
    },
    {'1': 'actor', '3': 2, '4': 1, '5': 9, '10': 'actor'},
    {'1': 'token_id', '3': 3, '4': 1, '5': 9, '10': 'tokenId'},
    {'1': 'procedure', '3': 5, '4': 1, '5': 9, '10': 'procedure'},
    {'1': 'subject', '3': 6, '4': 1, '5': 9, '10': 'subject'},
    {'1': 'outcome', '3': 7, '4': 1, '5': 9, '10': 'outcome'},
    {'1': 'code', '3': 8, '4': 1, '5': 9, '10': 'code'},
    {'1': 'task', '3': 9, '4': 1, '5': 9, '10': 'task'},
    {'1': 'detail', '3': 10, '4': 1, '5': 9, '10': 'detail'},
    {'1': 'run_id', '3': 11, '4': 1, '5': 9, '10': 'runId'},
    {'1': 'seq', '3': 12, '4': 1, '5': 4, '10': 'seq'},
    {'1': 'prev_hash', '3': 13, '4': 1, '5': 9, '10': 'prevHash'},
    {'1': 'hash', '3': 14, '4': 1, '5': 9, '10': 'hash'},
  ],
  '9': [
    {'1': 4, '2': 5},
  ],
};

/// Descriptor for `AuditEntry`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List auditEntryDescriptor = $convert.base64Decode(
    'CgpBdWRpdEVudHJ5Ei4KBHRpbWUYASABKAsyGi5nb29nbGUucHJvdG9idWYuVGltZXN0YW1wUg'
    'R0aW1lEhQKBWFjdG9yGAIgASgJUgVhY3RvchIZCgh0b2tlbl9pZBgDIAEoCVIHdG9rZW5JZBIc'
    'Cglwcm9jZWR1cmUYBSABKAlSCXByb2NlZHVyZRIYCgdzdWJqZWN0GAYgASgJUgdzdWJqZWN0Eh'
    'gKB291dGNvbWUYByABKAlSB291dGNvbWUSEgoEY29kZRgIIAEoCVIEY29kZRISCgR0YXNrGAkg'
    'ASgJUgR0YXNrEhYKBmRldGFpbBgKIAEoCVIGZGV0YWlsEhUKBnJ1bl9pZBgLIAEoCVIFcnVuSW'
    'QSEAoDc2VxGAwgASgEUgNzZXESGwoJcHJldl9oYXNoGA0gASgJUghwcmV2SGFzaBISCgRoYXNo'
    'GA4gASgJUgRoYXNoSgQIBBAF');

const $core.Map<$core.String, $core.dynamic> AuditServiceBase$json = {
  '1': 'AuditService',
  '2': [
    {
      '1': 'ListAuditEntries',
      '2': '.protectorate.v1.ListAuditEntriesRequest',
      '3': '.protectorate.v1.ListAuditEntriesResponse',
      '4': {'34': 1},
    },
    {
      '1': 'VerifyAudit',
      '2': '.protectorate.v1.VerifyAuditRequest',
      '3': '.protectorate.v1.VerifyAuditResponse',
      '4': {'34': 1},
    },
  ],
  '3': {},
};

@$core.Deprecated('Use auditServiceDescriptor instead')
const $core.Map<$core.String, $core.Map<$core.String, $core.dynamic>>
    AuditServiceBase$messageJson = {
  '.protectorate.v1.ListAuditEntriesRequest': ListAuditEntriesRequest$json,
  '.protectorate.v1.ListAuditEntriesResponse': ListAuditEntriesResponse$json,
  '.protectorate.v1.AuditEntry': AuditEntry$json,
  '.google.protobuf.Timestamp': $0.Timestamp$json,
  '.protectorate.v1.AuditHead': AuditHead$json,
  '.protectorate.v1.VerifyAuditRequest': VerifyAuditRequest$json,
  '.protectorate.v1.VerifyAuditResponse': VerifyAuditResponse$json,
};

/// Descriptor for `AuditService`. Decode as a `google.protobuf.ServiceDescriptorProto`.
final $typed_data.Uint8List auditServiceDescriptor = $convert.base64Decode(
    'CgxBdWRpdFNlcnZpY2USbAoQTGlzdEF1ZGl0RW50cmllcxIoLnByb3RlY3RvcmF0ZS52MS5MaX'
    'N0QXVkaXRFbnRyaWVzUmVxdWVzdBopLnByb3RlY3RvcmF0ZS52MS5MaXN0QXVkaXRFbnRyaWVz'
    'UmVzcG9uc2UiA5ACARJdCgtWZXJpZnlBdWRpdBIjLnByb3RlY3RvcmF0ZS52MS5WZXJpZnlBdW'
    'RpdFJlcXVlc3QaJC5wcm90ZWN0b3JhdGUudjEuVmVyaWZ5QXVkaXRSZXNwb25zZSIDkAIBGgSY'
    'tRgB');
