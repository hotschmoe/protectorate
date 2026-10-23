#![cfg(target_os = "linux")]
use protectorate_connect_client::Client;
use protectorate_node::{Agent, MetricsCollector, UninstallPlan};
use protectorate_node_metrics::NodeMetrics;
use std::{
    fs, io,
    os::unix::fs::PermissionsExt,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

struct Metrics;
impl MetricsCollector for Metrics {
    fn collect(&self) -> io::Result<NodeMetrics> {
        Ok(NodeMetrics::default())
    }
}

#[tokio::test]
async fn uninstall_responds_before_disabling_and_stopping() {
    let dir = tempfile::tempdir().unwrap();
    let user = dir.path().join("user");
    let home = dir.path().join("state");
    let exe = dir.path().join("protectorate-rs");
    let unit = user.join(".config/systemd/user/protectorate-node.service");
    fs::create_dir_all(unit.parent().unwrap()).unwrap();
    fs::create_dir_all(&home).unwrap();
    fs::write(&unit, b"unit").unwrap();
    fs::write(&exe, b"binary").unwrap();
    let log = dir.path().join("argv");
    let fake = dir.path().join("systemctl");
    fs::write(
        &fake,
        format!("#!/bin/sh\nprintf '%s\\n' \"$*\" > '{}'\n", log.display()),
    )
    .unwrap();
    fs::set_permissions(&fake, fs::Permissions::from_mode(0o700)).unwrap();
    let stopped = Arc::new(AtomicBool::new(false));
    let stop = Arc::clone(&stopped);
    let plan = UninstallPlan::new(&home, &user, &exe, true).unwrap();
    let agent = Agent::new(Arc::new(Metrics)).with_uninstall(
        plan,
        fake,
        Arc::new(move || stop.store(true, Ordering::SeqCst)),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(listener, agent.router().unwrap())
            .await
            .unwrap()
    });
    assert!(!log.exists());
    let reply = tokio::task::spawn_blocking(move || {
        Client::new(&endpoint, "node-secret", Duration::from_secs(2))
            .unwrap()
            .call(
                "/protectorate.v1.NodeAgentService/Uninstall",
                &serde_json::json!({}),
            )
            .unwrap()
    })
    .await
    .unwrap();
    assert_eq!(reply["actions"].as_array().unwrap().len(), 2);
    assert_eq!(reply["left"].as_array().unwrap().len(), 3);
    assert!(
        !log.exists(),
        "systemctl must wait until after the RPC response"
    );
    assert!(!stopped.load(Ordering::SeqCst));
    tokio::time::timeout(Duration::from_secs(3), async {
        while !stopped.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        fs::read_to_string(&log).unwrap(),
        "--user disable protectorate-node.service\n"
    );
    assert_eq!(fs::read(&unit).unwrap(), b"unit");
    assert_eq!(fs::read(&exe).unwrap(), b"binary");
    assert!(home.is_dir());
    server.abort();
}
