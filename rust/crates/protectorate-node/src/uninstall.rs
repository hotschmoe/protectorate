//! Non-destructive Linux and Termux node service unregistration.
use std::{
    io,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

const UNIT: &str = "protectorate-node.service";
const TERMUX_SERVICE: &str = "protectorate-node";
const TERMUX_MARKER: &str = "# Managed by Protectorate Rust node install\n";
const DELAY: Duration = Duration::from_secs(1);
const COMMAND_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone)]
pub struct Plan {
    home: PathBuf,
    user_home: PathBuf,
    executable: PathBuf,
    self_uninstall: bool,
    platform: Platform,
    prefix: Option<PathBuf>,
}

#[derive(Clone, Copy)]
enum Platform {
    Systemd,
    Termux,
}

impl Plan {
    pub fn new(
        home: impl Into<PathBuf>,
        user_home: impl Into<PathBuf>,
        executable: impl Into<PathBuf>,
        self_uninstall: bool,
    ) -> io::Result<Self> {
        if !cfg!(target_os = "linux") {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "node uninstall is supported on Linux only",
            ));
        }
        let home = home.into();
        let user_home = user_home.into();
        let executable = executable.into();
        if !home.is_absolute() || !user_home.is_absolute() || !executable.is_absolute() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "uninstall paths must be absolute",
            ));
        }
        Ok(Self {
            home,
            user_home,
            executable,
            self_uninstall,
            platform: Platform::Systemd,
            prefix: None,
        })
    }

    pub fn termux(
        home: impl Into<PathBuf>,
        user_home: impl Into<PathBuf>,
        executable: impl Into<PathBuf>,
        prefix: impl Into<PathBuf>,
        self_uninstall: bool,
    ) -> io::Result<Self> {
        let home = home.into();
        let user_home = user_home.into();
        let executable = executable.into();
        let prefix = prefix.into();
        if !home.is_absolute()
            || !user_home.is_absolute()
            || !executable.is_absolute()
            || !prefix.is_absolute()
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Termux uninstall paths must be absolute",
            ));
        }
        Ok(Self {
            home,
            user_home,
            executable,
            self_uninstall,
            platform: Platform::Termux,
            prefix: Some(prefix),
        })
    }

    pub fn actions(&self) -> Vec<String> {
        if matches!(self.platform, Platform::Termux) {
            return vec![
                "disable the Termux node service".into(),
                "stop the agent".into(),
            ];
        }
        if self.self_uninstall {
            vec![
                "disable the systemd user unit".into(),
                "stop the agent".into(),
            ]
        } else {
            vec!["stop the agent and disable the systemd user unit".into()]
        }
    }

    pub fn left(&self) -> Vec<String> {
        if let Platform::Termux = self.platform {
            let prefix = self.prefix.as_ref().expect("Termux plan has prefix");
            return vec![
                prefix
                    .join("var/service")
                    .join(TERMUX_SERVICE)
                    .display()
                    .to_string(),
                self.user_home
                    .join(".termux/boot")
                    .join(TERMUX_SERVICE)
                    .display()
                    .to_string(),
                self.executable.display().to_string(),
                format!("{} (node state)", self.home.display()),
            ];
        }
        let mut left = vec![
            self.user_home
                .join(".config/systemd/user")
                .join(UNIT)
                .display()
                .to_string(),
            self.executable.display().to_string(),
            format!("{} (node state)", self.home.display()),
        ];
        let release_root = self
            .user_home
            .join(".local/share/protectorate-rust/node-install");
        if self.executable.starts_with(&release_root) {
            left.push(format!("{} (Rust node releases)", release_root.display()));
        }
        left
    }

    /// Only asks the user manager to unregister startup. No files are removed.
    pub fn run(&self, service_command: &Path) -> io::Result<()> {
        if matches!(self.platform, Platform::Termux) {
            let prefix = self.prefix.as_ref().expect("Termux plan has prefix");
            let run = prefix.join("var/service").join(TERMUX_SERVICE).join("run");
            let data = std::fs::read(&run)?;
            if !data
                .splitn(2, |byte| *byte == b'\n')
                .nth(1)
                .is_some_and(|body| body.starts_with(TERMUX_MARKER.as_bytes()))
            {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    format!("unmanaged Termux service script: {}", run.display()),
                ));
            }
            return run_command(service_command, &[TERMUX_SERVICE]);
        }
        let mut args = vec!["--user", "disable"];
        if !self.self_uninstall {
            args.push("--now");
        }
        args.push(UNIT);
        run_command(service_command, &args)
    }
}

