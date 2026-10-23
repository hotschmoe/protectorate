//! Linux Headscale process ownership and backup coordination.

use crate::{ensure_private_dir, Config, Error, Result, HEADSCALE_VERSION};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

const LOG_MAX_SIZE: u64 = 10 << 20;
const LOG_KEEP: usize = 3;

#[derive(Clone, Default)]
pub struct StopToken(Arc<AtomicBool>);

impl StopToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn stop(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_stopped(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

#[derive(Clone, Debug)]
pub struct SupervisorTiming {
    pub stop_timeout: Duration,
    pub backoff_min: Duration,
    pub backoff_max: Duration,
    pub healthy_run: Duration,
    pub poll_interval: Duration,
}

impl Default for SupervisorTiming {
    fn default() -> Self {
        Self {
            stop_timeout: Duration::from_secs(10),
            backoff_min: Duration::from_secs(1),
            backoff_max: Duration::from_secs(30),
            healthy_run: Duration::from_secs(60),
            poll_interval: Duration::from_millis(100),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Supervisor {
    pub home: PathBuf,
    pub config: Config,
    pub binary: PathBuf,
    pub derp_cert_pin: Option<String>,
    pub timing: SupervisorTiming,
}

impl Supervisor {
    pub fn new(home: impl Into<PathBuf>, config: Config) -> Self {
        let home = home.into();
        Self {
            binary: home.join(format!("bin/headscale-{HEADSCALE_VERSION}")),
            home,
            config,
            derp_cert_pin: None,
            timing: SupervisorTiming::default(),
        }
    }

    pub fn with_binary(mut self, binary: impl Into<PathBuf>) -> Self {
        self.binary = binary.into();
        self
    }

    pub fn with_derp_cert_pin(mut self, pin: impl Into<String>) -> Self {
        self.derp_cert_pin = Some(pin.into());
        self
    }

    pub fn run(&self, stop: &StopToken) -> Result<()> {
        if !self.binary.is_file() {
            return Err(Error(format!(
                "headscale binary {} is missing (run `protectorate hub tailnet install`)",
                self.binary.display()
            )));
        }
        self.config
            .write_files(&self.home, self.derp_cert_pin.as_deref())?;
        let log = Arc::new(Mutex::new(RotatingLog::open(
            &self.home.join("tailnet/headscale.log"),
        )?));
        let mut backoff = self.timing.backoff_min;
        while !stop.is_stopped() {
            if requested(&self.home)? {
                sleep_or_stop(stop, self.timing.poll_interval);
                continue;
            }
            let Some(gate) = try_lock(&self.home, "database.lock")? else {
                sleep_or_stop(stop, self.timing.poll_interval);
                continue;
            };
            if requested(&self.home)? {
                drop(gate);
                continue;
            }
            let start = Instant::now();
            let paused = self.run_child(stop, &log)?;
            drop(gate);
            if stop.is_stopped() {
                return Ok(());
            }
            if paused {
                continue;
            }
            if start.elapsed() >= self.timing.healthy_run {
                backoff = self.timing.backoff_min;
            }
            sleep_or_stop(stop, backoff);
            backoff = backoff.saturating_mul(2).min(self.timing.backoff_max);
        }
        Ok(())
    }

    fn run_child(&self, stop: &StopToken, log: &Arc<Mutex<RotatingLog>>) -> Result<bool> {
        let mut child = match Command::new(&self.binary)
            .arg("serve")
            .arg("-c")
            .arg(self.home.join("tailnet/headscale.yaml"))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(error) => {
                eprintln!("Headscale start failed: {error}");
                return Ok(false);
            }
        };
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");
        let output = thread::spawn({
            let log = Arc::clone(log);
            move || copy_log(stdout, log)
        });
        let errors = thread::spawn({
            let log = Arc::clone(log);
            move || copy_log(stderr, log)
        });
        let mut paused = false;
        let mut observation_error = None;
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) => {}
                Err(error) => {
                    observation_error = Some(Error(format!("wait for Headscale: {error}")));
                    terminate(&mut child, self.timing.stop_timeout)?;
                    break;
                }
            }
            if stop.is_stopped() {
                terminate(&mut child, self.timing.stop_timeout)?;
                break;
            }
            match requested(&self.home) {
                Ok(true) => {
                    paused = true;
                    terminate(&mut child, self.timing.stop_timeout)?;
                    break;
                }
                Ok(false) => {}
                Err(error) => {
                    observation_error = Some(error);
                    terminate(&mut child, self.timing.stop_timeout)?;
                    break;
                }
            }
            sleep_or_stop(stop, self.timing.poll_interval);
        }
        for reader in [output, errors] {
            reader
                .join()
                .map_err(|_| Error("Headscale log reader panicked".into()))?
                .map_err(|error| Error(format!("write Headscale log: {error}")))?;
        }
        if let Some(error) = observation_error {
            return Err(error);
        }
        Ok(paused)
    }
}

fn sleep_or_stop(stop: &StopToken, duration: Duration) {
    let deadline = Instant::now() + duration;
    while !stop.is_stopped() && Instant::now() < deadline {
        thread::sleep(
            deadline
                .saturating_duration_since(Instant::now())
                .min(Duration::from_millis(100)),
        );
    }
}

fn terminate(child: &mut Child, timeout: Duration) -> Result<()> {
    #[cfg(unix)]
    {
        // SAFETY: pid belongs to the child returned by Command::spawn.
        let sent = unsafe { libc::kill(child.id() as libc::pid_t, libc::SIGTERM) };
        if sent != 0 && io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH) {
            return Err(Error(format!(
                "signal Headscale: {}",
                io::Error::last_os_error()
            )));
        }
    }
    #[cfg(not(unix))]
    child
        .kill()
        .map_err(|error| Error(format!("stop Headscale: {error}")))?;
    let deadline = Instant::now() + timeout;
    loop {
        if child
            .try_wait()
            .map_err(|error| Error(format!("wait for Headscale: {error}")))?
            .is_some()
        {
            return Ok(());
        }
        if Instant::now() >= deadline {
            child
                .kill()
                .map_err(|error| Error(format!("kill Headscale: {error}")))?;
            child
                .wait()
                .map_err(|error| Error(format!("reap Headscale: {error}")))?;
            return Ok(());
        }
        thread::sleep(Duration::from_millis(25));
    }
}

