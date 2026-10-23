//! Codex subscription usage. The preferred source is the Codex CLI's own
//! `app-server` JSON-RPC; the chatgpt.com usage endpoints are the fallback and
//! the only source of reset credits.
use crate::http::{bearer_headers, get_json};
use crate::proc::{kill_group, spawn, ManagedChild};
use crate::{
    clamp_percent, home_dir, parse_time, read_cred_file, sort_by_expiry, window_label, Ctx, Error,
    Reader, Reset, Result, Snapshot, Transport, Window,
};
use chrono::{DateTime, TimeZone, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

const DEFAULT_BASE: &str = "https://chatgpt.com/backend-api";
const RPC_CLOSE_WAIT: Duration = Duration::from_secs(3);
const RPC_POLL: Duration = Duration::from_millis(50);
const MAX_LINE: usize = 4 << 20;

#[derive(Clone, Debug)]
pub struct Codex {
    pub home: Option<PathBuf>,
    pub bin: String,
    pub base_url: String,
}

impl Default for Codex {
    fn default() -> Self {
        Self {
            home: None,
            bin: "codex".to_string(),
            base_url: DEFAULT_BASE.to_string(),
        }
    }
}

impl Codex {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_home(mut self, home: PathBuf) -> Self {
        self.home = Some(home);
        self
    }
    pub fn with_bin(mut self, bin: &str) -> Self {
        self.bin = bin.to_string();
        self
    }
    pub fn with_base_url(mut self, base: &str) -> Self {
        self.base_url = base.trim_end_matches('/').to_string();
        self
    }
    fn base(&self) -> String {
        if self.base_url.is_empty() {
            DEFAULT_BASE.to_string()
        } else {
            self.base_url.clone()
        }
    }
}

impl Reader for Codex {
    fn provider(&self) -> &str {
        "codex"
    }

    fn read(&self, ctx: &Ctx, http: &dyn Transport) -> Result<Snapshot> {
        let mut snap = Snapshot {
            provider: "codex".to_string(),
            ..Snapshot::default()
        };
        let home = home_dir(self.home.as_deref())?;
        let raw = read_cred_file(&home.join(".codex").join("auth.json"))?;
        let auth: CodexAuth = serde_json::from_slice(&raw)
            .map_err(|e| Error::Other(format!("decode credentials: {e}")))?;
        if auth.tokens.access_token.is_empty() {
            return Err(Error::unavailable("no OAuth access token"));
        }
        // The Codex CLI refreshes tokens on its own; refuse to launch it with a
        // stale token so a quota read can never trigger or rewrite a login.
        if let Some(exp) = jwt_expiry(&auth.tokens.access_token) {
            if ctx.now >= exp {
                return Err(Error::Expired);
            }
        }

        match rpc_rate_limits(ctx, &self.bin) {
            Ok(rl) => apply_rpc_limits(&mut snap, &rl),
            Err(rpc_err) => {
                let url = self.base() + "/wham/usage";
                let headers = bearer_headers(
                    &auth.tokens.access_token,
                    Some(&auth.tokens.account_id),
                    "protectorate",
                );
                match get_json::<WhamUsage>(http, ctx, &url, headers) {
                    Ok(u) => {
                        snap.source = "wham-usage".to_string();
                        snap.plan = u.plan_type.clone();
                        add_wham_windows(&mut snap, &u);
                    }
                    Err(e) => {
                        if e == Error::Unauthorized {
                            return Err(e);
                        }
                        return Err(Error::Other(format!(
                            "app-server: {rpc_err}; wham fallback: {e}"
                        )));
                    }
                }
            }
        }
        snap.fetched_at = Some(ctx.now);

        let url = self.base() + "/wham/rate-limit-reset-credits";
        let headers = bearer_headers(
            &auth.tokens.access_token,
            Some(&auth.tokens.account_id),
            "protectorate",
        );
        if let Ok(rc) = get_json::<WhamResetCredits>(http, ctx, &url, headers) {
            for cr in rc.credits {
                let exp = parse_time(&cr.expires_at);
                if cr.status != "available" {
                    continue;
                }
                if let Some(e) = exp {
                    if e <= ctx.now {
                        continue;
                    }
                }
                snap.resets.push(Reset {
                    kind: cr.reset_type,
                    count: 1,
                    expires_at: exp,
                });
            }
            sort_by_expiry(&mut snap.resets, |r| r.expires_at);
        }
        Ok(snap)
    }
}

fn apply_rpc_limits(snap: &mut Snapshot, rl: &RpcLimits) {
    snap.source = "codex-app-server".to_string();
    snap.plan = rl.rate_limits.plan_type.clone();
    add_rpc_windows(snap, "", &rl.rate_limits);
    let by = if rl.by_limit.is_empty() {
        &rl.by_limit2
    } else {
        &rl.by_limit
    };
    let mut keys: Vec<&String> = by
        .keys()
        .filter(|k| k.as_str() != "codex" && k.as_str() != rl.rate_limits.limit_id)
        .collect();
    keys.sort();
    for k in keys {
        let s = &by[k];
        let label = if s.limit_name.is_empty() {
            k.clone()
        } else {
            s.limit_name.clone()
        };
        add_rpc_windows(snap, &format!("{label} "), s);
    }
}

fn add_rpc_windows(snap: &mut Snapshot, prefix: &str, s: &RpcSnapshot) {
    for w in [&s.primary, &s.secondary] {
        let Some(w) = w else { continue };
        let mins = w.window_duration_mins.unwrap_or(0);
        snap.windows.push(Window {
            name: format!("{prefix}{}", window_label(mins)),
            used_percent: clamp_percent(w.used_percent),
            resets_at: w.resets_at.map(unix),
        });
    }
}

fn add_wham_windows(snap: &mut Snapshot, u: &WhamUsage) {
    let Some(rl) = u.rate_limit.as_ref() else {
        return;
    };
    for w in [&rl.primary, &rl.secondary] {
        let Some(w) = w else { continue };
        snap.windows.push(Window {
            name: window_label(w.limit_window_seconds / 60),
            used_percent: clamp_percent(w.used_percent),
            resets_at: Some(unix(w.reset_at)),
        });
    }
}

fn unix(secs: i64) -> DateTime<Utc> {
    Utc.timestamp_opt(secs, 0).single().unwrap_or_else(Utc::now)
}

/// `exp` from the JWT payload, if the token is a JWT with one.
pub fn jwt_expiry(token: &str) -> Option<DateTime<Utc>> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    let payload = b64url_decode(parts[1].trim_end_matches('='))?;
    #[derive(Deserialize)]
    struct Claims {
        #[serde(default)]
        exp: i64,
    }
    let claims: Claims = serde_json::from_slice(&payload).ok()?;
    if claims.exp == 0 {
        return None;
    }
    Some(unix(claims.exp))
}

