//! The hub's private CA, the certificates it issues and the address rule every
//! listener and node address obeys. Network location is never identity: the
//! hub and its nodes authenticate each other with these certificates.
use rcgen::{
    BasicConstraints, CertificateParams, DistinguishedName, DnType, ExtendedKeyUsagePurpose, IsCa,
    Issuer, KeyPair, KeyUsagePurpose, PublicKeyData, SignatureAlgorithm, PKCS_ECDSA_P256_SHA256,
};
use rustls::{
    client::danger::HandshakeSignatureValid,
    crypto::{ring, CryptoProvider},
    pki_types::{pem::PemObject, CertificateDer, PrivateKeyDer, ServerName, UnixTime},
    server::{
        danger::{ClientCertVerified, ClientCertVerifier},
        ClientHello, ResolvesServerCert, WebPkiClientVerifier,
    },
    sign::{CertifiedKey, SigningKey},
    CertificateError, DigitallySignedStruct, RootCertStore, ServerConfig, SignatureScheme,
};
use std::{
    fs, io,
    net::{IpAddr, SocketAddr},
    path::Path,
    sync::{Arc, RwLock},
    time::Duration,
};

/// The name every hub certificate carries. Node certificates are named under
/// `node.protectorate.internal`, so no node can present it.
pub const HUB_NAME: &str = "hub.protectorate.internal";
const NODE_DOMAIN: &str = "node.protectorate.internal";
const LEAF_LIFETIME: Duration = Duration::from_secs(365 * 24 * 3600);
const CA_LIFETIME: Duration = Duration::from_secs(20 * 365 * 24 * 3600);
/// The hub renews a node certificate once less than this is left.
pub const RENEW_BEFORE: Duration = Duration::from_secs(30 * 24 * 3600);
const MAX_PEM: u64 = 64 << 10;

/// The DNS name in a node's certificate. `id` is new on every join, so a
/// certificate from an earlier enrollment under the same name never matches.
pub fn node_name(name: &str, id: &str) -> String {
    format!("{id}.{name}.{NODE_DOMAIN}")
}

/// Loopback, RFC 1918, shared address space (100.64.0.0/10), unique local
/// (fc00::/7) and link-local IPv6 (fe80::/10).
pub fn private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let [a, b, ..] = v4.octets();
            v4.is_loopback() || v4.is_private() || (a == 100 && b & 0xc0 == 64)
        }
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => private_ip(IpAddr::V4(v4)),
            None => v6.is_loopback() || v6.is_unique_local() || v6.is_unicast_link_local(),
        },
    }
}

/// Listeners bind explicit private addresses only. There is no override.
pub fn check_listen(addr: SocketAddr) -> io::Result<()> {
    if addr.ip().is_unspecified() || !private_ip(addr.ip()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("refusing to listen on {addr}: only an explicit loopback, RFC 1918, 100.64.0.0/10, fc00::/7 or fe80::/10 address is allowed"),
        ));
    }
    Ok(())
}

fn invalid(error: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error.to_string())
}

fn provider() -> Arc<CryptoProvider> {
    Arc::new(ring::default_provider())
}

fn validity(params: &mut CertificateParams, lifetime: Duration) {
    let now = time::OffsetDateTime::now_utc();
    params.not_before = now - Duration::from_secs(3600);
    params.not_after = now + lifetime;
}

fn ca_params() -> CertificateParams {
    let mut params = CertificateParams::default();
    let mut name = DistinguishedName::new();
    name.push(DnType::CommonName, "Protectorate hub CA");
    params.distinguished_name = name;
    params.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
    params.key_usages = vec![
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::DigitalSignature,
    ];
    params
}

fn leaf(names: Vec<String>, lifetime: Duration) -> io::Result<CertificateParams> {
    let mut params = CertificateParams::new(names.clone()).map_err(invalid)?;
    let mut subject = DistinguishedName::new();
    subject.push(DnType::CommonName, names[0].as_str());
    params.distinguished_name = subject;
    params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    params.extended_key_usages = vec![
        ExtendedKeyUsagePurpose::ServerAuth,
        ExtendedKeyUsagePurpose::ClientAuth,
    ];
    params.use_authority_key_identifier_extension = true;
    validity(&mut params, lifetime);
    Ok(params)
}

/// A certificate chain and its private key, both PEM.
pub struct Identity {
    pub cert_pem: String,
    pub key_pem: String,
}

/// The hub's certificate authority. Its key never leaves the hub home.
pub struct Ca {
    issuer: Issuer<'static, KeyPair>,
    pem: String,
}

