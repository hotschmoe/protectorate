//! Blocking Connect JSON transport for the Rust CLI and MCP bridge.
//! Callers in an async runtime should use `spawn_blocking` for these methods.
use protectorate_auth::capability;
use protectorate_wire::{
    decode_json, encode, encode_envelope, encode_json, Catalog, Code, DynamicMessage, Method,
    ReflectMessage, Result, Rpc, RpcError, FLAG_END_STREAM,
};
use serde_json::Value;
use std::{
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};
use zeroize::Zeroizing;

const MAX_UNARY_BYTES: u64 = 16 << 20;
const MAX_STREAM_MESSAGE_BYTES: usize = 8 << 20;
const MAX_ERROR_BYTES: u64 = 64 << 10;
/// How often a call the hub holds for owner approval is repeated.
const APPROVAL_POLL: Duration = Duration::from_secs(2);
const RENEW_TOKEN: &str = "/protectorate.v1.TokenService/RenewToken";

/// How a client proves who it is on each request.
#[derive(Clone)]
pub enum Credential {
    /// No Authorization header; the hub refuses every call.
    None,
    /// The owner token (loopback only), a device token or a capability held
    /// in memory.
    Bearer(Zeroizing<String>),
    /// A token file, read for every request. A capability past half its
    /// lifetime is renewed through the hub and the file rewritten in place;
    /// any other token is used as it is.
    TokenFile(PathBuf),
}

#[derive(Clone)]
pub struct Client {
    base: String,
    credential: Credential,
    agent: ureq::Agent,
    catalog: Catalog,
}

impl Client {
    /// Creates a client with the same endpoint, credentials and TLS policy,
    /// but its own connection pool. Shutting this client down interrupts only
    /// calls made through this pool.
    pub fn isolated(&self) -> Self {
        Self {
            base: self.base.clone(),
            credential: self.credential.clone(),
            agent: self.agent.config().clone().into(),
            catalog: self.catalog.clone(),
        }
    }

    /// Interrupts active socket I/O and refuses further calls on this pool.
    pub fn shutdown(self) {
        self.agent.shutdown();
    }

    /// HTTPS trusts the public web PKI roots.
    pub fn new(base: &str, token: impl Into<String>, timeout: Duration) -> Result<Self> {
        let base = base.trim_end_matches('/');
        if !valid_endpoint(base) {
            return Err(RpcError::invalid(
                "client endpoint must be HTTPS or loopback HTTP",
            ));
        }
        let token = Zeroizing::new(token.into());
        if token.contains(['\r', '\n']) {
            return Err(RpcError::invalid("invalid bearer token"));
        }
        let config = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .max_redirects(0)
            .max_redirects_will_error(false)
            .timeout_global(Some(timeout));
        Ok(Self {
            base: base.into(),
            credential: if token.is_empty() {
                Credential::None
            } else {
                Credential::Bearer(token)
            },
            agent: config.build().into(),
            catalog: Catalog::embedded()?,
        })
    }

    /// The same endpoint with another credential.
    pub fn with_credential(mut self, credential: Credential) -> Self {
        self.credential = credential;
        self
    }

    /// Calls a unary RPC with generated request and response types.
    pub fn unary<Req: ReflectMessage, Resp: ReflectMessage + Default>(
        &self,
        rpc: Rpc<Req, Resp>,
        request: &Req,
    ) -> Result<Resp> {
        self.typed(rpc, request, None)
    }

    pub fn unary_with_timeout<Req: ReflectMessage, Resp: ReflectMessage + Default>(
        &self,
        rpc: Rpc<Req, Resp>,
        request: &Req,
        timeout: Duration,
    ) -> Result<Resp> {
        self.typed(rpc, request, Some(timeout))
    }

    /// Calls a server-streaming RPC with generated types; see `stream`.
    pub fn server_stream<Req: ReflectMessage, Resp: ReflectMessage + Default>(
        &self,
        rpc: Rpc<Req, Resp>,
        request: &Req,
        mut on_message: impl FnMut(Resp) -> Result<()>,
    ) -> Result<()> {
        let body = bounded(encode(request)?)?;
        self.stream_inner(rpc.procedure, &body, None, &mut |message| {
            on_message(transcode(&message)?)
        })
    }

