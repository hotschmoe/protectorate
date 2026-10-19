// This is a generated file - do not edit.
//
// Generated from protectorate/v1/session.proto.

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

@$core.Deprecated('Use sessionDescriptor instead')
const Session$json = {
  '1': 'Session',
  '2': [
    {'1': 'name', '3': 1, '4': 1, '5': 9, '10': 'name'},
    {'1': 'cwd', '3': 2, '4': 1, '5': 9, '10': 'cwd'},
    {'1': 'command', '3': 3, '4': 3, '5': 9, '10': 'command'},
    {
      '1': 'created_at',
      '3': 4,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Timestamp',
      '10': 'createdAt'
    },
    {'1': 'attached_clients', '3': 5, '4': 1, '5': 5, '10': 'attachedClients'},
    {
      '1': 'last_activity',
      '3': 6,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Timestamp',
      '10': 'lastActivity'
    },
    {'1': 'pane_cols', '3': 7, '4': 1, '5': 5, '10': 'paneCols'},
    {'1': 'pane_rows', '3': 8, '4': 1, '5': 5, '10': 'paneRows'},
  ],
};

/// Descriptor for `Session`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List sessionDescriptor = $convert.base64Decode(
    'CgdTZXNzaW9uEhIKBG5hbWUYASABKAlSBG5hbWUSEAoDY3dkGAIgASgJUgNjd2QSGAoHY29tbW'
    'FuZBgDIAMoCVIHY29tbWFuZBI5CgpjcmVhdGVkX2F0GAQgASgLMhouZ29vZ2xlLnByb3RvYnVm'
    'LlRpbWVzdGFtcFIJY3JlYXRlZEF0EikKEGF0dGFjaGVkX2NsaWVudHMYBSABKAVSD2F0dGFjaG'
    'VkQ2xpZW50cxI/Cg1sYXN0X2FjdGl2aXR5GAYgASgLMhouZ29vZ2xlLnByb3RvYnVmLlRpbWVz'
    'dGFtcFIMbGFzdEFjdGl2aXR5EhsKCXBhbmVfY29scxgHIAEoBVIIcGFuZUNvbHMSGwoJcGFuZV'
    '9yb3dzGAggASgFUghwYW5lUm93cw==');

@$core.Deprecated('Use listSessionsRequestDescriptor instead')
const ListSessionsRequest$json = {
  '1': 'ListSessionsRequest',
};

/// Descriptor for `ListSessionsRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List listSessionsRequestDescriptor =
    $convert.base64Decode('ChNMaXN0U2Vzc2lvbnNSZXF1ZXN0');

@$core.Deprecated('Use listSessionsResponseDescriptor instead')
const ListSessionsResponse$json = {
  '1': 'ListSessionsResponse',
  '2': [
    {
      '1': 'sessions',
      '3': 1,
      '4': 3,
      '5': 11,
      '6': '.protectorate.v1.Session',
      '10': 'sessions'
    },
  ],
};

/// Descriptor for `ListSessionsResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List listSessionsResponseDescriptor = $convert.base64Decode(
    'ChRMaXN0U2Vzc2lvbnNSZXNwb25zZRI0CghzZXNzaW9ucxgBIAMoCzIYLnByb3RlY3RvcmF0ZS'
    '52MS5TZXNzaW9uUghzZXNzaW9ucw==');

@$core.Deprecated('Use createSessionRequestDescriptor instead')
const CreateSessionRequest$json = {
  '1': 'CreateSessionRequest',
  '2': [
    {'1': 'name', '3': 1, '4': 1, '5': 9, '10': 'name'},
    {'1': 'cwd', '3': 2, '4': 1, '5': 9, '10': 'cwd'},
    {'1': 'command', '3': 3, '4': 3, '5': 9, '10': 'command'},
    {
      '1': 'env',
      '3': 4,
      '4': 3,
      '5': 11,
      '6': '.protectorate.v1.CreateSessionRequest.EnvEntry',
      '10': 'env'
    },
  ],
  '3': [CreateSessionRequest_EnvEntry$json],
};

