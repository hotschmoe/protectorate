use std::fs;
use std::path::{Path, PathBuf};

use pt_json::{FromJson, ToJson, record};

/// The Windows service name.
#[cfg(windows)]
pub const SERVICE: &str = "protectorate-node";

const MANIFEST: &str = "install-manifest.json";

record! {
    #[derive(Debug, Clone, PartialEq)]
    pub(crate) struct Entry {
        pub(crate) path: String,
        /// `file`, `dir` (removed only when empty) or, on Windows, `service`.
        pub(crate) kind: String,
    }

    #[derive(Debug, Clone, PartialEq)]
    pub(crate) struct Manifest {
        /// The systemd unit or Windows service.
        pub(crate) unit: String,
        pub(crate) entries: Vec<Entry> = Vec::new(),
    }
}

pub(crate) struct Opts {
    pub(crate) state: PathBuf,
}

pub(crate) fn manifest_path(state: &Path) -> PathBuf {
    state.join(MANIFEST)
}

pub(crate) fn save(state: &Path, m: &Manifest) -> Result<(), String> {
    let path = manifest_path(state);
    // Windows flushes only a handle opened for writing.
    fs::File::create(&path)
        .and_then(|mut f| {
            std::io::Write::write_all(&mut f, format!("{}\n", m.to_json()).as_bytes())?;
            f.sync_all()
        })
        .map_err(|e| format!("writing {}: {e}", path.display()))
}

pub(crate) fn load(path: &Path) -> Result<Manifest, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    let v = pt_json::parse(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    Manifest::from_json(&v).map_err(|e| format!("{}: {e}", path.display()))
}
