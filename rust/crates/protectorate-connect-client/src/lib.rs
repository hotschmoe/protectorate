//! Blocking Connect JSON transport for the Rust CLI and hub-to-node calls.
//! Callers in an async runtime should use `spawn_blocking` for these methods.
use protectorate_wire::{
    decode_json, encode_envelope, encode_json, Catalog, Code, Method, Result, RpcError,
    FLAG_END_STREAM,
};
use serde_json::Value;
use std::{io::Read, path::Path, sync::OnceLock, time::Duration};
use zeroize::Zeroizing;

const MAX_UNARY_BYTES: u64 = 16 << 20;
const MAX_STREAM_MESSAGE_BYTES: usize = 8 << 20;
const MAX_ERROR_BYTES: u64 = 64 << 10;
const PUBLIC_DEVICE_START: &str = "/protectorate.v1.DeviceService/StartDeviceAuth";
const PUBLIC_DEVICE_POLL: &str = "/protectorate.v1.DeviceService/PollDeviceAuth";
static VIRTUAL_PROXY_SECRET: OnceLock<Zeroizing<String>> = OnceLock::new();

/// Per-process credential for the local TCP adapter. The adapter strips this
/// header before dialing the virtual tailnet stream.
pub fn virtual_proxy_secret() -> Result<String> {
    if let Some(secret) = VIRTUAL_PROXY_SECRET.get() {
        return Ok(secret.to_string());
    }
    let mut bytes = [0_u8; 32];
    getrandom::getrandom(&mut bytes)
        .map_err(|error| RpcError::new(Code::Internal, format!("proxy entropy: {error}")))?;
    let secret = Zeroizing::new(bytes.iter().map(|byte| format!("{byte:02x}")).collect());
    let _ = VIRTUAL_PROXY_SECRET.set(secret);
    Ok(VIRTUAL_PROXY_SECRET
        .get()
        .expect("proxy secret initialized")
        .to_string())
}

/// Parsed private CA bundle for explicit daemon-to-node TLS trust.
#[derive(Clone)]
pub struct TrustRoots(Vec<ureq::tls::Certificate<'static>>);

impl TrustRoots {
    pub fn from_pem(pem: &[u8]) -> Result<Self> {
        if pem.is_empty() || pem.len() > 1024 * 1024 {
            return Err(RpcError::invalid("TLS CA bundle must be under 1 MiB"));
        }
        let mut roots = Vec::new();
        let mut check = rustls::RootCertStore::empty();
        for item in ureq::tls::parse_pem(pem) {
            match item.map_err(|e| RpcError::invalid(format!("invalid TLS CA PEM: {e}")))? {
                ureq::tls::PemItem::Certificate(cert) => {
                    check
                        .add(rustls::pki_types::CertificateDer::from(cert.der().to_vec()))
                        .map_err(|e| {
                            RpcError::invalid(format!("invalid TLS CA certificate: {e}"))
                        })?;
                    roots.push(cert);
                }
                _ => {
                    return Err(RpcError::invalid(
                        "TLS CA bundle must contain certificates only",
                    ))
                }
            }
        }
        if roots.is_empty() {
            return Err(RpcError::invalid("TLS CA bundle has no certificates"));
        }
        Ok(Self(roots))
    }
}

#[derive(Clone)]
pub struct Client {
    base: String,
    token: Zeroizing<String>,
    agent: ureq::Agent,
    catalog: Catalog,
}

/// Response metadata used by desktop input commands to show an automatic lease.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UnaryMetadata {
    pub desk_lease_id: Option<String>,
    pub desk_lease_expires: Option<String>,
}

impl Client {
    /// Creates a client with the same endpoint, credentials and TLS policy,
    /// but its own connection pool. Shutting this client down interrupts only
    /// calls made through this pool.
    pub fn isolated(&self) -> Self {
        Self {
            base: self.base.clone(),
            token: self.token.clone(),
            agent: ureq::Agent::new_with_config(self.agent.config().clone()),
            catalog: self.catalog.clone(),
        }
    }

