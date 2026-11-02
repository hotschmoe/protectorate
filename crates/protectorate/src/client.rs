use std::path::PathBuf;
use std::time::Duration;

use pt_api::{ErrorBody, Method};
use pt_json::{FromJson, ToJson, Value};

use crate::fail::Fail;

/// The hub over its unix socket.
pub struct Client {
    pub socket: PathBuf,
    pub token: Option<String>,
}

impl Client {
    fn raw(&self, method: &str, body: &Value, wait: Duration) -> Result<Value, Fail> {
        let client =
            pt_http::Client::unix(&self.socket).read_timeout(Some(wait + Duration::from_secs(30)));
        let auth = self.token.as_ref().map(|t| format!("Bearer {t}"));
        let mut headers = vec![("Content-Type", "application/json")];
        if let Some(a) = &auth {
            headers.push(("Authorization", a.as_str()));
        }
        let path = format!("{}{method}", pt_api::API_PREFIX);
        let r = client
            .request("POST", &path, &headers, body.to_string().as_bytes())
            .map_err(|e| Fail::Other(format!("{}: {e}", self.socket.display())))?;
        let v = pt_json::parse_bytes(&r.body).map_err(|e| {
            Fail::Other(format!(
                "{method}: hub answered {} with bad JSON: {e}",
                r.status
            ))
        })?;
        if r.status != 200 {
            return Err(match ErrorBody::from_json(&v) {
                Ok(b) => Fail::Api(b.error),
                Err(_) => Fail::Other(format!("{method}: hub answered {}: {v}", r.status)),
            });
        }
        Ok(v)
    }

    /// Calls `M`, allowing the hub `wait` before the answer (Wait, EnvoyWait, D9 approvals).
    pub fn call_for<M: Method>(
        &self,
        req: &M::Request,
        wait: Duration,
    ) -> Result<M::Response, Fail> {
        let v = self.raw(M::NAME, &req.to_json(), wait)?;
        M::Response::from_json(&v)
            .map_err(|e| Fail::Other(format!("{}: the hub's answer does not parse: {e}", M::NAME)))
    }

    pub fn call<M: Method>(&self, req: &M::Request) -> Result<M::Response, Fail> {
        self.call_for::<M>(req, Duration::from_secs(30))
    }
}
