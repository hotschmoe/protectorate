//! Credentials and scope decisions shared by the hub, CLI and terminal
//! endpoint. The method catalog is supplied by the wire layer so generated API
//! metadata remains the single source of truth.
pub mod capability;

pub const SCOPE_FULL: &str = "full";
pub const SCOPE_READ: &str = "read";
pub const SCOPE_EVENTS: &str = "events";
pub const SCOPE_TERMINAL: &str = "terminal";
pub const SCOPE_TERMINAL_READONLY: &str = "terminal:readonly";
pub const SCOPE_TERMINAL_HOST: &str = "terminal:host";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Method<'a> {
    pub tool: &'a str,
    pub service: &'a str,
    pub read_only: bool,
    pub owner_only: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ScopeError {
    EmptyList,
    EmptyScope,
    FullReserved,
    BadPattern(String),
    NoMatchingTool(String),
}

impl std::fmt::Display for ScopeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyList => write!(f, "at least one scope is required"),
            Self::EmptyScope => write!(f, "empty scope"),
            Self::FullReserved => write!(f, "Full access belongs to devices only"),
            Self::BadPattern(s) => write!(f, "bad scope {s:?}"),
            Self::NoMatchingTool(s) => write!(f, "scope {s:?} matches no tool"),
        }
    }
}

impl std::error::Error for ScopeError {}

/// Shell-style globs (`*`, `?`, `[a-z]`, `[^x]`, `\\` escapes; `*` and `?`
/// never match `/`) for scope names, without a glob dependency.
pub fn glob_matches(pattern: &str, name: &str) -> Result<bool, ScopeError> {
    let pat: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = name.chars().collect();
    let bad = || ScopeError::BadPattern(pattern.to_owned());
    let mut dp = vec![false; text.len() + 1];
    dp[0] = true;
    let mut i = 0;
    while i < pat.len() {
        let mut next = vec![false; text.len() + 1];
        match pat[i] {
            '*' => {
                next[0] = dp[0];
                for j in 1..=text.len() {
                    next[j] = dp[j] || (text[j - 1] != '/' && next[j - 1]);
                }
                i += 1;
            }
            '?' => {
                for j in 1..=text.len() {
                    next[j] = dp[j - 1] && text[j - 1] != '/';
                }
                i += 1;
            }
            '[' => {
                let (width, _) = parse_class(&pat[i..], '\0').map_err(|_| bad())?;
                for j in 1..=text.len() {
                    next[j] = dp[j - 1] && class_match(&pat[i..], text[j - 1]).1;
                }
                i += width;
            }
            '\\' => {
                i += 1;
                let c = *pat.get(i).ok_or_else(bad)?;
                for j in 1..=text.len() {
                    next[j] = dp[j - 1] && text[j - 1] == c;
                }
                i += 1;
            }
            c => {
                for j in 1..=text.len() {
                    next[j] = dp[j - 1] && text[j - 1] == c;
                }
                i += 1;
            }
        }
        dp = next;
    }
    Ok(dp[text.len()])
}

fn class_char(p: &[char], i: &mut usize) -> Result<char, ()> {
    if *i >= p.len() || p[*i] == '-' || p[*i] == ']' {
        return Err(());
    }
    if p[*i] == '\\' {
        *i += 1;
        if *i >= p.len() {
            return Err(());
        }
    }
    let c = p[*i];
    *i += 1;
    Ok(c)
}

fn parse_class(p: &[char], c: char) -> Result<(usize, bool), ()> {
    let mut i = 1;
    let negate = p.get(i) == Some(&'^');
    if negate {
        i += 1;
    }
    let mut matched = false;
    let mut count = 0;
    loop {
        if i >= p.len() {
            return Err(());
        }
        if p[i] == ']' && count > 0 {
            return Ok((i + 1, if negate { !matched } else { matched }));
        }
        let lo = class_char(p, &mut i)?;
        let hi = if p.get(i) == Some(&'-') {
            i += 1;
            class_char(p, &mut i)?
        } else {
            lo
        };
        matched |= lo <= c && c <= hi;
        count += 1;
    }
}

fn class_match(p: &[char], c: char) -> (usize, bool) {
    // glob_matches validates the entire pattern before matching.
    parse_class(p, c).expect("validated class")
}

