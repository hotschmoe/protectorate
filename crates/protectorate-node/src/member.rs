//! The node's `member --mode node`: what `join` records in `<state>/node.json`, how member is
//! started from it, and the supervisor `serve` runs it under (restart with backoff).

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use pt_json::record;

const BACKOFF_MIN: Duration = Duration::from_secs(1);
const BACKOFF_MAX: Duration = Duration::from_secs(60);
const STABLE_RUN: Duration = Duration::from_secs(60);

record! {
    /// Written by `join` once the hub approved the node.
    #[derive(Debug, Clone, PartialEq)]
    pub struct NodeConfig {
        pub server_url: String,
        pub name: String,
        /// The control key fingerprint member pinned at enrollment.
        pub control_key: String,
        /// member's fixed WireGuard UDP port; 0 picks one.
        pub udp_port: u16 = 0,
    }
}

pub fn config_path(state: &Path) -> PathBuf {
    state.join("node.json")
}

pub fn member_dir(state: &Path) -> PathBuf {
    state.join("member")
}

pub fn read_config(state: &Path) -> Result<Option<NodeConfig>, String> {
    let path = config_path(state);
    match std::fs::read(&path) {
        Ok(b) => pt_json::from_bytes(&b)
            .map(Some)
            .map_err(|e| format!("{}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("reading {}: {e}", path.display())),
    }
}

/// `member` next to this binary, unless given.
pub fn member_binary(given: Option<&Path>) -> Result<PathBuf, String> {
    let path = match given {
        Some(p) => p.to_path_buf(),
        None => {
            let exe = std::env::current_exe().map_err(|e| format!("finding this binary: {e}"))?;
            let name = if cfg!(windows) {
                "member.exe"
            } else {
                "member"
            };
            exe.parent()
                .map(|d| d.join(name))
                .ok_or("this binary has no directory")?
        }
    };
    if path.is_file() {
        Ok(path)
    } else {
        Err(format!(
            "no member binary at {} (pass --member PATH)",
            path.display()
        ))
    }
}

/// member for this node, with the knobs its control URL needs.
pub fn command(member: &Path, state: &Path, server_url: &str, name: &str) -> Command {
    let mut cmd = Command::new(member);
    cmd.arg("--mode")
        .arg("node")
        .arg("--state-dir")
        .arg(member_dir(state))
        .args(["--control-url", server_url, "--hostname", name]);
    if server_url.starts_with("http://") {
        // A LAN-direct headscale serves DERP over plain HTTP too (REVISIT.md).
        cmd.env("TS_DEBUG_USE_DERP_HTTP", "1");
    }
    cmd
}

/// Runs member forwarding accepted tailnet connections to `forward` (the agent's socket or
/// loopback port) for as long as this process lives, restarting it with backoff. Its log lines
/// go to stderr with a `member:` prefix.
pub fn supervise(member: PathBuf, state: PathBuf, cfg: NodeConfig, forward: String) {
    let mut backoff = BACKOFF_MIN;
    loop {
        let mut cmd = command(&member, &state, &cfg.server_url, &cfg.name);
        cmd.arg("--forward")
            .arg(&forward)
            .args(["--udp-port", &cfg.udp_port.to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        // member dies with the agent's supervising thread (Linux; REVISIT.md for Windows).
        let _ = pt_sys::pdeathsig_on_spawn(&mut cmd, pt_sys::SIGTERM);
        let started = Instant::now();
        match cmd.spawn() {
            Ok(mut child) => {
                eprintln!(
                    "protectorate-node: member started (pid {}), control {}",
                    child.id(),
                    cfg.server_url
                );
                if let Some(err) = child.stderr.take() {
                    thread::spawn(move || {
                        for line in BufReader::new(err).lines() {
                            let Ok(line) = line else { break };
                            eprintln!("member: {line}");
                        }
                    });
                }
                match child.wait() {
                    Ok(status) => eprintln!("protectorate-node: member exited: {status}"),
                    Err(e) => eprintln!("protectorate-node: waiting for member: {e}"),
                }
            }
            Err(e) => eprintln!(
                "protectorate-node: starting member {}: {e}",
                member.display()
            ),
        }
        if started.elapsed() >= STABLE_RUN {
            backoff = BACKOFF_MIN;
        }
        eprintln!(
            "protectorate-node: restarting member in {} s",
            backoff.as_secs()
        );
        thread::sleep(backoff);
        backoff = (backoff * 2).min(BACKOFF_MAX);
    }
}
