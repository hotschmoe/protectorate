//
//  Generated code. Do not modify.
//  source: protectorate/v1/directory.proto
//

import "package:connectrpc/connect.dart" as connect;
import "directory.pb.dart" as protectoratev1directory;
import "directory.connect.spec.dart" as specs;

/// DirectoryService answers which projects the sleeves work on and how they
/// depend on each other. The hub builds it deterministically from each sleeve
/// workspace's git remote and package manifests (go.mod, package.json,
/// Cargo.toml, pyproject.toml), on spawn and on a slow interval. A project is
/// identified by its normalized git remote, or by "path:<workspace>" without
/// one. Project A depends on project B when one of A's dependencies is a
/// package B provides. A project stays listed, with no owners, after its last
/// sleeve is gone.
extension type DirectoryServiceClient (connect.Transport _transport) {
  /// ListProjects returns every known project, optionally only those a sleeve
  /// works on.
  Future<protectoratev1directory.ListProjectsResponse> listProjects(
    protectoratev1directory.ListProjectsRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.DirectoryService.listProjects,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// GetProject returns one project by id, "owner/repo" or bare name. A name
  /// that several projects share is an error that lists them.
  Future<protectoratev1directory.GetProjectResponse> getProject(
    protectoratev1directory.GetProjectRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.DirectoryService.getProject,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }
}