fn run_command(binary: &Path, args: &[&str]) -> io::Result<()> {
    let mut child = Command::new(binary)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()?;
    let deadline = Instant::now() + COMMAND_TIMEOUT;
    loop {
        if let Some(status) = child.try_wait()? {
            return if status.success() {
                Ok(())
            } else {
                Err(io::Error::other(format!(
                    "{} exited {status}",
                    binary.display()
                )))
            };
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!("{} timed out", binary.display()),
            ));
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

pub type Stop = Arc<dyn Fn() + Send + Sync>;

pub(crate) struct Uninstaller {
    plan: Plan,
    systemctl: PathBuf,
    stop: Stop,
    scheduled: AtomicBool,
}

impl Uninstaller {
    pub(crate) fn new(plan: Plan, systemctl: PathBuf, stop: Stop) -> Self {
        Self {
            plan,
            systemctl,
            stop,
            scheduled: AtomicBool::new(false),
        }
    }

    pub(crate) fn schedule(&self) -> serde_json::Value {
        if !self.scheduled.swap(true, Ordering::SeqCst) {
            let plan = self.plan.clone();
            let systemctl = self.systemctl.clone();
            let stop = Arc::clone(&self.stop);
            tokio::spawn(async move {
                tokio::time::sleep(DELAY).await;
                match tokio::task::spawn_blocking(move || plan.run(&systemctl)).await {
                    Ok(Err(error)) => eprintln!("node uninstall: {error}"),
                    Err(error) => eprintln!("node uninstall worker: {error}"),
                    Ok(Ok(())) => {}
                }
                stop();
            });
        }
        serde_json::json!({"actions":self.plan.actions(),"left":self.plan.left()})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[cfg(unix)]
    fn fake_systemctl_disables_without_deleting_anything() {
        use std::{fs, os::unix::fs::PermissionsExt};
        let dir = tempfile::tempdir().unwrap();
        let user = dir.path().join("user");
        let home = dir.path().join("state");
        let exe = dir.path().join("protectorate-rs");
        let unit = user.join(".config/systemd/user").join(UNIT);
        fs::create_dir_all(unit.parent().unwrap()).unwrap();
        fs::create_dir_all(&home).unwrap();
        fs::write(&unit, "unit").unwrap();
        fs::write(&exe, "binary").unwrap();
        let log = dir.path().join("argv");
        let fake = dir.path().join("systemctl");
        fs::write(
            &fake,
            format!("#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\n", log.display()),
        )
        .unwrap();
        fs::set_permissions(&fake, fs::Permissions::from_mode(0o700)).unwrap();
        let local = Plan::new(&home, &user, &exe, false).unwrap();
        assert_eq!(local.left().len(), 3);
        local.run(&fake).unwrap();
        let self_plan = Plan::new(&home, &user, &exe, true).unwrap();
        self_plan.run(&fake).unwrap();
        assert_eq!(fs::read_to_string(&log).unwrap(),
            "--user disable --now protectorate-node.service\n--user disable protectorate-node.service\n");
        assert_eq!(fs::read(&unit).unwrap(), b"unit");
        assert_eq!(fs::read(&exe).unwrap(), b"binary");
        assert!(home.is_dir());
    }

    #[test]
    #[cfg(unix)]
    fn managed_install_reports_release_root_as_leftover() {
        let dir = tempfile::tempdir().unwrap();
        let user = dir.path().join("user");
        let exe =
            user.join(".local/share/protectorate-rust/node-install/release-1/protectorate-rs");
        let plan = Plan::new(dir.path().join("state"), &user, exe, false).unwrap();
        assert!(plan
            .left()
            .iter()
            .any(|entry| entry.contains("node-install (Rust node releases)")));
    }

    #[test]
    #[cfg(unix)]
    fn termux_disables_only_a_managed_service_and_leaves_files() {
        use std::{fs, os::unix::fs::PermissionsExt};
        let dir = tempfile::tempdir().unwrap();
        let prefix = dir.path().join("prefix");
        let user = dir.path().join("user");
        let home = dir.path().join("state");
        let exe = prefix.join("bin/protectorate-rs");
        let run = prefix.join("var/service/protectorate-node/run");
        let boot = user.join(".termux/boot/protectorate-node");
        for path in [&exe, &run, &boot] {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
        }
        fs::create_dir_all(&home).unwrap();
        fs::write(&exe, "binary").unwrap();
        fs::write(&run, format!("#!/bin/sh\n{TERMUX_MARKER}")).unwrap();
        fs::write(&boot, "boot").unwrap();
        let log = dir.path().join("argv");
        let fake = dir.path().join("sv-disable");
        fs::write(
            &fake,
            format!("#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\n", log.display()),
        )
        .unwrap();
        fs::set_permissions(&fake, fs::Permissions::from_mode(0o700)).unwrap();
        let plan = Plan::termux(&home, &user, &exe, &prefix, true).unwrap();
        assert!(plan
            .left()
            .iter()
            .any(|path| path == &run.parent().unwrap().display().to_string()));
        plan.run(&fake).unwrap();
        assert_eq!(fs::read_to_string(&log).unwrap(), "protectorate-node\n");
        for path in [&exe, &run, &boot] {
            assert!(path.exists());
        }
        fs::write(&run, "unmanaged").unwrap();
        assert_eq!(
            plan.run(&fake).unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
        assert_eq!(fs::read_to_string(&log).unwrap(), "protectorate-node\n");
    }
}
