// This is a generated file - do not edit.
//
// Generated from protectorate/v1/directory.proto.

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

@$core.Deprecated('Use listProjectsRequestDescriptor instead')
const ListProjectsRequest$json = {
  '1': 'ListProjectsRequest',
  '2': [
    {'1': 'sleeve', '3': 1, '4': 1, '5': 9, '10': 'sleeve'},
  ],
};

/// Descriptor for `ListProjectsRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List listProjectsRequestDescriptor =
    $convert.base64Decode(
        'ChNMaXN0UHJvamVjdHNSZXF1ZXN0EhYKBnNsZWV2ZRgBIAEoCVIGc2xlZXZl');

@$core.Deprecated('Use listProjectsResponseDescriptor instead')
const ListProjectsResponse$json = {
  '1': 'ListProjectsResponse',
  '2': [
    {
      '1': 'projects',
      '3': 1,
      '4': 3,
      '5': 11,
      '6': '.protectorate.v1.DirectoryProject',
      '10': 'projects'
    },
  ],
};

/// Descriptor for `ListProjectsResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List listProjectsResponseDescriptor = $convert.base64Decode(
    'ChRMaXN0UHJvamVjdHNSZXNwb25zZRI9Cghwcm9qZWN0cxgBIAMoCzIhLnByb3RlY3RvcmF0ZS'
    '52MS5EaXJlY3RvcnlQcm9qZWN0Ughwcm9qZWN0cw==');

@$core.Deprecated('Use getProjectRequestDescriptor instead')
const GetProjectRequest$json = {
  '1': 'GetProjectRequest',
  '2': [
    {'1': 'project', '3': 1, '4': 1, '5': 9, '10': 'project'},
  ],
};

/// Descriptor for `GetProjectRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List getProjectRequestDescriptor = $convert.base64Decode(
    'ChFHZXRQcm9qZWN0UmVxdWVzdBIYCgdwcm9qZWN0GAEgASgJUgdwcm9qZWN0');

@$core.Deprecated('Use getProjectResponseDescriptor instead')
const GetProjectResponse$json = {
  '1': 'GetProjectResponse',
  '2': [
    {
      '1': 'project',
      '3': 1,
      '4': 1,
      '5': 11,
      '6': '.protectorate.v1.DirectoryProject',
      '10': 'project'
    },
  ],
};

/// Descriptor for `GetProjectResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List getProjectResponseDescriptor = $convert.base64Decode(
    'ChJHZXRQcm9qZWN0UmVzcG9uc2USOwoHcHJvamVjdBgBIAEoCzIhLnByb3RlY3RvcmF0ZS52MS'
    '5EaXJlY3RvcnlQcm9qZWN0Ugdwcm9qZWN0');

@$core.Deprecated('Use directoryProjectDescriptor instead')
const DirectoryProject$json = {
  '1': 'DirectoryProject',
  '2': [
    {'1': 'id', '3': 1, '4': 1, '5': 9, '10': 'id'},
    {'1': 'name', '3': 2, '4': 1, '5': 9, '10': 'name'},
    {'1': 'display', '3': 3, '4': 1, '5': 9, '10': 'display'},
    {'1': 'remote', '3': 4, '4': 1, '5': 9, '10': 'remote'},
    {
      '1': 'owners',
      '3': 5,
      '4': 3,
      '5': 11,
      '6': '.protectorate.v1.ProjectOwner',
      '10': 'owners'
    },
    {'1': 'provides', '3': 6, '4': 3, '5': 9, '10': 'provides'},
    {'1': 'dependency_count', '3': 7, '4': 1, '5': 13, '10': 'dependencyCount'},
    {
      '1': 'depends_on',
      '3': 8,
      '4': 3,
      '5': 11,
      '6': '.protectorate.v1.ProjectEdge',
      '10': 'dependsOn'
    },
    {
      '1': 'dependents',
      '3': 9,
      '4': 3,
      '5': 11,
      '6': '.protectorate.v1.ProjectEdge',
      '10': 'dependents'
    },
    {'1': 'default_branch', '3': 10, '4': 1, '5': 9, '10': 'defaultBranch'},
    {'1': 'latest_tag', '3': 11, '4': 1, '5': 9, '10': 'latestTag'},
  ],
};

