// This is a generated file - do not edit.
//
// Generated from protectorate/v1/release.proto.

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

@$core.Deprecated('Use checkReleaseRequestDescriptor instead')
const CheckReleaseRequest$json = {
  '1': 'CheckReleaseRequest',
  '2': [
    {'1': 'channel', '3': 1, '4': 1, '5': 9, '10': 'channel'},
    {'1': 'version', '3': 2, '4': 1, '5': 9, '10': 'version'},
  ],
};

/// Descriptor for `CheckReleaseRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List checkReleaseRequestDescriptor = $convert.base64Decode(
    'ChNDaGVja1JlbGVhc2VSZXF1ZXN0EhgKB2NoYW5uZWwYASABKAlSB2NoYW5uZWwSGAoHdmVyc2'
    'lvbhgCIAEoCVIHdmVyc2lvbg==');

@$core.Deprecated('Use releaseInfoDescriptor instead')
const ReleaseInfo$json = {
  '1': 'ReleaseInfo',
  '2': [
    {'1': 'version', '3': 1, '4': 1, '5': 9, '10': 'version'},
    {'1': 'channel', '3': 2, '4': 1, '5': 9, '10': 'channel'},
    {'1': 'notes_url', '3': 3, '4': 1, '5': 9, '10': 'notesUrl'},
    {'1': 'can_update', '3': 4, '4': 1, '5': 8, '10': 'canUpdate'},
  ],
};

/// Descriptor for `ReleaseInfo`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List releaseInfoDescriptor = $convert.base64Decode(
    'CgtSZWxlYXNlSW5mbxIYCgd2ZXJzaW9uGAEgASgJUgd2ZXJzaW9uEhgKB2NoYW5uZWwYAiABKA'
    'lSB2NoYW5uZWwSGwoJbm90ZXNfdXJsGAMgASgJUghub3Rlc1VybBIdCgpjYW5fdXBkYXRlGAQg'
    'ASgIUgljYW5VcGRhdGU=');

@$core.Deprecated('Use updateHubRequestDescriptor instead')
const UpdateHubRequest$json = {
  '1': 'UpdateHubRequest',
  '2': [
    {'1': 'channel', '3': 1, '4': 1, '5': 9, '10': 'channel'},
    {'1': 'version', '3': 2, '4': 1, '5': 9, '10': 'version'},
  ],
};

/// Descriptor for `UpdateHubRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List updateHubRequestDescriptor = $convert.base64Decode(
    'ChBVcGRhdGVIdWJSZXF1ZXN0EhgKB2NoYW5uZWwYASABKAlSB2NoYW5uZWwSGAoHdmVyc2lvbh'
    'gCIAEoCVIHdmVyc2lvbg==');

@$core.Deprecated('Use updateHubProgressDescriptor instead')
const UpdateHubProgress$json = {
  '1': 'UpdateHubProgress',
  '2': [
    {
      '1': 'phase',
      '3': 1,
      '4': 1,
      '5': 14,
      '6': '.protectorate.v1.UpdatePhase',
      '10': 'phase'
    },
    {'1': 'message', '3': 2, '4': 1, '5': 9, '10': 'message'},
    {'1': 'version', '3': 3, '4': 1, '5': 9, '10': 'version'},
    {'1': 'previous_version', '3': 4, '4': 1, '5': 9, '10': 'previousVersion'},
    {'1': 'sent_bytes', '3': 5, '4': 1, '5': 4, '10': 'sentBytes'},
    {'1': 'total_bytes', '3': 6, '4': 1, '5': 4, '10': 'totalBytes'},
  ],
};

/// Descriptor for `UpdateHubProgress`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List updateHubProgressDescriptor = $convert.base64Decode(
    'ChFVcGRhdGVIdWJQcm9ncmVzcxIyCgVwaGFzZRgBIAEoDjIcLnByb3RlY3RvcmF0ZS52MS5VcG'
    'RhdGVQaGFzZVIFcGhhc2USGAoHbWVzc2FnZRgCIAEoCVIHbWVzc2FnZRIYCgd2ZXJzaW9uGAMg'
    'ASgJUgd2ZXJzaW9uEikKEHByZXZpb3VzX3ZlcnNpb24YBCABKAlSD3ByZXZpb3VzVmVyc2lvbh'
    'IdCgpzZW50X2J5dGVzGAUgASgEUglzZW50Qnl0ZXMSHwoLdG90YWxfYnl0ZXMYBiABKARSCnRv'
    'dGFsQnl0ZXM=');

const $core.Map<$core.String, $core.dynamic> ReleaseServiceBase$json = {
  '1': 'ReleaseService',
  '2': [
    {
      '1': 'CheckRelease',
      '2': '.protectorate.v1.CheckReleaseRequest',
      '3': '.protectorate.v1.ReleaseInfo',
      '4': {'34': 1},
    },
    {
      '1': 'UpdateHub',
      '2': '.protectorate.v1.UpdateHubRequest',
      '3': '.protectorate.v1.UpdateHubProgress',
      '4': {},
      '6': true
    },
  ],
};

@$core.Deprecated('Use releaseServiceDescriptor instead')
const $core.Map<$core.String, $core.Map<$core.String, $core.dynamic>>
    ReleaseServiceBase$messageJson = {
  '.protectorate.v1.CheckReleaseRequest': CheckReleaseRequest$json,
  '.protectorate.v1.ReleaseInfo': ReleaseInfo$json,
  '.protectorate.v1.UpdateHubRequest': UpdateHubRequest$json,
  '.protectorate.v1.UpdateHubProgress': UpdateHubProgress$json,
};

/// Descriptor for `ReleaseService`. Decode as a `google.protobuf.ServiceDescriptorProto`.
final $typed_data.Uint8List releaseServiceDescriptor = $convert.base64Decode(
    'Cg5SZWxlYXNlU2VydmljZRJXCgxDaGVja1JlbGVhc2USJC5wcm90ZWN0b3JhdGUudjEuQ2hlY2'
    'tSZWxlYXNlUmVxdWVzdBocLnByb3RlY3RvcmF0ZS52MS5SZWxlYXNlSW5mbyIDkAIBEl4KCVVw'
    'ZGF0ZUh1YhIhLnByb3RlY3RvcmF0ZS52MS5VcGRhdGVIdWJSZXF1ZXN0GiIucHJvdGVjdG9yYX'
    'RlLnYxLlVwZGF0ZUh1YlByb2dyZXNzIgigtRgBqLUYATAB');
