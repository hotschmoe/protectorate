//! Scope decisions shared by the future Rust hub, CLI and terminal endpoint.
//! The method catalog is supplied by the wire layer so generated API metadata
//! remains the single source of truth.

pub const SCOPE_FULL: &str = "full";
pub const SCOPE_READ: &str = "read";
pub const SCOPE_EVENTS: &str = "events";
pub const SCOPE_TERMINAL: &str = "terminal";
pub const SCOPE_TERMINAL_READONLY: &str = "terminal:readonly";
pub const SCOPE_TERMINAL_HOST: &str = "terminal:host";
pub const SCOPE_NODE_ADMIN_PREFIX: &str = "node_admin:";

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
    NodeAdminReserved(String),
    BadPattern(String),
    NoMatchingTool(String),
}

impl std::fmt::Display for ScopeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyList => write!(f, "at least one scope is required"),
            Self::EmptyScope => write!(f, "empty scope"),
            Self::FullReserved => write!(f, "Full access is issued through device approval"),
            Self::NodeAdminReserved(s) => write!(f, "scope {s:?} is only issued per task"),
            Self::BadPattern(s) => write!(f, "bad scope {s:?}"),
            Self::NoMatchingTool(s) => write!(f, "scope {s:?} matches no tool"),
        }
    }
}

impl std::error::Error for ScopeError {}

/// Implements Go's path.Match grammar for scope and node names.
/// This keeps grant decisions independent of a glob dependency.
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

fn node_glob(scope: &str) -> Option<&str> {
    scope.strip_prefix(SCOPE_NODE_ADMIN_PREFIX)
}

fn validate_node_glob(scope: &str) -> Result<(), ScopeError> {
    let g = node_glob(scope).unwrap_or("");
    if g.is_empty()
        || g.len() > 63
        || !g
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b".*?[]_-".contains(&b))
    {
        return Err(ScopeError::BadPattern(scope.into()));
    }
    glob_matches(g, "").map(|_| ())
}

fn normalize(
    scopes: &[&str],
    methods: &[Method<'_>],
    allow_grants: bool,
) -> Result<Vec<String>, ScopeError> {
    if scopes.is_empty() {
        return Err(ScopeError::EmptyList);
    }
    let mut out = Vec::new();
    for &s in scopes {
        if s.is_empty() {
            return Err(ScopeError::EmptyScope);
        }
        if s == SCOPE_FULL && !allow_grants {
            return Err(ScopeError::FullReserved);
        }
        if node_glob(s).is_some() {
            if !allow_grants {
                return Err(ScopeError::NodeAdminReserved(s.into()));
            }
            validate_node_glob(s)?;
        } else if ![
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
    normalize(scopes, methods, false)
}

pub fn grant_scopes(node: &str, methods: &[Method<'_>]) -> Result<Vec<String>, ScopeError> {
    let grant = format!("{SCOPE_NODE_ADMIN_PREFIX}{node}");
    normalize(&["job_*", &grant], methods, true)
}

pub fn admin_on(scopes: &[&str], node: &str) -> bool {
    scopes
        .iter()
        .filter_map(|s| node_glob(s))
        .any(|g| glob_matches(g, node).unwrap_or(false))
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
        _ if node_glob(s).is_some() => false,
        _ => glob_matches(s, method.tool).unwrap_or(false),
    })
}

/// Match the Go hub's public Grants helper: unknown and owner-only tools deny.
pub fn grants(scopes: &[&str], tool: &str, methods: &[Method<'_>]) -> bool {
    methods
        .iter()
        .find(|method| method.tool == tool)
        .is_some_and(|method| scopes_allow(scopes, method))
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

#[cfg(test)]
mod tests {
    use super::*;
    const METHODS: &[Method<'static>] = &[
        Method {
            tool: "job_run",
            service: "JobService",
            read_only: false,
            owner_only: false,
        },
        Method {
            tool: "job_list",
            service: "JobService",
            read_only: true,
            owner_only: false,
        },
        Method {
            tool: "token_create",
            service: "TokenService",
            read_only: false,
            owner_only: true,
        },
    ];

    #[test]
    fn normalization_and_grants() {
        assert_eq!(
            validate_scopes(&["job_*", "read", "job_*"], METHODS).unwrap(),
            ["job_*", "read"]
        );
        assert!(matches!(
            validate_scopes(&["full"], METHODS),
            Err(ScopeError::FullReserved)
        ));
        assert!(matches!(
            validate_scopes(&["node_admin:*"], METHODS),
            Err(ScopeError::NodeAdminReserved(_))
        ));
        assert!(matches!(
            validate_scopes(&["typo_*"], METHODS),
            Err(ScopeError::NoMatchingTool(_))
        ));
        assert_eq!(
            grant_scopes("node-[12]", METHODS).unwrap(),
            ["job_*", "node_admin:node-[12]"]
        );
        assert!(admin_on(&["node_admin:node-[12]"], "node-1"));
        assert!(!admin_on(&["node_admin:node-[12]"], "node-3"));
    }

    #[test]
    fn owner_only_and_terminals() {
        assert!(scopes_allow(&["read"], &METHODS[1]));
        assert!(!scopes_allow(&["read"], &METHODS[0]));
        assert!(!scopes_allow(&["full"], &METHODS[2]));
        assert!(!scopes_allow_terminal(&["terminal:readonly"], false));
        assert!(scopes_allow_terminal(&["terminal:readonly"], true));
        assert!(!scopes_allow(&["terminal"], &METHODS[0]));
        assert!(grants(&["read"], "job_list", METHODS));
        assert!(!grants(&["full"], "token_create", METHODS));
        assert!(!grants(&["full"], "unknown", METHODS));
    }

    #[test]
    fn glob_grammar() {
        assert!(glob_matches("job_[a-z]*", "job_run").unwrap());
        assert!(!glob_matches("job_[^a-z]*", "job_run").unwrap());
        assert!(glob_matches("job_?un", "job_run").unwrap());
        assert!(glob_matches("job_\\*", "job_*").unwrap());
        assert!(glob_matches("[", "x").is_err());
        assert!(!glob_matches(r"job_[a\-z]*", "job_run").unwrap());
        assert!(glob_matches(r"job_[a\-z]*", "job_-un").unwrap());
        assert!(glob_matches("job_[a-]*", "job_run").is_err());
        assert!(glob_matches("job_[a-b-c]*", "job_run").is_err());
        assert!(glob_matches("[/]", "/").unwrap());
        assert!(!glob_matches("*", "a/b").unwrap());
        assert!(!glob_matches("[z-a]", "z").unwrap());
        assert!(glob_matches(r"[a\]b]", "]").unwrap());
        assert!(glob_matches(r"[\\]", "\\").unwrap());
        assert!(glob_matches("[^a]", "b").unwrap());
        assert!(glob_matches("[]", "a").is_err());
        assert!(glob_matches(&"*a".repeat(1024), "a").is_ok());
    }
}
