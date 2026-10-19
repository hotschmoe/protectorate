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

@$core.Deprecated('Use sleeveDescriptor instead')
const Sleeve$json = {
  '1': 'Sleeve',
  '2': [
    {'1': 'id', '3': 1, '4': 1, '5': 9, '10': 'id'},
    {'1': 'name', '3': 2, '4': 1, '5': 9, '10': 'name'},
    {'1': 'harness', '3': 3, '4': 1, '5': 9, '10': 'harness'},
    {'1': 'state', '3': 4, '4': 1, '5': 9, '10': 'state'},
  ],
};

/// Descriptor for `Sleeve`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List sleeveDescriptor = $convert.base64Decode(
    'CgZTbGVldmUSDgoCaWQYASABKAlSAmlkEhIKBG5hbWUYAiABKAlSBG5hbWUSGAoHaGFybmVzcx'
    'gDIAEoCVIHaGFybmVzcxIUCgVzdGF0ZRgEIAEoCVIFc3RhdGU=');

@$core.Deprecated('Use listRequestDescriptor instead')
const ListRequest$json = {
  '1': 'ListRequest',
};

/// Descriptor for `ListRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List listRequestDescriptor =
    $convert.base64Decode('CgtMaXN0UmVxdWVzdA==');

@$core.Deprecated('Use listResponseDescriptor instead')
const ListResponse$json = {
  '1': 'ListResponse',
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

/// Descriptor for `ListResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List listResponseDescriptor = $convert.base64Decode(
    'CgxMaXN0UmVzcG9uc2USMQoHc2xlZXZlcxgBIAMoCzIXLnByb3RlY3RvcmF0ZS52MS5TbGVldm'
    'VSB3NsZWV2ZXM=');

@$core.Deprecated('Use spawnRequestDescriptor instead')
const SpawnRequest$json = {
  '1': 'SpawnRequest',
  '2': [
    {'1': 'name', '3': 1, '4': 1, '5': 9, '10': 'name'},
    {'1': 'harness', '3': 2, '4': 1, '5': 9, '10': 'harness'},
  ],
};

/// Descriptor for `SpawnRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List spawnRequestDescriptor = $convert.base64Decode(
    'CgxTcGF3blJlcXVlc3QSEgoEbmFtZRgBIAEoCVIEbmFtZRIYCgdoYXJuZXNzGAIgASgJUgdoYX'
    'JuZXNz');

@$core.Deprecated('Use spawnResponseDescriptor instead')
const SpawnResponse$json = {
  '1': 'SpawnResponse',
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

/// Descriptor for `SpawnResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List spawnResponseDescriptor = $convert.base64Decode(
    'Cg1TcGF3blJlc3BvbnNlEi8KBnNsZWV2ZRgBIAEoCzIXLnByb3RlY3RvcmF0ZS52MS5TbGVldm'
    'VSBnNsZWV2ZQ==');

const $core.Map<$core.String, $core.dynamic> SleeveServiceBase$json = {
  '1': 'SleeveService',
  '2': [
    {
      '1': 'List',
      '2': '.protectorate.v1.ListRequest',
      '3': '.protectorate.v1.ListResponse',
      '4': {'34': 1},
    },
    {
      '1': 'Spawn',
      '2': '.protectorate.v1.SpawnRequest',
      '3': '.protectorate.v1.SpawnResponse'
    },
  ],
};

@$core.Deprecated('Use sleeveServiceDescriptor instead')
const $core.Map<$core.String, $core.Map<$core.String, $core.dynamic>>
    SleeveServiceBase$messageJson = {
  '.protectorate.v1.ListRequest': ListRequest$json,
  '.protectorate.v1.ListResponse': ListResponse$json,
  '.protectorate.v1.Sleeve': Sleeve$json,
  '.protectorate.v1.SpawnRequest': SpawnRequest$json,
  '.protectorate.v1.SpawnResponse': SpawnResponse$json,
};

/// Descriptor for `SleeveService`. Decode as a `google.protobuf.ServiceDescriptorProto`.
final $typed_data.Uint8List sleeveServiceDescriptor = $convert.base64Decode(
    'Cg1TbGVldmVTZXJ2aWNlEkgKBExpc3QSHC5wcm90ZWN0b3JhdGUudjEuTGlzdFJlcXVlc3QaHS'
    '5wcm90ZWN0b3JhdGUudjEuTGlzdFJlc3BvbnNlIgOQAgESRgoFU3Bhd24SHS5wcm90ZWN0b3Jh'
    'dGUudjEuU3Bhd25SZXF1ZXN0Gh4ucHJvdGVjdG9yYXRlLnYxLlNwYXduUmVzcG9uc2U=');
