#![cfg(unix)]

use protectorate_cli::{dispatch_endpoint, execute, parse, Client, Code, Command};
use protectorate_hub::Hub;
use protectorate_wire::{decode_json, AuthContext, Authorizer, Catalog, RpcError, UnaryServer};
use serde_json::{json, Value};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::net::TcpListener;

#[tokio::test]
async fn events_streams_hub_envelopes_in_text_and_json() {
    let catalog = Catalog::embedded().unwrap();
    let output = catalog
        .method("/protectorate.v1.EventService/Subscribe")
        .unwrap()
        .output();
    let auth: Authorizer = Arc::new(|_, headers| {
        (headers
            .get("authorization")
            .is_some_and(|value| value == "Bearer owner-secret"))
        .then_some(AuthContext::default())
        .ok_or_else(|| RpcError::new(Code::Unauthenticated, "bad token"))
    });
    let mut server = UnaryServer::new(catalog, auth);
    server.register_streaming("/protectorate.v1.EventService/Subscribe", move |_, sender| {
        let output = output.clone();
        async move {
            let event = json!({"id":"4","time":"2026-10-22T12:00:00Z","kind":"job.updated","subject":"job-4","payload":{"state":"running"}});
            sender.send(decode_json(output, event.to_string().as_bytes())?).await?;
            Ok(Default::default())
        }
    }).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move { axum::serve(listener, server.router()).await.unwrap() });
    let (human, machine) = tokio::task::spawn_blocking(move || {
        let client = Client::new(&url, "owner-secret", Duration::from_secs(5)).unwrap();
        let human = execute(&client, &parse(["events"]).unwrap()).unwrap();
        let machine = execute(&client, &parse(["--json", "events"]).unwrap()).unwrap();
        (human, machine)
    })
    .await
    .unwrap();
    assert!(
        human.contains("\tjob.updated\tjob-4\t{\"state\":\"running\"}"),
        "{human}"
    );
    let event: Value = serde_json::from_str(machine.trim()).unwrap();
    assert_eq!(event["kind"], "job.updated");
    assert_eq!(event["payload"]["state"], "running");
    task.abort();
}

#[tokio::test]
async fn watch_streams_and_respond_sends_approval_to_connect_server() {
    let catalog = Catalog::embedded().unwrap();
    let watch_output = catalog
        .method("/protectorate.v1.ChatService/Watch")
        .unwrap()
        .output();
    let auth: Authorizer = Arc::new(|_, headers| {
        (headers
            .get("authorization")
            .is_some_and(|value| value == "Bearer owner-secret"))
        .then_some(AuthContext::default())
        .ok_or_else(|| RpcError::new(Code::Unauthenticated, "bad token"))
    });
    let calls = Arc::new(Mutex::new(Vec::<Value>::new()));
    let mut server = UnaryServer::new(catalog, auth);
    let watch_calls = Arc::clone(&calls);
    server.register_streaming("/protectorate.v1.ChatService/Watch", move |request, sender| {
        let calls = Arc::clone(&watch_calls);
        let output = watch_output.clone();
        async move {
            calls.lock().unwrap().push(serde_json::to_value(&request.message).unwrap());
            for event in [
                json!({"seq":"8","time":"2026-10-22T12:00:00Z","user":{"from":"owner","text":"Review it"}}),
                json!({"seq":"9","time":"2026-10-22T12:00:01Z","text":{"messageId":"m1","delta":"Looks "}}),
                json!({"seq":"9","time":"2026-10-22T12:00:01Z","text":{"messageId":"m1","delta":"good."}}),
            ] {
                sender.send(decode_json(output.clone(), event.to_string().as_bytes())?).await?;
            }
            Ok(Default::default())
        }
    }).unwrap();
    let respond_calls = Arc::clone(&calls);
    server
        .register("/protectorate.v1.ChatService/Respond", move |request| {
            let calls = Arc::clone(&respond_calls);
            async move {
                calls
                    .lock()
                    .unwrap()
                    .push(serde_json::to_value(&request.message).unwrap());
                Ok(decode_json(request.method.output(), b"{}")?.into())
            }
        })
        .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move { axum::serve(listener, server.router()).await.unwrap() });
    let test_url = url.clone();
    let (human, json_events, answer) = tokio::task::spawn_blocking(move || {
        let client = Client::new(&test_url, "owner-secret", Duration::from_secs(5)).unwrap();
        let human = execute(
            &client,
            &parse(["chat", "watch", "worker", "--after", "7"]).unwrap(),
        )
        .unwrap();
        let json_events = execute(
            &client,
            &parse(["--json", "chat", "watch", "worker", "--after=7"]).unwrap(),
        )
        .unwrap();
        let answer = dispatch_endpoint(
            &test_url,
            "owner-secret",
            [
                "chat",
                "respond",
                "worker",
                "approval-1",
                "--deny",
                "--note",
                "No access",
            ],
        )
        .unwrap();
        (human, json_events, answer)
    })
    .await
    .unwrap();
    assert!(human.contains("owner: Review it"), "{human}");
    assert!(human.contains("assistant Looks good."), "{human}");
    assert_eq!(json_events.lines().count(), 3);
    assert_eq!(answer, "answered\n");
    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 3);
    assert_eq!(calls[0]["afterSeq"], "7");
    assert_eq!(calls[0]["sleeve"], "worker");
    assert_eq!(calls[2]["approvalId"], "approval-1");
    assert!(calls[2]["allow"].is_null()); // Proto JSON omits false.
    assert_eq!(calls[2]["note"], "No access");
    task.abort();
}

