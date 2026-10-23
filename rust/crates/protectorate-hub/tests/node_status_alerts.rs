use protectorate_connect_client::Client;
use protectorate_hub::Hub;
use protectorate_inventory::Node;
use protectorate_node::{Agent, Jobs, MetricsCollector};
use protectorate_node_metrics::{FilesystemMetrics, MemoryMetrics, NodeMetrics};
use protectorate_wire::Code;
use serde_json::{json, Value};
use std::{io, sync::Arc, time::Duration};
use tokio::net::TcpListener;

const GET_NODE: &str = "/protectorate.v1.NodeService/GetNode";
const LIST_NODES: &str = "/protectorate.v1.NodeService/ListNodes";
const REMOVE_NODE: &str = "/protectorate.v1.NodeService/RemoveNode";
const SUBMIT: &str = "/protectorate.v1.JobService/SubmitJob";

// One filesystem over the threshold, one exactly at it, one just under, and
// memory under pressure.
struct Pressure;
impl MetricsCollector for Pressure {
    fn collect(&self) -> io::Result<NodeMetrics> {
        Ok(NodeMetrics {
            hostname: "peer".into(),
            os: "linux".into(),
            arch: "x86_64".into(),
            agent_version: "test".into(),
            memory: Some(MemoryMetrics {
                total_bytes: 1000,
                used_bytes: 914,
                available_bytes: 86,
                used_percent: 91.4,
                ..MemoryMetrics::default()
            }),
            filesystems: vec![
                FilesystemMetrics {
                    mount: "/".into(),
                    total_bytes: 1000,
                    used_bytes: 934,
                    free_bytes: 66,
                    used_percent: 93.4,
                    primary: true,
                    ..FilesystemMetrics::default()
                },
                FilesystemMetrics {
                    mount: "/data".into(),
                    total_bytes: 1000,
                    used_bytes: 900,
                    free_bytes: 100,
                    used_percent: 90.0,
                    ..FilesystemMetrics::default()
                },
                FilesystemMetrics {
                    mount: "/tmp".into(),
                    total_bytes: 1000,
                    used_bytes: 899,
                    free_bytes: 101,
                    used_percent: 89.9,
                    ..FilesystemMetrics::default()
                },
            ],
            ..NodeMetrics::default()
        })
    }
}

struct Calm;
impl MetricsCollector for Calm {
    fn collect(&self) -> io::Result<NodeMetrics> {
        Ok(NodeMetrics {
            hostname: "calm".into(),
            os: "linux".into(),
            arch: "x86_64".into(),
            agent_version: "test".into(),
            memory: Some(MemoryMetrics {
                total_bytes: 1000,
                used_bytes: 899,
                available_bytes: 101,
                used_percent: 89.9,
                ..MemoryMetrics::default()
            }),
            filesystems: vec![FilesystemMetrics {
                mount: "/".into(),
                used_percent: 12.0,
                primary: true,
                ..FilesystemMetrics::default()
            }],
            ..NodeMetrics::default()
        })
    }
}

async fn serve(router: axum::Router) -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (url, task)
}

fn client(url: &str) -> Client {
    Client::new(url, "owner-secret", Duration::from_secs(20)).unwrap()
}

fn address(url: &str) -> String {
    url.trim_start_matches("http://").to_owned()
}

