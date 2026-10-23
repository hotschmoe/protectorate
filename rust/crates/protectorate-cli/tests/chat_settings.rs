#![cfg(unix)]

//! `chat settings` over real Connect HTTP: a schema-checked server for the
//! read and write shapes, and the real hub for the storage-backed read.
use protectorate_cli::{dispatch_endpoint, parse, transport_timeout, ChatSettings, Command};
use protectorate_hub::Hub;
use protectorate_sleeve_registry::{Harness, Mode, Record};
use protectorate_wire::{
    decode_json, AuthContext, Authorizer, Catalog, Code, RpcError, UnaryServer,
};
use serde_json::{json, Value};
use std::{path::Path, sync::Arc, sync::Mutex, time::Duration};
use tokio::net::TcpListener;

const OWNER: &str = "owner-secret";

fn auth() -> Authorizer {
    Arc::new(|_, headers| {
        (headers
            .get("authorization")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value == format!("Bearer {OWNER}") || value == OWNER))
        .then_some(AuthContext::default())
        .ok_or_else(|| RpcError::new(Code::Unauthenticated, "bad token"))
    })
}

fn chat_status(model: &str, effort: &str) -> Value {
    json!({
        "state": "CHAT_STATE_IDLE",
        "harnessSessionId": "thread-1",
        "model": model,
        "reasoningEffort": effort,
        "contextTokens": "14372",
        "contextWindow": "475000",
        "lastSeq": "9",
        "provider": "openai",
        "settings": {
            "requested": {"model": model, "effort": effort},
            "effective": {"model": model, "effort": effort},
            "nativeDefaults": {"model": "gpt-5.1-codex-mini", "effort": "medium"},
        },
    })
}

/// GetSleeve and SetSelection served from the real proto schema, recording
/// every request body the CLI sends.
#[tokio::test]
async fn chat_settings_reads_and_writes_selection_over_connect_http() {
    let calls = Arc::new(Mutex::new(Vec::<(String, Value)>::new()));
    let mut server = UnaryServer::new(Catalog::embedded().unwrap(), auth());
    for (path, method, output) in [
        (
            "/protectorate.v1.SleeveService/GetSleeve",
            "GetSleeve",
            json!({"sleeve": {"name": "worker", "harness": "HARNESS_CODEX",
                "mode": "SLEEVE_MODE_CHAT", "state": "SLEEVE_STATE_RUNNING",
                "workspace": "/srv/project", "session": "sleeve-worker",
                "createdAt": "2026-10-22T12:00:00Z",
                "chat": chat_status("gpt-5.2-codex", "high")}}),
        ),
        (
            "/protectorate.v1.ChatService/SetSelection",
            "SetSelection",
            json!({"chat": chat_status("gpt-alt", "low")}),
        ),
    ] {
        let observed = Arc::clone(&calls);
        server
            .register(path, move |request| {
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
        let call = |args: &[&str]| dispatch_endpoint(&target, OWNER, args).unwrap();
        (
            call(&["chat", "settings", "worker"]),
            call(&["--json", "chat", "settings", "worker"]),
            call(&[
                "chat", "settings", "worker", "--model", "gpt-alt", "--effort", "low",
            ]),
            call(&["chat", "settings", "worker", "--native-defaults"]),
            call(&["--json", "chat", "settings", "worker", "--effort=low"]),
        )
    })
    .await
    .unwrap();
    assert_eq!(
        outputs.0,
        "model\tgpt-5.2-codex\neffort\thigh\nrequested model\tgpt-5.2-codex\n\
         requested effort\thigh\nnative model\tgpt-5.1-codex-mini\nnative effort\tmedium\n",
        "{}",
        outputs.0
    );
    let read_json: Value = serde_json::from_str(&outputs.1).unwrap();
    assert_eq!(read_json["settings"]["effective"]["model"], "gpt-5.2-codex");
    assert_eq!(
        outputs.2,
        "model\tgpt-alt\neffort\tlow\nrequested model\tgpt-alt\nrequested effort\tlow\n\
         native model\tgpt-5.1-codex-mini\nnative effort\tmedium\n",
        "{}",
        outputs.2
    );
    assert_eq!(outputs.3, outputs.2);
    let write_json: Value = serde_json::from_str(&outputs.4).unwrap();
    assert_eq!(write_json["reasoningEffort"], "low");
    let observed = calls.lock().unwrap();
    assert_eq!(
        observed
            .iter()
            .map(|(method, _)| method.as_str())
            .collect::<Vec<_>>(),
        [
            "GetSleeve",
            "GetSleeve",
            "SetSelection",
            "SetSelection",
            "SetSelection"
        ]
    );
    assert_eq!(observed[0].1, json!({"name": "worker"}));
    assert_eq!(
        observed[2].1,
        json!({"sleeve": "worker", "selection": {"model": "gpt-alt", "effort": "low"}})
    );
    // --native-defaults sends an empty selection: presence is what restores
    // the captured defaults.
    assert_eq!(observed[3].1, json!({"sleeve": "worker", "selection": {}}));
    assert_eq!(
        observed[4].1,
        json!({"sleeve": "worker", "selection": {"effort": "low"}})
    );
    task.abort();
}

/// The read path against the real hub's own sleeve records, and the write path
/// against a hub whose chat runtime is not running.
#[tokio::test]
async fn chat_settings_dispatches_against_the_real_hub() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("project");
    std::fs::create_dir_all(&workspace).unwrap();
    let hub = Hub::open(root.path(), OWNER, "test").unwrap();
    hub.sleeves
        .record_spawned(
            Record::new(
                "worker",
                Harness::Codex,
                Mode::Chat,
                Path::new(&workspace),
                "2026-10-22T12:00:00Z",
            )
            .unwrap(),
        )
        .unwrap();
    hub.sleeves
        .set_chat("worker", chat_status("gpt-5.2-codex", "high"))
        .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task =
        tokio::spawn(async move { axum::serve(listener, hub.router().unwrap()).await.unwrap() });
    let target = url.clone();
    let (text, machine, write, missing) = tokio::task::spawn_blocking(move || {
        let text = dispatch_endpoint(&target, OWNER, ["chat", "settings", "worker"]).unwrap();
        let machine =
            dispatch_endpoint(&target, OWNER, ["--json", "chat", "settings", "worker"]).unwrap();
        let write = dispatch_endpoint(
            &target,
            OWNER,
            [
                "chat", "settings", "worker", "--model", "gpt-alt", "--effort", "low",
            ],
        )
        .unwrap_err();
        let missing = dispatch_endpoint(&target, OWNER, ["chat", "settings", "gone"]).unwrap_err();
        (text, machine, write, missing)
    })
    .await
    .unwrap();
    assert_eq!(
        text,
        "model\tgpt-5.2-codex\neffort\thigh\nrequested model\tgpt-5.2-codex\n\
         requested effort\thigh\nnative model\tgpt-5.1-codex-mini\nnative effort\tmedium\n",
        "{text}"
    );
    let status: Value = serde_json::from_str(&machine).unwrap();
    assert_eq!(
        status["settings"]["nativeDefaults"]["model"],
        "gpt-5.1-codex-mini"
    );
    // The hub has no live Codex session to update, and the CLI passes that
    // through instead of pretending the selection was applied.
    assert_eq!(write.code, Code::Unavailable);
    assert_eq!(missing.code, Code::NotFound);
    task.abort();
}

