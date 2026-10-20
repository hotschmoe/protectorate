// This is a generated file - do not edit.
//
// Generated from protectorate/v1/token.proto.

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

import 'package:protobuf/well_known_types/google/protobuf/duration.pbjson.dart'
    as $0;
import 'package:protobuf/well_known_types/google/protobuf/timestamp.pbjson.dart'
    as $1;

@$core.Deprecated('Use grantServiceAdminRequestDescriptor instead')
const GrantServiceAdminRequest$json = {
  '1': 'GrantServiceAdminRequest',
  '2': [
    {'1': 'node', '3': 1, '4': 1, '5': 9, '10': 'node'},
    {
      '1': 'ttl',
      '3': 2,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Duration',
      '10': 'ttl'
    },
    {'1': 'task', '3': 3, '4': 1, '5': 9, '10': 'task'},
  ],
};

/// Descriptor for `GrantServiceAdminRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List grantServiceAdminRequestDescriptor = $convert.base64Decode(
    'ChhHcmFudFNlcnZpY2VBZG1pblJlcXVlc3QSEgoEbm9kZRgBIAEoCVIEbm9kZRIrCgN0dGwYAi'
    'ABKAsyGS5nb29nbGUucHJvdG9idWYuRHVyYXRpb25SA3R0bBISCgR0YXNrGAMgASgJUgR0YXNr');

@$core.Deprecated('Use grantServiceAdminResponseDescriptor instead')
const GrantServiceAdminResponse$json = {
  '1': 'GrantServiceAdminResponse',
  '2': [
    {
      '1': 'token',
      '3': 1,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.TokenInfo',
      '10': 'token'
    },
    {'1': 'secret', '3': 2, '4': 1, '5': 9, '10': 'secret'},
  ],
};

/// Descriptor for `GrantServiceAdminResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List grantServiceAdminResponseDescriptor =
    $convert.base64Decode(
        'ChlHcmFudFNlcnZpY2VBZG1pblJlc3BvbnNlEjAKBXRva2VuGAEgASgLMhoucHJvdGVjdG9yYX'
        'RlLnYxLlRva2VuSW5mb1IFdG9rZW4SFgoGc2VjcmV0GAIgASgJUgZzZWNyZXQ=');

@$core.Deprecated('Use tokenInfoDescriptor instead')
const TokenInfo$json = {
  '1': 'TokenInfo',
  '2': [
    {'1': 'id', '3': 1, '4': 1, '5': 9, '10': 'id'},
    {'1': 'name', '3': 2, '4': 1, '5': 9, '10': 'name'},
    {'1': 'scopes', '3': 3, '4': 3, '5': 9, '10': 'scopes'},
    {
      '1': 'created_at',
      '3': 4,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Timestamp',
      '10': 'createdAt'
    },
    {
      '1': 'expires_at',
      '3': 5,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Timestamp',
      '10': 'expiresAt'
    },
    {
      '1': 'last_used_at',
      '3': 6,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Timestamp',
      '10': 'lastUsedAt'
    },
    {
      '1': 'revoked_at',
      '3': 7,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Timestamp',
      '10': 'revokedAt'
    },
    {'1': 'task', '3': 8, '4': 1, '5': 9, '10': 'task'},
    {'1': 'minted_by', '3': 9, '4': 1, '5': 9, '10': 'mintedBy'},
  ],
};

/// Descriptor for `TokenInfo`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List tokenInfoDescriptor = $convert.base64Decode(
    'CglUb2tlbkluZm8SDgoCaWQYASABKAlSAmlkEhIKBG5hbWUYAiABKAlSBG5hbWUSFgoGc2NvcG'
    'VzGAMgAygJUgZzY29wZXMSOQoKY3JlYXRlZF9hdBgEIAEoCzIaLmdvb2dsZS5wcm90b2J1Zi5U'
    'aW1lc3RhbXBSCWNyZWF0ZWRBdBI5CgpleHBpcmVzX2F0GAUgASgLMhouZ29vZ2xlLnByb3RvYn'
    'VmLlRpbWVzdGFtcFIJZXhwaXJlc0F0EjwKDGxhc3RfdXNlZF9hdBgGIAEoCzIaLmdvb2dsZS5w'
    'cm90b2J1Zi5UaW1lc3RhbXBSCmxhc3RVc2VkQXQSOQoKcmV2b2tlZF9hdBgHIAEoCzIaLmdvb2'
    'dsZS5wcm90b2J1Zi5UaW1lc3RhbXBSCXJldm9rZWRBdBISCgR0YXNrGAggASgJUgR0YXNrEhsK'
    'CW1pbnRlZF9ieRgJIAEoCVIIbWludGVkQnk=');

@$core.Deprecated('Use createTokenRequestDescriptor instead')
const CreateTokenRequest$json = {
  '1': 'CreateTokenRequest',
  '2': [
    {'1': 'name', '3': 1, '4': 1, '5': 9, '10': 'name'},
    {'1': 'scopes', '3': 2, '4': 3, '5': 9, '10': 'scopes'},
    {
      '1': 'ttl',
      '3': 3,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Duration',
      '10': 'ttl'
    },
  ],
};

