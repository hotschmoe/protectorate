use crate::hex;
use rcgen::generate_simple_self_signed;
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

pub struct Certificate {
    pub der: Vec<u8>,
    pub key_der: Vec<u8>,
}

impl Certificate {
    pub fn load_or_create(dir: &Path, host: &str) -> io::Result<Self> {
        if host.is_empty() || host.contains(['\n', '\r']) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid DERP hostname",
            ));
        }
        fs::create_dir_all(dir)?;
        let cert_path = dir.join("derp_cert.der");
        let key_path = dir.join("derp_cert_key.der");
        let host_path = dir.join("derp_cert_host");
        if let (Ok(der), Ok(key_der), Ok(saved_host)) = (
            fs::read(&cert_path),
            fs::read(&key_path),
            fs::read_to_string(&host_path),
        ) {
            let cert = Self { der, key_der };
            if saved_host == host && cert.matches_key() {
                return Ok(cert);
            }
        }
        let generated =
            generate_simple_self_signed(vec![host.to_owned()]).map_err(io::Error::other)?;
        let der = generated.cert.der().to_vec();
        let key_der = generated.signing_key.serialize_der();
        replace_private(&key_path, &key_der)?;
        replace_private(&cert_path, &der)?;
        replace_private(&host_path, host.as_bytes())?;
        Ok(Self { der, key_der })
    }

    fn matches_key(&self) -> bool {
        rustls::ServerConfig::builder_with_provider(rustls::crypto::ring::default_provider().into())
            .with_safe_default_protocol_versions()
            .expect("ring supports safe protocol versions")
            .with_no_client_auth()
            .with_single_cert(
                vec![self.der.clone().into()],
                rustls::pki_types::PrivateKeyDer::Pkcs8(self.key_der.clone().into()),
            )
            .is_ok()
    }

    pub fn pin(&self) -> String {
        format!("sha256-raw:{}", hex(&Sha256::digest(&self.der)))
    }
}

fn replace_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let suffix = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
    let temp = path.with_extension(format!("new-{}-{suffix}", std::process::id()));
    write_private(&temp, bytes)?;
    let result = fs::rename(&temp, path);
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

pub(crate) fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn certificate_recovers_partial_state_and_host_change() {
        let dir = tempfile::tempdir().unwrap();
        let first = Certificate::load_or_create(dir.path(), "localhost").unwrap();
        let again = Certificate::load_or_create(dir.path(), "localhost").unwrap();
        assert_eq!(first.pin(), again.pin());
        fs::remove_file(dir.path().join("derp_cert.der")).unwrap();
        let recovered = Certificate::load_or_create(dir.path(), "localhost").unwrap();
        assert_ne!(first.pin(), recovered.pin());
        let changed = Certificate::load_or_create(dir.path(), "127.0.0.1").unwrap();
        assert_ne!(recovered.pin(), changed.pin());
        assert_eq!(
            fs::read_to_string(dir.path().join("derp_cert_host")).unwrap(),
            "127.0.0.1"
        );
        fs::write(dir.path().join("derp_cert_key.der"), b"bad key").unwrap();
        let repaired = Certificate::load_or_create(dir.path(), "127.0.0.1").unwrap();
        assert_ne!(changed.pin(), repaired.pin());
    }
}
