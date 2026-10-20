// This is a generated file - do not edit.
//
// Generated from protectorate/v1/device.proto.

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

import 'token.pbjson.dart' as $1;

@$core.Deprecated('Use startDeviceAuthRequestDescriptor instead')
const StartDeviceAuthRequest$json = {
  '1': 'StartDeviceAuthRequest',
  '2': [
    {'1': 'name', '3': 1, '4': 1, '5': 9, '10': 'name'},
    {'1': 'platform', '3': 2, '4': 1, '5': 9, '10': 'platform'},
  ],
};

/// Descriptor for `StartDeviceAuthRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List startDeviceAuthRequestDescriptor =
    $convert.base64Decode(
        'ChZTdGFydERldmljZUF1dGhSZXF1ZXN0EhIKBG5hbWUYASABKAlSBG5hbWUSGgoIcGxhdGZvcm'
        '0YAiABKAlSCHBsYXRmb3Jt');

@$core.Deprecated('Use startDeviceAuthResponseDescriptor instead')
const StartDeviceAuthResponse$json = {
  '1': 'StartDeviceAuthResponse',
  '2': [
    {'1': 'device_code', '3': 1, '4': 1, '5': 9, '10': 'deviceCode'},
    {'1': 'user_code', '3': 2, '4': 1, '5': 9, '10': 'userCode'},
    {
      '1': 'expires_at',
      '3': 3,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Timestamp',
      '10': 'expiresAt'
    },
    {'1': 'interval', '3': 4, '4': 1, '5': 5, '10': 'interval'},
  ],
};

/// Descriptor for `StartDeviceAuthResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List startDeviceAuthResponseDescriptor = $convert.base64Decode(
    'ChdTdGFydERldmljZUF1dGhSZXNwb25zZRIfCgtkZXZpY2VfY29kZRgBIAEoCVIKZGV2aWNlQ2'
    '9kZRIbCgl1c2VyX2NvZGUYAiABKAlSCHVzZXJDb2RlEjkKCmV4cGlyZXNfYXQYAyABKAsyGi5n'
    'b29nbGUucHJvdG9idWYuVGltZXN0YW1wUglleHBpcmVzQXQSGgoIaW50ZXJ2YWwYBCABKAVSCG'
    'ludGVydmFs');

@$core.Deprecated('Use pollDeviceAuthRequestDescriptor instead')
const PollDeviceAuthRequest$json = {
  '1': 'PollDeviceAuthRequest',
  '2': [
    {'1': 'device_code', '3': 1, '4': 1, '5': 9, '10': 'deviceCode'},
  ],
};

/// Descriptor for `PollDeviceAuthRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List pollDeviceAuthRequestDescriptor = $convert.base64Decode(
    'ChVQb2xsRGV2aWNlQXV0aFJlcXVlc3QSHwoLZGV2aWNlX2NvZGUYASABKAlSCmRldmljZUNvZG'
    'U=');

@$core.Deprecated('Use pollDeviceAuthResponseDescriptor instead')
const PollDeviceAuthResponse$json = {
  '1': 'PollDeviceAuthResponse',
  '2': [
    {'1': 'status', '3': 1, '4': 1, '5': 9, '10': 'status'},
    {'1': 'interval', '3': 2, '4': 1, '5': 5, '10': 'interval'},
    {'1': 'secret', '3': 3, '4': 1, '5': 9, '10': 'secret'},
    {
      '1': 'token',
      '3': 4,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.TokenInfo',
      '10': 'token'
    },
  ],
};

/// Descriptor for `PollDeviceAuthResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List pollDeviceAuthResponseDescriptor = $convert.base64Decode(
    'ChZQb2xsRGV2aWNlQXV0aFJlc3BvbnNlEhYKBnN0YXR1cxgBIAEoCVIGc3RhdHVzEhoKCGludG'
    'VydmFsGAIgASgFUghpbnRlcnZhbBIWCgZzZWNyZXQYAyABKAlSBnNlY3JldBIwCgV0b2tlbhgE'
    'IAEoCzIaLnByb3RlY3RvcmF0ZS52MS5Ub2tlbkluZm9SBXRva2Vu');

