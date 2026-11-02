//! Fails when a crate's modules are not levelized. The modules under one parent (a crate root,
//! or a module with children) must use each other without a cycle, and no module may use an item
//! that one of its ancestors defines, so a crate root sits on top and its leaves use nothing else
//! in the crate. Cargo already keeps the crate graph acyclic; this does the same inside a crate.
//!
//! Usage: pt-levels [--print] <crates dir>. `--print` also lists each parent's modules by level,
//! 0 being the modules that use no sibling.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Debug, PartialEq)]
enum Tok {
    Ident(String),
    Path,
    Open,
    Close,
    Semi,
    Comma,
    Star,
    Bang,
    Other(u8),
}

struct Token {
    tok: Tok,
    line: u32,
}

fn ident_byte(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

/// Skips a string body starting after its opening quote; returns the index after the closing one.
fn skip_string(b: &[u8], mut i: usize, line: &mut u32) -> usize {
    while i < b.len() {
        match b[i] {
            b'\\' => i += 2,
            b'"' => return i + 1,
            c => {
                if c == b'\n' {
                    *line += 1;
                }
                i += 1;
            }
        }
    }
    i
}

/// A raw string starting at `i` (`r"`, `r#"`, `br#"`), as the index after it, or None.
fn skip_raw_string(b: &[u8], i: usize, line: &mut u32) -> Option<usize> {
    let mut j = match (b[i], b.get(i + 1)) {
        (b'b', Some(b'r')) => i + 2,
        (b'r', _) => i + 1,
        _ => return None,
    };
    let start = j;
    while b.get(j) == Some(&b'#') {
        j += 1;
    }
    if b.get(j) != Some(&b'"') {
        return None;
    }
    let hashes = j - start;
    j += 1;
    while j < b.len() {
        if b[j] == b'"'
            && b[j + 1..]
                .iter()
                .take(hashes)
                .filter(|&&c| c == b'#')
                .count()
                == hashes
        {
            return Some(j + 1 + hashes);
        }
        if b[j] == b'\n' {
            *line += 1;
        }
        j += 1;
    }
    Some(j)
}

fn lex(src: &str) -> Vec<Token> {
    let b = src.as_bytes();
    let mut out = Vec::new();
    let (mut i, mut line) = (0, 1u32);
    while i < b.len() {
        let c = b[i];
        let next = b.get(i + 1).copied();
        let prev_ident = i > 0 && ident_byte(b[i - 1]);
        let push = |out: &mut Vec<Token>, tok| out.push(Token { tok, line });
        match c {
            b'\n' => {
                line += 1;
                i += 1;
            }
            b'/' if next == Some(b'/') => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if next == Some(b'*') => {
                let mut depth = 0;
                while i < b.len() {
                    if b[i..].starts_with(b"/*") {
                        depth += 1;
                        i += 2;
                    } else if b[i..].starts_with(b"*/") {
                        depth -= 1;
                        i += 2;
                        if depth == 0 {
                            break;
                        }
                    } else {
                        if b[i] == b'\n' {
                            line += 1;
                        }
                        i += 1;
                    }
                }
            }
            b'"' => i = skip_string(b, i + 1, &mut line),
            b'r' | b'b' if !prev_ident && skip_raw_string(b, i, &mut 0).is_some() => {
                i = skip_raw_string(b, i, &mut line).unwrap_or(b.len());
            }
            b'b' if !prev_ident && next == Some(b'"') => i = skip_string(b, i + 2, &mut line),
            b'b' if !prev_ident && next == Some(b'\'') => i = skip_char(b, i + 1),
            b'\'' => i = skip_char(b, i),
            b':' if next == Some(b':') => {
                push(&mut out, Tok::Path);
                i += 2;
            }
            b'{' | b'}' | b';' | b',' | b'*' | b'!' => {
                push(
                    &mut out,
                    match c {
                        b'{' => Tok::Open,
                        b'}' => Tok::Close,
                        b';' => Tok::Semi,
                        b',' => Tok::Comma,
                        b'*' => Tok::Star,
                        _ => Tok::Bang,
                    },
                );
                i += 1;
            }
            _ if c.is_ascii_digit() => {
                while i < b.len() && ident_byte(b[i]) {
                    i += 1;
                }
            }
            _ if ident_byte(c) || (c == b'$' && next.is_some_and(|n| n.is_ascii_alphabetic())) => {
                let start = i;
                i += 1;
                while i < b.len() && ident_byte(b[i]) {
                    i += 1;
                }
                push(&mut out, Tok::Ident(src[start..i].to_string()));
            }
            _ if c.is_ascii_whitespace() => i += 1,
            _ => {
                push(&mut out, Tok::Other(c));
                i += 1;
            }
        }
    }
    out
}

/// A char literal or a lifetime starting at the quote `i`; returns the index after it.
fn skip_char(b: &[u8], i: usize) -> usize {
    if b.get(i + 1) == Some(&b'\\') {
        let mut j = i + 2;
        while j < b.len() && b[j] != b'\'' {
            j += 1;
        }
        return j + 1;
    }
    let width = match b.get(i + 1) {
        Some(&c) if c >= 0xf0 => 4,
        Some(&c) if c >= 0xe0 => 3,
        Some(&c) if c >= 0xc0 => 2,
        _ => 1,
    };
    if b.get(i + 1 + width) == Some(&b'\'') {
        i + 2 + width
    } else {
        i + 1
    }
}

struct Module {
    path: Vec<String>,
    parent: Option<usize>,
    children: BTreeMap<String, usize>,
    /// The module whose file holds this one: itself, or for an inline `mod x { .. }` its file's.
    file: usize,
    /// `pub use` names, mapped to the absolute path they stand for.
    reexports: BTreeMap<String, Vec<String>>,
    /// Modules re-exported whole with `pub use x::*`.
    globs: Vec<Vec<String>>,
    /// Names of the items defined in this module.
    items: BTreeSet<String>,
}

struct Use {
    from: usize,
    path: Vec<String>,
    file: PathBuf,
    line: u32,
}

struct Tree {
    label: String,
    modules: Vec<Module>,
    files: Vec<(PathBuf, usize, PathBuf)>,
    uses: Vec<Use>,
    /// `#[macro_export]` macros live at the crate root but belong to the module defining them.
    macros: BTreeMap<String, usize>,
}

impl Tree {
    fn add(&mut self, parent: usize, name: &str, inline: bool) -> usize {
        if let Some(&m) = self.modules[parent].children.get(name) {
            return m;
        }
        let mut path = self.modules[parent].path.clone();
        path.push(name.to_string());
        let m = self.modules.len();
        self.modules.push(Module {
            path,
            parent: Some(parent),
            children: BTreeMap::new(),
            file: if inline { self.modules[parent].file } else { m },
            reexports: BTreeMap::new(),
            globs: Vec::new(),
            items: BTreeSet::new(),
        });
        self.modules[parent].children.insert(name.to_string(), m);
        m
    }

    fn is_ancestor(&self, a: usize, mut m: usize) -> bool {
        while let Some(p) = self.modules[m].parent {
            if p == a {
                return true;
            }
            m = p;
        }
        false
    }

    fn name(&self, m: usize) -> String {
        if self.modules[m].path.is_empty() {
            "crate".into()
        } else {
            self.modules[m].path.join("::")
        }
    }

    /// The deepest module `path` names, and the item it names inside that module, if any.
    fn resolve(&self, path: &[String], depth: u8) -> (usize, Option<String>) {
        let mut m = 0;
        for seg in path {
            if let Some(&c) = self.modules[m].children.get(seg) {
                m = c;
                continue;
            }
            if depth < 8
                && let Some(target) = self.modules[m].reexports.get(seg)
            {
                return self.resolve(target, depth + 1);
            }
            if m == 0
                && let Some(&def) = self.macros.get(seg)
            {
                return (def, None);
            }
            if depth < 8 {
                for glob in &self.modules[m].globs {
                    let (g, rest) = self.resolve(glob, depth + 1);
                    if rest.is_none() && self.modules[g].items.contains(seg) {
                        return (g, Some(seg.clone()));
                    }
                }
            }
            return (m, Some(seg.clone()));
        }
        (m, None)
    }
}

/// Reads `mod x;` and inline `mod x { .. }` declarations, recursing into child files.
fn build(tree: &mut Tree, m: usize, file: &Path, child_dir: &Path) -> Result<(), String> {
    let src = fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let toks = lex(&src);
    tree.files
        .push((file.to_path_buf(), m, child_dir.to_path_buf()));
    let mut stack: Vec<(usize, i32, PathBuf)> = vec![(m, 0, child_dir.to_path_buf())];
    let mut depth = 0;
    let mut i = 0;
    while i < toks.len() {
        let (cur, _, dir) = stack
            .last()
            .cloned()
            .unwrap_or((m, 0, child_dir.to_path_buf()));
        match &toks[i].tok {
            Tok::Ident(k) if k == "mod" => {
                if let (Some(Tok::Ident(name)), Some(after)) = (
                    toks.get(i + 1).map(|t| &t.tok),
                    toks.get(i + 2).map(|t| &t.tok),
                ) {
                    let child = tree.add(cur, name, *after == Tok::Open);
                    if *after == Tok::Semi {
                        let flat = dir.join(format!("{name}.rs"));
                        let nested = dir.join(name).join("mod.rs");
                        let path = if flat.exists() { flat } else { nested };
                        build(tree, child, &path, &dir.join(name))?;
                        i += 3;
                        continue;
                    }
                    if *after == Tok::Open {
                        depth += 1;
                        stack.push((child, depth, dir.join(name)));
                        i += 3;
                        continue;
                    }
                }
            }
            Tok::Ident(k)
                if matches!(
                    k.as_str(),
                    "struct" | "enum" | "union" | "trait" | "type" | "fn" | "const" | "static"
                ) =>
            {
                if let Some(Tok::Ident(name)) = toks.get(i + 1).map(|t| &t.tok) {
                    tree.modules[cur].items.insert(name.clone());
                }
            }
            Tok::Ident(k) if k == "macro_rules" => {
                if let Some(Tok::Ident(name)) = toks.get(i + 2).map(|t| &t.tok) {
                    tree.modules[cur].items.insert(name.clone());
                }
            }
            Tok::Open => depth += 1,
            Tok::Close => {
                if stack.len() > 1 && stack.last().is_some_and(|s| s.1 == depth) {
                    stack.pop();
                }
                depth -= 1;
            }
            _ => {}
        }
        i += 1;
    }
    Ok(())
}

/// Turns a path as written in module `m` into an absolute one, or None for another crate.
fn absolute(tree: &Tree, m: usize, segs: &[String]) -> Option<Vec<String>> {
    let first = segs.first()?;
    let mut base = match first.as_str() {
        "crate" | "$crate" => Vec::new(),
        "self" => tree.modules[m].path.clone(),
        "super" => {
            let mut at = m;
            let mut rest = segs;
            while rest.first().is_some_and(|s| s == "super") {
                at = tree.modules[at].parent?;
                rest = &rest[1..];
            }
            let mut p = tree.modules[at].path.clone();
            p.extend(rest.iter().cloned());
            return Some(p);
        }
        name if tree.modules[m].children.contains_key(name) => {
            let mut p = tree.modules[m].path.clone();
            p.push(name.to_string());
            p
        }
        _ => return None,
    };
    base.extend(segs[1..].iter().cloned());
    Some(base)
}

/// Parses one use tree at `toks[*i..]` into its leaf paths and their visible names.
fn use_tree(
    toks: &[Token],
    i: &mut usize,
    prefix: &[String],
    out: &mut Vec<(Vec<String>, String)>,
) {
    let mut path = prefix.to_vec();
    while *i < toks.len() {
        match &toks[*i].tok {
            Tok::Ident(s) if s == "self" && !path.is_empty() && path.len() == prefix.len() => {
                *i += 1;
                let name =
                    alias(toks, i).unwrap_or_else(|| path.last().cloned().unwrap_or_default());
                out.push((path.clone(), name));
                return;
            }
            Tok::Ident(s) => {
                path.push(s.clone());
                *i += 1;
                if toks.get(*i).map(|t| &t.tok) == Some(&Tok::Path) {
                    *i += 1;
                    continue;
                }
                let name = alias(toks, i).unwrap_or_else(|| s.clone());
                out.push((path, name));
                return;
            }
            Tok::Star => {
                *i += 1;
                path.push("*".into());
                out.push((path, "*".into()));
                return;
            }
            Tok::Open => {
                *i += 1;
                while *i < toks.len() && toks[*i].tok != Tok::Close {
                    if toks[*i].tok == Tok::Comma {
                        *i += 1;
                        continue;
                    }
                    let before = *i;
                    use_tree(toks, i, &path, out);
                    if *i == before {
                        *i += 1;
                    }
                }
                *i += 1;
                return;
            }
            _ => return,
        }
    }
}

fn alias(toks: &[Token], i: &mut usize) -> Option<String> {
    if let (Some(Tok::Ident(a)), Some(Tok::Ident(name))) = (
        toks.get(*i).map(|t| &t.tok),
        toks.get(*i + 1).map(|t| &t.tok),
    ) && a == "as"
    {
        *i += 2;
        return Some(name.clone());
    }
    None
}

/// Records every in-crate path each file names, and its `pub use` re-exports and exported macros.
fn collect(tree: &mut Tree) -> Result<(), String> {
    let files = tree.files.clone();
    for (file, m, _) in &files {
        let src = fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))?;
        let toks = lex(&src);
        let mut stack: Vec<(usize, i32)> = vec![(*m, 0)];
        let mut depth = 0;
        let mut i = 0;
        while i < toks.len() {
            let cur = stack.last().map_or(*m, |s| s.0);
            let prev = i.checked_sub(1).map(|p| &toks[p].tok);
            match &toks[i].tok {
                Tok::Ident(k) if k == "mod" => {
                    if let (Some(Tok::Ident(name)), Some(Tok::Open)) = (
                        toks.get(i + 1).map(|t| &t.tok),
                        toks.get(i + 2).map(|t| &t.tok),
                    ) && let Some(&child) = tree.modules[cur].children.get(name)
                    {
                        depth += 1;
                        stack.push((child, depth));
                        i += 3;
                        continue;
                    }
                }
                Tok::Ident(k) if k == "use" => {
                    let public = matches!(prev, Some(Tok::Ident(p)) if p == "pub")
                        || matches!(prev, Some(Tok::Other(b')')));
                    let line = toks[i].line;
                    i += 1;
                    if toks.get(i).map(|t| &t.tok) == Some(&Tok::Path) {
                        i += 1;
                    }
                    let mut leaves = Vec::new();
                    use_tree(&toks, &mut i, &[], &mut leaves);
                    for (segs, name) in leaves {
                        let Some(abs) = absolute(tree, cur, &segs) else {
                            continue;
                        };
                        if public && name == "*" {
                            tree.modules[cur].globs.push(abs[..abs.len() - 1].to_vec());
                        } else if public {
                            tree.modules[cur].reexports.insert(name, abs.clone());
                        }
                        tree.uses.push(Use {
                            from: cur,
                            path: abs,
                            file: file.clone(),
                            line,
                        });
                    }
                    continue;
                }
                Tok::Ident(k) if k == "macro_rules" => {
                    let exported = toks[i.saturating_sub(6)..i]
                        .iter()
                        .any(|t| matches!(&t.tok, Tok::Ident(a) if a == "macro_export"));
                    if exported
                        && toks.get(i + 1).map(|t| &t.tok) == Some(&Tok::Bang)
                        && let Some(Tok::Ident(name)) = toks.get(i + 2).map(|t| &t.tok)
                    {
                        tree.macros.insert(name.clone(), cur);
                    }
                }
                Tok::Ident(k)
                    if matches!(k.as_str(), "crate" | "$crate" | "super" | "self")
                        && prev != Some(&Tok::Path)
                        && toks.get(i + 1).map(|t| &t.tok) == Some(&Tok::Path) =>
                {
                    let line = toks[i].line;
                    let mut segs = vec![k.clone()];
                    let mut j = i + 1;
                    while toks.get(j).map(|t| &t.tok) == Some(&Tok::Path) {
                        match toks.get(j + 1).map(|t| &t.tok) {
                            Some(Tok::Ident(s)) => {
                                segs.push(s.clone());
                                j += 2;
                            }
                            _ => break,
                        }
                    }
                    if let Some(abs) = absolute(tree, cur, &segs) {
                        tree.uses.push(Use {
                            from: cur,
                            path: abs,
                            file: file.clone(),
                            line,
                        });
                    }
                    i = j;
                    continue;
                }
                Tok::Open => depth += 1,
                Tok::Close => {
                    if stack.len() > 1 && stack.last().is_some_and(|s| s.1 == depth) {
                        stack.pop();
                    }
                    depth -= 1;
                }
                _ => {}
            }
            i += 1;
        }
    }
    Ok(())
}

