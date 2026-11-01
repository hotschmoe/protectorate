//! Routes, authentication per listener, the `/api/events` WebSocket and the web build.

use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use pt_api::{
    ACCESS_TOKEN_PARAM, API_PREFIX, ApiError, ErrorCode, FROM_SEQ_PARAM, HubStreamMessage,
};
use pt_http::{Message, Peer, Request, Response};
use pt_json::ToJson;

use crate::api::{self, Principal, err};
use crate::bus::{Resume, Since};
use crate::hub::Hub;
use crate::log::debug;
use crate::wire;

#[derive(Clone, Copy, Debug)]
pub enum Listener {
    /// The owner's unix socket: peer credentials, or a token. Never mounted into a sandbox.
    Unix,
    /// The sleeves' unix socket: a sleeve capability, never peer credentials alone.
    Sleeves,
    /// 127.0.0.1: a token.
    Loopback,
    /// Behind Traefik: only the trusted proxy's address, with a token.
    Proxy { trusted: IpAddr },
}

/// One file of the web build, embedded by the `protectorate` binary's build script.
pub struct WebFile {
    /// Relative to the build root, such as `index.html` or `assets/FontManifest.json`.
    pub path: &'static str,
    pub etag: &'static str,
    pub bytes: &'static [u8],
}

/// Where the Flutter web build comes from.
pub enum Web {
    /// Not embedded: a page says so.
    None,
    /// From disk (`--web-dir`), for working on the app.
    Dir(PathBuf),
    Embedded(&'static [WebFile]),
}

fn error(e: &ApiError) -> Response {
    Response::json(e.code.http_status(), e.clone().body().to_json().to_string())
}

fn bearer(req: &Request) -> Option<String> {
    if let Some(v) = req.header("Authorization") {
        return v.strip_prefix("Bearer ").map(|t| t.trim().to_string());
    }
    // Browsers cannot set headers on a WebSocket.
    (req.path() == pt_api::EVENTS_PATH)
        .then(|| req.query_param(ACCESS_TOKEN_PARAM))
        .flatten()
}

/// Who is calling. A process inside a sleeve's scope is that sleeve whatever it presents; on
/// the owner's socket a caller without a token is the owner only when its cgroup was read and
/// names no sleeve of this hub. Anything that cannot be identified is refused.
fn principal(
    hub: &Hub,
    l: Listener,
    token: Option<&str>,
    peer: &Peer,
) -> Result<Principal, ApiError> {
    let view = hub.view();
    let by_token = |t: &str| {
        api::from_token(hub, &view, t)
            .ok_or_else(|| err(ErrorCode::Unauthenticated, "unknown token"))
    };
    let unknown = || err(ErrorCode::Forbidden, "cannot identify the calling process");
    match (l, peer) {
        (Listener::Unix | Listener::Sleeves, Peer::Unix(cred)) if cred.uid != hub.uid => {
            Err(err(ErrorCode::Forbidden, "not the hub's user"))
        }
        (Listener::Unix, Peer::Unix(cred)) => match api::peer_cgroup(cred.pid) {
            None => Err(unknown()),
            Some(cg) => match api::cgroup_sleeve(hub, &view, &cg) {
                api::CgroupSleeve::Live(p) => Ok(p),
                api::CgroupSleeve::Gone => Err(unknown()),
                api::CgroupSleeve::None => match token {
                    Some(t) => by_token(t),
                    None => Ok(Principal::Owner),
                },
            },
        },
        (Listener::Sleeves, Peer::Unix(cred)) => {
            let Some(t) = token else {
                return Err(err(
                    ErrorCode::Unauthenticated,
                    "a sleeve capability is required on this socket",
                ));
            };
            let p = by_token(t)?;
            if !matches!(p, Principal::Sleeve(_) | Principal::Envoy(_)) {
                return Err(err(
                    ErrorCode::Forbidden,
                    "only a sleeve capability is taken on this socket",
                ));
            }
            match api::peer_cgroup(cred.pid).map(|cg| api::cgroup_sleeve(hub, &view, &cg)) {
                Some(api::CgroupSleeve::Live(q)) if q != p => Err(err(
                    ErrorCode::Forbidden,
                    "that capability belongs to another sleeve",
                )),
                Some(api::CgroupSleeve::Gone) => Err(unknown()),
                _ => Ok(p),
            }
        }
        (Listener::Proxy { trusted }, Peer::Tcp(addr)) if addr.ip() != trusted => {
            Err(err(ErrorCode::Forbidden, "not the trusted proxy"))
        }
        (Listener::Unix | Listener::Sleeves, Peer::Tcp(_))
        | (Listener::Loopback | Listener::Proxy { .. }, Peer::Unix(_)) => Err(unknown()),
        (Listener::Loopback | Listener::Proxy { .. }, Peer::Tcp(_)) => match token {
            Some(t) => by_token(t),
            None => Ok(Principal::Anonymous),
        },
    }
}

/// A browser's Origin, when sent, must be this host: the web build is served same-origin.
fn same_origin(req: &Request) -> bool {
    let Some(origin) = req.header("Origin") else {
        return true;
    };
    let authority = origin.split_once("://").map_or(origin, |(_, a)| a);
    req.header("Host")
        .is_some_and(|h| h.eq_ignore_ascii_case(authority))
}

pub fn handler(
    hub: Arc<Hub>,
    l: Listener,
    web: Arc<Web>,
) -> impl Fn(&Request, &Peer) -> Response + Send + Sync + 'static {
    move |req, peer| {
        let path = req.path();
        let Some(method) = path.strip_prefix(API_PREFIX) else {
            return match req.method() {
                "GET" | "HEAD" => static_file(req, &web),
                _ => Response::method_not_allowed("GET, HEAD"),
            };
        };
        let token = bearer(req);
        let who = match principal(&hub, l, token.as_deref(), peer) {
            Ok(p) => p,
            Err(e) => return error(&e),
        };
        if path == pt_api::EVENTS_PATH {
            if req.method() != "GET" {
                return Response::method_not_allowed("GET");
            }
            if !matches!(
                who,
                Principal::Owner | Principal::Device(_) | Principal::Envoy(_)
            ) {
                let e = match who {
                    Principal::Anonymous => {
                        err(ErrorCode::Unauthenticated, "a bearer token is required")
                    }
                    _ => err(ErrorCode::Forbidden, "the event stream is for the owner"),
                };
                return error(&e);
            }
            if !same_origin(req) {
                return error(&err(ErrorCode::Forbidden, "cross-origin event stream"));
            }
            let from = req.query_param(FROM_SEQ_PARAM).and_then(|s| s.parse().ok());
            let (h, peer) = (hub.clone(), peer.clone());
            // A revoked device, or a token or sleeve that no longer names this caller, ends
            // the stream within a second.
            let still = move || principal(&h, l, token.as_deref(), &peer).is_ok_and(|p| p == who);
            return events(hub.clone(), from, still);
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
            Ok(_) => return error(&err(ErrorCode::Invalid, "the body is a JSON object")),
            Err(e) => return error(&err(ErrorCode::Invalid, format!("bad JSON: {e}"))),
        };
        debug!("{method} by {who:?}");
        match api::call(&hub, &who, method, &body) {
            Ok(v) => Response::json(200, v.to_string()),
            Err(e) => error(&e),
        }
    }
}

