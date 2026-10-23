#![cfg(unix)]

use protectorate_hub::Hub;
use protectorate_inventory::{Node, CARE};
use protectorate_node::{Agent, Jobs, MetricsCollector};
use protectorate_node_metrics::NodeMetrics;
use std::{
    io,
    process::{Command, Output},
    sync::Arc,
};
use tokio::net::TcpListener;

struct Metrics;
impl MetricsCollector for Metrics {
    fn collect(&self) -> io::Result<NodeMetrics> {
        Ok(NodeMetrics::default())
    }
}
async fn serve(router: axum::Router) -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (url, task)
}
fn cli(url: &str, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_protectorate-cli"))
        .env("PROTECTORATE_HUB", url)
        .env("PROTECTORATE_TOKEN", "owner-secret")
        .args(args)
        .output()
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn job_run_and_logs_stream_through_real_hub_and_node() {
    let root = tempfile::tempdir().unwrap();
    let jobs = Jobs::new(root.path().join("node-jobs"), 2)
        .unwrap()
        .with_plain_mode();
    let node = Agent::new(Arc::new(Metrics)).with_jobs(jobs);
    let (node_url, node_task) = serve(node.router().unwrap()).await;
    let hub = Hub::open(root.path().join("hub"), "owner-secret", "test").unwrap();
    hub.inventory
        .add(
            Node {
                name: "peer".into(),
                address: node_url.trim_start_matches("http://").into(),
                tier: CARE.into(),
                ..Node::default()
            },
            false,
        )
        .unwrap();
    let (url, hub_task) = serve(hub.router().unwrap()).await;

    let url2 = url.clone();
    let success = tokio::task::spawn_blocking(move || {
        cli(
            &url2,
            &[
                "job",
                "run",
                "--on",
                "peer",
                "--timeout",
                "0",
                "--",
                "/bin/sh",
                "-c",
                "printf 'hello\\000world'",
            ],
        )
    })
    .await
    .unwrap();
    assert!(
        success.status.success(),
        "{}",
        String::from_utf8_lossy(&success.stderr)
    );
    assert_eq!(success.stdout, b"hello\0world");
    let stderr = String::from_utf8_lossy(&success.stderr);
    assert!(stderr.contains("succeeded"), "{stderr}");
    let id = stderr
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .to_string();

    let url2 = url.clone();
    let id2 = id.clone();
    let logs = tokio::task::spawn_blocking(move || cli(&url2, &["job", "logs", &id2]))
        .await
        .unwrap();
    assert!(
        logs.status.success(),
        "{}",
        String::from_utf8_lossy(&logs.stderr)
    );
    assert_eq!(logs.stdout, b"hello\0world");

    let url2 = url.clone();
    let id2 = id.clone();
    let followed =
        tokio::task::spawn_blocking(move || cli(&url2, &["job", "logs", "--follow", &id2]))
            .await
            .unwrap();
    assert!(
        followed.status.success(),
        "{}",
        String::from_utf8_lossy(&followed.stderr)
    );
    assert_eq!(followed.stdout, b"hello\0world");

    let url2 = url.clone();
    let failed = tokio::task::spawn_blocking(move || {
        cli(
            &url2,
            &[
                "job",
                "run",
                "--on",
                "peer",
                "--json",
                "--",
                "/bin/sh",
                "-c",
                "printf bad; exit 7",
            ],
        )
    })
    .await
    .unwrap();
    assert_eq!(
        failed.status.code(),
        Some(7),
        "{}",
        String::from_utf8_lossy(&failed.stderr)
    );
    assert!(String::from_utf8_lossy(&failed.stderr).contains("bad"));
    assert!(String::from_utf8_lossy(&failed.stderr).contains("failed"));
    let final_job: serde_json::Value = serde_json::from_slice(&failed.stdout).unwrap();
    assert_eq!(final_job["state"], "JOB_STATE_FAILED");
    assert_eq!(final_job["exitCode"], 7);
    #[cfg(unix)]
    {
        let url2 = url.clone();
        let canceled = tokio::task::spawn_blocking(move || {
            let mut child = Command::new(env!("CARGO_BIN_EXE_protectorate-cli"))
                .env("PROTECTORATE_HUB", &url2)
                .env("PROTECTORATE_TOKEN", "owner-secret")
                .args([
                    "job", "run", "--on", "peer", "--", "/bin/sh", "-c", "sleep 10",
                ])
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap();
            std::thread::sleep(std::time::Duration::from_millis(300));
            // SAFETY: pid belongs to the child just spawned.
            unsafe {
                libc::kill(child.id() as i32, libc::SIGINT);
            }
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while child.try_wait().unwrap().is_none() {
                if std::time::Instant::now() >= deadline {
                    child.kill().unwrap();
                    panic!("SIGINT did not cancel the job promptly");
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            child.wait_with_output().unwrap()
        })
        .await
        .unwrap();
        assert_eq!(
            canceled.status.code(),
            Some(130),
            "{}",
            String::from_utf8_lossy(&canceled.stderr)
        );
        assert!(
            String::from_utf8_lossy(&canceled.stderr).contains("canceled"),
            "{}",
            String::from_utf8_lossy(&canceled.stderr)
        );
    }
    hub_task.abort();
    node_task.abort();
}
