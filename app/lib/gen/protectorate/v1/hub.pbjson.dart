// This is a generated file - do not edit.
//
// Generated from protectorate/v1/hub.proto.

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

@$core.Deprecated('Use getStatusRequestDescriptor instead')
const GetStatusRequest$json = {
  '1': 'GetStatusRequest',
};

/// Descriptor for `GetStatusRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List getStatusRequestDescriptor =
    $convert.base64Decode('ChBHZXRTdGF0dXNSZXF1ZXN0');

@$core.Deprecated('Use getStatusResponseDescriptor instead')
const GetStatusResponse$json = {
  '1': 'GetStatusResponse',
  '2': [
    {'1': 'version', '3': 1, '4': 1, '5': 9, '10': 'version'},
    {'1': 'hostname', '3': 2, '4': 1, '5': 9, '10': 'hostname'},
    {'1': 'os', '3': 3, '4': 1, '5': 9, '10': 'os'},
    {'1': 'arch', '3': 4, '4': 1, '5': 9, '10': 'arch'},
    {'1': 'uptime_seconds', '3': 5, '4': 1, '5': 3, '10': 'uptimeSeconds'},
    {
      '1': 'started_at',
      '3': 6,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Timestamp',
      '10': 'startedAt'
    },
    {
      '1': 'available_version',
      '3': 7,
      '4': 1,
      '5': 9,
      '10': 'availableVersion'
    },
    {'1': 'update_channel', '3': 8, '4': 1, '5': 9, '10': 'updateChannel'},
    {'1': 'release_notes_url', '3': 9, '4': 1, '5': 9, '10': 'releaseNotesUrl'},
  ],
};

/// Descriptor for `GetStatusResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List getStatusResponseDescriptor = $convert.base64Decode(
    'ChFHZXRTdGF0dXNSZXNwb25zZRIYCgd2ZXJzaW9uGAEgASgJUgd2ZXJzaW9uEhoKCGhvc3RuYW'
    '1lGAIgASgJUghob3N0bmFtZRIOCgJvcxgDIAEoCVICb3MSEgoEYXJjaBgEIAEoCVIEYXJjaBIl'
    'Cg51cHRpbWVfc2Vjb25kcxgFIAEoA1INdXB0aW1lU2Vjb25kcxI5CgpzdGFydGVkX2F0GAYgAS'
    'gLMhouZ29vZ2xlLnByb3RvYnVmLlRpbWVzdGFtcFIJc3RhcnRlZEF0EisKEWF2YWlsYWJsZV92'
    'ZXJzaW9uGAcgASgJUhBhdmFpbGFibGVWZXJzaW9uEiUKDnVwZGF0ZV9jaGFubmVsGAggASgJUg'
    '11cGRhdGVDaGFubmVsEioKEXJlbGVhc2Vfbm90ZXNfdXJsGAkgASgJUg9yZWxlYXNlTm90ZXNV'
    'cmw=');

@$core.Deprecated('Use getSleeveRuntimeRequestDescriptor instead')
const GetSleeveRuntimeRequest$json = {
  '1': 'GetSleeveRuntimeRequest',
};

/// Descriptor for `GetSleeveRuntimeRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List getSleeveRuntimeRequestDescriptor =
    $convert.base64Decode('ChdHZXRTbGVldmVSdW50aW1lUmVxdWVzdA==');

@$core.Deprecated('Use getSleeveRuntimeResponseDescriptor instead')
const GetSleeveRuntimeResponse$json = {
  '1': 'GetSleeveRuntimeResponse',
  '2': [
    {'1': 'bwrap_path', '3': 1, '4': 1, '5': 9, '10': 'bwrapPath'},
    {'1': 'bwrap_version', '3': 2, '4': 1, '5': 9, '10': 'bwrapVersion'},
    {'1': 'overlays', '3': 3, '4': 1, '5': 8, '10': 'overlays'},
    {'1': 'go_build_cache', '3': 4, '4': 1, '5': 9, '10': 'goBuildCache'},
    {'1': 'go_build_lower', '3': 5, '4': 1, '5': 9, '10': 'goBuildLower'},
    {'1': 'snapshot_bytes', '3': 6, '4': 1, '5': 4, '10': 'snapshotBytes'},
    {'1': 'snapshot_entries', '3': 7, '4': 1, '5': 4, '10': 'snapshotEntries'},
    {
      '1': 'snapshot_prepared_at',
      '3': 8,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Timestamp',
      '10': 'snapshotPreparedAt'
    },
    {
      '1': 'snapshot_preparing',
      '3': 9,
      '4': 1,
      '5': 8,
      '10': 'snapshotPreparing'
    },
    {'1': 'snapshot_error', '3': 10, '4': 1, '5': 9, '10': 'snapshotError'},
    {
      '1': 'snapshot_limit_bytes',
      '3': 11,
      '4': 1,
      '5': 4,
      '10': 'snapshotLimitBytes'
    },
  ],
};

