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

#[tokio::test(flavor = "multi_thread")]
async fn agent_destructive_calls_wait_for_owner_approval_deny_or_expiry() {
    let home = tempfile::tempdir().unwrap();
    let hub = Hub::open(home.path(), "owner-secret", "test")
        .unwrap()
        .with_approval_ttl(Duration::from_secs(5));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server =
        tokio::spawn(async move { axum::serve(listener, hub.router().unwrap()).await.unwrap() });
    let result = tokio::task::spawn_blocking(move || {
        let owner = |args: &[&str]| cli(&url, "owner-secret", args).unwrap();
        let json = |text: String| serde_json::from_str::<Value>(&text).unwrap();
        let token = |name: &str| {
            let issued = json(owner(&[
                "--json",
                "token",
                "create",
                name,
                "--scope",
                "backlog_*",
                "--scope",
                "read",
            ]));
            issued["secret"].as_str().unwrap().to_owned()
        };
        let envoy = token("orchestrator-envoy");
        let sleeve = token("sleeve:worker");
        let note = |title: &str| {
            json(owner(&["--json", "backlog", "add", title]))["id"]
                .as_str()
                .unwrap()
                .to_owned()
        };
        let pending = || {
            let deadline = Instant::now() + Duration::from_secs(60);
            loop {
                let listed = json(owner(&["--json", "approvals", "ls"]));
                if let Some(first) = listed["approvals"].as_array().and_then(|a| a.first()) {
                    return first.clone();
                }
                assert!(Instant::now() < deadline, "no approval request appeared");
                std::thread::sleep(Duration::from_millis(100));
            }
        };

        let doomed = note("doomed");
        let call = spawn_cli(&url, &envoy, &["backlog", "rm", &doomed]);
        let request = pending();
        assert_eq!(request["actor"], "token:orchestrator-envoy");
        assert_eq!(
            request["procedure"],
            "/protectorate.v1.BacklogService/DeleteNote"
        );
        assert_eq!(request["subject"], doomed.as_str());
        assert_eq!(request["reason"], "destructive");
        assert!(owner(&["backlog", "show", &doomed]).contains("doomed"));
        let id = request["id"].as_str().unwrap();
        assert_eq!(
            owner(&["approvals", "approve", id]),
            format!("approved {id}\n")
        );
        let (ok, output) = finish(call);
        assert!(ok, "{output}");
        assert!(
            output.contains(&format!("waiting for owner approval {id}")),
            "{output}"
        );
        assert!(cli(&url, "owner-secret", ["backlog", "show", &doomed]).is_err());

        let kept = note("kept");
        let call = spawn_cli(&url, &sleeve, &["backlog", "rm", &kept]);
        let request = pending();
        assert_eq!(request["actor"], "token:sleeve:worker");
        let id = request["id"].as_str().unwrap();
        owner(&["approvals", "deny", id]);
        let (ok, output) = finish(call);
        assert!(!ok, "{output}");
        assert!(
            output.contains(&format!("owner denied approval {id}")),
            "{output}"
        );

        let call = spawn_cli(&url, &sleeve, &["backlog", "rm", &kept]);
        let started = Instant::now();
        let (ok, output) = finish(call);
        assert!(!ok, "{output}");
        assert!(
            output.contains("denied approval"),
            "a denied call asked again: {output}"
        );
        assert!(started.elapsed() < Duration::from_secs(5));

        let other = note("other");
        let call = spawn_cli(&url, &sleeve, &["backlog", "rm", &other]);
        let id = pending()["id"].as_str().unwrap().to_owned();
        let (ok, output) = finish(call);
        assert!(!ok, "{output}");
        assert!(
            output.contains(&format!("approval {id} expired")),
            "{output}"
        );
        assert!(owner(&["backlog", "show", &kept]).contains("kept"));
        assert!(owner(&["backlog", "show", &other]).contains("other"));

        owner(&["backlog", "rm", &kept]);
        let all = json(owner(&["--json", "approvals", "ls", "--all"]));
        let states = all["approvals"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a["state"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        assert_eq!(
            states,
            [
                "APPROVAL_STATE_USED",
                "APPROVAL_STATE_DENIED",
                "APPROVAL_STATE_EXPIRED"
            ]
        );
        let audit = json(owner(&["--json", "audit", "ls", "--lines", "100"]));
        let entries = audit["entries"].as_array().unwrap();
        let count = |procedure: &str| {
            entries
                .iter()
                .filter(|e| e["procedure"] == procedure)
                .count()
        };
        assert_eq!(count("approval.request"), 3, "{audit}");
        assert_eq!(count("approval.expired"), 1, "{audit}");
        let decisions = entries
            .iter()
            .filter(|e| e["procedure"] == "/protectorate.v1.ApprovalService/DecideApproval")
            .map(|e| e["detail"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        assert_eq!(decisions.len(), 2, "{audit}");
        assert!(
            decisions[0].starts_with("denied destructive"),
            "{decisions:?}"
        );
        assert!(
            decisions[1].starts_with("approved destructive"),
            "{decisions:?}"
        );
        let report = json(owner(&["--json", "audit", "verify"]));
        assert_eq!(report["ok"], true);
    })
    .await;
    server.abort();
    result.unwrap();
}
