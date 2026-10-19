// This is a generated file - do not edit.
//
// Generated from protectorate/v1/backlog.proto.

// @dart = 3.3

// ignore_for_file: annotate_overrides, camel_case_types, comment_references
// ignore_for_file: constant_identifier_names
// ignore_for_file: curly_braces_in_flow_control_structures
// ignore_for_file: deprecated_member_use_from_same_package, library_prefixes
// ignore_for_file: non_constant_identifier_names, prefer_relative_imports

import 'dart:async' as $async;
import 'dart:core' as $core;

import 'package:protobuf/protobuf.dart' as $pb;

import 'backlog.pb.dart' as $1;
import 'backlog.pbjson.dart';

export 'backlog.pb.dart';

abstract class BacklogServiceBase extends $pb.GeneratedService {
  $async.Future<$1.ListNotesResponse> listNotes(
      $pb.ServerContext ctx, $1.ListNotesRequest request);
  $async.Future<$1.GetNoteResponse> getNote(
      $pb.ServerContext ctx, $1.GetNoteRequest request);
  $async.Future<$1.CreateNoteResponse> createNote(
      $pb.ServerContext ctx, $1.CreateNoteRequest request);
  $async.Future<$1.UpdateNoteResponse> updateNote(
      $pb.ServerContext ctx, $1.UpdateNoteRequest request);
  $async.Future<$1.DeleteNoteResponse> deleteNote(
      $pb.ServerContext ctx, $1.DeleteNoteRequest request);

  $pb.GeneratedMessage createRequest($core.String methodName) {
    switch (methodName) {
      case 'ListNotes':
        return $1.ListNotesRequest();
      case 'GetNote':
        return $1.GetNoteRequest();
      case 'CreateNote':
        return $1.CreateNoteRequest();
      case 'UpdateNote':
        return $1.UpdateNoteRequest();
      case 'DeleteNote':
        return $1.DeleteNoteRequest();
      default:
        throw $core.ArgumentError('Unknown method: $methodName');
    }
  }

  $async.Future<$pb.GeneratedMessage> handleCall($pb.ServerContext ctx,
      $core.String methodName, $pb.GeneratedMessage request) {
    switch (methodName) {
      case 'ListNotes':
        return listNotes(ctx, request as $1.ListNotesRequest);
      case 'GetNote':
        return getNote(ctx, request as $1.GetNoteRequest);
      case 'CreateNote':
        return createNote(ctx, request as $1.CreateNoteRequest);
      case 'UpdateNote':
        return updateNote(ctx, request as $1.UpdateNoteRequest);
      case 'DeleteNote':
        return deleteNote(ctx, request as $1.DeleteNoteRequest);
      default:
        throw $core.ArgumentError('Unknown method: $methodName');
    }
  }

  $core.Map<$core.String, $core.dynamic> get $json => BacklogServiceBase$json;
  $core.Map<$core.String, $core.Map<$core.String, $core.dynamic>>
      get $messageJson => BacklogServiceBase$messageJson;
}