fn b64url_decode(s: &str) -> Option<Vec<u8>> {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = Vec::new();
    let mut acc: u32 = 0;
    let mut bits: u32 = 0;
    for c in s.bytes() {
        if c == b'=' {
            break;
        }
        let idx = T.iter().position(|&x| x == c)?;
        acc = (acc << 6) | idx as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

#[derive(Deserialize, Default)]
struct CodexAuth {
    #[serde(default)]
    tokens: CodexTokens,
}

#[derive(Deserialize, Default)]
struct CodexTokens {
    #[serde(rename = "access_token", default)]
    access_token: String,
    #[serde(rename = "account_id", default)]
    account_id: String,
}

#[derive(Deserialize, Default, Clone)]
struct RpcWindow {
    #[serde(rename = "usedPercent", default)]
    used_percent: f64,
    #[serde(rename = "windowDurationMins", default)]
    window_duration_mins: Option<i64>,
    #[serde(rename = "resetsAt", default)]
    resets_at: Option<i64>,
}

#[derive(Deserialize, Default, Clone)]
struct RpcSnapshot {
    #[serde(rename = "limitId", default)]
    limit_id: String,
    #[serde(rename = "limitName", default)]
    limit_name: String,
    #[serde(default)]
    primary: Option<RpcWindow>,
    #[serde(default)]
    secondary: Option<RpcWindow>,
    #[serde(rename = "planType", default)]
    plan_type: String,
}

#[derive(Deserialize, Default)]
struct RpcLimits {
    #[serde(rename = "rateLimits", default)]
    rate_limits: RpcSnapshot,
    #[serde(rename = "rateLimitsByLimitId", default)]
    by_limit: HashMap<String, RpcSnapshot>,
    #[serde(rename = "rate_limits_by_limit_id", default)]
    by_limit2: HashMap<String, RpcSnapshot>,
}

#[derive(Deserialize, Default)]
struct WhamWindow {
    #[serde(rename = "used_percent", default)]
    used_percent: f64,
    #[serde(rename = "limit_window_seconds", default)]
    limit_window_seconds: i64,
    #[serde(rename = "reset_at", default)]
    reset_at: i64,
}

#[derive(Deserialize, Default)]
struct WhamRateLimit {
    #[serde(rename = "primary_window", default)]
    primary: Option<WhamWindow>,
    #[serde(rename = "secondary_window", default)]
    secondary: Option<WhamWindow>,
}

#[derive(Deserialize, Default)]
struct WhamUsage {
    #[serde(rename = "plan_type", default)]
    plan_type: String,
    #[serde(rename = "rate_limit", default)]
    rate_limit: Option<WhamRateLimit>,
}

#[derive(Deserialize, Default)]
struct WhamResetCredits {
    #[serde(default)]
    credits: Vec<WhamCredit>,
}

#[derive(Deserialize, Default)]
struct WhamCredit {
    #[serde(rename = "reset_type", default)]
    reset_type: String,
    #[serde(default)]
    status: String,
    #[serde(rename = "expires_at", default)]
    expires_at: String,
}

#[derive(Deserialize)]
struct RpcMessage {
    #[serde(default)]
    id: Option<i64>,
    #[serde(default)]
    result: Option<Value>,
    #[serde(default)]
    error: Option<RpcError>,
}

#[derive(Deserialize)]
struct RpcError {
    #[serde(default)]
    code: i64,
}

/// Drives `codex app-server` over newline-delimited JSON-RPC. stdin is closed
/// and the process (with its whole group) is reaped before returning.
struct AppServer {
    child: Option<ManagedChild>,
    stdin: Option<ChildStdin>,
    lines: Receiver<Vec<u8>>,
    stop: Arc<AtomicBool>,
    reader: Option<JoinHandle<()>>,
}

fn start_app_server(ctx: &Ctx, bin: &str) -> Result<AppServer> {
    let mut cmd = Command::new(bin);
    cmd.args(["-s", "read-only", "-a", "never", "app-server"]);
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut attempts = 0;
    let mut child = loop {
        if ctx.expired() {
            return Err(Error::Deadline);
        }
        match spawn(&mut cmd) {
            Ok(child) => break child,
            Err(error) if error.kind() == std::io::ErrorKind::ExecutableFileBusy => {
                attempts += 1;
                if attempts >= 8 {
                    return Err(Error::unavailable("cannot start codex"));
                }
                std::thread::sleep(
                    ctx.remaining()
                        .unwrap_or(Duration::from_millis(250))
                        .min(Duration::from_millis(250)),
                );
            }
            Err(_) => return Err(Error::unavailable("cannot start codex")),
        }
    };
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| Error::Other("cannot open codex stdin".to_string()))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| Error::Other("cannot open codex stdout".to_string()))?;

    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    let stop = Arc::new(AtomicBool::new(false));
    let reader_stop = stop.clone();
    let reader = std::thread::spawn(move || {
        let mut r = BufReader::new(stdout);
        let mut line: Vec<u8> = Vec::new();
        loop {
            line.clear();
            match r.read_until(b'\n', &mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
            if reader_stop.load(Ordering::Relaxed) {
                break;
            }
            let b: Vec<u8> = line.iter().copied().take(MAX_LINE).collect();
            if tx.send(b).is_err() {
                break;
            }
        }
    });
    Ok(AppServer {
        child: Some(child),
        stdin: Some(stdin),
        lines: rx,
        stop,
        reader: Some(reader),
    })
}

impl AppServer {
    fn call(&mut self, ctx: &Ctx, id: i64, method: &str, params: Option<Value>) -> Result<Value> {
        let mut msg = json!({"id": id, "method": method});
        if let Some(p) = params {
            msg.as_object_mut()
                .expect("object")
                .insert("params".to_string(), p);
        }
        let mut line = serde_json::to_vec(&msg).map_err(|e| Error::Other(e.to_string()))?;
        line.push(b'\n');
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| Error::Other("codex stdin closed".to_string()))?;
        stdin
            .write_all(&line)
            .map_err(|e| Error::Other(format!("write codex stdin: {e}")))?;
        stdin.flush().map_err(|e| Error::Other(e.to_string()))?;

        loop {
            if ctx.expired() {
                return Err(Error::Deadline);
            }
            match self.lines.recv_timeout(RPC_POLL) {
                Ok(line) => {
                    let m: RpcMessage = match serde_json::from_slice(&line) {
                        Ok(m) => m,
                        Err(_) => continue,
                    };
                    if m.id != Some(id) {
                        continue;
                    }
                    if let Some(e) = m.error {
                        return Err(Error::Other(format!(
                            "codex app-server error code {}",
                            e.code
                        )));
                    }
                    return Ok(m.result.unwrap_or(Value::Null));
                }
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(Error::Other("codex app-server exited early".to_string()))
                }
            }
        }
    }

    fn notify(&mut self, method: &str, params: Value) -> Result<()> {
        let msg = json!({"method": method, "params": params});
        let mut line = serde_json::to_vec(&msg).map_err(|e| Error::Other(e.to_string()))?;
        line.push(b'\n');
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| Error::Other("codex stdin closed".to_string()))?;
        stdin
            .write_all(&line)
            .map_err(|e| Error::Other(format!("write codex stdin: {e}")))?;
        stdin.flush().map_err(|e| Error::Other(e.to_string()))?;
        Ok(())
    }

    fn close(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(mut stdin) = self.stdin.take() {
            let _ = stdin.flush();
        }
        let mut exited = false;
        let deadline = std::time::Instant::now() + RPC_CLOSE_WAIT;
        while std::time::Instant::now() < deadline {
            match self.child.as_mut().expect("app-server child").try_wait() {
                Ok(Some(_)) | Err(_) => {
                    exited = true;
                    break;
                }
                Ok(None) => std::thread::sleep(RPC_POLL),
            }
        }
        if let Some(c) = self.child.as_mut() {
            kill_group(c);
            if !exited {
                let _ = c.wait();
            }
        }
        if let Some(h) = self.reader.take() {
            let _ = h.join();
        }
    }
}

