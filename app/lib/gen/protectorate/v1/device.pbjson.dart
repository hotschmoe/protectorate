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

@$core.Deprecated('Use deviceInfoDescriptor instead')
const DeviceInfo$json = {
  '1': 'DeviceInfo',
  '2': [
    {'1': 'id', '3': 1, '4': 1, '5': 9, '10': 'id'},
    {'1': 'name', '3': 2, '4': 1, '5': 9, '10': 'name'},
    {
      '1': 'added_at',
      '3': 3,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Timestamp',
      '10': 'addedAt'
    },
    {'1': 'added_by', '3': 4, '4': 1, '5': 9, '10': 'addedBy'},
    {
      '1': 'last_used_at',
      '3': 5,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Timestamp',
      '10': 'lastUsedAt'
    },
  ],
};

/// Descriptor for `DeviceInfo`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List deviceInfoDescriptor = $convert.base64Decode(
    'CgpEZXZpY2VJbmZvEg4KAmlkGAEgASgJUgJpZBISCgRuYW1lGAIgASgJUgRuYW1lEjUKCGFkZG'
    'VkX2F0GAMgASgLMhouZ29vZ2xlLnByb3RvYnVmLlRpbWVzdGFtcFIHYWRkZWRBdBIZCghhZGRl'
    'ZF9ieRgEIAEoCVIHYWRkZWRCeRI8CgxsYXN0X3VzZWRfYXQYBSABKAsyGi5nb29nbGUucHJvdG'
    '9idWYuVGltZXN0YW1wUgpsYXN0VXNlZEF0');

@$core.Deprecated('Use addDeviceRequestDescriptor instead')
const AddDeviceRequest$json = {
  '1': 'AddDeviceRequest',
  '2': [
    {'1': 'name', '3': 1, '4': 1, '5': 9, '10': 'name'},
  ],
};

/// Descriptor for `AddDeviceRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List addDeviceRequestDescriptor = $convert
    .base64Decode('ChBBZGREZXZpY2VSZXF1ZXN0EhIKBG5hbWUYASABKAlSBG5hbWU=');

@$core.Deprecated('Use addDeviceResponseDescriptor instead')
const AddDeviceResponse$json = {
  '1': 'AddDeviceResponse',
  '2': [
    {
      '1': 'device',
      '3': 1,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.DeviceInfo',
      '10': 'device'
    },
    {'1': 'token', '3': 2, '4': 1, '5': 9, '10': 'token'},
  ],
};

/// Descriptor for `AddDeviceResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List addDeviceResponseDescriptor = $convert.base64Decode(
    'ChFBZGREZXZpY2VSZXNwb25zZRIzCgZkZXZpY2UYASABKAsyGy5wcm90ZWN0b3JhdGUudjEuRG'
    'V2aWNlSW5mb1IGZGV2aWNlEhQKBXRva2VuGAIgASgJUgV0b2tlbg==');

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
      '6': '.protectorate.v1.DeviceInfo',
      '10': 'devices'
    },
  ],
};

/// Descriptor for `ListDevicesResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List listDevicesResponseDescriptor = $convert.base64Decode(
    'ChNMaXN0RGV2aWNlc1Jlc3BvbnNlEjUKB2RldmljZXMYASADKAsyGy5wcm90ZWN0b3JhdGUudj'
    'EuRGV2aWNlSW5mb1IHZGV2aWNlcw==');

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
      '1': 'device',
      '3': 1,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.DeviceInfo',
      '10': 'device'
    },
  ],
};

/// Descriptor for `RevokeDeviceResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List revokeDeviceResponseDescriptor = $convert.base64Decode(
    'ChRSZXZva2VEZXZpY2VSZXNwb25zZRIzCgZkZXZpY2UYASABKAsyGy5wcm90ZWN0b3JhdGUudj'
    'EuRGV2aWNlSW5mb1IGZGV2aWNl');

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
      '1': 'device',
      '3': 1,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.DeviceInfo',
      '10': 'device'
    },
  ],
};

/// Descriptor for `RenameDeviceResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List renameDeviceResponseDescriptor = $convert.base64Decode(
    'ChRSZW5hbWVEZXZpY2VSZXNwb25zZRIzCgZkZXZpY2UYASABKAsyGy5wcm90ZWN0b3JhdGUudj'
    'EuRGV2aWNlSW5mb1IGZGV2aWNl');

const $core.Map<$core.String, $core.dynamic> DeviceServiceBase$json = {
  '1': 'DeviceService',
  '2': [
    {
      '1': 'AddDevice',
      '2': '.protectorate.v1.AddDeviceRequest',
      '3': '.protectorate.v1.AddDeviceResponse'
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
  '.protectorate.v1.AddDeviceRequest': AddDeviceRequest$json,
  '.protectorate.v1.AddDeviceResponse': AddDeviceResponse$json,
  '.protectorate.v1.DeviceInfo': DeviceInfo$json,
  '.google.protobuf.Timestamp': $0.Timestamp$json,
  '.protectorate.v1.ListDevicesRequest': ListDevicesRequest$json,
  '.protectorate.v1.ListDevicesResponse': ListDevicesResponse$json,
  '.protectorate.v1.RevokeDeviceRequest': RevokeDeviceRequest$json,
  '.protectorate.v1.RevokeDeviceResponse': RevokeDeviceResponse$json,
  '.protectorate.v1.RenameDeviceRequest': RenameDeviceRequest$json,
  '.protectorate.v1.RenameDeviceResponse': RenameDeviceResponse$json,
};

/// Descriptor for `DeviceService`. Decode as a `google.protobuf.ServiceDescriptorProto`.
final $typed_data.Uint8List deviceServiceDescriptor = $convert.base64Decode(
    'Cg1EZXZpY2VTZXJ2aWNlElIKCUFkZERldmljZRIhLnByb3RlY3RvcmF0ZS52MS5BZGREZXZpY2'
    'VSZXF1ZXN0GiIucHJvdGVjdG9yYXRlLnYxLkFkZERldmljZVJlc3BvbnNlEl0KC0xpc3REZXZp'
    'Y2VzEiMucHJvdGVjdG9yYXRlLnYxLkxpc3REZXZpY2VzUmVxdWVzdBokLnByb3RlY3RvcmF0ZS'
    '52MS5MaXN0RGV2aWNlc1Jlc3BvbnNlIgOQAgESWwoMUmV2b2tlRGV2aWNlEiQucHJvdGVjdG9y'
    'YXRlLnYxLlJldm9rZURldmljZVJlcXVlc3QaJS5wcm90ZWN0b3JhdGUudjEuUmV2b2tlRGV2aW'
    'NlUmVzcG9uc2USWwoMUmVuYW1lRGV2aWNlEiQucHJvdGVjdG9yYXRlLnYxLlJlbmFtZURldmlj'
    'ZVJlcXVlc3QaJS5wcm90ZWN0b3JhdGUudjEuUmVuYW1lRGV2aWNlUmVzcG9uc2U=');
