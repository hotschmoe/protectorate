// This is a generated file - do not edit.
//
// Generated from protectorate/v1/backlog.proto.

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

@$core.Deprecated('Use itemStateDescriptor instead')
const ItemState$json = {
  '1': 'ItemState',
  '2': [
    {'1': 'ITEM_STATE_UNSPECIFIED', '2': 0},
    {'1': 'ITEM_STATE_TODO', '2': 1},
    {'1': 'ITEM_STATE_CLAIMED', '2': 2},
    {'1': 'ITEM_STATE_DONE', '2': 3},
  ],
};

/// Descriptor for `ItemState`. Decode as a `google.protobuf.EnumDescriptorProto`.
final $typed_data.Uint8List itemStateDescriptor = $convert.base64Decode(
    'CglJdGVtU3RhdGUSGgoWSVRFTV9TVEFURV9VTlNQRUNJRklFRBAAEhMKD0lURU1fU1RBVEVfVE'
    '9ETxABEhYKEklURU1fU1RBVEVfQ0xBSU1FRBACEhMKD0lURU1fU1RBVEVfRE9ORRAD');

@$core.Deprecated('Use claimDescriptor instead')
const Claim$json = {
  '1': 'Claim',
  '2': [
    {'1': 'by', '3': 1, '4': 1, '5': 9, '10': 'by'},
    {
      '1': 'at',
      '3': 2,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Timestamp',
      '10': 'at'
    },
  ],
};

/// Descriptor for `Claim`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List claimDescriptor = $convert.base64Decode(
    'CgVDbGFpbRIOCgJieRgBIAEoCVICYnkSKgoCYXQYAiABKAsyGi5nb29nbGUucHJvdG9idWYuVG'
    'ltZXN0YW1wUgJhdA==');

@$core.Deprecated('Use itemDescriptor instead')
const Item$json = {
  '1': 'Item',
  '2': [
    {'1': 'id', '3': 1, '4': 1, '5': 9, '10': 'id'},
    {'1': 'project', '3': 2, '4': 1, '5': 9, '10': 'project'},
    {'1': 'title', '3': 3, '4': 1, '5': 9, '10': 'title'},
    {'1': 'issue_url', '3': 4, '4': 1, '5': 9, '10': 'issueUrl'},
    {'1': 'priority', '3': 5, '4': 1, '5': 5, '10': 'priority'},
    {'1': 'harness', '3': 6, '4': 1, '5': 9, '10': 'harness'},
    {
      '1': 'state',
      '3': 7,
      '4': 1,
      '5': 14,
      '6': '.protectorate.v1.ItemState',
      '10': 'state'
    },
    {
      '1': 'claim',
      '3': 8,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.Claim',
      '10': 'claim'
    },
    {'1': 'notes', '3': 9, '4': 1, '5': 9, '10': 'notes'},
    {'1': 'donate', '3': 10, '4': 1, '5': 8, '10': 'donate'},
  ],
};

/// Descriptor for `Item`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List itemDescriptor = $convert.base64Decode(
    'CgRJdGVtEg4KAmlkGAEgASgJUgJpZBIYCgdwcm9qZWN0GAIgASgJUgdwcm9qZWN0EhQKBXRpdG'
    'xlGAMgASgJUgV0aXRsZRIbCglpc3N1ZV91cmwYBCABKAlSCGlzc3VlVXJsEhoKCHByaW9yaXR5'
    'GAUgASgFUghwcmlvcml0eRIYCgdoYXJuZXNzGAYgASgJUgdoYXJuZXNzEjAKBXN0YXRlGAcgAS'
    'gOMhoucHJvdGVjdG9yYXRlLnYxLkl0ZW1TdGF0ZVIFc3RhdGUSLAoFY2xhaW0YCCABKAsyFi5w'
    'cm90ZWN0b3JhdGUudjEuQ2xhaW1SBWNsYWltEhQKBW5vdGVzGAkgASgJUgVub3RlcxIWCgZkb2'
    '5hdGUYCiABKAhSBmRvbmF0ZQ==');

