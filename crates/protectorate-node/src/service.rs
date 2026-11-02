//! The system service. Either way `install-service` puts this binary in the release store,
//! `<state>/releases/<version>/` with `releases/current` and `releases/previous` naming it, so
//! updates switch it (see `update.rs`). Linux (D10): a systemd unit runs `probation` from
//! `releases/previous`, then `serve` as root from `releases/current`. Windows (D11): it limits the
//! state dir to SYSTEM and Administrators, copies this binary under
//! %ProgramFiles%\protectorate-node and registers (through sc.exe) an auto-start LocalSystem
//! service running that copy's `supervise --service`, which does what the unit does (see
//! `supervise.rs`). serve supervises the member this binary embeds (unpacked under
//! `<state>/net`), and every path or service created goes into the install manifest; `uninstall`
//! removes exactly what it lists. The state dir (node identity, member keys, jobs, releases) was
//! made by `join`, so it is not in the manifest and stays.

use std::fs;
use std::path::{Path, PathBuf};

use pt_json::{FromJson, ToJson, record};

use crate::join::{is_root, system_state};
#[cfg(not(windows))]
use crate::member;

pub const USAGE: &str = "  install-service [--state-dir DIR]
                                    after join, as root or an elevated administrator: install
                                    and start the service (systemd protectorate-node.service,
                                    or the Windows service protectorate-node)
  uninstall [--state-dir DIR]       stop the service and remove what install-service listed in
                                    DIR/install-manifest.json";

#[cfg(not(windows))]
const UNIT: &str = "protectorate-node.service";
/// The Windows service name.
#[cfg(windows)]
pub const SERVICE: &str = "protectorate-node";
/// Where member forwards the hub's connections (tailnet port 7475) to `serve` without a unix
/// socket: the Windows service, and `join`'s hint for a foreground run.
pub const LISTEN: &str = "127.0.0.1:7476";
#[cfg(not(windows))]
const UNIT_PATH: &str = "/etc/systemd/system/protectorate-node.service";
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

fn opts(args: &[String]) -> Result<Opts, String> {
    let mut o = Opts {
        state: system_state()?,
    };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = || it.next().ok_or_else(|| format!("{a} needs a value"));
        match a.as_str() {
            "--state-dir" => o.state = PathBuf::from(value()?),
            other => return Err(format!("unknown option '{other}'")),
        }
    }
    if !o.state.is_absolute() {
        return Err(format!(
            "--state-dir {}: must be absolute",
            o.state.display()
        ));
    }
    Ok(o)
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

