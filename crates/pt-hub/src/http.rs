//! Routes, authentication per listener, the `/api/events` WebSocket and the web build.

use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use pt_http::{Message, Peer, Request, Response};
use pt_json::obj;

use crate::api::{self, ApiError, Principal};
use crate::bus::Since;
use crate::hub::Hub;
use crate::log::debug;
use crate::wire;

#[derive(Clone, Copy, Debug)]
pub enum Listener {
    /// The unix socket: peer credentials, or a token.
    Unix,
    /// 127.0.0.1: a token.
    Loopback,
    /// Behind Traefik: only the trusted proxy's address, with a token.
    Proxy { trusted: IpAddr },
}

fn error(e: &ApiError) -> Response {
    Response::json(e.status, e.to_json().to_string())
}

fn bearer(req: &Request) -> Option<String> {
    if let Some(v) = req.header("Authorization") {
        return v.strip_prefix("Bearer ").map(|t| t.trim().to_string());
    }
    // Browsers cannot set headers on a WebSocket.
    (req.path() == "/api/events")
        .then(|| req.query_param("token"))
        .flatten()
}

fn principal(hub: &Hub, l: Listener, req: &Request, peer: &Peer) -> Result<Principal, ApiError> {
    let view = hub.view();
    let token = bearer(req);
    let by_token = |t: &str| {
        api::from_token(hub, &view, t).ok_or_else(|| ApiError {
            status: 401,
            code: "unauthenticated",
            message: "unknown token".into(),
        })
    };
    match (l, peer) {
        (Listener::Unix, Peer::Unix(cred)) => {
            if let Some(p) = api::from_cgroup(hub, &view, cred.pid) {
                return Ok(p);
            }
            if let Some(t) = &token {
                return by_token(t);
            }
            if cred.uid == hub.uid {
                Ok(Principal::Owner)
            } else {
                Err(ApiError::denied("not the hub's user"))
            }
        }
        (Listener::Proxy { trusted }, Peer::Tcp(addr)) if addr.ip() != trusted => {
            Err(ApiError::denied("not the trusted proxy"))
        }
        (_, _) => match &token {
            Some(t) => by_token(t),
            None => Err(ApiError {
                status: 401,
                code: "unauthenticated",
                message: "a bearer token is required".into(),
            }),
        },
    }
}

pub fn handler(
    hub: Arc<Hub>,
    l: Listener,
    web: Option<PathBuf>,
) -> impl Fn(&Request, &Peer) -> Response + Send + Sync + 'static {
    move |req, peer| {
        let path = req.path();
        if let Some(method) = path.strip_prefix("/api/") {
            let who = match principal(&hub, l, req, peer) {
                Ok(p) => p,
                Err(e) => return error(&e),
            };
            if method == "events" {
                if req.method() != "GET" {
                    return Response::method_not_allowed("GET");
                }
                if !matches!(who, Principal::Owner | Principal::Envoy(_)) {
                    return error(&ApiError::denied("the event stream is owner-only"));
                }
                let from = req.query_param("from_seq").and_then(|s| s.parse().ok());
                return events(hub.clone(), from);
            }
            if req.method() != "POST" {
                return Response::method_not_allowed("POST");
            }
            let body = if req.body().is_empty() {
                Ok(pt_json::Value::Object(pt_json::Map::new()))
            } else {
                pt_json::parse_bytes(req.body())
            };
            let body = match body {
                Ok(v) if v.as_object().is_some() => v,
                Ok(_) => return error(&ApiError::bad("the body is a JSON object")),
                Err(e) => return error(&ApiError::bad(format!("bad JSON: {e}"))),
            };
            debug!("{method} by {who:?}");
            return match api::call(&hub, who, method, &body) {
                Ok(v) => Response::json(200, v.to_string()),
                Err(e) => error(&e),
            };
        }
        match &web {
            Some(dir) if req.method() == "GET" || req.method() == "HEAD" => static_file(req, dir),
            _ => Response::not_found(),
        }
    }
}

/// A file of the web build, or `index.html` for any other path (the SPA's own routes).
fn static_file(req: &Request, dir: &std::path::Path) -> Response {
    let rel = req.path().trim_start_matches('/');
    let safe = !rel.split('/').any(|c| c == ".." || c.starts_with('.')) && !rel.contains('%');
    let file = dir.join(rel);
    let pick = if safe && !rel.is_empty() && file.is_file() {
        file
    } else {
        dir.join("index.html")
    };
    Response::file(req, &pick).unwrap_or_else(|_| Response::not_found())
}

fn snapshot(hub: &Hub) -> (u64, String) {
    let seq = hub.bus.seq();
    let view = hub.view();
    let msg = obj! {
        "kind" => "snapshot",
        "seq" => seq,
        "state" => wire::state(hub, &view, &|_| true),
    };
    (seq, msg.to_string())
}

/// The WebSocket fan-out: replays from `from_seq` while the ring still has it, else starts
/// with a full snapshot, then streams every message.
fn events(hub: Arc<Hub>, from: Option<u64>) -> Response {
    Response::websocket(move |mut ws| {
        let writer = ws.writer();
        let closed = Arc::new(AtomicBool::new(false));
        let flag = closed.clone();
        thread::spawn(move || {
            loop {
                match ws.recv() {
                    Ok(Message::Close(..)) | Err(_) => break,
                    Ok(_) => {}
                }
            }
            flag.store(true, Ordering::SeqCst);
        });
        let mut cursor = match from {
            Some(f) if f <= hub.bus.seq() => f,
            _ => {
                let (seq, text) = snapshot(&hub);
                if writer.send_text(&text).is_err() {
                    return;
                }
                seq
            }
        };
        while !closed.load(Ordering::SeqCst) {
            match hub.bus.since(cursor, Duration::from_secs(1)) {
                Since::Items(items) => {
                    for (seq, text) in items {
                        if writer.send_text(&text).is_err() {
                            return;
                        }
                        cursor = seq;
                    }
                }
                Since::Gap => {
                    let (seq, text) = snapshot(&hub);
                    if writer.send_text(&text).is_err() {
                        return;
                    }
                    cursor = seq;
                }
            }
        }
    })
}