    pub fn server_stream_with_timeout<Req: ReflectMessage, Resp: ReflectMessage + Default>(
        &self,
        rpc: Rpc<Req, Resp>,
        request: &Req,
        timeout: Duration,
        mut on_message: impl FnMut(Resp) -> Result<()>,
    ) -> Result<()> {
        let body = bounded(encode(request)?)?;
        self.stream_inner(rpc.procedure, &body, Some(timeout), &mut |message| {
            on_message(transcode(&message)?)
        })
    }

    fn typed<Req: ReflectMessage, Resp: ReflectMessage + Default>(
        &self,
        rpc: Rpc<Req, Resp>,
        request: &Req,
        timeout: Option<Duration>,
    ) -> Result<Resp> {
        let body = bounded(encode(request)?)?;
        transcode(&self.unary_inner(rpc.procedure, &body, timeout)?)
    }

    /// Calls a unary RPC with protobuf JSON values, for generic callers such
    /// as the MCP bridge and black-box tests.
    pub fn call(&self, procedure: &str, input: &Value) -> Result<Value> {
        self.call_json(procedure, input, None)
    }

    fn call_json(
        &self,
        procedure: &str,
        input: &Value,
        timeout: Option<Duration>,
    ) -> Result<Value> {
        let body = self.input(self.method(procedure)?, input)?;
        to_value(&self.unary_inner(procedure, &body, timeout)?)
    }

    fn unary_inner(
        &self,
        procedure: &str,
        body: &[u8],
        timeout: Option<Duration>,
    ) -> Result<DynamicMessage> {
        let method = self.method(procedure)?;
        if !method.is_unary() {
            return Err(RpcError::invalid("procedure is streaming"));
        }
        let mut approval = String::new();
        loop {
            let response =
                self.post_with_timeout(procedure, "application/json", body, timeout, &approval)?;
            match self.unary_response(method, response) {
                Err(error) => match pending_approval(&error) {
                    Some(id) => {
                        if approval.is_empty() {
                            eprintln!("waiting for owner approval {id} of {procedure}");
                        }
                        approval = id;
                        std::thread::sleep(APPROVAL_POLL);
                    }
                    None => return Err(error),
                },
                result => return result,
            }
        }
    }

    fn unary_response(
        &self,
        method: &Method,
        response: ureq::http::Response<ureq::Body>,
    ) -> Result<DynamicMessage> {
        let status = response.status().as_u16();
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_ascii_lowercase();
        let bytes = read_bounded(
            response.into_body().into_reader(),
            if status == 200 {
                MAX_UNARY_BYTES
            } else {
                MAX_ERROR_BYTES
            },
        )?;
        if status != 200 {
            return Err(remote_error(status, &bytes));
        }
        if !content_type.starts_with("application/json") {
            return Err(RpcError::new(
                Code::DataLoss,
                "unexpected unary content type",
            ));
        }
        decode_json(method.output(), &bytes).map_err(|e| RpcError::new(Code::DataLoss, e.message))
    }

    /// Calls a server-streaming RPC. The callback runs synchronously for each
    /// validated message. Returning an error closes the response immediately.
    pub fn stream(
        &self,
        procedure: &str,
        input: &Value,
        mut on_message: impl FnMut(Value) -> Result<()>,
    ) -> Result<()> {
        let body = self.input(self.method(procedure)?, input)?;
        self.stream_inner(procedure, &body, None, &mut |message| {
            on_message(to_value(&message)?)
        })
    }

