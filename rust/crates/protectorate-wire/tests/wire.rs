use protectorate_wire::*;
use serde_json::{json, Value as Json};
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
const STATUS: &str = "/protectorate.v1.HubService/GetStatus";
fn auth() -> Authorizer {
    Arc::new(|method, headers| {
        if headers
            .get("authorization")
            .is_none_or(|v| v != "Bearer test-secret")
        {
            return Err(RpcError::new(
                Code::Unauthenticated,
                "bearer token required",
            ));
        }
        assert_eq!(method.tool, "hub_get_status");
        Ok(AuthContext {
            actor: "owner".into(),
            owner: true,
            ..Default::default()
        })
    })
}
async fn serve(server: UnaryServer) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let router = server.router();
    let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (addr, task)
}
async fn http(
    addr: std::net::SocketAddr,
    method: &str,
    path: &str,
    body: &str,
    headers: &str,
) -> (u16, String, Json) {
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let request=format!("{method} {path} HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\nConnection: close\r\n{headers}\r\n{body}",body.len());
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await.unwrap();
    let response = String::from_utf8(bytes).unwrap();
    let (head, body) = response.split_once("\r\n\r\n").unwrap();
    let status = head.split_whitespace().nth(1).unwrap().parse().unwrap();
    (status, head.into(), serde_json::from_str(body).unwrap())
}
#[tokio::test]
async fn peer_authorizer_receives_transport_extension_and_handler_sees_same_peer() {
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;
    let catalog = Catalog::embedded().unwrap();
    let output = catalog.method(STATUS).unwrap().output();
    let peer: std::net::SocketAddr = "192.168.2.3:45000".parse().unwrap();
    let mut server = UnaryServer::new_with_peer(
        catalog,
        Arc::new(move |_, _, got| {
            assert_eq!(got, Some(peer));
            Ok(AuthContext::default())
        }),
    );
    server
        .register(STATUS, move |request| {
            let output = output.clone();
            async move {
                assert_eq!(request.peer_addr, Some(peer));
                Ok(DynamicMessage::new(output).into())
            }
        })
        .unwrap();
    let mut request = Request::post(STATUS)
        .header("content-type", "application/json")
        .header("x-forwarded-for", "8.8.8.8")
        .body(Body::from("{}"))
        .unwrap();
    request.extensions_mut().insert(peer);
    let response = server.router().oneshot(request).await.unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);
}
#[tokio::test]
async fn real_get_status_roundtrip_matches_flutter_json_transport() {
    let catalog = Catalog::embedded().unwrap();
    let output = catalog.method(STATUS).unwrap().output();
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    let mut server = UnaryServer::new(catalog, auth());
    server.register(STATUS,move|request|{let output=output.clone();let seen=seen.clone();async move{assert!(request.auth.owner);assert_eq!(request.auth.actor,"owner");assert_eq!(request.message.descriptor().full_name(),"protectorate.v1.GetStatusRequest");seen.fetch_add(1,Ordering::SeqCst);let message=decode_json(output,br#"{"version":"v1-rust","hostname":"hub","os":"linux","arch":"arm64","uptimeSeconds":"9007199254740993","startedAt":"2026-10-22T00:00:00.123456789Z"}"#)?;Ok(message.into())}}).unwrap();
    let (addr, task) = serve(server).await;
    let(status,headers,body)=http(addr,"POST",STATUS,"{}","Content-Type: application/json\r\nConnect-Protocol-Version: 1\r\nAuthorization: Bearer test-secret\r\n").await;
    assert_eq!(status, 200);
    assert!(headers
        .to_lowercase()
        .contains("content-type: application/json"));
    assert_eq!(body["version"], "v1-rust");
    assert_eq!(body["uptimeSeconds"], "9007199254740993");
    assert_eq!(body["startedAt"], "2026-10-22T00:00:00.123456789Z");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let (status, _, body) = http(
        addr,
        "POST",
        STATUS,
        "{\"unknown\":true}",
        "Content-Type: application/json\r\n",
    )
    .await;
    assert_eq!(status, 401);
    assert_eq!(body["code"], "unauthenticated");
    let (status, _, body) = http(
        addr,
        "POST",
        STATUS,
        "{\"unknown\":true}",
        "Content-Type: application/json\r\nAuthorization: Bearer test-secret\r\n",
    )
    .await;
    assert_eq!(status, 400);
    assert_eq!(body["code"], "invalid_argument");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    task.abort();
}
#[test]
fn descriptor_catalog_preserves_auth_and_streaming_metadata() {
    let c = Catalog::embedded().unwrap();
    let m = c.method(STATUS).unwrap();
    assert!(m.read_only);
    assert!(!m.owner_only);
    assert!(m.is_unary());
    let token = c
        .method("/protectorate.v1.TokenService/RevokeToken")
        .unwrap();
    assert!(token.owner_only);
    assert!(token.destructive);
    let update = c
        .method("/protectorate.v1.ReleaseService/UpdateHub")
        .unwrap();
    assert!(update.owner_only);
    assert!(update.device_allowed);
    assert!(update.server_streaming);
    assert!(c.methods().any(|m| m.node_local));
    assert!(c.hub_methods().all(|m| !m.node_local));
    assert!(c.hub_methods().count() > 50);
    let mut server = UnaryServer::new(c, auth());
    assert!(server
        .register("/protectorate.v1.ReleaseService/UpdateHub", |_| async {
            unreachable!()
        })
        .is_err());
}

#[test]
fn source_descriptor_exposes_proto_tool_descriptions() {
    let catalog = Catalog::with_source_info().unwrap();
    let method = catalog
        .method("/protectorate.v1.HubService/GetSleeveRuntime")
        .unwrap();
    assert!(method.description().contains("detected sandbox runtime"));
    assert_eq!(method.tool, "hub_get_sleeve_runtime");
}
#[test]
fn strict_codec_enforces_nested_schema_oneofs_and_json_completeness() {
    let c = Catalog::embedded().unwrap();
    let empty = c.message("protectorate.v1.GetStatusRequest").unwrap();
    for bytes in [&b""[..], b" ", b"null", b"[]", b"{} {}", b"{\"typo\":true}"] {
        assert!(decode_json(empty.clone(), bytes).is_err(), "{:?}", bytes)
    }
    let status = c.message("protectorate.v1.GetStatusResponse").unwrap();
    assert!(decode_json(
        status.clone(),
        br#"{"uptimeSeconds":"1","uptime_seconds":"2"}"#
    )
    .is_err());
    assert!(decode_json(
        status.clone(),
        br#"{"uptimeSeconds":"9223372036854775808"}"#
    )
    .is_err());
    let m = decode_json(
        status,
        br#"{"uptime_seconds":"42","started_at":"2026-10-22T00:00:00Z"}"#,
    )
    .unwrap();
    let out: Json = serde_json::from_slice(&encode_json(&m).unwrap()).unwrap();
    assert_eq!(out["uptimeSeconds"], "42");
    assert_eq!(out["startedAt"], "2026-10-22T00:00:00Z");
    let request = c.message("protectorate.v1.AcquireLeaseRequest").unwrap();
    assert!(decode_json(
        request.clone(),
        br#"{"resource":"node","selector":{"os":"linux"}}"#
    )
    .is_err());
    assert!(decode_json(request.clone(), br#"{"selector":{"unknown":1}}"#).is_err());
    let m = decode_json(
        request,
        br#"{"selector":{"gpuVramBytes":"18446744073709551615"},"ttl":"1.500s"}"#,
    )
    .unwrap();
    let out: Json = serde_json::from_slice(&encode_json(&m).unwrap()).unwrap();
    assert_eq!(out["selector"]["gpuVramBytes"], "18446744073709551615");
    assert_eq!(out["ttl"], "1.500s");
    let lease = c.message("protectorate.v1.Lease").unwrap();
    let m = decode_json(lease, br#"{"state":"LEASE_STATE_QUEUED","gpuIndex":0}"#).unwrap();
    let out: Json = serde_json::from_slice(&encode_json(&m).unwrap()).unwrap();
    assert_eq!(out["state"], "LEASE_STATE_QUEUED");
    assert_eq!(out["gpuIndex"], 0);
}
#[tokio::test]
async fn unary_transport_rejects_protocol_errors_and_enforces_limits() {
    let c = Catalog::embedded().unwrap();
    let output = c.method(STATUS).unwrap().output();
    let mut server = UnaryServer::new(c, auth()).with_max_request_bytes(16);
    server
        .register(STATUS, move |_| {
            let output = output.clone();
            async move { Ok(DynamicMessage::new(output).into()) }
        })
        .unwrap();
    let (addr, task) = serve(server).await;
    let valid = "Authorization: Bearer test-secret\r\nContent-Type: application/json\r\n";
    let (status, _, body) = http(addr, "GET", STATUS, "", valid).await;
    assert_eq!(status, 405);
    assert_eq!(body["code"], "unimplemented");
    let (status, _, _) = http(addr, "POST", "/missing", "{}", valid).await;
    assert_eq!(status, 404);
    let (status, _, _) = http(
        addr,
        "POST",
        STATUS,
        "{}",
        "Content-Type: text/plain\r\nAuthorization: Bearer test-secret\r\n",
    )
    .await;
    assert_eq!(status, 415);
    let (status, _, body) = http(addr, "POST", STATUS, "{                   }", valid).await;
    assert_eq!(status, 429);
    assert_eq!(body["code"], "resource_exhausted");
    let (status, _, body) = http(
        addr,
        "POST",
        STATUS,
        "{}",
        &format!("{valid}Connect-Protocol-Version: 2\r\n"),
    )
    .await;
    assert_eq!(status, 400);
    assert_eq!(body["code"], "invalid_argument");
    let (status, _, _) = http(
        addr,
        "POST",
        STATUS,
        "{}",
        &format!("{valid}Connect-Timeout-Ms: -1\r\n"),
    )
    .await;
    assert_eq!(status, 400);
    task.abort();
}
#[tokio::test]
async fn deadlines_and_wrong_response_types_become_connect_errors() {
    let c = Catalog::embedded().unwrap();
    let input = c.method(STATUS).unwrap().input();
    let completions = Arc::new(std::sync::Mutex::new(Vec::<Option<Code>>::new()));
    let seen = Arc::clone(&completions);
    let mut server =
        UnaryServer::new(c, auth()).with_completion_observer(Arc::new(move |_, _, _, error| {
            seen.lock().unwrap().push(error.map(|error| error.code));
        }));
    server
        .register(STATUS, move |_| {
            let input = input.clone();
            async move {
                tokio::time::sleep(Duration::from_millis(30)).await;
                Ok(DynamicMessage::new(input).into())
            }
        })
        .unwrap();
    let (addr, task) = serve(server).await;
    let valid = "Authorization: Bearer test-secret\r\nContent-Type: application/json\r\n";
    let (status, _, body) = http(
        addr,
        "POST",
        STATUS,
        "{}",
        &format!("{valid}Connect-Timeout-Ms: 1\r\n"),
    )
    .await;
    assert_eq!(status, 504);
    assert_eq!(body["code"], "deadline_exceeded");
    assert_eq!(*completions.lock().unwrap(), vec![Some(Code::Canceled)]);
    let (status, _, body) = http(addr, "POST", STATUS, "{}", valid).await;
    assert_eq!(status, 500);
    assert_eq!(body["code"], "internal");
    assert_eq!(
        *completions.lock().unwrap(),
        vec![Some(Code::Canceled), Some(Code::Internal)]
    );
    task.abort();
}
#[test]
fn connect_codes_have_canonical_http_mappings() {
    assert_eq!(Code::FailedPrecondition.http_status().as_u16(), 400);
    assert_eq!(Code::Unauthenticated.http_status().as_u16(), 401);
    assert_eq!(Code::PermissionDenied.http_status().as_u16(), 403);
    assert_eq!(Code::ResourceExhausted.http_status().as_u16(), 429);
    assert_eq!(
        serde_json::to_value(RpcError::new(Code::AlreadyExists, "exists")).unwrap(),
        json!({"code":"already_exists","message":"exists"})
    );
}
