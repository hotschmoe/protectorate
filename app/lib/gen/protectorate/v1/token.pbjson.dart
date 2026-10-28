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
    as $1;
import 'package:protobuf/well_known_types/google/protobuf/timestamp.pbjson.dart'
    as $0;

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
      '1': 'revoked_at',
      '3': 6,
      '4': 1,
      '5': 11,
      '6': '.google.protobuf.Timestamp',
      '10': 'revokedAt'
    },
    {'1': 'task', '3': 7, '4': 1, '5': 9, '10': 'task'},
    {'1': 'minted_by', '3': 8, '4': 1, '5': 9, '10': 'mintedBy'},
  ],
};

/// Descriptor for `TokenInfo`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List tokenInfoDescriptor = $convert.base64Decode(
    'CglUb2tlbkluZm8SDgoCaWQYASABKAlSAmlkEhIKBG5hbWUYAiABKAlSBG5hbWUSFgoGc2NvcG'
    'VzGAMgAygJUgZzY29wZXMSOQoKY3JlYXRlZF9hdBgEIAEoCzIaLmdvb2dsZS5wcm90b2J1Zi5U'
    'aW1lc3RhbXBSCWNyZWF0ZWRBdBI5CgpleHBpcmVzX2F0GAUgASgLMhouZ29vZ2xlLnByb3RvYn'
    'VmLlRpbWVzdGFtcFIJZXhwaXJlc0F0EjkKCnJldm9rZWRfYXQYBiABKAsyGi5nb29nbGUucHJv'
    'dG9idWYuVGltZXN0YW1wUglyZXZva2VkQXQSEgoEdGFzaxgHIAEoCVIEdGFzaxIbCgltaW50ZW'
    'RfYnkYCCABKAlSCG1pbnRlZEJ5');

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

@$core.Deprecated('Use renewTokenRequestDescriptor instead')
const RenewTokenRequest$json = {
  '1': 'RenewTokenRequest',
};

/// Descriptor for `RenewTokenRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List renewTokenRequestDescriptor =
    $convert.base64Decode('ChFSZW5ld1Rva2VuUmVxdWVzdA==');

@$core.Deprecated('Use renewTokenResponseDescriptor instead')
const RenewTokenResponse$json = {
  '1': 'RenewTokenResponse',
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

/// Descriptor for `RenewTokenResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List renewTokenResponseDescriptor = $convert.base64Decode(
    'ChJSZW5ld1Rva2VuUmVzcG9uc2USMAoFdG9rZW4YASABKAsyGi5wcm90ZWN0b3JhdGUudjEuVG'
    '9rZW5JbmZvUgV0b2tlbhIWCgZzZWNyZXQYAiABKAlSBnNlY3JldA==');

const $core.Map<$core.String, $core.dynamic> TokenServiceBase$json = {
  '1': 'TokenService',
  '2': [
    {
      '1': 'CreateToken',
      '2': '.protectorate.v1.CreateTokenRequest',
      '3': '.protectorate.v1.CreateTokenResponse',
      '4': {}
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
    {
      '1': 'RenewToken',
      '2': '.protectorate.v1.RenewTokenRequest',
      '3': '.protectorate.v1.RenewTokenResponse'
    },
  ],
};

@$core.Deprecated('Use tokenServiceDescriptor instead')
const $core.Map<$core.String, $core.Map<$core.String, $core.dynamic>>
    TokenServiceBase$messageJson = {
  '.protectorate.v1.CreateTokenRequest': CreateTokenRequest$json,
  '.google.protobuf.Duration': $1.Duration$json,
  '.protectorate.v1.CreateTokenResponse': CreateTokenResponse$json,
  '.protectorate.v1.TokenInfo': TokenInfo$json,
  '.google.protobuf.Timestamp': $0.Timestamp$json,
  '.protectorate.v1.ListTokensRequest': ListTokensRequest$json,
  '.protectorate.v1.ListTokensResponse': ListTokensResponse$json,
  '.protectorate.v1.RevokeTokenRequest': RevokeTokenRequest$json,
  '.protectorate.v1.RevokeTokenResponse': RevokeTokenResponse$json,
  '.protectorate.v1.RenewTokenRequest': RenewTokenRequest$json,
  '.protectorate.v1.RenewTokenResponse': RenewTokenResponse$json,
};

/// Descriptor for `TokenService`. Decode as a `google.protobuf.ServiceDescriptorProto`.
final $typed_data.Uint8List tokenServiceDescriptor = $convert.base64Decode(
    'CgxUb2tlblNlcnZpY2USXgoLQ3JlYXRlVG9rZW4SIy5wcm90ZWN0b3JhdGUudjEuQ3JlYXRlVG'
    '9rZW5SZXF1ZXN0GiQucHJvdGVjdG9yYXRlLnYxLkNyZWF0ZVRva2VuUmVzcG9uc2UiBKC1GAES'
    'XgoKTGlzdFRva2VucxIiLnByb3RlY3RvcmF0ZS52MS5MaXN0VG9rZW5zUmVxdWVzdBojLnByb3'
    'RlY3RvcmF0ZS52MS5MaXN0VG9rZW5zUmVzcG9uc2UiB5ACAaC1GAESYgoLUmV2b2tlVG9rZW4S'
    'Iy5wcm90ZWN0b3JhdGUudjEuUmV2b2tlVG9rZW5SZXF1ZXN0GiQucHJvdGVjdG9yYXRlLnYxLl'
    'Jldm9rZVRva2VuUmVzcG9uc2UiCIi1GAGgtRgBElUKClJlbmV3VG9rZW4SIi5wcm90ZWN0b3Jh'
    'dGUudjEuUmVuZXdUb2tlblJlcXVlc3QaIy5wcm90ZWN0b3JhdGUudjEuUmVuZXdUb2tlblJlc3'
    'BvbnNl');
