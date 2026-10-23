//! Bounded HTTP GET for release metadata and assets.
//!
//! Redirects are followed by this module instead of ureq so that every hop is
//! checked: at most ten hops, and HTTPS unless the release API itself is
//! plain HTTP.
use crate::{invalid, Error, Result};
use std::io::Read;
use std::time::Duration;

pub const CHECK_TIMEOUT: Duration = Duration::from_secs(10);
pub const FETCH_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const MAX_REDIRECTS: u32 = 10;
const USER_AGENT: &str = "protectorate-release";
const ACCEPT: &str = "application/vnd.github+json";

/// Which wall-clock budget a request gets: the short one for metadata checks,
/// the long one for asset downloads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Budget {
    Check,
    Fetch,
}

#[derive(Clone, Debug)]
pub struct Got {
    pub body: Vec<u8>,
    pub etag: String,
    pub not_modified: bool,
}

pub struct Transport {
    allow_http: bool,
    check: ureq::Agent,
    fetch: ureq::Agent,
}

impl std::fmt::Debug for Transport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Transport")
            .field("allow_http", &self.allow_http)
            .finish_non_exhaustive()
    }
}

impl Transport {
    pub fn new(api: &str) -> Transport {
        Transport {
            allow_http: api.starts_with("http://"),
            check: agent(CHECK_TIMEOUT),
            fetch: agent(FETCH_TIMEOUT),
        }
    }

    pub fn get(&self, raw: &str, etag: &str, budget: Budget, max_bytes: u64) -> Result<Got> {
        let agent = match budget {
            Budget::Check => &self.check,
            Budget::Fetch => &self.fetch,
        };
        let mut url = raw.to_string();
        let mut hops = 0u32;
        loop {
            validate_url(&url, self.allow_http)?;
            let mut request = agent
                .get(&url)
                .header("Accept", ACCEPT)
                .header("User-Agent", USER_AGENT);
            if !etag.is_empty() {
                request = request.header("If-None-Match", etag);
            }
            let response = match request.call() {
                Ok(response) => response,
                Err(ureq::Error::Timeout(_)) => {
                    return Err(Error::Http(format!("fetch {raw}: timed out")))
                }
                Err(ureq::Error::BadUri(e)) => {
                    return Err(Error::Http(format!("fetch {raw}: invalid URL {e}")))
                }
                Err(e) => return Err(Error::Http(format!("fetch {raw}: {e}"))),
            };
            let status = response.status().as_u16();
            if status == 304 {
                return Ok(Got {
                    body: Vec::new(),
                    etag: etag.to_string(),
                    not_modified: true,
                });
            }
            if (300..400).contains(&status) {
                hops += 1;
                if hops > MAX_REDIRECTS {
                    return Err(Error::Http(
                        "release request exceeded ten redirects".to_string(),
                    ));
                }
                let location = response
                    .headers()
                    .get("location")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("");
                url = resolve_url(&url, location)
                    .map_err(|e| Error::Http(format!("fetch {raw}: {e}")))?;
                continue;
            }
            if status != 200 {
                return Err(Error::Http(format!("fetch {raw}: HTTP {status}")));
            }
            let new_etag = response
                .headers()
                .get("etag")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_string();
            let mut body = Vec::new();
            let reader = response.into_body().into_reader();
            reader
                .take(max_bytes + 1)
                .read_to_end(&mut body)
                .map_err(|e| Error::Http(format!("read {raw}: {e}")))?;
            if body.len() as u64 > max_bytes {
                return Err(Error::Http(format!("asset exceeds {max_bytes} bytes")));
            }
            return Ok(Got {
                body,
                etag: new_etag,
                not_modified: false,
            });
        }
    }
}

fn agent(timeout: Duration) -> ureq::Agent {
    let config = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .max_redirects(0)
        .max_redirects_will_error(false)
        .timeout_global(Some(timeout))
        .build();
    config.into()
}

pub fn validate_url(raw: &str, allow_http: bool) -> Result<()> {
    let bad = || invalid(format!("invalid asset URL {raw:?}"));
    let (scheme, rest) = raw.split_once("://").ok_or_else(bad)?;
    if scheme != "http" && scheme != "https" {
        return Err(bad());
    }
    if scheme == "http" && !allow_http {
        return Err(bad());
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = authority.rsplit('@').next().unwrap_or("");
    if host.is_empty()
        || !host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ':' | '%'))
    {
        return Err(bad());
    }
    Ok(())
}

pub fn resolve_url(base: &str, location: &str) -> Result<String> {
    let location = location.split('#').next().unwrap_or("").trim();
    if location.is_empty() {
        return Err(invalid("redirect has no location"));
    }
    if location.starts_with("://") || location.contains("://") {
        return Ok(location.to_string());
    }
    let (scheme, rest) = base
        .split_once("://")
        .ok_or_else(|| invalid("invalid release URL"))?;
    if location.starts_with("//") {
        return Ok(format!("{scheme}:{location}"));
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let path = &rest[authority.len()..];
    let path = path.split(['?', '#']).next().unwrap_or(path);
    let target = if location.starts_with('/') {
        location.to_string()
    } else {
        let dir = match path.rfind('/') {
            Some(i) => &path[..i + 1],
            None => "/",
        };
        format!("{dir}{location}")
    };
    Ok(format!("{scheme}://{authority}{}", normalize_path(&target)))
}

fn normalize_path(path: &str) -> String {
    let (path, query) = match path.split_once('?') {
        Some((path, query)) => (path, Some(query)),
        None => (path, None),
    };
    let absolute = path.starts_with('/');
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            parts.pop();
            continue;
        }
        parts.push(part);
    }
    let mut out = String::new();
    if absolute {
        out.push('/');
    }
    out.push_str(&parts.join("/"));
    if let Some(query) = query {
        out.push('?');
        out.push_str(query);
    }
    out
}
