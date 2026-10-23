#![cfg(unix)]

mod common;

use common::cli;
use protectorate_hub::Hub;
use serde_json::Value;
use std::{
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use tokio::net::TcpListener;

fn spawn_cli(url: &str, token: &str, args: &[&str]) -> Child {
    Command::new(env!("CARGO_BIN_EXE_protectorate-cli"))
        .env("PROTECTORATE_HUB", url)
        .env("PROTECTORATE_TOKEN", token)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap()
}

fn finish(child: Child) -> (bool, String) {
    let output = child.wait_with_output().unwrap();
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned()
            + &String::from_utf8_lossy(&output.stderr),
    )
}

/// A hub whose approval requests expire after `ttl`, with an Envoy and a
/// sleeve capability.
async fn serve(ttl: Duration) -> (String, String, String, tokio::task::JoinHandle<()>) {
    let home = tempfile::tempdir().unwrap();
    let hub = Hub::open(home.path(), "owner-secret", "test")
        .unwrap()
        .with_approval_ttl(ttl);
    // Agent identities are hub-minted capabilities, never owner-created.
    let envoy = common::mint(&hub, "orchestrator-envoy", &["backlog_*", "read"]);
    let sleeve = common::mint(&hub, "sleeve:worker", &["backlog_*", "read"]);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let _home = home;
        axum::serve(listener, hub.router().unwrap()).await.unwrap()
    });
    (url, envoy, sleeve, server)
}

fn owner(url: &str, args: &[&str]) -> String {
    cli(url, "owner-secret", args).unwrap()
}

fn json(text: String) -> Value {
    serde_json::from_str(&text).unwrap()
}

fn note(url: &str, title: &str) -> String {
    json(owner(url, &["--json", "backlog", "add", title]))["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// The newest approval request, waiting for one to appear. `all` includes
/// requests that are no longer pending.
fn first_approval(url: &str, all: bool) -> Value {
    let mut args = vec!["--json", "approvals", "ls"];
    if all {
        args.push("--all");
    }
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        let listed = json(owner(url, &args));
        if let Some(first) = listed["approvals"].as_array().and_then(|a| a.first()) {
            return first.clone();
        }
        assert!(Instant::now() < deadline, "no approval request appeared");
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn states(url: &str) -> Vec<String> {
    json(owner(url, &["--json", "approvals", "ls", "--all"]))["approvals"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["state"].as_str().unwrap().to_owned())
        .collect()
}

fn audit(url: &str) -> Vec<Value> {
    let report = json(owner(url, &["--json", "audit", "verify"]));
    assert_eq!(report["ok"], true);
    json(owner(url, &["--json", "audit", "ls", "--lines", "100"]))["entries"]
        .as_array()
        .unwrap()
        .clone()
}

fn count(entries: &[Value], procedure: &str) -> usize {
    entries
        .iter()
        .filter(|e| e["procedure"] == procedure)
        .count()
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_destructive_calls_wait_for_owner_approval_or_denial() {
    // Long enough that the owner always decides first.
    let (url, envoy, sleeve, server) = serve(Duration::from_secs(120)).await;
    let result = tokio::task::spawn_blocking(move || {
        let doomed = note(&url, "doomed");
        let call = spawn_cli(&url, &envoy, &["backlog", "rm", &doomed]);
        let request = first_approval(&url, false);
        assert_eq!(request["actor"], "token:orchestrator-envoy");
        assert_eq!(
            request["procedure"],
            "/protectorate.v1.BacklogService/DeleteNote"
        );
        assert_eq!(request["subject"], doomed.as_str());
        assert_eq!(request["reason"], "destructive");
        assert!(owner(&url, &["backlog", "show", &doomed]).contains("doomed"));
        let id = request["id"].as_str().unwrap();
        assert_eq!(
            owner(&url, &["approvals", "approve", id]),
            format!("approved {id}\n")
        );
        let (ok, output) = finish(call);
        assert!(ok, "{output}");
        assert!(
            output.contains(&format!("waiting for owner approval {id}")),
            "{output}"
        );
        assert!(cli(&url, "owner-secret", ["backlog", "show", &doomed]).is_err());

        let kept = note(&url, "kept");
        let call = spawn_cli(&url, &sleeve, &["backlog", "rm", &kept]);
        let request = first_approval(&url, false);
        assert_eq!(request["actor"], "token:sleeve:worker");
        let id = request["id"].as_str().unwrap();
        owner(&url, &["approvals", "deny", id]);
        let (ok, output) = finish(call);
        assert!(!ok, "{output}");
        assert!(
            output.contains(&format!("owner denied approval {id}")),
            "{output}"
        );

        // Asking again would wait out the TTL and end expired instead.
        let call = spawn_cli(&url, &sleeve, &["backlog", "rm", &kept]);
        let (ok, output) = finish(call);
        assert!(!ok, "{output}");
        assert!(
            output.contains("denied approval"),
            "a denied call asked again: {output}"
        );
        assert!(owner(&url, &["backlog", "show", &kept]).contains("kept"));
        owner(&url, &["backlog", "rm", &kept]);

        assert_eq!(
            states(&url),
            ["APPROVAL_STATE_USED", "APPROVAL_STATE_DENIED"]
        );
        let entries = audit(&url);
        assert_eq!(count(&entries, "approval.request"), 2, "{entries:?}");
        let decisions = entries
            .iter()
            .filter(|e| e["procedure"] == "/protectorate.v1.ApprovalService/DecideApproval")
            .map(|e| e["detail"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        assert_eq!(decisions.len(), 2, "{entries:?}");
        assert!(
            decisions[0].starts_with("denied destructive"),
            "{decisions:?}"
        );
        assert!(
            decisions[1].starts_with("approved destructive"),
            "{decisions:?}"
        );
    })
    .await;
    server.abort();
    result.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn an_undecided_approval_expires_and_the_call_fails() {
    let (url, _envoy, sleeve, server) = serve(Duration::from_secs(3)).await;
    let result = tokio::task::spawn_blocking(move || {
        let other = note(&url, "other");
        let call = spawn_cli(&url, &sleeve, &["backlog", "rm", &other]);
        let id = first_approval(&url, true)["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let (ok, output) = finish(call);
        assert!(!ok, "{output}");
        assert!(
            output.contains(&format!("approval {id} expired")),
            "{output}"
        );
        assert!(owner(&url, &["backlog", "show", &other]).contains("other"));
        assert_eq!(states(&url), ["APPROVAL_STATE_EXPIRED"]);
        let entries = audit(&url);
        assert_eq!(count(&entries, "approval.request"), 1, "{entries:?}");
        assert_eq!(count(&entries, "approval.expired"), 1, "{entries:?}");
    })
    .await;
    server.abort();
    result.unwrap();
}
