//! Lease admission against live node agents: GPU busy state, freshness and
//! tier decide what is granted, and queued waiters are granted in priority
//! order only once the resource is really free.
use crate::support::fleet;

use fleet::{agent, call, gpu, poll, serve, Metrics};
use protectorate_hub::Hub;
use protectorate_inventory::{Node, CARE, SHARED};
use protectorate_node_metrics::{ContainerEngine, ContainerMetrics, NodeMetrics};
use protectorate_wire::Code;
use serde_json::{json, Value};

const OWNER: &str = "owner-secret";

fn gpu_box() -> NodeMetrics {
    NodeMetrics {
        hostname: "gpu-box".into(),
        os: "linux".into(),
        arch: "amd64".into(),
        gpus: vec![
            gpu(0, 1, 24, false),
            gpu(1, 22, 24, false),
            gpu(2, 15, 16, true),
        ],
        engine: Some(ContainerEngine {
            kind: "docker".into(),
            containers: vec![
                ContainerMetrics {
                    name: "ollama".into(),
                    state: "running".into(),
                    ..ContainerMetrics::default()
                },
                ContainerMetrics {
                    name: "old".into(),
                    state: "exited".into(),
                    ..ContainerMetrics::default()
                },
            ],
            ..ContainerEngine::default()
        }),
        ..NodeMetrics::default()
    }
}

