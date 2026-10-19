// This is a generated file - do not edit.
//
// Generated from protectorate/v1/desk.proto.

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

@$core.Deprecated('Use deskReasonDescriptor instead')
const DeskReason$json = {
  '1': 'DeskReason',
  '2': [
    {'1': 'DESK_REASON_UNSPECIFIED', '2': 0},
    {'1': 'DESK_REASON_AVAILABLE', '2': 1},
    {'1': 'DESK_REASON_OUTSIDE_WINDOW', '2': 2},
    {'1': 'DESK_REASON_USER_ACTIVE', '2': 3},
    {'1': 'DESK_REASON_NOT_INTERACTIVE', '2': 4},
    {'1': 'DESK_REASON_LOCKED', '2': 5},
    {'1': 'DESK_REASON_UNSUPPORTED', '2': 6},
    {'1': 'DESK_REASON_UNKNOWN', '2': 7},
  ],
};

/// Descriptor for `DeskReason`. Decode as a `google.protobuf.EnumDescriptorProto`.
final $typed_data.Uint8List deskReasonDescriptor = $convert.base64Decode(
    'CgpEZXNrUmVhc29uEhsKF0RFU0tfUkVBU09OX1VOU1BFQ0lGSUVEEAASGQoVREVTS19SRUFTT0'
    '5fQVZBSUxBQkxFEAESHgoaREVTS19SRUFTT05fT1VUU0lERV9XSU5ET1cQAhIbChdERVNLX1JF'
    'QVNPTl9VU0VSX0FDVElWRRADEh8KG0RFU0tfUkVBU09OX05PVF9JTlRFUkFDVElWRRAEEhYKEk'
    'RFU0tfUkVBU09OX0xPQ0tFRBAFEhsKF0RFU0tfUkVBU09OX1VOU1VQUE9SVEVEEAYSFwoTREVT'
    'S19SRUFTT05fVU5LTk9XThAH');

@$core.Deprecated('Use deskTargetDescriptor instead')
const DeskTarget$json = {
  '1': 'DeskTarget',
  '2': [
    {'1': 'name', '3': 1, '4': 1, '5': 9, '10': 'name'},
    {'1': 'lease_id', '3': 2, '4': 1, '5': 9, '10': 'leaseId'},
    {'1': 'allow_family', '3': 3, '4': 1, '5': 8, '10': 'allowFamily'},
  ],
};

/// Descriptor for `DeskTarget`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List deskTargetDescriptor = $convert.base64Decode(
    'CgpEZXNrVGFyZ2V0EhIKBG5hbWUYASABKAlSBG5hbWUSGQoIbGVhc2VfaWQYAiABKAlSB2xlYX'
    'NlSWQSIQoMYWxsb3dfZmFtaWx5GAMgASgIUgthbGxvd0ZhbWlseQ==');

@$core.Deprecated('Use screenshotRequestDescriptor instead')
const ScreenshotRequest$json = {
  '1': 'ScreenshotRequest',
  '2': [
    {
      '1': 'target',
      '3': 1,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.DeskTarget',
      '10': 'target'
    },
  ],
};

/// Descriptor for `ScreenshotRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List screenshotRequestDescriptor = $convert.base64Decode(
    'ChFTY3JlZW5zaG90UmVxdWVzdBIzCgZ0YXJnZXQYASABKAsyGy5wcm90ZWN0b3JhdGUudjEuRG'
    'Vza1RhcmdldFIGdGFyZ2V0');

@$core.Deprecated('Use screenshotResponseDescriptor instead')
const ScreenshotResponse$json = {
  '1': 'ScreenshotResponse',
  '2': [
    {'1': 'png', '3': 1, '4': 1, '5': 12, '10': 'png'},
    {'1': 'width', '3': 2, '4': 1, '5': 5, '10': 'width'},
    {'1': 'height', '3': 3, '4': 1, '5': 5, '10': 'height'},
  ],
};

