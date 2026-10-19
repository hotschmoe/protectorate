// This is a generated file - do not edit.
//
// Generated from protectorate/v1/message.proto.

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

@$core.Deprecated('Use messageKindDescriptor instead')
const MessageKind$json = {
  '1': 'MessageKind',
  '2': [
    {'1': 'MESSAGE_KIND_UNSPECIFIED', '2': 0},
    {'1': 'MESSAGE_KIND_NOTE', '2': 1},
    {'1': 'MESSAGE_KIND_REQUEST', '2': 2},
    {'1': 'MESSAGE_KIND_NOTICE', '2': 3},
  ],
};

/// Descriptor for `MessageKind`. Decode as a `google.protobuf.EnumDescriptorProto`.
final $typed_data.Uint8List messageKindDescriptor = $convert.base64Decode(
    'CgtNZXNzYWdlS2luZBIcChhNRVNTQUdFX0tJTkRfVU5TUEVDSUZJRUQQABIVChFNRVNTQUdFX0'
    'tJTkRfTk9URRABEhgKFE1FU1NBR0VfS0lORF9SRVFVRVNUEAISFwoTTUVTU0FHRV9LSU5EX05P'
    'VElDRRAD');

@$core.Deprecated('Use messageServiceSendRequestDescriptor instead')
const MessageServiceSendRequest$json = {
  '1': 'MessageServiceSendRequest',
  '2': [
    {'1': 'to', '3': 1, '4': 1, '5': 9, '10': 'to'},
    {'1': 'body', '3': 2, '4': 1, '5': 9, '10': 'body'},
    {'1': 'thread_id', '3': 3, '4': 1, '5': 9, '10': 'threadId'},
    {
      '1': 'kind',
      '3': 4,
      '4': 1,
      '5': 14,
      '6': '.protectorate.v1.MessageKind',
      '10': 'kind'
    },
    {
      '1': 'ack_required',
      '3': 5,
      '4': 1,
      '5': 8,
      '9': 0,
      '10': 'ackRequired',
      '17': true
    },
    {'1': 'request_id', '3': 6, '4': 1, '5': 9, '10': 'requestId'},
  ],
  '8': [
    {'1': '_ack_required'},
  ],
};

/// Descriptor for `MessageServiceSendRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List messageServiceSendRequestDescriptor = $convert.base64Decode(
    'ChlNZXNzYWdlU2VydmljZVNlbmRSZXF1ZXN0Eg4KAnRvGAEgASgJUgJ0bxISCgRib2R5GAIgAS'
    'gJUgRib2R5EhsKCXRocmVhZF9pZBgDIAEoCVIIdGhyZWFkSWQSMAoEa2luZBgEIAEoDjIcLnBy'
    'b3RlY3RvcmF0ZS52MS5NZXNzYWdlS2luZFIEa2luZBImCgxhY2tfcmVxdWlyZWQYBSABKAhIAF'
    'ILYWNrUmVxdWlyZWSIAQESHQoKcmVxdWVzdF9pZBgGIAEoCVIJcmVxdWVzdElkQg8KDV9hY2tf'
    'cmVxdWlyZWQ=');

@$core.Deprecated('Use messageServiceSendResponseDescriptor instead')
const MessageServiceSendResponse$json = {
  '1': 'MessageServiceSendResponse',
  '2': [
    {
      '1': 'message',
      '3': 1,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.MailMessage',
      '10': 'message'
    },
    {'1': 'duplicate', '3': 2, '4': 1, '5': 8, '10': 'duplicate'},
    {'1': 'waiting', '3': 3, '4': 1, '5': 8, '10': 'waiting'},
    {'1': 'backlog_item', '3': 4, '4': 1, '5': 9, '10': 'backlogItem'},
  ],
};

/// Descriptor for `MessageServiceSendResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List messageServiceSendResponseDescriptor = $convert.base64Decode(
    'ChpNZXNzYWdlU2VydmljZVNlbmRSZXNwb25zZRI2CgdtZXNzYWdlGAEgASgLMhwucHJvdGVjdG'
    '9yYXRlLnYxLk1haWxNZXNzYWdlUgdtZXNzYWdlEhwKCWR1cGxpY2F0ZRgCIAEoCFIJZHVwbGlj'
    'YXRlEhgKB3dhaXRpbmcYAyABKAhSB3dhaXRpbmcSIQoMYmFja2xvZ19pdGVtGAQgASgJUgtiYW'
    'NrbG9nSXRlbQ==');

