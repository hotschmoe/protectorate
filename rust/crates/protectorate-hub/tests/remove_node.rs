use protectorate_connect_client::Client;
use protectorate_hub::Hub;
use protectorate_inventory::{Node, CARE};
use protectorate_lease::{parse_resource, Chooser, Error as LeaseError};
use protectorate_node::{Agent, Jobs, MetricsCollector};
use protectorate_node_metrics::NodeMetrics;
use protectorate_wire::Code;
use serde_json::{json, Value};
use std::{io, sync::Arc, time::Duration};
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
fn client(url: &str) -> Client {
    Client::new(url, "owner-secret", Duration::from_secs(20)).unwrap()
}
const SUBMIT: &str = "/protectorate.v1.JobService/SubmitJob";
const GET: &str = "/protectorate.v1.JobService/GetJob";
const REMOVE: &str = "/protectorate.v1.NodeService/RemoveNode";
const ACQUIRE: &str = "/protectorate.v1.LeaseService/AcquireLease";
const LIST: &str = "/protectorate.v1.LeaseService/ListLeases";
const GET_NODE: &str = "/protectorate.v1.NodeService/GetNode";

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn wait_drain_blocks_new_jobs_and_leases_then_releases_old_lease() {
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
    // The custom chooser permits a lease on this care node for the drain test.
    let chooser: Chooser = Arc::new(|request, blocked| {
        let resource = parse_resource(request)?;
        if blocked(&resource).is_some() {
            return Err(LeaseError::Busy("held".into()));
        }
        Ok(resource)
    });
    let hub = hub.with_lease_chooser(chooser).unwrap();
    let (url, hub_task) = serve(hub.router().unwrap()).await;
    let url2 = url.clone();
    let job = tokio::task::spawn_blocking(move || {
        client(&url2).call(SUBMIT,&json!({
        "target":{"node":"peer"},"command":["/bin/sh","-c","sleep 2; printf done"],"timeout":"120s"
    }))
    })
    .await
    .unwrap()
    .unwrap();
    let id = job["id"].as_str().unwrap().to_owned();
    let url2 = url.clone();
    let lease = tokio::task::spawn_blocking(move || {
        client(&url2).call(
            ACQUIRE,
            &json!({
                "resource":"peer","holder":"owner","ttl":"10s"
            }),
        )
    })
    .await
    .unwrap()
    .unwrap();
    let lease_id = lease["id"].as_str().unwrap().to_owned();
    let url2 = url.clone();
    let queued = tokio::task::spawn_blocking(move || {
        client(&url2).call(
            ACQUIRE,
            &json!({"resource":"peer","holder":"queued","ttl":"10s","wait":true}),
        )
    })
    .await
    .unwrap()
    .unwrap();
    let ticket_id = queued["id"].as_str().unwrap().to_owned();
    let url2 = url.clone();
    let id2 = id.clone();
    let phases=tokio::task::spawn_blocking(move || {
        let watcher=client(&url2);
        let other=client(&url2);
        let mut phases=Vec::<Value>::new();
        watcher.stream(REMOVE,&json!({"name":"peer","jobs":"REMOVE_JOBS_WAIT","waitTimeout":"120s","uninstall":true}),|event|{
            if event["phase"]=="REMOVE_PHASE_DRAINING" && event["runningJobs"].as_u64().unwrap_or(0)>0 {
                let denied=other.call(SUBMIT,&json!({"target":{"node":"peer"},"command":["true"],"timeout":"5s"})).unwrap_err();
                assert_eq!(denied.code,Code::FailedPrecondition,"{denied}");
                let denied=other.call(ACQUIRE,&json!({"resource":"peer","holder":"later","ttl":"10s"})).unwrap_err();
                assert_eq!(denied.code,Code::FailedPrecondition,"{denied}");
            }
            phases.push(event);
            Ok(())
        }).unwrap();
        let final_job=other.call(GET,&json!({"id":id2})).unwrap();
        (phases,final_job,other.call(LIST,&json!({})).unwrap(),other.call(GET_NODE,&json!({"name":"peer"})).unwrap_err())
    }).await.unwrap();
    assert_eq!(phases.1["state"], "JOB_STATE_SUCCEEDED", "{}", phases.1);
    assert!(phases
        .0
        .iter()
        .any(|e| e["phase"] == "REMOVE_PHASE_REMOVED"));
    assert!(phases
        .0
        .iter()
        .any(|e| e["phase"] == "REMOVE_PHASE_REVOKING"
            && e["message"]
                .as_str()
                .unwrap()
                .contains("the node certificate is no longer trusted")));
    assert!(!phases.2.to_string().contains(&lease_id));
    assert!(!phases.2.to_string().contains(&ticket_id));
    assert_eq!(phases.3.code, Code::NotFound);
    hub_task.abort();
    node_task.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cancel_mode_stops_running_job_before_forgetting_node() {
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
    let job=tokio::task::spawn_blocking(move || client(&url2).call(SUBMIT,&json!({"target":{"node":"peer"},"command":["/bin/sh","-c","sleep 60"],"timeout":"120s"}))).await.unwrap().unwrap();
    let id = job["id"].as_str().unwrap().to_owned();
    let url2 = url.clone();
    let (phases, final_job) = tokio::task::spawn_blocking(move || {
        let client = client(&url2);
        let error = client
            .stream(REMOVE, &json!({"name":"peer"}), |_| Ok(()))
            .unwrap_err();
        assert_eq!(error.code, Code::FailedPrecondition);
        let mut phases = Vec::new();
        client
            .stream(
                REMOVE,
                &json!({"name":"peer","jobs":"REMOVE_JOBS_CANCEL","waitTimeout":"120s"}),
                |event| {
                    phases.push(event);
                    Ok(())
                },
            )
            .unwrap();
        let final_job = client.call(GET, &json!({"id":id})).unwrap();
        (phases, final_job)
    })
    .await
    .unwrap();
    assert_eq!(final_job["state"], "JOB_STATE_CANCELED", "{final_job}");
    assert!(phases.iter().any(|e| e["phase"] == "REMOVE_PHASE_REMOVED"));
    hub_task.abort();
    node_task.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[cfg(target_os = "linux")]
async fn fresh_node_receives_authenticated_uninstall_before_removal() {
    use protectorate_node::UninstallPlan;
    use std::sync::atomic::{AtomicBool, Ordering};
    let root = tempfile::tempdir().unwrap();
    let plan = UninstallPlan::new(
        root.path().join("node-state"),
        root.path().join("user"),
        root.path().join("node-bin"),
        true,
    )
    .unwrap();
    let stopped = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&stopped);
    let node = Agent::new(Arc::new(Metrics)).with_uninstall(
        plan,
        "/bin/true",
        Arc::new(move || {
            flag.store(true, Ordering::SeqCst);
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
    tokio::task::spawn_blocking(move || monitor.poll_node("peer"))
        .await
        .unwrap();
    assert!(hub
        .node_monitor
        .is_fresh("peer", node_url.trim_start_matches("http://")));
    let (url, hub_task) = serve(hub.router().unwrap()).await;
    let phases = tokio::task::spawn_blocking(move || {
        let mut phases = Vec::new();
        client(&url)
            .stream(REMOVE, &json!({"name":"peer","uninstall":true}), |event| {
                phases.push(event);
                Ok(())
            })
            .unwrap();
        phases
    })
    .await
    .unwrap();
    let uninstall = phases
        .iter()
        .find(|event| event["phase"] == "REMOVE_PHASE_UNINSTALLING")
        .unwrap();
    assert!(uninstall["message"]
        .as_str()
        .unwrap()
        .contains("the agent unregisters its startup and stops"));
    assert!(phases
        .iter()
        .any(|event| event["phase"] == "REMOVE_PHASE_REMOVED"));
    tokio::time::timeout(Duration::from_secs(60), async {
        while !stopped.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    hub_task.abort();
    node_task.abort();
}
