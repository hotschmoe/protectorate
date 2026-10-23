//! Blocking, authenticated DERP v2 relay client.

use crypto_box::{
    aead::{Aead, AeadCore, OsRng},
    PublicKey, SalsaBox, SecretKey,
};
use rustls::{
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    pki_types::{CertificateDer, ServerName, UnixTime},
    ClientConfig, ClientConnection, DigitallySignedStruct, RootCertStore, SignatureScheme,
    StreamOwned,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const MAX_PACKET: usize = 64 << 10;
const MAX_FRAME: usize = 1 << 20;
const MAGIC: &[u8; 8] = b"DERP\xf0\x9f\x94\x91";

/// An exact DER leaf pin is a trust anchor for private Headscale DERP relays.
/// WebPKI still validates certificate syntax, name, validity, and TLS
/// CertificateVerify signatures. This matches Tailscale's sha256-raw CertName
/// path without accepting arbitrary certificates.
#[derive(Debug)]
struct PinnedLeafVerifier {
    pin: [u8; 32],
    signatures: Arc<rustls::client::WebPkiServerVerifier>,
}

impl ServerCertVerifier for PinnedLeafVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        if !intermediates.is_empty() {
            return Err(rustls::Error::InvalidCertificate(
                rustls::CertificateError::UnknownIssuer,
            ));
        }
        let digest = Sha256::digest(end_entity.as_ref());
        let mut difference = 0u8;
        for (actual, wanted) in digest.iter().zip(self.pin) {
            difference |= actual ^ wanted;
        }
        if difference != 0 {
            return Err(rustls::Error::InvalidCertificate(
                rustls::CertificateError::UnknownIssuer,
            ));
        }
        let mut roots = RootCertStore::empty();
        roots.add(end_entity.clone()).map_err(|_| {
            rustls::Error::InvalidCertificate(rustls::CertificateError::BadEncoding)
        })?;
        let verifier = rustls::client::WebPkiServerVerifier::builder_with_provider(
            Arc::new(roots),
            Arc::new(rustls::crypto::ring::default_provider()),
        )
        .build()
        .map_err(|_| rustls::Error::InvalidCertificate(rustls::CertificateError::BadEncoding))?;
        verifier.verify_server_cert(end_entity, &[], server_name, ocsp_response, now)
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.signatures.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.signatures.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.signatures.supported_verify_schemes()
    }
}

#[derive(Clone, Debug)]
pub struct Config {
    /// TCP endpoint to dial. TLS always uses `hostname` for SNI and validation.
    pub addr: SocketAddr,
    pub hostname: String,
    /// Additional trusted DER-encoded CA certificate, useful for a private relay.
    pub ca_der: Option<Vec<u8>>,
    /// Optional exact leaf certificate hash: `sha256-raw:<64 lowercase hex>`.
    pub certificate_pin: Option<String>,
    /// Optional independent DERP server public key pin.
    pub server_key: Option<[u8; 32]>,
    pub timeout: Duration,
}