#[cfg(not(windows))]
fn systemctl(args: &[&str]) -> Result<(), String> {
    let out = std::process::Command::new("systemctl")
        .args(args)
        .output()
        .map_err(|e| format!("running systemctl {}: {e}", args.join(" ")))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "systemctl {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

pub fn install(args: &[String]) -> Result<(), String> {
    let o = opts(args)?;
    #[cfg(windows)]
    return crate::winservice::install(&o);
    #[cfg(not(windows))]
    install_systemd(&o)
}

#[cfg(not(windows))]
fn install_systemd(o: &Opts) -> Result<(), String> {
    if !cfg!(target_os = "linux") {
        return Err("install-service supports Linux and Windows only".into());
    }
    if !is_root() {
        return Err("install-service needs root (sudo)".into());
    }
    let cfg = member::read_config(&o.state)?
        .ok_or_else(|| format!("{} has no node.json: run join first", o.state.display()))?;
    if manifest_path(&o.state).exists() {
        return Err(format!(
            "already installed ({} exists); uninstall first",
            manifest_path(&o.state).display()
        ));
    }
    // The unit runs the embedded member; unpacking it now proves it is there.
    member::member_binary(None, &o.state)?;
    if Path::new(UNIT_PATH).exists() {
        return Err(format!("{UNIT_PATH} exists already; not overwriting it"));
    }
    crate::join::create_private(&o.state)?;
    let version = into_releases(&o.state)?;

    let mut m = Manifest {
        unit: UNIT.into(),
        entries: Vec::new(),
    };
    let state = o.state.display().to_string();
    let releases = o.state.join("releases");
    let bin = releases.join("current").join("protectorate-node");
    let unit = format!(
        "# Written by protectorate-node install-service; removed by uninstall.
[Unit]
Description=Protectorate node agent ({name}, control {url})
Wants=network-online.target
After=network-online.target

[Service]
Type=simple
ExecStartPre={prev} probation --state-dir {state}
ExecStart={bin} serve --state-dir {state} --forward-socket {state}/agent.sock
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
",
        name = cfg.name,
        url = cfg.server_url,
        prev = releases
            .join("previous")
            .join("protectorate-node")
            .display(),
        bin = bin.display(),
    );
    fs::write(UNIT_PATH, unit).map_err(|e| format!("writing {UNIT_PATH}: {e}"))?;
    m.entries.push(Entry {
        path: UNIT_PATH.into(),
        kind: "file".into(),
    });
    save(&o.state, &m)?;
    systemctl(&["daemon-reload"])?;
    systemctl(&["enable", "--now", UNIT])?;
    println!(
        "Installed and started {UNIT} on release {version} ({}).\nInstall manifest: {}\nUninstall: sudo {} uninstall --state-dir {state}",
        releases.display(),
        manifest_path(&o.state).display(),
        bin.display()
    );
    Ok(())
}

/// The service runs `<state>/releases/current/protectorate-node`, the path updates switch. This
/// binary is copied in as release `pt_version::VERSION` unless it runs from there already. The
/// releases are state: uninstall keeps them.
pub(crate) fn into_releases(state: &Path) -> Result<String, String> {
    let store = pt_update::Store::open(state).map_err(|e| e.to_string())?;
    let version = match store.running_release() {
        Some(v) => v,
        None => {
            let v = pt_version::VERSION.to_string();
            let exe = std::env::current_exe().map_err(|e| format!("finding this binary: {e}"))?;
            let dir = store.version_dir(&v);
            let bin = dir.join(format!("protectorate-node{}", std::env::consts::EXE_SUFFIX));
            if dir.exists() {
                let same = fs::read(&exe)
                    .ok()
                    .is_some_and(|a| fs::read(&bin).ok() == Some(a));
                if !same {
                    return Err(format!(
                        "{} exists and is not this binary; not overwriting it",
                        dir.display()
                    ));
                }
            } else {
                fs::create_dir(&dir).map_err(|e| format!("creating {}: {e}", dir.display()))?;
                fs::copy(&exe, &bin)
                    .map_err(|e| format!("copying {} to {}: {e}", exe.display(), bin.display()))?;
                set_mode(&bin, 0o755)?;
            }
            v
        }
    };
    store
        .check_binary(&version, "protectorate-node", "--version")
        .map_err(|e| e.to_string())?;
    store.switch(&version).map_err(|e| e.to_string())?;
    store.ensure_previous().map_err(|e| e.to_string())?;
    Ok(version)
}

pub fn uninstall(args: &[String]) -> Result<(), String> {
    let o = opts(args)?;
    if cfg!(unix) && !is_root() {
        return Err("uninstall needs root (sudo)".into());
    }
    let path = manifest_path(&o.state);
    let m = load(&path)?;
    #[cfg(windows)]
    crate::winservice::check_uninstaller(&m)?;
    #[cfg(not(windows))]
    if let Err(e) = systemctl(&["disable", "--now", &m.unit]) {
        eprintln!("protectorate-node: {e} (continuing)");
    }
    let mut failed = Vec::new();
    for e in m.entries.iter().rev() {
        let res = match e.kind.as_str() {
            "file" => remove_retrying(|| fs::remove_file(&e.path)),
            "dir" => remove_retrying(|| fs::remove_dir(&e.path)),
            #[cfg(windows)]
            "service" => crate::winservice::remove_service(&e.path),
            other => {
                failed.push(format!("{}: unknown kind {other}", e.path));
                continue;
            }
        };
        match res {
            Ok(()) => println!("removed {}", e.path),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                println!("already gone {}", e.path)
            }
            Err(err) => failed.push(format!("{}: {err}", e.path)),
        }
    }
    #[cfg(not(windows))]
    if let Err(e) = systemctl(&["daemon-reload"]) {
        eprintln!("protectorate-node: {e}");
    }
    if !failed.is_empty() {
        return Err(format!(
            "could not remove: {}; {} kept",
            failed.join("; "),
            path.display()
        ));
    }
    fs::remove_file(&path).map_err(|e| format!("removing {}: {e}", path.display()))?;
    println!(
        "removed {}\nKept the state dir {} (tailnet keys, node id, job logs). Remove the node on the hub too.",
        path.display(),
        o.state.display()
    );
    Ok(())
}

pub(crate) fn load(path: &Path) -> Result<Manifest, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    let v = pt_json::parse(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    Manifest::from_json(&v).map_err(|e| format!("{}: {e}", path.display()))
}

/// Windows keeps a just-stopped service's executables locked for a moment.
fn remove_retrying(mut f: impl FnMut() -> std::io::Result<()>) -> std::io::Result<()> {
    let mut tries = if cfg!(windows) { 20 } else { 1 };
    loop {
        match f() {
            Err(e)
                if tries > 1
                    && matches!(
                        e.kind(),
                        std::io::ErrorKind::PermissionDenied
                            | std::io::ErrorKind::DirectoryNotEmpty
                    ) =>
            {
                tries -= 1;
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
            r => return r,
        }
    }
}

#[cfg(unix)]
fn set_mode(p: &Path, mode: u32) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(p, fs::Permissions::from_mode(mode))
        .map_err(|e| format!("chmod {}: {e}", p.display()))
}

#[cfg(not(unix))]
fn set_mode(_p: &Path, _mode: u32) -> Result<(), String> {
    Ok(())
}
