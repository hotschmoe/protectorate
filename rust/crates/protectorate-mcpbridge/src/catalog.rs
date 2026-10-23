//! The tool catalog: every unary hub RPC, named `<service>_<method>`, plus the
//! allow/deny glob that decides which of them a bridge serves.
use protectorate_wire::{Catalog, Code, Method, Result, RpcError};

/// One unary RPC presented as an MCP tool.
#[derive(Clone, Debug)]
pub struct ToolDef {
    pub name: String,
    pub procedure: String,
    pub read_only: bool,
    pub destructive: bool,
    pub method: Method,
}

/// Every hub tool the catalog can offer, sorted by name. Node-local services,
/// owner-only services and methods, and any streaming RPC are never offered:
/// the bridge only speaks unary Connect JSON.
pub fn tool_defs(catalog: &Catalog) -> Result<Vec<ToolDef>> {
    let mut tools: Vec<ToolDef> = catalog
        .methods()
        .filter(|method| !method.node_local && !method.owner_only && method.is_unary())
        .map(|method| ToolDef {
            name: method.tool.clone(),
            procedure: method.procedure.clone(),
            read_only: method.read_only,
            destructive: method.destructive,
            method: method.clone(),
        })
        .collect();
    tools.sort_by(|a, b| a.name.cmp(&b.name));
    if let Some(pair) = tools.windows(2).find(|pair| pair[0].name == pair[1].name) {
        return Err(RpcError::new(
            Code::Internal,
            format!(
                "tool name {:?} is used by both {} and {}",
                pair[0].name, pair[0].procedure, pair[1].procedure
            ),
        ));
    }
    Ok(tools)
}

/// Selects which tools are served. With no `allow` patterns only read-only
/// tools pass; `allow` replaces that default with globs over tool names, and
/// `deny` always removes matches.
#[derive(Clone, Debug, Default)]
pub struct Filter {
    pub allow: Vec<String>,
    pub deny: Vec<String>,
}

impl Filter {
    pub fn validate(&self) -> Result<()> {
        for pattern in self.allow.iter().chain(self.deny.iter()) {
            tokenize(pattern).map_err(|reason| {
                RpcError::invalid(format!("bad pattern {:?}: {}", pattern, reason))
            })?;
        }
        Ok(())
    }

    pub fn permits(&self, tool: &ToolDef) -> Result<bool> {
        if any_match(&self.deny, &tool.name)? {
            return Ok(false);
        }
        if self.allow.is_empty() {
            return Ok(tool.read_only);
        }
        any_match(&self.allow, &tool.name)
    }
}

fn any_match(patterns: &[String], name: &str) -> Result<bool> {
    for pattern in patterns {
        let tokens = tokenize(pattern).map_err(|reason| {
            RpcError::invalid(format!("bad pattern {:?}: {}", pattern, reason))
        })?;
        if match_tokens(&tokens, &name.chars().collect::<Vec<char>>()) {
            return Ok(true);
        }
    }
    Ok(false)
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Token {
    Literal(char),
    Any,
    Star,
    Class { negated: bool, items: Vec<Item> },
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Item {
    Char(char),
    Range(char, char),
}

/// Globs in the shell sense the CLI already uses for token scopes: `*`, `?`,
/// `[abc]`, `[^abc]`, `[a-z]` and `\` escapes.
fn tokenize(pattern: &str) -> std::result::Result<Vec<Token>, String> {
    let chars: Vec<char> = pattern.chars().collect();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        match chars[index] {
            '*' => {
                tokens.push(Token::Star);
                index += 1;
            }
            '?' => {
                tokens.push(Token::Any);
                index += 1;
            }
            '[' => index = class_token(&chars, index, &mut tokens)?,
            '\\' => {
                let Some(escaped) = chars.get(index + 1) else {
                    return Err("syntax error: trailing backslash".to_owned());
                };
                tokens.push(Token::Literal(*escaped));
                index += 2;
            }
            c => {
                tokens.push(Token::Literal(c));
                index += 1;
            }
        }
    }
    Ok(tokens)
}

fn class_token(
    chars: &[char],
    start: usize,
    tokens: &mut Vec<Token>,
) -> std::result::Result<usize, String> {
    let mut index = start + 1;
    let negated = chars.get(index) == Some(&'^');
    if negated {
        index += 1;
    }
    let mut items = Vec::new();
    while index < chars.len() {
        if chars[index] == ']' && !items.is_empty() {
            tokens.push(Token::Class { negated, items });
            return Ok(index + 1);
        }
        let first = literal(chars, &mut index)?;
        if chars.get(index) == Some(&'-') && chars.get(index + 1).is_some_and(|c| *c != ']') {
            index += 1;
            let last = literal(chars, &mut index)?;
            if last < first {
                return Err(format!("syntax error: bad range {}-{}", first, last));
            }
            items.push(Item::Range(first, last));
        } else {
            items.push(Item::Char(first));
        }
    }
    Err("syntax error: missing closing ]".to_owned())
}

fn literal(chars: &[char], index: &mut usize) -> std::result::Result<char, String> {
    match chars.get(*index) {
        Some('\\') => {
            let Some(escaped) = chars.get(*index + 1) else {
                return Err("syntax error: trailing backslash".to_owned());
            };
            *index += 2;
            Ok(*escaped)
        }
        Some(c) => {
            *index += 1;
            Ok(*c)
        }
        None => Err("syntax error: truncated pattern".to_owned()),
    }
}

fn match_tokens(tokens: &[Token], text: &[char]) -> bool {
    match tokens.first() {
        None => text.is_empty(),
        Some(Token::Star) => (0..=text.len()).any(|skip| match_tokens(&tokens[1..], &text[skip..])),
        Some(token) => match text.first() {
            Some(first) if token_matches(token, *first) => match_tokens(&tokens[1..], &text[1..]),
            _ => false,
        },
    }
}

fn token_matches(token: &Token, c: char) -> bool {
    match token {
        Token::Literal(expected) => c == *expected,
        Token::Any => true,
        Token::Star => true,
        Token::Class { negated, items } => {
            items.iter().any(|item| match item {
                Item::Char(expected) => c == *expected,
                Item::Range(first, last) => c >= *first && c <= *last,
            }) != *negated
        }
    }
}
