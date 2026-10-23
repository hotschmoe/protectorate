//! The HTTP boundary. Readers talk to a `Transport`, so tests and callers can
//! substitute a fake; `UreqTransport` is the real one.
use crate::{Ctx, Error, Result, MAX_BODY};
use serde::de::DeserializeOwned;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct Request {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
    /// Wall-clock bound for this request, derived from the read deadline.
    pub timeout: Option<Duration>,
}

#[derive(Clone, Debug)]
pub struct Response {
    pub status: u16,
    pub body: Vec<u8>,
}

pub trait Transport: Send + Sync {
    fn send(&self, req: &Request) -> Result<Response>;
}

fn valid_url(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

/// Performs the request and returns the body on 2xx. 401 maps to
/// `Error::Unauthorized` and a 403 mentioning scope or permission to
/// `Error::Scope`.
pub fn do_raw(
    t: &dyn Transport,
    ctx: &Ctx,
    method: &str,
    url: &str,
    headers: Vec<(String, String)>,
    body: Option<Vec<u8>>,
) -> Result<Vec<u8>> {
    if ctx.expired() {
        return Err(Error::Deadline);
    }
    if !valid_url(url) {
        return Err(Error::Other("invalid request URL".to_string()));
    }
    let req = Request {
        method: method.to_string(),
        url: url.to_string(),
        headers,
        body,
        timeout: ctx.remaining(),
    };
    let resp = t.send(&req)?;
    let status = resp.status;
    let mut data = resp.body;
    data.truncate(MAX_BODY);
    match status {
        401 => Err(Error::Unauthorized),
        403 => {
            let l = String::from_utf8_lossy(&data).to_lowercase();
            if l.contains("scope") || l.contains("permission") {
                Err(Error::Scope)
            } else {
                Err(Error::Http(status))
            }
        }
        s if !(200..=299).contains(&s) => Err(Error::Http(status)),
        _ => Ok(data),
    }
}

pub fn get_json<T: DeserializeOwned>(
    t: &dyn Transport,
    ctx: &Ctx,
    url: &str,
    headers: Vec<(String, String)>,
) -> Result<T> {
    let data = do_raw(t, ctx, "GET", url, headers, None)?;
    serde_json::from_slice(&data).map_err(|e| Error::Other(format!("decode response: {e}")))
}

pub(crate) fn bearer_headers(
    token: &str,
    account_id: Option<&str>,
    user_agent: &str,
) -> Vec<(String, String)> {
    let mut h = vec![
        ("Authorization".to_string(), format!("Bearer {token}")),
        ("Accept".to_string(), "application/json".to_string()),
    ];
    if !user_agent.is_empty() {
        h.push(("User-Agent".to_string(), user_agent.to_string()));
    }
    if let Some(a) = account_id {
        if !a.is_empty() {
            h.push(("ChatGPT-Account-Id".to_string(), a.to_string()));
        }
    }
    h
}

/// The real transport: blocking HTTP/1.1 over rustls. One ureq agent per
/// request timeout so connection pooling works while each read keeps its own
/// wall-clock bound.
#[cfg(feature = "net")]
pub struct UreqTransport {
    agents: std::sync::Mutex<std::collections::HashMap<u64, ureq::Agent>>,
}

#[cfg(feature = "net")]
impl UreqTransport {
    pub fn new() -> Self {
        Self {
            agents: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    fn agent(&self, timeout: Option<Duration>) -> ureq::Agent {
        let key = timeout.map(|t| t.as_millis() as u64).unwrap_or(0);
        let mut agents = self.agents.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(a) = agents.get(&key) {
            return a.clone();
        }
        let mut builder = ureq::Agent::config_builder().http_status_as_error(false);
        if key > 0 {
            builder = builder.timeout_global(timeout);
        }
        let agent: ureq::Agent = builder.build().into();
        agents.insert(key, agent.clone());
        agent
    }
}

#[cfg(feature = "net")]
impl Default for UreqTransport {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "net")]
impl Transport for UreqTransport {
    fn send(&self, req: &Request) -> Result<Response> {
        let agent = self.agent(req.timeout);
        let resp = match req.method.as_str() {
            "POST" | "PUT" => {
                let mut builder = if req.method == "POST" {
                    agent.post(&req.url)
                } else {
                    agent.put(&req.url)
                };
                for (k, v) in &req.headers {
                    builder = builder.header(k, v.as_str());
                }
                builder.send(req.body.as_deref().unwrap_or(&[]))
            }
            "GET" => {
                let mut builder = agent.get(&req.url);
                for (k, v) in &req.headers {
                    builder = builder.header(k, v.as_str());
                }
                builder.call()
            }
            "DELETE" => {
                let mut builder = agent.delete(&req.url);
                for (k, v) in &req.headers {
                    builder = builder.header(k, v.as_str());
                }
                builder.call()
            }
            "HEAD" => {
                let mut builder = agent.head(&req.url);
                for (k, v) in &req.headers {
                    builder = builder.header(k, v.as_str());
                }
                builder.call()
            }
            m => return Err(Error::Other(format!("unsupported HTTP method {m}"))),
        };
        match resp {
            Ok(resp) => {
                let status = resp.status();
                let mut data = Vec::new();
                use std::io::Read;
                let mut body = resp.into_body();
                let reader = body.as_reader();
                reader
                    .take(MAX_BODY as u64)
                    .read_to_end(&mut data)
                    .map_err(|e| Error::Other(format!("read body: {e}")))?;
                Ok(Response {
                    status: status.into(),
                    body: data,
                })
            }
            Err(ureq::Error::Timeout(_)) => Err(Error::Deadline),
            Err(ureq::Error::BadUri(m)) => Err(Error::Other(format!("invalid request URL: {m}"))),
            Err(e) => Err(Error::Other(format!("request failed: {e}"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::{ctx, Fake};
    use crate::Error;

    #[test]
    fn status_codes_map_to_errors() {
        let now = ctx("2026-01-01T00:00:00Z");
        let t = Fake::new()
            .route("GET", "https://x.invalid/a", 204, "")
            .route("GET", "https://x.invalid/401", 401, "nope")
            .route(
                "GET",
                "https://x.invalid/403",
                403,
                "insufficient PERMISSIONS",
            )
            .route("GET", "https://x.invalid/403x", 403, "blocked")
            .route("GET", "https://x.invalid/302", 302, "moved")
            .route("GET", "https://x.invalid/500", 500, "boom");
        assert_eq!(
            do_raw(&t, &now, "GET", "https://x.invalid/a", vec![], None).unwrap(),
            Vec::<u8>::new()
        );
        assert_eq!(
            do_raw(&t, &now, "GET", "https://x.invalid/401", vec![], None).unwrap_err(),
            Error::Unauthorized
        );
        assert_eq!(
            do_raw(&t, &now, "GET", "https://x.invalid/403", vec![], None).unwrap_err(),
            Error::Scope
        );
        assert_eq!(
            do_raw(&t, &now, "GET", "https://x.invalid/403x", vec![], None).unwrap_err(),
            Error::Http(403)
        );
        assert_eq!(
            do_raw(&t, &now, "GET", "https://x.invalid/302", vec![], None).unwrap_err(),
            Error::Http(302)
        );
        assert_eq!(
            do_raw(&t, &now, "GET", "https://x.invalid/500", vec![], None).unwrap_err(),
            Error::Http(500)
        );
    }

    #[test]
    fn bodies_are_capped_and_deadlines_stop_requests() {
        let big = "x".repeat(MAX_BODY + 4096);
        let t = Fake::new().route("GET", "https://x.invalid/big", 200, &big);
        let now = ctx("2026-01-01T00:00:00Z");
        assert_eq!(
            do_raw(&t, &now, "GET", "https://x.invalid/big", vec![], None)
                .unwrap()
                .len(),
            MAX_BODY
        );

        let mut past = now;
        past.now = crate::testkit::time("2026-01-01T00:00:46Z");
        past.deadline = Some(crate::testkit::time("2026-01-01T00:00:45Z"));
        assert_eq!(
            do_raw(&t, &past, "GET", "https://x.invalid/big", vec![], None).unwrap_err(),
            Error::Deadline
        );
        assert_eq!(
            t.requests().len(),
            1,
            "the expired read never reached the transport"
        );
    }

    #[test]
    fn urls_are_validated_and_json_is_decoded() {
        let now = ctx("2026-01-01T00:00:00Z");
        let t = Fake::new()
            .route("GET", "https://x.invalid/ok", 200, r#"{"n":7}"#)
            .route("GET", "https://x.invalid/bad", 200, "not json");
        let v: serde_json::Value = get_json(&t, &now, "https://x.invalid/ok", vec![]).unwrap();
        assert_eq!(v["n"], 7);
        let err =
            get_json::<serde_json::Value>(&t, &now, "https://x.invalid/bad", vec![]).unwrap_err();
        assert!(err.to_string().starts_with("decode response: "));
        assert_eq!(
            do_raw(&t, &now, "GET", "/relative-only", vec![], None).unwrap_err(),
            Error::Other("invalid request URL".to_string())
        );
    }

    #[test]
    fn bearer_headers_carry_the_optional_account() {
        let h = bearer_headers("tok", Some("acct"), "protectorate");
        assert!(h.contains(&("ChatGPT-Account-Id".to_string(), "acct".to_string())));
        assert!(h.contains(&("User-Agent".to_string(), "protectorate".to_string())));
        let h = bearer_headers("tok", None, "");
        assert!(!h
            .iter()
            .any(|(k, _)| k == "ChatGPT-Account-Id" || k == "User-Agent"));
    }
}
