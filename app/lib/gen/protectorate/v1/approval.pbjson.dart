// This is a generated file - do not edit.
//
// Generated from protectorate/v1/approval.proto.

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

@$core.Deprecated('Use approvalStateDescriptor instead')
const ApprovalState$json = {
  '1': 'ApprovalState',
  '2': [
    {'1': 'APPROVAL_STATE_UNSPECIFIED', '2': 0},
    {'1': 'APPROVAL_STATE_PENDING', '2': 1},
    {'1': 'APPROVAL_STATE_APPROVED', '2': 2},
    {'1': 'APPROVAL_STATE_DENIED', '2': 3},
    {'1': 'APPROVAL_STATE_EXPIRED', '2': 4},
    {'1': 'APPROVAL_STATE_USED', '2': 5},
  ],
};

/// Descriptor for `ApprovalState`. Decode as a `google.protobuf.EnumDescriptorProto`.
final $typed_data.Uint8List approvalStateDescriptor = $convert.base64Decode(
    'Cg1BcHByb3ZhbFN0YXRlEh4KGkFQUFJPVkFMX1NUQVRFX1VOU1BFQ0lGSUVEEAASGgoWQVBQUk'
    '9WQUxfU1RBVEVfUEVORElORxABEhsKF0FQUFJPVkFMX1NUQVRFX0FQUFJPVkVEEAISGQoVQVBQ'
    'Uk9WQUxfU1RBVEVfREVOSUVEEAMSGgoWQVBQUk9WQUxfU1RBVEVfRVhQSVJFRBAEEhcKE0FQUF'
    'JPVkFMX1NUQVRFX1VTRUQQBQ==');

@$core.Deprecated('Use approvalDescriptor instead')
const Approval$json = {
  '1': 'Approval',
  '2': [
    {'1': 'id', '3': 1, '4': 1, '5': 9, '10': 'id'},
    {'1': 'actor', '3': 2, '4': 1, '5': 9, '10': 'actor'},
    {'1': 'procedure', '3': 3, '4': 1, '5': 9, '10': 'procedure'},
    {'1': 'subject', '3': 4, '4': 1, '5': 9, '10': 'subject'},
    {'1': 'reason', '3': 5, '4': 1, '5': 9, '10': 'reason'},
    {'1': 'detail', '3': 6, '4': 1, '5': 9, '10': 'detail'},
    {'1': 'run_id', '3': 7, '4': 1, '5': 9, '10': 'runId'},
    {
      '1': 'state',
      '3': 8,
      '4': 1,
      '5': 14,
      '6': '.protectorate.v1.ApprovalState',
      '10': 'state'
    },
    {
      '1': 'created_at',
      '3': 9,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Timestamp',
      '10': 'createdAt'
    },
    {
      '1': 'expires_at',
      '3': 10,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Timestamp',
      '10': 'expiresAt'
    },
    {'1': 'decided_by', '3': 11, '4': 1, '5': 9, '10': 'decidedBy'},
  ],
};

/// Descriptor for `Approval`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List approvalDescriptor = $convert.base64Decode(
    'CghBcHByb3ZhbBIOCgJpZBgBIAEoCVICaWQSFAoFYWN0b3IYAiABKAlSBWFjdG9yEhwKCXByb2'
    'NlZHVyZRgDIAEoCVIJcHJvY2VkdXJlEhgKB3N1YmplY3QYBCABKAlSB3N1YmplY3QSFgoGcmVh'
    'c29uGAUgASgJUgZyZWFzb24SFgoGZGV0YWlsGAYgASgJUgZkZXRhaWwSFQoGcnVuX2lkGAcgAS'
    'gJUgVydW5JZBI0CgVzdGF0ZRgIIAEoDjIeLnByb3RlY3RvcmF0ZS52MS5BcHByb3ZhbFN0YXRl'
    'UgVzdGF0ZRI5CgpjcmVhdGVkX2F0GAkgASgLMhouZ29vZ2xlLnByb3RvYnVmLlRpbWVzdGFtcF'
    'IJY3JlYXRlZEF0EjkKCmV4cGlyZXNfYXQYCiABKAsyGi5nb29nbGUucHJvdG9idWYuVGltZXN0'
    'YW1wUglleHBpcmVzQXQSHQoKZGVjaWRlZF9ieRgLIAEoCVIJZGVjaWRlZEJ5');

@$core.Deprecated('Use listApprovalsRequestDescriptor instead')
const ListApprovalsRequest$json = {
  '1': 'ListApprovalsRequest',
  '2': [
    {'1': 'all', '3': 1, '4': 1, '5': 8, '10': 'all'},
  ],
};

