//! The client builds of the release this hub runs (SPEC 6a). `HubInfo.clients` lists those its
//! `SHA256SUMS` names; `GET /api/client/<name>` serves one. The first request fetches it from
//! that release, checks it against the local `SHA256SUMS` and renames it into
//! `<home>/clients/<tag>/`; creating that directory removes the other releases' ones.

use std::fs::{self, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use pt_api::{ApiError, CLIENT_ASSETS, ErrorCode};
use pt_update::{Source, Store, Sums};

use crate::home::{Config, Home};
use crate::lock::lock;
use crate::log::{info, warning};

pub struct Clients {
    /// The running release's tag and its `SHA256SUMS`.
    release: Option<(String, Sums)>,
    dir: PathBuf,
    /// Held across a fetch, so concurrent first requests download once.
    fetching: Mutex<()>,
}

fn err(code: ErrorCode, m: impl Into<String>) -> ApiError {
    ApiError::new(code, m)
}

fn io_err(what: String) -> impl FnOnce(io::Error) -> ApiError {
    move |e| err(ErrorCode::Internal, format!("{what}: {e}"))
}

fn update_err(e: pt_update::Error) -> ApiError {
    let code = match e.kind {
        pt_update::ErrorKind::Fetch => ErrorCode::Unavailable,
        pt_update::ErrorKind::Release => ErrorCode::NotFound,
        _ => ErrorCode::Internal,
    };
    err(code, e.message)
}

impl Clients {
    /// The release `store` says this process runs; none for a hub not started from
    /// `releases/current`.
    pub fn open(home: &Home, store: &Store) -> Clients {
        let release = store
            .started_from_current("protectorate")
            .ok()
            .and_then(|()| store.running_release())
            .and_then(|tag| {
                let path = store.version_dir(&tag).join("SHA256SUMS");
                let sums = fs::read(&path)
                    .map_err(|e| e.to_string())
                    .and_then(|b| Sums::parse(&b).map_err(|e| e.to_string()));
                match sums {
                    Ok(sums) => Some((tag, sums)),
                    Err(e) => {
                        warning!("no client builds: {}: {e}", path.display());
                        None
                    }
                }
            });
        Clients {
            release,
            dir: home.clients(),
            fetching: Mutex::new(()),
        }
    }

    pub fn names(&self) -> Vec<String> {
        let Some((_, sums)) = &self.release else {
            return Vec::new();
        };
        CLIENT_ASSETS
            .iter()
            .filter(|n| sums.get(n).is_some())
            .map(|n| n.to_string())
            .collect()
    }

    /// The local copy of client build `name`, fetched and checked first when it is not here.
    pub fn path(&self, name: &str, config: &Config) -> Result<PathBuf, ApiError> {
        if !CLIENT_ASSETS.contains(&name) {
            return Err(err(
                ErrorCode::NotFound,
                format!("{name:?} is not a client build"),
            ));
        }
        let Some((tag, sums)) = &self.release else {
            return Err(err(
                ErrorCode::NotFound,
                "this hub does not run from releases/current, so it has no client builds",
            ));
        };
        if sums.get(name).is_none() {
            return Err(err(
                ErrorCode::NotFound,
                format!("release {tag} has no {name}"),
            ));
        }
        let vdir = self.dir.join(tag);
        let dest = vdir.join(name);
        if dest.is_file() {
            return Ok(dest);
        }
        let _one = lock(&self.fetching);
        if dest.is_file() {
            return Ok(dest);
        }
        self.fetch(tag, sums, name, &dest, config)
            .map_err(|e| err(e.code, format!("fetching {name} of {tag}: {}", e.message)))?;
        Ok(dest)
    }

    fn fetch(
        &self,
        tag: &str,
        sums: &Sums,
        name: &str,
        dest: &Path,
        config: &Config,
    ) -> Result<(), ApiError> {
        let vdir = self.dir.join(tag);
        fs::create_dir_all(&self.dir)
            .map_err(io_err(format!("creating {}", self.dir.display())))?;
        match fs::create_dir(&vdir) {
            Ok(()) => self.remove_others(tag),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(io_err(format!("creating {}", vdir.display()))(e)),
        }
        let src = Source::new(&config.release_api, &config.release_repo).map_err(update_err)?;
        let release = src.release(tag).map_err(update_err)?;
        let asset = release.asset(name).map_err(update_err)?;
        let part = vdir.join(format!(".{name}.part"));
        let res = src
            .download_asset(asset, &part)
            .and_then(|_| sums.verify_file(name, &part))
            .map_err(update_err)
            .and_then(|()| {
                OpenOptions::new()
                    .write(true)
                    .open(&part)
                    .and_then(|f| f.sync_all())
                    .and_then(|()| fs::rename(&part, dest))
                    .map_err(io_err(format!("finishing {}", dest.display())))
            });
        if res.is_err() {
            let _ = fs::remove_file(&part);
        }
        res?;
        info!("fetched client build {name} of {tag}");
        Ok(())
    }

    fn remove_others(&self, tag: &str) {
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return;
        };
        for e in entries.flatten() {
            let other = e.file_type().is_ok_and(|t| t.is_dir()) && e.file_name() != tag;
            if other && let Err(x) = fs::remove_dir_all(e.path()) {
                warning!("removing {}: {x}", e.path().display());
            }
        }
    }
}
