#![cfg(target_os = "linux")]
//! Connect deadlines over the hub's real listener.
use protectorate_hub::Hub;
use serde_json::{json, Value};
use std::time::{Duration, Instant};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

const OWNER: &str = "owner-secret";

async fn serve(router: axum::Router) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    addr
}

fn envelope(flags: u8, message: &Value) -> Vec<u8> {
    let payload = message.to_string().into_bytes();
    let mut frame = vec![flags];
    frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    frame.extend_from_slice(&payload);
    frame
}

/// Opens one request and returns the stream with the request written.
async fn open(
    addr: &str,
    path: &str,
    streaming: bool,
    timeout: Option<&str>,
    body: &[u8],
) -> TcpStream {
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let content_type = if streaming {
        "application/connect+json"
    } else {
        "application/json"
    };
    let timeout = timeout
        .map(|ms| format!("Connect-Timeout-Ms: {ms}\r\n"))
        .unwrap_or_default();
    let head = format!("POST {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: {content_type}\r\nAuthorization: Bearer {OWNER}\r\n{timeout}Content-Length: {}\r\nConnection: close\r\n\r\n", body.len());
    stream.write_all(head.as_bytes()).await.unwrap();
    stream.write_all(body).await.unwrap();
    stream
}

/// The status and de-chunked body of a finished response.
async fn finish(mut stream: TcpStream) -> (u16, Vec<u8>) {
    let mut raw = Vec::new();
    tokio::time::timeout(Duration::from_secs(30), stream.read_to_end(&mut raw))
        .await
        .expect("response never ended")
        .unwrap();
    let split = raw.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
    let head = String::from_utf8_lossy(&raw[..split]).to_ascii_lowercase();
    let mut body = &raw[split + 4..];
    let status = head.split_whitespace().nth(1).unwrap().parse().unwrap();
    if !head.contains("transfer-encoding: chunked") {
        return (status, body.to_vec());
    }
    let mut out = Vec::new();
    loop {
        let end = body.windows(2).position(|w| w == b"\r\n").unwrap();
        let size = usize::from_str_radix(std::str::from_utf8(&body[..end]).unwrap(), 16).unwrap();
        if size == 0 {
            return (status, out);
        }
        out.extend_from_slice(&body[end + 2..end + 2 + size]);
        body = &body[end + 4 + size..];
    }
}

/// Connect stream frames as (flags, JSON).
fn frames(mut body: &[u8]) -> Vec<(u8, Value)> {
    let mut out = Vec::new();
    while body.len() >= 5 {
        let length = u32::from_be_bytes(body[1..5].try_into().unwrap()) as usize;
        out.push((
            body[0],
            serde_json::from_slice(&body[5..5 + length]).unwrap(),
        ));
        body = &body[5 + length..];
    }
    out
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn connect_deadlines_end_unary_waits_and_streams_with_deadline_exceeded() {
    let home = tempfile::tempdir().unwrap();
    let hub = Hub::open(home.path(), OWNER, "test").unwrap();
    let addr = serve(hub.router().unwrap()).await;
    let stream = open(
        &addr,
        "/protectorate.v1.MessageService/Send",
        false,
        None,
        json!({"to":"orchestrator","body":"nobody answers this"})
            .to_string()
            .as_bytes(),
    )
    .await;
    let (status, sent) = finish(stream).await;
    let sent: Value = serde_json::from_slice(&sent).unwrap();
    assert_eq!(status, 200, "{sent}");
    let wait = json!({"target": sent["message"]["id"], "timeout": "600s"}).to_string();
    let subscribe = envelope(0, &json!({}));

    for (timeout, code, http) in [
        ("300", "deadline_exceeded", 504),
        ("soon", "invalid_argument", 400),
    ] {
        let started = Instant::now();
        let stream = open(
            &addr,
            "/protectorate.v1.MessageService/Wait",
            false,
            Some(timeout),
            wait.as_bytes(),
        )
        .await;
        let (status, body) = finish(stream).await;
        let body: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["code"], code, "unary {timeout}: {body}");
        assert_eq!(status, http, "unary {timeout}: {body}");
        assert!(
            started.elapsed() < Duration::from_secs(20),
            "unary {timeout} outlived its deadline"
        );

        let stream = open(
            &addr,
            "/protectorate.v1.EventService/Subscribe",
            true,
            Some(timeout),
            &subscribe,
        )
        .await;
        let (status, body) = finish(stream).await;
        assert_eq!(status, 200);
        let frames = frames(&body);
        let (flags, end) = frames.last().unwrap();
        assert_eq!(*flags, 2, "stream {timeout} has no end frame: {frames:?}");
        assert_eq!(end["error"]["code"], code, "stream {timeout}: {end}");
        assert!(
            started.elapsed() < Duration::from_secs(40),
            "stream {timeout} outlived its deadline"
        );
    }
}
