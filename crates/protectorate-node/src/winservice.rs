//! The Windows service (D11): `install-service` and `uninstall` through the system's sc.exe and
//! icacls.exe, with every path and the service recorded in the install manifest. The service
//! itself runs `supervise --service` as LocalSystem (supervise.rs, pt_node::windows) from the
//! copy under %ProgramFiles%, which is never switched: that supervisor starts
//! `releases\current\protectorate-node.exe serve`, the binary updates switch, and restarts it.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use crate::manifest::{Entry, Manifest, Opts, SERVICE, manifest_path, save};
use crate::member::{self, LISTEN};
use crate::update::into_releases;

const STOPPED: u32 = 1;
const RUNNING: u32 = 4;
const SERVICE_DOES_NOT_EXIST: i32 = 1060;
const WAIT: Duration = Duration::from_secs(30);

pub fn install(o: &Opts) -> Result<(), String> {
    let cfg = member::read_config(&o.state)?
        .ok_or_else(|| format!("{} has no node.json: run join first", o.state.display()))?;
    let manifest = manifest_path(&o.state);
    if manifest.exists() {
        return Err(format!(
            "already installed ({} exists); uninstall first",
            manifest.display()
        ));
    }
    if state(SERVICE)?.is_some() {
        return Err(format!("a service named {SERVICE} exists already"));
    }
    // The service runs the embedded member; unpacking it now proves it is there.
    member::member_binary(None, &o.state)?;
    let exe = std::env::current_exe().map_err(|e| format!("finding this binary: {e}"))?;
    let base = std::env::var_os("ProgramFiles").ok_or("ProgramFiles is not set")?;
    let prefix = PathBuf::from(base).join("protectorate-node");
    if prefix.exists() {
        return Err(format!(
            "{} exists already; not overwriting it",
            prefix.display()
        ));
    }
    let state_dir = quotable(&o.state)?;
    restrict(&o.state)?;
    let version = into_releases(&o.state)?;

    // Each item goes into the manifest as soon as it exists, so a failed install can be undone.
    let mut m = Manifest {
        unit: SERVICE.into(),
        entries: Vec::new(),
    };
    let record = |m: &mut Manifest, path: &str, kind: &str| {
        m.entries.push(Entry {
            path: path.into(),
            kind: kind.into(),
        });
        save(&o.state, m)
    };
    fs::create_dir(&prefix).map_err(|e| format!("creating {}: {e}", prefix.display()))?;
    record(&mut m, &prefix.display().to_string(), "dir")?;
    let bin = prefix.join("protectorate-node.exe");
    fs::copy(&exe, &bin)
        .map_err(|e| format!("copying {} to {}: {e}", exe.display(), bin.display()))?;
    record(&mut m, &bin.display().to_string(), "file")?;
    let command = format!(
        "{} supervise --service --state-dir {state_dir} --listen-tcp {LISTEN}",
        quotable(&bin)?
    );
    sc(&[
        "create",
        SERVICE,
        "binPath=",
        &command,
        "start=",
        "auto",
        "DisplayName=",
        "Protectorate node",
    ])?;
    record(&mut m, SERVICE, "service")?;
    let about = format!(
        "Protectorate node agent ({}, control {}); installed by protectorate-node install-service",
        cfg.name, cfg.server_url
    );
    sc(&["description", SERVICE, &about])?;
    // The supervisor restarts the node itself; these restart the supervisor: 5 s, 5 s, then
    // every 60 s, also after a clean exit with an error.
    sc(&[
        "failure",
        SERVICE,
        "reset=",
        "86400",
        "actions=",
        "restart/5000/restart/5000/restart/60000",
    ])?;
    sc(&["failureflag", SERVICE, "1"])?;
    sc(&["start", SERVICE])?;
    wait_for(SERVICE, RUNNING)?;
    println!(
        "Installed and started the service {SERVICE} on release {version} (LocalSystem, log {}).\nInstall manifest: {}\nUninstall, from a copy outside {}: {} uninstall --state-dir {}",
        o.state.join("node.log").display(),
        manifest.display(),
        prefix.display(),
        exe.display(),
        o.state.display()
    );
    Ok(())
}

