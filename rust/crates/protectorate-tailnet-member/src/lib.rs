//! Local identity, control key discovery, and registration for a native
//! Tailscale protocol member. Registration alone does not carry packets or
//! establish a connected state.
//!
//! Protocol references: Tailscale `tailcfg.OverTLSPublicKeyResponse`,
//! `types/key/machine.go`, and `control/tsp/tsp.go` at v1.104.1.

use base64::Engine;
use chacha20poly1305::{aead::AeadInPlace, ChaCha20Poly1305, KeyInit};
use curve25519_dalek::montgomery::MontgomeryPoint;
use serde::{Deserialize, Serialize};
use snow::{
    params::{CipherChoice, DHChoice, HashChoice},
    resolvers::{CryptoResolver, DefaultResolver},
    types::{Cipher, Dh, Hash, Random},
};
use std::{
    fmt,
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    net::{SocketAddr, TcpStream, ToSocketAddrs},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use ureq::unversioned::{
    resolver::{ResolvedSocketAddrs, Resolver},
    transport::{DefaultConnector, NextTimeout},
};

#[derive(Debug)]
struct FixedResolver(SocketAddr);

impl Resolver for FixedResolver {
    fn resolve(
        &self,
        _uri: &http::Uri,
        _config: &ureq::config::Config,
        _timeout: NextTimeout,
    ) -> std::result::Result<ResolvedSocketAddrs, ureq::Error> {
        let mut addresses = self.empty();
        addresses.push(self.0);
        Ok(addresses)
    }
}

const MACHINE_FILE: &str = "machine.key";
const NODE_FILE: &str = "node.key";
const CONTROL_FILE: &str = "control-key.json";
const MAX_KEY_RESPONSE: u64 = 64 << 10;
/// Tailscale v1.104.1's `tailcfg.CurrentCapabilityVersion`.
pub const CAPABILITY_VERSION: u16 = 148;

#[derive(Debug)]
pub enum Error {
    Invalid(String),
    Io(std::io::Error),
    Transport(String),
    Status(u16),
    Decode(String),
    ControlKeyChanged { expected: String, observed: String },
    ControlOriginChanged { expected: String, observed: String },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(s) | Self::Transport(s) | Self::Decode(s) => f.write_str(s),
            Self::Io(e) => write!(f, "member state: {e}"),
            Self::Status(code) => write!(f, "control key HTTP status {code}"),
            Self::ControlKeyChanged { expected, observed } => {
                write!(f, "control key changed from {expected} to {observed}")
            }
            Self::ControlOriginChanged { expected, observed } => {
                write!(f, "control origin changed from {expected} to {observed}")
            }
        }
    }
}
impl std::error::Error for Error {}
impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Registration {
    pub machine_authorized: bool,
    #[serde(default)]
    pub node_key_expired: bool,
    #[serde(default)]
    pub auth_url: String,
    #[serde(default)]
    pub error: String,
}

/// A 32-byte X25519 machine public key, in Tailscale's control-plane format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MachinePublic([u8; 32]);
impl MachinePublic {
    pub fn parse(raw: &str) -> Result<Self> {
        let hex = raw
            .strip_prefix("mkey:")
            .ok_or_else(|| Error::Invalid("machine public key needs mkey: prefix".into()))?;
        let bytes = decode_hex_32(hex)?;
        if bytes == [0; 32] {
            return Err(Error::Invalid("zero machine public key".into()));
        }
        Ok(Self(bytes))
    }

    pub fn bytes(self) -> [u8; 32] {
        self.0
    }
}
impl fmt::Display for MachinePublic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "mkey:{}", encode_hex(&self.0))
    }
}

/// Durable local identity, before control registration and networking.
pub struct MemberIdentity {
    directory: PathBuf,
    private_key: [u8; 32],
    node_private_key: [u8; 32],
}

impl MemberIdentity {
    /// Opens or creates a machine key. The state directory and key file are
    /// owner-only on Unix. The private key is never returned by this API.
    pub fn open(directory: impl AsRef<Path>) -> Result<Self> {
        let directory = directory.as_ref();
        create_private_dir(directory)?;
        let private_key = open_private_key(directory, MACHINE_FILE)?;
        let node_private_key = open_private_key(directory, NODE_FILE)?;
        Ok(Self {
            directory: directory.to_path_buf(),
            private_key,
            node_private_key,
        })
    }

    pub fn node_public_key(&self) -> String {
        format!(
            "nodekey:{}",
            encode_hex(&MontgomeryPoint::mul_base_clamped(self.node_private_key).to_bytes())
        )
    }

    /// Registered WireGuard/DERP identity for the packet engine. The caller
    /// must hold this copy in a private owner and clear it when that owner
    /// stops; never log or serialize it.
    pub fn node_private_key_bytes(&self) -> [u8; 32] {
        self.node_private_key
    }

