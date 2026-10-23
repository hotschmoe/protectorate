//! Release metadata for the builds published as GitHub releases.
//!
//! Input validation, channel matching, ETag-checked release lookups, checksum verified asset downloads staged into
//! a temporary directory and published by rename, bounded gzip/tar extraction
//! of the web archive, and the local release store (release.json, atomic
//! current and previous pointers, pruning). Pointers are symlinks on Unix and
//! fall back to a small JSON pointer file where symlinks are unavailable.
mod extract;
mod transport;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs, io,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
    time::{SystemTime, UNIX_EPOCH},
};

pub use extract::{extract_web, safe_web_path, MAX_WEB_BYTES};
pub use transport::{
    resolve_url, validate_url, Budget, Got, Transport, CHECK_TIMEOUT, FETCH_TIMEOUT,
};

/// Response body caps.
pub const MAX_RELEASE_JSON: u64 = 4 << 20;
pub const MAX_SUMS: u64 = 1 << 20;
pub const MAX_VERSION_ASSET: u64 = 1024;
pub const MAX_ASSET: u64 = 512 << 20;

pub const BINARIES: [&str; 5] = [
    "protectorate-linux-amd64",
    "protectorate-linux-arm64",
    "protectorate-windows-amd64.exe",
    "protectorate-windows-arm64.exe",
    "protectorate-android-arm64",
];
/// Rust server asset name in a published release. The architecture names are
/// shared by the node updater and `protectorate-rs version --json`.
pub fn rust_binary_name(os: &str, arch: &str) -> Result<String> {
    if !matches!(
        (os, arch),
        ("linux" | "windows", "amd64" | "arm64") | ("android", "arm64")
    ) {
        return Err(invalid(format!("no Rust server build for {os}/{arch}")));
    }
    let suffix = if os == "windows" { ".exe" } else { "" };
    Ok(format!("protectorate-rs-{os}-{arch}{suffix}"))
}

pub const RUST_BUNDLE: [&str; 6] = [
    "protectorate-rs",
    "protectorate-sleeve-runner",
    "protectorate-sleeve-sandbox-helper",
    "protectorate-node-sandbox-helper",
    "protectorate",
    "protectorate-mcpbridge",
];

pub fn rust_bundle_name(component: &str, os: &str, arch: &str) -> Result<String> {
    if os == "android" {
        return Err(invalid(
            "Android release contains a standalone Termux node, not a hub bundle",
        ));
    }
    if !RUST_BUNDLE.contains(&component) {
        return Err(invalid(format!(
            "unknown Rust release component {component}"
        )));
    }
    let server = rust_binary_name(os, arch)?;
    Ok(server.replacen("protectorate-rs", component, 1))
}

/// Verify the versioned binaries in a local Rust bundle. A shared release
/// cache can hold several architectures, so this does not inspect the
/// unversioned launch names used by an installed hub.
pub fn verify_rust_assets_dir(dir: &Path, os: &str, arch: &str) -> Result<PathBuf> {
    let metadata = fs::symlink_metadata(dir)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(invalid("Rust release directory is not a real directory"));
    }
    let sums = fs::read(dir.join("SHA256SUMS"))?;
    for component in RUST_BUNDLE {
        let name = rust_bundle_name(component, os, arch)?;
        let path = dir.join(&name);
        verify_executable_file(&path, &name)?;
        verify_checksum(&name, &fs::read(&path)?, &sums)?;
    }
    Ok(dir.join(rust_binary_name(os, arch)?))
}

/// Verify a complete installed hub bundle and its unversioned launch names.
pub fn verify_rust_bundle_dir(dir: &Path, os: &str, arch: &str) -> Result<PathBuf> {
    let binary = verify_rust_assets_dir(dir, os, arch)?;
    for component in RUST_BUNDLE {
        let name = rust_bundle_name(component, os, arch)?;
        let path = dir.join(&name);
        let stable = dir.join(component);
        if !launch_name_matches(&stable, &path)? {
            return Err(invalid(format!(
                "installed launch name {component} is missing"
            )));
        }
        verify_executable_file(&stable, component)?;
    }
    Ok(binary)
}
pub const DEFAULT_API: &str = "https://api.github.com";
pub const CHANNELS: [&str; 2] = ["stable", "dev"];

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Json(serde_json::Error),
    Invalid(String),
    Http(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "release: {e}"),
            Self::Json(e) => write!(f, "release: {e}"),
            Self::Invalid(s) => write!(f, "{s}"),
            Self::Http(s) => write!(f, "{s}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

fn invalid(msg: impl Into<String>) -> Error {
    Error::Invalid(msg.into())
}

pub fn valid_version(version: &str) -> bool {
    let mut chars = version.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphanumeric() {
        return false;
    }
    if version.len() > 160
        || !chars.all(|c| matches!(c, '.' | '_' | '+' | '-') || c.is_ascii_alphanumeric())
    {
        return false;
    }
    version != "current" && version != "previous"
}

pub fn validate_channel(channel: &str) -> Result<()> {
    if CHANNELS.contains(&channel) {
        Ok(())
    } else {
        Err(invalid(format!(
            "update channel {channel:?} must be stable or dev"
        )))
    }
}

pub fn validate_repo(repo: &str) -> Result<()> {
    let ok = match repo.split_once('/') {
        Some((owner, name)) => {
            !owner.is_empty()
                && !name.is_empty()
                && !name.contains('/')
                && owner
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
        }
        None => false,
    };
    if ok {
        Ok(())
    } else {
        Err(invalid(format!("release repo {repo:?} must be OWNER/REPO")))
    }
}

pub fn validate_api(api: &str) -> Result<String> {
    let bad = || invalid(format!("invalid release API URL {api:?}"));
    let (scheme, rest) = api.split_once("://").ok_or_else(bad)?;
    if (scheme != "http" && scheme != "https") || rest.contains('@') || rest.contains(['?', '#']) {
        return Err(bad());
    }
    let (host, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, ""),
    };
    if host.is_empty() || host.contains('\\') || path.contains('\\') {
        return Err(bad());
    }
    let hostname = host.split(':').next().unwrap_or("");
    if hostname.is_empty()
        || !hostname
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '%'))
    {
        return Err(bad());
    }
    if let Some(port) = host.split_once(':').map(|(_, p)| p) {
        if port.is_empty() || !port.chars().all(|c| c.is_ascii_digit()) {
            return Err(bad());
        }
    }
    Ok(api.trim_end_matches('/').to_string())
}

