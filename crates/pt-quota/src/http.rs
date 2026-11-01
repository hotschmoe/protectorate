//! Outbound HTTP through the system `curl` (SPEC 15). The URL and every header, which may hold a
//! bearer token, go to curl as a config on stdin, never on the command line where `ps` shows them.

use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::time::Duration;

use crate::error::{Error, Result};

/// Bodies are never read past this many bytes.
pub const MAX_BODY: usize = 1 << 20;

pub struct Get<'a> {
    pub url: &'a str,
    pub headers: &'a [(&'a str, String)],
    pub timeout: Duration,
}

/// GET `url` and return the body of a 2xx. 401 and scope-related 403 get their own errors.
pub fn get(req: &Get) -> Result<Vec<u8>> {
    let (status, body) = send(req)?;
    match status {
        200..=299 => Ok(body),
        401 => Err(Error::Unauthorized),
        403 => {
            let lower = String::from_utf8_lossy(&body).to_lowercase();
            if lower.contains("scope") || lower.contains("permission") {
                Err(Error::Scope)
            } else {
                Err(Error::Http(403))
            }
        }
        s => Err(Error::Http(s)),
    }
}

fn config_quote(s: &str) -> Result<String> {
    if s.chars().any(|c| c.is_control()) {
        return Err(Error::other("request contains a control character"));
    }
    Ok(format!(
        "\"{}\"",
        s.replace('\\', "\\\\").replace('"', "\\\"")
    ))
}

fn send(req: &Get) -> Result<(u16, Vec<u8>)> {
    let local = req.url.starts_with("http://");
    if !local && !req.url.starts_with("https://") {
        return Err(Error::other("request URL is not http or https"));
    }
    let mut config = format!("url = {}\n", config_quote(req.url)?);
    for (name, value) in req.headers {
        config += &format!("header = {}\n", config_quote(&format!("{name}: {value}"))?);
    }
    let secs = req.timeout.as_secs().max(1);
    let mut cmd = Command::new("curl");
    cmd.args(["-q", "-sS", "--config", "-", "--proto", "=http,https"])
        .args(["--max-time", &secs.to_string()])
        .args(["--connect-timeout", &secs.min(10).to_string()])
        .args(["--max-filesize", &MAX_BODY.to_string()])
        .args(["-w", "\n%{http_code}"]);
    if local {
        cmd.args(["--noproxy", "*"]);
    }
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd
        .spawn()
        .map_err(|e| Error::unavailable(format!("cannot start curl: {e}")))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| Error::other("curl stdin closed"))?;
    stdin
        .write_all(config.as_bytes())
        .map_err(|e| Error::other(format!("write curl config: {e}")))?;
    drop(stdin);
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| Error::other("curl stdout closed"))?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| Error::other("curl stderr closed"))?;
    let err_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stderr.by_ref().take(4096).read_to_end(&mut buf);
        buf
    });
    let mut out = Vec::new();
    stdout
        .by_ref()
        .take(MAX_BODY as u64 + 16)
        .read_to_end(&mut out)
        .map_err(|e| Error::other(format!("read curl output: {e}")))?;
    drop(stdout);
    let exit = child
        .wait()
        .map_err(|e| Error::other(format!("wait for curl: {e}")))?;
    let err_text = String::from_utf8_lossy(&err_reader.join().unwrap_or_default()).into_owned();
    if !exit.success() {
        return Err(match exit.code() {
            Some(28) => Error::Timeout,
            Some(code) => Error::other(format!(
                "curl exit {code}: {}",
                err_text.lines().next().unwrap_or("").trim()
            )),
            None => Error::other("curl was killed by a signal"),
        });
    }
    let cut = out
        .iter()
        .rposition(|&b| b == b'\n')
        .ok_or_else(|| Error::other("curl output has no status line"))?;
    let status = std::str::from_utf8(&out[cut + 1..])
        .ok()
        .and_then(|s| s.trim().parse::<u16>().ok())
        .ok_or_else(|| Error::other("curl output has an unreadable status"))?;
    out.truncate(cut.min(MAX_BODY));
    Ok((status, out))
}