    /// Registers with a preauthorized Headscale key over the pinned Noise
    /// channel. The returned authorization flag is checked before use.
    pub fn register_http(
        &self,
        control_origin: &str,
        preauth_key: &str,
        hostname: &str,
        timeout: Duration,
    ) -> Result<Registration> {
        self.register_with_ca_pem(control_origin, preauth_key, hostname, timeout, None)
    }

    /// Registers through a verified HTTPS control origin with an optional
    /// private PEM CA bundle, used instead of the public CA set.
    pub fn register_with_ca_pem(
        &self,
        control_origin: &str,
        preauth_key: &str,
        hostname: &str,
        timeout: Duration,
        ca_pem: Option<&[u8]>,
    ) -> Result<Registration> {
        self.register_with_local_addr(control_origin, preauth_key, hostname, timeout, ca_pem, None)
    }

    /// Register against the public origin while dialing a known local control
    /// socket. HTTP Host, TLS name checks, and the Noise key pin still use the
    /// public origin.
    pub fn register_with_local_addr(
        &self,
        control_origin: &str,
        preauth_key: &str,
        hostname: &str,
        timeout: Duration,
        ca_pem: Option<&[u8]>,
        local: Option<SocketAddr>,
    ) -> Result<Registration> {
        if preauth_key.is_empty() || preauth_key.len() > 4096 {
            return Err(Error::Invalid("preauth key must be 1..4096 bytes".into()));
        }
        if hostname.is_empty()
            || hostname.len() > 255
            || !hostname
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
        {
            return Err(Error::Invalid("invalid member hostname".into()));
        }
        let body = serde_json::json!({
            "Version": CAPABILITY_VERSION,
            "NodeKey": self.node_public_key(),
            "Auth": {"AuthKey": preauth_key},
            "Hostinfo": {"OS": std::env::consts::OS, "Hostname": hostname, "IPNVersion": "protectorate-rust"},
        });
        let bytes = serde_json::to_vec(&body)
            .map_err(|e| Error::Decode(format!("encode registration: {e}")))?;
        let channel =
            self.connect_control_with_local_addr(control_origin, timeout, ca_pem, local)?;
        let mut http = Http2Channel::new(channel, control_origin)?;
        let response = http.post("/machine/register", &bytes, 4 << 20)?;
        let registration: Registration = serde_json::from_slice(&response)
            .map_err(|e| Error::Decode(format!("decode registration: {e}")))?;
        if !registration.error.is_empty() {
            return Err(Error::Transport(format!(
                "registration: {}",
                registration.error
            )));
        }
        if !registration.machine_authorized {
            return Err(Error::Transport("registration was not authorized".into()));
        }
        Ok(registration)
    }

    pub fn machine_public_key(&self) -> MachinePublic {
        MachinePublic(MontgomeryPoint::mul_base_clamped(self.private_key).to_bytes())
    }

    /// Fetches `/key?v=148` and pins its Noise public key for this origin.
    /// This performs no registration and does not establish a Noise channel.
    /// On plain HTTP, the first contact is unauthenticated until the operator
    /// independently verifies the returned key.
    pub fn discover_control(&self, control_origin: &str, timeout: Duration) -> Result<ControlKey> {
        self.discover_control_with_ca_pem(control_origin, timeout, None)
    }

    /// Discovers a control key with normal TLS name checks and optional
    /// additional PEM trust roots for a private control CA.
    pub fn discover_control_with_ca_pem(
        &self,
        control_origin: &str,
        timeout: Duration,
        ca_pem: Option<&[u8]>,
    ) -> Result<ControlKey> {
        self.discover_control_with_local_addr(control_origin, timeout, ca_pem, None)
    }

