//! Antigravity quota, read only from the local `agy` CLI. No remote Google
//! endpoint is ever contacted.
use crate::proc::{kill_group, look_path, spawn};
use crate::{
    clamp_percent, parse_time, Ctx, Error, Reader, Result, Snapshot, Transport, Window, MAX_BODY,
};
use chrono::{DateTime, TimeZone, Utc};
use serde::Deserialize;
use serde_json::Value;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const POLL: Duration = Duration::from_millis(100);

#[derive(Clone, Debug)]
pub struct Antigravity {
    pub bin: String,
}

impl Default for Antigravity {
    fn default() -> Self {
        Self {
            bin: "agy".to_string(),
        }
    }
}

impl Antigravity {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Reader for Antigravity {
    fn provider(&self) -> &str {
        "antigravity"
    }

    fn read(&self, ctx: &Ctx, _http: &dyn Transport) -> Result<Snapshot> {
        let snap = Snapshot {
            provider: "antigravity".to_string(),
            source: "agy-cli".to_string(),
            ..Snapshot::default()
        };
        let bin = if self.bin.is_empty() {
            "agy"
        } else {
            &self.bin
        };
        let path = look_path(bin).ok_or_else(|| Error::unavailable("agy CLI not installed"))?;
        let stdout = run_agy(&path, ctx)?;
        parse_report(&stdout, snap)
    }
}

/// Runs the report command and returns at most MAX_BODY bytes of stdout.
fn run_agy(path: &Path, ctx: &Ctx) -> Result<Vec<u8>> {
    let mut cmd = Command::new(path);
    cmd.args([
        "-p",
        "/usage",
        "--output-format",
        "json",
        "--print-timeout",
        "90s",
    ]);
    cmd.current_dir(std::env::temp_dir());
    cmd.env("BROWSER", "false");
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = spawn(&mut cmd)
        .map_err(|e| Error::Other(format!("agy usage report failed: cannot start agy: {e}")))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| Error::Other("agy usage report failed: no stdout".to_string()))?;

    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    let reader = std::thread::spawn(move || {
        let mut r = stdout;
        let mut total = 0usize;
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            match r.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let take = (MAX_BODY - total).min(n);
                    let _ = tx.send(buf[..take].to_vec());
                    total += take;
                    if total >= MAX_BODY {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    let mut out: Vec<u8> = Vec::new();
    let limit = match ctx.remaining() {
        Some(r) => r.min(REPORT_TIMEOUT),
        None => REPORT_TIMEOUT,
    };
    let start = Instant::now();
    loop {
        match rx.recv_timeout(POLL) {
            Ok(chunk) => out.extend_from_slice(&chunk),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        if child.try_wait().ok().flatten().is_some() {
            break;
        }
        if start.elapsed() > limit {
            kill_group(&mut child);
            let _ = child.wait();
            let _ = reader.join();
            return Err(Error::Other(
                "agy usage report failed: timed out".to_string(),
            ));
        }
    }
    let status = child
        .wait()
        .map_err(|e| Error::Other(format!("agy usage report failed: {e}")))?;
    kill_group(&mut child);
    let _ = reader.join();
    if !status.success() {
        let code = status.code().unwrap_or(-1);
        return Err(Error::Other(format!(
            "agy usage report failed: exit status {code}"
        )));
    }
    Ok(out)
}

const REPORT_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Deserialize, Default)]
struct AgyBucket {
    #[serde(rename = "bucketId", default)]
    bucket_id: Option<String>,
    #[serde(rename = "bucket_id", default)]
    bucket_id_snake: Option<String>,
    #[serde(rename = "displayName", default)]
    display_name: Option<String>,
    #[serde(rename = "display_name", default)]
    display_name_snake: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(rename = "remainingFraction", default)]
    remaining_fraction: Option<f64>,
    #[serde(rename = "remaining_fraction", default)]
    remaining_snake: Option<f64>,
    #[serde(default)]
    remaining: Option<AgyRemaining>,
    #[serde(rename = "resetTime", default)]
    reset_time: Option<Value>,
    #[serde(rename = "reset_time", default)]
    reset_time_snake: Option<Value>,
    #[serde(default)]
    disabled: bool,
}

#[derive(Deserialize, Default)]
struct AgyRemaining {
    #[serde(rename = "remainingFraction", default)]
    f: Option<f64>,
    #[serde(rename = "remaining_fraction", default)]
    fs: Option<f64>,
}

#[derive(Deserialize, Default)]
struct AgyGroup {
    #[serde(rename = "displayName", default)]
    display_name: Option<String>,
    #[serde(rename = "display_name", default)]
    display_name_snake: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    buckets: Vec<AgyBucket>,
}

#[derive(Deserialize, Default)]
struct AgyReport {
    #[serde(default)]
    status: String,
    #[serde(default)]
    command: AgyCommand,
}

#[derive(Deserialize, Default)]
struct AgyCommand {
    #[serde(default)]
    name: String,
    #[serde(default)]
    data: AgyData,
}

#[derive(Deserialize, Default)]
struct AgyData {
    #[serde(default)]
    groups: Vec<AgyGroup>,
}

pub fn parse_report(data: &[u8], mut snap: Snapshot) -> Result<Snapshot> {
    let r: AgyReport = serde_json::from_slice(data)
        .map_err(|e| Error::Other(format!("decode agy report: {e}")))?;
    if r.status != "SUCCESS" || r.command.name != "usage" {
        return Err(Error::unavailable("agy usage report unsuccessful"));
    }
    for g in &r.command.data.groups {
        let gname = pick(&[&g.display_name, &g.display_name_snake, &g.name]);
        for b in &g.buckets {
            if b.disabled {
                continue;
            }
            let mut frac = b.remaining_fraction.or(b.remaining_snake);
            if frac.is_none() {
                if let Some(rem) = b.remaining.as_ref() {
                    frac = rem.f.or(rem.fs);
                }
            }
            let Some(frac) = frac else { continue };
            let mut name = pick(&[
                &b.display_name,
                &b.display_name_snake,
                &b.name,
                &b.bucket_id,
                &b.bucket_id_snake,
            ]);
            if !gname.is_empty() {
                name = format!("{gname} {name}");
            }
            snap.windows.push(Window {
                name: name.trim().to_string(),
                used_percent: clamp_percent((1.0 - frac) * 100.0),
                resets_at: agy_time(
                    b.reset_time
                        .as_ref()
                        .or(b.reset_time_snake.as_ref())
                        .unwrap_or(&Value::Null),
                ),
            });
        }
    }
    if snap.windows.is_empty() {
        return Err(Error::unavailable("agy usage report has no known quota"));
    }
    Ok(snap)
}

fn pick(candidates: &[&Option<String>]) -> String {
    for s in candidates.iter().copied().flatten() {
        if !s.is_empty() {
            return s.clone();
        }
    }
    String::new()
}

/// Reset times arrive as RFC 3339 strings or as unix-second numbers.
pub fn agy_time(raw: &Value) -> Option<DateTime<Utc>> {
    match raw {
        Value::String(s) => parse_time(s),
        Value::Number(n) => n
            .as_f64()
            .and_then(|f| Utc.timestamp_opt(f as i64, 0).single()),
        _ => None,
    }
}