fn normalize(scopes: &[&str], methods: &[Method<'_>]) -> Result<Vec<String>, ScopeError> {
    if scopes.is_empty() {
        return Err(ScopeError::EmptyList);
    }
    let mut out = Vec::new();
    for &s in scopes {
        if s.is_empty() {
            return Err(ScopeError::EmptyScope);
        }
        if s == SCOPE_FULL {
            return Err(ScopeError::FullReserved);
        }
        if ![
            SCOPE_FULL,
            SCOPE_READ,
            SCOPE_EVENTS,
            SCOPE_TERMINAL,
            SCOPE_TERMINAL_READONLY,
            SCOPE_TERMINAL_HOST,
        ]
        .contains(&s)
        {
            glob_matches(s, "")?;
            if !methods
                .iter()
                .filter(|m| !m.owner_only)
                .any(|m| glob_matches(s, m.tool).unwrap_or(false))
            {
                return Err(ScopeError::NoMatchingTool(s.into()));
            }
        }
        out.push(s.to_owned());
    }
    out.sort();
    out.dedup();
    Ok(out)
}

pub fn validate_scopes(scopes: &[&str], methods: &[Method<'_>]) -> Result<Vec<String>, ScopeError> {
    normalize(scopes, methods)
}

pub fn scopes_allow(scopes: &[&str], method: &Method<'_>) -> bool {
    if method.owner_only {
        return false;
    }
    scopes.iter().any(|s| match *s {
        SCOPE_FULL => true,
        SCOPE_READ => method.read_only,
        SCOPE_EVENTS => method.service == "EventService",
        SCOPE_TERMINAL | SCOPE_TERMINAL_READONLY | SCOPE_TERMINAL_HOST => false,
        _ => glob_matches(s, method.tool).unwrap_or(false),
    })
}

/// Whether `scopes` grant `tool`; unknown and owner-only tools deny.
pub fn grants(scopes: &[&str], tool: &str, methods: &[Method<'_>]) -> bool {
    methods
        .iter()
        .find(|method| method.tool == tool)
        .is_some_and(|method| scopes_allow(scopes, method))
}

/// The scopes that allow exactly what both `a` and `b` allow, used when the
/// hub mints for a caller holding `b` and when a holder narrowed a token. A
/// glob `b` only partly covers becomes the tool names both allow.
pub fn intersect(a: &[String], b: &[String], methods: &[Method<'_>]) -> Vec<String> {
    let b_scopes: Vec<&str> = b.iter().map(String::as_str).collect();
    let terminal = |s: &str| -> usize {
        match s {
            SCOPE_TERMINAL_READONLY => 1,
            SCOPE_TERMINAL => 2,
            SCOPE_TERMINAL_HOST | SCOPE_FULL => 3,
            _ => 0,
        }
    };
    let mut out = std::collections::BTreeSet::new();
    for s in a {
        match s.as_str() {
            _ if b_scopes.contains(&SCOPE_FULL) => {
                out.insert(s.clone());
            }
            SCOPE_FULL => out.extend(b.iter().cloned()),
            SCOPE_TERMINAL_READONLY | SCOPE_TERMINAL | SCOPE_TERMINAL_HOST => {
                let rank = terminal(s).min(b_scopes.iter().map(|s| terminal(s)).max().unwrap_or(0));
                if let Some(scope) = [SCOPE_TERMINAL_READONLY, SCOPE_TERMINAL, SCOPE_TERMINAL_HOST]
                    .get(rank.wrapping_sub(1))
                {
                    out.insert((*scope).to_owned());
                }
            }
            _ => {
                let mine: Vec<&Method<'_>> = methods
                    .iter()
                    .filter(|m| scopes_allow(&[s.as_str()], m))
                    .collect();
                if mine.iter().all(|m| scopes_allow(&b_scopes, m)) {
                    out.insert(s.clone());
                } else {
                    out.extend(
                        mine.iter()
                            .filter(|m| scopes_allow(&b_scopes, m))
                            .map(|m| m.tool.to_owned()),
                    );
                }
            }
        }
    }
    out.into_iter().collect()
}

pub fn needed(method: &Method<'_>) -> String {
    if method.read_only {
        format!("{SCOPE_READ} or {}", method.tool)
    } else if method.service == "EventService" {
        format!("{SCOPE_EVENTS} or {}", method.tool)
    } else {
        method.tool.to_owned()
    }
}

pub fn scopes_allow_terminal(scopes: &[&str], read_only: bool) -> bool {
    scopes.iter().any(|s| {
        matches!(*s, SCOPE_FULL | SCOPE_TERMINAL | SCOPE_TERMINAL_HOST)
            || (read_only && *s == SCOPE_TERMINAL_READONLY)
    })
}