    /// Interrupts active socket I/O and refuses further calls on this pool.
    pub fn shutdown(self) {
        self.agent.shutdown();
    }

    /// The caller must supply the Protectorate loopback proxy. Its per-process
    /// credential is checked locally and stripped before the tailnet dial.
    pub fn new_virtual(base: &str, timeout: Duration) -> Result<Self> {
        if !base.starts_with("http://127.0.0.1:") {
            return Err(RpcError::invalid(
                "virtual client requires a 127.0.0.1 proxy",
            ));
        }
        Self::new_with_trust(base, virtual_proxy_secret()?, timeout, None)
    }
    pub fn new(base: &str, token: impl Into<String>, timeout: Duration) -> Result<Self> {
        if let Ok(pem) = std::env::var("PROTECTORATE_TLS_CA_PEM") {
            if pem.len() > 16 * 1024 {
                return Err(RpcError::invalid("TLS CA environment exceeds 16 KiB"));
            }
            return Self::new_with_ca_pem(base, token, timeout, Some(pem.as_bytes()));
        }
        let ca = std::env::var_os("PROTECTORATE_TLS_CA_CERT");
        Self::new_with_ca(base, token, timeout, ca.as_deref().map(Path::new))
    }

    /// Connect to the public device authorization procedures without a bearer
    /// credential. The same endpoint, TLS and response limits apply.
    pub fn new_public(base: &str, timeout: Duration) -> Result<Self> {
        Self::new(base, "", timeout)
    }

    /// Add one trusted PEM certificate while retaining normal TLS name checks.
    pub fn new_with_ca(
        base: &str,
        token: impl Into<String>,
        timeout: Duration,
        ca: Option<&Path>,
    ) -> Result<Self> {
        let pem = ca
            .map(|path| {
                let metadata = std::fs::metadata(path)
                    .map_err(|e| RpcError::invalid(format!("TLS CA certificate: {e}")))?;
                if !metadata.is_file() || metadata.len() > 1024 * 1024 {
                    return Err(RpcError::invalid(
                        "TLS CA must be a regular file under 1 MiB",
                    ));
                }
                let mut pem = Vec::new();
                std::fs::File::open(path)
                    .map_err(|e| RpcError::invalid(format!("TLS CA certificate: {e}")))?
                    .take(1024 * 1024 + 1)
                    .read_to_end(&mut pem)
                    .map_err(|e| RpcError::invalid(format!("TLS CA certificate: {e}")))?;
                if pem.len() > 1024 * 1024 {
                    return Err(RpcError::invalid("TLS CA exceeds 1 MiB"));
                }
                Ok(pem)
            })
            .transpose()?;
        Self::new_with_ca_pem(base, token, timeout, pem.as_deref())
    }

    /// Trust a PEM certificate supplied by a confined process environment.
    pub fn new_with_ca_pem(
        base: &str,
        token: impl Into<String>,
        timeout: Duration,
        pem: Option<&[u8]>,
    ) -> Result<Self> {
        let trust = pem.map(TrustRoots::from_pem).transpose()?;
        Self::new_with_trust(base, token, timeout, trust.as_ref())
    }

    /// Creates a client with daemon-supplied roots, ignoring process CA env.
    pub fn new_with_trust(
        base: &str,
        token: impl Into<String>,
        timeout: Duration,
        trust: Option<&TrustRoots>,
    ) -> Result<Self> {
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
        let mut config = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .max_redirects(0)
            .max_redirects_will_error(false)
            .timeout_global(Some(timeout));
        if base.starts_with("https://") {
            if let Some(trust) = trust {
                config = config.tls_config(
                    ureq::tls::TlsConfig::builder()
                        .root_certs(ureq::tls::RootCerts::new_with_certs(&trust.0))
                        .build(),
                );
            }
        }
        let config = config.build();
        Ok(Self {
            base: base.into(),
            token,
            agent: config.into(),
            catalog: Catalog::embedded()?,
        })
    }

