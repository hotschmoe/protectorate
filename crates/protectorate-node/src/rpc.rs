//! `protectorate-node rpc`: a stand-in hub client for tests and manual checks. It writes the
//! header line member would write (a `tag:hub` peer unless `--header` says otherwise), sends one
//! request frame and prints the answer: a response or error as JSON on stdout, or for FetchLog
//! the raw log bytes on stdout and the end frame as JSON on stderr.

use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use pt_frame::Kind;
use pt_frame::msg::{self, Method, Request};
use pt_frame::wire::{FrameReader, write_frame};
use pt_json::{Map, Value};

pub const USAGE: &str =
    "  rpc --connect SOCKET|HOST:PORT [--header JSON] [--timeout SECS] [--raw] METHOD [FIELDS-JSON]
                                    send one request as the hub would (exit 1 on an error answer,
                                    3 when the connection fails or closes unanswered); --raw sends
                                    the fields unchecked, as a hostile hub might";

const DEFAULT_HEADER: &str = r#"{"peer_name":"rpc","tags":["tag:hub"],"ip":"127.0.0.1"}"#;

pub fn run(args: &[String]) -> i32 {
    match call(args) {
        Ok(code) => code,
        Err(Failure::Usage(e)) => {
            eprintln!("protectorate-node rpc: {e}");
            2
        }
        Err(Failure::Link(e)) => {
            eprintln!("protectorate-node rpc: {e}");
            3
        }
    }
}

enum Failure {
    Usage(String),
    Link(String),
}

fn call(args: &[String]) -> Result<i32, Failure> {
    let mut connect = None;
    let mut header = DEFAULT_HEADER.to_string();
    let mut timeout = 120u64;
    let mut raw = false;
    let mut rest = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = || {
            it.next()
                .cloned()
                .ok_or_else(|| Failure::Usage(format!("{a} needs a value")))
        };
        match a.as_str() {
            "--connect" => connect = Some(value()?),
            "--raw" => raw = true,
            "--header" => header = value()?,
            "--timeout" => {
                let v = value()?;
                timeout = v
                    .parse()
                    .map_err(|_| Failure::Usage(format!("--timeout: '{v}' is not a number")))?;
            }
            _ => rest.push(a.clone()),
        }
    }
    let connect = connect.ok_or_else(|| Failure::Usage("rpc needs --connect".into()))?;
    let (method, fields) = match rest.as_slice() {
        [m] => (m.as_str(), "{}"),
        [m, f] => (m.as_str(), f.as_str()),
        _ => {
            return Err(Failure::Usage(format!(
                "rpc needs METHOD [FIELDS-JSON]\n{USAGE}"
            )));
        }
    };
    let fields = pt_json::parse(fields).map_err(|e| Failure::Usage(format!("fields: {e}")))?;
    let payload = if raw {
        let mut m = Map::new();
        m.insert("type", method);
        for (k, v) in fields.as_object().into_iter().flatten() {
            m.insert(k.clone(), v.clone());
        }
        msg::payload(&Value::Object(m))
    } else {
        let method: Method = method.parse().map_err(|e| Failure::Usage(format!("{e}")))?;
        let req = Request::from_fields(method, &fields)
            .map_err(|e| Failure::Usage(format!("{method} fields: {e}")))?;
        msg::payload(&req)
    };

    let link = |e: io::Error| Failure::Link(format!("{connect}: {e}"));
    let timeout = Some(Duration::from_secs(timeout.max(1)));
    let (mut r, mut w): (Box<dyn Read>, Box<dyn Write>) = match connect.parse::<SocketAddr>() {
        Ok(addr) => {
            let s = TcpStream::connect(addr).map_err(link)?;
            s.set_read_timeout(timeout).map_err(link)?;
            (Box::new(s.try_clone().map_err(link)?), Box::new(s))
        }
        Err(_) => unix(&connect, timeout).map_err(link)?,
    };
    w.write_all(format!("{header}\n").as_bytes())
        .map_err(link)?;
    write_frame(&mut w, Kind::Request, 1, &payload).map_err(link)?;
    w.flush().map_err(link)?;

    let mut frames = FrameReader::new(&mut r, &[]);
    let mut out = io::stdout().lock();
    loop {
        let f = match frames.read() {
            Ok(Some(f)) => f,
            Ok(None) => return Err(Failure::Link("closed without an answer".into())),
            Err(e) => return Err(link(e)),
        };
        if f.id != 1 {
            return Err(Failure::Link(format!("answer for request {}, not 1", f.id)));
        }
        match f.kind {
            Kind::Response | Kind::Error => {
                out.write_all(&f.payload)
                    .and_then(|()| out.write_all(b"\n"))
                    .and_then(|()| out.flush())
                    .map_err(|e| Failure::Link(format!("stdout: {e}")))?;
                return Ok(if f.kind == Kind::Error { 1 } else { 0 });
            }
            Kind::Chunk => out
                .write_all(&f.payload)
                .map_err(|e| Failure::Link(format!("stdout: {e}")))?,
            Kind::End => {
                out.flush()
                    .map_err(|e| Failure::Link(format!("stdout: {e}")))?;
                eprintln!("{}", String::from_utf8_lossy(&f.payload));
                return Ok(0);
            }
            Kind::Request => return Err(Failure::Link("a request frame from the node".into())),
        }
    }
}

type Pair = (Box<dyn Read>, Box<dyn Write>);

#[cfg(unix)]
fn unix(path: &str, timeout: Option<Duration>) -> io::Result<Pair> {
    let s = std::os::unix::net::UnixStream::connect(path)?;
    s.set_read_timeout(timeout)?;
    Ok((Box::new(s.try_clone()?), Box::new(s)))
}

#[cfg(not(unix))]
fn unix(path: &str, _timeout: Option<Duration>) -> io::Result<Pair> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        format!("'{path}' is not HOST:PORT, and unix sockets are not supported here"),
    ))
}