/// Descriptor for `ListApprovalsRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List listApprovalsRequestDescriptor = $convert
    .base64Decode('ChRMaXN0QXBwcm92YWxzUmVxdWVzdBIQCgNhbGwYASABKAhSA2FsbA==');

@$core.Deprecated('Use listApprovalsResponseDescriptor instead')
const ListApprovalsResponse$json = {
  '1': 'ListApprovalsResponse',
  '2': [
    {
      '1': 'approvals',
      '3': 1,
      '4': 3,
      '5': 11,
      '6': '.protectorate.v1.Approval',
      '10': 'approvals'
    },
  ],
};

/// Descriptor for `ListApprovalsResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List listApprovalsResponseDescriptor = $convert.base64Decode(
    'ChVMaXN0QXBwcm92YWxzUmVzcG9uc2USNwoJYXBwcm92YWxzGAEgAygLMhkucHJvdGVjdG9yYX'
    'RlLnYxLkFwcHJvdmFsUglhcHByb3ZhbHM=');

@$core.Deprecated('Use decideApprovalRequestDescriptor instead')
const DecideApprovalRequest$json = {
  '1': 'DecideApprovalRequest',
  '2': [
    {'1': 'id', '3': 1, '4': 1, '5': 9, '10': 'id'},
    {'1': 'approve', '3': 2, '4': 1, '5': 8, '10': 'approve'},
  ],
};

/// Descriptor for `DecideApprovalRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List decideApprovalRequestDescriptor = $convert.base64Decode(
    'ChVEZWNpZGVBcHByb3ZhbFJlcXVlc3QSDgoCaWQYASABKAlSAmlkEhgKB2FwcHJvdmUYAiABKA'
    'hSB2FwcHJvdmU=');

@$core.Deprecated('Use decideApprovalResponseDescriptor instead')
const DecideApprovalResponse$json = {
  '1': 'DecideApprovalResponse',
  '2': [
    {
      '1': 'approval',
      '3': 1,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.Approval',
      '10': 'approval'
    },
  ],
};

/// Descriptor for `DecideApprovalResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List decideApprovalResponseDescriptor =
    $convert.base64Decode(
        'ChZEZWNpZGVBcHByb3ZhbFJlc3BvbnNlEjUKCGFwcHJvdmFsGAEgASgLMhkucHJvdGVjdG9yYX'
        'RlLnYxLkFwcHJvdmFsUghhcHByb3ZhbA==');

const $core.Map<$core.String, $core.dynamic> ApprovalServiceBase$json = {
  '1': 'ApprovalService',
  '2': [
    {
      '1': 'ListApprovals',
      '2': '.protectorate.v1.ListApprovalsRequest',
      '3': '.protectorate.v1.ListApprovalsResponse',
      '4': {'34': 1},
    },
    {
      '1': 'DecideApproval',
      '2': '.protectorate.v1.DecideApprovalRequest',
      '3': '.protectorate.v1.DecideApprovalResponse',
      '4': {}
    },
  ],
  '3': {},
};

@$core.Deprecated('Use approvalServiceDescriptor instead')
const $core.Map<$core.String, $core.Map<$core.String, $core.dynamic>>
    ApprovalServiceBase$messageJson = {
  '.protectorate.v1.ListApprovalsRequest': ListApprovalsRequest$json,
  '.protectorate.v1.ListApprovalsResponse': ListApprovalsResponse$json,
  '.protectorate.v1.Approval': Approval$json,
  '.google.protobuf.Timestamp': $0.Timestamp$json,
  '.protectorate.v1.DecideApprovalRequest': DecideApprovalRequest$json,
  '.protectorate.v1.DecideApprovalResponse': DecideApprovalResponse$json,
};

/// Descriptor for `ApprovalService`. Decode as a `google.protobuf.ServiceDescriptorProto`.
final $typed_data.Uint8List approvalServiceDescriptor = $convert.base64Decode(
    'Cg9BcHByb3ZhbFNlcnZpY2USZwoNTGlzdEFwcHJvdmFscxIlLnByb3RlY3RvcmF0ZS52MS5MaX'
    'N0QXBwcm92YWxzUmVxdWVzdBomLnByb3RlY3RvcmF0ZS52MS5MaXN0QXBwcm92YWxzUmVzcG9u'
    'c2UiB5ACAai1GAESZwoORGVjaWRlQXBwcm92YWwSJi5wcm90ZWN0b3JhdGUudjEuRGVjaWRlQX'
    'Bwcm92YWxSZXF1ZXN0GicucHJvdGVjdG9yYXRlLnYxLkRlY2lkZUFwcHJvdmFsUmVzcG9uc2Ui'
    'BKi1GAEaBJi1GAE=');