pub fn default_dir() -> PathBuf {
    let data = match std::env::var("XDG_DATA_HOME") {
        Ok(v) if !v.is_empty() => PathBuf::from(v),
        _ => {
            let home = std::env::var("HOME").unwrap_or_default();
            PathBuf::from(home).join(".local").join("share")
        }
    };
    data.join("protectorate").join("releases")
}

fn percent_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.as_bytes() {
        let c = *b;
        if c.is_ascii_alphanumeric()
            || matches!(
                c,
                b'-' | b'_'
                    | b'.'
                    | b'~'
                    | b'$'
                    | b'&'
                    | b'\''
                    | b'('
                    | b')'
                    | b'*'
                    | b'+'
                    | b','
                    | b';'
                    | b'='
                    | b':'
                    | b'@'
            )
        {
            out.push(c as char);
        } else {
            out.push('%');
            out.push_str(&format!("{c:02X}"));
        }
    }
    out
}

pub fn cache_key(channel: &str, version: &str) -> String {
    if version.is_empty() {
        channel.to_string()
    } else {
        format!("{channel}:{version}")
    }
}

pub fn release_path(channel: &str, version: &str) -> String {
    if version.is_empty() {
        if channel == "dev" {
            "tags/dev".to_string()
        } else {
            "latest".to_string()
        }
    } else {
        "tags/".to_string() + &percent_encode(version)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Asset {
    #[serde(default)]
    pub name: String,
    #[serde(rename = "browser_download_url", default)]
    pub url: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Info {
    #[serde(rename = "tag_name", default)]
    pub tag: String,
    #[serde(default)]
    pub name: String,
    #[serde(rename = "html_url", default)]
    pub notes_url: String,
    #[serde(default)]
    pub prerelease: bool,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub assets: Vec<Asset>,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub channel: String,
}

impl Info {
    pub fn asset_url(&self, name: &str) -> Result<String> {
        let mut found: Option<&str> = None;
        for a in &self.assets {
            if a.name == name {
                if found.is_some() {
                    return Err(invalid(format!("duplicate release asset {name}")));
                }
                found = Some(&a.url);
            }
        }
        match found {
            Some(url) if !url.is_empty() => Ok(url.to_string()),
            _ => Err(invalid(format!("release {} has no {name}", self.tag))),
        }
    }

    pub fn matches_channel(&self, channel: &str, version: &str) -> Result<()> {
        let mismatched = self.draft
            || (version.is_empty()
                && ((channel == "stable" && self.prerelease)
                    || (channel == "dev" && (!self.prerelease || self.tag != "dev"))));
        if mismatched {
            return Err(invalid(format!(
                "release {} does not match channel {channel}",
                self.tag
            )));
        }
        Ok(())
    }

    pub fn set_version_from_asset(&mut self, sums: &[u8], version: &[u8]) -> Result<()> {
        verify_checksum("VERSION", version, sums)?;
        let v = String::from_utf8_lossy(version).trim().to_string();
        if !valid_version(&v) {
            return Err(invalid(format!("invalid release build version {v:?}")));
        }
        self.version = v;
        Ok(())
    }
}

pub fn parse_checksums(sums: &[u8]) -> Result<BTreeMap<String, String>> {
    let text = String::from_utf8_lossy(sums);
    let mut map = BTreeMap::new();
    for line in text.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() != 2 {
            continue;
        }
        let name = fields[1].strip_prefix('*').unwrap_or(fields[1]);
        if map
            .insert(name.to_string(), fields[0].to_string())
            .is_some()
        {
            return Err(invalid(format!("duplicate checksum for {name}")));
        }
    }
    Ok(map)
}

fn decode_hex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(s.len() / 2);
    let nibble = |c: u8| -> Option<u8> {
        match c {
            b'0'..=b'9' => Some(c - b'0'),
            b'a'..=b'f' => Some(c - b'a' + 10),
            b'A'..=b'F' => Some(c - b'A' + 10),
            _ => None,
        }
    };
    for pair in b.chunks(2) {
        let hi = nibble(pair[0])?;
        let lo = nibble(pair[1])?;
        out.push(hi << 4 | lo);
    }
    Some(out)
}

pub fn verify_checksum(name: &str, bytes: &[u8], sums: &[u8]) -> Result<()> {
    let map = parse_checksums(sums)?;
    let Some(want) = map.get(name) else {
        return Err(invalid(format!("missing or invalid checksum for {name}")));
    };
    let Some(want) = decode_hex(want).filter(|h| h.len() == 32) else {
        return Err(invalid(format!("missing or invalid checksum for {name}")));
    };
    let actual = Sha256::digest(bytes);
    if actual[..] != want[..] {
        return Err(invalid(format!("checksum mismatch for {name}")));
    }
    Ok(())
}

/// Add the unversioned names the Linux installer executes (`current/protectorate-rs`
/// and its companions). A hard link matches the installer. A different existing
/// file is left untouched and fails the fetch.
fn link_installed_launch_names(dir: &Path, os: &str, arch: &str) -> Result<()> {
    for component in RUST_BUNDLE {
        let versioned_name = rust_bundle_name(component, os, arch)?;
        link_installed_launch_name(
            &dir.join(component),
            &dir.join(&versioned_name),
            &versioned_name,
            component,
        )?;
    }
    Ok(())
}

/// Publish files from a verified web archive into a partial release without
/// replacing anything already present there.
fn publish_missing_tree(source: &Path, dest: &Path) -> Result<()> {
    match fs::create_dir(dest) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error.into()),
    }
    let metadata = fs::symlink_metadata(dest)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(invalid(format!(
            "cached web path {} is not a directory",
            dest.display()
        )));
    }
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let dest_path = dest.join(entry.file_name());
        let metadata = fs::symlink_metadata(&source_path)?;
        if metadata.file_type().is_symlink() {
            return Err(invalid(format!(
                "staged web path {} is a symlink",
                source_path.display()
            )));
        }
        if metadata.is_dir() {
            publish_missing_tree(&source_path, &dest_path)?;
        } else if metadata.is_file() {
            match fs::hard_link(&source_path, &dest_path) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error.into()),
            }
            let cached = fs::symlink_metadata(&dest_path)?;
            if cached.file_type().is_symlink()
                || !cached.is_file()
                || fs::read(&source_path)? != fs::read(&dest_path)?
            {
                return Err(invalid(format!(
                    "cached web asset {} differs from verified release",
                    dest_path.display()
                )));
            }
        } else {
            return Err(invalid(format!(
                "staged web path {} is not a regular file or directory",
                source_path.display()
            )));
        }
    }
    for entry in fs::read_dir(dest)? {
        let entry = entry?;
        match fs::symlink_metadata(source.join(entry.file_name())) {
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Err(invalid(format!(
                    "cached web asset {} is absent from verified release",
                    entry.path().display()
                )));
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn verify_existing_tree(source: &Path, dest: &Path) -> Result<()> {
    let metadata = match fs::symlink_metadata(dest) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(invalid(format!(
            "cached web path {} is not a directory",
            dest.display()
        )));
    }
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let dest_path = dest.join(entry.file_name());
        let source_metadata = fs::symlink_metadata(&source_path)?;
        if source_metadata.is_dir() {
            verify_existing_tree(&source_path, &dest_path)?;
        } else if source_metadata.is_file() && !source_metadata.file_type().is_symlink() {
            let existing = match fs::symlink_metadata(&dest_path) {
                Ok(existing) => existing,
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.into()),
            };
            if existing.file_type().is_symlink()
                || !existing.is_file()
                || fs::read(&source_path)? != fs::read(&dest_path)?
            {
                return Err(invalid(format!(
                    "cached web asset {} differs from verified release",
                    dest_path.display()
                )));
            }
        } else {
            return Err(invalid(format!(
                "staged web path {} is not a regular file or directory",
                source_path.display()
            )));
        }
    }
    for entry in fs::read_dir(dest)? {
        let entry = entry?;
        match fs::symlink_metadata(source.join(entry.file_name())) {
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Err(invalid(format!(
                    "cached web asset {} is absent from verified release",
                    entry.path().display()
                )));
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn link_installed_launch_name(
    stable: &Path,
    versioned: &Path,
    versioned_name: &str,
    component: &str,
) -> Result<()> {
    if launch_name_matches(stable, versioned)? {
        return Ok(());
    }
    if let Err(error) = fs::hard_link(versioned, stable) {
        if error.kind() == io::ErrorKind::AlreadyExists && launch_name_matches(stable, versioned)? {
            return Ok(());
        }
        return Err(invalid(format!(
            "link {component} to {versioned_name}: {error}"
        )));
    }
    Ok(())
}

fn launch_name_matches(stable: &Path, versioned: &Path) -> Result<bool> {
    let meta = match fs::symlink_metadata(stable) {
        Ok(meta) => meta,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    if meta.file_type().is_symlink() {
        return Err(invalid(format!(
            "installed launch name {} is a symlink",
            stable.display()
        )));
    }
    if !meta.is_file() {
        return Err(invalid(format!(
            "installed launch name {} is not a file",
            stable.display()
        )));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let source = fs::symlink_metadata(versioned)?;
        if meta.dev() == source.dev() && meta.ino() == source.ino() {
            return Ok(true);
        }
    }
    if sha256_file(stable)? == sha256_file(versioned)? {
        return Ok(true);
    }
    Err(invalid(format!(
        "installed launch name {} does not match {}",
        stable.display(),
        versioned.display()
    )))
}

fn verify_executable_file(path: &Path, name: &str) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(invalid(format!(
            "Rust component {name} is not a regular file"
        )));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if fs::metadata(path)?.permissions().mode() & 0o111 == 0 {
            return Err(invalid(format!("Rust component {name} is not executable")));
        }
    }
    Ok(())
}

fn sha256_file(path: &Path) -> Result<[u8; 32]> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let count = io::Read::read(&mut file, &mut buf)?;
        if count == 0 {
            break;
        }
        hasher.update(&buf[..count]);
    }
    Ok(hasher.finalize().into())
}