impl Config {
    pub fn new(addr: SocketAddr, hostname: impl Into<String>) -> Self {
        Self {
            addr,
            hostname: hostname.into(),
            ca_der: None,
            certificate_pin: None,
            server_key: None,
            timeout: Duration::from_secs(10),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Event {
    Packet { source: [u8; 32], data: Vec<u8> },
    PeerGone { peer: [u8; 32], reason: u8 },
    KeepAlive,
    Pong([u8; 8]),
    Health(String),
    Restarting { reconnect_ms: u32, try_for_ms: u32 },
}

pub struct Client {
    stream: StreamOwned<ClientConnection, TcpStream>,
    public_key: [u8; 32],
    server_key: [u8; 32],
    read_timeout: Duration,
    pending: Vec<u8>,
}

impl Client {
    /// Connect, verify TLS and DERP server identity, then prove possession of
    /// the supplied registered node private key. Returns only after encrypted
    /// server info verifies the DERP key and protocol version.
    pub fn connect(config: &Config, node_private: [u8; 32]) -> io::Result<Self> {
        if config.timeout.is_zero() || config.timeout > Duration::from_secs(120) {
            return Err(invalid("DERP timeout must be 1..120 seconds"));
        }
        if config.hostname.is_empty()
            || config.hostname.len() > 253
            || config
                .hostname
                .bytes()
                .any(|b| b <= 0x20 || b == b'/' || b == b':')
        {
            return Err(invalid("invalid DERP TLS hostname"));
        }
        let expected_pin = config
            .certificate_pin
            .as_deref()
            .map(parse_pin)
            .transpose()?;
        let mut roots = RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        if let Some(der) = &config.ca_der {
            if der.is_empty() || der.len() > 1 << 20 {
                return Err(invalid("DERP CA certificate size is invalid"));
            }
            roots
                .add(der.clone().into())
                .map_err(|_| invalid("invalid DERP CA certificate"))?;
        }
        let builder =
            ClientConfig::builder_with_provider(rustls::crypto::ring::default_provider().into())
                .with_safe_default_protocol_versions()
                .map_err(io::Error::other)?;
        let tls = if config.ca_der.is_none() {
            if let Some(pin) = expected_pin {
                let signatures = rustls::client::WebPkiServerVerifier::builder_with_provider(
                    Arc::new(roots),
                    Arc::new(rustls::crypto::ring::default_provider()),
                )
                .build()
                .map_err(io::Error::other)?;
                builder
                    .dangerous()
                    .with_custom_certificate_verifier(Arc::new(PinnedLeafVerifier {
                        pin,
                        signatures,
                    }))
                    .with_no_client_auth()
            } else {
                builder.with_root_certificates(roots).with_no_client_auth()
            }
        } else {
            builder.with_root_certificates(roots).with_no_client_auth()
        };
        let name = rustls::pki_types::ServerName::try_from(config.hostname.clone())
            .map_err(|_| invalid("invalid DERP TLS hostname"))?;
        let socket = TcpStream::connect_timeout(&config.addr, config.timeout)?;
        socket.set_read_timeout(Some(config.timeout))?;
        socket.set_write_timeout(Some(config.timeout))?;
        let connection = ClientConnection::new(Arc::new(tls), name).map_err(io::Error::other)?;
        let mut stream = StreamOwned::new(connection, socket);
        while stream.conn.is_handshaking() {
            stream.conn.complete_io(&mut stream.sock)?;
        }
        if let Some(pin) = expected_pin {
            let leaf = stream
                .conn
                .peer_certificates()
                .and_then(|certs| certs.first())
                .ok_or_else(|| invalid("DERP TLS peer certificate missing"))?;
            let digest = Sha256::digest(leaf.as_ref());
            let mut difference = 0u8;
            for (actual, wanted) in digest.iter().zip(pin) {
                difference |= actual ^ wanted;
            }
            if difference != 0 {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "DERP certificate pin mismatch",
                ));
            }
        }
        let request = format!(
            "GET /derp HTTP/1.1\r\nHost: {}\r\nConnection: Upgrade\r\nUpgrade: derp\r\n\r\n",
            config.hostname
        );
        stream.write_all(request.as_bytes())?;
        let response = read_http_headers(&mut stream)?;
        let mut lines = response.split("\r\n");
        if !lines
            .next()
            .is_some_and(|line| line.starts_with("HTTP/1.1 101 "))
        {
            return Err(invalid("DERP server did not upgrade connection"));
        }
        let headers: Vec<_> = lines.filter_map(|line| line.split_once(':')).collect();
        let upgrade = headers.iter().any(|(name, value)| {
            name.eq_ignore_ascii_case("upgrade") && value.trim().eq_ignore_ascii_case("derp")
        });
        let connection = headers.iter().any(|(name, value)| {
            name.eq_ignore_ascii_case("connection")
                && value
                    .split(',')
                    .any(|token| token.trim().eq_ignore_ascii_case("upgrade"))
        });
        if !upgrade || !connection {
            return Err(invalid("DERP upgrade headers missing"));
        }
        let header_key = headers.iter().find_map(|(name, value)| {
            name.eq_ignore_ascii_case("derp-public-key")
                .then_some(value.trim())
        });
        let (kind, greeting) = read_frame(&mut stream, 1024)?;
        if kind != 0x01 || greeting.len() < 40 || &greeting[..8] != MAGIC {
            return Err(invalid("invalid DERP server greeting"));
        }
        let server_key: [u8; 32] = greeting[8..40].try_into().expect("exact slice length");
        if config
            .server_key
            .is_some_and(|expected| expected != server_key)
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "DERP server key mismatch",
            ));
        }
        if let Some(header) = header_key {
            if header != hex(&server_key) {
                return Err(invalid("DERP server key disagrees with HTTP header"));
            }
        }
        let secret = SecretKey::from(node_private);
        let public_key = *secret.public_key().as_bytes();
        let box_key = SalsaBox::new(&PublicKey::from(server_key), &secret);
        let nonce = SalsaBox::generate_nonce(&mut OsRng);
        let ciphertext = box_key
            .encrypt(&nonce, b"{\"version\":2,\"CanAckPings\":true}".as_slice())
            .map_err(|_| io::Error::other("encrypt DERP client info"))?;
        let mut client_info = Vec::with_capacity(32 + 24 + ciphertext.len());
        client_info.extend_from_slice(&public_key);
        client_info.extend_from_slice(&nonce);
        client_info.extend_from_slice(&ciphertext);
        write_frame(&mut stream, 0x02, &client_info)?;
        let (kind, server_info) = read_frame(&mut stream, MAX_FRAME)?;
        if kind != 0x03 || server_info.len() < 40 {
            return Err(invalid("missing DERP server info"));
        }
        let decrypted = box_key
            .decrypt((&server_info[..24]).into(), &server_info[24..])
            .map_err(|_| {
                io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "DERP server key proof failed",
                )
            })?;
        let info: Value = serde_json::from_slice(&decrypted)
            .map_err(|_| invalid("invalid DERP server info JSON"))?;
        if info.get("version").and_then(Value::as_u64) != Some(2) {
            return Err(invalid("unsupported DERP version"));
        }
        Ok(Self {
            stream,
            public_key,
            server_key,
            read_timeout: config.timeout,
            pending: Vec::new(),
        })
    }

    pub fn public_key(&self) -> [u8; 32] {
        self.public_key
    }
    pub fn server_key(&self) -> [u8; 32] {
        self.server_key
    }

    pub fn send(&mut self, destination: [u8; 32], packet: &[u8]) -> io::Result<()> {
        if packet.len() > MAX_PACKET {
            return Err(invalid("DERP packet too large"));
        }
        let mut payload = Vec::with_capacity(32 + packet.len());
        payload.extend_from_slice(&destination);
        payload.extend_from_slice(packet);
        write_frame(&mut self.stream, 0x04, &payload)
    }

    /// Read one event. If `allowed_source` is set, an incoming packet from
    /// another node is rejected; the caller should reconnect after this error.
    pub fn recv(&mut self, allowed_source: Option<[u8; 32]>) -> io::Result<Event> {
        self.recv_inner(self.read_timeout, allowed_source)?
            .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "DERP receive timed out"))
    }

    /// Read one event within `timeout`, returning `None` when no complete
    /// frame arrives. Partial DERP frames are retained for the next call, so
    /// the caller can alternate bounded reads and outbound sends safely.
    pub fn recv_timeout(&mut self, timeout: Duration) -> io::Result<Option<Event>> {
        self.recv_inner(timeout, None)
    }

    fn recv_inner(
        &mut self,
        timeout: Duration,
        allowed_source: Option<[u8; 32]>,
    ) -> io::Result<Option<Event>> {
        if timeout > Duration::from_secs(120) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "DERP receive timeout exceeds 120 seconds",
            ));
        }
        let deadline = Instant::now() + timeout;
        loop {
            let Some((kind, payload)) = self.read_next_frame(deadline)? else {
                return Ok(None);
            };
            let event = match kind {
                0x05 if (32..=32 + MAX_PACKET).contains(&payload.len()) => {
                    let source: [u8; 32] = payload[..32].try_into().expect("exact slice length");
                    if allowed_source.is_some_and(|peer| peer != source) {
                        return Err(io::Error::new(
                            io::ErrorKind::PermissionDenied,
                            "DERP packet source is not authorized",
                        ));
                    }
                    Event::Packet {
                        source,
                        data: payload[32..].to_vec(),
                    }
                }
                0x08 if payload.len() >= 32 => {
                    let peer = payload[..32].try_into().expect("exact slice length");
                    Event::PeerGone {
                        peer,
                        reason: payload.get(32).copied().unwrap_or(0),
                    }
                }
                0x06 if payload.is_empty() => Event::KeepAlive,
                0x12 if payload.len() == 8 => {
                    write_frame(&mut self.stream, 0x13, &payload)?;
                    continue;
                }
                0x13 if payload.len() == 8 => {
                    Event::Pong(payload.try_into().expect("exact slice length"))
                }
                0x14 => Event::Health(
                    String::from_utf8(payload).map_err(|_| invalid("invalid DERP health text"))?,
                ),
                0x15 if payload.len() == 8 => Event::Restarting {
                    reconnect_ms: u32::from_be_bytes(
                        payload[..4].try_into().expect("exact slice length"),
                    ),
                    try_for_ms: u32::from_be_bytes(
                        payload[4..].try_into().expect("exact slice length"),
                    ),
                },
                _ => return Err(invalid("invalid or unsupported DERP frame")),
            };
            return Ok(Some(event));
        }
    }

    fn read_next_frame(&mut self, deadline: Instant) -> io::Result<Option<(u8, Vec<u8>)>> {
        loop {
            if let Some(frame) = take_buffered_frame(&mut self.pending, MAX_FRAME)? {
                return Ok(Some(frame));
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Ok(None);
            }
            self.stream.sock.set_read_timeout(Some(remaining))?;
            let mut chunk = [0u8; 8192];
            let limit = (MAX_FRAME + 5 - self.pending.len()).min(chunk.len());
            match self.stream.read(&mut chunk[..limit]) {
                Ok(0) => {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "DERP connection closed",
                    ))
                }
                Ok(count) => self.pending.extend_from_slice(&chunk[..count]),
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                    ) =>
                {
                    return Ok(None)
                }
                Err(error) => return Err(error),
            }
        }
    }

    pub fn ping(&mut self, payload: [u8; 8]) -> io::Result<()> {
        write_frame(&mut self.stream, 0x12, &payload)
    }
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn read_http_headers(reader: &mut impl Read) -> io::Result<String> {
    let mut bytes = Vec::new();
    while bytes.len() < 8192 {
        let mut byte = [0];
        reader.read_exact(&mut byte)?;
        bytes.push(byte[0]);
        if bytes.ends_with(b"\r\n\r\n") {
            return String::from_utf8(bytes).map_err(|_| invalid("invalid DERP HTTP response"));
        }
    }
    Err(invalid("oversized DERP HTTP response"))
}