/// Windows cannot delete a running executable, so uninstall must not run from the install.
pub fn check_uninstaller(m: &Manifest) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| format!("finding this binary: {e}"))?;
    let exe = exe.display().to_string().to_lowercase();
    match m
        .entries
        .iter()
        .find(|e| e.kind == "file" && e.path.to_lowercase() == exe)
    {
        Some(e) => Err(format!(
            "uninstall cannot remove the binary it runs from ({}); run it from another copy",
            e.path
        )),
        None => Ok(()),
    }
}

/// Stops the service, waits until it has stopped, and deletes it.
pub fn remove_service(name: &str) -> std::io::Result<()> {
    let io = |e: String| std::io::Error::other(e);
    match state(name).map_err(io)? {
        None => return Err(std::io::ErrorKind::NotFound.into()),
        Some(STOPPED) => {}
        Some(_) => {
            if let Err(e) = sc(&["stop", name]) {
                eprintln!("protectorate-node: {e} (continuing)");
            }
            wait_for(name, STOPPED).map_err(io)?;
        }
    }
    sc(&["delete", name]).map_err(io)?;
    let deadline = Instant::now() + WAIT;
    while state(name).map_err(io)?.is_some() {
        if Instant::now() > deadline {
            return Err(io(format!(
                "{name} is still listed {} s after delete",
                WAIT.as_secs()
            )));
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    Ok(())
}

/// Only SYSTEM and Administrators may touch the state dir (member keys; jobs run as SYSTEM in
/// its work dir), instead of what %ProgramData% passes down to every user.
fn restrict(dir: &Path) -> Result<(), String> {
    let out = Command::new("icacls.exe")
        .arg(dir)
        .args([
            "/inheritance:r",
            "/grant:r",
            "*S-1-5-18:(OI)(CI)F",
            "*S-1-5-32-544:(OI)(CI)F",
            "/Q",
        ])
        .output()
        .map_err(|e| format!("running icacls: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "icacls {}: {}{}",
            dir.display(),
            String::from_utf8_lossy(&out.stdout).trim(),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(())
}

/// A path as one quoted argument of the service's command line.
fn quotable(p: &Path) -> Result<String, String> {
    let s = p.display().to_string();
    if s.contains('"') || s.ends_with('\\') {
        return Err(format!(
            "{s}: a path for the service may not contain '\"' or end in '\\'"
        ));
    }
    Ok(format!("\"{s}\""))
}

fn sc(args: &[&str]) -> Result<String, String> {
    let out = Command::new("sc.exe")
        .args(args)
        .output()
        .map_err(|e| format!("running sc.exe {}: {e}", args[0]))?;
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    if out.status.success() {
        Ok(text)
    } else {
        Err(format!(
            "sc.exe {} {}: {}{}",
            args[0],
            args.get(1).unwrap_or(&""),
            text.trim(),
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

/// The service's state code (1 stopped ... 4 running), `None` when there is no such service.
fn state(name: &str) -> Result<Option<u32>, String> {
    let out = Command::new("sc.exe")
        .args(["query", name])
        .output()
        .map_err(|e| format!("running sc.exe query: {e}"))?;
    if out.status.code() == Some(SERVICE_DOES_NOT_EXIST) {
        return Ok(None);
    }
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines()
        .find_map(|l| {
            let (key, value) = l.split_once(':')?;
            (key.trim() == "STATE").then(|| value.split_whitespace().next()?.parse().ok())?
        })
        .map(Some)
        .ok_or_else(|| format!("sc.exe query {name}: no STATE in {:?}", text.trim()))
}

fn wait_for(name: &str, want: u32) -> Result<(), String> {
    let deadline = Instant::now() + WAIT;
    loop {
        let now = state(name)?;
        if now == Some(want) {
            return Ok(());
        }
        if Instant::now() > deadline {
            return Err(format!(
                "{name} is in state {now:?}, not {want}, after {} s",
                WAIT.as_secs()
            ));
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}