    pub fn call(&self, procedure: &str, input: &Value) -> Result<Value> {
        self.call_with_metadata(procedure, input)
            .map(|(message, _)| message)
    }

    /// Call one unary RPC with a shorter per-request deadline while retaining
    /// this client's endpoint, bearer credential, and TLS trust roots.
    pub fn call_with_timeout(
        &self,
        procedure: &str,
        input: &Value,
        timeout: Duration,
    ) -> Result<Value> {
        self.call_with_metadata_timeout(procedure, input, Some(timeout))
            .map(|(message, _)| message)
    }

    /// Call a unary RPC and retain the bounded desktop lease response headers.
    pub fn call_with_metadata(
        &self,
        procedure: &str,
        input: &Value,
    ) -> Result<(Value, UnaryMetadata)> {
        self.call_with_metadata_timeout(procedure, input, None)
    }

    fn call_with_metadata_timeout(
        &self,
        procedure: &str,
        input: &Value,
        timeout: Option<Duration>,
    ) -> Result<(Value, UnaryMetadata)> {
        if self.token.is_empty() && !matches!(procedure, PUBLIC_DEVICE_START | PUBLIC_DEVICE_POLL) {
            return Err(RpcError::new(
                Code::PermissionDenied,
                "public client only permits device authorization",
            ));
        }
        let method = self.method(procedure)?;
        if !method.is_unary() {
            return Err(RpcError::invalid("procedure is streaming"));
        }
        let body = self.input(method, input)?;
        let response = self.post_with_timeout(procedure, "application/json", &body, timeout)?;
        let status = response.status().as_u16();
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_ascii_lowercase();
        let desk_unavailable = response
            .headers()
            .get("Protectorate-Desk-Unavailable")
            .and_then(|value| value.to_str().ok())
            .filter(|value| !value.is_empty() && value.len() <= 128)
            .map(str::to_owned);
        let metadata = UnaryMetadata {
            desk_lease_id: bounded_header(response.headers(), "protectorate-desk-lease"),
            desk_lease_expires: bounded_header(
                response.headers(),
                "protectorate-desk-lease-expires",
            ),
        };
        let bytes = read_bounded(
            response.into_body().into_reader(),
            if status == 200 {
                MAX_UNARY_BYTES
            } else {
                MAX_ERROR_BYTES
            },
        )?;
        if status != 200 {
            return Err(remote_error(status, &bytes, desk_unavailable));
        }
        if !content_type.starts_with("application/json") {
            return Err(RpcError::new(
                Code::DataLoss,
                "unexpected unary content type",
            ));
        }
        let message = decode_json(method.output(), &bytes)
            .map_err(|e| RpcError::new(Code::DataLoss, e.message))?;
        let message = serde_json::to_value(message)
            .map_err(|e| RpcError::new(Code::Internal, e.to_string()))?;
        Ok((message, metadata))
    }

    /// Calls a server-streaming RPC. The callback runs synchronously for each
    /// validated message. Returning an error closes the response immediately.
    pub fn stream(
        &self,
        procedure: &str,
        input: &Value,
        mut on_message: impl FnMut(Value) -> Result<()>,
    ) -> Result<()> {
        self.stream_with_timeout_inner(procedure, input, None, &mut on_message)
    }

    /// Stream with a per-request deadline while retaining this client's
    /// endpoint, bearer credential, and TLS trust roots.
    pub fn stream_with_timeout(
        &self,
        procedure: &str,
        input: &Value,
        timeout: Duration,
        mut on_message: impl FnMut(Value) -> Result<()>,
    ) -> Result<()> {
        self.stream_with_timeout_inner(procedure, input, Some(timeout), &mut on_message)
    }

