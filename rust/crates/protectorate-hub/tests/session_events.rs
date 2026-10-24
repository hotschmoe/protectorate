#![cfg(target_os = "linux")]
use crate::support::private_tmux;

use private_tmux::PrivateTmux;
use protectorate_hub::Hub;
use protectorate_session::{CreateOptions, Manager};
use serde_json::Value;
use std::{sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

async fn line(stream: &mut TcpStream) -> String {
    let mut bytes = Vec::new();
    loop {
        let mut byte = [0];
        stream.read_exact(&mut byte).await.unwrap();
        bytes.push(byte[0]);
        if bytes.ends_with(b"\r\n") {
            return String::from_utf8(bytes).unwrap();
        }
    }
}

async fn event(stream: &mut TcpStream) -> Value {
    let count = usize::from_str_radix(line(stream).await.trim(), 16).unwrap();
    let mut frame = vec![0; count];
    stream.read_exact(&mut frame).await.unwrap();
    assert_eq!(line(stream).await, "\r\n");
    assert_eq!(frame[0], 0);
    serde_json::from_slice(&frame[5..]).unwrap()
}

async fn next_kind(stream: &mut TcpStream, kind: &str) -> Value {
    tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            let received = event(stream).await;
            if received["kind"] == kind {
                return received;
            }
        }
    })
    .await
    .expect("session event did not arrive")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn external_tmux_lifecycle_reaches_event_stream() {
    let temp = tempfile::tempdir().unwrap();
    let socket = format!(
        "p-rust-session-events-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos()
    );
    let _tmux = PrivateTmux::new(&socket);
    let manager = Arc::new(Manager::new(&socket));
    let hub = Hub::open(temp.path(), "owner-secret", "test")
        .unwrap()
        .with_sessions(Arc::clone(&manager))
        .with_dead_retention(Duration::ZERO);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, hub.router().unwrap()).await.unwrap();
    });
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let frame = [0, 0, 0, 0, 2, b'{', b'}'];
    let headers = format!(
        "POST /protectorate.v1.EventService/Subscribe HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/connect+json\r\nAuthorization: Bearer owner-secret\r\nContent-Length: {}\r\n\r\n",
        frame.len()
    );
    stream.write_all(headers.as_bytes()).await.unwrap();
    stream.write_all(&frame).await.unwrap();
    assert!(line(&mut stream).await.contains("200"));
    while line(&mut stream).await != "\r\n" {}
    assert_eq!(event(&mut stream).await["kind"], "heartbeat");

    manager
        .create(&CreateOptions {
            name: "outside".into(),
            command: vec!["/bin/sh".into(), "-c".into(), "sleep 0.4; exit 7".into()],
            ..Default::default()
        })
        .unwrap();
    let created = next_kind(&mut stream, "session.created").await;
    assert_eq!(created["subject"], "outside");
    let exited = next_kind(&mut stream, "session.exited").await;
    assert_eq!(exited["subject"], "outside");
    assert_eq!(exited["payload"]["exit_code"].as_f64(), Some(7.0));
    manager.kill("outside").unwrap();
    let removed = next_kind(&mut stream, "session.removed").await;
    assert_eq!(removed["subject"], "outside");
    server.abort();
}