@$core.Deprecated('Use deviceAuthInfoDescriptor instead')
const DeviceAuthInfo$json = {
  '1': 'DeviceAuthInfo',
  '2': [
    {'1': 'user_code', '3': 1, '4': 1, '5': 9, '10': 'userCode'},
    {'1': 'name', '3': 2, '4': 1, '5': 9, '10': 'name'},
    {'1': 'platform', '3': 3, '4': 1, '5': 9, '10': 'platform'},
    {'1': 'requesting_ip', '3': 4, '4': 1, '5': 9, '10': 'requestingIp'},
    {
      '1': 'expires_at',
      '3': 5,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Timestamp',
      '10': 'expiresAt'
    },
  ],
};

/// Descriptor for `DeviceAuthInfo`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List deviceAuthInfoDescriptor = $convert.base64Decode(
    'Cg5EZXZpY2VBdXRoSW5mbxIbCgl1c2VyX2NvZGUYASABKAlSCHVzZXJDb2RlEhIKBG5hbWUYAi'
    'ABKAlSBG5hbWUSGgoIcGxhdGZvcm0YAyABKAlSCHBsYXRmb3JtEiMKDXJlcXVlc3RpbmdfaXAY'
    'BCABKAlSDHJlcXVlc3RpbmdJcBI5CgpleHBpcmVzX2F0GAUgASgLMhouZ29vZ2xlLnByb3RvYn'
    'VmLlRpbWVzdGFtcFIJZXhwaXJlc0F0');

@$core.Deprecated('Use listDeviceAuthRequestDescriptor instead')
const ListDeviceAuthRequest$json = {
  '1': 'ListDeviceAuthRequest',
};

/// Descriptor for `ListDeviceAuthRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List listDeviceAuthRequestDescriptor =
    $convert.base64Decode('ChVMaXN0RGV2aWNlQXV0aFJlcXVlc3Q=');

@$core.Deprecated('Use listDeviceAuthResponseDescriptor instead')
const ListDeviceAuthResponse$json = {
  '1': 'ListDeviceAuthResponse',
  '2': [
    {
      '1': 'requests',
      '3': 1,
      '4': 3,
      '5': 11,
      '6': '.protectorate.v1.DeviceAuthInfo',
      '10': 'requests'
    },
  ],
};

/// Descriptor for `ListDeviceAuthResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List listDeviceAuthResponseDescriptor =
    $convert.base64Decode(
        'ChZMaXN0RGV2aWNlQXV0aFJlc3BvbnNlEjsKCHJlcXVlc3RzGAEgAygLMh8ucHJvdGVjdG9yYX'
        'RlLnYxLkRldmljZUF1dGhJbmZvUghyZXF1ZXN0cw==');

@$core.Deprecated('Use approveDeviceAuthRequestDescriptor instead')
const ApproveDeviceAuthRequest$json = {
  '1': 'ApproveDeviceAuthRequest',
  '2': [
    {'1': 'user_code', '3': 1, '4': 1, '5': 9, '10': 'userCode'},
  ],
};

/// Descriptor for `ApproveDeviceAuthRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List approveDeviceAuthRequestDescriptor =
    $convert.base64Decode(
        'ChhBcHByb3ZlRGV2aWNlQXV0aFJlcXVlc3QSGwoJdXNlcl9jb2RlGAEgASgJUgh1c2VyQ29kZQ'
        '==');

@$core.Deprecated('Use approveDeviceAuthResponseDescriptor instead')
const ApproveDeviceAuthResponse$json = {
  '1': 'ApproveDeviceAuthResponse',
};

