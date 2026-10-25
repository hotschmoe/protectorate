//! A real hub binary over real HTTP: devices sign every request with the key
//! they registered while pairing, the owner token works only on the loopback
//! listener, and capabilities renew, narrow and die when revoked.
#![cfg(target_os = "linux")]
use crate::support;

use base64::{engine::general_purpose::STANDARD, Engine};
use protectorate_auth::{
    capability::{self, Caveat},
    proof::DeviceKey,
};
use protectorate_connect_client::{Client, Credential};
use protectorate_wire::Code;
use serde_json::{json, Value};
use std::{
    fs,
    io::{Read, Write},
    net::TcpStream,
    os::unix::fs::PermissionsExt,
    path::Path,
    sync::{mpsc, Arc},
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

/// One raw Connect call, so a test controls the Authorization header.
fn post(base: &str, path: &str, authorization: &str) -> (u16, Value) {
    let address = base.trim_start_matches("http://");
    let mut stream = TcpStream::connect(address).unwrap();
    write!(
        stream,
        "POST {path} HTTP/1.1\r\nHost: {address}\r\nContent-Type: application/json\r\nAuthorization: {authorization}\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}"
    )
    .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    let (head, body) = response.split_once("\r\n\r\n").unwrap();
    (
        head.split_whitespace().nth(1).unwrap().parse().unwrap(),
        serde_json::from_str(body).unwrap_or(Value::Null),
    )
}

fn tls_client(hub: &Hub, token: &str) -> Client {
    Client::new_with_ca(&hub.tls, token, Duration::from_secs(20), Some(&hub.ca())).unwrap()
}

/// Pairs a fresh key over the TLS listener; the owner approves on loopback.
fn pair(hub: &Hub, name: &str) -> (Arc<DeviceKey>, String) {
    let key = Arc::new(DeviceKey::generate().unwrap().0);
    let public = tls_client(hub, "");
    let started = public
        .call(
            &format!("{DEVICES}StartDeviceAuth"),
            &json!({"name": name, "platform": "linux", "publicKey": STANDARD.encode(key.public_key())}),
        )
        .unwrap();
    hub.call(
        &format!("{DEVICES}ApproveDeviceAuth"),
        json!({"userCode": started["userCode"]}),
    );
    let approved = public
        .call(
            &format!("{DEVICES}PollDeviceAuth"),
            &json!({"deviceCode": started["deviceCode"]}),
        )
        .unwrap();
    assert_eq!(approved["status"], "approved", "{approved}");
    let id = approved["device"]["id"].as_str().unwrap().to_owned();
    assert_eq!(id, key.id());
    (key, id)
}

#[test]
fn devices_sign_every_request_and_revocation_closes_their_streams() {
    let root = tempfile::tempdir().unwrap();
    let hub = Hub::start(root.path());
    let (key, id) = pair(&hub, "laptop");
    let device = tls_client(&hub, "").with_credential(Credential::Device(Arc::clone(&key)));
    assert!(device.call(STATUS, &json!({})).is_ok());
    // Owner approvals take signed device requests as well as the owner.
    let approvals = "/protectorate.v1.ApprovalService/ListApprovals";
    assert!(device.call(approvals, &json!({})).is_ok());
    assert!(hub.client.call(approvals, &json!({})).is_ok());

    let proof = format!("Proof {}", key.prove("POST", STATUS, now()).unwrap());
    assert_eq!(post(&hub.plain, STATUS, &proof).0, 200);
    let (status, body) = post(&hub.plain, STATUS, &proof);
    assert_eq!(status, 401, "{body}");
    assert!(
        body["message"].as_str().unwrap().contains("replayed"),
        "{body}"
    );
    let stale = format!("Proof {}", key.prove("POST", STATUS, now() - 120).unwrap());
    let (status, body) = post(&hub.plain, STATUS, &stale);
    assert_eq!(status, 401, "{body}");
    assert!(
        body["message"].as_str().unwrap().contains("clock skew"),
        "{body}"
    );
    let elsewhere = format!("Proof {}", key.prove("POST", "/elsewhere", now()).unwrap());
    assert_eq!(post(&hub.plain, STATUS, &elsewhere).0, 401);

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
}

#[test]
fn the_owner_token_works_only_on_the_loopback_listener() {
    let root = tempfile::tempdir().unwrap();
    let hub = Hub::start(root.path());
    assert!(hub.client.call(STATUS, &json!({})).is_ok());
    let error = tls_client(&hub, OWNER)
        .call(STATUS, &json!({}))
        .unwrap_err();
    assert_eq!(error.code, Code::Unauthenticated);
    assert!(error.message.contains("loopback listener"), "{error:?}");
}

#[test]
fn the_terminal_websocket_takes_the_proof_as_a_subprotocol() {
    use tungstenite::{client::IntoClientRequest, http::HeaderValue, Message};
    let root = tempfile::tempdir().unwrap();
    let hub = Hub::start(root.path());
    hub.call(
        "/protectorate.v1.SessionService/CreateSession",
        json!({"name": "term", "command": ["/bin/sh", "-c", "echo terminal-ready; exec cat"]}),
    );
    let (key, _) = pair(&hub, "phone");
    let url = format!("{}/ws/terminal/term", hub.plain.replace("http://", "ws://"));
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
        status(attach("protectorate.v1".into(), &format!("{url}?token={OWNER}")).unwrap_err()),
        401,
        "the query fallback is gone"
    );
    let proof = key.prove("GET", "/ws/terminal/term", now()).unwrap();
    let (mut socket, response) = attach(format!("protectorate.v1, proof.{proof}"), &url).unwrap();
    assert_eq!(
        response.headers()["sec-websocket-protocol"],
        "protectorate.v1",
        "the hub never echoes the proof"
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
    let _ = socket.close(None);
    let replayed = attach(format!("protectorate.v1, proof.{proof}"), &url).unwrap_err();
    assert_eq!(status(replayed), 401);
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
