//! The local model: an OpenAI-compatible endpoint (the b70 box). It has no quota; the reading is
//! its health (reachable) and the models it serves.

use std::time::{Duration, Instant};

use pt_api::Subscription;
use pt_json::Value;

use crate::http::{self, Get};
use crate::{Error, Reader, Result, Snapshot};

#[derive(Clone, Debug, Default)]
pub struct Local {
    /// Base URL of the endpoint, with or without `/v1`. Empty means not configured.
    pub base_url: String,
}

impl Local {
    pub fn models_url(&self) -> String {
        let u = self.base_url.trim_end_matches('/');
        if u.ends_with("/v1") {
            format!("{u}/models")
        } else {
            format!("{u}/v1/models")
        }
    }
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

impl Reader for Local {
    fn subscription(&self) -> Subscription {
        Subscription::Local
    }

    fn read(&self, _now_ms: u64, timeout: Duration) -> Result<Snapshot> {
        if self.base_url.is_empty() {
            return Err(Error::unavailable("no local endpoint configured"));
        }
        let headers = [("Accept", "application/json".to_string())];
        let started = Instant::now();
        let body = http::get(&Get {
            url: &self.models_url(),
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