@$core.Deprecated('Use listItemsRequestDescriptor instead')
const ListItemsRequest$json = {
  '1': 'ListItemsRequest',
  '2': [
    {'1': 'project', '3': 1, '4': 1, '5': 9, '10': 'project'},
    {
      '1': 'state',
      '3': 2,
      '4': 1,
      '5': 14,
      '6': '.protectorate.v1.ItemState',
      '10': 'state'
    },
    {'1': 'harness', '3': 3, '4': 1, '5': 9, '10': 'harness'},
  ],
};

/// Descriptor for `ListItemsRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List listItemsRequestDescriptor = $convert.base64Decode(
    'ChBMaXN0SXRlbXNSZXF1ZXN0EhgKB3Byb2plY3QYASABKAlSB3Byb2plY3QSMAoFc3RhdGUYAi'
    'ABKA4yGi5wcm90ZWN0b3JhdGUudjEuSXRlbVN0YXRlUgVzdGF0ZRIYCgdoYXJuZXNzGAMgASgJ'
    'UgdoYXJuZXNz');

@$core.Deprecated('Use listItemsResponseDescriptor instead')
const ListItemsResponse$json = {
  '1': 'ListItemsResponse',
  '2': [
    {
      '1': 'items',
      '3': 1,
      '4': 3,
      '5': 11,
      '6': '.protectorate.v1.Item',
      '10': 'items'
    },
  ],
};

/// Descriptor for `ListItemsResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List listItemsResponseDescriptor = $convert.base64Decode(
    'ChFMaXN0SXRlbXNSZXNwb25zZRIrCgVpdGVtcxgBIAMoCzIVLnByb3RlY3RvcmF0ZS52MS5JdG'
    'VtUgVpdGVtcw==');

@$core.Deprecated('Use addItemRequestDescriptor instead')
const AddItemRequest$json = {
  '1': 'AddItemRequest',
  '2': [
    {'1': 'project', '3': 1, '4': 1, '5': 9, '10': 'project'},
    {'1': 'title', '3': 2, '4': 1, '5': 9, '10': 'title'},
    {'1': 'issue_url', '3': 3, '4': 1, '5': 9, '10': 'issueUrl'},
    {'1': 'priority', '3': 4, '4': 1, '5': 5, '10': 'priority'},
    {'1': 'harness', '3': 5, '4': 1, '5': 9, '10': 'harness'},
  ],
};

/// Descriptor for `AddItemRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List addItemRequestDescriptor = $convert.base64Decode(
    'Cg5BZGRJdGVtUmVxdWVzdBIYCgdwcm9qZWN0GAEgASgJUgdwcm9qZWN0EhQKBXRpdGxlGAIgAS'
    'gJUgV0aXRsZRIbCglpc3N1ZV91cmwYAyABKAlSCGlzc3VlVXJsEhoKCHByaW9yaXR5GAQgASgF'
    'Ughwcmlvcml0eRIYCgdoYXJuZXNzGAUgASgJUgdoYXJuZXNz');

@$core.Deprecated('Use addItemResponseDescriptor instead')
const AddItemResponse$json = {
  '1': 'AddItemResponse',
  '2': [
    {
      '1': 'item',
      '3': 1,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.Item',
      '10': 'item'
    },
  ],
};

/// Descriptor for `AddItemResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List addItemResponseDescriptor = $convert.base64Decode(
    'Cg9BZGRJdGVtUmVzcG9uc2USKQoEaXRlbRgBIAEoCzIVLnByb3RlY3RvcmF0ZS52MS5JdGVtUg'
    'RpdGVt');

@$core.Deprecated('Use updateItemRequestDescriptor instead')
const UpdateItemRequest$json = {
  '1': 'UpdateItemRequest',
  '2': [
    {'1': 'id', '3': 1, '4': 1, '5': 9, '10': 'id'},
    {
      '1': 'state',
      '3': 2,
      '4': 1,
      '5': 14,
      '6': '.protectorate.v1.ItemState',
      '10': 'state'
    },
    {
      '1': 'claim',
      '3': 3,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.Claim',
      '10': 'claim'
    },
  ],
};

