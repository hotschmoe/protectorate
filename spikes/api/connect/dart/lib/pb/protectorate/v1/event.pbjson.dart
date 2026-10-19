// This is a generated file - do not edit.
//
// Generated from protectorate/v1/event.proto.

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

@$core.Deprecated('Use eventDescriptor instead')
const Event$json = {
  '1': 'Event',
  '2': [
    {'1': 'seq', '3': 1, '4': 1, '5': 3, '10': 'seq'},
    {'1': 'type', '3': 2, '4': 1, '5': 9, '10': 'type'},
    {'1': 'sleeve_id', '3': 3, '4': 1, '5': 9, '10': 'sleeveId'},
    {'1': 'message', '3': 4, '4': 1, '5': 9, '10': 'message'},
  ],
};

/// Descriptor for `Event`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List eventDescriptor = $convert.base64Decode(
    'CgVFdmVudBIQCgNzZXEYASABKANSA3NlcRISCgR0eXBlGAIgASgJUgR0eXBlEhsKCXNsZWV2ZV'
    '9pZBgDIAEoCVIIc2xlZXZlSWQSGAoHbWVzc2FnZRgEIAEoCVIHbWVzc2FnZQ==');

@$core.Deprecated('Use subscribeRequestDescriptor instead')
const SubscribeRequest$json = {
  '1': 'SubscribeRequest',
  '2': [
    {'1': 'sleeve_id', '3': 1, '4': 1, '5': 9, '10': 'sleeveId'},
  ],
};

/// Descriptor for `SubscribeRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List subscribeRequestDescriptor = $convert.base64Decode(
    'ChBTdWJzY3JpYmVSZXF1ZXN0EhsKCXNsZWV2ZV9pZBgBIAEoCVIIc2xlZXZlSWQ=');

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
};

/// Descriptor for `EventService`. Decode as a `google.protobuf.ServiceDescriptorProto`.
final $typed_data.Uint8List eventServiceDescriptor = $convert.base64Decode(
    'CgxFdmVudFNlcnZpY2USSAoJU3Vic2NyaWJlEiEucHJvdGVjdG9yYXRlLnYxLlN1YnNjcmliZV'
    'JlcXVlc3QaFi5wcm90ZWN0b3JhdGUudjEuRXZlbnQwAQ==');
