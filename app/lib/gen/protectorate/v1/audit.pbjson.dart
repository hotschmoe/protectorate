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
  ],
};

/// Descriptor for `ListAuditEntriesRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List listAuditEntriesRequestDescriptor = $convert.base64Decode(
    'ChdMaXN0QXVkaXRFbnRyaWVzUmVxdWVzdBIUCgVsaW1pdBgBIAEoBVIFbGltaXQSHQoKcGFnZV'
    '90b2tlbhgCIAEoCVIJcGFnZVRva2VuEhQKBWFjdG9yGAMgASgJUgVhY3RvchIcCglwcm9jZWR1'
    'cmUYBCABKAlSCXByb2NlZHVyZRIYCgdzdWJqZWN0GAUgASgJUgdzdWJqZWN0EhgKB291dGNvbW'
    'UYBiABKAlSB291dGNvbWU=');

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
  ],
};

/// Descriptor for `ListAuditEntriesResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List listAuditEntriesResponseDescriptor = $convert.base64Decode(
    'ChhMaXN0QXVkaXRFbnRyaWVzUmVzcG9uc2USNQoHZW50cmllcxgBIAMoCzIbLnByb3RlY3Rvcm'
    'F0ZS52MS5BdWRpdEVudHJ5UgdlbnRyaWVzEiYKD25leHRfcGFnZV90b2tlbhgCIAEoCVINbmV4'
    'dFBhZ2VUb2tlbg==');

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
    'ASgJUgR0YXNrEhYKBmRldGFpbBgKIAEoCVIGZGV0YWlsSgQIBBAF');

const $core.Map<$core.String, $core.dynamic> AuditServiceBase$json = {
  '1': 'AuditService',
  '2': [
    {
      '1': 'ListAuditEntries',
      '2': '.protectorate.v1.ListAuditEntriesRequest',
      '3': '.protectorate.v1.ListAuditEntriesResponse',
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
};

/// Descriptor for `AuditService`. Decode as a `google.protobuf.ServiceDescriptorProto`.
final $typed_data.Uint8List auditServiceDescriptor = $convert.base64Decode(
    'CgxBdWRpdFNlcnZpY2USbAoQTGlzdEF1ZGl0RW50cmllcxIoLnByb3RlY3RvcmF0ZS52MS5MaX'
    'N0QXVkaXRFbnRyaWVzUmVxdWVzdBopLnByb3RlY3RvcmF0ZS52MS5MaXN0QXVkaXRFbnRyaWVz'
    'UmVzcG9uc2UiA5ACARoEmLUYAQ==');