/// Descriptor for `ApproveDeviceAuthResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List approveDeviceAuthResponseDescriptor =
    $convert.base64Decode('ChlBcHByb3ZlRGV2aWNlQXV0aFJlc3BvbnNl');

@$core.Deprecated('Use denyDeviceAuthRequestDescriptor instead')
const DenyDeviceAuthRequest$json = {
  '1': 'DenyDeviceAuthRequest',
  '2': [
    {'1': 'user_code', '3': 1, '4': 1, '5': 9, '10': 'userCode'},
  ],
};

/// Descriptor for `DenyDeviceAuthRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List denyDeviceAuthRequestDescriptor = $convert.base64Decode(
    'ChVEZW55RGV2aWNlQXV0aFJlcXVlc3QSGwoJdXNlcl9jb2RlGAEgASgJUgh1c2VyQ29kZQ==');

@$core.Deprecated('Use denyDeviceAuthResponseDescriptor instead')
const DenyDeviceAuthResponse$json = {
  '1': 'DenyDeviceAuthResponse',
};

/// Descriptor for `DenyDeviceAuthResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List denyDeviceAuthResponseDescriptor =
    $convert.base64Decode('ChZEZW55RGV2aWNlQXV0aFJlc3BvbnNl');

@$core.Deprecated('Use listDevicesRequestDescriptor instead')
const ListDevicesRequest$json = {
  '1': 'ListDevicesRequest',
};

/// Descriptor for `ListDevicesRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List listDevicesRequestDescriptor =
    $convert.base64Decode('ChJMaXN0RGV2aWNlc1JlcXVlc3Q=');

@$core.Deprecated('Use listDevicesResponseDescriptor instead')
const ListDevicesResponse$json = {
  '1': 'ListDevicesResponse',
  '2': [
    {
      '1': 'devices',
      '3': 1,
      '4': 3,
      '5': 11,
      '6': '.protectorate.v1.TokenInfo',
      '10': 'devices'
    },
  ],
};

/// Descriptor for `ListDevicesResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List listDevicesResponseDescriptor = $convert.base64Decode(
    'ChNMaXN0RGV2aWNlc1Jlc3BvbnNlEjQKB2RldmljZXMYASADKAsyGi5wcm90ZWN0b3JhdGUudj'
    'EuVG9rZW5JbmZvUgdkZXZpY2Vz');

@$core.Deprecated('Use revokeDeviceRequestDescriptor instead')
const RevokeDeviceRequest$json = {
  '1': 'RevokeDeviceRequest',
  '2': [
    {'1': 'id', '3': 1, '4': 1, '5': 9, '10': 'id'},
  ],
};

/// Descriptor for `RevokeDeviceRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List revokeDeviceRequestDescriptor = $convert
    .base64Decode('ChNSZXZva2VEZXZpY2VSZXF1ZXN0Eg4KAmlkGAEgASgJUgJpZA==');

@$core.Deprecated('Use revokeDeviceResponseDescriptor instead')
const RevokeDeviceResponse$json = {
  '1': 'RevokeDeviceResponse',
  '2': [
    {
      '1': 'token',
      '3': 1,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.TokenInfo',
      '10': 'token'
    },
  ],
};

/// Descriptor for `RevokeDeviceResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List revokeDeviceResponseDescriptor = $convert.base64Decode(
    'ChRSZXZva2VEZXZpY2VSZXNwb25zZRIwCgV0b2tlbhgBIAEoCzIaLnByb3RlY3RvcmF0ZS52MS'
    '5Ub2tlbkluZm9SBXRva2Vu');

@$core.Deprecated('Use renameDeviceRequestDescriptor instead')
const RenameDeviceRequest$json = {
  '1': 'RenameDeviceRequest',
  '2': [
    {'1': 'id', '3': 1, '4': 1, '5': 9, '10': 'id'},
    {'1': 'name', '3': 2, '4': 1, '5': 9, '10': 'name'},
  ],
};

