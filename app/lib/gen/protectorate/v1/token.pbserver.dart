// This is a generated file - do not edit.
//
// Generated from protectorate/v1/token.proto.

// @dart = 3.3

// ignore_for_file: annotate_overrides, camel_case_types, comment_references
// ignore_for_file: constant_identifier_names
// ignore_for_file: curly_braces_in_flow_control_structures
// ignore_for_file: deprecated_member_use_from_same_package, library_prefixes
// ignore_for_file: non_constant_identifier_names, prefer_relative_imports

import 'dart:async' as $async;
import 'dart:core' as $core;

import 'package:protobuf/protobuf.dart' as $pb;

import 'token.pb.dart' as $2;
import 'token.pbjson.dart';

export 'token.pb.dart';

abstract class TokenServiceBase extends $pb.GeneratedService {
  $async.Future<$2.CreateTokenResponse> createToken(
      $pb.ServerContext ctx, $2.CreateTokenRequest request);
  $async.Future<$2.ListTokensResponse> listTokens(
      $pb.ServerContext ctx, $2.ListTokensRequest request);
  $async.Future<$2.RevokeTokenResponse> revokeToken(
      $pb.ServerContext ctx, $2.RevokeTokenRequest request);
  $async.Future<$2.RenewTokenResponse> renewToken(
      $pb.ServerContext ctx, $2.RenewTokenRequest request);

  $pb.GeneratedMessage createRequest($core.String methodName) {
    switch (methodName) {
      case 'CreateToken':
        return $2.CreateTokenRequest();
      case 'ListTokens':
        return $2.ListTokensRequest();
      case 'RevokeToken':
        return $2.RevokeTokenRequest();
      case 'RenewToken':
        return $2.RenewTokenRequest();
      default:
        throw $core.ArgumentError('Unknown method: $methodName');
    }
  }

  $async.Future<$pb.GeneratedMessage> handleCall($pb.ServerContext ctx,
      $core.String methodName, $pb.GeneratedMessage request) {
    switch (methodName) {
      case 'CreateToken':
        return createToken(ctx, request as $2.CreateTokenRequest);
      case 'ListTokens':
        return listTokens(ctx, request as $2.ListTokensRequest);
      case 'RevokeToken':
        return revokeToken(ctx, request as $2.RevokeTokenRequest);
      case 'RenewToken':
        return renewToken(ctx, request as $2.RenewTokenRequest);
      default:
        throw $core.ArgumentError('Unknown method: $methodName');
    }
  }

  $core.Map<$core.String, $core.dynamic> get $json => TokenServiceBase$json;
  $core.Map<$core.String, $core.Map<$core.String, $core.dynamic>>
      get $messageJson => TokenServiceBase$messageJson;
}

abstract class GrantServiceBase extends $pb.GeneratedService {
  $async.Future<$2.GrantServiceAdminResponse> admin(
      $pb.ServerContext ctx, $2.GrantServiceAdminRequest request);

  $pb.GeneratedMessage createRequest($core.String methodName) {
    switch (methodName) {
      case 'Admin':
        return $2.GrantServiceAdminRequest();
      default:
        throw $core.ArgumentError('Unknown method: $methodName');
    }
  }

  $async.Future<$pb.GeneratedMessage> handleCall($pb.ServerContext ctx,
      $core.String methodName, $pb.GeneratedMessage request) {
    switch (methodName) {
      case 'Admin':
        return admin(ctx, request as $2.GrantServiceAdminRequest);
      default:
        throw $core.ArgumentError('Unknown method: $methodName');
    }
  }

  $core.Map<$core.String, $core.dynamic> get $json => GrantServiceBase$json;
  $core.Map<$core.String, $core.Map<$core.String, $core.dynamic>>
      get $messageJson => GrantServiceBase$messageJson;
}