/// Descriptor for `GetSleeveRuntimeResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List getSleeveRuntimeResponseDescriptor = $convert.base64Decode(
    'ChhHZXRTbGVldmVSdW50aW1lUmVzcG9uc2USHQoKYndyYXBfcGF0aBgBIAEoCVIJYndyYXBQYX'
    'RoEiMKDWJ3cmFwX3ZlcnNpb24YAiABKAlSDGJ3cmFwVmVyc2lvbhIaCghvdmVybGF5cxgDIAEo'
    'CFIIb3ZlcmxheXMSJAoOZ29fYnVpbGRfY2FjaGUYBCABKAlSDGdvQnVpbGRDYWNoZRIkCg5nb1'
    '9idWlsZF9sb3dlchgFIAEoCVIMZ29CdWlsZExvd2VyEiUKDnNuYXBzaG90X2J5dGVzGAYgASgE'
    'Ug1zbmFwc2hvdEJ5dGVzEikKEHNuYXBzaG90X2VudHJpZXMYByABKARSD3NuYXBzaG90RW50cm'
    'llcxJMChRzbmFwc2hvdF9wcmVwYXJlZF9hdBgIIAEoCzIaLmdvb2dsZS5wcm90b2J1Zi5UaW1l'
    'c3RhbXBSEnNuYXBzaG90UHJlcGFyZWRBdBItChJzbmFwc2hvdF9wcmVwYXJpbmcYCSABKAhSEX'
    'NuYXBzaG90UHJlcGFyaW5nEiUKDnNuYXBzaG90X2Vycm9yGAogASgJUg1zbmFwc2hvdEVycm9y'
    'EjAKFHNuYXBzaG90X2xpbWl0X2J5dGVzGAsgASgEUhJzbmFwc2hvdExpbWl0Qnl0ZXM=');

const $core.Map<$core.String, $core.dynamic> HubServiceBase$json = {
  '1': 'HubService',
  '2': [
    {
      '1': 'GetStatus',
      '2': '.protectorate.v1.GetStatusRequest',
      '3': '.protectorate.v1.GetStatusResponse',
      '4': {'34': 1},
    },
    {
      '1': 'GetSleeveRuntime',
      '2': '.protectorate.v1.GetSleeveRuntimeRequest',
      '3': '.protectorate.v1.GetSleeveRuntimeResponse',
      '4': {'34': 1},
    },
  ],
};

@$core.Deprecated('Use hubServiceDescriptor instead')
const $core.Map<$core.String, $core.Map<$core.String, $core.dynamic>>
    HubServiceBase$messageJson = {
  '.protectorate.v1.GetStatusRequest': GetStatusRequest$json,
  '.protectorate.v1.GetStatusResponse': GetStatusResponse$json,
  '.google.protobuf.Timestamp': $0.Timestamp$json,
  '.protectorate.v1.GetSleeveRuntimeRequest': GetSleeveRuntimeRequest$json,
  '.protectorate.v1.GetSleeveRuntimeResponse': GetSleeveRuntimeResponse$json,
};

/// Descriptor for `HubService`. Decode as a `google.protobuf.ServiceDescriptorProto`.
final $typed_data.Uint8List hubServiceDescriptor = $convert.base64Decode(
    'CgpIdWJTZXJ2aWNlElcKCUdldFN0YXR1cxIhLnByb3RlY3RvcmF0ZS52MS5HZXRTdGF0dXNSZX'
    'F1ZXN0GiIucHJvdGVjdG9yYXRlLnYxLkdldFN0YXR1c1Jlc3BvbnNlIgOQAgESbAoQR2V0U2xl'
    'ZXZlUnVudGltZRIoLnByb3RlY3RvcmF0ZS52MS5HZXRTbGVldmVSdW50aW1lUmVxdWVzdBopLn'
    'Byb3RlY3RvcmF0ZS52MS5HZXRTbGVldmVSdW50aW1lUmVzcG9uc2UiA5ACAQ==');