/// Descriptor for `RenameDeviceRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List renameDeviceRequestDescriptor = $convert.base64Decode(
    'ChNSZW5hbWVEZXZpY2VSZXF1ZXN0Eg4KAmlkGAEgASgJUgJpZBISCgRuYW1lGAIgASgJUgRuYW'
    '1l');

@$core.Deprecated('Use renameDeviceResponseDescriptor instead')
const RenameDeviceResponse$json = {
  '1': 'RenameDeviceResponse',
  '2': [
    {
      '1': 'token',
      '3': 1,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.TokenInfo',
      '10': 'token'
    },
  ],
};

/// Descriptor for `RenameDeviceResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List renameDeviceResponseDescriptor = $convert.base64Decode(
    'ChRSZW5hbWVEZXZpY2VSZXNwb25zZRIwCgV0b2tlbhgBIAEoCzIaLnByb3RlY3RvcmF0ZS52MS'
    '5Ub2tlbkluZm9SBXRva2Vu');

const $core.Map<$core.String, $core.dynamic> DeviceServiceBase$json = {
  '1': 'DeviceService',
  '2': [
    {
      '1': 'StartDeviceAuth',
      '2': '.protectorate.v1.StartDeviceAuthRequest',
      '3': '.protectorate.v1.StartDeviceAuthResponse'
    },
    {
      '1': 'PollDeviceAuth',
      '2': '.protectorate.v1.PollDeviceAuthRequest',
      '3': '.protectorate.v1.PollDeviceAuthResponse'
    },
    {
      '1': 'ListDeviceAuth',
      '2': '.protectorate.v1.ListDeviceAuthRequest',
      '3': '.protectorate.v1.ListDeviceAuthResponse',
      '4': {'34': 1},
    },
    {
      '1': 'ApproveDeviceAuth',
      '2': '.protectorate.v1.ApproveDeviceAuthRequest',
      '3': '.protectorate.v1.ApproveDeviceAuthResponse'
    },
    {
      '1': 'DenyDeviceAuth',
      '2': '.protectorate.v1.DenyDeviceAuthRequest',
      '3': '.protectorate.v1.DenyDeviceAuthResponse'
    },
    {
      '1': 'ListDevices',
      '2': '.protectorate.v1.ListDevicesRequest',
      '3': '.protectorate.v1.ListDevicesResponse',
      '4': {'34': 1},
    },
    {
      '1': 'RevokeDevice',
      '2': '.protectorate.v1.RevokeDeviceRequest',
      '3': '.protectorate.v1.RevokeDeviceResponse'
    },
    {
      '1': 'RenameDevice',
      '2': '.protectorate.v1.RenameDeviceRequest',
      '3': '.protectorate.v1.RenameDeviceResponse'
    },
  ],
};

@$core.Deprecated('Use deviceServiceDescriptor instead')
const $core.Map<$core.String, $core.Map<$core.String, $core.dynamic>>
    DeviceServiceBase$messageJson = {
  '.protectorate.v1.StartDeviceAuthRequest': StartDeviceAuthRequest$json,
  '.protectorate.v1.StartDeviceAuthResponse': StartDeviceAuthResponse$json,
  '.google.protobuf.Timestamp': $0.Timestamp$json,
  '.protectorate.v1.PollDeviceAuthRequest': PollDeviceAuthRequest$json,
  '.protectorate.v1.PollDeviceAuthResponse': PollDeviceAuthResponse$json,
  '.protectorate.v1.TokenInfo': $1.TokenInfo$json,
  '.protectorate.v1.ListDeviceAuthRequest': ListDeviceAuthRequest$json,
  '.protectorate.v1.ListDeviceAuthResponse': ListDeviceAuthResponse$json,
  '.protectorate.v1.DeviceAuthInfo': DeviceAuthInfo$json,
  '.protectorate.v1.ApproveDeviceAuthRequest': ApproveDeviceAuthRequest$json,
  '.protectorate.v1.ApproveDeviceAuthResponse': ApproveDeviceAuthResponse$json,
  '.protectorate.v1.DenyDeviceAuthRequest': DenyDeviceAuthRequest$json,
  '.protectorate.v1.DenyDeviceAuthResponse': DenyDeviceAuthResponse$json,
  '.protectorate.v1.ListDevicesRequest': ListDevicesRequest$json,
  '.protectorate.v1.ListDevicesResponse': ListDevicesResponse$json,
  '.protectorate.v1.RevokeDeviceRequest': RevokeDeviceRequest$json,
  '.protectorate.v1.RevokeDeviceResponse': RevokeDeviceResponse$json,
  '.protectorate.v1.RenameDeviceRequest': RenameDeviceRequest$json,
  '.protectorate.v1.RenameDeviceResponse': RenameDeviceResponse$json,
};