/// Descriptor for `ScreenshotResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List screenshotResponseDescriptor = $convert.base64Decode(
    'ChJTY3JlZW5zaG90UmVzcG9uc2USEAoDcG5nGAEgASgMUgNwbmcSFAoFd2lkdGgYAiABKAVSBX'
    'dpZHRoEhYKBmhlaWdodBgDIAEoBVIGaGVpZ2h0');

@$core.Deprecated('Use listWindowsRequestDescriptor instead')
const ListWindowsRequest$json = {
  '1': 'ListWindowsRequest',
  '2': [
    {
      '1': 'target',
      '3': 1,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.DeskTarget',
      '10': 'target'
    },
  ],
};

/// Descriptor for `ListWindowsRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List listWindowsRequestDescriptor = $convert.base64Decode(
    'ChJMaXN0V2luZG93c1JlcXVlc3QSMwoGdGFyZ2V0GAEgASgLMhsucHJvdGVjdG9yYXRlLnYxLk'
    'Rlc2tUYXJnZXRSBnRhcmdldA==');

@$core.Deprecated('Use deskWindowDescriptor instead')
const DeskWindow$json = {
  '1': 'DeskWindow',
  '2': [
    {'1': 'title', '3': 1, '4': 1, '5': 9, '10': 'title'},
    {'1': 'x', '3': 2, '4': 1, '5': 5, '10': 'x'},
    {'1': 'y', '3': 3, '4': 1, '5': 5, '10': 'y'},
    {'1': 'width', '3': 4, '4': 1, '5': 5, '10': 'width'},
    {'1': 'height', '3': 5, '4': 1, '5': 5, '10': 'height'},
    {'1': 'pid', '3': 6, '4': 1, '5': 13, '10': 'pid'},
    {'1': 'minimized', '3': 7, '4': 1, '5': 8, '10': 'minimized'},
  ],
};

/// Descriptor for `DeskWindow`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List deskWindowDescriptor = $convert.base64Decode(
    'CgpEZXNrV2luZG93EhQKBXRpdGxlGAEgASgJUgV0aXRsZRIMCgF4GAIgASgFUgF4EgwKAXkYAy'
    'ABKAVSAXkSFAoFd2lkdGgYBCABKAVSBXdpZHRoEhYKBmhlaWdodBgFIAEoBVIGaGVpZ2h0EhAK'
    'A3BpZBgGIAEoDVIDcGlkEhwKCW1pbmltaXplZBgHIAEoCFIJbWluaW1pemVk');

@$core.Deprecated('Use listWindowsResponseDescriptor instead')
const ListWindowsResponse$json = {
  '1': 'ListWindowsResponse',
  '2': [
    {
      '1': 'windows',
      '3': 1,
      '4': 3,
      '5': 11,
      '6': '.protectorate.v1.DeskWindow',
      '10': 'windows'
    },
  ],
};

/// Descriptor for `ListWindowsResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List listWindowsResponseDescriptor = $convert.base64Decode(
    'ChNMaXN0V2luZG93c1Jlc3BvbnNlEjUKB3dpbmRvd3MYASADKAsyGy5wcm90ZWN0b3JhdGUudj'
    'EuRGVza1dpbmRvd1IHd2luZG93cw==');

@$core.Deprecated('Use clickRequestDescriptor instead')
const ClickRequest$json = {
  '1': 'ClickRequest',
  '2': [
    {
      '1': 'target',
      '3': 1,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.DeskTarget',
      '10': 'target'
    },
    {'1': 'x', '3': 2, '4': 1, '5': 5, '10': 'x'},
    {'1': 'y', '3': 3, '4': 1, '5': 5, '10': 'y'},
    {'1': 'button', '3': 4, '4': 1, '5': 9, '10': 'button'},
    {'1': 'double', '3': 5, '4': 1, '5': 8, '10': 'double'},
  ],
};

/// Descriptor for `ClickRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List clickRequestDescriptor = $convert.base64Decode(
    'CgxDbGlja1JlcXVlc3QSMwoGdGFyZ2V0GAEgASgLMhsucHJvdGVjdG9yYXRlLnYxLkRlc2tUYX'
    'JnZXRSBnRhcmdldBIMCgF4GAIgASgFUgF4EgwKAXkYAyABKAVSAXkSFgoGYnV0dG9uGAQgASgJ'
    'UgZidXR0b24SFgoGZG91YmxlGAUgASgIUgZkb3VibGU=');

