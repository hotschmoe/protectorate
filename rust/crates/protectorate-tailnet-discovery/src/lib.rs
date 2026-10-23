//! Durable X25519 disco identity for Tailscale control-plane map requests.
//!
//! Tailscale v1.104.1 `types/key/disco.go` uses a clamped X25519 private
//! key and serializes the public key as `discokey:` followed by 32 hex bytes.
//! This crate supplies identity only; it does not implement disco packets.

use curve25519_dalek::montgomery::MontgomeryPoint;
use std::{
    fmt, fs,
    fs::OpenOptions,
    io::{self, Read, Write},
    path::Path,
};

const FILENAME: &str = "disco.key";
const PRIVATE_PREFIX: &str = "privkey:";
const PUBLIC_PREFIX: &str = "discokey:";
const MAX_FILE: u64 = 80;

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Invalid(&'static str),
    Randomness(getrandom::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "disco identity: {e}"),
            Self::Invalid(e) => write!(f, "disco identity: {e}"),
            Self::Randomness(e) => write!(f, "disco identity randomness: {e}"),
        }
    }
}
impl std::error::Error for Error {}
impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// Public key accepted by Headscale and Tailscale's `tailcfg.MapRequest`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiscoPublic([u8; 32]);

impl DiscoPublic {
    pub fn parse(raw: &str) -> Result<Self> {
        let bytes = parse_hex_key(raw, PUBLIC_PREFIX)?;
        if bytes == [0; 32] {
            return Err(Error::Invalid("zero public key"));
        }
        Ok(Self(bytes))
    }

    pub fn bytes(self) -> [u8; 32] {
        self.0
    }
}

impl fmt::Display for DiscoPublic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(PUBLIC_PREFIX)?;
        write_hex(f, &self.0)
    }
}

/// Private key is never included in `Debug` output or serialized to a map.
pub struct DiscoIdentity {
    private: [u8; 32],
}

impl fmt::Debug for DiscoIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DiscoIdentity")
            .field("public", &self.public_key())
            .finish_non_exhaustive()
    }
}

impl DiscoIdentity {
    /// Open or create `directory/disco.key`. Creation is atomic across
    /// competing processes; a corrupt existing key is rejected, not replaced.
    pub fn open(directory: impl AsRef<Path>) -> Result<Self> {
        let directory = directory.as_ref();
        fs::create_dir_all(directory)?;
        verify_directory(directory)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(directory, fs::Permissions::from_mode(0o700))?;
        }
        let path = directory.join(FILENAME);
        match read_key(&path) {
            Ok(private) => Ok(Self { private }),
            Err(Error::Io(e)) if e.kind() == io::ErrorKind::NotFound => {
                let mut private = [0; 32];
                getrandom::getrandom(&mut private).map_err(Error::Randomness)?;
                clamp(&mut private);
                let mut payload = Vec::with_capacity(73);
                payload.extend_from_slice(PRIVATE_PREFIX.as_bytes());
                append_hex(&mut payload, &private);
                payload.push(b'\n');
                let result = create_key(directory, &path, &payload);
                payload.fill(0);
                private.fill(0);
                result?;
                Ok(Self {
                    private: read_key(&path)?,
                })
            }
            Err(e) => Err(e),
        }
    }

    pub fn public_key(&self) -> DiscoPublic {
        DiscoPublic(MontgomeryPoint::mul_base_clamped(self.private).to_bytes())
    }

    /// The private scalar for a future authenticated disco transport. Keep
    /// returned copies out of logs and clear them when their owner is dropped.
    pub fn private_key_bytes(&self) -> [u8; 32] {
        self.private
    }
}

impl Drop for DiscoIdentity {
    fn drop(&mut self) {
        self.private.fill(0);
    }
}

fn read_key(path: &Path) -> Result<[u8; 32]> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() {
        return Err(Error::Invalid("key path is not a regular file"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(Error::Invalid("key file must be owner-only"));
        }
    }
    let mut file = OpenOptions::new().read(true).open(path)?;
    let mut data = Vec::new();
    Read::by_ref(&mut file)
        .take(MAX_FILE + 1)
        .read_to_end(&mut data)?;
    let parsed = if data.len() as u64 > MAX_FILE {
        Err(Error::Invalid("key file is too large"))
    } else if data.last() != Some(&b'\n') {
        Err(Error::Invalid("key file is missing final newline"))
    } else {
        let raw = std::str::from_utf8(&data[..data.len() - 1])
            .map_err(|_| Error::Invalid("key file is not UTF-8"))?;
        let key = parse_hex_key(raw, PRIVATE_PREFIX)?;
        if key == [0; 32] || !is_clamped(&key) {
            Err(Error::Invalid("private key is zero or not clamped"))
        } else {
            Ok(key)
        }
    };
    data.fill(0);
    parsed
}

fn create_key(directory: &Path, path: &Path, payload: &[u8]) -> Result<()> {
    let mut suffix = [0u8; 8];
    getrandom::getrandom(&mut suffix).map_err(Error::Randomness)?;
    let temp = directory.join(format!(".disco-key-{}.tmp", hex(&suffix)));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temp)?;
    let result = (|| {
        file.write_all(payload)?;
        file.sync_all()?;
        match fs::hard_link(&temp, path) {
            Ok(()) => {
                sync_directory(directory)?;
                Ok(())
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(()),
            Err(e) => Err(e),
        }
    })();
    let _ = fs::remove_file(&temp);
    result.map_err(Error::Io)
}

#[cfg(unix)]
fn sync_directory(directory: &Path) -> io::Result<()> {
    fs::File::open(directory)?.sync_all()
}
#[cfg(not(unix))]
fn sync_directory(_directory: &Path) -> io::Result<()> {
    Ok(())
}

fn verify_directory(path: &Path) -> Result<()> {
    if !fs::symlink_metadata(path)?.file_type().is_dir() {
        return Err(Error::Invalid("state path is not a directory"));
    }
    Ok(())
}

fn clamp(key: &mut [u8; 32]) {
    key[0] &= 248;
    key[31] &= 127;
    key[31] |= 64;
}

fn is_clamped(key: &[u8; 32]) -> bool {
    key[0] & 7 == 0 && key[31] & 0x80 == 0 && key[31] & 0x40 != 0
}

fn parse_hex_key(raw: &str, prefix: &str) -> Result<[u8; 32]> {
    let hex = raw
        .strip_prefix(prefix)
        .ok_or(Error::Invalid("wrong key prefix"))?;
    if hex.len() != 64 {
        return Err(Error::Invalid("key must contain 32 hex bytes"));
    }
    let mut out = [0; 32];
    for (index, pair) in hex.as_bytes().chunks_exact(2).enumerate() {
        out[index] = (nibble(pair[0])? << 4) | nibble(pair[1])?;
    }
    Ok(out)
}

fn nibble(byte: u8) -> Result<u8> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(Error::Invalid("key contains non-hex characters")),
    }
}

fn write_hex(f: &mut fmt::Formatter<'_>, bytes: &[u8]) -> fmt::Result {
    for byte in bytes {
        write!(f, "{byte:02x}")?;
    }
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    let mut value = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use fmt::Write;
        write!(value, "{byte:02x}").expect("writing to String cannot fail");
    }
    value
}

fn append_hex(out: &mut Vec<u8>, bytes: &[u8]) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize]);
        out.push(HEX[(byte & 0x0f) as usize]);
    }
}