    /// Fetch the key through a local socket while retaining the public origin
    /// as the request Host, verified TLS name, and durable pin identity.
    pub fn discover_control_with_local_addr(
        &self,
        control_origin: &str,
        timeout: Duration,
        ca_pem: Option<&[u8]>,
        local: Option<SocketAddr>,
    ) -> Result<ControlKey> {
        validate_origin(control_origin)?;
        if timeout.is_zero() {
            return Err(Error::Invalid("control timeout must be positive".into()));
        }
        let _ = trust_roots(ca_pem)?;
        let mut config = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .max_redirects(0)
            .max_redirects_will_error(false)
            .timeout_global(Some(timeout));
        if let Some(ca_pem) = ca_pem {
            let mut certs = Vec::new();
            for item in ureq::tls::parse_pem(ca_pem) {
                if let ureq::tls::PemItem::Certificate(cert) =
                    item.map_err(|e| Error::Invalid(format!("invalid control CA PEM: {e}")))?
                {
                    certs.push(cert);
                }
            }
            config = config.tls_config(
                ureq::tls::TlsConfig::builder()
                    .root_certs(ureq::tls::RootCerts::new_with_certs(&certs))
                    .build(),
            );
        }
        let config = config.build();
        let agent: ureq::Agent = if let Some(address) = local {
            ureq::Agent::with_parts(config, DefaultConnector::default(), FixedResolver(address))
        } else {
            config.into()
        };
        let url = format!("{control_origin}/key?v={CAPABILITY_VERSION}");
        let mut response = agent
            .get(&url)
            .call()
            .map_err(|e| Error::Transport(format!("fetch control key: {e}")))?;
        if response.status().as_u16() != 200 {
            return Err(Error::Status(response.status().as_u16()));
        }
        let mut body = Vec::new();
        response
            .body_mut()
            .as_reader()
            .take(MAX_KEY_RESPONSE + 1)
            .read_to_end(&mut body)?;
        if body.len() as u64 > MAX_KEY_RESPONSE {
            return Err(Error::Decode("control key response too large".into()));
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct KeyResponse {
            public_key: String,
        }
        let response: KeyResponse = serde_json::from_slice(&body)
            .map_err(|e| Error::Decode(format!("decode control key response: {e}")))?;
        let observed = MachinePublic::parse(&response.public_key)?;
        let pin_path = self.directory.join(CONTROL_FILE);
        let pin = ControlPin {
            origin: control_origin.into(),
            public_key: observed.to_string(),
        };
        let encoded = serde_json::to_vec(&pin)
            .map_err(|e| Error::Decode(format!("encode control pin: {e}")))?;
        let newly_pinned = match create_private_file(&pin_path, &encoded) {
            Ok(()) => true,
            Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::AlreadyExists => false,
            Err(e) => return Err(e),
        };
        verify_private_file(&pin_path)?;
        let existing: ControlPin = serde_json::from_slice(&fs::read(&pin_path)?)
            .map_err(|e| Error::Decode(format!("decode control pin: {e}")))?;
        if existing.origin != control_origin {
            return Err(Error::ControlOriginChanged {
                expected: existing.origin,
                observed: control_origin.into(),
            });
        }
        let expected = MachinePublic::parse(&existing.public_key)?;
        if expected != observed {
            return Err(Error::ControlKeyChanged {
                expected: expected.to_string(),
                observed: observed.to_string(),
            });
        }
        Ok(ControlKey {
            public_key: observed,
            newly_pinned,
        })
    }

    /// Opens an authenticated TS2021 Noise channel. The server key is checked
    /// against the durable origin pin. This is only the control transport.
    pub fn connect_control_http(
        &self,
        control_origin: &str,
        timeout: Duration,
    ) -> Result<NoiseChannel> {
        self.connect_control_with_ca_pem(control_origin, timeout, None)
    }

    pub fn connect_control_with_ca_pem(
        &self,
        control_origin: &str,
        timeout: Duration,
        ca_pem: Option<&[u8]>,
    ) -> Result<NoiseChannel> {
        self.connect_control_with_local_addr(control_origin, timeout, ca_pem, None)
    }

    pub fn connect_control_with_local_addr(
        &self,
        control_origin: &str,
        timeout: Duration,
        ca_pem: Option<&[u8]>,
        local: Option<SocketAddr>,
    ) -> Result<NoiseChannel> {
        let key = self.discover_control_with_local_addr(control_origin, timeout, ca_pem, local)?;
        NoiseChannel::connect(
            control_origin,
            timeout,
            self.private_key,
            key.public_key,
            ca_pem,
            local,
        )
    }
}

/// Authenticated TS2021 Noise transport, before HTTP/2 registration.
pub struct NoiseChannel {
    socket: BufReader<ControlStream>,
    noise: snow::TransportState,
    handshake_hash: [u8; 32],
    received: Vec<u8>,
    next: usize,
    write_failed: bool,
}

enum ControlStream {
    Plain(TcpStream),
    Tls(Box<rustls::StreamOwned<rustls::ClientConnection, TcpStream>>),
}

impl Read for ControlStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Self::Plain(stream) => stream.read(buf),
            Self::Tls(stream) => stream.read(buf),
        }
    }
}

impl Write for ControlStream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            Self::Plain(stream) => stream.write(buf),
            Self::Tls(stream) => stream.write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Self::Plain(stream) => stream.flush(),
            Self::Tls(stream) => stream.flush(),
        }
    }
}