@$core.Deprecated('Use moveRequestDescriptor instead')
const MoveRequest$json = {
  '1': 'MoveRequest',
  '2': [
    {
      '1': 'target',
      '3': 1,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.DeskTarget',
      '10': 'target'
    },
    {'1': 'x', '3': 2, '4': 1, '5': 5, '10': 'x'},
    {'1': 'y', '3': 3, '4': 1, '5': 5, '10': 'y'},
  ],
};

/// Descriptor for `MoveRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List moveRequestDescriptor = $convert.base64Decode(
    'CgtNb3ZlUmVxdWVzdBIzCgZ0YXJnZXQYASABKAsyGy5wcm90ZWN0b3JhdGUudjEuRGVza1Rhcm'
    'dldFIGdGFyZ2V0EgwKAXgYAiABKAVSAXgSDAoBeRgDIAEoBVIBeQ==');

@$core.Deprecated('Use typeRequestDescriptor instead')
const TypeRequest$json = {
  '1': 'TypeRequest',
  '2': [
    {
      '1': 'target',
      '3': 1,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.DeskTarget',
      '10': 'target'
    },
    {'1': 'text', '3': 2, '4': 1, '5': 9, '10': 'text'},
  ],
};

/// Descriptor for `TypeRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List typeRequestDescriptor = $convert.base64Decode(
    'CgtUeXBlUmVxdWVzdBIzCgZ0YXJnZXQYASABKAsyGy5wcm90ZWN0b3JhdGUudjEuRGVza1Rhcm'
    'dldFIGdGFyZ2V0EhIKBHRleHQYAiABKAlSBHRleHQ=');

@$core.Deprecated('Use keyRequestDescriptor instead')
const KeyRequest$json = {
  '1': 'KeyRequest',
  '2': [
    {
      '1': 'target',
      '3': 1,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.DeskTarget',
      '10': 'target'
    },
    {'1': 'chord', '3': 2, '4': 1, '5': 9, '10': 'chord'},
  ],
};

/// Descriptor for `KeyRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List keyRequestDescriptor = $convert.base64Decode(
    'CgpLZXlSZXF1ZXN0EjMKBnRhcmdldBgBIAEoCzIbLnByb3RlY3RvcmF0ZS52MS5EZXNrVGFyZ2'
    'V0UgZ0YXJnZXQSFAoFY2hvcmQYAiABKAlSBWNob3Jk');

@$core.Deprecated('Use runRequestDescriptor instead')
const RunRequest$json = {
  '1': 'RunRequest',
  '2': [
    {
      '1': 'target',
      '3': 1,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.DeskTarget',
      '10': 'target'
    },
    {'1': 'program', '3': 2, '4': 1, '5': 9, '10': 'program'},
    {'1': 'args', '3': 3, '4': 3, '5': 9, '10': 'args'},
  ],
};

/// Descriptor for `RunRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List runRequestDescriptor = $convert.base64Decode(
    'CgpSdW5SZXF1ZXN0EjMKBnRhcmdldBgBIAEoCzIbLnByb3RlY3RvcmF0ZS52MS5EZXNrVGFyZ2'
    'V0UgZ0YXJnZXQSGAoHcHJvZ3JhbRgCIAEoCVIHcHJvZ3JhbRISCgRhcmdzGAMgAygJUgRhcmdz');

@$core.Deprecated('Use inputResponseDescriptor instead')
const InputResponse$json = {
  '1': 'InputResponse',
};

/// Descriptor for `InputResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List inputResponseDescriptor =
    $convert.base64Decode('Cg1JbnB1dFJlc3BvbnNl');

@$core.Deprecated('Use runResponseDescriptor instead')
const RunResponse$json = {
  '1': 'RunResponse',
  '2': [
    {'1': 'pid', '3': 1, '4': 1, '5': 13, '10': 'pid'},
  ],
};

