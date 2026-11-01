//! The hub's `member --mode hub`: its one-time enrollment (the hub registers and tags itself),
//! the supervised run, and the unix socket it serves: STATUS and DIAL.

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use pt_json::Value;

use crate::client::DeadlineStream;
use crate::headscale::{Admin, HsNode, TAG_HUB};
use crate::{Config, Error, Result};

const LINE_MAX: usize = 1024;
const ENROLL_TIMEOUT: Duration = Duration::from_secs(60);
const STATUS_TIMEOUT: Duration = Duration::from_secs(10);
/// member's own dial timeout is 30 s; its answer line comes after that.
const DIAL_TIMEOUT: Duration = Duration::from_secs(35);

fn connect_timeout(normal: Duration) -> Duration {
    #[cfg(debug_assertions)]
    if let Some(ms) = std::env::var("PROTECTORATE_FLEET_TEST_MEMBER_TIMEOUT_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
    {
        return Duration::from_millis(ms.max(1));
    }
    normal
}

pub(crate) fn state_dir(cfg: &Config) -> PathBuf {
    cfg.dir.join("member")
}

pub(crate) fn enrolled(cfg: &Config) -> bool {
    state_dir(cfg).join("tailscaled.state").exists()
}

/// The environment member gets: nothing of the hub's, plus the knobs it needs.
fn base_command(cfg: &Config) -> Command {
    let mut cmd = Command::new(&cfg.member);
    cmd.env_clear();
    for (k, v) in std::env::vars_os() {
        let k_str = k.to_string_lossy();
        if k_str == "PATH" || k_str.starts_with("TS_DEBUG_") {
            cmd.env(&k, v);
        }
    }
    if !cfg.derp.tls {
        // Plain-HTTP DERP for a LAN-direct headscale (tests); REVISIT.md.
        cmd.env("TS_DEBUG_USE_DERP_HTTP", "1");
    }
    cmd
}

fn control_url(cfg: &Config) -> String {
    format!("http://{}", cfg.listen)
}

/// Registers the hub's own member: `--print-registration`, a fingerprint check against
/// headscale's `/key`, then `auth register` and `tag:hub`.
pub(crate) fn enroll(cfg: &Config, admin: &Admin, fingerprint: &str) -> Result<HsNode> {
    let dir = state_dir(cfg);
    crate::private_dir(&dir)?;
    let mut cmd = base_command(cfg);
    cmd.args(["--mode", "hub", "--state-dir"])
        .arg(&dir)
        .args([
            "--control-url",
            &control_url(cfg),
            "--hostname",
            &cfg.hub_name,
        ])
        .arg("--print-registration")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::from(crate::append_log(
            &cfg.dir.join("member-enroll.log"),
        )?));
    let mut child = cmd.spawn().map_err(|e| {
        Error::new(format!(
            "starting {} for enrollment: {e}",
            cfg.member.display()
        ))
    })?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| Error::new("member has no stdout"))?;
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let result = (|| {
        let reg = next_event(&rx, "registration")?;
        let key = reg.get("control_key").and_then(Value::as_str).unwrap_or("");
        if key != fingerprint {
            return Err(Error::new(format!(
                "hub member sees control key {key}, headscale serves {fingerprint}"
            )));
        }
        let auth_id = reg
            .get("auth_id")
            .and_then(Value::as_str)
            .ok_or_else(|| Error::new(format!("registration without auth_id: {reg}")))?;
        let node = admin.register(auth_id)?;
        next_event(&rx, "registered")?;
        let node = admin.tag(node.id, TAG_HUB)?;
        Ok(node)
    })();
    match &result {
        Ok(_) => {
            let status = child
                .wait()
                .map_err(|e| Error::new(format!("waiting for the enrolling member: {e}")))?;
            if !status.success() {
                return Err(Error::new(format!("enrolling member exited {status}")));
            }
        }
        Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
    result.map_err(|e| e.context("enrolling the hub member"))
}

fn next_event(rx: &mpsc::Receiver<String>, event: &str) -> Result<Value> {
    loop {
        let line = rx.recv_timeout(ENROLL_TIMEOUT).map_err(|_| {
            Error::new(format!(
                "no {event} event from member within {} s",
                ENROLL_TIMEOUT.as_secs()
            ))
        })?;
        if let Ok(v) = pt_json::parse(&line)
            && v.get("event").and_then(Value::as_str) == Some(event)
        {
            return Ok(v);
        }
    }
}

/// The first `member.sock`, `member-2.sock`, ... that does not exist. member refuses a path that
/// exists, and one left by a killed member is never removed (REVISIT.md).
pub(crate) fn fresh_socket(cfg: &Config) -> Result<PathBuf> {
    let dir = &cfg.dir;
    for n in 1..1000u32 {
        let name = if n == 1 {
            "member.sock".to_string()
        } else {
            format!("member-{n}.sock")
        };
        let p = dir.join(name);
        if !p.exists() {
            if p.as_os_str().len() > 100 {
                return Err(Error::new(format!(
                    "socket path {} is too long for a unix socket",
                    p.display()
                )));
            }
            return Ok(p);
        }
    }
    Err(Error::new(format!(
        "no free member socket name in {}",
        dir.display()
    )))
}