fn requested(home: &Path) -> Result<bool> {
    Ok(try_lock(home, "backup-request.lock")?.is_none())
}

fn try_lock(home: &Path, name: &str) -> Result<Option<File>> {
    let dir = home.join("tailnet");
    ensure_private_dir(&dir)?;
    let path = dir.join(name);
    let mut opts = OpenOptions::new();
    opts.read(true).write(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600)
            .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW);
    }
    let file = opts
        .open(&path)
        .map_err(|error| Error(format!("open {}: {error}", path.display())))?;
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        loop {
            // SAFETY: fd is open and owned by file for the entire call.
            let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
            if result == 0 {
                return Ok(Some(file));
            }
            let error = io::Error::last_os_error();
            match error.raw_os_error() {
                Some(libc::EINTR) => continue,
                Some(libc::EWOULDBLOCK) => return Ok(None),
                _ => return Err(Error(format!("lock {}: {error}", path.display()))),
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = file;
        Err(Error(
            "Headscale supervisor requires Unix file locks".into(),
        ))
    }
}

struct RotatingLog {
    path: PathBuf,
    file: File,
    size: u64,
}

impl RotatingLog {
    fn open(path: &Path) -> Result<Self> {
        let file = Self::open_file(path)?;
        let size = file
            .metadata()
            .map_err(|error| Error(error.to_string()))?
            .len();
        Ok(Self {
            path: path.to_owned(),
            file,
            size,
        })
    }

    fn open_file(path: &Path) -> Result<File> {
        let mut opts = OpenOptions::new();
        opts.create(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600).custom_flags(libc::O_NOFOLLOW);
        }
        opts.open(path)
            .map_err(|error| Error(format!("open {}: {error}", path.display())))
    }

    fn write(&mut self, bytes: &[u8]) -> io::Result<()> {
        if self.size > 0 && self.size.saturating_add(bytes.len() as u64) > LOG_MAX_SIZE {
            self.rotate()?;
        }
        self.file.write_all(bytes)?;
        self.size += bytes.len() as u64;
        Ok(())
    }

    fn rotate(&mut self) -> io::Result<()> {
        self.file.flush()?;
        for index in (1..=LOG_KEEP).rev() {
            let from = if index == 1 {
                self.path.clone()
            } else {
                PathBuf::from(format!("{}.{}", self.path.display(), index - 1))
            };
            let to = PathBuf::from(format!("{}.{}", self.path.display(), index));
            if let Err(error) = fs::rename(from, to) {
                if error.kind() != io::ErrorKind::NotFound {
                    return Err(error);
                }
            }
        }
        self.file = Self::open_file(&self.path).map_err(io::Error::other)?;
        self.size = 0;
        Ok(())
    }
}