/// Descriptor for `RunResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List runResponseDescriptor =
    $convert.base64Decode('CgtSdW5SZXNwb25zZRIQCgNwaWQYASABKA1SA3BpZA==');

@$core.Deprecated('Use deskActionRequestDescriptor instead')
const DeskActionRequest$json = {
  '1': 'DeskActionRequest',
  '2': [
    {
      '1': 'screenshot',
      '3': 1,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.ScreenshotRequest',
      '9': 0,
      '10': 'screenshot'
    },
    {
      '1': 'list_windows',
      '3': 2,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.ListWindowsRequest',
      '9': 0,
      '10': 'listWindows'
    },
    {
      '1': 'click',
      '3': 3,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.ClickRequest',
      '9': 0,
      '10': 'click'
    },
    {
      '1': 'move',
      '3': 4,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.MoveRequest',
      '9': 0,
      '10': 'move'
    },
    {
      '1': 'type',
      '3': 5,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.TypeRequest',
      '9': 0,
      '10': 'type'
    },
    {
      '1': 'key',
      '3': 6,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.KeyRequest',
      '9': 0,
      '10': 'key'
    },
    {
      '1': 'run',
      '3': 7,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.RunRequest',
      '9': 0,
      '10': 'run'
    },
  ],
  '8': [
    {'1': 'action'},
  ],
};

/// Descriptor for `DeskActionRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List deskActionRequestDescriptor = $convert.base64Decode(
    'ChFEZXNrQWN0aW9uUmVxdWVzdBJECgpzY3JlZW5zaG90GAEgASgLMiIucHJvdGVjdG9yYXRlLn'
    'YxLlNjcmVlbnNob3RSZXF1ZXN0SABSCnNjcmVlbnNob3QSSAoMbGlzdF93aW5kb3dzGAIgASgL'
    'MiMucHJvdGVjdG9yYXRlLnYxLkxpc3RXaW5kb3dzUmVxdWVzdEgAUgtsaXN0V2luZG93cxI1Cg'
    'VjbGljaxgDIAEoCzIdLnByb3RlY3RvcmF0ZS52MS5DbGlja1JlcXVlc3RIAFIFY2xpY2sSMgoE'
    'bW92ZRgEIAEoCzIcLnByb3RlY3RvcmF0ZS52MS5Nb3ZlUmVxdWVzdEgAUgRtb3ZlEjIKBHR5cG'
    'UYBSABKAsyHC5wcm90ZWN0b3JhdGUudjEuVHlwZVJlcXVlc3RIAFIEdHlwZRIvCgNrZXkYBiAB'
    'KAsyGy5wcm90ZWN0b3JhdGUudjEuS2V5UmVxdWVzdEgAUgNrZXkSLwoDcnVuGAcgASgLMhsucH'
    'JvdGVjdG9yYXRlLnYxLlJ1blJlcXVlc3RIAFIDcnVuQggKBmFjdGlvbg==');

@$core.Deprecated('Use deskActionResponseDescriptor instead')
const DeskActionResponse$json = {
  '1': 'DeskActionResponse',
  '2': [
    {
      '1': 'screenshot',
      '3': 1,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.ScreenshotResponse',
      '9': 0,
      '10': 'screenshot'
    },
    {
      '1': 'list_windows',
      '3': 2,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.ListWindowsResponse',
      '9': 0,
      '10': 'listWindows'
    },
    {
      '1': 'input',
      '3': 3,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.InputResponse',
      '9': 0,
      '10': 'input'
    },
    {
      '1': 'run',
      '3': 4,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.RunResponse',
      '9': 0,
      '10': 'run'
    },
  ],
  '8': [
    {'1': 'result'},
  ],
};

