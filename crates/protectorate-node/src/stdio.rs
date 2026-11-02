//! `serve --stdio`: the hub's frames on stdin and stdout, for an sshd forced command such as
//! `restrict,command="protectorate-node serve --stdio --state-dir DIR"` on the hub's key. sshd
//! has authenticated the peer, so no header line is read.
//!
//! Jobs must outlive one ssh session, so the session does not run them itself: it splices into
//! the resident agent for DIR on `DIR/agent.sock`, starting it detached when none answers. The
//! resident leaves after `--idle-exit` seconds (default 300) with no job and no connection.
//! stdout carries frames only; the session logs to stderr, the resident to `DIR/agent.log`.

use crate::flags::Flags;

#[cfg(unix)]
pub fn run(f: &Flags) -> Result<(), String> {
    use std::os::unix::net::UnixStream;
    use std::time::{Duration, Instant};

    const SOCKET: &str = "agent.sock";
    let dir = f.state_dir.clone().ok_or("serve needs --state-dir")?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("creating {}: {e}", dir.display()))?;
    let dir = std::fs::canonicalize(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    // Socket paths are limited to about 100 bytes, so the socket is named relative to DIR.
    std::env::set_current_dir(&dir).map_err(|e| format!("entering {}: {e}", dir.display()))?;
    let stream = match UnixStream::connect(SOCKET) {
        Ok(s) => s,
        Err(_) => {
            let mut resident = start_resident(f, &dir, SOCKET)?;
            let deadline = Instant::now() + Duration::from_secs(20);
            loop {
                // A resident started by a concurrent session may win the lock; either answers.
                if let Ok(s) = UnixStream::connect(SOCKET) {
                    break s;
                }
                if Instant::now() >= deadline {
                    let exited = resident.try_wait().ok().flatten();
                    return Err(format!(
                        "no agent answers on {}/{SOCKET} (resident {}); see {}/agent.log",
                        dir.display(),
                        exited.map_or("still starting".to_string(), |s| format!("exited: {s}")),
                        dir.display()
                    ));
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    };
    splice(stream)
}

#[cfg(unix)]
fn start_resident(
    f: &Flags,
    dir: &std::path::Path,
    socket: &str,
) -> Result<std::process::Child, String> {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};

    let exe = std::env::current_exe().map_err(|e| format!("finding this binary: {e}"))?;
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("agent.log"))
        .map_err(|e| format!("opening {}/agent.log: {e}", dir.display()))?;
    let mut cmd = Command::new(exe);
    cmd.arg("serve")
        .arg("--state-dir")
        .arg(dir)
        .args(["--forward-socket", socket])
        .arg("--no-member")
        .arg("--idle-exit")
        .arg(f.idle_exit.unwrap_or(300).to_string())
        .arg("--presence-interval")
        .arg(f.presence_interval.to_string())
        .arg("--away-after")
        .arg(f.away_after.to_string());
    if !f.allow_scope {
        cmd.arg("--no-scope");
    }
    cmd.current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(log)
        .process_group(0)
        .spawn()
        .map_err(|e| format!("starting the resident agent: {e}"))
}

/// Writes the header for the sshd-authenticated peer, then copies frames both ways until the
/// agent closes; stdin's end is passed on as a write shutdown.
#[cfg(unix)]
fn splice(stream: std::os::unix::net::UnixStream) -> Result<(), String> {
    use pt_frame::msg::{HUB_TAG, PeerHeader};
    use pt_json::ToJson;
    use std::io::{Read, Write};
    use std::net::Shutdown;

    let ip = std::env::var("SSH_CONNECTION")
        .ok()
        .and_then(|c| c.split_whitespace().next().map(str::to_string));
    let header = PeerHeader {
        peer_name: Some("ssh".into()),
        tags: vec![HUB_TAG.into()],
        ip,
    };
    let mut up = stream
        .try_clone()
        .map_err(|e| format!("agent socket: {e}"))?;
    up.write_all(format!("{}\n", header.to_json()).as_bytes())
        .map_err(|e| format!("writing to the agent: {e}"))?;
    std::thread::spawn(move || {
        let _ = std::io::copy(&mut std::io::stdin().lock(), &mut up);
        let _ = up.shutdown(Shutdown::Write);
    });
    let mut down = stream;
    let mut out = std::io::stdout().lock();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = match down.read(&mut buf) {
            Ok(0) => return Ok(()),
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(format!("reading from the agent: {e}")),
        };
        out.write_all(&buf[..n])
            .and_then(|()| out.flush())
            .map_err(|e| format!("writing stdout: {e}"))?;
    }
}

#[cfg(not(unix))]
pub fn run(_f: &Flags) -> Result<(), String> {
    Err("--stdio needs a unix socket for its resident agent; not on Windows yet".into())
}