fn read_frame(reader: &mut impl Read, max: usize) -> io::Result<(u8, Vec<u8>)> {
    let mut header = [0; 5];
    reader.read_exact(&mut header)?;
    let length = u32::from_be_bytes(header[1..].try_into().expect("exact slice length")) as usize;
    if length > max {
        return Err(invalid("DERP frame too large"));
    }
    let mut payload = vec![0; length];
    reader.read_exact(&mut payload)?;
    Ok((header[0], payload))
}

fn take_buffered_frame(pending: &mut Vec<u8>, max: usize) -> io::Result<Option<(u8, Vec<u8>)>> {
    if pending.len() < 5 {
        return Ok(None);
    }
    let length = u32::from_be_bytes(pending[1..5].try_into().expect("exact slice length")) as usize;
    if length > max {
        return Err(invalid("DERP frame too large"));
    }
    if pending.len() < length + 5 {
        return Ok(None);
    }
    let kind = pending[0];
    let payload = pending[5..length + 5].to_vec();
    pending.drain(..length + 5);
    Ok(Some((kind, payload)))
}

fn write_frame(writer: &mut impl Write, kind: u8, payload: &[u8]) -> io::Result<()> {
    writer.write_all(&[kind])?;
    writer.write_all(&(payload.len() as u32).to_be_bytes())?;
    writer.write_all(payload)
}

