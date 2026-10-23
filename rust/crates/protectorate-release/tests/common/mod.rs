#![allow(dead_code)]

use flate2::{write::GzEncoder, Compression};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

pub type Reply = (u16, Vec<(String, String)>, Vec<u8>);
pub type Handler = Arc<dyn Fn(&str, &str, &[(String, String)]) -> Reply + Send + Sync>;
type Log = Arc<Mutex<Vec<(String, Vec<(String, String)>)>>>;

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|c| format!("{c:02x}")).collect()
}

pub fn sha(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

pub fn sums_for(entries: &[(&str, &[u8])]) -> String {
    entries
        .iter()
        .map(|(name, bytes)| format!("{}  {name}", sha(bytes)))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

/// A ustar block for one entry. `name` must fit the 100 byte name field.
pub fn tar_header(name: &str, kind: u8, size: u64) -> [u8; 512] {
    let mut block = [0u8; 512];
    let name_bytes = name.as_bytes();
    assert!(name_bytes.len() <= 100, "tar name {name:?} is too long");
    block[0..name_bytes.len()].copy_from_slice(name_bytes);
    write_octal(&mut block[100..108], 0o644);
    write_octal(&mut block[124..136], size);
    write_octal(&mut block[136..148], 1_700_000_000);
    block[148..156].copy_from_slice(b"        ");
    block[156] = kind;
    block[257..263].copy_from_slice(b"ustar\0");
    block[263..265].copy_from_slice(b"00");
    let sum: u32 = block.iter().map(|b| *b as u32).sum();
    let checksum = format!("{sum:07o}\0");
    block[148..156].copy_from_slice(checksum.as_bytes());
    block
}

fn write_octal(field: &mut [u8], value: u64) {
    let text = format!("{value:0o}\0");
    field[..text.len()].copy_from_slice(text.as_bytes());
}

/// An uncompressed tar archive: entries, then the two zero blocks.
pub fn tar(entries: &[(&str, u8, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    for (name, kind, data) in entries {
        out.extend_from_slice(&tar_header(name, *kind, data.len() as u64));
        out.extend_from_slice(data);
        let pad = (512 - data.len() % 512) % 512;
        out.extend(std::iter::repeat_n(0u8, pad));
    }
    out.extend(std::iter::repeat_n(0u8, 1024));
    out
}

pub fn gzip(data: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data).unwrap();
    encoder.finish().unwrap()
}

pub fn tar_gz(entries: &[(&str, u8, &[u8])]) -> Vec<u8> {
    gzip(&tar(entries))
}

/// A blocking HTTP/1.1 server good enough for the release client: one reply
/// per request, always closed so ureq never reuses a stale connection.
pub struct Server {
    pub base: String,
    requests: Log,
    stop: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl Server {
    pub fn start(
        handler: impl Fn(&str, &str, &[(String, String)]) -> Reply + Send + Sync + 'static,
    ) -> Server {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let handler: Handler = Arc::new(handler);
        let stop2 = stop.clone();
        let requests2 = requests.clone();
        let base = format!("http://127.0.0.1:{port}");
        let listening = base.clone();
        let handle = thread::spawn(move || loop {
            if stop2.load(Ordering::Relaxed) {
                break;
            }
            match listener.accept() {
                Ok((stream, _)) => {
                    let handler = handler.clone();
                    let requests = requests2.clone();
                    let base = listening.clone();
                    thread::spawn(move || {
                        let _ = serve(stream, &base, &handler, &requests);
                    });
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(3));
                }
                Err(_) => break,
            }
        });
        Server {
            base,
            requests,
            stop,
            handle: Some(handle),
        }
    }

    pub fn paths(&self) -> Vec<String> {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .map(|(path, _)| path.clone())
            .collect()
    }

    pub fn headers_for(&self, path: &str) -> Vec<Vec<(String, String)>> {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .filter(|(p, _)| p == path)
            .map(|(_, headers)| headers.clone())
            .collect()
    }

    pub fn saw(&self, path: &str) -> bool {
        self.requests.lock().unwrap().iter().any(|(p, _)| p == path)
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn serve(stream: TcpStream, base: &str, handler: &Handler, requests: &Log) -> std::io::Result<()> {
    let stream = stream;
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    if reader.read_line(&mut line)? == 0 {
        return Ok(());
    }
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let path = parts.next().unwrap_or("").to_string();
    let mut headers = Vec::new();
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header)? == 0 {
            break;
        }
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            headers.push((name.trim().to_lowercase(), value.trim().to_string()));
        }
    }
    requests
        .lock()
        .unwrap()
        .push((path.clone(), headers.clone()));
    let reply = if method == "GET" {
        handler(base, &path, &headers)
    } else {
        (405, Vec::new(), Vec::new())
    };
    let (status, reply_headers, body) = reply;
    let mut out = format!("HTTP/1.1 {status} OK\r\n");
    for (name, value) in &reply_headers {
        out.push_str(&format!("{name}: {value}\r\n"));
    }
    out.push_str(&format!(
        "Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    ));
    let mut stream = reader.into_inner();
    stream.write_all(out.as_bytes())?;
    stream.write_all(&body)?;
    stream.flush()?;
    let _ = stream.shutdown(std::net::Shutdown::Both);
    Ok(())
}

/// The release document plus every asset body, keyed by request path.
/// A `/redirect/...` path replies 302 with the same path minus the prefix.
pub struct Release {
    pub bodies: BTreeMap<String, Vec<u8>>,
    pub release_paths: Vec<String>,
    pub etag: String,
    pub tag: String,
    pub prerelease: bool,
    pub draft: bool,
    pub names: Vec<String>,
    pub urls: BTreeMap<String, String>,
    /// Extra bytes padded into the release document name field.
    pub pad: usize,
}

impl Release {
    pub fn new(tag: &str) -> Release {
        Release {
            bodies: BTreeMap::new(),
            release_paths: vec![
                "/repos/owner/repo/releases/latest".to_string(),
                "/repos/owner/repo/releases/tags/1.2.3".to_string(),
            ],
            etag: "W/\"etag1\"".to_string(),
            tag: tag.to_string(),
            prerelease: false,
            draft: false,
            names: Vec::new(),
            urls: BTreeMap::new(),
            pad: 0,
        }
    }

    /// Register an asset and return the path the client will be told to fetch.
    pub fn asset(&mut self, name: &str, body: &[u8], path: &str) {
        self.names.push(name.to_string());
        self.bodies.insert(path.to_string(), body.to_vec());
        self.urls.insert(name.to_string(), path.to_string());
    }

    pub fn serve(self) -> Server {
        let release = Arc::new(self);
        Server::start(move |base, path, headers| {
            if release.release_paths.iter().any(|p| p == path) {
                if headers
                    .iter()
                    .any(|(k, v)| k == "if-none-match" && v == &release.etag)
                {
                    return (304, Vec::new(), Vec::new());
                }
                let names: Vec<&str> = release.names.iter().map(|n| n.as_str()).collect();
                let body = release_json(
                    base,
                    &release.tag,
                    release.prerelease,
                    release.draft,
                    &names,
                    &release.urls,
                    release.pad,
                );
                return (
                    200,
                    vec![
                        ("Content-Type".to_string(), "application/json".to_string()),
                        ("ETag".to_string(), release.etag.clone()),
                    ],
                    body.into_bytes(),
                );
            }
            if path == "/loop" {
                return (
                    302,
                    vec![("Location".to_string(), "/loop".to_string())],
                    Vec::new(),
                );
            }
            if let Some(target) = path.strip_prefix("/redirect") {
                return (
                    302,
                    vec![("Location".to_string(), target.to_string())],
                    Vec::new(),
                );
            }
            match release.bodies.get(path) {
                Some(body) => (
                    200,
                    vec![(
                        "Content-Type".to_string(),
                        "application/octet-stream".to_string(),
                    )],
                    body.clone(),
                ),
                None => (404, Vec::new(), Vec::new()),
            }
        })
    }
}

/// The GitHub-shaped release document the client expects. `urls` maps an asset
/// name to the path under the server; a name absent from `urls` gets the plain
/// `/assets/<name>` path.
pub fn release_json(
    base: &str,
    tag: &str,
    prerelease: bool,
    draft: bool,
    names: &[&str],
    urls: &BTreeMap<String, String>,
    pad: usize,
) -> String {
    let assets: Vec<serde_json::Value> = names
        .iter()
        .map(|name| {
            let path = urls
                .get(*name)
                .cloned()
                .unwrap_or_else(|| format!("/assets/{name}"));
            serde_json::json!({
                "name": name,
                "browser_download_url": format!("{base}{path}"),
            })
        })
        .collect();
    serde_json::json!({
        "tag_name": tag,
        "name": format!("{tag}{}", "x".repeat(pad)),
        "html_url": format!("{base}/releases/tag/{tag}"),
        "prerelease": prerelease,
        "draft": draft,
        "assets": assets,
    })
    .to_string()
}
