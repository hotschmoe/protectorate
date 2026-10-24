//! Blocking Connect JSON transport for the Rust CLI and hub-to-node calls.
//! Callers in an async runtime should use `spawn_blocking` for these methods.
use protectorate_auth::{capability, proof::DeviceKey};
use protectorate_wire::{
    decode_json, encode, encode_envelope, encode_json, Catalog, Code, DynamicMessage, Method,
    ReflectMessage, Result, Rpc, RpcError, FLAG_END_STREAM,
};
use serde_json::Value;
use std::{
    io::Read,
    net::{SocketAddr, ToSocketAddrs},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use ureq::unversioned::{
    resolver::{ResolvedSocketAddrs, Resolver},
    transport::{DefaultConnector, NextTimeout},
};
use zeroize::Zeroizing;

const MAX_UNARY_BYTES: u64 = 16 << 20;
const MAX_STREAM_MESSAGE_BYTES: usize = 8 << 20;
const MAX_ERROR_BYTES: u64 = 64 << 10;
/// How often a call the hub holds for owner approval is repeated.
const APPROVAL_POLL: Duration = Duration::from_secs(2);
/// Procedures a client without a token may call: device pairing and a new
/// node redeeming its join code.
const PUBLIC: [&str; 3] = [
    "/protectorate.v1.DeviceService/StartDeviceAuth",
    "/protectorate.v1.DeviceService/PollDeviceAuth",
    "/protectorate.v1.NodeService/JoinNode",
];

const RENEW_TOKEN: &str = "/protectorate.v1.TokenService/RenewToken";

/// How a client proves who it is on each request.
#[derive(Clone)]
pub enum Credential {
    /// Only the public pairing and join procedures.
    None,
    /// The connection authenticates the caller: the hub's client certificate
    /// toward a node.
    Transport,
    /// The owner token (loopback only) or a capability held in memory.
    Bearer(Zeroizing<String>),
    /// A capability file, read for every request. Past half its lifetime the
    /// client renews it through the hub and rewrites the file in place.
    TokenFile(PathBuf),
    /// An approved device's key, which signs every request.
    Device(Arc<DeviceKey>),
}

/// The key in a device credential record saved by `protectorate login`:
/// `{"hub", "deviceId", "key": <base64 PKCS#8>, ...}`.
pub fn device_key(record: &Value) -> Result<Arc<DeviceKey>> {
    use base64::{engine::general_purpose::STANDARD, Engine};
    let pkcs8 = record
        .get("key")
        .and_then(Value::as_str)
        .filter(|key| !key.is_empty())
        .and_then(|key| STANDARD.decode(key).ok())
        .map(Zeroizing::new)
        .ok_or_else(|| RpcError::new(Code::DataLoss, "device credential has no key"))?;
    DeviceKey::from_pkcs8(&pkcs8)
        .map(Arc::new)
        .map_err(|error| RpcError::new(Code::DataLoss, error))
}

/// A device credential file another process chose, such as the CLI handing
/// one to the MCP bridge.
pub fn read_device_credential(path: &Path) -> Result<Credential> {
    let raw = Zeroizing::new(std::fs::read(path).map_err(|error| {
        RpcError::new(
            Code::Unauthenticated,
            format!("read device credential {}: {error}", path.display()),
        )
    })?);
    let record: Value = serde_json::from_slice(&raw)
        .map_err(|error| RpcError::new(Code::DataLoss, format!("device credential: {error}")))?;
    device_key(&record).map(Credential::Device)
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
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

/// What the hub presents to node agents: its certificate and key, and the hub
/// CA that a node's certificate must chain to.
#[derive(Clone)]
pub struct NodeTls {
    roots: TrustRoots,
    identity: ureq::tls::ClientCert,
}

impl NodeTls {
    pub fn from_pem(ca: &[u8], cert: &[u8], key: &[u8]) -> Result<Self> {
        let certs = ureq::tls::parse_pem(cert)
            .filter_map(|item| match item {
                Ok(ureq::tls::PemItem::Certificate(cert)) => Some(cert),
                _ => None,
            })
            .collect::<Vec<_>>();
        let key = ureq::tls::PrivateKey::from_pem(key)
            .map_err(|e| RpcError::invalid(format!("hub key: {e}")))?;
        if certs.is_empty() {
            return Err(RpcError::invalid("hub certificate PEM is empty"));
        }
        Ok(Self {
            roots: TrustRoots::from_pem(ca)?,
            identity: ureq::tls::ClientCert::new_with_certs(&certs, key),
        })
    }
}

/// Dials one fixed address whatever host the URL names, so a node's
/// certificate is checked against its identity rather than its address.
#[derive(Debug)]
struct Fixed(SocketAddr);

impl Resolver for Fixed {
    fn resolve(
        &self,
        _: &ureq::http::Uri,
        _: &ureq::config::Config,
        _: NextTimeout,
    ) -> std::result::Result<ResolvedSocketAddrs, ureq::Error> {
        let mut addrs = self.empty();
        addrs.push(self.0);
        Ok(addrs)
    }
}

#[derive(Clone)]
pub struct Client {
    base: String,
    credential: Credential,
    dial: Option<SocketAddr>,
    agent: ureq::Agent,
    catalog: Catalog,
}

fn agent(config: ureq::config::Config, dial: Option<SocketAddr>) -> ureq::Agent {
    match dial {
        Some(addr) => ureq::Agent::with_parts(config, DefaultConnector::default(), Fixed(addr)),
        None => config.into(),
    }
}

impl Client {
    /// Creates a client with the same endpoint, credentials and TLS policy,
    /// but its own connection pool. Shutting this client down interrupts only
    /// calls made through this pool.
    pub fn isolated(&self) -> Self {
        Self {
            base: self.base.clone(),
            credential: self.credential.clone(),
            dial: self.dial,
            agent: agent(self.agent.config().clone(), self.dial),
            catalog: self.catalog.clone(),
        }
    }

    /// Interrupts active socket I/O and refuses further calls on this pool.
    pub fn shutdown(self) {
        self.agent.shutdown();
    }

    /// A hub-to-node client. With `tls` it dials `address` over mutual TLS
    /// and requires the node's certificate to name `server_name`; without it
    /// `address` must be a loopback agent that takes plain HTTP.
    pub fn new_node(
        server_name: &str,
        address: &str,
        tls: Option<&NodeTls>,
        timeout: Duration,
    ) -> Result<Self> {
        let config = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .max_redirects(0)
            .max_redirects_will_error(false)
            .timeout_global(Some(timeout));
        let (base, config, dial) = match tls {
            Some(tls) => {
                let addr = address
                    .to_socket_addrs()
                    .ok()
                    .and_then(|mut addrs| addrs.next())
                    .ok_or_else(|| {
                        RpcError::new(Code::Unavailable, format!("cannot resolve {address}"))
                    })?;
                let config = config.tls_config(
                    ureq::tls::TlsConfig::builder()
                        .root_certs(ureq::tls::RootCerts::new_with_certs(&tls.roots.0))
                        .client_cert(Some(tls.identity.clone()))
                        .build(),
                );
                (
                    format!("https://{server_name}:{}", addr.port()),
                    config,
                    Some(addr),
                )
            }
            None => (format!("http://{address}"), config, None),
        };
        if !valid_endpoint(&base) {
            return Err(RpcError::invalid(
                "a node without a certificate must be a loopback agent",
            ));
        }
        Ok(Self {
            base,
            credential: Credential::Transport,
            dial,
            agent: agent(config.build(), dial),
            catalog: Catalog::embedded()?,
        })
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

    /// Connect to the public device authorization and node join procedures
    /// without a bearer credential. The same endpoint, TLS and response limits
    /// apply.
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
        Ok(Self {
            base: base.into(),
            credential: if token.is_empty() {
                Credential::None
            } else {
                Credential::Bearer(token)
            },
            dial: None,
            agent: config.build().into(),
            catalog: Catalog::embedded()?,
        })
    }

    /// The same endpoint and TLS trust with another credential.
    pub fn with_credential(mut self, credential: Credential) -> Self {
        self.credential = credential;
        self
    }

    fn public(&self) -> bool {
        matches!(self.credential, Credential::None)
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

    /// Call one unary RPC with a shorter per-request deadline while retaining
    /// this client's endpoint, bearer credential, and TLS trust roots.
    pub fn call_with_timeout(
        &self,
        procedure: &str,
        input: &Value,
        timeout: Duration,
    ) -> Result<Value> {
        self.call_json(procedure, input, Some(timeout))
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
        if self.public() && !PUBLIC.contains(&procedure) {
            return Err(RpcError::new(
                Code::PermissionDenied,
                "a client without a token may only pair a device or join a node",
            ));
        }
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

    /// Stream with a per-request deadline while retaining this client's
    /// endpoint, bearer credential, and TLS trust roots.
    pub fn stream_with_timeout(
        &self,
        procedure: &str,
        input: &Value,
        timeout: Duration,
        mut on_message: impl FnMut(Value) -> Result<()>,
    ) -> Result<()> {
        let body = self.input(self.method(procedure)?, input)?;
        self.stream_inner(procedure, &body, Some(timeout), &mut |message| {
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
        if self.public() {
            return Err(RpcError::new(
                Code::PermissionDenied,
                "a client without a token may only pair a device or join a node",
            ));
        }
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
        if let Some(authorization) = self.authorization("POST", procedure)? {
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
    /// The Authorization header for one `verb` request to `path`.
    pub fn authorization(&self, verb: &str, path: &str) -> Result<Option<Zeroizing<String>>> {
        let bearer = |token: &str| Some(Zeroizing::new(format!("Bearer {token}")));
        match &self.credential {
            Credential::None | Credential::Transport => Ok(None),
            Credential::Bearer(token) => Ok(bearer(token)),
            Credential::TokenFile(path) => Ok(bearer(&self.held_token(path)?)),
            Credential::Device(key) => key
                .prove(verb, path, now_unix())
                .map(|proof| Some(Zeroizing::new(format!("Proof {proof}"))))
                .map_err(|error| RpcError::new(Code::Internal, error)),
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
        let now = now_unix();
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
    serde_json::to_value(message).map_err(|e| RpcError::new(Code::Internal, e.to_string()))
}