@$core.Deprecated('Use inboxRequestDescriptor instead')
const InboxRequest$json = {
  '1': 'InboxRequest',
  '2': [
    {'1': 'unread_only', '3': 1, '4': 1, '5': 8, '10': 'unreadOnly'},
    {'1': 'peek', '3': 2, '4': 1, '5': 8, '10': 'peek'},
    {'1': 'as', '3': 3, '4': 1, '5': 9, '10': 'as'},
  ],
};

/// Descriptor for `InboxRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List inboxRequestDescriptor = $convert.base64Decode(
    'CgxJbmJveFJlcXVlc3QSHwoLdW5yZWFkX29ubHkYASABKAhSCnVucmVhZE9ubHkSEgoEcGVlax'
    'gCIAEoCFIEcGVlaxIOCgJhcxgDIAEoCVICYXM=');

@$core.Deprecated('Use inboxResponseDescriptor instead')
const InboxResponse$json = {
  '1': 'InboxResponse',
  '2': [
    {
      '1': 'messages',
      '3': 1,
      '4': 3,
      '5': 11,
      '6': '.protectorate.v1.MailMessage',
      '10': 'messages'
    },
  ],
};

/// Descriptor for `InboxResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List inboxResponseDescriptor = $convert.base64Decode(
    'Cg1JbmJveFJlc3BvbnNlEjgKCG1lc3NhZ2VzGAEgAygLMhwucHJvdGVjdG9yYXRlLnYxLk1haW'
    'xNZXNzYWdlUghtZXNzYWdlcw==');

@$core.Deprecated('Use ackRequestDescriptor instead')
const AckRequest$json = {
  '1': 'AckRequest',
  '2': [
    {'1': 'id', '3': 1, '4': 1, '5': 9, '10': 'id'},
    {'1': 'as', '3': 2, '4': 1, '5': 9, '10': 'as'},
  ],
};

/// Descriptor for `AckRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List ackRequestDescriptor = $convert.base64Decode(
    'CgpBY2tSZXF1ZXN0Eg4KAmlkGAEgASgJUgJpZBIOCgJhcxgCIAEoCVICYXM=');

@$core.Deprecated('Use ackResponseDescriptor instead')
const AckResponse$json = {
  '1': 'AckResponse',
  '2': [
    {
      '1': 'message',
      '3': 1,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.MailMessage',
      '10': 'message'
    },
  ],
};

/// Descriptor for `AckResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List ackResponseDescriptor = $convert.base64Decode(
    'CgtBY2tSZXNwb25zZRI2CgdtZXNzYWdlGAEgASgLMhwucHJvdGVjdG9yYXRlLnYxLk1haWxNZX'
    'NzYWdlUgdtZXNzYWdl');

@$core.Deprecated('Use replyRequestDescriptor instead')
const ReplyRequest$json = {
  '1': 'ReplyRequest',
  '2': [
    {'1': 'id', '3': 1, '4': 1, '5': 9, '10': 'id'},
    {'1': 'body', '3': 2, '4': 1, '5': 9, '10': 'body'},
    {'1': 'request_id', '3': 3, '4': 1, '5': 9, '10': 'requestId'},
  ],
};

/// Descriptor for `ReplyRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List replyRequestDescriptor = $convert.base64Decode(
    'CgxSZXBseVJlcXVlc3QSDgoCaWQYASABKAlSAmlkEhIKBGJvZHkYAiABKAlSBGJvZHkSHQoKcm'
    'VxdWVzdF9pZBgDIAEoCVIJcmVxdWVzdElk');

@$core.Deprecated('Use replyResponseDescriptor instead')
const ReplyResponse$json = {
  '1': 'ReplyResponse',
  '2': [
    {
      '1': 'message',
      '3': 1,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.MailMessage',
      '10': 'message'
    },
  ],
};

/// Descriptor for `ReplyResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List replyResponseDescriptor = $convert.base64Decode(
    'Cg1SZXBseVJlc3BvbnNlEjYKB21lc3NhZ2UYASABKAsyHC5wcm90ZWN0b3JhdGUudjEuTWFpbE'
    '1lc3NhZ2VSB21lc3NhZ2U=');

@$core.Deprecated('Use threadRequestDescriptor instead')
const ThreadRequest$json = {
  '1': 'ThreadRequest',
  '2': [
    {'1': 'thread_id', '3': 1, '4': 1, '5': 9, '10': 'threadId'},
  ],
};

/// Descriptor for `ThreadRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List threadRequestDescriptor = $convert.base64Decode(
    'Cg1UaHJlYWRSZXF1ZXN0EhsKCXRocmVhZF9pZBgBIAEoCVIIdGhyZWFkSWQ=');

