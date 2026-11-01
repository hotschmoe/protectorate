//! `protectorate-node join`: scan to join, the node's half (SPEC 3.2). member registers with
//! headscale and pins its control key; this shows the registration as a QR code and as text, waits
//! until the owner approves it on the hub, and records `node.json` for `serve`. Nothing is
//! downloaded and nothing on the hub is called.

use std::io::{BufRead, BufReader, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use pt_json::{ToJson, Value, obj};

use crate::member::{self, NodeConfig};

pub const USAGE: &str =
    "  join SERVER_URL [--name NAME] [--state-dir DIR] [--member PATH] [--udp-port N]
                                    register with the hub's headscale: shows a QR code and the
                                    hub command that approves it, waits, then records node.json
                                    (state dir default /var/lib/protectorate-node as root)";

/// headscale keeps a pending registration this long.
const APPROVAL_WAIT: Duration = Duration::from_secs(15 * 60);
const REGISTRATION_WAIT: Duration = Duration::from_secs(60);
pub const SYSTEM_STATE: &str = "/var/lib/protectorate-node";

pub fn run(args: &[String]) -> Result<(), String> {
    let mut url = None;
    let mut name = None;
    let mut state = None;
    let mut member_bin = None;
    let mut udp_port = 0u16;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = || it.next().ok_or_else(|| format!("{a} needs a value"));
        match a.as_str() {
            "--name" => name = Some(value()?.clone()),
            "--state-dir" => state = Some(PathBuf::from(value()?)),
            "--member" => member_bin = Some(PathBuf::from(value()?)),
            "--udp-port" => {
                let v = value()?;
                udp_port = v
                    .parse()
                    .map_err(|_| format!("--udp-port: '{v}' is not a port"))?;
            }
            s if s.starts_with("--") => return Err(format!("join: unknown option '{s}'")),
            _ if url.is_none() => url = Some(a.clone()),
            _ => return Err(format!("join: unexpected argument '{a}'")),
        }
    }
    let url = url.ok_or("join needs the hub's SERVER_URL")?;
    let url = url.trim_end_matches('/').to_string();
    check_url(&url)?;
    let name = match name {
        Some(n) => {
            check_name(&n)?;
            n
        }
        None => default_name(&pt_node::hostname())?,
    };
    let state = state_dir(state)?;
    if let Some(cfg) = member::read_config(&state)? {
        return Err(format!(
            "{} already joined {} as {}; to join again, uninstall and use a fresh state dir",
            state.display(),
            cfg.server_url,
            cfg.name
        ));
    }
    create_private(&state)?;
    create_private(&member::member_dir(&state))?;
    let member_bin = member::member_binary(member_bin.as_deref(), &state)?;

    let log_path = state.join("join-member.log");
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|e| format!("opening {}: {e}", log_path.display()))?;
    let mut cmd = member::command(&member_bin, &state, &url, &name);
    cmd.arg("--print-registration")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(log);
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("starting {}: {e}", member_bin.display()))?;
    let stdout = child.stdout.take().ok_or("member has no stdout")?;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let result = wait_approval(&rx, &url, &name, &state, &log_path);
    let result = result.and_then(|key| {
        let status = child
            .wait()
            .map_err(|e| format!("waiting for member: {e}"))?;
        if !status.success() {
            return Err(format!(
                "member exited {status} after approval; see {}",
                log_path.display()
            ));
        }
        Ok(key)
    });
    let control_key = match result {
        Ok(k) => k,
        Err(e) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(e);
        }
    };
    let cfg = NodeConfig {
        server_url: url,
        name,
        control_key,
        udp_port,
    };
    write_private(
        &member::config_path(&state),
        format!("{}\n", cfg.to_json()).as_bytes(),
    )?;
    println!(
        "Joined. Wrote {}.\nNext: protectorate-node install-service (as root or an elevated administrator), or\n  protectorate-node serve --state-dir {} --listen-tcp {}",
        member::config_path(&state).display(),
        state.display(),
        crate::service::LISTEN
    );
    Ok(())
}

/// Shows the registration, waits for approval, checks member's pin; returns the pinned key.
fn wait_approval(
    rx: &mpsc::Receiver<String>,
    url: &str,
    name: &str,
    state: &Path,
    log: &Path,
) -> Result<String, String> {
    let reg = next_event(rx, "registration", REGISTRATION_WAIT).map_err(|e| {
        format!(
            "{e}; is {url} reachable and serving headscale? See {}",
            log.display()
        )
    })?;
    let field = |k: &str| {
        reg.get(k)
            .and_then(Value::as_str)
            .map(String::from)
            .ok_or_else(|| format!("member's registration has no {k}: {reg}"))
    };
    let auth_id = field("auth_id")?;
    let key = field("control_key")?;
    show(url, name, &auth_id, &key)?;
    let started = Instant::now();
    let done = next_event(rx, "registered", APPROVAL_WAIT).map_err(|_| {
        format!(
            "not approved within {} minutes, and headscale has dropped the registration; run join again",
            APPROVAL_WAIT.as_secs() / 60
        )
    })?;
    let pin_path = member::member_dir(state).join("control-key.pin");
    let pinned = std::fs::read_to_string(&pin_path)
        .map_err(|e| format!("reading {}: {e}", pin_path.display()))?;
    if pinned.trim() != key {
        return Err(format!(
            "{} holds {}, but member registered with {key}",
            pin_path.display(),
            pinned.trim()
        ));
    }
    let ips: Vec<&str> = done
        .get("ips")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    println!(
        "Approved after {} s. Tailnet address {}. Control key {key} is pinned in {}.",
        started.elapsed().as_secs(),
        ips.join(", "),
        pin_path.display()
    );
    Ok(key)
}

