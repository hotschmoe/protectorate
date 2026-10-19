// This is a generated file - do not edit.
//
// Generated from protectorate/v1/events.proto.

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

import 'package:protobuf/well_known_types/google/protobuf/struct.pbjson.dart'
    as $1;
import 'package:protobuf/well_known_types/google/protobuf/timestamp.pbjson.dart'
    as $0;

@$core.Deprecated('Use subscribeRequestDescriptor instead')
const SubscribeRequest$json = {
  '1': 'SubscribeRequest',
};

/// Descriptor for `SubscribeRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List subscribeRequestDescriptor =
    $convert.base64Decode('ChBTdWJzY3JpYmVSZXF1ZXN0');

@$core.Deprecated('Use eventDescriptor instead')
const Event$json = {
  '1': 'Event',
  '2': [
    {'1': 'id', '3': 1, '4': 1, '5': 9, '10': 'id'},
    {
      '1': 'time',
      '3': 2,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Timestamp',
      '10': 'time'
    },
    {'1': 'kind', '3': 3, '4': 1, '5': 9, '10': 'kind'},
    {'1': 'subject', '3': 4, '4': 1, '5': 9, '10': 'subject'},
    {
      '1': 'payload',
      '3': 5,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Struct',
      '10': 'payload'
    },
  ],
};

/// Descriptor for `Event`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List eventDescriptor = $convert.base64Decode(
    'CgVFdmVudBIOCgJpZBgBIAEoCVICaWQSLgoEdGltZRgCIAEoCzIaLmdvb2dsZS5wcm90b2J1Zi'
    '5UaW1lc3RhbXBSBHRpbWUSEgoEa2luZBgDIAEoCVIEa2luZBIYCgdzdWJqZWN0GAQgASgJUgdz'
    'dWJqZWN0EjEKB3BheWxvYWQYBSABKAsyFy5nb29nbGUucHJvdG9idWYuU3RydWN0UgdwYXlsb2'
    'Fk');

const $core.Map<$core.String, $core.dynamic> EventServiceBase$json = {
  '1': 'EventService',
  '2': [
    {
      '1': 'Subscribe',
      '2': '.protectorate.v1.SubscribeRequest',
      '3': '.protectorate.v1.Event',
      '6': true
    },
  ],
};

@$core.Deprecated('Use eventServiceDescriptor instead')
const $core.Map<$core.String, $core.Map<$core.String, $core.dynamic>>
    EventServiceBase$messageJson = {
  '.protectorate.v1.SubscribeRequest': SubscribeRequest$json,
  '.protectorate.v1.Event': Event$json,
  '.google.protobuf.Timestamp': $0.Timestamp$json,
  '.google.protobuf.Struct': $1.Struct$json,
  '.google.protobuf.Struct.FieldsEntry': $1.Struct_FieldsEntry$json,
  '.google.protobuf.Value': $1.Value$json,
  '.google.protobuf.ListValue': $1.ListValue$json,
};

/// Descriptor for `EventService`. Decode as a `google.protobuf.ServiceDescriptorProto`.
final $typed_data.Uint8List eventServiceDescriptor = $convert.base64Decode(
    'CgxFdmVudFNlcnZpY2USSAoJU3Vic2NyaWJlEiEucHJvdGVjdG9yYXRlLnYxLlN1YnNjcmliZV'
    'JlcXVlc3QaFi5wcm90ZWN0b3JhdGUudjEuRXZlbnQwAQ==');
