// This is a generated file - do not edit.
//
// Generated from protectorate/v1/backlog.proto.

// @dart = 3.3

// ignore_for_file: annotate_overrides, camel_case_types, comment_references
// ignore_for_file: constant_identifier_names
// ignore_for_file: curly_braces_in_flow_control_structures
// ignore_for_file: deprecated_member_use_from_same_package, library_prefixes
// ignore_for_file: non_constant_identifier_names, prefer_relative_imports

import 'dart:core' as $core;

import 'package:protobuf/protobuf.dart' as $pb;

class ItemState extends $pb.ProtobufEnum {
  static const ItemState ITEM_STATE_UNSPECIFIED =
      ItemState._(0, _omitEnumNames ? '' : 'ITEM_STATE_UNSPECIFIED');
  static const ItemState ITEM_STATE_TODO =
      ItemState._(1, _omitEnumNames ? '' : 'ITEM_STATE_TODO');
  static const ItemState ITEM_STATE_CLAIMED =
      ItemState._(2, _omitEnumNames ? '' : 'ITEM_STATE_CLAIMED');
  static const ItemState ITEM_STATE_DONE =
      ItemState._(3, _omitEnumNames ? '' : 'ITEM_STATE_DONE');

  static const $core.List<ItemState> values = <ItemState>[
    ITEM_STATE_UNSPECIFIED,
    ITEM_STATE_TODO,
    ITEM_STATE_CLAIMED,
    ITEM_STATE_DONE,
  ];

  static final $core.List<ItemState?> _byValue =
      $pb.ProtobufEnum.$_initByValueList(values, 3);
  static ItemState? valueOf($core.int value) =>
      value < 0 || value >= _byValue.length ? null : _byValue[value];

  const ItemState._(super.value, super.name);
}

const $core.bool _omitEnumNames =
    $core.bool.fromEnvironment('protobuf.omit_enum_names');