impl Ca {
    /// Opens `dir/ca.pem` and `dir/ca-key.pem` (mode 0600), creating both on
    /// first use.
    pub fn open_or_create(dir: &Path) -> io::Result<Self> {
        let key_path = dir.join("ca-key.pem");
        let cert_path = dir.join("ca.pem");
        if key_path.try_exists()? {
            let key = KeyPair::from_pem(&read_private(&key_path)?).map_err(invalid)?;
            let pem = String::from_utf8(read_pem(&cert_path)?).map_err(invalid)?;
            return Ok(Self {
                issuer: Issuer::new(ca_params(), key),
                pem,
            });
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(dir)?;
        }
        #[cfg(not(unix))]
        fs::create_dir_all(dir)?;
        let key = KeyPair::generate().map_err(invalid)?;
        let mut params = ca_params();
        validity(&mut params, CA_LIFETIME);
        let pem = params.self_signed(&key).map_err(invalid)?.pem();
        // The certificate goes first: a crash before the key is written
        // leaves no key, so the next start creates a fresh pair.
        protectorate_fsutil::write_file(&cert_path, pem.as_bytes(), 0o644)?;
        protectorate_fsutil::write_file(&key_path, key.serialize_pem().as_bytes(), 0o600)?;
        Ok(Self {
            issuer: Issuer::new(ca_params(), key),
            pem,
        })
    }

    pub fn pem(&self) -> &str {
        &self.pem
    }

    /// A fresh key and certificate for the hub's TLS listeners and for its
    /// client role towards nodes, naming `HUB_NAME` and the given addresses.
    pub fn hub_identity(&self, addresses: &[String]) -> io::Result<Identity> {
        let key = KeyPair::generate().map_err(invalid)?;
        let mut names = vec![HUB_NAME.to_owned()];
        names.extend(addresses.iter().cloned());
        let cert = leaf(names, LEAF_LIFETIME)?
            .signed_by(&key, &self.issuer)
            .map_err(invalid)?;
        Ok(Identity {
            cert_pem: cert.pem(),
            key_pem: key.serialize_pem(),
        })
    }

    /// A certificate for a node's own P-256 key. Returns the PEM and its
    /// expiry in Unix seconds.
    pub fn issue_node(&self, name: &str, id: &str, public_key: &[u8]) -> io::Result<(String, i64)> {
        let key = P256(public_key);
        let cert = leaf(vec![node_name(name, id)], LEAF_LIFETIME)?
            .signed_by(&key, &self.issuer)
            .map_err(invalid)?;
        Ok((
            cert.pem(),
            protectorate_fsutil::unix_seconds() + LEAF_LIFETIME.as_secs() as i64,
        ))
    }
}

struct P256<'a>(&'a [u8]);
impl PublicKeyData for P256<'_> {
    fn der_bytes(&self) -> &[u8] {
        self.0
    }
    fn algorithm(&self) -> &'static SignatureAlgorithm {
        &PKCS_ECDSA_P256_SHA256
    }
}

/// The message a joining node signs with its new key.
pub fn join_message(name: &str, code: &str) -> Vec<u8> {
    format!("protectorate-join:{name}:{code}").into_bytes()
}

/// Checks that `proof` signs `message` with the uncompressed P-256 point
/// `public_key`, which also proves the point is valid.
pub fn verify_proof(public_key: &[u8], message: &[u8], proof: &[u8]) -> bool {
    public_key.len() == 65
        && ::ring::signature::UnparsedPublicKey::new(
            &::ring::signature::ECDSA_P256_SHA256_ASN1,
            public_key,
        )
        .verify(message, proof)
        .is_ok()
}

/// A new node key as PKCS#8 PEM, its public point and its signature over
/// `message`.
pub fn node_key(message: &[u8]) -> io::Result<(String, Vec<u8>, Vec<u8>)> {
    let key = KeyPair::generate().map_err(invalid)?;
    let proof = rcgen::SigningKey::sign(&key, message).map_err(invalid)?;
    Ok((key.serialize_pem(), key.public_key_raw().to_vec(), proof))
}

/// Reads a bounded PEM file without following a final symlink.
pub fn read_pem(path: &Path) -> io::Result<Vec<u8>> {
    use std::io::Read;
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > MAX_PEM {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{} must be a regular PEM file under 64 KiB", path.display()),
        ));
    }
    let mut pem = Vec::new();
    file.take(MAX_PEM).read_to_end(&mut pem)?;
    Ok(pem)
}

/// Reads a PEM key that must be readable by its owner only.
pub fn read_private(path: &Path) -> io::Result<String> {
    let pem = read_pem(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if fs::metadata(path)?.permissions().mode() & 0o077 != 0 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("{} must be readable only by its owner", path.display()),
            ));
        }
    }
    String::from_utf8(pem).map_err(invalid)
}

fn certs(pem: &[u8]) -> io::Result<Vec<CertificateDer<'static>>> {
    let certs = CertificateDer::pem_slice_iter(pem)
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid)?;
    if certs.is_empty() {
        return Err(invalid("PEM holds no certificate"));
    }
    Ok(certs)
}

fn signing_key(pem: &[u8]) -> io::Result<Arc<dyn SigningKey>> {
    let key = PrivateKeyDer::from_pem_slice(pem).map_err(invalid)?;
    ring::sign::any_supported_type(&key).map_err(invalid)
}

fn http1(mut config: ServerConfig) -> Arc<ServerConfig> {
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Arc::new(config)
}

