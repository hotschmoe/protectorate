//! Capability probe and command construction for explicitly elevated jobs.
//! The node agent token is the trust boundary; the hub separately checks the
//! caller's named-node admin grant before forwarding such a job.
use std::{path::PathBuf, sync::OnceLock};
#[cfg(unix)]
use std::{
    process::Stdio,
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Debug)]
#[cfg_attr(not(unix), allow(dead_code))]
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
    #[cfg(unix)]
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
    #[cfg(not(unix))]
    {
        (
            None,
            "no admin jobs: elevation unavailable on this platform".into(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sudo_preserves_only_named_job_variables_without_putting_values_in_argv() {
        let admin = Admin::Sudo(PathBuf::from("/usr/bin/sudo"));
        let command = admin.command(
            &["sh".into(), "-c".into(), "exit 0".into()],
            ["TOKEN".into()],
        );
        let args = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(
            args,
            [
                "-n",
                "--preserve-env=PROTECTORATE_JOB_DIR,PROTECTORATE_JOB_ID,TOKEN",
                "--",
                "sh",
                "-c",
                "exit 0"
            ]
        );
    }
}