    fn stream_inner(
        &self,
        procedure: &str,
        payload: &[u8],
        timeout: Option<Duration>,
        on_message: &mut dyn FnMut(DynamicMessage) -> Result<()>,
    ) -> Result<()> {
        let method = self.method(procedure)?;
        if !method.server_streaming || method.client_streaming {
            return Err(RpcError::invalid("procedure is not server streaming"));
        }
        let frame = encode_envelope(0, payload)?;
        let response =
            self.post_with_timeout(procedure, "application/connect+json", &frame, timeout, "")?;
        let status = response.status().as_u16();
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_ascii_lowercase();
        if status != 200 {
            let bytes = read_bounded(response.into_body().into_reader(), MAX_ERROR_BYTES)?;
            return Err(remote_error(status, &bytes));
        }
        if !content_type.starts_with("application/connect+json") {
            return Err(RpcError::new(
                Code::DataLoss,
                "unexpected stream content type",
            ));
        }
        let mut reader = response.into_body().into_reader();
        loop {
            let mut header = [0_u8; 5];
            reader.read_exact(&mut header).map_err(|e| {
                RpcError::new(Code::DataLoss, format!("truncated stream envelope: {e}"))
            })?;
            let length = u32::from_be_bytes(header[1..5].try_into().unwrap()) as usize;
            if length > MAX_STREAM_MESSAGE_BYTES {
                return Err(RpcError::new(
                    Code::ResourceExhausted,
                    "stream message exceeds 8 MiB",
                ));
            }
            let mut body = vec![0_u8; length];
            reader.read_exact(&mut body).map_err(|e| {
                RpcError::new(Code::DataLoss, format!("truncated stream message: {e}"))
            })?;
            match header[0] {
                0 => {
                    let message = decode_json(method.output(), &body)
                        .map_err(|e| RpcError::new(Code::DataLoss, e.message))?;
                    on_message(message)?;
                }
                FLAG_END_STREAM => {
                    let end: Value = serde_json::from_slice(&body).map_err(|e| {
                        RpcError::new(Code::DataLoss, format!("invalid end stream: {e}"))
                    })?;
                    if let Some(error) = end.get("error") {
                        let remote: RpcError =
                            serde_json::from_value(error.clone()).map_err(|e| {
                                RpcError::new(Code::DataLoss, format!("invalid stream error: {e}"))
                            })?;
                        return Err(remote);
                    }
                    if !end.is_object() {
                        return Err(RpcError::new(Code::DataLoss, "invalid end stream"));
                    }
                    let mut extra = [0_u8; 1];
                    if reader
                        .read(&mut extra)
                        .map_err(|e| RpcError::new(Code::Unavailable, e.to_string()))?
                        != 0
                    {
                        return Err(RpcError::new(Code::DataLoss, "data after end stream"));
                    }
                    return Ok(());
                }
                _ => {
                    return Err(RpcError::new(
                        Code::DataLoss,
                        "unsupported stream envelope flags",
                    ))
                }
            }
        }
    }

    fn method(&self, procedure: &str) -> Result<&Method> {
        self.catalog
            .method(procedure)
            .ok_or_else(|| RpcError::new(Code::Unimplemented, "unknown procedure"))
    }

    fn input(&self, method: &Method, input: &Value) -> Result<Vec<u8>> {
        let bytes =
            bounded(serde_json::to_vec(input).map_err(|e| RpcError::invalid(e.to_string()))?)?;
        let message = decode_json(method.input(), &bytes)?;
        encode_json(&message)
    }

    fn post_with_timeout(
        &self,
        procedure: &str,
        content_type: &str,
        body: &[u8],
        timeout: Option<Duration>,
        approval: &str,
    ) -> Result<ureq::http::Response<ureq::Body>> {
        let mut request = self
            .agent
            .post(&format!("{}{}", self.base, procedure))
            .header("Content-Type", content_type)
            .header("Connect-Protocol-Version", "1");
        if !approval.is_empty() {
            request = request.header("Protectorate-Approval", approval);
        }
        if let Some(authorization) = self.authorization()? {
            request = request.header("Authorization", authorization.as_str());
        }
        if let Some(timeout) = timeout {
            request = request.config().timeout_global(Some(timeout)).build();
        }
        request
            .send(body)
            .map_err(|e| RpcError::new(Code::Unavailable, e.to_string()))
    }
}

impl Client {
    /// The Authorization header of the next request.
    pub fn authorization(&self) -> Result<Option<Zeroizing<String>>> {
        let bearer = |token: &str| Some(Zeroizing::new(format!("Bearer {token}")));
        match &self.credential {
            Credential::None => Ok(None),
            Credential::Bearer(token) => Ok(bearer(token)),
            Credential::TokenFile(path) => Ok(bearer(&self.held_token(path)?)),
        }
    }

    fn held_token(&self, path: &Path) -> Result<Zeroizing<String>> {
        let raw = Zeroizing::new(std::fs::read_to_string(path).map_err(|error| {
            RpcError::new(
                Code::Unauthenticated,
                format!("read token file {}: {error}", path.display()),
            )
        })?);
        let token = Zeroizing::new(raw.trim().to_owned());
        if token.is_empty() || token.contains(['\r', '\n']) {
            return Err(RpcError::new(
                Code::Unauthenticated,
                format!("token file {} holds no token", path.display()),
            ));
        }
        let Ok(claims) = capability::decode(&token) else {
            return Ok(token);
        };
        let now = protectorate_fsutil::unix_seconds();
        if claims.expires <= now || claims.expires - now > (claims.expires - claims.issued) / 2 {
            return Ok(token);
        }
        // Renewal is best effort: until the old token expires it still works.
        Ok(self.renew(path, &token).unwrap_or(token))
    }