@$core.Deprecated('Use threadResponseDescriptor instead')
const ThreadResponse$json = {
  '1': 'ThreadResponse',
  '2': [
    {
      '1': 'messages',
      '3': 1,
      '4': 3,
      '5': 11,
      '6': '.protectorate.v1.MailMessage',
      '10': 'messages'
    },
  ],
};

/// Descriptor for `ThreadResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List threadResponseDescriptor = $convert.base64Decode(
    'Cg5UaHJlYWRSZXNwb25zZRI4CghtZXNzYWdlcxgBIAMoCzIcLnByb3RlY3RvcmF0ZS52MS5NYW'
    'lsTWVzc2FnZVIIbWVzc2FnZXM=');

@$core.Deprecated('Use mailMessageDescriptor instead')
const MailMessage$json = {
  '1': 'MailMessage',
  '2': [
    {'1': 'id', '3': 1, '4': 1, '5': 9, '10': 'id'},
    {'1': 'thread_id', '3': 2, '4': 1, '5': 9, '10': 'threadId'},
    {'1': 'from', '3': 3, '4': 1, '5': 9, '10': 'from'},
    {'1': 'to', '3': 4, '4': 1, '5': 9, '10': 'to'},
    {
      '1': 'kind',
      '3': 5,
      '4': 1,
      '5': 14,
      '6': '.protectorate.v1.MessageKind',
      '10': 'kind'
    },
    {'1': 'ack_required', '3': 6, '4': 1, '5': 8, '10': 'ackRequired'},
    {'1': 'body', '3': 7, '4': 1, '5': 9, '10': 'body'},
    {'1': 'reply_to', '3': 8, '4': 1, '5': 9, '10': 'replyTo'},
    {
      '1': 'created_at',
      '3': 9,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Timestamp',
      '10': 'createdAt'
    },
    {
      '1': 'deliveries',
      '3': 10,
      '4': 3,
      '5': 11,
      '6': '.protectorate.v1.MailDelivery',
      '10': 'deliveries'
    },
    {
      '1': 'escalated_at',
      '3': 11,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Timestamp',
      '10': 'escalatedAt'
    },
    {'1': 'backlog_item', '3': 12, '4': 1, '5': 9, '10': 'backlogItem'},
  ],
};

/// Descriptor for `MailMessage`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List mailMessageDescriptor = $convert.base64Decode(
    'CgtNYWlsTWVzc2FnZRIOCgJpZBgBIAEoCVICaWQSGwoJdGhyZWFkX2lkGAIgASgJUgh0aHJlYW'
    'RJZBISCgRmcm9tGAMgASgJUgRmcm9tEg4KAnRvGAQgASgJUgJ0bxIwCgRraW5kGAUgASgOMhwu'
    'cHJvdGVjdG9yYXRlLnYxLk1lc3NhZ2VLaW5kUgRraW5kEiEKDGFja19yZXF1aXJlZBgGIAEoCF'
    'ILYWNrUmVxdWlyZWQSEgoEYm9keRgHIAEoCVIEYm9keRIZCghyZXBseV90bxgIIAEoCVIHcmVw'
    'bHlUbxI5CgpjcmVhdGVkX2F0GAkgASgLMhouZ29vZ2xlLnByb3RvYnVmLlRpbWVzdGFtcFIJY3'
    'JlYXRlZEF0Ej0KCmRlbGl2ZXJpZXMYCiADKAsyHS5wcm90ZWN0b3JhdGUudjEuTWFpbERlbGl2'
    'ZXJ5UgpkZWxpdmVyaWVzEj0KDGVzY2FsYXRlZF9hdBgLIAEoCzIaLmdvb2dsZS5wcm90b2J1Zi'
    '5UaW1lc3RhbXBSC2VzY2FsYXRlZEF0EiEKDGJhY2tsb2dfaXRlbRgMIAEoCVILYmFja2xvZ0l0'
    'ZW0=');

@$core.Deprecated('Use mailDeliveryDescriptor instead')
const MailDelivery$json = {
  '1': 'MailDelivery',
  '2': [
    {'1': 'recipient', '3': 1, '4': 1, '5': 9, '10': 'recipient'},
    {
      '1': 'delivered_at',
      '3': 2,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Timestamp',
      '10': 'deliveredAt'
    },
    {
      '1': 'read_at',
      '3': 3,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Timestamp',
      '10': 'readAt'
    },
    {
      '1': 'acked_at',
      '3': 4,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Timestamp',
      '10': 'ackedAt'
    },
    {'1': 'reminders', '3': 5, '4': 1, '5': 13, '10': 'reminders'},
  ],
};