/// Child edges under each parent: parent -> (from child -> (to child -> first site)).
type Edges = BTreeMap<usize, BTreeMap<usize, BTreeMap<usize, String>>>;

fn check(tree: &Tree, root: &Path, print: bool, errors: &mut Vec<String>) {
    let mut edges: Edges = BTreeMap::new();
    let mut reported = std::collections::BTreeSet::new();
    for u in &tree.uses {
        let (to, item) = tree.resolve(&u.path, 0);
        let (from, to) = (tree.modules[u.from].file, tree.modules[to].file);
        let site = format!(
            "{}:{}",
            u.file.strip_prefix(root).unwrap_or(&u.file).display(),
            u.line
        );
        if to == from || tree.is_ancestor(from, to) {
            continue;
        }
        if tree.is_ancestor(to, from) {
            if let Some(item) = item
                && reported.insert((from, to, item.clone()))
            {
                errors.push(format!(
                    "{site}: {} uses `{}`, an item of its ancestor {}; move it into a module",
                    tree.name(from),
                    item,
                    tree.name(to)
                ));
            }
            continue;
        }
        let (mut a, mut b) = (from, to);
        let depth = |mut m: usize| {
            let mut d = 0;
            while let Some(p) = tree.modules[m].parent {
                d += 1;
                m = p;
            }
            d
        };
        while depth(a) > depth(b) {
            a = tree.modules[a].parent.unwrap_or(0);
        }
        while depth(b) > depth(a) {
            b = tree.modules[b].parent.unwrap_or(0);
        }
        while tree.modules[a].parent != tree.modules[b].parent {
            a = tree.modules[a].parent.unwrap_or(0);
            b = tree.modules[b].parent.unwrap_or(0);
        }
        let parent = tree.modules[a].parent.unwrap_or(0);
        edges
            .entry(parent)
            .or_default()
            .entry(a)
            .or_default()
            .entry(b)
            .or_insert(site);
    }
    for (m, module) in tree.modules.iter().enumerate() {
        if module.children.is_empty() || module.file != m {
            continue;
        }
        let empty = BTreeMap::new();
        let level_edges = edges.get(&m).unwrap_or(&empty);
        let kids: Vec<usize> = module
            .children
            .values()
            .copied()
            .filter(|&k| tree.modules[k].file == k)
            .collect();
        let sccs = strongly_connected(&kids, level_edges);
        let mut cyclic = false;
        for scc in sccs.iter().filter(|s| s.len() > 1) {
            cyclic = true;
            let mut msg = format!(
                "{}: in {}, these modules use each other in a cycle:",
                tree.label,
                tree.name(m)
            );
            for &a in scc {
                for (&b, site) in level_edges.get(&a).into_iter().flatten() {
                    if scc.contains(&b) {
                        msg.push_str(&format!(
                            "\n    {} -> {} ({site})",
                            tree.name(a),
                            tree.name(b)
                        ));
                    }
                }
            }
            errors.push(msg);
        }
        if print && !cyclic && kids.len() > 1 {
            let mut level: BTreeMap<usize, usize> = BTreeMap::new();
            for scc in &sccs {
                let k = scc[0];
                let l = level_edges
                    .get(&k)
                    .into_iter()
                    .flatten()
                    .map(|(t, _)| level.get(t).map_or(0, |l| l + 1))
                    .max()
                    .unwrap_or(0);
                level.insert(k, l);
            }
            let mut by_level: BTreeMap<usize, Vec<String>> = BTreeMap::new();
            for (k, l) in level {
                by_level
                    .entry(l)
                    .or_default()
                    .push(tree.modules[k].path.last().cloned().unwrap_or_default());
            }
            println!("{} {}:", tree.label, tree.name(m));
            for (l, names) in by_level {
                println!("  {l}: {}", names.join(" "));
            }
        }
    }
}

