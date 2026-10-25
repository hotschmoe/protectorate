//! Node health as the owner sees it: poll events, resource alerts, the
//! down transition and the drain mark while a node is being removed.
use crate::support::fleet;

use fleet::{agent, call, client, events_until, poll, serve, subscribe, Metrics};
use protectorate_hub::Hub;
use protectorate_inventory::{Node, CARE};
use protectorate_node_metrics::{FilesystemMetrics, MemoryMetrics, NodeMetrics};
use protectorate_node_monitor::{Monitor, Timing};
use serde_json::{json, Value};
use std::{sync::Arc, time::Duration};

const OWNER: &str = "owner-secret";
const STALE_AFTER: Duration = Duration::from_secs(2);

fn pressured() -> NodeMetrics {
    let disk = |mount: &str, used_percent: f64| FilesystemMetrics {
        mount: mount.into(),
        device: "/dev/sda1".into(),
        fstype: "ext4".into(),
        total_bytes: 100 << 30,
        used_percent,
        ..FilesystemMetrics::default()
    };
    NodeMetrics {
        hostname: "peer".into(),
        os: "linux".into(),
        arch: "amd64".into(),
        memory: Some(MemoryMetrics {
            total_bytes: 16 << 30,
            used_percent: 95.0,
            ..MemoryMetrics::default()
        }),
        filesystems: vec![disk("/", 97.0), disk("/data", 40.0)],
        ..NodeMetrics::default()
    }
}

fn alerts(status: &Value) -> Vec<(String, String)> {
    status["alerts"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|alert| {
            (
                alert["kind"].as_str().unwrap().to_owned(),
                alert["message"].as_str().unwrap_or_default().to_owned(),
            )
        })
        .collect()
}

