//! Capability probe and command construction for explicitly elevated jobs,
//! which only Linux nodes run. The hub checks the caller's named-node admin
//! grant before forwarding such a job.
use std::{path::PathBuf, sync::OnceLock};
#[cfg(target_os = "linux")]
use std::{
    process::Stdio,
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Debug)]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) enum Admin {
    Root,
    Sudo(PathBuf),
}

impl Admin {
    pub(crate) fn label(&self) -> &'static str {
        match self {
            Self::Root => "root",
            Self::Sudo(_) => "sudo",
        }
    }

    pub(crate) fn command(
        &self,
        argv: &[String],
        env_keys: impl IntoIterator<Item = String>,
    ) -> std::process::Command {
        match self {
            Self::Root => {
                let mut command = std::process::Command::new(&argv[0]);
                command.args(&argv[1..]);
                command
            }
            Self::Sudo(path) => {
                let mut command = std::process::Command::new(path);
                let mut keys = vec![
                    "PROTECTORATE_JOB_ID".to_owned(),
                    "PROTECTORATE_JOB_DIR".to_owned(),
                ];
                keys.extend(env_keys);
                keys.sort();
                keys.dedup();
                command
                    .arg("-n")
                    .arg(format!("--preserve-env={}", keys.join(",")))
                    .arg("--");
                command.args(argv);
                command
            }
        }
    }
}

pub(crate) fn probe() -> (Option<Admin>, String) {
    static RESULT: OnceLock<(Option<Admin>, String)> = OnceLock::new();
    RESULT.get_or_init(probe_once).clone()
}

fn probe_once() -> (Option<Admin>, String) {
    #[cfg(target_os = "linux")]
    {
        if unsafe { libc::geteuid() } == 0 {
            return (Some(Admin::Root), String::new());
        }
        let path = ["/usr/bin/sudo", "/bin/sudo"]
            .into_iter()
            .map(PathBuf::from)
            .find(|path| path.is_file());
        let Some(path) = path else {
            return (None, "no admin jobs: sudo is not installed".into());
        };
        let child = std::process::Command::new(&path)
            .args(["-n", "--", "true"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        let Ok(mut child) = child else {
            return (None, "no admin jobs: sudo probe could not start".into());
        };
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match child.try_wait() {
                Ok(Some(status)) if status.success() => {
                    return (Some(Admin::Sudo(path)), String::new())
                }
                Ok(Some(_)) | Err(_) => return (None, "no admin jobs: sudo -n fails".into()),
                Ok(None) if Instant::now() >= deadline => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return (None, "no admin jobs: sudo -n timed out".into());
                }
                Ok(None) => thread::sleep(Duration::from_millis(20)),
            }
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        (None, "no admin jobs: Linux-only".into())
    }
}
