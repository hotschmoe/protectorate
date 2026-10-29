//! A real hub binary over real HTTP: devices present the bearer token the
//! owner added them with, the proxy listener admits only the trusted proxy
//! and never the owner token, and capabilities renew, narrow and die when
//! revoked.
#![cfg(target_os = "linux")]
use crate::support;

use protectorate_auth::capability::{self, Caveat};
use protectorate_connect_client::{Client, Credential};
use protectorate_wire::Code;
use serde_json::json;
use std::{
    fs,
    io::{Read, Write},
    os::unix::fs::PermissionsExt,
    path::Path,
    sync::mpsc,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use support::*;

const STATUS: &str = "/protectorate.v1.HubService/GetStatus";
const DEVICES: &str = "/protectorate.v1.DeviceService/";
const TOKENS: &str = "/protectorate.v1.TokenService/";

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}

#[test]
fn devices_use_their_token_through_the_proxy_until_revoked() {
    let root = tempfile::tempdir().unwrap();
    let hub = Hub::start(root.path());
    let (id, token) = add_device(&hub, "laptop");
    assert!(token.starts_with("pd1."), "{token}");
    assert_eq!(token.len(), 4 + 43, "{token}");
    let (status, body) = post(&hub.proxy, STATUS, &bearer(&token), "{}");
    assert_eq!(status, 200, "{body}");
    // Owner approvals and device management take devices as well as the owner.
    let approvals = "/protectorate.v1.ApprovalService/ListApprovals";
    assert_eq!(post(&hub.proxy, approvals, &bearer(&token), "{}").0, 200);
    let (status, added) = post(
        &hub.proxy,
        &format!("{DEVICES}AddDevice"),
        &bearer(&token),
        r#"{"name":"phone"}"#,
    );
    assert_eq!(status, 200, "{added}");
    assert_eq!(added["device"]["addedBy"], "token:laptop", "{added}");
    let listed = hub.call(&format!("{DEVICES}ListDevices"), json!({}));
    let laptop = listed["devices"]
        .as_array()
        .unwrap()
        .iter()
        .find(|device| device["id"] == id.as_str())
        .unwrap_or_else(|| panic!("{listed}"));
    assert_eq!(laptop["name"], "laptop");
    assert_eq!(laptop["addedBy"], "owner");
    assert!(laptop["lastUsedAt"].is_string(), "{laptop}");
    // The hub keeps only a hash of each token.
    let identity = fs::read_to_string(hub.home.join("identity.json")).unwrap();
    assert!(!identity.contains(&token[4..]), "the token was stored");

    // Anything but the exact token is refused, before any handler runs.
    let mut forged = token.clone();
    forged.replace_range(10..11, if &token[10..11] == "A" { "B" } else { "A" });
    for wrong in [forged.as_str(), "pd1.", "pd1.not-a-token", "Proof x.1.2.3"] {
        let (status, body) = post(&hub.proxy, STATUS, &bearer(wrong), "{}");
        assert_eq!(status, 401, "{wrong}: {body}");
    }
    let (status, body) = post(&hub.proxy, STATUS, "", "{}");
    assert_eq!(status, 401, "{body}");

    // A device token works on loopback too; its open stream ends on revoke.
    let device = Client::new(&hub.plain, token.as_str(), Duration::from_secs(20)).unwrap();
    let (events, received) = mpsc::channel();
    let stream = device.isolated();
    let watcher = thread::spawn(move || {
        stream.stream(
            "/protectorate.v1.EventService/Subscribe",
            &json!({}),
            |_| {
                let _ = events.send(());
                Ok(())
            },
        )
    });
    received
        .recv_timeout(Duration::from_secs(120))
        .expect("the device's event stream delivers");
    hub.call(&format!("{DEVICES}RevokeDevice"), json!({"id": id}));
    let deadline = Instant::now() + Duration::from_secs(120);
    while !watcher.is_finished() {
        assert!(Instant::now() < deadline, "revocation left the stream open");
        thread::sleep(Duration::from_millis(50));
    }
    let error = watcher.join().unwrap().unwrap_err();
    assert_eq!(error.code, Code::Unauthenticated, "{error:?}");
    let error = device.call(STATUS, &json!({})).unwrap_err();
    assert!(error.message.contains("unknown credential"), "{error:?}");
    let (status, body) = post(&hub.proxy, STATUS, &bearer(&token), "{}");
    assert_eq!(status, 401, "{body}");
    let phone = added["token"].as_str().unwrap();
    assert_eq!(post(&hub.proxy, STATUS, &bearer(phone), "{}").0, 200);
}

