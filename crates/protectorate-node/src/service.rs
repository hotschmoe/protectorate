//! The Linux system service (D10): `install-service` copies this binary and member under
//! /usr/local/lib/protectorate-node, writes a systemd unit that runs `serve` as root (which
//! supervises member), and records every path it created in the install manifest. `uninstall`
//! removes exactly what that manifest lists. The state dir (node identity, member keys, jobs) was
//! made by `join`, so it is not in the manifest and stays.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use pt_json::{FromJson, ToJson, record};

use crate::join::{SYSTEM_STATE, create_private, is_root};
use crate::member;

pub const USAGE: &str = "  install-service [--state-dir DIR] [--member PATH]
                                    Linux, as root, after join: install the systemd unit
                                    protectorate-node.service and start it
  uninstall [--state-dir DIR]       stop the service and remove what install-service listed in
                                    DIR/install-manifest.json";

const UNIT: &str = "protectorate-node.service";
const UNIT_PATH: &str = "/etc/systemd/system/protectorate-node.service";
const PREFIX: &str = "/usr/local/lib/protectorate-node";
const MANIFEST: &str = "install-manifest.json";

record! {
    #[derive(Debug, Clone, PartialEq)]
    struct Entry {
        path: String,
        /// `file` or `dir` (removed only when empty).
        kind: String,
    }

    #[derive(Debug, Clone, PartialEq)]
    struct Manifest {
        unit: String,
        entries: Vec<Entry> = Vec::new(),
    }
}

struct Opts {
    state: PathBuf,
    member: Option<PathBuf>,
}

fn opts(args: &[String], allow_member: bool) -> Result<Opts, String> {
    let mut o = Opts {
        state: PathBuf::from(SYSTEM_STATE),
        member: None,
    };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = || it.next().ok_or_else(|| format!("{a} needs a value"));
        match a.as_str() {
            "--state-dir" => o.state = PathBuf::from(value()?),
            "--member" if allow_member => o.member = Some(PathBuf::from(value()?)),
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

fn manifest_path(state: &Path) -> PathBuf {
    state.join(MANIFEST)
}

fn save(state: &Path, m: &Manifest) -> Result<(), String> {
    let path = manifest_path(state);
    fs::write(&path, format!("{}\n", m.to_json()))
        .and_then(|()| fs::File::open(&path)?.sync_all())
        .map_err(|e| format!("writing {}: {e}", path.display()))
}

fn systemctl(args: &[&str]) -> Result<(), String> {
    let out = Command::new("systemctl")
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
    let o = opts(args, true)?;
    if !cfg!(target_os = "linux") {
        return Err(
            "install-service is Linux only; on Windows run serve in the foreground for now".into(),
        );
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
    let member_src = member::member_binary(o.member.as_deref())?;
    let exe = std::env::current_exe().map_err(|e| format!("finding this binary: {e}"))?;
    let prefix = Path::new(PREFIX);
    let bin = prefix.join("protectorate-node");
    let member_bin = prefix.join("member");
    for p in [prefix, Path::new(UNIT_PATH)] {
        if p.exists() {
            return Err(format!(
                "{} exists already; not overwriting it",
                p.display()
            ));
        }
    }
    create_private(&o.state)?;

    // Each path goes into the manifest as soon as it exists, so a failed install can be undone.
    let mut m = Manifest {
        unit: UNIT.into(),
        entries: Vec::new(),
    };
    let record = |m: &mut Manifest, path: &Path, kind: &str| {
        m.entries.push(Entry {
            path: path.display().to_string(),
            kind: kind.into(),
        });
        save(&o.state, m)
    };
    fs::create_dir(prefix).map_err(|e| format!("creating {PREFIX}: {e}"))?;
    record(&mut m, prefix, "dir")?;
    for (src, dst) in [(&exe, &bin), (&member_src, &member_bin)] {
        fs::copy(src, dst)
            .map_err(|e| format!("copying {} to {}: {e}", src.display(), dst.display()))?;
        record(&mut m, dst, "file")?;
        set_mode(dst, 0o755)?;
    }
    let state = o.state.display().to_string();
    let unit = format!(
        "# Written by protectorate-node install-service; removed by uninstall.
[Unit]
Description=Protectorate node agent ({name}, control {url})
Wants=network-online.target
After=network-online.target

[Service]
Type=simple
ExecStart={bin} serve --state-dir {state} --forward-socket {state}/agent.sock --member {member}
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
",
        name = cfg.name,
        url = cfg.server_url,
        bin = bin.display(),
        member = member_bin.display(),
    );
    fs::write(UNIT_PATH, unit).map_err(|e| format!("writing {UNIT_PATH}: {e}"))?;
    record(&mut m, Path::new(UNIT_PATH), "file")?;
    systemctl(&["daemon-reload"])?;
    systemctl(&["enable", "--now", UNIT])?;
    println!(
        "Installed and started {UNIT}.\nInstall manifest: {}\nUninstall: sudo {} uninstall --state-dir {state}",
        manifest_path(&o.state).display(),
        bin.display()
    );
    Ok(())
}

pub fn uninstall(args: &[String]) -> Result<(), String> {
    let o = opts(args, false)?;
    if !is_root() {
        return Err("uninstall needs root (sudo)".into());
    }
    let path = manifest_path(&o.state);
    let text = fs::read_to_string(&path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    let v = pt_json::parse(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let m = Manifest::from_json(&v).map_err(|e| format!("{}: {e}", path.display()))?;
    if let Err(e) = systemctl(&["disable", "--now", &m.unit]) {
        eprintln!("protectorate-node: {e} (continuing)");
    }
    let mut failed = Vec::new();
    for e in m.entries.iter().rev() {
        let p = Path::new(&e.path);
        let res = match e.kind.as_str() {
            "file" => fs::remove_file(p),
            "dir" => fs::remove_dir(p),
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
