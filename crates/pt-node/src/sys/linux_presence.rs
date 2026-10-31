//! Linux presence: logind session hints, refined by /dev/input event activity when readable
//! (root, or the `input` group). logind alone is a snapshot; evdev needs a long-lived Detector.

use crate::exec;
use crate::presence::{Config, Observation, Session};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{ErrorKind, Read};
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const RESCAN: Duration = Duration::from_secs(30);
const EV_KEY: u64 = 1 << 1;
const EV_REL: u64 = 1 << 2;
const EV_ABS: u64 = 1 << 3;

pub struct Platform {
    evdev: Evdev,
}

impl Platform {
    pub fn new() -> Platform {
        Platform {
            evdev: Evdev::new(),
        }
    }

    pub fn observe(&mut self, cfg: &Config) -> Observation {
        let lg = logind();
        let ev = self.evdev.poll();
        let ev_note = ev.note();
        if let Some(idle) = ev.exact_idle {
            return Observation {
                session: Session::Present,
                idle_secs: Some(idle),
                locked: lg.locked,
                source: "evdev",
                detail: format!("{ev_note}; logind: {}", lg.detail),
                alt_idle_secs: lg.idle,
            };
        }
        if let Some(bound) = ev.quiet_for.filter(|b| *b >= cfg.away_after_secs) {
            return Observation {
                session: Session::Present,
                idle_secs: Some(bound),
                locked: lg.locked,
                source: "evdev",
                detail: format!("{ev_note}; logind: {}", lg.detail),
                alt_idle_secs: lg.idle,
            };
        }
        Observation {
            session: lg.session,
            idle_secs: lg.idle,
            locked: lg.locked,
            source: "logind",
            detail: format!("{}; {ev_note}", lg.detail),
            alt_idle_secs: None,
        }
    }
}

struct Logind {
    session: Session,
    idle: Option<u64>,
    locked: Option<bool>,
    detail: String,
}

fn failed(detail: impl Into<String>) -> Logind {
    Logind {
        session: Session::Failed,
        idle: None,
        locked: None,
        detail: detail.into(),
    }
}

fn now_usec() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0)
}

const PROPS: &[&str] = &[
    "Id",
    "Seat",
    "Class",
    "Type",
    "Active",
    "State",
    "Remote",
    "IdleHint",
    "IdleSinceHint",
    "LockedHint",
];

fn show_session(id: &str) -> Option<HashMap<String, String>> {
    let mut args = vec!["show-session", id];
    let flags: Vec<String> = PROPS.iter().map(|p| format!("--property={p}")).collect();
    args.extend(flags.iter().map(String::as_str));
    let out = exec::run("loginctl", &args, Duration::from_secs(5))?;
    if !out.success {
        return None;
    }
    Some(
        out.stdout
            .lines()
            .filter_map(|l| {
                let (k, v) = l.split_once('=')?;
                Some((k.to_string(), v.to_string()))
            })
            .collect(),
    )
}

/// Reads logind's view of local seat sessions. Remote logins (no seat, or Remote=yes) are skipped.
fn logind() -> Logind {
    let Some(list) = exec::run(
        "loginctl",
        &["list-sessions", "--no-legend"],
        Duration::from_secs(5),
    ) else {
        return failed("loginctl not runnable");
    };
    if !list.success {
        return failed(format!(
            "loginctl list-sessions failed: {}",
            list.stderr.trim()
        ));
    }
    let now = now_usec();
    let (mut seat_sessions, mut greeters) = (0usize, 0usize);
    let mut active: Vec<(Option<u64>, bool, bool)> = Vec::new(); // idle, idle_hint_known, locked
    let mut future = false;
    for id in list
        .stdout
        .lines()
        .filter_map(|l| l.split_whitespace().next())
    {
        let Some(p) = show_session(id) else { continue };
        let get = |k: &str| p.get(k).map(String::as_str).unwrap_or("");
        if get("Seat").is_empty() || get("Remote") == "yes" || get("State") == "closing" {
            continue;
        }
        seat_sessions += 1;
        if get("Class") == "greeter" {
            greeters += 1;
            continue;
        }
        if get("Active") != "yes" {
            continue;
        }
        let locked = get("LockedHint") == "yes";
        let idle = if get("IdleHint") == "yes" {
            match get("IdleSinceHint").parse::<u64>() {
                Ok(since) if since > now + 2_000_000 => {
                    future = true;
                    None
                }
                Ok(since) if since > 0 => Some(now.saturating_sub(since) / 1_000_000),
                _ => None,
            }
        } else {
            None
        };
        active.push((idle, get("IdleHint") == "yes", locked));
    }
    if future {
        return failed("IdleSinceHint is in the future");
    }
    if active.is_empty() {
        return Logind {
            session: Session::Absent,
            idle: None,
            locked: None,
            detail: format!(
                "no active local user session ({seat_sessions} seat sessions, {greeters} greeter)"
            ),
        };
    }
    // Several active seats: the most recent activity wins, an unknown idle means present.
    let idle = active
        .iter()
        .map(|a| a.0)
        .try_fold(u64::MAX, |m, i| i.map(|i| m.min(i)));
    let locked = Some(active.iter().all(|a| a.2));
    let hint = active.iter().any(|a| a.1);
    Logind {
        session: Session::Present,
        idle,
        locked,
        detail: format!(
            "{} active local session(s), IdleHint={}",
            active.len(),
            if hint { "yes" } else { "no" }
        ),
    }
}