@$core.Deprecated('Use createSessionRequestDescriptor instead')
const CreateSessionRequest_EnvEntry$json = {
  '1': 'EnvEntry',
  '2': [
    {'1': 'key', '3': 1, '4': 1, '5': 9, '10': 'key'},
    {'1': 'value', '3': 2, '4': 1, '5': 9, '10': 'value'},
  ],
  '7': {'7': true},
};

/// Descriptor for `CreateSessionRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List createSessionRequestDescriptor = $convert.base64Decode(
    'ChRDcmVhdGVTZXNzaW9uUmVxdWVzdBISCgRuYW1lGAEgASgJUgRuYW1lEhAKA2N3ZBgCIAEoCV'
    'IDY3dkEhgKB2NvbW1hbmQYAyADKAlSB2NvbW1hbmQSQAoDZW52GAQgAygLMi4ucHJvdGVjdG9y'
    'YXRlLnYxLkNyZWF0ZVNlc3Npb25SZXF1ZXN0LkVudkVudHJ5UgNlbnYaNgoIRW52RW50cnkSEA'
    'oDa2V5GAEgASgJUgNrZXkSFAoFdmFsdWUYAiABKAlSBXZhbHVlOgI4AQ==');

@$core.Deprecated('Use createSessionResponseDescriptor instead')
const CreateSessionResponse$json = {
  '1': 'CreateSessionResponse',
  '2': [
    {
      '1': 'session',
      '3': 1,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.Session',
      '10': 'session'
    },
  ],
};

/// Descriptor for `CreateSessionResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List createSessionResponseDescriptor = $convert.base64Decode(
    'ChVDcmVhdGVTZXNzaW9uUmVzcG9uc2USMgoHc2Vzc2lvbhgBIAEoCzIYLnByb3RlY3RvcmF0ZS'
    '52MS5TZXNzaW9uUgdzZXNzaW9u');

@$core.Deprecated('Use killSessionRequestDescriptor instead')
const KillSessionRequest$json = {
  '1': 'KillSessionRequest',
  '2': [
    {'1': 'name', '3': 1, '4': 1, '5': 9, '10': 'name'},
  ],
};

/// Descriptor for `KillSessionRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List killSessionRequestDescriptor = $convert
    .base64Decode('ChJLaWxsU2Vzc2lvblJlcXVlc3QSEgoEbmFtZRgBIAEoCVIEbmFtZQ==');

@$core.Deprecated('Use killSessionResponseDescriptor instead')
const KillSessionResponse$json = {
  '1': 'KillSessionResponse',
};

/// Descriptor for `KillSessionResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List killSessionResponseDescriptor =
    $convert.base64Decode('ChNLaWxsU2Vzc2lvblJlc3BvbnNl');

@$core.Deprecated('Use capturePaneRequestDescriptor instead')
const CapturePaneRequest$json = {
  '1': 'CapturePaneRequest',
  '2': [
    {'1': 'name', '3': 1, '4': 1, '5': 9, '10': 'name'},
    {'1': 'lines', '3': 2, '4': 1, '5': 5, '10': 'lines'},
  ],
};

/// Descriptor for `CapturePaneRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List capturePaneRequestDescriptor = $convert.base64Decode(
    'ChJDYXB0dXJlUGFuZVJlcXVlc3QSEgoEbmFtZRgBIAEoCVIEbmFtZRIUCgVsaW5lcxgCIAEoBV'
    'IFbGluZXM=');

@$core.Deprecated('Use capturePaneResponseDescriptor instead')
const CapturePaneResponse$json = {
  '1': 'CapturePaneResponse',
  '2': [
    {'1': 'text', '3': 1, '4': 1, '5': 9, '10': 'text'},
  ],
};

