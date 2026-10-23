//! The node inventory over the owner API: add, update and re-tier survive a
//! hub restart, and a care node is never handed to an agent's job or lease.
#[path = "support/capability.rs"]
mod capability;
#[path = "support/fleet.rs"]
mod fleet;

use fleet::{agent, call, poll, serve, Metrics};
use protectorate_hub::Hub;
use protectorate_node_metrics::NodeMetrics;
use protectorate_wire::{Code, RpcError};
use serde_json::{json, Value};
use std::sync::Arc;

const OWNER: &str = "owner-secret";

fn code(result: Result<Value, RpcError>) -> (Code, String) {
    let error = result.expect_err("call should fail");
    (error.code, error.message)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn inventory_edits_persist_and_care_nodes_stay_out_of_agent_work() {
    let root = tempfile::tempdir().unwrap();
    let metrics = Metrics::new(NodeMetrics {
        hostname: "compute".into(),
        os: "linux".into(),
        arch: "amd64".into(),
        ..NodeMetrics::default()
    });
    let (node_addr, node_task) = agent(&metrics, Some(&root.path().join("node-jobs"))).await;
    let hub = Hub::open(root.path().join("hub"), OWNER, "test").unwrap();
    let (_, worker) = capability::mint(&hub, "worker", &["read", "job_*", "lease_*"]);
    let monitor = Arc::clone(&hub.node_monitor);
    let (addr, server) = serve(hub.router().unwrap()).await;
    let owner = |procedure: &'static str, input: Value| call(&addr, OWNER, procedure, input);
    let agent_call = |procedure: &'static str, input: Value| call(&addr, &worker, procedure, input);

    let added = json!({"node":{"name":" Compute ","address":node_addr,"tags":["GPU","gpu","linux"],"notes":" first ","sshAlias":"compute-ssh"}});
    let created = owner("NodeService/AddNode", added.clone()).await.unwrap();
    assert_eq!(created["name"], "compute");
    assert_eq!(created["tags"], json!(["gpu", "linux"]));
    assert_eq!(created["tier"], "shared");
    assert_eq!(created["notes"], "first");
    assert_eq!(
        code(owner("NodeService/AddNode", added).await).0,
        Code::AlreadyExists
    );
    assert_eq!(
        code(
            agent_call(
                "NodeService/AddNode",
                json!({"node":{"name":"rogue","address":"127.0.0.1:1"}})
            )
            .await
        )
        .0,
        Code::PermissionDenied
    );
    assert_eq!(
        code(
            owner(
                "NodeService/AddNode",
                json!({"node":{"name":"bad","address":"https://evil/path"}})
            )
            .await
        )
        .0,
        Code::InvalidArgument
    );
    let updated = owner(
        "NodeService/AddNode",
        json!({"update":true,"node":{"name":"compute","address":"","tags":["new"]}}),
    )
    .await
    .unwrap();
    assert_eq!(updated["address"], node_addr);
    assert_eq!(updated["notes"], "first");
    assert_eq!(updated["sshAlias"], "compute-ssh");
    assert_eq!(updated["tags"], json!(["new"]));

    poll(&monitor, "compute").await;
    let selector = json!({"target":{"selector":{}},"command":["true"]});
    let placed = agent_call("JobService/SubmitJob", selector.clone())
        .await
        .unwrap();
    assert_eq!(placed["node"], "compute", "{placed}");
    assert_eq!(placed["mode"], "JOB_MODE_SANDBOXED", "{placed}");
    let lease = agent_call(
        "LeaseService/AcquireLease",
        json!({"resource":"compute","holder":"worker","ttl":"30s"}),
    )
    .await
    .unwrap();
    owner("LeaseService/ReleaseLease", json!({"id":lease["id"]}))
        .await
        .unwrap();

    let care = owner(
        "NodeService/SetNodeTier",
        json!({"name":"COMPUTE","tier":"CARE"}),
    )
    .await
    .unwrap();
    assert_eq!(care["tier"], "care");
    poll(&monitor, "compute").await;
    let refusals = [
        (
            agent_call("JobService/SubmitJob", selector.clone()).await,
            Code::FailedPrecondition,
            "no fresh shared node matches selector",
        ),
        (
            agent_call(
                "JobService/SubmitJob",
                json!({"target":{"node":"compute"},"command":["true"]}),
            )
            .await,
            Code::PermissionDenied,
            "tier care node requires an owner or node admin grant",
        ),
        (
            agent_call(
                "LeaseService/AcquireLease",
                json!({"resource":"compute","holder":"worker","ttl":"30s"}),
            )
            .await,
            Code::FailedPrecondition,
            "node compute is not available for leases",
        ),
    ];
    for (result, want, message) in refusals {
        let (got, text) = code(result);
        assert_eq!(got, want, "{text}");
        assert!(text.contains(message), "{text}");
    }
    let maintained = owner(
        "JobService/SubmitJob",
        json!({"target":{"node":"compute"},"command":["true"]}),
    )
    .await
    .unwrap();
    assert_eq!(maintained["mode"], "JOB_MODE_PLAIN", "{maintained}");

    owner(
        "NodeService/AddNode",
        json!({"node":{"name":"pending","address":"127.0.0.1:9","invited":true}}),
    )
    .await
    .unwrap();
    assert_eq!(
        code(
            owner(
                "NodeService/SetNodeTier",
                json!({"name":"pending","tier":"care"})
            )
            .await
        )
        .0,
        Code::FailedPrecondition
    );
    server.abort();

    let hub = Hub::open(root.path().join("hub"), OWNER, "test").unwrap();
    let (_, worker) = capability::mint(&hub, "worker", &["read", "job_*"]);
    let monitor = Arc::clone(&hub.node_monitor);
    let (addr, server) = serve(hub.router().unwrap()).await;
    let listed = call(&addr, OWNER, "NodeService/ListNodes", json!({}))
        .await
        .unwrap();
    let row = |name: &str| {
        listed["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["node"]["name"] == name)
            .unwrap_or_else(|| panic!("no {name}: {listed}"))
            .clone()
    };
    let compute = row("compute");
    assert_eq!(compute["node"]["tier"], "care", "{compute}");
    assert_eq!(compute["node"]["tags"], json!(["new"]), "{compute}");
    assert_eq!(compute["node"]["notes"], "first", "{compute}");
    assert_eq!(row("pending")["freshness"], "NODE_FRESHNESS_INVITED");
    call(
        &addr,
        OWNER,
        "NodeService/SetNodeTier",
        json!({"name":"compute","tier":"shared"}),
    )
    .await
    .unwrap();
    poll(&monitor, "compute").await;
    let placed = call(&addr, &worker, "JobService/SubmitJob", selector)
        .await
        .unwrap();
    assert_eq!(placed["node"], "compute", "{placed}");
    server.abort();
    node_task.abort();
}
