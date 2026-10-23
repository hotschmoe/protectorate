//! Local model servers. An OpenAI-compatible endpoint has no quota; it reports
//! reachability, served model ids and round-trip latency.
use crate::http::get_json;
use crate::{Ctx, Error, Reader, Result, Snapshot, Transport};
use serde::Deserialize;
use std::time::Instant;

#[derive(Clone, Debug)]
pub struct Local {
    pub base_url: String,
}

impl Local {
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.to_string(),
        }
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
        let headers = vec![("Accept".to_string(), "application/json".to_string())];
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