/// Descriptor for `CreateTokenRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List createTokenRequestDescriptor = $convert.base64Decode(
    'ChJDcmVhdGVUb2tlblJlcXVlc3QSEgoEbmFtZRgBIAEoCVIEbmFtZRIWCgZzY29wZXMYAiADKA'
    'lSBnNjb3BlcxIrCgN0dGwYAyABKAsyGS5nb29nbGUucHJvdG9idWYuRHVyYXRpb25SA3R0bA==');

@$core.Deprecated('Use createTokenResponseDescriptor instead')
const CreateTokenResponse$json = {
  '1': 'CreateTokenResponse',
  '2': [
    {
      '1': 'token',
      '3': 1,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.TokenInfo',
      '10': 'token'
    },
    {'1': 'secret', '3': 2, '4': 1, '5': 9, '10': 'secret'},
  ],
};

/// Descriptor for `CreateTokenResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List createTokenResponseDescriptor = $convert.base64Decode(
    'ChNDcmVhdGVUb2tlblJlc3BvbnNlEjAKBXRva2VuGAEgASgLMhoucHJvdGVjdG9yYXRlLnYxLl'
    'Rva2VuSW5mb1IFdG9rZW4SFgoGc2VjcmV0GAIgASgJUgZzZWNyZXQ=');

@$core.Deprecated('Use listTokensRequestDescriptor instead')
const ListTokensRequest$json = {
  '1': 'ListTokensRequest',
};

/// Descriptor for `ListTokensRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List listTokensRequestDescriptor =
    $convert.base64Decode('ChFMaXN0VG9rZW5zUmVxdWVzdA==');

@$core.Deprecated('Use listTokensResponseDescriptor instead')
const ListTokensResponse$json = {
  '1': 'ListTokensResponse',
  '2': [
    {
      '1': 'tokens',
      '3': 1,
      '4': 3,
      '5': 11,
      '6': '.protectorate.v1.TokenInfo',
      '10': 'tokens'
    },
  ],
};

/// Descriptor for `ListTokensResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List listTokensResponseDescriptor = $convert.base64Decode(
    'ChJMaXN0VG9rZW5zUmVzcG9uc2USMgoGdG9rZW5zGAEgAygLMhoucHJvdGVjdG9yYXRlLnYxLl'
    'Rva2VuSW5mb1IGdG9rZW5z');

@$core.Deprecated('Use revokeTokenRequestDescriptor instead')
const RevokeTokenRequest$json = {
  '1': 'RevokeTokenRequest',
  '2': [
    {'1': 'id', '3': 1, '4': 1, '5': 9, '10': 'id'},
  ],
};

/// Descriptor for `RevokeTokenRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List revokeTokenRequestDescriptor =
    $convert.base64Decode('ChJSZXZva2VUb2tlblJlcXVlc3QSDgoCaWQYASABKAlSAmlk');

@$core.Deprecated('Use revokeTokenResponseDescriptor instead')
const RevokeTokenResponse$json = {
  '1': 'RevokeTokenResponse',
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

/// Descriptor for `RevokeTokenResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List revokeTokenResponseDescriptor = $convert.base64Decode(
    'ChNSZXZva2VUb2tlblJlc3BvbnNlEjAKBXRva2VuGAEgASgLMhoucHJvdGVjdG9yYXRlLnYxLl'
    'Rva2VuSW5mb1IFdG9rZW4=');

@$core.Deprecated('Use updateTokenScopesRequestDescriptor instead')
const UpdateTokenScopesRequest$json = {
  '1': 'UpdateTokenScopesRequest',
  '2': [
    {'1': 'id', '3': 1, '4': 1, '5': 9, '10': 'id'},
    {'1': 'add', '3': 2, '4': 3, '5': 9, '10': 'add'},
    {'1': 'remove', '3': 3, '4': 3, '5': 9, '10': 'remove'},
  ],
};

/// Descriptor for `UpdateTokenScopesRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List updateTokenScopesRequestDescriptor =
    $convert.base64Decode(
        'ChhVcGRhdGVUb2tlblNjb3Blc1JlcXVlc3QSDgoCaWQYASABKAlSAmlkEhAKA2FkZBgCIAMoCV'
        'IDYWRkEhYKBnJlbW92ZRgDIAMoCVIGcmVtb3Zl');