pub fn resolved_version(
    info: &Info,
    sums: Option<&[u8]>,
    version_asset: Option<&[u8]>,
) -> Result<String> {
    if let Some(v) = version_asset {
        let Some(sums) = sums else {
            return Err(invalid("release VERSION requires SHA256SUMS"));
        };
        verify_checksum("VERSION", v, sums)?;
        let v = String::from_utf8_lossy(v).trim().to_string();
        if !valid_version(&v) {
            return Err(invalid(format!("invalid release build version {v:?}")));
        }
        return Ok(v);
    }
    if info.tag == "dev" {
        return Err(invalid("dev release has no VERSION asset"));
    }
    let v = if info.version.is_empty() {
        info.tag.clone()
    } else {
        info.version.clone()
    };
    if !valid_version(&v) {
        return Err(invalid(format!("invalid release build version {v:?}")));
    }
    Ok(v)
}

#[derive(Clone, Debug)]
pub struct Cached {
    pub etag: String,
    pub info: Info,
}

#[derive(Debug)]
struct Cache {
    entries: BTreeMap<String, Cached>,
}

static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

fn tmp_suffix() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    format!("{nanos:x}{seq:x}")
}

fn create_dir_mode(path: &Path, mode: u32) -> Result<()> {
    fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    }
    #[cfg(not(unix))]
    let _ = mode;
    Ok(())
}

