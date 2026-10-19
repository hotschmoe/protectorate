//
//  Generated code. Do not modify.
//  source: protectorate/v1/directory.proto
//

import "package:connectrpc/connect.dart" as connect;
import "directory.pb.dart" as protectoratev1directory;

/// DirectoryService answers which projects the sleeves work on and how they
/// depend on each other. The hub builds it deterministically from each sleeve
/// workspace's git remote and package manifests (go.mod, package.json,
/// Cargo.toml, pyproject.toml), on spawn and on a slow interval. A project is
/// identified by its normalized git remote, or by "path:<workspace>" without
/// one. Project A depends on project B when one of A's dependencies is a
/// package B provides. A project stays listed, with no owners, after its last
/// sleeve is gone.
abstract final class DirectoryService {
  /// Fully-qualified name of the DirectoryService service.
  static const name = 'protectorate.v1.DirectoryService';

  /// ListProjects returns every known project, optionally only those a sleeve
  /// works on.
  static const listProjects = connect.Spec(
    '/$name/ListProjects',
    connect.StreamType.unary,
    protectoratev1directory.ListProjectsRequest.new,
    protectoratev1directory.ListProjectsResponse.new,
    idempotency: connect.Idempotency.noSideEffects,
  );

  /// GetProject returns one project by id, "owner/repo" or bare name. A name
  /// that several projects share is an error that lists them.
  static const getProject = connect.Spec(
    '/$name/GetProject',
    connect.StreamType.unary,
    protectoratev1directory.GetProjectRequest.new,
    protectoratev1directory.GetProjectResponse.new,
    idempotency: connect.Idempotency.noSideEffects,
  );
}
