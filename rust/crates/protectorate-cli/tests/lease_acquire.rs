#![cfg(unix)]

use protectorate_cli::{dispatch_endpoint, parse, Command};
use protectorate_hub::Hub;
use serde_json::Value;
use tokio::net::TcpListener;

#[tokio::test(flavor = "multi_thread")]
async fn acquire_active_and_queued_leases_over_real_http() {
    let home = tempfile::tempdir().unwrap();
    let hub = Hub::open(home.path(), "owner-secret", "test").unwrap();
    let name = hub.hostname.clone();
    hub.node_monitor.poll_node(&name);
    assert!(hub.node_monitor.is_fresh(&name, "local"));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server =
        tokio::spawn(async move { axum::serve(listener, hub.router().unwrap()).await.unwrap() });
    let result = tokio::task::spawn_blocking(move || {
        let active = dispatch_endpoint(
            &url,
            "owner-secret",
            [
                "--json", "lease", "acquire", "--node", &name, "--holder", "alice", "--ttl", "2m",
            ],
        )
        .unwrap();
        let active: Value = serde_json::from_str(&active).unwrap();
        assert_eq!(active["resource"], name);
        assert_eq!(active["holder"], "alice");
        assert_eq!(active["ttl"], "120s");
        let busy = dispatch_endpoint(
            &url,
            "owner-secret",
            ["lease", "acquire", "--node", &name, "--holder", "bob"],
        );
        assert!(busy.is_err());
        let queued = dispatch_endpoint(
            &url,
            "owner-secret",
            [
                "lease",
                "acquire",
                "--node",
                &name,
                "--holder",
                "bob",
                "--wait",
                "--priority",
                "7",
            ],
        )
        .unwrap();
        let rows = queued
            .lines()
            .map(|line| line.split_whitespace().collect::<Vec<_>>())
            .collect::<Vec<_>>();
        assert_eq!(rows[0][0], "ticket");
        assert!(rows[0][1].starts_with("ticket-"));
        assert_eq!(rows[1], ["resource", name.as_str()]);
        assert_eq!(rows[2], ["holder", "bob"]);
        assert_eq!(rows[3], ["position", "1"]);
    })
    .await;
    server.abort();
    result.unwrap();
}

#[test]
fn acquire_parser_covers_selector_sizes_and_exclusivity() {
    let command = parse([
        "lease",
        "acquire",
        "--os",
        "linux",
        "--arch",
        "arm64",
        "--min-free-mem",
        "8G",
        "--gpu-vram",
        "24GiB",
        "--gpu",
        "nvidia",
        "--npu",
        "hexagon",
        "--holder",
        "sleeve:one",
        "--wait",
        "--priority",
        "-2",
    ])
    .unwrap()
    .command;
    assert!(
        matches!(command, Command::LeaseAcquire(spec) if spec.resource.is_empty() && spec.min_free_memory_bytes == 8 << 30 && spec.gpu_vram_bytes == 24 << 30 && spec.wait && spec.priority == -2)
    );
    for args in [
        vec!["lease", "acquire", "--node", "box", "--gpu", "nvidia"],
        vec!["lease", "acquire", "--ttl", "oops"],
        vec!["lease", "acquire", "--priority", "999999999999"],
        vec!["lease", "acquire", "--holder", ""],
    ] {
        assert!(parse(args).is_err());
    }
}