fn trust_roots(ca_pem: Option<&[u8]>) -> Result<rustls::RootCertStore> {
    let mut roots = if ca_pem.is_some() {
        rustls::RootCertStore::empty()
    } else {
        rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned())
    };
    if let Some(pem) = ca_pem {
        if pem.is_empty() || pem.len() > 1 << 20 {
            return Err(Error::Invalid(
                "control CA PEM must be 1..1048576 bytes".into(),
            ));
        }
        let mut count = 0;
        for item in ureq::tls::parse_pem(pem) {
            let cert =
                match item.map_err(|e| Error::Invalid(format!("invalid control CA PEM: {e}")))? {
                    ureq::tls::PemItem::Certificate(cert) => cert,
                    _ => {
                        return Err(Error::Invalid(
                            "control CA PEM must contain certificates only".into(),
                        ))
                    }
                };
            roots
                .add(rustls::pki_types::CertificateDer::from(cert.der().to_vec()))
                .map_err(|e| Error::Invalid(format!("invalid control CA certificate: {e}")))?;
            count += 1;
        }
        if count == 0 {
            return Err(Error::Invalid("control CA PEM has no certificates".into()));
        }
    }
    Ok(roots)
}

impl NoiseChannel {
    fn connect(
        origin: &str,
        timeout: Duration,
        private_key: [u8; 32],
        control_key: MachinePublic,
        ca_pem: Option<&[u8]>,
        local: Option<SocketAddr>,
    ) -> Result<Self> {
        let uri: http::Uri = origin
            .parse()
            .map_err(|_| Error::Invalid("invalid control origin".into()))?;
        let authority = uri
            .authority()
            .ok_or_else(|| Error::Invalid("control origin has no authority".into()))?;
        let https = uri.scheme_str() == Some("https");
        let port = uri.port_u16().unwrap_or(if https { 443 } else { 80 });
        let host = uri
            .host()
            .ok_or_else(|| Error::Invalid("control origin has no host".into()))?;
        let mut addresses: Vec<SocketAddr> = if let Some(address) = local {
            vec![address]
        } else {
            (host, port).to_socket_addrs()?.collect()
        };
        if addresses.is_empty() {
            return Err(Error::Transport("control host has no addresses".into()));
        }
        addresses.reverse();
        let socket = loop {
            let address = addresses.pop().expect("nonempty control addresses");
            match TcpStream::connect_timeout(&address, timeout) {
                Ok(socket) => break socket,
                Err(error) if addresses.is_empty() => return Err(error.into()),
                Err(_) => continue,
            }
        };
        socket.set_read_timeout(Some(timeout))?;
        socket.set_write_timeout(Some(timeout))?;
        let mut socket = if https {
            let config = rustls::ClientConfig::builder_with_provider(Arc::new(
                rustls::crypto::ring::default_provider(),
            ))
            .with_safe_default_protocol_versions()
            .map_err(|e| Error::Transport(format!("TLS protocol versions: {e}")))?
            .with_root_certificates(trust_roots(ca_pem)?)
            .with_no_client_auth();
            let server_name = rustls::pki_types::ServerName::try_from(host.to_owned())
                .map_err(|_| Error::Invalid("invalid control TLS server name".into()))?;
            let conn = rustls::ClientConnection::new(Arc::new(config), server_name)
                .map_err(|e| Error::Transport(format!("control TLS: {e}")))?;
            ControlStream::Tls(Box::new(rustls::StreamOwned::new(conn, socket)))
        } else {
            ControlStream::Plain(socket)
        };

        let params: snow::params::NoiseParams = "Noise_IK_25519_ChaChaPoly_BLAKE2s"
            .parse()
            .map_err(|e| Error::Invalid(format!("Noise parameters: {e}")))?;
        let prologue = format!("Tailscale Control Protocol v{CAPABILITY_VERSION}");
        let mut handshake = snow::Builder::with_resolver(params, Box::new(ControlResolver))
            .local_private_key(&private_key)
            .remote_public_key(&control_key.0)
            .prologue(prologue.as_bytes())
            .build_initiator()
            .map_err(|e| Error::Transport(format!("Noise initiator: {e}")))?;
        let mut initial = [0; 96];
        let initial_len = handshake
            .write_message(&[], &mut initial)
            .map_err(|e| Error::Transport(format!("Noise initiation: {e}")))?;
        if initial_len != 96 {
            return Err(Error::Transport(
                "unexpected Noise initiation length".into(),
            ));
        }
        let mut frame = [0; 101];
        frame[..2].copy_from_slice(&CAPABILITY_VERSION.to_be_bytes());
        frame[2] = 1;
        frame[3..5].copy_from_slice(&96u16.to_be_bytes());
        frame[5..].copy_from_slice(&initial);
        let encoded = base64::engine::general_purpose::STANDARD.encode(frame);
        write!(
            socket,
            "POST /ts2021 HTTP/1.1\r\nHost: {authority}\r\nUpgrade: tailscale-control-protocol\r\nConnection: upgrade\r\nX-Tailscale-Handshake: {encoded}\r\nContent-Length: 0\r\n\r\n"
        )?;
        socket.flush()?;
        let mut socket = BufReader::new(socket);
        let headers = read_http_headers(&mut socket)?;
        let headers_text = std::str::from_utf8(&headers)
            .map_err(|_| Error::Transport("non-UTF-8 upgrade response".into()))?;
        let mut lines = headers_text.split("\r\n");
        let status = lines.next().unwrap_or_default();
        if !status.starts_with("HTTP/1.1 101 ") && status != "HTTP/1.1 101" {
            return Err(Error::Transport(format!(
                "control upgrade rejected: {status}"
            )));
        }
        let upgraded = lines.any(|line| {
            line.split_once(':').is_some_and(|(name, value)| {
                name.eq_ignore_ascii_case("upgrade")
                    && value
                        .trim()
                        .eq_ignore_ascii_case("tailscale-control-protocol")
            })
        });
        if !upgraded {
            return Err(Error::Transport("control upgrade protocol mismatch".into()));
        }
        let mut response = [0; 51];
        socket.read_exact(&mut response[..3])?;
        if response[0] == 3 {
            let size = u16::from_be_bytes([response[1], response[2]]) as usize;
            if size > 4093 {
                return Err(Error::Transport("oversized Noise error".into()));
            }
            let mut error = vec![0; size];
            socket.read_exact(&mut error)?;
            return Err(Error::Transport(format!(
                "control Noise error: {}",
                String::from_utf8_lossy(&error)
            )));
        }
        if response[0] != 2 || response[1..3] != 48u16.to_be_bytes() {
            return Err(Error::Transport("invalid Noise response frame".into()));
        }
        socket.read_exact(&mut response[3..])?;
        let mut payload = [0; 48];
        let len = handshake
            .read_message(&response[3..], &mut payload)
            .map_err(|e| Error::Transport(format!("Noise authentication failed: {e}")))?;
        if len != 0 {
            return Err(Error::Transport("unexpected Noise response payload".into()));
        }
        let mut handshake_hash = [0; 32];
        handshake_hash.copy_from_slice(handshake.get_handshake_hash());
        let noise = handshake
            .into_transport_mode()
            .map_err(|e| Error::Transport(format!("Noise transport: {e}")))?;
        Ok(Self {
            socket,
            noise,
            handshake_hash,
            received: Vec::new(),
            next: 0,
            write_failed: false,
        })
    }