fn rpc_rate_limits(ctx: &Ctx, bin: &str) -> Result<RpcLimits> {
    let bin = if bin.is_empty() { "codex" } else { bin };
    let mut p = start_app_server(ctx, bin)?;
    let res = (|| {
        p.call(
            ctx,
            1,
            "initialize",
            Some(json!({"clientInfo": {"name": "protectorate", "version": "0"}})),
        )?;
        p.notify("initialized", json!({}))?;
        let res = p.call(ctx, 2, "account/rateLimits/read", None)?;
        serde_json::from_value(res).map_err(|e| Error::Other(format!("decode rate limits: {e}")))
    })();
    p.close();
    res
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::{ctx, jwt, time, Fake};
    use crate::window_label;

    fn home(dir: &std::path::Path, token: &str) {
        std::fs::create_dir_all(dir.join(".codex")).unwrap();
        std::fs::write(
            dir.join(".codex").join("auth.json"),
            format!(r#"{{"tokens": {{"access_token": "{token}", "account_id": "acct-1"}}}}"#),
        )
        .unwrap();
    }

    #[test]
    fn jwt_expiry_reads_the_payload() {
        assert_eq!(
            jwt_expiry(&jwt(1_767_232_800)),
            Some(time("2026-01-01T02:00:00Z"))
        );
        assert!(jwt_expiry("not-a-jwt").is_none());
        assert!(jwt_expiry("a.b.c").is_none(), "no exp claim");
        assert!(jwt_expiry(&format!("{}.{}.sig", "e30", "e30")).is_none());
    }

    #[test]
    fn window_labels_match_the_provider_windows() {
        assert_eq!(window_label(300), "5h");
        assert_eq!(window_label(10080), "7d");
        assert_eq!(window_label(0), "window");
    }

    #[test]
    fn expired_tokens_never_launch_the_cli() {
        let dir = tempfile::tempdir().unwrap();
        home(dir.path(), &jwt(1_767_222_800));
        let c = Codex::new()
            .with_home(dir.path().to_path_buf())
            .with_bin("/nonexistent/codex");
        let err = c
            .read(&ctx("2026-01-01T00:00:00Z"), &Fake::new())
            .unwrap_err();
        assert_eq!(err, Error::Expired);
    }

    #[test]
    fn missing_credentials_are_unavailable() {
        let dir = tempfile::tempdir().unwrap();
        let c = Codex::new().with_home(dir.path().to_path_buf());
        let err = c
            .read(&ctx("2026-01-01T00:00:00Z"), &Fake::new())
            .unwrap_err();
        assert_eq!(
            err.to_string(),
            "source unavailable: credentials file not found"
        );
    }

    #[test]
    fn wham_fallback_reports_windows_and_credits() {
        let dir = tempfile::tempdir().unwrap();
        home(dir.path(), &jwt(1_767_232_800));
        let http = Fake::new()
            .route(
                "GET",
                "https://chatgpt.com/backend-api/wham/usage",
                200,
                r#"{"plan_type":"pro","rate_limit":{
                     "primary_window": {"used_percent": 25, "limit_window_seconds": 18000, "reset_at": 1767232800},
                     "secondary_window": {"used_percent": 80, "limit_window_seconds": 604800, "reset_at": 1767542400}}}"#,
            )
            .route(
                "GET",
                "https://chatgpt.com/backend-api/wham/rate-limit-reset-credits",
                200,
                r#"{"credits":[
                    {"reset_type":"weekly","status":"available","expires_at":"2026-01-05T00:00:00Z"},
                    {"reset_type":"redeemed","status":"used","expires_at":"2026-01-06T00:00:00Z"},
                    {"reset_type":"past","status":"available","expires_at":"2025-12-01T00:00:00Z"},
                    {"reset_type":"no-expiry","status":"available"}]}"#,
            );
        let c = Codex::new()
            .with_home(dir.path().to_path_buf())
            .with_bin("/nonexistent/codex");
        let snap = c.read(&ctx("2026-01-01T00:00:00Z"), &http).unwrap();

        assert_eq!(snap.source, "wham-usage");
        assert_eq!(snap.plan, "pro");
        let names: Vec<&str> = snap.windows.iter().map(|w| w.name.as_str()).collect();
        assert_eq!(names, ["5h", "7d"]);
        assert_eq!(snap.windows[0].used_percent, 25.0);
        assert_eq!(
            snap.windows[0].resets_at,
            Some(time("2026-01-01T02:00:00Z"))
        );
        let kinds: Vec<&str> = snap.resets.iter().map(|r| r.kind.as_str()).collect();
        assert_eq!(
            kinds,
            ["weekly", "no-expiry"],
            "redeemed and expired credits drop out"
        );

        let reqs = http.requests();
        assert!(reqs[0]
            .2
            .contains(&("ChatGPT-Account-Id".to_string(), "acct-1".to_string())));
    }

    #[test]
    fn both_sources_failing_reports_both_errors() {
        let dir = tempfile::tempdir().unwrap();
        home(dir.path(), &jwt(1_767_232_800));
        let http = Fake::new().route(
            "GET",
            "https://chatgpt.com/backend-api/wham/usage",
            401,
            "{}",
        );
        let c = Codex::new()
            .with_home(dir.path().to_path_buf())
            .with_bin("/nonexistent/codex");
        let err = c.read(&ctx("2026-01-01T00:00:00Z"), &http).unwrap_err();
        assert_eq!(
            err,
            Error::Unauthorized,
            "a rejected token is reported as itself"
        );

        let http = Fake::new().route(
            "GET",
            "https://chatgpt.com/backend-api/wham/usage",
            500,
            "boom",
        );
        let err = c.read(&ctx("2026-01-01T00:00:00Z"), &http).unwrap_err();
        assert!(err.to_string().starts_with("app-server: source unavailable: cannot start codex; wham fallback: unexpected HTTP status 500"));
    }

    #[test]
    fn rpc_limit_labels_are_prefixed_and_sorted() {
        let mut rl = RpcLimits::default();
        rl.rate_limits.limit_id = "codex".to_string();
        rl.rate_limits.plan_type = "plus".to_string();
        rl.rate_limits.primary = Some(RpcWindow {
            used_percent: 30.0,
            window_duration_mins: Some(300),
            resets_at: Some(1_767_232_800),
        });
        rl.by_limit.insert(
            "codex".to_string(),
            RpcSnapshot {
                primary: Some(RpcWindow {
                    used_percent: 99.0,
                    window_duration_mins: Some(300),
                    resets_at: None,
                }),
                ..RpcSnapshot::default()
            },
        );
        rl.by_limit.insert(
            "z-image".to_string(),
            RpcSnapshot {
                limit_name: "Image".to_string(),
                primary: Some(RpcWindow {
                    used_percent: 5.0,
                    window_duration_mins: Some(1440),
                    resets_at: None,
                }),
                ..RpcSnapshot::default()
            },
        );
        rl.by_limit.insert(
            "a-auto".to_string(),
            RpcSnapshot {
                primary: Some(RpcWindow {
                    used_percent: 7.0,
                    window_duration_mins: Some(60),
                    resets_at: None,
                }),
                ..RpcSnapshot::default()
            },
        );
        let mut snap = Snapshot {
            provider: "codex".to_string(),
            ..Snapshot::default()
        };
        apply_rpc_limits(&mut snap, &rl);
        assert_eq!(snap.source, "codex-app-server");
        assert_eq!(snap.plan, "plus");
        let names: Vec<&str> = snap.windows.iter().map(|w| w.name.as_str()).collect();
        assert_eq!(names, ["5h", "a-auto 1h", "Image 1d"]);
        assert_eq!(
            snap.windows[0].resets_at,
            Some(time("2026-01-01T02:00:00Z"))
        );
    }

    #[test]
    fn snake_case_limit_maps_are_accepted() {
        let mut rl = RpcLimits::default();
        rl.rate_limits.limit_id = "codex".to_string();
        rl.by_limit2.insert(
            "other".to_string(),
            RpcSnapshot {
                primary: Some(RpcWindow {
                    used_percent: 1.0,
                    window_duration_mins: Some(300),
                    resets_at: None,
                }),
                ..RpcSnapshot::default()
            },
        );
        let mut snap = Snapshot::default();
        apply_rpc_limits(&mut snap, &rl);
        assert_eq!(snap.windows.len(), 1);
        assert_eq!(snap.windows[0].name, "other 5h");
    }

    #[cfg(unix)]
    #[test]
    fn app_server_rpc_is_the_primary_source() {
        let dir = tempfile::tempdir().unwrap();
        home(dir.path(), &jwt(1_767_232_800));
        let script = dir.path().join("fake-codex");
        std::fs::write(
            &script,
            r#"#!/bin/sh
# a CLI that answers the quota method
while IFS= read -r line; do
  case "$line" in
    *'"method":"initialize"'*)
      echo '{"id":1,"result":{"ok":true}}'
      ;;
    *'"method":"account/rateLimits/read"'*)
      echo '{"id":2,"result":{"rateLimits":{"limitId":"codex","planType":"pro","primary":{"usedPercent":30,"windowDurationMins":300,"resetsAt":1767232800},"secondary":{"usedPercent":12,"windowDurationMins":10080,"resetsAt":1767542400}},"rateLimitsByLimitId":{"codex":{"primary":{"usedPercent":99}},"image":{"limitName":"Image","primary":{"usedPercent":5,"windowDurationMins":1440}}}}}'
      ;;
  esac