/// Tarjan's algorithm; components come out in reverse topological order (leaves first).
fn strongly_connected(
    nodes: &[usize],
    edges: &BTreeMap<usize, BTreeMap<usize, String>>,
) -> Vec<Vec<usize>> {
    struct Walk<'a> {
        edges: &'a BTreeMap<usize, BTreeMap<usize, String>>,
        index: BTreeMap<usize, usize>,
        low: BTreeMap<usize, usize>,
        stack: Vec<usize>,
        out: Vec<Vec<usize>>,
    }
    fn visit(w: &mut Walk, v: usize) {
        let n = w.index.len();
        w.index.insert(v, n);
        w.low.insert(v, n);
        w.stack.push(v);
        let targets: Vec<usize> = w
            .edges
            .get(&v)
            .into_iter()
            .flatten()
            .map(|(t, _)| *t)
            .collect();
        for t in targets {
            if !w.index.contains_key(&t) {
                visit(w, t);
                let l = w.low[&v].min(w.low[&t]);
                w.low.insert(v, l);
            } else if w.stack.contains(&t) {
                let l = w.low[&v].min(w.index[&t]);
                w.low.insert(v, l);
            }
        }
        if w.low[&v] == w.index[&v] {
            let mut scc = Vec::new();
            while let Some(x) = w.stack.pop() {
                scc.push(x);
                if x == v {
                    break;
                }
            }
            w.out.push(scc);
        }
    }
    let mut w = Walk {
        edges,
        index: BTreeMap::new(),
        low: BTreeMap::new(),
        stack: Vec::new(),
        out: Vec::new(),
    };
    for &v in nodes {
        if !w.index.contains_key(&v) {
            visit(&mut w, v);
        }
    }
    w.out
}