    pub fn handshake_hash(&self) -> [u8; 32] {
        self.handshake_hash
    }
}

/// A single encrypted HTTP/2 request over TS2021. Each instance handles one
/// request; reconnect to poll a new map stream.
pub struct Http2Channel {
    channel: NoiseChannel,
    authority: String,
    decoder: hpack::Decoder<'static>,
}

impl Http2Channel {
    pub fn new(mut channel: NoiseChannel, origin: &str) -> Result<Self> {
        let uri: http::Uri = origin
            .parse()
            .map_err(|_| Error::Invalid("invalid control origin".into()))?;
        let authority = uri
            .authority()
            .ok_or_else(|| Error::Invalid("control origin has no authority".into()))?
            .as_str()
            .to_owned();
        channel.write_all(b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n")?;
        write_h2_frame(&mut channel, 4, 0, 0, &[])?;
        channel.flush()?;
        Ok(Self {
            channel,
            authority,
            decoder: hpack::Decoder::new(),
        })
    }

    pub fn post(&mut self, path: &str, body: &[u8], max_response: usize) -> Result<Vec<u8>> {
        let mut response = Vec::new();
        self.post_stream(path, body, max_response, |data| {
            response.extend_from_slice(data);
            Ok(false)
        })?;
        Ok(response)
    }

    /// Delivers DATA frames incrementally. Return `true` from `on_data` when
    /// enough data has arrived; this closes the caller's use of this channel.
    /// `max_response` bounds total DATA bytes read during the poll.
    pub fn post_stream(
        &mut self,
        path: &str,
        body: &[u8],
        max_response: usize,
        mut on_data: impl FnMut(&[u8]) -> Result<bool>,
    ) -> Result<()> {
        if !path.starts_with('/') || body.len() > 4 << 20 || max_response > 16 << 20 {
            return Err(Error::Invalid("invalid HTTP/2 request bounds".into()));
        }
        let mut headers = Vec::new();
        for (name, value) in [
            (":method", "POST"),
            (":scheme", "https"),
            (":path", path),
            (":authority", self.authority.as_str()),
            ("content-type", "application/json"),
        ] {
            hpack_literal(&mut headers, name.as_bytes(), value.as_bytes());
        }
        write_h2_frame(&mut self.channel, 1, 4, 1, &headers)?;
        if body.is_empty() {
            write_h2_frame(&mut self.channel, 0, 1, 1, &[])?;
        } else {
            for (index, chunk) in body.chunks(16_384).enumerate() {
                let end = (index + 1) * 16_384 >= body.len();
                write_h2_frame(&mut self.channel, 0, u8::from(end), 1, chunk)?;
            }
        }
        self.channel.flush()?;

        let mut received = 0usize;
        let mut status = None;
        let mut headers_done = false;
        let mut header_block = Vec::new();
        for _ in 0..65_536 {
            let (kind, flags, stream, payload) = read_h2_frame(&mut self.channel)?;
            if stream == 0 {
                match kind {
                    4 if flags & 1 == 0 => {
                        write_h2_frame(&mut self.channel, 4, 1, 0, &[])?;
                        self.channel.flush()?;
                    }
                    6 if flags & 1 == 0 && payload.len() == 8 => {
                        write_h2_frame(&mut self.channel, 6, 1, 0, &payload)?;
                        self.channel.flush()?;
                    }
                    7 => return Err(Error::Transport("control HTTP/2 GOAWAY".into())),
                    _ => {}
                }
                continue;
            }
            if stream != 1 {
                return Err(Error::Transport("unexpected HTTP/2 stream".into()));
            }
            match kind {
                1 | 9 => {
                    if kind == 9 && headers_done {
                        return Err(Error::Transport("unexpected HTTP/2 continuation".into()));
                    }
                    // Headscale emits plain header blocks without PRIORITY or padding.
                    if flags & 0x28 != 0 {
                        return Err(Error::Transport("unsupported HTTP/2 header flags".into()));
                    }
                    header_block.extend_from_slice(&payload);
                    if header_block.len() > 64 << 10 {
                        return Err(Error::Transport("HTTP/2 headers too large".into()));
                    }
                    if flags & 4 != 0 {
                        let decoded = self
                            .decoder
                            .decode(&header_block)
                            .map_err(|e| Error::Decode(format!("decode HTTP/2 headers: {e:?}")))?;
                        for (name, value) in decoded {
                            if name == b":status" {
                                status = std::str::from_utf8(&value)
                                    .ok()
                                    .and_then(|s| s.parse::<u16>().ok());
                            }
                        }
                        headers_done = true;
                        header_block.clear();
                        if status != Some(200) {
                            return Err(Error::Status(status.unwrap_or(0)));
                        }
                    }
                }
                0 => {
                    if !headers_done || flags & 8 != 0 {
                        return Err(Error::Transport("invalid HTTP/2 DATA frame".into()));
                    }
                    received = received.saturating_add(payload.len());
                    if received > max_response {
                        return Err(Error::Transport("HTTP/2 response too large".into()));
                    }
                    let done = on_data(&payload)?;
                    // Restore both connection and stream receive windows after
                    // consumer delivery. Without this, a large netmap stalls.
                    if !payload.is_empty() {
                        let increment = (payload.len() as u32).to_be_bytes();
                        write_h2_frame(&mut self.channel, 8, 0, 0, &increment)?;
                        write_h2_frame(&mut self.channel, 8, 0, 1, &increment)?;
                        self.channel.flush()?;
                    }
                    if done {
                        return Ok(());
                    }
                }
                3 => return Err(Error::Transport("HTTP/2 stream reset".into())),
                _ => {}
            }
            if flags & 1 != 0 && (kind == 0 || kind == 1) {
                return Ok(());
            }
        }
        Err(Error::Transport("too many HTTP/2 frames".into()))
    }
}

fn hpack_integer(out: &mut Vec<u8>, value: usize, prefix: u8) {
    let mask = (1u8 << prefix) - 1;
    if value < usize::from(mask) {
        out.push(value as u8);
    } else {
        out.push(mask);
        let mut remaining = value - usize::from(mask);
        while remaining >= 128 {
            out.push((remaining as u8 & 0x7f) | 0x80);
            remaining >>= 7;
        }
        out.push(remaining as u8);
    }
}

fn hpack_literal(out: &mut Vec<u8>, name: &[u8], value: &[u8]) {
    out.push(0); // literal header without indexing, literal name
    hpack_integer(out, name.len(), 7);
    out.extend_from_slice(name);
    hpack_integer(out, value.len(), 7);
    out.extend_from_slice(value);
}

fn write_h2_frame(
    out: &mut impl Write,
    kind: u8,
    flags: u8,
    stream: u32,
    payload: &[u8],
) -> Result<()> {
    if payload.len() > 16_384 {
        return Err(Error::Invalid("HTTP/2 frame too large".into()));
    }
    let mut header = [0; 9];
    header[0..3].copy_from_slice(&(payload.len() as u32).to_be_bytes()[1..]);
    header[3] = kind;
    header[4] = flags;
    header[5..9].copy_from_slice(&stream.to_be_bytes());
    out.write_all(&header)?;
    out.write_all(payload)?;
    Ok(())
}

fn read_h2_frame(input: &mut impl Read) -> Result<(u8, u8, u32, Vec<u8>)> {
    let mut header = [0; 9];
    input.read_exact(&mut header)?;
    if &header[..5] == b"\xff\xff\xffTS" {
        let size = u32::from_be_bytes(header[5..9].try_into().unwrap()) as usize;
        if size > 64 << 10 {
            return Err(Error::Transport("EarlyNoise payload too large".into()));
        }
        let mut skipped = vec![0; size];
        input.read_exact(&mut skipped)?;
        input.read_exact(&mut header)?;
    }
    let size = u32::from_be_bytes([0, header[0], header[1], header[2]]) as usize;
    if size > 64 << 10 {
        return Err(Error::Transport("HTTP/2 frame too large".into()));
    }
    let stream = u32::from_be_bytes(header[5..9].try_into().unwrap()) & 0x7fff_ffff;
    let mut payload = vec![0; size];
    input.read_exact(&mut payload)?;
    Ok((header[3], header[4], stream, payload))
}

/// Tailscale's controlbase uses the Noise IK primitives but encodes the
/// ChaCha20-Poly1305 record counter in big-endian order. Snow's default
/// ChaCha cipher uses little-endian, so its transport fails after nonce zero.
struct ControlResolver;
impl CryptoResolver for ControlResolver {
    fn resolve_rng(&self) -> Option<Box<dyn Random>> {
        DefaultResolver.resolve_rng()
    }