fn parse_pin(pin: &str) -> io::Result<[u8; 32]> {
    let hex = pin
        .strip_prefix("sha256-raw:")
        .ok_or_else(|| invalid("invalid DERP certificate pin"))?;
    if hex.len() != 64 {
        return Err(invalid("invalid DERP certificate pin"));
    }
    let mut bytes = [0; 32];
    for (index, pair) in hex.as_bytes().chunks_exact(2).enumerate() {
        let high = nibble(pair[0]).ok_or_else(|| invalid("invalid DERP certificate pin"))?;
        let low = nibble(pair[1]).ok_or_else(|| invalid("invalid DERP certificate pin"))?;
        bytes[index] = (high << 4) | low;
    }
    Ok(bytes)
}

fn nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8] = b"0123456789abcdef";
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        result.push(DIGITS[(byte >> 4) as usize] as char);
        result.push(DIGITS[(byte & 15) as usize] as char);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use protectorate_derp::{Certificate, Config as ServerConfig, Service};
    use std::io::BufRead;
    use std::net::TcpListener;
    use std::thread;

    fn verifier(connections: usize) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/verify", listener.local_addr().unwrap());
        let worker = thread::spawn(move || {
            for _ in 0..connections {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut reader = io::BufReader::new(&mut stream);
                let mut first = String::new();
                reader.read_line(&mut first).unwrap();
                assert!(first.starts_with("POST /verify "));
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" {
                        break;
                    }
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length: ")
                    {
                        length = value.trim().parse().unwrap();
                    }
                }
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                assert!(String::from_utf8_lossy(&body).contains("nodekey:"));
                drop(reader);
                stream
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 14\r\n\r\n{\"Allow\":true}")
                    .unwrap();
            }
        });
        (url, worker)
    }

    #[test]
    fn tls_authenticated_peers_relay_and_bind_sources() {
        let dir = tempfile::tempdir().unwrap();
        let (verify_url, worker) = verifier(2);
        let service = Service::start(ServerConfig {
            listen: "127.0.0.1:0".parse().unwrap(),
            stun_listen: "127.0.0.1:0".parse().unwrap(),
            hostname: "localhost".into(),
            verify_url,
            state_dir: dir.path().to_owned(),
        })
        .unwrap();
        let cert = Certificate::load_or_create(dir.path(), "localhost").unwrap();
        let mut config = Config::new(service.derp_addr, "localhost");
        config.ca_der = Some(cert.der);
        config.certificate_pin = Some(service.certificate_pin.clone());
        let secret_a = SecretKey::generate(&mut OsRng);
        let mut a = Client::connect(&config, secret_a.to_bytes()).unwrap();
        config.server_key = Some(a.server_key());
        let secret_b = SecretKey::generate(&mut OsRng);
        let mut b = Client::connect(&config, secret_b.to_bytes()).unwrap();
        assert_eq!(a.public_key(), *secret_a.public_key().as_bytes());
        assert_eq!(b.public_key(), *secret_b.public_key().as_bytes());
        assert_eq!(a.recv_timeout(Duration::from_millis(25)).unwrap(), None);
        a.send(b.public_key(), b"wireguard packet").unwrap();
        assert_eq!(
            b.recv_timeout(Duration::from_secs(2)).unwrap(),
            Some(Event::Packet {
                source: a.public_key(),
                data: b"wireguard packet".to_vec()
            })
        );
        a.send(b.public_key(), b"second packet").unwrap();
        assert_eq!(
            b.recv(Some([0x55; 32])).unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
        a.ping(*b"12345678").unwrap();
        assert_eq!(a.recv(None).unwrap(), Event::Pong(*b"12345678"));
        worker.join().unwrap();
    }

    #[test]
    fn pin_and_hostname_are_checked_before_derp_exchange() {
        let dir = tempfile::tempdir().unwrap();
        let service = Service::start(ServerConfig {
            listen: "127.0.0.1:0".parse().unwrap(),
            stun_listen: "127.0.0.1:0".parse().unwrap(),
            hostname: "localhost".into(),
            verify_url: "http://127.0.0.1:9/verify".into(),
            state_dir: dir.path().to_owned(),
        })
        .unwrap();
        let cert = Certificate::load_or_create(dir.path(), "localhost").unwrap();
        let mut config = Config::new(service.derp_addr, "localhost");
        config.ca_der = Some(cert.der);
        config.certificate_pin = Some(format!("sha256-raw:{}", "0".repeat(64)));
        assert_eq!(
            Client::connect(&config, [7; 32]).err().unwrap().kind(),
            io::ErrorKind::PermissionDenied
        );
        config.hostname = "wrong.example".into();
        config.certificate_pin = None;
        assert!(Client::connect(&config, [7; 32]).is_err());
        config.certificate_pin = Some("sha256-raw:invalid".into());
        assert_eq!(
            Client::connect(&config, [7; 32]).err().unwrap().kind(),
            io::ErrorKind::InvalidData
        );
        config.hostname = "localhost".into();
        config.certificate_pin = Some(service.certificate_pin.clone());
        config.server_key = Some([0x55; 32]);
        assert_eq!(
            Client::connect(&config, [7; 32]).err().unwrap().kind(),
            io::ErrorKind::PermissionDenied
        );
    }

    #[test]
    fn headscale_leaf_pin_authenticates_self_signed_relay_without_ca() {
        let dir = tempfile::tempdir().unwrap();
        let (verify_url, worker) = verifier(1);
        let service = Service::start(ServerConfig {
            listen: "127.0.0.1:0".parse().unwrap(),
            stun_listen: "127.0.0.1:0".parse().unwrap(),
            hostname: "localhost".into(),
            verify_url,
            state_dir: dir.path().to_owned(),
        })
        .unwrap();
        let mut config = Config::new(service.derp_addr, "localhost");
        config.certificate_pin = Some(format!("sha256-raw:{}", "0".repeat(64)));
        assert!(Client::connect(&config, [8; 32]).is_err());
        config.certificate_pin = Some(service.certificate_pin.clone());
        config.hostname = "wrong.example".into();
        assert!(Client::connect(&config, [8; 32]).is_err());
        config.hostname = "localhost".into();
        let client = Client::connect(&config, [8; 32]).unwrap();
        assert_eq!(
            client.public_key(),
            *SecretKey::from([8; 32]).public_key().as_bytes()
        );
        worker.join().unwrap();
    }

    #[test]
    fn buffered_frame_preserves_partial_header_and_payload() {
        let mut pending = vec![0x05, 0, 0];
        assert!(take_buffered_frame(&mut pending, MAX_FRAME)
            .unwrap()
            .is_none());
        pending.extend_from_slice(&[0, 4, 1, 2]);
        assert!(take_buffered_frame(&mut pending, MAX_FRAME)
            .unwrap()
            .is_none());
        pending.extend_from_slice(&[3, 4]);
        assert_eq!(
            take_buffered_frame(&mut pending, MAX_FRAME).unwrap(),
            Some((0x05, vec![1, 2, 3, 4]))
        );
        assert!(pending.is_empty());
    }
}