/// Descriptor for `DirectoryProject`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List directoryProjectDescriptor = $convert.base64Decode(
    'ChBEaXJlY3RvcnlQcm9qZWN0Eg4KAmlkGAEgASgJUgJpZBISCgRuYW1lGAIgASgJUgRuYW1lEh'
    'gKB2Rpc3BsYXkYAyABKAlSB2Rpc3BsYXkSFgoGcmVtb3RlGAQgASgJUgZyZW1vdGUSNQoGb3du'
    'ZXJzGAUgAygLMh0ucHJvdGVjdG9yYXRlLnYxLlByb2plY3RPd25lclIGb3duZXJzEhoKCHByb3'
    'ZpZGVzGAYgAygJUghwcm92aWRlcxIpChBkZXBlbmRlbmN5X2NvdW50GAcgASgNUg9kZXBlbmRl'
    'bmN5Q291bnQSOwoKZGVwZW5kc19vbhgIIAMoCzIcLnByb3RlY3RvcmF0ZS52MS5Qcm9qZWN0RW'
    'RnZVIJZGVwZW5kc09uEjwKCmRlcGVuZGVudHMYCSADKAsyHC5wcm90ZWN0b3JhdGUudjEuUHJv'
    'amVjdEVkZ2VSCmRlcGVuZGVudHMSJQoOZGVmYXVsdF9icmFuY2gYCiABKAlSDWRlZmF1bHRCcm'
    'FuY2gSHQoKbGF0ZXN0X3RhZxgLIAEoCVIJbGF0ZXN0VGFn');

@$core.Deprecated('Use projectOwnerDescriptor instead')
const ProjectOwner$json = {
  '1': 'ProjectOwner',
  '2': [
    {'1': 'sleeve', '3': 1, '4': 1, '5': 9, '10': 'sleeve'},
    {'1': 'workspace', '3': 2, '4': 1, '5': 9, '10': 'workspace'},
    {'1': 'branch', '3': 3, '4': 1, '5': 9, '10': 'branch'},
    {'1': 'head', '3': 4, '4': 1, '5': 9, '10': 'head'},
  ],
};

/// Descriptor for `ProjectOwner`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List projectOwnerDescriptor = $convert.base64Decode(
    'CgxQcm9qZWN0T3duZXISFgoGc2xlZXZlGAEgASgJUgZzbGVldmUSHAoJd29ya3NwYWNlGAIgAS'
    'gJUgl3b3Jrc3BhY2USFgoGYnJhbmNoGAMgASgJUgZicmFuY2gSEgoEaGVhZBgEIAEoCVIEaGVh'
    'ZA==');

@$core.Deprecated('Use projectEdgeDescriptor instead')
const ProjectEdge$json = {
  '1': 'ProjectEdge',
  '2': [
    {'1': 'project', '3': 1, '4': 1, '5': 9, '10': 'project'},
    {'1': 'display', '3': 2, '4': 1, '5': 9, '10': 'display'},
    {'1': 'via', '3': 3, '4': 3, '5': 9, '10': 'via'},
  ],
};

/// Descriptor for `ProjectEdge`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List projectEdgeDescriptor = $convert.base64Decode(
    'CgtQcm9qZWN0RWRnZRIYCgdwcm9qZWN0GAEgASgJUgdwcm9qZWN0EhgKB2Rpc3BsYXkYAiABKA'
    'lSB2Rpc3BsYXkSEAoDdmlhGAMgAygJUgN2aWE=');

const $core.Map<$core.String, $core.dynamic> DirectoryServiceBase$json = {
  '1': 'DirectoryService',
  '2': [
    {
      '1': 'ListProjects',
      '2': '.protectorate.v1.ListProjectsRequest',
      '3': '.protectorate.v1.ListProjectsResponse',
      '4': {'34': 1},
    },
    {
      '1': 'GetProject',
      '2': '.protectorate.v1.GetProjectRequest',
      '3': '.protectorate.v1.GetProjectResponse',
      '4': {'34': 1},
    },
  ],
};

@$core.Deprecated('Use directoryServiceDescriptor instead')
const $core.Map<$core.String, $core.Map<$core.String, $core.dynamic>>
    DirectoryServiceBase$messageJson = {
  '.protectorate.v1.ListProjectsRequest': ListProjectsRequest$json,
  '.protectorate.v1.ListProjectsResponse': ListProjectsResponse$json,
  '.protectorate.v1.DirectoryProject': DirectoryProject$json,
  '.protectorate.v1.ProjectOwner': ProjectOwner$json,
  '.protectorate.v1.ProjectEdge': ProjectEdge$json,
  '.protectorate.v1.GetProjectRequest': GetProjectRequest$json,
  '.protectorate.v1.GetProjectResponse': GetProjectResponse$json,
};

/// Descriptor for `DirectoryService`. Decode as a `google.protobuf.ServiceDescriptorProto`.
final $typed_data.Uint8List directoryServiceDescriptor = $convert.base64Decode(
    'ChBEaXJlY3RvcnlTZXJ2aWNlEmAKDExpc3RQcm9qZWN0cxIkLnByb3RlY3RvcmF0ZS52MS5MaX'
    'N0UHJvamVjdHNSZXF1ZXN0GiUucHJvdGVjdG9yYXRlLnYxLkxpc3RQcm9qZWN0c1Jlc3BvbnNl'
    'IgOQAgESWgoKR2V0UHJvamVjdBIiLnByb3RlY3RvcmF0ZS52MS5HZXRQcm9qZWN0UmVxdWVzdB'
    'ojLnByb3RlY3RvcmF0ZS52MS5HZXRQcm9qZWN0UmVzcG9uc2UiA5ACAQ==');