/// Descriptor for `CapturePaneResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List capturePaneResponseDescriptor = $convert
    .base64Decode('ChNDYXB0dXJlUGFuZVJlc3BvbnNlEhIKBHRleHQYASABKAlSBHRleHQ=');

const $core.Map<$core.String, $core.dynamic> SessionServiceBase$json = {
  '1': 'SessionService',
  '2': [
    {
      '1': 'ListSessions',
      '2': '.protectorate.v1.ListSessionsRequest',
      '3': '.protectorate.v1.ListSessionsResponse',
      '4': {'34': 1},
    },
    {
      '1': 'CreateSession',
      '2': '.protectorate.v1.CreateSessionRequest',
      '3': '.protectorate.v1.CreateSessionResponse'
    },
    {
      '1': 'KillSession',
      '2': '.protectorate.v1.KillSessionRequest',
      '3': '.protectorate.v1.KillSessionResponse',
      '4': {}
    },
    {
      '1': 'CapturePane',
      '2': '.protectorate.v1.CapturePaneRequest',
      '3': '.protectorate.v1.CapturePaneResponse',
      '4': {'34': 1},
    },
  ],
};

@$core.Deprecated('Use sessionServiceDescriptor instead')
const $core.Map<$core.String, $core.Map<$core.String, $core.dynamic>>
    SessionServiceBase$messageJson = {
  '.protectorate.v1.ListSessionsRequest': ListSessionsRequest$json,
  '.protectorate.v1.ListSessionsResponse': ListSessionsResponse$json,
  '.protectorate.v1.Session': Session$json,
  '.google.protobuf.Timestamp': $0.Timestamp$json,
  '.protectorate.v1.CreateSessionRequest': CreateSessionRequest$json,
  '.protectorate.v1.CreateSessionRequest.EnvEntry':
      CreateSessionRequest_EnvEntry$json,
  '.protectorate.v1.CreateSessionResponse': CreateSessionResponse$json,
  '.protectorate.v1.KillSessionRequest': KillSessionRequest$json,
  '.protectorate.v1.KillSessionResponse': KillSessionResponse$json,
  '.protectorate.v1.CapturePaneRequest': CapturePaneRequest$json,
  '.protectorate.v1.CapturePaneResponse': CapturePaneResponse$json,
};

/// Descriptor for `SessionService`. Decode as a `google.protobuf.ServiceDescriptorProto`.
final $typed_data.Uint8List sessionServiceDescriptor = $convert.base64Decode(
    'Cg5TZXNzaW9uU2VydmljZRJgCgxMaXN0U2Vzc2lvbnMSJC5wcm90ZWN0b3JhdGUudjEuTGlzdF'
    'Nlc3Npb25zUmVxdWVzdBolLnByb3RlY3RvcmF0ZS52MS5MaXN0U2Vzc2lvbnNSZXNwb25zZSID'
    'kAIBEl4KDUNyZWF0ZVNlc3Npb24SJS5wcm90ZWN0b3JhdGUudjEuQ3JlYXRlU2Vzc2lvblJlcX'
    'Vlc3QaJi5wcm90ZWN0b3JhdGUudjEuQ3JlYXRlU2Vzc2lvblJlc3BvbnNlEl4KC0tpbGxTZXNz'
    'aW9uEiMucHJvdGVjdG9yYXRlLnYxLktpbGxTZXNzaW9uUmVxdWVzdBokLnByb3RlY3RvcmF0ZS'
    '52MS5LaWxsU2Vzc2lvblJlc3BvbnNlIgSItRgBEl0KC0NhcHR1cmVQYW5lEiMucHJvdGVjdG9y'
    'YXRlLnYxLkNhcHR1cmVQYW5lUmVxdWVzdBokLnByb3RlY3RvcmF0ZS52MS5DYXB0dXJlUGFuZV'
    'Jlc3BvbnNlIgOQAgE=');