done
"#,
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();

        let http = Fake::new().route(
            "GET",
            "https://chatgpt.com/backend-api/wham/rate-limit-reset-credits",
            200,
            r#"{"credits":[{"reset_type":"weekly","status":"available","expires_at":"2026-01-05T00:00:00Z"}]}"#,
        );
        let c = Codex::new()
            .with_home(dir.path().to_path_buf())
            .with_bin(script.to_str().unwrap());
        let snap = c.read(&ctx("2026-01-01T00:00:00Z"), &http).unwrap();

        assert_eq!(snap.source, "codex-app-server");
        assert_eq!(snap.plan, "pro");
        let names: Vec<&str> = snap.windows.iter().map(|w| w.name.as_str()).collect();
        assert_eq!(names, ["5h", "7d", "Image 1d"]);
        assert_eq!(snap.resets.len(), 1);
        // Only the credits endpoint was contacted; the usage endpoint was not.
        assert_eq!(http.requests().len(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn rpc_errors_are_reported_when_the_cli_refuses() {
        let dir = tempfile::tempdir().unwrap();
        home(dir.path(), &jwt(1_767_232_800));
        let script = dir.path().join("refusing-codex");
        std::fs::write(
            &script,
            r#"#!/bin/sh
# a CLI that refuses the rate limits method
while IFS= read -r line; do
  case "$line" in
    *'"method":"initialize"'*) echo '{"id":1,"result":{"ok":true}}' ;;
    *'"method":"account/rateLimits/read"'*) echo '{"id":2,"error":{"code":-32601}}' ;;
  esac
done
"#,
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();

        let http = Fake::new().route(
            "GET",
            "https://chatgpt.com/backend-api/wham/usage",
            200,
            r#"{"plan_type":"free","rate_limit":{"primary_window":{"used_percent":1,"limit_window_seconds":18000,"reset_at":1767232800}}}"#,
        );
        let c = Codex::new()
            .with_home(dir.path().to_path_buf())
            .with_bin(script.to_str().unwrap());
        let snap = c.read(&ctx("2026-01-01T00:00:00Z"), &http).unwrap();
        assert_eq!(snap.source, "wham-usage");
        assert_eq!(snap.plan, "free");
    }

    #[cfg(unix)]
    #[test]
    fn unresponsive_app_server_obeys_the_wall_deadline() {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("silent-codex");
        std::fs::write(&script, "#!/bin/sh\nwhile IFS= read -r line; do :; done\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let bound = ctx("2026-01-01T00:00:00Z").bounded(Duration::from_millis(30));
        let start = std::time::Instant::now();
        assert!(matches!(
            rpc_rate_limits(&bound, script.to_str().unwrap()),
            Err(Error::Deadline)
        ));
        assert!(start.elapsed() < Duration::from_secs(2));
    }
}