#[test]
fn chat_control_arguments_require_one_approval_decision() {
    assert!(matches!(
        parse(["chat", "watch", "worker"]).unwrap().command,
        Command::ChatWatch { after: 0, .. }
    ));
    assert!(parse(["chat", "respond", "worker", "approval-1"]).is_err());
    assert!(parse([
        "chat",
        "respond",
        "worker",
        "approval-1",
        "--allow",
        "--deny"
    ])
    .is_err());
    assert!(parse(["chat", "watch", "worker", "--after", "invalid"]).is_err());
}

#[tokio::test]
async fn respond_and_watch_validate_a_sleeve_on_real_hub() {
    let root = tempfile::tempdir().unwrap();
    let hub = Hub::open(root.path(), "owner-secret", "test").unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task =
        tokio::spawn(async move { axum::serve(listener, hub.router().unwrap()).await.unwrap() });
    let (respond, watch, interrupt, compact, models) = tokio::task::spawn_blocking(move || {
        let respond = dispatch_endpoint(
            &url,
            "owner-secret",
            ["chat", "respond", "missing", "approval-1", "--allow"],
        )
        .unwrap_err();
        let client = Client::new(&url, "owner-secret", Duration::from_secs(5)).unwrap();
        let watch = execute(&client, &parse(["chat", "watch", "missing"]).unwrap()).unwrap_err();
        let interrupt =
            dispatch_endpoint(&url, "owner-secret", ["chat", "interrupt", "missing"]).unwrap_err();
        let compact =
            dispatch_endpoint(&url, "owner-secret", ["chat", "compact", "missing"]).unwrap_err();
        let models = dispatch_endpoint(
            &url,
            "owner-secret",
            ["chat", "models", "--harness", "codex"],
        )
        .unwrap_err();
        (respond, watch, interrupt, compact, models)
    })
    .await
    .unwrap();
    assert_eq!(respond.code, Code::NotFound);
    assert_eq!(watch.code, Code::NotFound);
    assert_eq!(interrupt.code, Code::Unavailable);
    assert_eq!(compact.code, Code::NotFound);
    assert_eq!(models.code, Code::Unavailable);
    task.abort();
}

#[tokio::test]
async fn interrupt_compact_and_models_dispatch_over_connect_http() {
    let auth: Authorizer = Arc::new(|_, headers| {
        (headers
            .get("authorization")
            .is_some_and(|value| value == "Bearer owner-secret"))
        .then_some(AuthContext::default())
        .ok_or_else(|| RpcError::new(Code::Unauthenticated, "bad token"))
    });
    let calls = Arc::new(Mutex::new(Vec::<(String, Value)>::new()));
    let mut server = UnaryServer::new(Catalog::embedded().unwrap(), auth);
    for (method, output) in [
        ("Interrupt", json!({})),
        (
            "Compact",
            json!({"compaction":{"tokensBefore":"900","tokensAfter":"90"}}),
        ),
        (
            "ListModels",
            json!({"supported":true,"models":[{"model":"gpt-6","name":"GPT 6","efforts":["low","high"],"defaultEffort":"high"}]}),
        ),
    ] {
        let path = format!("/protectorate.v1.ChatService/{method}");
        let observed = Arc::clone(&calls);
        server
            .register(&path, move |request| {
                let observed = Arc::clone(&observed);
                let output = output.clone();
                let method = method.to_owned();
                async move {
                    observed
                        .lock()
                        .unwrap()
                        .push((method, serde_json::to_value(&request.message).unwrap()));
                    Ok(decode_json(request.method.output(), output.to_string().as_bytes())?.into())
                }
            })
            .unwrap();
    }
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move { axum::serve(listener, server.router()).await.unwrap() });
    let target = url.clone();
    let outputs = tokio::task::spawn_blocking(move || {
        let call = |args: &[&str]| dispatch_endpoint(&target, "owner-secret", args).unwrap();
        (
            call(&["chat", "interrupt", "worker"]),
            call(&["chat", "compact", "worker"]),
            call(&["chat", "models", "worker", "--harness", "pi"]),
            call(&["--json", "chat", "models", "--harness=codex"]),
        )
    })
    .await
    .unwrap();
    assert_eq!(outputs.0, "interrupted\n");
    assert_eq!(outputs.1, "compacted\t900 -> 90 tokens\n");
    assert_eq!(
        outputs.2,
        "gpt-6\tGPT 6\tefforts=[low high]\tdefault effort=high\n"
    );
    assert_eq!(
        serde_json::from_str::<Value>(&outputs.3).unwrap()["supported"],
        true
    );
    let observed = calls.lock().unwrap();
    assert_eq!(
        observed
            .iter()
            .map(|(method, _)| method.as_str())
            .collect::<Vec<_>>(),
        ["Interrupt", "Compact", "ListModels", "ListModels"]
    );
    assert_eq!(observed[0].1["sleeve"], "worker");
    assert_eq!(observed[1].1["sleeve"], "worker");
    assert_eq!(observed[2].1["harness"], "pi");
    assert_eq!(observed[3].1["harness"], "codex");
    assert!(observed[3].1["sleeve"].is_null());
    task.abort();
}

#[test]
fn chat_model_and_control_arguments_are_checked() {
    for args in [
        vec!["chat", "interrupt"],
        vec!["chat", "compact"],
        vec!["chat", "models", "one", "two"],
        vec!["chat", "models", "--harness"],
        vec!["chat", "models", "--harness="],
    ] {
        assert!(parse(args).is_err());
    }
    assert!(
        matches!(parse(["chat", "models", "--harness", "pi", "worker"]).unwrap().command,
        Command::ChatModels { sleeve: Some(name), harness } if name == "worker" && harness == "pi")
    );
}
