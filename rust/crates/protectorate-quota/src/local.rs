//! Local model servers. An OpenAI-compatible endpoint has no quota; it reports
//! reachability, served model ids and round-trip latency.
use crate::http::get_json;
use crate::{Ctx, Error, Reader, Result, Snapshot, Transport};
use serde::Deserialize;
use std::time::Instant;

#[derive(Clone, Debug)]
pub struct Local {
    pub base_url: String,
    pub api_key: String,
}

impl Local {
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.to_string(),
            api_key: String::new(),
        }
    }
    pub fn with_api_key(mut self, key: &str) -> Self {
        self.api_key = key.to_string();
        self
    }

    pub fn models_url(&self) -> String {
        let u = self.base_url.trim_end_matches('/');
        if u.ends_with("/v1") {
            format!("{u}/models")
        } else {
            format!("{u}/v1/models")
        }
    }
}

impl Reader for Local {
    fn provider(&self) -> &str {
        "local"
    }

    fn read(&self, ctx: &Ctx, http: &dyn Transport) -> Result<Snapshot> {
        let mut snap = Snapshot {
            provider: "local".to_string(),
            source: "openai-compat".to_string(),
            ..Snapshot::default()
        };
        if self.base_url.is_empty() {
            return Err(Error::unavailable("no local endpoint configured"));
        }
        let mut headers = vec![("Accept".to_string(), "application/json".to_string())];
        if !self.api_key.is_empty() {
            headers.push((
                "Authorization".to_string(),
                format!("Bearer {}", self.api_key),
            ));
        }
        #[derive(Deserialize)]
        struct Models {
            #[serde(default)]
            data: Vec<ModelEntry>,
        }
        #[derive(Deserialize)]
        struct ModelEntry {
            #[serde(default)]
            id: String,
        }
        let start = Instant::now();
        let res = get_json::<Models>(http, ctx, &self.models_url(), headers);
        snap.latency = Some(start.elapsed().as_millis() as u64);
        snap.fetched_at = Some(ctx.now);
        let out = res?;
        for m in out.data {
            if !m.id.is_empty() {
                snap.models.push(m.id);
            }
        }
        Ok(snap)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::{ctx, Fake};

    #[test]
    fn models_urls_follow_the_openai_convention() {
        assert_eq!(
            Local::new("http://127.0.0.1:1234").models_url(),
            "http://127.0.0.1:1234/v1/models"
        );
        assert_eq!(
            Local::new("http://127.0.0.1:1234/").models_url(),
            "http://127.0.0.1:1234/v1/models"
        );
        assert_eq!(
            Local::new("http://127.0.0.1:1234/v1").models_url(),
            "http://127.0.0.1:1234/v1/models"
        );
    }

    #[test]
    fn reachability_reports_models_and_latency() {
        let http = Fake::new().route(
            "GET",
            "http://127.0.0.1:1234/v1/models",
            200,
            r#"{"data":[{"id":"pi"},{"id":"llama3"},{"id":""}]}"#,
        );
        let l = Local::new("http://127.0.0.1:1234").with_api_key("k");
        let s = l.read(&ctx("2026-01-01T00:00:00Z"), &http).unwrap();
        assert_eq!(s.provider, "local");
        assert_eq!(s.source, "openai-compat");
        assert_eq!(s.models, ["pi", "llama3"]);
        assert!(s.latency.is_some());
        assert_eq!(
            s.fetched_at,
            Some(crate::testkit::time("2026-01-01T00:00:00Z"))
        );
        assert!(http.requests()[0]
            .2
            .contains(&("Authorization".to_string(), "Bearer k".to_string())));
    }

    #[test]
    fn an_unreachable_endpoint_is_an_error() {
        let http = Fake::new().route("GET", "http://127.0.0.1:1234/v1/models", 503, "down");
        let err = Local::new("http://127.0.0.1:1234")
            .read(&ctx("2026-01-01T00:00:00Z"), &http)
            .unwrap_err();
        assert_eq!(err, Error::Http(503));

        let err = Local::new("")
            .read(&ctx("2026-01-01T00:00:00Z"), &Fake::new())
            .unwrap_err();
        assert_eq!(
            err.to_string(),
            "source unavailable: no local endpoint configured"
        );
    }
}