#[test]
fn the_owner_token_and_debug_routes_work_only_on_the_loopback_listener() {
    let root = tempfile::tempdir().unwrap();
    let hub = Hub::start(root.path());
    assert!(hub.client.call(STATUS, &json!({})).is_ok());
    let (status, body) = post(&hub.proxy, STATUS, &bearer(OWNER), "{}");
    assert_eq!(status, 401, "{body}");
    assert!(
        body["message"]
            .as_str()
            .unwrap()
            .contains("loopback listener"),
        "{body}"
    );
    let debug = |base: &str| {
        let address = base.trim_start_matches("http://");
        let mut stream = std::net::TcpStream::connect(address).unwrap();
        write!(
            stream,
            "GET /debug/pprof/ HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {OWNER}\r\nConnection: close\r\n\r\n"
        )
        .unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response.split_whitespace().nth(1).unwrap().to_owned()
    };
    assert_eq!(debug(&hub.plain), "200");
    assert_eq!(debug(&hub.proxy), "403");
}

#[test]
fn the_proxy_listener_drops_every_peer_but_the_trusted_proxy() {
    let root = tempfile::tempdir().unwrap();
    let hub = Hub::start(root.path());
    let (_, token) = add_device(&hub, "laptop");
    let address: std::net::SocketAddr = hub.proxy.trim_start_matches("http://").parse().unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    // A client on another address reaches the port but never the hub.
    let mut stream = runtime.block_on(async {
        let socket = tokio::net::TcpSocket::new_v4().unwrap();
        socket.bind("127.0.0.3:0".parse().unwrap()).unwrap();
        socket.connect(address).await.unwrap().into_std().unwrap()
    });
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(60)))
        .unwrap();
    let request = format!(
        "POST {STATUS} HTTP/1.1\r\nHost: {address}\r\nContent-Type: application/json\r\nAuthorization: Bearer {token}\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}"
    );
    let _ = stream.write_all(request.as_bytes());
    let mut response = Vec::new();
    match stream.read_to_end(&mut response) {
        Ok(_) => assert!(
            response.is_empty(),
            "a peer that is not the proxy got: {}",
            String::from_utf8_lossy(&response)
        ),
        Err(error) => assert_eq!(error.kind(), std::io::ErrorKind::ConnectionReset, "{error}"),
    }
    // The trusted proxy is still served.
    assert_eq!(post(&hub.proxy, STATUS, &bearer(&token), "{}").0, 200);
}

#[test]
fn proxy_listener_flags_are_checked_at_start() {
    let root = tempfile::tempdir().unwrap();
    let token = root.path().join("owner-token");
    fs::write(&token, OWNER).unwrap();
    fs::set_permissions(&token, fs::Permissions::from_mode(0o600)).unwrap();
    let home = root.path().join("hub");
    for (extra, expected) in [
        (
            vec!["--proxy-listen", "127.0.0.2:0"],
            "--proxy-listen requires at least one --trusted-proxy",
        ),
        (
            vec!["--trusted-proxy", "127.0.0.1"],
            "--trusted-proxy applies only to a --proxy-listen",
        ),
        (
            vec![
                "--proxy-listen",
                "0.0.0.0:7470",
                "--trusted-proxy",
                "127.0.0.1",
            ],
            "refusing to listen",
        ),
        (
            vec![
                "--proxy-listen",
                "8.8.8.8:7470",
                "--trusted-proxy",
                "127.0.0.1",
            ],
            "refusing to listen",
        ),
        (
            vec![
                "--proxy-listen",
                "127.0.0.2:0",
                "--trusted-proxy",
                "8.8.8.8",
            ],
            "not an explicit private address",
        ),
        (vec!["--tls-listen", "127.0.0.2:0"], "unknown argument"),
    ] {
        let mut args = vec![
            "serve",
            "--home",
            home.to_str().unwrap(),
            "--owner-token-file",
            token.to_str().unwrap(),
        ];
        args.extend(&extra);
        let output = run(&args, root.path());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "{extra:?} was accepted");
        assert!(stderr.contains(expected), "{extra:?}: {stderr}");
    }
}