/// Descriptor for `MailDelivery`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List mailDeliveryDescriptor = $convert.base64Decode(
    'CgxNYWlsRGVsaXZlcnkSHAoJcmVjaXBpZW50GAEgASgJUglyZWNpcGllbnQSPQoMZGVsaXZlcm'
    'VkX2F0GAIgASgLMhouZ29vZ2xlLnByb3RvYnVmLlRpbWVzdGFtcFILZGVsaXZlcmVkQXQSMwoH'
    'cmVhZF9hdBgDIAEoCzIaLmdvb2dsZS5wcm90b2J1Zi5UaW1lc3RhbXBSBnJlYWRBdBI1CghhY2'
    'tlZF9hdBgEIAEoCzIaLmdvb2dsZS5wcm90b2J1Zi5UaW1lc3RhbXBSB2Fja2VkQXQSHAoJcmVt'
    'aW5kZXJzGAUgASgNUglyZW1pbmRlcnM=');

const $core.Map<$core.String, $core.dynamic> MessageServiceBase$json = {
  '1': 'MessageService',
  '2': [
    {
      '1': 'Send',
      '2': '.protectorate.v1.MessageServiceSendRequest',
      '3': '.protectorate.v1.MessageServiceSendResponse'
    },
    {
      '1': 'Inbox',
      '2': '.protectorate.v1.InboxRequest',
      '3': '.protectorate.v1.InboxResponse'
    },
    {
      '1': 'Ack',
      '2': '.protectorate.v1.AckRequest',
      '3': '.protectorate.v1.AckResponse'
    },
    {
      '1': 'Reply',
      '2': '.protectorate.v1.ReplyRequest',
      '3': '.protectorate.v1.ReplyResponse'
    },
    {
      '1': 'Thread',
      '2': '.protectorate.v1.ThreadRequest',
      '3': '.protectorate.v1.ThreadResponse',
      '4': {'34': 1},
    },
  ],
};

@$core.Deprecated('Use messageServiceDescriptor instead')
const $core.Map<$core.String, $core.Map<$core.String, $core.dynamic>>
    MessageServiceBase$messageJson = {
  '.protectorate.v1.MessageServiceSendRequest': MessageServiceSendRequest$json,
  '.protectorate.v1.MessageServiceSendResponse':
      MessageServiceSendResponse$json,
  '.protectorate.v1.MailMessage': MailMessage$json,
  '.google.protobuf.Timestamp': $0.Timestamp$json,
  '.protectorate.v1.MailDelivery': MailDelivery$json,
  '.protectorate.v1.InboxRequest': InboxRequest$json,
  '.protectorate.v1.InboxResponse': InboxResponse$json,
  '.protectorate.v1.AckRequest': AckRequest$json,
  '.protectorate.v1.AckResponse': AckResponse$json,
  '.protectorate.v1.ReplyRequest': ReplyRequest$json,
  '.protectorate.v1.ReplyResponse': ReplyResponse$json,
  '.protectorate.v1.ThreadRequest': ThreadRequest$json,
  '.protectorate.v1.ThreadResponse': ThreadResponse$json,
};

/// Descriptor for `MessageService`. Decode as a `google.protobuf.ServiceDescriptorProto`.
final $typed_data.Uint8List messageServiceDescriptor = $convert.base64Decode(
    'Cg5NZXNzYWdlU2VydmljZRJfCgRTZW5kEioucHJvdGVjdG9yYXRlLnYxLk1lc3NhZ2VTZXJ2aW'
    'NlU2VuZFJlcXVlc3QaKy5wcm90ZWN0b3JhdGUudjEuTWVzc2FnZVNlcnZpY2VTZW5kUmVzcG9u'
    'c2USRgoFSW5ib3gSHS5wcm90ZWN0b3JhdGUudjEuSW5ib3hSZXF1ZXN0Gh4ucHJvdGVjdG9yYX'
    'RlLnYxLkluYm94UmVzcG9uc2USQAoDQWNrEhsucHJvdGVjdG9yYXRlLnYxLkFja1JlcXVlc3Qa'
    'HC5wcm90ZWN0b3JhdGUudjEuQWNrUmVzcG9uc2USRgoFUmVwbHkSHS5wcm90ZWN0b3JhdGUudj'
    'EuUmVwbHlSZXF1ZXN0Gh4ucHJvdGVjdG9yYXRlLnYxLlJlcGx5UmVzcG9uc2USTgoGVGhyZWFk'
    'Eh4ucHJvdGVjdG9yYXRlLnYxLlRocmVhZFJlcXVlc3QaHy5wcm90ZWN0b3JhdGUudjEuVGhyZW'
    'FkUmVzcG9uc2UiA5ACAQ==');