    fn stream_with_timeout_inner(
        &self,
        procedure: &str,
        input: &Value,
        timeout: Option<Duration>,
        on_message: &mut impl FnMut(Value) -> Result<()>,
    ) -> Result<()> {
        if self.token.is_empty() {
            return Err(RpcError::new(
                Code::PermissionDenied,
                "public client only permits device authorization",
            ));
        }
        let method = self.method(procedure)?;
        if !method.server_streaming || method.client_streaming {
            return Err(RpcError::invalid("procedure is not server streaming"));
        }
        let payload = self.input(method, input)?;
        let frame = encode_envelope(0, &payload)?;
        let response =
            self.post_with_timeout(procedure, "application/connect+json", &frame, timeout)?;
        let status = response.status().as_u16();
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_ascii_lowercase();
        let desk_unavailable = response
            .headers()
            .get("Protectorate-Desk-Unavailable")
            .and_then(|value| value.to_str().ok())
            .filter(|value| !value.is_empty() && value.len() <= 128)
            .map(str::to_owned);
        if status != 200 {
            let bytes = read_bounded(response.into_body().into_reader(), MAX_ERROR_BYTES)?;
            return Err(remote_error(status, &bytes, desk_unavailable));
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
                    let value = serde_json::to_value(message)
                        .map_err(|e| RpcError::new(Code::Internal, e.to_string()))?;
                    on_message(value)?;
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
        let bytes = serde_json::to_vec(input).map_err(|e| RpcError::invalid(e.to_string()))?;
        if bytes.len() as u64 > MAX_UNARY_BYTES {
            return Err(RpcError::new(
                Code::ResourceExhausted,
                "request exceeds 16 MiB",
            ));
        }
        let message = decode_json(method.input(), &bytes)?;
        encode_json(&message)
    }

    fn post_with_timeout(
        &self,
        procedure: &str,
        content_type: &str,
        body: &[u8],
        timeout: Option<Duration>,
    ) -> Result<ureq::http::Response<ureq::Body>> {
        let mut request = self
            .agent
            .post(&format!("{}{}", self.base, procedure))
            .header("Content-Type", content_type)
            .header("Connect-Protocol-Version", "1");
        if !self.token.is_empty() {
            request = request.header("Authorization", &format!("Bearer {}", self.token.as_str()));
        }
        if let Some(timeout) = timeout {
            request = request.config().timeout_global(Some(timeout)).build();
        }
        request
            .send(body)
            .map_err(|e| RpcError::new(Code::Unavailable, e.to_string()))
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

fn bounded_header(headers: &ureq::http::HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty() && value.len() <= 256)
        .map(str::to_owned)
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

fn remote_error(status: u16, bytes: &[u8], desk_unavailable: Option<String>) -> RpcError {
    let mut error = serde_json::from_slice::<RpcError>(bytes)
        .unwrap_or_else(|_| RpcError::new(Code::Unavailable, format!("remote HTTP {status}")));
    if let Some(value) = desk_unavailable.and_then(|value| value.parse().ok()) {
        error.headers.insert("Protectorate-Desk-Unavailable", value);
    }
    error
}

#[cfg(test)]
mod trust_tests {
    use super::{Client, TrustRoots};
    use std::time::Duration;

    #[test]
    fn invalid_or_empty_private_ca_fails_before_network_use() {
        assert!(TrustRoots::from_pem(b"").is_err());
        assert!(TrustRoots::from_pem(b"not PEM").is_err());
        assert!(TrustRoots::from_pem(
            b"-----BEGIN CERTIFICATE-----\nYWJj\n-----END CERTIFICATE-----\n"
        )
        .is_err());
    }

    #[test]
    fn virtual_secret_is_never_configured_for_a_remote_url() {
        assert!(Client::new_virtual("https://node.example:7475", Duration::from_secs(1)).is_err());
        assert!(Client::new_virtual("http://127.0.0.1.evil:7475", Duration::from_secs(1)).is_err());
    }
}