#[test]
fn the_terminal_websocket_takes_the_token_as_a_subprotocol() {
    use tungstenite::{client::IntoClientRequest, http::HeaderValue, Message};
    let root = tempfile::tempdir().unwrap();
    let hub = Hub::start(root.path());
    hub.call(
        "/protectorate.v1.SessionService/CreateSession",
        json!({"name": "term", "command": ["/bin/sh", "-c", "echo terminal-ready; exec cat"]}),
    );
    let (id, token) = add_device(&hub, "phone");
    let url = format!("{}/ws/terminal/term", hub.proxy.replace("http://", "ws://"));
    let attach = |protocols: String, url: &str| {
        let mut request = url.into_client_request().unwrap();
        request.headers_mut().insert(
            "Sec-WebSocket-Protocol",
            HeaderValue::from_str(&protocols).unwrap(),
        );
        tungstenite::connect(request)
    };
    let status = |error: tungstenite::Error| match error {
        tungstenite::Error::Http(response) => response.status().as_u16(),
        other => panic!("not an HTTP refusal: {other}"),
    };
    assert_eq!(
        status(attach("protectorate.v1".into(), &url).unwrap_err()),
        401
    );
    assert_eq!(
        status(attach("protectorate.v1".into(), &format!("{url}?token={token}")).unwrap_err()),
        401,
        "the query fallback is gone"
    );
    assert_eq!(
        status(attach(format!("protectorate.v1, bearer.{OWNER}"), &url).unwrap_err()),
        401,
        "the owner token is refused through the proxy"
    );
    let (mut socket, response) = attach(format!("protectorate.v1, bearer.{token}"), &url).unwrap();
    assert_eq!(
        response.headers()["sec-websocket-protocol"],
        "protectorate.v1",
        "the hub never echoes the token"
    );
    let deadline = Instant::now() + Duration::from_secs(120);
    let mut seen = Vec::new();
    while !String::from_utf8_lossy(&seen).contains("terminal-ready") {
        assert!(Instant::now() < deadline, "no terminal output");
        if let Message::Binary(frame) = socket.read().unwrap() {
            if frame.first() == Some(&b'0') {
                seen.extend_from_slice(&frame[1..]);
            }
        }
    }
    // Revoking the device closes its open terminal.
    hub.call(&format!("{DEVICES}RevokeDevice"), json!({"id": id}));
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        assert!(
            Instant::now() < deadline,
            "revocation left the terminal open"
        );
        match socket.read() {
            Ok(Message::Close(_)) | Err(_) => break,
            Ok(_) => {}
        }
    }
    assert_eq!(
        status(attach(format!("protectorate.v1, bearer.{token}"), &url).unwrap_err()),
        401
    );
}

fn write_private(path: &Path, token: &str) {
    fs::write(path, format!("{token}\n")).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
}

#[test]
fn capabilities_renew_across_expiry_narrow_and_die_when_revoked() {
    let root = tempfile::tempdir().unwrap();
    let hub = Hub::start(root.path());
    let created = hub.call(
        &format!("{TOKENS}CreateToken"),
        json!({"name": "script", "scopes": ["read"], "ttl": "8s"}),
    );
    let first = created["secret"].as_str().unwrap().to_owned();
    let file = root.path().join("script.token");
    write_private(&file, &first);
    // The client renews a token file through the hub past half its life.
    let client = Client::new(&hub.plain, "", Duration::from_secs(10))
        .unwrap()
        .with_credential(Credential::TokenFile(file.clone()));
    let mut seen = vec![first.clone()];
    // Half the TTL is the margin a call on a loaded host has to renew it.
    // It stops once the first token is past its life and two renewals were seen.
    let start = Instant::now();
    let until = start + Duration::from_secs(20);
    while Instant::now() < until && !(seen.len() >= 3 && start.elapsed() > Duration::from_secs(9)) {
        client
            .call(STATUS, &json!({}))
            .unwrap_or_else(|error| panic!("renewed capability refused: {error:?}"));
        let current = fs::read_to_string(&file).unwrap().trim().to_owned();
        if seen.last() != Some(&current) {
            seen.push(current);
        }
        thread::sleep(Duration::from_millis(250));
    }
    assert!(
        seen.len() >= 3,
        "the token file was renewed {} times",
        seen.len() - 1
    );
    let error = Client::new(&hub.plain, first.as_str(), Duration::from_secs(5))
        .unwrap()
        .call(STATUS, &json!({}))
        .unwrap_err();
    assert!(error.message.contains("expired"), "{error:?}");

    let current = seen.last().unwrap().clone();
    let narrowed = capability::attenuate(&current, &Caveat::Scopes(vec!["events".into()])).unwrap();
    let widened = capability::attenuate(&narrowed, &Caveat::Scopes(vec!["*".into()])).unwrap();
    for token in [&narrowed, &widened] {
        let error = Client::new(&hub.plain, token.as_str(), Duration::from_secs(5))
            .unwrap()
            .call(STATUS, &json!({}))
            .unwrap_err();
        assert_eq!(error.code, Code::PermissionDenied, "{error:?}");
    }
    let shorter = capability::attenuate(&current, &Caveat::Expires(now() - 1)).unwrap();
    let error = Client::new(&hub.plain, shorter.as_str(), Duration::from_secs(5))
        .unwrap()
        .call(STATUS, &json!({}))
        .unwrap_err();
    assert!(error.message.contains("expired"), "{error:?}");

    hub.call(
        &format!("{TOKENS}RevokeToken"),
        json!({"id": created["token"]["id"]}),
    );
    for token in [&current, &narrowed] {
        let error = Client::new(&hub.plain, token.as_str(), Duration::from_secs(5))
            .unwrap()
            .call(STATUS, &json!({}))
            .unwrap_err();
        assert!(error.message.contains("revoked"), "{error:?}");
    }
}