/// Descriptor for `DeskActionResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List deskActionResponseDescriptor = $convert.base64Decode(
    'ChJEZXNrQWN0aW9uUmVzcG9uc2USRQoKc2NyZWVuc2hvdBgBIAEoCzIjLnByb3RlY3RvcmF0ZS'
    '52MS5TY3JlZW5zaG90UmVzcG9uc2VIAFIKc2NyZWVuc2hvdBJJCgxsaXN0X3dpbmRvd3MYAiAB'
    'KAsyJC5wcm90ZWN0b3JhdGUudjEuTGlzdFdpbmRvd3NSZXNwb25zZUgAUgtsaXN0V2luZG93cx'
    'I2CgVpbnB1dBgDIAEoCzIeLnByb3RlY3RvcmF0ZS52MS5JbnB1dFJlc3BvbnNlSABSBWlucHV0'
    'EjAKA3J1bhgEIAEoCzIcLnByb3RlY3RvcmF0ZS52MS5SdW5SZXNwb25zZUgAUgNydW5CCAoGcm'
    'VzdWx0');

@$core.Deprecated('Use deskPolicyDescriptor instead')
const DeskPolicy$json = {
  '1': 'DeskPolicy',
  '2': [
    {'1': 'window', '3': 1, '4': 1, '5': 9, '10': 'window'},
    {
      '1': 'idle',
      '3': 2,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Duration',
      '10': 'idle'
    },
    {'1': 'timezone', '3': 3, '4': 1, '5': 9, '10': 'timezone'},
    {'1': 'lock_after', '3': 4, '4': 1, '5': 8, '10': 'lockAfter'},
  ],
};

/// Descriptor for `DeskPolicy`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List deskPolicyDescriptor = $convert.base64Decode(
    'CgpEZXNrUG9saWN5EhYKBndpbmRvdxgBIAEoCVIGd2luZG93Ei0KBGlkbGUYAiABKAsyGS5nb2'
    '9nbGUucHJvdG9idWYuRHVyYXRpb25SBGlkbGUSGgoIdGltZXpvbmUYAyABKAlSCHRpbWV6b25l'
    'Eh0KCmxvY2tfYWZ0ZXIYBCABKAhSCWxvY2tBZnRlcg==');

@$core.Deprecated('Use deskStateDescriptor instead')
const DeskState$json = {
  '1': 'DeskState',
  '2': [
    {'1': 'available', '3': 1, '4': 1, '5': 8, '10': 'available'},
    {
      '1': 'reason',
      '3': 2,
      '4': 1,
      '5': 14,
      '6': '.protectorate.v1.DeskReason',
      '10': 'reason'
    },
    {'1': 'detail', '3': 3, '4': 1, '5': 9, '10': 'detail'},
    {
      '1': 'last_input',
      '3': 4,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Timestamp',
      '10': 'lastInput'
    },
    {
      '1': 'evaluated_at',
      '3': 5,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Timestamp',
      '10': 'evaluatedAt'
    },
    {
      '1': 'policy',
      '3': 6,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.DeskPolicy',
      '10': 'policy'
    },
  ],
};

/// Descriptor for `DeskState`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List deskStateDescriptor = $convert.base64Decode(
    'CglEZXNrU3RhdGUSHAoJYXZhaWxhYmxlGAEgASgIUglhdmFpbGFibGUSMwoGcmVhc29uGAIgAS'
    'gOMhsucHJvdGVjdG9yYXRlLnYxLkRlc2tSZWFzb25SBnJlYXNvbhIWCgZkZXRhaWwYAyABKAlS'
    'BmRldGFpbBI5CgpsYXN0X2lucHV0GAQgASgLMhouZ29vZ2xlLnByb3RvYnVmLlRpbWVzdGFtcF'
    'IJbGFzdElucHV0Ej0KDGV2YWx1YXRlZF9hdBgFIAEoCzIaLmdvb2dsZS5wcm90b2J1Zi5UaW1l'
    'c3RhbXBSC2V2YWx1YXRlZEF0EjMKBnBvbGljeRgGIAEoCzIbLnByb3RlY3RvcmF0ZS52MS5EZX'
    'NrUG9saWN5UgZwb2xpY3k=');