fn write_file(path: &Path, bytes: &[u8], mode: u32) -> Result<()> {
    protectorate_fsutil::write_file(path, bytes, mode)?;
    Ok(())
}

/// The release directory, refusing to treat a symlink as a release directory.
fn existing_release(dir: &Path, version: &str) -> Result<Option<PathBuf>> {
    match fs::symlink_metadata(dir) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(invalid(format!("inspect release {}: {e}", version))),
        Ok(fi) if fi.file_type().is_symlink() => Err(invalid(format!(
            "release version {version} is a symlink; refusing to write through it"
        ))),
        Ok(_) => Ok(Some(dir.to_path_buf())),
    }
}

/// A pointer written as a file instead of a symlink, for platforms where
/// creating a symlink is not permitted. It names the release version directly.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct PointerFile {
    version: String,
}

/// Local release store: validated metadata, the release directory layout and
/// the atomic current/previous pointers.
pub struct Source {
    repo: String,
    channel: String,
    api: String,
    dir: PathBuf,
    cache: Mutex<Cache>,
    check_lock: Mutex<()>,
    transport: Transport,
}

impl std::fmt::Debug for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Source")
            .field("repo", &self.repo)
            .field("channel", &self.channel)
            .field("api", &self.api)
            .field("dir", &self.dir)
            .finish_non_exhaustive()
    }
}

impl Source {
    pub fn new(repo: &str, channel: &str, api: &str, dir: &str) -> Result<Source> {
        validate_repo(repo)?;
        validate_channel(channel)?;
        let api = if api.is_empty() {
            validate_api(DEFAULT_API)?
        } else {
            validate_api(api)?
        };
        let dir = if dir.is_empty() {
            default_dir()
        } else {
            PathBuf::from(dir)
        };
        let dir = fs::canonicalize(&dir).unwrap_or_else(|_| {
            let abs = std::path::absolute(&dir).unwrap_or_else(|_| dir.clone());
            if abs.parent().is_some_and(|p| p.is_dir()) {
                abs
            } else {
                dir
            }
        });
        let transport = Transport::new(&api);
        Ok(Source {
            repo: repo.to_string(),
            channel: channel.to_string(),
            api,
            dir,
            cache: Mutex::new(Cache {
                entries: BTreeMap::new(),
            }),
            check_lock: Mutex::new(()),
            transport,
        })
    }

    pub fn repo(&self) -> &str {
        &self.repo
    }
    pub fn channel(&self) -> &str {
        &self.channel
    }
    pub fn api(&self) -> &str {
        &self.api
    }
    pub fn dir(&self) -> &Path {
        &self.dir
    }
    pub fn current_dir(&self) -> PathBuf {
        self.dir.join("current")
    }
    pub fn release_dir(&self, version: &str) -> PathBuf {
        self.dir.join(version)
    }