    fn resolve_dh(&self, choice: &DHChoice) -> Option<Box<dyn Dh>> {
        DefaultResolver.resolve_dh(choice)
    }

    fn resolve_hash(&self, choice: &HashChoice) -> Option<Box<dyn Hash>> {
        DefaultResolver.resolve_hash(choice)
    }

    fn resolve_cipher(&self, choice: &CipherChoice) -> Option<Box<dyn Cipher>> {
        match choice {
            CipherChoice::ChaChaPoly => Some(Box::<ControlCipher>::default()),
            _ => DefaultResolver.resolve_cipher(choice),
        }
    }
}

#[derive(Default)]
struct ControlCipher([u8; 32]);
impl Cipher for ControlCipher {
    fn name(&self) -> &'static str {
        "ChaChaPoly"
    }

    fn set(&mut self, key: &[u8]) {
        self.0.copy_from_slice(key);
    }

    fn encrypt(&self, nonce: u64, authtext: &[u8], plaintext: &[u8], out: &mut [u8]) -> usize {
        let mut nonce_bytes = [0; 12];
        nonce_bytes[4..].copy_from_slice(&nonce.to_be_bytes());
        out[..plaintext.len()].copy_from_slice(plaintext);
        let tag = ChaCha20Poly1305::new(&self.0.into())
            .encrypt_in_place_detached(&nonce_bytes.into(), authtext, &mut out[..plaintext.len()])
            .expect("ChaCha20-Poly1305 encryption");
        out[plaintext.len()..plaintext.len() + tag.len()].copy_from_slice(&tag);
        plaintext.len() + tag.len()
    }