fn kinds(events: &[Value]) -> Vec<&str> {
    events.iter().map(|e| e["kind"].as_str().unwrap()).collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn poll_events_alerts_down_transitions_and_drain_mark() {
    let root = tempfile::tempdir().unwrap();
    let peer = Metrics::new(pressured());
    let ghost = Metrics::default();
    let (peer_addr, peer_task) = agent(&peer, Some(&root.path().join("node-jobs"))).await;
    let (ghost_addr, ghost_task) = agent(&ghost, None).await;
    let hub = Hub::open(root.path().join("hub"), OWNER, "test").unwrap();
    for (name, address) in [("peer", &peer_addr), ("ghost", &ghost_addr)] {
        hub.inventory
            .add(
                Node {
                    name: name.into(),
                    address: address.clone(),
                    tier: CARE.into(),
                    ..Node::default()
                },
                false,
            )
            .unwrap();
    }
    let monitor = Arc::new(
        Monitor::new(
            Arc::clone(&hub.inventory),
            hub.hostname.clone(),
            root.path().join("hub"),
            "v-test".into(),
        )
        .with_timing(Timing {
            stale_after: STALE_AFTER,
            fetch_timeout: Duration::from_secs(10),
        }),
    );
    let hub = hub.with_node_monitor(Arc::clone(&monitor)).unwrap();
    let (addr, server) = serve(hub.router().unwrap()).await;
    let events = subscribe(&addr);

    poll(&monitor, "peer").await;
    let metrics = events_until(&events, "peer", "node.metrics");
    let payload = &metrics.last().unwrap()["payload"];
    assert_eq!(payload["freshness"], "fresh", "{payload}");
    assert_eq!(
        payload["alerts"],
        json!(["disk_percent", "mem_pressure"]),
        "{payload}"
    );
    assert_eq!(payload["worst_disk_mount"], "/", "{payload}");
    assert_eq!(payload["mem_percent"], 95.0, "{payload}");
    let status = call(&addr, OWNER, "NodeService/GetNode", json!({"name":"peer"}))
        .await
        .unwrap();
    let shown = alerts(&status);
    assert!(
        shown.contains(&("ALERT_KIND_DISK_PERCENT".into(), "/ is 97% full".into())),
        "{status}"
    );
    assert!(
        shown.contains(&(
            "ALERT_KIND_MEM_PRESSURE".into(),
            "memory is 95% used".into()
        )),
        "{status}"
    );
    assert!(!status.to_string().contains("/data is"), "{status}");
    assert!(status["draining"].is_null(), "{status}");

    // A node that never answered is down at once and says so once; one that
    // answered recently is only stale until its last good poll ages out.
    poll(&monitor, "ghost").await;
    poll(&monitor, "ghost").await;
    ghost.set(Some(NodeMetrics {
        hostname: "ghost".into(),
        ..NodeMetrics::default()
    }));
    poll(&monitor, "ghost").await;
    ghost.set(None);
    poll(&monitor, "ghost").await;
    let stale = call(&addr, OWNER, "NodeService/GetNode", json!({"name":"ghost"}))
        .await
        .unwrap();
    assert_ne!(stale["freshness"], "NODE_FRESHNESS_FRESH", "{stale}");
    tokio::time::sleep(STALE_AFTER + Duration::from_millis(200)).await;
    poll(&monitor, "ghost").await;
    poll(&monitor, "ghost").await;
    peer.edit(|m| m.memory.as_mut().unwrap().used_percent = 30.0);
    poll(&monitor, "peer").await;
    let seen = events_until(&events, "ghost", "node.unreachable");
    assert_eq!(kinds(&seen), ["node.unreachable"]);
    assert!(
        seen[0]["payload"]["error"]
            .as_str()
            .unwrap()
            .contains("probe failed"),
        "{seen:?}"
    );
    let seen = events_until(&events, "ghost", "node.unreachable");
    assert_eq!(kinds(&seen), ["node.metrics", "node.unreachable"]);
    let calmer = events_until(&events, "peer", "node.metrics");
    assert_eq!(
        calmer.last().unwrap()["payload"]["alerts"],
        json!(["disk_percent"])
    );
    let down = call(&addr, OWNER, "NodeService/GetNode", json!({"name":"ghost"}))
        .await
        .unwrap();
    assert_eq!(down["freshness"], "NODE_FRESHNESS_UNREACHABLE", "{down}");
    assert_eq!(alerts(&down)[0].0, "ALERT_KIND_UNREACHABLE", "{down}");

    let gate = root.path().join("finish");
    let job = call(
        &addr,
        OWNER,
        "JobService/SubmitJob",
        json!({"target":{"node":"peer"},"timeout":"120s","command":[
            "/bin/sh","-c",
            format!("while [ ! -e '{}' ]; do sleep 0.05; done", gate.display()),
        ]}),
    )
    .await
    .unwrap();
    let watched = client(&addr, OWNER);
    let job_id = job["id"].as_str().unwrap().to_owned();
    let other = client(&addr, OWNER);
    let (phases, during) = tokio::task::spawn_blocking(move || {
        let mut phases = Vec::new();
        let mut during = Vec::new();
        loop {
            let job = watched
                .call("/protectorate.v1.JobService/GetJob", &json!({"id":job_id}))
                .unwrap();
            if job["state"] == "JOB_STATE_RUNNING" {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        watched
            .stream(
                "/protectorate.v1.NodeService/RemoveNode",
                &json!({"name":"peer","jobs":"REMOVE_JOBS_WAIT","waitTimeout":"120s"}),
                |event| {
                    if event["phase"] == "REMOVE_PHASE_DRAINING" && during.is_empty() {
                        during.push(
                            other
                                .call(
                                    "/protectorate.v1.NodeService/GetNode",
                                    &json!({"name":"peer"}),
                                )
                                .unwrap(),
                        );
                        during.push(
                            other
                                .call("/protectorate.v1.NodeService/ListNodes", &json!({}))
                                .unwrap(),
                        );
                        std::fs::write(&gate, "").unwrap();
                    }
                    phases.push(event["phase"].as_str().unwrap_or_default().to_owned());
                    Ok(())
                },
            )
            .unwrap();
        (phases, during)
    })
    .await
    .unwrap();
    assert_eq!(during[0]["draining"], true, "{}", during[0]);
    let row = during[1]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["node"]["name"] == "peer")
        .unwrap();
    assert_eq!(row["draining"], true, "{row}");
    let ghost_row = during[1]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["node"]["name"] == "ghost")
        .unwrap();
    assert!(ghost_row["draining"].is_null(), "{ghost_row}");
    assert_eq!(phases.last().unwrap(), "REMOVE_PHASE_REMOVED", "{phases:?}");
    assert_eq!(
        kinds(&events_until(&events, "peer", "node.removed")).last(),
        Some(&"node.removed")
    );
    let gone = call(&addr, OWNER, "NodeService/GetNode", json!({"name":"peer"}))
        .await
        .unwrap_err();
    assert_eq!(gone.code, protectorate_wire::Code::NotFound, "{gone}");
    server.abort();
    peer_task.abort();
    ghost_task.abort();
}