#[test]
fn chat_settings_parses_read_and_write_forms() {
    let read = parse(["chat", "settings", "worker"]).unwrap();
    assert_eq!(
        read.command,
        Command::ChatSettings(ChatSettings {
            sleeve: "worker".into(),
            model: None,
            effort: None,
            native_defaults: false,
        })
    );
    assert_eq!(transport_timeout(&read), Duration::from_secs(15));

    let written = parse([
        "chat", "settings", "worker", "--model", "gpt-alt", "--effort", "low", "--json",
    ])
    .unwrap();
    assert!(written.json);
    assert_eq!(
        written.command,
        Command::ChatSettings(ChatSettings {
            sleeve: "worker".into(),
            model: Some("gpt-alt".into()),
            effort: Some("low".into()),
            native_defaults: false,
        })
    );
    assert!(transport_timeout(&written) >= Duration::from_secs(120));

    let defaults = parse(["chat", "settings", "worker", "--native-defaults"]).unwrap();
    assert_eq!(
        defaults.command,
        Command::ChatSettings(ChatSettings {
            sleeve: "worker".into(),
            model: None,
            effort: None,
            native_defaults: true,
        })
    );
    assert!(transport_timeout(&defaults) >= Duration::from_secs(120));

    // An explicit empty value asks the harness for that field's own default,
    // the way the Go CLI reads --model="".
    let cleared = parse(["chat", "settings", "worker", "--model=", "--effort", ""]).unwrap();
    assert_eq!(
        cleared.command,
        Command::ChatSettings(ChatSettings {
            sleeve: "worker".into(),
            model: Some(String::new()),
            effort: Some(String::new()),
            native_defaults: false,
        })
    );

    let off = parse(["chat", "settings", "worker", "--native-defaults=false"]).unwrap();
    assert_eq!(
        off.command,
        Command::ChatSettings(ChatSettings {
            sleeve: "worker".into(),
            model: None,
            effort: None,
            native_defaults: false,
        })
    );
}

#[test]
fn chat_settings_arguments_are_checked_before_the_hub_sees_them() {
    let long_model = "m".repeat(257);
    let long_effort = "e".repeat(65);
    for args in [
        vec!["chat", "settings"],
        vec!["chat", "settings", "--json"],
        vec!["chat", "settings", "worker", "extra"],
        vec!["chat", "settings", "worker", "--bogus"],
        vec!["chat", "settings", "worker", "--model"],
        vec!["chat", "settings", "worker", "--effort"],
        vec!["chat", "settings", "worker", "--model", long_model.as_str()],
        vec![
            "chat",
            "settings",
            "worker",
            "--effort",
            long_effort.as_str(),
        ],
        vec!["chat", "settings", "worker", "--model", "gpt\nalt"],
        vec![
            "chat",
            "settings",
            "worker",
            "--native-defaults",
            "--model",
            "m",
        ],
        vec![
            "chat",
            "settings",
            "worker",
            "--native-defaults",
            "--effort=x",
        ],
        vec!["chat", "settings", "worker", "--native-defaults=maybe"],
    ] {
        let given = args.clone();
        let error = parse(args).unwrap_err();
        assert_eq!(error.code, Code::InvalidArgument, "{given:?}");
        assert!(error.message.contains("Usage:"), "{given:?}");
    }
}