    fn decrypt(
        &self,
        nonce: u64,
        authtext: &[u8],
        ciphertext: &[u8],
        out: &mut [u8],
    ) -> std::result::Result<usize, snow::Error> {
        if ciphertext.len() < 16 {
            return Err(snow::Error::Decrypt);
        }
        let mut nonce_bytes = [0; 12];
        nonce_bytes[4..].copy_from_slice(&nonce.to_be_bytes());
        let length = ciphertext.len() - 16;
        out[..length].copy_from_slice(&ciphertext[..length]);
        ChaCha20Poly1305::new(&self.0.into())
            .decrypt_in_place_detached(
                &nonce_bytes.into(),
                authtext,
                &mut out[..length],
                ciphertext[length..].into(),
            )
            .map_err(|_| snow::Error::Decrypt)?;
        Ok(length)
    }
}

impl Read for NoiseChannel {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        while self.next == self.received.len() {
            let mut header = [0; 3];
            self.socket.read_exact(&mut header)?;
            let size = u16::from_be_bytes([header[1], header[2]]) as usize;
            if header[0] != 4 || !(16..=4093).contains(&size) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "invalid Noise record frame",
                ));
            }
            let mut ciphertext = vec![0; size];
            self.socket.read_exact(&mut ciphertext)?;
            self.received.resize(size - 16, 0);
            let len = self
                .noise
                .read_message(&ciphertext, &mut self.received)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            self.received.truncate(len);
            self.next = 0;
        }
        let len = output.len().min(self.received.len() - self.next);
        output[..len].copy_from_slice(&self.received[self.next..self.next + len]);
        self.next += len;
        Ok(len)
    }
}