    fn renew(&self, path: &Path, token: &str) -> Result<Zeroizing<String>> {
        let response = self
            .agent
            .post(&format!("{}{RENEW_TOKEN}", self.base))
            .header("Content-Type", "application/json")
            .header("Connect-Protocol-Version", "1")
            .header("Authorization", &format!("Bearer {token}"))
            .send(&b"{}"[..])
            .map_err(|e| RpcError::new(Code::Unavailable, e.to_string()))?;
        let status = response.status().as_u16();
        let bytes = read_bounded(response.into_body().into_reader(), MAX_ERROR_BYTES)?;
        if status != 200 {
            return Err(remote_error(status, &bytes));
        }
        let reply: Value = serde_json::from_slice(&bytes)
            .map_err(|e| RpcError::new(Code::DataLoss, e.to_string()))?;
        let fresh = reply
            .get("secret")
            .and_then(Value::as_str)
            .filter(|secret| capability::decode(secret).is_ok())
            .map(|secret| Zeroizing::new(secret.to_owned()))
            .ok_or_else(|| RpcError::new(Code::DataLoss, "hub renewed no capability"))?;
        protectorate_fsutil::write_file(path, format!("{}\n", fresh.as_str()).as_bytes(), 0o600)
            .map_err(|e| RpcError::new(Code::Internal, format!("rewrite token file: {e}")))?;
        Ok(fresh)
    }
}

fn valid_endpoint(base: &str) -> bool {
    if base.contains(['?', '#']) {
        return false;
    }
    let Some((scheme, rest)) = base.split_once("://") else {
        return false;
    };
    let authority = rest.split('/').next().unwrap_or("");
    if authority.is_empty() || authority.contains(['@', '\\', ' ']) {
        return false;
    }
    match scheme {
        "https" => true,
        "http" => {
            let (host, port) = if let Some(port) = authority.strip_prefix("127.0.0.1:") {
                ("127.0.0.1", port)
            } else if let Some(port) = authority.strip_prefix("[::1]:") {
                ("[::1]", port)
            } else {
                return false;
            };
            !host.is_empty()
                && !port.is_empty()
                && port.bytes().all(|b| b.is_ascii_digit())
                && port.parse::<u16>().is_ok_and(|p| p != 0)
        }
        _ => false,
    }
}

fn read_bounded(reader: impl Read, limit: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| RpcError::new(Code::Unavailable, e.to_string()))?;
    if bytes.len() as u64 > limit {
        return Err(RpcError::new(
            Code::ResourceExhausted,
            "response exceeds limit",
        ));
    }
    Ok(bytes)
}

/// The request id of a call the hub holds until the owner decides.
fn pending_approval(error: &RpcError) -> Option<String> {
    (error.code == Code::Aborted)
        .then(|| {
            error
                .details
                .iter()
                .find(|detail| detail["type"] == "protectorate.v1.Approval")
        })
        .flatten()
        .and_then(|detail| detail["debug"]["id"].as_str())
        .filter(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_hexdigit()))
        .map(str::to_owned)
}

fn remote_error(status: u16, bytes: &[u8]) -> RpcError {
    serde_json::from_slice::<RpcError>(bytes)
        .unwrap_or_else(|_| RpcError::new(Code::Unavailable, format!("remote HTTP {status}")))
}

fn bounded(bytes: Vec<u8>) -> Result<Vec<u8>> {
    if bytes.len() as u64 > MAX_UNARY_BYTES {
        return Err(RpcError::new(
            Code::ResourceExhausted,
            "request exceeds 16 MiB",
        ));
    }
    Ok(bytes)
}

fn transcode<T: ReflectMessage + Default>(message: &DynamicMessage) -> Result<T> {
    message
        .transcode_to::<T>()
        .map_err(|e| RpcError::new(Code::DataLoss, format!("transcode response: {e}")))
}

fn to_value(message: &DynamicMessage) -> Result<Value> {
    serde_json::to_value(message).map_err(RpcError::internal)
}