struct Dev {
    path: PathBuf,
    file: File,
}

struct EvReading {
    exact_idle: Option<u64>,
    quiet_for: Option<u64>,
    devices: usize,
    denied: usize,
}

impl EvReading {
    fn note(&self) -> String {
        if self.devices > 0 {
            format!("evdev watching {} devices", self.devices)
        } else if self.denied > 0 {
            format!("evdev unreadable ({} devices denied)", self.denied)
        } else {
            "evdev: no input devices".to_string()
        }
    }
}

struct Evdev {
    devs: Vec<Dev>,
    denied: usize,
    scanned: Option<Instant>,
    started: Instant,
    last_event: Option<Instant>,
}

impl Evdev {
    fn new() -> Evdev {
        Evdev {
            devs: Vec::new(),
            denied: 0,
            scanned: None,
            started: Instant::now(),
            last_event: None,
        }
    }

    fn scan(&mut self) {
        self.scanned = Some(Instant::now());
        self.denied = 0;
        let Ok(rd) = fs::read_dir("/sys/class/input") else {
            return;
        };
        let mut names: Vec<String> = rd
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| {
                n.strip_prefix("event")
                    .is_some_and(|d| d.bytes().all(|b| b.is_ascii_digit()))
            })
            .collect();
        names.sort();
        for name in names {
            let node = PathBuf::from("/dev/input").join(&name);
            if !is_human_device(&name) {
                continue;
            }
            if self.devs.iter().any(|d| d.path == node) {
                continue;
            }
            match fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NONBLOCK)
                .open(&node)
            {
                Ok(file) => self.devs.push(Dev { path: node, file }),
                Err(e) if e.kind() == ErrorKind::PermissionDenied => self.denied += 1,
                Err(_) => {}
            }
        }
    }

    fn poll(&mut self) -> EvReading {
        if self.scanned.is_none_or(|t| t.elapsed() >= RESCAN) {
            self.scan();
        }
        let mut buf = [0u8; 24 * 64];
        let mut gone = Vec::new();
        let mut seen = false;
        for (i, d) in self.devs.iter_mut().enumerate() {
            loop {
                match d.file.read(&mut buf) {
                    Ok(0) => {
                        gone.push(i);
                        break;
                    }
                    Ok(n) => {
                        if buf[..n].as_chunks::<24>().0.iter().any(|ev| {
                            let ty = u16::from_ne_bytes([ev[16], ev[17]]);
                            (1..=3).contains(&ty)
                        }) {
                            seen = true;
                        }
                    }
                    Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                    Err(e) if e.kind() == ErrorKind::Interrupted => {}
                    Err(_) => {
                        gone.push(i);
                        break;
                    }
                }
            }
        }
        for i in gone.into_iter().rev() {
            self.devs.remove(i);
        }
        if seen {
            self.last_event = Some(Instant::now());
        }
        let watching = !self.devs.is_empty();
        EvReading {
            exact_idle: self
                .last_event
                .filter(|_| watching)
                .map(|t| t.elapsed().as_secs()),
            quiet_for: (watching && self.last_event.is_none())
                .then(|| self.started.elapsed().as_secs()),
            devices: self.devs.len(),
            denied: self.denied,
        }
    }
}

fn hex_words(s: &str) -> Vec<u64> {
    s.split_whitespace()
        .rev()
        .map(|w| u64::from_str_radix(w, 16).unwrap_or(0))
        .collect()
}

fn has_bit(words: &[u64], n: usize) -> bool {
    words.get(n / 64).is_some_and(|w| w >> (n % 64) & 1 == 1)
}

/// Keyboards, mice and touch surfaces that are real hardware. Lid and power buttons, uinput and
/// other virtual devices are skipped.
fn is_human_device(event: &str) -> bool {
    let base = PathBuf::from("/sys/class/input").join(event);
    if fs::read_link(&base).is_ok_and(|l| l.to_string_lossy().contains("/devices/virtual/")) {
        return false;
    }
    let cap = |n: &str| {
        fs::read_to_string(base.join("device/capabilities").join(n))
            .map(|s| hex_words(s.trim()))
            .unwrap_or_default()
    };
    let ev = cap("ev").first().copied().unwrap_or(0);
    let key = cap("key");
    ev & EV_REL != 0
        || (ev & EV_KEY != 0 && (has_bit(&key, 30) || has_bit(&key, 330)))
        || (ev & EV_ABS != 0 && has_bit(&key, 330))
}
