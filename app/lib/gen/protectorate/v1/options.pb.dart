// This is a generated file - do not edit.
//
// Generated from protectorate/v1/options.proto.

// @dart = 3.3

// ignore_for_file: annotate_overrides, camel_case_types, comment_references
// ignore_for_file: constant_identifier_names
// ignore_for_file: curly_braces_in_flow_control_structures
// ignore_for_file: deprecated_member_use_from_same_package, library_prefixes
// ignore_for_file: non_constant_identifier_names, prefer_relative_imports

import 'dart:core' as $core;

import 'package:protobuf/protobuf.dart' as $pb;

export 'package:protobuf/protobuf.dart' show GeneratedMessageGenericExtensions;

class Options {
  static final destructive = $pb.Extension<$core.bool>(
      _omitMessageNames ? '' : 'google.protobuf.MethodOptions',
      _omitFieldNames ? '' : 'destructive',
      50001,
      $pb.PbFieldType.OB);
  static final ownerOnlyRpc = $pb.Extension<$core.bool>(
      _omitMessageNames ? '' : 'google.protobuf.MethodOptions',
      _omitFieldNames ? '' : 'ownerOnlyRpc',
      50004,
      $pb.PbFieldType.OB);
  static final nodeLocal = $pb.Extension<$core.bool>(
      _omitMessageNames ? '' : 'google.protobuf.ServiceOptions',
      _omitFieldNames ? '' : 'nodeLocal',
      50002,
      $pb.PbFieldType.OB);
  static final ownerOnly = $pb.Extension<$core.bool>(
      _omitMessageNames ? '' : 'google.protobuf.ServiceOptions',
      _omitFieldNames ? '' : 'ownerOnly',
      50003,
      $pb.PbFieldType.OB);
  static void registerAllExtensions($pb.ExtensionRegistry registry) {
    registry.add(destructive);
    registry.add(ownerOnlyRpc);
    registry.add(nodeLocal);
    registry.add(ownerOnly);
  }
}

const $core.bool _omitFieldNames =
    $core.bool.fromEnvironment('protobuf.omit_field_names');
const $core.bool _omitMessageNames =
    $core.bool.fromEnvironment('protobuf.omit_message_names');
