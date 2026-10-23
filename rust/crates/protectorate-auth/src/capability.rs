//! Capabilities are HMAC-SHA256 macaroons only the hub can mint or verify.
//! The hub signs the identifier and chains every caveat into the tag, so a
//! holder may append caveats to narrow a token but cannot remove one.
//!
//! `pc1.<base64url lines>.<base64url tag>`; the lines are the identifier,
//! then `principal=`, `scopes=`, an optional `task=`, `iat=` and `exp=`, then
//! any number of holder caveats, each `scopes=` or `exp=`.
use base64::{engine::general_purpose::URL_SAFE_NO_PAD as B64, Engine};
use ring::hmac;

pub const PREFIX: &str = "pc1.";
const MAX_TOKEN: usize = 4096;
const MAX_CAVEATS: usize = 32;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Malformed,
    Signature,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Malformed => "malformed capability",
            Self::Signature => "capability signature mismatch",
        })
    }
}

impl std::error::Error for Error {}

/// A caveat a holder may append.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Caveat {
    Scopes(Vec<String>),
    Expires(i64),
}

impl Caveat {
    fn line(&self) -> String {
        match self {
            Self::Scopes(scopes) => format!("scopes={}", scopes.join(" ")),
            Self::Expires(at) => format!("exp={at}"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Claims {
    pub id: String,
    pub principal: String,
    pub scopes: Vec<String>,
    pub task: String,
    pub issued: i64,
    /// The earliest expiry among every `exp` caveat.
    pub expires: i64,
    /// Scope sets appended by holders; a call must pass each of them.
    pub narrowed: Vec<Vec<String>>,
    holder: Vec<Caveat>,
}

fn field_ok(value: &str) -> bool {
    !value.is_empty() && value.len() <= 500 && !value.chars().any(char::is_control)
}

fn scopes_ok(scopes: &[String]) -> bool {
    scopes.iter().all(|s| field_ok(s) && !s.contains(' '))
}

fn tag(key: &[u8], lines: &[String]) -> hmac::Tag {
    let mut tag = hmac::sign(&hmac::Key::new(hmac::HMAC_SHA256, key), lines[0].as_bytes());
    for line in &lines[1..] {
        tag = hmac::sign(
            &hmac::Key::new(hmac::HMAC_SHA256, tag.as_ref()),
            line.as_bytes(),
        );
    }
    tag
}

fn encode(lines: &[String], tag: &[u8]) -> String {
    format!(
        "{PREFIX}{}.{}",
        B64.encode(lines.join("\n")),
        B64.encode(tag)
    )
}

fn split(token: &str) -> Result<(Vec<String>, Vec<u8>), Error> {
    if token.len() > MAX_TOKEN {
        return Err(Error::Malformed);
    }
    let rest = token.strip_prefix(PREFIX).ok_or(Error::Malformed)?;
    let (body, sig) = rest.split_once('.').ok_or(Error::Malformed)?;
    let body = B64.decode(body).map_err(|_| Error::Malformed)?;
    let sig = B64.decode(sig).map_err(|_| Error::Malformed)?;
    let body = String::from_utf8(body).map_err(|_| Error::Malformed)?;
    let lines: Vec<String> = body.split('\n').map(str::to_owned).collect();
    if sig.len() != 32 || lines.len() < 5 || lines.len() > 5 + MAX_CAVEATS {
        return Err(Error::Malformed);
    }
    Ok((lines, sig))
}

fn parse(lines: &[String]) -> Result<Claims, Error> {
    let bad = || Error::Malformed;
    let id = &lines[0];
    if id.len() != 32 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(bad());
    }
    let mut rest = lines[1..]
        .iter()
        .map(|line| line.split_once('=').ok_or_else(bad));
    let mut next = |key: &str| match rest.next() {
        Some(Ok((k, v))) if k == key => Ok(v.to_owned()),
        _ => Err(bad()),
    };
    let principal = next("principal")?;
    let scopes = words(&next("scopes")?);
    let after = &lines[3..];
    let (task, base) = match after.first().and_then(|l| l.strip_prefix("task=")) {
        Some(task) => (task.to_owned(), 4),
        None => (String::new(), 3),
    };
    let number = |line: Option<&String>, key: &str| {
        line.and_then(|l| l.strip_prefix(key))
            .and_then(|v| v.parse::<i64>().ok())
            .ok_or_else(bad)
    };
    let issued = number(lines.get(base), "iat=")?;
    let mut expires = number(lines.get(base + 1), "exp=")?;
    if !field_ok(&principal) || !scopes_ok(&scopes) || (base == 4 && !field_ok(&task)) {
        return Err(bad());
    }
    let mut narrowed = Vec::new();
    let mut holder = Vec::new();
    for line in &lines[base + 2..] {
        if let Some(value) = line.strip_prefix("scopes=") {
            let set = words(value);
            if !scopes_ok(&set) {
                return Err(bad());
            }
            narrowed.push(set.clone());
            holder.push(Caveat::Scopes(set));
        } else if let Some(value) = line.strip_prefix("exp=") {
            let at = value.parse::<i64>().map_err(|_| bad())?;
            expires = expires.min(at);
            holder.push(Caveat::Expires(at));
        } else {
            return Err(bad());
        }
    }
    Ok(Claims {
        id: id.clone(),
        principal,
        scopes,
        task,
        issued,
        expires,
        narrowed,
        holder,
    })
}

fn words(value: &str) -> Vec<String> {
    value
        .split(' ')
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

fn base_lines(claims: &Claims, issued: i64, expires: i64) -> Vec<String> {
    let mut lines = vec![
        claims.id.clone(),
        format!("principal={}", claims.principal),
        format!("scopes={}", claims.scopes.join(" ")),
    ];
    if !claims.task.is_empty() {
        lines.push(format!("task={}", claims.task));
    }
    lines.push(format!("iat={issued}"));
    lines.push(format!("exp={expires}"));
    lines
}

/// A new capability. `id` is 32 hex characters chosen by the hub.
pub fn mint(
    key: &[u8],
    id: &str,
    principal: &str,
    scopes: &[String],
    task: &str,
    issued: i64,
    expires: i64,
) -> Result<String, Error> {
    let claims = Claims {
        id: id.into(),
        principal: principal.into(),
        scopes: scopes.to_vec(),
        task: task.into(),
        issued,
        expires,
        narrowed: Vec::new(),
        holder: Vec::new(),
    };
    reissue(key, &claims, issued, expires)
}

/// The same capability, holder caveats included, with a new lifetime.
pub fn reissue(key: &[u8], claims: &Claims, issued: i64, expires: i64) -> Result<String, Error> {
    let mut lines = base_lines(claims, issued, expires);
    lines.extend(claims.holder.iter().map(Caveat::line));
    let encoded = encode(&lines, tag(key, &lines).as_ref());
    // A field that does not survive its own parse would mint an unusable token.
    match split(&encoded).and_then(|(lines, _)| parse(&lines)) {
        Ok(_) => Ok(encoded),
        Err(error) => Err(error),
    }
}

/// Narrow a capability offline. Only the hub can check the result.
pub fn attenuate(token: &str, caveat: &Caveat) -> Result<String, Error> {
    let (mut lines, sig) = split(token)?;
    parse(&lines)?;
    if lines.len() >= 5 + MAX_CAVEATS {
        return Err(Error::Malformed);
    }
    let line = caveat.line();
    let tag = hmac::sign(&hmac::Key::new(hmac::HMAC_SHA256, &sig), line.as_bytes());
    lines.push(line);
    let encoded = encode(&lines, tag.as_ref());
    split(&encoded).and_then(|(lines, _)| parse(&lines))?;
    Ok(encoded)
}

/// Claims without checking the tag, for a holder deciding when to renew.
pub fn decode(token: &str) -> Result<Claims, Error> {
    parse(&split(token)?.0)
}

pub fn verify(key: &[u8], token: &str) -> Result<Claims, Error> {
    let (lines, sig) = split(token)?;
    let claims = parse(&lines)?;
    let (last, chain) = lines.split_last().ok_or(Error::Malformed)?;
    let parent = tag(key, chain);
    hmac::verify(
        &hmac::Key::new(hmac::HMAC_SHA256, parent.as_ref()),
        last.as_bytes(),
        &sig,
    )
    .map_err(|_| Error::Signature)?;
    Ok(claims)
}