@$core.Deprecated('Use updateTokenScopesResponseDescriptor instead')
const UpdateTokenScopesResponse$json = {
  '1': 'UpdateTokenScopesResponse',
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

/// Descriptor for `UpdateTokenScopesResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List updateTokenScopesResponseDescriptor =
    $convert.base64Decode(
        'ChlVcGRhdGVUb2tlblNjb3Blc1Jlc3BvbnNlEjAKBXRva2VuGAEgASgLMhoucHJvdGVjdG9yYX'
        'RlLnYxLlRva2VuSW5mb1IFdG9rZW4=');

const $core.Map<$core.String, $core.dynamic> TokenServiceBase$json = {
  '1': 'TokenService',
  '2': [
    {
      '1': 'CreateToken',
      '2': '.protectorate.v1.CreateTokenRequest',
      '3': '.protectorate.v1.CreateTokenResponse'
    },
    {
      '1': 'UpdateTokenScopes',
      '2': '.protectorate.v1.UpdateTokenScopesRequest',
      '3': '.protectorate.v1.UpdateTokenScopesResponse'
    },
    {
      '1': 'ListTokens',
      '2': '.protectorate.v1.ListTokensRequest',
      '3': '.protectorate.v1.ListTokensResponse',
      '4': {'34': 1},
    },
    {
      '1': 'RevokeToken',
      '2': '.protectorate.v1.RevokeTokenRequest',
      '3': '.protectorate.v1.RevokeTokenResponse',
      '4': {}
    },
  ],
  '3': {},
};

@$core.Deprecated('Use tokenServiceDescriptor instead')
const $core.Map<$core.String, $core.Map<$core.String, $core.dynamic>>
    TokenServiceBase$messageJson = {
  '.protectorate.v1.CreateTokenRequest': CreateTokenRequest$json,
  '.google.protobuf.Duration': $0.Duration$json,
  '.protectorate.v1.CreateTokenResponse': CreateTokenResponse$json,
  '.protectorate.v1.TokenInfo': TokenInfo$json,
  '.google.protobuf.Timestamp': $1.Timestamp$json,
  '.protectorate.v1.UpdateTokenScopesRequest': UpdateTokenScopesRequest$json,
  '.protectorate.v1.UpdateTokenScopesResponse': UpdateTokenScopesResponse$json,
  '.protectorate.v1.ListTokensRequest': ListTokensRequest$json,
  '.protectorate.v1.ListTokensResponse': ListTokensResponse$json,
  '.protectorate.v1.RevokeTokenRequest': RevokeTokenRequest$json,
  '.protectorate.v1.RevokeTokenResponse': RevokeTokenResponse$json,
};

/// Descriptor for `TokenService`. Decode as a `google.protobuf.ServiceDescriptorProto`.
final $typed_data.Uint8List tokenServiceDescriptor = $convert.base64Decode(
    'CgxUb2tlblNlcnZpY2USWAoLQ3JlYXRlVG9rZW4SIy5wcm90ZWN0b3JhdGUudjEuQ3JlYXRlVG'
    '9rZW5SZXF1ZXN0GiQucHJvdGVjdG9yYXRlLnYxLkNyZWF0ZVRva2VuUmVzcG9uc2USagoRVXBk'
    'YXRlVG9rZW5TY29wZXMSKS5wcm90ZWN0b3JhdGUudjEuVXBkYXRlVG9rZW5TY29wZXNSZXF1ZX'
    'N0GioucHJvdGVjdG9yYXRlLnYxLlVwZGF0ZVRva2VuU2NvcGVzUmVzcG9uc2USWgoKTGlzdFRv'
    'a2VucxIiLnByb3RlY3RvcmF0ZS52MS5MaXN0VG9rZW5zUmVxdWVzdBojLnByb3RlY3RvcmF0ZS'
    '52MS5MaXN0VG9rZW5zUmVzcG9uc2UiA5ACARJeCgtSZXZva2VUb2tlbhIjLnByb3RlY3RvcmF0'
    'ZS52MS5SZXZva2VUb2tlblJlcXVlc3QaJC5wcm90ZWN0b3JhdGUudjEuUmV2b2tlVG9rZW5SZX'
    'Nwb25zZSIEiLUYARoEmLUYAQ==');

const $core.Map<$core.String, $core.dynamic> GrantServiceBase$json = {
  '1': 'GrantService',
  '2': [
    {
      '1': 'Admin',
      '2': '.protectorate.v1.GrantServiceAdminRequest',
      '3': '.protectorate.v1.GrantServiceAdminResponse'
    },
  ],
};

@$core.Deprecated('Use grantServiceDescriptor instead')
const $core.Map<$core.String, $core.Map<$core.String, $core.dynamic>>
    GrantServiceBase$messageJson = {
  '.protectorate.v1.GrantServiceAdminRequest': GrantServiceAdminRequest$json,
  '.google.protobuf.Duration': $0.Duration$json,
  '.protectorate.v1.GrantServiceAdminResponse': GrantServiceAdminResponse$json,
  '.protectorate.v1.TokenInfo': TokenInfo$json,
  '.google.protobuf.Timestamp': $1.Timestamp$json,
};

/// Descriptor for `GrantService`. Decode as a `google.protobuf.ServiceDescriptorProto`.
final $typed_data.Uint8List grantServiceDescriptor = $convert.base64Decode(
    'CgxHcmFudFNlcnZpY2USXgoFQWRtaW4SKS5wcm90ZWN0b3JhdGUudjEuR3JhbnRTZXJ2aWNlQW'
    'RtaW5SZXF1ZXN0GioucHJvdGVjdG9yYXRlLnYxLkdyYW50U2VydmljZUFkbWluUmVzcG9uc2U=');