/// The hub's listener configuration. Clients authenticate with tokens or
/// device keys; a node may also present the certificate the hub CA issued it,
/// which authenticates only the calls a node makes about itself.
pub fn hub_server_config(identity: &Identity, ca_pem: &[u8]) -> io::Result<Arc<ServerConfig>> {
    let key = PrivateKeyDer::from_pem_slice(identity.key_pem.as_bytes()).map_err(invalid)?;
    let mut roots = RootCertStore::empty();
    for cert in certs(ca_pem)? {
        roots.add(cert).map_err(invalid)?;
    }
    let clients = WebPkiClientVerifier::builder_with_provider(Arc::new(roots), provider())
        .allow_unauthenticated()
        .build()
        .map_err(invalid)?;
    let config = ServerConfig::builder_with_provider(provider())
        .with_safe_default_protocol_versions()
        .map_err(invalid)?
        .with_client_cert_verifier(clients)
        .with_single_cert(certs(identity.cert_pem.as_bytes())?, key)
        .map_err(invalid)?;
    Ok(http1(config))
}

/// Whether `cert_der` is the certificate `Ca::issue_node` issues for node
/// `name` under enrollment `id`. The caller has already verified the chain.
pub fn is_node_cert(cert_der: &[u8], name: &str, id: &str) -> bool {
    let Ok(dns) = ServerName::try_from(node_name(name, id)) else {
        return false;
    };
    let cert = CertificateDer::from(cert_der);
    webpki::EndEntityCert::try_from(&cert)
        .and_then(|cert| cert.verify_is_valid_for_subject_name(&dns))
        .is_ok()
}

/// A node's TLS state: its certificate, replaced in place on renewal, and the
/// hub CA that every client certificate must chain to.
#[derive(Debug)]
pub struct NodeCert {
    key: Arc<dyn SigningKey>,
    current: RwLock<Arc<CertifiedKey>>,
    chain: Arc<dyn ClientCertVerifier>,
}

impl NodeCert {
    pub fn new(ca_pem: &[u8], cert_pem: &[u8], key_pem: &[u8]) -> io::Result<Self> {
        let mut roots = RootCertStore::empty();
        for cert in certs(ca_pem)? {
            roots.add(cert).map_err(invalid)?;
        }
        let chain = WebPkiClientVerifier::builder_with_provider(Arc::new(roots), provider())
            .build()
            .map_err(invalid)?;
        let key = signing_key(key_pem)?;
        let current = Self::check(&chain, &key, cert_pem)?;
        Ok(Self {
            key,
            current: RwLock::new(current),
            chain,
        })
    }

    fn check(
        chain: &Arc<dyn ClientCertVerifier>,
        key: &Arc<dyn SigningKey>,
        cert_pem: &[u8],
    ) -> io::Result<Arc<CertifiedKey>> {
        let certs = certs(cert_pem)?;
        chain
            .verify_client_cert(&certs[0], &certs[1..], UnixTime::now())
            .map_err(invalid)?;
        let certified = CertifiedKey::new(certs, Arc::clone(key));
        certified.keys_match().map_err(invalid)?;
        Ok(Arc::new(certified))
    }

    /// Serves `cert_pem` from the next handshake on, if it chains to the hub
    /// CA and certifies this node's key.
    pub fn replace(&self, cert_pem: &[u8]) -> io::Result<()> {
        let next = Self::check(&self.chain, &self.key, cert_pem)?;
        *self.current.write().unwrap_or_else(|e| e.into_inner()) = next;
        Ok(())
    }

    /// Requires every client to present a hub certificate from the CA.
    pub fn server_config(self: &Arc<Self>) -> io::Result<Arc<ServerConfig>> {
        let config = ServerConfig::builder_with_provider(provider())
            .with_safe_default_protocol_versions()
            .map_err(invalid)?
            .with_client_cert_verifier(Arc::new(HubOnly(Arc::clone(&self.chain))))
            .with_cert_resolver(Arc::clone(self) as Arc<dyn ResolvesServerCert>);
        Ok(http1(config))
    }
}

impl ResolvesServerCert for NodeCert {
    fn resolve(&self, _: ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
        Some(Arc::clone(
            &self.current.read().unwrap_or_else(|e| e.into_inner()),
        ))
    }
}

/// Accepts a client certificate only if it chains to the hub CA and names
/// `HUB_NAME`, so another node's certificate is refused.
#[derive(Debug)]
struct HubOnly(Arc<dyn ClientCertVerifier>);

impl ClientCertVerifier for HubOnly {
    fn root_hint_subjects(&self) -> &[rustls::DistinguishedName] {
        self.0.root_hint_subjects()
    }

    fn verify_client_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        now: UnixTime,
    ) -> Result<ClientCertVerified, rustls::Error> {
        self.0.verify_client_cert(end_entity, intermediates, now)?;
        let hub = ServerName::try_from(HUB_NAME).expect("hub name is a DNS name");
        webpki::EndEntityCert::try_from(end_entity)
            .and_then(|cert| cert.verify_is_valid_for_subject_name(&hub))
            .map_err(|_| rustls::Error::InvalidCertificate(CertificateError::NotValidForName))?;
        Ok(ClientCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.0.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.0.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.supported_verify_schemes()
    }
}
