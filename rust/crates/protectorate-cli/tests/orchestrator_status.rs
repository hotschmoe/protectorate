use protectorate_cli::{dispatch_endpoint, parse, Command};
use protectorate_wire::{
    decode_json, AuthContext, Authorizer, Catalog, Code, RpcError, UnaryServer,
};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;

#[test]
fn orchestrator_commands_parse_and_reject_extra_arguments() {
    assert_eq!(
        parse(["orchestrator", "status"]).unwrap().command,
        Command::OrchestratorStatus
    );
    assert!(matches!(
        parse(["orchestrator", "spawn", "--harness", "pi"])
            .unwrap()
            .command,
        Command::SleeveEnvoy(_)
    ));
    assert!(parse(["orchestrator", "status", "extra"]).is_err());
    assert_eq!(
        parse([
            "orchestrator",
            "scopes",
            "--add",
            "read,events",
            "--remove",
            "lease_release",
            "--add=message_send"
        ])
        .unwrap()
        .command,
        Command::OrchestratorScopes {
            add: vec!["read".into(), "events".into(), "message_send".into()],
            remove: vec!["lease_release".into()],
        }
    );
    for args in [
        vec!["orchestrator", "scopes"],
        vec!["orchestrator", "scopes", "--add", ""],
        vec!["orchestrator", "scopes", "--add", "read", "extra"],
    ] {
        assert!(parse(&args).is_err(), "accepted {args:?}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn status_reads_envoy_and_selects_newest_token() {
    let auth: Authorizer = Arc::new(|_, headers| {
        headers
            .get("authorization")
            .is_some_and(|value| value == "Bearer owner-secret")
            .then_some(AuthContext::default())
            .ok_or_else(|| RpcError::new(Code::Unauthenticated, "bad token"))
    });
    let mut server = UnaryServer::new(Catalog::embedded().unwrap(), auth);
    let calls = Arc::new(Mutex::new(Vec::<String>::new()));
    let get_calls = Arc::clone(&calls);
    server
        .register("/protectorate.v1.SleeveService/GetSleeve", move |request| {
            let calls = Arc::clone(&get_calls);
            async move {
                let input = serde_json::to_value(&request.message).unwrap();
                assert_eq!(input["name"], "envoy");
                calls.lock().unwrap().push("sleeve".into());
                Ok(decode_json(
                    request.method.output(),
                    json!({"sleeve":{"name":"envoy","harness":"HARNESS_PI","state":"SLEEVE_STATE_RUNNING","workspace":"/tmp/envoy","createdAt":"2026-01-02T15:04:05Z"}})
                        .to_string()
                        .as_bytes(),
                )?
                .into())
            }
        })
        .unwrap();
    let list_calls = Arc::clone(&calls);
    server
        .register("/protectorate.v1.TokenService/ListTokens", move |request| {
            let calls = Arc::clone(&list_calls);
            async move {
                calls.lock().unwrap().push("tokens".into());
                Ok(decode_json(
                    request.method.output(),
                    json!({"tokens":[
                        {"id":"old","name":"orchestrator-envoy","createdAt":"2026-01-01T00:00:00Z","scopes":["read"]},
                        {"id":"other","name":"other","createdAt":"2026-01-04T00:00:00Z"},
                        {"id":"new","name":"orchestrator-envoy","createdAt":"2026-01-02T00:00:00Z","scopes":["read","events"]}
                    ]})
                    .to_string()
                    .as_bytes(),
                )?
                .into())
            }
        })
        .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server_task =
        tokio::spawn(async move { axum::serve(listener, server.router()).await.unwrap() });
    tokio::task::spawn_blocking(move || {
        let raw =
            dispatch_endpoint(&url, "owner-secret", ["orchestrator", "status", "--json"]).unwrap();
        let status: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(status["sleeve"]["name"], "envoy");
        assert_eq!(status["token"]["id"], "new");
        assert!(status.get("other").is_none());
        let text = dispatch_endpoint(&url, "owner-secret", ["orchestrator", "status"]).unwrap();
        assert!(text.contains("sleeve\tenvoy"), "{text}");
        assert!(text.contains("token\tnew (active)"), "{text}");
        assert!(text.contains("scopes\tread, events"), "{text}");
        assert_eq!(
            calls.lock().unwrap().as_slice(),
            ["sleeve", "tokens", "sleeve", "tokens"]
        );
    })
    .await
    .unwrap();
    server_task.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn status_reports_uncreated_envoy_without_masking_other_errors() {
    let auth: Authorizer = Arc::new(|_, _| Ok(AuthContext::default()));
    let mut server = UnaryServer::new(Catalog::embedded().unwrap(), auth);
    server
        .register("/protectorate.v1.SleeveService/GetSleeve", |_| async {
            Err(RpcError::new(Code::NotFound, "no envoy"))
        })
        .unwrap();
    server
        .register(
            "/protectorate.v1.TokenService/ListTokens",
            |request| async move {
                Ok(decode_json(request.method.output(), br#"{"tokens":[]}"#)?.into())
            },
        )
        .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server_task =
        tokio::spawn(async move { axum::serve(listener, server.router()).await.unwrap() });
    tokio::task::spawn_blocking(move || {
        let text = dispatch_endpoint(&url, "owner-secret", ["orchestrator", "status"]).unwrap();
        assert!(text.contains("sleeve\tnot created"), "{text}");
        assert!(text.contains("token\tnone"), "{text}");
        let raw =
            dispatch_endpoint(&url, "owner-secret", ["orchestrator", "status", "--json"]).unwrap();
        assert_eq!(serde_json::from_str::<Value>(&raw).unwrap(), json!({}));
    })
    .await
    .unwrap();
    server_task.abort();
}