pub(crate) fn run_command(cfg: &Config, socket: &Path) -> Command {
    let mut cmd = base_command(cfg);
    cmd.args(["--mode", "hub", "--state-dir"])
        .arg(state_dir(cfg))
        .args([
            "--control-url",
            &control_url(cfg),
            "--hostname",
            &cfg.hub_name,
            "--socket",
        ])
        .arg(socket)
        .args(["--udp-port", &cfg.hub_udp_port.to_string()]);
    cmd
}

/// One peer in member's STATUS.
#[derive(Clone, Debug, PartialEq)]
pub struct Peer {
    pub name: String,
    pub dns_name: String,
    /// headscale's node id, as a string.
    pub stable_id: String,
    pub ips: Vec<String>,
    pub tags: Vec<String>,
    pub online: bool,
    /// The direct UDP path, when there is one.
    pub cur_addr: Option<String>,
    /// The DERP region relaying, when there is no direct path.
    pub relay: Option<String>,
    pub last_handshake: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Status {
    pub backend_state: String,
    pub control_key: String,
    pub health: Vec<String>,
    pub this: Option<Peer>,
    pub peers: Vec<Peer>,
}

fn strings(v: &Value, k: &str) -> Vec<String> {
    v.get(k)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

fn opt(v: &Value, k: &str) -> Option<String> {
    v.get(k)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(String::from)
}

impl Peer {
    fn parse(v: &Value) -> Peer {
        Peer {
            name: opt(v, "name").unwrap_or_default(),
            dns_name: opt(v, "dns_name").unwrap_or_default(),
            stable_id: opt(v, "stable_id").unwrap_or_default(),
            ips: strings(v, "ips"),
            tags: strings(v, "tags"),
            online: v.get("online").and_then(Value::as_bool).unwrap_or(false),
            cur_addr: opt(v, "cur_addr"),
            relay: opt(v, "relay"),
            last_handshake: opt(v, "last_handshake"),
        }
    }
}

impl Status {
    fn parse(v: &Value) -> Status {
        Status {
            backend_state: opt(v, "backend_state").unwrap_or_default(),
            control_key: opt(v, "control_key").unwrap_or_default(),
            health: strings(v, "health"),
            this: v.get("self").filter(|s| !s.is_null()).map(Peer::parse),
            peers: v
                .get("peers")
                .and_then(Value::as_array)
                .map(|a| a.iter().map(Peer::parse).collect())
                .unwrap_or_default(),
        }
    }

    /// The peer for headscale node `id`.
    pub fn peer(&self, id: u64) -> Option<&Peer> {
        let id = id.to_string();
        self.peers.iter().find(|p| p.stable_id == id)
    }
}

fn connect(socket: &Path, timeout: Duration) -> Result<UnixStream> {
    let s = pt_sys::connect_unix_timeout(socket, connect_timeout(timeout))
        .map_err(|e| Error::new(format!("connecting to {}: {e}", socket.display())))?;
    s.set_read_timeout(Some(timeout))
        .and_then(|()| s.set_write_timeout(Some(timeout)))
        .map_err(|e| Error::new(format!("{}: {e}", socket.display())))?;
    Ok(s)
}

/// Reads one `\n`-terminated line a byte at a time, so nothing after it is consumed.
fn read_line(s: &mut impl Read) -> Result<String> {
    let mut line = Vec::new();
    let mut b = [0u8; 1];
    loop {
        let n = s
            .read(&mut b)
            .map_err(|e| Error::new(format!("reading member's answer: {e}")))?;
        if n == 0 {
            return Err(Error::new("member closed the connection without an answer"));
        }
        if b[0] == b'\n' {
            return Ok(String::from_utf8_lossy(&line).into_owned());
        }
        line.push(b[0]);
        if line.len() > LINE_MAX {
            return Err(Error::new("member's answer line is too long"));
        }
    }
}

pub(crate) fn status(socket: &Path) -> Result<Status> {
    let timeout = STATUS_TIMEOUT;
    let deadline = Instant::now() + timeout;
    let mut s = DeadlineStream {
        stream: connect(socket, timeout)?,
        deadline,
    };
    s.write_all(b"STATUS\n")
        .map_err(|e| Error::new(format!("member STATUS: {e}")))?;
    let mut body = Vec::new();
    s.take(16 << 20)
        .read_to_end(&mut body)
        .map_err(|e| Error::new(format!("member STATUS: {e}")))?;
    let text = String::from_utf8_lossy(&body);
    let text = text.trim();
    if let Some(e) = text.strip_prefix("ERR ") {
        return Err(Error::new(format!("member STATUS: {e}")));
    }
    let v = pt_json::parse(text).map_err(|e| Error::new(format!("member STATUS: {e}")))?;
    Ok(Status::parse(&v))
}

/// A raw stream to `target` (`<tailnet ip or name>:<port>`) through the hub member.
pub(crate) fn dial(socket: &Path, target: &str) -> Result<UnixStream> {
    let timeout = DIAL_TIMEOUT;
    let deadline = Instant::now() + timeout;
    let mut s = DeadlineStream {
        stream: connect(socket, timeout)?,
        deadline,
    };
    s.write_all(format!("DIAL {target}\n").as_bytes())
        .map_err(|e| Error::new(format!("DIAL {target}: {e}")))?;
    let line = read_line(&mut s).map_err(|e| e.context(format!("DIAL {target}")))?;
    match line.as_str() {
        "OK" => Ok(s.stream),
        other => Err(Error::new(format!(
            "DIAL {target}: {}",
            other.strip_prefix("ERR ").unwrap_or(other)
        ))),
    }
}