fn copy_log(mut source: impl Read, log: Arc<Mutex<RotatingLog>>) -> io::Result<()> {
    let mut buffer = [0_u8; 8192];
    loop {
        let size = source.read(&mut buffer)?;
        if size == 0 {
            return Ok(());
        }
        log.lock()
            .map_err(|_| io::Error::other("Headscale log mutex poisoned"))?
            .write(&buffer[..size])?;
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::{fd::AsRawFd, unix::fs::PermissionsExt};

    fn config() -> Config {
        let mut config = Config::for_server("http://127.0.0.1:7480");
        config.embedded_derp = false;
        config.public_derp = true;
        config
    }

    fn fake_binary(home: &Path, script: &str) -> PathBuf {
        let path = home.join("fake-headscale");
        fs::write(&path, script).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        path
    }

    fn timing() -> SupervisorTiming {
        SupervisorTiming {
            stop_timeout: Duration::from_millis(500),
            backoff_min: Duration::from_millis(30),
            backoff_max: Duration::from_millis(100),
            healthy_run: Duration::from_secs(1),
            poll_interval: Duration::from_millis(10),
        }
    }

    fn until(mut predicate: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while !predicate() {
            assert!(
                Instant::now() < deadline,
                "timed out waiting for fake Headscale"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn persists_private_config_and_requires_real_derp_pin() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = config();
        cfg.save(dir.path()).unwrap();
        assert_eq!(Config::load(dir.path()).unwrap(), cfg);
        assert_eq!(
            fs::metadata(dir.path().join("tailnet/config.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        cfg.write_files(dir.path(), None).unwrap();
        let rendered: serde_json::Value =
            serde_json::from_slice(&fs::read(dir.path().join("tailnet/headscale.yaml")).unwrap())
                .unwrap();
        assert_eq!(rendered["server_url"], cfg.server_url);
        let mut private = cfg;
        private.embedded_derp = true;
        assert!(private.write_files(dir.path(), None).is_err());
        private
            .write_files(
                dir.path(),
                Some("sha256-raw:1234567890123456789012345678901234567890123456789012345678901234"),
            )
            .unwrap();
        assert!(fs::read_to_string(dir.path().join("tailnet/derpmap.json"))
            .unwrap()
            .contains("999:"));
    }

    #[test]
    fn starts_child_and_stops_with_sigterm() {
        let dir = tempfile::tempdir().unwrap();
        let script = fake_binary(
            dir.path(),
            r#"#!/bin/sh
[ "$1" = serve ] || exit 44
[ "$2" = -c ] || exit 45
dir=$(dirname "$0")
echo start >> "$dir/starts"
echo child-output
trap 'echo term >> "$dir/terms"; exit 0' TERM
while :; do sleep 0.05; done
"#,
        );
        let mut sup = Supervisor::new(dir.path(), config()).with_binary(script);
        sup.timing = timing();
        let stop = StopToken::new();
        let runner = thread::spawn({
            let stop = stop.clone();
            move || sup.run(&stop)
        });
        until(|| dir.path().join("starts").exists());
        assert!(dir.path().join("tailnet/database.lock").exists());
        stop.stop();
        runner.join().unwrap().unwrap();
        assert!(dir.path().join("terms").exists());
        assert!(fs::read_to_string(dir.path().join("tailnet/headscale.log"))
            .unwrap()
            .contains("child-output"));
    }

    #[test]
    fn restarts_after_child_failure() {
        let dir = tempfile::tempdir().unwrap();
        let script = fake_binary(
            dir.path(),
            r#"#!/bin/sh
dir=$(dirname "$0")
echo start >> "$dir/starts"
if [ ! -f "$dir/failed-once" ]; then
  touch "$dir/failed-once"
  exit 17
fi
trap 'exit 0' TERM
while :; do sleep 0.05; done
"#,
        );
        let mut sup = Supervisor::new(dir.path(), config()).with_binary(script);
        sup.timing = timing();
        let stop = StopToken::new();
        let runner = thread::spawn({
            let stop = stop.clone();
            move || sup.run(&stop)
        });
        until(|| {
            fs::read_to_string(dir.path().join("starts"))
                .is_ok_and(|starts| starts.lines().count() >= 2)
        });
        stop.stop();
        runner.join().unwrap().unwrap();
    }

    #[test]
    fn backup_request_pauses_child_until_lock_is_released() {
        let dir = tempfile::tempdir().unwrap();
        let script = fake_binary(
            dir.path(),
            r#"#!/bin/sh
dir=$(dirname "$0")
echo start >> "$dir/starts"
trap 'echo term >> "$dir/terms"; exit 0' TERM
while :; do sleep 0.05; done
"#,
        );
        let mut sup = Supervisor::new(dir.path(), config()).with_binary(script);
        sup.timing = timing();
        let stop = StopToken::new();
        let runner = thread::spawn({
            let stop = stop.clone();
            move || sup.run(&stop)
        });
        until(|| dir.path().join("starts").exists());
        let request = OpenOptions::new()
            .read(true)
            .write(true)
            .open(dir.path().join("tailnet/backup-request.lock"))
            .unwrap();
        // SAFETY: the open descriptor remains valid until it is dropped.
        assert_eq!(
            unsafe { libc::flock(request.as_raw_fd(), libc::LOCK_EX) },
            0
        );
        until(|| dir.path().join("terms").exists());
        let starts = fs::read_to_string(dir.path().join("starts")).unwrap();
        thread::sleep(Duration::from_millis(80));
        assert_eq!(
            fs::read_to_string(dir.path().join("starts")).unwrap(),
            starts
        );
        drop(request);
        until(|| fs::read_to_string(dir.path().join("starts")).unwrap().len() > starts.len());
        stop.stop();
        runner.join().unwrap().unwrap();
    }
}
