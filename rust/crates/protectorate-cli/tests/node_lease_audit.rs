#![cfg(unix)]

//! The owner's node, lease and audit commands against a real hub admitting
//! leases on a live in-process node agent.
mod common;

use common::cli;
use protectorate_hub::Hub;
use protectorate_node::{Agent, MetricsCollector, UpdateRestart};
use protectorate_node_metrics::NodeMetrics;
use serde_json::Value;
use std::{fs, io, os::unix::fs::PermissionsExt, path::Path, sync::Arc, time::Duration};
use tokio::net::TcpListener;

const OWNER: &str = "owner-secret";

struct Metrics;
impl MetricsCollector for Metrics {
    fn collect(&self) -> io::Result<NodeMetrics> {
        Ok(NodeMetrics {
            hostname: "peer".into(),
            os: std::env::consts::OS.into(),
            arch: std::env::consts::ARCH.into(),
            agent_version: "v1".into(),
            ..NodeMetrics::default()
        })
    }
}

fn arch() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        other => other,
    }
}

fn node_binary(path: &Path) {
    let version = format!(
        r#"{{"version":"v1","os":"{}","arch":"{}"}}"#,
        std::env::consts::OS,
        arch()
    );
    fs::write(path, format!("#!/bin/sh\nprintf '%s\\n' '{version}'\n")).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

async fn serve(router: axum::Router) -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (url, task)
}

fn field<'a>(text: &'a str, label: &str) -> &'a str {
    text.lines()
        .find_map(|line| line.strip_prefix(label))
        .unwrap_or_else(|| panic!("no {label:?} in {text}"))
        .trim()
}