    pub fn current(&self) -> Info {
        let version = match self.read_pointer("current") {
            Ok(v) => v,
            Err(_) => return Info::default(),
        };
        let path = self.release_dir(&version).join("release.json");
        fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    /// The version a pointer names. A pointer is either a symlink whose target
    /// is the release directory name, or a pointer file (see PointerFile) when
    /// symlinks are unavailable. Anything else is refused.
    fn read_pointer(&self, name: &str) -> Result<String> {
        let path = self.dir.join(name);
        match fs::symlink_metadata(&path) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(String::new()),
            Err(e) => Err(invalid(format!("inspect {name} release pointer: {e}"))),
            Ok(fi) if fi.file_type().is_symlink() => {
                let target = fs::read_link(&path)
                    .map_err(|e| invalid(format!("read {name} release pointer: {e}")))?;
                let version = target.to_string_lossy().into_owned();
                if !valid_version(&version) {
                    return Err(invalid(format!(
                        "{name} release pointer names invalid version {version:?}"
                    )));
                }
                Ok(version)
            }
            Ok(fi) if fi.is_file() => {
                let text = fs::read(&path)
                    .map_err(|e| invalid(format!("read {name} release pointer: {e}")))?;
                let pointer: PointerFile = serde_json::from_slice(&text).map_err(|_| {
                    invalid(format!(
                        "refusing {name} release pointer that is not a symlink or a pointer file"
                    ))
                })?;
                if !valid_version(&pointer.version) {
                    return Err(invalid(format!(
                        "{name} release pointer names invalid version {:?}",
                        pointer.version
                    )));
                }
                Ok(pointer.version)
            }
            Ok(_) => Err(invalid(format!(
                "refusing {name} release pointer that is not a symlink or a pointer file"
            ))),
        }
    }

    pub fn available(&self, channel: &str) -> Info {
        let cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        cache
            .entries
            .get(channel)
            .map(|c| c.info.clone())
            .unwrap_or_default()
    }

    pub fn cached(&self, key: &str) -> Option<Cached> {
        let cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        cache.entries.get(key).cloned()
    }

    pub fn record(&self, key: &str, etag: &str, info: Info) {
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        cache.entries.insert(
            key.to_string(),
            Cached {
                etag: etag.to_string(),
                info,
            },
        );
    }

    /// Look up one release. An empty channel is the source channel; an empty
    /// version is the channel default. A cached ETag turns a repeat lookup into
    /// a 304 that returns the cached Info unchanged.
    pub fn check(&self, channel: &str, version: &str) -> Result<Info> {
        let channel = if channel.is_empty() {
            self.channel.clone()
        } else {
            channel.to_string()
        };
        validate_channel(&channel)?;
        if !version.is_empty() && !valid_version(version) {
            return Err(invalid(format!("invalid release version {version:?}")));
        }
        let key = cache_key(&channel, version);
        let path = release_path(&channel, version);
        let _serial = self.check_lock.lock().unwrap_or_else(|e| e.into_inner());
        let previous = self.cached(&key);
        let url = format!("{}/repos/{}/releases/{}", self.api, self.repo, path);
        let etag = previous.as_ref().map(|c| c.etag.as_str()).unwrap_or("");
        let got = self
            .transport
            .get(&url, etag, Budget::Check, MAX_RELEASE_JSON)?;
        if got.not_modified {
            if let Some(cached) = previous {
                return Ok(cached.info);
            }
        }
        let mut info: Info = serde_json::from_slice(&got.body)?;
        info.matches_channel(&channel, version)?;
        info.channel = channel.clone();
        info.version = info.tag.clone();
        if info.asset_url("VERSION").is_ok() {
            let sums = self.asset(&info, "SHA256SUMS", Budget::Check, MAX_SUMS)?;
            let asset = self.asset(&info, "VERSION", Budget::Check, MAX_VERSION_ASSET)?;
            info.set_version_from_asset(&sums, &asset)?;
        } else if info.tag == "dev" {
            return Err(invalid("release dev has no VERSION asset"));
        }
        if !valid_version(&info.version) {
            return Err(invalid(format!(
                "invalid release build version {:?}",
                info.version
            )));
        }
        self.record(&key, &got.etag, info.clone());
        Ok(info)
    }

    /// Download one named asset: the URL comes from the release document, the
    /// body is bounded, and the caller verifies it against SHA256SUMS.
    pub fn asset(&self, info: &Info, name: &str, budget: Budget, limit: u64) -> Result<Vec<u8>> {
        let raw = info.asset_url(name)?;
        Ok(self.transport.get(&raw, "", budget, limit)?.body)
    }

    /// Publish a release directory only after every fleet platform and the web
    /// archive have been downloaded and verified.
    /// `progress` sees each asset before and after its download and can cancel
    /// the whole fetch by returning an error. Re-fetching verifies cached
    /// assets and adds only verified files missing from a partial release.
    pub fn fetch<F>(&self, info: &Info, progress: F) -> Result<PathBuf>
    where
        F: FnMut(&str, u64, u64) -> Result<()>,
    {
        if !valid_version(&info.version) {
            return Err(invalid(format!(
                "invalid release version {:?}",
                info.version
            )));
        }
        create_dir_mode(&self.dir, 0o700)?;
        let stage = self.dir.join(format!(".fetch-{}", tmp_suffix()));
        create_dir_mode(&stage, 0o700)?;
        let mut names: Vec<String> = BINARIES.iter().map(|name| name.to_string()).collect();
        names.push(format!("protectorate-web-{}.tar.gz", info.version));
        if info.asset_url("VERSION").is_ok() {
            names.push("VERSION".into());
        }
        let result = self.fetch_staged(info, progress, &stage, &names, None, false);
        let _ = fs::remove_dir_all(&stage);
        result
    }

