use protectorate_connect_client::Client;
use protectorate_wire::{
    decode_json, AuthContext, Authorizer, Catalog, Code, RpcError, RpcResponse, UnaryServer,
};
use serde_json::json;
use std::{sync::Arc, time::Duration};

#[test]
fn endpoint_only_allows_tls_or_numeric_loopback_http() {
    for bad in [
        "http://example.com:80",
        "http://127.0.0.1:80.evil.test",
        "http://127.0.0.1:0",
        "http://[::1]:65536",
        "https://user@host",
        "https://host/#frag",
    ] {
        assert!(
            Client::new(bad, "token", Duration::from_secs(1)).is_err(),
            "{bad}"
        );
    }
    assert!(Client::new("http://127.0.0.1:18080", "token", Duration::from_secs(1)).is_ok());
}

#[tokio::test]
async fn unary_and_stream_interoperate_with_real_wire_server() {
    let catalog = Catalog::embedded().unwrap();
    let status_method = catalog
        .method("/protectorate.v1.HubService/GetStatus")
        .unwrap()
        .clone();
    let event_method = catalog
        .method("/protectorate.v1.EventService/Subscribe")
        .unwrap()
        .clone();
    let auth: Authorizer = Arc::new(|_, headers| {
        if headers
            .get("authorization")
            .is_none_or(|v| v != "Bearer secret")
        {
            return Err(RpcError::new(Code::Unauthenticated, "bad bearer"));
        }
        Ok(AuthContext::default())
    });
    let mut server = UnaryServer::new(catalog, auth);
    server
        .register("/protectorate.v1.HubService/GetStatus", move |_| {
            let output = status_method.output();
            async move {
                let mut response: RpcResponse = decode_json(output, br#"{}"#)?.into();
                response
                    .headers
                    .insert("Protectorate-Desk-Lease", "lease-123".parse().unwrap());
                response.headers.insert(
                    "Protectorate-Desk-Lease-Expires",
                    "2026-10-23T00:00:00Z".parse().unwrap(),
                );
                Ok(response)
            }
        })
        .unwrap();
    server
        .register("/protectorate.v1.NodeAgentService/GetMetrics", |_| async {
            let mut error = RpcError::new(Code::FailedPrecondition, "desktop is locked");
            error.headers.insert(
                "Protectorate-Desk-Unavailable",
                "DESK_REASON_LOCKED".parse().unwrap(),
            );
            error
                .headers
                .insert("X-Private-Header", "not-forwarded".parse().unwrap());
            Err(error)
        })
        .unwrap();
    server
        .register_streaming(
            "/protectorate.v1.EventService/Subscribe",
            move |_, sender| {
                let output = event_method.output();
                async move {
                    sender
                        .send(decode_json(output, br#"{"id":"1","kind":"notice"}"#)?)
                        .await?;
                    Err(RpcError::new(Code::Unavailable, "stream stopped"))
                }
            },
        )
        .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let task = tokio::spawn(async move { axum::serve(listener, server.router()).await.unwrap() });
    let base = format!("http://{addr}");
    let result = tokio::task::spawn_blocking(move || {
        let client = Client::new(&base, "secret", Duration::from_secs(3)).unwrap();
        assert_eq!(
            client
                .call("/protectorate.v1.HubService/GetStatus", &json!({}))
                .unwrap(),
            json!({})
        );
        let (message, metadata) = client
            .call_with_metadata("/protectorate.v1.HubService/GetStatus", &json!({}))
            .unwrap();
        assert_eq!(message, json!({}));
        assert_eq!(metadata.desk_lease_id.as_deref(), Some("lease-123"));
        assert_eq!(
            metadata.desk_lease_expires.as_deref(),
            Some("2026-10-23T00:00:00Z")
        );
        let desk_refusal = client
            .call("/protectorate.v1.NodeAgentService/GetMetrics", &json!({}))
            .unwrap_err();
        assert_eq!(desk_refusal.code, Code::FailedPrecondition);
        assert_eq!(
            desk_refusal.headers["Protectorate-Desk-Unavailable"],
            "DESK_REASON_LOCKED"
        );
        assert!(!desk_refusal.headers.contains_key("X-Private-Header"));
        let mut events = Vec::new();
        let err = client
            .stream(
                "/protectorate.v1.EventService/Subscribe",
                &json!({}),
                |event| {
                    events.push(event);
                    Ok(())
                },
            )
            .unwrap_err();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["id"], "1");
        assert_eq!(err.code, Code::Unavailable);
        assert_eq!(err.message, "stream stopped");
        assert_eq!(
            client
                .call("/protectorate.v1.EventService/Subscribe", &json!({}))
                .unwrap_err()
                .code,
            Code::InvalidArgument
        );
        assert_eq!(
            client
                .stream("/protectorate.v1.HubService/GetStatus", &json!({}), |_| Ok(
                    ()
                ))
                .unwrap_err()
                .code,
            Code::InvalidArgument
        );
        let denied = Client::new(&base, "bad", Duration::from_secs(3)).unwrap();
        assert_eq!(
            denied
                .call("/protectorate.v1.HubService/GetStatus", &json!({}))
                .unwrap_err()
                .code,
            Code::Unauthenticated
        );
    })
    .await;
    task.abort();
    result.unwrap();
}

#[tokio::test]
async fn public_device_client_sends_no_bearer_and_keeps_endpoint_rules() {
    const START: &str = "/protectorate.v1.DeviceService/StartDeviceAuth";
    let catalog = Catalog::embedded().unwrap();
    let output = catalog.method(START).unwrap().output().clone();
    let auth: Authorizer = Arc::new(|_, headers| {
        if headers.contains_key("authorization") {
            return Err(RpcError::new(Code::Unauthenticated, "unexpected bearer"));
        }
        Ok(AuthContext::default())
    });
    let mut server = UnaryServer::new(catalog, auth);
    server
        .register(START, move |request| {
            let output = output.clone();
            async move {
                let input = serde_json::to_value(&request.message).unwrap();
                assert_eq!(input["name"], "laptop");
                assert_eq!(input["platform"], "linux/arm64");
                Ok(decode_json(
                    output,
                    br#"{"deviceCode":"private-code","userCode":"ABCD-1234","expiresAt":"2026-10-23T00:00:00Z","interval":1}"#,
                )?
                .into())
            }
        })
        .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move { axum::serve(listener, server.router()).await.unwrap() });
    let result = tokio::task::spawn_blocking(move || {
        let client = Client::new_public(&base, Duration::from_secs(3)).unwrap();
        let response = client
            .call(START, &json!({"name":"laptop","platform":"linux/arm64"}))
            .unwrap();
        assert_eq!(response["userCode"], "ABCD-1234");
        assert_eq!(
            client
                .call("/protectorate.v1.HubService/GetStatus", &json!({}))
                .unwrap_err()
                .code,
            Code::PermissionDenied
        );
    })
    .await;
    task.abort();
    result.unwrap();
    assert!(Client::new_public("http://example.com:7470", Duration::from_secs(1)).is_err());
}

#[tokio::test]
async fn per_request_timeout_overrides_default_without_losing_auth() {
    const STATUS: &str = "/protectorate.v1.HubService/GetStatus";
    const EVENTS: &str = "/protectorate.v1.EventService/Subscribe";
    let auth: Authorizer = Arc::new(|_, headers| {
        (headers
            .get("authorization")
            .is_some_and(|value| value == "Bearer secret"))
        .then_some(AuthContext::default())
        .ok_or_else(|| RpcError::new(Code::Unauthenticated, "bad token"))
    });
    let mut server = UnaryServer::new(Catalog::embedded().unwrap(), auth);
    server
        .register(STATUS, |request| async move {
            tokio::time::sleep(Duration::from_millis(200)).await;
            Ok(decode_json(request.method.output(), br#"{"version":"0.1.1"}"#)?.into())
        })
        .unwrap();
    server
        .register_streaming(EVENTS, |request, sender| async move {
            tokio::time::sleep(Duration::from_millis(200)).await;
            sender
                .send(decode_json(request.method.output(), br#"{"id":"1"}"#)?)
                .await?;
            Ok(Default::default())
        })
        .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move { axum::serve(listener, server.router()).await.unwrap() });
    let result = tokio::task::spawn_blocking(move || {
        let client = Client::new(&base, "secret", Duration::from_secs(2)).unwrap();
        assert!(client
            .call_with_timeout(STATUS, &json!({}), Duration::from_millis(50))
            .is_err());
        assert_eq!(client.call(STATUS, &json!({})).unwrap()["version"], "0.1.1");
        assert!(client
            .stream_with_timeout(EVENTS, &json!({}), Duration::from_millis(50), |_| Ok(()))
            .is_err());
        let mut events = Vec::new();
        client
            .stream(EVENTS, &json!({}), |event| {
                events.push(event);
                Ok(())
            })
            .unwrap();
        assert_eq!(events[0]["id"], "1");
    })
    .await;
    task.abort();
    result.unwrap();
}
