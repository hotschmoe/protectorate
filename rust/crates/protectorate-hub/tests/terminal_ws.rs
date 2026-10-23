#![cfg(target_os = "linux")]
#[path = "support/capability.rs"]
mod capability;
use protectorate_hub::Hub;
use protectorate_session::{CreateOptions, Manager};
use std::{
    fs,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::net::TcpListener;
use tungstenite::{client::IntoClientRequest, http::HeaderValue, Message};

#[tokio::test]
async fn terminal_websocket_auth_readonly_and_input_use_private_tmux() {
    if std::process::Command::new("tmux")
        .arg("-V")
        .output()
        .is_err()
    {
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let socket = format!("protectorate-terminal-{}-{nonce}", std::process::id());
    let manager = Arc::new(Manager::new(&socket));
    manager
        .create(&CreateOptions {
            name: "sleeve-worker".into(),
            cwd: Some(root.path().to_path_buf()),
            command: vec!["/bin/sh".into()],
            ..Default::default()
        })
        .unwrap();
    let hub = Hub::open(root.path(), "owner-secret", "test")
        .unwrap()
        .with_sessions(Arc::clone(&manager));
    let (readonly_id, readonly_token) = capability::mint(&hub, "readonly", &["terminal:readonly"]);
    let audit = Arc::clone(&hub.audit);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!(
        "ws://{}/ws/terminal/sleeve-worker",
        listener.local_addr().unwrap()
    );
    let server =
        tokio::spawn(async move { axum::serve(listener, hub.router().unwrap()).await.unwrap() });
    let readonly_proof = root.path().join("readonly-proof");
    let writable_proof = root.path().join("writable-proof");
    let (bad, ro, writable) = tokio::task::spawn_blocking(move || {
        let bad = tungstenite::connect(format!("{url}?readonly=1")).unwrap_err();
        assert_eq!(bad_http_status(&bad), 401);
        let mut denied_request = url.clone().into_client_request().unwrap();
        denied_request.headers_mut().insert(
            "Authorization",
            HeaderValue::from_str(&format!("Bearer {readonly_token}")).unwrap(),
        );
        let denied = tungstenite::connect(denied_request).unwrap_err();
        assert_eq!(bad_http_status(&denied), 403);
        let mut request = format!("{url}?readonly=1").into_client_request().unwrap();
        request.headers_mut().insert(
            "Authorization",
            HeaderValue::from_str(&format!("Bearer {readonly_token}")).unwrap(),
        );
        request.headers_mut().insert(
            "Sec-WebSocket-Protocol",
            HeaderValue::from_static("protectorate.v1"),
        );
        let (mut readonly, response) = tungstenite::connect(request).unwrap();
        assert_eq!(
            response.headers()["sec-websocket-protocol"],
            "protectorate.v1"
        );
        if let tungstenite::stream::MaybeTlsStream::Plain(stream) = readonly.get_mut() {
            stream
                .set_read_timeout(Some(Duration::from_millis(100)))
                .unwrap();
        }
        readonly
            .send(Message::Binary(
                format!("0printf no > {}\n", readonly_proof.display())
                    .into_bytes()
                    .into(),
            ))
            .unwrap();
        std::thread::sleep(Duration::from_millis(250));
        let _ = readonly.close(None);
        let mut request = url.into_client_request().unwrap();
        request.headers_mut().insert(
            "Authorization",
            HeaderValue::from_static("Bearer owner-secret"),
        );
        let (mut live, _) = tungstenite::connect(request).unwrap();
        if let tungstenite::stream::MaybeTlsStream::Plain(stream) = live.get_mut() {
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
        }
        live.send(Message::Binary(
            format!("0printf yes > {}\n", writable_proof.display())
                .into_bytes()
                .into(),
        ))
        .unwrap();
        let mut output = Vec::new();
        for _ in 0..10 {
            if let Ok(Message::Binary(bytes)) = live.read() {
                if bytes.first() == Some(&b'0') {
                    output.extend_from_slice(&bytes[1..]);
                }
            }
            if writable_proof.exists() {
                break;
            }
        }
        let _ = live.close(None);
        (
            bad,
            readonly_proof.exists(),
            (writable_proof.exists(), output),
        )
    })
    .await
    .unwrap();
    assert!(matches!(bad, tungstenite::Error::Http(_)));
    assert!(!ro, "read-only attach accepted keyboard input");
    assert!(writable.0, "writable attach failed: {:?}", writable.1);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let clients = std::process::Command::new("tmux")
            .args(["-L", &socket, "list-clients", "-F", "#{client_pid}"])
            .output()
            .unwrap();
        assert!(clients.status.success());
        if clients.stdout.is_empty() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "detached terminal clients survived: {}",
            String::from_utf8_lossy(&clients.stdout)
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    let entries = audit.tail(10).unwrap();
    let terminal = entries
        .iter()
        .filter(|entry| entry.procedure.starts_with("terminal.attach"))
        .collect::<Vec<_>>();
    assert_eq!(terminal.len(), 4, "one audit entry per attach attempt");
    assert!(terminal.iter().any(|entry| entry.actor == "unauthenticated"
        && entry.outcome == "denied"
        && entry.procedure == "terminal.attach.readonly"));
    assert!(terminal.iter().any(|entry| entry.actor == "token:readonly"
        && entry.outcome == "denied"
        && entry.procedure == "terminal.attach"
        && entry.token_id == readonly_id));
    assert!(terminal.iter().any(|entry| entry.actor == "token:readonly"
        && entry.outcome == "ok"
        && entry.procedure == "terminal.attach.readonly"
        && entry.subject == "sleeve-worker"));
    assert!(terminal.iter().any(|entry| entry.actor == "owner"
        && entry.outcome == "ok"
        && entry.procedure == "terminal.attach"));
    assert!(!serde_json::to_string(&entries)
        .unwrap()
        .contains("owner-secret"));
    let _ = manager.kill("sleeve-worker");
    let _ = fs::remove_file(root.path().join("readonly-proof"));
    server.abort();
}

fn bad_http_status(error: &tungstenite::Error) -> u16 {
    match error {
        tungstenite::Error::Http(response) => response.status().as_u16(),
        _ => panic!("expected HTTP denial, got {error}"),
    }
}
