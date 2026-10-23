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
    Command::new(env!("CARGO_BIN_EXE_protectorate"))
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
    let canceled = tokio::task::spawn_blocking(move || {
        use std::io::{BufRead, BufReader, Read};
        let mut child = Command::new(env!("CARGO_BIN_EXE_protectorate"))
            .env("PROTECTORATE_HUB", &url)
            .env("PROTECTORATE_TOKEN", "owner-secret")
            .args([
                "job",
                "run",
                "--on",
                "peer",
                "--",
                "/bin/sh",
                "-c",
                "sleep 600",
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        // The CLI names the job once it is submitted, after its interrupt
        // handler is installed.
        let mut stderr = BufReader::new(child.stderr.take().unwrap());
        let mut submitted = String::new();
        stderr.read_line(&mut submitted).unwrap();
        assert!(submitted.starts_with("job "), "{submitted}");
        // SAFETY: pid belongs to the child just spawned.
        unsafe {
            libc::kill(child.id() as i32, libc::SIGINT);
        }
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut rest = String::new();
            let _ = stderr.read_to_string(&mut rest);
            let _ = sender.send(rest);
        });
        let rest = receiver
            .recv_timeout(std::time::Duration::from_secs(120))
            .unwrap_or_else(|_| {
                let _ = child.kill();
                panic!("SIGINT did not cancel the job");
            });
        (child.wait().unwrap(), submitted + &rest)
    })
    .await
    .unwrap();
    assert_eq!(canceled.0.code(), Some(130), "{}", canceled.1);
    assert!(canceled.1.contains("canceled"), "{}", canceled.1);
    hub_task.abort();
    node_task.abort();
}