const NOT_EMBEDDED: &str = "<!doctype html><meta charset=utf-8><title>Protectorate</title>\
<pre>This protectorate binary was built without the web app.\n\n\
Build it with `make web release` (or serve a build with --web-dir DIR).\n\
The API is at /api/.</pre>\n";

/// The build file a request names, or `index.html` for the app's own routes. A missing path
/// whose last segment has a dot is a missing asset, not a route.
fn pick(rel: &str, exists: impl Fn(&str) -> bool) -> Option<String> {
    let rel = rel.trim_start_matches('/');
    let rel = if rel.is_empty() { "index.html" } else { rel };
    if exists(rel) {
        return Some(rel.to_string());
    }
    let last = rel.rsplit('/').next().unwrap_or("");
    (!last.contains('.')).then(|| "index.html".to_string())
}

fn static_file(req: &Request, web: &Web) -> Response {
    match web {
        Web::None => Response::bytes(200, "text/html; charset=utf-8", NOT_EMBEDDED.as_bytes()),
        Web::Embedded(files) => {
            let find = |p: &str| files.iter().find(|f| f.path == p);
            let Some(f) = pick(req.path(), |p| find(p).is_some()).and_then(|p| find(&p)) else {
                return Response::not_found();
            };
            Response::bytes(200, pt_http::content_type(f.path), f.bytes)
                .header("Cache-Control", "no-cache")
                .etag(req, f.etag)
        }
        Web::Dir(dir) => {
            let rel = req.path().trim_start_matches('/');
            let safe =
                !rel.split('/').any(|c| c == ".." || c.starts_with('.')) && !rel.contains('%');
            if !safe {
                return Response::not_found();
            }
            let Some(p) = pick(rel, |p| dir.join(p).is_file()) else {
                return Response::not_found();
            };
            Response::file(req, &dir.join(Path::new(&p)))
                .map(|r| r.header("Cache-Control", "no-cache"))
                .unwrap_or_else(|_| Response::not_found())
        }
    }
}

/// The snapshot message, and the bus position it covers.
fn snapshot(hub: &Hub) -> (u64, String) {
    let (seq, pos) = hub.bus.mark();
    let view = hub.view();
    let state = wire::state(hub, &view, seq, &|_| true);
    let msg = HubStreamMessage::Snapshot { seq, state };
    (pos, msg.to_json().to_string())
}

/// The WebSocket fan-out: replays from `from_seq` while the ring or the stream log still holds
/// every kept message from it, else starts with a snapshot; then streams every message, deltas
/// included, for as long as `still` holds.
fn events(
    hub: Arc<Hub>,
    from: Option<u64>,
    still: impl Fn() -> bool + Send + Sync + 'static,
) -> Response {
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
        // Deltas up to here belong to messages the client already has or is about to replay.
        let replay_to = hub.bus.pos();
        let start = match from.and_then(|f| hub.bus.resume(f)) {
            Some(Resume::Ring(pos)) => Some(pos),
            Some(Resume::Log {
                from,
                until,
                cursor,
            }) => hub.bus.logged(from, until).and_then(|texts| {
                texts
                    .iter()
                    .all(|t| writer.send_text(t).is_ok())
                    .then_some(cursor)
            }),
            None => None,
        };
        let (mut cursor, replay_to) = match start {
            Some(pos) => (pos, replay_to),
            None => {
                let (pos, text) = snapshot(&hub);
                if writer.send_text(&text).is_err() {
                    return;
                }
                (pos, 0)
            }
        };
        while !closed.load(Ordering::SeqCst) {
            if !still() {
                let _ = writer.close(1008, "no longer authorized");
                return;
            }
            match hub.bus.since(cursor, Duration::from_secs(1)) {
                Since::Items(items) => {
                    for i in items {
                        cursor = i.pos;
                        if !i.kept && i.pos <= replay_to {
                            continue;
                        }
                        if writer.send_text(&i.text).is_err() {
                            return;
                        }
                    }
                }
                Since::Gap => {
                    let (pos, text) = snapshot(&hub);
                    if writer.send_text(&text).is_err() {
                        return;
                    }
                    cursor = pos;
                }
            }
        }
    })
}
