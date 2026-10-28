//! The hub's private CA, the server certificate it issues and the address rule
//! every listener obeys. Network location is never identity: devices pin this
//! CA when they pair.
use rcgen::{
    BasicConstraints, CertificateParams, DistinguishedName, DnType, ExtendedKeyUsagePurpose, IsCa,
    Issuer, KeyPair, KeyUsagePurpose,
};
use rustls::{
    crypto::{ring, CryptoProvider},
    pki_types::{pem::PemObject, CertificateDer, PrivateKeyDer},
    ServerConfig,
};
use std::{
    fs, io,
    net::{IpAddr, SocketAddr},
    path::Path,
    sync::Arc,
    time::Duration,
};

/// The name every hub certificate carries.
pub const HUB_NAME: &str = "hub.protectorate.internal";
const LEAF_LIFETIME: Duration = Duration::from_secs(365 * 24 * 3600);
const CA_LIFETIME: Duration = Duration::from_secs(20 * 365 * 24 * 3600);
const MAX_PEM: u64 = 64 << 10;

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
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
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

    /// A fresh key and certificate for the hub's TLS listeners, naming
    /// `HUB_NAME` and the given addresses.
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

fn http1(mut config: ServerConfig) -> Arc<ServerConfig> {
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Arc::new(config)
}

/// The hub's listener configuration. Clients authenticate with tokens or
/// device keys, never with certificates.
pub fn hub_server_config(identity: &Identity) -> io::Result<Arc<ServerConfig>> {
    let key = PrivateKeyDer::from_pem_slice(identity.key_pem.as_bytes()).map_err(invalid)?;
    let config = ServerConfig::builder_with_provider(provider())
        .with_safe_default_protocol_versions()
        .map_err(invalid)?
        .with_no_client_auth()
        .with_single_cert(certs(identity.cert_pem.as_bytes())?, key)
        .map_err(invalid)?;
    Ok(http1(config))
}
