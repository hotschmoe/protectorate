//! The local model: an OpenAI-compatible endpoint. It has no quota; the reading is
//! its health (reachable) and the models it serves.
//!
//! The endpoint wants the API key Pi already holds for it: the `apiKey` of the provider named
//! `provider` in `~/.pi/agent/models.json`, read only, sent as a bearer token through curl's
//! stdin config, never printed. That provider's `baseUrl` is the endpoint when `base_url` is
//! empty.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use pt_api::Subscription;
use pt_json::Value;

use crate::http::{self, Get};
use crate::{Error, Reader, Result, Snapshot, home_dir, read_login};

#[derive(Clone, Debug, Default)]
pub struct Local {
    /// Base URL of the endpoint, with or without `/v1`. Empty means the Pi provider's `baseUrl`.
    pub base_url: String,
    /// Where `.pi/agent/models.json` is looked up; `$HOME` when `None`.
    pub home: Option<PathBuf>,
    /// The provider in Pi's models.json that points at this endpoint.
    pub provider: String,
}

/// What Pi's models.json says about one provider.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Provider {
    pub base_url: String,
    pub api_key: Option<String>,
}

pub fn models_url(base: &str) -> String {
    let u = base.trim_end_matches('/');
    if u.ends_with("/v1") {
        format!("{u}/models")
    } else {
        format!("{u}/v1/models")
    }
}

/// The provider's entry from a models.json body. `apiKey` follows Pi's rules: the name of a set
/// environment variable, else the literal key; a `!command` key is never run here.
pub fn parse_provider(body: &[u8], name: &str) -> Result<Option<Provider>> {
    let v =
        pt_json::parse_bytes(body).map_err(|e| Error::other(format!("decode models.json: {e}")))?;
    let Some(p) = v.get("providers").and_then(|ps| ps.get(name)) else {
        return Ok(None);
    };
    let key = match p.get("apiKey").and_then(Value::as_str).unwrap_or("") {
        "" => None,
        k if k.starts_with('!') => {
            return Err(Error::unavailable(format!(
                "Pi provider {name:?} takes its key from a command, which the quota reader does not run"
            )));
        }
        k => Some(
            std::env::var(k)
                .ok()
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| k.to_string()),
        ),
    };
    Ok(Some(Provider {
        base_url: p
            .get("baseUrl")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        api_key: key,
    }))
}

/// Model ids of a `/v1/models` response.
pub fn parse_models(body: &[u8]) -> Result<Vec<String>> {
    let v = pt_json::parse_bytes(body).map_err(|e| Error::other(format!("decode models: {e}")))?;
    let data = v
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::other("models response has no data array"))?;
    Ok(data
        .iter()
        .filter_map(|m| m.get("id").and_then(Value::as_str))
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .collect())
}

impl Local {
    /// Pi's entry for `provider`, or `None` when there is no models.json or no such provider.
    fn provider(&self) -> Result<Option<Provider>> {
        if self.provider.is_empty() {
            return Ok(None);
        }
        let path = home_dir(self.home.as_deref())?.join(".pi/agent/models.json");
        match read_login(&path) {
            Ok(body) => parse_provider(&body, &self.provider),
            Err(Error::Unavailable(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

impl Reader for Local {
    fn subscription(&self) -> Subscription {
        Subscription::Local
    }

    fn read(&self, _now_ms: u64, timeout: Duration) -> Result<Snapshot> {
        let provider = self.provider()?.unwrap_or_default();
        let base = if self.base_url.is_empty() {
            provider.base_url.as_str()
        } else {
            self.base_url.as_str()
        };
        if base.is_empty() {
            return Err(Error::unavailable(format!(
                "no local endpoint configured (local_url, or a baseUrl for Pi provider {:?})",
                self.provider
            )));
        }
        let mut headers = vec![("Accept", "application/json".to_string())];
        if let Some(k) = &provider.api_key {
            headers.push(("Authorization", format!("Bearer {k}")));
        }
        let started = Instant::now();
        let body = http::get(&Get {
            url: &models_url(base),
            headers: &headers,
            timeout,
        })?;
        let latency = started.elapsed().as_millis();
        let models = parse_models(&body)?;
        Ok(Snapshot {
            plan: String::new(),
            windows: Vec::new(),
            note: format!("reachable in {latency} ms; models: {}", models.join(", ")),
        })
    }
}
