#![cfg(target_os = "linux")]
//! Removing a node with uninstall unregisters its startup first and only then
//! stops the agent, exactly once, even when the service manager fails.
use protectorate_connect_client::Client;
use protectorate_hub::Hub;
use protectorate_inventory::Node;
use protectorate_node::{Agent, MetricsCollector, UninstallPlan};
use protectorate_node_metrics::NodeMetrics;
use protectorate_wire::Code;
use serde_json::{json, Value};
use std::{
    fs, io,
    os::unix::fs::PermissionsExt,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
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

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    tokio::task::spawn_blocking(f).await.unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn uninstall_disables_startup_before_stopping_the_agent_once() {
    for exit in [0, 3] {
        let root = tempfile::tempdir().unwrap();
        let calls = root.path().join("systemctl-calls");
        let systemctl = root.path().join("systemctl");
        fs::write(
            &systemctl,
            format!(
                "#!/bin/sh\nsleep 0.3\nprintf '%s\\n' \"$*\" >> '{}'\nexit {exit}\n",
                calls.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&systemctl, fs::Permissions::from_mode(0o755)).unwrap();
        let plan = UninstallPlan::new(
            root.path().join("node-state"),
            root.path().join("user"),
            root.path().join("node-bin"),
            true,
        )
        .unwrap();
        let stops = Arc::new(Mutex::new(Vec::<String>::new()));
        let seen = Arc::clone(&stops);
        let recorded = calls.clone();
        let node = Agent::new(Arc::new(Metrics)).with_uninstall(
            plan,
            &systemctl,
            Arc::new(move || {
                seen.lock()
                    .unwrap()
                    .push(fs::read_to_string(&recorded).unwrap_or_default());
            }),
        );
        let (node_url, node_task) = serve(node.router().unwrap()).await;
        let hub = Hub::open(root.path().join("hub"), "owner-secret", "test").unwrap();
        hub.inventory
            .add(
                Node {
                    name: "peer".into(),
                    address: node_url.trim_start_matches("http://").into(),
                    ..Node::default()
                },
                false,
            )
            .unwrap();
        let monitor = Arc::clone(&hub.node_monitor);
        blocking(move || monitor.poll_node("peer")).await;
        let (hub_url, hub_task) = serve(hub.router().unwrap()).await;
        let url = hub_url.clone();
        let phases = blocking(move || {
            let mut phases = Vec::<Value>::new();
            Client::new(&url, "owner-secret", Duration::from_secs(30))
                .unwrap()
                .stream(
                    "/protectorate.v1.NodeService/RemoveNode",
                    &json!({"name":"peer","uninstall":true}),
                    |event| {
                        phases.push(event);
                        Ok(())
                    },
                )
                .unwrap();
            phases
        })
        .await;
        let order = phases
            .iter()
            .map(|event| event["phase"].as_str().unwrap_or_default().to_owned())
            .filter(|phase| phase == "REMOVE_PHASE_UNINSTALLING" || phase == "REMOVE_PHASE_REMOVED")
            .collect::<Vec<_>>();
        assert_eq!(
            order,
            ["REMOVE_PHASE_UNINSTALLING", "REMOVE_PHASE_REMOVED"],
            "{phases:?}"
        );
        let again = blocking(move || {
            Client::new(&node_url, "owner-secret", Duration::from_secs(10))
                .unwrap()
                .call("/protectorate.v1.NodeAgentService/Uninstall", &json!({}))
                .unwrap()
        })
        .await;
        assert_eq!(
            again["actions"],
            json!(["disable the systemd user unit", "stop the agent"])
        );
        let deadline = Instant::now() + Duration::from_secs(30);
        while stops.lock().unwrap().is_empty() {
            assert!(
                Instant::now() < deadline,
                "agent never stopped (exit {exit})"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        tokio::time::sleep(Duration::from_millis(1500)).await;
        let stops = stops.lock().unwrap().clone();
        assert_eq!(
            stops,
            ["--user disable protectorate-node.service\n"],
            "exit {exit}"
        );
        assert_eq!(fs::read_to_string(&calls).unwrap(), stops[0]);
        let gone = blocking(move || {
            Client::new(&hub_url, "owner-secret", Duration::from_secs(10))
                .unwrap()
                .call(
                    "/protectorate.v1.NodeService/GetNode",
                    &json!({"name":"peer"}),
                )
                .unwrap_err()
        })
        .await;
        assert_eq!(gone.code, Code::NotFound, "{gone:?}");
        hub_task.abort();
        node_task.abort();
    }
}