const $core.Map<$core.String, $core.dynamic> DeskServiceBase$json = {
  '1': 'DeskService',
  '2': [
    {
      '1': 'Screenshot',
      '2': '.protectorate.v1.ScreenshotRequest',
      '3': '.protectorate.v1.ScreenshotResponse'
    },
    {
      '1': 'ListWindows',
      '2': '.protectorate.v1.ListWindowsRequest',
      '3': '.protectorate.v1.ListWindowsResponse'
    },
    {
      '1': 'Click',
      '2': '.protectorate.v1.ClickRequest',
      '3': '.protectorate.v1.InputResponse'
    },
    {
      '1': 'Move',
      '2': '.protectorate.v1.MoveRequest',
      '3': '.protectorate.v1.InputResponse'
    },
    {
      '1': 'Type',
      '2': '.protectorate.v1.TypeRequest',
      '3': '.protectorate.v1.InputResponse'
    },
    {
      '1': 'Key',
      '2': '.protectorate.v1.KeyRequest',
      '3': '.protectorate.v1.InputResponse'
    },
    {
      '1': 'Run',
      '2': '.protectorate.v1.RunRequest',
      '3': '.protectorate.v1.RunResponse'
    },
  ],
};

@$core.Deprecated('Use deskServiceDescriptor instead')
const $core.Map<$core.String, $core.Map<$core.String, $core.dynamic>>
    DeskServiceBase$messageJson = {
  '.protectorate.v1.ScreenshotRequest': ScreenshotRequest$json,
  '.protectorate.v1.DeskTarget': DeskTarget$json,
  '.protectorate.v1.ScreenshotResponse': ScreenshotResponse$json,
  '.protectorate.v1.ListWindowsRequest': ListWindowsRequest$json,
  '.protectorate.v1.ListWindowsResponse': ListWindowsResponse$json,
  '.protectorate.v1.DeskWindow': DeskWindow$json,
  '.protectorate.v1.ClickRequest': ClickRequest$json,
  '.protectorate.v1.InputResponse': InputResponse$json,
  '.protectorate.v1.MoveRequest': MoveRequest$json,
  '.protectorate.v1.TypeRequest': TypeRequest$json,
  '.protectorate.v1.KeyRequest': KeyRequest$json,
  '.protectorate.v1.RunRequest': RunRequest$json,
  '.protectorate.v1.RunResponse': RunResponse$json,
};

/// Descriptor for `DeskService`. Decode as a `google.protobuf.ServiceDescriptorProto`.
final $typed_data.Uint8List deskServiceDescriptor = $convert.base64Decode(
    'CgtEZXNrU2VydmljZRJVCgpTY3JlZW5zaG90EiIucHJvdGVjdG9yYXRlLnYxLlNjcmVlbnNob3'
    'RSZXF1ZXN0GiMucHJvdGVjdG9yYXRlLnYxLlNjcmVlbnNob3RSZXNwb25zZRJYCgtMaXN0V2lu'
    'ZG93cxIjLnByb3RlY3RvcmF0ZS52MS5MaXN0V2luZG93c1JlcXVlc3QaJC5wcm90ZWN0b3JhdG'
    'UudjEuTGlzdFdpbmRvd3NSZXNwb25zZRJGCgVDbGljaxIdLnByb3RlY3RvcmF0ZS52MS5DbGlj'
    'a1JlcXVlc3QaHi5wcm90ZWN0b3JhdGUudjEuSW5wdXRSZXNwb25zZRJECgRNb3ZlEhwucHJvdG'
    'VjdG9yYXRlLnYxLk1vdmVSZXF1ZXN0Gh4ucHJvdGVjdG9yYXRlLnYxLklucHV0UmVzcG9uc2US'
    'RAoEVHlwZRIcLnByb3RlY3RvcmF0ZS52MS5UeXBlUmVxdWVzdBoeLnByb3RlY3RvcmF0ZS52MS'
    '5JbnB1dFJlc3BvbnNlEkIKA0tleRIbLnByb3RlY3RvcmF0ZS52MS5LZXlSZXF1ZXN0Gh4ucHJv'
    'dGVjdG9yYXRlLnYxLklucHV0UmVzcG9uc2USQAoDUnVuEhsucHJvdGVjdG9yYXRlLnYxLlJ1bl'
    'JlcXVlc3QaHC5wcm90ZWN0b3JhdGUudjEuUnVuUmVzcG9uc2U=');