fn next_event(rx: &mpsc::Receiver<String>, event: &str, within: Duration) -> Result<Value, String> {
    let deadline = Instant::now() + within;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        let line = rx.recv_timeout(left).map_err(|e| match e {
            mpsc::RecvTimeoutError::Timeout => {
                format!("no {event} from member within {} s", within.as_secs())
            }
            mpsc::RecvTimeoutError::Disconnected => format!("member exited before {event}"),
        })?;
        if let Ok(v) = pt_json::parse(&line)
            && v.get("event").and_then(Value::as_str) == Some(event)
        {
            return Ok(v);
        }
    }
}

fn show(url: &str, name: &str, auth_id: &str, key: &str) -> Result<(), String> {
    let payload =
        obj! { "v" => 1, "auth_id" => auth_id, "control_key" => key, "name" => name }.to_string();
    let qr = pt_qr::encode(payload.as_bytes(), pt_qr::Ecc::Medium)
        .map_err(|e| format!("QR code: {e}"))?;
    let mut out = std::io::stdout().lock();
    let ansi = out.is_terminal();
    let text = format!(
        "{}
Scan this in the Protectorate app, or approve it on the hub:

  protectorate node approve {auth_id} --name {name} --fingerprint {key}

  name         {name}
  auth id      {auth_id}
  fingerprint  {key}
  control      {url}
  paste        {payload}

Waiting for approval (headscale keeps it for 15 minutes)...
",
        qr.to_terminal(4, ansi)
    );
    out.write_all(text.as_bytes())
        .and_then(|()| out.flush())
        .map_err(|e| format!("writing stdout: {e}"))
}

fn check_url(url: &str) -> Result<(), String> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .ok_or_else(|| format!("{url}: SERVER_URL must start with http:// or https://"))?;
    if rest.is_empty() || rest.contains('/') || rest.contains(char::is_whitespace) {
        return Err(format!(
            "{url}: SERVER_URL is scheme://host[:port], no path"
        ));
    }
    Ok(())
}

fn check_name(name: &str) -> Result<(), String> {
    let ok = !name.is_empty()
        && name.len() <= 63
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !name.starts_with('-')
        && !name.ends_with('-');
    if ok {
        Ok(())
    } else {
        Err(format!(
            "--name {name:?}: use 1 to 63 lowercase letters, digits and inner hyphens"
        ))
    }
}

/// The hostname as a tailnet label: lowercase, other characters as hyphens.
fn default_name(host: &str) -> Result<String, String> {
    let host = host.split('.').next().unwrap_or("");
    let mut s: String = host
        .chars()
        .map(|c| {
            let c = c.to_ascii_lowercase();
            if c.is_ascii_lowercase() || c.is_ascii_digit() {
                c
            } else {
                '-'
            }
        })
        .collect();
    s.truncate(63);
    let s = s.trim_matches('-').to_string();
    check_name(&s)
        .map(|()| s)
        .map_err(|_| format!("hostname {host:?} makes no node name; pass --name"))
}

fn state_dir(given: Option<PathBuf>) -> Result<PathBuf, String> {
    if let Some(d) = given {
        return Ok(d);
    }
    if (cfg!(target_os = "linux") && is_root()) || cfg!(windows) {
        return system_state();
    }
    Err(
        "pass --state-dir DIR (the default, /var/lib/protectorate-node, is for root on Linux)"
            .into(),
    )
}

/// The service's state dir: /var/lib/protectorate-node, or %ProgramData%\protectorate-node.
pub fn system_state() -> Result<PathBuf, String> {
    if cfg!(windows) {
        let base = std::env::var_os("ProgramData").ok_or("ProgramData is not set")?;
        Ok(PathBuf::from(base).join("protectorate-node"))
    } else {
        Ok(PathBuf::from(SYSTEM_STATE))
    }
}

#[cfg(unix)]
pub fn is_root() -> bool {
    use std::os::unix::fs::MetadataExt;
    // /proc/self belongs to the effective uid.
    std::fs::metadata("/proc/self").is_ok_and(|m| m.uid() == 0)
}

#[cfg(not(unix))]
pub fn is_root() -> bool {
    false
}

pub fn create_private(dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("creating {}: {e}", dir.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| format!("restricting {}: {e}", dir.display()))?;
    }
    Ok(())
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let tmp = path.with_extension("tmp");
    let ctx = |e: std::io::Error| format!("writing {}: {e}", path.display());
    let mut f = std::fs::File::create(&tmp).map_err(ctx)?;
    f.write_all(bytes)
        .and_then(|()| f.sync_all())
        .map_err(ctx)?;
    std::fs::rename(&tmp, path).map_err(ctx)
}
