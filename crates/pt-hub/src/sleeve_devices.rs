use std::fs;
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::Path;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crate::hub::Hub;
use crate::principal::{CgroupSleeve, cgroup_sleeve, peer_cgroup};

pub fn watch(hub: Arc<Hub>) -> Result<(), String> {
    if hub.config.sleeve_devices.is_empty() {
        return Ok(());
    }
    let mut identities: Vec<_> = hub
        .config
        .sleeve_devices
        .iter()
        .map(|p| node_id(p))
        .collect();
    thread::Builder::new()
        .name("pt-devices".into())
        .spawn(move || {
            loop {
                thread::sleep(Duration::from_secs(1));
                for (path, previous) in hub.config.sleeve_devices.iter().zip(&mut identities) {
                    let Some(current) = node_id(path) else {
                        continue;
                    };
                    if *previous != Some(current) {
                        *previous = Some(current);
                        let view = hub.view();
                        for s in view.state.sleeves.values() {
                            if matches!(
                                s.life,
                                pt_core::Life::Running
                                    | pt_core::Life::Launching
                                    | pt_core::Life::Resleeving
                            ) {
                                hub.submit(pt_core::EventKind::DeviceChanged {
                                    sleeve: s.id,
                                    g: s.g,
                                    path: path.clone(),
                                });
                            }
                        }
                    }
                }
            }
        })
        .map_err(|e| format!("spawn device watcher: {e}"))?;
    Ok(())
}

fn node_id(path: &str) -> Option<(u64, u64)> {
    let meta = fs::metadata(path).ok()?;
    Some((meta.dev(), meta.ino()))
}

pub struct Device {
    pub path: String,
    pub resolved: Option<String>,
    pub status: String,
    pub holders: Vec<String>,
}

impl Device {
    pub fn description(&self) -> String {
        let mut line = format!("{}: {}", self.path, self.status);
        for holder in &self.holders {
            line.push_str(&format!("; holder {holder}"));
        }
        line
    }
}

fn identity(file: &str, id: u32) -> String {
    fs::read_to_string(file)
        .ok()
        .and_then(|data| {
            data.lines().find_map(|line| {
                let fields: Vec<_> = line.split(':').collect();
                (fields.get(2)?.parse::<u32>().ok()? == id).then(|| fields[0].to_string())
            })
        })
        .unwrap_or_else(|| id.to_string())
}

fn sleeve_descendant(mut pid: i32, roots: &[i32]) -> bool {
    for _ in 0..128 {
        if roots.contains(&pid) {
            return true;
        }
        let Some(parent) = fs::read_to_string(format!("/proc/{pid}/status"))
            .ok()
            .and_then(|s| {
                s.lines()
                    .find_map(|line| line.strip_prefix("PPid:")?.trim().parse::<i32>().ok())
            })
        else {
            return false;
        };
        if parent <= 1 || parent == pid {
            return false;
        }
        pid = parent;
    }
    false
}

pub fn inspect(hub: &Hub) -> Vec<Device> {
    let view = hub.view();
    let sleeve_roots: Vec<_> = view
        .state
        .sleeves
        .values()
        .filter(|s| s.life == pt_core::Life::Running)
        .filter_map(|s| i32::try_from(s.receipt.as_ref()?.pid).ok())
        .collect();
    let own_ns = fs::read_link("/proc/self/ns/pid").ok();
    let sleeve_namespaces: Vec<_> = view
        .state
        .sleeves
        .values()
        .filter_map(|s| {
            let pid = s.receipt.as_ref()?.pid;
            let namespace = fs::read_link(format!("/proc/{pid}/ns/pid")).ok()?;
            (Some(&namespace) != own_ns.as_ref()).then_some(namespace)
        })
        .collect();
    hub.config
        .sleeve_devices
        .iter()
        .map(|raw| {
            let path = Path::new(raw);
            let resolved = fs::canonicalize(path).ok();
            let meta = match fs::metadata(path) {
                Ok(meta) => meta,
                Err(e) => {
                    return Device {
                        path: raw.clone(),
                        resolved: None,
                        status: if e.kind() == std::io::ErrorKind::NotFound {
                            "missing".into()
                        } else {
                            format!("cannot inspect: {e}")
                        },
                        holders: Vec::new(),
                    };
                }
            };
            let target = resolved
                .as_ref()
                .map_or_else(|| raw.clone(), |p| p.display().to_string());
            let access = match pt_sys::device_read_write(path) {
                Ok(()) => "yes".to_string(),
                Err(e) => format!("no ({e})"),
            };
            let status = format!(
                "{target}, owner {} group {} mode {:04o}, read/write {access}",
                identity("/etc/passwd", meta.uid()),
                identity("/etc/group", meta.gid()),
                meta.mode() & 0o7777
            );
            let mut holders = Vec::new();
            if (meta.file_type().is_char_device() || meta.file_type().is_block_device())
                && let Ok(processes) = fs::read_dir("/proc")
            {
                for process in processes.flatten() {
                    let Ok(pid) = process.file_name().to_string_lossy().parse::<i32>() else {
                        continue;
                    };
                    if sleeve_descendant(pid, &sleeve_roots) {
                        continue;
                    }
                    let Ok(namespace) = fs::read_link(process.path().join("ns/pid")) else {
                        continue;
                    };
                    if sleeve_namespaces.contains(&namespace) {
                        continue;
                    }
                    let Some(cg) = peer_cgroup(pid) else { continue };
                    if !matches!(cgroup_sleeve(hub, &view, &cg), CgroupSleeve::None) {
                        continue;
                    }
                    let Ok(fds) = fs::read_dir(process.path().join("fd")) else {
                        continue;
                    };
                    if fds.flatten().any(|fd| {
                        fs::metadata(fd.path()).is_ok_and(|m| {
                            (m.file_type().is_char_device() || m.file_type().is_block_device())
                                && m.rdev() == meta.rdev()
                        })
                    }) {
                        let comm =
                            fs::read_to_string(process.path().join("comm")).unwrap_or_default();
                        holders.push(format!("pid {pid} {}", comm.trim()));
                    }
                }
            }
            holders.sort();
            Device {
                path: raw.clone(),
                resolved: resolved.map(|p| p.display().to_string()),
                status,
                holders,
            }
        })
        .collect()
}
