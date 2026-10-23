#![cfg(unix)]

use protectorate_hub::Hub;
use protectorate_inventory::{Node, CARE};
use protectorate_node::{Agent, Jobs, MetricsCollector};
use protectorate_node_metrics::NodeMetrics;
use std::{fs, io, process::Command as Process, sync::Arc};
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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cross_test_compiles_and_executes_a_real_rust_test_on_a_node() {
    let root = tempfile::tempdir().unwrap();
    let crate_dir = root.path().join("fixture");
    fs::create_dir_all(crate_dir.join("src")).unwrap();
    fs::write(
        crate_dir.join("Cargo.toml"),
        "[package]\nname = \"remote_fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    fs::write(
        crate_dir.join("src/lib.rs"),
        "#[test] fn runs_on_node() { println!(\"remote-rust-test-ran\"); }\n",
    )
    .unwrap();
    for args in [
        vec!["init", "--quiet"],
        vec!["add", "Cargo.toml", "src/lib.rs"],
        vec![
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "commit",
            "--quiet",
            "-m",
            "fixture",
        ],
    ] {
        assert!(Process::new("git")
            .current_dir(&crate_dir)
            .args(args)
            .status()
            .unwrap()
            .success());
    }
    let jobs = Jobs::new(root.path().join("node-jobs"), 1)
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
    let results = tokio::task::spawn_blocking(move || {
        let call = |source: &str| {
            Process::new(env!("CARGO_BIN_EXE_protectorate"))
                .env("PROTECTORATE_HUB", &url)
                .env("PROTECTORATE_TOKEN", "owner-secret")
                .args([
                    "job",
                    "test-cross",
                    "--on",
                    "peer",
                    source,
                    crate_dir.to_str().unwrap(),
                    "--",
                    "--offline",
                    "--",
                    "--nocapture",
                ])
                .output()
                .unwrap()
        };
        [call("--artifact"), call("--git")]
    })
    .await
    .unwrap();
    for result in results {
        assert!(
            result.status.success(),
            "stdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        let output = String::from_utf8_lossy(&result.stdout);
        assert!(output.contains("remote-rust-test-ran"), "{output}");
        assert!(
            String::from_utf8_lossy(&result.stderr).contains("job "),
            "stderr:\n{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    hub_task.abort();
    node_task.abort();
}
