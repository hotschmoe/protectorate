use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, fs, io::Read, path::Path};
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Manifest {
    pub kind: String,
    pub path: String,
    pub name: String,
    pub deps: Vec<String>,
}
fn package(kind: &str, name: &str) -> String {
    let n = match kind {
        "cargo" => name.to_lowercase().replace('_', "-"),
        "pypi" => {
            let mut out = String::new();
            for c in name.to_lowercase().chars() {
                if matches!(c, '-' | '_' | '.') {
                    if !out.ends_with('-') {
                        out.push('-')
                    }
                } else {
                    out.push(c)
                }
            }
            out
        }
        _ => name.into(),
    };
    format!("{kind}:{n}")
}
fn add_req(deps: &mut BTreeSet<String>, s: &str) {
    let name: String = s
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        .collect();
    if !name.is_empty() {
        deps.insert(package("pypi", &name));
    }
}
pub fn parse_manifest(filename: &str, src: &str) -> Option<Manifest> {
    let kind = match filename {
        "go.mod" => "go",
        "package.json" => "npm",
        "Cargo.toml" => "cargo",
        "pyproject.toml" => "pypi",
        _ => return None,
    };
    let mut m = Manifest {
        kind: kind.into(),
        ..Default::default()
    };
    let mut deps = BTreeSet::new();
    match kind {
        "go" => {
            let mut require = false;
            for line in src.lines() {
                let line = line.split("//").next().unwrap_or("");
                let fields: Vec<_> = line.split_whitespace().collect();
                if fields.is_empty() {
                    continue;
                }
                let unquote = |s: &str| s.trim_matches(['"', '`']).to_string();
                if require {
                    if fields[0] == ")" {
                        require = false
                    } else {
                        deps.insert(package(kind, &unquote(fields[0])));
                    }
                } else if fields[0] == "module" && fields.len() > 1 {
                    m.name = package(kind, &unquote(fields[1]));
                } else if fields[0] == "require" && fields.len() > 1 {
                    if fields[1] == "(" {
                        require = true
                    } else {
                        deps.insert(package(kind, &unquote(fields[1])));
                    }
                }
            }
            if m.name.is_empty() {
                return None;
            }
        }
        "npm" => {
            let v: serde_json::Value = serde_json::from_str(src).ok()?;
            let obj = v.as_object()?;
            if let Some(n) = obj
                .get("name")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
            {
                m.name = package(kind, n)
            }
            for section in [
                "dependencies",
                "devDependencies",
                "peerDependencies",
                "optionalDependencies",
            ] {
                if let Some(ds) = obj.get(section).and_then(|v| v.as_object()) {
                    deps.extend(ds.keys().map(|n| package(kind, n)));
                }
            }
        }
        "cargo" => {
            let v: toml::Value = toml::from_str(src).ok()?;
            if let Some(n) = v
                .get("package")
                .and_then(|v| v.get("name"))
                .and_then(|v| v.as_str())
            {
                m.name = package(kind, n)
            }
            fn dependency_tables(v: &toml::Value, deps: &mut BTreeSet<String>) {
                if let Some(t) = v.as_table() {
                    for key in ["dependencies", "dev-dependencies", "build-dependencies"] {
                        if let Some(ds) = t.get(key).and_then(|v| v.as_table()) {
                            for (name, spec) in ds {
                                let name =
                                    spec.get("package").and_then(|v| v.as_str()).unwrap_or(name);
                                deps.insert(package("cargo", name));
                            }
                        }
                    }
                }
            }
            dependency_tables(&v, &mut deps);
            if let Some(workspace) = v.get("workspace") {
                dependency_tables(workspace, &mut deps);
            }
            if let Some(targets) = v.get("target").and_then(|v| v.as_table()) {
                for target in targets.values() {
                    dependency_tables(target, &mut deps);
                }
            }
        }
        "pypi" => {
            let v: toml::Value = toml::from_str(src).ok()?;
            let poetry = v.get("tool").and_then(|v| v.get("poetry"));
            if let Some(n) = v
                .get("project")
                .and_then(|v| v.get("name"))
                .and_then(|v| v.as_str())
                .or_else(|| poetry.and_then(|v| v.get("name")).and_then(|v| v.as_str()))
            {
                m.name = package(kind, n)
            }
            fn reqs(v: Option<&toml::Value>, deps: &mut BTreeSet<String>) {
                if let Some(xs) = v.and_then(|v| v.as_array()) {
                    for x in xs {
                        if let Some(s) = x.as_str() {
                            add_req(deps, s)
                        }
                    }
                }
            }
            reqs(
                v.get("project").and_then(|v| v.get("dependencies")),
                &mut deps,
            );
            for table in [
                v.get("project")
                    .and_then(|v| v.get("optional-dependencies")),
                v.get("dependency-groups"),
            ]
            .into_iter()
            .flatten()
            {
                if let Some(t) = table.as_table() {
                    for x in t.values() {
                        reqs(Some(x), &mut deps)
                    }
                }
            }
            if let Some(p) = poetry {
                fn poetry_deps(v: &toml::Value, deps: &mut BTreeSet<String>) {
                    if let Some(t) = v.as_table() {
                        for (k, x) in t {
                            if k == "dependencies" || k == "dev-dependencies" {
                                if let Some(ds) = x.as_table() {
                                    for n in ds.keys().filter(|n| n.as_str() != "python") {
                                        deps.insert(package("pypi", n));
                                    }
                                }
                            } else {
                                poetry_deps(x, deps)
                            }
                        }
                    }
                }
                poetry_deps(p, &mut deps)
            }
        }
        _ => unreachable!(),
    }
    if m.name.is_empty() && deps.is_empty() && kind != "npm" {
        return None;
    }
    m.deps = deps.into_iter().collect();
    Some(m)
}
/// Bounded monorepo discovery: depth two, 300 directories, 1 MiB per manifest.
/// Directory symlinks are never followed.
pub fn read_manifests(root: impl AsRef<Path>) -> Vec<Manifest> {
    fn walk(root: &Path, rel: &Path, depth: usize, visited: &mut usize, out: &mut Vec<Manifest>) {
        let entries = match fs::read_dir(root.join(rel)) {
            Ok(e) => e,
            Err(_) => return,
        };
        *visited += 1;
        for name in ["go.mod", "package.json", "Cargo.toml", "pyproject.toml"] {
            let path = root.join(rel).join(name);
            let Ok(f) = fs::File::open(&path) else {
                continue;
            };
            let Ok(meta) = f.metadata() else { continue };
            if !meta.is_file() || meta.len() > 1 << 20 {
                continue;
            }
            let mut src = String::new();
            if f.take((1 << 20) + 1).read_to_string(&mut src).is_err() || src.len() > 1 << 20 {
                continue;
            }
            if let Some(mut m) = parse_manifest(name, &src) {
                m.path = rel.join(name).to_string_lossy().replace('\\', "/");
                out.push(m)
            }
        }
        if depth >= 2 {
            return;
        }
        let mut entries: Vec<_> = entries.filter_map(|e| e.ok()).collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let name = e.file_name().to_string_lossy().into_owned();
            if *visited >= 300 {
                break;
            }
            if name.starts_with('.')
                || [
                    "node_modules",
                    "vendor",
                    "target",
                    "dist",
                    "build",
                    "testdata",
                    "__pycache__",
                    "venv",
                ]
                .contains(&name.as_str())
                || !e.file_type().is_ok_and(|t| t.is_dir())
            {
                continue;
            }
            walk(root, &rel.join(name), depth + 1, visited, out)
        }
    }
    let mut out = Vec::new();
    walk(root.as_ref(), Path::new(""), 0, &mut 0, &mut out);
    out
}
