use protectorate_directory::*;
use std::{fs, path::Path, process::Command};
fn write(root: &Path, path: &str, text: &str) {
    let p = root.join(path);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, text).unwrap();
}
fn git_test(root: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{:?}: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().into()
}
fn repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    git_test(dir.path(), &["init", "-b", "main"]);
    git_test(dir.path(), &["config", "user.name", "Test"]);
    git_test(
        dir.path(),
        &["config", "user.email", "test@example.invalid"],
    );
    dir
}
fn commit(root: &Path, name: &str) -> String {
    write(root, "file.txt", name);
    git_test(root, &["add", "file.txt"]);
    git_test(root, &["commit", "-m", name]);
    git_test(root, &["rev-parse", "HEAD"])
}
#[test]
fn normalize_common_remotes() {
    for raw in [
        "git@GitHub.com:OWNER/Repo.git",
        "https://user:secret@GITHUB.com:443/Owner/Repo.git/",
        "ssh://git@github.com:22/owner/repo.git",
    ] {
        assert_eq!(normalize_remote(raw), "github.com/owner/repo")
    }
    assert_eq!(normalize_remote("file:///tmp/../srv/repo.git"), "/srv/repo");
    assert_eq!(normalize_remote("../repo.git"), "../repo");
    assert_eq!(normalize_remote(" /tmp/repo "), "/tmp/repo");
    assert_eq!(normalize_remote(""), "");
}
#[test]
fn manifest_ecosystems_and_bounded_scan() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "go.mod",
        "module github.com/a/lib\nrequire (\n github.com/a/base v1.0.0\n)\n",
    );
    write(
        dir.path(),
        "web/package.json",
        r#"{"name":"@a/web","dependencies":{"left-pad":"1"},"peerDependencies":{"react":"*"}}"#,
    );
    write(
        dir.path(),
        "rust/Cargo.toml",
        r#"[package]
name = 'my_crate'
[dependencies]
alias = { package = 'actual_crate', version = '1' }
[target.'cfg(unix)'.dependencies]
unix_dep = '1'
[workspace.dependencies]
shared = '1'
"#,
    );
    write(
        dir.path(),
        "python/pyproject.toml",
        r#"[project]
name = 'My_App'
dependencies = [
 'Requests>=2',
 'Some.Package[extra]; python_version > "3"',
]
[project.optional-dependencies]
test = ['pytest']
[tool.poetry.group.dev.dependencies]
ruff = '*'
"#,
    );
    write(
        dir.path(),
        "node_modules/package.json",
        r#"{"name":"hidden"}"#,
    );
    write(dir.path(), "a/b/c/package.json", r#"{"name":"too-deep"}"#);
    let ms = read_manifests(dir.path());
    assert_eq!(ms.len(), 4);
    let rust = ms.iter().find(|m| m.kind == "cargo").unwrap();
    assert_eq!(rust.name, "cargo:my-crate");
    assert_eq!(
        rust.deps,
        ["cargo:actual-crate", "cargo:shared", "cargo:unix-dep"]
    );
    let py = ms.iter().find(|m| m.kind == "pypi").unwrap();
    assert_eq!(py.name, "pypi:my-app");
    assert_eq!(
        py.deps,
        [
            "pypi:pytest",
            "pypi:requests",
            "pypi:ruff",
            "pypi:some-package"
        ]
    );
    assert!(ms.iter().any(|m| m.name == "go:github.com/a/lib"));
}
fn scan(name: &str, remote: &str, package: &str, deps: &[&str]) -> Scan {
    Scan {
        sleeve: name.into(),
        workspace: Workspace {
            path: format!("/work/{name}").into(),
            remote: remote.into(),
            manifests: vec![Manifest {
                kind: "cargo".into(),
                name: package.into(),
                deps: deps.iter().map(|s| s.to_string()).collect(),
                ..Default::default()
            }],
            ..Default::default()
        },
    }
}
#[test]
fn project_graph_persists_without_live_owners() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("nested/directory.json");
    let d = Directory::open(&path).unwrap();
    let a = scan(
        "alice",
        "github.com/a/app",
        "cargo:app",
        &["cargo:base", "cargo:app"],
    );
    let b = scan("bob", "github.com/a/base", "cargo:base", &[]);
    d.update(&[a.clone(), b]).unwrap();
    let app = d.get("A/APP").unwrap();
    assert_eq!(
        app.depends_on,
        vec![Edge {
            project: "github.com/a/base".into(),
            via: vec!["cargo:base".into()]
        }]
    );
    assert_eq!(app.project.deps, ["cargo:base"]);
    assert_eq!(
        d.get("base").unwrap().dependents[0].project,
        "github.com/a/app"
    );
    assert_eq!(d.projects_of("alice"), ["github.com/a/app"]);
    d.set_refs(
        "github.com/a/app",
        RefState {
            default: "main".into(),
            default_tip: "abc".into(),
            ..Default::default()
        },
    )
    .unwrap();
    let restored = Directory::open(&path).unwrap();
    assert!(restored.projects_of("alice").is_empty());
    assert_eq!(
        restored
            .get("app")
            .unwrap()
            .project
            .refs
            .unwrap()
            .default_tip,
        "abc"
    );
    d.update(&[a]).unwrap();
    assert!(d.get("base").unwrap().project.owners.is_empty());
    d.update(&[scan("other", "github.com/b/app", "cargo:other", &[])])
        .unwrap();
    assert!(matches!(d.get("app"), Err(Error::Ambiguous { .. })));
    assert!(matches!(d.get("absent"), Err(Error::NotFound(_))));
}
#[test]
fn actual_git_inspection_refs_and_change_facts() {
    let dir = repo();
    git_test(
        dir.path(),
        &["remote", "add", "origin", "git@GitHub.com:A/Example.git"],
    );
    write(dir.path(), "go.mod", "module example.com/demo\n");
    let first = commit(dir.path(), "first");
    git_test(dir.path(), &["tag", "v1"]);
    let old = read_refs(dir.path()).unwrap();
    assert_eq!(old.default, "main");
    assert_eq!(old.default_tip, first);
    assert_eq!(old.latest_tag(), "v1");
    let second = commit(dir.path(), "second");
    let now = read_refs(dir.path()).unwrap();
    let ancestor = |a: &str, b: &str| is_ancestor(dir.path(), a, b);
    let changes = changes(&old, &now, ancestor);
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].kind, "branch");
    let f = describe(dir.path(), &first, &second).unwrap();
    assert_eq!(f.total_commits, 1);
    assert_eq!(f.total_files, 1);
    assert!(f.files[0].contains("file.txt"));
    assert!(f.to_string().contains("second"));
    git_test(dir.path(), &["tag", "-a", "v2", "-m", "release"]);
    let tagged = read_refs(dir.path()).unwrap();
    assert!(tagged
        .tags
        .iter()
        .any(|t| t.name == "v2" && t.sha == second));
    assert_eq!(
        protectorate_directory::changes(&old, &tagged, ancestor)
            .iter()
            .filter(|c| c.kind == "branch")
            .count(),
        0
    );
    let w = inspect(dir.path());
    assert_eq!(w.remote, "github.com/a/example");
    assert_eq!(w.branch, "main");
    assert_eq!(w.head, second);
    assert_eq!(w.manifests[0].name, "go:example.com/demo");
    git_test(dir.path(), &["checkout", "--detach", &first]);
    assert!(inspect(dir.path()).branch.is_empty());
    let merged = merge(&tagged, &old, ancestor);
    assert_eq!(merged.default_tip, second);
    assert!(merged.tags.iter().any(|t| t.name == "v2"));
    assert!(protectorate_directory::changes(&now, &old, ancestor).is_empty());
}
#[test]
fn missing_git_still_discovers_packages() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "package.json", r#"{"name":"local"}"#);
    let w = inspect(dir.path());
    assert!(w.remote.is_empty());
    assert_eq!(w.manifests[0].name, "npm:local");
    assert!(project_id(&w).starts_with("path:"));
    assert!(!is_ancestor(dir.path(), "missing", "also-missing"));
}
#[cfg(unix)]
#[test]
fn scanning_does_not_follow_directory_symlinks() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    write(outside.path(), "package.json", r#"{"name":"outside"}"#);
    symlink(outside.path(), dir.path().join("linked")).unwrap();
    assert!(read_manifests(dir.path()).is_empty());
}
