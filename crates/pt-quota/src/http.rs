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

/// Whole seconds stay an integer. A shorter remainder is fractional, so a follow-up
/// request can be given only the time left in the read.
fn timeout_arg(timeout: Duration) -> String {
    if timeout.subsec_nanos() == 0 && timeout.as_secs() >= 1 {
        timeout.as_secs().to_string()
    } else {
        format!("{:.3}", timeout.as_secs_f64().max(0.001))
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
    let mut localhost_port = None;
    if local {
        let authority = req.url[7..].split(['/', '?', '#']).next().unwrap_or("");
        if authority.contains(['@', '\\', '%']) || authority.chars().any(char::is_whitespace) {
            return Err(Error::other(
                "misconfigured endpoint: HTTP is allowed only for loopback",
            ));
        }
        let (host, port) = if let Some(rest) = authority.strip_prefix('[') {
            let (host, rest) = rest
                .split_once(']')
                .ok_or_else(|| Error::other("invalid HTTP authority"))?;
            (host, rest)
        } else {
            authority
                .find(':')
                .map_or((authority, ""), |at| (&authority[..at], &authority[at..]))
        };
        let port = if port.is_empty() {
            80
        } else {
            port.strip_prefix(':')
                .and_then(|p| p.parse::<u16>().ok())
                .filter(|p| *p != 0)
                .ok_or_else(|| Error::other("invalid HTTP port"))?
        };
        let loopback_name = host.eq_ignore_ascii_case("localhost");
        let loopback_ip = host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback());
        if !loopback_name && !loopback_ip {
            return Err(Error::other(
                "misconfigured endpoint: HTTP is allowed only for loopback",
            ));
        }
        if loopback_name {
            localhost_port = Some(port);
        }
    }
    let mut config = format!("url = {}\n", config_quote(req.url)?);
    for (name, value) in req.headers {
        config += &format!("header = {}\n", config_quote(&format!("{name}: {value}"))?);
    }
    let max_time = timeout_arg(req.timeout);
    let connect = timeout_arg(req.timeout.min(Duration::from_secs(10)));
    let mut cmd = Command::new("curl");
    cmd.args(["-q", "-sS", "--config", "-", "--proto", "=http,https"])
        .args(["--max-time", &max_time])
        .args(["--connect-timeout", &connect])
        .args(["--max-filesize", &MAX_BODY.to_string()])
        .args(["-w", "\n%{http_code}"]);
    if local {
        cmd.args(["--noproxy", "*"]);
    }
    if let Some(port) = localhost_port {
        cmd.args(["--resolve", &format!("localhost:{port}:127.0.0.1")]);
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