#[tokio::test(flavor = "multi_thread")]
async fn node_lease_and_audit_commands_print_what_the_hub_did() {
    let root = tempfile::tempdir().unwrap();
    let exe = root.path().join("node-exe");
    node_binary(&exe);
    let restart: UpdateRestart = Arc::new(|| {});
    let agent = Agent::new(Arc::new(Metrics))
        .with_files(root.path().join("node-files"), false)
        .unwrap()
        .with_update(&exe, "v1", restart)
        .unwrap();
    let (node_url, node_task) = serve(agent.router().unwrap()).await;
    let bins = root.path().join("bins");
    fs::create_dir(&bins).unwrap();
    node_binary(&bins.join(format!(
        "protectorate-rs-{}-{}",
        std::env::consts::OS,
        arch()
    )));
    let hub = Hub::open(root.path().join("hub"), OWNER, "v1")
        .unwrap()
        .with_node_binary_dir(&bins)
        .with_node_update_timing(Duration::from_secs(5), Duration::from_millis(20));
    let (url, hub_task) = serve(hub.router().unwrap()).await;
    let address = node_url.trim_start_matches("http://").to_owned();

    let result = tokio::task::spawn_blocking(move || {
        let ok = |args: &[&str]| cli(&url, OWNER, args).unwrap_or_else(|e| panic!("{args:?}: {e}"));
        let fails = |args: &[&str]| cli(&url, OWNER, args).expect_err(&format!("{args:?}"));

        assert_eq!(
            ok(&["node", "add", "peer", "--addr", &address]),
            format!("added peer ({address}, tier shared)\n")
        );
        assert!(fails(&["node", "add", "peer", "--addr", "elsewhere:1"]).contains("exists"));
        assert_eq!(
            ok(&["node", "add", "peer", "--addr", &address, "--update", "--notes", "gpu box"]),
            format!("added peer ({address}, tier shared)\n")
        );
        assert!(ok(&["node", "show", "peer"]).contains("gpu box"));
        let pool = fails(&["node", "invite", "spare", "--pool", "gpu"]);
        assert!(pool.contains("--pool"), "{pool}");

        let held = ok(&[
            "lease", "acquire", "--node", "peer", "--holder", "first", "--ttl", "10m",
        ]);
        assert_eq!(field(&held, "resource"), "peer", "{held}");
        assert_eq!(field(&held, "holder"), "first", "{held}");
        let first = field(&held, "lease").to_owned();
        let busy = fails(&["lease", "acquire", "--node", "peer", "--holder", "other"]);
        assert!(busy.contains("held by first"), "{busy}");
        let mut tickets = Vec::new();
        for (holder, priority, position) in [("low", "0", "1"), ("high", "5", "1")] {
            let queued = ok(&[
                "lease",
                "acquire",
                "--node",
                "peer",
                "--holder",
                holder,
                "--wait",
                "--priority",
                priority,
            ]);
            assert_eq!(field(&queued, "position"), position, "{queued}");
            tickets.push(field(&queued, "ticket").to_owned());
        }
        let listing = ok(&["lease", "ls"]);
        let rows: Vec<Vec<&str>> = listing
            .lines()
            .skip(1)
            .map(|line| line.split_whitespace().collect())
            .collect();
        assert_eq!(rows.len(), 3, "{listing}");
        assert_eq!(
            &rows[0][..3],
            [first.as_str(), "peer", "first"],
            "{listing}"
        );
        assert!(
            rows[0][3] == "10m0s" || rows[0][3].starts_with("9m"),
            "{listing}"
        );
        assert_eq!(rows[1][2..], ["high", "queued", "#1"], "{listing}");
        assert_eq!(rows[2][2..], ["low", "queued", "#2"], "{listing}");

        assert_eq!(
            ok(&["lease", "release", &first]),
            format!("released {first}\n")
        );
        let granted = ok(&["lease", "ls"]);
        let rows: Vec<Vec<&str>> = granted
            .lines()
            .skip(1)
            .map(|line| line.split_whitespace().collect())
            .collect();
        // The head of the queue is granted a fresh lease it has two minutes to claim.
        assert_eq!(rows[0][1..3], ["peer", "high"], "{granted}");
        assert!(rows[0][0].starts_with("lease-"), "{granted}");
        assert!(matches!(rows[0][3], "2m0s" | "1m59s"), "{granted}");
        assert_eq!(
            rows[1][..],
            [tickets[0].as_str(), "peer", "low", "queued", "#1"]
        );
        let claim = rows[0][0].to_owned();
        let partial = fails(&["lease", "release", &claim, "no-such-lease"]);
        assert!(partial.contains("no-such-lease: "), "{partial}");
        let next = ok(&["lease", "ls"]);
        let low: Vec<&str> = next.lines().nth(1).unwrap().split_whitespace().collect();
        assert!(low[0].starts_with("lease-") && low[2] == "low", "{next}");
        assert_eq!(
            ok(&["lease", "release", low[0]]),
            format!("released {}\n", low[0])
        );
        assert_eq!(ok(&["lease", "ls"]).lines().count(), 1);

        assert_eq!(ok(&["node", "update", "peer"]), "peer\talready at v1\n");
        assert_eq!(ok(&["node", "update", "--all"]), "peer\talready at v1\n");
        assert!(fails(&["node", "update", "ghost"]).contains("no node \"ghost\""));

        assert_eq!(
            ok(&["node", "tier", "peer", "care"]),
            "peer is now tier care\n"
        );
        let care = fails(&["lease", "acquire", "--node", "peer", "--holder", "late"]);
        assert!(care.contains("not available for leases"), "{care}");

        let phases: Vec<String> = ok(&["node", "rm", "peer"])
            .lines()
            .map(|line| line.split('\t').next().unwrap().to_owned())
            .collect();
        assert_eq!(phases, ["draining", "revoking", "removed"]);
        assert!(!ok(&["node", "ls"]).contains("peer"));

        let total: usize = serde_json::from_str::<Value>(&ok(&["--json", "audit", "verify"]))
            .unwrap()["entries"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();
        assert!(total > 10, "{total}");
        let mut seqs = Vec::new();
        let mut token = String::new();
        loop {
            let mut args = vec!["--json", "audit", "ls", "--lines", "4"];
            if !token.is_empty() {
                args.extend(["--page-token", &token]);
            }
            let page: Value = serde_json::from_str(&ok(&args)).unwrap();
            let entries = page["entries"].as_array().unwrap();
            assert!(entries.len() <= 4);
            seqs.extend(
                entries
                    .iter()
                    .map(|e| e["seq"].as_str().unwrap().parse::<usize>().unwrap()),
            );
            token = page["nextPageToken"]
                .as_str()
                .unwrap_or_default()
                .to_owned();
            if token.is_empty() {
                break;
            }
        }
        assert_eq!(seqs, (1..=total).rev().collect::<Vec<_>>());
        let text = ok(&["audit", "ls", "--lines", "4"]);
        assert_eq!(text.lines().count(), 5, "{text}");
        assert!(
            text.lines().last().unwrap().starts_with("next page: "),
            "{text}"
        );
        let newest: Vec<&str> = text.lines().take(4).collect();
        let tail = ok(&["audit", "tail", "-n", "4"]);
        assert_eq!(tail.lines().rev().collect::<Vec<_>>(), newest, "{tail}");
        let failures = ok(&[
            "audit",
            "ls",
            "--procedure",
            "LeaseService/AcquireLease",
            "--outcome",
            "error",
        ]);
        assert_eq!(failures.lines().count(), 2, "{failures}");
    })
    .await;
    hub_task.abort();
    node_task.abort();
    result.unwrap();
}
