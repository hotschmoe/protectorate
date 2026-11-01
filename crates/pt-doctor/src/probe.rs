//! Running a program with a timeout, finding programs on PATH, and telling how one was installed.

use crate::{Binary, Install};
use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

pub(crate) const KEEP: usize = 64 << 10;

pub struct Out {
    pub ok: bool,
    pub stdout: String,
    pub stderr: String,
}

impl Out {
    /// The first non-empty line of stdout, else of stderr.
    pub fn first_line(&self) -> String {
        let pick = |s: &str| {
            s.lines()
                .map(str::trim)
                .find(|l| !l.is_empty())
                .map(|l| l.chars().take(120).collect::<String>())
        };
        pick(&self.stdout)
            .or_else(|| pick(&self.stderr))
            .unwrap_or_default()
    }

    pub fn failure(&self) -> String {
        let line = self.first_line();
        if line.is_empty() {
            "failed with no output".into()
        } else {
            line
        }
    }
}

fn drain(mut r: impl Read + Send + 'static) -> mpsc::Receiver<String> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        let mut chunk = [0u8; 8192];
        while let Ok(n) = r.read(&mut chunk) {
            if n == 0 {
                break;
            }
            if buf.len() < KEEP {
                buf.extend_from_slice(&chunk[..n.min(KEEP - buf.len())]);
            }
        }
        let _ = tx.send(String::from_utf8_lossy(&buf).into_owned());
    });
    rx
}

/// Runs `program args` with stdin closed. A run past `timeout` is killed and reported as a
/// failure. Output a lingering grandchild keeps open is given up on a second after the exit.
pub(crate) fn run(
    program: &Path,
    args: &[&str],
    env: &[(&str, &str)],
    timeout: Duration,
) -> Result<Out, String> {
    let mut c = Command::new(program);
    c.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in env {
        c.env(k, v);
    }
    let what = || format!("{} {}", program.display(), args.join(" "));
    let mut child = c.spawn().map_err(|e| format!("running {}: {e}", what()))?;
    let out = drain(child.stdout.take().expect("piped stdout"));
    let err = drain(child.stderr.take().expect("piped stderr"));
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "{} timed out after {} s",
                    what(),
                    timeout.as_secs()
                ));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => return Err(format!("waiting for {}: {e}", what())),
        }
    };
    let grace = Duration::from_secs(1);
    Ok(Out {
        ok: status.success(),
        stdout: out.recv_timeout(grace).unwrap_or_default(),
        stderr: err.recv_timeout(grace).unwrap_or_default(),
    })
}

#[cfg(unix)]
fn executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn executable(p: &Path) -> bool {
    p.is_file()
}

/// `name` on `path` (a PATH value), as the shell would find it.
pub(crate) fn which(name: &str, path: Option<&OsString>) -> Option<PathBuf> {
    let exts: &[&str] = if cfg!(windows) {
        &[".exe", ".cmd", ".bat", ""]
    } else {
        &[""]
    };
    std::env::split_paths(path?)
        .flat_map(|d| exts.iter().map(move |e| d.join(format!("{name}{e}"))))
        .find(|p| executable(p))
}

/// The absolute program a shell wrapper hands over to with `exec`, if it does.
fn exec_target(script: &str) -> Option<PathBuf> {
    script.lines().find_map(|l| {
        let arg = l.trim().strip_prefix("exec ")?.trim_start();
        let path = match arg.chars().next()? {
            q @ ('\'' | '"') => arg[1..].split(q).next()?,
            _ => arg.split_whitespace().next()?,
        };
        let path = Path::new(path);
        (path.is_absolute() && path.is_file()).then(|| path.to_path_buf())
    })
}

/// D33: a harness installs and updates natively. A compiled executable is native; a script
/// run by node, or anything under node_modules, is an npm wrapper. A shell wrapper that execs
/// an absolute path is judged by what it runs, which is returned with the method.
pub(crate) fn install_method(real: &Path) -> (Install, PathBuf) {
    let mut path = real.to_path_buf();
    for _ in 0..3 {
        let m = install_method_of(&path);
        let next = (m == Install::Script)
            .then(|| std::fs::read(&path).ok())
            .flatten()
            .and_then(|b| exec_target(&String::from_utf8_lossy(&b[..b.len().min(4096)])))
            .map(|t| std::fs::canonicalize(&t).unwrap_or(t));
        match next {
            Some(n) if n != path => path = n,
            _ => return (m, path),
        }
    }
    (Install::Script, path)
}

fn install_method_of(real: &Path) -> Install {
    let mut head = [0u8; 256];
    let n = std::fs::File::open(real)
        .and_then(|mut f| f.read(&mut head))
        .unwrap_or(0);
    let head = &head[..n];
    let under_node_modules = real.components().any(|c| c.as_os_str() == "node_modules");
    const MAGIC: [&[u8]; 6] = [
        b"\x7fELF",
        b"MZ",
        b"\xcf\xfa\xed\xfe",
        b"\xce\xfa\xed\xfe",
        b"\xca\xfe\xba\xbe",
        b"\xfe\xed\xfa\xcf",
    ];
    if MAGIC.iter().any(|m| head.starts_with(m)) {
        return Install::Native;
    }
    if under_node_modules {
        return Install::Npm;
    }
    if let Some(rest) = head.strip_prefix(b"#!") {
        let line = rest.split(|&b| b == b'\n').next().unwrap_or(&[]);
        let line = String::from_utf8_lossy(line);
        return if line.split(['/', ' ']).any(|w| w == "node") {
            Install::Npm
        } else {
            Install::Script
        };
    }
    Install::Other
}

/// Looks `path` up and, if `version_args` is given, runs it for a version line.
pub(crate) fn binary(
    name: &str,
    path: Option<PathBuf>,
    version_args: Option<&[&str]>,
    timeout: Duration,
) -> Binary {
    let Some(path) = path else {
        return Binary::missing(name);
    };
    let real = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
    let (install, real) = install_method(&real);
    let mut b = Binary {
        name: name.to_string(),
        present: true,
        path: path.display().to_string(),
        real_path: real.display().to_string(),
        install,
        version: String::new(),
        error: String::new(),
    };
    if let Some(args) = version_args {
        match run(&path, args, &[], timeout) {
            Ok(o) if o.ok => b.version = o.first_line(),
            Ok(o) => b.error = o.failure(),
            Err(e) => b.error = e,
        }
    }
    b
}