impl Write for NoiseChannel {
    fn write(&mut self, input: &[u8]) -> std::io::Result<usize> {
        if self.write_failed {
            return Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "Noise channel write previously failed",
            ));
        }
        if input.is_empty() {
            return Ok(0);
        }
        let size = input.len().min(4080);
        let mut ciphertext = [0; 4096];
        let len = self
            .noise
            .write_message(&input[..size], &mut ciphertext)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let mut frame = [0; 4096];
        frame[0] = 4;
        frame[1..3].copy_from_slice(&(len as u16).to_be_bytes());
        frame[3..3 + len].copy_from_slice(&ciphertext[..len]);
        if let Err(error) = self.socket.get_mut().write_all(&frame[..3 + len]) {
            self.write_failed = true;
            return Err(error);
        }
        Ok(size)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.socket.get_mut().flush()
    }
}

fn read_http_headers(socket: &mut impl BufRead) -> Result<Vec<u8>> {
    let mut headers = Vec::new();
    while !headers.ends_with(b"\r\n\r\n") {
        if headers.len() >= 16 << 10 {
            return Err(Error::Transport("control upgrade headers too large".into()));
        }
        let mut byte = [0];
        socket.read_exact(&mut byte)?;
        headers.push(byte[0]);
    }
    Ok(headers)
}

impl Drop for MemberIdentity {
    fn drop(&mut self) {
        self.private_key.fill(0);
        self.node_private_key.fill(0);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ControlKey {
    pub public_key: MachinePublic,
    pub newly_pinned: bool,
}

#[derive(Serialize, Deserialize)]
struct ControlPin {
    origin: String,
    public_key: String,
}

fn validate_origin(raw: &str) -> Result<()> {
    let uri: http::Uri = raw
        .parse()
        .map_err(|_| Error::Invalid("control URL must be an HTTP(S) origin".into()))?;
    if !matches!(uri.scheme_str(), Some("http" | "https"))
        || uri.host().is_none()
        || uri.path() != "/"
        || uri.query().is_some()
        || raw.ends_with('/')
        || raw.contains('#')
        || raw.contains('@')
    {
        return Err(Error::Invalid(
            "control URL must be a bare HTTP(S) origin".into(),
        ));
    }
    Ok(())
}

fn parse_private(raw: &str) -> Result<[u8; 32]> {
    let hex = raw
        .strip_prefix("privkey:")
        .ok_or_else(|| Error::Invalid("machine private key needs privkey: prefix".into()))?;
    let key = decode_hex_32(hex)?;
    if key == [0; 32] {
        return Err(Error::Invalid("zero machine private key".into()));
    }
    Ok(key)
}

fn open_private_key(directory: &Path, filename: &str) -> Result<[u8; 32]> {
    let path = directory.join(filename);
    if path.exists() {
        verify_private_file(&path)?;
    }
    match fs::read_to_string(&path) {
        Ok(raw) => parse_private(raw.trim_end_matches('\n')),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let mut key = [0; 32];
            getrandom::getrandom(&mut key)
                .map_err(|e| Error::Invalid(format!("key randomness: {e}")))?;
            clamp(&mut key);
            let data = format!("privkey:{}\n", encode_hex(&key));
            match create_private_file(&path, data.as_bytes()) {
                Ok(()) => Ok(key),
                Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    verify_private_file(&path)?;
                    parse_private(fs::read_to_string(&path)?.trim_end_matches('\n'))
                }
                Err(e) => Err(e),
            }
        }
        Err(e) => Err(e.into()),
    }
}

fn clamp(key: &mut [u8; 32]) {
    key[0] &= 248;
    key[31] &= 127;
    key[31] |= 64;
}

fn decode_hex_32(raw: &str) -> Result<[u8; 32]> {
    if raw.len() != 64 {
        return Err(Error::Invalid("key must contain 32 hex bytes".into()));
    }
    let mut out = [0; 32];
    for (i, chunk) in raw.as_bytes().chunks_exact(2).enumerate() {
        let h = (chunk[0] as char)
            .to_digit(16)
            .ok_or_else(|| Error::Invalid("key has non-hex characters".into()))?;
        let l = (chunk[1] as char)
            .to_digit(16)
            .ok_or_else(|| Error::Invalid("key has non-hex characters".into()))?;
        out[i] = ((h << 4) | l) as u8;
    }
    Ok(out)
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0xf) as usize] as char);
    }
    out
}

fn create_private_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path)?;
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_dir() {
        return Err(Error::Invalid(
            "member state path must be a directory".into(),
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn verify_private_file(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() {
        return Err(Error::Invalid(format!(
            "member state is not a regular file: {}",
            path.display()
        )));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(Error::Invalid(format!(
                "member state must be owner-only: {}",
                path.display()
            )));
        }
    }
    Ok(())
}

fn create_private_file(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file: File = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}