/// Descriptor for `UpdateItemRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List updateItemRequestDescriptor = $convert.base64Decode(
    'ChFVcGRhdGVJdGVtUmVxdWVzdBIOCgJpZBgBIAEoCVICaWQSMAoFc3RhdGUYAiABKA4yGi5wcm'
    '90ZWN0b3JhdGUudjEuSXRlbVN0YXRlUgVzdGF0ZRIsCgVjbGFpbRgDIAEoCzIWLnByb3RlY3Rv'
    'cmF0ZS52MS5DbGFpbVIFY2xhaW0=');

@$core.Deprecated('Use updateItemResponseDescriptor instead')
const UpdateItemResponse$json = {
  '1': 'UpdateItemResponse',
  '2': [
    {
      '1': 'item',
      '3': 1,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.Item',
      '10': 'item'
    },
  ],
};

/// Descriptor for `UpdateItemResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List updateItemResponseDescriptor = $convert.base64Decode(
    'ChJVcGRhdGVJdGVtUmVzcG9uc2USKQoEaXRlbRgBIAEoCzIVLnByb3RlY3RvcmF0ZS52MS5JdG'
    'VtUgRpdGVt');

const $core.Map<$core.String, $core.dynamic> BacklogServiceBase$json = {
  '1': 'BacklogService',
  '2': [
    {
      '1': 'ListItems',
      '2': '.protectorate.v1.ListItemsRequest',
      '3': '.protectorate.v1.ListItemsResponse',
      '4': {'34': 1},
    },
    {
      '1': 'AddItem',
      '2': '.protectorate.v1.AddItemRequest',
      '3': '.protectorate.v1.AddItemResponse'
    },
    {
      '1': 'UpdateItem',
      '2': '.protectorate.v1.UpdateItemRequest',
      '3': '.protectorate.v1.UpdateItemResponse'
    },
  ],
};

@$core.Deprecated('Use backlogServiceDescriptor instead')
const $core.Map<$core.String, $core.Map<$core.String, $core.dynamic>>
    BacklogServiceBase$messageJson = {
  '.protectorate.v1.ListItemsRequest': ListItemsRequest$json,
  '.protectorate.v1.ListItemsResponse': ListItemsResponse$json,
  '.protectorate.v1.Item': Item$json,
  '.protectorate.v1.Claim': Claim$json,
  '.google.protobuf.Timestamp': $0.Timestamp$json,
  '.protectorate.v1.AddItemRequest': AddItemRequest$json,
  '.protectorate.v1.AddItemResponse': AddItemResponse$json,
  '.protectorate.v1.UpdateItemRequest': UpdateItemRequest$json,
  '.protectorate.v1.UpdateItemResponse': UpdateItemResponse$json,
};

/// Descriptor for `BacklogService`. Decode as a `google.protobuf.ServiceDescriptorProto`.
final $typed_data.Uint8List backlogServiceDescriptor = $convert.base64Decode(
    'Cg5CYWNrbG9nU2VydmljZRJXCglMaXN0SXRlbXMSIS5wcm90ZWN0b3JhdGUudjEuTGlzdEl0ZW'
    '1zUmVxdWVzdBoiLnByb3RlY3RvcmF0ZS52MS5MaXN0SXRlbXNSZXNwb25zZSIDkAIBEkwKB0Fk'
    'ZEl0ZW0SHy5wcm90ZWN0b3JhdGUudjEuQWRkSXRlbVJlcXVlc3QaIC5wcm90ZWN0b3JhdGUudj'
    'EuQWRkSXRlbVJlc3BvbnNlElUKClVwZGF0ZUl0ZW0SIi5wcm90ZWN0b3JhdGUudjEuVXBkYXRl'
    'SXRlbVJlcXVlc3QaIy5wcm90ZWN0b3JhdGUudjEuVXBkYXRlSXRlbVJlc3BvbnNl');