fn node_row(listing: &Value, name: &str) -> Value {
    listing["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["node"]["name"] == name)
        .cloned()
        .unwrap_or_else(|| panic!("{name} missing from {listing}"))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn node_status_alerts_report_full_filesystems_and_memory_pressure() {
    let root = tempfile::tempdir().unwrap();
    let agent = Agent::new(Arc::new(Pressure));
    let (agent_url, agent_task) = serve(agent.router().unwrap()).await;
    let calm = Agent::new(Arc::new(Calm));
    let (calm_url, calm_task) = serve(calm.router().unwrap()).await;
    let hub = Hub::open(root.path().join("hub"), "owner-secret", "test").unwrap();
    for (name, url) in [("peer", address(&agent_url)), ("calm", address(&calm_url))] {
        hub.inventory
            .add(
                Node {
                    name: name.into(),
                    address: url,
                    ..Node::default()
                },
                false,
            )
            .unwrap();
    }
    let monitor = Arc::clone(&hub.node_monitor);
    let (url, hub_task) = serve(hub.router().unwrap()).await;
    tokio::task::spawn_blocking(move || monitor.poll_all())
        .await
        .unwrap();
    let status = {
        let url = url.clone();
        tokio::task::spawn_blocking(move || {
            client(&url)
                .call(GET_NODE, &json!({"name":"peer"}))
                .unwrap()
        })
        .await
        .unwrap()
    };
    assert_eq!(status["freshness"], "NODE_FRESHNESS_FRESH", "{status}");
    assert!(status["draining"].is_null(), "{status}");
    let alerts = status["alerts"].as_array().unwrap();
    assert_eq!(alerts.len(), 3, "{status}");
    assert_eq!(alerts[0]["kind"], "ALERT_KIND_DISK_PERCENT");
    assert_eq!(alerts[0]["subject"], "/");
    assert_eq!(alerts[0]["message"], "/ is 93% full");
    assert_eq!(alerts[0]["value"], 93.4);
    assert_eq!(alerts[0]["threshold"], 90.0);
    assert_eq!(alerts[1]["subject"], "/data");
    assert_eq!(alerts[1]["message"], "/data is 90% full");
    assert_eq!(alerts[2]["kind"], "ALERT_KIND_MEM_PRESSURE");
    assert!(alerts[2].get("subject").is_none(), "{status}");
    assert_eq!(alerts[2]["message"], "memory is 91% used");
    assert_eq!(alerts[2]["value"], 91.4);
    assert!(
        !alerts.iter().any(|alert| alert["subject"] == "/tmp"),
        "{status}"
    );
    let listing = {
        let url = url.clone();
        tokio::task::spawn_blocking(move || client(&url).call(LIST_NODES, &json!({})).unwrap())
            .await
            .unwrap()
    };
    assert_eq!(
        node_row(&listing, "peer")["alerts"],
        json!(alerts),
        "{listing}"
    );
    assert!(node_row(&listing, "calm")["alerts"].is_null(), "{listing}");
    hub_task.abort();
    agent_task.abort();
    calm_task.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn node_status_marks_a_node_while_the_hub_drains_it() {
    let root = tempfile::tempdir().unwrap();
    let jobs = Jobs::new(root.path().join("node-jobs"), 2)
        .unwrap()
        .with_plain_mode();
    let agent = Agent::new(Arc::new(Pressure)).with_jobs(jobs);
    let (agent_url, agent_task) = serve(agent.router().unwrap()).await;
    let hub = Hub::open(root.path().join("hub"), "owner-secret", "test").unwrap();
    hub.inventory
        .add(
            Node {
                name: "peer".into(),
                address: address(&agent_url),
                ..Node::default()
            },
            false,
        )
        .unwrap();
    let monitor = Arc::clone(&hub.node_monitor);
    let (url, hub_task) = serve(hub.router().unwrap()).await;
    tokio::task::spawn_blocking(move || monitor.poll_node("peer"))
        .await
        .unwrap();
    {
        let url = url.clone();
        tokio::task::spawn_blocking(move || {
            client(&url)
                .call(
                    SUBMIT,
                    &json!({"target":{"node":"peer"},"command":["/bin/sh","-c","sleep 2; printf done"],"timeout":"10s"}),
                )
                .unwrap();
        })
        .await
        .unwrap();
    }
    let before = {
        let url = url.clone();
        tokio::task::spawn_blocking(move || {
            client(&url)
                .call(GET_NODE, &json!({"name":"peer"}))
                .unwrap()
        })
        .await
        .unwrap()
    };
    assert!(before["draining"].is_null(), "{before}");
    let phases = {
        let url = url.clone();
        tokio::task::spawn_blocking(move || {
            let watcher = client(&url);
            let other = client(&url);
            let mut phases = Vec::<Value>::new();
            watcher
                .stream(
                    REMOVE_NODE,
                    &json!({"name":"peer","jobs":"REMOVE_JOBS_WAIT","waitTimeout":"10s"}),
                    |event| {
                        if event["phase"] == "REMOVE_PHASE_DRAINING"
                            && event["runningJobs"].as_u64().unwrap_or(0) > 0
                        {
                            let during = other.call(GET_NODE, &json!({"name":"peer"})).unwrap();
                            assert_eq!(during["draining"], json!(true), "{during}");
                            let listing = other.call(LIST_NODES, &json!({})).unwrap();
                            assert_eq!(
                                node_row(&listing, "peer")["draining"],
                                json!(true),
                                "{listing}"
                            );
                        }
                        phases.push(event);
                        Ok(())
                    },
                )
                .unwrap();
            phases
        })
        .await
        .unwrap()
    };
    assert!(
        phases
            .iter()
            .any(|phase| phase["phase"] == "REMOVE_PHASE_REMOVED"),
        "{phases:?}"
    );
    let gone = client(&url)
        .call(GET_NODE, &json!({"name":"peer"}))
        .unwrap_err();
    assert_eq!(gone.code, Code::NotFound);
    hub_task.abort();
    agent_task.abort();
}