fn holders(list: &Value) -> Vec<(String, bool)> {
    list["leases"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|lease| {
            (
                lease["holder"].as_str().unwrap().to_owned(),
                lease["state"] == "LEASE_STATE_QUEUED",
            )
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn gpu_leases_follow_live_busy_state_freshness_and_tier() {
    let dir = tempfile::tempdir().unwrap();
    let metrics = Metrics::new(gpu_box());
    let care = Metrics::new(NodeMetrics {
        hostname: "care-box".into(),
        ..NodeMetrics::default()
    });
    let (gpu_addr, gpu_task) = agent(&metrics, None).await;
    let (care_addr, care_task) = agent(&care, None).await;
    let hub = Hub::open(dir.path(), OWNER, "test").unwrap();
    for (name, address, tier) in [
        ("gpu-box", &gpu_addr, SHARED),
        ("care-box", &care_addr, CARE),
    ] {
        hub.inventory
            .add(
                Node {
                    name: name.into(),
                    address: address.clone(),
                    tier: tier.into(),
                    ..Node::default()
                },
                false,
            )
            .unwrap();
    }
    let monitor = std::sync::Arc::clone(&hub.node_monitor);
    let (addr, server) = serve(hub.router().unwrap()).await;
    let acquire = |resource: &str, holder: &str, extra: Value| {
        let mut input = json!({"resource":resource, "holder":holder, "ttl":"60s"});
        for (key, value) in extra.as_object().unwrap() {
            input[key] = value.clone();
        }
        call(&addr, OWNER, "LeaseService/AcquireLease", input)
    };
    let refused = |result: Result<Value, protectorate_wire::RpcError>, why: &str| {
        let error = result.expect_err(why);
        assert_eq!(error.code, Code::FailedPrecondition, "{error}");
        assert!(error.message.contains(why), "{error}");
    };

    refused(
        acquire("gpu-box:gpu0", "early", json!({})).await,
        "node gpu-box has no fresh successful metrics poll",
    );
    poll(&monitor, "gpu-box").await;
    poll(&monitor, "care-box").await;
    refused(
        acquire("care-box", "anyone", json!({})).await,
        "node care-box is not available for leases",
    );
    refused(
        acquire("gpu-box:gpu1", "trainer", json!({})).await,
        "node gpu-box gpu 1 is busy: 92% of its VRAM is in use (running containers: ollama)",
    );
    refused(
        acquire("gpu-box", "trainer", json!({})).await,
        "node gpu-box gpu 1 is busy",
    );
    refused(
        acquire("gpu-box:gpu2", "trainer", json!({})).await,
        "node gpu-box gpu 2 is busy",
    );

    // The busy discrete GPU frees up; the busy integrated one never blocks
    // a whole-node lease.
    metrics.edit(|m| m.gpus[1] = gpu(1, 2, 24, false));
    poll(&monitor, "gpu-box").await;
    let whole = acquire("gpu-box", "desk", json!({})).await.unwrap();
    assert_eq!(whole["resource"], "gpu-box");
    call(
        &addr,
        OWNER,
        "LeaseService/ReleaseLease",
        json!({"id":whole["id"]}),
    )
    .await
    .unwrap();

    let held = acquire("gpu-box:gpu0", "trainer", json!({})).await.unwrap();
    assert_eq!(held["resource"], "gpu-box:gpu0");
    for (holder, priority) in [("low", 0), ("high", 5)] {
        let ticket = acquire(
            "gpu-box:gpu0",
            holder,
            json!({"wait":true, "priority":priority}),
        )
        .await
        .unwrap();
        assert_eq!(ticket["state"], "LEASE_STATE_QUEUED", "{ticket}");
    }
    let other = acquire("gpu-box:gpu1", "other", json!({})).await.unwrap();
    let list = |addr: String| async move {
        call(&addr, OWNER, "LeaseService/ListLeases", json!({}))
            .await
            .unwrap()
    };
    let owned = |name: &str| (name.to_owned(), false);
    let queued = |name: &str| (name.to_owned(), true);
    assert_eq!(
        holders(&list(addr.clone()).await),
        [
            owned("trainer"),
            owned("other"),
            queued("high"),
            queued("low")
        ]
    );

    // The trainer's process still holds the GPU memory when it releases the
    // lease: nobody is granted a GPU the node reports busy.
    metrics.edit(|m| m.gpus[0] = gpu(0, 23, 24, false));
    poll(&monitor, "gpu-box").await;
    call(
        &addr,
        OWNER,
        "LeaseService/ReleaseLease",
        json!({"id":held["id"]}),
    )
    .await
    .unwrap();
    assert_eq!(
        holders(&list(addr.clone()).await),
        [owned("other"), queued("high"), queued("low")]
    );
    metrics.edit(|m| m.gpus[0] = gpu(0, 1, 24, false));
    poll(&monitor, "gpu-box").await;
    let granted = list(addr.clone()).await;
    assert_eq!(
        holders(&granted),
        [owned("other"), owned("high"), queued("low")]
    );
    let high = granted["leases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|lease| lease["holder"] == "high")
        .unwrap()
        .clone();
    assert_eq!(high["resource"], "gpu-box:gpu0");

    // A node whose last poll failed grants nothing until it answers again.
    metrics.set(None);
    poll(&monitor, "gpu-box").await;
    let node = call(
        &addr,
        OWNER,
        "NodeService/GetNode",
        json!({"name":"gpu-box"}),
    )
    .await
    .unwrap();
    assert_eq!(node["freshness"], "NODE_FRESHNESS_STALE", "{node}");
    call(
        &addr,
        OWNER,
        "LeaseService/ReleaseLease",
        json!({"id":high["id"]}),
    )
    .await
    .unwrap();
    refused(
        acquire("gpu-box:gpu1", "late", json!({})).await,
        "node gpu-box has no fresh successful metrics poll",
    );
    assert_eq!(
        holders(&list(addr.clone()).await),
        [owned("other"), queued("low")]
    );
    metrics.set(Some(gpu_box()));
    metrics.edit(|m| m.gpus[1] = gpu(1, 2, 24, false));
    poll(&monitor, "gpu-box").await;
    assert_eq!(
        holders(&list(addr.clone()).await),
        [owned("other"), owned("low")]
    );
    let busy = acquire("gpu-box", "desk", json!({})).await.unwrap_err();
    assert_eq!(busy.code, Code::FailedPrecondition, "{busy}");
    assert!(busy.message.contains("held by"), "{busy}");
    call(
        &addr,
        OWNER,
        "LeaseService/ReleaseLease",
        json!({"id":other["id"]}),
    )
    .await
    .unwrap();
    server.abort();
    gpu_task.abort();
    care_task.abort();
}