    pub fn fetch_rust_release<F>(
        &self,
        info: &Info,
        os: &str,
        arch: &str,
        progress: F,
    ) -> Result<PathBuf>
    where
        F: FnMut(&str, u64, u64) -> Result<()>,
    {
        self.fetch_rust_bundle(info, os, arch, progress, true)
    }

    /// Fetch a node update bundle into the shared release cache. The bundle
    /// keeps only architecture-qualified names so it cannot replace the hub's
    /// installed launch names when the node has another architecture.
    pub fn fetch_rust_node_bundle<F>(
        &self,
        info: &Info,
        os: &str,
        arch: &str,
        progress: F,
    ) -> Result<PathBuf>
    where
        F: FnMut(&str, u64, u64) -> Result<()>,
    {
        self.fetch_rust_bundle(info, os, arch, progress, false)
    }

    fn fetch_rust_bundle<F>(
        &self,
        info: &Info,
        os: &str,
        arch: &str,
        progress: F,
        installed_launch_names: bool,
    ) -> Result<PathBuf>
    where
        F: FnMut(&str, u64, u64) -> Result<()>,
    {
        if !valid_version(&info.version) {
            return Err(invalid("invalid Rust release version"));
        }
        create_dir_mode(&self.dir, 0o700)?;
        let stage = self.dir.join(format!(".fetch-{}", tmp_suffix()));
        create_dir_mode(&stage, 0o700)?;
        let mut names = RUST_BUNDLE
            .iter()
            .map(|component| rust_bundle_name(component, os, arch))
            .collect::<Result<Vec<_>>>()?;
        names.push(format!("protectorate-web-{}.tar.gz", info.version));
        if info.asset_url("VERSION").is_ok() {
            names.push("VERSION".into());
        }
        let result = self.fetch_staged(
            info,
            progress,
            &stage,
            &names,
            Some((os, arch)),
            installed_launch_names,
        );
        let _ = fs::remove_dir_all(&stage);
        result
    }

    /// Stage one Rust server executable alongside a previously verified
    /// release. The name is platform constrained and the bytes must match the
    /// release's SHA256SUMS before publication. An existing asset is verified
    /// and never overwritten.
    pub fn fetch_rust_binary(&self, info: &Info, os: &str, arch: &str) -> Result<PathBuf> {
        self.fetch_named_binary(info, rust_binary_name(os, arch)?)
    }