fn run(dir: &Path, print: bool) -> Result<(Vec<String>, usize), String> {
    let mut crates: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.join("src").is_dir())
        .collect();
    crates.sort();
    let mut errors = Vec::new();
    let mut modules = 0;
    for krate in crates {
        let src = krate.join("src");
        let name = krate
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let mut roots: Vec<PathBuf> = ["lib.rs", "main.rs"]
            .iter()
            .map(|r| src.join(r))
            .filter(|p| p.exists())
            .collect();
        if let Ok(bins) = fs::read_dir(src.join("bin")) {
            let mut bins: Vec<PathBuf> = bins
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().is_some_and(|x| x == "rs"))
                .collect();
            bins.sort();
            roots.extend(bins);
        }
        for root in roots {
            let rel = root
                .strip_prefix(&src)
                .unwrap_or(&root)
                .display()
                .to_string();
            let mut tree = Tree {
                label: if rel == "lib.rs" {
                    name.clone()
                } else {
                    format!("{name} ({rel})")
                },
                modules: vec![Module {
                    path: Vec::new(),
                    parent: None,
                    children: BTreeMap::new(),
                    file: 0,
                    reexports: BTreeMap::new(),
                    globs: Vec::new(),
                    items: BTreeSet::new(),
                }],
                files: Vec::new(),
                uses: Vec::new(),
                macros: BTreeMap::new(),
            };
            let child_dir = root.parent().unwrap_or(&src).to_path_buf();
            build(&mut tree, 0, &root, &child_dir)?;
            collect(&mut tree)?;
            modules += tree.modules.len() - 1;
            check(&tree, dir, print, &mut errors);
        }
    }
    Ok((errors, modules))
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let print = args.iter().any(|a| a == "--print");
    let Some(dir) = args.iter().find(|a| !a.starts_with("--")) else {
        eprintln!("usage: pt-levels [--print] <crates dir>");
        return ExitCode::from(2);
    };
    match run(Path::new(dir), print) {
        Ok((errors, modules)) if errors.is_empty() => {
            println!("pt-levels: ok ({modules} modules)");
            ExitCode::SUCCESS
        }
        Ok((errors, _)) => {
            for e in &errors {
                eprintln!("{e}");
            }
            eprintln!("pt-levels: {} problems", errors.len());
            ExitCode::FAILURE
        }
        Err(e) => {
            eprintln!("pt-levels: {e}");
            ExitCode::from(2)
        }
    }
}