/// Descriptor for `DeviceService`. Decode as a `google.protobuf.ServiceDescriptorProto`.
final $typed_data.Uint8List deviceServiceDescriptor = $convert.base64Decode(
    'Cg1EZXZpY2VTZXJ2aWNlEmQKD1N0YXJ0RGV2aWNlQXV0aBInLnByb3RlY3RvcmF0ZS52MS5TdG'
    'FydERldmljZUF1dGhSZXF1ZXN0GigucHJvdGVjdG9yYXRlLnYxLlN0YXJ0RGV2aWNlQXV0aFJl'
    'c3BvbnNlEmEKDlBvbGxEZXZpY2VBdXRoEiYucHJvdGVjdG9yYXRlLnYxLlBvbGxEZXZpY2VBdX'
    'RoUmVxdWVzdBonLnByb3RlY3RvcmF0ZS52MS5Qb2xsRGV2aWNlQXV0aFJlc3BvbnNlEmYKDkxp'
    'c3REZXZpY2VBdXRoEiYucHJvdGVjdG9yYXRlLnYxLkxpc3REZXZpY2VBdXRoUmVxdWVzdBonLn'
    'Byb3RlY3RvcmF0ZS52MS5MaXN0RGV2aWNlQXV0aFJlc3BvbnNlIgOQAgESagoRQXBwcm92ZURl'
    'dmljZUF1dGgSKS5wcm90ZWN0b3JhdGUudjEuQXBwcm92ZURldmljZUF1dGhSZXF1ZXN0GioucH'
    'JvdGVjdG9yYXRlLnYxLkFwcHJvdmVEZXZpY2VBdXRoUmVzcG9uc2USYQoORGVueURldmljZUF1'
    'dGgSJi5wcm90ZWN0b3JhdGUudjEuRGVueURldmljZUF1dGhSZXF1ZXN0GicucHJvdGVjdG9yYX'
    'RlLnYxLkRlbnlEZXZpY2VBdXRoUmVzcG9uc2USXQoLTGlzdERldmljZXMSIy5wcm90ZWN0b3Jh'
    'dGUudjEuTGlzdERldmljZXNSZXF1ZXN0GiQucHJvdGVjdG9yYXRlLnYxLkxpc3REZXZpY2VzUm'
    'VzcG9uc2UiA5ACARJbCgxSZXZva2VEZXZpY2USJC5wcm90ZWN0b3JhdGUudjEuUmV2b2tlRGV2'
    'aWNlUmVxdWVzdBolLnByb3RlY3RvcmF0ZS52MS5SZXZva2VEZXZpY2VSZXNwb25zZRJbCgxSZW'
    '5hbWVEZXZpY2USJC5wcm90ZWN0b3JhdGUudjEuUmVuYW1lRGV2aWNlUmVxdWVzdBolLnByb3Rl'
    'Y3RvcmF0ZS52MS5SZW5hbWVEZXZpY2VSZXNwb25zZQ==');