    fn fetch_named_binary(&self, info: &Info, name: String) -> Result<PathBuf> {
        let dir = self.release_dir(&info.version);
        if !dir.join("release.json").is_file() {
            return Err(invalid("fetch the release before its Rust binary"));
        }
        let sums = fs::read(dir.join("SHA256SUMS"))?;
        let path = dir.join(&name);
        if path.exists() {
            verify_checksum(&name, &fs::read(&path)?, &sums)?;
            return Ok(path);
        }
        let bytes = self.asset(info, &name, Budget::Fetch, MAX_ASSET)?;
        verify_checksum(&name, &bytes, &sums)?;
        let staged = dir.join(format!(".{name}-{}", tmp_suffix()));
        let result = (|| -> Result<()> {
            write_file(&staged, &bytes, 0o755)?;
            match fs::hard_link(&staged, &path) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    verify_checksum(&name, &fs::read(&path)?, &sums)?;
                }
                Err(error) => return Err(error.into()),
            }
            Ok(())
        })();
        let _ = fs::remove_file(staged);
        result?;
        Ok(path)
    }

    /// Verify a complete local Rust bundle without network access before it
    /// is used as a service executable.
    pub fn verify_rust_bundle(&self, version: &str, os: &str, arch: &str) -> Result<PathBuf> {
        if !valid_version(version) {
            return Err(invalid("invalid Rust release version"));
        }
        let dir = self.release_dir(version);
        verify_rust_bundle_dir(&dir, os, arch)
    }

    fn fetch_staged<F>(
        &self,
        info: &Info,
        mut progress: F,
        stage: &Path,
        names: &[String],
        rust_bundle: Option<(&str, &str)>,
        installed_launch_names: bool,
    ) -> Result<PathBuf>
    where
        F: FnMut(&str, u64, u64) -> Result<()>,
    {
        let sums = self.asset(info, "SHA256SUMS", Budget::Fetch, MAX_SUMS)?;
        for name in names {
            progress(name, 0, 0)?;
            let bytes = self.asset(info, name, Budget::Fetch, MAX_ASSET)?;
            verify_checksum(name, &bytes, &sums)?;
            let mode = if name.ends_with(".tar.gz") || name == "VERSION" {
                0o644
            } else {
                0o755
            };
            write_file(&stage.join(name), &bytes, mode)?;
            if name.ends_with(".tar.gz") {
                extract_web(&stage.join(name), &stage.join("web"))?;
            }
            let len = bytes.len() as u64;
            progress(name, len, len)?;
        }
        let metadata = serde_json::to_vec(info)?;
        write_file(&stage.join("release.json"), &metadata, 0o644)?;
        write_file(&stage.join("SHA256SUMS"), &sums, 0o644)?;
        if installed_launch_names {
            let (os, arch) = rust_bundle.expect("installed names require a Rust bundle");
            link_installed_launch_names(stage, os, arch)?;
        }

        let dest = self.release_dir(&info.version);
        if existing_release(&dest, &info.version)?.is_some() {
            // A cached release may contain only one Rust binary. Fill missing
            // verified assets without replacing anything a service can use.
            let cached = fs::read(dest.join("SHA256SUMS")).map_err(|_| {
                invalid(format!(
                    "release version {} already exists with different checksums",
                    info.version
                ))
            })?;
            if cached != sums {
                return Err(invalid(format!(
                    "release version {} already exists with different checksums",
                    info.version
                )));
            }
            for name in names {
                let path = dest.join(name);
                match fs::symlink_metadata(&path) {
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error.into()),
                    Ok(metadata) => {
                        if !metadata.is_file() || metadata.file_type().is_symlink() {
                            return Err(invalid(format!(
                                "cached asset {name} is not a regular file"
                            )));
                        }
                        if rust_bundle.is_some() && name != "VERSION" && !name.ends_with(".tar.gz")
                        {
                            verify_executable_file(&path, name)?;
                        }
                        verify_checksum(name, &fs::read(&path)?, &sums)?;
                    }
                }
            }
            if stage.join("web").is_dir() {
                verify_existing_tree(&stage.join("web"), &dest.join("web"))?;
            }
            for name in names {
                let path = dest.join(name);
                match fs::hard_link(stage.join(name), &path) {
                    Ok(()) => {}
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                    Err(error) => return Err(error.into()),
                }
                if rust_bundle.is_some() && name != "VERSION" && !name.ends_with(".tar.gz") {
                    verify_executable_file(&path, name)?;
                }
                verify_checksum(name, &fs::read(&path)?, &sums)?;
            }
            if stage.join("web").is_dir() {
                publish_missing_tree(&stage.join("web"), &dest.join("web"))?;
            }
            if let Some((os, arch)) = rust_bundle {
                verify_rust_assets_dir(&dest, os, arch)?;
            }
            if installed_launch_names {
                let (os, arch) = rust_bundle.expect("installed names require a Rust bundle");
                link_installed_launch_names(&dest, os, arch)?;
                self.verify_rust_bundle(&info.version, os, arch)?;
            }
            return Ok(dest);
        }
        fs::rename(stage, &dest).map_err(|e| invalid(format!("publish release: {e}")))?;
        Ok(dest)
    }

    /// Write release metadata for a version that is already on disk or is being
    /// staged. An existing release directory is never entered through a
    /// symlink and its assets are never rewritten: the stored checksums must
    /// already match, otherwise the write is refused. New metadata lands by
    /// rename of a private staging directory.
    pub fn store(&self, info: &Info, sums: &[u8]) -> Result<PathBuf> {
        let version = if info.version.is_empty() {
            info.tag.clone()
        } else {
            info.version.clone()
        };
        if !valid_version(&version) {
            return Err(invalid(format!("invalid release version {version:?}")));
        }
        parse_checksums(sums)?;
        create_dir_mode(&self.dir, 0o700)?;
        let dest = self.release_dir(&version);
        if existing_release(&dest, &version)?.is_some() {
            let cached = fs::read(dest.join("SHA256SUMS")).map_err(|_| {
                invalid(format!(
                    "release version {version} already exists with different checksums"
                ))
            })?;
            if cached != sums {
                return Err(invalid(format!(
                    "release version {version} already exists with different checksums"
                )));
            }
            return Ok(dest);
        }
        let stage = self.dir.join(format!(".store-{}", tmp_suffix()));
        create_dir_mode(&stage, 0o700)?;
        let result = (|| -> Result<()> {
            write_file(
                &stage.join("release.json"),
                &serde_json::to_vec(info)?,
                0o644,
            )?;
            write_file(&stage.join("SHA256SUMS"), sums, 0o644)?;
            fs::rename(stage.as_path(), dest.as_path())
                .map_err(|e| invalid(format!("publish release: {e}")))?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_dir_all(&stage);
        }
        result?;
        Ok(dest)
    }

    pub fn activate(&self, version: &str) -> Result<()> {
        if !valid_version(version) {
            return Err(invalid(format!("invalid release version {version:?}")));
        }
        if !self.release_dir(version).join("release.json").is_file() {
            return Err(invalid(format!(
                "release metadata: no release {version} in {}",
                self.dir.display()
            )));
        }
        self.set_pointer("current", version)
    }

    pub fn restore(&self, version: &str) -> Result<()> {
        if version.is_empty() {
            self.set_pointer("current", "")
        } else {
            self.activate(version)
        }
    }

    /// Preserve the rollback release before a confirmed hub trial is finalized.
    /// Pruning is separate so a cleanup failure cannot turn a healthy trial
    /// into an unconfirmed one.
    pub fn retain_previous(&self, version: &str) -> Result<()> {
        if !valid_version(version) || !self.release_dir(version).join("release.json").is_file() {
            return Err(invalid(format!("invalid previous release {version:?}")));
        }
        self.set_pointer("previous", version)
    }

    fn set_pointer(&self, name: &str, version: &str) -> Result<()> {
        let path = self.dir.join(name);
        if version.is_empty() {
            match fs::symlink_metadata(&path) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(invalid(format!("inspect {name} release pointer: {e}"))),
                Ok(fi) if fi.is_dir() => Err(invalid(format!(
                    "refusing to remove {name} release pointer that is a directory"
                ))),
                Ok(fi) if fi.file_type().is_symlink() => fs::remove_file(&path)
                    .map_err(|e| invalid(format!("remove {name} release pointer: {e}"))),
                Ok(fi) if fi.is_file() => {
                    // Only a pointer file we could have written is removable.
                    self.read_pointer(name)?;
                    fs::remove_file(&path)
                        .map_err(|e| invalid(format!("remove {name} release pointer: {e}")))
                }
                Ok(_) => Err(invalid(format!(
                    "refusing to remove {name} release pointer that is not a symlink or a pointer file"
                ))),
            }
        } else {
            // A rename must land on a pointer this store wrote, never on a
            // directory or on some unrelated file.
            self.pointer_target_is_replaceable(name, &path)?;
            let tmp = self.dir.join(format!(".pointer-{}", tmp_suffix()));
            fs::create_dir(&tmp)?;
            let staged = tmp.join(name);
            let result = (|| -> Result<()> {
                #[cfg(unix)]
                {
                    std::os::unix::fs::symlink(version, &staged)?;
                }
                #[cfg(windows)]
                {
                    match std::os::windows::fs::symlink_file(version, &staged) {
                        Ok(()) => {}
                        Err(symlink_err) => {
                            let body = serde_json::to_vec(&PointerFile {
                                version: version.to_string(),
                            })?;
                            protectorate_fsutil::write_file(&staged, &body, 0o644).map_err(|e| {
                                invalid(format!(
                                    "stage {name} release pointer (symlink refused: {symlink_err}): {e}"
                                ))
                            })?;
                        }
                    }
                }
                #[cfg(not(any(unix, windows)))]
                {
                    let body = serde_json::to_vec(&PointerFile {
                        version: version.to_string(),
                    })?;
                    protectorate_fsutil::write_file(&staged, &body, 0o644)?;
                }
                protectorate_fsutil::rename(&staged, &path)?;
                Ok(())
            })();
            let _ = fs::remove_dir_all(&tmp);
            result.map_err(|e: Error| match e {
                Error::Io(inner) => invalid(format!("switch {name} release pointer: {inner}")),
                other => other,
            })
        }
    }

    /// Whether a staged pointer may be renamed onto this path.
    fn pointer_target_is_replaceable(&self, name: &str, path: &Path) -> Result<()> {
        match fs::symlink_metadata(path) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(invalid(format!("inspect {name} release pointer: {e}"))),
            Ok(fi) if fi.is_dir() => Err(invalid(format!(
                "refusing to replace {name} release pointer that is a directory"
            ))),
            Ok(fi) if fi.file_type().is_symlink() => Ok(()),
            Ok(_) => self.read_pointer(name).map(|_| ()),
        }
    }

    pub fn prune(&self, previous: &str) -> Result<()> {
        let current = self.current().version;
        if current.is_empty() {
            return Ok(());
        }
        if !valid_version(&current) || (!previous.is_empty() && !valid_version(previous)) {
            return Err(invalid("invalid current or previous release version"));
        }
        let mut releases = Vec::new();
        for e in fs::read_dir(&self.dir)? {
            let e = e?;
            let name = e.file_name().to_string_lossy().into_owned();
            if !e.file_type()?.is_dir() || !valid_version(&name) {
                continue;
            }
            if !self.release_dir(&name).join("release.json").is_file() {
                continue;
            }
            let mod_time = e.metadata()?.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            releases.push((name, mod_time));
        }
        let (keep, previous) = prune_plan(&current, previous, &releases);
        self.set_pointer("previous", &previous)?;
        for (name, _) in &releases {
            if !keep.contains(name) {
                fs::remove_dir_all(self.release_dir(name))
                    .map_err(|e| invalid(format!("prune release {name}: {e}")))?;
            }
        }
        Ok(())
    }
}

/// Which releases survive a prune: current, the requested previous when it
/// exists, then the most recently modified ones up to two total. The second
/// return value is the version the previous pointer should name.
pub fn prune_plan(
    current: &str,
    previous: &str,
    releases: &[(String, SystemTime)],
) -> (BTreeSet<String>, String) {
    let mut sorted: Vec<&(String, SystemTime)> = releases.iter().collect();
    sorted.sort_by_key(|entry| std::cmp::Reverse(entry.1));
    let mut keep = BTreeSet::from([current.to_string()]);
    if !previous.is_empty() && previous != current && releases.iter().any(|(n, _)| n == previous) {
        keep.insert(previous.to_string());
    }
    for (name, _) in &sorted {
        if keep.len() < 2 {
            keep.insert((*name).clone());
        }
    }
    let pointer = keep
        .iter()
        .find(|n| *n != current)
        .cloned()
        .unwrap_or_default();
    (keep, pointer)
}
