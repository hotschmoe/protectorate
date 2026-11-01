//! A test server with a fixed set of routes, temp paths and a curl runner.
#![allow(dead_code)]

use pt_http::{Config, Message, Peer, Request, Response, Server, ShutdownHandle};
use std::io::Read;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

pub struct Running {
    pub addr: Option<SocketAddr>,
    pub handle: ShutdownHandle,
    pub thread: JoinHandle<std::io::Result<()>>,
}

impl Running {
    pub fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.addr.unwrap())
    }

    pub fn stop(self) {
        self.handle.shutdown(Duration::from_secs(2));
        self.thread.join().unwrap().unwrap();
    }
}

/// A fresh directory under the cargo test tmpdir.
pub fn temp_dir(name: &str) -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let n = N.fetch_add(1, Ordering::SeqCst);
    let d = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("pt-http-{}-{n}-{name}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

pub fn route(static_dir: &Path, req: &Request, peer: &Peer) -> Response {
    match (req.method(), req.path()) {
        ("GET", "/hello") => Response::text(200, "hello\n"),
        ("GET", "/query") => Response::text(200, req.query_param("name").unwrap_or_default()),
        ("GET", "/peer") => Response::text(
            200,
            match peer {
                Peer::Tcp(a) => format!("tcp {}", a.port()),
                Peer::Unix(c) => format!("unix {} {}", c.uid, c.pid),
            },
        ),
        ("POST", "/echo") => {
            let len = req.body().len().to_string();
            Response::bytes(200, "application/octet-stream", req.body().to_vec())
                .header("X-Len", &len)
        }
        ("GET", "/stream") => Response::stream(200, "text/plain", |w| {
            for part in ["one ", "two ", "three\n"] {
                w.write_all(part.as_bytes())?;
            }
            Ok(())
        }),
        ("GET", "/panic") => panic!("test handler panics on purpose"),
        ("GET", "/slow") => {
            thread::sleep(Duration::from_millis(400));
            Response::text(200, "slow done\n")
        }
        ("GET", "/ws") => Response::websocket(|mut ws| {
            loop {
                match ws.recv() {
                    Ok(Message::Text(t)) if t == "push" => {
                        let w = ws.writer();
                        thread::spawn(move || {
                            (0..3).for_each(|i| w.send_text(&format!("pushed {i}")).unwrap())
                        })
                        .join()
                        .unwrap();
                    }
                    Ok(Message::Text(t)) => ws.send_text(&t).unwrap(),
                    Ok(Message::Binary(b)) => ws.send_binary(&b).unwrap(),
                    Ok(Message::Pong(_)) => {}
                    Ok(Message::Close(..)) | Err(_) => return,
                }
            }
        }),
        (m, p) if p.starts_with("/static/") => {
            if m != "GET" && m != "HEAD" {
                return Response::method_not_allowed("GET, HEAD");
            }
            let name = &p["/static/".len()..];
            if name.contains('/') || name.starts_with('.') {
                return Response::not_found();
            }
            Response::file(req, &static_dir.join(name)).unwrap_or_else(|_| Response::not_found())
        }
        (_, "/hello" | "/echo") => Response::method_not_allowed("GET, POST"),
        _ => Response::not_found(),
    }
}

pub fn start(config: Config) -> Running {
    let dir = temp_dir("static");
    std::fs::write(dir.join("index.html"), b"<!doctype html><title>t</title>\n").unwrap();
    std::fs::write(dir.join("app.wasm"), vec![7u8; 100_000]).unwrap();
    let server = Server::bind_tcp(
        "127.0.0.1:0".parse().unwrap(),
        config,
        move |req: &Request, peer: &Peer| route(&dir, req, peer),
    )
    .unwrap();
    let addr = server.local_addr();
    let handle = server.shutdown_handle();
    Running {
        addr,
        handle,
        thread: thread::spawn(move || server.run()),
    }
}

pub fn start_unix(config: Config, path: &Path) -> Running {
    let dir = temp_dir("static-unix");
    let server = Server::bind_unix(path, config, move |req: &Request, peer: &Peer| {
        route(&dir, req, peer)
    })
    .unwrap();
    let handle = server.shutdown_handle();
    Running {
        addr: None,
        handle,
        thread: thread::spawn(move || server.run()),
    }
}

/// A hostile server for our client: accepts one connection, reads the request head and hands the
/// socket and that head to `f` on its own thread.
pub fn fake_server(f: impl FnOnce(TcpStream, String) + Send + 'static) -> SocketAddr {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();
    thread::spawn(move || {
        let (mut c, _) = l.accept().unwrap();
        let head = read_head(&mut c);
        f(c, head);
    });
    addr
}

/// Reads a message head byte by byte, so nothing after it is consumed.
pub fn read_head(c: &mut TcpStream) -> String {
    let mut head = Vec::new();
    let mut b = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        c.read_exact(&mut b).unwrap();
        head.push(b[0]);
    }
    String::from_utf8(head).unwrap()
}

/// True once the peer has closed `c`, by EOF or reset, within its read timeout.
pub fn closed_by_peer(c: &mut TcpStream) -> bool {
    let mut buf = [0u8; 4096];
    loop {
        match c.read(&mut buf) {
            Ok(0) => return true,
            Ok(_) => {}
            Err(e) => return e.kind() == std::io::ErrorKind::ConnectionReset,
        }
    }
}

pub struct Curl {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

pub fn curl(args: &[&str]) -> Curl {
    let out = Command::new("curl")
        .args(["-q", "-sS", "--max-time", "10"])
        .args(args)
        .output()
        .expect("run curl");
    Curl {
        code: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}
