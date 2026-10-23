use protectorate_wire::*;
use serde_json::{json, Value as Json};
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
const SUBSCRIBE: &str = "/protectorate.v1.EventService/Subscribe";
fn authorizer() -> Authorizer {
    Arc::new(|method, headers| {
        assert!(method.server_streaming);
        if headers
            .get("authorization")
            .is_none_or(|v| v != "Bearer allowed")
        {
            return Err(RpcError::new(Code::Unauthenticated, "bearer required"));
        }
        Ok(AuthContext {
            actor: "alice".into(),
            ..Default::default()
        })
    })
}
async fn serve(server: UnaryServer) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let task = tokio::spawn(async move { axum::serve(listener, server.router()).await.unwrap() });
    (addr, task)
}
async fn request(addr: std::net::SocketAddr, body: &[u8], headers: &str) -> (u16, String, Vec<u8>) {
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let request=format!("POST {SUBSCRIBE} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/connect+json\r\nConnect-Protocol-Version: 1\r\nContent-Length: {}\r\nConnection: close\r\n{headers}\r\n",body.len());
    stream.write_all(request.as_bytes()).await.unwrap();
    stream.write_all(body).await.unwrap();
    let mut raw = Vec::new();
    tokio::time::timeout(Duration::from_secs(3), stream.read_to_end(&mut raw))
        .await
        .unwrap()
        .unwrap();
    let split = raw.windows(4).position(|s| s == b"\r\n\r\n").unwrap();
    let headers = String::from_utf8(raw[..split].to_vec()).unwrap();
    let status = headers.split_whitespace().nth(1).unwrap().parse().unwrap();
    let body = &raw[split + 4..];
    let body = if headers
        .to_ascii_lowercase()
        .contains("transfer-encoding: chunked")
    {
        let mut body = body;
        let mut out = Vec::new();
        loop {
            let end = body.windows(2).position(|s| s == b"\r\n").unwrap();
            let size = usize::from_str_radix(
                std::str::from_utf8(&body[..end])
                    .unwrap()
                    .split(';')
                    .next()
                    .unwrap(),
                16,
            )
            .unwrap();
            body = &body[end + 2..];
            if size == 0 {
                break;
            }
            out.extend_from_slice(&body[..size]);
            assert_eq!(&body[size..size + 2], b"\r\n");
            body = &body[size + 2..];
        }
        out
    } else {
        body.to_vec()
    };
    (status, headers, body)
}
fn frames(mut body: &[u8]) -> Vec<(u8, Json)> {
    let mut out = Vec::new();
    while !body.is_empty() {
        assert!(body.len() >= 5);
        let flags = body[0];
        let len = u32::from_be_bytes(body[1..5].try_into().unwrap()) as usize;
        assert!(body.len() >= 5 + len);
        out.push((flags, serde_json::from_slice(&body[5..5 + len]).unwrap()));
        body = &body[5 + len..];
    }
    out
}
#[tokio::test]
async fn real_subscribe_stream_sends_data_then_success_and_trailers() {
    let c = Catalog::embedded().unwrap();
    let output = c.method(SUBSCRIBE).unwrap().output();
    let mut server = UnaryServer::new(c, authorizer()).with_stream_capacity(1);
    server.register_streaming(SUBSCRIBE,move|request,sender|{let output=output.clone();async move{assert_eq!(request.auth.actor,"alice");for i in 1..=3{let message=decode_json(output.clone(),&serde_json::to_vec(&json!({"id":i.to_string(),"kind":"note.created","payload":{"count":i}})).unwrap())?;sender.send(message).await?;}Ok(BTreeMap::from([("notice-count".into(),vec!["3".into()])]))}}).unwrap();
    let (addr, task) = serve(server).await;
    let request_frame = encode_envelope(0, b"{}").unwrap();
    let (status, headers, body) =
        request(addr, &request_frame, "Authorization: Bearer allowed\r\n").await;
    assert_eq!(status, 200);
    assert!(headers
        .to_lowercase()
        .contains("content-type: application/connect+json"));
    let frames = frames(&body);
    assert_eq!(frames.len(), 4);
    for (i, (flags, body)) in frames[..3].iter().enumerate() {
        assert_eq!(*flags, 0);
        assert_eq!(body["id"], (i + 1).to_string());
    }
    assert_eq!(
        frames[3],
        (FLAG_END_STREAM, json!({"metadata":{"notice-count":["3"]}}))
    );
    task.abort();
}
#[tokio::test]
async fn stream_errors_and_auth_failures_are_end_envelopes_with_http_200() {
    let c = Catalog::embedded().unwrap();
    let output = c.method(SUBSCRIBE).unwrap().output();
    let mut server = UnaryServer::new(c, authorizer());
    server
        .register_streaming(SUBSCRIBE, move |_, sender| {
            let output = output.clone();
            async move {
                sender
                    .send(decode_json(output, br#"{"id":"first"}"#)?)
                    .await?;
                Err(RpcError::new(Code::Unavailable, "retry later"))
            }
        })
        .unwrap();
    let (addr, task) = serve(server).await;
    let frame = encode_envelope(0, b"{}").unwrap();
    let (status, _, body) = request(addr, &frame, "Authorization: Bearer allowed\r\n").await;
    assert_eq!(status, 200);
    let out = frames(&body);
    assert_eq!(out.len(), 2);
    assert_eq!(out[1].0, FLAG_END_STREAM);
    assert_eq!(out[1].1["error"]["code"], "unavailable");
    let (status, _, body) = request(addr, &frame, "").await;
    assert_eq!(status, 200);
    assert_eq!(frames(&body)[0].1["error"]["code"], "unauthenticated");
    let bad = encode_envelope(0, br#"{"unknown":true}"#).unwrap();
    let (status, _, body) = request(addr, &bad, "Authorization: Bearer allowed\r\n").await;
    assert_eq!(status, 200);
    assert_eq!(frames(&body)[0].1["error"]["code"], "invalid_argument");
    task.abort();
}
#[tokio::test]
async fn stream_deadline_and_response_type_errors_have_terminal_errors() {
    let c = Catalog::embedded().unwrap();
    let input = c.method(SUBSCRIBE).unwrap().input();
    let outcomes = Arc::new(Mutex::new(Vec::<Code>::new()));
    let recorded = Arc::clone(&outcomes);
    let mut server = UnaryServer::new(c, authorizer()).with_completion_observer(Arc::new(
        move |_, _, _, error| {
            recorded.lock().unwrap().push(error.unwrap().code);
        },
    ));
    server
        .register_streaming(SUBSCRIBE, move |_, sender| {
            let input = input.clone();
            async move {
                tokio::time::sleep(Duration::from_millis(30)).await;
                sender.send(DynamicMessage::new(input)).await?;
                Ok(StreamTrailers::new())
            }
        })
        .unwrap();
    let (addr, task) = serve(server).await;
    let frame = encode_envelope(0, b"{}").unwrap();
    let (status, _, body) = request(
        addr,
        &frame,
        "Authorization: Bearer allowed\r\nConnect-Timeout-Ms: 1\r\n",
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(frames(&body)[0].1["error"]["code"], "deadline_exceeded");
    assert_eq!(*outcomes.lock().unwrap(), [Code::DeadlineExceeded]);
    let (_, _, body) = request(addr, &frame, "Authorization: Bearer allowed\r\n").await;
    assert_eq!(frames(&body)[0].1["error"]["code"], "internal");
    assert_eq!(
        *outcomes.lock().unwrap(),
        [Code::DeadlineExceeded, Code::Internal]
    );
    task.abort();
}
#[test]
fn request_frame_validation_rejects_truncation_multiple_messages_and_compression() {
    let c = Catalog::embedded().unwrap();
    let descriptor = c.method(SUBSCRIBE).unwrap().input();
    for raw in [
        vec![],
        vec![0, 0, 0, 0, 5, b'{'],
        encode_envelope(FLAG_END_STREAM, b"{}").unwrap().to_vec(),
        encode_envelope(FLAG_COMPRESSED, b"{}").unwrap().to_vec(),
    ] {
        assert!(decode_stream_request(descriptor.clone(), &raw, 1024).is_err())
    }
    let frame = encode_envelope(0, b"{}").unwrap();
    let mut double = frame.to_vec();
    double.extend_from_slice(&frame);
    assert!(decode_stream_request(descriptor.clone(), &double, 1024).is_err());
    assert!(decode_stream_request(descriptor, &frame, 1).is_err());
}
struct DropFlag(Arc<AtomicBool>);
impl Drop for DropFlag {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst)
    }
}
#[tokio::test]
async fn dropping_http_body_cancels_producer_and_sender_clones() {
    use axum::{body::Body, http::Request};
    let c = Catalog::embedded().unwrap();
    let output = c.method(SUBSCRIBE).unwrap().output();
    let produced = Arc::new(AtomicUsize::new(0));
    let dropped = Arc::new(AtomicBool::new(false));
    let seen = produced.clone();
    let cleanup = dropped.clone();
    let (tx, mut rx) = tokio::sync::mpsc::channel(1);
    let mut server = UnaryServer::new(c, authorizer()).with_stream_capacity(1);
    server
        .register_streaming(SUBSCRIBE, move |_, sender| {
            let output = output.clone();
            let seen = seen.clone();
            let cleanup = cleanup.clone();
            let tx = tx.clone();
            async move {
                let _guard = DropFlag(cleanup);
                tx.send(sender.clone()).await.unwrap();
                loop {
                    sender.send(DynamicMessage::new(output.clone())).await?;
                    seen.fetch_add(1, Ordering::SeqCst);
                }
            }
        })
        .unwrap();
    let router = server.router();
    use tower::ServiceExt;
    let frame = encode_envelope(0, b"{}").unwrap();
    let response = router
        .oneshot(
            Request::post(SUBSCRIBE)
                .header("Content-Type", "application/connect+json")
                .header("Authorization", "Bearer allowed")
                .body(Body::from(frame))
                .unwrap(),
        )
        .await
        .unwrap();
    let sender = rx.recv().await.unwrap();
    for _ in 0..5 {
        tokio::task::yield_now().await
    }
    assert_eq!(
        produced.load(Ordering::SeqCst),
        1,
        "bounded channel must stop the producer before body polling"
    );
    drop(response);
    tokio::time::timeout(Duration::from_secs(1), sender.cancelled())
        .await
        .unwrap();
    for _ in 0..10 {
        tokio::task::yield_now().await
    }
    assert!(dropped.load(Ordering::SeqCst));
    assert!(sender
        .send(DynamicMessage::new(sender_descriptor()))
        .await
        .is_err());
}
fn sender_descriptor() -> MessageDescriptor {
    Catalog::embedded()
        .unwrap()
        .method(SUBSCRIBE)
        .unwrap()
        .output()
}

#[tokio::test]
async fn actual_http_disconnect_releases_stream_producer() {
    let c = Catalog::embedded().unwrap();
    let output = c.method(SUBSCRIBE).unwrap().output();
    let dropped = Arc::new(AtomicBool::new(false));
    let cleanup = dropped.clone();
    let mut server = UnaryServer::new(c, authorizer());
    server
        .register_streaming(SUBSCRIBE, move |_, sender| {
            let output = output.clone();
            let cleanup = cleanup.clone();
            async move {
                let _guard = DropFlag(cleanup);
                let message = decode_json(output, br#"{"id":"first"}"#)?;
                loop {
                    sender.send(message.clone()).await?;
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            }
        })
        .unwrap();
    let (addr, task) = serve(server).await;
    let frame = encode_envelope(0, b"{}").unwrap();
    let mut client = TcpStream::connect(addr).await.unwrap();
    client.write_all(format!("POST {SUBSCRIBE} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/connect+json\r\nAuthorization: Bearer allowed\r\nContent-Length: {}\r\n\r\n",frame.len()).as_bytes()).await.unwrap();
    client.write_all(&frame).await.unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        let mut raw = Vec::new();
        let mut buffer = [0u8; 1024];
        while !raw.windows(5).any(|bytes| bytes == b"first") {
            let n = client.read(&mut buffer).await.unwrap();
            assert!(n > 0);
            raw.extend_from_slice(&buffer[..n]);
        }
    })
    .await
    .unwrap();
    drop(client);
    tokio::time::timeout(Duration::from_secs(2), async {
        while !dropped.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    task.abort();
}

#[tokio::test]
async fn deadline_cancels_a_backpressured_producer_without_body_polling() {
    use axum::{
        body::{to_bytes, Body},
        http::Request,
    };
    use tower::ServiceExt;
    let c = Catalog::embedded().unwrap();
    let output = c.method(SUBSCRIBE).unwrap().output();
    let dropped = Arc::new(AtomicBool::new(false));
    let cleanup = dropped.clone();
    let outcomes = Arc::new(Mutex::new(Vec::<Code>::new()));
    let recorded = Arc::clone(&outcomes);
    let mut server = UnaryServer::new(c, authorizer())
        .with_stream_capacity(1)
        .with_completion_observer(Arc::new(move |_, _, _, error| {
            recorded.lock().unwrap().push(error.unwrap().code);
        }));
    server
        .register_streaming(SUBSCRIBE, move |_, sender| {
            let output = output.clone();
            let cleanup = cleanup.clone();
            async move {
                let _guard = DropFlag(cleanup);
                loop {
                    sender.send(DynamicMessage::new(output.clone())).await?;
                }
            }
        })
        .unwrap();
    let response = server
        .router()
        .oneshot(
            Request::post(SUBSCRIBE)
                .header("content-type", "application/connect+json")
                .header("authorization", "Bearer allowed")
                .header("connect-timeout-ms", "20")
                .body(Body::from(encode_envelope(0, b"{}").unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(1), async {
        while !dropped.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    let body = to_bytes(response.into_body(), 4096).await.unwrap();
    assert_eq!(frames(&body)[0].1["error"]["code"], "deadline_exceeded");
    assert_eq!(*outcomes.lock().unwrap(), [Code::DeadlineExceeded]);
}

#[tokio::test]
async fn completed_stream_keeps_its_success_outcome_when_body_is_polled_late() {
    use axum::{
        body::{to_bytes, Body},
        http::Request,
    };
    use tower::ServiceExt;
    let c = Catalog::embedded().unwrap();
    let outcomes = Arc::new(Mutex::new(Vec::<Option<Code>>::new()));
    let recorded = Arc::clone(&outcomes);
    let mut server = UnaryServer::new(c, authorizer()).with_completion_observer(Arc::new(
        move |_, _, _, error| {
            recorded.lock().unwrap().push(error.map(|error| error.code));
        },
    ));
    server
        .register_streaming(
            SUBSCRIBE,
            move |_, _| async move { Ok(StreamTrailers::new()) },
        )
        .unwrap();
    let response = server
        .router()
        .oneshot(
            Request::post(SUBSCRIBE)
                .header("content-type", "application/connect+json")
                .header("authorization", "Bearer allowed")
                .header("connect-timeout-ms", "100")
                .body(Body::from(encode_envelope(0, b"{}").unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(1), async {
        while outcomes.lock().unwrap().is_empty() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_millis(110)).await;
    let body = to_bytes(response.into_body(), 4096).await.unwrap();
    assert!(frames(&body)[0].1.get("error").is_none());
    assert_eq!(*outcomes.lock().unwrap(), [None]);
}

#[tokio::test]
async fn configured_stream_limit_allows_a_file_chunk_sized_json_response() {
    use axum::{
        body::{to_bytes, Body},
        http::Request,
    };
    use tower::ServiceExt;

    let catalog = Catalog::embedded().unwrap();
    let output = catalog.method(SUBSCRIBE).unwrap().output();
    let mut server = UnaryServer::new(catalog, authorizer()).with_max_stream_message_bytes(8 << 20);
    server
        .register_streaming(SUBSCRIBE, move |_, sender| {
            let output = output.clone();
            async move {
                let payload = "x".repeat(5 << 20);
                let message = decode_json(
                    output,
                    &serde_json::to_vec(&json!({
                        "kind": "file.chunk",
                        "payload": {"data": payload}
                    }))
                    .unwrap(),
                )?;
                sender.send(message).await?;
                Ok(BTreeMap::new())
            }
        })
        .unwrap();

    let response = server
        .router()
        .oneshot(
            Request::post(SUBSCRIBE)
                .header("content-type", "application/connect+json")
                .header("authorization", "Bearer allowed")
                .body(Body::from(encode_envelope(0, b"{}").unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(response.into_body(), 8 << 20).await.unwrap();
    let events = frames(&body);
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].0, 0);
    assert_eq!(
        events[0].1["payload"]["data"].as_str().unwrap().len(),
        5 << 20
    );
    assert_eq!(events[1].0, FLAG_END_STREAM);
    assert!(events[1].1.get("error").is_none());
}
